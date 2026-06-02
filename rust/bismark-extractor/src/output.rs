//! Per-(mode-key) split-file map + splitting-report writer.
//!
//! Phase B opened 12 strand×context files eagerly at [`OutputFileMap::new`]
//! time. Phase E generalises this to all 5 non-`MbiasOnly` modes: the
//! `(key, filename)` list comes from [`crate::output_mode::mode_keys`], and
//! each file may be wrapped in a parallel-gzip `gzp::par::compress::ParCompress`
//! writer when `--gzip` is set. `MbiasOnly` skips eager-open entirely (the map is
//! empty; `route_call` short-circuits before any `write_call` ever runs).
//!
//! The map's value type changed from Phase B's `BufWriter<File>` to
//! `BufWriter<Box<dyn Write + Send>>` to accommodate the plain-vs-gzip
//! dispatch through a single code-path. The `+ Send` bound is
//! forward-looking for Phase F (per-worker `OutputFileMap`s moved between
//! threads at join time).

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use bismark_io::BismarkStrand;

use crate::call::MethCall;
use crate::cli::{OutputMode, ResolvedConfig};
use crate::error::BismarkExtractorError;
use crate::output_mode::{OutputKey, mode_keys, route_to_key, write_yacht_row};

/// Bismark version string. Hardcoded to lock byte-identity with Perl's
/// `$version` variable. Update in lockstep with Perl `bismark_methylation_extractor`
/// at release time.
pub const BISMARK_VERSION: &str = "v0.25.1";

/// The literal header line Perl writes as the first line of every split
/// file (when `!--no_header && !--mbias_only`). Verified at Perl lines
/// 5159, 5182, 5205, 5228, 5429, 5452, 5475, 5498, etc.
pub const SPLIT_FILE_HEADER: &str = "Bismark methylation extractor version v0.25.1\n";

/// Per-call type-erased boxed writer (plain `File` or a gzp
/// `ParCompress<Gzip>` over `File`) wrapped in an 8-KiB `BufWriter`.
/// Phase F may revisit static-dispatch
/// once profiling under multicore is available (Phase E plan §9.2 #2).
type BoxedWriter = BufWriter<Box<dyn Write + Send>>;

/// Result of [`OutputFileMap::finalize_with_empty_sweep`]. Lists every
/// file the sweep retained on disk (`kept`) vs unlinked (`swept`) as
/// absolute paths. **Phase G (rev 1 I10)**: the kept list feeds the
/// `bismark2bedGraph` subprocess as its positional argv tail; the swept
/// list is used by Phase H's harness to assert the file-set-match
/// contract vs Perl's `was empty -> deleted` sweep.
///
/// `kept` is sorted lexicographically so the argv ordering passed to
/// `bismark2bedGraph` is deterministic across runs (rev 1 I7 — the
/// underlying HashMap iteration order is not).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FinalizationReport {
    /// Absolute paths of files retained (records_written > 0).
    pub kept: Vec<PathBuf>,
    /// Absolute paths of files unlinked (records_written == 0).
    pub swept: Vec<PathBuf>,
}

/// Per-handle entry in [`OutputFileMap::files`].
///
/// Phase B opened files eagerly + carried `(path, writer)` as a tuple.
/// **Phase C.2 (#865)** adds `records_written: u64` so
/// [`OutputFileMap::finalize_with_empty_sweep`] can unlink per-strand
/// files that received only the version-banner header line (typical for
/// CTOT/CTOB strands in a directional library).
///
/// **Constraint**: `records_written` is bumped iff a call row is written
/// (via [`OutputFileMap::write_call`]'s successful exit path). Any future
/// writer that adds non-call non-header bytes to the file MUST also bump
/// this counter, or the empty-sweep will incorrectly classify the file
/// as empty and unlink it. SPEC §8.3 documents this invariant.
struct OutputFileEntry {
    path: PathBuf,
    writer: BoxedWriter,
    records_written: u64,
}

/// Eagerly-opened per-(mode-key) split files.
///
/// Rev 1 layout (Phase B): one map keyed by `OutputKey` storing both the
/// path (for cleanup) and an 8-KiB `BufWriter<File>`. Phase E widens the
/// value type's inner writer to `Box<dyn Write + Send>` so the same
/// `write_call` body handles plain and gzipped output through one code-path.
/// **Phase C.2 (#865)**: value type changed from `(PathBuf, BoxedWriter)`
/// to [`OutputFileEntry`] (adds `records_written` for the empty-file sweep).
pub struct OutputFileMap {
    files: HashMap<OutputKey, OutputFileEntry>,
    /// Resolved output mode — used by `write_call` to pick the per-mode
    /// key from `(context, strand)` and to dispatch yacht's 8-col row
    /// format.
    mode: OutputMode,
    /// `OutputKey` → **creation rank** = its index in the [`mode_keys`] list
    /// (which is returned in Perl's file creation order). The bedGraph tee
    /// passes this rank to `Aggregator::add_ranked` so chromosome ownership
    /// resolves to the lowest-rank (first-created) file — matching Perl's
    /// first-in-creation-order ownership. Precomputed here so the hot-path
    /// tee does a single `HashMap` lookup, never re-deriving the order.
    ranks: HashMap<OutputKey, u32>,
}

impl OutputFileMap {
    /// Eagerly open all per-mode split files in `output_dir`.
    ///
    /// Writes the version header line to each file unless `no_header == true`.
    /// Creates `output_dir` via `create_dir_all` if missing (matches Perl
    /// `make_path` behaviour).
    ///
    /// When `mode == MbiasOnly` returns an empty map (Perl `:5148-5151
    /// unless($mbias_only)` skip-eager-open). `flush_all` and `cleanup_all`
    /// remain valid no-ops on the empty map.
    ///
    /// When `gzip == true` every writer is wrapped in a gzp parallel-gzip
    /// `ParCompress<Gzip>`; filenames already carry the `.gz` suffix per
    /// [`mode_keys`].
    pub fn new(
        output_dir: &Path,
        input_basename: &str,
        no_header: bool,
        mode: OutputMode,
        gzip: bool,
    ) -> Result<Self, std::io::Error> {
        std::fs::create_dir_all(output_dir)?;

        let keys = mode_keys(mode, input_basename, gzip);
        let mut files: HashMap<OutputKey, OutputFileEntry> = HashMap::with_capacity(keys.len());
        // Creation rank = the key's index in the mode_keys list (Perl's file
        // creation order). Captured here so the bedGraph tee in `write_call`
        // can pass it to `Aggregator::add_ranked` without re-deriving the order
        // per call.
        let mut ranks: HashMap<OutputKey, u32> = HashMap::with_capacity(keys.len());

        for (rank, (key, filename)) in keys.into_iter().enumerate() {
            let path = output_dir.join(filename);
            let mut writer = open_writer(&path, gzip)?;
            if !no_header {
                writer.write_all(SPLIT_FILE_HEADER.as_bytes())?;
            }
            ranks.insert(key, rank as u32);
            files.insert(
                key,
                OutputFileEntry {
                    path,
                    writer,
                    records_written: 0,
                },
            );
        }

        Ok(OutputFileMap { files, mode, ranks })
    }

    /// Append a `MethCall` line to the appropriate split file.
    ///
    /// `record_name` is the raw QNAME bytes from the BAM (used verbatim in
    /// the output line — Bismark QNAMEs are ASCII in practice).
    ///
    /// `yacht_col6` / `yacht_col7` carry the strand-conditional col-6 /
    /// col-7 values for yacht mode (forward-class emits `(start, end)`;
    /// reverse-class emits `(end, start)`). Non-yacht modes ignore them.
    ///
    /// # Output format (5-col, all non-Yacht modes)
    ///
    /// Tab-separated row matching Perl 2911-2961:
    /// ```text
    /// read_id  meth_char  chr  ref_pos  xm_byte
    /// ```
    /// where `meth_char` is `+` for methylated calls and `-` for unmethylated.
    ///
    /// # Output format (8-col, Yacht mode)
    ///
    /// See [`write_yacht_row`].
    ///
    /// # Phase 3a (inline-streaming epic) — the bedGraph tee
    ///
    /// `agg` is the optional in-memory `bismark_bedgraph::Aggregator` (present
    /// iff `--bedGraph`/`--cytosine_report`). When `Some`, after routing the
    /// call to its destination [`OutputFileEntry`], this method tees the call
    /// into the aggregator via `add_ranked`, passing the destination file's
    /// **creation rank** (its index in [`mode_keys`], looked up from
    /// `self.ranks`) and its **basename** (`entry.path.file_name()`, borrowed
    /// `&str`, NO allocation — this is the hot path, ≈1B calls). The tee is
    /// gated by R4: feed iff `cx` OR the basename starts with `"CpG"` (mirrors
    /// bedGraph's `select_input_files`). Calls routed to `MbiasOnly` (which has
    /// no real `OutputKey`) are skipped by construction (the `route_to_key`
    /// `None` short-circuit below returns before the tee). The tee is purely
    /// ADDITIVE — the per-context write below is unchanged (D2).
    ///
    /// Passing the destination file's creation rank makes chromosome ownership
    /// resolve to the lowest-rank (first-created) file emitting a call for it —
    /// matching Perl, which hands `bismark2bedGraph` the per-context files in
    /// creation order (`OT, CTOT, CTOB, OB`, NO sort) and owns by first-touch.
    /// The basename is still passed so the resolved owner's order key is
    /// byte-identical to what bedGraph's file-read path would intern from the
    /// same on-disk file (incl. the `.txt`/`.txt.gz` suffix).
    ///
    /// # Errors
    ///
    /// `BismarkExtractorError::IoWrite` on I/O failures. `InternalError` if
    /// the routed [`OutputKey`] is somehow missing from the eager-open map
    /// — shouldn't be possible because [`OutputFileMap::new`] inserts every
    /// key from `mode_keys`. Surfaces loudly rather than panicking if it
    /// ever happens.
    // The arg count (9) exceeds clippy's default threshold (7) after Phase 3a
    // added the `agg`/`cx` tee parameters. A param struct would obscure the
    // hot-path call sites (`route.rs`/`parallel.rs` already destructure
    // `ExtractState` and forward fields by name); the yacht col-6/7 args predate
    // this. Keep the flat signature — it reads cleanly at the two callers.
    #[allow(clippy::too_many_arguments)]
    pub fn write_call(
        &mut self,
        record_name: &[u8],
        chr: &str,
        call: MethCall,
        strand: BismarkStrand,
        yacht_col6: u32,
        yacht_col7: u32,
        agg: Option<&mut bismark_bedgraph::Aggregator>,
        cx: bool,
    ) -> Result<(), BismarkExtractorError> {
        // `route_to_key` returns None for MbiasOnly. The route_call
        // short-circuit upstream means write_call is never invoked in that
        // mode, but if it ever were we'd silently no-op (consistent with
        // "no per-context files in mbias_only"). The bedGraph tee is also
        // skipped here for MbiasOnly — `--mbias_only` is unreachable under
        // `--bedGraph` (Perl :1037-1041), so this is a non-issue, but the
        // early return keeps the tee strictly tied to a real destination file.
        let key = match route_to_key(self.mode, call.context, strand) {
            Some(k) => k,
            None => return Ok(()),
        };
        // Creation rank of the destination file (its `mode_keys` index). Looked
        // up BEFORE the `&mut entry` borrow below, since `self.ranks` and
        // `self.files` would otherwise conflict. `OutputKey` is `Copy`, so this
        // is a cheap hash lookup; every routable key is present in `ranks`
        // (inserted alongside `files` in `new`). Defaults to `u32::MAX` if
        // somehow absent — a chromosome owned by such a file would sort by its
        // basename only (no rank-based revision), but this is unreachable given
        // the eager-open invariant.
        let rank = self.ranks.get(&key).copied().unwrap_or(u32::MAX);
        let entry =
            self.files
                .get_mut(&key)
                .ok_or_else(|| BismarkExtractorError::InternalError {
                    message: format!(
                        "OutputFileMap missing key {:?} for mode {:?} — \
                         eager-open should have created every key from mode_keys",
                        key, self.mode,
                    ),
                })?;

        // Phase 3a tee — BEFORE the per-context write so the borrow of
        // `entry.path` (the basename) is taken while we hold `&entry`, then
        // released before the `&mut entry.writer` write below. NO allocation:
        // the basename is a borrowed `&str` slice of the already-owned
        // `entry.path`. The per-context write is unchanged (D2 additive).
        if let Some(agg) = agg {
            // `file_name().to_str()` is a borrowed `&str` slice of the
            // already-owned `entry.path` — zero allocation (Bismark filenames
            // are ASCII by construction, so `to_str()` never returns None in
            // practice; the `unwrap_or("")` is defensive). The suffix
            // (.txt[.gz]) is included, matching what bedGraph's file-read path
            // interns from the same on-disk file (R1).
            let basename: &str = entry
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            // R4 selection: feed iff --CX OR the destination is a CpG file.
            if cx || basename.starts_with("CpG") {
                agg.add_ranked(chr, call.ref_pos, call.methylated, rank, basename);
            }
        }

        if self.mode == OutputMode::Yacht {
            write_yacht_row(
                &mut entry.writer,
                record_name,
                chr,
                &call,
                yacht_col6,
                yacht_col7,
                strand,
            )?;
        } else {
            // 5-col format (Phase B byte-identity locked).
            let meth_char: u8 = if call.methylated { b'+' } else { b'-' };
            entry.writer.write_all(record_name)?;
            entry.writer.write_all(b"\t")?;
            entry.writer.write_all(&[meth_char])?;
            entry.writer.write_all(b"\t")?;
            entry.writer.write_all(chr.as_bytes())?;
            entry.writer.write_all(b"\t")?;
            entry
                .writer
                .write_all(call.ref_pos.to_string().as_bytes())?;
            entry.writer.write_all(b"\t")?;
            entry.writer.write_all(&[call.xm_byte])?;
            entry.writer.write_all(b"\n")?;
        }
        // Phase C.2 (#865): bump records_written AFTER all writes succeed.
        // Per plan §5.3 step 2 + R4: partial-write failures (any of the
        // write_all `?`s above) propagate out before the counter bumps, so
        // we never over-count. The empty-sweep uses this counter to
        // decide whether to unlink the file at finalize time.
        entry.records_written = entry.records_written.saturating_add(1);
        Ok(())
    }

    /// Flush every writer in the map. Called from `ExtractState::finalize`
    /// before the splitting-report is written so any buffered call lines
    /// are on disk before the run terminates. On the empty `MbiasOnly`
    /// map this is a no-op.
    ///
    /// For gzipped writers, `BufWriter::flush` propagates to the inner gzp
    /// `ParCompress`, which writes its trailing gzip footer when the writer
    /// drops (which happens at `cleanup_all` time, or at struct-drop time
    /// for the normal exit path) — NOT on flush.
    pub fn flush_all(&mut self) -> Result<(), std::io::Error> {
        for entry in self.files.values_mut() {
            entry.writer.flush()?;
        }
        Ok(())
    }

    /// Sweep empty per-strand output files at flush time, matching Perl's
    /// end-of-run `was empty -> deleted` behaviour (closes #865).
    ///
    /// For each entry: drop the writer (closes the `File` + flushes the gzp
    /// `ParCompress` gzip trailer if applicable — `flush_all` does NOT write
    /// the gzip trailer, only `drop` does); if `records_written == 0`,
    /// unlink the file and emit `{filename} was empty ->\tdeleted` to
    /// **STDERR** via `eprintln!`. Otherwise emit `{filename} contains
    /// data ->\tkept`. Two trailing `eprintln!()` calls mirror Perl line
    /// 625's `warn "\n\n"`.
    ///
    /// Empties the internal map (the sweep is the terminal lifecycle
    /// method for `OutputFileMap`); subsequent `write_call` invocations
    /// would fall through to the `missing key` `InternalError` path.
    ///
    /// **STDERR vs STDOUT**: matches Perl `:607` + `:615` which use `warn`
    /// (stderr). Earlier rev-0 of this plan routed to stdout citing
    /// "matches Perl exactly" — that was wrong; corrected in rev 1 per
    /// dual plan-review C3.
    ///
    /// # Errors
    ///
    /// **Phase G rev 2 (code-review B H2 fix)**: per-file `remove_file`
    /// failures are now logged-and-skipped rather than aborting the
    /// loop. Previously the function returned on the first error,
    /// dropping subsequent unlinks AND skipping the post-sweep
    /// splitting-report + M-bias + Phase G chain writes. Now the loop
    /// always runs to completion; a failed unlink emits an
    /// `eprintln!("warning: …")` line and the file is still recorded as
    /// `swept` (the intent was to drop it; the partial result on disk
    /// reflects a transient FS issue, not a logical-state divergence).
    /// Returns `Ok` regardless; the function's signature retains
    /// `Result<_, io::Error>` for forward-compat with any future
    /// non-`remove_file` IO needs.
    pub fn finalize_with_empty_sweep(
        &mut self,
        logger: crate::logging::Logger,
    ) -> Result<FinalizationReport, std::io::Error> {
        let entries: Vec<_> = self.files.drain().collect();
        let mut kept: Vec<PathBuf> = Vec::new();
        let mut swept: Vec<PathBuf> = Vec::new();
        for (
            _,
            OutputFileEntry {
                path,
                writer,
                records_written,
            },
        ) in entries
        {
            // Explicit drop closes the writer AND flushes the gzip trailer
            // for gzipped writers (which `flush_all` doesn't — gzip trailer
            // emission is tied to gzp ParCompress's Drop impl, which calls
            // finish() to write the footer, not to its flush).
            // For kept files this is the seal-the-trailer point.
            drop(writer);
            // Phase G (rev 1 C4): canonicalize BEFORE potential removal so
            // the swept list still gets an absolute path. canonicalize
            // requires the file to exist; falling back to the as-is path
            // covers the (defensive) case where the file disappeared
            // between drop and stat.
            let abs_path = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            // Phase C.2 code-review B H1: emit the FULL path (matches
            // Perl `:607, :615` which use `$sorting_files[$index]` =
            // `$output_dir . $filename`). We display the canonical path
            // post-Phase-G so log lines match the argv passed downstream.
            let path_str = abs_path.display();
            if records_written == 0 {
                // Phase G rev 2 (code-review B H2): log-and-continue on
                // unlink failure so subsequent files still get processed
                // and post-sweep work runs.
                if let Err(e) = std::fs::remove_file(&path) {
                    // Genuine warning — never gated by --quiet.
                    eprintln!("warning: failed to remove empty output file {path_str}: {e}");
                }
                logger.note(&format!("{path_str} was empty ->\tdeleted"));
                swept.push(abs_path);
            } else {
                logger.note(&format!("{path_str} contains data ->\tkept"));
                kept.push(abs_path);
            }
        }
        // Perl line 625: `warn "\n\n";` — two trailing blank lines on
        // stderr to mark the end of the sweep block. Mirroring for
        // consistency with downstream tooling that visually parses the
        // captured stderr.
        logger.note("");
        logger.note("");
        // Phase G (rev 1 I7): sort kept lexicographically so the argv
        // positional tail passed to bismark2bedGraph is deterministic
        // across runs (underlying HashMap iteration order is not).
        kept.sort();
        swept.sort();
        Ok(FinalizationReport { kept, swept })
    }

    /// Drop all writers + best-effort remove every file. Called from
    /// `extract_se` / `extract_pe`'s pre-finalize error paths. One failed
    /// `remove_file` doesn't prevent the others — we log via `eprintln!`
    /// and continue. On the empty `MbiasOnly` map this is a no-op.
    ///
    /// Note: this only runs on clean error paths (where `main.rs::run`'s
    /// `Result::Err` handler invokes us). Panic-mid-write **does not**
    /// trigger cleanup — partial `.gz` files may be left on disk in
    /// possibly-truncated state. Documented in Phase E plan §4.6.
    pub fn cleanup_all(&mut self) {
        // Drain into a vec to avoid double-borrow.
        let entries: Vec<_> = self.files.drain().collect();
        for (
            _,
            OutputFileEntry {
                path,
                writer,
                records_written: _,
            },
        ) in entries
        {
            // Explicitly close the writer (and the inner gzp `ParCompress`,
            // if any) BEFORE calling `remove_file`. A named `let` binding (even
            // underscore-prefixed) lives to the end of the loop iteration,
            // so without this explicit drop the file would still be open
            // when `remove_file` runs — benign on Unix but fails on Windows
            // where `remove_file` on an open handle is denied.
            drop(writer);
            if let Err(e) = std::fs::remove_file(&path) {
                eprintln!(
                    "warning: failed to remove partial output file {}: {}",
                    path.display(),
                    e
                );
            }
        }
    }
}

/// Number of threads gzp uses for parallel `--gzip` compression.
///
/// **Decoupled from `--parallel` by design (#884 R2).** gzp's `ParCompress`
/// runs its own compression thread pool, independent of the extractor's
/// worker count. The default run is `--parallel 1`, yet single-threaded gzip
/// is the dominant serial wall (Phase-0 spike: `.gz` 75 s, of which ~52 s is
/// compression). A fixed pool of 4 captures that wall on *every* `--gzip`
/// path — including the common `--parallel 1` default — reproducing the
/// spike's measured ~4.1x (`.gz` 75 s -> ~18 s) regardless of `--parallel`.
/// Tying it to `--parallel` instead would leave the default path
/// single-threaded (~75 s), defeating R2's purpose.
///
/// **Aggregate thread footprint (dual code-review, Medium):** gzp spawns its
/// pool *eagerly* at `from_writer` (writer-open), not lazily, and each gzipped
/// file holds `1 writer + GZIP_COMPRESS_THREADS compressor` threads for the
/// whole run. So the real total is `(GZIP_COMPRESS_THREADS + 1) × open_files`
/// — e.g. Default mode opens 12 split files ⇒ ~60 gzip threads, independent of
/// `--parallel` (including zero-record CTOT/CTOB strands later swept). No
/// correctness/CPU impact (idle threads block on empty channels), but it is a
/// thread-count, not a "pool of 4". Lowering the value (e.g. 2–3) or
/// lazy-opening writers is a measured follow-up — 4 reproduces the validated
/// spike, so it stays for R2.
const GZIP_COMPRESS_THREADS: usize = 4;

/// Factory: open the per-key writer, dispatching to plain `File` or a
/// parallel-gzip `gzp::par::compress::ParCompress<Gzip>` writer based on `gzip`.
///
/// Returns the writer already wrapped in an 8-KiB `BufWriter` (matching
/// Phase B's capacity). `Box<dyn Write + Send>` is the inner type to
/// keep the `OutputFileMap::write_call` body branch-free w.r.t. plain-vs-gz.
///
/// **gzip output framing (#884 R2):** gzp's `Gzip` format emits a *single*
/// gzip member — one header, sync-flushed DEFLATE blocks, one stream-wide
/// CRC32+ISIZE footer (gzp `par/compress.rs` writes `header()`/`footer()`
/// once per stream). A plain single-member `GzDecoder` reads it correctly; no
/// `MultiGzDecoder` is needed. The footer is written when the writer is
/// **dropped** (gzp's `Drop` calls `finish()`), matching the flate2
/// `GzEncoder` drop-finalization the empty-sweep relies on. Caveats: a
/// footer-flush I/O error surfaces as a *panic* on drop (gzp `.unwrap()`s),
/// unlike flate2's silent swallow — mid-stream write errors still propagate
/// as `io::Error`. The `deflate_rust` backend (pure Rust, no cmake) skips the
/// cross-block dictionary, so the *compressed* bytes differ from flate2's,
/// but the *decompressed* content is byte-identical (no test hashes raw
/// `.gz`; the colossal smoke compares `zcat | sort | md5`).
fn open_writer(path: &Path, gzip: bool) -> Result<BoxedWriter, std::io::Error> {
    let file = File::create(path)?;
    let inner: Box<dyn Write + Send> = if gzip {
        // #884 R2: parallelize the single-threaded gzip compression wall via
        // gzp's ParCompress pool (deflate_rust backend). num_threads is a
        // fixed constant decoupled from --parallel — see GZIP_COMPRESS_THREADS.
        Box::new(
            gzp::par::compress::ParCompressBuilder::<gzp::deflate::Gzip>::new()
                .num_threads(GZIP_COMPRESS_THREADS)
                .expect("GZIP_COMPRESS_THREADS is nonzero")
                .from_writer(file),
        )
    } else {
        Box::new(file)
    };
    Ok(BufWriter::with_capacity(8 * 1024, inner))
}

/// Per-context counts accumulated during the SE/PE loop. Drives the
/// `_splitting_report.txt` content at finalize time.
#[derive(Debug, Default)]
pub struct SplittingReport {
    /// SE: number of records iterated. PE: number of PAIRS iterated (NOT
    /// 2×pairs). Matches Perl `bismark_methylation_extractor:2459` /
    /// `$counting{sequences_count}` which is incremented once per outer-
    /// loop iteration.
    ///
    /// **Phase C.2 (#864) correction:** rev 0 of Phase B added 2 per pair
    /// citing Perl line 2451, but `2451` is the `methylation_call_strings`
    /// counter, not `sequences_count`. C.2 splits the two counters
    /// (`records_processed` = pairs for PE; `call_strings_processed` =
    /// 2×pairs) and fixes pipeline.rs:254 + parallel.rs:770.
    pub records_processed: u64,
    /// SE: equals `records_processed`. PE: 2×pairs (one per XM string
    /// processed). Matches Perl `bismark_methylation_extractor:2451` /
    /// `$counting{methylation_call_strings}`.
    ///
    /// **Phase C.2 (#864) addition:** drives the Perl line 2483 report
    /// row `"Total number of methylation call strings processed: N"`.
    pub call_strings_processed: u64,
    /// Total methylation calls (`Z`+`z`+`X`+`x`+`H`+`h`).
    pub calls_total: u64,
    /// `Z`.
    pub calls_cpg_meth: u64,
    /// `z`.
    pub calls_cpg_unmeth: u64,
    /// `X`.
    pub calls_chg_meth: u64,
    /// `x`.
    pub calls_chg_unmeth: u64,
    /// `H`.
    pub calls_chh_meth: u64,
    /// `h`.
    pub calls_chh_unmeth: u64,
}

impl SplittingReport {
    /// Compute percent methylation for one context. Returns `0.00` (not NaN)
    /// for empty contexts — matches Perl behaviour for the zero-denominator
    /// case.
    pub fn percent_meth(meth: u64, unmeth: u64) -> f64 {
        let total = meth.saturating_add(unmeth);
        if total == 0 {
            0.0
        } else {
            (meth as f64) * 100.0 / (total as f64)
        }
    }

    /// Sum `other` field-wise into `self`. Commutative and associative
    /// (every field is a `u64::saturating_add` sum). Used by Phase F's
    /// collector to merge per-worker `SplittingReport` deltas at
    /// end-of-stream — rev 1 reuses the live type instead of a separate
    /// `SplittingReportDelta` per Reviewer A C2 / Reviewer B G4.
    pub fn add(&mut self, other: &Self) {
        self.records_processed = self
            .records_processed
            .saturating_add(other.records_processed);
        self.call_strings_processed = self
            .call_strings_processed
            .saturating_add(other.call_strings_processed);
        self.calls_total = self.calls_total.saturating_add(other.calls_total);
        self.calls_cpg_meth = self.calls_cpg_meth.saturating_add(other.calls_cpg_meth);
        self.calls_cpg_unmeth = self.calls_cpg_unmeth.saturating_add(other.calls_cpg_unmeth);
        self.calls_chg_meth = self.calls_chg_meth.saturating_add(other.calls_chg_meth);
        self.calls_chg_unmeth = self.calls_chg_unmeth.saturating_add(other.calls_chg_unmeth);
        self.calls_chh_meth = self.calls_chh_meth.saturating_add(other.calls_chh_meth);
        self.calls_chh_unmeth = self.calls_chh_unmeth.saturating_add(other.calls_chh_unmeth);
    }
}

/// True for output modes that emit `Output specified: comprehensive` in
/// the splitting report (Perl `$full == 1`). Perl sets `$full=1` for:
///   - `--comprehensive` (`OutputMode::Comprehensive`)
///   - `--comprehensive --merge_non_CpG` (`OutputMode::ComprehensiveMergeNonCpG`)
///   - `--yacht` (Perl `:1331`)
///
/// **Phase C.2 code-review B C1 fix:** centralises the predicate so all
/// three call sites (Output-specified, merge-note, percentage-block)
/// stay in sync. Pre-fix, `Yacht` was missing from the comprehensive
/// match, causing the report to emit "strand-specific (default)" for
/// `--yacht` which diverges from Perl's "comprehensive".
fn emits_comprehensive(mode: OutputMode) -> bool {
    matches!(
        mode,
        OutputMode::Comprehensive | OutputMode::ComprehensiveMergeNonCpG | OutputMode::Yacht
    )
}

/// True for output modes that emit the `Methylation in CHG and CHH
/// context will be merged …` report note AND collapse the percentage
/// block from 3 lines (CpG/CHG/CHH) to 2 (CpG/Non-CpG). Perl
/// `$merge_non_CpG == 1`. Set for:
///   - `--merge_non_CpG` (`OutputMode::MergeNonCpG`)
///   - `--comprehensive --merge_non_CpG` (`OutputMode::ComprehensiveMergeNonCpG`)
///   - `--yacht` (Perl `:1333`)
///
/// **Phase C.2 code-review B C1 fix.**
fn merges_non_cpg(mode: OutputMode) -> bool {
    matches!(
        mode,
        OutputMode::MergeNonCpG | OutputMode::ComprehensiveMergeNonCpG | OutputMode::Yacht
    )
}

/// Write one percentage row matching Perl's per-context format.
///
/// **Phase C.2 (#864) addition.** If `meth + unmeth == 0`, writes the
/// zero-denominator fallback string (Perl `:2528` / `:2548` / `:2556` /
/// `:2537`). Trailing newline count is `\n` for non-last rows; `\n\n\n`
/// for the LAST row (CHH in default 3-context output, or Non-CpG in
/// `--merge_non_CpG` mode). The triple-newline matches Perl `:2553` /
/// `:2534` / `:2556` / `:2537` which bake `\n\n\n` directly into the
/// last-line format string.
///
/// Uses `write_all` (not `writeln!`) so the per-Perl-line trailing-newline
/// count is auditable inline with the format string — see SPEC §8.3 and
/// Phase C.2 plan §A13.
fn write_percent_or_fallback(
    w: &mut impl Write,
    ctx_label: &str,
    meth: u64,
    unmeth: u64,
    is_last: bool,
) -> Result<(), std::io::Error> {
    let trailing: &[u8] = if is_last { b"\n\n\n" } else { b"\n" };
    let total = meth.saturating_add(unmeth);
    if total == 0 {
        w.write_all(b"Can't determine percentage of methylated Cs in ")?;
        w.write_all(ctx_label.as_bytes())?;
        w.write_all(b" context if value was 0")?;
        w.write_all(trailing)?;
    } else {
        let pct = (meth as f64) * 100.0 / (total as f64);
        w.write_all(b"C methylated in ")?;
        w.write_all(ctx_label.as_bytes())?;
        w.write_all(b" context:\t")?;
        let pct_str = format!("{pct:.1}");
        w.write_all(pct_str.as_bytes())?;
        w.write_all(b"%")?;
        w.write_all(trailing)?;
    }
    Ok(())
}

/// Write `{output_dir}/{basename}_splitting_report.txt` in the exact format
/// produced by Perl `bismark_methylation_extractor` v0.25.1.
///
/// **Phase C.2 rewrite (closes #864).** Mirrors Perl lines 4995-5047
/// (header block) + 2482-2556 (body block) byte-for-byte. Conditional
/// emission of `Ignoring …`, `Output specified: …`, `No overlapping
/// methylation calls specified`, `Genomic equivalent sequences …`, and
/// `Methylation in CHG and CHH context …` lines mirrors Perl semantics.
///
/// `is_paired` is the **resolved** SE-vs-PE boolean (from
/// `ExtractState::is_paired`), NOT `config.paired_mode` (which can be
/// `AutoDetect` even after the dispatch picked one).
///
/// Uses `write_all(b"\n")` instead of `writeln!` for byte-identity on
/// Windows — see Phase C.2 plan §A13.
///
/// # Errors
///
/// `std::io::Error` on file creation, write, or flush failure.
#[allow(clippy::write_with_newline)]
// Clippy suggests `writeln!` for `write!(... "...\n", ...)` patterns.
// We deliberately reject that lint here for **byte-count auditability**
// against the Perl reference (Phase C.2 plan §A13 / SPEC §8.3 byte-
// identity invariant). The Perl source has multiple trailing-newline
// patterns per line (`\n` for most, `\n\n` for spacer lines, `\n\n\n`
// for the last percentage line); keeping the `\n` counts visually
// inline with the format string makes the per-Perl-line correspondence
// easy to audit. `writeln!` would hide those counts, requiring readers
// to mentally insert one `\n` per macro call and reconstruct the byte-
// sequence from prose. Note: `writeln!` does NOT emit CRLF on Windows
// — Rust's `std::fs::File` is binary-mode by default. Clarification
// added per code-review B M1.
pub fn write_splitting_report(
    path: &Path,
    input_path: &Path,
    config: &ResolvedConfig,
    is_paired: bool,
    report: &SplittingReport,
) -> Result<(), std::io::Error> {
    let mut w = BufWriter::with_capacity(8 * 1024, File::create(path)?);

    // Step 2: bare basename (Perl :4995 — `print REPORT "$output_filename\n\n";`
    // where $output_filename is the input file basename without dir).
    let basename = input_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| input_path.display().to_string());
    w.write_all(basename.as_bytes())?;
    w.write_all(b"\n")?;
    // Step 3: blank line.
    w.write_all(b"\n")?;

    // Step 4-6: parameter block (Perl :4996-5004).
    w.write_all(b"Parameters used to extract methylation information:\n")?;
    w.write_all(b"Bismark Extractor Version: ")?;
    w.write_all(BISMARK_VERSION.as_bytes())?;
    w.write_all(b"\n")?;
    if is_paired {
        w.write_all(b"Bismark result file: paired-end (SAM format)\n")?;
    } else {
        w.write_all(b"Bismark result file: single-end (SAM format)\n")?;
    }

    // Step 7: conditional `Ignoring …` lines (Perl :5006-5028). SE vs PE
    // branches; emit only when the corresponding field is non-zero.
    if is_paired {
        if config.ignore_5p_r1 > 0 {
            write!(w, "Ignoring first {} bp of Read 1\n", config.ignore_5p_r1)?;
        }
        if config.ignore_5p_r2 > 0 {
            write!(w, "Ignoring first {} bp of Read 2\n", config.ignore_5p_r2)?;
        }
        if config.ignore_3p_r1 > 0 {
            write!(w, "Ignoring last {} bp of Read 1\n", config.ignore_3p_r1)?;
        }
        if config.ignore_3p_r2 > 0 {
            write!(w, "Ignoring last {} bp of Read 2\n", config.ignore_3p_r2)?;
        }
    } else {
        if config.ignore_5p_r1 > 0 {
            write!(w, "Ignoring first {} bp\n", config.ignore_5p_r1)?;
        }
        if config.ignore_3p_r1 > 0 {
            write!(w, "Ignoring last {} bp\n", config.ignore_3p_r1)?;
        }
    }

    // Step 8: Output specified (Perl :5030-5034). `$full` controls the
    // emission; Perl `:1331` sets `$full=1` for --yacht so Yacht emits
    // `comprehensive` here too. Centralised via [`emits_comprehensive`]
    // (per code-review B C1 — `Yacht` was previously missing from this
    // arm, causing three byte-divergences in --yacht mode).
    if emits_comprehensive(config.output_mode) {
        w.write_all(b"Output specified: comprehensive\n")?;
    } else {
        w.write_all(b"Output specified: strand-specific (default)\n")?;
    }

    // Step 9: no_overlap line — matches Perl :5037 `if ($no_overlap)`.
    // Perl's SE branch leaves `$no_overlap` undef (declared at :931, only
    // assigned in the PE branch at :1219/1224) → falsy → line skipped.
    //
    // Rust's resolver at cli.rs:467-471 sets `config.no_overlap = !include_overlap`
    // whenever `paired_mode != SingleEnd` (including `AutoDetect` — the Phase
    // C rev 1 broadening that catches the AutoDetect-then-PE leak). For an
    // AutoDetect-then-SE path the flag stays `true` even though the BAM is SE,
    // so we MUST gate on the post-detection `is_paired` boolean here, not on
    // `config.no_overlap` alone. (#876 Bug A regression: rev 0 of this code
    // gated only on `config.no_overlap`, causing every SE splitting_report on
    // the colossal 10M matrix to emit a spurious +43-byte overlap line.)
    if is_paired && config.no_overlap {
        w.write_all(b"No overlapping methylation calls specified\n")?;
    }

    // Step 10: fasta annotation (Perl :5040).
    if config.fasta_annotation {
        w.write_all(b"Genomic equivalent sequences will be printed out in FastA format\n")?;
    }

    // Step 11: merge_non_CpG note (Perl :5043). Yacht sets
    // `$merge_non_CpG=1` at Perl `:1333` so it emits this note too —
    // [`merges_non_cpg`] centralises the predicate.
    if merges_non_cpg(config.output_mode) {
        w.write_all(
            b"Methylation in CHG and CHH context will be merged into \"non-CpG context\" output\n",
        )?;
    }

    // Step 12: header→body gap. Perl :5047 emits `\n` (close header);
    // Perl :2482 emits leading `\n` of body. Combined: two blank lines
    // visible (3 consecutive \n bytes total: prev-line \n + 5047 \n +
    // 2482 leading \n). Write the two extras here. Phase C.2 plan rev 1
    // Critical C2 fix.
    w.write_all(b"\n\n")?;

    // Step 13: Perl :2482 `"\nProcessed $sequences_count lines in total\n"`
    // (the leading \n is included in step 12 above; here just the line).
    write!(w, "Processed {} lines in total\n", report.records_processed)?;

    // Step 14: Perl :2483 has trailing \n\n. The second \n becomes the
    // blank line before the "Final Cytosine Methylation Report" header.
    write!(
        w,
        "Total number of methylation call strings processed: {}\n\n",
        report.call_strings_processed
    )?;

    // Step 15-16: section header (Perl :2510).
    w.write_all(b"Final Cytosine Methylation Report\n")?;
    w.write_all(b"=================================\n")?; // 33 `=`

    // Step 17: total C's (Perl :2513 trailing \n\n).
    write!(
        w,
        "Total number of C's analysed:\t{}\n\n",
        report.calls_total
    )?;

    // Step 18: methylated trio (Perl :2515-2517). Last line ends \n\n.
    write!(
        w,
        "Total methylated C's in CpG context:\t{}\n",
        report.calls_cpg_meth
    )?;
    write!(
        w,
        "Total methylated C's in CHG context:\t{}\n",
        report.calls_chg_meth
    )?;
    write!(
        w,
        "Total methylated C's in CHH context:\t{}\n\n",
        report.calls_chh_meth
    )?;

    // Step 19: unmethylated trio (Perl :2519-2521). NOTE phrasing change:
    // "Total C to T conversions in {ctx} context:" not "Total unmethylated".
    write!(
        w,
        "Total C to T conversions in CpG context:\t{}\n",
        report.calls_cpg_unmeth
    )?;
    write!(
        w,
        "Total C to T conversions in CHG context:\t{}\n",
        report.calls_chg_unmeth
    )?;
    write!(
        w,
        "Total C to T conversions in CHH context:\t{}\n\n",
        report.calls_chh_unmeth
    )?;

    // Step 20: percentage block via write_percent_or_fallback. Branches:
    //   Default / Comprehensive: 3 lines (CpG/CHG/CHH; CHH is last).
    //   MergeNonCpG / ComprehensiveMergeNonCpG / Yacht: 2 lines
    //     (CpG/Non-CpG; Non-CpG is last) — Yacht sets `$merge_non_CpG=1`
    //     at Perl `:1333` so its percentage block collapses to two
    //     contexts (per code-review B C1).
    if merges_non_cpg(config.output_mode) {
        // Perl :2525-2528: CpG percent (\n only).
        write_percent_or_fallback(
            &mut w,
            "CpG",
            report.calls_cpg_meth,
            report.calls_cpg_unmeth,
            /*is_last=*/ false,
        )?;
        // Perl :2534-2537: Non-CpG combined (\n\n\n trailing, is_last=true).
        let non_cpg_meth = report.calls_chg_meth.saturating_add(report.calls_chh_meth);
        let non_cpg_unmeth = report
            .calls_chg_unmeth
            .saturating_add(report.calls_chh_unmeth);
        write_percent_or_fallback(&mut w, "non-CpG", non_cpg_meth, non_cpg_unmeth, true)?;
    } else {
        write_percent_or_fallback(
            &mut w,
            "CpG",
            report.calls_cpg_meth,
            report.calls_cpg_unmeth,
            false,
        )?;
        write_percent_or_fallback(
            &mut w,
            "CHG",
            report.calls_chg_meth,
            report.calls_chg_unmeth,
            false,
        )?;
        write_percent_or_fallback(
            &mut w,
            "CHH",
            report.calls_chh_meth,
            report.calls_chh_unmeth,
            /*is_last=*/ true,
        )?;
    }

    // Step 21: flush.
    w.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::PairedMode;

    #[test]
    fn write_percent_or_fallback_cpg_not_last_emits_single_newline() {
        // Phase C.2 (#864) regression guard: CpG percentage line (when not
        // the last context) ends with `\n`, not `\n\n\n`. Mirrors Perl
        // line 2525 / 2545.
        let mut buf = Vec::new();
        write_percent_or_fallback(&mut buf, "CpG", 50, 50, /*is_last=*/ false).unwrap();
        assert_eq!(buf, b"C methylated in CpG context:\t50.0%\n");
    }

    #[test]
    fn write_percent_or_fallback_chh_last_emits_triple_newline() {
        // Phase C.2 (#864) regression guard for Critical C1: CHH percentage
        // (last context in default 3-context output) ends with `\n\n\n`
        // baked INTO the line itself (matches Perl line 2553 format string).
        let mut buf = Vec::new();
        write_percent_or_fallback(&mut buf, "CHH", 1, 99, /*is_last=*/ true).unwrap();
        assert_eq!(buf, b"C methylated in CHH context:\t1.0%\n\n\n");
    }

    #[test]
    fn write_percent_or_fallback_zero_denom_cpg_emits_perl_fallback_string() {
        // Perl line 2528: `Can't determine percentage of methylated Cs in
        // CpG context if value was 0\n` (single \n for non-last CpG).
        let mut buf = Vec::new();
        write_percent_or_fallback(&mut buf, "CpG", 0, 0, /*is_last=*/ false).unwrap();
        assert_eq!(
            buf,
            b"Can't determine percentage of methylated Cs in CpG context if value was 0\n"
        );
    }

    #[test]
    fn write_percent_or_fallback_zero_denom_chh_last_emits_triple_newline() {
        // Perl line 2556: same fallback string but with `\n\n\n` trailing
        // for the LAST context (3-context CHH or merge_non_CpG Non-CpG).
        let mut buf = Vec::new();
        write_percent_or_fallback(&mut buf, "CHH", 0, 0, /*is_last=*/ true).unwrap();
        assert_eq!(
            buf,
            b"Can't determine percentage of methylated Cs in CHH context if value was 0\n\n\n"
        );
    }

    #[test]
    fn write_percent_or_fallback_uses_one_decimal_precision() {
        // Phase C.2 (#864): Perl `sprintf("%.1f", ...)` → 1 decimal place
        // (e.g. `12.5%`, not `12.50%`). Plan A5 / I2 documented the
        // banker's-rounding caveat for exact half-decimals.
        let mut buf = Vec::new();
        // 5/40 = 12.5 (representable exactly in f64; both rounding modes
        // produce 12.5).
        write_percent_or_fallback(&mut buf, "CpG", 5, 35, false).unwrap();
        assert_eq!(buf, b"C methylated in CpG context:\t12.5%\n");
    }

    #[test]
    fn splitting_report_add_is_commutative() {
        let a = SplittingReport {
            records_processed: 100,
            call_strings_processed: 200, // PE: 2× pairs
            calls_total: 500,
            calls_cpg_meth: 30,
            calls_cpg_unmeth: 70,
            calls_chg_meth: 50,
            calls_chg_unmeth: 100,
            calls_chh_meth: 80,
            calls_chh_unmeth: 170,
        };
        let b = SplittingReport {
            records_processed: 250,
            call_strings_processed: 500,
            calls_total: 1250,
            calls_cpg_meth: 100,
            calls_cpg_unmeth: 150,
            calls_chg_meth: 200,
            calls_chg_unmeth: 250,
            calls_chh_meth: 220,
            calls_chh_unmeth: 330,
        };
        // a + b
        let mut a_into_b = SplittingReport {
            records_processed: b.records_processed,
            call_strings_processed: b.call_strings_processed,
            calls_total: b.calls_total,
            calls_cpg_meth: b.calls_cpg_meth,
            calls_cpg_unmeth: b.calls_cpg_unmeth,
            calls_chg_meth: b.calls_chg_meth,
            calls_chg_unmeth: b.calls_chg_unmeth,
            calls_chh_meth: b.calls_chh_meth,
            calls_chh_unmeth: b.calls_chh_unmeth,
        };
        a_into_b.add(&a);
        // b + a
        let mut b_into_a = SplittingReport {
            records_processed: a.records_processed,
            call_strings_processed: a.call_strings_processed,
            calls_total: a.calls_total,
            calls_cpg_meth: a.calls_cpg_meth,
            calls_cpg_unmeth: a.calls_cpg_unmeth,
            calls_chg_meth: a.calls_chg_meth,
            calls_chg_unmeth: a.calls_chg_unmeth,
            calls_chh_meth: a.calls_chh_meth,
            calls_chh_unmeth: a.calls_chh_unmeth,
        };
        b_into_a.add(&b);

        assert_eq!(a_into_b.records_processed, b_into_a.records_processed);
        assert_eq!(
            a_into_b.call_strings_processed,
            b_into_a.call_strings_processed
        );
        assert_eq!(a_into_b.calls_total, b_into_a.calls_total);
        assert_eq!(a_into_b.calls_cpg_meth, b_into_a.calls_cpg_meth);
        assert_eq!(a_into_b.calls_cpg_unmeth, b_into_a.calls_cpg_unmeth);
        assert_eq!(a_into_b.calls_chg_meth, b_into_a.calls_chg_meth);
        assert_eq!(a_into_b.calls_chg_unmeth, b_into_a.calls_chg_unmeth);
        assert_eq!(a_into_b.calls_chh_meth, b_into_a.calls_chh_meth);
        assert_eq!(a_into_b.calls_chh_unmeth, b_into_a.calls_chh_unmeth);
        // Sanity sums:
        assert_eq!(a_into_b.records_processed, 350);
        assert_eq!(a_into_b.call_strings_processed, 700);
        assert_eq!(a_into_b.calls_total, 1750);
    }

    // ─── #876 Bug A regression guards: SE splitting_report omits overlap line ──
    //
    // Background: `config.no_overlap` is resolved at CLI-time to `!include_overlap`
    // whenever `paired_mode != SingleEnd` (cli.rs:467-471), which catches the
    // AutoDetect case. The BAM may later be detected as SE; at that point the
    // resolved `no_overlap=true` flag is stale for the SE case. The writer
    // must gate on the post-detection `is_paired` flag, not on `no_overlap` alone.
    //
    // Perl reference: bismark_methylation_extractor:931 declares `$no_overlap`;
    // assignments only at :1219/1224 inside the PE branch (L1215-1224). All
    // other 7 references are reads/pass-throughs. SE → `$no_overlap` stays
    // undef → falsy → line 5037 emission skipped.

    fn default_config_for_splitting_report(
        paired_mode: PairedMode,
        no_overlap: bool,
    ) -> ResolvedConfig {
        ResolvedConfig {
            files: vec![PathBuf::from("sample.bam")],
            paired_mode,
            output_mode: OutputMode::Default,
            ignore_5p_r1: 0,
            ignore_3p_r1: 0,
            ignore_5p_r2: 0,
            ignore_3p_r2: 0,
            no_overlap,
            output_dir: PathBuf::from("/tmp"),
            no_header: false,
            gzip: false,
            emit_splitting_report: true,
            fasta_annotation: false,
            mbias_off: false,
            bedgraph: false,
            cytosine_report: false,
            cutoff: 1,
            remove_spaces: false,
            counts: true,
            zero_based: false,
            cx_context: false,
            split_by_chromosome: false,
            ucsc: false,
            buffer_size: None,
            gazillion: false,
            ample_memory: false,
            genome_folder: None,
            parallel: 1,
            quiet: false,
            verbose: false,
        }
    }

    fn write_and_read_splitting_report(config: &ResolvedConfig, is_paired: bool) -> String {
        let tmp = tempfile::NamedTempFile::new().expect("create tempfile");
        let report = SplittingReport::default();
        write_splitting_report(
            tmp.path(),
            &PathBuf::from("sample.bam"),
            config,
            is_paired,
            &report,
        )
        .expect("write splitting report");
        std::fs::read_to_string(tmp.path()).expect("read splitting report")
    }

    #[test]
    fn splitting_report_omits_overlap_line_in_se_mode() {
        // Plain SE: paired_mode=SingleEnd, no_overlap=false (resolver default
        // for SingleEnd per cli.rs:469). Writer must NOT emit overlap line.
        let cfg = default_config_for_splitting_report(PairedMode::SingleEnd, false);
        let body = write_and_read_splitting_report(&cfg, /*is_paired=*/ false);
        assert!(
            !body.contains("No overlapping methylation calls specified"),
            "SE splitting_report must not contain the overlap line; got:\n{body}"
        );
    }

    #[test]
    fn splitting_report_omits_overlap_line_for_autodetect_se() {
        // The actual bug-triggering state from #876: CLI is invoked WITHOUT
        // -s/-p, so paired_mode resolves to AutoDetect. cli.rs:467 then sets
        // no_overlap = !include_overlap = true. BAM is later detected as SE
        // (is_paired=false at write time). Writer must STILL omit the line.
        let cfg = default_config_for_splitting_report(PairedMode::AutoDetect, true);
        let body = write_and_read_splitting_report(&cfg, /*is_paired=*/ false);
        assert!(
            !body.contains("No overlapping methylation calls specified"),
            "AutoDetect-then-SE splitting_report must not contain the overlap line; got:\n{body}"
        );
    }

    #[test]
    fn splitting_report_includes_overlap_line_for_pe_default() {
        // PE without --include_overlap (the normal PE case). Resolver sets
        // no_overlap=true. BAM is PE (is_paired=true). Writer MUST emit the
        // line (matches Perl L5037 PE-branch behaviour).
        let cfg = default_config_for_splitting_report(PairedMode::PairedEnd, true);
        let body = write_and_read_splitting_report(&cfg, /*is_paired=*/ true);
        assert!(
            body.contains("No overlapping methylation calls specified"),
            "PE-default splitting_report must contain the overlap line; got:\n{body}"
        );
    }

    #[test]
    fn splitting_report_omits_overlap_line_for_pe_with_include_overlap() {
        // PE with --include_overlap → resolver sets no_overlap=false. Writer
        // must omit the line (matches Perl L5037 `if ($no_overlap)` false).
        let cfg = default_config_for_splitting_report(PairedMode::PairedEnd, false);
        let body = write_and_read_splitting_report(&cfg, /*is_paired=*/ true);
        assert!(
            !body.contains("No overlapping methylation calls specified"),
            "PE --include_overlap splitting_report must not contain the overlap line; got:\n{body}"
        );
    }
}

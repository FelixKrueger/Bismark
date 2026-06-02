//! Per-(mode-key) split-file map + splitting-report writer.
//!
//! The `(key, filename)` list comes from [`crate::output_mode::mode_keys`]
//! (all 5 non-`MbiasOnly` modes); each file is a plain `File` or a parallel-gzip
//! `ParCompress<Gzip>` when `--gzip` is set. `MbiasOnly` yields an empty map
//! (`route_call` short-circuits before any `write_call`).
//!
//! **Lazy-open (#889 item 1):** [`OutputFileMap::new`] no longer creates files
//! or spawns gzip threads — each writer is opened on its strand's first
//! `write_call`, so never-written strands (e.g. CTOT/CTOB in a directional
//! library) cost no file and no threads.
//!
//! The per-file writer is the [`SplitWriter`] enum (`Plain | Gzip`), replacing
//! the former `BufWriter<Box<dyn Write + Send>>` (#889 item 2) so finalization
//! can call gzp's explicit `finish()` (propagating a footer error) instead of
//! a `Drop`-time `unwrap()` panic. Both arms are `Send` (auto-derived); note
//! the single `OutputFileMap` is collector-owned on the main thread, so `Send`
//! is a forward-looking invariant, not a structural requirement today.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use bismark_io::BismarkStrand;
use gzp::ZWriter;
use gzp::deflate::Gzip;
use gzp::par::compress::{ParCompress, ParCompressBuilder};

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

/// A per-split-file writer: plain `File` or a gzp parallel-gzip
/// `ParCompress<Gzip>`, each fronted by an 8-KiB `BufWriter`.
///
/// Replaces the former type-erased `BufWriter<Box<dyn Write + Send>>`
/// (#889 item 2) so finalization can call gzp's **explicit** `finish()`
/// — surfacing a footer-flush/thread-join error as `io::Error` — instead of
/// relying on `ParCompress`'s `Drop`-time `finish().unwrap()`, which *panics*
/// on such an error. Both arms are `Send` (auto-derived), preserving the
/// forward-looking bound the boxed writer carried.
enum SplitWriter {
    Plain(BufWriter<File>),
    Gzip(BufWriter<ParCompress<Gzip>>),
}

impl Write for SplitWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            SplitWriter::Plain(w) => w.write(buf),
            SplitWriter::Gzip(w) => w.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            SplitWriter::Plain(w) => w.flush(),
            SplitWriter::Gzip(w) => w.flush(),
        }
    }
}

impl SplitWriter {
    /// Flush + finalize the writer, returning any error as `io::Error`.
    ///
    /// `Plain`: flush the `BufWriter` to the `File` (no trailer to write).
    /// `Gzip`: `flush()` the buffer into the `ParCompress`, then `get_mut()` +
    /// gzp's explicit `ZWriter::finish()` (footer + worker-join), surfacing
    /// errors as `io::Error` instead of gzp's `Drop`-time `unwrap()` panic
    /// (#889 item 2). On the **success** path `finish()` `take()`s the
    /// channels/handle, so the subsequent drop of the `ParCompress` is a no-op.
    /// On the **error** path gzp returns from `flush_last(true)?` before taking
    /// them, so its `Drop` would re-panic — see the inline note for why we
    /// `mem::forget` the writer there. (Regression-tested by
    /// `split_writer_gzip_finish_surfaces_error_not_panic` +
    /// `finalize_surfaces_kept_finish_error_via_result` over a failing sink.)
    fn finish(self) -> std::io::Result<()> {
        match self {
            SplitWriter::Plain(mut w) => w.flush(),
            SplitWriter::Gzip(mut bw) => {
                // Flush the 8-KiB buffer into the ParCompress (gzp's `flush` is
                // `flush_last(false)` — no footer). Capture the error but do NOT
                // early-return: we must still reach `finish()` below.
                let flush_res = bw.flush();
                // Explicitly finish the ParCompress: on success it writes the
                // gzip footer, joins the workers, AND `take()`s its channels —
                // so the subsequent drop of `bw` is a no-op.
                let finish_res = bw.get_mut().finish().map_err(std::io::Error::other);
                let result = flush_res.and(finish_res);
                if result.is_err() {
                    // #889 item 2 — the load-bearing bit. gzp 0.11.3's `finish()`
                    // returns from `flush_last(true)?` BEFORE it `take()`s the
                    // channels/handle, so on a finalization I/O error they stay
                    // `Some` and the ParCompress's `Drop` re-runs
                    // `finish().unwrap()` → PANIC (par/compress.rs:312). gzp gives
                    // us no way to disarm that Drop (private fields), so leak the
                    // writer to suppress it. Only reached on a footer/last-block
                    // write failure (e.g. ENOSPC) where the worker has already
                    // errored and the run is aborting — surfacing the error as a
                    // clean `io::Error` matters more than reclaiming a dying
                    // thread handle. The common (success) path drops normally.
                    // (Regression-tested by `split_writer_gzip_finish_*` +
                    // `finalize_surfaces_kept_finish_error_via_result`.)
                    std::mem::forget(bw);
                }
                result
            }
        }
    }
}

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
    /// `None` until the strand receives its first data row (#889 item 1
    /// lazy-open): creating the file + writing the header + spawning the gzp
    /// thread pool is deferred to `write_call`, so never-written strands
    /// (e.g. CTOT/CTOB in a directional library) cost no file and no threads.
    writer: Option<SplitWriter>,
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
    /// Open-time params captured for lazy-open in `write_call`.
    gzip: bool,
    no_header: bool,
}

impl OutputFileMap {
    /// Build the per-mode split-file map. **Lazy-open (#889 item 1):** no
    /// files are created, no headers written, and no gzip threads spawned
    /// here — each writer is opened on its strand's first `write_call`. This
    /// bounds the gzp thread footprint to the strands that actually receive
    /// data (never-written strands, e.g. CTOT/CTOB in a directional library,
    /// cost zero threads/files).
    ///
    /// Creates `output_dir` via `create_dir_all` if missing (matches Perl
    /// `make_path`), then canonicalizes it once so every entry's `path` is
    /// absolute — the `kept`/`swept` lists stay absolute even for strands
    /// whose file is never created.
    ///
    /// When `mode == MbiasOnly` returns an empty map (Perl `:5148-5151
    /// unless($mbias_only)`). `flush_all`/`cleanup_all`/`finalize_*` remain
    /// valid no-ops on the empty map.
    ///
    /// When `gzip == true` each lazily-opened writer is a gzp parallel-gzip
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
        // Canonicalize the dir once so kept/swept paths are absolute even for
        // never-created files (lazy-open). Fall back to the as-is dir if
        // canonicalize fails (kept paths then match what the caller passed).
        let dir_abs =
            std::fs::canonicalize(output_dir).unwrap_or_else(|_| output_dir.to_path_buf());

        let keys = mode_keys(mode, input_basename, gzip);
        let mut files: HashMap<OutputKey, OutputFileEntry> = HashMap::with_capacity(keys.len());

        for (key, filename) in keys {
            files.insert(
                key,
                OutputFileEntry {
                    path: dir_abs.join(filename),
                    writer: None, // opened lazily on first write (#889 item 1)
                    records_written: 0,
                },
            );
        }

        Ok(OutputFileMap {
            files,
            mode,
            gzip,
            no_header,
        })
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
    /// # Errors
    ///
    /// `BismarkExtractorError::IoWrite` on I/O failures. `InternalError` if
    /// the routed [`OutputKey`] is somehow missing from the map
    /// — shouldn't be possible because [`OutputFileMap::new`] inserts every
    /// key from `mode_keys`. Surfaces loudly rather than panicking if it
    /// ever happens.
    pub fn write_call(
        &mut self,
        record_name: &[u8],
        chr: &str,
        call: MethCall,
        strand: BismarkStrand,
        yacht_col6: u32,
        yacht_col7: u32,
    ) -> Result<(), BismarkExtractorError> {
        // `route_to_key` returns None for MbiasOnly. The route_call
        // short-circuit upstream means write_call is never invoked in that
        // mode, but if it ever were we'd silently no-op (consistent with
        // "no per-context files in mbias_only").
        let key = match route_to_key(self.mode, call.context, strand) {
            Some(k) => k,
            None => return Ok(()),
        };
        let gzip = self.gzip;
        let no_header = self.no_header;
        let yacht = self.mode == OutputMode::Yacht;
        let entry =
            self.files
                .get_mut(&key)
                .ok_or_else(|| BismarkExtractorError::InternalError {
                    message: format!(
                        "OutputFileMap missing key {:?} for mode {:?} — \
                         new() inserts every key from mode_keys",
                        key, self.mode,
                    ),
                })?;

        // Lazy-open (#889 item 1): create the file + write the header + spawn
        // the gzp pool only on the strand's first data row. The header is thus
        // the first bytes of any created file — byte-identical to the former
        // eager-open for every kept file.
        if entry.writer.is_none() {
            let mut w = open_split_writer(&entry.path, gzip)?;
            if !no_header {
                w.write_all(SPLIT_FILE_HEADER.as_bytes())?;
            }
            entry.writer = Some(w);
        }
        let writer = entry
            .writer
            .as_mut()
            .expect("writer was opened on the line above");

        if yacht {
            write_yacht_row(
                writer,
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
            writer.write_all(record_name)?;
            writer.write_all(b"\t")?;
            writer.write_all(&[meth_char])?;
            writer.write_all(b"\t")?;
            writer.write_all(chr.as_bytes())?;
            writer.write_all(b"\t")?;
            writer.write_all(call.ref_pos.to_string().as_bytes())?;
            writer.write_all(b"\t")?;
            writer.write_all(&[call.xm_byte])?;
            writer.write_all(b"\n")?;
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
    /// Skips never-opened (`None`) writers (lazy-open #889 item 1). For gzipped
    /// writers, `BufWriter::flush` propagates to the inner gzp `ParCompress`
    /// (`flush_last(false)` — pushes pending blocks, NOT the footer). The gzip
    /// footer is written later by [`SplitWriter::finish`] at finalize/cleanup
    /// time (#889 item 2), not on flush and no longer via `Drop`.
    pub fn flush_all(&mut self) -> Result<(), std::io::Error> {
        for entry in self.files.values_mut() {
            if let Some(writer) = entry.writer.as_mut() {
                writer.flush()?;
            }
        }
        Ok(())
    }

    /// Sweep empty per-strand output files at flush time, matching Perl's
    /// end-of-run `was empty -> deleted` behaviour (closes #865).
    ///
    /// For each entry: if it has an opened writer, [`SplitWriter::finish`] it
    /// (seals the gzip footer for gzipped writers + surfaces any I/O error as
    /// `Result` — #889 item 2). If `records_written == 0` (never-opened lazy
    /// entries, or the rare opened-but-no-rows remnant) unlink any file and emit
    /// `{filename} was empty ->\tdeleted` to **STDERR** via the logger;
    /// otherwise emit `{filename} contains data ->\tkept`. Two trailing
    /// blank `note("")` calls mirror Perl line 625's `warn "\n\n"`. A kept-file
    /// finish error is collected and returned after the loop completes.
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
        // #889 item 2: a footer-flush error on a kept gzip file is collected
        // here and returned after the loop, so the sweep still completes.
        let mut first_err: Option<std::io::Error> = None;
        for (
            _,
            OutputFileEntry {
                path,
                writer,
                records_written,
            },
        ) in entries
        {
            // Phase G (rev 1 C4): canonicalize for an absolute path. A
            // never-created file (lazy-open) can't be canonicalized, so fall
            // back to the stored path — which is already absolute (`new`
            // canonicalizes the output dir).
            let abs_path = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            // Phase C.2 code-review B H1: emit the FULL path (matches Perl
            // `:607, :615`); post-Phase-G the canonical path matches the argv
            // passed downstream.
            let path_str = abs_path.display();
            match writer {
                // Kept: a writer is opened only on a successful first write, so
                // in the normal finalize path an opened writer has data. Seal
                // the gzip trailer via gzp's explicit `finish()` (#889 item 2)
                // — surfacing a footer-flush/join error as `io::Error` instead
                // of `ParCompress`'s `Drop`-time `unwrap()` panic. Keep the
                // first error and continue (fail-open-on-remove philosophy).
                Some(w) if records_written > 0 => {
                    if let Err(e) = w.finish() {
                        first_err.get_or_insert(e);
                    }
                    logger.note(&format!("{path_str} contains data ->\tkept"));
                    kept.push(abs_path);
                }
                // Empty: never-opened (`None`, the common lazy-open case — no
                // file on disk) or opened-but-no-rows (an error-path remnant).
                // Finish any writer, remove any file (fail-open; a never-created
                // file gives NotFound, which is expected), log + record swept.
                // Matches Perl's end-of-run `was empty -> deleted`.
                maybe_writer => {
                    if let Some(w) = maybe_writer
                        && let Err(e) = w.finish()
                    {
                        first_err.get_or_insert(e);
                    }
                    match std::fs::remove_file(&path) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => {
                            // Genuine warning — never gated by --quiet.
                            eprintln!(
                                "warning: failed to remove empty output file {path_str}: {e}"
                            );
                        }
                    }
                    logger.note(&format!("{path_str} was empty ->\tdeleted"));
                    swept.push(abs_path);
                }
            }
        }
        // Perl line 625: `warn "\n\n";` — two trailing blank lines on stderr.
        logger.note("");
        logger.note("");
        // Phase G (rev 1 I7): sort kept lexicographically so the argv
        // positional tail passed to bismark2bedGraph is deterministic
        // across runs (underlying HashMap iteration order is not).
        kept.sort();
        swept.sort();
        // #889 item 2: surface a kept-file footer-flush error now that every
        // entry has been finalized + the sweep has run.
        if let Some(e) = first_err {
            return Err(e);
        }
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
            // Finish/close any opened writer (and its inner gzp `ParCompress`)
            // BEFORE `remove_file` — an open handle blocks removal on Windows.
            // Best-effort on this error path: ignore the finish result. A
            // never-opened (`None`) entry has no file, so `remove_file` returns
            // NotFound, which we ignore.
            if let Some(w) = writer {
                let _ = w.finish();
            }
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => eprintln!(
                    "warning: failed to remove partial output file {}: {}",
                    path.display(),
                    e
                ),
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
/// **Aggregate thread footprint (#889 item 1, resolved):** gzp spawns its pool
/// *eagerly* at `from_writer`, holding `1 writer + GZIP_COMPRESS_THREADS`
/// threads per open gzip file. The writers are now **lazy-opened** (only on a
/// strand's first data row — see [`OutputFileMap::new`]/[`open_split_writer`]),
/// so `open_files` is bounded to the strands that actually receive data, not
/// all `mode_keys`. A directional library no longer spawns threads for its
/// zero-record CTOT/CTOB strands (was `(4+1)×12 ≈ 60`; now `(4+1)×written`).
/// The constant stays at 4 — it reproduces the validated ~4.1× gzip speedup
/// (lowering it would need a perf re-measure; deliberately out of scope here).
const GZIP_COMPRESS_THREADS: usize = 4;

/// Factory: create the per-key file + writer, dispatching to a plain `File`
/// or a parallel-gzip `ParCompress<Gzip>` based on `gzip`. Called **lazily**
/// from `write_call` on a strand's first data row (#889 item 1), so the file
/// + gzip thread pool exist only for strands that receive data.
///
/// Returns a [`SplitWriter`] (8-KiB `BufWriter` over either arm).
///
/// **gzip output framing (#884 R2):** gzp's `Gzip` format emits a *single*
/// gzip member — one header, sync-flushed DEFLATE blocks, one stream-wide
/// CRC32+ISIZE footer. A plain single-member `GzDecoder` reads it correctly;
/// no `MultiGzDecoder` is needed. The footer is written by
/// [`SplitWriter::finish`] (gzp's explicit `ZWriter::finish`), called at
/// finalize/cleanup time — **not** on `flush` and no longer via a `Drop`-time
/// `unwrap()` (#889 item 2 replaced that panic with a propagated `io::Error`).
/// The `deflate_rust` backend skips the cross-block dictionary, so the
/// *compressed* bytes differ from flate2's, but the *decompressed* content is
/// byte-identical (no test hashes raw `.gz`; the real-data smoke compares
/// `zcat | sort | md5`).
fn open_split_writer(path: &Path, gzip: bool) -> Result<SplitWriter, std::io::Error> {
    let file = File::create(path)?;
    if gzip {
        // #884 R2: parallelize the single-threaded gzip compression wall via
        // gzp's ParCompress pool. num_threads is a fixed constant decoupled
        // from --parallel — see GZIP_COMPRESS_THREADS.
        let par = ParCompressBuilder::<Gzip>::new()
            .num_threads(GZIP_COMPRESS_THREADS)
            .expect("GZIP_COMPRESS_THREADS is nonzero")
            .from_writer(file);
        Ok(SplitWriter::Gzip(BufWriter::with_capacity(8 * 1024, par)))
    } else {
        Ok(SplitWriter::Plain(BufWriter::with_capacity(8 * 1024, file)))
    }
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

    // ── #889 item 2: finish() surfaces errors as io::Error, never panics ──

    /// A sink that fails every write/flush. `ParCompress` type-erases its
    /// underlying writer (a worker thread owns it), so a `SplitWriter::Gzip`
    /// can wrap one — letting us prove `finish()` returns `Err` (gzp surfaces
    /// the sink error as `GzpError::Io`) instead of panicking via
    /// `ParCompress`'s `Drop`-time `finish().unwrap()`. (The `Plain` arm is
    /// concretely `BufWriter<File>` and its `finish` is a plain flush that
    /// never panicked — the panic risk was gzp-only.)
    struct FailingWriter;
    impl Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("sink write failed"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("sink flush failed"))
        }
    }

    #[test]
    fn split_writer_gzip_finish_surfaces_error_not_panic() {
        let par = ParCompressBuilder::<Gzip>::new()
            .num_threads(1)
            .expect("nonzero")
            .from_writer(FailingWriter);
        let mut w = SplitWriter::Gzip(BufWriter::new(par));
        // Buffer a row so the gzip worker has data to push to the failing sink.
        let _ = w.write_all(b"r\t+\tchr1\t100\tZ\n");
        // #889 item 2: must return Err (gzp surfaces the sink error as
        // GzpError::Io), NOT panic via ParCompress's Drop-time finish().unwrap().
        assert!(
            w.finish().is_err(),
            "Gzip finish over a failing sink must return Err, not panic"
        );
    }

    #[test]
    fn finalize_surfaces_kept_finish_error_via_result() {
        let tmp = tempfile::tempdir().unwrap();
        // One kept gzip entry whose sink fails → finish() errors at finalize.
        let par = ParCompressBuilder::<Gzip>::new()
            .num_threads(1)
            .expect("nonzero")
            .from_writer(FailingWriter);
        let mut bad = SplitWriter::Gzip(BufWriter::new(par));
        let _ = bad.write_all(b"r\t+\tchr1\t100\tZ\n");
        let mut files = HashMap::new();
        files.insert(
            OutputKey::Yacht, // key value is irrelevant; finalize iterates entries
            OutputFileEntry {
                path: tmp.path().join("kept.txt"),
                writer: Some(bad),
                records_written: 1,
            },
        );
        let mut map = OutputFileMap {
            files,
            mode: OutputMode::Default,
            gzip: true,
            no_header: false,
        };
        // finalize collects the finish error and returns it after the sweep loop
        // (#889 item 2) — instead of panicking.
        let result = map.finalize_with_empty_sweep(crate::logging::Logger::new(true, false));
        assert!(
            result.is_err(),
            "finalize must surface the kept-file finish error as Err"
        );
    }

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

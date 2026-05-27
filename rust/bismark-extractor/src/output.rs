//! Per-(mode-key) split-file map + splitting-report writer.
//!
//! Phase B opened 12 strand×context files eagerly at [`OutputFileMap::new`]
//! time. Phase E generalises this to all 5 non-`MbiasOnly` modes: the
//! `(key, filename)` list comes from [`crate::output_mode::mode_keys`], and
//! each file may be wrapped in a `flate2::write::GzEncoder` when
//! `--gzip` is set. `MbiasOnly` skips eager-open entirely (the map is
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
use flate2::Compression;
use flate2::write::GzEncoder;

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

/// Per-call type-erased boxed writer (plain `File` or `GzEncoder<File>`)
/// wrapped in an 8-KiB `BufWriter`. Phase F may revisit static-dispatch
/// once profiling under multicore is available (Phase E plan §9.2 #2).
type BoxedWriter = BufWriter<Box<dyn Write + Send>>;

/// Eagerly-opened per-(mode-key) split files.
///
/// Rev 1 layout (Phase B): one map keyed by `OutputKey` storing both the
/// path (for cleanup) and an 8-KiB `BufWriter<File>`. Phase E widens the
/// value type's inner writer to `Box<dyn Write + Send>` so the same
/// `write_call` body handles plain and gzipped output through one code-path.
pub struct OutputFileMap {
    files: HashMap<OutputKey, (PathBuf, BoxedWriter)>,
    /// Resolved output mode — used by `write_call` to pick the per-mode
    /// key from `(context, strand)` and to dispatch yacht's 8-col row
    /// format.
    mode: OutputMode,
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
    /// When `gzip == true` every writer is wrapped in a `flate2::write::GzEncoder`;
    /// filenames already carry the `.gz` suffix per [`mode_keys`].
    pub fn new(
        output_dir: &Path,
        input_basename: &str,
        no_header: bool,
        mode: OutputMode,
        gzip: bool,
    ) -> Result<Self, std::io::Error> {
        std::fs::create_dir_all(output_dir)?;

        let keys = mode_keys(mode, input_basename, gzip);
        let mut files: HashMap<OutputKey, (PathBuf, BoxedWriter)> =
            HashMap::with_capacity(keys.len());

        for (key, filename) in keys {
            let path = output_dir.join(filename);
            let mut writer = open_writer(&path, gzip)?;
            if !no_header {
                writer.write_all(SPLIT_FILE_HEADER.as_bytes())?;
            }
            files.insert(key, (path, writer));
        }

        Ok(OutputFileMap { files, mode })
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
    /// the routed [`OutputKey`] is somehow missing from the eager-open map
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
        let (_, writer) =
            self.files
                .get_mut(&key)
                .ok_or_else(|| BismarkExtractorError::InternalError {
                    message: format!(
                        "OutputFileMap missing key {:?} for mode {:?} — \
                         eager-open should have created every key from mode_keys",
                        key, self.mode,
                    ),
                })?;

        if self.mode == OutputMode::Yacht {
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
        Ok(())
    }

    /// Flush every writer in the map. Called from `ExtractState::finalize`
    /// before the splitting-report is written so any buffered call lines
    /// are on disk before the run terminates. On the empty `MbiasOnly`
    /// map this is a no-op.
    ///
    /// For gzipped writers, `BufWriter::flush` propagates to the inner
    /// `GzEncoder` which writes its trailing gzip footer when the writer
    /// drops (which happens at `cleanup_all` time, or at struct-drop time
    /// for the normal exit path).
    pub fn flush_all(&mut self) -> Result<(), std::io::Error> {
        for (_, writer) in self.files.values_mut() {
            writer.flush()?;
        }
        Ok(())
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
        for (_, (path, writer)) in entries {
            // Explicitly close the writer (and the inner GzEncoder, if any)
            // BEFORE calling `remove_file`. A named `let` binding (even
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

/// Factory: open the per-key writer, dispatching to plain `File` or
/// gzipped `GzEncoder<File>` based on `gzip`.
///
/// Returns the writer already wrapped in an 8-KiB `BufWriter` (matching
/// Phase B's capacity). `Box<dyn Write + Send>` is the inner type to
/// keep the `OutputFileMap::write_call` body branch-free w.r.t. plain-vs-gz.
fn open_writer(path: &Path, gzip: bool) -> Result<BoxedWriter, std::io::Error> {
    let file = File::create(path)?;
    let inner: Box<dyn Write + Send> = if gzip {
        Box::new(GzEncoder::new(file, Compression::default()))
    } else {
        Box::new(file)
    };
    Ok(BufWriter::with_capacity(8 * 1024, inner))
}

/// Per-context counts accumulated during the SE/PE loop. Drives the
/// `_splitting_report.txt` content at finalize time.
#[derive(Debug, Default)]
pub struct SplittingReport {
    /// Total records iterated (SE: one per record; PE: two per pair —
    /// matches Perl `bismark_methylation_extractor:2451`).
    pub records_processed: u64,
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
        self.calls_total = self.calls_total.saturating_add(other.calls_total);
        self.calls_cpg_meth = self.calls_cpg_meth.saturating_add(other.calls_cpg_meth);
        self.calls_cpg_unmeth = self.calls_cpg_unmeth.saturating_add(other.calls_cpg_unmeth);
        self.calls_chg_meth = self.calls_chg_meth.saturating_add(other.calls_chg_meth);
        self.calls_chg_unmeth = self.calls_chg_unmeth.saturating_add(other.calls_chg_unmeth);
        self.calls_chh_meth = self.calls_chh_meth.saturating_add(other.calls_chh_meth);
        self.calls_chh_unmeth = self.calls_chh_unmeth.saturating_add(other.calls_chh_unmeth);
    }
}

/// Write `{output_dir}/{basename}_splitting_report.txt`.
///
/// Phase B emits a Perl-shaped report; byte-equality is a Phase H concern.
/// The shape mirrors Perl's section ordering:
///   - parameter-summary block (input file, optional `--fasta` annotation,
///     etc.)
///   - per-context counts
///   - per-context methylation percentages
pub fn write_splitting_report(
    path: &Path,
    input_path: &Path,
    config: &ResolvedConfig,
    report: &SplittingReport,
) -> Result<(), std::io::Error> {
    let mut w = BufWriter::with_capacity(8 * 1024, File::create(path)?);

    // Header / parameter summary
    writeln!(
        w,
        "Bismark methylation extractor version {}",
        BISMARK_VERSION
    )?;
    writeln!(w)?;
    writeln!(w, "Input file: {}", input_path.display())?;
    writeln!(w, "Output directory: {}", config.output_dir.display())?;

    // --fasta annotation line (SPEC §3 row 4, Perl line 5040)
    if config.fasta_annotation {
        writeln!(
            w,
            "Genomic equivalent sequences will be printed out in FastA format"
        )?;
    }

    // Trim settings.
    writeln!(w, "--ignore: {}", config.ignore_5p_r1)?;
    writeln!(w, "--ignore_3prime: {}", config.ignore_3p_r1)?;

    writeln!(w)?;
    // Perl `bismark_methylation_extractor:2479` writes "Processed N lines in total"
    // (where N is the BAM-line count — SE: records, PE: 2×pairs). Phase C bug-fix.
    writeln!(w, "Processed {} lines in total", report.records_processed)?;
    writeln!(w)?;

    writeln!(w, "Total number of C's analysed:\t{}", report.calls_total)?;
    writeln!(w)?;

    writeln!(
        w,
        "Total methylated C's in CpG context:\t{}",
        report.calls_cpg_meth
    )?;
    writeln!(
        w,
        "Total unmethylated C's in CpG context:\t{}",
        report.calls_cpg_unmeth
    )?;
    writeln!(
        w,
        "Total methylated C's in CHG context:\t{}",
        report.calls_chg_meth
    )?;
    writeln!(
        w,
        "Total unmethylated C's in CHG context:\t{}",
        report.calls_chg_unmeth
    )?;
    writeln!(
        w,
        "Total methylated C's in CHH context:\t{}",
        report.calls_chh_meth
    )?;
    writeln!(
        w,
        "Total unmethylated C's in CHH context:\t{}",
        report.calls_chh_unmeth
    )?;
    writeln!(w)?;

    let pct_cpg = SplittingReport::percent_meth(report.calls_cpg_meth, report.calls_cpg_unmeth);
    let pct_chg = SplittingReport::percent_meth(report.calls_chg_meth, report.calls_chg_unmeth);
    let pct_chh = SplittingReport::percent_meth(report.calls_chh_meth, report.calls_chh_unmeth);
    writeln!(w, "C methylated in CpG context:\t{:.2}%", pct_cpg)?;
    writeln!(w, "C methylated in CHG context:\t{:.2}%", pct_chg)?;
    writeln!(w, "C methylated in CHH context:\t{:.2}%", pct_chh)?;

    w.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitting_report_add_is_commutative() {
        let a = SplittingReport {
            records_processed: 100,
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
        assert_eq!(a_into_b.calls_total, b_into_a.calls_total);
        assert_eq!(a_into_b.calls_cpg_meth, b_into_a.calls_cpg_meth);
        assert_eq!(a_into_b.calls_cpg_unmeth, b_into_a.calls_cpg_unmeth);
        assert_eq!(a_into_b.calls_chg_meth, b_into_a.calls_chg_meth);
        assert_eq!(a_into_b.calls_chg_unmeth, b_into_a.calls_chg_unmeth);
        assert_eq!(a_into_b.calls_chh_meth, b_into_a.calls_chh_meth);
        assert_eq!(a_into_b.calls_chh_unmeth, b_into_a.calls_chh_unmeth);
        // Sanity sums:
        assert_eq!(a_into_b.records_processed, 350);
        assert_eq!(a_into_b.calls_total, 1750);
    }
}

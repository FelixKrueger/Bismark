//! Per-(context × strand) split-file map + splitting-report writer.
//!
//! Phase B (rev 1): **eager-open** all 12 strand×context files at
//! [`OutputFileMap::new`] time and write the version header immediately,
//! matching Perl `bismark_methylation_extractor` lines 5405-5700+ (default
//! mode) and 5140-5325 (`--merge_non_CpG` mode). Rev 0's lazy-creation
//! design codified the wrong byte-identity invariant — see plan rev 1
//! changelog.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use bismark_io::BismarkStrand;

use crate::call::{CytosineContext, MethCall};
use crate::cli::ResolvedConfig;
use crate::error::BismarkExtractorError;

/// Bismark version string. Hardcoded to lock byte-identity with Perl's
/// `$version` variable. Update in lockstep with Perl `bismark_methylation_extractor`
/// at release time.
pub const BISMARK_VERSION: &str = "v0.25.1";

/// The literal header line Perl writes as the first line of every split
/// file (when `!--no_header && !--mbias_only`). Verified at Perl lines
/// 5159, 5182, 5205, 5228, 5429, 5452, 5475, 5498, etc.
pub const SPLIT_FILE_HEADER: &str = "Bismark methylation extractor version v0.25.1\n";

/// Key into the 12-element output-file map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct OutputKey {
    /// CpG / CHG / CHH.
    pub context: CytosineContext,
    /// OT / CTOT / CTOB / OB.
    pub strand: BismarkStrand,
}

/// All 12 (context, strand) keys for `OutputMode::Default` SE/PE. Order
/// follows Perl's file-open order: per-context block, then OT/CTOT/CTOB/OB
/// within each. (Per-file content is independent of this order; the order
/// only matters for any error-message stability and for the unit test.)
const DEFAULT_KEYS: [(CytosineContext, BismarkStrand); 12] = [
    (CytosineContext::CpG, BismarkStrand::OT),
    (CytosineContext::CpG, BismarkStrand::CTOT),
    (CytosineContext::CpG, BismarkStrand::CTOB),
    (CytosineContext::CpG, BismarkStrand::OB),
    (CytosineContext::CHG, BismarkStrand::OT),
    (CytosineContext::CHG, BismarkStrand::CTOT),
    (CytosineContext::CHG, BismarkStrand::CTOB),
    (CytosineContext::CHG, BismarkStrand::OB),
    (CytosineContext::CHH, BismarkStrand::OT),
    (CytosineContext::CHH, BismarkStrand::CTOT),
    (CytosineContext::CHH, BismarkStrand::CTOB),
    (CytosineContext::CHH, BismarkStrand::OB),
];

/// Eagerly-opened per-(context, strand) split files.
///
/// Rev 1 layout: one map keyed by [`OutputKey`] storing both the path
/// (for cleanup) and an 8-KiB `BufWriter<File>`. Combining the two
/// removes the rev-0 risk that `fhs` and `paths` could drift apart.
pub struct OutputFileMap {
    files: HashMap<OutputKey, (PathBuf, BufWriter<File>)>,
}

impl OutputFileMap {
    /// Eagerly open all 12 default-mode split files in `output_dir`.
    ///
    /// Writes the version header line to each file unless `no_header == true`.
    /// Creates `output_dir` via `create_dir_all` if missing (matches Perl
    /// `make_path` behaviour).
    pub fn new(
        output_dir: &Path,
        input_basename: &str,
        no_header: bool,
    ) -> Result<Self, std::io::Error> {
        std::fs::create_dir_all(output_dir)?;

        let mut files: HashMap<OutputKey, (PathBuf, BufWriter<File>)> =
            HashMap::with_capacity(DEFAULT_KEYS.len());

        for (context, strand) in DEFAULT_KEYS {
            let filename = format!(
                "{ctx}_{strand}_{base}.txt",
                ctx = context_prefix(context),
                strand = strand_label(strand),
                base = input_basename,
            );
            let path = output_dir.join(filename);
            let file = File::create(&path)?;
            let mut writer = BufWriter::with_capacity(8 * 1024, file);
            if !no_header {
                writer.write_all(SPLIT_FILE_HEADER.as_bytes())?;
            }
            files.insert(OutputKey { context, strand }, (path, writer));
        }

        Ok(OutputFileMap { files })
    }

    /// Append a `MethCall` line to the appropriate split file.
    ///
    /// `record_name` is the raw QNAME bytes from the BAM (used verbatim in
    /// the output line — Bismark QNAMEs are ASCII in practice).
    ///
    /// # Output format
    ///
    /// Tab-separated row matching Perl 2911-2961:
    /// ```text
    /// read_id<TAB>meth_char<TAB>chr<TAB>ref_pos<TAB>xm_byte<LF>
    /// ```
    /// where `meth_char` is `+` for methylated calls (uppercase XM) and `-`
    /// for unmethylated calls (lowercase XM). **It is a methylation-state
    /// indicator, not a strand indicator** (Reviewer A L1 fix in rev 2 —
    /// the original "strand char" labelling was a Perl-fidelity nit).
    ///
    /// # Errors
    ///
    /// `BismarkExtractorError::IoWrite` on I/O failures. `InternalError` if
    /// the `(context, strand)` key is somehow missing from the eager-open
    /// map — should not be possible because [`OutputFileMap::new`] inserts
    /// all 12 (3 contexts × 4 strands) keys at construction time, but
    /// surfaces loudly rather than panicking if it ever happens.
    pub fn write_call(
        &mut self,
        record_name: &[u8],
        chr: &str,
        call: MethCall,
        strand: BismarkStrand,
    ) -> Result<(), BismarkExtractorError> {
        let key = OutputKey {
            context: call.context,
            strand,
        };
        let (_, writer) =
            self.files
                .get_mut(&key)
                .ok_or_else(|| BismarkExtractorError::InternalError {
                    message: format!(
                        "OutputFileMap missing key (context={:?}, strand={:?}) — \
                     eager-open should have created all 12 keys at OutputFileMap::new time",
                        call.context, strand,
                    ),
                })?;
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
        Ok(())
    }

    /// Flush all 12 writers. Called from `ExtractState::finalize` before
    /// the splitting-report is written so any buffered call lines are on
    /// disk before the run terminates.
    pub fn flush_all(&mut self) -> Result<(), std::io::Error> {
        for (_, writer) in self.files.values_mut() {
            writer.flush()?;
        }
        Ok(())
    }

    /// Drop all writers + best-effort remove every file. Called from
    /// `extract_se`'s pre-finalize error paths. One failed `remove_file`
    /// doesn't prevent the others — we log via `eprintln!` and continue.
    pub fn cleanup_all(&mut self) {
        // Drain into a vec to avoid double-borrow.
        let entries: Vec<_> = self.files.drain().collect();
        for (_, (path, _writer)) in entries {
            // `_writer` drops here; file handle closes.
            if let Err(e) = std::fs::remove_file(&path) {
                // Don't propagate — this is best-effort cleanup. Print so
                // an operator can see what went wrong without us hiding it.
                eprintln!(
                    "warning: failed to remove partial output file {}: {}",
                    path.display(),
                    e
                );
            }
        }
    }
}

/// Per-context counts accumulated during the SE/PE loop. Drives the
/// `_splitting_report.txt` content at finalize time.
#[derive(Debug, Default)]
pub struct SplittingReport {
    /// Total records iterated (SE: one per record; PE: one per pair —
    /// PE arrives in Phase C).
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

/// Strand label used in output filenames.
fn strand_label(strand: BismarkStrand) -> &'static str {
    match strand {
        BismarkStrand::OT => "OT",
        BismarkStrand::CTOT => "CTOT",
        BismarkStrand::CTOB => "CTOB",
        BismarkStrand::OB => "OB",
    }
}

/// Context prefix used in output filenames.
fn context_prefix(context: CytosineContext) -> &'static str {
    match context {
        CytosineContext::CpG => "CpG",
        CytosineContext::CHG => "CHG",
        CytosineContext::CHH => "CHH",
    }
}

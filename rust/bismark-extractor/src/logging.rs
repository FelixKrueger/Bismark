//! Console diagnostics — a STDERR logger gated by `--quiet`, plus the pure
//! text builders for the startup banner, parameter summary, header provenance,
//! progress counter, and final methylation summary.
//!
//! **All diagnostic output goes to STDERR**, matching Perl Bismark's `warn`
//! model (the Perl extractor uses `warn` for every progress/summary line; 0
//! `print STDOUT`) and the pre-existing `eprintln!` sites in `output.rs`.
//! stdout stays clean (only `--version` writes there, in `main.rs`).
//!
//! The text builders (`header_provenance_lines`, `parameters_text`,
//! `final_summary_text`) are pure (return `String`/`Vec<String>`) so the
//! `--quiet` gate and the formatting are unit-testable without capturing
//! a real stderr.

use crate::cli::{PairedMode, ResolvedConfig};
use crate::output::SplittingReport;
use noodles_sam::Header;
use std::io::Write;

/// STDERR diagnostics logger. `Copy` so it can be handed to the producer
/// thread cheaply.
#[derive(Debug, Clone, Copy)]
pub struct Logger {
    quiet: bool,
    verbose: bool,
}

impl Logger {
    /// Construct a logger from explicit flags.
    #[must_use]
    pub fn new(quiet: bool, verbose: bool) -> Self {
        Self { quiet, verbose }
    }

    /// Construct a logger from the resolved CLI config (`--quiet`/`--verbose`).
    #[must_use]
    pub fn from_config(config: &ResolvedConfig) -> Self {
        Self::new(config.quiet, config.verbose)
    }

    /// Whether `@SQ` reference-dictionary lines are included in provenance.
    #[must_use]
    pub fn verbose(&self) -> bool {
        self.verbose
    }

    /// Write an informational string to `w` unless `--quiet`. Returns whether
    /// it wrote — the seam the unit tests use to assert the quiet gate without
    /// touching a real stderr.
    pub fn info_to<W: Write>(&self, w: &mut W, s: &str) -> bool {
        if self.quiet {
            return false;
        }
        let _ = w.write_all(s.as_bytes());
        true
    }

    /// General gated informational line: write `s` + newline to stderr unless
    /// `--quiet`. For ad-hoc lines like the empty-sweep `kept`/`deleted` log.
    pub fn note(&self, s: &str) {
        self.info(&format!("{s}\n"));
    }

    /// Informational string → stderr unless `--quiet`.
    fn info(&self, s: &str) {
        if self.quiet {
            return;
        }
        let mut err = std::io::stderr().lock();
        let _ = err.write_all(s.as_bytes());
    }

    /// Startup banner — the SUITE version (matches `--version`), NOT the
    /// v0.25.1-locked `BISMARK_VERSION` used for output-file headers.
    pub fn banner(&self) {
        self.info(&format!(
            "\n*** Bismark methylation extractor (Rust port) version {} ***\n\n",
            bismark_meta::SUITE_VERSION
        ));
    }

    /// SE/PE mode line + parameter summary.
    pub fn parameters(&self, config: &ResolvedConfig, is_paired: bool) {
        self.info(&parameters_text(config, is_paired));
    }

    /// `@HD` + `@PG` header provenance (`@SQ` only when `--verbose`).
    pub fn header_provenance(&self, header: &Header) {
        let lines = header_provenance_lines(header, self.verbose);
        if lines.is_empty() {
            return;
        }
        let mut s = String::from("Alignment provenance (from BAM/SAM header):\n");
        for l in &lines {
            s.push_str(l);
            s.push('\n');
        }
        s.push('\n');
        self.info(&s);
    }

    /// `Processed lines: N` progress tick.
    pub fn progress(&self, lines: u64) {
        self.info(&format!("Processed lines: {lines}\n"));
    }

    /// Final per-context methylation summary (mirror of `_splitting_report.txt`).
    pub fn final_summary(&self, report: &SplittingReport) {
        self.info(&final_summary_text(report));
    }
}

/// `@HD`/`@PG` (and, when `include_sq`, `@SQ`) header lines, serialized to
/// their on-disk SAM text form. Reuses the version-robust idiom from
/// `bismark_io::detect_paired_from_header`: serialize via
/// `noodles_sam::io::Writer` and filter the text lines, rather than walking the
/// in-memory `Map<Program>` (which can reorder tags). The `@SQ` reference
/// dictionary (190+ contigs on a human genome) is the noise we drop by default.
#[must_use]
pub fn header_provenance_lines(header: &Header, include_sq: bool) -> Vec<String> {
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut writer = noodles_sam::io::Writer::new(&mut buf);
        if writer.write_header(header).is_err() {
            return Vec::new();
        }
    }
    filter_header_text(&String::from_utf8_lossy(&buf), include_sq)
}

/// Filter serialized SAM-header text to provenance lines: keep everything
/// except the `@SQ` reference dictionary, unless `include_sq`. Split out as a
/// pure function so it's unit-testable on a literal header string.
#[must_use]
fn filter_header_text(text: &str, include_sq: bool) -> Vec<String> {
    text.lines()
        .filter(|line| include_sq || !line.starts_with("@SQ"))
        .map(std::string::ToString::to_string)
        .collect()
}

/// Parameter-summary block (Perl `bismark_methylation_extractor:54`-style).
#[must_use]
pub fn parameters_text(config: &ResolvedConfig, is_paired: bool) -> String {
    let lib = if is_paired {
        "paired-end"
    } else {
        "single-end"
    };
    // Mode-detection source (Perl `:1172`/`:1177` parenthetical): AutoDetect
    // resolved from the @PG line, vs explicitly forced with -s/-p.
    let source = match config.paired_mode {
        PairedMode::AutoDetect => "auto-detected from @PG line",
        PairedMode::SingleEnd | PairedMode::PairedEnd => "specified via -s/-p",
    };
    let mut s = String::new();
    s.push_str(&format!("Treating file(s) as {lib} data ({source})\n\n"));
    s.push_str("Summarising Bismark methylation extractor parameters:\n");
    s.push_str("=======================================================\n");
    s.push_str(&format!("Bismark {lib} format specified\n"));
    s.push_str(&format!(
        "Number of parallel workers: {}\n",
        config.parallel.max(1)
    ));
    s.push_str(&format!(
        "Output will be written to: {}\n",
        config.output_dir.display()
    ));
    if config.ignore_5p_r1 > 0 {
        s.push_str(&format!(
            "Ignoring first {} bp from the 5' end of Read 1\n",
            config.ignore_5p_r1
        ));
    }
    if config.ignore_3p_r1 > 0 {
        s.push_str(&format!(
            "Ignoring last {} bp from the 3' end of Read 1\n",
            config.ignore_3p_r1
        ));
    }
    if is_paired && config.ignore_5p_r2 > 0 {
        s.push_str(&format!(
            "Ignoring first {} bp from the 5' end of Read 2\n",
            config.ignore_5p_r2
        ));
    }
    if is_paired && config.ignore_3p_r2 > 0 {
        s.push_str(&format!(
            "Ignoring last {} bp from the 3' end of Read 2\n",
            config.ignore_3p_r2
        ));
    }
    if is_paired {
        if config.no_overlap {
            s.push_str("Overlapping paired-end calls will be counted once (--no_overlap)\n");
        } else {
            s.push_str("Overlapping paired-end calls will be counted twice (--include_overlap)\n");
        }
    }
    if config.gzip {
        s.push_str("Output files will be GZIP compressed (.gz)\n");
    }
    s.push('\n');
    s
}

/// Final methylation summary (mirror of the `_splitting_report.txt` numbers,
/// Perl `:2480`-`:2521` warned to stderr). Built from the same
/// [`SplittingReport`] that drives the file.
#[must_use]
pub fn final_summary_text(report: &SplittingReport) -> String {
    let cpg = SplittingReport::percent_meth(report.calls_cpg_meth, report.calls_cpg_unmeth);
    let chg = SplittingReport::percent_meth(report.calls_chg_meth, report.calls_chg_unmeth);
    let chh = SplittingReport::percent_meth(report.calls_chh_meth, report.calls_chh_unmeth);
    format!(
        "\nProcessed {lines} lines in total\n\
         Total number of methylation call strings processed: {call_strings}\n\n\
         Final Cytosine Methylation Report\n\
         =================================\n\
         Total number of C's analysed:\t{total}\n\n\
         Total methylated C's in CpG context:\t{cpg_m}\n\
         Total methylated C's in CHG context:\t{chg_m}\n\
         Total methylated C's in CHH context:\t{chh_m}\n\n\
         Total C to T conversions in CpG context:\t{cpg_u}\n\
         Total C to T conversions in CHG context:\t{chg_u}\n\
         Total C to T conversions in CHH context:\t{chh_u}\n\n\
         C methylated in CpG context:\t{cpg:.1}%\n\
         C methylated in CHG context:\t{chg:.1}%\n\
         C methylated in CHH context:\t{chh:.1}%\n\n",
        // `Processed N lines in total` mirrors the file line at
        // `output.rs:683` (Perl `:2482` `$sequences_count`) = records_processed
        // (= pairs for PE). The call-strings line is the separate 2×pairs
        // counter. For SE both are equal; they diverge for PE.
        lines = report.records_processed,
        call_strings = report.call_strings_processed,
        total = report.calls_total,
        cpg_m = report.calls_cpg_meth,
        chg_m = report.calls_chg_meth,
        chh_m = report.calls_chh_meth,
        cpg_u = report.calls_cpg_unmeth,
        chg_u = report.calls_chg_unmeth,
        chh_u = report.calls_chh_unmeth,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_gate_suppresses_info_but_returns_false() {
        let quiet = Logger::new(true, false);
        let loud = Logger::new(false, false);
        let mut buf_q: Vec<u8> = Vec::new();
        let mut buf_l: Vec<u8> = Vec::new();
        assert!(!quiet.info_to(&mut buf_q, "hello\n"));
        assert!(buf_q.is_empty(), "quiet must write nothing");
        assert!(loud.info_to(&mut buf_l, "hello\n"));
        assert_eq!(buf_l, b"hello\n");
    }

    #[test]
    fn final_summary_matches_perl_shape_and_percent() {
        let r = SplittingReport {
            // PE: `records_processed` (pairs) and `call_strings_processed`
            // (2×pairs) differ — the two report lines must use the right
            // counter each (mirrors the file at output.rs:683/690).
            records_processed: 4_250_754,
            call_strings_processed: 8_501_508,
            calls_total: 100,
            calls_cpg_meth: 818,
            calls_cpg_unmeth: 182, // 818/1000 = 81.8%
            ..SplittingReport::default()
        };
        let text = final_summary_text(&r);
        assert!(text.contains("Processed 4250754 lines in total"));
        assert!(text.contains("Total number of methylation call strings processed: 8501508"));
        assert!(text.contains("Total number of C's analysed:\t100"));
        assert!(text.contains("C methylated in CpG context:\t81.8%"));
    }

    #[test]
    fn provenance_drops_sq_by_default_keeps_hd_pg() {
        let header = "@HD\tVN:1.0\tSO:unsorted\n\
                      @SQ\tSN:1\tLN:248956422\n\
                      @SQ\tSN:MT\tLN:16569\n\
                      @PG\tID:Bismark\tVN:v0.25.1\tCL:\"bismark --genome g/ r.fq.gz\"\n";
        let default = filter_header_text(header, false);
        assert_eq!(default.len(), 2, "default must drop the 2 @SQ lines");
        assert!(default[0].starts_with("@HD"));
        assert!(default[1].starts_with("@PG") && default[1].contains("ID:Bismark"));
        let verbose = filter_header_text(header, true);
        assert_eq!(verbose.len(), 4, "--verbose keeps @SQ too");
    }
}

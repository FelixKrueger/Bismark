//! Deduplication report formatting.
//!
//! [`DedupReport::format`] produces a byte-equal-to-Perl dedup report.
//! The format below was verified character-by-character against Perl's
//! `deduplicate_bismark` lines 529–537:
//!
//! ```text
//! \n
//! Total number of alignments analysed in <input_path>:\t<count>\n
//! Total number duplicated alignments removed:\t<removed> (<pct_removed>%)\n
//! Duplicated alignments were found at:\t<n_positions> different position(s)\n
//! \n
//! Total count of deduplicated leftover sequences: <leftover> (<pct_leftover>% of total)\n
//! \n
//! ```
//!
//! Percentages use `sprintf("%.2f", ...)` formatting; `N/A` when
//! `count == 0`.
//!
//! `<input_path>` is the input filename **as supplied on the CLI** —
//! Perl echoes `$ARGV[i]` verbatim. The byte-identity test in Phase F
//! must therefore invoke the Rust binary with the same path string the
//! Perl baseline was generated with.

use std::fmt::Write as _;
use std::path::Path;

/// A render-ready deduplication report.
///
/// Construct via [`crate::dedup::dedup::DedupState::into_report`].
#[derive(Debug, Clone)]
pub struct DedupReport {
    file_label: String,
    count: u64,
    removed: u64,
    n_positions: usize,
    umi_mode: bool,
}

impl DedupReport {
    /// Construct a report.
    ///
    /// Crate-private intentionally: the only legitimate construction path
    /// is via [`crate::dedup::dedup::DedupState::into_report`] or
    /// [`crate::dedup::dedup::UmiDedupState::into_report`], both of which
    /// guarantee `removed <= count` (preventing an underflow in
    /// [`DedupReport::leftover`]). Callers outside this crate should use
    /// the `*DedupState` paths.
    ///
    /// `umi_mode = true` appends Perl's ` (UMI mode)` banner suffix
    /// after the input filename in the first analysed-alignments line
    /// (matches `deduplicate_bismark:908`). Position-only callers pass
    /// `false`.
    #[must_use]
    pub(crate) fn new(
        file_label: String,
        count: u64,
        removed: u64,
        n_positions: usize,
        umi_mode: bool,
    ) -> Self {
        Self {
            file_label,
            count,
            removed,
            n_positions,
            umi_mode,
        }
    }

    /// Total alignment records / pairs analysed.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Records flagged as duplicates.
    #[must_use]
    pub fn removed(&self) -> u64 {
        self.removed
    }

    /// Distinct positions at which a duplicate was observed.
    #[must_use]
    pub fn n_positions(&self) -> usize {
        self.n_positions
    }

    /// Records kept after dedup: `count - removed`.
    #[must_use]
    pub fn leftover(&self) -> u64 {
        self.count - self.removed
    }

    /// Render the report to a `String` in Perl-byte-equal **report file**
    /// format (line 908 of `deduplicate_bismark`). In UMI mode this
    /// emits the space-form banner ` (UMI mode)`. For the STDERR
    /// echo (which Perl emits at line 903 with the hyphen-form
    /// `(UMI-mode)`) use [`Self::format_stderr`] instead.
    ///
    /// Returns an owned `String` so callers can write it to a file or
    /// compare against a snapshot without intermediate allocation in
    /// the hot path.
    #[must_use]
    pub fn format(&self) -> String {
        let leftover = self.leftover();
        // count == 0 only happens on a zero-alignment input (header-only BAM).
        // Render `0.00%` rather than computing `0/0` (NaN). Deliberate divergence
        // from Perl, which dies on empty input and so emits no zero-count report
        // — there is no Perl oracle to match here. `0.00%` keeps the percentage
        // field numeric for downstream parsers (e.g. MultiQC). See
        // plans/06132026_dedup-empty-input/PLAN.md (Open Q-2).
        let (pct_removed, pct_leftover) = if self.count == 0 {
            (String::from("0.00"), String::from("0.00"))
        } else {
            let count_f = self.count as f64;
            (
                format!("{:.2}", (self.removed as f64) / count_f * 100.0),
                format!("{:.2}", (leftover as f64) / count_f * 100.0),
            )
        };

        // String::with_capacity for the typical-case length to avoid
        // reallocation on the hot path. ~256 bytes covers all paths.
        let mut s = String::with_capacity(256);
        // Perl `deduplicate_bismark:908` emits the banner as:
        //   "\nTotal number of alignments analysed in $file (UMI mode):\t$count\n"
        // The ` (UMI mode)` suffix (space-form) is appended after the
        // filename when UMI mode is engaged. Position-only emits no suffix.
        let umi_suffix = if self.umi_mode { " (UMI mode)" } else { "" };
        writeln!(
            s,
            "\nTotal number of alignments analysed in {}{}:\t{}",
            self.file_label, umi_suffix, self.count
        )
        .expect("write to String never fails");
        writeln!(
            s,
            "Total number duplicated alignments removed:\t{} ({}%)",
            self.removed, pct_removed
        )
        .expect("write to String never fails");
        writeln!(
            s,
            "Duplicated alignments were found at:\t{} different position(s)\n",
            self.n_positions
        )
        .expect("write to String never fails");
        writeln!(
            s,
            "Total count of deduplicated leftover sequences: {} ({}% of total)\n",
            leftover, pct_leftover
        )
        .expect("write to String never fails");
        s
    }

    /// Render the report in Perl-byte-equal **STDERR** format (line 903
    /// of `deduplicate_bismark`). In UMI mode this emits the
    /// **hyphen-form** banner ` (UMI-mode)` — distinct from the
    /// space-form ` (UMI mode)` used in the report file. In non-UMI
    /// mode it produces the same bytes as [`Self::format`].
    ///
    /// Per Reviewer A's C2-B finding: Perl emits the per-file summary
    /// to STDERR with hyphen and to the report file with space; the
    /// existing `format()` was being echoed to BOTH, so STDERR got
    /// the wrong form.
    #[must_use]
    pub fn format_stderr(&self) -> String {
        if self.umi_mode {
            // Substitute the report-file space banner for the STDERR
            // hyphen banner. Cheaper than re-implementing the formatter.
            self.format().replace(" (UMI mode):", " (UMI-mode):")
        } else {
            self.format()
        }
    }

    /// Write the rendered report to a file path.
    ///
    /// # Errors
    /// Returns `std::io::Error` if the file cannot be created or written.
    pub fn write_to(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, self.format())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected Perl-byte-equal output for a 10-record / 2-duplicates fixture.
    /// Derived character-by-character from Perl `deduplicate_bismark`
    /// lines 529–537 + Perl `sprintf("%.2f", ...)` semantics.
    const EXPECTED_TYPICAL: &str = "\nTotal number of alignments analysed in /path/sample.bam:\t10\n\
        Total number duplicated alignments removed:\t2 (20.00%)\n\
        Duplicated alignments were found at:\t1 different position(s)\n\n\
        Total count of deduplicated leftover sequences: 8 (80.00% of total)\n\n";

    #[test]
    fn format_matches_perl_byte_for_byte_typical_case() {
        let r = DedupReport::new("/path/sample.bam".to_string(), 10, 2, 1, false);
        assert_eq!(r.format(), EXPECTED_TYPICAL);
    }

    /// Zero-alignment input (header-only BAM) renders `0 (0.00%)` — NOT `N/A%`
    /// (rev 1, plans/06132026_dedup-empty-input). Deliberate divergence from
    /// Perl (which dies on empty input). Keeps the percent field numeric for
    /// downstream parsers (MultiQC).
    #[test]
    fn format_renders_zero_pct_when_count_is_zero() {
        let r = DedupReport::new("/path/empty.bam".to_string(), 0, 0, 0, false);
        let expected = "\nTotal number of alignments analysed in /path/empty.bam:\t0\n\
            Total number duplicated alignments removed:\t0 (0.00%)\n\
            Duplicated alignments were found at:\t0 different position(s)\n\n\
            Total count of deduplicated leftover sequences: 0 (0.00% of total)\n\n";
        assert_eq!(r.format(), expected);
    }

    #[test]
    fn format_removed_zero_no_duplicates() {
        let r = DedupReport::new("/path/clean.bam".to_string(), 100, 0, 0, false);
        let expected = "\nTotal number of alignments analysed in /path/clean.bam:\t100\n\
            Total number duplicated alignments removed:\t0 (0.00%)\n\
            Duplicated alignments were found at:\t0 different position(s)\n\n\
            Total count of deduplicated leftover sequences: 100 (100.00% of total)\n\n";
        assert_eq!(r.format(), expected);
    }

    #[test]
    fn format_real_data_10m_dataset_numbers() {
        // The exact numbers we expect from the 10M PE WGBS audit dataset
        // (PLAN.md §10.4): count=8,592,524, removed=622,892, leftover=7,969,632,
        // n_positions=571,488. Percent rounding: 622892/8592524*100 = 7.249847...
        // sprintf("%.2f",...) → "7.25". 7969632/8592524*100 = 92.750152 → "92.75".
        let r = DedupReport::new(
            "/Users/fkrueger/Desktop/TrimG_Bismark_test/profiling/SRR24827378_10M_R1_val_1_bismark_bt2_pe.bam".to_string(),
            8_592_524,
            622_892,
            571_488,
            false,
        );
        let formatted = r.format();
        assert!(
            formatted.contains("\t8592524\n"),
            "count without comma grouping"
        );
        assert!(
            formatted.contains("\t622892 (7.25%)\n"),
            "removed and percent"
        );
        assert!(
            formatted.contains("\t571488 different position(s)\n"),
            "n_positions"
        );
        assert!(
            formatted.contains(": 7969632 (92.75% of total)\n"),
            "leftover and percent"
        );
    }

    #[test]
    fn leftover_arithmetic() {
        let r = DedupReport::new("x".to_string(), 100, 30, 5, false);
        assert_eq!(r.leftover(), 70);
    }

    #[test]
    fn percent_rounds_to_two_decimal_places_via_sprintf_semantics() {
        // 1 dup in 3 records → 33.33333...% removed; sprintf("%.2f") → "33.33"
        let r = DedupReport::new("x".to_string(), 3, 1, 1, false);
        let s = r.format();
        assert!(s.contains("(33.33%)"), "got: {s}");
        assert!(s.contains("(66.67% of total)"), "got: {s}");
    }

    /// Phase B (v1.2 UMI): byte-precision check vs Perl `(UMI mode)` banner.
    /// Pins the EXACT bytes (space-form, line 908 of deduplicate_bismark)
    /// so any drift fails the build.
    #[test]
    fn format_with_umi_mode_emits_byte_precise_banner_at_line_908() {
        let r = DedupReport::new("/path/sample.bam".to_string(), 10, 2, 1, true);
        let expected = "\nTotal number of alignments analysed in /path/sample.bam (UMI mode):\t10\n\
            Total number duplicated alignments removed:\t2 (20.00%)\n\
            Duplicated alignments were found at:\t1 different position(s)\n\n\
            Total count of deduplicated leftover sequences: 8 (80.00% of total)\n\n";
        assert_eq!(r.format(), expected);
    }

    /// Per Reviewer A C2-B / Reviewer B H1: `format_stderr` in UMI mode
    /// emits the HYPHEN-form `(UMI-mode)` banner — distinct from the
    /// space-form used in the report file. Locks Perl line 903 vs 908.
    #[test]
    fn format_stderr_with_umi_mode_emits_hyphen_form_banner_at_line_903() {
        let r = DedupReport::new("/path/sample.bam".to_string(), 10, 2, 1, true);
        let expected = "\nTotal number of alignments analysed in /path/sample.bam (UMI-mode):\t10\n\
            Total number duplicated alignments removed:\t2 (20.00%)\n\
            Duplicated alignments were found at:\t1 different position(s)\n\n\
            Total count of deduplicated leftover sequences: 8 (80.00% of total)\n\n";
        assert_eq!(r.format_stderr(), expected);
    }

    #[test]
    fn format_stderr_non_umi_mode_identical_to_format() {
        let r = DedupReport::new("/path/sample.bam".to_string(), 10, 2, 1, false);
        assert_eq!(r.format_stderr(), r.format());
    }

    #[test]
    fn format_without_umi_mode_omits_banner() {
        // Position-only path must NOT contain `(UMI mode)` — regression
        // guard against accidentally enabling the suffix in non-UMI workflows.
        let r = DedupReport::new("/path/sample.bam".to_string(), 10, 2, 1, false);
        let out = r.format();
        assert!(
            !out.contains("UMI mode"),
            "non-UMI dedup report must not contain `UMI mode`, got: {out}"
        );
    }

    #[test]
    fn write_to_creates_and_writes_file() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.deduplication_report.txt");
        let r = DedupReport::new("/path/sample.bam".to_string(), 10, 2, 1, false);
        r.write_to(&path).unwrap();
        let read_back = std::fs::read_to_string(&path).unwrap();
        assert_eq!(read_back, EXPECTED_TYPICAL);
    }
}

//! Consistency-report formatting — byte-equal to Perl `methylation_consistency`.
//!
//! The templates below were copied from Perl lines 334–344 and
//! **byte-validated against a real Perl run** (Spike 2, `spikes/RESULTS.md`).
//! Exact layout (`\t` = tab, separator = exactly 49 hyphens, no leading `\n`,
//! no trailing blank line):
//!
//! ```text
//! Total <type> records     -\t<total>\n
//! -------------------------------------------------\n
//! All methylated    [ >= <upper>% ] -\t<all_meth> (<perc>%)\n
//! All unmethylated  [ <= <lower>% ] -\t<all_unmeth> (<perc>%)\n
//! Mixed methylation [ <lower>-<upper>% ] -\t<mixed> (<perc>%)\n
//! Too few CpGs   [min-count <min>] -\t<discarded> (<perc>%)\n
//! ```
//!
//! `<type>` is `paired-end` | `single-end`; the last line says `Too few CHHs`
//! in `--chh` mode. Percentages use `sprintf("%.2f", bucket/total*100)`, or
//! the literal `N/A` when the grand total is 0 (rendered as `(N/A%)`).

use std::fmt::Write as _;

/// The report separator — exactly 49 hyphens (Perl line 335). Guarded by a
/// dedicated unit test so a miscount can never slip through.
const SEPARATOR: &str = "-------------------------------------------------";

/// Per-bucket record counts accumulated over a file. For paired-end input
/// each counter increments **once per pair** (Perl increments per loop
/// iteration, and one iteration consumes a pair).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tally {
    /// Reads/pairs routed to the all-methylated bucket.
    pub all_meth: u64,
    /// Reads/pairs routed to the all-unmethylated bucket.
    pub all_unmeth: u64,
    /// Reads/pairs routed to the mixed bucket.
    pub mixed: u64,
    /// Reads/pairs discarded for having fewer than `min_count` calls.
    pub discarded: u64,
}

impl Tally {
    /// Grand total = sum of the four buckets (the report's "Total … records"
    /// figure). Perl line 310's denominator.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.all_meth + self.all_unmeth + self.mixed + self.discarded
    }

    /// Increment the counter for a routed bucket (one read, or one PE pair).
    pub fn record(&mut self, bucket: crate::methylation_consistency::classify::Bucket) {
        match bucket {
            crate::methylation_consistency::classify::Bucket::AllMeth => self.all_meth += 1,
            crate::methylation_consistency::classify::Bucket::AllUnmeth => self.all_unmeth += 1,
            crate::methylation_consistency::classify::Bucket::Mixed => self.mixed += 1,
        }
    }

    /// Render the consistency report to a byte-equal-to-Perl `String`.
    ///
    /// `paired` selects the `paired-end`/`single-end` word; `chh` selects the
    /// `Too few CHHs`/`Too few CpGs` label. `lower`/`upper`/`min_count` are
    /// echoed into the threshold annotations.
    #[must_use]
    pub fn render(
        &self,
        paired: bool,
        lower: i64,
        upper: i64,
        min_count: u32,
        chh: bool,
    ) -> String {
        let total = self.total();
        let type_str = if paired { "paired-end" } else { "single-end" };
        let too_few_label = if chh { "Too few CHHs" } else { "Too few CpGs" };

        // `sprintf("%.2f", n/total*100)`, or "N/A" when total == 0 (Perl
        // lines 310–318). Op-order pinned to match Perl's `f64` (Spike 1).
        let pct = |n: u64| -> String {
            if total == 0 {
                String::from("N/A")
            } else {
                format!("{:.2}", n as f64 / total as f64 * 100.0)
            }
        };

        let mut s = String::with_capacity(256);
        writeln!(s, "Total {type_str} records     -\t{total}").unwrap();
        writeln!(s, "{SEPARATOR}").unwrap();
        writeln!(
            s,
            "All methylated    [ >= {upper}% ] -\t{} ({}%)",
            self.all_meth,
            pct(self.all_meth)
        )
        .unwrap();
        writeln!(
            s,
            "All unmethylated  [ <= {lower}% ] -\t{} ({}%)",
            self.all_unmeth,
            pct(self.all_unmeth)
        )
        .unwrap();
        writeln!(
            s,
            "Mixed methylation [ {lower}-{upper}% ] -\t{} ({}%)",
            self.mixed,
            pct(self.mixed)
        )
        .unwrap();
        writeln!(
            s,
            "{too_few_label}   [min-count {min_count}] -\t{} ({}%)",
            self.discarded,
            pct(self.discarded)
        )
        .unwrap();
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separator_is_exactly_49_hyphens() {
        assert_eq!(SEPARATOR.len(), 49);
        assert!(SEPARATOR.bytes().all(|b| b == b'-'));
    }

    /// Byte-exact match against the real Perl run captured in Spike 2
    /// (3 fully-methylated SE reads → all_meth=3, others 0).
    #[test]
    fn render_matches_perl_spike2_single_end() {
        let tally = Tally {
            all_meth: 3,
            all_unmeth: 0,
            mixed: 0,
            discarded: 0,
        };
        let expected = format!(
            "Total single-end records     -\t3\n{SEPARATOR}\n\
             All methylated    [ >= 90% ] -\t3 (100.00%)\n\
             All unmethylated  [ <= 10% ] -\t0 (0.00%)\n\
             Mixed methylation [ 10-90% ] -\t0 (0.00%)\n\
             Too few CpGs   [min-count 5] -\t0 (0.00%)\n"
        );
        assert_eq!(tally.render(false, 10, 90, 5, false), expected);
    }

    #[test]
    fn render_no_leading_newline_no_trailing_blank_line() {
        let tally = Tally::default();
        let out = tally.render(false, 10, 90, 5, false);
        assert!(
            !out.starts_with('\n'),
            "report must NOT start with a blank line"
        );
        assert!(
            out.ends_with("%)\n"),
            "report ends with the last line + one \\n"
        );
        assert!(!out.ends_with("\n\n"), "no trailing blank line");
    }

    #[test]
    fn render_paired_end_word_and_per_pair_total() {
        let tally = Tally {
            all_meth: 2,
            all_unmeth: 3,
            mixed: 4,
            discarded: 1,
        };
        let out = tally.render(true, 10, 90, 5, false);
        assert!(out.starts_with("Total paired-end records     -\t10\n"));
    }

    #[test]
    fn render_na_when_total_zero() {
        let tally = Tally::default();
        let expected = format!(
            "Total single-end records     -\t0\n{SEPARATOR}\n\
             All methylated    [ >= 90% ] -\t0 (N/A%)\n\
             All unmethylated  [ <= 10% ] -\t0 (N/A%)\n\
             Mixed methylation [ 10-90% ] -\t0 (N/A%)\n\
             Too few CpGs   [min-count 5] -\t0 (N/A%)\n"
        );
        assert_eq!(tally.render(false, 10, 90, 5, false), expected);
    }

    #[test]
    fn render_chh_uses_chh_label() {
        let tally = Tally {
            all_meth: 1,
            all_unmeth: 1,
            mixed: 1,
            discarded: 1,
        };
        let out = tally.render(false, 10, 90, 5, true);
        assert!(out.contains("Too few CHHs   [min-count 5] -\t1 (25.00%)\n"));
        assert!(!out.contains("Too few CpGs"));
    }

    #[test]
    fn render_custom_thresholds_and_min_count_echoed() {
        let tally = Tally {
            all_meth: 1,
            all_unmeth: 0,
            mixed: 0,
            discarded: 0,
        };
        let out = tally.render(false, 20, 80, 3, false);
        assert!(out.contains("All methylated    [ >= 80% ] -\t"));
        assert!(out.contains("All unmethylated  [ <= 20% ] -\t"));
        assert!(out.contains("Mixed methylation [ 20-80% ] -\t"));
        assert!(out.contains("[min-count 3] -\t"));
    }

    #[test]
    fn percent_rounds_to_two_decimals() {
        // 1 of 3 → 33.333..% → "33.33"; 2 of 3 → "66.67".
        let tally = Tally {
            all_meth: 1,
            all_unmeth: 2,
            mixed: 0,
            discarded: 0,
        };
        let out = tally.render(false, 10, 90, 5, false);
        assert!(out.contains("-\t1 (33.33%)\n"), "got: {out}");
        assert!(out.contains("-\t2 (66.67%)\n"), "got: {out}");
    }
}

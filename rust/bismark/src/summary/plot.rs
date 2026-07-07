//! Plot-data assembly — the per-sample mutation + plot-exclusion that feed
//! the HTML graphs.
//!
//! Mirrors Perl `bismark2summary:406-456`. Runs AFTER the `.txt` row is
//! captured (§2.6), so it works on **mutated** copies: 0-defaulting of
//! unaligned/ambig/no_seq + the six methylation counts, the
//! `aligned_reads`-blanking when a dedup count is present, and the
//! plot-exclusion `next` for samples with zero calls in any context.
//!
//! `num_samples` is the **total** discovered BAM count (incl. plot-excluded
//! samples — Perl `$num_samples = scalar @bam_files` at `:247`), while the
//! per-array vectors hold only the **plotted** subset. The HTML x-values use
//! `num_samples` while the y-arrays use the (possibly shorter) plotted count
//! — the deliberate length mismatch Perl emits (§2.9 step 6).

use crate::summary::parse::SampleMetrics;

/// The comma-join-ready plot arrays (plotted samples only) plus the total
/// sample count.
#[derive(Debug, Default, Clone)]
pub struct PlotArrays {
    /// Total discovered BAM count (incl. plot-excluded samples).
    pub num_samples: usize,
    /// `'<munged name>'` per plotted sample (single-quote-wrapped).
    pub categories: Vec<String>,
    /// Raw aligned count; **empty** for deduplicated samples (blanked).
    pub aligned: Vec<String>,
    /// Unaligned (0-defaulted).
    pub not_aligned: Vec<String>,
    /// Ambiguously aligned (0-defaulted).
    pub ambig: Vec<String>,
    /// No genomic sequence (0-defaulted).
    pub no_seq: Vec<String>,
    /// Duplicate alignments removed; empty when no dedup report.
    pub dup: Vec<String>,
    /// Unique (deduplicated) alignments; empty when no dedup report.
    pub unique: Vec<String>,
    /// Methylated CpG count (0-defaulted).
    pub meth_cpg: Vec<String>,
    /// Unmethylated CpG count (0-defaulted).
    pub unmeth_cpg: Vec<String>,
    /// Methylated CHG count (0-defaulted).
    pub meth_chg: Vec<String>,
    /// Unmethylated CHG count (0-defaulted).
    pub unmeth_chg: Vec<String>,
    /// Methylated CHH count (0-defaulted).
    pub meth_chh: Vec<String>,
    /// Unmethylated CHH count (0-defaulted).
    pub unmeth_chh: Vec<String>,
}

/// Perl `0`-default: an empty string becomes `"0"` (Perl `$x = 0 if $x eq ''`).
fn default0(s: &str) -> String {
    if s.is_empty() {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// Numeric value of a count string (empty / non-numeric ⇒ 0, matching Perl
/// scalar coercion). Counts can exceed `u32`, so use `i64`.
#[must_use]
pub fn num(s: &str) -> i64 {
    s.parse::<i64>().unwrap_or(0)
}

/// Derive the plot label `$name` from the BAM filename (Perl `:406-410`):
/// strip (in order) `_bismark.bam$` (`.` = any char), `\.fq\.gz$`,
/// `_trimmed$`, `_[12]$`. On modern `*_bismark_bt2.bam` names these are
/// usually no-ops, leaving the full BAM name as the label.
#[must_use]
pub fn munge_name(bam: &str) -> String {
    let mut name = strip_bismark_bam(bam);
    if let Some(s) = name.strip_suffix(".fq.gz") {
        name = s.to_string();
    }
    if let Some(s) = name.strip_suffix("_trimmed") {
        name = s.to_string();
    }
    if name.ends_with("_1") || name.ends_with("_2") {
        name.truncate(name.len() - 2);
    }
    name
}

/// `s/_bismark.bam$//` where the `.` is the regex any-char (matches the
/// literal `.` too). The tail must be `_bismark` + 1 char + `bam`.
fn strip_bismark_bam(name: &str) -> String {
    let b = name.as_bytes();
    let n = b.len();
    if n >= 12 && &b[n - 12..n - 4] == b"_bismark" && &b[n - 3..] == b"bam" {
        // b[n-4] is the `.` wildcard.
        name[..n - 12].to_string()
    } else {
        name.to_string()
    }
}

/// Assemble the plot arrays from the per-sample metrics (Perl `:406-456`).
#[must_use]
pub fn assemble(samples: &[SampleMetrics]) -> PlotArrays {
    let mut a = PlotArrays {
        num_samples: samples.len(),
        ..Default::default()
    };

    for m in samples {
        let name = munge_name(&m.bam);

        let not_aligned = default0(&m.unaligned);
        let ambig = default0(&m.ambig_reads);
        let no_seq = default0(&m.no_seq_reads);
        // `if ($dup_reads ne '') { $aligned_reads = "" }` — blank the raw
        // aligned count when a dedup report was present (NOT 0-defaulted).
        let aligned = if m.dup_reads.is_empty() {
            m.aligned_reads.clone()
        } else {
            String::new()
        };
        // dup / unique are NOT 0-defaulted (stay "" when no dedup report).
        let dup = m.dup_reads.clone();
        let unique = m.unique_reads.clone();

        let meth_cpg = default0(&m.meth_cpg);
        let unmeth_cpg = default0(&m.unmeth_cpg);
        let meth_chg = default0(&m.meth_chg);
        let unmeth_chg = default0(&m.unmeth_chg);
        let meth_chh = default0(&m.meth_chh);
        let unmeth_chh = default0(&m.unmeth_chh);

        // Plot-exclusion: skip (do not push) a sample with zero calls in ANY
        // context. Its `.txt` row was already written (§2.7).
        if num(&meth_cpg) == 0 && num(&unmeth_cpg) == 0 {
            continue;
        }
        if num(&meth_chg) == 0 && num(&unmeth_chg) == 0 {
            continue;
        }
        if num(&meth_chh) == 0 && num(&unmeth_chh) == 0 {
            continue;
        }

        a.categories.push(format!("'{name}'"));
        a.aligned.push(aligned);
        a.not_aligned.push(not_aligned);
        a.ambig.push(ambig);
        a.no_seq.push(no_seq);
        a.dup.push(dup);
        a.unique.push(unique);
        a.meth_cpg.push(meth_cpg);
        a.unmeth_cpg.push(unmeth_cpg);
        a.meth_chg.push(meth_chg);
        a.unmeth_chg.push(unmeth_chg);
        a.meth_chh.push(meth_chh);
        a.unmeth_chh.push(unmeth_chh);
    }

    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(bam: &str) -> SampleMetrics {
        let mut m = SampleMetrics::new(bam);
        // give it nonzero calls in all 3 contexts so it is NOT excluded
        m.meth_cpg = "10".into();
        m.unmeth_cpg = "90".into();
        m.meth_chg = "1".into();
        m.unmeth_chg = "9".into();
        m.meth_chh = "2".into();
        m.unmeth_chh = "8".into();
        m
    }

    #[test]
    fn munge_modern_bt2_name_is_noop() {
        // `_bismark.bam$` does NOT match `_bismark_bt2.bam`.
        assert_eq!(
            munge_name("DRR_x_trimmed.fq.gz_bismark_bt2.bam"),
            "DRR_x_trimmed.fq.gz_bismark_bt2.bam"
        );
    }

    #[test]
    fn munge_legacy_and_trim_suffixes() {
        // Old `_bismark.bam` naming + the trim chain.
        assert_eq!(munge_name("sampleX_bismark.bam"), "sampleX");
        assert_eq!(munge_name("foo.fq.gz"), "foo");
        assert_eq!(munge_name("bar_trimmed"), "bar");
        assert_eq!(munge_name("baz_1"), "baz");
        assert_eq!(munge_name("baz_2"), "baz");
    }

    #[test]
    fn aligned_blanked_when_dedup_present() {
        let mut m = sample("x_bismark_bt2_pe.bam");
        m.aligned_reads = "800".into();
        m.dup_reads = "200".into();
        m.unique_reads = "600".into();
        let a = assemble(&[m]);
        assert_eq!(a.aligned, vec![""]); // blanked
        assert_eq!(a.dup, vec!["200"]);
        assert_eq!(a.unique, vec!["600"]);
    }

    #[test]
    fn aligned_kept_and_defaults_applied_when_no_dedup() {
        let mut m = sample("x_bismark_bt2.bam");
        m.aligned_reads = "400".into();
        // unaligned/ambig/no_seq empty → 0-defaulted; dup/unique empty stay "".
        let a = assemble(&[m]);
        assert_eq!(a.aligned, vec!["400"]);
        assert_eq!(a.not_aligned, vec!["0"]);
        assert_eq!(a.ambig, vec!["0"]);
        assert_eq!(a.no_seq, vec!["0"]);
        assert_eq!(a.dup, vec![""]);
        assert_eq!(a.unique, vec![""]);
    }

    #[test]
    fn plot_exclusion_keeps_count_but_drops_from_arrays() {
        let good = sample("good_bismark_bt2.bam");
        // zero CHH context → excluded from plots
        let mut bad = sample("bad_bismark_bt2.bam");
        bad.meth_chh = "0".into();
        bad.unmeth_chh = "0".into();
        let a = assemble(&[good, bad]);
        assert_eq!(a.num_samples, 2); // total count unchanged
        assert_eq!(a.categories.len(), 1); // only the good sample plotted
        assert_eq!(a.categories[0], "'good_bismark_bt2.bam'");
    }
}

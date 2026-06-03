//! Report parsers and the per-sample metric record.
//!
//! Mirrors Perl `bismark2summary:248-385`. Every field is a `String` that
//! starts empty (`''` in Perl) so the `.txt` reproduces Perl's "not found =
//! empty cell" semantics byte-for-byte (§2.6). Parsing is **last-match-wins**
//! (scan every line; a later match overwrites), matching Perl's
//! `$x = $1 if /.../` idiom. The dedup report **overwrites** `aligned_reads`;
//! the splitting report **overwrites** the context-methylation fields (with a
//! different unmethylated pattern — `Total C to T conversions`).
//!
//! These parsers are deliberately **duplicated** here rather than shared with
//! the not-yet-merged `bismark-report` crate (SPEC §3 / O2).

use std::path::Path;

use crate::discovery::derive_names;
use crate::error::BismarkSummaryError;

/// Per-sample metrics, captured as raw strings (empty = not found).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SampleMetrics {
    /// Raw BAM string (the `.txt` column 1 — verbatim, not the stripped base).
    pub bam: String,
    /// `Total Reads`.
    pub total_reads: String,
    /// `Aligned Reads` (overwritten by the dedup report when present).
    pub aligned_reads: String,
    /// `Unaligned Reads`.
    pub unaligned: String,
    /// `Ambiguously Aligned Reads`.
    pub ambig_reads: String,
    /// `No Genomic Sequence`.
    pub no_seq_reads: String,
    /// `Duplicate Reads (removed)` (dedup report only).
    pub dup_reads: String,
    /// `Unique Reads (remaining)` (dedup report only).
    pub unique_reads: String,
    /// `Total Cs`.
    pub total_c: String,
    /// `Methylated CpGs`.
    pub meth_cpg: String,
    /// `Unmethylated CpGs`.
    pub unmeth_cpg: String,
    /// `Methylated chgs` (note the Perl lowercase header quirk — §2.6).
    pub meth_chg: String,
    /// `Unmethylated chgs`.
    pub unmeth_chg: String,
    /// `Methylated CHHs`.
    pub meth_chh: String,
    /// `Unmethylated CHHs`.
    pub unmeth_chh: String,
}

impl SampleMetrics {
    /// A fresh record with the BAM string set and all metrics empty.
    #[must_use]
    pub fn new(bam: &str) -> Self {
        SampleMetrics {
            bam: bam.to_string(),
            ..Default::default()
        }
    }
}

/// Iterate `content` as Perl `while (<FH>) { chomp; ... }` would: split on
/// `'\n'`, dropping only the final empty element when `content` ends in
/// `'\n'` (Perl reads no trailing empty record). A trailing `'\r'` (CRLF) is
/// **kept** — Perl `chomp` removes only the `'\n'` (so an anchored `(\d+)$`
/// fails on a CRLF line, matching Perl). Interior blank lines are preserved.
fn chomped_lines(content: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = content.split('\n').collect();
    if content.ends_with('\n') {
        parts.pop();
    }
    parts
}

/// Match Perl `^<label>\s+(\d+)` on a chomped line; `anchored` adds the
/// trailing `$`. Returns the captured digit run. `\s+` requires ≥1 ASCII
/// whitespace between the label and the digits.
fn capture_count(line: &str, label: &str, anchored: bool) -> Option<String> {
    let rest = line.strip_prefix(label)?;
    let after_ws = rest.trim_start_matches(|c: char| c.is_ascii_whitespace());
    if after_ws.len() == rest.len() {
        return None; // `\s+` needs at least one whitespace char
    }
    let ndigits = after_ws.bytes().take_while(u8::is_ascii_digit).count();
    if ndigits == 0 {
        return None;
    }
    if anchored && ndigits != after_ws.len() {
        // `$`: nothing may follow the digits (a trailing `\r` fails too).
        return None;
    }
    Some(after_ws[..ndigits].to_string())
}

/// Match Perl `^Total number of alignments analysed in .+:\s+(\d+)$` (the
/// dedup "alignments analysed" line, whose middle `.+` is the input
/// filename). Greedy `.+:` ⇒ the colon immediately before the trailing
/// `\s+\d+$`.
fn capture_dedup_total(line: &str) -> Option<String> {
    const PREFIX: &str = "Total number of alignments analysed in ";
    if !line.starts_with(PREFIX) {
        return None;
    }
    let bytes = line.as_bytes();
    // Trailing (\d+)$
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1].is_ascii_digit() {
        i -= 1;
    }
    let ndigits = bytes.len() - i;
    if ndigits == 0 {
        return None;
    }
    // \s+ immediately before the digits
    let mut j = i;
    while j > 0 && (bytes[j - 1] as char).is_ascii_whitespace() {
        j -= 1;
    }
    if j == i {
        return None; // no whitespace
    }
    // ':' immediately before the whitespace
    if j == 0 || bytes[j - 1] != b':' {
        return None;
    }
    let colon = j - 1;
    // `.+` (≥1 char) between the prefix and the colon
    if colon <= PREFIX.len() {
        return None;
    }
    Some(line[i..].to_string())
}

/// Parse a Bismark **alignment** report (PE or SE pattern set), filling the
/// general-stats + context-methylation fields. Perl `:288-313`.
pub fn parse_alignment_report(content: &str, paired: bool, m: &mut SampleMetrics) {
    for line in chomped_lines(content) {
        if paired {
            if let Some(v) = capture_count(line, "Sequence pairs analysed in total:", true) {
                m.total_reads = v;
            }
            if let Some(v) = capture_count(
                line,
                "Sequence pairs with no alignments under any condition:",
                true,
            ) {
                m.unaligned = v;
            }
            if let Some(v) = capture_count(line, "Sequence pairs did not map uniquely:", true) {
                m.ambig_reads = v;
            }
            if let Some(v) = capture_count(
                line,
                "Sequence pairs which were discarded because genomic sequence could not be extracted:",
                true,
            ) {
                m.no_seq_reads = v;
            }
            if let Some(v) = capture_count(
                line,
                "Number of paired-end alignments with a unique best hit:",
                true,
            ) {
                m.aligned_reads = v;
            }
        } else {
            if let Some(v) = capture_count(line, "Sequences analysed in total:", true) {
                m.total_reads = v;
            }
            if let Some(v) = capture_count(
                line,
                "Sequences with no alignments under any condition:",
                true,
            ) {
                m.unaligned = v;
            }
            if let Some(v) = capture_count(line, "Sequences did not map uniquely:", true) {
                m.ambig_reads = v;
            }
            if let Some(v) = capture_count(
                line,
                "Sequences which were discarded because genomic sequence could not be extracted:",
                true,
            ) {
                m.no_seq_reads = v;
            }
            if let Some(v) = capture_count(
                line,
                "Number of alignments with a unique best hit from the different alignments:",
                true,
            ) {
                m.aligned_reads = v;
            }
        }

        // Context methylation (both modes). `total_c` is `$`-anchored; the
        // six context patterns are NOT (§2.5).
        if let Some(v) = capture_count(line, "Total number of C's analysed:", true) {
            m.total_c = v;
        }
        if let Some(v) = capture_count(line, "Total methylated C's in CpG context:", false) {
            m.meth_cpg = v;
        }
        if let Some(v) = capture_count(line, "Total methylated C's in CHG context:", false) {
            m.meth_chg = v;
        }
        if let Some(v) = capture_count(line, "Total methylated C's in CHH context:", false) {
            m.meth_chh = v;
        }
        if let Some(v) = capture_count(line, "Total unmethylated C's in CpG context:", false) {
            m.unmeth_cpg = v;
        }
        if let Some(v) = capture_count(line, "Total unmethylated C's in CHG context:", false) {
            m.unmeth_chg = v;
        }
        if let Some(v) = capture_count(line, "Total unmethylated C's in CHH context:", false) {
            m.unmeth_chh = v;
        }
    }
}

/// Parse a Bismark **deduplication** report. Overwrites `aligned_reads` and
/// fills `dup_reads` / `unique_reads`. Three independent matches (Perl
/// `:330/334/338` — not `elsif`).
pub fn parse_dedup_report(content: &str, m: &mut SampleMetrics) {
    for line in chomped_lines(content) {
        if let Some(v) = capture_dedup_total(line) {
            m.aligned_reads = v;
        }
        if let Some(v) = capture_count(line, "Total number duplicated alignments removed:", false) {
            m.dup_reads = v;
        }
        if let Some(v) = capture_count(
            line,
            "Total count of deduplicated leftover sequences:",
            false,
        ) {
            m.unique_reads = v;
        }
    }
}

/// Parse a Bismark methylation-extractor **splitting** report. **Overwrites**
/// the context-methylation fields, using `Total C to T conversions` for the
/// unmethylated counts (NOT `Total unmethylated C's`). Perl `:371-380`.
pub fn parse_splitting_report(content: &str, m: &mut SampleMetrics) {
    for line in chomped_lines(content) {
        if let Some(v) = capture_count(line, "Total number of C's analysed:", true) {
            m.total_c = v;
        }
        if let Some(v) = capture_count(line, "Total methylated C's in CpG context:", false) {
            m.meth_cpg = v;
        }
        if let Some(v) = capture_count(line, "Total methylated C's in CHG context:", false) {
            m.meth_chg = v;
        }
        if let Some(v) = capture_count(line, "Total methylated C's in CHH context:", false) {
            m.meth_chh = v;
        }
        if let Some(v) = capture_count(line, "Total C to T conversions in CpG context:", false) {
            m.unmeth_cpg = v;
        }
        if let Some(v) = capture_count(line, "Total C to T conversions in CHG context:", false) {
            m.unmeth_chg = v;
        }
        if let Some(v) = capture_count(line, "Total C to T conversions in CHH context:", false) {
            m.unmeth_chh = v;
        }
    }
}

/// Read a report file into a `String` (lossy UTF-8; reports are ASCII).
fn read_report(path: &Path) -> Result<String, BismarkSummaryError> {
    let bytes = std::fs::read(path).map_err(|e| BismarkSummaryError::Io {
        path: Some(path.to_path_buf()),
        source: e,
    })?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Collect one sample's metrics from its report set, resolving report paths
/// against `base_dir`. The alignment report is mandatory (error if missing);
/// dedup + splitting are optional. Mirrors the per-BAM loop body
/// (`bismark2summary:248-385`).
pub fn collect_sample(base_dir: &Path, bam: &str) -> Result<SampleMetrics, BismarkSummaryError> {
    let names = derive_names(bam);

    let aln_path = base_dir.join(&names.alignment_report);
    if !aln_path.exists() {
        return Err(BismarkSummaryError::MissingAlignmentReport(aln_path));
    }

    let mut m = SampleMetrics::new(bam);
    parse_alignment_report(&read_report(&aln_path)?, names.paired, &mut m);

    let dedup_path = base_dir.join(&names.dedup_report);
    let dedup_exists = dedup_path.exists();
    if dedup_exists {
        parse_dedup_report(&read_report(&dedup_path)?, &mut m);
    }

    let split_path = base_dir.join(names.splitting_report(dedup_exists));
    if split_path.exists() {
        parse_splitting_report(&read_report(&split_path)?, &mut m);
    }

    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_count_anchored_and_unanchored() {
        assert_eq!(
            capture_count(
                "Sequences analysed in total:\t100",
                "Sequences analysed in total:",
                true
            ),
            Some("100".to_string())
        );
        // Anchored: trailing content after digits → no match.
        assert_eq!(
            capture_count(
                "Total number of C's analysed:\t50 extra",
                "Total number of C's analysed:",
                true
            ),
            None
        );
        // Unanchored: trailing content allowed (meth lines have a trailing %).
        assert_eq!(
            capture_count(
                "Total methylated C's in CpG context:\t30 (12.3%)",
                "Total methylated C's in CpG context:",
                false
            ),
            Some("30".to_string())
        );
        // Needs ≥1 whitespace.
        assert_eq!(capture_count("Foo:bar", "Foo:", true), None);
    }

    #[test]
    fn capture_count_anchored_fails_on_trailing_cr() {
        // CRLF line, chomp keeps the \r → anchored `$` fails (matches Perl).
        assert_eq!(
            capture_count(
                "Sequences analysed in total:   7\r",
                "Sequences analysed in total:",
                true
            ),
            None
        );
    }

    #[test]
    fn dedup_total_handles_filename_wildcard() {
        assert_eq!(
            capture_dedup_total(
                "Total number of alignments analysed in sample_pe.bam in total:\t900"
            ),
            Some("900".to_string())
        );
        assert_eq!(
            capture_dedup_total("Total number of alignments analysed in:\t5"),
            None
        ); // no `.+`
        assert_eq!(capture_dedup_total("Unrelated line"), None);
    }

    #[test]
    fn alignment_pe_parse() {
        let report = "Bismark report for: x (version: v0.25.1)\n\
            Sequence pairs analysed in total:\t1000\n\
            Number of paired-end alignments with a unique best hit:\t800\n\
            Sequence pairs with no alignments under any condition:\t150\n\
            Sequence pairs did not map uniquely:\t50\n\
            Sequence pairs which were discarded because genomic sequence could not be extracted:\t0\n\
            Total number of C's analysed:\t5000\n\
            Total methylated C's in CpG context:\t100\n\
            Total methylated C's in CHG context:\t10\n\
            Total methylated C's in CHH context:\t20\n\
            Total unmethylated C's in CpG context:\t900\n\
            Total unmethylated C's in CHG context:\t490\n\
            Total unmethylated C's in CHH context:\t980\n";
        let mut m = SampleMetrics::new("x_bismark_bt2_pe.bam");
        parse_alignment_report(report, true, &mut m);
        assert_eq!(m.total_reads, "1000");
        assert_eq!(m.aligned_reads, "800");
        assert_eq!(m.unaligned, "150");
        assert_eq!(m.ambig_reads, "50");
        assert_eq!(m.no_seq_reads, "0");
        assert_eq!(m.total_c, "5000");
        assert_eq!(m.meth_cpg, "100");
        assert_eq!(m.unmeth_chh, "980");
    }

    #[test]
    fn se_patterns_do_not_fire_in_pe_mode() {
        // An SE total line must NOT match when paired=true.
        let mut m = SampleMetrics::new("x_pe.bam");
        parse_alignment_report("Sequences analysed in total:\t42\n", true, &mut m);
        assert_eq!(m.total_reads, "");
    }

    #[test]
    fn dedup_overwrites_aligned_and_sets_counts() {
        let mut m = SampleMetrics::new("x.bam");
        m.aligned_reads = "800".to_string(); // from alignment report
        let report = "Total number of alignments analysed in x.bam in total:\t800\n\
            Total number duplicated alignments removed:\t200 (25.00%)\n\
            Total count of deduplicated leftover sequences:\t600 (75.00% of total)\n";
        parse_dedup_report(report, &mut m);
        assert_eq!(m.aligned_reads, "800");
        assert_eq!(m.dup_reads, "200");
        assert_eq!(m.unique_reads, "600");
    }

    #[test]
    fn splitting_overwrites_methylation_with_c_to_t() {
        let mut m = SampleMetrics::new("x.bam");
        m.unmeth_cpg = "111".to_string(); // stale value from alignment report
        let report = "Total number of C's analysed:\t7000\n\
            Total methylated C's in CpG context:\t150\n\
            Total C to T conversions in CpG context:\t6850\n";
        parse_splitting_report(report, &mut m);
        assert_eq!(m.total_c, "7000");
        assert_eq!(m.meth_cpg, "150");
        assert_eq!(m.unmeth_cpg, "6850"); // overwritten via C-to-T pattern
    }

    #[test]
    fn last_match_wins() {
        let mut m = SampleMetrics::new("x.bam");
        parse_alignment_report(
            "Sequences analysed in total:\t1\nSequences analysed in total:\t2\n",
            false,
            &mut m,
        );
        assert_eq!(m.total_reads, "2");
    }
}

//! The tab-delimited `bismark_summary_report.txt` table.
//!
//! Perl `bismark2summary:228-246` (header), `:387-404` (rows), `:478-481`
//! (write). One row per sample in discovery/argv order, 15 tab-separated
//! columns, header + each row `\n`-terminated. Values are the **raw**
//! captured strings (empty = not found) — captured before the plot-array
//! 0-defaulting (§2.6).

use crate::parse::SampleMetrics;

/// The 15 column headers, verbatim from Perl (`:229-243`).
///
/// ⚠ Columns 12–13 are lowercase `chgs` while CpG/CHH are capitalised — a
/// source quirk reproduced exactly (§2.6). The stale `docs/images` oracle
/// says `CpHs` here; do not copy it.
pub const HEADER_FIELDS: [&str; 15] = [
    "File",
    "Total Reads",
    "Aligned Reads",
    "Unaligned Reads",
    "Ambiguously Aligned Reads",
    "No Genomic Sequence",
    "Duplicate Reads (removed)",
    "Unique Reads (remaining)",
    "Total Cs",
    "Methylated CpGs",
    "Unmethylated CpGs",
    "Methylated chgs",
    "Unmethylated chgs",
    "Methylated CHHs",
    "Unmethylated CHHs",
];

/// Build the complete `.txt` content: header + one row per sample.
#[must_use]
pub fn build_txt(samples: &[SampleMetrics]) -> String {
    let mut out = String::new();
    out.push_str(&HEADER_FIELDS.join("\t"));
    out.push('\n');
    for s in samples {
        let cols: [&str; 15] = [
            &s.bam,
            &s.total_reads,
            &s.aligned_reads,
            &s.unaligned,
            &s.ambig_reads,
            &s.no_seq_reads,
            &s.dup_reads,
            &s.unique_reads,
            &s.total_c,
            &s.meth_cpg,
            &s.unmeth_cpg,
            &s.meth_chg,
            &s.unmeth_chg,
            &s.meth_chh,
            &s.unmeth_chh,
        ];
        out.push_str(&cols.join("\t"));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_has_lowercase_chgs_quirk() {
        assert_eq!(HEADER_FIELDS[11], "Methylated chgs");
        assert_eq!(HEADER_FIELDS[12], "Unmethylated chgs");
        assert_eq!(HEADER_FIELDS[9], "Methylated CpGs");
        assert_eq!(HEADER_FIELDS[13], "Methylated CHHs");
    }

    #[test]
    fn empty_sample_list_is_header_only() {
        let txt = build_txt(&[]);
        assert_eq!(txt, format!("{}\n", HEADER_FIELDS.join("\t")));
    }

    #[test]
    fn row_keeps_empty_cells_and_trailing_newline() {
        let mut s = SampleMetrics::new("x_bismark_bt2.bam");
        s.total_reads = "100".to_string();
        s.aligned_reads = "80".to_string();
        // dup_reads / unique_reads left empty (no dedup) → empty cells.
        let txt = build_txt(&[s]);
        let lines: Vec<&str> = txt.split('\n').collect();
        assert_eq!(lines[0], HEADER_FIELDS.join("\t"));
        assert_eq!(
            lines[1],
            "x_bismark_bt2.bam\t100\t80\t\t\t\t\t\t\t\t\t\t\t\t"
        );
        assert_eq!(lines[2], ""); // trailing newline
        assert!(txt.ends_with('\n'));
    }
}

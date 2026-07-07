//! Deduplication report parser + fill (SPEC §2.7b).

use crate::report::reports::{before_first_ws, field1_owned, join_with, report_lines};
use crate::report::template::subst_all;

/// Captured deduplication fields.
#[derive(Debug, Default)]
pub struct Dedup {
    pub total_seqs: Option<Vec<u8>>,
    pub dups: Option<Vec<u8>>,
    pub diff_pos: Option<Vec<u8>>,
    pub leftover: Option<Vec<u8>>,
}

/// Parse a deduplication report (`bismark2report:522-548`).
pub fn parse(data: &[u8]) -> Dedup {
    let mut d = Dedup::default();
    for line in report_lines(data) {
        if line.starts_with(b"Total number of alignments") {
            d.total_seqs = field1_owned(line);
        } else if line.starts_with(b"Total number duplicated") {
            // keep only the leading number (drop the trailing " (NN.NN%)")
            d.dups = field1_owned(line).map(|v| before_first_ws(&v).to_vec());
        } else if line.starts_with(b"Duplicated alignments were found at") {
            d.diff_pos = field1_owned(line).map(|v| before_first_ws(&v).to_vec());
        } else if let Some(rest) =
            line.strip_prefix(b"Total count of deduplicated leftover sequences: ")
        {
            // Perl `(\d+)` — take the leading run of digits (≥1 required).
            let digits: Vec<u8> = rest
                .iter()
                .copied()
                .take_while(u8::is_ascii_digit)
                .collect();
            if !digits.is_empty() {
                d.leftover = Some(digits);
            }
        }
    }
    // Fallback: leftover = total - dups (integer; reproduce a negative if any).
    if d.leftover.is_none()
        && let (Some(t), Some(du)) = (&d.total_seqs, &d.dups)
    {
        d.leftover = Some((parse_i64(t) - parse_i64(du)).to_string().into_bytes());
    }
    d
}

fn parse_i64(v: &[u8]) -> i64 {
    std::str::from_utf8(v)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Fill the deduplication placeholders (Perl 551-559). Gate = `is_some()` on all
/// four (Perl `defined`, not truthiness — `0` counts are valid). On failure the
/// markers were already removed (section present) but the placeholders survive.
pub fn fill(mut doc: Vec<u8>, d: &Dedup) -> Vec<u8> {
    if !(d.dups.is_some() && d.total_seqs.is_some() && d.diff_pos.is_some() && d.leftover.is_some())
    {
        return doc;
    }
    let o = |x: &Option<Vec<u8>>| -> Vec<u8> { x.as_deref().unwrap_or(b"").to_vec() };
    let total = o(&d.total_seqs);
    let leftover = o(&d.leftover);
    let dups = o(&d.dups);
    doc = subst_all(doc, b"{{seqs_total_duplicates}}", &total);
    doc = subst_all(doc, b"{{unique_alignments_duplicates}}", &leftover);
    doc = subst_all(doc, b"{{duplicate_alignments_duplicates}}", &dups);
    doc = subst_all(doc, b"{{different_positions_duplicates}}", &o(&d.diff_pos));
    let plot = join_with(&[&leftover, &dups], b",");
    doc = subst_all(doc, b"{{duplication_stats_plotly}}", &plot);
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leftover_falls_back_to_total_minus_dups() {
        // No "Total count of deduplicated leftover sequences:" line → compute it.
        let d = parse(
            b"Total number of alignments analysed in x.bam:\t1000\n\
              Total number duplicated alignments removed:\t300 (30.00%)\n\
              Duplicated alignments were found at:\t250 different position(s)\n",
        );
        assert_eq!(d.leftover.as_deref(), Some(&b"700"[..]));
        assert_eq!(d.dups.as_deref(), Some(&b"300"[..])); // trailing " (30.00%)" dropped
        assert_eq!(d.diff_pos.as_deref(), Some(&b"250"[..]));
        let doc = fill(
            b"[{{unique_alignments_duplicates}}|{{duplicate_alignments_duplicates}}|{{duplication_stats_plotly}}]"
                .to_vec(),
            &d,
        );
        assert_eq!(doc, b"[700|300|700,300]");
    }

    #[test]
    fn explicit_leftover_line_wins_over_fallback() {
        let d = parse(
            b"Total number of alignments analysed in x.bam:\t1000\n\
              Total number duplicated alignments removed:\t300 (30.00%)\n\
              Duplicated alignments were found at:\t250 different position(s)\n\
              Total count of deduplicated leftover sequences: 712 (71.20% of total)\n",
        );
        assert_eq!(d.leftover.as_deref(), Some(&b"712"[..]));
    }
}

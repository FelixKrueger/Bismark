//! Nucleotide-coverage report parser + fill (SPEC §2.7e).
//!
//! Header (line 0) is validated; the body is iterated in a FIXED 20-key order
//! (NOT sorted). Missing keys → `0` for the percentages but the **empty string**
//! for counts/coverage (Perl undef-in-`s///`). The log2 ratio Perl computes is
//! commented out — no float is ever emitted.

use std::collections::HashMap;

use crate::report::error::ReportError;
use crate::report::reports::{join_with, report_lines, split_tab};
use crate::report::template::subst_all;

/// Fixed (di-)nucleotide row order (Perl 632) — NOT sorted.
const KEYS: [&[u8]; 20] = [
    b"A", b"T", b"C", b"G", b"AC", b"CA", b"TC", b"CT", b"CC", b"CG", b"GC", b"GG", b"AG", b"GA",
    b"TG", b"GT", b"TT", b"TA", b"AT", b"AA",
];

#[derive(Debug, Default, Clone)]
struct Entry {
    obs_pct: Option<Vec<u8>>,    // observed  (col 2)
    exp_pct: Option<Vec<u8>>,    // expected  (col 4)
    obs_counts: Option<Vec<u8>>, // count_obs (col 1)
    exp_counts: Option<Vec<u8>>, // count_exp (col 3)
    coverage: Option<Vec<u8>>,   // coverage  (col 5)
}

/// Captured nucleotide-coverage rows.
#[derive(Debug, Default)]
pub struct Nucleotide {
    nucs: HashMap<Vec<u8>, Entry>,
}

/// Remove the FIRST `\r` (Perl `s/\r//`, no `/g`).
fn strip_first_cr(line: &[u8]) -> Vec<u8> {
    match line.iter().position(|&b| b == b'\r') {
        Some(p) => {
            let mut o = line.to_vec();
            o.remove(p);
            o
        }
        None => line.to_vec(),
    }
}

/// Parse a nucleotide-coverage report (`bismark2report:581-614`). Errors if the
/// line-0 header is not a Bismark nucleotide report (cols 3 & 5).
pub fn parse(data: &[u8]) -> Result<Nucleotide, ReportError> {
    let mut n = Nucleotide::default();
    for (idx, raw) in report_lines(data).iter().enumerate() {
        let line = strip_first_cr(raw);
        let f = split_tab(&line);
        if idx == 0 {
            // col 3 (index 2) must be "percent sample"; col 5 (index 4) "percent genomic".
            if f.get(2).copied() != Some(b"percent sample") {
                return Err(ReportError::Validation(format!(
                    "Expected to find 'percent sample' as entry in line 1, column 3 but found '{}'. \
                     This doesn't look like a Bismark nucleotide coverage report. Please respecify!",
                    String::from_utf8_lossy(f.get(2).copied().unwrap_or(b""))
                )));
            }
            if f.get(4).copied() != Some(b"percent genomic") {
                return Err(ReportError::Validation(format!(
                    "Expected to find 'percent genomic' as entry in line 1, column 5 but found '{}'. \
                     This doesn't look like a Bismark nucleotide coverage report. Please respecify!",
                    String::from_utf8_lossy(f.get(4).copied().unwrap_or(b""))
                )));
            }
            continue;
        }
        let Some(&element) = f.first() else { continue };
        let e = n.nucs.entry(element.to_vec()).or_default();
        e.obs_counts = f.get(1).map(|s| s.to_vec());
        e.obs_pct = f.get(2).map(|s| s.to_vec());
        e.exp_counts = f.get(3).map(|s| s.to_vec());
        e.exp_pct = f.get(4).map(|s| s.to_vec());
        e.coverage = f.get(5).map(|s| s.to_vec());
    }
    Ok(n)
}

/// Fill the nucleotide placeholders (Perl 624-690), fixed key order.
pub fn fill(mut doc: Vec<u8>, n: &Nucleotide) -> Vec<u8> {
    let empty = Entry::default();
    // Per-key cell substitutions + accumulate the plot arrays.
    let mut x_sample: Vec<Vec<u8>> = Vec::with_capacity(20);
    let mut x_genomic: Vec<Vec<u8>> = Vec::with_capacity(20);
    for key in KEYS {
        let e = n.nucs.get(key).unwrap_or(&empty);
        let nuc_obs = e.obs_pct.as_deref().unwrap_or(b"0"); // missing % → 0
        let nuc_exp = e.exp_pct.as_deref().unwrap_or(b"0");
        let counts_obs = e.obs_counts.as_deref().unwrap_or(b""); // missing count → ""
        let counts_exp = e.exp_counts.as_deref().unwrap_or(b"");
        let cov = e.coverage.as_deref().unwrap_or(b"");
        let k = std::str::from_utf8(key).unwrap();
        doc = subst_all(doc, format!("{{{{nuc_{k}_p_obs}}}}").as_bytes(), nuc_obs);
        doc = subst_all(doc, format!("{{{{nuc_{k}_p_exp}}}}").as_bytes(), nuc_exp);
        doc = subst_all(
            doc,
            format!("{{{{nuc_{k}_counts_obs}}}}").as_bytes(),
            counts_obs,
        );
        doc = subst_all(
            doc,
            format!("{{{{nuc_{k}_counts_exp}}}}").as_bytes(),
            counts_exp,
        );
        doc = subst_all(doc, format!("{{{{nuc_{k}_coverage}}}}").as_bytes(), cov);
        x_sample.push(nuc_obs.to_vec());
        x_genomic.push(nuc_exp.to_vec());
    }

    // y-array: 'A','T',...,'AA' (join "','" then wrap in single quotes).
    let mut y = Vec::new();
    y.push(b'\'');
    for (i, k) in KEYS.iter().enumerate() {
        if i > 0 {
            y.extend_from_slice(b"','");
        }
        y.extend_from_slice(k);
    }
    y.push(b'\'');

    let xs: Vec<&[u8]> = x_sample.iter().map(Vec::as_slice).collect();
    let xg: Vec<&[u8]> = x_genomic.iter().map(Vec::as_slice).collect();
    doc = subst_all(doc, b"{{nucleo_sample_x}}", &join_with(&xs, b" , "));
    doc = subst_all(doc, b"{{nucleo_genomic_x}}", &join_with(&xg, b" , "));
    doc = subst_all(doc, b"{{nucleo_sample_y}}", &y);
    doc = subst_all(doc, b"{{nucleo_genomic_y}}", &y);
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &[u8] =
        b"(di-)nucleotide\tcount sample\tpercent sample\tcount genomic\tpercent genomic\tcoverage\n";

    #[test]
    fn bad_header_errors() {
        assert!(parse(b"wrong\theader\tline\n").is_err());
    }

    #[test]
    fn missing_key_renders_zero_percent_and_empty_counts() {
        // Only the A row present (issue #711, amplicon-like): absent keys → "0"
        // for percentages, empty string for counts/coverage (Perl undef-in-s///).
        let mut data = HEADER.to_vec();
        data.extend_from_slice(b"A\t100\t28.5\t900\t29.0\t0.11\n");
        let n = parse(&data).unwrap();
        let doc = fill(
            b"[{{nuc_A_p_obs}}|{{nuc_T_p_obs}}|{{nuc_T_counts_obs}}|{{nuc_T_coverage}}]".to_vec(),
            &n,
        );
        assert_eq!(doc, b"[28.5|0||]");
    }

    #[test]
    fn plot_arrays_use_correct_separators() {
        let mut data = HEADER.to_vec();
        data.extend_from_slice(b"A\t1\t10\t2\t20\t0.1\nT\t3\t30\t4\t40\t0.2\n");
        let n = parse(&data).unwrap();
        // y = quoted keys joined "','"; sample x = obs%% joined " , " (missing → 0).
        let doc = fill(b"[{{nucleo_sample_y}}]|[{{nucleo_sample_x}}]".to_vec(), &n);
        let s = String::from_utf8(doc).unwrap();
        assert!(s.starts_with("['A','T','C','G','AC',"), "y-array: {s}");
        assert!(s.contains("[10 , 30 , 0 , 0 ,"), "sample x-array: {s}");
    }
}

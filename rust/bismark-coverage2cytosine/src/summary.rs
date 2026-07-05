//! The always-on cytosine-context summary (`*.cytosine_context_summary.txt`).
//!
//! Mirrors Perl `reset_context_summary` (`:1961-1975`), `context_reporting`
//! (`:1977-1988`), and `print_context_summary` (`:63-78`): a 64-cell grid of
//! the 16 `C{A,C,G,T}{A,C,G,T}` trinucleotides × 4 upstream bases `{A,C,G,T}`,
//! accumulated only for pure-`ACTG` trinucleotide + upstream base, printed
//! sorted by `(trinucleotide, upstream base)` with a `%.2f` percentage or
//! `N/A` when uncovered.

use std::collections::BTreeMap;
use std::io::{self, Write};

/// 64-cell (16 trinucleotides × 4 upstream bases) methylated/unmethylated grid.
/// `BTreeMap` keys give Perl's `sort keys` order for free (trinucleotide then
/// upstream base, bytewise).
#[derive(Debug)]
pub struct ContextSummary {
    /// `(trinucleotide bytes, upstream base) -> (methylated, unmethylated)`.
    cells: BTreeMap<(Vec<u8>, u8), (u32, u32)>,
}

impl Default for ContextSummary {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextSummary {
    /// All 64 cells zeroed (Perl `reset_context_summary`).
    #[must_use]
    pub fn new() -> Self {
        let mut cells = BTreeMap::new();
        for &b1 in b"ACGT" {
            for &b2 in b"ACGT" {
                let tri = vec![b'C', b1, b2];
                for &ubase in b"ACGT" {
                    cells.insert((tri.clone(), ubase), (0, 0));
                }
            }
        }
        Self { cells }
    }

    /// Add coverage to `(tri_nt, ubase)` — but only when both are pure `ACTG`
    /// (Perl `context_reporting`'s `unless (tri =~ /[^ACTG]/ or ubase =~ /[^ACTG]/)`).
    pub fn accumulate(&mut self, tri_nt: &[u8], ubase: u8, meth: u32, nonmeth: u32) {
        if !is_actg(ubase) || !tri_nt.iter().all(|&b| is_actg(b)) {
            return;
        }
        let cell = self.cells.entry((tri_nt.to_vec(), ubase)).or_insert((0, 0));
        cell.0 += meth;
        cell.1 += nonmeth;
    }

    /// Write the summary (header + 64 sorted rows). Never gzipped.
    pub fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        writeln!(
            w,
            "upstream\tC-context\tfull context\tcount methylated\tcount unmethylated\tpercent methylation"
        )?;
        for ((tri, ubase), (m, u)) in &self.cells {
            let total = m + u;
            let perc = if total > 0 {
                format!("{:.2}", f64::from(*m) / f64::from(total) * 100.0)
            } else {
                "N/A".to_string()
            };
            // ubase \t tri \t ubase+tri \t m \t u \t perc
            w.write_all(&[*ubase])?;
            w.write_all(b"\t")?;
            w.write_all(tri)?;
            w.write_all(b"\t")?;
            w.write_all(&[*ubase])?;
            w.write_all(tri)?;
            writeln!(w, "\t{m}\t{u}\t{perc}")?;
        }
        Ok(())
    }
}

#[inline]
fn is_actg(b: u8) -> bool {
    matches!(b, b'A' | b'C' | b'T' | b'G')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_writes_64_rows_sorted_with_header() {
        let s = ContextSummary::new();
        let mut out = Vec::new();
        s.write_to(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            "upstream\tC-context\tfull context\tcount methylated\tcount unmethylated\tpercent methylation"
        );
        assert_eq!(lines.len(), 1 + 64);
        assert_eq!(lines[1], "A\tCAA\tACAA\t0\t0\tN/A"); // first sorted cell
        assert!(lines[2..].iter().all(|l| l.ends_with("\t0\t0\tN/A")));
    }

    #[test]
    fn summary_accumulates_pure_actg_only_and_formats_percent() {
        let mut s = ContextSummary::new();
        s.accumulate(b"CGT", b'A', 3, 1); // pure ACTG → counted
        s.accumulate(b"CNG", b'A', 9, 9); // tri has N → ignored
        s.accumulate(b"CGT", b'N', 9, 9); // ubase N → ignored
        let mut out = Vec::new();
        s.write_to(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("A\tCGT\tACGT\t3\t1\t75.00"));
    }
}

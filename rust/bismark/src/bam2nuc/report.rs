//! `*.nucleotide_stats.txt` report writer, faithful to Perl `bam2nuc`'s
//! `calculate_averages` (`:276-315`).
//!
//! Layout (tab-separated, trailing `\n` per line). The header is followed by
//! 4 mono rows (`A`,`C`,`G`,`T`) then 16 di rows (`AA`,`AC`,…,`TT`); each row is
//! `<word>\t<count sample>\t<percent sample>\t<count genomic>\t<percent genomic>\t<coverage>`:
//!
//! ```text
//! (di-)nucleotide<TAB>count sample<TAB>percent sample<TAB>count genomic<TAB>percent genomic<TAB>coverage
//! ```
//!
//! Byte-identity subtleties:
//! - **Separate totals** for the mono group (A/C/G/T) and the di group (16
//!   words); percentages are over the respective group total.
//! - **Empty count field:** an unseen sample word (`count == 0`, i.e. Perl
//!   `undef`) prints an EMPTY `count sample` field but uses `0` in the
//!   arithmetic (`pct = 0.00`, `coverage = 0.000`). Reproduced via the
//!   `count == 0 ⇔ absent` invariant of [`NucCounts`].
//! - **Division by zero → error:** a zero group total (empty/all-skipped
//!   sample) or a zero genomic word count (coverage denominator) makes Perl
//!   `die "Illegal division by zero"` mid-routine, leaving a partial file. The
//!   Rust port errors (exit 1) at the same point and likewise does not clean
//!   up the partial stats file.
//! - **Rounding:** Rust `format!("{:.2}")`/`{:.3}` is round-half-to-even,
//!   matching C `printf` (verified live by both plan reviewers; oxy confirms
//!   the target platform).

use std::io::Write;

use crate::bam2nuc::error::BismarkBam2nucError;
use crate::bam2nuc::freqs::NucCounts;

/// Tab-separated header line (Perl `:284`).
const HEADER: &str =
    "(di-)nucleotide\tcount sample\tpercent sample\tcount genomic\tpercent genomic\tcoverage";

/// Mononucleotides in Perl's fixed order (`:286`).
const MONO: [u8; 4] = [b'A', b'C', b'G', b'T'];

/// Dinucleotides in Perl's fixed order (`:302`).
const DI: [[u8; 2]; 16] = [
    [b'A', b'A'],
    [b'A', b'C'],
    [b'A', b'G'],
    [b'A', b'T'],
    [b'C', b'A'],
    [b'C', b'C'],
    [b'C', b'G'],
    [b'C', b'T'],
    [b'G', b'A'],
    [b'G', b'C'],
    [b'G', b'G'],
    [b'G', b'T'],
    [b'T', b'A'],
    [b'T', b'C'],
    [b'T', b'G'],
    [b'T', b'T'],
];

/// Write the full `*.nucleotide_stats.txt` report to `out`.
pub fn write_stats<W: Write>(
    out: &mut W,
    sample: &NucCounts,
    genomic: &NucCounts,
) -> Result<(), BismarkBam2nucError> {
    writeln!(out, "{HEADER}")?;

    // ── Mono group (its own totals) ──
    let total_s: u64 = MONO.iter().map(|&b| sample.mono(b)).sum();
    let total_g: u64 = MONO.iter().map(|&b| genomic.mono(b)).sum();
    for &b in &MONO {
        write_row(out, &[b], sample.mono(b), genomic.mono(b), total_s, total_g)?;
    }

    // ── Di group (its own totals) ──
    let total_s: u64 = DI.iter().map(|p| sample.di(p[0], p[1])).sum();
    let total_g: u64 = DI.iter().map(|p| genomic.di(p[0], p[1])).sum();
    for p in &DI {
        write_row(
            out,
            p,
            sample.di(p[0], p[1]),
            genomic.di(p[0], p[1]),
            total_s,
            total_g,
        )?;
    }

    Ok(())
}

/// Write one row. Mirrors the per-word computation order in Perl
/// `:291-296`/`:307-312`: percentage (÷ sample total), percentage_genomic
/// (÷ genomic total), coverage (÷ genomic word count) — Perl dies at the first
/// zero denominator, so a row whose denominator is zero is NEVER written (prior
/// rows already are).
fn write_row<W: Write>(
    out: &mut W,
    word: &[u8],
    cs: u64,
    cg: u64,
    total_s: u64,
    total_g: u64,
) -> Result<(), BismarkBam2nucError> {
    if total_s == 0 {
        return Err(BismarkBam2nucError::ZeroDivision {
            detail: "sample total is zero".into(),
        });
    }
    if total_g == 0 {
        return Err(BismarkBam2nucError::ZeroDivision {
            detail: "genomic total is zero".into(),
        });
    }
    if cg == 0 {
        return Err(BismarkBam2nucError::ZeroDivision {
            detail: format!(
                "genomic count for {:?} is zero",
                String::from_utf8_lossy(word)
            ),
        });
    }

    let pct_s = format!("{:.2}", 100.0 * cs as f64 / total_s as f64);
    let pct_g = format!("{:.2}", 100.0 * cg as f64 / total_g as f64);
    let cov = format!("{:.3}", cs as f64 / cg as f64);
    // Empty count field for an unseen sample word (Perl undef → "").
    let cs_field = if cs == 0 {
        String::new()
    } else {
        cs.to_string()
    };
    // `word` is always one of the ASCII MONO/DI constants.
    let word_str = std::str::from_utf8(word).expect("nucleotide word is ASCII");
    writeln!(out, "{word_str}\t{cs_field}\t{pct_s}\t{cg}\t{pct_g}\t{cov}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bam2nuc::freqs::process_sequence;

    fn counts_of(seq: &[u8]) -> NucCounts {
        let mut c = NucCounts::default();
        process_sequence(seq, &mut c);
        c
    }

    /// sample = "ACGTACGT" (mono A,C,G,T=2; di AC=2,CG=2,GT=2,TA=1).
    /// genomic = a de Bruijn B(4,2) sequence "AACAGATCCGCTGGTTA" (mono A=5,C=4,
    /// G=4,T=4; all 16 di=1) — chosen so EVERY genomic word is non-zero (no
    /// division-by-zero) yet many SAMPLE di-words are absent (empty fields).
    fn sample_and_genomic() -> (NucCounts, NucCounts) {
        (counts_of(b"ACGTACGT"), counts_of(b"AACAGATCCGCTGGTTA"))
    }

    fn render() -> String {
        let (s, g) = sample_and_genomic();
        let mut buf = Vec::new();
        write_stats(&mut buf, &s, &g).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn header_exact() {
        let out = render();
        let first = out.lines().next().unwrap();
        assert_eq!(
            first,
            "(di-)nucleotide\tcount sample\tpercent sample\tcount genomic\tpercent genomic\tcoverage"
        );
    }

    #[test]
    fn has_header_plus_4_mono_plus_16_di_lines() {
        let out = render();
        assert_eq!(out.lines().count(), 1 + 4 + 16);
    }

    #[test]
    fn mono_row_a_exact() {
        // A: cs=2, cg=5, total_s=8, total_g=17.
        // pct_s = 100*2/8 = 25.00; pct_g = 100*5/17 = 29.41; cov = 2/5 = 0.400.
        let out = render();
        assert!(
            out.lines().any(|l| l == "A\t2\t25.00\t5\t29.41\t0.400"),
            "mono A row missing/wrong in:\n{out}"
        );
    }

    #[test]
    fn di_empty_count_field_for_absent_word() {
        // AA absent from the sample → empty count field; cg=1, total_g=16.
        // pct_s = 0.00; pct_g = 100*1/16 = 6.25; cov = 0/1 = 0.000.
        // The literal `\t\t` (empty count column) is the fragile byte-detail.
        let out = render();
        assert!(
            out.lines().any(|l| l == "AA\t\t0.00\t1\t6.25\t0.000"),
            "AA empty-count-field row missing/wrong in:\n{out}"
        );
    }

    #[test]
    fn di_populated_rows_exact() {
        let out = render();
        // AC: cs=2, total_s=7 → 100*2/7 = 28.57; cov = 2/1 = 2.000.
        assert!(
            out.lines().any(|l| l == "AC\t2\t28.57\t1\t6.25\t2.000"),
            "{out}"
        );
        // TA: cs=1, total_s=7 → 100*1/7 = 14.29; cov = 1.000.
        assert!(
            out.lines().any(|l| l == "TA\t1\t14.29\t1\t6.25\t1.000"),
            "{out}"
        );
    }

    #[test]
    fn di_words_in_fixed_order() {
        let out = render();
        let di_lines: Vec<&str> = out.lines().skip(1 + 4).collect();
        let words: Vec<&str> = di_lines
            .iter()
            .map(|l| l.split('\t').next().unwrap())
            .collect();
        assert_eq!(
            words,
            vec![
                "AA", "AC", "AG", "AT", "CA", "CC", "CG", "CT", "GA", "GC", "GG", "GT", "TA", "TC",
                "TG", "TT"
            ]
        );
    }

    #[test]
    fn zero_sample_total_errors() {
        // Empty sample (all reads skipped) → mono total 0 → ZeroDivision on the
        // first mono row (header already written, matching Perl's mid-routine die).
        let sample = NucCounts::default();
        let genomic = counts_of(b"AACAGATCCGCTGGTTA");
        let mut buf = Vec::new();
        let err = write_stats(&mut buf, &sample, &genomic).unwrap_err();
        assert!(matches!(err, BismarkBam2nucError::ZeroDivision { .. }));
        // The header WAS written before the error (partial file).
        assert!(
            String::from_utf8(buf)
                .unwrap()
                .starts_with("(di-)nucleotide\t")
        );
    }

    #[test]
    fn zero_genomic_word_count_errors() {
        // genomic missing a di-word → coverage divides by zero on that word.
        let sample = counts_of(b"ACGTACGT");
        let genomic = counts_of(b"ACGT"); // di AA/AG/... absent in genome
        let mut buf = Vec::new();
        let err = write_stats(&mut buf, &sample, &genomic).unwrap_err();
        assert!(matches!(err, BismarkBam2nucError::ZeroDivision { .. }));
    }

    #[test]
    fn format_rounding_is_round_half_to_even() {
        // Documents the round-half-to-even contract (Rust core == C printf;
        // both reviewers verified live). 6.125 and 6.375 are EXACT in f64.
        assert_eq!(format!("{:.2}", 6.125_f64), "6.12"); // 2 is even
        assert_eq!(format!("{:.2}", 6.375_f64), "6.38"); // 8 is even
    }
}

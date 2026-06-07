//! `--combined_index` (v2) strand classification + provisional per-read selection.
//!
//! **Opt-in, never-silent, concordance-gated — NOT byte-identical** to the
//! faithful per-strand-instance path. The combined index is searched in ONE
//! both-strands Bowtie 2 pass (`-k 2`, no `--norc`/`--nofw`); each alignment's
//! strand is recovered from its **RNAME suffix (`_CT_converted`/`_GA_converted`)
//! × FLAG orientation** (the spike-validated rule):
//!
//! | sub-genome | fwd (FLAG&0x10==0) | rev (FLAG&0x10!=0) |
//! |------------|--------------------|--------------------|
//! | CT         | **OT**             | spurious           |
//! | GA         | spurious           | **OB**             |
//!
//! This module is deliberately a SIBLING of `merge.rs`, NOT a path through it:
//! the faithful `check_results_single_end` keys off **separate strand-restricted
//! instance streams**, whereas the combined search emits ONE stream whose `-k`
//! runner-up is computed across *both* sub-genomes (so the two-instance
//! lockstep's within-thread-vs-cross-instance ambiguity semantics do not map).
//! Instead, [`select`] emits a [`merge::Decision`] carrying a [`BestAlignment`]
//! with a **synthetic instance index** (OT→0, OB→1) so the byte-frozen output arm
//! (`extract_corresponding_genomic_sequence_single_end` → `methylation_call` →
//! `single_end_sam_output`) — which is keyed purely on `best.index` — is reused
//! UNCHANGED. Because the merge is bypassed, [`select`] also OWNS the
//! alignment-outcome counters the merge would normally bump (PLAN §3.6).
//!
//! The uniqueness rule here is **PROVISIONAL** (best-AS single co-optimal). Phase
//! 3 replaces it with the Bismark-faithful `chr:pos` + `>=` + Sylvain-Foret tie
//! resolution; isolating it in this module keeps that swap from touching
//! `merge.rs` or the frozen output arm.

use crate::align::SamRecord;
use crate::error::{AlignerError, Result};
use crate::mapq::calc_mapq;
use crate::merge::{BestAlignment, Counters, Decision};

/// Strand classification of one combined-index alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombinedClass {
    /// fwd + `_CT_converted` → original top strand (OT).
    Ot,
    /// rev + `_GA_converted` → original bottom strand (OB).
    Ob,
    /// `fwd+GA` / `rev+CT` → spurious (wrong sub-genome for the orientation).
    Spurious,
}

impl CombinedClass {
    /// The synthetic instance index for the byte-frozen output arm: **OT→0,
    /// OB→1** (the `methylation.rs` index→strand selector: `0 → (+,CT,CT)`,
    /// `1 → (-,CT,GA)`). Spurious has no valid index. **NB:** OB is `1`, NOT `3`
    /// — index 3 is `(+,GA,GA)` = CTOB and would emit the wrong FLAG-arm /
    /// `XR:GA` / `XG:GA` / methylation branch for every OB read.
    fn to_index(self) -> Option<usize> {
        match self {
            CombinedClass::Ot => Some(0),
            CombinedClass::Ob => Some(1),
            CombinedClass::Spurious => None,
        }
    }
}

/// Classify one alignment by RNAME suffix × FLAG orientation, returning the
/// de-converted (suffix-stripped) chromosome + the class. Errors if the RNAME
/// lacks a `_CT_converted`/`_GA_converted` suffix (Perl-faithful "Chromosome
/// number extraction failed").
pub fn classify(flag: u16, rname: &str) -> Result<(String, CombinedClass)> {
    let reverse = flag & 0x10 != 0;
    if let Some(chrom) = rname.strip_suffix("_CT_converted") {
        // CT sub-genome: fwd → OT, rev → spurious.
        let class = if reverse {
            CombinedClass::Spurious
        } else {
            CombinedClass::Ot
        };
        Ok((chrom.to_string(), class))
    } else if let Some(chrom) = rname.strip_suffix("_GA_converted") {
        // GA sub-genome: rev → OB, fwd → spurious.
        let class = if reverse {
            CombinedClass::Ob
        } else {
            CombinedClass::Spurious
        };
        Ok((chrom.to_string(), class))
    } else {
        Err(AlignerError::Validation(format!(
            "Chromosome number extraction failed for {rname}"
        )))
    }
}

/// Provisional per-read selection over the combined instance's `-k` line group
/// (PLAN §3.6). `records` are the (≤ k) alignment lines Bowtie 2 emitted for one
/// read, in its output order; `sequence` is the original (uc) read (its length
/// feeds `calc_mapq`). **PROVISIONAL** — Phase 3 installs the faithful tie
/// resolution. OWNS the alignment-outcome counters the bypassed merge would bump
/// (`unique_best_alignment_count` / `unsuitable_sequence_count` /
/// `no_single_alignment_found`; plus `combined_spurious_count` for visibility).
///
/// Rules:
/// - no mapped line → `NoAlignment` (Bowtie 2 emits one FLAG-4 line for a miss).
/// - ≥ 2 lines tied at the best `AS` (valid/valid OR valid/spurious) → `Ambiguous`
///   (the spike rule: a valid hit *tied with* a spurious hit is ambiguous — never
///   rescued by discarding the spurious side).
/// - exactly one best-`AS` line, classifying valid → `UniqueBest{index: 0|1}`.
/// - exactly one best-`AS` line, classifying spurious → `NoAlignment` (a
///   spurious-best read the faithful 2-instance path never sees; counted).
pub fn select(
    records: &[SamRecord],
    sequence: &str,
    score_min_intercept: f64,
    score_min_slope: f64,
    counters: &mut Counters,
) -> Result<Decision> {
    // Keep only mapped lines (Bowtie 2 emits a single FLAG-4 line for a miss).
    let mut mapped: Vec<&SamRecord> = records.iter().filter(|r| !r.is_unmapped()).collect();
    if mapped.is_empty() {
        counters.no_single_alignment_found += 1;
        return Ok(Decision::NoAlignment);
    }

    // AS is mandatory on a mapped record (Perl `die` 2838).
    for r in &mapped {
        if r.alignment_score.is_none() {
            return Err(AlignerError::Validation(format!(
                "Failed to extract alignment score from line {}",
                r.raw_line
            )));
        }
    }
    // Highest AS first; the runner-up (if any) is then `mapped[1]`.
    mapped.sort_by_key(|r| std::cmp::Reverse(r.alignment_score.unwrap()));
    let best_as = mapped[0].alignment_score.unwrap();

    // ≥ 2 alignments tied at the best AS → ambiguous (PROVISIONAL; Phase 3
    // refines via the faithful chr:pos + Foret rule).
    let tied_at_best = mapped
        .iter()
        .filter(|r| r.alignment_score.unwrap() == best_as)
        .count();
    if tied_at_best >= 2 {
        counters.unsuitable_sequence_count += 1;
        return Ok(Decision::Ambiguous { first_ambig: None });
    }

    // Exactly one best-AS line.
    let best = mapped[0];
    let (chromosome, class) = classify(best.flag, &best.rname)?;
    let Some(index) = class.to_index() else {
        // Spurious-best: no valid OT/OB hit at the top AS. PROVISIONAL — treat as
        // no alignment (routed to --unmapped); counted (incl. the spurious tally).
        counters.no_single_alignment_found += 1;
        counters.combined_spurious_count += 1;
        return Ok(Decision::NoAlignment);
    };

    // MD is mandatory on a mapped record (Perl `die` 2838).
    let md_tag = best.md_tag.clone().ok_or_else(|| {
        AlignerError::Validation(format!(
            "Failed to extract MD tag from line {}",
            best.raw_line
        ))
    })?;

    // Provisional second-best for MAPQ (PLAN §3.6): the runner-up `-k` line's AS
    // if a second line exists, else `None`. `None` vs `Some` flips a byte-visible
    // `calc_mapq` ladder branch, so this choice is pinned.
    let second_best = mapped.get(1).map(|r| r.alignment_score.unwrap());
    let mapq = calc_mapq(
        sequence.len(),
        None,
        best_as,
        second_best,
        score_min_intercept,
        score_min_slope,
    );

    counters.unique_best_alignment_count += 1;
    Ok(Decision::UniqueBest(BestAlignment {
        chromosome,
        position: best.pos,
        index,
        alignment_score: best_as,
        alignment_score_second_best: second_best,
        md_tag,
        cigar: best.cigar.clone(),
        bowtie_sequence: best.seq.clone(),
        mapq,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- classify: RNAME suffix × FLAG orientation -------------------------

    #[test]
    fn classify_fwd_ct_is_ot() {
        let (chr, c) = classify(0, "chr1_CT_converted").unwrap();
        assert_eq!(chr, "chr1");
        assert_eq!(c, CombinedClass::Ot);
        assert_eq!(c.to_index(), Some(0));
    }

    #[test]
    fn classify_rev_ga_is_ob() {
        let (chr, c) = classify(16, "chr7_GA_converted").unwrap();
        assert_eq!(chr, "chr7");
        assert_eq!(c, CombinedClass::Ob);
        // The headline correction: OB → index 1 (NOT 3).
        assert_eq!(c.to_index(), Some(1));
    }

    #[test]
    fn classify_fwd_ga_is_spurious() {
        let (_chr, c) = classify(0, "chr1_GA_converted").unwrap();
        assert_eq!(c, CombinedClass::Spurious);
        assert_eq!(c.to_index(), None);
    }

    #[test]
    fn classify_rev_ct_is_spurious() {
        let (_chr, c) = classify(16, "chr1_CT_converted").unwrap();
        assert_eq!(c, CombinedClass::Spurious);
        assert_eq!(c.to_index(), None);
    }

    #[test]
    fn classify_missing_suffix_errors() {
        assert!(classify(0, "chr1").is_err());
        assert!(classify(0, "chr1_converted").is_err());
    }

    // ---- select: provisional uniqueness + counter ownership + MAPQ ---------

    /// Build one mapped SAM line. `flag` 0 = fwd, 16 = rev.
    fn line(rname: &str, flag: u16, pos: u32, as_i: i64, md: &str) -> SamRecord {
        SamRecord::parse(&format!(
            "r1\t{flag}\t{rname}\t{pos}\t40\t6M\t*\t0\t0\tACGTAC\tIIIIII\tAS:i:{as_i}\tMD:Z:{md}"
        ))
        .unwrap()
    }
    fn unmapped() -> SamRecord {
        SamRecord::parse("r1\t4\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tIIIIII").unwrap()
    }
    fn sel(records: &[SamRecord]) -> (Decision, Counters) {
        let mut c = Counters::default();
        let d = select(records, "ACGTAC", 0.0, -0.2, &mut c).unwrap();
        (d, c)
    }

    #[test]
    fn select_unique_ot_maps_to_index_0() {
        let (d, c) = sel(&[line("chr1_CT_converted", 0, 10, 0, "6")]);
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.index, 0); // OT
                assert_eq!(b.chromosome, "chr1");
                assert_eq!(b.position, 10);
                assert_eq!(b.alignment_score, 0);
                assert_eq!(b.alignment_score_second_best, None); // single line → None
            }
            other => panic!("expected UniqueBest, got {other:?}"),
        }
        assert_eq!(c.unique_best_alignment_count, 1);
    }

    #[test]
    fn select_unique_ob_maps_to_index_1() {
        // The OB→1 (not 3) regression: a lone rev+GA line → index 1.
        let (d, c) = sel(&[line("chr2_GA_converted", 16, 25, 0, "6")]);
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.index, 1); // OB — NOT 3 (CTOB)
                assert_eq!(b.chromosome, "chr2");
            }
            other => panic!("expected UniqueBest, got {other:?}"),
        }
        assert_eq!(c.unique_best_alignment_count, 1);
    }

    #[test]
    fn select_unique_best_with_runner_up_sets_second_best() {
        // OT best (AS 0) + a lower spurious runner-up (AS -6): unique best, and the
        // provisional MAPQ second-best is the runner-up's AS (PLAN §3.6 pinned rule).
        let (d, c) = sel(&[
            line("chr1_CT_converted", 0, 10, 0, "6"),
            line("chr1_CT_converted", 16, 99, -6, "6"), // rev+CT = spurious, lower AS
        ]);
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.index, 0);
                assert_eq!(b.alignment_score_second_best, Some(-6));
            }
            other => panic!("expected UniqueBest, got {other:?}"),
        }
        assert_eq!(c.unique_best_alignment_count, 1);
    }

    #[test]
    fn select_tied_valid_valid_is_ambiguous() {
        let (d, c) = sel(&[
            line("chr1_CT_converted", 0, 10, 0, "6"),  // OT
            line("chr2_GA_converted", 16, 25, 0, "6"), // OB, same AS → tie
        ]);
        assert_eq!(d, Decision::Ambiguous { first_ambig: None });
        assert_eq!(c.unsuitable_sequence_count, 1);
        assert_eq!(c.unique_best_alignment_count, 0);
    }

    #[test]
    fn select_valid_tied_with_spurious_is_ambiguous() {
        // The spike rule: a valid OT tied (same AS) with a spurious hit is
        // AMBIGUOUS — NOT rescued by discarding the spurious side.
        let (d, c) = sel(&[
            line("chr1_CT_converted", 0, 10, 0, "6"),  // OT (valid)
            line("chr1_CT_converted", 16, 50, 0, "6"), // rev+CT spurious, SAME AS
        ]);
        assert_eq!(d, Decision::Ambiguous { first_ambig: None });
        assert_eq!(c.unsuitable_sequence_count, 1);
    }

    #[test]
    fn select_spurious_only_best_is_no_alignment() {
        let (d, c) = sel(&[line("chr1_GA_converted", 0, 10, 0, "6")]); // fwd+GA = spurious
        assert_eq!(d, Decision::NoAlignment);
        assert_eq!(c.no_single_alignment_found, 1);
        assert_eq!(c.combined_spurious_count, 1);
        assert_eq!(c.unique_best_alignment_count, 0);
    }

    #[test]
    fn select_unmapped_only_is_no_alignment() {
        let (d, c) = sel(&[unmapped()]);
        assert_eq!(d, Decision::NoAlignment);
        assert_eq!(c.no_single_alignment_found, 1);
        assert_eq!(c.combined_spurious_count, 0); // unmapped is not "spurious"
    }

    #[test]
    fn select_empty_is_no_alignment() {
        let (d, c) = sel(&[]);
        assert_eq!(d, Decision::NoAlignment);
        assert_eq!(c.no_single_alignment_found, 1);
    }

    /// A spurious runner-up below a valid best does NOT make the read ambiguous
    /// (only a TIE at the best AS does) — the valid hit wins, spurious feeds MAPQ.
    #[test]
    fn select_lower_spurious_does_not_block_valid_best() {
        let (d, _c) = sel(&[
            line("chr3_GA_converted", 16, 40, 0, "6"), // OB best (AS 0)
            line("chr3_CT_converted", 16, 80, -3, "6"), // rev+CT spurious, lower AS
        ]);
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.index, 1); // OB
                assert_eq!(b.alignment_score_second_best, Some(-3));
            }
            other => panic!("expected UniqueBest, got {other:?}"),
        }
    }
}

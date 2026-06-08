//! `--combined_index` (v2) strand classification + per-read selection.
//!
//! **Opt-in, never-silent, concordance-gated — NOT byte-identical** to the
//! faithful per-strand-instance path. The combined index is searched in ONE
//! both-strands Bowtie 2 pass per read-conversion (`-k 2`, no `--norc`/`--nofw`);
//! each alignment's strand is recovered from its **read-conversion × RNAME suffix
//! (`_CT_converted`/`_GA_converted`) × FLAG orientation** (the spike-validated
//! rule — [`classify`]):
//!
//! | read pass | sub-genome | fwd (FLAG&0x10==0) | rev (FLAG&0x10!=0) |
//! |-----------|------------|--------------------|--------------------|
//! | C→T       | CT         | **OT** (idx 0)     | spurious           |
//! | C→T       | GA         | spurious           | **OB** (idx 1)     |
//! | G→A       | CT         | spurious           | **CTOT** (idx 2)   |
//! | G→A       | GA         | **CTOB** (idx 3)   | spurious           |
//!
//! - **Directional** ([`select`]) runs ONE C→T pass → OT/OB (indices 0/1).
//! - **Non-directional** ([`select_nondir`]) runs a C→T pass AND a G→A pass and
//!   UNIONs them per read → OT/OB/CTOT/CTOB (indices 0–3).
//!
//! This module is deliberately a SIBLING of `merge.rs`, NOT a path through it:
//! the faithful `check_results_single_end` keys off **separate strand-restricted
//! instance streams**, whereas the combined search emits one stream per pass whose
//! `-k` runner-up is computed across *both* sub-genomes (so the per-instance
//! lockstep's within-thread-vs-cross-instance ambiguity semantics do not map).
//! Instead, the selectors emit a [`merge::Decision`] carrying a [`BestAlignment`]
//! with a **synthetic instance index** (0–3) so the byte-frozen output arm
//! (`extract_corresponding_genomic_sequence_single_end` → `methylation_call` →
//! `single_end_sam_output`) — which is keyed purely on `best.index` — is reused
//! UNCHANGED. Because the merge is bypassed, the selectors also OWN the
//! alignment-outcome counters the merge would normally bump.
//!
//! Both selectors funnel through the single shared [`select_core`], which is the
//! Bismark-faithful same-position tie resolution (`chr:pos` + `>=` + Sylvain-Foret
//! better-`AS`-trumps; `bismark` l.2798–2892, ported in `merge.rs`). Keeping ONE
//! tie core means the directional and 4-strand paths cannot drift — the
//! mechanism-vs-oracle tests cross-check it against `merge.rs` on identical inputs.

use std::collections::HashMap;

use crate::align::SamRecord;
use crate::error::{AlignerError, Result};
use crate::mapq::calc_mapq;
use crate::merge::{BestAlignment, Counters, Decision};

/// Which converted read pass an alignment came from — i.e. which converted read
/// file Bowtie 2 was fed. This selects the strand-classification table
/// ([`classify`]): the C→T pass yields OT/OB, the G→A pass yields CTOT/CTOB.
///
/// **Why this is NECESSARY (not incidental):** an identical SAM line — e.g.
/// `rev + chrN_CT_converted` — is **spurious** in the C→T pass but a **valid
/// CTOT** in the G→A pass. The class cannot be derived from RNAME-suffix ×
/// orientation alone; it depends on which converted read produced the alignment.
/// That is why the directional `classify` cannot be reused unparameterized for
/// non-directional libraries (PLAN 06072026 phase 5 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadConv {
    /// C→T-converted read pass (directional OT/OB; the non-dir first pass).
    Ct,
    /// G→A-converted read pass (the non-dir second pass → CTOT/CTOB).
    Ga,
}

/// Strand classification of one combined-index alignment. The four valid classes
/// are the Bismark strands OT/OB (C→T-read pass) + CTOT/CTOB (G→A-read pass);
/// everything else is spurious (wrong sub-genome for the orientation/pass).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombinedClass {
    /// C→T read, fwd + `_CT_converted` → original top strand (OT).
    Ot,
    /// C→T read, rev + `_GA_converted` → original bottom strand (OB).
    Ob,
    /// G→A read, rev + `_CT_converted` → complementary-to-OT strand (CTOT).
    Ctot,
    /// G→A read, fwd + `_GA_converted` → complementary-to-OB strand (CTOB).
    Ctob,
    /// Wrong sub-genome for the orientation/pass → discarded (never-silent).
    Spurious,
}

impl CombinedClass {
    /// The synthetic instance index for the byte-frozen output arm (the
    /// `methylation.rs` index→strand selector: `0 → (+,CT,CT)=OT`,
    /// `1 → (-,CT,GA)=OB`, `2 → (-,GA,CT)=CTOT`, `3 → (+,GA,GA)=CTOB`). Spurious
    /// has no valid index. **NB:** OB is `1`, NOT `3` — index 3 is CTOB and would
    /// emit the wrong FLAG-arm / `XR`/`XG` / methylation branch for an OB read.
    fn to_index(self) -> Option<usize> {
        match self {
            CombinedClass::Ot => Some(0),
            CombinedClass::Ob => Some(1),
            CombinedClass::Ctot => Some(2),
            CombinedClass::Ctob => Some(3),
            CombinedClass::Spurious => None,
        }
    }
}

/// Classify one alignment by **read-conversion × RNAME suffix × FLAG
/// orientation**, returning the de-converted (suffix-stripped) chromosome + the
/// class (PLAN 06072026 phase 5 §3.2). Errors if the RNAME lacks a
/// `_CT_converted`/`_GA_converted` suffix (Perl-faithful "Chromosome number
/// extraction failed").
///
/// | read_conv | sub-genome | orientation | class    |
/// |-----------|------------|-------------|----------|
/// | C→T       | CT         | fwd         | OT       |
/// | C→T       | GA         | rev         | OB       |
/// | G→A       | CT         | rev         | CTOT     |
/// | G→A       | GA         | fwd         | CTOB     |
/// | (any other read_conv × sub-genome × orientation) |     | Spurious |
///
/// The C→T rows are the shipped directional table; the G→A rows are the non-dir
/// extension, and the *valid* orientation per sub-genome FLIPS between the two
/// passes (so the spurious set flips too).
pub fn classify(read_conv: ReadConv, flag: u16, rname: &str) -> Result<(String, CombinedClass)> {
    let reverse = flag & 0x10 != 0;
    if let Some(chrom) = rname.strip_suffix("_CT_converted") {
        // CT sub-genome: C→T-read fwd → OT; G→A-read rev → CTOT; else spurious.
        let class = match (read_conv, reverse) {
            (ReadConv::Ct, false) => CombinedClass::Ot,
            (ReadConv::Ga, true) => CombinedClass::Ctot,
            _ => CombinedClass::Spurious,
        };
        Ok((chrom.to_string(), class))
    } else if let Some(chrom) = rname.strip_suffix("_GA_converted") {
        // GA sub-genome: C→T-read rev → OB; G→A-read fwd → CTOB; else spurious.
        let class = match (read_conv, reverse) {
            (ReadConv::Ct, true) => CombinedClass::Ob,
            (ReadConv::Ga, false) => CombinedClass::Ctob,
            _ => CombinedClass::Spurious,
        };
        Ok((chrom.to_string(), class))
    } else {
        Err(AlignerError::Validation(format!(
            "Chromosome number extraction failed for {rname}"
        )))
    }
}

/// One valid (OT/OB) alignment kept for the `chr:pos` map — mirrors `merge::Stored`.
struct Cand {
    chromosome: String,
    position: u32,
    index: usize,
    alignment_score: i64,
    md_tag: String,
    cigar: String,
    bowtie_sequence: String,
}

/// Directional combined-index per-read selection over the combined instance's
/// `-k` line group: every line is from the C→T-converted read pass
/// ([`ReadConv::Ct`]). A thin wrapper over [`select_core`] (the shared tie
/// machine) — **behaviour-identical to the shipped Phase-3 `select`** (the only
/// directional lines are OT/OB, indices 0/1). `records` are the (≤ k) lines Bowtie 2
/// emitted for one read; `sequence` is the original (uc) read (length feeds
/// `calc_mapq`).
pub fn select(
    records: &[SamRecord],
    sequence: &str,
    score_min_intercept: f64,
    score_min_slope: f64,
    counters: &mut Counters,
) -> Result<Decision> {
    // Keep only mapped lines (Bowtie 2 emits a single FLAG-4 line for a miss),
    // each tagged C→T (the directional pass). The shared core does the rest.
    let mapped: Vec<(ReadConv, &SamRecord)> = records
        .iter()
        .filter(|r| !r.is_unmapped())
        .map(|r| (ReadConv::Ct, r))
        .collect();
    select_core(
        mapped,
        sequence,
        score_min_intercept,
        score_min_slope,
        counters,
    )
}

/// The shared **Bismark-faithful same-position tie-resolution core** (PLAN 06072026
/// phase 3 + phase 5 §4; `bismark` l.2798–2892, ported in `merge.rs`). `mapped` is
/// the read's FULL set of mapped lines, each tagged with the [`ReadConv`] of the
/// pass that produced it — directional ([`select`]) tags every line `Ct`;
/// non-directional ([`select_nondir`]) tags the C→T pass's lines `Ct` and the G→A
/// pass's lines `Ga`. This is the SINGLE source of truth for the tie logic, so the
/// directional and 4-strand paths cannot drift (the §9 mechanism-vs-oracle test
/// guards it). OWNS the alignment-outcome counters the bypassed merge would bump.
///
/// Decision precedence (PLAN §3):
/// 1. no mapped line → `NoAlignment`.
/// 2. compute the GLOBAL best `AS` over ALL mapped (valid + spurious) + the Phase-2
///    MAPQ runner-up (`mapped[1]` after the desc sort — over the FULL set, NOT the
///    filtered `top`, so it matches the shipped directional rule; review A-Imp1/B-I2);
///    `top` = the candidates at the best `AS`, classified with each record's own
///    `ReadConv`.
/// 3. if `top` has any **spurious** hit (a wrong-sub-genome/-pass competitor at the
///    top score the faithful run never searched): also-valid → `Ambiguous`;
///    all-spurious → `NoAlignment` (+ `combined_spurious_count`). This single branch
///    also covers a spurious hit STRICTLY better than every valid hit — never a
///    silent rescue.
/// 4. else (`top` all valid): build a `chr:pos` map of `top`, processed in canonical
///    ascending-slot order (OT=0, OB=1, CTOT=2, CTOB=3) with `>=` overwrite
///    (later-equal replaces) — so a same-locus equal-`AS` collision collapses to ONE
///    entry won by the **HIGHEST index** (OB over OT; **CTOB over OT**; **CTOT over
///    OB**), exactly as `merge.rs`'s `@fhs`-order overwrite. 1 entry → `UniqueBest`
///    (KEPT, incl. the degenerate telomeric case); ≥2 entries (distinct loci) →
///    `Ambiguous` (cross-location); the `>4` guard is kept verbatim from merge
///    (unreachable under `-k 2`/pass — union ≤4).
///
/// MAPQ second-best is the Phase-2 rule (runner-up `AS` among ALL mapped, or `None`)
/// — it legitimately differs from merge's within-instance `XS`, so it is excluded
/// from the mechanism-vs-oracle test.
fn select_core(
    mut mapped: Vec<(ReadConv, &SamRecord)>,
    sequence: &str,
    score_min_intercept: f64,
    score_min_slope: f64,
    counters: &mut Counters,
) -> Result<Decision> {
    if mapped.is_empty() {
        counters.no_single_alignment_found += 1;
        return Ok(Decision::NoAlignment);
    }

    // AS is mandatory on a mapped record (Perl `die` 2838).
    for (_, r) in &mapped {
        if r.alignment_score.is_none() {
            return Err(AlignerError::Validation(format!(
                "Failed to extract alignment score from line {}",
                r.raw_line
            )));
        }
    }
    // Highest AS first → the GLOBAL best AS + the Phase-2 MAPQ runner-up (`mapped[1]`).
    mapped.sort_by_key(|(_, r)| std::cmp::Reverse(r.alignment_score.unwrap()));
    let best_as = mapped[0].1.alignment_score.unwrap();
    // MAPQ second-best (Phase-2 rule, UNCHANGED): runner-up AS among ALL mapped, or
    // None if the winner is the sole mapped candidate (PLAN §3.7). Computed over the
    // full `mapped` set (not `top`), so directional MAPQ is byte-unchanged.
    let second_best = mapped.get(1).map(|(_, r)| r.alignment_score.unwrap());

    // `top` = candidates at the global best AS; classify each with ITS pass's
    // `ReadConv` (Ct → OT/OB, Ga → CTOT/CTOB), noting spurious.
    let mut valid_top: Vec<(String, usize, &SamRecord)> = Vec::new();
    let mut any_spurious = false;
    for (rc, r) in mapped
        .iter()
        .filter(|(_, r)| r.alignment_score.unwrap() == best_as)
    {
        let (chrom, class) = classify(*rc, r.flag, &r.rname)?;
        match class.to_index() {
            Some(index) => valid_top.push((chrom, index, r)),
            None => any_spurious = true,
        }
    }

    // §3.3 spurious branch (on the GLOBAL best AS — pins "spurious strictly better
    // than the valid best" → NoAlignment, never a silent UniqueBest rescue).
    if any_spurious {
        if valid_top.is_empty() {
            counters.no_single_alignment_found += 1;
            counters.combined_spurious_count += 1;
            return Ok(Decision::NoAlignment);
        }
        counters.unsuitable_sequence_count += 1;
        return Ok(Decision::Ambiguous { first_ambig: None });
    }

    // §3.4 `top` is all valid → build the `chr:pos` map in canonical ascending-slot
    // order (OT=0, OB=1, CTOT=2, CTOB=3) so the `>=` overwrite leaves the HIGHEST
    // index the winner of a same-locus equal-AS collision (directional: OB over OT;
    // non-dir: CTOB over OT, CTOT over OB), reproducing `merge.rs::insert_alignment`
    // + the l.258 `@fhs`-order overwrite. (Restricting the map to the best-AS valid
    // set is load-bearing for Foret: a worse-AS slot is never in `top`, so it can
    // never overwrite a better-AS one — Q4.)
    valid_top.sort_by_key(|(_, index, _)| *index);
    let mut map: HashMap<String, Cand> = HashMap::new();
    for (chrom, index, r) in &valid_top {
        // MD mandatory on every mapped record entered into the map (Perl `die` 2838).
        let md_tag = r.md_tag.clone().ok_or_else(|| {
            AlignerError::Validation(format!("Failed to extract MD tag from line {}", r.raw_line))
        })?;
        map.insert(
            format!("{chrom}:{}", r.pos),
            Cand {
                chromosome: chrom.clone(),
                position: r.pos,
                index: *index,
                alignment_score: best_as,
                md_tag,
                cigar: r.cigar.clone(),
                bowtie_sequence: r.seq.clone(),
            },
        );
    }

    // §3.5 selection tail (the safely-shareable part of `merge.rs` l.322–349).
    // Every map entry is at `best_as` (Q4 restriction), so ≥2 entries is necessarily
    // a cross-location equal-AS tie (= merge's `entries[0].AS==entries[1].AS` branch,
    // l.322–326). Merge's clear-best/runner-up branch (l.327–335) is **winner-
    // equivalent-by-omission** here: it fires only when entries differ in AS, but a
    // below-best-AS hit is never in `top`, so it never reaches the map nor overwrites
    // a best-AS entry — the best-AS winner merge would pick is exactly this set's sole
    // survivor. Only merge's MAPQ *runner-up* (l.330–334) is lost, and that is
    // supplied separately by the Phase-2 `second_best` above (excluded from §9). The
    // `>4` guard is kept verbatim (unreachable under `-k 2`).
    let mut entries: Vec<Cand> = map.into_values().collect();
    if entries.len() > 4 {
        return Err(AlignerError::Validation(format!(
            "There are too many potential hits for this sequence (1-4 expected, but found: {})",
            entries.len()
        )));
    }
    if entries.len() >= 2 {
        counters.unsuitable_sequence_count += 1;
        return Ok(Decision::Ambiguous { first_ambig: None });
    }
    let best = entries.pop().expect("top all-valid → ≥1 map entry");

    let mapq = calc_mapq(
        sequence.len(),
        None,
        best.alignment_score,
        second_best,
        score_min_intercept,
        score_min_slope,
    );
    counters.unique_best_alignment_count += 1;
    Ok(Decision::UniqueBest(BestAlignment {
        chromosome: best.chromosome,
        position: best.position,
        index: best.index,
        alignment_score: best.alignment_score,
        alignment_score_second_best: second_best,
        md_tag: best.md_tag,
        cigar: best.cigar,
        bowtie_sequence: best.bowtie_sequence,
        mapq,
    }))
}

/// Non-directional combined-index per-read selection over the UNION of the two
/// passes' `-k` line groups (PLAN 06072026 phase 5 §3): the C→T-converted-read
/// pass (`ct_records` → OT/OB) and the G→A-converted-read pass (`ga_records` →
/// CTOT/CTOB). Each pass's mapped lines are tagged with its [`ReadConv`], unioned,
/// and fed to the shared [`select_core`] tie machine across all four slots (OT=0,
/// OB=1, CTOT=2, CTOB=3). A same-locus equal-`AS` collision is KEPT, won by the
/// later slot: **OT×CTOB → CTOB (index 3)**, **OB×CTOT → CTOT (index 2)** — the
/// `SPIKE…nondirectional.md` §4b telomeric productionization requirement. `sequence`
/// is the original (uc) read; counters are owned by `select_core` (the same set the
/// directional `select` bumps). `pbat` is irrelevant here (non-dir is its own mode).
pub fn select_nondir(
    ct_records: &[SamRecord],
    ga_records: &[SamRecord],
    sequence: &str,
    score_min_intercept: f64,
    score_min_slope: f64,
    counters: &mut Counters,
) -> Result<Decision> {
    // Keep only mapped lines from each pass (Bowtie 2 emits one FLAG-4 line for a
    // miss — the common ~50%-per-pass non-dir case), tag with the pass's read-
    // conversion, and union. The shared core computes the global best AS + the
    // MAPQ runner-up over this union (PLAN §3.8) and resolves ties across all four
    // slots in ascending-index order.
    let mapped: Vec<(ReadConv, &SamRecord)> = ct_records
        .iter()
        .filter(|r| !r.is_unmapped())
        .map(|r| (ReadConv::Ct, r))
        .chain(
            ga_records
                .iter()
                .filter(|r| !r.is_unmapped())
                .map(|r| (ReadConv::Ga, r)),
        )
        .collect();
    select_core(
        mapped,
        sequence,
        score_min_intercept,
        score_min_slope,
        counters,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- classify: RNAME suffix × FLAG orientation -------------------------

    // --- C→T-read pass (directional OT/OB; non-dir first pass) ---

    #[test]
    fn classify_ct_fwd_ct_is_ot() {
        let (chr, c) = classify(ReadConv::Ct, 0, "chr1_CT_converted").unwrap();
        assert_eq!(chr, "chr1");
        assert_eq!(c, CombinedClass::Ot);
        assert_eq!(c.to_index(), Some(0));
    }

    #[test]
    fn classify_ct_rev_ga_is_ob() {
        let (chr, c) = classify(ReadConv::Ct, 16, "chr7_GA_converted").unwrap();
        assert_eq!(chr, "chr7");
        assert_eq!(c, CombinedClass::Ob);
        // The headline correction: OB → index 1 (NOT 3).
        assert_eq!(c.to_index(), Some(1));
    }

    #[test]
    fn classify_ct_fwd_ga_is_spurious() {
        let (_chr, c) = classify(ReadConv::Ct, 0, "chr1_GA_converted").unwrap();
        assert_eq!(c, CombinedClass::Spurious);
        assert_eq!(c.to_index(), None);
    }

    #[test]
    fn classify_ct_rev_ct_is_spurious() {
        let (_chr, c) = classify(ReadConv::Ct, 16, "chr1_CT_converted").unwrap();
        assert_eq!(c, CombinedClass::Spurious);
        assert_eq!(c.to_index(), None);
    }

    // --- G→A-read pass (non-dir second pass → CTOT/CTOB) — the new code.
    // The *valid* orientation per sub-genome FLIPS vs the C→T pass, so the
    // spurious set flips too (PLAN §3.2). ---

    #[test]
    fn classify_ga_rev_ct_is_ctot() {
        // rev + CT (G→A read) → CTOT — the SAME (rev,CT) line that is spurious
        // in the C→T pass.
        let (chr, c) = classify(ReadConv::Ga, 16, "chr3_CT_converted").unwrap();
        assert_eq!(chr, "chr3");
        assert_eq!(c, CombinedClass::Ctot);
        assert_eq!(c.to_index(), Some(2));
    }

    #[test]
    fn classify_ga_fwd_ga_is_ctob() {
        // fwd + GA (G→A read) → CTOB — the SAME (fwd,GA) line that is spurious
        // in the C→T pass.
        let (chr, c) = classify(ReadConv::Ga, 0, "chr5_GA_converted").unwrap();
        assert_eq!(chr, "chr5");
        assert_eq!(c, CombinedClass::Ctob);
        assert_eq!(c.to_index(), Some(3));
    }

    #[test]
    fn classify_ga_fwd_ct_is_spurious() {
        // fwd + CT under the G→A pass → spurious (it would be OT for a C→T read).
        let (_chr, c) = classify(ReadConv::Ga, 0, "chr1_CT_converted").unwrap();
        assert_eq!(c, CombinedClass::Spurious);
        assert_eq!(c.to_index(), None);
    }

    #[test]
    fn classify_ga_rev_ga_is_spurious() {
        // rev + GA under the G→A pass → spurious (it would be OB for a C→T read).
        let (_chr, c) = classify(ReadConv::Ga, 16, "chr1_GA_converted").unwrap();
        assert_eq!(c, CombinedClass::Spurious);
        assert_eq!(c.to_index(), None);
    }

    #[test]
    fn classify_missing_suffix_errors() {
        assert!(classify(ReadConv::Ct, 0, "chr1").is_err());
        assert!(classify(ReadConv::Ct, 0, "chr1_converted").is_err());
        assert!(classify(ReadConv::Ga, 0, "chr1").is_err());
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

    // ---- Phase 3: faithful same-position tie resolution --------------------

    /// THE CORE FIX: an OT and an OB hit at the SAME de-converted `chr:pos` with
    /// equal AS collapse to ONE entry → KEPT (not Ambiguous), won by **OB (index
    /// 1)** via the `>=` overwrite in OT-then-OB order. Phase 2 wrongly discarded it.
    #[test]
    fn select_same_position_collision_kept_ob_wins() {
        let (d, c) = sel(&[
            line("chr1_CT_converted", 0, 100, 0, "6"),  // OT @ chr1:100
            line("chr1_GA_converted", 16, 100, 0, "6"), // OB @ chr1:100 (same locus), equal AS
        ]);
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.chromosome, "chr1");
                assert_eq!(b.position, 100);
                assert_eq!(b.index, 1); // OB wins the same-position equal-AS tie
            }
            other => panic!("expected UniqueBest (KEPT), got {other:?}"),
        }
        assert_eq!(c.unique_best_alignment_count, 1);
        assert_eq!(c.unsuitable_sequence_count, 0); // NOT ambiguous
    }

    /// Foret: a same-position tie with UNEQUAL AS → the better-AS strand wins
    /// (it alone is in `top`), KEPT — regardless of slot order.
    #[test]
    fn select_same_position_foret_better_as_wins() {
        // OT better → OT (index 0).
        let (d, _) = sel(&[
            line("chr1_CT_converted", 0, 100, 0, "6"), // OT AS 0 (better)
            line("chr1_GA_converted", 16, 100, -6, "6"), // OB AS -6 (worse) — not in top
        ]);
        assert!(matches!(d, Decision::UniqueBest(ref b) if b.index == 0));
        // OB better → OB (index 1).
        let (d, _) = sel(&[
            line("chr1_CT_converted", 0, 100, -6, "6"), // OT AS -6 (worse)
            line("chr1_GA_converted", 16, 100, 0, "6"), // OB AS 0 (better)
        ]);
        assert!(matches!(d, Decision::UniqueBest(ref b) if b.index == 1));
    }

    /// Re-review B-Important-1: a SPURIOUS hit strictly better than the only valid
    /// hit → `NoAlignment` (NOT a silent UniqueBest rescue). The global-best-AS
    /// branch runs before the valid map, so the lower valid hit never competes.
    #[test]
    fn select_spurious_strictly_better_than_valid_is_no_alignment() {
        let (d, c) = sel(&[
            line("chr1_GA_converted", 0, 10, 0, "6"), // fwd+GA = spurious, AS 0 (best)
            line("chr1_CT_converted", 0, 50, -6, "6"), // OT valid, AS -6 (below best)
        ]);
        assert_eq!(d, Decision::NoAlignment);
        assert_eq!(c.no_single_alignment_found, 1);
        assert_eq!(c.combined_spurious_count, 1);
        assert_eq!(c.unique_best_alignment_count, 0);
    }

    /// A spurious hit at the SAME position as a valid hit, equal best AS → still
    /// `Ambiguous` (spurious never collapses into the valid entry).
    #[test]
    fn select_same_position_spurious_with_valid_is_ambiguous() {
        let (d, c) = sel(&[
            line("chr1_CT_converted", 0, 100, 0, "6"), // OT valid
            line("chr1_GA_converted", 0, 100, 0, "6"), // fwd+GA spurious, SAME pos, equal AS
        ]);
        assert_eq!(d, Decision::Ambiguous { first_ambig: None });
        assert_eq!(c.unsuitable_sequence_count, 1);
    }

    // ---- mechanism-vs-oracle cross-test (the anti-drift §9 gate) -----------
    // Feed identical hand-built lines to BOTH combined::select and the faithful
    // merge oracle (slot 0 = OT, slot 1 = OB); assert the SAME tie outcome —
    // Decision variant + (chromosome, position, index), PROJECTING OUT mapq +
    // alignment_score_second_best (they legitimately differ — merge uses within-
    // instance XS, combined uses the raw runner-up).

    /// A minimal `SamStream` double over canned records (no subprocess).
    struct VecStream {
        recs: Vec<SamRecord>,
        pos: usize,
    }
    impl crate::align::SamStream for VecStream {
        fn current(&self) -> Option<&SamRecord> {
            self.recs.get(self.pos)
        }
        fn advance(&mut self) -> Result<()> {
            self.pos += 1;
            Ok(())
        }
    }

    /// Run the faithful merge oracle with slot 0 = `ot`, slot 1 = `ob`.
    fn oracle(ot: SamRecord, ob: SamRecord) -> Decision {
        let mut streams = vec![
            VecStream {
                recs: vec![ot],
                pos: 0,
            },
            VecStream {
                recs: vec![ob],
                pos: 0,
            },
        ];
        let mut c = Counters::default();
        crate::merge::check_results_single_end(
            "r1",
            "ACGTAC",
            &mut streams,
            true, // directional
            0.0,
            -0.2,
            false,
            &mut c,
        )
        .unwrap()
    }

    /// Project a Decision to the tie-relevant identity (mapq + second-best excluded).
    fn key(d: &Decision) -> Option<(String, u32, usize)> {
        match d {
            Decision::UniqueBest(b) => Some((b.chromosome.clone(), b.position, b.index)),
            _ => None,
        }
    }

    #[test]
    fn mechanism_matches_oracle_same_position_collision() {
        let ot = line("chr1_CT_converted", 0, 100, 0, "6");
        let ob = line("chr1_GA_converted", 16, 100, 0, "6"); // both → chr1:100, equal AS
        let (d_comb, _) = sel(&[ot.clone(), ob.clone()]);
        let d_orac = oracle(ot, ob);
        assert_eq!(key(&d_comb), key(&d_orac)); // same KEEP outcome
        assert_eq!(key(&d_comb), Some(("chr1".to_string(), 100, 1))); // OB wins
    }

    #[test]
    fn mechanism_matches_oracle_cross_location_tie() {
        let ot = line("chr1_CT_converted", 0, 10, 0, "6");
        let ob = line("chr2_GA_converted", 16, 25, 0, "6"); // distinct loci, equal AS
        let (d_comb, _) = sel(&[ot.clone(), ob.clone()]);
        let d_orac = oracle(ot, ob);
        assert!(matches!(d_comb, Decision::Ambiguous { .. }));
        assert!(matches!(d_orac, Decision::Ambiguous { .. }));
    }

    /// Foret (unequal-AS same-position) also matches the oracle (closes the
    /// anti-drift gap — both winner directions cross-checked, not just equal-AS).
    #[test]
    fn mechanism_matches_oracle_foret_unequal_as() {
        // OT better (AS 0 vs OB -6) → OT (index 0) on both paths.
        let ot = line("chr1_CT_converted", 0, 100, 0, "6");
        let ob = line("chr1_GA_converted", 16, 100, -6, "6");
        let (d_comb, _) = sel(&[ot.clone(), ob.clone()]);
        assert_eq!(key(&d_comb), key(&oracle(ot, ob)));
        assert_eq!(key(&d_comb), Some(("chr1".to_string(), 100, 0)));
        // OB better (AS 0 vs OT -6) → OB (index 1) on both paths.
        let ot = line("chr1_CT_converted", 0, 100, -6, "6");
        let ob = line("chr1_GA_converted", 16, 100, 0, "6");
        let (d_comb, _) = sel(&[ot.clone(), ob.clone()]);
        assert_eq!(key(&d_comb), key(&oracle(ot, ob)));
        assert_eq!(key(&d_comb), Some(("chr1".to_string(), 100, 1)));
    }

    /// §9 telomeric/degenerate framing: in a DIRECTIONAL library the motivating
    /// telomeric read aligns **OT-only** (the C→T read matches only the CT
    /// sub-genome) — it is `UniqueBest` OT, NOT a won OT/OB tie. (The OT↔CTOB
    /// same-position collision is a non-directional / Phase-5 phenomenon.)
    #[test]
    fn select_telomeric_directional_is_ot_only_not_a_tie() {
        let (d, c) = sel(&[line("chr1_CT_converted", 0, 100, 0, "6")]); // sole OT hit
        match d {
            Decision::UniqueBest(b) => assert_eq!(b.index, 0), // OT
            other => panic!("expected UniqueBest OT, got {other:?}"),
        }
        assert_eq!(c.unique_best_alignment_count, 1);
        assert_eq!(c.unsuitable_sequence_count, 0); // NOT a tie
    }

    /// §9 explicit MAPQ assertion: the `UniqueBest` MAPQ is exactly `calc_mapq` fed
    /// the §3.7-pinned second-best (here `None` — a lone valid hit).
    #[test]
    fn select_unique_best_mapq_equals_calc_mapq() {
        let (d, _) = sel(&[line("chr1_CT_converted", 0, 100, 0, "6")]);
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.alignment_score_second_best, None);
                assert_eq!(b.mapq, calc_mapq(6, None, 0, None, 0.0, -0.2));
            }
            other => panic!("expected UniqueBest, got {other:?}"),
        }
    }

    // ===================================================================
    // Phase 5: non-directional union selection (`select_nondir`)
    // ===================================================================
    // The C→T-read pass yields OT/OB; the G→A-read pass yields CTOT/CTOB. Lines
    // are split across the two record slices the way the two combined passes
    // produce them: a C→T-pass OT line is `fwd + _CT_converted`; a G→A-pass CTOB
    // line is `fwd + _GA_converted` (the SAME (fwd,GA) shape that is *spurious* in
    // the C→T pass — hence `ReadConv` is load-bearing).

    /// Run `select_nondir` over the C→T-pass group `ct` + the G→A-pass group `ga`.
    fn sel_nondir(ct: &[SamRecord], ga: &[SamRecord]) -> (Decision, Counters) {
        let mut c = Counters::default();
        let d = select_nondir(ct, ga, "ACGTAC", 0.0, -0.2, &mut c).unwrap();
        (d, c)
    }

    #[test]
    fn select_nondir_ot_only_via_ct_pass() {
        // Hits in the C→T pass only (G→A pass misses) — the common ~50% case.
        let (d, c) = sel_nondir(&[line("chr1_CT_converted", 0, 10, 0, "6")], &[unmapped()]);
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.index, 0); // OT
                assert_eq!(b.chromosome, "chr1");
                assert_eq!(b.position, 10);
            }
            other => panic!("expected UniqueBest OT, got {other:?}"),
        }
        assert_eq!(c.unique_best_alignment_count, 1);
    }

    #[test]
    fn select_nondir_ctot_only_via_ga_pass() {
        // G→A pass, rev + _CT_converted → CTOT (index 2); C→T pass misses.
        let (d, c) = sel_nondir(&[unmapped()], &[line("chr4_CT_converted", 16, 40, 0, "6")]);
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.index, 2); // CTOT
                assert_eq!(b.chromosome, "chr4");
            }
            other => panic!("expected UniqueBest CTOT, got {other:?}"),
        }
        assert_eq!(c.unique_best_alignment_count, 1);
    }

    #[test]
    fn select_nondir_ctob_only_via_ga_pass() {
        // G→A pass, fwd + _GA_converted → CTOB (index 3); C→T pass misses.
        let (d, c) = sel_nondir(&[unmapped()], &[line("chr5_GA_converted", 0, 50, 0, "6")]);
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.index, 3); // CTOB
                assert_eq!(b.chromosome, "chr5");
            }
            other => panic!("expected UniqueBest CTOB, got {other:?}"),
        }
        assert_eq!(c.unique_best_alignment_count, 1);
    }

    /// THE §4b telomeric case: an OT hit (C→T pass) and a CTOB hit (G→A pass) at
    /// the SAME de-converted `chr:pos`, equal AS → KEPT (one entry), won by **CTOB
    /// (index 3)** via the later-slot `>=` overwrite. (Directionally that read is
    /// OT-only; the cross-strand collision only arises non-directionally.)
    #[test]
    fn select_nondir_same_position_ot_ctob_kept_ctob_wins() {
        let (d, c) = sel_nondir(
            &[line("chr1_CT_converted", 0, 100, 0, "6")], // C→T pass: OT @ chr1:100
            &[line("chr1_GA_converted", 0, 100, 0, "6")], // G→A pass: CTOB @ chr1:100 (fwd+GA)
        );
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.chromosome, "chr1");
                assert_eq!(b.position, 100);
                assert_eq!(b.index, 3); // CTOB wins the same-position equal-AS tie
            }
            other => panic!("expected UniqueBest (KEPT), got {other:?}"),
        }
        assert_eq!(c.unique_best_alignment_count, 1);
        assert_eq!(c.unsuitable_sequence_count, 0); // NOT ambiguous
    }

    /// The symmetric collision: OB (C→T pass) × CTOT (G→A pass) same `chr:pos`,
    /// equal AS → KEPT, won by **CTOT (index 2)** (later slot than OB=1).
    #[test]
    fn select_nondir_same_position_ob_ctot_kept_ctot_wins() {
        let (d, c) = sel_nondir(
            &[line("chr1_GA_converted", 16, 100, 0, "6")], // C→T pass: OB @ chr1:100 (rev+GA)
            &[line("chr1_CT_converted", 16, 100, 0, "6")], // G→A pass: CTOT @ chr1:100 (rev+CT)
        );
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.position, 100);
                assert_eq!(b.index, 2); // CTOT wins
            }
            other => panic!("expected UniqueBest (KEPT), got {other:?}"),
        }
        assert_eq!(c.unique_best_alignment_count, 1);
        assert_eq!(c.unsuitable_sequence_count, 0);
    }

    /// Cross-location equal-AS hits (OT @ chr1 vs CTOB @ chr2) → genuine
    /// cross-strand ambiguity (the dominant non-dir churn class).
    #[test]
    fn select_nondir_cross_location_is_ambiguous() {
        let (d, c) = sel_nondir(
            &[line("chr1_CT_converted", 0, 10, 0, "6")], // OT @ chr1:10
            &[line("chr2_GA_converted", 0, 25, 0, "6")], // CTOB @ chr2:25
        );
        assert_eq!(d, Decision::Ambiguous { first_ambig: None });
        assert_eq!(c.unsuitable_sequence_count, 1);
        assert_eq!(c.unique_best_alignment_count, 0);
    }

    /// Foret: same-position UNEQUAL AS → the better-AS strand wins regardless of
    /// slot order (it alone is in `top`).
    #[test]
    fn select_nondir_same_position_foret_better_as_wins() {
        // OT better → OT (index 0).
        let (d, _) = sel_nondir(
            &[line("chr1_CT_converted", 0, 100, 0, "6")], // OT AS 0 (better)
            &[line("chr1_GA_converted", 0, 100, -6, "6")], // CTOB AS -6 (worse)
        );
        assert!(matches!(d, Decision::UniqueBest(ref b) if b.index == 0));
        // CTOB better → CTOB (index 3).
        let (d, _) = sel_nondir(
            &[line("chr1_CT_converted", 0, 100, -6, "6")], // OT AS -6 (worse)
            &[line("chr1_GA_converted", 0, 100, 0, "6")],  // CTOB AS 0 (better)
        );
        assert!(matches!(d, Decision::UniqueBest(ref b) if b.index == 3));
    }

    /// A G→A-pass spurious-only read (`fwd + _CT_converted` would be OT for a C→T
    /// read, but is spurious for a G→A read) → `NoAlignment` + spurious count.
    #[test]
    fn select_nondir_ga_pass_spurious_only_is_no_alignment() {
        let (d, c) = sel_nondir(&[unmapped()], &[line("chr1_CT_converted", 0, 10, 0, "6")]);
        assert_eq!(d, Decision::NoAlignment);
        assert_eq!(c.no_single_alignment_found, 1);
        assert_eq!(c.combined_spurious_count, 1);
        assert_eq!(c.unique_best_alignment_count, 0);
    }

    #[test]
    fn select_nondir_both_passes_miss_is_no_alignment() {
        let (d, c) = sel_nondir(&[unmapped()], &[unmapped()]);
        assert_eq!(d, Decision::NoAlignment);
        assert_eq!(c.no_single_alignment_found, 1);
        assert_eq!(c.combined_spurious_count, 0); // both-miss is not "spurious"
    }

    // ---- 4-slot mechanism-vs-oracle cross-test (the anti-drift §9 gate) -----
    // Feed identical hand-built lines to BOTH select_nondir and the faithful
    // 4-instance merge oracle (slot 0=OT, 1=OB, 2=CTOT, 3=CTOB), asserting the
    // SAME tie outcome (Decision variant + chrom/pos/index, mapq+second-best
    // projected out). The oracle MUST run NON-directional — under directional the
    // faithful merge rejects a chosen index 2/3 (CTOT/CTOB) as `Rejected`
    // (`merge.rs:352`), which would corrupt every CTOT/CTOB headline case.

    /// Run the faithful merge oracle with the 4 non-dir instance slots, in
    /// NON-directional mode. Missing slots take an `unmapped()` (FLAG-4) line.
    fn oracle_nondir(ot: SamRecord, ob: SamRecord, ctot: SamRecord, ctob: SamRecord) -> Decision {
        let mut streams = vec![
            VecStream {
                recs: vec![ot],
                pos: 0,
            },
            VecStream {
                recs: vec![ob],
                pos: 0,
            },
            VecStream {
                recs: vec![ctot],
                pos: 0,
            },
            VecStream {
                recs: vec![ctob],
                pos: 0,
            },
        ];
        let mut c = Counters::default();
        crate::merge::check_results_single_end(
            "r1",
            "ACGTAC",
            &mut streams,
            false, // NON-directional (else index 2/3 → Rejected, merge.rs:352)
            0.0,
            -0.2,
            false,
            &mut c,
        )
        .unwrap()
    }

    #[test]
    fn mechanism_nondir_matches_oracle_ot_ctob_same_position() {
        let ot = line("chr1_CT_converted", 0, 100, 0, "6");
        let ctob = line("chr1_GA_converted", 0, 100, 0, "6"); // both → chr1:100, equal AS
        let (d_comb, _) = sel_nondir(std::slice::from_ref(&ot), std::slice::from_ref(&ctob));
        let d_orac = oracle_nondir(ot, unmapped(), unmapped(), ctob);
        assert_eq!(key(&d_comb), key(&d_orac)); // same KEEP outcome
        assert_eq!(key(&d_comb), Some(("chr1".to_string(), 100, 3))); // CTOB wins
    }

    #[test]
    fn mechanism_nondir_matches_oracle_ob_ctot_same_position() {
        let ob = line("chr1_GA_converted", 16, 100, 0, "6");
        let ctot = line("chr1_CT_converted", 16, 100, 0, "6"); // both → chr1:100, equal AS
        let (d_comb, _) = sel_nondir(std::slice::from_ref(&ob), std::slice::from_ref(&ctot));
        let d_orac = oracle_nondir(unmapped(), ob, ctot, unmapped());
        assert_eq!(key(&d_comb), key(&d_orac));
        assert_eq!(key(&d_comb), Some(("chr1".to_string(), 100, 2))); // CTOT wins
    }

    #[test]
    fn mechanism_nondir_matches_oracle_cross_location() {
        let ot = line("chr1_CT_converted", 0, 10, 0, "6");
        let ctob = line("chr2_GA_converted", 0, 25, 0, "6"); // distinct loci, equal AS
        let (d_comb, _) = sel_nondir(std::slice::from_ref(&ot), std::slice::from_ref(&ctob));
        let d_orac = oracle_nondir(ot, unmapped(), unmapped(), ctob);
        assert!(matches!(d_comb, Decision::Ambiguous { .. }));
        assert!(matches!(d_orac, Decision::Ambiguous { .. }));
    }

    #[test]
    fn mechanism_nondir_matches_oracle_foret_unequal_as() {
        // OT better (0 vs CTOB -6) → OT (index 0) on both paths.
        let ot = line("chr1_CT_converted", 0, 100, 0, "6");
        let ctob = line("chr1_GA_converted", 0, 100, -6, "6");
        let (d_comb, _) = sel_nondir(std::slice::from_ref(&ot), std::slice::from_ref(&ctob));
        assert_eq!(
            key(&d_comb),
            key(&oracle_nondir(ot, unmapped(), unmapped(), ctob))
        );
        assert_eq!(key(&d_comb), Some(("chr1".to_string(), 100, 0)));
        // CTOB better (0 vs OT -6) → CTOB (index 3) on both paths.
        let ot = line("chr1_CT_converted", 0, 100, -6, "6");
        let ctob = line("chr1_GA_converted", 0, 100, 0, "6");
        let (d_comb, _) = sel_nondir(std::slice::from_ref(&ot), std::slice::from_ref(&ctob));
        assert_eq!(
            key(&d_comb),
            key(&oracle_nondir(ot, unmapped(), unmapped(), ctob))
        );
        assert_eq!(key(&d_comb), Some(("chr1".to_string(), 100, 3)));
    }
}

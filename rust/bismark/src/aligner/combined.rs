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
//! - **PBAT** ([`select_pbat`]) runs ONE G→A pass → CTOT/CTOB (indices 2/3) — the
//!   G→A-pass half of non-dir, standalone.
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

use crate::aligner::align::{SamPair, SamRecord};
use crate::aligner::error::{AlignerError, Result};
use crate::aligner::mapq::calc_mapq;
use crate::aligner::merge::{
    BestAlignment, BestAlignmentPaired, Counters, Decision, DecisionPaired, deconvert,
};

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

    /// The **paired-end** synthetic instance index for the byte-frozen PE output
    /// arm. **PE numbering DIFFERS from SE `to_index`** — OB and CTOB swap slots
    /// 1↔3: the PE arm interprets `index` as `0 → OT`, `1 → CTOB`, `2 → CTOT`,
    /// `3 → OB` (the `pe_instance_plan` order, `lib.rs`; the PE strand/conversion
    /// switch `methylation.rs:421-425`; the PE FLAG switch `output.rs:469-473`
    /// where index 3 → FLAG 83/163 = OB). **OB MUST map to 3** here: if it reused
    /// the SE `to_index` (OB→1), the faithful PE directional reject (chosen index
    /// 1/2, `merge.rs:725`) would silently drop EVERY OB pair, and the FLAG /
    /// methylation arm would treat it as CTOB. For the directional C→T pass only OT
    /// (→0) and OB (→3) are reachable, so neither hits the 1/2 reject. Spurious has
    /// no valid index.
    fn to_index_pe(self) -> Option<usize> {
        match self {
            CombinedClass::Ot => Some(0),
            CombinedClass::Ctob => Some(1),
            CombinedClass::Ctot => Some(2),
            CombinedClass::Ob => Some(3),
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
    score_min_local: bool,
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
        score_min_local,
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
    score_min_local: bool,
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
        score_min_local,
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
    score_min_local: bool,
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
        score_min_local,
        counters,
    )
}

/// PBAT combined-index per-read selection over the read's `-k` line group: every
/// line is from the G→A-converted read pass ([`ReadConv::Ga`]) → CTOT/CTOB. A thin
/// wrapper over [`select_core`] (the shared tie machine) — the **single-pass `Ga`
/// analog of the directional [`select`]** (which tags every line `Ct`). PBAT reads
/// originate from the complementary strands, so a hit on the original-strand
/// orientation (`fwd+CT` / `rev+GA`) is spurious here. A same-locus equal-`AS`
/// CTOT×CTOB collision is KEPT, won by **CTOB (index 3)** (the later slot via the
/// `>=` overwrite — the PBAT analog of the §4b telomeric case).
///
/// **NB:** the caller routes the resulting `Decision` through `route_se_decision`
/// with `pbat = FALSE` — `classify(Ga, …)` emits the synthetic index **2/3
/// directly**, so the faithful PBAT `+2` extraction modifier must NOT also fire
/// (else `eff = 4/5` → "Too many Bowtie 2 result filehandles"). This mirrors the
/// non-directional combined path, NOT the faithful 2-instance PBAT path (which
/// uses slot indices 0/1 + `pbat=true`).
pub fn select_pbat(
    records: &[SamRecord],
    sequence: &str,
    score_min_intercept: f64,
    score_min_slope: f64,
    score_min_local: bool,
    counters: &mut Counters,
) -> Result<Decision> {
    // Keep only mapped lines, each tagged G→A (the PBAT pass). The shared core
    // classifies (CTOT/CTOB/spurious), resolves ties, and owns the counters.
    let mapped: Vec<(ReadConv, &SamRecord)> = records
        .iter()
        .filter(|r| !r.is_unmapped())
        .map(|r| (ReadConv::Ga, r))
        .collect();
    select_core(
        mapped,
        sequence,
        score_min_intercept,
        score_min_slope,
        score_min_local,
        counters,
    )
}

// ===========================================================================
// Paired-end combined-index selection (v2.x Phase 2). The PE analog of
// `select`/`select_core`: a read pair's `-k 2` group is gathered into candidate
// pairs, each classified on its **R1 mate** (orientation × sub-genome suffix —
// the same `classify` table SE uses on the single read), scored by the **sum of
// both mates' AS**, and resolved by the same Bismark-faithful tie machine, this
// time emitting a [`DecisionPaired`]/[`BestAlignmentPaired`] so the byte-frozen PE
// output arm (`extract_corresponding_genomic_sequence_paired_end` →
// `methylation_call` ×2 → `paired_end_sam_output`) is reused UNCHANGED. The oracle
// is `merge::check_results_paired_end`; the `select_core_pe` mechanism is
// cross-checked against it in the tests. Phase 2 ships only the directional
// [`select_pe`]; Phases 3/4 add the non-dir/pbat wrappers over the same core.
// ===========================================================================

/// One valid PE alignment kept for the `chr:pos1:pos2` map — the PE analog of
/// [`Cand`] (mirrors `merge::StoredPair`, which is private to `merge.rs`).
struct CandPe {
    chromosome: String,
    position_1: u32,
    position_2: u32,
    index: usize,
    sum: i64,
    md_tag_1: String,
    md_tag_2: String,
    cigar_1: String,
    cigar_2: String,
    bowtie_sequence_1: String,
    bowtie_sequence_2: String,
    flag_1: u16,
    flag_2: u16,
}

/// Directional PE combined-index per-pair selection over the pair's `-k 2` group:
/// every candidate pair is from the C→T-converted read pass ([`ReadConv::Ct`],
/// `-1 C→T_R1 -2 G→A_R2`) → OT/OB. A thin wrapper over [`select_core_pe`] (the
/// shared PE tie machine) — the PE analog of the directional [`select`]. `pairs`
/// are the (≤ k) `SamPair`s Bowtie 2 emitted for one read pair; `sequence_1`/`_2`
/// are the original (uc) reads (their lengths feed `calc_mapq`).
pub fn select_pe(
    pairs: &[SamPair],
    sequence_1: &str,
    sequence_2: &str,
    score_min_intercept: f64,
    score_min_slope: f64,
    score_min_local: bool,
    counters: &mut Counters,
) -> Result<DecisionPaired> {
    // Drop the PE no-alignment marker (FLAG 77/141 — Bowtie 2 emits one such pair
    // for a miss); tag every surviving pair C→T (the directional pass). The shared
    // core does the rest.
    let mapped: Vec<(ReadConv, &SamPair)> = pairs
        .iter()
        .filter(|p| !p.is_unmapped_pair())
        .map(|p| (ReadConv::Ct, p))
        .collect();
    select_core_pe(
        mapped,
        sequence_1,
        sequence_2,
        score_min_intercept,
        score_min_slope,
        score_min_local,
        counters,
    )
}

/// Non-directional PE combined-index per-pair selection over the UNION of the two
/// passes' `-k 2` pair groups (PLAN 06102026 phase 3): the C→T-converted-read pass
/// (`ct_pairs`, `-1 C→T_R1 -2 G→A_R2` → OT/OB) and the G→A-converted-read pass
/// (`ga_pairs`, `-1 G→A_R1 -2 C→T_R2` → CTOT/CTOB). Each pass's mapped pairs are
/// tagged with its [`ReadConv`], unioned, and fed to the shared [`select_core_pe`]
/// tie machine across all four slots (OT=0, CTOB=1, CTOT=2, OB=3 via
/// [`CombinedClass::to_index_pe`]). A same-locus equal-sum cross-strand collision is
/// KEPT, won by the scan-order-last (`[0,3,1,2]`) slot: **OT×CTOB → CTOB (index 1)**,
/// **OB×CTOT → CTOT (index 2)**. The PE analog of the SE [`select_nondir`];
/// `select_core_pe` is reused UNCHANGED — it was built 4-slot-ready in Phase 2 (the
/// `select_core_pe_uses_literal_scan_order_not_ascending` test locks the only-non-dir-
/// reachable OB×CTOB collision). `sequence_1`/`_2` are the original (uc) reads.
#[allow(clippy::too_many_arguments)] // +score_min_local pushed this to 8 (the score-min trio rides together)
pub fn select_pe_nondir(
    ct_pairs: &[SamPair],
    ga_pairs: &[SamPair],
    sequence_1: &str,
    sequence_2: &str,
    score_min_intercept: f64,
    score_min_slope: f64,
    score_min_local: bool,
    counters: &mut Counters,
) -> Result<DecisionPaired> {
    // Drop each pass's PE no-alignment marker (FLAG 77/141), tag the C→T pass's pairs
    // `Ct` (→ OT/OB) and the G→A pass's pairs `Ga` (→ CTOT/CTOB), and union. The
    // shared core computes the global best sum + the MAPQ runner-up over the union and
    // resolves ties across all four slots in the `[0,3,1,2]` scan order.
    let mapped: Vec<(ReadConv, &SamPair)> = ct_pairs
        .iter()
        .filter(|p| !p.is_unmapped_pair())
        .map(|p| (ReadConv::Ct, p))
        .chain(
            ga_pairs
                .iter()
                .filter(|p| !p.is_unmapped_pair())
                .map(|p| (ReadConv::Ga, p)),
        )
        .collect();
    select_core_pe(
        mapped,
        sequence_1,
        sequence_2,
        score_min_intercept,
        score_min_slope,
        score_min_local,
        counters,
    )
}

/// PBAT PE combined-index per-pair selection over the pair's `-k 2` group: every
/// candidate pair is from the G→A-converted read pass ([`ReadConv::Ga`],
/// `-1 G→A_R1 -2 C→T_R2`) → CTOT/CTOB. A thin wrapper over the unchanged
/// [`select_core_pe`] — the single-pass `Ga` analog of the directional [`select_pe`]
/// (which tags every pair `Ct`), i.e. the PE analog of the SE [`select_pbat`]. PBAT
/// reads originate from the complementary strands, so a hit in the original-strand
/// orientation (`fwd+_CT` / `rev+_GA`) is **spurious** here (the `classify(Ga, …)`
/// table). PBAT is the G→A-pass half of the non-directional path
/// ([`select_pe_nondir`]) standalone (only CTOT/CTOB reachable, no C→T pass to union).
///
/// **NB:** the caller routes the resulting `DecisionPaired` through `route_pe_decision`,
/// which — unlike the SE `route_se_decision(pbat)` — takes **no `pbat` parameter**:
/// `classify(Ga, …)` emits the synthetic PE index **1/2** ([`CombinedClass::to_index_pe`]:
/// CTOB→1, CTOT→2) directly, and `extract_corresponding_genomic_sequence_paired_end`
/// maps strand/conversion as a pure function of that index (index 1/2 handled natively).
/// There is no faithful `+2` extraction modifier in PE, so the SE pbat double-shift
/// gotcha has no PE analog (nothing to disable). A same-locus equal-sum CTOT×CTOB
/// collision is KEPT, won by the scan-order-last (`[0,3,1,2]`) of {1,2} = **CTOT
/// (index 2)** — note this DIFFERS from SE pbat (where CTOB wins), because both the PE
/// renumbering and the literal scan order differ from SE's ascending {2,3}.
pub fn select_pe_pbat(
    pairs: &[SamPair],
    sequence_1: &str,
    sequence_2: &str,
    score_min_intercept: f64,
    score_min_slope: f64,
    score_min_local: bool,
    counters: &mut Counters,
) -> Result<DecisionPaired> {
    // Drop the PE no-alignment marker (FLAG 77/141); tag every surviving pair G→A
    // (the single PBAT pass). The shared core classifies (CTOT/CTOB/spurious),
    // resolves ties, and owns the counters.
    let mapped: Vec<(ReadConv, &SamPair)> = pairs
        .iter()
        .filter(|p| !p.is_unmapped_pair())
        .map(|p| (ReadConv::Ga, p))
        .collect();
    select_core_pe(
        mapped,
        sequence_1,
        sequence_2,
        score_min_intercept,
        score_min_slope,
        score_min_local,
        counters,
    )
}

/// The shared **Bismark-faithful PE tie machine** — `select_core` doubled for two
/// mates (PLAN 06102026 phase 2; the oracle is `merge::check_results_paired_end`).
/// `mapped` is the pair's FULL set of mapped candidate pairs, each tagged with the
/// [`ReadConv`] of the pass that produced it (directional [`select_pe`] tags every
/// pair `Ct`; Phases 3/4 will tag `Ga` / union both). OWNS the alignment-outcome
/// counters the bypassed merge would bump.
///
/// Decision precedence (mirrors [`select_core`], summed over both mates):
/// 1. no mapped pair → `NoAlignment`.
/// 2. AS mandatory on BOTH mates of every pair (Perl 3405–3406); `sum = AS_1 +
///    AS_2`. Sort highest-sum first → the GLOBAL best sum + the Phase-2 MAPQ
///    runner-up (`mapped[1]`'s sum, over the FULL set).
/// 3. `top` = pairs at the best sum, each classified on **R1** (`classify(rc,
///    r1.flag, r1.rname)` → orientation × sub-genome) and mapped to the **PE**
///    index via [`CombinedClass::to_index_pe`]; both mates must de-convert to the
///    SAME chromosome (Perl 3351–3364, [`deconvert`]).
/// 4. if `top` has any spurious pair (wrong sub-genome/pass at the best sum):
///    also-valid → `Ambiguous`; all-spurious → `NoAlignment` (+`combined_spurious_count`).
/// 5. else build a `chr:pos1:pos2` map over `top`, inserting in the oracle's
///    **literal scan order `[0,3,1,2]`** with `>=` overwrite (NOT an ascending-index
///    sort — they agree for directional's {0,3} but DIVERGE for the 4-slot reuse in
///    Phase 3; `merge::SCAN_ORDER`). A same-locus equal-sum collision collapses to ONE
///    entry won by the scan-order-last index (directional: OB(3) over OT(0)). Key is
///    RAW `chr:pos1:pos2` (the no-second-best branch, Perl 3593) — the combined path
///    never tracks a per-pair within-XS second-best, so the oracle's `chr:min:max`
///    second-best key never applies. 1 entry → `UniqueBest`; ≥2 distinct loci →
///    `Ambiguous`; `>4` → the verbatim too-many-hits guard (unreachable under `-k 2`).
///
/// MAPQ second-best is the Phase-2 rule (runner-up pair sum, or `None`) — it
/// legitimately differs from the merge's per-mate `XS`/`ZS`, so it is excluded from
/// the mechanism-vs-oracle test.
fn select_core_pe(
    mut mapped: Vec<(ReadConv, &SamPair)>,
    sequence_1: &str,
    sequence_2: &str,
    score_min_intercept: f64,
    score_min_slope: f64,
    score_min_local: bool,
    counters: &mut Counters,
) -> Result<DecisionPaired> {
    if mapped.is_empty() {
        counters.no_single_alignment_found += 1;
        return Ok(DecisionPaired::NoAlignment);
    }

    // AS mandatory on BOTH mates of every pair (Perl 3405–3406; merge.rs:555–566).
    for (_, p) in &mapped {
        if p.read1.alignment_score.is_none() {
            return Err(AlignerError::Validation(format!(
                "Failed to extract alignment score 1 from line {}",
                p.read1.raw_line
            )));
        }
        if p.read2.alignment_score.is_none() {
            return Err(AlignerError::Validation(format!(
                "Failed to extract alignment score 2 from line {}",
                p.read2.raw_line
            )));
        }
    }
    // Sum of both mates' AS per pair; sort highest-sum first → GLOBAL best sum + the
    // Phase-2 MAPQ runner-up (`mapped[1]`'s sum, over the FULL set, not `top`).
    let pair_sum = |p: &SamPair| -> i64 {
        p.read1.alignment_score.unwrap() + p.read2.alignment_score.unwrap()
    };
    mapped.sort_by_key(|(_, p)| std::cmp::Reverse(pair_sum(p)));
    let best_sum = pair_sum(mapped[0].1);
    let second_best = mapped.get(1).map(|(_, p)| pair_sum(p));

    // `top` = pairs at the global best sum; classify each on R1 (orientation ×
    // sub-genome) → PE index, requiring both mates on the same chromosome.
    let mut valid_top: Vec<(String, u32, u32, usize, &SamPair)> = Vec::new();
    let mut any_spurious = false;
    for (rc, p) in mapped.iter().filter(|(_, p)| pair_sum(p) == best_sum) {
        let (chrom, class) = classify(*rc, p.read1.flag, &p.read1.rname)?;
        let chrom2 = deconvert(&p.read2.rname)?;
        if chrom != chrom2 {
            return Err(AlignerError::Validation(
                "Paired-end alignments need to be on the same chromosome".into(),
            ));
        }
        match class.to_index_pe() {
            Some(index) => valid_top.push((chrom, p.read1.pos, p.read2.pos, index, p)),
            None => any_spurious = true,
        }
    }

    // §spurious branch (on the GLOBAL best sum — never a silent rescue).
    if any_spurious {
        if valid_top.is_empty() {
            counters.no_single_alignment_found += 1;
            counters.combined_spurious_count += 1;
            return Ok(DecisionPaired::NoAlignment);
        }
        counters.unsuitable_sequence_count += 1;
        return Ok(DecisionPaired::Ambiguous { first_ambig: None });
    }

    // §all-valid → build a `chr:pos1:pos2` map, inserting in the oracle's literal
    // scan order [0,3,1,2] with `>=` overwrite (later-equal wins) so a same-locus
    // equal-sum collision is won by the scan-order-last index. MD mandatory on both
    // mates of every entered pair (Perl 3405–3406).
    const SCAN_ORDER: [usize; 4] = [0, 3, 1, 2];
    let mut map: HashMap<String, CandPe> = HashMap::new();
    for &slot in &SCAN_ORDER {
        for (chrom, pos1, pos2, index, p) in
            valid_top.iter().filter(|(_, _, _, idx, _)| *idx == slot)
        {
            let md1 = p.read1.md_tag.clone().ok_or_else(|| {
                AlignerError::Validation(format!(
                    "Failed to extract MD tag 1 from line {}",
                    p.read1.raw_line
                ))
            })?;
            let md2 = p.read2.md_tag.clone().ok_or_else(|| {
                AlignerError::Validation(format!(
                    "Failed to extract MD tag 2 from line {}",
                    p.read2.raw_line
                ))
            })?;
            map.insert(
                format!("{chrom}:{pos1}:{pos2}"),
                CandPe {
                    chromosome: chrom.clone(),
                    position_1: *pos1,
                    position_2: *pos2,
                    index: *index,
                    sum: best_sum,
                    md_tag_1: md1,
                    md_tag_2: md2,
                    cigar_1: p.read1.cigar.clone(),
                    cigar_2: p.read2.cigar.clone(),
                    bowtie_sequence_1: p.read1.seq.clone(),
                    bowtie_sequence_2: p.read2.seq.clone(),
                    flag_1: p.read1.flag,
                    flag_2: p.read2.flag,
                },
            );
        }
    }

    let mut entries: Vec<CandPe> = map.into_values().collect();
    if entries.len() > 4 {
        return Err(AlignerError::Validation(format!(
            "There are too many potential hits for this sequence pair (1-4 expected, but found: {})",
            entries.len()
        )));
    }
    if entries.len() >= 2 {
        counters.unsuitable_sequence_count += 1;
        return Ok(DecisionPaired::Ambiguous { first_ambig: None });
    }
    let best = entries.pop().expect("top all-valid → ≥1 map entry");

    let mapq = calc_mapq(
        sequence_1.len(),
        Some(sequence_2.len()),
        best.sum,
        second_best,
        score_min_intercept,
        score_min_slope,
        score_min_local,
    );
    counters.unique_best_alignment_count += 1;
    Ok(DecisionPaired::UniqueBest(BestAlignmentPaired {
        chromosome: best.chromosome,
        index: best.index,
        position_1: best.position_1,
        position_2: best.position_2,
        cigar_1: best.cigar_1,
        cigar_2: best.cigar_2,
        md_tag_1: best.md_tag_1,
        md_tag_2: best.md_tag_2,
        bowtie_sequence_1: best.bowtie_sequence_1,
        bowtie_sequence_2: best.bowtie_sequence_2,
        flag_1: best.flag_1,
        flag_2: best.flag_2,
        sum_of_alignment_scores: best.sum,
        sum_of_alignment_scores_second_best: second_best,
        mapq,
    }))
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
        let d = select(records, "ACGTAC", 0.0, -0.2, false, &mut c).unwrap();
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
    impl crate::aligner::align::SamStream for VecStream {
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
        crate::aligner::merge::check_results_single_end(
            "r1",
            "ACGTAC",
            &mut streams,
            true, // directional
            0.0,
            -0.2,
            false,
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
                assert_eq!(b.mapq, calc_mapq(6, None, 0, None, 0.0, -0.2, false));
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
        let d = select_nondir(ct, ga, "ACGTAC", 0.0, -0.2, false, &mut c).unwrap();
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
        crate::aligner::merge::check_results_single_end(
            "r1",
            "ACGTAC",
            &mut streams,
            false, // NON-directional (else index 2/3 → Rejected, merge.rs:352)
            0.0,
            -0.2,
            false,
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

    // ===================================================================
    // Phase 7: PBAT single-pass selection (`select_pbat`)
    // ===================================================================
    // PBAT = the G→A pass STANDALONE → CTOT (rev+_CT_converted) / CTOB
    // (fwd+_GA_converted). The OT/OB orientations (fwd+CT, rev+GA) are spurious
    // here. `select_pbat` emits synthetic index 2/3 DIRECTLY (route with pbat=false).

    /// Run `select_pbat` over the G→A-pass `-k` line group.
    fn sel_pbat(records: &[SamRecord]) -> (Decision, Counters) {
        let mut c = Counters::default();
        let d = select_pbat(records, "ACGTAC", 0.0, -0.2, false, &mut c).unwrap();
        (d, c)
    }

    /// Faithful 2-instance PBAT oracle: slot 0 = CTOT (`Nofw,Ct`), slot 1 = CTOB
    /// (`Norc,Ga`), run NON-directional so the merge keeps them (slots are 0/1,
    /// below the index-2/3 reject threshold anyway). The merge stores the SLOT
    /// index 0/1 in its `Decision`; the faithful `+2` PBAT lift is applied only at
    /// extraction — so the caller compares against [`key_lift2`] (index + 2).
    fn oracle_pbat(ctot: SamRecord, ctob: SamRecord) -> Decision {
        let mut streams = vec![
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
        crate::aligner::merge::check_results_single_end(
            "r1",
            "ACGTAC",
            &mut streams,
            false, // non-dir/pbat: don't reject the complementary strands
            0.0,
            -0.2,
            false,
            false,
            &mut c,
        )
        .unwrap()
    }

    /// Project a Decision to (chrom, pos, **index + 2**) — lifts the faithful merge's
    /// SLOT index 0/1 to the effective CTOT/CTOB index 2/3 that the combined path
    /// (and the extraction `+2` modifier) produce. Compare `key(combined)` vs
    /// `key_lift2(oracle)`.
    fn key_lift2(d: &Decision) -> Option<(String, u32, usize)> {
        match d {
            Decision::UniqueBest(b) => Some((b.chromosome.clone(), b.position, b.index + 2)),
            _ => None,
        }
    }

    #[test]
    fn select_pbat_ctot_only_maps_to_index_2() {
        // rev + _CT_converted (G→A read) → CTOT.
        let (d, c) = sel_pbat(std::slice::from_ref(&line(
            "chr4_CT_converted",
            16,
            40,
            0,
            "6",
        )));
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
    fn select_pbat_ctob_only_maps_to_index_3() {
        // fwd + _GA_converted (G→A read) → CTOB.
        let (d, c) = sel_pbat(std::slice::from_ref(&line(
            "chr5_GA_converted",
            0,
            50,
            0,
            "6",
        )));
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.index, 3); // CTOB
                assert_eq!(b.chromosome, "chr5");
            }
            other => panic!("expected UniqueBest CTOB, got {other:?}"),
        }
        assert_eq!(c.unique_best_alignment_count, 1);
    }

    /// The PBAT §4b analog: a same-locus equal-`AS` CTOT×CTOB collision is KEPT,
    /// won by **CTOB (index 3)** (later slot via `>=`).
    #[test]
    fn select_pbat_same_position_ctot_ctob_kept_ctob_wins() {
        let (d, c) = sel_pbat(&[
            line("chr1_CT_converted", 16, 100, 0, "6"), // CTOT @ chr1:100 (rev+CT)
            line("chr1_GA_converted", 0, 100, 0, "6"),  // CTOB @ chr1:100 (fwd+GA), equal AS
        ]);
        match d {
            Decision::UniqueBest(b) => {
                assert_eq!(b.position, 100);
                assert_eq!(b.index, 3); // CTOB wins
            }
            other => panic!("expected UniqueBest (KEPT), got {other:?}"),
        }
        assert_eq!(c.unique_best_alignment_count, 1);
        assert_eq!(c.unsuitable_sequence_count, 0); // NOT ambiguous
    }

    #[test]
    fn select_pbat_cross_location_is_ambiguous() {
        let (d, c) = sel_pbat(&[
            line("chr1_CT_converted", 16, 10, 0, "6"), // CTOT @ chr1:10
            line("chr2_GA_converted", 0, 25, 0, "6"),  // CTOB @ chr2:25
        ]);
        assert_eq!(d, Decision::Ambiguous { first_ambig: None });
        assert_eq!(c.unsuitable_sequence_count, 1);
    }

    #[test]
    fn select_pbat_same_position_foret_better_as_wins() {
        // CTOT better → CTOT (index 2).
        let (d, _) = sel_pbat(&[
            line("chr1_CT_converted", 16, 100, 0, "6"), // CTOT AS 0 (better)
            line("chr1_GA_converted", 0, 100, -6, "6"), // CTOB AS -6 (worse)
        ]);
        assert!(matches!(d, Decision::UniqueBest(ref b) if b.index == 2));
        // CTOB better → CTOB (index 3).
        let (d, _) = sel_pbat(&[
            line("chr1_CT_converted", 16, 100, -6, "6"), // CTOT AS -6 (worse)
            line("chr1_GA_converted", 0, 100, 0, "6"),   // CTOB AS 0 (better)
        ]);
        assert!(matches!(d, Decision::UniqueBest(ref b) if b.index == 3));
    }

    #[test]
    fn select_pbat_original_strand_orientation_is_spurious() {
        // Under the G→A (PBAT) pass, the OT/OB orientations are spurious:
        // fwd+CT (would be OT for a C→T read) and rev+GA (would be OB).
        let (d, c) = sel_pbat(std::slice::from_ref(&line(
            "chr1_CT_converted",
            0,
            10,
            0,
            "6",
        )));
        assert_eq!(d, Decision::NoAlignment);
        assert_eq!(c.combined_spurious_count, 1);
        let (d, c) = sel_pbat(std::slice::from_ref(&line(
            "chr1_GA_converted",
            16,
            10,
            0,
            "6",
        )));
        assert_eq!(d, Decision::NoAlignment);
        assert_eq!(c.combined_spurious_count, 1);
    }

    // ---- mechanism-vs-oracle (PBAT): combined.index == oracle.index + 2 --------

    #[test]
    fn mechanism_pbat_matches_oracle_ctot_ctob_same_position() {
        let ctot = line("chr1_CT_converted", 16, 100, 0, "6");
        let ctob = line("chr1_GA_converted", 0, 100, 0, "6"); // both → chr1:100, equal AS
        let (d_comb, _) = sel_pbat(&[ctot.clone(), ctob.clone()]);
        let d_orac = oracle_pbat(ctot, ctob);
        assert_eq!(key(&d_comb), key_lift2(&d_orac)); // CTOB wins, eff index 3 both
        assert_eq!(key(&d_comb), Some(("chr1".to_string(), 100, 3)));
    }

    #[test]
    fn mechanism_pbat_matches_oracle_cross_location() {
        let ctot = line("chr1_CT_converted", 16, 10, 0, "6");
        let ctob = line("chr2_GA_converted", 0, 25, 0, "6"); // distinct loci, equal AS
        let (d_comb, _) = sel_pbat(&[ctot.clone(), ctob.clone()]);
        assert!(matches!(d_comb, Decision::Ambiguous { .. }));
        assert!(matches!(
            oracle_pbat(ctot, ctob),
            Decision::Ambiguous { .. }
        ));
    }

    #[test]
    fn mechanism_pbat_matches_oracle_foret_unequal_as() {
        // CTOT better → eff index 2 both paths.
        let ctot = line("chr1_CT_converted", 16, 100, 0, "6");
        let ctob = line("chr1_GA_converted", 0, 100, -6, "6");
        let (d_comb, _) = sel_pbat(&[ctot.clone(), ctob.clone()]);
        assert_eq!(key(&d_comb), key_lift2(&oracle_pbat(ctot, ctob)));
        assert_eq!(key(&d_comb), Some(("chr1".to_string(), 100, 2)));
        // CTOB better → eff index 3 both paths.
        let ctot = line("chr1_CT_converted", 16, 100, -6, "6");
        let ctob = line("chr1_GA_converted", 0, 100, 0, "6");
        let (d_comb, _) = sel_pbat(&[ctot.clone(), ctob.clone()]);
        assert_eq!(key(&d_comb), key_lift2(&oracle_pbat(ctot, ctob)));
        assert_eq!(key(&d_comb), Some(("chr1".to_string(), 100, 3)));
    }

    // ===================================================================
    // Paired-end combined-index selection (Phase 2) — select_pe /
    // select_core_pe, cross-checked against check_results_paired_end.
    // ===================================================================

    /// One PE SAM line (10bp read, CIGAR 10M, MD:Z:10) — the happy form.
    fn pe_line(id: &str, mate: u8, flag: u16, rname: &str, pos: u32, as_i: i64) -> String {
        format!(
            "{id}/{mate}\t{flag}\t{rname}\t{pos}\t40\t10M\t=\t{pos}\t0\tACGTACGTAC\tIIIIIIIIII\tAS:i:{as_i}\tMD:Z:10"
        )
    }

    /// Build a `SamPair` from R1/R2 (flag, rname, pos, AS); `from_lines` IDs R1 by `/1`.
    #[allow(clippy::too_many_arguments)]
    fn mk_pair(
        flag1: u16,
        rname1: &str,
        pos1: u32,
        as1: i64,
        flag2: u16,
        rname2: &str,
        pos2: u32,
        as2: i64,
    ) -> SamPair {
        SamPair::from_lines(
            &pe_line("r1", 1, flag1, rname1, pos1, as1),
            &pe_line("r1", 2, flag2, rname2, pos2, as2),
        )
        .unwrap()
    }

    /// OT-shaped pair: R1 fwd (flag 99) + R2 rev (147) on the CT sub-genome.
    fn ot_pair(pos1: u32, pos2: u32, as1: i64, as2: i64) -> SamPair {
        mk_pair(
            99,
            "chr1_CT_converted",
            pos1,
            as1,
            147,
            "chr1_CT_converted",
            pos2,
            as2,
        )
    }
    /// OB-shaped pair: R1 rev (flag 83) + R2 fwd (163) on the GA sub-genome.
    fn ob_pair(pos1: u32, pos2: u32, as1: i64, as2: i64) -> SamPair {
        mk_pair(
            83,
            "chr1_GA_converted",
            pos1,
            as1,
            163,
            "chr1_GA_converted",
            pos2,
            as2,
        )
    }

    /// Run directional `select_pe` over the candidate pairs.
    fn sel_pe(pairs: &[SamPair]) -> (DecisionPaired, Counters) {
        let mut c = Counters::default();
        let d = select_pe(pairs, "ACGTACGTAC", "ACGTACGTAC", 0.0, -0.2, false, &mut c).unwrap();
        (d, c)
    }

    /// Project a `DecisionPaired` to the tie-relevant identity (mapq + 2nd-best excluded).
    fn key_pe(d: &DecisionPaired) -> Option<(String, u32, u32, usize)> {
        match d {
            DecisionPaired::UniqueBest(b) => {
                Some((b.chromosome.clone(), b.position_1, b.position_2, b.index))
            }
            _ => None,
        }
    }

    /// Canned `PairedSamStream` double over fixed pairs (no subprocess).
    struct VecPairStream {
        pairs: Vec<SamPair>,
        pos: usize,
    }
    impl crate::aligner::align::PairedSamStream for VecPairStream {
        fn current_pair(&self) -> Option<&SamPair> {
            self.pairs.get(self.pos)
        }
        fn advance_pair(&mut self) -> Result<()> {
            self.pos += 1;
            Ok(())
        }
    }

    /// The faithful PE oracle with OT in slot 0 and OB in slot 3 (the directional
    /// `pe_instance_plan`). Cross-checked against `select_core_pe`.
    fn oracle_pe(ot: Option<SamPair>, ob: Option<SamPair>) -> DecisionPaired {
        let mk = |p: Option<SamPair>| {
            p.map(|pp| VecPairStream {
                pairs: vec![pp],
                pos: 0,
            })
        };
        let mut streams: Vec<Option<VecPairStream>> = vec![mk(ot), None, None, mk(ob)];
        let mut c = Counters::default();
        crate::aligner::merge::check_results_paired_end(
            "r1",
            "ACGTACGTAC",
            "ACGTACGTAC",
            &mut streams,
            true, // directional
            0.0,
            -0.2,
            false,
            false,
            crate::aligner::config::Aligner::Bowtie2,
            &mut c,
        )
        .unwrap()
    }

    #[test]
    fn to_index_pe_uses_pe_numbering_and_differs_from_se_on_ob_ctob() {
        // PE numbering (the `pe_instance_plan` / `methylation.rs:421` / `output.rs:469`
        // order): OT=0, CTOB=1, CTOT=2, OB=3; Spurious has no index.
        assert_eq!(CombinedClass::Ot.to_index_pe(), Some(0));
        assert_eq!(CombinedClass::Ctob.to_index_pe(), Some(1));
        assert_eq!(CombinedClass::Ctot.to_index_pe(), Some(2));
        assert_eq!(CombinedClass::Ob.to_index_pe(), Some(3));
        assert_eq!(CombinedClass::Spurious.to_index_pe(), None);
        // OB and CTOB swap slots 1↔3 vs the SE `to_index` — the reconciliation that
        // keeps OB (PE index 3) out of the PE directional reject (index 1/2).
        assert_ne!(
            CombinedClass::Ob.to_index(),
            CombinedClass::Ob.to_index_pe()
        );
        assert_ne!(
            CombinedClass::Ctob.to_index(),
            CombinedClass::Ctob.to_index_pe()
        );
        assert_eq!(CombinedClass::Ob.to_index(), Some(1)); // SE
        assert_eq!(CombinedClass::Ob.to_index_pe(), Some(3)); // PE
        // OT and CTOT are identical in both numberings.
        assert_eq!(
            CombinedClass::Ot.to_index(),
            CombinedClass::Ot.to_index_pe()
        );
        assert_eq!(
            CombinedClass::Ctot.to_index(),
            CombinedClass::Ctot.to_index_pe()
        );
    }

    #[test]
    fn select_pe_unique_ot_maps_to_index_0() {
        let (d, c) = sel_pe(&[ot_pair(100, 200, -2, -2)]);
        assert_eq!(key_pe(&d), Some(("chr1".to_string(), 100, 200, 0)));
        assert_eq!(c.unique_best_alignment_count, 1);
    }

    #[test]
    fn select_pe_unique_ob_maps_to_index_3() {
        // THE reconciliation test: OB → PE index 3 (NOT the SE `to_index` 1).
        let (d, _) = sel_pe(&[ob_pair(100, 200, -2, -2)]);
        assert_eq!(key_pe(&d), Some(("chr1".to_string(), 100, 200, 3)));
    }

    #[test]
    fn select_pe_runner_up_sets_second_best() {
        // Best OT (sum -4) + a lower hit (sum -8): unique best; MAPQ second-best =
        // the runner-up's sum (the AS runner-up over ALL mapped, not just `top`).
        let best = ot_pair(100, 200, -2, -2); // sum -4
        let lo = ot_pair(500, 600, -4, -4); // sum -8
        match sel_pe(&[best, lo]).0 {
            DecisionPaired::UniqueBest(b) => {
                assert_eq!((b.position_1, b.position_2, b.index), (100, 200, 0));
                assert_eq!(b.sum_of_alignment_scores, -4);
                assert_eq!(b.sum_of_alignment_scores_second_best, Some(-8));
            }
            other => panic!("expected UniqueBest, got {other:?}"),
        }
    }

    #[test]
    fn select_pe_cross_location_tie_is_ambiguous() {
        let (d, c) = sel_pe(&[ot_pair(100, 200, -2, -2), ot_pair(500, 600, -2, -2)]);
        assert!(matches!(d, DecisionPaired::Ambiguous { .. }));
        assert_eq!(c.unsuitable_sequence_count, 1);
    }

    #[test]
    fn select_pe_valid_and_spurious_tie_is_ambiguous() {
        // OT (valid) + a spurious pair (C→T read, R1 fwd on the GA sub-genome) at the
        // same best sum → Ambiguous (never a silent rescue of the valid hit).
        let ot = ot_pair(100, 200, -2, -2);
        let spur = mk_pair(
            99,
            "chr1_GA_converted",
            300,
            -2,
            147,
            "chr1_GA_converted",
            400,
            -2,
        );
        let (d, c) = sel_pe(&[ot, spur]);
        assert!(matches!(d, DecisionPaired::Ambiguous { .. }));
        assert_eq!(c.unsuitable_sequence_count, 1);
    }

    #[test]
    fn select_pe_all_spurious_is_no_alignment() {
        let spur = mk_pair(
            99,
            "chr1_GA_converted",
            300,
            -2,
            147,
            "chr1_GA_converted",
            400,
            -2,
        );
        let (d, c) = sel_pe(&[spur]);
        assert!(matches!(d, DecisionPaired::NoAlignment));
        assert_eq!(c.no_single_alignment_found, 1);
        assert_eq!(c.combined_spurious_count, 1);
    }

    #[test]
    fn select_pe_spurious_strictly_better_is_no_alignment() {
        // A spurious pair STRICTLY better (sum -2) than a valid OT (sum -8): the best
        // sum is the spurious one → top all-spurious → NoAlignment (never rescue the
        // worse valid hit).
        let spur = mk_pair(
            99,
            "chr1_GA_converted",
            300,
            -1,
            147,
            "chr1_GA_converted",
            400,
            -1,
        ); // sum -2
        let ot = ot_pair(100, 200, -4, -4); // sum -8
        let (d, c) = sel_pe(&[spur, ot]);
        assert!(matches!(d, DecisionPaired::NoAlignment));
        assert_eq!(c.combined_spurious_count, 1);
    }

    #[test]
    fn select_pe_same_locus_ot_ob_collision_kept_ob_wins() {
        // OT and OB at the SAME chr:pos1:pos2 + equal sum → ONE entry, won by OB
        // (index 3, the scan-order-last of {0,3}).
        let (d, _) = sel_pe(&[ot_pair(100, 200, -2, -2), ob_pair(100, 200, -2, -2)]);
        assert_eq!(key_pe(&d), Some(("chr1".to_string(), 100, 200, 3)));
    }

    #[test]
    fn select_core_pe_uses_literal_scan_order_not_ascending() {
        // Lock the literal [0,3,1,2] order: an OB (Ct→index 3) and a CTOB (Ga→index
        // 1) at the SAME locus + sum. The scan order visits 3 then 1 → CTOB(1) wins
        // (the faithful answer). An ascending-index sort would visit 1 then 3 →
        // OB(3), which is WRONG. Only reachable via the non-dir union, so this calls
        // `select_core_pe` directly with mixed Ct/Ga tags.
        let ob = ob_pair(100, 200, -2, -2); // Ct → OB → index 3
        let ctob = mk_pair(
            99,
            "chr1_GA_converted",
            100,
            -2,
            147,
            "chr1_GA_converted",
            200,
            -2,
        ); // Ga → CTOB → index 1
        let mut c = Counters::default();
        let d = select_core_pe(
            vec![(ReadConv::Ct, &ob), (ReadConv::Ga, &ctob)],
            "ACGTACGTAC",
            "ACGTACGTAC",
            0.0,
            -0.2,
            false,
            &mut c,
        )
        .unwrap();
        assert_eq!(key_pe(&d), Some(("chr1".to_string(), 100, 200, 1)));
    }

    #[test]
    fn select_pe_unmapped_only_is_no_alignment() {
        // The PE no-alignment marker (77,141) is filtered → empty → NoAlignment.
        let miss = SamPair::from_lines(
            "r1/1\t77\t*\t0\t0\t*\t*\t0\t0\tACGTACGTAC\tIIIIIIIIII",
            "r1/2\t141\t*\t0\t0\t*\t*\t0\t0\tACGTACGTAC\tIIIIIIIIII",
        )
        .unwrap();
        let (d, c) = sel_pe(&[miss]);
        assert!(matches!(d, DecisionPaired::NoAlignment));
        assert_eq!(c.no_single_alignment_found, 1);
    }

    #[test]
    fn select_pe_empty_is_no_alignment() {
        let (d, c) = sel_pe(&[]);
        assert!(matches!(d, DecisionPaired::NoAlignment));
        assert_eq!(c.no_single_alignment_found, 1);
    }

    #[test]
    fn select_pe_missing_as_errors() {
        let r1 =
            "r1/1\t99\tchr1_CT_converted\t100\t40\t10M\t=\t100\t0\tACGTACGTAC\tIIIIIIIIII\tMD:Z:10"; // no AS
        let p =
            SamPair::from_lines(r1, &pe_line("r1", 2, 147, "chr1_CT_converted", 200, -2)).unwrap();
        let mut c = Counters::default();
        let err =
            select_pe(&[p], "ACGTACGTAC", "ACGTACGTAC", 0.0, -0.2, false, &mut c).unwrap_err();
        assert!(format!("{err}").contains("alignment score"));
    }

    #[test]
    fn select_pe_missing_md_errors() {
        let r1 =
            "r1/1\t99\tchr1_CT_converted\t100\t40\t10M\t=\t100\t0\tACGTACGTAC\tIIIIIIIIII\tAS:i:-2"; // no MD
        let p =
            SamPair::from_lines(r1, &pe_line("r1", 2, 147, "chr1_CT_converted", 200, -2)).unwrap();
        let mut c = Counters::default();
        let err =
            select_pe(&[p], "ACGTACGTAC", "ACGTACGTAC", 0.0, -0.2, false, &mut c).unwrap_err();
        assert!(format!("{err}").contains("MD tag"));
    }

    #[test]
    fn select_pe_different_chromosome_errors() {
        let p = mk_pair(
            99,
            "chr1_CT_converted",
            100,
            -2,
            147,
            "chr2_CT_converted",
            200,
            -2,
        );
        let mut c = Counters::default();
        let err =
            select_pe(&[p], "ACGTACGTAC", "ACGTACGTAC", 0.0, -0.2, false, &mut c).unwrap_err();
        assert!(format!("{err}").contains("same chromosome"));
    }

    #[test]
    fn mechanism_pe_unique_best_matches_oracle() {
        // OT clear winner (sum -2) over OB (sum -8): combined picks OT idx 0; the
        // oracle (OT slot 0, OB slot 3) picks the same.
        let ot = ot_pair(100, 200, -1, -1);
        let ob = ob_pair(300, 400, -4, -4);
        let (d_comb, _) = sel_pe(&[ot.clone(), ob.clone()]);
        let d_orac = oracle_pe(Some(ot), Some(ob));
        assert_eq!(key_pe(&d_comb), key_pe(&d_orac));
        assert_eq!(key_pe(&d_comb), Some(("chr1".to_string(), 100, 200, 0)));
    }

    #[test]
    fn mechanism_pe_same_locus_collision_matches_oracle() {
        // OT and OB at the same locus + sum: combined → OB(3); the oracle (slot 0
        // OT, slot 3 OB, `>=` overwrite in scan order [0,3,1,2]) → OB(3).
        let ot = ot_pair(100, 200, -2, -2);
        let ob = ob_pair(100, 200, -2, -2);
        let (d_comb, _) = sel_pe(&[ot.clone(), ob.clone()]);
        let d_orac = oracle_pe(Some(ot), Some(ob));
        assert_eq!(key_pe(&d_comb), key_pe(&d_orac));
        assert_eq!(key_pe(&d_comb), Some(("chr1".to_string(), 100, 200, 3)));
    }

    #[test]
    fn mechanism_pe_cross_location_matches_oracle() {
        // OT and OB at different loci, equal sum → both paths Ambiguous.
        let ot = ot_pair(100, 200, -2, -2);
        let ob = ob_pair(300, 400, -2, -2);
        let (d_comb, _) = sel_pe(&[ot.clone(), ob.clone()]);
        let d_orac = oracle_pe(Some(ot), Some(ob));
        assert!(matches!(d_comb, DecisionPaired::Ambiguous { .. }));
        assert!(matches!(d_orac, DecisionPaired::Ambiguous { .. }));
    }

    // ===================================================================
    // Non-directional PE combined selection (Phase 3) — select_pe_nondir,
    // cross-checked against check_results_paired_end(directional=false).
    // ===================================================================

    /// CTOT-shaped pair (G→A pass): R1 rev (flag 83) + R2 fwd (163) on the CT
    /// sub-genome → `classify(Ga, rev, _CT)` = CTOT → `to_index_pe` 2.
    fn ctot_pair(pos1: u32, pos2: u32, as1: i64, as2: i64) -> SamPair {
        mk_pair(
            83,
            "chr1_CT_converted",
            pos1,
            as1,
            163,
            "chr1_CT_converted",
            pos2,
            as2,
        )
    }
    /// CTOB-shaped pair (G→A pass): R1 fwd (flag 99) + R2 rev (147) on the GA
    /// sub-genome → `classify(Ga, fwd, _GA)` = CTOB → `to_index_pe` 1.
    fn ctob_pair(pos1: u32, pos2: u32, as1: i64, as2: i64) -> SamPair {
        mk_pair(
            99,
            "chr1_GA_converted",
            pos1,
            as1,
            147,
            "chr1_GA_converted",
            pos2,
            as2,
        )
    }

    /// Run non-dir `select_pe_nondir` over the C→T pass pairs + G→A pass pairs.
    fn sel_pe_nondir(ct: &[SamPair], ga: &[SamPair]) -> (DecisionPaired, Counters) {
        let mut c = Counters::default();
        let d =
            select_pe_nondir(ct, ga, "ACGTACGTAC", "ACGTACGTAC", 0.0, -0.2, false, &mut c).unwrap();
        (d, c)
    }

    /// The faithful 4-instance PE oracle, slots 0=OT 1=CTOB 2=CTOT 3=OB (the
    /// non-dir `pe_instance_plan` numbering), `directional=false` (the index-1/2
    /// reject is off). Each pair is placed in the slot its `classify` index maps to,
    /// so the oracle scan assigns the same index the mechanism does.
    fn oracle_pe_nondir(
        ot: Option<SamPair>,
        ctob: Option<SamPair>,
        ctot: Option<SamPair>,
        ob: Option<SamPair>,
    ) -> DecisionPaired {
        let mk = |p: Option<SamPair>| {
            p.map(|pp| VecPairStream {
                pairs: vec![pp],
                pos: 0,
            })
        };
        let mut streams: Vec<Option<VecPairStream>> = vec![mk(ot), mk(ctob), mk(ctot), mk(ob)];
        let mut c = Counters::default();
        crate::aligner::merge::check_results_paired_end(
            "r1",
            "ACGTACGTAC",
            "ACGTACGTAC",
            &mut streams,
            false, // non-directional → index-1/2 reject OFF
            0.0,
            -0.2,
            false,
            false,
            crate::aligner::config::Aligner::Bowtie2,
            &mut c,
        )
        .unwrap()
    }

    /// The PE no-alignment marker pair (FLAG 77/141).
    fn miss_pair() -> SamPair {
        SamPair::from_lines(
            "r1/1\t77\t*\t0\t0\t*\t*\t0\t0\tACGTACGTAC\tIIIIIIIIII",
            "r1/2\t141\t*\t0\t0\t*\t*\t0\t0\tACGTACGTAC\tIIIIIIIIII",
        )
        .unwrap()
    }

    #[test]
    fn select_pe_nondir_unique_ctot_maps_to_index_2() {
        // G→A pass only → CTOT → PE index 2 (a slot the directional select_pe can't reach).
        let (d, c) = sel_pe_nondir(&[], &[ctot_pair(100, 200, -2, -2)]);
        assert_eq!(key_pe(&d), Some(("chr1".to_string(), 100, 200, 2)));
        assert_eq!(c.unique_best_alignment_count, 1);
    }

    #[test]
    fn select_pe_nondir_unique_ctob_maps_to_index_1() {
        let (d, _) = sel_pe_nondir(&[], &[ctob_pair(100, 200, -2, -2)]);
        assert_eq!(key_pe(&d), Some(("chr1".to_string(), 100, 200, 1)));
    }

    #[test]
    fn select_pe_nondir_same_locus_ot_ctob_kept_ctob_wins() {
        // OT (C→T pass, index 0) and CTOB (G→A pass, index 1) at the SAME locus + sum
        // → ONE map entry (same `chr:pos1:pos2` key), won by CTOB (index 1, scan-order-
        // last of {0,1} in [0,3,1,2]). The same-locus-in-both-passes collapse.
        let (d, _) = sel_pe_nondir(&[ot_pair(100, 200, -2, -2)], &[ctob_pair(100, 200, -2, -2)]);
        assert_eq!(key_pe(&d), Some(("chr1".to_string(), 100, 200, 1)));
    }

    #[test]
    fn select_pe_nondir_same_locus_ob_ctot_kept_ctot_wins() {
        // OB (index 3) and CTOT (index 2) at the same locus → CTOT (index 2, scan-order-
        // last of {3,2} in [0,3,1,2]).
        let (d, _) = sel_pe_nondir(&[ob_pair(100, 200, -2, -2)], &[ctot_pair(100, 200, -2, -2)]);
        assert_eq!(key_pe(&d), Some(("chr1".to_string(), 100, 200, 2)));
    }

    #[test]
    fn select_pe_nondir_cross_location_is_ambiguous() {
        // OT at one locus + CTOT at another, equal sum → 2 map entries → Ambiguous.
        let (d, c) = sel_pe_nondir(&[ot_pair(100, 200, -2, -2)], &[ctot_pair(500, 600, -2, -2)]);
        assert!(matches!(d, DecisionPaired::Ambiguous { .. }));
        assert_eq!(c.unsuitable_sequence_count, 1);
    }

    #[test]
    fn select_pe_nondir_all_spurious_is_no_alignment() {
        // A spurious pair in each pass (C→T fwd on GA = spurious; G→A fwd on CT = spurious).
        let ct_spur = mk_pair(
            99,
            "chr1_GA_converted",
            100,
            -2,
            147,
            "chr1_GA_converted",
            200,
            -2,
        );
        let ga_spur = mk_pair(
            99,
            "chr1_CT_converted",
            300,
            -2,
            147,
            "chr1_CT_converted",
            400,
            -2,
        );
        let (d, c) = sel_pe_nondir(&[ct_spur], &[ga_spur]);
        assert!(matches!(d, DecisionPaired::NoAlignment));
        assert_eq!(c.combined_spurious_count, 1);
    }

    #[test]
    fn select_pe_nondir_both_passes_miss_is_no_alignment() {
        let (d, c) = sel_pe_nondir(&[miss_pair()], &[miss_pair()]);
        assert!(matches!(d, DecisionPaired::NoAlignment));
        assert_eq!(c.no_single_alignment_found, 1);
    }

    #[test]
    fn mechanism_pe_nondir_ctot_matches_oracle() {
        // CTOT clear winner: combined picks index 2; oracle (CTOT in slot 2,
        // directional=false) picks index 2.
        let ctot = ctot_pair(100, 200, -2, -2);
        let (d_comb, _) = sel_pe_nondir(&[], std::slice::from_ref(&ctot));
        let d_orac = oracle_pe_nondir(None, None, Some(ctot), None);
        assert_eq!(key_pe(&d_comb), key_pe(&d_orac));
        assert_eq!(key_pe(&d_comb), Some(("chr1".to_string(), 100, 200, 2)));
    }

    #[test]
    fn mechanism_pe_nondir_ctob_matches_oracle() {
        let ctob = ctob_pair(100, 200, -2, -2);
        let (d_comb, _) = sel_pe_nondir(&[], std::slice::from_ref(&ctob));
        let d_orac = oracle_pe_nondir(None, Some(ctob), None, None);
        assert_eq!(key_pe(&d_comb), key_pe(&d_orac));
        assert_eq!(key_pe(&d_comb), Some(("chr1".to_string(), 100, 200, 1)));
    }

    #[test]
    fn mechanism_pe_nondir_same_locus_ot_ctob_matches_oracle() {
        // The headline cross-strand collision: combined → CTOB(1); oracle (OT slot 0 +
        // CTOB slot 1, same locus/sum, scan [0,3,1,2] → slot 1 overwrites slot 0) → CTOB(1).
        let ot = ot_pair(100, 200, -2, -2);
        let ctob = ctob_pair(100, 200, -2, -2);
        let (d_comb, _) = sel_pe_nondir(std::slice::from_ref(&ot), std::slice::from_ref(&ctob));
        let d_orac = oracle_pe_nondir(Some(ot), Some(ctob), None, None);
        assert_eq!(key_pe(&d_comb), key_pe(&d_orac));
        assert_eq!(key_pe(&d_comb), Some(("chr1".to_string(), 100, 200, 1)));
    }

    #[test]
    fn mechanism_pe_nondir_same_locus_ob_ctot_matches_oracle() {
        let ob = ob_pair(100, 200, -2, -2);
        let ctot = ctot_pair(100, 200, -2, -2);
        let (d_comb, _) = sel_pe_nondir(std::slice::from_ref(&ob), std::slice::from_ref(&ctot));
        let d_orac = oracle_pe_nondir(None, None, Some(ctot), Some(ob));
        assert_eq!(key_pe(&d_comb), key_pe(&d_orac));
        assert_eq!(key_pe(&d_comb), Some(("chr1".to_string(), 100, 200, 2)));
    }

    // ===================================================================
    // PBAT PE combined selection (Phase 4) — select_pe_pbat, the single
    // G→A-pass half of non-dir standalone (only CTOT/CTOB reachable),
    // cross-checked against check_results_paired_end(directional=false).
    // ===================================================================

    /// Run pbat `select_pe_pbat` over the single G→A pass's candidate pairs.
    fn sel_pe_pbat(pairs: &[SamPair]) -> (DecisionPaired, Counters) {
        let mut c = Counters::default();
        let d =
            select_pe_pbat(pairs, "ACGTACGTAC", "ACGTACGTAC", 0.0, -0.2, false, &mut c).unwrap();
        (d, c)
    }

    #[test]
    fn select_pe_pbat_unique_ctot_maps_to_index_2() {
        // One G→A pass; a CTOT pair → PE index 2. (The original-strand C→T slots OT/OB
        // are unreachable under pbat's `Ga` tag.)
        let (d, c) = sel_pe_pbat(&[ctot_pair(100, 200, -2, -2)]);
        assert_eq!(key_pe(&d), Some(("chr1".to_string(), 100, 200, 2)));
        assert_eq!(c.unique_best_alignment_count, 1);
    }

    #[test]
    fn select_pe_pbat_unique_ctob_maps_to_index_1() {
        let (d, _) = sel_pe_pbat(&[ctob_pair(100, 200, -2, -2)]);
        assert_eq!(key_pe(&d), Some(("chr1".to_string(), 100, 200, 1)));
    }

    #[test]
    fn select_pe_pbat_original_strand_orientation_is_spurious() {
        // An OT-shaped pair (R1 fwd on _CT) is original-strand orientation → spurious
        // under `Ga` (only Ga+rev on CT = CTOT is valid). All-spurious → NoAlignment.
        let (d, c) = sel_pe_pbat(&[ot_pair(100, 200, -2, -2)]);
        assert!(matches!(d, DecisionPaired::NoAlignment));
        assert_eq!(c.combined_spurious_count, 1);
    }

    #[test]
    fn select_pe_pbat_valid_and_spurious_tie_is_ambiguous() {
        // A valid CTOT + a spurious OT-shaped pair at the same best sum → Ambiguous
        // (never a silent rescue of the valid hit).
        let (d, _) = sel_pe_pbat(&[ctot_pair(100, 200, -2, -2), ot_pair(500, 600, -2, -2)]);
        assert!(matches!(d, DecisionPaired::Ambiguous { .. }));
    }

    #[test]
    fn select_pe_pbat_same_locus_ctot_ctob_kept_ctot_wins() {
        // CTOT (index 2) and CTOB (index 1) at the SAME locus + sum → ONE map entry, won
        // by CTOT (index 2, scan-order-last of {1,2} in [0,3,1,2]). NOTE this is the
        // OPPOSITE of SE pbat (where CTOB wins) — the PE renumbering + literal scan order.
        // (review B-I1) Pre-assert the canned pairs classify to the intended slots so the
        // divergent-from-SE result cannot pass for the wrong reason.
        let ctot = ctot_pair(100, 200, -2, -2);
        let ctob = ctob_pair(100, 200, -2, -2);
        assert_eq!(
            classify(ReadConv::Ga, ctot.read1.flag, &ctot.read1.rname)
                .unwrap()
                .1,
            CombinedClass::Ctot
        );
        assert_eq!(
            classify(ReadConv::Ga, ctob.read1.flag, &ctob.read1.rname)
                .unwrap()
                .1,
            CombinedClass::Ctob
        );
        let (d, _) = sel_pe_pbat(&[ctot, ctob]);
        assert_eq!(key_pe(&d), Some(("chr1".to_string(), 100, 200, 2)));
    }

    #[test]
    fn select_pe_pbat_cross_location_is_ambiguous() {
        // CTOT at one locus + CTOB at another, equal sum → 2 map entries → Ambiguous.
        let (d, c) = sel_pe_pbat(&[ctot_pair(100, 200, -2, -2), ctob_pair(500, 600, -2, -2)]);
        assert!(matches!(d, DecisionPaired::Ambiguous { .. }));
        assert_eq!(c.unsuitable_sequence_count, 1);
    }

    #[test]
    fn select_pe_pbat_miss_is_no_alignment() {
        let (d, c) = sel_pe_pbat(&[miss_pair()]);
        assert!(matches!(d, DecisionPaired::NoAlignment));
        assert_eq!(c.no_single_alignment_found, 1);
    }

    #[test]
    fn mechanism_pe_pbat_ctot_matches_oracle() {
        // CTOT clear winner: combined → index 2; oracle (CTOT in slot 2, directional=
        // false) → index 2. Reuses the non-dir oracle helper (pbat populates only the
        // CTOT/CTOB slots).
        let ctot = ctot_pair(100, 200, -2, -2);
        let (d_comb, _) = sel_pe_pbat(std::slice::from_ref(&ctot));
        let d_orac = oracle_pe_nondir(None, None, Some(ctot), None);
        assert_eq!(key_pe(&d_comb), key_pe(&d_orac));
        assert_eq!(key_pe(&d_comb), Some(("chr1".to_string(), 100, 200, 2)));
    }

    #[test]
    fn mechanism_pe_pbat_ctob_matches_oracle() {
        let ctob = ctob_pair(100, 200, -2, -2);
        let (d_comb, _) = sel_pe_pbat(std::slice::from_ref(&ctob));
        let d_orac = oracle_pe_nondir(None, Some(ctob), None, None);
        assert_eq!(key_pe(&d_comb), key_pe(&d_orac));
        assert_eq!(key_pe(&d_comb), Some(("chr1".to_string(), 100, 200, 1)));
    }

    #[test]
    fn mechanism_pe_pbat_same_locus_ctot_ctob_matches_oracle() {
        // The headline tie: combined → CTOT(2); oracle (CTOB slot 1 + CTOT slot 2, same
        // locus/sum, scan [0,3,1,2] → slot 2 overwrites slot 1) → CTOT(2). (review B-I1)
        // pre-assert the canned pairs' classes.
        let ctot = ctot_pair(100, 200, -2, -2);
        let ctob = ctob_pair(100, 200, -2, -2);
        assert_eq!(
            classify(ReadConv::Ga, ctot.read1.flag, &ctot.read1.rname)
                .unwrap()
                .1,
            CombinedClass::Ctot
        );
        assert_eq!(
            classify(ReadConv::Ga, ctob.read1.flag, &ctob.read1.rname)
                .unwrap()
                .1,
            CombinedClass::Ctob
        );
        let (d_comb, _) = sel_pe_pbat(&[ctot.clone(), ctob.clone()]);
        let d_orac = oracle_pe_nondir(None, Some(ctob), Some(ctot), None);
        assert_eq!(key_pe(&d_comb), key_pe(&d_orac));
        assert_eq!(key_pe(&d_comb), Some(("chr1".to_string(), 100, 200, 2)));
    }
}

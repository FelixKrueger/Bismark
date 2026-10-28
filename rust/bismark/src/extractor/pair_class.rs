//! Discordant-pair classification for `--allow_discordant`.
//!
//! A general-purpose bisulfite aligner emitting Bismark-format `XM`/`XR`/`XG`
//! tags (unlike the Bismark aligner) also emits cross-chromosome pairs,
//! same-chromosome discordant-orientation pairs, and half-mapped pairs. Under
//! `--allow_discordant` the producer classifies each adjacent primary pair into
//! one of two processing paths via [`classify_pair`].
//!
//! This module is shared by both `parallel.rs` (the shipping pipeline) and
//! `pipeline.rs` (the single-threaded byte-identity reference) so the
//! "parallel ≡ single-threaded" oracle stays valid for the new classes.
//!
//! ## Byte-identity
//!
//! [`classify_pair`] runs **only** under `--allow_discordant`. With the flag
//! off, the producer forms pairs exactly as before (`from_mates` → `Pe`), so
//! this code is never reached and the default path is unchanged.
//!
//! For every pair the Bismark aligner can emit (proper FR pairs whose two mates
//! carry complementary strand classes — `OT`↔`CTOT`, `OB`↔`CTOB`), the result
//! is always [`PairClass::Concordant`], so flag-on output on Bismark input is
//! byte-identical to the default path (modulo the opt-in report section).

use crate::extractor::overlap::is_forward_pair_strand;
use crate::io::{BismarkPair, BismarkStrand};

/// How a pair of adjacent primary records is processed under
/// `--allow_discordant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairClass {
    /// Same reference id, proper FR orientation, geometry consistent with the
    /// strand class → today's concordant path verbatim (pair-strand routing +
    /// the Perl half-plane `drop_overlap`).
    Concordant,
    /// Different reference ids, OR same-chr with an orientation/geometry that
    /// the half-plane `drop_overlap` cannot handle. Each mate is called
    /// independently, single-end style. Same-chromosome overlaps are handled
    /// by a generic reference-position dedup in the worker (`drop_overlap` is
    /// NOT reused — its half-plane predicate is valid only for FR pairs).
    Independent,
}

/// Classify an adjacent primary pair, evaluated only under `--allow_discordant`.
///
/// A pair is [`PairClass::Concordant`] iff **all** of:
/// 1. `r1_refid == r2_refid` (same chromosome), AND
/// 2. the two mates carry **complementary** strand classes — one forward
///    (`OT`/`CTOB`), one reverse (`OB`/`CTOT`). Every Bismark FR pair satisfies
///    this (`OT`↔`CTOT`, `OB`↔`CTOB`); a same-orientation (FF/RR) discordant
///    pair does not, and `drop_overlap`'s half-plane predicate would silently
///    mangle it — so it routes to [`PairClass::Independent`], AND
/// 3. the two mates share a **strand of origin** — both top (`OT`/`CTOT`,
///    `XG=CT`) or both bottom (`OB`/`CTOB`, `XG=GA`). Orientation (condition 2)
///    is orthogonal to origin, so on its own it still admits the two
///    opposite-origin combos an aligner can emit at a discordant locus but
///    Bismark never does — `OT`+`OB` and `CTOB`+`CTOT`. Those route to
///    [`PairClass::Independent`], where each mate is called against its own
///    strand (see the check's inline comment for why routing them to
///    `Concordant` would lose and mis-attribute the mate's calls), AND
/// 4. the reference spans are consistent with the strand-implied direction
///    (forward-class: R2 not entirely upstream of R1; reverse-class: R2 not
///    entirely downstream). A pair failing this gate is provably span-disjoint
///    (the annihilation case the half-plane predicate would drop wholesale), so
///    routing it to `Independent` and calling both mates fully is exact.
///
/// Spans are **untrimmed** (independent of `--ignore_3prime`): trimming only
/// shrinks R1's effective span, and the concordant cases hold for any shrinkage.
///
/// Conditions 2 and 3 are byte-neutral for Bismark input (every Bismark pair is
/// complementary and same-origin) and only re-route pairs the Bismark aligner
/// never emits; they keep same-orientation and opposite-origin overlapping pairs
/// off the FR-only half-plane `drop_overlap` path.
#[must_use]
pub fn classify_pair(pair: &BismarkPair, r1_refid: usize, r2_refid: usize) -> PairClass {
    if r1_refid != r2_refid {
        return PairClass::Independent;
    }

    // Complementary-orientation check: a proper FR pair has one forward-class
    // and one reverse-class mate. A same-orientation (FF/RR) pair fails this
    // and must NOT take the half-plane `drop_overlap` path.
    let r1_forward = is_forward_pair_strand(pair.r1().record_strand());
    let r2_forward = is_forward_pair_strand(pair.r2().record_strand());
    if r1_forward == r2_forward {
        return PairClass::Independent;
    }

    // Strand-of-origin check. Forward/reverse orientation is ORTHOGONAL to which
    // original strand a mate came from (the `XG` tag: `CT` = top {OT,CTOT},
    // `GA` = bottom {OB,CTOB}). A genuine Bismark concordant pair has BOTH mates
    // from the same original strand — `{OT,CTOT}` (both top) or `{OB,CTOB}`
    // (both bottom). The orientation check alone still admits the two
    // opposite-origin combos an aligner can emit at a discordant locus but
    // Bismark never does — `OT`+`OB` and `CTOB`+`CTOT`. Routed to `Concordant`,
    // those take the FR-only half-plane `drop_overlap`, which mis-drops the
    // mate's overlap-region calls AND mis-attributes its surviving calls to R1's
    // strand's output files (R2's genuinely opposite-strand cytosines are never
    // at a ref position R1 also calls, so they are lost, not deduped). Require
    // same origin and route the mismatches to `Independent`, where each mate is
    // called against its own strand. Order-agnostic: non-directional libraries
    // present a valid pair as `r1=CTOT, r2=OT` (see `from_mates_ctot_pair_non_directional`).
    if is_top_origin(pair.r1().record_strand()) != is_top_origin(pair.r2().record_strand()) {
        return PairClass::Independent;
    }

    // Geometry gate (untrimmed spans). `pair_strand()` is R1's record strand,
    // so `r1_forward` is exactly `is_forward_pair_strand(pair.pair_strand())`.
    let (Some(r1_start), Some(r2_start)) =
        (pair.r1().alignment_start(), pair.r2().alignment_start())
    else {
        // A mapped primary always has an alignment start; defensively route a
        // start-less record to Independent rather than risk a wrong drop.
        return PairClass::Independent;
    };
    let r1_ref_end = reference_end_of(pair.r1(), r1_start);
    let r2_ref_end = reference_end_of(pair.r2(), r2_start);

    let concordant = if r1_forward {
        // Forward-class (OT/CTOB): R1 is the upstream mate. R2 not entirely
        // upstream of R1.
        r2_ref_end >= r1_start
    } else {
        // Reverse-class (OB/CTOT): R1 is the downstream mate. R2 not entirely
        // downstream of R1.
        r2_start <= r1_ref_end
    };

    if concordant {
        PairClass::Concordant
    } else {
        PairClass::Independent
    }
}

/// Strand of origin (the `XG` tag): `true` for the top strand (`OT`/`CTOT`,
/// `XG=CT`), `false` for the bottom strand (`OB`/`CTOB`, `XG=GA`). Two mates of
/// a genuine Bismark concordant pair always agree on this.
fn is_top_origin(strand: BismarkStrand) -> bool {
    matches!(strand, BismarkStrand::OT | BismarkStrand::CTOT)
}

/// 1-based inclusive last reference position covered by `record`, from its
/// 1-based `start` and its CIGAR (InDel-aware via [`crate::io::CigarExt`]).
fn reference_end_of(record: &crate::io::BismarkRecord, start: usize) -> usize {
    use crate::io::CigarExt;
    record.cigar().reference_end(start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::{BismarkPair, BismarkRecord, BismarkStrand};
    use bstr::BString;
    use noodles_core::Position;
    use noodles_sam::alignment::record::Flags;
    use noodles_sam::alignment::record::cigar::Op;
    use noodles_sam::alignment::record::cigar::op::Kind;
    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::data::field::Value;
    use noodles_sam::alignment::record_buf::{Cigar, RecordBuf, Sequence};

    /// Synthesize a single-`M` `BismarkRecord`.
    fn synth(xr: &[u8], xg: &[u8], start: usize, len: usize, refid: usize) -> BismarkRecord {
        let mut record = RecordBuf::default();
        *record.name_mut() = Some(BString::from(b"q".to_vec()));
        *record.flags_mut() = Flags::from(0x1);
        *record.sequence_mut() = Sequence::from(vec![b'A'; len]);
        *record.alignment_start_mut() = Some(Position::try_from(start).unwrap());
        *record.reference_sequence_id_mut() = Some(refid);
        *record.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, len)]);
        let xm = vec![b'.'; len];
        record
            .data_mut()
            .insert(Tag::from(*b"XR"), Value::String(BString::from(xr.to_vec())));
        record
            .data_mut()
            .insert(Tag::from(*b"XG"), Value::String(BString::from(xg.to_vec())));
        record
            .data_mut()
            .insert(Tag::from(*b"XM"), Value::String(BString::from(xm)));
        BismarkRecord::from_noodles_record(record).unwrap()
    }

    /// OT R1 (XR=CT,XG=CT, forward), CTOT R2 (XR=GA,XG=CT, reverse): proper FR.
    fn fr_pair(r1_start: usize, r2_start: usize, len: usize, refid: usize) -> BismarkPair {
        let r1 = synth(b"CT", b"CT", r1_start, len, refid);
        let r2 = synth(b"GA", b"CT", r2_start, len, refid);
        assert_eq!(r1.record_strand(), BismarkStrand::OT);
        assert_eq!(r2.record_strand(), BismarkStrand::CTOT);
        BismarkPair::from_mates(r1, r2).unwrap()
    }

    #[test]
    fn cross_chr_is_independent() {
        let r1 = synth(b"CT", b"CT", 100, 50, 0);
        let r2 = synth(b"GA", b"CT", 100, 50, 1);
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(classify_pair(&pair, 0, 1), PairClass::Independent);
    }

    #[test]
    fn forward_r2_downstream_disjoint_is_concordant() {
        // OT R1 [100,149], CTOT R2 [200,249] — disjoint, R2 downstream.
        let pair = fr_pair(100, 200, 50, 0);
        assert_eq!(classify_pair(&pair, 0, 0), PairClass::Concordant);
    }

    #[test]
    fn forward_dovetail_overlap_is_concordant() {
        // OT R1 [100,149], CTOT R2 [90,139] — R2 starts upstream but overlaps.
        let pair = fr_pair(100, 90, 50, 0);
        assert_eq!(classify_pair(&pair, 0, 0), PairClass::Concordant);
    }

    #[test]
    fn forward_r2_entirely_upstream_is_independent() {
        // OT R1 [200,249], CTOT R2 [100,149] — R2 end (149) < R1 start (200).
        let pair = fr_pair(200, 100, 50, 0);
        assert_eq!(classify_pair(&pair, 0, 0), PairClass::Independent);
    }

    #[test]
    fn forward_boundary_equality_is_concordant() {
        // OT R1 [150,199], CTOT R2 [100,150] — r2_ref_end (150 == r1_start 150).
        // Boundary equality routes to Concordant per the `>=` gate.
        let r1 = synth(b"CT", b"CT", 150, 50, 0);
        let r2 = synth(b"GA", b"CT", 100, 51, 0); // ends at 150
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(classify_pair(&pair, 0, 0), PairClass::Concordant);
    }

    #[test]
    fn reverse_class_mirror_disjoint_is_independent() {
        // OB R1 (XR=CT,XG=GA, reverse) [100,149], CTOB R2 (XR=GA,XG=GA, forward)
        // [200,249] — R2 start (200) > R1 end (149) → Independent.
        let r1 = synth(b"CT", b"GA", 100, 50, 0);
        let r2 = synth(b"GA", b"GA", 200, 50, 0);
        assert_eq!(r1.record_strand(), BismarkStrand::OB);
        assert_eq!(r2.record_strand(), BismarkStrand::CTOB);
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(classify_pair(&pair, 0, 0), PairClass::Independent);
    }

    #[test]
    fn reverse_class_overlap_is_concordant() {
        // OB R1 [100,149], CTOB R2 [120,169] — r2_start (120) <= r1_end (149).
        let r1 = synth(b"CT", b"GA", 100, 50, 0);
        let r2 = synth(b"GA", b"GA", 120, 50, 0);
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(classify_pair(&pair, 0, 0), PairClass::Concordant);
    }

    #[test]
    fn same_orientation_ff_pair_is_independent() {
        // Both mates forward-class (OT R1 + CTOB R2, both forward) on the same
        // chromosome, overlapping — a same-orientation (FF) discordant pair.
        // The complementary-orientation check routes it to Independent even
        // though the geometry gate alone would say Concordant.
        let r1 = synth(b"CT", b"CT", 100, 50, 0); // OT (forward)
        let r2 = synth(b"GA", b"GA", 120, 50, 0); // CTOB (forward)
        assert_eq!(r1.record_strand(), BismarkStrand::OT);
        assert_eq!(r2.record_strand(), BismarkStrand::CTOB);
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(classify_pair(&pair, 0, 0), PairClass::Independent);
    }

    #[test]
    fn opposite_origin_ot_ob_pair_is_independent() {
        // OT R1 (forward, top-origin XG=CT) + OB R2 (reverse, bottom-origin
        // XG=GA), same chr, OVERLAPPING. Complementary orientation passes, but
        // the mates come from opposite original strands — a pair Bismark never
        // emits. Must route to Independent (not Concordant), else drop_overlap
        // would mis-drop/mis-attribute R2's bottom-strand calls.
        let r1 = synth(b"CT", b"CT", 100, 50, 0); // OT
        let r2 = synth(b"CT", b"GA", 120, 50, 0); // OB
        assert_eq!(r1.record_strand(), BismarkStrand::OT);
        assert_eq!(r2.record_strand(), BismarkStrand::OB);
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(classify_pair(&pair, 0, 0), PairClass::Independent);
    }

    #[test]
    fn opposite_origin_ctob_ctot_pair_is_independent() {
        // CTOB R1 (forward, bottom-origin XG=GA) + CTOT R2 (reverse, top-origin
        // XG=CT), same chr, overlapping. The mirror of the OT+OB case.
        let r1 = synth(b"GA", b"GA", 100, 50, 0); // CTOB
        let r2 = synth(b"GA", b"CT", 120, 50, 0); // CTOT
        assert_eq!(r1.record_strand(), BismarkStrand::CTOB);
        assert_eq!(r2.record_strand(), BismarkStrand::CTOT);
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(classify_pair(&pair, 0, 0), PairClass::Independent);
    }

    #[test]
    fn non_directional_swapped_order_ctot_ot_is_concordant() {
        // Non-directional libraries can present a genuine top-origin pair in
        // swapped file order: r1=CTOT (reverse), r2=OT (forward). Same origin
        // (both top), complementary orientation, overlapping → must remain
        // Concordant (the order-agnostic origin check must not regress this).
        let r1 = synth(b"GA", b"CT", 120, 50, 0); // CTOT (reverse)
        let r2 = synth(b"CT", b"CT", 100, 50, 0); // OT (forward)
        assert_eq!(r1.record_strand(), BismarkStrand::CTOT);
        assert_eq!(r2.record_strand(), BismarkStrand::OT);
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(classify_pair(&pair, 0, 0), PairClass::Concordant);
    }
}

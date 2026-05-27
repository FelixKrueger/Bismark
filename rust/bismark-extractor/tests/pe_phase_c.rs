//! Phase C unit tests — PE extraction loop, overlap detection, per-mate
//! ignore trims, SE-vs-PE auto-detect.
//!
//! Test names mirror plan §7.1's labels. End-to-end smoke that runs the
//! binary on a synthetic PE BAM lives at `tests/pe_phase_c_smoke.rs`.

#![allow(non_snake_case)]

use bismark_extractor::call::{CytosineContext, MethCall};
use bismark_extractor::cli::{Cli, PairedMode};
use bismark_extractor::overlap::{drop_overlap, is_forward_pair_strand};
use bismark_io::{BismarkPair, BismarkStrand};
use clap::Parser;

// ─────────────────────────────────────────────────────────────────────────
// Test helpers — synthetic BismarkRecord + BismarkPair construction
// ─────────────────────────────────────────────────────────────────────────

mod helpers {
    use bismark_io::{BismarkPair, BismarkRecord};
    use bstr::BString;
    use noodles_core::Position;
    use noodles_sam::alignment::record::Flags;
    use noodles_sam::alignment::record::cigar::Op;
    use noodles_sam::alignment::record::cigar::op::Kind;
    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::data::field::Value;
    use noodles_sam::alignment::record_buf::{Cigar, RecordBuf, Sequence};

    /// Build a synthetic `BismarkRecord` with the given XR/XG/XM, sequence,
    /// alignment_start, CIGAR ops, FLAG, QNAME, and refid.
    #[allow(clippy::too_many_arguments)]
    pub fn synth(
        xr: &[u8],
        xg: &[u8],
        xm: &[u8],
        seq: &[u8],
        alignment_start: usize,
        cigar_ops: &[(Kind, usize)],
        flags: u16,
        qname: &[u8],
        refid: usize,
    ) -> BismarkRecord {
        let mut record = RecordBuf::default();
        *record.flags_mut() = Flags::from(flags);
        *record.sequence_mut() = Sequence::from(seq.to_vec());
        *record.alignment_start_mut() = Some(Position::try_from(alignment_start).unwrap());
        *record.reference_sequence_id_mut() = Some(refid);
        *record.cigar_mut() = Cigar::from(
            cigar_ops
                .iter()
                .map(|(k, n)| Op::new(*k, *n))
                .collect::<Vec<_>>(),
        );
        *record.name_mut() = Some(BString::from(qname.to_vec()));
        record
            .data_mut()
            .insert(Tag::from(*b"XR"), Value::String(BString::from(xr.to_vec())));
        record
            .data_mut()
            .insert(Tag::from(*b"XG"), Value::String(BString::from(xg.to_vec())));
        record
            .data_mut()
            .insert(Tag::from(*b"XM"), Value::String(BString::from(xm.to_vec())));
        BismarkRecord::from_noodles_record(record).expect("synth produces a valid BismarkRecord")
    }

    /// Build an OT pair: R1 with XR=CT XG=CT (record_strand=OT, pair_strand=OT),
    /// R2 with XR=GA XG=CT (record_strand=CTOT). FLAG bits set per SAM spec.
    /// R1/R2 share qname. `r1_start` / `r2_start` are the alignment starts;
    /// CIGAR is a single Match op spanning the XM length on each side.
    pub fn ot_pair(
        r1_xm: &[u8],
        r1_start: usize,
        r2_xm: &[u8],
        r2_start: usize,
        qname: &[u8],
    ) -> BismarkPair {
        let r1 = synth(
            b"CT",
            b"CT",
            r1_xm,
            &vec![b'A'; r1_xm.len()],
            r1_start,
            &[(Kind::Match, r1_xm.len())],
            0x41, // paired + first-in-pair
            qname,
            0,
        );
        let r2 = synth(
            b"GA",
            b"CT",
            r2_xm,
            &vec![b'A'; r2_xm.len()],
            r2_start,
            &[(Kind::Match, r2_xm.len())],
            0x81, // paired + last-in-pair
            qname,
            0,
        );
        BismarkPair::from_mates(r1, r2).expect("OT pair construction")
    }

    /// Build an OB pair: R1 with XR=CT XG=GA (record_strand=OB, pair_strand=OB),
    /// R2 with XR=GA XG=GA (record_strand=CTOB). R1 is downstream, R2 upstream.
    pub fn ob_pair(
        r1_xm: &[u8],
        r1_start: usize,
        r2_xm: &[u8],
        r2_start: usize,
        qname: &[u8],
    ) -> BismarkPair {
        let r1 = synth(
            b"CT",
            b"GA",
            r1_xm,
            &vec![b'A'; r1_xm.len()],
            r1_start,
            &[(Kind::Match, r1_xm.len())],
            0x41,
            qname,
            0,
        );
        let r2 = synth(
            b"GA",
            b"GA",
            r2_xm,
            &vec![b'A'; r2_xm.len()],
            r2_start,
            &[(Kind::Match, r2_xm.len())],
            0x81,
            qname,
            0,
        );
        BismarkPair::from_mates(r1, r2).expect("OB pair construction")
    }

    /// Non-directional CTOT pair: R1 with XR=GA XG=CT (record_strand=CTOT,
    /// pair_strand=CTOT), R2 with XR=CT XG=CT (record_strand=OT). Used by
    /// `extract_pe_routes_ctot_pair_strand_correctly` to exercise the
    /// non-directional library path that's NOT covered by `ot_pair`.
    pub fn ctot_pair(
        r1_xm: &[u8],
        r1_start: usize,
        r2_xm: &[u8],
        r2_start: usize,
        qname: &[u8],
    ) -> BismarkPair {
        let r1 = synth(
            b"GA",
            b"CT",
            r1_xm,
            &vec![b'A'; r1_xm.len()],
            r1_start,
            &[(Kind::Match, r1_xm.len())],
            0x41,
            qname,
            0,
        );
        let r2 = synth(
            b"CT",
            b"CT",
            r2_xm,
            &vec![b'A'; r2_xm.len()],
            r2_start,
            &[(Kind::Match, r2_xm.len())],
            0x81,
            qname,
            0,
        );
        BismarkPair::from_mates(r1, r2).expect("CTOT pair construction")
    }

    /// Non-directional CTOB pair: R1 with XR=GA XG=GA (record_strand=CTOB,
    /// pair_strand=CTOB), R2 with XR=CT XG=GA (record_strand=OB).
    pub fn ctob_pair(
        r1_xm: &[u8],
        r1_start: usize,
        r2_xm: &[u8],
        r2_start: usize,
        qname: &[u8],
    ) -> BismarkPair {
        let r1 = synth(
            b"GA",
            b"GA",
            r1_xm,
            &vec![b'A'; r1_xm.len()],
            r1_start,
            &[(Kind::Match, r1_xm.len())],
            0x41,
            qname,
            0,
        );
        let r2 = synth(
            b"CT",
            b"GA",
            r2_xm,
            &vec![b'A'; r2_xm.len()],
            r2_start,
            &[(Kind::Match, r2_xm.len())],
            0x81,
            qname,
            0,
        );
        BismarkPair::from_mates(r1, r2).expect("CTOB pair construction")
    }

    /// Same as `ot_pair` but with arbitrary CIGAR ops on R1 (e.g. for InDel
    /// fixtures).
    pub fn ot_pair_with_r1_cigar(
        r1_xm: &[u8],
        r1_start: usize,
        r1_cigar: &[(Kind, usize)],
        r2_xm: &[u8],
        r2_start: usize,
        qname: &[u8],
    ) -> BismarkPair {
        let r1 = synth(
            b"CT",
            b"CT",
            r1_xm,
            &vec![b'A'; r1_xm.len()],
            r1_start,
            r1_cigar,
            0x41,
            qname,
            0,
        );
        let r2 = synth(
            b"GA",
            b"CT",
            r2_xm,
            &vec![b'A'; r2_xm.len()],
            r2_start,
            &[(Kind::Match, r2_xm.len())],
            0x81,
            qname,
            0,
        );
        BismarkPair::from_mates(r1, r2).expect("OT pair with R1 CIGAR construction")
    }

    /// Build a `Vec<MethCall>` from a list of (ref_pos, context, methylated)
    /// tuples. read_pos defaults to 0; xm_byte derived from context+methylated.
    pub fn calls_at(positions: &[(u32, super::CytosineContext, bool)]) -> Vec<super::MethCall> {
        positions
            .iter()
            .map(|&(ref_pos, context, methylated)| {
                let xm_byte = match (context, methylated) {
                    (super::CytosineContext::CpG, true) => b'Z',
                    (super::CytosineContext::CpG, false) => b'z',
                    (super::CytosineContext::CHG, true) => b'X',
                    (super::CytosineContext::CHG, false) => b'x',
                    (super::CytosineContext::CHH, true) => b'H',
                    (super::CytosineContext::CHH, false) => b'h',
                };
                super::MethCall {
                    ref_pos,
                    read_pos: 0,
                    context,
                    methylated,
                    xm_byte,
                }
            })
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 1. is_forward_pair_strand classification
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn is_forward_pair_strand_matches_perl_classification() {
    // Per Perl 2400/2415: OT and CTOB are forward (R1 upstream); OB and
    // CTOT are reverse (R2 upstream).
    assert!(is_forward_pair_strand(BismarkStrand::OT));
    assert!(is_forward_pair_strand(BismarkStrand::CTOB));
    assert!(!is_forward_pair_strand(BismarkStrand::OB));
    assert!(!is_forward_pair_strand(BismarkStrand::CTOT));
}

// ─────────────────────────────────────────────────────────────────────────
// 2. drop_overlap endpoint semantics (SPEC §7.4 verification)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn drop_overlap_forward_pair_drops_r2_at_or_before_r1_end() {
    // C.1 polarity fix (#862): OT pair, R1 50M at chrX:100 → r1_ref_end == 149.
    // R2 calls at 148, 149, 150. Perl drop predicate (post-transformation, line
    // 3826): `r2_pos <= 149` (inclusive) → drop 148, 149. Rust keep predicate
    // (strict inverse): `r2_pos > 149` → keep 150 (R2's unique downstream region).
    let pair = helpers::ot_pair(
        b"..........".repeat(5).as_slice(),
        100,
        b".....",
        130,
        b"q1",
    );
    let r2_calls = helpers::calls_at(&[
        (148, CytosineContext::CpG, true),
        (149, CytosineContext::CpG, true),
        (150, CytosineContext::CpG, true),
    ]);
    let kept = drop_overlap(r2_calls, &pair).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].ref_pos, 150);
}

#[test]
fn drop_overlap_reverse_pair_drops_r2_at_or_after_r1_start() {
    // C.1 polarity fix (#862): OB pair, R1 50M at chrX:200 → r1_ref_start == 200.
    // R2 calls at 199, 200, 201. Perl drop predicate (post-transformation, line
    // 3745): `r2_pos >= 200` (inclusive) → drop 200, 201. Rust keep predicate
    // (strict inverse): `r2_pos < 200` → keep 199 (R2's unique upstream region).
    let pair = helpers::ob_pair(
        b"..........".repeat(5).as_slice(),
        200,
        b".....",
        150,
        b"q2",
    );
    let r2_calls = helpers::calls_at(&[
        (199, CytosineContext::CpG, true),
        (200, CytosineContext::CpG, true),
        (201, CytosineContext::CpG, true),
    ]);
    let kept = drop_overlap(r2_calls, &pair).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].ref_pos, 199);
}

#[test]
fn drop_overlap_disjoint_forward_pair_keeps_all_r2_calls() {
    // C.1 polarity fix (#862): Forward (OT) pair, R1 50M at 100 → r1_ref_end
    // == 149. R2 starts at 300 (insert > 2×read_length, biologically
    // disjoint). All R2 calls are at ref_pos >= 300 → all > 149 (r1_ref_end).
    // Strict-`>` keep predicate KEEPS them ALL.
    //
    // **Rev 2 SPEC §7.4 incorrectly claimed all R2 dropped here** — that was
    // the bug. Per Perl source (line 3826), the drop predicate is
    // `r2_pos <= r1_ref_end`; for an R2 wholly downstream of R1, no R2 pos
    // satisfies the drop predicate so Perl `return` never fires and all R2
    // calls are emitted. C.1 corrects this; matches Perl real-data output
    // (read `.9` of the 10M PE BAM is exactly this geometry).
    let pair = helpers::ot_pair(
        b"..........".repeat(5).as_slice(),
        100,
        b".....",
        300,
        b"q3",
    );
    let r2_calls = helpers::calls_at(&[
        (300, CytosineContext::CpG, true),
        (310, CytosineContext::CpG, true),
        (340, CytosineContext::CpG, true),
    ]);
    let kept = drop_overlap(r2_calls, &pair).unwrap();
    assert_eq!(
        kept.len(),
        3,
        "all R2 calls kept (all > r1_ref_end; no overlap to dedup)"
    );
    assert_eq!(kept[0].ref_pos, 300);
    assert_eq!(kept[1].ref_pos, 310);
    assert_eq!(kept[2].ref_pos, 340);
}

#[test]
fn drop_overlap_fully_overlapping_pair_drops_all_r2_calls() {
    // C.1 polarity fix (#862): Forward (OT) pair, R1 50M at 100 → r1_ref_end
    // == 149. R2 also at 100 (innie with small insert) → R2 calls at 105,
    // 120, 148 are all <= 149 (in R1's span). Strict-`>` keep predicate DROPS
    // them all. R2 has no unique region in this fully-overlapping geometry.
    //
    // This is the correct biological interpretation of `--no_overlap`:
    // "avoid scoring overlapping methylation calls twice" — for fully
    // overlapping pairs, ALL R2 calls are redundant with R1.
    let pair = helpers::ot_pair(
        b"..........".repeat(5).as_slice(),
        100,
        b".....",
        100,
        b"q4",
    );
    let r2_calls = helpers::calls_at(&[
        (105, CytosineContext::CpG, true),
        (120, CytosineContext::CHG, true),
        (148, CytosineContext::CHH, true),
    ]);
    let kept = drop_overlap(r2_calls, &pair).unwrap();
    assert_eq!(
        kept.len(),
        0,
        "all R2 calls dropped (all <= r1_ref_end, in overlap region)"
    );
}

#[test]
fn drop_overlap_with_r1_indel_uses_reference_end() {
    // C.1 polarity fix (#862): R1 `50M2D50M` at 100 → reference_span = 102,
    // reference_end = 201. R2 calls at 200, 201, 202. Strict-`>` keep
    // predicate → drop 200, 201 (≤ 201); keep 202 (> 201, R2's unique region).
    use noodles_sam::alignment::record::cigar::op::Kind;
    let pair = helpers::ot_pair_with_r1_cigar(
        b"..........".repeat(10).as_slice(),
        100,
        &[(Kind::Match, 50), (Kind::Deletion, 2), (Kind::Match, 50)],
        b".....",
        150,
        b"q_indel",
    );
    let r2_calls = helpers::calls_at(&[
        (200, CytosineContext::CpG, true),
        (201, CytosineContext::CpG, true),
        (202, CytosineContext::CpG, true),
    ]);
    let kept = drop_overlap(r2_calls, &pair).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].ref_pos, 202);
}

#[test]
fn drop_overlap_with_r1_end_deletion() {
    // C.1 polarity fix (#862): R1 `49M2D1M` at 100. CIGAR consumes 50 read
    // bases + 52 reference positions, so reference_end == 151. R2 calls at
    // 150, 151, 152. Strict-`>` keep → drop 150, 151 (≤ 151); keep 152.
    use noodles_sam::alignment::record::cigar::op::Kind;
    let pair = helpers::ot_pair_with_r1_cigar(
        b"..........".repeat(5).as_slice(),
        100,
        &[(Kind::Match, 49), (Kind::Deletion, 2), (Kind::Match, 1)],
        b".....",
        130,
        b"q_enddel",
    );
    let r2_calls = helpers::calls_at(&[
        (150, CytosineContext::CpG, true),
        (151, CytosineContext::CpG, true),
        (152, CytosineContext::CpG, true),
    ]);
    let kept = drop_overlap(r2_calls, &pair).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].ref_pos, 152);
}

#[test]
fn drop_overlap_with_r1_insertion_shifts_read_pos_only() {
    // R1 `50M2I50M` at 100. Insertion consumes read (2 bases) but not
    // reference → R1 reads 102 bases, reference span = 100, reference_end
    // = 199. XM length must equal SEQ length = 102 (sum of read-consuming
    // CIGAR ops). R2 calls at 198, 199, 200 — C.1 polarity fix (#862)
    // keeps strict `>` 199 → drop 198, 199 (≤ 199); keep 200.
    use noodles_sam::alignment::record::cigar::op::Kind;
    let r1_xm = vec![b'.'; 102]; // 50M + 2I + 50M consumes 102 read bases
    let pair = helpers::ot_pair_with_r1_cigar(
        &r1_xm,
        100,
        &[(Kind::Match, 50), (Kind::Insertion, 2), (Kind::Match, 50)],
        b".....",
        150,
        b"q_ins",
    );
    let r2_calls = helpers::calls_at(&[
        (198, CytosineContext::CpG, true),
        (199, CytosineContext::CpG, true),
        (200, CytosineContext::CpG, true),
    ]);
    // C.1 polarity fix (#862): R1 `50M2I50M` at 100 → reference_end = 199.
    // Strict-`>` keep → drop 198, 199 (≤ 199); keep 200 (R2's unique region).
    let kept = drop_overlap(r2_calls, &pair).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].ref_pos, 200);
}

// ─────────────────────────────────────────────────────────────────────────
// 2b. C.1 regression-guard fixtures (added with #862 polarity fix)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn drop_overlap_real_data_fr_pair_with_gap_keeps_all_r2_calls() {
    // C.1 regression guard (#862) — mirrors read `.9` of the 10M PE BAM that
    // surfaced the polarity bug. R1 64M at 100 → r1_ref_end = 163. R2 65M at
    // 171 → R2 spans [171, 235], with a 7bp gap between R1's end and R2's
    // start. Non-overlapping geometry: all R2 calls must be kept (R2 has no
    // overlap to dedup against).
    //
    // Pre-C.1: Rust dropped all R2 calls because predicate was `r2_pos < 163`.
    // Post-C.1: Rust keeps all R2 calls because predicate is `r2_pos > 163`.
    let r1_xm = vec![b'.'; 64];
    let r2_xm = vec![b'.'; 65];
    let pair = helpers::ot_pair(&r1_xm, 100, &r2_xm, 171, b"q_realdata9");
    let r2_calls = helpers::calls_at(&[
        (175, CytosineContext::CpG, true),
        (200, CytosineContext::CpG, false),
        (220, CytosineContext::CHG, true),
        (235, CytosineContext::CHH, false),
    ]);
    let kept = drop_overlap(r2_calls, &pair).unwrap();
    assert_eq!(
        kept.len(),
        4,
        "all R2 unique-region calls kept (gap of 7bp between r1_ref_end=163 and r2_start=171)"
    );
    assert_eq!(kept[0].ref_pos, 175);
    assert_eq!(kept[1].ref_pos, 200);
    assert_eq!(kept[2].ref_pos, 220);
    assert_eq!(kept[3].ref_pos, 235);
}

#[test]
fn drop_overlap_partial_overlap_reverse_pair() {
    // C.1 (#862): OB partial overlap fixture. R2=[150, 213] (64M) is
    // upstream; R1=[200, 263] (64M) is downstream; overlap = [200, 213].
    // r1_ref_start = 200. R2 calls at 195, 199, 200, 201.
    // Strict-`<` keep predicate: keep r2_pos < 200 → keep [195, 199];
    // drop [200, 201] (both ≥ r1_ref_start, in the overlap region).
    let r1_xm = vec![b'.'; 64];
    let r2_xm = vec![b'.'; 64];
    let pair = helpers::ob_pair(&r1_xm, 200, &r2_xm, 150, b"q_ob_partial");
    let r2_calls = helpers::calls_at(&[
        (195, CytosineContext::CpG, true),
        (199, CytosineContext::CpG, true),
        (200, CytosineContext::CpG, true),
        (201, CytosineContext::CpG, true),
    ]);
    let kept = drop_overlap(r2_calls, &pair).unwrap();
    assert_eq!(kept.len(), 2);
    assert_eq!(kept[0].ref_pos, 195, "R2 unique upstream call kept");
    assert_eq!(
        kept[1].ref_pos, 199,
        "R2 unique upstream call kept (boundary -1)"
    );
    assert!(
        !kept.iter().any(|c| c.ref_pos == 200),
        "boundary call at r1_ref_start dropped"
    );
    assert!(
        !kept.iter().any(|c| c.ref_pos == 201),
        "R2 overlap-region call dropped"
    );
}

#[test]
fn drop_overlap_r1_with_n_skip_op() {
    // C.1 (#862): R1 with `N` skip CIGAR (spliced bisulfite-RNA-seq).
    // R1 CIGAR `50M1000N50M` at 100 — read consumes 100 bases; reference
    // span = 50 + 1000 + 50 = 1100; r1_ref_end = 100 + 1100 - 1 = 1199.
    // Confirms `N` op is counted in reference_span (matches Perl's $MDN_count).
    // R2 calls at 1198, 1199, 1200 → strict-`>` keep predicate → keep 1200.
    use noodles_sam::alignment::record::cigar::op::Kind;
    let r1_xm = vec![b'.'; 100]; // 50M + 50M consumed read = 100; N does not consume read
    let pair = helpers::ot_pair_with_r1_cigar(
        &r1_xm,
        100,
        &[(Kind::Match, 50), (Kind::Skip, 1000), (Kind::Match, 50)],
        b".....",
        1300, // R2 far past R1; we only care about R2 calls placed manually
        b"q_n_skip",
    );
    let r2_calls = helpers::calls_at(&[
        (1198, CytosineContext::CpG, true),
        (1199, CytosineContext::CpG, true),
        (1200, CytosineContext::CpG, true),
    ]);
    let kept = drop_overlap(r2_calls, &pair).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(
        kept[0].ref_pos, 1200,
        "only call strictly past r1_ref_end=1199 kept"
    );
}

#[test]
fn drop_overlap_r1_with_5prime_soft_clip() {
    // C.1 (#862, defensive guard per Reviewer A I4 + Reviewer B I2):
    // R1 CIGAR `10S100M` at 100 — soft-clip excluded from reference span.
    // alignment_start = 100 (BAM POS, the leftmost MAPPED base);
    // reference_span = 100; r1_ref_end = 100 + 100 - 1 = 199.
    // R2 calls at 198, 199, 200 → strict-`>` keep → keep 200 only.
    use noodles_sam::alignment::record::cigar::op::Kind;
    let r1_xm = vec![b'.'; 110]; // 10S + 100M consumed read = 110
    let pair = helpers::ot_pair_with_r1_cigar(
        &r1_xm,
        100,
        &[(Kind::SoftClip, 10), (Kind::Match, 100)],
        b".....",
        150,
        b"q_5p_softclip",
    );
    let r2_calls = helpers::calls_at(&[
        (198, CytosineContext::CpG, true),
        (199, CytosineContext::CpG, true),
        (200, CytosineContext::CpG, true),
    ]);
    let kept = drop_overlap(r2_calls, &pair).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(
        kept[0].ref_pos, 200,
        "soft-clip excluded from reference_span; only call > 199 kept"
    );
}

#[test]
fn drop_overlap_r1_with_3prime_soft_clip() {
    // C.1 (#862, defensive guard): R1 CIGAR `100M10S` at 100 — 3'-soft-clip
    // excluded from reference span. r1_ref_end = 100 + 100 - 1 = 199.
    // R2 calls at 198, 199, 200 → strict-`>` keep → keep 200 only.
    // Symmetric to the 5'-soft-clip test.
    use noodles_sam::alignment::record::cigar::op::Kind;
    let r1_xm = vec![b'.'; 110];
    let pair = helpers::ot_pair_with_r1_cigar(
        &r1_xm,
        100,
        &[(Kind::Match, 100), (Kind::SoftClip, 10)],
        b".....",
        150,
        b"q_3p_softclip",
    );
    let r2_calls = helpers::calls_at(&[
        (198, CytosineContext::CpG, true),
        (199, CytosineContext::CpG, true),
        (200, CytosineContext::CpG, true),
    ]);
    let kept = drop_overlap(r2_calls, &pair).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(
        kept[0].ref_pos, 200,
        "3' soft-clip excluded from reference_span; only call > 199 kept"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// 3. AutoDetect `no_overlap` regression (Phase A bug-fix)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn validate_auto_detect_keeps_no_overlap_default() {
    // Rev 1 (Reviewer A §1.1 Critical): without the Phase C cli.rs fix,
    // AutoDetect would resolve `no_overlap == false`, silently leaking R2
    // overlap calls when dispatching to PE. The fix sets `no_overlap = true`
    // for any non-SE paired_mode.
    let tmp = tempfile::Builder::new().suffix(".bam").tempfile().unwrap();
    std::fs::write(tmp.path(), b"x").unwrap();
    let cli = Cli::try_parse_from([
        "bismark-methylation-extractor-rs",
        tmp.path().to_str().unwrap(),
    ])
    .unwrap();
    let config = cli.validate().unwrap();
    assert_eq!(config.paired_mode, PairedMode::AutoDetect);
    assert!(
        config.no_overlap,
        "AutoDetect must inherit PE default no_overlap=true (rev 1 Critical fix)"
    );
}

#[test]
fn validate_paired_end_keeps_no_overlap_default() {
    let tmp = tempfile::Builder::new().suffix(".bam").tempfile().unwrap();
    std::fs::write(tmp.path(), b"x").unwrap();
    let cli = Cli::try_parse_from([
        "bismark-methylation-extractor-rs",
        "--paired-end",
        tmp.path().to_str().unwrap(),
    ])
    .unwrap();
    let config = cli.validate().unwrap();
    assert!(config.no_overlap);
}

#[test]
fn validate_paired_end_with_include_overlap_disables_no_overlap() {
    let tmp = tempfile::Builder::new().suffix(".bam").tempfile().unwrap();
    std::fs::write(tmp.path(), b"x").unwrap();
    let cli = Cli::try_parse_from([
        "bismark-methylation-extractor-rs",
        "--paired-end",
        "--include_overlap",
        tmp.path().to_str().unwrap(),
    ])
    .unwrap();
    let config = cli.validate().unwrap();
    assert!(!config.no_overlap);
}

// ─────────────────────────────────────────────────────────────────────────
// 4. BismarkPair construction error propagation
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn bismark_pair_from_mates_rejects_mismatched_qnames() {
    // Smoke check that bismark-io's qname-eq validation propagates as
    // expected; extractor's PE loop relies on it.
    use bismark_io::BismarkIoError;
    use noodles_sam::alignment::record::cigar::op::Kind;
    let r1 = helpers::synth(
        b"CT",
        b"CT",
        b".....",
        b"AAAAA",
        100,
        &[(Kind::Match, 5)],
        0x41,
        b"qname_a",
        0,
    );
    let r2 = helpers::synth(
        b"GA",
        b"CT",
        b".....",
        b"AAAAA",
        100,
        &[(Kind::Match, 5)],
        0x81,
        b"qname_b",
        0,
    );
    let err = BismarkPair::from_mates(r1, r2).unwrap_err();
    assert!(
        matches!(err, BismarkIoError::MateMismatch { .. }),
        "expected MateMismatch, got {:?}",
        err
    );
}

// ─────────────────────────────────────────────────────────────────────────
// 5. End-to-end PE behaviour via the binary (synthetic BAM in-test)
// ─────────────────────────────────────────────────────────────────────────
//
// These exercise the binary; they live here (not in pe_phase_c_smoke.rs)
// because they assert specific routing/file-content properties that are
// natural unit-level checks rather than smoke gates. The bulk smoke
// (12 files exist, exit 0) lives in the smoke file.

mod pe_e2e {
    use super::helpers;
    use std::fs;

    use assert_cmd::Command;
    use bismark_io::{BamWriter, BismarkRecord};
    use bstr::BString;
    use noodles_sam::Header;
    use noodles_sam::header::record::value::Map;
    use noodles_sam::header::record::value::map::ReferenceSequence;
    use std::num::NonZeroUsize;
    use std::path::PathBuf;

    fn header_with_chr1() -> Header {
        let mut header = Header::default();
        header.reference_sequences_mut().insert(
            BString::from(b"chr1".to_vec()),
            Map::<ReferenceSequence>::new(NonZeroUsize::new(10_000).unwrap()),
        );
        header
    }

    /// Write a PE BAM with the given (r1, r2) record sequence to `path`.
    fn write_pe_bam(path: &std::path::Path, records: Vec<BismarkRecord>) {
        let header = header_with_chr1();
        let mut writer = BamWriter::from_path(path, header).unwrap();
        for r in &records {
            writer.write_record(r).unwrap();
        }
        writer.finish().unwrap();
    }

    fn run_binary(bam: &std::path::Path, outdir: &std::path::Path, extra_args: &[&str]) {
        let mut cmd = Command::cargo_bin("bismark-methylation-extractor-rs").unwrap();
        cmd.arg(bam)
            .arg("--paired-end")
            .arg("--output_dir")
            .arg(outdir);
        for arg in extra_args {
            cmd.arg(arg);
        }
        cmd.assert().success();
    }

    /// Two OT pairs.
    #[test]
    fn extract_pe_handles_two_well_formed_pairs() {
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("two_pairs.bam");
        // Build two OT pairs (R1=OT, R2=CTOT).
        let pair1 = helpers::ot_pair(b"Z....", 100, b"....z", 130, b"pair1");
        let pair2 = helpers::ot_pair(b"..X..", 200, b"..x..", 230, b"pair2");
        write_pe_bam(
            &bam_path,
            vec![
                pair1.r1().clone(),
                pair1.r2().clone(),
                pair2.r1().clone(),
                pair2.r2().clone(),
            ],
        );
        let outdir = work.path().join("out");
        run_binary(&bam_path, &outdir, &[]);
        // Phase C.2 (#864): report counts PAIRS for PE (matches Perl
        // `sequences_count`, line 2482). 2 pairs → "Processed 2 lines"
        // (rev 0 of Phase B incorrectly counted 2×pairs; C.2 fixed both
        // increment sites in pipeline.rs + parallel.rs).
        let report = fs::read_to_string(outdir.join("two_pairs_splitting_report.txt")).unwrap();
        assert!(
            report.contains("Processed 2 lines in total"),
            "report should reflect pair-count for PE; got:\n{report}"
        );
        // The 2×pairs count lives in the new line 2483 counter.
        assert!(
            report.contains("Total number of methylation call strings processed: 4"),
            "call_strings_processed = 2×pairs for PE; got:\n{report}"
        );
    }

    /// Plan §7.1 row "pe_splitting_report_counts_lines_not_pairs" — name
    /// preserved for traceability, but **Phase C.2 (#864) inverted the
    /// semantic**: Perl `sequences_count` (line 2459, drives report line
    /// 2482) counts PAIRS for PE, not lines. The line-count goes into the
    /// new `Total number of methylation call strings processed` counter
    /// (Perl line 2483).
    #[test]
    fn pe_splitting_report_counts_pairs_in_main_line_post_c2() {
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("ten_pairs.bam");
        let mut records = Vec::with_capacity(20);
        for i in 0..10 {
            let pair = helpers::ot_pair(
                b"Z....",
                100 + i * 50,
                b"....z",
                130 + i * 50,
                format!("pair_{i}").as_bytes(),
            );
            records.push(pair.r1().clone());
            records.push(pair.r2().clone());
        }
        write_pe_bam(&bam_path, records);
        let outdir = work.path().join("out");
        run_binary(&bam_path, &outdir, &[]);
        let report = fs::read_to_string(outdir.join("ten_pairs_splitting_report.txt")).unwrap();
        assert!(
            report.contains("Processed 10 lines in total"),
            "10 pairs → 10 sequences_count; got:\n{report}"
        );
        assert!(
            report.contains("Total number of methylation call strings processed: 20"),
            "10 pairs × 2 = 20 call strings; got:\n{report}"
        );
    }

    /// Closes the Alan-Hoyle structural bug at PE unit level — R2 calls
    /// route to the pair-strand file (CpG_OT), not R2's record-strand file
    /// (CpG_CTOT).
    ///
    /// Uses `--include_overlap` to disable `drop_overlap`, so we test
    /// routing in isolation without the overlap-detection mechanic
    /// interfering with R2's calls.
    #[test]
    fn extract_pe_routes_r2_calls_to_pair_strand_file_not_record_strand_file() {
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("alan.bam");
        // OT pair: R1 record_strand = OT; R2 record_strand = CTOT.
        // Both R1's Z call and R2's Z call MUST land in CpG_OT_alan.txt,
        // NOT in CpG_CTOT_alan.txt.
        let pair = helpers::ot_pair(b"Z....", 100, b"Z....", 200, b"alan_pair");
        write_pe_bam(&bam_path, vec![pair.r1().clone(), pair.r2().clone()]);
        let outdir = work.path().join("out");
        run_binary(&bam_path, &outdir, &["--include_overlap"]);

        let cpg_ot = fs::read_to_string(outdir.join("CpG_OT_alan.txt")).unwrap();
        // Two call lines in CpG_OT (one from R1, one from R2's call routed
        // to pair-strand).
        let ot_call_lines = cpg_ot.lines().count() - 1; // minus header line
        // Phase C.2 (#865): empty CpG_CTOT file is swept at flush time
        // (matches Perl). Verify absence instead of reading content.
        assert!(
            !outdir.join("CpG_CTOT_alan.txt").exists(),
            "CpG_CTOT_alan.txt should be swept (empty — no calls routed there)"
        );
        let ctot_call_lines: usize = 0;
        assert_eq!(ot_call_lines, 2, "both calls in CpG_OT (pair-strand)");
        assert_eq!(
            ctot_call_lines, 0,
            "no calls in CpG_CTOT (R2's record_strand)"
        );
    }

    /// `--include_overlap` keeps R2 calls in the overlap region.
    /// Rev 1 fixture spec (Reviewer B V1): R1 5M at 100 (refs 100-104),
    /// R2 5M at 102 (refs 102-106), overlap = 102-104. R2 has methylation
    /// calls at ref-pos 103 (in overlap) and 105/106 (outside overlap).
    /// Under --include_overlap all three calls present; under default
    /// --no_overlap only 105 and 106 present.
    #[test]
    fn extract_pe_with_include_overlap_keeps_r2_overlap_calls() {
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("overlap_keep.bam");
        // R1 XM: `Z....` (Z at ref 100). R2 XM: `.Z.zZ` aligned at 102 →
        // XM[1]='Z' → ref_pos 103 (IN overlap); XM[3]='z' → ref_pos 105
        // (OUTSIDE overlap); XM[4]='Z' → ref_pos 106 (OUTSIDE overlap).
        let pair = helpers::ot_pair(b"Z....", 100, b".Z.zZ", 102, b"ovl");
        write_pe_bam(&bam_path, vec![pair.r1().clone(), pair.r2().clone()]);
        let outdir = work.path().join("out");
        run_binary(&bam_path, &outdir, &["--include_overlap"]);

        let cpg_ot = fs::read_to_string(outdir.join("CpG_OT_overlap_keep.txt")).unwrap();
        // R1 call at 100 + R2 calls at 103, 105, 106 = 4 lines beyond header.
        let call_lines = cpg_ot.lines().count() - 1;
        assert_eq!(call_lines, 4, "with --include_overlap all 4 calls present");
        assert!(
            cpg_ot.contains("\t103\t"),
            "R2 overlap-region call (ref 103) kept"
        );
        assert!(cpg_ot.contains("\t105\t"));
        assert!(cpg_ot.contains("\t106\t"));
    }

    #[test]
    fn extract_pe_with_no_overlap_drops_r2_overlap_keeps_unique() {
        // C.1 polarity fix (#862): renamed and re-asserted. The correct
        // Perl semantic (line 3826 drop predicate, post-coordinate-mutation:
        // `r2_pos <= r1_ref_end`) drops R2 calls IN the overlap region and
        // keeps R2 calls past R1's end (R2's unique downstream region).
        // Matches the documented `--no_overlap` intent: *"only methylation
        // calls of read 1 are kept for overlapping regions"*.
        //
        // R1 `Z....` 5M at 100 → r1_ref_end = 104. R2 `.Z.zZ` 5M at 102
        // (R2 is `-`-strand CTOT — iter_aligned reverses, so 5'-oriented
        // calls have ref_pos values 106, 105, 103). Strict-`>` keep:
        // - 106 > 104 → keep
        // - 105 > 104 → keep
        // - 103 <= 104 → drop (in overlap region with R1)
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("overlap_drop.bam");
        let pair = helpers::ot_pair(b"Z....", 100, b".Z.zZ", 102, b"ovl2");
        write_pe_bam(&bam_path, vec![pair.r1().clone(), pair.r2().clone()]);
        let outdir = work.path().join("out");
        run_binary(&bam_path, &outdir, &[]); // default --no_overlap

        let cpg_ot = fs::read_to_string(outdir.join("CpG_OT_overlap_drop.txt")).unwrap();
        assert!(cpg_ot.contains("\t100\t"), "R1 call kept");
        assert!(
            !cpg_ot.contains("\t103\t"),
            "R2 call in overlap (103 <= r1_ref_end=104) dropped"
        );
        assert!(
            cpg_ot.contains("\t105\t"),
            "R2 call past r1_ref_end (105 > 104) kept — R2 unique region"
        );
        assert!(
            cpg_ot.contains("\t106\t"),
            "R2 call past r1_ref_end (106 > 104) kept — R2 unique region"
        );
    }

    /// PE pair on different chromosomes → MateChromosomeMismatch.
    #[test]
    fn extract_pe_rejects_cross_chromosome_pair() {
        use noodles_sam::alignment::record::cigar::op::Kind;
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("crosschr.bam");
        // Build a 2-chr header. Need to do this manually since helpers
        // only build single-chr header.
        let mut header = Header::default();
        header.reference_sequences_mut().insert(
            BString::from(b"chr1".to_vec()),
            Map::<ReferenceSequence>::new(NonZeroUsize::new(10_000).unwrap()),
        );
        header.reference_sequences_mut().insert(
            BString::from(b"chr2".to_vec()),
            Map::<ReferenceSequence>::new(NonZeroUsize::new(10_000).unwrap()),
        );
        let mut writer = BamWriter::from_path(&bam_path, header).unwrap();
        let r1 = helpers::synth(
            b"CT",
            b"CT",
            b"Z....",
            b"AAAAA",
            100,
            &[(Kind::Match, 5)],
            0x41,
            b"cross",
            0, // chr1
        );
        let r2 = helpers::synth(
            b"GA",
            b"CT",
            b".Z...",
            b"AAAAA",
            200,
            &[(Kind::Match, 5)],
            0x81,
            b"cross",
            1, // chr2
        );
        writer.write_record(&r1).unwrap();
        writer.write_record(&r2).unwrap();
        writer.finish().unwrap();

        let outdir = work.path().join("out");
        let mut cmd = Command::cargo_bin("bismark-methylation-extractor-rs").unwrap();
        cmd.arg(&bam_path)
            .arg("--paired-end")
            .arg("--output_dir")
            .arg(&outdir)
            .assert()
            .failure()
            .stderr(predicates::str::contains("different chromosomes"));

        // Cleanup must have removed all partial files.
        if outdir.exists() {
            let count = fs::read_dir(&outdir).unwrap().count();
            assert_eq!(count, 0, "cleanup_partial_outputs left {count} stragglers");
        }
    }

    /// Odd-numbered record count → UnpairedFinalRecord.
    #[test]
    fn extract_pe_rejects_unpaired_final_record() {
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("odd.bam");
        let pair = helpers::ot_pair(b"Z....", 100, b"....z", 130, b"first");
        let orphan_r1 = helpers::synth(
            b"CT",
            b"CT",
            b"..Z..",
            b"AAAAA",
            200,
            &[(noodles_sam::alignment::record::cigar::op::Kind::Match, 5)],
            0x41,
            b"orphan",
            0,
        );
        write_pe_bam(
            &bam_path,
            vec![pair.r1().clone(), pair.r2().clone(), orphan_r1],
        );
        let outdir = work.path().join("out");
        let mut cmd = Command::cargo_bin("bismark-methylation-extractor-rs").unwrap();
        cmd.arg(&bam_path)
            .arg("--paired-end")
            .arg("--output_dir")
            .arg(&outdir)
            .assert()
            .failure()
            .stderr(predicates::str::contains("unpaired final record"));

        // Rev 2 backfill (Reviewer B Err3): assert cleanup ran. Sister test
        // `extract_pe_rejects_cross_chromosome_pair` had this; UnpairedFinalRecord
        // should too.
        if outdir.exists() {
            let count = fs::read_dir(&outdir).unwrap().count();
            assert_eq!(
                count, 0,
                "cleanup_partial_outputs should have removed all 12 files; found {count} stragglers"
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────
    // Rev 2 backfill: 7 plan §7.1 tests that were absent in rev 1.
    // Closes plan-manager COVERAGE_PHASE_C "MISSING" rows + Reviewer B S1.
    // ─────────────────────────────────────────────────────────────────

    /// Non-directional library CTOT pair (R1 record_strand=CTOT, pair_strand=CTOT).
    /// Reverse class per `is_forward_pair_strand` (R2 upstream, R1 downstream).
    /// All calls (R1 + R2) must route to `*_CTOT_*.txt`, never `*_OT_*.txt`
    /// (R2's per-record strand is OT for a non-directional CTOT pair).
    /// Closes the Alan-Hoyle split-across-files bug at the non-directional
    /// library level. Plan §7.1 / rev 1 Reviewer A §4.2.
    #[test]
    fn extract_pe_routes_ctot_pair_strand_correctly() {
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("ctot.bam");
        // CTOT is reverse class — R2 is upstream. R2 5M at 200 spans
        // [200, 204]; R1 5M at 230 → r1_ref_start = 230. Post-C.1 keep
        // predicate `r2_pos < r1_ref_start` keeps all R2 calls (R2 is
        // entirely in its upstream-unique region, no overlap with R1).
        // Runs with default `--no_overlap` — this test is load-bearing on
        // the corrected CTOT polarity (was using `--include_overlap` pre-C.1
        // with a comment that described the OLD buggy polarity).
        let pair = helpers::ctot_pair(b"Z....", 230, b"....Z", 200, b"ctot_pair");
        write_pe_bam(&bam_path, vec![pair.r1().clone(), pair.r2().clone()]);
        let outdir = work.path().join("out");
        run_binary(&bam_path, &outdir, &[]);

        // Both calls land in CpG_CTOT, not in CpG_OT (R2's record_strand)
        // and not in CpG_CTOB (irrelevant strand).
        let cpg_ctot = fs::read_to_string(outdir.join("CpG_CTOT_ctot.txt")).unwrap();
        let ctot_call_lines = cpg_ctot.lines().count() - 1;
        // Phase C.2 (#865): empty CpG_OT file is swept (no calls routed
        // there for a CTOT pair). Verify absence instead of reading.
        assert!(
            !outdir.join("CpG_OT_ctot.txt").exists(),
            "CpG_OT_ctot.txt should be swept (empty — R2's record_strand is OT but pair-strand routing puts calls in CpG_CTOT)"
        );
        assert_eq!(
            ctot_call_lines, 2,
            "both R1 + R2 calls route to CpG_CTOT (pair-strand)"
        );
    }

    /// Non-directional library CTOB pair (R1 record_strand=CTOB, pair_strand=CTOB).
    /// Forward class per `is_forward_pair_strand` (R1 upstream, R2 downstream).
    /// All calls must route to `*_CTOB_*.txt`, never `*_OB_*.txt`.
    /// Plan §7.1 / rev 1 Reviewer A §4.2.
    #[test]
    fn extract_pe_routes_ctob_pair_strand_correctly() {
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("ctob.bam");
        // CTOB is forward class. R1 at 100, R2 at 200 (downstream).
        // Use --include_overlap to bypass drop_overlap so R2 calls aren't dropped.
        let pair = helpers::ctob_pair(b"Z....", 100, b"....Z", 200, b"ctob_pair");
        write_pe_bam(&bam_path, vec![pair.r1().clone(), pair.r2().clone()]);
        let outdir = work.path().join("out");
        run_binary(&bam_path, &outdir, &["--include_overlap"]);

        let cpg_ctob = fs::read_to_string(outdir.join("CpG_CTOB_ctob.txt")).unwrap();
        let ctob_call_lines = cpg_ctob.lines().count() - 1;
        // Phase C.2 (#865): empty CpG_OB file is swept (no calls routed
        // there for a CTOB pair).
        assert!(
            !outdir.join("CpG_OB_ctob.txt").exists(),
            "CpG_OB_ctob.txt should be swept (empty — pair-strand routing puts calls in CpG_CTOB)"
        );
        let ob_call_lines: usize = 0;
        assert_eq!(
            ctob_call_lines, 2,
            "both R1 + R2 calls route to CpG_CTOB (pair-strand)"
        );
        assert_eq!(
            ob_call_lines, 0,
            "no calls in CpG_OB (R2's record_strand is OB but pair-strand wins)"
        );
    }

    /// Per-mate ignore trim differentiation: `--ignore_r2 3` skips R2's
    /// first 3 read-positions but leaves R1's intact. Distinguishes
    /// `--ignore_r2` from `--ignore` (which would trim R1 too).
    /// Plan §7.1 / §10 row "Per-mate ignore-region trimming".
    #[test]
    fn extract_pe_per_mate_ignore_r2_only_skips_r2_positions() {
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("ignore_r2.bam");
        // OT pair. R1 and R2 each have a methylation call at read_pos 0.
        // After --ignore_r2 3, R1's pos-0 call survives; R2's pos-0 call
        // is skipped.
        //
        // For an OT pair with --include_overlap to disable drop_overlap so
        // we can observe R2's calls independently.
        //
        // R1 XM: "Z...." → R1 call at ref_pos 100 (read_pos 0).
        // R2 XM: "....Z" with r2_start=200; R2 is CTOT (`-` strand).
        // After iter_aligned reversal, BAM pos 4 (Z) → read_pos_5p=0,
        // ref_pos = 200 + 4 = 204. So R2's read_pos 0 call lands at ref 204.
        // --ignore_r2 3 skips read_pos_5p < 3, dropping this call.
        let pair = helpers::ot_pair(b"Z....", 100, b"....Z", 200, b"ignore_r2_pair");
        write_pe_bam(&bam_path, vec![pair.r1().clone(), pair.r2().clone()]);
        let outdir = work.path().join("out");
        run_binary(
            &bam_path,
            &outdir,
            &["--include_overlap", "--ignore_r2", "3"],
        );

        let cpg_ot = fs::read_to_string(outdir.join("CpG_OT_ignore_r2.txt")).unwrap();
        assert!(
            cpg_ot.contains("\t100\t"),
            "R1's read_pos 0 call (ref 100) survives --ignore_r2: {cpg_ot}"
        );
        assert!(
            !cpg_ot.contains("\t204\t"),
            "R2's read_pos_5p 0 call (ref 204) skipped by --ignore_r2 3: {cpg_ot}"
        );
    }

    /// `--ignore_3prime_r2` mirror of the above for the 3' end.
    /// Plan §7.1 / §10.
    #[test]
    fn extract_pe_per_mate_ignore_3prime_r2_only_skips_r2_3prime() {
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("ignore_3prime_r2.bam");
        // R1 XM: "....Z" → R1 call at read_pos 4 (the 3' end), ref_pos = 104.
        // R2 XM: "Z...." with r2_start=200; R2 is CTOT (`-` strand).
        // After iter_aligned reversal, R2's read_pos_5p=4 is the 3' end of the
        // sequenced read, which corresponds to BAM-pos 0 ('Z'), ref_pos=200.
        // --ignore_3prime_r2 3 skips read_pos_5p >= seq_len - 3 = 2, so
        // R2's 5'-oriented positions 2, 3, 4 are skipped → drops the Z at
        // read_pos_5p=4 (ref 200).
        let pair = helpers::ot_pair(b"....Z", 100, b"Z....", 200, b"i3p_r2_pair");
        write_pe_bam(&bam_path, vec![pair.r1().clone(), pair.r2().clone()]);
        let outdir = work.path().join("out");
        run_binary(
            &bam_path,
            &outdir,
            &["--include_overlap", "--ignore_3prime_r2", "3"],
        );

        let cpg_ot = fs::read_to_string(outdir.join("CpG_OT_ignore_3prime_r2.txt")).unwrap();
        assert!(
            cpg_ot.contains("\t104\t"),
            "R1's 3'-end call (ref 104) survives (R1's 3p trim is 0): {cpg_ot}"
        );
        assert!(
            !cpg_ot.contains("\t200\t"),
            "R2's 3'-end call (ref 200, on reverse strand) skipped by --ignore_3prime_r2 3: {cpg_ot}"
        );
    }

    /// `--ignore_r2` operates on **5'-end read cycles**, not reference
    /// positions. For a reverse-strand R2 (CTOT), the 5' read-cycle end
    /// corresponds to the HIGHEST reference position (not the lowest).
    /// This test specifically verifies the read-cycle vs ref-position
    /// distinction by checking which calls drop with --ignore_r2 3 vs without.
    ///
    /// Plan §7.1 / rev 1 Reviewer A §2.4.
    #[test]
    fn extract_pe_ignore_r2_skips_read_cycles_not_ref_positions() {
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("cycles.bam");
        // OT pair. R2 is CTOT (reverse). R2 XM "ZZZZZ" at r2_start=200.
        // After iter_aligned reversal:
        //   read_pos_5p=0 ↔ BAM-pos 4 ↔ ref 204
        //   read_pos_5p=1 ↔ BAM-pos 3 ↔ ref 203
        //   read_pos_5p=2 ↔ BAM-pos 2 ↔ ref 202
        //   read_pos_5p=3 ↔ BAM-pos 1 ↔ ref 201
        //   read_pos_5p=4 ↔ BAM-pos 0 ↔ ref 200
        //
        // --ignore_r2 3 → skip read_pos_5p < 3 → drop calls at ref 204, 203, 202.
        // Calls at ref 200 and 201 survive (5'-oriented read cycles 4 and 3).
        //
        // **The naive "drop first 3 reference positions" interpretation** would
        // drop ref 200, 201, 202 (the lowest 3). This test pins read-cycle
        // semantics: the HIGHEST ref positions drop (5' read-cycle end of a
        // reverse-strand R2 read), not the lowest.
        let pair = helpers::ot_pair(b".....", 100, b"ZZZZZ", 200, b"cycles_pair");
        write_pe_bam(&bam_path, vec![pair.r1().clone(), pair.r2().clone()]);
        let outdir = work.path().join("out");
        run_binary(
            &bam_path,
            &outdir,
            &["--include_overlap", "--ignore_r2", "3"],
        );

        let cpg_ot = fs::read_to_string(outdir.join("CpG_OT_cycles.txt")).unwrap();
        // Read-cycle interpretation: refs 200, 201 KEPT (cycles 4, 3); refs
        // 202, 203, 204 DROPPED (cycles 2, 1, 0).
        assert!(
            cpg_ot.contains("\t200\t"),
            "ref 200 (cycle 4, 3' end of read) kept: {cpg_ot}"
        );
        assert!(
            cpg_ot.contains("\t201\t"),
            "ref 201 (cycle 3) kept: {cpg_ot}"
        );
        assert!(
            !cpg_ot.contains("\t202\t"),
            "ref 202 (cycle 2, dropped by --ignore_r2 3): {cpg_ot}"
        );
        assert!(
            !cpg_ot.contains("\t203\t"),
            "ref 203 (cycle 1, dropped): {cpg_ot}"
        );
        assert!(
            !cpg_ot.contains("\t204\t"),
            "ref 204 (cycle 0, 5' end of read, dropped): {cpg_ot}"
        );
    }

    /// PE-level re-verification of the M-bias R2-index routing. Phase B's
    /// `route_call_r2_goes_to_mbias_index_1` already locks `route_call`'s
    /// behaviour at unit level; this asserts the PE pipeline correctly
    /// threads `ReadIdentity::R2` into `route_call` for R2 calls.
    ///
    /// Approach: drive a PE binary run, then count occurrences of R2's
    /// qname in CpG_OT (which would only contain R1's qname if the PE
    /// loop accidentally passed `ReadIdentity::R1` for R2's calls — there'd
    /// be no way to distinguish). The plan §7.1 row says "R2 calls
    /// increment state.mbias[1] not state.mbias[0]". Since we can't
    /// observe state.mbias from outside the binary (no M-bias writer until
    /// Phase D), the proxy assertion is that R2's distinctive qname appears
    /// in the split-file output — which is what `pair.r2()` would emit
    /// when route_call sees ReadIdentity::R2.
    ///
    /// Phase D will add a stronger end-to-end M-bias test once the writer
    /// lands; this is the achievable Phase C re-verification.
    #[test]
    fn extract_pe_increments_mbias_R2_at_index_1() {
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("mbias_r2.bam");
        let pair = helpers::ot_pair(b"Z....", 100, b"....Z", 200, b"mbias_qname");
        write_pe_bam(&bam_path, vec![pair.r1().clone(), pair.r2().clone()]);
        let outdir = work.path().join("out");
        run_binary(&bam_path, &outdir, &["--include_overlap"]);

        let cpg_ot = fs::read_to_string(outdir.join("CpG_OT_mbias_r2.txt")).unwrap();
        // The PE loop routes R2 calls via route_call(state, pair.r2(), …,
        // ReadIdentity::R2). If R2 calls weren't routed at all, the call
        // line wouldn't be in CpG_OT. Both R1 and R2's call lines should be
        // present (same qname for R1 + R2 per BismarkPair invariant).
        let qname_occurrences = cpg_ot.matches("mbias_qname").count();
        assert_eq!(
            qname_occurrences, 2,
            "both R1 and R2 of pair 'mbias_qname' should have call lines (R2 reaches route_call with ReadIdentity::R2 → mbias[1]): {cpg_ot}"
        );
    }

    /// Empty PE BAM: header-only files + 0-lines splitting report. PE-level
    /// re-verification of an invariant Phase B's SE empty-BAM smoke also
    /// covers. Plan §7.1.
    #[test]
    fn extract_pe_empty_bam_writes_only_header_files() {
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("empty_pe.bam");
        let header = header_with_chr1();
        let writer = BamWriter::from_path(&bam_path, header).unwrap();
        writer.finish().unwrap();

        let outdir = work.path().join("out");
        run_binary(&bam_path, &outdir, &[]);

        // Phase C.2 (#865): empty PE BAM → no records routed → all 12
        // per-strand files are empty after the run → all 12 are swept
        // (unlinked) at finalize time. Only the splitting-report and
        // M-bias.txt survive.
        for ctx in ["CpG", "CHG", "CHH"] {
            for strand in ["OT", "CTOT", "CTOB", "OB"] {
                let p = outdir.join(format!("{ctx}_{strand}_empty_pe.txt"));
                assert!(
                    !p.exists(),
                    "{}: empty per-strand file should be swept",
                    p.display()
                );
            }
        }
        let report = fs::read_to_string(outdir.join("empty_pe_splitting_report.txt")).unwrap();
        assert!(
            report.contains("Processed 0 lines in total"),
            "empty PE BAM → 0 lines processed; got:\n{report}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 6. AutoDetect tests
// ─────────────────────────────────────────────────────────────────────────

mod auto_detect {
    use super::helpers;
    use assert_cmd::Command;
    use bismark_io::BamWriter;
    use bstr::BString;
    use noodles_sam::Header;
    use noodles_sam::header::record::value::Map;
    use noodles_sam::header::record::value::map::header::Version;
    use noodles_sam::header::record::value::map::program::tag::COMMAND_LINE;
    use noodles_sam::header::record::value::map::{Program, ReferenceSequence};
    use std::fs;
    use std::num::NonZeroUsize;
    use std::path::PathBuf;

    fn header_with_bismark_pg(cl: &str) -> Header {
        let mut hd =
            Map::<noodles_sam::header::record::value::map::Header>::new(Version::new(1, 6));
        hd.other_fields_mut().insert(
            noodles_sam::header::record::value::map::header::tag::SORT_ORDER,
            BString::from(b"unsorted".to_vec()),
        );
        let mut prog = Map::<Program>::default();
        prog.other_fields_mut()
            .insert(COMMAND_LINE, BString::from(cl.as_bytes().to_vec()));
        let mut header = Header::builder()
            .set_header(hd)
            .add_program(BString::from(b"Bismark".to_vec()), prog)
            .build();
        header.reference_sequences_mut().insert(
            BString::from(b"chr1".to_vec()),
            Map::<ReferenceSequence>::new(NonZeroUsize::new(10_000).unwrap()),
        );
        header
    }

    fn header_no_bismark_pg() -> Header {
        let mut hd =
            Map::<noodles_sam::header::record::value::map::Header>::new(Version::new(1, 6));
        hd.other_fields_mut().insert(
            noodles_sam::header::record::value::map::header::tag::SORT_ORDER,
            BString::from(b"unsorted".to_vec()),
        );
        let mut prog = Map::<Program>::default();
        prog.other_fields_mut().insert(
            COMMAND_LINE,
            BString::from(b"bowtie2 -x index -U reads.fq.gz".to_vec()),
        );
        let mut header = Header::builder()
            .set_header(hd)
            .add_program(BString::from(b"bowtie2".to_vec()), prog)
            .build();
        header.reference_sequences_mut().insert(
            BString::from(b"chr1".to_vec()),
            Map::<ReferenceSequence>::new(NonZeroUsize::new(10_000).unwrap()),
        );
        header
    }

    #[test]
    fn main_auto_detect_routes_pe_bam_to_extract_pe() {
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("auto_pe.bam");
        let header =
            header_with_bismark_pg("bismark --genome /path/genome -1 R1.fq.gz -2 R2.fq.gz");
        let mut writer = BamWriter::from_path(&bam_path, header).unwrap();
        let pair = helpers::ot_pair(b"Z....", 100, b"....z", 130, b"auto_pair");
        writer.write_record(pair.r1()).unwrap();
        writer.write_record(pair.r2()).unwrap();
        writer.finish().unwrap();

        let outdir = work.path().join("out");
        // No --single-end / --paired-end — AutoDetect must dispatch to PE.
        let mut cmd = Command::cargo_bin("bismark-methylation-extractor-rs").unwrap();
        cmd.arg(&bam_path)
            .arg("--output_dir")
            .arg(&outdir)
            .assert()
            .success();
        // Phase C.2 (#864): PE counter is now per-pair, not per-line.
        // 1 pair → "Processed 1 lines in total" (sequences_count semantic
        // matches Perl :2459).
        let report = fs::read_to_string(outdir.join("auto_pe_splitting_report.txt")).unwrap();
        assert!(report.contains("Processed 1 lines in total"));
    }

    #[test]
    fn main_auto_detect_routes_se_bam_to_extract_se() {
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("auto_se.bam");
        let header = header_with_bismark_pg("bismark --genome /path/genome reads.fq.gz");
        let mut writer = BamWriter::from_path(&bam_path, header).unwrap();
        let rec = helpers::synth(
            b"CT",
            b"CT",
            b"Z....",
            b"AAAAA",
            100,
            &[(noodles_sam::alignment::record::cigar::op::Kind::Match, 5)],
            0, // SE: FLAG 0
            b"se_read",
            0,
        );
        writer.write_record(&rec).unwrap();
        writer.finish().unwrap();

        let outdir = work.path().join("out");
        let mut cmd = Command::cargo_bin("bismark-methylation-extractor-rs").unwrap();
        cmd.arg(&bam_path)
            .arg("--output_dir")
            .arg(&outdir)
            .assert()
            .success();
        // SE counter should fire (1 line per record).
        let report = fs::read_to_string(outdir.join("auto_se_splitting_report.txt")).unwrap();
        assert!(
            report.contains("Processed 1 lines in total"),
            "got:\n{report}"
        );
    }

    #[test]
    fn main_auto_detect_fails_without_bismark_pg() {
        let work = tempfile::tempdir().unwrap();
        let bam_path: PathBuf = work.path().join("no_bismark.bam");
        let header = header_no_bismark_pg();
        let writer = BamWriter::from_path(&bam_path, header).unwrap();
        writer.finish().unwrap();

        let outdir = work.path().join("out");
        let mut cmd = Command::cargo_bin("bismark-methylation-extractor-rs").unwrap();
        cmd.arg(&bam_path)
            .arg("--output_dir")
            .arg(&outdir)
            .assert()
            .failure()
            .stderr(predicates::str::contains(
                "pass `--single-end` or `--paired-end` explicitly",
            ));
    }
}

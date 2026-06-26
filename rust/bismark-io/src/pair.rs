//! Paired-end Bismark records.
//!
//! [`BismarkPair`] is the structural-correctness scaffold that makes the
//! per-record-strand-vs-pair-strand distinction explicit at the type level.
//! Output-routing code for paired-end data uses `pair_strand()` — which is
//! derived from R1 alone — rather than each mate's `record_strand()` (which
//! differs between R1 and R2 of a directional pair).
//!
//! See `DESIGN.md` Q1 and `PLAN.md` rev 3 for the rationale, including
//! why the prior-art Rust port's per-call strand re-derivation produced
//! the strand-routing bug this type prevents.

use crate::error::BismarkIoError;
use crate::record::BismarkRecord;
use crate::strand::BismarkStrand;

/// A paired-end alignment with its library-level strand classification.
///
/// `pair_strand` is decided by R1 (the SAM-spec rule for directional
/// libraries). Both R1 and R2 calls should be routed to output files using
/// `pair_strand()`, not each mate's own `record_strand()`.
#[derive(Debug, Clone)]
pub struct BismarkPair {
    r1: BismarkRecord,
    r2: BismarkRecord,
    pair_strand: BismarkStrand,
}

impl BismarkPair {
    /// Construct a pair from two adjacent mates in **file order**.
    ///
    /// `r1` is the first record in file order (= sequencing Read 1); `r2`
    /// is the second (= sequencing Read 2). Pairing is by file adjacency +
    /// qname, exactly as Perl `deduplicate_bismark` / the Perl methylation
    /// extractor do — it does **not** gate on the SAM first/second-in-pair
    /// FLAG bits (`0x40`/`0x80`).
    ///
    /// This matters because for **non-directional** libraries Bismark
    /// deliberately *swaps* those FLAG bits for CTOT/CTOB pairs: the
    /// first-in-file record (still sequencing Read 1) carries `0x80`
    /// ("second in pair") and the second carries `0x40`. See `bismark`
    /// `paired_end_SAM_output` (the CTOT/CTOB block, ~lines 8821-8852) and
    /// issue #1030. An earlier flag-based R1/R2 identity gate here rejected
    /// every such pair with `ReadIdentityMismatch`, which Perl never did —
    /// so dedup/extraction crashed on all non-directional PE data. The gate
    /// is gone; `pair.r1()` is keyed off file order, which the swap does not
    /// perturb.
    ///
    /// Validates:
    /// - The records share a qname (else [`BismarkIoError::MateMismatch`]).
    ///
    /// `pair_strand` is set to `r1.record_strand()` (derived from R1's
    /// `XR`/`XG`, not the FLAG bits). Note that R2's per-record strand will
    /// be the complement (e.g. for an OT-pair, R1 is OT and R2 is CTOT);
    /// this is expected and not an error.
    pub fn from_mates(r1: BismarkRecord, r2: BismarkRecord) -> Result<Self, BismarkIoError> {
        // Borrow-compare first; only allocate on the error path. At 27M+
        // pairs from a typical PE WGBS run the cheap-path matters.
        //
        // Both-None case: when neither record has a qname (unusual — Bismark
        // always names reads), both sides become `b""` and compare equal.
        // We treat this as "matching qnames" rather than an error; if upstream
        // is feeding us nameless reads, surfacing a MateMismatch here would
        // be misleading. Real corruption would manifest as one name set and
        // the other missing, which still triggers MateMismatch correctly.
        let r1_name_opt = r1.inner().name();
        let r2_name_opt = r2.inner().name();
        let r1_bytes: &[u8] = r1_name_opt.as_ref().map_or(b"", |n| AsRef::as_ref(*n));
        let r2_bytes: &[u8] = r2_name_opt.as_ref().map_or(b"", |n| AsRef::as_ref(*n));
        if r1_bytes != r2_bytes {
            return Err(BismarkIoError::MateMismatch {
                r1_qname: r1_bytes.to_vec(),
                r2_qname: r2_bytes.to_vec(),
            });
        }

        let pair_strand = r1.record_strand();
        Ok(Self {
            r1,
            r2,
            pair_strand,
        })
    }

    /// Library-level strand classification, derived from R1.
    ///
    /// **Use this for output routing of both R1 AND R2 methylation calls
    /// in paired-end mode.** Do NOT use each mate's `record_strand()`
    /// (they differ for R1 vs R2 of the same pair in a directional
    /// library).
    pub fn pair_strand(&self) -> BismarkStrand {
        self.pair_strand
    }

    /// R1 of the pair.
    pub fn r1(&self) -> &BismarkRecord {
        &self.r1
    }

    /// R2 of the pair.
    pub fn r2(&self) -> &BismarkRecord {
        &self.r2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::ReadIdentity;
    use bstr::BString;
    use noodles_sam::alignment::RecordBuf;
    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::Sequence;
    use noodles_sam::alignment::record_buf::data::field::Value;

    fn synth(name: &[u8], xr: &[u8], xg: &[u8], xm: &[u8], seq: &[u8], flags: u16) -> RecordBuf {
        let mut record = RecordBuf::default();
        *record.name_mut() = Some(BString::from(name.to_vec()));
        *record.flags_mut() = noodles_sam::alignment::record::Flags::from(flags);
        *record.sequence_mut() = Sequence::from(seq.to_vec());
        record
            .data_mut()
            .insert(Tag::from(*b"XR"), Value::String(BString::from(xr.to_vec())));
        record
            .data_mut()
            .insert(Tag::from(*b"XG"), Value::String(BString::from(xg.to_vec())));
        record
            .data_mut()
            .insert(Tag::from(*b"XM"), Value::String(BString::from(xm.to_vec())));
        record
    }

    /// Build a pair from explicit per-mate (XR, XG) tags **and real Bismark
    /// FLAGs**. The FLAG matters: for non-directional CTOT/CTOB pairs Bismark
    /// swaps the first/second-in-pair bits, so the first-in-file record
    /// carries `0x80` ("R2") — the very thing `from_mates` must now tolerate.
    /// (The old helper hard-coded `0x41`/`0x81`, which Bismark never emits for
    /// CTOT/CTOB, so the "non_directional" tests below silently never
    /// exercised the swap — issue #1030.)
    fn make_pair_records(
        r1_xr_xg: (&[u8], &[u8]),
        r1_flag: u16,
        r2_xr_xg: (&[u8], &[u8]),
        r2_flag: u16,
    ) -> (BismarkRecord, BismarkRecord) {
        let r1 = synth(
            b"qname1", r1_xr_xg.0, r1_xr_xg.1, b".....", b"ACGTC", r1_flag,
        );
        let r2 = synth(
            b"qname1", r2_xr_xg.0, r2_xr_xg.1, b".....", b"ACGTC", r2_flag,
        );
        (
            BismarkRecord::from_noodles_record(r1).unwrap(),
            BismarkRecord::from_noodles_record(r2).unwrap(),
        )
    }

    #[test]
    fn from_mates_ot_pair() {
        // OT pair (directional): flag_1=99 (R1, 0x40), flag_2=147 (R2, 0x80).
        // R1 XR=CT XG=CT → OT, R2 XR=GA XG=CT → CTOT.
        let (r1, r2) = make_pair_records((b"CT", b"CT"), 99, (b"GA", b"CT"), 147);
        assert_eq!(r1.record_strand(), BismarkStrand::OT);
        assert_eq!(r2.record_strand(), BismarkStrand::CTOT);
        // No swap on OT: first-in-file carries the R1 bit.
        assert_eq!(r1.read_identity(), ReadIdentity::R1);
        assert_eq!(r2.read_identity(), ReadIdentity::R2);
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(pair.pair_strand(), BismarkStrand::OT);
    }

    #[test]
    fn from_mates_ob_pair() {
        // OB pair (directional): flag_1=83 (R1, 0x40), flag_2=163 (R2, 0x80).
        // R1 XR=CT XG=GA → OB, R2 XR=GA XG=GA → CTOB.
        let (r1, r2) = make_pair_records((b"CT", b"GA"), 83, (b"GA", b"GA"), 163);
        assert_eq!(r1.record_strand(), BismarkStrand::OB);
        assert_eq!(r2.record_strand(), BismarkStrand::CTOB);
        assert_eq!(r1.read_identity(), ReadIdentity::R1);
        assert_eq!(r2.read_identity(), ReadIdentity::R2);
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(pair.pair_strand(), BismarkStrand::OB);
    }

    #[test]
    fn from_mates_ctot_pair_non_directional() {
        // Non-directional CTOT-pair — Bismark SWAPS the FLAG bits:
        // flag_1=147 (0x80 → "R2"!), flag_2=99 (0x40 → "R1"!).
        // First-in-file is still sequencing Read 1: XR=GA XG=CT → CTOT.
        let (r1, r2) = make_pair_records((b"GA", b"CT"), 147, (b"CT", b"CT"), 99);
        assert_eq!(r1.record_strand(), BismarkStrand::CTOT);
        assert_eq!(r2.record_strand(), BismarkStrand::OT);
        // The swap: first-in-file carries the 0x80 ("R2") bit. This is the
        // exact case the old flag gate rejected (#1030); from_mates must now
        // accept it, keying r1 off file order rather than the FLAG bit.
        assert_eq!(r1.read_identity(), ReadIdentity::R2);
        assert_eq!(r2.read_identity(), ReadIdentity::R1);
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(pair.pair_strand(), BismarkStrand::CTOT);
    }

    #[test]
    fn from_mates_ctob_pair_non_directional() {
        // Non-directional CTOB-pair — Bismark SWAPS the FLAG bits:
        // flag_1=163 (0x80 → "R2"!), flag_2=83 (0x40 → "R1"!).
        // First-in-file is still sequencing Read 1: XR=GA XG=GA → CTOB.
        let (r1, r2) = make_pair_records((b"GA", b"GA"), 163, (b"CT", b"GA"), 83);
        assert_eq!(r1.record_strand(), BismarkStrand::CTOB);
        assert_eq!(r2.record_strand(), BismarkStrand::OB);
        assert_eq!(r1.read_identity(), ReadIdentity::R2);
        assert_eq!(r2.read_identity(), ReadIdentity::R1);
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(pair.pair_strand(), BismarkStrand::CTOB);
    }

    // NOTE: the former `from_mates_wrong_r1_identity_errors` /
    // `_wrong_r2_identity_errors` tests asserted the flag-based R1/R2 gate
    // that was removed for #1030 (it rejected legitimate non-directional
    // CTOT/CTOB pairs). Pairing is now by file order + qname only, matching
    // Perl, so those negative tests no longer describe real behaviour and
    // have been deleted. The qname-mismatch guard below remains.

    #[test]
    fn from_mates_qname_mismatch_errors() {
        // R1 has qname1, R2 has qname2
        let r1_raw = synth(b"qname1", b"CT", b"CT", b".....", b"ACGTC", 0x41);
        let r2_raw = synth(b"qname2", b"GA", b"CT", b".....", b"ACGTC", 0x81);
        let r1 = BismarkRecord::from_noodles_record(r1_raw).unwrap();
        let r2 = BismarkRecord::from_noodles_record(r2_raw).unwrap();
        let err = BismarkPair::from_mates(r1, r2).unwrap_err();
        match err {
            BismarkIoError::MateMismatch { r1_qname, r2_qname } => {
                assert_eq!(r1_qname, b"qname1");
                assert_eq!(r2_qname, b"qname2");
            }
            other => panic!("expected MateMismatch, got {other:?}"),
        }
    }

    #[test]
    fn accessors_return_inner_records() {
        let (r1, r2) = make_pair_records((b"CT", b"CT"), 99, (b"GA", b"CT"), 147);
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(pair.r1().record_strand(), BismarkStrand::OT);
        assert_eq!(pair.r2().record_strand(), BismarkStrand::CTOT);
    }
}

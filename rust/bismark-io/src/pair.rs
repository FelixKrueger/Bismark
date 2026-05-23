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
use crate::record::{BismarkRecord, ReadIdentity};
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
    /// Construct a pair from R1 and R2 records.
    ///
    /// Validates:
    /// - `r1.read_identity()` is `R1` (else [`BismarkIoError::ReadIdentityMismatch`]).
    /// - `r2.read_identity()` is `R2` (else [`BismarkIoError::ReadIdentityMismatch`]).
    /// - The records share a qname (else [`BismarkIoError::MateMismatch`]).
    ///
    /// `pair_strand` is set to `r1.record_strand()`. Note that R2's
    /// per-record strand will be the complement (e.g. for an OT-pair, R1
    /// is OT and R2 is CTOT); this is expected and not an error.
    pub fn from_mates(r1: BismarkRecord, r2: BismarkRecord) -> Result<Self, BismarkIoError> {
        if r1.read_identity() != ReadIdentity::R1 {
            return Err(BismarkIoError::ReadIdentityMismatch {
                description: format!("expected R1 for first mate, got {:?}", r1.read_identity()),
            });
        }
        if r2.read_identity() != ReadIdentity::R2 {
            return Err(BismarkIoError::ReadIdentityMismatch {
                description: format!("expected R2 for second mate, got {:?}", r2.read_identity()),
            });
        }

        // Borrow-compare first; only allocate on the error path. At 27M+
        // pairs from a typical PE WGBS run the cheap-path matters.
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

    fn make_pair_records(
        r1_xr_xg: (&[u8], &[u8]),
        r2_xr_xg: (&[u8], &[u8]),
    ) -> (BismarkRecord, BismarkRecord) {
        let r1 = synth(b"qname1", r1_xr_xg.0, r1_xr_xg.1, b".....", b"ACGTC", 0x41);
        let r2 = synth(b"qname1", r2_xr_xg.0, r2_xr_xg.1, b".....", b"ACGTC", 0x81);
        (
            BismarkRecord::from_noodles_record(r1).unwrap(),
            BismarkRecord::from_noodles_record(r2).unwrap(),
        )
    }

    #[test]
    fn from_mates_ot_pair() {
        // OT pair: R1 XR=CT XG=CT → OT, R2 XR=GA XG=CT → CTOT
        let (r1, r2) = make_pair_records((b"CT", b"CT"), (b"GA", b"CT"));
        assert_eq!(r1.record_strand(), BismarkStrand::OT);
        assert_eq!(r2.record_strand(), BismarkStrand::CTOT);
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(pair.pair_strand(), BismarkStrand::OT);
    }

    #[test]
    fn from_mates_ob_pair() {
        // OB pair: R1 XR=CT XG=GA → OB, R2 XR=GA XG=GA → CTOB
        let (r1, r2) = make_pair_records((b"CT", b"GA"), (b"GA", b"GA"));
        assert_eq!(r1.record_strand(), BismarkStrand::OB);
        assert_eq!(r2.record_strand(), BismarkStrand::CTOB);
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(pair.pair_strand(), BismarkStrand::OB);
    }

    #[test]
    fn from_mates_ctot_pair_non_directional() {
        // Non-directional CTOT-pair: R1 XR=GA XG=CT → CTOT, R2 XR=CT XG=CT → OT
        let (r1, r2) = make_pair_records((b"GA", b"CT"), (b"CT", b"CT"));
        assert_eq!(r1.record_strand(), BismarkStrand::CTOT);
        assert_eq!(r2.record_strand(), BismarkStrand::OT);
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(pair.pair_strand(), BismarkStrand::CTOT);
    }

    #[test]
    fn from_mates_ctob_pair_non_directional() {
        // Non-directional CTOB-pair: R1 XR=GA XG=GA → CTOB, R2 XR=CT XG=GA → OB
        let (r1, r2) = make_pair_records((b"GA", b"GA"), (b"CT", b"GA"));
        assert_eq!(r1.record_strand(), BismarkStrand::CTOB);
        assert_eq!(r2.record_strand(), BismarkStrand::OB);
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(pair.pair_strand(), BismarkStrand::CTOB);
    }

    #[test]
    fn from_mates_wrong_r1_identity_errors() {
        // Caller passes R2 + R2 (no R1)
        let (_r1, r2a) = make_pair_records((b"CT", b"CT"), (b"GA", b"CT"));
        let (_r1b, r2b) = make_pair_records((b"CT", b"CT"), (b"GA", b"CT"));
        let err = BismarkPair::from_mates(r2a, r2b).unwrap_err();
        assert!(matches!(err, BismarkIoError::ReadIdentityMismatch { .. }));
    }

    #[test]
    fn from_mates_wrong_r2_identity_errors() {
        // Caller passes R1 + R1 (no R2)
        let (r1a, _r2) = make_pair_records((b"CT", b"CT"), (b"GA", b"CT"));
        let (r1b, _r2b) = make_pair_records((b"CT", b"CT"), (b"GA", b"CT"));
        let err = BismarkPair::from_mates(r1a, r1b).unwrap_err();
        assert!(matches!(err, BismarkIoError::ReadIdentityMismatch { .. }));
    }

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
        let (r1, r2) = make_pair_records((b"CT", b"CT"), (b"GA", b"CT"));
        let pair = BismarkPair::from_mates(r1, r2).unwrap();
        assert_eq!(pair.r1().record_strand(), BismarkStrand::OT);
        assert_eq!(pair.r2().record_strand(), BismarkStrand::CTOT);
    }
}

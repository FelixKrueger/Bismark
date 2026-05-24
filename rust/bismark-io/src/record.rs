//! Bismark-aware record wrapper.
//!
//! [`BismarkRecord`] wraps [`noodles_sam::alignment::RecordBuf`] with eager
//! strand classification at parse time. The per-record strand is derived
//! once from the XR/XG tags and stored as a typed field — never recomputed.
//!
//! See `DESIGN.md` Q1 for the rationale: this is the structural
//! prevention for the per-call strand-routing bug that affected the prior-
//! art Rust port. Output routing for paired-end data should use
//! [`crate::pair::BismarkPair`]'s `pair_strand()` rather than this
//! `record_strand()` (they differ between R1 and R2 of a directional pair).

use noodles_sam::alignment::RecordBuf;
use noodles_sam::alignment::record_buf::Cigar;

use crate::error::BismarkIoError;
use crate::strand::BismarkStrand;
use crate::tags;

/// Read identity within a paired-end (or single-end) alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReadIdentity {
    /// Single-end read (FLAG has neither 0x40 nor 0x80 set).
    Single,
    /// First in pair (FLAG & 0x40).
    R1,
    /// Second in pair (FLAG & 0x80).
    R2,
}

impl ReadIdentity {
    /// Derive read identity from SAM flag bits.
    pub fn from_flags(flags: u16) -> Self {
        let is_first = (flags & 0x40) != 0;
        let is_last = (flags & 0x80) != 0;
        match (is_first, is_last) {
            (true, false) => Self::R1,
            (false, true) => Self::R2,
            _ => Self::Single,
        }
    }

    /// Canonical short label (`"R1"`, `"R2"`, or `"SE"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Single => "SE",
            Self::R1 => "R1",
            Self::R2 => "R2",
        }
    }
}

/// A Bismark-aware alignment record.
///
/// Wraps a [`RecordBuf`] with the per-record strand classification already
/// computed (eagerly, at parse time) and the read identity decoded from
/// the SAM flag.
#[derive(Debug, Clone)]
pub struct BismarkRecord {
    inner: RecordBuf,
    record_strand: BismarkStrand,
    read_identity: ReadIdentity,
}

impl BismarkRecord {
    /// Construct from a noodles record, performing eager strand
    /// classification and data-integrity checks.
    ///
    /// Validates:
    /// - `XR:Z:` and `XG:Z:` tags present, parseable, and forming a valid
    ///   Bismark strand combination.
    /// - `XM:Z:` tag length equals the read sequence length (no
    ///   misalignment between methylation-call string and bases).
    ///
    /// Does NOT filter unmapped reads — that filtering happens at the
    /// reader-iterator layer, before this constructor sees the record.
    pub fn from_noodles_record(inner: RecordBuf) -> Result<Self, BismarkIoError> {
        let data = inner.data();
        let xr = tags::xr(data)?;
        let xg = tags::xg(data)?;
        let record_strand = BismarkStrand::from_xr_xg(xr, xg)?;

        // XM/seq length parity check.
        let xm = tags::xm(data)?;
        let seq_len = inner.sequence().as_ref().len();
        if xm.len() != seq_len {
            return Err(BismarkIoError::XmSeqLengthMismatch {
                xm_len: xm.len(),
                seq_len,
            });
        }

        let flag_bits = u16::from(inner.flags());
        let read_identity = ReadIdentity::from_flags(flag_bits);

        Ok(Self {
            inner,
            record_strand,
            read_identity,
        })
    }

    /// Strand derived from THIS record's own `XR:Z:`/`XG:Z:` tags.
    ///
    /// For R2 of a directional OT pair, this returns `CTOT` — which is
    /// NOT the pair-level routing key. Output routing for paired-end work
    /// should use [`crate::pair::BismarkPair::pair_strand`] instead.
    pub fn record_strand(&self) -> BismarkStrand {
        self.record_strand
    }

    /// Read identity (`Single`, `R1`, or `R2`) decoded from SAM flag bits.
    pub fn read_identity(&self) -> ReadIdentity {
        self.read_identity
    }

    /// Reference to the wrapped noodles record. Escape hatch for cases not
    /// covered by the explicit Bismark-aware accessors.
    pub fn inner(&self) -> &RecordBuf {
        &self.inner
    }

    /// Methylation-call string from the `XM:Z:` tag.
    ///
    /// Length is guaranteed by construction to equal the read sequence
    /// length.
    pub fn xm(&self) -> &[u8] {
        // Safe: `from_noodles_record` validated this tag's presence.
        tags::xm(self.inner.data()).expect("XM presence validated at construction")
    }

    /// 1-based alignment start position on the reference, or `None` if the
    /// record has no alignment position (unmapped — filtered upstream, but
    /// defensive).
    pub fn alignment_start(&self) -> Option<usize> {
        self.inner.alignment_start().map(usize::from)
    }

    /// CIGAR string from the wrapped record. Use with [`crate::CigarExt`]
    /// for reference-span, read-span, and aligned-position helpers.
    pub fn cigar(&self) -> &Cigar {
        self.inner.cigar()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bstr::BString;
    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::Sequence;
    use noodles_sam::alignment::record_buf::data::field::Value;

    /// Build a synthetic RecordBuf with the given XR/XG/XM and sequence.
    /// Caller can override flags via `flags_override` (default: 0).
    fn synth(xr: &[u8], xg: &[u8], xm: &[u8], seq: &[u8], flags_override: u16) -> RecordBuf {
        let mut record = RecordBuf::default();
        *record.flags_mut() = noodles_sam::alignment::record::Flags::from(flags_override);
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

    #[test]
    fn from_noodles_record_classifies_ot() {
        let r = synth(b"CT", b"CT", b".....", b"ACGTC", 0);
        let bm = BismarkRecord::from_noodles_record(r).unwrap();
        assert_eq!(bm.record_strand(), BismarkStrand::OT);
        assert_eq!(bm.read_identity(), ReadIdentity::Single);
    }

    #[test]
    fn from_noodles_record_classifies_ctot() {
        let r = synth(b"GA", b"CT", b".....", b"ACGTC", 0);
        let bm = BismarkRecord::from_noodles_record(r).unwrap();
        assert_eq!(bm.record_strand(), BismarkStrand::CTOT);
    }

    #[test]
    fn from_noodles_record_classifies_ob() {
        let r = synth(b"CT", b"GA", b".....", b"ACGTC", 0);
        let bm = BismarkRecord::from_noodles_record(r).unwrap();
        assert_eq!(bm.record_strand(), BismarkStrand::OB);
    }

    #[test]
    fn from_noodles_record_classifies_ctob() {
        let r = synth(b"GA", b"GA", b".....", b"ACGTC", 0);
        let bm = BismarkRecord::from_noodles_record(r).unwrap();
        assert_eq!(bm.record_strand(), BismarkStrand::CTOB);
    }

    #[test]
    fn from_noodles_record_decodes_r1() {
        // FLAG 0x40 = first in pair, 0x01 = paired
        let r = synth(b"CT", b"CT", b".....", b"ACGTC", 0x41);
        let bm = BismarkRecord::from_noodles_record(r).unwrap();
        assert_eq!(bm.read_identity(), ReadIdentity::R1);
    }

    #[test]
    fn from_noodles_record_decodes_r2() {
        // FLAG 0x80 = second in pair, 0x01 = paired
        let r = synth(b"GA", b"CT", b".....", b"ACGTC", 0x81);
        let bm = BismarkRecord::from_noodles_record(r).unwrap();
        assert_eq!(bm.read_identity(), ReadIdentity::R2);
    }

    #[test]
    fn from_noodles_record_missing_xr_errors() {
        let mut r = synth(b"CT", b"CT", b".....", b"ACGTC", 0);
        // Remove XR
        r.data_mut().remove(&Tag::from(*b"XR"));
        let err = BismarkRecord::from_noodles_record(r).unwrap_err();
        assert!(matches!(err, BismarkIoError::MissingTag { tag: "XR" }));
    }

    #[test]
    fn from_noodles_record_missing_xg_errors() {
        let mut r = synth(b"CT", b"CT", b".....", b"ACGTC", 0);
        r.data_mut().remove(&Tag::from(*b"XG"));
        let err = BismarkRecord::from_noodles_record(r).unwrap_err();
        assert!(matches!(err, BismarkIoError::MissingTag { tag: "XG" }));
    }

    #[test]
    fn from_noodles_record_missing_xm_errors() {
        let mut r = synth(b"CT", b"CT", b".....", b"ACGTC", 0);
        r.data_mut().remove(&Tag::from(*b"XM"));
        let err = BismarkRecord::from_noodles_record(r).unwrap_err();
        assert!(matches!(err, BismarkIoError::MissingTag { tag: "XM" }));
    }

    #[test]
    fn from_noodles_record_malformed_strand_tags_errors() {
        let r = synth(b"XX", b"YY", b".....", b"ACGTC", 0);
        let err = BismarkRecord::from_noodles_record(r).unwrap_err();
        assert!(matches!(err, BismarkIoError::InvalidStrandTags { .. }));
    }

    #[test]
    fn from_noodles_record_xm_seq_length_mismatch_errors() {
        // XM is 5 chars, seq is 6 bases → mismatch
        let r = synth(b"CT", b"CT", b".....", b"ACGTCA", 0);
        let err = BismarkRecord::from_noodles_record(r).unwrap_err();
        assert!(matches!(
            err,
            BismarkIoError::XmSeqLengthMismatch {
                xm_len: 5,
                seq_len: 6
            }
        ));
    }

    #[test]
    fn from_noodles_record_accessors_work() {
        let r = synth(b"CT", b"CT", b".z.h.", b"ACGTC", 0);
        let bm = BismarkRecord::from_noodles_record(r).unwrap();
        assert_eq!(bm.xm(), b".z.h.");
        assert_eq!(bm.record_strand(), BismarkStrand::OT);
        // inner() escape hatch returns a reference to the wrapped record.
        let _ = bm.inner();
    }

    #[test]
    fn read_identity_from_flags_table() {
        assert_eq!(ReadIdentity::from_flags(0x00), ReadIdentity::Single);
        assert_eq!(ReadIdentity::from_flags(0x04), ReadIdentity::Single); // unmapped, no R1/R2 bit
        assert_eq!(ReadIdentity::from_flags(0x40), ReadIdentity::R1);
        assert_eq!(ReadIdentity::from_flags(0x80), ReadIdentity::R2);
        assert_eq!(ReadIdentity::from_flags(0x41), ReadIdentity::R1); // R1 + paired
        assert_eq!(ReadIdentity::from_flags(0x81), ReadIdentity::R2); // R2 + paired
        // Both R1 and R2 set is invalid in SAM spec; we treat as Single (defensive).
        assert_eq!(ReadIdentity::from_flags(0xC0), ReadIdentity::Single);
    }

    #[test]
    fn read_identity_as_str() {
        assert_eq!(ReadIdentity::Single.as_str(), "SE");
        assert_eq!(ReadIdentity::R1.as_str(), "R1");
        assert_eq!(ReadIdentity::R2.as_str(), "R2");
    }
}

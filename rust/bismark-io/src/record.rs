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
use smallvec::SmallVec;

use crate::cigar::CigarExt;
use crate::error::BismarkIoError;
use crate::strand::BismarkStrand;
use crate::tags;

/// One read-orientation-corrected XM call, yielded by
/// [`BismarkRecord::iter_aligned`].
///
/// Required by `bismark-extractor` (epic #798) for M-bias accumulation
/// by sequencing-cycle position. See [`BismarkRecord::iter_aligned`]
/// for the orientation contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlignedXmCall {
    /// 0-based read position from the **5' end of the sequenced read**.
    /// For `+` strand records (OT / CTOB) this equals the BAM-stored
    /// `read_pos`; for `-` strand records (OB / CTOT) it's the reversal
    /// (so position 0 = first sequencing cycle of the original read).
    pub read_pos_5p: u32,
    /// 1-based reference position the XM byte aligns to. Walks the CIGAR
    /// to handle InDels correctly.
    pub ref_pos: u32,
    /// Raw XM tag byte at this read position. NOT
    /// orientation-corrected — caller's `classify_xm_byte` interprets
    /// `Z`/`z`/`X`/`x`/`H`/`h`/`U`/`u`/`.` directly.
    pub xm_byte: u8,
}

/// Stack-allocated UMI storage. Inline capacity is 16 bytes — covers all
/// known Bismark UMI workflows (≤16 ASCII bytes including dual-UMI `+`
/// separators). UMIs longer than 16 bytes (notably dual-UMI of form
/// `XXXXXXXX+YYYYYYYY` at 17 bytes) heap-allocate transparently; the
/// dedup-key equality contract is unaffected.
pub type Umi = SmallVec<[u8; 16]>;

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
    /// Pre-extracted UMI (Phase B / v1.2). `None` for the v1.0/v1.1
    /// non-UMI path — readers constructed via [`crate::BamReader::new`]
    /// and the other no-UMI constructors set this to `None`. Set to
    /// `Some(...)` by the `*_with_umi` reader constructors when the qname
    /// matches the chosen extractor's pattern. UMI-aware dedup downstream
    /// errors on `None` records in UMI mode (faithful to Perl
    /// `deduplicate_bismark:662-663`).
    umi: Option<Umi>,
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
            umi: None,
        })
    }

    /// Construct as [`Self::from_noodles_record`] AND pre-extract the UMI
    /// from the record's qname using `extractor` (typically
    /// [`crate::umi::extract_barcode`] or
    /// [`crate::umi::extract_bclconvert`]).
    ///
    /// If `extractor` returns `Some`, the bytes are stored in the
    /// record's `umi` field. If it returns `None`, the field is left as
    /// `None` — the dedup pipeline downstream is responsible for emitting
    /// `UmiExtractionFailed` when UMI mode is engaged but a record has
    /// no UMI (faithful to Perl `deduplicate_bismark:662-663`).
    ///
    /// Records with no qname (`name() == None`) also get `umi: None`.
    pub fn from_noodles_record_with_umi(
        inner: RecordBuf,
        extractor: fn(&[u8]) -> Option<&[u8]>,
    ) -> Result<Self, BismarkIoError> {
        let mut rec = Self::from_noodles_record(inner)?;
        let qname_bytes: Option<&[u8]> = rec.inner.name().map(AsRef::as_ref);
        rec.umi = qname_bytes.and_then(extractor).map(SmallVec::from_slice);
        Ok(rec)
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

    /// Pre-extracted UMI (Phase B / v1.2). `None` when the reader was
    /// constructed via a non-UMI constructor (the v1.0/v1.1 default) or
    /// when the qname did not match the chosen UMI extractor's pattern.
    pub fn umi(&self) -> Option<&Umi> {
        self.umi.as_ref()
    }

    /// Set the record's UMI in place. Used by reader constructors that
    /// pre-extract UMIs at parse time, and by test code that constructs
    /// records manually.
    pub fn set_umi(&mut self, umi: Option<Umi>) {
        self.umi = umi;
    }

    /// Iterate XM calls oriented by the **5' end of the sequenced read**.
    ///
    /// For each XM byte that aligns to a reference position (skipping
    /// insertions and soft-clips), yields an [`AlignedXmCall`]. Walks
    /// the CIGAR to maintain `ref_pos` correctly through InDels.
    ///
    /// **Orientation correction** (the core reason for this method): BAM
    /// stores `-` strand reads reverse-complemented (so they align to
    /// the `+` strand). Walking the BAM-stored XM with the BAM-stored
    /// CIGAR puts M-bias positions end-to-end-flipped for every `-`
    /// strand record — `XM[0]` in the BAM is the **3'** end of the
    /// sequenced read, not the 5'. This iterator corrects that:
    ///
    /// - **`+` strand records (OT / CTOB)**: `read_pos_5p == BAM read_pos`;
    ///   iteration order is forward.
    /// - **`-` strand records (OB / CTOT)**: `read_pos_5p == seq_len - 1 - BAM read_pos`;
    ///   iteration order is reversed (so the first emitted item is at
    ///   `read_pos_5p == 0`, matching Perl's
    ///   `deduplicate_bismark`/`bismark_methylation_extractor` semantics
    ///   at lines 1619-1621 + 2877-2886).
    ///
    /// **Perl reference**: `bismark_methylation_extractor` reverses both
    /// the XM string AND the expanded CIGAR for `-` strand reads to
    /// achieve the same effect. This iterator hides the reversal
    /// complexity from consumers (extractor, future bismark2bedGraph).
    ///
    /// Returns a fully-materialized iterator — call cost is one CIGAR
    /// walk + one Vec allocation. For 100-bp reads with 95 aligned
    /// positions, that's ~95 × 12 bytes ≈ 1.1 KiB per record.
    ///
    /// **Insertion-position semantic divergence vs Perl** (documented for
    /// future readers): Perl `bismark_methylation_extractor` emits XM
    /// entries at insertion positions with `xm_byte == b'.'` and the
    /// preceding match's `ref_pos`. This Rust iterator **skips** them
    /// (via `CigarExt::aligned_positions().filter_map(|ap| ap.ref_offset?)`).
    /// Behaviorally identical for the bismark-extractor's M-bias use
    /// case (Perl's `.` at insertions is filtered by `classify_xm_byte`
    /// before counting), but consumers emitting raw XM bytes per read
    /// position (e.g. yacht-mode any_C_context output) should consult
    /// the XM tag directly rather than this iterator.
    ///
    /// Added in `bismark-io 1.0.0-beta.6` (issue #843, SPEC §6.5).
    pub fn iter_aligned(&self) -> std::vec::IntoIter<AlignedXmCall> {
        let is_forward = matches!(self.record_strand, BismarkStrand::OT | BismarkStrand::CTOB);
        let xm = self.xm();
        let seq_len = xm.len() as u32;
        // `from_noodles_record` doesn't validate alignment_start (unmapped
        // records are filtered upstream at the reader-iterator layer per
        // `read.rs::filter_unmapped_then_classify`). A mapped record without
        // alignment_start is a structural invariant violation — `expect`
        // surfaces it loudly rather than silently producing wrong ref_pos.
        let alignment_start = self.alignment_start().expect(
            "iter_aligned: record has no alignment_start; reader-iterator \
             should have filtered this as unmapped (flags & 0x4)",
        ) as u32;

        // Walk the CIGAR producing one AlignedPosition per read base.
        // Filter to positions that have a `ref_offset` (matches; skips
        // insertions and soft-clips) and pair with the XM byte at the
        // BAM-stored read_pos.
        let mut calls: Vec<AlignedXmCall> = self
            .cigar()
            .aligned_positions()
            .filter_map(|ap| {
                let ref_offset = ap.ref_offset?;
                let read_pos = ap.read_pos as u32;
                // Defensive: XM length == seq length per the
                // `from_noodles_record` parity check, so this index is
                // always in bounds for matches.
                let xm_byte = xm[ap.read_pos];
                Some(AlignedXmCall {
                    read_pos_5p: read_pos, // re-mapped below for `-` strand
                    ref_pos: alignment_start + ref_offset as u32,
                    xm_byte,
                })
            })
            .collect();

        if !is_forward {
            // `-` strand: re-map read_pos_5p to count from sequenced 5'
            // (which is the LAST BAM-stored read_pos), then reverse the
            // iteration order so the first emitted item is at
            // read_pos_5p == 0.
            for call in calls.iter_mut() {
                call.read_pos_5p = seq_len.saturating_sub(1).saturating_sub(call.read_pos_5p);
            }
            calls.reverse();
        }

        calls.into_iter()
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

    /// Synthesize a record with a qname (used by UMI tests). Returns
    /// a noodles `RecordBuf` ready for `from_noodles_record_with_umi`.
    fn synth_with_qname(qname: &[u8], xr: &[u8], xg: &[u8], xm: &[u8], seq: &[u8]) -> RecordBuf {
        let mut record = synth(xr, xg, xm, seq, 0);
        *record.name_mut() = Some(BString::from(qname.to_vec()));
        record
    }

    #[test]
    fn umi_field_is_none_for_default_constructor() {
        let r = synth_with_qname(b"read:CTCCTTAG", b"CT", b"CT", b".....", b"ACGTC");
        let bm = BismarkRecord::from_noodles_record(r).unwrap();
        assert!(
            bm.umi().is_none(),
            "non-UMI constructor must leave umi as None"
        );
    }

    #[test]
    fn from_noodles_record_with_umi_extracts_barcode_format() {
        let r = synth_with_qname(b"read:CTCCTTAG", b"CT", b"CT", b".....", b"ACGTC");
        let bm =
            BismarkRecord::from_noodles_record_with_umi(r, crate::umi::extract_barcode).unwrap();
        let umi = bm.umi().expect("barcode extractor must populate umi");
        assert_eq!(umi.as_slice(), b"CTCCTTAG");
    }

    #[test]
    fn from_noodles_record_with_umi_extracts_bclconvert_format() {
        let r = synth_with_qname(
            b"A00001:1:HABC:1:1101:1000:2000:CAAGAG_1:N:0:AATGACGC",
            b"CT",
            b"CT",
            b".....",
            b"ACGTC",
        );
        let bm =
            BismarkRecord::from_noodles_record_with_umi(r, crate::umi::extract_bclconvert).unwrap();
        let umi = bm.umi().expect("bclconvert extractor must populate umi");
        assert_eq!(umi.as_slice(), b"CAAGAG");
    }

    #[test]
    fn from_noodles_record_with_umi_no_umi_in_qname_yields_none() {
        // qname has no `:` → extractor returns None → umi field is None.
        // (Dedup pipeline downstream is responsible for `UmiExtractionFailed`.)
        let r = synth_with_qname(b"plain_qname_no_colon", b"CT", b"CT", b".....", b"ACGTC");
        let bm =
            BismarkRecord::from_noodles_record_with_umi(r, crate::umi::extract_barcode).unwrap();
        assert!(bm.umi().is_none());
    }

    #[test]
    fn set_umi_replaces_existing_umi() {
        let r = synth_with_qname(b"read:OLD", b"CT", b"CT", b".....", b"ACGTC");
        let mut bm = BismarkRecord::from_noodles_record(r).unwrap();
        assert!(bm.umi().is_none());
        bm.set_umi(Some(Umi::from_slice(b"NEWUMI42")));
        assert_eq!(bm.umi().unwrap().as_slice(), b"NEWUMI42");
        bm.set_umi(None);
        assert!(bm.umi().is_none());
    }

    // ─────────────────── iter_aligned() tests (#843) ───────────────────
    //
    // The orientation contract: `+` strand records (OT/CTOB) iterate
    // forward with `read_pos_5p == BAM read_pos`. `-` strand records
    // (OB/CTOT) iterate REVERSE with `read_pos_5p == seq_len - 1 -
    // BAM read_pos` (so position 0 = first sequencing cycle of the
    // original sequenced read, matching Perl semantics).

    use noodles_core::Position;
    use noodles_sam::alignment::record::cigar::Op;
    use noodles_sam::alignment::record::cigar::op::Kind;
    use noodles_sam::alignment::record_buf::Cigar;

    /// Build a synthetic record with explicit CIGAR ops + strand tags.
    /// Sequence + XM are the same length (5 bases / 5 XM chars by
    /// default; caller can pass longer via `xm` arg).
    fn synth_with_cigar(
        xr: &[u8],
        xg: &[u8],
        xm: &[u8],
        seq: &[u8],
        alignment_start: usize,
        cigar_ops: &[(Kind, usize)],
    ) -> BismarkRecord {
        let mut record = RecordBuf::default();
        *record.flags_mut() = noodles_sam::alignment::record::Flags::from(0);
        *record.sequence_mut() = Sequence::from(seq.to_vec());
        *record.alignment_start_mut() = Some(Position::try_from(alignment_start).unwrap());
        *record.cigar_mut() = Cigar::from(
            cigar_ops
                .iter()
                .map(|(k, n)| Op::new(*k, *n))
                .collect::<Vec<_>>(),
        );
        record
            .data_mut()
            .insert(Tag::from(*b"XR"), Value::String(BString::from(xr.to_vec())));
        record
            .data_mut()
            .insert(Tag::from(*b"XG"), Value::String(BString::from(xg.to_vec())));
        record
            .data_mut()
            .insert(Tag::from(*b"XM"), Value::String(BString::from(xm.to_vec())));
        BismarkRecord::from_noodles_record(record).unwrap()
    }

    #[test]
    fn iter_aligned_forward_strand_ot_5m_identity() {
        // OT pair-strand: walk BAM XM + CIGAR forward.
        // 5-base read, 5M CIGAR, alignment_start=100.
        // Expected: (read_pos_5p, ref_pos, xm) = (0,100,Z), (1,101,z), (2,102,X), (3,103,h), (4,104,.)
        let bm = synth_with_cigar(b"CT", b"CT", b"ZzXh.", b"ACGTC", 100, &[(Kind::Match, 5)]);
        let calls: Vec<_> = bm.iter_aligned().collect();
        assert_eq!(calls.len(), 5);
        assert_eq!(
            calls[0],
            AlignedXmCall {
                read_pos_5p: 0,
                ref_pos: 100,
                xm_byte: b'Z'
            }
        );
        assert_eq!(
            calls[4],
            AlignedXmCall {
                read_pos_5p: 4,
                ref_pos: 104,
                xm_byte: b'.'
            }
        );
    }

    #[test]
    fn iter_aligned_reverse_strand_ob_reverses_and_remaps_read_pos() {
        // OB pair-strand: walk BAM XM in REVERSE, with read_pos_5p remapped.
        // Same fixture as the forward test but XR=CT XG=GA → OB.
        // Expected (in reverse): (0, 104, '.'), (1, 103, h), (2, 102, X), (3, 101, z), (4, 100, Z)
        let bm = synth_with_cigar(b"CT", b"GA", b"ZzXh.", b"ACGTC", 100, &[(Kind::Match, 5)]);
        let calls: Vec<_> = bm.iter_aligned().collect();
        assert_eq!(calls.len(), 5);
        assert_eq!(
            calls[0],
            AlignedXmCall {
                read_pos_5p: 0,
                ref_pos: 104,
                xm_byte: b'.'
            }
        );
        assert_eq!(
            calls[4],
            AlignedXmCall {
                read_pos_5p: 4,
                ref_pos: 100,
                xm_byte: b'Z'
            }
        );
    }

    #[test]
    fn iter_aligned_forward_strand_with_insertion_skips_insertion() {
        // 5M2I3M: read positions 0-4 + 7-9 align to reference; 5-6 are
        // insertion (no ref_pos). Forward orientation.
        // alignment_start=100; ref positions: 100,101,102,103,104, [skip], 105,106,107
        let bm = synth_with_cigar(
            b"CT",
            b"CT",
            b"ZZZZZ..HHH", // 10 XM bytes (matches 5M2I3M read span)
            b"AAAAAAAAAA", // 10-base sequence
            100,
            &[(Kind::Match, 5), (Kind::Insertion, 2), (Kind::Match, 3)],
        );
        let calls: Vec<_> = bm.iter_aligned().collect();
        // 8 aligned positions (the 2 insertion bases are skipped).
        assert_eq!(calls.len(), 8);
        // First 5: read_pos 0-4 → ref_pos 100-104, XM Z
        assert_eq!(calls[0].read_pos_5p, 0);
        assert_eq!(calls[0].ref_pos, 100);
        assert_eq!(calls[0].xm_byte, b'Z');
        assert_eq!(calls[4].read_pos_5p, 4);
        assert_eq!(calls[4].ref_pos, 104);
        // Next 3: read_pos 7,8,9 (NOT 5,6,7 — insertion bumps read but not ref)
        //         → ref_pos 105,106,107, XM H
        assert_eq!(calls[5].read_pos_5p, 7);
        assert_eq!(calls[5].ref_pos, 105);
        assert_eq!(calls[5].xm_byte, b'H');
        assert_eq!(calls[7].read_pos_5p, 9);
        assert_eq!(calls[7].ref_pos, 107);
    }

    #[test]
    fn iter_aligned_forward_strand_with_deletion_skips_ref_positions() {
        // 5M2D3M: read 8 bases; ref 10 bases (deletion advances ref only).
        // alignment_start=100; ref positions: 100,101,102,103,104, [skip 105,106], 107,108,109
        let bm = synth_with_cigar(
            b"CT",
            b"CT",
            b"ZZZZZHHH",
            b"AAAAAAAA",
            100,
            &[(Kind::Match, 5), (Kind::Deletion, 2), (Kind::Match, 3)],
        );
        let calls: Vec<_> = bm.iter_aligned().collect();
        assert_eq!(calls.len(), 8); // 8 read bases align
        assert_eq!(calls[4].read_pos_5p, 4);
        assert_eq!(calls[4].ref_pos, 104);
        // Position 5 in the iter is read_pos=5, but ref_pos=107 (deletion-jump).
        assert_eq!(calls[5].read_pos_5p, 5);
        assert_eq!(calls[5].ref_pos, 107);
        assert_eq!(calls[5].xm_byte, b'H');
        assert_eq!(calls[7].ref_pos, 109);
    }

    #[test]
    fn iter_aligned_forward_strand_with_soft_clip_skips_clipped_positions() {
        // 2S6M2S: read 10 bases; ref 6 bases. Soft-clipped at both ends.
        // read_pos 0-1 + 8-9 are soft-clipped (no ref_pos); 2-7 align.
        // alignment_start=100; ref: 100..105 for read 2..7
        let bm = synth_with_cigar(
            b"CT",
            b"CT",
            b"..ZZZZZZ..",
            b"AAAAAAAAAA",
            100,
            &[(Kind::SoftClip, 2), (Kind::Match, 6), (Kind::SoftClip, 2)],
        );
        let calls: Vec<_> = bm.iter_aligned().collect();
        assert_eq!(calls.len(), 6); // only the 6 matched positions
        assert_eq!(calls[0].read_pos_5p, 2);
        assert_eq!(calls[0].ref_pos, 100);
        assert_eq!(calls[0].xm_byte, b'Z');
        assert_eq!(calls[5].read_pos_5p, 7);
        assert_eq!(calls[5].ref_pos, 105);
    }

    #[test]
    fn iter_aligned_reverse_strand_with_soft_clip_orient_correctly() {
        // 2S6M2S on `-` strand. Forward order would yield read_pos 2..7
        // mapped to ref 100..105. Reverse strand should yield in reverse
        // order, with read_pos_5p remapped.
        // seq_len = 10. read_pos_5p = 10 - 1 - bam_read_pos.
        // bam_read_pos 7 → read_pos_5p 2
        // bam_read_pos 2 → read_pos_5p 7
        let bm = synth_with_cigar(
            b"CT", // XR
            b"GA", // XG → OB (reverse pair-strand)
            b"..ZZZZZZ..",
            b"AAAAAAAAAA",
            100,
            &[(Kind::SoftClip, 2), (Kind::Match, 6), (Kind::SoftClip, 2)],
        );
        let calls: Vec<_> = bm.iter_aligned().collect();
        assert_eq!(calls.len(), 6);
        // First emitted: highest bam_read_pos (7) → read_pos_5p (10-1-7=2)
        //                ref_pos = 105 (rightmost)
        assert_eq!(calls[0].read_pos_5p, 2);
        assert_eq!(calls[0].ref_pos, 105);
        // Last emitted: lowest bam_read_pos (2) → read_pos_5p (10-1-2=7)
        //               ref_pos = 100 (leftmost)
        assert_eq!(calls[5].read_pos_5p, 7);
        assert_eq!(calls[5].ref_pos, 100);
    }

    #[test]
    fn iter_aligned_ctob_strand_is_forward_orientation() {
        // CTOB (XR=GA XG=GA) is in the forward group per
        // `is_forward(strand)`. Verify orientation matches OT.
        let bm = synth_with_cigar(b"GA", b"GA", b"ZzXh.", b"ACGTC", 100, &[(Kind::Match, 5)]);
        assert_eq!(bm.record_strand(), BismarkStrand::CTOB);
        let calls: Vec<_> = bm.iter_aligned().collect();
        // Should be forward order (same as OT case above)
        assert_eq!(calls[0].read_pos_5p, 0);
        assert_eq!(calls[4].read_pos_5p, 4);
        assert_eq!(calls[0].ref_pos, 100);
        assert_eq!(calls[4].ref_pos, 104);
    }

    /// PE-pair coverage (both reviewers' Medium finding): R2 of an OT
    /// pair has `record_strand == CTOT` (per-record XR/XG classification:
    /// XR=GA XG=CT for the reverse-complemented mate). `iter_aligned`
    /// must reverse the R2 walk so M-bias positions count from the
    /// sequenced 5' of the R2 read. Mirrors Perl's per-mate reversal at
    /// `bismark_methylation_extractor:1933-1939` (PE R1) +
    /// `:2877-2886` (PE R2).
    ///
    /// Note: this is structurally the same fixture as
    /// `iter_aligned_ctot_strand_is_reverse_orientation` (single record
    /// with CTOT classification) — the per-record orientation is what
    /// matters; `BismarkRecord` doesn't carry pair context. This test
    /// adds the FLAG bits (0x81 = R2 + paired) so future readers see
    /// the PE-pair context explicitly.
    #[test]
    fn iter_aligned_pe_r2_of_ot_pair_is_reverse_orientation() {
        // R2 of OT pair: XR=GA XG=CT → record_strand=CTOT (reverse).
        // FLAG 0x81 = R2 + paired.
        let mut record = RecordBuf::default();
        *record.flags_mut() = noodles_sam::alignment::record::Flags::from(0x81);
        *record.sequence_mut() = Sequence::from(b"ACGTC".to_vec());
        *record.alignment_start_mut() = Some(Position::try_from(200).unwrap());
        *record.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, 5)]);
        record.data_mut().insert(
            Tag::from(*b"XR"),
            Value::String(BString::from(b"GA".to_vec())),
        );
        record.data_mut().insert(
            Tag::from(*b"XG"),
            Value::String(BString::from(b"CT".to_vec())),
        );
        record.data_mut().insert(
            Tag::from(*b"XM"),
            Value::String(BString::from(b"Z.x.H".to_vec())),
        );
        let bm = BismarkRecord::from_noodles_record(record).unwrap();
        assert_eq!(bm.record_strand(), BismarkStrand::CTOT);
        assert_eq!(bm.read_identity(), ReadIdentity::R2);

        let calls: Vec<_> = bm.iter_aligned().collect();
        assert_eq!(calls.len(), 5);
        // Reverse-strand: first emitted is bam_read_pos=4 → read_pos_5p=0,
        // ref_pos=204, xm='H'. Last emitted is bam_read_pos=0 →
        // read_pos_5p=4, ref_pos=200, xm='Z'.
        assert_eq!(
            calls[0],
            AlignedXmCall {
                read_pos_5p: 0,
                ref_pos: 204,
                xm_byte: b'H'
            }
        );
        assert_eq!(
            calls[4],
            AlignedXmCall {
                read_pos_5p: 4,
                ref_pos: 200,
                xm_byte: b'Z'
            }
        );
    }

    #[test]
    fn iter_aligned_ctot_strand_is_reverse_orientation() {
        // CTOT (XR=GA XG=CT) is in the reverse group. Same orientation
        // as OB.
        let bm = synth_with_cigar(b"GA", b"CT", b"ZzXh.", b"ACGTC", 100, &[(Kind::Match, 5)]);
        assert_eq!(bm.record_strand(), BismarkStrand::CTOT);
        let calls: Vec<_> = bm.iter_aligned().collect();
        // Reverse: first emitted is bam_read_pos=4 → read_pos_5p=0, ref_pos=104
        assert_eq!(calls[0].read_pos_5p, 0);
        assert_eq!(calls[0].ref_pos, 104);
        assert_eq!(calls[0].xm_byte, b'.');
    }
}

//! Methylation-call classification + per-record extraction kernel.
//!
//! Phase B (rev 1): delegates the CIGAR walk + `-`-strand orientation
//! correction to `bismark-io 1.0.0-beta.6`'s `BismarkRecord::iter_aligned`.
//! Per SPEC §7.1 rev 2 note, the kernel is a thin filter over that iterator.

use crate::io::BismarkRecord;

use crate::extractor::error::BismarkExtractorError;

/// Cytosine context. `#[repr(u8)]` lets `[T; 3]` arrays index by context
/// via `as usize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CytosineContext {
    /// CpG context — `Z`/`z`.
    CpG = 0,
    /// CHG context — `X`/`x`.
    CHG = 1,
    /// CHH context — `H`/`h`.
    CHH = 2,
}

/// One methylation call extracted from a Bismark record.
///
/// `Copy` (16 bytes). Per-record extraction returns `Vec<MethCall>` which
/// the caller drains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethCall {
    /// 1-based reference position. From `AlignedXmCall::ref_pos`.
    pub ref_pos: u32,
    /// 0-based read position relative to the **first un-clipped base after
    /// `--ignore` trimming**. Soft-clip positions are counted in
    /// `aligned.read_pos_5p` (`iter_aligned` inherits
    /// `CigarExt::aligned_positions`'s read_pos which increments through
    /// soft-clip ops), but the per-call rebase at the `MethCall` struct
    /// literal in [`extract_calls`] subtracts `ignore_5p` so the first
    /// call reported to downstream consumers always lands at
    /// `read_pos == 0`.
    ///
    /// Examples:
    /// - `+`-strand `6M` record, `--ignore 0`: first call has `read_pos == 0`.
    /// - `+`-strand `6M` record, `--ignore 2`: first call has `read_pos == 0`
    ///   (the filter at L162 drops positions 0,1; the call at absolute
    ///   position 2 rebases to 0).
    /// - `+`-strand `5S6M` record, `--ignore 0`: first call has `read_pos == 5`
    ///   (soft-clip counted, ignore_5p does not subtract).
    /// - `+`-strand `5S6M` record, `--ignore 7`: first call has `read_pos == 0`
    ///   (filter drops soft-clip positions 0-4 and match position 5,6;
    ///   first surviving call at absolute position 7 rebases to 0).
    ///
    /// Matches Perl's `substr($meth_call, $ignore, ...)` rebasing at
    /// `bismark_methylation_extractor:1627` — Perl trims the meth_call
    /// string in-place; Rust trims the position via subtraction. Closes
    /// #876 Bug B (rev 0 emitted absolute positions, causing M-bias.txt to
    /// zero-pad slots 1..ignore_5p instead of starting data at slot 1).
    pub read_pos: u32,
    /// CpG / CHG / CHH.
    pub context: CytosineContext,
    /// `true` for uppercase XM (`Z`/`X`/`H`), `false` for lowercase.
    pub methylated: bool,
    /// Literal XM byte (`Z`/`z`/`X`/`x`/`H`/`h`). Preserved for
    /// `format_meth_line` byte-identity output.
    pub xm_byte: u8,
}

/// Outcome of classifying one XM byte.
///
/// Exposed as `pub` so integration tests can assert classification
/// directly; treated as an internal type for non-test callers (the public
/// kernel entry point is `extract_calls`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmClassification {
    /// Methylation call at known context + methylation state.
    Call(CytosineContext, /*methylated*/ bool),
    /// `U` / `u` — unknown context (CN or CHN). Silently skipped per
    /// Perl 2970/3052/4548.
    SkipUnknownContext,
    /// `.` — non-cytosine base. Silently skipped.
    SkipNonCytosine,
}

/// Classify one XM byte against the SPEC §5 table.
///
/// Phase B always errors on invalid bytes (mirrors Perl `die` at lines
/// 2972/3054). Phase E will add a `mbias_only_silence` kernel parameter
/// to mirror Perl's conditional die `die "..." unless ($mbias_only)`.
pub fn classify_xm_byte(
    byte: u8,
    ref_pos: u32,
    read_id: &str,
) -> Result<XmClassification, BismarkExtractorError> {
    match byte {
        b'Z' => Ok(XmClassification::Call(CytosineContext::CpG, true)),
        b'z' => Ok(XmClassification::Call(CytosineContext::CpG, false)),
        b'X' => Ok(XmClassification::Call(CytosineContext::CHG, true)),
        b'x' => Ok(XmClassification::Call(CytosineContext::CHG, false)),
        b'H' => Ok(XmClassification::Call(CytosineContext::CHH, true)),
        b'h' => Ok(XmClassification::Call(CytosineContext::CHH, false)),
        b'U' | b'u' => Ok(XmClassification::SkipUnknownContext),
        b'.' => Ok(XmClassification::SkipNonCytosine),
        other => Err(BismarkExtractorError::InvalidXmByte {
            byte: other,
            byte_char: other as char,
            ref_pos,
            read_id: read_id.to_string(),
        }),
    }
}

/// Render the QNAME of a record as a `String` (lossy-decode for non-UTF-8).
///
/// Used to attach a read-id to `InvalidXmByte` errors. Records without a
/// QNAME (rare) are rendered as the literal string `"<unnamed>"`.
fn render_qname(record: &BismarkRecord) -> String {
    match record.inner().name() {
        Some(name) => String::from_utf8_lossy(name.as_ref()).into_owned(),
        None => "<unnamed>".to_string(),
    }
}

/// Extract all methylation calls from one record.
///
/// Walks `record.iter_aligned()` (which already applies the `-`-strand
/// orientation correction per SPEC §6.5). Filters by ignore-region in
/// 5'-oriented read coordinates, classifies each XM byte, and emits one
/// `MethCall` per CpG/CHG/CHH call.
///
/// # Implementation invariant
///
/// This function **must use `aligned.xm_byte`** (carried by the iterator).
/// It must **never** re-index `record.xm()[read_pos_5p]` — for `-`-strand
/// records `read_pos_5p` counts from the sequenced 5' end while `record.xm()`
/// is BAM-stored, so the indices disagree by `seq_len - 1 - read_pos`.
///
/// # `mbias_only_silence` (Phase E)
///
/// When `true`, the kernel silently skips bytes that would otherwise
/// raise [`BismarkExtractorError::InvalidXmByte`] — mirroring Perl
/// `bismark_methylation_extractor:2972, 3054` (`die "unrecognised char"
/// unless ($mbias_only)`). Other classification outcomes (`U`/`u`/`.`)
/// continue to take the existing `Skip*` arms regardless of this flag.
/// The catch-arm is narrowed to specifically `InvalidXmByte` so any future
/// error variants in [`classify_xm_byte`] still propagate even under
/// `mbias_only_silence`.
///
/// # Errors
///
/// `BismarkExtractorError::InvalidXmByte` on any byte outside
/// `Z`/`z`/`X`/`x`/`H`/`h`/`U`/`u`/`.` **unless** `mbias_only_silence`
/// is set.
pub fn extract_calls(
    record: &BismarkRecord,
    ignore_5p: u32,
    ignore_3p: u32,
    mbias_only_silence: bool,
) -> Result<Vec<MethCall>, BismarkExtractorError> {
    // XM length equals the read sequence length (parity check in
    // `from_noodles_record`). Use this to compute the 3'-side ignore
    // boundary in 5'-oriented coordinates.
    let xm_len = record.xm().len() as u32;
    let lo = ignore_5p;
    let hi = xm_len.saturating_sub(ignore_3p);

    // Early-out if the ignore-region check would skip every position.
    if lo >= hi {
        return Ok(Vec::new());
    }

    // Render the QNAME once so error-path messages have it. The Vec allocation
    // here is small (typical QNAME ~30 bytes) and happens once per record.
    let read_id = render_qname(record);

    let mut calls: Vec<MethCall> = Vec::new();

    for aligned in record.iter_aligned() {
        // Ignore-region check operates on the 5'-oriented read position
        // (which includes soft-clip in the count — see `MethCall::read_pos`).
        if aligned.read_pos_5p < lo || aligned.read_pos_5p >= hi {
            continue;
        }

        // Use `aligned.xm_byte`; NEVER re-index `record.xm()[read_pos_5p]`.
        // The iterator carries the orientation-corrected byte.
        //
        // Phase E: narrow the silence path to specifically `InvalidXmByte`
        // (mirrors Perl `:2972/3054 die "..." unless ($mbias_only)`).
        // Any future error variants from `classify_xm_byte` continue to
        // propagate even under `mbias_only_silence`.
        match classify_xm_byte(aligned.xm_byte, aligned.ref_pos, &read_id) {
            Ok(XmClassification::Call(context, methylated)) => {
                // #876 Bug B fix: rebase to "0-based-after-clip" semantic so
                // downstream M-bias accumulators (route.rs:95 + parallel.rs:
                // 625/729/752) land the first surviving call in slot 1.
                // Matches Perl `substr($meth_call, $ignore, ...)` at :1627.
                //
                // The `saturating_sub` is defense-in-depth: the filter at
                // L162 above already guarantees `aligned.read_pos_5p >= lo`
                // where `lo = ignore_5p`, so the subtraction can never wrap.
                // Keeping `saturating_sub` (vs plain `-`) prevents future
                // refactors of the filter from introducing UB.
                calls.push(MethCall {
                    ref_pos: aligned.ref_pos,
                    read_pos: aligned.read_pos_5p.saturating_sub(ignore_5p),
                    context,
                    methylated,
                    xm_byte: aligned.xm_byte,
                });
            }
            Ok(XmClassification::SkipUnknownContext | XmClassification::SkipNonCytosine) => {}
            Err(BismarkExtractorError::InvalidXmByte { .. }) if mbias_only_silence => {
                // Skip the offending byte — matches Perl's silent-skip
                // branch under --mbias_only.
            }
            Err(e) => return Err(e),
        }
    }

    Ok(calls)
}

#[cfg(test)]
mod tests {
    //! #876 Bug B regression guards for `MethCall.read_pos` rebasing.
    //!
    //! The fix at line 177 transforms `read_pos = aligned.read_pos_5p` →
    //! `read_pos = aligned.read_pos_5p.saturating_sub(ignore_5p)`. This is the
    //! Choice 2 fix (plan rev 1): rebase at the source so all 4 M-bias
    //! accumulator consumers (route.rs:95 + parallel.rs:625/729/752) inherit
    //! the correct slot mapping for free. See plan §3 for full context.

    use super::*;
    use bstr::BString;
    use noodles_sam::alignment::record::cigar::Op;
    use noodles_sam::alignment::record::cigar::op::Kind;
    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::data::field::Value;
    use noodles_sam::alignment::record_buf::{Cigar, RecordBuf, Sequence};

    /// Build a minimal `BismarkRecord` with the given XM string,
    /// `{n_soft}S{n_match}M` CIGAR, `XR:Z:CT`, and the caller-chosen `XG`
    /// strand tag (`b"CT"` → OT/`+`, `b"GA"` → OB/`-`).
    /// Quality scores are filled with `30u8` matching seq length.
    fn synth_se_record_strand(
        xm: &[u8],
        n_soft: usize,
        n_match: usize,
        xg: &[u8],
    ) -> BismarkRecord {
        assert_eq!(
            xm.len(),
            n_soft + n_match,
            "XM length must match CIGAR length (soft + match)"
        );
        let seq_len = n_soft + n_match;
        let mut record = RecordBuf::default();
        *record.name_mut() = Some(BString::from(b"read1".to_vec()));
        *record.flags_mut() = noodles_sam::alignment::record::Flags::from(0u16);
        *record.reference_sequence_id_mut() = Some(0);
        *record.alignment_start_mut() = Some(noodles_core::Position::try_from(10).unwrap());
        // sequence: arbitrary but length must match. Use 'A' for all bases.
        *record.sequence_mut() = Sequence::from(vec![b'A'; seq_len]);
        *record.quality_scores_mut() =
            noodles_sam::alignment::record_buf::QualityScores::from(vec![30u8; seq_len]);
        // CIGAR: {n_soft}S{n_match}M (skipped if n_soft == 0).
        let mut ops: Vec<Op> = Vec::new();
        if n_soft > 0 {
            ops.push(Op::new(Kind::SoftClip, n_soft));
        }
        if n_match > 0 {
            ops.push(Op::new(Kind::Match, n_match));
        }
        *record.cigar_mut() = Cigar::from(ops);
        record
            .data_mut()
            .insert(Tag::from(*b"XM"), Value::String(BString::from(xm.to_vec())));
        record.data_mut().insert(
            Tag::from(*b"XR"),
            Value::String(BString::from(b"CT".to_vec())),
        );
        record
            .data_mut()
            .insert(Tag::from(*b"XG"), Value::String(BString::from(xg.to_vec())));
        BismarkRecord::from_noodles_record(record).expect("synth BismarkRecord")
    }

    /// OT/`+`-strand convenience wrapper (`XG:Z:CT`) — preserves the original
    /// `synth_se_record` API used by the existing `+`-strand tests.
    fn synth_se_record(xm: &[u8], n_soft: usize, n_match: usize) -> BismarkRecord {
        synth_se_record_strand(xm, n_soft, n_match, b"CT")
    }

    #[test]
    fn extract_calls_rebases_read_pos_after_ignore_5p() {
        // Setup: 6M CIGAR (no soft-clip), XM "zXhZxH" → 6 methylation calls
        // at absolute read positions 0..=5. With ignore_5p=2:
        // - Filter at L162 drops positions 0, 1 (read_pos_5p < lo=2)
        // - Surviving positions: 2, 3, 4, 5 (absolute)
        // - Bug B fix: rebase to 0, 1, 2, 3 (subtract ignore_5p=2)
        //
        // Without the fix, MethCall.read_pos would be [2, 3, 4, 5] (absolute).
        // With the fix, MethCall.read_pos is [0, 1, 2, 3] (rebased).
        let record = synth_se_record(b"zXhZxH", 0, 6);
        let calls = extract_calls(&record, /*ignore_5p=*/ 2, /*ignore_3p=*/ 0, false)
            .expect("extract_calls");
        let positions: Vec<u32> = calls.iter().map(|c| c.read_pos).collect();
        assert_eq!(
            positions,
            vec![0, 1, 2, 3],
            "after ignore_5p=2, positions must be rebased to 0..3 (not the absolute 2..5)"
        );
        // Defensive: also verify no extra calls leaked through the filter.
        assert_eq!(
            calls.len(),
            4,
            "must emit exactly 4 calls after 2-base ignore"
        );
    }

    /// #878 Test 1 — OB/`-`-strand rebase + OT≡OB orientation invariance.
    ///
    /// The existing #876 rebase guards are OT-only. For OB records
    /// `iter_aligned` reverses (`read_pos_5p = seq_len-1-BAM_index`,
    /// `record.rs:236`), so to make an OB fixture yield the SAME 5'-oriented
    /// calls as an OT fixture `XM_OT`, the OB XM must be `reverse(XM_OT)`.
    /// With `ignore_5p=2` the surviving calls rebase to read_pos `[0,1,2]`;
    /// reverting the fix (`call.rs:204` → `aligned.read_pos_5p`) makes both OT
    /// and OB produce `[2,3,4]`, so the `== [0,1,2]` assertion fails — the
    /// bidirectional regression-guard property. This guards the OB code path
    /// (a future refactor moving the `saturating_sub` onto the BAM-coord
    /// `read_pos` would silently break OB while passing OT-only tests).
    #[test]
    fn extract_calls_ob_strand_rebases_read_pos_after_ignore_5p() {
        // OT "..Zxh." → 5'-order calls: Z@2 (CpG,meth), x@3 (CHG,unmeth), h@4 (CHH,unmeth).
        let ot = synth_se_record_strand(b"..Zxh.", 0, 6, b"CT");
        // OB reverse("..Zxh.") = ".hxZ.." → identical 5'-oriented calls.
        let ob = synth_se_record_strand(b".hxZ..", 0, 6, b"GA");

        let ot_calls = extract_calls(&ot, /*ignore_5p=*/ 2, 0, false).expect("OT extract");
        let ob_calls = extract_calls(&ob, /*ignore_5p=*/ 2, 0, false).expect("OB extract");

        let ot_pos: Vec<u32> = ot_calls.iter().map(|c| c.read_pos).collect();
        let ob_pos: Vec<u32> = ob_calls.iter().map(|c| c.read_pos).collect();

        // Rebased to 0-based-after-clip (NOT the absolute 2,3,4).
        assert_eq!(
            ob_pos,
            vec![0, 1, 2],
            "OB read_pos must rebase after ignore_5p=2 (not stay absolute 2,3,4)"
        );
        // Orientation invariance: OT and OB (reversed XM) yield identical positions.
        assert_eq!(
            ot_pos, ob_pos,
            "OT and OB with reversed XM must agree on rebased read_pos"
        );
        // Context + methylation also match in 5'-order.
        let ob_ctx: Vec<(CytosineContext, bool)> =
            ob_calls.iter().map(|c| (c.context, c.methylated)).collect();
        assert_eq!(
            ob_ctx,
            vec![
                (CytosineContext::CpG, true),
                (CytosineContext::CHG, false),
                (CytosineContext::CHH, false),
            ],
            "OB calls must be CpG-meth, CHG-unmeth, CHH-unmeth in 5'-order"
        );
    }

    #[test]
    fn extract_calls_ignore_5p_zero_is_identity() {
        // Default-cell regression guard: with ignore_5p=0, read_pos values
        // must equal the absolute aligned.read_pos_5p (i.e., 0, 1, 2, 3, 4, 5
        // for a 6M record). The saturating_sub(0) must be a no-op.
        let record = synth_se_record(b"zXhZxH", 0, 6);
        let calls = extract_calls(&record, /*ignore_5p=*/ 0, /*ignore_3p=*/ 0, false)
            .expect("extract_calls");
        let positions: Vec<u32> = calls.iter().map(|c| c.read_pos).collect();
        assert_eq!(
            positions,
            vec![0, 1, 2, 3, 4, 5],
            "ignore_5p=0 must leave read_pos unchanged (identity transform)"
        );
        assert_eq!(calls.len(), 6, "must emit all 6 calls with no ignore");
    }

    #[test]
    fn extract_calls_rebase_combined_with_soft_clip() {
        // 5S6M CIGAR + XM "....zXhZxH" (5 soft-clip dots + 6 real calls).
        // iter_aligned filters out soft-clip positions (no XM emission for
        // soft-clip per bismark-io semantics), so the emitted aligned values
        // have read_pos_5p starting at 5 (post-soft-clip 5'-oriented).
        //
        // With ignore_5p=7 (skips 2 of the 6 match positions):
        // - Filter drops aligned.read_pos_5p < 7 → drops positions 5, 6
        // - Surviving: 7, 8, 9, 10 (absolute) → 4 calls
        // - Fix rebases to: 0, 1, 2, 3 (subtract ignore_5p=7)
        //
        // This proves the rebase is correct EVEN when soft-clip + ignore stack.
        let record = synth_se_record(b".....zXhZxH", 5, 6);
        let calls = extract_calls(&record, /*ignore_5p=*/ 7, /*ignore_3p=*/ 0, false)
            .expect("extract_calls");
        let positions: Vec<u32> = calls.iter().map(|c| c.read_pos).collect();
        assert_eq!(
            calls.len(),
            4,
            "must emit exactly 4 calls after 5S soft-clip + 2-base extra ignore"
        );
        assert_eq!(
            positions,
            vec![0, 1, 2, 3],
            "after 5S soft-clip + ignore_5p=7, rebased positions must be 0..3"
        );
    }
}

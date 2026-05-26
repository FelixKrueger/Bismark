//! Methylation-call classification + per-record extraction kernel.
//!
//! Phase B (rev 1): delegates the CIGAR walk + `-`-strand orientation
//! correction to `bismark-io 1.0.0-beta.6`'s `BismarkRecord::iter_aligned`.
//! Per SPEC §7.1 rev 2 note, the kernel is a thin filter over that iterator.

use bismark_io::BismarkRecord;

use crate::error::BismarkExtractorError;

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
    /// 0-based read position from the **5' end of the sequenced read**.
    /// **Includes soft-clipped positions in the count** (rev 1 correction
    /// per Reviewer B I1) — `iter_aligned` inherits `CigarExt::aligned_positions`'s
    /// `read_pos` which increments through soft-clip ops; the filter drops
    /// emission for soft-clip positions but does not renumber the remaining
    /// ones. For a `+`-strand `5S95M` record the first emitted call has
    /// `read_pos == 5`. Matches Perl `substr(meth_call, ignore)` indexing
    /// over the full XM tag length.
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
/// # Errors
///
/// `BismarkExtractorError::InvalidXmByte` on any byte outside
/// `Z`/`z`/`X`/`x`/`H`/`h`/`U`/`u`/`.`. Phase E will add an `mbias_only`
/// silencing path.
pub fn extract_calls(
    record: &BismarkRecord,
    ignore_5p: u32,
    ignore_3p: u32,
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
        match classify_xm_byte(aligned.xm_byte, aligned.ref_pos, &read_id)? {
            XmClassification::Call(context, methylated) => {
                calls.push(MethCall {
                    ref_pos: aligned.ref_pos,
                    read_pos: aligned.read_pos_5p,
                    context,
                    methylated,
                    xm_byte: aligned.xm_byte,
                });
            }
            XmClassification::SkipUnknownContext | XmClassification::SkipNonCytosine => {}
        }
    }

    Ok(calls)
}

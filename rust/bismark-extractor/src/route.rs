//! Per-call routing: M-bias accumulator → splitting-report counters →
//! split-file write.
//!
//! Phase B locked the ordering (M-bias → counters → `mbias_only` short-
//! circuit → write). Phase E adds two things:
//!   1. Yacht-mode col-6 / col-7 derivation (strand-conditional polarity
//!      per Perl `:4350, 4382, 4422-4447`) — computed once per call only
//!      when `state.mode == Yacht`, otherwise the writer gets `(0, 0)`
//!      sentinels it ignores.
//!   2. Threading the resolved [`OutputMode`] through to `write_call` via
//!      the `OutputFileMap::mode` field (no change to `route_call`'s
//!      signature; the mode lives on the writer struct).

use bismark_io::CigarExt;
use bismark_io::{BismarkRecord, BismarkStrand, ReadIdentity};

use crate::call::{CytosineContext, MethCall};
use crate::cli::OutputMode;
use crate::error::BismarkExtractorError;
use crate::state::ExtractState;

/// Route one extracted call.
///
/// Order:
/// 1. Increment M-bias counter (unless `state.mbias_off`).
/// 2. Increment splitting-report counters (unconditional — matches Perl;
///    happens even under `--mbias_only`).
/// 3. If `state.mbias_only`: return early (skips split-file write).
/// 4. Compute yacht col-6 / col-7 if mode is Yacht (otherwise `(0, 0)`
///    sentinels — write_call ignores them for non-yacht modes).
/// 5. Write the split-file line.
pub fn route_call(
    state: &mut ExtractState,
    record: &BismarkRecord,
    chr: &str,
    strand: BismarkStrand,
    call: MethCall,
    read_identity: ReadIdentity,
) -> Result<(), BismarkExtractorError> {
    // ── 1. M-bias accumulation ──
    if !state.mbias_off {
        let table_idx = match read_identity {
            ReadIdentity::Single | ReadIdentity::R1 => 0,
            ReadIdentity::R2 => 1,
        };
        let pos_1based = call.read_pos.saturating_add(1);
        state.mbias[table_idx].accumulate(call.context, pos_1based, call.methylated);
    }

    // ── 2. Splitting-report counters (unconditional, BEFORE mbias_only short-circuit) ──
    state.report.calls_total = state.report.calls_total.saturating_add(1);
    match (call.context, call.methylated) {
        (CytosineContext::CpG, true) => {
            state.report.calls_cpg_meth = state.report.calls_cpg_meth.saturating_add(1);
        }
        (CytosineContext::CpG, false) => {
            state.report.calls_cpg_unmeth = state.report.calls_cpg_unmeth.saturating_add(1);
        }
        (CytosineContext::CHG, true) => {
            state.report.calls_chg_meth = state.report.calls_chg_meth.saturating_add(1);
        }
        (CytosineContext::CHG, false) => {
            state.report.calls_chg_unmeth = state.report.calls_chg_unmeth.saturating_add(1);
        }
        (CytosineContext::CHH, true) => {
            state.report.calls_chh_meth = state.report.calls_chh_meth.saturating_add(1);
        }
        (CytosineContext::CHH, false) => {
            state.report.calls_chh_unmeth = state.report.calls_chh_unmeth.saturating_add(1);
        }
    }

    // ── 3. `--mbias_only` short-circuit (skips split-file write) ──
    if state.mbias_only {
        return Ok(());
    }

    // ── 4. Yacht col-6 / col-7 derivation (Phase E Critical-1 fix). ──
    //
    // Forward-class (OT, CTOB): col-6 = alignment_start, col-7 = reference_end.
    // Reverse-class (OB, CTOT): col-6 = reference_end, col-7 = alignment_start
    // (Perl swaps the semantic meaning of `$start` / `$end` for `-` reads;
    // see Perl :4350, 4382, 4422-4447). Non-yacht modes get (0, 0)
    // sentinels — `write_call` ignores them.
    let (yacht_col6, yacht_col7) = if state.mode == OutputMode::Yacht {
        let alignment_start_pos = record.inner().alignment_start().ok_or_else(|| {
            BismarkExtractorError::InternalError {
                message: "yacht record missing alignment_start; bismark-io should have \
                          filtered this as unmapped (FLAG & 0x4)"
                    .to_string(),
            }
        })?;
        // noodles Position is 1-based; usize() returns the raw 1-based value.
        let alignment_start_usize: usize = alignment_start_pos.get();
        let alignment_start: u32 = u32::try_from(alignment_start_usize).map_err(|_| {
            BismarkExtractorError::InternalError {
                message: format!(
                    "yacht alignment_start {} overflows u32",
                    alignment_start_usize
                ),
            }
        })?;
        let ref_end_usize: usize = record.cigar().reference_end(alignment_start_usize);
        let ref_end: u32 =
            u32::try_from(ref_end_usize).map_err(|_| BismarkExtractorError::InternalError {
                message: format!("yacht reference_end {} overflows u32", ref_end_usize),
            })?;
        match strand {
            BismarkStrand::OT | BismarkStrand::CTOB => (alignment_start, ref_end),
            BismarkStrand::OB | BismarkStrand::CTOT => (ref_end, alignment_start),
        }
    } else {
        (0, 0)
    };

    // ── 5. Split-file write ──
    let qname: &[u8] = match record.inner().name() {
        Some(name) => name.as_ref(),
        None => b"<unnamed>",
    };
    state
        .fhs
        .write_call(qname, chr, call, strand, yacht_col6, yacht_col7)?;
    Ok(())
}

//! Per-call routing: M-bias accumulator → splitting-report counters →
//! split-file write.
//!
//! Phase B locked the ordering (M-bias → counters → `mbias_only` short-
//! circuit → write). Phase E added yacht-mode col-6 / col-7 derivation
//! (strand-conditional polarity per Perl `:4350, 4382, 4422-4447`).
//!
//! Phase F (rev 1) factors the yacht-polarity computation out into the
//! `pub(crate) compute_yacht_columns` helper so Phase F's worker loop can
//! reuse it without duplicating the strand-match logic. The legacy
//! `route_call` (used by the single-threaded `extract_se` / `extract_pe`
//! paths kept as the byte-identity reference) continues to consume the
//! same helper.

use bismark_io::CigarExt;
use bismark_io::{BismarkRecord, BismarkStrand, ReadIdentity};

use crate::call::{CytosineContext, MethCall};
use crate::cli::OutputMode;
use crate::error::BismarkExtractorError;
use crate::state::ExtractState;

/// Compute yacht-mode col-6 / col-7 (strand-conditional polarity per Perl
/// `:4350, 4382, 4422-4447`).
///
/// Returns `(0, 0)` for non-yacht modes — caller treats as sentinel.
///
/// Forward-class (OT / CTOB): emits `(alignment_start, reference_end)`.
/// Reverse-class (OB / CTOT): emits `(reference_end, alignment_start)`
/// — Perl swaps the semantic meaning of `$start` / `$end` for `-` reads.
///
/// # Errors
///
/// `InternalError` if `record.alignment_start()` is None (unmapped record
/// reached this code path — bismark-io should have filtered) or if
/// either `alignment_start` or `reference_end` overflows `u32` (defensive
/// — human/mouse coordinates fit comfortably).
pub(crate) fn compute_yacht_columns(
    mode: OutputMode,
    record: &BismarkRecord,
    strand: BismarkStrand,
) -> Result<(u32, u32), BismarkExtractorError> {
    if mode != OutputMode::Yacht {
        return Ok((0, 0));
    }
    let alignment_start_pos =
        record
            .inner()
            .alignment_start()
            .ok_or_else(|| BismarkExtractorError::InternalError {
                message: "yacht record missing alignment_start; bismark-io should have \
                          filtered this as unmapped (FLAG & 0x4)"
                    .to_string(),
            })?;
    let alignment_start_usize: usize = alignment_start_pos.get();
    let alignment_start: u32 =
        u32::try_from(alignment_start_usize).map_err(|_| BismarkExtractorError::InternalError {
            message: format!("yacht alignment_start {alignment_start_usize} overflows u32"),
        })?;
    let ref_end_usize: usize = record.cigar().reference_end(alignment_start_usize);
    let ref_end: u32 =
        u32::try_from(ref_end_usize).map_err(|_| BismarkExtractorError::InternalError {
            message: format!("yacht reference_end {ref_end_usize} overflows u32"),
        })?;
    Ok(match strand {
        BismarkStrand::OT | BismarkStrand::CTOB => (alignment_start, ref_end),
        BismarkStrand::OB | BismarkStrand::CTOT => (ref_end, alignment_start),
    })
}

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

    // ── 4. Yacht col-6 / col-7 derivation (factored to compute_yacht_columns
    //       in Phase F rev 1 for reuse by the parallel worker). ──
    let (yacht_col6, yacht_col7) = compute_yacht_columns(state.mode, record, strand)?;

    // ── 5. Split-file write (+ Phase 3a bedGraph tee) ──
    let qname: &[u8] = match record.inner().name() {
        Some(name) => name.as_ref(),
        None => b"<unnamed>",
    };
    // Phase 3a (F1/F6): disjoint field borrow so the bedGraph aggregator can be
    // passed to `write_call` alongside the `&mut OutputFileMap`. The tee lives
    // at the shared `write_call` funnel; this is the test-only single-threaded
    // reference path (D5's "collector only" intent is preserved — in
    // `--parallel` the funnel runs on the collector via `write_routed_call`).
    let ExtractState {
        fhs,
        bedgraph_aggregator,
        bedgraph_cx,
        ..
    } = state;
    fhs.write_call(
        qname,
        chr,
        call,
        strand,
        yacht_col6,
        yacht_col7,
        bedgraph_aggregator.as_mut(),
        *bedgraph_cx,
    )?;
    Ok(())
}

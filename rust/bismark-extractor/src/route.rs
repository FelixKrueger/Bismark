//! Per-call routing: M-bias accumulator → splitting-report counters →
//! split-file write.
//!
//! Rev 1 ordering correction (from Reviewer B I4): the splitting-report
//! counter increment must happen BEFORE the `mbias_only` short-circuit,
//! not after — Perl accumulates counts even under `--mbias_only`. The SPEC
//! §7.5 pseudocode has the ordering wrong; SPEC fix is queued as a separate
//! task. Phase B locks the correct order here.

use bismark_io::{BismarkRecord, BismarkStrand, ReadIdentity};

use crate::call::{CytosineContext, MethCall};
use crate::error::BismarkExtractorError;
use crate::state::ExtractState;

/// Route one extracted call.
///
/// Order:
/// 1. Increment M-bias counter (unless `state.mbias_off`).
/// 2. Increment splitting-report counters (unconditional — matches Perl).
/// 3. If `state.mbias_only`: return early.
/// 4. Write the split-file line.
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
        // 1-based position for the M-bias table — matches the format
        // `M-bias.txt` will emit in Phase D.
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

    // ── 4. Split-file write ──
    let qname: &[u8] = match record.inner().name() {
        Some(name) => name.as_ref(),
        None => b"<unnamed>",
    };
    state.fhs.write_call(qname, chr, call, strand)?;
    Ok(())
}

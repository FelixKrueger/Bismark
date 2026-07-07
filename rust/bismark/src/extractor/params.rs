//! Argument-struct types for the extraction kernel.
//!
//! Per SPEC §6.3 + §7.7: typed parameter structs replace the 14-arg
//! `extract_calls` signature seen in the prior-art Rust port. Adding a
//! new flag = adding a typed field, not appending to a positional list.
//!
//! Phase A: defines the struct shape. Field-level usage lands in
//! Phase B (SE extraction loop) onward.

use crate::io::{BismarkPair, BismarkRecord, BismarkStrand, ReadIdentity};

/// Per-record extraction parameters threaded through `extract_calls`.
///
/// **Phase A shape — fields finalized in Phase B implementation.**
/// The lifetime `'a` borrows the record + state from the caller.
#[derive(Debug)]
pub struct ExtractParams<'a> {
    /// The Bismark record to extract from.
    pub record: &'a BismarkRecord,
    /// Maps `noodles` refID → workspace-interned chr_id. Built once per
    /// input file by the pipeline.
    pub refid_table: &'a [u32],
    /// Read identity (R1 / R2 / Single). Decides which M-bias counter
    /// table to increment.
    pub read_identity: ReadIdentity,
    /// 5' trim count (read coordinates, post-soft-clip).
    pub ignore_5p: u32,
    /// 3' trim count.
    pub ignore_3p: u32,
    /// The pair-level strand for PE records (R1's record_strand for SE
    /// or PE — for PE R2 this is the PAIR's strand, not R2's
    /// record_strand=CTOT/CTOB).
    pub pair_strand: BismarkStrand,
}

/// Per-pair extraction parameters for PE mode. Owns the pair and the
/// per-mate `--ignore_*` settings; lifetime tied to the caller's
/// extraction state.
#[derive(Debug)]
pub struct PairParams<'a> {
    /// The Bismark pair (R1 + R2 enforced by `BismarkPair::from_mates`).
    pub pair: &'a BismarkPair,
    /// chr_id intern map.
    pub refid_table: &'a [u32],
    /// 5' trim for R1.
    pub ignore_5p_r1: u32,
    /// 3' trim for R1.
    pub ignore_3p_r1: u32,
    /// 5' trim for R2.
    pub ignore_5p_r2: u32,
    /// 3' trim for R2.
    pub ignore_3p_r2: u32,
    /// Drop R2 calls overlapping R1's reference span (PE default).
    pub no_overlap: bool,
}

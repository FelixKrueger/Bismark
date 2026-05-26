//! M-bias accumulator.
//!
//! Phase B accumulates counters per (read-identity × context × 1-based
//! position) only; the `M-bias.txt` writer lands in Phase D.
//!
//! Per SPEC §6.2 + §7.7: the surrounding state holds an `[MbiasTable; 2]`
//! indexed by read-identity (0 = R1/SE, 1 = R2). Per-context iteration in
//! the writer (Phase D) enumerates all 3 contexts explicitly — no
//! `_ => {}` fallthrough — which closes Alan's missing-CHG/CHH bug
//! structurally.

use crate::call::CytosineContext;

/// Counts at one read-coordinate position. 16 bytes; `Copy`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MbiasPos {
    /// Methylated calls (uppercase XM).
    pub meth: u64,
    /// Unmethylated calls (lowercase XM).
    pub unmeth: u64,
}

/// Per (context × position) table for one read identity.
///
/// Each `Vec<MbiasPos>` grows lazily — index `i` is the 1-based read
/// position (so index 0 is unused; an extra slot vs Perl's hash-based
/// storage, but trivial in bytes).
#[derive(Debug, Default)]
pub struct MbiasTable {
    /// CpG counters keyed by 1-based read position.
    pub cpg: Vec<MbiasPos>,
    /// CHG counters.
    pub chg: Vec<MbiasPos>,
    /// CHH counters.
    pub chh: Vec<MbiasPos>,
}

impl MbiasTable {
    /// Increment the cell at (context, 1-based position) for one call.
    ///
    /// Grows the underlying `Vec` lazily; no preallocation needed because
    /// reads have bounded length (~150 bp typical, ~300 bp max).
    pub fn accumulate(&mut self, context: CytosineContext, position_1based: u32, methylated: bool) {
        let vec = match context {
            CytosineContext::CpG => &mut self.cpg,
            CytosineContext::CHG => &mut self.chg,
            CytosineContext::CHH => &mut self.chh,
        };
        let idx = position_1based as usize;
        if vec.len() <= idx {
            vec.resize(idx + 1, MbiasPos::default());
        }
        let bucket = &mut vec[idx];
        if methylated {
            bucket.meth = bucket.meth.saturating_add(1);
        } else {
            bucket.unmeth = bucket.unmeth.saturating_add(1);
        }
    }
}

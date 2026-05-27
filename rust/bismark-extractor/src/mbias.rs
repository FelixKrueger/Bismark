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
    ///
    /// # Invariant
    ///
    /// `position_1based` must be `>= 1`. The M-bias.txt writer (Phase D)
    /// iterates `1..=max_position`, skipping slot 0. If a future kernel
    /// change ever passes `0`, the slot-0 data would be silently dropped
    /// from the M-bias.txt output. The `debug_assert!` below surfaces
    /// the regression at unit-test time; zero cost in release builds.
    pub fn accumulate(&mut self, context: CytosineContext, position_1based: u32, methylated: bool) {
        debug_assert!(
            position_1based >= 1,
            "MbiasTable::accumulate: position must be 1-based (>= 1), got {position_1based}"
        );
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

    /// Highest 1-based position observed across all three context vectors.
    ///
    /// Returns `0` if all vecs are empty (writer interprets as "emit
    /// headers only, no per-position rows"). Also returns `0` if the only
    /// allocated slot is slot 0 (which is never written to in practice;
    /// see [`accumulate`] invariant).
    ///
    /// Per SPEC §4.2 / Perl `bismark_methylation_extractor:647-661`: the
    /// writer iterates `1..=max_position` for every context, even if
    /// that context's own vec is shorter than `max_position` (yields
    /// zero-row entries).
    ///
    /// [`accumulate`]: Self::accumulate
    pub fn max_position(&self) -> u32 {
        let m1 = self.cpg.len().saturating_sub(1) as u32;
        let m2 = self.chg.len().saturating_sub(1) as u32;
        let m3 = self.chh.len().saturating_sub(1) as u32;
        m1.max(m2).max(m3)
    }
}

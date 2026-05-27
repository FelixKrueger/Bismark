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

    /// Sum `other` position-wise into `self`. Commutative and associative
    /// (each per-position count is a `u64::saturating_add` sum, which is
    /// commutative + associative unless saturation is hit — for M-bias
    /// counts on a single run, totals fit far below `u64::MAX`).
    ///
    /// Used by Phase F's collector to merge per-worker M-bias deltas at
    /// end-of-stream. Order of merge is irrelevant → byte-identical
    /// `M-bias.txt` regardless of N workers.
    pub fn add(&mut self, other: &Self) {
        Self::add_one(&mut self.cpg, &other.cpg);
        Self::add_one(&mut self.chg, &other.chg);
        Self::add_one(&mut self.chh, &other.chh);
    }

    /// Helper: sum `src` position-wise into `dst`, growing `dst` if needed.
    /// When `dst.len() > src.len()`, the surplus `dst` entries are left
    /// unchanged (zip stops at the shorter iterator) — correct because
    /// those positions only had `dst`'s contribution.
    fn add_one(dst: &mut Vec<MbiasPos>, src: &[MbiasPos]) {
        if dst.len() < src.len() {
            dst.resize(src.len(), MbiasPos::default());
        }
        for (s, o) in dst.iter_mut().zip(src.iter()) {
            s.meth = s.meth.saturating_add(o.meth);
            s.unmeth = s.unmeth.saturating_add(o.unmeth);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a small MbiasTable with one CpG meth at position 1 and one
    /// CHG unmeth at position 2. Compact constructor for the merge tests.
    fn synth(cpg_meth_at_1: u64, chg_unmeth_at_2: u64, chh_meth_at_3: u64) -> MbiasTable {
        let mut t = MbiasTable::default();
        for _ in 0..cpg_meth_at_1 {
            t.accumulate(CytosineContext::CpG, 1, true);
        }
        for _ in 0..chg_unmeth_at_2 {
            t.accumulate(CytosineContext::CHG, 2, false);
        }
        for _ in 0..chh_meth_at_3 {
            t.accumulate(CytosineContext::CHH, 3, true);
        }
        t
    }

    #[test]
    fn mbias_table_add_is_commutative() {
        let a_into_b = {
            let mut a = synth(3, 5, 7);
            let b = synth(11, 13, 17);
            a.add(&b);
            a
        };
        let b_into_a = {
            let mut b = synth(11, 13, 17);
            let a = synth(3, 5, 7);
            b.add(&a);
            b
        };
        assert_eq!(a_into_b.cpg, b_into_a.cpg);
        assert_eq!(a_into_b.chg, b_into_a.chg);
        assert_eq!(a_into_b.chh, b_into_a.chh);
    }

    #[test]
    fn mbias_table_add_is_associative() {
        // (a + b) + c
        let left = {
            let mut a = synth(2, 4, 6);
            let b = synth(8, 10, 12);
            let c = synth(14, 16, 18);
            a.add(&b);
            a.add(&c);
            a
        };
        // a + (b + c)
        let right = {
            let mut a = synth(2, 4, 6);
            let mut bc = synth(8, 10, 12);
            let c = synth(14, 16, 18);
            bc.add(&c);
            a.add(&bc);
            a
        };
        assert_eq!(left.cpg, right.cpg);
        assert_eq!(left.chg, right.chg);
        assert_eq!(left.chh, right.chh);
    }

    #[test]
    fn mbias_table_add_grows_when_other_larger() {
        let mut small = MbiasTable::default();
        small.accumulate(CytosineContext::CpG, 5, true);

        let mut large = MbiasTable::default();
        large.accumulate(CytosineContext::CpG, 100, true);

        // Merging the large into the small should grow small.cpg to length 101.
        small.add(&large);
        assert_eq!(small.cpg.len(), 101);
        // Position 5 still has the original small contribution (1 meth).
        assert_eq!(small.cpg[5].meth, 1);
        // Position 100 picked up large's contribution (1 meth).
        assert_eq!(small.cpg[100].meth, 1);
        // Positions in between (6..100) are zero-default — nothing happened there.
        for pos in 6..100 {
            assert_eq!(small.cpg[pos].meth, 0);
            assert_eq!(small.cpg[pos].unmeth, 0);
        }
    }

    #[test]
    fn mbias_table_add_self_larger_keeps_tail() {
        let mut large = MbiasTable::default();
        large.accumulate(CytosineContext::CpG, 100, true);
        large.accumulate(CytosineContext::CpG, 50, false);

        let mut small = MbiasTable::default();
        small.accumulate(CytosineContext::CpG, 50, true);

        // Merging the smaller into the larger should keep large.cpg.len() == 101.
        large.add(&small);
        assert_eq!(large.cpg.len(), 101);
        // Position 100 unchanged from large's original contribution.
        assert_eq!(large.cpg[100].meth, 1);
        // Position 50 has BOTH large's unmeth + small's meth.
        assert_eq!(large.cpg[50].meth, 1);
        assert_eq!(large.cpg[50].unmeth, 1);
    }
}

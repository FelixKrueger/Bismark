//! Bismark-flavoured CIGAR helpers as an extension trait on
//! [`noodles_sam::alignment::record_buf::Cigar`].
//!
//! Centralises the off-by-one-prone CIGAR arithmetic in one place so every
//! downstream binary inherits the same correct computation. In particular,
//! [`CigarExt::reference_end`] is the direct prevention for the
//! `pos.saturating_sub(1)` off-by-one that affected the prior-art Rust
//! port (97-position drift in the 10M PE audit).

use noodles_sam::alignment::record::cigar::op::Kind;
use noodles_sam::alignment::record_buf::Cigar;

/// Extension methods on [`noodles_sam::alignment::record_buf::Cigar`].
///
/// Bring into scope with `use bismark_io::CigarExt;`.
pub trait CigarExt {
    /// Number of reference bases consumed by the alignment.
    ///
    /// Sum of `Op::len()` for ops that consume reference: `M`, `D`, `N`,
    /// `=`, `X`.
    fn reference_span(&self) -> usize;

    /// Number of read bases consumed by the alignment.
    ///
    /// Sum of `Op::len()` for ops that consume read: `M`, `I`, `S`, `=`,
    /// `X`.
    fn read_span(&self) -> usize;

    /// 1-based inclusive last reference position covered by the alignment.
    ///
    /// Given a 1-based `start` position (as `noodles_sam::Record::alignment_start()`
    /// returns), this is `start + reference_span() - 1`. Matches the SAM
    /// spec convention and `noodles_sam::Record::alignment_end()` semantics.
    ///
    /// **Do not** roll your own `pos.saturating_sub(1)` arithmetic — that
    /// shortcut collapses adjacent positions on reverse-strand reads and
    /// caused a 97-position drift in the prior-art Rust port's dedup
    /// report. Use this helper.
    fn reference_end(&self, start: usize) -> usize;

    /// Iterator over aligned positions, one item per read base.
    ///
    /// For each base in the read (read_pos goes 0..read_span), yields the
    /// corresponding reference offset (if any) and the CIGAR op kind:
    ///
    /// - `M`, `=`, `X`: `ref_offset = Some(...)`, increments per base.
    /// - `I`: `ref_offset = None`, read advances but reference does not.
    /// - `S`: `ref_offset = None` (soft-clipped read base; not aligned).
    /// - `D`, `N`: no item (those ops consume reference only, not read).
    /// - `H`, `P`: skipped (consume neither read nor reference).
    fn aligned_positions(&self) -> AlignedPositions<'_>;
}

/// One aligned position from [`CigarExt::aligned_positions`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlignedPosition {
    /// 0-based position into the read sequence.
    pub read_pos: usize,
    /// 0-based offset from the alignment start on the reference, or `None`
    /// if this read base is not aligned to a reference position (insertion
    /// or soft-clip).
    pub ref_offset: Option<usize>,
    /// CIGAR op kind that produced this position.
    pub op_kind: Kind,
}

/// Iterator returned by [`CigarExt::aligned_positions`].
pub struct AlignedPositions<'a> {
    ops: std::slice::Iter<'a, noodles_sam::alignment::record::cigar::Op>,
    current_op: Option<(Kind, usize)>, // (op kind, remaining length in current op)
    read_pos: usize,
    ref_offset: usize,
}

impl<'a> AlignedPositions<'a> {
    fn new(cigar: &'a Cigar) -> Self {
        Self {
            ops: cigar.as_ref().iter(),
            current_op: None,
            read_pos: 0,
            ref_offset: 0,
        }
    }
}

impl<'a> Iterator for AlignedPositions<'a> {
    type Item = AlignedPosition;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Refill `current_op` from the iterator if empty, skipping
            // CIGAR ops that yield no items (D, N, H, P).
            if self.current_op.is_none() {
                let op = self.ops.next()?;
                let kind = op.kind();
                let len = op.len();
                match kind {
                    Kind::Deletion | Kind::Skip => {
                        // Consumes reference only; no read positions.
                        self.ref_offset += len;
                        continue;
                    }
                    Kind::HardClip | Kind::Pad => {
                        // Consumes neither read nor reference.
                        continue;
                    }
                    _ => {
                        if len == 0 {
                            continue;
                        }
                        self.current_op = Some((kind, len));
                    }
                }
            }

            // Emit one item from the current op.
            let (kind, remaining) = self.current_op.as_mut().expect("just filled");
            let kind = *kind;

            let item = match kind {
                Kind::Match | Kind::SequenceMatch | Kind::SequenceMismatch => {
                    let item = AlignedPosition {
                        read_pos: self.read_pos,
                        ref_offset: Some(self.ref_offset),
                        op_kind: kind,
                    };
                    self.read_pos += 1;
                    self.ref_offset += 1;
                    item
                }
                Kind::Insertion | Kind::SoftClip => {
                    let item = AlignedPosition {
                        read_pos: self.read_pos,
                        ref_offset: None,
                        op_kind: kind,
                    };
                    self.read_pos += 1;
                    item
                }
                // Deletion, Skip, HardClip, Pad handled above; unreachable here.
                Kind::Deletion | Kind::Skip | Kind::HardClip | Kind::Pad => unreachable!(),
            };

            *remaining -= 1;
            if *remaining == 0 {
                self.current_op = None;
            }
            return Some(item);
        }
    }
}

impl CigarExt for Cigar {
    fn reference_span(&self) -> usize {
        self.as_ref()
            .iter()
            .filter_map(|op| match op.kind() {
                Kind::Match
                | Kind::Deletion
                | Kind::Skip
                | Kind::SequenceMatch
                | Kind::SequenceMismatch => Some(op.len()),
                _ => None,
            })
            .sum()
    }

    fn read_span(&self) -> usize {
        self.as_ref()
            .iter()
            .filter_map(|op| match op.kind() {
                Kind::Match
                | Kind::Insertion
                | Kind::SoftClip
                | Kind::SequenceMatch
                | Kind::SequenceMismatch => Some(op.len()),
                _ => None,
            })
            .sum()
    }

    fn reference_end(&self, start: usize) -> usize {
        // 1-based inclusive end. For an alignment starting at `start` and
        // covering `span` reference bases, the end is `start + span - 1`.
        let span = self.reference_span();
        if span == 0 { start } else { start + span - 1 }
    }

    fn aligned_positions(&self) -> AlignedPositions<'_> {
        AlignedPositions::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noodles_sam::alignment::record::cigar::Op;

    fn cigar_from_ops(ops: &[(Kind, usize)]) -> Cigar {
        Cigar::from(ops.iter().map(|(k, n)| Op::new(*k, *n)).collect::<Vec<_>>())
    }

    #[test]
    fn reference_span_simple_match() {
        let c = cigar_from_ops(&[(Kind::Match, 100)]);
        assert_eq!(c.reference_span(), 100);
    }

    #[test]
    fn reference_span_with_indels() {
        // 50M 5I 50M 5D 50M → ref consumes 50 + 0 + 50 + 5 + 50 = 155
        let c = cigar_from_ops(&[
            (Kind::Match, 50),
            (Kind::Insertion, 5),
            (Kind::Match, 50),
            (Kind::Deletion, 5),
            (Kind::Match, 50),
        ]);
        assert_eq!(c.reference_span(), 155);
    }

    #[test]
    fn reference_span_with_soft_clips_only_match_counts() {
        // 5S 100M 5S → ref consumes only 100
        let c = cigar_from_ops(&[(Kind::SoftClip, 5), (Kind::Match, 100), (Kind::SoftClip, 5)]);
        assert_eq!(c.reference_span(), 100);
    }

    #[test]
    fn reference_span_with_ref_skip() {
        // 50M 1000N 50M → ref consumes 50 + 1000 + 50 = 1100
        let c = cigar_from_ops(&[(Kind::Match, 50), (Kind::Skip, 1000), (Kind::Match, 50)]);
        assert_eq!(c.reference_span(), 1100);
    }

    #[test]
    fn reference_span_empty_cigar_is_zero() {
        let c = cigar_from_ops(&[]);
        assert_eq!(c.reference_span(), 0);
    }

    #[test]
    fn read_span_simple_match() {
        let c = cigar_from_ops(&[(Kind::Match, 100)]);
        assert_eq!(c.read_span(), 100);
    }

    #[test]
    fn read_span_with_indels() {
        // 50M 5I 50M 5D 50M → read consumes 50 + 5 + 50 + 0 + 50 = 155
        let c = cigar_from_ops(&[
            (Kind::Match, 50),
            (Kind::Insertion, 5),
            (Kind::Match, 50),
            (Kind::Deletion, 5),
            (Kind::Match, 50),
        ]);
        assert_eq!(c.read_span(), 155);
    }

    #[test]
    fn read_span_includes_soft_clips() {
        // 5S 100M 5S → read consumes 5 + 100 + 5 = 110
        let c = cigar_from_ops(&[(Kind::SoftClip, 5), (Kind::Match, 100), (Kind::SoftClip, 5)]);
        assert_eq!(c.read_span(), 110);
    }

    #[test]
    fn reference_end_inclusive_1based() {
        // Start at 100, span 50 → end at 149 (positions 100..149 inclusive = 50 bases)
        let c = cigar_from_ops(&[(Kind::Match, 50)]);
        assert_eq!(c.reference_end(100), 149);
    }

    #[test]
    fn reference_end_with_empty_cigar_returns_start() {
        let c = cigar_from_ops(&[]);
        assert_eq!(c.reference_end(100), 100);
    }

    #[test]
    fn reference_end_does_not_underflow_on_zero_start() {
        // Defensive: even with span=0 and start=0 we don't panic.
        let c = cigar_from_ops(&[]);
        assert_eq!(c.reference_end(0), 0);
    }

    #[test]
    fn aligned_positions_simple_match() {
        let c = cigar_from_ops(&[(Kind::Match, 3)]);
        let positions: Vec<_> = c.aligned_positions().collect();
        assert_eq!(positions.len(), 3);
        assert_eq!(
            positions[0],
            AlignedPosition {
                read_pos: 0,
                ref_offset: Some(0),
                op_kind: Kind::Match
            }
        );
        assert_eq!(
            positions[1],
            AlignedPosition {
                read_pos: 1,
                ref_offset: Some(1),
                op_kind: Kind::Match
            }
        );
        assert_eq!(
            positions[2],
            AlignedPosition {
                read_pos: 2,
                ref_offset: Some(2),
                op_kind: Kind::Match
            }
        );
    }

    #[test]
    fn aligned_positions_insertion_has_no_ref_offset() {
        // 2M 1I 2M: read positions 0,1 → ref 0,1; pos 2 → I (no ref); pos 3,4 → ref 2,3
        let c = cigar_from_ops(&[(Kind::Match, 2), (Kind::Insertion, 1), (Kind::Match, 2)]);
        let positions: Vec<_> = c.aligned_positions().collect();
        assert_eq!(positions.len(), 5);
        assert_eq!(positions[0].ref_offset, Some(0));
        assert_eq!(positions[1].ref_offset, Some(1));
        assert_eq!(positions[2].ref_offset, None);
        assert_eq!(positions[2].op_kind, Kind::Insertion);
        assert_eq!(positions[3].ref_offset, Some(2));
        assert_eq!(positions[4].ref_offset, Some(3));
    }

    #[test]
    fn aligned_positions_deletion_skipped_ref_advances() {
        // 2M 1D 2M: 4 read positions; ref offsets 0, 1, 3, 4
        let c = cigar_from_ops(&[(Kind::Match, 2), (Kind::Deletion, 1), (Kind::Match, 2)]);
        let positions: Vec<_> = c.aligned_positions().collect();
        assert_eq!(positions.len(), 4);
        assert_eq!(positions[0].ref_offset, Some(0));
        assert_eq!(positions[1].ref_offset, Some(1));
        assert_eq!(positions[2].ref_offset, Some(3));
        assert_eq!(positions[3].ref_offset, Some(4));
    }

    #[test]
    fn aligned_positions_soft_clip_has_no_ref_offset() {
        // 2S 2M: read pos 0 → S (no ref), 1 → S (no ref), 2 → ref 0, 3 → ref 1
        let c = cigar_from_ops(&[(Kind::SoftClip, 2), (Kind::Match, 2)]);
        let positions: Vec<_> = c.aligned_positions().collect();
        assert_eq!(positions.len(), 4);
        assert_eq!(positions[0].ref_offset, None);
        assert_eq!(positions[0].op_kind, Kind::SoftClip);
        assert_eq!(positions[1].ref_offset, None);
        assert_eq!(positions[2].ref_offset, Some(0));
        assert_eq!(positions[3].ref_offset, Some(1));
    }

    #[test]
    fn aligned_positions_hard_clip_skipped() {
        // 2H 2M: hard-clip emits nothing; 2 items for the match
        let c = cigar_from_ops(&[(Kind::HardClip, 2), (Kind::Match, 2)]);
        let positions: Vec<_> = c.aligned_positions().collect();
        assert_eq!(positions.len(), 2);
        assert_eq!(positions[0].read_pos, 0);
        assert_eq!(positions[0].ref_offset, Some(0));
    }

    #[test]
    fn aligned_positions_empty_cigar() {
        let c = cigar_from_ops(&[]);
        assert_eq!(c.aligned_positions().count(), 0);
    }

    // ─── #879 — CIGAR-trim primitive + ref-boundary helpers ──────────────
    //
    // These tests guard `trim_3p_read_positions` and the two thin caller
    // helpers (`reference_end_after_3p_trim`, `reference_start_after_3p_trim`)
    // that drop_overlap uses to mirror Perl's `--ignore_3prime` CIGAR
    // adjustment at bismark_methylation_extractor:1726-1782. See plan
    // file plans/05262026_bismark-extractor/BUG_879_FIXES_PLAN.md for
    // context + the two rounds of dual plan-review.

    // Helper: round-trip a Cigar through a Vec<(Kind, usize)> shape so test
    // assertions stay readable.
    fn ops_of(cigar: &Cigar) -> Vec<(Kind, usize)> {
        cigar.as_ref().iter().map(|op| (op.kind(), op.len())).collect()
    }

    #[test]
    fn trim_3p_zero_is_identity_right() {
        let c = cigar_from_ops(&[(Kind::Match, 100)]);
        let trimmed = c.trim_3p_read_positions(0, false);
        assert_eq!(ops_of(&trimmed), ops_of(&c));
    }

    #[test]
    fn trim_3p_zero_is_identity_left() {
        let c = cigar_from_ops(&[(Kind::Match, 100)]);
        let trimmed = c.trim_3p_read_positions(0, true);
        assert_eq!(ops_of(&trimmed), ops_of(&c));
    }

    #[test]
    fn trim_3p_simple_match_right() {
        // 100M trim 5 from right → 95M
        let c = cigar_from_ops(&[(Kind::Match, 100)]);
        let trimmed = c.trim_3p_read_positions(5, false);
        assert_eq!(ops_of(&trimmed), vec![(Kind::Match, 95)]);
    }

    #[test]
    fn trim_3p_simple_match_left() {
        // 100M trim 5 from left → 95M
        let c = cigar_from_ops(&[(Kind::Match, 100)]);
        let trimmed = c.trim_3p_read_positions(5, true);
        assert_eq!(ops_of(&trimmed), vec![(Kind::Match, 95)]);
    }

    #[test]
    fn trim_3p_with_trailing_deletion_strips_D() {
        // C1 regression guard: 90M5D, trim 5 from right.
        // The 5 read-positions of M are removed PLUS the now-trailing 5D is
        // stripped per Perl L1760-1764 `while ($op eq 'D')` loop.
        // Result: 85M.
        let c = cigar_from_ops(&[(Kind::Match, 90), (Kind::Deletion, 5)]);
        let trimmed = c.trim_3p_read_positions(5, false);
        assert_eq!(ops_of(&trimmed), vec![(Kind::Match, 85)]);
    }

    #[test]
    fn trim_3p_with_trailing_skip_strips_N() {
        // C1 regression guard: 90M5N, trim 5 from right.
        // Same as the trailing-D case but with Skip (N) ops.
        let c = cigar_from_ops(&[(Kind::Match, 90), (Kind::Skip, 5)]);
        let trimmed = c.trim_3p_read_positions(5, false);
        assert_eq!(ops_of(&trimmed), vec![(Kind::Match, 85)]);
    }

    #[test]
    fn trim_3p_with_leading_deletion_strips_D_when_from_left() {
        // C3 regression guard: 5D90M, trim 5 from left.
        // The 5 read-positions of M (from the front of the M block) are
        // removed PLUS the now-leading 5D is stripped (Perl reverse-strand
        // `shift @comp_cigar` + adjacent-D strip).
        // Result: 85M.
        let c = cigar_from_ops(&[(Kind::Deletion, 5), (Kind::Match, 90)]);
        let trimmed = c.trim_3p_read_positions(5, true);
        assert_eq!(ops_of(&trimmed), vec![(Kind::Match, 85)]);
    }

    #[test]
    fn trim_3p_clipping_into_insertion_no_ref_impact() {
        // 95M5I trim 5 from right → 95M (the 5 read positions of I are
        // removed; I doesn't consume ref so reference_end is unchanged).
        let c = cigar_from_ops(&[(Kind::Match, 95), (Kind::Insertion, 5)]);
        let trimmed = c.trim_3p_read_positions(5, false);
        assert_eq!(ops_of(&trimmed), vec![(Kind::Match, 95)]);
    }

    #[test]
    fn trim_3p_full_clip_returns_empty_cigar() {
        // 100M trim 100 from right → empty Cigar.
        let c = cigar_from_ops(&[(Kind::Match, 100)]);
        let trimmed = c.trim_3p_read_positions(100, false);
        assert_eq!(ops_of(&trimmed), vec![]);
    }

    #[test]
    fn trim_3p_middle_D_is_NOT_stripped() {
        // Reviewer A R2 negative-regression guard: 90M5D5M, trim 5 from right.
        // The 5M at the trailing end clips. The 5D in the MIDDLE must NOT be
        // stripped — only adjacent-to-trimmed-boundary D/N ops are stripped
        // per Perl L1760-1764. Result: 90M5D.
        let c = cigar_from_ops(&[(Kind::Match, 90), (Kind::Deletion, 5), (Kind::Match, 5)]);
        let trimmed = c.trim_3p_read_positions(5, false);
        assert_eq!(ops_of(&trimmed), vec![(Kind::Match, 90), (Kind::Deletion, 5)]);
    }

    #[test]
    fn trim_3p_left_with_soft_clip_prefix() {
        // Reviewer B R2 OB-soft-clip guard: 5S95M, trim 5 from left.
        // The 5 read-positions of S are removed. S doesn't consume ref so
        // the trimmed CIGAR's ref_span (95) == original ref_span (95);
        // reference_start_after_3p_trim(start, 5) therefore returns start
        // unchanged. This is the OB R1 BAM-5'-with-soft-clip edge case.
        let c = cigar_from_ops(&[(Kind::SoftClip, 5), (Kind::Match, 95)]);
        let trimmed = c.trim_3p_read_positions(5, true);
        assert_eq!(ops_of(&trimmed), vec![(Kind::Match, 95)]);
    }

    #[test]
    fn reference_end_after_3p_trim_zero_is_existing_reference_end() {
        // With n=0, the helper returns the same value as the existing
        // reference_end. Regression guard for default-cell behavior.
        let c = cigar_from_ops(&[
            (Kind::Match, 50),
            (Kind::Deletion, 5),
            (Kind::Match, 50),
        ]);
        assert_eq!(c.reference_end_after_3p_trim(100, 0), c.reference_end(100));
    }

    #[test]
    fn reference_end_after_3p_trim_simple() {
        // 100M from start=100, n=5: trimmed to 95M, reference_end = 194
        // (vs un-clipped 199).
        let c = cigar_from_ops(&[(Kind::Match, 100)]);
        assert_eq!(c.reference_end_after_3p_trim(100, 5), 194);
    }

    #[test]
    fn reference_start_after_3p_trim_zero_is_start() {
        // With n=0, no shift applied; helper returns start unchanged.
        let c = cigar_from_ops(&[(Kind::Match, 100)]);
        assert_eq!(c.reference_start_after_3p_trim(100, 0), 100);
    }

    #[test]
    fn reference_start_after_3p_trim_simple() {
        // 100M, start=100, n=5 from left: 5 ref positions consumed from
        // prefix → start shifts to 105.
        let c = cigar_from_ops(&[(Kind::Match, 100)]);
        assert_eq!(c.reference_start_after_3p_trim(100, 5), 105);
    }

    #[test]
    fn reference_start_after_3p_trim_with_leading_D() {
        // C3 regression guard for OB composite shift.
        // 5D90M, start=100, n=5 from left:
        //   - 5 M clipped (read positions)
        //   - 5 D adjacent stripped (ref-consuming)
        //   → trimmed CIGAR is 85M with ref_span 85
        //   → shift = 95 (original) - 85 (trimmed) = 10
        //   → result = 100 + 10 = 110
        // Equivalent to Perl L1803 `$start += $ignore_3prime + $D_count
        // + $N_count - $I_count` = 5 + 5 + 0 - 0 = 10.
        let c = cigar_from_ops(&[(Kind::Deletion, 5), (Kind::Match, 90)]);
        assert_eq!(c.reference_start_after_3p_trim(100, 5), 110);
    }

    #[test]
    fn reference_end_after_3p_trim_full_clip_returns_start() {
        // C2 regression guard: full-clip returns `start` (matching the
        // existing reference_end_with_empty_cigar_returns_start convention
        // at cigar.rs:276-278). 100M, start=100, n=100 → returns 100.
        let c = cigar_from_ops(&[(Kind::Match, 100)]);
        assert_eq!(c.reference_end_after_3p_trim(100, 100), 100);
    }
}

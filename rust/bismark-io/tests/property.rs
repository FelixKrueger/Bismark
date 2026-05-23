//! Property tests via [`proptest`].
//!
//! These exercise the pure-function corners of `bismark-io` with random
//! inputs to surface edge cases that example-based tests might miss.

use bismark_io::{BismarkStrand, CigarExt};
use noodles_sam::alignment::record::cigar::Op;
use noodles_sam::alignment::record::cigar::op::Kind;
use noodles_sam::alignment::record_buf::Cigar;
use proptest::prelude::*;

/// All four valid XR/XG byte combinations.
fn valid_xr_xg_combinations() -> impl Strategy<Value = (&'static [u8], &'static [u8])> {
    prop_oneof![
        Just((b"CT" as &[u8], b"CT" as &[u8])),
        Just((b"GA" as &[u8], b"CT" as &[u8])),
        Just((b"CT" as &[u8], b"GA" as &[u8])),
        Just((b"GA" as &[u8], b"GA" as &[u8])),
    ]
}

proptest! {
    /// `BismarkStrand::from_xr_xg` is deterministic — repeated calls with
    /// the same inputs yield the same strand.
    #[test]
    fn strand_from_xr_xg_is_deterministic((xr, xg) in valid_xr_xg_combinations()) {
        let a = BismarkStrand::from_xr_xg(xr, xg).unwrap();
        let b = BismarkStrand::from_xr_xg(xr, xg).unwrap();
        prop_assert_eq!(a, b);
    }

    /// `BismarkStrand::as_str` is unique per variant — no two variants
    /// share a canonical string.
    #[test]
    fn strand_as_str_is_unique_per_variant(
        (xr1, xg1) in valid_xr_xg_combinations(),
        (xr2, xg2) in valid_xr_xg_combinations(),
    ) {
        let a = BismarkStrand::from_xr_xg(xr1, xg1).unwrap();
        let b = BismarkStrand::from_xr_xg(xr2, xg2).unwrap();
        if a == b {
            prop_assert_eq!(a.as_str(), b.as_str());
        } else {
            prop_assert_ne!(a.as_str(), b.as_str());
        }
    }

    /// Any non-`{CT,GA}/{CT,GA}` combination must error, never silently
    /// produce a strand.
    #[test]
    fn strand_from_xr_xg_rejects_invalid(
        xr in any::<Vec<u8>>(),
        xg in any::<Vec<u8>>(),
    ) {
        let valid = matches!(
            (xr.as_slice(), xg.as_slice()),
            (b"CT", b"CT") | (b"GA", b"CT") | (b"CT", b"GA") | (b"GA", b"GA"),
        );
        let result = BismarkStrand::from_xr_xg(&xr, &xg);
        if valid {
            prop_assert!(result.is_ok());
        } else {
            prop_assert!(result.is_err());
        }
    }
}

/// Generate a random non-zero usize for CIGAR op lengths. Keep lengths
/// small enough that `reference_span` + `read_span` sums don't overflow
/// in test scenarios.
fn op_len() -> impl Strategy<Value = usize> {
    1usize..=1000
}

/// Generate a random CIGAR op. Avoid the operations our `aligned_positions`
/// iterator treats as "consumed-by-neither" if not needed (H, P, B) by
/// keeping the proptest cigar minimal.
fn cigar_op() -> impl Strategy<Value = Op> {
    (
        prop_oneof![
            Just(Kind::Match),
            Just(Kind::Insertion),
            Just(Kind::Deletion),
            Just(Kind::Skip),
            Just(Kind::SoftClip),
            Just(Kind::SequenceMatch),
            Just(Kind::SequenceMismatch),
        ],
        op_len(),
    )
        .prop_map(|(kind, len)| Op::new(kind, len))
}

fn cigar_strategy() -> impl Strategy<Value = Cigar> {
    prop::collection::vec(cigar_op(), 0..=20).prop_map(Cigar::from)
}

proptest! {
    /// `reference_span` + `read_span` consistency:
    /// for any op, the sum (ref_consumed + read_consumed) for op-types
    /// that consume both equals 2*len (we count it in both sums); for
    /// ref-only ops we count in ref-span only; for read-only ops we
    /// count in read-span only. Specifically: total bytes accounted-for
    /// = sum over ops of (ref_consumed + read_consumed) — and this
    /// equals reference_span() + read_span().
    #[test]
    fn cigar_spans_account_for_all_consumed_bytes(cigar in cigar_strategy()) {
        let ref_span = cigar.reference_span();
        let read_span = cigar.read_span();

        // Independently compute the expected sums op-by-op.
        let (expected_ref, expected_read) = cigar.as_ref().iter().fold(
            (0usize, 0usize),
            |(r, q), op| {
                let len = op.len();
                let (dr, dq) = match op.kind() {
                    Kind::Match | Kind::SequenceMatch | Kind::SequenceMismatch => (len, len),
                    Kind::Insertion | Kind::SoftClip => (0, len),
                    Kind::Deletion | Kind::Skip => (len, 0),
                    Kind::HardClip | Kind::Pad => (0, 0),
                };
                (r + dr, q + dq)
            },
        );

        prop_assert_eq!(ref_span, expected_ref);
        prop_assert_eq!(read_span, expected_read);
    }

    /// `reference_end(start)` = `start + reference_span() - 1` for any
    /// non-empty CIGAR with non-zero start. Round-trip invariant.
    #[test]
    fn cigar_reference_end_consistent_with_span(
        cigar in cigar_strategy(),
        start in 1usize..=1_000_000,
    ) {
        let span = cigar.reference_span();
        let end = cigar.reference_end(start);
        if span == 0 {
            prop_assert_eq!(end, start);
        } else {
            prop_assert_eq!(end, start + span - 1);
        }
    }

    /// `aligned_positions().count()` equals `read_span()`: per the API
    /// contract, one item per read position consumed.
    #[test]
    fn cigar_aligned_positions_count_equals_read_span(cigar in cigar_strategy()) {
        let read_span = cigar.read_span();
        let count = cigar.aligned_positions().count();
        prop_assert_eq!(count, read_span);
    }
}

//! Faithful reproduction of Perl's `substr(EXPR, OFFSET, LEN)` rvalue
//! semantics (SPEC §9).
//!
//! The sole caller passing a possibly-negative offset is the reverse-read
//! genome-window extraction in Phase B (`substr(chr, last_end-3, …)` for a
//! reverse read whose rightmost coordinate is at chromosome position 1 or 2);
//! all other calls pass non-negative offsets. Reproducing Perl's
//! negative-offset-from-end + over-length-truncation behaviour is what makes
//! those chromosome-start reverse reads emit Perl's all-zero output line.

/// Perl `substr` rvalue semantics:
///
/// - a negative `offset` counts from the end (`start = len + offset`);
/// - an out-of-range `start` (`< 0` or `> L`) yields an empty slice (Perl
///   returns `undef`, whose `length` is 0 — downstream a `len < 3` guard skips);
/// - `start == L` yields an empty slice (Perl returns `""`), with **no panic**;
/// - otherwise the result is `min(len, L - start)` bytes starting at `start`.
#[must_use]
pub fn perl_substr(s: &[u8], offset: isize, len: usize) -> &[u8] {
    let l = s.len() as isize;
    let start = if offset >= 0 { offset } else { l + offset };
    if start < 0 || start > l {
        return &[];
    }
    let start = start as usize; // 0..=L
    let end = start.saturating_add(len).min(s.len());
    &s[start..end] // start == L → &s[L..L] == &[]
}

#[cfg(test)]
mod tests {
    use super::perl_substr;

    const S: &[u8] = b"ABCDEFGH"; // L = 8

    #[test]
    fn negative_offset_in_range_returns_tail() {
        assert_eq!(perl_substr(S, -3, 3), b"FGH");
    }

    #[test]
    fn negative_offset_beyond_len_is_empty() {
        assert_eq!(perl_substr(S, -20, 3), b"");
    }

    #[test]
    fn over_length_truncates() {
        assert_eq!(perl_substr(S, 6, 5), b"GH");
    }

    #[test]
    fn offset_past_end_is_empty() {
        assert_eq!(perl_substr(S, 20, 3), b"");
    }

    #[test]
    fn offset_equals_len_is_empty_no_panic() {
        // SPEC §9 / P1: Perl `substr("ABCDEFGH", 8, 3)` returns "" (defined),
        // length 0. The Rust slice `&s[8..8]` must be empty, not panic.
        assert_eq!(perl_substr(S, 8, 3), b"");
    }

    #[test]
    fn interior_slice() {
        assert_eq!(perl_substr(S, 2, 3), b"CDE");
    }

    #[test]
    fn zero_len_returns_empty() {
        assert_eq!(perl_substr(S, 2, 0), b"");
    }

    #[test]
    fn negative_offset_exactly_at_start() {
        // offset == -L → start == 0.
        assert_eq!(perl_substr(S, -8, 2), b"AB");
    }
}

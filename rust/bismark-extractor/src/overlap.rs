//! Paired-end overlap detection (`--no_overlap`).
//!
//! Phase C (rev 1) — drops R2 calls overlapping R1's reference span.
//! Mirrors Perl `bismark_methylation_extractor` lines 2891-2906 (forward /
//! OT-CTOB) + 2976-2990 (reverse / OB-CTOT). Decision happens at the
//! **reference-position** level, accounting for InDels via
//! [`bismark_io::CigarExt::reference_end`].
//!
//! ## Polarity (SPEC §7.4 rev 2)
//!
//! Perl writes the *skip* predicate as `>=` (forward) and `<=` (reverse).
//! The Rust *keep* predicate is the inverse: strict `<` (forward) and
//! strict `>` (reverse). Both Phase C reviewers verified this against
//! Perl 2905 + 2989.

use bismark_io::{BismarkPair, BismarkStrand, CigarExt};

use crate::call::MethCall;
use crate::error::BismarkExtractorError;

/// Drop R2 calls overlapping R1's reference span. SPEC §7.4.
///
/// Pair-strand is recovered internally via [`BismarkPair::pair_strand`]
/// (Phase C rev 1 simplification per Reviewer B L6 — the explicit
/// `pair_strand` argument was redundant with `pair.pair_strand()`).
///
/// Uses [`Vec::retain`] rather than `into_iter().filter().collect()` to
/// avoid reallocating a new `Vec` when most R2 calls are kept (the common
/// case for partially-overlapping pairs).
///
/// # Errors
///
/// [`BismarkExtractorError::InternalError`] if R1 lacks an `alignment_start`
/// (filtered upstream by `bismark-io::open_reader`'s unmapped-filter, so this
/// should not fire in practice).
pub fn drop_overlap(
    mut r2_calls: Vec<MethCall>,
    pair: &BismarkPair,
) -> Result<Vec<MethCall>, BismarkExtractorError> {
    let r1_start =
        pair.r1()
            .alignment_start()
            .ok_or_else(|| BismarkExtractorError::InternalError {
                message: "R1 of PE pair missing alignment_start; bismark-io should have filtered \
                      this as unmapped (FLAG & 0x4)"
                    .to_string(),
            })?;

    if is_forward_pair_strand(pair.pair_strand()) {
        // OT/CTOB pair: R1 is upstream, R2 is downstream.
        // Perl 2905 skip predicate: `if r2_pos >= r1_ref_end { return; }`.
        // Keep predicate (strict inverse): `r2_pos < r1_ref_end`.
        let r1_ref_end = pair.r1().cigar().reference_end(r1_start) as u32;
        r2_calls.retain(|c| c.ref_pos < r1_ref_end);
    } else {
        // OB/CTOT pair: R2 is upstream, R1 is downstream.
        // Perl 2989 skip predicate: `if r2_pos <= r1_ref_start { return; }`.
        // Keep predicate (strict inverse): `r2_pos > r1_ref_start`.
        let r1_ref_start = r1_start as u32;
        r2_calls.retain(|c| c.ref_pos > r1_ref_start);
    }
    Ok(r2_calls)
}

/// Forward-class pair strands: R1's mapped position is the upstream end of
/// the paired insert. `OT` and `CTOB` are forward; `OB` and `CTOT` are
/// reverse.
///
/// Cites Perl `bismark_methylation_extractor:2400` (forward branch entry)
/// and line 2415 (reverse branch entry) — these are where R1's strand-tag
/// drives the per-pair direction selection in the Perl loop.
pub fn is_forward_pair_strand(strand: BismarkStrand) -> bool {
    matches!(strand, BismarkStrand::OT | BismarkStrand::CTOB)
}

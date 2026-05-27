//! Paired-end overlap detection (`--no_overlap`).
//!
//! Phase C.1 (closes #862) — drops R2 calls in the overlap region with R1's
//! reference span, keeping R2's unique-region calls. Mirrors Perl
//! `bismark_methylation_extractor` lines 3825-3828 (OT/CTOB R2, strand='-'
//! branch in the default 4-context strand-specific output) and 3744-3747
//! (OB/CTOT R2, strand='+' branch in the same section). Decision happens at
//! the **reference-position** level, accounting for InDels via
//! [`bismark_io::CigarExt::reference_end`].
//!
//! ## Polarity (SPEC §7.4 rev 3)
//!
//! Perl pre-mutates the start positions at lines 2401 (OT: `$start_read_2 +=
//! $MDN_count_2 - 1` → R2's rightmost) and 2415-2416 (OB: `$end_read_1 =
//! $start_read_1` BEFORE `$start_read_1 += $MDN_count_1 - 1`). The R2
//! iteration then walks from R2's unique region INTO R1's territory and
//! `return`s the moment iteration enters the overlap.
//!
//! Perl's drop predicates (post-transformation):
//! - OT/CTOB R2 (line 3826): `r2_pos <= r1_ref_end`. Keep is **strict `>`**.
//! - OB/CTOT R2 (line 3745): `r2_pos >= r1_ref_start`. Keep is **strict `<`**.
//!
//! The Rust *keep* predicate is the strict inverse of Perl's inclusive *drop*
//! predicate. Rev 2's polarity-reversed predicate (`r2_pos < r1_ref_end` for
//! OT) was the result of overlooking the coordinate pre-mutation; it kept the
//! overlap region (still double-counting!) and dropped R2's unique region
//! (losing data). Surfaced by the Phase H harness on 10M PE WGBS (1.87×
//! call-count gap vs Perl).
//!
//! The same predicates appear byte-identical in three other Perl branches
//! (`--comprehensive` at 3576/3657; `--merge_non_CpG` at 2905/2987;
//! `--comprehensive --merge_non_CpG` near 4065). Default-branch citations are
//! load-bearing for documentation; the polarity fix applies regardless.
//!
//! ## Monotonicity (`Vec::retain` ≡ Perl early-return)
//!
//! Perl's iteration is early-return; Rust's `Vec::retain` is set-based.
//! These produce the same set because R2's `ref_pos` is monotonic in
//! iteration order (descending for OT R2 via `$start - $index`; ascending
//! for OB R2 via `$start + $index`). Inherited from SAM CIGAR semantics.

use bismark_io::{BismarkPair, BismarkStrand, CigarExt};

use crate::call::MethCall;
use crate::error::BismarkExtractorError;

/// Drop R2 calls in the overlap region with R1's reference span. SPEC §7.4 rev 3.
///
/// Keeps R2 calls in R2's UNIQUE region (past R1 for OT/CTOB, before R1 for
/// OB/CTOT). Drops R2 calls in the overlap region (positions also covered by
/// R1) to prevent double-counting in the methylation summary — the documented
/// semantics of `--no_overlap` ("only methylation calls of read 1 are kept
/// for overlapping regions", per Perl POD at line 5860+).
///
/// Mirrors Perl `bismark_methylation_extractor` lines 3825-3828 (OT/CTOB R2,
/// strand='-' branch in the default 4-context strand-specific output) and
/// 3744-3747 (OB/CTOT R2, strand='+' branch in the same section). The
/// predicates are byte-identical to the corresponding predicates in three
/// other Perl branches (`--comprehensive` at 3576/3657; `--merge_non_CpG` at
/// 2905/2987; `--comprehensive --merge_non_CpG` around 4065); the citations
/// above point at the active code path for the default harness invocation.
///
/// Accounts for Perl's pre-mutation of `$start_read_2` / `$start_read_1` at
/// lines 2398-2402 and 2414-2416 respectively (the iteration direction is
/// "from R2's unique region into R1's territory until the boundary is
/// crossed, then early-return"). See SPEC §7.4 rev 3 for the full derivation.
///
/// Pair-strand is recovered internally via [`BismarkPair::pair_strand`]
/// (Phase C rev 1 simplification per Reviewer B L6 — the explicit
/// `pair_strand` argument was redundant with `pair.pair_strand()`).
///
/// Uses [`Vec::retain`] rather than `into_iter().filter().collect()` to
/// avoid reallocating a new `Vec` when most R2 calls are kept (the common
/// post-fix case for partially-overlapping pairs with R2 extending past R1).
/// Equivalence with Perl's early-return iteration is preserved by R2's
/// `ref_pos` monotonicity (descending for OT R2, ascending for OB R2).
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
        // Perl 3826 drop predicate (post-transformation): `if r2_pos <= r1_ref_end { return; }`.
        // Keep predicate (strict inverse): `r2_pos > r1_ref_end`.
        // (Pre-C.1 had the polarity reversed — see SPEC §7.4 rev 3.)
        let r1_ref_end = pair.r1().cigar().reference_end(r1_start) as u32;
        r2_calls.retain(|c| c.ref_pos > r1_ref_end);
    } else {
        // OB/CTOT pair: R2 is upstream, R1 is downstream.
        // Perl 3745 drop predicate (post-transformation): `if r2_pos >= r1_ref_start { return; }`.
        // Keep predicate (strict inverse): `r2_pos < r1_ref_start`.
        // (Pre-C.1 had the polarity reversed — see SPEC §7.4 rev 3.)
        let r1_ref_start = r1_start as u32;
        r2_calls.retain(|c| c.ref_pos < r1_ref_start);
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

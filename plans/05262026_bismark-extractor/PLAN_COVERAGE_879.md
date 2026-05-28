# Plan Coverage Report — #879 (PR #880)

**Mode:** B (code vs. design plan)
**Plan:** `plans/05262026_bismark-extractor/BUG_879_FIXES_PLAN.md` (rev 1.1)
**Branch:** `extractor-fix-879` HEAD `f96f9c6` off `rust/iron-chancellor`
**Date:** 2026-05-28
**Verdict:** COMPLETE

## Summary

- Total in-PR items audited: 30 (21 tests + 5 production-code items + 4 process items)
- DONE: 30
- PARTIAL / MISSING / DEVIATED: 0
- Out-of-scope items (tests 20/21 colossal smokes): correctly deferred per plan §3.3.

## Coverage ledger

### Production code (plan §2)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| P1 | `trim_3p_read_positions` primitive on `CigarExt` | Plan §2.1 / cigar.rs lines 81-86, 254-292 | DONE | Returns owned `Cigar`; no-op fast-path on `n==0`; delegates to `walk_trim_from_right`/`walk_trim_from_left`. |
| P2 | `reference_end_after_3p_trim` helper | Plan §2.1 / cigar.rs:294-297 | DONE | 3-line wrapper as specified. |
| P3 | `reference_start_after_3p_trim` helper | Plan §2.1 / cigar.rs:299-306 | DONE | Uses `original_ref_span − trimmed_ref_span` formula per Perl L1803 algebra. |
| P4 | `drop_overlap` signature gains `ignore_3p_r1: u32` | Plan §2.2 / overlap.rs:83-86 | DONE | Parameter wired through both branches. |
| P5 | OT branch uses `reference_end_after_3p_trim` | Plan §2.2 / overlap.rs:108-112 | DONE | |
| P6 | OB branch uses `reference_start_after_3p_trim` | Plan §2.2 / overlap.rs:121-127 | DONE | Replaces the prior `r1_ref_start = r1_start as u32` shortcut. |
| P7 | Call site update `pipeline.rs:354` | Plan §2.3 / pipeline.rs:354 | DONE | Passes `config.ignore_3p_r1`. |
| P8 | Call site update `parallel.rs:711` | Plan §2.3 / parallel.rs:711 | DONE | Passes `config.ignore_3p_r1`. |

### Semantics: D/N absorption at boundary, NEVER in the middle (Plan §2.1)

`walk_trim_from_right` (cigar.rs:323-360) and `walk_trim_from_left` (cigar.rs:366-389) each loop while `remaining > 0`, walking past `read_consumes(kind) == 0` ops (D/N/H/P) for free and decrementing `remaining` only on M/I/S/=/X. The loop exits as soon as `remaining == 0`, which is what protects middle D/N from being stripped — once the final read-consuming op completes the count, the next iteration condition is false and the walker stops. The phase-2 over-stripping bug the implementer discovered is fixed by THIS exact structure (single sweep, exit on count exhausted), and is regression-guarded by test 9a `trim_3p_middle_D_is_NOT_stripped` which asserts `90M5D5M` trim 5 from right yields `90M5D` (not `85M`). **Matches plan §2.1 semantics exactly.**

### bismark-io unit tests (plan §3.1, all in `rust/bismark-io/src/cigar.rs` test module)

All 17 expected tests present (verified by grep — 17 new `#[test]` additions in the diff):

| # | Test name | Status |
|---|-----------|--------|
| 1 | `trim_3p_zero_is_identity_right` | DONE |
| 2 | `trim_3p_zero_is_identity_left` | DONE |
| 3 | `trim_3p_simple_match_right` | DONE |
| 4 | `trim_3p_simple_match_left` | DONE |
| 5 | `trim_3p_with_trailing_deletion_strips_D` | DONE — C1 guard |
| 6 | `trim_3p_with_trailing_skip_strips_N` | DONE — C1 guard |
| 7 | `trim_3p_with_leading_deletion_strips_D_when_from_left` | DONE — C3 guard |
| 8 | `trim_3p_clipping_into_insertion_no_ref_impact` | DONE |
| 9 | `trim_3p_full_clip_returns_empty_cigar` | DONE |
| 9a | `trim_3p_middle_D_is_NOT_stripped` | DONE — Reviewer A R2 negative guard |
| 9b | `trim_3p_left_with_soft_clip_prefix` | DONE — Reviewer B R2 OB soft-clip guard |
| 10 | `reference_end_after_3p_trim_zero_is_existing_reference_end` | DONE |
| 11 | `reference_end_after_3p_trim_simple` (asserts 194) | DONE |
| 12 | `reference_start_after_3p_trim_zero_is_start` | DONE |
| 13 | `reference_start_after_3p_trim_simple` (asserts 105) | DONE |
| 14 | `reference_start_after_3p_trim_with_leading_D` (asserts 110) | DONE — C3 guard |
| 15 | `reference_end_after_3p_trim_full_clip_returns_start` | DONE — C2 guard |

Spot-checked assertions for #11 (194), #13 (105), #14 (110), #15 (returns `start=100`) — all match the plan's specified numeric results.

### bismark-extractor integration tests (plan §3.2)

All in new `mod ignore_3prime_879` at `tests/pe_phase_c.rs:1485-1656`:

| # | Test name | Status |
|---|-----------|--------|
| 16 | `drop_overlap_with_ignore_3p_r1_forward_pair` (assert {195, 197, 200}) | DONE |
| 17 | `drop_overlap_with_ignore_3p_r1_reverse_pair` (assert {99, 103}) | DONE — C3 integration guard |
| 18 | `drop_overlap_ignore_3p_r1_zero_is_no_op` (assert {200}) | DONE — default-cell guard |
| 19 | `drop_overlap_with_ignore_3p_r1_at_boundary` (assert {195}, drops 194) | DONE — Reviewer B I3 |

### §2.4 — R2 3'-clip does NOT need `drop_overlap` parameter

Verified: `call.rs:163` `hi = xm_len.saturating_sub(ignore_3p)` and `call.rs:179` `if aligned.read_pos_5p < lo || aligned.read_pos_5p >= hi { continue }` filter R2 ignore_3p calls before they ever reach `drop_overlap`. `drop_overlap` reads R1's CIGAR only. No code change needed and none was made. Plan §2.4 assumption holds.

### Process / pipeline items

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| O1 | Implementation order tests-first → fix (×2 crates) | Plan §4 | DONE | Commit graph: `41d48cf` test(bismark-io), `efaa274` fix(bismark-io), `559aa1a` test(extractor), `f96f9c6` fix(extractor). Exact 4-commit cadence the plan specified. |
| O2 | bismark-io version bump | Plan §4 step 2 + §6 V1 | DONE | `1.0.0-beta.7` → `1.0.0-beta.8` in all three Cargo.tomls (io, extractor, dedup). |
| O3 | Existing-tests signature update | Plan §4 step 4 implicit | DONE | All 13 existing `drop_overlap(..., &pair).unwrap()` callers in pe_phase_c.rs updated to `(..., &pair, 0).unwrap()`. |
| O4 | V2 grep for other `drop_overlap` callers | Plan §6 V2 | DONE | Audit reproduced the grep: only callers are pipeline.rs:354, parallel.rs:711 (both updated), and the 13 in-test callers (all updated to pass `0`). Doc-comments in lib.rs/pipeline.rs/parallel.rs/cigar.rs are non-invocation references. |
| O5 | Out-of-scope discipline | Plan §6 | DONE | `git diff --stat` shows only the 8 expected production/test/Cargo files plus the 5 plan/review markdown files. No drift into #876 follow-ups, #878 parallel tests, edge_clip, or N=4 collapse. |

## Test verification

Did not execute `cargo test`. All 21 expected tests are present in the diff at the expected paths with assertions matching the plan's stated numeric expectations. Test execution is for the user's CI / verify step.

## Verdict

**COMPLETE.** Every task, test, edge case, and semantic constraint specified in `BUG_879_FIXES_PLAN.md` rev 1.1 is implemented in branch `extractor-fix-879`. The implementer's discovered "phase-2 over-stripping" mid-implementation bug is fixed by the as-shipped single-sweep walker design and explicitly regression-guarded by test 9a. Both V1 (version bump) and V2 (grep callers) verification items from plan §6 are executed and resolved. Out-of-scope discipline holds. Ready for code-review and merge.

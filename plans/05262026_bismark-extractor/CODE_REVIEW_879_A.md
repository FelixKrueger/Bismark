# Code Review A — PR #880 (`extractor-fix-879`)

Reviewer: A (independent, fresh context)
Scope: 4-commit diff vs `rust/iron-chancellor`; 8 files, +605/-19.

## Summary

Verdict: **APPROVE with one Medium finding and minor Low cleanups.** The CIGAR-trim primitive is correct in all hand-traced cases (including the round-2 `5D90M5D5M` middle-D fixture), Perl L1726-1782 semantics are faithfully mirrored, both production call sites and all 12 existing test call sites are updated, `cargo test --no-fail-fast` reports 0 failures across the workspace (178 bismark-io tests, all extractor suites), and the version bumps + dep updates are consistent (only bismark-extractor + bismark-dedup depend on bismark-io). The single-sweep "walk past D/N for free" formulation in `walk_trim_from_{left,right}` is a clean, correct simplification of the Perl per-base loop and the regression for the "phase 2 over-stripped middle D" gotcha is locked down by the named test.

## Findings by area

### Logic
- Hand-traced `90M5D` n=1, n=5, n=7; `90M5D5M` n=5; `5D90M5D5M` n=5; `5M5D5M` n=5/n=6; `5I90M` n=5; `5D5I90M` n=5; `5I5D90M` n=5; `5S95M` n=5 — every case matches Perl's `for (1..$ignore_3prime) { pop; while D; while N; if I I_count++ }` semantics. The "inner D/N loop only fires when outer pop yielded D/N" subtlety is correctly preserved.
- `reference_start_after_3p_trim`'s `original_ref_span - trimmed_ref_span` formulation is algebraically equivalent to Perl L1803's `$ignore_3prime + $D_count + $N_count - $I_count` (verified against `5D5I90M` n=5 where I and D interact: both give shift=5).
- `reference_end_after_3p_trim` correctly delegates to `reference_end` which already has the `span == 0 → start` empty-CIGAR convention (cigar.rs:248), so the full-clip case returns `start` as documented.
- Both production call sites (`pipeline.rs:354`, `parallel.rs:711`) pass `config.ignore_3p_r1`. `grep` shows no other callers in src/.

### Efficiency
- `n == 0` fast-path in `trim_3p_read_positions` (cigar.rs:259) returns a fresh `Cigar::from(self.as_ref().to_vec())` — an allocation per default-cell record. The docstring acknowledges this but it is still a per-record `Vec` clone on the hot path. Since `drop_overlap` only consumes `ref_span` / `reference_end`, the wrapper helpers `reference_end_after_3p_trim` / `reference_start_after_3p_trim` could short-circuit *before* calling `trim_3p_read_positions` when `n_read_positions == 0`. `reference_start_after_3p_trim` already has the `n == 0 → start` short-circuit (cigar.rs:301), but `reference_end_after_3p_trim` (cigar.rs:295-298) does not — it always calls `trim_3p_read_positions(0, false)` then `.reference_end(start)`, paying for a CIGAR clone on every default-cell forward pair. **See Medium recommendation below.**

### Errors
- `trim_3p_read_positions` can return a CIGAR with a *leading D* (e.g., `5I5D90M` n=5 → `[5D, 90M]`). That is technically an invalid SAM CIGAR. Current call sites only consume `reference_span` / `reference_end` from the result, so correctness is preserved, but the public `CigarExt::trim_3p_read_positions` could be misused in future. Docstring does not warn. (Low.)
- No new panics. `usize` arithmetic in `reference_start_after_3p_trim` uses unchecked subtraction `original_ref_span - trimmed_ref_span`; trimmed span ≤ original span by construction so it cannot underflow. (Defensive: a `saturating_sub` would document intent for free.)

### Structure
- API naming: `reference_end_after_3p_trim` / `reference_start_after_3p_trim` parallel the existing `reference_end` / `reference_span` shape — good fit.
- The `from_left: bool` boolean argument on `trim_3p_read_positions` is slightly hard to read at call sites (e.g., `trim_3p_read_positions(n, false)`). A `TrimSide::{Left, Right}` enum would be more legible — but only 1 production-equivalent call exists (both helpers internal), so this is Low priority.
- 17 unit tests + 4 integration tests = 21 total, all 21 present in diff (including round-2 `trim_3p_middle_D_is_NOT_stripped` at cigar.rs:692 and `trim_3p_left_with_soft_clip_prefix` at cigar.rs:716). Plan §3 coverage is complete.

## Recommendations

| Priority | File:Line | Suggestion |
|---|---|---|
| Medium | cigar.rs:295-298 | Short-circuit `reference_end_after_3p_trim` when `n_read_positions == 0` to avoid per-record `Cigar` clone on the default cell. Mirrors the existing pattern in `reference_start_after_3p_trim` (cigar.rs:301). |
| Low | cigar.rs:53-66 | Docstring for `trim_3p_read_positions` should note that the returned CIGAR may begin with `D/N` when the trimmed prefix contains insertions but the next op is D/N (e.g., `5I5D90M` n=5 → `5D90M`). Recommend callers consume only `reference_span` / `reference_end` from the result, not re-emit it. |
| Low | cigar.rs:306 | `start + (original_ref_span - trimmed_ref_span)`: consider `saturating_sub` for self-documentation (cannot underflow today, but cheap insurance). |
| Low | cigar.rs (API) | Consider `TrimSide::{Left, Right}` enum vs `from_left: bool` in a future revision — purely ergonomic. |

Report path: `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/CODE_REVIEW_879_A.md`

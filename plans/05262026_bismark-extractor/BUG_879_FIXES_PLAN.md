# Plan — fix #879 (PE byte-identity FAIL with `--ignore_3prime` on R1)

**Status**: rev 1 — dual-reviewer findings absorbed, awaiting re-review (Felix-approved revision, 2026-05-28)
**Scope**: closes #879 by fixing `drop_overlap`'s R1-boundary calculation to mirror Perl's CIGAR-adjustment behavior under `--ignore_3prime`
**Workflow stage**: Plan (per CLAUDE.md mandatory plan → manual review → agent review → implement)
**Branch**: new branch `extractor-fix-879` off `rust/iron-chancellor` HEAD `45b4c61` (post-#876)

## Revision history

- **rev 0**: proposed two strand-specific CigarExt methods (`reference_end_clipping_right` + `reference_start_clipping_left`) that walk CIGAR from one end consuming read positions, return the boundary.
- **rev 1** (this version): both plan-reviewers independently raised THREE Critical correctness gaps in rev 0:
  - **C1 (both)** — rev 0 missed Perl's **trailing D/N strip** at L1760-1764. After the read-position pop loop completes, Perl strips any trailing D/N ops WITHOUT decrementing the clip counter. For CIGAR `90M5D` with `--ignore_3prime 5`: Perl removes 5M + 5D → ref_end shifts by 10 ref positions; rev 0 would only shift by 5.
  - **C2 (both)** — rev 0's full-clip return value `start.saturating_sub(1)` is inconsistent with existing `reference_end_with_empty_cigar_returns_start` convention at `cigar.rs:276-278` (returns `start`).
  - **C3 (Reviewer B unique)** — rev 0's OB-strand handling didn't explicitly account for Perl's L1803 composite shift `$start += $ignore_3prime + $D_count + $N_count - $I_count`. The single-helper formulation might not capture both contributions correctly without a CIGAR-trim primitive.
- Both reviewers' convergent **Important** recommendation: pivot the API to a **CIGAR-trim primitive** that returns a trimmed `Cigar` object, then reuse the existing `reference_end()` / `reference_span()` on it. This encapsulates D/N/I trailing-strip in one place; mirrors Perl's "rewrite the CIGAR then use existing end-calc" pattern at L1807-1828; sidesteps the directional/positional naming confusion in rev 0.
- Rev 1 adopts the CIGAR-trim primitive + two thin caller-facing helpers + adds 4 new tests covering trailing D/N + full-clip boundary + OB InDel-in-prefix case. Also cites `call.rs:179-182` for the §2.4 R2-3p-clip claim (Reviewer A I-citation).

## 1. Context

PE Phase H matrix on merged HEAD `45b4c61` FAILed at cell `r1r2_3p` (`--ignore_3prime 5 --ignore_3prime_r2 5`) with **all 8 files differing** — methylation data files (not just metadata). Rust emitted ~0.54% fewer total Cs than Perl (174,039,426 vs 174,982,585).

Isolation smokes:
- `--ignore_3prime 5` alone: **FAIL** 8/8 files
- `--ignore_3prime_r2 5` alone: **PASS** 8/8 files

**Root cause** (located in `overlap.rs:101`, full audit trail at [#879 comment](https://github.com/FelixKrueger/Bismark/issues/879#issuecomment-4567351382)): `drop_overlap()` computes the R1 overlap boundary from R1's **un-clipped** CIGAR, while Perl `bismark_methylation_extractor:1726-1782` **adjusts the CIGAR** when `--ignore_3prime N` fires THEN recomputes `$end_read_1`.

Symmetry confirmation across all 4 ignore flags is in #879 and rev 0; unchanged in rev 1.

## 2. Fix shape — CIGAR-trim primitive + two helpers + drop_overlap update

The fix lives across two crates:
1. **bismark-io** — add one CIGAR-trim primitive + two reference-boundary helpers to `CigarExt`
2. **bismark-extractor** — wire those into `drop_overlap` and pass `ignore_3p_r1` through

### 2.1 Bismark-io: CIGAR-trim primitive + 2 helpers

File: `rust/bismark-io/src/cigar.rs` (extend the existing `CigarExt` trait + `impl CigarExt for Cigar`)

**The primitive** (the actual CIGAR-trim logic, Perl-faithful):

```rust
/// Return a new `Cigar` with `n_read_positions` of read-consuming ops
/// trimmed from one end. After the read-position trim loop completes,
/// also strips any now-exposed trailing D/N ops (which consume reference
/// but no read positions) — mirrors Perl `bismark_methylation_extractor:
/// 1760-1764` `while ($op eq 'D' or eq 'N')` post-pop loops.
///
/// - `from_left=false` mirrors Perl `pop @comp_cigar` (forward-strand R1).
/// - `from_left=true` mirrors Perl `shift @comp_cigar` (reverse-strand R1).
///
/// Read-consuming ops: `M`, `I`, `S`, `=`, `X`. Ref-consuming ops:
/// `M`, `D`, `N`, `=`, `X`. `H` is ignored (consumes neither).
///
/// Edge cases:
/// - `n_read_positions == 0` returns the CIGAR unchanged (no-op fast-path).
/// - `n_read_positions >= total_read_consuming_positions` returns an
///   empty `Cigar` (degenerate "everything clipped").
fn trim_3p_read_positions(&self, n_read_positions: u32, from_left: bool) -> Cigar;
```

**The two caller-facing helpers** (thin wrappers, what `drop_overlap` will call):

```rust
/// Reference-end of the CIGAR's reference span AFTER trimming
/// `n_read_positions` from the read's 3' end via CIGAR's RIGHT side
/// (forward-strand R1 under `--ignore_3prime`). Returns the new
/// 1-based inclusive ref-end on the original `start` coordinate.
///
/// Implementation: `self.trim_3p_read_positions(n, false).reference_end(start)`.
/// Returns `start` when the trimmed CIGAR has zero reference span
/// (matching the existing `reference_end_with_empty_cigar_returns_start`
/// convention at `cigar.rs:276-278`).
fn reference_end_after_3p_trim(&self, start: usize, n_read_positions: u32) -> usize;

/// Reference-start AFTER trimming `n_read_positions` from the read's 3' end
/// via CIGAR's LEFT side (reverse-strand R1 under `--ignore_3prime`).
/// Returns the new 1-based ref position where the trimmed CIGAR begins
/// (the original `start` shifted right by the ref-positions consumed by
/// the trimmed-off prefix).
///
/// Implementation: trim from left, compute `start + (original_ref_span
/// - trimmed_ref_span)`. Equivalent to Perl L1803's composite shift
/// `$start += $ignore_3prime + $D_count + $N_count - $I_count` (where
/// the deltas are computed from the dropped prefix; D/N add to ref shift,
/// I subtracts; M is implicit in the ignore_3prime count).
fn reference_start_after_3p_trim(&self, start: usize, n_read_positions: u32) -> usize;
```

**Implementation notes**:
- The primitive's loop structure mirrors Perl L1756-1770: in the same iteration sweep, after the read-consuming op is popped, immediately strip any adjacent D/N ops at the trimmed boundary (Perl's `while ($op eq 'D' or eq 'N')` inner loops). This is a SINGLE sweep that combines read-position counting with trailing-D/N stripping — not two separate loops. **D/N ops in the MIDDLE of the CIGAR (not adjacent to the trim boundary) are NEVER stripped** — they're left untouched in the returned CIGAR. This fixes C1 (Round 1 critical: trailing-D/N strip missing) while NOT over-stripping middle ops.
- The two helpers are thin wrappers (3-5 lines each); the primitive does the actual work.
- The primitive returns a `Cigar` (owned), so the caller can chain `.reference_end(start)` or `.reference_span()` as needed. `Cigar` is internally a `Vec<Op>` (noodles-sam), so the allocation is small (typical CIGAR is <10 ops).

### 2.2 Bismark-extractor: drop_overlap signature + impl

File: `rust/bismark-extractor/src/overlap.rs` (lines 83-112)

**Proposed signature** (add `ignore_3p_r1` parameter):
```rust
pub fn drop_overlap(
    mut r2_calls: Vec<MethCall>,
    pair: &BismarkPair,
    ignore_3p_r1: u32,
) -> Result<Vec<MethCall>, BismarkExtractorError>
```

**Updated implementation** (replaces the inline ref calc at L101 + L108):
```rust
if is_forward_pair_strand(pair.pair_strand()) {
    // OT/CTOB pair: clip R1's 3' end = pop CIGAR RIGHT side (Perl L1729+).
    let r1_ref_end = pair.r1().cigar()
        .reference_end_after_3p_trim(r1_start, ignore_3p_r1) as u32;
    r2_calls.retain(|c| c.ref_pos > r1_ref_end);
} else {
    // OB/CTOT pair: clip R1's 3' end = shift CIGAR LEFT side (Perl L1781+).
    let r1_ref_start = pair.r1().cigar()
        .reference_start_after_3p_trim(r1_start, ignore_3p_r1) as u32;
    r2_calls.retain(|c| c.ref_pos < r1_ref_start);
}
```

When `ignore_3p_r1 == 0` (the common case), the helpers' no-op fast-path returns the same value as the existing `reference_end(start)` / `r1_start` — no perf regression on the default-cell path.

### 2.3 Call site updates

Two callers of `drop_overlap`:
- `rust/bismark-extractor/src/pipeline.rs:354` → `drop_overlap(r2_calls_raw, pair, config.ignore_3p_r1)?`
- `rust/bismark-extractor/src/parallel.rs:711` → `drop_overlap(r2_calls_raw, pair, config.ignore_3p_r1)?`

Both have `config: &ResolvedConfig` in scope already.

### 2.4 Why NOT also pass `ignore_3p_r2` (citations added per Reviewer A)

R2's 3'-clip is correctly handled by `extract_calls`'s position filter at **`call.rs:179-182`**:
```rust
for aligned in record.iter_aligned() {
    if aligned.read_pos_5p < lo || aligned.read_pos_5p >= hi {
        continue;
    }
    ...
}
```
where `hi = xm_len.saturating_sub(ignore_3p)`. R2 calls at the clipped read positions are FILTERED OUT before they reach `drop_overlap`. The R2 `MethCall` values that DO reach `drop_overlap` already have correct `ref_pos` (the reference coordinate is unaffected by which read-positions emit calls — only the SET of emitting positions changes).

`drop_overlap` reads R1's CIGAR (not R2's) to compute the R1 overlap boundary. R2's 3'-clip never affects R1's CIGAR. The iso_r2_3p PASS confirms this empirically: `--ignore_3prime_r2 5` alone produces byte-identical 8/8 output.

## 3. Test coverage (expanded from rev 0)

### 3.1 bismark-io unit tests (`rust/bismark-io/src/cigar.rs` test module)

**Primitive tests** (for `trim_3p_read_positions`):

1. **`trim_3p_zero_is_identity_right`**: `n_read_positions=0, from_left=false` → returned Cigar equals input.
2. **`trim_3p_zero_is_identity_left`**: `n_read_positions=0, from_left=true` → returned Cigar equals input.
3. **`trim_3p_simple_match_right`**: `100M`, trim 5 from right → returned Cigar is `95M`.
4. **`trim_3p_simple_match_left`**: `100M`, trim 5 from left → returned Cigar is `95M`.
5. **`trim_3p_with_trailing_deletion_strips_D` (C1 regression guard)**: `90M5D`, trim 5 from right → returned Cigar is `85M`. The 5 read-positions of M are removed PLUS the now-trailing 5D is stripped. Critical: validates the Perl L1760-1764 trailing-D loop.
6. **`trim_3p_with_trailing_skip_strips_N` (C1 regression guard)**: `90M5N`, trim 5 from right → returned Cigar is `85M`. Validates the Perl L1764 `while ($op eq 'N')` loop.
7. **`trim_3p_with_leading_deletion_strips_D_when_from_left` (C3 regression guard)**: `5D90M`, trim 5 from left → returned Cigar is `85M`. The 5 read-positions of M are removed from the front PLUS the now-leading 5D is stripped.
8. **`trim_3p_clipping_into_insertion_no_ref_impact`**: `95M5I`, trim 5 from right → returned Cigar is `95M` (the 5 read positions of I are removed). Existing `reference_end()` on `95M` returns `start + 95 - 1` = same as on original `95M5I` (because I doesn't consume ref). The reference_end is unchanged.
9. **`trim_3p_full_clip_returns_empty_cigar`**: `100M`, trim 100 → empty Cigar. Existing `reference_end_with_empty_cigar_returns_start` covers what happens next.
9a. **`trim_3p_middle_D_is_NOT_stripped` (Reviewer A R2 negative-regression guard)**: `90M5D5M`, trim 5 from right → returned Cigar is `90M5D` (NOT `85M`). Validates that the 5M at the trailing end clips, but the 5D in the MIDDLE remains. Guards against an over-aggressive strip loop that would walk past a non-boundary D.
9b. **`trim_3p_left_with_soft_clip_prefix` (Reviewer B R2 OB-soft-clip guard)**: `5S95M`, trim 5 from left → returned Cigar is `95M`. The 5 read-positions of S are removed from the front. Since S doesn't consume reference, `original_ref_span (95) − trimmed_ref_span (95) = 0`, so `reference_start_after_3p_trim(start, 5)` returns `start` unchanged. This is the OB R1 BAM-5'-end edge case where the read's sequenced 3' is the CIGAR's left and starts with a soft-clip — verified by Reviewer B as correct under spec.

**Helper tests** (for the wrappers):

10. **`reference_end_after_3p_trim_zero_is_existing_reference_end`**: with `n=0`, result equals existing `reference_end(start)`.
11. **`reference_end_after_3p_trim_simple`**: CIGAR `100M`, `start=100`, `n=5` → returns `start + 95 - 1 = 194` (was `199` un-clipped).
12. **`reference_start_after_3p_trim_zero_is_start`**: with `n=0`, returns `start`.
13. **`reference_start_after_3p_trim_simple`**: CIGAR `100M`, `start=100`, `n=5` → returns `start + 5 = 105` (reference-shifted right by the 5 ref-positions of trimmed prefix M).
14. **`reference_start_after_3p_trim_with_leading_D` (C3 regression guard for OB composite shift)**: `5D90M`, `start=100`, `n=5` → trimmed CIGAR is `85M`, shift = original `95` − trimmed `85` = `10` ref positions → `start + 10 = 110`. Validates Perl L1803's `$start += $ignore_3prime + $D_count` mechanism.
15. **`reference_end_after_3p_trim_full_clip_returns_start` (C2 regression guard)**: `100M`, `n=100` → trimmed empty Cigar, `reference_end` returns `start` (matches `cigar.rs:276-278` convention).

(17 unit tests in bismark-io. Each tightly scoped, each FAILable independently if its specific edge case regresses.)

### 3.2 bismark-extractor integration tests

16. **`drop_overlap_with_ignore_3p_r1_forward_pair`** (new test in a new `overlap.rs` test module): synthetic forward (OT) pair, R1 CIGAR `100M` start=100. R2 has calls at ref_pos 195, 197, 200. Without fix (un-clipped `r1_ref_end=199`): predicate `> 199` keeps only call at 200 (drops 195, 197). With fix (`ignore_3p_r1=5` → clipped `r1_ref_end=194`): predicate `> 194` keeps 195, 197, 200. Assert keep-set is `{195, 197, 200}`.
17. **`drop_overlap_with_ignore_3p_r1_reverse_pair`** (C3 regression guard): synthetic reverse (OB) pair, R1 CIGAR `100M` start=100 → `r1_ref_start=100` un-clipped, `=105` with `ignore_3p_r1=5`. R2 has calls at ref_pos 99, 103, 107. Without fix (predicate `< 100`): keeps only 99. With fix (predicate `< 105`): keeps 99, 103. Assert keep-set is `{99, 103}`.
18. **`drop_overlap_ignore_3p_r1_zero_is_no_op`**: regression guard — with `ignore_3p_r1=0`, behavior is byte-identical to pre-fix `drop_overlap`. Run side-by-side on identical input as test 16 with `ignore_3p_r1=0`; assert keep-set is `{200}` (the old buggy behavior — but now correctly representing "no clip applied").
19. **`drop_overlap_with_ignore_3p_r1_at_boundary` (Reviewer B I3 regression guard)**: same as test 16 but with R2 call exactly at `ref_pos = r1_ref_end + 1` (just inside the keep zone) and `ref_pos = r1_ref_end` (just outside) under both un-clipped and clipped boundaries. Asserts the predicate's strict-greater-than semantics are preserved through the fix.

(4 integration tests in bismark-extractor's `overlap.rs`.)

### 3.3 Integration check on colossal (manual, post-merge)

20. Re-run `iso_r1_3p` smoke (`--ignore_3prime 5` alone) on a fresh `--out` dir. Expect PASS 8/8 (was FAIL 8/8 pre-fix). Wall ~12 min.
21. Re-run the full Phase H PE matrix on the merged HEAD. Expect all 10 cells (including `r1r2_3p` × N=1+N=4) to PASS. Wall ~2-2.5 h.

### 3.4 What we are NOT adding to this plan

- Parallel.rs synthetic worker tests for the R1×R2 dispatch (tracked at #878).
- OB-strand SE test fixture (tracked at #878).
- N=4 perf collapse investigation (#876 Finding #4).
- Perl edge_clip hang workaround (#876 Finding #3).

## 4. Implementation order

Per CLAUDE.md TDD default:

1. **Tests-first commit** (`bismark-io`): unit tests 1-15 + 9a + 9b in `cigar.rs` (17 total). They MUST fail (compile error) on current HEAD — methods don't exist yet. Verify via `cargo check`.
2. **bismark-io fix commit**: implement `trim_3p_read_positions` + `reference_end_after_3p_trim` + `reference_start_after_3p_trim` in `cigar.rs`. Re-run unit tests 1-15 → expect PASS. **Verify bismark-io version FIRST** (V1 below); bump if Felix-approved.
3. **Tests-first commit** (`bismark-extractor`): integration tests 16-19 in a new `overlap.rs` test module. They MUST fail because the new signature isn't in place.
4. **bismark-extractor fix commit**: update `drop_overlap` signature + impl. Update 2 call sites (`pipeline.rs:354`, `parallel.rs:711`). Bump bismark-io path-dep version if applicable. Full test suite expected to be 310 (existing) + 17 (bismark-io) + 4 (extractor) = 331 passing.
5. **PR** against `rust/iron-chancellor`. Title: `fix(extractor): #879 drop_overlap respects --ignore_3prime via CIGAR-trim primitive`. Body links #879 + plan + dual-reviewer audit trail (PLAN_REVIEW_879_{A,B}.md + this rev 1).
6. **Post-merge on colossal**: tests 20-21 above. If both PASS, proceed to v1.0 release walk continuation.

## 5. Out of scope

Same as rev 0 — unchanged.

## 6. Assumptions / open decisions (rev 1)

| # | Question | Resolution | Source |
|---|---|---|---|
| A1 | API design — two strand-specific helpers OR CIGAR-trim primitive + helpers? | **CIGAR-trim primitive + 2 helpers** (rev 1 redirect). Both reviewers' I1 recommendation. Mirrors Perl L1807-1828; encapsulates D/N/I in one place. | Both reviewers (I1) |
| A2 | Helpers on `CigarExt` trait or free fns? | Trait methods (rev 0 unchanged) | Both reviewers OK |
| A3 | Full-clip return value: `start.saturating_sub(1)` or `start`? | **`start`** (rev 1 fix per C2) — matches existing `cigar.rs:276-278` convention | Both reviewers (C2) |
| A4 | Pass `ignore_3p_r2` to drop_overlap? | NO — R2 3'-clip handled by `call.rs:179-182` filter (citation added per Reviewer A I-citation request) | Both reviewers confirmed |
| A5 | Branch base | `rust/iron-chancellor` HEAD `45b4c61` | unchanged |
| V1 | Verify current `bismark-io` Cargo.toml version before bumping | **Run `git show 45b4c61:rust/bismark-io/Cargo.toml \| grep version` first**; the plan assumes `1.0.0-beta.7` but Reviewer A flagged uncertainty. If different, adjust the bump target. Bump cadence (every internal bismark-io change → version bump) confirmed by Reviewer B from git log. | Reviewer A V1 |
| V2 | Verify no other test in `tests/` directly calls `drop_overlap` with the 2-arg signature | grep first at commit time | Reviewer B V-style item |
| T1 (NEW per Reviewer B I3) | Boundary-case tests — assert `ref_pos == r1_ref_end` (drop) vs `== r1_ref_end + 1` (keep) under both un-clipped and clipped boundaries | Test #19 added | Reviewer B I3 |

## 7. Self-review (rev 1)

- [x] Bug root cause precisely identified — unchanged from rev 0, validated by both reviewers
- [x] **C1 absorbed**: trailing D/N strip explicitly described in primitive § 2.1 + 2 dedicated regression tests (#5, #6, #7)
- [x] **C2 absorbed**: full-clip returns `start` (matching `cigar.rs:276-278`) + dedicated regression test #15
- [x] **C3 absorbed**: OB composite shift covered by `reference_start_after_3p_trim`'s "original_ref_span − trimmed_ref_span" formula + dedicated regression test #14
- [x] **I1 absorbed**: CIGAR-trim primitive design (one primitive + 2 thin helpers) — Perl-faithful, encapsulates D/N/I in one place
- [x] **I2 absorbed**: naming pivoted from positional (`_clipping_right/_left`) to semantic (`_after_3p_trim`)
- [x] **I3 absorbed**: boundary-case test #19 added
- [x] **§2.4 citation absorbed**: `call.rs:179-182` cited explicitly to substantiate "R2 3'-clip doesn't need drop_overlap parameter" claim
- [x] No source edits performed — plan only
- [x] V1 verification flagged as commit-time implementer task
- [x] Out-of-scope items unchanged (no scope creep)
- [x] Plan-reviewer round 2 re-run on rev 1 — **both reviewers APPROVED**, 0 Critical, 2 Important refinements absorbed into rev 1.1 (tests 9a + 9b + §2.1 prose clarification on D/N strip interleaving)
- [ ] Implement trigger from Felix — pending

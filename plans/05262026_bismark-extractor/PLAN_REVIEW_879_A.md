# Plan Review A — `BUG_879_FIXES_PLAN.md`

**Reviewer**: A (independent, fresh context)
**Target**: `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/BUG_879_FIXES_PLAN.md` rev 0
**Verdict**: Sound fix; ship after addressing one Critical (test #5 off-by-one) and three Important issues (API shape, D-op walk semantics on test #4 / #8, plan §2.4 missing R2 trace).

---

## Logic review

The root cause is correctly identified: `overlap.rs:101` uses `reference_end` on R1's un-clipped CIGAR, while Perl L1726-1804 trims `@comp_cigar_1` THEN recomputes `$end_read_1 = $start_read_1 + $MDN_count_1 - 1` (L2400) or shifts `$start_read_1 += $MDN_count_1 - 1` from the trimmed CIGAR (L2415-2416). The asymmetry table (`§1`) is accurate: R1 5p / R2 5p / R2 3p don't touch R1's drop_overlap boundary; only R1 3p does, on both forward (right-pop) and reverse (left-shift) branches.

The OB reverse-strand semantics are correct. Perl L2406 reverses `@comp_cigar_1` BEFORE counting MDN, so the BAM-stored CIGAR's **left** side is the read's **3' end** when strand is `-`. Perl L1803 `$start += $ignore_3prime + $D_count + $N_count - $I_count` advances the alignment_start, which equals "ref-positions consumed by the dropped prefix" (M+D+N count, since `ignore_3prime` counts read-positions = M+I+S and subtracts back I). The plan's `reference_start_clipping_left → start + ref-positions of dropped prefix` matches.

## Assumptions / open questions

- **A1 (naming) — Important.** `reference_end_clipping_right` / `reference_start_clipping_left` is positional w.r.t. CIGAR storage. The OB branch wires "right-positional clip" to "5' BAM-storage / 3' read-orientation," which is exactly the confusion the table at `§1` is trying to dispel. Suggest `reference_end_after_clip_3p_storage_right` / `reference_start_after_clip_3p_storage_left`, or — cleaner — a single `cigar_with_trailing_read_clipped(n)` / `cigar_with_leading_read_clipped(n)` returning a trimmed `Cigar`, on which the caller calls existing `reference_end(start)`. The latter is **more Perl-faithful** (Perl literally rewrites the CIGAR, L1807-1828) and would let the new helpers be reused for any future code path that needs the trimmed CIGAR (e.g. an SE write path that revisits per-call CIGAR). Recommend pivoting to the trimming variant. Tests stay almost identical — they'd assert on `cigar.reference_end(start)` after trim.
- **D-op walk semantics — Important.** Test #4 (`90M5D10M`, clip 5) is correct: the 5D is BEFORE the 10M block being clipped, so it stays. But the plan never tests `100M5D` (D at the very tail). Perl L1760-1764 says: AFTER popping a non-D/N op from the end, if the NEW tail is D or N, keep popping D/N (the `while ($op eq 'D')` loop). This means a trailing D run **is consumed** without decrementing the clip counter, and the reference_end MUST move past those D ops. The plan's "D doesn't consume read" framing in `§2.1` is true but misses Perl's extra rule that **trailing D/N after the last read-consuming op are also stripped**. Add a test: `90M5D` clip 5 → reference_end loses 5 (from M) AND 5 (from trailing D) = 10. Same for `90M5D5M` clip 5 → strips 5M, then while-loop strips the 5D, then stops — reference_end loses 10. The current plan would compute "lose 5" and silently produce wrong results on reads with trailing deletions.
- **Test #5 (full clip) — Critical inconsistency.** Plan asserts `start.saturating_sub(1)` for clip-everything. Existing convention at `cigar.rs:276-278` returns `start` for empty CIGAR (and the existing `reference_end` impl at L185-186 returns `start` when `span==0`). Returning `start - 1` here is a new convention that will surprise readers and may interact badly with `r2_calls.retain(|c| c.ref_pos > r1_ref_end)` if `r1_start` is 1 (saturating_sub yields 0, every R2 with ref_pos>0 is kept — which happens to be wrong because "everything clipped" means no overlap and we should keep ALL R2 calls). With the trimming variant, this degenerates naturally to "trimmed CIGAR is empty → reference_end returns start → predicate `c.ref_pos > start` drops R2 calls AT R1's start." That's still arguably wrong, but at least matches existing convention. Either way, **the plan's chosen sentinel and the predicate's interaction with it must be jointly tested.** Currently no integration test exercises `ignore_3p_r1 ≥ read_span`.

## Efficiency

`drop_overlap` runs once per pair, hot path. Both new helpers are O(ops), same as `reference_span`. Zero-clip degenerates to identical work as today (the early-return `if n_read_positions == 0 { return reference_end(start); }` should be explicit in the impl — the plan implies it but the test #1 / #6 are the only enforcement). No regression. If the trimming variant is chosen, allocating a new `Cigar` per pair when `ignore_3p_r1>0` is a minor cost — acceptable given the flag is rare.

## Validation sufficiency

Gaps:
- **No test for trailing-D semantics** (Perl L1760-1764 while-loop). See above.
- **No test for full clip predicate behavior** (only the helper return value). Add an integration test: R1 with `ignore_3p_r1 = read_span`, assert all R2 calls kept (or whatever the correct biological semantic is — needs Felix sign-off).
- **Plan §2.4 is asserted, not traced.** "R2's 3'-clip is handled by `extract_calls` filter" — true for R2's own call set, but the claim that `drop_overlap` doesn't need to know about it depends on R2's `ref_pos` values being **independent of `ignore_3p_r2`**. Verify: `extract_calls` at `call.rs:179-182` filters by `read_pos`, leaving `ref_pos` untouched. Good — but the iso_r2_3p PASS only proves no regression in that one scenario. The combined cell `r1r2_3p` (the failing one) is what proves drop_overlap doesn't need `ignore_3p_r2`. The plan should cite the iso_r2_3p PASS plus the post-fix expectation that `r1r2_3p` PASSes once R1-side is fixed.
- **No test combining R1 3p clip with R1 having soft-clip + insertion + deletion**. The unit tests are each one-op-class. A `5S90M3I2D` style test would catch interaction bugs.

## Efficiency / version-bump nit (Important)

Git log shows bismark-io bumps tied to externally-visible feature work (`-beta.2` threaded readers #827, `-beta.3` magic-byte #831, `-beta.4` UMI #835, `-beta.6` iter_aligned #845). Adding two new public trait methods is genuinely API-visible → bump is justified. Note however that `-beta.7` is not in the log shown; confirm current Cargo.toml shows `-beta.7` before bumping to `-beta.8`. If current is `-beta.6` the plan's premise (`-beta.7 → -beta.8`) is off-by-one.

## Action items

**Critical**
1. Resolve test #5 off-by-one vs `reference_end_with_empty_cigar_returns_start` convention; add integration test for full-clip predicate behavior.

**Important**
2. Add trailing-D/N tests (`90M5D` clip 5, `90M5D5M` clip 5) and verify the impl mirrors Perl L1760-1764 / L1765-1769 while-loops.
3. Reconsider API shape: prefer `cigar_with_trailing_read_clipped(n) -> Cigar` (Perl-faithful) over the two boundary-returning helpers; rename to non-positional terms if keeping current shape.
4. Trace `§2.4` claim explicitly with code refs (`call.rs:179-182` for R2 filter, parallel.rs:704-714 for the order).
5. Confirm current `bismark-io` version in `rust/bismark-io/Cargo.toml` matches plan §4 step 2's `-beta.7` starting point.

**Optional**
6. Add a combined-op CIGAR test (`5S90M3I2D` style).
7. Update `overlap.rs` module docs to mention the new behavior under `--ignore_3prime`.
8. Consider whether SPEC §7.4 rev 4 should formalize the `ignore_3p_r1` parameter (plan §5 defers — fine, but ticket it).

---

Report written to `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PLAN_REVIEW_879_A.md`.

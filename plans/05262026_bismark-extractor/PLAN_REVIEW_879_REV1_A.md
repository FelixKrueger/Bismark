# Plan Review A — `BUG_879_FIXES_PLAN.md` rev 1 (round 2)

**Reviewer**: A (independent, fresh context — round 2)
**Target**: `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/BUG_879_FIXES_PLAN.md` rev 1
**Verdict**: **GO** with one Important clarification on D/N strip semantics. Round-1 Critical findings (C1/C2/C3) are correctly absorbed at the algorithmic level. One subtle test description is misleading; the underlying primitive is still correct.

---

## C1 absorption — Trailing D/N strip (verified against Perl L1756-1770)

The plan §2.1 primitive description and test #5 (`90M5D` clip 5 → `85M`) are **correct** but the plan's prose framing ("After the read-position trim loop completes, also strips any now-exposed trailing D/N ops") is **subtly misleading** about where the strip happens.

Reading Perl L1756-1770 literally: the `while ($op eq 'D')` and `while ($op eq 'N')` loops are **INSIDE** the `for (1..$ignore_3prime)` body. Each for-iteration pops one op; if that op is D/N, the while-loop pops until a non-D/N op surfaces; that non-D/N op counts as the iteration's read-position. There is NO post-loop strip in Perl.

The behavioral consequence:
- `90M5D` clip 5: iter 1 pops `D` → while pops 4 more D's → pops `M` (1 read-pos). Iters 2-5 pop M (4 more read-pos). Result: `85M`. ✓ (matches test #5)
- `90M5D5M` clip 5: iter 1 pops `M`, iter 2 pops `M`, ..., iter 5 pops `M`. Five M's gone. Trailing `5D` remains. Result: `90M5D`, **NOT** `85M`. Reviewer B round-1 wrote `100M_clip_traversing_D` with `90M5D5M` clip 10 expecting "85M"; under correct Perl semantics this would be `85M` only because the 10th iteration pops a D from the now-exposed 5D, triggering the inner while loop. So Reviewer B's example was right for clip=10, not clip=5.

**Action**: rephrase the primitive's docstring (plan §2.1, lines 43-60) from "After the read-position trim loop completes, also strips any now-exposed trailing D/N ops" to "Within each read-position consumed, if the popped op is D or N, continue popping D/N until a read-consuming op surfaces (that op counts as the read-position)". This matches Perl exactly and prevents a future maintainer from implementing a post-loop strip that would diverge on `90M5D5M` clip 5.

Test #5 / #6 / #7 are correct as written. Recommend adding a **negative regression** test: `90M5D5M` clip 5 → `90M5D` (NOT `85M`), to lock in the Perl-faithful semantic and prevent the post-loop-strip implementation drift.

## C2 absorption — Full-clip return convention (verified)

Plan A3, §2.1 docstring for `reference_end_after_3p_trim`, and test #15 all align: trimmed empty CIGAR → `reference_end` returns `start` per `cigar.rs:185-186` (`if span == 0 { start }`) and the existing `reference_end_with_empty_cigar_returns_start` test at L276-278. ✓ Clean absorption.

## C3 absorption — OB composite shift (algebraically verified)

Plan §2.1's `reference_start_after_3p_trim` formula `start + (original_ref_span - trimmed_ref_span)` is **algebraically equivalent** to Perl L1803's `$start += $ignore_3prime + $D_count + $N_count - $I_count`:
- Dropped prefix ref-positions = M_dropped + D_dropped + N_dropped (reference_span counts M+D+N+=+X).
- Perl: ignore_3prime = M+I+S read-positions; plus extra D + extra N (which are popped but don't count toward ignore_3prime); minus I (I consumes read but no ref, but ignore_3prime already counted it). Net: M + D + N. ✓
- Test #14 (`5D90M` clip 5 left → trimmed `85M`, shift=10, start+10=110) validates this directly.

## I1 absorption — CIGAR-trim primitive (good shape, mild redundancy concern)

The primitive + 2 helpers shape is right. One small efficiency note: the 2 helpers are 3-5 line wrappers that the call sites in `overlap.rs` could just as well inline (`pair.r1().cigar().trim_3p_read_positions(n, false).reference_end(start)`). Keeping the helpers buys: (a) named call-site semantics, (b) one place to enforce the full-clip return convention. Worth the keep, but consider documenting the helpers as "thin wrappers, may be inlined at call site if needed" so future refactors don't see them as load-bearing.

## I2 absorption — Naming (acceptable, mild concern)

`_after_3p_trim` is semantic and clearer than `_clipping_right/_left`. Minor concern: "trim" overlaps with Trim Galore terminology (the upstream read-trimming tool, often a step before Bismark). Risk is low because this lives in `bismark-io::CigarExt`, not at a user-facing layer. Optional: `_after_3p_clip` would be unambiguous in the Bismark/methylation domain (`--ignore_3prime` is a clip). Not blocking.

## I3 absorption — Boundary tests (partially absorbed)

Test #19 description (plan §3.2 lines 180-181) covers boundary cases under "both un-clipped and clipped boundaries" but the test is described in prose. To pin Reviewer B's I3 intent, the test should explicitly assert FOUR values:
- `r2_pos == r1_ref_end` → DROP (un-clipped)
- `r2_pos == r1_ref_end + 1` → KEEP (un-clipped)
- `r2_pos == clipped_r1_ref_end` → DROP (clipped)
- `r2_pos == clipped_r1_ref_end + 1` → KEEP (clipped)

The plan text says "the predicate's strict-greater-than semantics are preserved through the fix" which captures the intent but the implementer might write a one-sided boundary check. Recommend pinning the 4 assertions explicitly in the plan.

## §2.4 citation absorption (verified)

`call.rs:179-182` (verified): `if aligned.read_pos_5p < lo || aligned.read_pos_5p >= hi { continue; }`. Matches the plan claim. ✓

## V1 verification (done now)

`git show 45b4c61:rust/bismark-io/Cargo.toml | grep version` → `version = "1.0.0-beta.7"`. The plan's assumed starting point is correct. Bump target `-beta.8` is justified (new public trait methods = API-visible change). V1 can stay as a commit-time check; no plan blocker.

## V2 (Reviewer B) — `drop_overlap` callers

Plan §2.3 cites `pipeline.rs:354` and `parallel.rs:711` as the only callers. V2's grep-at-commit-time is fine. No plan blocker.

## New issues introduced by rev 1

Examined the API restructure for new bugs:
- **Test #8 semantics check**: `5S95M` trim 5 from LEFT → "ref unchanged at start". S consumes read but not ref. From-left trim consumes 5 read-positions of S; the M block is untouched; trimmed CIGAR `95M`; original_ref_span = 95, trimmed_ref_span = 95; shift = 0; start unchanged. ✓ Correct. But this is the OB-strand path where R1's BAM-stored 5'-side maps to the read's 3' end. For OB R1 with leading soft-clip, this means: "5S at BAM-position 0 = soft-clipped 3' read end" — the soft-clip is ALREADY removed from the read's effective end. Clipping further by `--ignore_3prime 5` should clip 5 MORE bases beyond the soft-clip — which the test doesn't quite express. Worth adding a test like `5S95M` clip 5 left where the expected semantic is "consume the 5S, then the next clip would need to dip into 95M" — but the primitive as specced only consumes 5 read-positions total. **Verify**: does Perl's `pop/shift @comp_cigar` include S ops? Yes — S is in `@comp_cigar`. So Perl `5S95M` clip 5 from left would consume the 5S and stop. The primitive matches Perl. ✓

## Round-1 findings cross-check

All round-1 Critical (C1, C2, C3) and Important (I1, I2, I3) items appear in §7 self-review and have corresponding plan-text changes + tests. Optional items O1/O2 not absorbed — fine, those are scope-deferred (CI smoke harness, byte-equality check). Reviewer A's I4/V1 absorbed.

---

## Action items

### Critical
None. Rev 1 absorbs C1/C2/C3 correctly at the algorithmic level.

### Important
1. **Reword §2.1 primitive docstring** to put the D/N strip INSIDE the per-read-position loop (matches Perl L1756-1770 literally). Current "After the read-position trim loop completes, also strips" phrasing risks implementer drift on `90M5D5M` clip 5.
2. **Add negative-regression test**: `90M5D5M` clip 5 from right → `90M5D` (NOT `85M`). Locks in the Perl-faithful "strip happens only when the popped op IS D/N, not after" semantic.
3. **Pin test #19's 4 boundary assertions explicitly** in plan §3.2 (un-clipped DROP/KEEP × clipped DROP/KEEP).

### Optional
4. Consider `_after_3p_clip` over `_after_3p_trim` to avoid Trim Galore terminology overlap.
5. Document the 2 helpers as "thin wrappers, callers may inline if preferred" to signal they're not load-bearing.
6. After implementation, run `cargo bench` (or hand-time) `drop_overlap` on a 100k-pair sample to confirm the `ignore_3p_r1=0` no-op fast-path actually short-circuits (the plan says it does; verify).

---

File written: `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PLAN_REVIEW_879_REV1_A.md`.

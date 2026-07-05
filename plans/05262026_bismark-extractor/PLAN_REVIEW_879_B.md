# Plan review — fix #879 (Reviewer B)

**Plan**: `plans/05262026_bismark-extractor/BUG_879_FIXES_PLAN.md` (rev 0)
**Reviewer**: B (independent, fresh context)
**Verdict**: **Conditional GO** — root cause is correct and the localized fix shape is right, but there are three correctness issues in the helper semantics and tests that should be resolved before implementation.

## Logic review

The diagnosis at `overlap.rs:101` is precise and the Perl contract at `bismark_methylation_extractor:1726-1804` + `:2400/:2415-2416` is faithfully cited. The symmetry table in §1 correctly explains why `iso_r1_3p` FAILs while the other three flags PASS. Quantitative deficit (-943k calls, OT direction) is consistent with the predicate flip story.

However, the helper specification has a load-bearing gap on the **reverse-strand (OB) branch**. Perl L1803 explicitly adds three terms: `$start += $ignore_3prime + $D_count + $N_count - $I_count`. This is then *followed* by the L2416 recomputation `$start_read_1 += $MDN_count_1 - 1` over the already-shortened CIGAR. The plan's `reference_start_clipping_left` collapses these into a single "ref positions of the dropped prefix" calc. That formulation is morally equivalent **only if** the helper treats D/N ops in the clipped prefix as contributing to ref-positions-consumed while NOT decrementing the read-position counter (mirroring the Perl `while $op eq 'D' shift @comp_cigar` loop on L1783-1790). The plan text in §2.1 says "D/N skip" the read-position counter, which is consistent, but the **prose is ambiguous about whether D/N ops in the clipped span are REMOVED from the CIGAR (Perl: yes — `shift`) or RETAINED**. The result differs.

## Critical findings

### C1. No test exercises clip crossing a D/N op in the clipped region
Test #4 (`90M5D10M`, clip 5) deliberately keeps the clip inside the trailing 10M and never reaches the 5D. Walk through `90M5D5M` with clip=10: trailing 5M consumed (counter 5/10), then 5D — D doesn't decrement the read counter but is *popped* by Perl (L1760-1764), then 5 more of the 90M block consumed (counter 10/10). Final remaining ref-span: 85 (not 90). The ref_end shifts by 15 ref positions while only 10 read positions were clipped. **Add a test for this case** — `100M_clip_traversing_D` and the OB-mirror `100M_clip_traversing_D_left`. Without it, the helper can silently keep or drop the D ops with no test signal.

### C2. Degenerate-empty return value contradicts existing convention
Plan §2.1 specifies `reference_end_clipping_right` returns `start.saturating_sub(1)` when fully clipped. But the existing convention at `cigar.rs:276-278` (`reference_end_with_empty_cigar_returns_start`) returns `start` for an empty CIGAR with span=0. Two adjacent functions in the same trait disagreeing on the "nothing aligned" return value is a footgun for the `drop_overlap` predicate. If `ignore_3p_r1` clips the entire R1, then `r1_ref_end = start - 1` means **all R2 calls with `ref_pos > start - 1` are kept** — i.e., almost everything, including the original overlap region. Is that the desired behavior? Plausibly yes (no R1, no overlap-zone), but it needs to be explicit. Recommendation: align with existing convention OR document explicitly why this case diverges, and assert the chosen behavior in a test that walks through what drop_overlap actually returns. Currently no integration test covers `ignore_3p_r1 >= read_span`.

### C3. OB-strand helper may not fully mirror Perl L1803's `+ ignore_3prime - I_count` adjustment
Perl L1803 adds `$ignore_3prime` (one count per clipped position) PLUS `$D_count` (extras) MINUS `$I_count`. Then L2416 re-derives `$start_read_1` from the trimmed CIGAR's MDN count. The plan's helper does the second step but the first step's `+ $ignore_3prime - $I_count` is subtle: it means each clipped M contributes 1 ref-position, each clipped I contributes −1 (so net 0, since the I never consumed a ref position), each clipped D contributes 1+1=2. Walking the Perl through `5M2I5M` clip 5 from left: 5 M's clipped → +5 ignore_3prime, 0 D, 0 N, 0 I → start += 5. CIGAR becomes `2I5M`. Then L2416: MDN_count = 5 → start += 4. Total shift from `$start_read_1`: 5 + 4 = 9. The plan's helper would compute "ref positions in clipped prefix" = 5 (just the M), then `start + 5`. The L2416 transform is the caller's responsibility (it doesn't happen in drop_overlap). **Verify the math matches by hand on a non-trivial OB R1 fixture before declaring victory.** Test #7 (`100M` clip 5) and test #8 (`5S95M` clip 5) don't exercise this.

## Important findings

### I1. API shape: `cigar_with_3p_clipped(n) -> Cigar` would be more Perl-faithful
The Perl algorithm is unambiguous: trim the CIGAR, then run the existing end/start arithmetic on the trimmed CIGAR. A helper that returns a trimmed `Cigar` and lets the caller use the existing `reference_end(start)` would (a) reduce surface area to one method instead of two, (b) match Perl's semantic exactly, and (c) make the D-pop policy fall out naturally (rebuild the CIGAR without the popped ops). Downside: an extra allocation per overlap pair. Quantify: on 55.7M PE reads with `--ignore_3prime` set, that's ~55.7M small `Vec<Op>` allocations. Probably negligible compared to BAM decode, but worth measuring. **At minimum, document why the two-helper approach was chosen over the trim-and-reuse approach.**

### I2. Naming — `_clipping_right` / `_clipping_left` is positional, not semantic
`right`/`left` refers to CIGAR-array position, but for OB pairs the **read 3'-end** is at the CIGAR's left side (because the BAM stores reads in reference orientation). A reader of `drop_overlap` will see "this is the `--ignore_3prime` case, so I want the 3' clipping helper" — and then have to remember "but for OB R1, 3' is left." Recommend `clip_3p_cigar_right` / `clip_3p_cigar_left` if the two-method shape stays, OR a single `clip_3p_read_end(n, strand)` that internally branches. The current names will confuse the next maintainer.

### I3. Tests #9-#11 are under-specified
Test #9's narrative ("Actually simpler: synthetic pair where un-clipped R1 ends at ref 199...") shows the plan author derived the predicate on the fly while drafting. That's a yellow flag — the test as written assumes the helper produces `r1_ref_end = 194` for `100M` clip 5 start 100. Confirm: 100M clipped by 5 → ref_span = 95 → reference_end = 100 + 95 − 1 = 194. ✅ correct. But the test doesn't pin which R2 call positions should be dropped vs kept across multiple boundary cases (192, 194, 195, 199). Add boundary tests at `r2_pos == r1_ref_end` (drop), `r2_pos == r1_ref_end + 1` (keep), `r2_pos == r1_ref_end - 1` (drop) — current text only covers a single intermediate position.

### I4. No integration assertion that #872's matrix passes
Plan §3.3 says "expect all 10 cells (including r1r2_3p × N=1+N=4) to PASS." That's the actual ground-truth signal — but it's framed as manual post-merge work. The dev-loop should ideally have a fast `iso_r1_3p` subset that runs in CI on every push. Out of scope for this fix, but flag it for the work-queue.

## Assumptions

- **A1 (naming)**: not OK — see I2.
- **A2 (trait methods)**: OK, consistent with existing `CigarExt`.
- **A3 (u32, not Option)**: OK, matches existing config field types.
- **A4 (no `ignore_3p_r2`)**: **traced and confirmed**. R2 3p-clip filters R2 calls' `ref_pos` upstream at `call.rs:179-182`, and `drop_overlap` only consults R1's CIGAR. The R2 calls that survive to `drop_overlap` already have the clipped tail removed from consideration. No code path in `drop_overlap` reads R2's CIGAR. The `iso_r2_3p` PASS empirically confirms this. ✅
- **A5 (target branch)**: OK.
- **Cross-crate version bump (§4 step 2)**: history shows every prior bismark-io change bumped the version (beta.1 → .2 → .3 → .4 → .6 → .7). Plan's `.7 → .8` matches that precedent. ✅

## Efficiency

The fix is O(CIGAR ops) per pair (typically <10 ops), guarded by a zero-cost path when `ignore_3p_r1 == 0`. No allocation, no extra iteration of `r2_calls`. No concerns. The two-helper vs trim-CIGAR alternative (I1) trades allocation for clarity — measure if uncertain.

## Validation sufficiency

**Insufficient as drafted**. Specific gaps:
1. No test where clipping crosses a D/N op (C1).
2. No test where the helper input is empty / fully-clipped + assertion on `drop_overlap` behavior (C2).
3. No boundary-edge tests for `drop_overlap` keep/drop predicate (I3).
4. No OB-strand test with InDels in the clipped prefix that verifies the Perl L1803 + L2416 composite shift (C3).
5. Plan mentions `cargo check` to verify compile-fail tests but no `cargo nextest` or `cargo test --workspace` invocation; OK but be explicit in §4.

## Alternatives

1. **Trim-CIGAR helper** (`clip_3p_cigar(n, from_left: bool) -> Cigar`) + reuse existing `reference_end`. See I1. Single API surface, exactly Perl-shaped, slight allocation cost.
2. **Inline the algorithm in `drop_overlap`** without adding `CigarExt` methods. Rejected — same reasoning as the plan (testability + reuse). Plan choice is correct here.
3. **Pre-compute clipped boundaries in `extract_calls`** and stash them on the pair. Rejected — couples extract_calls to overlap concerns, wrong layer.

## Action items

### Critical (block implementation)
- **[C1]** Add unit tests for `reference_end_clipping_right` and `reference_start_clipping_left` that traverse a D or N op in the clipped region. Document explicitly: do D/N ops in the clipped region get REMOVED from the trimmed CIGAR (Perl: yes)?
- **[C2]** Reconcile the degenerate "everything clipped" return convention with existing `reference_end_with_empty_cigar_returns_start` at `cigar.rs:276-278`. Either align (return `start`) or document why divergence is correct; add a `drop_overlap` test for `ignore_3p_r1 >= read_span`.
- **[C3]** Add an OB-strand test fixture with InDels in the first 5 CIGAR ops; hand-derive the expected `r1_ref_start` from the Perl L1803 + L2416 composite math and assert.

### Important (resolve in plan, before implement)
- **[I1]** Document why the two-helper approach was chosen over `clip_3p_cigar(n, from_left) -> Cigar`; quantify the allocation overhead vs the API-clarity win.
- **[I2]** Rename to `clip_3p_cigar_right` / `_left` or to a single strand-aware helper. `_clipping_right/_left` reads as positional in a context where "3'" already carries directional meaning.
- **[I3]** Pin the boundary cases in tests #9 and #10 explicitly (exact predicate, exact ref_pos at boundary ±1).

### Optional
- **[O1]** Add a fast `iso_r1_3p` smoke harness for CI (out of #879 scope; file separately).
- **[O2]** Confirm test #11 ("`ignore_3p_r1=0` is no-op") asserts byte-equality with the pre-fix output, not just "non-zero overlap dropped." Plan text is loose.

---
File written: `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PLAN_REVIEW_879_B.md`

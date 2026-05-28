# Plan Review — `BUG_879_FIXES_PLAN.md` rev 1 (Round 2, Reviewer B)

**Reviewer**: B round 2 (independent, fresh context — NOT same agent as PLAN_REVIEW_879_B.md round 1)
**Target**: `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/BUG_879_FIXES_PLAN.md` rev 1
**Verdict**: **APPROVED for implementation.** All three round-1 Criticals (C1/C2/C3) are correctly absorbed and verifiable by hand against the Perl reference. Important items I1/I2/I3 and Reviewer A's §2.4 citation are absorbed. V1 can be marked resolved now (verified below). One minor naming clarification noted as Optional.

---

## Logic review — verifying each round-1 finding is genuinely fixed

### C1 (trailing D/N strip) — ABSORBED

§2.1 explicitly describes the post-pop trailing D/N loop and cites Perl `bismark_methylation_extractor:1760-1764`. I verified the Perl source at L1760-1769: after `pop @comp_cigar` consumes a non-D/N op, two `while ($op eq 'D')` / `while ($op eq 'N')` loops keep popping without decrementing the for-loop counter `(1..$ignore_3prime)`. Rev 1's primitive description ("continue with a trailing-D/N strip loop that drops adjacent D/N ops at the trimmed boundary without decrementing the read-position counter") is the correct mirror.

Hand-trace test #5 (`90M5D`, n=5 from right):
- Read-position loop pops 5M-positions off the 90M → leaves `85M5D`.
- The strip loop sees trailing 5D → strips it → `85M`. ✓
- `reference_end(85M, start=100)` via existing impl = 100 + 85 − 1 = **184**. ✓
- Pre-fix bug would have computed 100 + 95 − 1 = 194 (because the un-clipped `90M5D` has ref_span 95). Fix shifts the boundary by 10 ref positions, matching Perl's `M_count + D_count`.

Test #6 (`90M5N`) is symmetric. Test #7 (`5D90M` from left) is the OB-direction analog — also correct.

### C2 (full-clip return value) — ABSORBED

§A3 commits to `start` (matching `cigar.rs:185-187` and the existing test at L276-278). The helper impl `self.trim_3p_read_positions(n, false).reference_end(start)` naturally yields `start` when the trimmed CIGAR is empty, because `reference_end` returns `start` when `span == 0`. Test #15 asserts this directly. No new convention is being introduced. ✓

### C3 (OB composite shift) — ABSORBED, with one note

§2.1's `reference_start_after_3p_trim` formula is `start + (original_ref_span − trimmed_ref_span)`. I verified the algebra against Perl L1803:

- Perl: `$start += $ignore_3prime + $D_count + $N_count − $I_count`.
- Perl `$ignore_3prime` counts read positions consumed = M + I in the dropped prefix.
- So Perl shift = `(M + I) + D + N − I = M + D + N` (ref-consuming ops in the dropped prefix).
- Rev 1's `original_ref_span − trimmed_ref_span` = sum of ref-consuming ops dropped = `M + D + N` (since `reference_span` already excludes I).
- **Identical.** ✓

Test #14 (`5D90M` start=100, n=5) hand-trace: from-left trim of 5 read-positions consumes 5M, then strip loop swallows the leading 5D (but wait — the trim is from the LEFT, so D comes BEFORE the M in CIGAR order; the strip after pop would target the new leftmost element). Reading the plan again: in the from-left case, Perl L1782-1790 `shift @comp_cigar` then `while ($op eq 'D') { $D_count++; $op = shift @comp_cigar }` — so D-strip happens BEFORE the M-consume? No: the `while` is entered when the shifted op IS D. The order is: shift one op; if it's D, keep shifting D's; if it's N, keep shifting N's; if it's I, count I and continue. So for `5D90M` n=5: shift 5D (D-loop swallows it, doesn't decrement); now shift 5 of the 90M one at a time → end with `85M`. Shift count = D-loop iterations + 5 for-loop iterations.

Hand-trace: trimmed CIGAR = `85M`, ref_span trimmed = 85, original ref_span (5D+90M) = 95, shift = 10, start = 110. ✓ Matches plan's expected test #14 outcome.

**Note**: the plan's primitive description says "after the read-position trim loop completes, also strips any now-exposed trailing D/N ops" — this phrasing is right for from-right but slightly misleading for from-left, where Perl actually strips D/N BEFORE consuming the next read-position op on each iteration. The end-state CIGAR is the same (D/N ops between/around the trimmed read-positions are stripped), but an implementer reading only §2.1's English description might code it wrong. **Optional**: clarify in §2.1 that the strip-D/N behavior applies symmetrically on both ends and is interleaved with read-position consumption per iteration (per Perl L1760-1769 for from-right and L1783-1793 for from-left).

### I1 (CIGAR-trim primitive) — ABSORBED, well-shaped

The primitive `trim_3p_read_positions(n, from_left) -> Cigar` plus two thin helpers is the right shape. The helpers are not strictly redundant — they hide the `from_left` boolean and the `reference_span` subtraction from call sites, giving `drop_overlap` clean semantics. Keeping them is worth the ~10 lines.

### I2 (semantic naming) — ABSORBED

`_after_3p_trim` is clearer than `_clipping_right/_left`. Slight risk of confusion with Trim Galore's read-trimming, but the `3p` qualifier and trait location (`CigarExt`) make the scope unambiguous. ✓

### I3 (boundary tests) — ABSORBED

Test #19 covers both `ref_pos == r1_ref_end + 1` (kept) and `ref_pos == r1_ref_end` (dropped) under both clipped and un-clipped boundaries — sufficient to guard against off-by-one in the predicate. ✓

### Reviewer A §2.4 citation — ABSORBED

`call.rs:179-182` cited and accurate. I verified the lines: the filter is `if aligned.read_pos_5p < lo || aligned.read_pos_5p >= hi { continue; }`, which is read-position-based and never touches `ref_pos`. ✓

### V1 (bismark-io version) — RESOLVED NOW

`git show 45b4c61:rust/bismark-io/Cargo.toml` shows `version = "1.0.0-beta.7"`. Plan §4 step 2's `-beta.7 → -beta.8` premise is correct. **Recommend striking V1 from "commit-time tasks" and marking it resolved in the table — one fewer thing for the implementer to forget.**

---

## New issues introduced by rev 1

### Question 9 (soft-clip prefix on OB) — verified safe

Test #8 (`95M5I` from right): trim 5 read positions of trailing I → returns `95M`; reference_end unchanged. ✓ But the question asked about `5S95M` from the LEFT (OB R1 BAM-stored 5' = read's sequenced 3'). The plan doesn't include this test. Tracing: from-left trim of 5 read-positions on `5S95M` consumes the 5S (S consumes read positions), then the strip-D/N loop is a no-op (next op is M, not D/N), leaving `95M`. Ref-span original = 95 (S doesn't consume ref), trimmed = 95, shift = 0, start unchanged. **This is correct biologically**: a soft-clip at the BAM-storage 5' (= read 3' under OB) means those bases were already unaligned, so clipping them shouldn't shift the alignment start. Adding a test for `5S95M` from-left would be a low-cost belt-and-braces. **Important**.

### Combined-op CIGAR test (Reviewer A round-1 Optional #6) — still missing

`5S90M3I2D` (or similar combined) is not in the test list. Each rev 1 test is one-op-class. **Optional** — but cheap to add and would catch state-machine interaction bugs the unit tests don't.

### V2 not actioned

The plan flags V2 ("grep for other `drop_overlap` callers") as a commit-time task. This is fine, but a 5-second grep before implementation start would be cheaper than discovering a third caller mid-implement. **Optional**.

---

## Round-1 items still unaddressed

Cross-checking §7 self-review against both round-1 reports:

- Round-1 Reviewer A Optional #6 (combined-op CIGAR test) — not addressed. **Optional**.
- Round-1 Reviewer A Optional #7 (overlap.rs module doc update) — not addressed. **Optional**, low value.
- Round-1 Reviewer B I4 ("integration assertion that #872's matrix passes") — covered implicitly by §3.3 tests 20-21 (post-merge Phase H matrix). ✓

No Critical/Important from round 1 leaked through. The round-1 → rev 1 absorption is thorough.

---

## Efficiency

`drop_overlap` is hot-path (once per pair). The CIGAR-trim primitive allocates a new `Cigar` (Vec<Op>, typically <10 ops) when `ignore_3p_r1 > 0`. With the no-op fast-path on `n == 0`, the default-cell allocation is zero. ✓ No regression risk.

---

## Validation sufficiency

15 unit tests + 4 integration tests is generous coverage. Gaps:
1. `5S95M` from-left soft-clip-at-BAM-5p test (Important above).
2. Combined-op CIGAR test (Optional).

Otherwise, every Critical and Important from round 1 has at least one regression test.

---

## Alternatives

The CIGAR-trim primitive + 2 helpers shape is the right call. Alternative shapes considered (and correctly rejected):
- Inline in `drop_overlap` — worse testability.
- Free fns instead of trait methods — inconsistent with existing `CigarExt`.
- Single helper that takes a from_left bool at the boundary level — loses Perl-faithfulness; current primitive+helpers is cleaner.

---

## Action items

### Critical
*(none — all round-1 Criticals are absorbed)*

### Important
1. Add test: `5S95M` trim 5 from left → trimmed `95M`, start unchanged. Validates soft-clip-at-BAM-5p doesn't shift the OB start.
2. Mark V1 resolved in §6 (verified `1.0.0-beta.7` at HEAD `45b4c61`). One fewer commit-time gotcha.

### Optional
3. Clarify §2.1's English: trailing-D/N strip is interleaved per iteration in Perl (not a separate post-pass), symmetric on both ends. Algebra is the same; just clearer for the implementer.
4. Add a combined-op CIGAR unit test (`5S90M3I2D` trim 5 from right) for state-machine interaction coverage.
5. Run `rg 'drop_overlap\\(' rust/` before starting to confirm only 2 callers (V2).

---

Report written to `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PLAN_REVIEW_879_REV1_B.md`.

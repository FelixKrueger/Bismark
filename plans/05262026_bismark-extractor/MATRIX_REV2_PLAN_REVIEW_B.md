# Plan Review B — PE matrix rev 2: overlap differential `≥5%` → `strictly > D`

**Reviewer:** B (independent, fresh context)
**Plan:** `plans/05262026_bismark-extractor/MATRIX_REV2_OVERLAP_DIFFERENTIAL_PLAN.md`
**Target:** `scripts/phase_h_pe_matrix.sh`
**Date:** 2026-05-29

**Overall verdict:** Approve with one **Critical** correction (the SPEC §8.3 claim
is wrong — the SPEC *does* pin the 5% and must be edited in this PR) and a couple
of Important wording-completeness fixes. The core logic change (`strictly > D`) is
sound, correctly traced, and fail-closed behavior is preserved.

---

## 1. Logic review

### 1.1 Is `strictly > D` the correct invariant?

Yes. `--include_overlap` overrides the PE default `--no_overlap`, which drops R2
methylation calls that fall in the R1/R2 reference-overlap region (SPEC §7.4,
lines 412–500). Switching it on *re-adds* those R2 calls. Every re-added call lands
at a read-relative M-bias position that already exists (R2 positions are bounded by
read length, identical to the default run), so:

- **Row count** is unchanged (no new positions) — this is why the plan correctly
  uses count-sum, not row count, for this cell.
- **Count-sum** (`sum(methylated + unmethylated)` over data rows) strictly
  increases by exactly the number of re-added overlap-region calls, **iff at least
  one R2 call was previously dropped**.

The only thing that guarantees ≥1 dropped call is that *some* mate overlap exists in
the library. The pre-flight gate asserts ≥80% properly-paired (flag 0x2), and
properly-paired FR reads with insert < 2×read_len overlap by construction. So on any
BAM that passes pre-flight, count-sum is strictly greater than D. The `+5%`
*magnitude* is not derivable from anything the gate guarantees — it is a function of
the overlapping-base fraction = (2·read_len − insert_len)/insert_len-ish, a per-library
insert-size property. The plan's reasoning here is correct and well-argued.

**Conclusion:** `strictly > D` is the tight, always-true invariant; `> D·1.05` was an
over-specification that fails on long-insert libraries (and would fail Perl-vs-Perl,
as the empirical evidence confirms: the overlap-cell M-bias.txt is byte-identical
Perl↔Rust yet only +2.28%).

### 1.2 Control-flow trace of the new assertion block (lines 502–511)

Current block:
```bash
if [[ -n "$OVERLAP_COUNTS" && -n "$D_COUNTS" && "$D_COUNTS" -gt 0 ]]; then
  OVERLAP_THRESHOLD=$(( D_COUNTS * 105 / 100 ))
  if [[ "$OVERLAP_COUNTS" -le "$OVERLAP_THRESHOLD" ]]; then PASS_FLAG=0; ... fi
else
  PASS_FLAG=0; ...  # unreadable counts
fi
```
Proposed:
```bash
if [[ -n "$OVERLAP_COUNTS" && -n "$D_COUNTS" && "$D_COUNTS" -gt 0 ]]; then
  if [[ "$OVERLAP_COUNTS" -le "$D_COUNTS" ]]; then PASS_FLAG=0; ... fi
else
  PASS_FLAG=0; ...  # unchanged
fi
```

- **Boundary correctness:** PASS iff NOT (`OVERLAP_COUNTS -le D_COUNTS`) ⇒ PASS iff
  `OVERLAP_COUNTS > D_COUNTS`. This is exactly "strictly greater than D". The
  equality case (`OVERLAP_COUNTS == D_COUNTS`, i.e. zero overlap) correctly FAILs —
  desirable, because zero increase under `--include_overlap` would itself be a Rust
  regression worth catching. Correct.
- **Dangling reference:** `OVERLAP_THRESHOLD` is assigned at line 503 and read only at
  504/506 (verified by grep — those are the only three occurrences in the file).
  Dropping it leaves no dangling reference. Correct.
- **Fail-closed `else`:** the `else` arm (empty/unreadable counts, or `D_COUNTS<=0`)
  still sets `PASS_FLAG=0`. Preserved verbatim. Good — this is the fail-open class
  bug SPEC §8.3 line 767 warns about, and the plan keeps it closed.
- **`ROW_COUNT_OK` init:** still `0` at line 420, flipped to `1` only at line 514
  inside `if PASS_FLAG -eq 1`. Untouched. Fail-closed posture intact.

### 1.3 Line-number accuracy vs current file

I re-verified every line number the plan cites against the live file. They are
accurate (file has not shifted):

| Plan claim | Actual | Status |
|---|---|---|
| Assertion "lines ~501–507" | block is 501–511 (`if`/`else` ends at 511) | OK — plan's "~501–507" covers the inner `if`; the replacement snippet it provides *does* include the `else`, so the full 502–511 region is replaced. Fine. |
| Header comment "line ~36" | line 36 (`> D + 5%`) | ✓ exact |
| Comment "line ~414" | line 414 (`count-sum > D + 5%`) | ✓ exact |
| speedup_table emitter "line ~699" | line 699 (`strictly > D by ≥5%`) | ✓ exact |
| verdict REASON "line ~773" | line 773 (`count-sum>D+5% for overlap`) | ✓ exact |

### 1.4 Exit-code trace (confirms exit 3)

After the fix, on the SRR24827378_10M data: FAIL_COUNT=0, CROSS_N_FAILS=0,
MBIAS_BASELINE_OK=1, ROW_COUNT_OK=1 (now passes since 192,423,276 > 188,123,599),
PERF_TARGET_MET=0 (0.58× scaling). The verdict ladder (lines 758–788) falls through
USAGE/FAIL/cross-N/baseline/differential and lands on
`elif [[ "$PERF_TARGET_MET" -eq 0 ]]` → **EXIT=3**. The header (lines 45) documents
exit 3 as "byte-identity PASSED but Rust scaling missed the perf target
(informational; v1.0 may ship at exit 3)". So exit 3 is release-acceptable per the
file's own contract. Confirmed — the plan's claim is correct.

---

## 2. Assumptions

### 2.1 Biological rationale — is it correct?

**Correct.** As traced in §1.1, the overlap bump is proportional to the count of R2
calls in the mate-overlap region, which scales with overlapping-base fraction (an
insert-size-vs-read-length property), not with read count or methylation rate. The
plan's framing matches SPEC §7.4's description of `drop_overlap` and the PE default.
The +2.28% empirical value is fully consistent with a longer-insert library where
mates overlap only modestly.

### 2.2 Could `+5%` have been protecting against a real failure mode?

**Argue both sides:**

*For keeping a magnitude floor:* A pure `strictly > D` test catches only a
*complete* failure of `--include_overlap` (count-sum collapses to ≤ D). A Rust bug
that re-adds *most but not all* overlap calls (e.g. an off-by-one in the
keep-predicate that drops the boundary base, or a partial-overlap edge case from
SPEC §7.4's `drop_overlap_partial_overlap_reverse_pair`) would still produce
count-sum > D and slip through. The 5% floor incidentally raised the detection
sensitivity for such partial regressions. So the floor was *accidentally* doing a
small amount of useful work.

*Against:* (1) The 5% number was never calibrated to any partial-failure threshold —
it was a guess at "the bump should be at least this big," and it is provably wrong
for the canonical release dataset, so it actively blocks the release on correct
output. A gate that fires on byte-identical-to-Perl output is worse than useless;
it trains the operator to bypass gates. (2) Partial-overlap correctness is already
covered far more precisely by (a) the per-cell **byte-identity** comparison
Perl↔Rust on the overlap cell's M-bias.txt — which is exact, not a 5% heuristic —
and (b) the dedicated `drop_overlap_*` unit tests in SPEC §7.4 / the test table
(lines 658–669). The differential check's job is a coarse *semantic* sanity guard
("the flag did something in the right direction"), not a precision correctness
check; byte-identity owns precision. (3) The fail-open `else` branch and the
strictly-`>` direction already guard the dangerous direction.

**My judgment:** dropping the floor does not meaningfully weaken the gate, because
byte-identity is the real correctness oracle and it is exact. The differential is
defense-in-depth for the *non*-byte-identity world (different dataset, future
refactor). `strictly > D` is the correct semantic for that role. The plan is right.
(But see Alternatives §5 for an epsilon-floor option that recovers a sliver of the
partial-regression sensitivity at near-zero cost.)

### 2.3 The SPEC §8.3 claim is WRONG — Critical

The plan states (lines 49–52): *"No SPEC change required (SPEC §8.3 does not pin the
5% magnitude; if it does on re-read, a one-line SPEC note will be added)."* I grepped
and read SPEC §8.3. **It does pin 5%**, explicitly, at line 766:

> `overlap`: M-bias data **count-sum** ... **> D's same metric by ≥ 5%.**

So the plan's central scope assumption is incorrect. The SPEC **must** be edited in
this same PR (line 766 `by ≥ 5%` → `(strictly greater; the magnitude is a
per-library insert-size property, not a fixed constant)`), otherwise SPEC and driver
diverge and the next reader re-derives the wrong 5% gate. The plan's contingency
("if it does on re-read…") at least leaves room for this, but it is filed under
"verify during implementation," which risks it being skipped. Promote to a
**required, named edit** with the exact line. This is the single most important
finding in this review.

---

## 3. Efficiency

Negligible. The change removes one arithmetic expansion (`D_COUNTS * 105 / 100`) per
run and one integer comparison branch. No loops, no I/O, no added subprocess. Runtime
impact is unmeasurable. No concerns.

---

## 4. Validation sufficiency

### 4.1 Stage A (instant re-eval) vs Stage B (fresh 2.5h re-run)

**Stage A** re-evaluates `192423276 > 188123599` against the already-on-disk,
byte-verified outputs and asserts the driver would now emit exit 3. This is
*logically* airtight: the only thing that changed in the driver is a constant in the
verdict math; the cell outputs are deterministic and already proven byte-identical,
so re-running cannot change them. As a proof that *the fix produces the intended
verdict*, Stage A is sufficient.

**Strongest argument for requiring Stage B before tagging v1.0:**
1. **Stage A does not exercise the edited code path.** It reasons *about* the new
   code by hand; it never runs the modified `phase_h_pe_matrix.sh`. A typo in the
   edit (e.g. accidentally writing `-lt` instead of `-le`, or breaking the `else`
   arm, or a stray syntax error under `set -euo pipefail`) would be invisible to
   Stage A and caught only by Stage B. Given the file runs under `set -e` with
   associative arrays, a syntax slip is a real risk.
2. **The release-gate *record* is an artifact, not an argument.** RELEASE_CHECKLIST
   asks for a `matrix_verdict.txt` produced by an actual run on a fresh `--out` dir.
   A hand-computed "it would say exit 3" is not that artifact. For a v1.0 tag — a
   one-way door — the provenance should be a real run, not a reconstruction.
3. **Determinism is an assumption, not a guarantee, across a fresh process.** The
   perf numbers (and thus exit 3 vs exit 0) depend on host load; the byte cells are
   deterministic but the *exit code* is partly perf-derived. Only a fresh run
   records the real scaling on the release host.

**Recommendation:** Use Stage A as an immediate sanity check (good — do it), but
require **Stage B** as the v1.0 gate record. The 2.5h cost is trivial against the
cost of tagging v1.0 on an unrun script. At minimum, if Stage B is skipped, run a
**fast syntax+logic smoke**: `bash -n phase_h_pe_matrix.sh` plus a unit-style
harness that sources the comparison block with mocked `OVERLAP_COUNTS`/`D_COUNTS` to
prove `192423276 > 188123599 ⇒ PASS` and `188123599 == 188123599 ⇒ FAIL` and the
`else` arm still FAILs. That closes the "edited code never executed" gap at ~1s cost.

### 4.2 Is exit 3 release-acceptable?

Yes per header line 45 and the speedup-table text (lines 661–662): "v1.0 may
legitimately ship at exit 3." The perf miss is tracked separately (#876 Finding #4 /
#798), out of scope here. Confirmed the driver yields 3, not 0, and that 3 is
shippable. Good.

### 4.3 Gap: no negative test for the new boundary

Neither stage proves the gate *still fires* when it should (e.g. a hypothetical
count-sum == D). Add the mocked-input check above to cover the FAIL direction;
otherwise we only ever observe the PASS direction and could ship a gate that never
fails.

---

## 5. Alternatives

| Option | Trade-off | Recommend? |
|---|---|---|
| **(A) `strictly > D`** (plan's choice) | Tightest always-true invariant; relies on byte-identity for precision. Loses incidental partial-regression sensitivity. | **Yes — primary.** Correct semantic, minimal change. |
| **(B) Epsilon floor** `> D + max(1, D/1000)` (≥0.1%) | Recovers a sliver of partial-regression sensitivity while clearing the +2.28% case. But picks *another* magic magnitude with no principled basis — same class of error as 5%, just smaller. | Optional. Only if reviewers want belt-and-suspenders; I'd skip it — byte-identity already owns precision. |
| **(C) Configurable threshold** `--overlap-min-bump-pct` (default 0) | Most flexible; lets exome/odd libraries tune. But adds a flag + arg-parsing + doc surface to a release harness for a check that byte-identity subsumes. Over-engineered. | No. |
| **(D) Dataset-keyed expected values** (hard-code 192,423,276 for SRR24827378) | Exact, catches *any* drift. But brittle: breaks the moment the fixture or Perl version changes, and the driver already MD5s the BAM + asserts Perl v0.25.1, so a keyed value duplicates that machinery and couples the gate to one dataset. | No. |
| **(E) Drop the overlap differential entirely** | Simplest. The overlap cell already gets full byte-identity Perl↔Rust + cross-N. The differential adds little beyond "flag did something." | Defensible but I'd keep a minimal `> D` guard — it's the only check that survives a future move to a non-byte-identical comparator and costs nothing. |

**Recommendation:** Option **A** (the plan's choice), exactly as written. If
reviewers want extra partial-regression insurance at near-zero cost, Option **B**
with an explicit comment that the epsilon is a "non-zero sanity margin, not a
calibrated threshold." Avoid C/D.

---

## 6. Action items

### Critical
1. **Fix the SPEC §8.3 claim and edit the SPEC.** The plan's statement that
   "SPEC §8.3 does not pin the 5% magnitude" is **false** — line 766 reads
   `> D's same metric by ≥ 5%`. Edit SPEC line 766 in this same PR
   (`by ≥ 5%` → `strictly greater than D's same metric (the bump magnitude is a
   per-library insert-size property, not a fixed constant)`). Make this a named,
   required edit in the implementation outline, not a "verify during implementation"
   contingency. SPEC §8.4 absorption-history note (around lines 928+) optionally gets
   a one-line rev entry.

### Important
2. **Add a FAIL-direction validation** (mocked `OVERLAP_COUNTS`/`D_COUNTS` smoke or a
   `bash -n` + sourced-block check) so the edit is actually exercised. Stage A alone
   never runs the modified code and cannot catch a `-le`/`-lt`/syntax slip under
   `set -e`. Cheap; closes the biggest validation gap.
3. **Require Stage B (fresh full re-run) as the v1.0 gate record**, or at minimum
   document that Stage A is provisional and a Stage B `matrix_verdict.txt` will back
   the tag. Hand-computed exit 3 is not the release artifact RELEASE_CHECKLIST asks
   for, and v1.0 is a one-way door.
4. **Wording-update completeness — line 700.** The plan's wording list (step 2)
   updates lines 36, 414, 699, 773 but **misses line 700**:
   `(--include_overlap accumulates counts at existing positions; rows unchanged)` is
   fine, but the *preceding* line 699 `strictly > D by ≥5%` and line 700 form one
   logical sentence in the emitted speedup_table — confirm the edit to 699 reads
   cleanly with 700 left intact (it does: "strictly > D" + "(--include_overlap
   accumulates...)"). Also double-check there are no other `5%`/`105`/`1.05`
   occurrences after the edit: grep confirms the only ones are lines 36, 414, 503,
   506, 699, 773, and 503/506 are inside the replaced block. Add a final
   `grep -n '5%\|105\|1\.05' scripts/phase_h_pe_matrix.sh` post-edit check to the
   plan to prove zero stragglers.

### Optional
5. Consider Option B (epsilon floor) only if reviewers want partial-regression
   insurance; otherwise A is cleaner.
6. The provenance note (plan step 3) is good; ensure it cites the colossal evidence
   dir `~/phase_h_pe_release_v879fix/` and the exact counts (192,423,276 vs
   188,123,599 = +2.28%) so the next reader sees *why* the floor was wrong, not just
   *that* it changed.
7. Out-of-scope BAM MD5 reconciliation (plan lines 138–140) — fine to defer, but the
   `4a44918c…` vs `9ebec4c9…` discrepancy is worth a tracking issue link in the PR
   description so it isn't lost.

---

## Summary

The core change is correct: `strictly > D` is the tight, always-true invariant for
`--include_overlap` (the bump scales with per-library mate-overlap fraction, not a
fixed 5%), the boundary logic (`-le` ⇒ PASS iff `> D`) is right, the fail-closed
`else` arm is preserved, `OVERLAP_THRESHOLD` has no other references, and the exit
code correctly resolves to 3 (perf-miss, release-acceptable). All cited line numbers
match the live file.

**One Critical correction:** the plan's claim that SPEC §8.3 does not pin 5% is
false — SPEC line 766 explicitly says "by ≥ 5%," so the SPEC must be edited in this
PR. Two Important items: add a FAIL-direction validation (Stage A never executes the
edited code), and prefer Stage B as the real v1.0 gate record over a hand-computed
exit 3.

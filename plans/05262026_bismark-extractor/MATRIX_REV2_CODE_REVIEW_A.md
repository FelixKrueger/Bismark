# Code Review A — PE matrix rev 2: overlap differential `≥5%` → `strictly > D`

**Reviewer:** A (independent, fresh context)
**Commit:** `0e3fd75` on `matrix-rev2-overlap-differential`
**Targets:** `scripts/phase_h_pe_matrix.sh`, `rust/bismark-extractor/SPEC.md:766`
**Date:** 2026-05-29

---

## Summary

The change is **correct and ready to ship**. It replaces an over-specified `count-sum > D + 5%`
floor with the always-true monotonic invariant `count-sum > D` (expressed as `-le` → FAIL),
removes the now-dead `OVERLAP_THRESHOLD` arithmetic, and aligns the SPEC normative pin plus four
operator-facing wording strings. I verified the edit on every axis the review prompt names:

- **Syntax:** `bash -n` passes.
- **Boundary logic:** I extracted the edited block and ran 6 synthetic cases (real-data `>D`,
  `==D`, `D+1`, `D-1`, empty overlap, `D==0`). All 6 produce the intended `PASS_FLAG` — including
  the `== D` FAIL and both fail-closed `else` paths. This is the boundary no real-data stage
  exercises, and it is correct.
- **No dangling reference:** `grep OVERLAP_THRESHOLD` → 0 matches.
- **Five edited sites agree** (assertion + comments 36/414, emitter 709, verdict 783) and the SPEC
  matches the script.
- **Residual `5%`/`≥5%` hits are all historical rationale in comments** (script lines 502/506,
  SPEC:766 parenthetical), never live logic.

No issues rise above **Low**. No fixes were required. The one previously-flagged Critical (the
plan's "no SPEC change required" premise) is already resolved in this commit — SPEC:766 is edited.

---

## Issues by area

### 1. Logic — correct

- **Assertion (script 513–521):** `PASS iff NOT (OVERLAP_COUNTS -le D_COUNTS)` ⇒ `PASS iff
  OVERLAP_COUNTS > D_COUNTS`. This is exactly "strictly > D." `-le` is the right operator for the
  stated contract (fail on `==D`, since zero net overlap = `--include_overlap` no-op = regression
  on a WGBS library). Confirmed against my 6-case harness.
- **Fail-close:** the outer guard `[[ -n "$OVERLAP_COUNTS" && -n "$D_COUNTS" && "$D_COUNTS" -gt 0 ]]`
  is preserved verbatim; the `else` still sets `PASS_FLAG=0` with a distinct "unreadable" detail.
  Empty/zero/missing counts force FAIL. Confirmed by my `""` and `D==0` cases.
- **No dangling `OVERLAP_THRESHOLD`:** the assignment and its sole consumer were both inside the
  replaced block; grep confirms zero remaining references. The `D_COUNTS -gt 0` guard is now
  strictly unnecessary (no division remains) but harmless and consistent with the file's defensive
  style — leave it.
- **FAIL-detail message** (`not > D=$D_COUNTS`) and the **verdict REASON** (`count-sum>D for
  overlap`) and the **speedup-table** line (`strictly > D`) are all consistent with the new logic.
  The dropped `+ 5% threshold=$OVERLAP_THRESHOLD` suffix in the FAIL detail is correct — that
  variable no longer exists.

### 2. Errors / bash pitfalls — none

- Under `set -euo pipefail`: the edited block is pure `[[ ]]` integer tests inside `if`, no command
  substitution, no pipelines, no unset-variable risk (all operands are `-n`-guarded before the
  inner `-le`). A failing `[[ ]]` inside an `if` condition does not trip `set -e`. No new exit path.
- The inner `-le` lacks the `2>/dev/null` that the three row-count tests (490/494/498) carry, but
  the outer guard already proves both operands are non-empty positive integers, so the inner test
  cannot emit an arithmetic diagnostic. Not a bug; see Low-1 for the optional symmetry note.
- Removing the `* 105 / 100` arithmetic also removes the only place integer truncation could have
  mattered — a non-issue now.

### 3. Structure / consistency — all 5 sites agree; SPEC matches

| Site | Line | Text after edit | OK |
|---|---|---|---|
| Header comment | 36, 39 | `strictly > D` + "magnitude is per-library … monotonic > D … rev 2" | ✓ |
| Inline comment block | 502–512 | full rev-2 rationale + documented `-le` decision | ✓ |
| Pre-block comment | 415 | `strictly > D (… rev 2 dropped the +5% floor)` | ✓ |
| speedup_table emitter | 709 | `strictly > D` (line 710 continuation reads cleanly, no `5%`) | ✓ |
| verdict REASON | 783 | `count-sum>D for overlap` | ✓ |
| SPEC §8.3 | 766 | `strictly > D's same metric` + rev-2 rationale | ✓ |

`grep -nE '5%|105|1\.05|OVERLAP_THRESHOLD'` confirms the only surviving hits are the rev-2
rationale comments (script 502/506) and the SPEC:766 parenthetical — **historical context, not
live logic.** The line-501 orphan-comment risk raised in plan-review A was handled: the old
one-liner is fully replaced by the new 11-line block. The line-700 continuation flagged by both
plan reviewers carries no `5%` and reads correctly after the 709 edit.

### 4. Validation — boundary covered in-session, not committed

The plan's Stage-0 synthetic boundary test (`==D` FAIL, `D+1` PASS, empty/`D==0` fail-closed) was
run in-session, not committed as a unit test. I independently re-ran the equivalent 6 cases here
and they all pass. For a single-purpose release-harness bash script with no existing test target,
this is an **acceptable gap** — the assertion is 4 lines of guarded integer comparison, the
boundary is now documented in an 11-line comment, and Stage B (the fresh full re-run) is the
canonical gate artifact. See Low-2 for an optional hardening.

---

## Fixes applied

None. The commit is correct as written; no unambiguous low-risk defect was found.

---

## Recommendations

### Critical
None.

### High
None.

### Medium
None.

### Low
1. **(Optional, defense-in-depth)** Mirror the `2>/dev/null` from the row-count tests
   (script 490/494/498) onto the inner `[[ "$OVERLAP_COUNTS" -le "$D_COUNTS" ]]` at line 514, so a
   future refactor that weakens the outer guard cannot turn an arithmetic diagnostic into a
   fail-open. Safe to skip today — the outer `-n && -gt 0` guard makes the inner test diagnostic-free.
2. **(Optional)** If a tests target for the harness is ever added, commit the Stage-0 boundary
   cases (especially `count-sum == D` → FAIL, which no real-data stage hits) as a millisecond unit
   test. Not required for this release-gate script; the in-session run + documented comment suffice.
3. **(Tracking only)** The `--skip-overlap-differential` escape hatch already advertised in the
   pre-flight error text (script ~155) remains the correct future handling for a genuinely-disjoint
   long-insert library that could legitimately produce `count-sum == D`. Out of scope here; the
   `-le` fail-on-equality is the right default for the WGBS-target gate.
</content>
</invoke>

# Code Review B — PE matrix rev 2: overlap differential `≥5%` → `strictly > D`

**Reviewer:** B (independent, fresh context)
**Commit:** `0e3fd75` on `matrix-rev2-overlap-differential`
**Targets:** `scripts/phase_h_pe_matrix.sh`, `rust/bismark-extractor/SPEC.md:766`
**Date:** 2026-05-29

---

## Summary

The recalibration is **correct and fail-closure is preserved**. Dropping the `+5%`
floor to a strict `> D` (encoded as `-le` ⇒ FAIL) is the right invariant: the
`--include_overlap` count bump scales with per-library mate-overlap-base fraction,
so the `+5%` magnitude false-FAILed byte-identical (Perl≡Rust) output at +2.28% on
SRR24827378_10M. The `OVERLAP_THRESHOLD` arithmetic is fully removed (repo-wide grep:
**0 live references**), `bash -n` passes, the fail-closed `else` arm is byte-for-byte
preserved, and the verdict ladder still resolves to exit 3 (perf-miss, v1.0-shippable)
on the release dataset.

**One real issue found that Reviewer A may miss:** the commit left a live, operator-facing
straggler in `RELEASE_CHECKLIST.md:179` still asserting "strictly > D **by ≥5%**" — the
exact gate-vs-doc contradiction this PR set out to remove, just relocated to the checklist.
I fixed it directly (low-risk doc edit). No other live-logic or normative-doc straggler
remains.

Verdict: **Approve** (with the one doc fix applied). No fail-open regression introduced.

---

## Area 1 — Fail-open regression risk (the asymmetric `>` cell)

**Did the edit weaken fail-closure? No.** Traced the full flow:

- `ROW_COUNT_OK=0` initialized fail-closed (line 421) and is flipped to `1` only at
  line 524 inside `if [[ "$PASS_FLAG" -eq 1 ]]`, which itself sits inside the
  "all 5 M-bias files present" guard (line 478+). Untouched by the commit.
- `PASS_FLAG=1` is set at line 488, then each of the 4 differential asserts can only
  ever **drive it to 0** (never back to 1). The overlap block (513–521) preserves this:
  - Outer guard `[[ -n "$OVERLAP_COUNTS" && -n "$D_COUNTS" && "$D_COUNTS" -gt 0 ]]` is
    unchanged. Its `else` arm still `PASS_FLAG=0` on empty/unreadable/`D==0` — the
    fail-open class the SPEC §8.3:767 header warns about stays **closed**.
  - Inner test changed from `-le "$OVERLAP_THRESHOLD"` to `-le "$D_COUNTS"`. Boundary:
    PASS iff `NOT (OVERLAP ≤ D)` ⇒ PASS iff `OVERLAP > D` (strictly). `== D` correctly
    FAILs. The `-le` direction (fail-on-equality) is retained deliberately and is now
    documented in the new comment block (510–512). Correct.

**Does relaxing the threshold let a real Rust regression slip through that +5% caught?**
Argued concretely: the only regressions that would have tripped `> D·1.05` but now pass
`> D` are *partial* overlap regressions whose bump lands in the `(0%, 5%]` band — e.g. an
off-by-one in the keep-predicate that drops a boundary base. **But this band is not a
defensible detector**: it is provably populated by *correct* output (the +2.28% byte-identical
case lives there), so any FAIL in that band is at least as likely a false positive as a true
catch. More importantly, partial-overlap correctness is owned **exactly** by the per-cell
Perl↔Rust **byte-identity** `cmp` on the overlap cell's `M-bias.txt` (an exact oracle, not a
heuristic), plus the `drop_overlap_*` Rust unit tests. The differential's job is the coarse
*semantic* "did `--include_overlap` do something in the right direction" guard; the strict
`> D` is the correct, parameter-free expression of that role and the dangerous direction
(`≤ D`, i.e. include_overlap behaving like no_overlap / polarity inversion) remains caught.
**No fail-open introduced; sensitivity loss is confined to a band that was never a sound
detector.**

---

## Area 2 — Exit-code ladder

Traced lines 766–794:

- USAGE → 2; any byte FAIL → 1; cross-N FAIL → 1; M-bias baseline drift → 1;
  `ROW_COUNT_OK -eq 0` (differential FAIL) → **1** (line 781); then
  `PERF_TARGET_MET -eq 0` → **EXIT=3** (line 785).
- On the release dataset post-fix: `OVERLAP_COUNTS=192,423,276 > D=188,123,599` ⇒ overlap
  asserts PASS ⇒ `ROW_COUNT_OK=1`; all byte cells PASS; cross-N PASS; baseline OK;
  `PERF_TARGET_MET=0` (0.58× scaling). The ladder therefore lands on line 785 → **exit 3**,
  **not 0**. Confirmed.
- Header line 46 documents exit 3 as "byte-identity PASSED but Rust scaling missed the perf
  target (informational; **v1.0 may ship at exit 3**)". The verdict REASON (line 786) matches.
  Consistent with the task's expectation.

---

## Area 3 — Consistency (script ↔ SPEC ↔ other docs)

- **Verdict REASON (line 783):** now reads `count-sum>D for overlap` — reads correctly. ✓
- **Speedup-table emitter (old line 699, now ~715–716):** the magnitude is dropped from the
  `Cells:` echo block; line 715 reads `strictly > D` and the continuation line
  (`--include_overlap accumulates counts at existing positions; rows unchanged`) reads
  cleanly with it. The PASS/FAIL emitter at 712–720 is metric-agnostic and unaffected. ✓
- **SPEC.md:766:** edited `≥ 5%` → `strictly > D's same metric` with full rev-2 rationale.
  SPEC §8.3:767 fail-open warning and surrounding bullets remain valid. **Script and SPEC are
  now mutually consistent.** ✓
- **`OVERLAP_THRESHOLD`:** repo-wide grep across `scripts/` and `rust/` → **0 matches**. Fully
  removed, no dangling reference. ✓
- **Stragglers in other docs the task named:**
  - `RELEASE_CHECKLIST.md:179` — **WAS a live straggler** asserting `strictly > D by ≥5%`.
    This is an operator-facing gate item read against the emitted `speedup_table.md`; leaving
    it would re-create a doc-vs-gate contradiction. **Fixed directly** (see Fixes Applied).
  - `plans/05262026_bismark-extractor/PHASE_H_PE_PLAN.md` — ~15 occurrences of `> D + 5%`.
    These are **historical rev-1 plan prose / design-rationale / edge-case tables**, not a
    live contract or operator checklist. Per repo convention (and the user's CLAUDE.md "do not
    read/modify plans unless asked"), rev-2 supersedes via `MATRIX_REV2_OVERLAP_DIFFERENTIAL_PLAN.md`
    rather than retro-editing the rev-1 plan. **Left as-is intentionally; flagged Low below.**
  - Remaining `5%` hits inside `phase_h_pe_matrix.sh` (lines 415, 502, 506) are all
    rev-2 rationale comments explaining *why the floor was dropped* — correct, not stragglers.

---

## Area 4 — Test gap (new `== D` boundary)

There is **no committed automated test** for the new boundary; only an in-session synthetic
Stage-0 check (6/6, recorded in the plan's implementation notes). Assessment of repo
conventions:

- `scripts/` contains only the three harness drivers + one Python helper. **There is no bats
  or shell-test framework anywhere in the repo** (grep for `@test`/`bats` → none); the harness
  scripts *are* the test infrastructure, and unit tests live in `rust/`. Introducing bats here
  would add a brand-new dependency/convention for a single assertion.
- The `== D` boundary is the one case neither real-data stage (Stage A/B, both at +2.28%)
  exercises, and a `-le`→`-lt` slip or a future weakening of the outer guard would be a
  fail-open regression invisible to Stage A.

**Recommendation (Medium):** rather than a new bats suite, add a tiny self-test path to the
existing harness or a committed `scripts/phase_h_pe_matrix.selftest.sh` (~10 lines) that sources
the differential block with mocked `OVERLAP_COUNTS`/`D_COUNTS` and asserts:
`D+1 ⇒ PASS`, `== D ⇒ FAIL`, `D-1 ⇒ FAIL`, empty/`D==0 ⇒ FAIL`. This pins the boundary in CI-able
form at ~1s cost and matches the existing "harness validates itself" pattern. Acceptable to defer
given the in-session Stage-0 evidence, but the boundary is currently asserted only by code-reading.

---

## Fixes applied

1. **`RELEASE_CHECKLIST.md:179`** — changed the operator verify-item from
   `strictly > D by ≥5%` to `strictly > D`, with a rev-2 note that the `≥5%` floor was dropped
   because the bump magnitude is a per-library mate-overlap property. Removes the last live
   doc-vs-gate contradiction. (Unambiguous, low-risk, and explicitly in the task's grep scope.)

---

## Recommendations

### Critical
*(none)*

### High
*(none)* — the SPEC edit both plan-reviewers flagged is present and correct.

### Medium
- **M1.** Add a committed millisecond boundary self-test (`== D ⇒ FAIL`, `D+1 ⇒ PASS`,
  empty/`D==0 ⇒ FAIL`) sourcing the differential block, in keeping with the repo's
  "harness validates itself" convention (no bats framework needed). Closes the one boundary
  no real-data stage exercises. (Area 4.)

### Low
- **L1.** `plans/05262026_bismark-extractor/PHASE_H_PE_PLAN.md` retains ~15 `> D + 5%`
  references in rev-1 prose. Harmless (superseded by the rev-2 plan + this PR), but a single
  "rev 2 supersedes the +5% magnitude" pointer at the top of that file would spare a future
  reader the diff archaeology. Optional; do not retro-edit the rev-1 body.
- **L2.** (Defense-in-depth, from the existing row-count asserts at 490/494/498) the three
  `<D` tests carry a trailing `2>/dev/null`; the overlap inner `[[ -le ]]` does not. Safe today
  because the outer `-n && -gt 0` guard proves integer operands before the inner test, but
  mirroring `2>/dev/null` would harden against a future weakening of that guard. Optional.
- **L3.** Stage B (fresh full re-run) is the canonical v1.0 gate artifact, not the
  hand-computed Stage A; the plan already launched it on colossal. Confirm the
  `matrix_verdict.txt` from `~/phase_h_pe_release_v879fix_rev2/` shows exit 3 + no `[FAIL …]`
  on the overlap line before tagging. (Process, not code.)

---

## Most important findings (top 3)

1. **Fail-closure intact, no fail-open introduced.** The relaxation only loses sensitivity in
   the `(0%, 5%]` bump band, which is provably populated by correct byte-identical output and
   is owned exactly by the per-cell Perl↔Rust byte-`cmp`. The dangerous `≤ D` direction is
   still caught. (Area 1)
2. **One live doc straggler fixed:** `RELEASE_CHECKLIST.md:179` still said `by ≥5%` — the same
   gate-vs-doc contradiction class this PR removes. Fixed directly. SPEC + script + checklist
   are now mutually consistent; `OVERLAP_THRESHOLD` has 0 live references. (Area 3)
3. **Boundary has no committed test:** the `== D ⇒ FAIL` case is exercised only by an
   in-session synthetic check, never by a real-data stage. Recommend a ~10-line committed
   self-test (Medium) — no new framework, matches repo convention. (Area 4)

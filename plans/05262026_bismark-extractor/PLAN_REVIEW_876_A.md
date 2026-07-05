# Plan Review A — BUG_876_FIXES_PLAN.md

**Reviewer**: A (independent, fresh context)
**Plan**: `plans/05262026_bismark-extractor/BUG_876_FIXES_PLAN.md`
**Verdict**: **Approve with one Critical correction (parallel.rs back-port) and three Important items.**

The two root-cause diagnoses are correct, the proposed fixes are minimal and well-localized, and the test set is reasonable. The plan as written, however, will ship Bug B fixed in the single-threaded driver only and silently leave it broken in the parallel worker — exactly the dual-driver back-port trap that bit #874 and the count_mbias_rows polish on `778f49d`.

## 1. Logic review

### 1.1 Bug A diagnosis — confirmed
`cli.rs:467-471` resolves `no_overlap = !include_overlap` whenever `paired_mode != SingleEnd`, which catches the `AutoDetect` case. The writer at `output.rs:643` then unconditionally emits the line. Gating the writer on `is_paired && config.no_overlap` is correct, `is_paired` is already a parameter to `write_splitting_report` (signature at `output.rs:574-580` — confirmed), and the call site at `state.rs:149-155` passes the post-detection `self.is_paired`. The fix is sound and one-line.

### 1.2 Bug B diagnosis — confirmed
`call.rs:177` sets `read_pos = aligned.read_pos_5p` (absolute 5'-oriented, includes soft-clip). The filter at `call.rs:162` drops emission for positions `< ignore_5p`, but surviving emissions keep their absolute index. `route.rs:95` then does `read_pos + 1` → M-bias slot N+1 instead of slot 1. Perl rebases the *string* at `:1627`. Both rebase strategies (transform the position vs trim the input) produce identical M-bias output.

### 1.3 **CRITICAL — parallel.rs is missing from the fix scope**
Bug B exists at **three** sites, not one:
- `route.rs:95` — single-threaded driver (called from `pipeline.rs:158, 360, 363`)
- `parallel.rs:625` — parallel SE worker (`process_se`)
- `parallel.rs:729` — parallel PE worker, R1
- `parallel.rs:752` — parallel PE worker, R2

All four lines have the identical `call.read_pos.saturating_add(1)` pattern with no rebase. The plan's §3 ("Bug B fix — `route.rs:95`") and §5 implementation order do not mention `parallel.rs` at all. The Phase H matrix runs the **single-threaded driver** at N=1 (where the bug is visible in the test report), but `parallel.rs` is what runs under `--parallel ≥2` and is what production users will hit. Leaving it unfixed will cause Bug B to silently reappear once N>1 cells use the parallel path — and the SE matrix at N=4 will continue to FAIL on Bug B after this plan lands.

This is exactly the **dual-driver back-port trap** memory: independent drivers ship infrastructure bugs twice. The plan even cites the awareness in §8 ("Bug A would affect PE matrix identically — fix once") for Bug A, but misses that Bug B has a sibling site in `parallel.rs`.

Required additions:
1. Apply the same rebase at `parallel.rs:625` (SE worker, uses `config.ignore_5p_r1`).
2. Apply the same rebase at `parallel.rs:729` (PE R1, uses `config.ignore_5p_r1`).
3. Apply the same rebase at `parallel.rs:752` (PE R2, uses `config.ignore_5p_r2`).
4. Add a unit test in `parallel.rs` (or an integration test) that exercises the worker M-bias path with `--parallel 2 --ignore 5` and asserts row #1 has the count that the SE driver produces for the same input.

If the plan keeps route_call as the rebase site, the parallel-worker sites do not benefit from the centralization — they need their own three edits. This argues for **Choice 2 (rebase at call.rs:177)** as the cleaner single-edit alternative, see §1.5 below.

### 1.4 B2 open question — answered by grep
`call.read_pos` is consumed at exactly four sites, all of which are M-bias accumulator inputs:
- `route.rs:95`, `parallel.rs:625`, `parallel.rs:729`, `parallel.rs:752`.

`MethCall.read_pos` is **not** referenced by any downstream split-file writer, yacht column derivation, or PE overlap-drop logic (`drop_overlap` keys on `ref_pos`, not `read_pos`). The split-file writer (`fhs.write_call` at `route.rs:138`) takes `call`, `strand`, `yacht_col6`, `yacht_col7` — `MethCall.read_pos` is not present in the output line at all (Perl emits position via SAM `POS + offset`, not read-relative). So the plan's B2 "Choice 2 rejected because semantics may matter downstream" is **factually wrong**: there are no other consumers.

### 1.5 Choice 1 vs Choice 2 — recommend reconsidering Choice 2
Given that `MethCall.read_pos` has only M-bias-accumulator consumers, **Choice 2 (rebase at call.rs:177)** has three concrete advantages:
- Single edit point, fixes all four call sites (route.rs + 3× parallel.rs) atomically.
- The plumbing change `extract_calls` → `MethCall` already has `ignore_5p` in scope at `call.rs:145` (it's an existing parameter); no signature change needed.
- The `MethCall.read_pos` docstring at `call.rs:32-39` becomes truthful: the field would be "read position relative to the first un-clipped emitted base, 0-based", matching Perl's `substr` semantic.

The plan's stated concern about "broader scope" (§3 "Choice 2 Rejected") was reasonable defensively but doesn't survive the grep result. If the implementer keeps Choice 1, the parallel.rs back-port is mandatory (see §1.3).

### 1.6 Pipeline.rs call site lines — verified
- SE: `pipeline.rs:158` (the plan says ~L142-143; actually 142-143 is the `extract_calls` site, the `route_call` is L158). Minor citation error but the same scope.
- PE R1: `pipeline.rs:360`, PE R2: `pipeline.rs:363` (plan says ~L342-349 — that range is the `extract_calls` calls, not `route_call`). Same minor citation error.
The plan's intent is clear; just update the line refs in the implementation step so the implementer doesn't get confused.

## 2. Assumptions surfaced

- **Assumed**: Phase H matrix at N=1 exercises the single-threaded path (`route.rs:95`). Confirmed: `pipeline.rs::extract_se` is the entry point and at `--parallel 1` Felix's `phase_h_se_matrix.sh` should dispatch through it. **Implementer must verify** by re-running with the post-fix binary that the matrix at N=4 also passes — that proves the parallel-worker rebase landed too.
- **Assumed**: `is_paired` in `write_splitting_report` reflects post-BAM-detection state. Confirmed at `state.rs:153`: it passes `self.is_paired` which is set during BAM open (`state::new`).
- **Implicit**: Bug A's fix does not regress the PE-default case. The proposed gate `is_paired && config.no_overlap` keeps the line emitted when PE is detected and `--include_overlap` is not passed (the resolver leaves `no_overlap = true`). Test #6 covers this.
- **Implicit**: `ignore_5p_for_identity` for `ReadIdentity::Single` should map to `ignore_5p_r1` (since SE has no R2). The plan states this in §3 but the signature should make it physically impossible to pass `ignore_r2` for an SE record. Consider asserting `read_identity != R2 || identity_ignore_came_from_r2` in debug builds, or pass an enum rather than `u32` for self-documenting safety.

## 3. Efficiency
No concerns. Both fixes are O(1) per call; the `saturating_sub` is a single instruction. No allocation or branching changes on the hot path beyond what's already there.

## 4. Alternatives considered

- **Resolver-side fix for Bug A**: rejected correctly. Restructuring `cli.rs:467` to defer until after BAM open would couple CLI resolution to BAM I/O — much larger blast radius for the same byte-identity outcome.
- **Rebase at extract_calls (Choice 2)**: see §1.5 — actually the better choice given grep results. Recommend the implementer reconsider.
- **Tests**: the plan uses `contains` checks for the overlap line — sensible (T1 default is right). For the M-bias unit tests, the plan does not specify whether to assert the full table or just a single slot. Suggest asserting (a) slot 1 has the count, AND (b) slot 1+ignore_5p has zero or the next call's count — both are needed to prove the rebase, not just the shift.

## 5. Validation sufficiency

**Gaps**:
1. **No parallel-worker test** (Critical — same root cause as §1.3). Without one, Bug B will regress at `--parallel ≥2` and CI will not catch it.
2. **No SE-driver-vs-parallel-driver M-bias equality test** under `--ignore 5`. This is the obvious cross-driver invariant and would have caught the dual-driver gap structurally.
3. **No PE M-bias test for `--ignore_r2`**. The plan's test #2 mentions it but doesn't specify a separate ignore_r1 / ignore_r2 to prove they aren't swapped — important because R1 and R2 use different slots and different config fields.
4. Test #5 (the splitting-report regression-guard for Bug A) requires `ResolvedConfig { paired_mode: AutoDetect, no_overlap: true }`. Check that `ResolvedConfig` can be constructed in a unit test — the field `no_overlap` is public per `cli.rs:485` so this is fine, but the implementer should verify all required fields are constructible (the struct has ~25 fields; a `Default` or builder would help — minor refactor opportunity).

**Adequacy** for the original Phase H matrix verdict: yes, given the regression-guard tests + the colossal re-run in §4 test #8.

## 6. Phase-C-rev-1 history concern — addressed correctly
The plan's choice to gate at the writer rather than revert `cli.rs:461-466` is sound: that broadening was a deliberate fix for an AutoDetect-then-PE leak. Adding the post-detection `is_paired` gate at the writer is the smallest correct intervention and doesn't undo any prior fix. Approve A1.

## 7. §6 "PE matrix excluded" claim — verified sound
Confirmed by reading `output.rs:574-680` and `state.rs:148-156`: there is a single `write_splitting_report` function for both SE and PE. No SE/PE-branched separate writer exists. Bug A's writer-gate fix simultaneously covers both modes. The plan's exclusion is sound.

## 8. Action items

### Critical
- **C1.** Extend Bug B fix to `parallel.rs:625, 729, 752`. Whether via Choice 2 (single edit at `call.rs:177`) or Choice 1 (three additional rebases in parallel.rs). Add a parallel-worker M-bias unit test under `--ignore N>0`. Without this the SE matrix at N=4 will still FAIL on Bug B.

### Important
- **I1.** Re-evaluate Choice 1 vs Choice 2 in light of grep evidence: `MethCall.read_pos` has zero non-M-bias consumers. Choice 2 is one edit; Choice 1 is four. Recommend Choice 2 unless the implementer finds a consumer I missed (which the plan's B2 question can now be closed: NO).
- **I2.** Add an explicit SE-driver-vs-parallel-driver M-bias equality test under `--ignore 5` (e.g., a tiny synthetic BAM + dispatch both paths + assert byte-identical M-bias output). This is the structural regression guard for the dual-driver trap class.
- **I3.** Correct the pipeline.rs line refs in §3: SE `route_call` is L158, PE R1/R2 are L360/363. The plan currently cites the extract_calls block (L142-143, L342-349).

### Optional
- **O1.** Document in the `MethCall.read_pos` doc comment what the post-rebase semantic is (if Choice 2 is adopted), since the docstring currently says "includes soft-clip" which would no longer be precise.
- **O2.** Consider making the regression-guard test #5 the byte-identity diff of two whole splitting_reports (with vs without bug) rather than a `contains` check, to also catch any future cosmetic regression in adjacent lines.
- **O3.** Consider a `cleanup_partial_outputs` audit for the AutoDetect-then-SE case to confirm nothing else in the SE finalize path leaks a PE-only artifact (similar class of bug as Bug A — low likelihood but free to check).

## 9. Summary
Diagnoses are correct, fix shape is right, but the plan only fixes one of four bug sites for Bug B and will not actually make the SE matrix pass at `--parallel ≥2`. The single most important change is to either (a) move the rebase to `call.rs:177` (one edit, fixes all four sites) or (b) explicitly back-port to all three `parallel.rs` sites and add a worker-path regression test. Do not implement until C1 is folded in.

**File path**: `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PLAN_REVIEW_876_A.md`

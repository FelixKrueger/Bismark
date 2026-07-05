# Plan Coverage Report

**Mode:** B (code vs the plan's "What was implemented" / 4 tests + acceptance)
**Plan(s):** `plans/05262026_bismark-extractor/TESTS_878_PLAN.md`
**Code:** worktree `/Users/fkrueger/Github/Bismark-extractor` (uncommitted, detached @ `2bfe722`)
**Date:** 2026-05-29
**Verdict:** COMPLETE

## Summary

- Total items: 9 (4 tests + 5 acceptance/deviation/process items)
- DONE: 6
- PARTIAL: 0
- MISSING: 0
- DEVIATED (documented = acceptable): 3
- Known-pending PR-time item (not a code gap): 1 (`BUG_876_FIXES_PLAN.md §6`)

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | Test 1 `extract_calls_ob_strand_rebases_read_pos_after_ignore_5p` + non-breaking `synth_se_record` → `synth_se_record_strand(xm,n_soft,n_match,xg)` refactor; OB rebase + OT≡OB | Plan §"Behavior" Test 1; impl-notes | DONE | Present in `call.rs` `mod tests`. OT `"..Zxh."` vs OB `".hxZ.."` (= reverse), `ignore_5p=2`, asserts `read_pos == [0,1,2]` for both + OT≡OB + context/meth `[(CpG,true),(CHG,false),(CHH,false)]`. `synth_se_record` retained as `b"CT"` wrapper (non-breaking). |
| 2 | Test 2 `parallel_se_worker_m_bias_rebased` — SE worker → `mbias[0]` slot 1 under `--ignore 3` | Plan §"Behavior" Test 2 | DONE | Present in `parallel.rs` `mod tests`. Calls private `process_se` via `super::*`. XM `"...Zxh"`, asserts CpG/CHG/CHH at rebased slots 1/2/3, absolute slots 4/5/6 zero, `mbias[1]` empty. Includes non-CpG (CHG+CHH) routing. Helpers `synth_rec`/`config_with`/`mbias_total` added. |
| 3 | Test 3 `parallel_pe_worker_m_bias_uses_r2_ignore_for_r2` — R1→`mbias[0]`, R2→`mbias[1]` under `--ignore 3 --ignore_r2 7` | Plan §"Behavior" Test 3 | DONE | Present in `parallel.rs` `mod tests`. Calls private `process_pe`. R1 `"...Z.."` @start 100, R2 `".Z......."` @start 200 (non-overlapping → `drop_overlap` keeps R2). Asserts `mbias[0].cpg[1].meth==1`, `mbias[1].cpg[1].meth==1`, absolute slots 4/8 zero, each table total==1. |
| 4 | Test 4 `se_driver_vs_parallel_driver_m_bias_equality` — single vs parallel driver M-bias equality + non-emptiness assertion | Plan §"Behavior" Test 4 | DONE | Present in `tests/parallel_phase_f.rs`. Uses `--ignore 2` (per C1, NOT the rev-0 `--ignore 5`), counts non-header CpG/CHG/CHH lines (`call_lines > 0`), then `assert_dirs_byte_identical(single, parallel-n4)`. Doc-comment states it guards divergence not revert. |
| 5 | Acceptance (a): all 4 tests pass | Plan §Validation 1 | DONE | All 4 named tests PASS. Full suite green: lib **105**, `parallel_phase_f` **18** (matches expected). |
| 6 | Acceptance (b): Tests 1–3 FAIL on reverting `call.rs:204`; Test 4 stays green | Plan §Validation 2 | DONE | Revert-smoke executed (Edit-tool revert; sandbox blocked `perl -i`/`cp` to /tmp): Tests 1,2,3 FAILED (+ pre-existing #876 OT guards `extract_calls_rebases_read_pos_after_ignore_5p`, `extract_calls_rebase_combined_with_soft_clip`). Test 4 stayed GREEN. Fix restored (md5 matches pre-revert backup). |
| 7 | Acceptance (c): NO source `*.rs` logic changes (only test-helper refactor) | Plan §Validation; impl-notes | DONE | `git diff` touches 3 files; all `call.rs`/`parallel.rs` hunks lie inside `#[cfg(test)] mod tests` (call.rs hunks @237+, mod tests @223; parallel.rs hunk @1141, mod tests @1070). `call.rs:204` rebase fix (`saturating_sub`) intact. |
| 8 | Documented DEVIATED: `process_se`/`process_pe` arg order is `(record/pair, chr_id, chr_table, config, …)` not `(…, config, chr_id, chr_table)` as rev-1 sketched | impl-notes Deviation 1 | DEVIATED (documented) | Tests call the actual signature; deviation documented. Acceptable. |
| 9 | Documented DEVIATED: R2 of OT pair is CTOT (`-`-strand reversed); R2 `Z` placed at BAM idx `seq_len-1-ignore_r2` (9-1-7=1). Plus `--mbias_only` adopted for Tests 2–3 | impl-notes Deviations 2 & 3 | DEVIATED (documented) | Empirically confirmed; pins exact placement vs rev-1's "control R2 orientation". `--mbias_only` adopted as the optional isolation path. Acceptable. |

## Known-pending PR-time item (per plan, NOT a code gap)

- **`BUG_876_FIXES_PLAN.md §6` update** (#8/#9/#10 deferrals → mark landed). Plan impl-notes explicitly list this as "Remaining: update `BUG_876_FIXES_PLAN.md §6` … at PR time." Verified: §6 of `BUG_876_FIXES_PLAN.md` currently lists out-of-scope items with no #8/#9/#10 "landed" markers — confirming the update has not yet been applied. Per the plan this is a PR-time documentation task, not a coverage gap in the #878 test work.

## Gaps (detail)

None. All 4 tests exist, do what the plan specifies, pass green, and produce the required revert-smoke behavior. The three DEVIATED items are documented in the plan's "Deviations" section (process arg order, R2=CTOT-reversed placement, `--mbias_only` adoption) and are acceptable.

## Test verification (Mode B)

| Test name | File | Status (green run) | Status on `call.rs:204` revert |
|-----------|------|--------------------|--------------------------------|
| `extract_calls_ob_strand_rebases_read_pos_after_ignore_5p` | `rust/bismark-extractor/src/call.rs` (tests mod) | PASS | FAILED (as required) |
| `parallel_se_worker_m_bias_rebased` | `rust/bismark-extractor/src/parallel.rs` (tests mod) | PASS | FAILED (as required) |
| `parallel_pe_worker_m_bias_uses_r2_ignore_for_r2` | `rust/bismark-extractor/src/parallel.rs` (tests mod) | PASS | FAILED (as required) |
| `se_driver_vs_parallel_driver_m_bias_equality` | `rust/bismark-extractor/tests/parallel_phase_f.rs` | PASS | PASS (divergence guard, stays green by design — A-I4) |

Suite-level: `cargo test -p bismark-extractor` → lib `105 passed`, `parallel_phase_f 18 passed`, all other binaries green (0 failures). Matches the expected counts (lib 105, parallel_phase_f 18).

Revert-smoke note: the sandbox blocked in-place writes to the source file via `perl -i`/`cp` (Operation not permitted; a stale cached read briefly masked this). The revert was applied with the Edit tool, tests run, then restored — final `call.rs` md5 is byte-identical to the pre-revert backup, and the git diff afterward contains only test-module hunks.

## Verdict

**COMPLETE.** All 4 regression-guard tests are present, behave as specified, pass on the green run, and satisfy the #878 acceptance: Tests 1–3 fail on reverting the `call.rs:204` rebase, Test 4 stays green as the divergence guard, and there are no source `*.rs` logic changes (only the non-breaking `synth_se_record` → `synth_se_record_strand` test-helper refactor). The three deviations are documented (process arg order; R2=CTOT-reversed; `--mbias_only` adopted) = acceptable. The `BUG_876_FIXES_PLAN.md §6` update is a known-pending PR-time documentation item per the plan, not a code gap.

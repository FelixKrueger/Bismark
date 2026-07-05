# Plan Review — BUG_876_FIXES_PLAN.md (rev 1, round 2)

**Reviewer**: B (round 2, fresh context — distinct from round-1 B)
**Plan**: `plans/05262026_bismark-extractor/BUG_876_FIXES_PLAN.md` (rev 1)
**Verdict**: **Approve. Two Optional notes; nothing blocking.** rev 1 cleanly absorbs both round-1 reports' Critical findings (parallel.rs scope, Choice 2 rebase) and the Important findings (Perl line-citations, PE smoke, dual-dispatch equality test). Independent verification below confirms the plan's empirical claims.

## 1. Independent verification of rev 1's load-bearing claims

| Claim | Verified at | Result |
|---|---|---|
| 4 sites use `call.read_pos.saturating_add(1)` | `grep -rn "saturating_add(1)" rust/bismark-extractor/src/` → `route.rs:95`, `parallel.rs:625, 729, 752` | Confirmed exactly 4, all M-bias accumulator inputs |
| `MethCall.read_pos` has zero non-M-bias consumers | `write_call` (output.rs:170-219), `write_yacht_row` (output_mode.rs:191-219), `drop_overlap` (overlap.rs:84-109) — all take `MethCall` or `Vec<MethCall>` but reference only `ref_pos`, `xm_byte`, `context`, `methylated` | Confirmed; rebase at construction is structurally safe |
| `ignore_5p` is in scope at `call.rs:177` | `extract_calls` signature at L135-140 takes `ignore_5p: u32`; used at L145 (`lo = ignore_5p`) | Confirmed |
| Filter at L162 guarantees `read_pos_5p >= ignore_5p` at L177 | L162: `if aligned.read_pos_5p < lo … continue;` where `lo = ignore_5p` | Confirmed — `saturating_sub` is genuinely defensive (see Optional O1) |
| Parallel workers call `extract_calls` with per-identity ignore | `parallel.rs:607-612` (SE→`ignore_5p_r1`), `:698-703` (R1→`ignore_5p_r1`), `:704-709` (R2→`ignore_5p_r2`) | Confirmed — Choice 2 inherits correct routing for all 4 sites |
| Perl `$no_overlap` assigned only in PE branch | `grep -n 'no_overlap' bismark_methylation_extractor`: L931 declaration; L1219 + L1224 (both inside L1215-block titled "--no_overlap is the default … for paired-end"). All other refs (L968 option, L1345 return, L2440/2448/2815/2891/3562/5037/5826) are reads/pass-throughs | Confirmed — V1 / I4 absorption is accurate |
| `write_splitting_report` already takes `is_paired` | output.rs:574-580; called from state.rs:149-155 passing `self.is_paired` (post-detection) | Confirmed — no signature change needed |
| `parallel.rs` already has test infrastructure | parallel.rs:968+ has `#[cfg(test)]` mod with realistic worker spawning (`Cli::try_parse_from`, tempfiles, crossbeam channels) at L1043-onwards | Tests 8/9/10 are feasible without new fixture infra |

## 2. Round-1 absorption check

All Critical / Important items from PLAN_REVIEW_876_A.md and PLAN_REVIEW_876_B.md are addressed in rev 1:

- A-C1 (parallel.rs scope) → addressed structurally by Choice 2 (§3); §4 tests 8-10 cover the worker path.
- A-I1 (`ignore` enum vs `u32` self-documenting safety) → moot under Choice 2 (single rebase site, no signature widening).
- A-I2 (cross-driver equality test) → §4 test 10.
- A-I3 (M-bias assertion = slot-1-has + bug-slot-zero) → §7 T1.
- A-O1 (docstring truthfulness) → §3 docstring update at L32-39.
- A-O3 (cleanup_partial_outputs) → explicitly deferred in §6.
- B-C1 (parallel.rs back-port) → Choice 2 + tests 8-10.
- B-C2 (PE smoke) → §4 test 12.
- B-I1 (B2 zero-consumer verification) → §3 verification table + §7 B2 row.
- B-I3 (state-side clearing alternative) → §2 "Alternative considered" — kept as latent option.
- B-I4 (Perl line citations) → §2 Perl-reference paragraph cites L931, L1219, L1224.
- B-O2 (squashed commit ordering) → §5 implementation order.

Nothing dropped.

## 3. Findings (round 2)

### Critical
None.

### Important
None.

### Optional

**O1. `saturating_sub` is defensive-not-necessary — worth a one-line code comment but not a behavior change.** The L162 filter guarantees `aligned.read_pos_5p >= ignore_5p`, so `read_pos_5p - ignore_5p` cannot underflow. The plan's inline comment at proposed L128 already says "saturating_sub is safe but used for defense-in-depth" — good. No change requested. Keep the saturating form for future-proofing against filter logic changes.

**O2. §4 test 12 (PE smoke) — consider expanding to 2 cells.** The plan specifies "1-cell PE smoke `--ignore 4 --parallel 1`". One cell at `--parallel 1` exercises the route.rs:95 path only; the PE-worker bug sites at `parallel.rs:729, 752` are NOT covered by this smoke. Unit tests 8-10 cover them at the synthetic level, but if Felix wants end-to-end evidence on real data before resuming v1.0 walk, recommend either (a) running the smoke at `--parallel 4` to hit parallel.rs:729+752, OR (b) two cells: one at `--parallel 1`, one at `--parallel 4`. This is cheap (smoke ≠ full matrix) and gives independent confirmation of the parallel-worker fix on real BAM. Filed Optional because tests 8+9 already cover the same logic at unit level.

## 4. Validation sufficiency

Adequate. Tests 1-4 cover Bug A (SE + AutoDetect + PE default + PE-include_overlap). Tests 5-7 cover Bug B at the call-site + downstream M-bias slot. Tests 8-10 cover the worker path and the dual-driver equality invariant (the structural regression-guard for the trap class). Test 11 = colossal SE matrix re-run on fresh `--out`. Test 12 = PE smoke. The only gap is real-data PE parallel-worker coverage (see O2).

## 5. Recommendation

**Approve rev 1 as-is.** Optional O2 (smoke-at-N=4) is a nice-to-have; if Felix accepts it the plan needs a 1-line edit at §4 test 12. Otherwise no changes needed. Tests 8-10 carry the structural load.

Report file: `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PLAN_REVIEW_876_REV1_B.md`

# Plan Review B — BUG_876_FIXES_PLAN.md

**Reviewer:** Plan Reviewer B (independent, fresh context)
**Plan:** `plans/05262026_bismark-extractor/BUG_876_FIXES_PLAN.md`
**Verdict:** Sound diagnosis, but the Bug B fix scope is **materially incomplete** — three additional call sites are missed and would silently re-introduce the bug under `--parallel >= 1`.

---

## Critical findings

### C1. Bug B fix omits three M-bias sites in `parallel.rs`

`grep -n "call.read_pos.saturating_add(1)"` returns FOUR sites, not one:

- `route.rs:95` — covered by the plan.
- `parallel.rs:625` — SE worker M-bias accumulate.
- `parallel.rs:729` — PE R1 worker M-bias accumulate.
- `parallel.rs:752` — PE R2 worker M-bias accumulate.

`parallel.rs` is a separate parallel pipeline (not behind `route_call`); fixing only `route.rs` leaves the parallel path broken. The colossal matrix was run at `--parallel N=1` and `N=4` — exactly the path this misses. The plan's §2 claim "All 8 completed cells FAIL" already includes parallel cells; the plan must patch all four. Note these sites are inlined (do not call `route_call`), so the "extend `route_call` signature" approach does NOT fix them — the rebase must also be applied per-site in `parallel.rs` (or via a shared helper).

**Action:** add `parallel.rs:625, 729, 752` to the Bug B fix list. Update unit-test coverage to include `process_se` / `process_pe` paths (or refactor: pull the `pos_1based = call.read_pos - ignore_5p + 1` logic into a tiny shared helper in `call.rs` or `mbias.rs` and call it from all 4 sites — DRY + structurally prevents a 5th regression). Also: `route_call`'s new `ignore_5p_for_identity` parameter is duplicate plumbing if the helper is used; consider passing the value into the helper rather than into route_call's signature.

### C2. PE matrix exclusion (§6) is correct for Bug A but wrong for Bug B

§6 says PE matrix is skipped because "Bug A would tautologically hit PE too — same writer." True for Bug A (one writer, one fix). But the plan never explicitly confirms PE M-bias is also fixed end-to-end. With C1 above, PE worker sites 729/752 are the bug. After fixing them, a PE matrix smoke-run (even a fast 1-cell `--ignore 4 --parallel 1` PE pass) should be added to §4 test 8 before declaring "ready for v1.0 walk."

---

## Important findings

### I1. B2 open question (downstream consumers of `MethCall.read_pos`) — answered

Grep result: `MethCall.read_pos` is consumed in EXACTLY the four M-bias-accumulate sites listed in C1. It is **not** referenced by `write_call` (output.rs:170-), `write_yacht_row` (output_mode.rs:191-), `overlap.rs::drop_overlap`, or `output_mode.rs::compute_yacht_columns`. So Choice 2 (rebase at `call.rs:177`) would be functionally equivalent and would fix all four sites simultaneously with one line. Recommend the implementer reconsider: Choice 2 is now **smaller and structurally safer** than Choice 1 + helper. The plan's caveat against Choice 2 ("MethCall.read_pos may be used downstream") is empirically false in the current tree.

### I2. Test #5 fixture feasibility

Test #5 ("splitting_report_omits_overlap_line_for_autodetect_se") needs constructing a `ResolvedConfig { paired_mode: AutoDetect, no_overlap: true }`. `write_splitting_report` (`output.rs:574-`) takes `(path, config, report, is_paired, input_path)` — `is_paired` is passed by the caller after detection. The fixture works: just build the struct manually and call the function directly. No need for a real BAM. Plan §4 test #5 is feasible as written — confirmed.

### I3. Resolver-side fix alternative for Bug A is **cleaner** than the plan claims

§2 dismisses fixing at the resolver because "resolver runs before BAM is opened." But a third option exists: leave the resolver as-is and clear `config.no_overlap = false` in `state.rs::new` (or right before writer call in `state.rs:148`) when `is_paired == false` after detection. This colocates the SE-safety guarantee with the SE/PE branch instead of duplicating an `is_paired && config.no_overlap` gate at every writer call site. Not blocking — the plan's choice works — but worth a one-line consideration in §7.

### I4. Perl reference contract for SE no_overlap — verify

Plan claims Perl's SE branch never sets `$no_overlap`. The plan should cite Perl line numbers for both: (a) the PE branch that sets it (claim: ~1215-1224), and (b) the SE branch that does NOT. Without (b) it's argument-from-absence. Implementer should confirm before commit.

---

## Optional

- **O1.** §4 test names use snake_case but the crate convention (cf. mbias_writer tests) is also snake_case — consistent, fine. Just verify no duplicate test-fn names with existing modules.
- **O2.** §5 commit-ordering interleaves test commits and fix commits — works, but consider squashing per-bug (test+fix in one commit per bug) for cleaner bisect.
- **O3.** Plan's "Bug B fix is small" self-review checkbox is now false with C1; update before manual review sign-off.

---

## Validation sufficiency

Tests 1-7 cover the unit-level invariants well. Gap: **no test exercises `parallel.rs` worker paths.** Add at least one parallel-path test (`process_se` with `--parallel 2` synthetic input, `ignore_5p=3`, assert M-bias slot 1 populated, not slot 4) — this is the test that would have caught C1 in CI.

---

## Action items

**Critical:**
1. Extend Bug B scope to include `parallel.rs:625, 729, 752` (C1).
2. Add parallel-path test (C1 / Validation gap).

**Important:**
3. Reconsider Choice 1 vs Choice 2 in light of empirical B2 answer (I1) — Choice 2 is smaller.
4. Add a PE smoke-run to §4 test 8 (C2).
5. Cite Perl line for SE-branch absence of `$no_overlap` (I4).

**Optional:**
6. Mention state.rs-side gating alternative in §7 (I3).
7. Update §8 self-review checklist after C1 is addressed (O3).

---

**Report file:** `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PLAN_REVIEW_876_B.md`

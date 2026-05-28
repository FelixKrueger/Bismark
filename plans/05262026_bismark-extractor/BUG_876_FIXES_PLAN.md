# Plan — fix Bugs A + B from #876 (SE matrix byte-identity FAIL)

**Status**: rev 1 — dual-reviewer findings absorbed, awaiting re-review (Felix-approved revision, 2026-05-28)
**Scope**: closes #876 by fixing two precisely-located Rust-side regressions in `rust/bismark-extractor/`
**Workflow stage**: Plan (per `~/.claude/CLAUDE.md` mandatory plan → manual review → agent review → implement)
**Branch**: new branch `extractor-fix-876` off `rust/iron-chancellor` HEAD `968553b`

## Revision history

- **rev 0** (initial draft): proposed Bug B fix at `route.rs:95` (Choice 1) with `route_call` signature widening + 2 caller updates in `pipeline.rs`.
- **rev 1** (this version): dual plan-reviewers independently caught that Bug B has **4 sites, not 1** — the parallel-worker paths at `parallel.rs:625, 729, 752` also need the rebase. Both reviewers also verified empirically that `MethCall.read_pos` has **zero non-M-bias consumers** in the crate, which makes Choice 2 (rebase at `call.rs:177`) the smaller and structurally safer fix — one line covers all 4 sites atomically. This rev 1 adopts Choice 2 and expands the test suite to cover the parallel-worker paths that rev 0 missed. The dual-driver back-port trap memory ([[feedback-dual-driver-back-port]]) was extended in this same session to capture the broader-scope lesson (sibling dispatch sites within one crate, not just dual shell drivers).

## 1. Context

`scripts/phase_h_se_matrix.sh` ran end-to-end against the colossal 10M_SE BAM on 2026-05-28 (first such real-data run; CI never had the 1.2 GB BAMs). All 8 completed cells FAIL byte-identity. Root causes traced in #876 to two distinct local bugs. Methylation **data** files (CHG/CHH/CpG contexts) are byte-identical or sorted-equivalent across all cells — only the **report-format** files diverge:

- **Bug A**: `*_splitting_report.txt` is `+43 B` in **every** cell because Rust emits the line `No overlapping methylation calls specified` even in SE mode. The line is conceptually PE-only (`--include_overlap` / `--no_overlap` apply only when there are two mates).
- **Bug B**: `*.M-bias.txt` is `+150 B` when `--ignore N > 0` (affects 4 of 8 cells) because Rust records calls at their **absolute** 5'-oriented read position; Perl rebases by trimming the meth-call string with `substr($meth_call, $ignore, …)` at `bismark_methylation_extractor:1627` so position 1 = first un-clipped base.

## 2. Bug A fix — `output.rs:643`

### Current code (file: `rust/bismark-extractor/src/output.rs`, lines 640-645)

```rust
// Step 9: no_overlap line — matches Perl :5037 `if ($no_overlap)`.
// Plan rev 1 I9: simplified to just check config.no_overlap (Perl's
// SE branch never sets it, so the check is naturally SE-safe).
if config.no_overlap {
    w.write_all(b"No overlapping methylation calls specified\n")?;
}
```

### Why it's broken

The comment "Perl's SE branch never sets it, so the check is naturally SE-safe" is incorrect for an `AutoDetect → SE` path. The Rust resolver at `cli.rs:467-471` sets:

```rust
let no_overlap = if paired_mode != PairedMode::SingleEnd {
    !self.include_overlap          // = true (since --include_overlap is not passed)
} else {
    false
};
```

In the matrix run no `--single-end` / `--paired-end` flag is passed → `paired_mode = AutoDetect` → `paired_mode != SingleEnd` is **true** → `no_overlap = true`. The BAM is later detected as SE, but the `config.no_overlap` flag has already been resolved as `true` and the output-writer's check at L643 fires.

This is the **same class of regression** that the Phase C rev 1 fix at `cli.rs:461-466` was trying to prevent (it correctly catches the AutoDetect-then-PE case), but it overcorrected by also catching AutoDetect-then-SE.

**Perl reference contract** (per Reviewer B I4):
- Perl PE branch sets `$no_overlap`: `bismark_methylation_extractor:1219` (`$no_overlap = 0` in `--include_overlap` case) and `:1224` (`$no_overlap = 1` default for PE).
- Perl SE branch: `$no_overlap` is declared at `:931` (`my $no_overlap;`) but **never assigned anywhere outside the PE block** (`grep -n 'no_overlap' bismark_methylation_extractor` shows assignments only at L1219, L1224 — both inside the PE branch L1215-1224; all other references are read-only). So in SE, `$no_overlap` stays `undef` → falsy → line 5037 condition `if ($no_overlap)` is false → emission skipped.

### Proposed fix (1 line change + comment refresh)

```rust
// Step 9: no_overlap line — matches Perl :5037 `if ($no_overlap)`.
// Perl's SE branch leaves `$no_overlap` undef (declared at :931, only
// assigned in the PE branch at :1219/1224) → falsy → line skipped.
// Rust's resolver sets config.no_overlap = !include_overlap whenever
// paired_mode != SingleEnd (incl. AutoDetect), so we gate on the
// post-detection is_paired result here, NOT on config.no_overlap alone.
if is_paired && config.no_overlap {
    w.write_all(b"No overlapping methylation calls specified\n")?;
}
```

`is_paired` is already a parameter to `write_splitting_report` (signature at `output.rs:574-580`, passes through from `state.rs:153` which sets it post-BAM-detection). Net diff: one `if` condition + comment block. No other call sites need changes.

### Alternative considered (Reviewer B I3)

Clear `config.no_overlap = false` in `state.rs::new` (or just before `write_splitting_report` call at `state.rs:148`) when `is_paired == false` after detection. This **colocates** the SE-safety guarantee with the SE/PE branch in the state setup, instead of putting an `is_paired &&` gate at every consumer. Currently there's only one consumer (the splitting_report writer), so the value is small; if future writers also start consuming `config.no_overlap`, the state-side fix scales better.

**Decision**: stick with the writer-gate fix (`if is_paired && config.no_overlap`) for rev 1 since there's only one consumer. If a future PR adds a second consumer that needs the same SE-safety, refactor to state-side clearing at that point. Filed as latent option, not blocking.

### Per-cell impact (after Bug A fix in isolation)

Bug A alone changes the verdict for: D@N=1, D@N=4, 3p@N=1, 3p@N=4 → all go from `FAIL: 1 of 8 differ` to `PASS`. The other 4 cells (5p@*, 5p+3p@*) still FAIL on M-bias until Bug B is also fixed.

## 3. Bug B fix — `call.rs:177` (Choice 2, rev 1 redirect)

### The four bug sites (all share the `call.read_pos.saturating_add(1)` anti-pattern)

`grep -n "call.read_pos.saturating_add(1)" rust/bismark-extractor/src/` returns:

| File:line | Context | Identity | Reached by colossal matrix |
|---|---|---|---|
| `route.rs:95` | single-threaded `route_call` M-bias accumulate | R1/Single/R2 dispatch via `read_identity` | Yes — `--parallel 1` cells |
| `parallel.rs:625` | parallel SE worker M-bias accumulate (`process_se`) | Single | Yes — `--parallel 4` SE cells |
| `parallel.rs:729` | parallel PE worker R1 M-bias accumulate (`process_pe`) | R1 | Yes — `--parallel 4` PE cells (post-fix) |
| `parallel.rs:752` | parallel PE worker R2 M-bias accumulate (`process_pe`) | R2 | Yes — `--parallel 4` PE cells (post-fix) |

All four lines have **the same anti-pattern**: `let pos_1based = call.read_pos.saturating_add(1);`. The colossal SE matrix runs at both `--parallel 1` (hits route.rs:95) AND `--parallel 4` (hits parallel.rs:625) — so rev 0's plan to only touch route.rs:95 would have left the N=4 cells still failing. The PE matrix (which would have run after SE PASSed) routes through parallel.rs:729+752.

### Why Choice 2 is right (B2 question closed by dual-reviewer grep)

Reviewer A §1.4 and Reviewer B I1 both independently grepped the crate for consumers of `MethCall.read_pos`. Result: **exactly four sites, all M-bias accumulator inputs**. NOT consumed by:

- `write_call` at `output.rs:170-` (split-file writer keys on `ref_pos`, not `read_pos`)
- `write_yacht_row` at `output_mode.rs:191-` (yacht column derivation)
- `overlap.rs::drop_overlap` (keys on `ref_pos`)
- `output_mode.rs::compute_yacht_columns`

So **rebasing `MethCall.read_pos` at its construction site** (`call.rs:177`) is functionally equivalent to a per-site rebase in route.rs + parallel.rs × 3, and is much safer structurally (no risk of a 5th sibling site reintroducing the bug; no `route_call` signature change; no `pipeline.rs` caller updates).

### Proposed fix

**File: `rust/bismark-extractor/src/call.rs`, line 177**

```rust
// CURRENT (broken):
calls.push(MethCall {
    ref_pos: aligned.ref_pos,
    read_pos: aligned.read_pos_5p,
    context,
    methylated,
    xm_byte: aligned.xm_byte,
});

// REVISED (one-line change at L177):
calls.push(MethCall {
    ref_pos: aligned.ref_pos,
    // Rebase to "1-based-after-clip" semantic, matching Perl's
    // substr($meth_call, $ignore, ...) at :1627. Filter at :162 already
    // guarantees aligned.read_pos_5p >= ignore_5p, so saturating_sub is
    // safe but used for defense-in-depth. After this transform,
    // `read_pos == 0` means "first un-clipped base"; downstream M-bias
    // accumulators do `.saturating_add(1)` to land in slot 1.
    read_pos: aligned.read_pos_5p.saturating_sub(ignore_5p),
    context,
    methylated,
    xm_byte: aligned.xm_byte,
});
```

`ignore_5p` is already in scope at `call.rs:177` — it's the parameter to `extract_calls` (signature at L137).

**MethCall struct docstring update** (also in `call.rs`, around L32-39 per Reviewer A O1): change "absolute 5'-oriented read position (includes soft-clip)" to "0-based read position relative to the first un-clipped base after `--ignore` trimming. Matches Perl's `substr($meth_call, $ignore, ...)` rebasing."

### What about the 3 parallel.rs sites?

Each is structurally `let pos_1based = call.read_pos.saturating_add(1);`. With `MethCall.read_pos` already rebased at construction, the existing `+1` correctly produces M-bias position 1 for the first un-clipped call. **No changes needed at parallel.rs:625, 729, 752 — they automatically inherit the correct rebase.** Same for route.rs:95.

This is what makes Choice 2 structurally robust: future contributors who add a 5th M-bias accumulator site will get correct behavior for free, as long as they consume `MethCall.read_pos` rather than re-deriving an absolute read position.

### Per-cell impact (Bug A + Bug B combined)

All 8 completed cells go from FAIL to PASS:

| Cell | Pre-fix | After Bug A only | After Bug A + B |
|---|---|---|---|
| D@N=1 | FAIL 1/8 | PASS | PASS |
| D@N=4 | FAIL 1/8 | PASS | PASS |
| 5p@N=1 | FAIL 2/8 | FAIL 1/8 (M-bias) | PASS |
| 5p@N=4 | FAIL 2/8 | FAIL 1/8 (M-bias) | PASS |
| 3p@N=1 | FAIL 1/8 | PASS | PASS |
| 3p@N=4 | FAIL 1/8 | PASS | PASS |
| 5p+3p@N=1 | FAIL 2/8 | FAIL 1/8 (M-bias) | PASS |
| 5p+3p@N=4 | FAIL 2/8 | FAIL 1/8 (M-bias) | PASS |

The `edge_clip` cell remains incomplete (Perl-side hang per #876 Finding #3 — out of scope for this PR).

## 4. Test coverage (expanded from rev 0 per dual-reviewer findings)

### Bug A regression-guard tests (in `output.rs` test module)

1. **`splitting_report_omits_overlap_line_in_se_mode`**: `ResolvedConfig { paired_mode: SingleEnd, no_overlap: false }` → `write_splitting_report` with `is_paired = false` → assert output does NOT contain `"No overlapping methylation calls specified"`.
2. **`splitting_report_omits_overlap_line_for_autodetect_se`** (the regression-guard for the actual bug-triggering state — Reviewer B I2 confirmed feasibility): `ResolvedConfig { paired_mode: AutoDetect, no_overlap: true }` → `write_splitting_report` with `is_paired = false` (post-detection) → assert output does NOT contain the overlap line.
3. **`splitting_report_includes_overlap_line_for_pe_default`**: `ResolvedConfig { paired_mode: PairedEnd, no_overlap: true }` → `write_splitting_report` with `is_paired = true` → assert output DOES contain the overlap line.
4. **`splitting_report_omits_overlap_line_for_pe_with_include_overlap`**: `ResolvedConfig { paired_mode: PairedEnd, no_overlap: false }` (after `--include_overlap`) → assert output does NOT contain the overlap line.

### Bug B regression-guard tests (in `call.rs` and `mbias.rs` test modules; new `parallel.rs` test module for the worker path)

5. **`extract_calls_rebases_read_pos_after_ignore_5p`** (in `call.rs`): synthesize a `BismarkRecord` with calls at absolute positions 5, 6, 7 (after a 5-base 5' clip would survive), pass `ignore_5p = 5` → assert returned `MethCall.read_pos` values are 0, 1, 2 (rebased), NOT 5, 6, 7 (absolute). Also assert positions <5 are filtered out (existing behavior).
6. **`extract_calls_ignore_5p_zero_is_identity`** (in `call.rs`): with `ignore_5p = 0`, `read_pos_5p` values pass through unchanged. Regression guard for the default cell (D) — must not break the existing passing case.
7. **`mbias_slot_1_populated_after_rebase`** (in `mbias.rs` or `route.rs`): synthesize `MethCall` with rebased `read_pos = 0`, pass through `route_call` → assert `mbias[0].cpg[1]` (slot 1) has the count, AND `mbias[0].cpg[6]` (the would-be-absolute-slot under the bug) is zero. Both assertions are required — proves the rebase, not just the shift (per Reviewer A §4 alternatives note).
8. **`parallel_se_worker_m_bias_rebased`** (in `parallel.rs` test module, NEW): construct a small `process_se` invocation with `ignore_5p = 3` on a synthetic BAM record → assert the worker's M-bias output places counts at slots 1, 2, 3, … not at slots 4, 5, 6, …. **This is the test that would have caught the rev 0 plan's parallel.rs gap in CI** (Reviewer A C1 / Reviewer B C1+validation-gap).
9. **`parallel_pe_worker_m_bias_uses_r2_ignore_for_r2`** (in `parallel.rs` test module, NEW): construct `process_pe` with `ignore_r1 = 3, ignore_r2 = 7` → assert R1 calls land in `mbias[0]` slot 1+ and R2 calls land in `mbias[1]` slot 1+, NOT swapped. Reviewer A §1 final note: prevents future "passed wrong ignore field to wrong identity" bugs.
10. **`se_driver_vs_parallel_driver_m_bias_equality`** (in `parallel.rs` test module or top-level integration test, NEW per Reviewer A I2 / Reviewer B C1): dispatch identical synthetic input through `pipeline::extract_se` (single-threaded) and `parallel::process_se` (parallel worker) with `--ignore 5` → assert M-bias accumulators are byte-equal. **This is the structural regression guard for the dual-driver/dual-dispatch trap class** ([[feedback-dual-driver-back-port]] extended scope).

### Integration check on colossal (manual, not added to in-repo CI)

11. Re-run `scripts/phase_h_se_matrix.sh /weka/.../10M_SE/...bam --out ~/phase_h_se_release_fix876/` on a **fresh** `--out` dir (do NOT clobber `~/phase_h_se_release/` evidence per RELEASE_CHECKLIST escalation §1). Expect: exit 0 (PASS) or exit 3 (perf-miss-only). All 8 non-edge_clip cells should now PASS byte-identity.
12. **PE smoke run** (Reviewer B C2): a 1-cell PE smoke `--ignore 4 --parallel 1` to confirm the PE workers also got the Bug B fix end-to-end, BEFORE declaring "ready for v1.0 walk continuation". Use `scripts/phase_h_smoke.sh` directly (not the full PE matrix) for fast turnaround.

## 5. Implementation order (squashed test+fix commits per Reviewer B O2)

1. **Tests-first commits**: write tests 1-10 above. They MUST fail on current `968553b` HEAD (specifically: tests 5, 7 fail on Bug B; tests 2 fails on Bug A; tests 8-10 fail because parallel-worker tests don't exist yet). Commit as `test(extractor): regression guard for #876 (Bug A + Bug B + dual-dispatch)`.
2. **Bug A fix + test re-run**: change `output.rs:643` from `if config.no_overlap` to `if is_paired && config.no_overlap` + comment refresh citing Perl L931/1219/1224. Re-run tests 1-4 → expect PASS. Commit as `fix(extractor): omit overlap line in SE splitting_report (closes #876 Bug A)`.
3. **Bug B fix + test re-run**: change `call.rs:177` to `read_pos: aligned.read_pos_5p.saturating_sub(ignore_5p)` + update MethCall docstring at L32-39. Re-run tests 5-10 → expect PASS. Verify the colossal matrix re-run (test 11) also passes by spawning a fresh dispatch. Commit as `fix(extractor): rebase MethCall.read_pos to 0-based-after-clip (closes #876 Bug B)`.
4. **PR** against `rust/iron-chancellor`, title: `fix(extractor): #876 byte-identity regressions (splitting_report + M-bias)`. Body links #876, references this plan, calls out the rev-1 redirect from Choice 1 to Choice 2 (and credits both plan-reviewers for catching the parallel.rs gap), and includes the test 11 + test 12 results.
5. **After PR merge, on colossal**:
   - Re-run SE matrix on a fresh `--out` (test 11).
   - Run PE smoke (test 12).
   - If both PASS: comment on #798 with the verdicts. Reopen the v1.0 tag walk per RELEASE_CHECKLIST.md.
   - If either FAILs: archive evidence, file a follow-up bug sub-issue under #798, do NOT re-run on same `--out` dir.

## 6. Out of scope for this plan (Felix to triage separately)

- **Perl `--ignore ≥ read_length` hang** on `edge_clip` cell (#876 Finding #3). Perl v0.25.1 issue, not Rust. Suggested follow-ups: drop the `edge_clip` cell, OR revise to `--ignore 50`, AND add a per-cell wall-clock timeout in `phase_h_smoke.sh`. Track as separate sub-issue under #798 if pursued.
- **N=4 perf collapse** (#876 Finding #4). Rust at `--parallel 4` is slower than Perl at `--multicore 4` (0.8-1.1×) on this BAM. Likely separate root cause from #876. Track as `perf(extractor):` sub-issue under #798 after this plan's PR merges.
- **Full PE matrix run** (post-merge). The 1-cell PE smoke in test 12 is sufficient for THIS plan's v1.0 readiness gate. The full PE matrix walk happens as part of the resumed v1.0 release walk per RELEASE_CHECKLIST.md.
- **`MethCall.read_pos` docstring at call.rs:32-39 truthfulness audit** (Reviewer A O1). Touched as part of the Bug B fix commit (small ~3-line docstring update); not a separate change.
- **`cleanup_partial_outputs` audit for SE finalize path** (Reviewer A O3). Low-likelihood-bug audit; defer until a concrete symptom appears.

## 7. Assumptions / open decisions (rev 1 — all closed except verification items)

| # | Question | Resolution | Source |
|---|---|---|---|
| A1 | Bug A fix location: writer-gate vs resolver-fix vs state-side-clear? | **Writer-gate** at output.rs:643 (smallest diff, only one consumer of `config.no_overlap`). State-side clearing (Reviewer B I3) noted as latent option for future. | Both reviewers approve |
| B1 | Bug B fix: rebase at route_call (Choice 1) vs at call.rs (Choice 2)? | **Choice 2** — rebase at call.rs:177. Both reviewers verified `MethCall.read_pos` has zero non-M-bias consumers (B2 closed). | Both reviewers recommend; rev 1 absorbs |
| B2 | Are there downstream consumers of `MethCall.read_pos` beyond the 4 M-bias sites? | **NO** — verified via grep in both reviewer contexts. Not consumed by write_call, write_yacht_row, drop_overlap, compute_yacht_columns. | Both reviewers grepped independently and converged |
| B3 | Does PE R2's `--ignore_r2` need its own threading? | **Moot under Choice 2.** `extract_calls` already receives `ignore_5p` as a per-call-site parameter (callers pass `ignore_5p_r1` or `ignore_5p_r2` based on identity). The rebase at call.rs:177 inherits this routing for free. | Closed by Choice 2 adoption |
| T1 | Tests: `contains` checks vs literal-byte fragment matches? | **`contains` for the overlap line; for M-bias, assert BOTH (a) target slot has count AND (b) bug-mode slot is zero** to prove rebase not just shift. | Reviewer A §4 alternatives |
| T2 (NEW) | Should test #10 (SE-driver vs parallel-driver equality) be a unit test or integration test? | Implementer choice — unit test using a synthetic in-memory BAM is preferred (faster CI, no fixture file). Integration test acceptable if synthetic in-memory BAM construction is awkward. | Per Reviewer A I2 |
| V1 (NEW) | Plan claims `MethCall.read_pos` has 4 consumers — verify-before-commit? | Already done by reviewers — but implementer should run `grep -rn "read_pos" rust/bismark-extractor/src/` one more time at commit time as a final safety check. | Defensive |
| V2 (NEW) | Are `ignore_5p` values reachable via `extract_calls` parameter at all 4 dispatch sites? | Verify at implementation time — Choice 2 only works if `extract_calls` is called with the correct per-identity `ignore_5p` at each site (route.rs callers via pipeline.rs, parallel.rs workers). | Implementer task before commit |

## 8. Self-review (rev 1)

- [x] Bug A's root cause precisely identified (cli.rs:467-471 + output.rs:643), not speculation
- [x] Bug B's root cause precisely identified (call.rs:177 + 4 M-bias sites + Perl L1627 contract)
- [x] **All 4 Bug B sites accounted for** (rev 1 fix — `MethCall.read_pos` rebase at construction propagates to all consumers)
- [x] Fix is small (1 line Bug A + 1 line Bug B + docstring update) and localized to two files
- [x] Tests cover regression cases (bug-triggering states 2, 5, 7), default-cell preservation (6), parallel-worker paths (8, 9), and the dual-dispatch equality invariant (10)
- [x] No source edits performed yet — plan only
- [x] Pre-fold of likely review questions in §7 (assumptions/decisions table includes all rev-0 open items as closed + new V1/V2 verification items)
- [x] Out-of-scope items explicitly named (§6) so they're not silently dropped
- [x] Acknowledges the dual-driver back-port trap memory [[feedback-dual-driver-back-port]]: Choice 2 structurally prevents the trap (rebase at source point, all 4 sibling sites benefit). Memory extended in this session to capture the "sibling dispatch within one crate" scope expansion.
- [x] Perl SE-branch absence cited with line numbers (L931 declaration; assignments only at L1219/L1224 in PE branch) — closes Reviewer B I4 (no longer argument-from-absence)
- [x] PE smoke test added to §4 test 12 — closes Reviewer B C2
- [x] pipeline.rs line ref corrections from Reviewer A §1.6 / Reviewer B C1: not needed in rev 1 plan since Choice 2 doesn't touch pipeline.rs at all
- [ ] Plan-reviewer re-run on this rev 1 — pending (Felix triggers after manual look)

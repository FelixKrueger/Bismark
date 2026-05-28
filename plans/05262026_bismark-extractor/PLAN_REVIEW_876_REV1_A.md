# Plan Review A (Round 2 / rev 1) — BUG_876_FIXES_PLAN.md

**Reviewer**: A, round 2 (fresh context, independent of round-1 A/B)
**Plan**: `plans/05262026_bismark-extractor/BUG_876_FIXES_PLAN.md` (rev 1)
**Verdict**: **Approve with minor clarifications.** Rev 1 correctly absorbs the round-1 Critical (4-site scope) and the four Important items. Two small notes on docstring truthfulness + one note on the saturating_sub justification. No new issues introduced.

---

## 1. Round-1 absorption audit

| Round-1 item | Rev 1 location | Absorbed? |
|---|---|---|
| Rev-A C1 (parallel.rs back-port) | §3 table; §7 B1; §8 self-review | YES — adopted Choice 2, which fixes all 4 sites in one line |
| Rev-A I1 (Choice 2 reconsideration) | §3, §7 B1, B2 | YES — Choice 2 adopted with grep-evidence cited |
| Rev-A I2 (cross-driver M-bias equality test) | §4 test 10 | YES |
| Rev-A I3 (pipeline.rs line refs) | §8 self-review checkbox | YES — moot since Choice 2 doesn't touch pipeline.rs |
| Rev-A O1 (MethCall docstring update) | §3 "MethCall struct docstring update" | YES (with caveat — see §3 below) |
| Rev-B C1 (parallel.rs scope) | §3 table | YES |
| Rev-B C2 (PE smoke) | §4 test 12 | YES (with caveat — see §5 below) |
| Rev-B I1 (B2 closed) | §7 B2 | YES |
| Rev-B I3 (state.rs alternative) | §2 "Alternative considered" | YES — filed as latent option |
| Rev-B I4 (Perl SE-branch citation) | §2 "Perl reference contract" | YES — verified independently: L931 declaration only; L1219/L1224 are inside PE `if/else` block at L1215-1224 (confirmed via direct read) |

All Critical and Important items absorbed.

## 2. Independent verification of rev 1's mechanical claims

- **`ignore_5p` in scope at call.rs:177**: confirmed. Parameter declared at `call.rs:137` and used at `:145`/`:162` already. The proposed `aligned.read_pos_5p.saturating_sub(ignore_5p)` change compiles trivially with no signature update.
- **Four bug sites**: `grep -n "call.read_pos.saturating_add(1)" rust/bismark-extractor/src/` returns exactly `route.rs:95`, `parallel.rs:625, 729, 752`. Confirmed.
- **Callers of `extract_calls` pass correct per-identity `ignore_5p` (V2)**: confirmed at all 4 entry points:
  - `pipeline.rs:140-145` (SE) → `config.ignore_5p_r1`
  - `pipeline.rs:340-345` (PE R1) → `config.ignore_5p_r1`
  - `pipeline.rs:346-351` (PE R2) → `config.ignore_5p_r2`
  - `parallel.rs:607-612` (SE worker) → `config.ignore_5p_r1`
  - `parallel.rs:698-703` (PE R1 worker) → `config.ignore_5p_r1`
  - `parallel.rs:704-709` (PE R2 worker) → `config.ignore_5p_r2`
  So Choice 2 inherits correct per-identity routing for free at every dispatch site. V2 in §7 is satisfied by current code — no implementer guesswork needed.
- **Perl SE-branch absence of `$no_overlap` assignment**: `grep -n no_overlap bismark_methylation_extractor` shows L931 (declaration only) and L1219/L1224 (assignments). The `if ($include_overlap){...} else { if ($paired_end){...} }` block at L1216-1226 confirms both assignments require PE. No SE assignment exists. Rev 1's citation is accurate.
- **`parallel.rs` already has a test module** (`#[cfg(test)] mod tests` at L968+) — so tests 8/9/10 can be added to the existing module rather than building new fixture infrastructure. Rev 1 doesn't say this explicitly; worth noting.

## 3. New issues introduced by rev 1

### Important
- **I1. `saturating_sub` necessity.** The filter at `call.rs:162` already guarantees `aligned.read_pos_5p >= ignore_5p` (it `continue`s for `read_pos_5p < lo` where `lo = ignore_5p`). A plain `aligned.read_pos_5p - ignore_5p` would never underflow. Rev 1's inline comment acknowledges this ("filter at :162 already guarantees..., so saturating_sub is safe but used for defense-in-depth"). This is correct, but worth a one-line code comment in the actual fix to prevent a future reviewer asking "why saturating?". The plan body covers it adequately; the implementer just needs to keep the rationale in the source comment.

- **I2. Docstring at call.rs L32-39 — partial truthfulness gap.** The current docstring explicitly says "**Includes soft-clipped positions in the count**... For a `+`-strand `5S95M` record the first emitted call has `read_pos == 5`". Rev 1 §3 proposes replacing this with "0-based read position relative to the first un-clipped base after `--ignore` trimming". After the fix, for a `5S95M` record with `ignore_5p=0`, the first emitted call would still have `read_pos == 5` (since soft-clip positions inflate `read_pos_5p` and we only subtract `ignore_5p`, not the soft-clip count). So the new docstring is **partially misleading** — it conflates "first un-clipped base" (soft-clip) with "after --ignore" (ignore region). Recommend more precise wording: "0-based read position in 5'-oriented coordinates (still includes soft-clip), rebased so that `read_pos == 0` corresponds to the first base after `ignore_5p` trimming. M-bias accumulators add 1 to land in slot 1." Implementer should fold this clarification before committing the docstring change.

### Optional
- **O1. Test 12 scope.** Rev 1 specifies "1-cell PE smoke" (a `--ignore 4 --parallel 1` PE pass). For full parity with the SE-bug-trigger conditions, a 2-cell PE smoke covering both `--parallel 1` (route.rs path) and `--parallel 4` (parallel.rs PE path at L729/752) would catch any asymmetry between drivers introduced by the rebase. The 1-cell version covers route.rs only and leaves parallel-PE end-to-end unproven on real data — though test 9 (`parallel_pe_worker_m_bias_uses_r2_ignore_for_r2`) does cover it at the unit level. Not blocking; flag for Felix's call.

- **O2. Test 8 wording.** The test name `parallel_se_worker_m_bias_rebased` is fine, but the body description says "synthetic BAM record" — `process_se` in parallel.rs expects a `BismarkRecord`, so the implementer can construct one in-memory using `bismark-io` test helpers (if they exist) or via a tiny inline BAM-bytes literal. Rev 1 should explicitly note which path it expects (T2 in §7 leaves it to implementer, which is fine, but flagging now that the `bismark-io` test fixture story may need a one-liner check before implementation start to avoid surprise effort).

## 4. Items NOT introduced as issues

- Rev 1's `extract_calls` signature is unchanged — confirmed not a breaking API change.
- Bug A fix at `output.rs:643` is unchanged from rev 0 — the round-1 reviewers approved this and I see no reason to revisit.
- Test 7's "BOTH slot 1 populated AND slot 6 empty" assertion correctly captures rev-A §4's recommendation.
- Tests 8/9/10 are well-targeted at the dual-driver back-port trap class.

## 5. Round-1 items not yet visible in rev 1 (minor)

- Rev-A O3 (`cleanup_partial_outputs` audit for SE finalize): explicitly deferred in §6 of rev 1. Acceptable — low-likelihood, no concrete symptom.
- Rev-B O1 (test-name collisions): not mentioned in rev 1. Trivial — implementer will catch via `cargo test` collisions. No action needed at plan stage.

## 6. Action items

### Critical
None. Rev 1 fully absorbs round-1 Criticals.

### Important
- **I1.** Tighten the `MethCall.read_pos` docstring per §3 above — current proposed wording conflates soft-clip semantics with ignore-trimming semantics. Use: "0-based read position in 5'-oriented coordinates (still includes soft-clip); rebased so `read_pos == 0` corresponds to the first base after `ignore_5p` trimming."
- **I2.** Add a one-line source comment at the `saturating_sub` line explaining "filter at L162 already guarantees `read_pos_5p >= ignore_5p`; saturating is defense-in-depth only" — prevents drive-by questions from future readers.

### Optional
- **O1.** Consider expanding test 12 (PE smoke) from 1 cell to 2 cells (`--parallel 1` + `--parallel 4`) to symmetrically cover both PE drivers end-to-end on real data. Unit test 9 already covers it at the synthetic level.
- **O2.** Verify the `bismark-io` test-helper situation supports in-memory `BismarkRecord` construction for tests 8/9/10 before implementation start (so the implementer doesn't discover mid-flight that fixture infra needs building).

## 7. Summary

Rev 1 correctly redirects to Choice 2 with grep-backed evidence, expands test coverage to all four dispatch sites + the dual-driver equality invariant, cites Perl SE-branch absence with line numbers, and adds a PE smoke gate before v1.0 walk resumption. All round-1 Criticals and Importants are absorbed. Two minor docstring/comment refinements (I1, I2) are the only remaining items before sign-off. **Ready for implementation upon Felix's explicit trigger.**

**File path**: `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PLAN_REVIEW_876_REV1_A.md`

# Plan Coverage Report — Phase H SE

**Mode:** B (code vs implementation plan, post-implementation audit)
**Plan:** `PHASE_H_SE_PLAN.md` rev 2
**Date:** 2026-05-28
**Verdict:** **COMPLETE — 18 DONE / 5 DEVIATED (documented) / 1 PARTIAL (1 unresolved item)**

> **Note:** This coverage audit was performed by the main thread inline, NOT by a fresh-context `plan-manager` Agent. The plan-manager Agent stalled at 600s (stream-watchdog timeout) and could not recover. The dual code-reviewer reports (`CODE_REVIEW_PHASE_H_SE_A.md` + `_B.md`) already provided independent verification perspectives; this report focuses on the coverage-mapping work the plan-manager would have done, which is deterministic and reproducible from the plan + code state. If a fresh-context audit is required, file a sub-issue to re-run plan-manager.

## Summary

- Total items: 24
- DONE: 18
- PARTIAL: 1 (§3.4 #4 M-bias row-count differential — documented in plan but not implemented in driver)
- DEVIATED: 5 (all documented in plan rev 2 Implementation Notes §1-5)
- MISSING: 0

## Coverage ledger

### Per-§ implementation tasks (plan §5)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | SPEC §8.3 Phase H matrix subsection added | §5.1 step 1 | DONE | Verified: `rust/bismark-extractor/SPEC.md` ~line 736 has the "Phase H matrix (rev 4)" subsection with 5-cell SE table, cross-N assertion, Perl version pre-flight, exit-code mapping. |
| 2 | SPEC §9.7 "Measured via Phase H" paragraph | §5.1 step 2 | DONE | Verified: cross-references `phase_h_se_matrix.sh` + `phase_h_pe_matrix.sh` (future); exit-code 3 framing for sub-target-miss. |
| 3 | SPEC §10 row H split (sub-gate 1 SE / 1 PE / 2-blocked-on-#797) | §5.1 step 3 | DONE | Verified: three rows replace the single "H" row in §10 table. |
| 4 | `git mv` oxy_phase_h_smoke.sh → phase_h_smoke.sh | §5.2 step 1 | DONE | Verified: `git log --diff-filter=R --summary` shows the rename; file history preserved. |
| 5 | phase_h_smoke.sh docstring rewrite | §5.2 step 2 | DONE | Verified: drops "oxy_" / "Phase F + flavour A"; new top-of-file describes "Phase H per-cell byte-identity smoke (post-colossal-migration)". |
| 6 | Grep for external references to old script name | §5.2 step 3 | DONE | Verified: only historical plan + review markdown still references `oxy_phase_h_smoke` (appropriately left unchanged). |
| 7 | `--extra-rust` / `--extra-perl` CLI flags with bash-array parsing | §5.3 step 1 | DONE | Verified: `read -r -a EXTRA_RUST <<< "$EXTRA_RUST_STR"` parsing; `"${EXTRA_RUST[@]}"` invocation. |
| 8 | SE branch kept-file expectation handling | §5.3 step 2 | DEVIATED (#4) | Implementation chose ANNOTATE not ENFORCE — emits `Library: SE` / `Library: PE` to `diff_summary.txt`. Documented in plan rev 2 Implementation Notes deviation §4. Acceptable since the existing Perl-vs-Rust file-set diff catches asymmetries symmetrically. |
| 9 | Wall-clock format pinned at `^Perl: <int>s$` / `^Rust: <int>s$` | §5.3 step 3 | DONE | Verified: smoke emits format via `echo "Perl: ${PERL_ELAPSED}s"`; inline comment locks the format. |
| 10 | New `scripts/phase_h_se_matrix.sh` driver | §5.4 | DEVIATED (#1) | DONE in 551 LOC (vs ~250 LOC estimate). All functional sub-components present: pre-flight, matrix exec, cross-N, speedup table, M-bias guard, exit-code mapping, verdict file. Size deviation justified in Implementation Notes §1; reviewers concur. |
| 11 | New top-level `RELEASE_CHECKLIST.md` | §5.5 | DONE | Verified: 162 LOC; sections include roles, three escalation paths, SE block with tmux-wrapped colossal invocation, PE TODO-stub referencing #872, v1.0 tag steps, sub-gate 2 placeholder. |
| 12 | PROGRESS.md row update | §5.6 | DONE | Verified: Phase H SE row reflects rev 2 implementation-complete status. |
| 13 | No crate code changes (preserves 303-test baseline) | §5.7 | DONE | Verified: `git diff --stat rust/iron-chancellor...HEAD -- 'rust/bismark-extractor/src/**' 'rust/bismark-extractor/tests/**'` returns empty. 303 tests pass. |
| 14 | `bash -n` syntax check on both scripts | §5.8 step 1+2 | DONE | Both clean (verified during implementation). |
| 15 | `shellcheck` on both scripts | §5.8 step 3 | DEVIATED (#2) | shellcheck not installed on dev Mac; skipped with note per plan's optional fallback. |
| 16 | Local-Mac smoke dry-run on tiny SE BAM | §5.8 step 4 | DEVIATED (#3) | Skipped — dev Mac lacks Perl bismark v0.25.1 in a `bioinf`-equivalent env; pre-flight would correctly reject. The pre-flight Perl-version assertion (rev 1 I8) is the design that makes this skip safe — actual end-to-end happens on colossal per RELEASE_CHECKLIST.md. Also: discovered post-implementation that bash 3.2 (default macOS) would have failed BEFORE reaching the matrix logic per Reviewer A's bash-compat findings. |
| 17 | `cargo test -p bismark-extractor` confirms 303-test baseline preserved | §5.8 step 5 | DONE | Verified: TOTAL PASSED = 303 (exact match to Phase G post-merge baseline). |
| 18 | Implementation Notes section in plan + Iteration log | (skill requirement) | DONE | Verified: rev 2 added Implementation Notes at top of plan; per-§ status table, deviations §1-5, 6-step iteration log. |

### Per-§4 signatures

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 19 | `scripts/phase_h_se_matrix.sh` CLI shape + 4-way exit codes | §4.1 | DONE | Verified: `<BAM> [--out DIR] [--parallel-set "1 4"]`; exit codes 0/1/2/3 mapped per §3.3.6. |
| 20 | `scripts/phase_h_smoke.sh` CLI additions | §4.2 | DONE | Verified: `--extra-rust` + `--extra-perl` flags present; library-mode annotation in diff_summary.txt. |
| 21 | `RELEASE_CHECKLIST.md` section structure | §4.3 | DONE | Verified: roles, escalation, SE matrix, PE TODO-stub, v1.0 tag steps, sub-gate 2 placeholder. |

### Per-§3 behavior contracts

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 22 | 5-cell matrix definition (D / 5p / 3p / 5p+3p / edge_clip × `--parallel {1,4}`) | §3.1 | DONE | Verified: `MATRIX_CELLS=( "D|0|0" "5p|5|0" "3p|0|5" "5p+3p|5|5" "edge_clip|250|0" )` in driver; matches plan + SPEC §8.3 table byte-for-byte. |
| 23 | Per-cell byte-identity contract (5 assertions) | §3.4 | **PARTIAL** | Assertions #1, #2, #3, #5 are DONE (sorted-content data files, file-set match, splitting-report cmp, cross-N invariance). Assertion #4 is **SPLIT**: (D,N=1) M-bias 5712 B hard-fail is DONE; **the M-bias row-count differential check for ignore-flag cells (5p, 3p, 5p+3p, edge_clip) is NOT implemented** in the driver. Documented in plan §3.4 #4 but no corresponding code. Reviewer B also flagged this. **THIS IS THE 1 UNRESOLVED ITEM in the verdict.** |
| 24 | Cross-N byte-identity per ignore-pair (rev 1 C1) | §3.3.4 | DONE | Verified: driver's "Cross-N byte-identity check" block iterates ignore-pair → all (N_a < N_b) pairs → `cmp -s` on every shared output file → aggregates to `cross_n_summary.txt`. |

### Plan-recognised deviations (documented in plan rev 2 Implementation Notes)

| Deviation | Description | Verified in plan? |
|---|---|---|
| §1 | Matrix driver 551 LOC vs ~250 LOC estimate | ✅ (Implementation Notes deviation §1; reviewers concur) |
| §2 | shellcheck not installed; skipped with note | ✅ |
| §3 | No local-Mac smoke dry-run | ✅ |
| §4 | Library-mode annotation chose ANNOTATE not ENFORCE | ✅ |
| §5 | Default OUT_DIR renamed from ./oxy_phase_h_out to ./phase_h_out | ✅ |

All deviations are documented in plan rev 2 with rationale. None invalidate the plan's goal or scope.

## Gaps (detail)

### Item 23: §3.4 #4 — M-bias row-count differential check for ignore-flag cells

**Expected** (plan §3.4 #4): "ignore-flag cells use row-count differential check (catches silent `--ignore`-no-op regressions per A-I8 / B-I11). For (5p, 0) and (0, 3p) and (5p, 3p), the M-bias row count must be less than the (D, N=1) cell's row count. For edge_clip expect empty/zero rows."

**Found:** The driver implements the size-equality check at (D, N=1) but does not iterate other cells' M-bias files to assert the row-count-decreases invariant. The `phase_h_smoke.sh` per-cell `cmp -s` on `*.M-bias.txt` provides per-cell Rust-vs-Perl byte equality, which catches divergences — but does NOT catch the case where BOTH Rust and Perl produce identical-but-wrong M-bias output (e.g. both ignore the `--ignore` flag silently).

**Gap:** Add a section in the matrix driver that, after the main loop, walks each ignore-flag cell's `cell_p1_i*/rust/*M-bias.txt`, counts data rows (excluding header), and asserts:
- `(5p, 0)` row count < `(D, N=1)` row count
- `(0, 3p)` row count < `(D, N=1)` row count
- `(5p, 3p)` row count <= both individual single-flag cells
- `edge_clip` row count == 0 (or near-0; depends on read length)

Fail the matrix with exit 1 if any assertion violated. Aggregate to `matrix_verdict.txt`.

**Severity:** Important but not Critical — the existing `cmp -s` per-cell catches Rust-vs-Perl divergences (the primary failure mode); this check catches the rarer both-binaries-silently-wrong failure mode. Code-reviewer B flagged as Medium ("M-bias row-count differential check from plan §3.4 #4 isn't implemented").

**Recommended action:** Fold into rev 3 absorption alongside the code-reviewer findings.

## Test verification

| Test | Command | Status |
|---|---|---|
| bash -n smoke | `bash -n scripts/phase_h_smoke.sh` | PASS (verified during implementation) |
| bash -n matrix | `bash -n scripts/phase_h_se_matrix.sh` | PASS |
| shellcheck | `shellcheck scripts/phase_h_*.sh` | SKIPPED (not installed; deviation §2) |
| cargo test baseline | `cargo test -p bismark-extractor` | PASS — 303/0/0, exact match to Phase G baseline |

## Verdict

**COMPLETE** with 1 PARTIAL (§3.4 #4 M-bias row-count differential) + 5 documented DEVIATED items.

The plan's primary goal — SE byte-identity matrix harness + speedup measurement + release checklist for v1.0 gate — is delivered. The PARTIAL item is documented in the plan but missing from the implementation; same issue surfaced by Reviewer B (and indirectly by Reviewer A's M-bias coverage critique). Recommended absorption in rev 3 alongside the two code-reviewers' findings.

No MISSING items. No CRITICAL coverage gaps. The 5 DEVIATED items are all justified in rev 2 Implementation Notes; reviewers did not push back on any of them.

Ready for the rev 3 absorption pass that the two code-reviewer reports flag (1 Critical + 3 distinct High + this 1 PARTIAL = ~5 items total).

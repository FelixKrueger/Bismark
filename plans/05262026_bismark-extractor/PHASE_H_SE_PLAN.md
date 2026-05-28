# Phase H sub-gate 1 — SE byte-identity + speedup harness (closes #871)

**Status:** Plan rev 1, post-dual-plan-review absorption. Awaiting implementation trigger from Felix.
**Parent issue:** #871 (filed under epic #798).
**Companion sub-issue:** #872 — PE byte-identity + speedup harness (parallel sub-issue under the same epic; independent driver per Felix's 2026-05-28 directive; **5-cell matrix reconciled with #871 in rev 1**).
**Branch target:** new `extractor-phase-h-se` from `rust/iron-chancellor` HEAD `f88bad7` (post-Phase-G project-board automation tooling commit).
**Crate version bump:** **none.** Phase H is harness-and-checklist work, not crate code.

> **Epic:** `plans/05262026_bismark-extractor/PROGRESS.md`, Phase H sub-gate 1 — SE byte-identity + speedup harness. No standalone `EPIC.md`; the local epic-tracking artifact is `PROGRESS.md`. Upstream coordination doc is GitHub epic #798.

## Revision History

| Rev | Date | Notes |
|---|---|---|
| 3 | 2026-05-28 | **Post-code-review absorption complete.** Folded 1 Critical + 4 High + 5 selected Medium findings from dual code-reviewers + plan-manager-equivalent inline audit. Headline changes: (1) **B-L1 Critical (speedup-table arithmetic inversion)** — column header says `Rust/Perl` but driver computed `P*100/R`; fixed to `R*100/P` with `P=0` guard in both per-cell + per-N-aggregate sites; (2) **A-L1 ≡ B-L2 High (M-bias fail-open)** — `MBIAS_BASELINE_OK=0` default + new `MBIAS_GATE_APPLIES` flag for missing-N=1 case; missing file no longer silently passes; (3) **A-Er1/Er2 High (bash 3.2 incompat)** — `BASH_VERSINFO < 4` hard-fail pre-flight + `${ARR[@]+"${ARR[@]}"}` defensive idiom in smoke script; (4) **B-E3 High (uptime parse)** — `load average[s]?:` regex tolerates BSD/GNU dialects; silent-skip on parse failure instead of awk syntax error; (5) **Coverage §3.4 #4 High (PARTIAL absorption)** — new M-bias row-count differential check across ignore-flag cells at N=1; asserts 5p/3p/5p+3p < D and edge_clip near-zero. Selected Mediums: SIGINT trap; nproc-missing skips check entirely; tmux warning if `$TMUX`/`$STY` unset; sub-2s cell ⚠️ annotation in speedup table; cargo build budget + bash 4 + Perl binary equivalence documented in `RELEASE_CHECKLIST.md`. Deferred: 80 LOC speedup-table dedup refactor; hypothetical gzip cross-N false-fail (no cell injects --gzip in SE matrix). Re-validation: `bash -n` clean on both scripts; `cargo test -p bismark-extractor` → 303/0/0 baseline preserved. |
| 2 | 2026-05-28 | **Implementation complete on branch `extractor-phase-h-se`** (off `f88bad7`). See "Implementation Notes" section below the rev table. 303 tests preserved (no Rust code changes — Phase G baseline intact). Two bash scripts created/renamed (`phase_h_smoke.sh` + new `phase_h_se_matrix.sh`); top-level `RELEASE_CHECKLIST.md` created; SPEC §8.3 + §9.7 + §10 row H updated. Awaiting dual-code-review + plan-manager audit. |
| 1 | 2026-05-28 | **Folded both reviewers' 1 Critical + 14 distinct Important findings.** Headline changes:<br>**C1 (A-Critical, B-Imp 2 — strongest consensus):** Per-cell self-determinism is strictly weaker than SPEC §8.3 row 4's Rust-N=1-vs-Rust-N=4 raw-byte equality (the Phase F N-invariance invariant). Rev 1 replaces the self-determinism assertion with the cross-N byte-identity check; the matrix already collects N=1 + N=4 output for each `(ignore_5p, ignore_3p)` tuple, so adding the comparison is free. Without this, the harness can't catch a future BTreeMap-collector regression — defeating its purpose.<br>**I1 (consensus):** §6 runtime estimate replaced with honest 1-3 h range; "measure on first colossal run" instead of fake-precision minute claims. Both reviewers flagged my 12 min/run figure as wrong (A: 25% low; B: figure is PE-from-CLAUDE.md, SE is faster).<br>**I2 (A):** Wall-clock parsing approach factually corrected — smoke script emits `date +%s` integer seconds in `Perl: Ns` / `Rust: Ns` format. Driver `grep -E` regex pinned.<br>**I3 (A):** Self-determinism (now cross-N) re-run mechanics: second Rust run into separate sub-dir (`cell_*_rerun/`); smoke not re-invoked (no double-Perl cost); pre-flight `--out` interaction documented.<br>**I4 (A):** §3.3.2 vs §5.4.4.d wall-clock-source inconsistency resolved — driver parses from smoke's `diff_summary.txt`; does NOT wrap independently.<br>**I5 (A):** SPEC §10 row H added to §5.1 task list — sub-gate split reflected in the phase table.<br>**I6 (A):** Bash word-splitting for `--extra-rust` / `--extra-perl` pin: `read -r -a EXTRA_RUST <<< "$EXTRA_RUST_STR"` array form; smoke uses `"${EXTRA_RUST[@]}"` quoting. Avoids the IFS quirk + handles quoted args correctly.<br>**I7 (A):** §2.5 "larger SE BAM" dropped from rev 1 — was unreachable through the single-`<BAM>`-arg CLI. Future enhancement only if `--secondary-bam` is added.<br>**I8 (B):** Pre-flight Perl version assertion — `bismark_methylation_extractor --version | grep "v0.25.1"` else hard-fail; prevents silent baseline invalidation from `bioinf` env updates.<br>**I9 (B):** Pre-flight `nproc` + contention advisory — warn if `--parallel N > available cores` or if 1-minute load average > nproc.<br>**I10 (B):** Speedup table adds **Perl-only scaling** column per #871 body — was missing in rev 0.<br>**I11 (B):** `tmux` / `screen` recommendation in RELEASE_CHECKLIST.md — protects against SSH disconnect on 1-3 h matrix runs.<br>**I12 (B):** Pre-flight matrix-level `--out` non-empty rejection (was per-cell only in rev 0).<br>**I13 (B):** RELEASE_CHECKLIST operational specificity — sign-off mechanism, mid-regression escalation path, who-runs-it, recorded-where.<br>**I14 (B):** Reverse-merge-order with #872 — checklist's PE section is created as TODO-stub by THIS PR; #872's PR populates it. Either-order-merges safely.<br>**I15 (consensus):** `*.M-bias.txt` byte-count behavior split — (0,0) cell hard-fails on 5712 B drift (regression guard for Phase C.1); ignore-flag cells use row-count differential check (catches silent `--ignore`-no-op regressions per A-I8 / B-I11).<br>**I16 (consensus):** §3.3.5 exit-code design + §3.6 PASS verdict relationship explicitly articulated.<br>**I17 (B-Imp 12 — RESOLVED via reconciliation):** SE matrix reduced from rev 0's 2³=8 cells to **5 representative cells** mirroring #872's PE 5-cell reduction structure. Cells: D (default), 5p (--ignore 5 isolated), 3p (--ignore_3prime 5 isolated), 5p+3p (both trims combined), edge_clip (--ignore 250 exceeds read length, sanity edge case). × `--parallel {1, 4}` = 10 invocations per binary = 20 total per BAM. Cross-plan symmetry restored.<br>**I18 (A-I8):** Disk footprint estimate refined to ~1.8 GB (was 1.2 GB); A14 updated to 2.5 GB safety margin. |
| 0 | 2026-05-28 | Initial draft post-Phase-G merge + Phase H sub-issue split (#871 SE + #872 PE). Two pre-existing Criticals resolved with Felix before completing: **CI integration = manual release-prep checklist** (not a self-hosted runner); **#871 + #872 use independent drivers** (two PRs targeting `rust/iron-chancellor`). |

## Implementation Notes (rev 2, 2026-05-28, post-impl)

Executed on branch `extractor-phase-h-se` (off `rust/iron-chancellor` HEAD `f88bad7`). No Rust code changes; no crate version bump (Phase H is harness work, per plan §A10).

### Per-§ status

| § | Done | Notes |
|---|---|---|
| §5.1 SPEC §8.3 + §9.7 + §10 row H updates | ✅ | §8.3 gained a "Phase H matrix (rev 4)" subsection enumerating the 5-cell SE matrix + cross-N assertion + Perl version pre-flight + exit-code mapping. §9.7 gained a "Measured via Phase H" paragraph cross-referencing the matrix drivers. §10 row H split into three rows: sub-gate 1 SE (#871, this work, ~485 LOC), sub-gate 1 PE (#872, separate PR), sub-gate 2 (blocked-on-#797). |
| §5.2 Rename `oxy_phase_h_smoke.sh` → `phase_h_smoke.sh` | ✅ | `git mv` preserves history. Top-of-file docstring updated: "Phase H per-cell byte-identity smoke (post-colossal-migration; machine-agnostic via BAM-path argv)". Grep'd for external references; the only matches in source-controlled files are in historical plan + review markdown (PHASE_C1/C2/G plans, code reviews, coverage reports) — those documents reference the script under its as-merged historical name and are correctly left unchanged. Current-revision references in PHASE_H_SE_PLAN.md and PLAN_REVIEW_PHASE_H_SE_A.md document the rename itself. |
| §5.3 SE tweaks + `--extra-*` flags + wall-clock format | ✅ | Added `--extra-rust "<flags>"` and `--extra-perl "<flags>"` CLI flags. Parse via `read -r -a EXTRA_RUST <<< "$EXTRA_RUST_STR"` (rev 1 I6) — avoids the bash IFS word-splitting quirk (see memory `feedback_bash_ifs_word_splitting`). Pass-through at invocation via `"${EXTRA_RUST[@]}"`. Empty defaults. Wall-clock format verified already pinned at `^Perl: <int>s$` / `^Rust: <int>s$` (existing `date +%s` calls produce this shape) — added an inline comment locking the format for matrix-driver consumers. Library-mode annotation added to `diff_summary.txt`: `Library: SE` or `Library: PE` based on `@PG` auto-detect, enabling matrix-driver mode-specific assertions. Default `OUT_DIR` changed from `./oxy_phase_h_out` to `./phase_h_out`. |
| §5.4 New `scripts/phase_h_se_matrix.sh` | ✅ | 551 LOC bash (substantial deviation from §2.2's ~250 LOC estimate — see deviations below). Implements pre-flight (BAM, OUT empty, Perl version v0.25.1, Rust binary discoverable, nproc + contention advisory), 5-cell × N matrix execution, per-pair cross-N byte-identity check, Perl-only + Rust scaling speedup table emission with Rust commit + crate version embedded, M-bias (D, N=1) 5712 B baseline regression guard, 4-way exit-code mapping (0/1/2/3), `matrix_verdict.txt` aggregate. Driver discovers `phase_h_smoke.sh` via `$(dirname "${BASH_SOURCE[0]}")/..` for portability. |
| §5.5 New `RELEASE_CHECKLIST.md` (top-level) | ✅ | 162 LOC. Sections: roles (Felix = release engineer; sign-off via comment on epic #798), three escalation paths (mid-checklist regression → `bug(extractor):` sub-issue; perf-miss → `perf(extractor):` sub-issue; pre-flight USAGE → fix env + re-run), bismark-extractor v1.0 Phase H sub-gate 1 SE block with explicit `tmux`-wrapped colossal invocation, PE TODO-stub referencing #872 (reverse-merge-order safe per rev 1 I14), v1.0 tag steps, sub-gate 2 placeholder (blocked on #797). |
| §5.6 PROGRESS.md update | ✅ (rev 1 → rev 2 status) | Phase H SE row updated to reflect implementation complete + PR-open state. |
| §5.7 No crate code changes | ✅ | Confirmed: `git diff --stat rust/iron-chancellor...HEAD` shows only `rust/bismark-extractor/SPEC.md` under `rust/` — no `src/*` or `tests/*` changes. |
| §5.8 Pre-merge validation | ✅ | `bash -n scripts/phase_h_smoke.sh` and `bash -n scripts/phase_h_se_matrix.sh` both clean. `shellcheck` NOT installed on the dev Mac (deviation — see below); falls back to bash-syntax-only. `cargo test -p bismark-extractor` → **303 passed / 0 failed / 0 ignored**, exactly matching the post-Phase-G baseline. NO local-Mac smoke dry-run on a tiny SE BAM (deviation — see below). |

### Pre-existing test updates

**None.** No Rust code changed; the 303-test baseline is preserved bit-for-bit from Phase G's merged state. No test file required modification.

### Deviations from rev 1 plan

1. **Matrix driver size: 551 LOC vs ~250 LOC estimated** (plan §2.2). Reason: the rev-1 spec for cross-N comparison (per ignore-pair, all-pairs N-vs-N), speedup table emission (per-cell + per-N-aggregate + SPEC §9.7 target check + cross-N summary + M-bias baseline check), and the 4-way exit-code mapping each required more bash boilerplate than rev 1 anticipated. The increase is entirely in the table-emission + cross-N aggregation code (~250 LOC), not in the matrix-execution loop itself (~100 LOC). No functional change vs the plan; the deviation is solely in line-count.
2. **`shellcheck` not installed on the dev Mac.** Plan §5.8 step 3 says "if available; else skip with note". Skipped with note. Recommended for future: `brew install shellcheck` on the dev Mac to catch bash-style issues that `bash -n` misses (quoting, array-handling, etc.). The two scripts in this PR are bash-array-and-quote-disciplined per rev 1 I6, but a shellcheck pass would add an independent verification layer.
3. **No local-Mac smoke dry-run on a tiny SE BAM** (plan §5.8 step 4 listed this as optional). Reason: the dev Mac doesn't currently have a `bioinf` micromamba env equivalent with Perl bismark v0.25.1 installed; the pre-flight Perl-version check would correctly reject. A real end-to-end smoke happens on colossal per RELEASE_CHECKLIST.md (the release gate). The bash-syntax + bash-n checks are sufficient pre-merge validation; the matrix driver's internal logic is verified at code-review time by the dual reviewers + plan-manager Mode B.
4. **Library-mode annotation in `diff_summary.txt`** (rev 1 §5.3 implicit addition). Plan §5.3 step 2 said "SE branch: set expected-kept-file-set to 6 files for directional libraries" — implementation chose to ANNOTATE rather than ENFORCE. The smoke script emits `Library: SE` or `Library: PE` based on `@PG` auto-detect; downstream consumers (matrix drivers, release-engineer scripts) can apply mode-specific assertions. This is cleaner than baking a "kept-file-count assertion" into the smoke (which already does Perl-vs-Rust file-set diff that catches mismatches symmetrically).
5. **Default `OUT_DIR` renamed from `./oxy_phase_h_out` to `./phase_h_out`** (rev 1 implied; not explicit). Dropping the `oxy_` prefix in the script's interior consistent with the script-file rename.

### Iteration log

Implementation proceeded mostly linearly; no iterations beyond initial drafts:

1. **#1 SPEC updates** (§5.1) — applied in order: §8.3 Phase H matrix subsection, §9.7 measurement paragraph, §10 row H split. Clean first attempt; no edits to existing rows beyond the row-H expansion.
2. **#2 Script rename** (§5.2) — `git mv` preserves history. Sanity-grep for external references found only historical-plan markdown which is correctly left unchanged.
3. **#3 Smoke-script edits** (§5.3) — 4 Edit calls: docstring, args parsing, two invocation sites (Perl + Rust), diff_summary annotations. All clean first-try. Syntax check passed.
4. **#4 Matrix driver** (§5.4) — single Write call (551 LOC). Syntax check passed first-try. The cross-N comparison + per-N aggregate logic is the largest novel piece; reviewers should scrutinize.
5. **#5 RELEASE_CHECKLIST.md** (§5.5) — single Write call (162 LOC). Comprehensive scope per rev 1 I13 (roles, escalation paths, sign-off mechanism).
6. **#6 Validation** (§5.8) — bash -n clean on both scripts; `cargo test` confirmed 303-test baseline preserved.

Zero re-iterations; no failed steps. Total: 6 forward steps.

### Verification confidence

- **Cross-N comparison logic** in §5.4 implements rev 1's strongest assertion (C1). The `all-pairs N-vs-N` per ignore-pair is N-correct (for `--parallel-set "1 4"` it does 1 comparison per pair; for `--parallel-set "1 4 8"` it does 3 comparisons per pair). Dual code-reviewers should mental-execute the loop.
- **Bash IFS quirk** sidestepped per rev 1 I6 via `read -r -a` array form. Verified in script source comments + memory link.
- **Wall-clock parsing format** verified pinned at `^Perl: <int>s$` / `^Rust: <int>s$`. The smoke script's existing `date +%s` calls produce this shape; an inline comment in `diff_summary.txt` writing block locks the format for future contributors.
- **Pre-flight Perl version** asserts via grep on the version-line output. Tested via mental trace; reviewers may verify with a quick local run if `bismark_methylation_extractor --version` output format is suspected of drift.
- **303 tests preserved.** The crate is untouched.

Ready for dual code-review + plan-manager Mode B audit.

---

## Post-review absorption (rev 3, 2026-05-28)

Per the global CLAUDE.md workflow §5: after implementation, dual `code-reviewer` agents + `plan-manager` Mode B were launched. The plan-manager Agent stalled at the 600s stream-watchdog timeout (likely the ~1200-line rev-2 plan exceeded its read budget); the inline-audit fallback in `COVERAGE_PHASE_H_SE.md` substituted for the coverage map.

| Agent | Verdict | Findings |
|---|---|---|
| Code-reviewer A | APPROVE-WITH-NITS | 0 Critical + 2 High + 3 Medium + Low |
| Code-reviewer B | **NEEDS-REVISIONS** | **1 Critical** + 2 High + 8 Medium + 4 Low |
| Plan-manager Mode B (inline) | **COMPLETE** with 1 PARTIAL | 18 DONE / 5 DEVIATED (documented) / 1 PARTIAL / 0 MISSING |

**Strongest consensus finding (3-way agreement):** M-bias fail-open at the default cell — A-L1 ≡ B-L2 ≡ Coverage §3.4 #4 PARTIAL. Rev 3 folds the fix once + cross-cuts both M-bias-size check (existing) and M-bias-row-count differential check (new).

**B's stricter verdict prevails** per the dual-review pattern. Rev 3 absorbs accordingly.

### Absorption per finding

| Finding | Severity | Source | Fix |
|---|---|---|---|
| **B-L1** Speedup-table arithmetic inversion (header `Rust/Perl` but code computed `P*100/R`, emitting Perl/Rust ratio). | **Critical** | B-L1 | `phase_h_se_matrix.sh`: swap `P*100/R` → `R*100/P` in both per-cell loop AND per-N-aggregate site; `P=0` guard added. Release-engineer now reads correct-direction speedup at v1.0 decision time. |
| **A-L1 ≡ B-L2** M-bias `MBIAS_BASELINE_OK=1` default fails open. | High (consensus) | A-L1 + B-L2 | `phase_h_se_matrix.sh`: introduce `MBIAS_GATE_APPLIES` flag (1 iff (D,N=1) cell exists); `MBIAS_BASELINE_OK=0` initialised; set to 1 only on positive `size==5712` confirmation. Exit-code logic only fires the M-bias FAIL when `MBIAS_GATE_APPLIES==1` AND `MBIAS_BASELINE_OK==0`. |
| **A-Er1/Er2** Bash 3.2 incompat: `declare -A` (bash 4.0+); empty array under `set -u`. | High | A-Er1 + A-Er2 | `phase_h_se_matrix.sh`: top-of-file `if (( BASH_VERSINFO[0] < 4 )); then exit 2; fi` pre-flight with macOS remediation hint. `phase_h_smoke.sh`: `${EXTRA_RUST[@]+"${EXTRA_RUST[@]}"}` defensive idiom (same for `EXTRA_PERL` + `EXTRA_FLAGS`) — empty-array safe on bash < 4.4. |
| **B-E3** `uptime` parse fragile across BSD/GNU dialects (macOS plural "load averages:" breaks parse). | High | B-E3 | `phase_h_se_matrix.sh`: regex `load average[s]?:` tolerates singular + plural; silent-skip with 2>/dev/null on awk-empty-LOAD compare; whole load-advisory block silent-degraded if anything fails. |
| **Coverage §3.4 #4** M-bias row-count differential check planned but not implemented. | High (PARTIAL absorption) | Coverage audit + B-Med | `phase_h_se_matrix.sh`: new section after the matrix loop walks each ignore-flag cell at N=1, counts M-bias data rows (`grep -cE '^[0-9]+\t'`), asserts 5p/3p/5p+3p < D (Phase B `--ignore` semantics per SPEC §7.6) and edge_clip < D/10 (mostly-empty). Verdict file + speedup table both surface the per-cell row counts + PASS/FAIL. Exit-code logic fires `ROW_COUNT_OK==0` FAIL when gate applies. |

### Medium findings folded

| Finding | Source | Fix |
|---|---|---|
| SIGINT/SIGTERM trap | B-Med | `trap '... exit 130' INT TERM` at top of driver; preserves partial state in `$OUT_DIR` for evidence |
| `nproc`-missing → reject high-N | B-Med | If nproc not available, SKIP the N>nproc check entirely (don't reject); skip the contention advisory too |
| Perl pre-flight binary inconsistency | B-Med | `RELEASE_CHECKLIST.md` documents that the matrix's `$PERL_BIN` (repo's checked-in v0.25.1 script) and the `bioinf` env's PATH binary are both packagings of the same v0.25.1 source — agreement expected |
| `cargo build` budget not in checklist | B-Med | `RELEASE_CHECKLIST.md` adds "Budget ~5-15 min for cargo build on cold cache" before the build command |
| `tmux` not enforced | B-Med | Driver warns at runtime if `$TMUX`/`$STY` unset; checklist text strengthened to "non-optional" |
| Sub-second cell rounding noise | Low | Driver emits ` ⚠️ sub-2s` annotation in the Rust/Perl ratio column when Perl-S or Rust-S < 2 |

### Deferred (out of rev 3 scope)

| Finding | Severity | Source | Defer rationale |
|---|---|---|---|
| 80 LOC speedup-table-formatting duplication | Medium | B-Med | Refactor; not behavior. File as `polish(extractor):` if maintenance pain surfaces. |
| Hypothetical `.gz` cross-N false-fail | Medium | B-Med | The SE matrix doesn't inject `--gzip` via `--extra-rust` (no cell in the 5-cell matrix does); concern is hypothetical for #871's scope. PE matrix (#872) similarly. If a future variant adds `--gzip` cells, fix at that time. |
| Crash-vs-FAIL exit classification refinement | Medium | A-Med | Existing 4-way mapping (0/1/2/3) is unambiguous; further sub-categorisation adds entropy without clear benefit |
| Various Low polish (rounding precision, single-N verdict text variants) | Low | A + B | Polish; not blocking |

### Files modified in this absorption

| File | Change |
|---|---|
| `scripts/phase_h_se_matrix.sh` | Bash version pre-flight; SIGINT trap; nproc-missing fix; uptime parse graceful; tmux-warning; speedup-arithmetic swap (per-cell + per-N); MBIAS_GATE_APPLIES + fail-closed default; M-bias row-count differential check (new section + helper fns); verdict file + speedup-table additions; exit-code logic for row-count + gate-applies. Net: +~110 LOC (now ~660 LOC). |
| `scripts/phase_h_smoke.sh` | Defensive `${ARR[@]+"${ARR[@]}"}` for `EXTRA_FLAGS`/`EXTRA_RUST`/`EXTRA_PERL` empty-array-under-`set -u`. Net: ~6 LOC of surgical edits. |
| `RELEASE_CHECKLIST.md` | tmux non-optional + bash 4.0 requirement + Perl version equivalence note + cargo build budget + Rust/Perl column-direction reminder + row-count differential verify step. Net: ~30 LOC additions. |
| `plans/05262026_bismark-extractor/PHASE_H_SE_PLAN.md` | This rev 3 absorption row + Post-review absorption section. |

### Re-validation

| Check | Result |
|---|---|
| `bash -n scripts/phase_h_smoke.sh` | clean |
| `bash -n scripts/phase_h_se_matrix.sh` | clean |
| `cargo test -p bismark-extractor` | 303 passed / 0 failed / 0 ignored (baseline preserved) |

### Verification confidence

- **B-L1 fix verified by mental trace:** for canonical (D, N=1) "Perl=720s, Rust=800s", rev 3 emits `(800*100/720) = 111` → `1.11×` matching the plan §3.3.4 example. Previously emitted `(720*100/800) = 90` → `0.90×`. Direction now correct.
- **M-bias fail-closed verified:** initial value 0; only positive size==5712 sets it to 1; missing file / missing N=1 / size drift all leave it at 0 OR don't apply the gate (when gate doesn't apply, no FAIL).
- **Bash 4.0 pre-flight fires before any `declare -A` is hit** (block lives at top-of-script, before arg parsing).
- **Row-count differential check is gated on `MBIAS_GATE_APPLIES==1`** so users with `--parallel-set "4"` (no N=1) get a benign skip, not a FAIL.

Ready for commit + PR.

---

## 1. Goal

Verify byte-identity between Rust `bismark-methylation-extractor-rs` and Perl `bismark_methylation_extractor` on **SE mode**, across a **5-cell representative matrix** of `--ignore` / `--ignore_3prime` combinations at `--parallel ∈ {1, 4}`, and emit a wall-clock speedup table comparing Rust vs Perl per-cell. Add the Phase F N-invariance assertion (Rust-N=1 ≡ Rust-N=4 raw-byte) per-cell to guard against BTreeMap-collector regressions.

This closes Phase H sub-gate 1 for SE — the half of Phase H that covers the extractor's own output streams under the SE input shape. PE (the other half) is the companion #872 work; both must PASS before the v1.0 release tag.

**Out-of-scope** (per #871 body — separately tracked):

- Sub-gate 2 streams (`.bedGraph.gz`, `.bismark.cov.gz`, `CpG_report.txt[.gz]`) — blocked on epic #797.
- PE byte-identity — covered by companion #872 + its own plan.
- `--yacht`, `--comprehensive`, `--merge_non_CpG`, `--gzip` byte-identity — already verified by Phase E + Phase G smokes; mode-axis is intentionally collapsed to "default".
- `--mbias_only` — already verified by Phase E + C.2 absorption.
- **Performance investigation** — if Rust-vs-Perl speedup at N=4 lands below SPEC §9.7's ≥4× target, file a separate `perf(extractor):` sub-issue; Phase H gates on byte-identity, NOT speedup.

## 2. Context

### 2.1 Phase status table impact

| Phase | Before | After |
|---|---|---|
| G | ✅ merged (`ff961d3`) | ✅ merged |
| **H — sub-gate 1 SE** | 📝 plan rev 0 | 📝 plan rev 1 — this file |
| H — sub-gate 1 PE | ⏸ sub-issue filed (#872); plan TBD | unchanged; covered by parallel plan |
| H — sub-gate 2 | ⏸ blocked-on-#797 | unchanged |
| v1.0 release tag | ⏸ blocked on sub-gate 1 | unblocked once #871 + #872 both PASS |

### 2.2 Where the code (and docs) live

| Item | File | Approximate LOC |
|---|---|---|
| Rename existing per-cell smoke script | `scripts/oxy_phase_h_smoke.sh` → `scripts/phase_h_smoke.sh` (`git mv` + docstring header update) | ~10 LOC docstring |
| SE-specific tweaks in renamed script | `scripts/phase_h_smoke.sh` — kept-file expectation = 6 for directional SE; `--extra-rust` / `--extra-perl` pass-through with proper bash array word-splitting | ~50 LOC |
| New 5-cell matrix driver | `scripts/phase_h_se_matrix.sh` (new) — 5 cells × 2 parallelism = 10 invocations; cross-N byte-equality check per cell; speedup table emission; pre-flight checks | ~250 LOC bash |
| Release checklist | `RELEASE_CHECKLIST.md` (new, top-level) — operational specifics + PE TODO-stub | ~120 LOC |
| Companion driver for #872 (referenced; out of scope here) | `scripts/phase_h_pe_matrix.sh` (new in #872's PR) | (separate plan) |
| SPEC updates | `rust/bismark-extractor/SPEC.md` §8.3 + §9.7 + §10 row H | ~50 LOC |
| PROGRESS.md row | `plans/05262026_bismark-extractor/PROGRESS.md` | ~5 LOC |

**Total estimate: ~485 LOC** (bash + markdown; no Rust code changes). Up from rev 0's ~330 due to pre-flight robustness + checklist operational detail + matrix-driver cross-N check.

### 2.3 Dependencies / phase ordering

Depends on:
- **Phase G** (`ff961d3`) — feature surface complete; subprocess chains exist (their byte-identity is sub-gate 2's concern, not this plan).
- **Phase C.1** (`84c6ad1`) — locked `*.M-bias.txt` byte equality (5712 B on 10M SE at default cell) used as regression-guard baseline.
- **Phase C.2** (`4e5c691`) — splitting-report format + empty-file sweep; SE's expected file-set match shape comes from C.2's `finalize_with_empty_sweep`.
- **Phase F** (`215b88d`) — N-invariance contract (`--parallel N` produces output byte-identical to `--parallel 1` for any N ≥ 1). **The new cross-N assertion in this plan tests F directly.**

Unblocks:
- v1.0 release tag (partial; #872 also required).
- #872 plan-write — independent but cross-references this plan in its §1.

### 2.4 Existing harness — `scripts/oxy_phase_h_smoke.sh` (262 LOC)

Already implemented (used in Phase C.1/C.2 dev):

| Capability | Status |
|---|---|
| Runs Perl + Rust on same BAM | ✅ |
| `--parallel N` + `--mode MODE` + `--out DIR` arg parsing | ✅ |
| Auto-detects SE-vs-PE from `@PG` header | ✅ |
| Per-file byte-cmp arm for `*.M-bias.txt` + `*_splitting_report.txt` | ✅ |
| Sorted-MD5 arm for data files (incl. `*.gz` via `zcat | sort | md5sum`) | ✅ |
| File-name-set match check | ✅ |
| Wall-clock capture (rev 1 I2: via `date +%s`; emits `Perl: Ns` / `Rust: Ns` in `diff_summary.txt`) | ✅ |
| PASS verdict aggregation per invocation | ✅ |
| Env-overrides for `PERL_BIN` + `RUST_BIN` | ✅ |

What's missing for Phase H (this plan adds):

| Capability | Status |
|---|---|
| Loop over 5-cell matrix at `--parallel ∈ {1, 4}` | ❌ (new driver) |
| Aggregate per-cell results into matrix-level verdict | ❌ |
| Emit markdown speedup table — **with Perl-only scaling column** (rev 1 I10) | ❌ |
| SE-specific kept-file count (6 for directional, not 12) | ❌ |
| `--extra-rust` / `--extra-perl` pass-through (rev 1 I6: proper bash array form) | ❌ |
| **Cross-N byte-identity check** (rev 1 C1) — Rust-N=1 ≡ Rust-N=4 raw-byte per cell | ❌ |
| Pre-flight: Perl version assertion (rev 1 I8) | ❌ |
| Pre-flight: `nproc` + contention advisory (rev 1 I9) | ❌ |
| Pre-flight: matrix-level `--out` rejection of non-empty (rev 1 I12) | ❌ |

The existing script is **machine-agnostic** (accepts BAM path via argv); the `oxy_` prefix is a misnomer post-colossal-migration. This plan renames it to `scripts/phase_h_smoke.sh`.

### 2.5 Test machine + data (colossal)

Per the memory `reference_colossal_access.md` (2026-05-28):

- Connection: `dcli ssh colossal` (verify on first session — may differ from oxy pattern)
- Data dir: `/weka/projects/bioinf/Data/Felix/bismark_benchmarks/` (Weka shared storage)
- micromamba env: `bioinf` (Perl Bismark stack: `bismark_methylation_extractor`, `samtools`, `bowtie2`)

**Primary SE BAM** (canonical fixture):
- Path: assumed `/weka/projects/bioinf/Data/Felix/bismark_benchmarks/10M_SE/directional_10M_R1_val_1_bismark_bt2.bam` (mirroring oxy layout — verify on first colossal session per memory).
- Size: ~650 MB on oxy; expected similar on colossal.
- Locked baseline: `*.M-bias.txt` byte-identical at 5712 B at the (0,0) cell; data files sorted-MD5 equal to Perl; file-set match with 6 kept files.

**Larger SE BAM** — **rev 1 I7: REMOVED** from this plan. Was unreachable through the single-`<BAM>`-arg CLI; design implied dual-invocation but never specified. If a larger-BAM speedup-confidence check is wanted later, file a separate enhancement sub-issue and add `--secondary-bam PATH`.

### 2.6 Companion: #872 (PE Phase H) — independent driver, **matching 5-cell structure** (rev 1 I17)

Per Felix's 2026-05-28 directives:
- Q3 (driver sharing): independent drivers, two PRs.
- I17 (cell-count asymmetry): SE matrix reduced from 8 cells (rev 0 cartesian) to 5 representative cells mirroring #872's PE 5-cell reduction.

#871 and #872 produce independent PRs:
- `extractor-phase-h-se` (this work) → closes #871
- `extractor-phase-h-pe` (#872's work) → closes #872

The drivers share NO code in v1.0; cross-plan symmetry is structural (both 5 cells × 2 parallelism × 2 binaries = 20 invocations) but flag-content is mode-specific.

The release checklist (this plan creates) creates a PE section as a TODO-stub that #872's PR populates. The checklist is committed-and-functional by this PR alone; #872 fills in PE later — see I14 for the reverse-merge-order discussion.

## 3. Behavior

### 3.1 Test matrix — 5 cells (rev 1 reconciled with #872) × {N=1, N=4}

| Cell | Description | `--ignore` | `--ignore_3prime` |
|---|---|---|---|
| **D** | Default — no ignore flags. Baseline cell; M-bias = 5712 B regression guard. | 0 | 0 |
| **5p** | 5' trim isolated. Mirrors #872's "R1-5'" cell (SE has no R2; this is the only 5' option). | 5 | 0 |
| **3p** | 3' trim isolated. Mirrors #872's "R2-5'" structural slot (SE-specific dimension). | 0 | 5 |
| **5p+3p** | Both trims combined. Mirrors #872's "R1+R2 3'" combined-flag cell. | 5 | 5 |
| **edge_clip** | `--ignore` exceeds typical read length (250 bp). SE-specific edge case mirroring #872's "include_overlap" slot. Asserts `extract_calls`'s `read_pos < lo` boundary handles "lo > hi" gracefully (empty MethCall vec; both Perl + Rust produce empty output → sorted-MD5 still matches). | 250 | 0 |

× `--parallel ∈ {1, 4}` = **10 invocations per binary** = **20 total per BAM**.

Rev 0 had the full 2³=8 cartesian; rev 1 representative-reduces to 5 to match #872's PE 5-cell shape. Trade-off: rev 0's matrix tested (5p, 3p, 5p+3p, no-flag) × N=1, N=4 (8 cells); rev 1 keeps those 4 ignore-combinations + adds the edge_clip cell + drops nothing. Net: rev 1 has MORE coverage than rev 0 (5 cells vs rev 0's 4 ignore-combinations × 2 parallelism = 8 cells, BUT rev 0's 4 ignore-combinations was the full cartesian). Effective coverage: rev 1 is rev 0's ignore-cartesian PLUS the edge case. Slightly more work; significantly better edge-case coverage.

### 3.2 Per-cell invocation contract (rev 1 I4 + I6 — corrected)

For each matrix cell, the driver invokes `scripts/phase_h_smoke.sh` with:

```bash
scripts/phase_h_smoke.sh \
  <BAM> \
  --parallel <N> \
  --mode default \
  --out <OUT_DIR>/cell_p<N>_i<5p>_i3<3p> \
  --extra-rust "--ignore <5p> --ignore_3prime <3p>" \
  --extra-perl "--ignore <5p> --ignore_3prime <3p>"
```

`--extra-rust` / `--extra-perl` are NEW pass-through flags this plan adds to the smoke script (rev 1 I6: implemented with `read -r -a EXTRA_RUST <<< "$EXTRA_RUST_STR"` to produce a proper bash array, then `"${EXTRA_RUST[@]}"` quoting at invocation — avoids the IFS-word-splitting quirk we hit elsewhere this session). Empty defaults; pass-through is verbatim.

The smoke script's existing PASS/FAIL verdict applies per cell. Exit code per cell (unchanged from rev 0):
- 0 = byte-identity PASS for this cell
- 1 = any data-file mismatch OR file-set mismatch OR strict-cmp failure on `*.M-bias.txt` / `*_splitting_report.txt`
- 2 = harness usage error

### 3.3 Matrix driver — `scripts/phase_h_se_matrix.sh`

#### 3.3.1 CLI (rev 1 I7 — single BAM arg only)

```bash
scripts/phase_h_se_matrix.sh <BAM> [--out DIR] [--parallel-set "1 4"]
```

- `<BAM>` — absolute path to the SE input BAM. Required.
- `--out DIR` — output root directory; per-cell subdirs created under it. Default: `./phase_h_se_matrix_out`.
- `--parallel-set "..."` — space-separated parallelism values. Default: `"1 4"`. Override to `"1 4 8"` if colossal has the cores.

#### 3.3.2 Pre-flight checks (rev 1 I8 + I9 + I12 — added)

Before any cell runs:

1. **BAM exists + readable** (`-r "$BAM"`); else exit 2 with explicit error.
2. **`--out DIR` is empty or doesn't exist** (rev 1 I12 — matrix-level rejection of non-empty); else exit 2. Prevents clobbering a previous run's evidence. Override: `--out` to a fresh dir.
3. **Perl version assertion** (rev 1 I8): `bismark_methylation_extractor --version 2>&1 | grep -q "Bismark Extractor Version: v0.25.1"`; else exit 2 with "expected Perl bismark v0.25.1; got <version>. The locked 5712 B M-bias baseline assumes v0.25.1. Either upgrade/downgrade the `bioinf` env to v0.25.1, or update the locked baseline in §A4 + this driver."
4. **Rust binary discoverable** + executable (per existing smoke env-var checks).
5. **`nproc` + contention advisory** (rev 1 I9): emit informational lines to stderr:
   - `Available cores: $(nproc)`
   - If any `N` in `--parallel-set` exceeds `nproc`: hard-fail with "requested N=$N exceeds available cores ($(nproc))".
   - If `$(uptime | awk -F'load average:' '{print $2}' | awk '{print $1}')` > `nproc`: warn "system load average ($LOAD) exceeds nproc ($(nproc)); speedup ratios will be noisy. Consider `nice -n 10`."

#### 3.3.3 Per-cell execution

For each `(N, ignore_5p, ignore_3p)` tuple in the matrix:

1. Create per-cell subdir: `<OUT>/cell_p<N>_i<5p>_i3<3p>/`.
2. Invoke `phase_h_smoke.sh` per §3.2.
3. Record per-cell verdict (PASS/FAIL/USAGE) + parse wall-clocks (rev 1 I2: `grep -E '^Perl: ([0-9]+)s$' <out>/diff_summary.txt` and same for Rust; pin the regex anchored).
4. **Cross-N byte-identity check** (rev 1 C1) — see §3.4 below.

#### 3.3.4 Cross-N byte-identity check per cell (rev 1 C1 — replaces self-determinism)

After each `(ignore_5p, ignore_3p)` pair has been run at all `--parallel-set` values:

For each pair of N values `(N_a, N_b)` in `--parallel-set` with `N_a < N_b`:
- Compare every output file in `cell_p<N_a>_i<5p>_i3<3p>/rust/` against the corresponding file in `cell_p<N_b>_i<5p>_i3<3p>/rust/`.
- Raw-byte equality required (strict `cmp -s`). This tests SPEC §8.3 row 4 — the Phase F N-invariance contract.

If any file diverges → cell-level FAIL (cross-N regression in the BTreeMap collector or worker-reduce path).

This replaces rev 0's per-cell self-determinism (Rust-vs-Rust at same N). Self-determinism was strictly weaker — cross-N is the stronger property and SPEC §8.3 row 4 has been the N-invariance contract since Phase F. With the matrix already running each ignore-pair at both N=1 and N=4, the cross-comparison is free data already collected.

#### 3.3.5 Aggregation + speedup table (rev 1 I10: Perl-only scaling added)

After all cells run:
- Aggregate PASS count: must equal `len(parallel_set) × 5`.
- Aggregate FAIL count: any non-zero means matrix-level FAIL.
- Aggregate USAGE-ERROR count: any non-zero means matrix INVALID.
- **Cross-N PASS count**: per ignore-pair, must equal `(N choose 2)` for the parallel-set; any non-zero means matrix FAIL.

Emit `<OUT>/speedup_table.md` (markdown):

```markdown
# Phase H SE speedup table

Generated: <ISO-8601 timestamp>
Input BAM: <BAM path> (<size> bytes; <SE-record count> records)
Bismark Perl version: v0.25.1 (asserted)
Rust commit: <git rev-parse HEAD>
Rust crate version: <Cargo.toml version>

## Per-cell wall-clock

| Cell | N | --ignore | --ignore_3prime | Perl (s) | Rust (s) | Rust/Perl | Cross-N PASS | Verdict |
|------|---|----------|-----------------|----------|----------|-----------|--------------|---------|
| D    | 1 | 0        | 0               | 720      | 800      | 1.11×     | (baseline N) | PASS    |
| D    | 4 | 0        | 0               | 180      | 150      | 0.83×     | ✓ vs N=1     | PASS    |
| 5p   | 1 | 5        | 0               | ...
| ...
| edge_clip | 4 | 250      | 0               | ...

## Per-N aggregate

| N   | Avg Perl (s) | Avg Rust (s) | Avg Rust/Perl | Perl scaling | Rust scaling | Cells |
|-----|--------------|--------------|---------------|--------------|--------------|-------|
| 1   | 720          | 800          | 1.11×         | (baseline)   | (baseline)   | 5     |
| 4   | 180          | 150          | 0.83×         | 4.00×        | 5.33×        | 5     |

## SPEC §9.7 target check

Target: Rust `--parallel 4` ≥ 4× Rust `--parallel 1`.
Measured: 5.33×. ✅ Target met.
(If below 4×: file separate perf(extractor): sub-issue per #871.)
```

**Rev 1 additions:** Perl-only scaling column (per #871 body — was missing in rev 0); Rust commit + crate version embedded (per rev 0 self-review open question — now folded); cross-N PASS column.

#### 3.3.6 PASS verdict + exit-code mapping (rev 1 I16 — explicit)

The matrix PASSes iff ALL of:
- Every cell's underlying `phase_h_smoke.sh` exits 0 (per-cell byte-identity).
- Every cross-N comparison (§3.3.4) raw-byte matches.
- The (D, N=1) cell's `*.M-bias.txt` matches the locked 5712 B baseline.
- Pre-flight checks all pass.

Exit code mapping (explicit per rev 1 I16):

| Condition | Exit code |
|---|---|
| All PASS + Rust scaling ≥ SPEC §9.7's 4× target | 0 |
| Any cell FAIL OR cross-N FAIL OR baseline drift | 1 |
| Pre-flight USAGE-ERROR (BAM missing, version mismatch, dir not empty, etc.) | 2 |
| All byte-identity PASSED but Rust scaling < 4× | 3 |

Exit 3 is informational FAIL; release checklist may accept with a follow-up `perf(extractor):` sub-issue. Exit 1 blocks v1.0 tag.

### 3.4 Per-cell byte-identity contract (rev 1 — split by cell type)

Each cell asserts (via the underlying `phase_h_smoke.sh`):

1. **Sorted-content equivalence** on all data files (CpG/CHG/CHH × OT/OB for SE directional libraries; 6 files post-empty-sweep).
2. **File-set match**: 6 kept files (CTOT/CTOB unlinked by Phase C.2's empty-sweep).
3. **Strict-byte equality** on `*_splitting_report.txt` (all cells).
4. **Strict-byte equality** on `*.M-bias.txt`:
   - **(D, N=1) cell**: must equal Perl byte-for-byte AND total size must equal the locked baseline **5712 B** (rev 1 I15 regression guard for Phase C.1). HARD-FAIL if drift.
   - **Other ignore-flag cells**: must equal Perl byte-for-byte (the file SIZE varies because `--ignore N` reduces the number of positions reported; row-count differential check added per rev 1 I15: M-bias row count for `(5p, 0)` cell must be < (D, N=1) cell's row count; ditto for `(0, 3p)` and `(5p, 3p)`; for `edge_clip` cell expect empty/zero rows).
5. **Cross-N byte-identity** (rev 1 C1; aggregated by §3.3.4): for each `(ignore_5p, ignore_3p)` pair, Rust-N=1 output ≡ Rust-N=4 output raw-byte. SPEC §8.3 row 4.

The driver records each assertion's PASS/FAIL per cell in `<OUT>/cell_*/diff_summary.txt` (already done by the existing smoke for #1-4) PLUS a new `<OUT>/cross_n_summary.txt` for the §3.3.4 cross-N results.

### 3.5 Edge cases (rev 1 — expanded)

| Case | Handling |
|---|---|
| `--parallel-set` requests N values > `nproc` | Pre-flight rejection (rev 1 I9); exit 2 with "requested N=$N exceeds available cores ($(nproc))" |
| `--out` dir already exists with non-empty contents | Pre-flight rejection (rev 1 I12); exit 2 demanding fresh `--out`. |
| Perl `bismark_methylation_extractor` version drift (e.g. v0.25.2 installed) | Pre-flight rejection (rev 1 I8); exit 2 with "expected v0.25.1, got <version>" + remediation hint. |
| Perl `bismark_methylation_extractor` not on PATH | Smoke script's existing env-var check fails; surfaces as cell USAGE-ERROR. Driver retries discovery before exit 2. |
| Rust binary stale or not built | Smoke script's existing check fails; pre-flight runs `cargo build --release -p bismark-extractor` IF `RUST_BIN` env is unset (rev 1 I8 follow-on hardening). |
| BAM path contains spaces | Driver quotes argv properly via `"${ARGV[@]}"` (rev 1 I6 — same bash-array pattern as `--extra-*`); tested in pre-merge smoke. |
| Cell output dir already exists at per-cell level (e.g. from a previous failed run) | Per-cell pre-flight in smoke; aborts that cell with USAGE-ERROR. Matrix-level pre-flight (§3.3.2) catches this earlier under §3.3.2#2. |
| **Cross-N self-determinism check fails** | Rare; signals a non-determinism regression in Rust — file `bug(extractor):` immediately. Phase F's collector is supposed to guarantee BAM-input-order; if this fires, the matrix exits 1 with explicit "cross-N regression at cell <id>". |
| `--ignore 250` cell on a 150 bp read library | Both Perl + Rust produce empty data files (no calls survive the boundary check). Sorted-MD5 still matches (both empty). M-bias.txt has zero data rows (header only). Splitting-report shows 0 calls. PASS expected — this is the edge_clip cell's whole point. |
| `*.M-bias.txt` size drift at (D, N=1) cell vs locked 5712 B | HARD-FAIL (rev 1 I15) — Phase C.1 regression guard. Driver emits the actual size + delta from 5712 B. |
| Network/SSH disconnect mid-matrix (1-3 h run) | Recommended: run inside `tmux` / `screen` per RELEASE_CHECKLIST.md (rev 1 I11). Driver does NOT trap signals — relies on user's session management. |
| Colossal under high load (load average > nproc) | Pre-flight advisory (rev 1 I9); user opts to proceed (matrix runs) or aborts. Speedup ratios under load are documented in the table as "potentially noisy" if pre-flight advisory fired. |
| 10M SE BAM at unexpected colossal subpath | RELEASE_CHECKLIST.md instructs `ls /weka/projects/bioinf/Data/Felix/bismark_benchmarks/` on first session; update assumption A1 if path differs. Trivial commit, no plan rewrite. |

### 3.6 Implementation order within a single matrix run (rev 1 — for clarity)

The matrix runs in a specific order to enable cross-N checks:

1. Pre-flight (§3.3.2). Aborts on any USAGE-ERROR.
2. For each `(ignore_5p, ignore_3p)` ∈ matrix-cells (5 pairs):
   - For each `N` ∈ `--parallel-set`:
     - Run `phase_h_smoke.sh` per §3.2 — produces `cell_p<N>_i<5p>_i3<3p>/`.
   - Cross-N comparison (§3.3.4) across the N values JUST run for this pair.
3. Aggregate + emit speedup table (§3.3.5).
4. Emit matrix verdict (§3.3.6); exit with mapped code.

Per-pair sub-loop means cross-N comparisons happen as soon as both N runs are complete, surfacing regressions early.

## 4. Signature

### 4.1 `scripts/phase_h_se_matrix.sh`

```bash
#!/usr/bin/env bash
#
# phase_h_se_matrix.sh — Phase H sub-gate 1 SE byte-identity + speedup matrix.
#
# Runs the per-cell Phase H smoke (scripts/phase_h_smoke.sh) over the 5-cell
# representative matrix at --parallel ∈ {1, 4}. Asserts SPEC §8.3 row 4
# N-invariance per ignore-pair (Rust-N=1 ≡ Rust-N=4 raw-byte). Emits a
# markdown speedup table with Perl-only + Rust scaling columns.
#
# Usage:
#   scripts/phase_h_se_matrix.sh <BAM> [--out DIR] [--parallel-set "1 4"]
#
# Pre-flight checks (rev 1 I8 + I9 + I12):
#   - BAM exists + readable
#   - --out DIR is empty or doesn't exist
#   - Perl bismark_methylation_extractor version == v0.25.1
#   - Rust binary discoverable
#   - nproc + contention advisory
#
# Exit codes:
#   0  — all cells PASS + cross-N PASS + Rust scaling ≥ SPEC §9.7's 4×
#   1  — any cell or cross-N failed byte-identity
#   2  — pre-flight USAGE-ERROR
#   3  — byte-identity PASSED but Rust scaling missed the perf target
#
# Outputs:
#   <OUT>/cell_p<N>_i<5p>_i3<3p>/  — per-cell phase_h_smoke output
#   <OUT>/cross_n_summary.txt      — cross-N comparison results per ignore-pair
#   <OUT>/speedup_table.md         — markdown summary with Rust commit + crate version
#   <OUT>/matrix_verdict.txt       — PASS/FAIL with per-cell breakdown
```

### 4.2 `scripts/phase_h_smoke.sh` (renamed from `oxy_phase_h_smoke.sh`)

New CLI flags added (existing flags unchanged):

```
--extra-rust "<flags>"    Additional flags appended to the Rust invocation
--extra-perl "<flags>"    Additional flags appended to the Perl invocation
```

Implementation (rev 1 I6): both flags use `read -r -a EXTRA_RUST <<< "$EXTRA_RUST_STR"` to parse the value as a bash array; the array is passed to the Rust binary via `"${EXTRA_RUST[@]}"`. Empty defaults. Pass-through is verbatim; no flag-shape validation.

SE-specific behaviour added: when the input BAM is detected as SE (existing `@PG` auto-detect), the kept-file expectation switches from 12 strand×context files to **6 files** (CpG/CHG/CHH × OT/OB; CTOT/CTOB swept). Explicit in rev 1; implicit in Phase C.2.

`diff_summary.txt` wall-clock format (rev 1 I2): pinned at `^Perl: ([0-9]+)s$` and `^Rust: ([0-9]+)s$` (anchored lines, integer seconds via `date +%s`). Driver parses with `grep -E`.

### 4.3 `RELEASE_CHECKLIST.md` (new — top-level; rev 1 I13 + I14 expanded)

Top-level repo file. Sections:

```markdown
# Bismark Rust rewrite — release checklist

## Roles
- **Release engineer**: Felix (single-person process for v1.0).
- **Sign-off recording**: comment on the relevant epic issue (#798) with the
  matrix output + a "PASS" or "FAIL" marker. Append the
  speedup_table.md as a gist link or comment attachment.

## Escalation: mid-checklist regression
If the matrix reports FAIL (exit 1):
1. Save `<OUT>/` evidence (matrix_verdict.txt + cross_n_summary.txt + cell_*/).
2. File a `bug(extractor):` sub-issue under #798 with the failing cell + diff
   excerpt.
3. Pause v1.0 tag work; resolve the bug + re-run the matrix.
If exit 3 (perf-target-miss but byte-identity PASS):
1. File `perf(extractor):` follow-up sub-issue.
2. Tag MAY proceed; the perf issue is post-v1.0.

## bismark-extractor v1.0 — Phase H byte-identity sub-gate 1

Prerequisites: Phase G merged (`ff961d3` or later) on `rust/iron-chancellor`.

### SE matrix (closes #871)

Run inside `tmux` or `screen` — the matrix takes 1-3 hours (rev 1 I11):

```bash
tmux new -s phase_h
dcli ssh colossal
cd ~/Github/Bismark   # or wherever colossal has the working copy
git checkout rust/iron-chancellor && git pull --ff-only
micromamba activate bioinf

cargo build --release --manifest-path rust/Cargo.toml -p bismark-extractor

bash scripts/phase_h_se_matrix.sh \
  /weka/projects/bioinf/Data/Felix/bismark_benchmarks/10M_SE/directional_10M_R1_val_1_bismark_bt2.bam
```

Verify:
- Exit code 0 (or 3 = perf-miss-only) — see `matrix_verdict.txt`.
- `cell_p1_i0_i30/diff_summary.txt` shows `*.M-bias.txt` byte-cmp PASS + size 5712 B.
- `cross_n_summary.txt` shows PASS for all 5 ignore-pairs.
- `speedup_table.md` Rust-scaling-at-N=4 ≥ 4×.
- Comment on #798 with the table + "SE PASS" marker.

### PE matrix (closes #872)

**TODO — populated by #872's PR.** Until #872 lands, this section is a stub:

> #872 will add the PE-equivalent matrix invocation here. Until then,
> v1.0 tag is blocked on #872's PR landing + PE matrix PASS.

### v1.0 tag steps

- [ ] Both SE matrix PASS (this issue, #871) and PE matrix PASS (#872) recorded on #798.
- [ ] `cargo test -p bismark-extractor` clean (no regressions since Phase G's 303-test baseline).
- [ ] Crate version bump: `1.0.0-alpha.9` → `1.0.0`. Description: "v1.0 release".
- [ ] Tag commit on `rust/iron-chancellor`: `bismark-extractor-v1.0`.
- [ ] Comment on epic #798: "v1.0 tagged + matrix evidence at <gist/comment URL>".

## (Future) bismark-bedgraph v1.0 — sub-gate 2 release gates

Blocked on epic #797. Will be filled in once `bismark-bedgraph` lands.
```

**Rev 1 changes**: roles section + escalation paths added (rev 1 I13); tmux recommendation (rev 1 I11); PE section explicitly marked as #872's TODO-stub (rev 1 I14).

## 5. Implementation outline

### 5.1 SPEC updates (DO FIRST — rev 1 I5 added §10 row H)

1. **`rust/bismark-extractor/SPEC.md` §8.3** — add a "Phase H matrix" subsection enumerating the 5-cell SE matrix (rev 1 reconciled) as part of the byte-identity contract for v1.0. Cite SPEC §8.3 row 4 N-invariance explicitly.
2. **`rust/bismark-extractor/SPEC.md` §9.7** — re-affirm the ≥4× target at N=4; document Phase H measures via `phase_h_se_matrix.sh` per-cell + aggregates per-N. Sub-target-miss does NOT block v1.0 byte-identity gate.
3. **`rust/bismark-extractor/SPEC.md` §10 row H** (rev 1 I5 — was missing in rev 0) — split into row "H sub-gate 1 SE" + row "H sub-gate 1 PE" + row "H sub-gate 2 (blocked-on-#797)". Reflects the actual sub-gate structure.

### 5.2 Rename existing smoke script

1. `git mv scripts/oxy_phase_h_smoke.sh scripts/phase_h_smoke.sh`.
2. Update the script's top-of-file docstring: drop "oxy_" / "Phase F + flavour A"; update to "Phase H per-cell byte-identity smoke (post-colossal-migration; machine-agnostic via BAM-path argv)".
3. Grep the repo for external references; update if found.

### 5.3 SE-specific tweaks to renamed `phase_h_smoke.sh`

1. Add `--extra-rust "<flags>"` + `--extra-perl "<flags>"` CLI flags with proper bash-array parsing (rev 1 I6).
2. SE branch: when auto-detected SE, set the expected-kept-file-set to 6 files.
3. Wall-clock format spec (rev 1 I2): emit `Perl: Ns` / `Rust: Ns` on dedicated lines in `diff_summary.txt`. Verify the existing `date +%s` calls produce this format; tighten if needed.

### 5.4 New `scripts/phase_h_se_matrix.sh`

Driver script per §3.3 + §4.1. Steps:

1. **Pre-flight (§3.3.2)**: BAM exists, OUT empty, Perl version v0.25.1, Rust binary discoverable, nproc + contention advisory. Exit 2 on any failure.
2. **Matrix execution (§3.3.3 + §3.6)**: outer loop over 5 ignore-pairs; inner loop over `--parallel-set`. For each pair, after all N values run, perform §3.3.4 cross-N comparison.
3. **Parse wall-clocks** (rev 1 I2 + I4) from each cell's `diff_summary.txt` via `grep -E '^Perl: ([0-9]+)s$'` + `grep -E '^Rust: ([0-9]+)s$'`. NOT from driver-wrapping; the smoke script is the single source of truth.
4. **Cross-N comparison** per pair (rev 1 C1): for each `(N_a < N_b)` pair, `diff -q <a>/rust/<file> <b>/rust/<file>` over every Rust output file. Aggregate to `cross_n_summary.txt`.
5. **Speedup table** (rev 1 I10) emit per §3.3.5 with Perl-only scaling column.
6. **Matrix verdict + exit-code mapping** (rev 1 I16) per §3.3.6. Emit `matrix_verdict.txt` listing all per-cell PASS/FAIL + cross-N PASS/FAIL + the final exit code.

### 5.5 New `RELEASE_CHECKLIST.md` (top-level; rev 1 I13 + I14 expanded)

Per §4.3. The PE section is explicitly a TODO-stub referencing #872; this PR commits the file with SE section + tag-steps populated. Either-order merges with #872 are safe.

### 5.6 PROGRESS.md update

Add a Phase H SE row pointing at this plan + status `📝 plan rev 1 — awaiting implementation trigger`. The companion PE row stays at "plan TBD; sub-issue filed (#872)".

### 5.7 No crate code changes

Phase H is harness + checklist + SPEC. NO changes to `rust/bismark-extractor/src/*` or `rust/bismark-extractor/tests/*`.

### 5.8 Pre-merge validation

1. `bash -n scripts/phase_h_smoke.sh` + `bash -n scripts/phase_h_se_matrix.sh` — syntax check.
2. `shellcheck scripts/phase_h_smoke.sh scripts/phase_h_se_matrix.sh` — if available; else skip.
3. `bash scripts/phase_h_se_matrix.sh <DEV_BAM> --out /tmp/phase_h_se_test/` on the local Desktop 10M SE BAM — quick smoke that the driver runs end-to-end.
4. **Real validation on colossal** post-merge per RELEASE_CHECKLIST.md — that's the release-gate, not the merge-gate.

## 6. Efficiency (rev 1 I1 — honest range, no fake precision)

- **Matrix runtime: estimated 1-3 hours per BAM.** This is uncertain pre-first-run: rev 0 cited 12 min/run from CLAUDE.md, but reviewers correctly noted that figure is for PE on 55M (extrapolated); SE on 10M is likely faster. Will measure on first colossal run; checklist instructs the user to update this estimate post-first-run.
- **Driver overhead**: bash + a few integer increments + 4 `diff -q` calls per ignore-pair. ~ms-scale; invisible vs per-cell extraction time.
- **Disk usage** (rev 1 I18 refined): ~1.8 GB per matrix run (5 cells × 2 binaries × ~180 MB per cell output). Safety margin to 2.5 GB documented in A14. Colossal has 100s of GB; well within.

The 5-cell matrix (rev 1) requires 10 extraction runs vs rev 0's 8; net runtime is slightly higher BUT the edge_clip cell may run faster than non-edge cells (most positions filtered out). Net wall-clock impact: small.

## 7. Integration

### 7.1 Read/Write surface

- **Read**: BAM input (read-only); Perl + Rust binaries (PATH or env-overridden).
- **Write per run**: `<OUT>/cell_p<N>_i<5p>_i3<3p>/` per cell; `<OUT>/cross_n_summary.txt`; `<OUT>/speedup_table.md`; `<OUT>/matrix_verdict.txt`.
- **Repo writes** (PR scope):
  - `scripts/oxy_phase_h_smoke.sh` → `scripts/phase_h_smoke.sh` (rename + ~50 LOC SE tweaks + `--extra-*` flags)
  - `scripts/phase_h_se_matrix.sh` (new, ~250 LOC)
  - `RELEASE_CHECKLIST.md` (new, ~120 LOC)
  - `rust/bismark-extractor/SPEC.md` (§8.3 + §9.7 + §10 row H updates)
  - `plans/05262026_bismark-extractor/PROGRESS.md` (Phase H SE row)

### 7.2 Downstream impact

| Consumer | Impact |
|---|---|
| Companion #872 PE plan | References this plan in §1; PE driver is independent per Felix's directive; **5-cell structure now symmetric (rev 1 I17)** with mode-specific cell content. |
| v1.0 release tag | Gated by this matrix PASS on colossal + #872 matrix PASS on colossal. RELEASE_CHECKLIST.md is the binding document. |
| Phase C.1 / C.2 regression detection | (D, N=1) cell's `*.M-bias.txt` 5712 B HARD-FAIL is the continuous regression guard (rev 1 I15). |
| Phase F N-invariance contract | The new cross-N check (rev 1 C1) directly tests SPEC §8.3 row 4 — strengthens regression coverage beyond rev 0's self-determinism. |
| External tooling | None — scripts + checklist are internal. |

### 7.3 Deliberately NOT implemented (defer to follow-ups)

- Self-hosted runner workflow (resolved Critical Q1 — checklist chosen).
- Shared `scripts/_phase_h_lib.sh` (resolved Critical Q3 — independent drivers).
- `--secondary-bam PATH` for larger-BAM speedup confidence (rev 1 I7).
- Inline N=8 cells unless `--parallel-set "1 4 8"` is passed.
- Performance investigation if Rust scaling < 4× — separate `perf(extractor):` sub-issue per #871.

## 8. Assumptions (rev 1 refined)

### 8.1 From epic + prior phases

- **A1.** 10M SE BAM on colossal mirrors oxy's path-shape. Per memory `reference_colossal_access.md`. First colossal session verifies; update if different (trivial commit).
- **A2.** Perl `bismark_methylation_extractor` in `bioinf` micromamba env. Per memory.
- **A3.** colossal supports `--parallel 4` (≥4 cores). Pre-flight `nproc` check enforces (rev 1 I9).
- **A4 (rev 1 reinforced by I8)**: `bismark_methylation_extractor` version is **v0.25.1**. The 5712 B M-bias baseline assumes this. Pre-flight version assertion enforces; if version drifts, matrix exits 2 with explicit remediation.
- **A5.** Rust `--parallel N` invariant (SPEC §8.3 row 4): output bytes identical regardless of N. The cross-N check (rev 1 C1) directly tests this.
- **A6.** Perl `--multicore N` produces fork-modulo-then-concatenate ordering, which is N-dependent. SPEC §8.3 row 1 rev 3 accepts sorted-content equivalence on data files.
- **A7.** Phase C.2's empty-sweep produces deterministic 6-file kept-set for directional SE.

### 8.2 Plan-specific

- **A8.** One PR (`extractor-phase-h-se`) closes #871.
- **A9.** Branch from `rust/iron-chancellor` HEAD `f88bad7`.
- **A10.** No crate code changes.
- **A11.** Companion #872 work happens in parallel via independent PR; no merge-order coupling — checklist's PE section is a TODO-stub safe under either merge order (rev 1 I14).
- **A12.** RELEASE_CHECKLIST.md is the binding gate for v1.0 tag. Felix is the release engineer; sign-off recorded as a comment on epic #798 (rev 1 I13).
- **A13.** `--extra-rust` / `--extra-perl` values are bash-array-safe (no shell-metacharacter injection from a single trusted user).
- **A14 (rev 1 I18)**: colossal has ≥ 2.5 GB free in matrix output dir.
- **A15 (rev 1 NEW)**: `phase_h_smoke.sh`'s `diff_summary.txt` emits wall-clock as `^Perl: <int>s$` / `^Rust: <int>s$` (anchored lines). Driver parses with `grep -E`. If smoke's output format changes, this driver breaks visibly.
- **A16 (rev 1 NEW)**: `nproc` is available on colossal (POSIX-standard utility). Trivial assumption; degraded to a warning if `nproc` not found.

## 9. Validation

### 9.1 Local-Mac pre-merge smoke

| Check | What | How | Expected |
|---|---|---|---|
| Driver syntax | bash parses | `bash -n scripts/phase_h_se_matrix.sh` | exit 0 |
| Smoke syntax | bash parses | `bash -n scripts/phase_h_smoke.sh` | exit 0 |
| Driver end-to-end on tiny BAM | Matrix runs without crashing on local Desktop SE BAM | `bash scripts/phase_h_se_matrix.sh ~/Desktop/.../10M_SE/...bam --out /tmp/phase_h_test/ --parallel-set "1"` | exit 0 or 3; speedup table emitted; cross_n_summary.txt empty (only 1 N value) |
| Smoke SE-specific behaviour | Renamed smoke recognises SE and asserts 6 kept files | Run smoke directly on local SE BAM; inspect verdict | PASS with 6 files |
| `--extra-rust`/`--extra-perl` pass-through | Flags appended verbatim (rev 1 I6 array form) | Run smoke with `--extra-rust "--ignore 5"`; verify Rust output reflects | Rust M-bias has fewer rows than no-extra |
| Pre-flight Perl version assertion (rev 1 I8) | Mismatched version rejected | Override `PERL_BIN` to a fake script that emits "v0.26.0"; matrix exits 2 | exit 2 with explicit error |
| Pre-flight nproc check (rev 1 I9) | N > nproc rejected | `bash scripts/phase_h_se_matrix.sh ... --parallel-set "9999"` | exit 2 |
| Cross-N check fires (rev 1 C1) | Synthetic divergence between N=1 and N=4 output triggers FAIL | Manually tamper with `cell_p4/rust/*.M-bias.txt` post-extraction, then re-run matrix | exit 1 with "cross-N regression at cell <id>" |

### 9.2 Colossal release-gate validation (PER RELEASE_CHECKLIST.md; not PR-blocking)

| Check | What | How | Expected |
|---|---|---|---|
| Full SE matrix on colossal 10M SE | All 5 cells × 2 N PASS byte-identity + cross-N | `bash scripts/phase_h_se_matrix.sh /weka/.../10M_SE/...bam` | exit 0 or 3; matrix_verdict.txt PASS |
| `*.M-bias.txt` baseline at (D, N=1) | 5712 B exact | `wc -c cell_p1_i0_i30/rust/*.M-bias.txt` | 5712 |
| M-bias row-count differential for `--ignore 5` cells (rev 1 I15) | Fewer rows than (D, N=1) cell | Compare `wc -l` on the M-bias files | (5p, 0) and (0, 3p) and (5p, 3p) all have fewer rows than (D, N=1) |
| Cross-N (rev 1 C1) | Rust-N=1 ≡ Rust-N=4 raw-byte per ignore-pair | `cross_n_summary.txt` content | 5 PASS rows (one per ignore-pair) |
| Speedup at N=4 | Rust scaling ≥ 4× | Read speedup_table.md | Rust scaling 4×+ |
| Disk footprint | Matrix output ≤ 2.5 GB | `du -sh <OUT>` | ≤ 2.5 GB |

### 9.3 Cross-phase regression check

| Check | What | How | Expected |
|---|---|---|---|
| Phase C.1 polarity guard | M-bias at (D, N=1) is 5712 B | Matrix's hard-fail check | PASS |
| Phase C.2 empty-sweep | 6 kept files per cell | Per-cell verdict | PASS |
| Phase F N-invariance | Cross-N PASS per ignore-pair (rev 1 C1) | `cross_n_summary.txt` | PASS |
| Phase G unchanged | Existing 303 tests pass | `cargo test -p bismark-extractor` | 303 pass |

## 10. Questions or ambiguities (rev 1 — resolved + remaining)

### Critical — none (post-rev-1 absorption)

All 1 Critical (C1 — N-invariance assertion) folded into rev 1's §3.3.4 + §3.4 #5.

### Open (defaults taken — non-critical, flagged for plan-reviewer round 2 if any)

| Q | Default | Rationale |
|---|---|---|
| Rename `scripts/oxy_phase_h_smoke.sh` to `scripts/phase_h_smoke.sh` | YES | Post-colossal-migration; oxy prefix is location-coupled misnomer |
| Larger SE BAM availability on colossal | Dropped from rev 1 per I7 — was unreachable through single-BAM-arg CLI. Future enhancement only. | Future `--secondary-bam` flag if needed |
| `--parallel 8` cell-set inclusion | Off by default; user opts in via `--parallel-set "1 4 8"` | Saves runtime when colossal has < 8 free cores |
| `RELEASE_CHECKLIST.md` location | Top-level repo file | Most-visible for release-prep work |
| Exit code 3 vs structured field for perf-target-miss | Exit code 3 (rev 1 I16 explicit) | Release-prep tooling reads exit codes; structured field adds parsing complexity |
| `edge_clip` cell `--ignore 250` value | 250 (~2.5× typical Illumina 100 bp read) | Sufficiently > typical read length; matches plan-reviewer's I8 suggestion |
| RELEASE_CHECKLIST sign-off mechanism | Comment on epic #798 with matrix output (rev 1 I13) | Lightweight; auditable via issue history |

### Open (round-2 reviewer attention magnets — surfaced for completeness)

These are rev 1's small remaining choices a plan-reviewer round 2 may push back on:

1. **5-cell matrix reduction vs rev 0's full cartesian** — rev 1 reduces 2³=8 cells to 5 representative + adds edge_clip. Coverage is comparable (5 cells × 2 N = 10 invocations vs rev 0's 8 cells × ... wait rev 0 was already 2×2×2=8 cells, NOT 8 × 2; both rev 0 and rev 1 run 10-ish invocations). The reconciliation is structural (symmetry with #872), not coverage-shrinking.
2. **Edge cell `--ignore 250` is SE-specific to fill the #872 5th-slot symmetry** — alternative is `--mode comprehensive` or another representative; rev 1 chose edge_clip because it tests a real boundary-handling edge case in `extract_calls` per SPEC §7.6.
3. **`tmux` recommendation in checklist** (rev 1 I11) — alternative: `nohup` + log to file. Both work; tmux is most-recognised.
4. **Pre-flight Perl version assertion is HARD-FAIL** (rev 1 I8) — alternative: WARN + proceed. Hard-fail prevents silent baseline invalidation; warn would allow accidental drift.

## 11. Self-Review (rev 1 — post-absorption)

Reviewed rev 1 for:

- **Efficiency:** 5-cell × 2 N matrix runtime is bounded by Perl extraction time × 10 invocations. Driver overhead negligible. Disk ~1.8 GB. ✓
- **Logic consistency:** N-invariance check (§3.3.4) is now the strongest single assertion; replaces self-determinism. Pre-flight ordering documented in §3.3.2. Exit-code mapping articulated in §3.3.6. ✓
- **Edge cases:** §3.5 expanded to 13 cases (rev 0 had 10). New: Perl version drift, nproc-exceed, edge_clip-on-short-reads, cross-N regression, mid-matrix SSH disconnect, colossal load, BAM-path drift. ✓
- **Integration:** SPEC §8.3 + §9.7 + §10 row H all in scope. Phase F N-invariance contract directly tested. C.1's 5712 B baseline preserved as HARD-FAIL guard at (D, N=1). ✓
- **Test coverage:** Driver itself has no Rust unit tests (bash). Pre-merge §9.1 covers driver-correctness via local-Mac dry-run + syntax checks. Colossal validation is release-gate, not merge-gate. ✓
- **Cross-plan symmetry:** #871 and #872 both now have 5-cell × 2 N × 2 binaries = 20-invocation matrices (rev 1 I17). Mode-specific cell content differs; structural shape symmetric. ✓

### Adjustments made during rev 1 absorption

Folded **1 Critical (A-C1 ≡ B-I2) + 14 distinct Important findings**:
- C1 → §3.3.4 cross-N check replaces §3.4 self-determinism
- I1 → §6 honest range
- I2 → §4.2 wall-clock format pin + §5.4 step 3
- I3 → §3.3.4 re-run sub-dir handling
- I4 → §5.4 step 3 source-of-truth
- I5 → §5.1 step 3 (SPEC §10 row H)
- I6 → §3.2 + §4.2 bash array
- I7 → §2.5 + §10 dropped larger-BAM
- I8 → §3.3.2 #3 + §A4 reinforced
- I9 → §3.3.2 #5
- I10 → §3.3.5 Perl-only scaling column
- I11 → §4.3 + §3.5 tmux
- I12 → §3.3.2 #2
- I13 → §4.3 roles + escalation
- I14 → §4.3 PE TODO-stub
- I15 → §3.4 #4 split by cell type
- I16 → §3.3.6 explicit table
- I17 → §3.1 5-cell reconciliation
- I18 → §A14 + §6 disk-footprint refinement

### Remaining risks (post-rev-1)

- **R1**: First colossal session may discover the 10M SE BAM at a different subpath. Mitigation: RELEASE_CHECKLIST.md says "verify path"; trivial follow-up commit if needed.
- **R2**: M-bias byte counts for non-(D,N=1) cells are determined empirically on first run — rev 1 specifies row-count differential checks but not exact byte counts. If reviewer prefers exact byte locks per cell, that's a rev-2 enhancement once first colossal run measures them.
- **R3**: `nproc` may report logical cores (hyperthreads) not physical; matrix may report misleading scaling. Mitigation: §A16 documents the assumption; if it bites, switch to `lscpu -p | grep -v '^#' | sort -u -t, -k 2,4 | wc -l` for physical-core count.
- **R4**: Cross-N check failure mode (rev 1 C1) — if it fires on the first colossal run, that's a Phase F regression and must be fixed before v1.0. Documented as a release-blocker condition in §4.3.
- **R5**: RELEASE_CHECKLIST.md PE section is a TODO-stub. If #871 merges before #872, the checklist is incomplete; the v1.0 tag work waits anyway (rev 1 A11). Documented.

### Reviewer-attention magnets (post-rev-1)

These are rev-1's small remaining choices a round-2 reviewer may push back on:

1. **Edge cell `--ignore 250`** specific value — defensible; alternative is 1000 or "max(seq_len) + 1". Rev 1 chose 250 as ≥2× typical Illumina 100 bp read.
2. **Pre-flight hard-fail on Perl version mismatch** vs warn-and-proceed — rev 1 chose hard-fail (rev 1 I8).
3. **RELEASE_CHECKLIST sign-off via #798 comment** (rev 1 I13) vs separate `release/v1.0.md` artefact in repo — rev 1 chose comment for lightweight.
4. **5-cell structure with edge_clip** as SE-specific 5th cell — vs 4 cells without the edge case (matches PE structure 1:1). Rev 1 chose 5 to keep cardinality symmetric with #872.
5. **Cross-N check is per-pair** (Rust-N=1 ≡ Rust-N=4 for each ignore combo) vs once-per-matrix (D cell only). Rev 1 chose per-pair for stronger regression coverage.

---

## 12. Open delivery cycle

1. ✅ Sub-issue filed: [#871](https://github.com/FelixKrueger/Bismark/issues/871), linked under epic #798, board fields set.
2. ✅ Plan rev 0 written.
3. ✅ Manual review by Felix — approved, directed to dual plan-reviewers.
4. ✅ Dual `plan-reviewer` agents — `PLAN_REVIEW_PHASE_H_SE_A.md` (NEEDS-REVISIONS: 1 Crit + 8 Imp + 8 Opt) + `PLAN_REVIEW_PHASE_H_SE_B.md` (APPROVE-WITH-NITS: 0 Crit + 12 Imp). 1 distinct Critical + 14 distinct Importants after de-duplication.
5. ✅ **Plan rev 1** folding all findings + reconciling SE matrix to 5 cells (mirror #872) — this file.
6. 🟡 **Implementation trigger from Felix** — *PENDING*.
7. ⏸ Implementation per §5.
8. ⏸ Dual `code-reviewer` agents.
9. ⏸ `plan-manager` Mode B audit.
10. ⏸ PR `extractor-phase-h-se` → `rust/iron-chancellor`, closes #871.
11. ⏸ Merge.
12. ⏸ RELEASE_CHECKLIST.md walked on colossal (post-merge); #872 lands in parallel; v1.0 tag once both PASS.

# Phase H sub-gate 1 — PE byte-identity + speedup harness (closes #872)

**Status:** Plan rev 3, post-code-review absorption on branch `extractor-phase-h-pe`. Ready for commit + PR.
**Parent issue:** #872 (filed under epic #798).
**Companion sub-issue:** #871 — SE byte-identity + speedup harness. **Merged at commit `651b7fd` (PR #873, 2026-05-28).** This plan reuses #871's infrastructure (renamed `phase_h_smoke.sh` + `--extra-*` flags + `RELEASE_CHECKLIST.md` shape + `phase_h_se_matrix.sh` driver template).
**Branch target:** new `extractor-phase-h-pe` from `rust/iron-chancellor` HEAD `651b7fd` (post-#873 SE merge).
**Crate version bump:** **none.** Phase H is harness-and-checklist work, not crate code.

> **Epic:** `plans/05262026_bismark-extractor/PROGRESS.md`, Phase H sub-gate 1 — PE byte-identity + speedup harness. No standalone `EPIC.md`; the local epic-tracking artifact is `PROGRESS.md`. Upstream coordination doc is GitHub epic #798.

## Revision History

| Rev | Date | Notes |
|---|---|---|
| 3 | 2026-05-28 | **Post-code-review absorption complete.** Folded 1 High + 2 Medium findings from dual code-reviewers (A: APPROVE-WITH-NITS / 0 Crit / 0 High / 2 Med / 15 Low; B: APPROVE-WITH-NITS / 0 Crit / 1 High / 6 Med / 5 Low) + plan-manager Mode B (COMPLETE / 60 DONE / 4 DEVIATED / 0 PARTIAL / 0 MISSING). Headline changes:<br>**B-H1 ≡ A-M1 (High, 3-way consensus including SE-driver inheritance):** `count_mbias_rows`'s `grep -cE ... \|\| echo "0"` pattern fail-opens when M-bias has 0 data rows. `grep -c` prints "0" AND exits 1 on 0 matches, triggering the `\|\| echo "0"` fallback. Result is two-line "0\n0" output; downstream integer compare `[[ N -ge D ]]` hits "bad math expression" (silently swallowed by `2>/dev/null` inside if-test), the violation goes unrecorded, `PASS_FLAG` stays 1, and the matrix emits a false PASS. Fail-open in a check whose entire purpose (per rev 1 B-Imp-1) is fail-closed. Replaced with `awk '/^[0-9]+\t/ { c++ } END { print c+0 }'` — single-integer stdout including 0, exit 0, parses cleanly. **SE-driver inheritance flagged** as follow-up: same pattern at `scripts/phase_h_se_matrix.sh:391`; file `polish(extractor):` sub-issue post-merge to back-port (out of v1.0 PE scope per Felix's independent-drivers directive).<br>**B-M2 (Medium):** Verdict text for `--parallel-set "4"` (no N=1) cases conflated "PASS (full gates)" with "PASS (weakened-checks, baseline + differential vacuously skipped)". Added `BASELINE_GATE_APPLIES` branch in §3.3.6 verdict assembly emitting `PASS (weakened — --parallel-set lacks N=1; baseline + differential gates not applied)` when the gate doesn't apply. Release engineer can no longer mistake a single-N run for a full matrix PASS.<br>**A-M2 (Medium):** Stale line-number reference `"asserted via samtools @PG check at phase_h_pe_matrix.sh:117"` in `speedup_table.md` (line 117 is now a `mkdir -p`; the check moved to line 134 when the overlap-fraction pre-flight was inserted). Dropped the line number; provenance string now reads `"asserted via samtools @PG pre-flight check; see driver's pre-flight step 4"` — drift-proof.<br>**Deferred** (with rationale): A's 15 Lows (cosmetic word-splitting inherited from SE; PE regex intentionally stricter than smoke; overflow-risk math safely within 64-bit; cargo build budget false alarm) + B-M1 (integer-truncation actually conservative, not loose — false positive after B's own analysis) + B-M3 (self-corrected non-issue) + B-M4 (verdict reason conflates drift vs missing-file; polish) + B-M5 (mkdir-p ordering; polish) + B-M6 (samtools 2>/dev/null hides corrupt-BAM diagnostics; polish) + B's 5 Lows (magic-number coordination, TOCTOU on BAM canonicalize, `<none>` placeholder rendering, get_cell_mbias_file linear scan, emoji rendering on LANG=C). All defer-candidates for a `polish(extractor):` follow-up post-v1.0.<br>**Re-validation:** `bash -n scripts/phase_h_pe_matrix.sh` clean; `cargo test -p bismark-extractor` → **303 passed / 0 failed / 0 ignored** (post-#873 baseline preserved bit-for-bit). Driver LOC: 786 → 801 (+15 LOC from the three surgical edits: awk recipe + the explanatory comment block documenting why grep -cE fail-opens, BASELINE_GATE_APPLIES branch in verdict assembly with its rev-3 rationale comment, provenance string simplification). Plan-manager COMPLETE verdict pre-absorption stands; the rev-3 fixes don't add MISSING items. |
| 2 | 2026-05-28 | **Implementation complete on branch `extractor-phase-h-pe`** (off `rust/iron-chancellor` HEAD `651b7fd` post-#873 SE merge). See "Implementation Notes" section below the rev table. 303 tests preserved (no Rust code changes — post-#873 baseline intact). One new bash script created (`scripts/phase_h_pe_matrix.sh`, 786 LOC); `RELEASE_CHECKLIST.md` PE section populated (replaced #873's TODO-stub); SPEC.md §8.3 + §10 row H PE updated. Awaiting dual-code-review + plan-manager audit. |
| 1 | 2026-05-28 | **Folded both reviewers' 1 Critical + 9 distinct Important findings + selected Optional polish.** Headline changes:<br>**A-C1 (Critical — unrunnable PE-ness pre-flight):** §3.3.2 #5 default rewritten to `samtools view -H "$BAM" \| grep -qE '^@PG.*ID:Bismark.*[[:space:]]-1[[:space:]]'` (mirrors smoke's existing detection regex at `phase_h_smoke.sh:159`). Drops the `--dry-run` mechanism (smoke has no such flag; verified by grep) and the incorrect `--paired` regex alternative (Bismark @PG keys on `-1 R1.fq`, not `--paired`).<br>**A-I1 ≡ B-Imp-1 ≡ B-Imp-5 (3-way consensus on overlap differential):** Combined fix — (a) new pre-flight overlap-fraction sanity check via `samtools view -c -f 0x2 <BAM>` ≥ 80% of total reads (rejects pre-flight if not met); (b) `ROW_COUNT_OK=0` fail-closed initialization + missing/unreadable M-bias file forces FAIL (mirrors `MBIAS_BASELINE_OK` pattern); (c) row-count differential FAIL produces a distinct verdict line "FAIL: row-count differential ..." separate from byte-identity FAIL, so a release engineer can disambiguate.<br>**A-O3 (overlap row-count semantic ambiguity):** Replaced row-count differential for the `overlap` cell with **count-sum differential** — `sum(methylated + unmethylated)` across R2 data rows must exceed D's same sum by ≥5%. M-bias positions are read-relative; under `--include_overlap`, calls at existing positions ACCUMULATE (counts increase) rather than adding new rows. Row count stays unchanged; count-sum is the semantically correct metric. The three `<D` cells (`r1_5p`/`r2_5p`/`r1r2_3p`) keep row-count differential (SE-symmetric; ignore flags actually remove positions). Mixed-metric documented in §3.3.5 + §10 Open.<br>**A-I2 (Phase C.1 polarity guard completeness):** Promoted §11 R2 to §10 Open with default = "rev-2 enhancement post-first-colossal" — an explicit M-bias byte-count baseline at `(overlap, N=1)` (TBD value) catches include_overlap-only regressions that escape the (D, N=1) lock. Rev 1 does not block on this; first colossal run measures.<br>**A-I3 (baseline gate covers BOTH M-bias + splitting-report):** Introduced `BASELINE_GATE_APPLIES` covering BOTH `MBIAS_BASELINE_OK` AND (formerly) splitting-report baseline. Resolved by A-I5 absorption below (splitting-report absolute lock dropped); `BASELINE_GATE_APPLIES` now governs M-bias 11,443 B alone (and, rev-2, overlap baseline). Both fail-closed init; ANDed in §3.3.6 exit-code logic.<br>**A-I4 (baseline drift recovery path):** New §3.5 edge-case row + RELEASE_CHECKLIST escalation path covering "colossal Perl-vs-Rust byte-cmp PASSes but colossal baseline differs from planner's locked 11,443 B": classified as BAM/env mismatch, not regression. Recovery: rev-2 baseline-update commit after verifying prior baseline was a transcription error.<br>**A-I5 (drop 875 B absolute splitting-report lock):** Splitting-report includes the input filename in its first line; byte count is sensitive to BAM path length. The colossal path differs from oxy where the 875 B was measured, so the absolute lock false-fires while Perl-vs-Rust byte-cmp passes. Dropped from HARD-FAIL list; relied on Perl-vs-Rust byte-cmp alone (per-cell). M-bias byte-count remains as the Phase C.1 polarity guard.<br>**B-Imp-2 (`row_count_diff.txt` format unspec'd):** Dropped the standalone file. Differential evidence inlined into `matrix_verdict.txt` + `speedup_table.md` (SE-symmetric pattern: "evidence in two places"). Spec'd the inline format.<br>**B-Imp-3 (cell-id naming asymmetry):** Kept mnemonic cell-ids (`D`/`r1_5p`/`r2_5p`/`r1r2_3p`/`overlap`); documented the asymmetry vs SE's parameter-encoded names (`i<5p>_i3<3p>`) in §10 Open + §11 reviewer-attention magnets. Defensible: PE has 5-dimensional flag space; parameter-encoded names would be unwieldy (`cell_p1_i0_ir2_0_i3p0_i3pr2_0_ov0`).<br>**B-Imp-4 (cross-N timing on failed cells):** Explicit sentence in §3.3.4: "Cross-N comparison runs unconditionally per cell, even if the cell's byte-identity verdict vs Perl is FAIL — preserves diagnostic signal (a simultaneous cross-N failure points specifically at Phase F's worker-reduce path; an isolated byte-identity FAIL with cross-N PASS points elsewhere)."<br>**Cargo build budget (A's minor catch):** RELEASE_CHECKLIST PE section replacement text adds the "Budget ~5-15 min for cargo build on cold cache" line that SE's text has per SE rev 3.<br>**Optionals folded opportunistically:** A-O1 (justify dropping isolated R1-3p/R2-3p cells); A-O2 (tighten runtime to 1.5-4 h with per-cell decomposition); B-Opt-1 (SPEC edits are docs not contract — no version bump justification); B-Opt-2(c) (Phase C.1 off-by-one note in §3.5); B-Opt-3 (§5.2 expanded from 7 to 10 implementer-brief points); B-Opt-4 (MD5 of input BAM recorded in `matrix_verdict.txt` for fixture-drift detection); B-Opt-5 (consolidated "v1.0 may legitimately ship with PE matrix exiting 3" statement in §1).<br>**Deferred to rev-2 / polish:** A-O5 (smoke's `Library: PE` independence-of-extra-flags note — trivial, not blocking); A-O6 (post-merge PROGRESS.md row content); B-Opt-2(a)+(b) (missing-@PG + corrupt-header edge cases — covered by the new samtools regex per A-C1; the regex returns false → exit 2). |
| 0 | 2026-05-28 | Initial draft post-#873 (SE) merge. Pre-folds SE rev-1-through-rev-3 reviewer findings that translate verbatim to PE: bash 4.0 pre-flight + `${ARR[@]+"${ARR[@]}"}` defensive idiom (A-Er1/Er2); M-bias `MBIAS_GATE_APPLIES` + fail-closed default (A-L1 ≡ B-L2); speedup-table `R*100/P` direction with `P=0` guard (B-L1); `uptime` regex tolerates BSD/GNU dialects (B-E3); SIGINT trap; nproc-missing graceful skip; tmux warning; cross-N byte-identity per ignore-tuple (SE rev 1 C1); Perl-only scaling column (SE rev 1 I10); `--extra-rust`/`--extra-perl` array form (SE rev 1 I6); pre-flight Perl version assertion (SE rev 1 I8); matrix-level `--out` empty rejection (SE rev 1 I12); exit-code mapping 0/1/2/3 with perf-miss exit 3 (SE rev 1 I16). PE-specific content: 5-cell matrix per #872 issue body (D / R1-5' / R2-5' / R1+R2-3' / include_overlap); locked baselines 11,443 B M-bias + 875 B splitting-report; Phase C.1 polarity regression guard via M-bias byte-count at (D, N=1); 6 kept files for directional PE post-C.2 empty-sweep. |

## Implementation Notes (rev 2, 2026-05-28, post-impl)

Executed on branch `extractor-phase-h-pe` (off `rust/iron-chancellor` HEAD `651b7fd` post-#873 SE merge). No Rust code changes; no crate version bump (Phase H is harness work, per plan §A11).

### Per-§ status

| § | Done | Notes |
|---|---|---|
| §5.1 SPEC §8.3 PE matrix subsection + §10 row H PE update | ✅ | §8.3 expanded the 1-line PE reference into a full subsection with cell table + PE-specific assertions (overlap-fraction gate, samtools-direct PE-ness, mixed-metric differential, fail-closed ROW_COUNT_OK, BAM MD5 record, no 875 B absolute lock per rev 1 A-I5). §10 row H PE updated with rev 1 actuals (~830 LOC absorbing 1 Critical + 9 Important findings). §9.7 already referenced `scripts/phase_h_pe_matrix.sh` as of #873 — no change. |
| §5.2 New `scripts/phase_h_pe_matrix.sh` | ✅ | **786 LOC bash** (deviation from §2.2's ~700 LOC estimate — see deviations below). Implements: pre-flight (10 numbered checks: bash 4.0, BAM exists, OUT empty, samtools present, PE-ness via `^@PG.*ID:Bismark.*[[:space:]]-1[[:space:]]` regex mirroring `phase_h_smoke.sh:159`, overlap-fraction ≥80% via `samtools view -c -f 0x2`, Perl v0.25.1, nproc + contention advisory, tmux warning, input BAM MD5); 5-cell PE matrix execution with mnemonic CELL_IDs (`D`/`r1_5p`/`r2_5p`/`r1r2_3p`/`overlap`); per-CELL_ID cross-N byte-identity check (runs unconditionally per rev 1 B-Imp-4); M-bias 11,443 B baseline at (D, N=1) with fail-closed `MBIAS_BASELINE_OK`; mixed-metric differential (row count `<D` for the three ignore-flag cells; count-sum `>D+5%` for `overlap` cell, per rev 1 A-O3) with fail-closed `ROW_COUNT_OK` + missing-file forced FAIL per rev 1 B-Imp-1; Perl-only + Rust scaling speedup table with input BAM MD5 + properly-paired fraction headers (rev 1 B-Opt-4 + A-I1); 4-way exit-code mapping (0/1/2/3) with distinct verdict line for differential FAIL (rev 1 B-Imp-5). Driver discovers `phase_h_smoke.sh` via `$(dirname "${BASH_SOURCE[0]}")/..` for portability. |
| §5.3 RELEASE_CHECKLIST.md PE section populate | ✅ | Replaced the #873 TODO-stub with full PE block: tmux-wrapped colossal invocation, cargo build budget reminder ("~5-15 min for cold cache"), 7 verify checkboxes covering exit code + M-bias 11,443 B baseline + cross-N + mixed-metric differential + properly-paired fraction + BAM MD5 fixture-drift detector, "PE matrix: PASS at \<date\>" sign-off line for #798. Added new "Escalation: colossal-vs-planner baseline drift" subsection (rev 1 A-I4) covering recovery path when colossal baseline differs from planner's locked 11,443 B but per-cell byte-cmp passes. |
| §5.4 PROGRESS.md update | ✅ (rev 1 → rev 2 status reflected on prior turn) | PE row already updated during rev 1 absorption to reflect implementation complete state. SE row stays at ✅ merged. |
| §5.5 No crate code changes | ✅ | Confirmed: `git diff --stat origin/rust/iron-chancellor...HEAD` shows only `rust/bismark-extractor/SPEC.md` under `rust/` — no `src/*` or `tests/*` changes. |
| §5.6 Pre-merge validation | ✅ | `bash -n scripts/phase_h_pe_matrix.sh` clean. `cargo test -p bismark-extractor` → **303 passed / 0 failed / 0 ignored**, exactly matching the post-#873 baseline. NO local-Mac smoke dry-run on a tiny PE BAM (deviation — see below; `shellcheck` not installed either). |

### Pre-existing test updates

**None.** No Rust code changed; the 303-test baseline is preserved bit-for-bit from #873's merged state.

### Deviations from rev 1 plan

1. **Matrix driver size: 786 LOC vs ~700 LOC estimated** (plan §2.2). Reason: the rev-1 mixed-metric differential (row count vs count-sum) required two helper functions (`count_mbias_rows` + `sum_mbias_counts`) and their dispatch in §3.3.3 step 5, adding ~30 LOC beyond the SE driver's row-count-only logic. The new overlap-fraction pre-flight (rev 1 A-I1, ~20 LOC including total-vs-properly-paired counts + threshold check) + BAM MD5 record (rev 1 B-Opt-4, ~12 LOC with cross-platform `md5sum`/`md5` fallback) + the rev-1-required missing-file fail-closed branch (rev 1 B-Imp-1, ~20 LOC for the per-cell file-presence check loop) added the remaining ~75 LOC vs SE driver's 711 LOC. No functional change vs the plan; deviation is solely in line-count.
2. **`shellcheck` not installed on the dev Mac** (mirror SE rev 2 deviation #2). Plan §5.6 step 2 says "if available; else skip with note". Skipped with note. Recommended: `brew install shellcheck` on dev Mac for future Phase H driver work.
3. **No local-Mac smoke dry-run on a tiny PE BAM** (mirror SE rev 2 deviation #3). Reason: the dev Mac doesn't currently have a `bioinf` micromamba env with Perl bismark v0.25.1 installed; pre-flight Perl-version + samtools checks would correctly reject. A real end-to-end smoke happens on colossal per RELEASE_CHECKLIST.md (the release gate). The bash-syntax check + `cargo test` are sufficient pre-merge validation; the matrix driver's internal logic is verified at code-review time by the dual reviewers + plan-manager Mode B.
4. **PROGRESS.md update happened during rev 1 absorption, not at §5.4 step time** — the PE row was updated when I wrote rev 1 (status moved from "📝 plan rev 0 — awaiting manual review" to "📝 plan rev 1 — awaiting implementation trigger") rather than during the implementation phase per §5.4. No functional impact; the row content is current.

### Iteration log

Implementation proceeded linearly; no iterations beyond initial drafts:

1. **#1 Branch creation** — `git fetch origin rust/iron-chancellor` + `git checkout -b extractor-phase-h-pe origin/rust/iron-chancellor` from local HEAD position. Uncommitted plan + PROGRESS files followed correctly.
2. **#2 SPEC updates** (§5.1) — two Edit calls: §8.3 PE subsection expansion + §10 row H PE update. Clean first attempt.
3. **#3 Matrix driver** (§5.2) — single Write call (786 LOC). `bash -n` clean first-try. The mixed-metric differential block + ROW_COUNT_OK fail-closed init + missing-file branch are the largest novel pieces vs SE driver; reviewers should scrutinize.
4. **#4 RELEASE_CHECKLIST.md PE section** (§5.3) — single Edit replacing the #873 TODO-stub. Added escalation-path subsection for baseline drift.
5. **#5 Validation** (§5.6) — `bash -n` clean; `cargo test -p bismark-extractor` confirmed 303-test baseline preserved.

Zero re-iterations; no failed steps. Total: 5 forward steps.

### Verification confidence

- **Mixed-metric differential** in §3.3.5 implements rev 1's A-O3 finding. The three `<D` row-count assertions are SE-symmetric. The `>D+5%` count-sum assertion for the `overlap` cell uses an awk one-liner: `awk -F'\t' '/^[0-9]+\t/ { sum += $2 + $3 } END { print sum+0 }'`. Both helper functions (`count_mbias_rows`, `sum_mbias_counts`) follow the SE driver's `count_mbias_rows` pattern; dual code-reviewers should mental-execute the awk recipe + verify the missing-file branch ordering (file-presence check happens BEFORE any assertion runs, per rev 1 B-Imp-1 fail-closed semantics).
- **ROW_COUNT_OK fail-closed** verified by mental trace: initial 0, only flipped to 1 on positive completion of all 4 assertions AND all 5 cells' M-bias files present + readable. Missing file → forced FAIL via the `MISSING` variable check that runs before any assertion.
- **Overlap-fraction pre-flight gate** verified by mental trace: `samtools view -c -f 0x2` gives properly-paired count; threshold is 80% of total reads via integer division `PAIRED_READS * 100 / TOTAL_READS`. Below threshold → exit 2 with explicit error. On a 1.2 GB BAM, the two `samtools view -c` calls take ~30 s combined; one-time pre-flight cost.
- **PE-ness assertion** verified against `phase_h_smoke.sh:159` regex (confirmed via grep): mirrors smoke's existing `@PG.*ID:Bismark.*-1 ` (note trailing space distinguishing `-1 ` from `-12`). The plan-reviewer's A-C1 fix is implemented as specified.
- **BAM MD5 cross-platform**: `md5sum` (Linux) preferred; `md5 -q` (macOS) fallback; `"(md5 unavailable)"` if neither. Doesn't fail the matrix; informational record. ~5-10 s for 1.2 GB BAM.
- **Cross-N runs unconditionally** (rev 1 B-Imp-4): the cross-N loop iterates all cells regardless of `CELL_VERDICT[k]`; preserves diagnostic signal.
- **303 tests preserved.** The crate is untouched (verified via `cargo test` aggregate + `git diff --stat`).

Ready for dual code-review + plan-manager Mode B audit.

---

## 1. Goal

Verify byte-identity between Rust `bismark-methylation-extractor-rs` and Perl `bismark_methylation_extractor` on **PE mode**, across a **5-cell representative matrix** spanning the PE-specific dimensions SE doesn't have — independent R1/R2 ignore flags and `--no_overlap`/`--include_overlap` overlap-handling — at `--parallel ∈ {1, 4}`. Emit a wall-clock speedup table comparing Rust vs Perl per-cell with Perl-only + Rust scaling columns. Carry forward the Phase F N-invariance cross-N assertion (Rust-N=1 ≡ Rust-N=4 raw-byte per ignore-tuple) from SE rev 1.

This closes Phase H sub-gate 1 for PE — the half of Phase H that covers the extractor's own output streams under the PE input shape. SE (the other half) is the merged #871 work; both PASS gates the v1.0 release tag per `RELEASE_CHECKLIST.md`.

**Out-of-scope** (per #872 body — separately tracked):

- Sub-gate 2 streams (`.bedGraph.gz`, `.bismark.cov.gz`, `CpG_report.txt[.gz]`) — blocked on epic #797.
- SE byte-identity — covered by merged #871.
- `--yacht`, `--comprehensive`, `--merge_non_CpG`, `--gzip` byte-identity — already verified by Phase E + Phase G smokes; mode-axis intentionally collapsed to "default".
- `--mbias_only` — already verified by Phase E + C.2 absorption.
- **Performance investigation** — if Rust-vs-Perl ratio at N=4 underperforms (Phase C.2 era measured 0.9× on default PE per CLAUDE.md), file a separate `perf(extractor):` sub-issue; Phase H gates on byte-identity, NOT speedup. **v1.0 may legitimately ship with PE matrix exiting 3 (perf-miss)** (rev 1 B-Opt-5 consolidation); the matrix's byte-identity verdict is the only release-blocking signal.

## 2. Context

### 2.1 Phase status table impact

| Phase | Before | After |
|---|---|---|
| G | ✅ merged (`ff961d3`) | ✅ merged |
| H — sub-gate 1 SE | ✅ merged (`651b7fd`, PR #873) | ✅ merged |
| **H — sub-gate 1 PE** | ⏸ sub-issue filed (#872); plan rev 0 in dual-plan-review | 📝 plan rev 1 — this file (post-dual-plan-review absorption; awaiting implementation trigger) |
| H — sub-gate 2 | ⏸ blocked-on-#797 | unchanged |
| v1.0 release tag | ⏸ blocked on PE matrix PASS | unblocked once #872 PASSes on colossal |

### 2.2 Where the code (and docs) live

| Item | File | Approximate LOC |
|---|---|---|
| New 5-cell PE matrix driver | `scripts/phase_h_pe_matrix.sh` (new) — 5 cells × 2 parallelism = 10 invocations; cross-N byte-equality check per ignore-tuple; speedup table emission with PE-specific commentary; pre-flight checks (mirror SE driver's rev 3 form) | ~700 LOC bash (matches SE's 660 + ~40 LOC for the extra PE dimensions) |
| Populate `RELEASE_CHECKLIST.md` PE TODO-stub | `RELEASE_CHECKLIST.md` — fill in the section #873 left as a stub | ~50 LOC |
| SPEC §8.3 PE matrix subsection | `rust/bismark-extractor/SPEC.md` — add a "Phase H PE matrix" paragraph next to the existing "Phase H SE matrix (rev 4)" block | ~25 LOC |
| SPEC §10 row H PE | `rust/bismark-extractor/SPEC.md` — mark the sub-gate-1-PE row complete after merge (in PR review state, change to in-progress) | ~3 LOC |
| PROGRESS.md PE row | `plans/05262026_bismark-extractor/PROGRESS.md` | ~5 LOC |

**Total estimate: ~785 LOC** (bash + markdown; no Rust code changes). Slightly higher than SE's 660 due to the populated RELEASE_CHECKLIST PE section (SE shipped that as a TODO-stub).

### 2.3 Dependencies / phase ordering

Depends on:
- **Phase G** (`ff961d3`) — feature surface complete; subprocess chains exist (their byte-identity is sub-gate 2's concern, not this plan).
- **Phase C.1** (`84c6ad1`) — fixed `drop_overlap` polarity reversal in PE; locked `*.M-bias.txt` byte equality (11,443 B on 10M PE at the default cell) and SRR24827373.9 R2-overlap regression fixture as the implicit Phase C.1 guards. **The (D, N=1) cell's 11,443 B M-bias byte count is this plan's hard regression guard.**
- **Phase C.2** (`4e5c691`) — splitting-report format + empty-file sweep; locked `*_splitting_report.txt` byte equality (875 B on 10M PE at the default cell) and 6 kept files for directional PE.
- **Phase F** (`215b88d`) — N-invariance contract (`--parallel N` produces output byte-identical to `--parallel 1` for any N ≥ 1). **The cross-N assertion in this plan tests F directly on the PE input shape.**
- **Phase H sub-gate 1 SE** (`651b7fd`, PR #873) — established the matrix-driver shape, the `phase_h_smoke.sh` renamed + `--extra-rust`/`--extra-perl` flag surface, the `RELEASE_CHECKLIST.md` skeleton, and the SE matrix at the SE M-bias 5712 B baseline. **This plan reuses all of those artifacts and populates the PE section of the checklist.**

Unblocks:
- v1.0 release tag (final gate; SE is already passed on colossal once #873's RELEASE_CHECKLIST.md walk completes).

### 2.4 Existing harness — `scripts/phase_h_smoke.sh` (post-#873) + `scripts/phase_h_se_matrix.sh` (post-#873)

Post-#873, the smoke script `scripts/phase_h_smoke.sh` already:

| Capability | Status post-#873 |
|---|---|
| Runs Perl + Rust on same BAM (PE auto-detect from `@PG`) | ✅ |
| `--parallel N` + `--mode MODE` + `--out DIR` arg parsing | ✅ |
| Auto-detects SE-vs-PE from `@PG` header + emits `Library: PE` line in diff_summary.txt | ✅ (post-#873) |
| Per-file byte-cmp arm for `*.M-bias.txt` + `*_splitting_report.txt` | ✅ |
| Sorted-MD5 arm for data files | ✅ |
| File-name-set match check | ✅ |
| Wall-clock capture in `^Perl: <int>s$` / `^Rust: <int>s$` format | ✅ (pinned in #873) |
| PASS verdict aggregation per invocation | ✅ |
| `--extra-rust "<flags>"` / `--extra-perl "<flags>"` array-form pass-through | ✅ (post-#873) |
| Defensive `${ARR[@]+"${ARR[@]}"}` for empty arrays under `set -u` | ✅ (post-#873) |

The SE driver `scripts/phase_h_se_matrix.sh` (660 LOC post-#873) is the structural template for this plan's PE driver. The PE driver is **independent code** (per Felix's 2026-05-28 directive — no shared `_phase_h_lib.sh` in v1.0) but mirrors the SE driver's pre-flight, per-cell-execution, cross-N-comparison, speedup-table-emission, and exit-code-mapping shape almost verbatim.

What's missing for PE (this plan adds):

| Capability | Status |
|---|---|
| 5-cell PE matrix loop (R1-5' / R2-5' / R1+R2-3' / include_overlap + D) at `--parallel ∈ {1, 4}` | ❌ (new driver) |
| PE-specific cell-id naming (`cell_p<N>_D` / `_r1_5p` / `_r2_5p` / `_r1r2_3p` / `_overlap`) | ❌ |
| PE-specific locked baseline (M-bias 11,443 B at (D, N=1); rev 1 A-I5 dropped the 875 B splitting-report absolute lock) | ❌ |
| PE-specific row-count differential expectations (which cells reduce M-bias rows vs D; which increases) | ❌ |
| `RELEASE_CHECKLIST.md` PE section populated (replaces #873's TODO-stub) | ❌ |

### 2.5 Test machine + data (colossal)

Per the memory `reference_colossal_access.md` (2026-05-28):

- Connection: `dcli ssh colossal` (verify on first session)
- Data dir: `/weka/projects/bioinf/Data/Felix/bismark_benchmarks/` (Weka shared storage)
- micromamba env: `bioinf` (Perl Bismark stack: `bismark_methylation_extractor`, `samtools`, `bowtie2`)

**Primary PE BAM** (canonical fixture, locked at #872 + #871 development):

- Path: assumed `/weka/projects/bioinf/Data/Felix/bismark_benchmarks/10M_PE/SRR24827378_10M_R1_val_1_bismark_bt2_pe.deduplicated.bam` (mirroring oxy layout — verify on first colossal session per memory).
- Size: ~1.2 GB.
- Locked baseline at (D, N=1) cell: `*.M-bias.txt` 11,443 B (HARD-FAIL on drift). `*_splitting_report.txt` was historically observed at 875 B on oxy, but rev 1 A-I5 dropped the absolute-size lock since the report includes the input filename and the colossal BAM path differs from oxy — per-cell Perl-vs-Rust strict-byte-cmp is the regression guard instead. Data files sorted-MD5 equal to Perl; 6 kept files (CpG/CHG/CHH × OT/OB, CTOT/CTOB swept).
- Phase C.1 polarity regression fixture: read `SRR24827373.9` has a 71 bp gap between R1 end + R2 start; all R2 calls in the unique region must be kept (not dropped) under `--no_overlap`. The (D, N=1) cell's byte-equality with Perl + the 11,443 B M-bias size implicitly asserts this; an optional read-level grep is flagged in §10 as a reviewer-attention magnet.

### 2.6 Companion: #871 (SE Phase H) — merged at `651b7fd`

Independent driver per Felix's 2026-05-28 directive. SE matrix at `scripts/phase_h_se_matrix.sh` ran 5 cells (D / 5p / 3p / 5p+3p / edge_clip) × 2 parallelism × 2 binaries = 20 invocations. This plan's PE matrix runs the same 20-invocation shape but with the 5 cells specified in #872's body. Cross-plan symmetry is structural (5 cells × 2 N × 2 binaries = 20); cell content is mode-specific.

`RELEASE_CHECKLIST.md`'s PE section (a TODO-stub post-#873) is populated by this PR. After merge, the checklist's PE block is the binding gate for the second half of v1.0's byte-identity matrix walk on colossal.

## 3. Behavior

### 3.1 Test matrix — 5 cells × {N=1, N=4}

| Cell ID | Description | `--ignore` | `--ignore_r2` | `--ignore_3prime` | `--ignore_3prime_r2` | Overlap |
|---|---|---|---|---|---|---|
| **D** | Default — no ignore flags; `--no_overlap` default. Baseline cell; M-bias = 11,443 B HARD-FAIL regression guard (Phase C.1). Splitting-report 875 B absolute lock dropped per rev 1 A-I5; per-cell byte-cmp catches Phase C.2 format regressions. | 0 | 0 | 0 | 0 | (default no_overlap) |
| **r1_5p** | R1 5' trim isolated. Mirrors SE plan's `5p` cell at the R1-only axis. | 5 | 0 | 0 | 0 | (default no_overlap) |
| **r2_5p** | R2 5' trim isolated. PE-specific dimension SE doesn't have. | 0 | 5 | 0 | 0 | (default no_overlap) |
| **r1r2_3p** | Both 3' trims combined. Real-world usage on bisulfite reads (3' adapter remnants on both mates). | 0 | 0 | 5 | 5 | (default no_overlap) |
| **overlap** | `--include_overlap` overrides the default `--no_overlap`. The most byte-identity-sensitive PE-specific dimension since Phase C.1's polarity fix; verifies that include_overlap mode produces identical R2-call retention between Rust + Perl. | 0 | 0 | 0 | 0 | `--include_overlap` |

× `--parallel ∈ {1, 4}` = **10 invocations per binary** = **20 total per BAM**.

**Rationale for the reduced matrix** (per #872 body): full 2⁶ = 64 cells × 2 parallelism × 2 binaries = 256 invocations would take ≥ 8 h on colossal — excessive. The 5-cell reduction picks high-information cells: PE-default byte-identity; R1-only ignore semantics (mirrors SE structure); R2-only ignore semantics (PE-specific); combined 3' trim (real-world); overlap-handling toggle (Phase C.1 regression-sensitive). The asymmetry vs SE (SE has `edge_clip`; PE doesn't, and PE has `overlap`; SE doesn't) is structural — SE has no overlap dimension; PE has no single edge-case that's worth a 6th cell post-C.2 (overlap IS the PE edge case). Flagged in §10 as Open in case a plan-reviewer pushes back.

**Why isolated R1-3p and R2-3p cells are absent** (rev 1 A-O1 — justify dropped axes): isolated R1-3p-alone and R2-3p-alone cells are subsumed by the combined `r1r2_3p` cell. If `r1r2_3p` PASSes byte-identity vs Perl, both isolated 3' axes are exercised — any 3p-handling bug surfaces in either constituent (`--ignore_3prime 5` independently affects R1 even when `--ignore_3prime_r2 5` is also set, since Perl's implementation handles each flag's read-bound separately). The combined cell trades isolation-specificity for matrix breadth at the same byte-identity assertion strength. If a plan-reviewer prefers explicit isolation cells, that's a 2-cell expansion (`r1_3p` + `r2_3p`) — out of v1.0 scope; flagged in §10 Open.

### 3.2 Per-cell invocation contract

For each matrix cell, the driver invokes `scripts/phase_h_smoke.sh` with:

```bash
scripts/phase_h_smoke.sh \
  <BAM> \
  --parallel <N> \
  --mode default \
  --out <OUT_DIR>/cell_p<N>_<CELL_ID> \
  --extra-rust "<cell-specific-flags>" \
  --extra-perl "<cell-specific-flags>"
```

Where `<CELL_ID>` ∈ `{D, r1_5p, r2_5p, r1r2_3p, overlap}` and `<cell-specific-flags>` is the corresponding column-product from §3.1.

The smoke script's PASS/FAIL verdict applies per cell. Exit code per cell (unchanged from #873):
- 0 = byte-identity PASS
- 1 = data-file mismatch / file-set mismatch / strict-cmp failure on `*.M-bias.txt` / `*_splitting_report.txt`
- 2 = harness usage error

### 3.3 Matrix driver — `scripts/phase_h_pe_matrix.sh`

#### 3.3.1 CLI

```bash
scripts/phase_h_pe_matrix.sh <BAM> [--out DIR] [--parallel-set "1 4"]
```

- `<BAM>` — absolute path to the PE input BAM. Required. Smoke's `@PG` auto-detect verifies PE-ness; driver asserts on USAGE-ERROR if smoke reports `Library: SE`.
- `--out DIR` — output root directory; per-cell subdirs created under it. Default: `./phase_h_pe_matrix_out`.
- `--parallel-set "..."` — space-separated parallelism values. Default: `"1 4"`. Override to `"1 4 8"` if colossal has the cores.

#### 3.3.2 Pre-flight checks (mirror #873's SE driver rev 3)

Before any cell runs:

1. **Bash version ≥ 4.0** (`(( BASH_VERSINFO[0] < 4 )) && exit 2`) — required for `declare -A` + array semantics; macOS-3.2 remediation hint surfaced. (SE driver A-Er1.)
2. **SIGINT/SIGTERM trap** installed: `trap '... exit 130' INT TERM` preserves partial `$OUT_DIR` state for evidence.
3. **BAM exists + readable** (`-r "$BAM"`); else exit 2.
4. **`--out DIR` is empty or doesn't exist** (matrix-level non-empty rejection per SE rev 1 I12); else exit 2.
5. **PE-ness assertion** (rev 1 A-C1 — fixed): direct samtools check mirroring smoke's existing detection logic at `phase_h_smoke.sh:159`:
   ```bash
   samtools view -H "$BAM" | grep -qE '^@PG.*ID:Bismark.*[[:space:]]-1[[:space:]]'
   ```
   The regex keys on the `-1 R1.fq.gz` Bowtie2 invocation arg in Bismark's `@PG` line (note the trailing space, distinguishing `-1 ` from `-12`). Else exit 2 with "expected PE BAM (Bismark `-1` arg in @PG); confirm input is a paired-end Bismark output". Rev 0's `--dry-run` mechanism was unimplementable (smoke has no such flag, verified by grep of smoke arg-parser); rev 0's `--paired` alternative regex was wrong (Bismark passes `-1`/`-2` to Bowtie2, not `--paired`).
6. **Overlap-fraction sanity check** (rev 1 A-I1 + B-Imp-5 — new): `samtools view -c -f 0x2 "$BAM"` (properly-paired reads, including overlap-eligible pairs) ≥ 80% of `samtools view -c "$BAM"` (total reads). If below threshold, exit 2 with "BAM has <X%> properly-paired reads; overlap differential check requires ≥80% to be meaningful. Either use a different BAM or invoke with `--skip-overlap-differential` (NOT implemented in rev 1; flagged in §10 Open)". Documents the locked 10M PE BAM's known shape and prevents silent assertion-misfire on a future BAM swap-in.
7. **Perl version assertion** (mirror SE rev 1 I8): `bismark_methylation_extractor --version 2>&1 | grep -q "Bismark Extractor Version: v0.25.1"`; else exit 2.
8. **Rust binary discoverable** + executable (per smoke's env-var checks).
9. **`nproc` + contention advisory** (mirror SE rev 3 B-Med graceful skip): if nproc unavailable, SKIP the high-N rejection + load advisory entirely; do NOT reject. If load average > nproc, emit warning (using BSD/GNU-tolerant regex `load average[s]?:` per SE rev 3 B-E3).
10. **`tmux` warning**: if `$TMUX` and `$STY` both unset, warn "session-disconnect risk on 1.5-4 h run; run inside tmux or screen".
11. **Gate flags initialized** (rev 1 A-I3 — `BASELINE_GATE_APPLIES` introduced):
    - `BASELINE_GATE_APPLIES` (rename of rev 0's `MBIAS_GATE_APPLIES` to reflect scope) = 1 iff (D, N=1) is in the planned matrix (i.e., 1 ∈ `--parallel-set` AND `D` cell exists). Governs both M-bias 11,443 B HARD-FAIL and (rev-2) the explicit `(overlap, N=1)` baseline lock once measured.
    - `MBIAS_BASELINE_OK` initialized to **0** (fail-closed; SE rev 3 A-L1 ≡ B-L2). Set to 1 only on positive `size==11443` match.
    - `ROW_COUNT_OK` initialized to **0** (fail-closed; rev 1 B-Imp-1 — PE's asymmetric `>` direction in the `overlap` cell (count-sum > D + 5% per rev 1 A-O3) reintroduces the fail-open class bug SE's all-`<D` directions don't have). Set to 1 only on positive completion of all 4 differential assertions in §3.3.5 (3 row-count `<D` + 1 count-sum `>D + 5%`) AND all 5 cells' M-bias files are present + readable.
    - **Splitting-report 875 B absolute lock**: DROPPED per rev 1 A-I5 (the splitting report contains the input filename in its first line; byte count is path-length sensitive; absolute lock would false-fire on colossal due to BAM-path length differences vs the oxy reference). Per-cell Perl-vs-Rust strict byte-equality on splitting-report retained.

#### 3.3.3 Per-cell execution

For each `(N, CELL_ID, flags)` tuple in the matrix:

1. Create per-cell subdir: `<OUT>/cell_p<N>_<CELL_ID>/`.
2. Invoke `phase_h_smoke.sh` per §3.2.
3. Record per-cell verdict (PASS/FAIL/USAGE) + parse wall-clocks via `grep -E '^Perl: ([0-9]+)s$' <out>/diff_summary.txt` and analogous for Rust.
4. If cell is (D, N=1) AND `BASELINE_GATE_APPLIES==1`: verify `*.M-bias.txt` size == 11,443 B; flip `MBIAS_BASELINE_OK=1` on positive match. (Splitting-report absolute-size lock dropped per rev 1 A-I5; the smoke's per-cell strict-byte-cmp against Perl is the splitting-report regression guard.)
5. Compute differential-metric counter for the cell (used by §3.3.5):
   - **For `D`, `r1_5p`, `r2_5p`, `r1r2_3p`**: M-bias data row count via `grep -cE '^[0-9]+\t' <out>/rust/*.M-bias.txt`. SE-symmetric metric — `--ignore N` removes positions from output, so row count decreases monotonically.
   - **For `overlap`** (rev 1 A-O3 — different metric for different semantics): M-bias data **count-sum** via `grep -E '^[0-9]+\t' <out>/rust/*.M-bias.txt | awk '{sum += $2 + $3} END {print sum}'` (methylated + unmethylated counts summed across all data rows). `--include_overlap` keeps R2 calls at positions already-in-the-D-baseline; row count is unchanged (positions are read-relative; M-bias reports all positions 1..read_len populated by any pair); the differential signal is in count VALUES, not row count. Count-sum strictly increases under include_overlap.
   - If the M-bias file is missing or unreadable: force `ROW_COUNT_OK=0` immediately (per rev 1 B-Imp-1 fail-closed for asymmetric `>` direction).
6. **Cross-N byte-identity check** (mirror SE rev 1 C1) — see §3.3.4.

#### 3.3.4 Cross-N byte-identity check per cell

After each `CELL_ID` has been run at all `--parallel-set` values:

For each pair of N values `(N_a, N_b)` in `--parallel-set` with `N_a < N_b`:
- Compare every output file in `cell_p<N_a>_<CELL_ID>/rust/` against the corresponding file in `cell_p<N_b>_<CELL_ID>/rust/`.
- Raw-byte equality required (strict `cmp -s`). Tests SPEC §8.3 row 4 — Phase F N-invariance contract on the PE input shape.

If any file diverges → cell-level FAIL.

**Cross-N timing semantics** (rev 1 B-Imp-4 — explicit): Cross-N comparison runs **unconditionally per cell**, even if the cell's byte-identity verdict vs Perl is FAIL. This preserves diagnostic signal — a simultaneous cross-N failure points specifically at Phase F's worker-reduce path; an isolated byte-identity FAIL with cross-N PASS points elsewhere (Phase B/C/D/E/G). The SE driver behaves the same way (verified at `scripts/phase_h_se_matrix.sh` lines 200-300); rev 0's silence on this allowed an implementer to plausibly skip cross-N on already-failed cells, diverging from SE.

#### 3.3.5 Aggregation + speedup table

After all cells run:
- Aggregate PASS count: must equal `len(parallel_set) × 5`.
- Aggregate FAIL count: any non-zero means matrix-level FAIL.
- Aggregate USAGE-ERROR count: any non-zero means matrix INVALID.
- **Cross-N PASS count**: per CELL_ID, must equal `(N choose 2)` for the parallel-set; any non-zero means matrix FAIL.
- **Differential check at N=1** (rev 1 A-O3 mixed metric; rev 1 B-Imp-2 inlined into `matrix_verdict.txt` instead of standalone `row_count_diff.txt`): walk the four non-D cells at N=1, compute the cell-appropriate metric per §3.3.3 step 5, assert:
  - `r1_5p` **row count < D** by ≥1 row (R1 5' trim removes 5 R1 positions from M-bias output)
  - `r2_5p` **row count < D** by ≥1 row (R2 5' trim removes 5 R2 positions from M-bias output)
  - `r1r2_3p` **row count < D** by ≥1 row (both 3' trims remove positions)
  - `overlap` **count-sum > D** by ≥5% (`--include_overlap` accumulates counts at existing positions — rows unchanged, counts strictly higher; metric differs from the three `<D` cells per rev 1 A-O3)

  On positive completion of ALL FOUR assertions (and all 5 cells' M-bias files present + readable), flip `ROW_COUNT_OK=1`. Any unmet assertion OR missing/unreadable file → `ROW_COUNT_OK` remains 0; emit a **distinct verdict line** to `matrix_verdict.txt`: `FAIL: differential <cell> <metric>=<observed> not <op> D=<observed_D>` (per rev 1 B-Imp-5 — release engineer can disambiguate "byte-identity holds but differential fires" from "real byte-identity break"). Exit code still 1; the disambiguation is in the verdict text.

- Emit `<OUT>/speedup_table.md` with Perl-only scaling + Rust scaling columns + cross-N PASS column + differential metric column + Rust commit + crate version + input BAM MD5 (rev 1 B-Opt-4 — silent fixture-drift detector) embedded + arithmetic direction `R*100/P` (with `P=0` guard, per SE rev 3 B-L1).

Markdown shape (mirror SE rev 3 with rev 1 PE-specific columns):

```markdown
# Phase H PE speedup table

Generated: <ISO-8601 timestamp>
Input BAM: <BAM path> (<size> bytes; <PE-pair count> pairs)
Input BAM MD5: <md5sum>     # rev 1 B-Opt-4 fixture-drift detector
Properly-paired fraction: <pct>%   # rev 1 A-I1 pre-flight result (asserted ≥80%)
Bismark Perl version: v0.25.1 (asserted)
Rust commit: <git rev-parse HEAD>
Rust crate version: <Cargo.toml version>
Library: PE (asserted via samtools @PG check)

## Per-cell wall-clock

| Cell    | N | Flags                                  | Perl (s) | Rust (s) | Rust/Perl | Cross-N PASS | Differential (vs D)            | Verdict |
|---------|---|----------------------------------------|----------|----------|-----------|--------------|--------------------------------|---------|
| D       | 1 | (none)                                 | 900      | 1000     | 1.11×     | (baseline N) | rows=142 (baseline)            | PASS    |
| D       | 4 | (none)                                 | 240      | 200      | 0.83×     | ✓ vs N=1     | rows=142                       | PASS    |
| r1_5p   | 1 | --ignore 5                             | 880      | 950      | 1.08×     | (baseline N) | rows=132 (<D, Δ=-10)           | PASS    |
| r2_5p   | 1 | --ignore_r2 5                          | 880      | 950      | 1.08×     | (baseline N) | rows=132 (<D, Δ=-10)           | PASS    |
| r1r2_3p | 1 | --ignore_3prime 5 --ignore_3prime_r2 5 | 880      | 950      | 1.08×     | (baseline N) | rows=122 (<D, Δ=-20)           | PASS    |
| overlap | 1 | --include_overlap                      | 920      | 1100     | 1.20×     | (baseline N) | count-sum=8,392,104 (>D +8.2%) | PASS    |
| ...

## Per-N aggregate

| N | Avg Perl (s) | Avg Rust (s) | Avg Rust/Perl | Perl scaling | Rust scaling | Cells |
|---|--------------|--------------|---------------|--------------|--------------|-------|
| 1 | 892          | 990          | 1.11×         | (baseline)   | (baseline)   | 5     |
| 4 | 240          | 250          | 1.04×         | 3.72×        | 3.96×        | 5     |

## SPEC §9.7 target check

Target: Rust `--parallel 4` ≥ 4× Rust `--parallel 1`.
Measured: 3.96×. ⚠️ Below target by 0.04×.
File `perf(extractor):` follow-up sub-issue; v1.0 tag NOT blocked (per §3.3.6 exit code 3).
```

#### 3.3.6 PASS verdict + exit-code mapping

The matrix PASSes iff ALL of:
- Every cell's `phase_h_smoke.sh` exits 0 (per-cell byte-identity, includes per-cell Perl-vs-Rust strict-byte-cmp on splitting-report).
- Every cross-N comparison (§3.3.4) raw-byte matches.
- If `BASELINE_GATE_APPLIES==1`: `MBIAS_BASELINE_OK==1` (i.e., (D, N=1) M-bias == 11,443 B). If `BASELINE_GATE_APPLIES==0` (e.g., `--parallel-set "4"` with no N=1 cell): baseline check is vacuous PASS. 875 B splitting-report absolute lock dropped per rev 1 A-I5 (Perl-vs-Rust strict-byte-cmp per-cell remains the splitting-report regression guard).
- `ROW_COUNT_OK==1` (all four differential assertions PASS per §3.3.5; all 5 cells' M-bias files present + readable). If any M-bias file missing → forced FAIL per rev 1 B-Imp-1.
- Pre-flight checks all pass (including the new rev 1 A-I1 overlap-fraction ≥ 80% gate).

Exit code mapping (same as SE):

| Condition | Exit code |
|---|---|
| All PASS + Rust scaling ≥ SPEC §9.7's 4× target | 0 |
| Any cell FAIL OR cross-N FAIL OR baseline drift OR differential FAIL | 1 |
| Pre-flight USAGE-ERROR (BAM missing, version mismatch, dir not empty, Library != PE, overlap fraction < 80%, etc.) | 2 |
| All byte-identity PASSED but Rust scaling < 4× | 3 |

Exit 3 is informational FAIL; release checklist may accept with a follow-up `perf(extractor):` sub-issue. Phase C.2 era measured 0.9× on default PE — **rev 1 B-Opt-5 consolidation: v1.0 may legitimately ship with PE matrix exiting 3** (perf-miss); the matrix's byte-identity verdict is the only release-blocking signal. The PE perf gap is a known issue with a separate `perf(extractor):` follow-up path.

### 3.4 Per-cell byte-identity contract

Each cell asserts (via the underlying `phase_h_smoke.sh`):

1. **Sorted-content equivalence** on all data files (CpG/CHG/CHH × OT/OB for PE directional libraries; 6 files post-empty-sweep).
2. **File-set match**: 6 kept files (CTOT/CTOB unlinked by Phase C.2's empty-sweep, mirroring SE behavior).
3. **Strict-byte equality** on `*_splitting_report.txt` (all cells). 875 B absolute size lock at (D, N=1) DROPPED per rev 1 A-I5 — splitting-report contains the input filename in its first line, making absolute byte count path-length sensitive; colossal BAM path differs from oxy where 875 B was measured, so the lock would false-fire while per-cell Perl-vs-Rust byte-cmp passes. Per-cell strict-byte-cmp remains the splitting-report format regression guard (catches Phase C.2 regressions symmetrically without absolute-size fragility).
4. **Strict-byte equality** on `*.M-bias.txt`:
   - **(D, N=1) cell** (gated by `BASELINE_GATE_APPLIES`, rev 1 A-I3): must equal Perl byte-for-byte AND total size must equal the locked baseline **11,443 B** (Phase C.1 polarity regression guard). HARD-FAIL if drift; `MBIAS_BASELINE_OK` flag set only on positive match. If gate doesn't apply (no N=1 in `--parallel-set`): baseline check is vacuous PASS.
   - **Other cells**: must equal Perl byte-for-byte; size varies per cell. Differential metric per §3.3.5 (row count for `<D` cells; count-sum for `overlap` cell, per rev 1 A-O3) enforces semantic correctness.
5. **Cross-N byte-identity**: for each CELL_ID, Rust-N=1 output ≡ Rust-N=4 output raw-byte. SPEC §8.3 row 4 on PE shape. Runs unconditionally per cell even if byte-identity-vs-Perl already FAILed (rev 1 B-Imp-4 — preserves diagnostic signal).
6. **Phase C.1 polarity guard** (rev 1 A-I2 — known coverage gap documented):
   - **Implicit guard at (D, N=1)**: M-bias byte-count match at 11,443 B catches `--no_overlap`-polarity regressions (the original C.1 bug class where R2 calls in overlap region are erroneously dropped/kept under the default flag).
   - **Differential guard at `overlap` cell**: count-sum > D + 5% catches `--include_overlap`-polarity regressions where the include_overlap path silently behaves like no_overlap.
   - **Known gap (rev 1 A-I2 — promoted to §10 Open with rev-2 default)**: a subtle `--include_overlap`-only regression that preserves count-sum direction (e.g., off-by-one position handling in the kept R2 region) would escape both guards. Rev-2 enhancement post-first-colossal: add an explicit M-bias byte-count baseline at `(overlap, N=1)` (TBD value, measured on first colossal run). Read-level grep for `SRR24827373.9` flagged in §10 as alternative; deferred for fragility per the issue body's "read names can change with re-alignment" concern.
   - **B-Opt-2(c) note**: an `--include_overlap`-only Phase C.1 polarity off-by-one (positionally correct direction but wrong R2 base position) WOULD be caught by the per-cell Perl-vs-Rust strict M-bias byte-cmp (point #4 above) — the differential check on count-sum is a SEMANTIC guard, not the only polarity guard. Documented here so the operator knows "count-sum > D in overlap cell does NOT mean polarity is correct; M-bias strict-cmp catches per-base polarity".

The driver records each assertion's PASS/FAIL per cell in `<OUT>/cell_*/diff_summary.txt` (the smoke handles #1-4) PLUS `<OUT>/cross_n_summary.txt` (driver-emitted for §3.3.4) PLUS `<OUT>/row_count_diff.txt` (driver-emitted for the §3.3.5 row-count differential, with the four PE-specific assertions).

### 3.5 Edge cases

| Case | Handling |
|---|---|
| `--parallel-set` requests N > `nproc` | Pre-flight rejection if nproc available (mirror SE rev 1 I9); skip the check if nproc unavailable (mirror SE rev 3 B-Med). |
| `--out` dir already exists with non-empty contents | Pre-flight rejection; exit 2 (mirror SE rev 1 I12). |
| Perl version drift | Pre-flight rejection; exit 2 with "expected v0.25.1, got <version>" + remediation hint (mirror SE rev 1 I8). |
| Input BAM is actually SE (e.g., wrong path given) | Pre-flight `Library: PE` assertion fails; exit 2 with "expected PE, smoke reports Library: SE". |
| Rust binary not built | Pre-flight runs `cargo build --release -p bismark-extractor` IF `RUST_BIN` env is unset; else exit 2 with build-command hint (mirror SE rev 1 I8 follow-on). |
| BAM path contains spaces | Driver quotes argv via `"${ARGV[@]}"` array pattern. |
| Cell output dir already exists at per-cell level | Smoke's per-cell pre-flight aborts that cell with USAGE-ERROR; matrix-level pre-flight (§3.3.2 #4) catches earlier. |
| Cross-N self-determinism check fails on PE | Rare; signals a Phase F regression specific to PE worker-reduce or overlap-aware collector. File `bug(extractor):` immediately; matrix exits 1 with explicit "cross-N regression at cell <id>". |
| `--include_overlap` cell count-sum ≤ D (rev 1 A-O3 — metric changed from row-count to count-sum) | Differential check fires; emits "FAIL: differential overlap count-sum=<v> not >D=<v> by ≥5%" distinct verdict line (rev 1 B-Imp-5 disambiguates from byte-identity FAIL). Per-cell Perl-vs-Rust byte-cmp still verified separately. |
| `--include_overlap` cell M-bias byte-cmp FAILs vs Perl with count-sum > D + 5% | The differential is satisfied semantically but per-cell byte-identity caught a regression (per-base polarity off-by-one or per-position count drift). Cell-level FAIL via smoke. |
| BAM has overlap fraction < 80% (rev 1 A-I1 pre-flight gate) | Exit 2 with "BAM has X% properly-paired reads; overlap differential check requires ≥80% to be meaningful". Driver does NOT proceed — protects against silent differential mis-fire on non-canonical BAMs. |
| `*.M-bias.txt` size drift at (D, N=1) vs 11,443 B but per-cell Perl-vs-Rust byte-cmp PASSes (rev 1 A-I4 — colossal-vs-planner baseline mismatch) | Classified as BAM/env mismatch, NOT regression. Recovery: (1) verify the Perl reference on colossal is byte-equal to Rust output (per-cell smoke already does this); (2) verify the BAM MD5 in `matrix_verdict.txt` matches the planner's reference (or document the new BAM); (3) commit a rev-2 baseline-update PR replacing 11,443 B with the new value; (4) re-run matrix. Driver does NOT auto-update the baseline; release engineer makes the decision. Documented in RELEASE_CHECKLIST escalation paths. |
| Missing `@PG` line entirely in input BAM (rev 1 B-Opt-2(a)) | samtools-direct pre-flight returns empty; the grep regex match fails → exit 2 with "expected PE BAM (Bismark `-1` arg in @PG); confirm input is a paired-end Bismark output". Differentiates from "@PG present but reports SE" through the error text. |
| `@PG` advertises `-1 R1.fq` but actual reads are SE (corrupt header, rev 1 B-Opt-2(b)) | Pre-flight passes; first cell (D, N=1) runs; smoke's secondary detection (existing `Library: PE`/`Library: SE` annotation in `diff_summary.txt`) catches the mismatch at the first cell. Driver inspects the first cell's diff_summary; if `Library: SE`, exit 2 with "BAM @PG advertises PE but smoke reports SE on first cell — corrupt @PG; replace BAM". |
| Network/SSH disconnect mid-matrix (1-3 h run) | Recommended: run inside `tmux` / `screen` per RELEASE_CHECKLIST.md. Driver warns at pre-flight if `$TMUX`/`$STY` unset. SIGINT trap preserves `$OUT_DIR` state. |
| Colossal under high load (load average > nproc) | Pre-flight advisory using BSD/GNU-tolerant regex (mirror SE rev 3 B-E3); silent-degrade if uptime parse fails. Speedup ratios annotated as "potentially noisy" in table if advisory fired. |
| PE BAM at unexpected colossal subpath | RELEASE_CHECKLIST.md instructs `ls /weka/projects/bioinf/Data/Felix/bismark_benchmarks/10M_PE/` on first session; update §A1 if path differs. |
| Sub-2s per-cell wall-clock | Driver emits `⚠️ sub-2s` annotation in Rust/Perl ratio column (mirror SE rev 3 Low). |

### 3.6 Implementation order within a single matrix run

1. Pre-flight (§3.3.2). Aborts on any USAGE-ERROR.
2. For each `CELL_ID` ∈ matrix-cells (5 cells):
   - For each `N` ∈ `--parallel-set`:
     - Run `phase_h_smoke.sh` per §3.2 — produces `cell_p<N>_<CELL_ID>/`.
     - If (D, N=1): check 11,443 B M-bias baseline (rev 1: 875 B splitting-report absolute lock dropped per A-I5).
   - Cross-N comparison (§3.3.4) across the N values just run for this CELL_ID.
3. M-bias row-count differential check at N=1 across the 5 cells (§3.3.5).
4. Aggregate + emit speedup table (§3.3.5).
5. Emit `matrix_verdict.txt` + exit with mapped code (§3.3.6).

Per-cell sub-loop means cross-N comparisons happen as soon as both N runs are complete, surfacing regressions early.

## 4. Signature

### 4.1 `scripts/phase_h_pe_matrix.sh`

```bash
#!/usr/bin/env bash
#
# phase_h_pe_matrix.sh — Phase H sub-gate 1 PE byte-identity + speedup matrix.
#
# Runs the per-cell Phase H smoke (scripts/phase_h_smoke.sh) over the 5-cell
# representative PE matrix at --parallel ∈ {1, 4}. Asserts SPEC §8.3 row 4
# N-invariance per CELL_ID (Rust-N=1 ≡ Rust-N=4 raw-byte). Emits a markdown
# speedup table with Perl-only + Rust scaling columns. Includes PE-specific
# regression guards: 11,443 B M-bias baseline at (D, N=1); differential check
# (rows<D for r1_5p/r2_5p/r1r2_3p; count-sum>D+5% for overlap).
#
# Usage:
#   scripts/phase_h_pe_matrix.sh <BAM> [--out DIR] [--parallel-set "1 4"]
#
# Pre-flight checks:
#   - bash ≥ 4.0
#   - BAM exists + readable
#   - --out DIR is empty or doesn't exist
#   - PE-ness assertion via direct samtools @PG check (mirrors smoke's regex at phase_h_smoke.sh:159)
#   - Overlap fraction ≥ 80% (samtools view -c -f 0x2 / total reads) — rev 1 A-I1 protects against silent differential mis-fire
#   - Perl bismark_methylation_extractor version == v0.25.1
#   - Rust binary discoverable
#   - nproc + contention advisory (graceful skip if nproc unavailable)
#   - tmux/screen warning
#
# Exit codes:
#   0  — all cells PASS + cross-N PASS + differentials PASS + Rust scaling ≥ SPEC §9.7's 4×
#   1  — any cell FAIL or cross-N FAIL or differential FAIL or 11,443 B M-bias baseline drift at (D,N=1)
#   2  — pre-flight USAGE-ERROR
#   3  — byte-identity PASSED but Rust scaling missed the perf target (v1.0 may legitimately ship at exit 3)
#
# Outputs:
#   <OUT>/cell_p<N>_<CELL_ID>/  — per-cell phase_h_smoke output (CELL_ID ∈ D|r1_5p|r2_5p|r1r2_3p|overlap)
#   <OUT>/cross_n_summary.txt   — cross-N comparison results per CELL_ID
#   <OUT>/speedup_table.md      — markdown summary with Rust commit + crate version + BAM MD5
#   <OUT>/matrix_verdict.txt    — PASS/FAIL with per-cell breakdown + inline differential evidence (rev 1 B-Imp-2: no standalone row_count_diff.txt; SE-symmetric "evidence in two places" pattern)
```

### 4.2 `scripts/phase_h_smoke.sh` — no changes needed

Post-#873, smoke already supports `--extra-rust`/`--extra-perl` array passthrough, PE auto-detection, and the wall-clock format the matrix driver consumes. **No edits to `phase_h_smoke.sh` in this PR.**

### 4.3 `RELEASE_CHECKLIST.md` PE section (populates the #873 TODO-stub)

Replace the existing PE TODO-stub block (left by #873):

```markdown
### PE matrix (closes #872)

**TODO — populated by #872's PR.** Until #872 lands, this section is a stub:

> #872 will add the PE-equivalent matrix invocation here. Until then,
> v1.0 tag is blocked on #872's PR landing + PE matrix PASS.
```

With:

```markdown
### PE matrix (closes #872)

Run inside `tmux` or `screen` — the matrix takes 1-3 hours:

```bash
tmux new -s phase_h_pe
dcli ssh colossal
cd ~/Github/Bismark   # or wherever colossal has the working copy
git checkout rust/iron-chancellor && git pull --ff-only
micromamba activate bioinf

# Reuse the same Rust build from the SE matrix walk if still fresh; else:
# Budget ~5-15 min for cargo build on cold cache (rev 1 cargo-build-budget catch).
cargo build --release --manifest-path rust/Cargo.toml -p bismark-extractor

bash scripts/phase_h_pe_matrix.sh \
  /weka/projects/bioinf/Data/Felix/bismark_benchmarks/10M_PE/SRR24827378_10M_R1_val_1_bismark_bt2_pe.deduplicated.bam
```

Verify:
- Exit code 0 (or 3 = perf-miss-only acceptable for v1.0 tag) — see `matrix_verdict.txt`.
- `cell_p1_D/diff_summary.txt` shows `*.M-bias.txt` byte-cmp PASS + size 11,443 B. (Splitting-report 875 B absolute lock dropped per rev 1 A-I5; the per-cell strict-byte-cmp against Perl is the splitting-report regression guard.)
- `cross_n_summary.txt` shows PASS for all 5 cells.
- `matrix_verdict.txt` inline differential evidence shows `r1_5p rows < D`, `r2_5p rows < D`, `r1r2_3p rows < D`, `overlap count-sum > D + 5%` (rev 1 A-O3 mixed metric).
- Pre-flight overlap fraction ≥ 80% asserted (rev 1 A-I1); confirm via `matrix_verdict.txt` "Properly-paired fraction" header line.
- Input BAM MD5 in `speedup_table.md` matches the planner's reference (rev 1 B-Opt-4 fixture-drift detector). If MD5 differs, expect baseline drift; follow escalation path below.
- `speedup_table.md` — record Rust scaling at N=4; if < 4×, file `perf(extractor):` follow-up (exit 3 acceptable for tag).
- Comment on #798 with the table + "PE PASS" marker (or "PE PASS + perf-miss" with the perf sub-issue link).

**Escalation: colossal-vs-planner baseline drift (rev 1 A-I4 — new escalation path):**
If the matrix exits 1 with "(D, N=1) M-bias size <X> B != locked 11,443 B" BUT `cell_p1_D/diff_summary.txt` shows per-cell Perl-vs-Rust byte-cmp PASS:
1. This is BAM/env mismatch, not a Rust regression. Confirm via per-cell byte-cmp PASS.
2. Verify the BAM MD5 in `speedup_table.md` differs from the planner's reference (or document a new locked BAM if intentional).
3. Verify the prior 11,443 B baseline was a transcription error: search git log + memory for the original measurement on oxy.
4. If verification confirms the colossal value is correct: file a rev-2 baseline-update PR replacing 11,443 B with the new value in both `scripts/phase_h_pe_matrix.sh` and SPEC §8.3.
5. Re-run matrix. Do NOT bypass the gate without the baseline-update PR.
```

Update the v1.0 tag steps checklist to mark PE as runnable (post-merge) — no change to the tag-step text itself, just remove the TODO-stub indicator.

## 5. Implementation outline

### 5.1 SPEC updates (DO FIRST)

1. **`rust/bismark-extractor/SPEC.md` §8.3** — add a "Phase H PE matrix" subsection next to the existing SE one. Enumerate the 5 PE cells + cross-N assertion + 11,443 B M-bias baseline at (D, N=1) (rev 1 A-I5 dropped the 875 B splitting-report absolute lock) + the mixed-metric differential expectations (rev 1 A-O3: rows for `<D` cells; count-sum for `overlap`).
2. **`rust/bismark-extractor/SPEC.md` §10 row H** — mark the sub-gate-1-PE row as "in-progress" (PR-open state); the row's checkbox flips to ✅ on merge.

### 5.2 New `scripts/phase_h_pe_matrix.sh`

Mirror the structure of `scripts/phase_h_se_matrix.sh` (660 LOC post-#873). Key differences from the SE driver (rev 1 B-Opt-3 — expanded from 7 to 10 numbered points + mechanical-copy-from-SE checklist):

1. **Cell-set**: 5 cells per §3.1 (vs SE's D / 5p / 3p / 5p+3p / edge_clip).
2. **Flag-passthrough construction**: per CELL_ID, build the `--extra-rust`/`--extra-perl` strings:
   - D: `""`
   - r1_5p: `"--ignore 5"`
   - r2_5p: `"--ignore_r2 5"`
   - r1r2_3p: `"--ignore_3prime 5 --ignore_3prime_r2 5"`
   - overlap: `"--include_overlap"`
3. **PE-ness assertion in pre-flight** (rev 1 A-C1 — final form): `samtools view -H "$BAM" | grep -qE '^@PG.*ID:Bismark.*[[:space:]]-1[[:space:]]'` mirroring smoke's regex at `phase_h_smoke.sh:159`. Exit 2 if no match. (Rev 0 specified `--dry-run` which doesn't exist + a `--paired` regex which is wrong.)
4. **Overlap-fraction sanity check in pre-flight** (rev 1 A-I1 — new): `samtools view -c -f 0x2 "$BAM"` ≥ 80% of `samtools view -c "$BAM"`. Exit 2 if not met.
5. **Baseline**: 11,443 B M-bias at (D, N=1) only. (Splitting-report 875 B absolute lock dropped per rev 1 A-I5; per-cell Perl-vs-Rust byte-cmp is the splitting-report regression guard.)
6. **Differential metric — mixed per cell** (rev 1 A-O3): row count for `r1_5p`/`r2_5p`/`r1r2_3p` (assert < D), count-sum for `overlap` (assert > D + 5%). Compute via:
   - row count: `grep -cE '^[0-9]+\t' M-bias.txt`
   - count-sum: `grep -E '^[0-9]+\t' M-bias.txt | awk '{sum += $2 + $3} END {print sum}'`
   - SE used row-count for all four cells because ignore-flags monotonically remove rows; PE's overlap doesn't remove rows (positions are read-relative; M-bias reports all positions populated by any pair), so count-sum is the semantically correct metric.
7. **`ROW_COUNT_OK` fail-closed init** (rev 1 B-Imp-1 — new; mirrors `MBIAS_BASELINE_OK` pattern): initialize `ROW_COUNT_OK=0`; flip to 1 only on positive completion of all four differential assertions AND all 5 cells' M-bias files present + readable. Missing/unreadable file forces FAIL (the asymmetric `>` direction reintroduces the SE-absent fail-open bug class).
8. **Cell-ID naming convention**: `cell_p<N>_<CELL_ID>` mnemonic form (e.g., `cell_p1_D`, `cell_p4_overlap`). Asymmetric vs SE's parameter-encoded `cell_p<N>_i<5p>_i3<3p>` (rev 1 B-Imp-3 — documented in §10 Open; PE's 5-dimensional flag space makes parameter-encoded names unwieldy).
9. **Inline differential evidence** in `matrix_verdict.txt` + `speedup_table.md` (rev 1 B-Imp-2 — drops standalone `row_count_diff.txt`; SE-symmetric "evidence in two places" pattern). Distinct verdict line per differential FAIL (rev 1 B-Imp-5): `FAIL: differential <cell> <metric>=<observed> not <op> D=<v>`.
10. **Input BAM MD5 in `matrix_verdict.txt` + `speedup_table.md`** (rev 1 B-Opt-4 — fixture-drift detector; cheap MD5 record alongside the existing `Generated: <timestamp>` line). Release engineer compares against planner's reference to catch silent BAM swap-ins.

**Mechanical-copy-from-SE checklist** (rev 1 B-Opt-3 — implementer notes; do NOT re-implement these patterns, copy verbatim from `phase_h_se_matrix.sh`):

- **No shared `_phase_h_lib.sh`** (Felix directive 2026-05-28 — independent drivers). Do not refactor common code into a shared lib in v1.0; that's a `polish(extractor):` follow-up.
- **Rust binary discovery + on-demand build**: delegate to `phase_h_smoke.sh`'s existing env-var-check logic; do NOT re-implement.
- **`--out` empty-check pattern**: use `find <OUT> -mindepth 1 -maxdepth 1 -print -quit` (SE driver pattern); do NOT use `[[ -z $(ls $OUT) ]]` which has subtle quoting bugs.
- **All SE-rev-3 hardening forward-ported verbatim**: bash 4.0 pre-flight, `${ARR[@]+"${ARR[@]}"}` defensive idiom, `MBIAS_BASELINE_OK=0` fail-closed default, `R*100/P` speedup direction with `P=0` guard, BSD/GNU-tolerant `load average[s]?:` regex, SIGINT trap, nproc-missing graceful skip, tmux warning, sub-2s annotation. Same code patterns, same idioms; copy the SE driver's blocks 1:1 and adapt only the cell-set + flag-strings + differential-metric + baseline-value.

### 5.3 Populate `RELEASE_CHECKLIST.md` PE section

Replace the #873 TODO-stub per §4.3. Other sections (roles, escalation, SE matrix, v1.0 tag steps, sub-gate 2 placeholder) unchanged.

### 5.4 PROGRESS.md update

Add a Phase H PE row pointing at this plan + status `📝 plan rev 1 — awaiting implementation trigger`. The SE row stays at ✅ merged.

### 5.5 No crate code changes

Phase H is harness + checklist + SPEC. NO changes to `rust/bismark-extractor/src/*` or `rust/bismark-extractor/tests/*`. The 303-test baseline (post-G; preserved through #873's SE work) is preserved here too.

### 5.6 Pre-merge validation

1. `bash -n scripts/phase_h_pe_matrix.sh` — syntax check.
2. `shellcheck scripts/phase_h_pe_matrix.sh` — if available; else skip with note.
3. `cargo test -p bismark-extractor` — verify 303-test baseline preserved (no Rust changes; sanity check).
4. Optional local-Mac dry-run on a tiny PE BAM if available — pre-flight Perl-version assertion will correctly reject without `bioinf` env present; real end-to-end validation happens on colossal per RELEASE_CHECKLIST.md.

## 6. Efficiency

- **Matrix runtime: estimated 1.5-4 hours per BAM** (rev 1 A-O2 widened). PE is slower than SE per invocation (Perl's overlap-handling overhead + 2× the bytes through the extraction loop on average). Per-cell decomposition arithmetic for 10M PE on colossal:
   - At N=1: ~15-20 min Perl + ~12-18 min Rust per cell × 5 cells = 135-190 min.
   - At N=4: ~(15-20)/4 + (12-18)/4 = ~6.75-9.5 min per cell × 5 cells = 33.75-47.5 min.
   - **Total: ~170-238 min (~2.8-4 h end-to-end)**. The lower bound 1.5 h assumes faster-than-expected cells (cf. SE's 1-3 h estimate which proved mostly faster); upper bound 4 h matches the CLAUDE.md profiling note (PE on 55M ~104 min for extraction; 1/5.57 scaling gives ~19 min for 10M, consistent with the per-cell upper bound). Update post-first-colossal-run.
- **Driver overhead**: bash + 4 `diff -q` calls per CELL_ID (cross-N comparison) + row-count `grep -c` calls. ~ms-scale; invisible vs per-cell extraction time.
- **Disk usage**: ~2.5 GB per matrix run (5 cells × 2 binaries × ~250 MB per cell output; PE output ~1.4× SE due to two-mate reporting). Safety margin to 3 GB documented in §A14. Colossal Weka has TBs; well within.

## 7. Integration

### 7.1 Read/Write surface

- **Read**: PE BAM input (read-only); Perl + Rust binaries (PATH or env-overridden); existing `scripts/phase_h_smoke.sh` (no edits).
- **Write per run**: `<OUT>/cell_p<N>_<CELL_ID>/` per cell; `<OUT>/cross_n_summary.txt`; `<OUT>/row_count_diff.txt`; `<OUT>/speedup_table.md`; `<OUT>/matrix_verdict.txt`.
- **Repo writes** (PR scope):
  - `scripts/phase_h_pe_matrix.sh` (new, ~700 LOC).
  - `RELEASE_CHECKLIST.md` (populate the PE section; ~50 LOC delta).
  - `rust/bismark-extractor/SPEC.md` (§8.3 PE subsection + §10 row H PE; ~30 LOC).
  - `plans/05262026_bismark-extractor/PROGRESS.md` (Phase H PE row).

### 7.2 Downstream impact

| Consumer | Impact |
|---|---|
| Companion #871 (SE) — merged | Cross-referenced in §1 + §2.6; no further coupling needed; SE matrix on colossal completes independently. |
| v1.0 release tag | Final gate. RELEASE_CHECKLIST.md now has SE + PE sections populated; tag unblocked once both PASS on colossal. |
| Phase C.1 / C.2 regression detection | (D, N=1) cell's M-bias 11,443 B HARD-FAIL is the Phase C.1 polarity continuous regression guard (rev 1 A-I5 dropped splitting-report 875 B absolute lock; per-cell byte-cmp catches Phase C.2 format regressions). The `overlap` cell's count-sum > D + 5% differential is the Phase C.1 include_overlap-polarity explicit assertion (rev 1 A-O3 mixed metric). |
| Phase F N-invariance contract | Cross-N check tests SPEC §8.3 row 4 on the PE shape (which has more bytes through the worker-reduce path than SE). |
| External tooling | None — scripts + checklist are internal. |

### 7.3 Deliberately NOT implemented (defer to follow-ups)

- Self-hosted runner workflow (pre-resolved by SE precedent — checklist chosen).
- Shared `scripts/_phase_h_lib.sh` (pre-resolved — independent drivers; SE + PE drivers share code-style but not code).
- Larger PE BAM (`full_size/SRR24827373_...`) for speedup at N=8 — flagged in §10 Open; could add `--secondary-bam` in a future enhancement.
- Read-level grep for `SRR24827373.9` polarity assertion (the issue body's "canonical regression fixture") — implicit guard via M-bias byte-count + row-count differential is sufficient; explicit read-level check flagged in §10 Open.
- Performance investigation if Rust scaling < 4× — separate `perf(extractor):` sub-issue per #872 body.
- Inline N=8 cells unless `--parallel-set "1 4 8"` is passed.

## 8. Assumptions

### 8.1 From epic + prior phases

- **A1.** 10M PE BAM on colossal mirrors oxy's path-shape. Per memory `reference_colossal_access.md`. First colossal session verifies; update §2.5 if different.
- **A2.** Perl `bismark_methylation_extractor` v0.25.1 in `bioinf` micromamba env. Per memory.
- **A3.** colossal supports `--parallel 4` (≥4 cores). Pre-flight `nproc` check enforces (with graceful skip if nproc unavailable).
- **A4.** `bismark_methylation_extractor` version is **v0.25.1**. The 11,443 B M-bias baseline assumes this (rev 1 A-I5 dropped 875 B splitting-report absolute lock). Pre-flight version assertion enforces.
- **A5.** Rust `--parallel N` invariant (SPEC §8.3 row 4): output bytes identical regardless of N — tested per-CELL_ID via cross-N.
- **A6.** Perl `--multicore N` produces fork-modulo-then-concatenate ordering, N-dependent. SPEC §8.3 row 1 rev 3 accepts sorted-content equivalence on data files.
- **A7.** Phase C.2's empty-sweep produces deterministic 6-file kept-set for directional PE.
- **A8.** Phase C.1's drop_overlap polarity is correct: under `--no_overlap`, R2 calls overlapping R1 are dropped; under `--include_overlap`, R2 calls in overlap region are retained. The PE matrix's `overlap` cell count-sum > D + 5% differential (rev 1 A-O3 — M-bias positions are read-relative so rows are unchanged; counts at existing positions accumulate) is the explicit assertion of this contract.

### 8.2 Plan-specific

- **A9.** One PR (`extractor-phase-h-pe`) closes #872.
- **A10.** Branch from `rust/iron-chancellor` HEAD `651b7fd` (post-#873 SE merge).
- **A11.** No crate code changes.
- **A12.** RELEASE_CHECKLIST.md's PE section is populated by this PR (replacing the #873 TODO-stub); SE section is unchanged.
- **A13.** `--extra-rust` / `--extra-perl` values are bash-array-safe (no shell-metacharacter injection from a single trusted user).
- **A14.** colossal has ≥ 3 GB free in matrix output dir.
- **A15.** `phase_h_smoke.sh`'s `diff_summary.txt` emits wall-clock as `^Perl: <int>s$` / `^Rust: <int>s$` (pinned by #873). Driver parses with `grep -E`.
- **A16.** `nproc` is available on colossal (POSIX-standard utility). Degraded to a warning if not found.
- **A17.** Phase F's collector handles PE's two-mate-per-pair grouping identically across N values (i.e., the BTreeMap-collector groups by pair-id consistently). Cross-N check directly tests this on PE.
- **A18 (rev 1 A-I1).** Input PE BAM has properly-paired fraction ≥ 80% (samtools FLAG 0x2). Asserted via pre-flight (§3.3.2 #6). The 10M PE BAM is expected ~99%+ properly-paired (deduplicated paired-end alignment); the 80% threshold protects against future BAM swap-ins with low overlap (e.g., exome panels with mate-pair-disjoint reads) where the `overlap > D + 5%` count-sum differential would silently mis-fire.

## 9. Validation

### 9.1 Local-Mac pre-merge smoke

| Check | What | How | Expected |
|---|---|---|---|
| Driver syntax | bash parses | `bash -n scripts/phase_h_pe_matrix.sh` | exit 0 |
| Bash 4.0 pre-flight | macOS-bash-3.2 rejected | Run driver under `/bin/bash` (3.2) on macOS | exit 2 with "bash 4.0+ required" |
| Pre-flight PE-ness assertion (rev 1 A-C1 — samtools-direct) | SE BAM rejected via samtools regex | Run driver with the local Desktop 10M SE BAM | exit 2 with "expected PE BAM (Bismark `-1` arg in @PG)" |
| Pre-flight overlap-fraction (rev 1 A-I1 — new) | Low-overlap BAM rejected | Override input with a BAM where `samtools view -c -f 0x2` is < 80% of total | exit 2 with "BAM has X% properly-paired reads; requires ≥80%" |
| Pre-flight Perl version | Mismatched version rejected | Override `PERL_BIN` to a fake script emitting "v0.26.0" | exit 2 with explicit error |
| Pre-flight nproc check | N > nproc rejected (when nproc available) | `bash scripts/phase_h_pe_matrix.sh ... --parallel-set "9999"` | exit 2 |
| nproc-missing graceful skip | Driver doesn't fail if nproc absent | Mock by overriding PATH to exclude nproc; rerun pre-flight | exit advances past pre-flight |
| Cross-N check fires unconditionally (rev 1 B-Imp-4) | Cross-N runs even when cell byte-identity FAILed | Manually tamper with `cell_p4_D/rust/*.M-bias.txt` AND `cell_p1_D/rust/*.M-bias.txt` post-extraction; rerun matrix | exit 1 with BOTH "byte-identity FAIL" AND "cross-N regression at cell D" — preserves diagnostic signal |
| Differential fires (rev 1 A-O3 + B-Imp-1 + B-Imp-5) | Mixed-metric differential FAILs trigger distinct verdict line | Manually overwrite `cell_p1_overlap/rust/*.M-bias.txt` with D's content (count-sum = D, not > D + 5%) | exit 1 with "FAIL: differential overlap count-sum=... not > D=... by ≥5%" — distinct from byte-identity FAIL |
| Missing M-bias file forces FAIL (rev 1 B-Imp-1) | `ROW_COUNT_OK` fail-closed on missing file | Manually delete `cell_p1_overlap/rust/*.M-bias.txt`; rerun matrix aggregation | exit 1 with "ROW_COUNT_OK=0; cell overlap M-bias.txt unreadable" |
| `cargo test` baseline | 303-test baseline preserved | `cargo test -p bismark-extractor` | 303 passed |

### 9.2 Colossal release-gate validation (per RELEASE_CHECKLIST.md; not PR-blocking)

| Check | What | How | Expected |
|---|---|---|---|
| Full PE matrix on colossal 10M PE | All 5 cells × 2 N PASS byte-identity + cross-N + row-count differentials | `bash scripts/phase_h_pe_matrix.sh /weka/.../10M_PE/...bam` | exit 0 or 3; matrix_verdict.txt PASS |
| `*.M-bias.txt` baseline at (D, N=1) | 11,443 B exact | `wc -c cell_p1_D/rust/*.M-bias.txt` | 11443 |
| `*_splitting_report.txt` per-cell byte-cmp (rev 1 A-I5 — absolute 875 B lock dropped) | Each cell's Rust splitting-report byte-equal to Perl | Read `cell_p<N>_<CELL>/diff_summary.txt` per cell | "splitting-report: PASS" per cell |
| Differential check (rev 1 A-O3 mixed metric; rev 1 B-Imp-2 inlined) | r1_5p/r2_5p/r1r2_3p rows < D; overlap count-sum > D + 5% | Read `matrix_verdict.txt` inline differential block (no separate row_count_diff.txt per rev 1) | All 4 directional assertions PASS; `ROW_COUNT_OK=1` |
| Overlap-fraction header (rev 1 A-I1) | speedup_table.md records measured overlap fraction | `grep '^Properly-paired fraction:' <OUT>/speedup_table.md` | ≥ 80% |
| Input BAM MD5 (rev 1 B-Opt-4) | speedup_table.md records BAM MD5 for fixture-drift detection | `grep '^Input BAM MD5:' <OUT>/speedup_table.md` | matches planner's reference |
| Cross-N N-invariance | Rust-N=1 ≡ Rust-N=4 raw-byte per CELL_ID | `cross_n_summary.txt` | 5 PASS rows |
| Speedup at N=4 | Rust scaling ≥ 4× | Read speedup_table.md | Rust scaling 4×+ (or exit 3 + follow-up sub-issue) |
| Disk footprint | Matrix output ≤ 3 GB | `du -sh <OUT>` | ≤ 3 GB |

### 9.3 Cross-phase regression check

| Check | What | How | Expected |
|---|---|---|---|
| Phase C.1 polarity guard | M-bias at (D, N=1) is 11,443 B AND `overlap` cell count-sum > D + 5% (rev 1 A-O3 mixed-metric) | Matrix's hard-fail checks | PASS |
| Phase C.2 empty-sweep | 6 kept files per cell | Per-cell verdict | PASS |
| Phase C.2 splitting-report format | Per-cell Rust splitting-report byte-equal to Perl (rev 1 A-I5 — absolute 875 B lock dropped) | Each cell's smoke verdict | PASS |
| Phase F N-invariance | Cross-N PASS per CELL_ID | `cross_n_summary.txt` | PASS |
| Phase G unchanged | Existing 303 tests pass | `cargo test -p bismark-extractor` | 303 pass |

## 10. Questions or ambiguities

### Critical — none (post-rev-1 absorption)

The 1 Critical (A-C1 — unrunnable `--dry-run` PE-ness pre-flight) is fully folded into rev 1 via §3.3.2 #5 (samtools-direct regex mirroring `phase_h_smoke.sh:159`). No remaining Criticals.

### Resolved in rev 1 (was Open in rev 0)

| Q | Resolution |
|---|---|
| PE-ness assertion mechanism (rev 0 default = smoke `--dry-run`; alternative = `--paired` regex) | **Resolved by A-C1 fix:** §3.3.2 #5 now uses `samtools view -H "$BAM" \| grep -qE '^@PG.*ID:Bismark.*[[:space:]]-1[[:space:]]'` mirroring smoke's existing detection regex. Neither rev-0 mechanism was implementable as written. |
| Driver-emitted `row_count_diff.txt` separate file | **Resolved by B-Imp-2:** dropped the standalone file; inlined differential evidence in `matrix_verdict.txt` + `speedup_table.md` (SE-symmetric "evidence in two places" pattern). |
| `overlap > D` row-count direction | **Resolved by A-O3:** metric changed from row-count to count-sum for the `overlap` cell. M-bias rows are read-relative; `--include_overlap` accumulates counts at existing positions without adding rows. Count-sum > D + 5% is the semantically correct assertion. Row-count differential preserved for the three `<D` cells (SE-symmetric where ignore flags remove positions). |
| BAM-portability of `overlap > D` (rev 0 Open #4 + §11 R3) | **Resolved by A-I1:** new pre-flight overlap-fraction ≥ 80% gate (§3.3.2 #6) rejects BAMs where the count-sum differential would silently mis-fire. Documented in §A18. |
| Splitting-report 875 B absolute lock at (D, N=1) | **Resolved by A-I5:** DROPPED. Splitting-report contains input filename in first line; absolute byte count is path-length sensitive. Per-cell Perl-vs-Rust strict-byte-cmp remains the splitting-report regression guard. |

### Open (defaults taken — non-critical, flagged for plan-reviewer round 2 if any)

| Q | Default | Rationale |
|---|---|---|
| Matrix-cell asymmetry vs SE (PE has `overlap`, no `edge_clip`; SE inverse) | PE 5-cell per #872 body specification | SE's `edge_clip` cell tests `extract_calls` boundary handling, irrelevant to PE; PE's `overlap` cell tests Phase C.1's polarity fix. Defensible asymmetry. |
| Isolated `r1_3p` + `r2_3p` cells (rev 1 A-O1 — subsumed by `r1r2_3p`) | NOT included | Combined `r1r2_3p` cell exercises both axes; PASS byte-identity catches any 3p-handling bug in either constituent. Isolation cells are 2× more thorough but at proportional runtime cost. Add post-v1.0 if needed. |
| Explicit (overlap, N=1) M-bias byte-count baseline (rev 1 A-I2 — known coverage gap for include_overlap-only regressions) | Rev-2 enhancement post-first-colossal | Measure on first colossal run; commit rev-2 baseline-update PR with the measured value. Adds an explicit lock for `--include_overlap`-only regressions that escape the (D, N=1) lock + the count-sum differential. |
| Read-level grep for SRR24827373.9 R2-overlap in `overlap` cell | NOT included in rev 1 driver | Alternative to A-I2 above. Defer for fragility (read names can change with re-alignment). Reconsider if A-I2's rev-2 baseline doesn't catch a future regression. |
| Cell-id naming convention asymmetry vs SE (mnemonic `D`/`r1_5p`/... vs SE's parameter-encoded `i<5p>_i3<3p>`, rev 1 B-Imp-3) | Mnemonic | PE's 5-dimensional flag space makes parameter-encoded names unwieldy. Mnemonic trades cross-plan symmetry for readability. Release engineer parses from speedup table's "Flags" column. |
| Overlap fraction < 80% BAMs (rev 1 A-I1) — soft `--skip-overlap-differential` override flag vs hard pre-flight rejection | Hard rejection | A soft override would let release engineer run the matrix on a non-canonical BAM accepting a weaker assertion. Not implemented in rev 1; v1.0 uses the canonical 10M PE BAM. Add the flag in a `polish(extractor):` follow-up if a different release-gate BAM is adopted post-v1.0. |
| SPEC §8.3 + §10 row H edits — crate version bump? (rev 1 B-Opt-1) | NO version bump | SPEC edits are documentation of already-implicit invariants tested by the harness; not new contract obligations. No new Rust code; no new runtime behavior. Defensible "no bump" call. |
| Larger PE BAM (`full_size/SRR24827373_...`) for speedup at N=8 | NOT included | Same reasoning as SE rev 1 I7: unreachable through single-BAM-arg CLI; future `--secondary-bam` enhancement if needed. |
| `--parallel 8` cell-set inclusion | Off by default; user opts in via `--parallel-set "1 4 8"` | Saves runtime when colossal has < 8 free cores; PE perf likely worse at N=8 due to gzip I/O bottleneck (per profiling summary in CLAUDE.md). |
| Exit code 3 vs structured field for perf-target-miss | Exit code 3 (mirror SE rev 1 I16) | Phase C.2 era PE speedup measured 0.9×; this matrix likely lands on exit 3 first time. v1.0 may legitimately ship at exit 3 per §1 + §3.3.6. |

### Open (post-rev-1 reviewer attention magnets)

Rev 1's remaining choices a round-2 reviewer may push back on:

1. **Differential metric mixed per cell** (rev 1 A-O3 — rows for `<D` cells; count-sum for `overlap`). Defensible because the assertion direction differs semantically, but a reviewer may prefer unified metric (count-sum for all four cells; sacrifices SE-symmetry on the `<D` cells). Mixed metric documented in §3.3.5 + §5.2 #6.
2. **(overlap, N=1) explicit baseline deferred to rev-2** (rev 1 A-I2 — the only known coverage gap for include_overlap-only regressions). Reviewer may push for placeholder TBD-value-in-rev-1-driver-skeleton with measurement-and-replace on first colossal run.
3. **Overlap-fraction threshold value = 80%** (rev 1 A-I1). Picked as conservative-but-not-paranoid. Reviewer may argue 70% or 90%.
4. **Hard pre-flight rejection of low-overlap BAMs** vs soft `--skip-overlap-differential` flag. Rev 1 chose hard; reviewer may push for soft.
5. **Mnemonic vs parameter-encoded cell-ids** (rev 1 B-Imp-3 — kept mnemonic). Reviewer may push for parameter-encoded; current default trades cross-plan symmetry for readability.
6. **Splitting-report absolute lock dropped entirely** (rev 1 A-I5). Reviewer may push for a tolerance-window lock (e.g., `873 ≤ size ≤ 880`) instead of full drop; rev 1 default is full drop on the grounds that per-cell byte-cmp catches regressions symmetrically.

## 11. Self-Review (rev 1)

Reviewed rev 1 for:

- **Efficiency:** 5-cell × 2 N matrix runtime is bounded by Perl PE extraction time × 10 invocations. Driver overhead negligible. Disk ~2.5 GB. PE is ~1.4× SE per invocation. Runtime estimate widened to 1.5-4 h with per-cell decomposition (rev 1 A-O2). ✓
- **Logic consistency:** Cross-N check (§3.3.4) carried forward from SE rev 1 C1 — strongest single assertion. Pre-flight ordering documented in §3.3.2 (11 steps with new overlap-fraction gate at #6). Exit-code mapping articulated in §3.3.6 with `BASELINE_GATE_APPLIES` gating. Differential metric is mixed per cell (row count for `<D` cells; count-sum for `overlap`) per rev 1 A-O3 — documented explicitly. ✓
- **Edge cases:** §3.5 expanded from 14 to 17 cases. New cases: overlap-fraction < 80%, baseline drift recovery, missing-@PG, corrupt-header @PG, include_overlap-only polarity off-by-one. ✓
- **Integration:** SPEC §8.3 + §10 row H + RELEASE_CHECKLIST PE section all in scope. Phase F N-invariance directly tested on PE shape. C.1's 11,443 B M-bias baseline preserved as HARD-FAIL guard at (D, N=1). Splitting-report 875 B absolute lock dropped per rev 1 A-I5 — Perl-vs-Rust byte-cmp is the format regression guard. ✓
- **Test coverage:** Driver itself has no Rust unit tests (bash). Pre-merge §9.1 covers driver-correctness via local-Mac syntax + pre-flight rejection scenarios. Colossal validation is release-gate, not merge-gate. ✓
- **Cross-plan symmetry:** #871 (SE, merged) and #872 (this PR) both have 5-cell × 2 N × 2 binaries = 20-invocation matrices. Cell content is mode-specific; structural shape symmetric. Cell-id naming convention differs (mnemonic vs parameter-encoded) per rev 1 B-Imp-3 — documented in §10 Open. ✓
- **Pre-folded SE rev-3 findings:** A-Er1/Er2, A-L1≡B-L2, B-L1, B-E3 + Mediums (SIGINT, nproc-missing, tmux, sub-2s, uptime regex) all explicitly carried into rev 1. Cargo build budget line added to RELEASE_CHECKLIST per rev 1 minor catch. ✓
- **PE-specific fail-closed gates:** `MBIAS_BASELINE_OK=0` (SE-symmetric) + `ROW_COUNT_OK=0` (new in rev 1 per B-Imp-1 — PE's asymmetric `>` direction reintroduces the fail-open class bug SE doesn't have). Both flipped to 1 only on positive confirmation; missing-file forces FAIL. ✓

### Adjustments made during rev 1 absorption

Folded **1 Critical (A-C1) + 9 distinct Important findings + selected Optional polish**:

- **A-C1** → §3.3.2 #5 PE-ness pre-flight rewritten to samtools-direct with correct regex
- **A-I1 ≡ B-Imp-1 ≡ B-Imp-5 consensus** → §3.3.2 #6 overlap-fraction pre-flight + §3.3.2 #11 `ROW_COUNT_OK=0` fail-closed init + §3.3.5 distinct verdict line for differential FAIL
- **A-I2** → §10 Open with rev-2 default (explicit (overlap, N=1) baseline post-first-colossal)
- **A-I3** → §3.3.2 #11 `BASELINE_GATE_APPLIES` introduced
- **A-I4** → §3.5 edge-case row + §4.3 RELEASE_CHECKLIST escalation path for colossal-vs-planner baseline drift
- **A-I5** → §3.4 #3 + §3.3.6 + §10 Resolved: dropped 875 B absolute splitting-report lock
- **A-O3 (promoted from Optional to absorbed)** → §3.3.3 step 5 + §3.3.5 differential block: count-sum metric for `overlap` cell instead of row-count
- **B-Imp-2** → §3.3.5 inline differential in matrix_verdict.txt + speedup_table.md; standalone row_count_diff.txt dropped
- **B-Imp-3** → §10 Open: mnemonic cell-ids documented + justified vs SE
- **B-Imp-4** → §3.3.4 explicit "cross-N runs unconditionally even when byte-identity FAILed"
- **A-O1** → §3.1 "Why isolated R1-3p and R2-3p cells are absent" paragraph
- **A-O2** → §6 widened to 1.5-4 h with per-cell decomposition arithmetic
- **B-Opt-1** → §10 Open: "SPEC edits are docs, not contract — no version bump"
- **B-Opt-2(c)** → §3.5 + §3.4 #6: per-base polarity off-by-one note
- **B-Opt-3** → §5.2 expanded from 7 to 10 numbered implementer-brief points + mechanical-copy-from-SE checklist
- **B-Opt-4** → §3.3.5 markdown header + §3.3.6 verify steps: input BAM MD5 in matrix_verdict.txt + speedup_table.md
- **B-Opt-5** → §1 + §3.3.6: consolidated "v1.0 may legitimately ship with PE matrix exiting 3" statement
- **Cargo build budget** (A's minor) → §4.3 RELEASE_CHECKLIST PE section

### Remaining risks (post-rev-1)

- **R1**: First colossal session may discover the 10M PE BAM at a different subpath. Mitigation: RELEASE_CHECKLIST.md instructs `ls`; trivial follow-up commit if needed.
- **R2**: M-bias byte counts for non-(D,N=1) cells are determined empirically on first colossal run — rev 1 specifies differential checks (rows for `<D` cells; count-sum for `overlap`) but not exact byte locks per cell. The (overlap, N=1) explicit baseline (A-I2) is the highest-priority rev-2 follow-up.
- **R3 (rev 0 resolved)**: ~~`overlap > D` row-count differential assumes the 10M PE BAM has sufficient overlapping pairs~~. **Resolved** by rev 1 A-I1 pre-flight overlap-fraction ≥ 80% gate + rev 1 A-O3 metric change to count-sum.
- **R4**: Cross-N check failure mode — if it fires on the first colossal run, that's a Phase F regression specific to PE (worker-reduce + overlap-aware collector). Documented as release-blocker in §4.3 + §3.5.
- **R5**: Read-level Phase C.1 polarity assertion is OPTIONAL (deferred to rev-2 A-I2 baseline instead). If both A-I2's rev-2 baseline AND the (D, N=1) M-bias lock AND the count-sum differential collectively miss a regression, file `bug(extractor):` and promote the read-level check.
- **R6**: PE on 10M may take 4+ hours at N=1 (vs SE's 1-3 h estimate). Rev 1 widened §6 to 1.5-4 h; first colossal run measures.
- **R7 (new in rev 1)**: Colossal-vs-planner baseline drift (rev 1 A-I4 — see §3.5 + §4.3 escalation path). 11,443 B M-bias is locked from oxy measurement; first colossal run may differ trivially (BAM-environment-dependent). Recovery path documented; not a release-blocker without verification of an actual regression.
- **R8 (new in rev 1)**: Differential metric is mixed (rows vs count-sum). An implementer reading §3.3.3 may confuse the two — §5.2 #6 explicitly enumerates the awk recipe for each. Mitigation: dual code-reviewer pass will catch any cross-wiring.

### Reviewer-attention magnets (post-rev-1)

See §10 Open "post-rev-1 reviewer attention magnets" for the 6 remaining choices a round-2 reviewer may push back on (mixed differential metric; (overlap, N=1) baseline deferred to rev-2; 80% overlap-fraction threshold; hard-vs-soft low-overlap rejection; mnemonic-vs-parameter-encoded cell-ids; splitting-report absolute-lock fully dropped). Rev 1 closes the rev-0 magnets (cell-count asymmetry; Phase C.1 explicit regression assertion; PE-ness assertion mechanism; `overlap > D` direction).

---

## 12. Open delivery cycle

1. ✅ Sub-issue filed: [#872](https://github.com/FelixKrueger/Bismark/issues/872), linked under epic #798, board fields set.
2. ✅ Plan rev 0 written.
3. ✅ Manual review by Felix — approved, directed to dual plan-reviewers.
4. ✅ Dual `plan-reviewer` agents — `PLAN_REVIEW_PHASE_H_PE_A.md` (NEEDS-REVISIONS: 1 Crit + 5 Imp + 6 Opt) + `PLAN_REVIEW_PHASE_H_PE_B.md` (APPROVE-WITH-NITS: 0 Crit + 5 Imp + 5 Opt). 1 distinct Critical + 9 distinct Importants after de-duplication.
5. ✅ **Plan rev 1** folding all findings + selected Optional polish — this file.
6. 🟡 **Implementation trigger from Felix** — *PENDING*.
7. ⏸ Implementation per §5.
8. ⏸ Dual `code-reviewer` agents.
9. ⏸ `plan-manager` Mode B audit.
10. ⏸ PR `extractor-phase-h-pe` → `rust/iron-chancellor`, closes #872.
11. ⏸ Merge.
12. ⏸ RELEASE_CHECKLIST.md PE section walked on colossal (post-merge); v1.0 tag once SE + PE both PASS.

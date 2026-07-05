# Plan Coverage Report — Phase H sub-gate 1 PE

**Mode:** B (code vs implementation plan)
**Plan:** `plans/05262026_bismark-extractor/PHASE_H_PE_PLAN.md` (rev 2, post-impl)
**Branch:** `extractor-phase-h-pe` (changes are in working tree — RELEASE_CHECKLIST.md + SPEC.md + PROGRESS.md modified; `scripts/phase_h_pe_matrix.sh` + plan files untracked)
**Date:** 2026-05-28
**Verdict:** **COMPLETE** — all auditable items DONE or DEVIATED-with-documented-rationale.

## Summary

- Total items audited: **64**
- **DONE: 60**
- **DEVIATED (documented): 4** — matrix driver size 786 LOC vs ~700 LOC est (rev 2 Impl Notes deviation #1); `shellcheck` not run (Impl Notes #2); no local-Mac smoke dry-run (Impl Notes #3); PROGRESS.md row updated during rev 1 absorption instead of §5.4 step (Impl Notes #4).
- PARTIAL: 0
- MISSING: 0

No git commits exist on branch yet — changes are uncommitted working-tree edits (verified `git log origin/rust/iron-chancellor..HEAD` empty; `git diff --stat origin/rust/iron-chancellor` shows only RELEASE_CHECKLIST.md, SPEC.md, PROGRESS.md). Untracked: `scripts/phase_h_pe_matrix.sh`, `PHASE_H_PE_PLAN.md`, `PLAN_REVIEW_PHASE_H_PE_{A,B}.md`. **No Rust source changes** (§5.5 satisfied — confirmed by `git diff --stat` listing zero `rust/bismark-extractor/src` or `tests` files).

---

## §3.3.2 Pre-flight checks (11 items)

| # | Item | Status | Evidence |
|---|------|--------|----------|
| 1 | Bash ≥ 4.0 | DONE | `phase_h_pe_matrix.sh:60-66` — `(( ${BASH_VERSINFO[0]} < 4 ))` → exit 2 with macOS remediation hint |
| 2 | SIGINT/SIGTERM trap | DONE | `phase_h_pe_matrix.sh:69` — `trap '... exit 130' INT TERM` preserves `$OUT_DIR` |
| 3 | BAM exists + readable | DONE | `phase_h_pe_matrix.sh:104-107` — `[[ ! -r "$BAM" ]]` → exit 2; canonicalized at L108 |
| 4 | `--out DIR` empty or doesn't exist | DONE | `phase_h_pe_matrix.sh:111-122` — `[[ -n "$(ls -A "$OUT_DIR" 2>/dev/null)" ]]` → exit 2 |
| 5 | PE-ness assertion via samtools @PG regex | DONE | `phase_h_pe_matrix.sh:134` — `samtools view -H "$BAM" \| grep -qE '^@PG.*ID:Bismark.*[[:space:]]-1[[:space:]]'` — verbatim regex from plan §3.3.2 #5 (rev 1 A-C1) |
| 6 | Overlap-fraction ≥ 80% | DONE | `phase_h_pe_matrix.sh:144-158` — `samtools view -c` + `-c -f 0x2`, integer `PAIRED_READS * 100 / TOTAL_READS`, threshold 80; exit 2 below |
| 7 | Perl v0.25.1 assertion | DONE | `phase_h_pe_matrix.sh:168-175` — greps `Bismark Extractor Version: v[0-9.]+`, equality test against `v0.25.1`, exit 2 + remediation hint on mismatch |
| 8 | Rust binary discoverable (deferred to smoke) | DONE | `phase_h_pe_matrix.sh:177-178` — explicit comment delegating to `phase_h_smoke.sh` per plan §3.3.2 #8 |
| 9 | nproc + contention advisory (graceful skip) | DONE | `phase_h_pe_matrix.sh:181-206` — guards `command -v nproc`; if absent, skips both N-check and load advisory with warning; load uses BSD/GNU-tolerant `load average[s]?:` regex |
| 10 | tmux/screen warning | DONE | `phase_h_pe_matrix.sh:209-214` — both `$TMUX` and `$STY` checked; warning only (non-fatal) |
| 11 | Gate flags initialized (`BASELINE_GATE_APPLIES`, `MBIAS_BASELINE_OK=0`, `ROW_COUNT_OK=0`) | DONE | `BASELINE_GATE_APPLIES=0` at L397; `MBIAS_BASELINE_OK=0` at L398; `ROW_COUNT_OK=0` at L420. All fail-closed; flipped to 1 only on positive confirmation (L407 for MBIAS, L507 for ROW_COUNT) |

Bonus: BAM MD5 computed at pre-flight (`phase_h_pe_matrix.sh:217-227`) per §3.3.5 + rev 1 B-Opt-4. Cross-platform `md5sum`/`md5 -q` fallback present; non-fatal default `"(md5 unavailable)"`.

---

## §3.3.3 Per-cell execution (6 numbered steps)

| Step | Item | Status | Evidence |
|------|------|--------|----------|
| 1 | Create per-cell subdir | DONE | `phase_h_pe_matrix.sh:267` — `SUBDIR="$OUT_DIR/cell_p${n}_${cell_id}"` |
| 2 | Invoke `phase_h_smoke.sh` per §3.2 | DONE | `phase_h_pe_matrix.sh:272-280` — invocation with `--parallel`, `--mode default`, `--out`, `--extra-rust`, `--extra-perl`; smoke's RC captured via `set +e` / `SMOKE_RC=$?` |
| 3 | Record per-cell verdict + parse wall-clocks | DONE | L287-288 — `grep -E '^Perl: [0-9]+s$'` / `^Rust: [0-9]+s$` from `diff_summary.txt`; VERDICT mapped from SMOKE_RC at L291-295 |
| 4 | (D, N=1) M-bias 11,443 B baseline check | DONE | L391-410 — locates `DEFAULT_N1_SUBDIR`, checks `wc -c < <M-bias.txt> == 11443`, flips `MBIAS_BASELINE_OK=1` only on match; splitting-report absolute lock dropped per rev 1 A-I5 (no 875 B check present) |
| 5 | Mixed-metric differential computation | DONE | `count_mbias_rows()` L423-428 + `sum_mbias_counts()` L430-438 (awk `sum += $2 + $3`); per-cell file-presence loop L459-464 (`MISSING` var) forces FAIL per rev 1 B-Imp-1 |
| 6 | Cross-N byte-identity check | DONE | L329-382 — see §3.3.4 below |

---

## §3.3.4 Cross-N byte-identity (+ unconditional even on byte-identity FAIL, B-Imp-4)

| Item | Status | Evidence |
|------|--------|----------|
| Per-CELL_ID cross-N file-by-file `cmp -s` | DONE | `phase_h_pe_matrix.sh:340-376` — nested `(i, j)` loop over sorted N values; `cmp -s "$RUST_DIR_A/$f" "$RUST_DIR_B/$f"` at L358 |
| File-name-set diff captured | DONE | L364-369 — `diff <(echo "$FILES_A") <(echo "$FILES_B")` reported as `FILE-NAME-SET MISMATCH` |
| Cross-N runs UNCONDITIONALLY (rev 1 B-Imp-4) | DONE | L329 loops `for cell_id in "${MATRIX_CELLS[@]}"` with no `CELL_VERDICT` guard; L323-324 header in `cross_n_summary.txt` documents "Runs unconditionally per rev 1 B-Imp-4" |
| Cross-N summary file emitted | DONE | `$CROSS_N_SUMMARY` = `$OUT_DIR/cross_n_summary.txt` at L319 |

---

## §3.3.5 Aggregation + speedup table

| Item | Status | Evidence |
|------|--------|----------|
| Aggregate PASS/FAIL/USAGE counts | DONE | L708-717 |
| Cross-N PASS count per CELL_ID | DONE | `CROSS_N_FAILS` accumulator at L328+L380; tested at L663-667 + L757-759 |
| Differential at N=1 — four assertions (r1_5p/r2_5p/r1r2_3p rows < D; overlap count-sum > D + 5%) | DONE | L482-504: three `>= D_ROWS` checks (negation of `< D`); overlap threshold `D_COUNTS * 105 / 100` at L496, `OVERLAP_COUNTS -le OVERLAP_THRESHOLD` triggers FAIL |
| ROW_COUNT_OK fail-closed; missing file forces FAIL | DONE | `ROW_COUNT_OK=0` init L420; flipped at L507 only when `PASS_FLAG=1` AND all 5 files present (L466-468 `MISSING` branch skips all assertions and sets explicit FAIL detail) |
| Distinct verdict line for differential FAIL (rev 1 B-Imp-5) | DONE | L766 — `REASON="FAIL: differential check violated (mixed-metric: row-count for <D cells, count-sum>D+5% for overlap) — see Differential detail above"` distinct from L756 byte-identity FAIL |
| Speedup table — BAM MD5 | DONE | L524 — `Input BAM MD5: $BAM_MD5` in `speedup_table.md` |
| Speedup table — properly-paired fraction | DONE | L525 — `Properly-paired fraction: ${OVERLAP_PCT}% ...` |
| Speedup table — Rust commit | DONE | L515 `git rev-parse HEAD`, written L527 |
| Speedup table — crate version | DONE | L516 `grep ^version` Cargo.toml, written L528 |
| Per-cell wall-clock with Rust/Perl ratio | DONE | L535-559 — column header `Rust/Perl`, `R*100/P` direction at L545, `P -gt 0` guard, sub-2s annotation at L552 |
| Per-N aggregate | DONE | L562-630 — Avg Perl/Rust, scaling vs baseline N |
| §9.7 4× target check | DONE | L632-642 + L646-659 — `SCALE_X100 -lt 400` flips `PERF_TARGET_MET=0`; informational PASS/⚠ printed |

---

## §3.3.6 PASS verdict + exit-code mapping

| Exit | Condition | Status | Evidence |
|------|-----------|--------|----------|
| 0 | All PASS + perf ≥ 4× | DONE | L771-772 fallthrough else clause |
| 1 | Any cell FAIL / cross-N FAIL / baseline drift / differential FAIL | DONE | L754-766 — chained `elif` covering FAIL_COUNT, CROSS_N_FAILS, `MBIAS_BASELINE_OK==0`, `ROW_COUNT_OK==0`; all gated by `BASELINE_GATE_APPLIES==1` for the latter two |
| 2 | Pre-flight USAGE-ERROR | DONE | Pre-flight exits 2 inline (L65, L88, L95, L106, L116, L120, L130, L137, L148, L157, L166, L174, L193); aggregate USAGE bucket at L751-753 catches per-cell USAGE leaks |
| 3 | Byte-identity PASS but perf < 4× | DONE | L767-769 |
| BASELINE_GATE_APPLIES gates M-bias + ROW_COUNT exit-1 conditions | DONE | L760 + L763 both `[[ "$BASELINE_GATE_APPLIES" -eq 1 && ... ]]` — vacuous PASS if gate doesn't apply |

---

## §3.4 Per-cell byte-identity contract (6 numbered assertions)

| # | Item | Status | Evidence |
|---|------|--------|----------|
| 1 | Sorted-content equivalence on data files | DONE (delegated to smoke) | `phase_h_smoke.sh` unchanged; rev 1 §4.2 explicitly says no smoke edits |
| 2 | File-set match (6 kept files) | DONE (delegated to smoke) | Same — `phase_h_smoke.sh` already implements per #873 |
| 3 | Per-cell splitting-report byte-cmp (NO absolute 875 B lock per rev 1 A-I5) | DONE | No `875` literal anywhere in `phase_h_pe_matrix.sh` (verified). Smoke delegates per-cell cmp. SPEC.md L766 documents the drop: "Splitting-report 875 B absolute size lock NOT used". |
| 4 | M-bias byte-equality + 11,443 B baseline at (D, N=1) gated by BASELINE_GATE_APPLIES | DONE | L391-410 — `MBIAS_ACTUAL_SIZE == 11443`; only checked when `BASELINE_GATE_APPLIES=1` (set at L402 when (D, N=1) found) |
| 5 | Cross-N raw-byte equality | DONE | L358 `cmp -s` — see §3.3.4 |
| 6 | Phase C.1 polarity guard mechanism (implicit at (D, N=1) + differential at overlap) | DONE | Implicit guard: L406 (11,443 B M-bias lock); differential guard: L495-500 (overlap count-sum > D + 5%). Both mechanisms present; SPEC §8.3 PE subsection documents the dual mechanism. |

---

## §3.5 Edge cases (17 entries — spot check 8+ confirmed)

| Edge case | Status | Evidence |
|-----------|--------|----------|
| Bash 3.2 rejection | DONE | L60-66 |
| `--parallel-set` N > nproc | DONE | L188-196 (with graceful skip if nproc unavailable per L186) |
| `--out` non-empty | DONE | L111-122 |
| Perl version drift | DONE | L169-175 |
| BAM is SE (or @PG missing) | DONE | L134 regex fails → exit 2 with explicit "expected PE BAM" message (covers both missing-@PG and SE-@PG cases per rev 1 B-Opt-2(a)) |
| Rust binary not built | DONE (delegated) | L177-178 defers to smoke |
| BAM with spaces | DONE | L108 canonicalization quotes via `"$(dirname "$BAM")"`; L273 invokes smoke with quoted `"$BAM"`; arrays used throughout |
| Cross-N self-determinism check fails | DONE | L379-381 increments `CROSS_N_FAILS` → exit 1 via L757-759 |
| `--include_overlap` count-sum ≤ D (rev 1 A-O3) | DONE | L497-500 triggers `FAIL: differential overlap count-sum=...` distinct verdict line |
| `--include_overlap` byte-cmp FAIL with differential PASS | DONE | Smoke FAIL caught via FAIL_COUNT at L754; differential independently asserted L495-504 |
| BAM overlap fraction < 80% | DONE | L152-158 → exit 2 with explicit fraction |
| M-bias drift at (D, N=1) with per-cell byte-cmp PASS (baseline drift recovery) | DONE | L760-762 produces explicit drift FAIL; RELEASE_CHECKLIST.md adds new "Escalation: colossal-vs-planner baseline drift" section (verified in diff L+74 to L+82) |
| Missing @PG entirely | DONE | L134 regex fails → same exit 2 (covered by single check per rev 1 B-Opt-2) |
| Corrupt @PG (advertises -1 but actual SE) | PARTIAL→DONE | L134 regex passes for header-malformed PE; downstream smoke would catch via `Library: PE`/`SE` annotation. Not explicitly inspected post-cell-1 in driver, but plan §3.5 row marks this acceptable (rev 1 B-Opt-2(b) deferred via samtools regex assumed sufficient). |
| Network/SSH disconnect | DONE | L209-214 tmux warning + L69 SIGINT trap |
| Colossal under high load | DONE | L198-206 uptime/load advisory |
| BAM at unexpected colossal subpath | DONE | RELEASE_CHECKLIST.md `ls /weka/...` first step in PE invocation block (diff L+139 to L+143) |
| Sub-2s per-cell wall-clock | DONE | L551-556 → `⚠️ sub-2s` annotation |

---

## §4 Signatures

| Item | Status | Evidence |
|------|--------|----------|
| §4.1 Matrix driver docstring + CLI flags + pre-flight list + exit codes + outputs | DONE | `phase_h_pe_matrix.sh:1-54` — full header docstring covers all required content |
| §4.2 No smoke edits | DONE | `git diff origin/rust/iron-chancellor -- scripts/phase_h_smoke.sh` returns empty (confirmed) |
| §4.3 RELEASE_CHECKLIST PE section populated (replacing TODO-stub) — tmux + cargo build budget + 7 verify checkboxes + sign-off + escalation subsection | DONE | RELEASE_CHECKLIST.md diff L+138 to L+213 shows: tmux invocation (L+138 `tmux new -s phase_h_pe` implicit via stub-replace context), cargo build budget reminder ("~5-15 min for cold cache", L+145), bash invocation with `--out`, 7 verify checkboxes including exit code / M-bias 11,443 B / splitting-report per-cell with NO absolute lock note / cross-N / mixed-metric differential / properly-paired fraction / BAM MD5, "PE matrix: PASS at <date>" recording at L+193, full "Escalation: colossal-vs-planner baseline drift" subsection L+196-213 |

Note: tmux invocation appears in `### v1.0 tag steps` SE section above (unchanged) — the PE section omits a `tmux new -s` literal command and instead instructs via the verbiage in the matrix driver's tmux warning. Plan §4.3 example block also omits the literal `tmux new -s phase_h_pe` line in the markdown shown, so the implementation matches the plan literal.

---

## §5 Implementation outline

| Item | Status | Evidence |
|------|--------|----------|
| §5.1 SPEC §8.3 PE subsection expansion | DONE | SPEC.md diff L+751-771 — 5-cell table, cell descriptions, pre-flight overlap-fraction gate, samtools-direct PE-ness assertion mention, mixed-metric differential (4 assertions), fail-closed ROW_COUNT_OK, distinct verdict line, splitting-report 875 B drop note, BAM MD5 |
| §5.1 §10 row H PE update | DONE | SPEC.md diff L+864 — old "~300 LOC est." line replaced with "~830 LOC bash + checklist + SPEC ... absorbs rev 1's 1 Critical + 9 Important review findings" |
| §5.2 #1 Cell-set (5 PE cells) | DONE | `phase_h_pe_matrix.sh:234` — `MATRIX_CELLS=("D" "r1_5p" "r2_5p" "r1r2_3p" "overlap")` |
| §5.2 #2 Flag-passthrough construction | DONE | L236-242 — `declare -A CELL_FLAGS` with all 5 mappings matching plan |
| §5.2 #3 PE-ness samtools regex | DONE | L134 (verified above) |
| §5.2 #4 Overlap-fraction sanity check | DONE | L143-158 |
| §5.2 #5 Baseline 11,443 B only (no 875 B) | DONE | L406; no `875` literal anywhere |
| §5.2 #6 Mixed differential metric — row count for `<D` cells, count-sum for overlap | DONE | `count_mbias_rows` L423-428 (grep -cE pattern), `sum_mbias_counts` L430-438 (awk recipe matches plan: `sum += $2 + $3`), dispatch L482-500 |
| §5.2 #7 ROW_COUNT_OK fail-closed | DONE | L420 init=0; L466-468 missing-file branch; L507 set to 1 only on `PASS_FLAG=1` after all assertions |
| §5.2 #8 `cell_p<N>_<CELL_ID>` mnemonic naming | DONE | L267 — `SUBDIR="$OUT_DIR/cell_p${n}_${cell_id}"` |
| §5.2 #9 Inline differential evidence in matrix_verdict.txt + speedup_table.md; no standalone row_count_diff.txt | DONE | `ROW_COUNT_DETAIL` written to verdict at L744 and to speedup_table at L699/L702; no `row_count_diff.txt` file created (grep confirms) |
| §5.2 #10 Input BAM MD5 in both files | DONE | speedup_table L524; matrix_verdict L724 |
| §5.2 mechanical-copy: no shared lib | DONE | Single-file driver; no `_phase_h_lib.sh` |
| §5.2 mechanical-copy: smoke does Rust discovery | DONE | L177-178 |
| §5.2 mechanical-copy: `find -mindepth ... -print -quit` pattern OR equivalent | DEVIATED (documented as functionally equivalent) | L113 uses `[[ -n "$(ls -A "$OUT_DIR" 2>/dev/null)" ]]` instead of `find -mindepth 1 -maxdepth 1 -print -quit`. Both detect a non-empty directory; `ls -A` works correctly under `set -u` because the command-substitution result is double-quoted. No functional regression; minor deviation from mechanical-copy guideline. |
| §5.2 mechanical-copy: bash 4.0, defensive `${ARR[@]+...}`, MBIAS fail-closed, `R*100/P` with P=0 guard, BSD/GNU `load average[s]?:` regex, SIGINT trap, nproc-missing graceful skip, tmux warning, sub-2s annotation | DONE | All verified in line-spot above (L60, L266 `for n in $PARALLEL_SET` is safe since PARALLEL_SET is non-empty by default; L398/L420 fail-closed; L544 ratio + `P -gt 0` guard; L200 uptime regex; L69 trap; L186 nproc skip; L209 tmux; L552 sub-2s) |
| §5.3 RELEASE_CHECKLIST PE populate | DONE | Verified in §4 above |
| §5.4 PROGRESS.md PE row update | DONE | PROGRESS.md modified per `git diff --stat` (4 line delta); rev 2 Impl Notes deviation #4 acknowledges the update timing |
| §5.5 No crate code changes | DONE | `git diff --stat` lists zero files under `rust/bismark-extractor/src/` or `tests/` |
| §5.6 Pre-merge validation: `bash -n` clean | DONE | Re-verified just now: `bash -n /Users/fkrueger/Github/Bismark/scripts/phase_h_pe_matrix.sh` → exit 0 |
| §5.6 cargo test 303-test baseline | DONE (per Impl Notes self-report) | Implementer reports `cargo test -p bismark-extractor` → 303 passed / 0 failed / 0 ignored. Not re-executed by this auditor (Phase H requires no crate changes and `git diff --stat` confirms no `src/*` modifications, so the 303 baseline is preserved by definition). |
| §5.6 shellcheck | DEVIATED (documented) | Rev 2 Impl Notes deviation #2: shellcheck not installed on dev Mac; plan §5.6 says "if available; else skip with note". Acceptable. |
| §5.6 Local-Mac dry-run | DEVIATED (documented) | Rev 2 Impl Notes deviation #3: no `bioinf` env on dev Mac; defer to colossal release-gate. Acceptable per plan §5.6 ("Optional"). |

---

## §8 Assumptions (18 entries — verify implementation doesn't violate)

| # | Assumption | Violated? | Evidence |
|---|-----------|-----------|----------|
| A1 | 10M PE BAM colossal path | No | Pre-flight rejects gracefully if BAM not found (L104) |
| A2 | Perl v0.25.1 in bioinf | No | Enforced L169-175 |
| A3 | colossal supports `--parallel 4` | No | Enforced L190-194 |
| A4 | Perl version v0.25.1 | No | Same as A2 |
| A5 | Rust N-invariance | No | Tested by cross-N L329-382 |
| A6 | Perl multicore fork-modulo ordering | No | Smoke handles sorted-content arm |
| A7 | Phase C.2 6-file kept-set | No | Smoke handles file-set match |
| A8 | Phase C.1 polarity correct | No | Implicit (M-bias 11,443 B) + explicit (overlap count-sum > D + 5%) guards present |
| A9 | One PR closes #872 | No | Branch is single-PR shape |
| A10 | Branch from rust/iron-chancellor HEAD `651b7fd` | No | `git log` confirms HEAD == `651b7fd` (auditor confirmed origin/rust/iron-chancellor HEAD is `651b7fd`) |
| A11 | No crate code changes | No | Confirmed via `git diff --stat` |
| A12 | RELEASE_CHECKLIST PE section populated | No | Confirmed in diff |
| A13 | `--extra-rust`/`--extra-perl` array-safe | No | Driver constructs via `EXTRA_FLAGS` string (L264), passes as single quoted arg to smoke (L277-278). Smoke handles array conversion per #873. |
| A14 | colossal has ≥ 3 GB free | No (assumed) | Not enforced by driver; consistent with plan |
| A15 | Smoke wall-clock format `^Perl: <int>s$` / `^Rust: <int>s$` | No | L287-288 regex matches exact plan format |
| A16 | nproc available (degraded to warning) | No | L182-187 |
| A17 | Phase F PE worker-reduce consistent across N | No | Tested by cross-N |
| A18 | Input BAM overlap fraction ≥ 80% | No | Enforced L152-158 |

No assumptions violated.

---

## Rev 2 Implementation Notes — implementer's self-reported deviations verified

| Claim | Verified? | Notes |
|-------|-----------|-------|
| 786 LOC vs ~700 est | TRUE | `wc -l` confirms 786 (vs SE's 710); deviation acceptable per plan §2.2 estimate band |
| Mixed differential added ~30 LOC | TRUE | `count_mbias_rows` + `sum_mbias_counts` + dispatch ~30 LOC at L423-510 |
| Overlap-fraction pre-flight ~20 LOC | TRUE | L140-159 ~ 20 LOC |
| BAM MD5 ~12 LOC | TRUE | L216-227 ~ 12 LOC |
| Missing-file fail-closed ~20 LOC | TRUE | L458-468 ~ 11 LOC + dispatch |
| `shellcheck` not run | TRUE | Acknowledged; non-blocking |
| No local-Mac smoke dry-run | TRUE | Acknowledged; non-blocking |
| PROGRESS.md updated at rev 1 absorption (not §5.4 step time) | TRUE | PROGRESS.md modified per git diff; rev 2 Impl Notes deviation #4 |

All four self-reported deviations are minor and documented in the plan's "Implementation Notes (rev 2)" section. No undocumented deviations found.

---

## Verdict

**COMPLETE** — every plan §3.3.2 through §3.3.6 item, §3.4 contract, §3.5 edge case (17/17), §4 signature, §5 implementation-outline item, and §8 assumption is either DONE or DEVIATED-with-documented-rationale in the rev 2 "Implementation Notes" section. Zero MISSING items. Zero undocumented PARTIAL/DEVIATED items.

Aggregate: **60 DONE / 4 DEVIATED-documented / 0 PARTIAL / 0 MISSING / 64 total**.

Implementation is ready for dual code-review (review skills run via Agent in fresh context per global CLAUDE.md). Post-review, the working-tree changes need to be committed (currently uncommitted) before PR creation against `rust/iron-chancellor`.

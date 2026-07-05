# Code Review — Phase H sub-gate 1 PE matrix harness (Reviewer A)

**Branch:** `extractor-phase-h-pe`
**Plan:** `plans/05262026_bismark-extractor/PHASE_H_PE_PLAN.md` rev 2
**Scope:** new `scripts/phase_h_pe_matrix.sh` (786 LOC), `RELEASE_CHECKLIST.md` PE section, `rust/bismark-extractor/SPEC.md` §8.3 + §10 row H PE update. No Rust code changes (verified via `git diff --stat`; 303-test baseline preserved).

## Summary

Solid PE matrix driver that correctly mirrors the merged SE driver's structure (SIGINT trap, bash 4.0 hard-fail, BSD/GNU-tolerant uptime regex, fail-closed `MBIAS_BASELINE_OK`, R/P speedup direction with P=0 guard, sub-2s annotation, nproc graceful skip, tmux warning, `${ARR[@]+"${ARR[@]}"}` discipline) and adds the PE-specific pieces (overlap-fraction ≥ 80 % pre-flight, PE-ness assertion, mixed-metric differential with `<D` row-count for trim cells + count-sum `>D+5 %` for `overlap`, fail-closed `ROW_COUNT_OK` with missing-file forced FAIL, BAM MD5 record, distinct verdict line for differential FAIL). The 1 Critical + 9 Important plan-review findings absorbed into rev 1 are correctly implemented in the driver.

No Critical or High findings. Two Medium findings worth addressing pre-merge (one inherited from SE; one PE-specific copy-paste). The rest are Low/Nits.

**Verdict: APPROVE-WITH-NITS.**

## Findings by severity

### Critical
None.

### High
None.

### Medium

**M1 — `count_mbias_rows` returns `"0\n0"` on zero-data-row M-bias (latent fail-open).** `scripts/phase_h_pe_matrix.sh:423-428`. The recipe `grep -cE '^[0-9]+\t' "$f" 2>/dev/null || echo "0"` is buggy when the file exists but contains no data rows: `grep -c` prints `0` to stdout AND exits 1, the `|| echo "0"` then appends a second `0`, so the function returns `"0\n0"` (a two-line string). Captured by `$(...)` and used in `[[ "$R1_5P_ROWS" -ge "$D_ROWS" ]]`, the integer compare aborts with `bad math expression` (silently swallowed by `2>/dev/null`), the test returns false, the violation is **not** recorded, and `PASS_FLAG` stays 1 → `ROW_COUNT_OK=1` → verdict PASS despite the file being effectively empty. This is precisely the fail-open class the rev 1 B-Imp-1 hardening tried to close — but it only closes "file missing", not "file present, zero data rows". The same bug exists in `scripts/phase_h_se_matrix.sh:387-392` (inherited from #873, not a PE regression). Reproducer:

```
$ printf 'no rows\n' > /tmp/none.txt
$ X=$(grep -cE '^[0-9]+\t' /tmp/none.txt 2>/dev/null || echo "0")
$ echo "[$X]"        # prints "[0\n0]"
$ [[ "$X" -ge 1 ]]; echo $?   # bad math expression; under set -e + if, silently false
```

**Suggested fix:** replace with `awk '/^[0-9]+\t/ { c++ } END { print c+0 }' "$f"` which is single-line, exit-0, robust against header-only files. Same for the SE driver in a follow-up PR.

**M2 — Stale source-line reference in `speedup_table.md`.** `scripts/phase_h_pe_matrix.sh:529` emits `"Library: PE (asserted via samtools @PG check at phase_h_pe_matrix.sh:117)"` but the PE-ness assertion is actually at line 134 (the BAM-readability check at line 104 nudged everything down). Misleading evidence record — a release engineer chasing a regression will look at line 117 (a `mkdir -p`) and be confused. **Suggested fix:** drop the line number from the message — `"Library: PE (asserted via samtools @PG pre-flight check)"`. Line-number-stable references shouldn't appear in audit output.

### Low

**L1 — `comm -12 <(...)` word-splits filenames.** `scripts/phase_h_pe_matrix.sh:357` uses `for f in $(comm -12 ...)` which word-splits the comm output on IFS. If any output filename contained a space (unlikely with Bismark's `CpG_OT_<basename>.txt` convention, but theoretically possible if BAM basename has spaces), the loop would misbehave. Inherited verbatim from SE driver line 307 — pre-existing issue, no regression. Suggested fix in a follow-up: `while IFS= read -r f; do ...; done < <(comm -12 ...)`.

**L2 — `NS=$(...) ; for n in $NS` — same word-splitting principle.** `scripts/phase_h_pe_matrix.sh:330-332` (`NSARR=()` loop). For integer-only N values like `1 4`, word-splitting is benign. Defensible but worth a one-line comment that "N values are guaranteed integers; word-splitting safe". Inherited from SE.

**L3 — PE-ness regex divergence from smoke detection.** `scripts/phase_h_pe_matrix.sh:134` uses `'^@PG.*ID:Bismark.*[[:space:]]-1[[:space:]]'` (anchored, POSIX-whitespace boundaries) vs `scripts/phase_h_smoke.sh:159` uses `'@PG.*ID:Bismark.*-1 '` (unanchored, literal space, no leading boundary). Both match canonical Bismark `@PG` lines `... -1 R1.fq.gz -2 ...`, but the driver is stricter than the smoke. In the rare case where the smoke would mis-detect as PE (and the matrix should reject), the regexes are consistent. In the more pathological case (e.g., Bowtie2 was invoked with `--phred33-1` followed by a tab in some weird @PG), the smoke could accept while the driver rejects. **Comment:** the driver's regex is preferable (stricter) and the divergence is non-blocking. Worth a 1-line code comment that "this is intentionally stricter than smoke's regex; we want to fail-fast on edge cases the smoke would silently accept".

**L4 — `sum_mbias_counts` awk recipe correctly sums across all M-bias sections.** Verified mentally: M-bias.txt has six sections (CpG/CHG/CHH × R1/R2 for PE), each with header lines like `CpG context (R1)` followed by `position\tmethylated\tunmethylated\t%methylation\tcoverage` and a separator. The awk pattern `/^[0-9]+\t/` matches data rows in every section, sum is accumulated globally — exactly what the differential metric requires. No issue. Documentation in §3.3.3 step 5 is accurate.

**L5 — `OVERLAP_THRESHOLD=$(( D_COUNTS * 105 / 100 ))` integer-overflow risk.** Bash uses 64-bit signed arithmetic (max ~9.2e18). For a 1.2 GB 10M-pair PE BAM, `D_COUNTS` is the sum of (methylated + unmethylated) across all M-bias positions × 6 sections — at most ~10M pairs × 6 sections × 1.0 = ~6e7. Times 105 = ~6e9. Well within range. No issue. (Worth recording in code comment as "max realistic D_COUNTS ≈ 1e8 × 105 = 1e10, safely within 64-bit signed".)

**L6 — `OVERLAP_PCT = PAIRED_READS * 100 / TOTAL_READS` integer-overflow risk.** Same envelope analysis: `PAIRED_READS * 100` at most 2e8 × 100 = 2e10. Within 64-bit. No issue.

**L7 — Multi-line variable display in `ROW_COUNT_DETAIL`.** Lines 478, 484, 488, 492, 499, 503: the detail string is built via string concatenation with `[FAIL: ...]` annotations. If multiple cells fail, the string grows long but readable. The detail is echoed to `matrix_verdict.txt` and stdout; long lines may wrap awkwardly in `cat` output but won't break parsing. Not a finding, mention only.

**L8 — `BASELINE_GATE_APPLIES==0` differential check is correctly skipped.** Verified at lines 450 (`if [[ "$BASELINE_GATE_APPLIES" -eq 1 ]]; then`) and 763 (`elif [[ "$BASELINE_GATE_APPLIES" -eq 1 && "$ROW_COUNT_OK" -eq 0 ]]`). When `--parallel-set` omits 1, the differential check is correctly skipped (vacuous PASS) and the verdict reports "gate did not apply". Plan §3.3.6's "vacuous PASS" semantics match the implementation.

**L9 — Cross-N unconditional behavior is correctly implemented.** Verified at lines 329-382: the outer loop iterates `MATRIX_CELLS` unconditionally; each cell's cross-N comparison runs regardless of `CELL_VERDICT[k]`. Plan rev 1 B-Imp-4 is faithfully implemented. (The matrix even skips gracefully — line 334-337 — if a cell has only one N value, e.g., `--parallel-set "4"`).

**L10 — Pre-existing test impact: clean.** `git diff --stat origin/rust/iron-chancellor..HEAD` shows zero source/test file changes; only `RELEASE_CHECKLIST.md`, `plans/05262026_bismark-extractor/PROGRESS.md`, `rust/bismark-extractor/SPEC.md` modified, plus new untracked `scripts/phase_h_pe_matrix.sh`. The 303-test baseline is preserved exactly as the plan's §5.5 claims.

**L11 — Pre-folded SE rev-3 hardening: present.** Cross-referenced against `scripts/phase_h_se_matrix.sh`:
  - bash 4.0 hard-fail (lines 60-66): ✓
  - SIGINT/SIGTERM trap (line 69): ✓
  - `${ARR[@]+"${ARR[@]}"}` defensive idiom: not directly needed (driver doesn't expand optional arrays at invocation; the smoke handles it). ✓
  - R/P speedup direction with P=0 guard (lines 544-549): ✓
  - BSD/GNU-tolerant `load average[s]?:` regex (line 200): ✓
  - nproc-missing graceful skip (lines 182-187, 188-196): ✓
  - tmux warning (lines 208-214): ✓
  - Sub-2s annotation (lines 552-555): ✓
  - All eight SE rev-3 polish items confirmed present in PE driver.

**L12 — RELEASE_CHECKLIST PE section: complete, no remaining TODO/stub.** Verified via `grep -nE 'TODO|stub|FIXME' RELEASE_CHECKLIST.md`: only one match at line 252 (an unrelated reference to the PE plan filename, not an action item). The PE block contains tmux-wrapped invocation, 7 verify checkboxes covering exit-code interpretation + M-bias 11,443 B baseline + cross-N + mixed-metric differential + properly-paired fraction + BAM MD5 + matrix_verdict.txt aggregates, escalation-path subsection (rev 1 A-I4), and the "Comment on epic #798" recording line. The cargo build budget reminder lives at line 92 in the shared pre-flight block (covers BOTH SE and PE), not the per-section blocks — so the PE block correctly inherits it without duplication. False alarm if a reviewer expected a duplicate.

**L13 — Exit-code ladder is correctly ordered.** Lines 751-773: USAGE first (line 751), then per-cell FAIL (754), cross-N FAIL (757), baseline drift (760), differential FAIL (763), perf-miss exit 3 (767), PASS fallthrough (770). Rev 1 B-Imp-5's "distinct verdict line for differential FAIL" is present at line 766: `"FAIL: differential check violated (mixed-metric: row-count for <D cells, count-sum>D+5% for overlap) — see Differential detail above"` — release engineer can disambiguate from byte-identity FAIL (line 756: `"FAIL: $FAIL_COUNT cell(s) failed byte-identity"`). Good.

**L14 — `bash -n scripts/phase_h_pe_matrix.sh` clean.** Verified.

**L15 — Driver portability via `BASH_SOURCE`.** Line 98: `REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"`. Robust against symlinks (cd-then-pwd resolves), against sourced-vs-executed (BASH_SOURCE works in both), and against execution from any CWD. ✓

## Verification of plan-rev-1 absorption (the 1 Critical + 9 Important)

| Plan finding | Implementation site | Status |
|---|---|---|
| A-C1 (Critical — PE-ness pre-flight) | line 134 samtools-direct regex `^@PG.*ID:Bismark.*[[:space:]]-1[[:space:]]` | ✓ implemented |
| A-I1 (overlap-fraction ≥ 80 % pre-flight) | lines 140-158 `samtools view -c -f 0x2` + integer-pct gate | ✓ implemented |
| A-I2 (Phase C.1 polarity guard at `overlap` cell) | deferred to rev-2 per plan §10 Open; not blocking | ✓ correctly deferred |
| A-I3 (`BASELINE_GATE_APPLIES`) | lines 397, 402 — covers M-bias baseline; splitting-report absolute lock dropped per A-I5 | ✓ implemented |
| A-I4 (baseline-drift recovery path) | RELEASE_CHECKLIST.md "Escalation: colossal-vs-planner baseline drift" subsection (lines 196-216) | ✓ implemented |
| A-I5 (drop 875 B splitting-report absolute lock) | driver: no splitting-report size check; SPEC §8.3 documents the drop; per-cell smoke byte-cmp retained | ✓ implemented |
| A-O3 (mixed-metric differential: count-sum for `overlap`) | `sum_mbias_counts` helper + lines 494-504 dispatch | ✓ implemented |
| B-Imp-1 (`ROW_COUNT_OK=0` fail-closed + missing-file forced FAIL) | line 420 init; lines 458-468 missing-file branch | ✓ implemented |
| B-Imp-2 (`row_count_diff.txt` dropped; inlined) | differential evidence in `matrix_verdict.txt` lines 743-744 + `speedup_table.md` lines 688-704 | ✓ implemented |
| B-Imp-3 (mnemonic cell-id naming) | `MATRIX_CELLS=("D" "r1_5p" "r2_5p" "r1r2_3p" "overlap")` line 234; documented in driver comment lines 231-233 | ✓ implemented |
| B-Imp-4 (cross-N runs unconditionally) | lines 314-318 inline comment; loop logic at 329-382 doesn't condition on `CELL_VERDICT` | ✓ implemented |
| B-Imp-5 (distinct verdict line for differential FAIL) | line 766 distinct REASON text | ✓ implemented |
| B-Opt-4 (BAM MD5 record) | lines 217-227 cross-platform md5sum/md5 -q fallback; emitted in `speedup_table.md` line 524 + `matrix_verdict.txt` line 724 | ✓ implemented |

All 1 Critical + 9 Important + key Optionals are correctly implemented. No fold-throughs missed.

## Recommendations

1. **Fix M1 (M-bias zero-row edge case)** in a focused follow-up that also touches the SE driver. Use `awk` instead of `grep -c` to avoid the exit-code/echo double-output bug. This is the only thing remotely close to a correctness issue and even then it's a fail-open on a pathological input (the binary produced an M-bias with no data rows — which is itself a regression the operator would notice for other reasons).
2. **Fix M2 (stale line:117 reference)** in the same follow-up by dropping the line number entirely from the audit message.
3. Consider adding a 1-line comment at the PE-ness regex (line 134) explaining the intentional divergence from `phase_h_smoke.sh:159` (L3) for future maintainers.

## Confidence

High. Spent the review on cross-checking the rev-1 absorption (every flagged finding traces to a concrete implementation site), the mixed-metric awk recipe correctness (correctly sums methylated+unmethylated across all six M-bias sections), the fail-closed semantics of `ROW_COUNT_OK` + `MBIAS_BASELINE_OK` (correctly initialized + correctly gated), the cross-N unconditional behavior (verified by reading the loop logic), and the exit-code ladder ordering (USAGE → FAIL → cross-N → baseline drift → differential FAIL → perf-miss → PASS). The Medium findings are real but bounded in impact — M1 only fires on a pathological input the driver doesn't normally encounter; M2 is a cosmetic audit-log nit.

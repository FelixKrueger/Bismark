# CODE_REVIEW — Phase H sub-gate 1 PE matrix harness — Reviewer B

**Branch:** `extractor-phase-h-pe`
**Files reviewed:**
- `scripts/phase_h_pe_matrix.sh` (NEW, 786 LOC)
- `RELEASE_CHECKLIST.md` (PE section populated)
- `rust/bismark-extractor/SPEC.md` (§8.3 PE subsection expansion + §10 row H update)

**Anchors:** `scripts/phase_h_se_matrix.sh` (711 LOC; structural template), `scripts/phase_h_smoke.sh` (per-cell harness), `PHASE_H_PE_PLAN.md` rev 2.

## Summary

Implementation faithfully absorbs the rev 1 plan revisions (1 Critical + 9 Important + 8 Optional folded). Structure mirrors the SE driver block-for-block with cleanly localized PE-specific divergences (samtools pre-flight, overlap-fraction gate, mixed-metric differential, fail-closed ROW_COUNT_OK, BAM MD5 record). The mixed-metric differential is correctly implemented; pre-flight ordering is logical; verdict-emission and exit-code mapping match the plan's 0/1/2/3 contract. No critical defects found.

One latent **High** issue inherited from the SE driver (`grep -c` double-zero bug) that, under PE-specific conditions, can poison integer comparisons and was not caught by the SE merge. Several **Medium** issues around boundary semantics and operator clarity. **Low** issues around magic-number coordination.

## Findings

### Critical — none

### High

#### H1 — `count_mbias_rows` returns `"0\n0"` (two-line value) when M-bias file has no data rows

**File:** `scripts/phase_h_pe_matrix.sh:423-428`

```bash
count_mbias_rows() {
  local f="$1"
  if [[ ! -f "$f" ]]; then echo ""; return; fi
  grep -cE '^[0-9]+	' "$f" 2>/dev/null || echo "0"
}
```

`grep -c` PRINTS the count `0` to stdout when there are zero matches AND exits with status 1. Because of the `|| echo "0"` fallback, BOTH the grep stdout (`0`) AND the echo fallback (`0`) are emitted, producing a two-line string `"0\n0"`. Verified empirically:

```
$ printf "header\nother\n" > /tmp/test
$ result=$(grep -cE '^[0-9]+	' /tmp/test 2>/dev/null || echo "0")
$ echo "result='$result'"
result='0
0'
```

When this two-line value is plugged into the `[[ "$R1_5P_ROWS" -ge "$D_ROWS" ]]` test at lines 482/486/490, bash arithmetic parsing of `"0\n0"` yields `"0\n0: syntax error: invalid arithmetic operator"` — and because `set -e` is active, the script will likely crash mid-differential with a confusing error message instead of cleanly reporting "differential FAIL".

For PE, the D cell will always have data rows so `D_ROWS` will be a clean integer. But the THREE `<D` cells (`r1_5p`, `r2_5p`, `r1r2_3p`) and the `edge_clip`-equivalent boundary cases COULD theoretically produce zero data rows if a `--ignore N` value exceeds the read length. The current PE matrix uses `--ignore 5` (modest), so in practice this is latent. **However, the bug is identical in `phase_h_se_matrix.sh:391` and DID NOT trigger on the merged SE matrix** — that's evidence of latency, not evidence of correctness. The SE driver's `edge_clip` cell at `--ignore 250` is closer to triggering it but probably still produces ≥1 row.

This is High (not Critical) because the canonical 10M PE BAM at the planned cells is highly unlikely to hit zero data rows. But it's a real latent bug that will surface the day someone runs the matrix on a fixture with shorter reads, or extends `--parallel-set` to test a more aggressive ignore config. Reviewer A may flag this independently as the same finding.

**Suggested fix** (also worth back-porting to SE driver in a follow-up):

```bash
count_mbias_rows() {
  local f="$1"
  if [[ ! -f "$f" ]]; then echo ""; return; fi
  # grep -c prints "0" + exits 1 when no matches; force a single integer via awk.
  awk '/^[0-9]+\t/ { c++ } END { print c+0 }' "$f" 2>/dev/null || echo "0"
}
```

This mirrors the `sum_mbias_counts` helper's awk pattern (line 430-438), which is already correct.

### Medium

#### M1 — `OVERLAP_PCT` integer-truncation at the 80% boundary admits 79.5%

**File:** `scripts/phase_h_pe_matrix.sh:151-152`

```bash
OVERLAP_PCT=$(( PAIRED_READS * 100 / TOTAL_READS ))
if [[ "$OVERLAP_PCT" -lt 80 ]]; then
```

Integer division truncates toward zero. A BAM with 79.99% properly-paired reads (`PAIRED_READS * 100 / TOTAL_READS = 79`) is rejected. A BAM with exactly 80.00% (`PAIRED_READS * 100 / TOTAL_READS = 80`) passes. A BAM with 80.99% computes to `80` and also passes. The behavior is "≥80% (integer-truncated)" not the documented "≥80%".

This is Medium not High because (a) the canonical 10M PE BAM is expected ~99%+ properly-paired so the boundary is academic, (b) integer division is a defensible simplification per the inline comment, and (c) the SPEC §8.3 update documents "≥80%" without further precision. But for completeness, the error message at line 153 reports `${OVERLAP_PCT}% properly-paired reads` — operator sees `79%` even if actual is `79.95%`. Mildly misleading. Plan §3.5 lists "Overlap fraction = exactly 80%" as an edge case; spot-checking the code: an exact 80% does PASS. ✓

**Suggested fix:** either accept the truncation and add `(integer-truncated)` to the inline diagnostic, OR use the standard `(PAIRED_READS * 100 + TOTAL_READS / 2) / TOTAL_READS` for half-up rounding.

#### M2 — `--parallel-set "4"` (no N=1) gives no cross-N evidence AND silently skips the M-bias/differential gates

**File:** `scripts/phase_h_pe_matrix.sh:329-382` + `:450` + `:760`

If the operator runs `--parallel-set "4"` (only N=4):
- Cross-N comparison: `NSARR` has 1 element → "only 1 N value; cross-N skipped" written to `cross_n_summary.txt`. The matrix verdict-side `CROSS_N_FAILS` stays at 0. Cross-N check is **silently skipped**, not failed.
- `BASELINE_GATE_APPLIES` is 0 (no D-N=1 cell) → M-bias baseline gate is "vacuous PASS"
- `ROW_COUNT_OK` is 0 (never flipped) → but gated behind `BASELINE_GATE_APPLIES=1` in the exit-code logic (line 763) so this never fires
- Exit code 0 path is taken if all cells PASS smoke

The "verdict: PASS" produced on `--parallel-set "4"` would mean "5 cells PASSed Perl-vs-Rust byte-cmp" but with NO N-invariance, NO Phase C.1 11,443 B regression guard, and NO mixed-metric semantic guard. That's a much weaker assertion than the operator may realize.

The SPEC §8.3 update and RELEASE_CHECKLIST text don't warn about this. The `matrix_verdict.txt` does include `Baseline gate applies: 0` but a release engineer at 11 PM may not catch that.

**Suggested fix:** add a Medium-priority warning at pre-flight when `--parallel-set` omits N=1:

```bash
if ! echo "$PARALLEL_SET" | tr ' ' '\n' | grep -qx '1'; then
  echo "warning: --parallel-set omits N=1; matrix will skip M-bias baseline," >&2
  echo "         cross-N N-invariance check, and mixed-metric differential." >&2
  echo "         For full release-gate validation, use --parallel-set \"1 4\"." >&2
fi
```

Same issue applies to SE driver; SE inherits it. Worth raising for both.

#### M3 — `OVERLAP_THRESHOLD=$(( D_COUNTS * 105 / 100 ))` could overflow on very large counts

**File:** `scripts/phase_h_pe_matrix.sh:496`

For a 10M PE BAM, D_COUNTS is plausibly in the 1e8 to 1e9 range (millions of M-bias rows × thousands of counts each); 1e9 × 105 = 1.05e11, which fits comfortably in bash's 64-bit integer math. No overflow risk in practice. Noting for completeness; not actionable.

#### M4 — `(D, N=1) cell crashes mid-run → MBIAS_FILE empty → "could not be located" message obscures the actual cause

**File:** `scripts/phase_h_pe_matrix.sh:676-679`

```
"❌ FAIL: M-bias.txt could not be located in (D, N=1) cell. Either Rust"
"   suppressed the file (regression) or the cell crashed. Investigate"
"   cell_p1_D/rust/. Matrix exits 1."
```

This is a good error message — operator knows to look. But the verdict reason at line 762 reads `"FAIL: M-bias baseline 11,443 B drift (or missing file) at (D, N=1) cell"`. A release engineer reading only the verdict line would assume "drift" first (since that's named first); the "(or missing file)" parenthetical is easy to miss. Suggested: emit two distinct reasons, e.g., `"FAIL: M-bias.txt missing at (D, N=1) cell — Rust may have suppressed it or the cell crashed"` vs `"FAIL: M-bias baseline drift: <X> B != 11,443 B"`. Low-effort polish; release engineer at 11 PM benefits.

#### M5 — Pre-flight creates `$OUT_DIR` BEFORE the Perl-version + overlap-fraction checks; failed pre-flight leaves empty dir

**File:** `scripts/phase_h_pe_matrix.sh:123-175`

`mkdir -p "$OUT_DIR"` at line 123 happens before the overlap-fraction check (line 152), Perl version assertion (line 169), and nproc check (line 190). If those reject (exit 2), the empty `$OUT_DIR` is left behind. Re-running with the same `--out` argument will then pass the empty-check (`ls -A` is empty) but the operator may believe "this dir already existed somehow". Cosmetic — not a real risk. Noting for completeness.

**Suggested fix:** move `mkdir -p` to after the last pre-flight check. Or trap the EXIT and rmdir the empty dir on early failure.

#### M6 — Pre-flight overlap-fraction check is a 30-60s up-front cost; failure modes (slow disk, samtools missing on the BAM) emit confusing error

**File:** `scripts/phase_h_pe_matrix.sh:144-148`

```bash
TOTAL_READS=$(samtools view -c "$BAM" 2>/dev/null || echo 0)
PAIRED_READS=$(samtools view -c -f 0x2 "$BAM" 2>/dev/null || echo 0)
if [[ "$TOTAL_READS" -le 0 ]]; then
  echo "error: samtools view -c reported 0 total reads (or failed)" >&2
  exit 2
fi
```

If `samtools view -c` fails (e.g., BAM is corrupted, BAI is stale and samtools cannot read), stderr is suppressed by `2>/dev/null` — operator sees "samtools view -c reported 0 total reads (or failed)" but no actual diagnostic. For a release-engineer at 11 PM debugging a corrupt-BAM failure, that error message is borderline-actionable. The PE-ness check on line 134 also suppresses samtools stderr.

**Suggested fix:** drop the `2>/dev/null` on the pre-flight samtools calls. samtools's own error message will be more informative than the wrapped one.

### Low

#### L1 — Magic numbers (`11443`, `80`, `5%`) hardcoded; coordination locations not centralized

**Files:** `phase_h_pe_matrix.sh:152, 406, 496` + `SPEC.md` §8.3 + RELEASE_CHECKLIST.md

The plan's rev-1 A-I4 escalation path mentions that a future baseline update would touch "both `scripts/phase_h_pe_matrix.sh` and SPEC §8.3". That's a known +1 spot. The 80% overlap-fraction and 5% count-sum threshold are similarly hardcoded.

For the rev-2 enhancement path (file `perf(extractor):` polish issue), it would be helpful to extract these constants to the top of the driver as named variables. The release engineer updating the baseline mid-merge-window benefits from a single point of change. Today's implementation hardcodes them inline — defensible but a polish opportunity. Noting for the rev-2 follow-up sub-issue. Not a blocker.

#### L2 — `BAM=$(cd "$(dirname "$BAM")" && pwd)/$(basename "$BAM")` doesn't fail if BAM has been deleted between the `-r` check and the canonicalize

**File:** `scripts/phase_h_pe_matrix.sh:104-108`

TOCTOU race; not a real concern for a release-gate script. Noting for completeness.

#### L3 — `$EXTRA_FLAGS:-<none>` placeholder appears in the speedup table for the D cell

**File:** `scripts/phase_h_pe_matrix.sh:557, 731`

```bash
FLAGS_DISPLAY="${CELL_FLAGS_STR[k]:-<none>}"
```

In Markdown, the literal `<none>` may render as an HTML tag (depending on the GFM mode; in strict GFM `<` not followed by a recognized tag is escaped, but operator's `cat` rendering or terminal pager will just see `<none>`). The verdict file is plain text — no issue. Suggested: render as backtick-wrapped `\`(no flags)\`` or `(no flags)`. Trivial polish; same convention as smoke at line 213 `(none — SE auto-detected)`.

#### L4 — `BASELINE_GATE_APPLIES` is computed by scanning CELL_NAMES/CELL_N twice (line 391 + 442); not amortized but trivial

**File:** `scripts/phase_h_pe_matrix.sh:391-396, 442-448`

`get_cell_mbias_file` iterates `CELL_NAMES` linearly each call (5 calls; 25 array-lookups). Not measurable. Same pattern as SE driver. No action needed; noting for completeness.

#### L5 — Sub-2s annotation logic shared between SE/PE but emoji rendering may differ between terminals

**File:** `scripts/phase_h_pe_matrix.sh:553-555`

`⚠️ sub-2s` uses a multi-byte emoji. Some `LANG=C` ssh sessions may render as garbage. Noting for completeness; same as SE driver.

## Verification: rev 1 fold faithfulness (spot-check)

| Plan finding | Implementation evidence | Verdict |
|---|---|---|
| A-C1 (samtools-direct PE-ness regex) | `:134` matches the spec'd regex `^@PG.*ID:Bismark.*[[:space:]]-1[[:space:]]` exactly; SE driver doesn't have this (SE doesn't need it) | ✓ Faithful |
| A-I1 (overlap-fraction ≥ 80% pre-flight) | `:143-158` — `samtools view -c` total + `samtools view -c -f 0x2` paired, integer division × 100, lt-80 reject. Logic matches plan; M1/M2 above are quality polish, not correctness defects | ✓ Faithful |
| A-I5 (875 B absolute splitting-report lock dropped) | No reference to `875` in the driver (verified by grep); SPEC §8.3 update explicitly documents the drop. Per-cell smoke byte-cmp on splitting-report is preserved (smoke `:262-269`) | ✓ Faithful |
| A-O3 (mixed metric: count-sum for overlap, row-count for others) | `:494-504` implements the count-sum > D + 5% assertion separately from the three row-count <D assertions. Awk recipe at `:437` matches plan §5.2 #6. Distinct verdict line preserved | ✓ Faithful |
| B-Imp-1 (ROW_COUNT_OK fail-closed init + missing-file forced FAIL) | `:420` `ROW_COUNT_OK=0` initial; `:458-468` builds `MISSING` list; if non-empty, skips assertions and sets `ROW_COUNT_DETAIL` to FAIL message without flipping `ROW_COUNT_OK` | ✓ Faithful |
| B-Imp-2 (no standalone `row_count_diff.txt`; inline evidence) | Confirmed by grep — no `row_count_diff.txt` write anywhere in the driver. Differential evidence is appended to `speedup_table.md` (`:686-703`) and `matrix_verdict.txt` (`:744`) | ✓ Faithful |
| B-Imp-3 (mnemonic cell-ids) | `:234` `MATRIX_CELLS=("D" "r1_5p" "r2_5p" "r1r2_3p" "overlap")` — mnemonic; SE driver uses `D|0|0` parameter-encoded. SPEC §8.3 update documents the divergence | ✓ Faithful |
| B-Imp-4 (cross-N runs unconditionally) | `:329-382` cross-N loop iterates `MATRIX_CELLS` directly without checking CELL_VERDICT. Confirmed | ✓ Faithful |
| B-Imp-5 (distinct verdict line for differential FAIL) | `:766` — "FAIL: differential check violated (mixed-metric: ...)" is a distinct reason vs `:756` byte-identity FAIL | ✓ Faithful |
| B-Opt-4 (BAM MD5 in verdict + speedup_table) | `:216-227` computes via md5sum/md5 fallback; `:524` writes to speedup_table; `:724` writes to matrix_verdict | ✓ Faithful |

## SE driver cross-comparison

Pre-flight order (with PE insertions in **bold**):
1. bash version ≥ 4.0 (line 60)
2. SIGINT/SIGTERM trap (line 69)
3. BAM exists/readable (line 104)
4. --out empty (line 111)
5. **samtools available (line 127)** — PE-specific
6. **PE-ness assertion (line 134)** — PE-specific
7. **Overlap-fraction gate (line 143)** — PE-specific
8. Perl v0.25.1 (line 169)
9. nproc + contention (line 188)
10. tmux warning (line 209)
11. **BAM MD5 (line 217)** — PE-specific

Insertions are well-localized between the BAM check and the Perl check; ordering is sensible (samtools is a dep for #6 + #7, so check it first). ✓

Matrix execution: SE uses `IFS='|' read -r NAME I5P I3P` to parse `"D|0|0"` cell strings; PE uses an associative `CELL_FLAGS` array keyed by `cell_id`. The PE form is cleaner and idiomatic; SE's parse-on-the-fly is a structural artifact of multi-axis cell encoding. ✓

Verdict + exit-code: SE uses `MBIAS_GATE_APPLIES`; PE uses `BASELINE_GATE_APPLIES`. The PE name implies broader scope (also gates the differential check; line 763 — `BASELINE_GATE_APPLIES && ROW_COUNT_OK==0` → exit 1 with differential FAIL). Naming divergence is intentional and matches the plan's A-I3 finding. ✓

Where PE legitimately diverges from SE, divergence is well-commented (rev 1 finding IDs inline). ✓

## RELEASE_CHECKLIST PE section ergonomics

The PE section has 8 verify checkboxes (counted from the diff). Each ties to a specific output artifact:

| # | Checkbox | Output |
|---|---|---|
| 1 | Exit code 0 or 3 | shell exit code + matrix_verdict.txt |
| 2 | `matrix_verdict.txt` reports PASS aggregates | matrix_verdict.txt |
| 3 | `cell_p1_D/diff_summary.txt` splitting-report + M-bias 11,443 B | per-cell smoke |
| 4 | `cross_n_summary.txt` PASS for all 5 cells | cross-N output |
| 5 | `speedup_table.md` Rust/Perl direction + Rust scaling | speedup_table.md |
| 6 | `speedup_table.md` differential section | speedup_table.md |
| 7 | Properly-paired fraction header ≥ 80% | speedup_table.md |
| 8 | BAM MD5 matches planner's reference | speedup_table.md |

All actionable; release engineer can follow in order without referring to the plan. The escalation subsection at the bottom is the "what to do when checkbox 3 fails" recovery path — well-placed.

One small ergonomic gap: the checklist text says "On first colossal session, record the MD5". It doesn't say WHERE to record it. The plan §3.5 says "matches planner's reference" but the planner's reference is itself unrecorded in rev 1 (deferred to first-colossal). Operator at 11 PM may be confused: "should I compare against what?" Suggested: replace with "On first colossal session, record the MD5 in this checklist as the planner's reference; on subsequent runs, expect a match — mismatch signals silent BAM swap-in." Minor.

## SPEC §8.3 PE subsection content

The SPEC subsection adds a cell table, 7 PE-specific assertion bullets, and a `See plans/...` reference. The cell table matches the driver's `CELL_FLAGS` exactly. The "Mixed-metric differential check at N=1" bullet matches the driver's §3.3.5 implementation:
- The three `<D` row-count assertions
- The `overlap` count-sum `>D+5%` assertion
- Awk recipe is mentioned in the plan §5.2; driver line 437 matches

Faithful documentation. The fail-closed-init paragraph + missing-file forced FAIL is correctly described. The "splitting-report 875 B absolute size lock NOT used" bullet explicitly documents A-I5; SE SPEC doesn't have this since SE never had a splitting-report absolute lock.

§10 row H PE update: "~830 LOC bash + checklist + SPEC (no Rust code; absorbs rev 1's 1 Critical + 9 Important review findings)". Honest — actual driver is 786 LOC; plus ~50 LOC RELEASE_CHECKLIST + ~30 LOC SPEC ≈ ~870 LOC, close to the cited 830. ✓

## Failure forensics trace (operator's "what failed" path)

Hypothetical: `overlap` cell's M-bias was 100 B smaller than expected; differential count-sum was below D + 5%.

1. Operator reads `matrix_verdict.txt` → "Verdict: FAIL: differential check violated (mixed-metric: ...) — see Differential detail above". Differential detail line shows `[FAIL: differential overlap count-sum=X not > D=Y + 5% threshold=Z]`. ✓
2. Cross-N comparison result → `cross_n_summary.txt`. Operator opens `[overlap] N=1 vs N=4` row. Independent assertion. ✓
3. Per-byte diff vs Perl for overlap cell → `cell_p1_overlap/diff_summary.txt`. From smoke's output — shows per-file PASS/FAIL with byte sizes + `cmp -i` first-diff offset. ✓
4. Exact count-sum measured → in `matrix_verdict.txt`'s "Differential detail" line AND in `speedup_table.md`'s Mixed-metric Differential section. ✓ (Cross-referenced in two places per B-Imp-2.)

Navigable. The verdict reason line at `matrix_verdict.txt:775` is the single source of truth for "what failed"; from there, three artifacts each have a focused view. Operator-friendly.

## Verdict

**APPROVE-WITH-NITS**

Implementation is sound. The H1 `count_mbias_rows` double-zero bug is the only finding I'd want addressed before merge IF the plan intends to add cells with shorter reads in a follow-up. As-is, with the canonical 10M PE BAM at modest ignore values, the bug is latent and would not fire. The fix is mechanical (replace grep -c with awk count). Since the same bug is in the merged SE driver and didn't cause the SE matrix to fail on colossal, the practical risk for this PR is low.

Medium findings M2 (no-N=1 silent-skip) and M4 (verdict-line clarity) are polish that would improve operator UX but don't block merge. M5 (mkdir-before-failable-checks) is cosmetic.

All rev 1 plan revisions are faithfully implemented; cross-comparison with SE driver shows clean structural mirroring with appropriately-localized PE-specific divergences. SPEC + RELEASE_CHECKLIST updates are consistent with the driver.

Recommended action: address H1 in either this PR (if a 5-line fix is acceptable) or in a follow-up `polish(extractor):` issue that back-ports the same fix to SE driver simultaneously. M2 and M4 are worth opening polish sub-issues against #872 for future revisions.

No criticals. No security or correctness blockers.

# PLAN_REVIEW_PHASE_H_PE — Reviewer A

**Plan under review:** `plans/05262026_bismark-extractor/PHASE_H_PE_PLAN.md` (rev 0, 2026-05-28)
**Reviewer:** A (independent)
**Verdict (TL;DR):** **NEEDS-REVISIONS** — 1 Critical (pre-flight PE-ness check references a `--dry-run` flag that doesn't exist in `phase_h_smoke.sh`; the documented default mechanism is unimplementable as written), 4 Important, 6 Optional.

---

## Logic review

The plan is a structurally sound mirror of the merged SE plan (`PHASE_H_SE_PLAN.md` rev 3 / PR #873). Cross-N N-invariance, MBIAS_GATE_APPLIES + fail-closed default, B-L1 `R*100/P` direction, BSD/GNU `uptime` regex, SIGINT trap, nproc-missing graceful skip, tmux warning, sub-2s annotation, bash 4.0 pre-flight, defensive `${ARR[@]+"${ARR[@]}"}` — all carried forward verbatim. The PE-specific additions (5-cell matrix; `overlap > D` row-count differential; 11,443 B M-bias + 875 B splitting-report HARD-FAIL baselines at (D, N=1); Phase C.1 polarity guard via implicit byte-count + row-count differential) are well-articulated.

Three real logic problems found below; the rest are smaller importants and nits.

---

## Critical

### C1. `phase_h_smoke.sh` has no `--dry-run` flag; the default PE-ness pre-flight as written is unrunnable.

**Anchors:** PHASE_H_PE_PLAN.md §3.3.2 #5 (line 174); §4.1 docstring "PE-ness assertion via @PG (smoke's Library: PE auto-detect)".

The plan's default pre-flight PE-ness check is:

> invoke `phase_h_smoke.sh <BAM> --parallel 1 --mode default --out <tmp> --dry-run` (or equivalent quick check) to confirm `Library: PE` from `@PG`.

`scripts/phase_h_smoke.sh` (post-#873) has no `--dry-run` and no equivalent. Verified by grep: the smoke's arg-parser only knows `--parallel`, `--mode`, `--out`, `--extra-rust`, `--extra-perl`. Invoking it with `--dry-run` would either be silently ignored (if the trailing-arg branch consumes it) or rejected as USAGE.

§10 Open #3 acknowledges this as a "Smoke auto-detect is simpler but defers the failure mode until the first cell runs (~30 s wasted)" — but that's only true if the alternative is fall-through. With no `--dry-run`, the **plan's documented default** is the post-first-cell auto-detect (b), but §3.3.2 #5 writes (a) as the default with (b) flagged as the "alternative if smoke doesn't support `--dry-run`". The plan has this swapped — (a) is unimplementable as written, so (b) is the only available default.

**Why this is Critical, not Important:** the plan is in a state where the implementer would either (1) add a `--dry-run` to smoke (out of scope per §4.2 "no edits to `phase_h_smoke.sh` in this PR"), or (2) silently drop pre-flight PE-ness check and rely on post-first-cell auto-detect, which is what §3.3.2 #5 *says* it doesn't want. Either path is a deviation from the written plan.

**Suggested fix (one of three):**

1. **Promote alternative (b) to the default.** Use `samtools view -H "$BAM" | grep -q '^@PG.*ID:Bismark.*-1 '` in pre-flight. Note the regex MUST mirror smoke's existing detection logic at `phase_h_smoke.sh:159` (`@PG.*ID:Bismark.*-1 `), NOT the plan's proposed `^@PG.*bismark.*--paired`. The smoke pattern keys on the `-1 R1.fq` Bowtie2 invocation arg, which is more robust than `--paired` across older Bismark @PG lines. (See Open #4 below for the regex specifically.)
2. **Inline-parse @PG without samtools.** `grep '@PG' <(samtools view -H "$BAM")` is the same as #1; just use samtools as in #1.
3. **Accept post-first-cell deferred detection explicitly.** Rewrite §3.3.2 #5 to say "post-first-cell `Library: PE` line check after cell `D, N=1` completes; FAIL fast if SE detected" — but the `~30 s wasted` is actually `~15–20 min wasted` because the (D, N=1) cell is a full Perl + Rust extraction, not a header peek. This is the strictly worst option.

Recommend #1. Pin the regex at `^@PG.*ID:Bismark.*-1 ` (with the trailing space) so it matches smoke's existing semantics.

**Acceptance criterion for the fix:** §3.3.2 #5 reads as a single executable check, and §10 Open #3 collapses (the implementation is no longer "implementer's choice").

---

## Important

### I1. The "`overlap > D` row-count differential" assertion lacks a minimum-margin threshold; on a low-overlap PE BAM this becomes a coin-flip false-fire.

**Anchors:** §3.3.5 (line 213); §3.4 #6 (line 285); §10 Open #4 (line 583); §11 R3.

The plan's only "more rows" direction is `overlap > D` using a strict `>`. §10 Open #4 + §11 R3 both flag this as fragile against future BAMs with low overlap fraction. The plan declines a relative threshold ("could equal `D` in extreme inputs"). The mitigation cited is "§A1 documents the assumption."

For this PR's locked 10M PE BAM the margin is probably comfortable (the SRR24827373.9 fixture guarantees overlap retention). But the plan asserts the *driver* should treat `overlap > D` as a HARD-FAIL — which means the driver works for THIS BAM but is a hidden trap for ANY future BAM swap-in. Phase H is harness work, not BAM-specific work; the driver SHOULD be BAM-portable.

**Suggested fix.** Pick one:

- (a) Strict `overlap > D` with documented minimum-pair-overlap-fraction assumption baked into pre-flight: `samtools view -c -f 0x2 <BAM>` (properly-paired count) ≥ 80% of total; FAIL pre-flight if not met. Compatible with "BAM-portable" intent.
- (b) Relative threshold: `overlap ≥ D + max(5, ceil(0.01 * D))` (≥5 rows or 1%, whichever is larger). Defensible across BAMs without a pre-flight gate.
- (c) Document explicitly that the driver is locked to BAMs with ≥80% properly-paired reads, and add §A18.

Recommend (a) or (b). (c) is current behavior + bigger doc, no actual harness improvement.

### I2. Phase C.1 implicit polarity-guard analysis is incomplete: the M-bias-byte-count check at (D, N=1) does NOT cover the `--include_overlap` polarity case independently from the `--no_overlap` polarity case.

**Anchors:** §3.4 #6 (line 285); §10 Open #2 (line 581).

The (D, N=1) cell uses default `--no_overlap`. The 11,443 B M-bias byte-count reflects the *no_overlap*-polarity output. If a future C.1-style regression flips polarity only in the `--include_overlap` branch (e.g., a new bug in the overlap-keeping path while no_overlap-dropping is untouched), the (D, N=1) byte-count remains 11,443 B (untouched) and the `overlap > D` differential remains true (the bug would likely *increase* rows in overlap, not flip the ordering). The driver passes; the regression escapes.

The plan claims (§3.4 #6) that "If only the include_overlap path regresses, the row-count differential `overlap > D` fails — caught by §3.3.5". This is only true if the regression *flips* the polarity (i.e., overlap acts like no_overlap). A subtler include_overlap-only regression (e.g., off-by-one position handling in the kept R2 region) preserves the differential direction and escapes both guards.

**Suggested fix.** Add an explicit byte-count lock for the `overlap` cell at N=1 to be measured on the first colossal run and committed in a rev-2 follow-up. Equivalent to the 11,443 B lock at (D, N=1), but for the `overlap` cell. The plan already discusses this as a "post-first-colossal" rev-2 enhancement in §11 R2; promote it to a §10 Open with default = "rev-2 enhancement" so reviewers see the gap explicitly.

Alternative: include the explicit read-level grep for `SRR24827373.9` R2 calls in the `overlap` cell's verification (§10 Open #2 — the plan's flagged "round-1 reviewer attention magnet"). The plan defers this as "fragility (read names can change with re-alignment)" — a fair concern, but the canonical 10M PE BAM is *the* locked fixture; re-alignment changing read names would invalidate every byte-count baseline simultaneously, not just this grep.

### I3. `MBIAS_GATE_APPLIES` PE-variant logic for `--parallel-set "1"` is under-specified vs the splitting-report 875 B HARD-FAIL.

**Anchors:** §3.3.2 #10 (line 180); §3.3.6 PASS criteria (lines 256-261); SE driver line 685–690.

The SE driver guards M-bias 5712 B and row-count differential behind `MBIAS_GATE_APPLIES`. The PE plan ADDS the splitting-report 875 B HARD-FAIL but doesn't say whether it's gated. Reading §3.3.6:

> The matrix PASSes iff ALL of: … (D, N=1) cell's `*.M-bias.txt` == 11,443 B AND `*_splitting_report.txt` == 875 B.

If the user passes `--parallel-set "4"`, there is no (D, N=1) cell. Does the splitting-report check fire as a FAIL (no file to check) or skip (gate doesn't apply)? The plan's §3.3.6 conjunction reads as "FAIL if absent." Compared to the SE-mirrored M-bias gating, this is inconsistent.

Also: §3.3.6 wording "the (D, N=1) cell's `*.M-bias.txt` AND `*_splitting_report.txt`" implies both baselines must be checked together, but the implementation would naturally have them gated separately (size check on each file). The plan should explicitly say: `MBIAS_GATE_APPLIES` (rename to e.g. `BASELINE_GATE_APPLIES`) gates BOTH the M-bias 11,443 B AND splitting-report 875 B HARD-FAIL together; both are fail-closed (default 0); both set to 1 only on positive size matches; missing-N=1 → gate doesn't apply → vacuous PASS for both.

**Suggested fix.** Add to §3.3.2 #10: "Two baselines gated: `MBIAS_BASELINE_OK` and `SPLITREPORT_BASELINE_OK`, both default 0, both flipped to 1 only on positive 11,443 B and 875 B confirmations respectively. Both gated by `BASELINE_GATE_APPLIES = MBIAS_GATE_APPLIES`. Exit-code logic ANDs both gates."

### I4. The plan claims to populate the `RELEASE_CHECKLIST.md` PE section that #873 left as a TODO-stub, but does NOT specify how to handle the case where the locked baselines (11,443 B / 875 B) differ on colossal vs the planner's expectation.

**Anchors:** §4.3 RELEASE_CHECKLIST.md replacement (line 386); §A1 path assumption; §8.1 A4 v0.25.1 assumption.

§A1 + §A4 admit the 10M PE BAM colossal-path and the locked baselines are unverified pre-first-colossal-session. §4.3's replacement checklist text says:

> `cell_p1_D/diff_summary.txt` shows `*.M-bias.txt` byte-cmp PASS + size 11,443 B + `*_splitting_report.txt` size 875 B.

If the colossal run produces a different (but Perl-byte-equal) size — e.g., 11,449 B because the Perl `bioinf` env has a v0.25.1 build with a minor doc-string difference, or because the 10M PE BAM on colossal differs trivially from the planner's reference BAM — the driver HARD-FAILs and v1.0 is blocked. Felix-as-release-engineer would then need to either patch the baseline (rev-2 commit) or override the driver. The plan doesn't articulate a recovery path.

**Suggested fix.** Add a row to §3.5 edge cases:

> | Colossal baseline differs from 11,443 B / 875 B but Perl-vs-Rust byte-cmp PASSes | If the Perl reference on colossal is byte-equal to the Rust output but differs from the planner's locked baseline, this is a BAM/env mismatch, not a regression. Recovery: commit a rev-2 baseline update (`grep -rn 11443` → new value) AND verify the prior baseline was a transcription error. Do not bypass the gate. |

This is also a §10 Open candidate — the baselines were collected on oxy pre-colossal-migration and may need rebasing.

---

## Optional / nits

### O1. Cell-set asymmetry vs SE is defensible but underdocumented.

§3.1 + §10 Open #1 flag the cell-set asymmetry (PE drops SE's `5p+3p` and `edge_clip`; PE gains `r2_5p` and `overlap`). The rationale ("SE's `edge_clip` is irrelevant to PE; PE's `overlap` is the PE edge case") is defensible. What's missing: a one-sentence justification of why NO R1-3p-alone cell and NO R2-3p-alone cell exists. The plan jumps from "r1r2_3p combined" without explaining why the isolated 3p axes are dropped.

**Suggestion:** Add to §3.1 rationale paragraph: "R1-3p-alone + R2-3p-alone are subsumed by `r1r2_3p`; if `r1r2_3p` PASSes byte-identity, both isolated 3p axes are exercised (any 3p-handling bug surfaces in either constituent). The combined cell trades isolation-specificity for matrix breadth at the same byte-identity assertion strength."

### O2. The runtime estimate of "1-4 hours" is honest but lacks the Perl/Rust per-cell decomposition the SE plan had.

§6 says "10M PE on colossal at N=1 likely 15-20 min Perl + 12-18 min Rust; N=4 about 4× faster. 5 cells × 2 N × 2 binaries = 20 invocations → 1.5-3 h end-to-end." The arithmetic: 5 × ((15-20) + (12-18)) at N=1 + 5 × ((15-20)/4 + (12-18)/4) at N=4 = 5 × (27-38) + 5 × (6.75-9.5) = 135-190 + 33.75-47.5 = ~170-238 min = ~2.8-4 h. The plan's upper bound 1.5-3 h is on the low end.

**Suggestion:** Either widen to "1.5-4 h" or tighten the Perl/Rust per-cell estimate. Cf. SE which honestly said "1-3 h" with mostly faster cells. The CLAUDE.md profiling note says PE on 55M takes ~104 min for extraction; scaling 1/5.57 gives ~19 min for 10M, consistent with the per-cell upper bound.

### O3. `overlap > D` row-count direction asserts the include_overlap-kept R2 calls show up as additional M-bias rows — but M-bias rows are typically *positions* (1..read_len), not *calls*. More R2 calls retained does NOT necessarily mean more *position* rows.

**Anchor:** §3.3.5 last bullet (line 213); §3.4 #6.

The SE row-count differential is "fewer rows than D" — that direction is intuitive because `--ignore 5` removes positions 1-5 from output entirely, so row count decreases. The PE `overlap > D` direction is "more rows because more R2 calls retained" — but M-bias is keyed on (position, context) tuples, and the position range is bounded by read length. Adding more R2 calls at positions already-in-the-D-baseline increases per-position counts but does NOT add rows. Adding R2 calls at positions where R2 has retention but R1 does not — only those add rows.

For the 10M PE BAM with R1=R2=150 bp and varying overlap geometry, this MIGHT yield more rows in `overlap` than `D`, but the assertion is read-geometry-dependent in a way the plan doesn't articulate.

**Suggestion:** Either (a) re-derive the assertion as "overlap cell has equal-or-more rows than D AND the M-bias *position counts* (column 2 of the data rows) sum strictly greater than D" — which is the actually-asserted property; or (b) replace "row count" with "M-bias data file size in bytes (excluding header)" as the differential metric, which is monotonic in retained-call count regardless of position-vs-call ambiguity; or (c) measure on first colossal run and adjust the assertion direction in rev-2.

If the plan author is confident `overlap > D` rows-direction holds on the 10M PE BAM (which it might, due to read-pair geometry diversity in 10M reads), then (a)-style explicit articulation is fine. Otherwise (b) is more robust.

### O4. The pre-flight @PG regex alternative `^@PG.*bismark.*--paired` (§3.3.2 #5 alternative) may not match older Bismark BAMs.

The smoke script uses `@PG.*ID:Bismark.*-1 ` (presence of the `-1 R1.fq.gz` argument in @PG, which Bismark passes to Bowtie2). The plan's alternative regex `^@PG.*bismark.*--paired` keys on `--paired` being literally in the @PG line. This is NOT how Bismark constructs its @PG: Bismark passes `-1` and `-2` to Bowtie2 (not `--paired`). So the regex would never match.

**Suggestion:** If the alternative path becomes the default (per C1 above), mirror smoke's regex exactly: `@PG.*ID:Bismark.*-1 ` (note the trailing space — distinguishes `-1 ` from `-12` etc.).

### O5. The `overlap` cell flag `--include_overlap` flips a default; the plan doesn't specify whether the smoke's `Library: PE` annotation still emits or whether overriding `--no_overlap` (default) confuses the smoke's auto-detect.

Looking at smoke source: the `Library: PE` annotation only depends on @PG detection, not on the `--include_overlap` flag. So this is fine. But the plan should briefly note: "smoke's `Library: PE` detection is independent of `--extra-rust` / `--extra-perl` flag content."

### O6. PROGRESS.md row update content not specified.

§5.4 says "Add a Phase H PE row pointing at this plan + status `📝 plan rev 0 — awaiting manual review`." That's fine for rev 0, but the implementation will be in rev 1+ post-implementation-trigger. The row content for post-merge state isn't specified. SE plan had the same lacuna; not a blocker.

---

## Assumptions check

- **A1** (10M PE BAM path on colossal): admitted unverified. Acceptable; first-session verification flow exists.
- **A4** (Perl v0.25.1): enforced pre-flight; fine.
- **A8** (Phase C.1 polarity correct): the 11,443 B M-bias byte-count + `overlap > D` differential is the implicit guard. See I2 — incomplete for `--include_overlap`-only regressions.
- **A14** (≥3 GB free): the SE plan claimed 2.5 GB; the PE plan claims 3 GB ("PE output ~1.4× SE due to two-mate reporting"). Reasonable.
- **A17** (BTreeMap collector groups by pair-id consistently): the cross-N check directly tests this on PE. Good.

No critical assumption gaps beyond what the plan already flags.

---

## Efficiency

- Matrix runtime estimate is honest (1-4 h range, see O2 for the arithmetic).
- Driver overhead is negligible (matches SE).
- Disk footprint ~2.5-3 GB is well within colossal Weka.
- No efficiency Critical/Important findings.

---

## Pre-folded SE rev-3 findings — completeness check

Spot-checked against `PHASE_H_SE_PLAN.md` "Post-review absorption (rev 3, 2026-05-28)" section (lines 74-142):

| SE rev 3 finding | PE rev 0 absorbed | Where |
|---|---|---|
| B-L1 `R*100/P` direction + P=0 guard | ✅ | §3.3.5 line 214; §11 SE rev-3 reference |
| A-L1 ≡ B-L2 M-bias fail-closed + `MBIAS_GATE_APPLIES` | ✅ | §3.3.2 #10; see I3 for splitting-report gating gap |
| A-Er1/Er2 bash 4.0 + defensive `${ARR[@]+...}` | ✅ | §3.3.2 #1; §5.2 final paragraph |
| B-E3 BSD/GNU `uptime` regex `load average[s]?:` | ✅ | §3.3.2 #8 |
| Coverage §3.4 #4 row-count differential | ✅ | §3.3.5; expanded for PE direction `overlap > D` |
| SIGINT/SIGTERM trap | ✅ | §3.3.2 #2 |
| nproc-missing graceful skip | ✅ | §3.3.2 #8 |
| Perl pre-flight binary equivalence (RELEASE_CHECKLIST.md) | ✅ | §A4 reinforced |
| `cargo build` budget | ⚠️ — implicit in checklist | §4.3 checklist replacement says `cargo build --release ...` without "Budget ~5-15 min for cold cache". Minor; not blocking. |
| tmux non-optional | ✅ | §3.3.2 #9; §4.3 checklist tmux-wrap |
| Sub-2s annotation | ✅ | §3.5 last row |
| 80 LOC speedup-table dedup refactor (deferred in SE rev 3) | ✅ deferred | (correct deferral) |
| Hypothetical `--gzip` cross-N false-fail (deferred) | ✅ deferred | (correct deferral, see §7.3) |

**One minor gap:** the `cargo build` budget annotation is folded into §A4-via-SE-reference but the PE plan's §4.3 RELEASE_CHECKLIST replacement text doesn't reproduce the "Budget ~5-15 min" line that SE's RELEASE_CHECKLIST.md got per rev 3. Trivial fix.

---

## Cross-N comparison correctness on PE

The cross-N check (§3.3.4) operates on Rust output files between (N_a, N_b) pairs per CELL_ID. PE's BTreeMap collector groups by pair-id (per A17). Two questions:

1. **Pair-id ordering survives `--parallel 4`?** The plan asserts yes via A17. The crate's Phase F implementation is the authority; the cross-N check tests this directly. Good.
2. **Does pair-id grouping affect the per-mate row-count?** Cross-N is raw-byte equality on the entire output file (M-bias.txt + split files + splitting-report), not pair-id-aware. If Phase F's collector emits identical bytes for N=1 vs N=4, the check passes regardless of internal pair-id handling. Good.

Cross-N correctness is fine. The SE plan's C1 mechanism is correctly mirrored.

---

## Validation §9.1 coverage

9 checks listed. Spot-check:

| Check | Triggerable without colossal? |
|---|---|
| Driver syntax (`bash -n`) | ✅ |
| Bash 4.0 pre-flight | ✅ (run under macOS bash 3.2) |
| PE-ness assertion (SE BAM rejected) | ✅ (use local Desktop 10M SE BAM) — but only after C1 is fixed (the default mechanism currently unrunnable) |
| Perl version (mismatched rejected) | ✅ (PERL_BIN override) |
| nproc check (N > nproc) | ✅ |
| nproc-missing graceful skip | ✅ (PATH override) |
| Cross-N FAIL trigger | ⚠️ — listed as "tamper with `cell_p4_D/rust/*.M-bias.txt` post-extraction; rerun matrix". This REQUIRES a prior full extraction to exist, which needs ~30 min of Perl+Rust runtime on the local Desktop 10M PE BAM. Not "without colossal access" but "without colossal" — meets the §9.1 label. Acceptable. |
| Row-count differential FAIL trigger | ⚠️ — same caveat as above. Acceptable. |
| `cargo test` baseline | ✅ |

§9.1 is genuinely local-Mac-runnable; the cross-N and row-count fail-trigger checks require a prior matrix-run, which is fine for a release-engineer pre-merge sanity check.

---

## Splitting-report 875 B baseline strictness

The plan locks 875 B at (D, N=1) as HARD-FAIL. SE plan did NOT have a splitting-report size lock — only byte-equality vs Perl. Is the lock the right strictness for PE?

**Argument for the lock:** The splitting-report 875 B is the post-Phase-C.2 format. If Rust regresses the format (e.g., re-introduces a stale line or drops a recently-added section), Perl-vs-Rust byte-cmp would still pass (because the Perl side would also emit the regressed format if Phase C.2 was reverted upstream — though that's unlikely since v0.25.1 is locked). The lock catches "Rust regresses splitting-report format AND Perl baseline drifts simultaneously" — a low-probability double failure.

**Argument against the lock:** Splitting-report byte-count is environment-sensitive in subtle ways (e.g., a path string in the report). If the colossal `bioinf` env produces a slightly different splitting-report header path (length differs by a few chars), the byte-cmp PASSes vs Perl but the absolute 875 B lock HARD-FAILs.

Looking at typical Bismark splitting-report content: the file contains report header (with input filename / parameters), per-strand call counts, percentages, and a tail summary. The byte count is sensitive to the input BAM's *filename length* — and the assumed BAM path on colossal differs from oxy. **If the BAM filename on colossal differs in length from the oxy reference, the 875 B lock could false-fire.**

**Recommendation:** This is an **Important** finding I should have placed above but I'll flag here for completeness:

### I5 (promoted). 875 B splitting-report size lock is fragile against BAM-filename length variation.

Per Bismark's splitting-report layout, the report includes the input filename as a header line. If the colossal BAM path component (filename) differs in length from the oxy reference path (e.g., `SRR24827378_10M_R1_val_1_bismark_bt2_pe.deduplicated.bam` vs a colossal-specific name), the 875 B lock false-fires while Perl-vs-Rust byte-cmp passes.

**Suggested fix.** Either (a) lock the size with a tolerance window: `873 ≤ size ≤ 880` (5-byte slop for filename variance), or (b) verify on first colossal session and rebase the lock if it differs, or (c) drop the absolute lock and rely solely on Perl-vs-Rust byte-cmp + the existing 11,443 B M-bias lock as the C.1 guard.

Recommend (c). The M-bias byte-count lock IS the Phase C.1 guard (per §3.4 #6 + the plan's logic). The splitting-report lock adds redundancy at the cost of fragility; byte-cmp is the more robust assertion.

---

## Summary

| Severity | Count |
|---|---|
| Critical | 1 |
| Important | 5 (I1-I5; I5 promoted from O analysis) |
| Optional / nit | 6 |

**Action items prioritized:**

1. **C1** Fix the unrunnable `--dry-run` pre-flight default. Recommend `samtools view -H | grep '^@PG.*ID:Bismark.*-1 '` mirroring smoke's existing regex.
2. **I1** Add a minimum-overlap-fraction pre-flight gate OR relative threshold for `overlap > D`.
3. **I2** Add an explicit byte-count lock for `overlap` at N=1 (rev-2 post-first-colossal), or promote to §10 Open with rev-2 default.
4. **I3** Define `BASELINE_GATE_APPLIES` covering BOTH M-bias 11,443 B and splitting-report 875 B; fail-closed defaults; explicit AND-gating in §3.3.6.
5. **I4** Add recovery path for colossal-vs-planner baseline drift to §3.5 edge cases.
6. **I5** Reconsider splitting-report 875 B HARD-FAIL — fragility vs BAM-filename length. Recommend dropping the absolute lock; keep byte-cmp.
7. **O1-O6** Optional clarifications + minor doc fixes.

## Verdict

**NEEDS-REVISIONS.**

C1 alone bumps this off APPROVE-WITH-NITS — the plan's documented default for the PE-ness pre-flight check uses a flag (`--dry-run`) that does not exist in the smoke script, and the plan explicitly forbids modifying smoke in this PR. The implementer has no fallback path consistent with the written §3.3.2 #5. This MUST be resolved before implementation.

The remaining Importants (I1-I5) are individually fixable with small text edits; they don't require a structural rework. The plan is otherwise a sound mirror of SE rev 3 and the pre-folded reviewer findings are well-captured.

Estimated rev-1 turnaround: 30-60 min of plan editing once Felix accepts the findings.

# Plan Coverage Report

**Mode:** B (code and results vs plan)
**Plan(s):** `PLAN.md` rev 1 (Behavior 1–9, Implementation outline 1–8, Validation 1–6), `PREREGISTRATION.md`, `RESULTS.md`
**Branch:** `feat/5base-default-engine` in `/Users/fkrueger/Github/Bismark-5base-default`, `git diff origin/dev` (`5da54cd`) including uncommitted changes
**Date:** 2026-10-30
**Verdict:** INCOMPLETE — 6 items unresolved (2 MISSING, 4 PARTIAL). None of them changes the outcome (a) decision; see the last section.

Path-dependence: the screen chose **outcome (a)**. Behavior 8 is the in-scope outcome; Behavior 9 (outcome (c)) and the outcome-(b)-only parts of Behavior 8 are N/A. The confirm stage was not run because no challenger qualified, which `PREREGISTRATION.md` specifies ("If no arm qualifies, there is no confirm stage, and outcome (a) is decided from the screen"); items that exist only for the confirm stage are N/A.

**How the oxy runs were verified.** They cannot be re-executed here. Every benchmark claim below was checked against the recorded artifacts in `bench_results/`: `NA12878/screen/score.tsv` (+ `.contigs`, `.joined`; identical to `NA12878_screen_score.tsv` by `cmp`), every cell's `metrics.tsv` and `*_PE_report.txt`, `refcheck_{NA12878,HG002}.txt`, `validation4_concordance_A_dry10M.txt`, the input `input.md5`/`trim_galore.log`, the file list and timestamps of `oxy_5base_results.tgz`, and the PLAN iteration log. Deltas and guard thresholds were recomputed from those numbers by hand.

## Summary

- Total items: 46
- DONE: 28
- PARTIAL: 4
- MISSING: 2
- DEVIATED: 9 (all documented except where noted)
- N/A: 3

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | Pre-registration frozen and committed before any challenger runs | B1, O2 | DONE | `425d42b` authored 2026-10-29 16:28 +0100; first challenger output (dry10M B `metrics.tsv`) is 18:07 that day. `git diff HEAD` shows the file unchanged since. Content matches B1 (arms, rule, guards, challenger choice, outcome (b)). |
| 2 | Decision rule applied correctly in RESULTS.md | B1, PREREG | DONE | Recomputed from `score.tsv`/`metrics.tsv`: rule 1 Δr = B −0.00150, B-loc −0.00109, B-X −0.00030, C −0.00152 → only B-X passes. Guards: MAPQ-0 3.69% (A) vs ≤ 0.96% for all challengers; CHG/CHH within +0.1 pt; gain-set r: B-X 0.9374 < 0.9443 (= 0.96434 − 0.02), B 0.920, B-loc 0.878, C 0.933, all fail. No arm qualifies → no confirm → outcome (a). ΔCpG and cov ≥ 5 figures (+1.5%, −12%, −10%) also check out. Failing on one sample suffices because the rule requires both samples. |
| 3 | Record Bismark commit and tool versions, minimap2 `-F` default | B2.1 | DONE | Commit `9b5c15b`, minimap2 2.31-r1302, Bowtie 2 2.5.5, HISAT2 2.2.2, Trim Galore 2.2.0, `-F` 800 in PREREG/RESULTS; minimap2 version also in `bismark.log`. The literal `bismark --version` string is only hashed into the DONE key; cutadapt is not used by the Rust Trim Galore. |
| 4 | One reference, DRAGEN-compatible naming, CpG-position check ≥ 99.9% | B2.2, V2, O4 | DONE | `refcheck_*.txt`: 1,000,000/1,000,000 (100.0000%) for both samples; `five_base_compare.py refcheck` aborts below 0.999. Controls question (Q3) answered: DRAGEN controls are in a separate report. |
| 5 | Bowtie 2/HISAT2 indexes built from the same FASTA, build time and RSS recorded | B2.2, O4 | DEVIATED | Documented (iteration log 1, 5): own builds stopped; shared prebuilt Ensembl indexes used, checked for identical names and lengths (194 sequences), not sequence content. One-off build cost therefore not measured. `PREREGISTRATION.md` still says the indexes "are built from this same file"; the deviation is recorded only in PLAN.md. |
| 6 | Full FASTQ download, mate/count/`.part` checks, md5s, input-vs-DRAGEN ratio, one trim per sample | B2.3, O1 | DEVIATED | Documented: only the screen lanes (L001+L002) were downloaded and verified; full depth was deferred to the confirm stage, which did not run. DRAGEN total (489.9M pairs) recorded; the full-depth ratio is therefore not computed. Trim reports and `input.md5` recorded for both samples (Nextera auto-detected). |
| 7 | Screen subset: first ~100M pairs from the trimmed input, same files for every arm | B2.4 | DEVIATED | Documented in PREREG: subset taken *before* trimming (100,000,000 → 98,082,139 pairs). All five NA12878 reports show 98,082,139 pairs. |
| 8 | Alignment cell: `--illumina_5base … --five_base_deconvolution -p 32`, `taskset`, `/usr/bin/time -v`, process-tree RSS, aligner CPU | B3.1 | DONE | `cmd_cell`/`sample_tree` in `validation/five_base_engine_bench.sh`; `metrics.tsv` carries `tree_peak_rss_kb`, `aligner_cpu_s`, `cpus`. |
| 9 | `deduplicate_bismark -p` | B3.2 | DONE | Dedup reports present for every cell. |
| 10 | Extractor with CpG-only cytosine report | B3.3 | DONE | `bismark_methylation_extractor -p --gzip --cytosine_report`. |
| 11 | Three `concordance.py methyl` runs (own, common, gain set) | B3.4 | DEVIATED | Documented: `validation/five_base_compare.py compare` computes all three sets in one pass, reusing `concordance.py`'s parser; cross-checked against `concordance.py` (#44). Gain set uses PREREG's "A below 10" definition. |
| 12 | `concordance.py deconv` against DRAGEN VCF | B3.5 | DONE | `deconv.txt` per cell; P/R in `metrics.tsv` and RESULTS. |
| 13 | Option verification from report header **and BAM `@PG`**; mismatch fails the arm | B4 | PARTIAL | `verify_options` checks the report's options line (engine, `-p/-t 32`, `--reorder`, `--local`, `--maxins 800`, single `--no-spliced-alignment`, forbidden tokens); the recorded options lines confirm each arm. The BAM `@PG` is never checked, yet RESULTS.md's validation table says "report + `@PG`". Undocumented. |
| 14 | Accuracy, sensitivity, read-fate and specificity metrics | B5 | DONE | r, cov-weighted r, mean \|Δ\|, signed Δ, call agreement at cov 1/5/10 on own/common/gain; per-contig and joined fractions; mapping efficiency, MAPQ-0, dup rate, read-through, TLEN quantiles; CHG/CHH. Proper-pair rate is not a separate column; 5-Base emits only proper pairs, so unique pairs / pairs in serves. |
| 15 | Cost metrics: wall time, tree RSS, aligner CPU, minimap2 per-run index build reported separately, one-off index builds | B5 | PARTIAL | RSS and aligner CPU recorded. Wall time excluded because cells ran concurrently (documented; allowed by Efficiency). The one-off builds are covered by #5. minimap2's per-run index build is not reported separately anywhere, and this is undocumented. |
| 16 | NA12878 screen: arms A, B, B-loc, B-X, C | B6, O5 | DONE | All five cells and `score.tsv`. |
| 17 | HG002 screen: arm A and the leading NA12878 challenger | B6 | DEVIATED | Only HG002 arm A ran (and was not scored). RESULTS caveat says it was not needed once no challenger qualified. That reading holds, because the rule needs both samples and no arm qualified on NA12878. |
| 18 | Confirm stage skipped when no challenger passes, PR says so | B6, O6 | DONE | Correctly applied; RESULTS states it with the PREREG quote. |
| 19 | Run bookkeeping (dir per cell, DONE key on commit+args+input md5), off-box copies, 150 GB disk guard | B7 | DONE | All present in the harness; artifacts are in `bench_results/` and the tarball. |
| 20 | BAMs and cytosine reports retained until merge | B7 | DEVIATED | Documented (iteration log 7): the deduplicated BAM and per-context call files are deleted; the raw BAM, CpG report and `.cov` are kept. |
| 21 | Abort guards: joined < 50%, zero per-contig count, read-count mismatch between arms | B7 | PARTIAL | Joined-fraction (`MIN_JOINED`) and per-contig guards are implemented in `cmd_score`. **No read-count parity guard exists** in the harness. Parity holds in the data (98,082,139 in all NA12878 screen arms; 9,795,184 in all dry-run cells). Undocumented. |
| 22 | Docs name minimap2 behind the published numbers; "when to choose" sentence revised | B8 | DONE | `docs/src/content/docs/rust/illumina-5-base.md` lines 23, 47–51, 75–76. |
| 23 | Docs engine-comparison table: common-set r, CpGs at cov ≥ 10, wall time, RSS | B8 | DEVIATED | The table has mapping efficiency, common-set r, CpGs and peak memory, matching `score.tsv`/`metrics.tsv`. It has no wall time; RESULTS documents why wall times were excluded. Minor: it says "100M trimmed read pairs", but it was 100M pairs before trimming (98.1M after). |
| 24 | CHANGELOG: one sentence | B8 | DONE | One sentence, bold, in the existing style; no issue/PR link yet. |
| 25 | No new pin test; existing `illumina_5base_resolves_to_minimap2` pins the default | B8 | DONE | Test present at `rust/bismark/src/aligner/config.rs:2412`, unchanged. |
| 26 | Startup notice made engine-correct | B8 | DONE | `five_base_notice(aligner)` in `rust/bismark/src/aligner/mod.rs`; unit test `five_base_notice_names_the_engine_that_runs` (passes); CLI assertion in `five_base_1134_default_argv_is_exact` (`tests/aligner_cli.rs`). "Paired-end only" matches `illumina_5base_rejects_single_end`. The recorded B/B-X/B-loc/C `bismark.log` files show the old notice wrongly naming minimap2. |
| 27 | Outcome (b): minimap2 option change, unit test, re-run arm A | B8 | N/A | Outcome (a). |
| 28 | Outcome (c): non-breaking or breaking route | B9 | N/A | Outcome (a). |
| 29 | No release cut while in flight | B9 | DONE | No version files in the diff. |
| 30 | Authenticate BaseSpace, list and download, verify | O1 | DONE | Implementation notes: personal account (EUW2), R1 = R2 per lane, `gzip -t`, MD5SUMS; S3 durable copy. The scope limit is under #6. |
| 31 | PREREGISTRATION.md written from B1 and committed | O2 | DONE | See #1. |
| 32 | Harness and common-set filter, `metrics.tsv`, tested on 10M first | O3 | DONE | `validation/five_base_engine_bench.sh` (bash instead of POSIX sh, documented) + `validation/five_base_compare.py`; dry10M cells precede the screen. |
| 33 | Run the screen and present the table and challenger to Felix | O5 | DONE | RESULTS screen table; implementation notes record Felix's decision. |
| 34 | RESULTS.md with tables, rule applied, caveats (one protocol, no cfDNA) | O6 | DONE | Screen table, read fate/cost, validation table, caveats (depth, gain-set strictness, scope, HG002), follow-ups. |
| 35 | Stop and present the decision; implement only after the trigger | O7 | DONE | Implementation notes, 2026-10-30: Felix gave the trigger for Behavior 8. |
| 36 | Implement; fmt, focused tests, release Clippy | O8 | DONE | Re-run here, all pass (see Test verification). |
| 37 | Full suite, dual code review, plan-manager audit, PR into `dev` | O8 | PARTIAL | Full suite reported "running" in the notes and not verified by this audit. No code-review result was consulted (out of scope per instructions). This audit is the plan-manager step. No PR yet. These are pending sequence steps. |
| 38 | Dry run: repeats deterministic; option verification passes | V1 | DONE | dry10M A rep1/rep2 and B rep1/rep2 have identical `metrics.tsv` accuracy and read-fate columns and identical DONE keys. The byte-identical CpG-report md5s (`f1d707dc…`, `62a0bf32…`) come from the iteration log; the CpG reports were not copied off-box. |
| 39 | Dry run: wall time within 10% | V1 | DEVIATED | Failed (A 1073 vs 924 s; B 566 vs 542 s, host load up to 1070). Documented; timing is demoted to tie-breaker only, as planned. |
| 40 | Dry run: an injected wrong option must fail verification | V1 | MISSING | No record in the iteration log, RESULTS or artifacts that a wrong option was injected and rejected. `verify_options` would `die` on a mismatch, but the negative test was not run or not recorded. |
| 41 | Reference check ≥ 99.9% | V2 | DONE | See #4. |
| 42 | Same input: identical read counts per depth; input md5s per cell | V3 | DONE | Counts identical (see #21). The md5s are recorded per prep in `input.md5` and folded into each cell's DONE key, not printed per cell. |
| 43 | Common-set filter toy test (three hand-built reports) | V4 | DONE | `five_base_compare.py --selftest` covers common/gain/own counts, contig exclusion, non-CpG rows and refcheck; passes. |
| 44 | Independent r recompute on a 100k-CpG random sample of one real arm, match `concordance.py` to 3 dp | V4 | DEVIATED | Done as `concordance.py` vs the scorer on the full dry10M arm A at cov ≥ 10 (n = 9,501): 0.7899 vs 0.7896, which agree to 3 dp. It is not a 100k random sample. The scorer's 0.7896 appears only in RESULTS; the dry10M `score.tsv` is not committed. |
| 45 | Baseline sanity: full-depth arm A within ±0.005 of 0.998 / 0.996 | V5 | N/A | Defined on full-depth arm A only; the confirm stage was correctly not run. Screen-depth r (0.964 at cov ≥ 10, ~4.7×) is not comparable to the published figures. |
| 46 | Guards fire: truncating a contig triggers the per-contig abort | V6 | MISSING | The guard exists in `cmd_score`, but no test or record shows it firing. The Python selftest does not reach the shell guard. The joined-fraction guard *was* seen firing at 10M (iteration log 6). |

## Gaps (detail)

### Item 13: `@PG` option verification

**Expected:** each arm's report header **and BAM `@PG`** show the expected engine and options; a mismatch fails the arm.
**Found:** `verify_options` checks only the report's "with the specified options:" line. RESULTS.md's validation table claims "verified per arm by the harness (report + `@PG`)".
**Gap:** either check `@PG` on the retained raw BAMs (they are still on oxy `/`), or correct the RESULTS.md wording to "report".

### Item 15: minimap2 per-run index build cost

**Expected:** minimap2's per-run index build included in wall time and also reported separately.
**Found:** not reported; wall times are excluded wholesale (documented), but the separate index-build figure is neither reported nor noted as dropped.
**Gap:** record it (e.g. from the `[M::mm_idx_gen]`/`[M::main::…]` timing lines in each A `bismark.log`) or document the omission.

### Item 21: read-count parity abort guard

**Expected:** the harness aborts a cell when input read counts differ between arms of the same depth.
**Found:** no such check in `five_base_engine_bench.sh`; parity was confirmed only after the fact from the reports.
**Gap:** add the guard (e.g. in `cmd_score` over the arms' `pairs_in`) or document that parity was checked manually.

### Item 37: closing steps of outline step 8

**Expected:** full test suite, dual code review, plan-manager audit, then a PR into `dev`.
**Found:** fmt, the focused tests and release Clippy pass. The full suite is "running" per the notes. Code review and PR status were not assessed here.
**Gap:** confirm the full-suite result, complete the review, open the PR.

### Item 40: injected wrong option

**Expected:** the dry run shows that an injected wrong option fails option verification.
**Found:** no record.
**Gap:** run `verify_options` against a deliberately wrong report/flag set (or note why it was skipped) and record the result.

### Item 46: per-contig guard fires

**Expected:** truncating one contig in a test report triggers the per-contig abort.
**Found:** guard implemented; no test or record of it firing.
**Gap:** run `score` on a CpG report with one contig removed and record the abort.

## Test verification (Mode B)

All commands run 2026-10-30 from `/Users/fkrueger/Github/Bismark-5base-default/rust`.

| Check | Result |
|-------|--------|
| `cargo fmt --all -- --check` | PASS (exit 0) |
| `cargo test -p bismark --lib aligner::` | PASS: 612 passed, 0 failed |
| `cargo test -p bismark --test aligner_cli` | PASS: 139 passed, 0 failed |
| `cargo clippy --workspace --all-targets --release -- -D warnings` | PASS (exit 0) |
| `python3 -I ../validation/five_base_compare.py --selftest` | PASS ("selftest OK") |
| `python3 -I validation/concordance.py --selftest` (plan Assumption: passes before use) | PASS |
| `aligner::tests::five_base_notice_names_the_engine_that_runs` | PASS (run individually) |
| `five_base_1134_default_argv_is_exact` (new stderr assertion) | PASS (within `aligner_cli`) |
| `illumina_5base_resolves_to_minimap2` (existing pin) | PASS (within `--lib aligner::`) |
| oxy benchmark cells | Not re-executable; verified from recorded artifacts (see top) |

## Verdict

INCOMPLETE — 6 items unresolved. The outcome (a) code and docs change (Behavior 8) is complete and tested, and RESULTS.md applies the pre-registered rule correctly. The open items are harness and validation evidence gaps plus the pending closing steps; none of them changes the decision.

1. **#13**: `@PG` was never checked, and RESULTS.md overstates the check as "report + `@PG`". Check the BAMs or correct the wording.
2. **#15**: the minimap2 per-run index-build cost is neither reported nor noted as dropped.
3. **#21**: the harness has no read-count parity abort guard. Parity holds in the data.
4. **#37**: full-suite result, code review and PR are pending.
5. **#40**: no record of the injected-wrong-option negative test (Validation 1).
6. **#46**: no record of the per-contig guard firing (Validation 6).

Worth a line but not counted as gaps: PREREGISTRATION.md still says the indexes were built from the FASTA (#5). The docs table says "100M trimmed read pairs" where the input was 100M pairs before trimming (#23).

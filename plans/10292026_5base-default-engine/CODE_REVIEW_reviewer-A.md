# Code review (Reviewer A): 5-Base default engine, outcome (a)

**Target:** `feat/5base-default-engine` vs `origin/dev` (commits `425d42b`, `69b6b90` plus uncommitted docs/CHANGELOG/`mod.rs`/`aligner_cli.rs` changes).
**Date:** 2026-10-30.

## Summary

The decision is right and holds up. I recomputed every delta and guard in the RESULTS screen table from `bench_results/NA12878/screen/score.tsv` and the five `metrics.tsv` files, and all of them match. Only B-X passes rule 1. It fails the gain-set guard, and it would fail rule 3 ("clearly better") anyway, with ΔCpG +0.6% and Δr −0.0003. So outcome (a) does not depend on the gain-set guard, which is the one debatable rule. The `five_base_notice` change is correct. SE input and `--rammap` are both rejected before the notice prints, the from-BAM modes return earlier, and `Aligner::name()` gives "minimap2", "Bowtie 2" and "HISAT2". The focused tests pass: `--lib aligner::five_base` 56/56, `five_base_notice_names_the_engine_that_runs` 1/1, `--test aligner_cli five_base` 14/14. `five_base_compare.py --selftest` passes and `bash -n` is clean on the harness.

The problems are in the write-ups, not the decision:
- **RESULTS.md:**
  - It says the harness verifies options from `@PG`. The harness only checks the report.
  - Two of its "What the screen shows" claims are wrong against its own table.
- **Docs table:** it mixes two denominators under a sentence that implies one. Its r values (~0.964) sit next to the stability contract's 0.998 at cov ≥ 10 with no word on depth.
- **Harness:** a few gaps could let a stale or partial cell be scored without warning. None of them affected these results.

## Issues by area

### 1. `five_base_notice` and tests

- **`rust/bismark/src/aligner/mod.rs:165-173` (correct).** The notice is reached only when `config.five_base` is set. By then SE input has been rejected (`config.rs:930/942`), `--rammap` with 5-Base has been rejected (`config.rs:1419`), and `--five_base_consensus_from_bam` has returned (`mod.rs:201`). So "aligned with {engine}" and "paired-end only" are always true when it prints.
- **`mod.rs:8456-8473` (Low).** The unit test builds its expected text from `aligner.name()` too, so it checks the interpolation, not the engine names. The Bowtie 2 and HISAT2 names are pinned by the CLI test. "minimap2" is pinned only through `name()`.
- **`tests/aligner_cli.rs:7551-7560` (Low).** The test asserts that the right engine is named but never that minimap2 is absent. A stray second "aligned with minimap2" notice would still pass. Add `assert!(!stderr.contains("aligned with minimap2"))`.
- **`mod.rs:252` (comment, OK).** The one-line comment is factual. The doc comment at `mod.rs:164` is fine.

### 2. Benchmark harness

**`validation/five_base_compare.py`: the set definitions and accumulators are correct.**

- **Own set:** arm and DRAGEN both at or above the threshold (`:140-146`).
- **Common set:** DRAGEN cov ≥ 10 and every arm cov ≥ 10, using `.get(k, 0)` (`:148-149`).
- **Gain set:** arm ≥ 10, DRAGEN ≥ 10 and baseline < 10, with an absent baseline counting as 0 (`:156-166`).
- **Joined fraction** uses cov ≥ 1 on both sides.

All of these match PREREGISTRATION "Metrics and sets".

- **Accumulators:**
  - The single-pass Pearson and weighted Pearson (`n` replaced by Σw, w = min(cov_arm, cov_DRAGEN)) are algebraically right and use the same weight as `concordance.py:124`.
  - With x, y in [0, 100] and n ≤ 55M, double-precision cancellation error is about 1e-5 relative, which is negligible.
  - The signed d is arm − DRAGEN, as pre-registered.
  - Key packing (contig << 33 | pos << 1 | strand) and value packing (meth << 32 | cov) cannot overflow for GRCh38.
- **Refcheck:**
  - The reservoir sampling is a correct Algorithm R.
  - The minus-strand test `s[pos-2:pos] == "CG"` is correct for a C on the minus strand at `pos`.
- **`:48` (Low):** `line.split("\t", 6)` keeps the newline in `f[5]` when a report has exactly 6 columns. Every row is then dropped. The failure is loud (`no CpGs` SystemExit), and both formats in use have 7 columns. `rstrip("\n")` would make it robust.
- **`_selftest` (Low):**
  - It checks set sizes and the mean signed d.
  - It never checks an r value against an independent computation (for example against `concordance.weighted_pearson` / `statistics.correlation` on the toy rows).
  - It never checks that refcheck fails on a non-CpG position: every toy position is a CpG.
  - Plan Validation 6 (a truncated contig must fire the per-contig abort) has no test and is not reported in RESULTS.
- **Efficiency (Low):** six dicts of about 55M int→int entries come to roughly 30–50 GB of RAM. That is fine on oxy but worth one line in the docstring, because it will not run on a laptop at full depth.

**`validation/five_base_engine_bench.sh`: these are the ways it could silently produce a wrong comparison.**

- **`:229-233` `cmd_score` (Medium):**
  - It never checks `$d/DONE` (or its key) before scoring a cell.
  - It takes `ls …/*CpG_report.txt* | head -1`.
  - A cell that died after the extractor started (`set -e` exits before `DONE`), or a stale cell, is scored without warning.
  - `rep1` is hard-coded, so `rep` cannot be chosen.
  - Fix: require `DONE` with a matching key, and require exactly one CpG report.
- **No cross-arm read-count parity guard (Medium).** Plan Behavior 7 lists "input read counts differ between arms of the same depth" as a cell-aborting guard. The harness never compares `pairs_in` across arms. Inputs are shared by construction, and I confirmed 98,082,139 in all five `metrics.tsv` files, so the result stands. The guard is still missing; add it to `cmd_score`.
- **`:56-80` `verify_options` checks only the report's options line (Medium).**
  - It never checks `@PG`, although plan Behavior 4 says "report header and BAM `@PG` must show".
  - RESULTS.md's validation table states "Options actually applied (report + `@PG`) | verified per arm by the harness". That is untrue for the committed harness.
  - Either add a `samtools view -H | grep @PG` check or correct the RESULTS row.
  - Minor gaps: arm A does not check `-x sr` and B-loc does not deny `--maxins`. The stored option lines I inspected are all as intended.
- **`:177-180` `setsid … &` then `wait $pid` (Low).**
  - This is correct only because a non-interactive script's background child is not a process-group leader, so `setsid` does not fork.
  - Under job control (`bash -i`, or someone sourcing the harness), `setsid` forks. `$!` then exits at once, `wait` returns 0 while bismark is still running, and the sampler records nothing.
  - `setsid -w` makes it safe.
- **`:173` `commit=$(bismark --version | grep -o …)` (Low).** Under `pipefail` a version string with no hash exits the script with no error message. Every other failure in the harness uses `die`.
- **`:121` DRAGEN CX selection (Low).** `ls …*.CX_report.txt.gz | grep -v control` would concatenate two non-control files if both were present. Assert exactly one.
- **`:148-158` `sample_tree` (Low, documented as approximate).** The 2 s sampling can miss short RSS peaks, and up to 2 s of aligner CPU at exit. It is fine for "tie-breaker only".
- **CHG/CHH guard precision (Low).** These come from the report at 1 decimal, against a +0.2 pt guard, so a ±0.1 rounding error is material at the margin. It is irrelevant here, but the extractor's splitting report gives more digits.

### 3. RESULTS.md vs the pre-registered rule (recomputed)

| Arm | common r | Δr (mine) | own cov ≥ 10 | ΔCpG (mine) | MAPQ-0 % (mine) | gain r vs common r − 0.02 |
|---|---|---|---|---|---|---|
| A | 0.964638 | — | 817,312 | — | 3.694 | — |
| B | 0.963133 | −0.001505 | 554,655 | −32.14% | 0.959 | 0.9198 < 0.9431 ✗ |
| B-loc | 0.963544 | −0.001094 | 580,613 | −28.96% | 0.155 | 0.8777 < 0.9435 ✗ |
| B-X | 0.964342 | −0.000296 | 822,432 | +0.63% | 0.916 | 0.9374 < 0.9443 ✗ |
| C | 0.963122 | −0.001516 | 664,790 | −18.66% | 0.194 | 0.9330 < 0.9431 ✗ |

- **Every figure in the RESULTS screen table matches,** including the deconvolution P/R from `deconv.txt` (A 94.2/61.3, B-X 93.5/61.6) and the read-fate table:
  - read-through A 0.86%, B-loc 1.03%, C 0.50%;
  - the TLEN quantiles and duplicates;
  - peak RSS from `tree_peak_rss_kb`.
- **The cov ≥ 5 caveat figures also match:** B-X 0.9540 vs A 0.9537 and +1.53%, B −12.1%, C −9.8%. These are own-set r; the text could say so.
- **The rule is applied correctly.** B-X is the only rule-1 passer. It passes MAPQ-0 (0.92 ≤ 5.69) and CHG/CHH. It fails the gain guard whether "common r" means its own (0.9443) or A's (0.9446). No arm qualifies, so there is no confirm stage. That is the pre-registered outcome (a).
- **Note (no action):** the gain set is by construction near the cov-10 threshold, where binomial noise lowers r. The guard therefore leans against any challenger. RESULTS already says so ("strict"), and the decision does not depend on it because B-X misses rule 3.
- **RESULTS.md:118 (Medium, factual error):** "minimap2 … the highest deconvolution recall". B-X's recall is 61.6%, above A's 61.3%. A has the best common-set r, not the best recall.
- **RESULTS.md:117 (Medium, factual error):** "B-loc … has the lowest accuracy". On the primary metric, common-set r, B-loc (0.96354) is above B (0.96313) and C (0.96312). It is lowest only on own-set and gain-set r. Its deconvolution precision (88.1) is the lowest, as stated.
- **RESULTS.md:119 (Low/Medium):** "HISAT2 … otherwise similar to Bowtie 2". The data say otherwise on read fate. C's TLEN quantiles (243/352/478) match minimap2's, while B's (p90 445) are truncated by the 500 bp cap. HISAT2's CpG loss therefore does not look like an insert-cap effect. This matters for the follow-up "make `-X 800`… the 5-Base Bowtie 2/HISAT2 default": the screen has no evidence for HISAT2, and no HISAT2 `-X 800` arm was run.
- **RESULTS.md:131 (Medium):** as above, the "report + `@PG`" option-verification claim is not backed by the harness.
- **RESULTS.md:129 (Low):** the scorer-side value 0.7896 for the `concordance.py` cross-check comes from a dry-run `score.tsv` that is not committed (`bench_results/NA12878/dry10M/` has no score file). Only the `concordance.py` side (`validation4_concordance_A_dry10M.txt`, 0.7899) can be checked. Commit it or cite where it lives.
- **Plan coverage (Low):** these plan Validation items are not reported in RESULTS:
  - "an injected wrong option must fail it" (V1);
  - V5 baseline sanity, which needs full depth and is reasonably moot under (a), but RESULTS should say it was not run;
  - V6, the truncated-contig abort.

### 4. Docs "Choosing an engine" (`docs/src/content/docs/rust/illumina-5-base.md:97-115`)

- **Numbers:** every cell matches the data.
  - Mapping efficiency: 95.2 / 87.9 / 94.5 / 91.2 / 91.3.
  - Common-set r: 0.9646 / 0.9631 / 0.9643 / 0.9635 / 0.9631. Each rounds correctly from `score.tsv`.
  - CpGs at cov ≥ 10: the own-set counts.
  - Peak memory: `tree_peak_rss_kb`.
  - "about 7 points" (7.3) and "a third of its well-covered CpGs" (32.1%) are right.
- **`:99-100` (Medium, clarity):** the lead-in says the scoring is "on the 400,760 CpGs every engine covered at cov ≥ 10". That is true for the r column only. "CpGs at cov ≥ 10" is each engine's own count jointly with DRAGEN (817,312, 554,655 and so on), so the sentence makes the table look internally inconsistent. Suggested wording: "r is on the 400,760 CpGs every engine and DRAGEN covered at cov ≥ 10; the CpG column is each engine's own count at cov ≥ 10 jointly with DRAGEN."
- **`:99` vs stability contract `:23-26` (Medium):** the table shows the default at r 0.9646 at cov ≥ 10. A few paragraphs earlier the contract says 0.998 at cov ≥ 10 for the same engine and sample. Nothing explains that the screen is about 4.7× per strand, where cov ≥ 10 is the high-coverage tail (about 1.5% of CpGs). A reader will see a contradiction. Add one clause, for example "a ~100M-pair subset, so cov ≥ 10 is the high-coverage tail; r is not comparable with the full-depth figures above".
- **`:99` (Low):** "On 100M trimmed read pairs". It is the first 100M raw pairs, which is 98.1M after trimming.
- **`:106`, Peak memory units (Low):** the figures are KiB/1e6. 16,295,504 KiB is 16.7 GB (15.5 GiB); 8.76M KiB is 9.0 GB (8.4 GiB). The ratio and conclusion are unchanged, but the label is off by about 2–7%.
- **`:111-112` (Low/Medium, unverified causal claim):** "minimap2 needs about twice the memory because it builds its index from the FASTA on every run". The benchmark did not separate index building from index residency. A prebuilt `.mmi` for `-x sr` would itself be large, so the "because" is an inference. Prefer "minimap2's in-memory index is about twice the size; it is rebuilt from the FASTA on every run", or drop the causal link.
- **`:75-76` (Low):** "500 bp, which is short for typical 5-Base fragments". The data cover one library (TLEN p90 478 bp). "short for the NA12878 demo library" or "can be short for 5-Base gDNA libraries" is supported. "typical" is a generalisation.
- **`:108-110` (OK):** the advice is limited to Bowtie 2, which is what the data support. Good: it does not tell HISAT2 users that `-X 800` helps.
- **Behavior 8 deviation (Low):** the plan's table spec includes wall time. It was dropped, rightly, because the screen ran concurrently, but this is not recorded among the deviations in PLAN.md's "Implementation notes — outcome (a)". Add one line.
- **`:23` (Low, pre-existing):** "at full depth, with the default minimap2 engine". The plan context records that the published NA12878 run used a partial download (about 384M of DRAGEN's 438–490M pairs). This diff adds a qualifier next to "full depth" without correcting it. Out of scope, but noted.

### 5. CHANGELOG (`CHANGELOG.md:18`)

- It is one sentence in the bold-sentence style of the neighbouring entries ✓.
- **Low:** every neighbouring entry carries an issue/PR link (`([#NNN](url))`) and this one has none. Add the PR link once the PR is open, or the follow-up issue number.

### 6. Comment style

- `mod.rs:164` and `mod.rs:252` are one line, factual, with no history ✓.
- The harness comments (`five_base_engine_bench.sh:55, 84, 147, 223`) are short and factual ✓.
- The `five_base_compare.py` docstrings are concise ✓.

### 7. Housekeeping

- **`PROGRESS.md` (Low):** Implementation still says "docs/notice change awaits Felix", and Code Review says "Planned". Update it after this review (`/progress update`).
- **`PLAN.md:407` (uncommitted notes, Low):** "The full suite is running." Record the result before commit.

## Fixes applied

None (report-only review, per instructions).

## Recommendations

**Critical:** none.

**High:** none. The decision is correctly derived and robust to the one debatable guard.

**Medium:**
1. RESULTS.md:131: remove "+ `@PG`" from the option-verification row, or add a real `@PG` check to `verify_options` (`five_base_engine_bench.sh:56`).
2. RESULTS.md:117-118: fix the two factual errors (B-X, not minimap2, has the highest deconvolution recall; B-loc is not lowest on common-set r).
3. Docs `:99-100`: say which column uses the common set and which uses the own set.
4. Docs: add one clause explaining why the screen's r (~0.964) differs from the contract's full-depth 0.998.
5. `cmd_score`: require a matching `DONE` per cell and exactly one CpG report, and assert `pairs_in` parity across arms (the Behavior 7 guard).

**Low:**
1. CLI test: also assert that minimap2 is absent from the notice for `--bowtie2`/`--hisat2`.
2. Docs: "100M raw pairs (98.1M after trimming)".
3. Docs: GB/GiB units.
4. Docs: soften or drop the "because it builds its index" causal claim.
5. Docs: "typical 5-Base fragments" → "this library".
6. RESULTS.md:119 / follow-up: HISAT2's loss is not explained by the insert cap (its TLEN matches minimap2's), so a HISAT2 `-X` default change needs its own evidence.
7. CHANGELOG: add a PR/issue link.
8. Harness hardening:
   - `setsid -w`;
   - `die` on a missing version hash;
   - assert a single DRAGEN CX file;
   - `rstrip` in `load_cpg`.
9. Selftest:
   - add an r-value check against an independent computation;
   - add a refcheck negative case;
   - add the V6 truncated-contig guard test.
10. Commit the dry-run `score.tsv` behind the 0.7896 figure. In RESULTS, note that V1 (injected-option failure), V5 and V6 were not run or not recorded.
11. Update PROGRESS.md. Record the wall-time-column deviation and the full-suite result in PLAN.md.

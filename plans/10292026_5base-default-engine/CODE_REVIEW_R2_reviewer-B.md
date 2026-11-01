# Code review, round 2: `feat/5base-default-engine` (Reviewer B)

Target: the uncommitted diff against `HEAD` in `/Users/fkrueger/Github/Bismark-5base-default`, which applies the fixes from `CODE_REVIEW_reviewer-B.md` and `PLAN.md` "Review round — 2026-11-01".

## Summary

**Verdict: APPROVE.** All five Medium findings from round 1 are resolved correctly. Of the nine Low findings, seven are resolved, one is still open and cosmetic (no read-through % for B and B-X), and one has to wait for the PR (CHANGELOG link). The fixes introduce no regressions that affect the decision or the published numbers. Every number in the docs table and in `RESULTS.md` matches `score.tsv` and the per-arm `metrics.tsv`. Memory is now correct in GiB.

What is left is all Low:
- a few guards are wired in but have no negative test;
- two harness checks can still pass or exit without saying why;
- `RESULTS.md` could say more plainly what the oxy wipe means for re-scoring.

Checks run:
- `python3 -I validation/five_base_compare.py --selftest`: OK. All new guards fire with the expected messages.
- `bash validation/five_base_engine_bench.sh selftest` (TMPDIR = scratchpad): 13 passed, 0 failed (5 pass cases, 8 injected failures).
- `bash -n validation/five_base_engine_bench.sh`: OK. shellcheck is not installed.
- `cargo test -p bismark --test aligner_cli five_base_1134_default_argv`: 1 passed.
- `cargo test -p bismark --lib five_base_notice`: 1 passed.
- `cargo fmt -p bismark -- --check`: clean.
- The new `verify_options` requirements, checked against the committed screen `*_PE_report.txt` option lines: arm A has ` -x sr `, B-loc has ` --reorder `, and C has `--no-spliced-alignment` exactly once. The tighter checks would have passed on the real run.
- Every number recomputed from `bench_results/NA12878/screen/{score.tsv,score.tsv.joined,score.tsv.contigs,*/metrics.tsv}` (table in §4).

## Round-1 findings: status

| # | Finding | Status |
|---|---|---|
| M1 | Unsupported memory cause ("builds its index") | **Resolved.** Docs say "minimap2 used about twice the peak memory". `RESULTS.md` says "The cause was not measured." |
| M2 | "short for typical 5-Base fragments" | **Resolved.** Now "can cut off the longest fragments of a 5-Base library". |
| M3 | B-loc "lowest accuracy"; minimap2 "highest recall" | **Resolved.** B-loc is lowest on own-set r at cov ≥ 1, 5 and 10, and on gain-set r. "Recall within 0.3 pt of the best (61.3% vs B-X's 61.6%)" is correct. |
| M4 | `score` does not require `DONE` | **Resolved.** Requires `[[ -s DONE ]]` and exactly one CpG report, and adds the read-parity check. |
| M5 | `setsid`/`wait` fragility | **Resolved.** `setsid` is gone, so `$!` is the taskset→time process that `wait` waits on. The tree is found by walking ppid links (§2). |
| L6 | "100M trimmed" | **Resolved.** "first 100M read pairs … 98.1M pairs after trimming". |
| L7 | Shared-set label vs own-count column | **Resolved** in the lead-in. Both sets are defined, and the DRAGEN cov ≥ 10 requirement is stated. |
| L8 | KiB read as kB | **Resolved.** GiB everywhere. Values verified in §4. |
| L9 | Screen r vs the 0.998 contract | **Resolved.** "about 5× per strand … r is lower than the full-depth figure". |
| L10 | `RESULTS.md` nits | Mostly **resolved**. "+ `@PG`" is now explained, the notice follow-up is marked done, and `--dovetail` is noted. **Open:** B and B-X read-through still has no % (0.003% each). Cosmetic. |
| L11 | `five_base_compare.py` guards and r assertion | **Resolved** (§1). |
| L12 | Harness: unique CX report, `-x sr`, B-loc `--reorder` | **Resolved.** For a caveat on the CX check, see §2. |
| L13 | CHANGELOG PR link | **Open.** No PR yet. |
| L14 | Call-site comment restates the notice | Kept, shortened to one line. Fine. |

## 1. `validation/five_base_compare.py`

- **Newline fix (`:46`):** `line.rstrip("\n").split("\t", 6)` gives `f[5] == "CG"` for a 6-column report. The self-test proves it with `six.txt`.
  - It does not strip `\r`, but CRLF reports are not an expected input. Not worth changing.
  - The screen inputs were 7-column (Bismark CpG report, and DRAGEN CX → `awk '$6=="CG"'`), so this fix does not affect the screen numbers.
- **Duplicate site (`:55-57`) and duplicate arm (`:117-118`):** both correct, and both fail before any output is written.
- **Guards moved from bash (`:163-190`):** they are semantically identical to the old bash. The per-contig check is `dragen_cov10 > 0 && joined_cov10 == 0`, and the joined-fraction check is `j / drg_cov1 < min_joined`, with the same default of 0.5 on both the CLI and `MIN_JOINED`.
  - Errors are collected and raised only after `.contigs` and `.joined` are written. That helps diagnosis, and `set -e` in the caller still stops before the copy to `$RESULTS`.
  - `drg_cov1` cannot be 0, because `load_cpg` refuses an empty file and keeps only cov > 0.
- **r self-test (`:296-299`):** I checked the hand-built vectors against the fixture.
  - A's own set at t = 1 is (100, 100, w = 10), (8.33, 0, w = 12), (50, 50, w = min(4, 12) = 4), (100, 100, w = 3).
  - That matches `Acc.add`, including the `min(oc, dc)` weight. So the assertion really ties the single-pass accumulators to `concordance.pearson` and `concordance.weighted_pearson`.
- **Low, test gap:** the empty-common-set guard has no assertion of its own. It does fire in the `chr2_only` case, whose output shows "common set = 0 CpGs", but `fails()` matches only the contig message. A one-line `"empty common set"` needle on that same call would pin it. The PLAN's claim that the self-test "covers each guard" is therefore slightly generous.
- The failing refcheck case (`off_by_one`) is a good addition.

## 2. `validation/five_base_engine_bench.sh`

**`tree_pids` / `sample_tree` (`:141-163`)**
- The BFS over `ps -e -o pid=,ppid=` is correct. `split` resets `c` on each call. The root is included, and there are no cycles. `ps -o … -p <comma list>` is valid procps syntax.
- Behaviour matches the old `ps -s` session sampling for the normal tree: taskset → `/usr/bin/time` → bismark → aligner, plus bismark's helper children.
- Peak RSS can now differ from the old method only if a descendant is reparented: if it double-forks, or its parent dies before it does. Bismark does not do that. Fine.
- The variable is still called `sid` but now holds a PID. Naming nit.
- **Low:** a peak of 0 is still not guarded. If `ps` fails, for example under a restricted sandbox or a missing procps, `2>/dev/null` hides it and `tree.tsv` records `0 0` in silence. Round 1 suggested this guard as an alternative to `setsid -w`. It is less important now that `wait` is robust, but `[[ $(cut -f1 "$d/tree.tsv") -gt 0 ]] || die …` would cost one line.

**`check_pg_line` / `verify_pg` (`:72-86`)**
- The format matches the Rust writer: `generate_sam_header` emits `ID:Bismark` with `CL:"bismark <verbatim argv>"` (`rust/bismark/src/aligner/output.rs:120-123`). The harness passes `-p "$THREADS"` as two adjacent argv tokens, so `"-p $THREADS"` matches as a pair. Stripping the quotes and padding with spaces makes the token match exact.
- **Low:** the `$(arm_flags "$arm")` tokens are checked one at a time. `-X 800` therefore becomes two independent tokens, `-X` and `800`, and `--five_base_index` is checked apart from its path. Since the harness always passes `$flags` as one contiguous run, checking `*" $flags "*` as a whole, as well as `--illumina_5base` and `-p N`, would be stricter at no cost. Arm A's flags are empty, so A's `@PG` check reduces to `--illumina_5base` and `-p N`. The report's "run with minimap2" check covers the engine.
- `verify_pg` takes the first `ID:Bismark` `@PG`, which is the only one in a fresh aligner BAM. Correct.

**`score` guards (`:251-269`)**
- The `DONE` check, the exactly-one CpG report check and read parity (`metrics.tsv` column 5 = `pairs_in`) are all correct.
- **Note:** read parity is close to tautological, because every arm reads the same `input/` directory. It would catch only a cell scored against a re-prepped input of a different size. A stronger check would compare each cell's `DONE` key against one recomputed from the current `input.md5`. That is optional.
- **Low:** if `metrics.tsv` had an empty `pairs_in` in every arm, the check would pass (one unique empty value). This is unlikely, because the cell would have produced garbage metrics elsewhere.

**`selftest` (`:272-301`)**
- The five pass cases use option strings that are identical to the real screen reports. I compared them against the committed `*_PE_report.txt` files.
- The 8 failures cover:
  - wrong `-t`;
  - wrong `-p` in `@PG`;
  - B missing `--reorder`;
  - B with `--maxins`;
  - B-loc missing `--local`;
  - B-X `@PG` missing `-X 800`;
  - C with a doubled no-splice flag;
  - C `@PG` missing `--illumina_5base`.
- **Low, test gap:** the two requirements this round added (A ` -x sr `, B-loc ` --reorder `) and the `--local` deny for B-X and C have no injected failure. Three more `expect fail` lines would cover them.

**Other harness changes**
- The missing-commit guard (`:171-172`) is correct under `pipefail`: grep finds nothing → `|| true` → `[[ -n ]]` fails → `die`.
- **Low, CX uniqueness (`:128-129`):** the count check is right when one or more files remain. With none, it fails in silence:
  - if the only CX report is a `.control.` file, `grep -v` returns 1;
  - `set -e` then exits the script before the `die`, with no message;
  - and `wc -l <<<""` returns 1 anyway, so the count check would not catch the empty case even if reached.

  `cx=$(…) || die "no non-control DRAGEN CX report"` closes both gaps.

## 3. CLI test (`rust/bismark/tests/aligner_cli.rs:7551-7565`)

- Correct.
- The notice is a single `eprintln!` line (backslash continuations), so `lines().find(starts_with("Note: --illumina_5base"))` captures all of it.
- The test asserts the right engine name and the absence of "minimap2" for both bowtie2 and hisat2. The minimap2 case is covered by the unit test `five_base_notice_names_the_engine_that_runs`.
- Both tests pass, and fmt is clean.

## 4. Docs "Choosing an engine" and `RESULTS.md` vs the data

The table below is recomputed from `metrics.tsv` (`tree_peak_rss_kb` / 1,048,576 = GiB) and `score.tsv`.

| Arm | map_eff | common r | own cov ≥ 10 n | tree RSS GiB | MAPQ-0 % | read-through | TLEN p10/50/90 | P / R |
|---|---|---|---|---|---|---|---|---|
| A | 95.2 | 0.964638 | 817,312 | 15.54 | 3.694 | 801,819 (0.858%) | 241/352/478 | 94.2 / 61.3 |
| B | 87.9 | 0.963133 | 554,655 | 8.36 | 0.959 | 2,937 (0.003%) | 242/346/445 | 93.2 / 58.3 |
| B-loc | 91.2 | 0.963544 | 580,613 | 8.38 | 0.155 | 919,122 (1.028%) | 235/344/446 | 88.1 / 56.2 |
| B-X | 94.5 | 0.964342 | 822,432 | 8.39 | 0.916 | 2,929 (0.003%) | 246/353/479 | 93.5 / 61.6 |
| C | 91.3 | 0.963122 | 664,790 | 8.28 | 0.194 | 446,094 (0.498%) | 243/352/478 | 95.2 / 57.7 |

All docs-table cells match, as do all `RESULTS.md` screen-table and read-fate cells. The derived figures also check out:

| Figure | Data |
|---|---|
| Δr | −0.00150 / −0.00109 / −0.00030 / −0.00152 |
| ΔCpG | −32.1 / −29.0 / +0.6 / −18.7% |
| Gain-set r | 0.920 / 0.878 / 0.937 / 0.933 |
| "B loses 7.3 points" | 95.2 − 87.9 |
| "about 7 points … a third" | vs B-X: 6.6 pt, −32.6% |
| "about twice the peak memory" | 1.86× |
| Joined fraction "96–98%" / "≥ 0.96" | 0.9617–0.9755 |
| "no empty contig" | No zero row in `.contigs` |
| CPU and busy threads | A 54,957 s / 14,611 s = 3.76; B 77,951 s / 6,625 s = 11.8; C 12.7 |
| minimap2 batches of "about 1,780 sequences" | Confirmed in `A.rep1/bismark.log` |

Minor nits:
- **Low, index-build time:** `RESULTS.md` says "index build took about 74 s". The log shows `loaded/built the index … 69.233` s; 74 s is when the first batch is mapped. Either "about 70 s" or "the first batch after 74 s". The 0.5% conclusion does not change.
- **Low, wall time:** the "about 3.8 of 32 threads busy" figure divides by wall time. `RESULTS.md` itself excludes wall times because of host contention (load up to ~1070), and A ran at a different time from B and C. The conclusion is hedged ("likely"), and the `-K` batch-size evidence from the log supports it independently. Still, a half-clause noting that busy-thread counts share the contention caveat would be consistent.
- **Low, docs wording:** "it needs no index build" sits beside `RESULTS.md`'s "index build took about 74 s". "Needs no prebuilt index" says what is meant.

## 5. Honesty about the oxy data loss

`RESULTS.md` is honest about the two items it names: the dry-run score file, and the `@PG` check that could not be applied retroactively. Both are stated plainly, and the dry-run scorer value is preserved in `PLAN.md`. Committed evidence remains for every headline number, and the decision does not depend on anything lost.

The account is incomplete in one respect (Low/Medium, wording only). It does not say that the screen's **CpG reports and BAMs** are gone along with the score file. The consequences are not stated:
- `score.tsv` was produced by the **pre-review** scorer, with the guards then in bash;
- it cannot be re-scored with the revised `five_base_compare.py` without re-aligning from the S3 inputs.

The scorer changes do not alter these numbers:
- the newline fix affects only 6-column input, and the screen inputs were 7-column;
- the moved guards are semantically identical and passed in bash;
- duplicate sites would not arise in Bismark or DRAGEN reports.

Two sentences in the validation table would close this.

Similarly, "n differs only by the excluded chrY/MT/scaffolds" (9,501 vs 9,492) can no longer be checked, because the dry-run files are gone. Say it was checked before the wipe, or soften it to "consistent with".

The determinism claim's md5s survive in `PLAN.md:396`. Good.

## Fixes applied

None (report-only review).

## Prioritised recommendations

**Critical / High:** none.

**Medium:** none.

**Low**
1. `RESULTS.md` validation table: state that the screen CpG reports and BAMs were also lost, that `score.tsv` comes from the pre-review scorer, and why the scorer changes do not alter it. Soften or date the "n differs only by excluded contigs" claim.
2. `five_base_engine_bench.sh` selftest: add `expect fail` cases for A without `-x sr`, B-loc without `--reorder`, and `--local` in B-X/C.
3. `five_base_compare.py` selftest: assert the `"empty common set"` message.
4. `check_pg_line`: match `$(arm_flags)` as one contiguous substring, not token by token.
5. `cmd_prep`: `cx=$(…) || die …` so a missing non-control CX report fails with a message.
6. `sample_tree`/`cmd_cell`: die if the `tree.tsv` peak is 0.
7. `RESULTS.md`: index build about 70 s (not 74); note that busy-thread counts share the contention caveat; add the 0.003% read-through for B and B-X.
8. Docs: "needs no prebuilt index" in place of "needs no index build".
9. `CHANGELOG.md`: add the PR link once the PR is opened.

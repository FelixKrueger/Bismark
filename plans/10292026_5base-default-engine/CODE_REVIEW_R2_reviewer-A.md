# Code review round 2 (Reviewer A): 5-Base default engine, outcome (a)

**Target:** the uncommitted fixes on `feat/5base-default-engine`, applied after round 1 (`CODE_REVIEW_reviewer-A.md`). See PLAN.md "Review round — 2026-11-01".
**Date:** 2026-11-01.

## Summary

The round-1 findings are resolved. I found no regression that affects correctness. Every number in the docs table and the changed RESULTS rows matches `score.tsv` and the per-arm `metrics.tsv`. The memory values are KiB / 1,048,576.

What remains is small:
- one honesty gap in RESULTS about what the oxy wipe cost;
- one new untested causal claim (minimap2 threads);
- a few Low-priority harness tightenings.

**Checks run:**
- `python3 -I validation/five_base_compare.py --selftest`: OK.
- `bash validation/five_base_engine_bench.sh selftest`: 13 passed, 0 failed.
- `bash -n`: clean.
- `cargo test -p bismark --test aligner_cli five_base`: 14/14.
- `--lib five_base_notice`: 1/1.
- `cargo fmt -p bismark -- --check`: clean.

**Extra checks I added:**
- I ran the real `verify_options` against the five committed screen `*_PE_report.txt` files, including the new `-x sr` (A) and `--reorder` (B-loc) requirements. All five pass.
- I ran `check_pg_line` on a `CL:` built exactly the way `cmd_cell` invokes bismark (format from `aligner/output.rs:105`). All five arms pass.
- I ran `tree_pids` against a stubbed `ps` tree. The descendant BFS is correct, including a leaf root and an absent root.

## Round-1 findings: status

| Round-1 finding | Status |
|---|---|
| RESULTS "+ `@PG`" claim unbacked | **Resolved.** The row now says the report line was checked and the `@PG` check came later, after the BAMs were lost. `verify_pg` now exists. |
| RESULTS B-loc "lowest accuracy" / minimap2 "highest recall" | **Resolved.** B-loc is lowest on own-set r (0.9487), gain-set r (0.8777) and precision (88.1). Recall is 61.3 vs 61.6. Both are now correct. |
| HISAT2 "similar to Bowtie 2" | **Resolved.** The TLEN match (243/352/478 vs 241/352/478) is stated, and the cause is left open. |
| Docs: mixed denominators | **Resolved.** The lead-in now separates the shared r set from each engine's own count. |
| Docs: screen r vs contract 0.998 | **Resolved** by the "about 5× per strand… high-coverage tail" sentence. RESULTS says 4.7×, which is consistent. |
| Docs: "100M trimmed pairs" | **Resolved:** "first 100M read pairs … 98.1M pairs after trimming". |
| Docs: GB/GiB | **Resolved.** 16,295,504 → 15.54, 8,763,276 → 8.36, 8,785,360 → 8.38, 8,794,284 → 8.39, 8,683,540 → 8.28 GiB. The table and RESULTS round correctly. |
| Docs: memory "because" / "typical fragments" | **Resolved.** Both were removed or softened ("can cut off the longest fragments"). |
| `cmd_score` DONE / single report | **Resolved, with one gap (L3).** `DONE` is required and the CpG report must be unique. |
| Cross-arm read parity (Behavior 7) | **Resolved.** `pairs_in` (metrics col 5) must be identical across arms. |
| `verify_options` gaps (`-x sr`, B-loc) | **Resolved.** B-loc now requires `--reorder` rather than denying `--maxins`, which is a reasonable choice. |
| `setsid` / `wait` hazard | **Resolved** by removing `setsid` and walking descendants. `$!` is the forked child that execs taskset → time, so bismark and the aligner are its descendants. |
| Missing commit hash under pipefail | **Resolved:** `|| true` then `die`. |
| Multiple DRAGEN CX files | **Resolved:** `wc -l` must be 1. |
| `load_cpg` newline | **Resolved.** `rstrip("\n")`, and the self-test loads a 6-column file. |
| Selftest: no independent r check | **Resolved.** I hand-checked xs/ys/ws against the A own-set cov ≥ 1 rows: weights min(10,10), min(12,12), min(4,12), min(3,3). They are right, so this is a real independent check against `concordance.pearson` / `weighted_pearson`. |
| Selftest: no refcheck negative | **Resolved.** The off-by-one `+` at chr1:11 hits a G, giving 1/2 = 50% → fails. |
| V6 truncated-contig guard untested | **Resolved.** Arm T has no chr1 rows, so the "joins no cov>=10 CpG on contig 1" guard fires. |
| V1 injected-option failure | **Resolved.** The bash selftest has 8 failure cases. Three of them (A `-p 8`, B-X without `-X 800`, C without `--illumina_5base`) can only be caught by `check_pg_line`, so the `@PG` path is genuinely exercised. |
| CLI test: minimap2 absent | **Partly resolved (L1).** |
| Dry-run 0.7896 provenance | **Partly resolved (M1).** |
| CHANGELOG link | Open. Fine until the PR exists. |
| `rep1` hard-coded in `cmd_score`; memory note in the docstring | Not addressed (Low, acceptable). |
| PROGRESS.md Implementation row | **Still stale (L6).** |

## New findings

### Medium

**M1. RESULTS understates what the oxy wipe cost.**
- **What RESULTS says.** It mentions the lost screen BAMs and the lost dry-run score file, each in one validation row.
- **What it leaves out:**
  - **The per-arm CpG reports are gone too.** `cmd_cell` never copies them to `$RESULTS`, and the tarball has none. So `score.tsv` cannot be recomputed without rerunning all five alignments.
  - **The committed `score.tsv` came from the pre-fix scorer.** The new guards (duplicate sites, empty common set, `--min-joined` in Python) were never run on the screen data. The guards change no values, because Bismark and DRAGEN reports have no duplicate sites, so the numbers stand. But the reader should know.
  - **The 0.789631 / n = 9,492 figure now exists only in this round's uncommitted PLAN notes,** written after the file was lost. RESULTS calls this "the plan's iteration log". The source should be named (session transcript? terminal scrollback?). The claim that "n differs only by the excluded chrY/MT/scaffolds" can no longer be checked.
  - **"Raw outputs: `bench_results/NA12878/screen/`"** now overstates what is there. It holds summaries and logs, not raw outputs.
- **Suggested fix:** add one short Caveats bullet, "Data retention", that lists:
  - what survives: metrics, reports, logs, `score.tsv`, inputs in S3;
  - what was lost: BAMs, CpG reports, the dry-run scores, HG002 A outputs;
  - the consequences: no re-scoring or `@PG` check without a rerun, and the screen was scored before the scorer's new guards.
- **The decision is not affected.**

**M2. New untested causal claim on minimap2 threading (RESULTS "Read fate and cost", plus the paragraph under the cost table).**
- **What RESULTS says:** "only about 3.8 of its 32 threads … so its configuration, not the engine, likely limits its speed".
- **The arithmetic is right.** 54,957 / 14,611 = 3.76. Over minimap2's own real time (13,749 s in its log) it is 4.0.
- **The batch size checks out.** The log shows `mapped 1780 sequences` per batch, and 250K / 1,780 ≈ 140 bp.
- **The index build checks out.** It takes about 74 s.
- **The causal claim is not established, for two reasons:**
  - It derives a throughput figure from wall times that the next paragraph says were excluded as unreliable: two concurrent streams, and a host load average of about 1070 from other tenants.
  - Batch size is a hypothesis, not a measurement. The Rust side feeding and draining minimap2 is an equally plausible limiter.
- **This is the same class of claim** this round removed from the docs (the memory "because").
- **Suggested fix:** "about 3.8 of 32 threads busy under the screen's contended host; a small `-K` is one candidate cause, untested". The `-K` follow-up is the right next step and already says it needs its own concordance check.
- Also, "about 12 busy threads for Bowtie 2 … and HISAT2" is imprecise: B is 11.8, B-X 12.3, C 12.7. "12–13" is closer.

### Low

**L1. CLI test (`tests/aligner_cli.rs:7551-7565`).**
- The assertion checks only the first `Note: --illumina_5base` line.
- A second, stray "aligned with minimap2" notice would still pass. That was the round-1 concern.
- `assert!(!stderr.contains("aligned with minimap2"))` covers it.
- Otherwise the test is correct and pins the Bowtie 2/HISAT2 names end to end.

**L2. `verify_pg` (`five_base_engine_bench.sh:82-86`).**
- **SIGPIPE false failure.** `samtools view -H | grep -m1` under `pipefail` can fail falsely: grep exits on the first match, and samtools gets SIGPIPE (141) if it is still writing.
  - The risk is small. The Bismark `@PG` follows the `@SQ` lines, and the GRCh38 primary assembly header is about 194 `@SQ` lines, well under a pipe buffer.
  - But it is a latent false-`die` on a larger reference.
  - Fix: drop `-m1`, or use `awk '/^@PG\tID:Bismark/{print; exit}'` on `samtools view -H … > file`.
- **No deny list.** `check_pg_line` has none, so for arm A (empty `arm_flags`) a `CL:` containing `--bowtie2` passes.
  - The report's "Bismark was run with minimap2" check already catches the wrong engine, so this is only belt-and-braces.
- **`-X` and `800` are checked as separate tokens**, not as an adjacent pair. That is acceptable.

**L3. `cmd_score` DONE check.**
- `[[ -s $d/DONE ]]` proves that the cell finished. It does not prove that it finished under the current commit and flags.
- `cmd_cell` does `rm -rf "$d"` before rerunning, so partial cells cannot survive. Stale-but-complete cells can.
- A cheap tightening: compare `$(cat $d/DONE)` with the `key` column of `metrics.tsv`.
- Also, an empty `pairs_in` in every arm would pass the parity check, because one unique empty value counts as a match. `[[ -n ]]` per value would close that.
- `cmd_cell:201` still uses `ls … | head -1` for the CpG report, which is inconsistent with `cmd_score`'s uniqueness check. This is cosmetic.

**L4. Python self-test.**
- The "empty common set" guard fires in the T case but is never asserted on its own, because only the contig message is checked.
- Add `assert "empty common set" in str(e)`, or a dedicated case.

**L5. `sample_tree`.**
- At process exit, `ps -p <dead pids>` exits 1, and under `set -euo pipefail` that ends the sampler subshell.
- This is harmless: `$out` is rewritten every iteration, and the final minimap2 CPU (54,957) equals minimap2's own report, so no tail was lost.
- The behaviour is the same as the old `ps -s` version. No action needed.

**L6. Housekeeping.**
- PROGRESS.md's Implementation row still says "docs/notice change awaits Felix", although the change is done.
- The CHANGELOG entry is one sentence ✓ and still has no link (add the PR link when it exists).

## Docs "Choosing an engine": number check

| Engine | Map eff | common r | own cov ≥ 10 | peak GiB | Docs |
|---|---|---|---|---|---|
| minimap2 | 95.2 | 0.964638 → 0.9646 | 817,312 | 15.54 → 15.5 | ✓ |
| Bowtie 2 | 87.9 | 0.963133 → 0.9631 | 554,655 | 8.36 → 8.4 | ✓ |
| Bowtie 2 `-X 800` | 94.5 | 0.964342 → 0.9643 | 822,432 | 8.39 → 8.4 | ✓ |
| Bowtie 2 `--local` | 91.2 | 0.963544 → 0.9635 | 580,613 | 8.38 → 8.4 | ✓ |
| HISAT2 | 91.3 | 0.963122 → 0.9631 | 664,790 | 8.28 → 8.3 | ✓ |

The prose checks out as well:
- "about 7 points" is 7.3.
- "a third" is 32.1%.
- "about twice the peak memory" is 1.86×.
- n = 400,760 ✓.

The RESULTS screen and read-fate tables also match: MAPQ-0, TLEN, duplicates, read-through, and deconvolution P/R.

## Fixes applied

None (report only).

## Recommendations

- **Critical / High:** none.
- **Medium:**
  - M1: add a data-retention caveat to RESULTS.
  - M2: qualify the minimap2-threads causal claim.
- **Low:**
  - L1: stderr-wide minimap2 assert.
  - L2: `verify_pg` SIGPIPE.
  - L3: DONE key vs metrics key, and a non-empty `pairs_in`.
  - L4: assert the empty-common-set guard.
  - L6: PROGRESS row.

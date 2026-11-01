# Plan Coverage Report — re-audit (round 2)

**Mode:** B (code and results vs plan)
**Plan(s):** `PLAN.md` rev 1 (Behavior 1–9, Implementation outline 1–8, Validation 1–6), `PREREGISTRATION.md`, `RESULTS.md`
**Branch:** `feat/5base-default-engine` in `/Users/fkrueger/Github/Bismark-5base-default`, HEAD `69b6b90` plus the uncommitted review-round changes (`git diff`)
**Previous audit:** `COVERAGE.md` (2026-10-30, INCOMPLETE: #13, #15, #21, #37 PARTIAL; #40, #46 MISSING)
**Date:** 2026-11-01
**Verdict:** COMPLETE — all 46 items DONE, DEVIATED-with-documentation, or N/A. All six earlier gaps are resolved.

Path-dependence: the screen chose **outcome (a)**, and `RESULTS.md` applies the pre-registered rule correctly (re-derived below from `bench_results/NA12878/screen/score.tsv` and each arm's `metrics.tsv`). Behavior 9 (outcome (c)), the outcome-(b)-only parts of Behavior 8, and items that exist only for the confirm stage are therefore N/A.

**Re-derivation of the rule (independent of RESULTS.md).** Common-set r at cov ≥ 10 (n = 400,760): A 0.964638, B 0.963133 (Δ −0.00150), B-loc 0.963544 (−0.00109), B-X 0.964342 (−0.00030), C 0.963122 (−0.00152). Only B-X passes rule 1. B-X: ΔCpG +0.63% (822,432 vs 817,312), so rule 3 also fails; MAPQ-0 0.92% vs A 3.69%; CHG/CHH 1.3/1.2 vs 1.3/1.1; gain-set r 0.937410 < 0.944638 (= A common r − 0.02), so the guard fails. Deconvolution P/R 93.5/61.6 vs 94.2/61.3 (within −1 pt). No arm qualifies → no confirm stage → outcome (a), as `PREREGISTRATION.md` specifies. `PREREGISTRATION.md` is unchanged since `425d42b` (2026-10-29 16:28 +0100). Read counts: 98,082,139 pairs in all five arms.

## Summary

- Total items: 46
- DONE: 33
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 10 (all documented)
- N/A: 3

## Status of the six earlier gaps

| # | Earlier | Now | Basis |
|---|---|---|---|
| 13 | PARTIAL | DEVIATED (documented, acceptable) | `check_pg_line`/`verify_pg` added and wired into `cmd_cell` after `verify_options`; RESULTS.md no longer claims "report + `@PG`" and states that the screen BAMs were lost before the check could be applied. See the judgement below. |
| 15 | PARTIAL | DONE | RESULTS.md reports minimap2's index build (~74 s, 0.5% of the run). Checked against `A.rep1/bismark.log`: `[M::main::69.233*1.25] loaded/built the index`, `mm_idx_stat` at 73.740 s, first batch at 74.05 s. Wall time stays excluded (documented, Efficiency). |
| 21 | PARTIAL | DONE | `cmd_score` collects `pairs_in` (column 5 of each `metrics.tsv`, confirmed) per arm and dies with "arms analysed different read counts" unless all are equal. It also requires each cell's `DONE` marker and exactly one CpG report. |
| 37 | PARTIAL | DONE (PR is the next step) | fmt, focused tests and release Clippy re-run here and pass; full suite recorded in the implementation notes (2324 passed, 0 failed); dual code review done (report files exist, not read per instructions); this file is the plan-manager step. Opening the PR into `dev` follows this audit. |
| 40 | MISSING | DONE | `five_base_engine_bench.sh selftest` feeds the real options of every arm (5 pass cases) and 8 injected wrong ones (wrong `-t`, wrong `-p` in `@PG`, missing `--reorder`, spurious `--maxins`, B-loc without `--local`, B-X without `-X 800` in `@PG`, doubled `--no-spliced-alignment`, missing `--illumina_5base`); all 8 are rejected. Run here: "13 passed, 0 failed". It exercises the same `verify_options`/`check_pg_line` functions the cells use, rather than a dry-run cell; the plan notes record this. |
| 46 | MISSING | DONE | The per-contig guard moved into `five_base_compare.py compare` (fails the score via `SystemExit`, which `set -e` propagates in `cmd_score`). The self-test feeds an arm with no CpG on contig 1 and asserts "T joins no cov>=10 CpG on contig 1"; it also covers the joined-fraction, empty-common-set, duplicate-arm and duplicate-site guards. Run here: "selftest OK". |

### Judgement on the lost screen BAMs (`@PG` not applied retroactively)

Acceptable, documented deviation; not a gap.

- It is documented in both places a reader would look: `RESULTS.md` (Validation checks, "Options actually applied") and `PLAN.md` ("Review round — 2026-11-01", data-loss bullet).
- The plan's purpose for Behavior 4 is that every arm ran with its intended engine options. That is established without `@PG`. Each committed `*_PE_report.txt` carries the engine name and the options line Bismark passed to the engine, and the harness verified it during the run. Re-applying the *current, stricter* `verify_options` (which now also requires `-x sr` for A and `--reorder` for B-loc) to the five committed reports passes for every arm (run here by sourcing the function from the harness). The `bismark.log` files also show the resolved `aligner_options`.
- The data corroborate that the arm-specific options took effect: B-X's TLEN p90 is 479 vs B's 445 and its mapping efficiency 94.5% vs 87.9%; B-loc shows soft-clip read-through (919,122) that end-to-end B (2,937) lacks.
- `@PG` would add only the Bismark-level flags (`--illumina_5base`, `-p 32`), and both are already implied by the report header ("against the unconverted genome", `-p 32`/`-t 32` in the engine options).
- The loss was caused by the pod restart, not by a choice; the check now runs on every future cell. The `@PG` line format the checker expects (`@PG\tID:Bismark\tVN:…\tCL:"bismark …"`) matches `rust/bismark/src/aligner/output.rs:105`/`:1629`.

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | Pre-registration frozen and committed before any challenger runs | B1, O2 | DONE | `425d42b` 2026-10-29 16:28 +0100, before the first challenger output; unchanged since (`git diff HEAD` empty). |
| 2 | Decision rule applied correctly in RESULTS.md | B1, PREREG | DONE | Re-derived above; matches RESULTS.md's table. |
| 3 | Bismark commit and tool versions, minimap2 `-F` default | B2.1 | DONE | `9b5c15b`; minimap2 2.31-r1302, Bowtie 2 2.5.5, HISAT2 2.2.2, Trim Galore 2.2.0; `-F` 800. `cmd_cell` now dies if `bismark --version` has no commit hash. |
| 4 | One reference, DRAGEN naming, CpG-position check ≥ 99.9% | B2.2, V2, O4 | DONE | `refcheck_*.txt` 100.0000% both samples; refcheck's failing path now self-tested (50% → "below the"). |
| 5 | Indexes built from the same FASTA, build time/RSS recorded | B2.2, O4 | DEVIATED | Documented (iteration log 1, 5): shared prebuilt indexes, names and lengths checked. `PREREGISTRATION.md` still says "built from this same file"; it cannot be edited without voiding the run, and PLAN.md records the deviation. |
| 6 | Full FASTQ download, completeness, md5s, input/DRAGEN ratio, one trim | B2.3, O1 | DEVIATED | Documented: screen lanes only; full depth was confirm-only. |
| 7 | Screen subset: first ~100M trimmed pairs, same files for every arm | B2.4 | DEVIATED | Documented in PREREG: subset before trimming (100M → 98,082,139). Docs now say so too. |
| 8 | Alignment cell: `-p 32`, `taskset`, `time -v`, process-tree RSS, aligner CPU | B3.1 | DONE | `sample_tree` now walks the launched PID's descendants (`tree_pids`) instead of `ps -s` + `setsid`. |
| 9 | `deduplicate_bismark -p` | B3.2 | DONE | Dedup reports per cell. |
| 10 | Extractor, CpG-only cytosine report | B3.3 | DONE | — |
| 11 | Three `concordance.py methyl` runs (own/common/gain) | B3.4 | DEVIATED | Documented: one-pass `five_base_compare.py compare` reusing `concordance.py`'s parser; self-test now checks r and weighted r against `concordance.py`'s `pearson`/`weighted_pearson`. |
| 12 | `concordance.py deconv` vs DRAGEN VCF | B3.5 | DONE | — |
| 13 | Option verification from report header and BAM `@PG` | B4 | DEVIATED | Documented. Report check ran during the screen; `@PG` check now in the harness but not applicable to the lost screen BAMs. Judged acceptable above. |
| 14 | Accuracy, sensitivity, read-fate, specificity metrics | B5 | DONE | — |
| 15 | Cost: wall, tree RSS, aligner CPU, minimap2 per-run index build separately, one-off builds | B5 | DONE | Index build ~74 s now reported; aligner CPU and threads-busy reported; RSS in GiB (15.5 = 16,295,504 KiB). Wall-time exclusion and one-off builds documented (#5). |
| 16 | NA12878 screen: A, B, B-loc, B-X, C | B6, O5 | DONE | — |
| 17 | HG002 screen: A and leading challenger | B6 | DEVIATED | Documented: no challenger qualified on NA12878, and the rule requires both samples. |
| 18 | Confirm skipped when no challenger passes; PR says so | B6, O6 | DONE | — |
| 19 | Run bookkeeping, off-box copies, 150 GB disk guard | B7 | DONE | — |
| 20 | BAMs and cytosine reports retained until merge | B7 | DEVIATED | Documented twice: deduplicated BAMs deleted by design (iteration log 7); the remaining screen BAMs lost to the oxy restart (review-round notes, RESULTS.md). Durable results remain in `bench_results/`, oxy `~/5base_results` and S3 inputs. |
| 21 | Abort guards: joined < 50%, zero per-contig, read-count mismatch | B7 | DONE | Joined and per-contig guards in `five_base_compare.py` (`--min-joined`, passed from `MIN_JOINED`); read-count parity in `cmd_score`. Guards act at scoring rather than per cell, which is where the cross-arm comparison exists. |
| 22 | Docs name minimap2 behind published numbers; "when to choose" revised | B8 | DONE | `docs/src/content/docs/rust/illumina-5-base.md`: stability contract and "Running it". |
| 23 | Docs comparison table: common-set r, CpGs, wall, RSS | B8 | DEVIATED | Documented: no wall time. Lead-in now says 100M pairs before trimming (98.1M after) and separates the shared r set from each engine's own CpG count; values match `score.tsv`/`metrics.tsv`. |
| 24 | CHANGELOG one sentence | B8 | DONE | One sentence; it has no issue/PR link yet (none exists). |
| 25 | No new pin test; existing pin | B8 | DONE | `illumina_5base_resolves_to_minimap2` passes. |
| 26 | Startup notice engine-correct | B8 | DONE | `five_base_notice(aligner)`; unit test passes; `five_base_1134_default_argv_is_exact` now asserts the Bowtie 2/HISAT2 notice names the engine and does not mention minimap2 (passes). |
| 27 | Outcome (b) | B8 | N/A | Outcome (a). |
| 28 | Outcome (c) | B9 | N/A | Outcome (a). |
| 29 | No release cut in flight | B9 | DONE | No version files in the diff. |
| 30 | BaseSpace auth, download, verify | O1 | DONE | — |
| 31 | PREREGISTRATION.md from B1, committed | O2 | DONE | See #1. |
| 32 | Harness + common-set filter, `metrics.tsv`, 10M first | O3 | DONE | bash instead of POSIX sh (documented). |
| 33 | Run screen, present table and challenger | O5 | DONE | — |
| 34 | RESULTS.md: tables, rule, caveats | O6 | DONE | B-loc and recall statements now match `score.tsv`/`metrics.tsv` (B-loc lowest own-set r 0.9487, gain r 0.878, precision 88.1; A recall 61.3 vs B-X 61.6). HISAT2 loss no longer attributed to the insert cap; memory cause marked unmeasured. |
| 35 | Stop and present decision; implement after trigger | O7 | DONE | — |
| 36 | fmt, focused tests, release Clippy | O8 | DONE | Re-run here; see Test verification. |
| 37 | Full suite, dual code review, plan-manager audit, PR | O8 | DONE | See the gap table above; PR follows this audit. |
| 38 | Dry run: repeats deterministic, option verification passes | V1 | DONE | Byte-identical CpG-report md5s from the iteration log; `metrics.tsv` identical across repeats. |
| 39 | Dry run: wall time within 10% | V1 | DEVIATED | Documented: failed under host load; timing demoted to tie-breaker. |
| 40 | Injected wrong option fails verification | V1 | DONE | Harness `selftest`: 8 injected wrong options rejected, 5 real ones accepted. |
| 41 | Reference check ≥ 99.9% | V2 | DONE | See #4. |
| 42 | Same input: identical read counts; md5s per cell | V3 | DONE | Counts identical and now enforced by `cmd_score`; md5s in `input.md5` and each cell's DONE key. |
| 43 | Common-set filter toy test | V4 | DONE | `five_base_compare.py --selftest`. |
| 44 | r recomputed independently on 100k random CpGs, 3 dp vs `concordance.py` | V4 | DEVIATED | Documented: full dry10M arm A (n ≈ 9.5k) rather than a 100k sample; 0.7899 vs 0.7896. The scorer-side file was lost with oxy's scratch disk; its value (0.789631, n = 9,492) is recorded in PLAN.md. |
| 45 | Baseline sanity at full depth | V5 | N/A | Full-depth arm A only; no confirm stage. |
| 46 | Truncated contig triggers per-contig abort | V6 | DONE | Self-test case "T joins no cov>=10 CpG on contig 1". |

## Test verification (Mode B)

Run 2026-11-01. Cargo commands from `/Users/fkrueger/Github/Bismark-5base-default/rust` (sandbox disabled), scripts from the worktree root.

| Check | Result |
|-------|--------|
| `cargo fmt --all -- --check` | PASS (exit 0) |
| `cargo test -p bismark --lib aligner::` | PASS: 612 passed, 0 failed |
| `cargo test -p bismark --test aligner_cli` | PASS: 139 passed, 0 failed |
| `cargo clippy --workspace --all-targets --release -- -D warnings` | PASS (exit 0) |
| `python3 -I validation/five_base_compare.py --selftest` | PASS ("selftest OK") |
| `TMPDIR=<scratchpad> bash validation/five_base_engine_bench.sh selftest` | PASS ("13 passed, 0 failed") |
| `python3 -I validation/concordance.py --selftest` | PASS |
| `aligner::tests::five_base_notice_names_the_engine_that_runs` | PASS (individually) |
| `five_base_1134_default_argv_is_exact` | PASS (individually) |
| `aligner::config::tests::illumina_5base_resolves_to_minimap2` | PASS (individually) |
| Current `verify_options` re-applied to the 5 committed screen `*_PE_report.txt` | PASS for A, B, B-loc, B-X, C |
| Full `bismark` suite | Not re-run here; recorded in PLAN.md as 2324 passed, 0 failed, 20 ignored. The Rust change since that run is one `aligner_cli` assertion, covered by the run above. |
| oxy benchmark cells | Not re-executable; verified from recorded artifacts in `bench_results/` |

## Verdict

COMPLETE. All six gaps from `COVERAGE.md` are resolved: #15, #21, #40, #46 are now DONE, #37 is DONE up to opening the PR, which is the next step, and #13 is a documented, acceptable deviation. The outcome (a) code, docs and harness changes match the plan, and `RESULTS.md` applies the pre-registered rule correctly.

Minor notes, not gaps:
- `PREREGISTRATION.md` still says the indexes were built from the FASTA (#5). It is frozen by design, and the deviation is in PLAN.md.
- The CHANGELOG entry needs its PR link once the PR exists (#24).
- `PROGRESS.md` should be updated to reflect this re-audit.

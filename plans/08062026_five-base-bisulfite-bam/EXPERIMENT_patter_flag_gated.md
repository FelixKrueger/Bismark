# Experiment — flag-gated `--five_base` patch: default path untouched (check 2)

**Date:** 2026-08-08 session (dated 2026-08-07 clock) · **Context:** [#787](https://github.com/FelixKrueger/Bismark/issues/787), upstream PR draft `DRAFT_upstream_wgbs_tools_PR.md` · **Verdict: check 2 PASSES at three levels; checks 1/4 carry over by observed byte-identity.**

Follow-up to `EXPERIMENT_patter_swap.md`. That experiment proved the *unconditional* constant swap
correct on 5-Base data. This one turns it into the real, submittable patch — `--five_base` threaded
through `patter`'s arg parsing and `bam2pat.py` — and proves the **default path is byte-identical
with the patch present but not enabled** (the "we broke nothing" evidence).

## The patch (final, validated form)

Two files, +15/−3. `wgbs_tools` @ `master` (`6f24bed`).

- **`src/pipeline_wgbs/main.cpp`** — parse `--five_base`; when set, override the public members
  after construction: `p.OT = ReadOrient{'T','C',0,0}; p.OB = ReadOrient{'A','G',1,1};` plus the
  usage string. Deliberately **no `patter.h`/constructor change**: the only construction site is
  `main.cpp`, so the entire behaviour change sits inside the flag's `if` — the default path is
  unchanged *by inspection*, then proven below by measurement.
- **`src/python/bam2pat.py`** — argparse `--five_base` (store_true), `proc_chr(..., five_base=False)`
  kwarg, `patter_cmd += ' --five_base '` under the flag, and the positional tuple in
  `Bam2Pat.start_threads` gains `self.args.five_base`.

Full diff saved as `five_base_wgbs_tools.patch` next to this file (reverse-apply-checked against
the tested tree).

## Results

| Level | Input | Patched, flag OFF vs stock | Patched, flag ON vs stock |
|---|---|---|---|
| `patter` standalone | synthetic pUC19 PE bisulfite, 41 pat lines, mixed methylation (C=220/T=210) | **byte-identical** | exact C↔T inversion, 0/41 line mismatches |
| `patter` standalone | **real WGBS** (SRR24827378 10M PE, chr21: 208,304 records → 59,250 pat lines) | **byte-identical** | exact per-line inversion, 0/59,250 mismatches (80.7 % → 19.3 % meth) |
| full `wgbstools bam2pat` CLI | pUC19 fixture (init_genome'd as `chr1`), sorted+indexed BAM | **byte-identical** decompressed `.pat.gz` (patched vs **stock `bam2pat.py`**, same binary — isolates the Python edits) | inverted multiset, 33/33 lines |

**Checks 1/4 carry over by observation, not argument:** a binary built with the *unconditional*
constant swap (the exact `EXPERIMENT_patter_swap.md` configuration, validated 0 %→100 % on 5-Base
data) produces output **byte-identical** to the flag-gated binary run with `--five_base`, on both
the synthetic fixture and real chr21 data.

Compiler determinism held: rebuilding the flag-gated source reproduced a byte-identical binary,
so the restore-and-rebuild in the middle of the experiment did not change what was tested.

## Method notes

- Mixed methylation matters: a fully-methylated fixture would never exercise the UNMETH branch
  (`patter.cpp:153`), leaving half the polarity surface untested by the byte-identity gate.
  Generator: `gen_pe_bisulfite.py` (scratch; pattern in this file), alternating CpGs methylated,
  21 OT + 21 OB molecules from pUC19, R1/R2 = two ends of ONE converted molecule (G17).
- One OB pair (`m002_OB_0`, mate at POS 1) was absent from Bismark's output despite the report
  counting 42 unique pairs — a Perl boundary quirk at chromosome start, not chased; 41 pairs suffice.
- Real-data leg: stream-filter the unsorted BAM (`samtools view -q 10 -F 1796 -f 3 | awk '$3=="21"'`)
  into a file once, feed the identical bytes to all binaries — no sort/index needed for the
  standalone comparison. chr21 CpG dict built from the Ensembl primary assembly (462,299 CpGs),
  `bgzip` + `tabix -s 1 -b 2 -e 2`.
- `wgbstools init_genome` **rejects non-chromosome contig names** (`is_valid_chrome`,
  `init_genome.py:278-281`, wants `^(chr)?(\d+|[XYM]|MT)$`) — symptom is
  "No objects to concatenate". Fixture genome renamed to `chr1` for the end-to-end leg.
- `bam2pat` runs used `--no_beta`: only `patter`/`match_maker` binaries were compiled, and the
  beta step 127s on the missing `stdin2beta` *after* the `.pat.gz` is complete.
- **`patter` flags must FOLLOW the two positionals** (`patter DICT REGION [flags]` — `InputParser`
  doesn't reorder; a leading flag becomes the dict path and the error surfaces as
  `tabix: unrecognized option`). First positive-control run produced an EMPTY file — "differs" that
  would have vacuously passed (G19); the per-line inversion assertion is what made it a real check.

## Scope finding for the PR

`add_cpg_counts.h:61-62` duplicates the same `ReadOrient OT{'C','T',0,0}; OB{'G','A',1,1};`
constants (the dual-driver pattern). `wgbstools bam2pat` does not use it; the patch does not touch
it. Noted in the PR body so the maintainer can decide whether `add_cpg_counts` should grow the same
flag.

## Status after this experiment

All four validation checks are satisfied: 1 & 4 (`EXPERIMENT_patter_swap.md` + carry-over cmp
above), 2 (this experiment), 3 (effectively — the reporter's real-data UXM confirmation on #787,
comment `5219193146`). The CLI plumbing has been read and the sketch replaced by the validated
patch. **The PR is technically ready; submission awaits Felix's go-ahead.**

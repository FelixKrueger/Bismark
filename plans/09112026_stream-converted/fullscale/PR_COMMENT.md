## Full-scale validation complete — all three claims pass

Ran [`FULLSCALE.md`](https://github.com/ewels/Bismark/blob/rust/stream-converted/plans/09112026_stream-converted/fullscale/FULLSCALE.md) on a 32-core / 124 GB machine. **52 benchmark runs, strictly one at a time.**

Full record, per-run time series and plots: [`fullscale/RESULTS.md`](https://github.com/ewels/Bismark/blob/rust/stream-converted/plans/09112026_stream-converted/fullscale/RESULTS.md) · summary in [`BENCHMARKS.md` §3](https://github.com/ewels/Bismark/blob/rust/stream-converted/plans/09112026_stream-converted/BENCHMARKS.md).

**Setup.** `SRR24766921` (mouse RRBS PE, 51 bp), first 10M pairs plus a 2M subset, against Ensembl 112 GRCm39. Each arm in its own container so cgroup v2 counters cover the whole process tree; page cache warmed identically before every run; arm order alternated between reps; a ~1 s time series of disk, memory, CPU and threads per run.

| | claim | result |
|---|---|---|
| **C1** | output byte-identical | **PASS** — 25 arm-pairs, 0 differing |
| **C2** | converted temp files gone | **PASS** — 0 KB vs up to 6.38 GiB |
| **C3** | no wall-time regression | **PASS** — never slower |

### C2 — the disk result

| shape | scale | streamed | files |
|---|---|---|---|
| directional | 10M | **0 KB** | 3.19 GiB |
| non-directional | 10M | **0 KB** | 6.38 GiB |
| `--multicore 2` | 10M | **0 KB** | 3.19 GiB |

Zero, not "small" — and the file arm holds that for the *entire* 50-minute run, not transiently. Sampled live, since both arms clean up at the end.

### C3 — wall time

−1.50 % (`-p 2`), −0.66 % / −0.45 % (directional 2M / 10M), −0.65 % / −0.55 % (non-directional 2M / 10M), −0.01 % (`--multicore 2`). **Negative or zero everywhere, at both scales.** The `--multicore` dead heat is a useful control — a genuine null looks like that, which argues the rest isn't the harness favouring an arm.

Honest headline is still **wall-neutral to slightly faster**. But the pre-merge worry was a *regression* from coupling the instances, and there is none.

### The asymmetry question (#1120 D-6, `BENCHMARKS.md` "known measurement gap") — closed

The coupling is real and large. On directional 10M:

| | CPU | instances finish |
|---|---|---|
| **files** | 800 % until t=1761 s, then **400 % for the last 1194 s** | 20 min apart |
| **streamed** | flat **645 %** | together |

With files the strand hypotheses fully decouple and the machine idles at half utilisation for the last 40 % of the run. Streaming paces the fast instance to the slow one — exactly the throttling that was the concern — and **it costs nothing**: total CPU 18,990 vs 18,993 CPU-seconds, and the streamed arm finished 8.9 s sooner. Wall time is set by the slowest instance either way.

**`CHANNEL_DEPTH` untouched, and on this evidence does not need raising.**

### §7 — the premise, demonstrated for the first time

`FULLSCALE.md` §7.2 notes this is "the whole point of the feature, and nothing has demonstrated it yet". On a 512 MiB `--temp_dir` against a converted set needing ~2×:

| arm | exit | records |
|---|---|---|
| files | **1** — `No space left on device (os error 28)` | 0 |
| streamed | **0** | 2,398,276 |

Also: a 203-character sample name under `--multicore 4` is alignment-identical to a short-named control (checked on alignment columns, since a FIFO collision would show as interleaved output rather than an error), and peak FD usage is 73 against a 65,535 limit.

### Two things to flag

**A regression I reported and then retracted.** The first 10M non-directional run had streaming 2.4 % slower, and a plausible mechanism was available. With three reps the same shape is 0.65 % *faster* — the original was an outlier 87 s above the next streamed rep. Left in the record rather than edited away.

**`FULLSCALE.md` §4's `-p 1` shape is not runnable.** Bismark rejects it — `Please select a value for -p of 2 or more!` (`aligner/options.rs:165`, matching Perl 7993-8007). Substituted `-p 2`, which is the least-slack configuration that runs. Worth fixing in the brief.

### What this does not cover

Combined-index, `--hisat2` and `--rammap_subprocess` keep files by design and are unchanged. One dataset, one genome, one machine — the human WGBS accession was not run. And a clean A/B is not evidence about edges beyond the three §7 probes; three defects in this work were invisible to the 14-shape container matrix.

**Recommendation: merge.**

<sub>One limit worth stating: on directional 10M the third streamed rep (2974.4 s) is slower than every files rep, so the per-rep spread is comparable to the sub-1 % effect being measured. The *direction* is consistent across all seven shape/scale groups; the magnitude on any single group is not well determined by three reps. C1 and C2 are exact comparisons and carry no such caveat.</sub>

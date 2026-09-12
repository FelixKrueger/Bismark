# BENCHMARKS — serial vs streamed conversion

**Spike:** [`SPIKE.md`](./SPIKE.md) · **Plan:** [`PLAN.md`](./PLAN.md) · **Issue:** [#1120](https://github.com/FelixKrueger/Bismark/issues/1120)
**Date:** 2026-09-11 · **Base:** `dev` `170bfce`
**Harnesses:** `spikes/bench.sh` (pre-implementation projection) · `spikes/bench-ab-run.sh` (the shipped code)

> **Headline, measured on the implementation:** the disk win is real and large — **4 KB of
> converted bytes against 96 MB**. The projected ~10 % speedup **did not reproduce**: wall time
> is a wash. Section 1 is the projection that was wrong and why; section 2 is the measurement
> that supersedes it.

## 1. Pre-implementation projection (superseded)

Kept because it was wrong in a way worth remembering: a stand-in converter flattered the result
by a factor the real one does not.

### Why streaming was expected to be a win, not a cost

Conversion currently runs to completion **before** the aligner is spawned: `process_se_chunk`
(`mod.rs:1200`) calls `convert_se_files(...)`, prints the "Created … converted version" banner,
then spawns. So conversion sits on the critical path today, and streaming overlaps it with
alignment.

### Projected measurement

300,000 read pairs, both directional instances, `-p 4`, 3 reps:

| | convert | align | total | converted on disk |
|---|---|---|---|---|
| Serial (current shape) | 472-605 ms | 4148-4210 ms | 4621-4815 ms | 139 MB |
| Streamed | overlapped | — | 4174-4355 ms | **0** |

~10% faster. FIFO transport in isolation showed no overhead (106-110 ms vs 107-109 ms).

**The caveat turned out to be the whole story.** The stand-in converter was `awk`; the real Rust
converter is fast enough that the serial phase being hidden is nearly free, so there was almost
nothing left to win. See section 2.

## 2. Measured on the implementation (authoritative)

`spikes/bench-ab-run.sh`, in the pinned container. 200,000 read pairs (93 MB uncompressed across
R1+R2, a 40x concatenation of `test_files` with unique read IDs), directional paired-end against
the E. coli index, `-p 4`, release build. **7 reps with the two arms interleaved** — arm-major
ordering would hand any thermal drift entirely to one arm.

| | wall, median of 7 | peak converted bytes in `--temp_dir` |
|---|---|---|
| Files (`--no_stream_converted`) | 6.35 s | **96 MB** |
| Streamed (default) | 6.33 s | **4 KB** |

Per-rep wall times (s), in run order:

| rep | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
|---|---|---|---|---|---|---|---|
| streamed | 6.36 | 6.35 | 6.14 | 6.14 | 6.13 | 6.33 | 6.34 |
| files | 6.95 | 6.34 | 6.32 | 6.74 | 6.35 | 6.53 | 6.34 |

**Wall time is a wash** — a 0.3 % median difference inside a spread several times that. The
honest claim is **wall-neutral**, not faster. Conversion does come off the critical path, but the
Rust converter is fast enough that the phase being hidden is small, and the fan-out's per-block
copy and thread handoffs cost about what it saves.

**The disk result is the point, and it is unambiguous**: 96 MB (≈1x the uncompressed input, as
predicted) versus 4 KB, which is the one FIFO inode per aligner instance. The 4 KB never grows
with read count; the 96 MB does.

## Known measurement gap — CLOSED, see section 3

Files fully decouple the two aligner instances; a tee couples them — a fast instance can be
throttled by the bounded queue behind a slow one. Proven: no deadlock, no starvation,
byte-identical under `-p 1` vs `-p 4` at depth 1. **Not measured: wall time under that
asymmetry.** Deferred to pre-merge validation by agreement; queue depth is the mitigation knob.

> **Closed on 2026-09-12.** The coupling is real and visible, and it costs nothing. Section 3
> measures it directly on a mammalian genome. `CHANNEL_DEPTH` was not touched.

## 3. Full-scale validation (2026-09-12)

Run against [`fullscale/FULLSCALE.md`](./FULLSCALE.md). Full record, per-run time series and
plots: [`fullscale/RESULTS.md`](./fullscale/RESULTS.md).

**Machine.** 32 cores, 124 GB RAM, 1.1 TB scratch. Container from `spikes/Dockerfile.ab`
(rust 1.89.0, bowtie2 2.5.0, samtools 1.16.1), release build of the PR branch.

**Data.** `SRR24766921`, mouse RRBS paired-end, 51 bp — first 10,000,000 pairs (565 MB gzipped)
and a 2,000,000-pair subset. **Genome** Ensembl 112 `GRCm39` primary assembly, prepared once
with `bismark_genome_preparation --bowtie2 --parallel 16` (38 min, 13 GB).

**Method.** One benchmark at a time behind a lockfile — never two arms, never two shapes. Each
arm in its own container so cgroup v2 counters cover the whole process tree from zero. Page
cache warmed identically before every run. Arm order alternated between reps. A ~1 s time
series per run: converted bytes on disk, whole temp dir, output, memory (total *and*
anonymous), CPU, threads.

**52 runs.**

### C1 — byte-identity: PASS

**25 arm-pairs compared, 0 differing.** md5 over `samtools view` record bodies plus the
path-filtered report, every shape, every rep, both scales. Record counts are stable across
reps within a shape, so both arms are deterministic.

Stronger than required: `--multicore 2` yields *the same digest as the single-chunk run*.
Chunking does not perturb the record stream at all.

### C2 — no converted bytes on disk: PASS

Peak converted bytes in `--temp_dir`, sampled while the run is live (both arms delete at the
end, so measuring afterwards reports zero for both and proves nothing):

| shape | scale | streamed | files |
|---|---|---|---|
| `pe_directional_p4` | 10M | **0 KB** | 3.19 GiB |
| `pe_nondirectional_p4` | 10M | **0 KB** | 6.38 GiB |
| `pe_directional_mc2` | 10M | **0 KB** | 3.19 GiB |
| `pe_directional_p4` | 2M | **0 KB** | 0.64 GiB |
| `pe_nondirectional_p4` | 2M | **0 KB** | 1.27 GiB |

Zero, not "small". The file arm holds a full uncompressed copy of the input — two copies for
non-directional — for the *entire* run, not transiently.

One distinction: under `--multicore 2` the streamed arm's `--temp_dir` is not empty. It holds
3.92 GiB of input chunk splits, which both arms write and this PR does not touch. The
*converted* payload is 0 against 3.19 GiB. Quoting temp-dir totals alone would understate the
effect and misattribute the remainder.

### C3 — no wall-time regression: PASS

Median wall, streamed against files. Negative is faster.

<!-- C3TABLE:START -->
| shape | scale | n | delta |
|---|---|---|---|
| `pe_directional_p2` | 2M | 4 | **−1.50 %** |
| `pe_directional_p4` | 2M | 5 | **−0.66 %** |
| `pe_nondirectional_p4` | 2M | 5 | **−0.65 %** |
| `pe_nondirectional_p4` | 10M | 3 | **−0.55 %** |
| `pe_directional_mc2` | 10M | 1 | **−0.50 %** |
| `pe_directional_p4` | 10M | 3 | **−0.45 %** |
| `pe_directional_mc2` | 2M | 5 | **−0.01 %** |
<!-- C3TABLE:END -->

**Streaming is not slower anywhere.** Every figure is negative or zero, both scales, every
shape. The 2M and 10M results agree on the shapes measured at both.

The `--multicore 2` dead heat is a control worth having: 0.01 % with fully interleaved
distributions is what a genuine null looks like, which argues the ~0.65 % elsewhere is not the
harness favouring an arm.

The honest headline remains **wall-neutral to slightly faster**. The deltas are under 1 % on
most shapes. But the pre-merge concern was a *regression* from coupling the instances, and
there is no evidence of one.

### The instance asymmetry, measured

The open item above. On `pe_directional_p4` at 10M:

| | CPU profile | instances finish |
|---|---|---|
| **files** | 800 % until t=1761 s, then **400 % for the remaining 1194 s** | one 20 min before the other |
| **streamed** | flat **645 %** throughout | together, t=2944 s |

The asymmetry is real and large. With files the strand hypotheses are fully decoupled and the
machine spends the last 40 % of the run at half utilisation. With streaming the bounded channel
paces the fast instance to the slow one — exactly the throttling that was the concern.

**It costs nothing.** Total CPU is identical (18,990 vs 18,993 CPU-seconds) and the streamed
arm finished 8.9 s sooner. Wall time is set by the slowest instance in both arms; streaming
does not slow that instance, it declines to let the fast one race ahead and then idle. Same
work, spread evenly instead of front-loaded.

`CHANNEL_DEPTH` was not raised, and on this evidence does not need to be.

### An ordering worth noting

The advantage tracks inversely with per-instance threads: −1.50 % at `-p 2`, −0.66 % at `-p 4`,
−0.01 % under `--multicore 2`. A hypothesis, not a measurement: with fewer threads the aligners
are slower, the single converter thread always keeps up, and the channel never becomes the
constraint — while the file arm still pays to write a gigabyte and read it back.

This inverts what `FULLSCALE.md` §4 expected. It nominated the fewest-threads shape as the one
where the fan-out has least slack and streaming was most likely to hurt. It is the shape where
streaming helps most.

### Memory

Peak **anonymous** memory, streamed against files: 8.73 vs 12.05 GiB (directional 10M),
14.95 vs 18.09 GiB (`--multicore` 10M), 22.77 vs 23.96 GiB (non-directional 10M). Lower
streamed in six of seven shape/scale combinations, tied in the seventh.

> **A trap.** cgroup `memory.current` counts page cache, so the file arm's *total* looks 6+ GiB
> higher purely because its own converted files are cached. Read unqualified that would trip
> `FULLSCALE.md` §2's "if peak RSS moves by more than a few hundred MB, something is wrong".
> The sampler records `memory.stat anon` separately for this reason; the figures above are
> allocation, not cache.

### Section 7 — all pass

**7.2, a constrained `--temp_dir` — the premise, demonstrated for the first time.** A 512 MiB
tmpfs, against a converted set needing roughly twice that:

| arm | exit | records |
|---|---|---|
| files | **1** — `I/O error: No space left on device (os error 28)` | 0 |
| streamed | **0** | 2,398,276 |

The file arm dies; the streamed arm completes with a full result set. This is the case the
feature exists for and nothing had shown it before.

**7.1, long names under `--multicore`.** A 203-character sample name under `--multicore 4` is
alignment-identical to a short-named control (2,398,276 records each). Checked by comparing
alignment columns, not exit code — a FIFO name collision would surface as interleaved or
truncated output rather than an error.

**7.3, thread and FD counts.** Under the widest fan-out (`--non_directional --multicore 2`,
8 pipes): peak **73** open FDs against a 65,535 soft limit — 0.1 %. 35 threads, 9 processes.

### What did not reproduce

**A 2.4 % non-directional regression.** The first 10M non-directional run had the streamed arm
72 s slower, and a plausible mechanism was available — four consumers sharing two converted
streams, less slack in the fan-out. With three reps the same shape is 0.65 % *faster*; the
original run was the slowest of three, sitting 87 s above the next streamed rep against a files
spread of 27 s. An outlier. **Retracted.**

Recorded because of how nearly it became a finding: the story survived on plausibility, not
evidence, and only repetition killed it.

### What this does not cover

- **Combined-index modes**, `--hisat2` and `--rammap_subprocess` keep files by design and are
  unchanged by the PR (`PROGRESS.md` D-11).
- **One dataset, one genome, one machine.** Mouse RRBS on GRCm39. The human WGBS accession
  named in `FULLSCALE.md` was not run.
- **A clean A/B is not evidence about edges.** Three defects in this work were invisible to the
  14-shape container matrix. Section 7 probes three specific edges; it does not exhaust them.

### A defect in the brief

`FULLSCALE.md` §4 lists `-p 1` as one of four shapes. **Bismark rejects it** — "Please select a
value for -p of 2 or more!" (`aligner/options.rs:165`, matching Perl 7993-8007). Substituted
`-p 2`, the least-slack configuration that runs.


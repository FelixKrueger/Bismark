---
title: "Benchmarks"
description: "Runtime, CPU and memory measurements for the Bismark Rust suite: a rough comparison with Perl Bismark v0.25.1 in default mode, and how the parallel-capable tools scale with --parallel / --multicore."
---

The [Bismark Rust suite](/Bismark/installation/#bismark-rust-suite-beta) reimplements the Bismark
tools in Rust. Its output is byte-identical to Perl Bismark `v0.25.1`, so the only differences worth
measuring are runtime and memory.

This page has two parts: a rough comparison with Perl in default mode, for orientation; and, for the
tools that take a worker count, how runtime, CPU use and memory scale with `--parallel` /
`--multicore`. The scaling section is the more useful of the two, and is still being filled in. The
measurements present so far are noted as such; the rest are listed under
[Not yet measured](#not-yet-measured).

## Methods

Measurements were taken on a single-tenant 64-core / 128-thread Linux x86_64 server with 256 GB of
RAM. The Perl baseline is Bismark `v0.25.1` run with `LC_ALL=C`. The same input was given to both
implementations, and outputs were compared after decompression (`cmp <(zcat a) <(zcat b)`) to confirm
byte-identity before timing. Wall-clock time, CPU utilisation (mean cores busy) and peak resident
memory were recorded with `/usr/bin/time -v`.

Most figures are single runs of large, deterministic jobs; the `--parallel` sweeps below use three
repetitions per point. Treat the numbers as indicative rather than error-barred. Peak memory for the
Perl gzip-output tools reads very low because Perl compresses in a separate `gzip` process that
`/usr/bin/time` does not attribute to the parent.

## A rough comparison in default mode

This is for orientation, not a claim that the Rust version is faster at everything; for several tools
the only goal was byte-identical output, and timing is incidental.

| Tool | Rust vs Perl (default mode) | Workload |
|---|---|---|
| `bismark_methylation_extractor` | ~4.8× faster | full human WGBS, 64.6M read pairs, at comparable core counts (Rust ~7 cores vs Perl `--multicore 12`) |
| `coverage2cytosine` | ~12× (CpG report) / ~2.6× (`--CX`) | full hg38 |
| `bismark2bedGraph` | ~3.4× (CpG) / ~4.4× (`--CX`) | human WGBS PE |
| `NOMe_filtering` | ~3.4× | 10M SE |
| `deduplicate_bismark`, `bam2nuc`, `filter_non_conversion`, `methylation_consistency`, `bismark2report`, `bismark2summary`, `bismark_genome_preparation` | byte-identical; not separately timed | — |

The extractor figure compares wall-clock time at comparable resourcing. The Rust extractor uses about
7 cores in gzip mode; Perl `--multicore 12` uses roughly 19. A default Perl run is single-threaded, so
a default-vs-default ratio would be much larger but misleading, and is not used here.

## Scaling with `--parallel` / `--multicore`

Only three tools take a worker count: the methylation extractor (`--parallel`), the deduplicator
(`--parallel`), and the aligner (`--multicore`, including the `--rammap_inprocess` backend). The other
nine tools are single-threaded, so there is nothing to scale.

The intended end state for this section is a set of comparable plots, one row per tool, showing
wall-clock time, CPU use and peak memory against worker count on the same axes. Those runs are in
progress (see [Not yet measured](#not-yet-measured)). The measurements available now are below.

### Methylation extractor

`--parallel` sweep, gzip output, full human WGBS (64.6M read pairs); three reps per point:

| `--parallel` | Wall (s) | CPU (cores) | Peak threads |
|---|---|---|---|
| 1 | ~99 | ~7.1 | 67 |
| 2 | ~101 | ~7.0 | 67 |
| 4 | ~100 | ~7.1 | 69 |
| 8 | ~98 | ~7.2 | 73 |
| 16 | ~95 | ~7.3 | 81 |

In gzip mode the extractor is limited by BAM decompression, which already keeps about 7 cores busy, so
raising `--parallel` barely changes wall time or CPU use; it mainly adds worker threads. Peak memory
stayed below ~0.7 GB across the sweep with no clear trend. (In uncompressed output mode the picture
differs: CPU use is much lower and memory grows with worker count, since output buffers are no longer
the bottleneck.)

### rammap in-process backend

The `--rammap_inprocess` aligner backend loads each converted index once and shares it across the
strand instances, aligning reads in parallel on a `--multicore`-sized thread pool (50,000 EM-seq
Nanopore reads):

| `--multicore` | Wall | Peak memory |
|---|---|---|
| 1 | 216 s | 31.3 GB |
| 16 | 74 s | 31.3 GB |

Wall time falls by 11.4× from one thread to sixteen while peak memory stays flat, because all threads
share a single in-memory index. See [rammap](#rammap-experimental) for the comparison with the
subprocess backend and with minimap2.

## Aligner

The aligner (`bismark_rs`) accounts for most of a run's wall time, but it calls the same external
Bowtie 2, HISAT2 and minimap2 binaries as the Perl version, so the mapping itself is unchanged and
there is no alignment speedup to report. The port provides two things instead:

- Byte-identical output to Perl `v0.25.1` at full scale, validated on 84M single-end reads, 84M
  paired-end reads, and 46.7M mouse RRBS reads (GRCm39), across directional, non-directional and PBAT
  libraries and both FastQ and FastA input, for Bowtie 2 and HISAT2, and for minimap2 in single-end
  mode.
- A worker-invariant `--multicore` / `--parallel` model, in which the output does not depend on the
  number of workers. The Perl model re-decompresses the input BAM once per worker; the Rust model
  splits the input into contiguous chunks and merges them in order.

A timed `--multicore` sweep for the aligner is part of the work still to come.

## rammap (experimental)

`--rammap` adds a fourth backend, [`rammap`](https://github.com/jwanglab/rammap), a pure-Rust
reimplementation of minimap2 for long-read alignment (for example EM-seq Nanopore data). It is opt-in
and concordance-gated, and is not byte-identical to minimap2; it is a separate experimental track from
the faithful port.

On 1M EM-seq Nanopore reads (GRCh38, run through the Bismark wrapper), rammap and minimap2 agree on
the fate of 98.3 % of reads, with unique-versus-ambiguous classification differing for 0.011 % of
reads, and on 99.8 % of per-CpG methylation calls at depth ≥ 1. rammap maps slightly more reads and
does not produce alignments that minimap2 does not.

Run in-process rather than as a subprocess, rammap is both faster and lighter, because the converted
index is loaded once and shared instead of once per strand instance. On the same 1M reads
(non-directional):

| Metric | Subprocess `--rammap` | In-process `--rammap_inprocess` |
|---|---|---|
| Wall time | 2451 s | 1382 s (~1.8× faster) |
| Peak memory | 70.9 GB | 32.3 GB (−54 %) |

Plain `--rammap` still runs the subprocess; `--rammap_inprocess` (available from `2.0.0-beta.11`) is
the explicit opt-in.

## Profiling the Perl pipeline

For context, the rewrite was prioritised from a profile of a complete Perl `v0.25.1` run (Apple M1
Pro, 55.7M paired-end reads, GRCh38):

| Stage | Perl wall time | Share of total |
|---|---|---|
| Alignment (Bowtie 2) | 472 min | 74 % |
| Methylation extraction | 104 min | 16 % |
| bedGraph + coverage report | 57 min | 9 % |
| Deduplication | 8.7 min | 1 % |

Alignment dominates, but that time is spent inside the external mapper, which a faithful port does not
change. The post-alignment tools are the part the rewrite can speed up.

## Not yet measured

The following are planned as a focused benchmark run on 10M-read subsets and will be added here as
comparable plots:

- `--parallel` sweeps for the **methylation extractor** (single-end and RRBS, alongside the WGBS PE
  data above) and the **deduplicator**, recording wall time, CPU use and peak memory.
- A `--multicore` sweep for the **aligner** across backends (Bowtie 2, HISAT2) and library types. The
  aligner is currently validated for correctness and worker-invariance but has not been timed.
- A full `--multicore` sweep for the **rammap in-process backend** beyond the two points above.

The methodology and raw logs for the figures here are kept with each tool in the repository.

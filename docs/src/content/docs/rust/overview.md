---
title: "Scope of the rewrite"
description: "Why Bismark is being reimplemented in Rust, what stays byte-identical to Perl v0.25.1, and the additional capabilities the Rust architecture enables."
---

The [Bismark Rust suite](/Bismark/installation/#the-bismark-rust-suite) reimplements Bismark's tools
in Rust with two aims: to match Perl Bismark `v0.25.1` output byte-for-byte, so the Rust build is a
drop-in replacement, and to remove the post-alignment bottlenecks that a faithful reimplementation can
actually address. This page explains the reasoning behind the rewrite, what is kept identical to the
Perl version, and the capabilities the Rust architecture adds on top.

## Why a rewrite

A profile of a complete Perl `v0.25.1` run (Apple M1 Pro, 55.7M paired-end reads, GRCh38) shows where
the time goes:

| Stage | Share of total |
|---|---|
| Alignment (Bowtie 2) | 74 % |
| Methylation extraction | 16 % |
| bedGraph + coverage report | 9 % |
| Deduplication | 1 % |

Alignment dominates, but it is performed by an external mapper (Bowtie 2, HISAT2 or minimap2). A
faithful port calls the same binaries, so it cannot make the mapping itself any faster. The remaining
roughly one quarter of the run (extraction, bedGraph, coverage reporting and deduplication) is wrapper
code that Bismark owns, and that part is addressable. Methylation extraction in particular scales worse
than linearly with read count, because gzip output becomes an I/O bottleneck, and Perl's `--multicore`
model forks one worker per slice and re-decompresses the input BAM once per worker, spending CPU
without turning it into throughput. (Full timings are on the
[benchmarks page](/Bismark/rust/benchmarks/#profiling-the-perl-pipeline).)

The goals of the rewrite follow from this profile: faster and lower-memory post-alignment processing,
on a maintainable modern codebase, while producing output that is **byte-identical to Perl `v0.25.1`**.
Byte-identity is the correctness contract that lets the Rust suite stand in for the Perl one without
revalidating an established pipeline. The measured speed-ups for each tool are on the
[benchmarks page](/Bismark/rust/benchmarks/). The Perl version is in maintenance
freeze (critical correctness and security fixes only) and is archived as tagged legacy, following the
precedent of Salmon's `cpp` branch.

## What stays the same

All twelve Bismark tools are reimplemented in Rust as a single `bismark` crate — one multicall binary,
with the shared BAM/SAM/CRAM I/O in its `bismark::io` module — and validated to be byte-identical to
Perl `v0.25.1`. The aligner faithfully wraps the **same** external
Bowtie 2 / HISAT2 / minimap2 binaries, so read mapping is unchanged; the Rust work is the per-read
in-silico bisulfite conversion and methylation-call tagging that surrounds the mapper. Its
`--multicore` parallelism is worker-invariant, meaning the output does not depend on the number of
workers (see below).

## What the rewrite additionally enables

Reimplementing the tools also made room for capabilities the Perl version does not have. Several of
these are opt-in and concordance-gated rather than byte-identical; that distinction is spelt out at the
end of this section.

1. **Combined-genome alignment.** Instead of running 2 (directional) or 4 (non-directional) separate
   per-strand aligner instances against the individual C→T and G→A converted genomes, the Rust aligner
   can align against a single combined index. It is exposed as one family of three flags:
   - `--combined_index` runs one both-strands pass per read-conversion.
   - `--combined_index_sequential` is a non-directional low-memory variant. It uses about a third less
     peak memory than the faithful default and is byte-identical to the parallel `--combined_index`
     run. The [alignment guide](/Bismark/usage/alignment/#is-combined-mode-advisable) covers when it is
     advisable.
   - `--combined_index_single_pass` runs a single pass over conversion-tagged interleaved reads. It is
     not byte-identical, and not decision-equivalent to the other modes, because tagging the read names
     perturbs Bowtie 2's internal RNG; it is therefore ground-truth-validated and never the default.

   The whole family is opt-in and concordance-gated against the faithful per-strand default.
   Measurements are on the [benchmarks page](/Bismark/rust/benchmarks/#combined-index-modes).
2. **rammap, an experimental fourth aligner.** `--rammap` adds
   [rammap](https://github.com/jwanglab/rammap), a pure-Rust reimplementation of minimap2, for
   long-read bisulfite data such as EM-seq Nanopore. Run in-process with `--rammap_inprocess`, the
   converted index is loaded once and shared across the strand instances, which uses about 54 % less
   memory and runs roughly 1.8× faster than the subprocess backend. It is opt-in, concordance-gated, and not byte-identical to minimap2; see the
   [benchmarks page](/Bismark/rust/benchmarks/#rammap-experimental).
3. **In-process post-alignment streaming.** The Rust methylation extractor drives bedGraph generation
   and coverage2cytosine in memory, in the same process, instead of launching separate Perl
   subprocesses and re-reading intermediate files. This is part of why the post-alignment stage is
   faster; see the [extractor benchmarks](/Bismark/rust/benchmarks/#methylation-extractor).
4. **Worker-invariant `--multicore`.** The output is independent of the worker count: the same input
   produces the same bytes whether it is run on one worker or many. Perl's fork-and-modulo scheme does
   not have this property.
5. **Graceful handling of empty input.** A sample where nothing aligned (a header-only BAM) flows
   through deduplication, methylation extraction and coverage2cytosine without crashing: each tool
   emits valid empty or all-zero output and exits cleanly. This is a deliberate, documented divergence
   from Perl `v0.25.1`, where deduplication and coverage2cytosine instead exit with an error on empty
   input. It keeps a no-alignment sample from aborting an automated pipeline such as nf-core/methylseq.
   Non-empty runs remain byte-identical.
6. **Pure-Rust BAM/SAM/CRAM I/O.** The suite reads and writes alignment files directly (via its
   `bismark::io` module), so the tools no longer depend on an external `samtools` for file I/O.

:::note[Byte-identical versus concordance-gated]
The combined-index modes and rammap are **opt-in and concordance-gated**: they trade byte-identity for
speed or memory and are never selected by default. The faithful per-strand alignment path, and every
other tool in the suite, remain **byte-identical to Perl `v0.25.1`**. That is the path to use when
reproducing published results or feeding a strictly validated pipeline.
:::

# `bismark-bedgraph` — Benchmarks

Consolidated speed + memory benchmarks for `bismark2bedGraph_rs` vs Perl
`bismark2bedGraph` v0.25.1. The README's *Performance* and *Memory footprint*
sections carry the headline; this file collects the full numbers and
methodology. Deeper write-ups:

- Parallel-parse investigation (prototyped, **rejected**): `plans/05302026_bedgraph-parallel-parse/PLAN.md` §14.
- Read-phase micro-profile: `plans/05292026_bismark-bedgraph/spikes/SPIKE_read_phase_split.md`.

## Methodology

- **Machine:** oxy (`dockyard-oxy-0`), Linux x86_64, 256 GB RAM. CPU was not the constraint for these runs.
- **Data:** human WGBS, paired-end, deduplicated (`SRR24827373`). The 10M figures use a 10M-read subset; the `--CX` figures use the full sample (837,741,418 distinct covered positions).
- **Baseline:** Perl `bismark2bedGraph` v0.25.1, `LC_ALL=C`. The **identical** ordered input-file list is fed to both producers.
- **Measurement:** wall-clock + peak RSS via `/usr/bin/time -v`; outputs compared **decompressed** (`cmp <(zcat a) <(zcat b)`).
- **Caveat:** single runs — these are large, deterministic I/O+CPU jobs, so run-to-run noise is « the reported gaps. The raw gate outputs lived on ephemeral oxy `/tmp` and have been cleaned; the numbers below are from the run logs.

## Speed (vs Perl v0.25.1)

| Workload | Perl | `bismark2bedGraph_rs` | Speedup |
|---|---|---|---|
| 10M PE, default (CpG) | 27 s | 8 s (parallel gzip) | ~3.4× |
| Full `--CX` (837,741,418 rows) | 3741 s (62m 21s) | 854 s (14m 14s, +mimalloc) | **~4.4×** |

- The 10M default win is in-process **parallel gzip** (`gzp`): a flamegraph showed ~70% of the pre-gzp runtime was *serial* DEFLATE (Perl is fast only because it offloads gzip to a parallel subprocess).
- The `--CX` win adds the **mimalloc** allocator: 973 s (system allocator) → 854 s (~12%), since the in-memory aggregation is allocation-heavy.

## Memory

Perl streams one chromosome at a time through UNIX `sort` (peak bounded by
`--buffer_size`, spilling to disk). This port holds **all** covered
`(chr, pos)` positions in memory at once — **no disk spill**;
`--buffer_size`/`--ample_memory` are accepted-but-ignored. Peak RSS scales with
distinct covered positions (~33–40 B each):

| Run (human/mouse) | Distinct positions¹ | Peak RAM |
|---|---|---|
| CpG-only | ~38 M covered / ~56 M genome-wide | ~1.5–2 GB |
| `--CX`, all contexts | ~840 M (measured) | **~28–30 GB** (measured) |
| Perl `--CX` (for comparison) | (streamed per-chr) | ~2 GB (bounded, spills) |

¹ **Counted per-cytosine, both strands.** bismark2bedGraph reports at single-C
resolution and does *not* merge strands, so each CpG dinucleotide (~28 M
genome-wide) yields **two** positions — top-strand C at *N* (`+`), bottom-strand
C at *N+1* (`−`) — i.e. ~2× the dinucleotide-site count. Measured per strand:
CpG_OT 19.2 M + CpG_OB ~19.2 M ≈ 38 M covered. Cross-check: the `--CX` total
= 2 × (CpG 19.2 M + CHG 88.6 M + CHH 310.9 M) ≈ 837 M ✓.

**Implication:** CpG-only runs on a laptop; a genome-wide `--CX` run needs a
large-memory host and will **OOM rather than spill** if RAM is exhausted. On a
memory-limited machine use Perl for full `--CX`, restrict to CpG, or split
inputs. A bounded/external-spill mode is a documented future capability
(SPEC §9).

## Read-phase profile (full `--CX`, per input file, mimalloc)

| File | calls → positions | decompress | parse | **insert** |
|---|---|---|---|---|
| CpG_OT | 35.1 M → 19.2 M | 29% | 15% | 56% |
| CHG_OT | 166 M → 88.6 M | 17% | 11% | 72% |
| CHH_OT | 588 M → 310.9 M | 11% | 9% | **79.5%** |

The read+aggregate phase is **hashmap-insert-bound** (memory-latency-bound), and
increasingly so with scale; gzip decompression — the only part that can't
parallelise within a single stream — is a shrinking minority.

## Why there is no `--parallel`

Parsing the per-context files concurrently was prototyped (byte-identical,
N-invariant) but **rejected after measurement** — it *anti-scales*:

| `--parallel` (full `--CX`, mimalloc) | wall |
|---|---|
| 1 (sequential) | **854 s — fastest** |
| 3 | 1382 s |
| 6 | 1125 s |

The read phase is memory-**bandwidth**-bound; building several multi-GB maps at
once contends on the shared memory bus (confirmed allocator- and
sharding-independent, with thread-state sampling). Sequential is optimal.
Full analysis + the controlled experiment: `plans/05302026_bedgraph-parallel-parse/PLAN.md` §14.

## Byte-identity

All numbers above were measured on output **verified decompressed-byte-identical
to Perl v0.25.1** — full `--CX` at 837,741,418 rows, and N-invariant across
`--parallel 1/3/6`. CI runs 9 hermetic fixture cells against Perl-generated
expected; the original port's real-data walk + flamegraph SVG are at colossal
`/weka/projects/bioinf/Data/Felix/bismark_benchmarks/benchmark_results/`.

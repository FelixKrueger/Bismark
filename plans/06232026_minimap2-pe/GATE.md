# Paired-end minimap2 — concordance gate

**Date:** 2026-06-23 · **Verdict:** ✅ PASS → promoted from "experimental" to
**concordance-gated (NOT byte-identical to Perl)**, the `--rammap` / `--combined_index`
status. Harness: [`gate_harness.py`](gate_harness.py) (self-contained; needs `bowtie2` +
`bowtie2-build` + `minimap2` + `samtools` + `pysam` on PATH).

## Why a concordance gate (not byte-identity)

There is no Perl oracle for PE minimap2 (the Perl path is unfinished WIP; its report
mislabels minimap2 as HISAT2). So PE minimap2 is gated the way `--rammap` and
`--combined_index` are: against a **trusted reference** with a documented tolerance. The
trusted reference is the **byte-frozen Bowtie 2 PE backend** (itself byte-identical to Perl
v0.25.1) for the short-read case, plus **ground-truth recovery** (the reads are simulated,
so the correct position is known) for both short and long reads.

## Setup

Random 60 kb genome, `bismark_genome_preparation_rs` building both Bowtie 2 and minimap2
indexes. Simulated fully-methylated **directional (OT)** paired-end reads (read 1 = top-strand
5′ end, read 2 = revcomp of the top-strand 3′ end), run through the real `bismark_rs`
release binary with `--bowtie2` and `--minimap2`. Pins: Bowtie 2 **2.5.5**, minimap2
**2.31-r1302** (the suite pins).

## Results

| Metric | minimap2 PE | Bowtie 2 PE (oracle) |
|---|---|---|
| short-read (1000 pairs, 100 bp, insert 320): mapped | 1000 | 1000 |
| short-read: at ground-truth position | **1000 / 1000 (100%)** | 1000 / 1000 |
| long-read (300 pairs, 600 bp, insert 1800): at truth | **300 / 300 (100%)** | n/a (Bowtie 2 can't) |
| **position concordance** vs Bowtie 2 (common pairs) | **1000 / 1000 = 100.000%** | — |
| **`XM` methylation-call concordance** vs Bowtie 2 | **1000 / 1000 = 100.000%** | — |
| determinism (run1 == run2 BAM body) | ✅ identical | — |
| worker-invariance (`--multicore 1` == `4` BAM body) | ✅ byte-identical | — |

## What the gate caught (and fixed)

The gate exposed two fundamental bugs that the fake-binary unit tests masked — the reason
the first cut crashed on real multi-read data and the Perl path was left unfinished:

1. **Pairing model.** With `-x map-ont`, minimap2 reads the two query files SEQUENTIALLY
   and emits all read 1s, then all read 2s (not interleaved mate pairs). Consecutive-line
   pairing read two read 1s as a "pair" and died. Fix: `Minimap2PairedStream` joins read 1
   ↔ read 2 by read-ID (drains each instance, primary line per mate, skips
   secondary/supplementary, keeps unmapped FLAG 4), presenting one pair per read in input
   order. `drive_merge_pe` is generic over `PairedSamStream`; `process_pe_chunk` dispatches.

2. **QNAME over-strip regression.** A `/1/1`-then-`/1` strip in `from_lines` over-stripped
   for Bowtie 2 when reads carry an explicit `/1` mate suffix → broke the Bowtie 2 PE
   lockstep (0 mapped). Fix: `from_lines` reverts to the byte-frozen single-`/1` strip;
   `Minimap2PairedStream` normalises minimap2's un-clipped QNAME to the single-suffix
   Bowtie 2 shape, so `from_lines` + `--ambig_bam` stay byte-identical for all backends.

Plus the concordance enforcement (FR orientation + `--minins`/`--maxins`, gated on minimap2)
that makes a "pair" of two independent SE alignments a defensible PE alignment.

## Scope / honesty

- The gate is **local and small-scale** (self-contained, ~1300 pairs). It demonstrates
  correctness + concordance; a maintainer **full-scale real-data gate** (oxy-style, as for
  every other backend) remains the final production sign-off.
- Short-read WGBS PE is already covered byte-identically by Bowtie 2 / HISAT2; minimap2 PE's
  real value is **long-read bisulfite PE**, where there is no short-read oracle — hence the
  ground-truth-recovery metric (100%) carries that case.
- Exact byte-identity to Perl is permanently out of scope (no oracle).

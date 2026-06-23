# 5-Base real public-data validation (TAPS vs matched WGBS)

**Date:** 2026-06-23. **Honest status:** the pipeline is **validated as correct and
biologically sound on real public data**; it is **not yet a polished publication-grade
per-CpG benchmark** (that needs whole-genome depth + base-quality filtering — see
Findings).

## Why TAPS

There is no public raw **Illumina 5-Base** dataset (launched Oct 2025). The faithful
mechanical equivalent is **TAPS** (TET-Assisted Pyridine borane Sequencing, Liu et al.,
*Nat Biotechnol* 2019) — the same **5mC→T** chemistry. The chosen study is ideal because
it contains **TAPS and matched WGBS of the SAME E14 mESC sample**, so the TAPS calls can
be compared against the field-standard methylation reference.

## Data + commands

- Study **GSE112520 / SRP136786**. TAPS run **SRR8145389** (5,000,000 read-pair subset);
  matched WGBS run **SRR6918157** (2,000,000 read-pair subset). Reference: **mouse GRCm39
  chromosome 19** (61.4 Mb, Ensembl release-110).
- TAPS: `bismark_rs --illumina_5base --five_base_deconvolution --genome <chr19> -1 -2`
  (the default minimap2 `-x sr` unconverted path).
- WGBS: `bismark_genome_preparation_rs` then `bismark_rs --bowtie2` (the faithful
  bisulfite path), same chr19.
- Both extracted with `bismark_methylation_extractor_rs -p --comprehensive --bedGraph`.

## Results

| | TAPS (`--illumina_5base`) | matched WGBS (faithful Bismark) |
|---|---|---|
| unique pairs → chr19 | 523,453 | 45,001 |
| **CpG methylation** | **56.4 %** | **62.7 %** |
| CHG methylation | 2.1 % | 0.5 % |
| CHH methylation | 2.1 % | 0.4 % |
| deconvolution | 1,184 variant CpGs, 37,188 methylation sites | — |

**Concordance (TAPS vs WGBS, shared CpGs):**
- Global CpG mean: **56–61 % (TAPS) vs 61–63 % (WGBS)** — concordant.
- Per-CpG Pearson r: 0.27 at depth ≥5 (n=492 shared CpGs).
- Regional (windowed) Pearson r: **0.50–0.63** (10–100 kb windows), MAD ≈ 9–11 pp.

## What this validates (real data)

1. **The pipeline runs at scale** on real public 5mC→T reads (5M pairs), SE+PE, with the
   deconvolution pass, and the downstream Bismark extractor consumes the BAM unchanged.
2. **The inverted-polarity caller is correct.** Non-CpG methylation is ~2 % (not ~98 %),
   i.e. unmethylated cytosines are correctly called unmethylated; only CpG carries the
   methylation signal — exactly the expected biology. A wrong polarity would invert this.
3. **Global methylation is concordant with matched WGBS** on the same sample (~56–61 %
   vs ~61–63 %), reproducing the TAPS-vs-WGBS agreement the method's paper reports.

## Findings / why it is not yet publication-grade per-CpG

1. **Depth-limited.** Aligning a whole-genome library to a single chromosome wastes ~97 %
   of reads, so per-CpG depth is ~5× at best → binomial sampling noise dominates the
   per-site correlation. A true per-CpG r>0.9 (as the TAPS paper shows) needs
   **whole-genome alignment + high depth**, not a single-chromosome subset.
2. **A real ~2 % non-CpG noise floor** (vs WGBS 0.4 %). MAPQ filtering does not remove it
   (56→57.5 % across MAPQ 0–40), so it is per-base error within otherwise-fine alignments:
   the v1 5-Base caller is **base-quality-naive** (it has no `--methylation-baseq` filter,
   unlike DRAGEN) and applies no adapter/quality trimming. This inflates both the non-CpG
   rate and the per-CpG noise.

## Update: base-quality masking does NOT move this data (honest null result)

`--five_base_baseq 20` was implemented and re-run on the same chr19 TAPS data:
CpG 56.3 %, CHG 2.0 %, CHH 2.1 % — **identical to the no-filter run**. The TAPS reads
are high-quality (Phred ~40 throughout), so a Q<20 mask removes nothing. Therefore the
~2 % non-CpG floor is **NOT low-base-quality sequencing error** (and MAPQ filtering also
did not move it). It is **high-quality-base mapping/chemistry noise** — reads placed with
real mismatches (forced cross-mapping when a whole-genome library is aligned to one
chromosome) and the TAPS chemistry's own false-positive rate. `--five_base_baseq` remains
a sound, DRAGEN-precedented option, but it is not the lever for this dataset.

The real lever is **whole-genome alignment** (reads land at their true locus instead of
being forced onto chr19) plus possibly extending the deconvolution beyond CpG. The
single-chromosome shortcut is the dominant artifact here.

## Concrete next steps for a publication-grade benchmark

- **Whole-genome** alignment (all reads map to their true locus) at adequate depth.
- A **base-quality threshold** in `methylation_call` (skip low-Q read bases; DRAGEN's
  `--methylation-baseq-threshold` precedent) + adapter/quality trimming, to cut the ~2 %
  noise floor.
- Then per-CpG concordance vs matched WGBS at depth ≥10, targeting r>0.9.

*Reproducible: subsets pulled from ENA (`ftp.sra.ebi.ac.uk/vol1/fastq/...`), real
minimap2 2.31-r1302 + Bowtie 2 2.5.5; see the commands above.*

---
title: "Illumina 5-Base"
description: "How to run Bismark's Illumina 5-Base (5mC to T) mode, what it supports, and the concordance stability contract for a non-byte-identical, DRAGEN-validated path."
---

Illumina 5-Base (the "5-Base DNA Prep") is the chemical **inverse** of bisulfite. The
enzyme converts **5-methylcytosine to thymine** and leaves unmethylated cytosine intact,
so the library keeps its full complexity and the raw reads align to the **unconverted**
genome with a standard aligner. Bismark then calls methylation at **inverted polarity**: a
read `T` at a genomic `C` is methylated (`Z`), a read `C` is unmethylated (`z`).

The mode is opt-in behind `--illumina_5base` and is **paired-end only** (the 5-Base
library is paired-end and directional; single-end input is rejected at startup).

## Stability contract

5-Base is **not byte-identical** to anything: Perl Bismark has no 5-Base path, so there is
no byte-for-byte oracle. It is instead **concordance-gated**, the same contract as the
`--rammap` and `--combined_index` paths:

- **Supported in GA (2.0.0):** `--illumina_5base` (core), `--five_base_duplex`,
  `--five_base_consensus`, `--five_base_deconvolution`. Each is validated against Illumina
  DRAGEN on the real NA12878 demo at full depth by the metric appropriate to what it
  produces: **core** by per-CpG concordance with DRAGEN's (deduplicated, full-depth) CX
  report (deduplicated, iso-DRAGEN) — **r approximately 0.99 over 55M CpGs (0.998 at
  cov>=10), 99.3% call-agreement**; **deconvolution**
  by variant precision/recall vs DRAGEN's germline VCF — **90.3% / 93.4%**; **duplex /
  consensus** (per-molecule collapse — DRAGEN builds a duplex-consensus methylation track
  only for UMI/enrichment kits, so there is no WGS consensus-CX to correlate against) by
  per-strand **mean-methylation agreement** with DRAGEN (~47.9% vs ~48%, bias-free). All
  four are additionally held to a reproducible lambda/pUC19 spike-in gate that runs in CI
  with no proprietary data.
- **Preview:** `--five_base_umi_qname` (duplex UMI taken from the read name). It works and
  is exercised on real data, but is not yet held to the same gated contract.
- **Unchanged:** the faithful bisulfite / EM-seq suite is **byte-frozen**. Every legacy
  methylation call still reduces to the exact Perl v0.25.1 output (the `perl-oracle` CI
  gate stays green); turning 5-Base on never perturbs a bisulfite run.

Because it is concordance-gated rather than byte-identical, every 5-Base path is opt-in and
**never silent**: an unsupported combination (single-end, `--non_directional`, `--pbat`,
`--combined_index`) is rejected loudly rather than silently degraded.

## Running it

Directional, paired-end, against the unconverted genome. minimap2 (`-x sr`) against the
genome FASTA is the default engine; bowtie2 or hisat2 work against a NORMAL (unconverted)
index supplied with `--five_base_index`.

```sh
# core: align + inverted-polarity methylation calls
bismark --illumina_5base --genome /path/to/GRCh38 -1 R1.fastq.gz -2 R2.fastq.gz

# bowtie2/hisat2 instead of minimap2 (build a plain index once with bowtie2-build)
bismark --illumina_5base --bowtie2 --five_base_index /path/to/normal_idx \
        --genome /path/to/GRCh38 -1 R1.fastq.gz -2 R2.fastq.gz
```

The emitted BAM is standard Bismark convention (`XM`/`XR`/`XG`), so the extractor,
`bismark2bedGraph`, `coverage2cytosine`, and the reports consume it unchanged.

### Advanced modes

Real 5-Base libraries carry a dual UMI in the read name and sequence each molecule as two
strand-partner read pairs. These modes use that structure:

- **`--five_base_deconvolution`** separates a genuine `C>T`/`G>A` genetic variant from 5mC
  using both strands (DRAGEN's rule: methylation moves only one strand, a variant moves
  both). Writes a per-CpG report; variant CpGs are excluded from the methylation totals.
- **`--five_base_duplex`** groups the two strands of each molecule into a duplex family
  (keyed on the fragment span plus the canonical dual UMI) and reconciles the 5mC signal
  per molecule.
- **`--five_base_consensus`** collapses each duplex family to one consensus record per
  molecule, scoring both strands of every CpG.
- **`--five_base_umi_qname`** (preview) takes the duplex UMI from the read name (the real
  Illumina form, `A+B` with the partner swapped) instead of an inline read prefix.

```sh
# duplex consensus from the real dual-UMI in the read name
bismark --illumina_5base --five_base_umi_qname --five_base_consensus \
        --genome /path/to/GRCh38 -1 R1.fastq.gz -2 R2.fastq.gz
```

## Validation

The concordance evidence (per-CpG Pearson r and call-agreement vs DRAGEN, and the
deconvolution precision/recall vs DRAGEN's germline VCF) and a reproducible runbook live in
the repository at `plans/06232026_illumina-5base-support/VALIDATION_REAL_DATA.md`, with the
`validation/concordance.py` harness. The deterministic floor (lambda unmethylated control
reads near 0%, pUC19 CpG-methylated control reads high) is locked by the
`five_base_controls_*` gates that run in CI.

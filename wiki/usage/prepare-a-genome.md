---
type: usage
title: Prepare a genome
description: Build the C-to-T and G-to-A converted genome and its indexes once, before the first alignment.
tags: [genome, index, bowtie2, hisat2, minimap2]
page_id: prepare-a-genome
subject_status: current
updated: 2026-10-12
sources:
  - repo: Bismark
    path: rust/bismark/src/genome_prep/cli.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/genome_prep/discovery.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/genome_prep/folders.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/genome_prep/indexer.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/aligner/discovery.rs
    commit: 65b183a
---

# Prepare a genome

## Summary

`bismark prepare` converts a genome in silico (C→T and G→A) and builds an aligner index for each converted copy. The developer says this is required, and that it runs only once per genome and aligner. Run it for the same aligner that you will align with.

## Steps

1. Put the genome FASTA file(s) in one folder.
2. Build the indexes for Bowtie 2 (the default):

   ```
   bismark prepare --parallel 4 /data/genomes/GRCh38/
   ```

   Add `--hisat2` or `--minimap2` to build for that aligner instead. Pick only one.
3. Pass the same folder to the aligner with `--genome /data/genomes/GRCh38/`.

## What it reads

- The FASTA files in the folder. It tries `.fa`, then `.fa.gz`, then `.fasta`, then `.fasta.gz`, and uses only the first extension that has files. It never mixes extensions (`find_fasta_files` in `genome_prep/discovery.rs`).
- The indexer: `bowtie2-build`, `hisat2-build` or `minimap2`, from `PATH` or from `--path_to_aligner <folder>`.

## What it writes

Inside the genome folder (`create_tree` in `genome_prep/folders.rs`, `run_both` in `genome_prep/indexer.rs`):

| Path | Content |
|---|---|
| `Bisulfite_Genome/CT_conversion/genome_mfa.CT_conversion.fa` | C→T converted genome |
| `Bisulfite_Genome/CT_conversion/BS_CT.*` | Index of the C→T genome (`BS_CT.mmi` for minimap2) |
| `Bisulfite_Genome/GA_conversion/genome_mfa.GA_conversion.fa` | G→A converted genome |
| `Bisulfite_Genome/GA_conversion/BS_GA.*` | Index of the G→A genome |
| `Bisulfite_Genome/Combined/BS_combined.*` | Only with `--combined_genome`: one index of both copies, for `--combined_index` |
| `genomic_nucleotide_frequencies.txt` | Only with `--genomic_composition` |

If `Bisulfite_Genome/` already exists, the tool prints a warning and overwrites it.

## Options this task uses

| Option | Effect |
|---|---|
| `--parallel <n>` | Threads per indexer, 2 or more. The two indexers run at the same time, so `--parallel 4` uses 8 cores. Without it each indexer gets 1 thread. |
| `--bowtie2` / `--hisat2` / `--minimap2` | The indexer. Bowtie 2 is the default. |
| `--single_fasta` | One FASTA file per chromosome instead of one multi-FASTA. Not with `--minimap2`. |
| `--large-index` | Forces a large Bowtie 2 / HISAT2 index. Not with `--minimap2`. |
| `--combined_genome` | Also builds the combined index. |

5-Base runs do not need this step: `--illumina_5base` reads the genome FASTA directly, or a normal (unconverted) index. See [usage/align-reads.md](align-reads.md).

## Links

- [usage/align-reads.md](align-reads.md): the next step.
- [troubleshooting.md](../troubleshooting.md): what the aligner says when an index file is missing.

## Open questions

- `--slam` is marked deprecated in `genome_prep/cli.rs`. When it is removed is not in the code.

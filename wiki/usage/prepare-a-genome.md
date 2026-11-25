---
type: usage
title: Prepare a genome
description: Build the C-to-T and G-to-A converted genome and its indexes, or a bwa-mem3 methylation index, once before the first alignment.
tags: [genome, index, bowtie2, hisat2, minimap2, bwa-mem3]
page_id: prepare-a-genome
subject_status: current
updated: 2026-11-25
sources:
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/genome_prep/cli.rs
    commit: 457509a
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/genome_prep/discovery.rs
    commit: dbd7c27
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/genome_prep/folders.rs
    commit: dbd7c27
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/genome_prep/indexer.rs
    commit: dbd7c27
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/discovery.rs
    commit: dbd7c27
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/genome_prep/bwamem3.rs
    commit: dbd7c27
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/genome_prep/pipeline.rs
    commit: dbd7c27
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/genome_prep/error.rs
    commit: dbd7c27
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/config.rs
    commit: 457509a
---

# Prepare a genome

## Summary

`bismark prepare` converts a genome in silico (C→T and G→A) and builds an aligner index for each converted copy. The developer says this is required, and that it runs only once per genome and aligner. Run it for the same aligner that you will align with. With `--bwamem3` it instead builds one bwa-mem3 methylation index of the unconverted genome, for `--illumina_5base` (its default engine) and `--taps` (see [bwa-mem3 index](#bwa-mem3-index---bwamem3)).

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
| `--parallel <n>` | Threads per indexer, 2 or more. The two indexers run at the same time, so `--parallel 4` uses 8 cores. Without it each indexer gets 1 thread. With `--bwamem3`: bwa-mem3's `-t`, 1 or more; without it bwa-mem3 picks its own count. |
| `--bowtie2` / `--hisat2` / `--minimap2` / `--bwamem3` | The indexer. Bowtie 2 is the default. Pick only one (`resolve` in `genome_prep/cli.rs`). |
| `--single_fasta` | One FASTA file per chromosome instead of one multi-FASTA. Not with `--minimap2`. |
| `--large-index` | Forces a large Bowtie 2 / HISAT2 index. Not with `--minimap2`. |
| `--combined_genome` | Also builds the combined index. |

5-Base runs with `--minimap2`, `--bowtie2` or `--hisat2` do not need this step: they read the genome FASTA directly, or a normal (unconverted) Bowtie 2 or HISAT2 index. The default 5-Base engine, bwa-mem3, needs the index below. See [usage/align-5-base-reads.md](align-5-base-reads.md).

## bwa-mem3 index (`--bwamem3`)

```
bismark prepare --bwamem3 --parallel 16 /data/genomes/GRCh38/
```

`run_bwamem3` in `genome_prep/pipeline.rs` runs instead of the bisulfite conversion. No `Bisulfite_Genome/` is written.

| Step | What happens (`genome_prep/bwamem3.rs`) |
|---|---|
| Version | `check_version` runs `bwa-mem3 version`. Below 0.16.0 → error. A version outside the validated set (0.16.0, 0.17.0, 0.18.0) → a warning. |
| Staging | `stage_input` links a single FASTA, or writes the decompressed concatenation of several, as `genome.fa` in a new `Bwamem3_Index.partial.<random>/` folder. |
| Input checks | Before indexing, `stage_input` and `seq_line_len` reject inputs that bwa-mem3 reads differently from Bismark: spaces, tabs or other whitespace and non-ASCII bytes in sequence lines, lines that start with `+` or `@`, empty contigs, a lone `\r` line before a contig's first base, CR-only line ends, a header with no name, and duplicate names. LF and CRLF files pass. |
| Index | `run_index` runs `bwa-mem3 index --meth` in the staging folder, with `--tmp-dir` inside it. With bwa-mem3 0.18.0 or newer it adds `--meth-only` (`bwamem3_has_meth_only`), which writes only the 7 files below. Older versions build the full index, and the extra files are deleted. |
| Check | `prune_and_check` requires exactly the 7 files, fails on any other file (for example an `.alt` or `.hdr` sidecar), and compares the contig names and lengths in the `.ann` file with the genome. |
| Publish | `publish` renames the staging folder to `Bwamem3_Index/`. An existing index is moved to `Bwamem3_Index.old.<random>/` first, and deleted after the swap. |

The index is `<genome>/Bwamem3_Index/genome.fa.{amb,ann,pac,meth.amb,meth.ann,meth.pac,meth.bwt.2bit.64}`. The FASTA itself is not kept. `--taps` and `--illumina_5base` (with no engine flag, or `--bwamem3`) find this folder when `--genome` points at the genome folder and `--five_base_index` is not given.

| Option | Effect |
|---|---|
| `--bwamem3_max_memory <size>` | bwa-mem3's `--max-memory`, for example `120G`. Without it, bwa-mem3 uses the detected RAM less a reserve. Only with `--bwamem3`. |
| `--genomic_composition` | Also writes `genomic_nucleotide_frequencies.txt`, after the index is built. |

`--single_fasta`, `--slam`, `--large-index` and `--combined_genome` are rejected with `--bwamem3` (`resolve` in `genome_prep/cli.rs`). A FASTA that ends in `.meth.fa` in the genome folder is rejected as a bwa-mem3 index file (`reject_index_files`).

**Leftover folders.** Each run notes, and never deletes, the `Bwamem3_Index.partial.*` and `Bwamem3_Index.old.*` folders of earlier runs (`leftover_notes`). A failed build removes its own staging folder. A finished build that cannot be renamed into place is kept, and the error names it (`Bwamem3NotPublished` in `genome_prep/error.rs`).

## Links

- [usage/align-reads.md](align-reads.md): the next step.
- [troubleshooting/errors.md](../troubleshooting/errors.md): what the aligner says when an index file is missing, and the bwa-mem3 index errors.
- [usage/align-taps-reads.md](align-taps-reads.md), [usage/align-5-base-reads.md](align-5-base-reads.md): the runs that use the bwa-mem3 index.

## Open questions

- `--slam` is marked deprecated in `genome_prep/cli.rs`. When it is removed is not in the code.
- The time, memory and disk that a human genome needs come from measurements, not from the code. The log line in `stage_and_index` gives about 104 GB of RAM, and about 35 GB of free disk with `--meth-only` (up to 70 GB without).

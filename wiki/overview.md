---
type: overview
title: Bismark overview
description: What Bismark does, who uses it, the usual workflow order, when to use it, and the status of every subcommand.
tags: [methylation, bisulfite, overview]
page_id: overview
subject_status: current
updated: 2026-10-12
sources:
  - repo: Bismark
    path: rust/bismark/src/cli.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/aligner/config.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/extractor/cli.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/io/read.rs
    commit: 65b183a
---

# Bismark overview

## Summary

Bismark aligns bisulfite-converted sequencing reads to a genome and calls the methylation state of each cytosine in CpG, CHG and CHH context. It is one Rust binary, `bismark`, with 12 subcommands that cover genome preparation, alignment, deduplication, methylation extraction, coverage and cytosine reports, and HTML reports. The developer says it is used by labs that run bisulfite, EM-seq and Illumina 5-Base sequencing.

## Who uses it

The developer gave this: bisulfite-seq, EM-seq and 5-Base sequencing labs use Bismark to align reads and to call methylation in the different cytosine contexts.

## The usual workflow

The developer gave this order. Most workflows run these steps:

| Step | Subcommand | Runs |
|---|---|---|
| 1 | `bismark prepare` | Once per genome |
| 2 | `bismark` (align) | Per sample |
| 3 | `bismark dedup` | Per sample, except for RRBS, amplicon and implicon data |
| 4 | `bismark extract` | Per sample |
| 5 | `bismark bedgraph`, `bismark cov2cyt` | Per sample; `extract --bedGraph --cytosine_report` runs both in the same process |

## When to use it

**Good fit.** The developer gave this: WGBS, RRBS, EM-seq, single-cell methylation, 5-Base, amplicon and implicon data, for all library types (directional, non-directional and PBAT).

**Limits in the code.**

- Reads are FastQ (default) or FastA; `--pbat` accepts only FastQ and no `--gzip` (`resolve_library` in `aligner/config.rs`).
- minimap2 and rammap align single-end reads only; paired-end input is rejected (`reject_unsupported_paired_aligner` in `aligner/config.rs`).
- `--illumina_5base` needs paired-end, directional, FastQ input, and does not combine with `--combined_index`, `--slam` or `--rammap` (`resolve` and `resolve_aligner` in `aligner/config.rs`).
- The aligner writes BAM only; `--sam` and `--cram` are rejected (`resolve_output` in `aligner/config.rs`).
- `--slam`, `--non_bs_mm`, `--rg_tag`/`--rg_id`/`--rg_sample` and `--sam-no-hd` stop the run (`reject_unsupported_output_flags` in `aligner/config.rs`).
- `bismark dedup` and paired-end `bismark extract` reject coordinate-sorted BAMs (`@HD SO:coordinate`); single-end extraction accepts them (`check_not_coordinate_sorted` in `io/read.rs`).
- `extract --cytosine_report` needs `--genome_folder` (`Cli::validate` in `extractor/cli.rs`).

## Subcommands

Each tool runs as `bismark <subcommand>` or under its classic name (`run_legacy_alias` and `run_subcommand` in `cli.rs`). Bare `bismark <args>` runs the aligner.

| Subcommand | Classic name | What it does | Wiki page |
|---|---|---|---|
| `align` (default) | `bismark` | Aligns reads, writes BAM + alignment report | [usage/align-reads.md](usage/align-reads.md) |
| `prepare` | `bismark_genome_preparation` | Builds the C→T and G→A genome indexes | [usage/prepare-a-genome.md](usage/prepare-a-genome.md) |
| `dedup` | `deduplicate_bismark` | Removes PCR duplicate alignments | [usage/deduplicate-alignments.md](usage/deduplicate-alignments.md) |
| `extract` | `bismark_methylation_extractor` | Writes per-cytosine calls and M-bias | [usage/extract-methylation-calls.md](usage/extract-methylation-calls.md) |
| `bedgraph` | `bismark2bedGraph` | bedGraph + `.bismark.cov.gz` from call files | [usage/make-coverage-and-cytosine-reports.md](usage/make-coverage-and-cytosine-reports.md) |
| `cov2cyt` | `coverage2cytosine` | Genome-wide cytosine report from a `.cov` file | [usage/make-coverage-and-cytosine-reports.md](usage/make-coverage-and-cytosine-reports.md) |
| `bam2nuc` | `bam2nuc` | Mono- and di-nucleotide coverage of an alignment | No page yet |
| `nome` | `NOMe_filtering` | Per-read NOMe-seq methylation filtering | No page yet |
| `filter` | `filter_non_conversion` | Drops reads with apparent incomplete conversion | No page yet |
| `consistency` | `methylation_consistency` | Splits a BAM by read-level methylation consistency | No page yet |
| `report` | `bismark2report` | HTML report for one sample | No page yet |
| `summary` | `bismark2summary` | Multi-sample summary (`.txt` + `.html`) | No page yet |

## Components

| Component | Path | subject_status |
|---|---|---|
| Rust suite (one crate, one binary) | `rust/bismark/` | current |
| Perl Bismark v0.25.1 scripts. The developer says this is the legacy version. | `legacy_perl/` | legacy |

## Links

- [usage/install.md](usage/install.md): how to get the binary and the aligners it needs.
- [troubleshooting.md](troubleshooting.md): the questions users ask most often.
- [reference/bam-output.md](reference/bam-output.md): the BAM format every downstream step reads.

## Open questions

- The developer listed amplicon and implicon data as a good fit. The code has no mode named for either; which aligner options suit them is not in the code.

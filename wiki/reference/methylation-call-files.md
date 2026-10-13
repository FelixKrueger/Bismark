---
type: reference
title: Methylation call files
description: Columns and file names of the extractor call files, M-bias.txt, the bedGraph, the .bismark.cov.gz coverage file and the cytosine report.
tags: [format, call-files, m-bias, bedgraph, coverage, cytosine-report]
page_id: methylation-call-files
subject_status: current
updated: 2026-10-12
sources:
  - repo: Bismark
    path: rust/bismark/src/extractor/output.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/extractor/output_mode.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/extractor/mbias_writer.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/bedgraph/output.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/coverage2cytosine/report.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/coverage2cytosine/summary.rs
    commit: 65b183a
---

# Methylation call files

## Summary

These are the files that `bismark extract`, `bismark bedgraph` and `bismark cov2cyt` write. All are tab-separated text. Call files have one line per called cytosine per read; the bedGraph and coverage files have one line per covered genome position; the cytosine report has one line per cytosine in the genome. Positions are 1-based unless the column says otherwise.

## Call files

`{context}_{strand}_<name>.txt[.gz]`; other layouts are in [usage/extract-methylation-calls.md](../usage/extract-methylation-calls.md). The first line is `Bismark methylation extractor version v0.25.1` unless `--no_header` was given (`SPLIT_FILE_HEADER` in `extractor/output.rs`).

| Column | Content |
|---|---|
| 1 | Read name |
| 2 | `+` methylated, `-` unmethylated |
| 3 | Chromosome |
| 4 | Position, 1-based |
| 5 | The `XM` character (`Z`/`z`, `X`/`x`, `H`/`h`) |

`--yacht` writes `any_C_context_<name>.txt` with 8 columns: the 5 above, then read start, read end and orientation (`write_yacht_row` in `extractor/output_mode.rs`).

## M-bias.txt

`<name>.M-bias.txt`, where `<name>` is the input name without `gz`, `sam`, `bam`, `cram` and `txt` (`mbias_txt_path` in `extractor/mbias_writer.rs`). One section per context (CpG, CHG, CHH), headed `CpG context`; paired-end input has separate `(R1)` and `(R2)` sections.

| Column | Content |
|---|---|
| `position` | Read position, from 1 to the longest read |
| `count methylated` | Methylated calls at that position |
| `count unmethylated` | Unmethylated calls |
| `% methylation` | Methylated / all, 2 decimals; empty when there are no calls |
| `coverage` | All calls at that position |

## bedGraph

`<name>.bedGraph.gz`. First line `track type=bedGraph`, then (`write_outputs_from_sorted` in `bedgraph/output.rs`):

| Column | Content |
|---|---|
| 1 | Chromosome |
| 2 | Start, 0-based |
| 3 | End, 1-based (the cytosine position) |
| 4 | % methylation |

Positions with fewer reads than `--cutoff` are left out of the bedGraph and the coverage file.

## Coverage file (.bismark.cov.gz)

`<name>.bismark.cov.gz`, the input for `bismark cov2cyt`:

| Column | Content |
|---|---|
| 1 | Chromosome |
| 2 | Start, 1-based |
| 3 | End, 1-based (same as start) |
| 4 | % methylation |
| 5 | Count methylated |
| 6 | Count unmethylated |

`--zero_based` also writes a plain-text `.bismark.zero.cov` with a 0-based start.

## Cytosine report

`<name>.CpG_report.txt` (`CX_report.txt` with `--CX`, `.gz` with `--gzip`), one line per cytosine on both strands, covered or not (`report_name` in `coverage2cytosine/report.rs`):

| Column | Content |
|---|---|
| 1 | Chromosome |
| 2 | Position, 1-based (0-based with `--zero_based`) |
| 3 | Strand, `+` or `-` |
| 4 | Count methylated |
| 5 | Count unmethylated |
| 6 | Context: `CG`, `CHG` or `CHH` |
| 7 | Trinucleotide context, e.g. `CGT` |

`--merge_CpGs` adds `<report>.merged_CpG_evidence.cov`, with both strands of each CpG pooled. `--coverage_threshold` drops positions below the given coverage.

## Context summary

`<name>.cytosine_context_summary.txt`, never gzipped (`write_to` in `coverage2cytosine/summary.rs`): columns `upstream`, `C-context`, `full context`, `count methylated`, `count unmethylated`, `percent methylation` (`N/A` when there is no coverage). Contexts with a base other than A, C, G or T are not counted.

## Links

- [usage/make-coverage-and-cytosine-reports.md](../usage/make-coverage-and-cytosine-reports.md): how to make the bedGraph, coverage and cytosine report.
- [reference/bam-output.md](bam-output.md): the `XM` characters.

## Open questions

- The call-file header line names Perl version `v0.25.1`, not the Rust suite version. This keeps the files byte-identical to Perl output; no code comment says whether that will change.

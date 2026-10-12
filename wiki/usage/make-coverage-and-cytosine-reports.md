---
type: usage
title: Make coverage and cytosine reports
description: Build the bedGraph, the .bismark.cov.gz coverage file and the genome-wide cytosine report, from the extractor or with the standalone tools.
tags: [bedgraph, coverage, cytosine-report, coverage2cytosine]
page_id: make-coverage-and-cytosine-reports
subject_status: current
updated: 2026-10-12
sources:
  - repo: Bismark
    path: rust/bismark/src/extractor/downstream_filenames.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/extractor/cli.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/bedgraph/cli.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/bedgraph/filename.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/coverage2cytosine/cli.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/coverage2cytosine/report.rs
    commit: 65b183a
---

# Make coverage and cytosine reports

## Summary

`bismark bedgraph` (classic name `bismark2bedGraph`) sums the extractor's call files into one methylation level per position and writes a bedGraph and a `.bismark.cov.gz` coverage file. `bismark cov2cyt` (classic name `coverage2cytosine`) takes the coverage file and the genome and reports every cytosine on both strands, including the ones with no coverage. The developer says both are part of many workflows. The simplest way is to let the extractor run them in the same process.

## From the extractor (recommended path)

```
bismark extract --gzip --bedGraph --cytosine_report --genome_folder /data/genomes/GRCh38/ sample.deduplicated.bam
```

- `--cytosine_report` turns on `--bedGraph` too, and needs `--genome_folder` (`Cli::validate` in `extractor/cli.rs`).
- The extractor calls the bedGraph and cytosine-report code directly; no second program starts (`run_downstream_chain` in `extractor/downstream_filenames.rs`).
- Without `--CX`, only the CpG call files feed the bedGraph. If there are none, the extractor prints a warning and skips both steps.

Outputs, for an input `sample.deduplicated.bam` (`derive_bedgraph_filename`, `derive_coverage_filename`, `derive_cytosine_filename`):

| File | Content |
|---|---|
| `sample.deduplicated.bedGraph.gz` | bedGraph, one line per covered position |
| `sample.deduplicated.bismark.cov.gz` | Coverage file with methylated and unmethylated counts |
| `sample.deduplicated.CpG_report.txt` | Every CpG in the genome (`CX_report.txt` with `--CX`; `.gz` added with `--gzip`) |
| `sample.deduplicated.cytosine_context_summary.txt` | Methylation per trinucleotide context |

Column details: [reference/methylation-call-files.md](../reference/methylation-call-files.md).

## Options for these steps

| Option on `extract` | Effect |
|---|---|
| `--cutoff <n>` | Minimum coverage for a position to appear in the bedGraph and `.cov` (default 1) |
| `--CX` | All contexts (CpG, CHG, CHH), not only CpG; much slower |
| `--zero_based` | Also write 0-based coordinates; needs `--bedGraph` or `--cytosine_report` |
| `--ucsc` | Also write a UCSC bedGraph (`chr` prefix, `MT` → `chrM`); needs `--bedGraph` |
| `--split_by_chromosome` | One cytosine report per chromosome; needs `--cytosine_report` |

## Standalone tools

bedGraph from existing call files:

```
bismark bedgraph -o sample.bedGraph --dir out/ CpG_*_sample.txt.gz
```

- `-o` is required and takes a file name only; `.gz` is added if missing (`normalize_bedgraph_name` in `bedgraph/filename.rs`). `--dir` sets the folder.
- Without `--CX`, only input files whose name starts with `CpG` are used.
- Each input's first line is taken as a version header and skipped; pass `--no_header` if the call files have none.

Cytosine report from an existing coverage file:

```
bismark cov2cyt --genome_folder /data/genomes/GRCh38/ -o sample sample.bismark.cov.gz
```

| Option on `cov2cyt` | Effect |
|---|---|
| `-o <name>` | Output basename (required) |
| `--genome_folder <dir>` / `--genome` | Genome FASTA folder (required) |
| `--CX` | All contexts |
| `--coverage_threshold <n>` | Minimum coverage to report a position (default 0, report all) |
| `--merge_CpGs` | Pool the two strands of each CpG into one line |
| `--gzip` | Gzip the report and `.cov` outputs |
| `--gc`, `--nome-seq`, `--drach`, `--ffs` | GpC, NOMe-seq, DRACH and extended-context reports |

## Links

- [usage/extract-methylation-calls.md](extract-methylation-calls.md): the step before.
- [reference/methylation-call-files.md](../reference/methylation-call-files.md): file columns.
- [troubleshooting.md](../troubleshooting.md): low coverage.

## Open questions

- The doc comments on `--bedGraph` and `--cytosine_report` in `extractor/cli.rs` still say a subprocess runs; the code runs them in the same process.
- `--buffer_size`, `--gazillion` and `--ample_memory` are accepted for compatibility; `bismark bedgraph` always sorts in memory (`bedgraph/cli.rs`).

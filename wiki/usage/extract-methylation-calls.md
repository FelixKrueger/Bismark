---
type: usage
title: Extract methylation calls
description: Turn a Bismark BAM into per-cytosine call files and an M-bias table, and use M-bias to set the --ignore trimming.
tags: [methylation-extraction, m-bias, cpg, chg, chh]
page_id: extract-methylation-calls
subject_status: current
updated: 2026-10-12
sources:
  - repo: Bismark
    path: rust/bismark/src/extractor/cli.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/extractor/mod.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/extractor/call.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/extractor/output_mode.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/extractor/parallel.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/extractor/output.rs
    commit: 65b183a
---

# Extract methylation calls

## Summary

`bismark extract` reads the `XM` call string of every aligned read and writes one line per called cytosine, split by context (CpG, CHG, CHH) and strand. It also writes an M-bias table, which shows the methylation level at each read position. Use the M-bias table to decide how many bases to ignore at the read ends, then run the extraction again with `--ignore`.

## Steps

1. Use the deduplicated BAM (or the aligner BAM for RRBS, amplicon and implicon data).
2. Run it:

   ```
   bismark extract --gzip sample_R1_bismark_bt2_pe.deduplicated.bam
   ```

   Single-end or paired-end comes from the `@PG ID:Bismark` header line; if there is none, the run stops and asks for `-s` or `-p` (`process_one_file` in `extractor/mod.rs`). Several input files are processed one after the other, each with its own outputs.
3. Open `<name>.M-bias.txt` and look for read positions where the methylation level departs from the flat middle of the read.
4. If the ends are biased, run again with the trimming that removes them, for example `--ignore 2 --ignore_r2 2`.
5. For bedGraph, coverage and cytosine reports in the same run, add `--bedGraph` or `--cytosine_report --genome_folder <dir>`: [usage/make-coverage-and-cytosine-reports.md](make-coverage-and-cytosine-reports.md).

## Trim biased read ends

| Option | Removes |
|---|---|
| `--ignore <n>` | `n` bases from the 5' end of read 1 (or the single-end read) |
| `--ignore_r2 <n>` | `n` bases from the 5' end of read 2 |
| `--ignore_3prime <n>` | `n` bases from the 3' end of read 1 |
| `--ignore_3prime_r2 <n>` | `n` bases from the 3' end of read 2 |

All four default to 0. Soft-clipped bases count as read positions (`extract_calls` in `extractor/call.rs`).

## Paired-end overlap

For paired-end input, calls from read 2 that overlap read 1 are dropped by default, so a cytosine is counted once per fragment. `--include_overlap` keeps them; it is accepted only together with an explicit `-p` (`Cli::validate` in `extractor/cli.rs`). Coordinate-sorted paired-end input is rejected, because mates must be next to each other; coordinate-sorted single-end input is fine (`extract_pe_parallel` in `extractor/parallel.rs`).

## Output layout

| Option | Call files (`output_mode::mode_keys`) |
|---|---|
| none | 12: `{CpG,CHG,CHH}_{OT,CTOT,CTOB,OB}_<name>.txt` |
| `--comprehensive` | 3: `{CpG,CHG,CHH}_context_<name>.txt` |
| `--merge_non_CpG` | 8: `{CpG,Non_CpG}_{strand}_<name>.txt` |
| both | 2: `{CpG,Non_CpG}_context_<name>.txt` |
| `--mbias_only` | none, only M-bias and the splitting report |

`<name>` is the input file name without `.bam`, `.sam` or `.cram`. `--gzip` adds `.gz`. Files with no calls are deleted at the end, except when a run produces no calls at all and downstream outputs are expected; then empty files are kept (`OutputFileMap` sweep in `extractor/output.rs`). Column details: [reference/methylation-call-files.md](../reference/methylation-call-files.md).

## Other options

| Option | Effect |
|---|---|
| `-o <dir>` | Output folder (default: current folder), created if missing |
| `--parallel <n>` | Worker threads (default 1); `0` is rejected |
| `--no_header` | No version line at the top of each call file |
| `--mbias_off` | Skip the M-bias table |
| `--yacht` | Single-end NOMe-seq mode: one `any_C_context_<name>.txt` with read start, end and orientation |
| `-q` | Quiet log; warnings and errors still print |

## Links

- [reference/methylation-call-files.md](../reference/methylation-call-files.md): call file, M-bias and splitting-report columns.
- [reference/bam-output.md](../reference/bam-output.md): the `XM` string the calls come from.
- [troubleshooting.md](../troubleshooting.md): reading an M-bias plot.

## Open questions

- `--include_overlap` is rejected when single-end/paired-end is auto-detected, even for paired-end input. Whether that is intended is not in the code.
- The doc comment on `--parallel` in `extractor/cli.rs` says raising it does not speed up BAM extraction; no code measures this.

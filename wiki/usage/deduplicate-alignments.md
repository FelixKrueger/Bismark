---
type: usage
title: Deduplicate alignments
description: Remove PCR duplicates from a Bismark BAM, when to skip this step, and the UMI modes.
tags: [deduplication, pcr-duplicates, umi, rrbs]
page_id: deduplicate-alignments
subject_status: current
updated: 2026-10-12
sources:
  - repo: Bismark
    path: rust/bismark/src/dedup/cli.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/dedup/dedup.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/dedup/pipeline.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/dedup/filename.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/io/read.rs
    commit: 65b183a
---

# Deduplicate alignments

## Summary

`bismark dedup` keeps the first alignment at each position and strand and removes the rest as PCR duplicates. The developer says most workflows need this step, and that RRBS, amplicon and implicon data should skip it. Run it on the BAM that `bismark` wrote, before methylation extraction.

## When to run it

The developer gave this:

| Data | Deduplicate? |
|---|---|
| WGBS, EM-seq, single-cell, 5-Base | Yes |
| RRBS, amplicon, implicon | No |

For 5-Base runs the aligner can also remove duplicates by UMI during alignment, with `--five_base_umi_len`. See [usage/align-reads.md](align-reads.md).

## Steps

1. Use the BAM straight from the aligner. It must not be coordinate-sorted: the tool stops on `@HD SO:coordinate` (`check_not_coordinate_sorted` in `io/read.rs`).
2. Run it:

   ```
   bismark dedup --parallel 4 sample_R1_bismark_bt2_pe.bam
   ```

   The tool reads single-end or paired-end from the `@PG` line of the BAM header. Force it with `-s` or `-p`.
3. Check the removed share in `<name>.deduplication_report.txt`: [reference/run-reports.md](../reference/run-reports.md).
4. Extract methylation from the `.deduplicated.bam`: [usage/extract-methylation-calls.md](extract-methylation-calls.md).

## What counts as a duplicate

The key is strand, chromosome, start and end (`DedupKey` in `dedup/dedup.rs`):

- Single-end: one position. Forward strands (OT, CTOB) use the alignment start; reverse strands (CTOT, OB) use the alignment end, counting deletions but not soft clips (`compute_se_key` in `dedup/pipeline.rs`).
- Paired-end: the start and end of the pair (`compute_pe_key` in `dedup/pipeline.rs`).
- UMI modes add the UMI to the key: `--barcode` (alias `--umi`) takes the UMI after the last `:` of the read name, and `--bclconvert` takes it from the bcl-convert read-name position.

## Options this task uses

| Option | Effect |
|---|---|
| `-s` / `-p` | Force single-end or paired-end |
| `--parallel <n>` | BGZF compression threads for reading and writing; output is the same for any value. `0` is rejected. |
| `--sam` | Write SAM instead of BAM |
| `-o <name>` | Output basename; not allowed with more than one input file |
| `--output_dir <dir>` | Output folder, created if missing |
| `--multiple` | Treat all input files as one sample |
| `--barcode`, `--bclconvert` | UMI-aware deduplication |

`--representative` is rejected.

## Output files

From `dedup/filename.rs`:

| Input | Output |
|---|---|
| `<stem>.bam` | `<stem>.deduplicated.bam` and `<stem>.deduplication_report.txt` |
| With `--multiple` | `<stem>.multiple.deduplicated.bam` and `<stem>.multiple.deduplication_report.txt` |

## Links

- [reference/bam-output.md](../reference/bam-output.md): the strand fields the key uses.
- [troubleshooting.md](../troubleshooting.md): coverage questions.

## Open questions

- With `--barcode`, the tool takes whatever follows the last `:` of the read name. On bcl-convert read names that is the i7 index, not the UMI (doc comment in `dedup/cli.rs`); the tool cannot tell the two apart.

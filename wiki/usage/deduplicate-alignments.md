---
type: usage
title: Deduplicate alignments
description: Remove PCR duplicates from a Bismark BAM, when to skip this step, and the UMI modes.
tags: [deduplication, pcr-duplicates, umi, rrbs]
page_id: deduplicate-alignments
subject_status: current
updated: 2026-11-19
sources:
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/dedup/cli.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/dedup/dedup.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/dedup/pipeline.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/dedup/filename.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/io/read.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/dedup/mod.rs
    commit: a0059f4
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

1. Use the BAM straight from the aligner. For paired-end input it must not be coordinate-sorted, because mates must be next to each other: the tool stops on `@HD SO:coordinate` (`open_dedup_reader` in `dedup/pipeline.rs`). Coordinate-sorted single-end input is accepted.
2. Run it:

   ```
   bismark dedup --parallel 4 sample_R1_bismark_bt2_pe.bam
   ```

   The tool reads single-end or paired-end from the `@PG` line of the BAM header; if that line has no `-1`/`-2` but the first record has FLAG `0x1`, the file is paired-end. Force it with `-s` or `-p`.
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
| `--parallel <n>` | BGZF compression threads for reading and writing; output is the same for any value. `0` is rejected. With `--sam` or CRAM input it prints a warning and runs single-threaded (`process_one` in `dedup/mod.rs`). |
| `--cram_ref <fasta>` | Reference for CRAM input; NM and MD are regenerated from it (`fill_md_nm` in `io/read.rs`) |
| `--sam` | Write SAM instead of BAM |
| `-o <name>` | Output basename; not allowed with more than one input file |
| `--output_dir <dir>` | Output folder, created if missing |
| `--multiple` | Treat all input files as one sample; all must have the same `@SQ` names in the same order (`validate_chr_consistency` in `dedup/pipeline.rs`) |
| `--barcode`, `--bclconvert` | UMI-aware deduplication |

`--representative` is rejected.

## Output files

From `dedup/filename.rs`:

| Input | Output |
|---|---|
| `<stem>.bam` | `<stem>.deduplicated.bam` and `<stem>.deduplication_report.txt` |
| `<stem>.cram` (with `--cram_ref`) | `<stem>.deduplicated.bam` and `<stem>.deduplication_report.txt` (`derive_output_stem` in `dedup/filename.rs`) |
| With `--multiple` | `<stem>.multiple.deduplicated.bam` and `<stem>.multiple.deduplication_report.txt` |

## Links

- [reference/bam-output.md](../reference/bam-output.md): the strand fields the key uses.
- [troubleshooting.md](../troubleshooting.md): coverage questions.

## Open questions

- With `--barcode`, the tool takes whatever follows the last `:` of the read name. On bcl-convert read names that is the i7 index, not the UMI (doc comment in `dedup/cli.rs`); the tool cannot tell the two apart.

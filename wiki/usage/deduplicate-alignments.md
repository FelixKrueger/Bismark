---
type: usage
title: Deduplicate alignments
description: Remove PCR duplicates from a Bismark BAM, when to skip this step, the UMI modes, and UMI grouping with mismatches.
tags: [deduplication, pcr-duplicates, umi, umi-mismatches, rrbs]
page_id: deduplicate-alignments
subject_status: current
updated: 2026-11-20
sources:
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/dedup/cli.rs
    commit: c25f0a6
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/dedup/dedup.rs
    commit: c25f0a6
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/dedup/pipeline.rs
    commit: c25f0a6
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/dedup/filename.rs
    commit: c25f0a6
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/io/read.rs
    commit: a0059f4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/dedup/mod.rs
    commit: c25f0a6
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/dedup/umi_group.rs
    commit: c25f0a6
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/dedup/error.rs
    commit: c25f0a6
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/io/umi.rs
    commit: c25f0a6
---

# Deduplicate alignments

## Summary

`bismark dedup` keeps the first alignment at each position and strand and removes the rest as PCR duplicates. The developer says most workflows need this step, and that RRBS, amplicon and implicon data should skip it. Run it on the BAM that `bismark` wrote, before methylation extraction. With a UMI mode, `--umi_mismatches N` also groups UMIs at the same position that differ by up to N bases, with the UMI-tools directional rule, which also compares their counts.

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
- UMI modes add the UMI to the key: `--barcode` (alias `--umi`) takes the UMI after the last `:` of the read name, and `--bclconvert` takes it from the bcl-convert read-name position. With both flags, `--bclconvert` is used (`Cli::validate` in `dedup/cli.rs`).
- With `--barcode` (and no `--bclconvert`), the run stops if the first mapped read name of an input is in bcl-convert format (`:<UMI>_<mate>:N:<d>:<i7>`, `extract_bclconvert` in `io/umi.rs`), because the part after the last `:` is then the i7 index, not the UMI; use `--bclconvert` for those files. Only that first read name is checked (`check_bclconvert_format_conflict` in `dedup/mod.rs`).
- By default UMIs must match exactly. `--umi_mismatches` groups near-identical UMIs: see the next section.

## UMI mismatches (--umi_mismatches)

```
bismark dedup --barcode --umi_mismatches 1 sample_R1_bismark_bt2_pe.bam
```

`--umi_mismatches N` takes 0 to 255; the default 0 matches UMIs exactly. Above 0 it needs `--barcode`/`--umi` or `--bclconvert` (`Cli::validate` in `dedup/cli.rs`), and the UMIs at each position (strand, chromosome, start, end) are grouped with the UMI-tools "directional" rule (`group_position` in `dedup/umi_group.rs`):

- UMI B joins UMI A when they differ at N or fewer bases and count(A) ≥ 2 × count(B) − 1. A UMI seen once joins any neighbour; of two UMIs seen twice each, neither absorbs the other.
- Grouping chains (A takes B, B takes C), so a group can hold UMIs more than N apart.
- Groups start from the most frequent UMI; with tied counts, the UMI that sorts first leads.
- An `N` in a UMI matches only another `N`; against any other base it is a mismatch. UMIs of different length never group (`hamming_within`).

Each group keeps the first record or pair in the input, as without grouping (`UmiGroupCollector::resolve`). With `--multiple`, grouping spans all the input files.

Above 0, every input must be a regular file, not a pipe, stream or directory, checked before any input is opened (`run` in `dedup/mod.rs`). Without `--multiple` the inputs run one after another, so the outputs of earlier inputs are already written when a later input stops the run.

Each input is then read twice (`grouped_umi` in `dedup/pipeline.rs`):

1. Pass 1 counts each position+UMI key and notes its first record or pair. Memory grows with the number of distinct keys (`UmiGroupCollector`).
2. If an input's size or modification time changed between the start of pass 1 and the start of pass 2, the run stops before any output or report is written (`input_snapshot`).
3. Pass 2 writes the kept records, using 1 bit per record or pair (`KeepSet`). If it reads a different number of records or pairs than pass 1, or hits a read error, the output is removed and the run stops without a report. A change during pass 2 that keeps the file readable and the count the same is not detected.

All modes work with grouping: `-s`/`-p`, `--multiple`, `--parallel` (also with `--multiple`), CRAM and `--sam`. CRAM or SAM input and `--sam` output run single-threaded, as without grouping (`process_one` and `process_multiple` in `dedup/mod.rs`).

The report keeps its usual lines; `Duplicated alignments were found at` then counts groups with more than one record or pair ([reference/run-reports.md](../reference/run-reports.md)). Stderr adds `UMI grouping (up to <N> mismatches): <a> UMIs merged into <b> groups at <c> positions` (`mismatch` when N is 1; `grouping_summary` in `dedup/mod.rs`): <a> distinct UMIs that joined another UMI's group, <b> groups that hold more than one distinct UMI, and <c> positions with at least one such group (`UmiGrouping`).

## Options this task uses

| Option | Effect |
|---|---|
| `-s` / `-p` | Force single-end or paired-end |
| `--parallel <n>` | BGZF compression threads for reading and writing; output is the same for any value. `0` is rejected. With `--sam` or CRAM input it prints a warning and runs single-threaded; SAM input also runs single-threaded (`process_one` in `dedup/mod.rs`). |
| `--cram_ref <fasta>` | Reference for CRAM input; NM and MD are regenerated from it (`fill_md_nm` in `io/read.rs`) |
| `--sam` | Write SAM instead of BAM |
| `-o <name>` | Output basename; not allowed with more than one input file |
| `--output_dir <dir>` | Output folder, created if missing |
| `--multiple` | Treat all input files as one sample; all must have the same `@SQ` names in the same order (`validate_chr_consistency` in `dedup/pipeline.rs`) |
| `--barcode`, `--bclconvert` | UMI-aware deduplication |
| `--umi_mismatches <N>` | Above 0, group UMIs at the same position with the UMI-tools directional rule (up to N mismatches, counts compared); needs a UMI mode; default 0 (exact) |

`--representative` is rejected.

## Output files

From `dedup/filename.rs`:

| Input | Output |
|---|---|
| `<stem>.bam` | `<stem>.deduplicated.bam` and `<stem>.deduplication_report.txt` |
| `<stem>.cram` (with `--cram_ref`) | `<stem>.deduplicated.bam` and `<stem>.deduplication_report.txt` (`derive_output_stem` in `dedup/filename.rs`) |
| With `--multiple` | `<stem>.multiple.deduplicated.bam` and `<stem>.multiple.deduplication_report.txt` |

## Errors

From `BismarkDedupError` in `dedup/error.rs`:

| Message | Cause | Fix |
|---|---|---|
| `--umi_mismatches needs --barcode (--umi) or --bclconvert` | `--umi_mismatches` above 0 without a UMI mode | Add `--barcode` or `--bclconvert`, or drop `--umi_mismatches` |
| `--umi_mismatches reads each input twice, so it must be a regular file, not a pipe or stream: <path>` | A pipe, FIFO, process substitution or directory as input | Write the BAM to a file first |
| `input changed while it was being deduplicated: <path>` | The input's size or modification time changed during pass 1 | Rerun when nothing else writes the file |
| `the second pass read <n> records or pairs, the first <n>; the output was removed` | Pass 2 read a different number of records or pairs | Rerun when nothing else writes the file |
| `input file's qnames look like bcl-convert format …` | `--barcode` on bcl-convert read names | Use `--bclconvert` |

## Links

- [reference/bam-output.md](../reference/bam-output.md): the strand fields the key uses.
- [troubleshooting.md](../troubleshooting.md): coverage questions.
- [reference/run-reports.md](../reference/run-reports.md): the deduplication report lines.

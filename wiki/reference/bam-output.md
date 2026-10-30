---
type: reference
title: Bismark BAM records
description: The header, FLAG values and the XM, XR, XG, NM, MD, YS, CB and UR tags in the BAM that the aligner writes.
tags: [bam, xm-tag, methylation-call-string, format]
page_id: bam-output
subject_status: current
updated: 2026-10-12
sources:
  - repo: Bismark
    path: rust/bismark/src/aligner/output.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/aligner/methylation.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/aligner/report.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/aligner/cli.rs
    commit: 65b183a
---

# Bismark BAM records

## Summary

The aligner writes one BAM record per uniquely aligned read, or two adjacent records (read 1, then read 2) per uniquely aligned pair. Unmapped and ambiguous reads are not in this BAM. Each record carries the methylation call string `XM` and the two conversion tags `XR` and `XG`, which every downstream Bismark tool reads.

## Header

From `generate_sam_header` in `aligner/output.rs`:

| Line | Content |
|---|---|
| `@HD` | `VN:1.0 SO:unsorted` |
| `@SQ` | One per chromosome, in genome order, with `LN` |
| `@PG` | `ID:Bismark VN:<version> CL:"bismark <arguments>"` |

`bismark dedup` and `bismark extract` read the `@PG ID:Bismark` line to tell single-end from paired-end.

## Strand tags

`XR` is the read conversion and `XG` the genome conversion. Together they name the strand (labels from `print_final_analysis_report_single_end` in `aligner/report.rs`):

| XR | XG | Strand |
|---|---|---|
| `CT` | `CT` | OT, original top |
| `CT` | `GA` | OB, original bottom |
| `GA` | `CT` | CTOT, complementary to original top |
| `GA` | `GA` | CTOB, complementary to original bottom |

## FLAG

Single-end (`single_end_sam_output`): 0 for OT and CTOT, 16 for OB and CTOB.

Paired-end (`paired_end_sam_output`), fixed per strand. For CTOT and CTOB the first-in-pair and second-in-pair bits are swapped:

| Strand | Read 1 | Read 2 |
|---|---|---|
| OT | 99 | 147 |
| CTOB | 163 | 83 |
| CTOT | 147 | 99 |
| OB | 83 | 163 |

Pair the mates by their order in the file and their read name, not by the 0x40/0x80 bits. Both mates share MAPQ; `RNEXT` is `=`.

## Tags

In this order (`single_end_sam_output`, `build_pe_mate`):

| Tag | Type | Content |
|---|---|---|
| `NM` | i | Edit distance, including indels |
| `MD` | Z | Mismatch string |
| `XM` | Z | Methylation call per read base, same length as `SEQ` |
| `XR` | Z | Read conversion |
| `XG` | Z | Genome conversion |
| `YS` | Z | Only with `--strandID`, paired-end: `OT`, `CTOT`, `CTOB` or `OB` |
| `CB` | Z | Only with `--add_barcode`: the read-name part before the first `_` (`<barcode>_<umi>_<name>`) |
| `UR` | Z | Only with `--add_umi`: the part between the first and second `_` |

`CB` and `UR` are left out when their field is empty (`append_barcode_umi_tags`).

## XM call string

From `methylation_call` in `aligner/methylation.rs`. Upper case is methylated, lower case is unmethylated:

| Character | Context |
|---|---|
| `Z` / `z` | CpG |
| `X` / `x` | CHG |
| `H` / `h` | CHH |
| `U` / `u` | Unknown (CN or CHN, where an `N` sets the context) |
| `.` | Not a cytosine in the genome |

For reverse-strand records `SEQ` is reverse-complemented and `XM` is reversed, so both read in genome order. In 5-Base mode the same characters are used with inverted chemistry: a read `T` at a genomic C is methylated.

## 5-Base consensus BAMs

`<out>.5base_consensus.bam` and `<out>.5base_simplex.bam` carry `mx:i` (2 = duplex, 1 = simplex) when `--five_base_emit_multiplicity` is `simplex` or `both` (`--five_base_emit_multiplicity` in `aligner/cli.rs`).

## Links

- [usage/extract-methylation-calls.md](../usage/extract-methylation-calls.md): turns `XM` into call files.
- [usage/deduplicate-alignments.md](../usage/deduplicate-alignments.md): uses the strand and position.
- [reference/run-reports.md](run-reports.md): the per-strand counts.

## Open questions

- Which records the unique-best-hit rule keeps, and how MAPQ is computed, is in `aligner/merge.rs` and `aligner/mapq.rs`; this page does not cover them yet.

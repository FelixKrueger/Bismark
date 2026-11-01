---
type: reference
title: Run reports
description: The lines of the alignment report, the deduplication report and the extractor splitting report, and what each number means.
tags: [report, mapping-efficiency, deduplication, qc]
page_id: run-reports
subject_status: current
updated: 2026-11-01
sources:
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/report.rs
    commit: b4f901b
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/dedup/report.rs
    commit: b4f901b
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/extractor/output.rs
    commit: b4f901b
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/mod.rs
    commit: b4f901b
---

# Run reports

## Summary

Each main step writes a plain-text report. The alignment report gives the mapping efficiency, the counts per strand and the methylation levels per context. The deduplication report gives the share of alignments removed. The extractor's splitting report repeats the methylation levels after trimming and overlap removal. `bismark report` and `bismark summary` turn these files into HTML.

## Alignment report

`<name>_bismark_<token>_SE_report.txt` or `_PE_report.txt`, written by `aligner/report.rs`.

**Header** (`write_report_header`): the input file(s) and Bismark version, the aligner and its options, and one line naming the library type and which strands were aligned (`library_line`). The aligner line says `against the bisulfite genome of <folder>`; a 5-Base run says `against the unconverted genome of <folder>` (`write_five_base_report_header`).

**Final Alignment report** (`print_final_analysis_report_single_end`, `print_final_analysis_report_paired_ends`). Paired-end lines say "Sequence pairs" where single-end lines say "Sequences".

| Line | Meaning |
|---|---|
| `Sequences analysed in total` | Reads (or pairs) read from the input |
| `Number of alignments with a unique best hit…` | Reads written to the BAM |
| `Mapping efficiency` | Unique best hits / reads analysed, in % |
| `Sequences with no alignments under any condition` | No alignment on any strand |
| `Sequences did not map uniquely` | Equally good hits; not written to the BAM |
| `…discarded because genomic sequence could not be extracted` | The genome window around the hit (read length + 2) could not be cut out in full (`drive_merge` in `aligner/mod.rs`) |
| `CT/CT`, `CT/GA`, `GA/CT`, `GA/GA` | Unique hits per strand: OT, OB, CTOT, CTOB. Paired-end labels have 3 parts (`CT/GA/CT` = OT, `GA/CT/CT` = CTOT, `GA/CT/GA` = CTOB, `CT/GA/GA` = OB). |
| `Number of alignments to (merely theoretical) complementary strands being rejected` | Hits to the strands the library type does not align; left out in non-directional runs |

**Final Cytosine Methylation Report** (`write_cytosine_report`):

| Line | Meaning |
|---|---|
| `Total number of C's analysed` | Calls in CpG, CHG and CHH context |
| `Total methylated C's in <context> context` | Upper-case `XM` calls |
| `Total unmethylated C's in <context> context` | Lower-case `XM` calls |
| `C methylated in <context> context` | Methylated / all calls, in %; `Can't determine…` when there are no calls |

The report ends with `Bismark completed in <d>d <h>h <m>m <s>s`.

## Deduplication report

`<stem>.deduplication_report.txt`, written by `DedupReport` in `dedup/report.rs`:

| Line | Meaning |
|---|---|
| `Total number of alignments analysed in <file>` | Records (or pairs) read; ` (UMI mode)` is added with `--barcode`/`--bclconvert` |
| `Total number duplicated alignments removed` | Count and % removed |
| `Duplicated alignments were found at` | Distinct positions with at least one duplicate |
| `Total count of deduplicated leftover sequences` | Count and % kept |

## Splitting report

`<name>_splitting_report.txt` from `bismark extract` (`write_splitting_report` in `extractor/output.rs`). It lists the parameters used, including each `--ignore` value, then `Processed <n> lines in total`, `Total number of methylation call strings processed`, and a **Final Cytosine Methylation Report** with methylated counts and `C to T conversions` (unmethylated counts) per context, and the % methylated per context. With `--merge_non_CpG` the CHG and CHH lines are merged into `non-CpG`.

With `--allow_discordant`, the report ends with a `Discordant read handling (--allow_discordant)` section, which also goes to stderr unless `-q` is given. It is written even when every count is 0 (`discordant_section_text`):

| Line | Meaning |
|---|---|
| `Cross-chromosome pairs called independently` | Pairs whose mates are on different chromosomes |
| `Same-chromosome discordant pairs called independently` | Same-chromosome pairs that are not proper FR pairs from one strand |
| `Orphan reads (mate unmapped) called` | Mapped reads whose mate is absent |
| `Secondary alignments skipped`, `Supplementary alignments skipped` | Records with FLAG `0x100` or `0x800` |

## Links

- [troubleshooting.md](../troubleshooting.md): what a low mapping efficiency or odd strand counts point to.
- [reference/bam-output.md](bam-output.md): the strand names.
- [usage/align-reads.md](../usage/align-reads.md): where the report is written.

## Open questions

- `bismark report` and `bismark summary` read these files; their HTML output is not covered by this wiki yet.

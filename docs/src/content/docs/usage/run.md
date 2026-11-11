---
title: "Whole workflow in one command"
description: "bismark run takes one sample from raw FastQ to coverage files: Trim Galore, alignment, deduplication, methylation extraction and the HTML report, with each step streaming its output to the next in memory."
---

**USAGE:**

```bash
bismark run --genome <genome_folder> [--preset <library>] [options] {-1 <R1> -2 <R2> | <reads>}
```

`bismark run` runs the usual per-sample workflow in one command:

1. **Trim** with [Trim Galore](https://github.com/FelixKrueger/TrimGalore) (version 2, must be on your `PATH`; skip with `--skip_trim`).
2. **Align** (`bismark align`).
3. **Deduplicate** (`bismark dedup`), unless the preset or `--skip_dedup` turns it off.
4. **Extract** methylation calls with `--gzip --bedGraph --cytosine_report` (`bismark extract`), which writes the bedGraph, the `.bismark.cov.gz` coverage file and the genome-wide CpG report.
5. **Report**: the per-sample HTML report (`bismark report`).

The genome must already be prepared with `bismark prepare`. All files go to `--output_dir`, named after `--basename` (default: the first read file without its FastQ suffix).

## No intermediate file is read back

Steps 1 to 4 run at the same time, each handing its output to the next step in memory, so no step reads the previous step's output back from disk. The output is the same as running the steps one after the other.

- Trim Galore's trimmed reads go straight to the aligner and are not written to disk. The aligner then runs its instances multi-threaded (`-p`) instead of with `--multicore`, which would split the reads into chunk files first.
- The aligner still writes its BAM, and deduplication still writes the deduplicated BAM, but each also hands its records to the next step.

Trim Galore writes the trimmed FastQ to disk first, and the aligner reads it back, when the run cannot stream it: with the Trim Galore options `--fastqc`, `--fastqc_args`, `--gzip`, `--dont_gzip`, `--output-format`, `--hardtrim5`, `--hardtrim3`, `--clock`, `--implicon` or `--demux`, and with an alignment that does not stream its input (HISAT2, rammap, `--no_stream_converted`, `--combined_index`, 5-Base). A note says so.

## Presets

`--preset` applies the recommendations of the [library types](../library-types/) table. The default is `wgbs`.

| `--preset` | Trim Galore | Alignment | Deduplication | Extraction (paired-end) |
|---|---|---|---|---|
| `wgbs` (BS-Seq) | default | default | yes | `--ignore_r2 2` |
| `rrbs` | `--rrbs` | default | **no** | default |
| `pbat` | `--library pbat` | `--pbat` | yes | default |
| `scbs` (single-cell, single-end only) | `--library scbs` | `--non_directional` | yes | default |
| `accel` (Accel-NGS / Swift) | `--library accel` | default | yes | default |
| `zymo` (Pico-Methyl) | `--library zymo` | `--non_directional` | yes | default |
| `emseq` (NEB EM-seq) | `--library emseq` | default | yes | default |

## Passing options to a step

Any option of `trim_galore`, `bismark align`, `bismark dedup` or `bismark extract` can be given to `bismark run` under its own name; it goes to the step that has it:

```bash
bismark run --genome GRCh38/ --preset emseq -1 s_R1.fq.gz -2 s_R2.fq.gz -o out/ --parallel 4 \
    --length 30 --score_min L,0,-0.4 --barcode --CX
```

Here `--length` goes to Trim Galore, `--score_min` to the aligner, `--barcode` to deduplication (UMI mode) and `--CX` to the extractor.

- A name that several steps share with the same meaning (for example `--quiet`) goes to each of them.
- A short option that means different things in different steps is refused, with the long names to use instead. For example `-q` is `--quality` in Trim Galore, `--fastq` in the aligner and `--quiet` in the extractor.
- Options that `bismark run` sets itself are refused: paired-end or single-end mode, output folders and file names, thread counts, and the extractor's `--gzip`, `--bedGraph` and `--cytosine_report`.

## Options of `bismark run`

| Option | Meaning |
|---|---|
| `--genome <dir>` | Genome folder prepared with `bismark prepare` (also `--genome_folder`) |
| `-1 <R1> -2 <R2>` or `<reads>` | Paired-end or single-end reads |
| `--preset <library>` | Library type (see above); default `wgbs` |
| `-o`, `--output_dir <dir>` | Output folder, created if missing; default the current folder |
| `-B`, `--basename <name>` | Sample name for the output files |
| `--parallel <n>` | Threads for every step: Trim Galore `--cores`, dedup and extract `--parallel`, and about `n` aligner threads in all: `-p n/2` for the 2 Bowtie 2 instances of a directional library, `n/4` for the 4 of a non-directional one (`--multicore` instead when the trimmed reads are on disk). More threads than cores mostly spin. Default 1 |
| `--skip_trim` | The reads are already trimmed |
| `--skip_dedup` | Do not deduplicate, whatever the preset |

## Output files

For paired-end sample `s` with deduplication:

| Step | Files |
|---|---|
| Trim | `*_trimming_report.txt` (plus `s_val_1.fq.gz`, `s_val_2.fq.gz` when they are written to disk) |
| Align | `s_pe.bam`, `s_PE_report.txt` |
| Dedup | `s_pe.deduplicated.bam`, `s_pe.deduplication_report.txt` |
| Extract | call files, `s_pe.deduplicated.M-bias.txt`, `s_pe.deduplicated_splitting_report.txt`, `s_pe.deduplicated.bedGraph.gz`, `s_pe.deduplicated.bismark.cov.gz`, `s_pe.deduplicated.CpG_report.txt.gz` |
| Report | `s_PE_report.html` |

Single-end runs drop the `_pe`/`_PE` infix (`s.bam`, `s_SE_report.txt`, …); without deduplication the extractor's files are named after the aligned BAM (`s_pe.bismark.cov.gz`, …).

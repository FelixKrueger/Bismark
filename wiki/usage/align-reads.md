---
type: usage
title: Align reads
description: Run the aligner on single-end or paired-end reads, choose the library type, the aligner and the threads, and see how the converted reads reach the aligner.
tags: [alignment, bowtie2, hisat2, minimap2, library-type, 5-base]
page_id: align-reads
subject_status: current
updated: 2026-11-25
sources:
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/cli.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/config.rs
    commit: 457509a
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/options.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/mod.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/report.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/aux_out.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/stream.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/parallel.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/staging.rs
    commit: a199ad4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/ubam.rs
    commit: a199ad4
---

# Align reads

## Summary

`bismark` (or `bismark align`) converts each read in silico, aligns it against the converted genomes, keeps reads with a unique best hit, and writes a BAM with a methylation call string per read plus an alignment report. You choose the read layout, the library type and the aligner. The full option list is in [reference/aligner-cli.md](../reference/aligner-cli.md).

## Steps

1. Prepare the genome once: [usage/prepare-a-genome.md](prepare-a-genome.md).
2. Run the aligner.

   Single-end:

   ```
   bismark --genome /data/genomes/GRCh38/ sample.fq.gz
   ```

   Paired-end:

   ```
   bismark --genome /data/genomes/GRCh38/ -1 sample_R1.fq.gz -2 sample_R2.fq.gz
   ```

   The genome can also be the first positional argument. `-1` and `-2` take comma-separated lists of equal length (`resolve_layout` in `aligner/config.rs`). A positional uBAM, unaligned CRAM or BINSEQ file that holds pairs runs as paired-end, so `-I`/`-X` apply to it (`positional_input_is_paired` in `aligner/mod.rs`).
3. Read the alignment report and check the mapping efficiency: [reference/run-reports.md](../reference/run-reports.md).
4. Go on to [usage/deduplicate-alignments.md](deduplicate-alignments.md), or for RRBS, amplicon and implicon data straight to [usage/extract-methylation-calls.md](extract-methylation-calls.md).

## Choose the library type

| Library | Option | Strands aligned (`library_line` in `aligner/report.rs`) |
|---|---|---|
| Directional | none (default) | OT, OB |
| Non-directional | `--non_directional` | OT, OB, CTOT, CTOB |
| PBAT | `--pbat` | CTOT, CTOB |

`--non_directional` and `--pbat` together are rejected. `--pbat` needs FastQ input and no `--gzip` (`resolve_library` in `aligner/config.rs`). Users often ask which library type to pick: see [troubleshooting/data-quality.md](../troubleshooting/data-quality.md).

## Choose the aligner

| Aligner | Option | Layout | Notes |
|---|---|---|---|
| Bowtie 2 | none (default), `--bowtie2` | SE + PE | End-to-end; `--local` for local mode |
| HISAT2 | `--hisat2` | SE + PE | Spliced alignment unless `--no-spliced-alignment` |
| minimap2 | `--minimap2` | SE only | Preset `map-ont` by default; `--mm2_short_reads`, `--mm2_pacbio` |
| rammap | `--rammap` | SE only | Pure-Rust minimap2 reimplementation; not byte-identical to minimap2 |

Only one aligner can be selected (`resolve_aligner` in `aligner/config.rs`). The genome folder must hold the index for that aligner.

## Threads

| Option | Effect |
|---|---|
| `-p <n>` / `--threads <n>` | Threads per aligner instance, 2 or more. For Bowtie 2 and HISAT2 it also adds `--reorder` (`build_aligner_options` in `aligner/options.rs`). For minimap2 and rammap it sets `-t`. |
| `--multicore <n>` / `--parallel <n>` | Splits the input into `n` chunks that run in parallel. For HISAT2 it means one instance with `-p n` instead, and a run that gives both `--multicore` and `-p` is rejected; in 5-Base runs `-p` wins instead. |

A single-end directional run starts 2 aligner instances and a non-directional run starts 4 (`se_instance_plan` in `aligner/mod.rs`); each instance gets the `-p` threads.

## Converted reads

Each read file is converted (C→T, G→A) before it is aligned. By default the converted reads go to each aligner instance through a named pipe (FIFO) in `--temp_dir`, and no `_C_to_T` / `_G_to_A` temp file is written (`resolve_stream_converted` in `aligner/config.rs`, `aligner/stream.rs`). The output is the same either way. The configuration summary on stderr shows the choice in its `converted reads:` line (`RunConfig::summary` in `aligner/config.rs`).

| Run | Converted reads |
|---|---|
| Bowtie 2 or minimap2 | Streamed through FIFOs |
| rammap, in-process backend, FastQ input | Passed in memory |
| `--no_stream_converted` | Written to `--temp_dir` as files |
| HISAT2, `--combined_index*`, rammap through the external binary (`--rammap_subprocess`, FastA input, or a build without the in-process backend), or a `--temp_dir` where a FIFO cannot be made | Written as files; a `Note:` line says why (`run` in `aligner/mod.rs`) |
| `--illumina_5base` | None; the raw reads are aligned |

Converted-read files are temporary: each is deleted once its input file has been aligned (`run_se` and `run_pe` in `aligner/mod.rs`). A converted file is named after its input file, `<file name>_C_to_T.fastq` or `<file name>_G_to_A.fastq` (`converted_name` in `aligner/convert.rs`). When the two files of a pair have the same file name, ignoring ASCII case, each mate's converted files also carry its number, for example `r.fq_1_C_to_T.fastq` and `r.fq_2_C_to_T.fastq`, so the mates never write to the same file (`ConvertOptions::for_pair` in `aligner/convert.rs`). `--gzip` has no effect on streamed reads. Streaming does not remove every temp file: with Bowtie 2, minimap2 or external rammap, `--multicore` above 1 writes plain input chunks `<file>.temp.<pid>.<i>` (paired-end: `<file>.temp.<pid>.<mate>.<i>`) to `--temp_dir` (`split_contiguous`, `split_contiguous_pe` in `aligner/parallel.rs`), and uBAM, unaligned CRAM or BINSEQ input is decoded to FastQ under `<temp_dir>/.bismark_ubam/<pid>.<i>/` or `.bismark_binseq/<pid>.<i>/` (`resolve_alt_inputs` in `aligner/mod.rs`).

If something reads the converted input before the aligner does (for example an aligner wrapper script), use `--no_stream_converted`.

## Output files

Each input (read 1 for paired-end) names its own outputs: the file name with `.fastq.gz`, `.fq.gz`, `.fastq` or `.fq` removed, or for a uBAM, unaligned CRAM or BINSEQ input the file name without its last extension, plus `_1` for a paired file (`derive_output_path` in `aligner/mod.rs`, `file_stem` in `aligner/ubam.rs`). Two inputs whose outputs would get the same name, or `--basename` with more than one input, stop the run before anything is written (`reject_output_name_collision` in `aligner/config.rs`). With `--unmapped` or `--ambiguous`, two `-1`/`-2` pairs whose read-2 files have the same file name stop it too, because both would write the same `<read-2 file>_unmapped_reads_2.fq.gz` or `_ambiguous_reads_2.fq.gz`; read-2 names that differ only in their ending, such as `r_2.fq` and `r_2.fastq`, write separate files and run. The error names the shared output; rename one input, or run each with its own `-o` folder. The check compares the names the outputs get (`output_stem` in `aligner/mod.rs`), so a single-end uBAM `s.bam` collides with `s.fq.gz` or with a single-end unaligned `s.cram`, and a paired binary file is compared as `s_1`. A uBAM, unaligned CRAM or BINSEQ file given with `-1`/`-2` is rejected; pass it as one positional file, and a paired file is split into mates (`resolve_alt_inputs` in `aligner/mod.rs`). While an input is aligned its outputs are named `<name>.<pid>.partial`; they get their final names once that input has finished, and a failed run deletes them (`scope` in `aligner/staging.rs`). Aligner token: `bt2`, `hisat2`, `mm2` or `rammap`.

| Layout | BAM | Report |
|---|---|---|
| Single-end | `<name>_bismark_<token>.bam` | `<name>_bismark_<token>_SE_report.txt` |
| Paired-end | `<name>_bismark_<token>_pe.bam` | `<name>_bismark_<token>_PE_report.txt` |

`--basename <b>` replaces the whole name with `<b>.bam`, `<b>_pe.bam`, `<b>_SE_report.txt` or `<b>_PE_report.txt`. `--prefix <p>` adds `<p>.` in front. `-o <dir>` sets the folder and creates it if missing. `--unmapped` and `--ambiguous` also write those reads to `<input>_unmapped_reads.fq.gz` and `<input>_ambiguous_reads.fq.gz` (`_1`/`_2` per mate for paired-end; `.fa.gz` for FastA input), named in `aux_out.rs`.

## 5-Base data

`--illumina_5base` aligns Illumina 5-Base reads (methylated C read as T) with bwa-mem3 (the default), minimap2, Bowtie 2 or HISAT2 against the unconverted genome: see [usage/align-5-base-reads.md](align-5-base-reads.md).

## Links

- [reference/aligner-cli.md](../reference/aligner-cli.md): every option, its default and the combinations that stop the run.
- [reference/bam-output.md](../reference/bam-output.md): the records in the BAM.
- [usage/align-5-base-reads.md](align-5-base-reads.md): Illumina 5-Base data.
- [troubleshooting/data-quality.md](../troubleshooting/data-quality.md): low mapping efficiency.
- [troubleshooting/errors.md](../troubleshooting/errors.md): streamed-run errors, and streamed runs that stop on 3.2.0 and 3.3.0.

## Open questions

- How to split cores between `--multicore` and `-p` for best speed is not in the code.

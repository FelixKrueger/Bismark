---
type: usage
title: Align reads
description: Run the aligner on single-end or paired-end reads, choose the library type, the aligner and the threads, see how the converted reads reach the aligner, and run 5-Base data.
tags: [alignment, bowtie2, hisat2, minimap2, library-type, 5-base]
page_id: align-reads
subject_status: current
updated: 2026-11-06
sources:
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/cli.rs
    commit: da431b7
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/config.rs
    commit: da431b7
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/options.rs
    commit: da431b7
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/mod.rs
    commit: da431b7
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/methylation.rs
    commit: da431b7
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/report.rs
    commit: da431b7
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/aux_out.rs
    commit: da431b7
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/stream.rs
    commit: da431b7
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/parallel.rs
    commit: da431b7
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

   The genome can also be the first positional argument. `-1` and `-2` take comma-separated lists of equal length (`resolve_layout` in `aligner/config.rs`). A positional uBAM or BINSEQ file that holds pairs runs as paired-end, so `-I`/`-X` apply to it (`positional_input_is_paired` in `aligner/mod.rs`).
3. Read the alignment report and check the mapping efficiency: [reference/run-reports.md](../reference/run-reports.md).
4. Go on to [usage/deduplicate-alignments.md](deduplicate-alignments.md), or for RRBS, amplicon and implicon data straight to [usage/extract-methylation-calls.md](extract-methylation-calls.md).

## Choose the library type

| Library | Option | Strands aligned (`library_line` in `aligner/report.rs`) |
|---|---|---|
| Directional | none (default) | OT, OB |
| Non-directional | `--non_directional` | OT, OB, CTOT, CTOB |
| PBAT | `--pbat` | CTOT, CTOB |

`--non_directional` and `--pbat` together are rejected. `--pbat` needs FastQ input and no `--gzip` (`resolve_library` in `aligner/config.rs`). Users often ask which library type to pick: see [troubleshooting.md](../troubleshooting.md).

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

`--gzip` has no effect on streamed reads. Streaming does not remove every temp file: with Bowtie 2, minimap2 or external rammap, `--multicore` above 1 writes plain input chunks `<file>.temp.<i>` to `--temp_dir` (`split_contiguous` in `aligner/parallel.rs`), and uBAM or BINSEQ input is decoded to FastQ under `<temp_dir>/.bismark_ubam/` or `.bismark_binseq/` (`resolve_alt_inputs` in `aligner/mod.rs`).

If something reads the converted input before the aligner does (for example an aligner wrapper script), use `--no_stream_converted`.

## Output files

The name comes from the first read file with `.fastq.gz`, `.fq.gz`, `.fastq` or `.fq` removed (`derive_output_path` in `aligner/mod.rs`). Aligner token: `bt2`, `hisat2`, `mm2` or `rammap`.

| Layout | BAM | Report |
|---|---|---|
| Single-end | `<name>_bismark_<token>.bam` | `<name>_bismark_<token>_SE_report.txt` |
| Paired-end | `<name>_bismark_<token>_pe.bam` | `<name>_bismark_<token>_PE_report.txt` |

`--basename <b>` replaces the whole name with `<b>.bam`, `<b>_pe.bam`, `<b>_SE_report.txt` or `<b>_PE_report.txt`. `--prefix <p>` adds `<p>.` in front. `-o <dir>` sets the folder and creates it if missing. `--unmapped` and `--ambiguous` also write those reads to `<input>_unmapped_reads.fq.gz` and `<input>_ambiguous_reads.fq.gz` (`_1`/`_2` per mate for paired-end; `.fa.gz` for FastA input), named in `aux_out.rs`.

## 5-Base data

`--illumina_5base` handles the Illumina 5-Base chemistry, which converts methylated C to T. The aligner aligns the raw reads to the unconverted genome, and the call has inverted polarity: a read T at a genomic C means methylated (`methylation_call` in `aligner/methylation.rs`). The XM vocabulary stays the same.

```
bismark --illumina_5base --genome /data/genomes/GRCh38/ -1 R1.fq.gz -2 R2.fq.gz
```

- Paired-end, directional, FastQ only (`resolve` in `aligner/config.rs`).
- minimap2 `-x sr` against the genome FASTA is the default. For Bowtie 2 or HISAT2, add `--bowtie2` or `--hisat2` with `--five_base_index <basename>` of a normal, unconverted index. A bisulfite `BS_CT`/`BS_GA` index is rejected.
- Bowtie 2 and HISAT2 take the normal tuning options (`-N`, `-L`, `-D`, `-R`, `--score_min`, `--rdg`, `--rfg`, `-I`, `-X`, `--quiet`, `--phred33-quals`, `--phred64-quals`, and `--local` for Bowtie 2). The default `--score_min` is `L,0,-0.6`, or `G,20,8` with Bowtie 2 `--local`. Bowtie 2 gets `--maxins 800` unless `-X` is given, rejects an `-I` above that maximum, and never gets `--dovetail`. HISAT2 always gets `--no-spliced-alignment` and never `--no-softclip`, so `--local` changes nothing for it. `--known-splicesite-infile` is rejected (`build_five_base_aligner_options` and `default_linear_score` in `aligner/options.rs`).
- Threads come from `-p`, else `--multicore`, else all cores (`five_base_aligner_options` in `aligner/mod.rs`).
- Optional outputs, each next to the BAM: `--five_base_deconvolution` (`.5base_deconvolution.txt`), `--five_base_duplex` (`.5base_duplex.txt`), `--five_base_consensus` (`.5base_consensus.bam`, and `.5base_simplex.bam` with `--five_base_emit_multiplicity simplex|both`).
- `--five_base_bisulfite_bam <bam>` re-encodes an existing 5-Base BAM into bisulfite convention without re-aligning.

5-Base output is not byte-identical to any Perl output, because Perl Bismark has no 5-Base mode.

## Links

- [reference/aligner-cli.md](../reference/aligner-cli.md): every option, its default and the combinations that stop the run.
- [reference/bam-output.md](../reference/bam-output.md): the records in the BAM.
- [troubleshooting.md](../troubleshooting.md): low mapping efficiency.

## Open questions

- How to split cores between `--multicore` and `-p` for best speed is not in the code.

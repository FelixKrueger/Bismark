---
type: reference
title: Aligner options
description: Every option of the bismark aligner, its default, and the combinations that stop the run.
tags: [cli, alignment, options]
page_id: aligner-cli
subject_status: current
updated: 2026-11-26
sources:
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/cli.rs
    commit: a96f6db
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/config.rs
    commit: a96f6db
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/options.rs
    commit: d48c830
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/convert.rs
    commit: 667e20b
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/mod.rs
    commit: a96f6db
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/stream.rs
    commit: 667e20b
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/aligner.rs
    commit: 6b6f0b4
---

# Aligner options

## Summary

The options of `bismark` / `bismark align`, defined in `aligner/cli.rs` and checked in `resolve` (`aligner/config.rs`) and `build_aligner_options` (`aligner/options.rs`). Spellings keep the Perl underscores (`--non_directional`, `--path_to_bowtie2`). A rejected combination stops the run before any aligner starts.

## Input and genome

| Option | Default | Meaning |
|---|---|---|
| `--genome <dir>` (alias `--genome_folder`) | first positional | Prepared genome folder |
| `-1 <files>` / `-2 <files>` | | Paired-end mate files, comma-separated, same count |
| `--single_end <files>` (alias `--se`) | positional files | Single-end files, `,` or `:` separated |
| `-q` / `-f` (`--fastq` / `--fasta`) | FastQ | Read format; not both |
| `--phred33-quals` / `--phred64-quals` | Phred+33 | Quality encoding; FastQ only, not both |
| `-s <n>` / `-u <n>` (`--skip` / `--upto`) | all | Skip the first `n` reads, or stop after `n` |
| `--icpc` | off | Cut read IDs at the first space or tab instead of replacing whitespace with `_` |

## Library and aligner

| Option | Default | Meaning |
|---|---|---|
| `--non_directional` | directional | Align to all four strands |
| `--pbat` | directional | Align to CTOT and CTOB only |
| `--bowtie2` / `--hisat2` / `--minimap2` / `--rammap` | Bowtie 2 | Aligner; pick one; not with `--illumina_5base` or `--taps` (`reject_non_bwamem3_flags`) |
| `--bwamem3` | off | bwa-mem3, the only 5-Base engine; optional with `--illumina_5base`, which needs an index in `<genome>/Bwamem3_Index/` or `--five_base_index` |
| `--taps` | off | TAPS mode: `--illumina_5base --bwamem3` with `--meth=taps`; uses the same bwa-mem3 index (`expand_taps`, `resolve_five_base_index`). See [usage/align-taps-reads.md](../usage/align-taps-reads.md) |
| `--path_to_bowtie2` (and `_hisat2`, `_minimap2`, `_rammap`, `_bwamem3`) `<dir>` | `PATH` | Folder that holds the aligner binary; with `--illumina_5base` or `--taps`, only `--path_to_bwamem3` is accepted (`reject_non_bwamem3_flags`) |
| `--rammap_subprocess` | in-process | Run the external `rammap` binary; needs `--rammap` |
| `--combined_index` | off | Align against `Bisulfite_Genome/Combined/BS_combined` |
| `--combined_index_sequential` / `_parallel` / `_single_pass` | sequential | Execution model for `--combined_index --non_directional` |

## Alignment parameters

| Option | Default | Meaning |
|---|---|---|
| `-N <0\|1>` (`-n`, `--seedmms`) | aligner default | Seed mismatches; other values rejected |
| `-L <n>` (`-l`, `--seedlen`) | aligner default | Seed length |
| `-D <n>`, `-R <n>` | aligner default | Bowtie 2 seed-extension and re-seed tries |
| `--score_min <func>` | `L,0,-0.2`; `G,20,8` with Bowtie 2 `--local` | Minimum alignment score |
| `--local` | off | Local alignment; Bowtie 2 and HISAT2 only |
| `--rdg <a,b>` / `--rfg <a,b>` | `5,3` | Read and reference gap penalties |
| `-I <n>` / `-X <n>` (`--minins` / `--maxins`) | `-X 500` | Insert size; paired-end only, including a positional uBAM, unaligned CRAM or BINSEQ file that holds pairs; rejected in 5-Base and TAPS runs |
| `--no_dovetail` | dovetail on | Paired-end Bowtie 2 gets `--dovetail` unless this is set; `--dovetail` and `--no_dovetail` are rejected in 5-Base and TAPS runs |
| `-p <n>` / `--threads <n>` | off | Threads per instance, 2 or more |
| `--multicore <n>` / `--parallel <n>` | 1 | Input chunks run in parallel; 1 or more |
| `--known-splicesite-infile <file>`, `--no-spliced-alignment` | | HISAT2 only; not both |
| `--mm2_short_reads`, `--mm2_pacbio`, `--mm2_nanopore` | `map-ont` | minimap2 / rammap preset |
| `--mm2_maximum_length <n>` | 10000 | minimap2 / rammap length cutoff, 100 to 100000 |
| `--quiet` | off | Fewer Bowtie 2 warnings |

Every Bowtie 2 / HISAT2 run gets `--ignore-quals`, and every paired-end one gets `--no-mixed --no-discordant` (`build_aligner_options`).

## Output

| Option | Default | Meaning |
|---|---|---|
| `-o <dir>` (`--output_dir`) | current folder | Created with parents if missing |
| `--temp_dir <dir>` | current folder | Where the converted-read FIFOs or files, `--multicore` input chunks, and decoded uBAM/CRAM/BINSEQ input go during the run |
| `--no_stream_converted` | off | Write the converted reads to `--temp_dir` as files instead of streaming them (`resolve_stream_converted`); each file is deleted once its input has been aligned; see [usage/align-reads.md](../usage/align-reads.md) |
| `-B <name>` (`--basename`) | from read file | Output basename |
| `--prefix <p>` | none | Prefix plus `.`; trailing dots stripped |
| `--bam` | on | Accepted; BAM is the only output |
| `--gzip` | off | Gzip the converted reads when they are written as files (`convert.rs`); no effect when they are streamed; not with `--pbat` |
| `--unmapped` (`--un`), `--ambiguous`, `--ambig_bam` | off | Extra outputs for unmapped and ambiguous reads; with `--illumina_5base` or `--taps`, only `--unmapped` (the pairs counted as no alignment, as read) |
| `--add_barcode`, `--add_umi` | off | `CB:Z:` / `UR:Z:` tags from the read name |
| `--strandID` | off | `YS:Z:` strand tag; paired-end only, a note is printed for single-end |

## 5-Base options

All need `--illumina_5base` (alias `--five_base`): `--five_base_index`, `--five_base_umi_len`, `--five_base_umi_qname`, `--five_base_baseq`, `--five_base_min_mapq`, `--five_base_deconvolution`, `--five_base_duplex`, `--five_base_consensus`, `--five_base_consensus_from_bam`, `--five_base_bisulfite_bam`. `--five_base_emit_multiplicity duplex|simplex|both` needs `--five_base_consensus` or `--five_base_consensus_from_bam`. `--five_base_umi_qname` and `--five_base_umi_len` exclude each other. `--taps` sets `--illumina_5base` itself, and accepts only `--five_base_index`, `--five_base_baseq` and `--five_base_bisulfite_bam` of these.

5-Base and TAPS runs align with bwa-mem3 only. `--five_base_index` is the FASTA path a `bwa-mem3 index --meth` index was built from, or a folder written by `bismark prepare --bwamem3`; without it the run uses `<genome>/Bwamem3_Index/` (`resolve_five_base_index` in `aligner/config.rs`). The bwa-mem3 options are fixed (`bwamem3_options`); `-p` or `--multicore` set its `-t`, and the Bowtie 2 / HISAT2 parameters above except `--quiet` are rejected (`reject_bwamem3_tuning`), and the `--mm2_*` options by `reject_non_bwamem3_flags`. Which pairs are kept and which bases are not called: [usage/align-5-base-reads.md](../usage/align-5-base-reads.md).

## Rejected options and combinations

| Case | Where |
|---|---|
| `--slam` (hidden; there is no SLAM-seq mode) | `reject_slam`, before any other check in `run` (`aligner/mod.rs`) and `resolve` |
| `--sam`, `--cram` | `resolve_output` |
| `--non_bs_mm`, `--rg_tag`/`--rg_id`/`--rg_sample`, `--sam-no-hd` | `reject_unsupported_output_flags` |
| `--old_flag` | `reject_old_flag` |
| `--nucleotide_coverage`, `--old_flag`, `--unmapped`, `--ambiguous` or `--ambig_bam` with `--five_base_consensus_from_bam` / `--five_base_bisulfite_bam` | `run` in `aligner/mod.rs` |
| `--ambiguous` or `--ambig_bam` with `--illumina_5base` or `--taps` | `resolve` |
| minimap2 or rammap with paired-end reads, or a positional paired uBAM, unaligned CRAM or BINSEQ | `reject_unsupported_paired_aligner` |
| `-D` or `-R` with minimap2 or rammap | `build_aligner_options` in `aligner/options.rs` |
| Read files with Windows (CRLF) line endings | `reject_crlf_inputs` in `aligner/mod.rs`, before any aligner starts |
| `--local` with minimap2, rammap or `--combined_index` | `resolve` |
| `--combined_index` with `--multicore` > 1, or with minimap2 / rammap | `reject_combined_index_unsupported` |
| `--hisat2` with both `--multicore` and `-p` | `resolve` |
| `-p 1` | `build_aligner_options`; `build_five_base_aligner_options` in 5-Base and TAPS runs |
| `--mm2_*` options without minimap2 or rammap (outside 5-Base and TAPS) | `resolve_mm2_max_length` |
| `--bwamem3` without `--illumina_5base` or `--taps`, or with another aligner; `--five_base_index` without `--illumina_5base` or `--taps` | `resolve_aligner` |
| `--bowtie2`, `--hisat2`, `--minimap2` or `--rammap`, `--path_to_bowtie2`/`_hisat2`/`_minimap2`/`_rammap`, or an `--mm2_*` option with `--illumina_5base` or `--taps`, also with `--five_base_consensus_from_bam` or `--five_base_bisulfite_bam` | `reject_non_bwamem3_flags` (from `expand_taps`) |
| A Bowtie 2 or HISAT2 index as `--five_base_index` (also found under `$BOWTIE2_INDEXES`/`$HISAT2_INDEXES`; a folder is not checked) | `resolve_five_base_index` |
| A 5-Base or TAPS run with no `--five_base_index` and no `<genome>/Bwamem3_Index/`, or a `--five_base_index` folder that holds no index | `resolve_five_base_index` |
| A 5-Base or TAPS run with `--local`, `--score_min`, `-N`, `-L`, `-D`, `-R`, `--rdg`, `--rfg`, `-I`/`--minins`, `-X`/`--maxins`, `--dovetail`, `--no_dovetail`, `--no-spliced-alignment`, `--known-splicesite-infile` or `--most_valid_alignments` | `reject_bwamem3_tuning` |
| A bwa-mem3 run with a missing index file, or bwa-mem3 older than 0.16.0 | `check_bwamem3_index_files`, `detect_aligner` in `aligner/aligner.rs` |
| `--taps` with `--illumina_5base`, another aligner, a 5-Base-only option (duplex, consensus, deconvolution, UMI, `--five_base_min_mapq`, `--five_base_emit_multiplicity`), or the bwa-mem3 tuning options above | `expand_taps`, `reject_non_bwamem3_flags`, `reject_bwamem3_tuning` |
| A uBAM, unaligned CRAM or BINSEQ file given with `-1`/`-2` | `resolve_alt_inputs` in `aligner/mod.rs` |
| Two inputs whose outputs would get the same name (`output_stem` in `aligner/mod.rs`), or `--basename` with more than one input | `reject_output_name_collision` |
| `--unmapped` or `--ambiguous` with two `-1`/`-2` pairs whose read-2 files have the same file name | `reject_output_name_collision` |

Outside 5-Base and TAPS runs, `--most_valid_alignments` and `--dovetail` have no effect.

## Links

- [usage/align-reads.md](../usage/align-reads.md): which options a normal run needs.
- [reference/bam-output.md](bam-output.md): what the output records contain.

## Open questions

- Several option doc comments in `aligner/cli.rs` still say "deferred in v1" or "wired in Phase 9"; the code rejects or runs them as the tables say.

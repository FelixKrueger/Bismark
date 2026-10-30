---
type: reference
title: Aligner options
description: Every option of the bismark aligner, its default, and the combinations that stop the run.
tags: [cli, alignment, options]
page_id: aligner-cli
subject_status: current
updated: 2026-10-12
sources:
  - repo: Bismark
    path: rust/bismark/src/aligner/cli.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/aligner/config.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/aligner/options.rs
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/aligner/convert.rs
    commit: 65b183a
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
| `--bowtie2` / `--hisat2` / `--minimap2` / `--rammap` | Bowtie 2 | Aligner; pick one |
| `--path_to_bowtie2` (and `_hisat2`, `_minimap2`, `_rammap`) `<dir>` | `PATH` | Folder that holds the aligner binary |
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
| `-I <n>` / `-X <n>` (`--minins` / `--maxins`) | `-X 500` | Insert size; paired-end only |
| `--no_dovetail` | dovetail on | Paired-end Bowtie 2 gets `--dovetail` unless this is set |
| `-p <n>` / `--threads <n>` | off | Threads per instance, 2 or more |
| `--multicore <n>` / `--parallel <n>` | 1 | Input chunks run in parallel; 1 or more |
| `--known-splicesite-infile <file>`, `--no-spliced-alignment` | | HISAT2 only; not both |
| `--mm2_short_reads`, `--mm2_pacbio`, `--mm2_nanopore` | `map-ont` | minimap2 / rammap preset |
| `--mm2_maximum_length <n>` | 10000 | minimap2 / rammap length cutoff, 100 to 100000 |
| `--quiet` | off | Fewer Bowtie 2 warnings |

Every Bowtie 2 / HISAT2 run gets `--ignore-quals`, and every paired-end run gets `--no-mixed --no-discordant` (`build_aligner_options`).

## Output

| Option | Default | Meaning |
|---|---|---|
| `-o <dir>` (`--output_dir`) | current folder | Created with parents if missing |
| `--temp_dir <dir>` | current folder | Where the converted reads are written during the run |
| `-B <name>` (`--basename`) | from read file | Output basename |
| `--prefix <p>` | none | Prefix plus `.`; trailing dots stripped |
| `--bam` | on | Accepted; BAM is the only output |
| `--gzip` | off | Gzip the temporary converted reads (`convert.rs`); not with `--pbat` |
| `--unmapped` (`--un`), `--ambiguous`, `--ambig_bam` | off | Extra outputs for unmapped and ambiguous reads |
| `--add_barcode`, `--add_umi` | off | `CB:Z:` / `UR:Z:` tags from the read name |
| `--strandID` | off | `YS:Z:` strand tag; paired-end only, a note is printed for single-end |

## 5-Base options

All need `--illumina_5base` (alias `--five_base`): `--five_base_index`, `--five_base_umi_len`, `--five_base_umi_qname`, `--five_base_baseq`, `--five_base_min_mapq`, `--five_base_deconvolution`, `--five_base_duplex`, `--five_base_consensus`, `--five_base_consensus_from_bam`, `--five_base_bisulfite_bam`. `--five_base_emit_multiplicity duplex|simplex|both` needs `--five_base_consensus` or `--five_base_consensus_from_bam`. `--five_base_umi_qname` and `--five_base_umi_len` exclude each other.

## Rejected options and combinations

| Case | Where |
|---|---|
| `--sam`, `--cram` | `resolve_output` |
| `--slam`, `--non_bs_mm`, `--rg_tag`/`--rg_id`/`--rg_sample`, `--sam-no-hd` | `reject_unsupported_output_flags` |
| minimap2 or rammap with paired-end reads (outside 5-Base) | `reject_unsupported_paired_aligner` |
| `--local` with minimap2, rammap or `--combined_index` | `resolve` |
| `--combined_index` with `--multicore` > 1, or with minimap2 / rammap | `reject_combined_index_unsupported` |
| `--hisat2` with both `--multicore` and `-p` | `resolve` |
| `-p 1` | `build_aligner_options` |
| `--mm2_*` options without minimap2 or rammap | `resolve_mm2_max_length` |

`--nucleotide_coverage` and `--old_flag` are accepted, print a note that they are not active, and change nothing (`deferred_flags`). `--most_valid_alignments` and `--dovetail` have no effect.

## Links

- [usage/align-reads.md](../usage/align-reads.md): which options a normal run needs.
- [reference/bam-output.md](bam-output.md): what the output records contain.

## Open questions

- Several option doc comments in `aligner/cli.rs` still say "deferred in v1" or "wired in Phase 9"; the code rejects or runs them as the tables say.

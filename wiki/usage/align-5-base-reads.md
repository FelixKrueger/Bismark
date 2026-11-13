---
type: usage
title: Align 5-Base reads
description: Run Illumina 5-Base data with minimap2, Bowtie 2, HISAT2 or bwa-mem3, which pairs Bismark keeps, which bases it does not call, and the optional 5-Base outputs.
tags: [alignment, 5-base, minimap2, bowtie2, hisat2, bwa-mem3, umi]
page_id: align-5-base-reads
subject_status: current
updated: 2026-11-12
sources:
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/cli.rs
    commit: 0882360
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/config.rs
    commit: 0882360
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/options.rs
    commit: 0882360
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/mod.rs
    commit: 0882360
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/aligner.rs
    commit: 0882360
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/methylation.rs
    commit: 0882360
---

# Align 5-Base reads

## Summary

`--illumina_5base` handles the Illumina 5-Base chemistry, which converts methylated C to T. One engine aligns the raw reads to the unconverted genome, and Bismark makes the calls with inverted polarity: a read T at a genomic C means methylated (`methylation_call` in `aligner/methylation.rs`). The XM vocabulary is the same as for bisulfite data. 5-Base output is not byte-identical to any Perl output, because Perl Bismark has no 5-Base mode.

## Steps

1. Choose the engine (next section) and, except for minimap2, build its index once.
2. Run the aligner. Input is paired-end, directional FastQ (`resolve` in `aligner/config.rs`):

   ```
   bismark --illumina_5base --genome /data/genomes/GRCh38/ -1 R1.fq.gz -2 R2.fq.gz
   bismark --illumina_5base --bwamem3 --five_base_index /data/bwm3/GRCh38.fa \
           --genome /data/genomes/GRCh38/ -1 R1.fq.gz -2 R2.fq.gz
   ```

3. Read the report: [reference/run-reports.md](../reference/run-reports.md). The BAM is `<name>_bismark_<token>_pe.bam`, with token `mm2`, `bt2`, `hisat2` or `bwamem3` (`run_pe_five_base` in `aligner/mod.rs`).

## Choose the engine

| Engine | Options | Index |
|---|---|---|
| minimap2 (default) | none | None: the genome FASTA |
| Bowtie 2 | `--bowtie2 --five_base_index <basename>` | A normal, unconverted Bowtie 2 index |
| HISAT2 | `--hisat2 --five_base_index <basename>` | A normal, unconverted HISAT2 index |
| bwa-mem3 | `--bwamem3 --five_base_index <genome.fa>` | `bwa-mem3 index --meth <genome.fa>` |

Only one engine can be selected; `--bwamem3` with another aligner flag is rejected, and `--bwamem3` needs `--illumina_5base` (`resolve_aligner` in `aligner/config.rs`). `--path_to_bwamem3 <folder>` names the folder that holds `bwa-mem3`. A bisulfite `BS_CT`/`BS_GA` index is rejected. Threads come from `-p`, else `--multicore`, else all cores (`five_base_aligner_options` in `aligner/mod.rs`).

The developer measured that minimap2 and Bowtie 2 under-cover and under-call CpG-dense, heavily methylated regions, and that bwa-mem3 does not: see [troubleshooting.md](../troubleshooting.md).

### Bowtie 2 and HISAT2

They take the normal tuning options (`-N`, `-L`, `-D`, `-R`, `--score_min`, `--rdg`, `--rfg`, `-I`, `-X`, `--quiet`, `--phred33-quals`, `--phred64-quals`, and `--local` for Bowtie 2). The default `--score_min` is `L,0,-0.6`, or `G,20,8` with Bowtie 2 `--local`. Bowtie 2 gets `--maxins 800` unless `-X` is given, rejects an `-I` above that maximum, and never gets `--dovetail`. HISAT2 always gets `--no-spliced-alignment` and never `--no-softclip`, so `--local` changes nothing for it. `--known-splicesite-infile` is rejected (`build_five_base_aligner_options` and `default_linear_score` in `aligner/options.rs`).

### bwa-mem3

- **Version.** Read from `bwa-mem3 version`. Below 0.16.0 is rejected; another version runs with a warning that the 5-Base engine is validated against 0.16.0 (`detect_aligner` in `aligner/aligner.rs`).
- **Index files.** `<genome.fa>.amb`, `.ann`, `.pac` and `.meth.amb`, `.meth.ann`, `.meth.pac`, `.meth.bwt.2bit.64` must exist before the run starts (`check_bwamem3_index_files` in `aligner/config.rs`). The plain `.bwt.2bit.64` and `.meth.fa` are not needed.
- **Same assembly.** The contig names and lengths in the `.ann` file must equal the genome folder's before bwa-mem3 starts, and the `@SQ` lines bwa-mem3 writes must equal them again (`read_bwa_ann`, `check_contigs_match` and `five_base_align_and_call_pe` in `aligner/mod.rs`).
- **Keep the index out of `--genome`.** A FASTA in the genome folder whose name ends in `.meth.fa` stops the run (`run_pe_five_base`).
- **Fixed options.** `mem --meth=5base --meth-scoring neutral --meth-context all --meth-seed-prune=spec30 --meth-tags none -K 100000000 -t <n>` (`bwamem3_options` in `aligner/options.rs`). `BWAMEM3_METH_SEED_PRUNE` is removed from bwa-mem3's environment, with a note (`run_pe_five_base`).
- **Rejected options.** `--local`, `--score_min`, `-N`, `-L`, `-D`, `-R`, `--rdg`, `--rfg`, `--minins`, `--maxins`, `--dovetail`, `--no_dovetail`, `--no-spliced-alignment`, `--known-splicesite-infile` and `--most_valid_alignments` (`reject_bwamem3_tuning` in `aligner/options.rs`).

## Which pairs are kept

A pair is written only when both mates are mapped and flagged proper, lie on one contig, are on opposite strands, and the reverse mate ends after the forward mate starts (`five_base_pair_geometry` in `aligner/mod.rs`). MAPQ is not filtered. Pairs that fail the last three tests are counted under "Proper pairs rejected" and as pairs with no alignment. Secondary and supplementary records are skipped. Every pair must come back from the engine, in FASTQ order, and the run fails when R1, R2 and the engine output differ in length (`five_base_align_and_call_pe`).

## Bases that are not called

| Bases | When | Function |
|---|---|---|
| Bases below `--five_base_baseq` | Always | `mask_low_quality` |
| The first N bases of each mate | `--five_base_umi_len N` | `five_base_emit_pe_record` |
| Bases of a dovetailed or contained pair past the mate's 5' end | Always | `five_base_overhang_lens` |

All three are in `aligner/mod.rs`. They change `XM` only, so the extractor and `--five_base_bisulfite_bam` follow them. `--five_base_duplex`, `--five_base_consensus` and `--five_base_deconvolution` read the bases from `SEQ` and CIGAR, so the UMI and overhang masks do not apply to them. The report counts the overhang bases under "Dovetail overhang bases not called".

## Optional outputs

Each is written next to the BAM: `--five_base_deconvolution` (`.5base_deconvolution.txt`), `--five_base_duplex` (`.5base_duplex.txt`), `--five_base_consensus` (`.5base_consensus.bam`, and `.5base_simplex.bam` with `--five_base_emit_multiplicity simplex|both`). `--five_base_bisulfite_bam <bam>` re-encodes an existing 5-Base BAM into bisulfite convention without re-aligning (`run_pe_five_base`).

## Links

- [usage/align-reads.md](align-reads.md): the bisulfite aligner, output names and threads.
- [reference/aligner-cli.md](../reference/aligner-cli.md): every 5-Base option and the combinations that stop the run.
- [reference/bam-output.md](../reference/bam-output.md): the `@PG` lines and the records in the BAM.
- [troubleshooting.md](../troubleshooting.md): low 5-Base coverage at methylated CpG islands, and bwa-mem3 errors.

## Open questions

- None.

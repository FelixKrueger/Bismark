---
type: usage
title: Align 5-Base reads
description: Run Illumina 5-Base data with bwa-mem3 (the default), minimap2, Bowtie 2 or HISAT2, which pairs Bismark keeps, which bases it does not call, and the optional 5-Base outputs.
tags: [alignment, 5-base, minimap2, bowtie2, hisat2, bwa-mem3, umi]
page_id: align-5-base-reads
subject_status: current
updated: 2026-11-25
sources:
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/cli.rs
    commit: 457509a
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/config.rs
    commit: 457509a
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/options.rs
    commit: 457509a
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/mod.rs
    commit: 457509a
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/aux_out.rs
    commit: db29eb9
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/aligner.rs
    commit: 457509a
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/methylation.rs
    commit: 855b812
---

# Align 5-Base reads

## Summary

`--illumina_5base` handles the Illumina 5-Base chemistry, which converts methylated C to T. One engine aligns the raw reads to the unconverted genome, and Bismark makes the calls with inverted polarity: a read T at a genomic C means methylated (`methylation_call` in `aligner/methylation.rs`). The XM vocabulary is the same as for bisulfite data. 5-Base output is not byte-identical to any Perl output, because Perl Bismark has no 5-Base mode.

## Steps

1. Choose the engine (next section) and, except for minimap2, build its index once. For the default engine, bwa-mem3, that is `bismark prepare --bwamem3 <genome folder>` ([usage/prepare-a-genome.md](prepare-a-genome.md)).
2. Run the aligner. Input is paired-end, directional FastQ (`resolve` in `aligner/config.rs`):

   ```
   bismark --illumina_5base --genome /data/genomes/GRCh38/ -1 R1.fq.gz -2 R2.fq.gz
   bismark --illumina_5base --minimap2 --genome /data/genomes/GRCh38/ -1 R1.fq.gz -2 R2.fq.gz
   ```

3. Read the report: [reference/run-reports.md](../reference/run-reports.md). The BAM is `<name>_bismark_<token>_pe.bam`, with token `bwamem3` (default), `mm2`, `bt2` or `hisat2` (`run_pe_five_base` in `aligner/mod.rs`).

## Choose the engine

| Engine | Options | Index |
|---|---|---|
| bwa-mem3 (default) | none, or `--bwamem3`; `--five_base_index <genome.fa or folder>` for an index outside the genome folder | `bismark prepare --bwamem3 <genome folder>` ([usage/prepare-a-genome.md](prepare-a-genome.md)), or `bwa-mem3 index --meth <genome.fa>` |
| minimap2 | `--minimap2` | None: the genome FASTA; `--five_base_index` is rejected |
| Bowtie 2 | `--bowtie2 --five_base_index <basename>` | A normal, unconverted Bowtie 2 index |
| HISAT2 | `--hisat2 --five_base_index <basename>` | A normal, unconverted HISAT2 index |

With no engine flag, `--illumina_5base` aligns with bwa-mem3 (`resolve_aligner` in `aligner/config.rs`). Only one engine can be selected; `--bwamem3` with another aligner flag is rejected, `--bwamem3` needs `--illumina_5base`, and `--rammap` is rejected. With bwa-mem3, Bowtie 2 or HISAT2, the 5-Base option errors and `-p 1` come before the index and binary checks (`resolve`). `--path_to_bwamem3 <folder>` names the folder that holds `bwa-mem3`. A bisulfite `BS_CT`/`BS_GA` index is rejected. Threads come from `-p`, else `--multicore`, else all cores (`five_base_aligner_options` in `aligner/mod.rs`).

The developer measured that minimap2 and Bowtie 2 under-cover and under-call CpG-dense, heavily methylated regions, and that bwa-mem3 does not: see [troubleshooting/data-quality.md](../troubleshooting/data-quality.md).

### Runs with no engine flag

A run without `--bwamem3`, `--minimap2`, `--bowtie2` or `--hisat2` uses bwa-mem3 (`five_base_engine_defaulted` in `aligner/config.rs`). It aligns exactly as `--bwamem3` does; only some checks and messages differ:

- The start-up notice ends with `bwa-mem3 is the default engine since 3.5.0; --minimap2 selects the previous one.` (`five_base_notice` in `aligner/mod.rs`).
- With no `--five_base_index` and no `<genome folder>/Bwamem3_Index/`, the run stops before it writes anything, and names `bismark prepare --bwamem3`. When bwa-mem3 is also missing, the same message says so (`resolve_five_base_index`).
- A `--five_base_index` that is a Bowtie 2 or HISAT2 index (`.1.bt2`, `.1.bt2l`, `.1.ht2` or `.1.ht2l`, and no bwa-mem3 index) is rejected with "add `--bowtie2`" or "add `--hisat2`" (`unconverted_index_kind`).
- The index, version and binary errors, the `--mm2_*` errors and `--path_to_minimap2` (rejected) end with the `--minimap2` opt-out (`with_minimap2_opt_out`, `MINIMAP2_OPT_OUT`).
- A Bowtie 2 tuning option is rejected with "add `--bowtie2` (with `--five_base_index`)"; `--dovetail` and `--no_dovetail` also name `--minimap2`, `--no-spliced-alignment` names `--hisat2`, and `--known-splicesite-infile` gets no engine hint, because no 5-Base engine takes it (`reject_bwamem3_tuning` in `aligner/options.rs`).

### Bowtie 2 and HISAT2

They take the normal tuning options (`-N`, `-L`, `-D`, `-R`, `--score_min`, `--rdg`, `--rfg`, `-I`, `-X`, `--quiet`, `--phred33-quals`, `--phred64-quals`, and `--local` for Bowtie 2). The default `--score_min` is `L,0,-0.6`, or `G,20,8` with Bowtie 2 `--local`. Bowtie 2 gets `--maxins 800` unless `-X` is given, rejects an `-I` above that maximum, and never gets `--dovetail`. HISAT2 always gets `--no-spliced-alignment` and never `--no-softclip`, so `--local` changes nothing for it. `--known-splicesite-infile` is rejected (`build_five_base_aligner_options` and `default_linear_score` in `aligner/options.rs`).

### bwa-mem3

- **Version.** Read from `bwa-mem3 version`. Below 0.16.0 is rejected; 0.16.0, 0.17.0 and 0.18.0 are validated (`VALIDATED_BWAMEM3_VERSIONS`), and another version runs with a warning that the 5-Base engine is validated against them (`detect_aligner` in `aligner/aligner.rs`). CI and the container use 0.18.0 (`PINNED_BWAMEM3_VERSION`).
- **Where the index is found** (`resolve_five_base_index` in `aligner/config.rs`). Without `--five_base_index`, the run uses `<genome folder>/Bwamem3_Index/genome.fa`, the index that `bismark prepare --bwamem3` builds, and prints `Using the bwa-mem3 index …`. No such folder → with `--bwamem3` or `--taps` the error names both ways to build an index (`bwa-mem3 index --meth`, or `bismark prepare --bwamem3`); with no engine flag it names `bismark prepare --bwamem3` and `--five_base_index` (see above). `--five_base_index` takes the index prefix (the FASTA path it was built from), or a folder that holds a `prepare --bwamem3` index; a genome folder that only holds one in `Bwamem3_Index/` gets an error that names that subfolder.
- **Index files.** `<genome.fa>.amb`, `.ann`, `.pac` and `.meth.amb`, `.meth.ann`, `.meth.pac`, `.meth.bwt.2bit.64` must exist before the run starts (`check_bwamem3_index_files` in `aligner/config.rs`). The plain `.bwt.2bit.64` and `.meth.fa` are not needed.
- **Same assembly.** The contig names and lengths in the `.ann` file must equal the genome folder's before bwa-mem3 starts, and the `@SQ` lines bwa-mem3 writes must equal them again (`read_bwa_ann`, `check_contigs_match` and `five_base_align_and_call_pe` in `aligner/mod.rs`).
- **Keep a hand-built index out of the top of `--genome`.** A FASTA in the genome folder whose name ends in `.meth.fa` stops the run (`run_pe_five_base`). The `Bwamem3_Index/` subfolder holds no FASTA, so it is not affected.
- **Fixed options.** `mem --meth=5base --meth-scoring neutral --meth-context all --meth-seed-prune=spec30 --meth-tags none -K 100000000 -t <n>` (`bwamem3_options` in `aligner/options.rs`). `BWAMEM3_METH_SEED_PRUNE` is removed from bwa-mem3's environment, with a note (`run_pe_five_base`).
- **Rejected options.** `--local`, `--score_min`, `-N`, `-L`, `-D`, `-R`, `--rdg`, `--rfg`, `--minins`, `--maxins`, `--dovetail`, `--no_dovetail`, `--no-spliced-alignment`, `--known-splicesite-infile` and `--most_valid_alignments` (`reject_bwamem3_tuning` in `aligner/options.rs`).

## Which pairs are kept

A pair is written only when both mates are mapped and flagged proper, lie on one contig, are on opposite strands, and the reverse mate ends after the forward mate starts (`five_base_pair_geometry` in `aligner/mod.rs`). MAPQ is not filtered. A pair whose R1 is not flagged proper, or with an unmapped mate, counts as no alignment. A pair whose R2 is not flagged proper, or whose mates are on different contigs, on one strand, or point apart, is counted under "Proper pairs rejected" and as no alignment (`five_base_emit_pe_record`). Secondary and supplementary records are skipped. Every pair must come back from the engine, in FASTQ order, and the run fails when R1, R2 and the engine output differ in length (`five_base_align_and_call_pe`).

With `--unmapped`, each pair counted as no alignment is written to `<read-1 file>_unmapped_reads_1.fq.gz` and `<read-2 file>_unmapped_reads_2.fq.gz` (`aux_filename` in `aligner/aux_out.rs`; `--prefix`, `--basename` and `-o` as in [usage/align-reads.md](align-reads.md)), with each record's four lines as they were read, read names included; a last line without a newline gets one (`five_base_align_and_call_pe`, `write_raw_fastq_record`). A pair whose genomic window runs past a contig end is counted under "genomic sequence could not be extracted" and written nowhere (`five_base_emit_pe_record`). `--ambiguous` and `--ambig_bam` stop the run: every proper pair is kept at any MAPQ, so no pair is ambiguous (`resolve` in `aligner/config.rs`).

## Bases that are not called

| Bases | When | Function |
|---|---|---|
| Bases below `--five_base_baseq` | Always | `mask_low_quality` |
| The first N bases of each mate | `--five_base_umi_len N` | `five_base_emit_pe_record` |
| Bases of a dovetailed or contained pair past the mate's 5' end | Always | `five_base_overhang_lens` |

All three are in `aligner/mod.rs`. They change `XM`, so the extractor and `--five_base_bisulfite_bam` follow them. `--five_base_duplex`, `--five_base_consensus` and `--five_base_deconvolution` read the bases from `SEQ` and CIGAR; they skip the UMI and overhang bases through the `mk:B:I` tag (`five_base_called_range`). `--five_base_duplex` and `--five_base_deconvolution` also call a CpG cytosine only from a C or T (G or A on the minus strand) at or above `--five_base_baseq`, as `XM` does (`cpg_observations`); `--five_base_consensus` does not skip low-quality bases. In the mate overlap, deconvolution and duplex count R1's base; R2's base counts only at a CpG where R1 has no information: an `N`, a base below `--five_base_baseq`, or a deletion (`MateSpan` and `Covered`). A BAM written by an earlier version has no `mk` tag, so `--five_base_consensus_from_bam` still counts those bases in it. The report counts the overhang bases under "Dovetail overhang bases not called".

## Optional outputs

Each is written next to the BAM: `--five_base_deconvolution` (`.5base_deconvolution.txt`), `--five_base_duplex` (`.5base_duplex.txt`), `--five_base_consensus` (`.5base_consensus.bam`, and `.5base_simplex.bam` with `--five_base_emit_multiplicity simplex|both`). `--five_base_bisulfite_bam <bam>` re-encodes an existing 5-Base BAM into bisulfite convention without re-aligning (`run_five_base_bisulfite_standalone`). `--five_base_consensus_from_bam` and `--five_base_bisulfite_bam` align no reads, so `--unmapped`, `--ambiguous` and `--ambig_bam` stop them (`run` in `aligner/mod.rs`).

## Links

- [usage/align-reads.md](align-reads.md): the bisulfite aligner, output names and threads.
- [usage/align-taps-reads.md](align-taps-reads.md): `--taps`, which runs TAPS libraries on this path with bwa-mem3.
- [reference/aligner-cli.md](../reference/aligner-cli.md): every 5-Base option and the combinations that stop the run.
- [reference/bam-output.md](../reference/bam-output.md): the `@PG` lines and the records in the BAM.
- [troubleshooting/data-quality.md](../troubleshooting/data-quality.md): low 5-Base coverage at methylated CpG islands.
- [troubleshooting/errors.md](../troubleshooting/errors.md): bwa-mem3 errors.

## Open questions

- None.

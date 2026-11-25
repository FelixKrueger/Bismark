---
type: usage
title: Align 5-Base reads
description: Run Illumina 5-Base data with bwa-mem3, which pairs Bismark keeps, which bases it does not call, and the optional 5-Base outputs.
tags: [alignment, 5-base, bwa-mem3, umi]
page_id: align-5-base-reads
subject_status: current
updated: 2026-11-25
sources:
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/cli.rs
    commit: d48c830
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/config.rs
    commit: d48c830
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/options.rs
    commit: d48c830
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/mod.rs
    commit: d48c830
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/aux_out.rs
    commit: db29eb9
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/aligner.rs
    commit: 6b6f0b4
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/methylation.rs
    commit: 855b812
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/discovery.rs
    commit: d48c830
---

# Align 5-Base reads

## Summary

`--illumina_5base` handles the Illumina 5-Base chemistry, which converts methylated C to T. bwa-mem3 aligns the raw reads to the unconverted genome, and Bismark makes the calls with inverted polarity: a read T at a genomic C means methylated (`methylation_call` in `aligner/methylation.rs`). The XM vocabulary is the same as for bisulfite data. 5-Base output is not byte-identical to any Perl output, because Perl Bismark has no 5-Base mode.

## Steps

1. Build the bwa-mem3 index once: `bismark prepare --bwamem3 <genome folder>` ([usage/prepare-a-genome.md](prepare-a-genome.md)), or pass one with `--five_base_index`.
2. Run the aligner. Input is paired-end, directional FastQ (`resolve` in `aligner/config.rs`):

   ```
   bismark --illumina_5base --genome /data/genomes/GRCh38/ -1 R1.fq.gz -2 R2.fq.gz
   ```

3. Read the report: [reference/run-reports.md](../reference/run-reports.md). The BAM is `<name>_bismark_bwamem3_pe.bam` (`run_pe_five_base` in `aligner/mod.rs`).

## The engine

bwa-mem3 is the only 5-Base engine (`resolve_aligner` in `aligner/config.rs`); `--bwamem3` is optional. `--bowtie2`, `--hisat2`, `--minimap2`, `--rammap`, `--path_to_bowtie2`, `--path_to_hisat2`, `--path_to_minimap2`, `--path_to_rammap` and the `--mm2_*` options stop the run before anything is read or written, also with `--five_base_consensus_from_bam` and `--five_base_bisulfite_bam` (`reject_non_bwamem3_flags`, called from `expand_taps`, which `run` in `aligner/mod.rs` and `resolve` call first). `--bwamem3` needs `--illumina_5base` or `--taps`. The 5-Base option errors and `-p 1` come before the index and binary checks (`resolve`). `--path_to_bwamem3 <folder>` names the folder that holds `bwa-mem3`. Threads come from `-p`, else `--multicore` above 1, else all detected cores (2 if none are detected) (`five_base_threads` in `aligner/mod.rs`).

The developer measured that minimap2 and Bowtie 2 under-cover and under-call CpG-dense, heavily methylated regions, and that bwa-mem3 does not, and gives this as the reason bwa-mem3 is the only engine: see [troubleshooting/data-quality.md](../troubleshooting/data-quality.md).

- **Version.** Read from `bwa-mem3 version`. Below 0.16.0 is rejected; 0.16.0, 0.17.0 and 0.18.0 are validated (`VALIDATED_BWAMEM3_VERSIONS`), and another version runs with a warning that the 5-Base engine is validated against them (`detect_aligner` in `aligner/aligner.rs`). CI and the container use 0.18.0 (`PINNED_BWAMEM3_VERSION`).
- **Where the index is found** (`resolve_five_base_index` in `aligner/config.rs`). Without `--five_base_index`, the run uses `<genome folder>/Bwamem3_Index/genome.fa`, the index that `bismark prepare --bwamem3` builds, and prints `Using the bwa-mem3 index …`. No such folder → the run stops before it writes anything, with a message that names `bismark prepare --bwamem3` (about 1 h and 104 GB of RAM for GRCh38) and `--five_base_index`, and says so when bwa-mem3 is also missing. `--five_base_index` takes the index prefix (the FASTA path it was built from), or a folder that holds a `prepare --bwamem3` index; a genome folder that only holds one in `Bwamem3_Index/` gets an error that names that subfolder.
- **Not a bwa-mem3 index.** A `--five_base_index` that is a Bowtie 2 or HISAT2 index (`.1.bt2`, `.1.bt2l`, `.1.ht2` or `.1.ht2l` at the path or, as those aligners look, under `$BOWTIE2_INDEXES` or `$HISAT2_INDEXES`, and no bwa-mem3 index), a bisulfite `BS_CT`/`BS_GA` index included, is rejected as one, with the `bismark prepare --bwamem3` command (`unconverted_index_kind` in `aligner/config.rs`, `unconverted_index_found` in `aligner/discovery.rs`). A folder is not checked this way; it is read as a bwa-mem3 index folder.
- **Index files.** `<genome.fa>.amb`, `.ann`, `.pac` and `.meth.amb`, `.meth.ann`, `.meth.pac`, `.meth.bwt.2bit.64` must exist before the run starts (`check_bwamem3_index_files` in `aligner/config.rs`). The plain `.bwt.2bit.64` and `.meth.fa` are not needed.
- **Same assembly.** The contig names and lengths in the `.ann` file must equal the genome folder's before bwa-mem3 starts, and the `@SQ` lines bwa-mem3 writes must equal them again (`read_bwa_ann`, `check_contigs_match` and `five_base_align_and_call_pe` in `aligner/mod.rs`).
- **Keep a hand-built index out of the top of `--genome`.** A FASTA in the genome folder whose name ends in `.meth.fa` stops the run (`run_pe_five_base`). The `Bwamem3_Index/` subfolder holds no FASTA, so it is not affected.
- **Fixed options.** `mem --meth=5base --meth-scoring neutral --meth-context all --meth-seed-prune=spec30 --meth-tags none -K 100000000 -t <n>` (`bwamem3_options` in `aligner/options.rs`). `BWAMEM3_METH_SEED_PRUNE` is removed from bwa-mem3's environment, with a note (`run_pe_five_base`).
- **Rejected options.** `--local`, `--score_min`, `-N`, `-L`, `-D`, `-R`, `--rdg`, `--rfg`, `--minins`, `--maxins`, `--dovetail`, `--no_dovetail`, `--no-spliced-alignment`, `--known-splicesite-infile` and `--most_valid_alignments`, each with "drop it" (`reject_bwamem3_tuning` in `aligner/options.rs`).

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
- [troubleshooting/data-quality.md](../troubleshooting/data-quality.md): 5-Base coverage at methylated CpG islands.
- [troubleshooting/errors.md](../troubleshooting/errors.md): bwa-mem3 errors.

## Open questions

- None.

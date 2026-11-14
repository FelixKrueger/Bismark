---
type: usage
title: Align TAPS reads
description: Run TAPS data with --taps, what it turns on, the options it accepts and rejects, and how the run names itself.
tags: [alignment, taps, bwa-mem3, 5-base]
page_id: align-taps-reads
subject_status: current
updated: 2026-11-14
sources:
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/config.rs
    commit: d1dd2e8
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/cli.rs
    commit: d1dd2e8
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/options.rs
    commit: d1dd2e8
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/mod.rs
    commit: d1dd2e8
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/aligner.rs
    commit: d1dd2e8
---

# Align TAPS reads

## Summary

`--taps` runs TAPS libraries on the Illumina 5-Base path with bwa-mem3. `expand_taps` in `aligner/config.rs` checks the options, then turns `--taps` into `--illumina_5base --bwamem3` with the chemistry set to TAPS. bwa-mem3 gets `--meth=taps` instead of `--meth=5base`, and the calls use the 5-Base inverted polarity: a read T at a genomic C means methylated (`methylation_call` in `aligner/methylation.rs`). The `--taps` help text in `aligner/cli.rs` says that 5mC and 5hmC both read as methylated.

## Steps

1. Build the bwa-mem3 methylation index once, from the same FASTA as the `--genome` folder, and keep it outside that folder: `bwa-mem3 index --meth <genome.fa>`.
2. Run the aligner. Input is paired-end, directional FastQ:

   ```
   bismark --taps --five_base_index /data/bwm3/GRCh38.fa \
           --genome /data/genomes/GRCh38/ -1 R1.fq.gz -2 R2.fq.gz
   ```

3. Read the report: [reference/run-reports.md](../reference/run-reports.md). The output names are those of a bwa-mem3 5-Base run: `<name>_bismark_bwamem3_pe.bam` and `<name>_bismark_bwamem3_PE_report.txt` (`run_pe_five_base` in `aligner/mod.rs`).
4. Trim read ends at extraction, from the M-bias table: [usage/extract-methylation-calls.md](extract-methylation-calls.md). `--taps` does not trim.

## What `--taps` checks first

`expand_taps` stops the run, before anything starts, for each of these:

| Combination | Message names |
|---|---|
| `--taps` with `--illumina_5base` (or its alias `--five_base`) | different chemistries; pick one |
| `--taps` with `--bowtie2`, `--hisat2`, `--minimap2` or `--rammap` | `--taps` aligns with bwa-mem3 only |
| `--five_base_duplex`, `--five_base_consensus`, `--five_base_consensus_from_bam`, `--five_base_deconvolution`, `--five_base_umi_qname`, or `--five_base_emit_multiplicity` other than `duplex` | validated for Illumina 5-Base only, not TAPS |
| `--five_base_min_mapq` above 0 | it filters only the duplex/consensus pass, which `--taps` does not run |
| `--five_base_umi_len` above 0 | the 5-Base inline-UMI read structure is not validated for TAPS |

After the expansion, the 5-Base checks in `resolve` (`aligner/config.rs`) apply, with messages that name `--taps` and TAPS: paired-end only, directional only, FastQ only, and no `--slam` or `--combined_index`. The bwa-mem3 tuning options that `--bwamem3` rejects are rejected with "is not supported with --taps" (`reject_bwamem3_tuning` in `aligner/options.rs`).

## Options it accepts

| Option | With `--taps` |
|---|---|
| `--five_base_index <genome.fa>` | Required: the FASTA that the `bwa-mem3 index --meth` index was built from |
| `--five_base_baseq <n>` | Bases below Phred n are not called (`mask_low_quality` in `aligner/mod.rs`); 0 is off |
| `--five_base_bisulfite_bam <bam>` | Re-encodes a TAPS BAM to bisulfite convention without re-aligning (`run_pe_five_base`) |
| `-p`, `--multicore`, `--path_to_bwamem3`, output options | As for `--illumina_5base --bwamem3`: [usage/align-5-base-reads.md](align-5-base-reads.md) |

The bwa-mem3 options are fixed: `mem --meth=taps --meth-scoring neutral --meth-context all --meth-seed-prune=spec30 --meth-tags none -K 100000000 -t <n>` (`bwamem3_options` in `aligner/options.rs`). The index files, the contig check, the version check, which pairs are kept, and the overhang bases that are not called are the same as for `--illumina_5base --bwamem3`: see [usage/align-5-base-reads.md](align-5-base-reads.md).

## How the run names itself

| Where | What it says |
|---|---|
| Start-up notice (`five_base_notice` in `aligner/mod.rs`) | `Note: --taps is the TAPS (mC->T) mode`, aligned with bwa-mem3 to the unconverted genome, not byte-identical, validated on public TAPS data |
| Resolved configuration | a `TAPS index:` line (`aligner/config.rs`) |
| bwa-mem3 version messages (`detect_aligner` in `aligner/aligner.rs`) | `--taps needs bwa-mem3 >= 0.16.0`; another version warns that the TAPS engine is validated against 0.16.0 |
| Alignment report | `Library chemistry: TAPS (mC->T)` after the `bwa-mem3 version:` line (`run_pe_five_base`) |
| BAM `@PG` | Bismark's line holds the command line with `--taps`; bwa-mem3's holds `--meth=taps` |

Without `--taps`, every 5-Base message and output is unchanged: `FiveBaseChemistry` in `aligner/config.rs` gives the 5-Base strings for `--illumina_5base`.

## Links

- [usage/align-5-base-reads.md](align-5-base-reads.md): the shared 5-Base path, bwa-mem3 index and version rules, pairs kept, bases not called.
- [reference/aligner-cli.md](../reference/aligner-cli.md): every aligner option and the combinations that stop the run.
- [reference/run-reports.md](../reference/run-reports.md): the alignment report lines.
- [usage/extract-methylation-calls.md](extract-methylation-calls.md): M-bias and the `--ignore` trims.

## Open questions

- None.

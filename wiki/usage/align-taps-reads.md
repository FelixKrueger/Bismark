---
type: usage
title: Align TAPS reads
description: Run TAPS data with --taps, what it turns on, the options it accepts and rejects, and how the run names itself.
tags: [alignment, taps, bwa-mem3, 5-base]
page_id: align-taps-reads
subject_status: current
updated: 2026-11-24
sources:
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/config.rs
    commit: db29eb9
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/cli.rs
    commit: 855b812
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/options.rs
    commit: 855b812
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/mod.rs
    commit: db29eb9
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/aligner.rs
    commit: 855b812
  - repo: FelixKrueger/Bismark
    path: rust/bismark/src/aligner/methylation.rs
    commit: 855b812
---

# Align TAPS reads

## Summary

`--taps` runs TAPS libraries on the Illumina 5-Base path with bwa-mem3. `expand_taps` in `aligner/config.rs` checks the options, then turns `--taps` into `--illumina_5base --bwamem3` with the chemistry set to TAPS. bwa-mem3 gets `--meth=taps` instead of `--meth=5base`, and the calls use the 5-Base inverted polarity: a read T at a genomic C means methylated (`methylation_call` in `aligner/methylation.rs`). The `--taps` help text in `aligner/cli.rs` says that 5mC and 5hmC both read as methylated.

## Steps

1. Build the bwa-mem3 methylation index once: `bismark prepare --bwamem3 <genome folder>` writes it to `<genome folder>/Bwamem3_Index/` ([usage/prepare-a-genome.md](prepare-a-genome.md)). An index built by hand with `bwa-mem3 index --meth <genome.fa>` works too: build it from the same FASTA as the `--genome` folder, keep it outside that folder, and pass it with `--five_base_index`.
2. Run the aligner. Input is paired-end, directional FastQ:

   ```
   bismark --taps --genome /data/genomes/GRCh38/ -1 R1.fq.gz -2 R2.fq.gz
   ```

3. Read the report: [reference/run-reports.md](../reference/run-reports.md). The output names are those of a bwa-mem3 5-Base run: `<name>_bismark_bwamem3_pe.bam` and `<name>_bismark_bwamem3_PE_report.txt` (`run_pe_five_base` in `aligner/mod.rs`). A record whose dovetail overhang bases are not called carries `mk:B:I` (`five_base_emit_pe_record`); the extractor ignores it: [reference/bam-output.md](../reference/bam-output.md).
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

After the expansion, the 5-Base checks in `resolve` (`aligner/config.rs`) apply, with messages that name `--taps` and TAPS: paired-end only, directional only, FastQ only, and no `--slam`, `--combined_index`, `--ambiguous` or `--ambig_bam`. The bwa-mem3 tuning options that `--bwamem3` rejects are rejected with "is not supported with --taps" (`reject_bwamem3_tuning` in `aligner/options.rs`).

## Options it accepts

| Option | With `--taps` |
|---|---|
| `--five_base_index <genome.fa or folder>` | Optional when `<genome folder>/Bwamem3_Index/` exists. Otherwise the FASTA that the `bwa-mem3 index --meth` index was built from, or a folder written by `bismark prepare --bwamem3` (`resolve_five_base_index` in `aligner/config.rs`) |
| `--five_base_baseq <n>` | Bases below Phred n are not called (`mask_low_quality` in `aligner/mod.rs`); 0 is off |
| `--five_base_bisulfite_bam <bam>` | Re-encodes a TAPS BAM to bisulfite convention without re-aligning (`run_five_base_bisulfite_standalone` in `aligner/mod.rs`) |
| `-p`, `--multicore`, `--path_to_bwamem3`, output options | As for `--illumina_5base --bwamem3`: [usage/align-5-base-reads.md](align-5-base-reads.md) |
| `--nucleotide_coverage` | Runs bam2nuc on the final BAM (`run_nucleotide_coverage` in `aligner/mod.rs`); bam2nuc counts the genomic sequence under each alignment, so the chemistry does not change it |

The bwa-mem3 options are fixed: `mem --meth=taps --meth-scoring neutral --meth-context all --meth-seed-prune=spec30 --meth-tags none -K 100000000 -t <n>` (`bwamem3_options` in `aligner/options.rs`). The index files, the contig check, the version check, which pairs are kept, what `--unmapped` writes, and the overhang bases that are not called are the same as for `--illumina_5base --bwamem3`: see [usage/align-5-base-reads.md](align-5-base-reads.md).

## How the run names itself

| Where | What it says |
|---|---|
| Start-up notice (`five_base_notice` in `aligner/mod.rs`) | `Note: --taps is the TAPS (mC->T) mode`, aligned with bwa-mem3 to the unconverted genome, not byte-identical, validated on public TAPS data |
| Resolved configuration | a `TAPS index:` line (`aligner/config.rs`) |
| bwa-mem3 version messages (`detect_aligner` in `aligner/aligner.rs`) | `--taps needs bwa-mem3 >= 0.16.0`; a version other than 0.16.0, 0.17.0 or 0.18.0 warns that the TAPS engine is validated against those three |
| Alignment report | `Library chemistry: TAPS (mC->T)` after the `bwa-mem3 version:` line (`run_pe_five_base`) |
| BAM `@PG` | Bismark's line holds the command line with `--taps`; bwa-mem3's holds `--meth=taps` |

Without `--taps`, every 5-Base output and run message is unchanged: `FiveBaseChemistry` in `aligner/config.rs` gives the 5-Base strings for `--illumina_5base`. The `--five_base_baseq` and `--five_base_bisulfite_bam` errors for a run with neither flag name both: "requires --illumina_5base or --taps" (`resolve` in `aligner/config.rs`, `run_five_base_bisulfite_standalone`).

## Links

- [usage/align-5-base-reads.md](align-5-base-reads.md): the shared 5-Base path, bwa-mem3 index and version rules, pairs kept, bases not called.
- [reference/aligner-cli.md](../reference/aligner-cli.md): every aligner option and the combinations that stop the run.
- [reference/run-reports.md](../reference/run-reports.md): the alignment report lines.
- [usage/extract-methylation-calls.md](extract-methylation-calls.md): M-bias and the `--ignore` trims.

## Open questions

- None.

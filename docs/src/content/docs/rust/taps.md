---
title: "TAPS"
description: "How to align and call TAPS (TET-assisted pyridine borane sequencing) libraries with bismark --taps: bwa-mem3 placement on the unconverted genome, Bismark's inverted-polarity calls, read-end trimming and what is validated."
---

TAPS (TET-assisted pyridine borane sequencing) converts **methylated** cytosine to thymine and
leaves unmethylated cytosine intact, like [Illumina 5-Base](/Bismark/rust/illumina-5-base/). The
reads keep their full complexity and align to the **unconverted** genome, and Bismark calls
methylation at **inverted polarity**: a read `T` at a genomic `C` is methylated (`Z`), a read `C`
is unmethylated (`z`).

`--taps` runs the 5-Base calling path with [bwa-mem3](https://github.com/fg-labs/bwa-mem3) in its
TAPS mode (`mem --meth=taps`) as the only engine. bwa-mem3 places the reads; Bismark makes the
calls, writes the BAM and the report, and everything downstream (deduplication, the extractor,
bismark2report, bismark2summary) runs as usual.

- **Paired-end, directional libraries only.** Single-end input, `--non_directional`, `--pbat`,
  `--slam`, `--fasta` and `--combined_index` are rejected.
- **5mC and 5hmC both read as methylated.** TAPS converts both, so a call is their sum; Bismark
  does not separate them.
- **No variant calling.** A genuine `C>T` variant reads exactly like methylation on that strand.

## Running it

Build the bwa-mem3 methylation index once from the same FASTA as the `--genome` folder, and keep
it outside that folder:

```sh
bwa-mem3 index --meth /path/to/bwm3/genome.fa

bismark --taps --five_base_index /path/to/bwm3/genome.fa \
        --genome /path/to/GRCh38 -1 R1.fastq.gz -2 R2.fastq.gz
```

`--five_base_index` is the FASTA path the index was built from (the option name is shared with
the 5-Base mode). The index requirements, memory, runtime and the bwa-mem3 options Bismark fixes
are the same as for `--illumina_5base --bwamem3`; see
[bwa-mem3 (`--bwamem3`)](/Bismark/rust/illumina-5-base/#bwa-mem3---bwamem3). bwa-mem3 0.16.0 is the
validated version; older versions are rejected and newer ones run with a warning.

Output names are those of a bwa-mem3 5-Base run (`*_bismark_bwamem3_pe.bam`,
`*_bismark_bwamem3_PE_report.txt`). The run identifies itself as TAPS: the start-up notice, the
`@PG` lines (`--taps` in Bismark's, `--meth=taps` in bwa-mem3's) and a
`Library chemistry: TAPS (mC->T)` line in the report, after the bwa-mem3 version. MultiQC reads
the report as before and does not show the chemistry.

### Options

| Option | With `--taps` |
|---|---|
| `--five_base_index` | required: the FASTA the `bwa-mem3 index --meth` index was built from |
| `--five_base_baseq N` | accepted: read bases below Phred N are not called (0, the default, is off); `30` matches common TAPS callers |
| `--five_base_bisulfite_bam` | accepted: re-encodes a TAPS BAM to bisulfite convention, as for 5-Base |
| `-p` / `--multicore`, `--path_to_bwamem3`, `--upto`, output options | as for `--illumina_5base --bwamem3` |
| `--bowtie2`, `--hisat2`, `--minimap2`, `--rammap` | rejected: TAPS aligns with bwa-mem3 only |
| `--illumina_5base` | rejected: a different chemistry label for the same path |
| `--five_base_duplex`, `--five_base_consensus`, `--five_base_consensus_from_bam`, `--five_base_deconvolution`, `--five_base_umi_qname`, `--five_base_emit_multiplicity`, `--five_base_umi_len`, `--five_base_min_mapq` | rejected: validated for Illumina 5-Base only |
| Bowtie 2 / minimap2 tuning (`--local`, `--score_min`, `-N`, `-L`, `--minins`, `--maxins`, …) | rejected, as with `--bwamem3` |

## Read-end trimming

`--taps` does not trim. TAPS libraries often show methylation bias at the read ends (end repair,
and the first bases of read 2); handle it at extraction, as for bisulfite data:

1. Run the extractor once and look at its M-bias report (`*.M-bias.txt`), per read and strand.
2. Re-run it with `--ignore` / `--ignore_r2` (bases ignored at the 5' end of read 1 / read 2) and
   `--ignore_3prime` / `--ignore_3prime_r2` (at the 3' end).

If you already have [rastair](https://github.com/bsbludwig/rastair) trims, `--nOT` and `--nOB` take
`r1_start,r1_end,r2_start,r2_end`, counted from each read's own 5' and 3' ends
([rastair CLI reference](https://github.com/bsbludwig/rastair/blob/v2.2.0/docs/src/cli.md)). So
`--nOT 0,0,20,0 --nOB 0,0,20,0` is `--ignore_r2 20`. Bismark applies a trim to both strands; rastair
can trim OT and OB differently.

```sh
bismark_methylation_extractor -p --ignore_r2 20 --bedGraph --gzip sample_bismark_bwamem3_pe.bam
```

## Validation

`--taps` is concordance-validated, not byte-identical (Perl Bismark has no TAPS mode).

- **bwa-mem3 mode.** bwa-mem3 0.16.0 maps `--meth=taps` and `--meth=5base` to the same chemistry
  ([`src/meth_opts.cpp`](https://github.com/fg-labs/bwa-mem3/blob/v0.16.0/src/meth_opts.cpp#L18-L21)),
  and a `--taps` run produces the same BAM records as `--illumina_5base --bwamem3`. CI checks that
  on every change.
- **Spike-in controls (CI).** 6,316 public TAPS read pairs on the CpG-methylated lambda and the
  unmodified 2 kb controls, from the mouse ES cell TAPS library of Liu et al. (2019), GEO
  [GSE112520](https://www.ncbi.nlm.nih.gov/geo/query/acc.cgi?acc=GSE112520), as packaged for
  nf-core/methylseq in [nf-core/test-datasets#2370](https://github.com/nf-core/test-datasets/pull/2370).
  Untrimmed, `--taps` calls lambda CpGs 96.6% methylated on both strands and the unmodified control
  0.3–0.4%, with non-CpG contexts below 0.3%. The CI gate (`aligner_taps_bwamem3`) runs on amd64
  and arm64 and also checks the exact report and call counts.
- **Full public library.** The whole TAPS library of the same study, GSM3454335 (SRR8145389–92,
  455.5 million read pairs, PE 80 bp), aligned with `--taps --five_base_baseq 30` to UCSC mm9 plus
  the two controls, then deduplicated and kept at MAPQ ≥ 20. 96.8% of pairs aligned uniquely, and
  90.9% were left after deduplication. Compared with the authors' own per-strand CpG calls
  (`GSM3454335_TAPS_mm9_CpG_blk.bed.gz`) at coverage ≥ 10:
  - 99.8% of their 20.1 million CpGs are in Bismark's output. Shifting Bismark's positions by one
    base drops that to below 0.01%.
  - Pearson r of the per-CpG methylation is 0.993 on each strand (9.0 million CpGs per strand).
    Bismark reads 0.8 points higher on average.
  - Untrimmed, lambda CpGs are 96.5% methylated on both strands and the unmodified control 0.33%.
    Trimmed by the M-bias rule below (`--ignore 2 --ignore_r2 5`), lambda is 98.0–98.1% and the
    control 0.33%, as in the study.

  The trims came from Bismark's own M-bias report by a fixed rule, set before the run: trim each
  read end up to the last position whose CpG methylation differs by more than 2 points from the
  median of the read's central half. Read 2's first two bases (end repair) are the main bias here.

```sh
bwa-mem3 index --meth mm9_taps_spikes.fa       # mm9 chr*.fa + the two controls from genome_taps.fa
bismark --taps --five_base_index mm9_taps_spikes.fa --genome mm9_taps/ --five_base_baseq 30 \
        -1 TAPS_R1.fq.gz -2 TAPS_R2.fq.gz
deduplicate_bismark -p --bam TAPS_R1_bismark_bwamem3_pe.bam
samtools view -b -q 20 -o TAPS_q20.bam TAPS_R1_bismark_bwamem3_pe.deduplicated.bam
bismark_methylation_extractor -p --bedGraph --gzip TAPS_q20.bam
bismark_methylation_extractor -p --ignore 2 --ignore_r2 5 --bedGraph --gzip TAPS_q20.bam
```

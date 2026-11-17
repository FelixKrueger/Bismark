---
title: "TAPS"
description: "How to align and call TAPS (TET-assisted pyridine borane sequencing) libraries with bismark --taps: bwa-mem4 placement on the unconverted genome, Bismark's inverted-polarity calls, read-end trimming and what is validated."
---

TAPS (TET-assisted pyridine borane sequencing) converts **methylated** cytosine to thymine and
leaves unmethylated cytosine intact, like [Illumina 5-Base](/Bismark/rust/illumina-5-base/). The
reads keep their full complexity and align to the **unconverted** genome, and Bismark calls
methylation at **inverted polarity**: a read `T` at a genomic `C` is methylated (`Z`), a read `C`
is unmethylated (`z`).

`--taps` runs the 5-Base calling path with [bwa-mem4](https://github.com/IPNP-BIPN/bwa-mem4) in its
TAPS mode (`mem --meth=taps`) as the only engine. bwa-mem4 places the reads; Bismark makes the
calls, writes the BAM and the report, and everything downstream (deduplication, the extractor,
bismark2report, bismark2summary) runs as usual.

- **Paired-end, directional libraries only.** Single-end input, `--non_directional`, `--pbat`,
  `--slam`, `--fasta` and `--combined_index` are rejected.
- **5mC and 5hmC both read as methylated.** TAPS converts both, so a call is their sum; Bismark
  does not separate them.
- **No variant calling.** A genuine `C>T` variant reads exactly like methylation on that strand.

## Running it

Build the methylation index once from the same FASTA as the `--genome` folder, and keep
it outside that folder:

```sh
bwa-mem4 index --meth /path/to/bwm4/genome.fa

bismark --taps --five_base_index /path/to/bwm4/genome.fa \
        --genome /path/to/GRCh38 -1 R1.fastq.gz -2 R2.fastq.gz
```

`--five_base_index` is the FASTA path the index was built from (the option name is shared with
the 5-Base mode). The index requirements, memory, runtime and the bwa-mem4 options Bismark fixes
are the same as for `--illumina_5base --bwamem4`; see
[bwa-mem4 (`--bwamem4`)](/Bismark/rust/illumina-5-base/#bwa-mem4---bwamem4). An index built by
`bwa-mem3 index --meth` works too. bwa-mem4 4.5.0 is validated; older versions are rejected and
other versions run with a warning.

Output names are those of a bwa-mem4 5-Base run (`*_bismark_bwamem4_pe.bam`,
`*_bismark_bwamem4_PE_report.txt`). The run identifies itself as TAPS: the start-up notice, the
`@PG` lines (`--taps` in Bismark's, `--meth=taps` in bwa-mem4's) and a
`Library chemistry: TAPS (mC->T)` line in the report, after the bwa-mem4 version. MultiQC reads
the report as before and does not show the chemistry. A read whose dovetail overhang bases are
not called carries an `mk:B:I` tag (the bases masked at its 5' and 3' end); the extractor
ignores it.

### Options

| Option | With `--taps` |
|---|---|
| `--five_base_index` | required: the FASTA the `bwa-mem4 index --meth` (or `bwa-mem3 index --meth`) index was built from |
| `--five_base_baseq N` | accepted: read bases below Phred N are not called (0, the default, is off); `30` matches common TAPS callers |
| `--five_base_bisulfite_bam` | accepted: re-encodes a TAPS BAM to bisulfite convention, as for 5-Base |
| `-p` / `--multicore`, `--path_to_bwamem4`, `--upto`, output options | as for `--illumina_5base --bwamem4` |
| `--nucleotide_coverage` | accepted: runs bam2nuc on the BAM, which counts the genomic sequence under each alignment, so the chemistry does not matter |
| `--bowtie2`, `--hisat2`, `--minimap2`, `--rammap` | rejected: TAPS aligns with bwa-mem4 only |
| `--illumina_5base` | rejected: a different chemistry label for the same path |
| `--five_base_duplex`, `--five_base_consensus`, `--five_base_consensus_from_bam`, `--five_base_deconvolution`, `--five_base_umi_qname`, `--five_base_emit_multiplicity`, `--five_base_umi_len`, `--five_base_min_mapq` | rejected: validated for Illumina 5-Base only |
| Bowtie 2 / minimap2 tuning (`--local`, `--score_min`, `-N`, `-L`, `--minins`, `--maxins`, …) | rejected, as with `--bwamem4` |

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
bismark_methylation_extractor -p --ignore_r2 20 --bedGraph --gzip sample_bismark_bwamem4_pe.bam
```

## Validation

`--taps` is concordance-validated, not byte-identical (Perl Bismark has no TAPS mode). All numbers
below come from public data and can be reproduced with the commands at the end of this section.

### What CI checks on every change

- **bwa-mem4 mode.** bwa-mem4 maps `--meth=taps` and `--meth=5base` to the same chemistry
  ([`docs/meth-mode.md`](https://github.com/IPNP-BIPN/bwa-mem4/blob/v4.5.0/docs/meth-mode.md)), as
  bwa-mem3 0.16.0 and 0.17.0 do
  ([`src/meth_opts.cpp`](https://github.com/fg-labs/bwa-mem3/blob/v0.17.0/src/meth_opts.cpp#L18-L21)),
  and a `--taps` run produces the same BAM records as `--illumina_5base --bwamem4`.
- **Spike-in controls.** 6,316 public TAPS read pairs on the CpG-methylated lambda and the
  unmodified 2 kb controls, from the mouse ES cell TAPS library of Liu et al. (2019), GEO
  [GSE112520](https://www.ncbi.nlm.nih.gov/geo/query/acc.cgi?acc=GSE112520), as packaged for
  nf-core/methylseq in [nf-core/test-datasets#2370](https://github.com/nf-core/test-datasets/pull/2370).
  Untrimmed, `--taps` calls lambda CpGs 96.6% methylated on both strands and the unmodified control
  0.3–0.4%, with non-CpG contexts at most 0.6% (lambda CHG; CHH and the unmodified control below
  0.3%). The gate (`aligner_taps_bwamem4`) runs on amd64 and arm64 and also checks the exact report
  and call counts, which bwa-mem3 0.16.0 and 0.17.0 gave too.

### The full public TAPS library

The whole TAPS library of the same study, GSM3454335 (SRR8145389–92, 455.5 million read pairs,
PE 80 bp), was aligned with `--taps --five_base_baseq 30` (with bwa-mem3, before the engine moved
to bwa-mem4) to UCSC mm9 plus the two controls,
deduplicated and kept at MAPQ ≥ 20. 96.8% of pairs aligned uniquely and 90.9% were left after
deduplication. The calls were compared with the authors' per-strand CpG calls
(`GSM3454335_TAPS_mm9_CpG_blk.bed.gz`), on the 20.1 million CpGs with coverage ≥ 10 in their file.

| Compared with the authors' TAPS calls | Untrimmed | Trimmed (`--ignore 2 --ignore_r2 5`) |
|---|---|---|
| Their CpGs found in Bismark's output | 99.8% | 99.8% |
| … with Bismark coverage ≥ 10 too | 89.6% | 83.5% |
| Found after shifting Bismark's positions by one base | < 0.01% | < 0.01% |
| Pearson r of per-CpG methylation, + / − strand (9 million CpGs each) | **0.993 / 0.993** | 0.989 / 0.989 |
| Mean per-CpG difference, Bismark − authors | +0.8 points | +1.5 points |
| Genome-wide CpG methylation, Bismark / authors | 61.1% / 59.7% | 62.0% / 59.7% |

| Spike-in controls, + / − strand | Untrimmed | Trimmed |
|---|---|---|
| CpG-methylated lambda | 96.5% / 96.5% | 98.0% / 98.1% |
| Unmodified 2 kb control | 0.33% / 0.33% | 0.33% / 0.33% |

Trimmed, the controls match the study's own figures (about 98% and 0.4%). The trims came from
Bismark's M-bias report by a fixed rule, set before the run: trim each read end up to the last
position whose CpG methylation differs by more than 2 points from the median of the read's central
half. Here the rule removed the first 2 bases of read 1 and the first 5 of read 2; read 2's first
two bases (2.9% and 26.5% methylated, from end repair) are the main bias. No 3' trim was needed.

Bismark also calls 667,330 CpG positions (302,640 with coverage ≥ 10) that are not in the authors' file.

### The same DNA by bisulfite sequencing

The study also sequenced the same E14 DNA by whole-genome bisulfite sequencing (GSM3071998). Against
its processed CpG calls (`GSM3071998_WGBS_mm9_CpG_blk.bed.gz`), on the 2.9 million CpGs per strand
with coverage ≥ 10 in both, the `--taps` calls correlate at r = 0.72 (0.73 trimmed) and read 1–2
points lower per CpG on average; genome-wide, CpG methylation is 61–62% against 65.8%. This is a comparison across chemistries,
not a test of Bismark: the bisulfite library is shallow, and TAPS also counts 5hmC.

### Coverage in CpG-dense regions

CpG-dense, heavily methylated regions are hard for aligners that score every methylated CpG as a
mismatch (see [Illumina 5-Base](/Bismark/rust/illumina-5-base/)). The methylation-aware mode of
bwa-mem3, which bwa-mem4 ports, does not. The table compares Bismark's coverage with the authors' calls by CpG density (CpGs in a
151 bp window), for CpGs at least 80% methylated. Each value is Bismark's coverage relative to its
genome-wide mean, divided by the same ratio for the authors' calls, so 1.00 means the same coverage
profile.

| CpGs per 151 bp | 1–2 | 3–5 | 6–9 | 10–14 | ≥ 15 |
|---|---|---|---|---|---|
| CpGs in the cell | 5.2 M | 4.1 M | 0.70 M | 33,000 | 575 |
| Relative coverage, MAPQ ≥ 20 | 1.01 | 1.01 | 0.99 | 0.82 | 0.81 |
| Relative coverage, no MAPQ filter | 0.99 | 0.99 | 1.00 | 1.03 | 0.99 |
| Bismark coverage 0, MAPQ ≥ 20 | 0.3% | 0.3% | 1.0% | 7.7% | 10.6% |
| Bismark coverage 0, no MAPQ filter | 0.0% | 0.0% | 0.01% | 0.07% | 0.5% |

Without a MAPQ filter Bismark covers the densest methylated CpGs as well as the authors' calls do,
so the engine places those reads; the drop at MAPQ ≥ 20 comes from the filter, which removes the
low-MAPQ reads there. Dropping the filter keeps them but adds some noise from reads that map to
several places (r 0.993 → 0.992). In these cells, mean methylation agrees with the authors' within
about 1 point either way.

### Reports

`bismark report` and `bismark summary` run on a `--taps` run's reports unchanged, and MultiQC's
Bismark section is filled in as for 5-Base. None of them shows the chemistry.

### Reproduce

```sh
bwa-mem4 index --meth mm9_taps_spikes.fa       # mm9 chr*.fa + the two controls from genome_taps.fa
bismark --taps --five_base_index mm9_taps_spikes.fa --genome mm9_taps/ --five_base_baseq 30 \
        -1 TAPS_R1.fq.gz -2 TAPS_R2.fq.gz
deduplicate_bismark -p --bam TAPS_R1_bismark_bwamem4_pe.bam
samtools view -b -q 20 -o TAPS_q20.bam TAPS_R1_bismark_bwamem4_pe.deduplicated.bam
bismark_methylation_extractor -p --bedGraph --gzip TAPS_q20.bam
bismark_methylation_extractor -p --ignore 2 --ignore_r2 5 --bedGraph --gzip TAPS_q20.bam
```

The comparisons join Bismark's per-strand `.bismark.cov.gz` with the GEO files on chromosome,
position (GEO start + 1) and strand.

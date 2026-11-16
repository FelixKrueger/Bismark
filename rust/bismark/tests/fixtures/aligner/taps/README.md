# TAPS spike-in fixture

Public TAPS read pairs for `tests/aligner_taps_bwamem3.rs` (`--taps` through the real bwa-mem3).

| File | Contents |
|---|---|
| `taps_R1.fq.gz`, `taps_R2.fq.gz` | 6,316 TAPS read pairs whose two primary bwa-mem3 records lie on a spike-in contig |
| `spikes.fa` | the two spike-in contigs: `bacteriophage_lambda_CpG` (CpG-methylated lambda) and `2kb_3_Unmodified` |
| `expected.tsv` | the report counts and per contig/strand/context `XM` call counts a `--taps` run gives |

## Source

nf-core/test-datasets, `methylseq` branch ([#2370](https://github.com/nf-core/test-datasets/pull/2370), MIT):
`testdata/taps/taps_mm10_chr19_R{1,2}.fastq.gz` and `reference/genome_taps.fa.gz`. These are rastair v2.2.0's test
data, reused with its author's agreement: a subset of the mouse E14 ES cell TAPS library of Liu et al. (2019),
*Bisulfite-free direct detection of 5-methylcytosine and 5-hydroxymethylcytosine at base resolution*, Nature
Biotechnology 37, 424–429, GEO [GSE112520](https://www.ncbi.nlm.nih.gov/geo/query/acc.cgi?acc=GSE112520)
(GSM3454335).

## Selection

All 12,417 pairs were aligned with bwa-mem3 0.16.0 to `genome_taps.fa` (mm10 chr19 + the two spike-ins) using
Bismark's option string with `--meth=taps`. A pair is kept when both of its primary records (`-F 0x904`) are on
`bacteriophage_lambda_CpG` or `2kb_3_Unmodified`. `spikes.fa` is `samtools faidx genome_taps.fa
bacteriophage_lambda_CpG 2kb_3_Unmodified`.

## expected.tsv

Written by running `bismark --taps` on this fixture against `spikes.fa`, then counting `XM` calls per contig,
strand (`XG:Z:CT` = `+`, `GA` = `-`) and context with `samtools view | awk`, independently of the Rust test.
Calls are untrimmed.

## sha256

```
129083a0fd1ffdefdfcabcd43265c433df35d4e8bca39cf9e1777380f07c5bef  taps_R1.fq.gz
ff50f926a75a711dfd033d529c9f5b1987515f918b115fa505f923d362c90927  taps_R2.fq.gz
fa1a0e89edc18b8b98f8e4e880ec65f7e11fe350d039f837a63600a8b3dd0eb2  spikes.fa
626d36fffc555deee325c10fd417d9b3b8f5cbed6b8f9a3f1cad6af7d0788259  genome_taps.fa.gz (source)
10589e1cf2fedb4752c43ff013ef011b5d6339b28f3b7812ecf3c21bc2de1558  taps_mm10_chr19_R1.fastq.gz (source)
cb7178c7177529736ce764bc82cf0dbd037ee872b30890af0b565a437ac0f91f  taps_mm10_chr19_R2.fastq.gz (source)
```

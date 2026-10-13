# Bismark wiki

Bismark aligns bisulfite, EM-seq and Illumina 5-Base sequencing reads to a genome and calls cytosine methylation in CpG, CHG and CHH context. It is one Rust binary, `bismark`, with subcommands for genome preparation, alignment, deduplication, methylation extraction and coverage reports. This wiki describes the code on the `dev` branch.

## Routes

| The task mentions | Read these pages, and stop |
|---|---|
| install, Bioconda, cargo, Docker, container, methylseq | `usage/install.md` |
| genome index, `bismark_genome_preparation`, `Bisulfite_Genome` | `usage/prepare-a-genome.md` |
| align, map reads, paired-end, Bowtie 2, HISAT2, minimap2, threads | `usage/align-reads.md`, `reference/aligner-cli.md` |
| directional, non-directional, PBAT, library type | `usage/align-reads.md`, `troubleshooting.md` |
| 5-Base, `--illumina_5base`, duplex, consensus | `usage/align-reads.md`, `reference/aligner-cli.md` |
| an aligner option, a rejected flag | `reference/aligner-cli.md` |
| XM, XR, XG, FLAG, BAM tags, methylation call string | `reference/bam-output.md` |
| mapping efficiency, alignment report, deduplication report | `reference/run-reports.md`, `troubleshooting.md` |
| duplicates, `deduplicate_bismark`, UMI, RRBS | `usage/deduplicate-alignments.md` |
| methylation extractor, call files, `--ignore`, overlap | `usage/extract-methylation-calls.md`, `reference/methylation-call-files.md` |
| M-bias | `usage/extract-methylation-calls.md`, `troubleshooting.md` |
| bedGraph, `.cov`, coverage2cytosine, CpG report | `usage/make-coverage-and-cytosine-reports.md`, `reference/methylation-call-files.md` |
| low coverage, an error message | `troubleshooting.md` |
| which subcommands exist, when to use Bismark | `overview.md` |

## Pages

The full list with descriptions is in `index.md`.

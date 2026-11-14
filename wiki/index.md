# Bismark wiki index

## Overview

- `overview.md`: What Bismark does, who uses it, the usual workflow order, when to use it, and the status of every subcommand.

## Usage

- `usage/install.md`: The four ways to get the bismark binary, and the external aligners each one needs.
- `usage/prepare-a-genome.md`: Build the C-to-T and G-to-A converted genome and its indexes once, before the first alignment.
- `usage/align-reads.md`: Run the aligner on single-end or paired-end reads, choose the library type, the aligner and the threads, and see how the converted reads reach the aligner.
- `usage/align-5-base-reads.md`: Run Illumina 5-Base data with minimap2, Bowtie 2, HISAT2 or bwa-mem3, which pairs Bismark keeps, which bases it does not call, and the optional 5-Base outputs.
- `usage/align-taps-reads.md`: Run TAPS data with --taps, what it turns on, the options it accepts and rejects, and how the run names itself.
- `usage/deduplicate-alignments.md`: Remove PCR duplicates from a Bismark BAM, when to skip this step, and the UMI modes.
- `usage/extract-methylation-calls.md`: Turn a Bismark BAM into per-cytosine call files and an M-bias table, use M-bias to set the --ignore trimming, and read BAMs from other aligners with --allow_discordant.
- `usage/make-coverage-and-cytosine-reports.md`: Build the bedGraph, the .bismark.cov.gz coverage file and the genome-wide cytosine report, from the extractor or with the standalone tools.

## Reference

- `reference/aligner-cli.md`: Every option of the bismark aligner, its default, and the combinations that stop the run.
- `reference/bam-output.md`: The header, FLAG values and the XM, XR, XG, NM, MD, YS, CB and UR tags in the BAM that the aligner writes.
- `reference/run-reports.md`: The lines of the alignment report, the deduplication report and the extractor splitting report, and what each number means.
- `reference/methylation-call-files.md`: Columns and file names of the extractor call files, M-bias.txt, the bedGraph, the .bismark.cov.gz coverage file and the cytosine report.

## Troubleshooting

- `troubleshooting.md`: Low mapping efficiency, choosing the library type, coverage problems, low 5-Base coverage at methylated CpG islands, M-bias, the errors that stop a run (including bwa-mem3 errors), and the warnings that do not.

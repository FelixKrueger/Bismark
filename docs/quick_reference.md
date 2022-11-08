---
hide:
  - navigation
---

# Quick Reference

Bismark needs a working version of Perl and it is run from the command line. Furthermore, [Bowtie 2](http://bowtie-bio.sourceforge.net/bowtie2) or [HISAT2](https://ccb.jhu.edu/software/hisat2/index.shtml) needs to be installed on your computer. For more information on how to run Bismark with Bowtie 2 please go to the end of this manual.

As of version 0.14.0 or higher, Bismark may be run using parallelisation for both the alignment and the methylation extraction step. Search for `--parallel` / `--multicore` for more details below.

First you need to download a reference genome and place it in a genome folder. Genomes can be obtained e.g. from the [Ensembl](http://www.ensembl.org/info/data/ftp/index.html/) or [NCBI](ftp://ftp.ncbi.nih.gov/genomes/) websites. For the example below you would need to download the _Homo sapiens_ genome. Bismark supports reference genome sequence files in `FastA` format, allowed file extensions are either either `.fa` or `.fasta`. Both single-entry and multiple-entry `FastA` files are supported.

The following examples will use the file `test_dataset.fastq` which is available for download from the Bismark project or Github pages (it contains 10,000 reads in FastQ format, Phred33 qualities, 50 bp long reads, from a human directional BS-Seq library). An example report can be found in Appendix IV.

## Genome preparation

**USAGE:**

```bash
bismark_genome_preparation [options] <path_to_genome_folder>
```

A typical genome indexing could look like this:

```bash
/bismark/bismark_genome_preparation --path_to_aligner /usr/bin/bowtie2/ --verbose /data/genomes/homo_sapiens/GRCh37/
```

## Alignment

**USAGE:**

```bash
bismark [options] --genome <genome_folder> {-1 <mates1> -2 <mates2> | <singles>}
```

Typical alignment example:

```bash
bismark --genome /data/genomes/homo_sapiens/GRCh37/ test_dataset.fastq
```

This will produce two output files:

1. `test_dataset_bismark_bt2.bam` (contains all alignments plus methylation call strings)
2. `test_dataset_bismark_SE_report.txt` (contains alignment and methylation summary)

!!! note

    In order to work properly the current working directory must contain the sequence files to be analysed.

## Deduplication

```bash
deduplicate_bismark --bam [options] <filenames>
```

This command will deduplicate the Bismark alignment BAM file and remove all reads but one which align to the the very same position and in the same orientation. This step is recommended for whole-genome bisulfite samples, but should not be used for reduced representation libraries such as RRBS, amplicon or target enrichment libraries.

## Methylation extraction

**USAGE:**

```bash
bismark_methylation_extractor [options] <filenames>
```

A typical command to extract context-dependent (CpG/CHG/CHH) methylation could look like this:

```bash
bismark_methylation_extractor --gzip --bedGraph test_dataset_bismark_bt2.bam
```

This will produce three methytlation output files:

- `CpG_context_test_dataset_bismark_bt2.txt.gz`
- `CHG_context_test_dataset_bismark_bt2.txt.gz`
- `CHH_context_test_dataset_bismark_bt2.txt.gz`

as well as a bedGraph and a Bismark coverage file. For more on these files and their formats please see below.

## Sample report

**USAGE:**

```
bismark2report [options]
```

This command attempts to find Bismark alignment, deduplication and methylation extraction (splitting) reports as well as M-bias files to generate a graphical HTML report such as this [example Bismark paired-end report](http://www.bioinformatics.babraham.ac.uk/projects/bismark/PE_report.html) for each sample in a directory.

## Summary report

**USAGE:**

```
bismark2summary [options]
```

This command scans the current working directory for different Bismark alignment, deduplication and methylation extraction (splitting) reports to produce a graphical summary HTML report, as well as a data table, for all files in a directory. Here is a sample [Bismark Summary Report](http://www.bioinformatics.babraham.ac.uk/projects/bismark/bismark_summary_WGBS.html). The Bismark summary report is meant to give you a quick visual overview of the alignment statistics for a large number of samples (tens, hundreds or thousands of samples); if you only want to look at a single report please check out the `bismark2report`.

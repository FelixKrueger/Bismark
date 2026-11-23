---
title: "Genome preparation"
description: "This script needs to be run only once to prepare the genome of interest for bisulfite alignments. You need to specify a directory containing the genome…"
---

This script needs to be run only once to prepare the genome of interest for bisulfite alignments. You need to specify a directory containing the genome you want to align your reads against (please be aware that `bismark prepare` (classic alias: `bismark_genome_preparation`) expects `FastA` files in this folder (with either `.fa` or `.fasta` extension, single or multiple sequence entries per file). Bismark will create two individual folders within this directory, one for a C->T converted genome and the other one for the G->A converted genome. After creating C->T and G->A versions of the genome they will be indexed in parallel using the indexer `bowtie2-build` (or `hisat2-build`). Once both C->T and G->A genome indices have been created you do not need to use the genome preparation script again (unless you want to align against a different genome...).

Again, **please note** that Bowtie 2 and HISAT2 indexes are not compatible! To create a genome index for use with HISAT2 the option `--hisat2` needs to be included as well.

### Running `bismark prepare`

**USAGE:** `bismark prepare [options] <path_to_genome_folder>`

A typical command could look like this:

```
bismark prepare --path_to_aligner /usr/bin/bowtie2/ --verbose /data/genomes/homo_sapiens/GRCh38/
```

### bwa-mem3 methylation index (`--bwamem3`)

`bismark --taps` and `bismark --illumina_5base` (bwa-mem3 is its default engine) align with bwa-mem3, which converts the
genome itself, so they need a bwa-mem3 methylation index instead of the bisulfite genomes:

```
bismark prepare --bwamem3 --parallel 16 /data/genomes/homo_sapiens/GRCh38/
bismark --taps --genome /data/genomes/homo_sapiens/GRCh38/ -1 R1.fastq.gz -2 R2.fastq.gz
```

The index is written to `Bwamem3_Index/` in the genome folder, and the aligner uses it when
`--five_base_index` is not given. A human genome takes about 1 h and about 104 GB of RAM; if
bwa-mem3 reports that its memory budget is too small, raise it with `--bwamem3_max_memory`.

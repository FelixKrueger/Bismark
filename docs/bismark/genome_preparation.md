# Genome preparation

This script needs to be run only once to prepare the genome of interest for bisulfite alignments. You need to specify a directory containing the genome you want to align your reads against (please be aware that the `bismark_genome_preparation` script expects `FastA` files in this folder (with either `.fa` or `.fasta` extension, single or multiple sequence entries per file). Bismark will create two individual folders within this directory, one for a C->T converted genome and the other one for the G->A converted genome. After creating C->T and G->A versions of the genome they will be indexed in parallel using the indexer `bowtie2-build` (or `hisat2-build`). Once both C->T and G->A genome indices have been created you do not need to use the genome preparation script again (unless you want to align against a different genome...).

Again, **please note** that Bowtie 2 and HISAT2 indexes are not compatible! To create a genome index for use with HISAT2 the option `--hisat2` needs to be included as well.

### Running `bismark_genome_preparation`

**USAGE:** `bismark_genome_preparation [options] <path_to_genome_folder>`

A typical command could look like this:

```
bismark_genome_preparation --path_to_aligner /usr/bin/bowtie2/ --verbose /data/genomes/homo_sapiens/GRCh38/
```

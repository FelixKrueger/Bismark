---
hide:
  - navigation
---

# Bismark Bisulfite Mapper

![Bismark](images/bismark.png)

This User Guide outlines the Bismark suite of tools and gives more details for each individual step. For troubleshooting some of the more commonly experienced problems in sequencing in general and bisulfite-sequencing in particular please browse through the sequencing section at [QCFail.com](https://sequencing.qcfail.com/).

## General Information

### What is Bismark?

Bismark is a set of tools for the time-efficient analysis of Bisulfite-Seq (BS-Seq) data. Bismark performs alignments of bisulfite-treated reads to a reference genome and cytosine methylation calls at the same time. Bismark is written in Perl and is run from the command line. Bisulfite-treated reads are mapped using the short read aligner Bowtie 2, or alternatively HISAT2. Therefore, it is a requirement that Bowtie 2 (or HISAT2) are also installed on your machine (see Dependencies).

All files associated with Bismark as well as a test BS-Seq data set can be downloaded from [Github](https://github.com/FelixKrueger/Bismark).

We would like to hear your comments, suggestions or bugs about Bismark! Please e-mail them to: [fkrueger@altoslabs.com](mailto:fkrueger@altoslabs.com)

### Which kind of BS-Seq files are supported?

Bismark supports the alignment of bisulfite-treated reads (whole genome shotgun BS-Seq (WGSBS), reduced-representation BS-Seq (RRBS) or PBAT-Seq (Post-Bisulfite Adapter Tagging) for the following conditions:

- sequence format either `FastQ` or `FastA`
- single-end or paired-end reads
- input files can be uncompressed or `gzip`-compressed (ending in `.gz`)
- variable read length support
- directional or non-directional BS-Seq libraries

A full list of alignments modes can be found in [`Bismark_alignment_modes.pdf`](http://www.bioinformatics.babraham.ac.uk/projects/bismark/Bismark_alignment_modes.pdf).

In addition, Bismark retains much of the flexibility of Bowtie 2 / HISAT2 / minimap2 (adjustable seed length, number of mismatches, insert size ...). For a full list of options please run:

```
bismark --help
```

or see the Appendix at the end of this User Guide.

**NOTE:** It should be mentioned that Bismark supports only reads in base-space, such as from the Illumina platform. There are currently no plans to extend its functionality to colour-space reads from the SOLiD platform.

### How does Bismark work?

Sequence reads are first transformed into fully bisulfite-converted forward (C->T) and reverse read (G->A conversion of the forward strand) versions, before they are aligned to similarly converted versions of the genome (also C->T and G->A converted). Sequence reads that produce a unique best alignment from the four alignment processes against the bisulfite genomes (which are running in parallel) are then compared to the normal genomic sequence and the methylation state of all cytosine positions in the read is inferred. A read is considered to align uniquely if an alignment has a unique best alignment score (as reported by the `AS:i` field). If a read produces several alignments with the same number of mismatches or with the same alignment score (`AS:i` field), a read (or a read-pair) is discarded altogether.

### Bismark alignment and methylation call report

Upon completion, Bismark produces a run report containing information about the following:

- Summary of alignment parameters used
- Number of sequences analysed
- Number of sequences with a unique best alignment (mapping efficiency)
- Statistics summarising the bisulfite strand the unique best alignments came from
- Number of cytosines analysed
- Number of methylated and unmethylated cytosines
- Percentage methylation of cytosines in CpG, CHG or CHH context (where H can be either A, T or C). This percentage is calculated individually for each context following the equation:

> % methylation (context) = 100 \* methylated Cs (context) / (methylated Cs (context) + unmethylated Cs (context)).

It should be stressed that the percent methylation value (context) is just a very rough calculation performed directly at the mapping step. Actual methylation levels after post-processing or filtering have been applied may vary.

## Credits

Bismark was written by Felix Krueger at the [Babraham Bioinformatics Group](http://www.bioinformatics.babraham.ac.uk/), now at Altos Labs, [Cambridge Institute](https://altoslabs.com/).

![Babraham Bioinformatics](images/bioinformatics_logo.png)

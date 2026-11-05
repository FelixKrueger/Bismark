---
title: "Deduplication"
description: "bismark dedup removes alignments to the same genomic position and strand from Bismark mapping output — recommended for whole-genome bisulfite samples."
---

**USAGE:**

```bash
bismark dedup [options] filename(s)
```

`bismark dedup` (classic alias: `deduplicate_bismark`) removes alignments to the same position in the genome from the Bismark mapping output (both single and paired-end SAM/BAM files), which can arise by e.g. excessive PCR amplification. Sequences which align to the same genomic position but on different strands are scored individually.

**It is important to note that deduplication is not recommended for RRBS, amplicon or other target enrichment-type libraries!**

In the default mode, the first alignment to a given position will be used irrespective of its methylation call. Which copy that is depends on the order of the alignments in the input file.

### Which duplicate is kept

Perl Bismark v0.25.1 run with `--multicore` and Bowtie 2 writes the same alignments as a single-core run, but in a different order. nf-core/methylseq releases that run Perl Bismark set `--multicore` automatically. Deduplicating such a BAM keeps a different copy of many duplicate sets than a BAM from the Rust aligner, or from Perl without `--multicore`.

The number of reads kept is the same. Call counts and methylation values move slightly, as with any other choice of copy; neither choice is more correct. To compare with such a run, re-run Perl without `--multicore` and compare the records with `samtools view`, since the BAM files themselves also differ in compression and `@PG` lines. With HISAT2, Perl `--multicore` also changes which reads align.

For **single-end** alignments, only the chromosome, start coordinate and strand of a read will be used for deduplication.

For **paired-end** alignments, the chromosome, the strand of a read pair, the start-coordinate of the first read as well as the start coordinate of the second read will be used for deduplication. This script expects the Bismark output to be in SAM/BAM format.

**Please note that for paired-end BAM files the deduplication script expects Read 1 and Read 2 to follow each other in consecutive lines!** If the file has been sorted by position for whatever reason, please make sure that you resort it by read name first (e.g. using `samtools sort -n`)

### Deduplication using UMIs or barcodes

In addition to chromosome, start (and end position for paired-end libraries) position and strand orientation the option `--barcode` will also take a potential barcodes or UMIs (unique molecular identifiers) into consideration while deduplicating. The barcode needs to be the last element of the read ID and has to beseparated by a `:`, e.g.:

```
MISEQ:14:000000000-A55D0:1:1101:18024:2858_1:N:0:CTCCT
```

This option is equivalent to using [UmiBam](https://github.com/FelixKrueger/Umi-Grinder) in the following mode:
`UmiBam --umi input.bam`, however UmiBam has additional functionality such as a double UMI feature or the option to allow mismatches in the UMI(s).

### Deduplication of multiple files of the same library

When using the option `--multiple`, all specified input files are treated as a **single** sample and concatenated together for deduplication. This uses Unix `cat` for SAM files and `samtools cat` for BAM files.

Additional notes for BAM files: Although this works on either BAM or CRAM, all input files must be the same format as each other. The sequence dictionary of each input file must be identical, although this command does not check this. By default the header is taken from the first file to be concatenated.

# Deduplication

**USAGE:**

```bash
./deduplicate_bismark [options] filename(s)
```

The script `deduplicate_bismark` is supposed to remove alignments to the same position in the genome from the Bismark mapping output (both single and paired-end SAM/BAM files), which can arise by e.g. excessive PCR amplification. Sequences which align to the same genomic position but on different strands are scored individually.

**It is important to note that deduplication is not recommended for RRBS, amplicon or other target enrichment-type libraries!**

In the default mode, the first alignment to a given position will be used irrespective of its methylation call. As the alignments are not ordered in any way this is near enough a random read for each position.

For **single-end** alignments, only the chromosome, start coordinate and strand of a read will be used for deduplication.

For **paired-end** alignments, the chromosome, the strand of a read pair, the start-coordinate of the first read as well as the start coordinate of the second read will be used for deduplication. This script expects the Bismark output to be in SAM/BAM format.

**Please note that for paired-end BAM files the deduplication script expects Read 1 and Read 2 to follow each other in consecutive lines!** If the file has been sorted by position for whatever reason, please make sure that you resort it by read name first (e.g. using `samtools sort -n`)

### Deduplication using UMIs or barcodes

In addition to chromosome, start (and end position for paired-end libraries) position and strand orientation the option `--barcode` will also take a potential barcodes or UMIs (unique molecular identifiers) into consideration while deduplicating. The barcode needs to be the last element of the read ID and has to beseparated by a `:`, e.g.:

```
MISEQ:14:000000000-A55D0:1:1101:18024:2858_1:N:0:CTCCT
```

This option option is equivalent to using [UmiBam](https://github.com/FelixKrueger/Umi-Grinder) in the following mode:
`UmiBam --umi input.bam`, however UmiBam has additional functionality such as a double UMI feature or the option to allow mismatches in the UMI(s).

### Deduplication of multiple files of the same library

When using the option `--multiple`, all specified input files are treated as a **single** sample and concatenated together for deduplication. This uses Unix `cat` for SAM files and `samtools cat` for BAM files.

Additional notes for BAM files: Although this works on either BAM or CRAM, all input files must be the same format as each other. The sequence dictionary of each input file must be identical, although this command does not check this. By default the header is taken from the first file to be concatenated.

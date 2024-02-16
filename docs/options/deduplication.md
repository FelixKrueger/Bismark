## Appendix (III): Bismark Deduplication

This script is supposed to remove alignments to the same position in the genome from the Bismark mapping output (both single and paired-end SAM/BAM files), which can arise by e.g. excessive PCR amplification. If sequences align to the same genomic position but on different strands they will be scored individually.

!!! important

    Please note that for paired-end BAM files the deduplication script expects Read1 and Read2 to follow each other in consecutive lines! If the file has been sorted by position make sure that you resort it by read name first (e.g. using `samtools sort -n`)

A brief description of the Bismark deduplication and a full list of options can also be viewed by typing `deduplicate_bismark --help`.

#### USAGE: `deduplicate_bismark [options] <filename(s)>`

#### ARGUMENTS:

- `<filenames>`

A space-separated list of Bismark result files in BAM/SAM format.


#### OPTIONS:

- `-s/--single-end`

  Deduplicate single-end BAM/SAM Bismark files. Default: [AUTO-DETECT]

- `-p/--paired-end`

  Deduplicate paired-end BAM/SAM Bismark files. Default: [AUTO-DETECT]

- `-o/--outfile [filename]`

The basename of a desired output file. This basename is modified to end into `.deduplicated.bam`, or `.multiple.deduplicated.bam` in `--multiple` mode, for consistency reasons.

- `--output_dir [path]`

Output directory, either relative or absolute. Output is written to the current directory if not specified explicitly.

- `--barcode`

In addition to chromosome, start position and orientation this will also take a potential barcode into consideration while deduplicating. The barcode needs to be the last element of the read ID and separated by a ':', e.g.: MISEQ:14:000000000-A55D0:1:1101:18024:2858_1:N:0:CTCCT

- `--bam`

The output will be written out in BAM format. This script will attempt to use the path to Samtools that was specified with `--samtools_path`, or, if it hasn't been specified,attempt to find Samtools in the `PATH`. If no installation of Samtools can be found, a GZIP compressed output is written out instead (yielding a `.sam.gz` output file). Default: ON.

- `--sam`

The output will be written out in SAM format. Default: OFF.

- `--multiple`

All specified input files are treated as one sample and concatenated together for deduplication. This uses Unix `cat` for SAM files and `samtools cat` for BAM files. Additional notes for BAM files:	Although this works on either BAM or CRAM, all input files must be the same format as each other. The sequence dictionary of each input file must be identical, although this command does not check this. By default the header is taken from the first file to be concatenated.

- `--samtools_path [path]`

The path to your Samtools installation, e.g. `/home/user/samtools/`. Does not need to be specified explicitly if Samtools is in the `PATH` already

- `--version`

Print version information and exit


#### OUTPUT:

The output is a BAM format by default, as well as a deduplication report (ending in '_deduplication_report.txt') 


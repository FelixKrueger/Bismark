# Filtering out non-bisulfite converted reads

Filtering incomplete bisulfite conversion from Bismark BAM files (optional). This script examines the methylation calls of reads, or read pairs for paired-end sequencing, and filters out reads that exceed a certain threshold of methylated calls in non-CG context (the default is 3). By default, `filter_non_conversion` looks for a certain number of methylated non-CG calls, but a percentage methylation cutoff may be specified alternatively.

**Please Note**: Be aware that this kind of filtering is not advisable - and _will_ introduce biases - if you work with organisms which exhibit any appreciable levels of non-CG methylation (e.g. most plants).

Writes out a file called _nonCG_filtered.bam_, also a file called _nonCG_removed_seqs.bam_ as well as a short report how many sequences have been analysed and removed.

**USAGE:**

```
filter_non_conversion [options] [Bismark BAM files]
```

**Please also note** that for paired-end BAM files `filter_non_conversion` expects Read 1 and Read 2 to follow each other in consecutive lines! If the file has been sorted by position make sure that you resort it by read name first (e.g. using `samtools sort -n`)

- `-s/--single`

Deduplicate single-end Bismark BAM files. If not specified the library type is auto-detected.

- `-p/--paired`

Deduplicate paired-end Bismark BAM files. If not specified the library type is auto-detected.

- `--threshold [int]`

The number of methylated cytosines in non-CG context at which reads or read pairs are filtered out. For paired-end files either Read 1 or Read 2 can fail the entire read pair. [Default: 3].

- `--percentage_cutoff [int]`

Instead of filtering on an absolute count of methylated cytosines in non-CG context (see `--threshold [int]`) this option allows you to define an overall percentage of methylation in non-CG context (both CHH and CHG) which, if reached or exceeded, results in the read or read pair being filtered out. For paired-end files either Read 1 or Read 2 can fail the entire read pair. Also requires a minimum number of cytosines in non-CG context to make confident filtering choices (see `--minimum_count [int]`).

- `--minimum_count [int]`

At least this number of cytosines in non-CG context (CHH or CHG) have to be seen in a read (irrespective of their methylation state) before the `--percentage_cutoff` filter kicks in. [Default: 5].

- `--consecutive`

Non-CG methylation has to be found on consecutive non-CGs. Any kind of unmethylated cytosine (in any context) resets the methylated non-CG counter to 0. [Default: OFF].

- `--samtools_path`

The path to your Samtools installation, e.g. /home/user/samtools/. Does not need to be specified explicitly if Samtools is in the PATH already.

- `--help`

Displays this help text end exits.

- `--version`

Displays version information and exits.

If you get stuck at any point or have any questions or comments please contact me via e-mail: [fkrueger@altoslabs.com](mailto:fkrueger@altoslabs.com)

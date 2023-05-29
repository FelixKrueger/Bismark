## Appendix (III): Bismark Methylation Extractor

A brief description of the Bismark methylation extractor and a full list of options can also be viewed by typing `bismark_methylation_extractor --help`

#### USAGE: `bismark_methylation_extractor [options] <filenames>`

#### ARGUMENTS:

- `<filenames>`

A space-separated list of Bismark result files in SAM format from which methylation information is extracted for every cytosine in the reads. For alignment files in the older custom Bismark output see option `--vanilla`.

#### OPTIONS:

- `-s/--single-end`

  Input file(s) are Bismark result file(s) generated from single-end read data. If neither `-s` nor `-p` is set the type of experiment will be determined automatically.

- `-p/--paired-end`

  Input file(s) are Bismark result file(s) generated from paired-end read data. If neither `-s` nor `-p` is set the type of experiment will be determined automatically.

- `--vanilla`

  The Bismark result input file(s) are in the old custom Bismark format (up to version 0.5.x) and not in SAM format which is the default as of Bismark version 0.6.x or higher. Default: OFF.

- `--no_overlap`

  For paired-end reads it is theoretically possible that Read 1 and Read 2 overlap. This option avoids scoring overlapping methylation calls twice (only methylation calls of read 1 are used for in the process since read 1 has historically higher quality basecalls than read 2). Whilst this option removes a bias towards more methylation calls in the center of sequenced fragments it may _de facto_ remove a sizeable proportion of the data. This option is on by default for paired-end data but can be disabled using `--include_overlap`. Default: ON.

- `--include_overlap`

  For paired-end data all methylation calls will be extracted irrespective of whether they overlap or not. Default: OFF.

- `--ignore <int>`

  Ignore the first &lt;int> bp from the 5' end of Read 1 (or single-end alignment files) when processing the methylation call string. This can remove e.g. a restriction enzyme site at the start of each read or any other source of bias (such as PBAT-Seq data).

- `--ignore_r2 <int>`

  Ignore the first &lt;int> bp from the 5' end of Read 2 of paired-end sequencing results only. Since the first couple of bases in Read 2 of BS-Seq experiments show a severe bias towards non-methylation as a result of end-repairing sonicated fragments with unmethylated cytosines (see M-bias plot), it is recommended that the first couple of bp of Read 2 are removed before starting downstream analysis. Please see the section on M-bias plots in the Bismark User Guide for more details.

- `--ignore_3prime <int>`

  Ignore the last &lt;int> bp from the 3' end of Read 1 (or single-end alignment files) when processing the methylation call string. This can remove unwanted biases from the end of reads.

- `--ignore_3prime_r2 <int>`

  Ignore the last &lt;int> bp from the 3' end of Read 2 of paired-end sequencing results only. This can remove unwanted biases from the end of reads.

- `--comprehensive`

  Specifying this option will merge all four possible strand-specific methylation info into context-dependent output files. The default contexts are:

  - CpG context
  - CHG context
  - CHH context

- `--merge_non_CpG`

  This will produce two output files (in `--comprehensive mode`) or eight strand-specific output files (default) for Cs in

  - CpG context
  - non-CpG context

- `--report`

  Prints out a short methylation summary as well as the parameters used to run this script. Default: ON.

- `--no_header`

  Suppresses the Bismark version header line in all output files for more convenient batch processing.

- `-o/--output DIR`

  Allows specification of a different output directory (absolute or relative path). If not specified explicitly, the output will be written to the current directory.

- `--samtools_path`

  The path to your Samtools installation, e.g. /home/user/samtools/. Does not need to be specified explicitly if Samtools is in the PATH already.

- `--gzip`

  The methylation extractor files (CpG_OT\_..., CpG_OB\_... etc) will be written out in a `GZIP` compressed form to save disk space. This option is also passed on to the genome-wide cytosine report. `bedGraph` and `coverage` files are written out as `.gz` by default.

- `--mbias_only`

  The methylation extractor will read the entire file but only output the M-bias table and plots as well as a report (optional) and then quit. Default: OFF.

- `--mbias_off`

  The methylation extractor will process the entire file as usual but doesn't write out any M-bias report. Only recommended for users who deliberately want to keep an earlier version of the M-bias report. Default: OFF.

- `--multicore <int>`

  Sets the number of cores to be used for the methylation extraction process. If system resources are plentiful this is a viable option to speed up the extraction process (we observed a near linear speed increase for up to 10 cores used). Please note that a typical process of extracting a BAM file and writing out `.gz` output streams will in fact use ~3 cores per value of `--multicore <int>` specified (1 for the methylation extractor itself, 1 for a Samtools stream, 1 for a GZIP stream), so `--multicore 10` is likely to use around 30 cores of system resources. This option has no bearing on the `bismark2bedGraph` or `coverage2cytosine` (genome-wide cytosine report) processes.

- `--version`

  Displays version information.

- `-h/--help`

  Displays this help file and exits.

##### bedGraph specific options:

- `--bedGraph`

  After finishing the methylation extraction, the methylation output is written into a sorted `bedGraph` file that reports the position of a given cytosine and its methylation state (in %, see details below) using 0-based genomic start and 1-based end coordinates. The methylation extractor output is temporarily split up into temporary files, one per chromosome (written into the current directory or folder specified with `-o/--output`); these temp files are then used for sorting and deleted afterwards. By default, only cytosines in CpG context are sorted. The option `--CX_context` may be used to report all cytosines irrespective of sequence context (this will take _MUCH_ longer!). The `bedGraph` conversion step is performed by the external module `bismark2bedGraph`; this script needs to reside in the same folder as the bismark_methylation_extractor itself.

- `--zero_based`

  Write out an additional coverage file (ending in `.zero.cov`) that uses 0-based genomic start and 1-based genomic end coordinates (zero-based, half-open), like used in the `bedGraph` file, instead of using 1-based coordinates throughout. Default: OFF.

- `--cutoff [threshold]`

The minimum number of times any methylation state (methylated or unmethylated) has to be seen for a nucleotide before its methylation percentage is reported. Default: 1.

- `--remove_spaces`

  Replaces white spaces in the sequence ID field with underscores to allow sorting.

- `--CX/--CX_context`

  The sorted `bedGraph` output file contains information on every single cytosine that was covered in the experiment irrespective of its sequence context. This applies to both forward and reverse strands. Please be aware that this option may generate large temporary and output files and may take a long time to sort (up to many hours). Default: OFF (i.e. Default = CpG context only).

- `--buffer_size <string>`

  This allows you to specify the main memory sort buffer when sorting the methylation information. Either specify a percentage of physical memory by appending % (e.g. `--buffer_size 50%`) or a multiple of 1024 bytes, e.g. `K` multiplies by 1024, `M` by 1048576 and so on for `T` etc. (e.g. `--buffer_size 20G`). For more information on sort type `info sort` on a command line. Defaults to 2G.

- `--scaffolds/--gazillion`

  Users working with unfinished genomes sporting tens or even hundreds of thousands of scaffolds/contigs/chromosomes frequently encountered errors with pre-sorting reads to individual chromosome files. These errors were caused by the operating system's limit of the number of filehandles that can be written to at any one time (typically 1024; to find out this limit on Linux, type: `ulimit -a`). To bypass the limitation of open filehandles, the option `--scaffolds` does not pre-sort methylation calls into individual chromosome files. Instead, all input files are temporarily merged into a single file (unless there is only a single file), and this file will then be sorted by both chromosome AND position using the Unix sort command. Please be aware that this option might take a l*ooooo*ng time to complete, depending on the size of the input files, and the memory you allocate to this process (see `--buffer_size`). Nevertheless, it seems to be working.

- `--ample_memory`

  Using this option will not sort chromosomal positions using the UNIX `sort` command, but will instead use two arrays to sort methylated and unmethylated calls. This may result in a faster sorting process of very large files, but this comes at the cost of a larger memory footprint (two arrays of the length of the largest human chromosome 1 (~250M bp) consume around 16GB of RAM). Due to overheads in creating and looping through these arrays it seems that it will actually be _slower_ for small files (few million alignments), and we are currently testing at which point it is advisable to use this option. Note that `--ample_memory` is not compatible with options `--scaffolds/--gazillion` (as it requires pre-sorted files to begin with).

##### Genome-wide cytosine methylation report specific options:

- `--cytosine_report`

  After the conversion to bedGraph has completed, the option `--cytosine_report` produces a genome-wide methylation report for all cytosines in the genome. By default, the output uses 1-based chromosome coordinates (zero-based start coords are optional) and reports CpG context only (all cytosine context is optional). The output considers all Cs on both forward and reverse strands and reports their position, strand, trinucleotide content and methylation state (counts are 0 if not covered). The cytosine report conversion step is performed by the external module `coverage2cytosine`; this script needs to reside in the same folder as the bismark_methylation_extractor itself.

- `--CX/--CX_context`

  The output file contains information on every single cytosine in the genome irrespective of its context. This applies to both forward and reverse strands. Please be aware that this will generate output files with > 1.1 billion lines for a mammalian genome such as human or mouse. Default: OFF (i.e. Default = CpG context only).

- `--zero_based`

  Uses 0-based genomic coordinates instead of 1-based coordinates. Default: OFF.

- `--genome_folder <path>`

  Enter the genome folder you wish to use to extract sequences from (full path only). Accepted formats are FastA files ending with `.fa` or `.fasta`. Specifying a genome folder path is mandatory.

- `--split_by_chromosome`

  Writes the output into individual files for each chromosome instead of a single output file. Files are named to include the input filename as well as the chromosome number.

#### OUTPUT

##### The bismark_methylation_extractor output is in the form (tab delimited, 1-based coords):

    <seq-ID> <methylation state*> <chromosome> <start position (= end position)> <methylation call>

      Methylated cytosines receive a '+' orientation,
    Unmethylated cytosines receive a '-' orientation.

##### The bedGraph output (optional) looks like this (tab-delimited, 0-based start, 1-based end coords):

    track type=bedGraph (header line)
    <chromosome> <start position> <end position> <methylation percentage>

##### The coverage output looks like this (tab-delimited; 1-based genomic coords):

    <chromosome> <start position> <end position> <methylation percentage> <count methylated> <count unmethylated>

##### The genome-wide cytosine report (optional) is tab-delimited in the following format (1-based coords):

    <chromosome> <position> <strand> <count methylated> <count unmethylated> <C-context> <trinucleotide context>

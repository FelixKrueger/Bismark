# Bismark Changelog

## Changelog for Bismark v0.24.2 (release on 27 Sep 2023) 

### Bismark

- removed an `exit 0` that would terminate runs after processing a single (set of) input file(s).

### deduplicate_bismark

- Changed the path to Samtools to custom variable ([#609](https://github.com/FelixKrueger/Bismark/issues/609))

### coverage2cytosine

- set threshold reads to 1 (if it was 0) for `--gc_context` as intended and mentioned in the help text. Fixes [#621](https://github.com/FelixKrueger/Bismark/issues/621)


Added scripts for merging coverage files (e.g. for when R1 and R2 had been run in single-end mode) 


## Changelog for Bismark v0.24.1 (release on 29 May 2023)

- Added new [documentation website](http://felixkrueger.github.io/Bismark/), built using [Material for Mkdocs](https://squidfunk.github.io/mkdocs-material/). Thanks to @ewels for a great (late-night) effort to break up and restructure what had become a fairly unwieldy monolithic beast

- Added documentation for cytosine context summary, useful for `GpC` methylation or filtering for specific C context (e.g. `CpA`)

- Updated docs for the dovetailing 


### Bismark
- Warning messages for closing ambiguous and unmapped filehandles only occur when these options were specified [see here](https://github.com/FelixKrueger/Bismark/commit/1e86f6473415b5f95ba686f93efab7aa70c7bb86)




## Changelog for Bismark v0.24.0 (Release on 07 October 2022)

### Bismark

- Added new option `--strandID` which reports the alignment strand identity for paired-end, non-directional libraries, e.g. `YS:Z:CTOT`. This information may be difficult to obtain if third party tools interfered with the read ordering (admittedly there is a fine balance of read reporting position, FLAG, Read 1 and Genome conversion state to make it work in the first place. More information can be found in [this thread](https://github.com/FelixKrueger/Bismark/issues/455)).

- runs with `--parallel` / `--multicore` > 1 specified will now terminate with an error message whenever one of the child processes fails. This prevents potentially incomplete result files making it through to the end unnoticed (more [#494](https://github.com/FelixKrueger/Bismark/issues/494))

- runs with `--parallel` / `--multicore` > 1 as well as `--unmapped` and/or `--ambiguous` specified will no longer produce potentially corrupt FastQ files (more [#495](https://github.com/FelixKrueger/Bismark/issues/495))

- Added option `--mm2/--minimap2` to use [minimap2](https://github.com/lh3/minimap2) as the underlying aligner. The minimap2 alignment modes include Oxford Nanopore, PacBio and accurate short reads. In its current implementation, minimap2 can be invoked in one of the following ways:

- `--mm2_nanopore`: Sets preset settings for Oxford Nanopore vs reference mapping '-x map-ont' [default]

- `--mm2_pacbio`: Sets preset settings for PacBio vs. reference mapping '-x map-pb'

- `--mm2_short_reads`: Sets preset settings for accurate short reads '-x sr'

- added option `--mm2_maximum_length <int>` to set a maximum length cutoff, which might be required for very long reads exceeding the maximum number of CIGAR operations tolerated by the BAM formatted reads (>65535). The default is 10,000 bp.

Other options that are currently set within Bismark include '-a' (SAM output), '--MD' (MD tag), '--secondary=no'.

Prompted by fairly slow alignment speeds with the minimap2 default settings, we set out to improve the performance of the alignment process by tweaking several different parameters

Speed optimisiation: [optimisation of minimap2 parameters](https://github.com/FelixKrueger/Bismark/issues/446)

k-mer size
Due to the reduced DNA alphabet the minimap2 default k-mer size of 15 leads to substantially higher alignment times. Based on our tests we settled for a new default of ‘-k 20’
minibatch size
The minimap2 default minibatch size of 500 million bp means that a substantial amount of data is aligned and held in memory before additional alignment threads can be started. Reducing the minibatch size to 250K reads seemed to be a good compromise (‘-K 250K’).
minimap2 multi-threading
minimap2 alignments may utilize multiple cores for each alignment process; we found that ‘-t 2’ offered a good speed-up, while allowing additional resources had diminishing returns.
Bismark multi-threading
We also tested the potential of using additional resources for Bismark itself (--parallel), which appeared to result in a speed-up of the alignment process as expected; however this comes at the cost of requiring additional system resources.

As a result of these tests, we changed the default settings for minimap2 alignment parameters to ‘-t 2 -k 20 -K 250K’.

### methylation_consistency

- Added new option `--chh` to use cytosines in CHH instead of CpG context to enable some trouble shooting and method development

### bismark2report

- The CHH/CHG labels for the Cytosine Methylation after Extraction plot now appear in the correct order

### bismark_methylation_extractor

- removed a print statement that would flood STDOUT the logfile if `--merge_non_CG` (but not `--comprehensive`) had been selected

- runs with `--parallel` / `--multicore` specified will now terminate with an error message whenever one of the child processes fails. This prevents potentially incomplete result files making it through to the end unnoticed

- changed the option `-o/--output` to `-o/--output_dir` for consistency reasons...

### bismark_genome_preparation

- Added option `--mm2/--minimap2`. The genome indexing process (`bismark_genome_preparation`) writes out a minimap2 index to the genome folder, using the optimized k-mer size of `-k 20` (see comments for bismark itself). This pre-generated minimap2 index takes precedence over indexing options that would otherwise happen as part of the alignment procedure.

### deduplicate_bismark

- when using an output filename `-o customname` the deduplication report will also be derived from customname.

Added a sentence to the Docs that Genozip 14 and above supports Bismark BAM files (with a substantial gain in compression).

## Changelog for Bismark v0.23.1 (release on 26 07 2021)

### filter_non_conversion

- fixed global setting of `--paired` or `--single` mode. Auto-detection now works by only looking at the `@PG ID:Bismark` line of the SAM header.

### methylation_consistency

- Auto-detection now works by only looking at the `@PG ID:Bismark` line of the SAM header.

### coverage2cytosine

- Swapped the columns for count methylated and count unmethylated for the context summary report to match the header line.

## Changelog for Bismark v0.23.0 (release on 09 11 2020)

- Migrated CI tests from Travis to Github Actions

### deduplicate_bismark

- the command `deduplicate_bismark --barcode *bam` now works again. Previously the output file names were accidentally all derived from the first supplied file in `--barcode` (= UMI) mode (it had been fixed for normal files in 0.22.2).

- Changed the way the library auto-detection works by only looking at the `@PG ID:Bismark` line of the SAM header.

### bismark_methylation_extractor / bismark2bedGraph

- Added a new option `--ucsc` to `bismark2bedGraph` and `bismark_methylation_extractor` that will produce a UCSC-ready bedGraph file if the genome version used came from Ensembl. This option (i) prefixes chromosome names with 'chr', and (ii) changes the mitochondrial chromosome from 'MT' to 'chrM'. In addition, it will also write out a new file ending in `.chromosome_sizes.txt` for easier use of [`bedGraphToBigWig`](https://www.encodeproject.org/software/bedgraphtobigwig/). [More here](https://github.com/FelixKrueger/Bismark/issues/371).

- Changed the way the library auto-detection works to only look at the `@PG ID:Bismark` line of the SAM header.

### coverage2cytosine

- Added a new output file for all cytosine context methylation totals. More information here: [#321](https://github.com/FelixKrueger/Bismark/issues/382).

- Added new option `--drach/--m6A`. Most `m6A` sites are found in the conserved sequence motif `DRACH` (where `D`=`G`/`A`/`U`, `R`=`G`/`A`, `H`=`A`/`U`/`C`), and if bound by anti-m6A antibody, it causes the reverse transcriptase to introduce `C` to `T` transitions at the cytosine which follows `A` in the `DRACH` motif. This option also sets a coverage threshold of at 1 unless specified explicitly. This is a very specialised option and should only be used by experimentalists looking at `m6A` methylation (where the C to T transition acts as a proxy of `m6A`).

### bismark2summary

- Samples with absolutely 0 methylation calls in some context are now excluded from the graphical HTML output (as they break rendering the entire summary graph section). These samples and their statistics do still appear in the file `bismark_summary_report.txt`. More information here: [#315](https://github.com/FelixKrueger/Bismark/issues/315).

## Changelog for Bismark v0.22.3 (release on 19 Nov 2019)

### Bismark

- Accepted pull request to fix the MAPQ score calculation in `local` mode.

### methylation_consistency

- Added a new script to assess the concordance of methylation calls. See more here: https://github.com/FelixKrueger/Bismark/tree/master/Docs#x-concordance-of-methylation-calls-across-bisulfite-reads

## Changelog for Bismark v0.22.2 (release on 16 10 2019)

Added [FAQ](docs/faq/README.md) document for questions that keep coming up.

### Bismark

- the option `--non_bs_mm` is now only allowed in end-to-end mode

- Fixed the calculation of non bisulfite mismatches for paired-end data which happened correctly only when R2 had an InDel (see [here](https://github.com/FelixKrueger/Bismark/issues/262))

- When the option `-u` was used in conjunction with `--parallel`, only `-u` sequences will be written to the temporary subset files for each spawn of Bismark (previously, the entire file was split for `--parallel`, but then only a small subset of those files was used for `-u`, which resulted in very long runs even for a small number of analysed sequences)

### deduplicate_bismark

- the command `deduplicate_bismark *bam` now works again. Previously the output file names were accidentally all derived from the first supplied file.

### coverage2cytosine

- Added new option `--coverage_threshold INT`. Positions have to be covered by at least INT calls (irrespective of their methylation state) before they get reported. For NOMe-seq, the minimum threshold is automatically set to 1 unless specified explicitly. Setting a coverage threshold does not work in conjunction with `--merge_CpGs` (as all genomix CpGs are required for this). Default: 0 (i.e. all genomic positions get reported)

### bismark2report

- added seconds to the timestamp report statement (which caused a warning on certain, but not all, platforms)

### bismark2summary

- Now reads splitting reports even for non-deduplicated files (such as RRBS).

## Changelog for Bismark v0.22.1 (release on 21 04 2019)

### Bismark

- Hotfixed (as in: removed) the cause of delay during the MD:Z: field computation for reads containing a deletion (which was roughly one second per read). Apologies, I did it again...

- Changed the see default `--score_min` function for HISAT2 back to a linear model (instead of using the logarithmic model that is employed by Bowtie 2). The default is now `--score_min L,0,-0.2` for both end-to-end (default) and `--local` mode. It should be mentioned that we currently don't understand how exactly the scoring mode in HISAT2 works, so this might change somewhat in the future. See [here for more info](https://github.com/DaehwanKimLab/hisat2/issues/202).

## Changelog for Bismark v0.22.0 (release on 16 04 2019)

Expanding on our observation that single-cell BS-seq, or PBAT libraries in general, can [generate chimeric read pairs](https://sequencing.qcfail.com/articles/pbat-libraries-may-generate-chimaeric-read-pairs/), a recent publication by [Wu et al.](https://www.ncbi.nlm.nih.gov/pubmed/30859188) described in further detail that intra-fragment chimeras can hinder the efficient alignment of single-cell BS-seq libraries. In there, the authors described a pipeline that uses paired-end alignments first, followed by a second, single-end alignment step that uses local alignments in a bid to improve the mapping of intra-molecular chimeras. To allow this type of improvement for single-cell or PBAT libraries, we have been experimenting with allowing local alignments.

### Bismark

- Added support for local alignments by introducing the new option `--local`. This means that the CIGAR operation `S` (soft-clipping) is now supported

- fixed typo in option `--path_to_bowtie2` (a single missing `2` was preventing the specified path to be accepted)

- fixed type in option `--no-spliced-alignment` in HISAT2 mode

- fixed missing end-of-line character for unmapped or ambiguous FastQ sequences in paired-end FastQ mode

- fixed output file naming in `--hisat2` and `--parallel` mode (\_hisat2 was missing in `--parallel` mode). Thanks to @phue for spotting this.

### bismark_genome_preparation

- Added option `--large-index` to force the generation of LARGE genome indexes. This may be required for indexing extremely large genomes (e.g. the Axolotl (32 GigaBases)) in `--parallel` mode. For more information on why the indexing was failing previously see [here](https://github.com/FelixKrueger/Bismark/issues/251)

### bismark_methylation_extractor

- Now supporting reads containing soft-clipped bases (CIGAR operation S)

### bam2nuc

- Now supporting reads containing soft-clipped bases (CIGAR operation S)

### deduplicate_bismark

- Now supporting reads containing soft-clipped bases (CIGAR operation S)

## Changelog for Bismark v0.21.0

For the upcoming version Bismark has undergone some substantial changes, which sometimes affect more than one module within the Bismark suite. Here is a short description of the major changes:

#### Bowtie 1 support

- `Bowtie (1)` support, and all of its options, has been completely dropped from `bismark_genome_preparation` and `bismark`. This decision was not made lightly, but it seems no one is using the original Bowtie short read aligner anymore, even short reads have moved on...
- Consequently, the option `--vanilla` and its handling has been removed from a number of modules (`bismark_genome_preparation`, `bismark`, `bismark_methylation_extractor` and `deduplicate_bismark`). Too bad, I liked that name...

#### HISAT2 support

- Instead, the DNA and RNA aligner [HISAT2](https://ccb.jhu.edu/software/hisat2/index.shtml) has been added as a new choice of aligner. The reason for this is not necessarily that RNA methylation is now a thing, but certain alignment modes (see below) do require splice-aware mapping if we don't want to miss out on a whole class of (spliced) alignments. Bowtie 2 is the default mode, HISAT2 alignments can be enabled with the option `--hisat2`

- Similar to the Bowtie2 mode, alignments with HISAT2 are restricted to global (end-to-end) alignments, i.e. soft-clipping is disabled. Furthermore, in paired-end mode, the options `--no-mixed` and `--no-discordant` are permanently enabled, meaning that only properly aligned read pairs are put out.

- As the `--hisat2` mode supports spliced alignments, the new `CIGAR` operation `N` is now supported in all Bismark modules (this includes `bismark_genome_preparation`, `bismark`, `bismark_methylation_extractor`, `deduplicate_bismark` and some others).

At the time of writing this, the `--hisat2` mode appears to be working as expected. It should be mentioned however that we have not done a lot of testing of these new files, so comments and feedback are welcome.

#### SLAM-seq mode

We also added a new, experimental and completely different type of alignment for SLAM-seq type data (option `--slam`). This fairly recent method to interrogate newly synthesized messenger RNA is akin to bisulfite conversion, in that newly synthesized RNA may contain T to C conversions following an alkylation reaction ([original publication](http://science.sciencemag.org/content/360/6390/800) and https://www.nature.com/articles/nmeth.4435). The new Bismark alignment mode `--slam` performs T>C conversions of both the genome (in the genome preparation step) and the subsequent alignment steps (Bismark alignment step). Currently, the rest of the processing of SLAM-seq data hijacks the standard methylation pipeline:

- T>C conversions are written out as `methylation events` in CpG context, while T-T matches are scored as `unmethylated events` in CpG context. Other cytosine contexts are not being used.

So in a nut-shell: methylation calls in `--slam` mode are either Ts (unmethylated calls = matches at T positions), or T to C mismatches (methylated calls = C mismatches at T positions).

It should be noted that this is currently an **experimental workflow**. One might argue that T/C conversion aware (or T/C mis-mapping agnostic) mapping is currently not necessary for [SLAM-seq](https://www.nature.com/articles/nmeth.4435), [NASC-Seq](https://www.biorxiv.org/content/10.1101/498667v1.article-info), or [scSLAM-seq](https://www.biorxiv.org/content/10.1101/486852v1) data as the labeling reaction is very inefficient (1 in only 50 to 200 newly incorporated Ts is a 4sU, which may get alkylated). This might be true - for now. If and when the conversion reaction improves over time, C/T agnostic mapping, similar to bisulfite-Seq data, might very well become necessary.

- Added documentation for NOMe-seq or scNMT-seq processing.

### bismark

- Dropped support for Bowtie

- Removed all traces of `--vanilla`

- Added support for HISAT2 with option `--hisat2`.

- Added HISAT2 option `--no-spliced-aligments` to disable spliced alignments altogether

- Added HISAT2 option `--known-splicesite-infile <path>` to provide a list of known splice sites.

- Added option `--slam` to allow T/C mismatch agnostic mapping (3-letter alignment). More [here](https://github.com/FelixKrueger/Bismark/blob/master/CHANGELOG.md#slam-seq-mode).

- Added a new option `--icpc` to truncate read IDs at the first space (or tab) it encounters in the (FastQ) read ID, which are sometimes used to add comments to a FastQ entry (instead of replacing them with underscores which is the default behaviour).

### bismark_genome_preparation

- Dropped support for Bowtie

- Added support for HISAT2 with option `--hisat2`.

- Added option `--slam`. Instead of performing an in-silico bisulfite conversion, this mode transforms T to C (forward strand), or A to G (reverse strand). The folder structure and rest of the indexing process is currently exactly the same as for bisulfite sequences, but this might change at some point. This means that a genome prepared in `--slam` mode is currently indistinguishable from a true Bisulfite Genome (until the alignments are in) so please make sure you name the genome folder appropriately to avoid confusion.

### deduplicate_bismark

- Removed all traces of `--vanilla`

- `--bam` mode is now the default. Uncompressed SAM output may still be obtained using the new option `--sam`

- Added new option `-o/--outfile <basename>`. This basename is then modified to remove file endings such as `.bam`, `.sam`, `.txt` or `.gz`, and `.deduplicated.bam`, or `.multiple.deduplicated.bam` in `--multiple` mode, is then appended for consistency reasons.

* Added support for new CIGAR operation `N`

### bismark_methylation_extractor

- Added support for new CIGAR operation `N` for all extraction modes

- Removed all traces of `--vanilla`

### bismark2summary/bismark2report

- Adapted to work with Bismark HISAT2 reports instead of Bowtie 1 reports.

### bam2nuc

- Reads containing spliced reads are now also skipped when determining the genomic base composition (as are reads with InDels).

## Changelog for Bismark v0.20.1

### bismark

- Added check to prevent users from inadvertently specifying the very same file as both R1 and R2

- Added a check for file truncation, or more generally the same number of reads between R1 and R2 for paired-end FastQ files (directional, non-directional and PBAT mode.

- Added Travis CI testing for most Bismark modules and commands. This should help spotting problems a early, e.g. if I release a new version right before the Christmas holidays ...

- Changed error message for failed `fork` command in `--parallel` mode to `[FATAL ERROR:] ...` to alert users that something isn't working as intended.

### bismark_genome_preparation

- Added multi-threading to the Bowtie2-based genome preparation (thanks to Rahul Karnik)

- Added test to see whether specified files exist, or die otherwise

### bismark2summary

- Fixed division by zero errors when a C-context was not covered by _any_ reads. This will now use values of `0/0` for the context plots, which looks a bit odd, but at least it still works.

- Detects if (non-deduplicated) RRBS and WGBS samples are mixed together, and bails with a meaningful error message.

### bam2nuc

- Changed `samtools` to `$samtools_path` during single-end/paired-end file testing.

### bismark_methylation_extractor

- Changed the order in which `--ample_mem` and `--buffer_size` are checked.

## Changelog for Bismark v0.20.0

### bismark_methylation_extractor

- The methylation extractor now creates output directories if they don't exist already.

- The options `--ample_mem` and `--buffer_size <string>` are now mutually exclusive.

- Changed the directory being passed on when `--cytosine_report` is specified from parent directory' to 'output directory'.

### bismark2report

- Major rewrite of `bismark2report`: HTML file are now rendered using [Plotly.js](https://plot.ly/javascript/) which is completely open source and free to use. Highcharts and JQuery were dropped, as was raised here: https://github.com/FelixKrueger/Bismark/issues/177.
  The files `bioinfo.logo`, `bismark.logo`, `plot.ly` and `plotly_template.tpl` are read in dynamically from a new folder plotly. `bismark_sitrep` and all its contents no longer ship with Bismark. The Bismark HTML reports should be completely self-contained.

### bismark2summary

- Major rewrite of `bismark2summary`: HTML file are now rendered using [Plotly.js](https://plot.ly/javascript/) which is completely open source and free to use. Highcharts and JQuery were dropped, as was raised here: https://github.com/FelixKrueger/Bismark/issues/177. The files `bioinfo.logo`, `bismark.logo`, `plot.ly` and `plotly_template.tpl` are read in dynamically from a new folder plotly. `bismark_sitrep` and all its contents no longer ship with Bismark. The Bismark HTML Summary reports should be completely self-contained.

## Changelog for Bismark v0.19.1

### Bismark

Child processes are now terminated properly once the mapping and merging steps have completed successfully. This means that supplying a comma-separated list of input files such as `-1 R1.fastq,simulated_1.fastq,ZZZ_R1.fastq -2 R2.fastq,simulated_2.fastq,ZZZ_R2.fastq --multicore 4` does no longer spawn a steadily increasing number of Bismark instances. [issue #138](https://github.com/FelixKrueger/Bismark/issues/138)

Bismark now also accepts genome FastA files if they are `gzip` compressed (ending in `.gz`)

### coverage2cytosine

Restructured the way output and input file paths are handled. All should be working now, inluding combinations of `--gzip`, `--dir /PATH/`, `--merge_CpG`, `--disco`, `--split_by_chromosome` etc.

The genome folder may now be specified as full or relative path.

Now also accepts genome FastA files if they are `gzip` compressed (ending in `.gz`)

### bam2nuc

Now also accepts genome FastA files if they are `gzip` compressed (ending in `.gz`)

### bismark_genome_preparation

Now also accepts genome FastA files if they are `gzip` compressed (ending in `.gz`)

### deduplicate_bismark

Changed the way strands are handled by replacing `+` and `-` for a strand identity `OT`,`CTOT`, `CTOB` and `OB` instead. This should avoid conflicts in (the extremely rare) occasions where reads with the same starting and end positions might have come from both the `OT` and `CTOB` strands, or its bottom strand equivalent. (see here for more info: [issue #161](https://github.com/FelixKrueger/Bismark/issues/161#issuecomment-371878189) )

Completely removed the code for the `--representative` mode. People should have stopped wanting that anyway.

## Changelog for Bismark v0.19.0

### Bismark

Changed the methylation call behaviour so that insertions in a read (which are filled in with `X` for the methylation call) are also considered as `Unknown context` for the methylation call. Here is [issue #135](https://github.com/FelixKrueger/Bismark/issues/135).

### filter_non_conversion

Added new options `--percentage_cutoff [int]` and `--minimum_count [int]` to allow filtering reads for non-bisulfite conversion using an overall methylation percentage and count cutoff. Here is [issue #122](https://github.com/FelixKrueger/Bismark/issues/122).

### deduplicate_bismark

Added option `--multiple` to the deduplicator to treat several input SAM/BAM files as the same sample. Here is [issue #107](https://github.com/FelixKrueger/Bismark/issues/107).

Added option `--output_dir` to `deduplicate_bismark` so that it can be used in the Google cloud. Here is [issue #123](https://github.com/FelixKrueger/Bismark/issues/123)

### coverage2cytosine

Output files are now handled better and more consistently. Default processing now produces the following output files (with `--gzip`):

```
CpG_report.txt(.gz) or
CX_report.txt(.gz)
```

The option `--NOMe-Seq` now produces four output files (with `--gzip`):

```
NOMe.CpG_report.txt(.gz)
NOMe.CpG.cov(.gz)
NOMe.GpC_report.txt(.gz)
NOMe.GpC.cov(.gz)
```

The option `--split_by_chromosome` should work in either default, `--gc` or `--NOMe-seq` mode.

NOMe-Seq processing if now ignoring processing that were not covered by any reads.

Improved handling of the `--output_dir`, i.e. the folder will be created if it doesn't exist already and making the path absolute.

Added new option `--discordance <int>` to allow filtering for discordance pf top and bottom strand when in `--merge_CpG` mode. CpG positions for which either the top or bottom strand was not measured at all will not be assessed for discordance and hence appear in the regular 'merged_CpG_evidence.cov' file. More details in [issue #91](https://github.com/FelixKrueger/Bismark/issues/91).

Fixed context extraction for Gs at positions 1 and 2 of a chromosome/contig. Also, last cytosine positions of not covered chromosomes are now ignored in the same way as for covered chromosomes [issue #127](https://github.com/FelixKrueger/Bismark/issues/127)

### copy_files_for_release

Is now working from any location.

## Changelog for Bismark v0.18.2

### Bismark

Changed the timing of when ambiguous within same thread alignments are reset. Previously some alignments were incorrectly considered ambiguous ([see here](https://github.com/FelixKrueger/Bismark/issues/108)).

### bismark2bedGraph

The option `--ample_mem` is now mutually exclusive with specifying memory for the `UNIX sort` command via the option `--buffer_size`.

## Changelog for Bismark v0.18.1

Commented out warning messages for certain ambiguous alignments for paired-end alignments.

## Changelog for Bismark v0.18.0

Changed `FindBin qw($Bin)` to `FindBin qw($RealBin)` for `bismark`, `bismark_methylation_extractor`, `bismark2report` and `bismark2summary` so that symlinks are resolved before calling different modules.

### Bismark

Fixed the behaviour of (very rare) ambiguous [corner cases](https://github.com/FelixKrueger/Bismark/issues/105) where a sequence had a perfect sequence duplication within the valid paired-end distance.

### bismark_methylation_extractor

Added new option `--yacht` (for **Y**et **A**nother **C**ontext **H**unting **T**ool) that writes out additional information about the read a methylation call belongs to, and its output is meant to be fed into the NOMe_filtering script (see below). This option writes out a single 'any_C_context' file that contains all methylation calls for a read consecutively. Its intended use is single-cell NOMe-Seq data, so it only works in single-end mode (paired-end reads often suffer from chimaera problems...)

`--yacht` adds three additional columns to the standard methylation call files:

`<read start> <read end> <read orientation>`

For forward reads (+ orientation) the start position is the left-most position wheras for reverse reads (- orientation) it is the rightmost position.

Changed FindBin qw($Bin) to FindBin qw($RealBin) so that symlinks are resolved before calling different modules.

### NOMe_filtering

This script reads in methylation call files from the Bismark methylation extractor that contain additional information about the reads that methylation calls belonged to. It processes entire (single-end) reads and then filters calls for NOMe-Seq positions (nucleosome occupancy and methylome sequencing) where accessible DNA gets methylated in a GpC context:

     (i) filters CpGs to only output cytosines in A-CG and T-CG context
    (ii) filters GC context to only report cytosines in GC-A, GC-C and GC-T context

Both of these measures aim to reduce unwanted biases, i.e. the influence of G-CG (intended) and C-CG (off-target) on endogenous CpG methylation, and the influence of CpG methylation on (the NOMe-Seq specific) GC context methylation.

The NOMe-Seq filtering output reports cytosines in CpG context only if they are in A-CG or T-CG context,
and cytosines in GC context only when the C is not in CpG context. The output file is tab-delimited and in
the following format (1-based coords):

```
<readID>  <chromosome>  <read start>  <read end>  <count methylated CpG>  <count non-methylated CpG>  <count methylated GC>  <count non-methylated GC>
HWI-D00436:298:C9KY4ANXX:1:1101:2035:2000_1:N:0:_ACAGTGGT 10 8517979 8518098 0 1 0 1
HWI-D00436:298:C9KY4ANXX:1:1101:5072:1993_1:N:0:_ACAGTGGT 8 9476630 9476748 0 0 0 2
```

### coverage2cytosine

Fixed an [issue](https://github.com/FelixKrueger/Bismark/issues/89) in `--merge_CpG` mode caused by chromosomes ending in CG.

Fixed an [issue](https://github.com/FelixKrueger/Bismark/issues/98) caused by specifying `--zero` as well as `--merge_CpG`.

### bam2nuc

Fixed an issue where the option `--output_dir` had been ignored.

### filter_non_conversion

Removed help text indicating that this script also did the deduplication.

## RELEASE NOTES FOR Bismark v0.17.0 (18 01 2017)

### Bismark

The option `--dovetail` is now the default behaviour for paired-end Bowtie2 libraries to assist with
alignments that have undergone 5'-end trimming. Can be disabled using the new option `--no_dovetail`.

Added time stamp to the Bismark run.

Chromosome names with leading spaces now cause Bismark to bail.

Fixed path handling for `--multicore` mode when `--prefix` had been specified as well.

Bismark now quits if the Bowties could not be executed properly.

Bails if supplied filenames do not exist.

### Documentation

Added Overview of different library types and kits to the Bismark User Guide (https://github.com/FelixKrueger/Bismark/tree/master/Docs#viii-notes-about-different-library-types-and-commercial-kits).

Also added documentation for Bismark modules `bam2nuc`, `bismark2report`, `bismark2summary` and `filter_non_conversion`.

Added a Markdown to HTML converter (make_docs.pl; thanks to Phil Ewels for this).

### filter_non_conversion

Added a new script that allows filtering out of reads or read-pairs if the apparent non-CG methylation exceeds a certain threshold (3 by default). Optionally, the non-CG count may be forced to occur on consecutive non-CGs using the option `--consecutive`.

Added time stamp to filtering step.

### bismark2bedGraph

For the creation of temporary files, we are now replacing `/` characters in the chromosome names with `_` (underscores), similar to `|` (pipe) characters, as these `/` would attempt to write files to non-existing directories.

### deduplicate_bismark

Single-/paired-end detection now also accepts --1 or --2.

Added EOF or truncation detection, causing the deduplicator to die.

### bismark_methylation_extractor

Single-/paired-end detection now also accepts --1 or --2.

Added EOF or truncation detection, causing the methylation extractor to die.

Addded fatal ID1/ID2 check to paired-end extraction so that files which went out-of-sync at a later stage do not complete silently (but incorrectly!)

### bismark2report

Major refactoring of `bismark2report`, the output should look the same though. Massive thanks to Phil Ewels for this.

### coverage2cytosine

Added a new option `--NOMe-seq` to filter nucleosome occupancy and methylome sequencing (NOMe-Seq) data where accessible DNA gets enzymatically methylated in a GpC context. The option `--NOMe-seq`:

     i) filters the genome-wide CpG-report to only output cytosines in ACG and TCG context
    ii) filters the GC context output to only report cytosines in GCA, GCC and GCT context

Both of these measures aim to reduce unwanted biases, namely the influence of GCG and CCG on endogenous CpG methylation, and the inlfluence of CpG methylation on (the NOMe-Seq specific) GC context methylation. PLEASE NOTE that NOMe-Seq data requires a .cov.gz file as input which has been generated in non-CG mode (`--CX`).

### bismark_genome_preparation

Fixed a bug that arose when `--genomic_composition` was specified (now moving back to the genome directory for _in silico_ conversion).

## RELEASE NOTES FOR Bismark v0.16.3 (25 07 2016)

### Bismark

Fixed another bug where a subset of ambiguous Bowtie 2 alignments where considered unique even though
they had been ambiguous in a different thread before, e.g.:
Read 1: AS:i:0 XS:i:0
Read 2: AS:i:0
In such cases the 'ambiguous within thread' variable is now only reset if the second alignment is truly better. This
also affects the 'ambig.bam' output.

Added support for large Bowtie (1) index files ending in .ebwtl which had been added in Bowtie v1.1.0.

## RELEASE NOTES FOR Bismark v0.16.2 (19 07 2016)

### Bismark

Fixed a bug for Bowtie 2 alignments where reads that should be considered ambiguous were incorrectly
assigned to the first alignment thread. This error had crept in during the 'changing the behavior of
corner cases' in v0.16.0). Thanks to John Gaspar for spotting this!

Changed the Shebang in all scripts of the Bismark suite to "#!/usr/bin/env perl" instead of
"#!/usr/bin/perl".

### deduplicate_bismark

Does now bail with a useful error message when the input files are empty.

### bismark_genome_preparation

Added new option --genomic_composition so that the genomic composition can be calculated and written right
at the genome preparation stage rather than by using bam2nuc.

### bam2nuc

Now also calculates a fold coverage for the various (di-)nucleotides. The changes in the nucleotide_stats
text file are also picked up and plotted by bismark2report.

Added a new option --genomic_composition_only to just process the genomic sequence without requiring
any data files.

### bismark2summary

Added option -o/--basename <filename> to specify a certain filename. If not specified the name will
remain 'bismark_summary_report.txt/html'.

Added documentation and the options --help and --version to be consistent with the rest of Bismark.

Added option --title <string> to give the HTML report a different title

## RELEASE NOTES FOR Bismark v0.16.1 (25 04 2016)

### Bismark

Removed unintended warn/sleep statement during PE/Bowtie 2 alignments that would slow alignments down
dramatically. Sorry for that.

## RELEASE NOTES FOR Bismark v0.16.0 (20 04 2016)

### Bismark

File endings .fastq | .fq | .fastq.gz | .fq.gz are now removed from the output file (unless they were
specified with --basename) in a bid to reduce the length of the already long filenames.

Enabled the new option '--dovetail' (which will be turned on by default for '--pbat' libraries) which
will now allow dovetailing reads to be reported. For a more in-depth description see
https://github.com/FelixKrueger/Bismark/issues/14.

Changed the behaviour of corner cases to where several non-directional alignments could have existed
for the very same position but to different strands so that now the best alignment trumps the weaker
one. As an example: If you relaxed the alignment criteria of a given alignment to allow ~ 60 mismatches
for PE alignment we did find an alignment to the OT strand with a combined AS of -324, but there also
was an alignment to the CTOB strand with and AS of 0 (perfect alignment). The CTOB now trumps the OT
alignment, and the methylation information information is now reported for the bottom strand.
Credits go to Sylvain Foret (ANU, Canberra) for bringing this to our attention!

### New module: bismark2summary

bismark2summary accepts Bismark BAM files as input. It will then try to identify Bismark reports,
and optionally deduplication reports or methylation extractor (splitting) reports automatically based
the BAM file basename. It produces a tab delimited overview table (.txt) as well as a graphical HTML
report.
Examples can be found at http://www.bioinformatics.babraham.ac.uk/projects/bismark/bismark_summary_report.html
and http://www.bioinformatics.babraham.ac.uk/projects/bismark/bismark_summary_report.txt. Thanks to
@ewels for help with the Java Script part!

### New module: bam2nuc

The new Bismark module bam2nuc calculcates the average mono- and di-nucleotide coverage of libraries
and compares this to the genomic average composition. bam2nuc can be called straight from within
Bismark (option '--nucleotide_coverage') or run stand-alone. bam2nuc creates a '...nucleotide_stats.txt'
file that is also automatically detected by bismark2report and incorporated into the HTML report.

### bismark2_sitrep.tpl

Removed an extra function call in bismark_sitrep.tpl so that the M-bias 2 plot is drawn once
the M-bias 1 plot has finished drawing (parallel processing could with certain browsers and data
may have resulted in a white spaceholder only).

### Methylation extractor

Altering the file path handling of coverage2cytosine and bismark2bedGraph also required some changes
in the methylation extractor.

### bismark2bedGraph

Input file path handling has been completely reworked. The output file which can be specified as
'-o output.bedGraph' now has to be a single file name and mustn't contain any path information.
A particular output folder may be specified with '-dir /any/path/'.

Addressing the file path handling issue also fixed a similar issue with the option --remove_spaces
when -o had been specified.

### coverage2cytosine

Changed gunzip -c for gunzip -c when reading a gzipped coverage file.

Changed the way in which the coverage input file is handed over from the methylation_extractor
to coverage2cytosine (previously the path information might have been part of the filename, but
instead it will now be only part of the --dir output_directory option.

## RELEASE NOTES FOR Bismark v0.15.0 (08 01 2016)

### Bismark

Added option '--se/--single_end <list>'. This sets single-end mapping mode explicitly giving a
list of file names as <list>. The filenames may be provided as a comma [,] or colon [:]-separated
list.

Added option '--genome_folder <path/to/genome>' as alternative to supplying the genome as the
first argument.

Added an option '--rg_tag' to print an @RG header line as well as and RG:Z: tag to each read.
The ID and SAMPLE fields default to 'SAMPLE', but can be specified manually with '--rg_id' or
'--rg_sample'.

Added new option '--ambig_bam' for Bowtie2-mode only, which writes out a single alignment for
sequences with multiple alignments to a special file ending in '.ambiguous.bam'. The alignments
are in Bowtie2 format and do not any contain Bismark specific entries such as the methylation
call etc. These ambiguous BAM files are intended to be used as coverage estimators for variant
callers. Works for single-end and paired-end alignments in single or multi-core mode.

Added the new options '--cram' and '--cram_ref' to Bismark for both paired- and single-end alignments
in single or multi-core mode. This option requires Samtools version 1.2 or higher. A genome
FastA reference may be supplied as a single file with the option '--cram_ref'; if this is not
specified the file is derived from the reference FastA files used for the Bismark run, and written
to the file 'Bismark_genome_CRAM_reference.mfa' into the output directory.

### deduplicate_bismark

Added better handling of cases when the input file was empty (died for percentage calculation
instead of calling it N/A)

Added a note mentioning that Read1 and Read2 of paired-end files are expected to follow each
other in two consecutive lines and possibly require name-sorting prior to deduplication. Also
added a check that reads the first 100000 lines to see if the file appears to have been sorted
and bail out if this is true.

### Methylation extractor

Added support for CRAM files (this option requires Samtools version 1.2 or higher).

### bismark_genome_preparation

Added process handling to the child processes.

### coverage2cytosine

Added option --gzip to compress output files. This currently only works for the default CpG_report
and CX_report output files (and thus not with the option --gc or --split_files. The option --gzip
is now also passed on from the bismark_methylation_extractor.

Added a check to coverage2cytosine to bail if no information was found in the coverage file, e.g. if
a wrong file path for a cov.gz file had been specified.

### bismark2bedGraph

Changed the way gzip compressed input files are handled when using the unix sort command (i.e. with
--scaffolds/--gazillion or without --ample_memory.

## RELEASE NOTES FOR Bismark v0.14.5 (20 08 2015)

### deduplicate_bismark

Changed all instances of literal calls of 'samtools' calls to '$samtools_path'.

## RELEASE NOTES FOR Bismark v0.14.4 (17 08 2015)

### Bismark

Input files specified with filepath information for FastA files are now handled properly in
--multicore runs (this was fixed only for FastQ files in the previous patch).

Changed the FLAG values of paired-end alignments to the CTOT or CTOB strands so that reads can
be properly displayed in SeqMonk when imported as BAM files. This change affects only paired-end
alignments in --pbat or --non_directional mode. In detail we simply swapped the Read 1 and Read 2
FLAG values round so reads now resemble exactly concordant read pairs to the OT and OB strands.
Note that results produced by the methylation extractor or further downstream of that are not
affected by this change. FLAG values now look like this:

                                      Read 1       Read 2

                              OT:         99          147

                              OB:         83          163

                              CTOT:      147           99

                              CTOB:      163           83

Changed the default mode of operation to --bowtie2. Bowtie (1) alignments may still be chosen using
the option --bowtie1.

Unmapped (option --unmapped) and ambiguous (option --ambiguous) files are now written out as gzip
compressed files so they don't have to be gzipped manually every single time.

### Bismark Genome Preparation

Changed the execution of the genome indexing of the parent process to system() rather
than an exec() call since this seemed to lead to interesting faults when run in
a pipeline setting.

Changed the default indexing mode to --bowtie2. Bowtie (1) indexing is still available via the
option --bowtie1.

### bismark2bedGraph

The coverage (.cov) and bedGraph (.bedGraph) files are now written out as gzip compressed files so
you don't have to gzip them manually every single time.

### coverage2cytosine

Added a new option --gc_context to reprocess the genome and find methylation in GpC context. This might
be useful for certain applications where GpC methylases had been deployed. The output format is exactly
the same as for the normal CpG report, and only positions covered by at least one read are reported. A
coverage file will also be written out.

### deduplicate_bismark

Removed redundant close() statements so there shouldn't be any warning messages popping up again.

## RELEASE NOTES FOR Bismark v0.14.3 (06 May 2015)

### Bismark

Changed the renaming settings for paired-end files so that 'sam' within the filename no longer gets
renamed to 'bam' (e.g. smallsample.sam -> smallbample.sam).

Input files specified with filepath information are now handled properly in --multicore runs.

The --multicore option currently requires the files to be in BAM format, so specifying --sam at the
same time is disallowed.

### Methylation extractor

Another bug fix for the same issue as in 0.14.1 that had crept in the 0.14.2 release.

### coverage2cytosine

Changed the option --merge_CpG so that CGs starting at position 1 are not considered (since the 3-base
sequence context of the bottom strand C at position 2 can not be determined)

## RELEASE NOTES FOR Bismark v0.14.2 (27 Mar 2015)

### Methylation extractor

Added a bug fix for the same issue as in 0.14.1 that was overlooked in the earlier release. Apologies

## RELEASE NOTES FOR Bismark v0.14.1 (27 Mar 2015)

### Bismark

Fixed the cleaning up stage in a --multicore run when --gzip had been specified as well.

Fixed the handling of files in a --multicore run when the input files had been specified including
file path information.

### deduplicate_bismark

Now also removing newline characters from the read conversion tag in case other programs interfered
with the tag ordering and put this tag into the very last column.

### Methylation_extractor

Fixed a bug with paired-end reads when the reads should have been trimmed from their 3' ends (option
--ignore_3prime). More specifically the position of reads on the - strand wasn't adjusted appropriately
when the read had been trimmed from its 3' end, which would result in offset methylation call positions.
Thanks to V. Brendel for spotting this!

## RELEASE NOTES FOR Bismark v0.14.0 (06 Mar 2015)

### Bismark

Finally added parallelization to the Bismark alignment step using the option '--muticore <int>'
which sets the number of parallel instances of Bismark to be run concurrently. At least in this
first distribution this is achieved by forking the Bismark alignment step very early on so that
each individual Spawn of Bismark (SoB?) processes only every n-th sequence (n being set by --multicore).
Once all processes have completed, the individual BAM files, mapping reports, unmapped or ambiguous
FastQ files are merged into single files in very much the same way as they would have been generated
running Bismark conventionally with only a single instance.

If system resources are plentiful this is a viable option to speed up the alignment process
(we observed a near linear speed increase for up to --multicore 8 tested so far). However, please note
that a typical Bismark run will use several cores already (Bismark itself, 2 or 4 threads of
Bowtie/Bowtie2, Samtools, gzip etc...) and ~10-16GB of memory depending on the choice of aligner and
genome. WARNING: Bismark Parallel (BP?) is resource hungry! Each value of --multicore specified
will effectively lead to a linear increase in compute and memory requirements, so --multicore 4 for
e.g. the GRCm38 mouse genome will probably use ~20 cores and eat ~40GB or RAM, but at the same time
reduce the alignment time to ~25-30%. You have been warned.

Changed the default output to BAM. SAM output may be requested using the option --sam.

No longer generates a piechart (.png) with the alignment stats. bismark2report generates
a much nicer report anyway.

### Bismark Methylation extractor

To detect paired-end alignment mode from the @PG header line, white spaces before and after -1 and
-2 are now required. Previously files containing e.g. -1-2 in the filenames could have been identified
incorrectly as paired-end files.

### deduplicate_bismark

To detect paired-end alignment mode from the @PG header line, white spaces before and after -1 and
-2 are now required. Previously files containing e.g. -1-2 in the filenames could have been identified
incorrectly as paired-end files.

Added option --version so that Clusterflow can report a version number.

### bismark2bedGraph

Fixed path handling for cases where the input files were given with path information and an output
directory had been specified as well.

### coverage2bismark

Fixed a typo in the shebang which prevented coverage2cytosine from running.

## RELEASE NOTES FOR Bismark v0.13.1 (26 Dec 2014)

### Bismark Genome Preparation

Added a check for unique chromosome names to the Bismark indexer to avoid disappointments later.

### Bismark Methylation extractor

Added a new option --mbias_off, which processes the files as normal but does not write out any M-bias
files. This option is meant for users who run the methylation extractor two times, the first time to
figure out whether there is a bias that needs to be removed, and the second time using the --ignore options,
but without overwriting the already existent M-bias reports.

Fixed a bug for the M-bias reports when the option --multicore was used, in which case only the numbers
of one core were used to constuct the report. Now every different thread writes out an individual M-bias
table, and once the methylation extraction has completed all these individual files are merged into a single,
cumulative table as it should be.

Added closing statements for the BAM in disguise filehandle.

### bismark2bedGraph

Deferred removal of the input file path information a little so that specifying file paths doesn't prevent
bismark2bedGraph from finding the input files anymore.

If the specified output directory doesn't exist it will be created for you.

Changed the way scaffolds are sorted (with --gazillion specified) to -k3,3V (this was done following
a suggestion by Volker Brendel, Indiana University: "The -k3,3V sort option is critical when the
sequence names are numbered scaffolds (without left-buffering of zeros). Omit the V, and things go
very wrong in the tallying of reads.")

### coverage2cytosine

Added a new option --merge_CpG that will post-process the genome-wide report to write out an
additional coverage file which has the top and bottom strand methylation evidence pooled into a
single CpG dinucleotide entity. This may be the desirable input format for some downstream processing
tools such as the R-package bsseq (by K.D. Hansen). An example would be:

genome-wide CpG report (old)
gi|9626372|ref|NC_001422.1| 157 + 313 156 CG
gi|9626372|ref|NC_001422.1| 158 - 335 156 CG

merged CpG evidence coverage file (new)
gi|9626372|ref|NC_001422.1| 157 158 67.500000 648 312

This option is currently experimental, and only works if CpG context only and a single genome-wide report
were specified (i.e. it doesn't work with the options --CX or --split_by_chromosome).

Changed the processing of not-covered chromosomes so that they are sorted and not processed randomly.
This should make runs more reproducible.

## RELEASE NOTES FOR Bismark v0.13.0 (01 Oct 2014)

### Bismark

Fixed renaming issue for SAM to BAM files (which would have replaced any occurrence of sam in the
file name, e.g. sample1\_... instead of the file extension .sam).

### Methylation extractor

Added new option '--multicore <int>' to set the number of cores to be used for the methylation
extraction process. If system resources are plentiful this is a viable option to speed up the
extraction process (we observed a near linear speed increase for up to 10 cores used). Please
note that a typical process of extracting a BAM file and writing out '.gz' output streams will
in fact use ~3 cores per value of --multicore <int> specified (1 for the methylation extractor
itself, 1 for a Samtools stream, 1 for GZIP stream), so --multicore 10 is likely to use around
30 cores of system resources. This option has no bearing on the bismark2bedGraph or genome-wide
cytosine report processes.

Added two new options '--ignore_3prime <INT>' (for single-end alignments and Read 1 of paired-end
alignments) and '--ignore_3prime_r2 <INT>' (for Read 2 of paired-end alignments) to remove positions
that display a methylation call bias from the 3' end of reads.

The option --no_overlap is now the default for paired-end data. One may explicitly choose to include
overlapping data with the option '--include_overlap'.

The splitting report will now be written out by default (option --report).

In paired-end mode, read-pairs which had been skipped because either read was shorter than a specified
(very high) value of '--ignore' or '--ignore_r2' will now have the information of the other read
extracted if it meets the length criteria (if applicable). Thanks to Andrew Dei Rossi for contributing
a patch.

### bismark2bedGraph

Fixed the location of the sorting directory which could have failed if an output directory had been
specified.

## RELEASE NOTES FOR Bismark v0.12.5 (21 July 2014)

### Bismark

Added one more check to improve the ambiguous alignment detection. In more detail this adds a check
whether the current ambiguous alignment is worse than the best alignment so far, in which case the
sequence does not get flagged as ambiguous. Thanks to Ashwath Kumar for spotting these issues).

## RELEASE NOTES FOR Bismark v0.12.4 (21 July 2014)

### Bismark

Improved the way ambiguous alignments are handled in Bowtie 2 mode. Previously, sequences were
classified as ambiguously aligning as soon as a sequence produced several equally good alignments
within the same alignment thread. Under certain circumstances however there may exist equally good
alignments within the same alignment thread, but the sequence might have a better (unique) alignment
in another thread. Such a unique alignment will now trump the ambiguous alignment flag.

Got rid of 2 warning messages of MD-tag information for reads containing deletions (Bowtie 2 mode only)
which accidentally made it through to the release.

Added '-x' to the Bowtie 2 invocation for FastA sequences so that it works again. (It used to work
previously only because Bowtie 2 did not check it properly and automatically used bowtie2-align-s, but
now it does check...).

### Methylation extractor

Line endings are now chomped at an earlier stage so that interfering with the optional fields in the
Bismark BAM file doesn't break the methylation extractor (e.g. reordering of optional tags by
Picard).

## RELEASE NOTES FOR Bismark v0.12.3 (17 June 2014)

### Bismark

Replaced the XX-tag field (base-by-base mismatches to the reference, excluding indels) by an MD:Z:
field that now properly reflects mismatches as well as indels.

Fixed the hemming distance value (NM:i: field) for reads containing insertions (Bowtie 2 mode only),
which was previously offset by the number of insertions in the read.

### bismark2bedGraph

Changed the '--zero_based' option of the methylation extractor and bismark2bedGraph to write out an
additional coverage file (ending in .zero.cov) which uses the UCSC zero-based, half-open standard.

Changed the requirement of CpG context files to start with CpG... (from CpG\_...).

## RELEASE NOTES FOR Bismark v0.12.2 (14 May 2014)

Added support for the new 64-bit large index file for very large genomes in Bowtie 2 mode. The
indexes end in .bt2l (instead of .bt2 for small genomes). If both small (.bt2) and large index
files are in the same folder the small ones are used.

Fixed a bug that caused a the second last chromosome of a multi-FastA reference file to be absent
from the SAM header (really only the header line, the genome as well as the alignments were not
affected by this).

When the option --basename is specified, SE amibiguous file names now feature an underscore. Also, the
pie chart file names are derived from the the basename.

### Methylation extractor

Introduced a length check when the options --ignore or --ignore_r2 were set to ensure that only reads
that are long enough are being processed.

## RELEASE NOTES FOR Bismark v0.12.1 (29 Apr 2014)

Added calculation of MAPQ values for SAM/BAM output generated with Bowtie 2 for both single-end and
paired-end mode. The calculation is implemented like in Bowtie 2 itself, so please don't ask me why
the values are what they are. Thanks to Andrew Dei Rossi (Stanford) for taking the initiative to adapt
the Bowtie 2 code to Perl after a discussion that was sparked on SeqAnswers). Mapping quality values
are still unavailable for alignments performed with Bowtie and retain a value of 255 throughout.

Fixed an uninitialised value warning for PE alignments with Bowtie 2 that occurred whenever Read 2
aligned to the very start of a chromosome (this only affected the warning itself and has no impact on
any results).

### coverage2cytosine

Changed this module so that all chromosomes or scaffolds are processed irrespective of whether they
were covered in the sequencing experiment. For organisms with few chromosomes and lots of reads the
outcome would most probably be the same, but it might have affected the CpG/Cytosine reports for genomes
with lots of very small scaffolds that were not covered by any reads. Thanks to S. Jhanwar for bringing
this to my attention.

## RELEASE NOTES FOR Bismark v0.11.1 (07 Apr 2014)

The option --pbat now also works for use with Bowtie 2, in both single-end and paired-end mode. The
only limitation to that is that it only works with FastQ files and uncompressed temporary files.

Changed the order the @SQ lines are written out to the SAM/BAM header from random to the same order
they are being read in from the genomes folder (or the order of the files in which they occur within
a multi-FastA file).

Included a new option -B/--basename <basename> for output files instead of deriving these names from
the input file. --basename takes precedence over the option --prefix.

Unmapped or ambiguous files now end in .fq or.fa for FastA or FastQ files, respectively (instead
of .txt files).

### Methylation extractor

The methylation extractor willl no longer attempt to delete unused files if --mbias_only was speficied.

Added a test to see if a file that does not end in .bam is in fact a BAM file, and if this succeeds open
the file using Samtools view.

## RELEASE NOTES FOR Bismark v0.10.1 (26 Nov 2013)

### Methylation extractor

The methylation extractor does now detect automatically whether Bismark alignment file(s) were run
in single-end or paired-end mode (this detection only happens once for the first file to be analysed
and will then be used for all files should there be more than one arguments in the same command).
The automatic detection can be overridden by manually specifying -s or -p, and this option is only
available for SAM/BAM files.

### deduplicate_bismark

The deduplication script does now detect automatically whether a Bismark alignment file was run
in single-end or paired-end mode (this happens separately for every file analysed). The automatic
detection can be overridden by manually specifying -s or -p, and this option is only available
for SAM/BAM files.

### bismark2bedGraph

When run in stand-alone mode, the coverage file will replace 'bedGraph' as the file ending with
'bismark.cov'. If the output filename is anything other than 'bedGraph', '.bismark.cov' will be
appended to the bedGraph filename.

When run in stand-alone mode, '--counts' will be enabled by default for the coverage output.

Added a new option --scaffolds/--gazillion for users working with unfinished genomes sporting tens or
even hundreds of thousands of scaffolds/contigs/chromosomes. Such a large number of reference sequences
frequently resulted in errors with pre-sorting reads to individual chromosome files because of the operating
system's limitation of the number of filehandles that can be written to at any one time (typically this
limit is anything between 128 and 1024 filehandles; to find out this limit on Linux, type: ulimit -a).
To bypass the limitation of open filehandles, the option --scaffolds does not pre-sort methylation
calls into individual chromosome files. Instead, all input files are temporarily merged into a single file
(unless there is only a single file), and this file will then be sorted by both chromosome AND position
using the Unix sort command.
Please be aware that this option might take a looooong time to complete, depending on the size of
the input files, and the memory you allocate to this process (see --buffer_size). Nevertheless, it seems
to be working (even with the option --CX specified).

Added a new option '--ample*memory'. Using this option will not sort chromosomal positions using the UNIX
'sort' command, but will instead use two arrays to sort methylated and unmethylated calls, respectively.
This may result in a faster sorting process for very large files, but this comes at the cost of a larger
memory footprint (as an estimate, two arrays of the length of the largest human chromosome 1 (~250 million bp)
consume around 16GB of RAM). Note however that due to the overhead of creating and looping through huge
arrays this option might in fact be \_slower* for small-ish files (up to a few million alignments). Note
also that this option is not currently compatible with options '--scaffolds/--gazillion'. This option
still needs some efficiency testing as to when it actually makes sense to use it, but it produces identical
results to the default sort option.

### bismark2report

Specifying a single file for each of the optional reports does now will now work as intended, instead
of being skipped.

### coverage2cytosine

Added some counting and statements to indicate when the run finished successfully (it proved to be
difficult to follow the report process for a genome with nearly half a million scaffolds...)

## RELEASE NOTES FOR Bismark v0.10.0 (11 Oct 2013)

### Bismark

The option --prefix <some.prefix> does now also work for the C->T and G->A transcribed temporary
files to allow multiple instances of Bismark to be run on the same file in the same folder (e.g.
using Bowtie and Bowtie 2 or some stricter and laxer parameters concurrently).

### Bismark Genome Preparation

Made a couple of changes to make the genome preparation fully non-interactive. This means that the
path to the genome folder and to Bowtie (1/2) have to be specified up front (for Bowtie (1/2) it
is otherwise assumed that it is in the PATH). Furthermore, already existing bisulfite indices in
the target folder will be overwritten and the user is no longer prompted if he agrees to this. We
got rid of this because creating a second index (Bowtie 1 as well as 2) in the same folder in
non-interactive mode got stuck in loops asking whether it is alright to proceed or not, generating
therabyte sized log files without ever starting doing anything useful...).

### Methylation extractor

The methylation extractor will now delete unused methylation context files (e.g. CTOT and CTOB files
for a directional library). I finally got round to implementing this after having to delete manually
thousands of files containing the header line only...

### bismark2bedGraph

Dropped the option -k3,3 from the sort command to result in a dramatic speed increase while sorting.
This option had been used previously to enable sorting by chromosome in addition to position, but
should no longer be needed because the files are being read in sorted by chromosome already.

This module does now produces these two output files:

     (1) A bedGraph file, which now contains a header line: 'track type=bedGraph'
         The genomic start coords are 0-based, the end coords are 1-based.

     (2) A coverage file ending in .cov. This file replaces the former 'bedGraph --counts' file and is
         required to proceed with the subsequent step to generate a genome-wide cytosine report (the
         module doing this has been renamed to coverage2cytosine to reflect this file name change.

### coverage2cytosine

Changed the name of this module from 'bedGraph2cytosine' to 'coverage2cytosine' to reflect the change
that this module now requires the methylation coverage file produced by the bismark2bedGraph module
, ending in .cov (this coverage file replaces the former "bedGraph --counts" output).

Previously, the cytosine report would always report every C position in any context, even though
the default should have reported CpG positions only. This has now been fixed.

### bismark2report

Changed the behavior of this module to automatically find all Bismark mapping reports in the current
working directory, and to try and work out whether the optional reports are present as well (i.e.
deduplication, splitting and M-bias reports). This uses the file basename and will fail if the files
have been renamed at any stage. Specifying file names using the individual options takes precedence over
the automatic detection.

### deduplicate_bismark

Renamed the rather long deduplication script to this slightly shorter one. Also added some filehandle
closing statements that might have caused buffering issues under certain circumstances.

## RELEASE NOTES FOR Bismark v0.9.0 (16 Aug 2013)

### Bismark

Implemented the new methylation call symbols 'U' and 'u' for methylated or unmethylated cytosines
in unknown sequence context, respectively. If the sequence context bases contain any N, e.g. CN or
CHN, the context cannot be determined accurately (previously, these cases were assumed to be in CHH
context).
These situations may arise whenever the reference sequence contains Ns, or when insertions in the
read occur close to a cytosine position (bases inserted into the read have no direct equivalent in
the reference sequence and were assumed to be Ns for the methylation call). In practical terms, the
'U/u' methylation calls will only occur for Bowtie 2 alignments because Bowtie 1 does not support
gapped alignments or read alignments if the reference contains any N's. The Bismark report will now
also include the 'U/u' statistics, such as count and % methylation, however only if run in Bowtie 2
mode.
Thanks to Pete Hickey for his contributions towards resolving this issue.

Fixed a bug that occurred when generating the alignment overview pie chart that occurred for PBAT
libraries only.

### Methylation extractor

Added handling of the newly introduced methylation call U/u for cytosines in Unknown sequence context.
These methylation calls are simply ignored to not cause too much confusion for downstream analysis.

### bismark2report

With this version, we are introducing the new module 'bismark2report' which generates a graphical HTML
report of Bismark alignment, deduplication, splitting and M-bias statistics. The alignment report is
required for the HTML report, the deduplication, splitting and M-bias reports are optional. For the
rendering to work as intended your browser needs to support Javascript. The bismark2report module is
part of the Bismark suite, and it reqires the template file 'bismark_sitrep.tpl' to also reside in the
same directory (one may still specify a different output directory).

Since several different modules of Bismark may be included into this report that may or may not have
been run, bismark2report requires the user to specify the relevant reports as input files. Many thanks
to Phil Ewels (@tallphil) for the conceptual design and his help with this report!

### bismark2bedGraph

Added a check to see whether input files start with CpG\_\* or not. If they don't, please include the
option '--CX' to work properly. This is only relevant when bismark2bedGraph is run as a stand-alone
tool.

## RELEASE NOTES FOR Bismark v0.8.3 (26 Jul 2013)

### Bismark

In paired-end SAM mode, Bismark deliberately used to set somewhat unconventional FLAG values
due to the weird nature of bisulfite reads. In addition, the read IDs for read 1 and read 2 had
'/1' and '/2' appended, respectively, to make it easier to spot the reads relative to the input file.
Since both the appended read IDs and custom FLAG values may cause problems with some widely used
downstream tools such as Picard, new defaults were implemented as of version 0.8.3. The former
custom FLAG values and /1 /2 read IDs are still available for via the option '--old_flag' for some
time (however this option might disappear entirely in future versions.

                           default                          old_flag
                      ===================              ===================
                      Read 1       Read 2              Read 1       Read 2

             OT:         99          147                  67          131

             OB:         83          163                 115          179

             CTOT:       99          147                  67          131

             CTOB:       83          163                 115          179

Thanks to Peter Hickey, Australia, for bringing this to my attention and for contributing a patch.

### Methylation extractor

Implemented two quick tests for paired-end SAM/BAM files to see if the file had been sorted by
chromosomal position prior to using the methylation extractor. This would cause problems with
the strand identity and overlaps since both reads 1 and read 2 are expected to follow each other
directly in the Bismark alignment file. The first test attempts to find the @SO (for sorted) tag
in the SAM header. If this cannot be found, the first 100000 sequences are checked for whether
or not their ID is the same (/1 and /2 which were appended in previous versions of Bismark will
be removed prior to testing for equality). If the file appears to have been sorted, the methylation
extractor will bail and ask for an unsorted file.

Changed the additional check for the module GD::Graph::colour to an 'eval {require ...}' statement
instead of using 'use' (which might still fail at compile time); if it can't be found on the
system drawing of the M-bias plot will be skipped.

## RELEASE NOTES FOR Bismark v0.8.2 (24 Jul 2013)

### Bismark

Changed the values of the TLEN values in paired-end SAM format generated by Bowtie 2 whenever
one read was completely contained within the other, like this:

Read 1 ---------------------->
Read 2 <---------------------

In these cases both TLEN values will be set to the length of the longer fragment, here Read 1.
Read 1 will receive a positive value if Read 1 was the longest fragment, and Read 2 will be
negative, or vice versa round if Read 2 was longer.

Changed the output filename for Bowtie 2 files for single-end reads from '...bt2_bismark.sam'
to '...bismark_bt2.sam' so that single-end and paired-end file names are more consistent.

### Methylation extractor

Added a new option '--mbias_only'. If this option is specified, the M-bias plot(s) and their
data are being written out. The standard methylation report ('--report') is optional. Since
this option will not extract any methylation data, neither bedGraph nor cytosine report conversion
are not allowed.

When a specific output directory and '--cytosine_report' are specified at the same time, the
bedGraph2cytosine module will now use the bedGraph file located in the output directory as
intended.

Added an additional check for the module GD::Graph::colour; if it can't be found on the system
drawing of the M-bias plot will be skipped.

## RELEASE NOTES FOR Bismark v0.8.1 (16 Jul 2013)

### Bismark

Changed the way in which the alignment overview file is being named to not generate a warning
message.

### Methylation extractor

Changed the function of '--ignore <int>' to ignore the first <int> bp from the 5' end of
single-end reads or Read 1 of paired-end files. In addition, added a new option '--ignore_r2 <int>'
to ignore the first <int> bp from the 5' end of Read 2 of paired-end files. Since the first
couple of bases in Read 2 of BS-Seq experiments show a severe bias towards non-methylation
as a result of the end-repair of sonicated fragments with unmethylated cytosines (see M-bias plot),
it is recommended that the the first couple of bp of Read 2 are removed before starting downstream
analysis. Please see the section on M-bias plots in the Bismark User Guide for more details.

Changed colours, legends and background colour of the M-bias plot, but I think Simon
is still not happy...

## RELEASE NOTES FOR Bismark v0.8.0 (12 Jul 2013)

### Bismark

Added new option '--prefix <prefix>' to add <prefix> to the output filenames. Trailing
dots will be replaced by a single one. For example. '--prefix test' with 'file.fq'
would result in the output file 'test.file.fq_bismark.sam' etc.

Fixed a warning message that occurred when chromosomal sequences could not be extracted
in paired-end Bowtie2 mode.

Bismark will now generate a pie chart with the alignment statistics when a run has finished;
this allows to get a quick overview of how many sequences aligned uniquely, sequences that
did not align, either due to producing no alignment at all, multiple mapping or because it
was impossible to extract the chromosomal sequence. Drawing this plot will require the Perl
module GD::Graph; if it is not found on the system the plot will be skipped.

### Methylation extractor

Upon completion, the methylation extractor will now produce an M-bias (methylation bias)
plot, which shows the methylation proportion across each possible read position (described
in: Hansen et al., Genome Biology, 2012, 13:R83). The data for the M-bias plot will be
written into a text file (to generate graphs by alternative means) and drawn into a .png
plot which requires the Perl module GD::Graph; if GD::Graph cannot be found on the system,
only the table will be printed. The plot also contains the absolute number of methylation
calls per position.

## RELEASE NOTES FOR Bismark v0.7.12 (10 May 2013)

### Bismark

Removed a rogue sleep(1) command that would slow down single-end Bowtie 2 alignments
for a single lane of HiSeq (200M sequences) from ~1 day to 6 years and 4 months (roughly).

### bismark2bedGraph

bismark2bedGraph now keeps track of the temp files it just created instead of using
all files in the output folder ending in ".methXtractor.temp". This lets you kick off
the bedGraph conversion step from already sorted, individual methXtractor.temp files if
desired.

## RELEASE NOTES FOR Bismark v0.7.11 (22 Apr 2013)

### Bismark

Fixed non-functional single-end alignments with Bowtie2 which were accidentally broken
by introducing the option '--pbat' in v0.7.10 (an evil 'if' instead of 'elsif'...).

For paired-end alignments with Bowtie 1, the option '--non_bs_mm' would accidentally
confuse the number of mismatches of read 1 and read 2 whenever the first read aligned
in reverse orientation, i.e. for OB and CTOT alignments. This has now been corrected.

Previously, the option '--non_bs_mm' would potentially output non-integer values for
Bowtie 2 alignments if the read (or reference) contained 'N' characters. Alignment
scores from 'N's are now adjusted so that they count as mismatches similar to what
Bowtie 1 does. This works for fine reads with up to and including 5 N's (which is quite
a lot...).

### Methylation extractor

To avoid duplication and keep code modular, the bedGraph conversion step invoked by
the option '--bedGraph' is now been farmed out to the module 'bismark2bedGraph'. This
script is independent of the methylation extractor and also works as a stand-alone tool
from the methylation extractor output (compressed or gzip compressed files). To work
well from within the methylation extractor this script (which is now included in the
Bismark package) needs to reside in the same folder as the 'bismark_methylation_extractor'
itself.

bismark2bedGraph

Temporary chromosome files now have an input file name included in their file name to
enable parallel processing of several files in the same directory at the same time.

To avoid duplication and keep code modular, the bedGraph to genome-wide cytosine methylation
report step invoked by the option '--cytosine_report' has now been split out to the
module 'bedGraph2cytosine'. This script is independent of the methylation extractor and
also works as a stand-alone tool from the Bismark bedGraph '--counts' output (compressed
or gzip compressed files). To work well from within the methylation extractor this
script (which is now included in the Bismark package) needs to reside in the same folder
as the 'bismark_methylation_extractor' itself.

### Deduplication script

Fixed some warnings that were thrown if '--bam' was not specified.

## RELEASE NOTES FOR Bismark v0.7.10 (18 Apr 2013)

### Bismark

Added new option '--gzip' that causes temporary bisulfite conversion files to be
written out in a GZIP compressed form in order to save disk space. This option is
available for most alignment modes but is not available for paired-end FastA files
(not many people use PE FastA files...). This option might be somewhat slower than
writing out uncompressed files, but this awaits further testing.

Added new option '--bam' that causes the output file to be written out in BAM format
instead of the default SAM format. Bismark will attempt to use the path to Samtools
that was specified with '--samtools_path', or, if it hasn't been specified explicitly,
attempt to find Samtools in the PATH. If no installation of Samtools can be found
the SAM output will be compressed with GZIP instead (yielding a .sam.gz output file).

Added new option '--samtools_path' to point Bismark to your Samtools installation, e.g.
/home/user/samtools/. Does not need to be specified explicitly if Samtools is in the
PATH.

Added new option '--pbat' which may be used for PBAT-Seq libraries (Post-Bisulfite
Adapter Tagging; Kobayashi et al., PLoS Genetics, 2012). This is essentially the exact
opposite of alignments in 'directional' mode, as it will only launch two alignment
threads to the CTOT and CTOB strands instead of the normal OT and OB ones. The option
--pbat works only for single-end and paired-end FastQ files for use with Bowtie1 (and
uncompressed temporary files only).

### Methylation extractor

The methylation extractor does now also read BAM files, however this requires a working
copy of Samtools. For this we added the new option '--samtools_path' to point the Bismark
methylation extractor to your Samtools installation, e.g. /home/user/samtools/. This does
not need to be specified explicitly if Samtools is in the PATH.

Added new option '--gzip' to write out the primary methylation extractor files (CpG*OT*...,
CpG*OB*... etc) in a GZIP compressed form to save disk space. This option does not work
on bedGraph and genome-wide cytosine reports as they are 'tiny' anyway.

The methylation extractor does now treat InDel free reads differently than before which
results in a ~60% increase in extraction speed for ungapped alignments in SAM format!

### Deduplication script

The deduplication script does now also read BAM files, however this requires a working
copy of Samtools. The new option '--samtools_path' may point the script to your Samtools
installation, e.g. /home/user/samtools/. This does not need to be specified explicitly
if Samtools is in the PATH.

The deduplication script also received the new option '--bam' to write out deduplicated
files directly in BAM format. If no installation of Samtools can be found the SAM output
will be compressed with GZIP instead (yielding a .sam.gz output file).

## RELEASE NOTES FOR Bismark v0.7.9 (05 Mar 2013)

### Methylation extractor

The new function '--buffer_size <string>' for the bedGraph sort command is set to the
new default value of 2G (sort would die if this option was not set).

The replacement of pipe ('|') characters in the name of reference chromosomes for the
bedGraph sorting step should now work as expected.

If multiple files are to be processed for genome-wide methylation reports in a single
command, the reference genome is only read in once (instead of stopping due to the
presence of several chromosomes with the same name...).

## RELEASE NOTES FOR Bismark v0.7.8 (01 Mar 2013)

### Bismark

Added an option '--non_bs_mm' which prints an extra column at the end of SAM files
showing the number of non-nisulfite mismatches of a read. This option is not available
for '--vanilla' format.

Format for single-end reads: "XA:Z:mismatches"

Format for paired-end reads: read 1: "XA:Z:mismatches"
read 2: "XB:Z:mismatches"

If Bowtie 2 was used for alignments, the alignment score and CIGAR strings are processed
to exclude potential insertions or deletions before the number of non-bisulfite mismatches
is determined.

The mapping report file names were changed to _bismark_(SE/PE)_report.txt (Bowtie 1) or
bt2_bismark_(SE/PE)\_report.txt (Bowtie 2) to keep it more uniform.

### Methylation extractor

The input file(s) may now be specified with a file path which abrogates the need to be in
the same directory as the input file(s) when calling the methylation extractor.

Reference sequence files containing pipe ('|') characters were found to crash the methylation
extractor as the chromosome name was used for filenames. These characters are now replaced
with underscores '\_' when the reads are sorted during the bedGraph step.

Added a new funtion '--buffer_size <string>' for the bedGraph sort command to allow using
more memory than the default (1024K). Allowed values are either a percentage of physical
memory (e.g. 50%) or a number along with a multiplier in bytes (e.g. 500M, 50G etc).

Updated the Bismark User Guide with sections for the bedGraph and genome-wide methylation
report outputs and Appendix IV is now showing alignment stats for the test data.

## RELEASE NOTES FOR Bismark v0.7.7 (02 Oct 2012)

### Bismark

When reading in the genome file Bismark does now automatically remove \r line ending
characters as well. This sometimes caused problems when genome files had been edited
on Windows machines.

Added support for the Bowtie 2 options '--rdg <int1>,<int2>' and '--rfg <int1>,<int2>'
to adjust the gap open and extension penalties for both read and reference sequence.
This might be useful for very special conditions (e.g. PacBio data...)

### Bismark Methylation extractor

Added new function '-o/--output' to specify an output directory. This became necessary for
better integration into Galaxy.

Added new function '--no_header' to suppress the Bismark version header in the output files
if plain alignment data is more desirable.

Renamed methylation extractor to bismark_methylation_extractor.

bedGraph output:

Added option '--bedGraph' to produce a bedGraph output file once the methylation extraction
has finished; this reports the genomic location of a cytosine and its methylation state (in %).
This basically implements the finctionality of the script:
genome_methylation_bismark2bedGraph.pl directly into the mbismark_methylation_extractor.
The bedGraph output file is sorted by chromosomal positions. By default, only cytosines in
CpG context will be sorted. The option '--CX_context' may be used to report all cyosines
irrespective of sequence context (however this will take MUCH longer!).

Implemented option '--cutoff threshold' to set the minimum number of times a methylation state
has to be seen for that nucleotide before its methylation percentage is reported.

Implemented option '--counts' which adds two additional columns to the bedGraph output file
to enable further calculations:

                     col 5: number of methylated calls
                     col 6: number of unmethylated calls

Implemented option '--CX_context' so that the sorted bedGraph output file contains information
on every single cytosine that was covered in the experiment irrespective of its sequence context.

Genome-wide cytosine methylation report output:

Added option '--cytosine_report' which produces a genome-wide methylation report for all cytosines
in the genome. By default, the output uses 1-based chromosome coordinates (zero-based cords are
optional) and reports CpG context only (all cytosine context is optional). The output considers
all Cs on both forward and reverse strands and reports their position, strand, trinucleotide
content and methylation state (counts are 0 if not covered).

Option '--CX_context' applies to the cytosine report as well. The output file wil contain information
on every single cytosine in the genome irrespective of its context. This applies to both forward
and reverse strands.

Implemented option '--zero_based' to use zero-based coordinates like used in e.g. bed files
instead of 1-based coordinates.

Implemented option '--genome_folder <FULL PATH>' to be used to extract sequences from. Accepted
formats are FastA files ending with '.fa' or '.fasta'.

Added an option '--split_by_chromosome' which writes the cytosine report output to individual
chromosome files instead of to one single large file.

UPDATE for genome_methylation2bedGraph script (23 Aug 2012)

Added an option '--split_by_chromosome' to enable sorting of very large files. The methylation
extractor output is first written into temporary files chromosome by chromosome. These
files are then sorted by position and deleted afterwards.

Added an option '--counts' which adds 2 more lines to the bedGraph output file:

      Column 5: count of methylated calls per position, and
      Column 6: count of unmethylated calls per position.

Technically, this renders the output to be no longer in bedGraph format, but it might enable
additional calculations with the output.

## RELEASE NOTES FOR Bismark v0.7.6 (31 Jul 2012)

Methylation extractor

Changed the way in which the methylation extractor identifies the read and genome
conversion flags in SAM output. This might become relevant if the Bismark SAM mapping output
was compressed/decompressed with CRAM or Goby at some point, since these tools may change
the order of the tags in a SAM entry. Thanks to Zachary Zeno for pointing this out and
contributing a patch.

Reworked the way in which SAM files (both single and paired-end) are handled in the
methylation extractor so that reads containing InDels, which may be generated by Bismark
using Bowtie 2, are now handled as intended. Insertions or deletion resulted in small
positional shifts of methylation calls.

## RELEASE NOTES FOR Bismark v0.7.5 (16 Jul 2012)

Bismark

Trailing read ID segment number (e.g. /1,/2 or /3) are now removed internally for Bowtie 2
alignments in paired-end mode as this might have caused no reads to align at all if the
segment number was not 1 or 2. As of Bowtie 2 version 2.0.0-beta7 this behavior has been
disabled for unpaired reads.

The Bowtie 2 option -M is now deprecated (as of Bowtie 2 version 2.0.0-beta7). What used
to be called -M mode is still the default mode, but adjusting the -M setting is deprecated.
The options -D and -R should be used to adjust the effort expended to find valid alignments.
The help text for the -M mode is still being displayed but may be removed in a subsequent
release.

Changed the default seed mismatch parameter (controlled by -n) to 1 (down from 2). This
increases alignment speed noticably and typically produces very similar results for good
quality read data.

Fixed a bug where the chromosomal sequence could not be extracted for very short genomic
sequences for alignments with Bowtie 2.

Methylation extractor

Does now read both raw and gzipped (.gz) Bismark mapping files.

Deduplication script

Does now read both raw and gzipped (.gz) Bismark mapping files.

## RELEASE NOTES FOR Bismark v0.7.4 (26 Apr 2012)

Bismark

Introduced a new option '--temp <dir>' to which the C-to-T or G-to-A transcribed temporary
files can be written to instead of into the same folder that contains the input files. If the
specified folder does not exist Bismark will try to create it first.

The input files to be aligned may now contain path information, e.g. /home/user/file.fq or
../temp/file.fq, and one no longer has to call Bismark from within the directory containing
the input files.

## RELEASE NOTES FOR Bismark v0.7.3 (05 Apr 2012)

Bismark

Corrected a bug for the TLEN field in paired-end SAM output. This value was occasionally
calculated incorrectly if both reads were overlapping almost entirely with a difference
of only 1 bp between the end of one read and the start of the second read.

Removed a potential source of crashes with gzipped input files and the option -u/--qupto.

Methylation Extractor

Corrected a potential flaw for the 'remove overlap' option for paired-end alignments in --vanilla
mode when the first read aligned in a reverse orientation. Everything is now working as intended.

Output files will now only have a single .txt file extension if the Bismark results file was
already ending in .txt.

## RELEASE NOTES FOR Bismark v0.7.2 (14 Mar 2012)

methylation_extractor

Changed the file ending for all files generated by the methylation extractor to .txt. This is
to avoid confusing these files with SAM formatted Bismark output files.

## RELEASE NOTES FOR Bismark v0.7.1 (29 Feb 2012)

Bismark

Adjusted Bismark so that white spaces or tab characters in the read IDs get replaced on the fly
with underscore characters. This step is necessary because some read ID checks would cause Bismark
to fail. In particular, Bowtie2 truncates read IDs if it encounters white space characters in the
read ID (probably consistent with the notion that everything after the first word in the read ID
is an optional description). Since IDs generated by the latest version of the Illumina RTA always
contain spaces, these input files would not work. In contrast, Bowtie 1 doesn't mind 'simple space'
characters but truncates read IDs if a 'tab' characters was encountered. As a solution that does
not truncate all read IDs to the word before the first white space (e.g. the example on the FastQ
article on Wikipedia:

@SRR001666.1 071112_SLXA-EAS1_s_7:5:1:817:345 length=36

would have every single read ID truncated to:

SRR001666.1

which I don't find particularly helpful), we decided to replace all white spaces with underscores.
This should work equally well for both Bowtie 1 and Bowtie 2.

RRBS User Guide and trim_galore

This package contains a brief guide to quality control aspects of RRBS experiments (some of
which are also relevant for standard shotgun libraries). It also contains the trim_galore script
which wraps around Cutadapt to perform quality and/or adapter trimming as well as some
trimming steps to remove cytosines with an experimentally introduced methylation state (this is
specific to RRBS-type experiments). Also included is a validate_paired_end_script that reads in
two paired-end files at the same time and removes read pairs for which one (or both) reads are
shorter than a certain length (25bp by default).

## RELEASE NOTES FOR Bismark v0.7.0 (24 Feb 2012)

Bismark

As a result of several requests we changed Bismark's behavior for "--directional" mode
(which is on by default) to run only 2 parallel instances of Bowtie 1/2 to the original top
(OT) and bottom (OB) strands, instead of 4 to all possible bisulfite strands. This change
might result in faster alignment times (because reads from directional libraries don't
typically align very well to the complementary strands) and possibly a somewhat increased
mapping efficiency for directional libraries (because less reads get booted due to ambiguous
mapping to the theoretical complementary strands).

It is still possible to run the 4-alignment strand mode for any combination of input file(s)
and choice of aligner by specifying --non_directional. If one wants to get an idea of the
number of mismapping events one could run Bismark in --non_directional mode, look at the
number of alignments to the CTOT or CTOB strands and then ignore the output to these
complementary strands via the methylation extractor.

Changed the --score_min default function for Bowtie 2 alignments to a more stringent setting of
"L,0,-0.2" instead of using the Bowtie2 default function (which was "L,0,-0.6"). Thanks to E.
Harris for this suggestion.

## RELEASE NOTES FOR Bismark v0.6.4 (06 Feb 2012)

Bismark

For paired-end mode, the options --unmapped and --ambiguous do now output unaligned or
multiply aligned reads, respectively, to their correct output files as intended.

Adjusted the options -u and -s so that only the non-skipped part of the input file will
be transcribed and analysed. This allows splitting up very large files into smaller chunks to
allow parallel processing, e.g -s 10000000 -u 20000000 would analyse sequences 10000001 to 20000000. The alignment report will be based on this reduced number of reads analysed.

Sequences in FastA format do now receive Phred score qualities of 40 throughout (ASCII 'I')
to prevent the SAM to BAM conversion in SAMtools from failing.

If a genomic sequence could not be extracted it will now also be counted and reported for
use with Bowtie 1.

Suppressed debugging warning meassages that were printed in error for Bowtie2 alignments
(single-end only).

## RELEASE NOTES FOR Bismark v0.6.3 (04 Jan 2012)

Bismark

Changed the XX:Z mismatch field in the SAM output to display mismatching nucleotides
in the reference sequence (instead of in the read sequence).

Fixed a bug that occured when a read was called '0'.

Methylation Extractor

The methylation extractor does now also accept Bismark output files in SAM format. Please
type "methylation_extractor --help" or refer to the Bismark User Guide for more information.

## RELEASE NOTES FOR Bismark v0.6.beta2 (15 Dec 2011)

Bismark

Added a paralleliztion option for Bowtie 2 alignments with the option '-p'. Please note that
this is only recommended if your system resources are plentyful. Please bear in mind that
specifying for instance '-p 2' will already use 8 threads/cores for Bowtie2 plus 1 additional
core for Bismark itself, and thereby consume more than 15GB of memory!

This parallelization switch in Bismark also uses the Bowtie 2 option '--reorder', and thus
requires at least Bowtie 2 version 2.0.0-beta5 or higher (released on Dec 15, 2011).

## RELEASE NOTES FOR Bismark v0.6.beta1 (08 Dec 2011)

Bismark_genome_preparation

Added option '--bowtie2' to create Bowtie 2 indexes. Please note that Bowtie 1 and Bowtie 2
indexes are not compatible. Bowtie 2 indexes can be safely written into the same folder
already containing the Bowtie 1 indexes.

Bismark

Please make sure that BS-Seq data is checked for adapter contamination and/or poor quality
base calls and appropriately trimmed before alignments are carried out.

Bowtie 1 mode (default)

- Changed the default output format to SAM. Unaligned reads or ambiguous alignments are not
  reported into the output file. These can be written out to \_unaligned_read or \_ambiguous_read
  files in FastQ format if desired as usual. The 'old' Bismark output can still be obtained by
  specifying '--vanilla'.

- Alignments are now in the former '--directional' mode by default, i.e. only alignments to
  the original top (OT) and original bottom (OB) strand will be reported. The full 4-strand
  output for non-directional libraries can be re-enabled by specifying '--non_directional'.

- Alignments are now run with the Bowtie parameters '--norc' or '--nofw' where appropriate.
  This may result in a slightly increased mapping efficiency as less reads are removed due to
  non-sensical alignments.

- The default value for paired-end maximum insert size (-X/--maxins) was increased to 500bp
  (up from 250bp).

Bowtie 2 mode (optional by specifying '--bowtie2')

- The options used for running Bowtie 2 are still to be considered experimental until we
  manage to figure out the most sensible way. Currently, these parameters are freely adjustable:

-M <int> (reporting the best out of <int> valid alignments)
-N <int> (multi-seed mismatches)
-L <int> (seed length)
-D <int> (maximum number of seed extension fail tries)
-R <int> (reseeding of repetitive alignments)
--score-min <func> (setting minimum alignment score for valid alignments)

For correct and reliable methylation calls it is essential that bisulfite reads are aligned
correctly without tolerating too many mismatches. Therefore, when using Bowtie 2 the option
'--ignore-quals' is always on, meaning that a mismatch between the read and the genome will
always receive a penalty of '6', and this penalty is not reduced by a low basecall quality.
As mentioned above, poor quality data should be quality-trimmed before carrying out sequence
alignments.

- Paired-end alignments in Bowtie 2 are carried out using the options '--no-mixed' and
  '--no-discordant'. This means that Bowtie 2 only looks for concordant paired-end alignments
  and does not automatically look for discordant alignments or single-end alignments in case it
  can't find any concordant paired-end alignments. The latter might change in a future release.

Relevant for both modes

- added option '--no_sam_header', so that output files start with alignments straight away.
  This might be useful when large input files are split up into several smaller files and the
  output files are to be merged.

bismark2SAM.pl

The bismark2SAM conversion script (version 6) does now reverse the quality and methylation
call strings if a sequence was reverse complemented for SAM output.

## RELEASE NOTES FOR Bismark v0.5.4 (17 Oct 2011)

Bismark

Bismark will now accept input files in either normal, uncompressed or gzipped format
(files have to ending in .gz).

Added the option -o/--output_dir <dir> to Bismark which lets you specify the folder
for all Bismark output files instead of writing into the same folder as the input file(s).
If the output directory does not exist already it will be created.

The path to the genome folder can now be absolute or relative (e.g. ../genomes/mouse/).

Changed the way unmapped or ambiguous reads are reported so that one output file (and/or
ambiguous read file) is generated per input file. Their name will be derived from the
input file name. For paired-end samples, the unmapped or ambiguous filenames can be
discriminated by \_1 and \_2 in their file names.

Added the number of sequences analysed in total to the paired-end report file (was only
printed on screen previously).

Fixed a bug for the FastQ output for ambiguous reads where quality scores were not followed
by a new line.

## RELEASE NOTES FOR Bismark v0.5.3 (13 Sep 2011)

Bismark

The '--chunkmbs' default parameter was increased to 512 MB (up from 256 MB) to avoid
memory exhaustion warnings.

Corrected a mix-up of the strand origin names in the printed final alignment report:
GA/CT is now correctly labelled as CTOT (complementary to (converted) top strand), and
GA/GA is now correctly labelled as CTOB (complementary to (converted) bottom strand).

genome_methylation_bismark2bedGraph_v3.pl

This new version fixes a bug in the methylation percentage which was introduced by
implementing 0-based (bedGraph) coordinates. Many thanks to Michael A. Bentley for
spotting this issue and for contributing this new version.

bismark2SAM_v5.pl

This new version of the bismark2SAM conversion script introduces adjusted bitwise FLAG
values for non-directional single-end and for paired-end alignments. This is to better
reflect the strand origin of a read or a read pair. E.g., alignments to the OT strand
are always found in '+' orientation, whereas alignments to the CTOT strand are always found
in a '-' orientation. Both these alignments will now get a FLAG value of '0' indicating that
the read originated from the original top strand. A similar logic is also applied for
alignments to other strands and for paired-end alignments. Thanks to Enrique Vidal for
bringing this to my attention and for his contributions to this new version.

## RELEASE NOTES FOR Bismark v0.5.2 (16 Aug 2011)

Bismark

The '--chunkmbs' default parameter was increased to 256 MB (up from 64 MB) to avoid
memory exhaustion warnings.

Bismark will now accept single-end file names in both comma- and space-separated
format.

Sequence files with sequence ID names containing tab stops are truncated by Bowtie
to the first element, which results in no sequence alignments. Bismark will detect
whether the seqID field contains tab characters and print a warning at the end of
the run (also into the log file). Simply replacing tabs with whitespaces in the
seqID lines of the input file fixes this problem (e.g. with $id =~ s/\t/ /g; ),
however this needs to be handled before invoking Bismark.

Methylation Extractor

Fixed a bug which resulted in offset methylation positions (by the read length)
for single-end reverse strand alignments when the option '--ignore' was specified.

## RELEASE NOTES FOR Bismark v0.5.1 (16 June 2011)

Bismark_genome_preparation:

The genome folder can now be specified either as absolute or relative path.

Bismark

Fixed a bug where a newline character was missing after the quality value in
the FastQ unmapped read output.

Fixed a bug which prevented paired-end read alignments in FastA format.

Methylation Extractor

The input file(s) can now also be specified with a relative path. The output
files will be written into the current working directory.

## RELEASE NOTES FOR Bismark v0.5.0 (21 Apr 2011)

Bismark

Due to upcoming changes in the Illumina Casava 1.8 pipeline the FastQ output format
for paired-end files will look different to earlier versions. If run in paired-end
mode, Bismark is now always appending /1 to all reads from the file specified by -1,
/2 to all reads from the file specified by -2 while the sequences are converted into
bisulfite sequences. This should ensure that pretty much any format will be aligned
correctly.

The Bismark single-end mapping output will now have an additional column at the end
showing the basecall quality scores of the FastQ file to allow for quality filtering.
The field is left blank for FastA input.

The Bismark paired-end mapping output will now have two additional columns at the end
showing the basecall quality scores of the FastQ files for read 1 and read 2 to allow
for quality filtering. Both fields are left blank for FastA input.

Fixed a bug for paired-end alignments whereby alignments to the CTOT strand were
incorrectly assigned to the CTOB strand and vice versa.

Methylation Extractor

Fixed a bug for paired-end alignments whereby alignments to the CTOT strand were
incorrectly assigned to the CTOB strand and vice versa.

## RELEASE NOTES FOR Bismark v0.4.1 (10 Feb 2011)

Bismark_genome_preparation:

The Bismark genome preparation will now write both bisulfite converted versions
of the genome to one multi-FastA file (MFA) file per genome by default. This allows
indexing of newly assembled genomes with several thousands of chromosomes (and/or
contigs or scaffolds). Trying to index genomes with 20,000-50,000 chromosomes
previously exceeded the operating system limit of concatenting chromosome names into
a list. Chromosomes can still be converted and written out individually by selecting
the option --single.

Bismark

Changed the way paired-end alignments are reported internally slightly. This means
that sequences which produce two alignments to the exact same position in different
alignment processes will now be preferentially assigned to the original top (OT)
and orginal bottom (OB) strands, as intended. Previously, alignments were prefer-
entially assigned to the CTOB strand before the OB strand. This was only relevant for
alignments for which sequences did either not contain a single C or G, or if sequences
showed a 100% protection, i.e. methylation, of all Cs and if --directional was selected.

## RELEASE NOTES FOR Bismark v0.4.0 (04 Feb 2011)

The option '--directional' is now also working for paired-end alignments. If
the BS-Seq library was generated in a strand-specific way (i.e. only the
(bisuflite converted versions) of the top and bottom strand are being sequenced),
best alignments to the strands complementary to either the original top or bottom
strands will be ignored. It is recommended to use this option for directional
paired-end libraries.

Fixed a strand-identity mix-up which was printed in the mapping summary report
of paired-end alignmnents (this only concerned the report itself, alignments as
such or the output of the methylation extractor were not affected).

## RELEASE NOTES FOR Bismark v0.3.0 (26 Jan 2011)

The Bismark documentation received a complete overhaul. The Bismark User Guide
replaces the previous documentation (INSTALL.txt and README.txt). It contains
a Quick reference to get started quickly witout having to read the entire User
Guide. In addition to that it also contains detailed information about BS-Seq
in general, about how Bismark works, its output formats and discusses some of the
available options. The Appendix section does now include all available options
of all three scripts of the Bismark package (bismark_genome_preparation, bismark
and methylation_extractor).

We do now offer a BS-Seq test data set for download so that users can try Bismark
out. The test data set consists of 10,000 sequences taken from a human shotgun
BS-Seq dataset (SRR020138, Lister et al. 2009). The sequences have been trimmed
to 50 bp and its base call qualities are Phred33 encoded.

Both the bismark_genome_preparation and bismark itself will now accept reference
genome sequence files (in FastA format) with the file extension .fasta in addition
to .fa.

## RELEASE NOTES FOR Bismark v0.2.6 (18 Jan 2011)

Fixed a bug which might occur if the alignment parameters are set very laxly.
This only affected alignments if 10 or more non-bisulfite mismatches are
tolerated (Please note that we would absolutely not recommend allowing that
many mismatches for BS-Seq!!).

## RELEASE NOTES FOR Bismark v0.2.5 (22 Dec 2010)

Added the new option --un <filename> to Bismark which will write out all
reads failing to align uniquely to <filename> in the same format they were
provided (FastQ or FastA). This will inlcude both reads that do not
produce any alignments and reads which are being rejected due to ambiguous
mapping unless --ambiguous <filename> has been specified as well (see below).

Added the new option --ambiguous <filename> to Bismark which will write out
all reads that are being rejected due to ambiguous mapping. Reads which are
reported by --ambiguous will not appear in the output of --un <filename>.

## RELEASE NOTES FOR Bismark v0.2.4 (18 Nov 2010)

Bismark

Added the new options -I/--minins <int> and -X/--maxins <int> to Bismark to
allow the specification of minimum and maximum insert sizes for paired-end
alignments.

Bismark_genome_preparation

Changed the remove_tree command in the File::Path core module rm_tree in the
same module instead, as some older versions of Perl would throw an error
otherwise.

## RELEASE NOTES FOR Bismark v0.2.3 (04 Nov 2010)

Added the new option --directional to Bismark. If the BS-Seq library was
constructed in a strand-specific way one would expect to see only sequences
corresponding to the (C -> T converted) original top or bottom strands. The
two strands which are complementary to the original strands are - in this case

- only theoretical and should not be observable in the sequencing experiment.
  Specifying --directional will reject alignments to these in silico existing
  strands and will generate a small report about rejected sequences after the
  Bismark run has been completed.

Changed the default alignment option of Bismark to --best to ensure the most
credible alignment results. This can be turned off by specifying --no_best.
Disabling --best can speed up the alignment process (good for testing purposes),
but this will increase the risk of mismappings at the same time.

Added the option -e/--maqerr <int> so that one can play around with the maximum
number of tolerated mismatches if this is desired/required at some point.

The output files generated by Bismark will now end in '\_bismark.txt' for single-
end files or '\_bismark_pe.txt' for paired-end files. The mapping and splitting
reports will also end in .txt.

The alignment and methylation summary reports have been slightly modified to
allow better readability.

## RELEASE NOTES FOR Bismark v0.2.2 (13 Sep 2010)

Fixed a bug in the methylation extactor that would offset a subset of
reverse mapped reads by a couple of bases. Positions should now be correct.

## RELEASE NOTES FOR Bismark v0.2.1 (08 Sep 2010)

Bismark will now handle multi-fasta-files as intended.

## RELEASE NOTES FOR Bismark v0.2.0 (07 Sep 2010)

Bismark

Non-CpG context is now subdivided into cytosines in CHG and CHH context,
whereby H can be either A, T or C. This change also means that the genomic
sequence a bisulfite read is compared against must be 2 bp longer than the
actual read itself; this genomic sequence is also reported in the Bismark
mapping results file. Cytosines in non-CpG context received the following
new characters in the methylation call string to avoid confusion with older
Bismark files:

CHG-context: X / x (methylated / unmethylated)
CHH-context: H / h (methylated / unmethylated)

Due to recent changes in the Bowtie source code, Bismark produced lots
of warnings ('chunk memory exhausted...'). To counteract this problem
Bismark will now understand the additional option '--chunkmbs <int>' (to
increase the memory allocation for individual alignments from 64 (default)
to any integer). These errors were especially frequent in --best mode or for
paired-end alignments. Bismark will also understand the '--quiet' option
to suppress memory chunk exhaustion (and other) warnings.

FastA files do no longer require the file ending ".fa".

Fixed an issues so that Bismark will no longer tolerate chromosomes with
same name when reading the genome into memory.

Methylation Extractor

The methylation extractor will by default distinguish between cytosines in
the three contexts CpG, CHG or CHH. If this is not needed, CHG and CHH context
can be merged into 'non-CpG' context by specifying '--merge_non_CpG'.

Due to the fundamental changes in v0.2.0 (CHG and CHH context methylation
calls) the methylation extractor will require that the Bismark mapping result
file was generated with the same Bismark version (the Bismark version is
contained within the first line of the mapping result file).

## RELEASE NOTES FOR Bismark v0.1.5 (09 Aug 2010)

Fixed a bug where specifying "-n 0" as alignment parameter would not
be executed properly.

## RELEASE NOTES FOR Bismark v0.1.4 (06 Aug 2010)

The Bismark alignment process would previously grind to a halt when it
encountered DNA ambiguity bases in the reference genome sequence (R,M...)
while trying to determine the sequence context of (un-)methylated Cs.
This behaviour has now been changed so that everything else than a C in
CpG context was will now be treated as C in not-CpG context.

Fixed a bug whereby the single-end strand-specific output got two of
the four possible strands mixed up (fixed properly this time).

## RELEASE NOTES FOR Bismark v0.1.3 (03 Aug 2010)

Bismark Genome Preparation

If the specified genome directory does already contain a bisulfite
genome folder, all contents of this directory will be removed before
creating and indexing a new bisulfite genome. Make sure that this
directory does not contain any other data.

The genome indexer will now convert DNA ambiguity code into N's before
making the bisulfite genomes (anything else than C, A, T or G will appear
as N afterwards).

The indexer will now also handle fastA files with mutltiple sequence
entries in addition to (a list of) fastA files in the specified genome
folder. (Please note that bowtie-build only accepts a few hundred
individual files (or 'chromosomes') for indexing. If you want to index
more sequences than that they need to be merged in some way).

Methylation Extractor

Fixed a bug whereby the single-end strand-specific output got two of
the four possible strands mixed up. Also, the --ignore <int> option did
previously offset some of the positions of the methylation calls by the
<int> specified. Both options should now work as intended.

For paired-end alignments with rather short fragment lengths it is
theoretically possible to read stretches of overlapping sequence with
both read 1 and read 2. In order not to score the methylation calls for
overlapping sequences twice, we added an option (--no_overlap) to score
overlapping methylation calls only from the first read of a given alignment.

It is a somewhat icky decision to not use the full information of both
reads, as on the one hand it is good to get as much methylation call
information as possible, on the other hand cytosines in the middle of
paired-end fragments might get considerably more methylation calls than
more distal ones. Please note that we are at this stage not comparing or
evaluating the methylation calls from both reads (even though this is
entirely possible) but rather just use the calls from one read.

## RELEASE NOTES FOR Bismark v0.1.2 (17 Jun 2010)

The Bismark output files for single-end and paired-end reads have been
modified so that they contain only vital information, thereby reducing
their file size and confusion. More details on the output format can be
found at http://seqanswers.com/wiki/Custom_Bismark_output_format or in
the README.txt.

Both Bismark and the Methylation Extractor now write out their version
info into the first line of their output files so it is easier to track
errors.

Reads aligning to the very edges of chromosomes previously produced
several error messages when trying to extract one additional bp to
determine if Cs are in CpG context. These reads (which are very few in
in the datasets tested so far) will now be excluded from the methylation
call analysis.

## RELEASE NOTES FOR Bismark v0.1.1 (15 Jun 2010)

Both the Bismark genome preparation as well as Bismark itself should
now also run with genome FASTA files that do not look like Ensembl
files (i.e. chr1.fa,chr.2.fa or similar will be fine, too). For the
moment it is still required however that the files end on '.fa' and
only one sequence entry is allowed per file. (Multiple fasta entries
per file will be supported soon).

## RELEASE NOTES FOR Bismark v0.1 (14 Jun 2010)

Bismark v0.1 is an initial beta release and as such is still a
work in progress.

We have successfully used Bismark for bisulfite mapping against
various genomes (mouse NCBIM37, human NCBI36 and GRCh37, and Yeast
SGD1.01). In our tests the code appears to be working as expected.

We have initially designed Bismark to support the kinds of analyses
we require, thus if you have some ideas or suggestions which could
be implemented please let us know.

You can report feedback or bug reports either though our bug-
reporting tool at:

www.bioinformatics.babraham.ac.uk/bugzilla/

...or directly to felix.krueger@babraham.ac.uk

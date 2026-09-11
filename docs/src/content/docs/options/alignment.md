---
title: "Alignment"
description: "A brief description of Bismark and a full list of options can also be viewed by typing:"
---

## Appendix (II): Bismark

A brief description of Bismark and a full list of options can also be viewed by typing:
`bismark --help`

#### USAGE:

```
bismark [options] --genome <genome_folder> {-1 <mates1> -2 <mates2> | <singles>}
```

#### ARGUMENTS:

- `<genome_folder>`

  The full path to the folder containing the unmodified reference genome as well as the sub folders created by `bismark prepare` (`Bisulfite_Genome/CT_conversion/` and `Bisulfite_Genome/GA_conversion/`). Bismark expects one or more `FastA` files in this folder (file extension: `.fa` or `.fasta`). The path to the genome folder can be relative or absolute. The path may also be set as `--genome_folder /path/to/genome/folder/`.

- `-1 <mates1>`

  Comma-separated list of files containing the #1 mates (filename usually includes `_1`), e.g. `flyA_1.fq`, `flyB_1.fq`). Sequences specified with this option must correspond file-for-file and read-for-read with those specified in `<mates2>`. Reads may be a mix of different lengths. Bismark will produce one mapping result and one report file per paired-end input file pair.

- `-2 <mates2>`

  Comma-separated list of files containing the #2 mates (filename usually includes `_2`), e.g. `flyA_2.fq`, `flyB_2.fq`). Sequences specified with this option must correspond file-for-file and read-for-read with those specified in `<mates1>`. Reads may be a mix of different lengths.

- `<singles>`

  A comma or space separated list of files containing the reads to be aligned (e.g. `lane1.fq`,`lane2.fq` `lane3.fq`). Reads may be a mix of different lengths. Bismark will produce one mapping result and one report file per input file.

#### OPTIONS:

##### Input:

- `--se <list>` / `--single_end <list>`

  Sets single-end mapping mode explicitly giving a list of file names as <list>. The filenames may be provided as a comma [`,`] or colon [`:`]-separated list.

- `-q` / `--fastq`

  The query input files (specified as `<mate1>`, `<mate2>` or `<singles>` are FastQ files (usually having extension `.fq` or `.fastq`). Input files may also be `gzip` compressed (ending in `.gz`). This is the default. See also `--solexa-quals` and `--integer-quals`.

- `-f` / `--fasta`

  The query input files (specified as `<mate1>`, `<mate2>` or `<singles>` are FastA files (usually having extension `.fa`, `.mfa`, `.fna` or similar). Input files may also be `gzip` compressed (ending in `.gz`). All quality values are assumed to be 40 on the Phred scale. FASTA files are expected to contain the read name and the sequence on a single line each (and not spread over several lines)

- `-s <int>` / `--skip <int>`

  Skip (i.e. do not align) the first &lt;int> reads or read pairs from the input.

- `-u <int>` / `--upto <int>`

  Only aligns the first &lt;int> reads or read pairs from the input. Default: no limit.

- `--phred33-quals`

  FastQ qualities are ASCII chars equal to the Phred quality plus 33. Default: ON.

- `--phred64-quals`

  FastQ qualities are ASCII chars equal to the Phred quality plus 64. Default: OFF.

- `--path_to_bowtie2`

  The full path `</../../>` to the Bowtie 2 installation on your system. If not specified it will be assumed that Bowtie 2 is in the `PATH`.

- `--path_to_hisat2`

The full path `</../../>` to the HISAT2 installation on your system. If not specified it will be assumed that HISAT2 is in the `PATH`.

##### Alignment:

The Bowtie 2 seed options below may be given in either letter case (`-N`/`-n`, `-L`/`-l`) — both are
accepted, as they were in Perl Bismark.

- `-N <int>`

  Sets the number of mismatches to be allowed in a seed alignment during multiseed alignment. Can be set to 0 or 1. Setting this higher makes alignment slower (often _much_ slower) but increases sensitivity. Default: 0.

- `-L <int>`

  Sets the length of the seed substrings to align during multiseed alignment. Smaller values make alignment slower but more sensitive. Default: the `--sensitive` preset of Bowtie 2 is used by default, which sets `-L` to 20.

- `--ignore-quals`

  When calculating a mismatch penalty, always consider the quality value at the mismatched position to be the highest possible, regardless of the actual value. i.e. input is treated as though all quality values are high. This is also the default behaviour when the input doesn't specify quality values (e.g. in `-f` mode). For bisulfite alignments in Bismark, this option is invariably turned on by default.

- `-I <int>` / `--minins <int>`

  The minimum insert size for valid paired-end alignments. E.g. if `-I 60` is specified and a paired-end alignment consists of two 20-bp alignments in the appropriate orientation with a 20-bp gap between them, that alignment is considered valid (as long as `-X` is also satisfied). A 19-bp gap would not be valid in that case. Default: 0.

- `-X <int>` / `--maxins <int>`

  The maximum insert size for valid paired-end alignments. E.g. if `-X 100` is specified and a paired-end alignment consists of two 20-bp alignments in the proper orientation with a 60-bp gap between them, that alignment is considered valid (as long as `-I` is also satisfied). A 61-bp gap would not be valid in that case. Default: 500.

- `--parallel <int>`

(May also be --multicore <int>). Sets the number of parallel instances of Bismark to be run concurrently. This forks the Bismark alignment step very early on so that each individual Spawn of Bismark processes only every n-th sequence (n being set by `--multicore`). Once all processes have completed, the individual BAM files, mapping reports, unmapped or ambiguous FastQ files are merged into single files in very much the same way as they would have been generated running Bismark conventionally with only a single instance.

If system resources are plentiful this is a viable option to speed up the alignment process (we observed a near linear speed increase for up to `--multicore 8` tested). However, please note that a typical Bismark run will use several cores already (Bismark itself, 2 or 4 threads for Bowtie/Bowtie2, gzip etc...) and ~10-16GB of memory per thread depending on the choice of aligner and genome.
**WARNING:** Bismark Parallel is **resource hungry**! Each value of `--multicore` specified will effectively lead to a linear increase in compute and memory requirements, so `--parallel 4` for e.g. the GRCm38 mouse genome will probably use ~20 cores and eat ~48GB of RAM, but at the same time reduce the alignment time to ~25-30%. _You have been warned_.

- `--local`

In this mode, it is not required that the entire read aligns from one end to the other. Rather, some characters may be omitted (“soft-clipped”) from the ends in order to achieve the greatest possible alignment score. For Bowtie 2, the match bonus `--ma` (default: 2) is used in this mode, and the best possible alignment score is equal to the match bonus (`--ma`) times the length of the read. This is mutually exclusive with end-to-end alignments. DEFAULT: OFF.

  Reported MAPQ values are normalised against that best possible score, so local-mode MAPQ differs from end-to-end MAPQ for the same alignment. Among the modes `--local` selects, this applies to Bowtie 2 only: HISAT2 exposes no `--local` option of its own and always scores matches as 0, so with `--hisat2 --local` the best possible score is 0 and MAPQ is reported on the same scale as end-to-end. `--local` still enables soft-clipping for HISAT2 (Bismark drops `--no-softclip`); only the MAPQ scale is shared with end-to-end.

  Note that a positive best possible score is not exclusive to `--local`: minimap2 and rammap align locally by design (they reject `--local`) and also score matches positively, so their MAPQ is normalised the same way — see `--minimap2` below.

##### Output:

- `--non_directional`

  The sequencing library was constructed in a non strand-specific manner, alignments to all four bisulfite strands will be reported.
  (The current Illumina protocol for BS-Seq is directional, in which case the strands complementary to the original strands are merely theoretical and should not exist in reality. Specifying directional alignments (which is the default) will only run 2 alignment threads to the original top (OT) or bottom (OB) strands in parallel and report these alignments. This is the recommended option for strand-specific libraries).
  Default: OFF

- `--pbat`

  This option may be used for PBAT-Seq libraries (Post-Bisulfite Adapter Tagging; Kobayashi et al., PLoS Genetics, 2012). This is essentially the exact opposite of alignments in 'directional' mode, as it will only launch two alignment threads to the CTOT and CTOB strands instead of the normal OT and OB ones. Use this option only if you are certain that your libraries were constructed following a PBAT protocol (if you don't know what PBAT-Seq is you should not specify this option). The option `--pbat` works only for FastQ files and uncompressed temporary files.

- `--sam-no-hd`

  Suppress SAM header lines (starting with @). This might be useful when very large input files are split up into several smaller files to run concurrently and the output files are to be merged afterwards.

- `--rg_tag`

  Write out a Read Group tag to the resulting SAM/BAM file. This will write the following line to the SAM header:

  @RG PL: ILLUMINA ID:SAMPLE SM:SAMPLE

  to set ID and SM see `--rg_id` and `--rg_sample`. In addition each read receives an `RG:Z:RG-ID` tag. Default: OFF (to not inflate file sizes).

- `--rg_id <string>`

  Sets the ID field in the `@RG` header line. Default: SAMPLE

- `--rg_sample <string>`

  Sets the SM field in the `@RG` header line; can't be set without setting `--rg_id` as well. Default: SAMPLE

- `--strandID`

For non-directional paired-end libraries, the strands identity is encoded by the order in which R1 and R2 are reported, as well as the read and genome conversion state. If third party tools re-organise this order it may become difficult to determine the alignment strand identity. This option adds an optional tag, e.g. `YS:Z:OT` or `YS:Z:CTOB` to preserve this information. See also [this thread for more details](https://github.com/FelixKrueger/Bismark/issues/455). Default: OFF.

- `--quiet`

  Print nothing besides alignments.

- `--un`

  Write all reads that could not be aligned to the file `_unmapped_reads.fq.gz` in the output directory. Written reads will appear as they did in the input, without any translation of quality values that may have taken place within `Bowtie` or `Bismark`. Paired-end reads will be written to two parallel files with `_1` and `_2` inserted in their filenames, i.e. `unmapped_reads_1.fq.gz` and `unmapped_reads_2.fq.gz`. Reads with more than one valid alignment with the same number of lowest mismatches (ambiguous mapping) are also written to `unmapped_reads.fq.gz` unless `--ambiguous` is also specified.

- `--ambiguous`

  Write all reads which produce more than one valid alignment with the same number of lowest mismatches or other reads that fail to align uniquely to `_ambiguous_reads.fq`. Written reads will appear as they did in the input, without any of the translation of quality values that may have taken place within `Bowtie` or `Bismark`. Paired-end reads will be written to two parallel files with `_1` and `_2` inserted in their filenames, i.e. `_ambiguous_reads_1.fq` and `_ambiguous_reads_2.fq`. These reads are not written to the file specified with `--un`.

- `-o/--output_dir <dir>`

  Write all output files into this directory. By default the output files will be written into the same folder as the input file. If the specified folder does not exist, Bismark will attempt to create it first. The path to the output folder can be either relative or absolute.

- `--temp_dir <dir>`

  Write temporary files to this directory instead of into the same directory as the input files. If the specified folder does not exist, Bismark will attempt to create it first. The path to the temporary folder can be either relative or absolute.

- `--non_bs_mm`

  Optionally outputs an extra column specifying the number of non-bisulfite mismatches a read during the alignment step. This option is only available for SAM format. In Bowtie 2 context, this value is just the number of actual non-bisulfite mismatches and ignores potential insertions or deletions. The format for single-end reads and read 1 of paired-end reads is `XA:Z:number of mismatches` and `XB:Z:number of mismatches` for read 2 of paired-end reads.

- `--gzip`

  Temporary bisulfite conversion files will be written out in a `GZIP` compressed form to save disk space. This option is available for most alignment modes but is not available for paired-end `FastA` files.

- `--no_stream_converted`

  Write the in-silico converted reads (`_C_to_T` / `_G_to_A`) to `--temp_dir` as files instead of streaming them to the aligner through named pipes.

  Bismark converts your reads before aligning them, and by default those converted reads now go straight into the aligner through a FIFO and are never written to disk. They have exactly one consumer — the aligner — so nothing is lost by not keeping them: the methylation call re-reads your *original* FastQ. On a disk-constrained scratch this is the difference between a run that starts and one that does not; the converted set is typically around 5x the size of a gzipped input, and `--gzip` only ever made it smaller, never absent. Wall time is unchanged — conversion no longer has to finish before the aligner starts, but that serial phase was small to begin with.

  Output is byte-identical either way. Use this flag if you want to inspect the converted reads, or to bisect a suspected streaming problem. `--gzip` has no effect on the streamed reads — there is no file to compress.

  `--rammap`'s default in-process backend goes one better: it aligns inside the Bismark process, so it receives the converted reads in memory with neither a file nor a pipe in between. `--rammap_subprocess` runs the external binary and keeps files.

  Some runs cannot stream and fall back to files on their own, saying so on STDERR: `--hisat2` (HISAT2 rejects a named pipe as its read input), the `--combined_index` alignment models, `--rammap_subprocess`, and a `--temp_dir` on a filesystem that will not host a FIFO. `--illumina_5base` aligns to the unconverted genome and writes no converted reads at all.

- `--sam`

  The output will be written out in `SAM` format instead of the default `BAM` format.

- `--cram`

  Requests `CRAM` output instead of `BAM`. **Not supported by the Rust aligner in v1 (BAM only)** — a `--cram` run is rejected fail-loud; CRAM *output* is deferred to a later release. (CRAM *input* elsewhere in the suite is read via pure-Rust `noodles`, no Samtools required.)

- `--cram_ref <ref_file>`

Once CRAM output is supported, it requires you to specify a reference genome as a single FastA file. If this single-FastA reference file is not supplied explicitly it will be regenerated from the genome `.fa` sequence(s) used for the Bismark run and written to a file called `Bismark_genome_CRAM_reference.mfa` into the output directory.

- `--samtools_path`

  Accepted for compatibility but **ignored** by the Rust suite, which does all BAM/SAM/CRAM I/O with pure-Rust `noodles` — no Samtools installation is required.

- `--prefix <prefix>`

  Prefixes `<prefix>` to the output file names. Trailing dots will be replaced by a single one. For example, `--prefix test` with `file.fq` would result in the output file `test.file_bismark.bam` etc.

- `-B/--basename <basename>`

  Write all output to files starting with this base file name. For example, `--basename foo` would result in the files `foo.bam` and `foo_SE_report.txt` (or its paired-end equivalent). Takes precedence over `--prefix`.

- `--old_flag`

  Only in paired-end SAM mode, uses the FLAG values used by Bismark 0.8.2 and before. In addition, this options appends /1 and /2 to the read IDs for reads 1 and 2 relative to the input file. Since both the appended read IDs and custom FLAG values may cause problems with some downstream tools such as Picard, new defaults were implemented as of version 0.8.3.

                default                  old_flag
           ===================     ===================
           Read 1       Read 2     Read 1       Read 2
         OT:    99           147        67           131
         OB:    83           163       115           179
         CTOT:  99           147        67           131
         CTOB:  83           163       115           179

- `--ambig_bam`

For reads that have multiple alignments a random alignment is written out to a special file ending in `.ambiguous.bam`. The alignments are in Bowtie2 format and do not any contain Bismark specific entries such as the methylation call etc. These ambiguous BAM files are intended to be used as coverage estimators for variant callers.

- `--nucleotide_coverage`

  Calculates the mono- and di-nucleotide sequence composition of covered positions in the analysed BAM file and compares it to the genomic average composition once alignments are complete by calling `bismark bam2nuc`. Since this calculation may take a while, `bismark bam2nuc` attempts to write the genomic sequence composition into a file called _genomic_nucleotide_frequencies.txt_ inside the reference genome folder so it can be re-used the next time round instead of calculating it once again. If a file _nucleotide_stats.txt_ is found with the Bismark reports it will be automatically detected and used for the Bismark HTML report. This option works only for BAM or CRAM files.

##### Other:

- `-h/--help`

  Displays this help file. Displays version information.

- `-v/--version`

  Displays version information and exits.

##### BOWTIE 2 SPECIFIC OPTIONS

- `--bowtie2`

  Default: ON. Uses Bowtie 2. Bismark limits Bowtie 2 to only perform end-to-end alignments, i.e. searches for alignments involving all read characters (also called untrimmed or unclipped alignments). Bismark assumes that raw sequence data is adapter and/or quality trimmed where appropriate. Both small (`.bt2`) and large (`.bt2l`) Bowtie 2 indexes are supported.

- `--no_dovetail`

  It is possible, though unusual, for the mates to "dovetail", with the mates seemingly extending "past" each other as in this example:

                         Mate 1:                 GTCAGCTACGATATTGTTTGGGGTGACACATTACGC
                         Mate 2:            TATGAGTCAGCTACGATATTGTTTGGGGTGACACAT
                         Reference: GCAGATTATATGAGTCAGCTACGATATTGTTTGGGGTGACACATTACGCGTCTTTGAC

  Dovetailing is considered inconsistent with concordant alignment, but by default Bismark calls Bowtie 2 with `--dovetail`, causing it to consider dovetailing alignments as concordant. This becomes relevant whenever reads are clipped from their 5' end prior to mapping, e.g. because of quality or bias issues such as in PBAT or EM-seq libraries.

  Specify `--no_dovetail` to turn off this behaviour for paired-end libraries. Default: OFF.


##### HISAT2 SPECIFIC OPTIONS:

- `--hisat2`

  Default: OFF. Uses HISAT2. Bismark limits HISAT2 to perform end-to-end alignments, i.e. searches for alignments involving all read characters (also called untrimmed or unclipped alignments) using the option `--no-softclipping`. Bismark assumes that raw sequence data is adapter and/or quality trimmed where appropriate. Both small (`.ht2`) and large (`.ht2l`) HISAT2 indexes are supported.

- `--no-spliced-alignment`

  Disable spliced alignment.

- `--known-splicesite-infile <path>`

  Provide a list of known splice sites.

##### Paired-end options:

- `--no-mixed`

  This option disables the behaviour to try to find alignments for the individual mates if it cannot find a concordant or discordant alignment for a pair. This option is invariably on by default.

- `--no-discordant`

  Normally, Bowtie 2 or HISAT2 look for discordant alignments if they cannot find any concordant alignments. A discordant alignment is an alignment where both mates align uniquely, but that does not satisfy the paired-end constraints (`--fr`/`--rf`/`--ff`, `-I`, `-X`). This option disables that behaviour and is on by default.

##### Bowtie 2 Effort options:

- `-D <int>`

  Up to &lt;int&gt; consecutive seed extension attempts can "fail" before Bowtie 2 moves on, using the alignments found so far. A seed extension "fails" if it does not yield a new best or a new second-best alignment. Default: 15.

- `-R <int>`

  &lt;int&gt; is the maximum number of times Bowtie 2 will "re-seed" reads with repetitive seeds. When "re-seeding," Bowtie 2 simply chooses a new set of reads (same length, same number of mismatches allowed) at different offsets and searches for more alignments. A read is considered to have repetitive seeds if the total number of seed hits divided by the number of seeds that aligned at least once is greater than 300. Default: 2.

##### Parallelization options:

- `-p NTHREADS`

  Launch NTHREADS parallel search threads (default: 1). Threads will run on separate processors/cores and synchronize when parsing reads and outputting alignments. Searching for alignments is highly parallel, and speed-up is close to linear. NOTE: It is currently unclear whether this speed increase also translates into a speed increase of Bismark since it is running several instances of Bowtie 2 concurrently! Increasing `-p` increases Bowtie 2's memory footprint. E.g. when aligning to a human genome index, increasing `-p` from 1 to 8 increases the memory footprint by a few hundred megabytes (for each instance of Bowtie 2!). This option is only available if Bowtie is linked with the pthreads library (i.e. if BOWTIE_PTHREADS=0 is not specified at build time). In addition, this option will automatically use the option `--reorder`, which guarantees that output SAM records are printed in an order corresponding to the order of the reads in the original input file, even when `-p` is set greater than 1 (Bismark requires the Bowtie 2 output to be this way). Specifying `--reorder` and setting `-p` greater than 1 causes Bowtie 2 to run somewhat slower and use somewhat more memory than if `--reorder` were not specified. Has no effect if `-p` is set to 1, since output order will naturally correspond to input order in that case.

##### Scoring options:

- `--score_min <func>`

  Sets a function governing the minimum alignment score needed for an alignment to be considered "valid" (i.e. good enough to report). This is a function of read length. For instance, specifying `L,0,-0.2` sets the minimum-score function `f` to `f(x) = 0 + -0.2 * x`, where `x` is the read length. See also: setting function options at http://bowtie-bio.sourceforge.net/bowtie2. The default is: L,0,-0.2.

- `--rdg <int1>,<int2>`

  Sets the read gap open (&lt;int1>) and extend (&lt;int2>) penalties. A read gap of length N gets a penalty of `<int1> + N * <int2>`. Default: 5, 3.

- `--rfg <int1>,<int2>`

  Sets the reference gap open (&lt;int1>) and extend (&lt;int2>) penalties. A reference gap of length N gets a penalty of `<int1> + N * <int2>`. Default: 5, 3.


#### MINIMAP2-SPECIFIC OPTIONS:


- `--minimap2/--mm2`

Uses minimap2 as the underlying read aligner. This mode is very new and currently experimental. Expect that things may change in the near future. The default mapping mode is `--nanopore` (preset `-x map-ont` (Nanopore reads)). Internally, minimap2 is run with the options `-a --MD`. More information here: https://lh3.github.io/minimap2/minimap2.html. Default: OFF.

  **MAPQ:** minimap2 reports *positive* alignment scores (every preset Bismark can select awards 2 per matching base), so the best possible score is `2 × read_length` and Bismark normalises MAPQ against it, exactly as it does for Bowtie 2 `--local`. MAPQ is **22–44 for a uniquely-aligned read**, and can be **lower — down to 2 — when another strand instance produced a competing score** for the same read. This is **Bismark's own scale, not minimap2's**: minimap2 derives its own MAPQ from chain scores on a 0–60 scale, and Bismark discards it because it recomputes MAPQ across the 2–4 strand instances it runs. The same applies to `--rammap`.

  Four consequences worth knowing:

  - Because the score is compared against the *full* read length, a soft-clipped alignment scores below perfect and earns a lower MAPQ.
  - `--score_min` is **not passed to minimap2** — minimap2 gets a fixed option string — yet its value still enters this normalisation, so setting an unusually steep `--score_min` raises minimap2 MAPQ without changing a single alignment.
  - The competing-score branch is entered only when a **later** strand instance strictly out-scores every earlier one. Two reads with the same pair of scores can therefore end up with different MAPQ depending on which strand won — a pre-existing asymmetry that this normalisation widens. Reads on that branch can fall below a typical `-q` cut, so MAPQ-filtered read counts differ from Bismark 3.1.0.
  - `--ambig_bam` copies MAPQ straight from the aligner's own output rather than recomputing it, so a `--minimap2 --ambig_bam` run legitimately writes **two different MAPQ scales**: Bismark's 22–44 in the main BAM and minimap2's 0–60 in the ambiguous-alignments BAM.

- `--mm2_nanopore`

Using the minimap2 preset for Oxford Nanopore (ONT) vs reference mapping (`-x map-ont`). Only works in conjuntion with `--minimap2` or `--rammap`. Default mode when `--minimap2` is specified without additional qualifiers.

- `--mm2_pacbio`

Using the minimap2 preset for PacBio vs reference mapping (`-x map-pb`). Only works in conjuntion with `--minimap2` or `--rammap`. Default: OFF.

  Note this preset differs from the default in index-build parameters (k-mer size and homopolymer compression), plus one chaining coefficient derived from that k-mer size. Bisulfite alignments run against a pre-built index, which supplies its own, so `--mm2_pacbio` is near-inert there — expect alignments equivalent to the default `map-ont`, which a `--rammap` run says on stderr. (The exception is `--illumina_5base`, which reads the genome FASTA directly, so the preset's index parameters do apply.)

- `--mm2_short_reads`

This option invokes the minmap2 preset setting `-x sr` and is intended for genomic short-read mapping with accurate reads (probably Illumina 150bp+ ?). For spliced short-reads, please use `--hisat2` instead. The `sr` preset mode (short single-end reads without splicing) uses the following options:
`-k21 -w11 --sr --frag=yes -A2 -B8 -O12,32 -E2,1 -r50 -p.5 -N20 -f1000,5000 -n2 -m20 -s40 -g200 -2K50m --heap-sort=yes --secondary=no`. Default: OFF.

  Works with `--rammap` as well as `--minimap2`. Because `sr` changes the chain and DP thresholds as well as the mismatch and gap penalties, it changes **which** reads map, not only their alignment scores — expect mapping-efficiency percentages to differ from a `map-ont` run. Against a pre-built bisulfite index the `-k21 -w11` part has no effect (the index supplies its own), so `sr` there means the scoring and filtering options rather than the seeding ones.

- `--mm2_maximum_length <int>`

Maximum length cutoff for very long sequences (currently allowed 100-100,000 bp). Default: 10000.


#### RAMMAP-SPECIFIC OPTIONS:


- `--rammap/--ram`

Uses [rammap](https://github.com/jwanglab/rammap), a pure-Rust reimplementation of minimap2, as the underlying read aligner. Intended for long-read bisulfite data (EM-seq Nanopore/PacBio). Single-end only, and like `--minimap2` defaults to the `map-ont` preset and honours the `--mm2_*` selectors above. **Experimental and concordance-gated — NOT byte-identical to minimap2.** rammap is compiled into the conda, Docker and release builds, so this works with no extra install. By default it runs the **in-process** backend (the converted index is loaded once and shared across strand instances; auto-threaded to the available cores, capped), which uses less memory and is faster than spawning the external binary. Default: OFF.

- `--rammap_subprocess`

Opt out of the in-process backend and spawn the external `rammap` binary on your `PATH` instead (exactly like `--minimap2`). Use this for exact rammap-CLI parity, or on a build compiled without the in-process backend. Requires `--rammap`. Default: OFF.

- `--path_to_rammap <path>`

The folder containing the `rammap` executable (used by `--rammap_subprocess`; not the executable itself).

- `--rammap_inprocess`

**Deprecated (hidden).** The in-process backend is now the `--rammap` default, so this flag is inert — accepted for backward compatibility and removed in a later release. Use `--rammap_subprocess` to opt out to the external binary.

#### OUTPUT:

The output is a BAM file by default, as well as a aligment report text file.

For a single-end file called 'simulated.fastq', the expected output for Bowtie 2, HISAT2 or minimap2 is:

```
simulated_bismark_bt2.bam
simulated_bismark_bt2_SE_report.txt
simulated_bismark_hisat2.bam
simulated_bismark_hisat2_SE_report.txt
simulated_bismark_mm2.bam
simulated_bismark_mm2_SE_report.txt
```

In a paired-end situation, for files 'simulated_1.fastq' and 'simulated_2.fastq' you would expect the following result files Bowtie2 or HISAT2:

```
simulated_1_bismark_bt2_pe.bam
simulated_1_bismark_bt2_PE_report.txt
simulated_1_bismark_hisat2_pe.bam
simulated_1_bismark_hisat2_PE_report.txt
```

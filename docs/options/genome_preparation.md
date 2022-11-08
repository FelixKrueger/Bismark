## Appendix (I): Bismark Genome Preparation

A full list of options can also be viewed by typing: `bismark_genome_preparation --help`

#### USAGE: `bismark_genome_preparation [options] <arguments>`

#### OPTIONS:

- `--help`

  Displays help text.

- `--version`

  Displays version information and exits.

- `--verbose`

  Print verbose output for more details or debugging.

- `--path_to_aligner </../../>`

  The full path to the Bowtie 2 or HISAT2 installation folder on your system (depending on which aligner/indexer you intend to use; please note that thi is the folder and not any executable). Unless this path is specified, it is assumed that the aligner in question (Bowtie 2/HISAT2) is in the PATH.

- `--bowtie2`

  This will create bisulfite indexes for use with Bowtie 2. Recommended for most bisulfite sequencing applications (Default: ON).

- `--hisat2`

  This will create bisulfite indexes for use with HISAT2. At the time of writing, this is still largely unchartered territory, and only recommended for specialist applications such as RNA-methylation analyses or SLAM-seq type applications (see also: --slam). (Default: OFF).

- `--single_fasta`

  Instruct the Bismark Indexer to write the converted genomes into single-entry FastA files instead of making one multi-FastA file (MFA) per chromosome. This might be useful if individual bisulfite converted chromosomes are needed (e.g. for debugging), however it can cause a problem with indexing if the number of chromosomes is vast (this is likely to be in the range of several thousand files; operating systems can only handle lists up to a certain length. Some newly assembled genomes may contain 20000-500000 contig of scaffold files which do exceed this list length limit).

- `--genomic_composition`

  Calculate and extract the genomic sequence composition for mono- and di-nucleotides and write the genomic composition table _genomic_nucleotide_frequencies.txt_ to the genome folder. This may be useful later on when using `bam2nuc` or the Bismark option `--nucleotide_coverage`.

- `--slam`

  Instead of performing an in-silico bisulfite conversion, this mode transforms T to C (forward strand), or A to G (reverse strand). The folder structure and rest of the indexing process is currently exactly the same as for bisulfite sequences, but this might change at some point. This means that a genome prepared in --slam mode is currently indistinguishable from a true Bisulfite Genome, so please make sure you name the genome folder appropriately to avoid confusion.

#### ARGUMENTS:

- `<path_to_genome_folder>`

  The path to the folder containing the genome to be bisulfite converted (this may be an absolute or relative path). Bismark Genome Preparation expects one or more `FastA` files in the folder (valid file extensions: `.fa` or `.fasta`).

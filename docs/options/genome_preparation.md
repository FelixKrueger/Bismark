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

- `--minimap2/--mm2`

This will create bisulfite indexes for use with minimap2 (https://github.com/lh3/minimap2). This is recommended only for specialist applications such as EM-seq with ONT (Oxford Nanopore Technologies) or PacBio reads. (Default: OFF).

- `--parallel INT`

Use several threads for each indexing process to speed up the genome preparation step. Remember that the indexing is run twice in parallel already (for the top and bottom strand separately), so e.g. `--parallel 4` will use 8 threads in total. Please also see `--large-index` for parallel processing of VERY LARGE genomes (e.g. the axolotl)

- `--single_fasta`

  Instruct the Bismark Indexer to write the converted genomes into single-entry FastA files instead of making one multi-FastA file (MFA) per chromosome. This might be useful if individual bisulfite converted chromosomes are needed (e.g. for debugging), however it can cause a problem with indexing if the number of chromosomes is vast (this is likely to be in the range of several thousand files; operating systems can only handle lists up to a certain length. Some newly assembled genomes may contain 20000-500000 contig of scaffold files which do exceed this list length limit). Does not work in conjunction with `--minimap2`.

- `--genomic_composition`

  Calculate and extract the genomic sequence composition for mono- and di-nucleotides and write the genomic composition table _genomic_nucleotide_frequencies.txt_ to the genome folder. This may be useful later on when using `bam2nuc` or the Bismark option `--nucleotide_coverage`.

- `--slam`

  Instead of performing an in-silico bisulfite conversion, this mode transforms T to C (forward strand), or A to G (reverse strand). The folder structure and rest of the indexing process is currently exactly the same as for bisulfite sequences, but this might change at some point. This means that a genome prepared in --slam mode is currently indistinguishable from a true Bisulfite Genome, so please make sure you name the genome folder appropriately to avoid confusion.

- `--large-index`

Force generated index to be 'large', even if reference has fewer than 4 billion nucleotides. At the time of writing this is required for parallel processing of VERY LARGE genomes (e.g. the axolotl). Does not work in conjunction with `--minimap2`.

#### ARGUMENTS:

- `<path_to_genome_folder>`

  The path to the folder containing the genome to be bisulfite converted (this may be an absolute or relative path). Bismark Genome Preparation expects one or more `FastA` files in the folder (valid file extensions: `.fa` or `.fasta`).

#### OUTPUT:

This script is supposed to convert a specified reference genome into two different bisulfite converted versions and index them for alignments with Bowtie 2 (default), HISAT2 or minimap2. The first bisulfite genome will have all Cs converted to Ts (C->T), and the other one will have all Gs converted to As (G->A).
Both bisulfite genomes will be stored in subfolders within the reference genome folder containing the unconverted reference sequence (in FastA format). Once the bisulfite
conversion has been completed, the program will fork and launch two simultaneous instances of the Bowtie 2, HISAT2 or minimap2 indexer (bowtie2-build or hisat2-build or minimap2 -d, resepctively). This is the structure of the reference genome folder after successful indexing (with Bowtie2 in this case):

```
├── Bisulfite_Genome
│   ├── CT_conversion
│   │   ├── BS_CT.1.bt2
│   │   ├── BS_CT.2.bt2
│   │   ├── BS_CT.3.bt2
│   │   ├── BS_CT.4.bt2
│   │   ├── BS_CT.rev.1.bt2
│   │   ├── BS_CT.rev.2.bt2
│   │   └── genome_mfa.CT_conversion.fa
│   └── GA_conversion
│       ├── BS_GA.1.bt2
│       ├── BS_GA.2.bt2
│       ├── BS_GA.3.bt2
│       ├── BS_GA.4.bt2
│       ├── BS_GA.rev.1.bt2
│       ├── BS_GA.rev.2.bt2
│       └── genome_mfa.GA_conversion.fa
└── reference_sequence.fa
```

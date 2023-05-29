# Bismark

[![install with bioconda](https://img.shields.io/badge/install%20with-bioconda-brightgreen.svg?style=flat)](http://bioconda.github.io/recipes/bismark/README.html)

> **See the documentation**: <https://felixkrueger.github.io/Bismark>

Bismark is a program to map bisulfite treated sequencing reads to a genome of interest and perform methylation calls in a single step. The output can be easily imported into a genome viewer, such as [SeqMonk](http://www.bioinformatics.babraham.ac.uk/projects/seqmonk/), and enables a researcher to analyse the methylation levels of their samples straight away. It's main features are:

- Bisulfite mapping and methylation calling in one single step
- Supports single-end and paired-end read alignments
- Supports ungapped, gapped or spliced alignments
- Alignment seed length, number of mismatches etc. are adjustable
- Output discriminates between cytosine methylation in `CpG`, `CHG` and `CHH` context

## Documentation

The Bismark documentation can be found with the code in the [docs](docs) subfolder and can also be read online: <https://felixkrueger.github.io/Bismark/>

There is also an overview of the alignment modes that are currently supported by Bismark: [Bismark alignment modes](http://www.bioinformatics.babraham.ac.uk/projects/bismark/Bismark_alignment_modes.pdf) (pdf).

## Installation

Bismark is written in Perl and is executed from the command line. To install Bismark simply download the latest release of the code from the [Releases page](https://github.com/FelixKrueger/Bismark/releases) and extract the files into a Bismark installation folder.

Bismark needs the following tools to be installed and ideally available in the `PATH` environment:

- [Bowtie2](http://bowtie-bio.sourceforge.net/bowtie2/) or [HISAT2](https://ccb.jhu.edu/software/hisat2/index.shtml) or [minimap2](https://lh3.github.io/minimap2/minimap2.html)
- [Samtools](http://www.htslib.org/)

## Links

- Bismark Publication
  - http://www.ncbi.nlm.nih.gov/pubmed/21493656
- Our review about primary data analysis in BS-Seq
  - http://www.ncbi.nlm.nih.gov/pubmed/22290186
  
## Credits

Bismark was written by Felix Krueger, part of the [Babraham Bioinformatics](http://www.bioinformatics.babraham.ac.uk/projects/bismark/) group.

## Licences

Bismark itself is free software, `bismark2report` and `bismark2summary` produce HTML graphs powered by [Plot.ly](https://plot.ly/javascript/) which are also free to use and look at!

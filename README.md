# Bismark

Bismark is a program to map bisulfite treated sequencing reads to a genome of interest and perform methylation calls in a single step. The output can be easily imported into a genome viewer, such as [SeqMonk](http://www.bioinformatics.babraham.ac.uk/projects/seqmonk/), and enables a researcher to analyse the methylation levels of their samples straight away. It's main features are:

* Bisulfite mapping and methylation calling in one single step
* Supports single-end and paired-end read alignments
* Supports ungapped and gapped alignments
* Alignment seed length, number of mismatches etc. are adjustable
* Output discriminates between cytosine methylation in `CpG`, `CHG` and `CHH` `*` context

## Installation
Bismark is written in Perl and is executed from the command line. To install Bismark simply download the latest release of the code from the [Releases page](https://github.com/FelixKrueger/Bismark/releases) and extract the files into a Bismark installation folder.

Bismark needs the following tools to be installed and available in the `PATH`:
* [Bowtie2](http://bowtie-bio.sourceforge.net/bowtie2/) or [Bowtie1](http://bowtie-bio.sourceforge.net/)
* [Samtools](http://www.htslib.org/)

## Documentation
The Bismark documentation can be found with the code in the [Docs](Docs) subfolder and also read online.

There is also an overview of the alignment modes that are currently supported by Bismark: [Bismark alignment modes](http://www.bioinformatics.babraham.ac.uk/projects/bismark/Bismark_alignment_modes.pdf) (pdf).

## Links
* Bismark Publication
    * http://www.ncbi.nlm.nih.gov/pubmed/21493656
* Our review about primary data analysis in BS-Seq
    * http://www.ncbi.nlm.nih.gov/pubmed/22290186
* Our protocol _"Quality Control, trimming and alignment of Bisulfite-Seq data"_
    * http://www.epigenesys.eu/en/protocols/bio-informatics/483-quality-control-trimming-and-alignment-of-bisulfite-seq-data-prot-57

## Credits
Bismark was written by Felix Krueger, part of the [Babraham Bioinformatics](http://www.bioinformatics.babraham.ac.uk/projects/bismark/) group.
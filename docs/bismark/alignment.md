# Alignment

This step represents the actual bisulfite alignment and methylation calling part. Bismark requires the user to specify only two things:

1. The directory containing the genome of interest. This folder must contain the unmodified genome (as `.fa` or `.fasta` files) as well as the two bisulfite genome subdirectories which were generated in the Bismark Genome Preparations step (see above).
2. The sequence file(s) to be analysed (in either `FastQ` or `FastA` format).

All other parameters are optional.

For each sequence file or each set of paired-end sequence files, Bismark produces one alignment and methylation call output file as well as a report file detailing alignment and methylation call statistics for your information and record keeping.

### Running `bismark`

Before running Bismark we recommend spending some time on quality control of the raw sequence files using [FastQC](http://www.bioinformatics.babraham.ac.uk/projects/fastqc/). FastQC might be able to spot irregularities associated with your BS-Seq file, such as high base calling error rates or contaminating sequences such as PCR primers or Illumina adapters. Many sources of error impact detrimentally the alignment efficiencies and/or alignment positions, and thus possibly also affect the methylation calls and conclusions drawn from the experiment.

If no additional options are specified Bismark will use a set of default values, some of which are:

### Using Bowtie 2:

- Using Bowtie 2 is the default mode
- If no specific path to Bowtie 2 is specified it is assumed that the `bowtie2` executable is in the `PATH`
- Standard alignments use a multi-seed length of 20bp with 0 mismatches. These parameters can be modified using the options `-L` and `-N`, respectively
- Standard alignments use the default minimum alignment score function L,0,-0.2, i.e. f(x) = 0 + -0.2 \* x (where x is the read length). For a read of 75bp this would mean that a read can have a lowest alignment score of -15 before an alignment would become invalid. This is roughly equal to 2 mismatches or ~2 indels of 1-2 bp in the read (or a combination thereof). The stringency can be set using the `--score_min <func>` function.

Even though the user is not required to specify additional alignment options it is often advisable to do so (e.g. when the default parameters are too strict). To see a full list of options please type `bismark --help` on the command line or see the Appendix at the end of this User Guide.

### Directional BS-Seq libraries (default)

Bisulfite treatment of DNA and subsequent PCR amplification can give rise to four (bisulfite converted) strands for a given locus. Depending on the adapters used, BS-Seq libraries can be constructed in two different ways:

1. If a library is directional, only reads which are (bisulfite converted) versions of the original top strand (OT) or the original bottom strand (OB) will be sequenced. Even though the strands complementary to OT (CTOT) and OB (CTOB) are generated in the BS-PCR step they will not be sequenced as they carry the wrong kind of adapter at their 5’-end. By default, Bismark performs only 2 read alignments to the OT and OB strands, thereby ignoring alignments coming from the complementary strands as they should theoretically not be present in the BS-Seq library in question.
2. Alternatively, BS-Seq libraries can be constructed so that all four different strands generated in the BS-PCR can and will end up in the sequencing library with roughly the same likelihood. In this case all four strands (OT, CTOT, OB, CTOB) can produce valid alignments and the library is called non- directional. Specifying `--non_directional` instructs Bismark to use all four alignment outputs.

To summarise again: alignments to the original top strand or to the strand complementary to the original top strand (OT and CTOT) will both yield methylation information for cytosines on the top strand. Alignments to the original bottom strand or to the strand complementary to the original bottom strand (OB and CTOB) will both yield methylation information for cytosines on the bottom strand, i.e. they will appear to yield methylation information for G positions on the top strand of the reference genome.

For more information about how to extract methylation information of the four different alignment strands please see below in the section on the Bismark methylation extractor.

**USAGE:**

```
bismark [options] --genome <genome_folder> {-1 <mates1> -2 <mates2> | <singles>}
```

A typical single-end analysis could look like this:

```
bismark --genome /data/genomes/homo_sapiens/GRCh38/ sample.fastq.gz
```

### What does the Bismark output look like?

Since version 0.6.x the default output of Bismark is in BAM/SAM format (which is required to encode gapped alignments).

### Bismark BAM/SAM output (default)

By default, Bismark generates SAM output for all alignment modes. Please note that reported quality values are encoded in Sanger format (Phred 33 scale), even if the input was in Phred64.

1. `QNAME` (seq-ID)
2. `FLAG` (this flag tries to take the strand a bisulfite read originated from into account (this is different from ordinary DNA alignment flags!))
3. `RNAME` (chromosome)
4. `POS` (start position)
5. `MAPQ` (calculated for Bowtie 2 and HISAT2)
6. `CIGAR`
7. `RNEXT`
8. `PNEXT`
9. `TLEN`
10. `SEQ`
11. `QUAL` (Phred33 scale)
12. `NM-tag` (edit distance to the reference)
13. `MD-tag` (base-by-base mismatches to the reference) (14) XM-tag (methylation call string)
14. `XR-tag` (read conversion state for the alignment) (16) XG-tag (genome conversion state for the alignment)

The mate read of paired-end alignments is written out as an additional separate line in the same format.

### BAM compression with Genozip

!!! info

    3rd party program notice.

    Information valid as of: 21/09/2022.

Genozip v14 and above supports the compression of Bismark-generated BAM files. A benchmark with a Bismark test file (PE) showed that compression resulted in a 7X vs BAM and more than 2X vs CRAM 3.1 (see [this issue](https://github.com/FelixKrueger/Bismark/issues/526)). More information on Genozip on its [website](https://www.genozip.com), conda installation `conda install genozip`.

Please note that while Genozip is free for academic use, it is a commercial product, so users would need to register to it separately.

### Data visualisation

To see the location of the mapped reads the Bismark output file can be imported into a genome viewer, such as SeqMonk, using the chromosome, start and end positions (this can be useful to identify regions in the genome which display an artefactually high number of aligned reads). The alignment output can also be used to apply post-processing steps such as de-duplication (allowing only 1 read for each position in the genome to remove PCR artefacts) or filtering on the number of bisulfite conversion related non-bisulfite mismatches \* (please note that such post-processing scripts are not part of the Bismark package).

!!! tip

    Bisulfite conversion related non-bisulfite mismatches are mismatch positions which have a C in the BS-read but a T in the genome; such mismatches may occur due to the way bisulfite read alignments are performed. Reads containing this kind of mismatches are not automatically removed from the alignment output in order not to introduce a bias for methylated reads.

    It should be noted that, even though no methylation calls are performed for these positions, reads containing bisulfite conversion related non-bisulfite mismatches might lead to false alignments if particularly lax alignment parameters were specified.

### Methylation call

The methylation call string contains a dot `.` for every position in the BS-read not involving a cytosine, or contains one of the following letters for the three different cytosine methylation contexts (UPPER CASE = METHYLATED, lower case = unmethylated):

- `z` - C in CpG context - unmethylated
- `Z` - C in CpG context - methylated
- `x` - C in CHG context - unmethylated
- `X` - C in CHG context - methylated
- `h` - C in CHH context - unmethylated
- `H` - C in CHH context - methylated
- `u` - C in Unknown context (CN or CHN) - unmethylated
- `U` - C in Unknown context (CN or CHN) - methylated
- `.` - not a C or irrelevant position

### Local alignments in Bowtie 2 or HISAT2 mode

!!! note

    This has been previously only been mentioned in the release notes here: <https://github.com/FelixKrueger/Bismark/releases/tag/0.22.0>

Expanding on our observation that single-cell BS-seq, or PBAT libraries in general, can [generate chimeric read pairs](https://sequencing.qcfail.com/articles/pbat-libraries-may-generate-chimaeric-read-pairs/), a publication by [Wu et al.](https://www.ncbi.nlm.nih.gov/pubmed/30859188) described in further detail that intra-fragment chimeras can hinder the efficient alignment of single-cell BS-seq libraries. In there, the authors described a pipeline that uses paired-end alignments first, followed by a second, single-end alignment step that uses local alignments in a bid to improve the mapping of intra-molecular chimeras. To allow this type of improvement for single-cell or PBAT libraries, Bismark also allows the use of local alignments.

**Please note** that we still do not recommend using local alignments as a means to _magically_ increase mapping efficiencies (please see [here](https://sequencing.qcfail.com/articles/soft-clipping-of-reads-may-add-potentially-unwanted-alignments-to-repetitive-regions/)), but we do acknowledge that PBAT/scBSs-seq/scNMT-seq are exceptional applications where local alignments might indeed make a difference (there is only so much data to be had from a single cell...).
We didn't have the time yet to set more appropriate or stringent default values for local alignments (suggestions welcome), nor did we investigate whether the methylation extraction will require an additional `--ignore` flag if a read was found to the be soft-clipped (the so called 'micro-homology domains'). This might be added in the near future.

# Bismark FAQs

This will be a collection of fairly common issues that arise fairly regularly. Started on 03 Sept 2019

## Thoughts and considerations regarding single-cell (scBS-seq), scNMT and PBAT libraries (September 18, 2019).


[priming issues](https://sequencing.qcfail.com/articles/mispriming-in-pbat-libraries-causes-methylation-bias-and-poor-mapping-efficiencies/)

#### Chimeric reads

Has been described at [QCFail](https://sequencing.qcfail.com/articles/pbat-libraries-may-generate-chimaeric-read-pairs/)
A word about hybrid reads in PBAT libraries

While the measures described above should be pretty effective in combating errors introduced by the PBAT protocol it should be noted that we have seen a tendency of hybrid reads in PBAT libraries. Hybrid reads in this context are reads that would be considered discordant, i.e. where R1 and R2 align to completely different places in the genome. These reads will not be reported by Bismark as valid alignments, but we have seen paired-end libraries with in excess of 30% of all fragments aligning as to different places in the genome similar to a bisulfite Hi-C experiment (should there be one…). It is debatable whether a read pair starting at say chromosome 3 and continuing into chromosome 17 contains credible methylation information, but clearly a fairly large portion of hybrid reads seems to contribute to the generally low-ish mapping efficiency observed for some PE PBAT libraries.

As a solution to this problem we would probably recommend to run 5′ clipping and trimming first, and run PE alignments with  --unmapped specified in Bismark. The resulting unmapped Read 1 and Read 2 files may then be aligned in single-end mode afterwards to salvage as much data from alignable hybrid reads in the experiment as possible (--pbat for R1 and default settings for R2).

#### mapping strategies

A recent article in Bioinformatics (https://www.ncbi.nlm.nih.gov/pubmed/30859188) also demonstrated in that chimeric reads are also the main problem for the low mapping efficiency in scBS-seq. In their article, Wu and colleagues demonstrate that the post-bisulfite based library construction protocol leads to a substantial amount of chimeric molecules as a result of recombination of genomic proximal sequences with ‘microhomology regions (MR)’. As a means to combat this problem the authors suggest a method that uses local alignments of reads that do not align in a traditional manner, and in addition remove MR sequences from these local alignments as they can introduce noise into the methylation data.


Such chimeric “Hi-C like bisulfite reads” deliberately do not produce valid (i.e. concordant) paired-end alignments with Bismark. To rescue as much data from a paired-end PBAT library with low mapping efficiency as possible we sometimes perform the following method (affectionately termed “Dirty Harry” because it is not the most straight forward or cleanest approach):

Paired-end alignments (--pbat) to start while writing out the unmapped R1 and R2 reads using the option --unmapped. Properly aligned PE reads should be methylation extracted while counting overlapping reads only once (which is the default). Also mind 5′ trimming mentioned in this post.
unmapped R1 is then mapped in single-end mode (--pbat)
unmapped R2 is then mapped in single-end mode (in default = directional mode).
Single-end aligned R1 and R2 can then be methylation extracted normally as they should in theory map to different places in the genome anyway so don’t require attention to overlapping reads. Finally, the methylation calls from the PE and SE alignments can merged together before proceeding to the bismark2bedGraph or further downstream steps.

 

Edit March 2019: Please also see above the suggested mitigation approach for scBS-seq data using local alignments.

## Low mapping effiency of paired-end bisulfite-seq sample

This is a question that pops up every so often, and might have been discussed in numerous issues on Github or at www.seqanswers.com. 

**A few things to try out:**

- run adapter and quality trimming
- multi-genome alignment (e.g. using [FastQ Screen](https://www.bioinformatics.babraham.ac.uk/projects/fastq_screen/) (in `--bisulfite` mode!))
- align in single-end mode
- look at sequence composition plots in FastQC. Is there anything unusual/unexpected?



## Context change/discrepancy between Bismark coverage and genome-wide cytosine reports

A question that comes up every so often is: "Why do some positions have a different cytosine context between the coverage 
and genome-wide cytosine reports produced by `coverage2cytosine`? In rare(r) cases, the same position can even be present 
in several different contexts - how is this possible?"

**Answer**:

The Bismark coverage files contain every position that received a methylation call during the mapping step. There are certain 
cases where the cytosine context may change due to a deletion in the immediate downstream proximity of the cytosine, like in the following example:

`CAATGGGA` Here, the first C is in CHH context. If there was a deletion of AAT, one would would get an alignment like this:

`C---GGGA` here the context would effectively have changed from CHH to CG. 

At least for mammalian systems it is quite likely that such a change would also affect the methylation state of the cytosine involved 
 because CpGs are typically methylated whereas non-CG cytosines are largely completely unmethylated.

This context change only ever occurs for deletions immediate downstream of a cytosine, but *not insertions*. The reason for this is
that insertions are padded with `X` during the methylation call procedure, which would render the cytosine context `Unknown`.

`coverage2cytosine` on the other hand is fully reference genome-based, so it will go through the reference sequence
and check whether a cytosine was covered or not. The sequence context is purely assigned based on the reference sequence.

* For CpG context only (the default), it will therefore ‘miss’ out any of the calls where a deletion had changed the sequence
context from `CpG` to either `CHG` or `CHH`.

*  In `--CX` context: One may encounter cases where the context of a cytosine has changed, or much more rarely, where the very same position 
may have been called with different cytosine contexts in the intitial CHG, CHH and CpG-context files produced by the `bismark_methylation_extractor`.
If now, howevever, `--CX` was used for `bismark2bedGraph`
and `coverage2cytosine` (or directly during the `bismark_methylation_extractor` step), those *different* cytosine contexts will be merged 
again for that position, and will get the cytosine context assigned purely based on the reference sequence. In other words: If there truly 
was a cytosine context - that may or may not affect the methylation state - it would be, probably erroneously, attributed to the context 
provided by the reference sequence.




#### In a nutshell:

By design, Bismark generally bases its methylation call behavior on the reference genome, with the rationale being that
sequencing errors occur much more frequently than true polymorphisms in the genome sequence. The sequence context of insertions
would be dependent on the basecall accuracy of the inserted base(s), so we chose to call these methylation calls in `Unknown` context.
Deletions on the other hand are a very rare error in Illumina data, and we believe it should be fine to proceed with the default behavior
of calling changed sequence context because it is very likely that a change from CHH to CpG context, or vice versa, will really
also lead to a change in that residue's methylation state. 


If you work with **coverage files** in `CpG` context only you may miss a few positions that should be CpG positions according to
the reference sequence. On the other hand it may also contain a few newly gained CpGs positions that had a different context 
(`CHG` or `CHH`) in the reference sequence but where the read sequence says otherwise.

If you are working in `--CX` mode, or with the genome-wide report (or both), I am afraid it is a little more complicated...


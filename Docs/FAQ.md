# Bismark FAQs

This will be a collection of fairly common issues that arise fairly regularly. Started on 03 Sept 2019

## Issue 1) Thoughts and considerations regarding single-cell and PBAT libraries (September 18, 2019).

Bisulfite sequencing based on post-bisulfite adapter tagging (PBAT), including [scBS-seq](https://www.nature.com/articles/nmeth.3035) (single-cell Bisulfite-Seq) or [scNMT-seq](https://www.nature.com/articles/s41467-018-03149-4) (single-cell nucleosome, methylation and transcription sequencing) often suffers from a number of 'issues' that one should keep in mind when processing the data:

- **Special alignment mode**

PBAT libraries pull down strands of DNA that are complementary to the 'usual' DNA strands (OT and OB), and therefore normally require mapping in `--pbat` mode to the complementary strands (CTOT and CTOB). Single-cell techniques on the other hand undergo several rounds of DNA amplification after the bisulfite conversion step, which means they have to be aligned in `--non_directional` mode.

- **Mis-priming**

PBAT/ single-cell libraries typically have a very biased sequence composition at the 5' end of reads which reflects the non-randomness of the priming/ pull-down process. The symptoms and possible mitigation procedures have already been discussed in more detail here: [QCFail mis-priming issues](https://sequencing.qcfail.com/articles/mispriming-in-pbat-libraries-causes-methylation-bias-and-poor-mapping-efficiencies/). In conclusion, reads should be hard-trimmed from their 5'-ends before doing the alignments to prevent lower mapping effiency and mis-mapping/mis-calling methylation states. See also our trimming and processing notes for various library strategies in the [Bismark Manual](https://github.com/FelixKrueger/Bismark/tree/master/Docs#ix-notes-about-different-library-types-and-commercial-kits).

- **Chimeric reads**

The presence of chimeric reads has previously been discussed in more detail over at [QCFail](https://sequencing.qcfail.com/articles/pbat-libraries-may-generate-chimaeric-read-pairs/). Hybrid reads in this context are reads that would be considered discordant, i.e. where R1 and R2 align to completely different places in the genome, or where only one read aligns well but the other one doesn't.

Expanding on this idea, a recent [article in Bioinformatics](https://www.ncbi.nlm.nih.gov/pubmed/30859188 "Wu et al., 2019") also demonstrated that chimeric reads are the main problem for the low mapping efficiency in scBS-seq. In their article, Wu and colleagues demonstrate that the post-bisulfite based library construction protocol leads to a substantial amount of chimeric molecules as a result of recombination of genomic proximal sequences with ‘microhomology regions (MR)’. As a means to combat this problem the authors suggest a method that uses local alignments of reads that do not align in a traditional manner, and in addition remove MR sequences from these local alignments as they can introduce noise into the methylation data.


### Mapping strategies for paired-end data

#### PBAT

To rescue as much data from a paired-end PBAT library with low mapping efficiency as possible we sometimes perform the following method (affectionately termed “Dirty Harry” because it is not the most straight forward or cleanest approach): 

- We would recommend running 5′ clipping and trimming first (e.g. `trim_galore --clip_r1 6 --clip_r2 6 --paired *fastq.gz`, and run Bismark in paired-end mode with `--unmapped` specified

- Properly aligned PE reads should be methylation extracted while counting overlapping reads only once (which is the default)
- unmapped R1 is then mapped in single-end mode (`--pbat`)
- unmapped R2 is then mapped in single-end mode (in default = directional mode)

Single-end aligned R1 and R2 can then be methylation extracted normally as they should in theory map to different places in the genome anyway so don’t require attention to overlapping reads. Finally, the methylation calls from the PE and SE alignments can merged together before proceeding to the `bismark2bedGraph` or further downstream steps. A sample command for this would be:

```
bismark2bedGraph -o SE_and_PE_merged.bedGraph CpG*
```

If you are feeling adventurous you could also attempt using local alignments (option `--local` in Bismark) for either the paired-end step, or the second single-end step, or both. (Please also see our thoughts on [local alignments/ soft-clipping](https://sequencing.qcfail.com/articles/soft-clipping-of-reads-may-add-potentially-unwanted-alignments-to-repetitive-regions/).


#### Single-cell data

Because of the issues described above we have traditionally aligned single-cell data in single-end mode from the start (after hard-clipping biased positions, and adapter/quality trimming). With the more recent versions of Bismark one would in theory also have the option of using local alignments (`--local`), either on its own or in combination with the methods described above (Dirty Harry/ Wu et al.).

 
 ===
 
 

## Issue 2) Low mapping effiency of paired-end bisulfite-seq sample

This is a question that pops up every so often, and might have been discussed in numerous issues on Github or at seqanswers.com. 

**Here are some suggestions what you might want to try out:**

- Run adapter and quality trimming. Usually, a standard run through [Trim Galore](https://github.com/FelixKrueger/TrimGalore) will do the trick

- Perform a multi-genome alignment (e.g. using [FastQ Screen](https://www.bioinformatics.babraham.ac.uk/projects/fastq_screen/) (in `--bisulfite` mode!)). The "usual suspect" genomes should include PhiX, Lamda (or other spike-ins), as well as genomes that are typically handled in your lab/sequencing facility (don't forget to add the human genome!).

- Align the data in single-end mode. R1 typically needs to be aliged in default (=directional) mode, and R2 required `--pbat` to align. This will give you an indication if either R1 or R2 on its own aligns well, and if it is the paired-end nature of the data that is impairing high mapping efficiencies.

- Did you use a special library preparation technique, or an exotic commercial kit? Please see our [trimming and alignment recommendations here](https://github.com/FelixKrueger/Bismark/tree/master/Docs#ix-notes-about-different-library-types-and-commercial-kits)

- Look at sequence composition plots in FastQC. Is there anything unusual/unexpected that might prevent alignments?

If you still have any questions, feel free to send me an email with your issues. You might want to attach ~200K raw FastQ reads (compressed withy `gzip`) and mention the genome of interest in your email.



===



## Issue 3) Context change/discrepancy between Bismark coverage and genome-wide cytosine reports

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


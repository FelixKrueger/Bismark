# Bismark FAQs

This will be a collection of fairly common issues that arise fairly regularly. Started on 03 Sept 2019

- [Single-cell and PBAT libraries](#issue-1)
- [Low mapping efficiency of paired-end libraries](#issue-2)
- [Context change between coverage and cytosine reports](#issue-3)
- [Bisulfite conversion efficiency](#issue-4)


### Issue 1
## Thoughts and considerations regarding single-cell and PBAT libraries (September 18, 2019).

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
 
 

### Issue 2
## Low mapping effiency of paired-end bisulfite-seq sample

This is a question that pops up every so often, and might have been discussed in numerous issues on Github or at seqanswers.com. 

**Here are some suggestions what you might want to try out:**

- Run adapter and quality trimming. Usually, a standard run through [Trim Galore](https://github.com/FelixKrueger/TrimGalore) will do the trick

- Perform a multi-genome alignment (e.g. using [FastQ Screen](https://www.bioinformatics.babraham.ac.uk/projects/fastq_screen/) (in `--bisulfite` mode!)). The "usual suspect" genomes should include PhiX, Lamda (or other spike-ins), as well as genomes that are typically handled in your lab/sequencing facility (don't forget to add the human genome!).

- Align the data in single-end mode. R1 typically needs to be aliged in default (=directional) mode, and R2 required `--pbat` to align. This will give you an indication if either R1 or R2 on its own aligns well, and if it is the paired-end nature of the data that is impairing high mapping efficiencies.

- Relaxing the alignment stringency. The easiest way to accomolish this is to change the `--score_min` function, e.g from `--score_min L,0,-0.2` (the default) to `--score_min L,0,-0.6`. This might be useful for alignments against genomes which are not as polished as the human or mouse genomes, and might give you an indication whether the number of mismatches and/or indels has a detrimental impact on the alignment efficiency. Keep in mind though that laxer mapping parameters may take a longer time to align, and potentially return more ambiguous (and at some point incorrect alignments).

- Bisulfite libraries typcially contain fairly short fragments (compared to standard DNA libraries), with fragment sizes often peaking between 80 and 120bp in length. Very occasionally, library prepr protocols inlude longer fragments (longer than the default cutoff of 500bp). To test whether this is the case, you can temporarily increase the maximum fragment length to e.g. `-X 1000` (if this doesn't help then go back down to the default as increasing `-X` increases the alignment time.

- As a last resort to relaxing the mapping stringency, try to use `--local` mode. This comes with a number of caveats, but it might assist in determining whether or not there is usable sequence present in the library.

- Did you use a special library preparation technique, or an exotic commercial kit? Please see our [trimming and alignment recommendations here](https://github.com/FelixKrueger/Bismark/tree/master/Docs#ix-notes-about-different-library-types-and-commercial-kits)

- Look at sequence composition plots in FastQC. Is there anything unusual/unexpected that might prevent alignments?

If you still have any questions, feel free to send me an email with your issues. You might want to attach ~200K raw FastQ reads (compressed withy `gzip`) and mention the genome of interest in your email.



===



### Issue 3
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


### Issue 4
## Bisulfite conversion rate - Considerations

The methylation state of cytosines which were called as methylated is generally a mix of at least three factors: 

 - genuine methylation
 - bisulfite non-conversion
 - mis-mapping events (which tend to result in random, garbage methylation calls) 

Generally, the bisulfite conversion of unmethylated cytosine residues should be as complete as possible, so that any cytosine that is found non-converted can be assumed to be really methylated. However, if the bisulfite conversion was for whatever reason not very efficient (e.g. wrong temperature, incubation time too short, wrong salt concentrations, etc.) one has to expect spurious methylation calls, and hence noise in your results.

If the bisulfite coversion rate is found to be quite high, one might opt to repeat the experiment altogether (if that is possible at all) rather than dealing with potentially quite noisy data. If repeating the experiment is not an option, you are frankly somewhat limited with what you can do downstream to alleviate the problem. You could try to identify and remove reads that completely 'evaded' conversion using [`methylation_consistency`](https://github.com/FelixKrueger/Bismark/tree/master/Docs#x-concordance-of-methylation-calls-across-bisulfite-reads), or bear the non-conversion value in mind when interpreting the data. Simply subtracting the conversion error from the results doesn't seem to be a great option, as in our experience different contexts are affected differently (CpG methylation seems to be less affected than non-CG content, probably because it tends to have more methylation generally?)

To judge the bisulfite conversion rate and gauge whether this is likely a factor that has to be taken into consideration, one has several options: 


**Look for lowest methylation genome-wide in non-CG context**

If you don't have any spike-in controls (see below), one could simply look at the lowest methylation levels you see anywhere in your experiment. In mammalian systems, where the rate of methylation in non-CG context is often very low (\*), it may be enough to just look at the methylation levels in non-CG context. As an example, if you see a general methylation of 0.3% in non-CG context over many millions of methylation calls, then the bisulfite conversion must have been at least 99.7% efficient. 

**\*)** A value of 0.3% methylation in non-CG context does not necessarily mean that this is the exact number of methylated cytosines that did not get converted, see above (it may have been 99.95% efficient for all we know...), but it certainly can't have been any worse.... 

Other organisms, such as plants, may display elevated and different levels of methylation in non-CG context, so judging the conversion efficiency in this way may not be possible.

**Look for lowest methylation genome-wide elsewhere**

It may also be possible to look at more specific regions of the genome to find the lowest methylation possible. This could be CpG islands (which tend be be lowly methylated even in CpG context), the mitochondria (chrMT/chrM), or in plants reads aligned to the chloroplast sequence (which should not be methylated either). Whether mitochondria can be regarded as completely unmethylated or displaying some forms of methylation is probably still debatable... On top of this there could be tertiary structures or other conversion artefacts preventing some residues from getting bisulfite converted, as has been nicely demonstrated for the [methylation of mitochondria](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC5671948/).


**Spike-in controls**

Some experimentalists choose to add spike-in controls into their experiment to measure the rate of (non-)conversion in their experiment. This may include unmethylated (e.g. Lambda, phiX, M13) or methylated controls (e.g. in-vitro methylated pUC19), or even oligonucleotides with known methylation states.

These spike-in controls do normally not align to any other genome, so one can index a spike-in genome and run a separate alignment to the spike-in genome in addition to the alignment against your genome of interest (without having to worry about cross-mapping artefacts). 

Instead of running two consecutive rounds of alignments, one to the genome of interest and then second one against the spike-in sequence, another possibility would be to include the spike-in sequence as an additional 'chromosome' to the genome of interest, and then carry our the genome indexing once more. In this way you should be able to get both genomic alignments and conversion rates in a single step. 

This is not to say that the spike-in sequences will always be a useful control, almost more often than not the spike-ins seem to behave in a slightly weird way, e.g. the conversion efficiencies appear to be (slighlty) worse than what one sees for methylation in non-CG context of the genome of interest. In the end, one can often only take the values of the spike-in controls with a pinch of salt, acknowledge them - and move on regardless :)


============

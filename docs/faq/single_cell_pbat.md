# Single cell PBAT

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

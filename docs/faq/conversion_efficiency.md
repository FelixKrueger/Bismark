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

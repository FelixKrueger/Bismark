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

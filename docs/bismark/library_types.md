# Library types

Here is a table summarising general recommendations for different library types and/or different commercially available kits. Some more specific notes can be found below.

<table>
    <thead>
        <tr>
            <th align="left">Technique</th>
            <th align="center">5' Trimming</th>
            <th align="center">3' Trimming</th>
            <th align="center">Mapping</th>
            <th align="center">Deduplication</th>
            <th align="center">Extraction</th>
    </tr>
    </thead>
    <tbody>
        <tr>
            <td align="left">BS-Seq</td>
            <td align="center"><g-emoji alias="white_large_square" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2b1c.png" ios-version="6.0">⬜️</g-emoji></td>
            <td align="center"><g-emoji alias="white_large_square" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2b1c.png" ios-version="6.0">⬜️</g-emoji></td>
            <td align="center"><g-emoji alias="white_large_square" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2b1c.png" ios-version="6.0">⬜️</g-emoji></td>
            <td align="center"><g-emoji alias="white_check_mark" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2705.png" ios-version="6.0">✅</g-emoji></td>
            <td align="center"><code>--ignore_r2 2</code></td>
        </tr>
        <tr>
            <td align="left">RRBS</td>
            <td align="center"><code>--rrbs</code> (R2 only)</td>
            <td align="center"><code>--rrbs</code> (R1 only)</td>
            <td align="center"><g-emoji alias="white_large_square" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2b1c.png" ios-version="6.0">⬜️</g-emoji></td>
            <td align="center"><g-emoji alias="x" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/274c.png" ios-version="6.0">❌</g-emoji></td>
            <td align="center"><g-emoji alias="white_large_square" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2b1c.png" ios-version="6.0">⬜️</g-emoji></td>
        </tr>
        <tr>
            <td align="left">RRBS (NuGEN Ovation)</td>
            <td align="center">special processing</td>
            <td align="center">special processing</td>
            <td align="center"><g-emoji alias="white_large_square" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2b1c.png" ios-version="6.0">⬜️</g-emoji></td>
            <td align="center"><g-emoji alias="x" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/274c.png" ios-version="6.0">❌</g-emoji></td>
            <td align="center"><code>--ignore_r2 2</code></td>
        </tr>
        <tr>
            <td align="left">PBAT</td>
            <td align="center">6N / 9N</td>
            <td align="center">(6N / 9N)</td>
            <td align="center"><code>--pbat</code></td>
            <td align="center"><g-emoji alias="white_check_mark" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2705.png" ios-version="6.0">✅</g-emoji></td>
            <td align="center"><g-emoji alias="white_large_square" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2b1c.png" ios-version="6.0">⬜️</g-emoji></td>
        </tr>
        <tr>
            <td align="left">single-cell (scBS-Seq)</td>
            <td align="center">6N</td>
            <td align="center">(6N)</td>
            <td align="center"><code>--non_directional</code>; single-end mode</td>
            <td align="center"><g-emoji alias="white_check_mark" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2705.png" ios-version="6.0">✅</g-emoji></td>
            <td align="center"><g-emoji alias="white_large_square" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2b1c.png" ios-version="6.0">⬜️</g-emoji></td>
        </tr>
        <tr>
            <td align="left">TruSeq (EpiGnome)</td>
            <td align="center">8 bp</td>
            <td align="center">(8 bp)</td>
            <td align="center"><g-emoji alias="white_large_square" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2b1c.png" ios-version="6.0">⬜️</g-emoji></td>
            <td align="center"><g-emoji alias="white_check_mark" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2705.png" ios-version="6.0">✅</g-emoji></td>
            <td align="center"><g-emoji alias="white_large_square" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2b1c.png" ios-version="6.0">⬜️</g-emoji></td>
        </tr>
        <tr>
            <td align="left">Accel-NGS (Swift)</td>
            <td align="center">R1: 10, R2:15bp</td>
            <td align="center">(10 bp)</td>
            <td align="center"><g-emoji alias="white_large_square" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2b1c.png" ios-version="6.0">⬜️</g-emoji></td>
            <td align="center"><g-emoji alias="white_check_mark" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2705.png" ios-version="6.0">✅</g-emoji></td>
            <td align="center"><g-emoji alias="white_large_square" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2b1c.png" ios-version="6.0">⬜️</g-emoji></td>
        </tr>
        <tr>
            <td align="left">Zymo    Pico-Methyl</td>
            <td align="center">10 bp</td>
            <td align="center">(10 bp)</td>
            <td align="center"><code>--non_directional</code></td>
            <td align="center"><g-emoji alias="white_check_mark" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2705.png" ios-version="6.0">✅</g-emoji></td>
            <td align="center"><g-emoji alias="white_large_square" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2b1c.png" ios-version="6.0">⬜️</g-emoji></td>
        </tr>
        <tr>
            <td align="left">EM-seq (NEB)</td>
            <td align="center">10 bp</td>
            <td align="center">10 bp</td>
            <td align="center"><g-emoji alias="white_large_square" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2b1c.png" ios-version="6.0">⬜️</g-emoji></td>
            <td align="center"><g-emoji alias="white_check_mark" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2705.png" ios-version="6.0">✅</g-emoji></td>
            <td align="center"><g-emoji alias="white_large_square" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2b1c.png" ios-version="6.0">⬜️</g-emoji></td>
        </tr>
    </tbody>
</table>

- <g-emoji alias="white_large_square" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2b1c.png" ios-version="6.0">⬜️</g-emoji> -
  Default settings (nothing in particular is required, just use Trim Galore or Bismark default parameters)
- <g-emoji alias="white_check_mark" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/2705.png" ios-version="6.0">✅</g-emoji> -
  Yes, please!
- <g-emoji alias="x" fallback-src="https://assets-cdn.github.com/images/icons/emoji/unicode/274c.png" ios-version="6.0">❌</g-emoji> -
  No, absolutely not!

**5' Trimming** can be accomplished with Trim Galore using:

`--clip_r1 <NUMBER>` (Read 1) or

`--clip_r2 <NUMBER>` (Read 2)

**3' Trimming** can be accomplished with Trim Galore using:

`--three_prime_clip_r1 <NUMBER>` (Read 1) or

`--three_prime_clip_r2 <NUMBER>` (Read 2).

## Specific library kit notes

### RRBS

RRBS is a specialised technique to only look at CpG rich regions of the genome by using the restriction enzyme MspI (please see this [RRBS Guide](http://www.bioinformatics.babraham.ac.uk/projects/bismark/RRBS_Guide.pdf) for some more specifics regarding data processing). For reasons explained in the RRBS-guide, the second last position of all reads before reading into the Illumina adapter exhibits an artificially (not methylated) methylation state as a result of the end-repair reaction. The option `--rrbs` within Trim Galore removes 2 extra bases whenever adapter contamination has been detected. This 3' end trimming that needs to be carried out for single-end runs or Read 1 of paired-end libraries. Read 2 of paired-end libraries is however not affected by this 3' bias, but instead the first couple of positions on the 5' end of Read 2 suffer from the read-through problem as Read 1 (Read 2 is a mere copy of Read 1), so Read 2 needs to have the **first** 2 bp removed instead. As of the current development version of [Trim Galore](https://github.com/FelixKrueger/TrimGalore) (v0.4.2_dev; 12/16/2016) the option `--rrbs` removes:

- 2 bp from the 3' end of single-end and Read 1 of paired-end reads in addition to adapter contamination, and
- 2 bp from the 5' end of Read 2 of paired-end reads

### RRBS NuGEN Ovation Methyl-Seq System

([Manufacturer's page](http://www.nugen.com/products/ovation-rrbs-methyl-seq-system))

Owing to the fact that the NuGEN Ovation kit attaches a varying number of nucleotides (0-3) after each MspI site Trim Galore should be run _WITHOUT_ the option `--rrbs`. The trimming is accomplished in a subsequent diversity trimming step afterwards, please see the manufacturer's manual for more details.

### PBAT

The amount of bases that need to be trimmed from the 5' end depends on the length of the oligo used for random priming, which - as we know - isn't all that random, and in fact causes [misalignments and methylation biases](https://sequencing.qcfail.com/articles/mispriming-in-pbat-libraries-causes-methylation-bias-and-poor-mapping-efficiencies/). While the original PBAT paper used 4N oligoes, these days 6N or 9N seem to be most common. Please also see the section _3' Trimming in general_ below.

### Single-cell

The [scBS-Seq method](http://www.nature.com/nmeth/journal/v11/n8/full/nmeth.3035.html) uses a PBAT-type protocol but employs five rounds of sequence capture and elongation to amplify the starting material so all four different bisulfite strands (OT, CTOT, OB, CTOB) are sequenced. Since 6N oligos are used to for the random priming step, 6 bp need to be removed from the 5' ends. Since scBS and PBAT libraries tend to result in [chimaeric fragments](https://sequencing.qcfail.com/articles/pbat-libraries-may-generate-chimaeric-read-pairs/) we tend to treat scBS-Seq as single-end reads always. Please also see the section _3' Trimming in general_ below.

### TruSeq DNA-Methylation Kit (formerly EpiGnome)

([Manufacturer's page](http://www.illumina.com/products/by-type/sequencing-kits/library-prep-kits/truseq-dna-methylation.html))
This Illumina kit (previously known as EpiGnome kit from epicentre) also employs a post-bisulfite strategy using 6N oligos, but in contrast to the PBAT technique only the standard original top and bottom strands (OT and OB) are sequenced, meaning that Bismark can be run in default (= directional) mode. Even though the random priming is performed with 6N oligoes we often saw that the methylation bias extends to 7 or 8 bp, so trimming 8 bp off the 5' end(s) is recommended initially. Please do have a look at the M-bias plots nevertheless to see of more bases need removing/ignoring during the methylation extraction process. Please also see the section _3' Trimming in general_ below.

### Zymo Pico Methyl-Seq

([Manufacturer's page](https://www.zymoresearch.com/epigenetics/dna-methylation/genome-wide-5-mc-analysis/pico-methyl-seq-library-prep-kit))
The Pico Methyl-Seq kit also uses a random priming step similar to the PBAT // single-cell methods above. This kit uses random tetramers (4N) for the amplification step, however the biases seen in the base composition and M-bias plots indicate that one should trim off at least the first 10 bp from each read. This kit performs three rounds of amplification which yields non-directional libraries (similar to the scBS-Seq protocol), so all four different bisulfite strands (OT, CTOT, OB, CTOB) are present in the library. According to the manufacturer, the library construction is designed for a starting input material of 100 ng, but can be scaled up or down (to 100 pg). Please also see the section _3' Trimming in general_ below.

### Swift

[Manufacturer's page](https://swiftbiosci.com/products/accel-ngs-methyl-seq-dna-library-kit/)
The Accel-NGS Methyl-Seq protocol uses Adaptase technology for capturing single-stranded DNA in an unbiased (again, not that unbiased actually...) manner. Also here, the first ~10-15 bp show extreme biases in sequence composition and M-bias, so trimming off at _least_ 10 bp is advisable (please check the M-bias plot if even more is needed). Please also see the section _3' Trimming in general_ below.

### EM-seq (NEB)

[EM-seq protocol](https://www.neb.com/protocols/2019/03/28/protocol-for-use-with-large-insert-libraries-470-520-bp-e7120).
The Enzymatic Methyl-seq (EM-seq) protocol uses different enzymes to detect 5mC and 5hmC in a non-bisulfite dependent manner that allows capturing longer fragments and working with very low levels of starting material ([EM-seq paper](https://genome.cshlp.org/content/31/7/1280.full)). NEB internally don't trim more than 5bp from each read, but as discussed in [this thread](https://github.com/FelixKrueger/Bismark/issues/509), the recommended conservative trimming parameters are:

```
--clip_R1 10 --clip_R2 10 --three_prime_clip_R1 10 --three_prime_clip_R2 10
```

### Random priming and 3' Trimming in general

As we have seen before, the random priming of post-bisulfite methods (such as PBAT, scBS-Seq, EpiGnome, Pico Methyl, Accel etc.) introduces [errors, indels and methylation biases](https://sequencing.qcfail.com/articles/mispriming-in-pbat-libraries-causes-methylation-bias-and-poor-mapping-efficiencies/) that may detrimentally affect your mapping efficiencies and methylation calls. These problems are fairly easy to spot at the 5' ends of reads because all reads will equally suffer from the problems at the same positions at the start (5' end) of reads.
The same problems of random priming (indels, mispriming) will however most likely occur on both sides of the fragment to be sequenced, but it is doubtful that one would be able to spot these problems on the 3' end of reads because the problems would be expected on the 3' end of reads just before reading through into the adapter, and this may occur

- at different positions in the read (depending on how short the fragment was)
- at different positions within the read because of quality trimming in addition to adapter read-through contamination
- not at all within the read length (whenever a fragment is longer than the sequenced read length)
- at the 3' end even without hitting the adapter (i.e. just before the adapter)

I guess there is a trade-off between accepting that a certain proportion of the reads may have a few biased biased positions towards their 5' ends, and preemptively trimming the 3' end by the same amount of bases as the 5' end. As a general rule it is probably safe to say that the shorter the average insert size of a library - the more of a problem the bias is. We have e.g. seen Pico Methyl libraries where ~80% of all fragments were shorter than 100bp, so a 2x125bp run would most likely be affected by the random priming bias on the 5' and 3' ends in nearly all fragments sequenced. We realise that trimming off say 10 bp from the 5' end and 3' end of a 100 bp read already removes 20% of the actually sequenced data, but this is the price you have to pay for using post-bisulfite kits...

For these reasons we have put the _3' Trimming_ values in the table above in _(parentheses)_ as a reminder that you **should** perform 3' trimming of the data as well.

---
title: "Alignment"
description: "This step represents the actual bisulfite alignment and methylation calling part. Bismark requires the user to specify only two things:"
---

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

:::note[Third-party tool]
Genozip is a commercial product (free for academic use, registration required) and is not part of Bismark.
:::

[Genozip](https://www.genozip.com) v14 and above supports compressing Bismark-generated BAM files. A benchmark on a Bismark paired-end test file showed roughly 7× compression versus BAM and more than 2× versus CRAM 3.1 (see [issue #526](https://github.com/FelixKrueger/Bismark/issues/526)). Install with `conda install genozip`.

### Data visualisation

To see the location of the mapped reads the Bismark output file can be imported into a genome viewer, such as SeqMonk, using the chromosome, start and end positions (this can be useful to identify regions in the genome which display an artefactually high number of aligned reads). The alignment output can also be used to apply post-processing steps such as de-duplication (allowing only 1 read for each position in the genome to remove PCR artefacts) or filtering on the number of bisulfite conversion related non-bisulfite mismatches \* (please note that such post-processing scripts are not part of the Bismark package).

:::tip

Bisulfite conversion related non-bisulfite mismatches are mismatch positions which have a C in the BS-read but a T in the genome; such mismatches may occur due to the way bisulfite read alignments are performed. Reads containing this kind of mismatches are not automatically removed from the alignment output in order not to introduce a bias for methylated reads.

It should be noted that, even though no methylation calls are performed for these positions, reads containing bisulfite conversion related non-bisulfite mismatches might lead to false alignments if particularly lax alignment parameters were specified.
:::
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

:::note

This has been previously only been mentioned in the release notes here: <https://github.com/FelixKrueger/Bismark/releases/tag/0.22.0>
:::
Expanding on our observation that single-cell BS-seq, or PBAT libraries in general, can [generate chimeric read pairs](https://sequencing.qcfail.com/articles/pbat-libraries-may-generate-chimaeric-read-pairs/), a publication by [Wu et al.](https://www.ncbi.nlm.nih.gov/pubmed/30859188) described in further detail that intra-fragment chimeras can hinder the efficient alignment of single-cell BS-seq libraries. In there, the authors described a pipeline that uses paired-end alignments first, followed by a second, single-end alignment step that uses local alignments in a bid to improve the mapping of intra-molecular chimeras. To allow this type of improvement for single-cell or PBAT libraries, Bismark also allows the use of local alignments.

**Please note** that we still do not recommend using local alignments as a means to _magically_ increase mapping efficiencies (please see [here](https://sequencing.qcfail.com/articles/soft-clipping-of-reads-may-add-potentially-unwanted-alignments-to-repetitive-regions/)), but we do acknowledge that PBAT/scBSs-seq/scNMT-seq are exceptional applications where local alignments might indeed make a difference (there is only so much data to be had from a single cell...).
We didn't have the time yet to set more appropriate or stringent default values for local alignments (suggestions welcome), nor did we investigate whether the methylation extraction will require an additional `--ignore` flag if a read was found to the be soft-clipped (the so called 'micro-homology domains'). This might be added in the near future.

## Combined-index alignment (opt-in)

The [Bismark Rust suite](/Bismark/installation/) adds an opt-in **combined-index** alignment mode to `bismark`. Instead of running 2 (directional) or 4 (non-directional) separate per-strand aligner instances, it aligns against a **single combined C→T + G→A index** in one both-strands pass per read-conversion.

It is **opt-in, never-silent, and concordance-gated — NOT byte-identical** to the faithful per-strand default (a small, benign churn vs the per-strand oracle: directional ~0.013%, non-directional ~0.022–0.044%, pbat ~0.044%, almost all unique↔ambiguous flips). The faithful default path is unchanged; combined mode is used only when you ask for it. minimap2 and `--multicore` are not supported in combined mode (they fail loudly).

### Is combined mode advisable?

Combined mode trades byte-identity for speed (and, for non-directional libraries, memory):

- **Non-directional — yes, a clear win, and now the default.** Non-directional combined runs the sequential low-memory model by default — about a third less memory than the faithful default (~11 vs 16 GB), and faster on large/bandwidth-bound genomes. (On a small index with many cores the concurrent `--combined_index_parallel` model can be faster; the memory win holds regardless.)
- **Directional / pbat — a speed-only win.** Roughly 1.3× faster, but a single combined index is slightly *larger* than the two per-strand indices it replaces, so it is not a memory saving.
- **If you need byte-identical output to Perl** (reproducing published results, or a strict validated pipeline) — stay on the faithful per-strand default. Combined mode is never byte-identical (benign churn ~0.1%, almost entirely unique↔ambiguous flips; actual mis-placement ~0.005%).

See [Which mode to choose](#which-mode-to-choose) below for the per-mode wall-clock and memory numbers.

**Build the combined index once** (genome preparation adds a `Bisulfite_Genome/Combined/` directory):

```bash
bismark prepare --combined_genome /path/to/genome/   # add --hisat2 for a HISAT2 combined index
```

**Coverage:** single-end and paired-end · Bowtie 2 and HISAT2 · directional / non-directional / pbat.

| Flag | What it does | Scope |
|------|--------------|-------|
| `--combined_index` | One both-strands pass per read-conversion. **Non-directional now defaults to the sequential low-memory model** (below) — one index resident, ~½ the peak memory. | SE+PE · Bowtie 2+HISAT2 · dir/non-dir/pbat |
| `--combined_index_sequential` | The non-directional default (since v3.x): the two both-strands passes run **one at a time** (one index resident → ~½ the peak memory). **BAM byte-identical** to the concurrent model (a). The flag is retained as an explicit selector — you no longer need to pass it. | non-dir · SE+PE · Bowtie 2+HISAT2 |
| `--combined_index_parallel` | Opt into the **concurrent** model (a): the two both-strands passes run at once (two indexes co-resident, ~2× peak RAM). BAM byte-identical to the sequential default; can be faster on a small index with many cores. | non-dir · SE+PE · Bowtie 2+HISAT2 |
| `--combined_index_single_pass` | One pass over conversion-tagged interleaved reads (one index load). **Not byte-identical / not decision-equivalent** to the parallel run (the read-name tag perturbs Bowtie 2's RNG) — ground-truth-validated, never the default. | non-dir · SE+PE · Bowtie 2 only |

```bash
# directional, combined index, one both-strands pass:
bismark --combined_index --genome /path/to/genome/ -p 16 reads.fq.gz

# non-directional: the sequential low-memory model is now the DEFAULT (no extra flag):
bismark --combined_index --non_directional --genome /path/to/genome/ -p 16 reads.fq.gz
# ...pass --combined_index_parallel to opt into the concurrent (2× RAM) model instead.
```

### Which mode to choose

Figures are a real 10M-read WGBS GRCh38 run (single-end, 16-core budget); the full graphs and method are on the [benchmarks page](/Bismark/rust/benchmarks/#combined-index-modes).

**Non-directional** — this is where the memory choice matters:

| Mode | Wall | Peak RAM | Byte-identical? |
|---|---|---|---|
| Faithful 4-instance (default) | 477 s | 16 GB | — (it is the oracle) |
| **`--combined_index` non-dir default** (sequential) | 400 s | **11 GB** | no vs Perl (benign churn); **BAM == concurrent model (a)** |
| `--combined_index_parallel` (concurrent) | 434 s | 19 GB | no vs Perl (benign churn); BAM == sequential |
| `--combined_index_single_pass` | **377 s** | 11 GB | no (read-name-tag RNG) |

**The non-directional combined default is now the sequential model** — the only mode that is both **lower memory (~11 GB vs 16 GB, about a third less)** and **faster** than the faithful default on this run, while its **BAM stays byte-identical to the concurrent model (a)**. Pass `--combined_index_parallel` to force the concurrent model (~2× RAM; can win on a small index with many cores), or `--combined_index_single_pass` for the fastest wall time if you can accept a non-byte-identical (still ground-truth-validated) result. (The `*_report.txt` line and stderr banner name whichever model ran.)

**Directional / pbat:** plain `--combined_index` is fastest (one index load — e.g. directional 176 s vs 229 s faithful) but is **not** a memory saving: one large combined index is about the size of the two per-strand indices it replaces.

## Unaligned BAM (uBAM) input

The Rust `bismark` aligner accepts an **unaligned BAM** as read input in addition to FASTQ/FASTA — useful for the increasingly common uBAM raw-read container (ONT/PacBio basecaller output, 10x, archival). It is **auto-detected by the file's BAM magic bytes** (not the extension), so no extra flag is needed:

```bash
# single-end uBAM
bismark --genome /data/genomes/GRCh38/ reads.bam

# paired-end: a single name-collated uBAM (both mates) passed positionally —
# auto-detected as paired and split into R1/R2 automatically
bismark --genome /data/genomes/GRCh38/ pairs.bam
```

The reads are extracted (equivalent to `samtools fastq`) and fed into the unchanged bisulfite-convert → align → merge pipeline, so the result is **byte-identical to running Bismark on the corresponding `samtools fastq` output** (validated on real GRCh38 data for single- and paired-end across directional / non-directional / pbat). All library types and aligner backends work; it is purely an input front-end.

Notes:
- **Paired-end uBAMs must be name-collated** (mates adjacent, as `samtools fastq` requires); a desynchronised pairing fails loudly. Run `samtools collate` first if needed.
- A paired-end uBAM is passed as **one positional file**, not via `-1`/`-2`; a uBAM supplied through `-1`/`-2` is rejected with guidance.
- uBAM input is incompatible with `-f`/`--fasta` (BAM carries qualities → FASTQ), and — like FASTQ/FASTA — paired-end is unsupported for the minimap2/rammap backends.

## BINSEQ (`.vbq` + `.cbq`) input

The Rust `bismark` aligner also accepts an [Arc Institute **BINSEQ**](https://github.com/arcinstitute/binseq) file as read input, decoded **in-process** (via the `binseq` crate — no `bqtools` needed at runtime) and auto-detected by extension. Both the verbose `.vbq` and the columnar `.cbq` variant are supported:

```bash
# single-end VBQ (or CBQ)
bismark --genome /data/genomes/GRCh38/ reads.vbq
bismark --genome /data/genomes/GRCh38/ reads.cbq

# paired-end: a single BINSEQ file carries BOTH mates per record —
# auto-detected as paired and split into R1/R2 automatically
bismark --genome /data/genomes/GRCh38/ pairs.vbq
```

The reads are decoded to a temporary FASTQ matching `bqtools decode` and fed into the unchanged bisulfite-convert → align → merge pipeline, so the result is **identical to running Bismark on the corresponding `bqtools decode` output** (the same equivalence contract as the uBAM front-end above). All library types and aligner backends work; it is purely an input front-end.

Notes:
- **VBQ and CBQ are supported; `.bq` is not.** `.bq` is recognised but **rejected with a clear message** — it carries no per-read quality or names and cannot be faithfully aligned. Convert it to VBQ/CBQ or FASTQ first. (`.cbq` decoding requires `binseq >= 0.9.3`, which fixed an upstream `N`-decode defect; the Rust suite pins it.)
- A VBQ **must carry per-read quality scores and headers** (encode with `bqtools encode` preserving both). A quality-less or name-less VBQ is rejected rather than silently filled — Bismark needs real qualities and original read names (output QNAMEs).
- A paired-end VBQ is passed as **one positional file**, not via `-1`/`-2`; `.vbq` via `-1`/`-2` is rejected, and `.vbq` + `-f`/`--fasta` is rejected (it carries qualities → FASTQ).
- **Prebuilt release binaries and the container image support `.vbq`/`.cbq` out of the box.** A *from-source* build enables it with the `binseq-input` Cargo feature — `cargo build -p bismark-aligner --features binseq-input` (or add it to the suite `cargo install`). A default source build still recognises a BINSEQ file and exits with a clear "compiled without BINSEQ support" message rather than mis-reading it.

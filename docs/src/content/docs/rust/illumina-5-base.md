---
title: "Illumina 5-Base"
description: "How to run Bismark's Illumina 5-Base (5mC to T) mode, what it supports, and the concordance stability contract for a non-byte-identical, DRAGEN-validated path."
---

Illumina 5-Base (the "5-Base DNA Prep") is the chemical **inverse** of bisulfite. The
enzyme converts **5-methylcytosine to thymine** and leaves unmethylated cytosine intact,
so the library keeps its full complexity and the raw reads align to the **unconverted**
genome with a standard aligner. Bismark then calls methylation at **inverted polarity**: a
read `T` at a genomic `C` is methylated (`Z`), a read `C` is unmethylated (`z`).

The mode is opt-in behind `--illumina_5base` and is **paired-end only** (the 5-Base
library is paired-end and directional; single-end input is rejected at startup).

## Stability contract

5-Base is **not byte-identical** to anything: Perl Bismark has no 5-Base path, so there is
no byte-for-byte oracle. It is instead **concordance-gated**, the same contract as the
`--rammap` and `--combined_index` paths:

- **Supported in GA (3.0.0):** `--illumina_5base` (core), `--five_base_duplex`,
  `--five_base_consensus`, `--five_base_consensus_from_bam`, `--five_base_deconvolution`. Each is validated against Illumina
  DRAGEN on the real NA12878 demo at full depth, with the default minimap2 engine, by the metric appropriate to what it
  produces: **core** by per-CpG concordance with DRAGEN's (deduplicated, full-depth) CX
  report (deduplicated, iso-DRAGEN) — **r approximately 0.99 over 55M CpGs (0.998 at
  cov>=10), 99.3% call-agreement at cov>=10 (98.8% at cov>=1)**; **deconvolution**
  by variant precision/recall vs DRAGEN's germline VCF — **90.3% / 93.4%**; **duplex /
  consensus** (per-molecule collapse — DRAGEN builds a duplex-consensus methylation track
  only for UMI/enrichment kits, so there is no WGS consensus-CX to correlate against) by
  per-strand **mean-methylation agreement** with DRAGEN (~47.9% vs ~48%, bias-free). All
  four are additionally held to a reproducible lambda/pUC19 spike-in gate that runs in CI
  with no proprietary data.
- **Preview:** `--five_base_umi_qname` (duplex UMI taken from the read name). It works and
  is exercised on real data, but is not yet held to the same gated contract.
- **Unchanged:** the faithful bisulfite / EM-seq suite is **byte-frozen**. Every legacy
  methylation call still reduces to the exact Perl v0.25.1 output (the `perl-oracle` CI
  gate stays green); turning 5-Base on never perturbs a bisulfite run.

Because it is concordance-gated rather than byte-identical, every 5-Base path is opt-in and
**never silent**: an unsupported combination (single-end, `--non_directional`, `--pbat`,
`--combined_index`) is rejected loudly rather than silently degraded.

## Known limitation: CpG-dense methylated regions

5-Base reads align to the unconverted genome, so each methylated CpG (read `T`, genome `C`) is a
mismatch to the aligner. A read from a CpG-dense, heavily methylated region can carry enough of
them that it no longer aligns, while the same fragment unmethylated aligns normally, so methylated
reads are lost there selectively. This applies to both engines, minimap2 (the default) and Bowtie 2.

On the NA12878 and HG002 demo data, compared with a methylation-invariant alignment of the same
reads and with DRAGEN:

- CpGs in sparse and moderate CpG density (up to about 9 CpGs per 151 bp, around 85% of CpGs)
  are unaffected.
- Partly methylated CpGs at 15 or more CpGs per 151 bp read 6–11 points low.
- The densest, most methylated CpGs (15 or more per 151 bp, ≥80% methylated) keep only 40–50% of
  the expected coverage, and 20–35% of them get no coverage at all.
- Global CpG methylation (the report's "% CpG methylated") reads about 1–1.6 points low.

The regions affected are methylated CpG islands: imprinted regions, CpG islands on the inactive X,
and hypermethylated islands in cancer or cfDNA samples. If your analysis depends on those, expect
them to be under-covered and under-called. Filtering 5-Base BAMs by MAPQ makes the coverage loss
worse, because methylated reads receive lower MAPQ.

The genome-wide concordance with DRAGEN under [Validation](#validation) still holds: these regions
are too small a share of all CpGs to show up in it.

**If those regions matter, use [`--bwamem3`](#bwa-mem3---bwamem3).** bwa-mem3's methylation-aware
mode does not count a read `T` at a genomic `C` as a mismatch. On the same two samples it keeps
1.01–1.02× DRAGEN's depth in the densest, most methylated CpGs (minimap2: 0.48–0.50×), and only
0.5–1.1% of them get no coverage (minimap2: 20–23%).

## Running it

Directional, paired-end, against the unconverted genome. minimap2 (`-x sr`) against the
genome FASTA is the default engine; bowtie2 or hisat2 work against a NORMAL (unconverted)
index supplied with `--five_base_index`. minimap2 is the default: it needs no prebuilt
index, and in a head-to-head comparison against DRAGEN no alternative was clearly
better (see [Choosing an engine](#choosing-an-engine)). Choose `--bowtie2` or
`--hisat2` when you need their tuning options or want to match the aligner of your
bisulfite runs. Choose [`--bwamem3`](#bwa-mem3---bwamem3) for methylated CpG islands, if you
can afford its index and memory.

`--genome` needs only to contain the genome FASTA — the reference the methylation calls are
made against. There is no `bismark_genome_preparation` step for 5-Base, because no path here
reads the bisulfite-converted indexes.

`--five_base_index` is checked before the run starts, so a missing or incomplete index fails
immediately rather than part-way through alignment. As with bowtie2 and hisat2 themselves, a
basename that is not found is also looked up under `$BOWTIE2_INDEXES` / `$HISAT2_INDEXES`.

```sh
# core: align + inverted-polarity methylation calls
bismark --illumina_5base --genome /path/to/GRCh38 -1 R1.fastq.gz -2 R2.fastq.gz

# bowtie2/hisat2 instead of minimap2 (build a plain index once with bowtie2-build)
bismark --illumina_5base --bowtie2 --five_base_index /path/to/normal_idx \
        --genome /path/to/GRCh38 -1 R1.fastq.gz -2 R2.fastq.gz

# bwa-mem3 (methylation-aware; build its index once with `bwa-mem3 index --meth genome.fa`)
bismark --illumina_5base --bwamem3 --five_base_index /path/to/bwm3/genome.fa \
        --genome /path/to/GRCh38 -1 R1.fastq.gz -2 R2.fastq.gz
```

### Bowtie 2 and HISAT2 tuning

Both engines accept `-N`, `-L`, `-D`, `-R`, `--rdg`, `--rfg`, `--score_min`,
`-I`/`--minins`, and `-X`/`--maxins` in 5-Base mode. Quality-encoding flags
(`--phred33-quals` or `--phred64-quals`) and `--quiet` are also passed through. The default
linear score threshold is `L,0,-0.6`. The default maximum insert is 800 bp for Bowtie 2
and 500 bp for HISAT2, which can cut off the longest fragments of a 5-Base library (see
[Choosing an engine](#choosing-an-engine)). With Bowtie 2, an `-I` above the maximum
insert is an error. `--dovetail` has no effect in 5-Base mode, because dovetailed overhangs
carry non-genomic bases that read as methylation; `--no_dovetail` only flips the TLEN sign
of OB pairs whose mates cover the same span. For short fragments, for example:

```sh
bismark --illumina_5base --bowtie2 --five_base_index /path/to/normal_idx \
        --genome /path/to/GRCh38 -1 R1.fastq.gz -2 R2.fastq.gz \
        -L 10 -N 1 -I 10 -X 1000 --score_min L,0,-0.2
```

Bowtie 2 `--local` enables local alignment and defaults to `--score_min G,20,8`;
an explicit threshold must use the `G` form. HISAT2 already permits soft clipping
in 5-Base mode, including with Bismark's `--local`, and uses the `L` form; it
receives no Bowtie 2 `--local` flag. HISAT2 alignment stays unspliced, so
`--known-splicesite-infile` is rejected in this mode.

For either engine, `-p N` sets threads in the single aligner instance. Otherwise,
`--multicore N` supplies the thread count when N is greater than one; with neither
setting (or `--multicore 1`), Bismark uses the available logical CPUs. In 5-Base
mode an explicit `-p` takes precedence over `--multicore`; ordinary bisulfite
HISAT2 keeps its existing conflict rule. Input ordering is always preserved.

### Choosing an engine

The engines were compared on the first 100M read pairs of the NA12878 demo (Sample8,
100 ng; 98.1M pairs after trimming), each scored per CpG against DRAGEN's CX report.
Pearson r uses the 400,760 CpGs that every engine and DRAGEN cover at cov ≥ 10; the CpG
column counts each engine's own CpGs at cov ≥ 10 in both it and DRAGEN.

| Engine | Mapping efficiency | Pearson r vs DRAGEN | CpGs at cov ≥ 10 | Peak memory |
|---|---|---|---|---|
| minimap2 (default) | 95.2% | 0.9646 | 817,312 | 15.5 GiB |
| Bowtie 2 (default, 800 bp) | 94.5% | 0.9643 | 822,432 | 8.4 GiB |
| Bowtie 2 `-X 500` | 87.9% | 0.9631 | 554,655 | 8.4 GiB |
| Bowtie 2 `--local -X 500` | 91.2% | 0.9635 | 580,613 | 8.4 GiB |
| HISAT2 | 91.3% | 0.9631 | 664,790 | 8.3 GiB |

This depth is about 5× per strand, so cov ≥ 10 is the high-coverage tail and r is lower
than the full-depth figure in the stability contract. A 500 bp insert cap costs Bowtie 2
about 7 points of mapping efficiency and a third of its well-covered CpGs on this library;
at its 800 bp default (minimap2's own limit) it is level with minimap2. On HG002 the 800 bp
default raised mapping from 90.5% to 94.5% and CpGs at cov ≥ 10 by 30%. minimap2 used about
twice the peak memory. The engine comparison covers one 5-Base WGS gDNA library (NA12878),
and the Bowtie 2 insert-cap result was repeated on a second (HG002); short-fragment
libraries such as cfDNA were not tested.

The emitted BAM is standard Bismark convention (`XM`/`XR`/`XG`), so the extractor,
`bismark2bedGraph`, `coverage2cytosine`, and the reports consume it unchanged.

### bwa-mem3 (`--bwamem3`)

[bwa-mem3](https://github.com/fg-labs/bwa-mem3) 0.16.0 has a methylation-aware 5-Base mode
(`mem --meth=5base`) that does not penalise a read `T` at a genomic `C`, so methylated reads
from CpG-dense regions align as well as unmethylated ones (see the
[known limitation](#known-limitation-cpg-dense-methylated-regions)). bwa-mem3 only places the
reads; Bismark makes the methylation calls, exactly as for the other engines.

It is opt-in because it is expensive:

| | bwa-mem3 |
|---|---|
| Index | `bwa-mem3 index --meth genome.fa`: about 1 h and about 104 GB RAM for GRCh38 |
| Index on disk | about 23 GB needed for GRCh38; `bwa-mem3 index --meth` writes about 39 GB, and `genome.fa.meth.fa` (6.3 GB) and the plain `genome.fa.bwt.2bit.64` (10.4 GB) can be deleted |
| Alignment memory | bwa-mem3's own RSS (about 28 GiB at 32 threads; it grows with threads) plus about 3.2 GB for Bismark's copy of the genome |
| Runtime | about 38–39 min per 95–98M pairs at 32 threads (bwa-mem3 alone) |

Pass the FASTA path the index was built from as `--five_base_index`. Build the index from
the same FASTA as the `--genome` folder: Bismark compares the index's contig names and
lengths with the genome folder before it starts, and the engine's `@SQ` lines again at run
time, and stops on any difference. Keep the index outside the `--genome` folder, so its
files are not mistaken for genome FASTA. The index is loaded once per mate-pair file pair.
An index built by nf-core's `bwamem3/index` module or given to nf-core/methylseq as
`--bwamem3_index` works as it is.

Bismark fixes every bwa-mem3 option that affects placement (`--meth-scoring neutral
--meth-context all --meth-seed-prune=spec30`), because bwa-mem3's defaults have changed
between releases, and it ignores `$BWAMEM3_METH_SEED_PRUNE`. The Bowtie 2 and minimap2
tuning options and `--local` are rejected with `--bwamem3`; `-p`/`--multicore` set its
threads. Versions before 0.16.0 are rejected, and other versions run with a warning that
0.16.0 is the validated one. The BAM records the bwa-mem3 version and command line in a
`@PG ID:bwa-mem3` line, and the report states the version.

As with the other engines, Bismark keeps proper pairs at any MAPQ; about 5% of bwa-mem3's
records have MAPQ 0. Dovetailed pairs are kept too, but bases that run past the mate's 5' end
are adapter read-through rather than genome, so they are not called; the report counts them as
"Dovetail overhang bases not called". This applies to minimap2 as well. Pairs whose mates point
apart are rejected, and the report counts them under "Proper pairs rejected" and as pairs with
no alignment.

With `--five_base_umi_len N`, the first N bases of each mate are never called, for every engine,
including bases the engine aligned rather than soft-clipped. The UMI and overhang masks apply to
the core calls (`XM`, and everything the extractor reads); `--five_base_duplex`,
`--five_base_consensus` and `--five_base_deconvolution` do not apply them yet.

Before Bismark validates a newer bwa-mem3, the per-CpG comparison with the validated version is
re-run on 1M read pairs; until then, other versions run with the warning above.

`bismark2summary` does not discover 5-Base BAMs on its own, for any engine. Pass them to it
explicitly: `bismark2summary *_bismark_*_pe.bam`.

### Advanced modes

Real 5-Base libraries carry a dual UMI in the read name and sequence each molecule as two
strand-partner read pairs. These modes use that structure:

- **`--five_base_deconvolution`** separates a genuine `C>T`/`G>A` genetic variant from 5mC
  using both strands (DRAGEN's rule: methylation moves only one strand, a variant moves
  both). Writes a per-CpG report; variant CpGs are excluded from the methylation totals.
- **`--five_base_duplex`** groups the two strands of each molecule into a duplex family
  (keyed on the fragment span plus the canonical dual UMI) and reconciles the 5mC signal
  per molecule.
- **`--five_base_consensus`** collapses each duplex family to one consensus record per
  molecule, scoring both strands of every CpG.
- **`--five_base_consensus_from_bam`** re-runs the duplex-consensus collapse over one or
  more already-aligned 5-Base BAMs (same output as `--five_base_consensus`, skipping
  re-alignment).
- **`--five_base_umi_qname`** (preview) takes the duplex UMI from the read name (the real
  Illumina form, `A+B` with the partner swapped) instead of an inline read prefix.
- **`--five_base_emit_multiplicity {duplex|simplex|both}`** selects which consensus
  families are emitted (see below). `duplex` is the default.

```sh
# duplex consensus from the real dual-UMI in the read name
bismark --illumina_5base --five_base_umi_qname --five_base_consensus \
        --genome /path/to/GRCh38 -1 R1.fastq.gz -2 R2.fastq.gz
```

### Simplex molecules: `--five_base_emit_multiplicity`

A duplex family needs the molecule to be seen from **both** strands. Molecules seen from one
strand only are counted and, by default, discarded — on shallow input such as cfDNA that is
most of the library. `--five_base_emit_multiplicity` mirrors DRAGEN's
`--umi-emit-multiplicity`:

| Mode | Emits |
|---|---|
| `duplex` (default) | `<out>.5base_consensus.bam` — duplex families, two records each |
| `simplex` | `<out>.5base_simplex.bam` — single-strand families, one record each. **No duplex BAM** |
| `both` | both files |

A simplex family still contains every PCR copy of its strand, so the collapse deduplicates
and error-corrects; only the cross-strand check is missing. In `simplex` and `both` modes every
consensus record carries `mx:i` (`2` = duplex, `1` = simplex — unrelated to the `XM` call
string), so the two classes stay distinguishable even after a `samtools merge`. Each record is
emitted on its molecule's own strand: the opposite-strand record would report calls for a
strand that was never sequenced.

:::caution
A simplex consensus has **no cross-strand variant check** — the check that separates 5mC from a
genuine `C>T` needs the opposite strand, so on simplex reads a real `C>T` variant reads as
methylation. Treat simplex output as lower confidence than duplex, and pair it with
[`--five_base_deconvolution`](#advanced-modes) for the population-level check.
:::

The run reports how many single-strand families were found and their read-count distribution
(a family can hold a single read: `--five_base_min_mapq` filters per record, so one mate of a
pair can be dropped while the other survives).

### Interop: `--five_base_bisulfite_bam` (for `wgbs_tools` / UXM)

Some downstream tools read methylation out of the BAM's `SEQ` rather than out of `XM`. The
clearest example is [`wgbs_tools`](https://github.com/nloyfer/wgbs_tools) — its `bam2pat`
compares the read base at each CpG against the reference (`C`/`G` = methylated, `T`/`A` = not),
which is why it needs a genome. 5-Base `SEQ` holds the raw read and 5-Base chemistry is the
inverse of bisulfite, so those tools produce **systematically inverted** calls on 5-Base data —
not noisy, inverted.

`--five_base_bisulfite_bam` re-encodes an existing 5-Base BAM into bisulfite convention: at
each called cytosine the base becomes what bisulfite chemistry would have produced, taken from
the `XM` tag Bismark already wrote. It needs no genome and no re-alignment, and `XM` is left
untouched — so the output is still readable by Bismark's own extractor, and `bam2pat --ds_test`
(which does read `XM`) keeps working.

```sh
# re-encode, then sort + index: bam2pat REFUSES a BAM that is not SO:coordinate
bismark --illumina_5base --five_base_bisulfite_bam sample_pe.bam
samtools sort -o sample_pe.bisulfite.sorted.bam sample_pe.bisulfite.bam
samtools index sample_pe.bisulfite.sorted.bam
wgbstools bam2pat --genome hg38 sample_pe.bisulfite.sorted.bam
```

The genome you aligned against must use the **same chromosome naming** as your
`wgbstools init_genome` (so `chr1`, not `1`). A total mismatch fails early and loudly; a
*partial* one silently drops the contigs that do not match.

:::caution
This is never the primary BAM. Its `SEQ` no longer matches what the sequencer read, and
because 5-Base cannot separate a genuine `C>T` from 5mC without the opposite strand, real
variants are encoded as methylation. Harmless for fragment-level CpG tools like UXM; **do not
feed it to a variant caller.**
:::

The run reports a **flip rate**, which is exactly `1.000` for 5-Base input and `0.000` for
bisulfite input, because every called position necessarily changes under inversion. Anything in
between means the input is mixed or partly converted, and the run fails rather than writing a
file that is half one convention and half the other. Running the converter on an ordinary
bisulfite BAM is therefore a no-op by construction, and that identity is a CI gate.

## Validation

The concordance evidence (per-CpG Pearson r and call-agreement vs DRAGEN, and the
deconvolution precision/recall vs DRAGEN's germline VCF) and a reproducible runbook live in
the repository at `validation/VALIDATION_REAL_DATA.md`, with the
`validation/concordance.py` harness. The deterministic floor (lambda unmethylated control
reads near 0%, pUC19 CpG-methylated control reads high) is locked by the
`five_base_controls_*` gates that run in CI.

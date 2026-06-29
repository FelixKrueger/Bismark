# bismark-aligner

Rust port of the Perl `bismark` aligner **wrapper** — the largest component of
the Bismark pipeline (~74% of runtime). `bismark` is not an aligner: it converts
reads (C→T, plus the G→A complement for non-directional libraries), drives 2–4
external **Bowtie 2** instances against the bisulfite-converted indexes produced
by `bismark_genome_preparation`, merges and scores their SAM output in read-ID
lockstep, performs the bisulfite best-alignment selection + strand assignment +
the `XM`/`XR`/`XG` methylation call, and writes the Bismark BAM + reports.

**Binary:** `bismark_rs`.

**Input formats:** FastQ, FastA, and **unaligned BAM (uBAM)** — a uBAM is auto-detected by its BAM magic bytes (single-end, or a single name-collated paired-end uBAM that is auto-split into mates) and transcoded to a temp FASTQ matching `samtools fastq`, so output is byte-identical to the equivalent FASTQ run. See the [Alignment usage docs](https://felixkrueger.github.io/Bismark/usage/alignment/) for details.

## Status — built phase by phase

This crate is implemented incrementally against a phased epic
(`plans/05312026_bismark-aligner/`). **Acceptance gate:** byte-identical
*decompressed* SAM content (`samtools view` + `-H`) versus Perl Bismark v0.25.1
driving the pinned **Bowtie 2 2.5.5** (raw BGZF bytes are not gated — the Rust
path writes BAM via `noodles`, not `samtools`).

- **Phase 1 (current):** CLI + option parsing + genome/index discovery + Bowtie 2
  detection + `aligner_options` assembly → a resolved `RunConfig`. **No alignment
  is performed yet** — the binary parses, validates, discovers, detects, prints a
  resolved-configuration summary, and exits.
- Later phases add read conversion, single-instance alignment, the N-way merge +
  scoring, the methylation call + SAM/BAM output, reports, paired-end,
  non-directional/pbat, FastA, and order-preserving multicore.

HISAT2 / minimap2 aligners are deferred to a `v1.x` follow-up.

## Build & test

```bash
cargo build -p bismark-aligner
cargo test  -p bismark-aligner
```

## Illumina 5-Base mode (`--illumina_5base`, experimental, #787)

Opt-in, never-silent, **concordance-gated** support for **Illumina 5-Base** data
(GitHub issue #787). 5-Base is the chemical **inverse** of bisulfite: the enzyme
converts **5mC → T** and leaves unmethylated C intact, so library complexity is
preserved and reads align to the **unconverted** genome with a standard aligner.

`--illumina_5base` (alias `--five_base`) therefore does NOT run the C→T/G→A
converted-index spine. It aligns the raw reads with **minimap2** (`-x sr`) against
the unconverted reference FASTA, derives the strand from the SAM FLAG (forward =
OT, reverse = OB), and reuses the byte-frozen genomic-extraction + `XM`/`XR`/`XG`
output, with the methylation call run at **inverted polarity** (a read `T` at a
genomic C = methylated). The BAM it writes is standard Bismark-convention, so the
extractor / bedGraph / coverage2cytosine / report consume it unchanged.

Both **single-end and paired-end** are supported (PE runs one minimap2 PE instance
over the unconverted genome; the per-pair index is OT/OB from R1's strand).

**Aligner backends.** The default is **minimap2** (`-x sr`) reading the genome FASTA
directly. `--bowtie2`/`--hisat2` are also supported via `--five_base_index <basename>`,
a NORMAL (unconverted) index of the genome (5-Base keeps full complexity, so a plain
index works; build it once with `bowtie2-build`/`hisat2-build`).

**UMI dedup** (`--five_base_umi_len N`). Takes the first `N` bases of each read as its
UMI (e.g. `8` for the 5-Base 7 bp UMI + 1 spacer) and drops PCR/optical duplicates
sharing (UMI, chrom, position, strand), removing methylation bias. (Relies on the
aligner soft-clipping the UMI prefix; soft-clipped bases produce no methylation call.)

### Validation against DRAGEN (real data)

The supported path is the **core per-read SE+PE 5-Base BAM** above. On the real Illumina
5-Base demo (NA12878 100 ng, BaseSpace; ~44×, whole GRCh38), the extracted per-CpG
cytosine report is **per-CpG equivalent to DRAGEN's `CX_report`**: Pearson **r ≈ 0.99**,
call agreement **97.5 %** over **55 M** shared CpGs, global CpG 49.7–50.1 % vs DRAGEN
49.98–50.48 %, with non-CpG at DRAGEN's own lambda-control floor and directional-only
confirmed (DRAGEN CTOT/CTOB = 0). It is **NOT byte-identical** (Perl Bismark has no
5-Base oracle); the reproducible CI gate is synthetic ground-truth vs the real minimap2
(`tests/five_base_groundtruth.rs`, which **fail loud in CI if minimap2 is absent**). See
`plans/06232026_illumina-5base-support/VALIDATION_REAL_DATA.md`.

### Experimental / preview modes (#787)

These secondary modes are **wired end-to-end and never-silent**, but are **not
byte-identity- or per-site-CI-gated** — treat them as preview:

- **`--five_base_deconvolution`** — variant-vs-5mC deconvolution. A C>T genetic variant
  reads as `T` like 5mC; a two-strand pass flags a CpG gone on BOTH strands as a
  **variant** (excluded from methylation), DRAGEN's rule. Writes
  `<out>.5base_deconvolution.txt`.
- **`--five_base_duplex`** — groups a molecule's two strands into a duplex family and
  reconciles 5mC→T per molecule (`<out>.5base_duplex.txt`). **PE keys on the fragment
  span (POS + mate-pos + TLEN)** — the real workflow (5-Base is paired-end). **SE-duplex
  is a known limitation:** SE OT/OB reads cover opposite fragment ends with different
  spans and do not pair on real data, so SE-duplex is a degenerate non-workflow.
- **`--five_base_consensus`** — collapses each duplex family to a consensus (a forward +
  reverse record per family in `<out>.5base_consensus.bam`) via the asymmetric 5mC>T rule,
  reconciled by **molecule strand** (OT carries a `+` CpG, OB a `-` CpG). DRAGEN-validated on
  real NA12878 (24×, both strands r ≈ 0.77; per-CpG r 0.77 at cov≥1 → 0.86 at cov≥3). The
  high-resolution methylation path is still the core per-read BAM (r ≈ 0.99); the consensus
  is the per-molecule duplex view. Replay it on existing BAMs (no re-alignment) with
  `--five_base_consensus_from_bam <bam>` (repeatable; families pair across files).
- **`--five_base_umi_qname`** — takes the duplex dual-UMI from the read NAME (`A+B`, with
  the partner's halves swapped) instead of inline bases; this is the real-data UMI form.

**Scope (rejected loudly):** `--non_directional`/`--pbat` (DRAGEN documents 5-Base as
**directional-only** — a permanent non-goal), `--slam`, `--fasta`, `--combined_index*`.
`--multicore` is honored as a thread count (single instance, scale with `-p`). See
`plans/06232026_illumina-5base-support/`.

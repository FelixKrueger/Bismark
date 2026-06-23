# bismark-aligner

Rust port of the Perl `bismark` aligner **wrapper** — the largest component of
the Bismark pipeline (~74% of runtime). `bismark` is not an aligner: it converts
reads (C→T, plus the G→A complement for non-directional libraries), drives 2–4
external **Bowtie 2** instances against the bisulfite-converted indexes produced
by `bismark_genome_preparation`, merges and scores their SAM output in read-ID
lockstep, performs the bisulfite best-alignment selection + strand assignment +
the `XM`/`XR`/`XG` methylation call, and writes the Bismark BAM + reports.

**Binary:** `bismark_rs`.

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

**This is NOT byte-identical** — Perl Bismark has no 5-Base path, so there is no
v0.25.1 oracle. Validation is two synthetic **ground-truth gates against the real
minimap2** (`tests/five_base_groundtruth.rs`, SE + PE): reads carrying a known
5mC→T pattern are recovered with the correct `Z`/`z` call at every aligned CpG. A
DRAGEN-concordance gate is pending an external dataset. Requires `minimap2` on
`PATH` (or `--path_to_minimap2`).

**Scope:** directional, FASTQ, single instance (SE + PE). Rejected loudly:
`--non_directional`/`--pbat` (DRAGEN documents 5-Base as **directional-only**, so
this is a permanent non-goal, not a deferred phase), `--slam`, `--fasta`,
`--multicore`, `--combined_index*`, and the other aligner backends. UMI/duplex-
consensus collapsing and variant-vs-methylation deconvolution (DRAGEN does both)
are not yet implemented — the caller is SNP-naive (at parity with the Bismark
bisulfite caller); pairs that are not properly mapped are skipped. See
`plans/06232026_illumina-5base-support/`.

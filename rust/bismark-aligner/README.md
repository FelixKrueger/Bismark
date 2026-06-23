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

> **Status note (the Phase-1 text above is outdated):** the v1.x epic is complete —
> HISAT2 (SE+PE) and minimap2 SE are byte-identical to Perl v0.25.1, and **minimap2 PE
> is now wired as an EXPERIMENTAL path** (mirrors Perl's positional two-file invocation;
> enabled via `--minimap2 -1/-2` with a never-silent notice, **NOT byte-identical** — the
> Perl PE minimap2 path is unfinished WIP with no trustworthy oracle). See the suite
> `rust/README.md` status table + milestones for the live status.

## Build & test

```bash
cargo build -p bismark-aligner
cargo test  -p bismark-aligner
```

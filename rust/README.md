# Bismark Rust rewrite

Active rewrite of Bismark from Perl to Rust. Progress is tracked at
the [Bismark Rust rewrite project](https://github.com/users/FelixKrueger/projects/1).

Working branch: [`rust/iron-chancellor`](https://github.com/FelixKrueger/Bismark/tree/rust/iron-chancellor).

## Layout

- `bismark-io/` — shared library: BAM/SAM/CRAM I/O via [noodles](https://github.com/zaeleus/noodles). See `bismark-io/DESIGN.md` for the design contract.
- Per-binary crates are added incrementally (`bismark-dedup/`, `bismark-bedgraph/`, `bismark-extractor/`, …). Phase 1 priorities are tracked on the project board.

## Binary naming during coexistence

Rust binaries take an `_rs` suffix through approximately v0.26 → v1.0 so they can be installed alongside the Perl Bismark scripts on the same PATH without conflicts:

| Perl                            | Rust binary (during coexistence) |
|---------------------------------|----------------------------------|
| `deduplicate_bismark`           | `deduplicate_bismark_rs`         |
| `bismark_methylation_extractor` | `bismark_methylation_extractor_rs` |
| `bismark2bedGraph`              | `bismark2bedGraph_rs`            |
| `coverage2cytosine`             | `coverage2cytosine_rs`           |

After v1.0 of the Rust port, the `_rs` suffix is dropped — the Rust binaries become the default `deduplicate_bismark` etc., and the Perl scripts move to a `legacy/` directory.

## Architecture decisions

- **BAM/SAM/CRAM I/O via pure-Rust `noodles`** — no `rust-htslib` (no htslib C build-time dep), no `samtools` subprocess (no external runtime dep).
- **One cargo workspace** with a binary crate per Bismark tool plus the shared `bismark-io` library. Library+binary split per crate so pure logic is unit-testable.
- **Byte-equal output to Perl Bismark v0.25.1** is a CI gate for the tools we have validated.
- Edition 2024; MSRV pinned in the workspace manifest.

## Building

```bash
cd rust
cargo build --release
```

## Status

| Perl tool | Rust crate (binary) | Version | State |
|---|---|---|---|
| _(shared library)_ | `bismark-io` | 1.0.0-beta.8 | noodles BAM/SAM/CRAM I/O + `ThreadedBam{Reader,Writer}` (parallel BGZF) + `BamWriter::write_raw_record` (unvalidated passthrough for the aligner's `--ambig_bam`); byte-equal output is a CI invariant for consumers |
| `deduplicate_bismark` | `bismark-dedup` (`deduplicate_bismark_rs`) | 1.2.1-beta.1 | **Byte-identical** to Perl v0.25.1 on real-data WGBS (10M + ~55M PE); optional `--parallel N` BGZF threading |
| `bismark_methylation_extractor` | `bismark-extractor` (`bismark-methylation-extractor-rs`) | 1.0.0-beta.1 | **Byte-identical** at full scale (WGBS PE/SE + RRBS, worker-count-invariant); **~4.8×** vs Perl `--multicore 12` |
| `bismark2bedGraph` | `bismark-bedgraph` (`bismark2bedGraph_rs`) | 1.0.0-beta.1 | **Byte-identical** (decompressed content); **~3.4×** |
| `coverage2cytosine` | `bismark-coverage2cytosine` (`coverage2cytosine_rs`) | 1.0.0-alpha.1 | In progress — byte-identity golden tests through phase D |
| `bismark_genome_preparation` | `bismark-genome-preparation` (`bismark_genome_preparation_rs`) | 1.0.0-alpha.1 | Converted CT/GA FASTA **byte-identical** to Perl v0.25.1 (indexing delegated to the external indexer) |
| `methylation_consistency` | `bismark-methylation-consistency` (`methylation_consistency_rs`) | 1.0.0-beta.1 | **Byte-identical** output vs Perl v0.25.1 |
| `bam2nuc` | `bismark-bam2nuc` (`bam2nuc_rs`) | 1.0.0-alpha.1 | **Byte-identical** to Perl v0.25.1 (mono/di-nucleotide stats; local goldens + oxy real-data gate) |
| `NOMe_filtering` | `bismark-nome-filtering` (`NOMe_filtering_rs`) | 1.0.0-beta.1 | **Byte-identical** to Perl v0.25.1 (synthetic goldens + full 10M SE oxy gate) |
| `filter_non_conversion` | `bismark-filter-nonconversion` (`filter_non_conversion_rs`) | 1.0.0-alpha.1 | **Byte-identical** to Perl v0.25.1 (9 golden cells + oxy 10M SE + PE × 4 decision modes) |
| `bismark2report` | `bismark-report` (`bismark2report_rs`) | 1.0.0-alpha.1 | **Byte-identical** HTML vs Perl v0.25.1 (modulo the `localtime` timestamp line); validated on synthetic + real WGBS PE (10M + ~55M) |
| `bismark` (aligner) | `bismark-aligner` (`bismark_rs`) | 1.0.0-alpha.1 | In progress — Phases 1–7/10: the **single-end + paired-end directional spine is complete** (CLI → C→T/G→A conversion → Bowtie 2 → lockstep merge/scoring/MAPQ → genomic-seq + `XM`/`XR`/`XG` → BAM + alignment report + `--unmapped`/`--ambiguous`/`--ambig_bam`), **byte-identical** to Perl v0.25.1 + Bowtie 2 2.5.5 on oxy (SE + PE WGBS, 1M reads/pairs). Non-directional / pbat / FastA / threading remain |

Versions are the crate manifests on `rust/iron-chancellor`. "Byte-identical" = validated against Perl Bismark v0.25.1 per each crate's README/CHANGELOG + golden/real-data tests; speedups are full-scale where measured. Per-crate detail lives in each crate's `README.md` / `CHANGELOG.md`. `bismark-io` and `bismark-dedup` have early beta lines published to crates.io; later betas are queued for the next publish window.

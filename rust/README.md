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

One headline per module — current state at a glance. Per-crate detail lives in each crate's `README.md` / `CHANGELOG.md`; the dated shipping log is under [Milestones](#milestones). Rows are in rough pipeline order. State key: ✅ shipped on `rust/iron-chancellor` · 🚧 in progress · ⬜ planned.

| Perl tool | Rust crate (binary) | Version | State |
|---|---|---|---|
| _(shared library)_ | `bismark-io` | 1.0.0-beta.8 | ✅ noodles BAM/SAM/CRAM I/O + `ThreadedBam{Reader,Writer}` (parallel BGZF); byte-equal output is a CI invariant for consumers |
| `bismark_genome_preparation` | `bismark-genome-preparation` (`bismark_genome_preparation_rs`) | 1.0.0-alpha.2 | ✅ Converted CT/GA FASTA **byte-identical** to Perl v0.25.1 + `--genomic_composition`; all 3 aligners (Bowtie2 / HISAT2 / minimap2), indexing delegated to the external indexer |
| `bismark` (aligner) | `bismark-aligner` (`bismark_rs`) | 1.0.0-alpha.1 | 🚧 In progress on `rust/aligner` — **Phases 1–8/10**: Bowtie 2 backend, **SE + PE FastQ, all 3 library types (directional / non-directional / pbat)** — read-conversion → 2–4 instances → lockstep merge/scoring/MAPQ → `XM`/`XR`/`XG` → BAM + report + `--unmapped`/`--ambiguous`/`--ambig_bam`, **byte-identical** to Perl v0.25.1 + Bowtie 2 2.5.5 (oxy, all 4 mode×layout cells, 1M reads/pairs). The ~74% runtime "big beast". FastA + threading (Ph 9) + full-scale gate (Ph 10) remain; HISAT2/minimap2 = v1.x |
| `deduplicate_bismark` | `bismark-dedup` (`deduplicate_bismark_rs`) | 1.2.1-beta.1 | ✅ **Byte-identical** to Perl v0.25.1 on real-data WGBS (10M + ~55M PE); UMI/RRBS modes; optional `--parallel N` BGZF threading |
| `filter_non_conversion` | `bismark-filter-nonconversion` (`filter_non_conversion_rs`) | 1.0.0-alpha.1 | ✅ **Byte-identical** to Perl v0.25.1 (9 golden cells + oxy 10M SE + PE × 4 decision modes) |
| `NOMe_filtering` | `bismark-nome-filtering` (`NOMe_filtering_rs`) | 1.0.0-beta.1 | ✅ **Byte-identical** to Perl v0.25.1 (synthetic goldens + full 10M SE oxy gate); **~3.4×** |
| `bismark_methylation_extractor` | `bismark-extractor` (`bismark_methylation_extractor_rs`) | 1.0.0-beta.1 | ✅ **Byte-identical** at full scale (WGBS PE/SE + RRBS, worker-count-invariant); **~4.8×** vs Perl `--multicore 12` |
| `bismark2bedGraph` | `bismark-bedgraph` (`bismark2bedGraph_rs`) | 1.0.0-beta.1 | ✅ **Byte-identical** (decompressed content); **~3.4×** |
| `coverage2cytosine` | `bismark-coverage2cytosine` (`coverage2cytosine_rs`) | 1.0.0-alpha.1 _(tag `…beta.2`)_ | ✅ **Byte-identical** core + niche modes (`--gc`/`--nome-seq`/`--drach`/`--ffs`) vs Perl v0.25.1; 15-cell full-hg38 oxy gate; **~12×** CpG-report / **~2.6×** `--CX` |
| `bam2nuc` | `bismark-bam2nuc` (`bam2nuc_rs`) | 1.0.0-alpha.1 | ✅ **Byte-identical** to Perl v0.25.1 (mono/di-nucleotide stats; local goldens + oxy real-data gate) |
| `methylation_consistency` | `bismark-methylation-consistency` (`methylation_consistency_rs`) | 1.0.0-beta.1 | ✅ **Byte-identical** output vs Perl v0.25.1 |
| `bismark2report` | `bismark-report` (`bismark2report_rs`) | 1.0.0-alpha.1 | ✅ **Byte-identical** HTML vs Perl v0.25.1 (modulo the `localtime` timestamp line); synthetic + real WGBS PE (10M + ~55M) |
| `bismark2summary` | `bismark-summary` (`bismark2summary_rs`) | 1.0.0-beta.1 | ✅ **Byte-identical** project-level multi-sample summary (HTML + `.txt`) vs Perl v0.25.1 |

Versions are the crate manifests on `rust/iron-chancellor` (a release **git tag** such as `…beta.2` may lead its manifest version). "Byte-identical" = validated against Perl Bismark v0.25.1 per each crate's README/CHANGELOG + golden/real-data tests; speedups are full-scale where measured. `bismark-io` and `bismark-dedup` have early beta lines published to crates.io; later betas are queued for the next publish window.

> **Keeping this journal current:** every module-merge PR into `rust/iron-chancellor` should update that tool's row above **and** add a dated line to [Milestones](#milestones). The helper scripts (`copy_bismark_files_for_release.pl`, the `merge_*coverage*` Python utilities) are out of scope for the Rust port.

## Milestones

Reverse-chronological log of the main Rust-rewrite shipping events (merges into `rust/iron-chancellor`). One headline per event; per-crate detail is in the crate READMEs/CHANGELOGs.

- **2026-06-02** — `bismark` aligner **Phases 1–8** merged (#930) — SE + PE FastQ, **all 3 library types** (directional / non-directional / pbat); byte-identical to Perl v0.25.1 + Bowtie 2 2.5.5 on oxy (4 mode×layout cells, 10k + 1M reads/pairs). FastA + threading + full-scale gate (Phases 9–10) remain.
- **2026-06-02** — `coverage2cytosine` **v1.x niche modes** (`--gc`/`--nome-seq`/`--drach`/`--ffs`) merged (#934); 15-cell full-hg38 oxy gate byte-identical to Perl v0.25.1, tag `…beta.2`. **c2c port complete.**
- **2026-06-01** — `bismark2summary` ported (#932) — byte-identical project-level summary; the **last post-alignment module**.
- **2026-06-01** — `bismark2report` ported (#931) — byte-identical per-sample HTML report.
- **2026-06-01** — `filter_non_conversion` ported (#927) — byte-identical non-CpG / incomplete-conversion read filter.
- **2026-06-01** — `bam2nuc` ported (#922) — byte-identical mono/di-nucleotide coverage QC.
- **2026-06-01** — CI **`perl-oracle byte-identity`** gate added (#933) — runs the live Perl tools in CI and byte-compares.
- **2026-06-01** — `NOMe_filtering` ported (#928) — byte-identical standalone NOMe filter.
- **2026-05-31** — `bismark_genome_preparation` ported (#913) + `--genomic_composition` (#925) — byte-identical converted FASTA.
- **2026-05-31** — `coverage2cytosine` **v1.0** core merged (#892) — byte-identical CpG/CX report + `--merge_CpGs`; ~12× on the CpG-report path.
- **2026-05-30** — `bismark2bedGraph` ported (#893, epic #797) — byte-identical coverage/bedGraph; mimalloc perf (#915).
- **2026-05-30** — `methylation_consistency` ported (#896, epic #890) — byte-identical.
- **2026-05-26 → 05-29** — `bismark_methylation_extractor` ported (Phases A–G, #847–#883) — byte-identical at full scale, **~4.8×**; the ~16% runtime hot-spot.
- **2026-05-24 → 05-26** — `deduplicate_bismark` v1.2 UMI/RRBS modes (#819–#844) — byte-identical.
- _(earlier)_ — `bismark-io` shared library — the noodles BAM/SAM/CRAM foundation all consumers build on.
- 🚧 **Next:** the `bismark` aligner **Phases 9–10** (FastA + order-preserving threading, then the full-scale real-data gate) — on `rust/aligner`. Phases 1–8 (SE + PE, all library types) are byte-identical and merged.

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
| `bismark_genome_preparation` | `bismark-genome-preparation` (`bismark_genome_preparation_rs`) | 1.0.0-beta.1 | ✅ Converted CT/GA FASTA **byte-identical** to Perl v0.25.1 + `--genomic_composition`; all 3 aligners (Bowtie2 / HISAT2 / minimap2), indexing delegated to the external indexer |
| `bismark` (aligner) | `bismark-aligner` (`bismark_rs`) | 1.0.0-alpha.1 | ✅ **All 10 phases complete** (#930 = Ph 1–8, #942 = Ph 9a, #945 = Ph 9b; **Ph 10 full-scale real-data gate PASSED**, #948). Bowtie 2 backend, **SE + PE, FastQ + FastA, all 3 library types (directional / non-directional / pbat)** — read-conversion → 2–4 instances → lockstep merge/scoring/MAPQ → `XM`/`XR`/`XG` → BAM + report + `--unmapped`/`--ambiguous`/`--ambig_bam`, **byte-identical** to Perl v0.25.1 + Bowtie 2 2.5.5 at 1M reads/pairs and **content byte-identical at full real-data scale** (Ph 10 on oxy: 84M SE / 84M PE / 46.7M mouse-RRBS GRCm39 / pbat; 173/181/52 contigs; + V13 cross-check vs the pre-existing Perl `--parallel 4` BAMs). **Order-preserving `--multicore`/`--parallel`** (worker-count-invariant). The ~74% runtime "big beast" — **faithful (Bowtie 2) port complete**. **v1.x backend set COMPLETE** — **HISAT2** (SE+PE, FastQ+FastA, all 3 libraries, byte-identical to Perl v0.25.1 + HISAT2 2.2.2; `--multicore`+`--hisat2` rejected — splice-site discovery is input-batch-global; #949) **and minimap2 SE** (byte-identical to Perl v0.25.1 + minimap2 2.31-r1302, clean-slate `-x map-ont` options + positional `.mmi`; SE only — PE deferred, no trustworthy Perl oracle; worker-invariant; #950). **Phase-5 combined 10M gate: all 13 cells byte-identical** (Bowtie 2 + HISAT2 SE+PE + minimap2 SE × {dir, non-dir, pbat} + mouse **GRCm39** RRBS). epic `plans/06052026_bismark-aligner-v1x/` |
| `deduplicate_bismark` | `bismark-dedup` (`deduplicate_bismark_rs`) | 1.2.1-beta.1 | ✅ **Byte-identical** to Perl v0.25.1 on real-data WGBS (10M + ~55M PE); UMI/RRBS modes; optional `--parallel N` BGZF threading |
| `filter_non_conversion` | `bismark-filter-nonconversion` (`filter_non_conversion_rs`) | 1.0.0-beta.1 | ✅ **Byte-identical** to Perl v0.25.1 (9 golden cells + oxy 10M SE + PE × 4 decision modes) |
| `NOMe_filtering` | `bismark-nome-filtering` (`NOMe_filtering_rs`) | 1.0.0-beta.1 | ✅ **Byte-identical** to Perl v0.25.1 (synthetic goldens + full 10M SE oxy gate); **~3.4×** |
| `bismark_methylation_extractor` | `bismark-extractor` (`bismark_methylation_extractor_rs`) | 1.0.0-beta.1 | ✅ **Byte-identical** at full scale (WGBS PE/SE + RRBS, worker-count-invariant); **~4.8×** vs Perl `--multicore 12`. **Inline streaming**: `--bedGraph`/`--cytosine_report` drive bismark2bedGraph + coverage2cytosine **in-process** (in-memory tee, no Perl subprocesses) — byte-identical downstream incl. `--CX` (Phase H sub-gate 2) |
| `bismark2bedGraph` | `bismark-bedgraph` (`bismark2bedGraph_rs`) | 1.0.0-beta.1 | ✅ **Byte-identical** (decompressed content); **~3.4×** |
| `coverage2cytosine` | `bismark-coverage2cytosine` (`coverage2cytosine_rs`) | 1.0.0-beta.2 | ✅ **Byte-identical** core + niche modes (`--gc`/`--nome-seq`/`--drach`/`--ffs`) vs Perl v0.25.1; 15-cell full-hg38 oxy gate; **~12×** CpG-report / **~2.6×** `--CX` |
| `bam2nuc` | `bismark-bam2nuc` (`bam2nuc_rs`) | 1.0.0-beta.1 | ✅ **Byte-identical** to Perl v0.25.1 (mono/di-nucleotide stats; local goldens + oxy real-data gate) |
| `methylation_consistency` | `bismark-methylation-consistency` (`methylation_consistency_rs`) | 1.0.0-beta.1 | ✅ **Byte-identical** output vs Perl v0.25.1 |
| `bismark2report` | `bismark-report` (`bismark2report_rs`) | 1.0.0-beta.1 | ✅ **Byte-identical** HTML vs Perl v0.25.1 (modulo the `localtime` timestamp line); synthetic + real WGBS PE (10M + ~55M) |
| `bismark2summary` | `bismark-summary` (`bismark2summary_rs`) | 1.0.0-beta.1 | ✅ **Byte-identical** project-level multi-sample summary (HTML + `.txt`) vs Perl v0.25.1 |

Versions are the crate manifests on `rust/iron-chancellor` (a release **git tag** such as `…beta.2` may lead its manifest version). "Byte-identical" = validated against Perl Bismark v0.25.1 per each crate's README/CHANGELOG + golden/real-data tests; speedups are full-scale where measured. `bismark-io` and `bismark-dedup` have early beta lines published to crates.io; later betas are queued for the next publish window.

> **Keeping this journal current:** every module-merge PR into `rust/iron-chancellor` should update that tool's row above **and** add a dated line to [Milestones](#milestones). The helper scripts (`copy_bismark_files_for_release.pl`, the `merge_*coverage*` Python utilities) are out of scope for the Rust port.

## Milestones

Reverse-chronological log of the main Rust-rewrite shipping events (merges into `rust/iron-chancellor`). One headline per event; per-crate detail is in the crate READMEs/CHANGELOGs.

- **2026-06-08** — `bismark` aligner **v2 `--combined_index` NON-DIRECTIONAL mode shipped** (#956) — extends the combined-index path (#955) to non-directional: **two** both-strands Bowtie 2 passes over `BS_combined` (C→T reads → OT/OB, G→A reads → CTOT/CTOB), `-k 2` each, **unioned per read**; read-conversion-aware classification + the shared `chr:pos`+`>=`+Sylvain-Foret tie machine (`combined::select_core`) across all four synthetic indices (OT=0/OB=1/CTOT=2/CTOB=3), **CTOB winning the §4b telomeric OT×CTOB same-position collision** (later slot). Model **(a)** (two parallel passes + per-read union); the single conversion-tagged-invocation model (b) was spike-rejected (the qname tag perturbs Bowtie 2's read-name-seeded RNG → not decision-equivalent) and deferred to v2.x. **oxy non-dir gate PASS** vs the faithful 4-instance oracle (same binary, flag off) on 1M Sherman balanced non-dir reads, `-k 2`: oracle-unique-stays-unique 99.978% (churn 0.022%); 4 strands balanced; per-strand exact-POS/`XM` ≥99.9987% (incl. the new CTOT/CTOB); `XR`/`XG`/FLAG ≥99.99% + =100% among POS-concordant; §4b same-pos-strand divergence 0; Sherman position accuracy combined 99.9639% ≈ oracle 99.9663% (|Δ| 0.0024 pp). **PBAT combined still deferred** (no spike). The faithful default + the directional combined path are untouched.
- **2026-06-08** — `bismark` aligner **v2 `--combined_index` directional mode shipped** (#955) — opt-in, never-silent, **concordance-gated (NOT byte-identical)**: one both-strands Bowtie 2 pass over the combined CT+GA index (`Bisulfite_Genome/Combined/BS_combined`) instead of separate per-strand instances, recovering strand from RNAME-suffix×FLAG (OT→synthetic index 0, OB→1) into the byte-frozen output arm; faithful `chr:pos`+`>=`+Sylvain-Foret same-position tie resolution. **Single-end directional only** (non-dir/pbat/PE/HISAT2/minimap2/multicore hard-rejected); the faithful default path is untouched. **oxy directional gate PASS** vs the faithful 2-instance oracle (same binary, flag off) on real GRCh38 WGBS-SE at full 84M: oracle-unique-stays-unique 99.9008%; per-strand exact-POS OT 99.99491% / OB 99.99501%; `XM` OT 99.99583% / OB 99.99580%; `XR` 100% both strands; `XM`-among-POS-concordant 100% (no methylation divergence beyond placement). Non-directional = future phases.
- **2026-06-06** — `bismark` aligner **v1.x minimap2 SE backend + combined 10M gate — v1.x epic COMPLETE** (#950). minimap2 single-end byte-identical to Perl v0.25.1 + minimap2 2.31-r1302 (a pure wrapper: clean-slate `-a --MD --secondary=no -t 2 -x map-ont -K 250K` + positional `.mmi`; the merge/MAPQ/XM core reused unchanged — minimap2's `s2:i:` is ignored → `second_best=None`). PE-minimap2 hard-rejected (the Perl PE path is unfinished WIP — documented known gap). **Phase-5 combined 10M single-core gate: all 13 cells byte-identical** — HISAT2 SE+PE × {dir, non-dir, pbat}, minimap2 SE × {dir, non-dir, pbat} (worker-invariant `--parallel 8`==`1` @10M), Bowtie 2 SE/PE anchors, **+ mouse GRCm39 RRBS** (HISAT2 PE 11.73M rec + Bowtie 2 PE 12.56M rec); `ht2_pe_pbat` via R1↔R2 swap = real pbat (16.25M rec). The Bismark aligner now supports **Bowtie 2 + HISAT2 (SE+PE) + minimap2 (SE)**, all byte-identical to Perl driving the same pinned aligner.
- **2026-06-04** — `bismark` aligner **Phase 10 (full-scale real-data gate) PASSED — faithful aligner epic COMPLETE.** On oxy vs Perl v0.25.1 + Bowtie 2 2.5.5: content byte-identical at full real-data scale across WGBS SE (84.0M reads, 71.3M recs, 173 contigs), PE (84.0M pairs, 143.4M recs, 181 contigs), **mouse RRBS GRCm39** (46.7M pairs, 55.4M recs, 52 contigs), and **pbat** (R1↔R2 swap, 143.4M recs). Gate A (10M) = strict byte-identity vs Perl single-core + worker-invariance + *measured* multicore-multiset-invariance; Gate B (full) = content-multiset + report + count-reconciliation + RNAME-set + aux + perf; **V13** = the pre-existing Perl `--parallel 4` BAMs carry the same content md5 (four layouts converge). Dual code-review + plan-manager COMPLETE. The aligner is byte-faithful end-to-end: SE+PE, FastQ+FastA, all 3 library types, worker-invariant, full-scale-validated.
- **2026-06-03** — `bismark_methylation_extractor` **inline bedGraph/coverage2cytosine streaming** merged — `--bedGraph`/`--cytosine_report` now drive `bismark2bedGraph` + `coverage2cytosine` **in-process** (an in-memory tee feeds the bedGraph aggregator; c2c runs from the `.cov.gz`), replacing the Perl-subprocess scaffold. Byte-identical to Perl v0.25.1 across WGBS SE+PE × {bg, cr, cr_cx, cr_split, cutoff2, zero, ucsc} + RRBS `bg` at full scale (Phase H sub-gate 2, 16/16 oxy cells). `--CX` peak RSS comparable to Perl (~26–34 GB; inline is genome-bounded, not a regression at scale).
- **2026-06-03** — `bismark` aligner **Phase 9a (FastA, #942) + Phase 9b (order-preserving `--multicore`/`--parallel`)** — FastA input across all library types, plus worker-count-invariant file-level threading: `--parallel N` output byte-identical to `--parallel 1` and Perl single-core on oxy (6 cells SE/PE × {directional, non-directional, pbat}, 10k + 1M, incl. a non-divisible count). Full-scale real-data gate (Ph 10) remains.
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
- ✅ **The faithful (Bowtie 2) `bismark` aligner port is COMPLETE** (all 10 phases, #948) — byte/content-identical to Perl v0.25.1 + Bowtie 2 2.5.5 across SE+PE, FastQ+FastA, all library types, full real-data scale (incl. mouse GRCm39 + pbat).
- ✅ **`bismark` aligner v1.x COMPLETE — HISAT2 (SE+PE) + minimap2 (SE)** (epic `plans/06052026_bismark-aligner-v1x/`, #949 + #950). Both backends byte-identical to Perl v0.25.1 driving the same pinned aligner; the Phase-5 combined 10M gate confirmed all 13 cells (3 backends × library types + mouse GRCm39). The Bismark aligner now offers all three engines.
- 🚧 **Next: v2 alternative engines** (rammap / combined-index, concordance-gated, spike-first).

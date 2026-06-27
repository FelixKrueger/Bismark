# Changelog

All notable changes to `bismark-extractor` will be documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versioning: [SemVer](https://semver.org/spec/v2.0.0.html).

> This file was introduced on 2026-06-12. Earlier history — the initial port
> (`1.0.0-beta.1`), the always-on parallel BGZF decode + gzip/mimalloc perf work
> ([#884](https://github.com/FelixKrueger/Bismark/pull/884)), full-dataset
> validation ([#920](https://github.com/FelixKrueger/Bismark/pull/920)), the
> inline bedGraph + coverage2cytosine streaming epic
> ([#947](https://github.com/FelixKrueger/Bismark/pull/947)), and single-end
> coordinate-sorted + multi-file input
> ([#971](https://github.com/FelixKrueger/Bismark/pull/971)) — predates it and is
> recorded in git history and the `rust/README.md` status journal.

## [1.0.0-beta.2] — 2026-06-26

### Fixed

- **Non-directional paired-end input no longer crashes.** `bismark_methylation_extractor_rs --paired-end` on
  a `--non_directional` BAM aborted with `read identity mismatch: expected R1 for first mate, got R2` on the
  first CTOT/CTOB pair: Bismark deliberately swaps the SAM first/second-in-pair FLAG bits for those strands
  (the first-in-file record — still sequencing Read 1 — carries `0x80`), and the shared
  `bismark_io::BismarkPair::from_mates` gate rejected the swap. Perl's `bismark_methylation_extractor` never
  inspected those bits. Fixed in `bismark-io` (pairs by file order + qname only). The extractor's PE path was
  already FLAG-independent — strand routing keys off `pair_strand` (`XR`/`XG`) and M-bias bucketing uses the
  literal `R1`/`R2` tied to `pair.r1()`/`pair.r2()` file order in both the serial and `--parallel` paths — so
  output is **byte-identical to Perl v0.25.1** on the issue's reproducer (CpG/CHG/CHH context files, M-bias,
  and splitting report all identical). Resolves
  [#1030](https://github.com/FelixKrueger/Bismark/issues/1030). New test file
  `tests/nondir_swapped_flags_1030.rs`: real-data crash-gone, `--parallel` byte-invariance, swapped-vs-idealized
  flag output equality on an overlapping CTOT pair (`--no_overlap`/`drop_overlap` coverage), and mixed
  4-strand coexistence.

### Changed

- **Accept `--CX` with `--bedGraph` (no `--cytosine_report`)**, matching Perl
  Bismark v0.25.1 (`bismark_methylation_extractor:1258-1259`,
  `die … unless ($cytosine_report or $bedGraph)`). `--CX` now makes the
  coverage/bedGraph cover **all** C-contexts (CpG + CHG + CHH) instead of CpG
  only, exactly as Perl does — methylseq then runs `coverage2cytosine --CX` as a
  separate downstream step on the resulting all-context `.cov.gz`. Previously the
  CLI was stricter than Perl and required `--cytosine_report`, which broke the
  nf-core/methylseq drop-in (the extractor command
  `--bedGraph --counts --gzip --report -s --CX …` was rejected). `--CX` with
  neither `--bedGraph` nor `--cytosine_report` is still rejected (it would have no
  output stream). The error variant `CxRequiresCytosineReport` was renamed
  `CxRequiresBedgraphOrCytosineReport` to mirror the `--zero_based` sibling. The
  all-context bedGraph aggregation itself was already implemented and
  Perl-byte-identical (the R4 tee gate); this is a validation relaxation only.

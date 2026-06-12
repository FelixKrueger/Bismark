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

## [Unreleased]

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

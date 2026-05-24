# Changelog

All notable changes to `bismark-io` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] — 2026-05-24

First stable release of `bismark-io`, the shared library crate for Bismark's
Rust rewrite. Wraps the [`noodles`](https://github.com/zaeleus/noodles) family
to provide Bismark-aware BAM/SAM/CRAM I/O: strand-classified record types,
tag-decoded accessors, CIGAR-aware position helpers.

This release is feature-complete for the v1.0 scope defined in `DESIGN.md`.
Downstream binary crates (`bismark-dedup`, `bismark-bedgraph`,
`bismark-extractor`, `bismark-coverage2cytosine`) will pin to `=1.0.0`.

### Added

- **Strand classification** (`BismarkStrand`, `BismarkStrand::from_xr_xg`).
  Eager classification at parse time; per-record strand vs pair-strand
  distinction enforced at the type level via separate `BismarkRecord` and
  `BismarkPair` types. `#[repr(u8)]` pins the discriminant layout (PR #816).
  Sub-issue #805, PR #806.

- **Typed errors** (`BismarkIoError` with `thiserror`). Variants:
  `MissingTag`, `MalformedTag`, `InvalidStrandTags`, `XmSeqLengthMismatch`,
  `MateMismatch`, `ReadIdentityMismatch`, `UnsortedInput`,
  `MissingCramReference`, `MissingFastaIndex`, `DuplicateChromosomeName`,
  `UnsupportedKind`, `Io`. Sub-issue #805, PR #806.

- **CIGAR extension trait** (`CigarExt`). `reference_span`, `read_span`,
  `reference_end`, `aligned_positions`. Property-tested for spec drift via
  an independent ground-truth table derived from SAM spec §1.4.6. Sub-issue
  #805 + #811, PR #806 + #812.

- **Tag accessors** (`tags::xm`, `tags::xr`, `tags::xg`, `tags::md`,
  `tags::nm`). Sub-issue #805, PR #806.

- **Record types** (`BismarkRecord`, `BismarkPair`, `ReadIdentity`).
  `BismarkRecord::from_noodles_record` performs eager strand classification +
  XM/seq length parity check. `BismarkPair::from_mates` validates qname
  equality + R1/R2 read-identity. Sub-issue #805, PR #806.

- **BAM + SAM readers** (`BamReader`, `SamReader`). Iterator-level
  unmapped-record filter + coordinate-sort detection via `@HD SO:`. Opt-out
  via `without_sort_check`. Sub-issue #805, PR #806.

- **CRAM reader** (`CramReader`) + **reference reconstitution helper**
  (`reconstitute_cram_reference_from_bismark_genome`). Atomic write, byte-fidelity
  chromosome names, `.fa.gz` support, duplicate-chromosome detection.
  Sub-issue #807, PR #808.

- **Path-dispatching reader** (`open_reader` + `AnyReader` enum). Routes to
  `BamReader` / `SamReader` / `CramReader` based on file extension. Sub-issue
  #807, PR #808.

- **BAM + SAM + CRAM writers** (`BamWriter`, `SamWriter`, `CramWriter`).
  `#[must_use]` on writer types; `finish()` consumes by value and is required
  before drop. Sub-issue #809, PR #810.

- **Path-dispatching writer** (`open_writer` + `AnyWriter` enum). Same
  enum-dispatch pattern as `AnyReader`, chosen over `Box<dyn Trait>` because
  `noodles-cram 0.93` exposes records via a borrowed iterator. Sub-issue
  #809, PR #810.

- **CRAM round-trip** end-to-end. Synthetic-record `cram_writer_roundtrip_via_tempfile`
  test exercises reference-based decoding through the full read-write cycle.
  Sub-issue #813, PR #814.

- **Test fixtures + property tests** (`tests/integration_fixture_bam.rs`,
  `tests/property.rs`). 22 KB Bismark-Perl-generated PE BAM (`tiny_pe_bismark.bam`)
  pinned to Bismark Perl v0.25.1; 6 proptest properties covering strand
  derivation + CIGAR span/end consistency. Sub-issue #811, PR #812.

- **`#![forbid(unsafe_code)]`** and **`#![warn(missing_docs)]`** at the crate
  root. All public items have rustdoc.

### Design contract

See [`DESIGN.md`](./DESIGN.md) for the design contract. Notable decisions:

- No `samtools` subprocess, no `rust-htslib` C-link, no `unsafe` blocks.
- Strand classification is eager (computed once at parse time, stored as a
  typed field). Output routing for paired-end data uses `BismarkPair::pair_strand()`
  (R1-derived), NOT each mate's `record_strand()`.
- CRAM support is read **and** write — strictly stronger than Perl Bismark's
  current behaviour (Perl pipes BAM through `samtools view -h -C` for CRAM).
- Public API surface is sync-only for v1.0. Async support is a future decision.

### MSRV

Rust **1.89.0**. Required by `noodles-bam` 0.89.

### Test coverage

- **108 tests total** (96 lib + 5 integration + 6 property + 1 doctest); 0 ignored.
- `cargo clippy --all-targets -- -D warnings` clean.
- `cargo fmt --check` clean.

### Notes for v1.0 consumers

- `Cargo.toml`: pin with `bismark-io = "=1.0.0"` if you want a strict
  match, or `bismark-io = "1"` for caret-compatible. The crate follows
  semver — breaking changes will bump the major version.
- This release is **not yet published to crates.io**. Path-dep usage from
  within the Bismark workspace is the supported integration model until at
  least one downstream binary crate (`bismark-dedup` first) lands. crates.io
  publication is deferred to keep the publish-bump cycle in lockstep with
  binary crates.

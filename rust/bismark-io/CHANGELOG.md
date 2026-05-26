# Changelog

All notable changes to `bismark-io` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0-beta.6] — 2026-05-26

**Read-orientation `iter_aligned()` adapter** on `BismarkRecord` for the
upcoming bismark-extractor port (epic #798). Resolves
[#843](https://github.com/FelixKrueger/Bismark/issues/843).

Additive only — non-extractor workflows continue to use `xm()` +
`cigar()` directly. The new iterator hides the
`-`-strand-reverse-complement orientation correction needed for M-bias
accumulation by sequencing-cycle position.

### Added

- New struct `AlignedXmCall { read_pos_5p: u32, ref_pos: u32, xm_byte: u8 }`
  in `record.rs`. Re-exported at the crate root.
- New method `BismarkRecord::iter_aligned() -> std::vec::IntoIter<AlignedXmCall>`.
  For each XM byte that aligns to a reference position (skipping
  insertions and soft-clips), yields a triple with the read coordinate
  oriented by the **5' end of the sequenced read**:
  - **`+` strand records (OT / CTOB)**: forward iteration; `read_pos_5p == BAM read_pos`.
  - **`-` strand records (OB / CTOT)**: reverse iteration; `read_pos_5p == seq_len - 1 - BAM read_pos`.

  Internally: one CIGAR walk via `CigarExt::aligned_positions` + one
  `Vec<AlignedXmCall>` allocation per record (~1.1 KiB for 100-bp reads
  with 95 aligned positions). Materializes before yielding to keep the
  type signature clean (`std::vec::IntoIter`).
- 8 unit tests covering:
  - Forward strand (OT) 5M CIGAR — identity.
  - Reverse strand (OB) 5M CIGAR — reversal + remapping.
  - Forward + insertion (skips insertion positions, ref_pos correct).
  - Forward + deletion (advances ref_pos correctly).
  - Forward + soft-clip (skips clipped positions).
  - Reverse + soft-clip (orient + clip together).
  - CTOB strand — forward orientation (closes the OT/CTOB grouping).
  - CTOT strand — reverse orientation (closes the OB/CTOT grouping).

### Why

`bismark_methylation_extractor` (Perl, line 1619-1621 SE + 2877-2886 PE)
reverses both the XM string AND the expanded CIGAR for `-` strand reads
because BAM stores reverse-complemented reads aligned to the `+` strand.
Walking BAM-stored XM with BAM-stored CIGAR for `-` strand records puts
M-bias positions end-to-end-flipped on every reverse-strand call. This
iterator centralizes the orientation correction in `bismark-io` so the
extractor (and any future consumer doing per-cycle XM analysis)
inherits the corrected stream.

### Compatibility

- bismark-dedup's `=1.0.0-beta.5` path-dep pin needs updating to `=1.0.0-beta.6`
  to compile (workspace constraint), but bismark-dedup's source is unchanged
  and its own version stays at `1.2.x-beta.y`.
- No existing API touched. `xm()`, `cigar()`, `record_strand()`, etc.
  still return the same BAM-stored bytes/types.

### Perl reference

- `bismark_methylation_extractor:1619-1621` (SE `meth_call` reverse)
- `:1933-1939` (PE R1)
- `:2877-2886` (PE R2 + CIGAR reverse)
- `:4422-4425` (yacht)

## [1.0.0-beta.5] — 2026-05-25

UMI plumbing on `BismarkRecord` for Phase B of the v1.2 UMI/RRBS epic.
Additive only; non-UMI workflows are byte-for-byte unchanged.

### Added

- `BismarkRecord::umi() -> Option<&Umi>` accessor and
  `BismarkRecord::set_umi(Option<Umi>)` setter.
- `BismarkRecord::from_noodles_record_with_umi(inner, extractor)` —
  constructs a record AND pre-extracts the UMI from its qname using
  the supplied `bismark_io::umi::extract_*` function.
- `BamReader::records_with_umi(extractor)` and the same on
  `ThreadedBamReader`, `SamReader`, `CramReader`, `AnyReader`. Each
  yields `BismarkRecord`s with `umi` populated at parse time.
- New type alias `bismark_io::Umi = smallvec::SmallVec<[u8; 16]>` —
  stack-allocated for ≤16-byte UMIs (covers all known Bismark
  workflows including 8-mer dual-UMI without the `+`), heap fallback
  for longer UMIs (e.g. 17-byte dual-UMI of form `XXXXXXXX+YYYYYYYY`).

### Changed

- `BismarkRecord` gains a private `umi: Option<Umi>` field. The struct
  has only private fields, so this is a semver-additive change — no
  external caller can pattern-match or struct-literal-construct.

### Dependencies

- New: `smallvec = "=1.13.2"` (widely used; one transitive dep).
  Pinned exact-version per the workspace's policy.

## [1.0.0-beta.4] — 2026-05-25

UMI extraction helpers for Bismark qnames, supporting Perl
`deduplicate_bismark v0.25.1`'s `--barcode`/`--umi` and `--bclconvert`
modes. Additive within the beta line; existing API surface unchanged.

### Added

- New module `bismark_io::umi` with two zero-copy public functions:

  - **`extract_barcode(&[u8]) -> Option<&[u8]>`** — extracts the
    tail-of-qname UMI per Perl regex `:([\w\+]+)$` at
    `deduplicate_bismark:659`. Used by Perl's `--barcode`/`--umi`
    modes.

  - **`extract_bclconvert(&[u8]) -> Option<&[u8]>`** — extracts the
    internal-position UMI in bcl-convert read ID format per Perl
    regex `:([CAGTN\+]+)_\d:N:\d:([CAGTN\+]+)$` at
    `deduplicate_bismark:650`. Used by Perl's `--bclconvert` mode.

- 29 unit tests (12 + 17) covering every edge case enumerated in the
  Phase A plan (empty input, no-colon, invalid chars, dual-UMI `+`
  separator, post-`fix_IDs` format, underflow guard, mode mismatch).

- 2 doc tests (one per function) with both the Perl-docs canonical
  example AND the post-Bismark-`fix_IDs` format that `bismark-dedup`
  will actually encounter in v1.2.

### Performance

- Both extractors are O(N) over the qname bytes, single-pass or
  reverse-scan. Zero allocation: return type is `Option<&[u8]>`
  borrowed from the input.
- Hand-rolled byte comparison (no `regex` crate dependency added).
  Estimated ~100 ns per call on a 100-byte qname — negligible vs
  dedup hash work.

### Phase context

This release lands as part of the v1.2 UMI/RRBS epic. Phase A ships
the extractors; the consumer-side integration (UMI-aware `DedupKey`
and pipeline) lands in `bismark-dedup v1.2.0-beta.1`. Real-data
byte-identity vs Perl `deduplicate_bismark v0.25.1` is validated on a
synthesized-UMI RRBS dataset.

## [1.0.0-beta.3] — 2026-05-25

Magic-byte file-format detection for reader-side dispatch. Tolerates
mis-named files (`.bam` containing SAM bytes, `.sam` containing CRAM
bytes, files with no extension at all) — matches Perl Bismark's
behaviour. Writers continue to use extension-based dispatch (the file
doesn't exist yet at writer-call time).

### Added

- **`AlignmentKind::from_extension(&Path)`** — pure extension dispatch,
  I/O-free. Preserves the pre-`1.0.0-beta.3` behaviour of `from_path`.
  Used by `open_writer` and any caller that explicitly wants
  extension-only.
- Three new `BismarkIoError` variants emitted by the new `from_path`:
  `TooShortToDetect { path, bytes_read }`, `UnrecognizedFormat
  { path, magic_first_byte }`, `UnrecognizedBgzfPayload { path,
  payload_head }`.

### Changed (behaviour)

- **`AlignmentKind::from_path(&Path)` now opens the file**, reads + (for
  BGZF) decompresses the first block, and dispatches based on actual
  file content rather than file extension. A SAM file with a `.bam`
  extension is now correctly classified as SAM (previously it would
  have errored at parse time with a BAM-decoder error).
- `open_reader` uses the new sniff behaviour; `open_writer` is migrated
  to `from_extension` so its semantics are unchanged.
- The `UnsupportedKind` variant's doc-comment is narrowed: it's now
  emitted only by `from_extension` (writer-side). Reader-side dispatch
  emits the new variants above instead.

### Changed (error variants)

- Downstream consumers that **exhaustive-match** on `BismarkIoError`
  need a new arm for each new variant. Consumers using `#[from]`
  propagation are unaffected.

### Performance

- `from_path` is no longer I/O-free. Per-call cost is ~100-700 µs
  (dominated by the ~200-500 µs BGZF inflate of one block). For
  hot-path callers iterating over many input files, consider caching
  the result.
- `from_extension` is unchanged: ~0.2 µs per call.

### Pinning

Downstream consumers pinning `=1.0.0-beta.2` should bump to
`=1.0.0-beta.3` when they want magic-byte detection. `bismark-dedup
v1.1.0-beta.2` requires `=1.0.0-beta.3`.

## [1.0.0-beta.2] — 2026-05-24

Additive release adding parallel BGZF reader/writer support, used by
`bismark-dedup` v1.1.0-beta.1's `--parallel N` flag. **Public API
unchanged for existing callers** — `BamReader<R>`, `BamWriter<W>`,
`AnyReader`, `AnyWriter`, `open_reader`, `open_writer` all behave
identically to v1.0.0-beta.1.

### Added

- **`ThreadedBamReader`** — new concrete struct wrapping
  `noodles_bam::io::Reader<noodles_bgzf::io::MultithreadedReader<File>>`.
  Mirrors `BamReader`'s public API (`header()`, `records()`,
  `from_path`, `from_path_without_sort_check`) but with a worker-thread
  pool for parallel BGZF block decompression. Use when consuming large
  BAM files where decompression is the bottleneck.

  ```rust
  use std::num::NonZero;
  use bismark_io::ThreadedBamReader;

  let mut reader = ThreadedBamReader::from_path(
      Path::new("sample.bam"),
      NonZero::new(4).unwrap(),  // 4 BGZF decoder worker threads
  )?;
  for result in reader.records() {
      // ...
  }
  ```

- **`ThreadedBamWriter`** — symmetric. Wraps `noodles_bam::io::Writer<noodles_bgzf::io::MultithreadedWriter<File>>`.
  Same `#[must_use]` finalisation contract as `BamWriter`. The BGZF
  EOF marker is verified-equivalent to single-threaded output by the
  `threaded_bam_writer_finish_writes_bgzf_eof_marker` test. Block
  boundaries on disk **will** differ between threaded and single-threaded
  output (different worker assignment patterns produce different block
  sizes), but the **decoded record stream is byte-identical** — which is
  what byte-identity gates in downstream consumers (e.g. bismark-dedup's
  Phase F gate against Perl baseline) actually check.

- 7 new tests covering: order preservation, strand classification
  preservation, EOF-marker validity, threaded-writer→single-threaded-reader
  cross-compatibility round-trip.

### Not added (deferred to a later beta)

- Generic refactor of `BamReader<R>` / `BamWriter<W>` (the "option (a)"
  approach from the v1.1 plan) — superseded by the simpler additive-struct
  approach. The existing `BamReader<R>` and `BamWriter<W>` remain unchanged.
- `open_reader_with_threads` / `open_writer_with_threads` path-dispatching
  helpers — out of scope because `AnyReader`/`AnyWriter` don't need to
  unify threaded + single-threaded variants under one enum (the threaded
  path in `bismark-dedup` v1.1 calls `ThreadedBamReader`/`ThreadedBamWriter`
  directly, bypassing the `AnyReader` enum).

### Pinning

Downstream consumers pinning `=1.0.0-beta.1` should bump to `=1.0.0-beta.2`
when they want the threaded readers/writers. `bismark-dedup v1.1.0-beta.1`
requires `=1.0.0-beta.2`.

### Downstream-measured performance

`bismark-dedup v1.1.0-beta.1`'s `--parallel N` path (which uses
`ThreadedBamReader` + `ThreadedBamWriter`) is **~4.8× faster at N=4**
than its single-threaded counterpart on real-data WGBS, with
byte-identical output across N. Verified on two independent samples:
**10M PE WGBS** (4.88× speedup, 455 MB RSS) and **full PE WGBS,
SRR24827373 Buckberry 2023, 55M reads** (4.75× speedup, 3.4 GB RSS).
The architecture ceiling holds across 6.6× input-size scaling; N=8
saturates (no further speedup) because only BGZF (de)compression
parallelizes — the dedup state itself is single-threaded. Memory cost
of threading is negligible (≈30-40 MB BGZF queue overhead). See
bismark-dedup's CHANGELOG for the full per-N curves on both datasets.

## [1.0.0-beta.1] — 2026-05-24

First **public pre-release** of `bismark-io` on crates.io. Feature-complete
and test-passing per the v1.0 contract; published as beta to allow a
period of integration feedback before the immutable 1.0.0 lands.

The beta is intended to be **functionally identical** to what 1.0.0 will
ship — no breaking changes are planned between `1.0.0-beta.N` and `1.0.0`.
Downstream consumers pinning `=1.0.0-beta.1` and `=1.0.0` should observe
the same behaviour.

`bismark-io` is the shared library crate for Bismark's Rust rewrite.
Wraps the [`noodles`](https://github.com/zaeleus/noodles) family to
provide Bismark-aware BAM/SAM/CRAM I/O: strand-classified record types,
tag-decoded accessors, CIGAR-aware position helpers.

This release is feature-complete for the v1.0 scope defined in `DESIGN.md`.
Downstream binary crates (`bismark-dedup`, `bismark-bedgraph`,
`bismark-extractor`, `bismark-coverage2cytosine`) will pin to `=1.0.0-beta.1`
during the beta period, then bump to `=1.0.0` at final release.

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
  > **Update (2026-05-24, Phase A of v1.1 rayon epic):** published as
  > `1.0.0-beta.1` to crates.io. See the `[1.0.0-beta.2]` entry above for
  > the v1.1 successor.

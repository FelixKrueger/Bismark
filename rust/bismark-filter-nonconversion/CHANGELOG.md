# Changelog

All notable changes to `bismark-filter-nonconversion` are documented here.
Format loosely follows [Keep a Changelog](https://keepachangelog.com/).

## [1.0.0-alpha.1] — 2026-05-31

Initial Rust port of Bismark Perl's `filter_non_conversion` (v0.25.1). Binary
`filter_non_conversion_rs`.

### Added
- Non-CG-conversion read/read-pair filtering over the `XM` tag, with all three
  Perl decision modes: `--threshold` (default 3), `--consecutive`, and
  `--percentage_cutoff` + `--minimum_count` (default 5).
- Single-end (`-s`) / paired-end (`-p`) modes + `@PG`-based auto-detection
  (via `bismark-io::detect_paired_from_header`).
- Three byte-identical outputs per input: `*.nonCG_filtered.bam`,
  `*.nonCG_removed_seqs.bam`, `*.non-conversion_filtering.txt`.
- Multiple positional inputs, each processed independently; the run-time line
  is written only to the last file's report (matching Perl).
- Pure-Rust BAM I/O via noodles (raw `RecordBuf` pass-through preserving
  unmapped reads and all tags); `mimalloc` global allocator.
- Hermetic byte-identity test suite (Perl-generated goldens via
  `tests/data/generate_goldens.sh`) + edge-case tests + an `#[ignore]`d,
  env-gated real-data gate (`FNC_PERL`/`FNC_REAL_SE`/`FNC_REAL_PE`).

### Validated
- Byte-identical to Perl v0.25.1 across SE/PE × default / `--consecutive` /
  `--percentage_cutoff` / `--threshold 5`, `@PG` auto-detect, an unmapped read
  in SE, and the `N/A`-report (header-only, non-`.bam`-named) path.
- Edge cases pinned: PE lone-trailing-R1 die (valid partial BAMs + 0-byte
  report), empty-`.bam` die with no output, unmapped mate in PE die,
  coordinate-sorted PE rejection, multi-file timing placement.

### Deviations from Perl (documented)
- BAM input only; `--samtools_path` accepted and ignored (pure-Rust I/O).
- No `--parallel` (single-threaded + mimalloc).
- `--help` exits 0 (Perl exits 1); truncation detection is noodles-native.
- An empty `XM` *value* yields `""` (Rust structured tag read) rather than the
  Perl regex's garbage capture — equivalent for all real data.

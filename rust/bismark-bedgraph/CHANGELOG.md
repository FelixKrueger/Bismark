# Changelog

All notable changes to `bismark-bedgraph` will be documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versioning: [SemVer](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **In-memory streaming ingestion** (`Aggregator::add_min_owner`) and a
  write/feed split (`output::write_outputs_from_sorted`), so an in-process caller
  (the methylation extractor) can tee calls directly into the aggregator instead
  of re-reading the per-context files, then write the bedGraph/coverage from the
  same sorted records. Ownership resolves to the minimum basename — equivalent to
  the first file in lexicographically-sorted argv order, so the file-reading
  output is reproduced regardless of tee arrival order. Additive: the file path
  (`run()`/`add()`/`write_outputs`) is unchanged and byte-preserved.

### Changed

- **`mimalloc` global allocator** for `bismark2bedGraph_rs` (matching
  `bismark-extractor`). The in-memory `(chr, pos)` aggregation map grows through
  many allocations; mimalloc is ~12% faster than the system allocator on a full
  `--CX` run (973 s → 854 s, ~4.4× vs Perl). Allocator-only — byte-identical.
- Internal: `ChrMeta` stores the owner basename and builds the bytewise ordering
  key lazily in `into_sorted()` (was eager at intern time). Byte-neutral for the
  file path (gated by the 8 ownership regression tests); enables `add_min_owner`
  to revise ownership before emission.

### Investigated and rejected

- A `--parallel` flag to parse the per-context input files **concurrently** was
  prototyped (byte-identical, N-invariant) but **rejected after measurement.**
  The read+aggregate phase is **memory-bandwidth-bound**, so concurrent
  multi-GB map builds *anti-scale*: on a full `--CX` gate sequential beat 6-way
  parallel (854 s vs 1125 s) even with mimalloc, and a controlled experiment
  (parse-only, interleaved, %CPU + thread-state sampling) confirmed the cause is
  the shared memory bus + CHH-file load imbalance — fixed by neither a faster
  allocator nor sharding. Parse/aggregate stays single-threaded. Full
  investigation + data: `plans/05302026_bedgraph-parallel-parse/`.

## [1.0.0-beta.1] — 2026-05-29

Initial Rust port of Bismark Perl's `bismark2bedGraph` (v0.25.1). Binary
installs as `bismark2bedGraph_rs` during the Perl→Rust coexistence period.
Epic [#797](https://github.com/FelixKrueger/Bismark/issues/797); spec
sub-issue [#802](https://github.com/FelixKrueger/Bismark/issues/802); PR
[#893](https://github.com/FelixKrueger/Bismark/pull/893).

**Byte-identity:** decompressed-content identical to Perl `bismark2bedGraph`
v0.25.1 across the full SE+PE × default+`--CX` matrix on real 10M-read GRCh38
data (10M SE directional + 10M PE deduplicated). **Speed:** ~3.4× faster than
Perl on 10M PE default (8 s vs 27 s).

### Added

- New crate `bismark-bedgraph` (library + `bismark2bedGraph_rs` binary):
  consumes the methylation extractor's per-context call files and emits a
  sorted gzip bedGraph + coverage file.
- **In-memory aggregation** by `(chr, pos) → (methylated, unmethylated)`
  (`FxHashMap`), replacing Perl's per-chromosome temp files + UNIX `sort`.
- **Chromosome ordering** reproduced exactly: ownership = first input file in
  argv order; output order = bytewise sort of the synthetic temp-filename
  strings (Perl `sort @temp_files`). Verified against Perl, including a
  chromosome present only in a later input file.
- **Faithful C `%.15g`** methylation-percentage formatter (`fmt_g`), validated
  against C `printf` across ~2.8M values incl. the scientific-notation
  boundary (e.g. `1/1e7 → 1e-05`).
- Full flag surface: `-o/--output`, `--dir`, `--cutoff`, `--CX/--CX_context`,
  `--zero_based`, `--ucsc`, `--no_header`, `--remove_spaces`, `--counts`,
  `--buffer_size`, `--gazillion/--scaffolds`, `--ample_memory`, `--version`,
  `--man`. Output filename derivation matches Perl (including the latent
  `.bedGraph.gz.bismark.zero.cov` quirk).
- **`flate2` `zlib-rs` backend** (pure-Rust, no C/cmake) for gzip read and
  single-stream writes.
- **`gzp` parallel block-gzip** (`deflate_rust`) for the two large output
  streams (bedGraph + coverage); under feature unification the codec is
  zlib-rs, so this is parallel zlib-rs. Closes the perf gap a flamegraph
  attributed to serial DEFLATE (~70% of runtime).
- Hermetic CI byte-identity tests (`tests/byte_identity_fixtures.rs`) against
  Perl-generated expected files; env-gated real-data gate
  (`tests/byte_identity_real_data.rs`); live harness
  (`scripts/bedgraph_byte_identity.sh`).

### Notes / intentional divergences from Perl

- **Decompressed-content identity, not raw `.gz` bytes** — `zlib-rs`/`gzp`
  DEFLATE output differs from GNU `gzip` byte-for-byte but decompresses to
  identical content.
- `--buffer_size`, `--ample_memory`, `--gazillion`/`--scaffolds` are
  **accepted-but-ignored** (in-memory aggregation needs no external sort).
  Mutually-exclusive combinations are still rejected for CLI parity.
- `--gazillion` scaffold mode (Perl `sort -V`) is **not** replicated;
  byte-identity is guaranteed for the default chromosome ordering only.
- `--remove_spaces` produces **no** `.spaces_removed.txt` intermediate (the
  read-id field is unused, so it has no effect on the output).
- Positions parse as `u32` (ample for any real chromosome); a malformed line
  fails with a specific error message (missing field vs bad position).
- No `coverage2cytosine` — out of scope (a separate future crate).

### Performance journey (for the record)

1. v1 used `flate2`'s default `miniz_oxide` — byte-identical but ~2× slower
   than Perl.
2. Switching to `flate2` `zlib-rs` alone barely helped (60 s → 57 s): a
   flamegraph showed the cost was *serial* DEFLATE, not the backend.
3. Adding `gzp` parallel compression closed the gap → 8 s (~3.4× faster than
   Perl), byte-identity preserved.

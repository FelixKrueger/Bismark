# Changelog

All notable changes to `bismark-dedup` will be documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versioning: [SemVer](https://semver.org/spec/v2.0.0.html).

## [1.0.0] — 2026-05-24

First stable release. Feature-complete Rust port of Bismark Perl v0.25.1's `deduplicate_bismark` script — **verified byte-identical** to Perl's output on the 10M PE WGBS audit dataset (7,969,632 retained qnames + 294-byte dedup report).

The binary installs as `deduplicate_bismark_rs` during the v0.26 → v1.0 coexistence period; the `_rs` suffix is dropped once the Perl scripts move to a `legacy/` directory.

### Added

- **Single-end and paired-end deduplication** via `pipeline::run_single`.
  - SE key: `(strand, chr_id, key_pos)` where `key_pos = alignment_start` for forward strands (OT/CTOB) or `cigar.reference_end(start)` for reverse strands (CTOT/OB). Matches Perl lines 343–388.
  - PE key: `(pair_strand, chr_id, start, end)` where `start`/`end` come from R1/R2 depending on pair-strand direction. Matches Perl lines 397–492.
- **Multi-file mode** via `pipeline::run_multiple` (`--multiple` flag). All inputs accumulate into one shared dedup state; cross-file `@SQ` name-set consistency validated at startup.
- **Auto-detection of SE vs PE mode** from the input BAM's `@PG ID:Bismark` line (matches Perl lines 90–116). Falls back to the explicit `-s`/`-p`/`--single`/`--paired` flags.
- **Output format mirrors input**: CRAM in → CRAM out (with `--cram_ref`), BAM in → BAM out, SAM in → SAM out.
- **clap-derive CLI** with the full Perl flag surface: `-s`/`-p`/`--bam`/`--sam`/`--cram_ref`/`-o`/`--output_dir`/`--multiple`/`-V`/`--help`. Compat-only flags (`--parallel`, `--samtools_path`) silently accepted.
- **v1.0-deferred-flag stubs** for `--barcode`/`--umi`/`--bclconvert` (RRBS UMI mode) — exit non-zero with a clear "use Perl `deduplicate_bismark`" message. v1.1 will add UMI support.
- **Perl-verbatim joke** for the long-deprecated `--representative` flag: `"Deduplication in '--representative' mode is no longer supported. Please stop wanting that."`
- **DedupKey type** (`#[repr(C)]`, 16 bytes pinned by compile-time `const _: () = assert!(size_of::<DedupKey>() == 16);`). Shared between the `seen` hash-set and the `duplicate_positions` counter — **structurally eliminates the 97-position drift bug** present in the prior-art Rust port at `alanhoyle/Bismark@rust-port` (whose `pack_pos_pe(strand, chr, start)` dropped the `end` component when keying the positions counter).
- **DedupReport** with `format()` producing Perl-byte-equal output (sprintf-style `%.2f` percentages; `N/A` on `count == 0`).
- **Byte-identity test gate** (`tests/byte_identity_real_data.rs`, `#[ignore]`'d) running on the 10M PE WGBS dataset (`SRR24827378_10M_R1_val_1_bismark_bt2_pe.bam`, 8,592,524 records). Compares Rust output qname set vs Perl baseline and report bytes for exact equality.
- **Twelve integration tests** (`tests/integration_dedup.rs`) covering SE/PE/CTOT (non-directional) dedup, R1-R2 adjacency invariant, report-byte snapshot vs Rust formatter, `--outfile` path-strip behaviour, `--multiple` cross-file dedup, `--multiple` empty-file1 ordering invariant, mixed-format rejection, `@SQ` mismatch detection, and the `removed = 0` no-duplicate report.
- **Compile-time provenance** via `version_string()` — `deduplicate_bismark_rs 1.0.0 (<os>/<arch>)`. v1.1 will extend this with vergen-driven git-hash + ISO-8601 build timestamp.

### Design contract

- **No `samtools` subprocess**, no `htslib` C-link, no `unsafe` blocks. All BAM/SAM/CRAM I/O via `bismark-io` v1.0 (which uses pure-Rust [noodles](https://github.com/zaeleus/noodles)).
- **Strand classification is eager** at parse time (per-record from XR/XG tags). Pair-strand is R1-derived and used for output routing; this is enforced at the type level via separate `BismarkRecord::record_strand()` and `BismarkPair::pair_strand()` methods.
- **`BamWriter::finish()` consumes by value** — `#[must_use]` annotation ensures callers can't accidentally drop the writer with un-flushed data.
- **`run_multiple` peeks file1 BEFORE opening the output writer** — empty-file1 errors leave no header-only BAM on disk (PLAN §10.9 invariant).

### MSRV

Rust **1.89.0**. Required by `bismark-io` v1.0 → `noodles-bam` 0.89.

### Test coverage

- **199 tests pass** workspace-wide (90+ unit tests in `bismark-dedup` + the entire `bismark-io` v1.0 suite).
- **1 `#[ignore]`'d byte-identity gate** verified against real 10M PE WGBS data; not run by default to avoid the ~2-minute wall time per `cargo test`.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo fmt --all -- --check`: clean.

### Out of scope (deferred to v1.1+)

- **RRBS UMI dedup mode** (`--barcode`/`--umi`/`--bclconvert`). v1.0 stubs these flags with a clear deferral error.
- **Multi-threading** (`--parallel N > 1`). v1.0 silently accepts the flag for CLI compatibility but runs single-threaded.
- **vergen-based provenance** (git hash + ISO-8601 build timestamp in `--version`).
- **Sorted-input auto-renaming** — v1.0 errors on `SO:coordinate` BAMs with a clear "re-sort with `samtools sort -n` first" message.

### Not yet published to crates.io

By design — within the Bismark workspace, path-dep usage is the supported integration model. crates.io publication is deferred until the v1.0 → v1.1 stabilisation period.

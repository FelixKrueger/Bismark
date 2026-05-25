# Changelog

All notable changes to `bismark-dedup` will be documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versioning: [SemVer](https://semver.org/spec/v2.0.0.html).

## [1.1.0-beta.1] — 2026-05-24

First v1.1 pre-release. Adds **BGZF-threaded BAM I/O** behind `--parallel N`,
keeping every v1.0 byte-identity guarantee intact.

### Added

- **`--parallel N`** now does real work for BAM inputs/outputs. The flag
  previously existed for CLI compatibility but was silently ignored; with
  `N > 1` and BAM I/O, the pipeline now uses
  `noodles_bgzf::MultithreadedReader`/`MultithreadedWriter` to parallelise
  the BGZF (de)compression step.
- **`pipeline::run_single_parallel`** and **`pipeline::run_multiple_parallel`** —
  additive library entry points; existing `run_single`/`run_multiple`
  retain their v1.0 signatures and single-threaded behaviour.
- **Startup line** `BGZF threading: N worker(s) per reader/writer` printed
  to stderr when the threaded path is taken, so users can confirm the
  flag is in effect.
- **CRAM fallback warning** — `--parallel N` with a CRAM input or output
  emits a single-line stderr warning and runs single-threaded; the
  parallel path is BAM-only in this release.
- **Seven new integration tests** in `tests/integration_dedup.rs`:
  - `pe_parallel_4_produces_same_qname_set_as_single_threaded` (2000 PE pairs spanning multiple BGZF blocks)
  - `se_parallel_4_produces_same_qname_set_as_single_threaded` (3000 SE reads)
  - `multiple_parallel_4_produces_same_qname_set_as_single_threaded` (1000 pairs per file × 2 files)
  - `pe_parallel_4_preserves_r1_followed_by_r2_adjacency` (1500 PE pairs)
  - `parallel_zero_is_rejected_at_validate`
  - `cram_with_parallel_n_logs_warning_and_runs_single_threaded` — verifies the CRAM fallback warning fires exactly once and the threaded-path startup banner does NOT appear (proving the warn-and-fall-back contract)
  - `parallel_4_multiple_mode_output_ends_with_bgzf_eof_marker` — confirms the v1.1 `ThreadedBamWriter` emits the canonical 28-byte BGZF EOF terminator end-to-end through `run_multiple_parallel` (PLAN V8)
- **Fixture-size guards** in each `--parallel` equivalence test:
  `assert!(bam_size > 64 KiB)` to prevent future regressions that
  would silently collapse the synthetic BAM into a single BGZF block
  (leaving the `MultithreadedReader`'s in-order frame contract
  unstressed). Fixtures use varied-base + varied-XM data to defeat
  BGZF dictionary compression.

### Changed

- **`bismark-io` pin bumped to `=1.0.0-beta.2`** — additive: re-exports
  `ThreadedBamReader` / `ThreadedBamWriter` used by the new entry points.

### Validated

- **`--parallel 0`** is now an explicit `InvalidParallelValue` error at
  CLI-validate time. clap's `u32` parser previously accepted 0; the
  validate stage rejects it before any I/O begins.
- **`--parallel N` with N > 4** emits a one-line stderr warning about
  measured saturation past N=4 (the dedup state is single-threaded, so
  only BGZF (de)compression scales; the Phase D oxy benchmark showed
  N=8 = N=4 in wall-time). The run still succeeds — this is
  informational, not a hard cap. Users on much-bigger metal who want to
  probe the curve themselves can still pass `--parallel 16`, `--parallel
  32`, etc., and will just see a warning that the value is past the
  measured sweet spot.

### Byte-identity contract preserved

- The retained-qname set under `--parallel N` is equal to the
  single-threaded set across SE, PE, and `--multiple` modes (V3 of the
  plan). PE pair adjacency is preserved by `MultithreadedReader`'s
  in-order FIFO frame contract (V4).
- The existing `byte_identity_real_data_10m_pe_wgbs` gate (`#[ignore]`'d) is
  unchanged and still passes. A new sibling test
  `byte_identity_real_data_10m_pe_wgbs_parallel_4` (also `#[ignore]`'d)
  asserts the same byte-identity invariant on the BGZF-threaded path —
  retained-qname set and report bytes must both equal Perl v0.25.1's
  single-threaded baseline. Run with:
  ```sh
  BISMARK_REAL_DATA_DIR=<dataset-dir> \
    cargo test --release -- --ignored --exact byte_identity_real_data_10m_pe_wgbs_parallel_4
  ```
  `--exact` matters because the v1.0 gate name is a prefix of the v1.1
  one (substring-match would trigger both).
  The common body is shared via `run_byte_identity_at_parallel(parallel)` so
  the two tests cannot drift apart.
- Phase D extends the gate to `--parallel 2` and `--parallel 8` via
  `byte_identity_real_data_10m_pe_wgbs_parallel_2` and
  `_parallel_8` (also `#[ignore]`'d) — same `run_byte_identity_at_parallel`
  body, just different parameter. All four N values produce identical
  retained-qname set + identical report bytes on the 10M PE WGBS oxy
  dataset.

### Measured speedup + memory (oxy, 2026-05-24, median of 3 runs)

Real-data WGBS dedup of the 10M PE `SRR24827378` dataset (8,592,524
records, GRCh38). Same hardware (`dockyard-oxy-0`, Amazon Linux 2023
x86_64), same input, same dataset directory. All runs produce identical
7,699,136 retained qnames + 294-byte report — **byte-identity preserved
across N**, validated by the four `byte_identity_real_data_10m_pe_wgbs[_parallel_N]`
gates above.

| `--parallel` | Wall-time (min / median / max) | Peak RSS (median) | Speedup vs N=1 (median) |
|-------------:|-------------------------------:|------------------:|------------------------:|
| 1 | 81.39 / 81.46 / 81.77 s | 455 MB | 1.00× |
| 2 | 30.48 / 30.54 / 30.56 s | 456 MB | **2.67×** |
| 4 | 16.66 / 16.71 / 16.94 s | 457 MB | **4.88×** |
| 8 | 16.57 / 16.68 / 16.89 s | 457 MB | 4.88× (saturation) |

**Second data point — full PE WGBS (~55M reads, oxy, 2026-05-25, median of 3):**

Same hardware, independent biological sample (`SRR24827373` Buckberry
2023, ~11 GB BAM, 75,272,478 records). Byte-identity verified at N=4
via `byte_identity_real_data_full_pe_wgbs_parallel_4` (`#[ignore]`'d):
**64,636,610 retained qnames + 359-byte report — Rust `--parallel 4`
output byte-identical to Perl v0.25.1**.

| `--parallel` | Wall-time (min / median / max)  | Peak RSS (median) | Speedup vs N=1 (median) |
|-------------:|--------------------------------:|------------------:|------------------------:|
| 1 | 691.08 / 691.52 / 692.68 s (~11:32) | 3.40 GB | 1.00× |
| 2 | 255.54 / 255.61 / 255.77 s (~4:16)  | 3.44 GB | **2.71×** |
| 4 | 144.91 / 145.67 / 148.30 s (~2:26)  | 3.44 GB | **4.75×** |
| 8 | 145.45 / 146.24 / 148.31 s (~2:26)  | 3.44 GB | 4.73× (slight regression) |

The 4.75× ceiling at N=4 on 55M reads is consistent with the 4.88×
ceiling on 10M reads — the architecture limit holds across 6.6× input
scale. N=8 even shows a tiny regression (146.24 vs 145.67 s), confirming
that "saturation" isn't an artifact of small-data noise. Peak RSS
scales with input size (≈ 7.5× from 10M to full PE) but is essentially
flat across N (BGZF queue overhead ≈ 30-40 MB).

**Methodology:** measurements taken with `/usr/bin/time -v` wrapping the
release-mode `deduplicate_bismark_rs` binary directly (no `cargo test`
wrapper). The wrapper's post-dedup qname-set comparison would add ~60 s
of constant-cost test scaffolding to every run, masking the real
parallel speedup as a smaller ratio.

**Caveat:** measured on a single host (`dockyard-oxy-0`), single input
dataset (10M PE WGBS), single dedup workload. Absolute numbers will
vary with hardware and input characteristics. The N=2/4/8 byte-identity
claim is validated by the corresponding `#[ignore]`'d integration tests
in `tests/byte_identity_real_data.rs`. Speedup ceiling at ~5× reflects
that the dedup state itself is single-threaded; only BGZF
(de)compression parallelizes — N=8 shows no improvement over N=4
because the BGZF queue depth has saturated the I/O parallelism budget.
Memory cost of threading is negligible (≤2 MB across N).

### Out of scope (still deferred)

- RRBS UMI dedup mode (`--barcode`/`--umi`/`--bclconvert`).
- vergen-based provenance string in `--version`.
- CRAM parallelism (current release is BAM-only for the threaded path).

## [1.0.0-beta.1] — 2026-05-24

First **public pre-release** of `bismark-dedup`. Feature-complete and
verified byte-identical to Bismark Perl v0.25.1 on real WGBS data;
published as beta to allow a period of integration feedback before the
immutable 1.0.0 lands on crates.io.

The beta is intended to be **functionally identical** to what 1.0.0 will
ship: no breaking changes are planned between 1.0.0-beta.N and 1.0.0.

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

> **Update (2026-05-24):** published to crates.io as `bismark-dedup 1.0.0-beta.1` in PR #825. The v1.1 successor is `1.1.0-beta.1` — see the `[1.1.0-beta.1]` entry above.

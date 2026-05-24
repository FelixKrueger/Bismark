# `bismark-dedup`

Rust port of [Bismark](https://github.com/FelixKrueger/Bismark)'s `deduplicate_bismark` script — removes PCR-duplicate alignments from Bismark BAM/SAM/CRAM files.

**Status:** v1.0.0-beta.1 — feature-complete, byte-identical to Bismark Perl v0.25.1's output on real WGBS data; first public pre-release on crates.io. The beta is functionally identical to what 1.0.0 will ship; published as beta to allow integration feedback before the immutable 1.0.0 lands. See [`CHANGELOG.md`](./CHANGELOG.md).

## What it does

Given a Bismark-aligned BAM/SAM/CRAM file, `bismark-dedup` removes duplicate alignments — typically arising from PCR amplification — and writes a deduplicated BAM/SAM/CRAM. Identification of duplicates uses Bismark's strand-aware key formula:

| Mode | Dedup key |
|------|-----------|
| Single-end | `(strand, chromosome, key_position)` — key_position is the alignment start (forward strands OT/CTOB) or `reference_end` (reverse strands CTOT/OB) |
| Paired-end | `(pair_strand, chromosome, fragment_start, fragment_end)` — pair_strand is R1-derived; fragment bounds come from R1's start + R2's reference_end (forward pairs) or R2's start + R1's reference_end (reverse pairs) |

This matches Bismark Perl `deduplicate_bismark` v0.25.1 exactly.

## Installation

```sh
# Within the Bismark workspace (path dependency):
cd rust/
cargo install --path bismark-dedup

# After release on crates.io (not yet published):
# cargo install bismark-dedup
```

During the Perl → Rust coexistence period, the binary installs as **`deduplicate_bismark_rs`** (with `_rs` suffix) so it can sit alongside the Perl `deduplicate_bismark` on `$PATH` without conflict.

## Usage

```sh
# Single-end auto-detect (from Bismark's @PG line):
deduplicate_bismark_rs sample_bismark_bt2.bam

# Paired-end explicit:
deduplicate_bismark_rs --paired sample_bismark_bt2_pe.bam

# Output to a specific directory + custom basename:
deduplicate_bismark_rs --paired --output_dir results/ --outfile my_sample sample.bam

# Multiple inputs combined into one sample:
deduplicate_bismark_rs --paired --multiple file1.bam file2.bam file3.bam

# SAM output instead of BAM:
deduplicate_bismark_rs --sam --paired sample.bam

# CRAM input/output (mirrors input format):
deduplicate_bismark_rs --paired --cram_ref genome.fa sample.cram
```

### Flag reference

| Flag | Purpose |
|------|---------|
| `<files>...` | One or more Bismark BAM/SAM/CRAM input files (positional) |
| `-s`, `--single` | Force single-end mode (auto-detected if neither `-s` nor `-p` is set) |
| `-p`, `--paired` | Force paired-end mode |
| `--bam` | Output BAM format (default) |
| `--sam` | Output SAM format |
| `--cram_ref <FASTA>` | Reference FASTA — required when input or output is CRAM |
| `-o`, `--outfile <NAME>` | Custom output basename (path prefix stripped per Perl regex chain) |
| `--output_dir <DIR>` | Output directory (created if missing) |
| `--multiple` | Treat all positional inputs as one combined sample |
| `--barcode`, `--umi` | **Not in v1.0** — errors with v1.1 deferral message |
| `--bclconvert` | **Not in v1.0** — errors with v1.1 deferral message |
| `--parallel <N>` | Accepted for compat, silently ignored (single-threaded in v1.0) |
| `--samtools_path <PATH>` | Accepted for compat, silently ignored (`bismark-dedup` is pure-Rust) |
| `--representative` | Errors with Perl-verbatim joke (deprecated upstream) |
| `-V`, `--version` | Print provenance string and exit |
| `-h`, `--help` | clap-generated help |

## Output

For input `sample.bam`:

| File | Contents |
|------|----------|
| `sample.deduplicated.bam` | Deduplicated BAM (PE: R1-then-R2 adjacency preserved) |
| `sample.deduplication_report.txt` | Byte-equal to Perl's report format |

For `--multiple` mode: the `.multiple.` infix appears in both filenames (`sample.multiple.deduplicated.bam`, `sample.multiple.deduplication_report.txt`) — matches Perl's convention.

## Byte-identity invariant

v1.0 is verified byte-identical to Bismark Perl v0.25.1's output on:

- **10M PE WGBS dataset** (`SRR24827378_10M_R1_val_1_bismark_bt2_pe.bam`, 8,592,524 records, GRCh38, directional): **7,969,632 retained qnames + 294-byte report — exact match.**

Test it locally:

```sh
BISMARK_REAL_DATA_DIR=/path/to/dataset/ \
  cargo test --release -- --ignored byte_identity_real_data
```

(Default: `~/Desktop/TrimG_Bismark_test/profiling/`. Skips with explicit reason if dataset absent.)

## Out of scope for v1.0 (deferred to v1.1+)

- **UMI / RRBS mode** (`--barcode`, `--umi`, `--bclconvert`) — use Bismark Perl `deduplicate_bismark` for these workflows.
- **Multi-threading** (`--parallel N > 1`) — single-threaded in v1.0; rayon-based chunked dedup deferred to v1.1.
- **Sorted-input auto-handling** — coordinate-sorted PE input is rejected with a clear "re-sort with `samtools sort -n` first" error message rather than auto-sorting.

## How is this different from Bismark Perl's `deduplicate_bismark`?

| | Perl `deduplicate_bismark` | `deduplicate_bismark_rs` (v1.0) |
|---|----|----|
| Runtime deps | `samtools` (subprocess) | None — pure Rust via [noodles](https://github.com/zaeleus/noodles) |
| Input formats | `.bam` (via samtools), `.sam`, `.sam.gz` | `.bam`, `.sam`, `.cram` |
| Output formats | `.bam` (via samtools), `.sam` | `.bam`, `.sam`, `.cram` |
| Strand classification | Re-derived per record from XR/XG | Eager at parse time; per-record vs pair-strand distinction enforced by the type system |
| `--multiple` `@SQ` validation | Implicit via samtools cat behaviour | Explicit equality check across inputs |
| PE mate qname validation | Implicit; assumes R1/R2 adjacency | Explicit via `BismarkPair::from_mates` qname check |

## Building from source

Requires Rust 1.89 or later (set in workspace `Cargo.toml`).

```sh
cd rust/
cargo build --release --package bismark-dedup
./target/release/deduplicate_bismark_rs --version
```

## Workspace dependencies

`bismark-dedup` depends on:

- [`bismark-io`](../bismark-io/README.md) (path dep, version `=1.0.0-beta.1`) — Bismark-aware BAM/SAM/CRAM I/O on top of noodles.
- [`clap`](https://crates.io/crates/clap) `=4.5.30` — CLI parsing
- [`rustc-hash`](https://crates.io/crates/rustc-hash) `=2.1.0` — `FxHashSet` for dedup-key storage
- [`thiserror`](https://crates.io/crates/thiserror) `=2.0.0` — typed errors
- [`anyhow`](https://crates.io/crates/anyhow) `=1.0.86` — binary-level error context (main only)

All deps exact-pinned, matching `bismark-io` v1.0's noodles version choices.

## License

GPL-3.0-only. Matches the upstream Perl Bismark license.

## See also

- [Bismark project](https://github.com/FelixKrueger/Bismark)
- [The Rust rewrite plan](../../../.claude/plans/05242026_bismark-dedup-v1/PLAN.md) (internal, not committed)
- [bismark-io design contract](../bismark-io/DESIGN.md)

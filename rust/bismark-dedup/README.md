# `bismark-dedup`

Rust port of [Bismark](https://github.com/FelixKrueger/Bismark)'s `deduplicate_bismark` script — removes PCR-duplicate alignments from Bismark BAM/SAM/CRAM files.

**Status:** v1.1.0-beta.1 — adds BGZF-threaded BAM I/O behind `--parallel N`
while preserving every byte-identity guarantee from v1.0.0-beta.1. See
[`CHANGELOG.md`](./CHANGELOG.md).

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

# From crates.io once 1.1.0-beta.1 is published (1.0.0-beta.1 is already there):
# cargo install bismark-dedup --version 1.1.0-beta.1
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

# v1.1: parallel BGZF (de)compression for BAM input/output:
deduplicate_bismark_rs --paired --parallel 4 sample_bismark_bt2_pe.bam
```

### `--parallel N` (v1.1)

`--parallel N` parallelises the BGZF (de)compression step for BAM inputs
and outputs using `noodles_bgzf::MultithreadedReader` / `MultithreadedWriter`.
The dedup state itself remains single-threaded — byte-identity with the
single-threaded path is preserved.

- **BAM only.** CRAM input or output with `--parallel N > 1` emits a
  one-line stderr warning and runs single-threaded. The parallel path is
  scheduled to gain CRAM support in a later release.
- **N = 0 is rejected** at CLI-validate time (`--parallel must be ≥ 1`).
  `N = 1` takes the existing single-threaded path.
- **Same output as N = 1.** Retained-qname set, PE pair adjacency, and
  report bytes are unchanged regardless of N.

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
| `--barcode`, `--umi` | **v1.2+**: engages UMI-aware dedup. UMI is the tail-of-qname token after the last `:` (Perl `deduplicate_bismark:659`). **v1.2.1+**: bcl-convert qname format is auto-detected — running this on bcl-convert reads fatal-errors with a clear hint pointing to `--bclconvert` (mirrors Perl's `test_readIDs_for_bclconvert`). |
| `--bclconvert` | **v1.2+**: engages UMI-aware dedup with bcl-convert internal UMI format (Perl `deduplicate_bismark:650`). Wins over `--barcode/--umi` if both flags are set. |
| `--parallel <N>` | v1.1: parallel BGZF (de)compression workers for BAM I/O (`N ≥ 1`). CRAM falls back to single-threaded with a warning. `N > 4` emits a soft "diminishing returns" warning — measured saturation at N=4 on 10M PE WGBS. |
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
# v1.0 gate (single-threaded):
BISMARK_REAL_DATA_DIR=/path/to/dataset/ \
  cargo test --release -- --ignored --exact byte_identity_real_data_10m_pe_wgbs

# v1.1 gate (--parallel 4, BGZF-threaded path):
BISMARK_REAL_DATA_DIR=/path/to/dataset/ \
  cargo test --release -- --ignored --exact byte_identity_real_data_10m_pe_wgbs_parallel_4
```

`--exact` matters: without it, the v1.0 invocation's name is a prefix of
the v1.1 one and would substring-match both gates.

Both gates compare against the **same** Perl v0.25.1 baseline — the v1.1
contract is that BGZF threading produces byte-identical output to the
single-threaded path, not merely byte-identical to itself.

(Default: `~/Desktop/TrimG_Bismark_test/profiling/`. Skips with explicit reason if dataset absent.)

## Out of scope (still deferred)

- **CRAM parallelism** — `--parallel N` is BAM-only in v1.1/v1.2; CRAM input or output falls back to single-threaded with a warning.
- **Sorted-input auto-handling** — coordinate-sorted PE input is rejected with a clear "re-sort with `samtools sort -n` first" error message rather than auto-sorting.

## Using as a library in other tools

`bismark-dedup` ships as both a binary (`deduplicate_bismark_rs`) AND a Rust library. Other tools can embed the dedup pipeline as a direct function call rather than spawning the binary — matching the [Trim Galore ↔ fastqc-rust](https://github.com/FelixKrueger/TrimGalore) integration model.

Add to your `Cargo.toml`:

```toml
[dependencies]
bismark-dedup = "=1.1.0-beta.1"
```

End-to-end example — dedup a Bismark BAM from within your own Rust pipeline:

```rust
use bismark_dedup::pipeline::run_single;
use std::path::Path;

fn dedup_bam(input: &Path, output: &Path) -> anyhow::Result<()> {
    // is_paired: true for PE, false for SE. Auto-detection is in `detect_paired_from_header`.
    let report = run_single(
        input,
        output,
        None,                                // cram_ref — None for BAM/SAM input
        /* is_paired = */ true,
        input.display().to_string(),         // file_label echoed in the report
    )?;

    // Report can be written to a file OR consumed in-memory:
    println!("dedup complete: {} records analysed, {} removed ({:.2}%)",
             report.count(), report.removed(),
             100.0 * report.removed() as f64 / report.count() as f64);
    Ok(())
}
```

Threaded variant (v1.1 — BGZF-parallel BAM I/O):

```rust
use bismark_dedup::pipeline::run_single_parallel;
use std::num::NonZero;
use std::path::Path;

fn dedup_bam_threaded(input: &Path, output: &Path) -> anyhow::Result<()> {
    let parallel = NonZero::new(4).unwrap();
    let report = run_single_parallel(
        input,
        output,
        /* is_paired = */ true,
        input.display().to_string(),
        parallel,
    )?;
    println!("{}", report.format());
    Ok(())
}
```

Note: `run_single_parallel` / `run_multiple_parallel` are **BAM-only** —
the threaded path does not accept SAM or CRAM input. Use `run_single` /
`run_multiple` for non-BAM inputs, or for `parallel == 1`.

Multi-file mode (one combined sample across N input BAMs):

```rust
use bismark_dedup::pipeline::run_multiple;

fn dedup_combined(inputs: &[std::path::PathBuf], output: &std::path::Path) -> anyhow::Result<()> {
    let report = run_multiple(
        inputs,
        output,
        None,
        true,  // is_paired
        inputs[0].display().to_string(),
    )?;
    report.write_to(&output.with_extension("deduplication_report.txt"))?;
    Ok(())
}
```

Lower-level primitives — if you want to drive the dedup loop yourself (e.g., on records already in memory, or with a custom input source):

```rust
use bismark_dedup::{DedupKey, DedupState};
use bismark_io::BismarkStrand;

let mut state = DedupState::new();
let key = DedupKey::pe(BismarkStrand::OT, /* chr_id */ 0, /* start */ 100, /* end */ 200);

if state.observe(key) {
    // record is unique — emit it to your output
} else {
    // record is a duplicate — drop it
}

let report = state.into_report("my_sample.bam".to_string());
// report.format() returns the Perl-byte-equal report string.
```

See `cargo doc --open --package bismark-dedup` for the full library API. The same algorithm that powers the `deduplicate_bismark_rs` binary is available to your code with zero subprocess overhead.

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

- [`bismark-io`](../bismark-io/README.md) (path dep, version `=1.0.0-beta.2`) — Bismark-aware BAM/SAM/CRAM I/O on top of noodles.
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

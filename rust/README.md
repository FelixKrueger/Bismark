# Bismark Rust rewrite

Active rewrite of Bismark from Perl to Rust. Progress is tracked at
the [Bismark Rust rewrite project](https://github.com/users/FelixKrueger/projects/1).

Working branch: [`rust/iron-chancellor`](https://github.com/FelixKrueger/Bismark/tree/rust/iron-chancellor).

## Layout

- `bismark-io/` — shared library: BAM/SAM/CRAM I/O via [noodles](https://github.com/zaeleus/noodles). See `bismark-io/DESIGN.md` for the design contract.
- Per-binary crates are added incrementally (`bismark-dedup/`, `bismark-bedgraph/`, `bismark-extractor/`, …). Phase 1 priorities are tracked on the project board.

## Binary naming during coexistence

Rust binaries take an `_rs` suffix through approximately v0.26 → v1.0 so they can be installed alongside the Perl Bismark scripts on the same PATH without conflicts:

| Perl                            | Rust binary (during coexistence) |
|---------------------------------|----------------------------------|
| `deduplicate_bismark`           | `deduplicate_bismark_rs`         |
| `bismark_methylation_extractor` | `bismark_methylation_extractor_rs` |
| `bismark2bedGraph`              | `bismark2bedGraph_rs`            |
| `coverage2cytosine`             | `coverage2cytosine_rs`           |

After v1.0 of the Rust port, the `_rs` suffix is dropped — the Rust binaries become the default `deduplicate_bismark` etc., and the Perl scripts move to a `legacy/` directory.

## Architecture decisions

- **BAM/SAM/CRAM I/O via pure-Rust `noodles`** — no `rust-htslib` (no htslib C build-time dep), no `samtools` subprocess (no external runtime dep).
- **One cargo workspace** with a binary crate per Bismark tool plus the shared `bismark-io` library. Library+binary split per crate so pure logic is unit-testable.
- **Byte-equal output to Perl Bismark v0.25.1** is a CI gate for the tools we have validated.
- Edition 2024; MSRV pinned in the workspace manifest.

## Building

```bash
cd rust
cargo build --release
```

The workspace currently contains:

- **`bismark-io`** (library) at `1.0.0-beta.2` — shared BAM/SAM/CRAM I/O on noodles, with v1.1's `ThreadedBamReader` / `ThreadedBamWriter` for parallel BGZF (de)compression. `1.0.0-beta.1` is published to crates.io; `1.0.0-beta.2` is queued for the next publish window.
- **`bismark-dedup`** (library + `deduplicate_bismark_rs` binary) at `1.1.0-beta.1` — Rust port of Perl `deduplicate_bismark`, byte-identical to v0.25.1 on real-data WGBS (10M PE + ~55M PE) with optional `--parallel N` BGZF threading. `1.0.0-beta.1` is published to crates.io; `1.1.0-beta.1` is queued for the next publish window.

Additional binary crates (`bismark-extractor`, `bismark-bedgraph`, `bismark-coverage2cytosine`, etc.) land as their Phase 1 sub-issues are implemented.

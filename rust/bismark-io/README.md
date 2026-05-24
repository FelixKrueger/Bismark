# `bismark-io`

Bismark-aware BAM/SAM/CRAM I/O on top of [`noodles`](https://github.com/zaeleus/noodles).

`bismark-io` is the shared library crate for [Bismark](https://github.com/FelixKrueger/Bismark)'s Rust rewrite. It wraps the `noodles` crate family to expose record types that already know about Bismark's strand classification (OT/CTOT/OB/CTOB derived from the `XR:Z:` and `XG:Z:` tags), tag-decoded accessors (`XM`, `XR`, `XG`, `MD`, `NM`), and CIGAR-aware position helpers. Every Bismark Rust binary crate (`bismark-dedup`, `bismark-extractor`, `bismark-bedgraph`, …) depends on it.

**Status:** v1.0.0-beta.2 — adds `ThreadedBamReader` / `ThreadedBamWriter` for parallel BGZF decode/encode (additive; existing API unchanged). Used by `bismark-dedup v1.1.0-beta.1`'s `--parallel N` flag. See [`CHANGELOG.md`](./CHANGELOG.md).

## Why a Bismark wrapper around `noodles`?

`noodles` is an excellent pure-Rust BAM/SAM/CRAM library, but it's deliberately generic — it doesn't know anything about Bismark-specific tags or strand classification. Without a wrapper, every downstream binary would have to re-derive strand from `XR`/`XG` on every call site. That's exactly where the Bismark-flavoured Rust rewrites that came before this one introduced bugs (silently routing per-record strand to per-pair output files, see [audit](https://github.com/FelixKrueger/Bismark/issues/794)).

`bismark-io` makes strand classification **structurally impossible to get wrong**:

- `BismarkStrand::from_xr_xg` decodes the four-way OT/CTOT/OB/CTOB enum from the raw tag bytes.
- `BismarkRecord` performs the classification **eagerly at parse time** and stores the result as a typed field. The per-record strand is computed once and never re-derived per call.
- `BismarkPair` exposes a separate `pair_strand()` method that is **always R1-derived**. Output routing for paired-end data uses `pair_strand()`, NOT each mate's `record_strand()` (they differ for R1 vs R2 of a directional pair).

## Design priorities

In order:

1. **Structural correctness over ergonomic shortcuts.** Strand is a property of a read (or pair), not of a position-on-a-read. Make the latter impossible to express.
2. **Zero external runtime deps.** No `samtools` subprocess, no `htslib` C-link, no `unsafe` blocks.
3. **Byte-equal output to Perl Bismark v0.25.1.** A CI invariant for downstream binaries.
4. **Testable without I/O.** Pure functions (CIGAR span, strand derivation, tag decoding) live behind interfaces that take byte slices, not `File`s.

Full design contract: [`DESIGN.md`](./DESIGN.md).

## Supported formats

| Format     | Read   | Write   | Notes                                                  |
|------------|--------|---------|--------------------------------------------------------|
| BAM        | ✅     | ✅      | Via `noodles-bam`                                      |
| SAM        | ✅     | ✅      | Via `noodles-sam`                                      |
| CRAM 3.0   | ✅     | ✅      | Via `noodles-cram`; requires reference FASTA           |

For CRAM, the reference is required for both read **and** write. A helper, `reconstitute_cram_reference_from_bismark_genome`, builds a multi-FASTA reference from a Bismark genome directory (matching Perl Bismark's behaviour at `bismark:5131`).

## Public API surface

```rust
use bismark_io::{
    // Records and strand
    BismarkRecord, BismarkPair, BismarkStrand, ReadIdentity,
    // Errors
    BismarkIoError,
    // CIGAR helpers (extension trait on noodles' Cigar)
    CigarExt, AlignedPosition, AlignedPositions,
    // I/O
    open_reader, AnyReader, BamReader, SamReader, CramReader, AlignmentKind,
    open_writer, AnyWriter, BamWriter, SamWriter, CramWriter,
    // CRAM reference helper
    reconstitute_cram_reference_from_bismark_genome,
};
```

Module-level docs explain each type's contract. See `cargo doc --open --package bismark-io`.

## Quick example

```rust
use bismark_io::{open_reader, BismarkStrand};
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let mut reader = open_reader(Path::new("aligned.bam"), None)?;
    let header = reader.header().clone();

    for result in reader.records(&header) {
        let record = result?;
        match record.record_strand() {
            BismarkStrand::OT   => { /* original top */ }
            BismarkStrand::CTOT => { /* complementary to OT */ }
            BismarkStrand::OB   => { /* original bottom */ }
            BismarkStrand::CTOB => { /* complementary to OB */ }
        }
    }
    Ok(())
}
```

For paired-end work, pair adjacent records with `BismarkPair::from_mates(r1, r2)?` and use `pair.pair_strand()` for output routing.

## Using as a library in other tools

`bismark-io` is designed to be the I/O foundation for Bismark-aware Rust tools — both the binary crates in this workspace (`bismark-dedup`, future `bismark-extractor` / `bismark-bedgraph` / etc.) and external consumers (pipeline frameworks, custom analysis scripts, methylation orchestrators).

Add to your `Cargo.toml`:

```toml
[dependencies]
bismark-io = "=1.0.0-beta.1"
```

End-to-end example — count records per strand:

```rust
use bismark_io::{open_reader, BismarkStrand};
use std::path::Path;

fn count_records_by_strand(bam_path: &Path) -> anyhow::Result<[u64; 4]> {
    let mut reader = open_reader(bam_path, None)?;
    let mut counts = [0u64; 4]; // OT, CTOT, OB, CTOB

    for result in reader.records() {
        let record = result?;
        let idx = match record.record_strand() {
            BismarkStrand::OT   => 0,
            BismarkStrand::CTOT => 1,
            BismarkStrand::OB   => 2,
            BismarkStrand::CTOB => 3,
        };
        counts[idx] += 1;
    }
    Ok(counts)
}
```

Pair-strand example (paired-end with R1-derived routing):

```rust
use bismark_io::{open_reader, BismarkPair};

fn pair_strands(bam_path: &std::path::Path) -> anyhow::Result<Vec<bismark_io::BismarkStrand>> {
    let mut reader = open_reader(bam_path, None)?;
    let mut records = reader.records();
    let mut pair_strands = Vec::new();
    loop {
        let Some(r1) = records.next().transpose()? else { break };
        let Some(r2) = records.next().transpose()? else {
            anyhow::bail!("PE input ended with unpaired R1");
        };
        let pair = BismarkPair::from_mates(r1, r2)?;  // validates qname + R1/R2 ordering
        pair_strands.push(pair.pair_strand());
    }
    Ok(pair_strands)
}
```

CIGAR-aware position helpers (the dedup key formula uses these):

```rust
use bismark_io::CigarExt;

// reference_end(start) = start + reference_span - 1
// (or `start` for empty CIGAR — see CigarExt docs for the edge case).
let end = record.cigar().reference_end(record.alignment_start().unwrap());
```

See `cargo doc --open --package bismark-io` for the full API surface, including `BismarkRecord`, `BismarkPair`, `CigarExt`, `tags::{xm, xr, xg, md, nm}`, and the `reconstitute_cram_reference_from_bismark_genome` helper.

## MSRV

Rust **1.89.0**. Required by `noodles-bam` 0.89.

## Dependencies

The `noodles` family is **exact-pinned** in `Cargo.toml`:

| Crate          | Version  | Why exact-pin                                              |
|----------------|----------|------------------------------------------------------------|
| noodles-bam    | =0.89.0  | Sets the MSRV; defines `Record` shape we wrap              |
| noodles-sam    | =0.85.0  | Mutually-resolvable with bam 0.89                          |
| noodles-cram   | =0.93.0  | Mutually-resolvable; CRAM 3.0 + DataSeries::QualityScores  |
| noodles-fasta  | =0.61.0  | Required by cram 0.93                                      |
| noodles-core   | =0.20.0  | Common types (`Position`)                                  |
| noodles-bgzf   | =0.47.0  | BGZF block boundaries                                      |
| noodles-csi    | =0.50.0  | CRAM container indexing                                    |

The pin policy: noodles releases frequently and occasionally bumps MSRV. Exact-pinning lets us upgrade as a deliberate workspace-wide decision rather than getting silently dragged along.

## Testing

- **108 tests total**: 96 lib unit tests + 5 integration tests + 6 proptest properties + 1 doctest.
- `cargo test --workspace` runs everything.
- `cargo clippy --all-targets -- -D warnings` is clean.
- `cargo fmt --check` is clean.
- The test fixture `test_files/tiny_pe_bismark.bam` is Bismark Perl v0.25.1-generated; see [`test_files/README.md`](./test_files/README.md) for provenance.

## Stability

This is **v1.0.0-beta.1** — the public API is stable; no breaking changes are planned between `1.0.0-beta.N` and `1.0.0`. The `DESIGN.md` document is the canonical contract; if a future change to `bismark-io` contradicts `DESIGN.md`, the design doc gets updated in lockstep.

## crates.io

This release is **not yet published to crates.io**. Within the Bismark workspace, path-dep usage is the supported integration model. Publication is deferred until at least one downstream binary crate lands, to keep the publish-bump cycle in lockstep with binary crates.

## License

GPL-3.0-only. Matches the upstream Perl Bismark license.

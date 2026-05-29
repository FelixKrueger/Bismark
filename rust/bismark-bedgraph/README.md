# `bismark-bedgraph`

Rust port of [Bismark](https://github.com/FelixKrueger/Bismark)'s `bismark2bedGraph` script — turns the methylation extractor's per-context call files into a sorted, gzip-compressed **bedGraph** and **coverage** file.

**Status:** v1.0.0-beta.1 — decompressed-content byte-identical to Bismark
Perl `bismark2bedGraph` v0.25.1 across the SE+PE × default+`--CX` matrix, and
~3.4× faster than Perl thanks to parallel gzip. See [`CHANGELOG.md`](./CHANGELOG.md)
and [`SPEC.md`](./SPEC.md).

## What it does

Given the Bismark methylation extractor's per-context call files
(`CpG_OT_*`, `CpG_OB_*`, … and the CHG/CHH equivalents under `--CX`), it
aggregates the per-cytosine methylation calls by genomic position and writes:

- a **bedGraph** (`<output>.gz`) — `chromosome  start(0-based)  end(1-based)  %methylation`, with a `track type=bedGraph` header;
- a **coverage** file (`<output>.bismark.cov.gz`) — `chromosome  start(1-based)  end(1-based)  %methylation  count_methylated  count_unmethylated` (the input to `coverage2cytosine`).

This matches Bismark Perl `bismark2bedGraph` v0.25.1 exactly (decompressed
content). The methylation percentage uses Perl's `%.15g` number formatting,
and chromosomes are emitted in Perl's exact order (see *Byte-identity*).

## Installation

```sh
# Within the Bismark workspace (path dependency):
cd rust/
cargo install --path bismark-bedgraph
```

During the Perl → Rust coexistence period the binary installs as
**`bismark2bedGraph_rs`** (with the `_rs` suffix) so it sits alongside the
Perl `bismark2bedGraph` on `$PATH` without conflict.

## Usage

```sh
# Default (CpG context only) — only files whose basename starts with `CpG`:
bismark2bedGraph_rs -o sample.bedGraph CpG_OT_sample.txt CpG_OB_sample.txt

# All cytosine contexts (CpG + CHG + CHH):
bismark2bedGraph_rs --CX -o sample.CX.bedGraph CpG_*.txt CHG_*.txt CHH_*.txt

# Coverage cutoff (report only positions with ≥ 5 reads):
bismark2bedGraph_rs --cutoff 5 -o sample.bedGraph CpG_OT_sample.txt CpG_OB_sample.txt

# Write into a specific directory (created if missing):
bismark2bedGraph_rs --dir results/ -o sample.bedGraph CpG_OT_sample.txt CpG_OB_sample.txt

# Also emit a 0-based coverage file and a UCSC-compatible bedGraph:
bismark2bedGraph_rs --zero_based --ucsc -o sample.bedGraph CpG_OT_sample.txt CpG_OB_sample.txt

# Gzipped inputs are read transparently:
bismark2bedGraph_rs -o sample.bedGraph CpG_OT_sample.txt.gz CpG_OB_sample.txt.gz
```

### Flag reference

| Flag | Purpose |
|------|---------|
| `<files>...` | Methylation-extractor call file(s), `.txt` or `.txt.gz` (positional). Default mode uses only files whose basename starts with `CpG`. |
| `-o`, `--output <NAME>` | Output bedGraph filename (mandatory). No path separators — use `--dir`. `.gz` is appended if absent. |
| `--dir <DIR>` | Output directory (created if missing; defaults to the CWD). |
| `--cutoff <N>` | Minimum read coverage before a position is reported (default: 1). |
| `--CX`, `--CX_context` | Process all cytosine contexts (CpG, CHG, CHH), not just CpG. |
| `--zero_based` | Also write a 0-based, half-open coverage file (plain text, `.bismark.zero.cov`). |
| `--ucsc` | Also write a UCSC-compatible bedGraph (`chr` prefix, `MT`→`chrM`). |
| `--no_header` | Inputs have no version-header line — do not drop the first line of each file. |
| `--remove_spaces` | Accepted for Perl compatibility; no effect on output (the read-id field is unused). |
| `--counts` | Accepted for Perl compatibility; coverage counts are always emitted. |
| `--buffer_size <SIZE>` | Accepted for Perl compatibility; an in-memory sort is always used. |
| `--gazillion`, `--scaffolds` | Accepted for Perl compatibility; unnecessary (no open-filehandle limit). |
| `--ample_memory` | Accepted for Perl compatibility; an in-memory sort is always used. |
| `--version` | Print provenance string and exit. |
| `--man`, `-h`, `--help` | Help text. |

`--buffer_size` / `--ample_memory` / `--gazillion` are *accepted-but-ignored*:
this port aggregates in memory rather than shelling out to UNIX `sort`, so the
sort-tuning flags are unnecessary. They are still validated for CLI parity
(e.g. `--ample_memory` + `--buffer_size` is rejected, as in Perl).

## Output

For `-o sample.bedGraph`:

| File | Contents |
|------|----------|
| `sample.bedGraph.gz` | bedGraph (gzip): `chr  start(0-based)  end  %meth`, `track type=bedGraph` header |
| `sample.bismark.cov.gz` | Coverage (gzip): `chr  start  end  %meth  count_meth  count_unmeth` (1-based) |
| `sample.bedGraph.gz.bismark.zero.cov` | *(with `--zero_based`)* 0-based coverage, plain text |
| `sample.bedGraph_UCSC.bedGraph.gz` | *(with `--ucsc`)* UCSC-compatible bedGraph |

The slightly odd `.zero.cov` filename reproduces a Perl filename quirk exactly
— byte-identity over "tidiness".

## Byte-identity invariant

The contract is **decompressed-content** byte-identity to Perl v0.25.1: the
raw `.gz` bytes need not match (any DEFLATE implementation differs), but
`gunzip`-decompressed content is identical. Verified on real 10M-read GRCh38
data against Perl `bismark2bedGraph` v0.25.1:

| Dataset | Mode | Result |
|---------|------|--------|
| 10M SE (directional) | default (CpG) | byte-identical |
| 10M SE | `--CX` (all contexts) | byte-identical |
| 10M PE (deduplicated) | default (CpG) | byte-identical |
| 10M PE | `--CX` (all contexts) | byte-identical |

A hermetic CI test (`tests/byte_identity_fixtures.rs`) checks decompressed
output against **Perl-generated** expected files for default / `--cutoff` /
`--CX` / `--zero_based` / `--ucsc` cells. A real-data gate
(`tests/byte_identity_real_data.rs`, `#[ignore]`) and a harness
(`scripts/bedgraph_byte_identity.sh`) run the live Perl-vs-Rust comparison:

```sh
BISMARK_BEDGRAPH_REAL_DATA_DIR=/path/to/CpG_files \
  PERL_BG=/path/to/bismark2bedGraph \
  cargo test -p bismark-bedgraph --release -- --ignored byte_identity_real_data
```

## Performance

The two large output streams are written with [`gzp`](https://crates.io/crates/gzp)
**parallel block-gzip**. A flamegraph showed ~70% of runtime was serial
DEFLATE; Perl is fast only because it offloads compression to a parallel
`gzip` subprocess. Parallelising compression in-process (the `gzp` worker
threads compress while the main thread generates) makes the port **~3.4×
faster than Perl** on 10M PE default (8 s vs 27 s), versus ~2× *slower* before
the change.

Because the byte-identity contract is on *decompressed* content, the
compression backend is free: under Cargo feature unification with the crate's
`flate2` `zlib-rs` feature, gzp compresses using zlib-rs.

## How is this different from Bismark Perl's `bismark2bedGraph`?

| | Perl `bismark2bedGraph` | `bismark2bedGraph_rs` |
|---|----|----|
| Sort | shells out to UNIX `sort` per chromosome | in-memory aggregation by `(chr, pos)` |
| gzip | `gzip -c` subprocess (1 process per stream) | in-process `gzp` parallel block-gzip |
| Runtime deps | `gzip`, `gunzip`, `sort` on `$PATH` | none — pure Rust |
| `--gazillion` / `--buffer_size` / `--ample_memory` | switch sort strategy | accepted-but-ignored (in-memory always) |
| Chromosome order | `sort` of per-chr temp filenames | same order, reproduced from the input argv order |

`--gazillion` (Perl's `sort -V` scaffold mode) is an accepted no-op:
byte-identity is guaranteed for the **default** chromosome ordering only.

## Using as a library

`bismark-bedgraph` is both a binary and a library:

```rust
use bismark_bedgraph::cli::Cli;
use clap::Parser;

fn main() -> Result<(), bismark_bedgraph::BismarkBedgraphError> {
    let config = Cli::parse().validate()?;   // parse + validate CLI → ResolvedConfig
    bismark_bedgraph::run(&config)            // read → aggregate → write outputs
}
```

See `cargo doc --open --package bismark-bedgraph` for the full API
(`input`, `aggregate::Aggregator`, `fmt_g::format_g15`, `output`, `ucsc`).

## Building from source

Requires Rust 1.89 or later (set in the workspace `Cargo.toml`).

```sh
cd rust/
cargo build --release --package bismark-bedgraph
./target/release/bismark2bedGraph_rs --version
```

## Workspace dependencies

This crate has **no `bismark-io` / noodles dependency** — it reads plain/gzipped
text, not BAM. Direct deps:

- [`clap`](https://crates.io/crates/clap) `=4.5.30` — CLI parsing
- [`flate2`](https://crates.io/crates/flate2) `=1.1.9` (`zlib-rs` backend) — gzip read + single-stream writes
- [`gzp`](https://crates.io/crates/gzp) `=0.11.3` (`deflate_rust`) — parallel block-gzip for the bedGraph + coverage streams
- [`rustc-hash`](https://crates.io/crates/rustc-hash) `=2.1.0` — `FxHashMap` for the `(chr, pos)` aggregation
- [`thiserror`](https://crates.io/crates/thiserror) `=2.0.0` — typed errors

All deps exact-pinned, matching the workspace convention.

## License

GPL-3.0-only. Matches the upstream Perl Bismark license.

## See also

- [Bismark project](https://github.com/FelixKrueger/Bismark)
- [`bismark-dedup`](../bismark-dedup/README.md), [`bismark-extractor`](../bismark-extractor/README.md) — sibling Rust ports
- [`SPEC.md`](./SPEC.md) — the binding byte-identity contract

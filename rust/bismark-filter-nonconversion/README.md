# bismark-filter-nonconversion

A Rust port of Bismark's Perl `filter_non_conversion` script — part of the
[Bismark](https://github.com/FelixKrueger/Bismark) Rust rewrite. Installs as
the binary **`filter_non_conversion_rs`** during the coexistence period.

## What it does

Reads a Bismark BAM, walks each read's `XM:Z:` methylation-call string, and
removes reads (single-end) or read-pairs (paired-end) that show too much
**non-CpG** methylation — a hallmark of *incomplete bisulfite conversion*. It
is a verbatim pass-through: records are written **unchanged**; only their
routing (kept vs removed) is decided. It does not read the genome and uses
only the `XM` tag.

> ⚠️ This kind of filtering is **not advisable** for organisms with appreciable
> non-CpG methylation (e.g. most plants) — it will introduce bias.

Per input file it writes three outputs, **next to the input**:

| Output | Contents |
|--------|----------|
| `<name>.nonCG_filtered.bam` | reads/pairs that passed (kept) |
| `<name>.nonCG_removed_seqs.bam` | reads/pairs removed as likely non-converted |
| `<name>.non-conversion_filtering.txt` | the filtering report |

(`<name>` is the input with a single trailing `.bam` removed; the directory is
preserved.)

## Install

```sh
cargo build --release -p bismark-filter-nonconversion
# binary at target/release/filter_non_conversion_rs
```

## Usage

```sh
# Single-end, default: remove a read with >= 3 methylated non-CG calls
filter_non_conversion_rs -s sample_bismark_bt2.bam

# Paired-end (either mate failing removes the whole pair)
filter_non_conversion_rs -p sample_bismark_bt2_pe.bam

# Auto-detect SE/PE from the @PG line (no -s/-p)
filter_non_conversion_rs sample_bismark_bt2.bam
```

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `-s`, `--single` | auto | Force single-end. |
| `-p`, `--paired` | auto | Force paired-end. |
| `--threshold <N>` | 3 | Remove at `>= N` methylated non-CG calls. |
| `--consecutive` | off | Count *consecutive* methylated non-CG calls; any unmethylated cytosine (`z`/`h`/`x`) resets the run. Mutually exclusive with `--percentage_cutoff`. |
| `--percentage_cutoff <P>` | — | Remove on an overall non-CG methylation percentage (0–100) instead of an absolute count. |
| `--minimum_count <M>` | 5 | Minimum non-CG calls before `--percentage_cutoff` applies. |
| `--samtools_path <PATH>` | — | Accepted for Perl compatibility, **ignored** (this port is pure-Rust). |
| `--version`, `--help` | — | Version / help. |

If neither `-s` nor `-p` is given, the library type is auto-detected from the
Bismark `@PG` header line.

## Correctness

`filter_non_conversion_rs` is validated **byte-identical** to Perl Bismark
v0.25.1: the decompressed alignment-record bodies of both output BAMs (same
records kept/removed, same order, same per-read tags) and the report text
match exactly. (`samtools view` is used to compare decoded records; raw BGZF
bytes are never diffed, and the `@PG` header line — which `samtools` appends
per invocation — is excluded.) The report's non-deterministic run-time line is
the only normalized field. See `tests/` and
`plans/05312026_bismark-filter-nonconversion/`.

## Notes / deviations from the Perl

- **BAM input only** (matches the Perl `=~ /bam$/` gate).
- **Pure Rust I/O** via [noodles](https://github.com/zaeleus/noodles) — no
  `samtools` subprocess. `--samtools_path` is accepted and ignored.
- Single-threaded; uses the `mimalloc` allocator. There is no `--parallel`.
- `--help` exits 0 (Perl's exits 1); `--version` exits 0.

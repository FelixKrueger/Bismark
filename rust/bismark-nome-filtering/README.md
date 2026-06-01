# bismark-nome-filtering

A Rust port of Bismark's **standalone** Perl `NOMe_filtering` (v0.25.1) — a
per-read NOMe-Seq classifier. Binary: **`NOMe_filtering_rs`**.

> ⚠️ This is the **standalone** `NOMe_filtering` tool, **not** `coverage2cytosine
> --nome-seq` (a separate in-c2c flag).

**Byte-identical to Perl v0.25.1** (synthetic goldens + a full 10M SE real-data
gate on hg38 — see `CHANGELOG.md`), and ~3.4× faster single-threaded.

## What it does

Reads the methylation extractor's `--yacht` output (one line per cytosine call,
with the read's start/end/orientation), groups calls by read, extracts each
read's genome sequence ±2 bp, and tallies NOMe-filtered methylation:

- **CpG** counted only in **A-CG / T-CG** context (filtering out G-CG / C-CG bias).
- **GpC** (non-CpG) counted only when the C is preceded by **G** (the NOMe
  accessibility signal), and not in CpG context.

It writes one always-gzipped per-read line to `<input-stem>.manOwar.txt.gz`:

```
ReadID  Chr  Start  End  meth_CG  unmeth_CG  meth_GC  unmeth_GC
```

## Usage

```
NOMe_filtering_rs -g <genome_folder> <yacht_input[.gz]>
```

- `-g`/`--genome_folder` — directory of genome FASTA (mandatory). Accepts `.fa`
  then `.fasta` (plain only — **not** `.fa.gz`; matches Perl).
- `--dir <path>` — output directory (input and output are resolved relative to
  it, matching how the extractor invokes the tool).
- Input must come from `bismark_methylation_extractor -s --yacht` (single-end).

Flags accepted for Perl compatibility but inert: `--zero_based`, `--CX`, `--GC`,
`--gzip` (output is always gzipped), `--nome-seq`, `--merge_CpGs` (the last only
errors when combined with `--CX`).

## Build / test

```
cargo build --release -p bismark-nome-filtering
cargo test  -p bismark-nome-filtering
```

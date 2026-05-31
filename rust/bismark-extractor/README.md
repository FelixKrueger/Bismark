# `bismark-extractor`

Rust port of [Bismark](https://github.com/FelixKrueger/Bismark)'s `bismark_methylation_extractor` script — extracts methylation calls from Bismark-aligned BAM/SAM/CRAM files. The biggest single-tool rewrite in the Bismark Rust workspace: Perl source is **6,050 LOC** across **35 CLI flags**.

**Status:** v1.0.0-beta.1 — **fully implemented (Phases A–H).** Byte-identical to Perl Bismark v0.25.1 at full scale — validated on WGBS PE (64.6M read pairs), WGBS SE (63.6M reads), and mouse RRBS (30.6M read pairs): every output stream parity-confirmed and worker-count-invariant — and **~4.8× faster than Perl `--multicore 12`** on fewer cores. Production-capable; a tagged `v1.0` release follows final integration. See [issue #798](https://github.com/FelixKrueger/Bismark/issues/798) for status.

## What it does

Walk a Bismark-aligned BAM, classify each XM-tag byte (`Z`/`z`/`X`/`x`/`H`/`h`/…) per the per-record strand classification (OT/CTOT/CTOB/OB), route methylation calls to per-context split files, accumulate per-(context × read-identity) M-bias counters, and emit a `_splitting_report.txt`. Optionally chain into `bismark2bedGraph` + `coverage2cytosine` for downstream bedGraph + cytosine-report outputs.

**Byte-identity to Perl Bismark v0.25.1** is a hard invariant for every output stream (12 strand-specific split files, `M-bias.txt`, `_splitting_report.txt`, optional bedGraph + cytosine-report chain).

## Phased implementation

Per [`SPEC.md`](./SPEC.md) §10 (rev 2, locked):

| Phase | Scope | LOC | Status |
|-------|-------|-----|--------|
| **A** | Workspace scaffold + CLI + argument structs + validation | ~600 | ✅ shipped |
| **B** | Core SE extraction loop + XM routing + output-file map | ~800 | ✅ shipped |
| **C** | PE extraction + overlap handling (`--no_overlap`) | ~600 | ✅ shipped |
| **D** | M-bias accumulation per (context × read_identity) + writer | ~500 | ✅ shipped |
| **E** | Output mode dispatch (`--comprehensive`/`--merge_non_CpG`/`--yacht`) + `--gzip` | ~400 | ✅ shipped |
| **F** | `--parallel N` worker pipeline (byte-identical invariant) | ~700 | ✅ shipped |
| **G** | `--bedGraph` + `--cytosine_report` subprocess chain | ~400 | ✅ shipped |
| **H** | Real-data byte-identity gate (full-scale WGBS PE/SE + RRBS) + CHANGELOG + version tag | ~200 test | ✅ shipped |

Each phase is its own PR, dual-code-reviewed, and squash-merged to the integration branch (`rust/iron-chancellor`).

## CLI surface (35 flags — names match Perl exactly)

```text
$ bismark-methylation-extractor-rs --help
Extract methylation calls from Bismark-aligned BAM/SAM/CRAM files

Usage: bismark-methylation-extractor-rs [OPTIONS] [FILES]...

(See `--help` for the full 35-flag list — names match Perl exactly.)
```

All flags map 1:1 to Perl `bismark_methylation_extractor` (line citations in [`SPEC.md` §3](./SPEC.md)):

| Group | Flags |
|-------|-------|
| Library mode | `-s/--single-end`, `-p/--paired-end` |
| Read-region trim | `--ignore`, `--ignore_r2`, `--ignore_3prime`, `--ignore_3prime_r2` |
| Output mode | `--comprehensive`, `--merge_non_CpG`, `--yacht`, `--mbias_only`, `--mbias_off` |
| Output control | `-o/--output_dir`, `--gzip`, `--no_header`, `--report`, `--fasta` |
| PE overlap | `--no_overlap`, `--include_overlap` |
| BedGraph chain | `--bedGraph`, `--cutoff`, `--remove_spaces`, `--counts`, `--zero_based`, `--ucsc`, `--buffer_size`, `--gazillion`/`--scaffolds`, `--ample_memory` |
| Cytosine-report chain | `--cytosine_report`, `-g/--genome_folder`, `--CX`/`--CX_context`, `--split_by_chromosome` |
| Compat | `--samtools_path` (silently accepted, no-op — bismark-io is pure-Rust noodles) |
| Parallelism | `--parallel`/`--multicore` |
| Meta | `-V/--version`, `-h/--help` |

`Cli::validate()` rejects every documented mutex pair + precondition from SPEC §11 + Perl source (e.g. `--mbias_only` × `--bedGraph`, `--gazillion` × `--ample_memory`, `--yacht` × `--paired-end`, `--cytosine_report` without `--genome_folder`).

## Resource usage (HPC & nf-core)

The extractor's speed is **architectural, not a tuning knob.** Unlike Perl's `--multicore` fork model, you do **not** scale `--parallel` to go faster — and `--parallel 1` is **not** single-threaded: BGZF decode (2 threads) and gzip compression (a ~60-thread pool) run in parallel automatically, so the default already uses **~7–8 CPU cores** in gzip mode (by design, not a runaway process). `--parallel` only adds extraction workers on top, and raising it does **not** improve throughput on BAM input — the pipeline is decode-bound, so wall time is flat from `--parallel 1` to `16`.

Request a fixed allocation **by output mode**, not by `--parallel`:

| Mode | cpus | memory | notes |
|------|------|--------|-------|
| gzip (default) | ~8 | ~2 GB | ~80 threads peak → ensure `ulimit -u` / `nproc` headroom |
| `--mbias_only` | ~3 | ~0.1 GB | no per-context output files written |
| plain `.txt` | ~1 | ~1.5 GB | write-I/O-bound — uses <1 core; output is large + uncompressed |

**Tip:** keep gzip output enabled — it is *faster* than plain `.txt`, not slower, because it slashes the volume written to disk.

At full scale (human WGBS, 64.6M read pairs, gzip) the Rust extractor is byte-identical to Perl Bismark v0.25.1 and **~4.8× faster than Perl `--multicore 12`, using ~7 cores vs ~19** (Perl `--multicore 1`: ~76 min → Rust default: ~99 s). Validated byte-identical and worker-count-invariant on WGBS PE/SE and mouse RRBS.

## Installation

```sh
# Within the Bismark workspace (path dependency):
cd rust/
cargo install --path bismark-extractor
```

The binary installs as `bismark-methylation-extractor-rs` (with `_rs` suffix during Perl coexistence; matches `deduplicate_bismark_rs`).

Byte-identical to Perl `bismark_methylation_extractor` v0.25.1 — validated for production extraction at full scale (see Status). A formal `v1.0` release tag follows final integration into the main Bismark distribution.

## Structural design choices

Locked in [SPEC.md §6](./SPEC.md) — each choice structurally prevents a class of bug observed in the prior-art Rust port:

1. **`BismarkStrand` derived once per pair** (via [`bismark-io::BismarkPair::pair_strand()`](https://crates.io/crates/bismark-io)) — closes Alan Hoyle's port's "strand-routing splits one read across multiple files" bug structurally.
2. **M-bias counters as `[MbiasTable; 2]`** indexed by `ReadIdentity` — closes the missing-CHG/CHH M-bias context bug; every iteration site must explicitly handle all three contexts.
3. **Argument structs** (`ExtractParams`, `PairParams`) instead of 14-parameter functions — replaces the wide-signature smell from Alan's port.
4. **`std::thread`-based `--multicore N`** with always-on 2-thread parallel BGZF decode + bounded MPMC channel + per-worker scratch + input-order output collector — replaces Perl's wasteful fork+modulo model. Byte-identity invariant: output is identical at any N ≥ 1.
5. **CIGAR + XM orientation correction** lives in [`bismark-io::BismarkRecord::iter_aligned()`](https://crates.io/crates/bismark-io/1.0.0-beta.6) (v1.0.0-beta.6+) — the extractor consumes orientation-corrected `(read_pos_5p, ref_pos, xm_byte)` triples without needing to know about `-`-strand reverse-complement.
6. **`--bedGraph` / `--cytosine_report` subprocess** to Perl `bismark2bedGraph` / `coverage2cytosine` (Phase G). Inline-Rust migration is a v1.x concern.

## Workspace context

- Shared library: [`bismark-io`](../bismark-io/) v1.0.0-beta.6 (noodles-based BAM/SAM/CRAM I/O, Bismark-aware record types, `iter_aligned()` adapter).
- Sibling binary: [`bismark-dedup`](../bismark-dedup/) v1.2.1-beta.1 (the production-ready Rust port of `deduplicate_bismark`; byte-identical to Perl v0.25.1).
- Long-running integration branch: [`rust/iron-chancellor`](https://github.com/FelixKrueger/Bismark/tree/rust/iron-chancellor).
- Project board: [Bismark Rust rewrite](https://github.com/users/FelixKrueger/projects/1).

## Tests

```sh
cd rust/bismark-extractor
cargo test
```

105 lib unit tests (CLI parse/validate, derived-config resolution, and the parallel worker pipeline) plus integration tests asserting byte-identity (legacy vs parallel at N=1 and N≥2), M-bias accumulation, and gzip round-trips. `help_text_lists_all_35_flags` remains a structural guard against silent flag drops.

## References

- **Perl source**: [`bismark_methylation_extractor`](https://github.com/FelixKrueger/Bismark/blob/master/bismark_methylation_extractor) (v0.25.1, 6,050 LOC).
- **SPEC**: [`SPEC.md`](./SPEC.md) (rev 2, dual plan-reviewed, locked).
- **Epic**: [#798](https://github.com/FelixKrueger/Bismark/issues/798).
- **Phase A sub-issue**: [#846](https://github.com/FelixKrueger/Bismark/issues/846).

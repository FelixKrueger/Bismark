# `bismark-extractor`

Rust port of [Bismark](https://github.com/FelixKrueger/Bismark)'s `bismark_methylation_extractor` script — extracts methylation calls from Bismark-aligned BAM/SAM/CRAM files. The biggest single-tool rewrite in the Bismark Rust workspace: Perl source is **6,050 LOC** across **35 CLI flags**.

**Status:** v1.0.0-alpha.1 — **Phase A: scaffold only.** The binary boots, `--help` prints all 35 flags, and `--version` emits a TG-style provenance string. **Extraction logic is NOT yet implemented.** For production use, run Perl `bismark_methylation_extractor` until Phase H (the byte-identity gate) lands. See [issue #798](https://github.com/FelixKrueger/Bismark/issues/798) for status.

## What it will do (when Phases B–H land)

Walk a Bismark-aligned BAM, classify each XM-tag byte (`Z`/`z`/`X`/`x`/`H`/`h`/…) per the per-record strand classification (OT/CTOT/CTOB/OB), route methylation calls to per-context split files, accumulate per-(context × read-identity) M-bias counters, and emit a `_splitting_report.txt`. Optionally chain into `bismark2bedGraph` + `coverage2cytosine` for downstream bedGraph + cytosine-report outputs.

**Byte-identity to Perl Bismark v0.25.1** is a hard invariant for every output stream (12 strand-specific split files, `M-bias.txt`, `_splitting_report.txt`, optional bedGraph + cytosine-report chain).

## Phased implementation

Per [`SPEC.md`](./SPEC.md) §10 (rev 2, locked):

| Phase | Scope | LOC | Status |
|-------|-------|-----|--------|
| **A** | Workspace scaffold + CLI + argument structs + validation | ~600 | ✅ shipped (this release) |
| **B** | Core SE extraction loop + XM routing + output-file map | ~800 | pending |
| **C** | PE extraction + overlap handling (`--no_overlap`) | ~600 | pending |
| **D** | M-bias accumulation per (context × read_identity) + writer | ~500 | pending |
| **E** | Output mode dispatch (`--comprehensive`/`--merge_non_CpG`/`--yacht`) + `--gzip` | ~400 | pending |
| **F** | Rayon-based `--multicore N` (byte-identical invariant) | ~700 | pending |
| **G** | `--bedGraph` + `--cytosine_report` subprocess chain | ~400 | pending |
| **H** | Real-data byte-identity gate (10M PE WGBS + 55M full) + CHANGELOG + version tag | ~200 test | pending |

Each phase is its own PR, dual-code-reviewed, and squash-merged to the integration branch (`rust/iron-chancellor`).

## CLI surface (Phase A — all 35 flags parse + validate)

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

## Installation (Phase A)

```sh
# Within the Bismark workspace (path dependency):
cd rust/
cargo install --path bismark-extractor
```

The binary installs as `bismark-methylation-extractor-rs` (with `_rs` suffix during Perl coexistence; matches `deduplicate_bismark_rs`).

**Do not use for production extraction** — Phase A only validates flags. Real extraction lands in Phase B+.

## Structural design choices

Locked in [SPEC.md §6](./SPEC.md) — each choice structurally prevents a class of bug observed in the prior-art Rust port:

1. **`BismarkStrand` derived once per pair** (via [`bismark-io::BismarkPair::pair_strand()`](https://crates.io/crates/bismark-io)) — closes Alan Hoyle's port's "strand-routing splits one read across multiple files" bug structurally.
2. **M-bias counters as `[MbiasTable; 2]`** indexed by `ReadIdentity` — closes the missing-CHG/CHH M-bias context bug; every iteration site must explicitly handle all three contexts.
3. **Argument structs** (`ExtractParams`, `PairParams`) instead of 14-parameter functions — replaces the wide-signature smell from Alan's port.
4. **Rayon-based `--multicore N`** with single BGZF decompression + bounded MPMC channel + per-worker scratch + input-order output collector — replaces Perl's wasteful fork+modulo model. Byte-identity invariant: output is identical at any N ≥ 1.
5. **CIGAR + XM orientation correction** lives in [`bismark-io::BismarkRecord::iter_aligned()`](https://crates.io/crates/bismark-io/1.0.0-beta.6) (v1.0.0-beta.6+) — the extractor consumes orientation-corrected `(read_pos_5p, ref_pos, xm_byte)` triples without needing to know about `-`-strand reverse-complement.
6. **`--bedGraph` / `--cytosine_report` subprocess** to Perl `bismark2bedGraph` / `coverage2cytosine` (Phase G). Inline-Rust migration is a v1.x concern.

## Workspace context

- Shared library: [`bismark-io`](../bismark-io/) v1.0.0-beta.6 (noodles-based BAM/SAM/CRAM I/O, Bismark-aware record types, `iter_aligned()` adapter).
- Sibling binary: [`bismark-dedup`](../bismark-dedup/) v1.2.1-beta.1 (the production-ready Rust port of `deduplicate_bismark`; byte-identical to Perl v0.25.1).
- Long-running integration branch: [`rust/iron-chancellor`](https://github.com/FelixKrueger/Bismark/tree/rust/iron-chancellor).
- Project board: [Bismark Rust rewrite](https://github.com/users/FelixKrueger/projects/1).

## Tests (Phase A)

```sh
cd rust/bismark-extractor
cargo test
```

40 tests — 35 lib unit tests covering CLI parse + validate + derived-config resolution; 5 sanity tests including `help_text_lists_all_35_flags` (structural guard against silent flag drops in future refactors).

## References

- **Perl source**: [`bismark_methylation_extractor`](https://github.com/FelixKrueger/Bismark/blob/master/bismark_methylation_extractor) (v0.25.1, 6,050 LOC).
- **SPEC**: [`SPEC.md`](./SPEC.md) (rev 2, dual plan-reviewed, locked).
- **Epic**: [#798](https://github.com/FelixKrueger/Bismark/issues/798).
- **Phase A sub-issue**: [#846](https://github.com/FelixKrueger/Bismark/issues/846).

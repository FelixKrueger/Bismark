# EPIC — `bismark-coverage2cytosine` (Rust port of Perl `coverage2cytosine`)

**Status:** rev 0 (phases confirmed by Felix 2026-05-29). Awaiting per-phase plans + dual plan-review before implementation.

**Design contract:** [`SPEC.md`](./SPEC.md) (rev 1) — read it first. This epic is the lean coordination layer; the SPEC holds the byte-identity algorithm detail.

**Branch / worktree:** `rust/coverage2cytosine` in the isolated worktree `/Users/fkrueger/Github/Bismark-c2c` (off `origin/rust/iron-chancellor` @ 8a2a147). New crate `bismark-coverage2cytosine` in the `rust/` workspace.

---

## 1. Goal

Port Perl `coverage2cytosine` (v0.25.1, 2,321 LOC — the genome-wide cytosine-report producer) to a Rust `lib`+`bin` crate that is **byte-identical to Perl v0.25.1** across the v1.0 output streams, and is callable as a library so `bismark-extractor` can later invoke it **inline** (closing Phase H **sub-gate 2** of the extractor's byte-identity gate — see [[project_phase_h_byte_identity_ordering]]). It is the downstream consumer of `bismark-bedgraph`'s (#797) `.bismark.cov.gz`.

## 2. Scope

**In (v1.0):** genome-wide CpG / `--CX` report; always-on `*.cytosine_context_summary.txt`; `--zero_based`; `--split_by_chromosome`; `--coverage_threshold`; `--gzip`; `--genome_folder` (mandatory, no mouse default); `-o`/`--dir`/`--parent_dir` naming; `--merge_CpGs` (+ `--discordance_filter`); a library API for the future inline switch.

**Out (deferred to v1.x, separate phases/epic):** `--gc`/`--gc_context`, `--nome-seq`, `--drach`/`--m6A`, `--ffs` — **rejected at the CLI** in v1.0, never silently ignored. A parallel-genome-walk perf pass is a v1.x candidate.

**Never gated:** STDERR byte-identity (informational chatter; dedup/extractor precedent).

Full flag inventory + validation rules in SPEC §3; output topology in SPEC §5.

## 3. Phase breakdown (execution order)

Strictly sequential A→B→C→D→E (each merges to `rust/coverage2cytosine`; the branch merges to `rust/iron-chancellor` at the end). Each phase is independently byte-identity-testable against Perl, so the riskiest pieces are isolated in their own dual-review cycle.

- **Phase A — Scaffold + CLI + genome reader.** Workspace member crate (`lib.rs` + `bin coverage2cytosine_rs`); clap-derive `Cli` → `ResolvedConfig::validate()` enforcing every SPEC §3 mutex/range rule **and rejecting the v1.x flags**; `genome.rs` whole-genome reader on noodles-fasta with the Perl quirks (uppercase, `Mus_musculus.NCBIM37.fa` skip, four-suffix glob priority, insertion-ordered map, dup-name error, `u32` overflow guard); `BismarkC2cError` enum. Acceptance: crate boots, `--help`/`--version` print, genome loads. (SPEC §6, §10.1–10.3, §10.6.)

- **Phase B — Core genome-wide report (the crux).** Cov-file parse (gz-aware) + per-chromosome buffering; the genome C/G walk with exact `pos = i+1` coordinate arithmetic + forward-C / reverse-G `tri_nt`/`upstream` extraction + `tr/ACTG/TGAC/` complement; per-position guards; context classification (CG/CHG/CHH); CpG-only vs `--CX` emission; `--zero_based`; `--coverage_threshold`; covered (cov-appearance order) + uncovered (sorted) chromosome ordering; the always-on `*.cytosine_context_summary.txt`. Plain (uncompressed) output. (SPEC §4, §7, §8.)

- **Phase C — `--gzip` + `--split_by_chromosome`.** `BufWriter<GzEncoder>` wrapping (context summary stays uncompressed); per-chromosome writer open/close + `.chr<NAME>` filename-infix derivation. (SPEC §5, §10.5.)

- **Phase D — `--merge_CpGs` (+ `--discordance_filter`).** The post-pass that re-reads the CpG report and pools `+`/`-` strand pairs: chromosome-start resync (historical bugs #98/#229), sanity asserts (typed errors), discordance routing with the both-strands-measured gate, `%.6f` percentages, zero-based half-open coords. (SPEC §9.)

- **Phase E — Real-data byte-identity gate (colossal).** Driver script + flag matrix on colossal ([[reference_colossal_access]]) against a **Perl-`bismark2bedGraph`-generated** `.cov.gz` (keeps the two c2c producers independent); raw-byte compare of reports + summary + merged/discordant cov (gzip compared post-decompression); distinct out-dir from other sessions; RELEASE checklist. Gates the `bismark-coverage2cytosine-v1.0` tag. (SPEC §12.3, §13.)

## 4. Sub-plan table

| # | Phase | Plan file | Depends on |
|---|-------|-----------|------------|
| A | Scaffold + CLI + genome reader | `phase-a-scaffold-cli-genome/PLAN.md` | — |
| B | Core genome-wide report | `phase-b-core-report/PLAN.md` _(to be written)_ | #A |
| C | `--gzip` + `--split_by_chromosome` | `phase-c-gzip-split/PLAN.md` _(to be written)_ | #B |
| D | `--merge_CpGs` (+ `--discordance`) | `phase-d-merge-cpgs/PLAN.md` _(to be written)_ | #B (#C if gzip-merge) |
| E | Real-data byte-identity gate | `phase-e-byte-identity-gate/PLAN.md` _(to be written)_ | #B, #C, #D |

## 5. Shared assumptions (apply across all phases)

1. **Byte-identity to Perl v0.25.1** is the binding contract for every in-scope output stream; STDERR is exempt.
2. **Input cov is 1-based, tab-separated, sorted by chr then pos**, column 4 (percentage) discarded; produced by Perl `bismark2bedGraph` for v1.0 tests (SPEC §4, §15 open item on #797 coordination).
3. **The report is genome-driven**: output order follows genome sequence order; uncovered cytosines emit `0 0` (unless a positive `--coverage_threshold`). Covered chromosomes emit in cov-file appearance order (insertion-ordered map, **never `BTreeMap`**); uncovered in bytewise-sorted order.
4. **Genome is uppercased on load** (soft-mask safety) and held wholly in memory (matches Perl).
5. **`u32`** for positions + counts, with a Phase-A overflow guard rejecting chromosomes > `u32::MAX`.
6. **Coordinate arithmetic** per SPEC §7.1 is the single source of correctness — `pos = i+1`; `substr(pos-1,3)` / `substr(pos-3,3)`+revcomp; `substr(seq,-1,3)` negative-wrap at `i=0`; last-base exclusion.
7. **Crate conventions** match `bismark-dedup`/`bismark-extractor`: `#![forbid(unsafe_code)]`, `#![warn(missing_docs)]`, clap-derive `Cli`→`validate()`, `thiserror` errors, `BufWriter` (+`GzEncoder` for `--gzip`), `flate2` for gz I/O, partial-output cleanup on error.
8. **All work in the `../Bismark-c2c` worktree** on `rust/coverage2cytosine`; never `git checkout` the shared main checkout; never edit `rust/bismark-extractor` or `rust/bismark-bedgraph`.

## 6. Integration points

- **Phase A → B–E:** `ResolvedConfig`, `Genome`, and `BismarkC2cError` from A are consumed by every later phase. The genome reader is the shared input to both the report walk (B) and the (B-emitted, D-reread) merge pass.
- **Phase B → C:** C wraps B's writers in gzip and multiplexes them per-chromosome; B's writer abstraction must be parameterizable over sink (plain/gz) and over single-vs-per-chr from the start (designed in B, exercised in C).
- **Phase B → D:** D re-reads B's CpG report file, so B's exact report-line bytes are D's input contract; a B regression surfaces in D's sanity asserts.
- **Phase B/C/D → E:** E runs the real binary end-to-end across the flag matrix; it is the integration test of all prior phases against Perl.
- **Crate → `bismark-extractor` (future, out of scope here):** the `lib` API (a single `run(config) -> Result<(), BismarkC2cError>` entry, plus the genome reader) is the seam the extractor will call inline to replace its Perl `coverage2cytosine` subprocess (SPEC §13). The wiring lives in the extractor crate (parallel session) — this epic only makes the seam exist.

## 7. References

- Design contract: [`SPEC.md`](./SPEC.md). Progress: [`PROGRESS.md`](./PROGRESS.md).
- Perl source: `coverage2cytosine` (v0.25.1) at the Bismark repo root.
- Sibling epic: #797 `bismark-bedgraph` (upstream producer; parallel session on `rust/bismark-bedgraph`).
- Memory: [[project_coverage2cytosine_port]], [[project_phase_h_byte_identity_ordering]], [[reference_colossal_access]], [[project_rust_rewrite]].

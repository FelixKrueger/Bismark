# Epic — Apple Silicon optimization of `bismark-aligner` (M4 Max), byte-identical

Branch: `apple-silicon-opt` (off `origin/rust/iron-chancellor`).
Maintainer directive (PR #1006 thread): *"base everything off of the iron-chancellor branch."*

## Goal & constraint

Make `bismark-aligner` faster on aarch64 (Apple Silicon) **without changing a
single output byte** on the faithful Bowtie 2 path (CLAUDE.md invariant:
byte-identical to Perl v0.25.1). The aligner is ~74% of pipeline runtime. The
maintainer already optimized I/O de/compression parallelism, so the remaining
headroom is the **CPU-bound Rust work**: per-record allocation churn in the
FastQ conversion loop + the bowtie2-output parse path, the bisulfite byte
transform, and the methylation/tag path.

Every change here is a **transparent, output-neutral win** — none introduces a
new algorithm or output, so **none needs a `--flag` / concordance gate** (unlike
`--combined_index` / `--rammap`). The single opt-in (`target-cpu`) is a
*build-portability* choice, not a runtime output gate.

## Discipline (per change)

**profile → prove byte-identity → measure.**
1. Rust-vs-Rust byte-identity oracle on **RRBS-10M** (fast loop) — zero bytes
   different vs the pre-change baseline binary.
2. Re-profile with `samply` to confirm the expected hotspot moved.
3. Wall-time (median of ≥3, `/usr/bin/time -l`) at `--parallel 1` and `8`.

Datasets: iterate on mouse RRBS / GRCm39; authoritative gate on human
WGBS-10M / GRCh38 (Phase 6). See `bismark-aligner/tests/byte_identity_real_data.rs`
and the `just aligner-*` / `profile-aligner` recipes.

## Phases

- **P0 — scaffolding (no behavior change):** `[profile.profiling]` (DWARF-keeping,
  release-equivalent codegen; never built by ci/reproduce/build); the Rust-vs-Rust
  oracle test + `just aligner-golden`/`aligner-oracle`/`profile-aligner`/`build-native`
  recipes; capture baseline golden + flamegraphs (`--parallel 1` and `8`) on RRBS-10M.
- **P1 — mimalloc:** `#[global_allocator]` in `src/main.rs` + `mimalloc = { version =
  "=0.1.52", default-features = false }` (exact sibling pin). Output-neutral; relieves
  allocator contention on per-record String/Vec churn. Gate: oracle + `just reproduce`.
- **P2 — per-record allocation reduction:** buffer-reuse / in-place variants in
  `src/convert.rs` (`convert_seq_c_to_t`/`_g_to_a`/`fix_id`, loop in `convert_fastq_impl`);
  gate the `raw_line` clone in `src/align.rs` `SamRecord::parse` (only when `--ambig_bam`).
- **P3 — vectorize the bisulfite transform (profile-gated):** branchless autovectorizable
  rewrite first; explicit `core::arch::aarch64` NEON only if the flamegraph shows the
  transform is still hot (with a property test). `std::simd` is OUT (nightly-only on MSRV 1.89).
- **P4 — `SamRecord` borrow refactor (higher-risk):** borrow `&str`/ranges from a reused
  line buffer instead of owning 5–7 Strings; threads lifetimes through `merge.rs`/`mapq.rs`/
  `output.rs`. Own commit + full oracle pass; fall back to P2's `raw_line` win if not provably
  clean. Also buffer-reuse in `src/methylation.rs` (`methylation_call`, `reverse_complement`,
  genomic-window extraction).
- **P5 — build tuning + micro-benches:** `just build-native` (`RUSTFLAGS=-C target-cpu=native`,
  opt-in only, NEVER a committed `.cargo/config.toml` default); criterion benches for the
  transform / `SamRecord::parse` / `methylation_call`.
- **P6 — authoritative gates + PR:** GRCh38 prepared; WGBS-10M oracle; `run_gate.sh`
  Rust-vs-Perl on RRBS + WGBS; `just ci`; `just reproduce`; open one PR to iron-chancellor
  citing before/after flamegraphs + wall-time table.

## Environment notes (M4 Max)

- Toolchain rustc 1.95.0 stable; MSRV 1.89 → `std::simd` nightly-only (P3 uses autovec / NEON intrinsics).
- `samply`, `samtools`, `bowtie2 2.5.5`, Perl `bismark` all present.
- Datasets at `/Users/benjamin/bismark_benchmarks/` (`RRBS_PE/SRR24766921_10M_{1,2}`, `WGBS_PE/`, `genomes/{GRCm39,GRCh38}`).

## Progress log

- **2026-06-22 — P0 scaffolding landed:** `[profile.profiling]` (`rust/Cargo.toml`),
  `tests/byte_identity_real_data.rs`, justfile recipes.
- **2026-06-22 — genome-prep incident:** the pre-existing GRCm39 bisulfite index was
  **corrupt** (an interrupted `bismark_genome_preparation` left `.bt2.tmp` temp files with
  truncated BWT arrays; `bowtie2-inspect -s` read only the intact header, so it looked valid,
  but `bowtie2-align` failed with `Error reading _ebwt[] array: no more data`). GRCh38 had no
  index. Rebuilt both with `bismark_genome_preparation_rs --parallel 6` (user-approved). The
  RRBS fast loop + WGBS gate both stand on freshly-built indices.
- *(pending)* P0 baseline capture (golden + flamegraphs) once prep completes → `GATE_00_baseline.md`.

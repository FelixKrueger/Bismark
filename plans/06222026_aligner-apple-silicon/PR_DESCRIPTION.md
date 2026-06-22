# perf(aligner): mimalloc fixes `--multicore` allocator-contention anti-scaling (3.1x), + byte-identical parse/output cleanups

## What

Performance work on `bismark-aligner`, profiled on Apple Silicon (M4 Max),
**byte-identical to Perl v0.25.1** on the faithful Bowtie 2 path. Everything here
is output-neutral (no new flag, no concordance gate).

### The headline: mimalloc

`bismark-aligner` was the only hot crate without a multithreaded allocator (4
sibling crates already use mimalloc). Under `--multicore`, the worker threads
contended on the system allocator's arena locks, an anti-scaling pathology:

| RRBS-10M, GRCm39, M4 Max | wall |
|---|---|
| `-p 6` (1 Rust pipeline) | 1161 s |
| `--multicore 4`, system allocator | **5025 s** (4.3x SLOWER than `-p 6`) |
| `--multicore 4`, mimalloc | **1603 s** (3.13x faster, contention gone) |

Under `-p` the aligner is ~90% blocked on bowtie2, so the Rust allocator is off
the critical path. A median-of-3 here measured the full PR stack **~6% slower**
under `-p 6` (BASE 1145 s vs AFTER 1217 s) — likely mimalloc's single-thread
overhead (or uncontrolled laptop thermal drift; the same session showed 2.1×
thermal variance on long runs). This needs a **clean re-measurement on the Linux
x86_64 benchmark host** before relying on it; flagged honestly rather than hidden.
Net: a clear `--multicore` win, possibly a small `-p` cost — you may want to weigh
gating mimalloc on `--multicore`. The change is **byte-identical** and
**bit-reproducible** (`SOURCE_DATE_EPOCH`, same as the 4 sibling mimalloc crates).
See `BENCHMARKS.md` for the full table + thermal caveat.

### Minor byte-identical cleanups (honest framing)

- **`SamRecord::parse` byte-level field split**: replaced `str::split('\t')`
  (`CharSearcher`, per-char decode) with a byte scan over the already-validated
  `&str`. **2.0x faster at the function level** (criterion: 278 ns -> 136 ns), but
  **within end-to-end noise** because bowtie2 dominates. Zero ripple (signature
  unchanged), byte-identical.
- **`build_pe_mate`**: dropped two per-mate intermediate allocations (an `md`
  scratch `Vec` the SE path already avoided, and an `md_value` `String` copy).
  Byte-identical; end-to-end within noise.

Profiling **ruled out** vectorizing the C->T/G->A transform (it is ~0.2% of wall;
GATE_00) — a profile-gated non-action.

## Why it is framed honestly

Under `-p` (the common single-pipeline usage), the aligner is **bowtie2-bound**:
~90% of `bismark_rs` wall is blocked in `read()` waiting on the external bowtie2,
~10% is Rust CPU (GATE_00 profile). So **no `-p` speedup is claimed** from
Rust-side work. The win is the `--multicore` allocator-contention fix, which is
exactly the regime the suite's scaling benchmarks use. The parse/output cleanups
are free, byte-identical, and faster at the function level, so they stay, but
they are not over-claimed as an end-to-end speedup.

## Validation

- **Byte-identity (Rust-vs-Rust)**: a new `#[ignore]`d oracle
  (`tests/byte_identity_real_data.rs`) compares the full BAM record stream
  (12,558,088 records) + report against a golden; verified identical across
  mimalloc + parse + build_pe_mate, and across `-p` vs `--multicore` (also proves
  parallel-invariance).
- **Byte-identity (Rust-vs-Perl)**: the full stack (mimalloc + parse +
  build_pe_mate) vs Perl Bismark v0.25.1 on RRBS / GRCm39: **246,856 records
  identical, report identical** (sorted `samtools view` + report diff, the
  `run_gate.sh` methodology run inline since that script trips macOS bash 3.2's
  `set -u` + empty-array bug).
- **Reproducibility**: `bismark_rs` built twice (clean) under `SOURCE_DATE_EPOCH`
  is bit-identical.
- **CI**: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`,
  `cargo test --workspace` all green.

## Notes for review

- The work is profiled on M4 Max only; the mimalloc win should re-measure on the
  Linux x86_64 benchmark host (it is universal, not ARM-specific — the extractor's
  mimalloc win was on Linux).
- Adds a `profiling` cargo profile (symbols for samply; never built by CI), a few
  `just` perf recipes, a `criterion` dev-dependency + one bench, and a documented
  **opt-in** `just build-native` (`RUSTFLAGS=-C target-cpu=native`; never the
  committed/CI default, to keep release binaries portable/reproducible).
- Provenance + per-change measurements live in `plans/06222026_aligner-apple-silicon/`.

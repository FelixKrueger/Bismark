# GATE 01 — mimalloc global allocator

**Change:** add mimalloc `#[global_allocator]` to `bismark-aligner` (Cargo.toml +
src/main.rs), exact sibling pin `=0.1.52, default-features = false`.
**Byte-identity:** allocator-only, output-neutral by construction (verified below).
**Hardware:** Apple M4 Max (12 perf cores, 128 GB). **Dataset:** mouse RRBS PE,
`SRR24766921_10M` (10M pairs), GRCm39, Bowtie 2 2.5.5, directional.
Wall-time via `/usr/bin/time -l` (single run each; medians for the PR later).

## Wall-time

| Regime | baseline | mimalloc | delta | note |
|---|---|---|---|---|
| `-p 6` (1 Rust pipeline, 2 bowtie2 × 6 threads) | 1161.01 s | 1158.29 s | **−0.2 % (noise)** | Rust side single-threaded → no allocator contention |
| `--multicore 4` (4 Rust workers) | **5025.17 s** | **1602.97 s** | **3.13× faster (−68 %)** | system allocator anti-scaling → mimalloc cures it |

Peak RSS: ~3.0 GB baseline / ~2.9 GB mimalloc in both regimes.

**The headline:** the system-allocator `--multicore 4` baseline (5025 s) is **4.3× SLOWER
than `-p 6`** (1161 s) — a catastrophic anti-scaling pathology (the 4 Rust worker
threads spin on allocator arena locks; peak RSS stays at 3 GB, so it is
lock-bound, not memory-bound — the exact shape the extractor hit). mimalloc
removes the contention: `--multicore 4` drops to 1603 s, a **3.13× speedup**, and
`--multicore` becomes usable. (On this dataset `-p 6` is still marginally faster
than the cured `--multicore 4`, but `--multicore` is the regime the maintainer's
benchmarks use for scaling past Perl, and without mimalloc it is unusable.)

## Interpretation (honest)

The `-p 6` result confirms the prior: under single-pipeline `-p`, the aligner's
wall-time is **dominated by the external bowtie2 process** (alignment compute +
the Rust side waiting on bowtie2 output), so the Rust-side allocator is off the
critical path and mimalloc is neutral (−0.2 %, within run-to-run noise). This is
expected for an orchestrator-around-bowtie2 design.

mimalloc's documented win in the sibling crates (extractor 155.8 s → 23.5 s) came
specifically from **arena-lock contention across worker threads** under
`--parallel N`. The aligner's equivalent is `--multicore N` (N concurrent Rust
pipelines). The `--multicore 4` row is therefore the decisive measurement for
whether mimalloc earns its place here.

**Implication for the epic:** if `--multicore` also shows ≈0, then the realistic
`-p` usage is bowtie2-bound and the Rust-side optimizations (mimalloc + alloc
reduction) move the needle only at high `--multicore` parallelism — which is
exactly the regime where the maintainer's benchmarks show Rust scaling past Perl.
We will state this scope explicitly rather than over-claiming a `-p` speedup.

## Byte-identity

Rust-vs-Rust oracle (decompressed BAM record stream + report with path +
`Bismark completed in` duration lines normalized out), golden captured from the
baseline `-p 6` run (**12,558,088** records):

| Output | BAM records | report (numbers) |
|---|---|---|
| mimalloc `-p 6` vs golden | ✓ identical | ✓ identical |
| mimalloc `--multicore 4` vs golden | ✓ identical | ✓ identical |
| baseline `--multicore 4` vs golden | ✓ identical | ✓ identical |

→ mimalloc is **output-neutral**, and the aligner is **parallel-invariant**
(`-p` ≡ `--multicore`) at the byte level. The only report delta is the
`Bismark completed in <duration>` line (a per-run wall-clock timing, sanctioned
timestamp class). `normalize_report` (test) + the `just aligner-oracle` recipe
strip path lines + that duration line.

`just reproduce` (bit-identical binaries with the mimalloc C-dep under fixed
`SOURCE_DATE_EPOCH`): ✓ **PASS** — `bismark_rs` built twice (clean before each)
under `SOURCE_DATE_EPOCH=1700000000` is byte-for-byte identical. mimalloc's
`cc`-built C dep is deterministic here, as it already is for the 4 sibling crates.

## Verdict

mimalloc **lands**: a 3.13× speedup of the `--multicore` path (curing a 4.3×
allocator-contention anti-scaling pathology), neutral under `-p` (bowtie2-bound),
byte-identical output, bit-reproducible. Pure win, no gate flag. The one-line PR
framing: *"fix `--multicore` allocator-contention anti-scaling in the aligner
(mimalloc), 3.1× on the contended path, byte-identical."*

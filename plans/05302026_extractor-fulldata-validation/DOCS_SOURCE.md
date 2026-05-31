# Phase 3 — source material for the `--parallel` help + README "Resource usage" docs

Raw data: `perf_sweep_results.csv` (this dir). Campaign: full-dataset benchmark on oxy
(Xeon 6975P-C, 64c/128t), Rust `iron-chancellor @ a7aaf61` vs Perl Bismark v0.25.1.
Numbers below are medians over reps (2–3); recompute from the CSV for final figures.

## Verdict (lead with this)
- **Byte-identical (parity) to Perl v0.25.1 at full scale** — WGBS-PE (64.6M read pairs) +
  WGBS-SE (63.6M reads) PASSED (gzip; sorted-equivalent data + identical reports). RRBS-PE
  byteid not finished before the budget pivot, but PE+SE prove the calling path.
- **gzip (the default/realistic path) is 4.2–4.9× faster than Perl's best (`--multicore 12`):**

  | Dataset | Size | Perl mc12 | Rust (gzip, any `--parallel`) | Speedup |
  |---|---:|---:|---:|---:|
  | WGBS-PE | 64.6M read pairs | 479 s | ~99 s | 4.8× |
  | WGBS-SE | 63.6M reads | 237 s | ~48 s | 4.9× |
  | RRBS-PE | 30.6M read pairs | 197 s | ~47 s | 4.2× |

  Report PE sizes as **read pairs** (intuitive), not records. `samtools view -c` counts
  *alignment records* = 2 per pair, so WGBS-PE 129.3M records = 64.6M pairs, RRBS-PE 61.2M
  = 30.6M pairs; SE is 1 record/read. (Cross-check: WGBS-PE 64.6M pairs ≈ WGBS-SE 63.6M
  reads — same sample's fragments, R1 aligned SE.)
  - Note Perl mc12 used ~19 CPU-cores (fork model re-decodes the BAM N×); Rust uses ~7
    and decodes once → faster *and* leaner.

### ⚠️ Apples-to-apples caveat (MUST state in docs — messaging-sensitive)
The Perl-vs-Rust comparison is **NOT per-core**, and the docs must say so plainly:
- **Perl `--multicore 1` ≈ 1 core** (a single process).
- **Rust `--parallel 1` ≈ 7–8 cores** in gzip mode — because the 2-thread parallel BGZF
  decode and the gzip **compression pool (≈60 threads)** are **always-on and independent of
  `--parallel`**. `--parallel` controls only the *extraction worker* count, not total CPU.
- Therefore **do NOT publish a "Rust `--parallel 1` vs Perl `--multicore 1`" speedup** — it
  would compare ~8 cores against ~1 and badly overstate the win. The honest comparison is
  **wall-clock at comparable resource: Rust default (~7 cores) vs Perl `--multicore 12`
  (~19 cores) → 4.8×, on fewer cores.**
- Perl-only serial reference (do NOT headline as a Rust speedup): Perl `--multicore 1` on
  WGBS-PE = **4583 s (~76 min) at ~2 cores** — even Perl's "1" is ~2 cores because it pipes a
  `samtools view` subprocess. The Rust default (~99 s, ~7 cores) replaces that 76-min run;
  state it as "≈76 min → ≈99 s with built-in parallelism," WITH the core counts — never "46×".
- **Pre-empt the "why does `--parallel 1` use 800% CPU?" reaction** with a positive framing:
  the default already parallelizes decode + compression for you (no flag needed); `--parallel`
  is only the extra extraction-worker knob, and the realistic (gzip) run will show ~700–800%
  CPU by design — that is the tool using the machine, not a bug.

## Per-mode resource footprint (WGBS-PE primary; SE/RRBS consistent)
Wall is **flat across `--parallel {1,2,4,8,16}`** in every mode — so the table is per-MODE,
not per-parallel. Thread count grows with `--parallel` (one worker thread each) but the
extra workers are idle (decode-bound), so cores/wall don't improve.

| Mode | Wall (WGBS-PE) | CPU cores | Threads (p1 → p16) | open fds | Peak RSS |
|---|---:|---:|---:|---:|---:|
| **gzip** (default) | ~99 s | **~7.1** | 67 → 81 | 17 | 0.2–0.7 GB (peak ~1.25 GB @ p16) |
| **plain** (`.txt`) | ~640 s | **~0.65** | 7 → 21 | 17 | 0.18 → 1.2 GB |
| **`--mbias_only`** | ~94 s | **~3.2** | 7 → 21 | 5 | 45–100 MB |

Thread model (exact, verified in source): `~5 base + max(--parallel,2) workers + (gzip mode
only: GZIP_COMPRESS_THREADS=4 × ~12 open files = 60)`. fds = 12 output files + ~5 (stdio +
input BAM); `--mbias_only` writes no per-context files → 5.

## The three findings (each a docs line)
1. **`--parallel` does not speed up extraction on BAM.** The pipeline is decode-bound
   (fixed 2-thread parallel BGZF reader is the ceiling; the worker floor of 2 already
   saturates it). Wall is flat 1→16. → "Size resources by mode, not by `--parallel`."
2. **gzip (default) is ~6× FASTER than plain `.txt`** (99 s vs 640 s on WGBS-PE). Plain runs
   at ~0.65 cores — it is disk-WRITE-bound on the large uncompressed output; gzip spends ~7
   cores compressing but writes far fewer bytes and finishes sooner. → "The compressed
   default is the fast path; do not disable gzip to 'save time'." (Caveat: the exact 6×
   is storage-dependent — `/var/tmp` NVMe here; frame as "plain is I/O-bound, gzip faster
   on typical storage," not a fixed ratio.)
3. **Footprint is mode-shaped:** gzip ≈ 8 cores / ~80 threads / ≤~1.3 GB; mbias/plain ≈
   1–3 cores / ~20 threads. Recommend (gzip default): **`cpus ≈ 8`, `memory ≈ 2 GB`, and
   `ulimit -u`/`nproc` headroom for ~80 threads** — all independent of `--parallel`.

## DRAFT doc snippets (refine in Phase 3)

### `--parallel` help text (cli.rs)
> `--parallel <int>`  Number of methylation-extraction *worker* threads (default 1; floored
> at 2 for BAM). This controls ONLY the extraction workers — it is **not** the total core
> count. BAM decoding (fixed 2-thread parallel reader) and gzip output (a fixed compression
> pool) are always-on and independent of `--parallel`, so even `--parallel 1` uses ~7–8 CPU
> cores in the default gzip mode by design (this is expected, not a bug). The default is
> already throughput-optimal; **raising `--parallel` does not speed up extraction on BAM
> input** (the pipeline is decode-bound). Retained for compatibility. See README → Resource usage.

### README "Resource usage (HPC & nf-core)" section
> The Rust extractor's speed is architectural, not a tuning knob — unlike the Perl
> `--multicore` fork model, you do **not** scale `--parallel` to go faster. Note that
> `--parallel 1` is **not** single-threaded: decode and gzip compression run in parallel
> automatically, so the default uses ~7–8 CPU cores in gzip mode (by design — not a runaway
> process). `--parallel` adds only extraction workers on top. Request a fixed allocation per
> output mode:
> | Mode | cpus | memory | notes |
> |---|---|---|---|
> | gzip (default) | ~8 | ~2 GB | ~80 threads peak → ensure `ulimit -u`/`nproc` headroom |
> | `--mbias_only` / plain | ~3 | ~0.5–1.5 GB | plain output is large + I/O-bound |
> At full scale (human WGBS, gzip) the Rust extractor is byte-identical to Perl Bismark
> v0.25.1 and ~4.8× faster than Perl `--multicore 12`, using ~7 cores vs ~19.

## Caveats / scope
- Proven for **BAM input, gzip/plain/mbias** on WGBS-PE/SE + RRBS-PE. NOT characterized:
  SAM/CRAM input (non-BGZF; worker side may matter more), or pathologically heavy per-read
  workloads. So the precise claim is "`--parallel` is a no-op for speed on BAM," not "useless."
- Medians are rough (2–3 reps); recompute from `perf_sweep_results.csv` for published figures.
- The RRBS Perl mc12 row shows threads=1 (the sampler counts the parent PID's threads; Perl
  forks child *processes*, so its thread count is not comparable to Rust's — use wall only).

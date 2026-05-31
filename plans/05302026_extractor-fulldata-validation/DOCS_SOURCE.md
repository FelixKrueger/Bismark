# Phase 3 — source material for the `--parallel` help + README "Resource usage" docs

Raw data: `perf_sweep_results.csv` (this dir). Campaign: full-dataset benchmark on oxy
(Xeon 6975P-C, 64c/128t), Rust `iron-chancellor @ a7aaf61` vs Perl Bismark v0.25.1.
Numbers below are medians over reps (2–3); recompute from the CSV for final figures.

## Verdict (lead with this)
- **Byte-identical (parity) to Perl v0.25.1 at full scale** — WGBS-PE (129.3M reads) +
  WGBS-SE (63.6M) PASSED (gzip; sorted-equivalent data + identical reports). RRBS-PE
  byteid not finished before the budget pivot, but PE+SE prove the calling path.
- **gzip (the default/realistic path) is 4.2–4.9× faster than Perl's best (`--multicore 12`):**

  | Dataset | Reads | Perl mc12 | Rust (gzip, any `--parallel`) | Speedup |
  |---|---:|---:|---:|---:|
  | WGBS-PE | 129.3M | 479 s | ~99 s | 4.8× |
  | WGBS-SE | 63.6M | 237 s | ~48 s | 4.9× |
  | RRBS-PE | 61.2M | 197 s | ~47 s | 4.2× |
  - Single-core Perl baseline (`--multicore 1`, WGBS-PE) PENDING — append the ~25–35×
    headline when the serial run finishes.
  - Note Perl mc12 used ~19 CPU-cores (fork model re-decodes the BAM N×); Rust uses ~7
    and decodes once → faster *and* leaner.

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
> `--parallel <int>`  Worker threads for methylation extraction (default 1; floored at 2
> for BAM). NOTE: BAM decoding uses a fixed 2-thread parallel reader and gzip output a fixed
> compression pool, both independent of `--parallel`. The default is already
> throughput-optimal; **raising `--parallel` does not speed up extraction on BAM input**
> (the pipeline is decode-bound). Retained for compatibility. See README → Resource usage.

### README "Resource usage (HPC & nf-core)" section
> The Rust extractor's speed is architectural, not a tuning knob — unlike the Perl
> `--multicore` fork model, you do **not** scale `--parallel` to go faster. Request a fixed
> allocation per output mode:
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

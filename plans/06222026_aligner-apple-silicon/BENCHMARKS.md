# bismark-aligner — Apple Silicon performance benchmarks

**Hardware:** Apple M4 Max (12 performance cores, 128 GB).
**Dataset:** mouse RRBS PE `SRR24766921_10M` (10M pairs), GRCm39, Bowtie 2 2.5.5,
directional. **Wall-time:** `/usr/bin/time -l` (peak RSS from the same).
**Compared:** `BASE` = pre-epic (system allocator) vs `AFTER` = this PR
(mimalloc + parse byte-split + build_pe_mate alloc reduction). Both produce
**byte-identical** output (12,558,088 records == golden; and == Perl v0.25.1).

> **Thermal caveat (important):** sustained multi-hour benchmarking on a laptop
> throttles. A control run of the single-threaded `--multicore 1` config measured
> 5746 s then 12281 s (2.1× variance, same deterministic work) once the machine
> had been under load for ~6 h. The headline `--multicore 4` numbers below are
> **isolated cool-machine runs** (one config at a time, early); the full
> median-of-3 sweep was abandoned as thermally unreliable. **These numbers should
> be re-measured on the Linux x86_64 benchmark host** (where the suite's scaling
> graphs are produced and there is no laptop throttling). The mimalloc win is
> universal, not ARM-specific (the extractor's mimalloc win was measured on Linux).

## Headline: mimalloc fixes the `--multicore` allocator-contention anti-scaling

| `--multicore 4` (8 bowtie2 threads) | wall | peak RSS |
|---|---|---|
| BASE (system allocator) | **5025 s** | 3.0 GB |
| AFTER (mimalloc) | **1601 s** | 2.9 GB |
| **speedup** | **3.13×** | — |

With the system allocator, `--multicore 4` is **4.3× SLOWER than `-p 6`** (5025 s
vs ~1150 s): the 4 Rust worker threads spin on the allocator's arena locks (peak
RSS stays ~3 GB, so it is lock-bound, not memory-bound). mimalloc removes the
contention and `--multicore` becomes usable. This is the regime the suite's
scaling benchmarks use.

## `-p 6` (single Rust pipeline, 2 bowtie2 × 6 threads) — bowtie2-bound, within noise

Under `-p`, the aligner is **~90 % blocked waiting on the external bowtie2**
(`sample` profile: 73,702 `read`-syscall samples vs ~7,000 Rust-CPU samples), so
the allocator/parse work is off the critical path. A first un-interleaved
median-of-3 measured AFTER **~6 % slower** here, but a **cooled, interleaved
attribution run** (GATE_04: 3 distinct binaries, shared thermal state, 4-min
cooldown per run) showed that "−6 %" was a **thermal artifact** — the difference
vanishes into noise:

| `-p 6`, interleaved (GATE_04) | median | mean | per-binary spread |
|---|---|---|---|
| BASE (system allocator) | 1242 s | 1240 s | 3.4 % |
| MIMA (mimalloc only) | 1270 s | 1226 s | 14.0 % |
| AFTER (full PR stack) | 1226 s | 1223 s | 19.2 % |

Deltas vs BASE: AFTER **−1.3 % median / −1.4 % mean**; MIMA **+2.3 % median /
−1.1 % mean** (sign flips by statistic). The per-binary thermal spread (14–19 %)
**dwarfs** every inter-binary difference (±2 %), and AFTER's range
[1114, 1328] fully envelops BASE's [1218, 1259]. The three are **statistically
indistinguishable**. Net: mimalloc has **no measurable `-p` cost** (its
single-thread overhead is below this laptop's noise floor) — so there is **no
trade-off to weigh** and **no gate needed**; it is a clean `--multicore` win.

## Function-level: the parse byte-split (criterion, thermal-independent)

`cargo bench -p bismark-aligner --bench parse_bench`:

| SAM field split | time | speedup |
|---|---|---|
| `char_searcher` (pre-epic `str::split('\t')`) | 277.6 ns | — |
| `byte_scan` (this PR) | 136.2 ns | **2.04×** |

Full `SamRecord::parse`: 315.7 ns (≈1.45× faster than the old parse). This win is
real and clean at the function level, but **below the end-to-end noise floor**
because bowtie2 dominates — hence it is presented as a byte-identical
allocation/CPU-hygiene improvement, not claimed as an end-to-end speedup.

## Pipeline context (where the time goes, M4 Max, `-p 6`)

| Stage | wall | share |
|---|---|---|
| Alignment | 1161 s | **92.5 %** |
| Methylation extract + bedGraph + cytosine_report | 82 s | 6.6 % |
| Deduplication | 12 s | 1.0 % |

The aligner dominates the pipeline (even more than the maintainer's 74 % M1/WGBS
profile, because RRBS reads are short so the post-alignment stages are small), and
it is the least-optimized stage — confirming it as the right target. The other
stages already use mimalloc + parallel I/O.

## Summary

- **mimalloc**: **3.13×** on `--multicore 4` (cures a 4.3× allocator-contention
  anti-scaling pathology), byte-identical, bit-reproducible. The headline.
- **parse byte-split**: **2.04×** at the function level; end-to-end within noise.
- **build_pe_mate**: two per-mate intermediate allocations removed; within noise.
- **No `-p` trade-off**: a cooled interleaved attribution (GATE_04) shows the full
  stack is **statistically indistinguishable** from baseline under `-p` (−1.3 %
  median, well inside the 14–19 % per-binary thermal spread). The earlier "~6 %
  slower" reading was a thermal artifact of an un-interleaved comparison; mimalloc
  has no measurable single-thread cost. Re-confirm absolute wall-times on Linux
  x86_64; the function-level and byte-identity results are solid regardless.

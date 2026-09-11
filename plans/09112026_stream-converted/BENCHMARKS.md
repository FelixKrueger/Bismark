# BENCHMARKS — serial vs streamed conversion

**Spike:** [`SPIKE.md`](./SPIKE.md) · **Plan:** [`PLAN.md`](./PLAN.md) · **Issue:** [#1120](https://github.com/FelixKrueger/Bismark/issues/1120)
**Date:** 2026-09-11 · **Base:** `dev` `170bfce`
**Harnesses:** `spikes/bench.sh` (pre-implementation projection) · `spikes/bench-ab-run.sh` (the shipped code)

> **Headline, measured on the implementation:** the disk win is real and large — **4 KB of
> converted bytes against 96 MB**. The projected ~10 % speedup **did not reproduce**: wall time
> is a wash. Section 1 is the projection that was wrong and why; section 2 is the measurement
> that supersedes it.

## 1. Pre-implementation projection (superseded)

Kept because it was wrong in a way worth remembering: a stand-in converter flattered the result
by a factor the real one does not.

### Why streaming was expected to be a win, not a cost

Conversion currently runs to completion **before** the aligner is spawned: `process_se_chunk`
(`mod.rs:1200`) calls `convert_se_files(...)`, prints the "Created … converted version" banner,
then spawns. So conversion sits on the critical path today, and streaming overlaps it with
alignment.

### Projected measurement

300,000 read pairs, both directional instances, `-p 4`, 3 reps:

| | convert | align | total | converted on disk |
|---|---|---|---|---|
| Serial (current shape) | 472-605 ms | 4148-4210 ms | 4621-4815 ms | 139 MB |
| Streamed | overlapped | — | 4174-4355 ms | **0** |

~10% faster. FIFO transport in isolation showed no overhead (106-110 ms vs 107-109 ms).

**The caveat turned out to be the whole story.** The stand-in converter was `awk`; the real Rust
converter is fast enough that the serial phase being hidden is nearly free, so there was almost
nothing left to win. See section 2.

## 2. Measured on the implementation (authoritative)

`spikes/bench-ab-run.sh`, in the pinned container. 200,000 read pairs (93 MB uncompressed across
R1+R2, a 40x concatenation of `test_files` with unique read IDs), directional paired-end against
the E. coli index, `-p 4`, release build. **7 reps with the two arms interleaved** — arm-major
ordering would hand any thermal drift entirely to one arm.

| | wall, median of 7 | peak converted bytes in `--temp_dir` |
|---|---|---|
| Files (`--no_stream_converted`) | 6.35 s | **96 MB** |
| Streamed (default) | 6.33 s | **4 KB** |

Per-rep wall times (s), in run order:

| rep | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
|---|---|---|---|---|---|---|---|
| streamed | 6.36 | 6.35 | 6.14 | 6.14 | 6.13 | 6.33 | 6.34 |
| files | 6.95 | 6.34 | 6.32 | 6.74 | 6.35 | 6.53 | 6.34 |

**Wall time is a wash** — a 0.3 % median difference inside a spread several times that. The
honest claim is **wall-neutral**, not faster. Conversion does come off the critical path, but the
Rust converter is fast enough that the phase being hidden is small, and the fan-out's per-block
copy and thread handoffs cost about what it saves.

**The disk result is the point, and it is unambiguous**: 96 MB (≈1x the uncompressed input, as
predicted) versus 4 KB, which is the one FIFO inode per aligner instance. The 4 KB never grows
with read count; the 96 MB does.

## Known measurement gap

Files fully decouple the two aligner instances; a tee couples them — a fast instance can be
throttled by the bounded queue behind a slow one. Proven: no deadlock, no starvation,
byte-identical under `-p 1` vs `-p 4` at depth 1. **Not measured: wall time under that
asymmetry.** Deferred to pre-merge validation by agreement; queue depth is the mitigation knob.


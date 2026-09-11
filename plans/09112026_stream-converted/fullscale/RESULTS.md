# FULL-SCALE VALIDATION — RESULTS (#1120 / PR #1121)

**Brief:** [`../FULLSCALE.md`](../FULLSCALE.md) · **Benchmarks:** [`../BENCHMARKS.md`](../BENCHMARKS.md)
**Run started:** 2026-09-11 · **Status:** in progress — this file is updated as each phase lands.

---

## 1. Machine and setup

| | |
|---|---|
| CPU | 32 cores |
| RAM | 124 GB |
| Scratch | 1.1 TB (`/tmp`, `scratch_fusion-volume`) |
| Container | `spikes/Dockerfile.ab` — rust 1.89.0, bowtie2 2.5.0, samtools 1.16.1 |
| Build | `cargo build --locked --release -p bismark --bins` |
| Branch | PR #1121 head, `ce76320` |

**Data.** `SRR24766921` (mouse RRBS, paired-end), first 10,000,000 read pairs taken
straight off ENA, 51 bp:

```
SRR24766921_10M_1.fastq.gz   282,591,292 bytes
SRR24766921_10M_2.fastq.gz   281,578,687 bytes
```

**Genome.** Ensembl release 112 `Mus_musculus.GRCm39.dna.primary_assembly`, prepared once
with `bismark_genome_preparation --bowtie2 --parallel 16` (38 minutes; 13 GB of
`Bisulfite_Genome`) and reused by every run.

---

## 2. Method

`harness/drive.sh` drives the whole thing; `harness/run_arm.sh` is one arm.

**One benchmark at a time.** The driver takes a lockfile and refuses to start if another run
holds it. Nothing else runs on the box while a benchmark is in flight — no parallel arms, no
parallel shapes. Every run therefore sees the same idle machine.

**Each arm runs in its own container**, so cgroup v2 counters
(`memory.current`, `memory.peak`, `cpu.stat`, `pids.current`) describe exactly that run's
process tree, starting from zero. This is whole-tree accounting including every `bowtie2`
child — not the parent's RSS.

**Time series, not just peaks.** Every ~1 s each run records converted bytes on disk, whole
`--temp_dir`, output dir, memory, CPU and thread count. The sample loop uses bash builtins
for the clock and the cgroup reads and one `find` for the disk walk, so a tick costs ~16 ms
and each sample's timestamp is honest. (An earlier version spent over a second per tick,
which put the label a long way in front of the measurement — fatal for the disk curve, where
the whole point is *when* the converted bytes appear.)

**Peak temp-dir usage is sampled live.** Both arms delete their temp files at the end, so
measuring afterwards reports zero for both and proves nothing (`FULLSCALE.md` §5).

**Page cache is warmed identically before every run** — the 13 GB index and both inputs are
read into cache first, so no run is penalised for being the one that faulted them in. With
124 GB of RAM they stay resident.

**Arm order alternates between reps** (stream-first on odd reps, files-first on even). A
fixed order would hand any residual first-run penalty to the same arm every time.

**C1 is compared over `samtools view` record bodies**, not raw BAM bytes: the header's
`@PG CL:` line is the verbatim argv and differs between the arms by design. Reports are
compared with absolute paths filtered out, for the same reason.

---

## 3. Measured cost per run

Established by probe, `pe_directional_p4` at 10M pairs, `-p 4`: **~49 min per arm**.
Scaled estimates used for scheduling: non-directional ~60 min, `--multicore 2` ~25 min,
`-p 1` ~190 min (it gets 2 cores against 8).

---

## 4. Results

### 4.1 `pe_directional_p4` at 10,000,000 pairs — C1 and C2 (2026-09-11)

Directional paired-end, `-p 4`, one rep per arm, run serially on an idle machine.

| | wall | converted bytes on disk (peak, sampled live) | output | records |
|---|---|---|---|---|
| **streamed** (default) | **2946.77 s** (49.1 min) | **0 KB** | 856 MB | 12,131,476 |
| **files** (`--no_stream_converted`) | 2955.66 s (49.3 min) | **3,346,733 KB (3.19 GiB)** | 856 MB | 12,131,476 |

**C1 — PASS.** `samtools view` record bodies are identical (md5 over 12,131,476 records
matches), and the filtered report is identical. See `results/run/C1.tsv`.

**C2 — PASS.** The streamed arm never puts a converted byte on disk. The file arm holds
3.19 GiB — one uncompressed copy of the input — for the *entire* 49-minute run, not
briefly. Sampled live, because both arms clean up at the end.

**C3 — no regression.** The streamed arm was **8.9 s faster**, 0.30 % of a ~49-minute run.
That is well inside noise for n=1 and the honest reading is *wall-neutral*, which is what
the 200,000-pair bench found. Total CPU was also within 0.25 % (18,946 s vs 18,899 s).
Reps follow.

**What the time series shows.** The file arm spends its first ~33 seconds at 100 % CPU —
one core, converting — with zero output written. Only then does CPU rise to ~800 % as the
aligners start. The streamed arm is at ~800 % and emitting output within 12 seconds.
Conversion coming off the critical path is directly visible rather than inferred.

**Threads.** 29 peak (streamed) against 23 (files): six extra, being the converter and
writer threads behind the fan-out. Nowhere near any limit — see §4.2.

> **A measurement trap worth recording.** The file arm's peak `memory.current` is 15.88 GiB
> against the streamed arm's 9.58 GiB, which looks like the streamed path saving 6 GiB of
> memory — or, read the other way, would have tripped `FULLSCALE.md` §2's "if peak RSS moves
> by more than a few hundred MB, something is wrong". Neither reading is right. `memory.current`
> counts page cache, and the difference is the file arm's own 3.19 GiB of converted files
> sitting in cache plus the cache for reading them back. The sampler now records
> `memory.stat anon` alongside, so later runs separate allocation from cache; these two
> pre-date that column and carry `NA` for it.

Plots: `plots/run/pe_directional_p4_r1.png` (six stacked panels — converted bytes, whole
temp dir, output, cgroup memory, CPU, threads).

#### The instance asymmetry, measured

`FULLSCALE.md` §1 calls natural instance asymmetry "the real unknown", and `BENCHMARKS.md`
closes on it as the known measurement gap: a tee couples the aligner instances, so a fast one
can be throttled behind a slow one, and small fixtures never run long enough to show it. The
CPU trace answers it directly.

| | CPU profile | both instances finish |
|---|---|---|
| **files** | 800 % until t=1761 s, then **400 % for the remaining 1194 s** | no — one finishes 20 min early |
| **streamed** | a flat **645 %** for the whole run, never dropping | yes — together, at t=2944 s |

The asymmetry is real and large: with files, the two strand hypotheses are fully decoupled,
one finishes twenty minutes before the other, and the machine spends the last 40 % of the run
at half utilisation. With streaming, the shared bounded channel paces the fast instance to the
slow one, so both run steadily and finish together.

**And it is free.** Total CPU is identical — 18,990 vs 18,993 CPU-seconds — and the streamed
arm finished 8.9 s *sooner*. The reason is structural rather than lucky: wall time is set by
the slowest instance in both arms, and streaming does not slow that instance down; it only
declines to let the fast one race ahead and then idle. The same work, spread evenly instead of
front-loaded.

This is the throttling that `CHANNEL_DEPTH` exists to mitigate, observed at full scale and
costing nothing. It does not need the knob turned.

Thread counts corroborate: the file arm steps 20 → 15 at t=1761 s as an instance exits, while
the streamed arm holds 26 until both finish at t=2944 s.


### 4.2 Section 7 — the cases only a big run can reach (2026-09-11)

All three pass. Log: `logs/edge.log`.

#### 7.2 A genuinely constrained `--temp_dir` — **the premise, demonstrated**

`FULLSCALE.md` calls this "the whole point of the feature, and nothing has demonstrated it
yet". It has now. A 512 MiB tmpfs as `--temp_dir`, against 2M read pairs whose converted set
needs roughly twice that:

| arm | exit | records |
|---|---|---|
| files (`--no_stream_converted`) | **1** — `I/O error: No space left on device (os error 28)` | 0 |
| streamed (default) | **0** | 2,398,276 |

One converted copy of R1 alone is 318 MiB and the file arm needs two, so 512 MiB cannot hold
them. The file arm dies; the streamed arm completes normally and produces a full result set.
This is the feature working as designed on a disk-constrained scratch, which is the case it
exists for.

#### 7.1 Long sample names under `--multicore`

FIFO names cap the stem at `NAME_STEM_CAP` (64 bytes) and path uniqueness comes from a
process-wide counter rather than the name. A collision would surface as interleaved or
truncated output rather than an error, so exit code alone proves nothing — this compares
alignment columns against a short-named control.

| run | basename | exit | records |
|---|---|---|---|
| control | 26 chars | 0 | 2,398,276 |
| long name | **203 chars**, `--multicore 4` | 0 | 2,398,276 |

Alignment-identical. **PASS** — the fix that had unit tests but had never run at scale holds.

#### 7.3 Thread and FD counts

Measured across the whole process tree under the widest fan-out available
(`--non_directional --multicore 2` — 4 converted streams and 8 pipes, concurrently):

| | peak | limit |
|---|---|---|
| open FDs | **73** | 65,535 soft — **0.1 % used** |
| threads | 35 | — |
| processes | 9 | — |

**PASS**, with three orders of magnitude of headroom. Nothing approaches a ulimit.

Worth noting from the same run: all four conversion streams logged
`no temp file written`, so non-directional paired-end under `--multicore` streams every one
of its eight pipes.

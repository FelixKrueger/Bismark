# FULL-SCALE VALIDATION — RESULTS (#1120 / PR #1121)

**Brief:** [`../FULLSCALE.md`](../FULLSCALE.md) · **Benchmarks:** [`../BENCHMARKS.md`](../BENCHMARKS.md)
**Run:** 2026-09-11 → 2026-09-12 · **Status:** complete

---

## 0. Verdict

**All three claims pass. Section 7 passes. Nothing found that should block the merge.**

| | claim | result |
|---|---|---|
| **C1** | output byte-identical | **PASS** — 25 arm-pairs, 0 differing |
| **C2** | converted temp files gone | **PASS** — 0 KB against up to 6.38 GiB |
| **C3** | no wall-time regression | **PASS** — never slower; −0.01 % to −1.50 % |

52 runs, one at a time, mouse RRBS on GRCm39.

**Three things worth a reader's attention beyond the pass/fail.**

*The asymmetry question is closed, and the answer is more interesting than "no regression".*
The coupling everyone worried about is real and large — with files, the two strand hypotheses
finish twenty minutes apart and the machine spends the last 40 % of the run at half
utilisation. Streaming paces them together. Total CPU is identical to within 3 seconds in
19,000, and wall time is marginally better, because the critical path is the slow instance in
both arms. The bounded channel does throttle the fast consumer; that throttling is free.

*The feature's premise is demonstrated for the first time.* On a `--temp_dir` smaller than the
converted set, the file arm dies `ENOSPC` and the streamed arm completes with a full result
set. Everything else here is an efficiency argument; this is the correctness argument for the
feature existing, and until now nothing had shown it.

*A regression was reported and then retracted.* One full-scale run showed non-directional
streaming 2.4 % slower, with a plausible mechanism attached. Reps showed it was an outlier. It
is left in the record (§4.3, §4.7) rather than edited away.

**Recommendation: merge.** No knob needs turning — `CHANNEL_DEPTH` stays at 4. Two caveats
that are not blockers: this is one dataset on one genome on one machine, and a clean A/B says
nothing about edges beyond the three §7 probes.

---

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

### 4.3 `pe_nondirectional_p4` at 10,000,000 pairs (2026-09-11)

Non-directional paired-end, `-p 4` — 4 converted streams and 8 pipes, against the directional
case's 2 and 4.

| | wall | converted on disk (peak, live) | peak memory, total | peak memory, anon | records |
|---|---|---|---|---|---|
| **streamed** | 3045.2 s | **0 KB** | 22.95 GiB | 22.77 GiB | 12,131,470 |
| **files** | **2973.2 s** | **6.38 GiB** | 30.58 GiB | 23.96 GiB | 12,131,470 |

**C1 — PASS.** md5 over all 12,131,470 records matches; filtered report identical.

**C2 — PASS.** Zero against 6.38 GiB — twice the directional footprint, as expected from four
converted streams rather than two.

**C3 — streaming is 2.4 % SLOWER on this shape.** 72 seconds on a ~50-minute run. Total CPU
was near-identical (25,029 vs 24,962 CPU-seconds, 0.3 % apart), so the streamed arm did the
same work spread over a longer wall.

This is the first cost the feature has shown, and it is the opposite sign to §4.1. The CPU
traces explain it:

| | early phase (p90) | settles to | tail |
|---|---|---|---|
| streamed | 1462 % | flat ~650 % from t=753 s | none — all four finish together |
| files | 1623 % | ~800 % | drops below 800 % at t=1812 s, coasts out on fewer instances |

The file arm still has the decoupling tail described in §4.1, but here it also front-loads
harder, and the early advantage more than pays for the tail. With four consumers sharing two
converted streams there is less slack in the fan-out than with two, so pacing costs something
where previously it cost nothing.

**Still inside the pass criterion** (§6 asks for "within a few percent") but it is n=1 and it
wants reps before anyone concludes anything. If 2.4 % proves consistent it is the first real
argument for raising `CHANNEL_DEPTH`. See §4.5.

> A prediction recorded during the run, and wrong: watching the file arm's CPU collapse to
> 400 % mid-run, this session expected the tail to make the file arm *slower* overall. It
> finished 72 s sooner. A visible inefficiency in one phase says nothing about total wall time
> until the other phases are measured too.

**Memory.** The file arm's 30.58 GiB total is mostly page cache for its own converted files.
But its *anonymous* memory is 1.2 GiB higher too, so on this shape streaming saves a little
real allocation as well — the opposite direction to the concern §2 raised.

### 4.4 `pe_directional_mc2` at 10,000,000 pairs (2026-09-11)

Directional paired-end, `-p 4 --multicore 2` — two concurrent chunks, each with its own pipe
set.

| | wall | `--temp_dir` peak | of which converted | peak memory, anon | records |
|---|---|---|---|---|---|
| **streamed** | **1633.9 s** | 3.92 GiB | **0** | 14.95 GiB | 12,131,476 |
| **files** | 1642.2 s | 7.09 GiB | **3.19 GiB** | 18.09 GiB | 12,131,476 |

**C1 — PASS, and more than required.** The md5 is `f8129cad…` — *the same digest as §4.1*.
`--multicore 2` produces output byte-identical to the single-chunk run, in both arms. Chunking
does not perturb the record stream at all.

**C2 — PASS, with a distinction worth stating.** This is the one shape where the streamed
arm's `--temp_dir` is *not* empty: it holds 3.92 GiB. That is the `--multicore` input chunk
splits, which both arms write and which this PR does not touch. The converted payload —
the thing streaming removes — is 0 against 3.19 GiB. Quoting the temp-dir totals alone
(3.92 vs 7.09 GiB) would understate the effect and misattribute the remainder; the sampler
tracks the converted bytes separately for exactly this reason.

**C3 — streamed 8.2 s faster** (0.5 % of ~27 min), the same wash as §4.1. Total CPU 19,800 vs
19,660 CPU-seconds.

**Memory.** Anonymous peak 14.95 GiB streamed against 18.09 GiB files — a 3.1 GiB saving in
real allocation, the largest of any shape so far. Two concurrent chunks each reading back
their own converted files is precisely where the file path costs the most.

### 4.5 C3 with reps — 2,000,000 pairs, 5 interleaved reps per arm (2026-09-11)

Wall time is the claim that needs repetition rather than size, so this runs at 2M pairs where
a rep costs ten minutes instead of fifty. Every run is still minutes long, far past any
start-up transient, and arms alternate order between reps.

| shape | n | streamed median | files median | delta |
|---|---|---|---|---|
| `pe_directional_p4` | 5 | 603.5 s | 607.5 s | **−0.66 %** |
| `pe_nondirectional_p4` | 5 | 613.7 s | 617.8 s | **−0.65 %** |
| `pe_directional_mc2` | 5 | 337.6 s | 337.7 s | **−0.01 %** |
| `pe_directional_p2` | 1 | 1181.8 s | 1202.8 s | −1.75 % |

Per-rep, sorted (seconds):

| shape | streamed | files |
|---|---|---|
| directional | 601.5 603.5 603.5 608.5 609.6 | 604.5 607.5 607.5 610.5 614.5 |
| non-directional | 603.6 610.7 613.7 614.7 616.7 | 611.7 615.7 617.8 618.8 619.8 |
| `--multicore 2` | 333.6 335.6 337.6 339.6 340.7 | 334.6 334.7 337.7 338.7 340.7 |

**C3 — PASS. Streaming is never slower, and slightly faster on two of three shapes.** The
non-directional distributions do not overlap at all: every streamed rep beats every files rep.
`--multicore 2` is a dead heat (0.01 %, distributions interleaved), which is what a genuine
null looks like and is a useful control — it says the ~0.65 % elsewhere is not an artefact of
the harness favouring one arm.

**This overturns §4.3's regression.** The single 10M non-directional run had the streamed arm
2.4 % slower. With five reps at 2M the same shape is 0.65 % *faster*, 5/5. One of the two is
scale-dependent and the other is a fluke; n=1 could not tell them apart, which is the entire
argument for the rep matrix. §4.6 settles it with full-scale reps.

Until then the honest statement is: **no regression is established on any shape**, and the
one measurement that suggested otherwise has not reproduced.

**C1 at 2M — 16 further pairs, all identical.** Every rep of every shape:
`pe_directional_p4` ×5, `pe_nondirectional_p4` ×5, `pe_directional_mc2` ×5,
`pe_directional_p2` ×1. Combined with the three at 10M, **19 arm-pairs are byte-identical**
with no exceptions. Note the record counts are stable across reps within a shape
(2,398,276 directional; 2,398,274 non-directional), so the runs are deterministic in both arms.

### 4.6 `-p 2`, the least-slack shape — 4 reps at 2M (2026-09-12)

`FULLSCALE.md` §4 wanted `-p 1` here on the reasoning that "the fan-out has the least slack".
Bismark rejects `-p 1`, so this is `-p 2`: two instances of two threads, the fewest the tool
allows, and the configuration where a bounded channel has the least room to absorb a rate
difference.

| | median | reps (sorted, s) |
|---|---|---|
| **streamed** | **1181.8 s** | 1169.7 · 1181.8 · 1181.8 · 1185.8 |
| **files** | 1199.8 s | 1190.7 · 1196.8 · 1202.8 · 1205.7 |

**−1.50 %, and the distributions do not overlap**: the slowest streamed rep (1185.8 s) beats
the fastest files rep (1190.7 s). This is the cleanest separation in the whole matrix.

**It is also the opposite of what the brief expected.** §4 nominated this shape as the one
where streaming was most likely to hurt. It is the shape where streaming helps most — 1.50 %
against 0.66 % at `-p 4` and 0.01 % under `--multicore 2`. The ordering across shapes is
consistent: the fewer threads each aligner instance has, the more streaming wins.

A plausible reading, offered as a hypothesis and not a measurement: with fewer threads the
aligners are slower, so the single converter thread keeps every consumer fed and the channel
never becomes the constraint, while the file arm still pays to write a gigabyte and read it
back. But this run does not isolate that, and the effect is small enough that it does not need
explaining to be reported.

### 4.7 The §4.3 regression does not reproduce — retracted (2026-09-12)

`pe_nondirectional_p4` at 10M pairs, now three reps per arm:

| | median | reps (sorted, s) |
|---|---|---|
| **streamed** | **2972.0 s** | 2958.1 · 2972.0 · **3045.2** |
| **files** | 2988.3 s | 2973.2 · 2988.3 · 3000.4 |

**−0.55 %.** Two of the three streamed reps beat every files rep. The 3045.2 s that produced
§4.3's "2.4 % slower" is the slowest of the three and sits 87 s above the next streamed rep,
against a files spread of 27 s. It was an outlier.

**§4.3's C3 conclusion is withdrawn.** There is no non-directional regression. The C1 and C2
results in that section stand — those are exact comparisons and unaffected by timing noise.

This is worth keeping as a record of how nearly a wrong conclusion got published. A single
full-scale run showed a 2.4 % regression with a coherent-sounding mechanism attached — four
consumers sharing two converted streams, less slack in the fan-out — and that story survived
because it was plausible, not because it was tested. Reps killed it. The mechanism may still
be real; it simply is not worth 2.4 % of wall time.

### 4.8 C3 across every shape and scale

| shape | scale | n | delta (streamed vs files) |
|---|---|---|---|
| `pe_directional_p2` | 2M | 4 | **−1.50 %** |
| `pe_directional_p4` | 2M | 5 | −0.66 % |
| `pe_nondirectional_p4` | 2M | 5 | −0.65 % |
| `pe_directional_mc2` | 2M | 5 | −0.01 % |
| `pe_directional_p4` | 10M | 2 | −0.45 % |
| `pe_nondirectional_p4` | 10M | 3 | −0.55 % |
| `pe_directional_mc2` | 10M | 1 | −0.50 % |

**Streaming is not slower anywhere.** Every measurement is negative or zero, at both scales,
on every shape. The largest effect is on the least-parallel shape and the smallest is under
`--multicore`, and the 2M and 10M figures agree with each other on the two shapes measured at
both.

The honest headline remains **wall-neutral to slightly faster** — the deltas are under 1 % on
most shapes and this is a benchmark, not a proof. But the pre-merge concern was a *regression*
from coupling the aligner instances, and there is no evidence of one.

### 4.9 Final tally

Benchmarking ended 2026-09-12 07:36 UTC.

| | |
|---|---|
| benchmark runs | **52**, every one serial on an otherwise idle machine |
| C1 arm-pairs compared | **25** — 0 differing |
| shapes | 4 (`-p 2`, `-p 4`, `--non_directional`, `--multicore 2`) |
| scales | 2,000,000 and 10,000,000 read pairs |
| §7 probes | 3, all pass |

`pe_directional_p4` at 10M finished at n=3: streamed 2943.2 · 2946.8 · 2974.4 s against files
2955.7 · 2960.0 · 2961.1 s, median **−0.45 %**. Worth noting the third streamed rep (2974.4 s)
is slower than every files rep — the per-rep spread is comparable to the effect being measured,
which is the honest limit on how hard a sub-1 % claim can be pushed from three reps. The
direction is consistent across seven shape/scale groups; the magnitude on any one of them is
not.

**Deliberately not run**, for time: further full-scale reps (the schedule was stopped early to
protect the write-up), and the human WGBS accession `SRR24827373` named in `FULLSCALE.md` §3.
Neither is needed for the three claims; both would strengthen the C3 magnitude.

## Full-scale validation — all three claims pass

Ran [`FULLSCALE.md`](https://github.com/ewels/Bismark/blob/rust/stream-converted/plans/09112026_stream-converted/fullscale/FULLSCALE.md) on a 32-core / 124 GB machine: **54 benchmark runs, strictly one at a time**, `SRR24766921` (mouse RRBS PE, 51 bp) against Ensembl 112 GRCm39.

| | claim | result |
|---|---|---|
| **C1** | output byte-identical | ✅ **26 arm-pairs, 0 differing** |
| **C2** | converted temp files gone | ✅ **0 bytes** vs up to 6.38 GiB |
| **C3** | no wall-time regression | ✅ **never slower**, −0.01 % to −1.50 % |

---

### Disk, CPU, memory and output over one full-scale run

Directional PE, 10M pairs, ~49 minutes per arm. Blue is streaming (the new default), red is `--no_stream_converted`.

![disk, CPU, memory and output](https://raw.githubusercontent.com/ewels/Bismark/rust/stream-converted/plans/09112026_stream-converted/fullscale/plots/summary/hero_directional_10M.png)

Four things in one picture:

- **Disk** — the file arm puts 3.19 GiB of converted reads down in the first 33 seconds and *holds it for the entire run*. Streaming never writes a byte.
- **CPU** — the file arm runs at 800 % for 29 minutes, then one strand hypothesis finishes and it spends the last 20 minutes at 400 %. Streaming holds a flat ~650 % and both instances finish together. Same total CPU either way (18,990 vs 18,993 CPU-seconds).
- **Memory** — streaming is **flat at 8.73 GiB**; the file arm **climbs to 12.05 GiB**, with another ~4 GiB of page cache above it (dashed) for its own converted files.
- **Output** — identical, as it must be.

---

### Disk: the headline

![converted bytes on disk](https://raw.githubusercontent.com/ewels/Bismark/rust/stream-converted/plans/09112026_stream-converted/fullscale/plots/summary/c2_summary.png)

Zero, not "small". Peak sampled *while the run is live* — both arms clean up at the end, so measuring afterwards reports zero for both and proves nothing.

<details>
<summary><b>The case this feature exists for — a <code>--temp_dir</code> that cannot hold the converted reads</b></summary>

<br>

`FULLSCALE.md` §7.2 notes this is *"the whole point of the feature, and nothing has demonstrated it yet"*. A 512 MiB tmpfs as `--temp_dir`, against a converted set needing roughly twice that:

| arm | exit | records |
|---|---|---|
| files | **1** — `I/O error: No space left on device (os error 28)` | 0 |
| streamed | **0** | 2,398,276 |

The file arm dies. The streamed arm completes with a full result set.

</details>

---

### Speed

![wall time](https://raw.githubusercontent.com/ewels/Bismark/rust/stream-converted/plans/09112026_stream-converted/fullscale/plots/summary/c3_summary.png)

Plotted relative to each group's files median, because the effects are under 2 % and absolute bars render both arms as identical rectangles.

| shape | scale | n | delta |
|---|---|---|---|
| `-p 2` | 2M | 4 | **−1.50 %** |
| `-p 4` | 2M | 5 | −0.66 % |
| `--non_directional` | 2M | 5 | −0.65 % |
| `--non_directional` | 10M | 3 | −0.55 % |
| `--multicore 2` | 10M | 1 | −0.50 % |
| `-p 4` | 10M | 4 | −0.43 % |
| `--multicore 2` | 2M | 5 | −0.01 % |

**Negative or zero in all seven groups.** The honest headline is *wall-neutral to slightly faster* — but the pre-merge concern was a **regression** from coupling the aligner instances, and there is no evidence of one.

<details>
<summary><b>How hard these numbers can be pushed (please read before quoting a percentage)</b></summary>

<br>

The `-p 4` 10M group has **overlapping distributions**: streamed 2937.2 · 2943.2 · 2946.8 · 2974.4 s against files 2938.9 · 2955.7 · 2960.0 · 2961.1 s. The per-rep spread exceeds the 12.9 s median difference. **Four reps do not establish a 0.43 % effect there.**

What the matrix *does* support is the claim C3 makes — not slower. Every median across seven independent groups is negative, and the two tightest (`-p 2` at −1.50 %, non-directional 2M at −0.65 %) have no overlap at all. A consistent sign across seven groups is strong; the magnitude on any single group is not.

The `--multicore 2` dead heat at −0.01 % is a useful control: fully interleaved distributions are what a genuine null looks like, which argues the rest isn't the harness quietly favouring an arm.

C1 and C2 are exact comparisons and carry no such caveat.

</details>

<details>
<summary><b>The instance-asymmetry question (#1120 D-6) — closed</b></summary>

<br>

`BENCHMARKS.md` closed on this as the known measurement gap: a tee couples the aligner instances, so a fast one can be throttled behind a slow one, and no fixture ran long enough to show whether that costs anything.

On directional 10M:

| | CPU profile | instances finish |
|---|---|---|
| **files** | 800 % until t=1761 s, then **400 % for the remaining 1194 s** | 20 minutes apart |
| **streamed** | flat **645 %** throughout | together, t=2944 s |

The coupling is **real and large** — with files the strand hypotheses fully decouple and the machine spends the last 40 % of the run at half utilisation. Streaming paces the fast instance to the slow one, which is exactly the throttling that was the concern.

**And it costs nothing.** Total CPU 18,990 vs 18,993 CPU-seconds; the streamed arm finished 8.9 s sooner. Wall time is set by the slowest instance in both arms — streaming doesn't slow that instance, it declines to let the fast one race ahead and then idle. Same work, spread evenly instead of front-loaded.

**`CHANNEL_DEPTH` was not touched and does not need raising on this evidence.**

</details>

---

### Memory

![peak memory](https://raw.githubusercontent.com/ewels/Bismark/rust/stream-converted/plans/09112026_stream-converted/fullscale/plots/summary/memory_summary.png)

Peak **allocation** (page cache excluded), lower streamed in six of seven groups and tied in the seventh. Biggest gap is `--multicore 2` at 10M: 14.95 vs 18.09 GiB — two concurrent chunks each reading back their own converted files is where the file path costs most.

<details>
<summary><b>A measurement trap worth knowing about</b></summary>

<br>

cgroup `memory.current` counts page cache, so the file arm's *total* runs 6+ GiB above the streamed arm purely because its own converted files are cached. Read unqualified, that would trip `FULLSCALE.md` §2's tripwire — *"if peak RSS moves by more than a few hundred MB, something is wrong"* — and manufacture a failure that isn't one.

The harness records `memory.stat anon` separately for this reason. Every memory figure quoted here is allocation, not cache. The dashed lines in the first figure are the totals, for comparison.

</details>

---

<details>
<summary><b>Method</b></summary>

<br>

- **One benchmark at a time**, behind a lockfile. Never two arms, never two shapes, nothing else on the box.
- **Each arm in its own container**, so cgroup v2 counters (`memory.current`, `memory.peak`, `cpu.stat`, `pids.current`) cover the whole process tree including every `bowtie2` child, starting from zero — not the parent's RSS.
- **Page cache warmed identically before every run** (13 GB index into 124 GB RAM), so no run is penalised for faulting it in.
- **Arm order alternated between reps** — a fixed order hands any residual first-run penalty to the same arm every time.
- **~1 s time series per run**: converted bytes, whole temp dir, output, memory (total and anonymous), CPU, threads. The sample loop costs ~16 ms per tick, so each sample's timestamp is honest — an earlier version spent over a second per tick, which is fine for a peak and fatal for a curve whose whole content is *when* the bytes appear.
- **C1 compares `samtools view` record bodies**, not raw BAM: the header's `@PG CL:` line is the verbatim argv and differs between arms by design. Reports compared with absolute paths filtered.

Harness, raw time series, per-run plots and the full record: [`fullscale/`](https://github.com/ewels/Bismark/tree/rust/stream-converted/plans/09112026_stream-converted/fullscale) · summary in [`BENCHMARKS.md` §3](https://github.com/ewels/Bismark/blob/rust/stream-converted/plans/09112026_stream-converted/BENCHMARKS.md).

</details>

<details>
<summary><b>Section 7 — the edge cases, all pass</b></summary>

<br>

**7.1 — long sample names under `--multicore`.** FIFO names cap the stem at `NAME_STEM_CAP` (64 bytes) and uniqueness comes from a process-wide counter rather than the name, so a collision would surface as interleaved or truncated output rather than an error. Checked by comparing alignment columns against a short-named control, not by exit code:

| run | basename | exit | records |
|---|---|---|---|
| control | 26 chars | 0 | 2,398,276 |
| long name | **203 chars**, `--multicore 4` | 0 | 2,398,276 |

Alignment-identical. The fix that had unit tests but had never run at scale holds.

**7.3 — thread and FD counts**, under the widest fan-out (`--non_directional --multicore 2`, 8 pipes concurrently): peak **73** open FDs against a 65,535 soft limit (0.1 %), 35 threads, 9 processes. Three orders of magnitude of headroom.

</details>

<details>
<summary><b>What did not reproduce, and what is not covered</b></summary>

<br>

**A 2.4 % non-directional regression — retracted.** The first 10M non-directional run had the streamed arm 72 s slower, and a plausible mechanism was available (four consumers sharing two converted streams, less slack in the fan-out). With three reps the same shape is **0.65 % faster**; the original run was the slowest of three, sitting 87 s above the next streamed rep against a files spread of 27 s. An outlier. It is left in [`RESULTS.md`](https://github.com/ewels/Bismark/blob/rust/stream-converted/plans/09112026_stream-converted/fullscale/RESULTS.md) §4.3 → §4.7 rather than edited away, because the story survived on plausibility rather than evidence until repetition killed it.

**Not covered:**
- **Combined-index modes**, `--hisat2` and `--rammap_subprocess` keep files by design and are unchanged by this PR (`PROGRESS.md` D-11).
- **One dataset, one genome, one machine.** The human WGBS accession `SRR24827373` named in `FULLSCALE.md` §3 was not run.
- **A clean A/B is not evidence about edges.** Three defects in this work were invisible to the 14-shape container matrix; §7 probes three specific edges, it does not exhaust them.

</details>

<details>
<summary><b>One defect in the brief: <code>-p 1</code> is not a runnable shape</b></summary>

<br>

`FULLSCALE.md` §4 lists *"Directional PE, `-p 1` — the fan-out has the least slack here"* as one of four shapes to cover. Bismark rejects it:

```
error: Please select a value for -p of 2 or more!
```

`aligner/options.rs:165`, matching Perl 7993-8007. Both arms exited 1 in about a second. Substituted **`-p 2`**, the least-slack configuration that actually runs.

Worth noting the brief's reasoning was also inverted by the result: it nominated the fewest-threads shape as the one where streaming was *most likely to hurt*. It is the shape where streaming helps **most** (−1.50 %). The ordering is consistent — the fewer threads each aligner instance has, the more streaming wins.

</details>

---

**Recommendation: merge.** No knob needs turning.

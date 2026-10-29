# FULL-SCALE VALIDATION — converted-read streaming (#1120)

**Plan:** [`PLAN.md`](./PLAN.md) · **Progress:** [`PROGRESS.md`](./PROGRESS.md) · **Benchmarks:** [`BENCHMARKS.md`](./BENCHMARKS.md)
**PR:** [#1121](https://github.com/FelixKrueger/Bismark/pull/1121) · **Issue:** [#1120](https://github.com/FelixKrueger/Bismark/issues/1120)
**Written:** 2026-09-11 · **Status:** not yet run

A handover for a fresh session with a large machine. Read this file first. Read
`PROGRESS.md` next, for the decisions log. You do not need to re-derive anything from the
source.

---

## 1. What this run has to answer

The PR makes Bismark hand the converted reads to the aligner through named pipes instead of
writing them to `--temp_dir`. Three claims need testing at a scale the small fixture cannot
reach.

| # | Claim | How it fails |
|---|---|---|
| **C1** | Output is byte-identical to the file path | any BAM record or report number differs |
| **C2** | The converted temp files are gone | any `_C_to_T` / `_G_to_A` file appears in `--temp_dir` |
| **C3** | Wall time does not regress | the streamed arm is materially slower than the file arm |

C3 is the real unknown. C1 and C2 are proven on 5,000 read pairs across 14 shapes; what a
big run adds is **natural instance asymmetry**. Bismark runs 2 or 4 aligner instances that
share one conversion pass through a bounded channel, so an instance that falls behind can in
principle throttle its siblings. On a small fixture nothing runs long enough to diverge.

> **Correcting an earlier framing.** `PROGRESS.md` D-6 calls this "asymmetric-consumer
> benchmarking" and the research harness simulated it with one instance at `-p 1` beside one
> at `-p 4`. **You cannot reproduce that in production**: `-p N` applies to every instance.
> The asymmetry that actually occurs is the natural rate difference between strand
> hypotheses, which happens on every run. So there is no special experiment to design. A
> full-scale streamed-vs-files comparison measures it directly.

---

## 2. Where things stand going in

Measured on 200,000 read pairs (93 MB uncompressed), E. coli, `-p 4`, 7 interleaved reps:

| | wall, median | peak converted bytes in `--temp_dir` |
|---|---|---|
| Files (`--no_stream_converted`) | 6.35 s | 96 MB |
| Streamed (default) | 6.33 s | 4 KB |

So the expectation is **wall-neutral, large disk win**. An earlier projection of ~10% faster
did not reproduce and is superseded; see `BENCHMARKS.md` section 1.

Memory should barely move. The fan-out holds `CHANNEL_DEPTH` (4) × `BLOCK_BYTES` (256 KiB) ≈
1 MiB per consumer, so at most ~8 MiB for non-directional paired-end, against a multi-GB
index. If peak RSS moves by more than a few hundred MB, something is wrong.

---

## 3. What you need

**Data.** The repo names its benchmark datasets but ships no fetch script and no data. The
paths in `rust/justfile` (`/Users/benjamin/bismark_benchmarks/...`) are a maintainer's local
machine; every recipe takes overrides.

| Dataset | Accession | Genome | Note |
|---|---|---|---|
| Mouse RRBS, paired-end | `SRR24766921` | GRCm39 | **start here** — what every existing aligner benchmark used, as `SRR24766921_10M_{1,2}.fastq.gz` (first 10M pairs) |
| Human WGBS, paired-end | `SRR24827373` (GSM7445361) | GRCh38 | bigger, slower; use if you want a second point |

Both are plain SRA accessions. Verify they are still public before planning around them.

**Genome.** Prepare once, and reuse for both arms:

```sh
bismark_genome_preparation --bowtie2 --parallel <N> /path/to/GRCm39
```

This is the slow part of setup. Do it before anything else.

**Tools.** `bowtie2`, `samtools`, and a release build of this branch:

```sh
cd rust && cargo build --release -p bismark --bins
```

---

## 4. The comparison

Run the **same binary** twice on the **same input**, changing only one flag.

```sh
# arm A — streamed (the new default)
bismark --genome $GENOME -1 $R1 -2 $R2 -p 4 -o out_stream --temp_dir tmp_stream

# arm B — files (the previous behaviour)
bismark --genome $GENOME -1 $R1 -2 $R2 -p 4 -o out_files --temp_dir tmp_files \
        --no_stream_converted
```

Give each arm **its own `--temp_dir`**, or the peak-disk measurement is meaningless.

Cover at least these shapes. Directional and non-directional differ in the property under
test: 2 converted streams and 4 pipes versus 4 streams and 8 pipes.

1. Directional PE, `-p 4`
2. Non-directional PE (`--non_directional`), `-p 4`
3. Directional PE, `-p 1` — the fan-out has the least slack here
4. Directional PE, `--multicore 2` — concurrent chunks, each with its own pipe set

---

## 5. How to measure, and the traps

**Wall, CPU and peak RSS** — use `/usr/bin/time -v` per arm. `scripts/bench_run.sh` already
does this for the extractor and is worth reading for the field list, though it is not
directly reusable here.

**Interleave the arms, and use at least 5 reps.** Running all of arm A then all of arm B
hands any thermal or noisy-neighbour drift to one arm. `spikes/bench-ab.sh` interleaves; copy
that shape.

**Peak `--temp_dir` usage must be SAMPLED while the run is in flight.** Both arms delete
their temp files at the end, so measuring afterwards reports zero for both and proves
nothing. `spikes/bench-ab.sh` samples `du -sk` at 0.2 s intervals against the live PID.

**Compare `samtools view` records, not raw BAM bytes.** The BAM header's `@PG CL:` line is
the verbatim argv, which necessarily differs between the arms (one carries
`--no_stream_converted`). The record bodies carry no paths and must match exactly:

```sh
samtools view out_stream/*.bam > a.sam
samtools view out_files/*.bam  > b.sam
cmp a.sam b.sam
```

**Filter absolute paths out of the report before diffing.** The output and temp directories
differ by design. `rust/justfile`'s `aligner-oracle` recipe shows the exact filter used
elsewhere in this repo.

**Assert the streamed arm's temp dir is empty at the end**, and that no `_C_to_T`,
`_G_to_A` or `*.fifo.*` entry survives.

---

## 6. Pass criteria

- **C1** — `samtools view` output identical, and the filtered report identical, for every
  shape. Any difference is a hard fail; capture the diff and stop.
- **C2** — peak sampled `--temp_dir` on the streamed arm is a few KB (the FIFO inodes), not a
  fraction of the input size. The file arm should show roughly one uncompressed copy of the
  input per converted set, which is also the number worth quoting.
- **C3** — streamed median wall within a few percent of the file arm. A consistent regression
  beyond that is the asymmetry question showing up; `CHANNEL_DEPTH` in
  `rust/bismark/src/aligner/stream.rs` is the single knob, and raising it trades memory for
  slack.

---

## 7. Worth probing, since only a big run can

1. **Long sample names with `--multicore`.** FIFO names are capped at 64 bytes of stem
   (`NAME_STEM_CAP`), and path uniqueness comes from a process-wide set counter rather than
   the name. That fix has unit tests but has never run at scale. Symlink the input to a
   ~200-character name and run `--multicore 4`. Expect a normal run; a collision would show
   as interleaved or truncated output rather than an error.
2. **A genuinely constrained `--temp_dir`.** Point it at a small filesystem — smaller than
   the uncompressed input — and confirm the streamed arm completes where the file arm fails.
   That is the whole point of the feature, and nothing has demonstrated it yet.
3. **Thread and FD counts.** Streaming adds one converter thread per source and one writer
   thread per consumer (up to 12 threads for non-directional PE), plus one FD per pipe.
   Confirm nothing approaches a ulimit under `--multicore`.

---

## 8. What is NOT in scope

- **Combined-index modes** (`--combined_index*`) still write converted files by design, and
  say so on STDERR. They are unchanged by this PR. See `PROGRESS.md` D-11.
- **`--hisat2`** and **`--rammap_subprocess`** likewise keep files.
- **Perl parity.** This PR's gate is Bismark-vs-Bismark. The existing Perl oracle is
  unaffected and already runs in CI.

---

## 9. Record the result

Add a section to `BENCHMARKS.md` in this directory, in the shape of section 2 there: the
machine, the dataset, the shapes, a median table, and an explicit statement of what did and
did not reproduce. If C3 fails, say so plainly and record the `CHANNEL_DEPTH` value tried.

Then update the pipeline table row 16 in `PROGRESS.md`, and comment on
[#1121](https://github.com/FelixKrueger/Bismark/pull/1121).

---

## 10. One caution from the small-scale work

Three separate defects in this PR were invisible to the 14-shape container matrix
(`spikes/ab-run.sh`): an aligner that exits without reading its input, a FIFO name collision
under `--multicore`, and a clippy failure behind a feature flag. In each case real aligners on
short fixture names behaved perfectly.

The matrix proves byte-identity on realistic input. It does not probe edges. Treat a clean
full-scale A/B the same way: it is strong evidence for C1 to C3, and it is not evidence about
anything in section 7 unless you deliberately set that case up.

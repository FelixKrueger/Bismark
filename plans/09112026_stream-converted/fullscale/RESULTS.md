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

_Populated as each phase completes._


# Phase 5 — §9 #18 real-data byte-identity gate (oxy/Linux) — ✅ PASSED 2026-06-01

**Verdict: BYTE-IDENTICAL.** The Rust `bismark_rs` SE-directional output is byte-identical to Perl Bismark
v0.25.1 (decompressed SAM content), on real human WGBS reads against GRCh38, run on Linux.

## Environment (oxy `dockyard-oxy-0`)
- Perl **Bismark v0.25.1**, **Bowtie 2 2.5.5** (`bowtie2-align-s version 2.5.5`), **samtools 1.23.1**
  (micromamba env `bismark-test`).
- Rust `bismark_rs` built **on oxy** (`cargo 1.96.0`, `--release`) from the uncommitted `rust/aligner`
  worktree (source shipped via `tar | dcli ssh` — no commit/push).
- Genome: `~/bismark_benchmarks/genome` = GRCh38 (`Homo_sapiens.GRCh38.dna.primary_assembly.fa` + the
  `Bisulfite_Genome` Bowtie 2 index, both S3-backed symlinks).
- Reads: `~/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz` (real directional WGBS SE), subset to
  the first N reads.

## Method
Both binaries invoked with an **identical argv** → the reconstructed Bismark `@PG CL:` matches:
```
<bin> --genome <genome> reads.fq --output_dir <out> --temp_dir <tmp>
```
Run sequentially into the same `--output_dir` (Perl BAM moved aside before the Rust run, so no collision and
the CL is identical). Gate (P1 — normalise the samtools `@PG` out of both sides):
```
diff <(samtools view -h perl.bam | grep -vE '^@PG.*ID:samtools') \
     <(samtools view -h rust.bam | grep -vE '^@PG.*ID:samtools')
```
Script: `/var/tmp/aligner_gate/run_gate.sh <N>` (built on oxy for this gate).

## Results

| Reads (N) | Perl records | Rust records | `samtools view -h` diff | Verdict |
|-----------|--------------|--------------|--------------------------|---------|
| 10,000 | 8,402 | 8,402 | empty (8,598 header+record lines) | ✅ BYTE-IDENTICAL |
| 1,000,000 | 848,124 | 848,124 | empty (848,320 header+record lines) | ✅ BYTE-IDENTICAL |

- The 10k run surfaced `could-not-extract genomic: 1` (a real chromosome-edge read) — counted in
  `unique_best`, in **no** strand bucket, and **not written** — and the record counts still match Perl
  exactly, confirming the three-counter edge-guard design on real data.
- The 1M run exercised ~848k mapped reads incl. thousands of indel/deletion/soft-clip reads (the
  `make_mismatch_string` deletion path + the minus-strand revcomp + the `@SQ`/`@PG`/`@HD` header) — all
  byte-identical. The Bismark `@PG CL:` line matched (identical argv); only the samtools-pipe `@PG` was
  normalised out (P1).
- The env's `bowtie2-align-s 2.5.5` was observed running with exactly the Rust port's assembled options
  (`-q --score-min L,0,-0.2 --ignore-quals --norc/--nofw`), confirming `aligner_options` parity.

## Scope / notes
- This is the **Phase-5 gate** (SE-directional WGBS). The **full-scale** gate (full WGBS SE + PE + RRBS) is
  Phase 10.
- Adjudicated on **Linux** (oxy), per the "byte-identity adjudicated on Linux, never macOS" rule.
- The oxy workdir `/var/tmp/aligner_gate` was removed after capturing results (shared box; a sibling c2c
  gate was running concurrently — the SE gate is single-threaded-per-instance and low-contention).

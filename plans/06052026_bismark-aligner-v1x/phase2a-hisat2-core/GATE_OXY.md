# GATE_OXY — Phase 2a HISAT2 wrapper core, SE byte-identity gate (V9)

- **Date:** 2026-06-05 · **Box:** oxy `dockyard-oxy-0` (cgroup 32c/256G; node advertises 128c/991G).
- **Oracle / pins:** Perl Bismark **v0.25.1** + **HISAT2 2.2.2** (`hisat2-align-s version 2.2.2`) + samtools 1.23.1, env `~/micromamba/envs/bismark-test`.
- **Binary:** `bismark_rs` (release) built ON oxy from the uncommitted `rust/aligner-v1x` worktree (`cargo 1.96.0`, `/var/tmp/aligner_2a`).
- **Genome:** real GRCh38 `~/bismark_benchmarks/genome` (`.ht2` 8-suffix index present); reads `~/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz`.
- **Harness:** `phase2a_hisat2_se_gate.sh` (this dir). Identical argv into the SAME `-o` (Perl moved aside), diff of **decompressed SAM** (`samtools view -h`, `ID:samtools` @PG filtered) + the **report** (wall-clock line filtered). The basename match between Perl/Rust outputs IS the naming-token check.

---

## Verdict: ✅ PASS

**Single-core `--hisat2` is byte-identical to Perl v0.25.1 + HISAT2 2.2.2** across SE {directional, non-directional, pbat} + FastA SE, at 10k and (directional/non-dir) 1M. **`--multicore`/`--parallel` + `--hisat2` is correctly hard-rejected** (a byte-identity finding — see below).

### Single-core cells — byte-identical (decompressed SAM + report)

| Cell | argv delta | N=10k | N=1M |
|------|-----------|-------|------|
| `se_dir` | (directional, default) | ✅ 8360 rec | ✅ **844,267 rec** |
| `se_nondir` | `--non_directional` | ✅ 8362 rec | ✅ **843,765 rec** |
| `se_pbat` | `--pbat` | ✅ 49 rec | — (pbat on directional data lands ~0 complementary; 10k suffices) |
| `se_fasta` | `-f` (FastA SE, directional) | ✅ 8358 rec (`se.fa_bismark_hisat2.bam`) | — |

Every cell: BAM decompressed-SAM byte-identical (`@PG ID:samtools` filtered) **and** `_bismark_hisat2_SE_report.txt` identical modulo the wall-clock line. Output naming token = `hisat2` (the basename match Perl↔Rust confirms it). The `se_dir` 10k count **8360** matches the Phase-1 spike exactly.

### Multicore cell — `--multicore`/`--parallel` + `--hisat2` HARD-REJECTED

`bismark_rs --hisat2 --parallel 8 …` exits 1 with *"--multicore/--parallel is not supported with --hisat2: HISAT2 discovers splice sites across the whole input read set …"*. Verified on oxy (`se_multicore_reject: PASS`).

---

## The multicore finding (why the reject)

The plan assumed `--multicore` is aligner-agnostic (it is for Bowtie2). The gate proved it is **not** for HISAT2. Diagnostic at N=1M (order-normalized `samtools view | LC_ALL=C sort | md5sum`):

| Variant | records | spliced (N-CIGAR) | sorted-body md5 |
|---------|--------:|------------------:|-----------------|
| Perl single-core | 844267 | 1310 | `c2c020…` |
| Rust single-core | 844267 | 1310 | `c2c020…` ✅ **== Perl** |
| Rust `--parallel 1` | 844267 | 1310 | `c2c020…` ✅ |
| Perl `--multicore 8` | 844305 | 1219 | `c32366…` |
| Rust `--parallel 8` | 844256 | 1225 | `e2a9d5…` |

- **HISAT2 discovers splice sites globally across the whole input read set**, then aligns using that set. Splitting the input into chunks changes the discovered splice sites → spliced reads align differently (observed: `31M3949N33M` ↔ `64M` at a different locus for the same read).
- **Perl itself is not worker-invariant here**: Perl single-core (1310 spliced) ≠ Perl `--multicore 8` (1219 spliced). So this is HISAT2's architecture, *not* a Rust bug.
- Rust's contiguous-chunk split (Phase 9b, worker-invariant for read-independent Bowtie2) ≠ Perl's `--multicore` split strategy, so even Rust-p8 ≠ Perl-mc8 (844256 vs 844305).
- **Conclusion:** there is no byte-identical multicore target for HISAT2 — the Phase-9b worker-invariance guarantee holds only for read-independent aligners. 115/207 of the differing diff lines carry an `N` CIGAR (spliced), confirming the mechanism.

**Resolution (Felix, 2026-06-05): hard-reject `--multicore`/`--parallel N>1` + `--hisat2`** (`config.rs`, mirrors the prior `--ambig_bam`+multicore reject which it subsumes). Single-core `--hisat2` — the byte-identical faithful path — is unaffected. Matching Perl's `--multicore` exactly (still non-worker-invariant) or accepting concordance-only multicore are possible future paths, out of 2a scope.

---

## Reproduce
```
# on oxy (env on PATH):
bash /var/tmp/aligner_2a_gate.sh 10000            # all cells at 10k
bash /var/tmp/aligner_2a_gate.sh 1000000 se_dir se_nondir   # 1M scale
# multicore reject is asserted by the se_multicore cell.
```
Logs: `/var/tmp/aligner_2a_gate_10k.log`, `/var/tmp/aligner_2a_gate_1M.log` (oxy `/var/tmp` is ephemeral — this doc is the durable record). The PE path + its `ZS` read-1 asymmetry, and any non-dir/pbat at 1M, are Phase 2b.

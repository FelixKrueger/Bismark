# GATE_OXY — Phase 2b HISAT2 paired-end, PE byte-identity gate (V8)

- **Date:** 2026-06-05 · **Box:** oxy `dockyard-oxy-0` (cgroup 32c/256G).
- **Oracle / pins:** Perl Bismark **v0.25.1** + **HISAT2 2.2.2** + samtools 1.23.1, env `~/micromamba/envs/bismark-test`.
- **Binary:** `bismark_rs` (release) built ON oxy from the uncommitted `rust/aligner-v1x` worktree (incl. the read-1 `ZS` mask + the dovetail fix), `/var/tmp/aligner_2a`.
- **Genome:** real GRCh38; reads `~/bismark_benchmarks/10M_PE/directional_10M_R{1,2}_val_{1,2}.fq.gz`.
- **Harness:** `phase2b_hisat2_pe_gate.sh` (this dir). Identical argv into the same `-o` (Perl moved aside); diff of **decompressed SAM** (`samtools view -h`, `ID:samtools` @PG filtered) + `_PE_report.txt` (wall-clock filtered) + the gzipped `--unmapped`/`--ambiguous` aux (decompressed). Single-core (multicore+HISAT2 rejected, 2a).

---

## Verdict: ✅ PASS — PE `--hisat2` byte-identical to Perl v0.25.1 + HISAT2 2.2.2

| Cell | argv | 10k | 1M |
|------|------|-----|-----|
| `pe_dir` | `-1 -2` (directional) | ✅ 16,032 rec | ✅ **1,620,342 rec** |
| `pe_nondir` | `--non_directional` | ✅ 16,034 | ✅ **1,620,302** |
| `pe_pbat` | `--pbat` | ✅ 24 | — (pbat lands ~0 on directional data) |
| `pe_fasta_dir` | `-f` | ✅ 16,034 | — |
| `pe_fasta_nondir` | `-f --non_directional` | ✅ 16,036 | — |
| `pe_ambig_dir` | `--ambig_bam --unmapped --ambiguous` | ✅ main 16,032 + **ambig 1,780** + 4 aux | ✅ main 1,620,342 + **ambig 173,420** + 4 aux |

Every cell: PE BAM decompressed-SAM byte-identical (`@PG ID:samtools` filtered) **and** `_PE_report.txt` identical modulo wall-clock. The `--ambig_bam` cell additionally byte-matched the **raw-aligner-passthrough** ambig BAM (`output.rs build_raw_record`, FLAG/RNEXT/PNEXT/TLEN verbatim — the only PE path that bypasses Bismark reconstruction; review B's Critical) and all four `--unmapped`/`--ambiguous` `_1`/`_2` aux files (decompressed). Naming token = `hisat2` (`_bismark_hisat2_pe*`, basename match).

---

## 🔴 Gate finding: the PE TLEN `dovetail` bug (fixed)

The **read-1 `ZS` mask** (the planned 2b change) was correct — but the first gate run **failed** `pe_dir`/`pe_nondir` with a 12-line diff (~6 records each), reports identical. Diagnosis: the sole difference was the **TLEN sign** on **fully-overlapping pairs where both mates map to the same POS and read-1 is reverse (FLAG 83)** — e.g. read `SRR24827373.1175` (60M/60M at chr7:81287727): Perl gave read-1 TLEN `-60`, Rust `+60`.

**Root cause:** Perl line 8047 sets the `$dovetail` *variable* to `1` for **every** aligner (`unless $no_dovetail`); the `if($bowtie2)` at 8051 only gates whether `--dovetail` is *pushed to the aligner options*. The PE TLEN computation (8898/8946) keys the FLAG-83 sign on `$dovetail`. Rust derived its TLEN `dovetail` from `aligner_options.contains("--dovetail")` (correct for Bowtie 2) → **`false` for HISAT2** (2a suppresses the flag) → flipped TLEN on same-POS reverse-read-1 pairs. The faithful Bowtie 2 gate (even 84M pairs) never caught it: Bowtie 2 doesn't produce these exactly-overlapping pairs for these reads; HISAT2 does. Neither the plan (which declared TLEN "aligner-agnostic, reused unchanged") nor the dual code-review flagged it.

**Fix:** `RunConfig.dovetail = !cli.no_dovetail` (Perl's `$dovetail`, aligner-independent); `lib.rs` uses `config.dovetail` for the PE TLEN, not a scan of `aligner_options`. Bowtie 2 is a no-op (the two are equal there). Regression guard: `output.rs::pe_tlen_tree` gains the index-3 (FLAG 83) same-POS cases (dovetail true → `-11/+11`; false → `+11/-11`). After the fix, **all cells PASS at 10k and 1M** (above).

---

## Reproduce
```
# on oxy (env on PATH); binary at /var/tmp/aligner_2a/rust/target/release/bismark_rs:
bash /var/tmp/aligner_2b_gate.sh 10000                                  # all 6 cells
bash /var/tmp/aligner_2b_gate.sh 1000000 pe_dir pe_nondir pe_ambig_dir  # 1M scale
```
Logs: `/var/tmp/aligner_2b_gate_{10k,1M}.log` (oxy `/var/tmp` is ephemeral — this doc is the durable record). minimap2 (Phases 3–4) + the combined full-scale gate (Phase 5) remain.

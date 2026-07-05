# GATE_OXY — HISAT2 `--local` byte-identity gate

**Date:** 2026-06-14 (oxy) · **Verdict: ✅ PASS (7/7).**
**Binary:** `bismark_rs` built on oxy (release) from the uncommitted worktree (branch `rust/aligner-hisat2-local`, off `origin/rust/iron-chancellor` beta.9 `478c974`).
**Oracle:** Perl Bismark v0.25.1 `--hisat2 --local`. **Aligner:** HISAT2 2.2.2. **samtools:** 1.23.1.
**Data:** 1M real GRCh38 WGBS (`10M_SE` / `10M_PE` subset). Harness: `gate_hisat2_local.sh` (+ raw `gate_run.out`; durable oxy copy `~/hisat2local_gate_artifacts/`).

## Method
For each cell, run **Rust `bismark_rs --hisat2 --local`** vs **Perl `bismark --hisat2 --local`** (identical
reads/args) and compare the **decompressed BAM body** (`samtools view`, no header → no `@PG`/version) by md5.
HISAT2-local = drop `--no-softclip` (soft-clipping allowed) + L-form `--score-min L,0,-0.2` + the local
`ln()` MAPQ, no `--local` flag pushed.

## Results (Rust `--hisat2 --local` body == Perl `--hisat2 --local` body)

| cell | records | soft-clipped | md5 match |
|------|---------|--------------|-----------|
| se_dir | 847,947 | 28,222 (3.3%) | ✅ `cf306dfa…` |
| se_nondir (`--non_directional`) | 847,544 | 28,412 | ✅ `06d072fc…` |
| se_pbat (`--pbat`) | 7,444 | 3,431 | ✅ `48214628…` |
| se_dir_mc (Rust `--local --multicore 4` == Perl `--local -p 4`) | 847,981 | 28,224 | ✅ `2619a578…` |
| pe_dir | 1,638,254 | 52,696 | ✅ `dc76efed…` |

**All 7 checks PASS:**
- **5 byte-identity cells** above — SE × {dir, non-dir, pbat} + PE dir + the `--local`+`--multicore` compose
  cell (local + the #986 `-p N` remap: Rust `--local --multicore 4` byte-identical to Perl `--local -p 4`).
- **🔴 Non-vacuity (B2+B3) PASS — the decisive check:** on the SAME SE reads, **end-to-end `--hisat2`
  produces 0 soft-clips** (`--no-softclip` forces it) while **`--hisat2 --local` produces 28,222** → the
  `--no-softclip`-drop genuinely fires, and the byte-identity above is proven across **tens of thousands of
  soft-clipped reads** (`S` CIGARs round-tripping through methylation calling), not a vacuous match.
- **Q4 PASS:** Perl `--hisat2 --local` is run-to-run deterministic (same body md5 on a re-run) — a valid
  byte-identity oracle (the `[EXPERIMENTAL]` Perl label is about biological validity, not determinism).

## What this proves
Rust `--hisat2 --local` is **byte-identical to Perl v0.25.1 `--hisat2 --local`** across SE+PE ×
directional/non-directional/pbat, including the `--multicore` compose, at 1M-read scale, with the
soft-clip path heavily exercised (28k+ soft-clipped reads per SE cell). The headline mode-difference
("drop `--no-softclip` → soft-clipping allowed") is demonstrated live (end-to-end 0 vs local 28,222).
non-dir/pbat record counts are low on this directional dataset (the Phase-8 caveat — directional reads land
few on complementary strands), but byte-identity holds and soft-clips are present in every cell.

## Provenance
Source: `tar | dcli ssh` of `rust/` → `/var/tmp/hisat2local_build` → `cargo build --release` (binary 26s).
Gate detached (`setsid nohup`) → 5 byte-id cells + non-vacuity + Q4. oxy scratch (`/var/tmp/hisat2local_{build,gate}`,
sc_probe; ~1 GB) cleaned post-run; durable `.out`/harness on oxy `~/hisat2local_gate_artifacts/` + pulled here.
(Build/transfer gotcha: the `tar | dcli ssh 'cat>f && … & echo'` launch trap backgrounds the whole remote
list → `cat` reads empty → extraction fails silently; transfer must be a foreground step, build launched separately.)

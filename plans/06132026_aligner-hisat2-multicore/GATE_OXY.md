# GATE_OXY — HISAT2 `--multicore N` (Approach B-faithful) byte-identity gate

**Date:** 2026-06-14 (oxy) · **Verdict: ✅ PASS (7/7 cells byte-identical)**
**Binary:** `bismark_rs` built on oxy (release) from the uncommitted worktree (branch `rust/aligner-hisat2-multicore`).
**Oracle:** Perl Bismark v0.25.1 `--hisat2 -p N` (the B-faithful oracle — NOT single-core, NOT Perl
`--multicore N`). **Aligner:** HISAT2 2.2.2. **samtools:** 1.23.1. **Data:** 1M real GRCh38 WGBS
(`10M_SE` / `10M_PE` subset). Harness: `gate_hisat2_multicore.sh` (+ raw `gate_run.out`; durable oxy copy
`~/hisat2mc_spike_artifacts/`).

## Method

For each cell, run **Rust `bismark_rs --hisat2 --multicore N`** (the remap → ONE instance `-p N --reorder`)
vs **Perl `bismark --hisat2 -p N`** (matching N), identical reads, into separate dirs. Compare the
**decompressed BAM body** (`samtools view`, NO header → no `@PG`, no version line) by md5. `--reorder` +
single instance ⇒ in-order body is deterministic, so a plain md5 compare is exact.

## Results (Rust `--multicore N` body == Perl `-p N` body)

| Cell | N | records | spliced | rust md5 == perl md5 | matches spike `-p N`? |
|------|---|---------|---------|----------------------|------------------------|
| se_dir_p2 | 2 | 844,296 | 1307 | ✅ `f05ee7e5…` | ✅ (exact) |
| se_dir_p4 | 4 | 844,305 | 1303 | ✅ `3bc6b7da…` | ✅ (exact) |
| se_dir_p8 | 8 | 844,316 | 1298 | ✅ `bfbf38b0…` | ✅ (exact) |
| se_nondir_p4 | 4 | 843,807 | 1393 | ✅ `69907701…` | — |
| se_pbat_p4 | 4 | 5,615 | 432 | ✅ `6043a713…` | — |
| se_dir_ambig_p4 | 4 | 844,305 | — | ✅ main `3bc6b7da…` **+ ambig `e7d5802b…`** | — |
| pe_dir_p4 | 4 | 1,620,340 | — | ✅ `7b23af3b…` | — |

**All 7 cells byte-identical.** Notes:
- **SE directional N∈{2,4,8}:** the per-N route maps `--multicore N`→`-p N` correctly, and each cell's md5
  is **identical to the Phase-0 spike's Perl `-p N` md5** (cross-validation: Rust reproduces Perl `-p N`
  exactly, and the spike's per-N values are reproducible).
- **`--ambig_bam`:** BOTH the main BAM and the `.ambig.bam` are byte-identical — confirms the remap stays on
  the single-instance path (Perl's Bowtie-2-only multicore ambig temp machinery is never reached). [Verified
  by direct md5; the harness's auto-compare false-flagged this cell because the output **dir name**
  `r_se_dir_ambig_p4` contains the substring "ambig", which its `grep -v ambig` BAM-selector filter stripped
  — a cosmetic harness bug, not a data divergence. Confirmed manually: main+ambig both match.]
- **PE directional:** byte-identical (covers the layout-agnostic route for paired-end; closes the SE-only
  e2e narrowing flagged in code-review B-C1).
- **non-dir / pbat caveat:** the dataset is directional, so `--non_directional`/`--pbat` land few reads on
  the complementary strands (pbat → only 5,615 mapped) — same caveat as the faithful Phase-8 gate. The cell
  still proves byte-identity of the route + strand arithmetic at scale; the integration tests prove the
  strand routing on synthetic data.

## What this proves

Rust `--hisat2 --multicore N` is **byte-identical to Perl `--hisat2 -p N`** (decompressed BAM) across SE+PE,
directional/non-directional/pbat, and `--ambig_bam`, at 1M-read scale — the B-faithful gate. The result is
deterministic per N (and, per the spike, N-dependent — NOT single-core-equivalent, by HISAT2's nature). The
`@PG`/argv difference (Rust `--multicore N` vs Perl `-p N`) is header-only and excluded by the body compare;
the report echoes only `aligner_options` (`-p N --reorder`), identical on both sides.

## Build/run provenance
Source: `tar | dcli ssh` of `rust/` from the worktree → `/var/tmp/hisat2mc_build` → `cargo build --release
-p bismark-aligner` (binary `target/release/bismark_rs`). Gate run detached (`setsid nohup`) → 7 cells × 2
tools. oxy scratch (`/var/tmp/hisat2mc_{build,gate}`, ~1.1 GB) cleaned post-run; durable artifacts (the
`.out` logs + harness) kept on oxy `~/hisat2mc_spike_artifacts/` + pulled to this dir.

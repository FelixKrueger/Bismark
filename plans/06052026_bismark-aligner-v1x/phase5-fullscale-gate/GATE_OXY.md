# GATE_OXY — Phase 5 (v1.x) combined real-data gate (10M single-core strict)

**Date:** 2026-06-06 · **Box:** oxy `dockyard-oxy-0` (cgroup 32c/256G) · **Verdict: ✅ ALL 13 CELLS PASS.**

`bismark_rs` is **byte-identical** to Perl Bismark v0.25.1 driving the same pinned aligner, at **10M reads single-core**, across all three v1.x backends and both genomes. This completes the v1.x aligner epic (the HISAT2 + minimap2 backends are byte-identical at 10× the per-phase scale, with the minimap2 worker-invariance re-confirmed and a second genome covered).

## Setup / repro tuple
- **Rust:** `bismark_rs` Version 1.0.0-alpha.1, commit `21bac5d` (branch `rust/aligner-mm2` = iron-chancellor `49a1518` [HISAT2 SE+PE] + the Phase-4 minimap2 commit = the PR #950 payload). Built on oxy `cargo 1.96 --release`; copied to `/home/bismark_rs_p5` (recycle insurance).
- **Oracle / pins:** Perl Bismark **v0.25.1** · Bowtie 2 **2.5.5** · HISAT2 **2.2.2** · minimap2 **2.31-r1302** · samtools **1.23.1** (`~/micromamba/envs/bismark-test/bin`).
- **Genomes:** human GRCh38 (`~/bismark_benchmarks/genome`) + mouse GRCm39 (`~/bismark_benchmarks/RRBS_PE/genome`, `.ht2`+`.bt2` present).
- **Reads:** first 10,000,000 of `10M_SE` / `10M_PE` (GRCh38 WGBS) + the mouse RRBS `R1/R2` (raw, S3-staged).
- **Harness:** `phase5_combined_gate.sh` (this dir). Identical-argv-into-same-`-o` (Perl moved aside); compare DECOMPRESSED SAM (keep the Bismark `@PG` — identical argv → it matches — drop only the samtools `@PG`) via the **Phase-10 streaming `cmp_files`** comparator (O(1) memory + bounded `sed`-window on mismatch, NOT a buffering `diff`); report wall-clock-filtered; `LC_ALL=C`. **Anti-vacuous-pass backstop:** non-empty + `samtools view -c` count-equality before each PASS.

## Results — all 13 cells byte-identical
| Cell | Backend | Layout/Lib | Genome | Records | Result |
|---|---|---|---|---|---|
| `ht2_se_dir` | HISAT2 | SE dir | GRCh38 | 8,462,737 | ✅ byte-identical + report |
| `ht2_se_nondir` | HISAT2 | SE non-dir | GRCh38 | 8,458,192 | ✅ |
| `ht2_se_pbat` | HISAT2 | SE pbat | GRCh38 | 60,486 | ✅ |
| `ht2_pe_dir` | HISAT2 | PE dir | GRCh38 | 16,249,792 | ✅ |
| `ht2_pe_nondir` | HISAT2 | PE non-dir | GRCh38 | 16,248,950 | ✅ |
| `ht2_pe_pbat` | HISAT2 | PE pbat (R1↔R2 swap) | GRCh38 | 16,248,540 | ✅ **real pbat signal** |
| `mm2_se_dir` | minimap2 | SE dir | GRCh38 | 7,993,577 | ✅ |
| `mm2_se_worker` | minimap2 | SE `--parallel 8`==`1` | GRCh38 | 7,993,577 (body) | ✅ **worker-invariant @10M** |
| `mm2_se_nondir` | minimap2 | SE non-dir | GRCh38 | 8,001,999 | ✅ |
| `mm2_se_pbat` | minimap2 | SE pbat | GRCh38 | 68,689 | ✅ |
| `bt2_se_dir` | Bowtie 2 | SE dir | GRCh38 | 8,501,508 | ✅ anchor |
| `bt2_pe_dir` | Bowtie 2 | PE dir | GRCh38 | 17,084,770 | ✅ anchor |
| `rrbs_bt2_pe_dir` | Bowtie 2 | PE dir | **GRCm39** | 12,558,088 | ✅ 2nd genome |
| `rrbs_ht2_pe_dir` | HISAT2 | PE dir | **GRCm39** | 11,726,516 | ✅ **2nd genome** |

(`ht2_se_dir` ran as the measurement cell — `p5_measure.log`; the other 12 in `p5_main.log`. Both persisted to oxy `/home`.)

## Key results
- **Byte-identity at 10M across every v1.x backend × library type × both genomes** — decompressed SAM + report identical, Perl vs Rust, single-core.
- **HISAT2 single-core strict** is the only faithful HISAT2 comparison (multicore is hard-rejected — not worker-invariant — so the Phase-10 content-multiset shortcut is invalid for it); confirmed at 10M for SE+PE dir/non-dir/pbat on GRCh38 **and** PE dir on GRCm39.
- **minimap2 worker-invariance re-confirmed at 10M** (`--parallel 8` == `--parallel 1`, 7,993,577 rec) — 10× the Phase-4 scale.
- **`ht2_pe_pbat` via the R1↔R2 swap** gives a genuine 4-instance pbat signal at scale (16.2M rec), not a vacuous near-empty cell.
- **Second genome (GRCm39):** mouse RRBS PE byte-identical for both HISAT2 and Bowtie 2 — new scaffold diversity beyond the human cells.

## Scope / caveats (honest)
- **non-dir/pbat on the directional GRCh38 dataset** land ~0 reads on the complementary strands (the small SE pbat counts 60k/69k reflect this) — those cells prove byte-identity-at-scale + strand routing, not full-strength non-dir/pbat coverage; the genuine non-dir/pbat coverage is the per-phase **1M gates** (2b / Phase 8) + worker-invariance 1M (9b). `ht2_pe_pbat` (swap) + the mouse cells carry the real new signal.
- minimap2 is SE-only (PE hard-rejected); `rrbs_mm2` dropped (RRBS is PE).
- Default `map-ont` gated; `sr`/`map-pb` unit-tested but not oxy-gated.

## Robustness note (this run)
The detached gate (`setsid nohup`) **survived a ~3 h loss of the local VPN/SSH connection to oxy** — it kept running on the pod and all cells completed; reconnecting recovered the full result. Binary + harness + logs on persistent `/home` made the run recycle/disconnect-resilient. oxy `/var/tmp` is ephemeral — this file + the `/home` logs are the durable record.

## Reproduce
```
# on oxy, binary at /home/bismark_rs_p5 (or /var/tmp/mm2_gate/.../bismark_rs)
bash /home/phase5_combined_gate.sh 8 10000000            # all cells (~10-15 h single-core)
bash /home/phase5_combined_gate.sh 8 10000000 ht2_se_dir # one cell
```

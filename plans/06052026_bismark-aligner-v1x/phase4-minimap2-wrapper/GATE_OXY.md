# GATE_OXY — Phase 4 (v1.x) minimap2 SE byte-identity gate

**Date:** 2026-06-05 · **Box:** oxy `dockyard-oxy-0` (cgroup 32c/256G) · **Verdict: ✅ ALL CELLS PASS (10k + 1M).**

`bismark_rs --minimap2` is **byte-identical** to Perl Bismark v0.25.1 + **minimap2 2.31-r1302** (samtools 1.23.1), on real human GRCh38 WGBS single-end reads, across all three library types + worker-invariance, at 10k and 1M reads.

## Setup
- **Rust binary:** built ON oxy from the uncommitted `rust/aligner-mm2` worktree (`tar | dcli ssh`, `cargo 1.96 --release`) → `/var/tmp/mm2_gate/rust/target/release/bismark_rs`.
- **Oracle:** `~/micromamba/envs/bismark-test/bin/{bismark (v0.25.1), minimap2 (2.31-r1302), samtools (1.23.1)}` (prepended to PATH; not activatable non-interactively).
- **Genome:** `~/bismark_benchmarks/genome` (incl. `BS_CT.mmi` / `BS_GA.mmi`, ~7.9 GB each).
- **Reads:** first N of `~/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz`.
- **Harness:** `phase4_minimap2_se_gate.sh <N>` (this dir). Identical argv into the same `-o` (Perl moved aside); compare DECOMPRESSED SAM (`samtools view -h | grep -v ID:samtools`) + report (wall-clock-filtered). A basename match between the Perl/Rust outputs IS the `_bismark_mm2` naming-token check.

## Cells
| Cell | Argv | 10k | 1M |
|------|------|-----|-----|
| `se_dir` | `--minimap2 se.fq` | ✅ byte-identical (7,932 rec) + report | ✅ byte-identical (**796,919 rec**) + report |
| `se_nondir` | `--minimap2 --non_directional se.fq` | ✅ byte-identical (7,940 rec) + report | ✅ byte-identical (**797,799 rec**) + report |
| `se_pbat` | `--minimap2 --pbat se.fq` | ✅ byte-identical (51 rec) + report | ✅ byte-identical (**6,858 rec**) + report |
| `se_multicore_invariance` | Rust `--parallel 8` vs `--parallel 1` (SAM body) | ✅ identical (7,932) | ✅ identical (**796,919**) — worker-invariant |
| `zero_secondary` | raw minimap2, all 4 instances | ✅ 0 secondary | ✅ 0 secondary (2 supplementary — informational, see below) |

(`se_dir` 7,932 = the Phase-3 spike's 7,933 unique − 1 chromosome-edge discard.)

## Key results
- **Byte-identity holds at scale** for SE directional / non-directional / pbat — decompressed SAM + report identical, Perl vs Rust, at both 10k and 1M.
- **Worker-invariance CONFIRMED (OQ-4d):** Rust `--minimap2 --parallel 8` == `--parallel 1` on the SAM body at 10k AND 1M. Unlike HISAT2 (hard-rejected — batch-global splice discovery), minimap2 is per-read-independent → multicore is byte-identical and is **allowed**. With `se_dir` (Rust-p1 == Perl single-core), this gives Rust-p8 content == Perl transitively.
- **`--secondary=no` works perfectly: 0 SECONDARY records** (flag 256) on every instance at 10k and 1M → the lockstep "one primary per read" invariant (Reviewer A's V9 ask) is satisfied.

### Scale-dependent SUPPLEMENTARY finding (informational — no divergence)
At 1M, the GA-index / G→A-reads instance (CTOB / the non-dir 4th instance) emits **2 SUPPLEMENTARY records** (flag 2064 = 2048+16, `SA:Z:` present, hard-clipped chimeric CIGAR `17M10D28M4D11M9H`) for reads `SRR24827373.542634` and `.545321`. This is **not** a secondary alignment — `--secondary=no` does not (and is not designed to) suppress supplementary/split alignments. It is **byte-harmless**:
- The `se_nondir` 1M cell (which drives this exact instance) is **byte-identical, diff = 0 lines**.
- Both affected reads are **absent from BOTH the Perl and the Rust BAM** (identical handling) — Bismark's best-alignment selection treats the chimeric alignment the same way on both sides.

So minimap2's supplementary alignments at scale are handled byte-identically by the Perl oracle and the Rust port → zero byte-identity exposure. (The gate's `zero_secondary` check was corrected to fail only on SECONDARY, not supplementary, and to report supplementary as informational.)

## Scope / caveats
- **SE only.** PE-minimap2 is deferred out of v1.x (no trustworthy Perl oracle) and is hard-rejected by `bismark_rs` (test `minimap2_paired_end_is_rejected`).
- **pbat / non-dir caveat (same as Phase 8):** the directional dataset lands ~0 reads on the complementary strands under `--pbat` (51 / 6,858 rec), so those cells prove byte-identity-at-scale on the dominant strand + the strand routing; the integration tests prove the strand arithmetic.
- **Default `map-ont` gated** (OQ-4b); `sr`/`map-pb` are unit-tested but not oxy-gated.
- Bowtie 2 + HISAT2 remain byte-frozen (the local 277-test baseline + the kind-gated argv/option guards; their own gates unaffected by this `kind`-gated change).

## Reproduce
```
# on oxy, with the binary built at /var/tmp/mm2_gate/rust/target/release/bismark_rs
bash /var/tmp/phase4_minimap2_se_gate.sh 10000      # fast
bash /var/tmp/phase4_minimap2_se_gate.sh 1000000    # ~detach+poll
```
oxy `/var/tmp` is ephemeral (recycle wipes it) — this file is the durable record.

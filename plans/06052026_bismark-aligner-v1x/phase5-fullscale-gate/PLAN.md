# PLAN — Phase 5: combined v1.x real-data gate (10M strict) + README bump + PR

> **Epic:** `06052026_bismark-aligner-v1x/EPIC.md`, Phase 5 — *Combined full-scale gate + PR* (the **LAST** phase of the v1.x epic). **Depends on:** #2b (HISAT2 PE, merged `49a1518` via #949) + #4 (minimap2 SE, in **PR #950**, CI-green/MERGEABLE).

- **Created:** 2026-06-05 · **rev 0** (awaiting manual review → dual plan-review → implement trigger).
- **Branch / worktree:** **`rust/aligner-mm2`** @ `~/Github/Bismark-aligner` (the Phase-4 branch — already carries all three backends: Bowtie 2 frozen + HISAT2 from `49a1518` + minimap2). The Phase-5 gate doc + harness + the README bump **fold into PR #950** (Felix decision).
- **Oracle / pins:** Perl Bismark **v0.25.1** · Bowtie 2 **2.5.5** · HISAT2 **2.2.2** · minimap2 **2.31-r1302** · samtools **1.23.1** (oxy `~/micromamba/envs/bismark-test`).
- **Status:** Planning. **No gate has been run.** Execution is gated on Felix's explicit `implement` trigger.

---

## 1. Goal
Confirm, **at 10M reads on real human (GRCh38) + mouse (GRCm39) data**, that `bismark_rs` reproduces Perl Bismark v0.25.1 driving the *same pinned aligner*, for the two **new** v1.x backends (HISAT2 SE+PE, minimap2 SE) — with Bowtie 2 as a regression anchor — then complete the v1.x epic: bump `rust/README.md` (status row + Milestones) and ship via **PR #950**.

**New information beyond the per-phase gates** (which proved byte-identity at 10k/1M): **10× scale** on each backend, **more chromosome diversity**, and a **second genome (GRCm39)** for HISAT2. Scale chosen = **10M, single-core strict** (Felix): small enough that single-core runs finish in hours (so HISAT2 — which is *not* worker-invariant and cannot use a content-multiset shortcut — is gateable strictly), and in-order so the gate uses simple streaming `cmp`, not the Phase-10 content-multiset machinery.

## 2. Context
### Placement / dependencies
- **Writes no production Rust code.** Like the faithful Phase 10, this is a validation/gate phase: an oxy build, one (parameterized) gate harness, a run procedure, a results doc (`GATE_OXY.md`), and a `rust/README.md` bump. Repo artifacts live under `plans/06052026_bismark-aligner-v1x/phase5-fullscale-gate/` + the README bump.
- **Binary under test = the `rust/aligner-mm2` build** (= iron-chancellor `49a1518` incl. HISAT2 + the Phase-4 minimap2 commit `21bac5d`). This is exactly what #950 will squash-merge, so gating it == gating the merged v1.x.
- **Reuses the gate methodology** of the per-phase oxy gates (`phase2{a,b}_hisat2_*_gate.sh`, `phase4_minimap2_se_gate.sh`) and the faithful Phase 10 (`plans/05312026_bismark-aligner/phase10-realdata-gate-oxy/`) — but at **10M strict in-order** (not 84M content-multiset).

### Per-backend gate shape (the load-bearing distinction)
| Backend | Worker-invariant? | Gate at 10M |
|---|---|---|
| **Bowtie 2** | yes | Rust `--parallel 1` **==** Perl single-core (strict). Anchor; full-scale already proven in faithful Phase 10. |
| **HISAT2** | **NO** (batch-global splice discovery → multicore hard-rejected) | Rust **single-core** **==** Perl **single-core** (strict). **No worker leg.** This is the *only* faithful HISAT2 comparison. |
| **minimap2 SE** | yes (per-read-independent) | Rust `--parallel 1` **==** Perl single-core (strict) **+** a bonus A-worker leg (Rust `--parallel P` == `--parallel 1`). |

## 3. Behavior — the gate definition
### 3.1 Method (per cell, 10M, in-order strict)
Identical-argv-into-the-same-`-o` (Perl output moved aside), then compare:
- **BAM:** DECOMPRESSED SAM content via **streaming `LC_ALL=C cmp`** (the runs are in-order at single-core — see the in-order justification below — so O(1) memory; on mismatch, map the byte offset to a bounded line window with `sed` on both filtered streams — **never re-`diff` the full stream** (the buffering hazard). Use the **Phase-10 `cmp`-based comparator** (`phase10_subset_strict_gate.sh`'s `cmp_files` + the `sed`-window-on-mismatch recipe), **NOT** the `phase4_minimap2_se_gate.sh` `run_cell`'s `diff <(…) <(…)`, which buffers both multi-GB streams (review A/B — the load-bearing harness fix).
  - **`@PG` filter, per cell type:** the *strict* cells run **identical argv** on both sides → the Bismark `@PG CL` line MATCHES; only the samtools-pipe `@PG` differs → `grep -v 'ID:samtools'` suffices (as proven in the 2a/4 gates). The **minimap2 A-worker leg** varies `--parallel` → filter the **whole `@PG` block** there (or body-only, below).
  - **🔴 anti-vacuous-pass backstop (review A/B):** a streaming `cmp` of two identically-truncated/empty streams "passes". So BEFORE declaring a cell PASS, assert (a) **both BAMs non-empty** and (b) `samtools view -c` **count equality** Perl==Rust==report-implied (carry Phase-10's B1.5 guard — an in-order `cmp` subsumes count/header/RNAME identity ONLY if it ran to completion on non-empty input).
- **Report:** wall-clock line (`^Bismark completed in `) filtered, `LC_ALL=C cmp`.
- **Aux** (`--unmapped`/`--ambiguous`/`--ambig_bam`): decompressed compare (FastQ **record-ized via `paste - - - -`** before any compare; `--ambig_bam` via `samtools view`).
- **Naming check** is implicit: a basename match between Perl/Rust outputs proves the `_bismark_{hisat2,mm2,bt2}` token.
- **minimap2 only — A-worker leg:** Rust `--parallel P` SAM **body** (`samtools view`, no `-H`) == Rust `--parallel 1` (the `@PG` CL embeds `--parallel` → body-only). Confirms worker-invariance at 10× the Phase-4 scale.
- **minimap2 in-order guard (review B I-3):** minimap2 input-order is spike-validated at 10k + byte-identical at 1M (Phase-4 gate), but not >1M. If a strict `cmp` FAILs on a minimap2 cell, run a **sorted-multiset diagnostic** (`LC_ALL=C sort | md5sum` both sides) to distinguish a *reorder-only* anomaly (same multiset, different order → investigate minibatch ordering, not necessarily a content divergence) from a *real content* divergence — before treating it as a byte-identity failure.

### 3.2 Cells (all at 10M; strict Rust==Perl unless noted)
**GRCh38 (human WGBS), reads = first 10M of `~/bismark_benchmarks/10M_SE|10M_PE`:**
| Cell | Backend | Layout | Library | Notes |
|---|---|---|---|---|
| `bt2_se_dir` / `bt2_pe_dir` | Bowtie 2 | SE / PE | directional | **anchor** (Rust `--p1` == Perl single-core); fast (worker-invariant) |
| `ht2_se_dir` / `ht2_pe_dir` | HISAT2 | SE / PE | directional | single-core strict — the headline new-scale datapoint |
| `ht2_se_nondir` / `ht2_se_pbat` | HISAT2 | SE | non-dir / pbat | single-core; ~0 reads on complementary strand for a directional dataset (Phase-8 caveat) |
| `ht2_pe_nondir` | HISAT2 | PE | non-dir | single-core; dominant-strand only on directional data (caveat) |
| `ht2_pe_pbat` | HISAT2 | PE | pbat | single-core; **run via the R1↔R2 swap** (feed R2 as `-1`, R1 as `-2` with `--pbat`) so the directional data aligns as genuine pbat (CTOT/CTOB) — a REAL 4-instance pbat signal at scale (Phase-10 O5 trick), not a vacuous near-empty cell |
| `mm2_se_dir` | minimap2 | SE | directional | strict + **A-worker** leg |
| `mm2_se_nondir` / `mm2_se_pbat` | minimap2 | SE | non-dir / pbat | strict (Phase-8 caveat) |

> **🔴 non-dir/pbat coverage caveat (review A I-4 / B I-2 — stated, not hidden):** on a *directional* 10M dataset, the non-dir/pbat cells land only a rounding-error number of reads on the complementary strands, so (except `ht2_pe_pbat` via the R1↔R2 swap above) they **mostly re-prove the directional cell** + the strand-routing wiring — they do NOT add full-strength non-dir/pbat coverage at 10M, and a *strand-only* bug at scale would be near-invisible to them. The genuine non-dir/pbat coverage remains the per-phase **1M gates** (2b / Phase 8) + the worker-invariance 1M (9b). The "v1.x complete" claim rests on: dir at 10M (all backends) + the `ht2_pe_pbat` real-pbat swap + the mouse 2nd-genome cell + the per-phase 1M non-dir/pbat gates — **not** on these directional-dataset non-dir/pbat cells being full-strength.

**GRCm39 (mouse RRBS, raw `~/bismark_benchmarks/RRBS_PE` reads, subset to 10M if larger):**
| Cell | Backend | Layout | Notes |
|---|---|---|---|
| `rrbs_ht2_pe_dir` | HISAT2 | PE dir | single-core strict — **second genome**, the strongest new scaffold-diversity datapoint |
| `rrbs_bt2_pe_dir` | Bowtie 2 | PE dir | anchor on GRCm39 |

*(OQ-5c RESOLVED — `rrbs_mm2_se` DROPPED: RRBS is PE and minimap2 is SE-only; minimap2 SE is already gated on human GRCh38, so a contrived R1-as-SE mouse run adds little. Mouse = HISAT2 PE + Bowtie 2 PE.)*

### 3.3 What legitimately differs (filter, don't fail on)
`@PG` block (per-run argv + samtools abs path), the report wall-clock line, gz/BGZF framing (compare decompressed content), `--parallel` in the minimap2 worker-leg `@PG` (body-only compare). Same rules as the per-phase gates.

## 4. Implementation outline (the run procedure)
> All on oxy via `dcli ssh oxy '…'` with `dangerouslyDisableSandbox: true`. **None runs until Felix's `implement` trigger.**
0. **Build on oxy** from `rust/aligner-mm2` (`tar --exclude target | dcli ssh … && cargo build --release -p bismark-aligner`). The Phase-4 gate already built this at `/var/tmp/mm2_gate` — reuse if the pod hasn't recycled, else rebuild. Capture `bismark_rs --version` + commit `21bac5d` for the repro tuple.
1. **Sanity-check the mouse v1.x indexes (OQ-5a — RESOLVED: expected present).** Felix: the GRCm39 indexes should already exist on oxy. Confirm `BS_CT/BS_GA` **`.ht2`** (HISAT2) + **`.bt2`** (Bowtie 2) under the mouse `Bisulfite_Genome/` before the mouse cells. The mouse **`.mmi` is NOT required** (the mouse minimap2 cell was dropped, OQ-5c). If the `.ht2` is unexpectedly missing → surface it (don't auto-build) and fall back to `rrbs_bt2_pe_dir` only.
2. **Measure + stage inputs.** Read counts for SE/PE/RRBS; subset to 10M (`zcat | head -n 40000000`); stage S3-symlinked RRBS reads to `/var/tmp`.
3. **Write `phase5_combined_gate.sh`** — parameterized on `(backend, layout, library, genome, reads, N)`. Take the identical-argv-into-same-`-o` + `@PG`/wall-clock-filter structure from `phase4_minimap2_se_gate.sh`, **but the BAM comparator MUST be the Phase-10 streaming `cmp_files`** (`phase10_subset_strict_gate.sh`) + the `sed`-window-on-mismatch recipe + the **non-empty + `samtools view -c` count backstop** — NOT phase4's buffering `diff <(…) <(…)` (review A/B C-1). HISAT2 cells: single-core both sides (no worker leg). minimap2 `mm2_se_dir`: add the A-worker leg + the in-order sorted-multiset diagnostic on `cmp`-FAIL (§3.1). Bowtie 2 cells: Rust `--p1` vs Perl single-core. `ht2_pe_pbat`: the R1↔R2 swap (§3.2). `LC_ALL=C` on every `cmp`/`sort`/`md5sum`.
4. **Run the gate (detached, polled, off-box capture per cell).** Order: Bowtie 2 anchors (fast) → minimap2 SE → HISAT2 SE → HISAT2 PE → mouse cells. Single-core HISAT2 at 10M is the runtime driver (hours/cell) → `setsid nohup … </dev/null &` + frequent poll; capture each cell's verdict to `GATE_OXY.md` as it finishes (recycle insurance).
5. **Author `GATE_OXY.md`** — per-cell PASS/FAIL + record counts + the reproduction tuple (binary commit, all four `--version`s, dataset sizes/md5s, exact argv). Honest runtime note (single-core HISAT2 is slow by design — not a perf claim).
6. **On PASS (ALL cells) → complete the epic (fold into #950):** **only after every cell PASSes** (review A/B I-4: CI green ≠ gate green — CI does NOT run the oxy gate, so the README "v1.x complete" line must not precede an actual all-cells gate PASS) bump `rust/README.md` aligner row (Bowtie 2 + HISAT2 SE+PE + minimap2 SE, all byte-identical; v1.x complete) + a dated Milestones line; update `EPIC.md` Phase 5 + `PROGRESS.md`; commit the gate doc + harness + README bump to `rust/aligner-mm2`; push to #950 (CI re-runs). **Squash-merge #950 into iron-chancellor only on Felix's explicit "merge".**
7. **On any FAIL:** stop, save the diff window + logs off-box, do **not** auto-fix — report the gap and wait (`~/.claude/CLAUDE.md`).

## 5. Efficiency
- 10M in-order → **streaming `cmp`** (O(1) memory); no 84M content-multiset sort needed. `LC_ALL=C` for a deterministic total order. Bowtie 2/minimap2 cells run `--parallel P` (fast); only HISAT2 is single-core-bound (the deliberate cost of faithfulness). Run cells sequentially; capture off-box per cell.

## 6. Integration
- **Reads:** the human GRCh38 `.bt2/.ht2/.mmi` indexes (present) + the mouse GRCm39 indexes (verify, OQ-5a) + the 10M SE/PE + RRBS reads. **Writes:** only `/var/tmp` run output (ephemeral) + the phase5 artifacts + the `rust/README.md` bump (ride #950).
- **Downstream:** the validated v1.x BAM is the input contract for the already-ported Rust post-alignment tools — confirming byte-identity at 10M across all three backends closes the v1.x loop.

## 7. Assumptions
**From the epic:** oracle = Perl v0.25.1 + pinned aligners; gate on decompressed SAM content (`@PG`/wall-clock filtered), adjudicated on Linux/oxy; Bowtie 2 byte-frozen; don't name external *bisulfite* aligners in committed docs (Bowtie 2/HISAT2/minimap2 are declared deps — fine).
**Phase-5 specific:** HISAT2 is single-core-only (no worker leg; multicore rejected); minimap2 SE is worker-invariant (A-worker leg valid); the directional 10M dataset lands ~0 reads on complementary strands under non-dir/pbat (Phase-8 caveat — those cells prove byte-identity-at-scale + routing, not new strand coverage); `/var/tmp` survives a single cell (detach + per-cell off-box capture + idempotent re-run); the `rust/aligner-mm2` binary == the #950 merge payload.

## 8. Validation (the gate IS the validation)
| # | Verify | How | Expect |
|---|---|---|---|
| V1 | Bowtie 2 anchor unchanged | `bt2_se_dir`/`bt2_pe_dir` 10M, Rust `--p1` == Perl single-core | byte-identical |
| V2 | HISAT2 SE/PE dir at 10M | single-core strict | byte-identical SAM + report + aux |
| V3 | HISAT2 non-dir/pbat (SE+PE) at 10M | single-core strict | byte-identical |
| V4 | minimap2 SE dir/non-dir/pbat at 10M | single-core strict | byte-identical |
| V5 | minimap2 worker-invariance at 10M | `mm2_se_dir` Rust `--pP` == `--p1` (body) | identical |
| V6 | second genome (GRCm39) | `rrbs_ht2_pe_dir` (+ `rrbs_bt2_pe_dir`) single-core strict | byte-identical (mouse scaffolds) |
| V7 | README/epic completion | row + Milestones bumped; EPIC/PROGRESS updated | v1.x marked complete |

## 9. Questions or ambiguities — all RESOLVED (Felix, 2026-06-05)
- **OQ-5a (RESOLVED + CONFIRMED LIVE):** both plan-reviewers verified on oxy that the mouse GRCm39 `BS_{CT,GA}.1-8.ht2` + `.bt2` indexes exist (the `.mmi` exists too but is unused — mouse minimap2 dropped). **Not a risk.** Step 1 keeps the sanity-check as a guard; the Bowtie 2-only fallback is moot.
- **OQ-5b (RESOLVED → keep the full matrix):** run all ~12 cells (Bowtie 2 SE/PE dir + HISAT2 SE+PE × dir/non-dir/pbat + minimap2 SE × dir/non-dir/pbat + mouse HISAT2/Bowtie 2 PE), sequentially, detached. Single-core HISAT2 at 10M is the runtime driver — accepted.
- **OQ-5c (RESOLVED → drop `rrbs_mm2_se`):** mouse = HISAT2 PE + Bowtie 2 PE (RRBS is PE, minimap2 is SE-only).
- **OQ-5d (RESOLVED → P=8):** the minimap2 A-worker leg + the Bowtie 2/minimap2 `--parallel` cells run at P=8 (cgroup 32c/256G; 2P=16 instances, headroom — matches the faithful Phase 10 rev-2 correction).

## Revision History
- **rev 2 (2026-06-05):** dual plan-review (`PLAN_REVIEW_A.md` APPROVE-WITH-CHANGES 0C/4I + `PLAN_REVIEW_B.md` APPROVE-WITH-CHANGES 1C/4I; both re-derived HISAT2-single-core-only + in-order-`cmp`-validity from the crate code, and **confirmed live on oxy** that the mouse `.ht2`/`.bt2` indexes exist + the pins match + the binary == the #950 payload). Folded: **C-1/A#1 (load-bearing) — the BAM comparator MUST be the Phase-10 streaming `cmp_files`, NOT phase4's buffering `diff`** (§3.1, §4 step 3); the **anti-vacuous-pass backstop** (non-empty + count-equality before PASS, §3.1); the **`@PG` filter clarified per cell type** (strict = `ID:samtools`; worker leg = whole block/body-only, §3.1); the **minimap2 10M in-order guard** (sorted-multiset diagnostic on `cmp`-FAIL, §3.1); the **non-dir/pbat caveat made explicit** + **`ht2_pe_pbat` committed to the R1↔R2 swap** for a real pbat signal (§3.2); **README "complete" only after an all-cells gate PASS** (CI green ≠ gate green, §4 step 6); OQ-5a confirmed live.
- **rev 1 (2026-06-05):** manual review — Felix resolved all four open questions. Folded: OQ-5a (mouse `.ht2`/`.bt2` expected present, `.mmi` not needed, sanity-check don't auto-build); OQ-5b (keep the full ~12-cell matrix incl. non-dir/pbat); OQ-5c (drop `rrbs_mm2_se` — mouse = HISAT2 PE + Bowtie 2 PE); OQ-5d (P=8). Ready for dual plan-review.
- **rev 0 (2026-06-05):** initial Phase-5 plan — 10M single-core strict, fold into #950 (Felix scope decisions), reusing the per-phase gate methodology + the faithful Phase 10 template (adapted: in-order streaming `cmp`, not 84M content-multiset).

## 10. Self-Review
- **Logic:** the per-backend gate shape is correct — HISAT2 *must* be single-core-strict (it's the only faithful comparison; the Phase-10 content-multiset trick is invalid for it because Perl HISAT2 multicore is not multiset-invariant), while minimap2/Bowtie 2 get the cheap worker legs. 10M in-order → streaming `cmp`, no heavy machinery.
- **Edge cases:** `@PG`/wall-clock filters; FastQ-aux record-ization; `--ambig_bam` via `samtools view`; the non-dir/pbat directional-dataset caveat surfaced (not hidden); mouse-index existence flagged as a blocking step-0 check (OQ-5a).
- **Integration:** no production code → zero regression surface beyond the gate; the README bump + gate doc fold into the already-green #950.
- **Risks (named):** (a) single-core HISAT2 10M runtime (hours; mitigated by detach + per-cell capture); (b) mouse v1.x indexes may be absent (OQ-5a — could trim mouse to Bowtie 2 only); (c) non-dir/pbat add little new coverage at 10M (OQ-5b trim candidate); (d) faithful-port circularity is by design (the Perl run is the oracle).

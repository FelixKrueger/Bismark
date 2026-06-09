# Benchmark results — Bismark alignment modes (time · CPU · memory · concordance)

> Companion to [`BENCHMARK_SPEC_alignment_modes.md`](BENCHMARK_SPEC_alignment_modes.md). One apples-to-apples
> comparison of Bismark alignment modes for the nf-core beta announcement. Scope = **alignment only**.
> **Run:** 2026-06-09 on **oxy** (`dockyard-oxy-0`, 64c/128t K8s pod). Slug `plans/06092026_bismark-beta/`.
> Feeds [[project_full_beta_nfcore_announcement]].

## Run configuration
| | |
|---|---|
| Rust binary | `bismark_rs` **2.0.0-beta.1**, built fresh from shipped `rust/iron-chancellor` **`0b6bb8b`** in an isolated oxy worktree (`~/Bismark-bench`) |
| Perl oracle | **Bismark v0.25.1** (`micromamba/envs/bismark-test`) |
| Bowtie 2 | **2.5.5** (byte-identity prerequisite) · samtools 1.23.1 |
| Genome | GRCh38, `~/bismark_benchmarks/genome` (per-strand `.bt2` + combined `BS_combined.*.bt2l`) |
| Dataset | **10,000,000 real WGBS SE reads** (`directional_10M_R1_val_1.fq.gz`) — same file for every row |
| Core budget | **fixed ~16-core total** (instances × `-p` ≈ 16): 2×8, 4×4, 1×16 |
| RSS metric | **process-tree sampler** (descendant-PID walk from the launched root; 0.3 s; peak retained) summing wrapper + all `bowtie2-align-{s,l}` — *not* `time -v` MaxRSS |

## Results (8 rows; pbat dropped — see Caveats)

| Mode | Engine | Wall | CPU core-s | Peak tree-RSS | Idx loads | Concordance vs faithful/Perl |
|------|--------|-----:|-----------:|--------------:|----------:|------------------------------|
| Perl 0.25.1 directional      | 2 inst | 604 s (10:04) | 8600.8 | 10.03 GB | 2 | — (baseline) |
| Perl 0.25.1 non-directional  | 4 inst | 665 s (11:05) | 9407.6 | 16.58 GB | 4 | — (baseline) |
| **Rust faithful directional**     | 2 inst | **229 s (3:49)** | **3145.0** | 9.76 GB | 2 | **byte-identical** to Perl ✅ |
| **Rust faithful non-directional** | 4 inst | **477 s (7:57)** | **6910.3** | 16.32 GB | 4 | **byte-identical** to Perl ✅ |
| Rust combined directional         | 1 pass | 176 s (2:56) | 2406.3 | 11.13 GB | **1** | churn **0.0999 %** vs faithful dir |
| Rust combined non-dir — parallel (a) | 2 pass | 434 s (7:14) | 6238.7 | 18.98 GB | 2 | churn **0.1324 %** vs faithful non-dir |
| Rust combined non-dir — **sequential** | 2 serial | 400 s (6:40) | 5609.1 | **11.14 GB** | **1** | **byte-identical** to parallel (a) ✅ |
| Rust combined non-dir — single-pass (b) | 1 pass | 377 s (6:17) | 5602.4 | **11.13 GB** | **1** | **0.0000 %** churn vs parallel (a) |

*(Wall affected by ~10–17 node-level load avg from co-tenants on the shared K8s node; this pod's own processes were idle at launch. Prior gates ran under identical conditions.)*

## Headline findings

### 1. The faithful Rust aligner is meaningfully faster than Perl — not "≈ Perl"
Same Bowtie 2 2.5.5, same `-p`, same reads, same box. The difference is **wrapper efficiency**: Bismark does heavy per-read work *around* Bowtie 2 — in-silico bisulfite conversion of every read and methylation-call/tag processing of every alignment. Perl does this slowly; Rust does it efficiently, and at 10M reads that dominates.

| | Perl | Rust faithful | speedup |
|---|---:|---:|---:|
| directional wall | 604 s | 229 s | **2.64×** |
| directional CPU core-s | 8600.8 | 3145.0 | **2.73×** less |
| non-directional wall | 665 s | 477 s | **1.39×** |
| non-directional CPU core-s | 9407.6 | 6910.3 | **1.36×** less |

…and **byte-identical** output (decompressed BAM records, header/@PG excluded). The spec deliberately under-claimed alignment wall speed; the data supports a stronger, still-fair claim.

### 2. The combined-index memory win is **mode-specific** (the honest framing)
A single *combined* index holds *both* CT+GA genomes, so it's ~as large as the per-strand pair. RAM, lowest→highest:

| Layout | Mode(s) | Peak RSS | Idx loads |
|---|---|---:|---:|
| 2 thin per-strand | faithful directional | 9.76 GB | 2 |
| **1 fat combined** | combined dir / **seq** / **1-pass** | **11.1 GB** | **1** |
| 4 thin per-strand | faithful non-directional | 16.32 GB | 4 |
| 2 fat combined | combined non-dir parallel (a) | 18.98 GB | 2 |

- **The genuine RAM win is non-directional, sequential or single-pass:** **11.1 GB vs the 4-instance faithful's 16.3 GB → −31.7 %**, *and* it collapses 4 concurrent index loads to 1.
- **Directional:** combined uses *slightly more* RAM (11.1 vs 9.8 GB). Its win is "one index / one process / `-p 16`," not memory.
- **Parallel (a)** is the worst on RAM (2 fat indices, 19 GB) — avoid it when memory matters; prefer **sequential** (byte-identical result, half the resident indices).

### 3. Sequential is a dual win for non-directional; single-pass is the fastest
- **Sequential** (400 s, 11.14 GB) beats the **faithful 4-instance** (477 s, 16.32 GB) on **both wall *and* memory**, and is **byte-identical to parallel (a)**. Serial passes are *not* a wall penalty here: each pass drives the combined index at `-p 16` (uncontended) vs 4 contended `-p 4` instances.
- **Single-pass (b)** is fastest of the non-dir combined modes (377 s) at the same 11.1 GB, with **0.0000 % churn vs parallel (a)** on this real 10M set (no parallel-a-unique read moved or vanished).

## Concordance detail
Computed on the **final shipped BAMs** (see Caveats for why, not the spike SAM analyzer).

| Comparison | Result |
|---|---|
| `rust_dir` vs `perl_dir` | **byte-identical** (md5 `bd2df4bd…`) |
| `rust_nondir` vs `perl_nondir` | **byte-identical** (md5 `728d4030…`) |
| `comb_nondir_seq` vs `comb_nondir_parA` | **byte-identical** (md5 `bf813fcd…`) |
| `comb_dir` vs `rust_dir` (faithful) | churn **0.0999 %** — of 8,501,508 faithful-unique reads: 8,494 changed (8,079 → ambiguous/unmapped, **415 moved locus** = 0.0049 %); 5,316 gained |
| `comb_nondir_parA` vs `rust_nondir` | churn **0.1324 %** — of 8,494,541: 11,250 changed (10,699 → ambiguous, **551 moved** = 0.0065 %); 5,396 gained |
| `comb_nondir_1pass` vs `comb_nondir_parA` | churn **0.0000 %** (0 of 8,489,238) |

**Interpretation:** the combined-index "cost" is almost entirely reads that flip **unique↔ambiguous** (the irreducible cross-sub-genome tie the combined search can see and the per-strand search cannot). Actual *mis-placement* is negligible (415 / 551 reads = ~0.005–0.007 %). Faithful methods and combined-sequential are exact.

## Caveats & methodology notes
- **pbat rows dropped.** The dataset is *directional* WGBS; under `--pbat` reads are expected from the complementary (CTOT/CTOB) strands, so only **0.4 %** aligned (smoke: 870/200k). pbat throughput/concordance are not benchmarkable on this set (byte-identity `rust_pbat == perl_pbat` *was* confirmed on the smoke subset). A meaningful pbat benchmark needs a real PBAT read set.
- **Churn on final BAMs, not the spike analyzer.** `analyze_combined_*.py` operate on the spike's raw `-k2` bowtie2 SAM keyed to the 1M synthetic Sherman baseline; no such baseline exists for this 10M real set. Final-BAM "oracle-unique-stays-unique" concordance is both the right tool for this dataset and more credible for an announcement. It runs **higher than the synthetic spike anchors** (dir 0.10 % vs ~0.013 %; non-dir 0.13 % vs ~0.022–0.044 %) because (a) real reads carry errors/repeats and (b) the metric counts unique→ambiguous transitions, which the narrower spike metric did not.
- **RSS is summed per-process** (matches the gate method). Read-only `.bt2l` pages are mmap-shared between parallel-a's two passes, so the sum over-counts shared pages — the absolute parallel-a figure (19 GB) is an upper bound; the *ratio* and single-index figures are sound. Full-tree numbers run ~3 GB above the bowtie2-only spike anchors (15.70/7.82 GB) by design (they include the wrapper).
- **RSS validated at 200k too.** Peak RSS is index-dominated (read-count-independent); the smoke subset reproduced these figures, confirming RSS conclusions hold across scale.
- **Shared K8s node.** Node load avg 10–17 from co-tenants throughout; cannot guarantee node-idleness, only pod-idleness. Wall figures carry that (unavoidable, consistent-with-prior-gates) noise.

## Reproduce
```
# isolated shipped binary (does not disturb a parallel session's ~/Bismark WIP):
git -C ~/Bismark worktree add ~/Bismark-bench 0b6bb8b
cd ~/Bismark-bench/rust && cargo build --release -p bismark-aligner
# harness (8 rows, fixed 16-core budget, tree-RSS sampler, md5 + churn concordance):
bash run_bench_alignment_modes.sh ~/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz full10M
```
Harness `run_bench_alignment_modes.sh` + churn comparator `bam_churn.py` are committed alongside this file.

## Artifacts (captured off-box from oxy `~/v2spike_out/bench_align_modes/full10M/`)
- `full10M_summary.tsv` — raw per-cell metrics
- `full10M_concordance.tsv` — raw md5 + churn output
- `full10M_bench.log` — full run log with per-cell timestamps
- `run_bench_alignment_modes.sh`, `bam_churn.py` — the harness + comparator

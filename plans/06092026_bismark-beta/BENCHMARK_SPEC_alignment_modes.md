# Benchmark spec — Bismark alignment modes (time · CPU · memory · concordance)

> **Purpose:** produce ONE apples-to-apples table comparing every Bismark alignment mode for the nf-core beta announcement. Scope = **alignment only** (downstream-tool speedups are already documented; deliberately excluded here for clarity/lower complexity).
> **Run in a clean session on oxy.** This spec is turnkey: the matrix, exact commands, metric-capture method, dataset, core scheme, N/A cells, and pre-filled sanity anchors are all below.
> **Created:** 2026-06-09 · slug `plans/06092026_bismark-beta/` · feeds [[project_full_beta_nfcore_announcement]].

## 0. The framing that shapes the table (read first)
The faithful Rust aligner is a **Bowtie 2 wrapper** → its wall-clock ≈ Perl's (same Bowtie 2, same work). So this table's story is **memory + concordance + CPU-efficiency**, NOT dramatic alignment wall speedup. Specifically the expected headline is the **combined-index memory win** (one index load vs 2–4) and the per-read CPU-efficiency of combined alignment — plus byte-identical / low-churn concordance. Do not over-claim alignment wall speed; the suite's wall wins are downstream (out of scope here).

## 1. Modes (rows) — exact commands
Same real read set + genome for every row (the modes differ in alignment strategy, not input). `$BR` = the phase-9+ Rust binary `~/Bismark/rust/target/release/bismark_rs`; `$PERL_BM` = **Perl Bismark v0.25.1** (the gate oracle — confirm its path, §6); `$ENV=~/micromamba/envs/bismark-test/bin` (bowtie2+samtools); `$G=~/bismark_benchmarks/genome` (has `Bisulfite_Genome/` incl. `Combined/BS_combined`).

| # | Mode | Engine | Command (SE) | bowtie2 `-p` |
|---|------|--------|--------------|-------------|
| 1 | Perl 0.25.1 directional | 2 per-strand instances | `$PERL_BM --genome $G -p 8 -o OUT $READS` | 8 |
| 2 | Perl 0.25.1 non-directional | 4 instances | `$PERL_BM --genome $G --non_directional -p 4 -o OUT $READS` | 4 |
| 3 | Perl 0.25.1 pbat | 2 instances | `$PERL_BM --genome $G --pbat -p 8 -o OUT $READS` | 8 |
| 4 | Rust faithful directional | 2 instances | `$BR --genome $G --path_to_bowtie2 $ENV -p 8 -o OUT $READS` | 8 |
| 5 | Rust faithful non-directional | 4 instances | `$BR --genome $G --non_directional --path_to_bowtie2 $ENV -p 4 -o OUT $READS` | 4 |
| 6 | Rust faithful pbat | 2 instances | `$BR --genome $G --pbat --path_to_bowtie2 $ENV -p 8 -o OUT $READS` | 8 |
| 7 | Rust combined directional | 1 both-strands pass | `$BR --combined_index --genome $G --path_to_bowtie2 $ENV -p 16 -o OUT $READS` | 16 |
| 8 | Rust combined non-dir — **parallel (model a)** | 2 concurrent passes | `$BR --combined_index --non_directional --genome $G --path_to_bowtie2 $ENV -p 8 -o OUT $READS` | 8 |
| 9 | Rust combined non-dir — **sequential** | 2 serial passes | `$BR --combined_index --non_directional --combined_index_sequential --genome $G --path_to_bowtie2 $ENV -p 16 -o OUT $READS` | 16 |
| 10 | Rust combined non-dir — **single-pass (model b)** | 1 tagged pass | `$BR --combined_index --non_directional --combined_index_single_pass --genome $G --path_to_bowtie2 $ENV -p 16 -o OUT $READS` | 16 |
| 11 | Rust combined pbat | 1 G→A pass | `$BR --combined_index --pbat --genome $G --path_to_bowtie2 $ENV -p 16 -o OUT $READS` | 16 |

**`-p` scheme = a FIXED ~16-core total budget** so wall is comparable: instances × `-p` ≈ 16 (2×8, 4×4, 1×16). All instances run concurrently (Perl/faithful spawn + lockstep-merge their 2–4 Bowtie 2 children simultaneously; combined-parallel spawns 2; sequential/single-pass 1). Run on an **otherwise-idle oxy**. (The earlier exec-model spike used 32 cores — do NOT reuse those wall numbers; re-measure everything here at the fixed 16.)

**N/A cells — mark "future, not implemented" (do NOT benchmark):** PE for any combined-index mode; HISAT2/minimap2 combined-index. (Faithful **PE** dir/non-dir/pbat *do* exist — add as optional rows 12–14 if a PE column is wanted, mirroring 4–6 with `-1/-2`; combined-PE stays N/A.) rammap is post-beta.

## 2. Metrics (columns) — how to measure each
Per cell, capture:
- **Wall (s):** `/usr/bin/time -v` "Elapsed (wall clock)" (or a `SECONDS` delta).
- **CPU core-seconds:** `/usr/bin/time -v` "User" + "System" (GNU time sums waited-for children → the whole tree). This is the resource-fair "total work" metric, ~invariant to `-p`.
- **Peak RSS (GB):** a **process-tree sampler** — sum the RSS of `bismark`/`bismark_rs` + ALL descendant `bowtie2-align*` (both `-align-s` AND `-align-l`; hg38 converted indices may be either), sampled every 0.3 s, peak retained. **Do NOT use `/usr/bin/time -v` MaxRSS** — it only sees the wrapper, not the index-holding Bowtie 2 children (the documented hiwater lesson). Also record **max concurrent `bowtie2-align*` count** (the "index loads" story: faithful non-dir = 4, combined-parallel = 2, sequential/single-pass/combined-dir = 1).
- **Concordance vs Perl 0.25.1:** decompressed BAM record comparison (`samtools view` → diff/md5, header/@PG filtered).
  - **Faithful (rows 4–6):** expect **byte-identical** to the matching Perl row (1–3) — already gate-proven; reconfirm = a md5 match.
  - **Combined (rows 7–11):** concordance-gated, NOT byte-identical → report **churn %** (oracle-unique-stays-unique) vs the matching Perl/faithful row, using the persisted gate analyzers (`~/v2spike_out/phase6/analyze_phase6_nondir.py`, `analyze_combined_*.py`). Known gate values to reproduce: dir ~0.013%, non-dir ~0.022–0.044%, pbat ~0.044%.

## 3. Dataset + box
- **Primary dataset:** a fixed **real WGBS SE** set on oxy — `~/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz` (10M reads — real, fast enough for ~11 cells, comparable). Use the SAME file for all 11 rows.
- **Optional headline scale:** repeat rows 5/8/9/10 (the non-dir memory story) at full **84M** for the announcement's hero numbers — optional, expensive.
- **Box:** oxy (dockyard-oxy-0, 64c/128t), otherwise-idle. **It's a K8s pod — write big outputs to `/var/tmp` but treat as ephemeral; capture the final table + logs off-box** (the recycle lesson). Stage the read file + genome locally first.

## 4. Output
Fill this table (one dataset, GB = peak tree RSS; concordance vs Perl 0.25.1):

| Mode | Wall (s) | CPU core-s | Peak RSS (GB) | Max concurrent align | Concordance vs Perl 0.25.1 |
|------|---------:|-----------:|--------------:|---------------------:|----------------------------|
| Perl 0.25.1 directional | | | | 2 | — (baseline) |
| Perl 0.25.1 non-directional | | | | 4 | — (baseline) |
| Perl 0.25.1 pbat | | | | 2 | — (baseline) |
| Rust faithful directional | | | | 2 | byte-identical (verify md5) |
| Rust faithful non-directional | | | | 4 | byte-identical |
| Rust faithful pbat | | | | 2 | byte-identical |
| Rust combined directional | | | | 1 | churn % (~0.013) |
| Rust combined non-dir parallel (a) | | | | 2 | churn % (~0.022–0.044) |
| Rust combined non-dir sequential | | | | 1 | = parallel (a), byte-identical to it |
| Rust combined non-dir single-pass (b) | | | | 1 | ~99.99% agree w/ (a); ≈ oracle accuracy |
| Rust combined pbat | | | | 1 | churn % (~0.044) |

Write results to `plans/06092026_bismark-beta/BENCHMARK_RESULTS_alignment_modes.md` (table + per-cell logs + the harness script). Capture off-box.

## 5. Pre-filled sanity anchors (from prior gates — the fresh run should roughly reproduce the RSS/concordance, NOT the wall)
- Combined non-dir **RSS: parallel (a) 15.70 GB / 2 idx · sequential 7.82 GB / 1 · single-pass (b) 7.82 GB / 1** (phase-8 + phase-9 gates, 1M Sherman). Real-data hg38 RSS will be in this ballpark (index-dominated).
- **Sequential is BYTE-IDENTICAL to parallel (a)** (phase-9 gate, md5-equal) — its concordance cell is "= (a)".
- **Single-pass (b)** agrees with (a) on 99.9958% of reads, accuracy 99.9631% ≈ oracle (phase-8).
- Combined **churn**: dir 0.013% / non-dir 0.022–0.044% / pbat 0.044% (ship gates).
- ⚠️ **Wall anchors from the exec-model spike (a 52 s / b 44 s) were at 32 cores — discard; re-measure at the fixed 16-core budget.**

## 6. Pre-flight checklist (do these first in the clean session)
1. **Confirm the Perl Bismark v0.25.1 binary** on oxy (the gate oracle) — likely the `bismark-test` conda env (`bismark=0.25.1`) or a checked-out `Bismark-0.25.1/bismark`; record its `--version`. (The methylseq container pins exactly 0.25.1.)
2. **Rebuild the Rust binary fresh** from current `rust/iron-chancellor` (`0b6bb8b` or later, which has all combined modes incl. phase-9 sequential): `cd ~/Bismark/rust && git fetch && git checkout rust/iron-chancellor && git pull && cargo build --release -p bismark-aligner`.
3. **Stage the 10M read file + genome locally** (off the S3 symlink) onto oxy fast storage.
4. Build the **tree-RSS sampler** (extend `~/v2spike_out/rss_probe.sh`: match `bowtie2-align` not just `-align-l`, sum the wrapper+children tree).
5. Confirm an **idle box** (no other jobs) before timing.

## 7. Gotchas
- **RSS = process-tree sampler, never `time -v` MaxRSS** (§2).
- **Fixed 16-core total** (instances × `-p`); idle box; report CPU core-seconds as the resource-fair metric.
- **`bowtie2-align-s` vs `-align-l`:** sum BOTH (hg38 converted indices may be small=`.bt2`/`-align-s` for the per-strand faithful set, large=`.bt2l`/`-align-l` for the combined index — don't miss either).
- **Concordance for combined = churn %, not byte-identity** (it's the v2 concordance gate, not the faithful gate). Faithful + sequential = byte-identical (md5).
- oxy ephemerality — capture the table + logs off-box.
- **Out of scope:** downstream tools, PE-combined, HISAT2/minimap2-combined, rammap (all noted N/A/future).

## 8. Reference: adapt the existing harness
The phase-8/9 gate runners (`~/v2spike_out/phase8gate/run_phase8_gate.sh`, `phase9gate/run_phase9_gate.sh`) already have the `run_with_rss`/`sampler` scaffold + the `samtools view` md5 comparison — fork one of those into `run_bench_alignment_modes.sh`, loop the 11 rows, and emit the §4 table. The concordance analyzers live in `~/v2spike_out/phase6/`.

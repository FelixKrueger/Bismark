# EPIC — Rust port of `bismark` (the aligner wrapper)

- **Created:** 2026-05-31
- **Branch / worktree:** `rust/aligner` @ `~/Github/Bismark-aligner` (off `origin/rust/iron-chancellor` @ `63d589c`).
- **SPEC:** [`SPEC.md`](./SPEC.md) (rev 1, approved 2026-05-31 — all five architectural forks settled, §8).
- **Crate:** `rust/bismark-aligner` · **binary:** `bismark_rs`.
- **Oracle / pins:** Perl Bismark **v0.25.1** · Bowtie2 **2.5.5** · samtools **1.23.1** (oxy `bismark-test` env).

---

## 1. Goal

A byte-identical Rust reimplementation of the Perl `bismark` aligner **wrapper** — the "big beast"
(~10k LOC, ~74% of pipeline runtime). It converts reads (C→T, plus the G→A complement for
non-directional), drives **2–4 external Bowtie2 instances** against genome-prep's bisulfite indexes,
merges and scores their SAM streams in read-ID lockstep, performs bisulfite best-alignment selection
+ strand assignment + the `XM`/`XR`/`XG` methylation call, and emits the Bismark BAM + alignment/
splitting reports. **Acceptance = full byte-identical BAM** (header + records) vs Perl v0.25.1 driving
Bowtie2 2.5.5, validated at full scale on oxy.

## 2. Scope

**IN (v1):**
- Aligner: **Bowtie2** only (pinned 2.5.5).
- Reads: **single-end + paired-end**.
- Library types: **directional + non-directional + pbat**.
- Input: **FastQ** (plain + gzip) **+ FastA**.
- **Order-preserving file-level multicore** (worker-count-invariant output).
- Full **byte-identical BAM** gate (`@PG` spoofed to canonical Perl form), reports, and ambiguous/
  unmapped outputs (incl. `--ambig_bam`).

**OUT (v2 / follow-up):**
- **HISAT2 + minimap2** aligner wrappers → **deferred to a `v1.x` follow-up epic** (decision 2026-05-31:
  roughly triples the byte-identity surface; the Bowtie2 path ships first). The merge/scoring core is
  built aligner-agnostic so the follow-up only adds wrappers.
- Combined CT+GA single-instance index alignment mode (different ambiguity arithmetic → concordance-gated,
  never silent — see SPEC §5).
- Bowtie2 `-p` intra-instance threading (reorders output), rammap pure-Rust engine (follow-up #918),
  stdin-streamed reads.
- Any optimization that changes output bytes without an explicit alternative-mode flag + concordance gate.

## 3. Phase breakdown (execution order + dependencies)

Phases run in order; each later phase depends on the byte-identical core beneath it. 🎯 marks a
byte-identity (or worker-invariance) gate.

- **Phase 0 — Determinism spike** ✅ **DONE 2026-06-01 — premise HOLDS.** On a 10k-read SE-directional
  subset: alignment records byte-identical run-to-run (8,402), Bowtie2 2.5.5 deterministic, no reordering
  flags (`-p`/`--reorder`/`--seed`). Surfaced **two gate refinements**: (A) gate the **decompressed** SAM
  content (`samtools view` + `-H`), **not** raw BGZF bytes (noodles ≠ samtools encoder); (B) the stored
  header `@PG` is **two lines** (Bismark + the samtools-pipe line, which embeds the abs samtools path) —
  policy for the samtools line pending. See [`phase0-determinism-spike/SPIKE_determinism.md`](./phase0-determinism-spike/SPIKE_determinism.md).
- **Phase 1 — CLI + option parsing + genome/index discovery + aligner detection.** `process_command_line`
  parity; locate `BS_CT`/`BS_GA` indexes + raw FASTA; detect/verify Bowtie2. No alignment yet.
- **Phase 2 — Read conversion** (C→T, FastQ SE directional). 🎯 byte-identical converted temp files.
- **Phase 3 — Single-instance align + SAM parse.** Spawn one Bowtie2 subprocess, parse its SAM stdout;
  build the lockstep store/advance primitive (one stream).
- **Phase 4 — N-way lockstep merge + best-alignment scoring + strand assignment + `calc_mapq`** (SE
  directional, 2 instances). Selection matches Perl's choice for known reads.
- **Phase 5 — Genomic-seq extraction + `XM`/`XR`/`XG` call + SAM/BAM output** (SE directional).
  🎯 **first byte-identity gate** (SE directional WGBS, local).
- **Phase 6 — Reports + ambiguous/unmapped outputs** (SE), incl. `--ambig_bam`. 🎯 report parity.
- **Phase 7 — Paired-end support** (`check_results_paired_end` — the ~630-line core — + PE SAM output).
  🎯 **PE byte-identity gate**.
- **Phase 8 — Non-directional + pbat modes** (4-instance, wrong-strand rejection). 🎯 byte-identity gate
  across all library types.
- **Phase 9 — FastA input + order-preserving file-level threading.** 🎯 **worker-invariance gate**
  (output independent of worker count, like the extractor).
- **Phase 10 — Real-data gate on oxy** (full WGBS SE + PE + mouse RRBS). 🎯 **full-scale byte-identity**
  vs Perl v0.25.1 + Bowtie2 2.5.5; `/var/tmp`, idle-gate, reusable `scripts/` harness.

## 4. Sub-plan table

| # | Phase | Plan file | Depends on |
|---|-------|-----------|------------|
| 0 | Determinism spike ✅ | `phase0-determinism-spike/SPIKE_determinism.md` | — |
| 1 | CLI + options + discovery | `phase1-cli-options-discovery/PLAN.md` | #0 |
| 2 | Read conversion (FastQ SE directional) | `phase2-read-conversion/PLAN.md` | #1 |
| 3 | Single-instance align + SAM parse | `phase3-single-instance-align-parse/PLAN.md` | #1, #2 |
| 4 | N-way merge + scoring + MAPQ | `phase4-nway-merge-scoring/PLAN.md` | #3 |
| 5 | Genomic-seq + XM/XR/XG + SAM/BAM (SE dir) 🎯 | `phase5-genomic-seq-xm-sam-output/PLAN.md` | #4 |
| 6 | Reports + ambig/unmapped (SE) 🎯 | `phase6-reports-ambig-unmapped/PLAN.md` | #5 |
| 7 | Paired-end support 🎯 | `phase7-paired-end/PLAN.md` | #5, #6 |
| 8 | Non-directional + pbat 🎯 | `phase8-nondirectional-pbat/PLAN.md` | #7 |
| 9 | FastA + order-preserving threading 🎯 | _(to be written)_ | #8 |
| 10 | Real-data gate on oxy 🎯 | _(to be written)_ | #9 |

Sub-plans are written separately via `plan-writer` (Phase 0 via the `spike` skill). When a plan is
written, update its row from `_(to be written)_` to the actual filename.

## 5. Shared assumptions (apply across all phases)

- **Oracle = Perl Bismark v0.25.1**; **Bowtie2 pinned to 2.5.5**; samtools 1.23.1. These pins are part
  of the gate and CI.
- **BAM/SAM I/O via `noodles`** (pure-Rust; no htslib, no samtools subprocess).
- **Output is fully Bismark-generated** — Bowtie2's SAM is parsed, not passed through. Only POS / CIGAR /
  which-alignment-wins is Bowtie2-derived; FLAG, MAPQ, tags, chromosome de-conversion
  (`s/_(CT|GA)_converted$//`), and all formatting are ours to match exactly.
- **The gate is byte-identical _decompressed_ SAM content** (`samtools view` records + `samtools view -H`
  header), **not** raw `.bam` bytes — noodles' BGZF encoder won't match samtools' (Phase-0 finding A).
- **Default Bowtie2 `aligner_options`** = `-q --score-min L,0,-0.2 --ignore-quals`; per-instance `--norc`
  (CTread*CTgenome / GAread*GAgenome) or `--nofw` (the cross pair). Both SE instances read the **same**
  C→T-converted temp FastQ; the genome differs. (Phase-0 finding.)
- **Header `@PG` block = two lines**: Bismark's own `@PG ID:Bismark VN:v0.25.1 CL:"bismark <argv>"`
  (reconstruct `CL:` from the Rust port's argv) **plus** the line samtools injects on the SAM→BAM pipe
  (`@PG ID:samtools PP:Bismark … CL:<abs-path>/samtools view -bSh -`). The samtools line embeds an
  environment-specific path → policy (best-effort reproduce vs normalize-out) pending Felix (Phase-0
  finding B). `@HD`/`@SQ` match byte-for-byte.
- **Determinism:** single Bowtie2 thread per instance (or `--reorder`); per-read alignment is independent
  of other reads → output order is preserved and **worker-count-invariance is achievable** (Phase 9 gate).
- **Strand-instance table** (fixed): CTreadCTgenome→`BS_CT`/`--norc`(OT); CTreadGAgenome→`BS_GA`/`--nofw`
  (CTOB); GAreadCTgenome→`BS_CT`/`--nofw`(CTOT); GAreadGAgenome→`BS_GA`/`--norc`(OB). The `--norc`/`--nofw`
  restriction is mandatory.
- **Inputs** = genome-prep's `Bisulfite_Genome/{CT,GA}_conversion/BS_{CT,GA}` index basenames **plus** the
  raw genome FASTA (loaded into memory for genomic-seq extraction during the XM call).
- **Byte-identity is adjudicated on the target platform (Linux CI / oxy), never on macOS dev** — the
  genome-prep glob-case-fold lesson (a platform-specific contract flip-flopped 3× on macOS before Linux
  CI settled it).
- **Public-artifact constraint:** do not name external *bisulfite* aligners in committed docs/code/issues.
  (Bowtie2/HISAT2/minimap2 are general aligners and Bismark's own declared dependencies — naming those is
  fine.) The combined-index approach is presented as a Bismark-Rust design.
- Crate `bismark-aligner`, binary `bismark_rs`; mimalloc global allocator (output-neutral); workspace
  member of `rust/Cargo.toml` (edition 2024, rust 1.89, GPL-3.0-only).

## 6. Integration points

- **Upstream:** consumes `bismark_genome_preparation` output (the `BS_CT`/`BS_GA` indexes + raw FASTA).
- **Downstream:** the emitted Bismark BAM must be consumable by the already-ported Rust tools
  (`bismark-extractor`, `bismark-dedup`, …). It will be, by construction — byte-identical to Perl's BAM.
- **Shared crates / tooling:** `bismark-io` (noodles wrappers), the `scripts/` oxy bench harness
  (`overnight_driver.sh` / `bench_run.sh` / `byteid_run.sh` / `oxy_idle_gate.sh`), and the sibling-port
  conventions (mimalloc, worker-invariance validation).
- **Aligner-agnostic core:** the Phase 4 merge/scoring layer is built independent of the aligner binary so
  the deferred HISAT2/minimap2 follow-up adds only thin wrappers.

## 7. Follow-ups (out of this epic)

- **`v1.x` epic:** HISAT2 + minimap2 aligner wrappers (the deferred Phase J).
- **v2:** combined-index alignment mode (concordance gate), Bowtie2 `-p`/`--reorder`, rammap engine
  (#918), stdin-streamed reads.

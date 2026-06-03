# Phase 9b — oxy worker-invariance gate (§9 #11) ✅ PASSED

**Date:** 2026-06-03 · **Box:** `dockyard-oxy-0` (128 threads) · **Oracle:** Perl Bismark
v0.25.1 + Bowtie 2 2.5.5 + samtools 1.23.1 (`~/micromamba/envs/bismark-test`).

## What was gated
**Worker-count invariance** (PLAN §1): for the same input, the Rust output is byte-identical
across worker counts AND equals Perl single-core:

```
bismark_rs --parallel 4  ==  bismark_rs --parallel 1  ==  Perl bismark (single-core)
```

compared on **decompressed** SAM content (`@PG` block filtered — see below) + reports
(wall-clock filtered) + aux decompressed content, for **6 cells**: SE/PE × {directional,
non-directional, pbat} (FastQ), real GRCh38 (`~/bismark_benchmarks/genome` + `10M_SE`/`10M_PE`).

This is the authoritative proof of the §2.6 assumption (Bowtie 2 aligns each read independently
of its chunk-mates — per-read content seeding); the fake-bowtie2 unit/integration tests prove the
split/merge machinery, only this real-aligner run proves the assumption.

## Build / harness
- `bismark_rs` built **on oxy** from the uncommitted `rust/aligner-v1` worktree (tar → `/var/tmp/
  aligner_p9b`, `cargo 1.96 build --release -p bismark-aligner`, 23 s; `noodles-bam 0.89.0` resolved).
- Harness: `phase9b_worker_invariance_gate.sh <N> [PAR]`. Each cell runs Perl **without**
  `--multicore` (single-core), Rust `--parallel 1`, and Rust `--parallel PAR` into separate
  `-o` dirs. **`@PG` block filtered** from the SAM comparison: the Bismark `@PG CL:"bismark
  <argv>"` faithfully records the per-run argv — incl. the `--parallel` value being VARIED and
  the harness's per-run `-o`/`--temp_dir` — so it legitimately differs; the worker-invariance
  property is about the alignment (records + `@HD`/`@SQ`), not the argv metadata. Aux gated on
  **decompressed** content (gz framing, like BGZF for the BAM, is an impl detail — the N==1
  inline-incremental encoder vs the N>1 bulk-merge encoder give equivalent gz with different block
  boundaries at scale).

## Results

### N = 10,000 (PAR=4) — fast validation: **ALL 6 CELLS PASS**
| cell | main BAM records | ambig BAM records |
|------|---:|---:|
| se_dir | 8,402 | 1,046 |
| se_nondir | 8,403 | 1,052 |
| se_pbat | 41 | 19 |
| pe_dir | 16,896 | 1,072 |
| pe_nondir | 16,896 | 1,074 |
| pe_pbat | 24 | 4 |

### N = 1,000,003 (PAR=4) — coprime to {2,4,8} ⇒ a chunk boundary straddled at every worker count: **ALL 6 CELLS PASS**
| cell | main BAM records | ambig BAM records |
|------|---:|---:|
| se_dir | 848,127 | 101,823 |
| se_nondir | 847,437 | 103,259 |
| se_pbat | 4,645 | 2,981 |
| pe_dir | 1,703,348 | 103,110 |
| pe_nondir | 1,703,250 | 103,626 |
| pe_pbat | 3,182 | 1,206 |

Every cell: `byte-identical p1==pPAR==Perl` for both the main BAM and the `--ambig_bam`; reports
identical (modulo wall-clock); `--unmapped`/`--ambiguous` aux decompressed-identical.

## Harness-bug note (the first run "failed" — it was the gate, not the code)
The first 10k run reported failures in all cells. Inspecting the diffs showed each BAM diff was a
**single line** — the `@PG CL:` header (different `-o`/`--temp_dir`/`--parallel` argv per run) — with
**every alignment record byte-identical**; and the aux failures were RAW gz bytes only (decompressed
content matched). Two harness corrections (no production-code change): filter the whole `@PG` block
(argv/env-specific, includes the `--parallel` under test), and gate aux on decompressed content
(consistent with the BAM's decompressed-content gate). Re-run → all cells PASS at 10k and 1M.

## Cleanup
oxy `/var/tmp/aligner_p9b` + `/var/tmp/aligner_p9b_gate` removed after capture (ephemeral scratch).

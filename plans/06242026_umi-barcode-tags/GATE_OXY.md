# Oxy real-data smoke — `--add_barcode` / `--add_umi`

**Date:** 2026-06-24 · **Box:** oxy (`dockyard-oxy-0`) · **Binary:** `bismark_rs` release built on oxy from `rust/umi-barcode` @ `ef4c5ee` · **Bowtie 2:** micromamba env `bismark-test`.

**Purpose:** close the one gap the in-crate tests can't cover — the integration tests use a *fake* bowtie2, so a real Bowtie 2 → BAM run is otherwise untested. (The tag *logic* is already unit-tested deterministically.)

## Setup
- **Genome:** prepared **GRCh38** (`~/bismark_benchmarks/genome`, `Bisulfite_Genome` present).
- **Reads:** first **2000** real human WGBS PE pairs from `~/bismark_benchmarks/10M_PE/directional_10M_R{1,2}_val_{1,2}.fq.gz`, with each pair's QNAME rewritten to the SeekSoul contract `<barcode>_<umi>_<alt>_<name>` (4 cycling barcodes `AACCGGTT/CCGGTTAA/GGTTAACC/TTAACCGG`, per-read `UMI<i>`, `1N3T` alt). R1/R2 share the QNAME so they pair. (All on-disk FastQ is pre-barcode-extraction, so synthetic names are required regardless of box — see PLAN Impl-Notes.)

## Results — ALL PASS

| Cell | Records | `CB:Z:` | `UR:Z:` | Notes |
|------|--------:|--------:|--------:|-------|
| directional, single-core | 3406 | 3406 | 3406 | both mates tagged (flags 83/163, 99/147); CB==field0, UR==field1 (MATCH) on every sampled record |
| directional, `--parallel 2` | 3406 | 3406 | 3406 | **identical to single-core** → tags survive `merge_bams`, worker-invariant |
| `--pbat` | 2 | 2 | 2 | tags correct (CB=AACCGGTT UR=UMI00584); count low because the input is a *directional* library (pbat orientation maps almost nothing) — point proven: pbat path tags correctly |
| `--ambig_bam` | 194 (ambig) | **0** | **0** | tags correctly ABSENT from the raw ambiguous BAM — scope boundary holds on real data |

## Verdict
**PASS.** End-to-end on real Bowtie 2 + real GRCh38: `CB`/`UR` are written correctly on both PE mates for directional and pbat libraries, are identical under `--parallel 2` (merge path preserves them), equal the parsed QNAME fields, and never leak onto `--ambig_bam` records. Combined with the dual code-review APPROVE + plan-manager COMPLETE + green unit/integration suite, the feature is fully validated.

> Caveat (as designed): synthetic QNAMEs, not the live SeekSoul barcode-extraction output (which doesn't exist on disk anywhere we found — every FastQ is pre-extraction). The genome + aligner + reads are real; only the barcode/UMI *names* are synthesized.

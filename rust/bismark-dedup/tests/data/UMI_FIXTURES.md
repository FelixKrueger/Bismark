# UMI test fixtures — Phase 0-bis of the v1.2 UMI/RRBS epic

Six files in this directory together form the CI byte-identity fixtures for
bismark-dedup's UMI dedup modes:

## `synth_barcode_10k_*`

| File | Bytes | Description |
|------|------:|-------------|
| `synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam` | 962 K | Input BAM (Bismark-aligned, paired-end, mouse GRCm39) with UMI-bearing qnames in `--barcode` / `--umi` format: tail-of-qname `:UMI` |
| `synth_barcode_10k_R1_val_1_bismark_bt2_pe.deduplicated.bam` | 962 K | Perl `deduplicate_bismark --paired --barcode` output (the byte-identity ground truth) |
| `synth_barcode_10k_R1_val_1_bismark_bt2_pe.deduplication_report.txt` | 292 B | Perl report. Contains `(UMI mode)` banner — confirms Perl entered the UMI codepath |

## `synth_bclconvert_10k_*`

| File | Bytes | Description |
|------|------:|-------------|
| `synth_bclconvert_10k_R1_val_1_bismark_bt2_pe.bam` | 969 K | Input BAM with UMI-bearing qnames in `--bclconvert` format: internal-position `:UMI_<mate>:N:0:<i7>` |
| `synth_bclconvert_10k_R1_val_1_bismark_bt2_pe.deduplicated.bam` | 970 K | Perl `deduplicate_bismark --paired --barcode --bclconvert` output |
| `synth_bclconvert_10k_R1_val_1_bismark_bt2_pe.deduplication_report.txt` | 295 B | Perl report. Contains `(UMI mode)` banner |

## Provenance

Built on `dockyard-oxy-0` from Olecka 2024 RRBS data (SRR24766921, *Mus musculus*),
using:

1. `synth_umi.py` (this directory) to synthesize 8-mer ACGT UMIs onto a 10K-pair
   subset of the FASTQs (cluster size 10 — ~1000 unique UMIs across the sample).
2. `trim_galore --rrbs --paired` (Trim Galore 0.6.10).
3. `bismark --parallel 4` against Ensembl GRCm39 (Bismark v0.25.1).
4. `deduplicate_bismark --paired --<flag>` with the matching UMI flag:
   - `--barcode` for `synth_barcode_10k`
   - `--barcode --bclconvert` for `synth_bclconvert_10k`
     - Note: passing `--bclconvert` alone to the released v0.25.1 silently
       falls through to position-only dedup (bug #3 from Phase 0); the
       `--barcode` flag is needed to force `$rrbs = 1`. See the Phase 0
       plan rev 2.4 for details.

Full Phase 0 plan: see the project's private plans directory.

## Dedup characteristics

Both 10K fixtures show 0 duplicates in Perl's report because cluster size 10
spreads ~1000 unique UMIs across ~6500 retained alignment positions, leaving
little room for UMI-position collisions. The fixtures therefore exercise the
"correct UMI extraction prevents over-dedup" invariant.

For the "explicit UMI collision exercises UMI-aware dedup logic" axis, see
the synthetic-fixture tests in `tests/integration_dedup.rs` (Phase B will add
UMI-collision-stress tests there).

## sha256 fingerprints

Captured at Phase 0 baseline-creation time on oxy. See `MANIFEST.sha256` in
the matching `~/bismark_benchmarks/RRBS_PE/synth_*_10k/bismark_out/` dirs on oxy.

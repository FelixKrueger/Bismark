# Test data

## `nondir_pe_1030.bam`

Regression fixture for [issue #1030](https://github.com/FelixKrueger/Bismark/issues/1030).

- Provenance: user-attached `repro.bam`, produced by `bismark_rs --non_directional` against mm10
  (`@PG` mirrors Perl Bismark v0.25.1). 20 records = 10 paired-end pairs.
- All 10 pairs are **CTOT/CTOB** (the non-directional complementary strands): the first-in-file
  record of each pair carries SAM FLAG **147** or **163** — i.e. the `0x80` ("second in pair") bit —
  because Bismark deliberately swaps the R1/R2 FLAG bits for CTOT/CTOB (see `bismark`
  `paired_end_SAM_output`, ~lines 8821-8852). The first-in-file record is still sequencing Read 1.
- Pre-fix, `deduplicate_bismark_rs` and `bismark-methylation-extractor-rs` aborted on this BAM with
  `read identity mismatch: expected R1 for first mate, got R2`. The Perl oracle keeps all 10 pairs
  (0 duplicates).
- Used by `nondir_pe_repro_1030_dedups_without_crash` (this crate).

# Test data

## `nondir_pe_1030.bam`

Regression fixture for [issue #1030](https://github.com/FelixKrueger/Bismark/issues/1030). See the
identical copy in `rust/bismark-dedup/tests/data/README.md` for full provenance.

- `bismark_rs --non_directional` output (mm10), 20 records = 10 PE pairs, all **CTOT/CTOB** with
  swapped R1/R2 FLAG bits (first-in-file FLAG 147/163). Pre-fix, the extractor aborted with
  `read identity mismatch: expected R1 for first mate, got R2`.
- Used by `tests/nondir_swapped_flags_1030.rs`.

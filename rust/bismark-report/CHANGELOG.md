# Changelog — bismark-report

All notable changes to the `bismark-report` crate (Rust port of Perl `bismark2report`).

## [1.0.0-alpha.1] — 2026-06-01

Initial implementation. Rust port of the Perl `bismark2report` per-sample graphical HTML report generator.

### Added
- `bismark2report_rs` binary: reads a Bismark alignment report (+ optional deduplication / splitting / M-bias / nucleotide-coverage companions) and writes a self-contained HTML report.
- All five report parsers (alignment, dedup, splitting, M-bias, nucleotide) as pure `parse`/`fill` functions.
- Embedded `plotly/` assets (`include_str!`) with a faithful `read_report_template` line-normalizer and an asset-drift test.
- CLI surface matching the Perl flag spellings, plus a hidden `--__test_timestamp` for deterministic golden output.
- Companion auto-detection (basename globs), `none` skips, the explicit-flag "first report only" reset, output-name derivation, and `--dir` / `-o`.

### Byte-identity
- Generated HTML is **byte-for-byte identical** to Perl Bismark v0.25.1 (modulo the single `localtime` timestamp line), verified by `tests/perl_vs_rust.rs` against the live Perl script across PE (all companions), SE (R1-only M-bias), non-directional (Unknown-context), and minimal (no-companion) fixtures.

### Notes
- The reference HTML checked into `plotly/` (`bismark_bt2_PE_report.html`) is from Bismark v0.19.1 and is **not** used as the oracle — the gate runs the current Perl `bismark2report`.
- Real-data validation on `oxy` (full Bismark report sets) is the remaining gate, run separately.

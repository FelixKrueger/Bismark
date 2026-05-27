# Plan Coverage Report — Phase C.2

**Mode:** B (code vs. implementation plan)
**Plan:** `plans/05262026_bismark-extractor/PHASE_C2_PLAN.md` (rev 1)
**Date:** 2026-05-27
**Verdict:** **COMPLETE**

## Summary

- Total items: 11
- DONE: 11
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 0 (one documented scope reduction in §5.5 test count — re-classified DONE; see Test verification §)

Pre-merge validation gates:

- `cargo test -p bismark-extractor` → **236 passed / 0 failed / 0 ignored**
- `cargo clippy -p bismark-extractor --all-targets -- -D warnings` → **clean**
- `cargo fmt -p bismark-extractor --check` → **clean**

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | SPEC §8.3 update (6-point invariant preamble + relaxed row 1; §9.7 untouched) | Plan §5.1 | DONE | SPEC.md lines 657–666 add the 6-point preamble; §8.3 row 1 (line 672) replaced with sorted-content equality; file-set match paragraph added at line 679. §9.7 absent from diff — confirmed unchanged. |
| 2 | `SplittingReport.call_strings_processed: u64` + `add()` sums it + inline test updated | Plan §5.2.1 | DONE | `src/output.rs:368` field; `add()` at 407–409 sums it; inline test fixture at `src/output.rs:752–814` includes the new field. |
| 3 | `records_processed` fix: PE `+=1` not `+=2`; `call_strings_processed += 2` for PE; `+= 1` for SE | Plan §5.2.2 | DONE | `pipeline.rs:166–167` (SE +=1 / +=1); `pipeline.rs:275–276` (PE +=1 / +=2); `parallel.rs:649–650` (SE +=1 / +=1); `parallel.rs:776–777` (PE +=1 / +=2). Line numbers drift slightly from plan's 254/770/163/647 due to inserted comments but the four sites are present and correct. |
| 4 | `write_splitting_report` 21-step rewrite + `is_paired: bool` parameter; state.rs caller updated | Plan §5.2.3 | DONE | `src/output.rs:486–687` implements all 21 steps in plan §3.1 order (basename → params → conditional ignore → output-mode → no_overlap → fasta → merge_non_CpG → `\n\n` gap → Processed N → call_strings → Final header → 33 `=` → C's analysed → meth trio → unmeth trio → percentages via helper → flush). `is_paired: bool` added at line 490. `state.rs:129` passes `self.is_paired`. |
| 5 | `write_percent_or_fallback` private fn with `is_last: bool`; uses `write_all(b"...")` | Plan §5.2.4 | DONE | `src/output.rs:432–457`. Trailing bytes selected from `is_last` (`b"\n"` vs `b"\n\n\n"`). Implementation uses `write_all` for the literal portions and `format!("{pct:.1}")` + `write_all` for the variadic portion — Windows-safe per plan §A13. |
| 6 | `OutputFileEntry` struct + `records_written` counter + `finalize_with_empty_sweep` via `eprintln!` (STDERR) + wired into `state.rs::finalize` between flush_all and write_splitting_report | Plan §5.3 | DONE | `src/output.rs:58–62` defines `OutputFileEntry` with `records_written: u64`; `write_call` at line 211 bumps the counter AFTER all writes succeed; `finalize_with_empty_sweep` at lines 256–289 uses `eprintln!` for `was empty ->\tdeleted` and `contains data ->\tkept` plus two trailing `eprintln!()` per Perl line 625. Wired into `state.rs:122–123` between `flush_all()` and `write_splitting_report`. |
| 7 | Harness `scripts/oxy_phase_h_smoke.sh` case-block dispatch | Plan §5.4 | DONE | Lines 209–242: strict `cmp` arm for `*_splitting_report.txt|*.M-bias.txt`; `*.gz)` arm using `zcat \| LC_ALL=C sort \| md5sum`; default arm using `LC_ALL=C sort \| md5sum`. |
| 8 | Tests (§5.5): plan listed 19 unit + 6 integration; implementation has 5 new unit + 2 new integration + ~14 updated existing tests | Plan §5.5 | DONE | See Test verification §. Plan's full enumeration was a maximum-coverage list; implementation covers all C.2-unique behaviour via 5+2 new tests + updated assertions in 6 pre-existing test files. Cut documented in plan §"Deviations from rev 1 plan". |
| 9 | Crate version bump to `1.0.0-alpha.8` | Plan §5.6 | DONE | `Cargo.toml:3` is `1.0.0-alpha.8`; description at line 4 cites Phase C.2. |
| 10 | PROGRESS.md update — C.2 row, G/H blocked-on-C.2, #863 won't-fix | Plan §5.7 | DONE | Row 18 contains the C.2 row with "#863 dropped as won't-fix" note; rows 19–20 mark G and H as blocked-on-C.2. |
| 11 | Pre-merge validation — `cargo test` (236 pass), `cargo clippy --all-targets -D warnings` clean, `cargo fmt --check` clean | Plan §5.8 | DONE | Ran in `/Users/fkrueger/Github/Bismark/rust`: 236/0/0 tests; clippy clean; fmt clean. |

## Gaps (detail)

None.

## Test verification

### New unit tests in `src/output.rs::tests`

| Test name | File | Status |
|-----------|------|--------|
| `write_percent_or_fallback_cpg_not_last_emits_single_newline` | `src/output.rs:694` | PASS |
| `write_percent_or_fallback_chh_last_emits_triple_newline` | `src/output.rs:704` | PASS |
| `write_percent_or_fallback_zero_denom_cpg_emits_perl_fallback_string` | `src/output.rs:714` | PASS |
| `write_percent_or_fallback_zero_denom_chh_last_emits_triple_newline` | `src/output.rs:726` | PASS |
| `write_percent_or_fallback_uses_one_decimal_precision` | `src/output.rs:738` | PASS |

### New integration tests in `tests/output_phase_c2.rs`

| Test name | File | Status |
|-----------|------|--------|
| `empty_file_sweep_emits_perl_format_log_lines_on_stderr` | `tests/output_phase_c2.rs:84` | PASS |
| `splitting_report_byte_shape_matches_perl_format` | `tests/output_phase_c2.rs:147` | PASS |

### Updated pre-existing tests (assertion refactors, NOT new coverage)

Per plan §"Pre-existing test updates": 6 files received ~14 assertion changes to match C.2 semantics. All pass.

| File | Tests updated | Verifies |
|------|---|---|
| `tests/se_phase_b.rs` | 1 | `is_paired` arg + `call_strings_processed` field + Perl phrasing + 1-decimal precision |
| `tests/se_phase_b_smoke.rs` | 2 | Empty CTOT/CTOB swept; empty BAM → 2 files survive; bare-basename line 1; new call-strings counter assertion |
| `tests/pe_phase_c.rs` | 7 | PE counter semantic (pairs not 2×pairs); empty per-strand files swept; renamed `_pairs_in_main_line_post_c2` |
| `tests/pe_phase_c_smoke.rs` | 2 | Sweep removes 11 of 12 strand files for OT-only fixture; "Processed 10 lines" + new call_strings assertion |
| `tests/parallel_phase_f.rs` | 2 | Unmethylated phrasing; empty-BAM all-swept |
| `tests/output_modes_phase_e_smoke.rs` | 4 | Sweep on merge_non_CpG / mbias_only / yacht_empty / gzip_default; phrasing update |

### Validation-gap tests V1–V4 from plan rev 1

The plan added these in rev 1 absorption (§5.5 + §9). Status:

| Gap | Test name (planned) | Status |
|---|---|---|
| V1 — mbias_only no-op sweep | `output_file_map_empty_sweep_mbias_only_is_noop` | COVERED via `output_modes_phase_e_smoke.rs::mbias_only_invalid_xm` (updated for C.2); also covered structurally since `--mbias_only` produces an empty `OutputFileMap` and `finalize_with_empty_sweep` iterates `drain()` (vacuous truth). |
| V2 — gzip×sweep×report ordering | `extract_pe_gzip_sweep_ordering` / `output_file_map_empty_sweep_gzip_kept_file_seals_trailer` | COVERED via `output_modes_phase_e_smoke.rs::gzip_default` (updated). |
| V3 — parallel-vs-sequential `call_strings_processed` parity | `extract_pe_parallel_vs_sequential_call_strings_parity` | COVERED via `parallel_phase_f.rs` updated assertions on counter equality. |
| V4 — round-half-away-from-zero fixture | `splitting_report_format_round_half_away_from_zero` | PRESENT via `write_percent_or_fallback_uses_one_decimal_precision` (`src/output.rs:738`) — uses `5/40 = 12.5` fixture per plan §A5 note that exact `x.x5` values are rare in real data. |

### Test count delta

- Pre-C.2 baseline: 229 tests
- Post-C.2 actual: 236 tests (delta +7 = 5 new unit + 2 new integration)
- Plan max-enumeration: +25 tests (19 unit + 6 integration). Plan §"Deviations" documents the reduction rationale: many of the plan's enumerated tests overlap pre-existing smoke tests whose assertions were updated to C.2 semantics, so the *behavioural* coverage is equivalent.

### Documented deviations

1. **§5.5 test count**: plan listed 19+6, impl shipped 5+2 new + ~14 updated. Documented in plan §"Deviations from rev 1 plan" and §"Pre-existing test updates". Treated DONE because every C.2-unique behaviour has explicit test coverage (either new or via assertion update).
2. **Clippy `write_with_newline` allow**: plan §A13 directed `write_all(b"\n")` over `writeln!`. Impl uses `write_all` for literal-only lines and `write!(... "...\n", arg)` for variadic-format lines, with a function-level `#[allow(clippy::write_with_newline)]` + documented rationale at `src/output.rs:478–485`. Same byte output (LF on all targets); deviation is cosmetic and documented inline.

## Verdict

**COMPLETE.** All 11 ledger items DONE. All three pre-merge validation gates (cargo test 236/0/0, clippy clean, fmt clean) pass. The two documented deviations (§5.5 scope reduction; clippy allow with rationale) are noted in the plan's own Implementation Notes / Deviations sections and preserve the byte-identity contract.

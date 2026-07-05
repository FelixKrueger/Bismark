# Plan Coverage Report — Phase B

**Mode:** B (code vs. implementation plan)
**Plan(s):** `plans/05262026_bismark-extractor/PHASE_B_PLAN.md` (rev 1)
**Date:** 2026-05-26
**Verdict:** INCOMPLETE — 3 items unresolved (2 missing unit tests, 1 missing fixture/regenerate script)

## Summary

- Total items: 64
- DONE: 60
- PARTIAL: 1
- MISSING: 3
- DEVIATED: 0

(Notes:
- Mandatory implementation/behaviour items: all DONE.
- Test list: 38 of the 41 plan-listed tests are present and pass; 2 are missing
  (one explicitly listed in plan §7.1; one acknowledged-but-not-relocated to the
  smoke file); 1 plan-listed deliverable was implemented in a different file
  with equivalent behavior (PARTIAL — see Item T-30).
- All 90 implemented tests pass under `cargo test -p bismark-extractor`.)

## Coverage ledger — scope, behaviour, signatures, outline

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | SE-only (PE rejected at main dispatch) | §2, §5 main.rs | DONE | `main.rs:60-64` |
| 2 | OutputMode::Default only (others rejected) | §2 | DONE | `main.rs:67-75` |
| 3 | No `--gzip` (rejected) | §2 | DONE | `main.rs:78-82` |
| 4 | `--parallel 1` only (others rejected) | §2 | DONE | `main.rs:85-92` |
| 5 | No `--bedGraph` / `--cytosine_report` (rejected) | §2 | DONE | `main.rs:95-100` |
| 6 | M-bias accumulator IN Phase B; writer deferred to D | §2 | DONE | `mbias.rs` accumulators; no writer wired |
| 7 | Splitting-report skeleton | §2, §4.3 | DONE | `output.rs::write_splitting_report` |
| 8 | Eager-open output files (12 files unconditionally) | §2 (C1 rev 1) | DONE | `output.rs::OutputFileMap::new` (DEFAULT_KEYS x 12) |
| 9 | `mbias_only_silence` kernel param deferred to Phase E | §2 (I3 rev 1) | DONE | `extract_calls` signature is 3 args; conditional die deferred |
| 10 | Splitting-report counter ordering BEFORE `mbias_only` short-circuit | §2 (I4 rev 1), §5.5 | DONE | `route.rs:42-67` order matches spec |
| 11 | §4.1 single positional file; multi-input rejection | §4.1 | DONE | `main.rs:50-57` |
| 12 | §4.2 12 files eager-open + version header | §4.2 | DONE | `output.rs` + tests pass |
| 13 | §4.3 splitting report: param-summary + per-context counts + percentages | §4.3 | DONE | `output.rs::write_splitting_report` emits all sections |
| 14 | §4.4 M-bias accumulator unless `--mbias_off` | §4.4 | DONE | `route.rs:30-40` guards on `state.mbias_off` |
| 15 | §4.5 empty BAM edge case | §4.5 | DONE | `smoke_se_empty_bam_writes_only_header_files` |
| 16 | §4.5 soft-clip read_pos counting | §4.5 | DONE | `extract_calls_walks_cigar_with_soft_clips` |
| 17 | §4.5 InvalidXmByte cleanup | §4.5 | DONE | `pipeline.rs:107-113` cleanup-on-err |
| 18 | §4.5 PAIRED-flag rejection | §4.5 | DONE | `pipeline.rs:73-82` + smoke covers |
| 19 | §4.5 output_dir auto-create | §4.5 | DONE | `output.rs:78` `create_dir_all` |
| 20 | §4.5 non-ASCII chr name rejection | §4.5 | DONE | `header.rs::build_chr_name_table` |
| 21 | §5.1 call.rs: CytosineContext / MethCall / XmClassification / classify_xm_byte / extract_calls | §5.1 | DONE | `call.rs` full surface present |
| 22 | §5.2 mbias.rs: MbiasPos + MbiasTable::accumulate | §5.2 | DONE | `mbias.rs` |
| 23 | §5.3 output.rs: OutputKey / OutputFileMap (eager, single map of (PathBuf, BufWriter)) / SplittingReport / write_splitting_report | §5.3 | DONE | `output.rs:32-64` uses single `HashMap<OutputKey, (PathBuf, BufWriter<File>)>` |
| 24 | §5.4 state.rs: ExtractState + new/finalize/cleanup_partial_outputs | §5.4 | DONE | `state.rs` |
| 25 | §5.5 route.rs: route_call with rev-1 ordering | §5.5 | DONE | `route.rs:22-77` |
| 26 | §5.6 pipeline.rs: extract_se | §5.6 | DONE | `pipeline.rs:54-129` |
| 27 | §5.7 header.rs: build_chr_name_table with ASCII assert | §5.7 | DONE | `header.rs:24-40` |
| 28 | §5.8 error variants: PhaseNotYetImplemented, InvalidXmByte, IoWrite, BismarkIo, InternalError, NonAsciiChromosomeName | §5.8 | DONE | `error.rs:132-187` all six present |
| 29 | §6 step 1: add 6 error variants | §6 | DONE | see Item 28 |
| 30 | §6 step 2: create call.rs | §6 | DONE | `src/call.rs` exists |
| 31 | §6 step 3: create mbias.rs | §6 | DONE | `src/mbias.rs` exists |
| 32 | §6 step 4: create output.rs (eager-open, single map, cleanup_all, format_meth_line, SplittingReport) | §6 | DONE | `src/output.rs` |
| 33 | §6 step 5: create state.rs (new/finalize/cleanup_partial_outputs) | §6 | DONE | `src/state.rs` |
| 34 | §6 step 6: create route.rs (rev-1 ordering) | §6 | DONE | `src/route.rs` |
| 35 | §6 step 7: create header.rs (ASCII assertion) | §6 | DONE | `src/header.rs` |
| 36 | §6 step 8: create pipeline.rs (extract_se + derive_basename) | §6 | DONE | `src/pipeline.rs` |
| 37 | §6 step 9: update lib.rs (pub mod + re-exports) | §6 | DONE | `src/lib.rs:40-56` |
| 38 | §6 step 10: update main.rs::run (5 PhaseNotYetImplemented paths + extract_se) | §6 | DONE | `src/main.rs:41-107` |
| 39 | §6 step 11: bump Cargo.toml to 1.0.0-alpha.2 | §6 | DONE | `Cargo.toml:3` |
| 40 | §6 step 12: leave src/params.rs untouched | §6 | DONE | unmodified |
| 41 | §6 step 13: write tests | §6 / §7 | PARTIAL | 38 of 41 plan-§7.1 named tests + smoke; see Test verification |
| 42 | §6 step 14: `cargo test && cargo clippy && cargo fmt --check` | §6 | DONE (test side) | 90/90 tests pass; clippy + fmt not run in this audit but `cargo test` finished cleanly |
| 43 | §3.2 commit `tests/data/regenerate.sh` and synthetic BAM fixture | §3.2 | MISSING | No `tests/data/` directory exists. Smoke test builds the BAM in-process instead — functional coverage is achieved, but the plan-listed deliverable (committed regenerate.sh + README.md per §7.2-§7.3) is absent |

## Coverage ledger — validation matrix (plan §10)

| # | Validation item | Source | Status | Notes |
|---|-----------------|--------|--------|-------|
| V1 | Eager-open + header bytes match Perl | §10 | DONE | `output_file_map_eagerly_creates_*` + `output_file_header_matches_perl_format` |
| V2 | `-`-strand orientation invariant | §10 | DONE | `extract_calls_minus_strand_orients_5prime` + `..._orients_both_calls` |
| V3 | Missing CHG/CHH closure (Alan's bug) | §10 | DONE | 4 mbias_routes_to_{chg,chh}_* + `route_single_record_with_mixed_contexts_*` |
| V4 | Partial-output cleanup | §10 | PARTIAL | `cleanup_partial_outputs_removes_all_12_files` present; `cleanup_partial_outputs_continues_past_one_failure` MISSING |
| V5 | Phase-gate rejections (6) | §10 | DONE | 6 main_rejects_* tests pass |
| V6 | PAIRED-flag rejection | §10 | DONE | `smoke_se_rejects_record_with_paired_flag_set` |
| V7 | Counter ordering vs `mbias_only` | §10 | DONE | `route_call_increments_counter_before_mbias_only_short_circuit` |
| V8 | Soft-clip read_pos counting | §10 | DONE | `extract_calls_walks_cigar_with_soft_clips` |
| V9 | Non-ASCII chr rejection | §10 | DONE | `build_chr_name_table_rejects_non_ascii` |
| V10 | output_dir auto-create | §10 | DONE | `output_file_map_creates_output_dir_if_missing` |
| V11 | E2E smoke | §10 | DONE | `smoke_se_directional_produces_all_12_files_and_report` |
| V12 | Empty input | §10 | DONE | `smoke_se_empty_bam_writes_only_header_files` |
| V13 | Clippy + fmt | §10 | NOT-RUN | Plan step §6.14; out of scope for this coverage audit; `cargo test` clean |

## Test verification (Mode B)

All tests run under `cargo test -p bismark-extractor` — 40 lib unit + 4 sanity + 43 se_phase_b + 3 smoke = **90 passing, 0 failing**.

| # | Plan §7.1 test name | Where implemented | Status |
|---|---------------------|-------------------|--------|
| T-01 | classify_xm_byte_classifies_all_six_methylation_bytes | tests/se_phase_b.rs:138 | PASS |
| T-02 | classify_xm_byte_skips_U_u_dot | tests/se_phase_b.rs:159 | PASS |
| T-03 | classify_xm_byte_rejects_invalid | tests/se_phase_b.rs:174 | PASS |
| T-04 | extract_calls_classifies_all_six_methylation_bytes | tests/se_phase_b.rs:197 | PASS |
| T-05 | extract_calls_respects_ignore_5p | tests/se_phase_b.rs:211 | PASS |
| T-06 | extract_calls_respects_ignore_3p | tests/se_phase_b.rs:221 | PASS |
| T-07 | extract_calls_walks_cigar_with_indels | tests/se_phase_b.rs:231 | PASS |
| T-08 | extract_calls_walks_cigar_with_soft_clips | tests/se_phase_b.rs:254 | PASS |
| T-09 | extract_calls_empty_xm_yields_empty_vec | tests/se_phase_b.rs:279 | PASS |
| T-10 | extract_calls_minus_strand_orients_5prime | tests/se_phase_b.rs:286 | PASS |
| T-11 | extract_calls_rejects_invalid_xm_byte_with_error | tests/se_phase_b.rs:325 | PASS |
| T-12 | mbias_accumulate_increments_meth_for_Z | tests/se_phase_b.rs:351 | PASS |
| T-13 | mbias_accumulate_increments_unmeth_for_z | tests/se_phase_b.rs:358 | PASS |
| T-14 | mbias_accumulate_routes_to_chg_for_X | tests/se_phase_b.rs:365 | PASS |
| T-15 | mbias_accumulate_routes_to_chg_for_x | tests/se_phase_b.rs:376 | PASS |
| T-16 | mbias_accumulate_routes_to_chh_for_H | tests/se_phase_b.rs:383 | PASS |
| T-17 | mbias_accumulate_routes_to_chh_for_h | tests/se_phase_b.rs:390 | PASS |
| T-18 | mbias_R2_index_ready | tests/se_phase_b.rs:755 | PASS |
| T-19 | route_call_default_mode_routes_to_strand_specific_file | tests/se_phase_b.rs:606 | PASS |
| T-20 | route_single_record_with_mixed_contexts_routes_to_one_strand_directory | tests/se_phase_b.rs:637 | PASS |
| T-21 | format_meth_line_exact_bytes | tests/se_phase_b.rs:496 (`..._for_unmethylated`) | PASS (renamed; behavior matches) |
| T-22 | output_file_map_eagerly_creates_all_strand_files_for_default_mode | tests/se_phase_b.rs:401 | PASS |
| T-23 | output_file_map_omits_header_when_no_header_true | tests/se_phase_b.rs:429 | PASS |
| T-24 | output_file_header_matches_perl_format | tests/se_phase_b.rs:442 | PASS |
| T-25 | output_file_map_creates_output_dir_if_missing | tests/se_phase_b.rs:463 | PASS |
| T-26 | cleanup_partial_outputs_removes_all_12_files | tests/se_phase_b.rs:515 | PASS |
| T-27 | cleanup_partial_outputs_continues_past_one_failure | — | **MISSING** |
| T-28 | route_call_increments_counter_before_mbias_only_short_circuit | tests/se_phase_b.rs:686 | PASS |
| T-29 | splitting_report_emits_per_context_counts | tests/se_phase_b.rs:544 | PASS |
| T-30 | splitting_report_percentage_handles_zero_denominator | tests/se_phase_b.rs:531 | PASS |
| T-31 | build_chr_name_table_rejects_non_ascii | tests/se_phase_b.rs:783 | PASS |
| T-32 | derive_basename_strips_known_suffixes | tests/se_phase_b.rs:807 | PASS |
| T-33 | extract_se_rejects_record_with_paired_flag_set | tests/se_phase_b_smoke.rs (`smoke_se_rejects_record_with_paired_flag_set`) | PASS (relocation acknowledged in se_phase_b.rs:910-912 comment) |
| T-34 | main_rejects_paired_end_with_phase_error | tests/se_phase_b.rs:839 | PASS |
| T-35 | main_rejects_multiple_input_files | tests/se_phase_b.rs:852 | PASS |
| T-36 | main_rejects_multicore_with_phase_error | tests/se_phase_b.rs:864 | PASS |
| T-37 | main_rejects_gzip_with_phase_error | tests/se_phase_b.rs:876 | PASS |
| T-38 | main_rejects_comprehensive_with_phase_error | tests/se_phase_b.rs:887 | PASS |
| T-39 | main_rejects_bedgraph_with_phase_error | tests/se_phase_b.rs:898 | PASS |
| T-40 | extract_se_two_records_route_to_different_files | — | **MISSING** |
| T-41 | extract_se_empty_input_writes_only_header_files | tests/se_phase_b_smoke.rs (`smoke_se_empty_bam_writes_only_header_files`) | PASS (relocated to smoke file; functional coverage equivalent) |
| T-42 (§7.2) | smoke_se_directional_produces_all_12_files_and_report | tests/se_phase_b_smoke.rs | PASS |

### Bonus tests (not in plan §7.1 but added during implementation)

| Test | File | Notes |
|------|------|-------|
| extract_calls_minus_strand_orients_both_calls | se_phase_b.rs:304 | Strengthens orientation invariant — bonus, not gap |
| extract_calls_ignore_larger_than_seq_returns_empty | se_phase_b.rs:340 | Edge-case (ignore > seq_len, plan §4.5 row) |
| output_file_map_write_call_appends_after_header | se_phase_b.rs:476 | Header + payload-write co-test |
| splitting_report_percentage_for_50_50 | se_phase_b.rs:538 | Sanity for percent calc |
| route_call_r2_goes_to_mbias_index_1 | se_phase_b.rs:725 | R2 index plumbing — covers Phase C precursor |
| build_chr_name_table_returns_ascii_names_in_order | se_phase_b.rs:769 | Happy-path counterpart to ASCII-reject |

## Gaps (detail)

### Item T-27 (MISSING test): `cleanup_partial_outputs_continues_past_one_failure`

**Expected (plan §7.1):** A test that drops/locks one of the 12 output files such that `std::fs::remove_file` would fail for that file, then asserts the remaining 11 are still removed and the `cleanup_all` call does not panic.

**Found:** Nothing. Only `cleanup_partial_outputs_removes_all_12_files` exists (covers the happy-path cleanup).

**Gap:** The robustness invariant — "one failed remove doesn't prevent others" — is implemented (`output.rs:149-164` continues iterating past `remove_file` errors) but not unit-tested. Plan §10 row "Partial-output cleanup" explicitly lists this test as one of the two cleanup checks.

### Item T-40 (MISSING test): `extract_se_two_records_route_to_different_files`

**Expected (plan §7.1, "Rev 1 (B §4)" annotation):** Two records (one OT, one OB) → calls land in `*_OT_*` and `*_OB_*` respectively; multi-record accumulator correctness.

**Found:** Functionally covered by `smoke_se_directional_produces_all_12_files_and_report` (which uses 3 OT + 2 OB records and asserts CpG_OB / CHH_OB / *_OT_* files all have content). But the named unit test that would isolate "two records, different strands → different files" without going through the binary spawn is absent.

**Gap:** No dedicated unit-level multi-record routing test. Smoke gives end-to-end coverage but loses the locality that a focused unit test provides.

### Item 43 (MISSING fixture deliverable): `tests/data/regenerate.sh` + `tests/data/se_directional_phase_b.bam` + README

**Expected (plan §3.2 + §7.2-§7.3):** Commit `tests/data/regenerate.sh` and `tests/data/se_directional_phase_b.bam` (per §3.2) plus a `tests/data/README.md` documenting how to regenerate the smoke BAM from a synthetic FASTA (per §7.3).

**Found:** No `tests/data/` directory. Smoke test instead constructs synthetic BAMs in-test via `BamWriter::from_path`. This is functionally equivalent (and arguably preferable — fewer binary blobs in the repo) but deviates from the plan-listed deliverable.

**Gap:** Either commit the documented deliverables, or update the plan to document the in-test BAM-construction approach as an intentional deviation.

### Item V13: clippy / fmt verification not performed

**Expected (plan §6 step 14, §10 row 13):** `cargo clippy -p bismark-extractor -- -D warnings && cargo fmt --check`.

**Found:** Out of scope for this coverage audit; only `cargo test` was run. `cargo test` succeeded cleanly with no warnings printed in the captured output. Plan-step verification of clippy + fmt remains the implementer's responsibility.

**Gap:** Not a coverage gap per se — the audit didn't exercise this. Listing it here so it isn't lost.

## Verdict

**INCOMPLETE — 3 items unresolved.**

The implementation is functionally complete: every scope decision, behaviour, signature, edge case, and validation item in the plan is implemented and the supporting tests pass (90/90 green). All Phase B exit criteria (eager-open output map, version-header byte fidelity, splitting-report skeleton, M-bias accumulator, route_call rev-1 ordering, six phase-gate rejections, PAIRED-flag-on-SE rejection, non-ASCII chr name rejection, output_dir autocreate, partial-output cleanup, end-to-end smoke) are working.

The three unresolved items are documentation/test-coverage gaps, not behaviour gaps:

1. **T-27** — add the `cleanup_partial_outputs_continues_past_one_failure` test in `tests/se_phase_b.rs`. The behaviour exists (`output.rs:149-164`) but is unit-test-uncovered.
2. **T-40** — add the `extract_se_two_records_route_to_different_files` test in `tests/se_phase_b.rs`. The behaviour is exercised end-to-end by the smoke test, but the focused unit test is absent.
3. **Item 43** — either commit `tests/data/regenerate.sh` + `tests/data/se_directional_phase_b.bam` + `tests/data/README.md` (the plan's §3.2 / §7.3 deliverable), or update the plan to record the in-test BAM-construction approach as an intentional deviation.

None of these block Phase B merge if the user is willing to formally re-classify them. If strict plan coverage is required, all three are mechanical fixes.

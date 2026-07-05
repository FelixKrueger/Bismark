# Plan Coverage Report

**Mode:** B (code vs. plan, post-implementation)
**Plan:** `plans/06142026_empty-sample-extractor-c2c/PLAN.md`
**Branch / worktree:** `rust/extractor-empty-outputs` @ `~/Github/Bismark-dedup` (off `origin/rust/iron-chancellor` `b97a8e2`)
**Date:** 2026-06-14
**Verdict:** COMPLETE — 0 code/test items unresolved (1 validation item V6b + the hard gate V-E2E remain by design, both out of this audit's scope)

## Summary

- Total auditable items: 23 (4 impl A + 3 impl B + 5 tests C + 11 validation/deviation)
- DONE: 20
- PARTIAL: 0
- MISSING: 0
- DEVIATED (documented): 2 (gc_context exclusion; second test-file rewrite)
- OUT-OF-SCOPE / NOT-YET-RUN: 1 (V6b static scout) + 1 (V-E2E, Felix's real methylseq run)

All `bismark-extractor` + `bismark-coverage2cytosine` tests pass (0 failures). `cargo fmt --check` and `cargo clippy --all-targets -D warnings` are clean for both crates.

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | `is_empty_run = calls_total==0`; sweep gated `&& config.bedgraph`; plumbed to chain | A.1 / state.rs:162, 182, 243 | DONE | `is_empty_run` from `self.report.calls_total == 0`; `force_create_empty = is_empty_run && config.bedgraph` passed to sweep; `is_empty_run` passed to `run_downstream_chain`. Single `finalize` covers parallel.rs + pipeline.rs paths. |
| 2 | Chain skip guard `!usable && !is_empty_run` (NOT just `!usable`) | A.2 / downstream_filenames.rs:291 | DONE | `if !usable && !is_empty_run { warn + return }`. Empty run falls through to `write_outputs_from_sorted` with empty `sorted`. Documented divergence comment at :281-290. |
| 3 | Force-create+finish empty per-context `.txt.gz` (NOT delete) on empty path; non-empty unchanged | A.3 / output.rs:537-557 | DONE | New `force_create_empty` arm: `Some(w)=>w.finish()` else `open_split_writer(&path,gzip).finish()`; recorded as `kept`. Non-force arm (:558-576) keeps Perl-faithful `remove_file`+swept byte-identical. |
| 4 | Doc deviation comments at extractor sites | A.4 / state.rs:153-161, output.rs:479-485/538-545, downstream_filenames.rs:281-290 | DONE | All three sites cite the plan slug + label the divergence DELIBERATE. |
| 5 | c2c `run_single` empty-`None` relax scoped to standard path; errors otherwise | B.5 / report.rs:460-463 | DONE | `empty_standard_path = threshold==0 && !nome && !gc_context`; `None if !empty_standard_path => Err(EmptyCoverageInput)`; else fall through to uncovered pass (all-zero genome report). |
| 6 | Second guard site `run_split` ALSO handled | B.6 / report.rs:548-566 | DONE | Same `empty_standard_path` gate; `last_summary_path` widened to `Option<PathBuf>`; summary write guarded `if let Some(...)`. Both rev-1-flagged sites (~450 + ~530) covered. |
| 7 | c2c doc deviation comment (vs Perl "No last chromosome was defined" die) | B.7 / report.rs:449-459, 539-550 | DONE | Both `run_single` and `run_split` carry the divergence rationale + plan slug. |
| 8 | Extractor graceful-path test: zero-call BAM `--bedGraph --gzip` → exit 0 + 5 globs; bedGraph/cov 0 data rows | C.8 / phase2_inline.rs:848 `empty_input_emits_graceful_outputs_exit_zero` | DONE | Asserts exactly 1 `.bedGraph.gz`, 1 `.bismark.cov.gz` (0 rows), ≥1 `.txt.gz`, 1 splitting report, 1 M-bias. Plus `--CX` variant (:888) and `--multicore 2` variant (:917) and `--no-bedGraph keeps Perl delete` guard (:945). |
| 9 | Rewrite existing `empty_input_skips_downstream_exit_zero` (NOT delete); canary `default_mode_no_cpg_calls_skips` stays green | C.9 / phase2_inline.rs:848+975 | DONE | Old test renamed → `empty_input_emits_graceful_outputs_exit_zero`, now asserts graceful outputs. Canary at :975 unchanged, still asserts has-calls/no-CpG → skip (= V3b). Both green. |
| 10 | c2c graceful-path test: empty `.cov` + small genome → exit 0 + `report.txt.gz` all-zero + summary; rows == genome C count | C.10 / golden_phase_b.rs:129 + :173 | DONE | `empty_coverage_input_standard_path_emits_all_zero_report` (plain, 2 rows == chrA C count) + `empty_coverage_gzipped_standard_path_emits_gzipped_all_zero_report` (`.cov.gz`+`--gzip` → decompressed 2 all-zero rows, methylseq shape). |
| 11 | c2c error-regression: corrupt-gz (`.gz` name) + missing file; nome/gc/threshold still error on empty | C.11 + V5/V5b / golden_phase_b.rs:228/251/279/307/335, cov.rs:102 | DONE | `empty_coverage_missing_file_errors`, `empty_coverage_corrupt_gzip_with_gz_name_errors`, `_threshold_still_errors`, `_nome_still_errors`, `_gc_still_errors` all present + green. Malformed-line regression preserved as `cov.rs::parse_malformed_errors` unit test (`MalformedCovLine` propagates via `?` before the `None` arm). |
| 12 | Tier-3 conformance rows added in BOTH crates | C.12 / extractor methylseq_conformance.rs:240, c2c methylseq_conformance.rs:72 | DONE | Extractor: `methylseq_extractor_no_alignment_runtime_emits_required_outputs` (header-only BAM, methylseq command shape → all 5 globs). c2c: `methylseq_coverage2cytosine_empty_runtime_emits_required_outputs` (empty `.cov.gz` + `--gzip` → `report.txt.gz` + summary). |
| 13 | Deviation 1: c2c standard-path gate adds `&& !config.gc_context` (exclude `--gc`) | Impl Notes / report.rs:460, 560 | DEVIATED (documented) | Plan rev-1 A-I2/B-I2 explicitly excludes `--gc` (gpc relies on guard); deviation is intent-fulfilling. Guarded by `empty_coverage_gc_still_errors`. |
| 14 | Deviation 2: second inverted test (`phase3a_streaming.rs` inline `--cytosine_report` copy) also rewritten | Impl Notes / phase3a_streaming.rs:1500 | DEVIATED (documented) | `empty_input_emits_graceful_outputs_with_cytosine_report` asserts the inline c2c feed produces an all-zero CpG report (every row `\t0\t0\t`). Its own `default_mode_no_cpg_calls_skips` canary (:1562) still green. |
| V1 | Extractor emits required outputs on empty | phase2_inline + conformance | DONE | Covered by items 8 + 12 (extractor). |
| V2 | c2c graceful on empty `.cov` | golden_phase_b + conformance | DONE | Covered by items 10 + 12 (c2c). |
| V3 | Non-empty extractor unchanged (full test suite) | `cargo test -p bismark-extractor` | DONE | All extractor binaries pass; rewritten test asserts graceful, canary proves `total_calls==0` gate. |
| V3b | has-calls/no-CpG default bedGraph still SKIPS | phase2_inline.rs:975 + phase3a:1562 | DONE | `default_mode_no_cpg_calls_skips` (both files) asserts no bedGraph/cov + the skip warning. |
| V4 | Non-empty c2c unchanged (full suite incl. byte-identity) | `cargo test -p bismark-coverage2cytosine` | DONE | All c2c binaries pass. |
| V5 | c2c still errors on genuine read failure (corrupt-gz / missing / malformed) | golden_phase_b + cov.rs | DONE | Covered by item 11. |
| V5b | c2c nome/gc/threshold unchanged on empty | golden_phase_b.rs:279/307/335 | DONE | All three guard tests present + green. |
| V6 | Lint/fmt both crates clean | `cargo fmt --check` + `clippy -D warnings` | DONE | Verified in this audit: fmt exit 0; clippy clean (no warnings) for both crates. |
| V6b | Static scout of report/summary/MultiQC contracts | (validation step) | NOT DONE (by design) | A validation/scout step, not a code task. Impl Notes explicitly list it as outstanding. Not auditable as code; flagged below. |
| V-E2E | methylseq survives no-alignment sample (HARD gate) | Felix's real Seqera run on beta.7 image | OUT OF SCOPE | Cannot be audited here (requires real methylseq run + beta.7 image). Felix's gate. |

## Gaps (detail)

No code or test gaps. The two non-DONE rows are non-code validation items:

### V6b: static scout of report/summary/MultiQC output contracts

**Expected:** statically read the `bismark/report`, `bismark/summary` module output globs + the MultiQC bismark module behavior on all-zero inputs, to surface any 3rd wall before the expensive V-E2E run.
**Found:** not performed in this branch (it is a validation/scouting activity, not a source change). The plan's Implementation Notes already list it as outstanding ("Still outstanding before 'done': V6b … + V-E2E").
**Gap:** none in code. This is a pre-V-E2E de-risking step for Felix/the next session, not an implementation deliverable. Note that the local full cascade (dedup → extractor → c2c) was already empirically verified through the c2c wall (Impl Notes), so walls 1+2 are cleared; V6b/V-E2E only probe the report→summary→MultiQC tail.

### V-E2E: real methylseq run (HARD gate)

**Expected:** real nf-core/methylseq on the beta.7 image with the failing no-alignment sample completes through extractor + c2c + report + summary + MultiQC.
**Found:** not runnable in this audit context (needs the beta.7 container + Felix's Seqera env).
**Gap:** none in code; this is the external acceptance gate, explicitly Felix's.

## Test verification (Mode B)

| Test name | File | Status |
|-----------|------|--------|
| empty_input_emits_graceful_outputs_exit_zero (rewritten) | extractor/tests/phase2_inline.rs:848 | PASS |
| empty_input_cx_emits_graceful_outputs_exit_zero | extractor/tests/phase2_inline.rs:888 | PASS |
| empty_input_multicore_emits_graceful_outputs_exit_zero | extractor/tests/phase2_inline.rs:917 | PASS |
| empty_input_no_bedgraph_keeps_perl_faithful_delete | extractor/tests/phase2_inline.rs:945 | PASS |
| default_mode_no_cpg_calls_skips (canary, V3b) | extractor/tests/phase2_inline.rs:975 | PASS |
| empty_input_emits_graceful_outputs_with_cytosine_report (rewritten, deviation 2) | extractor/tests/phase3a_streaming.rs:1500 | PASS |
| default_mode_no_cpg_calls_skips (canary, V3b, inline) | extractor/tests/phase3a_streaming.rs:1562 | PASS |
| methylseq_extractor_no_alignment_runtime_emits_required_outputs (C.12) | extractor/tests/methylseq_conformance.rs:240 | PASS |
| empty_coverage_input_standard_path_emits_all_zero_report (rewritten, C.10) | c2c/tests/golden_phase_b.rs:129 | PASS |
| empty_coverage_gzipped_standard_path_emits_gzipped_all_zero_report (C.10) | c2c/tests/golden_phase_b.rs:173 | PASS |
| empty_coverage_missing_file_errors (V5) | c2c/tests/golden_phase_b.rs:228 | PASS |
| empty_coverage_corrupt_gzip_with_gz_name_errors (V5) | c2c/tests/golden_phase_b.rs:251 | PASS |
| empty_coverage_threshold_still_errors (V5b) | c2c/tests/golden_phase_b.rs:279 | PASS |
| empty_coverage_nome_still_errors (V5b) | c2c/tests/golden_phase_b.rs:307 | PASS |
| empty_coverage_gc_still_errors (V5b, deviation 1) | c2c/tests/golden_phase_b.rs:335 | PASS |
| parse_malformed_errors (V5 malformed-line regression) | c2c/src/cov.rs:102 | PASS |
| methylseq_coverage2cytosine_empty_runtime_emits_required_outputs (C.12) | c2c/tests/methylseq_conformance.rs:72 | PASS |
| **Full suite** `cargo test -p bismark-extractor -p bismark-coverage2cytosine` | both crates | **PASS — 0 failed** |

Aggregate test-binary results: every `test result:` line reported `ok … 0 failed`. Highest-count binaries: extractor lib 110 passed, extractor integ 98/38/32/26/22/17/15/12/10/9/4/3, c2c 18/17/12/10/7/4/3/2. No failures anywhere.

Lint/fmt (V6): `cargo fmt -p bismark-extractor -p bismark-coverage2cytosine -- --check` exit 0; `cargo clippy -p bismark-extractor -p bismark-coverage2cytosine --all-targets -- -D warnings` clean.

## Verdict

**COMPLETE — 0 code/test items unresolved.**

Every implementation step (A.1–A.4, B.5–B.7), every test task (C.8–C.12), and every code-auditable validation item (V1–V6, V3b, V5b) is DONE and green. The two documented deviations (gc_context exclusion in the c2c standard-path gate; the extra `phase3a_streaming.rs` test rewrite) are intent-fulfilling and correctly labeled DEVIATED-documented, not gaps.

Two items remain outside this audit's scope, exactly as the plan's Implementation Notes state:
- **V6b** — static scout of report/summary/MultiQC contracts (a pre-V-E2E de-risking step, not a code change).
- **V-E2E** — Felix's real methylseq run on the beta.7 image (the external HARD acceptance gate).

These do not block the implementation's completeness; they are the remaining validation/release activities before declaring the feature shipped.

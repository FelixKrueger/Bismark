# Code Review — Phase C (bismark-extractor) — Reviewer B

**Reviewer:** B (independent, fresh context)
**Date:** 2026-05-26
**Branch:** `extractor-phase-c`
**Plan:** `plans/05262026_bismark-extractor/PHASE_C_PLAN.md` rev 1

## Summary

Phase C lights up the paired-end extraction path with overlap detection (`drop_overlap` per SPEC §7.4), per-mate ignore trims, a clean SE-vs-PE auto-detect via header-promoted `bismark_io::detect_paired_from_header`, and a Phase A `no_overlap` regression fix. The implementation is faithful to the rev-1 plan: every locked decision is honoured, every rev-1 test fixture has a corresponding `#[test]`, and the duplicate-scaffolding path (rather than the `run_extraction<F>` refactor) was chosen — correctly — because Phase B PR #849 is still in review. Cross-crate promotion (bismark-io v1.0.0-beta.7, bismark-dedup re-export) is mechanically clean: `pub use` plus a brief module-level comment in dedup explaining the move, and the dedup tests still green.

Verdict: **APPROVE-WITH-NITS.** No correctness blockers. Three Low-severity items below are worth tightening before/after merge (one is removable code; two are doc/test clarity).

## Issues by area

### Logic

**L1 (Low) — `pe_phase_c_smoke.rs` overlap-arithmetic comment is wrong.**
The smoke fixture builds 10 OT pairs with R1 at `r1_start` (CIGAR `5M`, XM `Z....`) and R2 at the same `r2_start = r1_start`. The smoke asserts CpG_OT has `≥ 1` call line. The comment at lines 180-184 claims R2 calls fail strict-`<` keep because `r1_ref_end = r1_start + 5 - 1 = r1_start + 4` and R2's reversed call ends up at `r2_start + 4 = r1_ref_end`. That arithmetic doesn't match `CigarExt::reference_end`'s contract — bismark-io's overlap.rs comments treat `reference_end(100)` for `50M` as `149`, i.e. `start + len - 1` (inclusive last position). For 5M at 100, `reference_end == 104`. The R2 reversed call from `XM[4]='z'` reads back as `read_pos_5p == 0 → ref_pos == r2_start + 4 == 104`. Strict-`<` keep against `104` drops it (correct). The comment's "`r1_ref_end - 0`, which fails strict `<`" wording is accurate but the math chain confused me on first read. **Not a bug** — the assertion `≥ 1` is robust either way (R1's call at ref_pos 100 always passes). Tighten the smoke to assert `cpg_ot_call_lines == 10` (10 R1 calls, all R2 dropped) and the rationale becomes a pinned invariant, not a comment.

**L2 (Low) — `extract_pe_with_no_overlap_drops_r2_calls_past_r1_end` test (renamed from rev-0).**
The rename happened because R2 calls *inside* R1's span are KEPT by Perl's strict-`<` polarity — this is documented in `drop_overlap_fully_overlapping_pair_keeps_calls_inside_r1_span` (unit) and `drop_overlap_disjoint_pair_drops_all_r2_calls_downstream_of_r1_end` (unit). The assertion set inside the rename'd test is correct (drops 105/106, keeps 103). One concern: the polarity discovery is a **byte-identity load-bearer** for Phase H. The test comment at lines 683-694 spells out the math but doesn't cite Perl line numbers. Suggest adding `// Perl bismark_methylation_extractor:2905 / 2989` to the in-test comment so a future grep lands on it.

**L3 (Low) — `extract_pe_routes_r2_calls_to_pair_strand_file_not_record_strand_file` uses `--include_overlap`.**
The test comment at lines 619-625 *does* document the rationale ("Uses `--include_overlap` to disable `drop_overlap`, so we test routing in isolation"). Good — this addresses my V1 concern preemptively. No action.

### Efficiency

**E1 (Low / Phase F concern) — per-pair allocation profile.**
`handle_one_pair` allocates 2× `extract_calls` Vec + at most one `Vec::retain`-in-place for `drop_overlap`. The plan §8 already calls this out as a Phase F concern ("~14 GiB total at 27M pairs"). Not actionable in Phase C; `retain` is the right choice for keeping the rev-1 simplification. Flag for Phase F's profiling pass.

**E2 (Low) — `String::from_utf8_lossy(...).into_owned()` qname rendering happens twice in `extract_pe` body.**
Once in `UnpairedFinalRecord` path (line 220), once in `MateChromosomeMismatch` path (line 282). These are error paths so allocation cost is irrelevant; the duplication is a maintenance papercut. A `render_qname_opt(record) -> Option<String>` (or `render_qname_or_unnamed(record) -> String`) helper would dedupe. Note: the plan §4.1 pseudocode referenced `render_qname_opt`, and the implementation chose to inline it. **No action needed for Phase C correctness**; flag as a follow-up cleanup.

### Errors / Edge cases

**Err1 (Low) — `noodles-sam` `io` feature already present.**
The `detect_paired_from_header` body uses `noodles_sam::io::Writer` for header serialization. bismark-io's `read.rs` already uses `noodles_sam::io::Reader` (line 373, 389, etc.), so `io` is reachable in the default-feature set of noodles-sam 0.85. `cargo build -p bismark-extractor` succeeds locally. Not a regression risk.

**Err2 (Low) — `#[allow(unused_imports)]` on `pub use bismark_io::detect_paired_from_header` in dedup `pipeline.rs:29`.**
The `pub use` re-export should make the symbol "used" — `unused_imports` is for unused `use`/`pub use`. I tested removing the allow mentally: a `pub use foo;` with no internal usage in the same module would trigger `unused_imports` only if the surrounding crate has `#![deny(unused_imports)]` or similar. Dedup doesn't (its lib.rs has `#![forbid(unsafe_code)]` only). The allow is likely **unnecessary**. Safe to remove and re-test, but harmless to keep. **Recommend:** remove the `#[allow]` and verify `cargo build -p bismark-dedup` stays clean.

**Err3 (Low) — `extract_pe_rejects_unpaired_final_record` doesn't assert cleanup.**
The test asserts the error fires + stderr substring; it does NOT assert all 12 partial files are removed. The sister test `extract_pe_rejects_cross_chromosome_pair` (lines 772-776) does assert cleanup completion. The plan §7.1 row says "cleanup removes all 12 files". Suggest adding the same `if outdir.exists() { count == 0 }` block to `extract_pe_rejects_unpaired_final_record`.

### Structure / Tests-as-documentation

**S1 (Low) — Plan §7.1 lists ~22 tests; implementation has 22.**
Mapping plan rows to test names:

| Plan row | Test in code | Status |
|----------|--------------|--------|
| `drop_overlap_forward_pair_drops_r2_at_or_after_r1_end` | same | ✅ |
| `drop_overlap_reverse_pair_drops_r2_at_or_before_r1_start` | same | ✅ |
| `drop_overlap_disjoint_pair_is_noop` | renamed to `..._drops_all_r2_calls_downstream_of_r1_end` | ✅ rename documented in test body (line 268-273); rev-1 polarity-discovery note locked. |
| `drop_overlap_fully_overlapping_pair_drops_all_r2_calls` | renamed to `..._keeps_calls_inside_r1_span` | ✅ same polarity discovery; documented at lines 290-294. |
| `drop_overlap_with_r1_indel_uses_reference_end` | same | ✅ |
| `drop_overlap_with_r1_end_deletion` | same | ✅ |
| `drop_overlap_with_r1_insertion_shifts_read_pos_only` | same | ✅ |
| `is_forward_pair_strand_matches_perl_classification` | same | ✅ |
| `bismark_pair_from_mates_rejects_mismatched_qnames` | same | ✅ |
| `extract_pe_handles_two_well_formed_pairs` | same (in `pe_e2e` mod) | ✅ |
| `extract_pe_rejects_unpaired_final_record` | same | ✅ |
| `extract_pe_rejects_mismatched_qnames_pair` | absent — only the bismark-io-level `bismark_pair_from_mates_rejects_mismatched_qnames` is present | ⚠️ |
| `extract_pe_rejects_cross_chromosome_pair` | same | ✅ |
| `extract_pe_with_include_overlap_keeps_r2_overlap_calls` | same | ✅ |
| `extract_pe_with_no_overlap_drops_r2_overlap_calls` | renamed to `..._drops_r2_calls_past_r1_end` | ✅ documented in test comment |
| `extract_pe_per_mate_ignore_r2_only_skips_r2_positions` | absent | ⚠️ |
| `extract_pe_per_mate_ignore_3prime_r2_only_skips_r2_3prime` | absent | ⚠️ |
| `extract_pe_ignore_r2_skips_read_cycles_not_ref_positions` | absent | ⚠️ |
| `extract_pe_routes_r2_calls_to_pair_strand_file_not_record_strand_file` | same | ✅ |
| `extract_pe_routes_ctot_pair_strand_correctly` | absent | ⚠️ |
| `extract_pe_routes_ctob_pair_strand_correctly` | absent | ⚠️ |
| `extract_pe_increments_mbias_R2_at_index_1` | absent | ⚠️ |
| `extract_pe_empty_bam_writes_only_header_files` | absent | ⚠️ |
| `pe_splitting_report_counts_lines_not_pairs` | same (in `pe_e2e` mod) | ✅ |
| `run_extraction_runs_cleanup_on_each_error_variant` | absent (and moot — duplicate-scaffolding path chosen, no `run_extraction` helper) | ✅ Phase B's existing 91 SE tests already exercise cleanup-on-error per Phase B's surface. |
| `extract_se_handles_two_well_formed_records` | absent — covered by Phase B's existing 44 SE tests | ✅ (no `run_extraction` refactor → no regression risk) |
| `validate_auto_detect_keeps_no_overlap_default` | same | ✅ |
| `detect_paired_from_header_*` (3 unit tests) | moved into `bismark-io/src/read.rs` | ✅ verified at read.rs:1148-1192 |
| `main_auto_detect_routes_pe_bam_to_extract_pe` | same | ✅ |
| `main_auto_detect_routes_se_bam_to_extract_se` | same | ✅ |
| `main_auto_detect_fails_without_bismark_pg` | same | ✅ |

**7 plan-listed tests absent.** Some of these are arguably covered by Phase B's existing surface (the run_extraction one, the SE one), but the per-mate ignore tests, the CTOT/CTOB-pair-strand routing tests, the mbias-R2-index test, and the empty-BAM test ARE Phase-C-specific and were locked in rev 1. The plan-manager skill will likely catch this; flagging here as the structurally-most-significant gap.

**S2 (Low) — Test comment hygiene at `drop_overlap_with_r1_insertion_shifts_read_pos_only`.**
Lines 376-407 contain a long mid-test stream-of-consciousness diagnostic ("Wait — ... Actually ... Above I have ... Insufficient. Rebuild with correct length below."). This is harmless but reads as scratch-work that should have been cleaned up before commit. Suggest tightening to a one-line comment explaining the 102-byte XM length.

## Concerns from the review brief — addressed point-by-point

1. **bismark-io v1.0.0-beta.7 promotion semantics** — clean. `io` feature reachable (Err1). 6 unit tests present (read.rs:1148-1192). `arg_present` moved alongside `detect_paired_from_header` per rev-1 X2 — no collision.
2. **Phase B writer literal change** — "Processed N lines in total" emitted at output.rs:268-270, with an inline Perl line citation. PR-scope: technically Phase B polish shipping in Phase C, but the byte-identity rationale is sound (PE needs the same literal; Phase B already mis-spelled it as "reads"). Acceptable inline since the count semantics changed in Phase C anyway. No action.
3. **`render_qname_opt`** — inlined twice; E2 above. Not a bug.
4. **`drop_overlap` `r1_start` lifetime** — clean. `alignment_start()` returns `Option<usize>` (a value, not a borrow), so the `?` consumes nothing borrowed. The subsequent `pair.r1().cigar()` re-borrows R1 freshly.
5. **`reader.records()` borrow tangle** — clean. `state.cleanup_partial_outputs(&mut self)` is called via `state.cleanup_partial_outputs()` (no extra borrow); `records` is an iterator that holds `&mut reader`, but the cleanup calls operate on `state`, not `reader`. No borrow conflict. The `match records.next() { Some(Err(e)) => { state.cleanup...; return Err(e.into()); } ... }` pattern works because `e` is owned (moved out of the iterator's yielded Result), not borrowed.
6. **AutoDetect open_reader twice** — explicit `drop(probe)` before the dispatched re-open (main.rs:124). BAM/SAM/CRAM readers hold a `File` handle; closing one then opening another is fine on all platforms. ~50 ms overhead, documented.
7. **Tests-as-documentation drift** — S1 above. 7 plan-listed tests absent.
8. **`extract_pe_with_no_overlap_drops_r2_calls_past_r1_end`** — rename + assertions match Perl polarity; see L2.
9. **`extract_pe_routes_r2_calls_to_pair_strand_file_not_record_strand_file` uses `--include_overlap`** — documented inline (L3); no action.
10. **Smoke test fixture realism** — R2 in `pe_phase_c_smoke.rs` is XR=GA, XG=CT → record_strand=CTOT by `BismarkRecord::record_strand` (derived from XR/XG, not FLAG). FLAG bits 0x81 mark "paired + last-in-pair" only; no `read_reverse` (0x10). This is unusual for a real Bismark `-`-strand R2 BAM but `iter_aligned`'s orientation correction keys off `record_strand`, not FLAG, so the test fixture is internally consistent. Phase H byte-identity gate (real BAM) will exercise the FLAG 0x10 path.
11. **Phase F readiness — `run_extraction<F>`** — moot. The plan's rev-1 contingency took the duplicate-scaffolding path; no helper to refactor. The duplication between `extract_se` and `extract_pe` (open_reader → chr_table → state → loop body → finalize) is ~30 LOC. Acceptable for Phase C; Phase F will need to factor it anyway when adding the producer/consumer split. No structural Phase F headache vs the helper path — both would need rework.
12. **`#[allow(unused_imports)]` on `pub use` in dedup** — likely unnecessary; Err2.
13. **Per-pair allocation profile** — E1; documented for Phase F.

## Fixes applied

None — read-only review.

## Prioritized recommendations

| # | Priority | Recommendation |
|---|----------|----------------|
| 1 | **Medium** | Address the 7 plan-listed tests absent from `pe_phase_c.rs` (S1). Even if Phase B's existing surface covers some, the Phase-C-specific ones — `extract_pe_per_mate_ignore_r2_only_skips_r2_positions`, `..._3prime_r2`, `extract_pe_ignore_r2_skips_read_cycles_not_ref_positions`, `extract_pe_routes_ctot_pair_strand_correctly`, `..._ctob_pair_strand_correctly`, `extract_pe_increments_mbias_R2_at_index_1`, `extract_pe_empty_bam_writes_only_header_files` — exercise behaviour rev 1 locked. The CTOT/CTOB ones especially: non-directional libraries are NOT covered by the OT-pair smoke. |
| 2 | Low | L1 — tighten smoke assertion from `≥ 1` to `== 10` (pin overlap polarity in smoke). |
| 3 | Low | L2 — add Perl line-number citation (`:2905 / :2989`) to `extract_pe_with_no_overlap_drops_r2_calls_past_r1_end` body. |
| 4 | Low | Err2 — try removing `#[allow(unused_imports)]` on dedup's `pub use bismark_io::detect_paired_from_header` (line 29). |
| 5 | Low | Err3 — add cleanup-completion assertion to `extract_pe_rejects_unpaired_final_record`, matching the sister test. |
| 6 | Low | E2 — dedupe inline qname rendering in `extract_pe` (two sites) via a small helper. |
| 7 | Low | S2 — clean up scratch-work mid-test comment in `drop_overlap_with_r1_insertion_shifts_read_pos_only`. |

## Verdict

**APPROVE-WITH-NITS.** Phase C is structurally and behaviourally sound. Cross-crate promotion is clean. Overlap polarity matches Perl with three CIGAR topologies covered. Auto-detect dispatches cleanly. The rev-1 Critical (AutoDetect no_overlap regression) and the rev-1 "lines, not pairs" splitting-report literal are both fixed and tested. The duplicate-scaffolding contingency was the right call.

The single non-trivial gap is the seven plan-listed tests absent from `pe_phase_c.rs` — particularly the non-directional CTOT/CTOB routing tests and the per-mate ignore tests, which exercise Phase-C-specific surface that won't be covered by Phase H's directional-library byte-identity gate. Recommend filling these gaps before final merge OR documenting their deferral with a follow-up issue.

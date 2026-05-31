# Plan Coverage Report

**Mode:** B (code vs. implementation plan)
**Plan(s):** `phase-b-core-report/IMPL.md` (T1–T9 + 26-item Plan Coverage Checklist); cross-ref `phase-b-core-report/PLAN.md` rev 1 (§3.1–§3.6, V1–V24)
**Date:** 2026-05-29
**Verdict:** INCOMPLETE — 3 items unresolved (all test-coverage gaps; production code is complete and byte-identical)

> **RESOLVED 2026-05-29 (post-audit):** all 3 gaps + review-B M-1/M-2 closed by adding discriminating tests to `tests/golden_phase_b.rs` — `covered_chromosomes_emit_in_cov_appearance_order_not_sorted` (V10), `duplicate_position_last_write_wins` (V21), `three_way_covered_then_uncovered_sorted` (V23), `blank_and_trailing_lines_are_ignored_end_to_end` (V22) — and switching the golden assertions to raw `Vec<u8>` compare (M-1). **71 tests pass**, clippy clean. Verdict now effectively COMPLETE.

## Summary

- Total ledger items: 26 (checklist) + 24 validations (V1–V24) audited
- DONE: 23 of 26 checklist items; 19 of 24 V-rows
- PARTIAL: 3 checklist items (#16 dup-position, #13 covered-ordering proof, #17 blank-line e2e) / 5 V-rows (V10, V18, V21, V22, V23) — behavior implemented, test missing or non-discriminating
- MISSING: 0 (no production code is missing)
- DEVIATED: 1 documented, accepted divergence (strict `u32` parse → `MalformedCovLine` vs Perl lenient coercion — V20; explicitly documented in PLAN §3.1.2 + error.rs)

**Test/build verification (run 2026-05-29 in `/Users/fkrueger/Github/Bismark-c2c/rust`):**
- `cargo test -p bismark-coverage2cytosine` → **67 passed, 0 failed** (55 unit + 7 golden/streaming + 5 sanity + 0 doc).
- `cargo clippy -p bismark-coverage2cytosine --all-targets -- -D warnings` → **clean**.
- All four V15 byte-identity goldens {default, cx, zero, thr} (report + summary) match.

**Provenance / commit-state caveat (process item #26):** `generate_goldens.sh` runs the repo's Perl-v0.25.1 `coverage2cytosine` (`../../../../../coverage2cytosine`) on the committed fixtures — provenance is sound. However, the new files (`src/{cov,report,summary}.rs`, `tests/golden_phase_b.rs`, all of `tests/data/`) are currently **untracked (`??`), not yet committed**. IMPL T8 and the PLAN notes state goldens "are committed"; they are staged-pending. Not a code gap — flagged so the commit step (IMPL §"Commit plan") actually stages `rust/bismark-coverage2cytosine/**`.

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | `perl_substr` negative-wrap helper | T1 / §3.3 | DONE | `report.rs:48-60`; tested `perl_substr_interior_and_truncation`, `perl_substr_negative_wraps_from_end` (V1). |
| 2 | `revcomp` (N untouched) | T1 / §3.3 | DONE | `report.rs:64-75`; tested `revcomp_complements_acgt_leaves_n` (V2). |
| 3 | `classify_context` CG/CHG/CHH/None | T1 / §3.3.6 | DONE | `report.rs:79-90`; tested `classify_context_matches_perl_regex` incl. `CNG`/`CNN`/`CCG`/len<3 (V3). |
| 4 | `ContextSummary` 64-cell + accumulate (pure-ACTG gate) + write (%.2f/N/A, sorted, header) | T2 / §3.6 | DONE | `summary.rs`; tested `summary_writes_64_rows_sorted_with_header`, `summary_accumulates_pure_actg_only_and_formats_percent` + golden summaries (V14). |
| 5 | gz-aware cov open | T3 / §3.1.1 | DONE | `cov.rs::open_cov:22-35` (`.gz`→`MultiGzDecoder`). No gz-specific unit test, but mirrors verified `genome.rs` pattern; non-blocking. |
| 6 | cov line parse: CRLF strip, blank skip, fields 0/1/4/5, strict u32 → `MalformedCovLine` | T3 / §3.1.2 | DONE | `cov.rs:42-72`; tested `parse_strips_crlf_and_reads_fields`, `parse_handles_trailing_newline`, `parse_blank_line_is_skipped`, `parse_malformed_errors` (V19/V20). |
| 7 | `EmptyCoverageInput` + `MalformedCovLine` errors | T3,T5 / §5,§3.1.5 | DONE | `error.rs:113-130`; both raised & tested (`empty_coverage_input_errors`, `parse_malformed_errors`). |
| 8 | forward-C extraction (tri_nt, upstream incl. i=0 wrap) | T4 / §3.3.1 | DONE | `report.rs::extract:95-99`; exercised by anchor test + i=0 wrap in goldens (`chr1:2 CGT`). (V4) |
| 9 | reverse-G extraction (revcomp, i<2 edge, upstream revcomp) | T4 / §3.3.1 | DONE | `report.rs::extract:101-110` (`i<2`→`seq[0..=i]` dropped by len<3); anchor `chr1:3 - CGT` + scaf_short. (V5) |
| 10 | guards: len<3, last-base, threshold | T4 / §3.3.2-5 | DONE | `report.rs:134,139,145`; tested `last_base_excluded_and_short_tri_skipped`, `threshold_filters_below_cutoff` + thr golden (V6/V7). |
| 11 | context-summary accumulate (before CpG filter, covered only) | T4 / §3.3.7 | DONE | `report.rs:154-156` (before emit filter); `accumulate_summary=false` on uncovered pass (line 289). Default summary golden shows non-CG contexts counted. |
| 12 | emit: CpG-only vs --CX; report-line bytes; --zero_based | T4 / §3.3.8,§3.4 | DONE | `report.rs:158-176`; tested `cx_emits_chg_chh_too`, `exact_report_line_bytes` (V16), `zero_based_subtracts_one` + cx/zero goldens (V8/V9). |
| 13 | streaming per-chr flush; covered = appearance order | T5 / §3.1.3/3.1.6 | PARTIAL | Streaming flush implemented (`run_report:231-277`, flush on every transition + EOF, no BTreeMap). **No test proves appearance-order ≠ sorted-order** — every test/golden uses already-sorted covered chrs (chrA→chrB, chr1→chr2). V10 not discriminating. |
| 14 | fresh-buffer seeding (triggering line) | T5 / §3.1.3 | PARTIAL | Seeding implemented (`run_report:255-258` clears then re-inserts the triggering line at 260). Covered **transitively** by `non_contiguous_chromosome_re_emits` (chrB pos2 must appear) + goldens (chr2 first covered pos), but **no dedicated V18 test**. |
| 15 | non-contiguous re-flush; `seen` ≠ flush-dedup | T5 / §3.1.4 | DONE | `run_report:243-259` (flush keyed on `chr != cur_chr`, `seen` only used at line 284 uncovered pass); tested `non_contiguous_chromosome_re_emits` (V17). |
| 16 | duplicate-position last-write-wins | T5 / §3.1.3 | PARTIAL | Implemented (`run_report:260` `buffer.insert`, last write wins). **No test exercises two cov lines at the same chr+pos** — V21 is described in the plan but not tested anywhere (grep confirms). |
| 17 | blank/trailing-line no phantom flush | T5 / §3.1.2 | PARTIAL | Parse-level skip is unit-tested (`parse_blank_line_is_skipped`), and `run_report:238-240` `continue`s on `None`. **No end-to-end test** that a cov with a blank/trailing line yields no phantom chr and does not misfire `EmptyCoverageInput` (V22 e2e leg). |
| 18 | empty-cov → `EmptyCoverageInput` before uncovered pass | T5 / §3.1.5 | DONE | `run_report:264-265` returns before the threshold==0 uncovered block; tested `empty_coverage_input_errors` (V12). |
| 19 | uncovered pass: `names_sorted()\seen`, threshold==0 only, no summary | T6 / §3.5 | DONE | `run_report:281-296`; `accumulate_summary=false`; thr golden shows uncovered suppressed (V11/V24); default/cx/zero goldens show chr3uncov sorted last. |
| 20 | cov chr absent from genome → emits nothing | T6 / §3.2 | DONE | `flush_chromosome:190-192` early-return on `genome.get`=None; tested `cov_chromosome_absent_from_genome_emits_nothing_for_it` (V13). |
| 21 | `lib.rs::run` (load genome + run_report); wire `main.rs` | T7 / §2,§4 | DONE | `lib.rs:43-50`, `main.rs:35-38`. Sanity tests run the real binary end-to-end. |
| 22 | filename derivation (.CpG_report/.CX_report/.cytosine_context_summary) | T7 / §3.4/§3.6 | DONE | `report.rs:309-319`; tested `filename_derivation`; goldens confirm file naming. |
| 23 | `open_report_writer` seam for Phase C | T7 / §5 step6 | DONE | `report.rs:329-339` `Box<dyn Write>` seam; `open_summary_writer` parallel. |
| 24 | V1–V24 validations | T1–T8 / §9 | PARTIAL | 19/24 with concrete discriminating tests; V10/V18/V21/V22/V23 are implemented but under-tested (see V-table). |
| 25 | byte-identity golden integration (V15) | T8 / §9 V15 | DONE | `golden_phase_b.rs` 4 modes × (report+summary) all pass against Perl goldens; non-round 403/803→50.19 + 408/808→50.50 present. |
| 26 | clippy/fmt/workspace build | T9 / §9 process | PARTIAL | clippy `-D warnings` clean; tests green. **Files not yet committed** (untracked) — provenance script present, but the "fixtures + goldens committed" claim is not yet true; commit step pending. |

## V-row → test mapping

| V | Behavior | Test (file:fn) | Status |
|---|----------|----------------|--------|
| V1 | `perl_substr` | report.rs: `perl_substr_interior_and_truncation`, `perl_substr_negative_wraps_from_end` | DONE |
| V2 | `revcomp` N-safe | report.rs: `revcomp_complements_acgt_leaves_n` | DONE |
| V3 | `classify_context` | report.rs: `classify_context_matches_perl_regex` | DONE |
| V4 | forward-C extraction | report.rs: `cpg_report_matches_perl_anchor` (i=0 wrap via chr1:2 in goldens) | DONE |
| V5 | reverse-G extraction | report.rs: anchor (`chr1:3 -`); scaf_short golden (2bp drop) | DONE |
| V6 | last-base exclusion | report.rs: `last_base_excluded_and_short_tri_skipped` | DONE |
| V7 | threshold guard | report.rs: `threshold_filters_below_cutoff` + `thr` golden | DONE |
| V8 | CpG vs --CX | report.rs: `cx_emits_chg_chh_too` + `cx` golden | DONE |
| V9 | `--zero_based` | report.rs: `zero_based_subtracts_one` + `zero` golden | DONE |
| V10 | covered order = cov appearance (NOT sorted) | — (all tests use already-sorted covered chrs) | **PARTIAL** — no discriminating test |
| V11 | uncovered sorted, threshold-gated | `thr` golden (suppressed) + default golden (chr3uncov last) | DONE |
| V12 | empty cov → error | golden_phase_b.rs: `empty_coverage_input_errors` | DONE |
| V13 | cov chr absent from genome | golden_phase_b.rs: `cov_chromosome_absent_from_genome_emits_nothing_for_it` | DONE |
| V14 | context summary (64 rows, %.2f/N-A, pure-ACTG) | summary.rs: 2 unit tests + all 4 summary goldens | DONE |
| V15 | byte-identity integration | golden_phase_b.rs: `golden_{default_cpg,cx,zero_based,coverage_threshold}` | DONE |
| V16 | exact report-line bytes | report.rs: `exact_report_line_bytes` (6 tabs, no trailing tab, `\n`) | DONE |
| V17 | non-contiguous chr re-flush | golden_phase_b.rs: `non_contiguous_chromosome_re_emits` | DONE |
| V18 | fresh-buffer seeding | — (transitive via V17 + goldens; no dedicated test) | **PARTIAL** |
| V19 | CRLF cov line | cov.rs: `parse_strips_crlf_and_reads_fields` | DONE |
| V20 | malformed cov line (documented divergence) | cov.rs: `parse_malformed_errors` | DONE (DEVIATED, documented) |
| V21 | duplicate-position last-write-wins | — (grep: no test) | **MISSING test** (code implemented) |
| V22 | blank/trailing line no phantom flush | cov.rs: `parse_blank_line_is_skipped` (parse-level only; no e2e) | **PARTIAL** |
| V23 | three-way interleaved ordering | — (no test of covered chr between two uncovered, bytewise) | **MISSING test** |
| V24 | threshold>0 suppresses uncovered pass | `thr` golden (only 2 covered lines, no uncovered) | DONE |

## Gaps (detail)

### Item V21 — duplicate-position last-write-wins (checklist #16)
**Expected:** a test feeding two cov lines for the same chr+pos and asserting the second `(meth,nonmeth)` is the one emitted (Perl `%chr` overwrite, `:224`).
**Found:** production code is correct (`report.rs:260` `buffer.insert(start, …)`; HashMap overwrite = last-write-wins). No test anywhere exercises a repeated position (the `non_contiguous` test uses two *different* positions pos2/pos3).
**Gap:** add a unit/integration test, e.g. cov `chrA\t2\t…\t1\t0` then `chrA\t2\t…\t9\t0` → emitted line shows `9 0`.

### Item V23 — three-way interleaved ordering (checklist #24)
**Expected:** a covered chromosome appearing mid-genome between uncovered ones, asserting covered emitted in cov-appearance order and the uncovered chromosomes bytewise-sorted around it.
**Found:** default/cx/zero goldens cover {chr1, chr2} (covered) + chr3uncov (uncovered, sorts last) — they confirm uncovered-after-covered and bytewise uncovered order, but do **not** place an uncovered chromosome that sorts *before* a covered one to prove the two passes are independent and correctly ordered.
**Gap:** add a fixture/test where, e.g., genome has `aaa`(uncovered, sorts first), `mmm`(covered), `zzz`(uncovered) and assert order = covered `mmm` block first (appearance), then `aaa`, `zzz` (sorted) in the uncovered pass.

### Item V10 — covered order = cov appearance, not sorted (checklist #13)
**Expected:** a test where covered chromosomes appear in the cov file in a *non*-sorted order (e.g. cov lists `chrB` before `chrA`) and the report emits chrB's block before chrA's — proving appearance-order, not `BTreeMap`/sorted-order.
**Found:** streaming implementation is correct (no BTreeMap; flush in transition order). But every existing test/golden lists covered chrs already in sorted order (chrA→chrB, chr1→chr2), so the test would still pass under a sorted implementation.
**Gap:** add a test with reverse-sorted covered chrs in the cov to make V10 discriminating.

### Item V18 — fresh-buffer seeding (checklist #14) — minor
**Expected (plan):** a dedicated unit asserting the first covered position of a *non-first* chromosome is present (not dropped on the transition).
**Found:** behavior is correct and **transitively** verified — `non_contiguous_chromosome_re_emits` requires chrB's seeded pos to flow through, and the chr2 covered positions appear in the goldens. No test is *named/targeted* at the seeding invariant.
**Gap (optional):** a focused regression test would make a future seeding regression fail loudly at the exact assertion rather than via a multi-chr golden.

### Item V22 — blank/trailing line no phantom flush (checklist #17) — minor
**Expected:** an end-to-end test that a cov containing a blank line / trailing newline produces no phantom chromosome and does not misfire `EmptyCoverageInput`.
**Found:** parse-level skip is unit-tested (`parse_blank_line_is_skipped`) and wired (`run_report` `continue`s on `None`). No e2e test drives a blank-line-containing cov through the binary.
**Gap (optional):** add an integration cov with an interior blank line + trailing `\n` and assert clean output.

### Process — uncommitted goldens/sources (checklist #26)
**Expected:** "Fixtures + goldens are committed" (IMPL T8 / PLAN notes).
**Found:** `git status` shows `src/{cov,report,summary}.rs`, `tests/golden_phase_b.rs`, and all of `tests/data/` as untracked. error.rs/lib.rs/main.rs are modified-tracked.
**Gap:** stage + commit `rust/bismark-coverage2cytosine/**` (and the plans dir) per IMPL's commit plan so the goldens are actually committed.

## Test verification

| Test name | File | Status |
|-----------|------|--------|
| perl_substr_interior_and_truncation | src/report.rs | PASS |
| perl_substr_negative_wraps_from_end | src/report.rs | PASS |
| revcomp_complements_acgt_leaves_n | src/report.rs | PASS |
| classify_context_matches_perl_regex | src/report.rs | PASS |
| cpg_report_matches_perl_anchor | src/report.rs | PASS |
| zero_based_subtracts_one | src/report.rs | PASS |
| cx_emits_chg_chh_too | src/report.rs | PASS |
| last_base_excluded_and_short_tri_skipped | src/report.rs | PASS |
| threshold_filters_below_cutoff | src/report.rs | PASS |
| exact_report_line_bytes | src/report.rs | PASS |
| filename_derivation | src/report.rs | PASS |
| summary_writes_64_rows_sorted_with_header | src/summary.rs | PASS |
| summary_accumulates_pure_actg_only_and_formats_percent | src/summary.rs | PASS |
| parse_strips_crlf_and_reads_fields | src/cov.rs | PASS |
| parse_handles_trailing_newline | src/cov.rs | PASS |
| parse_blank_line_is_skipped | src/cov.rs | PASS |
| parse_malformed_errors | src/cov.rs | PASS |
| golden_default_cpg | tests/golden_phase_b.rs | PASS |
| golden_cx | tests/golden_phase_b.rs | PASS |
| golden_zero_based | tests/golden_phase_b.rs | PASS |
| golden_coverage_threshold | tests/golden_phase_b.rs | PASS |
| non_contiguous_chromosome_re_emits | tests/golden_phase_b.rs | PASS |
| empty_coverage_input_errors | tests/golden_phase_b.rs | PASS |
| cov_chromosome_absent_from_genome_emits_nothing_for_it | tests/golden_phase_b.rs | PASS |
| (genome::* — 9 Phase-A tests) | src/genome.rs | PASS |
| (sanity::* — 5 CLI/version tests) | tests/sanity.rs | PASS |
| **V21 duplicate-position last-write-wins** | (none) | MISSING |
| **V23 three-way interleaved ordering** | (none) | MISSING |
| **V10 non-sorted covered-order discriminator** | (none) | MISSING |

Total: 67 tests, 67 PASS, 0 FAIL. clippy `-D warnings` clean.

## Verdict

**INCOMPLETE — 3 items unresolved.** All production code for T1–T9 / §3.1–§3.6 exists, is correct, and is byte-identical to Perl v0.25.1 on the {default, --CX, --zero_based, --coverage_threshold} golden matrix (report + summary). The gaps are **test-coverage** gaps for behaviors the plan explicitly promised tests for, plus one process item:

1. **V21 (duplicate-position last-write-wins)** — code implemented (`report.rs:260`), but no test feeds two cov lines at the same chr+pos. Plan checklist #16 + V21 promised this test.
2. **V23 (three-way interleaved ordering)** — no test places a covered chromosome between two uncovered ones (bytewise) to prove the covered-appearance vs uncovered-sorted passes are independent. Plan checklist #24 + V23 promised this.
3. **V10 (covered = cov-appearance order, not sorted)** — implemented via streaming, but every test/golden uses already-sorted covered chrs, so the assertion does not distinguish appearance-order from a hypothetical sorted implementation.

Lower-severity, optional hardening (behavior verified transitively/at parse level, not by a targeted test): **V18** fresh-buffer-seeding dedicated test, **V22** end-to-end blank/trailing-line test.

Process: the new sources, tests, and goldens are **untracked** — IMPL's commit step must stage `rust/bismark-coverage2cytosine/**` so the "goldens committed" claim holds.

No production logic is missing or wrong; the documented `MalformedCovLine` strict-parse divergence (V20) is the only intentional deviation and is fully documented + tested.

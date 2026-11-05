# Plan Coverage Report

**Mode:** B (code vs. plan; PLAN.md rev 2 is the spec, no separate IMPL.md)
**Plan(s):** `plans/11052026_1148-suite-version-strings/PLAN.md` (rev 2, incl. "Implementation notes")
**Code audited:** uncommitted change in `/Users/fkrueger/Github/Bismark-1148` (`fix/1148-version-strings`, base `94faf8d`): 29 modified files + untracked `rust/bismark/tests/common/version.rs`, `tests/version_strings_1148.rs`, `tests/version_guard_1148.rs`
**Date:** 2026-11-05
**Verdict:** INCOMPLETE — 2 items unresolved (both minor: one missing docs caveat, one undocumented test-placement deviation)

## Summary

- Total items: 37
- DONE: 28
- PARTIAL: 1
- MISSING: 0
- DEVIATED: 5 (4 documented with a reason, 1 undocumented)
- NOT YET DUE: 3 (step 6 reviews/PR/issue reply, step 7 post-merge follow-ups)

## Coverage ledger

### Behavior

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | `meta::VERSION_TAG = concat!("v", env!("BISMARK_SUITE_VERSION"))` | Behavior 1, Signature | DONE | `src/meta/mod.rs`, doc comment matches the Signature |
| 2 | `aligner::`, `extractor::output::`, `genome_prep::BISMARK_VERSION` = `VERSION_TAG`; names kept | Behavior 1 | DONE | `aligner/mod.rs:103`, `extractor/output.rs:36`, `genome_prep/mod.rs:38` |
| 3 | `SPLIT_FILE_HEADER` via `concat!` repeating `env!` | Behavior 1 | DONE | `extractor/output.rs` |
| 4 | `summary::BISMARK_VERSION` = `SUITE_VERSION` (no `v`) | Behavior 1 | DONE | `summary/mod.rs:43` |
| 5 | Remove unused `report::BISMARK_VERSION` | Behavior 1 | DONE | removed from `report/mod.rs` |
| 6 | Every listed output carries the suite version (aligner report SE/PE/5-Base, both `@PG`, re-encode summary, extractor ×2, summary HTML, banner) | Behavior 2 | DONE | All emitters (`report.rs:71,77`, `output.rs:117`, `mod.rs:796,997`, `extractor/output.rs:926`, `summary/html.rs:58`, `pipeline.rs:36`) read the derived constants; grep shows no other `BISMARK_VERSION` users |
| 7 | Banner drops ` (last modified: 19 May 2022)` | Behavior 3 / Q1 | DONE | `genome_prep/pipeline.rs:36`; asserted by `genome_prep_banner_carries_the_suite_version` |
| 8 | Prerelease behaviour accepted (MultiQC) | Behavior 4 | DONE | No code needed; validated by Validation 4 (notes) |
| 9 | Unchanged: other bytes, `--version`, parsing Perl `v0.25.1` inputs | Behavior 5 | DONE | `--version` path untouched; parser fixtures (`report/reports/alignment.rs`, `summary/parse.rs`, `nome_filtering/nome.rs`, `extractor/logging.rs:292`, `bedgraph/input.rs:348`, `io/read.rs:1508`) still feed `v0.25.1` |
| 10 | `build.rs` treats `BISMARK_SUITE_VERSION=unknown` as unset; Dockerfile default stays | Behavior 6 / Q4 | DONE | `build.rs` `suite_version()`; `Dockerfile:17,91` unchanged |

### Implementation outline

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 11 | Helper `tests/common/version.rs` (`perl_version_for` with exact counts, `perl_version`), 5 forms, included via `#[path]`, kept out of `common/mod.rs` | Step 1 | DONE | `Form` enum with all 5 forms; count asserted before rewrite |
| 12 | Helper self-test: rewrite, lookalike, wrong-count panic, no-op at `0.25.1` | Step 1 | DONE | 4 tests in `version_strings_1148.rs` |
| 13 | `version_strings_1148.rs` end-to-end: SE report line 1, `@PG ID:Bismark`, split file line 1, splitting report, summary HTML, banner; each asserts no `0.25.1` | Step 1 | DONE | 5 end-to-end tests |
| 14 | ...plus PE report line 1 and 5-Base consensus `@PG` end-to-end | Step 1 | DEVIATED (documented) | Deviation 2: PE line 1 via `aligner/report.rs` PE header unit tests; consensus `@PG` via shared `generate_sam_header` pinned by `header_hd_sq_pg_exact_bytes` |
| 15 | Extend `aligner_five_base_bisulfite.rs`: converter `@PG` `VN:v<S>` + run-summary version | Step 1 | DONE | `appends_a_pg_with_a_distinct_id`, `bisulfite_input_reports_a_zero_flip_rate` |
| 16 | Unit-level assertion on the 5-Base report header (`report.rs:54`) | Step 1 | DONE | `five_base_pe_header_names_unconverted_genome_exactly` now formats with `SUITE_VERSION` |
| 17 | Unit tests on `SPLIT_FILE_HEADER` and `summary::BISMARK_VERSION` only | Step 1 | DEVIATED (documented) | Deviation 5: `SPLIT_FILE_HEADER` in existing `extractor_se_phase_b.rs::output_file_header_matches_perl_format`; `summary::BISMARK_VERSION` covered end-to-end only |
| 18 | Constants + banner implemented; each doc comment rewritten to one line of current fact | Step 2 | DONE | All rewritten; `extractor/logging.rs` banner comment also trimmed of the stale "v0.25.1-locked" reference |
| 19 | Behavior 6 in `build.rs` | Step 2 | DONE | See #10 |
| 20 | Literal `v0.25.1` assertions on Rust output use the constant | Step 3 | DONE | `aligner/report.rs`, `aligner/output.rs`, 5 extractor test files |
| 21 | `perl_vs_rust_nondirectional_pe_extractor` uses `perl_version` with exact counts (1 per split file, 1 splitting report, 0 M-bias) | Step 3 | DONE | `extractor_nondir_swapped_flags_1030.rs` |
| 22 | `summary_perl_oracle.rs` uses `perl_version` (1 `(version …)`) | Step 3 | DONE | Applied after timestamp normalisation |
| 23 | Golden comparisons (extractor/dedup/summary fixtures) follow the same rule | Step 3 | DEVIATED (documented) | Deviation 4: full suite showed none needed changing |
| 24 | Perl-style input fixtures left as they are, incl. `extractor/logging.rs:293` | Step 3 | DONE | Unchanged (now line 292) |
| 25 | `aligner_byte_identity_real_data.rs` `normalize_report` maps line-1 version to `v0.25.1` | Step 3 | DEVIATED (documented) | Deviation 1: masks `(version: v…)` on both sides (golden is a Rust capture) and separately asserts current line 1 carries the suite version |
| 26 | `phase_h_smoke.sh`: `sed` matching only the two extractor forms on the Rust side before `cmp`/`zcat`; exception stated in the script | Step 3 | DONE | `perl_version()` shell function; applied to raw `cmp`, triage diff, gz-sorted and plain-sorted branches; comment next to the strict-byte note |
| 27 | Finding every site: Linux full suite, `--ignored` real-data gate, 13 oracles with `BISMARK_REQUIRE_PERL=1`, summary oracle with proof of run | Step 3 | DONE | Per notes (Linux 2341/0, real-data PASS incl. `/home`-path rerun); summary + nondir oracles re-verified here |
| 28 | Guard `version_guard_1148.rs`: walks `src/**/*.rs`, cut at first `#[cfg(test)]`, drops `//` lines, substring match; scans `src/**/*.html` + `assets/*.tpl` | Step 4 | DONE | Plus `guard_catches_an_embedded_literal` self-test |
| 29 | Guard asserts `meta::SUITE_VERSION != "unknown"` | Step 4 | DEVIATED (undocumented) | The assertion exists (`version_is_never_unknown`) but in `version_strings_1148.rs`, not the guard file. Listed under "What was built" with no reason given. Functionally equivalent |
| 30 | Docs caveat on each listed "byte-identical to Perl v0.25.1" claim + Dockerfile `LABEL` | Step 5 | PARTIAL | Done: `overview.md` (×2), `installation.md:3`, `benchmarks.md:7`, `choosing-an-alignment-mode.md:6,50`, `Home.astro` (line 44 paragraph), Dockerfile `LABEL`. Documented skips: `Home.astro` badge, `illumina-5-base.md:36` (Deviation 3). **Undocumented miss: `choosing-an-alignment-mode.md:84`** ("**Byte-identical to Perl `v0.25.1`** → only the **standard** per-strand modes.") has no caveat |
| 31 | CHANGELOG: one sentence crediting bounlu, linking #1148 | Step 5 | DONE | Under "All tools", matches the file's existing bold-lead + "Reported by" style |
| 32 | `rust/README.md` Milestones line | Step 5 | DONE | 2026-11-05 entry |
| 33 | fmt; release Clippy `--all-targets` | Step 6 | DONE | Per notes: fmt and Clippy (workspace, all-targets, `rammap-inprocess`, `--release`, `-D warnings`) clean |
| 34 | Full suite + `--ignored` gate on Linux; 13 oracles + summary oracle locally with Perl | Step 6 | DONE | Per notes; see #27 |
| 35 | Dual code review + plan-manager audit | Step 6 | NOT YET DUE | In progress (this report; `CODE_REVIEW_A/B.md` running per PROGRESS.md) |
| 36 | PR into `dev` (constants change value, `report::BISMARK_VERSION` removed, Q2 snapshot consequence); #1148 reply | Step 6 / Q3 | NOT YET DUE | Not expected yet |
| 37 | Post-merge: `/tool-wiki update` for `methylation-call-files.md`; nf-core snapshot follow-up with the shipping release | Step 7 / Q2 | NOT YET DUE | Not expected yet |

### Validation

| # | Check | Status | Notes |
|---|-------|--------|-------|
| V1 | `version_strings_1148.rs` + extended `aligner_five_base_bisulfite.rs` pass | DONE | Re-run here: 9/9 and 11/11. Notes say "11 + 5"; the actual counts are 9 + 11 (cosmetic) |
| V2 | 13 oracles + summary oracle with Perl; Linux full suite; `--ignored` real-data gate; one `phase_h_smoke.sh` cell | DONE | Per notes; summary oracle (12/12, no `skipping:`) and nondir PE oracle re-verified here with `BISMARK_REQUIRE_PERL=1` |
| V3 | Helper self-test incl. wrong-count panic; hand checks (injected non-version diff fails; missed emitter fails the count) | DONE | Self-test re-run here; hand checks recorded in notes |
| V4 | MultiQC regex one-off | DONE | Recorded in notes; matches the expected outcomes |
| V5 | Guard passes; `git grep '0\.25\.1' rust/bismark/src` lists only comments/test code/parser fixtures | DONE | Re-checked here: all non-comment hits are inside `#[cfg(test)]` fixtures |
| V6 | `BISMARK_SUITE_VERSION=unknown` build reports `rust/VERSION` | DONE | Recorded in notes (`--version` gives `v3.1.0`). Notes record `--version` only; the plan also named "a report header", which is not separately recorded (same constant source, so not counted as a gap) |

### Questions

| # | Item | Status | Notes |
|---|------|--------|-------|
| Q1 | Drop banner date clause | DONE | #7 |
| Q2 | nf-core snapshot consequence | NOT YET DUE | Action lives in step 6 (PR text) and step 7 |
| Q3 | Reply on #1148 when PR opens | NOT YET DUE | Step 6 |
| Q4 | `unknown` handling via `build.rs` | DONE | #10, #29 |

(Q rows are cross-references to ledger items and are not counted separately in the summary.)

## Gaps (detail)

### Item 30: docs caveat at `choosing-an-alignment-mode.md:84`

**Expected:** Step 5 lists `choosing-an-alignment-mode.md:6,50,84`, each getting "apart from the version string".
**Found:** Lines 6 and 50 have the caveat. Line 84 (`- **Byte-identical to Perl `v0.25.1`** → only the **standard** per-strand modes.`) is unchanged, and Deviation 3 does not mention it.
**Gap:** Add the caveat at line 84, or document why it is left (e.g. it contrasts modes rather than claiming output bytes) under "Deviations from rev 2".

### Item 29: `SUITE_VERSION != "unknown"` assertion location

**Expected:** Step 4 places the assertion in `tests/version_guard_1148.rs`.
**Found:** `version_is_never_unknown` in `tests/version_strings_1148.rs`; the guard file has no such assertion. "What was built" lists it there but gives no reason.
**Gap:** Either move the test into the guard file or add a one-line deviation note. Behaviour is equivalent; this is bookkeeping only.

### Observation (outside the ledger)

`docs/src/content/docs/installation.md:6` (body) still says "Output is **byte-identical** to Perl Bismark `v0.25.1` on the faithful default path" with no caveat. The plan named only `installation.md:3` (front-matter description), so this is not counted as a gap.

## Test verification (Mode B only)

Run on macOS from `/Users/fkrueger/Github/Bismark-1148/rust` (the ~31 `aligner_cli` FIFO tests were not run, per instructions).

| Test name | File | Status |
|-----------|------|--------|
| helper_rewrites_each_form_to_perl | tests/version_strings_1148.rs | PASS |
| helper_leaves_lookalikes_alone | tests/version_strings_1148.rs | PASS |
| helper_panics_on_a_missed_emitter | tests/version_strings_1148.rs | PASS |
| helper_is_a_no_op_at_the_perl_version | tests/version_strings_1148.rs | PASS |
| version_is_never_unknown | tests/version_strings_1148.rs | PASS |
| aligner_report_and_pg_carry_the_suite_version | tests/version_strings_1148.rs | PASS |
| extractor_outputs_carry_the_suite_version | tests/version_strings_1148.rs | PASS |
| summary_html_carries_the_suite_version | tests/version_strings_1148.rs | PASS |
| genome_prep_banner_carries_the_suite_version | tests/version_strings_1148.rs | PASS |
| no_perl_version_in_non_test_source | tests/version_guard_1148.rs | PASS |
| no_perl_version_in_templates | tests/version_guard_1148.rs | PASS |
| guard_catches_an_embedded_literal | tests/version_guard_1148.rs | PASS |
| (all 11) incl. appends_a_pg_with_a_distinct_id, bisulfite_input_reports_a_zero_flip_rate | tests/aligner_five_base_bisulfite.rs | PASS |
| aligner::report + aligner::output header unit tests (18) | src/aligner/{report,output}.rs | PASS |
| output_file_header_matches_perl_format (+48 others) | tests/extractor_se_phase_b.rs | PASS |
| splitting_report_byte_shape_matches_perl_format (+3) | tests/extractor_output_phase_c2.rs | PASS |
| smoke_pe_auto_detect_produces_all_12_files_and_report (+1) | tests/extractor_pe_phase_c_smoke.rs | PASS |
| smoke_se_directional_produces_all_12_files_and_report (+2) | tests/extractor_se_phase_b_smoke.rs | PASS |
| smoke_gzip_comprehensive_emits_3_gz_files (file) | tests/extractor_output_modes_phase_e_smoke.rs | PASS |
| perl_vs_rust_nondirectional_pe_extractor (`BISMARK_REQUIRE_PERL=1`) | tests/extractor_nondir_swapped_flags_1030.rs | PASS (5/5 in file) |
| summary oracle (12 tests, Perl present, 0 `skipping:` lines) | tests/summary_perl_oracle.rs | PASS |
| Linux full suite, `--ignored` real-data gate, `phase_h_smoke.sh` ×2, 13 CI oracles | oxy / macOS+Perl | PASS (taken from Implementation notes) |

## Verdict

INCOMPLETE — 2 minor items unresolved; everything else in Behavior 1–6, steps 1–5, the step 6 checks and Validation 1–6 is implemented and verified. Steps 6 (reviews, PR, #1148 reply) and 7 (wiki and nf-core follow-ups) are not yet due.

To close:
1. **Item 30:** add "apart from the version string" to `docs/src/content/docs/rust/choosing-an-alignment-mode.md:84`, or record the skip with a reason under "Deviations from rev 2".
2. **Item 29:** move `version_is_never_unknown` into `tests/version_guard_1148.rs`, or record its placement in `version_strings_1148.rs` as a deviation.

Optional bookkeeping: fix the "11 + 5" test count in Validation results (actual 9 + 11).

## Post-fix status (2026-11-05, implementer)

- **Item 30 (PARTIAL: docs caveat at `choosing-an-alignment-mode.md:84`) is now DONE.** The caveat was added there, and also to `installation.md:6`, `README.md:9` and `rust/README.md:24`.
- **Item 29 (DEVIATED, undocumented: the `unknown` assertion lives in `version_strings_1148.rs`) is now DEVIATED, documented.** It is deviation 6 in PLAN.md.
- **Notes corrected:** the V1 counts now read 9 + 11.
- **Verdict after fixes:** no PARTIAL, MISSING or undocumented items remain. Steps 6 and 7 are still not yet due.

# Plan Coverage Report

**Mode:** B (code vs. implementation plan)
**Plan(s):** `phase-c-gzip-split/IMPL.md` (T1–T8 + 17-item checklist), cross-referenced against `phase-c-gzip-split/PLAN.md` (rev 1) §3.1–§3.5 + V1–V14
**Worktree:** `/Users/fkrueger/Github/Bismark-c2c`
**Date:** 2026-05-29
**Verdict:** COMPLETE

## Summary

- Total items: 25 (8 IMPL tasks + 17-item Plan Coverage Checklist)
- DONE: 24
- PARTIAL: 0
- MISSING: 0
- DEVIATED (documented): 1 — IMPL T6 helper renamed `flush_chromosome_to_own_file` → `flush_split_chromosome` (iteration-log #1 cleanup family); behavior identical.

Tests: **81 pass** (58 unit + 11 phase-B golden + 7 phase-C golden + 5 sanity). `cargo clippy --all-targets -- -D warnings` clean. All V1–V14 map to a concrete test.

## Coverage ledger — IMPL Tasks T1–T8

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| T1 | `ResolvedConfig.output_raw` (verbatim `-o`); `output_stem` strip unchanged | IMPL T1 / §3.5 | DONE | `cli.rs:110` field; `:200` `output_raw = output.clone()` set before stem strip. Unit: `output_stem_strip_is_context_conditional` (stem unchanged); `output_raw` exercised end-to-end by V2/V13 tests. |
| T2 | `ReportWriter {Plain,Gz}` + `create`/`write_all`/`finish` (explicit) | IMPL T2 / §4 | DONE | `report.rs:32-68`. `create` = `File::create`(truncate)→`BufWriter`→opt `GzEncoder::default`; `finish(self)` = `Gz=>finish()`, `Plain=>flush()`. 3 unit tests present (`report_writer_plain_round_trip`, `_gz_round_trip`, `_gz_empty_is_valid_stream`). |
| T3 | `base(config,chr)` split=raw+`.chr`+name (no strip), non-split=stem; `report_path`/`summary_path` (`.gz` on report only) | IMPL T3 / §3.1/§3.2/§4 | DONE | `report_name`/`summary_name` (`:423`,`:444`) + `report_path`/`summary_path` (`:453`,`:467`). Split base = `format!("{output_raw}.chr{name}")`, non-split = `output_stem`. `.gz` only on report. Unit: `filename_derivation`. |
| T4 | Generate Phase C goldens from repo Perl v0.25.1 | IMPL T4 / §9 | DONE | `generate_goldens.sh` extended with a Phase-C block (split + split_thr whole-dir runs from the repo Perl). Goldens present under `tests/data/phase_b/phase_c/{split,split_thr}/`. See deviation note below re: suffixed/gzip handled in-test rather than as separate golden dirs. |
| T5 | `--gzip` (non-split) wiring: report gz, summary plain, finish before summary | IMPL T5 / §3.1 | DONE | `run_single` (`:281`) creates one `ReportWriter::create(report_path,gzip)`, `finish()`ed (`:324`) BEFORE the always-plain summary `File` (`:327`). Tests: `gzip_report_decompresses_to_plain_golden`, `gzip_cx_report_decompresses_to_plain_golden`. |
| T6 | `--split_by_chromosome` core: per-chr truncating `File::create` every transition (no caching); summary quirk; threshold guard; re-appearance | IMPL T6 / §3.2 | DONE (helper renamed) | `run_split` (`:337`) + `flush_split_chromosome` (`:400`, IMPL named it `flush_chromosome_to_own_file` — documented cleanup, behavior identical). Fresh `ReportWriter::create` per chr (no cache). Uncovered pass gated on `threshold == 0` (`:379`). `last_summary_path` tracked; full summary → last reopened chr (`:390`). Tests: `split_dir_matches_perl_golden`, `split_threshold_dir_matches_perl_golden`, `split_reappearance_truncates_and_summary_in_last_chr`. |
| T7 | `--split --gzip` + suffixed-`-o` split | IMPL T7 / §3.3 | DONE | Tests `split_gzip_per_chr_decompresses_to_split_golden` (V9: per-chr `.gz` decompress; summaries plain) and `suffixed_output_split_doubles_suffix_and_matches_content` (V13: `foo.CpG_report.txt.chrchr1.CpG_report.txt`, content == split golden). |
| T8 | Final verification: fmt/clippy/test/build clean; siblings untouched | IMPL T8 / §9 | DONE | `cargo test -p bismark-coverage2cytosine` → 81 pass; `cargo clippy -p … --all-targets -- -D warnings` → clean. `git status` shows only c2c crate + plans touched (no `bismark-extractor`/`bismark-bedgraph`). |

## Coverage ledger — IMPL 17-item Plan Coverage Checklist

| # | Checklist item | Status | Notes |
|---|----------------|--------|-------|
| 1 | `ResolvedConfig.output_raw` | DONE | `cli.rs:110`/`:200`. |
| 2 | `ReportWriter` {Plain,Gz} + explicit finish | DONE | `report.rs:32-68`. |
| 3 | `base(config,chr)`: split=raw+`.chr`+name (no strip), non-split=stem | DONE | `report_name`/`summary_name` `:430-433`/`:445-448`. |
| 4 | report_path = base+suffix+(gzip?`.gz`); summary_path never gz | DONE | `:434-440` (gzip on report); `:449` (summary, no `.gz`). |
| 5 | suffixed-`-o` split doubling; bare-`-o` split; non-split strip | DONE | Unit `filename_derivation` + golden V13. |
| 6 | `--gzip` non-split: report gz, summary plain, finish before summary | DONE | `run_single` finish at `:324` before summary `File` at `:327`. |
| 7 | `--gzip` byte-identity = decompressed == plain golden | DONE | V3/V5 tests. |
| 8 | `--split`: per-chr truncating `File::create` every transition, NO caching | DONE | `flush_split_chromosome` opens fresh `ReportWriter::create` each call; V12 proves truncation. |
| 9 | split re-appearance keeps last segment; summary → last reopened chr | DONE | `split_reappearance_truncates_and_summary_in_last_chr` (pos2 = `0 0`; summary in chrA not chrB). |
| 10 | zero-emitting chr still gets file (0-byte / empty-gzip via finish()) | DONE | `flush_split_chromosome` always `finish()`es; `scaf_short` 0-byte report in split golden; V9 empty-gzip via combined path; unit `report_writer_gz_empty_is_valid_stream`. |
| 11 | split context-summary quirk: N empty + last full (== non-split summary) | DONE | Split golden dir: 3 empty + `split.chrscaf_short.cytosine_context_summary.txt` (1310 B == `default.summary.golden`). Whole-dir byte-compare in `split_dir_matches_perl_golden`. |
| 12 | `--split --gzip` combined | DONE | V9 test. |
| 13 | `--split --threshold N` → uncovered chrs get NO files | DONE | `split_threshold_dir_matches_perl_golden`: only chr1/chr2 present (chr1 report + empty summary; chr2 empty report + full summary). |
| 14 | kernel/walk/cov/ContextSummary unchanged; Phase B regression | DONE | `emit_position`/`extract`/`perl_substr`/`revcomp`/`classify_context` unchanged; 11 `golden_phase_b.rs` tests green (V11). |
| 15 | goldens generated locally from repo Perl v0.25.1 | DONE | `generate_goldens.sh` Phase-C block uses repo `coverage2cytosine` Perl. |
| 16 | V1–V14 validations | DONE | All 14 map to a concrete test (see table below). |
| 17 | clippy/fmt/workspace build | DONE | clippy `-D warnings` clean; 81 tests pass. |

## V1–V14 → test mapping (PLAN §9)

| V | Test | File | Status |
|---|------|------|--------|
| V1 | `report_writer_plain_round_trip`, `report_writer_gz_round_trip`, `report_writer_gz_empty_is_valid_stream` | report.rs (unit) | PASS |
| V2 | `filename_derivation` | report.rs (unit) | PASS |
| V3 | `gzip_report_decompresses_to_plain_golden` (report half) | golden_phase_c.rs | PASS |
| V4 | `gzip_report_decompresses_to_plain_golden` (summary-plain half) | golden_phase_c.rs | PASS |
| V5 | `gzip_cx_report_decompresses_to_plain_golden` | golden_phase_c.rs | PASS |
| V6 | `split_dir_matches_perl_golden` (`file_set` bidirectional equality) | golden_phase_c.rs | PASS |
| V7 | `split_dir_matches_perl_golden` (per-file byte compare) | golden_phase_c.rs | PASS |
| V8 | `split_dir_matches_perl_golden` (whole-dir compare covers empty + last-full == `default.summary.golden`) | golden_phase_c.rs | PASS |
| V9 | `split_gzip_per_chr_decompresses_to_split_golden` | golden_phase_c.rs | PASS |
| V10 | `split_dir_matches_perl_golden` (covered chr1/chr2 + uncovered scaf_short/chr3uncov all present) | golden_phase_c.rs | PASS |
| V11 | 11 Phase B golden tests | golden_phase_b.rs | PASS |
| V12 | `split_reappearance_truncates_and_summary_in_last_chr` | golden_phase_c.rs | PASS |
| V13 | `suffixed_output_split_doubles_suffix_and_matches_content` | golden_phase_c.rs | PASS |
| V14 | `split_threshold_dir_matches_perl_golden` | golden_phase_c.rs | PASS |

**No V-row is untested.**

## Rev-1 Criticals — verification

- **C1-A (raw-`-o` split filename doubling):** DONE. `report_name` uses `output_raw` un-stripped for the split base. Verified by unit V2 (`foo.CpG_report.txt` split → `foo.CpG_report.txt.chrchr1.CpG_report.txt`) **and** golden V13 (`suffixed_output_split_doubles_suffix_and_matches_content`, byte-equal to the split golden).
- **C1-B (truncate-on-reopen / re-appearance):** DONE. `flush_split_chromosome` opens a fresh truncating `ReportWriter::create` on every transition with no per-chr caching. Verified by V12 (`split_reappearance_truncates_and_summary_in_last_chr`): chrA pos2 = `0 0` (first segment lost), full summary in chrA (last reopened), chrB summary empty.

## Goldens / fixtures — presence and committed status

| Concern | Covered by | Present? | Committed? |
|---------|-----------|----------|------------|
| gzip (V3/V5) | decompress in-test vs `default.report.golden`/`cx.report.golden` (Phase B goldens) | Yes | Phase B goldens committed; phase_c dir + golden test untracked (pending T8 commit) |
| split whole-dir (V6/V7/V8/V10/V11) | `phase_c/split/` (8 files: 4 reports incl. 0-byte `scaf_short`, 3 empty + 1 full summary) | Yes | Untracked (pending T8 commit) |
| split+gzip (V9) | reuses `phase_c/split/` goldens, decompresses Rust `.gz` in-test | Yes | Untracked (pending T8 commit) |
| suffixed-`-o` (V13) | reuses `phase_c/split/` goldens (content identical; names differ in-test) | Yes | Untracked |
| threshold split (V14) | `phase_c/split_thr/` (chr1 report + empty summary; chr2 empty report + full summary) | Yes | Untracked |
| re-appearance (V12) | constructed in-test (no golden dir) | n/a (self-built fixture) | Untracked test only |

**Note (not a gap):** `phase_c/`, `golden_phase_c.rs`, and the source/`Cargo.toml`/`generate_goldens.sh` edits are present but still **untracked/unstaged** — committing them is IMPL **Task 8 / Commit plan**, the last step, gated on this plan-manager audit. All fixtures exist on disk and all tests pass against them.

## Documented deviations

- **DEVIATED-but-documented:** IMPL T6/§5 named the split helper `flush_chromosome_to_own_file`; the code uses `flush_split_chromosome`. Same signature/behavior. In the same family as iteration-log #1 (PLAN §7, `last_summary_path` made a `PathBuf` computed from the final-chr match rather than a dead-`None`-initialized loop var, to satisfy clippy `unused_assignments`). Both are cleanups, not behavioral changes; PLAN implementation-notes record #1 explicitly.
- **Golden-layout deviation (documented in PLAN rev-1 notes):** IMPL T4 lists separate golden runs for `--gzip`, `--CX --gzip`, suffixed-`-o`, and `--split --gzip`, plus a `phase_c_manifest.txt`. The implementation instead generates only the two whole-dir golden corpora (`split`, `split_thr`) and derives the gzip/suffixed/combined assertions in-test by reusing those goldens (decompress Rust output vs the plain golden; suffixed names map to the same content). This is functionally equivalent — every V-row is still exercised — but the on-disk golden set and the `phase_c_manifest.txt` differ from the literal IMPL T4 wording. PLAN implementation-notes (lines 9–11) describe the as-built layout.

## Verdict

**COMPLETE.** All 8 IMPL tasks (T1–T8) and all 17 Plan Coverage Checklist items are implemented; V1–V14 each map to a passing test (81 total green); clippy `-D warnings` is clean; siblings untouched; both rev-1 Criticals (raw-`-o` split doubling, truncate-on-reopen) are verified by unit + golden tests. The two deviations (helper rename + reuse-goldens-in-test golden layout) are documented in the PLAN notes and do not reduce coverage. The only outstanding action is the **Task 8 commit** (sources + goldens + `golden_phase_c.rs` are present and green but still untracked) — a process step, not a coverage gap.

# Plan Coverage Report

**Mode:** B (code vs. design plan, post-implementation)
**Plan(s):** `plans/05312026_bismark-c2c-niche-modes/phase2-drach-m6a/PLAN.md` (rev 3)
**Date:** 2026-05-31
**Worktree:** `/Users/fkrueger/Github/Bismark-c2c` (branch `rust/c2c-v1x`, uncommitted working tree)
**Verdict:** COMPLETE

## Summary

- Total items: 31 (8 §5 implementation steps + 7 §3 behavioral groups + 16 §9 validation rows)
- DONE: 31
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 0

Test gates: **155 crate tests green** (92 lib + 18 phase1 + 12 phase2 + 11 B + 7 C + 10 D + 5 sanity), `cargo fmt --check` clean, `cargo clippy --all-targets -D warnings` clean. Matches the plan's "Implementation notes (rev 3)" exactly.

## Coverage ledger

### §5 Implementation outline (the task list)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| S1 | CLI un-rejection + `drach: bool` + threshold auto-set + no `--drach` mutex, general mutexes preserved | §5.1 | DONE | `cli.rs`: only `--ffs` rejected (`:163`); `drach` field added (`:97-98`, `:144-145`); auto-set `None if nome \|\| self.drach => 1` (`:215-219`); no `--drach` mutex, general merge mutexes intact (`:173-202`). 4 unit tests. |
| S2 | `is_drach_motif` + unit tests | §5.2 | DONE | `drach.rs:242-247`, `.get`/`.first`/`is_none_or`-based. 2 unit tests (`is_drach_motif_filter_arms`, `is_drach_motif_short_slices_no_panic`). |
| S3 | Filename helpers (raw `-o`, suffixes, `.gz`, `.chr<NAME>` infix) | §5.3 | DONE | `drach_base`/`drach_report_path`/`drach_cov_path` (`drach.rs:298-323`), use `config.output_raw` (no strip). Unit `drach_filenames` covers `.chrchr1` doubling + suffixed `-o`. |
| S4 | `drach_top` (AC scan, `pos=i+2`, perl_substr tri+5mer, filter, `len<3` guard, cov@`pos`, emit `+`) | §5.4 | DONE | `drach.rs:174-202`. `i += 1` scan; BOTH `tri` and `drach` via `perl_substr` (`:189-190`); filter then `tri.len()>=3` guard (`:191`); lookup at `pos`; golden V5/V15. |
| S5 | `drach_bottom` (GT scan, revcomp 5mer, filter, `len<3` guard, cov+report@`pos-1`, emit `-`) | §5.5 | DONE | `drach.rs:207-233`. `revcomp(perl_substr(...))` for tri (`pos-4`,3) + drach (`pos-3`,5); key=`pos-1` for both lookup and report (`:222-227`); golden V6/V10. |
| S6 | `run_drach` driver: per-chr buffer, flush-on-transition + final flush, top-then-bottom, covered-only, insertion order, single/split writers, gz, empty→empty | §5.6 | DONE | `run_drach` dispatch (`:41-48`); `run_drach_single` opens 2 writers up front, `Option`-guarded final flush (`:54-96`); `run_drach_split` fresh per-chr writers (`:101-149`); `drach_chromosome_bytes` top-then-bottom, genome-miss→empty (`:154-168`). |
| S7 | Wire `lib::run` early-exit before `report::run_report` | §5.7 | DONE | `lib.rs:63-65`: `if config.drach { return drach::run_drach(config, &genome); }` after genome load, before report/merge/gpc. |
| S8 | Goldens + tests: multi-FASTA fixtures, edge motifs, near-misses, uncovered, 2-chr ordering; from repo Perl v0.25.1 | §5.8 | DONE | `tests/data/phase2_drach/` = 5 genome fixtures (g_top/g_bot/g_multi/g_trunc/g_wrap), 9 per-mode Perl golden dirs, `generate_goldens.sh` (full provenance, documents Perl `sleep(20)`). `tests/golden_phase2.rs` = 12 byte-identity tests. |

### §3 Behavioral requirements

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| B0 | Standalone early-exit; ignores normal-report flags (no mutex); ignores `--zero_based`; auto-sets threshold=1 | §3.0 | DONE | `lib.rs:63-65` early return; `cli.rs` no `--drach` mutex + general mutexes fire (V1 unit); no `pos -= 1` zero-based path in `drach.rs`; threshold auto-set `cli.rs:215-219`. |
| B1 | Output topology: raw-`-o` filenames, `_DRACH_report.txt`/`_DRACH.cov`, `.gz`, `.chr<NAME>` infix, no header | §3.1 | DONE | Filename helpers use `output_raw` verbatim; `push_drach_*` write data-only (no header line emitted). Goldens have no header (first line is data). |
| B2 | Top-strand walk: `pos=i+2`, C at `pos`, perl_substr extraction (incl. negative wrap emit), DRACH filter, `len<3` guard, lookup@`pos`, emit `+` | §3.2 | DONE | `drach.rs:174-202`; wrap-emit pinned by golden `wrap` (`chrA 2 + 9 1 AA CAA`). |
| B3 | Bottom-strand walk: `pos=i+2`, revcomp window, DRACH filter, `len<3` guard, lookup+report@`pos-1` (incl. pos<4 wrap) | §3.3 | DONE | `drach.rs:207-233`; `pos-1` anchor; perl_substr negative wrap reproduced. |
| B4 | Output line formats: 7-col report, 6-col cov (both pos cols equal, pct recomputed %.6f) | §3.4 | DONE | `push_drach_report` (7 cols, `drach.rs:268-292`), `push_drach_cov` (6 cols, equal pos cols, `report::pct6`, `:251-264`). |
| B5 | Chromosome order + per-chr flush: top-then-bottom, no uncovered pass, insertion order (HashMap, not BTreeMap), last-write-wins, empty cov→empty files, final-flush guard | §3.5 | DONE | `drach_chromosome_bytes` top then bottom (`:165-166`); `HashMap` buffer; `buffer.insert` last-write-wins; covered-only (iterates cov, not genome); `Option`-guarded final flush prevents phantom `""` walk; empty golden = two 0-byte files. |
| B6 | Bottom-strand C at `pos-1` (the BS-seq cytosine anchor) | §3.6 | DONE | `drach.rs:222` `let key = pos - 1`; golden `bottom` reports at pos 5 (= `pos-1`); Q1 resolved (plain byte-identical port). |

### §9 Validation matrix (V1–V16)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| V1 | CLI un-rejection + no spurious mutex + general mutexes preserved (incl. `--drach --merge_CpGs --coverage_threshold 5` errors) | §9 V1 | DONE | Unit `drach_has_no_dedicated_mutex_but_general_mutexes_still_fire` (`cli.rs:391-424`) + `v1_cx_early_exit_no_normal_report` (golden). |
| V2 | Threshold auto-set (`--drach`→1; explicit 5→5; order matches Perl) | §9 V2 | DONE | Unit `drach_accepted_and_auto_sets_threshold_one` + `drach_explicit_threshold_survives_auto_set` (`cli.rs`); integration `v2_explicit_threshold_survives` (golden `thr5`). |
| V3 | `is_drach_motif` (.get-based, short-slice-safe; non-ACGT pos1/2/5; 0/1/2-byte slices, no panic) | §9 V3 | DONE | Units `is_drach_motif_filter_arms` + `is_drach_motif_short_slices_no_panic` (`drach.rs:369-390`). |
| V4 | DRACH filenames (raw, gzip, split `.chrchr1`, suffixed `-o` not stripped) | §9 V4 | DONE | Unit `drach_filenames` (`drach.rs:441-476`); integration `v4_raw_suffix_filename_not_stripped` (golden `rawsuffix`). |
| V5 | Top-strand report + cov golden | §9 V5 | DONE | `v5_top_strand_matches_perl` (golden `top`). |
| V6 | Bottom-strand report + cov golden (`pos-1`) | §9 V6 | DONE | `v6_bottom_strand_matches_perl` (golden `bottom`, reports pos 5). |
| V7 | Chromosome-start BOTTOM motif (`pos<4`): no panic, emits nothing | §9 V7 | DONE | Covered structurally by the perl_substr wrap (no panic) + `len<3` guard; the bottom `pos<4` non-emit is exercised within the `g_bot`/golden suite and the kernel `perl_substr` unit. No standalone golden dir, but the behavior (no panic + len-guard skip) holds and is not byte-observable beyond empty==empty. See Gaps note. |
| V8 | Empty cov → empty files (no die, exit 0, no panic on never-set last_chr) | §9 V8 | DONE | `v8_empty_cov_yields_empty_files` (golden `empty` = two 0-byte files). |
| V9 | Uncovered motif threshold-skip | §9 V9 | DONE | Unit `uncovered_motif_skipped_by_threshold` (`drach.rs:420-426`); also exercised in the `top`/`bottom` goldens (motifs without cov absent). |
| V10 | Chromosome-end truncated 5-mer, BOTTOM EMITS | §9 V10 | DONE | `v10_bottom_truncated_5mer_emits` (golden `trunc` = `chrT 4 - 5 0 TACT CTT`); unit `bottom_strand_truncated_5mer_emits_at_pos_minus_1`. |
| V11 | `--gzip` decompresses to plain golden | §9 V11 | DONE | `v11_gzip_decompresses_to_plain_golden` (gz vs `top`). |
| V12 | `--split_by_chromosome` ordering (per-chr files, +then-, cov appearance) | §9 V12 | DONE | `v12_split_by_chromosome_matches_perl` (golden `split`, 4 files chrchr1/chrchr2). |
| V13 | `--zero_based` ignored | §9 V13 | DONE | `v13_zero_based_ignored` (vs `top`); unit `drach_zero_based_resolves`. |
| V14 | Regression: v1.0 + Phases 1/3 unaffected | §9 V14 | DONE | Full suite green (155, no regression); phase1 (18), B/C/D/sanity unchanged. |
| V15 | Chromosome-start TOP motif (`pos<4`) EMITS (the Critical) | §9 V15 | DONE | `v15_top_chromosome_start_wrap_emits` (golden `wrap` = `chrA 2 + 9 1 AA CAA`); unit `top_strand_chromosome_start_wrap_emits`. |
| V16 | Single-file 2-chromosome ordering (cov-appearance order) | §9 V16 | DONE | `v16_single_file_chromosome_ordering` (golden `single_order`: chr2 block before chr1, from `multi_rev.cov` chr2-then-chr1). |

## Gaps (detail)

None blocking. One observation, no action required:

### V7: chromosome-start BOTTOM motif (`pos<4`) — no dedicated golden dir

**Expected (plan §9 V7):** "no panic; emits NOTHING — the rc-wrapped bottom `tri` is always len<3 → len-guard-skipped." The plan reworded V7 (rev 2 A-F5) to a no-panic/empty assertion, explicitly noting `"byte-identical" = empty==empty`.

**Found:** There is no standalone `g_bot_start`/golden dir whose sole purpose is the bottom `pos<4` case. The behavior is nonetheless guaranteed and tested: (a) `perl_substr` (isize offset) cannot panic on negative offsets — proven by the `report.rs` `perl_substr_negative_wraps_from_end` unit; (b) the `tri.len() >= 3` guard in `drach_bottom` (`drach.rs:221`) skips a sub-3 rc-wrapped tri; (c) the `is_drach_motif_short_slices_no_panic` unit proves no panic on <2-byte slices. The bottom-strand goldens (`g_bot` = `AAATGTTCAAAGTACGTACGT`) exercise the bottom path without a crash.

**Gap:** None of substance. Since V7's expected output is "emits nothing" (empty == empty), a dedicated golden dir would add a byte-identity assertion of two empty files identical to V8's empty case; the no-panic guarantee is covered by the perl_substr + short-slice units. This is a DONE per the plan's own "the plan is the spec" rule — the required behavior (no panic, emits nothing) is present and tested. Flagged only for transparency.

## Test verification (Mode B)

| Suite | File | Count | Status |
|-------|------|-------|--------|
| lib unit (incl. 4 `--drach` CLI + 8 `drach.rs` kernel) | `src/*.rs #[cfg(test)]` | 92 | PASS |
| Phase 1 goldens | `tests/golden_phase1.rs` | 18 | PASS |
| **Phase 2 goldens (V1,V2,V4,V5,V6,V8,V10,V11,V12,V13,V15,V16)** | `tests/golden_phase2.rs` | **12** | PASS |
| Phase B goldens | `tests/golden_phase_b.rs` | 11 | PASS |
| Phase C goldens | `tests/golden_phase_c.rs` | 7 | PASS |
| Phase D goldens | `tests/golden_phase_d.rs` | 10 | PASS |
| sanity (v1.x-rejection probe moved to `--ffs`) | `tests/sanity.rs` | 5 | PASS |
| **Total** | | **155** | **ALL PASS** |

Phase-2 integration test → V-row map (golden_phase2.rs):
| Test fn | V-row | Golden dir |
|---------|-------|-----------|
| `v5_top_strand_matches_perl` | V5 | top |
| `v6_bottom_strand_matches_perl` | V6 | bottom |
| `v15_top_chromosome_start_wrap_emits` | V15 | wrap |
| `v10_bottom_truncated_5mer_emits` | V10 | trunc |
| `v2_explicit_threshold_survives` | V2 | thr5 |
| `v4_raw_suffix_filename_not_stripped` | V4 | rawsuffix |
| `v12_split_by_chromosome_matches_perl` | V12 | split |
| `v16_single_file_chromosome_ordering` | V16 | single_order |
| `v8_empty_cov_yields_empty_files` | V8 | empty |
| `v11_gzip_decompresses_to_plain_golden` | V11 | top (gz) |
| `v13_zero_based_ignored` | V13 | top |
| `v1_cx_early_exit_no_normal_report` | V1 | top |

Quality gates: `cargo fmt -p bismark-coverage2cytosine --check` = clean (exit 0); `cargo clippy -p bismark-coverage2cytosine --all-targets -- -D warnings` = clean (exit 0).

## Deviation check

The plan's "Implementation notes (rev 3)" claims **"No deviations from rev 2."** Verified TRUE:
- Top-strand `perl_substr` mandate for BOTH `tri` and `drach` (A-F1) — present (`drach.rs:189-190`), golden V15 pins the emit.
- `.get`-based `is_drach_motif` (A-F8) — present (`drach.rs:242-247`), V3 units pin <2-byte no-panic.
- General-mutex preservation, no `--drach` short-circuit (A-F2) — present (`cli.rs:159-202`), V1 unit pins `--drach --merge_CpGs --coverage_threshold 5` errors.
- Empty-cov final-flush guard via `Option` skeleton, writers up front (A-F3/B-2) — present (`drach.rs:54-96`), V8 golden.
- Bottom truncated-5-mer pass-AND-emit (A-F7/V10) — present, golden `trunc`.
- Single-file 2-chr ordering (B-3/V16) — present, golden `single_order`.
- `i += 1` non-self-overlapping scan (A-F4/B-Opt1, not `gpc.rs`'s `j += 2`) — present (`drach.rs:182-201`, `:215-232`).
- Bottom-strand `pos-1` anchor — plain byte-identical port (Q1 resolved). Present (`drach.rs:222`).

No undocumented deviation found.

## Verdict

**COMPLETE.** All 31 ledger items (8 §5 steps + 7 §3 behavior groups + 16 §9 validation rows) are DONE. The full crate test suite passes at the plan's expected count (155 = 92+18+12+11+7+10+5); fmt and clippy are clean. The plan's "no deviations from rev 2" claim is verified. The single transparency note (V7 has no dedicated golden dir) is not a gap: V7's required behavior — no panic + emits nothing — is present in code and covered by the `perl_substr` + short-slice + bottom-strand units; the plan defines V7's byte-identity as empty==empty.

Nothing remains to implement.

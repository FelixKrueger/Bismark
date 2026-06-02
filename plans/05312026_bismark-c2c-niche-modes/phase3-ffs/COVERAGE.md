# Plan Coverage Report

**Mode:** B (code vs. plan, post-implementation)
**Plan(s):** `plans/05312026_bismark-c2c-niche-modes/phase3-ffs/PLAN.md` (rev 2)
**Date:** 2026-06-01
**Verdict:** COMPLETE

## Summary

- Total items: 23 (7 §5 steps + 16 §9 validation rows + behavioral/structural checks rolled in)
- DONE: 22
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 1 (sanctioned — the standalone `ffs_fields` helper)

All 168 crate tests pass; `cargo fmt --check` clean; `cargo clippy --all-targets -D warnings` exit 0.

## Coverage ledger — §5 implementation outline (steps 1–7)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| S1 | Pin offset table in unit tests first (forward interior/`i=0,1` wrap/chr-end empty + reverse `i=3` empty-penta) | §5.1 | DONE | 5 `ffs_*` units in `report.rs` (`ffs_forward_interior`, `ffs_forward_hexa_negative_wrap`, `ffs_forward_empty_windows_at_chr_end`, `ffs_reverse_fields_and_empty_penta`, `ffs_passes_n_windows_verbatim`) — all pass |
| S2 | Compute the six fields per §3.2 offset table (forward signed `i-2` wrap; reverse revcomp guarded `i≥3`/`i≥4`; N passthrough) | §5.2 | DEVIATED (sanctioned) | Implemented as standalone `fn ffs_fields(seq,i,strand)->(Vec,Vec,Vec)` (`report.rs:176-217`) instead of extending `extract`/`Extracted` struct. Plan §4 explicitly left struct-vs-tuple to the implementer; rev-2 "Implementation notes" records this. Forward guards are the numeric `len≥i+4`/`i+5`/`i+4`; forward hexa uses signed `i as isize-2` (negative-wrap intact); reverse uses `revcomp(perl_substr(...))` guarded `i≥3`/`i≥4`. Output identical. |
| S3 | `emit_position` gains `ffs: bool`; append `\t{tetra}\t{penta}\t{hexa}` after `tri`, before `\n`; cols 1–7 byte-unchanged; computed only when `ffs` | §5.3 / §3.3 | DONE | `report.rs:233` (param next to `nome`), append block `report.rs:297-305` (calls `ffs_fields` only inside `if ffs`), `\n` at `:306`. Cols 1–7 untouched. |
| S4 | Thread `config.ffs` through `chromosome_report_bytes`; update test harness | §5.4 | DONE | `chromosome_report_bytes` passes `config.ffs` at `report.rs:357`; single call site. Harness `run_nome` threads `ffs` (passes `false`, `report.rs:780`); `run_t`/`run` are thin wrappers. |
| S5 | CLI: delete `--ffs` rejection; add `ffs` to `ResolvedConfig`+constructor; update `:97` help; narrow rejection test; add positive resolve test (V7) | §5.5 / §3.7 | DONE | No `--ffs` rejection anywhere (only a comment at `cli.rs:160`). `pub ffs: bool` in `ResolvedConfig` (`:147`) + constructor (`:259`). Help text updated (`:99-101`). Old `rejects_v1x_flags` loop removed; `ffs_resolves_and_composes` (`cli.rs:330`) added (composes with `--merge_CpGs`/`--CX`). `--drach` rejection logic untouched (Phase 2). |
| S6 | Goldens + integration tests: `tests/golden_phase3.rs` + `tests/data/phase3_ffs/` fixture dir + per-phase `generate_goldens.sh`; cover all modes incl. N-window | §5.6 | DONE | `golden_phase3.rs` (9 tests). Fixtures: 3 genome dirs (`g_main`, `g_merge`, `g_nwin`) + 3 cov files + `generate_goldens.sh` + 7 Perl golden dirs (`ffs_cpg`, `ffs_cx`, `ffs_zero`, `ffs_split`, `ffs_merge`, `plain_merge`, `ffs_nwin`). `generate_goldens.sh` `gen` lines map to V6/V8–V11/V15. |
| S7 | Regression: full suite green; non-ffs runs emit identical 7-col lines; merge unaffected | §5.7 / V14 | DONE | 168/168 pass: 97 lib + 18 phase1 + 12 phase2 + 9 phase3 + 11 B + 7 C + 10 D + 4 sanity. No regression. |

## Coverage ledger — §3 behavioral requirements

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| B1 | Columns appear in CpG-only AND `--CX`, covered AND uncovered, orthogonal to zero_based/split | §3.1 | DONE | V8 (CpG) + V9 (CX) + V13 (uncovered chrC 10-col `0 0`) + V10 (zero_based) + V11 (split) goldens all match. |
| B2 | Six offsets exactly per §3.2 (forward signed `i-2` wrap; reverse `i-3`/`i-4`; N not filtered) | §3.2 | DONE | `ffs_fields` `report.rs:176-217` matches the table; V1–V4 + V15 units + N-window golden confirm. |
| B3 | Emission appends 3 cols after `tri`, empties render nothing-between-tabs | §3.3 | DONE | `report.rs:297-306`. |
| B4 | Guards / ordering unchanged from v1.0 | §3.4 | DONE | `emit_position` guards (`report.rs:242-277`) byte-unchanged; `ffs_fields` computed inside the `if ffs` post-emit append, never gates. |
| B5 | Context summary unchanged by `--ffs` | §3.5 | DONE | Summary fed `tri`+`upstream` only; V5 confirms ffs vs no-ffs summary byte-identical. |
| B6 | `--merge_CpGs` interaction — no `merge.rs` change; tolerates 10 cols | §3.6 | DONE | `parse_report_row` (`merge.rs:47-72`) requires `f.len()≥6`, reads only `f[0..6]`. No change. V6 confirms merged cov == no-ffs. |
| B7 | CLI un-reject + composes with every flag, no new mutex; `--drach` rejection stays | §3.7 | DONE | See S5. `--drach` path untouched. |

## §9 Validation matrix → test mapping

| V# | Verify | Test fn (file) | Status |
|----|--------|----------------|--------|
| V1 | forward tetra/penta/hexa interior | `ffs_forward_interior` (`report.rs`) | PASS |
| V2 | forward hexa negative-wrap at i=1,0 | `ffs_forward_hexa_negative_wrap` (`report.rs`) | PASS |
| V3 | forward empty windows at chr-end | `ffs_forward_empty_windows_at_chr_end` (`report.rs`) | PASS |
| V4 | reverse fields + empty penta at i=3 | `ffs_reverse_fields_and_empty_penta` (`report.rs`) | PASS |
| V5 | context summary unchanged by --ffs | `v5_ffs_does_not_change_the_context_summary` (`golden_phase3.rs`) | PASS |
| V6 | --ffs --merge_CpGs merged cov unchanged | `v6_ffs_merge_cov_unchanged_by_ffs` (`golden_phase3.rs`) | PASS |
| V7 | CLI: --ffs resolves + composes | `ffs_resolves_and_composes` (`cli.rs`) | PASS |
| V8 | CpG --ffs golden (10-col) | `v8_ffs_cpg_matches_perl` (`golden_phase3.rs`) | PASS |
| V9 | --CX --ffs golden | `v9_ffs_cx_matches_perl` (`golden_phase3.rs`) | PASS |
| V10 | --ffs --zero_based golden + context cols frozen | `v10_ffs_zero_based_shifts_pos_but_not_context_columns` (`golden_phase3.rs`) | PASS |
| V11 | --ffs --split_by_chromosome golden | `v11_ffs_split_matches_perl` (`golden_phase3.rs`) | PASS |
| V12 | --ffs --gzip decompresses to plain golden | `v12_ffs_gzip_decompresses_to_plain_golden` (`golden_phase3.rs`) | PASS |
| V13 | uncovered-chromosome 10-col `0 0` lines | `v13_uncovered_chromosome_emits_10col_zero_lines` (`golden_phase3.rs`) | PASS |
| V14 | regression: v1.0 + Phase D suites green | full `cargo test` (159 non-phase3 tests) | PASS |
| V15 | N-window NOT filtered | `v15_n_window_emitted_verbatim_not_filtered` (`golden_phase3.rs`) + `ffs_passes_n_windows_verbatim` unit (`report.rs`) | PASS |
| V16 | (optional) all-three-empty trailing-tab + reverse all-empty | Covered by `ffs_forward_empty_windows_at_chr_end` (all-three-empty at `chrC i=6`) + byte-level golden lines in V8/V9 (empties render nothing-between-tabs); `parse_report_row` tolerates via V6 | PASS (optional, satisfied) |

## Test verification (all binaries)

| Binary | Tests | Result |
|--------|-------|--------|
| lib (unit, incl. 5 `ffs_*` + V7 in cli) | 97 | PASS |
| golden_phase1 | 18 | PASS |
| golden_phase2 | 12 | PASS |
| golden_phase3 (V5–V15) | 9 | PASS |
| golden_phase_b | 11 | PASS |
| golden_phase_c | 7 | PASS |
| golden_phase_d | 10 | PASS |
| sanity | 4 | PASS |
| **Total** | **168** | **PASS** |

`cargo fmt -p bismark-coverage2cytosine -- --check` → exit 0 (clean).
`cargo clippy -p bismark-coverage2cytosine --all-targets -- -D warnings` → exit 0 (clean).

## Gaps (detail)

None. No PARTIAL or MISSING items.

## Sanctioned deviation (detail)

### S2: standalone `ffs_fields` helper instead of extending `extract`/`Extracted` struct

**Expected (§4/§5):** extend `extract` to return an `Extracted` struct (or 6-tuple) carrying tetra/penta/hexa.
**Found:** a separate `fn ffs_fields(seq, i, strand) -> (Vec<u8>, Vec<u8>, Vec<u8>)` (`report.rs:176-217`); `extract` left at its shipped `(tri, upstream, strand)` shape; `emit_position` calls `ffs_fields` only inside the `if ffs` append block.
**Verdict:** DEVIATED (documented, sanctioned). PLAN §4 explicitly leaves the struct-vs-tuple choice to the implementer ("Implementer's choice"); the rev-2 "Implementation notes" section records this deviation and its rationale (less churn, preserves `extract`'s shipped semantics, directly unit-testable). Output is byte-identical. The rev-2 "no deviations beyond the sanctioned `ffs_fields` shape" claim is **confirmed** — no other structural or behavioral deviation found.

## Verdict

**COMPLETE.** Every §5 step (1–7) is present in the working tree, every §3 behavioral requirement is satisfied, and every §9 validation row (V1–V16) maps to an existing test that passes. The full 168-test suite is green and matches the plan's expected breakdown exactly (97/18/12/9/11/7/10/4); fmt and clippy are clean. `merge.rs` is correctly unchanged (the `f.len()≥6`/`f[0..6]` reader tolerates 10-col lines). The single deviation (standalone `ffs_fields` vs an `Extracted` struct) is the one the plan explicitly sanctioned and is accepted, not a gap.

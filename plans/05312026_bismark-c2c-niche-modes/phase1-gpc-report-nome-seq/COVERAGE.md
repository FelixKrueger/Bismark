# Plan Coverage Report

**Mode:** B (code vs. plan — the design PLAN is the implementation spec)
**Plan(s):** `plans/05312026_bismark-c2c-niche-modes/phase1-gpc-report-nome-seq/PLAN.md` (rev 2)
**Date:** 2026-05-31
**Verdict:** COMPLETE

## Summary

- Total ledger items: 41 (7 §5 steps + 6 §3 behavior groups + 21 §9 V-rows + 1 documented deviation + 6 cross-cutting checks)
- DONE: 40
- PARTIAL: 0
- MISSING: 0
- DEVIATED (documented, accepted): 1

All 131 crate tests pass (`cargo test -p bismark-coverage2cytosine`): 80 lib + 18 golden_phase1 + 11 phase_b + 7 phase_c + 10 phase_d + 5 sanity. `cargo` build clean.

## Coverage ledger

### §5 Implementation outline (steps 1–7)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| S1 | CLI: un-reject `--gc`/`--nome-seq`; add `NomeWithCx`/`NomeWithMerge`; resolve `gc_context = gc \|\| nome_seq`; NOMe threshold default 1; add `gc_context`/`nome` to `ResolvedConfig` | §5.1 | DONE | `cli.rs:156-161` (only `--drach`/`--ffs` rejected), `:179-184` (NOMe mutexes in Perl order), `:202-212` (resolution), `ResolvedConfig` fields `:127-130`. Unit tests `gc_alone_resolves...`, `nome_implies_gc...`, `nome_explicit_threshold_kept`, `nome_mutexes`, `nome_merge_threshold_triple_fires...`, `rejects_v1x_flags` |
| S2 | `gpc.rs` primitives + kernel: non-overlapping `GC` walk + `emit_gpc_dinucleotide` (bottom-then-top, two `len<3` + classify-both guards + NOMe CG-skip) | §5.2 | DONE | `gpc.rs:168-270` (`gpc_chromosome_bytes` + `emit_gpc_dinucleotide`); `j += 2`/`j += 1` scan `:180-197`. Unit tests `gpc_primary_anchor...`, `gpc_nome_drops_cg...`, `gpc_edge_guards...`, `gpc_gcgc_is_non_overlapping...`, `gpc_uncovered_positions_dropped...` |
| S3 | `gpc.rs` filenames: `gpc_report_path`/`gpc_cov_path` (raw-`-o` + `.chr` + `.NOMe` + suffix + gz) | §5.3 | DONE | `gpc.rs:325-354` (`gpc_base`/`gpc_report_path`/`gpc_cov_path`). Unit test `gpc_filenames_plain_gzip_split_nome_and_raw_suffix` covers all 5 shapes |
| S4 | `gpc.rs` driver `run_gpc`: per-chr streaming (single vs split, mirroring report.rs minus summary + uncovered pass); 2 ReportWriters; local `gpc_threshold = max(threshold,1)` | §5.4 | DONE | `gpc.rs:41-162` (`run_gpc`/`run_gpc_single`/`run_gpc_split`/`flush_gpc_split_chromosome`); `gpc_threshold = config.threshold.max(1)` at `:45`, config never mutated |
| S5 | `report.rs` NOMe core: thread `nome` + `.cov` companion through `emit_position`/`chromosome_report_bytes`/`run_single`/`run_split`; `.NOMe.*` filenames; gate uncovered pass on `!nome` | §5.5 | DONE | `emit_position` gains `nome`+`cov_out` `:169-258` (ACG/TCG filter `:219`, `.cov` write `:243-257`); `chromosome_report_bytes` returns `(report,cov)` `:264-296`; `run_single`/`run_split` open `.cov` only under nome `:326-333`/`:487-491`; uncovered gate `threshold==0 && !nome` `:380`/`:450`; `report_name` `nome` flag `:503-525`; `nome_cov_path` `:561-568` |
| S6 | `lib.rs`: `pub mod gpc;` + `if config.gc_context { gpc::run_gpc(...) }` after report → merge | §5.6 | DONE | `lib.rs:38` (mod), `:67-69` (call, after `run_report` `:58` and `run_merge` `:60-62`) |
| S7 | Goldens + integration: `tests/data/phase1/` + `generate_goldens.sh` + `tests/golden_phase1.rs` | §5.7 | DONE | 5 fixture genomes (`g_*/`), 6 cov files, 15 Perl golden dirs (`gold/*`), `generate_goldens.sh` present, `golden_phase1.rs` (18 tests) |

### §3 Behavior groups

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| B3.1 | CLI resolution (NOMe block in Perl order, threshold default 1, `--gc` no threshold bump, never mutate config.threshold) | §3.1 | DONE | `cli.rs:175-212`; NOMe mutexes precede merge-threshold check (`:179-187`); `gpc_threshold` local in `gpc.rs:45` |
| B3.2 | Core-report NOMe changes (ACG/TCG filter after summary; `.NOMe.CpG.cov` companion point-coord; `.NOMe.*` filenames; uncovered skip) | §3.2 | DONE | `report.rs:219` (filter after `context_reporting` at `:207`), `:243-257` (cov point-coord honouring zero_based), `report_name`/`nome_cov_path`, `:380`/`:450` skip gate |
| B3.3 | GpC walk + coordinate arithmetic (`pos=j+2`, top/bottom tri, two len<3 guards, classify both, bottom-before-top, NOMe CG-skip, no zero_based) | §3.3 | DONE | `gpc.rs:212-270` — `pos=(j+2)`, `tri_top=substr(pos-1,3)`, `tri_bottom=revcomp(substr(pos-4,3))`, guards `:233-242`, bottom `:250-263` then top `:265-269`, no zero_based |
| B3.4 | GpC filenames from raw `-o` (+ `.chr`, `.NOMe`, suffix, gz) | §3.4 | DONE | `gpc.rs:325-354`; uses `config.output_raw` verbatim |
| B3.5 | Split writer lifecycle (fresh truncating writer per chr, no caching, no summary) | §3.5 | DONE | `flush_gpc_split_chromosome` `:147-162` opens fresh writers per chr; no per-name cache; no summary written by GpC |
| B3.6 | Edge cases (GC-at-start, GC-at-end, GCGC overlap, empty cov, NOMe div-by-zero, cov-chr-not-in-genome) | §3.6 | DONE | start/end drops via `len<3` guard + `perl_substr` neg-wrap; `GCGC` `j+=2`; empty cov errors in core before GpC; threshold≥1 prevents div-by-zero; `genome.get`→None yields no bytes (`gpc.rs:177-179`) |

### §9 Validation matrix (V1–V21)

| # | Verify | Test | Status |
|---|--------|------|--------|
| V1 | `--nome-seq` / `--gc` resolution | `cli::gc_alone_resolves_supported_threshold_zero` + `cli::nome_implies_gc_and_threshold_one` | DONE |
| V2 | NOMe mutexes + explicit threshold | `cli::nome_mutexes` + `cli::nome_explicit_threshold_kept` | DONE |
| V3 | `--drach`/`--ffs` still rejected | `cli::rejects_v1x_flags` (+ `sanity::unsupported_v1x_flag_is_rejected`) | DONE |
| V4 | GpC report golden (`--gc`) | `golden_phase1::v4_gc_primary_matches_perl` + `gpc::gpc_primary_anchor_matches_perl` | DONE |
| V5 | `--gc` emits core report too, unchanged | `golden_phase1::v5_gc_core_report_unaffected` | DONE |
| V6 | GpC chromosome-edge guards | `golden_phase1::v6_gc_edge_guards_match_perl` + `gpc::gpc_edge_guards_drop_first_and_last_gc` | DONE |
| V7 | GpC `GCGC` non-overlapping | `golden_phase1::v7_gc_gcgc_non_overlapping_matches_perl` + `gpc::gpc_gcgc_is_non_overlapping_and_consecutive` | DONE |
| V8 | `--gc --gzip` | `golden_phase1::v8_gc_gzip_decompresses_to_plain_golden` | DONE |
| V9 | `--gc --split_by_chromosome` (multi-chr) | `golden_phase1::v9_gc_split_multi_chromosome_matches_perl` | DONE |
| V10 | `--gc --zero_based` == `--gc` (GpC frozen) | `golden_phase1::v10_v20_gc_zero_based_core_shifts_gpc_frozen` (GpC-equal assert) | DONE (folded with V20) |
| V11 | NOMe core report golden (ACG/TCG) | `golden_phase1::v11_nome_acg_tcg_upstream_filter_matches_perl` + `report::nome_filters_acg_tcg_upstream_and_writes_cov` | DONE |
| V12 | NOMe drops non-ACG/TCG CpGs | `golden_phase1::v12_nome_primary_matches_perl` + `gpc::gpc_nome_drops_cg_context_keeps_chh` | DONE |
| V13 | NOMe summary filename (no `.NOMe`) + threshold-1-gated content | `golden_phase1::v12_nome_primary_matches_perl` / `v14_v19_nome_split_matches_perl` (golden dirs carry plain `cytosine_context_summary.txt`) | DONE (folded; covered structurally by the byte-identical golden summary) |
| V14 | NOMe skips uncovered chromosomes | `golden_phase1::v14_v19_nome_split_matches_perl` | DONE (folded with V19) |
| V15 | raw-`-o` GpC filename | `golden_phase1::v15_gc_raw_suffix_filename_matches_perl` + `gpc::gpc_filenames_...` raw-suffix case | DONE |
| V16 | NOMe `.cov` no division-by-zero | `report::nome_drops_non_acg_tcg_cpg` + threshold≥1 invariant (no panic across all NOMe goldens) | DONE |
| V17 | regression: plain run writes no `.cov`/`.GpC`/`.NOMe` | `golden_phase1::v17_plain_run_writes_no_cov_or_gpc_or_nome_files` (golden dir = exactly 2 files) | DONE |
| V18 | non-contiguous chr re-appearance (single + split) | `golden_phase1::v18_gc_non_contiguous_chr_reappearance_single` + `..._split` | DONE |
| V19 | `--nome-seq --split_by_chromosome` | `golden_phase1::v14_v19_nome_split_matches_perl` | DONE (folded with V14) |
| V20 | `--gc --zero_based` discriminator (core shifts, GpC frozen) | `golden_phase1::v10_v20_gc_zero_based_core_shifts_gpc_frozen` (core `assert_ne` + GpC `assert_eq`) | DONE (folded with V10) |
| V21 | `--nome-seq --zero_based` cov point-coord | `golden_phase1::v21_nome_zero_based_point_cov_coord` | DONE |

### Cross-cutting / extra coverage

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| X1 | `pct6` promoted to `report.rs` (`pub(crate)`), reused by `merge.rs` (`round6` + cov writes) | rev-2 note / §1 | DONE | `report.rs:92-94` (`pct6`); `merge.rs:35-37` (`round6` calls `report::pct6`), `:167/:177/:196` cov writes use `report::pct6`. Phase-D goldens still byte-identical (10/10 pass) |
| X2 | `error.rs` `NomeWithCx`/`NomeWithMerge` variants | §4 | DONE | Referenced + matched in `cli.rs:180/:183` and `cli` unit tests; build compiles + matches resolve |
| X3 | `--gc --coverage_threshold N` runs core at N (B-M2) | §3.1 step 4 | DONE | `golden_phase1::b_m2_gc_threshold_runs_core_at_user_threshold` (`gc_thr3` golden) |
| X4 | `-o foo --nome-seq` raw-suffix `.cov` divergence | rev-2 deviation | DONE | `golden_phase1::nome_raw_suffix_cov_uses_raw_base` + `report::nome_cov_path_uses_raw_base`; golden `nome_rawsuffix` confirms report stem-strips, `.cov` keeps raw |
| X5 | NOMe `--gzip` | §3.2 | DONE | `golden_phase1::nome_gzip_decompresses_to_plain_golden` |
| X6 | `--gc` composes with `--CX` (no new mutex) | §3.1 mutex summary | DONE | `cli::gc_composes_with_cx` |

### Documented deviation

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| D1 | NOMe core `.cov` filename derives from the **raw `-o`** (`output_raw`), NOT the stripped stem (§3.2.3 prose said `{stem}.NOMe.CpG.cov`) | "Implementation notes (rev 2)" | DEVIATED (documented, accepted) | `report::nome_cov_path` uses `config.output_raw` (`:561-568`). For plain `-o sample` the bases coincide (prose correct there); for `-o foo.CpG_report.txt` the report stem-strips while the `.cov` keeps the raw base. Verified against live Perl v0.25.1 (`handle_filehandles` never strips `$cytosine_coverage_file`). Reviewer-confirmed MORE faithful to byte-identity. Pinned by `nome_raw_suffix_cov_uses_raw_base` golden + `nome_cov_path_uses_raw_base` unit test. NOT a gap. |

## Test verification

| Suite | File | Count | Status |
|-------|------|-------|--------|
| lib unit tests (cli + report + gpc + merge + genome + cov + summary) | `src/*.rs #[cfg(test)]` | 80 | PASS |
| Phase-1 goldens (V4–V21 + raw-suffix divergence) | `tests/golden_phase1.rs` | 18 | PASS |
| Phase-B goldens (regression) | `tests/golden_phase_b.rs` | 11 | PASS |
| Phase-C goldens (regression) | `tests/golden_phase_c.rs` | 7 | PASS |
| Phase-D goldens (regression — confirms `pct6` promotion behaviour-preserving) | `tests/golden_phase_d.rs` | 10 | PASS |
| CLI/version sanity | `tests/sanity.rs` | 5 | PASS |
| **Total** | | **131** | **ALL PASS** |

Key Phase-1 golden test → V-row map (all green):
- `v4_gc_primary_matches_perl` → V4
- `v5_gc_core_report_unaffected` → V5
- `v6_gc_edge_guards_match_perl` → V6
- `v7_gc_gcgc_non_overlapping_matches_perl` → V7
- `v8_gc_gzip_decompresses_to_plain_golden` → V8
- `v9_gc_split_multi_chromosome_matches_perl` → V9
- `v10_v20_gc_zero_based_core_shifts_gpc_frozen` → V10 + V20
- `v11_nome_acg_tcg_upstream_filter_matches_perl` → V11
- `v12_nome_primary_matches_perl` → V12 + V13 (summary)
- `v14_v19_nome_split_matches_perl` → V14 + V19
- `v15_gc_raw_suffix_filename_matches_perl` → V15
- `v17_plain_run_writes_no_cov_or_gpc_or_nome_files` → V17
- `v18_gc_non_contiguous_chr_reappearance_single` / `..._split` → V18
- `v21_nome_zero_based_point_cov_coord` → V21
- `b_m2_gc_threshold_runs_core_at_user_threshold` → B-M2 (§3.1 step 4)
- `nome_gzip_decompresses_to_plain_golden`, `nome_raw_suffix_cov_uses_raw_base` → extra coverage

## Verdict

**COMPLETE.** Every §5 implementation step (1–7), every §3 behavioral requirement (3.1–3.6), and every §9 validation row (V1–V21) is implemented and verified by a corresponding test that exists and passes. The §8 assumptions and §10 resolutions are all reflected in code (raw-`-o` GpC filenames, GpC no-`--zero_based`, `gpc_threshold = max(threshold,1)`, NOMe summary non-`.NOMe`/threshold-1-gated, NOMe `.cov` companion, covered-only GpC). The single documented deviation (D1 — NOMe core `.cov` derives from the raw `-o`) is reviewer-confirmed correct, live-Perl-verified, and pinned by a golden + a unit test, so it is marked DEVIATED (documented, accepted), not a gap.

No items are PARTIAL or MISSING. Nothing remains to be addressed.

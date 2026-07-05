# Plan Coverage Report

**Mode:** B (code vs. plan — the plan IS the spec; single plan-writer plan, no separate IMPL)
**Plan:** `plans/06242026_umi-barcode-tags/PLAN.md`
**Codebase:** worktree `/Users/fkrueger/Github/Bismark-umi`, crate `rust/bismark-aligner`, branch `rust/umi-barcode` (uncommitted)
**Diff base:** `origin/rust/iron-chancellor`
**Date:** 2026-06-24
**Verdict:** COMPLETE — all 10 implementation steps DONE; 2 documented deviations (both valid); §6–8 integration deferred to the oxy gate (DEVIATED, documented).

## Summary

- Total ledger items: 23 (10 outline steps + 8 edge-case rows + 1 out-of-scope boundary + 9 validation items, deduped/grouped)
- DONE: 19
- PARTIAL: 0
- MISSING: 0
- DEVIATED (documented, accepted): 4 — Step 9 (notice site), Validation §6, §7, §8 (end-to-end fixtures → oxy gate)

## Coverage ledger

### Implementation outline (Steps 1–10)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | `--add_barcode` / `--add_umi` clap fields | Step 1 / `cli.rs` | DONE | `cli.rs:260-269`, `#[arg(long = "add_barcode")]` / `"add_umi"`, both `pub … : bool`, documented. |
| 2 | `RunConfig` fields + `Cli→RunConfig` mapping + `barcode_umi_tags()` | Step 2 / `config.rs` | DONE | Struct fields `config.rs:220-224`; mapping `add_barcode: cli.add_barcode` / `add_umi: cli.add_umi` at `config.rs:438-439` (in the `Ok(RunConfig { … })` block, next to `ambig_bam`); accessor `RunConfig::barcode_umi_tags()` at `config.rs:962-969`. |
| 3 | `Data` import + `BarcodeUmiTags` + `parse_barcode_umi` + `append_barcode_umi_tags` | Step 3 / `output.rs` | DONE | `use …record_buf::data::Data;` at `output.rs:22`; `BarcodeUmiTags` (pub, `Copy/Default`) + `enabled()` at `output.rs:39-56`; `parse_barcode_umi` (pub) at `output.rs:64-67`; `append_barcode_umi_tags` (private) at `output.rs:75-87`. Inserts `Tag::from(*b"CB")` / `*b"UR"` only when field non-empty, after early `enabled()` return. |
| 4 | SE builder `opts` param + helper after XG | Step 4 / `output.rs` | DONE | `opts: BarcodeUmiTags` added to `single_end_sam_output` (`output.rs:402`); `append_barcode_umi_tags(rec.data_mut(), id, opts)` at `output.rs:494`, immediately after the `XG` insert, before `from_noodles_record`. |
| 5 | PE builder `opts` forwarded to both `build_pe_mate` + helper (one point → both mates) | Step 5 / `output.rs` | DONE | `opts` added to `paired_end_sam_output` (`output.rs:520`); forwarded into both `build_pe_mate` calls (`output.rs:610` rec1, `output.rs:631` rec2); `opts` added to `build_pe_mate` (`output.rs:660`); single `append_barcode_umi_tags(...)` after XG at `output.rs:724` → tags **both** mates via the shared inner builder. |
| 6 | `Counters` fields + `Counters::merge` extension | Step 6 / `merge.rs` | DONE | `pub add_barcode_missing: u64` / `pub add_umi_missing: u64` at `merge.rs:135-138`; both summed in `Counters::merge` at `merge.rs:171-172`. |
| 7 | SE call-site counting + tag-passing at `UniqueBest` | Step 7 / `lib.rs` | DONE | `route_se_decision` `Decision::UniqueBest` arm: `barcode_umi = config.barcode_umi_tags()`, gated `parse_barcode_umi(identifier)` counter bumps (`lib.rs:1022-1031`), `barcode_umi` passed to `single_end_sam_output` (`lib.rs:1041`). Counted once per read (inside the arm). |
| 8 | PE call-site counting + tag-passing at `UniqueBest` (once per pair) | Step 8 / `lib.rs` | DONE | `route_pe_decision` `DecisionPaired::UniqueBest` arm: same gated counter on the single `identifier` (`lib.rs:3072-3081`) — counted once per **pair**, not per mate — and `barcode_umi` passed to `paired_end_sam_output` (`lib.rs:3095`). |
| 9 | Run-end never-silent notice | Step 9 / `lib.rs` | **DEVIATED (documented, valid)** | Implemented as `push_barcode_umi_notice(&mut String, &Counters)` (`lib.rs:4090-4109`) woven into `counters_summary` (`lib.rs:4154`) and `counters_summary_pe` (`lib.rs:4128`) instead of at each report site. Gated on `count > 0` (count only ever bumped when flag set → no config needed). Verified funnel: **14** production callers (6 SE + 6 PE in `lib.rs`, + the 2 `--multicore` merge sites `parallel.rs:756`/`921` on the merged `total`) all route through these two formatters → notice fires exactly once per run on the aggregate, incl. multicore. Deviation explicitly recorded in PLAN "Implementation Notes → Deviations #1". Cleaner than the literal plan and covers all paths. |
| 10 | 9 existing builder test call sites updated to `BarcodeUmiTags::default()` | Step 10 / `output.rs` tests | DONE | All 9 updated (SE ×7 + PE ×2), verified in the diff: SE at `output.rs` test sites ~1059/1083/1272/1320/1377/1423/1495; PE at ~1680/1841. No-flag path assertions unchanged (the no-flag regression). |

### Behavior — edge-case table (Behavior §60–71)

| # | QNAME row | Source | Status | Notes |
|---|-----------|--------|--------|-------|
| 11 | `AACGTGAT_TTGCAA_1N3T_VL00347:…` (real 4-field) → CB=field0, UR=field1, alt+name ignored | Behavior | DONE | `se_both_flags_write_cb_and_ur_from_real_name` (asserts `CB=AACGTGAT`, `UR=TTGCAA`) + `parse_barcode_umi_splits_max_3_fields`. |
| 12 | `BC_UMI__name` (empty `_alt`) → CB=BC, UR=UMI | Behavior | DONE | `parse_barcode_umi_splits_max_3_fields` (`("BC","UMI")`). |
| 13 | `BC_UMI_rest_a_b` (remainder ignored) → CB=BC, UR=UMI | Behavior | DONE | `parse_barcode_umi_splits_max_3_fields`. |
| 14 | `nounderscore` → CB=whole-name, no UR (+notice) | Behavior | DONE | `se_empty_fields_skip_their_tag` (CB present, UR absent) + parse test (`("nounderscore","")`); missing-UR count path covered by `barcode_umi_notice_emitted_when_fields_missing`. |
| 15 | `_UMI_rest` (leading `_`) → no CB (+notice), UR=UMI | Behavior | DONE | `se_empty_fields_skip_their_tag` (CB absent, UR=UMI) + parse test (`("","UMI")`). |
| 16 | `BC_` (trailing `_`) → CB=BC, no UR (+notice) | Behavior | DONE | `parse_barcode_umi_splits_max_3_fields` (`("BC","")`). |
| 17 | `BC_UMI` (no remainder) → CB=BC, UR=UMI | Behavior | DONE | `parse_barcode_umi_splits_max_3_fields`. |
| 18 | `BC__rest` (empty middle field) → CB=BC, no UR (+notice) | Behavior | DONE | `se_empty_fields_skip_their_tag` (CB=BC, UR absent) — the easy-to-get-wrong case, explicitly tested. |

### Out-of-scope boundary (Behavior §73–76)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 19 | No tags on `--ambig_bam` / unmapped / ambiguous | Behavior | DONE (structural) | Verified there are exactly 2 production callers of the builders (`lib.rs:1032` SE, `lib.rs:3082` PE), both inside `UniqueBest` arms. `--ambig_bam` uses `write_raw_sam_line_to_bam` (`lib.rs:1052`) / `write_raw_pe_ambig_lines` (`lib.rs:3104`); unmapped/ambiguous go to FastQ aux. None touch `append_barcode_umi_tags`. The boundary is guaranteed by construction. (Validation §7 below is the end-to-end fixture form of this.) |

### Validation (§1–9)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 20 | §1 Unit parse/insert; §2 flag matrix incl. no-flag regression; §3 malformed-table skips + counters; §4 PE equal & non-empty CB/UR; §5 notice once-per-run | Validation §1–5 | DONE | Covered by the 6 new unit tests: `parse_barcode_umi_splits_max_3_fields`, `se_both_flags_write_cb_and_ur_from_real_name`, `se_flag_matrix_barcode_only_umi_only_neither` (incl. neither → no CB/UR), `se_empty_fields_skip_their_tag`, `pe_both_mates_carry_equal_nonempty_cb_ur` (asserts equal **and** non-empty on both mates), `barcode_umi_notice_emitted_when_fields_missing` (none→no WARNING; umi-only→one line w/ count; both→both lines; SE + PE summary paths). All 6 PASS. |
| 21 | §6 Integration fixture (PE realistic SeekSoul names, single-core, + `--pbat`) | Validation §6 | **DEVIATED (documented)** | Not a crate test — needs prepared genome + Bowtie 2 + binary run = oxy/real-data gate's job (consistent with prior aligner phases). Documented in PLAN "Implementation Notes → Deviations #2". **Recommend the oxy smoke before merge.** |
| 22 | §7 Integration `--ambig_bam` clean (zero CB/UR in `.ambig.bam`) | Validation §7 | **DEVIATED (documented)** | End-to-end form deferred to oxy; the boundary itself is proven structurally (item 19). |
| 23 | §8 Integration `--multicore` (tags on every merged record; count matches single-core) | Validation §8 | **DEVIATED (documented)** | End-to-end deferred to oxy; structurally covered (RunConfig clone carries bools; `Counters::merge` unit-summed; tags are a pure QNAME function; merge sites verified at `parallel.rs:756`/`921`). |
| — | §9 Regression: full suite green; fmt; clippy | Validation §9 | DONE | See "Gate verification" below. |

## Gate verification (run in this audit, 2026-06-24)

| Gate | Command | Result |
|------|---------|--------|
| Full aligner suite | `cargo test -p bismark-aligner -- --test-threads=2` | **PASS** — lib 426/426; integration 100/100 (incl. 6 `worker_invariance_*`); methylseq 3/3; rammap 0; doc 0. 0 failed. |
| New unit tests | (subset of above) | **PASS** — all 6 named tests green. |
| fmt | `cargo fmt -p bismark-aligner -- --check` | **CLEAN** (exit 0). |
| clippy | `cargo clippy -p bismark-aligner --all-targets -- -D warnings` | **CLEAN** (no warnings/errors). |

## Test verification

| Test name | File | Status |
|-----------|------|--------|
| parse_barcode_umi_splits_max_3_fields | output.rs (mod tests) | PASS |
| se_both_flags_write_cb_and_ur_from_real_name | output.rs | PASS |
| se_flag_matrix_barcode_only_umi_only_neither | output.rs | PASS |
| se_empty_fields_skip_their_tag | output.rs | PASS |
| pe_both_mates_carry_equal_nonempty_cb_ur | output.rs | PASS |
| barcode_umi_notice_emitted_when_fields_missing | lib.rs (mod tests) | PASS |

## Deviations (all documented in PLAN Implementation Notes — accepted)

1. **Step 9 notice site** — woven into `counters_summary`/`counters_summary_pe` (single formatter point) instead of ~10–14 individual report sites. Verified all 14 driver paths (incl. `--multicore` merge) funnel through these two functions, so the notice fires exactly once per run on the merged total. Valid and cleaner.
2. **Validation §6–8 end-to-end fixtures** — deferred to the oxy real-data gate (needs genome + Bowtie 2 + binary), consistent with prior aligner phases. The deterministic parse/insert/notice/merge logic is unit-covered; the boundary (§7) is proven structurally.

## Non-blocking observations (not plan-coverage gaps)

- **Two stray untracked test-output files** in the crate root: `rust/bismark-aligner/reads_bismark_bt2.bam` and `reads_bismark_bt2_SE_report.txt` (generated by a test run, 24 Jun). Same pattern seen in Phase 9b/10; should be removed before the commit. Not part of the plan and not a coverage gap.

## Verdict

**COMPLETE.** Every implementation outline step (1–10) is implemented as specified; every edge-case Behavior row and the out-of-scope boundary are covered (unit tests + structural proof); fmt/clippy/full-suite gates are green (426 lib + 100 integration, 0 failed). The two deviations are documented in the plan and are improvements/standard-practice deferrals, not gaps.

**Recommended before merge:** the PLAN-flagged **oxy real-data smoke** (`bismark_rs --add_barcode --add_umi` PE, directional + `--pbat`, then `samtools view` to confirm `CB:Z:`/`UR:Z:` on aligned records and absence on `--ambig_bam`), which is the end-to-end form of Validation §6–8 that was deferred from crate tests. Also remove the two stray test-output files before committing.

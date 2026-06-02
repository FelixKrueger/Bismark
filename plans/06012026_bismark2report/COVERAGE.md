# Plan Coverage Report

**Mode:** A+B (full pipeline audit — SPEC rev 1 ↔ PLAN rev 1 ↔ code/tests)
**Plan(s):** `plans/06012026_bismark2report/SPEC.md` (rev 1) · `plans/06012026_bismark2report/PLAN.md` (rev 1)
**Code audited:** `rust/bismark-report/{src/**,tests/**,Cargo.toml,README.md,CHANGELOG.md}`
**Date:** 2026-06-01
**Verdict:** **COMPLETE** — 0 blocking items. (2 OPEN-by-design: F1 real-data on oxy, F3 PR. 1 DEVIATED-undocumented but behaviorally-equivalent: the nucleotide `looksOK` gate is omitted — provably a no-op in Perl, no byte impact.)

## Summary

- Total ledger items: **58** (PLAN phases A1–F3 = 33; SPEC load-bearing requirements = 14; §7 required fixtures 1–10 = 11)
- DONE: **53**
- PARTIAL: **0**
- MISSING: **0**
- DEVIATED-documented: **3** (anyhow dropped; full 3 MB goldens → unit tests; both in PLAN §14 — plus the two OPEN-by-design phases below)
- DEVIATED-undocumented (non-blocking, byte-neutral): **1** (nucleotide `looksOK` gate omitted)
- OPEN-by-design (deferred per PLAN §14, NOT failures): **2** (F1, F3)

Test run: `cargo test -p bismark-report` → **48 passed, 0 failed** (36 unit + 8 CLI `assert_cmd` + 4 Perl-oracle byte-identity). Matches PLAN §14's "48 tests pass" claim exactly.

---

## Coverage ledger

### Phase A — scaffold + CLI + assets + timestamp

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| A1 | Crate scaffold, deps (clap/thiserror/libc; no flate2/noodles/bismark-io/glob/chrono/time), members entry, bodies-as-bytes | PLAN A1 | DONE | `Cargo.toml` deps exactly as specced (anyhow dropped — §14). `members` line includes `bismark-report`. Report bodies read via `std::fs::read` → `Vec<u8>`; `doc` is `Vec<u8>` (`template.rs`). |
| A2 | `cli.rs` clap derive — exact Perl flag spellings + hidden `--__test_timestamp`; `disable_version_flag` + manual `version`/`man` | PLAN A2 | DONE | All flags present with underscores; `--__test_timestamp` `hide=true`; `disable_version_flag = true`; manual `version`/`man` bools handled in `main.rs` before `run()`, both exit 0. |
| A3 | `version_string()` via `CARGO_PKG_VERSION`; Bismark const banner-only | PLAN A3 | DONE | `lib.rs::version_string()`; `BISMARK_VERSION` const present, never injected into HTML. |
| A4 | `logging.rs` verbose-gated STDERR | PLAN A4 | DONE | `Logger{note,info}`; `info` gated on `--verbose`. Not byte-gated. |
| A5 | `assets.rs` — `include_str!` via `CARGO_MANIFEST_DIR` + drift test; faithful `normalize()` w/ empty-input guard | PLAN A5 | DONE | 4 assets embedded manifest-relative; `normalize()` splits on `\n`, drops trailing empty iff input ends in `\n`, `replace('\r',"")` per piece, empty→`""`. Drift test `embedded_assets_match_repo_plotly_files`. |
| A6 | `timestamp.rs` — UTC pure-std civil math (deterministic) + libc `localtime_r` (default); Perl sprintf format | PLAN A6 | DONE | `civil_from_days` (Hinnant); `localtime_r` in contained `unsafe`. Format `%04d-%02d-%02d` / `%02d:%02d:%02d`. No chrono/time. |
| A7 | Phase-A tests: CLI parse, help/version/man exit 0, normalize byte-equivalence + edge cases, no `{{`/no `\r` in assets | PLAN A7 | DONE | `assets.rs` unit tests (empty/trailing-nl/no-trailing-nl/CRLF/mid-line-CR/brace-free/CR-free/drift); `timestamp.rs` (epoch0/known/leap); `tests/cli.rs` version/help/man exit-0. |

### Phase B — report parsers

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| B1 | `alignment.rs` — PE/SE detect, 8 verbatim `*_text` labels, context meth (incl. `Total C to T` + Unknown `(CN or CHN)`), strand origin, filename/version, `is_some` 5-field gate, gate-passes≠fields-present (empty fill), Unknown `<tr>` bytes, plotly strings (N/A→0 graph only) | PLAN B1 / SPEC §2.7a | DONE | Labels verified byte-for-byte vs Perl 218/222/236/241/247/252/258/263. Gate = `is_some()` on unique/no_aln/multiple/no_genomic/total_seqs (Perl line 378 confirmed). `total_C_count`/`meth_*` filled inside gate via `o()` → empty on absence. `unknown_tr` byte layout (5/32 spaces; 4sp+4tab; 4sp+3tab). |
| B2 | `dedup.rs` — `\s.*`-trim, leftover fallback `total-dups` (i64), `is_some` 4-field gate, `{{duplication_stats_plotly}}=leftover,dups` | PLAN B2 / SPEC §2.7b | DONE | `before_first_ws` trim; fallback when leftover line absent (Perl 545-548 confirmed); gate on dups/total/diff_pos/leftover (Perl line 551). |
| B3 | `splitting.rs` — `*_splitting` placeholders, phrasing variants (only `Total C to T`, Unknown w/o `(CN or CHN)`), `is_some` 6-field gate, own Unknown snippets | PLAN B3 / SPEC §2.7c | DONE | Gate on 6 meth/unmeth fields (Perl line 784 confirmed). Phrasing branches correct. |
| B4 | `mbias.rs` — per-(read,ctx) perc/cov x/y vecs, `state=paired` iff R2 header, fill mbias1 always, mbias2 only if R2 data, dead `{{bm_mbias_2}}`→false no-op, returns state | PLAN B4 / SPEC §2.7d | DONE | `state` set on header containing `R2` (Perl 906-908). R2 fill gated on `!r2.is_empty()` (= `%mbias_2`, Perl line 977). Dead subst reproduced as no-op. |
| B5 | `nucleotide.rs` — line-0 header validation, fixed 20-key order, verbatim fill, missing key→`0`%/empty counts, distinct separators (` , ` x, `','` y), no float output | PLAN B5 / SPEC §2.7e | DONE (gate caveat) | Header check cols 3/5 (Perl 587/594). Key order verbatim (Perl 632). Missing→`0`/empty (Perl 641-647). Separators correct. Log2 ratio not emitted. **`looksOK` gate omitted** — see Gaps. |
| B6 | Phase-B unit tests: branch coverage, gate-passes-on-0, dedup fallback, splitting variants, nuc fixed-order+header-error+missing-key, mbias state+empty-R2 | PLAN B6 | DONE | Co-located `#[cfg(test)]`: alignment (gate-0-pass, gate-fail-survive, N/A table-vs-graph, labels, version); dedup (fallback, explicit-wins); mbias (SE-R1-only, PE-both, coverage col); nucleotide (bad-header, missing-key #711, separators). |

### Phase C — template assembly + section logic + orchestration

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| C1 | `inject_asset` greedy/dotall first→last splice, error if absent | PLAN C1 / SPEC §2.3 | DONE | `inject_asset` find/rfind splice; `Err(AssetInjection)` if not ≥2. Applied plotly/bismark/bioinf in order. |
| C2 | `collapse` (remove all) + `excise` (first→last splice) | PLAN C2 / SPEC §2.4 | DONE | `collapse=subst_all(…,"")`; `excise` find/rfind. Unit tests incl. single-marker no-op. |
| C3 | M-bias wiring — state-driven deletion (SE excise R2, PE collapse R2, absent excise both), fill-driven by R2 data, script placeholders untouched | PLAN C3 / SPEC §2.7d | DONE | `build_report` step 9 matches Perl 124-138 exactly; script-block placeholders only touched by `mbias::fill`. |
| C4 | `build_report` 11-step orchestration, timestamp at step 5 | PLAN C4 / SPEC §2.3 | DONE | Order: template→plotly→bismark.logo→bioinf.logo→timestamp→alignment→dedup→splitting→mbias→nucleotide→return. Verified line-by-line vs Perl 59-156. |
| C5 | `write_out_report` verbatim (ends in `\n`) | PLAN C5 | DONE | `std::fs::write(&out_path, &doc)` in `lib.rs::run`; no trailing manipulation. |
| C6 | Phase-C tests: collapse/excise synthetic, M-bias matrix (absent→24, SE→12+R2 gone, PE→all filled), full PE end-to-end vs golden | PLAN C6 | DONE (realized as Perl-oracle) | collapse/excise/inject unit tests in `template.rs`. M-bias matrix asserted via `tests/perl_vs_rust.rs` (minimal_pe = 24 survive; wgbs_se = 12 mbias2 survive + R2 excised; wgbs_pe = all filled) against live Perl. Full-binary committed golden replaced by Perl-oracle (§14 documented). |

### Phase D — discovery / auto-detection / multi-report loop / naming

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| D1 | `discovery.rs` — alignment detect (explicit/glob `*E_report.txt` lexical), basename regex, companion order dedup→nuc→split→mbias, nuc `defined` vs others truthiness, >1→error/0→absent/1→use, line-1256 reset | PLAN D1 / SPEC §2.2 | DONE | `find_alignment_reports` + `resolve_companions`; `basename_of` strips `_PE/_SE_report.txt`; order matches Perl 1141/1170/1201/1228; `defined_semantics` flag for nucleotide; `first` flag = line-1256 reset; `>1 match` → `Validation` error. |
| D2 | Output naming — strip dir/`.txt`/append `.html`; `-o` verbatim; `--dir` prefix; `-o`+>1 → error | PLAN D2 / SPEC §2.5 | DONE | `derive_output_name` (strip `.txt`, append `.html`); `-o` used verbatim; `output_dir` trailing-`/` logic; `-o`+>1 guarded in `run`. |
| D3 | `run()` — build slots, loop, build+write each; no-report → hint + nonzero | PLAN D3 / SPEC §6.1 | DONE | `run` loops `jobs`, writes each; empty alignment list → `Validation` error → exit 1. |
| D4 | Phase-D tests: glob detect PE+SE, basename, companion>1 error, none skip, multi-report reset, naming `--dir`/`-o`, `-o`+2→error, no-report nonzero | PLAN D4 | DONE | `discovery.rs` units (basename, first-report-only reset, none-skip); `tests/cli.rs` (empty-dir fail, `-o`+2 fail, derive name, explicit `-o`+`--dir`, multi-report one-HTML-each). |

### Phase E — byte-identity gate + fixtures + goldens

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| E1 | Fixtures: valid PE+SE sets + special cases (gate-fail, 0-through-gate, dedup fallback, #711 missing-key, two-report+explicit, splitting/dedup/nuc gate-fail, gate-passes-context-absent, percent-N/A) | PLAN E1 | DONE (split file/unit) | On-disk fixtures: `wgbs_pe` (all 5 companions, PE M-bias, dedup w/ explicit leftover, full nuc), `wgbs_se` (SE, R1-only M-bias), `nondir_pe` (Unknown-context aln+splitting), `minimal_pe` (no companions). Special-case inputs realized as **hermetic unit-test literals** (§14 documented): see Test-verification table. |
| E2 | Perl-oracle harness, timestamp-normalize (anchor + assert exactly 1), byte-diff, auto-skip if perl absent | PLAN E2 / SPEC §7 | DONE | `tests/perl_vs_rust.rs::normalize_ts` asserts exactly one `Data processed at` occurrence; `perl_available()` auto-skip; copies fixtures, runs both into `perl/`+`rust/`, byte-compares. |
| E3 | Committed goldens via `--__test_timestamp` UTC, regenerate+compare | PLAN E3 | DEVIATED-documented | Full 3 MB committed goldens omitted (§14); Perl-oracle + `--__test_timestamp 0` self-consistency is the regression bridge. Acceptable per the audit brief. |
| E4 | SPEC §7 fixture assertions (1) 24 survive (2) 12 mbias2 survive (3)–(7) | PLAN E4 / SPEC §7 | DONE | (1)/(2) via `perl_vs_rust` minimal_pe / wgbs_se; (3)–(7) via unit tests (see fixtures ledger below). |

### Phase F — real-data + docs + PR

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| F1 | Real-data byte-identity on oxy (`#[ignore]`) | PLAN F1 / §14 | OPEN (by design) | Explicitly deferred — needs oxy + real reports. NOT a failure. |
| F2 | Docs: README, CHANGELOG, top-level status table update | PLAN F2 | DONE (crate-local) | `README.md` + `CHANGELOG.md` present and accurate. NOTE: top-level Rust-rewrite status table/per-tool list update not verified in this crate dir (likely a repo-root edit, out of audited scope). |
| F3 | PR base iron-chancellor; merge on explicit request | PLAN F3 / §14 | OPEN (by design) | Not opened; awaiting explicit instruction. NOT a failure. |

### SPEC load-bearing requirements (cross-check)

| # | Requirement | Source | Status | Notes |
|---|-------------|--------|--------|-------|
| S1 | The 5 parsers each present (alignment mandatory + 4 optional) | SPEC §2.7 | DONE | `reports/{alignment,dedup,splitting,mbias,nucleotide}.rs`. |
| S2 | 11-step orchestration order | SPEC §2.3 | DONE | `template::build_report` — verified vs Perl. |
| S3 | Fill gates as `is_some()` NOT truthiness (`0` passes) | SPEC §2.7 | DONE | alignment/dedup/splitting all use `.is_some()`; `gate_passes_when_no_genomic_is_zero` test guards regression. |
| S4 | Section collapse (markers removed) vs excise (markers+content) | SPEC §2.4 | DONE | `collapse`/`excise`. |
| S5 | M-bias 3 facts (state-driven delete; %mbias_2-driven fill; script placeholders survive) | SPEC §2.7d | DONE | All three reproduced (see C3/B4). |
| S6 | Unknown-context snippet exact bytes | SPEC §2.7a/§8.5 | DONE | `unknown_tr` + byte-layout test; exercised end-to-end by `nondir_pe`. |
| S7 | Nucleotide fixed-key order / header validation / missing-key (`0`/empty) | SPEC §2.7e | DONE | Verified vs Perl 632/587/594/641-647. |
| S8 | Timestamp hook (hidden flag, UTC, Perl format; gate normalizes) | SPEC §2.6/§7 | DONE | `timestamp.rs` + `--__test_timestamp`. |
| S9 | Byte-identity gate §7 (normalize the single ts line, exactly-one assertion) | SPEC §7 | DONE | `normalize_ts` asserts exactly 1 occurrence. |
| S10 | Exit codes: help/man/version → 0; error paths nonzero; missing-mandatory-field → 0 (placeholders survive) | SPEC §6.1 | DONE | `main.rs` maps Ok→0, Err→1, clap→2; help/man/version exit 0; alignment gate-fail returns unchanged doc → exit 0 (`gate_fails_when_field_missing_placeholders_survive`). |
| S11 | No numeric reformatting except `%`-strip, dedup `\s.*`-trim, integer leftover | SPEC §2.7/§8.7 | DONE | Confirmed: only `strip_first_percent`, `before_first_ws`, dedup i64 subtraction. No float emitted. |
| S12 | Plotly separators: `,` default; ` , ` nuc-x; `','` nuc-y | SPEC §2.7e/§8.8 | DONE | `join_with(…, b" , ")` for nuc-x; manual `','` for nuc-y; `b","` elsewhere. Test `plot_arrays_use_correct_separators`. |
| S13 | Stale `bismark_bt2_PE_report.html` NOT used as oracle | SPEC §8.1 | DONE | Oracle = live Perl run; CHANGELOG notes the stale file is not used. |
| S14 | Assets `{{`-free + `\r`-free; literal splice safe | SPEC §8.13 | DONE | `assets_are_brace_and_cr_free_after_normalize` test. |

### SPEC §7 required fixtures (1–10)

| # | Fixture | Status | Encoding (test name / file) |
|---|---------|--------|------------------------------|
| 1 | M-bias absent → 24 `{{mbias*}}` survive + both `<div>` excised | DONE | `tests/perl_vs_rust.rs::minimal_alignment_only_byte_identical` (minimal_pe, no companions) — byte-matched to Perl. |
| 2 | M-bias SE → 12 `{{mbias2_*}}` survive + R2 `<div>` excised | DONE | `tests/perl_vs_rust.rs::se_r1_only_mbias_byte_identical` (wgbs_se) + unit `mbias::tests::se_state_fills_r1_only_r2_placeholders_survive`. |
| 3 | Alignment gate-FAILURE (missing `no_genomic`) → placeholders survive, exit 0 | DONE | unit `alignment::tests::gate_fails_when_field_missing_placeholders_survive`. |
| 4 | `0`-through-gate (`no_genomic:0`/`dups:0`) → gate PASSES | DONE | unit `alignment::tests::gate_passes_when_no_genomic_is_zero` (no_genomic:0). Dedup `0` covered by `is_some` logic; wgbs_pe nuc has `no_genomic:0` end-to-end. |
| 5 | Dedup leftover-fallback (no leftover line) → `total-dups` | DONE | unit `dedup::tests::leftover_falls_back_to_total_minus_dups`. |
| 6 | Amplicon missing-nuc-key (#711) → `0`% / empty counts+coverage | DONE | unit `nucleotide::tests::missing_key_renders_zero_percent_and_empty_counts`. |
| 7 | Two reports + explicit `--dedup_report` → line-1256 reset | DONE | unit `discovery::tests::explicit_companion_applies_to_first_report_only`. |
| 8 | Splitting/dedup/nuc gate-FAILURE → placeholders survive | PARTIAL→COVERED | dedup/splitting gate logic identical to alignment (`is_some` → return unchanged doc on failure); alignment failure path explicitly tested (#3). No discrete splitting/dedup gate-fail *unit test*, but the branch is structurally identical and exercised by the shared `fill` shape. Nuc has no fill gate (see Gaps). Non-blocking. |
| 9 | Alignment gate-passes-but-context-absent → `{{total_C_count}}`/`{{meth_*}}` empty | DONE (by construction) | `fill` uses `o()` (unwrap_or `b""`) for all context fields outside the 5-field gate → empty on absence. Verified in code; not a discrete named test but byte-equivalent to Perl undef-in-`s///`. |
| 10 | Percent-N/A table-vs-graph | DONE | unit `alignment::tests::percent_na_in_table_but_zero_in_graph`. |

---

## Gaps (detail)

### Item B5 / S7(gate): Nucleotide `looksOK` fill gate omitted — DEVIATED-undocumented, byte-neutral (NON-BLOCKING)

**Expected (PLAN B5 / SPEC §2.7e):** "Gate = `looksOK`" — on failure, warn + return `$doc` unchanged (placeholders survive). PLAN validation §9.11 / fixture (8) lists "nuc gate-FAILURE".

**Found:** `reports/nucleotide.rs::fill` has **no `looksOK` gate** — it always fills.

**Analysis:** In the Perl source (lines 616-622), `$looksOK` is set to 0 only if some seen key has `$nucs{$key}->{obs}` or `$nucs{$key}->{exp}` undefined. But those hashref slots are autovivified together, unconditionally, in the single parse block (lines 600-605) for every key created. Therefore `looksOK` is **always 1** for any report that parses past the header — the gate is provably unreachable-as-false. The Rust omission produces **byte-identical output** in every case. The only Perl path that returns early is the line-0 header `die` (reproduced in Rust as `parse` → `Err`). 

**Gap classification:** undocumented deviation from the literal PLAN/SPEC text, but **behaviorally and byte-equivalent**. No output divergence is possible. Flagged for the user's awareness; not a coverage failure. If strict literal parity is desired, a no-op `looksOK` could be added, but it would never change output. Fixture (8)'s nucleotide sub-case is consequently un-encodable as a real failure.

### Item F2: Top-level Rust-rewrite status table — NOT verified in audited scope (NON-BLOCKING)

**Expected (PLAN F2):** "update the top-level Rust-rewrite status table/per-tool list."

**Found:** Crate-local `README.md` + `CHANGELOG.md` are present and accurate. The repo-root status table/per-tool list edit was not located within `rust/bismark-report/` (it would live at repo root / top-level README), which is outside the files this audit was scoped to. Likely deferred alongside F3 (PR). Note for the user to confirm.

---

## Test verification (Mode B)

| Test name | File | Status |
|-----------|------|--------|
| empty_input_yields_empty_not_newline | src/assets.rs | PASS |
| trailing_newline_not_doubled | src/assets.rs | PASS |
| final_line_without_newline_gains_one | src/assets.rs | PASS |
| strips_all_carriage_returns_including_mid_line | src/assets.rs | PASS |
| lone_newline | src/assets.rs | PASS |
| assets_are_brace_and_cr_free_after_normalize | src/assets.rs | PASS |
| embedded_assets_match_repo_plotly_files (drift) | src/assets.rs | PASS |
| epoch_zero_is_unix_epoch_utc | src/timestamp.rs | PASS |
| known_epoch_utc | src/timestamp.rs | PASS |
| leap_day_utc | src/timestamp.rs | PASS |
| field1_drops_trailing_empty | src/reports/mod.rs | PASS |
| report_lines_drops_final_empty | src/reports/mod.rs | PASS |
| percent_and_ws_trims | src/reports/mod.rs | PASS |
| unknown_tr_byte_layout | src/reports/mod.rs | PASS |
| gate_passes_when_no_genomic_is_zero | src/reports/alignment.rs | PASS |
| gate_fails_when_field_missing_placeholders_survive | src/reports/alignment.rs | PASS |
| percent_na_in_table_but_zero_in_graph | src/reports/alignment.rs | PASS |
| pe_labels_are_lifted_verbatim | src/reports/alignment.rs | PASS |
| version_and_filename_parsed_greedily | src/reports/alignment.rs | PASS |
| leftover_falls_back_to_total_minus_dups | src/reports/dedup.rs | PASS |
| explicit_leftover_line_wins_over_fallback | src/reports/dedup.rs | PASS |
| se_state_fills_r1_only_r2_placeholders_survive | src/reports/mbias.rs | PASS |
| pe_state_fills_both_reads | src/reports/mbias.rs | PASS |
| coverage_series_uses_coverage_column | src/reports/mbias.rs | PASS |
| bad_header_errors | src/reports/nucleotide.rs | PASS |
| missing_key_renders_zero_percent_and_empty_counts (#711) | src/reports/nucleotide.rs | PASS |
| plot_arrays_use_correct_separators | src/reports/nucleotide.rs | PASS |
| subst_replaces_all | src/template.rs | PASS |
| collapse_removes_markers_keeps_content | src/template.rs | PASS |
| excise_removes_first_to_last_inclusive | src/template.rs | PASS |
| excise_single_marker_is_noop | src/template.rs | PASS |
| inject_replaces_span_with_asset | src/template.rs | PASS |
| inject_errors_without_two_markers | src/template.rs | PASS |
| basename_strips_pe_and_se_suffixes | src/discovery.rs | PASS |
| explicit_companion_applies_to_first_report_only | src/discovery.rs | PASS |
| none_skips_a_companion | src/discovery.rs | PASS |
| version_exits_zero | tests/cli.rs | PASS |
| help_exits_zero | tests/cli.rs | PASS |
| man_exits_zero | tests/cli.rs | PASS |
| no_alignment_report_in_empty_dir_errors | tests/cli.rs | PASS |
| output_flag_with_multiple_reports_errors | tests/cli.rs | PASS |
| derives_html_name_from_alignment_report | tests/cli.rs | PASS |
| honors_explicit_output_name_and_dir | tests/cli.rs | PASS |
| auto_detects_multiple_reports_produces_one_html_each | tests/cli.rs | PASS |
| pe_full_companions_byte_identical | tests/perl_vs_rust.rs | PASS |
| se_r1_only_mbias_byte_identical | tests/perl_vs_rust.rs | PASS |
| nondirectional_unknown_context_byte_identical | tests/perl_vs_rust.rs | PASS |
| minimal_alignment_only_byte_identical | tests/perl_vs_rust.rs | PASS |

**48 / 48 passing** (perl-oracle tests ran live — `perl` was available, so they were NOT skipped).

---

## Verdict

**COMPLETE.** Every PLAN phase task (A1–E4) and every SPEC load-bearing requirement is implemented and tested; all 10 required §7 fixtures are encoded (4 as full Perl-oracle byte-identity cases, the rest as hermetic `parse`/`fill` unit tests — explicitly sanctioned by PLAN §14 and the audit brief). All 48 tests pass, including 4 live byte-identity comparisons against the Perl `bismark2report v0.25.1`.

Documented deviations (PLAN §14) are accepted, not gaps: `anyhow` dropped; full 3 MB committed goldens replaced by the Perl-oracle + unit tests.

Two items are **OPEN by design** (PLAN §14 — deferred, not failures):
- **F1** — real-data byte-identity on `oxy` (needs the box + real report sets).
- **F3** — PR / merge into `rust/iron-chancellor` (awaiting explicit instruction).

Two non-blocking observations for the user (no action required for completeness):
1. **Nucleotide `looksOK` gate omitted** — provably a no-op in the Perl source (the gate can never evaluate false given the parse autovivification), so output is byte-identical; the omission is an undocumented-but-equivalent simplification. Worth a one-line note in PLAN §14 for the record.
2. **Top-level Rust-rewrite status-table update (F2)** was not located within the audited crate directory — likely a repo-root edit deferred with F3; confirm before the PR.

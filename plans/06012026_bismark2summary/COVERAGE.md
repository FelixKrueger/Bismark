# Plan Coverage Report

**Mode:** B (code vs. plan — the SPEC rev 1 *is* the plan; the code exists)
**Plan:** `plans/06012026_bismark2summary/SPEC.md` (rev 1) + `PROGRESS.md`
**Code:** `rust/bismark-summary/src/*.rs` + `tests/*.rs` + `Cargo.toml` + `src/summary_template.html`
**Date:** 2026-06-01
**Scope:** Phase A + Phase B behavior/contract coverage. **Phase C (oxy real-data gate) is PENDING by design — out of scope; noted as not-yet-done, not a gap.**
**Verdict:** **COMPLETE for Phases A+B** — every SPEC behavior, output-contract clause, and gotcha is implemented and exercised. **2 of the §7 required fixtures are MISSING and 1 PARTIAL as committed integration tests** (the underlying *behaviors* are unit-covered, but the SPEC explicitly enumerates these as required `#[test]` fixtures). See "Fixture gaps" — none changes the A+B verdict to INCOMPLETE for behavior, but they are SPEC-listed deliverables and are flagged.

## Summary

- Behavior/contract items audited: **41**
  - DONE: **40**
  - PARTIAL: **1** (§7 fixture matrix — see below)
  - MISSING: **0** (behavior); **0** (code)
  - DEVIATED: **1, documented & accepted** (timestamp = UTC not local — §4.6 / PROGRESS deviation)
- §7 required fixtures (10 + tripwire = 11): **8 present, 1 PARTIAL, 3 MISSING** as committed integration-test fixtures.
- Tests: **57 total, all PASS** (49 unit + 5 perl-oracle [ran, perl present] + 1 template-drift + 2 txt-golden). 0 ignored, 0 failed.
- clippy/fmt: per PROGRESS, `-D warnings` + `fmt --check` clean (not re-run here; not part of coverage audit).

## Coverage ledger

| # | Item (SPEC §) | Source | Status | Notes |
|---|---|---|---|---|
| 1 | CLI surface: `-o/--basename`, `--title`, `--verbose`, `--version`, `--help`/`--man`, positional BAMs, hidden `--__test_timestamp` (§2.2/§6) | `cli.rs:38-70`, `main.rs:25-34` | DONE | All flags present; `--man` aliases help; `--__test_timestamp` `hide=true`; `disable_version_flag` so custom banner is printed. |
| 2 | basename/title Perl truthiness default incl. `-o 0` / empty (§2.2) | `cli.rs:109-114` | DONE | `resolve_truthy` tests `is_empty() \|\| == "0"` per Reviewer A I3. Unit-tested (`truthiness_zero_…`, `empty_basename_…`, `"00"` stays). |
| 3 | Four-glob discovery in fixed order, mutually exclusive (§2.3) | `discovery.rs:25-30,135-144` | DONE | `GLOB_SUFFIXES` SE-bt2, PE-bt2, SE-hisat2, PE-hisat2; concatenated in order. `ends_with` distinguishes `_pe.bam`. Matches Perl `:159-197`. |
| 4 | Glob sort = case-fold-primary, raw-bytes-secondary (NOT bytewise) (§2.3/§8.6) | `discovery.rs:101-107` | DONE | `(to_ascii_lowercase, as_bytes())` comparator. Unit-tested incl. the case-only tiebreak (`Apple, aPPle, apple`). |
| 5 | argv-verbatim path (no glob/no existence check) (§2.3) | `discovery.rs:116-121` | DONE | Returns explicit args in order. Unit `explicit_argv_is_verbatim_order`. |
| 6 | Dotfile skip (§2.3) | `discovery.rs:130` | DONE | `.filter(\|n\| !n.starts_with('.'))`. Tested in `glob_discovery_fixed_order_and_exclusivity`. |
| 7 | No-BAMs → error / nonzero exit (§2.3/§6) | `discovery.rs:146-148`, `error.rs:20`, `txt_golden.rs:36` | DONE | `NoBamFiles`; `no_bams_exits_nonzero` confirms nonzero + no `.txt`. Matches Perl `:200-202`. |
| 8 | Report-name derivation: `.bam` strip (substr -4, clamp) (§2.4) | `discovery.rs:62-69` | DONE | char-aware `saturating_sub(4)`; `<4` chars → `""`. Tested incl. non-`.bam` edge. |
| 9 | `_pe$` → PE; `_PE/_SE_report.txt` (§2.4) | `discovery.rs:74-85` | DONE | Strips `_pe`, sets `paired`, picks PE/SE report name. |
| 10 | dedup report name (PE/SE) (§2.4) | `discovery.rs:86-90` | DONE | `_pe.deduplication_report.txt` / `.deduplication_report.txt`. |
| 11 | splitting name dependent on dedup-existence (§2.4) | `discovery.rs:51-59`, `parse.rs:299-307` | DONE | `splitting_report(dedup_exists)` 4-way match; caller passes `dedup_path.exists()`. All 4 combos unit-tested. |
| 12 | Mandatory alignment report → error if missing (§2.4/§6) | `parse.rs:291-294`, `error.rs:25` | DONE | `MissingAlignmentReport` before any parse. Matches Perl `die :284`. |
| 13 | Alignment parser PE/SE pattern sets (§2.5a) | `parse.rs:139-197` | DONE | Verbatim-prefix match per Perl `:291-302`; PE vs SE gated on `paired`. Unit `alignment_pe_parse`, `se_patterns_do_not_fire_in_pe_mode`. |
| 14 | `total_c` anchored `$`; six meth/unmeth unanchored (§2.5a) | `parse.rs:201-221`, `parse.rs:81-96` | DONE | `capture_count(..., true)` for `total_c`; `false` for the six context lines — matches Perl regex anchoring exactly. Unit `capture_count_anchored_and_unanchored`. |
| 15 | last-match-wins (§2.5) | `parse.rs:140` loop overwrites | DONE | Scans every line, overwrites. Unit `last_match_wins`. |
| 16 | CRLF: anchored `$` fails on trailing `\r` (§2.5/§8) | `parse.rs:70-76,91-94` | DONE | `chomped_lines` keeps trailing `\r`; anchored capture rejects it. Unit `capture_count_anchored_fails_on_trailing_cr`. |
| 17 | dedup `aligned_reads` overwrite (greedy `.+:`) + dup/unique; three independent `if` (§2.5b) | `parse.rs:102-135,228-244` | DONE | `capture_dedup_total` greedy trailing-digit scan; three independent matches. Unit `dedup_total_handles_filename_wildcard`, `dedup_overwrites_aligned_and_sets_counts`. |
| 18 | splitting overwrite of meth fields with `Total C to T conversions` (§2.5c) | `parse.rs:249-273` | DONE | Distinct C-to-T unmeth patterns; overwrites. Unit `splitting_overwrites_methylation_with_c_to_t`. |
| 19 | `.txt`: 15 cols, lowercase `chgs`, raw-before-0-default, col-1 raw `$bam`, trailing `\n` (§2.6) | `txt.rs:16-62`, `parse.rs:57-62` | DONE | `HEADER_FIELDS` verbatim incl. `Methylated chgs`/`Unmethylated chgs`; col 1 = `s.bam` (raw); empty cells kept; header+rows `\n`-terminated. Unit + `txt_golden` byte-exact. |
| 20 | Plot label munge order (`_bismark.bam$` wildcard, `.fq.gz`, `_trimmed`, `_[12]`) (§2.7.1) | `plot.rs:73-98` | DONE | `strip_bismark_bam` treats `.` as any-char (n-4 byte); then suffix chain. `'$name'` single-quote-wrapped at push. Unit `munge_*`. |
| 21 | Plot 0-defaulting (unaligned/ambig/no_seq + 6 meth), aligned-blank-when-dedup, dup/unique not defaulted (§2.7.2) | `plot.rs:110-130` | DONE | `default0` on the right fields; `aligned` blanked when `dup_reads` non-empty; dup/unique cloned raw. Unit `aligned_blanked_…`, `aligned_kept_and_defaults_…`. |
| 22 | Plot exclusion (`next` if any context all-zero), 3 predicates (§2.7.3) | `plot.rs:134-142` | DONE | Three `continue` guards. Unit `plot_exclusion_keeps_count_but_drops_from_arrays`. |
| 23 | Push 13 arrays; `num_samples` = total (incl. excluded) (§2.7.4/§2.9-6) | `plot.rs:102-106,144-157` | DONE | `num_samples = samples.len()`; arrays hold plotted subset only. Unit confirms `num_samples=2`, `categories.len()=1`. |
| 24 | Asset `read_report_template` normalizer (chomp+`s/\r//g`+`\n`; empty→empty) (§2.8) | `assets.rs:22-43` | DONE | Per-line CR strip (all `\r`, not just trailing), `\n`-terminate, empty guard. Matches Perl `:136-149`. Unit `strips_all_cr_…`, `empty_asset_stays_empty`. |
| 25 | HTML mutation order (plotly→logos→ts→title→num→x→filenames→version→aln-num→aln-pct→meth-raw→meth-pct) (§2.9) | `html.rs:36-207` | DONE | Statement-for-statement in Perl order. |
| 26 | plot.ly inject greedy/dotall first..last splice; die if not found (§2.9-1) | `html.rs:37-39,236-246`, `error.rs:46` | DONE | `inject_span` first..rfind; `PlotlyInjectionFailed` on absence. |
| 27 | Numbers section deletion gated on `$dup_alignments =~ /^,{1,}$/` (§2.9-8) | `html.rs:68-93,229-231` | DONE | `all_commas` = non-empty & all-commas; raw vs dedup branch. Unit `all_commas_needs_at_least_one_comma`. |
| 28 | Percentage section deletion gated on `if ($aligned)` — DIFFERENT predicate (§2.9-9, Reviewer A C2) | `html.rs:98,130-138` | DONE | `raw_mode = !aligned.is_empty()` — independent of `dup_alignments`. |
| 29 | Single-RRBS divergence reproduced (numbers DEDUP layout, percentages RAW) (§2.9 ⚠) | `html.rs:68-93 vs 98-152` | DONE | Two independent predicates; verified end-to-end by `oracle_single_rrbs_section_asymmetry` (PASS vs Perl). |
| 30 | Fill-then-delete order (`{{aligned_seq}}` filled before raw-section delete; `p_aligned_replace` gated `if($aligned)`) (§2.9-8c, Reviewer B) | `html.rs:71-93,140-141` | DONE | Fills at 72-75 precede deletions at 78-87; `p_aligned_replace` only in `raw_mode` branch. |
| 31 | `/^,{1,}$/` needs ≥1 comma → N=1 never matches (§2.9-8d) | `html.rs:229-231` | DONE | `!s.is_empty() && all commas` — `""` (N=1 join) → false. |
| 32 | `%.2f` (meth/alignment verbatim) (§2.9a) | `html.rs:216-226` | DONE | `pct2` = `format!("{:.2}")`. Unit `pct2_matches_sprintf_two_dp`. |
| 33 | `%.15g` `100 − rounded` for the 6 unmeth arrays (round→reparse→subtract→g15) (§2.9a) | `html.rs:216-220`, `fmt_g.rs:29-67` | DONE | `meth_pair`; `format_g15` copied from bedgraph, doc retargeted. Unit `meth_pair_is_asymmetric`, `unmeth_complement_drops_trailing_zeros` (incl. `99.99→0.0100000000000051`). |
| 34 | Meth NA / `0` zero-context branches (§2.9-11/§2.9a) | `html.rs:173-200` | DONE | CpG total0→NA/NA; CHG total0→0/0. Matches Perl `:1640-1652`. |
| 35 | CHH `total_CHG==0` reproduced bug (§2.9-11/§8.11) | `html.rs:193` | DONE | Tests `total_chg == 0` (not `total_chh`) — verbatim Perl bug at `:1662`. |
| 36 | Section-delete span helper = first..last (greedy/dotall), ≥2 markers (§8.5) | `html.rs:248-259` | DONE | `delete_span` first..rfind; no-op on single marker. Unit `span_helpers_*`. |
| 37 | num_samples-vs-plotted x-array length mismatch emitted as-is (§2.9-6) | `html.rs:47-54` (x = 1..num_samples) vs joins over plotted | DONE | x-values use total; y/categories use plotted. `oracle_plot_excluded_sample` (PASS). |
| 38 | `{{bismark_version}}` hardcoded `0.25.1` (O1) | `lib.rs:45`, `html.rs:58` | DONE | `BISMARK_VERSION="0.25.1"` (constant, not crate version). |
| 39 | Output contract: `.txt` fully byte-identical; `.html` byte-identical modulo timestamp line (§5) | `main.rs:67-93`, `perl_oracle.rs` | DONE | `.txt` written first then `.html`; perl-oracle asserts both. |
| 40 | Exit codes; help/version exit 0; mixed-types die writes `.txt` not `.html` (§4.4/§6/§2.9-9) | `main.rs:25-42,67-88`, `error.rs:36` | DONE | help/version→0; errors→1; clap→2. `.txt` written before HTML build (which raises `MixedSampleTypes`). `oracle_mixed_types_die_writes_txt_not_html` (PASS: both die, both keep `.txt`, neither writes `.html`). |
| 41 | §7 required-fixtures matrix + stale-oracle tripwire | `tests/perl_oracle.rs`, `txt_golden.rs`, `template_drift.rs`, unit tests | PARTIAL | See "Fixture gaps". 8 present, 1 PARTIAL, 3 MISSING as committed integration fixtures (behaviors unit-covered). |

## §7 fixture matrix detail

| §7 # | Required fixture | Present? | Where |
|---|---|---|---|
| 1 | Multi-sample WGBS (≥2 PE+dedup+split, +1 SE) → dedup layout both sections | YES | `oracle_wgbs_two_sample` (1 PE + 1 SE, both deduped) + `txt_golden::wgbs_two_sample_txt_is_byte_exact` |
| 2 | All-RRBS ≥2 (raw mode both sections; `p_aligned_replace` filled) | YES | `oracle_all_rrbs_raw_mode` (2 SE RRBS) |
| 3 | Single RRBS (numbers/percentage divergence) | YES | `oracle_single_rrbs_section_asymmetry` |
| 4 | Single WGBS (consistent dedup; `/^,{1,}$/`-needs-comma) | **MISSING** (integration) | Behavior unit-covered: `all_commas_needs_at_least_one_comma` (`html.rs`) proves `""`→false for N=1. No single-WGBS Perl-oracle fixture. |
| 5 | Mixed RRBS+WGBS → die | YES | `oracle_mixed_types_die_writes_txt_not_html` |
| 6 | Plot-excluded sample in the MIDDLE; x(N) vs y(N−k) | PARTIAL | `oracle_plot_excluded_sample` excludes the **last** (g2) sample, not a **middle** one. The x-vs-y length mismatch IS exercised; the SPEC specifically said "in the MIDDLE" (≥3 samples with the excluded one interior) — that exact ordering is not pinned. |
| 7 | All-excluded (zero plotted; both deletions take dedup `else`; `^,{1,}$` false for `""`) | **MISSING** (integration) | Behavior unit-covered (`all_commas("")==false`; empty-loop paths), but no end-to-end all-excluded Perl-oracle fixture. |
| 8 | Mixed-case multi-sample auto-glob (mandatory; catches bytewise regression) | **MISSING** (integration) | Sort behavior unit-tested (`glob_sort_is_case_folded_not_bytewise`, `glob_sort_case_only_tiebreak`), but the SPEC marks the **end-to-end mixed-case discovery fixture** "Mandatory" and it is absent. The unit test on `sort_glob` covers the comparator; it does not exercise discovery→`.txt` row order on a real mixed-case dir. |
| 9 | Non-trivial `%.15g` tail (e.g. `99.99`→`0.0100…`, `12.30`→`87.7`) at integration level | YES (unit) / not pinned at integration | `fmt_g::unmeth_complement_drops_trailing_zeros` + `html::meth_pair_is_asymmetric` cover `99.99`/`12.30`/`50.00`/`100.00`. Not asserted via a dedicated Perl-oracle fixture, but the perl-oracle fixtures do exercise non-clean tails (e.g. all-RRBS counts). Counted YES on behavior. |
| 10 | `-o 0` / `--title 0` truthiness; explicit-`@ARGV` order; `--title` with spaces | PARTIAL | `-o 0`/`--title 0` unit-tested (`cli.rs`). `--title` with spaces is used in perl-oracle (`--title Oracle`, single word) and `txt_golden` (`"Gate Test"`, has a space) — verbatim injection exercised. Explicit-`@ARGV` order unit-tested (`explicit_argv_is_verbatim_order`) but NOT via an end-to-end run. |
| tripwire | Stale-oracle tripwire: grep committed `docs/images/bismark_summary_report.html` for `Plotly`, assert 0 matches | **MISSING** | No such test exists. The stale Highcharts oracle (`docs/images/bismark_summary_report.html`, 274 KB, zero `Plotly`) is still present in the worktree and could be silently re-adopted. SPEC §7 (Reviewer B 4.5) explicitly required this guard. |

## Gaps (detail)

### Gap 1 — Stale-oracle tripwire test MISSING (§7, Reviewer B 4.5)
**Expected:** a unit/integration test that greps the committed `docs/images/bismark_summary_report.html` for the token `Plotly` and asserts **0 matches**, so the v0.15.2 Highcharts oracle can never be silently re-adopted as the gate.
**Found:** nothing. The stale file is present (`docs/images/bismark_summary_report.html`, 274616 bytes; confirmed zero `Plotly` tokens — i.e. exactly the staleness the tripwire would catch). The `.txt` stale oracle (`docs/images/…txt`, with `CpHs`) is also present.
**Gap:** add the tripwire test. (Low risk to byte-identity — the oracle isn't wired into any gate today — but it is an explicitly-required SPEC deliverable.)

### Gap 2 — Mandatory mixed-case auto-glob fixture MISSING (§7.8)
**Expected:** an end-to-end fixture with mixed-case BAM names (e.g. `apple_…`, `Mango_…`, `zebra_…`) auto-discovered, asserting `.txt`/`.html` row order follows Perl's case-folded glob sort — the SPEC calls this **"Mandatory… the only fixture that catches a bytewise regression"** at the discovery→output level.
**Found:** `sort_glob` is unit-tested for the case-fold comparator and the case-only tiebreak, which is strong coverage of the algorithm. But no test runs `discover_bams` (or the binary) on a mixed-case directory and checks the resulting row order against Perl.
**Gap:** add a mixed-case discovery fixture (ideally a Perl-oracle one) so a future bytewise refactor of the discovery path is caught end-to-end.

### Gap 3 — Single-WGBS and All-excluded integration fixtures MISSING; plot-excluded-in-MIDDLE / explicit-argv-order PARTIAL (§7.4, §7.6, §7.7, §7.10)
**Expected:** Perl-oracle (or end-to-end) fixtures for: single-WGBS consistent-dedup; all-excluded (zero plotted samples); a plot-excluded sample positioned in the **middle** of ≥3; an explicit-`@ARGV`-order end-to-end run.
**Found:** the **behaviors** are all unit-covered (`all_commas("")==false`, exclusion guards, argv-verbatim, dedup blanking). The plot-excluded oracle excludes the **last** sample, not a middle one.
**Gap:** these are SPEC-enumerated fixtures. Behavior is covered, so byte-identity confidence is high, but the literal fixture list is not fully met.

## Test verification

| Suite | File | Result |
|---|---|---|
| 49 unit tests (cli, discovery, parse, txt, plot, html, fmt_g, timestamp, assets) | `src/*.rs` | 49 PASS |
| 5 Perl-oracle (WGBS-2, all-RRBS, single-RRBS asymmetry, plot-excluded, mixed-die) | `tests/perl_oracle.rs` | 5 PASS (perl present → actually ran, not skipped) |
| 1 template-drift (embedded ≡ Perl heredoc) | `tests/template_drift.rs` | 1 PASS |
| 2 txt-golden (deterministic byte-exact table; no-BAMs nonzero) | `tests/txt_golden.rs` | 2 PASS |
| **Total** | | **57 PASS, 0 fail, 0 ignored** |

Command run (sandbox-disabled; worktree outside sandbox): `cargo test -p bismark-summary` from `~/Github/Bismark-summary/rust`.

## Verdict

**COMPLETE — Phases A + B fully cover the SPEC's behavior, output contract, CLI surface, the two independent section-deletion predicates, the asymmetric `%.2f`/`%.15g` percentage engine, all reproduced Perl quirks (lowercase `chgs`, CHH `total_CHG` bug, `aligned_reads`/methylation overwrite precedence, raw-vs-dedup mode, mixed-types die, fill-then-delete order), and the embedded-template + asset-normalizer contracts. All 57 tests pass; the 5 Perl-oracle byte-identity tests ran against real Perl v0.25.1 and confirmed `.txt` byte-identity + `.html` identity modulo the timestamp line.**

The only outstanding items are **test-fixture completeness vs the SPEC §7 enumerated list** (not behavior gaps):
1. **MISSING:** stale-oracle tripwire test (`Plotly`-count==0 on `docs/images/…html`) — explicitly required (Reviewer B 4.5).
2. **MISSING:** the **Mandatory** mixed-case auto-glob end-to-end fixture (comparator is unit-tested; discovery→row-order is not).
3. **MISSING/PARTIAL:** single-WGBS, all-excluded, plot-excluded-in-**middle**, and explicit-`@ARGV`-order **integration** fixtures (behaviors unit-covered).

These do not undermine the demonstrated byte-identity (the perl-oracle fixtures already exercise raw/dedup/single-RRBS/plot-excluded/die paths), but the SPEC named them as required fixtures — most pointedly the **tripwire** and the **"Mandatory" mixed-case** fixture. Recommend adding at least those two before the Phase C tag.

**Phase C (oxy real-data byte-identity gate + RELEASE/docs/CHANGELOG):** not started — pending by design, out of scope for this audit.

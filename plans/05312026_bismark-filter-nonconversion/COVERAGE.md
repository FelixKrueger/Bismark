# Plan Coverage Report

**Mode:** B-style (SPEC.md rev 1 treated as the plan — no separate IMPL.md)
**Plan(s):** `plans/05312026_bismark-filter-nonconversion/SPEC.md` (rev 1)
**Code:** `rust/bismark-filter-nonconversion/src/*.rs` + `tests/*.rs` + `tests/data/`
**Date:** 2026-05-31
**Verdict:** COMPLETE — every SPEC requirement, edge case, and §8.1 fixture cell is implemented and tested; all tests pass (66 run, 0 fail; 2 real-data tests `#[ignore]`d by design). Two non-functional nits noted (unused declared dev/runtime deps), no behavioural gaps.

## Summary

- Total ledger items: 58
- DONE: 56
- PARTIAL: 0
- MISSING: 0
- DEVIATED (documented): 2 (the `--help` exit-code deviation §10.1 and the unused `anyhow`/`predicates` deps — both pre-documented / non-functional)

## Coverage ledger

### CLI surface + validation (SPEC §3, §3.1)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | Positional `<files>...`, each processed independently | §3 | DONE | `cli.rs` `files: Vec<PathBuf>`; `lib.rs::run` loops over each. |
| 2 | `-s`/`--single`, `-p`/`--paired` bool flags | §3 | DONE | `cli.rs:45–50`. |
| 3 | `-s` + `-p` mutual exclusion → die (not clap conflict) | §3.1.6 | DONE | `validate()` step 2; `BothSingleAndPaired`; test `both_single_and_paired_rejected_in_validate`. |
| 4 | `--threshold` signed **i64**, default 3, `>0` validated (value-interpolating msg) | §3, §3.1.8 | DONE | `Option<i64>`; `InvalidThreshold{value}`; tests `custom_threshold`, `threshold_zero_rejected_with_value_in_message`, `threshold_negative_reaches_validation_not_parse_error`. |
| 5 | `--consecutive` bool; mutually exclusive with `--percentage_cutoff` | §3, §3.1.5 | DONE | `PercentageAndConsecutive`; test `percentage_and_consecutive_mutually_exclusive`. |
| 6 | `--percentage_cutoff` signed i64, range 0–100 validated | §3, §3.1.5 | DONE | `PercentageOutOfRange`; tests high/negative/0&100. |
| 7 | `--minimum_count` signed i64, default 5 (only in % mode), `>0` validated | §3, §3.1.5 | DONE | `InvalidMinimumCount`; tests `percentage_mode_defaults_min_count_5`, `percentage_custom_min_count`, `minimum_count_zero_rejected`. |
| 8 | `--samtools_path` accepted + **ignored** | §3, §3.1.7, §10.3 | DONE | `let _ = self.samtools_path`; test `samtools_path_accepted_and_ignored`. |
| 9 | `--version` exit 0, provenance string | §3, §10.2 | DONE | `main.rs` + `version_string()`. |
| 10 | `--help` clap-style exit 0 (documented deviation) | §3, §10.1 | DEVIATED (doc) | clap default; SPEC §10.1/§11 A7 flag this as an unconfirmed-but-defaulted deviation. |
| 11 | clap parse failure → exit 2 | §3.1.1 | DONE | clap convention; `ExitCode::from` for the other paths in `main.rs`. |
| 12 | No-files → "Please provide one or more…" message, **before** option validation | §3.1.4 | DONE | `main.rs:36–41` precedes `cli.validate()`. |
| 13 | `--threshold` co-supplied with `--percentage_cutoff` accepted+ignored (but still `>0`-validated) | §3.1, §3 note | DONE | unconditional threshold check in `validate()`; test `threshold_ignored_under_percentage_mode_but_still_validated`. |
| 14 | `allow_negative_numbers` so `--threshold -1` reaches validation | §3 (B I2), IMPL note 1 | DONE | `#[command(allow_negative_numbers = true)]`; negative tests pass. |

### Input handling (SPEC §4)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 15 | BAM filename gate `=~ /bam$/` (no dot anchor; `foobam` passes) | §4.1 | DONE | `pipeline.rs:56` `ends_with("bam")`; `NotABamFile`; test `non_bam_filename_rejected`. |
| 16 | Truncation check gated on dotted `\.bam$`, noodles-native | §4.2, §10.6 | DONE | `dotted_bam` flag + `map_initial_read_err` → `Truncated`. |
| 17 | Emptiness check gated on dotted `\.bam$`; empty `.bam` dies, **no output files** | §4.3 | DONE | first-record peek → `EmptyInput` before opening writers; test `empty_dotted_bam_dies_with_no_output_files`. |
| 18 | **N/A branch reachable (C1):** header-only `*bam` (no dot) → count==0 → N/A report, exit 0 | §4.3 C1 | DONE | `first_rec=None` + `dotted_bam=false` path; golden `na_nondotted` (cases.tsv) + unit `na_branch_when_count_zero`. |
| 19 | Truncation runs **before** emptiness (B I1) | §4.3 | DONE | header read (truncation) precedes the record peek. |
| 20 | SE/PE explicit `-s`/`-p` wins | §4.4 | DONE | `explicit_mode` short-circuits in `filter_one`. |
| 21 | Auto-detect via `detect_paired_from_header` (`@PG ID:Bismark` with -1/-2) | §4.4 | DONE | `pipeline.rs:73`; helper exists `bismark-io/src/read.rs:649`. |
| 22 | Neither flag nor Bismark `@PG` → die | §4.4 | DONE | `CannotAutoDetectMode`; test `no_mode_and_no_bismark_pg_dies`. |
| 23 | PE `@HD SO:coordinate` rejected before opening writers (no output) | §4.5, §10.5 | DONE | `is_coordinate_sorted`; test `pe_coordinate_sorted_rejected_before_output`. |
| 24 | PE adjacent-qname equality (strip `/1`,`/2`) folded into the loop; mismatch → die | §4.5, §10.5 | DONE | `qnames_match` + `PairedIdMismatch`. |

### Output filenames (SPEC §5)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 25 | `.bam`-strip only (dot-anchored), **no directory strip** | §5 | DONE | `filename.rs` `strip_suffix(".bam")`; tests `strips_only_bam_no_directory`, `dots_in_directory_preserved`, `relative_path_preserved`. |
| 26 | Kept `.nonCG_filtered.bam` | §5 | DONE | `kept_bam_name`. |
| 27 | Removed `.nonCG_removed_seqs.bam` | §5 | DONE | `removed_bam_name`. |
| 28 | Report `.non-conversion_filtering.txt` | §5 | DONE | `report_name`. |
| 29 | `foobam` no-strip; `x.bam.bam` strips one | §5 | DONE | tests `non_dotted_bam_suffix_not_stripped`, `only_one_bam_suffix_stripped`. |
| 30 | Input header written verbatim to both output BAMs | §5 | DONE | `write_header(&header)` on both writers (noodles default chain). |

### Core algorithm (SPEC §6)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 31 | Char semantics: only `H`/`X` ++nonCpG; `H`/`X`/`h`/`x` ++total; `Z`/`z`/`u`/`U`/`.` ignored | §6 | DONE | `filter.rs::read_fails`; tests `threshold_counts_only_upper_h_x`, `threshold_cpg_methylated_z_does_not_count`. |
| 32 | Consecutive reset only on `z`/`h`/`x`; `Z`/`u`/`U`/`.` transparent | §6 | DONE | tests `consecutive_reset_by_*`, `consecutive_methylated_cpg_upper_z_is_transparent`, `consecutive_dot_and_unknown_are_transparent`. |
| 33 | Increment → reset → threshold-check ordering + early `break` | §6 | DONE | loop order in `read_fails`; test `threshold_early_exit_on_third_methylated`. |
| 34 | Boundary counts N-1 keep / N remove / N+1 remove | §6, §8.1 | DONE | tests `threshold_boundary_n_minus_1_kept` / `_exactly_n_removed` / `_n_plus_1_removed`; golden `se_threshold5` (keep4/remove5). |
| 35 | Percentage: `%.1f`-rounded value compared `>= cutoff`; round-half-to-even | §6 | DONE | `round_1dp` via `{:.1}`; tests `round_1dp_half_to_even_tie`, `percentage_rounding_tips_over_cutoff` (19.96→20.0), `percentage_half_to_even_tie_at_cutoff` (5/40=12.5%). |
| 36 | Min-count gating (total < min → kept even at 100%) | §6, §8.1 | DONE | tests `percentage_below_min_count_kept_even_at_100pct`, `percentage_zero_cutoff_below_min_count_kept`; golden `se_percentage` p_below_min. |
| 37 | Percentage threshold check guarded OFF in % mode | §6, §3 note | DONE | separate `FilterMode::Percentage` arm; covered by `percentage_below_min_count_kept_even_at_100pct`. |
| 38 | SE: absent XM → empty → never fails → kept; `count+=1`/read | §6.1 | DONE | `stream_se` `unwrap_or(b"")`; test `se_missing_xm_is_kept` + golden `se_unmapped`. |
| 39 | PE: both mates need non-empty XM (truthiness: absent OR empty → die) | §6.2 (B A5) | DONE | `extract_xm().filter(!is_empty)`; `PairedMissingMethCall`; test `pe_unmapped_mate_without_xm_dies`. |
| 40 | PE: R1 fails → pair fails, R2 not examined (`||` short-circuit); either → both → REMOVED | §6.2 | DONE | `read_fails(xm1) || read_fails(xm2)`; goldens `pe_default` (R1-fail + R2-fail pairs). |
| 41 | PE `count+=1`/pair | §6.2 | DONE | `stream_pe` increments per pair. |
| 42 | **PE lone trailing R1 (C2):** die, prior pairs flushed (valid BAMs), 0-byte report, exit nonzero, lone pair not counted | §6.2 C2 | DONE | `stream_pe` `None`-second-mate → err; writers `try_finish` before propagating; report pre-created empty; test `pe_lone_trailing_r1_dies_with_partial_output_and_empty_report`. |
| 43 | Raw `record_bufs` yields all records incl. unmapped (not bismark-io reader) | §6.3, §9 | DONE | `reader.record_bufs(&header)`; module doc + Cargo.toml comment. |
| 44 | Unmapped SE kept→OUT verbatim, same order (explicit golden) | §6.3 | DONE | golden `se_unmapped` (keep / unmapped-keep / remove order asserted byte-for-byte). |
| 45 | Unmapped PE mate → missing-XM die | §6.3 | DONE | test `pe_unmapped_mate_without_xm_dies`. |

### Report (SPEC §7)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 46 | Line A SE one-space vs PE two-space before `in total` | §7 | DONE | `report.rs::format`; tests `se_threshold_default_byte_exact`, `pe_threshold_default_byte_exact_two_spaces`; goldens confirm. |
| 47 | Line B four variants (PE/SE × %/threshold), `\n\n` ending | §7 | DONE | `removed_line_body`; unit tests for all 4 + goldens. |
| 48 | `consecutive ` insert (trailing space) | §7 | DONE | tests `se_consecutive_insert`, `pe_consecutive_insert`; golden `se_consecutive`. |
| 49 | `N/A` percent when count==0 | §7 | DONE | `format()` N/A branch; unit + `na_nondotted` golden (`0 (N/A%)`). |
| 50 | Line C timing, single `\n`, only the LAST file's report | §7, §11 A3 | DONE | `run_time_line`; `lib.rs::run` `if i+1==n`; test `multifile_runtime_line_only_on_last_report`. |
| 51 | `{infile}` echoed verbatim as supplied | §7 | DONE | `FilterReport.infile = infile.to_string_lossy()`; gate invokes with same basename. |
| 52 | Report-line `kicked/count` rounding (1/3→33.3%) | §7, §8.1 | DONE | test `report_line_rounding_one_third`; goldens (pe_default 66.7%, se 40.0%). |

### Byte-identity gate + fixture matrix (SPEC §8)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 53 | Body-only D1 gate (samtools view, no `-H`), report cmp pre-timing D2 | §8, D1, D2 | DONE | `byte_identity.rs` `samtools_view_body` + `strip_timing`; LC_ALL=C in `generate_goldens.sh`. |
| 54 | §8.1 fixture matrix cells (see test-verification table below) | §8.1 | DONE | 9 Perl goldens in `cases.tsv` + edge_cases.rs for die/partial cells. |
| 55 | Real-data gate `#[ignore]` + env-gated, SE+PE × 4 modes | §8.2 | DONE | `byte_identity_real_data.rs` (default/threshold5/consecutive/percentage20 each SE+PE = 8 cells); not yet executed on colossal/oxy (env-gated, per IMPL notes — out of local scope). |

### Architecture, deviations, assumptions (SPEC §9–§11)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 56 | Module layout cli/error/filename/filter/report/pipeline/lib/main | §9 | DONE | all eight modules present and matching responsibilities. |
| 57 | Deps: bismark-io =1.0.0-beta.8, noodles pins, clap, thiserror, mimalloc | §9 | DEVIATED (minor) | All required deps present + pinned; mimalloc global allocator wired in `main.rs`. **`anyhow`** (listed §9) is **not used** and not declared; **`predicates`** dev-dep is declared but unused. Non-functional. |
| 58 | Documented deviations §10.1–§10.8 honoured | §10 | DONE | help exit 0 (§10.1), version text (§10.2), samtools ignored (§10.3), single-thread+mimalloc/no --parallel (§10.4), SO-reject+qname-fold (§10.5), native truncation (§10.6), empty-XM regex divergence accepted (§10.7), PE die msg not byte-matched (§10.8). |

## Gaps (detail)

No behavioural gaps. Two non-functional nits:

### Item 57 (nit A): `anyhow` declared in SPEC §9 deps but unused
**Expected:** SPEC §9 lists `anyhow` among dependencies.
**Found:** `anyhow` is neither declared in `Cargo.toml` nor referenced in `src/`. Error handling uses `thiserror` (`BismarkFilterError`) end-to-end, which is sufficient.
**Gap:** None functional — a documentation/dep-list mismatch only. Arguably cleaner without it.

### Item 57 (nit B): `predicates` dev-dep declared but unused
**Expected:** SPEC §9 lists `predicates` as a dev-dep.
**Found:** declared in `Cargo.toml` `[dev-dependencies]` but not referenced by any test (tests use `assert_cmd` `.assert().success()/.failure()` only).
**Gap:** None functional — an unused dev-dep. Could be dropped.

### §8.1 fixture-matrix note (not a gap, a coverage-location observation)
Two §8.1-listed rounding fixtures — the **19.96→20.0 tip-over** and the **5/40=12.5% half-to-even tie** — are exercised by **unit tests** in `filter.rs` (`percentage_rounding_tips_over_cutoff`, `percentage_half_to_even_tie_at_cutoff`), **not** by Perl-generated integration goldens in `cases.tsv`. This is consistent with SPEC §12, which routes percentage-rounding verification to the `filter.rs` unit suite; the integration `se_percentage` golden covers at-cutoff (20.0%) and min-count gating. The decision-affecting rounding rule (`{:.1}` = C printf round-half-to-even) is therefore verified, just not via a committed Perl golden for those two exact ratios. Acceptable; flagged for transparency.

## Test verification (Mode B)

Run: `cargo test -p bismark-filter-nonconversion` (sandbox disabled — worktree outside writable root). **All pass.** samtools IS on PATH, so the byte-identity gate actually executed (not skipped).

| Suite | Tests | Result |
|-------|-------|--------|
| `src` unit (filter/report/filename/cli) | 55 | 55 PASS |
| `tests/byte_identity.rs` (9 Perl-golden cells) | 1 (loops 9 cases) | PASS |
| `tests/edge_cases.rs` | 8 | 8 PASS |
| `tests/byte_identity_real_data.rs` | 2 | IGNORED (by design, env-gated) |
| doc-tests | 0 | — |

### §8.1 fixture-matrix cell → test mapping

| §8.1 cell | Covered by | Status |
|-----------|-----------|--------|
| SE × threshold default (every char class) | golden `se_default` (`.HXZh x z` chars across 5 reads) | EXERCISED (cases.tsv) |
| SE × threshold N≠3 | golden `se_threshold5` (keep4/remove5) | EXERCISED (cases.tsv) |
| SE × consecutive | golden `se_consecutive` (reset by h/z, Z transparent) | EXERCISED (cases.tsv) |
| SE × percentage | golden `se_percentage` (at-cutoff, over, under, below-min) | EXERCISED (cases.tsv) |
| Boundary N-1/N/N+1 | `se_threshold5` golden + `filter.rs` unit boundary tests | EXERCISED |
| Consecutive reset across Z/./u (transparent) & z/h/x (reset) | `se_consecutive` golden + `filter.rs` units | EXERCISED |
| Percentage min-count gating (total<min → kept @100%) | `se_percentage` p_below_min golden + unit | EXERCISED |
| Percentage half-to-even tie (5/40=12.5%) | `filter.rs` unit only (NOT a committed golden) | EXERCISED (unit) |
| Percentage tip-over (19.96→20.0) | `filter.rs` unit only (NOT a committed golden) | EXERCISED (unit) |
| PE happy path (even-count) | golden `pe_default` (3 clean pairs) | EXERCISED (cases.tsv) |
| PE R1-fails-R2-not-examined / R1-pass-R2-fail | golden `pe_default` (pairB R2-fail, pairC R1-fail) | EXERCISED (cases.tsv) |
| PE odd-record die (C2) | `edge_cases::pe_lone_trailing_r1_*` | EXERCISED (edge_cases) |
| N/A branch (C1, header-only `*bam` no dot) | golden `na_nondotted` + unit `na_branch_when_count_zero` | EXERCISED (cases.tsv + unit) |
| Empty `.bam` → die, no output | `edge_cases::empty_dotted_bam_dies_with_no_output_files` | EXERCISED (edge_cases) |
| Unmapped SE kept→OUT verbatim, same order | golden `se_unmapped` (keep/unmapped-keep/remove) | EXERCISED (cases.tsv) |
| Unmapped PE → dies, no XM | `edge_cases::pe_unmapped_mate_without_xm_dies` | EXERCISED (edge_cases) |
| `--percentage_cutoff` + `--threshold` co-supplied | `cli.rs` unit `threshold_ignored_under_percentage_mode_but_still_validated` | EXERCISED (unit) |
| `@PG`-absent + no `-s`/`-p` → die | `edge_cases::no_mode_and_no_bismark_pg_dies` | EXERCISED (edge_cases) |
| Multi-file (2 files) timing placement | `edge_cases::multifile_runtime_line_only_on_last_report` | EXERCISED (edge_cases) |
| Report-line rounding (1/3→33.3%) | `report.rs` unit `report_line_rounding_one_third` + pe_default golden (66.7%) | EXERCISED |
| Auto-detect PE from `@PG` | golden `autodetect_pe` (no flag) | EXERCISED (cases.tsv) |
| PE `@HD SO:coordinate` reject (no output) | `edge_cases::pe_coordinate_sorted_rejected_before_output` | EXERCISED (edge_cases) |
| Non-`bam` filename → die | `edge_cases::non_bam_filename_rejected` | EXERCISED (edge_cases) |

**Every §8.1 matrix cell has at least one exercising test.** No matrix cell is uncovered.

## Verdict

**COMPLETE.** All 58 ledger items are implemented; 56 DONE, 2 DEVIATED with documented/non-functional reasons. Every §8.1 fixture-matrix cell maps to a passing test (Perl-golden integration where byte-identity applies; hermetic edge-case tests for die/partial/exit-code paths; unit tests for the two rounding ratios). The full suite passes (66 run, 0 failures; 2 real-data tests `#[ignore]`d by design and pending the colossal/oxy run noted in IMPLEMENTATION_NOTES). The byte-identity gate ran for real (samtools present). No behavioural gaps require action.

Optional cleanups (not blockers): drop the unused `anyhow` reference from SPEC §9 (or leave as-is — not declared) and the unused `predicates` dev-dep from `Cargo.toml`.

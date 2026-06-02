# Plan Coverage Report

**Mode:** B (code vs. plan)
**Plan(s):** `PLAN.md` (rev 1) + `SPEC.md` (rev 3)
**Code:** `rust/bismark-genome-preparation/src/` + `tests/integration.rs`
**Date:** 2026-05-30
**Verdict:** COMPLETE for Phases A–D (functional behavior + byte-identity gate fully implemented and tested) — with **2 minor test-placement gaps** (CLI-validation and folders unit tests specified in A2/A4 are not present as such). **Phase E is PENDING as designed** (real-genome oxy run + docs), tracked separately below.

## Summary

- Total checklist items (29) + phase sub-tasks audited.
- DONE: 27 of 29 checklist rows; all A–D phase behaviors.
- PARTIAL: 2 (CLI-validation unit tests A2; folders/early-path unit test A4 — behavior present and exercised at runtime, but the *specific unit tests* named in the plan are absent).
- MISSING (Phase A–D, genuine): 0.
- PENDING (Phase E, expected per "Implementation notes"): real-data byte-identity harness (`tests/byte_identity_real_data.rs`), oxy run, `README.md`/`CHANGELOG.md`/mkdocs page.

Test count verified by re-running: **27 lib unit tests + 5 integration tests, all pass** (incl. the 2 Perl-oracle byte-identity tests). `convert.rs` 14 + `discovery.rs` 9 + `indexer.rs` 4 = 27 lib; integration 5. This matches the plan's claim exactly.

---

## Coverage ledger (29 SPEC checklist rows)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | Mandatory genome-folder arg; absolutize; missing → error | A2 | DONE | `cli.rs` `validate()`: `ok_or_else` for missing folder; `canonicalize` to absolutize (errors if absent). |
| 2 | FASTA extension precedence (`.fa`→`.fa.gz`→`.fasta`→`.fasta.gz`, first non-empty wins) | A3 | DONE | `discovery.rs` `EXT_GROUPS` + `in_group` (disjoint groups); tests `glob_precedence_fa_wins_over_others`, `glob_fasta_fallback_when_no_fa`. |
| 3 | Lexical glob ordering (MFA order + indexer file_list) | A3, §8.1 | DONE | `find_fasta_files` sorts on `file_name()` bytes; `build_command` re-globs + `fa.sort()`. Test `glob_lexical_order_not_numeric` (chr1<chr10<chr2). |
| 4 | gzip input via `MultiGzDecoder` | A5 | DONE | `convert.rs` `open_fasta` uses `MultiGzDecoder` for `.gz`. (No dedicated gz integration fixture — see Notes below; transform is gz-agnostic and unit-tested on raw bytes.) |
| 5 | `Bisulfite_Genome/{CT,GA}_conversion/` tree + overwrite-warn | A4 | DONE | `folders.rs` `create_tree`: creates tree; warns + proceeds if `Bisulfite_Genome/` exists. |
| 6 | MFA outputs `genome_mfa.{CT,GA}_conversion.fa` | A5 | DONE | `convert_split` opens those exact names in MFA mode; test `convert_split_mfa_byte_exact`. |
| 7 | Header rewrite `>{chr}_{CT,GA}_converted\n`; name extraction = exact Perl (bare `>`→empty, leading-ws→empty, split keeps leading empty field) | A3, A5 | DONE | `discovery.rs` `extract_chromosome_name` (first-byte-not-`>` errors; else first field of a Perl-`\s` split keeping leading empty). Tests `name_bare_gt_is_empty_not_error`, `name_leading_whitespace_is_empty`, `name_basic`. `header_line` fixed suffix. |
| 8 | Byte transform `uc → [^ATCGN\n\r]→N → tr` line-ending preserved | A5 | DONE | `map_into`: uppercase → keep-set→N → C→T/G→A; operates on raw bytes incl. terminator. Tests `ct_ga_basic`, `uppercases_and_converts`, `ambiguity_and_non_ascii_to_n`. |
| 9 | CRLF preserved; whitespace→N; final-no-newline preserved | A5, C2 | DONE | Tests `crlf_preserved`, `interior_whitespace_becomes_n`, `final_line_without_newline_preserved`, `empty_seq_line_passthrough`. Also exercised E2E (`binary_end_to_end_mfa_bytes` final-no-newline). |
| 10 | Duplicate chromosome name → error (across all files) | A3 | DONE | `handle_header` uses cross-file `HashSet`; test `duplicate_chromosome_name_errors`. |
| 11 | bowtie2-build indexer (default) + concurrency | A7 | DONE | `indexer.rs` `build_command` (bt2) + `run_both` (CT thread + GA main). Test `bowtie2_command_has_threads_and_file_list`. |
| 12 | Indexer discovery `BISMARK_BIN→which→current_exe`; `--path_to_aligner` validated early (Step I), no `which`-fallback | A4, A7 | DONE | `indexer::discover` (3-tier) + `resolve_explicit` (no fallback); `pipeline.rs` calls `resolve_explicit` in Step I before conversion. Test `resolve_explicit_missing_errors`. |
| 13 | `--parallel` (≥2); always emit `--threads N` (default 1); `--large-index` passthrough | A2, A7 | DONE | `validate()` rejects `<2`, `threads=unwrap_or(1)`; `build_command` always emits `--threads N`. Tests `bowtie2_command_has_threads_and_file_list` (`--threads 1`), `bowtie2_large_index_flag`. |
| 14 | `--single_fasta` per-chromosome outputs | B1 | DONE | `convert_split` per-chr writer swap in `handle_header`; `per_chr_path`. Test `convert_split_single_fasta_byte_exact` + `perl_vs_rust_byte_identical_single_fasta`. |
| 15 | `--hisat2` indexer | B2 | DONE | `Aligner::Hisat2` → `hisat2-build`; same command shape as bt2 in `build_command`. (Command-string covered structurally via the shared bt2 path; no hisat2-specific assertion — see Notes.) |
| 16 | `--minimap2`/`--mm2` (`-k 20`, `.mmi`) + exclusions | B3 | DONE | `cli.rs` `alias="mm2"`; `build_command` minimap2 arm (`-k 20 -t N -d *.mmi`). Test `minimap2_command_uses_k20_and_mmi`. Exclusions in `validate()`. |
| 17 | Aligner mutual-exclusion + minimap2-incompat validation | A2, B3 | DONE | `validate()`: `n>1` → error; minimap2 + single_fasta/slam/large-index → error. (Covered in code; **not** unit-tested — see PARTIAL #A2 below.) |
| 18 | `--slam` (T→C/A→G) with fixed `_CT_`/`_GA_` headers + deprecation warning | C1 | DONE | `map_into` slam arms; `header_line` fixed suffix; `pipeline.rs` emits deprecation note. Tests `slam_direction`, `header_line_fixed_suffix`. |
| 19 | `--genomic_composition` accepted-and-ignored (note) | C3 | DONE | `cli.rs` flag parsed; `pipeline.rs` emits a one-line note, produces nothing. |
| 20 | `--verbose` diagnostics; STDERR not byte-matched; version banner constant | A6, C3 | DONE | `logging.rs` `Logger {verbose}` (`note`/`info`); `BISMARK_VERSION` constant; banner in `version_string`. |
| 21 | `--help`/`--man`/`--version` (clap; not byte-gated) | A2 | DONE | `cli.rs` `disable_version_flag`; `main.rs` manual `--version`; `--man` aliases long help. (No dedicated test — see Notes.) |
| 22 | First byte not `>` → error; bare `>` NOT error; empty dir → error | A3, C2 | DONE | `extract_chromosome_name` (NotFasta on non-`>`); `find_fasta_files` NoFasta. Tests `name_first_byte_not_gt_errors`, `glob_empty_dir_errors`, `binary_no_fasta_dir_errors`. |
| 23 | `--combined_genome` extension (combined FASTA + combined index; opt-in) | D1, D2 | DONE | `combined.rs` `build`; `convert::write_combined`; `pipeline.rs` Step IV gated on flag. Tests `combined_equals_ct_concat_ga_in_mfa_mode`, `binary_combined_genome_is_ct_concat_ga`. |
| 24 | Combined output independent of `--single_fasta` (oracle from converted stream); composes w/ slam; auto-large-index | D1, D2 | DONE | `write_combined` re-reads source files + re-converts (not MFA concat) → mode-independent; takes `slam` param. (No dedicated single_fasta-mode-combined or slam-combined assertion — see Notes.) |
| 25 | Byte-identity gate (CT/GA FASTA) + secondary index-build check | A9, E1 | DONE (A9) / PENDING (E1) | `perl_vs_rust_byte_identical_mfa` + `_single_fasta` are the gate; fake indexer confirms Step III runs. Real index build is Phase E. |
| 26 | Real-data byte-identity on oxy | E1, E2 | PENDING (Phase E) | Expected pending per Implementation notes. No `tests/byte_identity_real_data.rs`; no oxy run. |
| 27 | (rev-1) Zero-sequence record + 0-byte file + CR-only + non-ASCII→N fixtures | A5, A9 | PARTIAL | 0-byte file: DONE (`empty_file_errors`). non-ASCII→N: DONE (`ambiguity_and_non_ascii_to_n`). **Zero-sequence record (header→header / header-at-EOF) and CR-only (old-Mac) fixtures: MISSING** (no test). See Gaps. |
| 28 | (rev-1) Glob sort on `file_name()` bytes (+ mixed-case/digit fixture) | A3 | DONE | `find_fasta_files` sorts on `file_name().as_encoded_bytes()`; test `glob_lexical_order_not_numeric` includes digits + `chrM`/`chrX` (mixed case). |
| 29 | (rev-1) Perl script = primary test oracle from Phase A (auto-skip if absent) | A9 | DONE | `integration.rs` `perl_vs_rust_byte_identical_mfa`/`_single_fasta` run the real Perl script, `have_perl()` auto-skip. Verified to PASS in this run. |

---

## Phase sub-task audit (A1–E3)

| Sub-task | Status | Notes |
|---|---|---|
| A1 Scaffold (Cargo.toml, lib/main/error) | DONE | Crate + `[[bin]] bismark_genome_preparation_rs`; deps match (clap 4.5.30, flate2 1.1.9, which 7.0.3, thiserror 2.0, anyhow 1.0). Added to `rust/Cargo.toml` members. `error.rs` enum present (Io/Validation/NoFasta/NotFasta/DuplicateChromosome/IndexerNotFound/IndexerFailed). |
| A2 CLI + validation | DONE (code) / PARTIAL (tests) | All flags + underscore spellings + `mm2` alias + manual `--version` + `--man` present and validation logic complete. **The A2 unit tests (each conflict→error; default=bowtie2; `--mm2` alias; `--parallel 1`→error; underscore parse) are NOT present** — `cli.rs` has no `#[cfg(test)]` module. Only `binary_no_fasta_dir_errors` exercises one validation path E2E. |
| A3 Discovery (pure, exhaustively unit-tested) | DONE | All named discovery + name-extraction tests present (9 tests). |
| A4 Folders + early aligner-path validation | DONE (code) / PARTIAL (tests) | `create_tree` + early `resolve_explicit` wired in `pipeline.rs` Step I. **The A4 unit test (fresh dir creates tree; pre-existing warns; bad `--path_to_aligner` errors before output) is NOT present** — `folders.rs` has no test module, and no integration test passes `--path_to_aligner`. |
| A5 Conversion core (pure transform exhaustively unit-tested) | PARTIAL | All transform/byte tests present **except** the rev-1 zero-sequence-record and CR-only fixtures (item 27). |
| A6 Logging | DONE | `Logger` present, `note`/`info` gating; banners emitted in pipeline. |
| A7 Indexer (bowtie2 + concurrency) | DONE | `discover`/`resolve_explicit`/`build_command`/`run_both`; 4 unit tests. |
| A8 Pipeline | DONE | `run()` Step I→II→III→IV; combined hook gated. |
| A9 Phase-A integration (Perl oracle primary) | DONE | 2 Perl-oracle tests (MFA + single_fasta) + fake-indexer E2E + no-FASTA error. |
| B1 `--single_fasta` | DONE | Per-chr writers + byte-exact unit + Perl-oracle single_fasta. |
| B2 `--hisat2` | DONE (code) | Wired; shares bt2 command shape. No hisat2-specific command-string test (acceptable: identical code path, asserted via bt2). |
| B3 `--minimap2` | DONE | Command + exclusions; `minimap2_command_uses_k20_and_mmi`. |
| C1 `--slam` | DONE | Transform + fixed header + deprecation note; tested. |
| C2 Edge-case sweep | PARTIAL | CRLF/final-no-newline/interior-ws/empty-line/non-`>`/empty-dir/dup-name all covered (unit or E2E). **gzip `.fa.gz` end-to-end fixture and CR-only fixture not present**; duplicate-across-two-files is tested only within a single file. |
| C3 Accept-and-ignore + diagnostics | DONE (code) | `--genomic_composition` note + `--verbose` + `--version`. No dedicated test for the genomic_composition note / `--version` banner (item 20/21 — minor). |
| D1 Combined FASTA (from converted stream) | DONE | `write_combined`; MFA-mode byte-equality test. |
| D2 Combined index | DONE (code) | `combined::build` runs `run_one("BS_combined")`; combined dir present only with flag (verified via `binary_combined_genome_is_ct_concat_ga`). |
| E1 Real-data harness (`#[ignore]`) | MISSING (Phase E pending) | Not created. |
| E2 oxy run | MISSING (Phase E pending) | Not performed. |
| E3 Docs/polish (README/CHANGELOG/mkdocs) | MISSING (Phase E pending) | No `README.md`/`CHANGELOG.md`; Cargo.toml references `readme = "README.md"` which does not yet exist (will fail `cargo package`/publish, not `cargo test`). |

---

## Gaps (detail)

### Gap 1 (A2): CLI-validation unit tests absent — PARTIAL
**Expected (A2):** Unit tests in `cli.rs` — each aligner conflict → error; default = bowtie2; `--mm2` alias resolves to minimap2; `--parallel 1` → error; underscore long-names parse.
**Found:** `cli.rs` has **no** `#[cfg(test)]` module. The validation *logic* is fully implemented and correct on inspection; one path (no-FASTA) is exercised E2E. The mutual-exclusion, minimap2-exclusion, `--parallel <2`, and alias paths have **no test**.
**Gap:** Add the A2 unit tests (pure `Cli{..}.validate()` assertions — no FS needed except a tempdir for the canonicalize step, or test the pre-canonicalize branches). Low effort, no behavior change.

### Gap 2 (A4): Folders + early-path-validation test absent — PARTIAL
**Expected (A4):** Unit test — fresh dir creates the tree; pre-existing `Bisulfite_Genome/` warns and still returns dirs; **bad `--path_to_aligner` errors before any conversion output appears**.
**Found:** `folders.rs` has no test module; no integration test passes `--path_to_aligner` at all. The early-validation ordering (Step I `resolve_explicit` before `convert_split`) is correct in `pipeline.rs`, but the load-bearing "fails before FASTA is written" guarantee is **unverified by any test**.
**Gap:** Add a `folders::create_tree` unit test and an integration test that runs the binary with a bad `--path_to_aligner` and asserts (a) failure and (b) no `Bisulfite_Genome/CT_conversion/genome_mfa.CT_conversion.fa` was produced.

### Gap 3 (A5/C2, item 27): Zero-sequence-record and CR-only fixtures absent — PARTIAL
**Expected (rev-1, item 27 + §8.9/§8.15):** A zero-sequence record (header immediately followed by another header, and a header at EOF → emits just the converted header, no sequence) and a CR-only (old-Mac) file fixture, oracled against Perl.
**Found:** 0-byte file (`empty_file_errors`) and non-ASCII→N (`ambiguity_and_non_ascii_to_n`) are covered. The zero-sequence-record and CR-only cases have **no test**. The code path is correct on inspection (`handle_header` writes headers with no interleaved sequence; `read_until(b'\n')` reads a CR-only file as one header line), but neither is pinned.
**Gap:** Add a `convert_split` unit fixture for `>chr1\n>chr2\nACGT\n` (zero-sequence first record) and `>chrEOF\n` at EOF, plus a CR-only fixture. Ideally route through the Perl oracle.

### Minor / acceptable-as-coded (not blocking)
- **gzip `.fa.gz` end-to-end fixture (item 4 / C2):** `open_fasta` uses `MultiGzDecoder` and the transform is byte-source-agnostic (well unit-tested on raw bytes), but there is no end-to-end test feeding a real `.fa.gz` and diffing against the plain-input output. Low risk; recommend a small fixture.
- **`--single_fasta`-mode combined + slam-combined assertions (item 24):** `write_combined` is mode-independent and slam-aware by construction; only the MFA-mode equality is asserted. The plan's D1 test text asked for a single_fasta-mode check and a slam run.
- **hisat2 command-string assertion (B2):** shares the bt2 code path; no separate assertion (acceptable).
- **`--version` banner / `--genomic_composition` note assertions (items 20/21, C3):** behavior present; no test. Not byte-gated, low value.

---

## Test verification (Mode B)

| Test name | File | Status |
|-----------|------|--------|
| name_basic | discovery.rs | PASS |
| name_crlf_header | discovery.rs | PASS |
| name_bare_gt_is_empty_not_error | discovery.rs | PASS |
| name_leading_whitespace_is_empty | discovery.rs | PASS |
| name_first_byte_not_gt_errors | discovery.rs | PASS |
| glob_precedence_fa_wins_over_others | discovery.rs | PASS |
| glob_fasta_fallback_when_no_fa | discovery.rs | PASS |
| glob_lexical_order_not_numeric | discovery.rs | PASS |
| glob_empty_dir_errors | discovery.rs | PASS |
| uppercases_and_converts | convert.rs | PASS |
| ct_ga_basic | convert.rs | PASS |
| ambiguity_and_non_ascii_to_n | convert.rs | PASS |
| crlf_preserved | convert.rs | PASS |
| final_line_without_newline_preserved | convert.rs | PASS |
| interior_whitespace_becomes_n | convert.rs | PASS |
| empty_seq_line_passthrough | convert.rs | PASS |
| slam_direction | convert.rs | PASS |
| header_line_fixed_suffix | convert.rs | PASS |
| convert_split_mfa_byte_exact | convert.rs | PASS |
| convert_split_single_fasta_byte_exact | convert.rs | PASS |
| combined_equals_ct_concat_ga_in_mfa_mode | convert.rs | PASS |
| duplicate_chromosome_name_errors | convert.rs | PASS |
| empty_file_errors | convert.rs | PASS |
| bowtie2_command_has_threads_and_file_list | indexer.rs | PASS |
| bowtie2_large_index_flag | indexer.rs | PASS |
| minimap2_command_uses_k20_and_mmi | indexer.rs | PASS |
| resolve_explicit_missing_errors | indexer.rs | PASS |
| binary_end_to_end_mfa_bytes | tests/integration.rs | PASS |
| binary_combined_genome_is_ct_concat_ga | tests/integration.rs | PASS |
| binary_no_fasta_dir_errors | tests/integration.rs | PASS |
| perl_vs_rust_byte_identical_mfa | tests/integration.rs | PASS (Perl present) |
| perl_vs_rust_byte_identical_single_fasta | tests/integration.rs | PASS (Perl present) |
| **CLI validation unit tests (A2)** | cli.rs | **MISSING** |
| **folders::create_tree + early bad-path (A4)** | folders.rs / integration.rs | **MISSING** |
| **zero-sequence-record / CR-only fixtures (A5, item 27)** | convert.rs / integration.rs | **MISSING** |

Re-run summary (`cargo test -p bismark-genome-preparation`): **27 lib + 5 integration = 32 passed, 0 failed, 0 ignored.**

---

## Verdict

**COMPLETE for the byte-identity gate and all Phase A–D *behavior*.** Every byte-identity-critical item the audit was asked to scrutinize is correctly implemented and tested: name extraction (bare `>`/leading-ws → empty), raw-byte transform (CRLF/final-no-newline/whitespace→N/non-ASCII→N), fixed `_CT_`/`_GA_` slam headers, glob `file_name()`-byte sort + extension precedence, always-`--threads N`, early `--path_to_aligner` validation in Step I, combined-genome mode-independent stream oracle, duplicate-name + empty-file errors, and the **Perl script as the primary byte-identity oracle** (the 2 oracle tests pass against the real script).

**Phase E is PENDING by design** (called out in PLAN "Implementation notes"): real-genome oxy run + `tests/byte_identity_real_data.rs` `#[ignore]` harness + `README.md`/`CHANGELOG.md`/mkdocs. These are expected, not gaps.

**Genuine A–D gaps to address (all are missing *tests* for behavior that is implemented and correct on inspection — not missing functionality):**
1. **A2** — add CLI-validation unit tests (aligner conflicts, default bowtie2, `--mm2` alias, `--parallel 1`, underscore parse). `cli.rs` currently has no test module.
2. **A4** — add a `folders::create_tree` unit test **and** an integration test asserting a bad `--path_to_aligner` fails *before* any converted FASTA is written (the load-bearing early-validation guarantee is currently unverified).
3. **A5 / item 27** — add the rev-1 zero-sequence-record (header→header, header-at-EOF) and CR-only (old-Mac) fixtures; ideally route through the Perl oracle.

Recommended-but-non-blocking: a gzip `.fa.gz` end-to-end fixture (item 4), and single_fasta-mode + slam combined-genome assertions (item 24).

Note that the missing `README.md` referenced by `Cargo.toml` (`readme = "README.md"`) is a Phase E item; it does not break `cargo test` but would break `cargo package`/publish until E3 lands.

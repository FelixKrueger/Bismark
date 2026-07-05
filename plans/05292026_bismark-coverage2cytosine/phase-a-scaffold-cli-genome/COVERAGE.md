# Plan Coverage Report

**Mode:** B (code vs. implementation plan)
**Plan(s):** `phase-a-scaffold-cli-genome/IMPL.md` (Tasks T1–T10 + 30-item Plan Coverage Checklist), cross-referenced against sibling `PLAN.md` (rev 1) §3/§5/§3.3 + V1–V17.
**Date:** 2026-05-29
**Verdict:** COMPLETE

## Summary

- Total ledger items: 30 checklist items + 10 tasks (T1–T10) + 17 design-validation rows (V1–V17) verified.
- DONE: 30 / 30 checklist items; 10 / 10 tasks; 17 / 17 V-rows mapped to passing tests.
- PARTIAL: 0
- MISSING: 0
- DEVIATED (documented): 3 — (a) `#[derive(Debug)]` on `Genome`; (b) `type FastaRecords` alias; (c) `cov_infile` modelled as `Option<PathBuf>` + a `MissingCovInput` validate gate (additive, not in the 30-item list but specified in PLAN §3.1/§3.2 step 2 and §5). All three are documented in the plan; none are silent.

Tests: `cargo test -p bismark-coverage2cytosine` → **40 passed / 0 failed** (35 unit + 5 integration + 0 doc).
`cargo clippy -p bismark-coverage2cytosine --all-targets -- -D warnings` → **clean** (no warnings).

## Coverage ledger (30-item checklist)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | Workspace member crate (lib+bin `coverage2cytosine_rs`) | T1 / §1,§2 | DONE | `rust/Cargo.toml` members line includes `bismark-coverage2cytosine`; `Cargo.toml` `[lib]` + `[[bin]] name="coverage2cytosine_rs"`. |
| 2 | `version_string()` TG-style provenance; `--version`/`-V` | T1 / §3.1,§4 | DONE | `lib.rs:40` `version_string()`; `main.rs:23` handles `cli.version`; sanity tests `version_output_matches_provenance_regex`, `short_version_flag_works_too` pass. |
| 3 | `#![forbid(unsafe_code)]`, `#![warn(missing_docs)]` | T1 / §8.4 | DONE | `lib.rs:25-26`. |
| 4 | `BismarkC2cError` enum (all Phase-A variants) | T2 / §5 | DONE | `error.rs:11-111`; all 14 §5 variants present (incl. `MalformedFastaHeader`, plus `MissingCovInput`). |
| 5 | `Cli` struct — all flags incl. positional cov infile | T3 / §3.1 | DONE | `cli.rs:30-100`; every §3.1 row present. |
| 6 | `--CX_context` + `visible_alias CX`; `-CX` NOT a short | T3 / §3.1 (rev1) | DONE | `cli.rs:56`; tests `cx_long_and_alias_both_parse`, `dash_cx_is_not_a_valid_short`. |
| 7 | v1.x flags declared but rejected (`--gc/--nome-seq/--drach/--ffs`) | T3,T4 / §3.1,§3.2.2 | DONE | `cli.rs:88-99` declared (gc has `GC/GC_context/gc_context` aliases); `cli.rs:144-155` rejects in gc→nome→drach→ffs order; test `rejects_v1x_flags`. |
| 8 | `disable_version_flag`; clap `debug_assert` valid | T3 / §3.1 | DONE | `cli.rs:28`; test `clap_definition_is_valid` calls `Cli::command().debug_assert()`. |
| 9 | Reject missing `-o` → `MissingOutput` | T4 / §3.2.3 | DONE | `cli.rs:158`; test `rejects_missing_output`. |
| 10 | Reject missing `-g` → `MissingGenomeFolder` | T4 / §3.2.4 | DONE | `cli.rs:159-161`; test `rejects_missing_genome`. |
| 11 | `merge_cpgs`+`cx`/+`split`/+`threshold` mutexes | T4 / §3.2.5-7 | DONE | `cli.rs:163-171`; tests `rejects_merge_with_cx/split/threshold`. |
| 12 | `discordance` without merge; range 1..=100 | T4 / §3.2.8-9 | DONE | `cli.rs:172-179`; tests `rejects_discordance_without_merge`, `rejects_discordance_out_of_range` (0 and 101), `accepts_discordance_in_range`. |
| 13 | `threshold == Some(0)` is error; `None`⇒0 report-all | T4 / §3.2.10 | DONE | `cli.rs:180-182` (error); `cli.rs:186` `unwrap_or(0)`; tests `rejects_threshold_zero`, `threshold_none_defaults_zero`. |
| 14 | `cpg_only = !cx_context` coupling | T5 / §3.2.11 | DONE | `cli.rs:185`; test `cpg_only_coupling`. |
| 15 | **C1** context-conditional `output_stem` strip | T5 / §3.2.11 (rev1) | DONE | `cli.rs:188-198` selects `.CX_report.txt` iff cx else `.CpG_report.txt`, strips exactly one; test `output_stem_strip_is_context_conditional` asserts all 5 cross cases incl. the byte-identity traps. |
| 16 | **C2** `output_dir=""` vs `parent_dir=getcwd()`; `--dir`→abs+trailing `/` | T5 / §3.2.11 (rev1) | DONE | `cli.rs:200-205` + `resolve_output_dir` (`cli.rs:230-242`); tests `dir_defaults_are_split`, `given_dir_is_absolute_with_trailing_slash`. |
| 17 | Genome load: multi-FASTA, name=first-token, uppercase | T6 / §3.3.3-5 | DONE | `genome.rs:178-195` (uppercase via `to_ascii_uppercase`, `record.name()` first-token); test `loads_multifasta_first_token_name_and_uppercases`. |
| 18 | `Genome` API: get/contains/names_sorted/len; **no public order iterator** | T6 / §3.3 (rev1 invariant) | DONE | `genome.rs:87-117`; `chromosomes` field private; NO `iter()`/`keys()` passthrough — invariant honoured; test `names_sorted_is_bytewise`. |
| 19 | Four-suffix glob **priority** (first tier with ≥1 filename wins) | T7 / §3.3.1 | DONE | `discover_fasta_files` (`genome.rs:121-147`) iterates `FASTA_TIERS` and returns first non-empty tier (decided by filename match, no union); test `glob_priority_fa_beats_fa_gz`. |
| 20 | `Mus_musculus.NCBIM37.fa` skip; Mus-only tier⇒empty,no error | T7 / §3.3.2 | DONE | `genome.rs:68` skip inside per-file loop (after tier chosen); tests `mus_only_tier_yields_empty_genome_no_error`, `mus_skipped_among_others`. |
| 21 | All-four-globs-empty ⇒ `NoGenomeFasta` | T7 / §3.3.1 | DONE | `genome.rs:144-146`; test `no_fasta_anywhere_errors`. |
| 22 | `.fa.gz`/`.fasta.gz` via `MultiGzDecoder` (plain + BGZF) | T8 / §3.3.3 (rev1) | DONE | `genome.rs:161-168` `MultiGzDecoder` for `.gz`; tests `loads_plain_gzip_fa_gz`, `loads_bgzf_fa_gz`. |
| 23 | Duplicate name (incl. cross-file) ⇒ `DuplicateChromosomeName` | T9 / §3.3.6 | DONE | `genome.rs:72-76` `HashSet` seen-name guard; test `duplicate_name_cross_file_errors` (two files, same `chr1`). |
| 24 | Malformed/empty file in winning tier ⇒ `MalformedFastaHeader` | T9 / §3.3.2a (rev1) | DONE | `genome.rs:170-175` (zero-record file) + `genome.rs:182-184` (noodles parse error); tests `empty_file_in_winning_tier_errors`, `headerless_file_errors`. |
| 25 | CRLF handled (noodles auto-strip locked by test) | T9 / §3.3.5 (rev1) | DONE | relies on noodles; test `crlf_sequence_has_no_carriage_return` locks it (asserts no `\r`). |
| 26 | Empty-sequence record kept | T9 / §3.3.5 | DONE | stored as empty `Vec` (`genome.rs:186-192` collects whatever sequence noodles yields); test `empty_sequence_record_kept`. |
| 27 | `u32` length overflow guard ⇒ `ChromosomeTooLong` | T9 / §3.3.7 | DONE | `check_chr_len` (`genome.rs:198-206`) called at `genome.rs:77`; test `u32_overflow_guard_helper` exercises `u32::MAX+1` (helper-level, real 4 GiB fixture infeasible). |
| 28 | `--help` lists v1.0 flags | T3 / §9 V2 | DONE | sanity test `help_lists_v1_flags` asserts `--merge_CpGs`, `--CX_context`, `--split_by_chromosome`. |
| 29 | Dep pins + add to workspace `members` | T1 / §2 | DONE | `Cargo.toml` pins clap=4.5.30, thiserror=2.0.0, noodles-fasta=0.61.0, noodles-core=0.20.0, flate2=1.1.9, dev: assert_cmd/predicates/tempfile/bstr/noodles-bgzf=0.47.0; `rust/Cargo.toml` members updated. |
| 30 | Final clippy/fmt clean + regression sweep | T10 / §9 V1 | DONE | clippy `-D warnings` clean; full suite 40/40 green; workspace builds. |

## Task-level ledger (T1–T10)

| Task | Goal | Status | Notes |
|------|------|--------|-------|
| T1 | Crate scaffold, workspace member, `--version` sanity | DONE | Cargo.toml (lib+bin), `rust/Cargo.toml` member, `lib.rs` version_string + forbid(unsafe)/warn(missing_docs), `main.rs` ExitCode map (0/1/clap-2), sanity version tests. |
| T2 | `BismarkC2cError` enum | DONE | `error.rs` — all variants incl. `MalformedFastaHeader`; test `error_display_strings_present`. |
| T3 | `Cli` struct (parse, --help, --CX alias, clap valid) | DONE | `cli.rs:30-100`; 4 parse tests + sanity help test. |
| T4 | `validate()` rejections (mutexes, ranges, v1.x, required) | DONE | `cli.rs:142-182`, priority order matches §3.2; 11 rejection tests. |
| T5 | `validate()` resolution (C1 stem strip, C2 dir defaults, cpg_only) | DONE | `cli.rs:184-221`; 5 resolution tests incl. context-conditional stem. |
| T6 | `genome.rs` reader: multi-FASTA, names, uppercase, accessors | DONE | `Genome` + `load`/`get`/`contains`/`names_sorted`/`len`/`is_empty`, no public iterator. |
| T7 | glob priority + Mus skip + empty-dir error | DONE | `discover_fasta_files`; 4 tests. |
| T8 | gz FASTA support (plain gzip + BGZF) via `MultiGzDecoder` | DONE | open helper branches on `.gz`; 2 tests; noodles-bgzf dev-dep added. |
| T9 | edge cases: dup, malformed/empty, CRLF, empty-seq, u32 guard | DONE | all 5 guards present and individually tested. |
| T10 | Final verification + integration sanity | DONE | clippy clean, 40/40 tests, missing-`-o` exit-code sanity test present (`missing_output_fails_with_clear_message`, code 1). |

## Design-validation rows (V1–V17 → test mapping)

| V | Requirement | Test function | Status |
|---|-------------|---------------|--------|
| V1 | Builds + lints clean | `cargo build/clippy/fmt` (run by auditor) | PASS |
| V2 | `--help` v1.0 flags; `--version` provenance | `help_lists_v1_flags`, `version_output_matches_provenance_regex` | PASS |
| V3 | clap definition valid | `cli::tests::clap_definition_is_valid` | PASS |
| V4 | Each validation rule fires | `rejects_missing_output/genome`, `rejects_merge_with_cx/split/threshold`, `rejects_discordance_without_merge`, `rejects_discordance_out_of_range`, `rejects_threshold_zero` | PASS |
| V5 | v1.x flags rejected | `cli::tests::rejects_v1x_flags` | PASS |
| V6 | `cpg_only` coupling | `cli::tests::cpg_only_coupling` | PASS |
| V7 | Context-conditional stem strip (C1) | `cli::tests::output_stem_strip_is_context_conditional` (5 cross cases) | PASS |
| V7b | `output_dir`/`parent_dir` defaults (C2) | `dir_defaults_are_split`, `given_dir_is_absolute_with_trailing_slash` | PASS |
| V8 | Glob priority (.fa beats .fa.gz) | `genome::tests::glob_priority_fa_beats_fa_gz` | PASS |
| V9 | Mus skip among others | `genome::tests::mus_skipped_among_others` | PASS |
| V9b | Mus-only tier ⇒ empty genome, no error | `genome::tests::mus_only_tier_yields_empty_genome_no_error` | PASS |
| V10 | Uppercase | covered by `loads_multifasta_first_token_name_and_uppercases` (lowercase `acgt`→`ACGT`) | PASS |
| V11 | Multi-FASTA + first-token name + description drop | `loads_multifasta_first_token_name_and_uppercases` (`>chr1 some description`) | PASS |
| V11b | Cross-file duplicate name → error | `genome::tests::duplicate_name_cross_file_errors` | PASS |
| V11c | Malformed/empty file in winning tier → error | `empty_file_in_winning_tier_errors`, `headerless_file_errors` | PASS |
| V12 | `.fa.gz` plain-gzip load | `genome::tests::loads_plain_gzip_fa_gz` | PASS |
| V12b | `.fa.gz` BGZF load | `genome::tests::loads_bgzf_fa_gz` | PASS |
| V13 | `names_sorted` bytewise order | `genome::tests::names_sorted_is_bytewise` (chr10 < chr2) | PASS |
| V14 | Empty genome dir → `NoGenomeFasta` | `genome::tests::no_fasta_anywhere_errors` | PASS |
| V15 | `--CX` alias = `--CX_context`; `-CX` not a short | `cx_long_and_alias_both_parse`, `dash_cx_is_not_a_valid_short` | PASS |
| V16 | CRLF handled | `genome::tests::crlf_sequence_has_no_carriage_return` | PASS |
| V17 | Empty-sequence record kept | `genome::tests::empty_sequence_record_kept` | PASS |

## Documented deviations (DEVIATED-but-documented, not silent)

### D-a: `#[derive(Debug)]` on `Genome`
**Expected (plan body):** plain `pub struct Genome { chromosomes: HashMap<...> }`.
**Found:** `genome.rs:44` adds `#[derive(Debug)]`.
**Status:** DEVIATED — documented in `PLAN.md` Implementation-notes iteration log #1 ("`Genome::unwrap_err()` in tests needs `Genome: Debug`"). Mechanical, necessary for `.unwrap_err()` in tests. Not silent.

### D-b: `type FastaRecords` alias
**Expected (plan T9 refactor):** `read_one_fasta(path) -> Result<Vec<(Vec<u8>,Vec<u8>)>, _>`.
**Found:** `genome.rs:30` `type FastaRecords = Vec<(Vec<u8>, Vec<u8>)>;`, used in `read_one_fasta`/`collect_records`.
**Status:** DEVIATED — documented in `PLAN.md` iteration log #2 (clippy `type_complexity`). Mechanical. Not silent.

### D-c: `cov_infile` as `Option<PathBuf>` + `MissingCovInput`
**Expected (IMPL T3 prose):** "`cov_infile: PathBuf` (positional, required)".
**Found:** `cli.rs:33` `cov_infile: Option<PathBuf>`; validated via `MissingCovInput` (`cli.rs:157`, `error.rs:21`); test `rejects_missing_cov_infile`.
**Status:** DEVIATED — documented and consistent with `PLAN.md` §3.2 step 2 ("missing infile → `MissingCovInput`") and §5 (the variant is listed). The design plan explicitly resolves the infile to a `validate()` rejection (so missing infile yields a clear `BismarkC2cError` exit-1 rather than a clap exit-2). The IMPL T3 "required" wording is the looser of the two; the implementation follows PLAN §3.2/§5. Not silent; behaviour is tested. (Note: `MissingCovInput` is an additive variant beyond the 30-item checklist's enumerated errors, but it is enumerated in PLAN §5? — §5 lists it as "missing infile"; the IMPL §5/T2 list does not name it explicitly, yet the behaviour is required by PLAN §3.2 step 2. Treated as documented-and-required, not a gap.)

## Test verification

| Suite | Command | Result |
|-------|---------|--------|
| Unit + integration + doc | `cargo test -p bismark-coverage2cytosine` | 40 passed / 0 failed (35 unit + 5 integration + 0 doc) |
| Lint | `cargo clippy -p bismark-coverage2cytosine --all-targets -- -D warnings` | clean (no warnings) |

(Run from worktree `/Users/fkrueger/Github/Bismark-c2c/rust` via `--manifest-path`, sandbox disabled — worktree is outside the sandbox-writable root.)

## Verdict

**COMPLETE.** All 30 checklist items, all 10 tasks (T1–T10), and all 17 design-validation rows (V1–V17) are implemented and covered by passing tests. The full suite is green (40/40) and clippy is clean under `-D warnings`. The three deviations (Debug derive, FastaRecords alias, Option-typed cov_infile + MissingCovInput) are all documented in the plan's implementation notes and/or design body — none are silent. No items unresolved.

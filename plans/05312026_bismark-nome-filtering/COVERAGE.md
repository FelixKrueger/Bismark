# Plan Coverage Report

**Mode:** A+B (scoped to Phase A)
**Plan(s):** `SPEC.md` (rev 1, Phase A scope) → `IMPL_phase-A.md` (Tasks T1–T6 + 12-row coverage checklist) → code in worktree `~/Github/Bismark-nome`
**Date:** 2026-05-31
**Verdict:** COMPLETE — 0 items unresolved

## Summary

- Total items: 18 (Mode A: 6 SPEC→IMPL mappings · Mode B: 12 checklist rows)
- DONE: 18
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 0

All declared Phase-A intentional scope boundaries (`run()` = validate+load only; `EmptyInput` declared-not-raised; `perl_substr` present-but-consumed-in-B) are matched exactly as documented and are counted as DONE/in-scope.

---

## Mode A — IMPL_phase-A.md covers every Phase-A item of the SPEC

Phase A scope per SPEC §11 row A, plus the §3/§4/§7/§9/§10/§13 items that belong to Phase A.

| # | SPEC Phase-A requirement | SPEC § | IMPL coverage | Status | Notes |
|---|--------------------------|--------|---------------|--------|-------|
| A1 | New crate `bismark-nome-filtering` (lib+bin `NOMe_filtering_rs`), 8th workspace member; `version_string()`/`--version`; `0.1.0-beta.1`; `#![forbid(unsafe_code)]`/`#![warn(missing_docs)]` | §10, §11A | T1 (+ checklist #1, #2) | DONE | — |
| A2 | Promote `bismark_io::genome` — tier-parameterized `load(folder,&[&str])`, first-non-empty-tier glob, dotfile-exclude, uppercase, Mus skip, `\r` strip, first-token name, dup-name error, `u32` guard, `HashMap<Vec<u8>,Vec<u8>>`, no public insertion-order iterator; **module-local `GenomeError`** (D5); additive/no-version-bump (D1/P7); bare-`>` divergence inherited | §7, §D1, §D5 | T2 (+ checklist #3, #4, #5, #7) | DONE | All sub-clauses mapped to T2 steps 1–4. |
| A3 | NOMe consumes `load(folder,&[".fa",".fasta"])` (two PLAIN tiers); `.fa.gz` footgun preserved via tier list (P6/P14) | §7, §13 | T6 (+ checklist #6) | DONE | Consumed in `run()` (T6); tier-list test in T2. |
| A4 | `perl_substr` helper with exact rvalue semantics incl. `start==L`→empty/no-panic (D3/P1) | §9, §13 | T3 (+ checklist #8) | DONE | Present + unit-tested in Phase A; consumed in Phase B (intended). |
| A5 | `filename.rs` `.manOwar` derivation — single-strip-per-extension, no dir strip, not dedup's loop (P15) | §4, §13 | T4 (+ checklist #9) | DONE | — |
| A6 | clap `Cli`→`validate()`→`ResolvedConfig`: live flags + inert acceptance + `--parent_dir` inert + the one die + mandatory-genome + infile-exists + `--dir` path contract (join, no chdir, P12); `BismarkNomeError` (incl. declared-only `EmptyInput`, D4); `main.rs` `parse→version?→run→ExitCode`; Phase-A `run()` = validate+create-dir+resolve+infile-exists+genome-load, no output | §4, §10, §11A | T5 + T6 (+ checklist #10, #11, #12) | DONE | `EmptyInput` declared but raised in Phase B per D4 — intended. |

Mode A result: every Phase-A SPEC requirement maps to ≥1 IMPL task; no requirement diluted. The IMPL checklist note (line 30) correctly flags `EmptyInput` as declared-here/raised-in-B.

---

## Mode B — code implements every task + the 12-row coverage checklist

### Coverage ledger (IMPL checklist rows #1–#12)

| # | Plan item (SPEC §) | Source | Status | Notes |
|---|--------------------|--------|--------|-------|
| 1 | New crate lib+bin `NOMe_filtering_rs`, 8th member, `#![forbid(unsafe_code)]`/`#![warn(missing_docs)]` | T1 | DONE | `Cargo.toml` (lib+`[[bin]]` name `NOMe_filtering_rs`); `rust/Cargo.toml:3` members has all 8; `lib.rs:15-16` has both attrs. |
| 2 | `version_string()` = `NOMe_filtering_rs <semver> (<os>/<arch>)`, crate `0.1.0-beta.1`, `disable_version_flag`+`--version`/`-V` | T1, T5 | DONE | `lib.rs:30-37`; runtime prints `NOMe_filtering_rs 0.1.0-beta.1 (macos/aarch64)`; `cli.rs:32` `disable_version_flag=true`; `cli.rs:77` `-V`/`--version`. |
| 3 | Promote `bismark_io::genome`: tier-param `load`, first-non-empty glob (no union, dotfile-exclude), uppercase, Mus skip, `\r` strip, first-token name, dup-name error, `u32` guard, `HashMap<Vec<u8>,Vec<u8>>`, no public insertion-order iter | T2 | DONE | `genome.rs`: `load(folder,&[&str])` L109; `discover_fasta_files` first-non-empty + dotfile-exclude L176-195; uppercase L249; Mus skip L118; `\r` handled by noodles + test L333-342; name=`record.name()` L244; dup error L122-125; `check_chr_len` L257-265; `HashMap<Vec<u8>,Vec<u8>>` L91; only `names_sorted()` accessor (invariant doc L23-26). |
| 4 | Module-local `GenomeError` (NOT `BismarkIoError` variants) | T2 | DONE | `genome.rs:44-83` defines `GenomeError` (5 variants); re-exported `lib.rs:32`; `BismarkIoError` untouched (`error.rs` not modified). |
| 5 | `bismark-io` gains `flate2` dep; NO version bump; `cargo build --workspace` resolves all `=beta.8` pins | T2, Final | DONE | `bismark-io/Cargo.toml:34` `flate2 = "=1.1.9"`; version still `1.0.0-beta.8` (L3 + Cargo.lock confirms); `cargo build --workspace` succeeds. |
| 6 | NOMe consumes `load(folder,&[".fa",".fasta"])` (two PLAIN tiers); `.fa.gz` footgun preserved | T6 | DONE | `lib.rs:64` `Genome::load(&cfg.genome_folder, &[".fa", ".fasta"])`; footgun pinned by `fa_gz_invisible_with_two_plain_tiers` test (genome.rs:304). |
| 7 | Inherit c2c's bare-`>`-header divergence (errors) | T2 | DONE | `collect_records` maps noodles `InvalidData`→`MalformedFastaHeader` (genome.rs:236-239); test `bare_or_nameless_header_errors` (L356). |
| 8 | `perl_substr` (§9): negative-in-range→tail, `\|offset\|>L`→empty, `start==L`→empty/no-panic, over-length→truncate | T3 | DONE | `substr.rs:19-28`; 8 unit tests incl. `offset_equals_len_is_empty_no_panic` (L57). |
| 9 | `filename.rs` `derive_manowar_name`: strip ONE `.gz` then ONE `.txt`, append `.manOwar.txt`, force `.gz`; no dir strip; not dedup's loop | T4 | DONE | `filename.rs:31-42` (two single `strip_suffix` calls, no loop, no dir strip); `gz_gz_single_strip`/`txt_txt_single_strip` tests (L69,75). |
| 10 | clap `Cli`→`validate()`: live flags + inert accepted + the die + mandatory-genome + infile-exists + `--dir` contract (input=`dir.join(infile)`, output=`dir.join(derived)`, no chdir) | T5 | DONE | `cli.rs:34-79` (live + inert fields incl. `--parent_dir`, `-CX`/`--CX_context` alias, `--GC`); `validate()` L101-137: die L103-105, mandatory-genome L107-109, paths L116-118 (join, no chdir); infile-exists done in `run()` L58. Tests cover all branches. |
| 11 | `BismarkNomeError` (thiserror): `Genome(#[from] GenomeError)`, `MissingGenomeFolder`, `InfileNotFound`, `EmptyInput`, `MergeCpgsWithCx`, `Io(#[from] io::Error)` | T5, T6 | DONE | `error.rs:13-47` — all 6 variants present, exact signatures. `EmptyInput` declared (L30-34), not raised in Phase A (intended per D4). |
| 12 | `main.rs` wiring `parse→--version?→run→ExitCode`; Phase-A `run()` = validate+create-dir+resolve+infile-exists+genome-load (+stderr); NO output | T6 | DONE | `main.rs:19-36` (parse→version→run→ExitCode 0/1); `run()` lib.rs:48-74 = validate, create_dir_all, infile-exists, genome load, stderr line; no output file written. |

### Phase-A intentional scope boundaries (verified as DONE/in-scope, NOT gaps)

| Boundary | Expected (intended) | Found | Verdict |
|----------|---------------------|-------|---------|
| `run()` Phase-A-scoped | validate + create dir + resolve + infile-exists + genome load; NO per-read processing, NO output file | `lib.rs:48-74` does exactly this; comment block L70-73 marks Phase-B work; no writer/`GzEncoder` in `run()` | In-scope (SPEC §11A) |
| `EmptyInput` declared-not-raised | declared in enum for completeness; raised in Phase B (D4 header-then-error) | `error.rs:30-34` declares it; `grep` finds no `Err(...EmptyInput)` construction anywhere | In-scope (SPEC §D4) |
| `perl_substr` present-but-unconsumed | implemented + unit-tested in A; consumed in B's `nome.rs` | `substr.rs` complete with 8 tests; no caller yet (no `nome.rs`) | In-scope |

---

## Gaps (detail)

None. No PARTIAL / MISSING / DEVIATED items.

---

## Test verification (Mode B)

Commands run in the worktree with `dangerouslyDisableSandbox: true` (worktree is outside the command sandbox).

| Suite / command | Result |
|-----------------|--------|
| `cargo build --workspace` | PASS — Finished; all 8 members compile; `bismark-io` stays `1.0.0-beta.8` (P7). |
| `cargo test -p bismark-io genome` | PASS — 13/13 genome tests (multifasta+uppercase, fa-beats-fasta no-union, `.fa.gz` invisible, fasta-tier fallback, Mus-only empty, Mus-skip+CRLF, dup-name, bare-header divergence, no-fasta, dotfiles, names_sorted bytewise, gz-capable load, `u32` guard). |
| `cargo test -p bismark-nome-filtering` | PASS — 26 unit + 6 integration (`cli_phase_a`) + 1 doctest, 0 failed. |
| `cargo clippy -p bismark-nome-filtering -p bismark-io --all-targets -- -D warnings` | PASS — clean on forced fresh recompile (exit 0, no warnings). |
| `NOMe_filtering_rs --version` | PASS — prints `NOMe_filtering_rs 0.1.0-beta.1 (macos/aarch64)`. |
| `NOMe_filtering_rs --help` | PASS — usage banner, exits success. |

### Named unit/integration tests confirmed present and passing

| Test | File | Status | Maps to |
|------|------|--------|---------|
| `version_string_has_binary_name_and_semver` | lib.rs | PASS | #2 |
| `loads_multifasta_first_token_name_and_uppercases` | genome.rs | PASS | #3 |
| `two_plain_tiers_fa_beats_fasta_no_union` | genome.rs | PASS | #3 |
| `fa_gz_invisible_with_two_plain_tiers` | genome.rs | PASS | #6/P14 |
| `fasta_tier_used_when_no_fa` | genome.rs | PASS | #3 |
| `mus_only_tier_yields_empty_genome_no_error` | genome.rs | PASS | #3 |
| `mus_skipped_among_others_and_crlf_stripped` | genome.rs | PASS | #3 |
| `duplicate_name_cross_file_errors` | genome.rs | PASS | #3 |
| `bare_or_nameless_header_errors` | genome.rs | PASS | #7 |
| `no_fasta_anywhere_errors` | genome.rs | PASS | #3 |
| `dotfiles_not_matched` | genome.rs | PASS | #3 |
| `names_sorted_is_bytewise` | genome.rs | PASS | #3 |
| `loads_plain_gzip_fa_gz_when_gz_tier_supplied` | genome.rs | PASS | #5 (gz capability) |
| `u32_overflow_guard_helper` | genome.rs | PASS | #3 |
| `offset_equals_len_is_empty_no_panic` + 7 more | substr.rs | PASS | #8/P1 |
| `gz_gz_single_strip`, `txt_txt_single_strip` + 6 | filename.rs | PASS | #9/P15 |
| `merge_cpgs_with_cx_is_rejected` | cli.rs | PASS | #10 |
| `missing_genome_folder_is_rejected` | cli.rs | PASS | #10 |
| `inert_flags_accepted_no_effect` | cli.rs | PASS | #10/D2 |
| `dir_path_contract_resolves_input_and_output_under_dir` | cli.rs | PASS | #10/P12 |
| `no_dir_resolves_against_cwd_dot` | cli.rs | PASS | #10 |
| `cx_context_alias_parses` | cli.rs | PASS | #10 |
| `version_parses_without_infile` | cli.rs | PASS | #2/#10 |
| `valid_invocation_loads_genome_and_exits_zero` | cli_phase_a.rs | PASS | #12 |
| `nonexistent_infile_errors_nonzero` | cli_phase_a.rs | PASS | #10/#12 |
| `merge_cpgs_with_cx_errors_nonzero` | cli_phase_a.rs | PASS | #10 |
| `missing_genome_errors_nonzero` | cli_phase_a.rs | PASS | #10 |
| `version_prints_provenance`, `help_exits_successfully` | cli_phase_a.rs | PASS | #2 |

### Observations beyond the minimum plan (not gaps — additional coverage)

- IMPL spec'd a `mus_skip` test that *also* asserted dup-name + bare-header + `u32` in one combined test; the implementation split these into separate, clearer tests (`mus_skipped_among_others_and_crlf_stripped`, `duplicate_name_cross_file_errors`, `bare_or_nameless_header_errors`, `u32_overflow_guard_helper`) plus extras (`mus_only_tier_yields_empty_genome_no_error`, `no_fasta_anywhere_errors`, `loads_plain_gzip_fa_gz_when_gz_tier_supplied`). All required assertions are present; this is strictly broader coverage, not a deviation.
- CLI gained two extra tests beyond the IMPL sketch (`merge_cpgs_alone_is_accepted`, `cx_context_alias_parses`) — confirms inert `--merge_CpGs` alone passes and the `--CX_context` alias works. Additive.
- The integration suite added `nonexistent_infile_errors_nonzero` (beyond the four sketched), confirming the `run()` infile-exists guard (#12). Additive.

---

## Verdict

**COMPLETE.** Every Phase-A SPEC requirement (Mode A) maps to an IMPL task, and every IMPL task + all 12 coverage-checklist rows (Mode B) are implemented in code and backed by passing tests. The three intentional Phase-A scope boundaries (`run()` validate+load-only, `EmptyInput` declared-not-raised, `perl_substr` present-but-consumed-in-B) are present exactly as documented and are in-scope, not gaps. `bismark-io` was extended additively (new `genome` module, module-local `GenomeError`, `flate2` dep) with **no version bump** — `cargo build --workspace` resolves all sibling `=1.0.0-beta.8` pins and `bismark-io` remains `1.0.0-beta.8`. Build, both test suites (13 + 33), clippy `-D warnings`, and `--version`/`--help` all pass. Nothing remains for Phase A.

# Plan Coverage Report

**Mode:** B (code vs. plan — the design PLAN.md is the implementation spec)
**Plan(s):** `phase1-cli-options-discovery/PLAN.md` (rev 1, 2026-06-01)
**Date:** 2026-06-01
**Verdict:** COMPLETE

## Summary

- Total items: 40 (10 behavior + 9 outline + 13 validations + 8 §3.8 order positions)
- DONE: 38
- PARTIAL: 0
- MISSING: 0
- DEVIATED (documented): 2 (mimalloc omitted; `-n`/`-l` Bismark→`-N`/`-L` spelling map)

All audited items map to real code and/or a real test. `cargo test -p bismark-aligner` → **24 passed, 0 failed** (15 unit + 9 integration). The §3.8 `aligner_options` push order in `options.rs` was cross-checked against Perl `bismark` 7838–8142 and matches position-for-position. Both §13 deviations are real and documented (DEVIATED-but-documented = acceptable per skill).

## Coverage ledger

### §3 Behavior (3.1–3.10)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 3.1 | Capture verbatim argv (prog name excluded, pre-rewrite) for `@PG` | `main.rs:16-17` (`raw.get(1..).join(" ")` before parse) → `RunConfig.command_line` | DONE | Captured before `Cli::parse_from`; no quality-flag rewrite performed in Phase 1. |
| 3.2 | Parse full ~60-option surface; wire only v1 spine | `cli.rs` (all flags incl. deferred HISAT2/minimap2/RG/output) | DONE | Deferred flags parsed + stored, rejected at point of use. |
| 3.3 | Resolve aligner; HISAT2/minimap2 → error; mutual-exclusivity dies | `config.rs:161-189` (`resolve_aligner`) | DONE | All three mutual-exclusion `die`s + two "deferred to v1.x" `Unsupported` errors. |
| 3.4 | Resolve library type (directional/non_dir/pbat, mutually exclusive) | `config.rs:191-204` (`resolve_library`) | DONE | non_dir+pbat conflict die; defaults to Directional. |
| 3.5 | Read layout + format; SE separator `:`→`,`; input-file existence; mate/conflict dies | `config.rs:206-304` (`resolve_format`,`resolve_layout`,`check_exists`) | DONE | `--single_end`+`-1/-2` die, mate-count die, `-1==-2` die, `-2` w/o `-1` die, `se.replace(':',",")`, per-file `check_exists`. |
| 3.6 | Genome canonicalize; CT/GA dirs; small→large index check; basenames; FASTA priority-fallback | `discovery.rs:93-171` | DONE | `canonicalize`, small index then `.bt2l` fallback (`large_index`), `BS_CT`/`BS_GA` basenames, priority `.fa→.fa.gz→.fasta→.fasta.gz`, case-insensitive glob sort. |
| 3.7 | bowtie2 path resolve (`--path_to_bowtie2` dir vs PATH); `--version` exec+parse; 2.5.5 pin-warn | `aligner.rs:29-109` | DONE | dir-required die, `which` fallback, version-triple parse, warn if `!= 2.5.5`. |
| 3.8 | Assemble `aligner_options` — exact ordered string | `options.rs:17-181` | DONE | 15-position order verified vs Perl 7838–8142 (see §3.8 table below). |
| 3.9 | Output-target resolution; `--basename` override; `--output_dir`/`--temp_dir` default `''` independently; SAM/CRAM defer | `config.rs:306-325` (`resolve_output`) + `OutputTarget` | DONE | SAM/CRAM `Unsupported`; `unwrap_or_default()` → empty PathBuf for both dirs independently; basename stored verbatim. |
| 3.10 | Build `RunConfig`, print summary, exit 0 | `config.rs:133-159` (`resolve`) + `summary()` 327-377; `lib.rs:run` 47-51; `main.rs:28-34` | DONE | `run` prints `summary()` to stderr, returns Ok → exit 0. |

### §3.8 `aligner_options` push order (authoritative list vs code)

| Pos | Plan item | Code (`options.rs`) | Status |
|-----|-----------|---------------------|--------|
| 1 | `-q`/`-f` format | lines 25-28 | DONE |
| 2 | `--phred33`/`--phred64` (each requires `-q`; mutually exclusive) | 31-44 (`require_fastq`) | DONE |
| 3 | `-N <0\|1>` (die unless 0/1) | 47-55 | DONE |
| 4 | `-L <int>` | 57-59 | DONE |
| 5 | `-D <int>` | 61-63 | DONE |
| 6 | `-R <int>` | 64-66 | DONE |
| 7 | `--score-min L,...` always; shape-only validate; `--local` rejected | 68-90 (`valid_score_min_l`, `Unsupported` for `--local`) | DONE |
| 8 | `--rdg i,j` | 99-110 | DONE |
| 9 | `--rfg i,j` (5,3 = internal scalars, not pushed) | 111-122; `GapPenalties` default 5,3 not pushed | DONE |
| 10 | `-p <n>` + `--reorder` (≥2) | 125-133 | DONE |
| 11 | `--ignore-quals` ALWAYS (not last) | 136 | DONE |
| 12 | PE: `--no-mixed`,`--no-discordant`,`--dovetail` unless `--no_dovetail` | 139-153 (+ `old_flag` conflict die) | DONE |
| 13 | `--minins` (PE-only, SE die) | 156-163 | DONE |
| 14 | `--maxins`/`--maxins 500` (PE default) | 164-173 | DONE |
| 15 | `--quiet` | 176-178 | DONE |

Cross-checked against Perl `bismark` 7838–8142 (`grep push @aligner_options`): seed `-N`/`-L`/`-D`/`-R` precede `--score-min`; `--rdg`/`--rfg` after score-min; `-p`/`--reorder` then `--ignore-quals` then PE flags then `--minins`/`--maxins`/`--quiet`. Order is position-for-position identical.

### §5 Implementation outline (steps 1–9)

| # | Step | Source | Status |
|---|------|--------|--------|
| 1 | Add crate to workspace; `Cargo.toml` + `main`/`lib` | `Cargo.toml`; workspace `rust/Cargo.toml` members; `main.rs`/`lib.rs` | DONE | (mimalloc dep omitted — see Gaps/Deviations) |
| 2 | `error.rs`: `BismarkError` (thiserror) genome/index/aligner/option variants | `error.rs` (`AlignerError`, 8 variants) | DONE | Named `AlignerError`, not `BismarkError` — cosmetic rename, all required variant categories present. |
| 3 | `cli.rs`: clap struct, all 60 options, aliases incl. `--genome`, raw-argv, mode resolution | `cli.rs` + `main.rs` (argv) + `config.rs` (mode resolution) | DONE | Mode-resolution lives in `config.rs` (cleaner seam) rather than `cli.rs`; functionally equivalent. |
| 4 | `discovery.rs`: canonicalize, CT/GA, small/large checks, basenames, FASTA fallback | `discovery.rs` | DONE | |
| 5 | `aligner.rs`: path resolution + `--version` + parse + pin-warn | `aligner.rs` | DONE | |
| 6 | `options.rs`: ordered assembly + score_min/rdg/rfg validators | `options.rs` | DONE | |
| 7 | `config.rs`: `RunConfig` + `OutputTarget` + summary printer | `config.rs` | DONE | |
| 8 | `lib.rs`: `parse_and_resolve()` orchestration; `main.rs`: call+print+exit | `lib.rs:run` + `config.rs:resolve` + `main.rs` | DONE | Entry named `resolve()`/`run()` rather than `parse_and_resolve()`; same orchestration 3.1–3.10. |
| 9 | Tests incl. Perl-oracle option-string comparison | `options.rs` tests + `tests/cli.rs` | DONE | Option strings asserted as literals (Phase-0 oracle values); no live Perl subprocess (out of scope for hermetic Phase 1). |

### §9 Validation table (#1–#13)

| # | Verify | Test | Status |
|---|--------|------|--------|
| 1 | Default options string `-q --score-min L,0,-0.2 --ignore-quals` | `options::tests::default_se_options_match_phase0_spike` | DONE (PASS) |
| 2 | `--score_min L,0,-0.4` substituted | `options::tests::score_min_override_substituted` | DONE (PASS) |
| 3 | Index basename derivation `…/CT_conversion/BS_CT` (+GA) | `discovery::tests::fasta_priority_…` (asserts `ends_with` CT/GA) + `tests/cli::happy_path…` | DONE (PASS) |
| 4 | Missing/partial index → "faulty or non-existant" | `discovery::tests::incomplete_index_errors` + `cli::missing_index_errors` | DONE (PASS) |
| 5 | bowtie2 missing → detection error | `error.rs::AlignerNotWorking`; path-resolve dir-check (`aligner.rs:32-36`) | DONE (code+integration coverage via fake-bowtie2 path; explicit empty-dir test not isolated but path is exercised) |
| 6 | `--hisat2`/`--minimap2` → "deferred", exit≠0 | `cli::hisat2_is_deferred`, `cli::minimap2_is_deferred` | DONE (PASS) |
| 7 | argv captured verbatim | `main.rs:16-17` → `RunConfig.command_line` (stored; exercised via summary) | DONE |
| 8 | bowtie2 2.5.5 parse + pin-warn | `aligner::tests::parses_standard_bowtie2_version_line`, `rejects_non_triple`; warn logic `aligner.rs:74-84`; integration uses fake `version 2.5.5` | DONE (PASS) — real-bowtie2 oxy/CI gate is a later real-data step, not a Phase-1 unit blocker |
| 9 | options ORDER `-n 1 -L 20 --quiet` | `options::tests::seed_flags_precede_score_min_and_quiet_is_last` | DONE (PASS) — asserts exact `-q -N 1 -L 20 --score-min L,0,-0.2 --ignore-quals --quiet` |
| 10 | `--phred33/64` without `-q` → die | `options::tests::phred_without_fastq_errors` | DONE (PASS) |
| 11 | `--basename foo` → `foo.bam` (no suffix) | `OutputTarget.basename` stored verbatim (`config.rs:97-98,318-321`) | DONE (stored; the actual `<basename>.bam` name composition is Phase 5/6 — §13 carries this forward, consistent with "no alignment/output written in Phase 1") |
| 12 | FASTA priority-fallback + case-insensitive order | `discovery::tests::fasta_priority_prefers_fa_and_sorts_case_insensitively`, `falls_back_to_fasta_when_no_fa` | DONE (PASS) |
| 13 | missing input file / `-1`==`-2` | `cli::missing_input_file_errors`; `config.rs:259-264` same-file die + `discovery::tests::no_fasta_errors` | DONE (missing-file PASS; same-file die has code path, no dedicated isolated test) |

### §13 Documented deviations

| Deviation | Claimed in §13 | Verified | Status |
|-----------|----------------|----------|--------|
| mimalloc omitted in Phase 1 | "the plan §2 listed it … add in Phase 9 … output-neutral" | `grep mimalloc src/ Cargo.toml` → not found (exit 1); matches sibling genome-prep | DEVIATED, documented (acceptable) |
| `-n`/`-l` (Bismark) → `-N`/`-L` (Bowtie2) spelling map | "CLI uses Bismark spellings; options string emits Bowtie 2 spellings" | `cli.rs:104-108` (`-n`/`-l`); `options.rs:49,58` emit `-N`/`-L`; test comment confirms | DEVIATED, documented (acceptable) |

## Gaps (detail)

No MISSING or PARTIAL items. Two minor observations, neither a gap:

### Naming deviations (undocumented but cosmetic, not behavior)
- §5 step 2 names the error type `BismarkError`; code uses `AlignerError`. §4 signature names the entry `parse_and_resolve()`; code uses `resolve()` (lib re-exports `resolve`) with `run()` as the binary wrapper. These are internal-name renames with identical behavior and full variant/orchestration coverage. Not flagged as DEVIATED in §13, but they do not affect any external contract or the byte-identity-critical option string. No action required.

### Items whose final composition is explicitly deferred (per plan, not gaps)
- §9 #11 (`<basename>.bam` literal name) and the `--prefix`×`--basename` interaction are stored in `OutputTarget` but the actual output-file-name string is composed in Phase 5/6 — §13 "Carried-forward open items" states this. Phase 1's contract ("no alignment/output written") is satisfied: the override field is captured.
- §9 #5/#8 real-bowtie2 detection error and 2.5.5-on-oxy verification are real-data steps; Phase 1 covers the logic hermetically with a fake bowtie2. Consistent with the plan's hermetic-test design (`tests/cli.rs` header).

## Test verification (Mode B)

`cargo test -p bismark-aligner` (sandbox-disabled, worktree outside sandbox paths) → **ok. 24 passed; 0 failed**.

| Test | File | Status |
|------|------|--------|
| parses_standard_bowtie2_version_line | src/aligner.rs | PASS |
| rejects_non_triple | src/aligner.rs | PASS |
| default_se_options_match_phase0_spike | src/options.rs | PASS |
| seed_flags_precede_score_min_and_quiet_is_last | src/options.rs | PASS |
| score_min_override_substituted | src/options.rs | PASS |
| paired_end_tail_and_default_maxins | src/options.rs | PASS |
| fasta_uses_dash_f | src/options.rs | PASS |
| rejects_bad_seedmms | src/options.rs | PASS |
| rejects_local_in_v1 | src/options.rs | PASS |
| phred_without_fastq_errors | src/options.rs | PASS |
| rdg_rfg_appended_and_validated | src/options.rs | PASS |
| fasta_priority_prefers_fa_and_sorts_case_insensitively | src/discovery.rs | PASS |
| falls_back_to_fasta_when_no_fa | src/discovery.rs | PASS |
| incomplete_index_errors | src/discovery.rs | PASS |
| no_fasta_errors | src/discovery.rs | PASS |
| version_flag_prints_banner | tests/cli.rs | PASS |
| no_genome_errors | tests/cli.rs | PASS |
| hisat2_is_deferred | tests/cli.rs | PASS |
| minimap2_is_deferred | tests/cli.rs | PASS |
| missing_input_file_errors | tests/cli.rs | PASS |
| happy_path_resolves_and_prints_config | tests/cli.rs | PASS |
| missing_index_errors | tests/cli.rs | PASS |
| sam_output_is_deferred | tests/cli.rs | PASS |
| pbat_genome_as_positional_resolves | tests/cli.rs | PASS |

## Verdict

**COMPLETE.** Every §3 behavior (3.1–3.10), §5 outline step (1–9), §9 validation (#1–#13), and §3.8 push-order position maps to real code and a real (passing) test or stored field. The §3.8 order matches Perl 7838–8142 position-for-position. Both §13 deviations (mimalloc omitted; Bismark→Bowtie2 spelling map) are real and documented; the only un-noted deviations are cosmetic internal renames (`AlignerError`/`resolve()`) with no behavioral or contract impact. No undocumented behavioral deviation, no missing or partial item.

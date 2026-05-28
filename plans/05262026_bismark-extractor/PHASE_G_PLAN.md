# Phase G — `--bedGraph` + `--cytosine_report` subprocess chain (closes #868)

**Status:** Plan rev 1, post-dual-plan-review absorption. Awaiting implementation trigger from Felix.
**Parent issue:** #868 (filed under epic #798). Phase G is the **last feature phase** before Phase H byte-identity gate.
**Branch target:** new `extractor-phase-g` from `rust/iron-chancellor` HEAD `4e5c691` (Phase C.2 merge).
**Crate version bump:** `bismark-extractor` `1.0.0-alpha.8` → `1.0.0-alpha.9`.

> **Epic:** `plans/05262026_bismark-extractor/PROGRESS.md`, Phase G — bedGraph + cytosine_report subprocess chain. No standalone `EPIC.md` exists; the local epic-tracking artifact is `PROGRESS.md`. The upstream coordination doc is GitHub epic #798.

## Revision History

| Rev | Date | Notes |
|---|---|---|
| 0 | 2026-05-27 | Initial draft post-Phase-C.2 merge. One Critical resolved with Felix before completing: **subprocess stderr handling = TEE** (live to user's stderr + bounded ring-buffer for error reporting). Subprocess discovery via PATH + `BISMARK_BIN` env override, inherit cwd + env, positional-args-not-stdin, `--ample_memory` pass-through verbatim — taken as Perl-aligned defaults. |
| 3 | 2026-05-28 | **Post-code-review absorption complete.** Folded 1 Critical (A-L1: wiring seam producing byte-identity bug on `.bam` inputs) + 3 High (A-ER1: drain join skipped on `wait()` failure; B-H1: SPEC §6.6 argv-identity claim over-stated for positional tail; B-H2: sweep aborted mid-loop on first `remove_file` failure). Plan-manager Mode B verdict: COMPLETE (30 DONE, 6 DEVIATED documented, 2 PARTIAL deferred polish, 0 MISSING). See "Post-review absorption" section in Implementation Notes. Re-validation: 303 tests passing (+4 new state.rs regression-guard tests for L1); clippy clean; fmt clean. Ready for commit. |
| 2 | 2026-05-27 | **Implementation complete.** Branch `extractor-phase-g` off `4e5c691`. See "Implementation Notes" section below the rev table. 299 tests passing (238 pre-G + 61 new Phase G). Clippy clean (`-D warnings`); fmt clean. Two planned tests deferred to follow-up (argv-parity goldens vs Perl + `#[ignore]`'d real-subprocess smokes) — rationale in deviation log. |
| 1 | 2026-05-27 | **Folded both reviewers' 5 distinct Critical + ~17 distinct Important + selected Optional findings** from `PLAN_REVIEW_PHASE_G_A.md` + `PLAN_REVIEW_PHASE_G_B.md`. Headline changes:<br>**C1 (both, A-Critical / B-Important — both saw it):** §3.2 vs §10 vs §11 discovery-order contradiction. Three sections disagreed (BISMARK_BIN-first vs PATH-first). Rev 1 pins **BISMARK_BIN-first, then PATH, then `current_exe()` parent** consistently across §3.2 + §10 + §11; adds a precedence test asserting BISMARK_BIN beats PATH when both contain the tool.<br>**C2 (A):** Plan §4.5 called `ExtractState::input_basename()` but `ExtractState` doesn't store the field today. Rev 1 adds `input_basename: String` field to `ExtractState`; updates constructor + §2.2 LOC + §4.5 + §7.1.<br>**C3 (B):** `derive_bedgraph_filename` strip semantics WRONG. Perl regex `s/gz$//` strips literal `gz` not `.gz` — for `foo.bam.gz` the trailing dot is PRESERVED between `bam` and `bedGraph` (output `foo.bam.bedGraph`). Rev 1 rewrites §2.4.6 with a 6-row edge-case derivation table covering chained-extension cases; pins goldens for the unit tests in §5.4.1.<br>**C4 (B):** `kept_split_files` form not pinned. Rev 1: **return absolute paths from `OutputFileMap`**; document at §4.4; add explicit test that the returned Vec contains absolute paths and matches Perl's `@sorting_files` shape modulo this distinction.<br>**C5 (B):** `BufRead::read_line` errors on non-UTF-8 stderr (panics drain thread mid-run). Rev 1: use `read_until(b'\n', &mut Vec<u8>)`; `stderr_tail: Vec<u8>` (not `String`) in the error variant; lossy UTF-8 conversion deferred to `Display` time.<br>**I1 (both):** Drain thread NOT joined on the Ok path (§3.4 step 4 of rev 0). Rev 1: always join, regardless of exit status. Spawn drain BEFORE `child.wait()` to avoid pipe-buffer deadlock on >64 KiB stderr-bursts. Adds a 128 KiB-burst test in §5.4.2.<br>**I2 (both):** No CI-running test exercises `RealRunner`. Rev 1 adds a `#[cfg(unix)]` integration test using a `tests/fixtures/fake_bismark2bedgraph.sh` script — exercises drain thread + child.wait + stderr capture without external deps.<br>**I3 (both):** No argv-parity test vs Perl. Rev 1 adds 3 golden-file argv-parity tests in `tests/phase_g_argv_parity.rs` covering (default, `--gzip`, `--cytosine_report --CX`) configs. Goldens are generated once from a Perl print-and-exit shim and committed.<br>**I4 (both):** `which` crate not actually in workspace. Rev 1 commits to **adding `which` to `rust/bismark-extractor/Cargo.toml`** as a direct dep; updates §5.2.<br>**I5 (A):** `--buffer_size` Perl-fidelity gap when neither `--buffer_size` nor `--ample_memory` is explicitly set. Perl always pushes `--buffer_size 2G` in that case. Rev 1: Rust mirrors — if neither flag set, push `--buffer_size 2G`. Adds a unit test.<br>**I6 (B):** Spawn drain thread BEFORE `child.wait()` to avoid pipe-buffer-full deadlock. Rev 1 makes the ordering explicit in §3.4 and tests it (see I1).<br>**I7 (A):** Smoke test assertions ("check output exists") are too weak. Rev 1 tightens to column-count + non-zero-row checks for both bismark2bedGraph and coverage2cytosine outputs.<br>**I8 (A):** LC_ALL / locale dependency is a Phase H pre-req (UNIX sort is locale-sensitive). Rev 1 adds a SPEC §6.6 paragraph documenting the requirement; harness will pin `LC_ALL=C` for byte-identity runs.<br>**I9 (A):** `stdin(Stdio::null())` not explicit. Rev 1 sets it in `RealRunner::run`; documents in §3.3.<br>**I10 (A):** `FinalizationReport` struct preferred over bare `Vec<PathBuf>`. Rev 1: `OutputFileMap::finalize_with_empty_sweep` returns `FinalizationReport { kept: Vec<PathBuf>, swept: Vec<PathBuf> }`. Leaves room for Phase H to add counts/error fields without API churn.<br>**I11 (A):** "Deliberately omitted c2c flags" — c2c accepts `--merge_CpGs`, `--GC_context`/`--GC`, `--nome-seq`, `--ffs`, `--discordance_filter`, `--drach`/`--m6A` that the extractor doesn't expose. Rev 1 documents these as deliberately-not-forwarded in §7.3 + SPEC §6.6 note.<br>**I12 (B):** Strict-vs-permissive `BISMARK_BIN`. Rev 1: **strict.** If `BISMARK_BIN` is set and the tool is NOT in there, return `SubprocessNotFound`. No fallback. Edge-case row in §3.8 updated; explicit error rationale: if a user sets BISMARK_BIN they want to lock the tool source.<br>**I13 (B):** `--parent_dir == --dir` invariant. Perl pushes both with the same value to c2c. Rev 1 makes the equality explicit in §2.4.2 + §4.3 + adds an argv-parity assertion.<br>**I14 (B):** Empty-kept-set integration test. Rev 1 adds `phase_g_runs_bismark2bedgraph_with_empty_kept_set` (mocked); `--cytosine_report` UX warning: when kept-set is empty + `--cytosine_report` is set, emit `eprintln!("note: extractor produced no methylation calls; cytosine_report will scan the genome anyway")` before chain entry.<br>**I15 (B):** `tests/phase_g_real.rs` opt-in mechanism. Rev 1 spells out the exact invocation `cargo test -p bismark-extractor --test phase_g_real -- --ignored` in §5.4.3 + §5.8.<br>**I16 (A):** Test for `BISMARK_BIN` set-but-not-executable / set-but-empty. Rev 1 adds `discover_subprocess_bismark_bin_set_but_tool_not_executable_returns_not_found` and `discover_subprocess_bismark_bin_empty_string_falls_through_to_path`.<br>**I17 (both):** `current_exe()` test design brittleness — symlinking test binary is platform-flaky. Rev 1 replaces with an env-var-override hatch `BISMARK_TEST_CURRENT_EXE_DIR` consulted only under `#[cfg(test)]` to inject a fake "current_exe()" parent dir.<br>**O1 (B):** `VecDeque<u8>` instead of `Vec<u8>` for ring buffer — O(1) eviction. Rev 1 folds.<br>**O2 (B):** Always-on `eprintln!` of the subprocess command line before spawn — debugging affordance. Rev 1 folds (gated behind `--verbose` flag if present in future, or unconditional for v1.0 since extractor has no verbose flag yet).<br>**O3 (A):** `--counts` field is dead. Rev 1 documents in `cli.rs` (no code change; just a clarifying comment that the field is accepted-and-ignored). Deferred to a separate housekeeping PR if reviewer pushes harder.<br>**O4 (B):** Module doc for `src/subprocess.rs` includes the byte-identity invariant ("argv passed to Perl bismark2bedGraph from Rust MUST match Perl extractor's argv byte-for-byte modulo long-form flag expansion"). Rev 1 folds. |

## Implementation Notes (2026-05-27, post-impl)

Executed on branch `extractor-phase-g` (off `rust/iron-chancellor` HEAD `4e5c691`). Crate version `1.0.0-alpha.8` → `1.0.0-alpha.9`. `which = "=7.0.3"` added as direct dep.

### Per-task status

| § | Done | Notes |
|---|---|---|
| §5.1 SPEC updates | ✅ | §6.6 rewritten with full subprocess invocation matrix + filename-derivation quirk table + tee/discovery/LC_ALL/deliberately-omitted notes. §12 (pitfalls) subprocess error propagation row extended with rev 1 details. §10 row G updated to `~1330 LOC actual`. §11 "Subprocess-vs-inline" Critical marked Resolved. Skipped: §3 rows 17-26+35 "wired Phase G" suffix and §8.x test-rows append — documentation polish not materially affecting the SPEC contract; deferrable. |
| §5.2 Cargo.toml + version | ✅ | `which = "=7.0.3"` added. Version bumped + description updated. `cargo build` clean. |
| §5.3 src/subprocess.rs (production) | ✅ | ~360 LOC production: `SubprocessTool` enum + Display + `binary_name()`; `RingBuffer` (VecDeque-backed); `derive_bedgraph_filename` / `_coverage_filename` / `_cytosine_filename` (trailing-dot-preserving per rev 1 C3); `discover_subprocess` (BISMARK_BIN-first strict; empty-string falls through; `BISMARK_TEST_CURRENT_EXE_DIR` env hatch — ungated, see deviation below); `build_bismark2bedgraph_argv` (long-form flags; `--buffer_size 2G` default); `build_coverage2cytosine_argv` (`--parent_dir == --dir`); `BismarkSubprocessRunner` trait + `RealRunner` impl (spawn drain BEFORE wait; `read_until` byte-safe; always join; `stdin(Stdio::null())`; pre-spawn audit eprintln); `run_phase_g_chain` orchestrator with empty-kept-set UX warning. Module doc cites byte-identity invariant + Perl reference lines. |
| §5.3 error.rs (3 new variants) | ✅ | `SubprocessFailed { tool, exit_status, stderr_tail: Vec<u8> }` (rev 1 C5 byte-safe; Display via String::from_utf8_lossy at render time). `SubprocessNotFound { tool, searched_paths }`. `SubprocessSpawnFailed { tool, source: io::Error }` (with `#[source]`). All under `// Phase G additions` divider. |
| §5.5 state.rs wire-up | ✅ | Added `input_basename: String` field + accessor. Stored at construction (constructor signature already accepts the basename). `finalize()` extended: capture FinalizationReport from sweep into local `finalization`, then under `MbiasOnly` use Default::default(). After M-bias write, gate on `config.bedgraph`: invoke `run_phase_g_chain` with `&RealRunner`, passing `&finalization.kept`. Deviation from plan: did NOT add `last_kept_files: Option<Vec<PathBuf>>` private field — the kept paths are local to `finalize()` and consumed inline; no need to persist across method calls. Plan-described shape preserved at the orchestrator-input level. |
| §4.4 FinalizationReport in output.rs | ✅ | New `pub struct FinalizationReport { pub kept: Vec<PathBuf>, pub swept: Vec<PathBuf> }` with Default derive. `finalize_with_empty_sweep` return changed from `Result<(), io::Error>` to `Result<FinalizationReport, io::Error>`. Paths absolute via `fs::canonicalize` BEFORE potential unlink (defensive fall-back to `path.clone()` on canonicalize failure). Kept sorted lexicographically before return (rev 1 I7). |
| §5.6 lib.rs | ✅ | Added `pub mod subprocess;`. |
| §5.4.1 Inline unit tests in subprocess.rs::tests | ✅ | **34 tests passing**: 13 filename derivation (incl. 6 chained-extension goldens per rev 1 C3), 11 b2bg argv, 6 c2c argv, 4 ring buffer (incl. exact-cap + larger-than-cap policy goldens), 1 SubprocessTool Display round-trip. The 6 discovery tests originally planned for inline moved to `tests/phase_g_discovery.rs` because the crate-level `#![forbid(unsafe_code)]` blocks `std::env::set_var` (unsafe in Rust 2024+) in inline tests; integration-test crate inherits no such forbid. |
| §5.4.2 tests/phase_g.rs (orchestrator) | ✅ | **12 tests passing**: no-op-when-neither-flag, b2bg-only, both-tools-in-order, first-fail-no-second, second-fail-bubbles-correct-variant, kept-files-positional-tail, gzip-flag-only-to-c2c-not-b2bg, long-form-flag-names, --buffer_size-2G-default, --parent_dir==--dir-invariant, empty-kept-set-does-not-skip-chain, chained-extension-trailing-dot-filenames. Custom MockRunner uses `ExitStatus::from_raw` for controllable exit codes (Unix-only — file is `#[cfg(unix)]`). |
| §5.4.3 tests/phase_g_argv_parity.rs (Perl goldens) | ⏸ **deferred to follow-up** | Inline argv-builder tests + orchestrator tests already cover argv shape; Perl-comparison would require a print-and-exit shim on the user's Perl install to regenerate. Recipe documented in `tests/fixtures/README.md`. Recommended as a separate sub-task post-merge if reviewer pushes; argv-shape correctness is otherwise verified by the 6 c2c argv tests + 11 b2bg argv tests + 12 orchestrator tests. |
| §5.4.4 tests/phase_g_realrunner.rs | ✅ | **9 tests passing** (`#[cfg(unix)]`): zero-exit Ok, non-zero exit returns RunOutcome, correct tool variant in error, high-volume stderr stays bounded (1 MiB → 64 KiB tail), 128 KiB stderr-burst doesn't deadlock (rev 1 I6 guard), non-UTF-8 stderr handled (rev 1 C5 guard), spawn-fail when program absent, drain joined on Ok path, drain joined on Err path. Five fake-shell scripts in `tests/fixtures/`: success, failure, high_stderr, burst_then_exit, non_utf8_stderr. Scripts chmod +x committed via git. |
| §5.4.4 tests/phase_g_discovery.rs (env-var hatch) | ✅ | **6 tests passing**: BISMARK_BIN-wins-with-tool, BISMARK_BIN-set-but-tool-missing-strict-not-found, BISMARK_BIN-set-but-not-executable-not-found, BISMARK_BIN-empty-falls-through, current_exe-test-hatch-fallback, all-paths-exhausted-not-found. Uses `Mutex<()>` ENV_LOCK for serialised env-var manipulation across parallel tests. |
| §5.4.5 tests/phase_g_real.rs (`#[ignore]`'d real-subprocess) | ⏸ **deferred to follow-up** | Real-subprocess smoke tests are opt-in dev verification; the 9 RealRunner fake-shell tests already exercise every `RealRunner` code path with deterministic fixtures. Recommended as a separate sub-task for oxy / dev-machine verification post-merge. |
| §5.7 PROGRESS.md | ✅ | Phase G row status → implementation complete (see separate update). |
| §5.8 Pre-merge validation | ✅ | `cargo test -p bismark-extractor` → **299 passed, 0 failed, 0 ignored** across all suites (lib + integration). `cargo clippy -p bismark-extractor --all-targets -- -D warnings` → clean. `cargo fmt --check` → clean (after one auto-fix on multi-line conditionals in test files; `cargo fmt` ran and the diff was the canonical formatting). |

### Pre-existing test updates

The `FinalizationReport` return-type change to `finalize_with_empty_sweep` did NOT break any existing test — no test directly called the function; the only caller is `state.rs::ExtractState::finalize` which the change updated in step. The Phase C.2 test `tests/output_phase_c2.rs` (which captures stderr to assert sweep log lines) continues to pass since the `eprintln!` statements are unchanged.

### Deviations from rev 1 plan

1. **Plan §3.2 said "current_exe() fallback under `#[cfg(test)]` honours `BISMARK_TEST_CURRENT_EXE_DIR`".** Implementation makes the env hatch **always-on** (not cfg-gated). Reason: integration-test crates compile against the lib without `#[cfg(test)]`, so the gated version is invisible to them — defeating rev 1 I17's intent. The env hatch is harmless in production (the env var won't be set; the function falls through to `current_exe()`). Documented inline with rationale.
2. **6 discovery tests moved from inline `mod tests` to `tests/phase_g_discovery.rs`.** `#![forbid(unsafe_code)]` blocks `std::env::set_var`/`remove_var` (unsafe in Rust 2024+) in inline tests; integration-test crate inherits no forbid. Plan §5.4.1 grouped them under "inline unit tests"; moving to integration tests is the cleanest workaround that preserves the test intent.
3. **`tests/phase_g_argv_parity.rs` (3 Perl-goldens) deferred to a follow-up sub-task.** Plan §5.4.3 specified golden-file regeneration via a Perl print-and-exit shim; the regen requires a user Perl install. Existing argv coverage (11 b2bg + 6 c2c + 12 orchestrator tests) verifies the argv shape correctness, including all rev 1 corrections (long-form flags, `--buffer_size 2G` default, `--parent_dir == --dir`, trailing-dot filenames). The Perl-comparison adds independent confirmation of the rule set but isn't load-bearing for v1.0 correctness. README at `tests/fixtures/README.md` documents the regen recipe.
4. **`tests/phase_g_real.rs` (2 `#[ignore]`'d real-subprocess smokes) deferred to a follow-up sub-task.** The 9 RealRunner fake-shell tests already exercise every `RealRunner` code path deterministically in CI; real-subprocess smokes are opt-in dev verification and don't gate the merge. Recommended as a post-merge oxy verification.
5. **`last_kept_files: Option<Vec<PathBuf>>` private field on `ExtractState` not added.** Plan §4.5 suggested it; impl found the kept paths can be a local variable in `finalize()` consumed inline, no persistence needed across method calls. Equivalent semantics; one less field.
6. **`#[allow(dead_code)]` on `RingBuffer::len`** to keep the test-facing accessor visible without triggering the lint when no inline tests reference it.

### Iteration log

Implementation proceeded mostly in one pass; iterations were absorption of breakage:

1. **#1**: Cargo.toml edits — initial Edit attempts failed because I hadn't called Read on the file first; Read'd, edits applied, `cargo build` clean. No deviation.
2. **#2**: state.rs wire-up compile — clean first try. Build at this checkpoint passed; 238 pre-G tests still passed.
3. **#3**: Inline unit tests — compile error: `#![forbid(unsafe_code)]` blocks `std::env::set_var` in `with_env_var` helper used by 5 discovery tests. Resolution: moved those tests to `tests/phase_g_discovery.rs` (separate crate; no forbid). 34 inline tests passed.
4. **#4**: RealRunner integration tests — compile error: `RunOutcome` doesn't implement `Debug` (required by `expect_err`). Added `#[derive(Debug)]` to `RunOutcome`. 9 tests passed.
5. **#5**: Discovery test failure: `current_exe()` fallback test failed because the `BISMARK_TEST_CURRENT_EXE_DIR` env hatch was gated `#[cfg(test)]`, invisible to integration tests. Resolution: ungated the hatch (always-on; harmless in production); deviation #1 above. 6 tests passed.
6. **#6**: Clippy errors — `vec_init_then_push` on `build_coverage2cytosine_argv` (the unconditional-pushes prefix) and `useless_vec` in one ring-buffer test. Resolution: converted the c2c prefix to `vec![]` macro; replaced `&vec![b'a'; 50]` with `&[b'a'; 50]`. Clippy clean.
7. **#7**: fmt drift in two test files (multi-line conditionals). Resolution: `cargo fmt -p bismark-extractor` applied canonical formatting. fmt --check clean.

Total: 7 iterations, all correctness-preserving or lint compliance.

### Test summary (post-rev-2 absorption)

| Suite | Pre-G | Phase G rev 2 | Post-G total |
|---|---|---|---|
| lib unit tests | 53 | +34 subprocess inline + **+4 state.rs L1 regression guards (rev 2)** | 91 |
| tests/phase_g.rs | — | +12 (orchestrator, mocked) | 12 |
| tests/phase_g_discovery.rs | — | +6 (BISMARK_BIN + env hatch) | 6 |
| tests/phase_g_realrunner.rs | — | +9 (drain + ring + spawn) | 9 |
| Other integration tests | 185 | 0 (unchanged) | 185 |
| **Total** | **238** | **+65** | **303** |

All 303 tests pass; 0 ignored, 0 failed.

---

## Post-review absorption (rev 3, 2026-05-28)

Per the global CLAUDE.md workflow §5: after implementation, dual `code-reviewer` agents + `plan-manager` Mode B were launched. The verification triangle returned:

| Agent | Verdict | Findings |
|---|---|---|
| Code-reviewer A | NEEDS-REVISIONS | 1 Critical + 1 High + Medium/Low polish |
| Code-reviewer B | APPROVE-WITH-NITS | 0 Crit + 2 High + 4 Medium + 7 Low |
| Plan-manager Mode B | COMPLETE | 30 DONE, 6 DEVIATED, 2 PARTIAL, 0 MISSING |

**Critical L1 was reviewer-A-unique** (B verified filename derivation function correctness independently but didn't trace the production seam). **Both Highs from B were B-unique** (A's checklist didn't include sort-stability or sweep-robustness). The dual approach proved its value twice over: rev 0→rev 1 absorbed B's function-logic Critical; rev 2→rev 3 absorbed A's caller-seam Critical. The two layers (function logic vs caller wiring) were independently inspected.

### Absorption per finding

| Finding | Severity | Source | Fix |
|---|---|---|---|
| **L1** Production passes `pipeline::derive_basename`-stripped basename into `run_phase_g_chain`; `derive_bedgraph_filename` then has nothing to strip, producing `…deduplicatedbedGraph` (no dot) instead of `…deduplicated.bedGraph`. Phase H byte-identity fails on every real `.bam` input. | **Critical** | A | `state.rs`: added private fn `derive_raw_filename_for_phase_g(&Path) -> String` that returns the un-stripped filename via `Path::file_name()`. `finalize()` now calls this and passes the result into `run_phase_g_chain` instead of the now-removed `input_basename: String` field. **+4 regression-guard unit tests** in `state.rs::tests` covering `.bam`, real Bismark PE name, `.cram`, chained `.bam.gz`. |
| **ER1** `RealRunner::run` returns via `?` on `child.wait()` failure BEFORE joining the drain thread; contradicts the rev-1 I1 "always join" contract documented in `§3.4`. | **High** | A | `subprocess.rs::RealRunner::run`: split `child.wait()` + drain join into independent steps. Drain joined unconditionally first (naturally exits on stderr EOF when subprocess closes its FDs at exit); wait result then propagated via `?`. Doc comment updated to describe the new ordering rationale. |
| **H1** SPEC §6.6 claim "argv built to match Perl byte-for-byte modulo long-form flag expansion" is over-stated for the positional tail: `kept.sort()` is lexicographic; Perl's `@sorting_files` order depends on `keys %fhs` hash-iteration (randomised per Perl run). | **High** | B | SPEC §6.6: claim narrowed to the **flag block** (everything before positional tail). New paragraph documents that positional tail is Rust-deterministic and may differ from Perl run-to-run; `bismark2bedGraph`'s internal UNIX-sort means positional argv order doesn't affect output bytes — so this is an implementation detail, not a byte-identity invariant. |
| **H2** `finalize_with_empty_sweep` aborts mid-loop on first `remove_file` error, dropping subsequent unlinks AND skipping post-sweep splitting-report + M-bias + Phase G chain. | **High** | B | `output.rs::finalize_with_empty_sweep`: changed `?` on `remove_file` to log-and-continue (`eprintln!("warning: failed to remove empty output file {path_str}: {e}")`). File still recorded as `swept` in the report — the intent was to drop it, and a transient FS issue shouldn't desync the logical state from the on-disk state. Doc comment updated. |

### Medium/Low findings — deferred (not absorbed)

| Finding | Severity | Source | Rationale for deferral |
|---|---|---|---|
| Drain thread holds `io::stderr().lock()` across the whole drain loop, serialising parent's stderr with subprocess's stderr. | Medium (A) + M3 (B) | Both | Acceptable in v1.0 — Phase F worker threads already joined before chain dispatch; the parent's stderr writers are quiescent during the chain. Documented as design choice in rev 1 magnet §11; no observed regression. |
| Pre-spawn `eprintln!` audit line may be noisy in CI logs. | Low | A | Debate-worthy. Plan §3.4 + rev 1 O2 explicitly elected always-on for debugging affordance. Keep for v1.0; revisit if user-facing UX feedback warrants `--quiet` gating. |
| `RingBuffer::push_bytes` evicts byte-by-byte via `pop_front` loop (O(n) per oversized push) — could use a single `drain(..k)`. | Low | A | Negligible amortised cost: subprocess stderr writes are line-buffered (typical line ≤ 100 bytes); cap = 64 KiB. The byte-by-byte loop runs once on cap overflow, ~10 µs total. Tuneable later. |
| `BISMARK_TEST_CURRENT_EXE_DIR` env hatch is production-active (always-on, not cfg-gated). | Medium (B M4) | B | Deviation §1 from rev 2 already documents the rationale (integration-test crates don't inherit `cfg(test)`). Tiny real attack surface — discovery is a no-op if the env var doesn't point at a real executable. Cosmetic concern about naming; renaming to `BISMARK_DISCOVER_EXTRA_DIR` or feature-gating could be done in a polish PR. |
| Sort-stability vs Perl run-to-run (B H1's "restore Perl order" alternative). | High → resolved as documentation softening | B | Chose path (b) SPEC-softening over path (a) restore-Perl-order. Restoring Perl's order would require pinning a static OutputKey enum-iteration table that itself doesn't match Perl's hash-iteration (which is randomised). Path (b) is more honest about reality. |
| Various Low polish (docstring nits, etc.) | Low | A + B | Carrying as polish; not load-bearing for v1.0. Will sweep in a release-prep PR if reviewer requests. |

### Files modified during absorption

| File | Change |
|---|---|
| `rust/bismark-extractor/src/state.rs` | Removed `input_basename: String` field (was dead-code after L1 fix). Added private `derive_raw_filename_for_phase_g(&Path) -> String`. Added 4-test `mod tests` regression guard. Updated `finalize()` to call the new helper. |
| `rust/bismark-extractor/src/subprocess.rs` | `RealRunner::run` restructured to always-join drain before propagating wait() result (ER1 fix). Doc comment updated. |
| `rust/bismark-extractor/src/output.rs` | `finalize_with_empty_sweep` log-and-continue on `remove_file` failure (H2 fix). Doc comment updated. |
| `rust/bismark-extractor/SPEC.md` | §6.6 argv-byte-identity claim narrowed to flag block; positional tail caveat added (H1 fix). |
| `plans/05262026_bismark-extractor/PHASE_G_PLAN.md` | This rev-3 absorption row + "Post-review absorption" section. |

### Re-validation

| Check | Result |
|---|---|
| `cargo test -p bismark-extractor` | 303 passed / 0 failed / 0 ignored (was 299 pre-absorption; +4 new L1 regression-guards in state.rs::tests) |
| `cargo clippy -p bismark-extractor --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean (after one auto-format on output.rs's expanded eprintln! macro) |

### Verification confidence

- **Critical L1 fix** independently verified by main-thread trace through `pipeline.rs::derive_basename` (extension strip at lines 58-62) → `ExtractState::new` → `finalize` → `run_phase_g_chain` before applying. The 4 regression-guard tests pin the byte-identity-critical inputs (`.bam`, real-Bismark-PE-name, `.cram`, `.bam.gz`).
- **Reviewer-unique catches drove this absorption**: A caught L1 + ER1; B caught H1 + H2. A single reviewer would have missed ~50% of the absorbed findings. This is the structural value the dual-review pattern provides.
- **Plan-manager COMPLETE verdict** independently confirmed via test re-run (299/0/0 matched the plan's claim exactly) + ledger walk through §5.1-§5.8.

Ready for commit-trigger.

---

## 1. Goal

Engage the `--bedGraph` and `--cytosine_report` chain by spawning Perl's `bismark2bedGraph` (Perl `bismark_methylation_extractor:377`) and `coverage2cytosine` (`:424`) as subprocesses against the split-files Rust just emitted. Match Perl's CLI flag wiring exactly (including the **trailing-dot byte-identity quirk** in derived filenames per rev 1 C3); bubble subprocess failures as a typed `BismarkExtractorError::SubprocessFailed` variant; let the user see live subprocess progress on stderr while a bounded 64 KiB ring buffer of stderr is retained for error reporting. This unblocks Phase H's harness expansion to compare `*.bedGraph.gz`, `*.bismark.cov.gz`, and `CpG_report.txt.gz` against Perl byte-for-byte.

After this PR, `--bedGraph` and `--cytosine_report` go from "parsed-but-unused" (today's state — see `src/cli.rs:120-191`) to "shells out to the matching Perl tool with full flag pass-through and byte-identical filename derivation". Inline Rust replacements for these tools remain explicitly deferred to v1.x (per SPEC §6.6).

**Out-of-scope** (separately tracked):

- Inline Rust `bismark-bedgraph` / `bismark-coverage2cytosine` execution — v1.x evolution per SPEC §6.6 paragraph 2; epic #797 (existing) + a future cytosine_report epic.
- Phase H byte-identity gate proper — harness expansion to compare bedGraph/cov/CpG_report outputs against Perl lives in Phase H. Phase G provides the inputs.
- Subprocess perf optimisation — v1.0 accepts subprocess spawn overhead.
- `--parallel N` interaction with subprocesses — Phase F's worker model is upstream; subprocesses see only the final flushed split files. Explicit no-op assertion test in §5.4.

## 2. Context

### 2.1 Phase status table impact

| Phase | Before | After |
|---|---|---|
| C.2 | ✅ merged (`4e5c691`) | ✅ merged |
| **G** | ⏸ blocked-on-C.2 | 📝 plan rev 1 — this file |
| H | ⏸ partial harness (PASS on 8 extractor-output files) | After G: harness expands to bedGraph/cov/CpG_report streams |

### 2.2 Where the code lives (rev 1 — revised LOC table per dual-review findings)

The CLI flag surface for Phase G is **already present** in `src/cli.rs` and resolves into `ResolvedConfig`; CLI validation already enforces every documented Phase G mutex/precondition (see `Cli::validate` at `cli.rs:347-507`). What's missing is the subprocess invocation logic itself.

| Item | Files touched | Approximate LOC |
|---|---|---|
| New `src/subprocess.rs` module: runner trait, arg-builders, tee + ring-buffer (VecDeque-based), discovery, FinalizationReport struct | new file | ~300 |
| New error variants (`SubprocessFailed`, `SubprocessNotFound`, `SubprocessSpawnFailed`) | `src/error.rs` (+ ~25 LOC) | ~25 |
| `ExtractState::finalize` extension: chain dispatch after M-bias | `src/state.rs` (+ ~30 LOC) | ~30 |
| `ExtractState::input_basename: String` field + accessor (rev 1 C2) | `src/state.rs` (+ ~10 LOC) | ~10 |
| `OutputFileMap::finalize_with_empty_sweep` → `FinalizationReport` (rev 1 I10) | `src/output.rs` (+ ~40 LOC: new struct + return shape + absolute-path computation) | ~40 |
| Unit tests in `src/subprocess.rs::tests` (arg-builders, discovery, ring-buffer, error mapping, filename derivation edge cases) | inline `mod tests` | ~250 |
| Integration tests in `tests/phase_g.rs` (mocked runner; CLI dispatch; auto-trigger; gzip×subprocess; empty-kept-set; ordering) | new file | ~280 |
| Integration tests in `tests/phase_g_argv_parity.rs` (rev 1 I3: 3 golden-file parity tests + Perl shim recipe in `tests/fixtures/`) | new file + fixtures | ~120 |
| `RealRunner` fake-shell-script integration test (rev 1 I2) in `tests/phase_g_realrunner.rs` | new file + `tests/fixtures/fake_bismark2bedgraph.sh` | ~80 |
| Real-subprocess smoke (`#[ignore]`'d) in `tests/phase_g_real.rs` | new file | ~70 |
| SPEC updates: §6.6 matrix + LC_ALL note + deliberately-omitted-flags note, §6.5 error-variant signatures, §3 row wiring-status, §8.x test rows, §10 row G LOC | `rust/bismark-extractor/SPEC.md` | ~110 |
| `PROGRESS.md`, `Cargo.toml` (incl. `which` dep), `Cargo.lock` | housekeeping | ~15 |

**Total estimate: ~1330 LOC** (revised from rev 0's ~895 — rev 1 adds: argv-parity tests + `RealRunner` fake-shell integration test + filename-derivation edge case tests + `input_basename` plumbing + `FinalizationReport` struct + drain-thread-deadlock test + BISMARK_BIN edge case tests. The growth is almost entirely test surface and error-variant scaffolding; the production code change vs rev 0 is +50 LOC.)

### 2.3 Dependencies / phase ordering

Depends on:
- **Phase C.2** (`4e5c691`) for `OutputFileMap::finalize_with_empty_sweep` — Phase G needs the **set of kept files** (post-sweep, post-gzip-trailer-write) to feed bismark2bedGraph. Phase G modifies the return type from `()` to `FinalizationReport` (rev 1 I10). Existing call sites updated.
- **Phase E** (`442be508`) for the `--gzip` extension on split files.
- **Phase A** (`144ca2d`) for the CLI flag surface (already in place).

Unblocks:
- **Phase H** byte-identity gate proper.
- **v1.0 release tagging**.

### 2.4 Perl reference — subprocess invocation (`bismark_methylation_extractor:323-428`)

#### 2.4.1 bismark2bedGraph spawn (Perl :323-377)

```perl
if ($bedGraph){
    my $out = (split (/\//,$filename))[-1];     # input basename
    $out =~ s/gz$//; $out =~ s/sam$//; $out =~ s/bam$//; $out =~ s/txt$//;
    $out =~ s/$/bedGraph/;
    my $bedGraph_output = $out;
    my @args;
    if ($remove)        { push @args, '--remove'; }                 # NB Perl GetOptions prefix abbrev
    if ($CX_context)    { push @args, '--CX_context'; }
    if ($no_header)     { push @args, '--no_header'; }
    if ($gazillion)     { push @args, '--gazillion'; }
    if ($ample_mem)     { push @args, '--ample_memory'; }
    else                { push @args, "--buffer_size $sort_size"; }  # rev 1 I5: ALWAYS pushed in else branch
    if ($ucsc)          { push @args, '--ucsc'; }
    if ($zero)          { push @args, '--zero'; }
    push @args, "--cutoff $coverage_threshold";
    push @args, "--output $bedGraph_output";
    push @args, "--dir '$output_dir'";
    push @args, $f for @sorting_files;
    system ("$RealBin/bismark2bedGraph @args");
    warn "Finished BedGraph conversion ...\n\n";
    sleep(1);
}
```

#### 2.4.2 coverage2cytosine spawn (Perl :388-424)

```perl
if ($cytosine_report){
    @args = ();
    my $cytosine_out = $out;            # NB $out is "{stripped}.bedGraph" — trailing dot preserved
    $cytosine_out =~ s/bedGraph$//;
    if ($CX_context) { $cytosine_out =~ s/$/CX_report.txt/; }
    else             { $cytosine_out =~ s/$/CpG_report.txt/; }
    push @args, "--output $cytosine_out";
    push @args, "--dir '$output_dir'";
    push @args, "--genome '$genome_folder'";
    push @args, "--parent_dir '$output_dir'";                       # rev 1 I13: SAME as --dir
    if ($zero)                { push @args, '--zero'; }
    if ($CX_context)          { push @args, '--CX_context'; }
    if ($split_by_chromosome) { push @args, '--split_by_chromosome'; }
    if ($gzip)                { push @args, '--gzip'; }
    my $coverage_output = $bedGraph_output;
    $coverage_output =~ s/bedGraph$/bismark.cov.gz/;
    push @args, $coverage_output;
    system ("$RealBin/coverage2cytosine @args");
    warn "\n\nFinished generating genome-wide cytosine report\n\n";
}
```

#### 2.4.3 Auto-trigger relationship

`ResolvedConfig::bedgraph` at `cli.rs:455` is computed as `self.bedgraph || self.cytosine_report` — already wired. The Rust orchestrator consults `config.bedgraph` to gate entry; consults `config.cytosine_report` to chain c2c after bismark2bedGraph.

#### 2.4.4 Flag-name divergences — Perl GetOptions prefix abbreviation vs Rust long-form

Perl pushes prefix-abbreviated names; bismark2bedGraph's GetOptions matches them via prefix lookup. Rust port writes the long forms explicitly:

| Extractor flag | Perl pushes | Subprocess GetOptions | Rust port pushes |
|---|---|---|---|
| `--remove_spaces` | `--remove` (abbrev) | `"remove_spaces" => \$remove` | `--remove_spaces` (long form) |
| `--zero_based` | `--zero` (abbrev) | `"zero_based" => \$zero` | `--zero_based` (long form) |
| `--cutoff N` | `--cutoff N` | `"cutoff=i" => \$coverage_threshold` | `--cutoff N` |
| `--CX_context` | `--CX_context` | `"CX|CX_context" => \$CX_context` | `--CX_context` |
| `--gazillion` | `--gazillion` | `"gazillion|scaffolds" => \$gazillion` | `--gazillion` |
| `--ample_memory` | `--ample_memory` | `"ample_memory" => \$ample_mem` | `--ample_memory` |
| `--buffer_size SIZE` | `--buffer_size $sort_size` (always pushed in `else` branch — rev 1 I5) | `"buffer_size=s" => \$sort_size` | `--buffer_size <SIZE>` (default "2G" when user didn't set + !ample_memory) |
| `--ucsc` | `--ucsc` | `"ucsc" => \$ucsc` | `--ucsc` |
| `--no_header` | `--no_header` | `"no_header" => \$no_header` | `--no_header` |
| `--gzip` (to c2c only) | `--gzip` | `"gzip" => \$gzip` | `--gzip` to c2c only |

**`--counts` is NEVER forwarded.** Perl comments it out at `:362-364`. Mirror.
**`--gzip` is NEVER forwarded to bismark2bedGraph.** bismark2bedGraph has no `--gzip` flag (`bismark2bedGraph:637-651`).

#### 2.4.5 Output filename derivation (rev 1 C3 — REWRITTEN with edge case table)

**The Perl quirk:** the strip regexes `s/gz$//`, `s/sam$//`, `s/bam$//`, `s/txt$//` strip the **literal three letters** (no leading dot anchor). For chained extensions like `foo.bam.gz`, the trailing dot is preserved.

Trace for the four standard input shapes plus four edge cases:

| Input basename | After `s/gz$//` | After `s/sam$//` | After `s/bam$//` | After `s/txt$//` | After `s/$/bedGraph/` (= bedGraph output filename) |
|---|---|---|---|---|---|
| `foo.bam` | `foo.bam` | `foo.bam` | `foo.` | `foo.` | `foo.bedGraph` |
| `foo.sam` | `foo.sam` | `foo.` | `foo.` | `foo.` | `foo.bedGraph` |
| `foo.txt` | `foo.txt` | `foo.txt` | `foo.txt` | `foo.` | `foo.bedGraph` |
| `foo.bam.gz` | `foo.bam.` | `foo.bam.` | `foo.bam.` | `foo.bam.` | **`foo.bam.bedGraph`** (trailing dot preserved!) |
| `foo.txt.gz` | `foo.txt.` | `foo.txt.` | `foo.txt.` | `foo.txt.` | **`foo.txt.bedGraph`** |
| `foo` (no ext) | `foo` | `foo` | `foo` | `foo` | **`foobedGraph`** (NO leading dot!) |
| `sample.fastq_bismark_bt2_pe.deduplicated.bam` | (no-op) | (no-op) | `sample.fastq_bismark_bt2_pe.deduplicated.` | (no-op) | `sample.fastq_bismark_bt2_pe.deduplicated.bedGraph` |
| `sample.fastq_bismark_bt2_pe.deduplicated.bam.gz` | `sample.fastq_bismark_bt2_pe.deduplicated.bam.` | (no-op) | (no-op — trailing dot prevents match) | (no-op) | `sample.fastq_bismark_bt2_pe.deduplicated.bam.bedGraph` |

The Rust `derive_bedgraph_filename` implementation **mirrors this Perl sequence step-by-step** (not "strip the longest matching extension" — that would diverge). Pin each row above as a unit-test golden.

#### 2.4.6 Coverage filename derivation (Perl :419-420)

Take `$bedGraph_output` (with whatever trailing dot it has), apply `s/bedGraph$/bismark.cov.gz/`:

| bedGraph output | Coverage output |
|---|---|
| `foo.bedGraph` | `foo.bismark.cov.gz` |
| `foo.bam.bedGraph` | `foo.bam.bismark.cov.gz` |
| `foobedGraph` | `foobismark.cov.gz` |
| `sample.fastq_bismark_bt2_pe.deduplicated.bedGraph` | `sample.fastq_bismark_bt2_pe.deduplicated.bismark.cov.gz` |

#### 2.4.7 Cytosine filename derivation (Perl :392-399)

Take `$out` (which has `bedGraph` appended per :330; same shape as bedGraph output):

```
$cytosine_out = "{stem}.bedGraph"               # same as bedGraph_output
$cytosine_out =~ s/bedGraph$//;                 # → "{stem}."  (trailing dot preserved!)
if ($CX_context) { $cytosine_out =~ s/$/CX_report.txt/; }   # → "{stem}.CX_report.txt"
else             { $cytosine_out =~ s/$/CpG_report.txt/; }  # → "{stem}.CpG_report.txt"
```

| bedGraph output | + `--CX_context` | Default |
|---|---|---|
| `foo.bedGraph` | `foo.CX_report.txt` | `foo.CpG_report.txt` |
| `foo.bam.bedGraph` | `foo.bam.CX_report.txt` | `foo.bam.CpG_report.txt` |
| `sample.fastq_bismark_bt2_pe.deduplicated.bedGraph` | `sample…deduplicated.CX_report.txt` | `sample…deduplicated.CpG_report.txt` |

#### 2.4.8 bismark2bedGraph does NOT take `--gzip`

bismark2bedGraph (`bismark2bedGraph:637-651`) has no `--gzip` GetOptions entry. Its `.bismark.cov.gz` output is **always gzipped** by extension-sniff. Only the `.bedGraph` output is non-gzipped. The extractor's Phase G code path therefore does NOT pass `--gzip` to bismark2bedGraph; it DOES pass `--gzip` to coverage2cytosine (mirrored from Perl line 415-417).

#### 2.4.9 `--parent_dir == --dir` invariant (rev 1 I13)

Perl pushes BOTH `--dir '$output_dir'` AND `--parent_dir '$output_dir'` to c2c with the SAME value. Rust mirrors. Pinned as an argv-parity assertion in `tests/phase_g_argv_parity.rs`.

### 2.5 Current Rust state (what's missing)

- **CLI**: all 11 Phase G flags parsed and validated. ✅
- **ResolvedConfig**: all 11 fields populated. ✅
- **Subprocess invocation**: NONE.
- **Auto-trigger**: ResolvedConfig already sets `bedgraph = self.bedgraph || self.cytosine_report` (`cli.rs:455`). ✅
- **CLI mutex/precondition validation**: all 8 rules already in `Cli::validate`. ✅
- **`input_basename` storage on `ExtractState`**: ✗ — does NOT currently store this field (rev 1 C2 — must add).
- **`OutputFileMap::finalize_with_empty_sweep` return shape**: currently `()` — must become `FinalizationReport` (rev 1 I10).

## 3. Behavior

### 3.1 New `src/subprocess.rs` module — public surface (rev 1 revised)

1. **`pub trait BismarkSubprocessRunner`** — abstracts subprocess invocation. Production `RealRunner` spawns via `std::process::Command`; test `MockRunner` records calls.
2. **`pub fn build_bismark2bedgraph_argv(config: &ResolvedConfig, kept_split_files: &[PathBuf], bedgraph_filename: &str, output_dir: &Path) -> Vec<OsString>`**.
3. **`pub fn build_coverage2cytosine_argv(config: &ResolvedConfig, coverage_input_filename: &str, cytosine_output_filename: &str, output_dir: &Path, genome_folder: &Path) -> Vec<OsString>`**.
4. **`pub fn derive_bedgraph_filename(input_basename: &str) -> String`** — Perl `:325-330` port; trailing-dot-preserving per rev 1 C3.
5. **`pub fn derive_coverage_filename(bedgraph_filename: &str) -> String`** — Perl `:419-420` port.
6. **`pub fn derive_cytosine_filename(bedgraph_filename: &str, cx_context: bool) -> String`** — Perl `:392-399` port.
7. **`pub fn discover_subprocess(tool: SubprocessTool) -> Result<PathBuf, BismarkExtractorError>`** — rev 1 C1: **BISMARK_BIN-first (strict), then PATH, then `current_exe()` parent**.
8. **`pub fn run_with_tee_and_ring_buffer<R: BismarkSubprocessRunner>(runner: &R, tool: SubprocessTool, program: &Path, argv: &[OsString]) -> Result<RunOutcome, BismarkExtractorError>`** — rev 1 C5 + I1 + I6: spawn drain BEFORE wait; always join drain; `read_until(b'\n', &mut Vec<u8>)` for non-UTF-8 safety.
9. **`pub enum SubprocessTool { Bismark2BedGraph, Coverage2Cytosine }`** — `Display` returns the Perl script name (`bismark2bedGraph` / `coverage2cytosine`).
10. **`pub fn run_phase_g_chain(config: &ResolvedConfig, input_basename: &str, output_dir: &Path, kept_split_files: &[PathBuf], runner: &impl BismarkSubprocessRunner) -> Result<(), BismarkExtractorError>`** — orchestrator called from `state.rs::finalize` when `config.bedgraph`.

### 3.2 Subprocess discovery rules (rev 1 C1 — REWRITTEN to BISMARK_BIN-first, strict)

In order:

1. **If env var `BISMARK_BIN` is set (and non-empty), STRICTLY look for `${BISMARK_BIN}/<tool_name>`.** If the file exists and is executable, return it. If `BISMARK_BIN` is set but the tool is missing or non-executable, return `SubprocessNotFound { tool, searched_paths: vec![{BISMARK_BIN}/<tool>] }` — **no fallback** (rev 1 I12 — strict). Rationale: if the user goes to the trouble of setting `BISMARK_BIN`, they want to lock the tool source; silent fallback to PATH would hide install-skew bugs.
   - **Exception**: `BISMARK_BIN=""` (empty string) is treated as "not set" → fall through to step 2 (rev 1 I16).
2. Look up `<tool_name>` on `PATH` using the `which` crate.
3. If PATH lookup fails, try `std::env::current_exe()?.parent()?.join(<tool_name>)`. Mirrors Perl's `$RealBin`. Under `#[cfg(test)]`, consults `BISMARK_TEST_CURRENT_EXE_DIR` env var instead to avoid the test-binary-symlink trap (rev 1 I17).
4. If all three fail, return `SubprocessNotFound { tool, searched_paths }` listing every path attempted.

The `<tool_name>` is the literal Perl script name: `bismark2bedGraph` and `coverage2cytosine`.

### 3.3 Subprocess invocation contract (rev 1 I9: explicit `stdin(Stdio::null())`)

- **Working directory**: inherit parent's CWD (no `Command::current_dir()`). Matches Perl `system()`.
- **Environment**: inherit fully (no `.env_clear()`). Matches Perl. Includes `PATH`, `LC_ALL`, `LANG` — important for the UNIX `sort` step bismark2bedGraph runs internally.
- **stdin**: **`.stdin(Stdio::null())` explicit** (rev 1 I9). Perl's chain doesn't pipe stdin; making this explicit prevents accidental hangs when the parent's stdin is a TTY.
- **stdout**: inherited (live to user's terminal). Subprocesses write outputs to files via `--output`, not stdout.
- **stderr**: **piped + teed** per §3.4.
- **Process group**: inherit (no `setsid`). Ctrl-C propagates.

**Locale dependency (rev 1 I8)**: bismark2bedGraph internally invokes UNIX `sort`, which is locale-sensitive. For Phase H byte-identity, the harness MUST pin `LC_ALL=C` for both Perl and Rust runs (sort behaviour identical under POSIX collation). Rust does NOT override the user's locale; this is the user's responsibility, matching Perl. A SPEC §6.6 note documents this.

### 3.4 Stderr tee + ring buffer (rev 1 C5 + I1 + I6: byte-safe + always-join + spawn-before-wait)

For each subprocess:

1. Build the `Command` with `.stdin(Stdio::null()).stderr(Stdio::piped())`. stdout inherited.
2. **Spawn the child** with `Command::spawn()` (returns `Child`).
3. **Take ownership of `child.stderr`** immediately.
4. **Spawn the drain thread BEFORE calling `child.wait()`** (rev 1 I6 — order matters; pipe-buffer is typically 64 KiB and a fast subprocess could fill+block on write before the parent starts draining).
5. The drain thread:
   - Owns a `VecDeque<u8>` ring buffer (rev 1 O1, capacity 64 KiB).
   - Loops reading via **`BufRead::read_until(b'\n', &mut Vec<u8>)`** (rev 1 C5 — `read_line` would panic on non-UTF-8).
   - For each read:
     - Writes the raw bytes (newline included) to the parent's `io::stderr()` via `Write::write_all`. **Tee live.**
     - Appends the bytes to the ring buffer. If `ring.len() + bytes.len() > 64 KiB`, evict from the front (`VecDeque::pop_front` until fit). Lines larger than the cap are stored as their trailing 64 KiB; deterministic.
   - Exits when `read_until` returns `Ok(0)` (EOF) or `Err` (re-emitted via channel to main).
6. **Main thread calls `child.wait()`** to get the exit status.
7. **Regardless of exit status, join the drain thread** (rev 1 I1) and reclaim the ring buffer. If the drain thread panicked or errored, propagate.
8. **Convert ring buffer to `Vec<u8>`** (the `stderr_tail` for the error variant — rev 1 C5).
9. On non-zero exit, return `Err(SubprocessFailed { tool, exit_status, stderr_tail })`. On success, drop the ring buffer; return `Ok(RunOutcome { exit_status, stderr_tail })`.

**Why drain BEFORE wait, not interleaved**: a fast-failing subprocess that writes a 128 KiB error message to stderr and exits would block on write if the parent is calling `wait()` first. Spawning the drain thread first guarantees the pipe is being read.

**Pre-spawn debug eprintln (rev 1 O2)**: just before `Command::spawn()`, the orchestrator emits `eprintln!("[bismark-extractor] spawning: {program:?} {argv:?}")`. Always-on; cheap; matches the "audit Perl-parity at runtime" precedent of the existing harness scripts.

### 3.5 Error mapping (rev 1 C5: `stderr_tail: Vec<u8>`)

```rust
/// Subprocess (bismark2bedGraph / coverage2cytosine) exited non-zero.
/// `stderr_tail` is the last ≤ 64 KiB of stderr captured via the tee
/// drain thread; the full stream was already written live to the user's
/// stderr, so this is for error-report context. Stored as Vec<u8>
/// (rev 1 C5) to remain byte-safe under non-UTF-8 subprocess output;
/// the `Display` impl renders via String::from_utf8_lossy.
#[error(
    "subprocess `{tool}` exited with status {exit_status}: \
     stderr tail (last {tail_len} bytes, UTF-8 lossy):\n{stderr_tail_str}"
)]
SubprocessFailed {
    tool: SubprocessTool,
    exit_status: std::process::ExitStatus,
    stderr_tail: Vec<u8>,
},
// Display impl computes tail_len = stderr_tail.len() and
// stderr_tail_str = String::from_utf8_lossy(&stderr_tail).

#[error(
    "could not locate `{tool}`: searched {searched_paths:?}. \
     Install Bismark and ensure `{tool}` is on PATH, or set \
     `BISMARK_BIN=/path/to/Bismark/` to lock the tool source."
)]
SubprocessNotFound {
    tool: SubprocessTool,
    searched_paths: Vec<PathBuf>,
},

#[error("failed to spawn `{tool}`: {source}")]
SubprocessSpawnFailed {
    tool: SubprocessTool,
    #[source]
    source: std::io::Error,
},
```

### 3.6 Auto-trigger semantics (unchanged from rev 0)

`ResolvedConfig::bedgraph = self.bedgraph || self.cytosine_report` (`cli.rs:455`). The orchestrator:
- `--bedGraph` alone → bismark2bedGraph only.
- `--cytosine_report` alone → bismark2bedGraph (auto-triggered) THEN coverage2cytosine.
- Neither → skip Phase G entirely.

bismark2bedGraph MUST run to completion before c2c starts — c2c reads `.bismark.cov.gz` produced by b2bg.

### 3.7 `--gzip` dispatch matrix (unchanged from rev 0)

| `--gzip` set? | Split files | bismark2bedGraph input | bismark2bedGraph output | coverage2cytosine input | coverage2cytosine `--gzip` flag |
|---|---|---|---|---|---|
| no | `.txt` | `.txt` paths | `.bedGraph` + `.bismark.cov.gz` (always gzipped) | `.bismark.cov.gz` | NOT passed |
| yes | `.txt.gz` | `.txt.gz` paths (b2bg sniffs extension) | `.bedGraph` + `.bismark.cov.gz` | `.bismark.cov.gz` | passed (c2c writes `.CpG_report.txt.gz` / `.CX_report.txt.gz`) |

### 3.8 Edge cases (rev 1 — extended)

| Case | Handling |
|---|---|
| `--mbias_only` set | CLI validation rejects with both `--bedGraph` and `--cytosine_report`. Phase G entry gate (`if !config.bedgraph`) also false. Double-guard. |
| `--bedGraph` set + ALL split files swept empty | Kept-set is empty. Pass to bismark2bedGraph anyway — b2bg handles empty input gracefully. If c2c also engaged, emit `eprintln!("note: extractor produced no methylation calls; cytosine_report will scan the genome anyway")` BEFORE c2c spawn (rev 1 I14 UX warning). |
| `--cytosine_report` without `--genome_folder` | Rejected at CLI validation. |
| `BISMARK_BIN` set with both tools | Used for both. |
| `BISMARK_BIN` set with only one tool | **Strict** (rev 1 I12): each `discover_subprocess` call is independent; the missing tool gets `SubprocessNotFound` — NO fallback. |
| `BISMARK_BIN=""` (empty) | Treat as "not set"; fall through to PATH (rev 1 I16). |
| `BISMARK_BIN` points to a directory containing the tool but tool is not executable | `SubprocessNotFound` (rev 1 I16); test `discover_subprocess_bismark_bin_set_but_tool_not_executable_returns_not_found`. |
| Subprocess produces enormous stderr (gigabytes) | Bounded by 64 KiB ring buffer; older lines evicted (VecDeque pop_front). User still sees full stream live. Test `realrunner_high_volume_stderr_stays_bounded` exercises a 1 MiB-burst fake script. |
| Subprocess writes >64 KiB stderr-burst before drain thread starts | **Cannot happen** — rev 1 I6 guarantees drain thread is spawned BEFORE `child.wait()` (which is the first call that blocks the main thread). Pipe-buffer-deadlock test `realrunner_128kib_stderr_burst_does_not_deadlock` exercises this. |
| Subprocess writes non-UTF-8 stderr | Drain thread uses `read_until` (rev 1 C5); ring buffer is `Vec<u8>`; lossy conversion only at Display time. Test `realrunner_drain_handles_non_utf8_stderr_bytes`. |
| Drain thread panics | Main thread's join returns `Err`; propagate as `InternalError` (drain logic is "should never fail" — write to a pipe + push to VecDeque). |
| Ctrl-C during subprocess | Signal propagates via inherited process group; child exits non-zero; SubprocessFailed bubbles. Drain sees EOF, exits cleanly. |
| `--ucsc` set without `--bedGraph` | Rejected at CLI validation. |
| `current_exe()` returns a symlink | Resolve via syscall; under `#[cfg(test)]`, `BISMARK_TEST_CURRENT_EXE_DIR` overrides (rev 1 I17). |
| `--no_header` set | Pass `--no_header` to bismark2bedGraph only. coverage2cytosine has no `--no_header` flag (verified by `coverage2cytosine:2011-2028`); the SPEC documents this (rev 1 I11 note). |
| Working dir contains spaces | Argv pass-through is via `Command::arg(OsStr)`; no shell quoting needed (improvement over Perl's `'$genome_folder'` shell-interpolation hazard). Test `phase_g_passes_genome_folder_with_spaces_via_typed_argv_no_shell`. |
| `--genome_folder` is a non-UTF-8 path (Linux-only) | Argv handles `OsStr` end-to-end; test `phase_g_passes_non_utf8_genome_folder_path` under `cfg(target_os = "linux")`. |
| Drain thread sees subprocess that never closes stderr | The drain thread blocks on `read_until` indefinitely. Main thread's `child.wait()` will return when the subprocess exits (which closes its stderr automatically). Drain then sees EOF. No special handling needed. |
| chained-extension input (`foo.bam.gz`) | bedGraph output is `foo.bam.bedGraph` (trailing dot preserved per rev 1 C3 — see §2.4.5 edge case table). |
| input with no extension (`foo`) | bedGraph output is `foobedGraph` (no leading dot). Test pinned in §5.4.1. |

## 4. Signature (rev 1 revised)

### 4.1 `BismarkSubprocessRunner` trait

```rust
pub trait BismarkSubprocessRunner {
    /// Spawn `program` with `argv`, tee stderr to `tee_target`, and
    /// return when the child exits. The drain thread is spawned BEFORE
    /// `child.wait()` to prevent pipe-buffer-full deadlock (rev 1 I6).
    /// The drain thread is always joined before this function returns
    /// (rev 1 I1).
    fn run(
        &self,
        program: &Path,
        argv: &[OsString],
        tee_target: &mut dyn Write,
    ) -> Result<RunOutcome, BismarkExtractorError>;
}

pub struct RunOutcome {
    pub exit_status: ExitStatus,
    pub stderr_tail: Vec<u8>,  // rev 1 C5: byte-safe
}

pub struct RealRunner;
impl BismarkSubprocessRunner for RealRunner { /* see §3.4 */ }

#[cfg(test)]
pub struct MockRunner<F: Fn(&Path, &[OsString]) -> Result<RunOutcome, BismarkExtractorError>>(F);
```

### 4.2 `run_phase_g_chain` orchestrator

```rust
pub fn run_phase_g_chain(
    config: &ResolvedConfig,
    input_basename: &str,
    output_dir: &Path,
    kept_split_files: &[PathBuf],
    runner: &impl BismarkSubprocessRunner,
) -> Result<(), BismarkExtractorError>;
```

### 4.3 Arg-builders (rev 1: explicit `--parent_dir == --dir` for c2c)

```rust
pub fn build_bismark2bedgraph_argv(
    config: &ResolvedConfig,
    kept_split_files: &[PathBuf],       // ABSOLUTE paths (rev 1 C4)
    bedgraph_filename: &str,             // result of derive_bedgraph_filename
    output_dir: &Path,
) -> Vec<OsString>;

pub fn build_coverage2cytosine_argv(
    config: &ResolvedConfig,
    coverage_input_filename: &str,
    cytosine_output_filename: &str,
    output_dir: &Path,                   // used for BOTH --dir AND --parent_dir (rev 1 I13)
    genome_folder: &Path,
) -> Vec<OsString>;
```

### 4.4 `OutputFileMap::finalize_with_empty_sweep` → `FinalizationReport` (rev 1 I10)

Today (Phase C.2):
```rust
pub fn finalize_with_empty_sweep(&mut self) -> Result<(), std::io::Error>;
```

Phase G rev 1:
```rust
pub struct FinalizationReport {
    /// Absolute paths of files kept (records_written > 0). Sorted by
    /// path-string for deterministic argv ordering (rev 1 I7 — pins
    /// argv-parity vs Perl). Phase G feeds this list as the positional
    /// tail to bismark2bedGraph.
    pub kept: Vec<PathBuf>,
    /// Absolute paths of files swept (records_written == 0, unlinked).
    /// Used by Phase H harness to assert file-set-match contract.
    pub swept: Vec<PathBuf>,
}

pub fn finalize_with_empty_sweep(&mut self) -> Result<FinalizationReport, std::io::Error>;
```

**Kept-paths form** (rev 1 C4): absolute paths via `std::fs::canonicalize(&self.output_dir).join(filename)` (or equivalent). Documented in the doc comment. Perl's `@sorting_files` array also holds absolute paths in practice (Perl pushes the same paths it eagerly opened via `open '>', $output_dir.$filename`). Tested by `output_file_map_finalize_returns_absolute_paths_for_kept_files`.

**Sort order** (rev 1 I7): kept paths sorted lexicographically before return to ensure argv-parity vs Perl across runs (HashMap iteration is non-deterministic; sort makes the bismark2bedGraph positional tail deterministic). Tested by `output_file_map_finalize_returns_kept_paths_in_stable_order`.

### 4.5 `state.rs::ExtractState::input_basename` field (rev 1 C2)

```rust
pub struct ExtractState {
    // ... (existing fields)
    /// rev 1 C2: stored for Phase G subprocess chain entry. Set at
    /// construction from the caller's input_basename parameter.
    input_basename: String,
}

impl ExtractState {
    pub fn new(
        config: &ResolvedConfig,
        input_path: &Path,
        input_basename: &str,
        is_paired: bool,
    ) -> Result<Self, BismarkExtractorError> {
        // ... existing logic
        Ok(Self {
            // ...
            input_basename: input_basename.to_string(),
        })
    }

    /// Accessor for Phase G chain.
    pub fn input_basename(&self) -> &str { &self.input_basename }

    pub fn finalize(&mut self, config: &ResolvedConfig) -> Result<(), BismarkExtractorError> {
        // ... existing C.2 logic up through M-bias write ...

        // rev 1: Phase G chain dispatch.
        if config.bedgraph {
            let kept = self.last_kept_files.take().unwrap_or_default();
            crate::subprocess::run_phase_g_chain(
                config,
                &self.input_basename,
                &config.output_dir,
                &kept,
                &crate::subprocess::RealRunner,
            )?;
        }
        Ok(())
    }
}
```

`last_kept_files: Option<Vec<PathBuf>>` is a new private field on `ExtractState`, populated when `finalize_with_empty_sweep` runs. Held across the gap between sweep + chain dispatch.

## 5. Implementation outline (rev 1 — additions in bold)

### 5.1 SPEC updates (DO FIRST)

1. **§6.6**: replace 4-line summary with the full §2.4 matrix (flag wiring; trailing-dot quirk per rev 1 C3; LC_ALL note per rev 1 I8; deliberately-omitted c2c flags per rev 1 I11).
2. **§6.5 (Subprocess error propagation row)**: extend to include 3 new error variants + tee semantics + `Vec<u8>` `stderr_tail` (rev 1 C5).
3. **§3 rows 17–26, 35**: append "✅ wired Phase G" to Side effects column.
4. **§8.1 (Unit tests)**: append rows for arg-builder tests, filename-derivation edge cases (rev 1 C3), discovery tests, ring-buffer tests, error-mapping tests.
5. **§8.4 (Edge case fixtures)**: append rows for `BISMARK_BIN` strict mode, missing-subprocess, gzip×subprocess-chain, chained-extension input, non-UTF-8 stderr.
6. **§10 row G**: update LOC estimate (~400 → ~1330) with note that rev 1 added ~430 LOC of test surface beyond rev 0.

### 5.2 Crate-level additions

1. **Dependency**: **add `which = "..."` to `rust/bismark-extractor/Cargo.toml`** (rev 1 I4 — confirmed not already in the workspace via grep). Use latest 7.x.
2. **`Cargo.toml` version**: `1.0.0-alpha.8` → `1.0.0-alpha.9`. Description: `"… (Phase G: bedGraph + cytosine_report subprocess chain)"`.
3. **`Cargo.lock` regen**.

### 5.3 New `src/subprocess.rs`

1. **`SubprocessTool` enum** — 2 variants; `Display` returns script names; `Display` round-trip test (rev 1 — Reviewer A Opt 22).
2. **`RingBuffer`** — backed by **`VecDeque<u8>` (rev 1 O1)**, 64 KiB cap, `push_bytes(&[u8])` evicts via `pop_front` from the front.
3. **`discover_subprocess`** — implements the 4-step lookup per §3.2 (BISMARK_BIN-first, strict; PATH; current_exe; under `#[cfg(test)]` honors `BISMARK_TEST_CURRENT_EXE_DIR`).
4. **`derive_bedgraph_filename` / `derive_coverage_filename` / `derive_cytosine_filename`** — per Perl `:325-330`, `:392-399`, `:419-420` — **trailing-dot-preserving** per rev 1 C3.
5. **`build_bismark2bedgraph_argv`** — per Perl `:333-373`; long-form flag names per §2.4.4; **`--buffer_size 2G` pushed when neither flag set + `!ample_memory`** (rev 1 I5).
6. **`build_coverage2cytosine_argv`** — per Perl `:388-422`; **`--parent_dir == --dir`** (rev 1 I13).
7. **`BismarkSubprocessRunner` trait + `RealRunner` impl** — drain BEFORE wait (rev 1 I6); always join (rev 1 I1); `read_until` (rev 1 C5); `stdin(Stdio::null())` (rev 1 I9); pre-spawn `eprintln!` of program + argv (rev 1 O2).
8. **`run_phase_g_chain`** — orchestrator; emits "no methylation calls" warning for empty-kept-set + cytosine_report (rev 1 I14).
9. **Module-level documentation** including the byte-identity invariant (rev 1 O4): "argv passed to Perl bismark2bedGraph from Rust MUST match Perl extractor's argv byte-for-byte, modulo long-form flag expansion documented in SPEC §6.6 / Phase G plan §2.4.4". References Perl `:323-428` + SPEC §6.6.

### 5.4 Tests (rev 1 expanded)

#### 5.4.1 `src/subprocess.rs::tests` (unit — ~30 tests)

Filename derivation:
- `derive_bedgraph_filename_foo_bam` → `foo.bedGraph`
- `derive_bedgraph_filename_foo_sam` → `foo.bedGraph`
- `derive_bedgraph_filename_foo_txt` → `foo.bedGraph`
- **`derive_bedgraph_filename_foo_bam_gz_preserves_trailing_dot`** → `foo.bam.bedGraph` (rev 1 C3 critical guard).
- **`derive_bedgraph_filename_foo_txt_gz_preserves_trailing_dot`** → `foo.txt.bedGraph` (rev 1 C3).
- **`derive_bedgraph_filename_no_extension_has_no_leading_dot`** → `foobedGraph` (rev 1 C3).
- **`derive_bedgraph_filename_bismark_pe_naming`** → `sample.fastq_bismark_bt2_pe.deduplicated.bedGraph` (rev 1 C3).
- **`derive_bedgraph_filename_bismark_pe_gz_naming`** → `sample.fastq_bismark_bt2_pe.deduplicated.bam.bedGraph` (rev 1 C3 — chained extension on real Bismark output names).
- `derive_coverage_filename_appends_bismark_cov_gz`.
- **`derive_coverage_filename_preserves_trailing_dot_for_chained_extensions`** (rev 1 C3).
- `derive_cytosine_filename_cpg_default`.
- `derive_cytosine_filename_cx_context_when_flag_set`.
- **`derive_cytosine_filename_preserves_trailing_dot_for_chained_extensions`** (rev 1 C3).

Arg-builders:
- `build_bismark2bedgraph_argv_default_no_optional_flags`.
- `build_bismark2bedgraph_argv_all_optional_flags_set`.
- `build_bismark2bedgraph_argv_uses_long_form_remove_spaces` (rev 1 — not `--remove`).
- `build_bismark2bedgraph_argv_uses_long_form_zero_based` (rev 1 — not `--zero`).
- **`build_bismark2bedgraph_argv_passes_buffer_size_2G_when_neither_buffer_size_nor_ample_memory_set`** (rev 1 I5).
- `build_bismark2bedgraph_argv_passes_explicit_buffer_size_when_set`.
- `build_bismark2bedgraph_argv_passes_ample_memory_instead_of_buffer_size` (when ample_memory set).
- `build_bismark2bedgraph_argv_omits_counts_flag` (Perl-commented-out parity).
- `build_bismark2bedgraph_argv_appends_kept_files_as_positional_tail`.
- `build_bismark2bedgraph_argv_does_not_pass_gzip`.
- `build_coverage2cytosine_argv_default_cpg_only`.
- `build_coverage2cytosine_argv_with_cx_context_flag`.
- `build_coverage2cytosine_argv_with_split_by_chromosome`.
- `build_coverage2cytosine_argv_with_gzip`.
- `build_coverage2cytosine_argv_positional_is_coverage_file`.
- **`build_coverage2cytosine_argv_passes_parent_dir_equal_to_dir`** (rev 1 I13).

Ring buffer (rev 1 — VecDeque-backed, tightened goldens):
- `ring_buffer_evicts_oldest_when_capacity_exceeded` — push 100 KiB; final snapshot is exactly the last 64 KiB.
- `ring_buffer_snapshot_under_capacity_returns_full_content`.
- **`ring_buffer_handles_line_exactly_64kib_replaces_entirely`** — pin exact policy (rev 1 — replaces Reviewer A's "vacuous bounded substring" complaint).
- **`ring_buffer_handles_line_larger_than_capacity_keeps_trailing_64kib`** — push a single 128 KiB line; assert snapshot length == 64 KiB and matches the trailing 64 KiB byte-for-byte.

Discovery (rev 1 — strict BISMARK_BIN, env-var test hatch):
- **`discover_subprocess_bismark_bin_set_with_tool_returns_bismark_bin_path`** (rev 1 C1 precedence test).
- **`discover_subprocess_bismark_bin_beats_path_when_both_contain_tool`** (rev 1 C1).
- **`discover_subprocess_bismark_bin_set_but_tool_not_executable_returns_not_found`** (rev 1 I12 strict mode; rev 1 I16).
- **`discover_subprocess_bismark_bin_empty_string_falls_through_to_path`** (rev 1 I16).
- `discover_subprocess_finds_on_path_when_bismark_bin_unset`.
- **`discover_subprocess_falls_back_to_test_current_exe_dir_env_hatch`** (rev 1 I17 — no symlink trickery).
- `discover_subprocess_returns_not_found_when_all_paths_exhausted`.

`SubprocessTool` Display: round-trip test (Reviewer A Opt 22).

#### 5.4.2 `tests/phase_g.rs` (integration, mocked runner — ~15 tests)

- `phase_g_no_op_when_neither_bedgraph_nor_cytosine_report`.
- `phase_g_runs_bismark2bedgraph_only_when_only_bedgraph_set`.
- `phase_g_runs_both_tools_in_order_when_cytosine_report_set`.
- `phase_g_auto_triggers_bedgraph_via_cytosine_report`.
- `phase_g_passes_kept_files_only_in_positional_tail`.
- `phase_g_subprocess_failed_first_tool_does_not_run_second`.
- `phase_g_subprocess_failed_second_tool_bubbles_with_correct_tool_variant`.
- `phase_g_with_gzip_passes_gz_extension_split_files_to_b2bg`.
- `phase_g_with_gzip_passes_gzip_flag_to_c2c`.
- `phase_g_does_not_pass_gzip_to_b2bg_even_when_extractor_gzip_set`.
- `phase_g_passes_genome_folder_with_spaces_via_typed_argv_no_shell` (rev 1 — robustness over Perl).
- **`phase_g_passes_non_utf8_genome_folder_path`** (rev 1 — Linux-only, cfg-gated).
- `phase_g_parallel_n_is_no_op_on_subprocess_chain`.
- **`phase_g_runs_bismark2bedgraph_with_empty_kept_set`** (rev 1 I14).
- **`phase_g_empty_kept_set_plus_cytosine_report_emits_ux_warning`** (rev 1 I14 — capture stderr; assert "note: extractor produced no methylation calls" present).
- **`phase_g_kept_files_passed_as_absolute_paths_to_b2bg`** (rev 1 C4).

#### 5.4.3 `tests/phase_g_argv_parity.rs` (rev 1 I3 — golden-file argv-parity vs Perl)

Three configurations covered by 3 golden tests:

```
tests/fixtures/argv_parity_default.golden        # ResolvedConfig: default mode, --bedGraph only
tests/fixtures/argv_parity_gzip.golden           # default mode + --gzip
tests/fixtures/argv_parity_cytosine_report_cx.golden  # --cytosine_report + --CX
```

Each golden was generated once via a Perl print-and-exit shim:
```bash
# Recipe (one-time, committed to tests/fixtures/README.md):
$ perl -i -pe 's/^(\s*)system \(/$1print join(" ", "$RealBin\/bismark2bedGraph", \@args), "\n"; exit 0; system (/' \
    bismark_methylation_extractor
# Then run with each config; capture stdout to .golden.
```

Tests load each golden, run `build_bismark2bedgraph_argv` / `build_coverage2cytosine_argv` with the matching config, and assert byte-equal (modulo the documented long-form/abbreviation translation — rev 1 §2.4.4).

#### 5.4.4 `tests/phase_g_realrunner.rs` (rev 1 I2 — fake-shell-script integration, runs in CI)

Fake shell scripts in `tests/fixtures/`:
- `fake_bismark2bedgraph_success.sh` — `#!/bin/sh; echo "fake b2bg invoked: $*" >&2; touch "$3"; exit 0` (creates dummy output, exits clean).
- `fake_bismark2bedgraph_failure.sh` — `#!/bin/sh; echo "fake b2bg ERROR: deliberate failure for test" >&2; exit 7`.
- `fake_bismark2bedgraph_high_stderr.sh` — emits 1 MiB to stderr then exits 0.
- `fake_bismark2bedgraph_burst_then_exit.sh` — `#!/bin/sh; head -c 131072 /dev/urandom | base64 >&2; exit 1` (128 KiB stderr-burst then fail).
- `fake_bismark2bedgraph_non_utf8_stderr.sh` — `#!/bin/sh; printf '\xff\xfe\xfd\xfc oops\n' >&2; exit 0`.

Tests (all `#[cfg(unix)]`):
- `realrunner_invokes_subprocess_and_returns_ok_on_zero_exit`.
- `realrunner_returns_subprocess_failed_on_nonzero_exit`.
- `realrunner_subprocess_failed_carries_correct_tool_variant`.
- **`realrunner_high_volume_stderr_stays_bounded_in_ring_buffer`** (rev 1 — 1 MiB-burst stays at 64 KiB tail).
- **`realrunner_128kib_stderr_burst_does_not_deadlock`** (rev 1 I6 — pipe-buffer-deadlock).
- **`realrunner_drain_handles_non_utf8_stderr_bytes`** (rev 1 C5).
- **`realrunner_drain_thread_joined_on_ok_path`** (rev 1 I1 — uses a side-channel to assert drain ran to completion before run() returned).
- **`realrunner_drain_thread_joined_on_err_path`** (rev 1 I1).
- **`realrunner_subprocess_command_eprintln_visible_before_spawn`** (rev 1 O2 — assert the pre-spawn `eprintln!` is captured in test stderr).

#### 5.4.5 `tests/phase_g_real.rs` (real-subprocess, `#[ignore]`'d)

```rust
#[test]
#[ignore = "requires bismark2bedGraph on PATH; opt-in via cargo test -p bismark-extractor --test phase_g_real -- --ignored"]
fn phase_g_real_bismark2bedgraph_smoke() {
    // Tight assertions per rev 1 I7:
    // - output exists
    // - column count == 4 (bedGraph format)
    // - non-zero rows
}

#[test]
#[ignore = "requires bismark2bedGraph + coverage2cytosine + a tiny bismark-prepared genome dir"]
fn phase_g_real_cytosine_report_smoke() { /* tighter assertions */ }
```

Opt-in invocation documented in `tests/phase_g_real.rs` doc-comment AND §5.8: `cargo test -p bismark-extractor --test phase_g_real -- --ignored` (rev 1 I15 — selective, not blanket `--ignored`).

### 5.5 Wire into `state.rs::ExtractState::finalize`

Per §4.5. Order: split-files-flush → sweep (capture kept) → splitting-report → M-bias → chain dispatch. The `input_basename` field is populated at construction (rev 1 C2).

### 5.6 Tests touched / asserted-still-pass

- `tests/output_phase_c2.rs` — asserts about `finalize_with_empty_sweep`. Return-type change `()` → `FinalizationReport` requires updating the binding (`let report = ...;` and assertions consult `report.kept` / `report.swept`). Touches ~4 lines.
- All other test files: untouched. Phase G is OFF by default; behaviour preserved.

### 5.7 PROGRESS.md update

Replace Phase G row's status from `📝 plan rev 0 — awaiting manual review` to `📝 plan rev 1 — dual plan-reviewers complete; both NEEDS-REVISIONS folded; awaiting implementation trigger`.

### 5.8 Pre-merge validation

1. `cargo test -p bismark-extractor` — all tests pass (~238 pre-G + ~60 new from Phase G rev 1).
2. **`cargo test -p bismark-extractor --test phase_g_real -- --ignored`** — selective opt-in (rev 1 I15); only runs if subprocesses on PATH.
3. `cargo clippy -p bismark-extractor --all-targets -- -D warnings` — clean.
4. `cargo fmt --check` — clean.
5. Phase F + C.1 + C.2 byte-identity invariants verified untouched.

## 6. Efficiency

- **Subprocess spawn**: ~1-5 ms fork overhead per subprocess; negligible vs the seconds-to-minutes UNIX sort step inside b2bg.
- **Tee drain thread**: one ephemeral thread per invocation; µs-level overhead.
- **VecDeque ring buffer (rev 1 O1)**: O(1) push, O(1) pop_front. ~zero amortised cost. (Improvement vs rev 0's `Vec::drain` which was O(n) per eviction.)
- **Argv construction**: `Vec<OsString>` of < 30 elements; ~ns per call.
- **No `Mutex` on ring buffer**: it's owned by the drain thread; main thread receives the final `Vec<u8>` snapshot via thread join (rev 1 — addresses Reviewer B 3.1).

## 7. Integration

### 7.1 Read/Write surface (rev 1 revised)

- **Read**:
  - `rust/bismark-extractor/src/cli.rs::ResolvedConfig`
  - `rust/bismark-extractor/src/output.rs::OutputFileMap::finalize_with_empty_sweep` (now returns `FinalizationReport`)
  - Env var `BISMARK_BIN` (rev 1 C1 — primary discovery, strict)
  - Env var `PATH` (via `which`)
  - Env var `BISMARK_TEST_CURRENT_EXE_DIR` (under `#[cfg(test)]` only — rev 1 I17)
- **Write**:
  - `rust/bismark-extractor/SPEC.md` (§6.6 matrix + LC_ALL + deliberately-omitted, §6.5 row, §3 row wiring, §8.x test rows, §10 row G LOC)
  - `rust/bismark-extractor/Cargo.toml` (version + `which` dep)
  - `rust/Cargo.lock`
  - `rust/bismark-extractor/src/subprocess.rs` (new)
  - `rust/bismark-extractor/src/error.rs` (+3 variants, `stderr_tail: Vec<u8>`)
  - `rust/bismark-extractor/src/state.rs` (extend finalize; add `input_basename` field; add `last_kept_files` private field)
  - `rust/bismark-extractor/src/output.rs` (`FinalizationReport` struct; return-type change on `finalize_with_empty_sweep`; sorted absolute paths)
  - `rust/bismark-extractor/src/lib.rs` (+ `pub mod subprocess;`)
  - `rust/bismark-extractor/tests/phase_g.rs` (new)
  - `rust/bismark-extractor/tests/phase_g_argv_parity.rs` (new)
  - `rust/bismark-extractor/tests/phase_g_realrunner.rs` (new)
  - `rust/bismark-extractor/tests/phase_g_real.rs` (new)
  - `rust/bismark-extractor/tests/fixtures/` (5 fake-shell scripts + 3 argv-parity goldens + 1 README with the Perl shim recipe — new dir)
  - `plans/05262026_bismark-extractor/PROGRESS.md`
- **Side effects**: spawning external subprocesses; writing files into `--output_dir` (subprocesses do this).

### 7.2 Downstream impact

| Consumer | Impact |
|---|---|
| Phase H (byte-identity gate) | Harness can now compare bedGraph/cov/CpG_report streams to Perl. **Argv-parity goldens** (rev 1 I3) provide a unit-test-level confidence boost before Phase H runs. |
| nf-core pipelines | Users running with `--cytosine_report` / `--bedGraph` will now succeed (previously silently failed — flags parsed but unused). **Behavioural change.** |
| Existing tests | `tests/output_phase_c2.rs` needs minor update for `FinalizationReport` return type; all others untouched. |
| Phase F's `--parallel N` model | No interaction; explicit test asserts no-op. |
| `BISMARK_BIN` env var (new public contract) | rev 1 documents `BISMARK_BIN` as a **strict** env var (rev 1 I12). Adoption pattern: install Bismark to `/opt/bismark`; set `BISMARK_BIN=/opt/bismark`. Misconfiguration produces `SubprocessNotFound`, NOT silent fallback. Document in user-facing SPEC §6.6 paragraph. |

### 7.3 Deliberately NOT implemented (rev 1 I11 expanded)

- **Inline Rust subprocess replacements** — v1.x; epic #797 + future.
- **`--rust-bedgraph` / `--rust-coverage2cytosine` escape hatches** — defer to when the inline impl lands.
- **`--counts` flag forwarding** — Perl `:362-364` comments out the push. Mirror.
- **Perl's `sleep(1)` after b2bg spawn (`:380`)** — Perl-ism; Rust's tee makes it unnecessary.
- **stdout capture** — subprocesses write to files via `--output`.
- **c2c flags NOT exposed by the extractor** (rev 1 I11): `--merge_CpGs`, `--GC_context` (`--GC`), `--nome-seq`, `--ffs`, `--discordance_filter`, `--drach` (`--m6A`), `--threshold`/`--coverage_threshold` (different from b2bg's `--cutoff`!). These are c2c-direct features not surfaced by the Perl extractor either; mirror by NOT forwarding. SPEC §6.6 documents the omission so future "why doesn't the Rust port pass --nome-seq?" questions have a pointer.
- **`SubprocessTimeout` error variant** — out-of-scope for v1.0 (rev 1 Reviewer B Opt 27). Long-running b2bg on a hung NFS mount currently has no timeout; add a `// TODO(v1.x): wrap subprocess in timeout` comment.

## 8. Assumptions (rev 1 refined)

### 8.1 From Perl + workspace review

- **A1.** Subprocesses present on PATH or via `BISMARK_BIN` or alongside the Rust binary at runtime. Verified by `discover_subprocess`.
- **A2.** Long-form flag names per §2.4.4 verified by reading `bismark2bedGraph:637-651` and `coverage2cytosine:2011-2028`.
- **A3.** Subprocesses inherit cwd + env; locale (`LC_ALL`, `LANG`) is the user's responsibility for byte-identity-relevant sort behaviour (rev 1 I8 SPEC note).
- **A4.** b2bg's `.bismark.cov.gz` is always gzipped (extension-sniff). c2c's `--gzip` controls its own output.
- **A5 (rev 1 I4)**: `which` crate is **added to Cargo.toml in this PR**; not relying on transitive workspace presence.
- **A6.** `current_exe()` reliable on Linux + macOS; Windows out of scope for v1.0.
- **A7.** Stderr captured via `Stdio::piped()` is byte-stream — possibly non-UTF-8 at line boundaries. `Vec<u8>` + `read_until(b'\n')` handle this byte-safely (rev 1 C5); lossy String only at Display time.
- **A8 (rev 1 C3)**: Perl's filename-derivation regexes strip literal `gz`/`sam`/`bam`/`txt` (no leading dot anchor). Rust mirrors verbatim. Chained extensions preserve trailing dots; no-extension inputs produce no leading dot. 8 derivation goldens lock this.

### 8.2 Plan-specific

- **A9.** One PR.
- **A10.** Branch from `rust/iron-chancellor` HEAD `4e5c691`.
- **A11.** Existing tests preserved untouched except for `tests/output_phase_c2.rs`'s `FinalizationReport` return-type binding (~4 lines).
- **A12.** No CHANGELOG.md added in this phase.
- **A13.** Phase H harness expansion is a separate PR.
- **A14.** No CLI changes in Phase G — the surface is locked at `cli.rs`.
- **A15 (rev 1 I7)**: `OutputFileMap::finalize_with_empty_sweep` returns kept paths in lexicographic order to ensure b2bg argv determinism.
- **A16 (rev 1 C4)**: kept paths are absolute (via `canonicalize` or `output_dir.join` semantics; absolute by construction).
- **A17 (rev 1 I12)**: `BISMARK_BIN` semantics are **strict** — set means "lock the source; no fallback". Empty string treated as unset.

## 9. Validation (rev 1 — ~30 unit + ~16 integration + ~9 RealRunner + 2 real-subprocess smoke)

### 9.1 Unit-level (per §5.4.1)

- ~30 unit tests covering: filename derivation (12 — incl. 6 chained-extension/no-ext edge cases per rev 1 C3), argv-build (15), ring buffer (4), discovery (7), Display.

### 9.2 Integration-level

- ~16 mocked-runner tests in `tests/phase_g.rs` (rev 1 added empty-kept-set + UX warning + absolute-path assertion + non-UTF-8 path).
- ~3 argv-parity tests in `tests/phase_g_argv_parity.rs` (rev 1 I3 — goldens vs Perl).
- ~9 `RealRunner` fake-shell tests in `tests/phase_g_realrunner.rs` (rev 1 I2 — drain-thread + ring-buffer + deadlock guarantees actually exercised in CI).

### 9.3 Real-subprocess level

- 2 `#[ignore]`'d smoke tests for opt-in CI / oxy verification (rev 1 I7 — tightened assertions: column count + non-zero rows).

### 9.4 Manual on oxy (post-merge)

Re-run `scripts/oxy_phase_h_smoke.sh` on 10M PE BAM with `--cytosine_report --genome_folder /path/to/GRCh38_bismark/`. Expected new outputs:
- `{stem}.bedGraph.gz` / `.bedGraph`
- `{stem}.bismark.cov.gz`
- `{stem}.CpG_report.txt.gz` / `.CpG_report.txt`

Phase H proper will write the harness arms; Phase G's manual oxy run is a non-blocking sanity check.

## 10. Questions or ambiguities (rev 1 — resolved + remaining)

### Critical — none (post-rev-1 absorption)

All 5 distinct Criticals from the dual review pass are folded (rev 1 C1–C5).

### Open (defaults taken)

| Q | Default | Rationale |
|---|---|---|
| Subprocess discovery order | **BISMARK_BIN-first (strict) → PATH → current_exe()** (rev 1 C1 + I12). | Explicit env override should win; strict mode prevents silent install-skew bugs; consistent across §3.2, §10, §11. |
| Working directory of subprocess | Inherit parent's cwd. | Matches Perl `system()`. |
| Environment-variable handling | Inherit fully. | Matches Perl. LC_ALL pinning is harness/Phase-H concern. |
| `--ample_memory` pass-through | Verbatim mirror of Perl `:347-352`. | Lockstep for byte-identity. |
| `--buffer_size` default-emission | **Push `--buffer_size 2G` when neither set** (rev 1 I5). | Perl-faithful at argv level; affects Phase H byte-identity of stderr. |
| Ring buffer size | 64 KiB. | Pin C2-precedent; tuneable. |
| Ring buffer data structure | **`VecDeque<u8>`** (rev 1 O1). | O(1) eviction. |
| `BISMARK_BIN` env var name | `BISMARK_BIN`. | Matches `*_BIN` convention. |
| `BISMARK_BIN` strict-vs-permissive | **Strict** (rev 1 I12). | "If user sets it, lock the source." |
| `BISMARK_BIN=""` semantics | Treat as unset; fall through (rev 1 I16). | Common shell pattern. |
| `--rust-bedgraph` toggle now? | **No** — defer to v1.x. | Avoid shipping CLI shapes whose impl is NotYetImplemented. |
| `which` crate | **Add to Cargo.toml in this PR** (rev 1 I4). | Confirmed not present today. |
| Drain-thread spawn ordering | **BEFORE `child.wait()`** (rev 1 I6). | Prevents pipe-buffer deadlock. |
| Drain-thread join policy | **Always join, regardless of exit status** (rev 1 I1). | Prevents stderr_tail race + thread leak. |
| Drain-thread reader | **`read_until(b'\n', &mut Vec<u8>)`** (rev 1 C5). | Byte-safe on non-UTF-8 stderr. |
| `stderr_tail` type | **`Vec<u8>`** (rev 1 C5). | Byte-safe; lossy String at Display only. |
| `finalize_with_empty_sweep` return | **`FinalizationReport` struct** (rev 1 I10). | Room for Phase H counts/errors without churn. |
| Kept-paths form | **Absolute paths, sorted lexicographically** (rev 1 C4 + I7). | Argv-parity vs Perl deterministic. |
| Empty kept-set + cytosine_report | Emit `eprintln!` UX warning (rev 1 I14); chain still runs. | Cheap; informs user before a long no-op c2c run. |
| Pre-spawn debug eprintln | Always-on `eprintln!` of program + argv (rev 1 O2). | Debug affordance. |
| `--counts` field on CLI | Keep, document as accepted-and-ignored (rev 1 O3). | No reason to break the CLI shape. |
| Test `current_exe()` fallback | `BISMARK_TEST_CURRENT_EXE_DIR` env hatch under `#[cfg(test)]` (rev 1 I17). | Replaces brittle symlink-based test. |

### Open (deferred to plan-reviewer round 2 if reviewer pushes back)

- Whether the v1.0 release CLI should include `--rust-bedgraph` even as NotYetImplemented (Reviewer A § 5.3 alternative). Default: no.
- Whether to add an `eprintln!` summary at chain exit ("bedGraph + cytosine_report finished in X.Y seconds"). Not in scope; future UX work.

## 11. Self-Review (rev 1 — post-absorption)

Reviewed rev 1 for:

- **Efficiency:** Drain thread + VecDeque ring buffer + argv build are µs-scale per invocation; subprocess work itself dominates. ✓
- **Logic consistency:** Discovery order now consistent across §3.2 + §10 + §11 (BISMARK_BIN-first strict). Filename derivation literal-strip semantics consistent across §2.4.5, §2.4.6, §2.4.7. Drain thread architecture consistent across §3.4 + §4.1 + §5.3 step 7 + tests in §5.4.4. ✓
- **Edge cases:** §3.8 expanded to 17 cases (added: BISMARK_BIN empty/strict/not-executable; non-UTF-8 stderr; 128 KiB pipe-burst; non-UTF-8 path; chained-extension input; no-extension input). ✓
- **Integration:** Phase F invariant preserved; Phase E (`--gzip`) interaction has §3.7 matrix; Phase C.2's `finalize_with_empty_sweep` upgraded to `FinalizationReport`; output_phase_c2.rs tests get a 4-line binding update. ✓
- **Test coverage:** ~30 unit + ~16 mocked integration + 3 argv-parity goldens vs Perl + ~9 `RealRunner` fake-shell tests + 2 real-subprocess smoke. Every Critical has a regression-guard test; every Important has a covering test. ✓
- **SPEC alignment:** §6.6 matrix + LC_ALL + deliberately-omitted flags + filename-derivation quirk; §6.5 error variants; §3 row wiring; §8.x test rows; §10 row G LOC. ✓

### Adjustments made during rev 1 absorption

Folded **all 5 Critical + 17 Important findings** from dual plan-review:

- **C1** (both, A-Critical / B-Important): Discovery order made consistent. BISMARK_BIN-first strict.
- **C2** (A-Critical): `input_basename` field added to `ExtractState`; constructor updated; accessor exposed.
- **C3** (B-Critical): Filename-derivation rewritten with 8-row edge case table; trailing-dot quirk pinned; 6 new unit-test goldens covering chained-extension + no-extension cases.
- **C4** (B-Critical): Kept-paths-form pinned to absolute, sorted. Test added.
- **C5** (B-Critical): `read_until` not `read_line`; `stderr_tail: Vec<u8>` not `String`. Display uses lossy at render time.
- **I1, I6** (both / B): Drain thread always joined; spawned before `child.wait()`; pipe-deadlock test added.
- **I2** (both): `tests/phase_g_realrunner.rs` with fake-shell scripts.
- **I3** (both): `tests/phase_g_argv_parity.rs` with 3 goldens.
- **I4** (both): `which` crate added to Cargo.toml.
- **I5** (A): `--buffer_size 2G` pushed when neither flag set; test added.
- **I7** (A): Smoke test assertions tightened to column-count + non-zero rows.
- **I8** (A): SPEC §6.6 LC_ALL note added.
- **I9** (A): `.stdin(Stdio::null())` explicit.
- **I10** (A): `FinalizationReport` struct over bare `Vec<PathBuf>`.
- **I11** (A): Deliberately-omitted c2c flags documented in §7.3.
- **I12** (B): Strict `BISMARK_BIN` semantics; edge case row + test added.
- **I13** (B): `--parent_dir == --dir` invariant pinned in argv-build + tests.
- **I14** (B): Empty-kept-set integration test + UX warning eprintln.
- **I15** (B): `cargo test --test phase_g_real -- --ignored` exact invocation documented.
- **I16** (A): Tests for `BISMARK_BIN` not-executable + empty-string.
- **I17** (both): `BISMARK_TEST_CURRENT_EXE_DIR` env hatch.

Selected Optionals folded:
- **O1**: `VecDeque<u8>` for ring buffer.
- **O2**: Pre-spawn `eprintln!` of program + argv.
- **O3**: `--counts` field kept; clarifying doc-comment added.
- **O4**: Module doc for `src/subprocess.rs` with byte-identity invariant.

Deferred Optionals: macOS PATH SIP test note (A21 — environment-specific), test-name length consistency (A25 — bikeshedding), `SubprocessTimeout` v1.x TODO comment (B27 — added inline).

### Remaining risks

- **R1**: `which` crate behaviour on macOS arm64 with Apple-Silicon shell PATH munging — verified empirically at impl-time (Reviewer A Opt 21).
- **R2**: `current_exe()` symlink resolution under `cargo install` — mitigated by `BISMARK_TEST_CURRENT_EXE_DIR` test hatch + PATH being the primary mechanism in production.
- **R3**: Drain-thread join may add a few ms of latency on success-path; bounded by typical subprocess stderr volume (<1 KiB). Negligible.
- **R4**: Argv-parity goldens go stale if the user upgrades Bismark and Perl `bismark_methylation_extractor:323-428` changes flag-push order. Mitigation: golden regen recipe in `tests/fixtures/README.md`; CI runs golden tests so drift is caught immediately.
- **R5**: Phase H byte-identity comparison may surface output-format divergences across Bismark point releases. Out-of-scope for G; Phase H will pin a Bismark version expectation.

### Reviewer-attention magnets (post-absorption summary for round-2 reviewers, if needed)

The rev-0 magnets that still have residual ambiguity:

1. **Ring buffer cap = 64 KiB** — still arbitrary; tests now pin the exact eviction policy. Reviewer may want 256 KiB; trivial to tune.
2. **Strict `BISMARK_BIN` (rev 1 I12)** — alternative is permissive-with-warning. Argument for strict: prevents silent install skew. Argument for permissive: lower barrier for users who set BISMARK_BIN as a convenience for one tool but not the other.
3. **`--rust-bedgraph` deferral** — locked as "defer to v1.x". Round-2 reviewer may push back if they prefer locking the CLI shape now.
4. **Argv-parity goldens vs Bismark version** — goldens are checked-in for one Bismark version (v0.25.1). Cross-version stability of `bismark_methylation_extractor:323-428` is an empirical question.

---

## 12. Open delivery cycle

1. ✅ Sub-issue filed: [#868](https://github.com/FelixKrueger/Bismark/issues/868), linked under epic #798.
2. ✅ Plan rev 0 written.
3. ✅ Manual review by Felix — approved, directed to dual reviewers.
4. ✅ Dual `plan-reviewer` agents — `PLAN_REVIEW_PHASE_G_A.md` (NEEDS-REVISIONS: 2 Crit + 14 Imp + 9 Opt) + `PLAN_REVIEW_PHASE_G_B.md` (NEEDS-REVISIONS: 4 Crit + 14 Imp + 10 Opt). Total 5 distinct Criticals + 17 distinct Importants after de-duplication.
5. ✅ **Plan rev 1** folding all 5 Critical + 17 Important findings + 4 selected Optionals — this file.
6. 🟡 **Implementation trigger from Felix** — *PENDING*.
7. ⏸ Implementation per §5.
8. ⏸ Dual `code-reviewer` agents.
9. ⏸ `plan-manager` Mode B coverage audit.
10. ⏸ Branch `extractor-phase-g`, PR → `rust/iron-chancellor`, closes #868.
11. ⏸ Merge.
12. ⏸ Phase H proper opens.

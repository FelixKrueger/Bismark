# Plan Coverage Report

**Mode:** B — code vs. plan (post-implementation)
**Plan(s):** `plans/08112026_1100-five-base-index-validation/PLAN.md` (rev 2)
**Audited:** working tree on branch `1100-five-base-index-validation` (**uncommitted**; `git diff` vs `688d919`)
**Date:** 2026-08-11
**Verdict at audit time:** INCOMPLETE — 6 items unresolved in 4 gaps, **none behavioural** (1 doc sentence, 1 missing test half, 1 unasserted property, 3 unrecorded sabotage claims)
**Verdict now: COMPLETE** — all four gaps closed in the merged implementation (`7596b0d`, PR #1102), verified 2026-08-16. See [Resolution](#resolution); the ledger below is left as the point-in-time record.

## Summary

- Total items: **45** — 3 decisions (§0), 16 task sub-items (§4), 14 tests (§5a/§5b), 6 sabotage claims (§5c), 6 verification streams (§6)
- DONE: **38**
- PARTIAL: **5** (items 31, 32, 35, 38, 39)
- MISSING: **1** (item 8)
- DEVIATED, documented: **1** (item 26)

No item is a behavioural gap. Every gap is a documentation, test-coverage or
sabotage-record omission. The shipped code does what §3 specifies, verified both by the plan's
own test suite and by all four of §6's manual steps run against a **real** `bowtie2-build` index.

**Every §6 gate re-run here and green:** full suite 80 suites / 0 failures / exit 0; `cargo fmt --check`
clean on exit code; all three `clippy -D warnings` invocations exit 0.

## Coverage ledger

### §0 Decisions

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | D1 — hard error, not a warning | §0 D1 | DONE | `AlignerError::Validation` returned and `?`-propagated (`config.rs:881`). Manual §6 step 1: exit 1, one error line, nothing written |
| 2 | D2 — include the `--output_dir` fix | §0 D2 | DONE | Task 3 implemented |
| 3 | D3 — `create_dir_all` + Perl's notice + CHANGELOG disclosure | §0 D3 | DONE | All three present. Notice matches `legacy_perl/bismark:8189` (`Created output directory <dir>!` + blank line); nested-parent excess disclosed in the CHANGELOG |

### §4 Tasks

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 4 | `validate_unconverted_index` + `index_env_for` in `discovery.rs` | Task 1 | DONE | `discovery.rs:193` and `:184` |
| 5 | `first_missing` / `index_suffixes` stay **private** | Task 1 | DONE | Neither gained `pub`; new helpers `index_files_for`/`first_missing_at`/`any_index_file_at`/`index_env_name` are private too |
| 6 | Probe mirrors §3's four steps (both arms → gate → env arm → error naming flag, aligner, every path probed) | Task 1 / §3 | DONE | `discovery.rs:193-247`. Error also adds `($BOWTIE2_INDEXES is not set)` when the variable is absent |
| 7 | Candidates by `OsString` concatenation — never `Path::join`/`parent()`/`file_name()` | Task 1 / §3 | DONE | `index_files_for` pushes the suffix onto the basename `OsString`; the env arm pushes `"/"` then the basename |
| 8 | One sentence noting `--large-index` is unreachable on this route (`five_base_build_argv` emits only `-x`/`-1`/`-2`) | Task 1 | **MISSING** | See Gap 1 |
| 9 | Call site between `discover_genome_for_run` and `detect_aligner` | Task 2 | DONE | `config.rs:874-882`; `discover_genome_for_run` at `:861`, `detect_aligner` at `:890` |
| 10 | Guarded on `illumina_5base && matches!(aligner, Bowtie2\|Hisat2)`; `if let Some(idx)` not `.expect()` | Task 2 | DONE | `config.rs:876-878` — exactly that guard, `if let Some(index) = cli.five_base_index.as_deref()` |
| 11 | Repair `five_base_bowtie2_unconverted_index_end_to_end` | Task 2b | DONE | `make_stub_bowtie2_index(&genome.path().join("normal_idx"))` added; test passes |
| 12 | Repair `five_base_resolve_does_not_require_the_converted_index` | Task 2b | DONE | Literal `/nonexistent/idx` replaced by a 6-file stub; a new assertion pins that the check must not fire on a valid index; test passes |
| 13 | `--output_dir` auto-created in `resolve_output`, fail-loud, with Perl's notice | Task 3 | DONE | `config.rs:1573-1579`. One call site (`:913`) — genuinely a choke point — and it runs **after** `detect_aligner` (`:890`), so a rejected index leaves nothing |
| 14 | `create_dir_all("")` safe for the `PathBuf::new()` default | Task 3 | DONE | Guarded by `!output_dir.as_os_str().is_empty()`, so the empty case is not even attempted — stronger than the plan's reliance on `Ok(())` |
| 15 | Unify `mod.rs`'s silent `create_dir_all(&out_dir).ok()` onto the erroring form | Task 3 | DONE | `mod.rs:579-584` now `map_err` → `Validation`, matching the pre-existing sibling at `:653` |
| 16 | CHANGELOG bullet — early failure, env vars honoured, **D1's stub/wrapper consequence** | Task 4 | DONE | `CHANGELOG.md:16`, under `## Unreleased` → `### bismark (aligner)`. Names `--path_to_bowtie2` shims/wrappers explicitly |
| 17 | CHANGELOG — `--output_dir` its **own** bullet, scoped to all runs, notice + nested deviation | Task 4 | DONE | `CHANGELOG.md:17`. "for every run, not just 5-Base"; discloses the nested-parent excess over Perl |
| 18 | Docs sentence on `illumina-5-base.md` | Task 4 | DONE | Lines 53-55, in "Running it". Placed immediately **before** the `--five_base_index` example block rather than after it — content is present and reads correctly in place |
| 19 | `--five_base_index` help text gains the env-var clause | Task 4 | DONE | `cli.rs:120-121` |

### §5a Unit tests — `validate_unconverted_index`

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 20 | `complete_small_index_accepted` | §5a-1 | DONE | PASS |
| 21 | `complete_large_index_accepted` | §5a-2 | DONE | PASS — pins the large arm |
| 22 | hisat2 arity: 8 `.ht2` Ok, 6 Err, 8 `.ht2l` Ok | §5a-3 | DONE | `hisat2_arity_and_large_arm`, PASS. All three assertions present. The "6 → Err" case uses 6 *bowtie2*-suffixed files probed as hisat2 — it still fails if hisat2 were given bowtie2's arity or extension, which is the defect the item targets |
| 23 | `missing_index_rejected`, message names the flag | §5a-4 | DONE | `missing_index_rejected_naming_the_flag`, PASS; also asserts `nope.1.bt2` appears |
| 24 | `env_fallback_accepted` | §5a-5 | DONE | PASS |
| 25 | `env_fallback_applies_to_a_separator_basename` | §5a-6 | DONE | PASS. Independently reconfirmed here against **real bowtie2**: `./puc` + `BOWTIE2_INDEXES` runs to completion (see Verification) |
| 26 | `env_fallback_is_skipped_when_the_basename_partially_matches` | §5a-7 | **DEVIATED** (documented §11) | Deleted as vacuous, replaced by an integration test. The intended coverage exists and is *stronger*. See Deviation 1 |
| 27 | `partial_index_rejected`, naming the missing file | §5a-8 | DONE | `partial_index_rejected_naming_the_missing_file`, PASS |
| 28 | `pathological_basenames_error_not_panic` — `..`, `/`, trailing dot | §5a-9 | DONE | `pathological_basenames_error_rather_than_panic`, PASS; all three probes present |
| 29 | False-rejection guards — symlinked files, symlinked directory, absolute basename, `../`-relative basename | §5a-10 | DONE | Split across `accepts_absolute_and_dot_dot_basenames` and `accepts_symlinked_index_files_and_directory` (`#[cfg(unix)]`), both PASS. The `../` case is an absolute path *containing* `..`; a cwd-relative `../puc` is not unit-testable for the reason §11 gives, and the mechanism under test (`..` resolving through the filesystem rather than through path splitting) is covered |

### §5b Integration tests — `tests/aligner_cli.rs`

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 30 | `five_base_index_missing_fails_early` | §5b-11 | DONE | PASS. Asserts non-zero exit, `--five_base_index` present, `desync` and `panicked` absent, and the output dir **entirely empty** — stronger than the plan's "no `.bam` and no report" |
| 31 | `output_dir_is_created_with_a_notice` | §5b-11b | **PARTIAL** | Notice + nested creation + success all asserted and PASS. The clause "also assert a rejected index creates **nothing**" is unasserted. See Gap 2 |
| 32 | `five_base_index_resolved_via_env_is_accepted` | §5b-12 | **PARTIAL** | The `BOWTIE2_INDEXES` half is present and PASS. The plan's "**Plus the `HISAT2_INDEXES` twin**" is absent. See Gap 3 |
| 33 | `five_base_minimap2_route_is_not_index_checked` | §5b-13 | DONE | Implemented via the plan's own sanctioned alternative — "one added clause on the #1099 test's first loop arm". `five_base_resolve_does_not_require_the_converted_index` now asserts `!e.to_string().contains("--five_base_index")` on both arms, the first being the default minimap2 route. PASS. The complementary property (minimap2 + `--five_base_index` rejected outright) is pre-existing and covered by `illumina_5base_engine_selection` (`config.rs:2367-2371`) |

Also present, beyond §5: `five_base_index_resolved_from_cwd_is_accepted` — the positive form §5
could not reach (Reviewer A's F6), noted in §11.

### §5c Sabotage claims

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 34 | Drop the large arm → tests 2, 3 red | §5c | DONE | §11 row 1 records **3 red**, naming `complete_large_index_accepted` and `hisat2_arity_and_large_arm`. Mechanically credible: removing the `true` iteration of `for large in [false, true]` makes both `.bt2l`/`.ht2l` fixtures unresolvable. The unnamed "+1" is a loose end in the record, not a coverage gap |
| 35 | Drop the env fallback → 5, 6, **12** red | §5c | **PARTIAL** | §11 row 2 records only **2 red** (the two unit tests). Test 12's redness under this same sabotage is not recorded — evidently only the lib suite was re-run. Mechanically 12 would also go red, and its seam is separately sabotage-verified by row 4, so the risk is low |
| 36 | Drop the wrapper's gate → test 7 red | §5c | DONE | §11 row 3, against test 7's replacement. Credible: with the gate forced true, the complete set under `$BOWTIE2_INDEXES` resolves, the run no longer errors on the flag, and the test's `contains("--five_base_index")` assertion fails |
| 37 | Call site passes `None` for `index_env` → 12 red | §5c | DONE | §11 row 4. Credible and precisely the seam Reviewer B flagged: with `None`, the bare `puc` resolves nowhere and the test's `.success()` fails |
| 38 | Move the check before `:861` → assert the #1099 guard stops reaching discovery | §5c | **PARTIAL** | Not recorded in §11, and now largely unobservable — see Gap 4 |
| 39 | Move the check after `detect_aligner` → test 11 red on a box without bowtie2 | §5c | **PARTIAL** | Not recorded in §11. Not observable on this machine: `bowtie2` is on `PATH` (`/opt/homebrew/bin/bowtie2`), so `detect_aligner` succeeds and the check would still fire. A legitimate reason to skip, but §11 does not say so |

### §6 Verification

| # | Item | Status | Result |
|---|------|--------|--------|
| 40 | `cargo test -p bismark --lib aligner::{config,discovery}` | DONE | Run as two invocations (libtest takes one filter): `aligner::discovery` **35 passed, 0 failed**; `aligner::config` **45 passed, 0 failed** |
| 41 | `cargo test -p bismark --test aligner_cli five_base` | DONE | **9 passed, 0 failed**, including the repaired `five_base_bowtie2_unconverted_index_end_to_end`. The two new tests whose names do not match the `five_base` filter were run individually: `output_dir_is_created_with_a_notice` PASS, `partial_index_at_the_basename_blocks_the_env_fallback` PASS |
| 42 | `cargo test -p bismark` | DONE | **80 suites, 0 failures, exit 0** — matches §11's claim exactly. Took ~25 min (exceeds a 10-minute foreground limit; run detached) |
| 43 | `cargo fmt -p bismark -- --check` | DONE | **Clean**, asserted on the command's own exit code (0) per §11 iteration-log #4 — not through a pipe |
| 44 | `cargo clippy` ×3 (default, `rammap-inprocess`, `binseq-input`) | DONE | All three **exit 0** with `-D warnings`: default PASS, `--features rammap-inprocess` PASS, `--features binseq-input` PASS (`rust_ci.yml:135`) |
| 45 | Manual steps 1-4 | DONE | All four run against the debug binary explicitly. See below |

**Manual verification (§6), all four steps plus two added controls.** Run as
`rust/target/debug/bismark` (binary mtime 11:43 vs newest changed source 11:17, so it reflects the
working tree), scratch under a unique `$TMPDIR` dir.

| Step | Command | Result |
|---|---|---|
| 1 | `--five_base_index /no/such/idx` over a FASTA-only genome | exit 1; **one** error line: `--five_base_index /no/such/idx is not a complete Bowtie 2 index ($BOWTIE2_INDEXES is not set). Probed: /no/such/idx.1.bt2, /no/such/idx.1.bt2l. Build a NORMAL (unconverted) index once with bowtie2-build.` Output dir **empty (0 entries)**; `desync` 0 hits; `panicked` 0 hits |
| 2 | positive control — a **real** `bowtie2-build` index | exit 0, no `--five_base_index` in stderr, real `test_R1_bismark_bt2_pe.bam` + PE report written. **The check does not falsely reject a genuine index** |
| 3 | `BOWTIE2_INDEXES=$G`, bare basename `puc`, cwd elsewhere | exit 0, BAM + report written — the env fallback works end-to-end through real bowtie2 |
| 3b | *(added control)* the same command with the variable **unset** | exit 1, `--five_base_index puc … ($BOWTIE2_INDEXES is not set)`. Confirms step 3 succeeds *because of* the fallback, not incidentally |
| 4 | bare basename resolved from cwd, no env var | exit 0, BAM + report written |
| §2 row 3 | *(added control)* `--five_base_index ./puc` with `BOWTIE2_INDEXES` set | exit 0, BAM + report written, and real bowtie2 raised no index complaint. **Independently reconfirms the plan's pivotal §2 finding** — a separator does not disable the fallback, so the separator heuristic §2 rejected would have been a false rejection |

**§11 iteration-log #2 — `discovery.rs` integrity after the `git checkout --` wipe: CONFIRMED
INTACT.** Every function §3/§4 specifies is present exactly once (`index_files_for`,
`first_missing_at`, `any_index_file_at`, `index_env_name`, `index_env_for`,
`validate_unconverted_index`), `grep -o 'fn [a-z_0-9]*' | sort | uniq -d` reports **no duplicates**,
the pre-existing `first_missing`/`index_suffixes`/`discover_genome`/`discover_genome_fasta_only`/
`discover_fastas` are all still there, the file compiles, and all 35 of its tests pass. No missing or
duplicated function.

**§11 counts — one cosmetic slip.** "`aligner::discovery` 25 → 36" mixes two different counts: the
file has **26 → 36** `#[test]` attributes and **25 → 35** tests *execute on macOS*, because one
pre-existing test is `#[cfg(target_os = "linux")]`. The delta (**+10**, matching the 10 new tests) is
correct either way, and 36 is the CI/Linux figure. `aligner_cli` "116 → 121" is likewise +5 (117 → 122
attributes). Bookkeeping only.

## Gaps (detail)

### Gap 1 — Task 1's `--large-index` note is absent (MISSING)

**Expected:** Task 1 — "One sentence noting `--large-index` is unreachable for this route
(`five_base_build_argv` emits only `-x`/`-1`/`-2`), so the two arms cover every case."
**Found:** Nothing. `five_base_build_argv` is not mentioned anywhere in `discovery.rs`, and no comment
explains *why* probing both arms unconditionally is complete rather than a guess. The nearest text —
"a mammalian bowtie2 index is `.bt2l`" in the CHANGELOG and "in either index size" on
`any_index_file_at` — states the behaviour but not the reachability argument.
**Gap:** One sentence, on `validate_unconverted_index` or `index_files_for`. Purely documentary: the
code already probes both arms, so nothing behavioural depends on it. It matters only to the next
reader wondering whether an explicit `--large-index` flag needs handling.

### Gap 2 — "a rejected index creates nothing" is unasserted (PARTIAL)

**Expected:** §5b-11b — "Also assert a rejected index creates **nothing**, which holds because
`resolve_output` runs after `detect_aligner`."
**Found:** No test asserts it. `output_dir_is_created_with_a_notice` only asserts the *positive*
(`nested.is_dir()` after a successful run). `five_base_index_missing_fails_early` asserts an
**already-existing** `TempDir` output dir stays empty — a weaker, different claim, since it cannot
observe whether the directory would have been *created*.
**Verified true, just untested:** I ran it — a rejected index with `-o <tmp>/rej_$$/a/b/c` exits 1 and
**nothing is created**, not even the top-level `rej_$$`. So this is an unguarded-regression risk, not
a defect. The ordering it depends on is real (`resolve_output` at `config.rs:913`, `detect_aligner` at
`:890`), but nothing would catch someone hoisting `resolve_output` above the index check.
**Gap:** One line — a `!path.exists()` assertion on a nonexistent nested `-o` in a rejected run.

### Gap 3 — the `HISAT2_INDEXES` twin is missing (PARTIAL)

**Expected:** §5b-12 — "Plus the `HISAT2_INDEXES` twin." The item exists specifically to catch "a
misspelled or swapped variable name, `env::var` instead of `var_os`, or a hard-coded `None` at the
call site … with every unit test green."
**Found:** `HISAT2_INDEXES` appears in exactly two places in the repository: the source arm
`Aligner::Hisat2 => Some("HISAT2_INDEXES")` (`discovery.rs:178`) and the help text (`cli.rs:121`). **No
test sets it, and no test calls `index_env_for(Aligner::Hisat2)` or `validate_unconverted_index` with a
hisat2 `index_env`.** The hisat2 unit test (`hisat2_arity_and_large_arm`) passes `None` for the
environment throughout.
**Consequence:** the exact defect class the item names is unguarded for hisat2 — swap that arm to
`"BOWTIE2_INDEXES"`, or return `None`, and the entire suite stays green. §2 argues the variable is
broken upstream so the arm can only over-accept, which bounds the *severity*; it does not supply the
coverage §5 asked for.
**Feasibility:** `make_fake_hisat2_pe` and five other fake-hisat2 helpers already exist in
`aligner_cli.rs`, so the twin is a copy of the bowtie2 test with `--hisat2`, eight `.ht2` stub files
and `--path_to_hisat2`.
**Gap:** one integration test, or a unit test passing `Some(dir)` for a hisat2 basename.

### Gap 4 — three §5c sabotage claims incompletely recorded (items 35, 38, 39)

**Expected:** §5c lists **six** sabotages; §11's table records **four**. The three shortfalls:

- **Item 35** — "Drop the env fallback → 5, 6, **12** red." §11 row 2 records only the two unit tests
  going red. Test 12 (`five_base_index_resolved_via_env_is_accepted`) is an integration test, so
  evidently only `--lib` was re-run under this sabotage. Mechanically 12 *would* go red, and row 4
  sabotage-verifies that same test through the call-site seam, so the residual risk is small.
- **Item 38** — "Move the check before `:861` → the #1099 guard stops reaching discovery (**assert it,
  don't just reason about it**)." Not recorded.
- **Item 39** — "Move it after `detect_aligner` → 11 red on a box without bowtie2." Not recorded.

**Found:** §11's sabotage table has four rows; items 38 and 39 are absent, and §11 does not mention
skipping them.
**Assessment — the first is now largely unobservable, and that is worth knowing:** Task 2b gave
`five_base_resolve_does_not_require_the_converted_index` a **valid** stub index, so moving the check
before genome discovery no longer diverts that test — the check passes either way and the guard still
reaches discovery. The rev-1 hazard §5c was written against (an *invalid* index short-circuiting the
guard into vacuous green) was removed by Task 2b rather than tested. The related property "a missing
genome error wins over an index error", which §3 states as a design intent, is pinned by **no test**:
no test passes both a bad genome and a bad `--five_base_index`. The new assertion added to that test
pins "the check must not fire on a valid index", which is adjacent but not the ordering.
**Assessment — the second is not observable here:** test 11 supplies no `--path_to_bowtie2`, and
`bowtie2` is on this machine's `PATH`, so `detect_aligner` succeeds and the relocated check would
still fire. Skipping it locally is defensible; the omission is that §11 does not say so.
**Gap:** either record all three sabotages with the reason each was skipped (re-running item 35's
against `--test aligner_cli` is cheap), or add the one test that would make item 38 observable
(bad genome + bad index → the *genome* error).

## Deviations (detail)

### Deviation 1 — planned unit test 7 deleted and re-sited (DEVIATED, documented in §11)

**Planned:** §5a-7 `env_fallback_is_skipped_when_the_basename_partially_matches` — a unit test: 1 of 6
files at the basename, a complete set under the env dir, expect `Err`.
**Found:** Deleted. Replaced by `partial_index_at_the_basename_blocks_the_env_fallback` in
`tests/aligner_cli.rs`, which uses `.current_dir(here)` with `puc.1.bt2` alone in the child's cwd and a
complete set under `BOWTIE2_INDEXES`, and asserts the run fails naming `--five_base_index`.
**Documented:** yes, in §11, with the reason — the unit version used an *absolute* basename, so the
env-join could never resolve and it passed whether or not the gate existed. Vacuous.
**Assessment: the coverage §5 intended now exists, and is strictly stronger.** The gate
(`if !any_index_file_at(...) && let Some(dir) = index_env`) is only observable when the env-joined path
*could* have resolved, which requires a relative basename, which requires cwd control — unsafe
in-process for the same reason `set_var` is, safe in a child. The replacement is sabotage-verified
(§11 row 3) where the deleted one could not have been. **This is DEVIATED, not MISSING**, and the
deviation improved the test. It also closed a positive form §5 never covered
(`five_base_index_resolved_from_cwd_is_accepted`).

## Verdict

> **Superseded — see [Resolution](#resolution).** All four gaps were closed in the merged
> implementation; what follows is the 2026-08-11 assessment, kept intact.

**INCOMPLETE — 6 items unresolved in 4 gaps, none behavioural.** Every task in §4 is implemented,
every §0 decision is realised in code, 13 of the 14 planned tests exist and pass, the 14th is a
documented deviation that improved on the plan, every §6 gate is green (80 suites / 0 failures, fmt
clean, three clippy runs at `-D warnings`), and all four of §6's manual steps pass — including the
positive control against a real `bowtie2-build` index, which is the check that matters most under D1's
false-rejection risk.

What remains, in the order a reviewer should care:

1. **Gap 3 — the `HISAT2_INDEXES` twin test (§5b-12).** The only gap that leaves a real defect class
   unguarded: the hisat2 environment-variable arm has zero test coverage, so a swapped or misspelled
   name ships green. §5b-12 was written explicitly to prevent this. One test.
2. **Gap 2 — assert a rejected index creates nothing (§5b-11b).** The property holds today (verified
   by hand), but nothing protects the `resolve_output`-after-`detect_aligner` ordering it rests on.
   One assertion.
3. **Gap 4 — the two unrecorded §5c placement sabotages.** Record them with the reason each was
   skipped, or add the bad-genome-plus-bad-index test that would make the first observable and pin
   §3's "genome errors should also win" intent.
4. **Gap 1 — Task 1's one-sentence `--large-index` note.** Documentary only.

Not gaps, recorded so they are not re-litigated: §11's `25 → 36` count mixes runtime and attribute
totals (delta correct); the docs sentence sits just before rather than after the example block; and
§5a-3's "6 → Err" and §5a-10's `../` case are realised in forms that still catch the defects those
items target.

## Resolution

Verified **2026-08-16** against the merged squash **`7596b0d`** (PR #1102, on `dev`) — not the
uncommitted working tree this audit was taken from, which is why the verdict above went stale. PLAN §12
records the closures as Phase-5 review fixes; each is confirmed in the shipped code, and the three
tests were run rather than merely located.

| Gap | Items | Closed by | Evidence in `7596b0d` |
|---|---|---|---|
| 1 | 8 | The `--large-index`-unreachable note | `discovery.rs:195-196` — "Only two index sizes are probed because this route never emits `--large-index` (`five_base_build_argv` passes `-x` and the mates, nothing else)" |
| 2 | 32 | A rejected run now asserts nothing was created | `tests/aligner_cli.rs:6311` — `assert!(!nested.exists())` inside `five_base_index_missing_fails_early`, which also pins the error naming `--five_base_index` and the absence of "desync"/"panicked". **Runs green** |
| 3 | 31 | The hisat2 environment arm gained coverage | `discovery.rs:605` `index_env_name_maps_each_aligner` and `discovery.rs:614` `hisat2_env_fallback_accepted`. **Both run green** |
| 4 | 35, 38, 39 | Both unrun sabotages executed | PLAN §12, "Sabotage record — all **six** §5c claims now verified": the relocated check goes **1 red** with bowtie2 off `PATH` *with a same-PATH control passing*, and re-breaking #1099 goes **2 red** — so Task 2b's stub index did not weaken #1099's guard. Both guard tests run green today |

Two fixes landed in the same phase that are **not** gap closures but do change what this audit
concluded, so they belong here rather than being discovered later:

- **C1 — a real false rejection.** Reviewer B measured a working configuration this change rejected: a
  directory holding only `puc.2.bt2` with `BOWTIE2_INDEXES` pointing at a real index — real bowtie2
  exit 0, this check exit 1. The gate now keys on `<basename>.1.{bt2,bt2l}` alone, which is what the
  **binary** (`adjustEbwtBase`) decides on; the wrapper script's wider glob is not the deciding layer.
  §2/§7's "no false-reject case found" was **false as written**, and D1's "not stricter than the
  aligner" held only against the wrapper.
- **Converted-index rejection** — `reject_converted_five_base_index`, canonicalising so `..` and
  symlinks cannot slip past.

§2's separate conclusion that `$HISAT2_INDEXES` is broken upstream was also wrong, settled by reading
hisat2's `gfm.cpp`: `adjustEbwtBase` probes `<base>.1.<ext>` and on failure joins
`getenv("HISAT2_INDEXES")`, so the variable works via the binary. That is what makes Gap 3's tests
meaningful rather than pinning a dead path.

**No item of the 45 remains open.**

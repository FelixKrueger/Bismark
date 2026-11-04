# Plan Coverage Report

**Mode:** B (code, tests and run artefacts vs plan)
**Plan(s):** `PLAN.md` rev 6 (authoritative), amending rev 5, amending rev 4 (motivation/Goal/Behavior); `PREREGISTRATION.md` rev 2
**Worktree / branch:** `/Users/fkrueger/Github/Bismark-5base-bt2`, `feat/5base-bowtie2-defaults` @ `7295720` (base dev `d1764d0`)
**Date:** 2026-11-04
**Verdict:** INCOMPLETE — 4 items unresolved (3 PARTIAL, 1 DEVIATED), plus 2 PENDING by design (dual code review, PR into `dev`)

## Summary

- Total items: 56
- DONE: 50
- PARTIAL: 3
- MISSING: 0
- DEVIATED: 1 (undocumented, semantically equivalent)
- PENDING (in progress, not counted as gaps): 2

## Coverage ledger

### Builder (rev 4 Goal/Behavior, rev 5/6 Behavior)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | 5-Base Bowtie 2 default `--maxins 800` when no `-X` | rev 4 Goal | DONE | `options.rs:283` `cli.maxins.or((aligner == Bowtie2).then_some(800))` |
| 2 | Builder default string `-q --score-min L,0,-0.6 --maxins 800` | rev 4 Behavior | DONE | asserted in `five_base_defaults_and_explicit_defaults` and `five_base_bowtie2_defaults_to_maxins_800` |
| 3 | Engine argv `... --maxins 800 -p N --reorder` | rev 4 Behavior | DONE | `mod.rs` test + `aligner_cli::five_base_1134_default_argv_is_exact`; oxy reports show `-q --score-min L,0,-0.6 --maxins 800 -p 32 --reorder` |
| 4 | `-X N` → `--maxins N`; `-I N` → `--minins N --maxins 800` | rev 4 Behavior | DONE | unit cases `-X 500/1200`, `-I 600`, `-I 10` |
| 5 | No `--dovetail` token on 5-Base (both `--dovetail`, `--no_dovetail` emit nothing) | rev 4 Behavior, Q5 | DONE | unit cases + `--dovetail` back on the forbidden list in `five_base_omits_bisulfite_policy_and_runtime_threads` |
| 6 | `--no_dovetail` still sets driver TLEN sign (OB same-start pairs) | rev 4/rev 6 wording | DONE | driver untouched (`mod.rs` diff is test-only); docs use the rev 6 wording |
| 7 | `-I` > effective max insert errors, Bowtie 2 only, exact message | rev 5 + rev 6 Behavior | DONE | `options.rs:285-290`; HISAT2 excluded by `aligner == Bowtie2` filter |
| 8 | Check is on 5-Base path only; bisulfite path unchanged | rev 5 Behavior | DONE | change confined to `build_five_base_aligner_options` |
| 9 | HISAT2 and minimap2 unchanged | rev 4 Goal | DONE | asserted in `five_base_bowtie2_defaults_to_maxins_800` |
| 10 | One-line source comments | rev 3 outline / CLAUDE.md | DONE | two one-line comments |

### Tests (rev 5 list + rev 6 additions)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 11 | Exact strings: default, `-X 500`, `-X 1200`, `-I 600`, `--no_dovetail`, `--dovetail`, both, `--old_flag`, `--local` (`-q --local --score-min G,20,8 --maxins 800`) | rev 5 Tests | DONE | `five_base_bowtie2_defaults_to_maxins_800` |
| 12 | `-I 900 -X 800` errors naming 800 | rev 6 Tests | DONE | `five_base_bowtie2_rejects_minins_above_the_insert_cap` |
| 13 | `-I 600 -X 500` errors naming 500 | rev 6 Tests | DONE | same test |
| 14 | `-I 801` errors (and `-I 900`) | rev 5/6 Tests | DONE | same test |
| 15 | `-I 800` and `-I 900 -X 1000` pass | rev 5/6 Tests | DONE | exact-string cases |
| 16 | Exact error text asserted | rev 6 Tests | DONE | full message via `contains` in unit and CLI tests |
| 17 | HISAT2: no default `--maxins`; `-I 900` unchanged | rev 5 Tests | DONE | |
| 18 | minimap2 unchanged | rev 5 Tests | DONE | |
| 19 | `five_base_omits_bisulfite_policy_and_runtime_threads` forbids `--dovetail` | rev 5 Tests | DONE | |
| 20 | `five_base_defaults_and_explicit_defaults` / `..._token_order` without `--dovetail`, keeping `--maxins 800`/`1000` | rev 5 Tests | DONE | |
| 21 | `five_base_local_scoring_is_engine_specific` without `--dovetail`, keeping `--maxins 800` | rev 6 Tests | DONE | |
| 22 | `mod.rs` stub and input-order test with `--maxins 800`, no `--dovetail` | rev 5 Tests | DONE | `mod.rs:8422`, `:8433` |
| 23 | CLI exact default argv `-q --score-min L,0,-0.6 --maxins 800 -p 4 --reorder` | rev 5 Tests | DONE | |
| 24 | #1134 tuning test asserts `--dovetail` count 0 for both engines | rev 5 Tests | DONE | `aligner_cli.rs` added `assert!(!args...any(== "--dovetail"))` |
| 25 | Defaults CLI test: no `--dovetail` every case, `--maxins` value exactly once | rev 5 Tests | DONE | `five_base_bowtie2_maxins_default_reaches_argv`; `assert_option` asserts count 1 |
| 26 | `-I 900` joins `five_base_1134_invalid_tuning_fails_before_output_creation`, no output dir, no engine call, Bowtie 2 only | rev 6 Tests | DONE | `rejects("bowtie2", ["-I","900"], ...)` checks `output` and `argv` absent |
| 27 | End-to-end report assertion `-q --score-min L,0,-0.6 --maxins 800` | rev 5 Tests | DONE | `aligner_cli.rs:7112` |
| 28 | Renames: `..._defaults_to_maxins_800`, `..._maxins_default_reaches_argv` | rev 6 Tests | DONE | |
| 29 | Real-Bowtie 2 insert-cap gate: 1,000 bp excluded at default / admitted `-X 1200`; 600 bp admitted / excluded `-X 500` | rev 5 Tests | DONE | `five_base_pe_bowtie2_default_max_insert_is_800` (passes locally, bowtie2 on PATH) |
| 30 | Dovetail test renamed `five_base_pe_bowtie2_drops_dovetailed_pairs`; control + OT + OB; runs no flag / `--dovetail` / `--no_dovetail`; exactly 2 records, flags 99/147, TLEN ±200 | rev 5 Tests | DONE | |
| 31 | `align()` takes several named pairs; control check by QNAME | rev 6 Tests | DONE | `SmallGenome::align(&[(name, r1, r2)])`; `rec.inner().name() == "control"` |
| 32 | fmt, focused tests, release Clippy (step 1) | rev 5 outline 1 | DONE | re-run here: fmt clean, lib 614, bt2 3, CLI five_base 15, clippy `--release --all-targets -D warnings` clean |

### Harness (rev 5 + rev 6)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 33 | Arms B (`-X 500`), B-loc (`--local -X 500`), B-X (`-X 800`), B-new (defaults), B-old (rev 3, identification only); `--five_base_umi_qname` dropped except B-old | rev 5 Harness | DONE | `arm_flags`, `verify_options` need/deny lists |
| 34 | B-new `@PG` denies `-X`, `--maxins`, `--dovetail`, `--no_dovetail` | rev 5 Harness | DONE | bench lines 109-112 + self-test cases |
| 35 | `--maxins` exactly-once counts for all B arms | rev 5 Harness | DONE | `once=(--maxins)` |
| 36 | `BIN` overridable; deploy dir `~/5base_harness_bt2_r6`; binary `bin5` | rev 5/6 Harness | DONE | `BIN=${BIN:-$ROOT/bin}`; `versions_d27b11e.txt` shows `/var/tmp/5base_engines/bin5/bismark` |
| 37 | `cell … B-old` refuses with "rev 3 cross-check arm; not re-runnable" before any `rm -rf` | rev 6 Harness | DONE | check is the first statement of `cmd_cell` |
| 38 | Disjoint CPU sets 0-31 / 32-63 | rev 6 Harness | DONE | `CPUS` env + `taskset`; recorded in PREREG |
| 39 | `versions` writes `versions_<commit>.txt` | rev 6 Harness | DONE | `bench_results/versions_d27b11e.txt`; rev 3 `versions.txt` kept |
| 40 | `repro <sample> <tag> <arm> <ref-dir>` writes `repro_<arm>.tsv`, `repro_score_<arm>.tsv`, score label = arm | rev 6 Harness | DONE | `cmd_repro`, `own_cov10` |
| 41 | New `repro-compare` over repro columns + deconv counts + own-set n/r; writes `repro_compare_<a1>_<a2>.tsv`; any difference fails | rev 6 Harness | DONE | `cmd_repro_compare` / `compare_cells` |
| 42 | `rescued` draws: one `Random(1146)` draw per B-new pair in FASTQ order, whatever the set | rev 6 Harness | DONE | `rng.random()` before set labelling |
| 43 | Control (rest, draw < 0.01), admitted sample (admitted, < 0.10, every base), pool (rest, < 0.05); equal-size sample removed | rev 6 Harness | DONE | constants `CONTROL_FRAC/ADMIT_FRAC/POOL_FRAC` |
| 44 | `rest_long` (450 ≤ \|TLEN\| ≤ 500) tallied, report only | rev 6 Harness | DONE | |
| 45 | M-bias windows 0–9 and 60–69 from R1 3′; short reads only count to 0–9 | rev 6 Harness | DONE | `SetStats.add` skips a window if `len(xm) <= window[-1]` |
| 46 | TLEN reporting: p10/p50/p90 + 500–800 fraction | rev 6 Harness | DONE | |
| 47 | `verdict` takes old/new arm names and evaluates A1–A7 + secondary checks | rev 6 Harness | DONE | bench passes positional arms, helper takes `--old-arm/--new-arm` (CLI naming matches PREREG rev 2) |
| 48 | Duplex/consensus code and D0–D3 removed | rev 5 Harness | DONE | no matches in either file |
| 49 | Helper self-test coverage: `--all-bases` incl. sample draw, TLEN quantiles + 500–800 fraction, A5 matched-site, A6 diff-of-diffs, criteria with arbitrary arm names | rev 6 Self-test | DONE | `_selftest_units/_criteria/_end_to_end`; `selftest OK` |
| 50 | Helper self-test: a pass/fail boundary case for each of A1–A7 | rev 6 Self-test | PARTIAL | see Gaps |
| 51 | Bench self-test covers B-old refusal **and per-arm `repro` filenames** | rev 6 Self-test | PARTIAL | see Gaps |

### PREREGISTRATION rev 2, run and results

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 52 | PREREG rev 2: rev 1 withdrawn + clause lapses; own void clause; rev 6 definitions; seen-at-freeze table (NA12878 and HG002 B-old); binary `d27b11e` + harness `1c126c3` | rev 5/6 PREREG | DONE | committed `6e0aa32` 16:11:09Z, before the oxy launch at 16:13Z; `git diff 6e0aa32..HEAD` is empty for `PREREGISTRATION.md` and `validation/` |
| 53 | oxy run: NA12878 gate (`repro B-new` vs #1146 B-X) + `repro-compare B-old B-new`; HG002 B, B-new, `repro-compare`, `rescued`, `score`, `verdict` | rev 6 outline 4 | DONE | all files present under `bench_results/`; every row identical; `verdict_B_B-new.tsv` all PASS/INFO; reports show the expected option strings |
| 54 | `RESULTS.md` + small results committed; figures match artefacts | rev 5 outline 5 | DONE | `f6068ce`; RESULTS figures match `verdict_B_B-new.tsv` and the repro TSVs |

### Step 7 (docs, CHANGELOG, README, suite, reviews, PR)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 55a | `illumina-5-base.md` tuning text: 800 bp Bowtie 2 / 500 bp HISAT2; `-I` error with Bowtie 2; `--dovetail` no-op; `--no_dovetail` OB same-start TLEN line; "raise the cap" advice rewritten; "two libraries" | rev 5/6 Docs | DONE | |
| 55b | Engine table relabel: `-X 800` row → default; plain row → "before this change (500 bp cap)"; `--local` caveated as measured at 500 bp | rev 5 Docs | DEVIATED | see Gaps |
| 55c | `options/alignment.md`: 5-Base `-X` default exception; exception to "always passes `--dovetail`" | rev 5 Docs | DONE | |
| 55d | One-sentence CHANGELOG entry | rev 5 step 7 | DONE | |
| 55e | `rust/README.md`: aligner row **and** dated Milestones line | rev 6 Declined/Adopted | PARTIAL | see Gaps |
| 55f | Full suite | rev 5 step 7 | DONE | macOS: 31 bisulfite FIFO tests fail environmentally (per caller); Linux oxy at `d27b11e` 2329/0 (per caller). Consistent: `git diff d27b11e..HEAD -- rust/bismark/src rust/bismark/tests` is empty (only `rust/README.md` changed). Not yet recorded in PLAN.md implementation notes |
| 55g | Plan-manager audit | rev 5 step 7 | DONE | this report |
| 56a | Dual code review | rev 5 step 7 | PENDING | running in parallel; `CODE_REVIEW_A/B.md` not yet present |
| 56b | PR into `dev`, squash message "5-Base: Bowtie 2 defaults to `--maxins 800`" with dovetail history in body only | rev 5 step 7 | PENDING | not yet opened |

Not audited (out of this PR by the plan): rev 5 Follow-ups (minimap2 overhangs, bisulfite `--dovetail`, methylseq snapshots), which are for Felix to file. Step 6 (present to Felix) is not verifiable from artefacts; the committed docs step implies it happened.

## Gaps (detail)

### Item 50: helper self-test boundary cases for A1–A7

**Expected:** rev 6 Self-test: "a pass/fail boundary case for each of A1–A7". The self-test's own comment claims "each criterion passes at its edge and fails just beyond it".
**Found:** A1, A2, A4, A6, A7 have pass-at-edge and fail-beyond cases. A3 has only a fail case (CHG 1.6 vs 1.0, 0.1 pt beyond) and a pass at 1.0 vs 1.0, with no pass at the +0.5 edge. A5 has a fail case for the mismatch rate (1.005% vs 2×0.5%) and INSUFFICIENT for n, but no fail case for the A5 CHG/CHH `+1.0` sub-criteria and no pass at an edge.
**Gap:** add an A3 pass case at exactly rest + 0.5, and A5 CHG/CHH fail cases just beyond control + 1.0 (plus edge passes), or note the reduced coverage in PLAN.md.

### Item 51: bench self-test, per-arm `repro` filenames

**Expected:** rev 6 Self-test: "the bench self-test covers the B-old refusal and the per-arm `repro` filenames".
**Found:** the B-old refusal is tested (exact message). `compare_cells` is tested on equal and differing tuples. No self-test asserts that `repro` writes `repro_<arm>.tsv` / `repro_score_<arm>.tsv`, that `repro-compare` writes `repro_compare_<a1>_<a2>.tsv`, or that the score label is the arm name. The real run did produce the correctly named files, so the behaviour is right but the specified self-test is absent.
**Gap:** add a self-test of the per-arm output filenames, or record the omission as a deviation in PLAN.md.

### Item 55b: engine-table labels in `illumina-5-base.md`

**Expected:** rev 5 step 7: the plain Bowtie 2 row relabelled "before this change (500 bp cap)", and the `--local` row caveated as measured at 500 bp.
**Found:** rows read "Bowtie 2 (default, 800 bp)", "Bowtie 2 `-X 500`" and "Bowtie 2 `--local -X 500`". The meaning is the same (the 500 bp figure is shown as an explicit `-X 500` invocation, which is what a user would now type), but the wording differs and no deviation note exists.
**Gap:** record the label choice as a documented deviation in PLAN.md (no doc change needed if accepted).

### Item 55e: `rust/README.md` aligner row

**Expected:** rev 6 "Adopted instead": "the `rust/README.md` status-journal row (R4-B) ... the aligner row and a dated Milestones line."
**Found:** the dated Milestones line (2026-11-04) is added. The `bismark` (aligner) row in the per-tool table (line 161) is unchanged and does not mention the 5-Base Bowtie 2 800 bp default.
**Gap:** add a short clause to the aligner row's 5-Base text, or document why only the Milestones line was updated (the README calls the per-tool columns a historical record).

## Test verification

| Test | File | Status |
|------|------|--------|
| `cargo test -p bismark --lib aligner::` (614) | `rust/bismark/src/aligner/` | PASS |
| `five_base_bowtie2_defaults_to_maxins_800` | `options.rs` | PASS (within the 614) |
| `five_base_bowtie2_rejects_minins_above_the_insert_cap` | `options.rs` | PASS |
| `five_base_local_scoring_is_engine_specific`, `five_base_defaults_and_explicit_defaults`, `five_base_forwards_tuning_in_bisulfite_token_order`, `five_base_omits_bisulfite_policy_and_runtime_threads` | `options.rs` | PASS |
| `mod.rs` 5-Base stub / input-order tests | `mod.rs` | PASS |
| `five_base_pe_bowtie2_default_max_insert_is_800` | `tests/aligner_five_base_bowtie2.rs` | PASS (real bowtie2) |
| `five_base_pe_bowtie2_drops_dovetailed_pairs` | `tests/aligner_five_base_bowtie2.rs` | PASS (real bowtie2) |
| `five_base_pe_bowtie2_multithreaded_keeps_every_pair_in_order` | `tests/aligner_five_base_bowtie2.rs` | PASS |
| `cargo test -p bismark --test aligner_cli five_base` (15, incl. `five_base_1134_default_argv_is_exact`, `five_base_bowtie2_maxins_default_reaches_argv`, `five_base_1134_tuning_reaches_each_aligner_and_report`, `five_base_1134_invalid_tuning_fails_before_output_creation`, `five_base_bowtie2_unconverted_index_end_to_end`) | `tests/aligner_cli.rs` | PASS |
| `cargo fmt -p bismark -- --check` | — | PASS |
| `cargo clippy -p bismark --release --all-targets -- -D warnings` | — | PASS |
| `python3 -I validation/five_base_rescued.py --selftest` | `validation/five_base_rescued.py` | PASS (`selftest OK`) |
| `bash validation/five_base_engine_bench.sh selftest` | `validation/five_base_engine_bench.sh` | PASS (52/52) |
| Full suite | — | Not re-run here; Linux 2329/0 at `d27b11e` per caller, Rust unchanged since |

## Verdict

INCOMPLETE — 4 items unresolved, none of which affects shipped behaviour or the pre-registered outcome (every PREREG rev 2 check is PASS and the artefacts match `RESULTS.md`):

1. **Item 50:** the helper self-test lacks an A3 pass-at-edge case and A5 CHG/CHH fail cases (rev 6 asks for a pass/fail boundary for each of A1–A7).
2. **Item 51:** the bench self-test does not cover the per-arm `repro` / `repro-compare` output filenames (rev 6 Self-test).
3. **Item 55b:** the engine-table labels differ from rev 5's wording ("Bowtie 2 `-X 500`" rather than "before this change (500 bp cap)"); semantically equivalent; needs a deviation note or a relabel.
4. **Item 55e:** `rust/README.md` has the Milestones line but not the aligner-row update rev 6 adopted.

Pending by design: dual code review (item 56a) and the PR into `dev` with the specified squash message (item 56b).

Bookkeeping observations (not plan items): PLAN.md implementation notes do not yet list `f6068ce` (results) or `7295720` (docs), nor the Linux full-suite run; `PROGRESS.md` still says "Last updated 2026-11-02" and "docs/CHANGELOG await Felix".

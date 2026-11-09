# Plan Coverage Report

**Mode:** B (code vs. implementation plan — PLAN.md §5 served as the implementation plan, §4 as the test-file spec, §9 as the validation table)
**Plan(s):** `plans/08082026_legacy-perl-move/PLAN.md` (rev 2)
**Change audited:** branch `legacy-perl-move` (PR #1098 → `dev`), commits `dea61f9` (pure move) + `19cb7db` (path fixes)
**Date:** 2026-08-08
**Verdict:** **COMPLETE** — 0 items unresolved

## Summary

- Total items: **63** (51 §5 implementation items + 10 §9 validation rows + 2 whole-commit properties)
- DONE: **60**
- PARTIAL: 0
- MISSING: 0
- DEVIATED (all documented or coverage-positive): **3** — V2's 6th filter term (§12 dev. 1), the
  7th golden script repaired beyond the plan's table of 6 (undocumented but a superset), and V9
  not run (explicitly optional, deferred by the plan itself)

CI status is no longer pending: **all four workflow runs on `19cb7db` completed green**, so V6
and V8 are DONE rather than PENDING (evidence below).

## Coverage ledger — §5 implementation outline

### Step 0 — pre-move baseline

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | Record `N_baseline` ok-count + skip list on `dev` | §5 Step 0 | DONE | §12: **72 ok / 0 failed / 0 test-skips**; the 4 `skipping` lines identified as Perl `bismark2report` fixture chatter, not test skips |

### Commit 1 — pure move

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 2 | 12 Perl executables under `legacy_perl/` | §3.1 / §5 C1 | DONE | All 12 present: `bam2nuc bismark bismark2bedGraph bismark2report bismark2summary bismark_genome_preparation bismark_methylation_extractor coverage2cytosine deduplicate_bismark filter_non_conversion methylation_consistency NOMe_filtering` |
| 3 | `copy_bismark_files_for_release.pl` moved as historical artifact, accepted broken | §2 / Open-5 | DONE | `legacy_perl/copy_bismark_files_for_release.pl`; no fix attempted (as decided); rationale restated in the commit message |
| 4 | `plotly/` — 10 tracked files moved | §3.1 | DONE | 10 files, incl. the 4 code-read assets (`plot.ly`, `plotly_template.tpl`, `bismark.logo`, `bioinf.logo`) |
| 5 | `test_data.fastq` moved | §3.1 / Open-2 | DONE | `legacy_perl/test_data.fastq`; `docs/src/content/docs/installation.md:99,105` verified to be bare-filename v0.7.8 examples, not location claims — no edit needed |
| 6 | Mode bits preserved | Assumption 6 | DONE | `git ls-tree`: all 13 Perl files `100755`, plotly + fastq `100644` |
| 7 | Commit 1 is content-free (rename detection clean) | §5 C1 / §6 | DONE | `git show --stat dea61f9`: 24 files, **0 insertions / 0 deletions**, all rendered as `X => legacy_perl/X` renames |
| 8 | Sabotage observation between commits | §5 / G19 | DONE | §12 V1: **20 loud failures = exactly the predicted set** (2 plotly panics + 13 `perl_vs_rust_*` + 5 report `*_byte_identical`), 52+20=72; the 13 skip-capable summary/template tests passed with **zero** output |
| 9 | Branch pushed whole (Commit 1 alone wouldn't trigger `rust_ci.yml`) | §5 note / Assumption 9 | DONE | Both `rust_ci.yml` and `ci_tests.yml` ran on head `19cb7db` (push + pull_request) |

### Commit 2 (a) — the 8 Rust test literals

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 10 | `dedup_integration_dedup.rs` → `../../legacy_perl/deduplicate_bismark` | §5(a) | DONE | verified in diff |
| 11 | `extractor_nondir_swapped_flags_1030.rs` → `…/legacy_perl/bismark_methylation_extractor` | §5(a) | DONE | verified |
| 12 | `genome_prep_integration.rs` → `…/legacy_perl/bismark_genome_preparation` | §5(a) | DONE | doc comment updated too |
| 13 | `report_perl_vs_rust.rs:64` → `repo_root().join("legacy_perl/bismark2report")` | §5(a) | DONE | `repo_root()` left meaning repo root, as specified |
| 14 | `summary_perl_oracle.rs` script literal | §5(a) | DONE | verified |
| 15 | `summary_perl_oracle.rs` plotly literal | §5(a) | DONE | `../../legacy_perl/plotly/plot.ly`; stale `bismark-summary` crate-dir comment also corrected to `rust/bismark` |
| 16 | `summary_template_drift.rs:12` | §5(a) | DONE | verified |
| 17 | `methylation_consistency_integration.rs:466` | §5(a) | DONE | doc comment updated too |

### Commit 2 (b)–(c) — drift guards and the new layout gate

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 18 | `report/assets.rs` base → `../../legacy_perl/plotly` + adjacent comment adjusted | §5(b) | DONE | comment now names the new path |
| 19 | `summary/assets.rs` base → `../../legacy_perl/plotly` + adjacent comment adjusted | §5(b) | DONE | same |
| 20 | Historical narrations left untouched | §5(b) | DONE | `report/assets.rs:7` and `summary/assets.rs:18` still read `../../plotly/` / `../../../plotly/` — the only two hits of the §5 `.rs` idiom grep, exactly as §12 V5 records |
| 21 | New `tests/legacy_perl_layout.rs` per §4 | §3.3 / §4 / §5(c) | DONE | 28 lines, rustfmt-expanded; all **16** names asserted (12 scripts + 4 plotly assets); unconditional, no Perl needed, no skip path |

### Commit 2 (d) — golden-generation scripts

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 22 | `data/bam2nuc/generate_goldens.sh` 4→5 ups + `legacy_perl/bam2nuc` | §5(d) | DONE | resolves to `/Users/fkrueger/Github/Bismark` (re-verified) |
| 23 | `data/filter_nonconversion/generate_goldens.sh` hardcoded absolute → computed 5-up root, `PERL_FNC` override kept | §5(d) | DONE | `PERL_FNC="${PERL_FNC:-$(cd …/../../../../.. && pwd)/legacy_perl/filter_non_conversion}"` |
| 24 | `data/coverage2cytosine/phase1/generate_goldens.sh` 5→6 ups | §5(d) | DONE | re-verified |
| 25 | `data/coverage2cytosine/phase_b/generate_goldens.sh` 5→6 ups | §5(d) | DONE | re-verified |
| 26 | `data/coverage2cytosine/phase2_drach/generate_goldens.sh` 5→6 ups | §5(d) | DONE | re-verified |
| 27 | `data/nome_filtering/phase_b/generate_goldens.sh` 5→6 ups | §5(d) | DONE | re-verified |
| 28 | `data/coverage2cytosine/phase3_ffs/generate_goldens.sh` | not in §5(d) | **DEVIATED (extra)** | Same relic class, also repaired (5→6 ups + `legacy_perl/`). Coverage-positive superset; the plan table and the commit message both say "six" while seven were fixed. See Deviations. |
| 29 | Cosmetic hints: `nome_gate.sh:8,19` | §5(d) | DONE | both the usage example and the `PERL_NOME:?` error string updated |
| 30 | Cosmetic hint: `filter_nonconversion_byte_identity_real_data.rs:17` | §5(d) | DONE | doc example path updated |

Completeness check beyond the plan: all **8** `*.sh` files under `rust/bismark/tests/data/` are
accounted for — the 7 golden scripts (all repaired) plus `nome_gate.sh` (env-driven, hint only).
No further script in that tree carries the stale up-count.

### Commit 2 (e) — `scripts/` byte-identity harnesses

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 31 | `c2c_byte_identity_matrix.sh:120` default + usage text `:29` | §5(e) | DONE | both hunks present |
| 32 | `phase_h_smoke.sh:134` default + usage text `:57` | §5(e) | DONE | both hunks present |
| 33 | `phase_h_se_matrix.sh:116` default | §5(e) | DONE | no path-bearing usage text in this file (nothing stale left) |
| 34 | `phase_h_pe_matrix.sh:163` default | §5(e) | DONE | same |

### Commit 2 (f) — `ci_tests.yml`

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 35 | All **44** invocations → `./legacy_perl/<name>` | §5(f) / §3.6 | DONE | 44 occurrences of `./legacy_perl/<tool>` counted; §12 dev. 4 records longest-name-first `replace_all` instead of sed |
| 36 | No line-anchored pattern — inline `:37` converted | §5(f) | DONE | `run: ./legacy_perl/bismark --help` |
| 37 | `./test_files/` args, output refs, and step labels untouched | §5(f) | DONE | spot-checked `:92–118`: `--genome ./test_files/` intact, `- name: coverage2cytosine` / `- name: bam2nuc` labels unchanged |
| 38 | No trigger-filter change needed | §5(f) / Assumption 3 | DONE | `:5` is still `on: [push, pull_request]` |

### Commit 2 (g) — attributes / ignores

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 39 | `.gitattributes:10` `plotly/**` → `legacy_perl/plotly/**`, unanchored `:11` kept | §5(g) | DONE | both lines present; linguist-vendored coverage preserved throughout the transition (Assumption 10) |
| 40 | `.dockerignore` gains `legacy_perl/` with a one-line reason | §5(g) / §3.8 | DONE | `# Legacy Perl toolchain — nothing in it is consumed by the image build.` |
| 41 | `.prettierignore` deliberately **not** edited | §3.8 | DONE | zero diff against `dev` |

### Commit 2 (h) — prose

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 42 | README banner | §5(h) / §3.9 | DONE | "(the scripts at this repo root)" → "(now in [`legacy_perl/`](legacy_perl))" |
| 43 | README Legacy section | §5(h) | DONE | "remain at this repo root" → "live in [`legacy_perl/`](legacy_perl)" |
| 44 | CHANGELOG unreleased entry | §5(h) / §7 | DONE | New "Repository layout" block: the move, no packaging impact, broken `blob/master/<script>` deep links, `v0.25.1` as the stable Perl reference, PATH-users note |
| 45 | `rust/README.md:102` reworded true on both `dev` and at the tag | §5(h) / §3.9 | DONE | "live in `legacy_perl/` (at the `v0.25.1` tag: the repository root)" |
| 46 | docs + CONTRIBUTING sweep | §5(h) / §3.9 | DONE | Sweep for root-location claims across `docs/**.md` + `CONTRIBUTING.md` returns **zero** — near-empty as predicted; the only `test_data.fastq` mentions are bare-filename v0.7.8 output examples |
| 47 | Project `CLAUDE.md` updated | §5(h) | DONE (deviation 5) | `CLAUDE.md:3` and `:51` now say `legacy_perl/`; file remains untracked, its prior status |
| 48 | `.claude/settings.local.json` staleness noted, not fixed | §5(h) | DONE | Stale absolute-path entries confirmed present at `:7–11`; untracked, outside the PR, re-prompts only — exactly the documented expectation, no action required |

### Verify block

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 49 | `cargo fmt --all -- --check` | §5 verify | DONE | **re-run by this audit: clean**; CI `cargo fmt --check` job also green |
| 50 | `cargo clippy -p bismark --all-targets -- -D warnings`, default + `rammap-inprocess` | §5 verify / G26/G27 | DONE | §12 records both clean (dev. 3: the rammap run needed an unsandboxed cargo for a registry unpack — environmental); CI `cargo clippy` + both feature build/test jobs green |
| 51 | Straggler sweep — positive checklist then the three idiom greps | §5 verify / V5 | DONE | See V5 below |
| 52 | PR into `dev` | §5 verify / §7 | DONE | PR **#1098**, `legacy-perl-move` → `dev`, OPEN |

### Explicitly-unchanged invariants (§3)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 53 | `license.txt` stays at repo root | §3 / Assumption 4 | DONE | still at root; `release.yml:137` `cp license.txt` unaffected |
| 54 | Both Python coverage helpers stay at root | §3 / Open-1 (Felix) | DONE | `merge_arbitrary_coverage_files.py`, `merge_coverage_files_ARGV.py` still at root |
| 55 | No version literals touched | §3 / §7 | DONE | Diff contains no `Cargo.toml`, `Cargo.lock`, or `VERSION` file |
| 56 | `test_files/`, `docs/`, `Dockerfile`, `docker/`, `validation/`, PDF, `RELEASE_CHECKLIST*.md`, `rust_ci.yml` unchanged | §3 | DONE | None appear in the diff; `Dockerfile:73–75` and `release.yml:131–168` re-confirmed as Rust multicall symlink loops (`ln -s bismark.bin`/`ln -s bismark`), not move-set consumers |

## Coverage ledger — §9 validation rows

| # | Row | Status | Evidence |
|---|-----|--------|----------|
| 57 | **V0** pre-move baseline | DONE | §12: 72 ok / 0 failed / 0 test-skips on `dev` |
| 58 | **V1** sabotage — moved paths are load-bearing | DONE | §12: 20 loud failures = exactly the predicted set; 52+20=72 accounts for the whole population |
| 59 | **V2** no oracle quietly switched off | DONE (deviation 1) | §12: **73 ok = N_baseline + 1** (the layout test), 0 failed, same 4 chatter lines. Deviation: a 6th filter term `legacy_perl_toolchain` was required — none of the plan's five name filters matches the new test |
| 60 | **V3** layout gate, sabotaged once | DONE | §12: observed **FAILED** with `NOMe_filtering` renamed away, ok after restore. **Re-run by this audit:** `cargo test -p bismark --test legacy_perl_layout` → `1 passed; 0 failed` |
| 61 | **V4** drift guards against canonical plotly | DONE | **Re-run by this audit:** `cargo test -p bismark --lib -- match_repo_plotly_files` → `report::assets::tests::embedded_assets_match_repo_plotly_files ... ok`, `summary::assets::tests::vendored_assets_match_repo_plotly_files ... ok` (2 passed) |
| 62 | **V5** no straggler paths | DONE | Checklist rows (a)–(f) all ticked above. **All three §5 greps re-run verbatim:** `.rs` idiom → only the 2 deliberately-kept historical comments; `.sh` idiom → zero; `ci_tests.yml` non-prefixed → zero. Plus a broader repo-wide sweep (all `*.rs/*.sh/*.yml/*.toml/Dockerfile`, target and plans excluded): every remaining hit is a `tests/data/` fixture directory, a conda-env `PERL_BIN` default (`$HOME/micromamba/envs/bismark-test/bin/…`, move-immune), or documentation prose |
| 63 | **V6** runtime plotly + workflow health | DONE (was PENDING) | `ci_tests.yml` **success** on the PR run `31246781842` (and on the push run `31246768936`), head `19cb7db`, via `gh run view --json jobs` |
| 64 | **V7** local Perl smoke incl. a `$RealBin` coupling | DONE | §12: `bismark --version` ok; `perl legacy_perl/bismark2report` over the `wgbs_pe` fixture produced HTML with plotly spliced (0 placeholders, 19 payload markers). **Re-checked by this audit:** `./legacy_perl/bismark --version` prints `v0.25.1`; `$RealBin` couplings intact at `bismark2report:1029`, `bismark2summary:140`, `bismark_methylation_extractor:377,424` — all now resolving inside `legacy_perl/`. Also confirmed the `use lib "$RealBin/../lib"` edge case is a genuine no-op on both sides: neither `<repo>/lib` nor `<repo>/../lib` exists |
| 65 | **V8** whole-suite health, both workflows, all jobs enumerated | DONE (was PENDING) | All four runs on `19cb7db` **success**. `Rust CI` (PR run `31246781836`) jobs: `cargo clippy`, `cargo fmt --check`, `cargo test`, `perl-oracle byte-identity`, `cargo build/test (rammap-inprocess feature)`, `cargo build/test (binseq-input feature)` — all success. `Bismark CI workflow` (`31246781842`): `BismarkCI` success. **Bonus proof:** the `perl-oracle` job's own step asserts `n == EXPECTED (13)` and fails on any `^skipping:` line — its green is independent CI-side confirmation that all 13 oracles really executed against the `legacy_perl/` paths, i.e. the plan's goal of "oracle count proven unchanged" holds on CI, not just locally |
| 66 | **V9** `.dockerignore` risk retirement (cheap, optional) | DEVIATED — deferred by design | §12: not run locally; the plan itself marks V9 optional and routes it to the next `release.yml dry_run=true`. `.dockerignore` correctness is low-risk: nothing under `legacy_perl/` is `COPY`ed or built |

## Deviations (detail)

### D1 — V2 battery needed a 6th filter term (documented, §12 dev. 1)

**Expected:** the five §5 Step-0 filter strings suffice for the post-fix battery.
**Found:** none of them matches `legacy_perl_toolchain_is_present`, so `N_baseline + 1` was
unmeasurable without adding `legacy_perl_toolchain`.
**Assessment:** documented in §12, measurement-preserving, no code impact. Not a gap.

### D2 — a 7th golden script was repaired (coverage-positive, not in §12's deviation list)

**Expected:** §5(d) enumerates **six** golden scripts; the commit message likewise says "six".
**Found:** `rust/bismark/tests/data/coverage2cytosine/phase3_ffs/generate_goldens.sh` was also
repaired (5→6 ups + `legacy_perl/coverage2cytosine`) — same relic, same fix.
**Assessment:** a strict superset of the plan and the right call (the plan's inventory of that
class was one short). The only shortfall is documentary: §12's deviation list and the commit
message still say six. Coverage is unaffected; worth a one-line note if the commit message is
ever amended.

### D3 — V9 not run (deferred by the plan itself)

**Expected:** optional local `docker build` reaching the `cargo build` layer.
**Found:** not run; §12 defers it to the next `release.yml dry_run`.
**Assessment:** the plan marks the row "(cheap, optional)" and names the deferral target, so
this is executed-as-written. Residual risk is the one §11 already lists.

## Test verification

| Test / gate | File or job | Status |
|---|---|---|
| `legacy_perl_toolchain_is_present` | `rust/bismark/tests/legacy_perl_layout.rs` | **PASS** (re-run: 1 passed, 0 failed) |
| `report::assets::tests::embedded_assets_match_repo_plotly_files` | `rust/bismark/src/report/assets.rs` | **PASS** (re-run) |
| `summary::assets::tests::vendored_assets_match_repo_plotly_files` | `rust/bismark/src/summary/assets.rs` | **PASS** (re-run) |
| Oracle battery, post-fix | 8 affected test files | **PASS** — 73 ok = baseline + 1, 0 failed (§12 V2) |
| 13 `perl_vs_rust_*` oracles, count-asserted | CI job `perl-oracle byte-identity` | **PASS** — `n == 13`, no `^skipping:` line |
| `cargo test` (full suite) | CI job, `Rust CI` run `31246781836` | **PASS** |
| `cargo build/test (rammap-inprocess feature)` | CI job | **PASS** |
| `cargo build/test (binseq-input feature)` | CI job | **PASS** |
| `cargo clippy` / `cargo fmt --check` | CI jobs | **PASS** (fmt also re-run locally: clean) |
| Perl toolchain end-to-end | CI workflow `Bismark CI workflow`, 44 `./legacy_perl/<name>` invocations | **PASS** |
| Three §5 straggler greps | `rust/**`, `scripts/**`, `ci_tests.yml` | **PASS** (only the 2 intentional historical comments) |
| §5 `.sh` up-count arithmetic | 7 golden scripts | **PASS** — all 7 resolve to `/Users/fkrueger/Github/Bismark` |

## Verdict

**COMPLETE.** Every task in PLAN.md §5 and every validation row in §9 is satisfied. Nothing is
MISSING or PARTIAL. The three DEVIATED items are the two §12-documented measurement/tooling
adjustments plus one coverage-positive extra (a seventh golden script), and the optional V9 row
the plan itself defers.

Two facts worth carrying forward, neither a gap:

1. **V6 and V8 are green, not pending** — all four runs on head `19cb7db` (push and
   pull_request × both workflows) completed success, with all seven `Rust CI` jobs enumerated.
   The `perl-oracle` job's `n == 13` assertion plus its `^skipping:` tripwire make the "no
   oracle quietly switched off" claim CI-enforced, not only locally observed.
2. **The §13 follow-ups remain open by design** — `perl-oracle` at `EXPECTED=26` and a
   centralized `legacy_perl_script()` helper were deliberately not absorbed into this move. The
   13 skip-capable summary/template oracles are still guarded only by the fixed 16-name layout
   test plus the ok-count baseline, exactly as §11 states.

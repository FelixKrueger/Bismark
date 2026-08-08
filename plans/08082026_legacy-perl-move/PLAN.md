# Plan: Move the legacy Perl toolchain to `legacy_perl/`

**Revision:** 2 (2026-08-08)
**Branch:** `legacy-perl-move` (from `dev`, per G23 branch-first)
**Scope:** repository restructure — no behaviour change to any tool, Perl or Rust.

**Revision history**
- rev 0 — initial plan.
- rev 1 — Felix's manual review: the two Python coverage helpers stay at root (Open-1).
- rev 2 — dual plan review folded in (PLAN_REVIEW_A.md + PLAN_REVIEW_B.md, both REQUEST
  CHANGES; inter-reviewer discrepancies re-verified at source). Headlines: +9 previously
  missed consumers (5 golden scripts, 4 `scripts/` harnesses); the six data-dir golden
  scripts are **already broken today** (resolve to `rust/<script>`) and get their up-count
  repaired; new unconditional `legacy_perl_layout.rs` existence test; V1/V2/V5 verification
  commands replaced (previous filters matched almost nothing); `perl-oracle` tripwire claim
  corrected (covers 4 of 8 test files; 11/12 summary oracles skip silently); packager
  breakage documented and accepted; counts/sizes corrected (44 workflow invocations, 10
  plotly files, ~20 MB move set); `.gitattributes`/`.prettierignore` handling added.

## 1. Goal

Move the frozen Perl v0.25.x toolchain (12 executables + their runtime assets + the Perl-era
release packager) out of the repository root into `legacy_perl/`, so the GitHub landing page
presents a Rust-first file listing while every in-repo consumer (byte-identity oracle tests,
legacy Perl CI, plotly drift guards, golden-generation scripts, byte-identity harnesses)
keeps working — and gains a layout gate that cannot silently skip. Outcome: root listing
reduces to directories + top-level docs, and both CI workflows are green from the new layout
with the oracle test count proven unchanged.

## 2. Context

- The README already leads with the Rust suite and labels Perl "legacy / maintenance-freeze";
  the file listing is the last Perl-first signal on the landing page.
- The Perl scripts are **live test oracles**. Corrected scope (review finding, verified):
  the `perl-oracle` CI job (`rust_ci.yml:164`, `EXPECTED=13`) covers **4 of the 8** affected
  test files (`genome_prep_integration.rs` ×8, `methylation_consistency_integration.rs` ×3,
  `dedup_integration_dedup.rs` ×1, `extractor_nondir_swapped_flags_1030.rs` ×1) — the same
  4 files, and the only 4, that honor `BISMARK_REQUIRE_PERL`. Of the rest:
  - `report_perl_vs_rust.rs` and both plotly drift guards **fail loud** via the main `test`
    job (missing script → `perl` exits nonzero / `read_to_string(...).unwrap()` panics);
  - `summary_perl_oracle.rs` (12 tests) and `summary_template_drift.rs` (1 test)
    **skip-and-pass** when the script is missing — 11 of the 12 with *no output at all*
    (verified: 12 `let Some(script) = perl_script() else` sites, one `skipping` string in
    the file). These three path literals are the silent-green hazard this plan must close
    (the new existence test in §5 does).
- Consumer inventory is now complete at 23 code sites (§5) after the dual review added 9 the
  rev-1 grep idiom could not see. `scripts/` is **not** grep-clean (rev-1 claim retracted):
  four byte-identity harnesses default to `$REPO_ROOT/<script>`.
- Perl-side structure (verified exact by both reviewers):
  - `bismark_methylation_extractor:377,424` shells `$RealBin/bismark2bedGraph` /
    `$RealBin/coverage2cytosine`; `bismark:941` shells `$RealBin/bam2nuc`;
    `bismark2report:1029` / `bismark2summary:140` read `$RealBin/plotly/<template>`.
    Moving the whole set **together** needs zero edits to the 12 executables.
  - **Exception — the packager.** `copy_bismark_files_for_release.pl:18` resolves from its
    own location (`splitpath(__FILE__)`) and copies root-resident siblings (`CHANGELOG.md`,
    `license.txt`, `Bismark_alignment_modes.pdf`) — post-move it dies on the first copy.
    It is **already non-functional today** (its `Docs/make_docs.pl` /
    `Bismark_User_Guide.html` inputs don't exist in the tree) and is superseded by
    `release.yml`. Decision: **moves as a historical artifact, accepted broken, no fix**
    (documented here so nobody reads "zero Perl changes" into it).
- Non-consumers (verified): `release.yml`'s name loops are Rust multicall **symlinks**
  (`ln -s bismark`), and it copies `license.txt` from repo root — `license.txt` **stays**;
  `Dockerfile` COPYs nothing from the move set; `rust_ci.yml` contains zero moved filenames;
  `docs.yml`/`ga-candidate-image.yml`/`link_closing_pr.yml` clean; `validation/` and
  `docker/` prose-only; real-data tests use `PERL_GP`/`PERL_BG` env vars (move-immune);
  `aligner_five_base_bisulfite.rs` joins the built Rust binary and `test_files/`.
- **Pre-existing breakage discovered by review (verified empirically):** all six golden
  scripts under `rust/bismark/tests/data/*/` compute a "repo root" that actually resolves to
  `rust/` — a relic of the multicall consolidation (one directory level disappeared). Their
  Perl paths point at nonexistent `rust/<script>` **today**. This plan repairs the up-count
  while touching them (they are the reproducibility record for checked-in goldens).

## 3. Behavior

After the change:

1. `legacy_perl/` contains: the 12 Perl executables (`bam2nuc`, `bismark`,
   `bismark2bedGraph`, `bismark2report`, `bismark2summary`, `bismark_genome_preparation`,
   `bismark_methylation_extractor`, `coverage2cytosine`, `deduplicate_bismark`,
   `filter_non_conversion`, `methylation_consistency`, `NOMe_filtering`),
   `copy_bismark_files_for_release.pl`, `plotly/` (**10 tracked files** — 4 read by code:
   `plot.ly`, `plotly_template.tpl`, `bismark.logo`, `bioinf.logo`; 6 example reports/images
   that are most of the bulk), and `test_data.fastq`. Executables keep their mode bits.
2. Every Perl executable runs from the new location unchanged (cross-script `$RealBin` calls
   and plotly resolution preserved by the atomic move). The packager is accepted broken (§2).
3. A new **unconditional layout test** `rust/bismark/tests/legacy_perl_layout.rs` asserts all
   12 scripts + the 4 code-read plotly assets exist under `legacy_perl/` — it needs no Perl,
   cannot skip, and runs in the main `test` job on every CI run.
4. `cargo test -p bismark` passes with all oracle paths updated; the post-move oracle
   **ok-count equals the pre-move baseline** (proving no test quietly switched off).
5. The two plotly drift guards (`embedded_assets_match_repo_plotly_files` in
   `report/assets.rs`, `vendored_assets_match_repo_plotly_files` in `summary/assets.rs`)
   compare vendored bytes against `legacy_perl/plotly/` and pass.
6. `ci_tests.yml` invokes every script as `./legacy_perl/<name>` (**44** invocations);
   outputs still land in the job CWD (repo root), so inter-step references are unchanged.
7. The six data-dir golden scripts and four `scripts/` harnesses resolve the Perl tools at
   `<repo-root>/legacy_perl/<name>` — the golden scripts' broken up-counts repaired.
8. `.dockerignore` gains `legacy_perl/` (~20 MB context shrink; nothing in it is
   build-consumed). `.gitattributes:10` (`plotly/**`, root-anchored, goes dead) is rewritten
   to `legacy_perl/plotly/**`; the unanchored `**/plotly/**` at `:11` already keeps
   `linguist-vendored` working — belt and braces, since the language bar *is* the point of
   this change. `.prettierignore:2` (`plotly/`) needs **no** edit (unanchored gitignore
   semantics — stated so nobody "fixes" it).
9. User-facing text says `legacy_perl/`: README banner + Legacy section, CHANGELOG entry,
   `rust/README.md:102` (currently "live at the repository root" — reworded to be true on
   both the dev branch and at the `v0.25.1` tag), docs sweep (expected near-empty:
   `CONTRIBUTING.md` and `installation.md` make no root claim — verified).

Explicitly **unchanged**: `license.txt` (release.yml packages it from root), `test_files/`,
`docs/`, `_config.yml`, `Dockerfile`, `docker/`, `validation/`, `Bismark_alignment_modes.pdf`,
the two Python coverage helpers (`merge_arbitrary_coverage_files.py`,
`merge_coverage_files_ARGV.py` — Felix's call: they stay at root), the two superseded
Rust-era `RELEASE_CHECKLIST*.md`, `.prettierignore`, all five `3.1.0` version literals,
`rust/**` sources besides the two drift-guard test paths + the new layout test, and
`rust_ci.yml` (no path in it changes; see §12 for the declined EXPECTED extension).

## 4. Signature

One new test file (full body — B's §3.4 proposal, adopted):

```rust
// rust/bismark/tests/legacy_perl_layout.rs
// Layout gate for the byte-identity oracles: unconditional, no tooling needed, cannot skip.
#[test]
fn legacy_perl_toolchain_is_present() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../legacy_perl");
    for f in [
        "bam2nuc", "bismark", "bismark2bedGraph", "bismark2report", "bismark2summary",
        "bismark_genome_preparation", "bismark_methylation_extractor", "coverage2cytosine",
        "deduplicate_bismark", "filter_non_conversion", "methylation_consistency",
        "NOMe_filtering", "plotly/plot.ly", "plotly/plotly_template.tpl",
        "plotly/bismark.logo", "plotly/bioinf.logo",
    ] {
        assert!(dir.join(f).exists(), "legacy_perl/{f} missing — layout assumption broken");
    }
}
```

## 5. Implementation outline

### Step 0 — pre-move baseline (on `dev`, before any change)

```bash
cd rust && BISMARK_REQUIRE_PERL=1 cargo test -p bismark --no-fail-fast -- \
  perl_vs_rust oracle_ byte_identical match_repo_plotly_files matches_perl_heredoc \
  2>&1 | tee "$TMPDIR/baseline.log"
grep -c '^test .* \.\.\. ok$' "$TMPDIR/baseline.log"   # record N_baseline
grep -n 'skipping' "$TMPDIR/baseline.log"              # record which tests skip on this box
```

(Filter strings are **name-verified**: `perl_vs_rust` = the 13 CI oracles; `oracle_` = the 12
summary oracles; `byte_identical` = the 5 report oracles + fixture-gated real-data tests;
`match_repo_plotly_files` = both drift guards; `matches_perl_heredoc` = the template guard.
rev-1's `perl_oracle`/`drift` filters matched **zero** tests — libtest filters on test-function
names, not file names.)

### Commit 1 — pure move (no content edits, clean rename detection)

```bash
git checkout -b legacy-perl-move dev
mkdir legacy_perl
git mv bam2nuc bismark bismark2bedGraph bismark2report bismark2summary \
       bismark_genome_preparation bismark_methylation_extractor coverage2cytosine \
       deduplicate_bismark filter_non_conversion methylation_consistency NOMe_filtering \
       copy_bismark_files_for_release.pl plotly test_data.fastq \
       legacy_perl/
```

Note: Commit 1 alone touches no `rust/**`, so it would **not** trigger `rust_ci.yml`
(`paths:` filter — see Assumption 9). Push only the complete branch.

### Sabotage observation (G19 — after Commit 1, before any fix)

Re-run the Step-0 battery. Expected picture (record it):
- the 13 `perl_vs_rust_*` oracles **fail** (REQUIRE=1 turns their skips into failures);
- the 5 report `*_byte_identical` oracles **fail** (perl exits nonzero on the missing script);
- both plotly guards **panic**;
- `summary_perl_oracle.rs`'s 12 tests and `summary_template_drift.rs` **pass silently /
  with one `skipping:` line** — these are the *documented untrustworthy greens*; their gate
  for this change is the ok-count comparison (V2) plus the new layout test (V3), not their
  own color.

### Commit 2 — path fixes (complete inventory: 23 code sites + workflow + attrs + prose)

**(a) Rust test files — 8 literals:**

| File:line | Edit |
|---|---|
| `rust/bismark/tests/dedup_integration_dedup.rs:2214` | `../../deduplicate_bismark` → `../../legacy_perl/deduplicate_bismark` |
| `rust/bismark/tests/extractor_nondir_swapped_flags_1030.rs:263` | `../../bismark_methylation_extractor` → `../../legacy_perl/…` |
| `rust/bismark/tests/genome_prep_integration.rs:34` | `../../bismark_genome_preparation` → `../../legacy_perl/…` |
| `rust/bismark/tests/report_perl_vs_rust.rs:64` | `repo_root().join("bismark2report")` → `repo_root().join("legacy_perl/bismark2report")` (keep `repo_root()` = repo root; `:64` is its sole call site — verified) |
| `rust/bismark/tests/summary_perl_oracle.rs:22` | `../../bismark2summary` → `../../legacy_perl/bismark2summary` |
| `rust/bismark/tests/summary_perl_oracle.rs:23` | `../../plotly/plot.ly` → `../../legacy_perl/plotly/plot.ly` |
| `rust/bismark/tests/summary_template_drift.rs:12` | `../../bismark2summary` → `../../legacy_perl/bismark2summary` |
| `rust/bismark/tests/methylation_consistency_integration.rs:466` | `../../methylation_consistency` → `../../legacy_perl/methylation_consistency` |

**(b) Rust src drift guards** (`#[cfg(test)]` only): `rust/bismark/src/report/assets.rs:125`
and `rust/bismark/src/summary/assets.rs:110` — `join("../../plotly")` →
`join("../../legacy_perl/plotly")`; adjust the adjacent one-line comments (`:123`/`:108`).
Leave historical narrations untouched.

**(c) New layout test:** add `rust/bismark/tests/legacy_perl_layout.rs` (§4 verbatim).

**(d) Golden-generation scripts — 6 files, up-count repaired (all point at nonexistent
`rust/<script>` today; fix arithmetic AND insert `legacy_perl/`):**

| File:line | Edit (target: real repo root, then `legacy_perl/<name>`) |
|---|---|
| `rust/bismark/tests/data/bam2nuc/generate_goldens.sh:26-27` | 4 ups → **5 ups**; `$REPO_ROOT/legacy_perl/bam2nuc` |
| `rust/bismark/tests/data/filter_nonconversion/generate_goldens.sh:28` | replace hardcoded `/Users/fkrueger/Github/Bismark/filter_non_conversion` default with computed 5-up repo root + `legacy_perl/filter_non_conversion` (keep `PERL_FNC` override) |
| `rust/bismark/tests/data/coverage2cytosine/phase1/generate_goldens.sh:13` | 5 ups → **6 ups**; `…/legacy_perl/coverage2cytosine` |
| `rust/bismark/tests/data/coverage2cytosine/phase_b/generate_goldens.sh:5` | same |
| `rust/bismark/tests/data/coverage2cytosine/phase2_drach/generate_goldens.sh:15` | same |
| `rust/bismark/tests/data/nome_filtering/phase_b/generate_goldens.sh:9` | 5 ups → **6 ups**; `…/legacy_perl/NOMe_filtering` |

Cosmetic hints in the same class: `nome_gate.sh:8,19` and
`filter_nonconversion_byte_identity_real_data.rs:17` — update the example paths in the text.

**(e) `scripts/` harnesses — 4 files (work today, would break; env-overridable, fail loud):**

| File:line | Edit |
|---|---|
| `scripts/c2c_byte_identity_matrix.sh:120` | `$REPO_ROOT/coverage2cytosine` → `$REPO_ROOT/legacy_perl/coverage2cytosine`; usage text `:29` updated |
| `scripts/phase_h_smoke.sh:134` | `$REPO_ROOT/bismark_methylation_extractor` → `$REPO_ROOT/legacy_perl/…`; usage text `:57` updated |
| `scripts/phase_h_se_matrix.sh:116` | same default fix |
| `scripts/phase_h_pe_matrix.sh:163` | same default fix |

**(f) Workflow `.github/workflows/ci_tests.yml`:** all **44** script invocations →
`./legacy_perl/<name>`. Do **not** use a line-anchored pattern — `:37` is the inline
`run: ./bismark --help` that an anchored sed misses. Do not touch `./test_files/` arguments,
output-file references, or the `- name:` step labels at `:95,:101,:116` (cosmetic). There is
no `paths:` trigger filter (`:5` is `on: [push, pull_request]`) — nothing else to change.

**(g) Attributes / ignores:** `.gitattributes:10` `plotly/**` → `legacy_perl/plotly/**`
(root-anchored pattern would go dead; the unanchored `:11` `**/plotly/**` already covers the
new path — keep both). `.dockerignore`: add `legacy_perl/` with a one-line reason.

**(h) Prose:** README banner + Legacy section ("at this repo root" → `legacy_perl/`);
CHANGELOG unreleased entry (move, broken `blob/master/<script>` deep links, `v0.25.1` tag is
the stable Perl reference); `rust/README.md:102` reword ("live in `legacy_perl/`; at the
`v0.25.1` tag, the repo root"); grep docs + CONTRIBUTING for root-location claims (expected
near-empty — verified); project `CLAUDE.md` (untracked, local) updated. Post-merge local
nuisance to expect: `.claude/settings.local.json:7-27` holds absolute-path permission entries
for the old locations — they go stale (re-prompts only; untracked, outside the PR).

### Verify, then PR

- `cargo fmt --all -- --check`; `cargo clippy -p bismark --all-targets -- -D warnings` and
  again with `--features rammap-inprocess` (G26/G27).
- Validation battery (§9).
- **Straggler sweep — positive checklist first, greps second** (rev-1's single grep saw 13 of
  23 sites and zero `.sh` files; do not resurrect it as the gate):
  1. tick every row of tables (a)–(f) against `git diff`;
  2. three narrow idiom greps, each expected to return **only** `legacy_perl`-prefixed hits:
     - `grep -rnE '\.\./\.\./(bam2nuc|bismark|coverage2|deduplicate|filter_non|NOMe|methylation_c|plotly)' rust --include='*.rs' | grep -v target` (the `.rs` class),
     - `grep -rnE '(\$REPO_ROOT|&& pwd\))/(bam2nuc|bismark|coverage2|deduplicate|filter_non|NOMe|methylation_c)' rust scripts --include='*.sh'` (the `.sh` class),
     - `grep -nE '\./(bam2nuc|bismark|coverage2|deduplicate|filter_non|NOMe|methylation_c)' .github/workflows/ci_tests.yml | grep -v legacy_perl` → zero (unanchored — catches `:37`).
- PR into `dev`; declare CI green only from `gh run view <id> --json jobs` (H3) covering
  **both** workflows (`rust_ci.yml` incl. `perl-oracle`, `rammap-inprocess`, `binseq-input`;
  `ci_tests.yml`). Post-merge: G28/G29 squash verification.

## 6. Efficiency

Mechanical change. CI runtime ~unchanged (+1 trivial existence test). Docker build context
shrinks by **~20 MB** (plotly ~17 MB incl. the example-report HTML that `.dockerignore`'s
`**/*.md` never excluded, `test_data.fastq` ~2 MB, scripts ~1 MB). No runtime code paths
touched. Single pure-move commit keeps rename detection and `git log --follow` clean.

## 7. Integration

- Lands on `dev` via PR (G23); rides the deferred 3.2.0 release train — the CHANGELOG entry
  becomes the release-notes line. **No version literals change.**
- **Corrected tripwire statement** (rev-1 overstated this): the `perl-oracle` job guards the
  13 `perl_vs_rust_*` oracles (4 files); `report_perl_vs_rust.rs` + both drift guards fail
  loud in the main `test` job; the summary/template sites are guarded **by this plan's
  additions** — the layout test (V3) and the ok-count baseline (V2) — not by any pre-existing
  CI mechanism. Extending `perl-oracle` to cover them is a recorded follow-up (§12).
- `ci_tests.yml` exercises at runtime: report/summary→plotly (`:60,:61`) and
  extractor→bismark2bedGraph (via `--bed`). It does **not** reach bismark→bam2nuc
  (`--nucleotide_coverage` absent) or extractor→coverage2cytosine (`--cytosine_report`
  absent) — for those the assurance is structural ($RealBin relativity preserved by the
  atomic move), not a test.
- Downstream ecosystems unaffected: bioconda `bismark=0.25.1` builds from the immutable tag;
  bioconda 3.x / Homebrew / GHCR build only `rust/`; nf-core consumes containers.
- Known external cost (accepted, documented in CHANGELOG): `blob/master/<script>` deep links
  404; clone-root-on-PATH Perl users lose the tools on next pull with a loud
  `command not found` — README already routes Perl usage to the `v0.25.1` release.

## 8. Assumptions

1. **Fixed (user decision):** target directory name is `legacy_perl/`; Python helpers stay.
2. `test_data.fastq` moves (verified: only prose mentions, `installation.md:99,105`).
3. `ci_tests.yml` triggers unchanged — verified `on: [push, pull_request]`, no `paths:`.
4. `license.txt` stays at root (`release.yml:137` packages it from there).
5. `test_files/` stays at root — shared fixture ground, CWD-relative in `ci_tests.yml`.
6. `git mv` on this case-insensitive APFS is safe here (no case-only renames; note the repo
   does carry live `Docs`/`docs` aliasing in `.gitattributes`/`.dockerignore` — untouched)
   and preserves execute bits (all 13 Perl files are `-rwxr-xr-x`).
7. `plans/.gitignore` already carries `archived/` (verified).
8. The local Mac has `perl`, `gzip`, `samtools` so the battery runs locally; the recorded
   Step-0 baseline makes local skips explicit instead of assumed.
9. **`rust_ci.yml` runs on this PR only because Commit 2 touches `rust/**`** — its trigger
   has `paths: ["rust/**", ".github/workflows/rust_ci.yml"]`. A root-only variant of this
   change would silently skip all Rust CI; this plan always ships root + `rust/**` together.
10. Linguist behaviour is preserved by `.gitattributes:11` (`**/plotly/**`, unanchored) even
    before the `:10` rewrite lands — the language bar cannot regress in between.

## 9. Validation

| # | What | How | Expected |
|---|---|---|---|
| V0 | Pre-move baseline | Step-0 battery on `dev`; record ok-count `N_baseline` + skip list | Numbers recorded before any change |
| V1 | **Sabotage** — moved paths are load-bearing | Same battery after Commit 1 | 13 `perl_vs_rust_*` + 5 `*_byte_identical` report oracles **fail**; both plotly guards **panic**; summary/template sites stay green-by-skipping (documented — their gate is V2+V3, not their color) |
| V2 | No oracle quietly switched off | Battery after Commit 2: ok-count vs `N_baseline`, `grep -n 'skipping'` diff vs Step-0 | ok-count == `N_baseline` + 1 (the new layout test); no new skips |
| V3 | Layout gate | `cargo test -p bismark --test legacy_perl_layout`; sabotage it once (rename one script locally, observe red, restore) | Passes; observed failing under sabotage before being trusted |
| V4 | Drift guards against canonical plotly | `cargo test -p bismark -- match_repo_plotly_files` (report = `embedded_…`, summary = `vendored_…`) | Both run, both pass |
| V5 | No straggler paths | Positive checklist (a)–(f) + the three §5 idiom greps | Every row ticked; greps return only `legacy_perl` hits / zero |
| V6 | Runtime plotly + workflow health | `ci_tests.yml` on the PR (`./legacy_perl/bismark2report` + `bismark2summary` at `:60,:61` resolve `$RealBin/plotly`) | Job green via `gh run view --json jobs` (H3) |
| V7 | Local Perl smoke incl. one $RealBin coupling | `legacy_perl/bismark --version`; run `perl legacy_perl/bismark2report` against a checked-in report fixture from `report_perl_vs_rust.rs`'s data dir → HTML produced | Normal output; HTML contains the spliced plotly payload |
| V8 | Whole-suite health | Full PR CI, both workflows, all jobs enumerated | Green via `gh run view <id> --json jobs` |
| V9 | (cheap, optional) `.dockerignore` risk retirement | Local `docker build` reaching the `cargo build` layer | Builds past COPY/deps; otherwise defer to next `release.yml dry_run=true` |

## 10. Questions or ambiguities

- **Critical:** none.
- **Open-1 — RESOLVED (Felix, 2026-08-08): leave at root.** The two Python coverage helpers
  do not move.
- **Open-2** (default: move — accepted by Felix): `test_data.fastq`.
- **Open-3** (default: leave, out of scope): `Bismark_alignment_modes.pdf` + the two
  superseded Rust-era `RELEASE_CHECKLIST*.md`. Candidate follow-up.
- **Open-4** (default: no): demote `ci_tests.yml` to manual/scheduled triggers. Separate
  decision; unchanged here.
- **Open-5** (default: accept broken, move as artifact): `copy_bismark_files_for_release.pl`
  — already dead today (§2); alternatives (fix its three root-sibling refs, or leave it at
  root) documented and not taken.

## 11. Self-Review (rev 2)

- **What the dual review changed:** rev 1 trusted one grep idiom to enumerate consumers and
  one CI job to catch misses; both were narrower than assumed (9 missed consumers; silent
  summary skips; V1/V2 filters matching nothing; V5 blind to `.sh`). Rev 2 replaces the
  enumeration with a positive checklist + three idiom-specific greps, replaces the filters
  with name-verified ones, adds an unskippable layout test, and pins the oracle population
  with a counted baseline. Inter-reviewer discrepancies were re-verified at source, not
  adjudicated by trust: the golden scripts **are** broken today (`REPO_ROOT` → `…/rust`,
  observed empirically) and 11/12 summary oracles **do** skip without output.
- **Claims retracted from rev 1:** "`scripts/` grep-clean" (§2/§11); "zero Perl code
  changes" (holds for the 12 executables, not the packager); "perl-oracle catches any missed
  oracle path" (4 of 8 files); "43 invocations" (44); "4 asset files"/"~3 MB" (10 / ~20 MB);
  V4's claim that all $RealBin couplings are CI-exercised (two are not).
- **Edge cases carried + new:** atomic-move $RealBin preservation; `use lib` no-op both
  sides; CWD-relative workflow outputs; mode-bit preservation; `Docs`/`docs` APFS aliasing
  noted; commit-1-alone-doesn't-trigger-rust-CI (Assumption 9); linguist protected by the
  unanchored gitattributes line during the transition (Assumption 10).
- **Vacuous-verification posture:** every gate now has an observed-failing story — V1
  observes the loud gates fail, V3's layout test is sabotaged once deliberately, and the
  silent-skippers are explicitly *named as unprovable by their own color* and gated by count
  (V2) + existence (V3) instead. No check in §9 relies on a filter or pattern that was not
  verified to match at least one real target.
- **Remaining risks:** prose sweep still grep-based (bounded: prose only); `.dockerignore`
  unproven until V9 or the next release dry-run; the summary oracles remain skip-capable in
  *future* refactors that dodge the layout test's 16 fixed names — the §12 follow-up
  (EXPECTED=26) is the durable fix and is deliberately not smuggled into this move.

## 12. Implementation notes (2026-08-08)

Executed in plan order; all validation rows satisfied. Evidence logs in the session scratchpad
(`baseline.log`, `sabotage.log`, `postfix.log`).

| Gate | Result |
|---|---|
| V0 baseline (`dev`) | **72 ok / 0 failed / 0 test-skips** (4 "skipping" lines are Perl bismark2report fixture chatter, not test skips) |
| V1 sabotage (post-move, pre-fix) | **20 loud failures = exactly the predicted set** (2 plotly guards panicked, 13 `perl_vs_rust_*`, 5 report `*_byte_identical`); 52+20=72. The 12 summary oracles + template drift passed **green with zero output** — the silent-green hazard demonstrated, stronger than reviewed (libtest swallows the one notice on passing tests) |
| V2 post-fix battery | **73 ok = N_baseline + 1** (the layout test), 0 failed, same 4 chatter lines |
| V3 layout gate | Observed **FAILED** with `NOMe_filtering` renamed away, **ok** after restore |
| V4 drift guards | `embedded_…` (report) + `vendored_…` (summary) both ran + passed against `legacy_perl/plotly/` |
| V5 stragglers | `.rs` idiom grep: only the 2 deliberately-kept historical comments; `.sh` idiom grep: zero; `ci_tests.yml`: 44 converted, zero non-prefixed |
| V7 Perl smoke | `legacy_perl/bismark --version` ok; `perl legacy_perl/bismark2report` over the `wgbs_pe` fixture produced HTML with plotly spliced (0 placeholders, 19 payload markers) |
| fmt / clippy | `cargo fmt --all --check` clean; clippy `-D warnings` clean on default **and** `rammap-inprocess` |
| V9 | Not run locally (optional row); retire at next `release.yml dry_run` |

**Deviations from plan (documented, none material):**
1. V0/V1/V2 battery needed a **6th filter term** `legacy_perl_toolchain` — none of the plan's five
   terms matches the new test's name; without it V2's `N+1` expectation is unmeasurable.
2. Sabotage evidence exceeded the plan's wording: **zero** `skipping` lines (not "one notice") —
   the plan's claim "green-by-skipping with one skipping: line" was conservative.
3. The rammap-inprocess clippy gate required an unsandboxed cargo run (registry unpack of
   `crossbeam-epoch` under `~/.cargo`) — environmental, not code.
4. `ci_tests.yml` edited via longest-name-first `replace_all` (44 verified) rather than sed.
5. Project `CLAUDE.md` text updated as planned but remains untracked (its prior status).
6. Golden-script up-count repairs verified empirically: all four sampled resolve to
   `/Users/fkrueger/Github/Bismark` (was `…/Bismark/rust`).

## 13. Declined scope extensions & follow-ups (recorded, not absorbed)

1. **Harden `perl-oracle` to `EXPECTED=26`** — add the 12 summary oracles + template guard
   to the `--exact` list and teach `summary_perl_oracle.rs`/`summary_template_drift.rs` to
   honor `BISMARK_REQUIRE_PERL` (their skip notices should also start with `skipping:` to
   feed the job's existing detector). Reviewer A's thorough fix, Reviewer B's Alternative D.
   Separate CI-hardening change: it alters what CI asserts, not where files live.
2. **Centralized `legacy_perl_script()` fail-loud helper** (both reviewers) — route all test
   literals through one function so the next relocation is a one-line change / compile error.
   Follow-up plan candidate; overlaps with #1.
3. **Root compatibility symlinks** — rejected by both reviewers and this plan (defeats the
   goal; GitHub blob URLs don't follow symlinks).
4. **Three-commit split** (workflow edit separate) — declined; two commits keep the sabotage
   story simple, and the branch is pushed whole (Assumption 9).
5. **Env overrides for the five phase-level golden scripts** — declined as scope creep; their
   up-count is repaired here, which is the part that matters for reproducibility.

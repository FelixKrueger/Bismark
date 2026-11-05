# Report the suite version, not `v0.25.1` (#1148)

- **Rev 2 (2026-11-05):** folds in the dual plan review (`PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md`). The changes are listed under "Revision notes" at the end.
- **Rev 1 (2026-11-05):** Q1–Q3 resolved after Felix's review; Q2 checked against the nf-core snapshots.

## Goal

Every version string the Rust suite writes into an output names the suite version from `rust/VERSION` (now `3.1.0`). This covers report headers, BAM `@PG`, extractor file headers, the summary HTML and the genome-preparation banner. Today these say `v0.25.1`, so a 3.x run reports itself as Perl Bismark 0.25.1, in Bismark's own reports and in MultiQC.

The Perl byte-identity contract stays intact for everything except this one token. The oracle and golden comparisons treat the version token as the single documented difference, as they already treat the `localtime` line in the report and summary gates.

Issue: [#1148](https://github.com/FelixKrueger/Bismark/issues/1148), reported by bounlu. Felix chose option 1 (2026-11-05): the suite version everywhere.

Worktree: `/Users/fkrueger/Github/Bismark-1148`, branch `fix/1148-version-strings`, from `dev` at `94faf8d`.

## Context

**Where `v0.25.1` reaches an output.** Each source is a `pub const`, documented as locked to Perl:

| Output | Text now | Source |
|---|---|---|
| Aligner report, line 1 (bisulfite and 5-Base) | `Bismark report for: … (version: v0.25.1)` | `aligner::BISMARK_VERSION` (`aligner/mod.rs`), used in `aligner/report.rs` (the 5-Base header is at `report.rs:54`) |
| BAM `@PG` (aligner, 5-Base consensus) | `@PG ID:Bismark VN:v0.25.1` | `aligner::BISMARK_VERSION`, in `aligner/output.rs` (`generate_sam_header`) |
| `--five_base_bisulfite_bam` `@PG` | `@PG ID:bismark-five-base-bisulfite … VN:v0.25.1` | `aligner::BISMARK_VERSION`, in `aligner/mod.rs:796-800` |
| `--five_base_bisulfite_bam` run summary | the version line | `aligner::BISMARK_VERSION`, in `aligner/mod.rs:986-997` |
| Extractor split files, line 1 | `Bismark methylation extractor version v0.25.1` | `extractor::output::SPLIT_FILE_HEADER` |
| Extractor splitting report | `Bismark Extractor Version: v0.25.1` | `extractor::output::BISMARK_VERSION` |
| `bismark summary` HTML | `(version 0.25.1)` | `summary::BISMARK_VERSION` (no `v`), in `summary/html.rs` |
| Genome-preparation banner (stdout/stderr) | `Bisulfite Genome Indexer version v0.25.1 (last modified: 19 May 2022)` | `genome_prep::BISMARK_VERSION`, in `genome_prep/pipeline.rs` |

**Not emitters.** Both reviewers confirmed this list, and a `git grep '0\.25\.1'` over `rust/bismark/src` on 2026-11-05 shows that every other hit is a doc comment or a test fixture.
- `report::BISMARK_VERSION` is defined but has no caller.
- The `bismark report` HTML prints whatever version it parses from the input alignment report (`report/reports/alignment.rs`), so it follows the aligner change by itself.
- Both HTML templates take the version only through the `{{bismark_version}}` placeholder.

**Already correct:** `--version` everywhere uses `meta::version_line` and `meta::SUITE_VERSION`. `SUITE_VERSION` is `env!("BISMARK_SUITE_VERSION")`, which `build.rs` `suite_version()` sets from the first of these that exists:
1. `$BISMARK_SUITE_VERSION`;
2. `rust/VERSION`;
3. the vendored crate `VERSION`;
4. the literal `"unknown"`.

The `Dockerfile` defaults `ARG BISMARK_SUITE_VERSION=unknown` (lines 17 and 91). The release workflows pass the real version (`release.yml:113,263`, `ga-candidate-image.yml:42`). A plain local `docker build` therefore sets the variable to `unknown`. Today that only affects `--version`; after this change it would also reach BAMs and reports. See Behavior 6.

**Readers of these lines:**
- The bedGraph reader (`bedgraph/input.rs`) and the NOMe reader drop the extractor header by its `Bismark` prefix or its position, not by its version.
- `bismark report` parses `(version: …)` with any value.
- MultiQC's Bismark module (`multiqc/modules/bismark/bismark.py`) sets the software version with two regexes:
  - aligner report: `\(version: v([\d\.]+)\)`;
  - extractor report: `Bismark Extractor Version: v([\d\.]+)`.

  Release versions such as `v3.1.0` match both. Prerelease versions differ; see Behavior 4.

**Tests and gates that see the token:**
- **Live-Perl oracles that compare a version-bearing file:**
  - Among the 13 `perl-oracle` CI tests (`rust_ci.yml:205-224`), only one: `perl_vs_rust_nondirectional_pe_extractor` (`tests/extractor_nondir_swapped_flags_1030.rs:337-346`), which compares the splitting report and the split files.
  - `tests/summary_perl_oracle.rs` (`:77-97`) compares the HTML. It is not one of the 13. It runs in the ordinary test job, skips itself (printing `skipping:`) when Perl is missing, and ignores `BISMARK_REQUIRE_PERL`.
  - The report oracle (`report_perl_vs_rust.rs`) builds HTML from fixture reports, so it is unchanged.
- **Perl-generated golden files containing `v0.25.1`:** several extractor, dedup and summary fixtures. The goldens stay as Perl wrote them.
- **Smoke and unit tests that assert the literal `v0.25.1` on Rust output:**
  - `tests/extractor_*`;
  - `aligner/report.rs` and `aligner/output.rs` unit tests.
- **Tests that feed a Perl-style `v0.25.1` header as input:** these test parsing Perl output and stay as they are. They are in `report/reports/alignment.rs`, `summary/parse.rs` and `nome_filtering/nome.rs`. The input-header fixture at `extractor/logging.rs:293` belongs here too.
- **Gates outside the normal test run:**
  - `tests/aligner_byte_identity_real_data.rs` is `#[ignore]`d. It compares the report with a golden from `just aligner-golden`, and its `normalize_report` (`:135-146`) drops only lines containing `/Users/`, `/tmp/`, `/var/folders/` or `/private/`. On oxy (`/home/...`), report line 1 survives and is compared.
  - `scripts/phase_h_smoke.sh:289-336`, which `phase_h_{se,pe}_matrix.sh` drive, compares the Perl and Rust splitting reports with a strict `cmp` and the split files after `zcat`.
  - Not affected: `scripts/byteid_run.sh` compares Rust with Rust across `--parallel` and skips the reports. The `c2c_byte_identity_matrix.sh` outputs have no version.
- **BAM comparisons:** the aligner real-data gate already drops the whole `@PG` block.

## Behavior

1. **One version source.** Add `meta::VERSION_TAG` = `concat!("v", env!("BISMARK_SUITE_VERSION"))`, a `&str` const. The module constants keep their names, to limit churn:
   - `aligner::BISMARK_VERSION`, `extractor::output::BISMARK_VERSION` and `genome_prep::BISMARK_VERSION` equal `meta::VERSION_TAG`;
   - `extractor::output::SPLIT_FILE_HEADER` = `concat!("Bismark methylation extractor version v", env!("BISMARK_SUITE_VERSION"), "\n")`. It repeats the `env!`, because `concat!` takes only literals, not a `const`;
   - `summary::BISMARK_VERSION` = `meta::SUITE_VERSION`, with no `v`, because the template already prints `version {{bismark_version}}`;
   - the unused `report::BISMARK_VERSION` is removed.
2. **The outputs.** At suite version 3.1.0:
   - aligner report (bisulfite and 5-Base): `Bismark report for: … (version: v3.1.0)`;
   - BAM: `@PG ID:Bismark VN:v3.1.0`, and `VN:v3.1.0` on the `bismark-five-base-bisulfite` `@PG`;
   - the `--five_base_bisulfite_bam` run summary: `v3.1.0`;
   - extractor: `Bismark methylation extractor version v3.1.0` and `Bismark Extractor Version: v3.1.0`;
   - summary HTML: `(version 3.1.0)`;
   - banner: `Bisulfite Genome Indexer version v3.1.0`.
3. **Genome-preparation banner.** Drop the clause ` (last modified: 19 May 2022)`, because it describes the Perl script and would be wrong next to a 3.x version (Q1, resolved). The banner is stdout/stderr only; the genome-prep oracle compares FASTA bytes.
4. **Prerelease suites.** `3.2.0-beta.1` gives `v3.2.0-beta.1`. For MultiQC this has two effects:
   - The extractor regex captures `3.2.0`.
   - The aligner-report regex does not match at all, because `\)` must follow the digits, so MultiQC shows no aligner version for a beta. Today it shows the wrong one.

   This is accepted, and it does not affect releases. A MultiQC regex change upstream would fix it, but that is out of scope.
5. **Unchanged:**
   - Every other byte of every output.
   - `--version`.
   - Parsing Perl-written reports. The report and summary tools still accept `v0.25.1` headers from Perl runs.
6. **No `unknown` in outputs.** `build.rs` `suite_version()` treats `$BISMARK_SUITE_VERSION=unknown` as unset and falls through to `rust/VERSION` or the vendored `VERSION`. This fixes the plain-`docker build` case, because the Docker build has the crate's `VERSION` files. A guard test asserts `meta::SUITE_VERSION != "unknown"`, so a build that reaches the last fallback fails the test suite. The `Dockerfile` default stays, because the build now ignores it (Q4, default).

## Signature

```rust
// rust/bismark/src/meta/mod.rs
/// The version tag written into report headers, BAM `@PG` and file headers.
pub const VERSION_TAG: &str = concat!("v", env!("BISMARK_SUITE_VERSION"));
```

## Implementation outline

1. **Tests first.**
   - **Helper.** Add `rust/bismark/tests/common/version.rs`, a standalone file that each user includes with `#[path = "common/version.rs"] mod version;`. It stays out of `tests/common/mod.rs`, which holds the summary fixture builders. It provides:
     - `perl_version_for(s: &str, suite: &str, expected: &[(Form, usize)]) -> String`. It first asserts that each listed form occurs in `s` with `suite` exactly the expected number of times. It then rewrites only those forms to `0.25.1`. A missed emitter still writing `v0.25.1` therefore fails the count instead of passing silently. The forms:
       - `(version: v<S>)`;
       - `VN:v<S>`;
       - `extractor version v<S>`;
       - `Extractor Version: v<S>`;
       - `(version <S>)`.
     - `perl_version(s, expected)`, which calls `perl_version_for` with `bismark::meta::SUITE_VERSION`.
   - **Helper self-test** with an explicit `suite` argument:
     - each form is rewritten;
     - a lookalike, such as `v3.1.0` outside these forms, is left alone;
     - a wrong count panics;
     - with `suite = "0.25.1"` the output equals the input.
   - **`tests/version_strings_1148.rs`** asserts every Behavior 2 output against `meta::SUITE_VERSION` on the existing small fixtures:
     - the SE and PE bisulfite report line 1;
     - `@PG ID:Bismark VN`;
     - the 5-Base consensus `@PG`;
     - the split-file line 1 and the splitting report line;
     - the summary HTML;
     - the banner.

     It also asserts that `0.25.1` appears in none of these outputs.
   - **5-Base `--five_base_bisulfite_bam`:** extend the existing fixture run in `tests/aligner_five_base_bisulfite.rs` to assert `VN:v<S>` on the `bismark-five-base-bisulfite` `@PG` and the version in the run summary. Also add a unit-level assertion on the 5-Base report header (`aligner/report.rs:54`).
   - **Unit tests on the constants:** only on the two built differently, `SPLIT_FILE_HEADER` (equals `format!("Bismark methylation extractor version {}\n", meta::VERSION_TAG)`) and `summary::BISMARK_VERSION` (equals `meta::SUITE_VERSION`). Tests on the constants that are equal to `VERSION_TAG` would be circular, so none are added.
2. **Constants and build.**
   - Implement Behavior 1 and the banner change (Behavior 3). Rewrite each constant's doc comment to the current fact, in one line.
   - Implement Behavior 6 in `build.rs` `suite_version()`.
3. **Update the tests and gates that see the token.**
   - Literal-`v0.25.1` assertions on Rust output now use the constant, or `perl_version` where they compare with a Perl golden.
   - `perl_vs_rust_nondirectional_pe_extractor` and `summary_perl_oracle.rs` apply `perl_version` to the Rust side, with exact counts, before comparing:
     - splitting report: 1 `Extractor Version:`;
     - each split file: 1 `extractor version` (line 1);
     - summary HTML: 1 `(version …)`, which the implementer confirms against the template.

     The goldens stay unchanged.
   - Golden comparisons (extractor, dedup, summary fixtures) follow the same rule wherever the compared file is a Rust output that carries the version.
   - **Tests that feed a Perl-style header as input stay as they are,** including `extractor/logging.rs:293`.
   - **`tests/aligner_byte_identity_real_data.rs`:** `normalize_report` also maps report line 1's `(version: v<S>)` to `(version: v0.25.1)`, using the same form rule, so the gate passes on oxy against the existing goldens. The goldens are not regenerated.
   - **`scripts/phase_h_smoke.sh`:** before the strict `cmp` of `*_splitting_report.txt` and the `zcat` comparison of the split files, normalise the Rust side's version token with a `sed` that matches only the two extractor forms. The script states this exception next to the existing ones. Every other byte stays strict.
   - **Finding every site.** Run all of the following, then fix each failure by the rules above:
     - the full suite on Linux (oxy);
     - `cargo test -- --ignored` for the real-data gate on oxy;
     - the 13 CI oracles locally with `BISMARK_REQUIRE_PERL=1`;
     - `summary_perl_oracle.rs` locally with Perl present, keeping the output that shows it ran (no `skipping:` line).

     Never loosen a comparison beyond the version token.
4. **Guard.** Add `tests/version_guard_1148.rs`. It walks `rust/bismark/src/**/*.rs` with `std::fs`; `include_str!` cannot glob.
   - **Rust files:** in each, it keeps only the text before the first `#[cfg(test)]` (in all eight affected files the test module is last) and drops lines whose trimmed form starts with `//`. It then fails on any substring `0.25.1` that is left. A substring match catches a version embedded in a longer literal, like today's `SPLIT_FILE_HEADER`.
   - **Templates:** it also scans `rust/bismark/src/**/*.html` and `rust/bismark/assets/*.tpl` for `0.25.1`.
   - **Version:** it asserts `meta::SUITE_VERSION != "unknown"`.
5. **Docs.**
   - Add the version-token caveat to each "byte-identical to Perl v0.25.1" claim:
     - `docs/src/content/docs/rust/overview.md`;
     - `installation.md:3`;
     - `benchmarks.md:7`;
     - `choosing-an-alignment-mode.md:6,50,84`;
     - `illumina-5-base.md:36`;
     - `Home.astro:32`.

     Each caveat is a short clause: "apart from the version string". The Dockerfile `LABEL` (`:37`) gets the same clause.
   - CHANGELOG: one sentence, crediting bounlu and linking #1148.
   - `rust/README.md`: a Milestones line.
6. **Checks.**
   - fmt; release Clippy with `--all-targets`, which covers the new test files.
   - The full suite and the `--ignored` real-data gate on Linux.
   - The 13 oracles and the summary oracle locally with Perl.
   - The dual code review and the plan-manager audit.
   - The PR into `dev`. Its description says:
     - the public crate constants change value, and `report::BISMARK_VERSION` is removed;
     - the nf-core snapshot consequence from Q2.

     Then the reply on #1148 (Q3).
7. **After merge:**
   - Run `/tool-wiki update` for `wiki/reference/methylation-call-files.md`, which quotes the `v0.25.1` header. That can go on the parked `wiki/update-2026-11-04` branch.
   - At the release that ships #1148, open the nf-core snapshot follow-up together with the version bump, not later (Q2).

## Efficiency

Compile-time constants, with no runtime cost. The test helper does a few string replaces per compared file. The guard test reads about 200 source files once, in milliseconds.

## Integration

- **Changes:** the version text in every output listed in Context; `build.rs` ignores `BISMARK_SUITE_VERSION=unknown`.
- **Public API:** `bismark` is published on crates.io:
  - the `pub` constants keep their names and change value;
  - `report::BISMARK_VERSION` is removed.

  No known downstream user exists, but the PR says so.
- **Downstream:**
  - MultiQC reports the true version for releases. For prereleases it shows no aligner version (Behavior 4).
  - nf-core/methylseq and `nf-core/modules` snapshots: the extractor split files and splitting report are md5'd, so they change at every suite bump. BAMs, alignment reports and HTML are not affected. See Q2.
  - Pipelines that grep for `v0.25.1` would break; none is known.
- **Unchanged:** every non-version byte, `--version`, and parsing of Perl reports.

## Assumptions

- **No `unknown` in outputs.** A build with no `$BISMARK_SUITE_VERSION` and no `VERSION` file still compiles: `build.rs` falls back to `"unknown"`, and `env!` is always set. Behavior 6 and the guard keep `unknown` out of tested builds.
- **MultiQC capture.** For release versions, `[\d\.]+` captures the whole version. For prereleases, the extractor regex captures the numeric part and the aligner regex does not match (Behavior 4).
- **Readers.** No Rust reader keys on `v0.25.1`. Checked by both reviewers: bedGraph and NOMe skip by prefix or position, and the report parser takes any value.
- **Oracle scope.** Of the 13 `perl-oracle` CI tests, only `perl_vs_rust_nondirectional_pe_extractor` compares a version-bearing file. The summary oracle is a separate, skippable test.
- **Guard scope.** The guard skips each `#[cfg(test)]` item by brace matching, wherever it sits; some files (`aligner/stream.rs`, `aligner/binseq_decode.rs`, `aligner/config.rs`) have one mid-file. A string literal with unbalanced braces inside a test item could mis-skip, which in practice scans test code and fails loudly. (Corrected after code review: rev 2 cut at the first `#[cfg(test)]`, which silently skipped production code after it.)

## Validation

1. **The version reaches each output.** `version_strings_1148.rs` and the extended `aligner_five_base_bisulfite.rs` pass:
   - each output has `v3.1.0` (or `3.1.0` in the summary HTML);
   - none has `0.25.1`.
2. **Byte-identity holds apart from the token.**
   - The 13 oracles pass locally with `BISMARK_REQUIRE_PERL=1`, and the `perl-oracle` CI job is green.
   - `summary_perl_oracle.rs` passes with Perl present, with the output kept as proof that it did not skip.
   - On Linux, the full suite passes, the `--ignored` real-data gate passes, and one `phase_h_smoke.sh` cell passes.
3. **The helper is narrow and strict.**
   - Its self-test passes, including the wrong-count panic.
   - Checked once by hand and recorded in the notes:
     - an oracle comparison with an injected non-version difference still fails;
     - one emitter left at `v0.25.1` fails the count.
4. **MultiQC parses the new lines.** Run a Python one-off with MultiQC's two regexes, copied into the check, and record the result in the notes. Expected:
   - a real 3.1.0 aligner report and splitting report: both capture `3.1.0`;
   - a synthetic `v3.2.0-beta.1` line: the extractor regex captures `3.2.0`, the aligner regex does not match.

   MultiQC itself is not a test dependency.
5. **No hard-coded version remains.**
   - The guard test passes.
   - `git grep -n '0\.25\.1' rust/bismark/src` lists only doc comments, test code and parser fixtures.
6. **No `unknown`.** A build with `BISMARK_SUITE_VERSION=unknown` reports the `rust/VERSION` value in `--version` and in a report header. Checked once and recorded in the notes.

## Questions or ambiguities

- **Q1 (Resolved, 2026-11-05):** drop the banner's ` (last modified: 19 May 2022)` clause. Felix took the recommendation.
- **Q2 (Resolved, 2026-11-05): yes, it is relevant.** Checked against local checkouts (`~/Github/modules` 2026-07-28, `~/Github/methylseq` 2026-08-18; the snapshot style is unchanged since then):
  - **Affected: the extractor outputs.** The split files (`C{pG,HG,HH}_O{T,B}_*.txt.gz`, line 1) and `*_splitting_report.txt` are md5'd:
    - in `nf-core/modules` `bismark/methylationextractor`, through `snapshot(sanitizeOutput(process.out))`;
    - in every methylseq bismark pipeline test (`default`, `bismark_variants`, `bismark_hisat_variants`, `combined_index`, `targeted_sequencing_variants`), through `getAllFilesFromDir` with `tests/.nftignore`.
  - **Not affected:**
    - BAMs: both repos use `bam(...).getReadsMD5()`, which skips the header.
    - Alignment reports: `modules` asserts `readLines().contains(...)`, and methylseq ignores `*/alignments/logs/*.txt`.
    - The dedup report: it has no version.
    - The report and summary HTML: methylseq ignores `*/{reports,summary}/*`, and `modules` snapshots the file name only.
    - bedGraph, `.cov`, CpG_report, M-bias: they have no version.
    - MultiQC software versions: ignored.
  - **Consequence:** after #1148, every suite version bump, patch releases included, must regenerate those md5s with a real `nf-test --update-snapshot` run in both repos. Today's find/replace of the `versions` strings is no longer enough. The first bump to hit this is the release that ships #1148. The PR description says so, and the follow-up opens with that release (step 7). Dropping these files from the snapshots upstream is a possible follow-up, not part of this plan.
  - **Aside:** the real-data test in `modules` `bismark/methylationextractor` calls `snapshot(...).match()` without `assert`, unlike its siblings. It may be worth a one-line upstream fix when that module is next touched.
- **Q3 (Resolved, 2026-11-05):** reply on #1148 when the PR opens, as a 2–3 sentence comment.
- **Q4 (Resolved, 2026-11-05, Felix agreed to the default):** how to keep `unknown` out of outputs. The answer is Behavior 6: `build.rs` ignores `BISMARK_SUITE_VERSION=unknown`, and the guard test asserts the version is not `unknown`. The alternative is to drop the `Dockerfile` default so a plain `docker build` fails without `--build-arg`. That is stricter, but it breaks a local `docker build` that works today.

## Self-Review

- **Logic:**
  - One source (`meta`) feeds every constant, so the version cannot drift between tools.
  - The summary keeps its v-less form, because its template adds the word "version".
  - The report HTML needs no change, because it echoes its input.
  - The count check in the helper keeps the Perl comparisons able to catch a missed emitter.
- **Edge cases:**
  - Prerelease versions (Behavior 4).
  - `unknown` (Behavior 6).
  - Perl-written inputs stay parseable (Behavior 5).
  - A suite at 0.25.1 would make the helper a no-op, which its self-test covers with an explicit `suite` argument.
- **Integration risk:**
  - The main risk is a missed test site or gate. Step 3 names the gates outside the normal suite and runs every suite, oracle and `--ignored` gate, rather than relying on grep.
  - The second risk is downstream md5 snapshots (Q2).
- **Kept narrow:** no compatibility flag to restore `v0.25.1`. Nothing known needs it, and a flag would make the Perl gate configuration-dependent.

## Revision notes (rev 1 → rev 2)

| Review item | Source | Change |
|---|---|---|
| MultiQC aligner regex does not match prereleases | A1, B3 | Behavior 4, Assumptions and Validation 4 corrected |
| `unknown` can reach BAMs and reports | A2, B5 | Behavior 6, guard assertion, Q4; Assumption 1 corrected |
| `#[ignore]` real-data gate and `phase_h_smoke.sh` would break unnoticed | A4, B4 | Named in Context; handled in step 3; added to Validation 2 |
| `byteid_run.sh` named as affected | A4 | Checked: it compares Rust with Rust across `--parallel` and skips the reports, so it is not affected |
| Helper cannot catch a missed emitter; self-test needs a `suite` argument | A6, B1 | `perl_version_for` with exact counts; Validation 3 adds the missed-emitter check |
| Oracle list wrong; summary oracle skips silently | A7, B2 | Context and Assumptions corrected; proof-of-run in step 3 and Validation 2 |
| 5-Base emitters not covered by tests | A3, B (optional) | Added to step 1; the `@PG` ID corrected in the Context table |
| Guard rule ambiguous | A5, B (optional) | Step 4 spells out a substring match, the cut at `#[cfg(test)]`, comment skipping and templates |
| Helper location | A8, B (optional) | Standalone `tests/common/version.rs` via `#[path]` |
| More byte-identity doc claims | A9, B (optional) | All listed in step 5, plus the Dockerfile LABEL |
| Public constants change value | A10 | Integration and PR description |
| `PROGRESS.md` stale | A11 | Updated |
| Circular constant unit tests | A12, B (optional) | Kept only for `SPLIT_FILE_HEADER` and `summary::BISMARK_VERSION` |
| `extractor/logging.rs:293` misclassified | B (optional) | Moved to the input-fixture group |
| nf-core follow-up timing | B (optional) | Step 7: opens with the release that ships #1148 |

## Implementation notes (2026-11-05)

Implemented on `fix/1148-version-strings` (uncommitted), following rev 2 in order. 29 modified files and 3 new test files.

### What was built

- `meta::VERSION_TAG`. Every module constant now derives from `meta` (Behavior 1), and the unused `report::BISMARK_VERSION` is removed.
- `build.rs` treats `BISMARK_SUITE_VERSION=unknown` as unset (Behavior 6).
- The genome-prep banner no longer has its date clause (Behavior 3).
- `tests/common/version.rs`: `perl_version_for` / `perl_version` with exact-count checks per form, included via `#[path]`.
- `tests/version_strings_1148.rs`:
  - helper self-tests: rewrite, lookalike, wrong-count panic, and the no-op at `0.25.1`;
  - `SUITE_VERSION != "unknown"`;
  - end-to-end checks of the SE aligner report line 1 and the BAM `@PG`, the extractor split files and splitting report, the summary HTML, and the banner, each also asserting the output has no `0.25.1`.
- `tests/version_guard_1148.rs`: walks `src/**/*.rs` and fails on any `0.25.1` in non-comment code before the first `#[cfg(test)]`. It also scans the `.html`/`.tpl` templates and includes a self-test on an embedded literal.
- `tests/aligner_five_base_bisulfite.rs`: asserts the converter `@PG` has `VN:v<S>` and the re-encode report line 1 has the version.
- Updated sites:
  - literal-version unit and smoke tests: `aligner/report.rs`, `aligner/output.rs`, 5 extractor test files;
  - the two Perl oracles (nondir PE extractor, summary) via `perl_version` with exact counts;
  - `aligner_byte_identity_real_data.rs`;
  - `scripts/phase_h_smoke.sh`.
- Docs caveats, the Dockerfile LABEL, CHANGELOG (one sentence), and a `rust/README.md` Milestones line.

### Deviations from rev 2

1. **`aligner_byte_identity_real_data.rs` masks the version on both sides** instead of mapping the Rust side to `v0.25.1`. Its golden is a *Rust* capture (`just aligner-golden`), so it carries whatever version it was built at. The test now:
   - masks `(version: v…)` on report line 1, on both sides;
   - separately asserts that the current report line 1 has the suite version.
2. **PE report line 1 and the 5-Base consensus `@PG` are covered at unit level, not end-to-end:**
   - PE line 1: the existing `aligner/report.rs` PE header tests now format with `meta::SUITE_VERSION`;
   - 5-Base consensus `@PG`: it is built by the same `generate_sam_header` as the bisulfite `@PG`, which `header_hd_sq_pg_exact_bytes` pins.

   The end-to-end aligner check is SE only, using the fake-`bowtie2` pattern from `aligner_cli.rs`, with `--no_stream_converted` so it does not hit the macOS FIFO problem.
3. **Two docs items left as they are:**
   - the `Home.astro` hero badge "Rust · byte-identical to Perl v0.25.1": the caveat went into the paragraph on line 44 instead, because a badge cannot carry a clause;
   - `illumina-5-base.md:36`: it is about methylation calls, not output bytes.
4. **No Perl-golden test needed changing.** The full suite showed that every golden comparison is either of a file without a version line or is already covered by the updates above.
5. **Unit tests on the constants** are the existing `extractor_se_phase_b.rs::output_file_header_matches_perl_format`, which now asserts `SPLIT_FILE_HEADER` and `BISMARK_VERSION` against `format!` of `SUITE_VERSION`. `summary::BISMARK_VERSION` is covered end-to-end by `summary_html_carries_the_suite_version`, so it has no separate unit test.
6. **The `SUITE_VERSION != "unknown"` assertion lives in `version_strings_1148.rs`** (`version_is_never_unknown`), not in the guard file. It sits with the other output assertions; the effect is the same.
7. **`overview.md:3` and `:7` keep their wording.** They state the page's aim; the same page states the version exception at line 34.

### Validation results

| # | Check | Result |
|---|---|---|
| 1 | `version_strings_1148.rs`, extended `aligner_five_base_bisulfite.rs` | pass (9 + 11) |
| 2 | 13 CI oracles, `BISMARK_REQUIRE_PERL=1`, macOS | 13/13 ok, no `skipping:` |
| 2 | `summary_perl_oracle.rs` with Perl present | 12/12 ok; ran with no `skipping:` line (the "No deduplication report present, skipping..." lines are Perl's own output) |
| 2 | Full suite, Linux (oxy, `/var/tmp/Bismark-1148` = `94faf8d` + this change) | 2340 pass, 1 fail: the guard read macOS AppleDouble `._*` files carried over by `tar`. Deleted, then the guard passed, so 2341/0 |
| 2 | Full suite, macOS | 2309 pass, 31 fail, all `aligner_cli` FIFO-streaming (the known environmental set; they pass on Linux) |
| 2 | `--ignored` real-data gate (oxy, 200k-pair RRBS subset of SRR24766921, GRCm39, golden captured by the unmodified `94faf8d` build) | PASS: 246,856 BAM records identical and report identical. Rerun with `/home` paths so line 1 survives the path filter (golden `v0.25.1` vs current `v3.1.0`): PASS, so the masking itself is exercised |
| 2 | `phase_h_smoke.sh` on that BAM, `--mode default` and `--mode gzip`, `--parallel 4` | PASS both: splitting report and M-bias raw-identical, 6 split files sorted-equivalent |
| 3 | Helper self-test incl. wrong-count panic | pass |
| 3 | Hand check: `extractor::output::BISMARK_VERSION` set back to `"v0.25.1"` | the oracle fails on the count (`expected 1 × "Extractor Version: v3.1.0\n" … found 0`), and the guard names the line |
| 3 | Hand check: one extra space in `Bismark result file: paired-end (SAM  format)` | the oracle fails on the splitting report diff |
| 4 | MultiQC regexes (Python one-off) | a real splitting report gives `3.1.0`; aligner `v3.1.0` gives `3.1.0`; aligner `v3.2.0-beta.1` does not match; extractor `v3.2.0-beta.1` gives `3.2.0` |
| 5 | Guard, plus `git grep '0\.25\.1' rust/bismark/src` | the guard passes; remaining hits are doc comments and `#[cfg(test)]` input fixtures and parser tests |
| 6 | `BISMARK_SUITE_VERSION=unknown cargo build`, then `bismark --version` | `v3.1.0`, from `rust/VERSION` |
| — | `cargo fmt --all --check`; Clippy `--workspace --all-targets`, `--features rammap-inprocess`, `--release` (all `-D warnings`) | clean |

### Iteration log

- #1: The hand-check restore used `cp`, which is aliased to `cp -i` here, so the restore silently did nothing and check 2 ran on top of check 1. Restored with `command cp -f`, confirmed with `cmp`, and redid check 2. The file is unchanged from before the checks.
- #2: On oxy, the guard failed with `stream did not contain valid UTF-8` on AppleDouble `._*.rs` files from the macOS `tar`. Deleted them (32 files) and the guard passed. Next time, ship with `COPYFILE_DISABLE=1`.
- #3: The first oxy real-data script exited silently at `zcat | head` (SIGPIPE under `pipefail` + `set -e`). Fixed with `{ zcat … || true; } | head` and an `ERR` trap.
- #4: The first real-data PASS ran with `/var/tmp` paths, which the gate's path filter drops along with line 1, so the masking was not exercised. Recaptured the golden and reran through `~/1148_rd`, and it passed.

### oxy artefacts (cleanup pending)

These are scratch artefacts for #1148:
- the detached worktrees `/var/tmp/Bismark-1148` and `/var/tmp/Bismark-1148-base`, both registered in `~/Github/Bismark`;
- `/var/tmp/1148_rd`;
- the `~/1148_rd` symlink;
- `/var/tmp/1148_smoke_{default,gzip}`;
- `/var/tmp/1148.tgz`;
- `/var/tmp/rd1148*.sh` and their logs.

### Review fixes (2026-11-05, after `CODE_REVIEW_A.md`, `CODE_REVIEW_B.md`, `COVERAGE.md`)

- **Guard:** it now skips each `#[cfg(test)]` item by brace matching instead of cutting the file at the first one (both reviewers, Medium). That cut left ~480 / ~340 / 87 production lines of `aligner/stream.rs`, `aligner/binseq_decode.rs` and `aligner/config.rs` unscanned. New self-test `guard_scans_past_an_early_cfg_test_item` covers: a `#[cfg(test)]` fn and mod inside an impl, a `#[cfg(test)]` `use`, and a same-line `#[cfg(test)] mod`. The guard still passes, with no hits.
- **`phase_h_smoke.sh`:**
  - the splitting-report mapping is limited to lines 1–10 (the split-file mapping was already line 1 only);
  - the failure message reports the mapped Rust size.
  - Re-run on oxy (`--mode gzip`): PASS.
- **Tidy-ups:**
  - dropped `Form::AlignerReport` and `Form::PgVersion`, which nothing used outside the helper's self-tests;
  - removed the stray blank line in `summary_perl_oracle.rs`'s imports;
  - fixed the `summary/html.rs:57` comment.
- **Docs caveats added:** `choosing-an-alignment-mode.md:84`, `installation.md:6`, `README.md:9`, `rust/README.md:24`.
- **Not changed:**
  - the smoke `sed` is not pinned to the exact binary version;
  - no assertion on the golden's line 1;
  - the CHANGELOG keeps its "Reported by" form, which is the file's existing pattern;
  - the Docker `bismark` wrapper's `vunknown` predates this change and goes to a separate issue.
- **Checks after the fixes:**
  - fmt clean;
  - `cargo clippy --workspace --all-targets -D warnings` clean;
  - with `BISMARK_REQUIRE_PERL=1`, all pass: `version_guard_1148` 4/4, `version_strings_1148` 9/9, `summary_perl_oracle` 12/12, `extractor_nondir_swapped_flags_1030` 4/4.

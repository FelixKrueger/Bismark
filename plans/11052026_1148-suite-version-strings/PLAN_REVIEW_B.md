# Plan review B: #1148 suite version strings (PLAN.md rev 1)

Reviewer B, 2026-11-05. Read-only review against worktree `/Users/fkrueger/Github/Bismark-1148` at `94faf8d`. No builds or tests were run.

Verdict: the plan holds up. The emitter list is complete, and the `meta`/`build.rs` plumbing supports the proposed constants. There are no critical findings. Five items should be fixed in the plan before implementation. They cluster around validation: which oracles actually see the token, a helper that cannot detect a missed emitter, a MultiQC claim that is wrong for prereleases, and off-CI gates that will start failing.

## 1. Logic review

**The emitter list is complete.** I grepped `rust/bismark/src` for every string literal containing `version`/`VN:` and every use of `BISMARK_VERSION`/`SPLIT_FILE_HEADER`. The only output emitters are the ones in the Context table:
- `aligner/report.rs:68,75` (report line 1);
- `aligner/output.rs:117` (`@PG VN`);
- `aligner/mod.rs:796` (the bisulfite-BAM `@PG`) and `aligner/mod.rs:997` (the re-encode summary);
- `extractor/output.rs:383` (`SPLIT_FILE_HEADER`) and `:925-926` (splitting report);
- `summary/html.rs:58`;
- `genome_prep/pipeline.rs:36`.

`report::BISMARK_VERSION` (`report/mod.rs:33`) has no caller (`git grep report::BISMARK_VERSION` is empty). Both HTML templates use a placeholder only (`summary/summary_template.html:875`, `assets/plotly_template.tpl:1150`). The report HTML takes its version from the parsed input (`report/reports/alignment.rs:147,190`). Dedup, filter_non_conversion, bam2nuc, c2c and methylation_consistency write no `@PG` and no version.

**Small factual error in the Context table.** The `--five_base_bisulfite_bam` `@PG` is not `ID:Bismark`. It is `ID:bismark-five-base-bisulfite` (`aligner/mod.rs:797-800`). The `VN:v<S>` helper form still covers it, but the table and the Behavior 2 list should say so, and `version_strings_1148.rs` should assert this `@PG` and the re-encode summary line (`aligner/mod.rs:986-997`), which are absent from the Implementation step 1 list.

**`concat!`/`env!` with a `pub const` is sound.**
- `concat!("v", env!("BISMARK_SUITE_VERSION"))` is valid, because `env!` expands to a literal. The crate already does this (`report/assets.rs:15`, `include_str!(concat!(env!(...)...))`).
- The plan correctly avoids `concat!("v", SUITE_VERSION)`, which would not compile. `concat!` takes literals, not const items.
- `pub const BISMARK_VERSION: &str = crate::meta::VERSION_TAG;` is fine. So is `SPLIT_FILE_HEADER = concat!("Bismark methylation extractor version v", env!("BISMARK_SUITE_VERSION"), "\n")`.
- `meta` is `pub` (`lib.rs:24`), so integration tests can reach `bismark::meta::SUITE_VERSION`.

**Misclassified unit test.** `extractor/logging.rs:293` feeds an *input* `@PG ... VN:v0.25.1` to `filter_header_text`, which echoes the input header. It is an input fixture like the parser tests, not an assertion on Rust output, so it stays unchanged. Step 3 lists it among the tests that must change.

**Readers.** The plan's reader claims are confirmed:
- bedGraph skips by prefix (`bedgraph/input.rs:176`, `line.starts_with("Bismark")`);
- the report parser takes any value (`report/reports/alignment.rs:133-150`);
- the summary parser does not use the version.

The NOMe reader I did not trace beyond its test fixture (`nome_filtering/nome.rs:481`), which uses a Perl-style header.

## 2. Assumptions

- **A1 is wrong as stated** ("`env!` fails the build otherwise"). `build.rs` `suite_version()` falls back to the literal `"unknown"` and always emits `cargo:rustc-env=BISMARK_SUITE_VERSION`. A build with no env var, no `../VERSION` and no vendored `VERSION` therefore succeeds and writes `vunknown` into every report and BAM. Today that is limited to `--version`; after #1148 it reaches output files. Both `rust/VERSION` and `rust/bismark/VERSION` exist (3.1.0), so the risk is low, but the assumption should be corrected. The plan should either accept `vunknown` or have the guard test fail when `SUITE_VERSION == "unknown"`.
- **A2 / Behavior 4 contradict the regex the plan quotes.** The aligner-report regex `\(version: v([\d\.]+)\)` has a trailing `\)`. With `v3.2.0-beta.1)`, `[\d\.]+` stops at `-` and `\)` then fails, so **the aligner report does not match at all**: MultiQC shows no version, rather than `3.2.0`. The extractor regex has no trailing anchor, so it does capture `3.2.0`. The impact is cosmetic: beta builds show no version in MultiQC's Bismark section. But Behavior 4 states something false, and Validation 4 only tests `3.1.0`.
- **A4 is wrong: the oracles are the wrong set.** The `perl-oracle` job (`rust_ci.yml:205-224`) runs genome-prep ×8, methylation_consistency ×3, dedup ×1 and extractor ×1. Of these, **only `perl_vs_rust_nondirectional_pe_extractor`** compares a version-bearing file (`tests/extractor_nondir_swapped_flags_1030.rs:329-347`).
  - `summary_perl_oracle.rs` and `report_perl_vs_rust.rs` are **not** in the 13. They auto-skip when Perl or the assets are absent, and neither reads `BISMARK_REQUIRE_PERL` (no hit for `REQUIRE` in `summary_perl_oracle.rs`; `report_perl_vs_rust.rs:15,113` uses `perl_available()`).
  - The Context bullet lists the summary oracle under "Live-Perl oracles". Step 3 and Validation 2 then rely on "the 13 oracles with `BISMARK_REQUIRE_PERL=1`". That would not force the one oracle that compares the summary HTML to run, and a skip passes green.
  - The report oracle is in fact unaffected, because its inputs are Perl fixtures, as the plan says.
- **Unstated assumption: no golden-vs-Rust gate outside `cargo test` compares a version-bearing file.** It is false; see V3.

## 3. Efficiency

There is nothing to flag. The constants are compile-time and the helper does a handful of `str::replace` calls. A guard that walks the source tree costs a few milliseconds.

## 4. Validation sufficiency

- **V1: the helper cannot catch a missed emitter.** `perl_version` rewrites the suite token to `v0.25.1` on the Rust side. If an emitter is missed and still writes `v0.25.1`, the helper does nothing and the oracle **passes**. Every Perl comparison therefore loses its power to detect the very bug class this plan fixes. The only remaining nets are `version_strings_1148.rs` (which covers only the outputs listed there) and the source guard. Fix: make the helper count its rewrites, or make the oracle call assert that the Rust side contains `meta::VERSION_TAG` (or `(version <S>)` for the summary) before normalising. For example, `perl_version(s, expected_min_hits)`, or return the count and assert it is at least 1. This also tightens the "narrow" property the plan wants. The self-test then becomes: each form is rewritten, a lookalike is untouched, the count is right, and the S = 0.25.1 no-op case is explicit.
- **V2: the summary oracle could pass vacuously.** See A4. The plan should require the summary-oracle run to show no `skipping:` line, or add `summary_perl_oracle` (one test) to the CI list. The latter means raising `EXPECTED`, a CI change outside scope. Suggest at least the manual check, recorded in the notes.
- **V3: off-CI gates will start failing, and the plan does not mention them.**
  - `scripts/phase_h_smoke.sh:289-336` runs a strict `cmp`, or `zcat | cmp`, of Perl vs Rust `*_splitting_report.txt` and the split files. It is the full-scale extractor gate (and is driven by `phase_h_{se,pe}_matrix.sh`). After #1148 it fails on line 1 of every file.
  - `tests/aligner_byte_identity_real_data.rs:135-146,224-228` (`#[ignore]`) compares the report with a golden captured by `just aligner-golden` (`rust/justfile:108-113`). `normalize_report` drops line 1 only if the path contains `/Users/`, `/tmp/`, `/var/folders/` or `/private/`. On oxy (`/home/...`), line 1 survives, so the old golden (with `v0.25.1`) mismatches.
  - "Full suite on Linux" does not run `#[ignore]` tests.

  The plan should state how each gate handles the token: apply the same narrow normalisation (a `sed` on the version token in `phase_h_smoke.sh`), recapture the golden, or note them as known and accepted.
- **V4: the source guard is workable, but needs a concrete rule.**
  - In all eight `src` files with test literals, `#[cfg(test)]` begins the trailing test module: `aligner/output.rs:970`, `aligner/report.rs:350`, `bedgraph/input.rs:205`, `extractor/logging.rs:252`, `io/read.rs:935`, `nome_filtering/nome.rs:342`, `report/reports/alignment.rs:269`, `summary/parse.rs:313`. There are no separate `tests.rs` module files in `src`.
  - So "truncate at the first `#[cfg(test)]`, skip lines whose trimmed start is `//`, then search for `0.25.1`" is enough. No doc comment holds a quoted `"…0.25.1"` literal today.
  - Use an `fs` walk from `CARGO_MANIFEST_DIR`, not `include_str!`, which needs every file listed by hand and so would not catch a new file.
  - Include `src/**/*.html` and `assets/*.tpl`, so that a hard-coded version in a template is caught too.
  - Also note that a first `#[cfg(test)]` placed above production code would hide that code. That is acceptable as a heuristic, but say so.
- **V5: the "byte-identity apart from the token" check (Validation 3) is manual and one-off.** That is fine, given V1's count assertion. Without V1, it does not prove anything durable.
- **V6: the unit test `aligner::BISMARK_VERSION == format!("v{}", SUITE_VERSION)` is tautological** once the constant is defined as `meta::VERSION_TAG`. It is harmless but adds little. The integration test is the real coverage.

## 5. Alternatives

- **The helper's home.** `tests/common/mod.rs` already exists. It holds summary fixture builders, has `#![allow(dead_code)]`, and is used by `summary_perl_oracle.rs:15` and `summary_txt_golden.rs:5`. Adding `tests/common/version.rs` means adding `pub mod version;` there, and every extractor test that uses it then compiles the summary builders too. That works, but a self-contained `tests/support_version.rs`, included via `#[path = "support_version.rs"] mod support_version;`, avoids coupling. Either is acceptable; the plan should name which.
- **Helper shape.** Instead of rewriting Rust to Perl, compare after replacing the token on *both* sides with a placeholder (for example `v<VERSION>`), asserting the expected count on each side. That is symmetric and states the "one documented difference" literally. It is equivalent in strength to V1's fix; either works.
- **Downstream churn (Q2).** The resolved consequence is that every suite bump re-md5s the methylseq/modules extractor snapshots. That is the cost of option 1, and it is already accepted. The upstream follow-up the plan mentions (ignoring the header line, or snapshotting the split files by content minus line 1) is the right lever. It is worth opening alongside the release that ships #1148, not later.

## 6. Action items

### Critical

None.

### Important

1. **Make `perl_version` detect a missed emitter (V1).** Assert the Rust side contains the suite token (count of at least 1, or the exact expected count) before rewriting. Otherwise every Perl comparison passes silently on an unconverted emitter. Evidence: the helper as specified in PLAN.md Implementation step 1; the oracle compares at `tests/extractor_nondir_swapped_flags_1030.rs:337-346` and `tests/summary_perl_oracle.rs:77-97`.
2. **Correct the oracle inventory and ensure the summary oracle actually runs (A4, V2).** Only `perl_vs_rust_nondirectional_pe_extractor` among the 13 (`.github/workflows/rust_ci.yml:205-224`) sees the token. `summary_perl_oracle.rs` is outside the 13 and auto-skips without honouring `BISMARK_REQUIRE_PERL` (`tests/summary_perl_oracle.rs:100-103`). Add "no `skipping:` in the summary-oracle run" to Validation 2.
3. **Fix the MultiQC prerelease claim (A2).** `\(version: v([\d\.]+)\)` does not match `(version: v3.2.0-beta.1)`, so MultiQC shows no version for betas. Reword Behavior 4 and Assumption 2, and add a prerelease string to Validation 4. Evidence: PLAN.md Context "Readers" bullet 3, Behavior 4.
4. **Handle the off-CI gates (V3).** Fix or list as known:
   - `scripts/phase_h_smoke.sh:289-336`: strict cmp of the splitting report and split files, Perl vs Rust;
   - `tests/aligner_byte_identity_real_data.rs:135-146,224-228`: the line-1 report comparison against a golden from `rust/justfile:108-113`, which survives `normalize_report` on `/home` paths;

   Both will fail after the change.
5. **Correct Assumption 1 (A1).** `build.rs` `suite_version()` falls back to `"unknown"`, so the build succeeds and writes `vunknown`. Either accept that, or have the guard test assert `SUITE_VERSION != "unknown"`.

### Optional

6. **Fix the Context table:** the bisulfite-BAM `@PG` ID is `bismark-five-base-bisulfite` (`aligner/mod.rs:797-800`). Add it and the re-encode summary (`aligner/mod.rs:986-997`) to `version_strings_1148.rs`.
7. **Reclassify `extractor/logging.rs:293`** as an input fixture that stays unchanged.
8. **Pin the guard rule (V4):** `fs` walk; truncate at the first `#[cfg(test)]`; skip `//` lines; also scan `src/**/*.html` and `assets/*.tpl`.
9. **Choose the helper's module home** (`tests/common/mod.rs` + `pub mod version;`, or a standalone `#[path]` file), given that `tests/common/` already exists.
10. **Docs scope:** the "byte-identical to Perl v0.25.1" claim also appears in `docs/src/components/Home.astro:32`, `docs/src/content/docs/installation.md:3`, `docs/src/content/docs/rust/benchmarks.md:7`, `docs/src/content/docs/rust/choosing-an-alignment-mode.md:6,50,84` and `docs/src/content/docs/rust/illumina-5-base.md:36`. Decide whether only `overview.md` carries the caveat, and say so.
11. **Drop or keep minimal the tautological constant-equality unit test (V6).**
12. **Open the nf-core snapshot follow-up with the release that ships #1148**, not afterwards.

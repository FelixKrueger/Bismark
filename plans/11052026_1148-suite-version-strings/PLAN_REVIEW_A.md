# Plan Review A — #1148 suite version strings (PLAN.md rev 1)

Reviewer A, 2026-11-05. Worktree `/Users/fkrueger/Github/Bismark-1148` at `94faf8d`. Read-only review; every claim below was checked against the code (paths relative to `rust/bismark/` unless stated).

**Verdict:** the plan is sound and the emitter list is correct. Before implementation, fix two factual errors (the MultiQC prerelease claim and the build-time fallback assumption) and widen validation to cover the 5-Base emitters and the off-CI byte-identity gates. Nothing here is Critical.

---

## 1. Logic review

### Emitter list: verified complete
The only non-comment, non-test `0.25.1` literals in `src/` are the six constants the plan names:
- `src/aligner/mod.rs:103`
- `src/extractor/output.rs:38` and `:43`
- `src/genome_prep/mod.rs:39`
- `src/report/mod.rs:33`
- `src/summary/mod.rs:45`

Every other hit sits inside a `#[cfg(test)]` module:
- `aligner/output.rs:970`→`1629`
- `aligner/report.rs:350`→`407…603`
- `bedgraph/input.rs:205`→`348`
- `extractor/logging.rs:252`→`293`
- `io/read.rs:935`→`1508`
- `nome_filtering/nome.rs:342`→`481`
- `report/reports/alignment.rs:269`→`329…348`
- `summary/parse.rs:313`→`379`

The constants are used where the plan says:
- `aligner::BISMARK_VERSION`: `aligner/report.rs:71,77` (normal and 5-Base headers share `write_report_header_for_reference`), `aligner/output.rs:117` (`generate_sam_header` `@PG VN`), `aligner/mod.rs:796` (bisulfite-BAM `@PG`) and `aligner/mod.rs:997` (bisulfite run summary).
- Extractor: `SPLIT_FILE_HEADER` at `extractor/output.rs:383`, `BISMARK_VERSION` at `extractor/output.rs:926`.
- Genome prep: `genome_prep/pipeline.rs:36`.
- Summary: `summary/html.rs:58` → template `summary/summary_template.html:875` `(version {{bismark_version}})`. The plan is right to keep the summary value without a `v`.
- `report::BISMARK_VERSION` has no caller. `bismark report` takes the version from the input report (`report/reports/alignment.rs:147,190`).

The Perl side matches. The only version prints that reach output files are `bismark:1007/1010/1642/1843` (report line 1), `bismark_methylation_extractor:4997` (splitting report) and `:5077…` (split files). No other Rust tool (dedup, bam2nuc, c2c, NOMe, methylation_consistency, filter_non_conversion, bedGraph) writes a version.

### Readers: verified
- `bedgraph/input.rs:176` and `nome_filtering/nome.rs:263` skip by `starts_with("Bismark")`.
- `report/reports/alignment.rs:53,147` takes any value up to `)`.
- `io/read.rs:900` keys on `ID:Bismark`, not on `VN`.

### `concat!`/`env!` signature: sound
`concat!("v", env!("BISMARK_SUITE_VERSION"))` compiles because `env!` expands to a string literal, and `concat!` accepts literal-producing macros. The plan correctly does **not** write `concat!("v", SUITE_VERSION)`: `concat!` rejects a `const` path. `pub const BISMARK_VERSION: &str = crate::meta::VERSION_TAG;` is fine. `SPLIT_FILE_HEADER` re-derives from `env!` rather than from `VERSION_TAG`, which gives two places that spell the `v` prefix. That is acceptable because the plan's constant-equality unit test pins them together.

### Logic gaps
- **L1, Behavior 4 is wrong for the aligner report.** MultiQC's aligner regex is `r"Bismark report for: .+ \(version: v([\d\.]+)\)"`: the `\)` must follow the digits directly (checked in an installed MultiQC, `multiqc/modules/bismark/bismark.py:33`). With `v3.2.0-beta.1)` the regex **does not match at all**, so MultiQC records no version for the alignment report. It does not "capture `3.2.0`". The extractor regex (`:53`, `Bismark Extractor Version: v([\d\.]+)`) has no trailing anchor, so it does capture `3.2.0`. The effect is harmless: the version is a separate key and the stats still parse. But Behavior 4 and the second Assumption bullet state the wrong behaviour, and Validation 4 (which runs only on `3.1.0`) would never show it.
- **L2, the "unknown" version now reaches data outputs.** `build.rs:105-131` (`suite_version`) falls back to the literal `"unknown"`. It also accepts `$BISMARK_SUITE_VERSION=unknown` as a real value, because only an empty string falls through. The `Dockerfile:17-18` default is `ARG BISMARK_SUITE_VERSION=unknown` / `ENV BISMARK_SUITE_VERSION=${BISMARK_SUITE_VERSION}`. So any `docker build .` without `--build-arg` (local, ga-candidate, third-party) sets `"unknown"`, even though `rust/VERSION` is in the build context. Today that only affects `--version`. After #1148 it writes `VN:vunknown`, `(version: vunknown)` and so on into BAMs and reports, and MultiQC's regexes fail on it. The plan's Assumption ("`env!` fails the build otherwise") is inaccurate: `env!` never fails, because `build.rs` always emits the variable.
- **L3, the guard test is ambiguous about what it matches.** Step 4 says "fails if `"v0.25.1"` or `"0.25.1"` appears as a string literal". Read literally (a whole-literal match), it would not have caught the current `SPLIT_FILE_HEADER` (`"Bismark methylation extractor version v0.25.1\n"`) or the banner format string. It needs to be a **substring** match on `0.25.1` inside non-comment code. Validation 5's `git grep -n '"v0.25.1"'` has the same gap.
- **L4, the guard's "outside `#[cfg(test)]`" rule is fragile.** Every current test hit is in a trailing `mod tests`, so "scan only the text before the first `#[cfg(test)]` line, skipping `//` lines" works today. A mid-file `#[cfg(test)]` helper would silently truncate the scan, though, and a textual parser cannot see real cfg scoping. State the exact rule in the plan, and keep it simple: scan up to the first `#[cfg(test)]`, skip comment lines, and allow-list nothing.

## 2. Assumptions

| Assumption (plan) | Status |
|---|---|
| `BISMARK_SUITE_VERSION` is always set; `env!` fails otherwise | **Partly false**: always set, but possibly to `"unknown"` (L2) |
| MultiQC capture survives prerelease suffixes | **False for the aligner report** (L1) |
| No Rust reader keys on `v0.25.1` | Verified |
| `perl-oracle` runs exactly 13 tests | Verified (`.github/workflows/rust_ci.yml:205-224`). Only **one** of the 13 compares a version-bearing file: `perl_vs_rust_nondirectional_pe_extractor` (`tests/extractor_nondir_swapped_flags_1030.rs:296-345`, splitting report + context files). The genome-prep oracles compare FASTA, the methylation_consistency oracles have no version, and dedup passes the input `@PG` through. |
| Summary oracle is among the oracles to fix | Correct, but it is **not** one of the 13. `tests/summary_perl_oracle.rs` runs in the ordinary `cargo test` job whenever `perl` and `legacy_perl/plotly` exist, which they do on ubuntu CI. It auto-skips silently (`:98-101`), so a Perl-less local run passes it vacuously. The plan's wording ("possibly others among the 13 CI oracles") suggests the 13 are the whole set. They are not. |
| Report oracle unchanged | Verified: its inputs are the checked-in fixture reports (`tests/fixtures/report/*`) |
| BAM comparisons drop `@PG` | True for the aligner real-data gate's BAM. Dedup, filter and bam2nuc BAM fixtures carry `VN:v0.25.1` (for example `tests/data/dedup/*.bam`, `tests/data/filter_nonconversion/*/in.bam`, `tests/data/bam2nuc/*.bam`) and pass it through. That is correct and unaffected. |

Unstated assumptions worth writing down:
- **A1.** The `#[ignore]` real-data gates and the shell parity harnesses are outside "the full suite" (see V2), so step 3's "run everything and fix each failure" will not reach them.
- **A2.** `report::BISMARK_VERSION` and the other constants are `pub` in a published crates.io lib (`bismark`). Removing or redefining them is technically an API change. That is negligible in practice; one line in the PR body covers it.

## 3. Efficiency
No concerns. All the values are compile-time constants. The helper does a few `str::replace` calls per compared file. The summary HTML is about 3 MB because of the embedded plotly, which is still trivial.

## 4. Validation sufficiency

- **V1, the version test omits the 5-Base emitters.** Context lists three 5-Base sites, but `tests/version_strings_1148.rs` (step 1) checks only the SE/PE report, `@PG`, extractor, summary and banner:
  - the 5-Base report header (`aligner/report.rs:54`);
  - the `--five_base_bisulfite_bam` `@PG` (`aligner/mod.rs:796`);
  - the bisulfite run summary (`aligner/mod.rs:997`).

  `tests/aligner_five_base_bisulfite.rs` already runs the converter on `tests/data/five_base_bisulfite/softclip_indel_se.bam` with no aligner, and reads the `@PG` lines (`:289-323`) and the report (`:238`). Adding both asserts there costs nothing. For the aligner report header, the unit level (`write_report_header` / `write_five_base_report_header` into a `Vec<u8>`, as `aligner/report.rs:380-395` already does) is cheaper and more reliable than an end-to-end run that needs Bowtie 2 or a fake.
- **V2, the off-CI byte-identity gates are not addressed.**
  - `tests/aligner_byte_identity_real_data.rs` (`#[ignore]`) compares the alignment report against a captured golden. `normalize_report` (`:135-146`) drops only lines containing `/Users/`, `/tmp/`, `/var/folders/` or `/private/`. On oxy the input paths are under `/home/...`, so report line 1 `(version: v0.25.1)` **is** compared and will fail against the pre-#1148 golden.
  - `scripts/phase_h_smoke.sh:289-319` and `scripts/byteid_run.sh` strict-`cmp` `*_splitting_report.txt` and the split files, Rust against live Perl. They will fail on the version line at the next full-data run.

  Step 3's "full suite on Linux" does not run `--ignored` or these scripts, so they would break silently until someone next runs them. The plan should either normalise the version there too (with the same `perl_version` rule, or a `sed` in the shell harness), or recapture the aligner golden, and note it under Integration.
- **V3, the helper's "no-op when S is 0.25.1" self-test cannot be written as specified.** `S` is the compile-time `meta::SUITE_VERSION`, which is `3.1.0` here. To test the no-op case, the helper needs the suite version as a parameter: `perl_version_for(s, suite)`, with `perl_version(s)` delegating. Then the self-test can pass `"0.25.1"`, `"3.1.0"` and a prerelease string.
- **V4, the helper should assert expected counts, not just replace.** `summary_perl_oracle.rs:40-50` (`normalize_timestamp`) already sets the precedent of asserting exactly one occurrence. Without a count, a silent no-op (an emitter that writes the old literal or a different shape) still fails the comparison, so it is not unsafe. But an assertion like "each split file has exactly one `extractor version v<S>`, line 1" gives a clear message and stops an accidental match in a body. Suggested signature: return the rewritten string plus a per-form count, or take the expected count. It would also make Validation 3 (an injected difference still fails) automatic rather than a one-off manual check.
- **V5, the MultiQC check runs only on the release version.** Validation 4 runs on `3.1.0`, so it would not catch L1. Either run it on a prerelease string too or change Behavior 4's text. Better, put the two regex-equivalent shape checks into `version_strings_1148.rs` as string assertions: line 1 ends with `(version: v` + digits/dots + `)`. Then the check is permanent rather than a note.
- **V6, the summary oracle can skip without notice.** For the local Perl run in step 6, run it with `perl` present and confirm the 17 `summary_perl_oracle` tests did not print `skipping:`. It does not honour `BISMARK_REQUIRE_PERL`.
- What **is** covered well: the 13-oracle job's run count and its no-`skipping:` check (`rust_ci.yml:226-230`) will flag a broken extractor oracle, and the constant-equality unit tests plus the guard stop a regression to a literal.

## 5. Alternatives

- **Placeholder normalisation of both sides** (`v<S>`→`v__VERSION__` on the Rust side and `v0.25.1`→`v__VERSION__` on the Perl side) rather than rewriting Rust to Perl's literal. It is equivalent in strength. It is marginally clearer in a failing diff and does not hard-code `0.25.1` in the tests. Either is fine; the plan's choice is acceptable.
- **Skip the textual guard.** The guard (step 4) parses source text. A behavioural guard is already in the plan: the integration test asserts "no `0.25.1`" in every emitter's output, and V1 extends that to all emitters. Together with the unit equality tests, that catches a regression in existing emitters without parsing source. The textual guard adds value only for a *new* emitter that hard-codes the version. Keep it if Felix wants that, but make it the simple substring rule (L3/L4).
- **Fix "unknown" at the source.** In `build.rs` `suite_version`, treat `"unknown"` like an empty env var and fall through to `../VERSION`. Alternatively, drop the `=unknown` default in `Dockerfile:17` so an unset build-arg lets `rust/VERSION` win. This is a one-line change and removes L2 entirely. It is arguably in scope, since #1148 is what makes the fallback visible in data outputs.

## 6. Action items

### Critical
None.

### Important
1. **Correct Behavior 4 and the MultiQC Assumption.** A prerelease suffix makes the aligner-report regex miss entirely (`\(version: v([\d\.]+)\)` needs `)` right after the digits; installed MultiQC `bismark.py:33`). Only the extractor regex (`:53`) truncates to `3.2.0`. State it as acceptable, and extend Validation 4 to cover a prerelease string. (L1, V5)
2. **Decide how "unknown" is handled.** `build.rs:105-131` accepts `BISMARK_SUITE_VERSION=unknown`, and `Dockerfile:17-18` defaults to it, so a plain `docker build` would write `vunknown` into `@PG` and the reports. Either make `build.rs` fall through on `"unknown"` (or drop the Dockerfile default), or record it as a known limitation. Fix the Assumption text ("`env!` fails the build otherwise" is not what happens). (L2)
3. **Add the 5-Base emitters to the version test.** Cover the `write_five_base_report_header` output (unit level, `aligner/report.rs:54`), and the bisulfite-BAM `@PG` plus run summary (`aligner/mod.rs:796,997`) through the existing `tests/aligner_five_base_bisulfite.rs` fixture run. (V1)
4. **Cover the off-CI gates.** Add `tests/aligner_byte_identity_real_data.rs` (`normalize_report` `:135`; report line 1 is compared on oxy), `scripts/phase_h_smoke.sh:289-319` and `scripts/byteid_run.sh` (strict `cmp` of splitting reports and split files against Perl) to step 3. Either normalise the version token there or recapture the golden, and note it under Integration. (V2)
5. **Make the guard a substring match on `0.25.1`** in non-comment lines before the first `#[cfg(test)]`, and fix Validation 5's grep the same way. Otherwise it misses embedded literals such as the old `SPLIT_FILE_HEADER`. (L3, L4)
6. **Make `perl_version` testable and self-checking.** Parameterise the suite version (`perl_version_for(s, suite)`) so the 0.25.1 no-op self-test can exist. Assert the expected occurrence count per form, following the `summary_perl_oracle.rs:40-50` precedent. (V3, V4)

### Optional
7. Fix the Context wording: the summary oracle is not one of the 13 `perl-oracle` tests. It runs in the ordinary test job and auto-skips without notice (`summary_perl_oracle.rs:98-101`), so confirm it actually ran during the step-6 local check. Only `perl_vs_rust_nondirectional_pe_extractor` among the 13 sees the token. (V6)
8. Put `perl_version` somewhere other than `tests/common/mod.rs`, which holds summary-only fixture builders and is used by only two test binaries. A small `tests/common/version.rs` included via `#[path]` (or `mod common;` in the extractor oracle) is fine. Keep `#![allow(dead_code)]` coverage so `clippy --all-targets -D warnings` (`rust_ci.yml:68`) stays green.
9. The docs step names `overview.md` and `installation.md`. Other "byte-identical to Perl v0.25.1" claims are in `docs/src/content/docs/rust/choosing-an-alignment-mode.md:6,50,84`, `docs/src/content/docs/rust/benchmarks.md:7`, `docs/src/components/Home.astro:32` and the `Dockerfile` image LABEL. Decide whether "except the version token" belongs there too, or whether the overview caveat is enough.
10. Mention in the PR body that the `pub` constants change value and `report::BISMARK_VERSION` is removed, since the crate is published. (A2)
11. `PROGRESS.md` still says "rev 0 written; awaiting Felix's review (Q1–Q3 open)". Update it to rev 1 with Q1–Q3 resolved.
12. The per-constant equality unit tests (step 1) are close to tautological once each constant is `= meta::VERSION_TAG`. They are worth keeping only for `SPLIT_FILE_HEADER` and `summary::BISMARK_VERSION`, which are built differently.

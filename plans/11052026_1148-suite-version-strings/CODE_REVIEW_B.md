# Code Review B — #1148 suite version strings

Reviewer B · 2026-11-05 · worktree `/Users/fkrueger/Github/Bismark-1148` (`fix/1148-version-strings`, base `94faf8d`), uncommitted diff plus the 3 untracked test files.

## Summary

The change is correct and well scoped. I found **no missed version-bearing output**. Every Perl `$version`/`$bismark_version` print into an output file (`bismark` :1007/1010/1642/1843 reports, :8480/8482 `@PG`; extractor :4997 splitting report, :5077–5688 split-file headers; `bismark2summary` :1405 footer) has a Rust counterpart that now goes through `meta::VERSION_TAG`/`SUITE_VERSION`. The 5-Base re-encode report and its `@PG` pick it up through `aligner::BISMARK_VERSION`, and `bismark2report` passes the version through from the input report, as Perl does. The `0.25.1` literals left in `src/` all sit in `#[cfg(test)]` fixtures. The test helper is narrow and fails closed. The `phase_h_smoke.sh` changes are safe under `set -euo pipefail`, and the `build.rs` change is minimal and correct.

I found nothing Critical or High. The main weakness is a blind spot in the guard test (Medium). The rest are Low-priority tightening and leftover doc claims.

Validation I ran (read-only):

- `cargo test -p bismark --test version_guard_1148 --test version_strings_1148 --test summary_perl_oracle --test extractor_nondir_swapped_flags_1030 --test extractor_se_phase_b --test extractor_output_phase_c2 --test aligner_five_base_bisulfite`: all pass. The live Perl oracles ran; none skipped.
- `summary_template_drift`, `summary_stale_oracle_tripwire`, `summary_txt_golden`, `report_perl_vs_rust`, `genome_prep_integration` and the three extractor smoke targets: all pass.
- `cargo test --lib -- aligner::report aligner::output genome_prep summary extractor::output`: 178 pass.
- `cargo fmt -p bismark -- --check` is clean, `cargo clippy -p bismark --tests` gives no warnings, and `bash -n scripts/phase_h_smoke.sh` passes.
- I hand-tested the `perl_version` sed on BSD sed with `v3.1.0`, `v3.1.0-beta.2+abc`, and a header-shaped string on a non-first line (correctly left alone). GNU sed reads `\10` as `\1` followed by `0`, so it is portable.

## Fixes applied

None. This run was recommend-only.

## Issues by area

### Logic / correctness

**L1 (Medium): the guard skips production code after a non-module `#[cfg(test)]`.**
- Where: `rust/bismark/tests/version_guard_1148.rs:14` (`hits_in_rust`). It keeps only the text before the first `#[cfg(test)]`.
- Two files have a `#[cfg(test)]` item well before their test module:
  - `src/aligner/stream.rs`: at line 529, on `fn finish`. The 479 production lines from there to `mod tests` at line 1008 are never scanned.
  - `src/aligner/config.rs`: at line 1834, on `run_config_stub`. That leaves 87 lines unscanned.
- Neither region contains `0.25.1` today, so there is no current miss. But a literal later added to `stream.rs` (the streaming converter, which does build headers) would pass the guard silently.
- Fix: cut at the first `#[cfg(test)]` whose next non-blank line starts with `mod `. Alternatively, cut at the last `#[cfg(test)]\nmod tests`. Add a self-test case where a `#[cfg(test)] fn` precedes production code.

**L2 (Low): `phase_h_smoke.sh` maps any digit-led version, not this binary's version.**
- Where: `scripts/phase_h_smoke.sh:300–303`.
- The pattern is `v[0-9][0-9A-Za-z.+-]*$`, which is looser than the Rust helper's exact-`SUITE_VERSION` match. A Rust binary writing a stale or wrong version (say `v0.24.0`, or a version from a different build) would still pass as byte-identical. `unknown` would still fail, because it is letter-led.
- The line anchors (`1s` plus `^…$`) keep the rewrite to the two intended lines, so it cannot hide data differences.
- Optional tightening: read the version once with `"$RUST_BIN" --version`, escape the dots, and substitute that exact string. Either way, the smoke run does not gate the version string itself; the Rust tests do.

**L3 (Low): the triage message mixes raw and mapped bytes.**
- Where: `scripts/phase_h_smoke.sh:325–330`.
- `SIZE_R` is still the raw Rust file size, but `FIRST_DIFF` and the triage `diff` now run on the mapped stream. On a strict-file failure, the `rust=NB` figure is off by `len(suite) − len("0.25.1")` bytes relative to the diff shown, which could mislead a reader during triage.
- Fix: compute `SIZE_R` from `perl_version < "$RUST_OUT/$f" | wc -c`, or label the figure as raw.

**L4 (Low): `mask_version` masks the golden's version without checking it.**
- Where: `tests/aligner_byte_identity_real_data.rs:150–159`.
- This is reasonable, since the golden may have been captured at either version, and current line 1 is asserted separately at `:239–244`, so nothing real is masked.
- Note that line 1 usually holds absolute input paths, so the `/Users/` and `/tmp/` filter drops it from both sides anyway. The explicit `ends_with` assertion is what actually carries the check. Consider a one-line note, or assert on golden line 1 being `v0.25.1` or the suite version, so a corrupted golden header is still noticed.

**Test helper: correct and narrow.**
- `tests/common/version.rs` uses whole-token forms that include their delimiters (`)`, `\t`, `\n`), asserts exact counts, and rewrites only after the count check. Forms are independent, so the order of replacement does not matter. The self-tests cover lookalikes (`-beta` suffix), a missed emitter (`should_panic`), and the no-op case at `0.25.1`.
- It cannot mask a real difference: the post-rewrite string must still equal Perl's bytes exactly. It cannot false-positive either, given the exact suite token.
- Minor: `AlignerReport` and `PgVersion` are used only by the self-test. That is fine with `#![allow(dead_code)]`.

**build.rs: correct.**
- `rerun-if-env-changed=BISMARK_SUITE_VERSION` was already present (`build.rs:201`).
- Treating `unknown` as unset means a bare `docker build` now falls through to the `rust/VERSION` copied by `COPY . .`, which is the intended fix.
- `version_is_never_unknown` (`tests/version_strings_1148.rs:43`) guards the fallback chain.

### Errors / robustness (`set -euo pipefail`)

- `RAW_MATCH=$(cmp … <(perl_version < …) && echo 1 || echo 0)`: the `&&/||` absorbs cmp's status. A sed failure inside the process substitution truncates its output, so cmp reports a difference and the check fails closed. Correct.
- `RMD=$(perl_version < … | sort | md5sum | cut …)` and the `zcat | perl_version | sort` pipelines: under `pipefail`, a sed or zcat failure makes the assignment fail and `set -e` aborts the script. That fails loud, which is acceptable and matches the existing `PMD` lines.
- `.gz` files skip mapping on the raw cmp. That is correct: gzip containers differ anyway, and these files fall through to the decompress-and-map sorted path.

### Structure / style

**S1 (Low): stray blank line between imports.**
- Where: `tests/summary_perl_oracle.rs:20` (between `use std::path::…;` and `use std::process::Command;`). It splits one `std` import group.
- rustfmt accepts it, so this is cosmetic. Remove it.

**S2 (Low): public constants removed or changed.**
- `bismark::report::BISMARK_VERSION` is removed. `summary::BISMARK_VERSION` and `aligner::`, `extractor::output::` and `genome_prep::BISMARK_VERSION` change value.
- They are `pub` items of a crate published on crates.io. Nothing in the workspace uses the removed one, so it only matters to outside library users, but it is a public-API change worth a mention in the PR or commit message, not the source.

**Comments: compliant.**
- Every new or edited source comment is one or two lines and describes the current state: `aligner/mod.rs:102`, `extractor/output.rs:35,38`, `genome_prep/mod.rs:37`, `summary/mod.rs:42`, `meta/mod.rs:16`, `extractor/logging.rs:73`, `build.rs:113`.
- The edit also removed the old "v0.25.1-locked" narrative from `extractor/logging.rs`.
- The `phase_h_smoke.sh` comment (2 lines) and the test comments are fine.
- The pre-existing "(The vendored tier was re-added in the Phase-4 packaging rework.)" in `build.rs:112` is historical narrative. It predates this change and need not be touched here.

**CHANGELOG: compliant with the file's format.**
- One bold sentence plus the `Reported by @bounlu.` credit. That matches the neighbouring entries' shape.
- If the one-sentence rule is read strictly, fold the credit into the sentence, for example "…instead of `v0.25.1` (#1148, reported by @bounlu)."

### Docs

**D1 (Low): unqualified "byte-identical to Perl v0.25.1" claims remain.**
- The edit qualified six places. The following still make the claim without "apart from the version string":
  - `docs/src/content/docs/installation.md:6` (body, right below the edited front-matter description)
  - `docs/src/components/Home.astro:32` (hero badge)
  - `README.md:9`
  - `docs/src/content/docs/rust/overview.md:3,7`
  - `docs/src/content/docs/rust/choosing-an-alignment-mode.md:84`
  - `rust/README.md:24`
- The per-tool rows in the `rust/README.md` table are a historical record and can stay as they are.
- Whether a short badge needs the qualifier is a judgment call. The `installation.md:6` body line sits right next to an edited one, so the inconsistency is visible there.

**D2 (Low, pre-existing, out of scope): the Docker wrapper still prints `vunknown` by default.**
- Where: `Dockerfile:91–95`.
- A local `docker build` without `--build-arg` still renders the `bismark -v` wrapper banner as `Bismark Version: vunknown`, while the real binaries now report the `rust/VERSION` value. Release builds pass the arg, so this only affects local images.
- Worth one clause in the PR ("happy to take X too"), not a change here.

## Recommendations

| Priority | Item | Where |
|---|---|---|
| Medium | L1: make the guard skip only the trailing `#[cfg(test)] mod …` block, not everything after the first `#[cfg(test)]`; add a self-test for a `#[cfg(test)] fn` before production code | `tests/version_guard_1148.rs:14` |
| Low | L2: optionally pin the smoke mapping to the Rust binary's own version | `scripts/phase_h_smoke.sh:300` |
| Low | L3: report the mapped Rust size in the strict-diff triage line | `scripts/phase_h_smoke.sh:326` |
| Low | L4: note or assert the golden line-1 version | `tests/aligner_byte_identity_real_data.rs:150` |
| Low | S1: remove the stray blank line in the imports | `tests/summary_perl_oracle.rs:20` |
| Low | S2: mention the `pub const` removal and value change in the PR or commit | `src/report/mod.rs`, `src/summary/mod.rs` |
| Low | D1: qualify the remaining byte-identity claims, at least `installation.md:6` | docs, README |
| Low | D2: follow-up only, Docker wrapper `vunknown` default | `Dockerfile:91` |
| Low | CHANGELOG: optionally fold the credit into the one sentence | `CHANGELOG.md:12` |

**Verdict:** approve with minor changes. L1 is worth fixing before merge; everything else is optional.

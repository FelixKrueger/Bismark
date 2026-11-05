# Code Review A — #1148 suite version strings

**Target:** uncommitted change in `/Users/fkrueger/Github/Bismark-1148` (branch `fix/1148-version-strings`, base `94faf8d`), including untracked `rust/bismark/tests/common/version.rs`, `tests/version_strings_1148.rs`, `tests/version_guard_1148.rs`.
**Reviewer:** A (independent). No files were modified (recommend-only run).

## Summary

The change is correct and well-scoped. Every version-bearing output Perl writes is covered: aligner report line 1, `@PG VN` (main BAM and ambig BAM through the shared `generate_sam_header`), the 5-Base converter `@PG` and report, extractor split-file header (one central write site, `extractor/output.rs:383`), splitting report, summary HTML footer, and genome-prep banner. bismark2report picks the version up from the input report, so it follows without a code change. I cross-checked against every `print … $version` / `$bismark_version` in `legacy_perl/` and found nothing missed. The other Perl tools (dedup, bam2nuc, c2c, bedGraph, NOMe, consistency, filter_non_conversion) write no version into outputs.

Verified locally (macOS):
- `version_guard_1148`, `version_strings_1148`, `summary_perl_oracle` (live Perl ran), `extractor_nondir_swapped_flags_1030` (live Perl ran), `extractor_se_phase_b`, `extractor_output_phase_c2`: all pass.
- Full `cargo test -p bismark --no-fail-fast`: the only failures are the 31 known environmental `aligner_cli` FIFO tests (none of them assert a version).
- `cargo fmt -p bismark -- --check` clean; `cargo clippy -p bismark --tests` produced no warnings.
- `phase_h_smoke.sh` `perl_version` sed checked on BSD sed: `\10.25.1` is read as `\1` + `0.25.1` (sed backrefs are single-digit, so this holds on GNU too). The `1s` anchor leaves later lines alone, a `-beta.N` suffix is mapped, and CRLF is left unmapped (fails loud).

One real gap: the guard's coverage is narrower than the plan claims (Medium). The rest are Low.

## Issues by area

### Guard (`tests/version_guard_1148.rs`)

**M1 — the cut at the first `#[cfg(test)]` silently skips production code in 2 files.** `hits_in_rust` (`version_guard_1148.rs:23-30`) keeps only the text before the first `#[cfg(test)]`. The plan (PLAN.md "Guard scope") says a file that breaks the tests-last layout gives "a false positive, never a silent miss". That is backwards: when production code follows an early `#[cfg(test)]`, the guard never scans it. Current instances:
- `src/aligner/stream.rs:529`: a `#[cfg(test)] fn finish` inside an `impl`. Lines ~530-1006 of production code (`finish_checked`, `writer_thread`, `converter_thread`, …) go unscanned.
- `src/aligner/binseq_decode.rs:191`: a nested `#[cfg(test)] mod tests` inside an inner module. Lines ~197-537 of production code go unscanned.
- `src/aligner/config.rs:1834` (the `run_config_stub`) is fine, because only test code follows it.

Nothing in those ranges carries `0.25.1` today, so there is no current bug. But the guard is weaker than it says it is, and it will get weaker as files grow.
*Recommendation:* cut at the **last** `#[cfg(test)]` that is directly followed by a `mod` line. Or cut only where `#[cfg(test)]` is followed by a top-level `mod tests` (column 0). Then add a self-test case with an inner `#[cfg(test)] fn` followed by a prod literal. Also fix the PLAN.md wording.

**L1 — literal-only match.** The guard catches exactly `0.25.1`. A future hard-coded `"v3.1.0"` or `"v0.24.2"` would pass. That is acceptable for #1148's scope. If you want wider coverage, you could also forbid `VN:v` / `version v` string literals that aren't built from `meta::`, but that is optional.

### Test helper (`tests/common/version.rs`)

The helper is correct and narrow. Every token has a hard terminator (`)`, `\t`, `\n`), so `v3.1.0-beta` cannot be mistaken for `v3.1.0` (self-test `helper_leaves_lookalikes_alone`). The exact-count `assert_eq!` before the replace turns a missed emitter into a failure (self-test `helper_panics_on_a_missed_emitter`). It cannot hide a real difference: it only rewrites the suite token at the expected forms and counts, and every other byte stays strict.

**L2 — `Form::AlignerReport` and `Form::PgVersion` are unused outside the self-tests.** The aligner real-data gate uses its own `mask_version` instead, and the `@PG` block is filtered elsewhere. `#![allow(dead_code)]` hides this. Either use `AlignerReport` in `aligner_byte_identity_real_data.rs` (see L3) or drop both forms. The second option keeps the helper's vocabulary equal to its real callers.

### `tests/aligner_byte_identity_real_data.rs`

`mask_version` (lines 150-159) is correct. It only touches lines that start with `Bismark report for: `, end in `)` and contain `(version: v`. It masks both sides symmetrically, and the new line-1 assertion (lines 239-244) pins the current report to the suite version before masking, so masking cannot hide a wrong version on the Rust side. Masking runs before the path filter, and when line 1 carries an absolute path the path filter drops it anyway, so the mask is harmless there.

**L3 — two mechanisms for one job.** Here the golden is a pre-#1148 Rust capture, so its version is not a guaranteed `0.25.1`. That makes masking a defensible choice over `perl_version(…, AlignerReport, 1)`, and the doc comment says why. Fine as-is; just note it ties into L2.

### `scripts/phase_h_smoke.sh`

The logic is correct under `set -euo pipefail`. `RAW_MATCH=$(cmp … && echo 1 || echo 0)` can't trip `-e`. A `sed` failure inside `<(…)` is not propagated, but it would show up as a mismatch and be reported, so it fails closed. The `.gz` raw-cmp path is correctly left unmapped: it always differs, and the `*.gz` arm maps after `zcat`, before `sort`. Downstream ordered arms carry no version line and correctly stay raw.

**L4 — triage sizes are raw while the comparison is mapped.** In the strict arm, `SIZE_R=$(wc -c < "$RUST_OUT/$f")` reports the unmapped size, so a genuine diff shows a misleading ±1-byte size delta from the version length (`3.1.0` vs `0.25.1`). Consider `SIZE_R=$(perl_version < "$RUST_OUT/$f" | wc -c)`.

**L5 — full-file `sed` pass over GB-scale split files.** For plain-mode split files, the first `cmp` now streams the whole Rust file through `sed`, and the sorted arm streams it again. The second expression (`s/^Bismark Extractor Version…/`) is evaluated on every line of every data file, although it only ever applies to the splitting report. At full WGBS scale this adds measurable minutes. A cheap tightening is to restrict both expressions to line 1–10, e.g. `sed -E -e '1s/…/' -e '1,10s/^(Bismark Extractor Version: v)…/'` (the report's version is on line 4), so sed's per-line regex work is trivial. Optional.

**L6 — BSD sed newline-at-EOF.** macOS `sed` appends a trailing newline when the input has none, but GNU sed does not. Bismark outputs always end in `\n` and the gate runs on Linux, so this is a theoretical concern. No action needed beyond awareness.

### `build.rs`

The change is correct. `rerun-if-env-changed=BISMARK_SUITE_VERSION` was already emitted (`build.rs:201`), so moving between `unknown` and a real value re-runs the script. Falling through to `../VERSION`, then the vendored `VERSION`, gives a plain `docker build` (no `--build-arg`) the real version. `version_is_never_unknown` backs this up.

**L7 — the Docker wrapper is still inconsistent with the binaries on a plain `docker build`.** `Dockerfile:91-94` still bakes `__SUITE_VERSION__` → `unknown` into `/usr/local/bin/bismark`'s `-v` banner, while the binaries now report the real version. This predates the change and is out of #1148's scope. It is worth a one-line follow-up (e.g. have the wrapper read `bismark.bin --version`).

### Source constants and comments

**L8 — stale comment.** `src/summary/html.rs:57`: `// bismark_version (…:1405) = the hardcoded constant (SPEC O1).` The value is no longer hard-coded. Suggest `// bismark_version (\`/g\`, \`:1405\`) = the suite version.`

**L9 — `SPLIT_FILE_HEADER` duplicates `VERSION_TAG`'s construction.** `extractor/output.rs:39-43` re-derives the tag from `env!("BISMARK_SUITE_VERSION")` because `concat!` can't take a `const`. The duplication is small and both sides are tested. Accept as-is, or add a unit assertion `SPLIT_FILE_HEADER == format!("Bismark methylation extractor version {VERSION_TAG}\n")`. `extractor_se_phase_b.rs:468-478` effectively already does this.

**L10 — stray blank line.** `tests/summary_perl_oracle.rs:20-22`: the new blank line between `use std::path…` and `use std::process::Command;` splits one import group for no reason (rustfmt accepts it). Remove it to keep the diff minimal.

The comment style otherwise complies: single-line comments that describe the current state, with no historical narrative. `extractor/logging.rs:73` correctly drops the old "NOT the v0.25.1-locked" contrast. The `phase_h_smoke.sh` comment is two lines and describes the current state.

### CHANGELOG / README / docs

CHANGELOG: one bold sentence plus the file's existing `Reported by` attribution, which complies with the one-sentence rule. The `rust/README.md` Milestones line follows the journal's established format. The docs edits consistently add "apart from the version string". I found no docs page showing a sample output with the old `v0.25.1` header that would now be wrong.

## Fixes applied

None (recommend-only run).

## Recommendations

| Priority | ID | Recommendation |
|---|---|---|
| Medium | M1 | Make the guard cut at the last top-level `#[cfg(test)] mod`, not the first `#[cfg(test)]`. `stream.rs` (~477 prod lines) and `binseq_decode.rs` (~340 prod lines) are currently unscanned. Add a self-test, and correct the PLAN.md "never a silent miss" claim. |
| Low | L2/L3 | Drop the unused `Form::AlignerReport`/`Form::PgVersion`, or use them. |
| Low | L4 | Report the mapped Rust size in the `phase_h_smoke.sh` triage line. |
| Low | L5 | Restrict the `sed` expressions to the first few lines to avoid a full regex pass over GB split files. |
| Low | L8 | Fix the stale "hardcoded constant" comment at `summary/html.rs:57`. |
| Low | L10 | Remove the stray blank import line in `summary_perl_oracle.rs`. |
| Low | L7 | Follow-up (out of scope): the Docker `bismark` wrapper still prints `vunknown` on a plain `docker build`. |
| Low | L1/L6/L9 | Awareness only; no change required. |

No Critical or High findings.

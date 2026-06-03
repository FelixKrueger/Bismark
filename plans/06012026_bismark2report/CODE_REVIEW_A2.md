# CODE_REVIEW_A2 — Second-round focused review of the 4 post-review fixes

**Reviewer:** Code Reviewer A2 (fresh context, second round)
**Crate:** `bismark-report` (Rust port of Perl `bismark2report v0.25.1`)
**Scope:** ONLY the 4 fixes applied in response to the first dual review (PLAN §15) — verifying each correctly resolves its finding without regressing byte-identity. Recommend-only; no source modified.
**Date:** 2026-06-01

## Summary

**All 4 fixes are sound.** Each correctly resolves its first-round finding and matches live-Perl behavior, verified against the running Perl `bismark2report v0.25.1` on the exact edge cases the task enumerated. No regressions found. **No new byte-identity-affecting issue.**

Gates re-run locally (worktree, sandbox disabled):
- `cargo test -p bismark-report` → **52 passed / 0 failed** (39 unit + 8 CLI + 5 Perl-oracle byte-identity incl. the new `crlf_alignment_byte_identical`).
- `cargo clippy -p bismark-report --all-targets -- -D warnings` → clean.

I additionally ran the **live Perl regex** for every version-parse case and **live Perl `File::Glob`** for the collation cases (including `LC_ALL=C` / `en_US.UTF-8`) to confirm the Rust port matches ground truth, not just its own assertions.

---

## Per-fix verdicts

### Fix 1 — HIGH — CRLF version parse (`alignment.rs::parse_report_for`) — CORRECT, no regression

The change from `after.last() == Some(&b')')` to `rfind(after, b")")` (version = bytes up to the LAST `)` within `after`) faithfully reproduces Perl's `/^Bismark report for: (.*) \(version: (.*)\)/` — greedy, non-`$`-anchored, last-`(version:`-wins. Verified against live Perl for all required cases:

| Input after `report for: `        | Perl fname            | Perl ver       | Rust result        | Match |
|------------------------------------|-----------------------|----------------|--------------------|-------|
| `s.fq.gz (version: v0.25.1)` (clean) | `s.fq.gz`           | `v0.25.1`      | same               | ✓ |
| `s.fq.gz (version: v0.25.1)\r` (CRLF)| `s.fq.gz`           | `v0.25.1`      | same (`\r` ignored)| ✓ |
| `s.fq.gz (version: v0.25.1)beta)`  | `s.fq.gz`             | `v0.25.1)beta` | same (last `)`)    | ✓ |
| `s.fq.gz (version: v0.25.1` (no `)`)| **NO MATCH** (undef)  | undef          | **both None**      | ✓ |
| `s.fq.gz` (no `(version:`)         | **NO MATCH**          | undef          | both None          | ✓ |
| `A (version: v1) extra (version: v2)`| `A (version: v1) extra` | `v2`        | same (rfind sep)   | ✓ |
| `A) (version: v1)` (`)` in fname)  | `A)`                  | `v1`           | same               | ✓ |
| `A (version: )` (empty version)    | `A`                   | `` (empty)     | same (ver=`after[..0]`) | ✓ |

(a) CRLF fixed; (b) no regression on normal / no-`)` / embedded-paren / greedy-last-sep cases; (c) **no panic** on `after[..rparen]` — `rparen` is always a valid in-bounds index from `rfind`, and the empty-version case yields the zero-length slice `after[..0]` which is valid. The two-`(version:)` case is correctly handled by the existing `rfind` for the separator (untouched by this fix). Verdict: **correct**.

### Fix 2 — MEDIUM — glob collation (`discovery.rs`) — CORRECT, no regression

`out.sort()` → `out.sort_by(|x,y| glob_order_key(x).cmp(&glob_order_key(y)).then_with(|| x.cmp(y)))` with `glob_order_key = as_encoded_bytes().to_ascii_lowercase()`.

Verified against **live Perl `<*E_report.txt>`** with extra adversarial names (`1_, _, a2_, a_, B_, C_`):
- Perl (default, `LC_ALL=C`, and `en_US.UTF-8` — all identical): `1_, _, a2_, a_, B_, C_`.
- Rust `glob_order_key` model (simulated): **identical** — including the subtle `_`(0x5F) before `a_`, `1_` before `_`, `a2_` before `a_` (`2`<`_`), and the lowercase-`a` group sorting ahead of `B_`/`C_`.

Notably, macOS Perl's `File::Glob` is case-folded even under `LC_ALL=C` (the `:nocase` default per `File/Glob.pm`), so the ASCII-lowercase primary key is the right model on the gate platform (where F1 ran). The committed test `glob_order_matches_perl_caseinsensitive_collation` pins `a2_, a_, B_, C_` — matches verified Perl, **not a tautology**.

The `then_with(|| x.cmp(y))` raw-byte tiebreak is stable and only engages for case-only-differing names (e.g. `a_` vs `A_`), which require a case-sensitive FS (Linux/oxy) to coexist. There, GLOB_NOCASE is off and glob falls back to raw-byte `strcmp`, ordering `A_`(0x41) before `a_`(0x61) — exactly what the raw-byte tiebreak produces. So the tiebreak is consistent with both platforms; it is low-stakes and unobservable on the case-insensitive local FS, but defensibly chosen. Doc comment (`discovery.rs:1-12`, `183-186`) is now accurate (it correctly scopes the byte-relevance to the line-1256 first-report reset). Verdict: **correct**.

### Fix 3 — LOW — `-o ""` (`lib.rs`) — CORRECT, no regression

`out_name` now uses `Some(o) if !o.is_empty()` (truthiness) while the `>1`-report guard keeps `is_some()` (defined). Confirmed against the Perl source:
- Perl line 50–52: `if ($manual_output_file)` → **truthiness**; `-o ""` is falsy → derives the name. Rust matches: empty falls through to `derive_output_name`. ✓
- Perl line 1129: `if (defined $manual_output_file)` → **defined**; `-o ""` is defined → triggers the >1-report die. Rust guard (`lib.rs:62`, `is_some()`) matches. ✓

The asymmetry is faithfully reproduced. (`-o ""` with >1 report errors in both; `-o ""` with one report derives in both.) Verdict: **correct**.

### Fix 4 — LOW — doc reference (`assets.rs:6`) — CORRECT

The doc comment now references `embedded_assets_match_repo_plotly_files`, which exists as the test at `assets.rs:121`. No more dangling reference to the non-existent `tests/assets.rs`. Verdict: **correct**.

---

## New/changed test sanity-check

- `alignment.rs::version_parsed_on_crlf_line_trailing_cr_ignored` — asserts `v0.25.1` from a `\r`-terminated line. Pre-fix this would have set both fields None. **Non-tautological, asserts the fixed behavior.**
- `alignment.rs::version_uses_last_paren_when_value_contains_paren` — asserts `v0.25.1)beta` for the embedded-paren input. Matches live Perl. **Non-tautological.**
- `discovery.rs::glob_order_matches_perl_caseinsensitive_collation` — asserts `a2_, a_, B_, C_`, the empirically-verified Perl order. **Non-tautological** (pure byte `sort()` would give `B_, C_, a2_, a_`).
- `perl_vs_rust.rs` refactor into `run_both_and_compare` + `crlf_alignment_byte_identical` — `run_both_and_compare` runs the **live Perl** and the Rust binary independently on the same dir and byte-compares (timestamp-normalized); it is NOT a self-comparison. The CRLF test builds a genuine CRLF report from `minimal_pe/sampleD_PE_report.txt` (confirmed to contain `Bismark report for: … (version: v0.25.1)`, with the 5-field gate passing), so the `{{filename}}`×2 + `{{bismark_version}}` substitutions are actually exercised — a pre-fix run would diverge. **Meaningful regression guard.**

---

## New issues

**None** affecting byte-identity. The four fixes are localized, minimal, and match Perl on every probed case.

## Recommendations (by priority)

- **None blocking.** All fixes can ship as-is.
- **Low / optional (informational, not a defect):** the glob raw-byte tiebreak for genuine case-only-differing report names is verified-by-reasoning but **not exercised by a test on a case-sensitive FS** (local FS is case-insensitive; the committed test cannot create `a_`+`A_` as distinct files). If a Linux/oxy CI ever wants belt-and-suspenders coverage, add an `#[ignore]` real-data-style test that drops `a_`/`A_` reports + an explicit `--dedup_report` and asserts the explicit companion attaches to the `A_`-sorted report. This is genuinely low stakes (Bismark always passes explicit `--alignment_report`), so a SPEC/doc note that this ordering is unverified-on-case-sensitive-FS is an acceptable alternative.

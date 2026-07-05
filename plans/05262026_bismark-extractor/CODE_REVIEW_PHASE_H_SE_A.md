# Code Review — Phase H SE matrix harness (Reviewer A)

**Branch:** `extractor-phase-h-se` off `rust/iron-chancellor` HEAD `f88bad7`
**Scope:** `scripts/phase_h_se_matrix.sh` (new, 551 LOC), `scripts/phase_h_smoke.sh` (renamed + edited), `RELEASE_CHECKLIST.md` (new), `rust/bismark-extractor/SPEC.md` §8.3/§9.7/§10 updates, plan rev 2 Implementation Notes.
**Reviewer designation:** A — independent of Reviewer B.

## Summary

This is harness-and-checklist work; no Rust code changes (303-test baseline preserved). The matrix driver implements all rev 1 absorbed findings (cross-N invariance per ignore-pair, Perl version pre-flight, `nproc` advisory, 4-way exit-code mapping, Perl-only + Rust scaling columns, M-bias regression guard) and the bash mechanics are mostly sound. I mental-executed the cross-N pair-comparison loop, the wall-clock parse, the Perl version regex against the actual Perl script (`my $version = 'v0.25.1';`), and the bash-array `--extra-*` passthrough — all behave as the plan intends in the **happy path on bash ≥4.4 with `--parallel-set "1 4"`**.

The defects I'm flagging are **edge-of-CLI** issues, not happy-path bugs: a silent M-bias regression-guard bypass via non-default `--parallel-set`, bash-version assumptions (4.0+ for `declare -A`, 4.4+ for `"${EMPTY[@]}"` under `set -u`), and a few minor classification/precision issues. No critical bug exists when invoked per the RELEASE_CHECKLIST default workflow.

## Issues by area

### Logic

**A-L1 [High] M-bias 5712 B regression guard is silently bypassed when `--parallel-set` omits N=1.**
`scripts/phase_h_se_matrix.sh:303-322` only checks M-bias against the locked 5712 B baseline on the cell where `NAME == "D" && N == "1"`. If a user invokes the driver with `--parallel-set "4"` (or `"4 8"`), no `(D, N=1)` cell exists → `DEFAULT_N1_SUBDIR=""` → `MBIAS_ACTUAL_SIZE=""` → speedup_table emits `⚠️ Could not locate M-bias.txt in (D, N=1) cell — investigate.` But the verdict code at lines 529-531 only fails when `MBIAS_BASELINE_OK -eq 0`; the initial-default `MBIAS_BASELINE_OK=1` at line 312 means the regression guard silently no-ops, returning exit 0 if everything else passes. **Recommendation:** Either (a) hard-fail in pre-flight if `"1"` not in `$PARALLEL_SET` (the simplest fix; RELEASE_CHECKLIST already mandates `"1 4"`), or (b) treat "could not locate" as `MBIAS_BASELINE_OK=0` so the verdict reports FAIL and forces the user to address it explicitly.

**A-L2 [Medium] Matrix verdict cannot distinguish "byte-identity FAIL" from "binary crashed".**
The smoke script exits 1 both for byte-identity differences (line 314) AND for binary crashes via the `|| { echo "...failed"; exit 1; }` arms at lines 174/188. The matrix driver classifies any exit 1 as `VERDICT="FAIL"`. Operationally this means a colossal-side Perl-binary crash (OOM, segfault) shows up as a byte-identity failure in `matrix_verdict.txt`, sending the release engineer down the wrong investigation path. **Recommendation:** Smoke could reserve a distinct exit (e.g. 4 for crash) and matrix could map it to a separate verdict (`CRASH`). Low-effort, high diagnostic value.

**A-L3 [Medium] `--parallel-set "1"` (single N) silently treats perf-target as "met".**
At line 437 `PERF_TARGET_MET=1` defaults to "met" and the guard at line 439 only enters when `HIGHEST_N != BASELINE_N`. Single-N runs therefore exit 0 with `PERF_TARGET_VALUE=""` (correctly emitting "insufficient data" in the table) but the verdict-text on line 537 says "perf target met" which is technically inaccurate ("not measured" would be honest). **Recommendation:** Introduce a tri-state — `PERF_TARGET_MET=2` for not-measured — and adjust verdict-text. Or simply word the verdict as "perf target not measured (single N)" when `PERF_TARGET_VALUE` is empty.

**A-L4 [Low] `cell_p1_i0_i30` in RELEASE_CHECKLIST.md is visually ambiguous.**
`RELEASE_CHECKLIST.md:97` references `cell_p1_i0_i30`. The actual directory naming is `cell_p<N>_i<5p>_i3<3p>` so `i0_i30` means "i5p=0, i3p=0", not "i5p=0, i3p=30". A reader unfamiliar with the format might misread. **Recommendation:** Either add a one-line note ("the `i3` is the prefix for ignore_3prime; trailing 0 is its value") or insert a separator (e.g. `cell_p1__i0__i3_0`). Cosmetic only.

### Efficiency

**A-E1 [Low] Per-N average uses integer division before formatting.**
Lines 399-400 compute `PAVG=$(( N_PERL_SUM[$n] / COUNT ))` as integer seconds before any decimal formatting. For low-runtime cells (e.g. the `edge_clip` cell which may produce empty data files), the sum might round-truncate appreciably (e.g. sum=23s, count=5 → avg=4s instead of 4.6s). The ratio computation downstream then uses these rounded averages, propagating up-to-~10% rounding error into the speedup numbers. **Recommendation:** Compute averages as `×100` first to preserve 2-decimal precision (consistent with the ratio code at line 354). Easy fix; meaningful for sub-minute cells.

### Errors

**A-Er1 [High] `declare -A` requires bash 4.0; no version check before use.**
`phase_h_se_matrix.sh:173-174` and `:372-376` use `declare -A`. macOS ships bash 3.2; running `bash scripts/phase_h_se_matrix.sh ...` from a Mac terminal (which a release engineer might do mid-session, e.g. local dry-run before tmux-on-colossal) produces a cryptic `declare: -A: invalid option`. Confirmed by reproduction on the dev box. **Recommendation:** Add a pre-flight `[[ ${BASH_VERSION%%.*} -ge 4 ]] || { echo "this script requires bash 4.0+ (macOS users: install via brew); current: $BASH_VERSION" >&2; exit 2; }` near the top, BEFORE the args parser. Same for the smoke if/when it grows associative arrays.

**A-Er2 [High] Empty `EXTRA_FLAGS=()` + `set -u` errors on bash <4.4.**
`scripts/phase_h_smoke.sh:60` sets `set -euo pipefail`. `EXTRA_FLAGS=()` (line 136) is empty when MODE=default (the matrix's mode). On bash 4.3 and earlier, expanding `"${EXTRA_FLAGS[@]}"` of an empty array under `set -u` raises `unbound variable`. Reproduced on the dev Mac's bash 3.2. Modern Linux (CentOS 7+, Ubuntu 18.04+) ships bash 4.4+ which fixed this, so colossal is almost certainly safe — but the dependency is implicit. **Recommendation:** Either match A-Er1 with a `[[ ${BASH_VERSION%%.*} -ge 4 && ... ]]` guard, or use the defensive idiom `${ARR[@]+"${ARR[@]}"}` at expansion sites. The defensive idiom is the most portable single-line fix.

**A-Er3 [Medium] `for f in $(comm -12 ...)` word-splits on whitespace in filenames.**
Lines 275 and others rely on Bismark output filenames being whitespace-free (they are — derived from BAM basename + `_OT_` / `_CTOT_` etc.), so this is happy-path safe. But it's a fragile pattern that becomes a real bug if a future basename contains spaces. **Recommendation:** Switch to `while IFS= read -r f; do ... done < <(comm -12 ...)`. Mechanical change.

**A-Er4 [Low] Wall-clock parsing tolerates leading zeros but the regex's `head -1` masks dup-line bugs.**
If for any reason the smoke's `diff_summary.txt` contains two `Perl: <int>s` lines (e.g. due to a future bug in the summary writer), the matrix's `head -1` silently picks the first. Defensive measure: assert `grep -cE '^Perl: [0-9]+s$' "$SUBDIR/diff_summary.txt" -eq 1` or emit a warning if multiple matches. Not a current bug; future-proofing.

**A-Er5 [Low] `"$PERL_VERSION" != "Bismark Extractor Version: v0.25.1"` is exact-match.**
The regex `grep -oE 'Bismark Extractor Version: v[0-9.]+'` will match anything with that prefix and digits/dots. The follow-up `[[ "$PERL_VERSION" != "Bismark Extractor Version: v0.25.1" ]]` is then exact-string compare. If `bismark_methylation_extractor` ever updates and prints `v0.25.1.1` or `v0.25.10`, the regex would capture the longer string and the equality check would correctly fail. So the current logic is robust to that edge case. No fix needed; documented for completeness.

### Structure

**A-S1 [Low] 551-LOC bash without lint coverage is risky.**
The plan deviation note acknowledges shellcheck wasn't installed on the dev Mac. For a script gating a v1.0 release tag, **installing shellcheck and running it** is the cheapest single addition. shellcheck will catch the `set -u` + empty array, the `for f in $()` word-split, the unquoted ratio expansions in printf, and a few other classes I haven't enumerated.
**Recommendation:** Either add shellcheck to the dev-Mac toolchain (`brew install shellcheck`) and run it pre-merge, or accept the deviation explicitly in plan rev 2 (it's already there). I'd add it; the script is operationally load-bearing.

**A-S2 [Low] `cmp` output line format coupling.**
Smoke's line 268 captures `FIRST_DIFF=$(cmp ... | head -1)` to display in `diff_summary.txt`. `cmp`'s output format ("file1 file2 differ: byte N, line M") varies between GNU and BSD cmp. Colossal is Linux (GNU), so OK. Cosmetic only.

**A-S3 [Low] SPEC §8.3 5-cell table matches matrix-driver `MATRIX_CELLS` array byte-for-byte.**
I cross-checked SPEC.md:742-748 against `phase_h_se_matrix.sh:148-154`. Cell names (D / 5p / 3p / 5p+3p / edge_clip) and ignore-flag values (0,0 / 5,0 / 0,5 / 5,5 / 250,0) all match. ✅

## Fixes applied

None — all findings are recommendations rather than unambiguous low-risk fixes. The matrix driver is the v1.0 release-gate script; even mechanical changes warrant Felix's sign-off rather than mid-review patching.

## Recommendations summary

| Priority | ID | What |
|---|---|---|
| High | A-L1 | M-bias 5712 B guard silently bypassed if `--parallel-set` omits N=1. Hard-fail in pre-flight, or treat "not located" as FAIL. |
| High | A-Er1 | Pre-flight `BASH_VERSION ≥ 4.0` check (matrix uses `declare -A`). |
| High | A-Er2 | `"${EXTRA_FLAGS[@]}"` etc. fails under `set -u` on bash <4.4. Use `${ARR[@]+"${ARR[@]}"}` defensive idiom. |
| Medium | A-L2 | Smoke can't distinguish byte-identity FAIL from binary crash; both exit 1. |
| Medium | A-L3 | Single-N run reports "perf target met" when it should report "not measured". |
| Medium | A-Er3 | Switch `for f in $(comm -12 ...)` to `while read` to protect against future whitespace in filenames. |
| Low | A-L4 | `cell_p1_i0_i30` directory name is visually ambiguous in RELEASE_CHECKLIST.md. |
| Low | A-E1 | Per-N averages use integer division before ratio formatting; ~10% rounding error possible on sub-minute cells. |
| Low | A-Er4 | Wall-clock regex `head -1` masks future dup-line bugs. |
| Low | A-Er5 | Documented: exact-match Perl version check is robust to future `v0.25.10` edge case. |
| Low | A-S1 | Install + run shellcheck on both scripts before merge. |
| Low | A-S2 | `cmp` output line format varies between GNU and BSD; happy-path on colossal Linux. |
| Low | A-S3 | SPEC §8.3 5-cell table verified byte-consistent with `MATRIX_CELLS` array. |

## Verdict

**APPROVE-WITH-NITS.**

The matrix driver correctly implements all rev 1 absorbed findings — cross-N invariance per ignore-pair (verified by mental-executing the all-pairs loop on `"1 4"` → 1 comparison and `"1 4 8"` → 3 comparisons), Perl version pre-flight (verified against the actual Perl script's `'v0.25.1'` literal), wall-clock parsing (anchored regex matches smoke's emission shape), 4-way exit-code mapping (USAGE > FAIL > cross-N > M-bias > perf precedence reads correctly for the release-gate context), and `--out` dir empty/non-empty distinction (`[[ -e ]] && ls -A` correctly handles all three cases).

The two High-priority findings — M-bias bypass via non-default `--parallel-set` (A-L1) and bash-version assumptions (A-Er1 / A-Er2) — are real but operationally low-risk under the documented RELEASE_CHECKLIST workflow (which always uses `--parallel-set "1 4"` on colossal). I'd recommend addressing A-L1 (the M-bias bypass) before merge since the regression guard is a load-bearing part of the matrix's purpose, and at minimum adding the `BASH_VERSION` pre-flight for A-Er1 to convert a cryptic error into a clean exit.

The Medium findings are defensive improvements; the Low findings are polish. None of these gate the v1.0 release tag if Felix accepts them as known-issues in plan rev 3 and runs only the documented happy-path invocation.

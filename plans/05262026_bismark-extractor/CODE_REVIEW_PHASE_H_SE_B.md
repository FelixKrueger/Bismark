# Code Review — Phase H sub-gate 1 SE harness (#871) — Reviewer B

**Branch:** `extractor-phase-h-se` off `rust/iron-chancellor` HEAD `f88bad7`
**Reviewer angle:** orthogonal to Reviewer A — focused on operational realities, shell-script portability, RELEASE_CHECKLIST.md workflow practicality, M-bias baseline-drift handling at non-default cells, plan-vs-impl fidelity, #872 coupling, alternative-approach trade-offs, shellcheck-miss classes, `--extra-*` design, and interrupt recovery.

## Summary

The PR is harness/checklist/SPEC-only with no Rust code changes, and the 303-test baseline is preserved. The matrix driver implements the rev-1 design competently (pre-flight, 5-cell × N matrix, cross-N N-invariance, M-bias regression guard, 4-way exit-code mapping, speedup table) and the rename of `oxy_phase_h_smoke.sh` is clean with a sensible `--extra-rust` / `--extra-perl` pass-through.

However, I found one **Critical** bug in the speedup-table column semantics (column-header label `Rust/Perl` vs computed value `Perl/Rust` are inverses — the column will display 0.90× when the plan example shows 1.11×), one **High** logic gap where a missing M-bias file silently passes the baseline guard, and several portability + operational concerns around `uptime` parsing, `cargo build` budget, tmux non-enforcement, and a latent `.gz` cross-N false-fail if anyone runs `--extra-rust "--gzip"`. RELEASE_CHECKLIST.md is operationally well-thought-through but has a few gaps (no documented recovery for `cargo build` budget overrun, no #872-blocked guard in v1.0 tag list other than a prose mention).

The 551-LOC matrix driver is significantly larger than the ~250 LOC plan estimate; ~200 LOC of that is the speedup-table emission + per-N aggregate arithmetic, which is potentially over-engineered (could be a Python one-pager with cleaner arithmetic) but is justifiable as a one-shot artifact.

**Verdict: NEEDS-REVISIONS** — the Rust/Perl column inversion is a release-gate evidence bug; the M-bias missing-file fail-open is a regression-guard hole.

---

## Issues by area

### Logic

**[Critical-L1] Speedup-table column header `Rust/Perl` mismatches computed value (`Perl/Rust`).**
`scripts/phase_h_se_matrix.sh:344` declares the column `| Rust/Perl |` in the per-cell wall-clock table, AND `| Avg Rust/Perl |` in the per-N aggregate. But the math at lines 354, 411-412, 417-418 computes `P * 100 / R` — i.e. Perl/Rust. The PHASE_H_SE_PLAN.md §3.3.5 example confirms the intended display: at (D, N=1) with Perl=720s, Rust=800s, the example shows "1.11×" (= 800/720 = Rust/Perl ratio); at (D, N=4) with Perl=180, Rust=150 it shows "0.83×" (= 150/180 = Rust/Perl). The code, however, will emit "0.90×" and "1.20×" — the inverses. This is release-gate evidence: a reader looking at the table will mis-interpret "0.90×" as "Rust is slower than Perl by 10%" when the plan-intended semantics is "Rust takes 90% of Perl wall-clock = 10% faster". A future release-engineer comparing against the plan example will be confused; worse, a future "perf regression < 1.0×" guardrail would have the wrong polarity.

Fix is one of: (a) flip the arithmetic to `R * 100 / P`; or (b) rename the column header to `Perl/Rust` (less intuitive) and update the plan §3.3.5 example to match. Recommend (a).

**[High-L2] M-bias missing-file fail-open at baseline guard (lines 314-322, 312, 529).**
`MBIAS_BASELINE_OK=1` is initialized as PASS. The `ls`-pipeline that locates `*M-bias.txt` uses `2>/dev/null | head -1 || true` and can silently produce an empty `MBIAS_FILE`. If the file isn't found, the `if [[ -n "$MBIAS_FILE" ]]` block is skipped entirely, `MBIAS_BASELINE_OK` stays at `1`, and the exit-code logic at line 529 (`elif [[ "$MBIAS_BASELINE_OK" -eq 0 ]]`) does NOT trigger FAIL. The speedup-table emits "⚠️ Could not locate M-bias.txt" (warning only), so the matrix exits 0 (or 3) under what should be a hard FAIL — the Phase C.1 5712 B regression guard is supposed to be a release-blocker (§3.3.6, §3.4 #4). A subtle bug in the Rust output-dir code that suppresses M-bias.txt entirely would slip through this matrix.

Fix: when `MBIAS_FILE` empty, set `MBIAS_BASELINE_OK=0` (or a sentinel like `MBIAS_BASELINE_OK=-1` distinguishing "drift" from "missing"). Then the exit-code logic catches it.

**[Medium-L3] `phase_h_smoke.sh` failure modes leak through `set +e ... set -e` window with no per-cell `tee` of output.**
`scripts/phase_h_se_matrix.sh:188-197`: `set +e` brackets the smoke invocation. Its stdout/stderr goes to the matrix's stderr via `>&2`. If a cell crashes mid-way (e.g. Rust binary panics), the matrix records VERDICT=FAIL (rc=1) but the cell's diagnostic output is interleaved with subsequent cells' output. There's no per-cell log file capturing only that cell's run. A release engineer triaging a mid-matrix FAIL has to disentangle the colossal `tmux` scrollback. Recommend tee-ing each cell's smoke output to `<SUBDIR>/run.log` for easier post-hoc diagnosis (would be ~3 LOC).

**[Medium-L4] M-bias row-count differential check from plan §3.4 #4 is not implemented in driver.**
Plan rev 1 I15 explicitly says: "Other ignore-flag cells: must equal Perl byte-for-byte ... row-count differential check added per rev 1 I15: M-bias row count for (5p, 0) cell must be < (D, N=1) cell's row count; ditto for (0, 3p) and (5p, 3p); for edge_clip cell expect empty/zero rows". The driver hard-fails only on the (D, N=1) 5712 B size; it does NOT compare row counts across ignore-flag cells against (D, N=1). Plan §9.2 lists this as "M-bias row-count differential for `--ignore 5` cells (rev 1 I15)" in the colossal release-gate validation table — implying it's an explicit gate.

The smoke script's per-cell `cmp -s` against Perl catches it indirectly *if Perl honours `--ignore` correctly*, but the differential check is a *silent-no-op* guard for the Rust extractor (catches a regression where Rust ignores the flag but still produces matching output by coincidence). Either implement the differential check in the driver or document it as a deferred follow-up in the plan's deviation log.

**[Medium-L5] `cargo build` budget not surfaced in pre-flight or RELEASE_CHECKLIST timing estimate.**
The driver defers Rust-binary discovery to the smoke (line 116-117 comment). On a fresh colossal session with cold cargo cache, `cargo build --release -p bismark-extractor` can take 8-15 minutes pulling/compiling all transitive deps (noodles, rayon, etc.). The smoke triggers this on the *first cell* — extending the wall-clock of that cell by 8-15 min and SKEWING the (D, N=1) Perl-vs-Rust speedup measurement for that cell because the timer (line 181: `RUST_START=$(date +%s)`) starts AFTER the cargo build returns — so actually OK for the timer, but the cell's *real* wall-clock seen by the user is 8-15 min longer than the table entry. Recommend RELEASE_CHECKLIST.md notes "first run: cargo build may take 10+ min" so the user doesn't kill the matrix on apparent stall.

**[Medium-L6] `tmux` recommendation is not enforced; driver has no pre-flight check for interactive-session vs detached-session.**
RELEASE_CHECKLIST.md §pre-matrix-setup says "Run inside tmux or screen". If the user forgets and SSHes directly into colossal, then `dcli ssh colossal` disconnects 2h in, the driver dies, the partial `<OUT>/` is left as evidence (which is good for forensics but the v1.0 release is delayed by a full re-run). Driver could detect missing `$TMUX` and `$STY` and emit a warning. ~3 LOC. Not critical but operationally sensible.

**[Low-L7] Cross-N raw-`cmp` will spuriously fail on `.gz` outputs if anyone runs `--extra-rust "--gzip"`.**
`scripts/phase_h_se_matrix.sh:276`: `cmp -s "$RUST_DIR_A/$f" "$RUST_DIR_B/$f"` over every file. The current SE matrix runs `--mode default` (no `--gzip`), so all outputs are plaintext. BUT the `--extra-rust`/`--extra-perl` pass-through can inject `--gzip`. Gzip headers include OS byte + timestamps that are typically non-deterministic across runs, so two byte-identical decompressed streams produce two byte-different `.gz` files. A future matrix run with `--gzip` injected would spuriously cross-N FAIL. Mitigation: either (a) cross-N check decompresses `.gz` files before `cmp` (mirroring smoke §SPEC-§8.3 logic); or (b) document "do not inject `--gzip` via `--extra-*`; cross-N raw-cmp does not handle it". Not in scope of #871's current cell-set, so Low priority.

### Efficiency

**[Low-E1] `cmp -s` runs serially over all cross-N file pairs.**
~6 files × 4 pairs = 24 `cmp` calls. Cheap. No action.

**[Low-E2] `ls -1 | sort` in two places — equivalent to `printf '%s\n' *` + `sort`, but the `ls` invocation is fine.**
No action.

### Errors / Operational

**[High-E3] `uptime` load-average parse is fragile across `uptime` output dialects.**
`scripts/phase_h_se_matrix.sh:137`: `LOAD=$(uptime | awk -F'load average:' '{print $2}' | awk '{gsub(/,/,""); print $1}')`. Linux GNU uptime: `... load average: 0.06, 0.12, 0.08` — works. macOS BSD uptime: `... load averages: 0.06 0.12 0.08` (plural "averages", no comma) — `awk -F'load average:'` does not split on "load averages:", so `$2` is empty, `$1` of the empty string is empty, and the subsequent `awk "BEGIN {exit !($LOAD > $NCORES)}"` evaluates `BEGIN {exit !( > N)}` which is an awk syntax error, breaking with `set -e`. On Linux with non-standard locale: `uptime` may localize the label. Colossal is presumably Linux GNU, so most likely fine, BUT a defensive `if [[ -n "$LOAD" ]]` guard around the awk-compare would make the script portable. ~2 LOC.

Actually re-checking: `awk "BEGIN {exit !($LOAD > $NCORES)}"` with empty `$LOAD` produces `BEGIN {exit !( > 16)}` which IS a syntax error → `awk` exits with rc=1 → under `set -e` this kills the script. The `if awk ...; then` form catches non-zero as falsy, so script continues silently — only the load warning is skipped. So defensively safe under `set -e`'s interplay with `if`. Still, an obvious silent-skip bug-trap.

**[Medium-E4] `command -v nproc` fallback uses `NCORES=4` but then `for n in $PARALLEL_SET; do if [[ "$n" -gt "$NCORES" ]]` will reject N=8 on a machine that may actually have 16 cores.**
Line 121-122: if `nproc` is missing, the fallback `NCORES=4` is a *floor* assumption that hard-fails legitimate `--parallel-set "8"` runs. macOS has `sysctl -n hw.ncpu`; Linux has `nproc`. For colossal (Linux), `nproc` is standard, so this code path is unlikely to fire. But if it does, the user gets an opaque "requested N=8 exceeds available cores (4)" while their machine has 16. Either: (a) skip the check entirely when `nproc` missing (the comment says "skipping core-count check"); (b) try `sysctl -n hw.ncpu` as a second fallback. Currently the script SAYS it's skipping the check then runs the check anyway with the floor value.

Fix: when `nproc` missing, just `NCORES=999` or set a flag and skip the per-N check. ~3 LOC.

**[Medium-E5] Pre-flight Perl version assertion uses the *repo's* Perl script, not the micromamba env's binary.**
Line 101: `PERL_BIN="${PERL_BIN:-$REPO_ROOT/bismark_methylation_extractor}"`. On colossal, after `git checkout rust/iron-chancellor && git pull --ff-only`, `$REPO_ROOT/bismark_methylation_extractor` is the *Perl source from the repo*, NOT the binary installed by `micromamba activate bioinf`. The repo's Perl script will always report whatever version is in its `our $extractor_version` line — which on `rust/iron-chancellor` HEAD is v0.25.1 (verified separately).

If colossal has a DIFFERENT Perl binary on PATH (`bismark_methylation_extractor` in `bioinf`), the matrix invokes the repo's script, NOT the micromamba env's. That's actually consistent (smoke uses the same default), but it means the pre-flight version check is *tautological* — it's checking the repo's own checked-out source, not the production binary the user `micromamba activate`d for.

This is a documented design choice (smoke uses `PERL_BIN="${PERL_BIN:-$REPO_ROOT/bismark_methylation_extractor}"` identically) but RELEASE_CHECKLIST.md §pre-matrix-setup explicitly says `bismark_methylation_extractor --version | head -3` and expects micromamba env to provide it — that command-line invocation uses the env's binary, not the repo's. There's an inconsistency: the manual sanity check uses one binary, the matrix uses another. If the two diverge (e.g. dev edits the repo's Perl source post-Phase C.1 to test a hypothesis, forgets to revert), the user's manual eyeball check sees v0.25.1 from env but the matrix sees the dev-modified version. Recommend RELEASE_CHECKLIST.md note `PERL_BIN=$(which bismark_methylation_extractor) bash scripts/phase_h_se_matrix.sh ...` to pin against the env, OR document that the repo's script IS the binary used (and the env's binary is only used as fallback documentation).

**[Medium-E6] Driver doesn't trap SIGINT to leave a partial-verdict marker.**
Per rev 1's edge-case §3.5 "mid-matrix SSH disconnect" — "Driver does NOT trap signals — relies on user's session management." If user Ctrl-C's after 3 cells, `<OUT>/cell_*/` has 3 cell directories but no `matrix_verdict.txt`, no `speedup_table.md`, no `cross_n_summary.txt`. A subsequent re-run on the same `<OUT>` is correctly rejected by pre-flight #2. But the user gets no obvious "interrupted at cell N" marker. Recommend `trap 'echo "INTERRUPTED at cell ..." > "$OUT_DIR/INTERRUPTED.txt"; exit 130' INT TERM`. ~3 LOC.

**[Low-E7] `dcli ssh colossal` is not parameterized; RELEASE_CHECKLIST.md assumes that command works.**
If colossal's hostname/route changes, the checklist breaks. Trivial follow-up; out of scope of #871.

### Structure

**[Medium-S1] 551 LOC vs ~250 LOC plan estimate is over the documented deviation budget.**
Implementation Notes §1 says "the increase is entirely in the table-emission + cross-N aggregation code (~250 LOC), not in the matrix-execution loop itself (~100 LOC)." Looking at the driver:
- Args + pre-flight: ~145 LOC (lines 45-153)
- Matrix execution loop: ~75 LOC (lines 158-227)
- Cross-N section: ~75 LOC (lines 234-301)
- Speedup table + per-N aggregate: ~155 LOC (lines 324-481)
- Matrix verdict + exit: ~70 LOC (lines 483-551)

The speedup-table emission (~155 LOC) and cross-N section (~75 LOC) are the bulk. Both are necessary per rev 1. The speedup table could be ~50-70 LOC shorter by extracting a `ratio_x100_to_string()` helper (defined 4 times inline with subtle variations) and a `format_per_n_row()` helper. ~80 LOC reduction is achievable with minor refactoring. Not blocking, but worth noting for maintainability.

**[Medium-S2] Could this driver have been Python?**
Pros of Python: testable with pytest, cleaner arithmetic (no integer-×100 dance for two decimals), better dict semantics for the cross-N pair tracking (no `IGNORE_PAIR_NS` / `IGNORE_PAIR_SUBDIRS` parallel-array dance with `|`-delimited keys), `subprocess.run` for invoking the smoke script.

Cons: adds a Python runtime dep on colossal — but `micromamba activate bioinf` already provides Python; the bioinf env should have Python ≥3.9 trivially.

The plan's §11 magnet 4 didn't surface this. Reviewer's recommendation: keep bash for v1.0 (already implemented, working, syntax-clean). For v1.1+ matrix expansion (more cells, more flags) consider a Python rewrite to make per-cell logic unit-testable.

**[Low-S3] `EXTRA_FLAGS` variable in matrix driver line 186 shadows `EXTRA_FLAGS` in smoke (line 136).**
The matrix driver builds an inline string `EXTRA_FLAGS="--ignore $I5P --ignore_3prime $I3P"` (line 186) and passes it via `--extra-rust "$EXTRA_FLAGS"`. The smoke then parses it into `EXTRA_RUST` array (line 100). Naming collision is benign (different scopes), but readers tracking the variable can confuse the two. Rename matrix driver's local to `EXTRA_FLAGS_STR` or `IGNORE_FLAGS_STR` for clarity. ~1 LOC.

**[Low-S4] `--out` canonicalize-after-mkdir order means an `--out path-that-exists-and-is-not-a-dir` reaches `mkdir -p` and fails opaquely.**
Actually re-read: line 86-92 explicitly checks `if [[ -d "$OUT_DIR" ]]; else echo "error: --out path exists and is not a directory"`. OK, handled. No action.

**[Low-S5] RELEASE_CHECKLIST.md v1.0 tag step list mixes [ ] checkboxes with prose commands.**
The "v1.0 tag steps" section is good. The "SE matrix" section uses ```bash code blocks for commands but the "Verify:" block uses [ ] checkboxes for expected outcomes. Consistent with checklist style but mixing makes it harder to spot if a step has been done. Cosmetic; no action.

**[Low-S6] PE TODO-stub in RELEASE_CHECKLIST.md doesn't explicitly block v1.0 tag if a release engineer reads ONLY the SE section.**
Line 122: "Both SE matrix (this section, #871) and PE matrix (#872) recorded PASS on epic #798." — this DOES block. But a release engineer skim-reading the SE block alone could think "SE PASS = done". The PE block (lines 110-118) is clear that it's a stub but doesn't have a hard "STOP: do not tag v1.0 if you're reading this." Recommend prepending the PE section with `> **⚠️ v1.0 tag is blocked until this section is populated by #872's PR.**` to make the gate explicit at the place where a reader might mistake it for "PE not applicable yet". ~1 LOC.

---

## Fixes applied

None — all findings are documented as recommendations. The Critical L1 (Rust/Perl column header inversion) and High L2 (M-bias missing-file fail-open) are simple, unambiguous, low-risk fixes that I considered applying directly, but since the matrix driver is a release-gate artifact and the user will exercise it on colossal, I prefer Felix decides whether to:
- L1: flip arithmetic vs. rename column header (semantic choice; flipping arithmetic recommended).
- L2: treat "missing M-bias" as FAIL or as USAGE-error (USAGE may be more accurate since it suggests config drift).

---

## Recommendations summary

| # | Priority | Issue | Estimated fix |
|---|----------|-------|---------------|
| L1 | **Critical** | Speedup-table column header `Rust/Perl` vs computed `Perl/Rust` inversion | 1-2 LOC arithmetic flip + plan §3.3.5 example reconcile |
| L2 | **High** | M-bias missing-file fail-open at baseline guard | 2 LOC: set `MBIAS_BASELINE_OK=0` when file not found |
| E3 | **High** | `uptime` load-average parse fragile across dialects (macOS BSD breaks silently; defensible) | 2-3 LOC guard on empty `$LOAD` |
| L3 | Medium | No per-cell smoke-output log file | ~3 LOC tee |
| L4 | Medium | M-bias row-count differential check from plan §3.4 #4 not implemented | ~15 LOC OR document as deferred |
| L5 | Medium | RELEASE_CHECKLIST.md doesn't surface `cargo build` 10+ min first-run budget | 1 line in checklist |
| L6 | Medium | `tmux`/`screen` not enforced; no pre-flight detection | ~3 LOC `$TMUX`/`$STY` check |
| E4 | Medium | `nproc`-missing fallback `NCORES=4` rejects legitimate high-N runs | ~3 LOC: skip check when nproc missing |
| E5 | Medium | Pre-flight Perl version uses repo's script, not env's binary (inconsistent with checklist's manual sanity check) | Document or align |
| E6 | Medium | Driver doesn't trap SIGINT for partial-verdict marker | ~3 LOC `trap ... INT TERM` |
| S1 | Medium | 551 LOC vs ~250 LOC: speedup-table emission has ~80 LOC of reducible duplication | refactor helpers |
| S2 | Medium | Bash vs Python: defer to v1.1+ matrix-expansion refactor | none for v1.0 |
| L7 | Low | Cross-N raw-`cmp` will spuriously fail on `.gz` outputs (latent if `--gzip` injected) | ~5 LOC: decompress before cmp |
| S3 | Low | `EXTRA_FLAGS` naming collision between matrix driver + smoke | rename ~1 LOC |
| S6 | Low | PE TODO-stub doesn't visually block v1.0 tag | 1 line warning prefix |
| E7 | Low | `dcli ssh colossal` hardcoded in checklist | doc only |

**0 USAGE-only issues; 1 Critical; 2 High; 8 Medium; 4 Low.**

---

## Verdict

**NEEDS-REVISIONS.** The Rust/Perl column inversion (Critical L1) and the M-bias missing-file fail-open (High L2) are correctness bugs in release-gate artifacts. Once those two are fixed (≤5 LOC total), this becomes APPROVE-WITH-NITS — the remaining Medium/Low items are operational polish that can be follow-ups.

The harness design is sound: pre-flight checks are thorough, cross-N N-invariance directly tests Phase F's contract, the 4-way exit-code mapping cleanly separates byte-identity from perf misses, and RELEASE_CHECKLIST.md's escalation paths are well-thought-through. The 551-LOC overshoot is justified by the per-cell + per-N table emission + cross-N aggregation; the documented deviation is honest. 303 tests preserved confirms zero crate-code impact.

Report file: `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/CODE_REVIEW_PHASE_H_SE_B.md`

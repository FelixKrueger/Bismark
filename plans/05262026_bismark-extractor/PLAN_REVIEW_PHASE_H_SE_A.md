# Plan Review — Phase H sub-gate 1 SE (Reviewer A)

**Plan reviewed:** `plans/05262026_bismark-extractor/PHASE_H_SE_PLAN.md` rev 0
**Reviewer:** A (independent of Reviewer B)
**Date:** 2026-05-28
**Verdict:** **NEEDS-REVISIONS** — one CRITICAL gap on SPEC §8.3 row 4 ("Rust N=4 vs N=1 byte-identical"), plus several IMPORTANTs around self-determinism mechanics, wall-clock parsing terminology, and the per-cell N-invariance assertion. Foundation is sound; this is mechanical tightening rather than a redesign.

---

## 1. Logic review

### 1.1 Matrix completeness vs #871

Cross-checked the plan's 8-cell matrix (§3.1) against the matrix specified in #871 body. They match exactly:

| Source | Dimensions | Cells |
|---|---|---|
| #871 body "Test matrix" table | `--parallel ∈ {1, 4 (8?)}` × `--ignore ∈ {0, 5}` × `--ignore_3prime ∈ {0, 5}` | 8 (12 with N=8) |
| Plan §3.1 | identical | 8 (12 with N=8) |

`--mode default` is the implicit fourth axis collapsed to one value. #871 body says explicitly: *"SE × default mode × ignore-flag-matrix is what Phase H proper adds"*, so the mode collapse is correct.

`--mode comprehensive` / `--merge_non_CpG` / `--gzip` / `--yacht` are explicitly listed as out-of-scope in both #871 and plan §1; this is consistent with prior Phase E/G coverage. **PASS — matrix is complete per #871.**

SE-mode-specific dimensions correctly omitted: PE has `--ignore_r2` / `--ignore_3prime_r2` / `--no_overlap` / `--include_overlap`; these are #872's scope and the plan correctly excludes them. **PASS.**

One missing dimension worth checking: directional vs non-directional SE. The plan §3.4 says "6 kept files (CpG/CHG/CHH × OT/OB for SE directional libraries)" — directional is the only SE flavour exercised. §10's table flags this as a default ("non-directional SE would be a separate `mode` value"). Not in the matrix but acknowledged. **Acceptable; flagged below as Optional.**

### 1.2 CRITICAL: SPEC §8.3 row 4 ("Rust N=4 byte-identical to Rust N=1") assertion is MISSING from §3.4

SPEC §8.3 rev 3 enumerates 6 byte-identity invariants. Row 4 (the table in SPEC §8.3 line 730) says:

> **`--multicore 4` byte-identity vs `--multicore 1` Rust output** — Run Rust extractor at N=1 and N=4 on same input; compare each split file with `cmp` (unsorted). The locked invariant from §9 — "any N produces byte-identical output to N=1." **This is the strongest test of the parallelism design** AND covers the worker-reorder regression the rev-2 sorted-md5-only check could have hidden.

The plan's §3.4 enumerates assertions:
1. Strict-byte equality on `*.M-bias.txt` + `*_splitting_report.txt`
2. Sorted-content equivalence on data files (vs Perl)
3. File-set match
4. Self-determinism (Rust ×2 at same parallelism)

**Self-determinism (4) is NOT the same as Rust-N=1-vs-Rust-N=4** (SPEC §8.3 row 4). Self-determinism is "two runs at the same N produce identical bytes"; the row-4 invariant is "Rust at N=1 produces the same bytes as Rust at N=4". The two checks defend different failure modes:

- Self-determinism breaks if a worker's iteration order is non-deterministic (e.g. HashMap-derived).
- Row 4 breaks if the collector's reordering is wrong (e.g. workers stream straight to disk without `input_idx` reordering).

A regression of the BTreeMap-collector (Phase F's load-bearing design choice — see SPEC §9.4) would PASS self-determinism but FAIL row 4. The matrix runs cells at both N=1 and N=4 — they share an `(ignore_5p, ignore_3p)` pair across two N values. **Adding a cross-N raw-byte cmp on the matching pair is a 4-cell-pair check** (4 ignore-flag pairs × 1 cmp = 4 assertions) at near-zero cost beyond what the matrix already runs.

This is the single most-load-bearing missing assertion. The whole point of running both N=1 and N=4 in the matrix should be to catch worker-reorder regressions, but the plan as-written never compares them to each other.

**Action:** Add a 5th assertion to §3.4: "Cross-N raw-byte equality — for each `(ignore_5p, ignore_3p)` pair, `cmp` Rust-N=1 output against Rust-N=4 output, every split file + `*.M-bias.txt` + `*_splitting_report.txt`. Strict equality." This is SPEC §8.3 row 4 (and §9 header). **CRITICAL.**

### 1.3 Self-determinism mechanics underspecified (§3.4 #4 / §5.4.4.e)

§3.4 #4 says: *"The driver invokes Rust TWICE per cell at the same parallelism and asserts."*
§5.4.4.e says: *"Self-determinism check: re-run Rust at the same parallelism + diff the second-run output against the first; assert byte-identical."*

Three problems:

1. **Where does the second run's output go?** The smoke script (§3.2) writes to `<OUT>/perl/` + `<OUT>/rust/` + `<OUT>/diff_summary.txt`. Calling the smoke script a second time into the same `<OUT>` would (a) re-run Perl (doubling runtime — Perl is the slow component), (b) overwrite the first Rust output before diff. Neither matches the intent.
2. **Should the second invocation skip Perl?** The smoke has no `--rust-only` flag today. Either the plan adds one (and the implementation outline §5.3 doesn't mention this addition), or the matrix driver calls the Rust binary directly (bypassing smoke for the second run) — neither is specified.
3. **Pre-flight check conflict**: §3.5 says "If `<OUT>/cell_*/` exists with non-empty contents, abort with USAGE-ERROR". The self-determinism re-run would trip this.

**Action:** Specify the mechanism. Recommended: add a `--rust-only --out-suffix DIR` option to the smoke script OR have the matrix driver invoke `RUST_BIN` directly into `<OUT>/cell_*/rust_rerun/` and `cmp` against `<OUT>/cell_*/rust/`. Update §5.3 + §5.4 to enumerate the smoke-script flag additions. **IMPORTANT.**

### 1.4 Wall-clock parsing — factual error about `time` (§5.4.4.d)

§5.4.4.d says: *"Capture wall-clock for Perl + Rust via `time` parsed from the smoke's `diff_summary.txt` (already emitted by the existing script)."*

Inspected `scripts/oxy_phase_h_smoke.sh:131-153`: the smoke uses `date +%s` arithmetic (integer seconds), NOT `time(1)`. The `diff_summary.txt` emits lines:

```
── Wall-clock ──
Perl: ${PERL_ELAPSED}s
Rust: ${RUST_ELAPSED}s
Speedup: <X>.<Y>× (Perl/Rust)
```

The matrix driver will need to parse `^Perl: \([0-9]\+\)s$` and `^Rust: \([0-9]\+\)s$` regex (or similar). The plan should:

(a) Correct the "via `time`" wording — it's `date +%s`-based.
(b) Specify the parsing regex / awk expression in §5.4.4.d.
(c) Decide whether integer-second precision is sufficient (it is, for cells >2s as the plan acknowledges in §3.5; but for the per-N aggregate "Avg Perl 165 s vs Avg Rust 145 s" the ±0.5s noise per cell becomes ±2s aggregated — still acceptable but worth documenting).

Alternative: upgrade the smoke script to use bash's `$SECONDS` builtin or `printf '%(%s)T' -1` for millisecond resolution. Probably overkill at v1.0.

**Action:** Correct the "via `time`" wording in §5.4.4.d; specify the parsing approach. **IMPORTANT.**

### 1.5 Per-cell wall-clock — double capture or parse? (§3.3.2 vs §5.4.4.d inconsistency)

§3.3.2 says: *"Capture wall-clock around the per-binary invocations (Perl + Rust)"* — implying the driver wraps the smoke invocation in its own timing.
§5.4.4.d says: *"Capture wall-clock for Perl + Rust via `time` parsed from the smoke's `diff_summary.txt`"* — implying parsing from the smoke output.

These are inconsistent. The driver-wrap approach measures Perl + Rust *combined* (since the smoke runs them serially), losing per-binary granularity. The parse-from-smoke approach gets per-binary numbers but at integer-second precision.

§3.3.4's speedup table has separate "Perl (s)" and "Rust (s)" columns, which requires the parse-from-smoke approach.

**Action:** Pick one. The parse-from-smoke approach is necessary for the per-binary columns; remove §3.3.2's implication that the driver does its own timing. **IMPORTANT.**

### 1.6 Bash quoting in §5.4 matrix-loop argv

Mental execution of cell `(N=4, ignore_5p=5, ignore_3p=5)`:

```bash
scripts/phase_h_smoke.sh \
  "$BAM" \
  --parallel 4 \
  --mode default \
  --out "$OUT/cell_p4_i5_i35" \
  --extra-rust "--ignore 5 --ignore_3prime 5" \
  --extra-perl "--ignore 5 --ignore_3prime 5"
```

When the smoke receives `--extra-rust "--ignore 5 --ignore_3prime 5"`, the value is `--ignore 5 --ignore_3prime 5` as a single argv entry (correctly quoted). Inside the smoke, to append this to the Rust invocation as four separate flags, the implementation must word-split. Two safe options:

(a) `EXTRA_RUST_ARRAY=( $EXTRA_RUST )` — unquoted expansion performs word-splitting on IFS (default: space/tab/newline). Safe for ASCII flag-strings; flagged by `shellcheck SC2206`.
(b) `read -r -a EXTRA_RUST_ARRAY <<< "$EXTRA_RUST"` — explicit array read; cleaner.

The plan §4.2 says "passed through verbatim" + §8 A13 says "ASCII bash-literal-safe values" — implying option (a) is acceptable. But the plan doesn't show either approach; reviewer should require the implementation to use one of these (and document `shellcheck` disabled directive if `SC2206`).

**Action:** Add a one-line note in §5.3.1 specifying the word-splitting approach (recommend `read -r -a`). **IMPORTANT.**

### 1.7 Self-determinism per-cell vs per-matrix tradeoff

§3.4 #4 + §11 magnet 4 acknowledge the choice. Per-cell catches "regression that only manifests at, say, N=4 + `--ignore 5`". This is principled — `--ignore`-flag handling has read-coordinate logic that interacts with worker-batch-boundary positions in non-obvious ways. Per-cell is correct.

But §6 says self-determinism check "adds ~30 min total ≈ 2.7 hours". On 8 cells × 1 extra Rust run, this is 8 × Rust-time ≈ 8 × varies-with-N. At N=1 Rust is ~12 min/cell, at N=4 ~5 min/cell — so 4 × 12 + 4 × 5 = 68 min extra, not 30 min. Plan §6 underestimates by ~2×. Not a blocker (it's still feasible) but the matrix-runtime estimate should be corrected.

**Action:** Fix §6's self-determinism overhead estimate. **OPTIONAL.**

### 1.8 §2.5 + §3.5 larger-SE-BAM handling — out-of-scope of the driver CLI

§2.5 mentions a "Larger SE BAM (optional speedup confidence at N=8)" with path TBD. §3.5 says "Run matrix on 10M SE only if larger not present". But §3.3.1's driver CLI takes only ONE `<BAM>` positional — there's no mechanism to run on a second BAM in the same invocation. The plan implicitly assumes "run the driver twice, once per BAM" without saying so.

§4.3 RELEASE_CHECKLIST.md only mentions the 10M SE BAM. So in practice the "larger SE BAM" is aspirational and never actually runs in the SE matrix path. **The plan should clarify**: either drop the larger-BAM mention entirely (cleaner) or specify "driver run twice, once per BAM, into separate `--out` dirs; checklist shows both".

**Action:** Resolve §2.5 + §3.5 — either drop the larger-BAM mention or specify the dual-invocation pattern in the checklist. **IMPORTANT.**

### 1.9 SPEC §10 row H is NOT in plan §5.1 update scope

§5.1 says SPEC §8.3 and §9.7 get updates. SPEC §10's phase table (around line 800-809) currently has one Phase H row: *"H — Real-data byte-identity gate (10M PE WGBS + 55M full) + CHANGELOG + version tag"*. This row predates the Phase H sub-gate split.

After #871 (this) + #872 land, the row should reflect: "Phase H sub-gate 1 SE (#871) + sub-gate 1 PE (#872) + sub-gate 2 deferred to post-#797". Not updating this leaves SPEC §10 inconsistent with SPEC §6.6 rev 3.

**Action:** Add SPEC §10 row H update to §5.1's task list. **IMPORTANT.**

### 1.10 §3.3.5 exit code 3 ordering hazard

Exit code 3 is "byte-identity PASSED but Rust scaling missed". But if both `(any cell FAIL byte-identity)` AND `(perf miss)` are true, which exit code wins? The plan implies "1 wins over 3" (1 is the harder failure) but doesn't say so. Trivial to fix by listing priority: 2 > 1 > 3 > 0.

**Action:** Add exit-code priority ordering to §3.3.5. **OPTIONAL.**

---

## 2. Assumptions

### 2.1 Surfaced assumptions (plan §8.1 + §8.2)

The plan's §8 lists 14 assumptions; I cross-checked each. Most are sound. Notes:

- **A4** (5712 B exact) — locked since Phase C.1, verified via §9.2 regression-guard read. Sound.
- **A5** (Rust N-invariance per Phase F) — sound per SPEC §9; but the matrix as written does NOT actually verify it (see CRITICAL §1.2). This assumption is what §1.2's missing assertion would close.
- **A6** (Perl multicore output is N-dependent) — sound; the existing sorted-content check accommodates this.
- **A7** (Phase C.2 empty-sweep produces 6-file set) — sound; verified.
- **A13** (ASCII pass-through values) — sound for the default matrix; flagged in §1.6 as needing implementation specificity.

### 2.2 Implicit assumptions not in §8 (surface these)

- **AI1.** The smoke script's `--mode default` value is the implicit fourth axis. The plan never says "do NOT pass `--mode comprehensive` to any matrix cell" but the §3.2 invocation template hardcodes `--mode default`. Worth making explicit in §8 or §3.1.
- **AI2.** Perl's `bismark_methylation_extractor` accepts `--ignore N --ignore_3prime M` for SE input *without* requiring `--single_end`. Verified by reading Perl source (called out in #872 referencing line numbers 963/989) but worth a plan-level note.
- **AI3.** The driver assumes `phase_h_smoke.sh` is at a known path (`scripts/phase_h_smoke.sh` relative to repo root). §5.4 doesn't show how the driver locates it — typical bash pattern is `SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"`. Trivial to address.
- **AI4.** `phase_h_smoke.sh`'s existing exit codes are 0/1/2 (per its header). The plan's §3.2 contract reuses this. Confirmed in source.
- **AI5.** Cell-output dirs do not collide across `--parallel-set` iterations: the dir name is `cell_p<N>_i<5p>_i3<3p>` so `N` is in the path. Sound.
- **AI6.** SPEC §6.6 rev 3's `LC_ALL=C` invariant (mentioned in SPEC line 282) — the smoke does NOT currently set `LC_ALL=C` before invoking Perl + Rust. For sub-gate 1 (extractor's own output streams) the bedGraph-pipeline locale doesn't matter, but this would matter if the matrix is ever extended to sub-gate 2. Worth noting that the SE matrix's LC dependence is none — defensive `LC_ALL=C` at the matrix-driver top would future-proof.

**Action:** Surface AI1–AI3 as plan §8.2 additions. **OPTIONAL** (rev 0 is close enough; these are polish).

---

## 3. Efficiency analysis

### 3.1 Matrix runtime

Plan §6 estimates ~2.7 h with self-determinism. Recomputed:

- 4 cells at N=1: Perl ~12 min + Rust ~12 min = 24 min/cell → 96 min for the four N=1 cells
- 4 cells at N=4: Perl ~5.4 min + Rust ~5 min = 10.4 min/cell → 42 min for the four N=4 cells
- Self-determinism (Rust ×2 per cell): adds Rust-only-time per cell = 12+12+12+12+5+5+5+5 = 68 min

Total: 96 + 42 + 68 = **206 min ≈ 3.4 h**, not 2.7 h. The plan estimate is off by ~25%. Not blocking — the workflow is "set it running overnight" — but the §6 number should be corrected.

### 3.2 Disk footprint

§6 estimates ~1.2 GB. Recomputed: 8 cells × (Perl-out + Rust-out + Rust-rerun-out for self-determinism) × ~75 MB per output set = 8 × 3 × 75 MB = **1.8 GB**. Same as plan's "well within colossal" but the 1.2 GB number is also light.

### 3.3 Sequential vs parallel cells

The matrix runs cells serially. At N=1 (Perl single-process + Rust single-thread), the box has unused cores. Could parallelize 2-4 cells concurrently at N=1, halving 96 min → ~25 min. Adds complexity (output-dir uniqueness already handled; binary contention for stdout/stderr would need per-cell logfiles).

**Optional optimization:** add `--cells-in-parallel K` to the driver. Not necessary at v1.0; document as a follow-up.

### 3.4 Speedup table emission

§3.3.4's table is O(cells), millisecond-scale. Fine.

---

## 4. Validation sufficiency

### 4.1 What the validations catch

The proposed validations cover:

- Format-locked byte equality on M-bias + splitting-report (§3.4 #1).
- Content equivalence on data files via sorted-MD5 (§3.4 #2). Catches drift, content bugs.
- File-set drift (§3.4 #3). Catches empty-sweep regressions.
- Self-determinism (§3.4 #4). Catches HashMap-iteration / non-deterministic worker output bugs.
- M-bias baseline byte-count (§3.6). Catches Phase C.1 / C.2 regressions on the default cell.

### 4.2 Gaps

- **G1 (CRITICAL):** No Rust-N=1-vs-Rust-N=4 raw-byte equality assertion (the §9-header / §8.3 row 4 invariant). See §1.2 above. This is the single most important validation gap.
- **G2:** No assertion that the M-bias table row-count *decreases* (or shifts) appropriately under `--ignore 5 --ignore_3prime 5`. The plan acknowledges "smaller-or-larger" but doesn't bound the expected shape. A driver-level sanity check ("with `--ignore 5`, M-bias rows for read positions 1-5 should be missing") would catch a regression where `--ignore` is silently no-op'd. **IMPORTANT.**
- **G3:** No assertion on per-cell call-count totals (from `*_splitting_report.txt`). The splitting-report itself gets cmp'd (strict byte), but a regression where Rust *silently* drops more or fewer calls than Perl under `--ignore 5` would still pass the smoke if both report files happen to byte-match (they would, since both binaries report their own call counts). This is sort of a circular check. A cross-binary call-count diff is what catches this — and the existing sorted-MD5 on data files DOES catch it. So this is covered. **No action.**
- **G4:** No "did Perl and Rust both produce the same set of `--ignore`-filtered positions?" check. The sorted-MD5 covers data files, but each context-file's positional coverage is implicit in the MD5. If both binaries get `--ignore` semantics wrong by the same magnitude, the MD5 would still match. This is a known limitation of byte-identity testing — covered by unit tests in `tests/phase_b_extraction.rs` etc., not the harness. Out of harness scope. **No action.**
- **G5:** The matrix runs entirely on 10M SE. No assertion on a larger BAM. The plan §3.5 acknowledges this and says "degrade gracefully". But the M-bias 5712 B baseline is 10M-specific; a 55M run would expect a different baseline. Plan doesn't define the baseline for the larger BAM (because path is TBD). **IMPORTANT — flagged in §1.8.**
- **G6:** No assertion that the `*.M-bias.txt` row count is consistent between Perl and Rust at the non-default cells (where strict-byte equality is the assertion). I.e. if `--ignore 5` produces a 4500-byte M-bias.txt in both Perl and Rust, the strict cmp covers byte-equality already. **Covered. No action.**

### 4.3 Self-determinism vs N-invariance — both needed, only one specified

Plan §3.4 #4 is self-determinism only. SPEC §8.3 row 4 + §9 invariant requires N-invariance. **Both need to be asserted**:

- Self-determinism: Rust(N=k, run1) == Rust(N=k, run2), byte-identical.
- N-invariance: Rust(N=1) == Rust(N=4), byte-identical.

The matrix runs cells at both N=1 and N=4, so the N-invariance check is essentially free — for each `(ignore_5p, ignore_3p)` pair, compare the matching N=1 cell's Rust output against the matching N=4 cell's Rust output.

**Action:** Add explicit N-invariance assertion. Closes G1. **CRITICAL — same as §1.2.**

### 4.4 What's NOT validated (acceptable but worth flagging)

- `--ignore` semantics correctness at the unit-test level — covered by `tests/phase_b_extraction.rs` / `tests/phase_c_pair_extraction.rs`. Harness assumes unit tests pass.
- Performance regression detection between runs — `speedup_table.md` has absolute numbers but no diff against a stored baseline. Plan §3.3.4 doesn't track "rust scaling delta from last release". Optional v1.1 enhancement.
- M-bias plot rendering — explicitly deferred per SPEC §11. Out of scope.

---

## 5. Alternative approaches

### 5.1 §11 magnet 1 — exit code 3 alternative

The author flagged this. Alternatives:

- **Single-exit-code + flag-file:** Driver writes `<OUT>/perf_target_met.txt` containing `0` or `1`; release-prep tooling reads the file. Avoids exit-code pollution. **Pro:** standard; **Con:** RELEASE_CHECKLIST.md becomes a multi-step check.
- **Single-exit-code + structured `matrix_verdict.txt`:** Already in §4.1 output list. Add a `perf_target_met: yes/no` line; release-prep greps. Equivalent to the flag-file approach, cleaner.
- **Exit code 3 (plan's choice):** Pro: single decision via `$?`. Con: non-standard.

Reasonable trade-off. The plan's choice is defensible. **Recommendation:** Add a parseable line `perf_target_met: yes|no` to `matrix_verdict.txt` AS WELL AS exit code 3, so checklist authors can choose either mechanism. Trivial extension. **OPTIONAL.**

### 5.2 §11 magnet 2 — `byte_identity_smoke.sh` rename

The renaming is purely cosmetic. The plan chose `phase_h_smoke.sh`. **Recommendation:** keep the plan's choice. `phase_h_` is well-understood in the SPEC/plan ecosystem; future phases that need a similar harness can rename if/when. The cost of getting the rename "right" is dominated by other concerns at this point. **No action.**

### 5.3 §11 magnet 3 — `RELEASE_CHECKLIST.md` location

Top-level vs `docs/` vs `rust/`. The plan chose top-level. **Pro:** most-visible. **Con:** clutters root.

Alternative: `docs/RELEASE_CHECKLIST.md` with a top-level `RELEASE.md` stub that points to it. Cleaner-feeling root.

This is a project-convention call. Looking at the Bismark repo root: there's already `Bismark_User_Guide.md`, `CHANGELOG.md`, `CLAUDE.md`, `Docs/`, `Manual/`. A top-level `RELEASE_CHECKLIST.md` fits the pattern (capital-letter top-level docs). **Plan's choice is reasonable. No action.**

### 5.4 §11 magnet 6 — single `--extra` vs `--extra-rust`/`--extra-perl`

Plan picked separate per-binary flags. **Pro:** future-proofs against flag-name divergence. **Con:** verbose in the default-case (where both are identical).

Alternative: `--extra "<shared-flags>"` + `--extra-rust-only "..."` + `--extra-perl-only "..."`. More complex; not warranted.

The plan's choice is correct. **No action.**

### 5.5 Alternative: drop the `oxy_` smoke rename entirely

The smoke rename is part of the PR scope (§5.2). It's housekeeping and could trivially be a separate PR. The argument for combining: this PR touches the smoke anyway (adding `--extra-rust` / `--extra-perl`), so renaming + editing in one commit is atomic. Reasonable.

Alternative: split the rename into a one-line follow-up PR. Costs a separate review cycle; saves one commit. **Not worth it. No action.**

### 5.6 Alternative: per-cell parallelism inside the matrix driver

Sequential cells (plan's choice) makes the wall-clock measurement clean — no inter-cell contention. Parallel cells would invalidate the speedup table's per-cell numbers. Plan's choice is correct. **No action.**

---

## 6. Action items

### Critical

1. **(§1.2 / §4.3)** Add SPEC §8.3 row 4 assertion to plan §3.4 — "Rust-N=1-vs-Rust-N=4 raw-byte equality, per `(ignore_5p, ignore_3p)` pair, every split file + M-bias + splitting-report". This is the single most-load-bearing missing validation in rev 0.

### Important

2. **(§1.3)** Specify the self-determinism re-run mechanism: where the second-run output goes (e.g. `<OUT>/cell_*/rust_rerun/`), how the smoke is invoked (or whether the matrix driver calls `RUST_BIN` directly bypassing smoke), and how the §3.5 pre-flight non-empty-dir check interacts. Update §5.3 / §5.4 to enumerate the necessary smoke-script flag additions if any.

3. **(§1.4)** Correct the "via `time`" wording in §5.4.4.d — the smoke uses `date +%s` arithmetic, integer seconds, format `Perl: ${N}s` / `Rust: ${N}s`. Specify the parsing regex/awk expression.

4. **(§1.5)** Resolve §3.3.2 vs §5.4.4.d inconsistency — pick parse-from-smoke (necessary for per-binary columns) and drop §3.3.2's driver-side timing implication.

5. **(§1.6)** Specify the word-splitting approach inside `phase_h_smoke.sh` for `--extra-rust` / `--extra-perl` pass-through (recommend `read -r -a`, document the choice). Reference in §5.3.1.

6. **(§1.8)** Resolve §2.5 + §3.5's "larger SE BAM" mention — either drop or specify the dual-invocation pattern (driver run twice, both runs documented in RELEASE_CHECKLIST.md). Currently the driver CLI takes one BAM and the larger-BAM dimension is unreachable.

7. **(§1.9)** Add SPEC §10 row H update to §5.1's task list. Sub-gate split must be reflected in the phase table.

8. **(§4.2 G2)** Add a driver-level sanity check that with `--ignore 5 --ignore_3prime 5`, the M-bias.txt row count is *smaller* than the default-cell baseline by 10 rows (5 5'-positions + 5 3'-positions per read identity). Catches silent-no-op regressions on `--ignore`-flag semantics. Or assert the row-count differential explicitly.

9. **(§3.1)** Correct the §6 runtime estimate (~3.4 h, not ~2.7 h) and the §6 disk-footprint estimate (~1.8 GB, not ~1.2 GB).

### Optional

10. **(§1.10)** Add exit-code priority ordering to §3.3.5: `2 > 1 > 3 > 0`. (Trivial; surface to avoid implementation ambiguity.)

11. **(§2.2 AI1–AI3)** Surface implicit assumptions in §8.2: `--mode default` is hardcoded; Perl SE input does not require `--single_end`; matrix driver finds smoke via `SCRIPT_DIR`.

12. **(§5.1)** Add a parseable `perf_target_met: yes|no` line to `matrix_verdict.txt` alongside exit code 3, so RELEASE_CHECKLIST.md can use either mechanism.

13. **(§3.3)** Document the sequential-cells choice in §6 and acknowledge "parallel cells" as a possible v1.1 driver enhancement. (Avoids future "why is this serial?" question.)

14. **(§4.3)** Consider stretching the matrix with one over-length cell (`--ignore 250`) as an explicit row, per §10 default. The 9th cell catches a class of bug that 8 cells doesn't. Author's argument for omitting (unit-test territory) is reasonable but the smoke can be the integration check.

15. **(§AI6)** Defensive `LC_ALL=C` set at the matrix-driver top — costs nothing, future-proofs against sub-gate-2 expansion.

16. **(§5.5)** RELEASE_CHECKLIST.md template — show the actual `bash` command lines with absolute paths users would copy-paste. The §4.3 skeleton has the right shape but lacks copy-pasteable commands (the user has to substitute `<SE_BAM>` etc.).

17. **(non-critical)** Add a "phase H matrix completed (date: ...)" entry plan for `CHANGELOG.md` so the release narrative is visible. Plan §5 doesn't mention CHANGELOG.

---

## 7. Verdict

**NEEDS-REVISIONS.**

The plan is well-structured, scope-correct, and faithful to #871's stated matrix. The author's `§11` self-flagged magnets show good self-awareness. The implementation is bash + markdown — straightforward.

The one **CRITICAL** finding (action item #1: missing Rust-N=1-vs-Rust-N=4 raw-byte assertion) is the SPEC §8.3 row-4 invariant. This is the load-bearing test of Phase F's BTreeMap-collector design. Running cells at both N=1 and N=4 in the matrix but never comparing them to each other is wasted signal. Self-determinism (which the plan does specify) is a strictly weaker check.

The Important findings are mostly mechanical tightening — wall-clock parsing terminology, self-determinism re-run plumbing, SPEC §10 update scope, larger-BAM CLI handling. None of these change the plan's goal; all are addressable in a rev 1 in ~1 hour.

Implementation as-written would likely PASS on colossal (since Phase F's invariants are already verified by other tests) but the **harness would not catch a future regression** of those invariants — defeating the harness's purpose. Action item #1 is the difference between "harness verifies what we claim" and "harness happens to pass today".

Recommend rev 1 addressing Criticals 1 + Importants 2-9; verdict on rev 1 will likely be APPROVE-WITH-NITS.

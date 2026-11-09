# Plan Coverage Report

**Mode:** B — code vs. plan (post-implementation; `PLAN.md` rev 1 is the spec, no separate `IMPL.md`)
**Plan(s):** `plans/07292026_local-mapq-denominator/PLAN.md` (rev 1)
**Codebase:** `/Users/fkrueger/Github/Bismark`, branch `rust/local-mapq-denominator`, uncommitted working tree
**Date:** 2026-07-29
**Verdict:** **INCOMPLETE — 6 items unresolved**

## Summary

- Total items: 54
- DONE: 46
- PARTIAL: 4
- MISSING: 1
- DEVIATED: 3 (2 documented in §12, 1 undocumented)

Unresolved = 4 PARTIAL + 1 MISSING + 1 undocumented DEVIATED. None of the six is a
correctness defect in the fix itself; they are missing validation cells and unfinished
release/tracking chores. The core semantics (D1 + D2), the refactor, and all four
hand-derived HISAT2 expectations were independently re-derived and confirmed correct.

**Build gates verified, not assumed:**

| Gate | Claimed | Measured |
|---|---|---|
| `cargo fmt -p bismark -- --check` | clean | **clean** (exit 0) |
| `cargo clippy -p bismark --all-targets` | 0 warnings | **0 warnings, 0 errors** |
| `cargo test -p bismark` | 2107 pass / 0 fail | **2107 passed / 0 failed / 20 ignored** across 77 test binaries |

---

## Coverage ledger

### §5 Implementation outline — PR 1 (mechanical refactor)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | `ScoreMinForm` + `ScoreModel` added to `config.rs`; `score_min_params` returns the emitted form | Step 1 (end state) | DONE | `config.rs:68-74` (`ScoreMinForm`), `:87-95` (`ScoreModel`, all 5 fields private), `options.rs:376` returns `(f64, f64, ScoreMinForm)` |
| 2 | PR 1's constructor must be verbatim-behaviour (`match_bonus = 0.0` everywhere, HISAT2-local still `Log`) so D2 does not land inside the inert step | Step 1 warning box | DEVIATED (documented) | §12 records the resolution: PR 1 left `score_min_params` untouched and set `form = if local { Log } else { Linear }`; `match_bonus`/`perfect()`/`BOWTIE2_LOCAL_MATCH_BONUS` held to PR 2 to avoid `dead_code` under `-D warnings`. Not independently re-verifiable — both PRs sit in one uncommitted tree with no PR-1 commit |
| 3 | Thread `ScoreModel` in place of the trio at every site, by value; drop 2 `too_many_arguments` allows, keep the PE one | Step 2 | DONE | Verified counts: **2** `merge.rs` signatures (`check_results_single_end`, `check_results_paired_end`), **8** `combined.rs` signatures (`select`, `select_core`, `select_nondir`, `select_pbat`, `select_pe`, `select_pe_nondir`, `select_pe_pbat`, `select_core_pe`), **2** fn-pointer aliases (`SelectFn` `mod.rs:2757`, `SelectFnPe` `mod.rs:2766`), **6** `mod.rs` config-expansion sites (2604, 3138, 3516, 4633, 5235, 5676), `RunConfig.score_model` + its literal, **4** production `calc_mapq` call sites (`merge.rs:367,740`; `combined.rs:339,711`). Passed by value (`score_model: ScoreModel`). Allows deleted at `combined.rs` `select_pe_nondir` and `merge.rs` SE; PE allow retained (`merge.rs:514`). Trio fully retired from `RunConfig` — only local variable names survive inside `config::resolve` |

### §5 Implementation outline — PR 2 (semantics)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 4 | **D1** — `match_bonus > 0.0` dispatch via `model.normalize(...)`; `BOWTIE2_LOCAL_MATCH_BONUS` set for Bowtie 2 + local; stale `// scores vary by up to this much (max AS = 0)` comment rewritten; `float_cmp` allow still holds | Step 4 | DONE | `config.rs:191-208` `normalize()` is character-for-character the plan's §3.4 snippet; `config.rs:78` const `= 2.0`; stale comment gone; `#[allow(clippy::float_cmp)]` retained with its justification on all three ladder fns |
| 5 | **D2** — `form` straight from `score_min_params` ⇒ HISAT2-local `Linear`; the four affected expectations hand-recomputed | Step 5 | DONE | All four present and **independently re-derived by this audit** against `calc_mapq_local` (`mapq.rs:137-214`): `-1/None` 22→**44** (bestOver 9 ≥ 0.8·10); `0/Some(-1)` 40→**31** (bestDiff 1 ≥ 0.1·10, bestOver==diff); `-1/Some(-1)` 0→**1** (bestDiff 0, bestOver 9 ≥ 0.5·10); PE `150+150 0/Some(-1)` 34→**11** (bestDiff 1 < 0.1·60, bestOver 60 ≥ 30). Test renamed `local_hisat2_uses_the_linear_form_it_was_emitted` |
| 6 | Retire the two tautologies (`mapq.rs:376` and `:416` — both computed `expect` by calling the function under test with `sc.abs()`) | Step 6 | DONE | `:376`'s host test replaced by `local_bowtie2_matches_independent_reference`; `:416`'s routing loop deleted and replaced with linear-form assertions. No `sc.abs()` self-comparison remains |
| 7 | Re-home the `ln()`-boundary coverage as a **Bowtie 2-local** case with the same "why this cell" comment style | Step 7 | DONE | `local_bowtie2_ln_derived_bucket_boundary` (len 25, `G,20,8`): re-derived here as scMin 45.7511, perfect 50, diff 4.2489 → AS 50 lands `bestOver == diff` → 44; AS 48 → ratio 0.5293 → rung 36, boundary at 2.1237. Comments name the boundary and its margin |
| 8 | Docs — `mapq.rs` header + `config.rs` `RunConfig` field doc no longer describe `local` as "`ln()` scMin + local ladder" | Step 8 | DONE | `mapq.rs:1-17` gained the deliberate-deviation paragraph; `config.rs:366-370` rewritten to describe the score model |
| 9 | Docs — `CHANGELOG.md` under a new `## Unreleased` heading | Step 8 | **PARTIAL** | Present with: Bowtie 2-local now matches Bowtie 2 ✓, local values differ from 3.1.0/v0.25.1 ✓, end-to-end + HISAT2-local unaffected by D1 ✓, HISAT2 perfect score still open ✓, @9xg credited ✓, minimap2/rammap noted ✓. **Missing: O4's note** — that the `bestOver == diff` rungs now mean `AS == perfect`, which is why 39/35/34/… start appearing |
| 10 | Docs — `rust/README.md` aligner **row** + dated Milestones line | Step 8 | **PARTIAL** | Milestones line added (`README.md:181`, dated 2026-07-29). The `bismark` (aligner) **row** in the per-tool table (`README.md:161`) is untouched — required both by the plan and by the README's own rule at `README.md:175` |
| 11 | File the two deferred issues (HISAT2 perfect score carrying O3's reasoning; minimap2/rammap positive-AS per A9) | Step 9 | **MISSING** | §12 discloses "not yet filed". Both are referenced from the code and CHANGELOG as "tracked separately", so the references currently point at nothing |
| 12 | Release cut is NOT part of either PR — all three version literals stay at `3.1.0`, both guard tests pass | Step 10 | DONE | `rust/VERSION`, `rust/bismark/VERSION`, `rust/bismark/Cargo.toml:3` all `3.1.0` and absent from `git diff`; `vendored_version_matches_repo_version` + `cargo_pkg_version_matches_suite_version` green inside the 2107 |

### §9 Validation

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 13 | **V1** — end-to-end + HISAT2-local denominators frozen; sweep `len ∈ 1..=500` × 6 `--score_min` × `as_best` grid × `{None, Some}` × SE **and** PE against a frozen reference fn | V1 | **DEVIATED (undocumented)** | `frozen_paths_match_the_pre_fix_formula` implements the frozen reference verbatim and covers **all 6** `--score_min` cells, `as_best ∈ {0,-1,-5,-20,5}`, `sec ∈ {None, Some(-1), Some(-7)}`, SE + PE, plus the HISAT2-local `abs(scMin)` denominator. But the `len` axis is an **11-point sample** `[1,2,3,4,5,6,20,50,100,250,500]`, not `1..=500`. Not recorded as a deviation in §12 |
| 14 | **V2** — D1 vs an independent reference: the **f64 analogue** of `unique.h:206-222`, commented as such, gridded over `len × AS × second-best` | V2 | DONE | `bowtie2_local_reference` (`mapq.rs:417-430`) transcribes `scPer` / `max(1, scPer − scMin)` with Bismark's `f64` scMin and says so; grid 5 lens × 5 AS × 3 second-best + 3 PE pairs. Kills the replaced tautology |
| 15 | **V3** — reporter's case #1: `len=100, G,20,8, AS=100`, no second best ⇒ **24** (was 42) | V3 | DONE | Re-derived here: scMin 56.8414, perfect 200, diff 143.1586, ratio 0.3015 → 24; old ratio 0.759 → 42. Asserted in `local_bowtie2_denominator_uses_perfect_score_issue_1079` |
| 16 | **V4** — PE sums both mates: `diff = 2×200 − scMin(both)` and differs from the SE-only value | V4 | DONE | Same test: `assert_eq!(diff, 400.0 - sc)` where `sc = 2·(20+8·ln 100)`, plus `assert_ne!(diff, se_diff)` |
| 17 | **V5** — D2: `scMin == −0.2·len` **and the converse** `!= −0.2·ln(len)`; emitted option still `L,0,-0.2`; the four recomputed values | V5 | DONE | `linear_diff == 10.0` + `assert_ne!(linear_diff, (i + s·ln 50).abs())`; `score_min_params_aligner_and_mode_defaults` pins `(0.0,-0.2,Linear)` for HISAT2-local and `(20.0,8.0,Log)` for Bowtie 2-local; the emitted **string** `L,0,-0.2` is pinned by the pre-existing `hisat2_local_option_string` (`options.rs:634`), still green |
| 18 | **V6** — the clamp: Bowtie 2-local `diff == 1.0`, **and assert which rung results** (risk is a silent top rung, not a panic); plus end-to-end `scMin == 0` still gives `diff == 0` | V6 | **PARTIAL** | `local_diff_is_clamped_to_at_least_one` asserts `diff == 1.0` (len 6, `G,20,8`) and the frozen `e2e == 0.0`. It **never calls `calc_mapq`**, so the "which rung" half — the whole point, given the plan's own finding that a degenerate `diff` fails by silently winning the top rung — is unasserted |
| 19 | **V7** — PR 1 is a true no-op: green with **no expected-value edits**, call-shape edits only | V7 | DONE | §12 records a proof-by-inverse-transform (whitespace-normalized test modules byte-identical to `HEAD`). Independently confirmed from the full `git diff`: every `merge.rs` / `combined.rs` / `mod.rs` test edit is exactly `0.0, -0.2, false` → `ScoreModel::end_to_end(0.0, -0.2)`, and every end-to-end expected value in `mapq.rs` (42/40/24/23/8/3/0, 39/38/35, 1/26/33, 42, 42/40) is unchanged. PR 1 cannot be re-run in isolation (single uncommitted tree) |
| 20 | **V8** — real Bowtie 2 oracle on a fixture, restricted comparison set, every disagreement explained by int-vs-float | V8 | DEVIATED (documented) | Not done. §12 says so explicitly and names V2 as the standing gate — which is exactly the escape §9 V8 authorizes ("if the restricted set can't be built reliably, say so and rely on V2 — do not quietly drop it") |
| 21 | **V9** — wiring at unit level: the `(local, aligner)` → model matrix, incl. minimap2/rammap + local unreachable | V9 | DONE | `score_model_construction_matrix` pins Bowtie 2-local (`Log`, `diff == perfect − scMin` ⇒ `match_bonus == 2.0`, local ladder), HISAT2-local (`Linear`, `diff == 20.0` = `abs(scMin)`, local ladder), and all **4** aligners end-to-end (`!local_ladder`, `diff == 20.0`, equal to the `end_to_end` ctor). The unreachable arm is covered by the pre-existing `resolve_local_aligner_scope`, `rammap_rejects_local`, `resolve_rejects_local_with_combined_index` |
| 22 | **V10** — wiring end-to-end: a local MAPQ value out of a real BAM, **two cells** — (a) a ≥23 bp read at default `G,20,8` (len 25 → scMin 45.75, perfect 50); (b) the 6 bp fixture with `--score_min G,1,0` | V10 | **PARTIAL** | Only cell **(b)** built: `make_fake_bowtie2_local_partial_score` (`AS:i:6`) + `bowtie2_local_mapq_uses_the_perfect_score_denominator_end_to_end`. Re-derived here: scMin 1, perfect 12, diff 11, bestOver 5, ratio 0.4545 → **28**; old `abs(scMin)=1` → ratio 5 → **44**. It discriminates, and the failure message names the cause. Cell **(a)** — the realistic-parameters cell — is absent, and §12 does not record dropping it |
| 23 | **V11** — reporter's case #2: Bowtie 2-local perfect alignment `len=100, AS=200` ⇒ `best_over == diff == 143.1586`; 44 with no second best; the `==diff` leaves reachable with a second best | V11 | DONE | `44` asserted for the SE perfect alignment. The `best_over == diff` equality is asserted for the PE 100+100 analogue (`assert_eq!(bo, 400.0 - sc)`) and at len 25 (`assert_eq!(bo, diff)`) rather than for the SE len-100 cell itself. The `==diff` leaves are exercised inside V2's grid — e.g. len 25 / `as_best` 50 / `sec` 49 gives `bestDiff 1 ≥ 0.2·diff` with `bestOver == diff` → leaf 32 (`mapq.rs:193`) |

### §3 Behavior

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 24 | `score_min_params` returns its selected form; `ScoreModel` built **once** in `config::resolve` from form + aligner + `cli.local`; replaces the three scalars on `RunConfig` | §3.1 | DONE | Single construction at `config.rs:771-777`, after all aligner overrides/guards |
| 25 | `scMin = intercept + slope × f(len)`, `f = ln` iff `form == Log`, summed over mates, **derived from the emitted form, never re-decided** | §3.2 | DONE | `ScoreModel::score_min` (`config.rs:158-172`); no `local`-based form decision anywhere downstream |
| 26 | `perfect = match_bonus × (len1 + len2.unwrap_or(0))`; `match_bonus = 2.0` only for Bowtie 2 + local | §3.3 | DONE | `ScoreModel::perfect` (`config.rs:175-177`); `from_emitted` gates on `local && aligner == Aligner::Bowtie2` |
| 27 | Denominator gated on `match_bonus > 0.0`, **not** on `local` | §3.4 | DONE | `normalize` matches the plan's snippet exactly, including `.max(1.0)` |
| 28 | `bestOver = as_best − scMin` **unchanged** | §3.5 | DONE | `(as_best as f64 - sc_min, diff)`; structurally coupled to `scMin` by returning the pair together |
| 29 | Ladder selected by `local_ladder`; both ladders' thresholds and return values untouched | §3.6 | DONE | Diff shows no threshold or return-value edit in either ladder. The end-to-end branch was extracted into `calc_mapq_end_to_end` (§12 documents why: the frozen reference must call the real ladder; the alternative was a forbidden `unreachable!()` stub) |

### §3 Edge cases

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 30 | End-to-end / HISAT2-local invariance byte-identical **by construction** | Edge cases | DONE | The `match_bonus == 0.0` arm is the unmodified `sc_min.abs()`; V1 is the regression net |
| 31 | Non-default `--score_min` | Edge cases | DONE | V1's 6-cell `--score_min` axis, including `(10,−0.2)` and `(0,0)` |
| 32 | `len = 0` | Edge cases | DONE | Plan states this is invariant "by reasoning, not by assertion" — no code or test commitment. V1's `len` axis starting at 1 is consistent with that |
| 33 | `diff` clamp `max(1.0)` per Bowtie 2 | Edge cases | DONE | Present in `normalize`; the rung-assertion shortfall is tracked under item 18 |
| 34 | `diff == 0` failure shape — no division, so a zero `diff` silently wins the top rung | Edge cases | DONE | Confirmed: neither ladder divides; every comparison is `best_over >= diff * k` |
| 35 | Bowtie 2-local `scMin` sign guaranteed positive (`bt2_search.cpp:1861`) | Edge cases | DONE | Rationale only — no code commitment; nothing to implement |
| 36 | PE perfect sums both mates `2·(l1+l2)` | Edge cases | DONE | See item 16 |
| 37 | `--combined_index` (all 4 variants): `--local` rejected ⇒ all 8 `combined.rs` sites are refactor-only | Edge cases | DONE | Rejection intact (`config.rs:667`) and tested; all 8 `combined.rs` diffs are signature/call-shape only |
| 38 | 5-Base never calls `calc_mapq` | Edge cases | DONE | Verified: no `calc_mapq` or `score_model` reference in `five_base_deconv.rs` / `five_base_duplex.rs`. `--five_base_min_mapq` (`mod.rs:571,1465`) untouched |

### §8 Assumptions — does the code contradict any?

| # | Item | Status | Notes |
|---|------|--------|-------|
| 39 | A1 Bowtie 2-local perfect = `2 × len` per mate, summed for PE | DONE | `perfect()` |
| 40 | A2 `G` is logarithmic ⇒ Bowtie 2-local `ln()` is correct and kept | DONE | `ScoreMinForm::Log` retained for Bowtie 2-local only |
| 41 | A3 HISAT2-local perfect = `0` pending investigation | DONE | `match_bonus = 0.0`; the code comment says "deliberately 0 pending #1079" |
| 42 | A4 `--ma` unreachable ⇒ `2.0` is a constant | DONE | `BOWTIE2_LOCAL_MATCH_BONUS` with the "a future `--ma` passthrough would have to feed this" note |
| 43 | A5 both ladders correct and untouched | DONE | No threshold/return edits |
| 44 | A6 `bestOver = as_best − scMin` unchanged | DONE | Enforced structurally by `normalize` |
| 45 | A7 local mode has no perl-oracle cell (and no aligner cell at all) | DONE | `rust_ci.yml` unchanged — no `--local` cell added, per §10's "Noted" |
| 46 | A8 read length == the length Bowtie 2 scored | DONE | All 4 call sites still pass `sequence.len()` / `sequence_2.len()`; unchanged |
| 47 | A9 minimap2/rammap `match_bonus = 0.0` is a **freeze, not a verified value** | DONE | Recorded in the `from_emitted` comment, the CHANGELOG's third bullet, and V9's matrix. (The issue that should carry it is item 11) |
| 48 | A10 `scMin`/`diff`/`bestOver` stay `f64` | DONE | No truncation introduced anywhere |
| 49 | A11 `--score_min` may be non-default and is numerically unvalidated | DONE | `options.rs` validation still shape-only; V1 exercises the consequence |
| 50 | (A-set) no assumption contradicted by the implementation | DONE | — |

### §1 Explicit non-goals — genuinely left alone?

| # | Item | Status | Notes |
|---|------|--------|-------|
| 51 | HISAT2 perfect score stays `0` — **not** silently implemented | DONE | `match_bonus` is 0 for HISAT2 in both `from_emitted` and `hisat2_local`; V9 pins `diff == abs(scMin)` there |
| 52 | Bowtie 2's `int64` truncation **not** adopted | DONE | All arithmetic `f64`; the reference fn explicitly documents the accepted divergence |
| 53 | minimap2/rammap `perfect = 0` left frozen | DONE | `match_bonus` 0 and `--local` still rejected for both |

### §4 Signature

| # | Item | Status | Notes |
|---|------|--------|-------|
| 54 | `ScoreMinForm`, `ScoreModel` (private fields), `BOWTIE2_LOCAL_MATCH_BONUS`, `from_emitted`, the 3 named constructors, `score_min`, `perfect`, `normalize`, and `calc_mapq(.., model)` | DONE | All present with the specified derives and privacy. Two necessary additions the sketch omitted: `normalize` is `pub` (it is consumed from a sibling module, so module-private is impossible) and a `local_ladder()` accessor exists (required by the private-fields design). Neither weakens the invariant the private fields protect |

---

## Gaps (detail)

### Item 9: CHANGELOG missing O4's note

**Expected (step 8):** "Add O4's note (the `bestOver == diff` rungs now mean `AS == perfect`, which is why 39/35/34/… start appearing)."
**Found:** The `## Unreleased` entry covers every other required point — the normalization change, the saturation symptom with the 250 bp example, `max(1, perfect − score_min)`, SE+PE, the explicit "values differ from 3.1.0 and Perl v0.25.1", end-to-end/HISAT2-local unaffected by D1, HISAT2's perfect score still open, minimap2/rammap noted, @9xg credited. There is no mention of the `==diff` rungs or of new MAPQ values appearing in local output.
**Gap:** One sentence. It matters because users diffing local BAMs will see MAPQ values (39/35/34/33/32/31) that literally never occurred before — those leaves were unreachable while `diff = abs(scMin)`.

**Related observation (not a plan gap):** as written the entry reads as self-contradictory — bullet 1 ends "…stays byte-identical …, as does HISAT2 `--local`", and bullet 2 opens by saying HISAT2-local MAPQ changes. Bullet 1 means "unaffected *by D1*", which is what the plan asked for, but a reader hitting bullet 1 first will conclude the opposite of bullet 2.

### Item 10: `rust/README.md` aligner row not updated

**Expected (step 8):** "`rust/README.md`: aligner row + dated Milestones line — its own rule (L175) is **per module-merge PR**, so this belongs in PR 2."
**Found:** Only the Milestones line (`README.md:181`). The `bismark` (aligner) row in the per-tool status table (`README.md:161`) is byte-unchanged; `git diff --stat` shows `rust/README.md | 1 +`.
**Gap:** Add the `--local` MAPQ change to the aligner row. The row currently claims the aligner is "byte-identical to Perl v0.25.1 + Bowtie 2 2.5.5" with no local-mode carve-out, which is now false for `--local`.

### Item 11: The two deferred issues are not filed

**Expected (step 9):** File (a) HISAT2 perfect score / correct ladder, carrying O3's reasoning that HISAT2 has no `--local` mode and monotone scoring, so `0` is likely genuinely correct rather than provisional; and (b) minimap2/rammap positive-AS ⇒ top-rung MAPQ (A9), the same defect class in a default path.
**Found:** Nothing filed. §12 discloses "not yet filed".
**Gap:** File both. Three places now promise a tracking issue that does not exist: `config.rs`'s "deliberately 0 pending #1079" comment, the CHANGELOG's "that is tracked separately", and `mapq.rs`'s "pending that follow-up". Without the issues, O3's reasoning — the reason A3 is probably *correct* and not a stopgap — lives only in this plan file and will be re-derived from scratch next time.

### Item 13 (V1): `len` axis is sampled, not swept

**Expected:** `len ∈ 1..=500`.
**Found:** `[1, 2, 3, 4, 5, 6, 20, 50, 100, 250, 500]` — 11 points.
**Gap:** Either widen to `1..=500` (the test is pure arithmetic; 6 × 500 × 5 × 3 × 2 ≈ 90k cells still runs in milliseconds) or record the sampling as a deliberate deviation in §12. Low risk as it stands: every length that matters for the `abs(scMin) == −scMin` boundary is included (1-6 for the default slope, 20 for the `−0.05` cell), and §12 confirms the injected-fault check failed at `len=1`. But the plan specified a sweep, and an 11-point sample is not one.

### Item 18 (V6): the clamped case's rung is never asserted

**Expected:** "Bowtie 2-local `perfect − scMin ∈ [0,1)` … ⇒ `diff == 1.0`, **and assert which rung results** (risk is a silent top rung, not a panic)."
**Found:** `local_diff_is_clamped_to_at_least_one` asserts `diff == 1.0` and the frozen end-to-end `diff == 0.0`. It calls `normalize` only — never `calc_mapq`, so no MAPQ value is pinned for a clamped `diff`.
**Gap:** Add a `calc_mapq` assertion for the clamped cell. This is the one gate whose *stated rationale* is the failure mode the plan identified as silent (a degenerate `diff` collapsing every threshold to `>= 0` so the top rung wins). Asserting only `diff == 1.0` verifies the clamp fired, not what the ladder then does with it. Concretely: `calc_mapq(6, None, 0, None, bowtie2_local(20.0, 8.0))` — `bestOver = 0 − 34.33 = −34.33`, `diff = 1.0`, so every rung fails and the no-second-best floor 22 results.

### Item 22 (V10): cell (a) not built

**Expected:** Two cells — (a) a **≥23 bp** read with default `G,20,8` (len 25 → scMin 45.7527, perfect 50, diff 4.2473); (b) the existing 6 bp fixture with `--score_min G,1,0`.
**Found:** Only (b).
**Gap:** Build cell (a), or record dropping it. The two cells are not redundant: (b) proves the wiring works but only under a hand-picked non-default `--score_min` on a 6 bp read that real Bowtie 2 could never report; (a) is the only end-to-end cell that would exercise the **default** local parameters at a realistic read length — the configuration real users run. §12 discusses which fixture route discriminates but never says cell (a) was dropped, so this reads as an omission rather than a decision.

---

## Test verification

| Test | File | Maps to | Status |
|---|---|---|---|
| `frozen_paths_match_the_pre_fix_formula` | `src/aligner/mapq.rs` | V1 | PASS (len axis sampled — item 13) |
| `bowtie2_local_reference` (helper) + `local_bowtie2_matches_independent_reference` | `src/aligner/mapq.rs` | V2, V11 `==diff` leaves | PASS |
| `local_bowtie2_denominator_uses_perfect_score_issue_1079` | `src/aligner/mapq.rs` | V3, V4, V11 | PASS |
| `local_hisat2_uses_the_linear_form_it_was_emitted` | `src/aligner/mapq.rs` | V5, step 5 | PASS (4 values re-derived independently) |
| `score_min_params_aligner_and_mode_defaults` | `src/aligner/options.rs` | V5 form plumbing | PASS |
| `hisat2_local_option_string` (pre-existing, unchanged) | `src/aligner/options.rs` | V5 emitted-option cross-check | PASS |
| `local_diff_is_clamped_to_at_least_one` | `src/aligner/mapq.rs` | V6 | PASS (rung unasserted — item 18) |
| `score_model_construction_matrix` | `src/aligner/mapq.rs` | V9 | PASS |
| `resolve_local_aligner_scope`, `rammap_rejects_local`, `resolve_rejects_local_with_combined_index` (pre-existing) | `src/aligner/config.rs` | V9 unreachable arm | PASS |
| `local_bowtie2_ln_derived_bucket_boundary` | `src/aligner/mapq.rs` | step 7 | PASS |
| `bowtie2_local_mapq_uses_the_perfect_score_denominator_end_to_end` | `tests/aligner_cli.rs` | V10 cell (b) | PASS |
| V10 cell (a) — ≥23 bp read at default `G,20,8`, out of a BAM | — | V10 cell (a) | **MISSING** |
| V8 — real Bowtie 2 oracle | — | V8 | **MISSING** (documented decision) |
| `vendored_version_matches_repo_version`, `cargo_pkg_version_matches_suite_version` | `src/meta/mod.rs` | step 10 | PASS |
| `local_no_second_best_ladder`, `local_second_best_ladder`, `inner_threshold_leaves_pinned`, `no_second_best_ladder`, `with_second_best_top_buckets`, `with_second_best_not_at_diff`, `non_integer_scmin`, `user_score_min_slope` | `src/aligner/mapq.rs` | frozen-path regressions (V7) | PASS, expected values unchanged |
| Whole crate | `-p bismark` | — | **2107 passed / 0 failed / 20 ignored** |

---

## Verdict

**INCOMPLETE — 6 items unresolved.** The fix itself is complete and correct: D1 and D2 are implemented exactly as §3 specifies, the refactor hit every site the plan enumerated with no expected-value drift, and every hand-derived expectation in the plan (the four HISAT2 values, V3's 24, V10's 28, the len-25 boundary 44/36) was independently re-derived in this audit and matched. What remains is three validation cells and three release/tracking chores:

1. **Item 9 — CHANGELOG:** add O4's note (the `bestOver == diff` rungs now mean `AS == perfect`, which is why MAPQ 39/35/34/33/32/31 start appearing in local output). Optionally reconcile bullet 1's "as does HISAT2 `--local`" with bullet 2, which says HISAT2-local changes.
2. **Item 10 — `rust/README.md`:** update the `bismark` (aligner) **row** (`README.md:161`), not only the Milestones line. Its unqualified "byte-identical to Perl v0.25.1 + Bowtie 2 2.5.5" is now false for `--local`.
3. **Item 11 — file both deferred issues:** HISAT2 perfect score/ladder (carrying O3's "HISAT2 has no `--local` mode ⇒ 0 is probably genuinely correct" reasoning) and minimap2/rammap positive-AS (A9). Three code/doc comments already promise them.
4. **Item 13 (V1) — widen the `len` axis** to `1..=500` as specified, or record the 11-point sample in §12 as a deliberate deviation.
5. **Item 18 (V6) — assert which rung results when `diff` is clamped.** e.g. `calc_mapq(6, None, 0, None, ScoreModel::bowtie2_local(20.0, 8.0))` → 22. Asserting `diff == 1.0` alone does not cover the silent-top-rung failure this gate exists to catch.
6. **Item 22 (V10) — build cell (a)** (a ≥23 bp read at the default `G,20,8`, MAPQ read out of a BAM), or record dropping it. Cell (b) proves the wiring under a hand-picked non-default `--score_min` on a read real Bowtie 2 could not report; cell (a) is the only end-to-end coverage of the default local configuration.

Items 1-3 are release/tracking work; items 4-6 are test work. None blocks the semantics, and nothing found here suggests the implemented behaviour is wrong.

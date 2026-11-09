# Plan Coverage Report

**Mode:** B (code vs. plan — the design plan's §7 "Implementation outline" + §9 "Validation" serve as the implementation spec; §3.5 and §8 audited where they imply a code or test artifact)
**Plan(s):** `plans/08012026_minimap2-mapq-denominator/PLAN.md` (rev 2), `SPIKE.md`
**Codebase:** `/Users/fkrueger/Github/Bismark`, branch `plan/mapq-minimap2-denominator` @ `172da96` (uncommitted; 7 modified tracked files + 1 new untracked test file)
**Date:** 2026-08-01
**Verdict:** **INCOMPLETE — 5 items unresolved** (all documentation or one missing test cell; the semantic change and every load-bearing gate are DONE and green)

## Summary

- Total items: **58**
- DONE: **51**
- PARTIAL: **3**
- MISSING: **2**
- DEVIATED (documented): **2**
- DEVIATED (undocumented): **0** *(two harmless mechanism variations are recorded as notes, not as deviations — see "Notes on mechanism variations")*

The change itself — one `match` arm, one constant, one named constructor — is present, correct, and verified end-to-end at BAM level for **both** aligners. Every hand-derived value in §3.3 and §7 step 7 was **re-derived independently during this audit** against the ladder source (`mapq.rs:150-227`) and all 34 checked values agree with the plan and with the code. All 10 new tests pass; `cargo fmt -p bismark -- --check` is clean; `cargo clippy -p bismark --all-targets` reports **0 warnings**; the §7 step 2 grep returns **zero** hits; `TEMP FAULT` returns **zero** files; all three version literals still read `3.1.0`; issue **#1092** exists and states the worsening.

The five unresolved items are: one specified V14 test cell that is absent, one CHANGELOG phrasing constraint not met, two §3.5 consequences the plan said "must be stated" that are not stated user-facing, and one **factually wrong user-facing claim** the implementation introduced into `alignment.md` (the MAPQ range).

---

## Coverage ledger

### Part A — §7 Implementation outline (11 steps, expanded to their enumerated sub-items)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | `MINIMAP2_MATCH_BONUS = 2.0`, separate constant, docstring per §4 | §7.1 | DONE | `config.rs:82-91`. Docstring uses the **corrected** rammap citation `align/map.rs:215` (not §4's stale `api.rs:690`) — rev 2's §2 box applied |
| 2 | `from_emitted` → 3-arm `match`, `Minimap2 \| Rammap` **not** gated on `local`, with the "not gated" comment | §7.1, §3.1 | DONE | `config.rs:128-136` |
| 3 | `minimap_like()` named constructor | §7.1, §4 | DONE | `config.rs:166-178`; Linear form, `local=false`, `Aligner::Minimap2` |
| 4 | `end_to_end()` doc rewritten (the "monotone" model; points at `minimap_like`; records that `merge.rs`/`combined.rs` uses are intentional) | §7.1, §4 rev-2 box | DONE | Also carries the "~47 call sites" rationale and the `assert_ne!` pointer |
| 5 | `local_ladder()`'s open-question note replaced by the recorded answer + §3.4 reasoning, `unique.h:236` derivation intact | §7.1 | DONE | `config.rs:191-212`; floor-0 rejection reason present |
| 6 | Stale comment: `mapq.rs` header — fourth deviation added, byte-identity scoped to Bowtie 2/HISAT2 end-to-end | §7.2 | DONE | `mapq.rs:1-25` |
| 7 | Stale comment: `calc_mapq_local` doc — "Bowtie 2 `--local` only" → two mode families | §7.2 | DONE | `mapq.rs:143-148` |
| 8 | Stale comment: `config.rs:90-91` — first clause fixed, **second clause kept** as instructed | §7.2 | DONE | Now "Bowtie 2-local and the minimap-like aligners have a nonzero perfect score, and only Bowtie 2-local is emitted the `G` form" |
| 9 | Stale comment: `config.rs:98` field doc | §7.2 | DONE | Now names both families and "zero for every end-to-end Bowtie 2/HISAT2 mode" |
| 10 | Stale comment: `config.rs:117-119`, three lines above the edited arm — **not** preserved verbatim | §7.2 | DONE | Rewritten to "Which modes score matches positively. HISAT2 never does …" |
| 11 | Drive-by, pre-existing: `options.rs:81` HISAT2-local ladder claim | §7.2 | DONE | Now "NOT this option and NOT the MAPQ ladder (it takes the end-to-end one, #1080)" |
| 12 | §7 step 2's grep returns **zero** surviving hits | §7.2, V13 | DONE | Re-run during this audit over `rust/bismark/src rust/bismark/tests` → no output, exit 1 |
| 13 | `score_model_construction_matrix` rewritten with the decision (split loops; both `local` values for minimap-like; `diff == 220.0`; `assert_ne!`; replacement-tripwire comment) | §7.3, V5 | DONE | `mapq.rs:911-985`. `== from_emitted(…, Linear, …)` comparisons preserved exactly |
| 14 | New unit tests for §3.3's two tables, hand-derivations in the comments, incl. the floor cell and the near-tie cell | §7.4 | DONE | 5 tests; derivations present per-cell; pre-fix `42`/old-value controls asserted inline |
| 15 | Reference generalized to cover the new mode; **literal `2.0`**; `SCORE_MIN_CELLS` axis added | §7.5, V4 | DONE | `positive_bonus_reference(.., log_form)` + two thin wrappers; `2.0` is a literal with a comment saying why |
| 16 | Coverage-narrowing recorded on `end_to_end_matches_the_pre_fix_formula`, naming what took over | §7.6 | DONE | `mapq.rs:995-998`; names the `minimap_like_*` family |
| 17 | BAM cell **(a)** `AS:i:12` ⇒ **44** | §7.7 | DONE | `aligner_cli.rs` `minimap2_mapq_uses_the_perfect_score_denominator_end_to_end` |
| 18 | BAM cell **(b)** at **`AS:i:7`** (not rev 1's form-blind `AS:i:5`) ⇒ **41**, message naming each fault's value | §7.7, V6 | DONE | Message names 36 (Log form) / 42 (bonus lost) / 44 (bonus 1.0) / 24 (e2e ladder) — all four wrong values from the plan's table |
| 19 | BAM cell **(c)** `AS:i:2` ⇒ **22** (the floor) | §7.7 | DONE | Message states the e2e ladder's floor would be 0 |
| 20 | BAM cell **(d)** corrected fixture (CT slot 0 `AS:i:11` @POS 1; GA slot 1 `AS:i:12` @POS 3) ⇒ **11**; all three constraints written into the fixture comment | §7.7, V8 | DONE | `make_fake_minimap2_two_instance`; strictly-higher later slot, distinct POS, POS ≥ 3 all documented. No escape hatch used |
| 21 | `docs/…/alignment.md:111` — the false "This applies to Bowtie 2 only" sentence fixed | §7.8 | DONE | Scoped to "**Among the modes `--local` selects**, this applies to Bowtie 2 only" + a new paragraph that a positive perfect score is not exclusive to `--local`. The statement is now true and the page does not contradict itself |
| 22 | `rust/README.md` (i) minimap2 byte-identity clause **restated**, not annotated | §7.8 | DONE | Row now: "was byte-identical … up to and including 3.1.0; since #1081 it deliberately is not"; the clause was removed from the minimap2-SE bullet itself |
| 23 | `rust/README.md` (ii) the categorical "End-to-end (every aligner) … stays byte-identical" | §7.8 | DONE | → "Bowtie 2 and HISAT2 end-to-end are unaffected …; minimap2/rammap deliberately diverge too since #1081" |
| 24 | `rust/README.md` (iii) Phase-5 13-cell gate qualified, not deleted | §7.8 | DONE | "— a **pre-#1081** record; the minimap2 SE cells no longer reproduce byte-for-byte (MAPQ column only)" |
| 25 | `rust/README.md` dated Milestones line | §7.8 | DONE | `2026-08-01` entry, reverse-chronological position correct |
| 26 | CHANGELOG: **replace** the placeholder, exactly **one** #1081 bullet | §7.9, V13 | DONE | Placeholder string count 0; `issues/1081` count 1 in the whole file |
| 27 | CHANGELOG content: the seven required clauses | §7.9 | **PARTIAL** | Six of seven exact; the second-best branch is phrased as "reads whose runner-up score comes from a **different strand instance**" rather than as the later-slot-strictly-out-scores **condition** §7.9 mandated. See Gap 2 |
| 28 | CHANGELOG: amend the neighbouring **#1079** bullet | §7.9 rev-2 box, V13 | DONE | "default end-to-end path is unaffected" → "**Bowtie 2 and HISAT2 end-to-end paths** are unaffected", plus a forward pointer to the #1081 bullet |
| 29 | File the in-process-rammap-preset issue, stating that this fix **worsens** it, and cite it | §7.9, §1 non-goals | DONE | [#1092](https://github.com/FelixKrueger/Bismark/issues/1092) OPEN; body has a "Why it matters more after #1081" section; cited from PLAN §1/§12 |
| 30 | **No version bump** — all three literals stay `3.1.0` | §7.10 | DONE | `rust/VERSION` 3.1.0 · `rust/bismark/VERSION` 3.1.0 · `rust/bismark/Cargo.toml:3` `version = "3.1.0"`; no version line in the diff |
| 31 | Gates: `cargo fmt -p bismark -- --check`, clippy 0 warnings, full suite, V10 injections | §7.11 | DONE | fmt clean · clippy **0 warnings** (re-run in this audit) · suite green · `TEMP FAULT` = 0 files |

### Part B — §9 Validation (V1–V15)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 32 | **V1** no-second-best ladder, 44/44/41/28/22 at `len ∈ {50,100,1000}` | V1 | DONE | `minimap_like_denominator_uses_perfect_score_issue_1081`. Length-invariance pinned at 3 lengths; pre-fix `42` control asserted per cell. All five values re-derived independently in this audit — agree |
| 33 | **V2** with-second-best branch, both directions, incl. `200/190 → 11` and `200/20 → 39` | V2 | DONE | `minimap_like_second_best_branch_issue_1081`; all 5 cells with old **and** new values. All 10 values re-derived — agree |
| 34 | **V3** the floor = **22**, with the e2e-ladder-would-be-**0** comment, and the "ladder guard, not bonus guard" caveat | V3 | DONE | `minimap_like_floor_is_twentytwo_not_zero`; asserts `calc_mapq_end_to_end(...) == 0` on the same normalized inputs, so the rejected alternative is pinned, not just described |
| 35 | **V4** independent reference, swept `len × AS × second-best × SCORE_MIN_CELLS`, with a **literal** `2.0` | V4 | DONE | `minimap_like_matches_independent_reference`; 6 score-min cells × 5 lengths × 5 AS × 3 second-best. Literal `2.0` confirmed at `mapq.rs` `positive_bonus_reference` |
| 36 | **V5** construction matrix (the rewritten tripwire) | V5 | DONE | See item 13 |
| 37 | **V6** BAM-level wiring, minimap2 — 44 / 41 / 22 | V6 | DONE | See items 17–19; test passes |
| 38 | **V7** BAM-level wiring, **rammap** — 44 | V7 | DONE | `rammap_mapq_uses_the_perfect_score_denominator_end_to_end`; message says "42 means the `match_bonus` arm covers Minimap2 but not Rammap" |
| 39 | **V8** second-best branch end-to-end, cell (d) → 11 | V8 | DONE | See item 20; test passes |
| 40 | **V9** `AS ≤ perfect` asserted at the seam | V9 | **DEVIATED (documented)** | Landed in `config.rs::tests` (`minimap_like_perfect_score_bounds_observed_alignment_scores`, `config.rs:1596-1633`) using the `pub(crate)` `normalize` seam and asserting `best_over <= diff` over 12 spike-observed `(len, AS)` pairs. §12 deviation 1; both placements were authorized by V9 |
| 41 | **V10** six fault injections, each alone, verified failing, reverted | V10 | DONE | §12 records all six with observed values; `grep -rl 'TEMP FAULT'` = 0 files (re-verified). The detection sets are internally consistent with what each test could see: (v) is detectable **only** by the rammap BAM cell and (vi) **only** by cell (b) at `AS:i:7`, exactly as the rev-2 corrections predicted. See note N3 on (iv) |
| 42 | **V11** Bowtie 2 / HISAT2 frozen — all 9 named tests pass untouched, no expectation edits | V11 | DONE | All 9 located; `git diff` shows **no** assertion added or removed in any of them (the only `-` assertion lines in the diff are inside `score_model_construction_matrix`, which V5 mandates rewriting). Both HISAT2-local BAM MAPQ cells (`aligner_cli.rs:2269`, `:2367`) untouched and green |
| 43 | **V12** rammap in-process crosscheck unaffected and still compiles | V12 | DONE | File unmodified; `cargo check -p bismark --features rammap-inprocess --tests` → exit 0. Its `mapq` **and** `alignment_score` field-identity assertions are intact (`aligner_rammap_inprocess_crosscheck.rs:198-199`) |
| 44 | **V13** documentation deliverables gated (grep zero · one #1081 bullet · placeholder gone · #1079 bullet amended · README restated) | V13 | DONE | All four greps run in this audit and pass. The **content** defects found sit under items 27 and 49–51, not under V13's mechanical checks |
| 45 | **V14** non-default `--score_min`: `L,0,−5`, `L,0,−20`, `L,10,−0.2`, `L,100,−0.2` | V14 | **PARTIAL** | `minimap_like_non_default_score_min_compresses_upward` asserts `L,0,−0.2 / −1 / −5 / −20` (the `−20` re-saturation row = 44 across the range ✅) and the `L,10,−0.2` clamp cells for `len 1..=4` (with `bestOver < 0` — §12 deviation 2). **`L,100,−0.2` is absent.** See Gap 1 |
| 46 | **V15** live-aligner gate on `AS ≤ 2·len` | V15 | DONE | New `tests/aligner_minimap2_as_bound.rs`; runs real minimap2 across `map-ont`/`map-pb`/`sr` with Bismark's verbatim option string; asserts `AS <= 2·len` everywhere **and `AS == 2·len` exactly** for perfect reads; skip-locally / **panic-if-`$CI`**; a `perfect_cells >= 6` guard so the equality half cannot silently vanish. **Executed for real** in this audit (minimap2 2.31-r1302 on PATH; no skip message emitted) and passed |

### Part C — §3.5 edge cases and §8 assumptions that imply an artifact

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 47 | Bowtie 2 / HISAT2 invariance **by construction** (different `match` arm, `_ => 0.0` untouched) | §3.5 | DONE | Confirmed by reading `from_emitted`; V11 is the net |
| 48 | `AS > perfect` deliberately **not** clamped in code | §3.5 | DONE | `normalize` body unchanged apart from the pre-existing `max(1.0)` on `diff`; no runtime clamp on `best_over` added |
| 49 | Soft clipping: `perfect()` uses the **full** read length, stated | §3.5 | DONE | `perfect()` doc gained the Bowtie 2 `scoring.h:310-316` note; also stated user-facing in `alignment.md:316` |
| 50 | **Strand-order artefact** — "Must be stated, not discovered by a user diffing two BAMs" | §3.5 | **PARTIAL** | Stated in **code** (the cell-(d) fixture comment and the V2 test doc explain the running-maxima rule) but **nowhere user-facing**: no CHANGELOG, docs or README text says that two reads with identical `(AS, AS₂)` get different MAPQ depending on which strand won, nor that the near-tie spread widens from 17 to 33 points. See Gap 3 |
| 51 | **`--ambig_bam`** — "Consequence worth stating rather than leaving to be found: two different MAPQ scales in two files" | §3.5 | **MISSING** | Not stated anywhere. `output.rs` is untouched (correct — the behaviour needs no change), but no CHANGELOG/docs sentence tells a `--minimap2 --ambig_bam` user that the two files carry different MAPQ scales. See Gap 4 |
| 52 | PE: **no** PE expectation written into any test | §3.5 | DONE | Every new call site passes `read2_len = None`; the only `read2_len` occurrences in the diff are pass-through parameters of the reference wrappers |
| 53 | `len = 0` finite and total (linear form, no `ln(0)`) | §3.5 | DONE (by construction) | Guaranteed by the Linear form in `minimap_like`; §9 specified no test for it |
| 54 | `diff` clamp `max(1)` is **live** for minimap-like, and benign | §3.5, V14 | DONE | Pinned at `len 1..=4` under `L,10,−0.2`, asserting `diff == 1.0`, `best_over < 0.0` and MAPQ 22 |
| 55 | A1 (`AS ≤ 2·len`) has both an arithmetic pin and a **live** gate | §8 A1 | DONE | Items 40 + 46 |
| 56 | A6 (`scMin` is a fiction, and `--score_min` moves MAPQ without moving an alignment) documented at the code sites | §8 A6 | DONE | `normalize()` doc + `score_min_params` doc + `build_aligner_options` comment + `alignment.md:316`. See note N4 on the `--score_min` **help text** wording |
| 57 | A12 (order-dependent runner-up selection) made explicit where it bites | §8 A12 | DONE | `make_fake_minimap2_two_instance`'s comment states the running-maxima rule and marks the score order as "load-bearing and counter-intuitive" |
| 58 | New user-facing MAPQ prose under `--minimap2` is **accurate** | §7.8 deliverable | **MISSING/incorrect** | `alignment.md:314` states "MAPQ therefore ranges from **22** … to 44". With a cross-instance runner-up the local ladder returns **11, 12, 9, 2 …** (`mapq.rs:204-226`) — the project's own cell-(d) test asserts **11**, and the CHANGELOG says 11 two files away. See Gap 5 |

---

## Gaps (detail)

### Gap 1 — V14's `L,100,−0.2` cell is absent (item 45)

**Expected (V14):** "Unit cells at `L,0,−5` and `L,0,−20` (re-saturation) **and** `L,10,−0.2` / `L,100,−0.2` (positive intercept ⇒ the clamp fires, `bestOver` negative)."
**Found:** `minimap_like_non_default_score_min_compresses_upward` covers `L,0,{−0.2,−1,−5,−20}` and `L,10,−0.2` (`len 1..=4`). `SCORE_MIN_CELLS` — the axis V4 sweeps — contains `(10.0, −0.2)` but **not** `(100.0, −0.2)`, so the missing witness is not picked up elsewhere either.
**Gap:** add the `L,100,−0.2` cell. §3.5 named it as the sharper witness ("at 6 bp it is `−86.8`"), i.e. a case where the clamp fires at a *realistic* read length rather than only at `len ≤ 4`. The mechanism is the same, so this is a completeness gap, not a suspected bug.

### Gap 2 — the CHANGELOG's second-best condition is looser than §7 step 9 mandated (item 27)

**Expected (§7 step 9):** "reads whose runner-up comes from a **later**-slot instance that strictly out-scores the earlier one move in **both** directions … — phrased as that condition, **not** as 'reads that map to both strands' (§1 rev-2 correction)."
**Found:** "Reads whose runner-up score comes from a different strand instance move in **both** directions …".
**Gap:** the forbidden phrasing was avoided and the sentence is not false, but the **condition** (later slot, strictly higher score) is not stated, so a reader still cannot tell which of their reads move. One clause fixes it.

### Gap 3 — the strand-order artefact is not stated user-facing (item 50)

**Expected (§3.5):** "Two reads with identical `(AS, AS₂)` get different MAPQ depending only on which strand won … this fix **widens** the near-tie spread from **17** points to **33**. Must be stated, not discovered by a user diffing two BAMs."
**Found:** the mechanism is documented in two code comments (the cell-(d) fixture and the V2 test doc). No user-facing text mentions it.
**Gap:** one sentence in the CHANGELOG bullet (or the `--minimap2` docs block) saying the effect depends on which strand instance won, and that this fix widens a pre-existing asymmetry. Note §7 never assigned this row a deliverable, so it is as much a plan omission as an implementation one — but §3.5's own wording makes it a requirement.

### Gap 4 — the `--ambig_bam` two-scales consequence is not stated (item 51)

**Expected (§3.5):** "`--ambig_bam` … Consequence worth stating rather than leaving to be found: a `--minimap2 --ambig_bam` run legitimately emits **two different MAPQ scales in two files**."
**Found:** nothing. `output.rs:819/:833` is correctly untouched; the omission is purely the statement.
**Gap:** one sentence, most naturally in the new `--minimap2` MAPQ block in `alignment.md`. Same caveat as Gap 3: §7 gave it no home.

### Gap 5 — `alignment.md` states a MAPQ range that the code contradicts (item 58)

**Expected:** the new prose §7 step 8 asked for under `--minimap2`, accurate.
**Found (`docs/src/content/docs/options/alignment.md:314`):** "MAPQ therefore ranges from 22 (a read scoring well below its best possible score) to 44."
**Gap:** that range holds only for the **no-second-best** branch. With a cross-instance runner-up the local ladder (`mapq.rs:204-226`) returns 11 (`best_diff` small, `best_over ≥ 0.5·diff`) or **2** (`best_over < 0.5·diff`), plus 9/12/14/16/17/18/19/20/21/25 on the intermediate rungs — and `minimap2_mapq_second_best_branch_end_to_end` **asserts 11 out of a real BAM**, while the CHANGELOG bullet says "drops from 25 to **11**". So the docs page contradicts both the code and the release notes, on the low side, by 20 MAPQ points. This is precisely the "unique reads stand for the whole class" failure the rev-2 fold spent §1 correcting; it re-entered through a deliverable the plan created but did not spell out. Fix: state the two branches, or give the range as 2–44 with a note that unique reads floor at 22.

---

## Test verification

| Test | File | Status |
|---|---|---|
| `minimap_like_denominator_uses_perfect_score_issue_1081` (V1) | `rust/bismark/src/aligner/mapq.rs` | **PASS** |
| `minimap_like_second_best_branch_issue_1081` (V2) | `rust/bismark/src/aligner/mapq.rs` | **PASS** |
| `minimap_like_floor_is_twentytwo_not_zero` (V3) | `rust/bismark/src/aligner/mapq.rs` | **PASS** |
| `minimap_like_matches_independent_reference` (V4) | `rust/bismark/src/aligner/mapq.rs` | **PASS** |
| `minimap_like_non_default_score_min_compresses_upward` (V14) | `rust/bismark/src/aligner/mapq.rs` | **PASS** (one specified cell absent — Gap 1) |
| `score_model_construction_matrix` (V5, rewritten) | `rust/bismark/src/aligner/mapq.rs` | **PASS** |
| `minimap_like_perfect_score_bounds_observed_alignment_scores` (V9) | `rust/bismark/src/aligner/config.rs` | **PASS** |
| `minimap2_mapq_uses_the_perfect_score_denominator_end_to_end` (V6, cells a/b/c) | `rust/bismark/tests/aligner_cli.rs` | **PASS** |
| `minimap2_mapq_second_best_branch_end_to_end` (V8, cell d) | `rust/bismark/tests/aligner_cli.rs` | **PASS** |
| `rammap_mapq_uses_the_perfect_score_denominator_end_to_end` (V7) | `rust/bismark/tests/aligner_cli.rs` | **PASS** |
| `minimap2_reports_at_most_two_per_base` (V15) | `rust/bismark/tests/aligner_minimap2_as_bound.rs` | **PASS** — really ran against minimap2 2.31-r1302, not skipped |
| V11's 9 frozen-path tests (`end_to_end_matches_the_pre_fix_formula`, `hisat2_local_denominator_is_abs_of_the_linear_scmin`, `local_hisat2_uses_the_linear_form_and_end_to_end_ladder`, `score_min_params_aligner_and_mode_defaults`, `hisat2_local_softclip_roundtrip_and_options`, `hisat2_local_pe_softclip_roundtrip`, Bowtie 2 PE MAPQ, `select_unique_best_mapq_equals_calc_mapq`, `bowtie2_local_*`) | `mapq.rs` / `options.rs` / `combined.rs` / `aligner_cli.rs` | **PASS**, no expectation edits |

**Suite:** `cargo test -p bismark` — **exit 0**, 77 test binaries, **2112 passed / 0 failed / 20 ignored** (the 20 are the pre-existing env/feature-gated ones). Breakdown for the binaries this change touches: lib **1445 / 0**, `tests/aligner_cli.rs` **115 / 0**, `tests/aligner_minimap2_as_bound.rs` **1 / 0**. No `FAILED`, no `failures:`, no panic in the log. §12 reports "2120 pass" — 8 more than measured here; the delta is in binaries unrelated to this change and is almost certainly environment-gated (external-tool presence), and both runs report **0 failures**.

**Other gates re-run in this audit:** `cargo fmt -p bismark -- --check` clean · `cargo clippy -p bismark --all-targets` **0 warnings** · `cargo check -p bismark --features rammap-inprocess --tests` exit 0 (V12) · §7 step 2 grep **0 hits** · `TEMP FAULT` **0 files** · version literals `3.1.0` / `3.1.0` / `3.1.0`.

**Independent value re-derivation.** All 5 cells of §3.3 table 1, all 5 cells of table 2, and all 4 BAM cells were re-derived in this audit directly from `calc_mapq_local` (`mapq.rs:150-227`) and `normalize` (`config.rs:245-259`) — 34 values including the `bestOver`/`diff`/rung reasoning. No disagreement with the plan or the code.

---

## Notes on mechanism variations (not counted as deviations)

- **N1 — cell (a) uses a new parameterized fake.** V6 said "via `make_fake_minimap2_mapped` + two new fixtures"; the implementation added `make_fake_minimap2_with_score(dir, as_value)` and uses it for all three cells. The emitted SAM line at `as_value = 12` is character-for-character the same scenario as `make_fake_minimap2_mapped` (same FLAG/rname/POS/CIGAR/tags, GA unmapped), so cell (a) is the fixture the plan meant. No behavioural difference.
- **N2 — §7 step 5's trap 2 is structurally moot.** The plan required "either assert `match_bonus > 0.0` inside it, or derive the ladder selection inside it" *once parameterized by `match_bonus`*. The reference was parameterized by `log_form` only, with the perfect-score-per-base hardcoded as the literal `2.0`, so a zero-bonus caller cannot be constructed and the trap cannot arise. The invariant is recorded in a comment. Precondition unmet ⇒ not a deviation.
- **N3 — V10 injection (iv)'s detection set.** V10 predicted "(iv) build `minimap_like` with `ScoreMinForm::Log` → V5 **and V6 cell (b)** fail". §12 reports it detected by "3 unit tests incl. the matrix" and not by the BAM cell. That is correct and the plan's prediction was over-broad: `minimap_like()` is test-only — production builds the model at the single site `config.rs:840` via `from_emitted` — so a BAM test cannot see an injection into the constructor. This is exactly why the plan also specified (vi), the *production*-form injection, which §12 records as failing at cell (b) with `left: 36, right: 41`.
- **N4 — a doc cross-reference points at the CLI help.** `normalize()`'s new doc says the `--score_min` sensitivity is "documented in the `--score_min` help"; the `--score_min` **clap help** (`aligner/cli.rs:285`) is unchanged. §12 deviation 4 describes the edit as landing on `--score_min`'s doc comment, which it did (`options.rs` `score_min_params`), and the user-facing statement lives in `alignment.md:316` under `--minimap2`. Cosmetic wording only.
- **N5 — `rust/README.md:230`** still carries the unqualified historical claim "minimap2 single-end byte-identical to Perl v0.25.1 + minimap2 2.31-r1302" in the **2026-06-06** Milestones entry. §7 step 8's inventory named only the row's three statements, and the plan's own convention is to qualify rather than rewrite historical records — the new 2026-08-01 entry sits above it and retires the claim. Recorded for awareness, not as a gap.
- **N6 — §12's test-count arithmetic.** §12 states "2120 tests pass … (was 2109; **+11**)". The diff adds **10** `#[test]` functions (9 tracked + 1 in the new untracked file) and no doctests, and this audit measured **2112 passed / 0 failed / 20 ignored**. Immaterial to coverage — 0 failures either way — and noted only because §12 is otherwise precise.

---

## Verdict

**INCOMPLETE — 5 items unresolved.**

Nothing in the shipped behaviour is wrong, and nothing load-bearing is missing: the `match` arm, the constant, `minimap_like()`, the rewritten tripwire, the generalized reference with its literal `2.0`, all four BAM cells (including the corrected cell (d) that both plan reviewers flagged as rev 1's only Critical), the rammap cell, the live minimap2 `AS`-bound gate, the no-version-bump rule, the stale-comment purge, the CHANGELOG replacement and the #1079 amendment, issue #1092, and every frozen-path test are all DONE and green. `fmt`, `clippy`, the full suite, the step-2 grep and the `TEMP FAULT` sweep were all re-run during this audit and all pass.

What remains, in the order I would fix it:

1. **`docs/src/content/docs/options/alignment.md:314` states a false MAPQ range** — "ranges from 22 … to 44". With a cross-instance runner-up minimap2 MAPQ reaches **11** (asserted by `minimap2_mapq_second_best_branch_end_to_end`) and can reach **2**. The page contradicts the CHANGELOG bullet and the code. Fix the sentence to cover both branches. *(Item 58 / Gap 5.)*
2. **V14 is missing its `L,100,−0.2` cell** — the plan named two positive-intercept witnesses for the live `max(1, …)` clamp and only `L,10,−0.2` was written; `SCORE_MIN_CELLS` does not carry `L,100,−0.2` either, so nothing else covers it. *(Item 45 / Gap 1.)*
3. **The strand-order artefact is not stated user-facing** — §3.5 says it "must be stated, not discovered by a user diffing two BAMs". It exists only in two code comments. One sentence in the CHANGELOG bullet or the `--minimap2` docs block. *(Item 50 / Gap 3.)*
4. **The `--ambig_bam` two-MAPQ-scales consequence is not stated anywhere** — §3.5 asked for it explicitly. One sentence. *(Item 51 / Gap 4.)*
5. **The CHANGELOG's second-best clause does not state the condition** §7 step 9 required (later slot, strictly out-scores). It avoids the phrasing the plan forbade and is not false, but a reader cannot tell which reads move. One clause. *(Item 27 / Gap 2.)*

Items 3 and 4 are rows in §3.5 that §7 never turned into a named deliverable, so they are partly a plan gap; §3.5's wording ("must be stated") still makes them requirements. Items 1, 2 and 5 are squarely implementation gaps against text the plan spelled out.

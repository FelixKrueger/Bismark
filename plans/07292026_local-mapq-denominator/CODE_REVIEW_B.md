# CODE REVIEW B — Local-mode MAPQ denominator (#1079)

**Reviewer:** B (independent, fresh context)
**Target:** uncommitted working tree on `rust/local-mapq-denominator`, 9 files, +730/−317
**Date:** 2026-07-29
**Verdict:** **APPROVE WITH CHANGES** — the semantics are correct and independently verified against upstream Bowtie 2 v2.5.5. One **High** finding: the D2 wiring (emitted score-min form → `ScoreModel`) has **no test with teeth**, which is the same silent-no-op failure shape the plan named load-bearing. Five **Medium** findings are documentation/gate accuracy, three of them in user-facing release notes.

---

## 1. Summary

The change does what it claims. I verified the arithmetic independently rather than re-running the suite:

**Verified correct**

| Claim | How I checked | Result |
|---|---|---|
| Upstream formula | fetched `unique.h` v2.5.5 L195-235 + `scoring.h` L308-316 | `scPer` sums both mates; `diff = max(1, scPer − scMin)`; `bestOver = best − scMin`; `perfectScore = monotone ? 0 : rdlen × match(30)`. Implementation matches. ✅ |
| Bismark's local ladder | leaf-for-leaf vs `unique.h`'s non-monotone branch | identical (44/42/41/36/28/24/22; 40/39/38/37; 35/25/20, 34/21/19, 33/18/16, 32/17/12, 31/14/9, 11/2, 1/0). ✅ Matters now that the fix makes those rungs reachable. |
| `calc_mapq_end_to_end` is a faithful extraction | programmatic whitespace-normalized diff of the body vs `git show HEAD:…/mapq.rs` | **86 lines both sides, identical.** No changed threshold or return. ✅ |
| Frozen-path bit-identity | read both formulas | `intercept + slope·x` in the same association order, `term(l1)` then `+= term(l2)`, then `.abs()`, then `as_best − sc_min`. Bit-identical, not merely equal. ✅ |
| HISAT2 hand-derived cells (44/44/44/31/1/11) | re-derived from the linear `scMin` in an independent Python ladder | all six ✅ |
| #1079 reporter cases | same | len 100 → **24** (was 42) ✅; `AS = perfect = 200` → **44**, `bo == diff` ✅; PE `400 − 2·scMin`, and `t + t == 2.0·t` is exact in binary FP so the test's `2.0 *` shortcut is safe ✅ |
| V10 fixture discrimination | same | len 6 / `G,1,0` / `AS:i:6` → **28** new vs **44** pre-fix ✅ |
| `ln()`-boundary cell | same | as_best 50 → 44, as_best 48 → 36 ✅ (comment *numbers* are off — see L1) |
| Gating airtight | traced `options.rs:82`, `:376`, `config.rs:769-778` | `match_bonus != 0` ⟺ `cli.local && aligner == Bowtie2` — the **identical predicate** that pushes `--local` to the aligner and selects the `G` form, resolved once at one construction point. ✅ |
| Refactor fidelity | diff + grep | 10 signatures, 2 fn-pointer aliases, 6 expansion sites, 4 production call sites; SE passes `None`, PE passes `Some(sequence_2.len())` at all four; **zero** trio leftovers. No swapped/dropped/duplicated argument. ✅ |
| Allow removals | arg counts | `check_results_single_end` 7, `select_pe_nondir` 6 — both under clippy's `>7`; `check_results_paired_end` is still 9 and correctly keeps its allow. ✅ |
| Toolchain | re-ran myself | **2107 pass / 0 fail**, `clippy -p bismark --all-targets` clean, `cargo fmt --check` clean. ✅ |
| CHANGELOG quantitative claims | recomputed | 250 bp @ 30 % of perfect → 44 old / **22** new ✅; HISAT2 `−0.92` vs `−20` at 100 bp ✅ |

**`f64 > 0.0` is the right predicate.** The field is private and only ever the const `2.0` or a literal `0.0` — no rounding can land it in between, and `>` is not `float_cmp`. It is also strictly narrower than gating on `local`, exactly as the plan argued.

**`normalize()`'s API is sound.** Returning `(best_over, diff)` from one length pair structurally prevents the mismatch it was designed to prevent. A caller can still pass wrong lengths, but that is inherent to any signature, and all four production sites pass the right ones.

**The V10 fixture's AS/CIGAR decoupling is acceptable.** MAPQ reads the `AS:i:` tag; the fixture exists to prove the resolved model reaches the BAM. It masks nothing about the MAPQ computation. The residual exposure — that no *real* Bowtie 2 comparison exists (V8) — is a plan-level accepted risk, not a fixture defect.

---

## 2. Issues by area

### Logic

#### **H1 (High) — the D2 wiring is untested: `--score_min G,1,0` is form-blind**

`rust/bismark/tests/aligner_cli.rs:137-141`

D2's entire point is that the emitted score-min **form** is single-sourced from `score_min_params` into `ScoreModel`. Nothing tests that join:

- `options.rs` tests exercise `score_min_params` **in isolation** → they pass whatever `resolve` does with the result.
- `mapq.rs::score_model_construction_matrix` exercises `from_emitted` **in isolation** → passes a hand-written `form`.
- **V10 uses `--score_min G,1,0`. With slope 0, `i + s·ln(len)` and `i + s·len` are both exactly `1.0`** — the test cannot see the form at all.
- The two existing `--hisat2 --local` integration tests (`:2125`, `:2208`) assert the option string and the `2S4M` CIGAR, **no MAPQ**.
- There is no successful-`resolve` unit test in `config.rs` (every `resolve` test there is an error path, no genome fixture), and `grep score_model` finds no test reading `RunConfig.score_model`.

So a `resolve` that hardcoded `ScoreMinForm::Linear` — giving Bowtie 2-local `scMin = 20 + 8·len` instead of `20 + 8·ln(len)`, badly wrong — would ship with **2107/2107 green**. That is precisely the failure shape §9 calls load-bearing ("a mis-wired `aligner` makes the fix a silent no-op with a fully green suite — the same failure shape that let #1079 ship"); the implementation closed it for `match_bonus` and left it open for `form`.

**Fix (one-line, verified numerically).** Change V10's score-min to a slope-bearing G function. On the existing 6 bp / `AS:i:6` fixture:

| `--score_min` | new + `Log` | new + `Linear` | pre-fix (`abs(scMin)`) |
|---|---|---|---|
| `G,1,0` (current) | 28 | **28** ← blind | 44 |
| **`G,1,1`** | **24** | 22 | 44 |
| `G,0,1` | 28 | 22 | 44 |

Prefer **`G,1,1`** → expected MAPQ **24**: it discriminates the match bonus *and* the form in one cell, and `scMin = 1 + ln(len) ≥ 1` keeps the strictly-positive property the test comment leans on ("a legal always-positive G function"). `G,0,1` preserves the current expected `28` but has `scMin = 0` at len 1, losing that property. Update the trailing comment to `perfect = 12, scMin = 1 + ln(6) = 2.7918 → diff = 9.2082; bestOver = 3.2082; 3.2082/9.2082 = 0.3484 → 24`, and extend the failure message to name both faults (lost match bonus → 44; wrong form → 22).

#### **M6 (Medium) — V6 half-implemented, and the plan's stated risk is wrong**

`mapq.rs:565-573` (`local_diff_is_clamped_to_at_least_one`)

V6 required "⇒ `diff == 1.0`, **and assert which rung results** (risk is a silent top rung, not a panic)". The test asserts `diff == 1.0` and the frozen `e2e == 0.0`, but never calls `calc_mapq`, so the rung is unpinned.

I computed it: for len 6 / `G,20,8`, `sc_min = 34.334`, so `best_over = as_best − 34.334` is hugely negative and the result is **22 — the bottom rung — for every `as_best` in `0..=perfect`**. The plan's "silent top rung" description is therefore wrong for the only reachable clamp configuration, and it is worth recording that inversion rather than leaving a future reader to re-derive it.

Add, in the same test:
```rust
// The clamp lands on the BOTTOM rung, not the top: best_over is -22.33 here.
assert_eq!(calc_mapq(6, None, 0, None, ScoreModel::bowtie2_local(20.0, 8.0)), 22);
```

#### **L3 (Low) — `ScoreModel::end_to_end` hardcodes `Aligner::Bowtie2`, and A9 is going to break that**

`config.rs:128-136`

Correct today (every end-to-end match bonus is 0) and `score_model_construction_matrix` pins the aligner-independence, so a future divergence fails loudly rather than silently. But A9 — filed as a follow-up by *this* work — will give minimap2/rammap a nonzero perfect score in the **end-to-end** path, at which point `end_to_end()` becomes Bowtie 2-specific at ~29 call sites and `frozen_paths_match_the_pre_fix_formula` goes blind to two aligners. Add one line to the doc comment: `/// Aligner-independent only while every end-to-end match bonus is 0 — the minimap2/rammap follow-up (A9) must revisit this.`

#### **L8 (Low, informational) — the clamp launders NaN**

`--score_min L,nan,-0.2` parses (Rust's `f64::from_str` accepts `"nan"`/`"inf"`, and validation is shape-only per A11). `f64::max` returns the non-NaN operand, so `(perfect − NaN).max(1.0)` yields `diff = 1.0` while `best_over` stays NaN. Every comparison is still false, so both ladders return their bottom rung (22 / 0) exactly as before — **no behaviour change**, and pre-existing. Noted only because the clamp silently converts a NaN denominator into a finite one; if numeric validation is ever added to `--score_min`, that is the place to reject it.

### Errors

None found. No panic path, no division (both ladders compare only), no overflow, no unwrap on user input in the changed code.

### Efficiency

No issues. `normalize` is one extra multiply plus one compare per accepted alignment; `perfect()` is evaluated only inside the `match_bonus > 0.0` branch; `ScoreModel` is 32 bytes, `Copy`, by value — no lifetime in the two fn-pointer aliases. Arity dropped by 2 at ten sites. One test-only redundancy (L4).

#### **L4 (Low) — a loop-invariant assertion runs 990 times for 66 distinct facts**

`mapq.rs:641-676` — the HISAT2 block inside `frozen_paths_match_the_pre_fix_formula`:
```rust
let (_, hd) = ScoreModel::hisat2_local(i, s).normalize(len, None, best);
assert_eq!(hd, (i + s * len as f64).abs(), …);
```
`hd` is the *denominator*, independent of `best` and `sec`, but the assertion sits inside both inner loops (6 × 11 × 5 × 3). Hoist it to the `len` loop.

Substantively: this assertion does have teeth (if HISAT2-local ever acquired a match bonus, `(10.0, −0.2)` at len 1 would give `max(1, 0 − 9.8) = 1.0 ≠ 9.8` and fail), but it only pins the denominator — HISAT2-local is never routed through `calc_mapq` in the sweep, so the *ladder* half of the frozen claim is unswept for that mode. That is unavoidable (the form changed, so no full pre-fix comparison exists) and the doc comment explains it; worth one clarifying clause so the name "frozen paths match the pre-fix formula" isn't read as stronger than it is.

#### **L5 (Low) — the independent reference has no `--score_min` axis**

`mapq.rs:433-465` sweeps `len × as_best × as_second` and PE, but only at `G,20,8`. `G,1,0` (which V10 relies on) and small-slope G forms — the clamp-adjacent regime — are never reference-compared. V1 got a `--score_min` axis for exactly this reason; V2 should get one `(i, s)` pair too (e.g. `(1.0, 1.0)`), which also then covers the cell H1 wants added.

#### **L7 (Low) — V11's `==diff` second-best leaves are only reference-compared**

The reporter's case #2 is pinned for the no-second-best half (`44`), and the `==diff` leaves (`mapq.rs:168,176,184,192,200`) are exercised inside the grid — but only against `bowtie2_local_reference`, never as a pinned rung value. Reference agreement is a real gate (the reference is an independent transcription), so this is a nice-to-have: one `assert_eq!(calc_mapq(25, None, 50, Some(49), bowtie2_local(20.0, 8.0)), 32)` would pin the leaf that became reachable for the first time.

Separately, the surviving `assert_ne!(bowtie2_local(20,8), end_to_end(20,8))` at `:468-471` compares against a nonsense configuration (a *positive* end-to-end slope → `scMin = 420`, MAPQ 0 vs 44). Inherited from the replaced test; harmless but near-zero information.

### Structure & documentation

#### **M1 (Medium) — CHANGELOG and README re-introduce the false equivalence the plan spent §3 refuting**

`CHANGELOG.md` bullet 1 and `rust/README.md:181` both say the end-to-end path is unaffected **"(there the perfect score is 0, so the two expressions agree)"** / **"(perfect score 0 makes the two expressions agree)"**.

They do **not** agree. `max(1, 0 − scMin) ≡ abs(scMin)` only for `scMin ≤ −1`; PLAN §3 ("Why gate on `match_bonus`, not `local`") and rev-1's **C1** exist because rev 0 made this exact claim and it was wrong — the divergence table there lists `L,10,-0.2` at 50 bp going 42 → 0 under a universal clamp. End-to-end is unaffected because the code **branches** on `match_bonus`, keeping `abs(scMin)` untouched.

This is the highest-value doc fix in the change: it is the precise justification a future maintainer would cite to "simplify" the branch away, and `frozen_paths_match_the_pre_fix_formula` exists to catch that fault (§12 records it failing at `len=1` when injected). Shipping the refuted reasoning as release notes undoes that.

Reword both to, e.g.: *"the default end-to-end path is unaffected — it keeps `abs(score_min)` unchanged. The new denominator is applied only where the perfect score is nonzero; the two expressions are **not** equivalent in general, so it is deliberately not applied universally."*

#### **M2 (Medium) — CHANGELOG contradicts itself about HISAT2 `--local`**

Bullet 1 ends: *"The default end-to-end path is unaffected and stays byte-identical …, **as does HISAT2 `--local`**."*
Bullet 2 states: *"… so **HISAT2 `--local` MAPQ changes**."*

In context bullet 1 means "unaffected *by the denominator change*", but as written a HISAT2-local user reads bullet 1 and concludes nothing changed for them — and their MAPQ values do change (e.g. 50 bp / `AS −1` / no second best: 22 → 44). Qualify bullet 1: *"…as does HISAT2 `--local`'s **denominator** — but its score-min form changes; see below."*

#### **M3 (Medium) — the release notes point at trackers that do not exist**

CHANGELOG bullet 2: *"that is tracked separately"*; bullet 3: *"Left byte-frozen here"* … *"tracked separately"*. PLAN §12 records: *"The two deferred issues (HISAT2 perfect score; minimap2/rammap positive-AS) are **not yet filed**."* Either file them and cite the numbers, or soften to "tracked as a follow-up" until they exist. As written the notes are factually wrong at merge time.

#### **M4 (Medium) — stale `options.rs` comments still assert the pre-D2 model, one of them inside the doc comment that was edited**

- `options.rs:80-81`: *"HISAT2-local uses the SAME L-form as end-to-end … its local-ness is the dropped `--no-softclip` in the HISAT2 tail **+ the ln() MAPQ scMin**, NOT this option."*
- `options.rs:366-367`: *"HISAT2 uses the L-form even in local mode; **the local-ness is the `ln()` scMin, not the form**"* — sitting five lines above the newly added *"Returns the emitted **form** … `calc_mapq` must evaluate the same function the aligner was given"*. The same doc comment now says both things.

Both statements are now false, and this is the exact stale-comment class that let D2 survive in the first place (PLAN step 8 called for fixing the docs that "describe `local` as selecting `ln()` scMin" — `mapq.rs` and `config.rs` were fixed, `options.rs` was missed). Replace the `ln()` clauses with "the dropped `--no-softclip` in the HISAT2 tail + the local MAPQ ladder".

#### **M5 (Medium) — `rust/README.md` aligner **row** not updated (plan step 8), and not recorded as a deviation**

PLAN step 8 requires *"`rust/README.md`: **aligner row** + dated Milestones line"*. Only the Milestones line (`:181`) landed. Row `:161` — the canonical per-tool status record — still headlines *"**byte-identical** to Perl v0.25.1 + Bowtie 2 2.5.5"* with no mention that Bowtie 2 `--local` now deliberately diverges. Literally true of the *gated* configurations (all end-to-end), but the row is what a reader consults to answer "is this byte-identical to Perl?", and §12 does not list the omission among its two documented deviations, so it reads as done.

Append to row `:161`: *"**Bowtie 2 `--local` deliberately diverges from Perl v0.25.1 since #1079** — MAPQ uses Bowtie 2's own `max(1, perfect − score_min)` denominator (Perl's `abs(score_min)` assumed a perfect score of 0); end-to-end is unaffected and stays byte-identical."*

#### **L1 (Low) — hand-derived comment numbers are wrong in the third decimal**

`mapq.rs:570-586` (`local_bowtie2_ln_derived_bucket_boundary`):

| Comment | Actual |
|---|---|
| `scMin = 20 + 8·ln(25) = 45.7527…` | **45.7510066** |
| `diff = 4.2473…` | **4.2489934** |
| `best_over 2.247` | **2.2489934** |
| `0.5/0.4 boundary sits at 2.1237` | **2.1244967** |

The conclusions are right (rung **36**, margin ≈ 0.1245 — the comment's "~0.12" is correct). But this file's gate *is* hand-derivation ("NOT read back from the implementation"), so an inexact audit trail undercuts the one thing that makes these assertions independent. Correct the four figures.

Related, same file: the new HISAT2 cell at `:544-548` relies on `1.0 >= 10.0 * 0.1` being **exactly** true. It is (`10.0 * 0.1 == 1.0` under IEEE-754 round-to-nearest — I checked), and Perl computes the identical double, so it is deterministic and parity-safe. But its margin is **zero**, where the test it replaced explicitly boasted *"Margin to the boundary here is ~0.002, not 1 ULP — the value is robust across platforms."* The comment does flag the exactness; consider adding one neighbouring cell with real margin so the mode isn't pinned solely on a knife edge.

#### **L2 (Low) — the `float_cmp` allow on `calc_mapq` is now vacuous**

`mapq.rs:22`. Every float comparison moved out: into the two ladders (each carries its own allow, correctly) and into `normalize` (which only does `> 0.0` and `.max`, neither being `float_cmp`). The attribute now reads as if `calc_mapq` compares floats. Remove it, or move its explanatory comment to `normalize`.

To answer the brief's question directly: **the justification is now stronger, not weaker.** `best_over == diff` used to mean `as_best == 2·scMin` (an arbitrary coincidence). It now means `as_best == perfect`, and both sides are the *same* f64 subtraction of the *same* `sc_min`, so the equality is exact whenever the integers are equal. (Note for completeness: all three allows are decorative under the current CI config — `clippy::float_cmp` is `pedantic`/allow-by-default, there is no `[lints]` table or crate-level attribute, and CI runs plain `cargo clippy --workspace --all-targets -- -D warnings`. Pre-existing convention; not a defect.)

#### **L6 (Low) — the docs site carries no HISAT2 caveat**

`docs/src/content/docs/options/alignment.md:106` already documents the correct Bowtie 2 formula ("the best possible alignment score is equal to the match bonus (`--ma`) times the length of the read") — which is the plan's own justification for calling this a fix, so no change is needed there. But the page drops the Perl help's HISAT2 sentence ("for HISAT2, it is currently not exactly known how the best alignment is calculated"), so HISAT2-local users get changed MAPQ values with no user-facing note anywhere except the CHANGELOG. One sentence closes it.

#### **L9 (Low, informational) — upstream multiplies by `(double)0.8f`, not `0.8`**

`unique.h` writes every threshold as `diff * (double)0.8f` — a *float* literal widened to double, i.e. `0.800000011920928955078125`. Bismark (like Perl) uses the double `0.8`. Same class as A10's `int64` non-goal and equally out of scope, but it is a second, undocumented int/float divergence, and it reinforces the caveat already on `bowtie2_local_reference` ("deliberately NOT a faithful transcription"). Worth one clause in that doc comment so a future reader doesn't try to make the reference exact.

---

## 3. Recommendations, by priority

| # | Priority | Action | File:line |
|---|---|---|---|
| H1 | **High** | Make V10 form-sensitive: `--score_min G,1,0` → **`G,1,1`**, expected MAPQ `28` → **`24`**; update the arithmetic comment and the failure message (44 = lost match bonus, 22 = wrong form) | `tests/aligner_cli.rs:137-141,158-164` |
| M1 | **Medium** | Delete the "the two expressions agree" parenthetical from both places; state that end-to-end is unaffected because the code branches, and that the expressions are **not** equivalent | `CHANGELOG.md` bullet 1; `rust/README.md:181` |
| M2 | **Medium** | Qualify "as does HISAT2 `--local`" → "as does HISAT2 `--local`'s denominator; its score-min form does change (see below)" | `CHANGELOG.md` bullet 1 |
| M3 | **Medium** | File the two deferred issues and cite the numbers, or soften "tracked separately" | `CHANGELOG.md` bullets 2-3 |
| M4 | **Medium** | Strike the two "local-ness is the `ln()` scMin" clauses (one contradicts the doc comment it sits in) | `options.rs:80-81`, `options.rs:366-367` |
| M5 | **Medium** | Add the `--local` divergence note to the aligner **row**, per plan step 8 | `rust/README.md:161` |
| M6 | **Medium** | Assert the clamped rung (**22**, bottom — not top) | `mapq.rs:565-573` |
| L1 | Low | Fix the four `ln(25)` figures (45.7510 / 4.24899 / 2.24899 / 2.12450) | `mapq.rs:570-586` |
| L2 | Low | Remove the now-vacuous `float_cmp` allow on `calc_mapq` | `mapq.rs:22` |
| L3 | Low | Note that `end_to_end()`'s aligner-independence must be revisited by the A9 follow-up | `config.rs:128-136` |
| L4 | Low | Hoist the loop-invariant HISAT2 denominator assertion out of the `best`/`sec` loops; note that the sweep pins the denominator, not the HISAT2 ladder | `mapq.rs:641-676` |
| L5 | Low | Add one non-default `(i, s)` to the reference grid | `mapq.rs:433-465` |
| L6 | Low | One HISAT2 caveat sentence on the `--local` docs page | `docs/…/options/alignment.md:106` |
| L7 | Low | Pin one `==diff` second-best leaf (e.g. `(25, 50, Some(49)) → 32`) | `mapq.rs:475-500` |
| L9 | Low | Note upstream's `(double)0.8f` thresholds in the reference's caveat | `mapq.rs:414-421` |

**Nothing here blocks the semantics.** H1 is a gate hole, not a bug: the shipped wiring is correct — I verified `resolve` passes `score_min_form` through and that `score_min_params` returns `Log` iff `cli.local && aligner == Bowtie2`, the same predicate that emits `--local`. H1 is that nothing would *notice* if that stopped being true, in the one place the plan singled out as the failure shape that let #1079 ship.

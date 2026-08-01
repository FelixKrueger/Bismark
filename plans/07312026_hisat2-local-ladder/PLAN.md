# PLAN — HISAT2 `--local`: use the end-to-end MAPQ ladder

**Issue:** [#1080](https://github.com/FelixKrueger/Bismark/issues/1080) (deferred from [#1079](https://github.com/FelixKrueger/Bismark/issues/1079))
**Type:** correctness/consistency fix, HISAT2 `--local` only
**Scope:** small — one derivation change plus recomputed test expectations

---

## 1. Goal

Bismark applies Bowtie 2's **local** MAPQ ladder to HISAT2 `--local`. HISAT2 itself would use the **end-to-end** ladder, because both Bowtie 2 and HISAT2 select the ladder from whether scoring is monotone (`unique.h:236`, `if(sc_.monotone)`) — and HISAT2's scoring is *always* monotone.

Switch HISAT2-local to the end-to-end ladder.

### Why (settled during the #1080 investigation)

HISAT2's perfect alignment score is **0**, definitively — not "unknown" as Bismark's `--local` help still claims:

- `--local` is **commented out** of HISAT2's option table (`hisat2.cpp:639`); only `--end-to-end` is live, so `localAlign` stays `false`.
- `--ma` *is* accepted, but before scoring is built (`hisat2.cpp:3916`) HISAT2 forces it to zero and warns:
  > `Warning: Match bonus always = 0 in --end-to-end mode; ignoring user setting`
- The only path to `localAlign = true` is `--bwa-sw-like`, which Bismark never passes (verified).

So `monotone` is always true ⇒ `perfectScore = 0` ⇒ HISAT2's `diff` is `abs(scMin)` (what Bismark already does, correctly) **and** HISAT2 selects the end-to-end ladder (what Bismark does *not* do).

The decisive practical argument: **for HISAT2, `--local` changes the MAPQ scale without changing the score regime.** `AS ≤ 0` and `diff = abs(scMin)` in both modes, yet Bismark caps MAPQ at 44 with `--local` and 42 without — a different ceiling for no scoring reason. Bowtie 2's local ladder exists for the positive-score regime, which HISAT2 never enters.

The present state also pairs a local ladder with a zero perfect score, a combination neither upstream aligner can produce.

### Honest limitation (state it, don't bury it)

There is **no ground-truth oracle**. Bismark recomputes MAPQ across the 2–4 strand instances and discards HISAT2's own per-instance value, so "match HISAT2's ladder choice" is an argument from consistency with the score regime, not from measurement. This is weaker than #1079, where Bismark's own docs already specified the correct formula. Recorded as assumption A1.

### Timing

#1079's fix (#1083) already changes HISAT2-local MAPQ (the `scMin` form) and is merged to `dev` but **not released**. Doing this now means HISAT2-local users re-baseline **once** rather than twice.

---

## 2. Context

| File | Role |
|---|---|
| `rust/bismark/src/aligner/config.rs` | `ScoreModel` — `local_ladder` field + `from_emitted`; `local_ladder()` accessor |
| `rust/bismark/src/aligner/mapq.rs` | `calc_mapq` dispatches on `model.local_ladder()`; the HISAT2 test whose expectations move |
| `bismark` (Perl, root) | `--local` help text still says HISAT2's best score "is currently not exactly known" — now known |
| `CHANGELOG.md`, `docs/src/content/docs/options/alignment.md` | user-visible notes |

Both ladders themselves are untouched. `calc_mapq_local` and `calc_mapq_end_to_end` stay verbatim ports.

---

## 3. Behavior

**Derive the ladder from the match bonus** rather than carrying it as an independent field, mirroring upstream exactly:

```rust
/// Bowtie 2 and HISAT2 both select the ladder from whether scoring is monotone
/// (`unique.h:236`, `if(sc_.monotone)`) — i.e. from the match bonus. A zero perfect
/// score means scores only ever go down, which is the end-to-end regime.
pub(crate) fn local_ladder(&self) -> bool {
    self.match_bonus > 0.0
}
```

and delete the `local_ladder` field.

### Resulting per-mode table

| Mode | `match_bonus` | ladder before | ladder after |
|---|---|---|---|
| Bowtie 2 end-to-end | 0 | end-to-end | end-to-end (unchanged) |
| Bowtie 2 `--local` | 2 | local | local (unchanged) |
| HISAT2 end-to-end | 0 | end-to-end | end-to-end (unchanged) |
| **HISAT2 `--local`** | **0** | **local** | **end-to-end ← the only change** |
| minimap2 / rammap | 0 | end-to-end | end-to-end (unchanged) |

**Only HISAT2-local changes.** Everything else is byte-identical, including the whole default path.

### Why derive rather than keep two fields

Keeping both would leave them redundant today and re-open the "constructible in a state the aligners cannot produce" objection that shaped #1083's design. Deriving makes the invariant unrepresentable.

**Consequence to record (A2):** if [#1081](https://github.com/FelixKrueger/Bismark/issues/1081) later gives minimap2/rammap a nonzero match bonus, they would *also* pick up the local ladder. That is what Bowtie 2's own logic implies for a non-monotone aligner, and it is precisely the question #1081 has to decide — so coupling surfaces the decision rather than hiding it. A comment on `local_ladder()` says so.

### Edge cases

| Case | Handling |
|---|---|
| Bowtie 2-local | `match_bonus = 2 > 0` ⇒ local ladder. Unchanged. |
| All end-to-end | `match_bonus = 0` ⇒ end-to-end ladder. Unchanged — same branch, same arithmetic. |
| `--combined_index` | `--local` rejected (`config.rs:531-537`), so `match_bonus` is always 0 there. Unaffected. |
| minimap2/rammap | `--local` rejected; `match_bonus = 0`. Unaffected today (see A2). |
| 5-Base | Never calls `calc_mapq`. Unaffected. |

---

## 4. Implementation outline

1. **`config.rs`** — delete the `local_ladder` field; construct without it; make `local_ladder()` return `self.match_bonus > 0.0` with the `unique.h:236` rationale and the #1081 note. Update `ScoreModel`'s doc comment (it currently describes `local_ladder` as a stored flag).
2. **`mapq.rs`** — recompute the HISAT2 expectations in `local_hisat2_uses_the_linear_form_and_end_to_end_ladder` (hand-derived, see §6), and rename it to say what it now pins. Update `score_model_construction_matrix`: HISAT2-local must assert `!local_ladder()`.
3. **Docs** — retire "for HISAT2, it is currently not exactly known how the best alignment is calculated" in the Perl `--local` help and in `docs/.../options/alignment.md`; replace with the confirmed value and its consequence.
4. **CHANGELOG** — under the existing `## Unreleased`, fold into the HISAT2 bullet added by #1083 rather than adding a competing one, so the release reads as one HISAT2-local change.
5. **No version bump** — release cut, as with #1083.

---

## 5. Assumptions

| # | Assumption | Status |
|---|---|---|
| A1 | "Match HISAT2's ladder choice" is a consistency argument, not a measured one — Bismark discards HISAT2's own MAPQ | ⚠️ Stated, not resolvable; the honest basis for this change |
| A2 | Coupling the ladder to `match_bonus` pre-commits minimap2/rammap to the local ladder *if* #1081 gives them a bonus | ✅ Deliberate; documented in code and #1081 |
| A3 | HISAT2's perfect score is 0 and cannot be changed via Bismark | ✅ Verified in `hisat2.cpp:639, 3916` + Bismark never passes `--bwa-sw-like` |
| A4 | Both ladder functions are correct and stay untouched | ✅ Diffed leaf-by-leaf against `unique.h` during #1079 |
| A5 | No consumer depends on HISAT2-local's 44 ceiling | ⚠️ Unverifiable; the stated risk of this change |

---

## 6. Validation

Hand-derived from the linear `scMin` with the end-to-end ladder (computed independently, not read back from the implementation):

| Cell (`len, mate2, AS, second`) | diff | bestOver | before | **after** |
|---|---|---|---|---|
| `50, None, 0, None` | 10 | 10 | 44 | **42** |
| `50, None, -1, None` | 10 | 9 | 44 | **42** |
| `150, None, 0, None` | 30 | 30 | 44 | **42** |
| `50, None, 0, Some(-1)` | 10 | 10 | 31 | **30** |
| `50, None, -1, Some(-1)` | 10 | 9 | 1 | **1** (unchanged) |
| `150, Some(150), 0, Some(-1)` | 60 | 60 | 11 | **6** |
| `100, None, 0, Some(-3)` | 20 | 20 | 31 | **30** |
| `100, None, 0, Some(-5)` | 20 | 20 | 32 | **31** |
| `100, None, 0, Some(-1)` | 20 | 20 | 11 | **6** |

| # | Verify | Expected |
|---|---|---|
| **V1** | HISAT2-local now uses the end-to-end ladder | The nine cells above; the `1` cell is a useful control (same under both ladders) |
| **V2** | Bowtie 2-local is **unchanged** | `local_bowtie2_matches_independent_reference` + `local_bowtie2_denominator_uses_perfect_score_issue_1079` + `local_bowtie2_ln_derived_bucket_boundary` pass untouched |
| **V3** | Every end-to-end path is unchanged | `end_to_end_matches_the_pre_fix_formula` passes untouched (6 `--score_min` cells × `len 1..=500` × SE/PE) |
| **V4** | HISAT2-local's **denominator** is still `abs(linear scMin)` | `hisat2_local_denominator_is_abs_of_the_linear_scmin` passes untouched — this change touches the ladder only |
| **V5** | Construction matrix reflects the new derivation | HISAT2-local ⇒ `!local_ladder()`; Bowtie 2-local ⇒ `local_ladder()`; all four aligners end-to-end ⇒ `!local_ladder()` |
| **V6** | The BAM-level wiring tests still pass | Both `bowtie2_local_mapq_*_end_to_end` tests unchanged (Bowtie 2, so unaffected) |
| **V7** | Teeth | Reverting `local_ladder()` to a stored `local` flag must fail V1 |

Full suite + `cargo fmt --check` + `cargo clippy --all-targets` clean.

---

## 7. Questions / risks

| Priority | Item |
|---|---|
| **Open** | A5 — nobody should depend on the 44 ceiling, but it is unverifiable. The mitigation is the CHANGELOG being explicit that HISAT2-local MAPQ changes. |
| **Noted** | A2's coupling is deliberate and hands #1081 a decision rather than making one. |
| **Resolved** | Perfect score = 0 (A3). The "not exactly known" doc caveat is retired here. |

---

## 8. Self-Review

- **Logic:** only the ladder selector changes; both ladders and the `normalize` denominator are untouched, so HISAT2-local's `diff` stays `abs(scMin)` and only the mapping from ratios to MAPQ moves. Verified the change is inert for every other mode via the per-mode table in §3.
- **Efficiency:** removes a field (32 → 24 bytes plus padding) and one branch's worth of state. No runtime cost.
- **Edge cases:** the `50, None, -1, Some(-1)` cell returns `1` under *both* ladders, so it is kept as a control that the test is exercising the right cells rather than merely re-baselining everything.
- **Risk:** a second consecutive MAPQ change to HISAT2-local. Mitigated by bundling into the same unreleased cycle as #1083 so users see one re-baseline, and by folding into that CHANGELOG bullet rather than adding a second.

---

## 9. Post-review fixes (2026-07-31)

Dual code review (`CODE_REVIEW_A.md` / `CODE_REVIEW_B.md`) — both verdicts **correct, ship it**, no Critical findings. All findings applied. No contradictions between reviewers; each caught something the other missed.

### A closed the concern this plan raised

Reducing upstream's `monotone = matchType == COST_MODEL_CONSTANT && matchConst == 0` to `match_bonus > 0.0` looked like dropping a conjunct. It isn't: `scoring.h:163` and `:197` **hard-code** `matchType = COST_MODEL_CONSTANT` and `:227` asserts `matchConst >= 0`, so `monotone ⟺ !(match_bonus > 0)` is **exact** and no caller can construct a counterexample. A also checked a path this plan missed — `-P/--preset` *is* live, but presets only emit policy tokens, so even `-P sensitive-local` leaves `localAlign == false`.

### The finding that mattered most (A HIGH-1) — the floor, not the ceiling

The local ladder's no-second-best floor is `22`; the end-to-end one is `0`. So a **uniquely aligned** HISAT2-local read with `bestOver/diff < 0.3` now returns **0**, which it previously never could — and MAPQ 0 is treated as discard by `samtools view -q 1` and methylseq's filters. Worse, nothing tested it: all three no-second-best cells sat on the top rung, leaving the entire changed sub-ladder (`40/24/23/8/3/0` for `42/41/36/28/24/22`) unexercised.

**A5 in this plan is why it was missed** — it named "the 44 ceiling" as the risk, and the ceiling is what then got tested and documented. Enumerating one instance of a class licensed ignoring the rest of it. Fixed: two cells added (`AS -5 → 23`, `AS -8 → 0`, both hand-derived) and the CHANGELOG now states the floor move as the consequential one.

### B H1 — the change was invisible at BAM level

Two `--hisat2 --local` integration tests already read the output BAM and both of their MAPQ values move with this commit, but neither asserted MAPQ — so the ladder flip passed through unexercised end-to-end, the exact standard #1079 established. Assertions added to both.

**B's predicted PE value was wrong and re-deriving caught it.** B expected 40→39 by summing `ZS:i:-2` from both mates. Bismark **masks read 1's `ZS`** on the HISAT2 PE path, so `sum_second = as1(0) + zs2(-2) = -2`, giving `bestDiff = 2` against `diff = 2.4` → the 0.8 bucket → **38** (local ladder: flat 39). The assertion is 38 with that derivation in the comment; had the observed value simply been pasted in, the reasoning would have been wrong even though the number was right.

### Agreed by both — the CHANGELOG overclaimed

The headline asserted HISAT2-local "now matches what HISAT2 itself would compute", contradicting A1 in this plan. Between them the reviewers listed four divergences from HISAT2's own `mapq()`: integer `scMin` truncation; no `max(1, …)` clamp on `diff`; a **60** early return for a unique alignment where no second-best was sought (Bismark never returns 60, so the claim was false for the commonest case); and cross-instance aggregation of best/second-best. Retitled to "uses the MAPQ ladder HISAT2 itself would select", with the limitation stated inline.

### Also applied

| Finding | Fix |
|---|---|
| B M1 | `mapq.rs` header now lists **three** deliberate `--local` deviations and notes the local ladder is Bowtie 2-only |
| B M2 | `calc_mapq_local`'s doc no longer says the form/denominator are "aligner-dependent" — it is Bowtie 2-only |
| B M3 | stale references to the renamed test (`mapq.rs`, this plan) |
| B M4 | docs + Perl help: `--local` still enables soft-clipping for HISAT2; "exposes no `--local` **option**" (HISAT2 does have local DP via `--bwa-sw-like`) |
| B L1 | note that `hisat2_local(i,s) == end_to_end(i,s)` now holds, so that equality assertion is weaker than it reads |
| B L2 | pre-existing error in the #1079 CHANGELOG bullet: `39` is the unconditional 0.8 rung, not a `best_over == diff` rung |
| B L3 | `from_emitted` records the `--ma`/`--bwa-sw-like` assumption and where to guard it |
| B L4 | **A2 is enforced, not just documented** — `score_model_construction_matrix` asserts `!local_ladder()` for all four aligners end-to-end, so #1081 cannot add a match bonus without failing a test. Corrects this plan's weaker claim. |

**Not applied:** B L5 (a `monotone()` reading helper) — explicitly optional, and B warned against folding the two `> 0.0` predicates since they encode different decisions that merely coincide. B L6 was checked and dismissed by B itself.

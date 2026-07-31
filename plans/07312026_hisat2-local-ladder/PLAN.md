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
2. **`mapq.rs`** — recompute the HISAT2 expectations in `local_hisat2_uses_the_linear_form_it_was_emitted` (hand-derived, see §6), and rename it to say what it now pins. Update `score_model_construction_matrix`: HISAT2-local must assert `!local_ladder()`.
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

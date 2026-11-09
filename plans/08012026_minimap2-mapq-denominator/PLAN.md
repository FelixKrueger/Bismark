# PLAN — minimap2/rammap MAPQ denominator: `perfect = 2 × read_length`

**Issue:** [FelixKrueger/Bismark#1081](https://github.com/FelixKrueger/Bismark/issues/1081) (deferred from [#1079](https://github.com/FelixKrueger/Bismark/issues/1079))
**Type:** correctness fix, minimap2 + rammap only — **in a default (non-`--local`) path**
**Revision:** **rev 3** — dual code review + coverage audit folded (`CODE_REVIEW_A.md`, `CODE_REVIEW_B.md`, `COVERAGE.md`); plan reviews folded in rev 2; decisions locked in rev 1 (see §13)
**Spike:** `SPIKE.md` (same directory) — the `end_bonus` blocker is cleared; findings F1–F8 are load-bearing throughout.
**Base:** `dev` `172da96`, branch `plan/mapq-minimap2-denominator`

**Decisions locked (Felix, 2026-08-01):**

| # | Decision | Consequence |
|---|---|---|
| **D-PARITY** | **Ship unconditionally.** No compat flag, no opt-in. | The shipped "minimap2 SE is byte-identical to Perl v0.25.1 + minimap2 2.31-r1302" claim is **retired**, not narrowed — end-to-end is minimap2's only mode. §7 step 8 restates it; §7 step 9 says so in the CHANGELOG. |
| **D-LADDER** | **Local ladder.** | `local_ladder()` needs **no code change at all** — #1088's `match_bonus > 0.0` derivation already produces it. Ceiling 44, **floor 22**. Four comments that assert the local ladder is "Bowtie 2 only" become false and must be rewritten (§7 step 2). |

The end-to-end-ladder columns are kept throughout as the **record of the rejected alternative** (§3.4) and as the justification for the floor assertions — not as a live option.

---

## 1. Goal

`calc_mapq` normalizes MAPQ by `diff`, the score range a valid alignment can span. For modes whose best possible score is 0 that is `abs(scMin)` — which assumes `AS ≤ 0`. **minimap2 and rammap report positive `AS:i:`** and route through `calc_mapq` with `match_bonus = 0.0`, so

```
bestOver / diff = (AS + 0.2·len) / (0.2·len) = AS/(0.2·len) + 1   >  1   always
```

Give minimap2/rammap `perfect = 2 × read_length` and Bowtie 2's denominator `max(1, perfect − scMin)`, so the ratio lands in `(0, 1]` and the ladder discriminates.

### The defect has two shapes, not one (spike F6)

| branch | reached when | today | after (local ladder) |
|---|---|---|---|
| **no second best** | the read aligns in one instance only, **or a later-slot instance does not strictly out-score every earlier one** | **42**, degenerate at every length and every score | 44 / 44 / 41 / 28 / 22 across 100 → 20 % of perfect |
| **with second best** | a **later**-slot instance **strictly** out-scores every earlier one | **33** for almost anything, **25** for near-ties — not pinned, but scored on a denominator ~11× too small | near-tie 200 vs 190: **25 → 11**; clear winner 200 vs 20: **33 → 39** |

The second branch moves in **both** directions. #1081's own text describes only the first, and a test set that covers only unique reads repeats #1080's miss (a risk note that enumerates one instance licenses ignoring the class).

> **⚠️ Rev 2 correction (both reviewers, independently — A I6 / B C2).** Rev 1 said the second branch is reached "whenever a read aligns in both instances". **False, and it is the root cause of the rev-1 cell (d) defect.** `alignments` only ever receives the running maxima in slot order: `overwrite` is set only when `alignment_score >= best_as_so_far` (`merge.rs:254-280`) and insertion requires `overwrite` (`:283-310`). So a later instance carrying a *lower* AS is **never stored**, an equal one is `Decision::Ambiguous` (`:338-342`), and same-`chromosome:pos` pairs collapse on the HashMap key (`:399`). For SE-directional (slots `[CT, GA]`, `mod.rs:769`) the runner-up branch therefore fires only when **`AS(GA) > AS(CT)`** — roughly **half** of the reads that align in both.
>
> Consequences, all folded below: the near-tie `25 → 11` improvement lands for ~half of near-tie reads while the other half moves `42 → 44`, so the **strand-order artefact widens from a 17-point spread to 33** (§3.5); "the larger behavioural change" is a *condition*, not a population, and §7 step 9's CHANGELOG wording must not imply otherwise; and non-directional SE has 4 slots (`mod.rs:777`), where the running-maxima rule bites differently. The artefact is faithful pre-existing Perl behaviour — this fix widens it, it does not create it.

### What this fix is, and is not

**It is not agreement with minimap2.** minimap2 derives MAPQ from **chain** scores on a **0–60** scale and has no Bowtie-family ladder at all — the spike observed its own MAPQ at 4, 17, 18, 56, 59, 60 on the same reads Bismark would score 42. #1079 could cite Bismark's own `--local` help text as specifying the correct formula; #1080 could cite HISAT2 selecting its ladder from `monotone`. **Neither is available here.** What the fix buys is that Bismark's MAPQ becomes *discriminating and internally consistent on Bismark's own scale* — a real improvement over a constant, and nothing more. Say exactly that in the CHANGELOG; do not write "now matches what minimap2 would compute" (the sentence #1080's reviewers had to strike).

**Pass-through is not the easy way out.** Using minimap2's own MAPQ fails for the reason Bismark recomputes at all: Bismark runs 2–4 strand instances and picks the best across them, so a per-instance MAPQ does not describe the cross-instance decision. (It is also on a different scale, 0–60.)

**`--five_base_min_mapq` is untouched** and is *not* an argument for either ladder — the 5-Base path never calls `calc_mapq` (§5, spike F8).

### Explicit non-goals

| Non-goal | Why |
|---|---|
| Making `scMin` real for minimap2 | minimap2 **never receives `--score-min`** (the clean slate at `options.rs:233` discards the base string), yet `calc_mapq` uses `(0.0, −0.2)`. Keeping `scMin` in the denominator is the faithful analogue of Bowtie 2's `perfect − scMin`. Recorded as **A6**. ⚠️ **Rev 2: the accepted cost is larger than rev 1 believed and is now measured — see the box below.** |

> **⚠️ Rev 2 — rev 1 had the `--score_min` direction backwards (A I1, unique to Reviewer A; independently re-measured).** Rev 1's §3.5 said a steep `--score_min` "inflates `diff` and pushes everything toward the floor". It does the **opposite**, and at the extreme it **undoes the fix**:
>
> | `--score_min` | MAPQ across 100 → 20 % of perfect (len 100) |
> |---|---|
> | `L,0,-0.2` (default) | 44 / 44 / 41 / 28 / 22 ← the intended fix |
> | `L,0,-1` | 44 / 44 / 42 / 41 / 28 |
> | `L,0,-5` | 44 / 44 / 44 / 44 / 42 |
> | `L,0,-20` | **44 / 44 / 44 / 44 / 44** ← #1081 fully restored, silently |
>
> `bestOver = AS − scMin` and `diff = perfect − scMin` **both** contain `−scMin`, so `ratio = (AS + |scMin|)/(perfect + |scMin|) → 1` as `|scMin|` grows. The reasoning that misled rev 1 was its own justification — "the faithful analogue of Bowtie 2's `perfect − scMin`" is structurally true but **behaviourally inverted**, because Bowtie 2-local's `scMin` is *positive* (`G,20,8`), so there a steeper function *shrinks* `diff` and **spreads** MAPQ (measured: `G,20,30` → 44/22/22/22/22), whereas minimap-like's `scMin` is *negative*, so a steeper slope inflates both terms and **compresses upward**.
>
> **Decision: keep the non-goal, document the sensitivity precisely, and pin it with a test** (§9 V14). Rationale: a `scMin`-free normalization is not the one-line change it appears to be — dropping `scMin` from `diff` alone leaves `bestOver = AS − scMin`, so a perfect read gives `ratio = 220/200 = 1.1 > 1` and the top rung saturates again; doing it properly means `bestOver = AS`, `diff = perfect`, which is a *different* normalization from Bowtie 2's rather than an adaptation of it. That is a separate issue with its own re-baseline, exactly as §3.4 says of filterability. Practical exposure is low but real: methylseq emits non-default `L,0,−N` forms, and at `L,0,−0.6` the ladder still discriminates (44/44/**41**/36/24 — rev-3 correction from Reviewer A: at len 100 `bestOver/diff = 180/260 = 0.692 < 0.7`, so it is the 0.6 rung, not the 0.7 rung); only steep slopes collapse it.
| Matching minimap2's integer arithmetic | Same non-goal as #1079's (A10): Bismark computes in `f64`. |
| Fixing the in-process rammap preset bug | `--rammap` in-process hard-codes `Preset::MapOnt` and silently ignores `--mm2_short_reads`/`--mm2_pacbio` (spike F5). Real and separate. It *reduces* `end_bonus` exposure — but ⚠️ **rev 2 (B O15): this fix slightly WORSENS it.** The silently-ignored flag now also moves the MAPQ column, because in-process (`map-ont`) and subprocess (`sr`) produce different `AS` distributions through a denominator that is now `AS`-sensitive. Rev 1's "reduces exposure" framing was one-sided. ✅ **Filed as [#1092](https://github.com/FelixKrueger/Bismark/issues/1092)**, stating the worsening. |
| A compat flag for the old MAPQ | Declined under D-PARITY, consistent with #1079's "unconditionally, no compat flag" for the same defect class. |

---

## 2. Context

### Spike outcome (the reason this plan can be written at all)

`-x sr` sets `end_bonus = 10`, which fed minimap2's DP and could have pushed `AS` above `2 × len`. It does not:

- `AS:i:` prints `r->p->dp_score` (`format.c:403`); all five writes to `dp_score` are `+= ez->max` or `+= ez->score` (`align.c:794,861,869,886,950`) — no `end_bonus` term.
- Inside ksw2, `end_bonus` appears in exactly one executable line, a **traceback-start decision**: `ez->mqe + end_bonus > (int)ez->max` (`ksw2_extz2_sse.c:304`). It chooses *which* alignment is reported, never the score added.
- Empirically (minimap2 **2.31-r1302**, Bismark's verbatim option string, 64 alignments): 0 cells with `AS > 2·len`, and a perfect read under `-x sr` scores **exactly** `2·len` at 5 lengths — a `+10`/`+20` leak would be visible in that one number.
- `a = 2` for **every** preset Bismark can select (`map-ont` inherits `opt->a = 2`; `map-pb` overrides index options only; `sr` sets it explicitly). One `2.0` constant suffices.
- **The block-partition step, which the spike asserted but did not show** (supplied by Reviewer B, verified): `mm_align1` scores the left extension over query `[qs0, qs)` (`align.c:779-798`), the gap-fill loop over `[qs, qe)` advancing `rs = re, qs = qe` per iteration (`:800-866`), and the right extension over `[qe, qe0)` (`:871-889`), with `assert(qe1 <= qlen)` at `:890`. No block overlaps another ⇒ `Σ ez->max ≤ a·qlen`. Z-drop cannot double-add (the second pass at `:842` overwrites `ez` before the single `+=`); a Z-drop split is a *separate record* with its own `dp_score` over its own sub-range; `mm_update_dp_max` (`:1022-1044`, `:1092-1094`) writes `dp_max`, **not** `dp_score`.
- **rammap: the conclusion holds, but rev 1 cited the wrong field** — see the box.

> **⚠️ Rev 2 (B I5, unique to Reviewer B; independently verified).** rammap's naming is **transposed** relative to minimap2, and the spike traced the wrong field. `AS:i:` is `r.align_score` (`pipeline.rs:2483`, `:2598`), which is what `Mapping.score` carries (`api.rs:158-159`) and therefore what the in-process backend emits (`inprocess.rs:252,258`). The field *named* `dp_score` in `align/extend.rs` — the one SPIKE F5 traced — is rammap's analogue of minimap2's `dp_max`, and it is what receives `update_dp_max`, the splice bonus (`pipeline.rs:1771-1777`) and the jump adjustments (`jump.rs:347-352`, `:462-467`).
>
> Two consequences:
> 1. **One additive non-match term sits on rammap's AS path and is NOT splice-gated** (`pipeline.rs:1070-1074`): `align_score = ez.max + gap_open + gap_extend * stripped_gap_len` when a leading gap was stripped. Its own comment says *"Add back the cost of the stripped leading gap"*, i.e. it restores a penalty that was definitely subtracted, which preserves `AS ≤ a·qlen` — but **the spike never made that argument because it never looked at this field.** Recorded as A3's unquantified residual, rammap only. Blast radius if it is loose: a few reads get `bestOver > diff` ⇒ the local ladder's top rung 44, i.e. today's behaviour, no panic (§3.5 covers that shape).
> 2. **The `match_score` citation was for the wrong preset.** `api.rs:690,692` is the **`sr`** branch; the in-process default hard-codes `Preset::MapOnt`, and `map-ont` sets only `k`/`w` (`api.rs:617-619`). The load-bearing constant for the default path is the struct default `match_score: 2` at **`align/map.rs:215`** (verified: 2). The fact holds; rev 1's citation did not support it.

### Files

| File | Role |
|---|---|
| `rust/bismark/src/aligner/config.rs` | **`ScoreModel`** (L82-225). `from_emitted` `match_bonus` arm **L123-127** ← the whole semantic change. `BOWTIE2_LOCAL_MATCH_BONUS` L80. `end_to_end()` L131-143 — its doc **already flags this issue** as the thing that may invalidate its aligner-independence. `local_ladder()` L161-176 — its doc **already names #1081** as the decision to make. Construction point **L793-802**. `--local` reject for minimap2/rammap: rationale comment `:673`, the actual `matches!` guard **`:677-684`** (rev 1 cited the comment line). PE reject `reject_unsupported_paired_aligner`. **Three field/constructor doc comments become false — `:90-91`, `:98`, `:117-119`** (§7 step 2). `perfect()` is **private** (`:196`, no `pub(crate)`) — constrains where V9 can live. |
| `rust/bismark/src/aligner/mapq.rs` | Header L1-19 (lists the deliberate deviations — gains a fourth). `calc_mapq` L29-43. Ladders L48-135 / L143-220 — **both untouched**. `score_model_construction_matrix` **L661-699** ← fails by design today. `end_to_end_matches_the_pre_fix_formula` L711-736 — stays green, **coverage narrows**. |
| `rust/bismark/src/aligner/options.rs` | `minimap2_options` L256-289 (closed string, `-x` preset selectors only). `score_min_params` L376-404 (returns `Linear`, `(0.0, −0.2)` for minimap2 — the fiction, A6). Clean slate L233. |
| `rust/bismark/src/aligner/merge.rs` | `calc_mapq` SE call site **L367**; `second_for_mapq` derivation **L332-350**. ⚠️ **The running-maxima gate that makes cell (d) buildable or not: `overwrite` at `:254-280`, insertion at `:283-310`, tie→`Ambiguous` at `:338-342`, HashMap key `"{chromosome}:{pos}"` at `:399`.** PE site L740 unreachable here. Slot order from `se_instance_plan` (`mod.rs:769` directional, `:777` non-directional). Only the **first** record per read per instance is considered (`:312-315`) — the mechanism A9 actually rests on. |
| `rust/bismark/src/aligner/combined.rs` | **Two further `calc_mapq` call sites, L339 + L711** — rev 1 missed both. Unreachable for these aligners (`reject_combined_index_unsupported` requires Bowtie 2/HISAT2, `config.rs:929,983`), so the scope conclusion survives, but "traced every call site" must name all **four**. |
| `rust/bismark/src/aligner/methylation.rs` | `:155-158` — index 1 prepends two genomic bases and bails with `extracted: false` when `pos < 2`. Sets cell (d)'s POS floor. |
| `rust/bismark/tests/aligner_cli.rs` | `make_fake_minimap2_mapped` **L2671-2688** (`AS:i:12`, 6 bp, CT-only → GA unmapped); `make_fake_rammap_mapped` L2811 (same scores); `make_genome_mmi`'s `chr1` is **8 bp** (`:2656`). **No minimap2/rammap MAPQ assertion exists anywhere** — but rev 1 undercounted the existing ones: there are **≥7**, at L165, L241, L602, **L885, L886** (Bowtie 2 PE), **L2269, L2367** (HISAT2-local SE/PE — #1080's BAM-level ladder gates, whose messages literally say *"44 means the local ladder is still selected"*), L2630. All Bowtie 2/HISAT2, so the load-bearing conclusion holds; L2269/L2367 belong in V11. |
| `CHANGELOG.md` | `## Unreleased` → `### bismark (aligner)` already carries a **placeholder bullet** ("Related and deliberately unchanged: minimap2/rammap … tracked in #1081"). **Replace it** — do not add a competing bullet. |
| `rust/README.md` L161 (aligner row) + Milestones | The minimap2 byte-identity claim lives in the row; the row rule is per-module-merge-PR. |
| `docs/src/content/docs/options/alignment.md` L111 | The MAPQ paragraph; currently Bowtie 2/HISAT2 only. |

### Blast radius

**Small, and nothing like #1079's.** One `match` arm, one constant, one named constructor, one rewritten test, N new tests, 4 doc surfaces. `ScoreModel`'s shape, `normalize`, `perfect`, both ladders, every signature and every call site are untouched — #1079 already built the seam this fix drops into. **Therefore a single PR**, not #1079's two: its split existed to isolate ~50 threading edits, and there are none here.

### Not in play

`--local` (rejected for both aligners, `config.rs:674`) · PE (rejected, `reject_unsupported_paired_aligner`) · `--combined_index` (rejected) · Bowtie 2 and HISAT2 in any mode · 5-Base (§5) · `--multicore`/worker-invariance (MAPQ is per-read).

---

## 3. Behavior

### 3.1 The change

`from_emitted` gains one arm:

```rust
match_bonus: match aligner {
    Aligner::Bowtie2 if local => BOWTIE2_LOCAL_MATCH_BONUS,
    // minimap2/rammap always score matches positively — there is no end-to-end
    // mode to fall back to (`--local` is rejected because they are local by
    // design), so this is NOT gated on `local` (#1081).
    Aligner::Minimap2 | Aligner::Rammap => MINIMAP2_MATCH_BONUS,
    _ => 0.0,
},
```

Everything else follows from code that already exists:

- `perfect()` = `match_bonus × (len1 + len2.unwrap_or(0))` → `2 × len` (SE only here).
- `normalize()` takes the `match_bonus > 0.0` branch → `diff = max(1, perfect − scMin)`.
- `local_ladder()` returns `true` → the local ladder. **No change to that function** — #1088 already derives it from `match_bonus > 0.0`, which is precisely why it documented #1081 as the decision it was forcing (D-LADDER; §3.4 records why the alternative was rejected).

### 3.2 Per-mode table after the change

| Mode | `scMin` form | `match_bonus` | `diff` | ladder |
|---|---|---|---|---|
| Bowtie 2 end-to-end | linear | 0 | `abs(scMin)` | end-to-end — **frozen** |
| Bowtie 2 `--local` | ln | 2.0 | `max(1, 2·len − scMin)` | local — unchanged (#1079) |
| HISAT2 end-to-end | linear | 0 | `abs(scMin)` | end-to-end — **frozen** |
| HISAT2 `--local` | linear | 0 | `abs(scMin)` | end-to-end — unchanged (#1080) |
| **minimap2 / rammap** | linear | **2.0** | **`max(1, 2·len − scMin)`** | **local ← the only change** |

Only minimap2/rammap move. Every Bowtie 2 and HISAT2 path is byte-identical **by construction** (a different `match` arm), not by sweep — the #1079 discipline: prefer invariants from code structure over invariants from test sweeps.

### 3.3 Hand-derived values (derive these before touching code; do not read them back from the implementation)

`len = 100`, default `L,0,−0.2` ⇒ `scMin = −20`, `perfect = 200`, **new `diff` = 220**, old `diff` = 20.

No second best:

| `AS` | % perfect | `bestOver` | ratio | old | **new / local** | new / e2e |
|---|---|---|---|---|---|---|
| 200 | 100 % | 220 | 1.000 | 42 | **44** | 42 |
| 160 | 80 % | 180 | 0.818 | 42 | **44** | 42 |
| 120 | 60 % | 140 | 0.636 | 42 | **41** | 24 |
| 80 | 40 % | 100 | 0.455 | 42 | **28** | 8 |
| 40 | 20 % | 60 | 0.273 | 42 | **22** | **0** |

With a cross-instance second best (`bestDiff = |AS − AS₂|`):

| `AS` | `AS₂` | `bestDiff` | old | **new / local** | new / e2e | why |
|---|---|---|---|---|---|---|
| 200 | 190 | 10 | 25 | **11** | 6 | `10 < 0.1·220` ⇒ `bestDiff > 0` leaf, `bestOver ≥ 0.5·diff` |
| 200 | 100 | 100 | 33 | **34** | 34 | `0.4` rung, `bestOver == diff` leaf |
| 200 | 20 | 180 | 33 | **39** | 38 | `0.8` rung (flat 39 in the local ladder) |
| 120 | 110 | 10 | 25 | **11** | 2 | as row 1 |
| 120 | 40 | 80 | 33 | **18** | 3 | `0.3` rung, `bestOver ≥ 0.5·diff` |

Length-invariance is structural: `AS`, `perfect` and `scMin` all scale with `len`, so every ratio above is length-free. The spike confirmed identical values at 50/100/150/250/1000 bp.

**Both reviewers re-derived all 30 values in these two tables plus the four BAM cells against an independently extracted Perl `calc_mapq` oracle (`bismark:3923-4186`), including the "why" column: no wrong value and no wrong reasoning.** The numbers are settled; what moved in rev 2 is *reachability* (which instance must carry which score) and the validation around them.

> ⚠️ **Reaching table 2 at all is order-dependent.** `AS₂` here is the cross-instance runner-up, and per the §1 correction it exists only when a **later** slot strictly out-scores every earlier one. A fixture or a mental model that puts the higher score on slot 0 lands in table 1, not table 2 — the rev-1 cell (d) defect exactly.

### 3.4 The rejected alternative: the end-to-end ladder (recorded, not open)

Considered and **not** chosen. Kept here so it is not re-litigated every review, and because the numbers justify §9's floor assertions.

**Why it was rejected.**
1. **It needs new mechanism.** `local_ladder()` could no longer be `match_bonus > 0.0`; minimap2/rammap would need it **false** with a nonzero bonus. That re-introduces the stored-vs-derived split #1088 deliberately removed, so the honest implementation is a *second* predicate (e.g. `monotone_ladder()`, keyed on the aligner) — not a resurrected field — and #1088's `unique.h:236` rationale would have to be contradicted rather than extended.
2. **It contradicts the rule the code already encodes.** Bowtie 2 and HISAT2 both select by `monotone`, i.e. by whether the match bonus is zero. minimap2 is non-monotone, so that rule returns the local ladder. (This is analogy, not correspondence — minimap2 uses no ladder — but it is the only rule in play, and #1080 applied the same rule in the opposite direction to reach the *end-to-end* ladder for HISAT2. Applying it consistently is what keeps the two issues from contradicting each other.)
3. **Its floor is 0.** A poorly-scoring *unique* minimap2 read would be newly discarded by `samtools view -q 1` and by methylseq's filters — the exact hazard #1080's code review flagged as its HIGH-1, here in a **default** path rather than an opt-in mode.

**What that choice costs — ⚠️ CORRECTED rev 3; rev 2 stated this wrongly and shipped the error into three user-facing surfaces.**

Rev 2 said: *"The local ladder's floor is 22, above a conventional `-q 20`, so a MAPQ filter remains close to a no-op … The fix restores discrimination but not filterability."* **The floor of 22 belongs to the no-second-best branch only.** Both code reviewers found this independently; re-measured over all 20,100 `(AS, AS₂)` cells at len 100:

| | reachable MAPQ on the second-best branch | cells below 20 |
|---|---|---|
| pre-fix | 6, 12, 17, 18, 21, 22, 25, 26, 27, 33 | **6.9 %** |
| post-fix | 2, 9, 11, 12, 14, 16, 17, 18, 19, 21, 25, 31–40 | **67.6 %** |

Three facts that make this a *scoping* correction rather than a reversal:

1. **Sub-20 values were already reachable pre-fix** (6, 12, 17, 18). This is not a new hazard class — the same class becomes ~10× more populated.
2. **It does not reopen D-LADDER.** Both ladders bottom out at 2 on the second-best branch; what separates them is the **unique**-read floor (local 22 vs end-to-end 0). Reasons 1–3 above are untouched. `0` and `1` remain unreachable for minimap-like either way, because they require `bestDiff == 0`, which is `Decision::Ambiguous`.
3. **The affected reads are gated by a condition, not spread across the run** — only where a later slot strictly out-scores every earlier one (§1).

**So the honest statement is:** a uniquely-aligned read cannot fall below 22, so for that majority a `-q 20` filter stays close to a no-op; reads with a competing cross-instance alignment are scored on the sub-rungs and **can fall to 2**, so `-q`-filtered counts *do* change for that subset. That is the fix working — a read matching two strand instances almost equally is not high-confidence and 25 overstated it — and it means the release notes can *claim* partial filterability rather than denying it. Grid density is **not** read density: neither reviewer could quantify the real-data population, and the 6.9 %→67.6 % figures must never be quoted as a read fraction. Quantifying it on real bisulfite data is a follow-up, not a blocker.

### 3.5 Edge cases

| Case | Handling |
|---|---|
| **Bowtie 2 / HISAT2 invariance** | Byte-identical **by construction** — different `match` arm, unchanged `_ => 0.0`. `end_to_end_matches_the_pre_fix_formula` is the regression net, not the proof. |
| `AS` **above** `perfect` (`bestOver > diff`) | Impossible: spike F1/F2 bound `AS ≤ 2·len` structurally for every reachable preset. If a future minimap2 broke that, `diff` stays positive and the ratio simply exceeds 1 again ⇒ silent top rung, no panic. **Not** guarded in code (a runtime clamp would hide the regression); guarded by V9's assertion instead. |
| `AS` below `scMin` | Unreachable in practice (minimap2's `min_chain_score`/`min_dp_max` filters keep `AS` well above 0 > `scMin`), and harmless: `bestOver` stays positive because `scMin < 0`. |
| Soft-clipped reads | `perfect` uses the **full** read length, so a soft-clipped alignment scores below perfect and gets a lower MAPQ. Exactly Bowtie 2-local's behaviour (`scoring.h:310-316` also uses full `rdlen`), and the conservative direction. |
| `-x sr` reports `AS` **above** its own CIGAR's score | Real (spike F4: `100M` with `AS:i:198` where the CIGAR scores 190). Bounded by `2·len`, so the denominator is unaffected. Consequence: **never derive an expected `AS` from a CIGAR** in a test for `-x sr`. |
| `len = 0` | At the **default** `--score_min`: `scMin = 0`, `perfect = 0`, `diff = max(1, 0) = 1`, `bestOver = AS`. Finite and total; no `ln(0)` because the form is linear. (Bowtie 2-local's `−inf` case does not arise.) ⚠️ Rev 2: `scMin = 0` assumes intercept 0 — with `L,10,−0.2` it is `10`. |
| `diff` clamp `max(1.0)` | ⚠️ **Rev 2 — rev 1 got this wrong twice (A I7 / B I8, independently).** Rev 1 claimed "reachable only for `perfect − scMin ∈ [0,1)` ⇒ `len = 0`; effectively dead", a derivation valid only at the default `(0, −0.2)`. `--score_min` is shape-validated only (`options.rs:410-414`), so with the legal **`L,10,−0.2`** — literally a cell in the repo's own `SCORE_MIN_CELLS` (`mapq.rs:237`) — `perfect − scMin = 2.2·len − 10`, so the clamp **fires for every `len ≤ 4`**, and the quantity is **negative**, not in `[0,1)`. With `L,100,−0.2` at 6 bp it is `−86.8`, `bestOver` goes negative and every read floors at 22. **The clamp is live for minimap-like, not dead.** Outcome is benign (22 with or without it, verified at len 1–4), so this is a reasoning defect, not a behavioural one — but it is the same "derived at the default only" slip that produced the `--score_min` direction error. |
| Non-default `--score_min` | Legal (shape-validated only) and **still moves minimap2 MAPQ without moving an alignment** (A6). ⚠️ **Rev 2: a steep slope compresses MAPQ toward the CEILING, and `L,0,−20` restores the defect outright** — see the §1 box for the measured table and why the "faithful analogue" reasoning inverted it. Pinned by V14. |
| **Strand-order artefact (rev 2, A I6)** | Two reads with identical `(AS, AS₂)` get different MAPQ depending only on which strand won: OB (slot 1) winning by 10 at 100 bp ⇒ old 25, **new 11**; OT (slot 0) winning by 10 ⇒ no runner-up is ever recorded ⇒ old 42, **new 44**. The artefact is faithful pre-existing Perl behaviour; this fix **widens** the near-tie spread from **17** points (42 vs 25) to **33** (44 vs 11). Must be stated, not discovered by a user diffing two BAMs. |
| **`--ambig_bam` (rev 2, B O14)** | Unaffected: it copies MAPQ from field 4 of the raw aligner line (`output.rs:819`, `:833`), i.e. minimap2's own 0–60 value. Consequence worth stating rather than leaving to be found: a `--minimap2 --ambig_bam` run legitimately emits **two different MAPQ scales in two files**. |
| PE | Unreachable (rejected). `perfect()` sums both mates and would be correct if PE ever lands; **no PE expectation is written into any test**, per "confirm before writing PE behaviour into the plan". |
| in-process vs subprocess rammap | Same `ScoreModel` (both go through `config::resolve`), and the in-process crosscheck asserts field-identity on the **SAM record's** `mapq` (the aligner's own 0–60), not on Bismark's computed MAPQ — so `aligner_rammap_inprocess_crosscheck.rs` is unaffected. |
| 5-Base | Never calls `calc_mapq` (§5). |

---

## 4. Signature

No signature changes. One constant and one named constructor are added to `config.rs`:

```rust
/// minimap2's match score (`-A`, `options.c:47` `opt->a = 2`). Every preset Bismark can
/// select keeps it at 2 — `map-ont` is "the same as the default" (`:96`), `map-pb` overrides
/// index options only (`:103`), `sr` sets it explicitly (`:155`) — and there is no `-A`
/// passthrough (`options.rs:288` emits a closed string). rammap mirrors this (`api.rs:690`).
///
/// Deliberately a SEPARATE constant from `BOWTIE2_LOCAL_MATCH_BONUS` despite the equal
/// value: the two record unrelated upstream facts and can drift independently.
pub const MINIMAP2_MATCH_BONUS: f64 = 2.0;

impl ScoreModel {
    /// minimap2 / rammap (both minimap-like): linear `L` form from a `--score-min` the
    /// aligner never receives (`options.rs:233` discards it), positive perfect score
    /// `2·len`. `--local` is rejected for both, so there is no second variant. NOT
    /// `end_to_end()` — that is the zero-perfect-score model (#1081).
    pub fn minimap_like(intercept: f64, slope: f64) -> Self;
}
```

`end_to_end()` keeps its name and behaviour (~29 call sites, all Bowtie 2/HISAT2), and its doc comment — which today says *"End-to-end (any aligner) … Revisit if the minimap2/rammap follow-up gives a non-local aligner a nonzero match bonus"* — is rewritten to say it is the **monotone / zero-perfect-score** model and to point at `minimap_like`. The footgun (a future minimap2 test reaching for `end_to_end`) is closed structurally by `assert_ne!(end_to_end(i,s), minimap_like(i,s))` in the construction matrix (V5), not by the doc.

> **Alternative considered:** rename `end_to_end` → `monotone` so the name cannot be misread at all. Compiler-enforced and permanent, but **47** mechanical edits (rev 2 count: `combined.rs` 16 + `mapq.rs` 22 + `merge.rs` 9; rev 1 said "~29") in a PR whose semantic diff is 4 lines. Both reviewers endorsed *not* renaming. Recorded as an Open question (§10); the assertion is the cheaper guard.
>
> ⚠️ **Rev 2 (A O5): be honest about what `assert_ne!` does and does not close.** It stops the two *constructors* from colliding; it cannot stop a future minimap2-shaped test from reaching for `end_to_end`. `merge.rs` already has 9 `end_to_end` uses in **deliberately aligner-agnostic** selection tests — including `rammap_supplementary_does_not_displace_primary` (`merge.rs:910`) — and none asserts a MAPQ value, so nothing breaks. State that they are intentional and stay, so the next reader does not "fix" them.

---

## 5. Scope verification (all four re-verified against `dev` `172da96`, not carried over)

| Claim | Verdict |
|---|---|
| **5-Base never calls `calc_mapq`** | ✅ **Confirmed, and it is stronger than "likely not".** `five_base_emit_record` passes the aligner's own MAPQ through verbatim (`mod.rs:1319` `mapq: rec.mapq`; PE reconciliation `:1662` `rec1.mapq.min(rec2.mapq)`). No `calc_mapq`/`score_model` reference exists in `five_base_deconv.rs` or `five_base_duplex.rs`. `--five_base_min_mapq` filters that same passed-through column (`:2060`), i.e. minimap2's 0–60 scale, which is what a DRAGEN-style `20` was designed for. **5-Base is unaffected under either ladder.** |
| **`-x sr` reachability through `calc_mapq`** | ✅ Narrower than the issue implies. 5-Base is **paired-end only** (`mod.rs:591` — SE 5-Base is rejected at `resolve`) and runs its own `run_pe_five_base` path. So `-x sr` reaches `calc_mapq` **only** via `--mm2_short_reads` (minimap2, or rammap with `--rammap_subprocess`). The in-process rammap default cannot reach `sr` at all (hard-coded `Preset::MapOnt`). |
| **`--local` rejected; PE rejected** | ✅ `config.rs:674` and `reject_unsupported_paired_aligner`. Only the SE end-to-end path is in play; no PE behaviour is written into this plan. |
| **rammap in-process `AS` semantics match the subprocess** | ✅ `inprocess.rs:252,258` emit `AS:i:{m.score}` from `rammap::Mapping.score`, which is `dp_score` accumulated exactly as minimap2 does; `second_best` is explicitly `None`. Same model, same denominator. |
| **Which tests move** | ✅ Contained, and **both reviewers re-verified it independently**. `score_model_construction_matrix` (by design). Nothing else asserts a minimap2/rammap MAPQ: `aligner_methylseq_conformance.rs` is CLI-shape only (one minimap2 comment, no checksums); `aligner_five_base_groundtruth.rs` has **no** MAPQ assertion; `genome_prep_byte_identity_real_data.rs`'s mention is index-related. The `perl-oracle` CI job (`EXPECTED=13`) has **no aligner cell at all**, so CI provides zero protection for MAPQ — which is why V6/V7 are load-bearing rather than nice-to-have. ⚠️ **Rev 2: rev 1 said "the 4 MAPQ assertions"; there are ≥7** (see §2's `aligner_cli.rs` row). The conclusion holds — all are Bowtie 2/HISAT2 — but two of the uncounted ones are #1080's BAM-level ladder gates and belong in V11. |

---

## 6. Sequencing — rammap first? **No. One change, both aligners.**

The issue and the handoff both float doing rammap first because it is concordance-gated rather than byte-frozen. Evaluated and **rejected**:

1. **rammap's gate is against minimap2, not against Perl.** `rust/README.md`: *"concordance-gated (NOT byte-identical to minimap2)"*, and the whole design premise is *"minimap-like at every site … same `--mm2_*` knobs (apples-to-apples vs `--minimap2`)"*. Moving rammap alone makes Bismark's MAPQ differ between the two backends **by construction, for every read** — trading a Perl-parity divergence (deliberate, documented, twice-precedented) for a divergence in the gate rammap actually has. That is not a risk reduction, it is a risk swap into the worse column.
2. **It buys nothing mechanically.** The change is one `match` arm covering `Minimap2 | Rammap`. Splitting means writing `Aligner::Rammap => …` first and widening later — two reviews, two CHANGELOG entries, and an intermediate release state where two "minimap-like" backends disagree.
3. **It would not answer the parity question**, only postpone it, while adding a second re-baseline for rammap users.

Moot now that D-PARITY says ship, but recorded because it is the argument to reach for if a future backend raises the same question: the fallback to "not yet" is **defer both**, never "ship rammap alone".

---

## 7. Implementation outline

1. **`config.rs` — the semantics.** Add `MINIMAP2_MATCH_BONUS` (docstring per §4). Extend `from_emitted`'s `match_bonus` to the three-arm `match` in §3.1, with the "not gated on `local`" comment. Add `minimap_like()`. Rewrite `end_to_end()`'s doc (it currently *instructs* this revisit) and **`local_ladder()`'s L168-173 note**, which currently reads *"if #1081 gives minimap2/rammap a nonzero match bonus they would also pick up the local ladder. That is a default that forces the question rather than an implication"* — replace the open question with the recorded answer and the §3.4 reasoning, keeping the `unique.h:236` derivation intact.
2. **Rewrite every comment that says the local ladder — or a nonzero perfect score — is Bowtie 2-only.** #1080 deliberately *tightened* several of these to Bowtie 2, so they read as authoritative and would survive a diff-only review. ⚠️ **Rev 2: both reviewers independently found three sites in `config.rs` that rev 1 missed**, one of them three lines above the arm §3.1 rewrites.
   - `mapq.rs:10-19` — lists three deliberate `--local` deviations and asserts *"End-to-end, for every aligner, remains byte-identical to Perl"* → add the fourth deviation, scope byte-identity to **Bowtie 2/HISAT2** end-to-end. (Rev 1 listed `:2-3`, `:10-19` and `:18` as three sites; they are one contiguous header block — **two** edit locations in `mapq.rs`, not four.)
   - `mapq.rs:140-141` — `calc_mapq_local`'s doc: *"Reached by **Bowtie 2 `--local` only**: … Bowtie 2-local is the sole mode with a nonzero one (#1080)"* → now two mode families.
   - **`config.rs:90-91`** — *"only Bowtie 2-local has a nonzero perfect score, and only Bowtie 2-local is emitted the `G` form"* → first clause false; **keep the second, it stays true**.
   - **`config.rs:98`** — *"Perfect-alignment score per base; nonzero only for Bowtie 2 `--local`"* → false. A one-line field doc, i.e. the most authoritative-looking form of this defect.
   - **`config.rs:117-119`** — *"Only Bowtie 2 --local scores matches positively; every other mode's best possible score is 0 — including HISAT2 …"* → false, and it sits **immediately above the edited arm**, so the diff will render it as context rather than as a claim. §3.1's snippet must not preserve it verbatim.
   - **Drive-by, pre-existing:** `options.rs:81` says HISAT2-local's local-ness is *"the dropped `--no-softclip` … + the local MAPQ ladder"* — already false since #1080, missed by it. Fix here.
   - **Run this and require zero surviving hits** (six today):
     ```
     command grep -rn "Bowtie 2 \`--local\` only\|nonzero only for Bowtie 2\|only Bowtie 2-local\|Only Bowtie 2 --local\|sole mode with a nonzero" rust/bismark/src rust/bismark/tests
     ```
3. **`mapq.rs` — `score_model_construction_matrix` (L661-699).** The four-aligner loop asserting `!local_ladder()` **fails by design** — that is #1088's B-L4 tripwire firing as intended, and it must be *rewritten with the decision*, never relaxed:
   - Bowtie 2 / HISAT2 end-to-end → `!local_ladder()`, `diff == 20.0`, `== end_to_end(0.0,−0.2)` (unchanged).
   - minimap2 / rammap → `local_ladder()`, `diff == 220.0` at `len 100`, `== minimap_like(0.0,−0.2)`, and **`!= end_to_end(0.0,−0.2)`**.
   - `local` is irrelevant for minimap2/rammap: assert both `local` values give the same model, documenting that `--local` is rejected upstream anyway.
   - Add a comment that this test is now the **replacement tripwire**: a fifth aligner added later inherits `_ => 0.0` and must be classified here deliberately.
4. **`mapq.rs` — new unit tests.** §3.3's two tables, hand-derived, with the derivation in the comment (per #1080: *"the number the test produced was right and the reasoning would have been wrong"*). Include the floor cell (`AS = 40` → 22) and the near-tie cell (`200/190` → 11) — the two ends the class-not-instance lesson says get skipped.
5. **`mapq.rs` — an independent reference for the new mode.** `bowtie2_local_reference` (added by #1079) is already an f64 analogue of `unique.h:206-222`; generalize it to take `(form, match_bonus)` so minimap2's model is checked against the same transcription rather than a second, minimap2-specific copy. Sweep `len × AS × {None, Some(second)}` × **`SCORE_MIN_CELLS`** (`mapq.rs:233`; rev 1 had no `--score_min` axis anywhere in V1–V12, and that axis is what would have surfaced the direction error). **Do not** re-baseline from the implementation. Two traps, both found by Reviewer B (I6):
   - ⚠️ **The reference must take a literal `2.0`, never `MINIMAP2_MATCH_BONUS`.** If it is handed the constant, V10(iii)'s injection (`= 1.0`) propagates into *both* sides and cancels — V4 goes vacuous exactly where it is supposed to be independent. This mirrors how it already takes literal `20.0, 8.0` for Bowtie 2-local (`mapq.rs:466-468`).
   - The reference calls `calc_mapq_local` unconditionally (`mapq.rs:449`). Once parameterized by `match_bonus`, passing `0.0` would compare an end-to-end model against the local ladder — either assert `match_bonus > 0.0` inside it, or derive the ladder selection inside it.
6. **`mapq.rs` — record the coverage narrowing.** `end_to_end_matches_the_pre_fix_formula` still passes and now covers **less**: minimap2/rammap used to be inside its class and no longer are. Add that to its doc comment and name the test that took over. (Silent narrowing of a frozen-path sweep is how the next defect hides.)
7. **`tests/aligner_cli.rs` — BAM-level wiring, the #1079 V10 standard.** Assert MAPQ read out of a real output BAM for `--minimap2` **and** `--rammap` (both fakes emit `AS:i:12` on a 6 bp read; `scMin = −1.2`, `perfect = 12`, `diff = 13.2`):
   - **(a) existing fixture, no second best** — `AS:i:12` ⇒ old **42**, new **44**.
   - **(b) new fixture, a lower rung — `AS:i:7`, NOT rev 1's `AS:i:5`** ⇒ `bestOver 8.2`, ratio 0.621 ⇒ new **41** (old 42). ⚠️ **Rev 2 (A I2): `AS:i:5` is form-blind** — it returns 28 under both a Linear and a Log `scMin`, so rev 1's BAM cells could not see a wrong `ScoreMinForm`. That is #1079's code-review **H1 verbatim**, the lesson rev 1's own V10 rationale cites and then failed to apply. `AS:i:7` separates **five** fault modes at once (all verified):

     | state | MAPQ at `AS:i:7`, 6 bp |
     |---|---|
     | correct (bonus 2.0, Linear, local ladder) | **41** |
     | wrong `scMin` form (Log) | **36** |
     | lost match bonus (0.0) | **42** |
     | `MINIMAP2_MATCH_BONUS = 1.0` | **44** |
     | ladder forced end-to-end | **24** |

     The failure message must name which fault each wrong value implies, as `aligner_cli.rs:88-92` does for #1079.
   - **(c) new fixture, the floor** — `AS:i:2` ⇒ `bestOver 3.2`, ratio 0.242 ⇒ new **22** (old 42). The floor is the consequential end (#1080 HIGH-1); under the e2e ladder this cell is **0**. (Also form-blind, and that is fine — (b) carries the form; see V3's note that this cell is a *ladder* guard, insensitive to the bonus magnitude.)
   - **(d) with a cross-instance second best — ⚠️ REV 2: rev 1's recipe was BACKWARDS and could not reach the branch at all.** Rev 1 said CT `AS:i:12`, GA `AS:i:11`. Slot order is `[CT, GA]` (`mod.rs:769`), and a later slot with a **lower** AS never sets `overwrite` (`merge.rs:254-280`) so it is never inserted (`:283-310`) ⇒ `entries.len() == 1` ⇒ `second_for_mapq = None` ⇒ **44**, a silent duplicate of cell (a). **Both reviewers found this independently and it was the only Critical in either review.** The buildable fixture, with the existing 8 bp `chr1 = ACGTACGT`:

     | arm | rname | POS | AS |
     |---|---|---|---|
     | CT (slot 0) | `chr1_CT_converted` | 1 | **`AS:i:11`** |
     | GA (slot 1) | `chr1_GA_converted` | **3** | **`AS:i:12`** |

     Three constraints, all load-bearing — state them in the fixture comment:
     1. the later slot must score **strictly** higher (equal ⇒ `Decision::Ambiguous`, `merge.rs:338-342`);
     2. the two records need **different POS**, because the HashMap key is `"{chromosome}:{pos}"` and CT/GA de-convert to the same chromosome name (`merge.rs:399`, `:227-237`);
     3. the slot-1 winner needs **POS ≥ 3**: index 1 prepends two genomic bases and bails with `extracted: false` when `pos < 2` (`methylation.rs:155-158`). At POS 3 the window is `chr[0..2] + chr[2..8]` = 8 bytes = `read_len + 2`, so the guard passes on the existing chromosome — no new genome fixture needed. (Rev 1 flagged the `read_len + 2` guard as the obstacle; it is real but **secondary** to the score-ordering rule, and rev 1 named the wrong one.)

     Expected values are unchanged: old **27** → new **11** (oracle-confirmed by both reviewers). Only the fixture moves.

     ⚠️ **The escape hatch is now conditional on the corrected recipe.** Rev 1's *"if that cannot be built reliably, say so and cover the branch at unit level only"* combined with a wrong recipe is the single best "ships green" fault either reviewer constructed (B §4): the implementer builds cell (d), gets 44, concludes the branch is unreachable through a fake, and **records that false conclusion in a comment** — leaving the only end-to-end coverage of §1's second branch absent *and* inoculating the next reviewer. If the corrected recipe still fails, the written-down reason must name the specific mechanism that blocked it.
8. **Docs.** ⚠️ **Rev 2: rev 1 misread the docs sentence it was telling the implementer to preserve, and missed two more false claims in the README — both reviewers, independently.**
   - `docs/.../options/alignment.md:111` — rev 1 said *"This applies to Bowtie 2 only"* is about `--local` **mode** and is "still true". **It is not.** The antecedent of "This" is the preceding sentence — *normalising against a positive best-possible score* — and the justification that follows is about the **match bonus** ("HISAT2 … always scores matches as 0"), which confirms the reading. After this fix minimap2/rammap are normalised against `2·len` too, so the sentence is **false and must be rewritten**, not preserved with a minimap2 note appended (which is precisely what rev 1's framing invited: ship a page that contradicts itself). Rev 1's *goal* — don't let "no `--local` mode" and "no local ladder" get conflated — was right; only its reading was wrong. Also consider placing the new prose in the `--minimap2` (`:308-320`) and `--rammap` (`:335`) sections, cross-referenced, since the paragraph currently lives under a flag both aligners reject.
   - `rust/README.md:161` — **three** statements, not one: (i) the minimap2 byte-identity clause must be **restated, not annotated** (#1079's M5/E4 was exactly this omission) and framed the way rammap and 5-Base are; (ii) *"⚠️ **Byte-identity is the END-TO-END contract:** … End-to-end (every aligner) is unaffected and stays byte-identical"* — under D-PARITY this **general** sentence is false, and it is the more dangerous one because it is the categorical claim a reader trusts (its twin at `mapq.rs:18-19` is covered by step 2; this README copy is not); (iii) *"Phase-5 combined 10M gate: all 13 cells byte-identical (… minimap2 SE × {dir, non-dir, pbat} …)"* — those minimap2 cells stop reproducing, so qualify it "(pre-#1081)" rather than deleting a historical gate record.
   - Plus a dated Milestones line (the README row rule is per-module-merge-PR).
9. **CHANGELOG + issues.** **Replace** the placeholder bullet under `## Unreleased` → `### bismark (aligner)` (do not add a second bullet — #1080's M2 lesson). It must state, concretely: MAPQ changes for **every** minimap2/rammap alignment; unique reads move `42` → **44 / 44 / 41 / 28 / 22** across 100→20 % of perfect; reads whose runner-up comes from a **later**-slot instance that strictly out-scores the earlier one move in **both** directions (`25` → **11** for a near-tie, `33` → **39** for a clear winner) — phrased as that condition, **not** as "reads that map to both strands" (§1 rev-2 correction); the floor **for a uniquely-aligned read** is 22, so a `-q 20`-style filter stays close to a no-op for that majority — but the second-best branch reaches **2**, so `-q`-filtered counts *do* change for reads with a competing cross-instance alignment, and that is the correction working rather than a regression (§3.4, corrected in rev 3 — do **not** write "the floor is 22" unqualified); `--score_min` does not reach minimap2 but **does** move its MAPQ, and a steep slope compresses it back toward the ceiling (§1 box); this is Bismark's own scale and **not** agreement with minimap2's own MAPQ (a 0–60 chain-score value Bismark discards); and that **minimap2 SE is no longer byte-identical to Perl v0.25.1** (D-PARITY).

   ⚠️ **Rev 2 (A I4 / B I3): amend the neighbouring #1079 bullet** (`CHANGELOG.md:12`), which says *"The **default end-to-end path is unaffected and stays byte-identical** — it keeps `abs(score_min)` untouched, because the new denominator is applied only where the perfect score is nonzero."* After #1081 minimap2 **has** a nonzero perfect score, so the first clause is false and the shipped release notes would contradict themselves two bullets apart in the same `## Unreleased` section. #1080's M2 lesson was "don't add a competing bullet"; the harder problem here is **an existing neighbouring bullet that becomes wrong** — a class neither precedent hit.

   File the in-process-rammap-preset issue (§1 non-goals), noting that this fix **worsens** it, and cite it.
10. **No version bump** — release cut, exactly as #1079/#1080. All three literals stay at `3.1.0` (`rust/VERSION`, `rust/bismark/VERSION`, `rust/bismark/Cargo.toml`); the two guard tests stay consistent. Cut magnitude is already **minor** (3.2.0) from #1079/#1080; this rides it.
11. **Gates:** `cargo fmt -p bismark -- --check`, `cargo clippy --all-targets` (0 warnings), full suite, then the fault injections in V10.

---

## 8. Assumptions

| # | Assumption | Status |
|---|---|---|
| **A1** | `AS ≤ 2 × read_length` for every preset Bismark can select ⇒ `perfect = 2·len` | ✅ **Spike F1-F3 + Reviewer B's independent attempt to break it, which failed** for minimap2: the block-partition step, Z-drop's single-add, splits as separate records, `mm_update_dp_max` writing `dp_max` not `dp_score`, the `sr` ungapped path, inversions, indels (subtract only), `sc_ambi ≤ 1 < a`. The one additive DP bonus that could break it (`junc_bonus`) is splice-only ⇒ unreachable. ⚠️ Rev 1's claim that V9 *guards* this at runtime was **false** (B I1) — see V9/V15 |
| **A2** | The match score is unreachable by the user | ✅ `--mm2_*` are preset selectors only (`cli.rs`); `minimap2_options` emits a closed string; no `trailing_var_arg`/`allow_hyphen_values` |
| **A3** | rammap's `AS` semantics equal minimap2's, in-process and subprocess | ⚠️ **Weaker than rev 1 stated, and its evidence was misattributed** (B I5, §2 box). `AS:i:` is `r.align_score` (`pipeline.rs:2483`), not the field named `dp_score` the spike traced; the default-path `match_score` constant is `align/map.rs:215`, not `api.rs:690` (the `sr` branch). One additive term, `pipeline.rs:1070-1074`, is on the AS path and is **not** splice-gated — it restores a definitely-subtracted gap penalty, so the bound plausibly survives, but this is the **unquantified residual for rammap only**. rammap's splice bonus lands on `dp_score` where minimap2 puts it on `dp_max` — a genuine divergence, unreachable from Bismark (gated on `AlignFlags::SPLICE` + a jump DB, `pipeline.rs:2113-2121`). Partly mitigated after all: the in-process crosscheck asserts field-identity on `alignment_score`, not just `mapq` (V12) |
| **A4** | The **local** ladder is the right one | 🟠 **Decided (D-LADDER), on an argument from score regime — not correspondence.** Bowtie 2 and HISAT2 both select by `monotone` (`unique.h:236`), which minimap2 is not — but minimap2 uses **no ladder at all**, so this is analogy. #1088 made the coupling deliberate precisely so this issue would have to answer it; §3.4 records the rejected alternative and the cost (floor 22 ⇒ no filterability). The assumption does not become true by being decided — the CHANGELOG must not claim otherwise |
| **A5** | Nobody depends on the current 42/33 values | ⚠️ Unverifiable, and **weaker than #1080's A5** because this is a default path. **Now the top residual risk** (D-PARITY is settled, so nothing else absorbs it). Mitigated by the CHANGELOG's concreteness. ⚠️ **Rev 2 (both reviewers): the "degenerate ⇒ carries no information" mitigation applies only to the no-second-best branch.** The 33/25/27 branch *does* vary with `bestDiff`, so a consumer could have depended on it. Split the claim; do not let the strong half cover the weak half |
| **A6** | `scMin` is a **fiction** for minimap2 and stays one | ⚠️ **Load-bearing, and rev 1 stated its consequence backwards** (A I1; §1 box). minimap2 never receives `--score-min` (`options.rs:233`), yet the denominator uses `(0.0, −0.2)`. Keeping it is faithful to Bowtie 2's `perfect − scMin`. Real consequence: a **steep** `--score_min` compresses MAPQ toward the **ceiling**, and `L,0,−20` restores #1081 entirely. Documented + pinned (V14), not fixed — a `scMin`-free normalization needs `bestOver` to change too, which is a different normalization, not an adaptation |
| **A12** | The SE merge's runner-up selection is **order-dependent** (later slot must strictly out-score every earlier one) | ✅ **New in rev 2, made explicit by both reviewers.** `merge.rs:254-310`, `:338-342`, `:399`. It is the difference between a buildable and an unbuildable cell (d), and between a correct and an overstated §1 |
| **A13** | MAPQ never exceeds `u8` | ✅ Ladder max is 44; `MappingQuality::new` at `output.rs:478`. Even if A1 were violated the ratio only saturates the top rung — no overflow path. Stated so a reader need not wonder |
| **A7** | `--five_base_min_mapq` is unaffected | ✅ §5 — 5-Base passes the aligner's MAPQ through (`mod.rs:1319`) |
| **A8** | Both ladders are correct and untouched | ✅ Diffed leaf-by-leaf against `unique.h:223-380` during #1079; this change touches neither |
| **A9** | Read length == the length minimap2 scored | ✅ Same as #1079's A8: no `--trim5/--trim3/--clip_r*`; soft-clipped records retain full SEQ. Soft clipping lowers `AS` against a full-length `perfect`, which is Bowtie 2-local's behaviour too |
| **A10** | f64 throughout, no attempt at integer parity | ✅ Deliberate non-goal, inherited from #1079 A10 |
| **A11** | minimap2 has no ladder and no comparable MAPQ ⇒ **no oracle exists** | ✅ Verified (0–60 chain-score scale; spike observed 4/17/18/56/59/60). This is the honest basis of the whole change, the analogue of #1080's A1 |

---

## 9. Validation

| # | Verify | How | Expected |
|---|---|---|---|
| **V1** | The no-second-best ladder | §3.3 table 1, hand-derived, at `len ∈ {50, 100, 1000}` to pin length-invariance | 44 / 44 / 41 / 28 / 22 at every length |
| **V2** | **The with-second-best branch** | §3.3 table 2 — must include the near-tie (`200/190` → **11**, was 25) and a clear winner (`200/20` → **39**, was 33) | As tabulated; both directions present |
| **V3** | **The floor of the no-second-best branch** | `AS = 0.2 × perfect`, no second best | **22** (local) — and a comment stating it is **0** under the e2e ladder. ⚠️ **Rev 3: name the branch.** Rev 2 called this "The floor" unqualified, which is what licensed the false user-facing claim (§3.4) — the second-best branch reaches **2**, and V2 is what covers it. Also (A O4) this cell is a **ladder** guard, not a bonus guard: with `MINIMAP2_MATCH_BONUS = 4.0` it still returns 22 |
| **V4** | Independent reference | The generalized `bowtie2_local_reference((form, match_bonus))`, swept `len × AS × second-best` **× `SCORE_MIN_CELLS`** | Agreement in every cell; no re-baselining. ⚠️ **Must take a literal `2.0`** — see §7 step 5; handed the constant, V10(iii) cancels and V4 goes vacuous |
| **V5** | **Construction matrix** (the rewritten tripwire) | Per §7 step 3, incl. `minimap_like != end_to_end` and `local` irrelevance for minimap-like | As specified. Keep the `== from_emitted(…, Linear, …)` comparison **exactly** as written — both reviewers identified it as the whole unit-level gate on the #1079 wrong-form class |
| **V6** | **BAM-level wiring, minimap2** | Cells (a)(b)(c) of §7 step 7 via `make_fake_minimap2_mapped` + two new fixtures | **44 / 41 / 22** (were 42 / 42 / 42). Cell (b) is `AS:i:7` → **41**, which is *form-sensitive*; rev 1's `AS:i:5` → 28 was not |
| **V7** | **BAM-level wiring, rammap** | Cell (a) at minimum via `make_fake_rammap_mapped` | 44 (was 42) — proves the arm covers `Rammap`, not just `Minimap2`. Without this, an `Aligner::Minimap2`-only arm ships green |
| **V8** | Second-best branch end-to-end | Cell (d) — the **corrected** two-instance fake (CT slot 0 `AS:i:11` @POS 1; GA slot 1 `AS:i:12` @POS 3) | old 27 → new **11**. The escape hatch applies only if the *corrected* recipe fails, and must name the blocking mechanism |
| **V9** | `AS ≤ perfect` is asserted at the seam | Unit, **in `config.rs`'s `mod tests`** (`:1536`) or via the `pub(crate)` seam `let (bo, d) = model.normalize(len, None, as_best); assert!(bo <= d)` — ⚠️ **`perfect()` is private (`config.rs:196`), so rev 1's placement in `mapq.rs` cannot compile** (both reviewers) | Holds for the spike's observed `(len, AS)` pairs. ⚠️ **Rev 2: rev 1's claim that this catches "a future minimap2" was false** — a unit test over hardcoded pairs never runs an aligner. V15 is what makes A1 a live gate |
| **V10** | **Teeth — six fault injections** | Each injected alone, verified failing, reverted: **(i)** `match_bonus = 0.0` for minimap-like → V1/V6 fail; **(ii)** ladder forced end-to-end → V1/V3/V6 fail (42/8/0; cell (b) → 24); **(iii)** `MINIMAP2_MATCH_BONUS = 1.0` → V1/V6 fail (cell (b) → 44) **and V4 only if it holds a literal 2.0**; **(iv)** ⚠️ *rev 1's version was wrong* — `end_to_end()` at the production construction site (`config.rs:796`) would break Bowtie 2-local and fail V11 rather than isolate V5, so instead build `minimap_like` with `ScoreMinForm::Log` → V5 **and V6 cell (b)** fail (41 → 36); **(v)** drop `\| Aligner::Rammap` from the arm → **V7** fails; **(vi)** a Log `scMin` form for minimap-like → V6 cell (b) fails | All six fail, with messages naming the fault. One injection proves non-vacuity, not teeth (#1079 12b: a `match_bonus` injection missed a wrong-*form* bug entirely — and rev 1 repeated it) |
| **V11** | Bowtie 2 / HISAT2 frozen | `end_to_end_matches_the_pre_fix_formula`, `hisat2_local_denominator_is_abs_of_the_linear_scmin`, `local_bowtie2_*`, `bowtie2_local_mapq_*_end_to_end`, **`score_min_params_aligner_and_mode_defaults` (`options.rs:577`)**, **`local_hisat2_uses_the_linear_form_and_end_to_end_ladder` (`mapq.rs:554`)**, **`hisat2_local_softclip_roundtrip_and_options` (`aligner_cli.rs:2214`, MAPQ at `:2269`)**, **`hisat2_local_pe_softclip_roundtrip` (`:2305`, MAPQ at `:2367`)**, Bowtie 2 PE (`:885`), `select_unique_best_mapq_equals_calc_mapq` (`combined.rs:1133`) — **all pass untouched** | Green with no expectation edits. ⚠️ Rev 2: the two HISAT2-local BAM cells are the **only** end-to-end proof that a mis-scoped `match` arm has not leaked the local ladder to HISAT2 — their messages literally say *"44 means the local ladder is still selected"*. Rev 1 omitted them |
| **V12** | rammap in-process crosscheck unaffected | `aligner_rammap_inprocess_crosscheck.rs` (feature-gated, env-skipped) still compiles; its field-identity assertions are unchanged | Compiles. It asserts the aligner's own `mapq` **and `alignment_score`** — the latter is a real, if indirect, partial mitigation for A3 that rev 1 did not credit |
| **V13** | **The documentation deliverables are gated** (rev 2, B I9) | Run and record at implementation time: §7 step 2's grep returns **zero** hits; `CHANGELOG.md`'s `## Unreleased` → `### bismark (aligner)` contains exactly **one** #1081 bullet and the placeholder is gone; the amended #1079 bullet no longer claims end-to-end byte-identity for every aligner; `command grep -c "byte-identical to Perl v0.25.1 + minimap2" rust/README.md` reflects the restatement | As stated. Both precedent reviews caught **doc** defects (#1079 M5/E4, #1080 M2) and rev 1's §9 gated only code — the one class this lineage has now missed twice |
| **V14** | **Non-default `--score_min` for the minimap-like model** (rev 2, A I1 / B I8) | Unit cells at `L,0,−5` and `L,0,−20` (re-saturation) **and** `L,10,−0.2` / `L,100,−0.2` (positive intercept ⇒ the clamp fires, `bestOver` negative) | Pins the documented limitation rather than fixing it: `L,0,−20` ⇒ **44 across the whole score range**; `L,10,−0.2` ⇒ clamp active for `len ≤ 4`, MAPQ 22. Rev 1 had **no** `--score_min` axis for minimap-like anywhere |
| **V15** | **A1 gets a live gate against a real aligner** (rev 2, B I1) | minimap2 is **already installed in CI** (`rust_ci.yml:35-42`, `:93-100`); reuse `aligner_five_base_groundtruth.rs:55-67`'s skip-locally / **panic-if-`$CI`** pattern so it cannot pass vacuously. Port SPIKE §5's one-liner: a perfect read against a small reference under each of `map-ont`/`map-pb`/`sr` reports `AS == 2·len`, and no alignment reports `AS > 2·len` | Holds. Two payoffs: A1 stops being an unguarded premise, and because CI runs Ubuntu's packaged minimap2 rather than 2.31-r1302 it extends the evidence to a **second version** — closing the spike's own stated limitation ("one binary, one version… not pinned in CI") |

**Load-bearing:** V5 + V6 + V7 (without them a mis-wired arm is a silent no-op with a green suite — the failure shape that let #1079 ship), V6 cell (b) (the only *form*-sensitive end-to-end cell), V8 (the second branch), V3 (the floor), V10 (the gates' teeth), V13 (the only gate on the class both precedents actually shipped defects in).

**Deliberately not built:** a Perl/minimap2 MAPQ oracle. There is none (A11) — this issue's defining constraint, not a corner cut. The `perl-oracle` job must **not** gain a minimap2 cell: it would codify the old degenerate values. Note V15 is *not* that oracle — it gates the `AS ≤ 2·len` premise, not the MAPQ mapping.

---

## 10. Questions and ambiguities

| Priority | Item |
|---|---|
| **Resolved — D-PARITY (Felix, 2026-08-01)** | **Ship unconditionally.** No compat flag, no opt-in gate. Rationale on the record: the current values are degenerate (42 / 33), so nothing but a byte-comparison can depend on them; the cut is already a minor carrying two deliberate MAPQ divergences (#1079, #1080), so minimap2 users re-baseline **once** rather than twice; and #1079 set the precedent for this defect class. Accepted cost: the minimap2-SE byte-identity claim is retired (not narrowed), in a default path, with no documented formula to appeal to (A11). Options (B) opt-in flag and (C) defer both were declined. |
| **Resolved — D-LADDER (Felix, 2026-08-01)** | **Local ladder.** Needs no new mechanism — #1088's `match_bonus > 0.0` derivation already yields it, which is why #1088 documented #1081 as the decision it was forcing. Ceiling 44, floor 22. §3.4 records the rejected end-to-end variant and the accepted cost: **filterability is not restored** (floor 22 > a conventional `-q 20`); this fixes discrimination only. Neither ladder is "what minimap2 would compute" (A11) and the CHANGELOG must not imply it is. |
| **Open** | Rename `end_to_end` → `monotone` (§4)? Recommended answer: no — keep the name, fix the doc, and let `assert_ne!(end_to_end, minimap_like)` be the structural guard. Cheap to revisit; the only remaining open question, and it does not block implementation. |
| **Resolved (spike)** | `end_bonus` does not affect `AS`; `perfect = 2·len` holds at every reachable preset; one `2.0` constant suffices; rammap mirrors minimap2 |
| **Resolved (§5)** | 5-Base out of scope (confirmed, not "likely"); `--local`/PE/combined rejected; SE only; no test outside `score_model_construction_matrix` breaks; no CI oracle protection exists |
| **Resolved (§6)** | Sequencing: one change covering both aligners; rammap-first evaluated and rejected |
| **Resolved (§7 step 10)** | Version bump belongs to the release cut, not this PR |
| **Filed separately** | in-process rammap hard-codes `Preset::MapOnt` and silently ignores `--mm2_short_reads`/`--mm2_pacbio` (spike F5) |

---

## 11. Self-Review

**Efficiency.** One extra `match` arm at config-resolve time (once per run) and the arithmetic #1079 already added (one multiply, one compare per accepted alignment). `ScoreModel` stays `Copy`, same size. No allocation, no measurable cost.

**Logic.** Traced the `calc_mapq` call sites — **four**, not the two rev 1 named: `merge.rs:367` (the only reachable one here), `merge.rs:740` (PE, rejected), and `combined.rs:339` + `:711` (combined-index, rejected for these aligners at `config.rs:929,983`). The scope conclusion survives, but rev 1's "traced every call site" was a claim over an incomplete inventory (A O2). Confirmed the reachable site carries the read length — and specifically the **FastQ** read length (`sequence.len()`, documented at `merge.rs:513`), not the SAM `SEQ`, so clipping cannot reach it; only the first record per read per instance is considered (`merge.rs:312-315`), which is the mechanism A9 actually rests on. Confirmed `bestOver` must not move — only the denominator — which `normalize()` enforces structurally. Confirmed the ladder follows `match_bonus` automatically, which is why D-LADDER was a decision rather than an implementation detail.

**Edge cases (corrected in rev 2 — three of these were wrong or under-derived in rev 1).** `len = 0` (finite, linear form, no `ln(0)` — but `scMin = 0` only at intercept 0); the `max(1, …)` clamp (**live, not dead**: it fires for `len ≤ 4` under the legal `L,10,−0.2`, and `perfect − scMin` is then *negative*, not in `[0,1)`); soft clipping (lowers MAPQ against a full-length `perfect`, matching Bowtie 2-local); `AS > perfect` (structurally excluded; **not** clamped, and rev 1 was wrong that a unit test would catch a future aligner — V15's real-aligner CI gate is what does); non-default `--score_min` (legal, still a fiction, and a **steep slope compresses MAPQ upward** — the opposite of rev 1's claim, with `L,0,−20` restoring the defect); the strand-order artefact (the near-tie improvement lands for ~half of near-tie reads; spread widens 17 → 33 points); `--ambig_bam` (unaffected, and therefore a second MAPQ scale in a second file); and both `second_for_mapq` sources (`ZS:i:` never present for these aligners, so only the cross-instance runner-up — reachable only when a **later** slot strictly out-scores every earlier one).

**Integration.** No internal consumer of `calc_mapq`'s MAPQ; `--five_base_min_mapq` narrowed and cleared (§5); dedup and the extractor do not filter on MAPQ; the rammap in-process crosscheck asserts a different column. External `samtools view -q` users are the intended audience of the change.

**What this plan corrected relative to the handoff and the issue.**
1. *"MAPQ is 42 for every alignment"* is the **no-second-best** branch. The with-second-best branch is not saturated and moves in both directions (25 → 11, 33 → 39). Added as §1's second row and V2 — omitting it would have reproduced #1080's HIGH-1 miss exactly.
2. *"`-x sr` is what Bismark selects for `--mm2_short_reads` and `--illumina_5base`"* — true of the option string, but 5-Base never reaches `calc_mapq` and is PE-only, so `sr` reaches this code **only** via `--mm2_short_reads`. Narrower, and it removes 5-Base from the argument for either ladder.
3. *"rammap may be able to move ahead of minimap2"* — evaluated and rejected (§6): rammap's gate is against **minimap2**, so moving it alone breaks the gate it actually has.
4. Found the **`scMin` fiction** (A6) — the denominator mixes a real `perfect` with a `--score-min` minimap2 never receives — and the **in-process rammap preset** bug (separate issue).

**What rev 2 changed, and what it says about rev 1's method.** Both reviewers re-derived every MAPQ value against an independently extracted Perl oracle and found **no wrong number and no wrong reasoning** — the arithmetic core was right. What was wrong was everything *around* the arithmetic: a fixture that could not reach its branch, a direction-of-effect claim, two edge-case rows derived only at the default `--score_min`, three missed stale comments, two missed false README claims, a misread docs sentence, a misattributed spike citation, and three undercounted inventories. The pattern is consistent and worth naming: **rev 1 verified what it computed and asserted what it read.** Every rev-2 correction came from re-reading code that rev 1 had cited but not traced, or from varying a parameter rev 1 had held at its default.

**Remaining risks (rev 2).**
1. **A5 — a default path re-baselines and nobody can prove what depended on it.** Now the top risk. Weaker mitigation than #1079/#1080 had (they touched opt-in `--local`), partly offset by the old values carrying no information. Mitigation is the CHANGELOG's concreteness (§7 step 9), not a test.
2. **No oracle, by nature** (A11). The gates are internal consistency plus an f64 transcription of Bowtie 2's formula. Stated in the CHANGELOG rather than papered over — and the local ladder being *decided* does not make A4 *verified*.
3. **Filterability is knowingly not restored** (§3.4). Floor 22 sits above a conventional `-q 20`, so a user who reads "MAPQ fixed" and expects `-q` to start working will be disappointed. The CHANGELOG must say so in the same breath as the fix.
4. **rammap: source-only, and one term unclosed** (A3 as restated). `pipeline.rs:1070-1074` adds `gap_open + gap_extend·stripped_gap_len` back onto the AS path, un-splice-gated. It reads as restoring a definitely-subtracted penalty, but nobody has closed it by inspection or measurement. Options: measure once through the existing in-process crosscheck (which already compares `alignment_score`), or carry it explicitly as rammap's residual. **Do not let "✅ source-verified" stand for it.**
5. **A steep `--score_min` re-saturates minimap2 MAPQ** (A6 as restated). Documented and pinned (V14), not fixed. Low practical exposure — methylseq's `L,0,−N` forms still discriminate — but it means the fix is not unconditional in the way a reader would assume.
6. **The construction matrix is now the only tripwire** for a future fifth aligner silently inheriting `_ => 0.0`. #1088's version fired correctly here; the replacement must be written to fire again (§7 step 3).
7. **Stale prose is this change's dominant defect class, and grep is the only reliable finder.** Rev 1 found four sites by grepping; both reviewers then found **three more in `config.rs`** (one three lines above the edited arm), two more in `rust/README.md`, a misread sentence in `docs/`, and a pre-existing one at `options.rs:81` that #1080 itself missed. §7 step 2's grep must be re-run at implementation time and V13 gates it — this list is a starting point, not an inventory.

---

## 12. Implementation Notes (2026-08-01)

**Branch:** `plan/mapq-minimap2-denominator`, based on `origin/dev` `172da96`. **Status:** implemented, not committed. `cargo fmt -p bismark -- --check` clean · `cargo clippy -p bismark --all-targets` **0 warnings** · **2120 tests pass / 0 fail** (was 2109; +11).

**Every hand-derived value in §3.3 and §7 step 7 matched on the first run** — all five new unit tests and all three new BAM tests passed without a single expectation edit. That is the payoff of both reviewers having re-derived them against a Perl oracle before any code was written; there was nothing left to re-baseline.

### Site-by-site

| Plan step | Outcome |
|---|---|
| 1 (`config.rs`) | `MINIMAP2_MATCH_BONUS` added as a **separate** constant; `from_emitted` now a 3-arm `match`; `minimap_like()` added. `end_to_end()`'s doc retargeted ("the **monotone** model"), `local_ladder()`'s open question replaced by the recorded answer + the §3.4 reasoning. Also updated `perfect()`'s doc (full read length even when soft-clipped) and `normalize()`'s (the `scMin`-is-Bismark's-own note). |
| 2 (stale comments) | All six sites rewritten; the V13 grep returns **0**. The `config.rs:117-119` one — three lines above the edited arm — was the sharpest: the diff renders it as context. |
| 3 (construction matrix) | Rewritten. The four-aligner loop split into `[Bowtie2, Hisat2]` (end-to-end ladder) and `[Minimap2, Rammap]` (local ladder, `diff == 220.0`), the latter looped over **both** `local` values to pin that `--local` is irrelevant, plus `assert_ne!(end_to_end, minimap_like)`. |
| 4 (unit tests) | 5 added: the no-second-best ladder × 3 lengths **with a pre-fix control asserting 42**, the second-best branch × 5 cells with old+new, the reference sweep, the floor, and the `--score_min` sensitivity. |
| 5 (reference) | `bowtie2_local_reference` generalized to `positive_bonus_reference(.., log_form)` with two thin wrappers. **The `2.0` is a literal**, per B's I6 — verified by fault (iii) failing. |
| 6 (coverage narrowing) | Noted on `end_to_end_matches_the_pre_fix_formula`: minimap2/rammap used to be in its class and no longer are. |
| 7 (BAM cells) | `make_fake_minimap2_with_score(dir, as)` + `make_fake_minimap2_two_instance`; a `minimap2_bam_mapq` helper; 3 tests (mm2 a/b/c, mm2 cell d, rammap a). |
| 8-9 (docs) | `alignment.md` — the `--local` paragraph's "Bowtie 2 only" rewritten to "**among the modes `--local` selects**", plus a new MAPQ block under `--minimap2` (its natural home, since minimap2 rejects `--local`). `rust/README.md` — all three statements restated + a Milestones entry. `CHANGELOG.md` — placeholder bullet replaced; **the neighbouring #1079 bullet amended** ("default end-to-end path" → "Bowtie 2 and HISAT2 end-to-end paths"). |
| 10 | No version bump; all three literals still `3.1.0`. |
| Issue | **[#1092](https://github.com/FelixKrueger/Bismark/issues/1092)** filed for the in-process rammap preset, stating that this change worsens it. |

### The corrected cell (d) works exactly as both reviewers predicted

`make_fake_minimap2_two_instance` puts `AS:i:11` on CT (slot 0) and `AS:i:12` at **POS 3** on GA (slot 1) and returns **11** on the first run. Rev 1's ordering would have returned 44. All three constraints were load-bearing and are documented in the fixture: strictly-higher later slot, distinct POS, and POS ≥ 3 for the slot-1 prepend guard. **No escape hatch was needed** — the branch is reachable through a fake, and rev 1's warning that it "cannot be built reliably" would have been false.

### All six fault injections verified (V10)

Each injected alone, run, reverted; `grep -rl 'TEMP FAULT'` returns 0 files, and the suite is green again at 2120.

| Fault | Detected by | Observed |
|---|---|---|
| (i) `match_bonus = 0.0` for minimap-like | 6 unit tests + mm2 BAM | FAILED |
| (ii) ladder forced end-to-end | 6 unit tests | FAILED |
| (iii) `MINIMAP2_MATCH_BONUS = 1.0` | 6 unit tests | FAILED |
| (iv) `minimap_like` built with `Log` | 3 unit tests incl. the matrix | FAILED |
| **(v) drop `\| Aligner::Rammap`** | rammap BAM — `left: 42, right: 44` — **plus the construction matrix** (rev-3 correction from A: the matrix loops `[Minimap2, Rammap]`, so it catches it too; the rammap BAM cell is the only **end-to-end** detector, which is the claim that matters) | FAILED, **and the minimap2 BAM test stayed GREEN** |
| **(vi) production form → `Log`** | **mm2 BAM cell (b)** — `left: 36, right: 41` | FAILED |

(v) and (vi) are the two that justify the rev-2 corrections, and both landed on the predicted value. (v) is B's O12 addition: without the rammap cell, a `Minimap2`-only arm ships entirely green. (vi) is A's I2 — #1079's H1 fault class — and it is caught **only** because cell (b) is `AS:i:7`; at rev 1's `AS:i:5` the observed value would have been 28 on both sides.

### V15: the AS bound is now gated against a live aligner

New file `tests/aligner_minimap2_as_bound.rs`. Runs real minimap2 with Bismark's verbatim option string across all three reachable presets, asserting `AS <= 2·len` everywhere **and `AS == 2·len` exactly for perfect reads** (the equality is what would catch a smaller leak than `end_bonus`'s +10). Skips locally without minimap2, **panics when `$CI` is set** so it cannot pass vacuously. CI already installs minimap2, so no workflow change was needed — and because CI runs the distro package rather than 2.31-r1302, this extends the evidence to a second version, closing the spike's stated limitation. Passes locally against 2.31-r1302.

### Deviations from the plan

1. **V9 landed in `config.rs::tests`, not through `normalize` in `mapq.rs`.** Both were authorized; the `config.rs` home is better because it can assert `best_over <= diff` over the spike's real `(len, AS)` pairs without re-exporting anything. Its doc states plainly that it pins arithmetic, not the aligner, and points at V15 as the live gate — the claim rev 1 got wrong.
2. **V14's clamp cells assert `bestOver < 0`, not just `diff == 1.0`.** The plan specified the clamp fires; asserting the sign as well records *why* the outcome is benign (every rung comparison fails against a negative `bestOver`, so 22 results with or without the clamp).
3. **The pre-fix control (`42`) is asserted inline in the new unit tests** rather than left implicit. Cheap, and it makes each test self-documenting about what changed.
4. **`--score_min`'s doc comment gained the A6 note** (`options.rs`), which the plan implied but did not list as an edit site.

### Post-review fixes (2026-08-02)

Dual code review (`CODE_REVIEW_A.md`, `CODE_REVIEW_B.md`) + coverage audit (`COVERAGE.md`). Both reviewers' verdicts were "ship after the one HIGH", and the HIGH was **the same finding, reached independently**. Coverage came back **INCOMPLETE — 5 items**, all documentation or one test cell. Felix's decisions: **scope the floor claim and claim the win**; fix **all gaps + MEDIUMs, skip LOWs**.

#### The one HIGH — the floor claim was false, and the root cause was this plan

Both reviewers enumerated the second-best branch and found MAPQ reaches **2**, not 22. I re-measured: ten reachable values below 22, nine below `-q 20`. Every surface that carried it is corrected — `CHANGELOG.md`, `docs/.../alignment.md`, `rust/README.md`, plus **§3.4 and V3 here**, which is where the error originated and where a future reader would otherwise re-derive it. `minimap_like_floor_is_twentytwo_not_zero` → `minimap_like_no_second_best_floor_is_twentytwo_not_zero`, and `local_ladder()`'s doc now says the floor comparison is about *uniquely-aligned* reads.

Three facts made this a scoping correction and not a reversal: sub-20 was already reachable pre-fix (6/12/17/18), so the class is not new; D-LADDER is untouched because both ladders bottom out at 2 on that branch and only differ on the unique-read floor; and the affected reads are gated by the later-slot condition. The corrected notes now *claim* partial filterability instead of denying it.

#### The other HIGH — a CI break my local gate could not see

`rammap_mapq_uses_the_perfect_score_denominator_end_to_end` passed bare `--rammap`, which on a `--features rammap-inprocess` build defaults to the in-process backend and tries to load the fake 1-byte `.mmi`. `rust_ci.yml:101` runs exactly that. **Both reviewers found it independently**; B fixed it in place with `--rammap_subprocess`, A verified the fix. The sibling test already documented the trap and I had not followed the precedent. Re-verified here: feature-ON 4/4 rammap tests pass, feature-OFF unchanged. **Lesson for the next `--rammap`-touching PR: `cargo test -p bismark` is not the whole gate — the feature build is a separate CI job.**

#### Everything else applied

| Finding | Fix |
|---|---|
| **A MEDIUM-1** — "a fifth backend cannot inherit `_ => 0.0` without failing here" was **false** (the matrix looped hardcoded arrays) | Merged into one loop over all four aligners with an **exhaustive `match`** deciding the expectation, so a fifth variant now fails to **compile**. Claim made true rather than softened, in `mapq.rs` and `config.rs` |
| **Coverage Gap 1** — V14's `L,100,−0.2` cell absent | Added, as a two-witness loop. ⚠️ My first attempt asserted the clamp is inert one length past its reach and **failed at `i=10, len=5`**, where `perfect − scMin` is *exactly* 1.0 — the clamp is a no-op there yet `diff == 1.0` still. A had already noted that boundary. Now asserts inertness at a realistic length (100 bp) and records the exact-1.0 boundary in the comment |
| **Coverage Gap 3** — strand-order artefact not stated user-facing | Added to the `--minimap2` docs block and the CHANGELOG |
| **Coverage Gap 4** — `--ambig_bam` two-MAPQ-scales consequence | Added to the `--minimap2` docs block |
| **Coverage Gap 5 / A MEDIUM-2 / B MEDIUM-1** — CHANGELOG's second-best condition and the "42 for every unique read" headline | Both corrected: the condition is now stated as later-slot-strictly-out-scores, and the headline is qualified "with no competing alignment from another strand instance (the large majority)". Also in `rust/README.md` ×2 |
| **B MEDIUM-2** — `rust/README.md`'s trailing ✅ summary block still made both corrected claims; V13's grep could not match its phrasing | Restated the way the row at `:161` was, tagged "pre-#1081" |
| **B MEDIUM-3** — `docs/usage/alignment.md:65` said MAPQ is "calculated for Bowtie 2 and HISAT2" | → "calculated by Bismark across the strand instances, for every aligner" |
| **Record corrections** | §12's fault row (v) now credits the construction matrix as well as the rammap BAM cell; §1's `L,0,−0.6` row `42` → **41**; the `positive_bonus_reference` parameter removal recorded as deviation 5 below |

**LOW items deliberately not done** (Felix's scope call): comment trimming, the V15 `-x` option ordering, the `--score_min` clap-help cross-reference, the `--rammap` docs MAPQ pointer, the CI step-name comments. All are recorded in the two review files.

#### Deviation 5 (added in this pass)

`positive_bonus_reference` **removed** the `match_bonus` parameter rather than adding §7 step 5's `assert!(match_bonus > 0.0)`. Both reviewers noted this is a *structural* close of the trap — a zero-bonus caller cannot be constructed — and therefore better than what the plan asked for. Recorded because rev 2's deviation list omitted it.

### Not done

- **No commit or PR** — awaiting review.
- Version literals untouched (release cut).
- Real-data quantification of the `-q 20` population shift — a follow-up, not a blocker (§3.4).

## 13. Revision History

**rev 3 (2026-08-02)** — dual code review + coverage audit folded; see §12's "Post-review fixes". The substantive change to *this document* is **§3.4**: rev 2's unscoped "the local ladder's floor is 22, so a MAPQ filter remains close to a no-op" was **false** — that floor is the no-second-best branch's, and the second-best branch reaches **2**. Both code reviewers found it independently and both correctly declined to fix the downstream prose, because the error originated here and was mandated verbatim by §7 step 9. Corrected in §3.4, V3 and §7 step 9, with the measured tables and the three facts that make it a scoping correction rather than a reversal of D-LADDER. Also corrected: §1's `L,0,−0.6` row (42 → **41**) and §12's fault row (v).

**rev 2 (2026-08-01)** — dual plan-review folded. Both reviewers: **not ready as-is**, with **the same single Critical**, reached independently.

| Finding | A | B | Resolution |
|---|---|---|---|
| **Cell (d)'s fixture cannot reach the second-best branch** — a later slot with a lower AS is never stored | C1 | C1 | **Both correct** (verified at `merge.rs:254-310`). Scores swapped; three constraints written down; escape hatch made conditional on the corrected recipe. §7 step 7 |
| The second-best branch is **order-dependent**, so rev 1's "aligns in both instances" framing was wrong | I6 | C2 | Both correct. §1 rewritten as a condition; strand-order artefact added to §3.5 (spread widens 17 → 33 points); CHANGELOG wording constrained |
| Three more stale comments in `config.rs`, one above the edited arm | I3 | I2 | Both correct. §7 step 2 + a grep that must return zero |
| `rust/README.md`'s categorical "End-to-end (every aligner) … byte-identical" + the Phase-5 13-cell claim | I4 | I3 | Both correct. §7 step 8 |
| `alignment.md:111` misread — "This applies to Bowtie 2 only" is about the **denominator**, not `--local` mode | I5 | I4 | Both correct; rev 1's framing invited "leave it". §7 step 8 |
| §3.5's clamp / `len = 0` rows derived only at the default `--score_min` | I7 | I8 | Both correct, with different witnesses (`L,10,−0.2` and `L,100,−0.2`). §3.5 + V14 |
| V9 cannot compile where rev 1 put it (`perfect()` is private) | O1 | I1 | **B's severity is right** — rev 1 also *claimed* V9 would catch "a future minimap2", which is false. V9 rewritten, V15 added |
| MAPQ-assertion count wrong (≥7, not 4) | O2 | I7 | Both correct. **B's addition is the valuable half**: two uncounted ones are #1080's BAM-level ladder gates → V11 |

**Unique to A** — and the most valuable finding in either review: **the `--score_min` direction was backwards** (I1), and at `L,0,−20` the fix is silently *undone* (44 across the range). Re-measured and confirmed; §1 now carries the table and the sign-asymmetry explanation, A6 is restated, V14 pins it. Also A-only: **every rev-1 BAM cell was form-blind** (I2) — `AS:i:5` returns 28 under both `scMin` forms, so rev 1 repeated #1079's own H1; replaced with `AS:i:7`, which separates five faults (41/36/42/44/24). Also A-only: `calc_mapq` has **four** call sites (rev 1 named two), `end_to_end` has **47** (rev 1 said ~29), V3 is insensitive to the bonus magnitude, and `assert_ne!` does not close the footgun it was credited with.

**Unique to B** — **the spike traced the wrong rammap field** (I5): `AS:i:` is `r.align_score` (`pipeline.rs:2483`), not the field *named* `dp_score`; rammap's naming is transposed relative to minimap2, and one additive term on the real AS path (`pipeline.rs:1070-1074`) is **not** splice-gated and remains unclosed. Verified; A3 restated, and it is now risk 4. Also B-only: **V4 goes vacuous under injection (iii)** if handed the constant instead of a literal `2.0` (I6); **no gate on the documentation deliverables** → V13 (I9), the one class both precedents actually shipped defects in; **minimap2 is already installed in CI** with a skip-or-panic pattern to copy → V15 (I1) makes A1 a live gate on a *second* minimap2 version, closing the spike's stated limitation; injection (iv) was mis-specified; `--ambig_bam` carries a second MAPQ scale; the rammap-preset issue is **worsened**, not only mitigated; A12/A13 added; the block-partition and Z-drop steps supplied for A1.

**Contradiction, resolved:** A wanted a form-sensitive **BAM** cell; B judged V5's unit assertion "the whole gate" on the wrong-form class. **Resolved in A's favour** — #1079's H1 was precisely that the *BAM-level* wiring cell was form-blind, and its fix was to change the fixture so the BAM cell discriminated. A's cell costs one fixture value. B's point is kept too: V5's `== from_emitted(…, Linear, …)` comparison must stay exactly as written.

Not folded: nothing. Every Critical, Important and Optional finding from both reviews is either applied or explicitly recorded above.

**rev 1 (2026-08-01)** — both blocking decisions locked by Felix: **D-PARITY = ship unconditionally**, **D-LADDER = local ladder**. Folded in:

- Header decision table → locked, with the consequence of each choice.
- §3.1/§3.4 — `local_ladder()` needs **no code change**; §3.4 turned from "if it flips" into the recorded rejected alternative, with the accepted cost (floor 22 ⇒ discrimination restored, filterability not) stated as a first-class consequence rather than a footnote.
- **§7 step 2 is new**, from a grep run during the fold: **four** comments assert the local ladder is "Bowtie 2 `--local` only" (`mapq.rs:2-3`, `:10-19`, `:18`, `:140-141`) plus `local_ladder()`'s open-question note (`config.rs:168-173`). #1080 *tightened* those lines deliberately, so they read as authoritative and would plausibly have survived a diff-only review — the same stale-comment class both prior reviews caught.
- §7 step 8 — flagged that `alignment.md`'s "This applies to Bowtie 2 only" is about `--local` **mode** (still true) and must not be conflated with the ladder (no longer Bowtie 2-only).
- §7 step 9 — CHANGELOG requirements made concrete (the five unique-read values, the two second-best directions, the floor-vs-filterability sentence, the retired byte-identity claim).
- §8 A4/A5 restated (decided ≠ verified; A5 promoted to top residual risk). §10 both rows → Resolved with rationale, leaving one non-blocking Open question. §11 remaining risks re-ranked, 6 items.

**rev 0 (2026-08-01)** — initial plan, written after `SPIKE.md` cleared the `end_bonus` blocker. Precedents: `plans/07292026_local-mapq-denominator/PLAN.md` (rev 1) for structure and for the `match_bonus`-gating rationale it established; `plans/07312026_hisat2-local-ladder/PLAN.md` for the honest-limitation framing and the floor-before-ceiling lesson from its §9.

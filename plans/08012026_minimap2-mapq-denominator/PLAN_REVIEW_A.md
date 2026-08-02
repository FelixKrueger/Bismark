# PLAN_REVIEW_A — minimap2/rammap MAPQ denominator (`plans/08012026_minimap2-mapq-denominator/PLAN.md`, rev 1)

**Reviewer:** A (independent; no coordination with Reviewer B)
**Target:** `/Users/fkrueger/Github/Bismark/plans/08012026_minimap2-mapq-denominator/PLAN.md` rev 1
**Tree:** branch `plan/mapq-minimap2-denominator`, base `dev` `172da96` (verified `git log`)
**Verdict:** **Not ready as-is — 1 Critical, 7 Important.** The reasoning, the decisions' consequences and *every one of the ~40 hand-derived MAPQ values* check out. What does not is one BAM fixture that cannot reach the branch it exists to cover, one edge-case row whose direction is backwards, and a validation set that is blind to exactly the fault class #1079's code review already caught once. All are cheap to fix; none touches the locked decisions.

---

## 0. What I actually checked (so the "ready" claim is auditable)

**Re-derived, not checked for self-consistency.** I transcribed both ladders from the **Perl** source (`bismark:3947-4076` end-to-end, `bismark:4082-4177` local — read directly, not from `mapq.rs`) into an independent Python oracle, plus the pre/post-fix denominators, and ran the plan's tables through it:

| Plan cells | Result |
|---|---|
| §3.3 table 1 (5 rows × old / new-local / new-e2e = 15 values) | **15/15 correct** |
| §3.3 table 2 (5 rows × 3 = 15 values) **and every "why" column** | **15/15 correct**, and the stated rung/leaf reasoning is right in all five rows |
| §3.3 length-invariance at 50/100/150/250/1000 bp | **44/44/41/28/22 at every length** — confirmed |
| §7 step 7 BAM cells (a) 42→44, (b) 42→28, (c) 42→22, (d) 27→11 | **all four numeric values correct**; (b)'s stated margins (0.92 above the 0.4 rung, 0.4 below 0.5) are exact; (c) is indeed **0** under the e2e ladder |
| §1's summary rows (42 degenerate; 33/25 with second best) | correct |

So the #1080 failure mode the review brief warned about — *right number, wrong reasoning* — does **not** occur in §3.3. It occurs once, in §7 step 7 cell (d): the numbers are right and the fixture that is supposed to produce them cannot (C1).

**Verified against the tree** (sampled ~25 citations; all landed within 0–3 lines): `config.rs:80,82-225,123-127,131-143,161-176,793-802`; the `--local` reject (comment at `:674`, the actual `matches!` at `:677`); `mapq.rs:1-19,29-43,48-135,143-220,661-699,711-736`; `options.rs:233,256-289,376-404`; `merge.rs:332-350,367,740`; `aligner_cli.rs:2671,2811`; `mod.rs:591,968,1319,1662,2060`; `inprocess.rs:252,258` + `second_best: None` at `:224,277`; `CHANGELOG.md` placeholder bullet; `rust/README.md:161`; `docs/.../alignment.md:111`; `EXPECTED=13` with no aligner cell (`rust_ci.yml:199-217`).

**Scope claims in §5 — all four independently confirmed true:** 5-Base passes the aligner's MAPQ through and never calls `calc_mapq`; minimap2/rammap are SE-only and `--local`/`--combined_index` are rejected (`config.rs:677`, `reject_unsupported_paired_aligner`, `reject_combined_index_unsupported` requires Bowtie 2/HISAT2 at `:929,:983`); rammap in-process emits `AS:i:{m.score}` with `second_best: None`; and nothing outside `score_model_construction_matrix` asserts a minimap2/rammap MAPQ. I also confirmed a claim the plan did not make but depends on: `--mm2_maximum_length` **drops** over-long reads (`convert.rs:332-336`, `continue`) rather than truncating them, so A9's "read length == the length minimap2 scored" is safe.

---

## 1. Critical

### C1 — §7 step 7 cell (d)'s fixture puts the scores on the wrong instances; as written it cannot reach the second-best branch at all

The plan specifies *"a fake that also maps on the **GA** index (e.g. CT `AS:i:12`, GA `AS:i:11`) ⇒ `bestDiff 1` ⇒ old **27**, new **11**"*. That arrangement never produces a second best.

`merge.rs` stores an alignment only under `if overwrite` (`merge.rs:289-310`), and `overwrite` is set only when `alignment_score >= best_as_so_far` (`merge.rs:256-280`). Streams are visited in index order (`merge.rs:207`), index 0 = the `BS_CT` instance. So:

- index 0 (CT, `AS:i:12`) → `best_as_so_far = 12`, inserted.
- index 1 (GA, `AS:i:11`) → `11 >= 12` is false → `overwrite = false` → **the GA record is never inserted**.
- `entries.len() == 1` → `second_for_mapq = b.second_best` (`merge.rs:332-335`), which is `None` for minimap2/rammap because `s2:i:` is ignored by construction (`align.rs:138-142`; pinned at `align.rs:982` and `:995`).
- Result: `calc_mapq(6, None, 12, None, …)` = **44**, i.e. cell (d) collapses into a duplicate of cell (a).

The repo already encodes the correct arrangement: `best_across_instances_by_score` (`merge.rs:929-947`) deliberately puts the *lower* AS (`-6`) on instance 0 and the higher (`0`) on instance 1, and only then gets `alignment_score_second_best == Some(-6)`.

**Fix:** swap the scores — CT (index 0) `AS:i:11`, GA (index 1) `AS:i:12`. The expected values are unchanged (verified: old **27**, new **11**), so only the fixture moves. Two further constraints the plan should state, because they are what will otherwise make the cell look "unbuildable":

- `insert_alignment` keys on `chromosome:pos` (`merge.rs:399`), so the two records must be at **different POS**.
- The winner is now index 1, which prepends 2 genomic bases and needs `pos >= 2` (`methylation.rs:155-161`), i.e. POS ≥ 3; with `make_genome_mmi`'s 8 bp `chr1` (`aligner_cli.rs:2656`) POS must be **exactly 3** for the `read_len + 2` extraction to succeed. Give the new fixture a longer chromosome instead of threading that needle.

**Why Critical rather than Important:** the assertion would fail during implementation, and the plan supplies a pre-authorised escape hatch (*"if that cannot be built reliably, say so and cover the branch at unit level only"*). The likely outcome is therefore that the **only end-to-end coverage of the half of the blast radius §1 calls "the larger behavioural change"** gets dropped for a reason that is not real.

---

## 2. Important

### I1 — §3.5's non-default `--score_min` row states the wrong direction, and A6's real consequence is that the defect comes back

§3.5 says: *"A large `|slope|` inflates `diff` and pushes everything toward the floor."* It does the opposite. `bestOver = AS − scMin` and `diff = perfect − scMin` **both** contain `−scMin`, so

```
ratio = (AS + |scMin|) / (perfect + |scMin|)  →  1   as |scMin| grows
```

Measured (len 100, AS = 200/160/120/80/40 i.e. 100 %→20 % of perfect):

| `--score_min` | MAPQ across the score range |
|---|---|
| `L,0,-0.2` (default) | 44 / 44 / 41 / 28 / 22 ← the intended fix |
| `L,0,-1` | 44 / 44 / 42 / 41 / 28 |
| `L,0,-5` | 44 / 44 / 44 / 44 / 42 |
| `L,0,-20` | **44 / 44 / 44 / 44 / 44** ← the #1081 defect, fully restored |

So A6 is not merely "documented, not fixed": under a steep `--score_min` the fix is *undone*, silently, and in the direction a user will not notice (MAPQ stays high). Worse, `--score_min` is a **no-op** for minimap2 (`options.rs:233` discards the string), so a user copying a permissive `L,0,-0.6`-style setting from a Bowtie 2 recipe changes only MAPQ.

The reason the plan got this backwards is worth recording, because it is the plan's own justification: §1's non-goal table calls keeping `scMin` *"the faithful analogue of Bowtie 2's `perfect − scMin`"*. Structurally true, behaviourally inverted — Bowtie 2-local's `scMin` is **positive** (`G,20,8`), so a steeper function *shrinks* `diff` and **spreads** MAPQ; minimap-like's `scMin` is **negative**, so a steeper slope inflates both terms and **compresses** MAPQ upward. Verified:

```
Bowtie2-local  G,20,8   -> 44 42 28 22 22      G,20,30 -> 44 22 22 22 22   (spreads)
minimap-like   L,0,-0.2 -> 44 44 41 28 22      L,0,-5  -> 44 44 44 44 42   (compresses)
```

**Action:** correct the row; restate A6's consequence as "a steep `--score_min` re-saturates minimap2/rammap MAPQ" rather than "pushes toward the floor"; add a unit cell that pins it (documenting the limitation, not fixing it); say it in the docs/CHANGELOG alongside "`--score_min` does not reach minimap2". Whether to revisit the non-goal itself (`diff = max(1, perfect)`) is Felix's call — but the choice should be made against the measured behaviour, not against the inverted description.

### I2 — the new validation is blind to a wrong `scMin` **form**, which is exactly #1079's code-review finding H1

Injected a Log-instead-of-Linear `scMin` for minimap-like and compared against every cell §9 enumerates:

| Gate | correct | wrong form | verdict |
|---|---|---|---|
| V6/V7 BAM (a)(b)(c)(d) | 44, 28, 22, 11 | 44, 28, 22, 11 | **ships green** |
| V1 (len 50/100/1000, 15 cells) | 44/44/41/28/22 × 3 | identical | **ships green** |
| V2 (5 second-best cells) | 11, 34, 39, 11, 18 | identical | **ships green** |
| V3 (floor) | 22, 22 | 22, 22 | **ships green** |

Not one value cell in the plan can see it. This is `plans/07292026_local-mapq-denominator/CODE_REVIEW_B.md:66,192` verbatim — H1 there was "make the wiring cell form-sensitive", fixed by choosing `--score_min G,1,1` so that **24 / 22 / 44** separate *correct / wrong form / lost match bonus* (that reasoning is now the comment at `aligner_cli.rs:88-92`). The plan cites this lesson in V10's rationale and then does not apply it to its own cells.

The gap is narrow rather than total — `score_min_params_aligner_and_mode_defaults` (`options.rs:577-620`) already pins `Minimap2 → (0.0, -0.2, ScoreMinForm::Linear)`, and V5's `assert_eq!(m, minimap_like(0.0, -0.2))` would catch a Log typo inside the new constructor (`form` participates in `PartialEq`). But the plan names neither as the form guard, so the coverage is accidental, and that test has **no `Rammap` cell**.

**Cheapest complete fix — one extra BAM cell.** On the existing 6 bp fixture at the default `--score_min`, `AS:i:7` separates *five* fault modes at once (verified):

| fault | MAPQ at `AS:i:7`, 6 bp |
|---|---|
| correct (`match_bonus` 2.0, Linear, local ladder) | **41** |
| wrong `scMin` form (Log) | **36** |
| lost match bonus (0.0) | **42** |
| `MINIMAP2_MATCH_BONUS = 1.0` | **44** |
| ladder forced end-to-end | **24** |

(`AS:i:3` works too: 24 / 22 / 42 / 36 / 3. The plan's chosen `AS:i:5` and `AS:i:2` are both form-blind — 28/28 and 22/22.)

**Also:** add `SCORE_MIN_CELLS` (already a const at `mapq.rs:233`) as an axis to V4's sweep — the plan's V4 is `len × AS × second-best` only, with no `--score_min` axis anywhere in V1–V12. #1079 introduced that axis precisely because it is where this defect class hides, and it is what would have surfaced I1. Add a form injection as V10 (v).

### I3 — the stale-comment sweep (§7 step 2) misses three false assertions in `config.rs`, one of them at the exact line being edited

All four listed sites are in `mapq.rs`, and three of the four (`:2-3`, `:10-19`, `:18`) are inside the **same** contiguous header block (L1-19) — so the mapq.rs inventory is really 2 edit locations, not 4. Meanwhile `config.rs` has three assertions that D-LADDER makes false and the plan does not list:

| Site | Text | Status after the fix |
|---|---|---|
| `config.rs:90-91` | *"only Bowtie 2-local has a nonzero perfect score, and only Bowtie 2-local is emitted the `G` form"* | first clause **false** |
| `config.rs:98-99` | *"Perfect-alignment score per base; nonzero only for Bowtie 2 `--local`."* | **false** |
| `config.rs:117-122` | *"Only Bowtie 2 --local scores matches positively; every other mode's best possible score is 0 — including HISAT2 …"* | **false**, and it sits directly above the `match_bonus` arm §3.1 rewrites — §3.1's snippet keeps it |

The last one is the sharpest case of the very class §11 risk 6 describes ("would have survived review as obviously still true"), because the diff will show the arm changing three lines below it.

**Drive-by from the same grep:** `options.rs:81` says HISAT2-local's local-ness is *"the dropped `--no-softclip` in the HISAT2 tail + the local MAPQ ladder"* — already false since #1080 (HISAT2-local takes the **end-to-end** ladder). Pre-existing, missed by #1080; the sweep step is the natural place to fix it.

### I4 — two byte-identity claims outside the minimap2 row are not scheduled for restatement, and one of them contradicts the new CHANGELOG bullet in the same section

§7 steps 8/9 cover the `rust/README.md:161` minimap2 clause and the placeholder CHANGELOG bullet. Three more statements become false:

1. **`rust/README.md:161`**, same row: *"⚠️ **Byte-identity is the END-TO-END contract:** … **End-to-end (every aligner) is unaffected and stays byte-identical.**"* This is the more categorical claim and the one a reader will trust.
2. **`rust/README.md:161`**, same row: *"**Phase-5 combined 10M gate: all 13 cells byte-identical** (Bowtie 2 + HISAT2 SE+PE + **minimap2 SE** × {dir, non-dir, pbat} …)"* — a historical gate record whose minimap2 cells would no longer reproduce. Annotate rather than delete.
3. **`CHANGELOG.md:12`** — the **#1079 bullet, in the same `## Unreleased` section**: *"The **default end-to-end path is unaffected and stays byte-identical** — it keeps `abs(score_min)` untouched, because the new denominator is applied only where the perfect score is nonzero."* After #1081 that final clause is exactly what changes, so the shipped release notes would contradict themselves two bullets apart. #1080's M2 lesson was "don't add a competing bullet"; the harder problem here is an **existing neighbouring bullet that becomes wrong**.

### I5 — §7 step 8's reading of `docs/.../alignment.md:111` is wrong, and would preserve a false sentence

The plan says: *"⚠️ That paragraph's 'This applies to Bowtie 2 only' is about `--local` **mode** (still true — minimap2 has no `--local`), not about the ladder."* Read the actual sentence (`alignment.md:111`):

> Reported MAPQ values are normalised against that best possible score, so local-mode MAPQ differs from end-to-end MAPQ for the same alignment. **This applies to Bowtie 2 only:** HISAT2 exposes no `--local` option of its own and **always scores matches as 0**, so with `--hisat2 --local` the best possible score is 0 …

The antecedent of "This" is *normalising against the best possible score*, and the justification that follows is the **match bonus**, not `--local`. That is precisely what this change extends to minimap2/rammap, so the sentence becomes **false**. The plan's instruction ("reword") and its stated reason ("still true") pull in opposite directions; the likely resolution is "leave it".

Placement also deserves a thought: the paragraph lives under the `--local` bullet, a flag minimap2 rejects. `--minimap2` is documented at `alignment.md:308-320` and `--rammap` at `:335` — the new prose belongs there (or in both, cross-referenced).

### I6 — the second-best half of the fix is strand-order-dependent and lands for only ~half of near-tie reads; the plan's framing overstates it

Same mechanism as C1, but as a claim about real data. For directional SE the cross-instance runner-up branch is reached **iff `AS(GA instance) > AS(CT instance)`** — strictly greater, because equal scores go to `Decision::Ambiguous` (`merge.rs:338-342`), and lower scores from the later instance are discarded (`merge.rs:289-310`). So for a read that aligns in both instances:

| winner | old | new |
|---|---|---|
| OB (index 1) wins by 10 at 100 bp | 25 | **11** ← the plan's headline improvement |
| OT (index 0) wins by 10 at 100 bp | 42 | **44** ← no second best is ever recorded |

Two reads with identical `(AS, AS₂)` pairs therefore get MAPQ 44 or 11 depending only on which strand won. That artefact is faithful pre-existing Perl behaviour, not a defect introduced here — but the fix **widens** it for near-ties from a 17-point spread (42 vs 25) to **33** points (44 vs 11). §1's *"the second branch … is the larger behavioural change"* and §7 step 9's *"reads with a cross-instance runner-up move in both directions"* are both true but read as if the near-tie improvement is general; it applies to roughly half of near-tie reads. Add it to §3.5 as an edge case and temper the CHANGELOG sentence.

### I7 — §3.5's `max(1, …)` clamp reachability is derived only at the default `--score_min`, and is wrong twice

The row says: *"Reachable only for `perfect − scMin ∈ [0,1)`, i.e. `2·len + 0.2·len < 1` ⇒ `len = 0`. Effectively dead here."* With `--score_min L,10,-0.2` — legal (shape-only validation), and literally a cell in the repo's own `SCORE_MIN_CELLS` (`mapq.rs:237`, *"positive intercept → scMin > 0 for short reads"*) — `perfect − scMin = 2.2·len − 10`, so the clamp fires for **every `len ≤ 4`**, and the quantity is **negative**, not in `[0,1)`. Outcome is benign (MAPQ 22 with or without the clamp, verified at len 1–4), so this is a reasoning defect rather than a behavioural one — but it is the same "derived at the default only" slip that produced I1, in the adjacent table row.

---

## 3. Optional

- **O1 — V9 has no valid home as written.** `ScoreModel::perfect` is fully private (`config.rs:196`, no `pub(crate)`), so `perfect(len) >= AS` cannot be asserted from `mapq.rs`, where §7 steps 4–6 put the new unit tests. Put it in `config.rs`'s `mod tests` (`config.rs:1536`) or derive it through the `pub(crate)` `normalize` (`perfect = diff + scMin`). §7 never assigns V9 a file.
- **O2 — three inventories are undercounts. Every conclusion drawn from them still holds; the counts don't.**
  - `calc_mapq` has **four** call sites, not two: `merge.rs:367`, `merge.rs:740`, **`combined.rs:339`, `combined.rs:711`**. §11 claims to have traced "the one reachable call site … the PE site at `:740` is unreachable" and never mentions `combined.rs`. I verified the combined sites are genuinely unreachable for these aligners (`reject_combined_index_unsupported` requires Bowtie 2/HISAT2, `config.rs:929,983`), so the scope claim survives — but a "traced every call site" claim should name all four.
  - `aligner_cli.rs` has **8** MAPQ assertions at 7 lines (156, 235, 602, 885, 886, 2269, 2367, 2629), not "the 4 … at L165, L241, L602 and L2630" (§2). The extras are Bowtie 2 PE (`:885-886`) and HISAT2-local SE/PE (`:2269`, `:2367`) — so §5's load-bearing conclusion ("no minimap2/rammap MAPQ assertion exists anywhere") is still **true**, which I checked case by case.
  - `ScoreModel::end_to_end` has **47** call sites, not "~29" (§4). This *strengthens* the plan's recommendation to keep the name.
- **O3 — V11's frozen list is under-inclusive.** Add `score_min_params_aligner_and_mode_defaults` (`options.rs:577`), `local_hisat2_uses_the_linear_form_and_end_to_end_ladder` (`mapq.rs:554`), the HISAT2-local BAM cells (`aligner_cli.rs:2269`, `:2367`), Bowtie 2 PE (`:885`), and `select_unique_best_mapq_equals_calc_mapq` (`combined.rs:1133`). All pass untouched; naming them makes "green with no expectation edits" a checkable statement.
- **O4 — V3 is a ladder guard, not a bonus guard, and the test comment should say so.** With `MINIMAP2_MATCH_BONUS = 4.0` both V3 floor cells still return **22** (verified) — the floor is insensitive to the bonus magnitude, which V1/V2/V6 carry instead. Worth one line so a future reader doesn't over-trust the cell the plan calls "the single most important assertion in the change".
- **O5 — `assert_ne!(end_to_end, minimap_like)` (V5) does not close the footgun it is credited with.** It stops the two *constructors* from colliding; it cannot stop a new minimap2-shaped test from reaching for `end_to_end`. `merge.rs` already has 9 `end_to_end` uses in aligner-agnostic selection tests, including `rammap_supplementary_does_not_displace_primary` (`merge.rs:910`). None asserts a MAPQ value, so nothing breaks — but the plan should state that merge/combined `end_to_end` uses are deliberately aligner-agnostic and stay, so the next reader doesn't "fix" them.

---

## 4. Assumptions review

| # | Assessment |
|---|---|
| A1 (`AS ≤ 2·len`) | Sound; the F1/F2 source chain is the right kind of argument (a bound cannot be sampled) and F3's "perfect scores exactly `2·len`" is the correct discriminating measurement. |
| A2 (match score unreachable) | Verified: `minimap2_options` (`options.rs:256-289`) emits a closed string; presets are selectors only. |
| A3 (rammap ≡ minimap2, source-only) | Honestly labelled. Note V7 makes the *wiring* testable even though the *semantics* stay source-verified — worth saying so, since it is a real partial mitigation the risk list doesn't credit. |
| A4 (local ladder) | The "decided ≠ verified" framing is right and the §3.4 rejection reasoning is sound: I confirmed the e2e ladder's floor is **0** at 20 % of perfect and the local ladder's is **22**, so the floor argument is factual, not rhetorical. |
| A5 (nobody depends on 42/33) | Correctly promoted to top risk. The "degenerate values carry no information" mitigation is genuinely strong for the no-second-best branch; it is **weaker than stated for the second-best branch**, where today's values (33/25/27) *do* vary with `bestDiff` and are not degenerate. Worth splitting. |
| **A6 (`scMin` is a fiction)** | **The load-bearing one, and its consequence is stated backwards — see I1.** The assumption itself is correct and well-sourced (`options.rs:233` discards the string; `score_min_params` still returns `(0.0, −0.2)` at `options.rs:380`). What is wrong is the direction of the effect and, therefore, how serious it is. |
| A7 / A8 / A9 / A10 / A11 | Verified or correctly labelled. A9 is stronger than the plan claims: the length fed to `calc_mapq` is the **FastQ** read length (`merge.rs:367` `sequence.len()`, documented at `merge.rs:513`), not the SAM SEQ, so clipping cannot reach it; and `--mm2_maximum_length` drops rather than truncates (`convert.rs:332-336`). |

---

## 5. Efficiency

Nothing to add — §11's assessment is correct. One `match` arm at resolve time (once per run), one multiply and one compare per accepted alignment inside arithmetic #1079 already added, `ScoreModel` unchanged in size and still `Copy`. No allocation, no plumbing, no measurable cost. The single-PR argument (§2 "Blast radius") is sound: unlike #1079 there are no threading edits to isolate.

---

## 6. Alternatives

- **`end_to_end` → `monotone` rename (§10 Open).** Agree with the plan's recommended answer: don't. The real cost is ~47 mechanical edits (O2), not ~29, against a 4-line semantic diff.
- **Dropping `scMin` from the minimap-like denominator (`diff = max(1, perfect)`).** The plan declares this a non-goal on faithfulness grounds. That is defensible, but I1 shows the accepted cost is larger than the plan believes — a steep `--score_min` restores the defect. Recommend keeping the non-goal *and* documenting the consequence precisely, so the decision is on the record against the measured behaviour. If it is ever revisited, it becomes a separate issue with its own re-baseline, exactly as §3.4 says of filterability.
- **Sequencing (rammap first) and MAPQ pass-through** — both rejected on sound grounds (§6, §1). I have nothing to add; §6's "the fallback is *defer both*, never ship rammap alone" is the right invariant to have written down.

---

## 7. Action items

| # | Priority | Item | Where |
|---|---|---|---|
| **C1** | **Critical** | Cell (d): swap the scores — CT (index 0) `AS:i:11`, GA (index 1) `AS:i:12`; a later-instance record with a *lower* AS is never stored (`merge.rs:289-310`). Record the two fixture constraints (distinct POS; index-1 winner needs POS ≥ 3 with room for `read_len + 2`, so give the fixture a longer chromosome). Expected 27 → 11 unchanged. | §7 step 7, V8 |
| **I1** | Important | Correct the `--score_min` direction: a steep `L,0,−s` pushes MAPQ to the **ceiling** and at `L,0,−20` restores the defect entirely (measured). Restate A6's consequence; add a unit cell pinning it; note the Bowtie 2-local sign asymmetry that made the "faithful analogue" reasoning misleading. | §1 non-goals, §3.5, A6, §7 step 8/9 |
| **I2** | Important | Make the wiring test form-sensitive (#1079 H1): add a BAM cell at `AS:i:7` → **41** (36 wrong form / 42 lost bonus / 44 bonus 1.0 / 24 e2e ladder). Add `SCORE_MIN_CELLS` as a V4 axis. Add a form injection as V10 (v). Cite `score_min_params_aligner_and_mode_defaults` as the upstream form guard in V11 and add its missing `Rammap` cell. | §7 step 5/7, V4, V10, V11 |
| **I3** | Important | Extend the stale-comment sweep to `config.rs:90-91`, `:98-99`, `:117-122` (the last sits above the edited arm). Note the mapq.rs "four sites" are two locations. Drive-by: `options.rs:81` is already false post-#1080. | §7 step 2 |
| **I4** | Important | Restate `rust/README.md:161`'s *"End-to-end (every aligner) … stays byte-identical"* and annotate the Phase-5 13-cell gate line; amend the **#1079 CHANGELOG bullet** (`CHANGELOG.md:12`), which otherwise contradicts the new bullet in the same `## Unreleased` section. | §7 steps 8, 9 |
| **I5** | Important | Fix the reading of `alignment.md:111`: *"This applies to Bowtie 2 only"* is about the **denominator**, not `--local` mode, and becomes false. Consider putting the new prose in the `--minimap2`/`--rammap` sections (`alignment.md:308,335`). | §7 step 8 |
| **I6** | Important | Record that the runner-up branch fires only when `AS(GA) > AS(CT)`, so the near-tie 25 → 11 win lands for ~half of near-tie reads while the other half moves 42 → 44 — and the strand-order spread widens from 17 to 33 points. Temper §1 and the CHANGELOG. | §3.5, §1, §7 step 9 |
| **I7** | Important | Fix the clamp-reachability derivation: with the legal `L,10,−0.2` the clamp fires for `len ≤ 4` and `perfect − scMin` is negative, not in `[0,1)`. | §3.5 |
| **O1–O5** | Optional | V9's file (private `perfect`); the three undercounted inventories; V11's missing frozen tests; V3's insensitivity to bonus magnitude; the limits of `assert_ne!(end_to_end, minimap_like)`. | as cited |

**Bottom line.** Fix C1 and fold in I1–I4 and the plan is implementable; I5–I7 are prose/edge-case corrections that can ride the same revision. The arithmetic core of the plan is correct — I re-derived all of it against Perl and found no wrong value anywhere — and the locked decisions' stated reasoning is factually sound (the local ladder's floor really is 22 vs the e2e ladder's 0; `local_ladder()` really needs no code change; the "no oracle exists" premise really holds). What rev 1 has not fully closed is the *class* discipline it argues for elsewhere: one fixture that can't reach its branch, and a validation set that is blind to the one fault #1079 already shipped once.

# PROGRESS — minimap2/rammap MAPQ denominator (#1081)

**Plan:** [`PLAN.md`](./PLAN.md) · **Spike:** [`SPIKE.md`](./SPIKE.md) · **Issue:** [#1081](https://github.com/FelixKrueger/Bismark/issues/1081) (deferred from [#1079](https://github.com/FelixKrueger/Bismark/issues/1079))
**Last updated:** 2026-08-01 · **Base:** `dev` `172da96`, branch `plan/mapq-minimap2-denominator`

**Status legend:** 📋 Planned · 🔨 In progress · ✅ Done · ⛔ Blocked · ⏸ Awaiting user

| # | Pipeline step | Status | Notes |
|---|---|---|---|
| 0 | **Spike — does `-x sr`'s `end_bonus` break `perfect = 2·len`?** | ✅ Done | `SPIKE.md` + `spikes/spike_end_bonus.py` + `spikes/run.log`. **16/16 checks passed.** `end_bonus` is a ksw2 traceback-start tie-breaker only (`ksw2_extz2_sse.c:304`), never a score term; a perfect read under `-x sr` scores **exactly** `2·len` on real minimap2 2.31-r1302. Blocker cleared |
| 1 | Triage / verify report | ✅ Done | All issue claims re-verified against `dev` `172da96`. Two corrections found: the "42 everywhere" symptom is the **no-second-best** branch only (the with-second-best branch sits on its top rungs at 33/25 and moves in **both** directions); and `-x sr` reaches `calc_mapq` only via `--mm2_short_reads`, not via `--illumina_5base` |
| 2 | Plan written (rev 0) | ✅ Done | `PLAN.md`. Single PR (no #1079-style split — the seam already exists). Two decisions left to Felix: **D-PARITY** (ship at all) and **D-LADDER** (local vs end-to-end) |
| 2b | **Decisions locked → PLAN rev 1** | ✅ Done | Felix 2026-08-01: **D-PARITY = ship unconditionally**, **D-LADDER = local ladder**. Folded; see `PLAN.md` §12. The fold also surfaced a **new step**: four comments asserting the local ladder is "Bowtie 2 `--local` only" (`mapq.rs:2-3, 10-19, 18, 140-141`) plus `local_ladder()`'s open-question note (`config.rs:168-173`) are now false — §7 step 2 |
| 3 | Manual review of rev 1 (Felix) | ✅ Done | Felix requested agent review directly after locking the two decisions |
| 4 | **Agent review (dual `plan-reviewer`)** | ✅ Done | `PLAN_REVIEW_A.md` + `PLAN_REVIEW_B.md`, independent fresh contexts. **Both verdicts: not ready as-is**, with **the same single Critical** (cell (d)'s fixture cannot reach the second-best branch). A: 1 Critical + 7 Important + 5 Optional. B: 1 Critical + 9 Important + 5 Optional. 8 findings reached independently by both; 1 contradiction, resolved in A's favour |
| 4b | **Fold both reviews → PLAN rev 2** | ✅ Done | Every Critical/Important/Optional applied or explicitly recorded. See `PLAN.md` §12. Three most consequential: the cell-(d) fixture recipe; the `--score_min` direction was **backwards** (`L,0,−20` silently restores the defect); the spike traced the **wrong rammap field**. Validation grew V1–V12 → **V1–V15** (+ doc gate, + non-default `--score_min`, + a live CI gate for `AS ≤ 2·len`). `SPIKE.md` gained a §8 corrections section |
| 4c | Felix review of rev 2 | ✅ Done | "implement" trigger given 2026-08-01 |
| 5 | **Implement** | ✅ Done | fmt clean · clippy **0 warnings** · **2120 pass / 0 fail** (was 2109). **Every hand-derived value matched on the first run** — no expectation edits. Cell (d)'s corrected fixture returns **11**, so no escape hatch was needed. **All 6 fault injections verified failing then reverted** (0 `TEMP FAULT` residue): (v) drop-Rammap fails the rammap cell while the mm2 cell stays green; (vi) production-form fails cell (b) at **36 vs 41**. See `PLAN.md` §12 |
| 6 | Verify (dual `code-reviewer` + `plan-manager`) | ✅ Done | `CODE_REVIEW_A.md` · `CODE_REVIEW_B.md` · `COVERAGE.md`. Both reviewers "ship after the one HIGH" — **and it was the same HIGH, reached independently**: the "floor is 22 / `-q 20` stays a no-op" claim is **false** (the second-best branch reaches **2**), root-caused to `PLAN.md` §3.4. Both also independently found a **CI break** the local gate could not see (`--rammap` defaults to in-process under `--features rammap-inprocess`); B fixed it. Coverage **INCOMPLETE — 5 items**, all docs or one test cell |
| 6b | **Apply review findings → PLAN rev 3** | ✅ Done | Felix's calls: **scope the floor claim and claim the win**; **all gaps + MEDIUMs, skip LOWs**. All 5 coverage gaps + A MEDIUM-1 + B MEDIUM-1/2/3 + 3 record corrections applied. fmt clean · clippy **0** · **2120 pass / 0 fail** · `--features rammap-inprocess` rammap tests **4/4** · V13 gates all pass · version literals `3.1.0`. See `PLAN.md` §12 "Post-review fixes" |
| 7 | File the in-process-rammap-preset issue | ✅ Done | **[#1092](https://github.com/FelixKrueger/Bismark/issues/1092)** — states that #1081 *worsens* it (the ignored preset now moves the MAPQ column too), and offers plumb-through vs never-silent-reject |
| 8 | CHANGELOG | ✅ Done | Placeholder bullet replaced; **the neighbouring #1079 bullet amended** so the two do not contradict each other in the same `## Unreleased` section. V13 doc gates all pass (stale-comment grep = 0, one #1081 bullet, README restated) |
| 9 | Commit + PR | 📋 Planned | Target `dev`. Nothing committed yet |
| 10 | Version bump | 📋 Release cut | All three literals stay at `3.1.0`; the cut is already a minor (3.2.0) from #1079/#1080 |

## Open questions

- Rename `ScoreModel::end_to_end` → `monotone` (~29 mechanical test edits) vs keep the name + `assert_ne!(end_to_end, minimap_like)` as the structural guard. Plan recommends keeping the name. **Non-blocking.** `PLAN.md` §4, §10.

## Key decisions

- **2026-08-01 (Felix)** — **D-PARITY: ship unconditionally.** No compat flag, no opt-in. The minimap2-SE byte-identity claim vs Perl v0.25.1 is **retired** (not narrowed — end-to-end is minimap2's only mode). Accepted because the current values are degenerate (42 / 33, informationless) and the cut already carries two deliberate MAPQ divergences, so users re-baseline once.
- **2026-08-01 (Felix)** — **D-LADDER: local ladder.** Needs no new mechanism: #1088's `match_bonus > 0.0` derivation already yields it. Ceiling 44, floor 22. Accepted cost, recorded in `PLAN.md` §3.4: **filterability is not restored** (floor 22 > a conventional `-q 20`) — this fixes discrimination only.
- **2026-08-01** — Spike before plan, because `end_bonus` could have invalidated the whole fix. It did not; `perfect = 2 × read_length` holds at every preset Bismark can select, and one `2.0` constant suffices.
- **2026-08-01** — **rammap-first evaluated and rejected** (`PLAN.md` §6). rammap's gate is concordance *against minimap2*, so moving rammap alone breaks the gate it actually has and contradicts its "apples-to-apples vs `--minimap2`" design premise. One change, both aligners.
- **2026-08-01** — Single PR, not #1079's two-PR split: that split existed to isolate ~50 threading edits, and #1079 already built the `ScoreModel` seam this fix drops into.

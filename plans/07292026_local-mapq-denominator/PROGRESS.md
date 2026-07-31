# PROGRESS — Local-mode MAPQ denominator (#1079)

**Plan:** [`PLAN.md`](./PLAN.md) · **Issue:** [#1079](https://github.com/FelixKrueger/Bismark/issues/1079) · **Reporter:** @9xg
**Last updated:** 2026-07-29

**Status legend:** 📋 Planned · 🔨 In progress · ✅ Done · ⛔ Blocked · ⏸ Awaiting user

| # | Pipeline step | Status | Notes |
|---|---|---|---|
| 1 | Triage / verify report | ✅ Done | All 4 claims verified at source, incl. upstream `unique.h` v2.5.5 fetch. Impact quantified (MAPQ saturates at 44; up to +22 too high). Reply posted: [comment](https://github.com/FelixKrueger/Bismark/issues/1079#issuecomment-5116863162) |
| 2 | Plan written (rev 0) | ✅ Done | `PLAN.md`. Decisions: unconditional fix; Bowtie 2 denominator + HISAT2 `scMin` form; HISAT2 perfect score deferred |
| 3 | Manual review (Felix) | ✅ Done | Felix reviewed rev 0, sent straight to agent review |
| 4 | Agent review (dual `plan-reviewer`) | ✅ Done | `PLAN_REVIEW_A.md` + `PLAN_REVIEW_B.md`. **Both verdicts: rev-1 required before implementing.** A: 3 Critical (C1 invariance, C2 int-vs-float oracle, C3 no wiring test). B: 2 Critical (same C1/C2) + 8 Important. ~12 findings agreed independently; 1 contradiction (step ordering — B right, D2 lands inside the "no-op" step) |
| 4b | Fold findings → PLAN rev 1 | ✅ Done | All Critical + Important folded; both contradictions resolved in B's favour. See PLAN §12 Revision History |
| 4c | Felix review of rev 1 | ✅ Done | "implement" trigger given 2026-07-29 |
| 5 | Implement | ✅ Done | Branch `rust/local-mapq-denominator` (off `origin/dev` `0a9eeb7`). Both PRs in one tree. fmt clean, **0 clippy warnings**, **2107 pass / 0 fail**. V7 proven by inverse-transform; V1 + V10 both verified to fail under injected faults. **Not committed** — see PLAN §12 |
| 6 | Verify (dual `code-reviewer` + `plan-manager`) | ✅ Done | Both reviewers **APPROVE**, no correctness defect. Coverage **INCOMPLETE — 6 items**. Reports: `CODE_REVIEW_A.md`, `CODE_REVIEW_B.md`, `COVERAGE.md` |
| 6b | Apply all review findings | ✅ Done | **All** findings applied (1 High, 6 Medium, ~12 Low) + all 6 coverage gaps closed. fmt clean, 0 clippy, **2109 pass / 0 fail**. Three fault injections re-verified. See PLAN §12b |
| 7 | File deferred issues | ✅ Done | **[#1080](https://github.com/FelixKrueger/Bismark/issues/1080)** HISAT2 perfect score · **[#1081](https://github.com/FelixKrueger/Bismark/issues/1081)** minimap2/rammap positive-AS. Both cited in CHANGELOG + code |
| 8 | CHANGELOG | ✅ Done | Under `## Unreleased`. **Version bump deferred to the release cut** (Felix, 2026-07-29) — all three literals stay at 3.1.0 |
| 9 | Commit + PRs | ✅ Done | Two stacked PRs off `dev`: **[#1082](https://github.com/FelixKrueger/Bismark/pull/1082)** refactor → **[#1083](https://github.com/FelixKrueger/Bismark/pull/1083)** fix. PR links posted on #1079 |
| 10 | **#1082 MERGED** | ✅ Done | Squash-merged to `dev` as `6129d4d` (14/14 green). ⚠️ Squash orphaned #1083's base → rebased `rust/local-mapq-denominator` with `git rebase --onto origin/dev ede8899`, verified tree byte-identical to the tested state, re-ran gates (2109 pass), force-pushed, retargeted #1083 to `dev`, deleted the merged branch |
| 11 | #1083 CI after rebase | ✅ Green | 15 pass / 1 skip (`deploy`, merge-only). `perl-oracle byte-identity` passes — independent confirmation the end-to-end path is untouched |
| 12 | #1083 review + merge | ⏸ Awaiting | Needs a human approving review |
| 11 | Version bump | 📋 Release cut | All three literals stay at 3.1.0; CHANGELOG entry sits under `## Unreleased` |

## Key decisions
- **2026-07-29** — Fix **unconditionally**, no compat flag. Rationale: Bismark's own `--local` docs (`bismark:9729`) already promise `perfect = --ma × read_length`; no `perl-oracle` CI cell covers local mode; current output is actively misleading (saturation). End-to-end stays byte-identical.
- **2026-07-29** — Scope = Bowtie 2 denominator (**D1**) + HISAT2 `ln()`→linear `scMin` (**D2**). HISAT2 **perfect score deferred** — Bismark's docs admit it is unknown.
- **2026-07-29** — Take the fix in-house rather than accept @9xg's offered PR (touches the byte-identity gate + needed the parity-policy call).
- **2026-07-29** — Design: collapse the `(intercept, slope, local)` trio into one `ScoreModel` rather than add a 4th scalar — *reduces* arity at ~10 threading sites and retires the existing `#[allow(clippy::too_many_arguments)]` at `combined.rs:535`.

## Open items
- **D-EDGE** — universal vs local-only `max(1, …)` clamp; `max(1,…)` ≠ `abs(scMin)` for reads < 5 bp, so end-to-end invariance must be *proven* (V1), not assumed. Evidence-driven, no user input needed.
- Version bump: minor (3.2.0) vs patch — confirm.
- CHANGELOG credit for @9xg — assumed yes.

## Milestones
- [x] Report triaged + verified against upstream Bowtie 2 — ✅ 2026-07-29
- [x] Public reply posted on #1079 — ✅ 2026-07-29
- [x] Plan rev 0 written — ✅ 2026-07-29
- [ ] Felix manual review of rev 0
- [ ] Dual plan-reviewer pass
- [ ] Implementation
- [ ] Dual code-review + coverage audit
- [ ] HISAT2 follow-up issue filed

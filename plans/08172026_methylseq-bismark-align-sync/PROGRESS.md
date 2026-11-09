# Progress: Sync methylseq's vendored bismark modules onto the merged `-p` threading model

**Last updated:** 2026-08-17

## Status

| Step | Status | Notes |
|------|--------|-------|
| Plan | ✅ Complete | `PLAN.md` — **rev-1**, dual-review findings folded |
| Plan Review | ✅ Complete | `PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md` — both REQUEST CHANGES, both folded |
| Impl Plan | ❌ Excluded | Went straight from reviewed plan to implementation on Felix's trigger |
| Implementation | ✅ Complete | Commit `721ae33c` (15 files, +212/−92); **PR [nf-core/methylseq#622](https://github.com/nf-core/methylseq/pull/622)** open against `dev`. CI: 95 pass / 4 red, all four attributed and none from the change; explanatory [note posted](https://github.com/nf-core/methylseq/pull/622#issuecomment-5319587585). Awaiting maintainer review. See PLAN.md §12 |
| Code Review | 📋 Planned | — |
| Coverage | 📋 Planned | — |

## Notes

- Completes the Q6 chain: `nf-core/modules#12385` merged 2026-07-29, but methylseq still pins
  `bismark/align` at the pre-#12385 sha, so the pipeline has not received the change.
- ~~Headline risk: HISAT2 output is thread-dependent by design, so two methylseq snapshot files are
  expected to re-baseline while every Bowtie 2 snapshot must stay byte-identical.~~ **Reversed in
  rev-1:** upstream ran this exact change at the same cpus (4) with snapshotted HISAT2 reads MD5s and
  nothing moved, so the expectation is now **no snapshot moves anywhere**; any movement is a defect
  signal. The residual risk moved to the *unmeasured* configurations — production HISAT2 at `-p 6` and
  the production non-directional `-p 3` × 4 cell, which no automated test in either repo covers.
- Every validation gate needs a methylseq clone plus docker and network test data — none of it can
  run from this repo.

## History

- 2026-08-17: Implementation → ✅ Complete (PR #622 open, CI attributed, note posted; awaiting maintainer review)
- 2026-08-17: Plan → ✅ Complete (rev-1 written; snapshot expectation inverted, 4 vacuous-pass gates closed)
- 2026-08-17: Plan Review → ✅ Complete (dual reviewers, both REQUEST CHANGES)
- 2026-08-17: Plan → ✅ Complete (PLAN.md created)

# Progress: Report the suite version, not `v0.25.1` (#1148)

**Type:** standalone
**Plan:** `PLAN.md`
**Last updated:** 2026-11-05

| Step | Status | Notes |
|---|---|---|
| Plan | ✅ Done | rev 2; Q1–Q4 resolved |
| Plan Review | ✅ Done | `PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md`; all items folded into rev 2 |
| Impl Plan | ✅ Done | Outline in PLAN.md |
| Implementation | ✅ Done | Uncommitted on `fix/1148-version-strings`; notes and validation in PLAN.md "Implementation notes" |
| Code Review | ✅ Done | `CODE_REVIEW_A.md`, `CODE_REVIEW_B.md`: no Critical/High; Medium guard gap + agreed Lows fixed |
| Coverage | ✅ Done | `COVERAGE.md`: INCOMPLETE (2 minor), both resolved post-fix |

## History

- 2026-11-05: Plan rev 0 created (Felix chose option 1: suite version everywhere).
- 2026-11-05: Rev 1: Q1 (drop the banner's "last modified" clause), Q2 (nf-core extractor md5 snapshots affected) and Q3 (reply when the PR opens) resolved.
- 2026-11-05: Dual plan review; rev 2 folds in all items. Q4 (keeping `unknown` out of outputs: `build.rs` treats it as unset) resolved.
- 2026-11-05: Implemented. 13 CI oracles and the summary oracle pass with Perl. Linux: 2341/0. Real-data gate and phase_h_smoke pass on oxy.
- 2026-11-05: Dual code review and coverage audit; guard fixed to skip `#[cfg(test)]` items wherever they are, smoke-script and docs fixes applied. Next: commit, PR into `dev`, #1148 reply.

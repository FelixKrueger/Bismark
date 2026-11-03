# Progress: 5-Base Bowtie 2 defaults

**Type:** standalone
**Plan:** `PLAN.md`
**Last updated:** 2026-11-02

| Step | Status | Notes |
|---|---|---|
| Plan | ✅ Complete | rev 3: quick recheck (PLAN_REVIEW_R2.md) applied; Felix approved rev 2 subject to the recheck |
| Plan Review | ✅ Complete | PLAN_REVIEW_reviewer-A.md, PLAN_REVIEW_reviewer-B.md (rev 1) |
| Impl Plan | ✅ Complete | Outline in PLAN.md |
| Implementation | 🚧 Implementing | Outline 1 builder `72bbf7b`; 2 harness `909182d`; 3 PREREGISTRATION.md; 4–6 oxy run pending |
| Code Review | 📋 Planned | |
| Coverage | 📋 Planned | |

## History

- 2026-11-01: Plan created.
- 2026-11-01: rev 1 — Q1–Q4 resolved (S3 symlinks, --dovetail default + no-harm check, both samples, HISAT2 out of scope).
- 2026-11-02: Dual plan review of rev 1; rev 2 written.
- 2026-11-02: Quick recheck of rev 2 (fix first: N1 duplex without UMIs + N2–N7); rev 3 written.
- 2026-11-02: Implementation started: builder + tests (`72bbf7b`), harness (`909182d`), PREREGISTRATION.md frozen.
- 2026-11-02: 10M dry run: the harness works, but the `--dovetail` overhang bases look non-genomic (R2/R4/R5/R6 fail). Paused before the screen B-new cells, awaiting Felix.
- 2026-11-02: Felix chose to drop `--dovetail` (rev 4); Q5 resolved as a no-op. Dual review of rev 4 (PLAN_REVIEW_R4_A/B.md); rev 5 written.
- 2026-11-03: Dual review of rev 5 (PLAN_REVIEW_R5_A/B.md); rev 6 written.

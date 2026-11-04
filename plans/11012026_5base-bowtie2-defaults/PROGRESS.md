# Progress: 5-Base Bowtie 2 defaults

**Type:** standalone
**Plan:** `PLAN.md`
**Last updated:** 2026-11-04

| Step | Status | Notes |
|---|---|---|
| Plan | ✅ Complete | rev 6 (`--maxins 800` only; rev 3 kept as history) |
| Plan Review | ✅ Complete | rev 1: `PLAN_REVIEW_reviewer-A/B.md`; rev 2: `PLAN_REVIEW_R2.md`; rev 4: `PLAN_REVIEW_R4_A/B.md`; rev 5: `PLAN_REVIEW_R5_A/B.md` |
| Impl Plan | ✅ Complete | Outline in PLAN.md |
| Implementation | ✅ Complete | builder `d27b11e`, harness `1c126c3`, PREREGISTRATION rev 2 `6e0aa32`; run PASS (RESULTS.md); docs `7295720`; review cleanup |
| Code Review | ✅ Complete | `CODE_REVIEW_A/B.md`: no Critical or High findings; Medium and Low items applied in the cleanup commit |
| Coverage | 🚧 Implementing | `COVERAGE.md` verdict INCOMPLETE (4 small gaps), all addressed in the cleanup commit; not re-audited |

## History

- 2026-11-01: Plan created.
- 2026-11-01: rev 1 — Q1–Q4 resolved (S3 symlinks, --dovetail default + no-harm check, both samples, HISAT2 out of scope).
- 2026-11-02: Dual plan review of rev 1; rev 2 written.
- 2026-11-02: Quick recheck of rev 2 (fix first: N1 duplex without UMIs + N2–N7); rev 3 written.
- 2026-11-02: Implementation started: builder + tests (`72bbf7b`), harness (`909182d`), PREREGISTRATION.md frozen.
- 2026-11-02: 10M dry run: the harness works, but the `--dovetail` overhang bases look non-genomic (R2/R4/R5/R6 fail). Paused before the screen B-new cells, awaiting Felix.
- 2026-11-02: Felix chose to drop `--dovetail` (rev 4); Q5 resolved as a no-op. Dual review of rev 4 (PLAN_REVIEW_R4_A/B.md); rev 5 written.
- 2026-11-03: Dual review of rev 5 (PLAN_REVIEW_R5_A/B.md); rev 6 written.
- 2026-11-03: Rev 6 implementation: builder `d27b11e`, harness `1c126c3`, PREREGISTRATION rev 2 `6e0aa32`; oxy streams launched.
- 2026-11-04: Run complete, every pre-registered check PASS (NA12878 gate, both cross-checks, HG002 A1–A7 and genome-wide). RESULTS.md written.
- 2026-11-04: Docs, CHANGELOG and README; dual code review (no Critical or High) and coverage audit (4 gaps); one cleanup commit; PR into dev.

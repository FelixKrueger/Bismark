# Progress: Move legacy Perl toolchain to `legacy_perl/`

**Last updated:** 2026-08-08

## Status

| Step | Status | Notes |
|------|--------|-------|
| Plan | ✅ Complete | PLAN.md (rev 2 — all dual-review findings folded in; awaiting implementation trigger) |
| Plan Review | ✅ Complete | PLAN_REVIEW_A.md + PLAN_REVIEW_B.md — both REQUEST CHANGES; all Critical+Important findings folded into rev 2 |
| Implementation | ✅ Complete | All V-rows green: V1 sabotage = exactly the 20 predicted loud failures + 13 silent greens demonstrated; V2 = 73 ok (baseline+1, 0 fail); V3 layout gate red-under-sabotage; fmt+clippy×2 clean; PLAN.md §12 has notes + 6 minor deviations |
| Code Review | ⬜ Not started | Dual reviewers post-implementation |
| Coverage | ⬜ Not started | plan-manager audit |

## History

- 2026-08-08: Plan → rev 2 (dual-review findings folded in: +9 consumers, golden-script up-count repair, legacy_perl_layout.rs existence test, V0 baseline + rewritten V1–V9, perl-oracle scope corrected, packager accepted-as-broken as Open-5, counts/sizes fixed, §12 declined-scope register added)
- 2026-08-08: Plan Review → ✅ Complete (both REQUEST CHANGES; near-identical Critical sets independently derived; 2 inter-reviewer discrepancies resolved at source — golden scripts already broken today, 11/12 summary oracles skip silently)
- 2026-08-08: Plan Review → 🔄 launched (dual independent reviewers, fresh contexts, PLAN_REVIEW_A/B.md)
- 2026-08-08: Plan → rev 1 (Felix's manual review: Open-1 resolved — Python coverage helpers stay at root; Open-2/3/4 defaults accepted)
- 2026-08-08: Plan → ✅ Complete (PLAN.md rev 0 written; full consumer inventory traced at source: 8 Rust test path literals, 2 src drift guards, 3 golden scripts, 43 ci_tests.yml invocations, prose sweep; release.yml/Dockerfile/scripts/validation verified as non-consumers; 4 Open questions, 0 Critical)

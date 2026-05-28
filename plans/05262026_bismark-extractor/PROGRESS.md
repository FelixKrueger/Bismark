# `bismark-extractor` — overall progress (umbrella #798)

**Design contract:** `rust/bismark-extractor/SPEC.md` (rev 2, in-repo).
**Integration branch:** `rust/iron-chancellor`.
**Crate version (in `extractor-phase-c`):** `1.0.0-alpha.2` (Phase B). Phase C will bump to `1.0.0-alpha.3`.

## Phase status table

| Phase | Scope | LOC est. | Status | Sub-issue | Plan file |
|-------|-------|----------|--------|-----------|-----------|
| A | Workspace scaffold + CLI + flag-validation | ~500 | ✅ **merged** (commit `144ca2d`, PR #847) | #846 | (spec recon, `~/.claude/plans/create-a-team-of-snoopy-music.md`) |
| **B** | **SE extraction loop + XM routing + eager output-file map + splitting-report skeleton + M-bias accumulator** | ~1,100 LOC actual | 🟢 **PR up: [#849](https://github.com/FelixKrueger/Bismark/pull/849)** (closes [#848](https://github.com/FelixKrueger/Bismark/issues/848)); awaiting review + merge | [#848](https://github.com/FelixKrueger/Bismark/issues/848) | `PHASE_B_PLAN.md` rev 2 |
| **C** | **PE extraction + overlap handling + `--ignore_r2` / `--ignore_3prime_r2` + `detect_paired_from_header` promotion to `bismark-io v1.0.0-beta.7` + Phase A bug-fix (AutoDetect `no_overlap`)** | ~600 | 📝 **plan rev 1 — folded both plan-review reports' findings; awaiting implementation trigger** | [#850](https://github.com/FelixKrueger/Bismark/issues/850) | `PHASE_C_PLAN.md` rev 1 |
| **D** | **M-bias.txt writer + SE/PE section ordering + 3 retroactive SPEC fixes** (accumulator wired in Phases B+C; finalize reorder per Perl `:2463`→`:316`) | ~500 LOC / ~700 actual | ✅ implementation complete (rev 2; 566 tests green; both code-reviewers approve, plan-manager 0 gaps); ready to commit + PR | [#852](https://github.com/FelixKrueger/Bismark/issues/852) | `PHASE_D_PLAN.md` rev 2 |
| **E** | **`--comprehensive` / `--merge_non_CpG` / `--yacht` output mode dispatch + `--gzip` + `--mbias_only` enablement + `mbias_only_silence` kernel param** | ~400 / ~530 actual | ✅ **merged** (commit `442be508`, PR #855) | [#854](https://github.com/FelixKrueger/Bismark/issues/854) | `PHASE_E_PLAN.md` rev 1 |
| **F** | **`--multicore N` producer/worker/collector pipeline (byte-identical across N) per SPEC §6.4 + §9. Std::thread::spawn workers (deviated from plan's rayon::scope after deadlock at N=1).** | ~700 / ~1100 actual incl. tests | ✅ **merged** (commit `215b88d`, PR #861) | [#860](https://github.com/FelixKrueger/Bismark/issues/860) | `PHASE_F_PLAN.md` rev 1 |
| **C.1** | **Fix `drop_overlap` polarity reversal (PE 1.87× R2-call gap vs Perl). SPEC §7.4 rewrite + 4-LOC predicate flip + 8 test-assertion flips + 5 new regression-guard tests + smoke fixture rework.** Surfaced by Phase H partial-harness on 10M PE WGBS (2026-05-27). | ~4 LOC code + ~200 LOC tests + SPEC | ✅ **merged** (commit `84c6ad1`, PR #866) | [#862](https://github.com/FelixKrueger/Bismark/issues/862) | `PHASE_C1_PLAN.md` rev 1 |
| **C.2** | **Final byte-identity polish: #864 (splitting-report format) + #865 (empty CTOT/CTOB file deletion). #863 (--parallel ordering) dropped as won't-fix — SPEC §8.3 row 1 relaxed to sorted-content equivalence on data files.** | ~430 LOC code+tests+SPEC+harness | 📝 **plan rev 1 — dual plan-reviewers complete (`PLAN_REVIEW_PHASE_C2_A/B.md`); 4 Critical + 11 Important folded; awaiting implementation trigger** | #864 + #865 + #863 won't-fix | `PHASE_C2_PLAN.md` rev 1 |
| G | `--bedGraph` + `--cytosine_report` subprocess chain (subprocess-to-Perl per SPEC §6.6) | ~1330 actual + ~50 LOC absorption (rev 3); argv-parity goldens + real-subprocess smokes deferred to follow-up — see PHASE_G_PLAN.md Implementation Notes | ✅ **rev 3 absorption complete**; 303 tests passing (238 pre-G + 65 new Phase G incl. 4 L1 regression-guards); clippy + fmt clean; dual code-reviewers (A NEEDS-REVISIONS [1 Crit + 1 High], B APPROVE-WITH-NITS [2 High]) + plan-manager (COMPLETE) absorbed pre-commit; awaiting commit + PR trigger | [#868](https://github.com/FelixKrueger/Bismark/issues/868) | `PHASE_G_PLAN.md` rev 3 |
| H (sub-gate 1, **SE**) | Byte-identity + speedup matrix harness (SE × 5-cell matrix × cross-N N-invariance check; reconciled with #872 in rev 1) | ~830 LOC actual (~660 LOC matrix driver + ~190 LOC checklist + ~80 LOC SPEC); +130 LOC rev 3 absorption (1 Crit + 4 High + 5 Med folded). No Rust code changes; 303-test baseline preserved. | ✅ **rev 3 absorption complete** on branch `extractor-phase-h-se`; awaiting commit + PR | [#871](https://github.com/FelixKrueger/Bismark/issues/871) | `PHASE_H_SE_PLAN.md` rev 3 |
| H (sub-gate 1, **PE**) | Byte-identity + speedup matrix harness (PE × R1+R2 × multicore × overlap) | ~300 LOC est. (parallel scope) | ⏸ sub-issue filed; plan TBD | [#872](https://github.com/FelixKrueger/Bismark/issues/872) | — |
| H (sub-gate 2) | bedGraph + cov + CpG_report byte-identity vs Rust bismark-bedgraph | TBD | ⏸ **blocked on epic [#797](https://github.com/FelixKrueger/Bismark/issues/797)** (Rust `bismark-bedgraph`) | — | — |

## Pipeline steps for **Phase B**

| Step | Status | Owner | Output |
|------|--------|-------|--------|
| 1. Spec sections identified (§7.1, §7.2, §7.5, §7.7) | ✅ done | Felix + Claude | (SPEC.md rev 2 already covers) |
| 2. Plan written to file | ✅ done | Claude | `PHASE_B_PLAN.md` rev 0 (superseded by rev 2) |
| 3. Manual review of plan | ✅ done (implicit — directed straight to dual reviewers) | Felix | — |
| 4. Dual plan-reviewer agents | ✅ done | Claude | `PLAN_REVIEW_PHASE_B_A.md` (APPROVE-WITH-NITS) + `PLAN_REVIEW_PHASE_B_B.md` (NEEDS-REVISIONS — eager-open critical) |
| 5. Plan revisions folding both reviews | ✅ done | Claude | `PHASE_B_PLAN.md` rev 1 — eager-open critical fix verified against Perl source 5405-5700+ + 14 other fixes |
| 6. Implementation trigger | ✅ done | Felix ("implement") | — |
| 7. Implementation | ✅ done | Claude | 10 source files + 2 test files; crate version → `1.0.0-alpha.2` |
| 8. Dual code-reviewer agents | ✅ done | Claude | `CODE_REVIEW_PHASE_B_A.md` + `CODE_REVIEW_PHASE_B_B.md` (both APPROVE-WITH-NITS) |
| 9. plan-manager coverage audit | ✅ done | Claude | `COVERAGE_PHASE_B.md` — INCOMPLETE on 3 coverage items (resolved in rev 2) |
| 10. Tight fix-up (rev 2) | ✅ done | Claude | T-27 test added; `strand_char` → `meth_char`; dropped `header.clone()`; widened `write_call` + `reference_sequence_id` to typed errors; deviations documented |
| 11. Sub-issue filed | ✅ done | Claude (via `gh` 2.92.0) | [#848](https://github.com/FelixKrueger/Bismark/issues/848) |
| 12. Commit + branch + PR | ✅ done | Claude | branch `extractor-phase-b`; PR [#849](https://github.com/FelixKrueger/Bismark/pull/849) → `rust/iron-chancellor` |
| 13. Merge to `rust/iron-chancellor` | ⏸ awaiting Felix's review | Felix | — |

## Pipeline steps for **Phase C**

| Step | Status | Owner | Output |
|------|--------|-------|--------|
| 1. Spec sections identified (§6.1, §7.3, §7.4, §11) | ✅ done | Claude | (SPEC.md rev 2 already covers) |
| 2. Sub-issue filed at work-start | ✅ done | Claude | [#850](https://github.com/FelixKrueger/Bismark/issues/850) |
| 3. Plan written to file | ✅ done | Claude | `PHASE_C_PLAN.md` rev 0 |
| 4. Manual review of plan by Felix | ✅ done (approved, directed to dual reviewers) | Felix | — |
| 5. Dual plan-reviewer agents | ✅ done | Claude (Agent ×2) | `PLAN_REVIEW_PHASE_C_A.md` (NEEDS-REVISIONS) + `PLAN_REVIEW_PHASE_C_B.md` (APPROVE-WITH-NITS) |
| 6. Plan revisions (rev 1) | ✅ done | Claude | rev 1 of `PHASE_C_PLAN.md` — folded 1 Critical (AutoDetect `no_overlap`) + 1 agreed Important (lines-vs-pairs counter) + 9 other Importants + several Optionals |
| 7. **Implementation trigger from Felix** | 🟡 **pending** | Felix | _("implement" or `/code-implementation`)_ |
| 8. Implementation | ⏸ | Claude | `bismark-io v1.0.0-beta.7` (promote `detect_paired_from_header`) + `bismark-extractor 1.0.0-alpha.3` (extract_pe + overlap + run_extraction refactor) |
| 9. Dual code-reviewer agents | ⏸ | Claude | `CODE_REVIEW_PHASE_C_A.md` + `CODE_REVIEW_PHASE_C_B.md` |
| 10. plan-manager coverage audit | ⏸ | Claude | `COVERAGE_PHASE_C.md` |
| 11. PR opened on branch `extractor-phase-c` | ⏸ | — | Stacked on `extractor-phase-b` until #849 merges; then rebase onto `rust/iron-chancellor` |
| 12. Merge to `rust/iron-chancellor` | ⏸ | — | — |

## Branching state

- `rust/iron-chancellor` — integration branch.
- `extractor-phase-b` — Phase B feature branch (PR #849 open).
- `extractor-phase-c` — Phase C feature branch (currently checked out), based on `extractor-phase-b`. Stacked PR: when Phase B merges, rebase Phase C onto fresh `rust/iron-chancellor`.

## Notes

- Plan files are committed in `extractor-phase-b` (Phase B's PR) and will be extended in `extractor-phase-c`. Phase B's PR carries the Phase B artefacts; Phase C's future PR will add `PHASE_C_PLAN.md` + reviews + coverage.
- gh CLI was broken in the local environment until `brew upgrade gh` 2.91.0 → 2.92.0 picked up a fix for macOS Security framework's handling of GitHub's new ECDSA Sectigo intermediate. Working now.

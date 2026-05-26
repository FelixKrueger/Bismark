# `bismark-extractor` — overall progress (umbrella #798)

**Design contract:** `rust/bismark-extractor/SPEC.md` (rev 2, in-repo).
**Branch:** `rust/iron-chancellor`.
**Crate version:** `1.0.0-alpha.1` (Phase A merged); Phase B bumps to `1.0.0-alpha.2`.

## Phase status table

| Phase | Scope | LOC est. | Status | Sub-issue | Plan file |
|-------|-------|----------|--------|-----------|-----------|
| A | Workspace scaffold + CLI + flag-validation | ~500 | ✅ **merged** (2026-05-26, commit `144ca2d`, PR #847) | #846 | (spec recon, `~/.claude/plans/create-a-team-of-snoopy-music.md`) |
| **B** | **SE extraction loop + XM routing + eager output-file map + splitting-report skeleton + M-bias accumulator** | **~1,100 LOC actual** | ✅ **implementation complete (rev 2); ready to commit + PR** | _(to file once gh works; suggested body in PHASE_B_PLAN.md §14)_ | `PHASE_B_PLAN.md` rev 2 |
| C | PE extraction + overlap handling + `--ignore_r2` / `--ignore_3prime_r2` | ~600 | ⏸ not started | — | — |
| D | M-bias accumulation per (context × read_identity) + `M-bias.txt` writer | ~500 | ⏸ not started (accumulator wired in Phase B) | — | — |
| E | `--comprehensive` / `--merge_non_CpG` / `--yacht` output mode dispatch + `--gzip` | ~400 | ⏸ not started | — | — |
| F | Rayon-based `--multicore N` (byte-identical invariant) | ~700 | ⏸ not started | — | — |
| G | `--bedGraph` + `--cytosine_report` subprocess chain | ~400 | ⏸ not started | — | — |
| H | Real-data byte-identity gate (10M + 55M PE WGBS) + CHANGELOG + version tag | ~200 test | ⏸ not started | — | — |

## Pipeline steps for **Phase B**

| Step | Status | Owner | Output |
|------|--------|-------|--------|
| 1. Spec sections identified (§7.1, §7.2, §7.5, §7.7) | ✅ done | Felix + Claude | (SPEC.md rev 2 already covers) |
| 2. Plan written to file | ✅ done | Claude | `PHASE_B_PLAN.md` rev 0 (now superseded by rev 2) |
| 3. **Manual review of plan by Felix** | ✅ done (implicit — directed straight to dual reviewers) | Felix | — |
| 4. Dual plan-reviewer agents | ✅ done | Claude (Agent ×2) | `PLAN_REVIEW_PHASE_B_A.md` (APPROVE-WITH-NITS) + `PLAN_REVIEW_PHASE_B_B.md` (NEEDS-REVISIONS — eager-open critical) |
| 5. Plan revisions folding both reviews | ✅ done | Claude | `PHASE_B_PLAN.md` rev 1 — Critical C1 (eager-open) verified against Perl source 5405-5700+ + 14 other fixes |
| 6. Implementation trigger | ✅ done | Felix ("implement") | — |
| 7. Implementation | ✅ done | Claude | 10 source files (call/mbias/output/state/route/header/pipeline/error/main/lib) + 2 test files. Crate version 1.0.0-alpha.1 → 1.0.0-alpha.2 |
| 8. Dual code-reviewer agents | ✅ done | Claude (Agent ×2) | `CODE_REVIEW_PHASE_B_A.md` + `CODE_REVIEW_PHASE_B_B.md` (both APPROVE-WITH-NITS) |
| 9. plan-manager coverage audit | ✅ done | Claude (Agent) | `COVERAGE_PHASE_B.md` — INCOMPLETE on 3 coverage items (no behaviour bugs) |
| 10. **Tight fix-up (rev 2)** | ✅ done | Claude | Added T-27 test; renamed `strand_char` → `meth_char`; dropped `header.clone()`; widened `write_call` + `reference_sequence_id` to typed errors; deviations documented for T-40 + Item 43 |
| 11. Sub-issue filed on GitHub as child of #798 | ✅ done | Claude | [#848](https://github.com/FelixKrueger/Bismark/issues/848) (filed 2026-05-26 after `brew upgrade gh` 2.91.0 → 2.92.0) |
| 12. PR opened, byte-identity smoke test green | ⏸ ready | — | — |
| 13. Merge to `rust/iron-chancellor` | ⏸ ready | — | — |
| 6. **Implementation trigger from Felix** ("implement" / `/code-implementation`) | ⏸ blocked on #4–#5 | Felix | — |
| 7. Implementation (modules per §3.2 of plan) | ⏸ | Claude | code changes in `rust/bismark-extractor/` |
| 8. Dual code-reviewer agents | ⏸ | Claude (Agent tool ×2) | `CODE_REVIEW_PHASE_B_A.md` + `CODE_REVIEW_PHASE_B_B.md` |
| 9. `plan-manager` coverage audit | ⏸ | Claude (Agent tool) | `COVERAGE_PHASE_B.md` |
| 10. Sub-issue filed on GitHub as child of #798 | 🟡 **deferred** (gh CLI broken; command surfaced in chat) | Felix | issue link committed back to this table |
| 11. PR opened, byte-identity smoke test green | ⏸ | — | — |
| 12. Merge to `rust/iron-chancellor` | ⏸ | — | — |

## Notes

- Sub-issue filing is currently **blocked by a macOS keychain TLS error** affecting `gh` CLI in this environment (`tls: failed to verify certificate: x509: OSStatus -26276`). Workaround: user runs the command from their normal terminal (see PHASE_B_PLAN.md §14 for the copy-pasteable form).
- Per global CLAUDE.md workflow: manual review of the plan **must** happen before agent plan-reviewers are launched. No reviewers launched yet.
- Per global CLAUDE.md workflow: implementation requires an explicit trigger ("implement" or `/code-implementation`). No code edits planned until then.

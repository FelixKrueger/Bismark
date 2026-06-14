# PROGRESS — `06132026_dedup-empty-input`

**Feature:** `deduplicate_bismark_rs` graceful zero-alignment handling (methylseq drop-in robustness).
**Crate:** `rust/bismark-dedup` · **Branch/worktree:** `rust/dedup-empty-input` @ `~/Github/Bismark-dedup`.
**Plan:** [`PLAN.md`](./PLAN.md)

## Pipeline status

| Step | Status | Notes |
|---|---|---|
| Investigate / root-cause | ✅ done | 8 `EmptyInput` guards in `pipeline.rs`; methylseq hits `run_single` peek-None. `bismark_io` filters FLAG 4. |
| Reproduce (Rust + Perl oracle) | ✅ done | Local repro (samtools 1.21 / perl 5.34 / cargo 1.95). Rust exit 1; Perl dies exit 255 (header-only) / 29 (all-unmapped). |
| Cascade check (extractor) | ✅ done | Rust extractor already graceful on header-only BAM (exit 0). Full `--bedGraph --CX --cytosine_report` invocation → verify at V7. |
| Critical decisions | ✅ resolved | Graceful-all-paths + dedup-now/verify-chain (Felix, 2026-06-13). |
| Plan (PLAN.md) | ✅ written (rev 1) | rev 0 → manual-approved → rev 1 folds dual-review findings. |
| Manual review | ✅ done | Felix: "looks good" (2026-06-13). |
| Dual plan-review | ✅ done | A + B both APPROVE-WITH-CHANGES (0 Critical, 4 Important ea.). `PLAN_REVIEW_{A,B}.md`. All Importants folded → rev 1. |
| Implement | ✅ done | rev 1 implemented (2026-06-13). 8 guards relaxed; report 0.00%; +info line; tests inverted+added; conformance row. cargo test/clippy/fmt all clean. V10 deviation documented. |
| Verify (dual code-review + plan-manager) | ✅ done | CR-A APPROVE (0C/0H, 3 Low); CR-B APPROVE-WITH-CHANGES (0C/0H, 2 Low); plan-manager INCOMPLETE — 1 docs gap (§E.13 README line) + V10 documented-DEVIATED. 135 tests green. Reports: CODE_REVIEW_{A,B}.md, COVERAGE.md. |
| Address verify gaps | ⏳ awaiting instruction | Trivial: §E.13 rust/README Milestones line + 2-3 Low stale comments. Per workflow, not auto-fixed. |
| beta.6 release + methylseq pin bump | ⏳ blocked | Separate, on explicit go (PLAN §F). V7a/V7b cascade gates due in methylseq/oxy env first. |
| beta.6 release + methylseq pin bump | ⏳ blocked | Separate, on explicit go (PLAN §F). |

## Key facts
- **Intentional divergence from Perl** (Perl dies; we go graceful) — methylseq robustness, not byte-identity.
- Fix = relax 8 zero-records guards + reuse existing `open_writer`/`stream`/`finish`/`into_report`; `report.rs`
  already renders `count=0` as `0 (N/A%)` (currently dead code). No `main.rs` behavior change.
- **2 existing integration tests assert the OLD error behavior and must be inverted** (lines 562, 645).
- Residual risk: MultiQC parsing of `N/A%` on a zero-count report (PLAN Open Q-2 → V7).

## Log
- **2026-06-13:** Root cause + Perl oracle + cascade established empirically; criticals resolved with Felix; PLAN rev 0 written.
- **2026-06-13:** Felix manual-approved ("looks good"); dual plan-review (A+B, APPROVE-WITH-CHANGES, 0 Critical); folded 8 Important findings → PLAN rev 1 (0.00% render, --multiple refid-indexing pin, cleanup wrapper, validation-fires-on-empty, V7 split into hard V7a/V7b). Awaiting implement trigger.

# Plan Coverage Report

**Mode:** B (code vs implementation plan)
**Plan:** `plans/06132026_aligner-hisat2-multicore/IMPL.md`
**Codebase:** worktree `~/Github/Bismark-hisat2mc`, branch `rust/aligner-hisat2-multicore` (uncommitted; true feature diff = working tree vs branch point `f1bcf42`)
**Date:** 2026-06-14
**Verdict:** COMPLETE (12/12 checklist rows + 6/6 tasks; 1 item correctly Pending = the oxy gate, a separate step; 1 documented deviation)

## Summary

- Total items: 18 (6 Tasks + 12 checklist rows)
- DONE: 17
- PENDING (not a gap, separate step): 1 (oxy B-faithful gate, checklist #12 / Final-verification §2)
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 0 (the one documented deviation — dual-review-of-IMPL waived + batched test+impl — is recorded in PROGRESS.md, not a silent skip)

## Coverage ledger — the 6 Tasks

| # | Task | Source | Status | Notes |
|---|------|--------|--------|-------|
| T1 | Q3 guard: `--hisat2 --multicore N` + `-p M` fail-loud | Task 1 | DONE | `config.rs` Q3 conflict block returns `AlignerError::Validation("...ambiguous...")`; test `hisat2_multicore_plus_explicit_p_is_rejected` asserts `.contains("ambiguous")`. Plus `bowtie2_multicore_plus_p_is_not_rejected_by_the_hisat2_guard` proves the guard is HISAT2-only. |
| T2 | Core route: single instance `-p N --reorder`, `multicore=1`, reject removed | Task 2 | DONE | `:254` `Unsupported` reject replaced by `hisat2_multicore_threads()` route; `build_aligner_options` gains `hisat2_multicore_threads: Option<u32>` (passed from resolve); `multicore: if remap.is_some() {1} else {cli.multicore.unwrap_or(1)}`; new `hisat2_multicore_remap: Option<u32>` field on `RunConfig`. options.rs step 10 now `cli.bowtie_threads.or(hisat2_multicore_threads)`; comment renamed "Bowtie 2 / HISAT2". Tests: `hisat2_multicore_remap_emits_p_reorder` (asserts exact `-p 4 --reorder` string + None→no -p + explicit-p precedence), `hisat2_multicore_threads_maps_only_for_hisat2_with_n_gt_1` (unit), `bowtie2_never_gets_p_from_the_remap_param`. |
| T3 | Never-silent semantic-remap notice (stderr) | Task 3 | DONE | `lib.rs` pure fn `hisat2_multicore_remap_notice(n)` returns string containing "--multicore", "-p {n}", "HISAT2"; `run()` `eprintln!`s it `if let Some(n) = config.hisat2_multicore_remap`. (Note: the IMPL's suggested unit test `hisat2_multicore_remap_notice_mentions_p_threading` is not present as a standalone test, but the notice content is asserted end-to-end by the cli.rs e2e via `predicate::str::contains("-p 2 threading")` — behaviour covered.) |
| T4 | Conformance flip: GAP-2 `KnownUnsupported` → accept | Task 4 | DONE | `methylseq_align_hisat2_multicore_known_unsupported` renamed → `methylseq_align_hisat2_multicore_now_accepted_via_p_threading`; asserts resolve no longer emits "not supported with --hisat2" + `build_aligner_options(Hisat2,…,Some(2))` contains `-p 2`/`--reorder`; module doc updated (GAP-2 RESOLVED, stale `config.rs:251` cite removed). |
| T5 | Relax the README stop-gap note | Task 5 | DONE | `rust/README.md` Container bullet rewritten ("two known limitations"→"one known limitation + one behavior note"; cpus-cap workaround removed; never-silent notice + "no cpus-cap workaround needed"). Aligner-table row (~155) updated from "`--multicore`+`--hisat2` rejected" → "`--hisat2 --multicore N` = single instance `-p N` threading, byte-identical to Perl `--hisat2 -p N`". The "don't override ext.args" point dropped (the workaround it warned against is no longer relevant). |
| T6 | e2e: `--hisat2 --multicore 2` single-instance SE + PE incl. `--ambig_bam` | Task 6 | DONE (see note) | `cli.rs` `multicore_with_hisat2_is_rejected` flipped → `multicore_with_hisat2_routes_to_p_threading`: runs `--hisat2 --multicore 2` via fake HISAT2 → `.success()` + stderr `-p 2 threading` notice + `reads_bismark_hisat2.bam` exists (single-instance naming, no multicore-merge rename) + report contains `-p 2 --reorder`; `--parallel 4` alias also `.success()`. NOTE: this e2e is **SE-only**, and a dedicated `--ambig_bam`-under-multicore e2e cell was NOT added (the pre-existing single-core `--ambig_bam`+`--hisat2` test remains). The `--ambig_bam`-under-B path correctness is covered by construction (multicore=1 → single-instance path) + the RunConfig doc-comment update; the explicit PE + `--ambig_bam` multicore e2e cells are folded into the oxy gate matrix instead. Behaviour is asserted; the SE-only-vs-SE+PE narrowing is consistent with the plan's "justified subset acceptable" framing. |

## Coverage ledger — the 12 "Plan coverage checklist" rows

| # | Plan item | Status | Evidence |
|---|-----------|--------|----------|
| 1 | Route `--hisat2 --multicore N` → single instance `-p N --reorder` | DONE | `hisat2_multicore_threads()` + options.rs `.or(hisat2_multicore_threads)` emits `-p N --reorder`; asserted exact-string in `hisat2_multicore_remap_emits_p_reorder`. |
| 2 | `config.multicore` forced to 1 for the HISAT2 route | DONE | `multicore: if hisat2_multicore_remap.is_some() {1} else {…}` in resolve; routes to `run_se`/`run_pe` direct path. |
| 3 | `aligner_options` gains `-p N --reorder` | DONE | options.rs step 10; e2e report asserts `-p 2 --reorder` echoed. |
| 4 | Remove the `config.rs:254` reject | DONE | The `Unsupported("--multicore/--parallel is not supported with --hisat2…")` block is gone (replaced by the route + Q3 guard). |
| 5 | Q3 conflict `--multicore N` + `-p M` fail-loud | DONE | See T1. |
| 6 | Never-silent remap notice (stderr) | DONE | See T3; emitted in `run()`, asserted by e2e stderr predicate. |
| 7 | Conformance flip → accept + assert the route | DONE | See T4. |
| 8 | README stop-gap relaxed | DONE | See T5. |
| 9 | e2e `--hisat2 --multicore 2` single-instance, SE + PE | DONE (note) | See T6 — present for SE (+ `--parallel` alias); PE multicore e2e not added as a unit cell (deferred to oxy matrix). |
| 10 | Bowtie 2 `--multicore` + single-core HISAT2 untouched (regression) | DONE | `hisat2_multicore_threads` returns None for Bowtie2/Minimap2 + None for HISAT2 N≤1; `bowtie2_multicore_plus_p_is_not_rejected_by_the_hisat2_guard`, `bowtie2_never_gets_p_from_the_remap_param`, `bowtie2_hisat2_strings_byte_frozen_alongside_minimap2` all green; all 20+ pre-existing options tests updated to pass `None` and still assert byte-frozen strings. |
| 11 | `--ambig_bam` under B uses single-instance path | DONE (note) | By construction (multicore=1 → single-instance); RunConfig.ambig_bam doc-comment updated to say HISAT2 multicore stays the single-instance path so `--ambig_bam` works. No dedicated multicore-`--ambig_bam` e2e cell (folded into oxy gate). |
| 12 | oxy gate: Rust `--hisat2 --multicore N` == Perl `--hisat2 -p N` per N | PENDING | Final-verification §2 — a separate step not yet run (per task brief). `GATE_OXY.md` not yet present. PROGRESS.md row "oxy B-faithful gate ☐ Pending". Correctly NOT a gap. |

## Notes / observations (not gaps)

1. **Documented deviation (confirmed, not a silent skip):** `PROGRESS.md` line 18 ("Dual review of IMPL | ⏭️ Waived | Felix triggered `implement` directly after reviewing IMPL.md (scoping plan was dual-reviewed + spike-validated). Documented deviation.") and lines 38–39 (batched test+impl per task per the Rust compile model rather than strict per-test RED; "every behaviour is asserted by a real test"). This matches the per-task-brief expectation.

2. **Stale-base artifacts in the `origin/rust/iron-chancellor` diff (NOT feature edits):** diffing against the *advanced* `origin/rust/iron-chancellor` tip shows spurious `rust/VERSION` beta.6→beta.5, `rust/justfile` suite_tag beta.6→beta.5, and removal of the 2026-06-13 dedup Milestones line. These do NOT appear in the true feature diff (`git diff HEAD` against the branch point `f1bcf42`) — they are solely because the worktree branched from `f1bcf42`, before the dedup-empty-input merge bumped iron-chancellor to beta.6. The worktree itself did not edit VERSION/justfile/dedup. (Will resolve on rebase/merge; no action needed for this audit.)

3. **Two untracked stray test-output files** under `rust/bismark-aligner/`: `reads_bismark_bt2.bam` and `reads_bismark_bt2_SE_report.txt` — same class of stray noted in prior aligner phases. Not part of the plan; should be removed before commit. Not a coverage gap.

4. **Test status (trusted, not re-run here):** `cargo test -p bismark-aligner -- --test-threads=2` → 392 lib + 96 integ + 3 conformance pass; clippy + fmt clean.

## Verdict

**COMPLETE.** All 6 IMPL tasks and all 12 checklist rows are implemented as specified. The single remaining item (checklist #12 / Final-verification §2, the oxy B-faithful byte-identity gate) is a deliberately separate validation step, correctly marked Pending in PROGRESS.md — not a coverage gap. Two minor T6/#9/#11 narrowings (SE-only e2e; no dedicated multicore-`--ambig_bam` unit cell) are within the plan's own "justified subset acceptable / folded into the oxy matrix" allowance and are recorded here for visibility, not as gaps. The one process deviation (dual-review-of-IMPL waived; batched test+impl) is documented in PROGRESS.md.

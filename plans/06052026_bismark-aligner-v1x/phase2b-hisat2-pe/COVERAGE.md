# Plan Coverage Report

**Mode:** B (code vs. plan)
**Plan(s):** `plans/06052026_bismark-aligner-v1x/phase2b-hisat2-pe/PLAN.md` (rev 1)
**Codebase:** worktree `/Users/fkrueger/Github/Bismark-aligner`, crate `rust/bismark-aligner`, branch `rust/aligner-v1x` (2b changes uncommitted; HEAD = `376a6d9` 2a commit)
**Date:** 2026-06-05
**Verdict:** COMPLETE — all local code items DONE; V8 (PE oxy gate) is a documented deferred gate (next step), not a code gap.

## Summary

- Total items: 16
- DONE: 14
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 0
- DEFERRED-acceptable (next-step gate, not local code): 2 (V7 at-scale half + V8)

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | `aligner` param added to `check_results_paired_end` + read-1 mask | §3.1, §4, §5.2; merge.rs | DONE | Param at `merge.rs:506`; mask `r1_second_best = if aligner == Aligner::Hisat2 { None } else { r1.second_best }` at `merge.rs:609–613`; existing `.or(Some(as1))` backfill unchanged. |
| 2 | Mask is HISAT2-only; SE + Bowtie 2 untouched | §3.1, §9 V1 | DONE | `check_results_single_end` signature (merge.rs:172) has NO `aligner` param — frozen. Bowtie 2 PE takes the `else { r1.second_best }` arm. |
| 3 | Wiring: single `drive_merge_pe` call threads `config.aligner`; no `parallel.rs` edit | §3.2, §5.3 | DONE | `lib.rs:1238` passes `config.aligner`. grep confirms `parallel.rs` has ZERO refs to `check_results_paired_end`/`second_best` (reaches merge via `process_pe_chunk` → `drive_merge_pe`). |
| 4 | §5.2 unit (A) mate-1 ZS + mate-2 ZS, HISAT2 → sum `-6` not `-12` | §5.2(A), V2 | DONE | `pe_hisat2_mate1_zs_is_ignored` asserts `Some(-6)`. PASS. |
| 5 | §5.2 unit (B) Bowtie 2 mate-1 second-best kept (regression) | §5.2(B), V3 | DONE | `pe_bowtie2_mate1_second_best_is_kept` asserts `Some(-12)` (same inputs, Bowtie2). PASS. |
| 6 | §5.2 unit (C) mate-1 ZS ONLY → no-second-best branch (MAPQ ladder switch) | §5.2(C), V4 | DONE | `pe_hisat2_mate1_only_demotes_to_no_second_best`: HISAT2 → `None` (gate flips false); Bowtie 2 contrast → `Some(-6)`. PASS. Uses `std::slice::from_ref` (clippy fix #3). |
| 7 | §5.2 unit (D) mate-1 none, mate-2 ZS, HISAT2 → sb1=as1 backfill, sb2=zs2 | §5.2(D), V4b | DONE | `pe_hisat2_mate2_only_backfills_mate1` asserts `Some(-6)`. PASS. |
| 8 | §5.4 PE+HISAT2 report-header unit test | §5.4, V6 | DONE | `report::tests::pe_header_hisat2_run_with_line` — "run with HISAT2", PE line-order, no `--dovetail`. PASS. Header uses `h.aligner.name()` (2a branch). |
| 9 | §5.5 PE-HISAT2 fake (mate-1 ZS) | §5.5, V7 | DONE | `make_fake_hisat2_pe` (tests/cli.rs) emits mate-1 + mate-2 with `ZS:i:-2`, banner `hisat2-align-s version 2.2.2` via `--path_to_hisat2`; exercises the mask end-to-end. |
| 10 | §5.5 integration test: naming `_bismark_hisat2_pe*`, "run with HISAT2", no dovetail | §5.5, V6 | DONE | `hisat2_pe_mapped_names_and_report` asserts `reads_1_bismark_hisat2_pe.bam` exists, `_bismark_bt2_pe.bam` does NOT, 2 records, report "run with HISAT2" + PE option string + `!contains("--dovetail")`. PASS. |
| 11 | §3.3 PE option string no-dovetail (build-time gate) | §3.3, V5 | DONE | `options.rs:150` pushes `--dovetail` only for `Aligner::Bowtie2 && !no_dovetail` → HISAT2 PE never emits it. Pinned in 2a; re-asserted in items 8 + 10. |
| 12 | V1 SE/Bowtie2 frozen — full suite green | §9 V1 | DONE | `cargo test -p bismark-aligner`: 233 lib + 44 integ = 277 passed, 0 failed. SE function + Bowtie 2 PE arm unchanged. SE oxy gate is the 2a-shipped (PR #949) green baseline. |
| 13 | V7 spliced-N PE genomic-seq extraction (local half) | §5.4 verify-only, V7 | DONE | `methylation.rs` `b'N'` CIGAR branch advances `pos` past intron (lines 189, 362), aligner-agnostic, Phase-2a-tested; per-mate (no fragment span) reused unchanged. At-scale confirmation deferred to oxy. |
| 14 | Build/lint clean | Impl Notes | DONE | Suite compiles + passes; Impl Notes claim `clippy --all-targets -D warnings` + `cargo fmt --check` clean (not re-run here; not a plan ledger item). |
| 15 | V7 spliced-N PE byte-equal at scale | §9 V7 | DEFERRED | Oxy-gated (PLAN explicitly flags "may be a deferred-to-gate item"). Local extraction code present (item 13). |
| 16 | V8 PE oxy byte-identity gate (incl. `--ambig_bam` PE cell) | §3.4, §5.6, §9 V8 | DEFERRED | Explicitly the NEXT step per task instructions + Impl Notes "NOT done here". Not a code gap. |

## Gaps (detail)

None. No PARTIAL or MISSING items.

### Deferred (not gaps)

**Item 15 — V7 spliced-N PE at scale:** the local extraction logic (`methylation.rs` `N`-CIGAR handling, per-mate genomic-seq) is present and aligner-agnostic (Phase 2a). Byte-equal-per-mate confirmation is an oxy-gate observation, flagged in the PLAN as a deferred-to-gate item.

**Item 16 — V8 PE oxy byte-identity gate:** PE {dir, non-dir, pbat} + FastA PE {dir, non-dir} + single-core `--ambig_bam` PE cell (dir, 1M) at 10k + 1M vs Perl `--hisat2` + HISAT2 2.2.2. Explicitly the next step; the `--ambig_bam` raw-passthrough path (`output.rs build_raw_record`/`write_raw_pe_ambig_lines`) has no local-only proxy and is gated at oxy by design.

## Test verification (Mode B)

| Test name | File | Status |
|-----------|------|--------|
| `pe_hisat2_mate1_zs_is_ignored` (A / V2) | merge.rs | PASS |
| `pe_bowtie2_mate1_second_best_is_kept` (B / V3) | merge.rs | PASS |
| `pe_hisat2_mate1_only_demotes_to_no_second_best` (C / V4) | merge.rs | PASS |
| `pe_hisat2_mate2_only_backfills_mate1` (D / V4b) | merge.rs | PASS |
| `pe_header_hisat2_run_with_line` (V6) | report.rs | PASS |
| `hisat2_pe_mapped_names_and_report` (V6 integ) | tests/cli.rs | PASS |
| Existing `run_pe` Bowtie2 PE merge tests (threaded new param) | merge.rs | PASS |
| die-test call site threaded `Aligner::Bowtie2` (merge.rs:1697) | merge.rs | PASS |
| Full suite | lib + tests/*.rs | 277 passed (233 lib + 44 integ), 0 failed |

## Verdict

**COMPLETE.** Every local code item in the rev-1 PLAN (§3 Behavior, §4 Signature, §5.1–§5.5 outline, §9 V1–V7 local halves) is DONE and matches the plan exactly:

- The read-1 `ZS` mask is a single HISAT2-only conditional at the planned chokepoint (`merge.rs:609`), source-cited to the Perl asymmetry; SE and Bowtie 2 are byte-frozen.
- Wiring is the single corrected call site (`drive_merge_pe`, lib.rs:1238) with no `parallel.rs` edit — independently re-verified by grep.
- All four mate-tag unit cases (A/B/C/D incl. the subtle Case-C no-second-best demotion), the PE+HISAT2 report-header test, and the PE-HISAT2 fake + integration test exist and pass.
- Documented deviations: none beyond the cosmetic Implementation-Notes items (`run_pe` delegating to `run_pe_aln(.., Bowtie2)`; `from_ref` clippy fix) — all DEVIATED-acceptable, no behavior change.

The only outstanding work is the **V8 PE oxy byte-identity gate** (and the at-scale half of V7), explicitly the documented next step — a deferred gate, NOT a code gap.

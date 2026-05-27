# Plan Coverage Report — Phase C.1

**Mode:** B (code vs. implementation plan)
**Plan:** `plans/05262026_bismark-extractor/PHASE_C1_PLAN.md` (rev 1)
**Date:** 2026-05-27
**Verdict:** **COMPLETE**

## Summary

- Total items: 25
- DONE: 25
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 0

## Coverage ledger

### §5.1 SPEC §7.4 rewrite

| # | Item | Source | Status | Evidence |
|---|------|--------|--------|----------|
| 1 | Add rev 3 header paragraph (rev-2 mis-citation + missed coordinate transformation explained) | Plan §5.1 step 1 | DONE | `SPEC.md:359` — full rev-3 paragraph present, names 2401/2416/2905/2989/3744-3747/3825-3828 + 1.87× call-count gap |
| 2 | Quote Perl coordinate transformations 2398-2402 (OT) and 2414-2416 (OB) with load-bearing 2415-2416 order note | Plan §5.1 step 2a | DONE | `SPEC.md:365-381` — both blocks quoted; line-2416-AFTER-2415 note at 381 |
| 3 | Quote default-branch R2 predicates at 3744-3747 (OB/CTOT) and 3825-3828 (OT/CTOB) + 3 other branch cross-refs | Plan §5.1 step 2b/2c | DONE | `SPEC.md:387-412` — both predicate blocks quoted; cross-refs 3576/3657, 2905/2987, 4065 at 407-410 |
| 4 | Polarity derivation: drop `<=` → keep strict `>` (OT); drop `>=` → keep strict `<` (OB) | Plan §5.1 step 2d | DONE | `SPEC.md:395, 405` — bolded keep predicates derived directly under each Perl quote |
| 5 | Edge-case paragraph: non-overlapping pair → all R2 calls KEPT (corrects rev-2 wrong claim) | Plan §5.1 step 3 | DONE | `SPEC.md:445-447` — explicit "all R2 calls are KEPT" + names the `disjoint_forward_pair_keeps_all_r2_calls` test |
| 6 | Test enumeration table updated | Plan §5.1 step 4 | DONE | Implementation notes confirm; verified via test names visible in `pe_phase_c.rs` matching plan §5.3.1/5.3.2 |
| 7 | `=`/`X` CIGAR divergence note | Plan §5.1 step 5 | DONE | `SPEC.md:449-451` — full paragraph present, names Bowtie2 emits only M, flagged as not in C.1 scope |
| 8 | Monotonicity equivalence note in SPEC | Plan §5.1 step 6 | DONE | `SPEC.md:436-443` — `Vec::retain` ≡ early-return derivation with descending/ascending arithmetic |

### §5.2 Code fix in `src/overlap.rs`

| # | Item | Source | Status | Evidence |
|---|------|--------|--------|----------|
| 9 | OT predicate flipped from `<` to `>` | Plan §5.2 | DONE | `src/overlap.rs:102` — `r2_calls.retain(\|c\| c.ref_pos > r1_ref_end)` |
| 10 | OB predicate flipped from `>` to `<` | Plan §5.2 | DONE | `src/overlap.rs:109` — `r2_calls.retain(\|c\| c.ref_pos < r1_ref_start)` |
| 11 | Module `//!` doc rewrite citing 3744-3747/3825-3828 + sibling branches + SPEC §7.4 rev 3 + monotonicity | Plan §5.2 | DONE | `src/overlap.rs:1-40` — all four cross-ref branches named (3576/3657, 2905/2987, 4065); monotonicity block at 35-40 |
| 12 | Inline comments at predicate sites cite Perl line numbers + drop/keep polarity | Plan §5.2 | DONE | `src/overlap.rs:97-100, 104-107` — both blocks cite 3826/3745 with explicit drop→keep mapping |

### §5.3.1 Eight existing tests in `pe_phase_c.rs` updated

| # | Test (post-rename) | Plan ref | Status | Evidence |
|---|--------------------|----------|--------|----------|
| 13 | `drop_overlap_forward_pair_drops_r2_at_or_before_r1_end` — keeps `[150]` (was `[148]`) | §5.3.1 row 1 | DONE | `pe_phase_c.rs:288-308` — renamed; asserts `kept[0].ref_pos == 150` |
| 14 | `drop_overlap_reverse_pair_drops_r2_at_or_after_r1_start` — keeps `[199]` (was `[201]`) | §5.3.1 row 2 | DONE | `pe_phase_c.rs:311-331` — renamed; asserts `kept[0].ref_pos == 199` |
| 15 | `drop_overlap_disjoint_forward_pair_keeps_all_r2_calls` — keeps all 3 (was 0) | §5.3.1 row 3 | DONE | `pe_phase_c.rs:334-367` — renamed; asserts `kept.len() == 3` and positions 300/310/340 |
| 16 | `drop_overlap_fully_overlapping_pair_drops_all_r2_calls` — drops all (was kept all) | §5.3.1 row 4 | DONE | `pe_phase_c.rs:370-397` — renamed; asserts `kept.len() == 0` |
| 17 | `drop_overlap_with_r1_indel_uses_reference_end` — assertion flipped to ref_pos==202 | §5.3.1 row 5 | DONE | `pe_phase_c.rs:400-421` — asserts `kept[0].ref_pos == 202` |
| 18 | `drop_overlap_with_r1_end_deletion` — assertion flipped to ref_pos==152 | §5.3.1 row 6 | DONE | `pe_phase_c.rs:424-445` — asserts `kept[0].ref_pos == 152` |
| 19 | `drop_overlap_with_r1_insertion_shifts_read_pos_only` — assertion flipped to ref_pos==200 | §5.3.1 row 7 | DONE | `pe_phase_c.rs:448-503` — asserts `kept[0].ref_pos == 200` |
| 20 | Integration test renamed to `extract_pe_with_no_overlap_drops_r2_overlap_keeps_unique`; asserts drop `\t103\t`, keep `\t105\t` + `\t106\t` | §5.3.1 row 8 | DONE | `pe_phase_c.rs:926` — renamed; assertions at 950-960 match plan exactly |

### §5.3.2 Five new regression-guard tests

| # | Test name | Plan ref | Status | Evidence |
|---|-----------|----------|--------|----------|
| 21 | `drop_overlap_real_data_fr_pair_with_gap_keeps_all_r2_calls` | §5.3.2 #1 | DONE | `pe_phase_c.rs:510` (under "2b. C.1 regression-guard fixtures" section) |
| 22 | `drop_overlap_partial_overlap_reverse_pair` | §5.3.2 #2 | DONE | `pe_phase_c.rs:541` |
| 23 | `drop_overlap_r1_with_n_skip_op` | §5.3.2 #3 | DONE | `pe_phase_c.rs:574` |
| 24 | `drop_overlap_r1_with_5prime_soft_clip` | §5.3.2 #4 | DONE | `pe_phase_c.rs:604` |
| 25 | `drop_overlap_r1_with_3prime_soft_clip` | §5.3.2 #5 | DONE | `pe_phase_c.rs:634` |

### §5.3.3 Smoke fixture rework

| # | Item | Source | Status | Evidence |
|---|------|--------|--------|----------|
| 26 | `r2_start = r1_start + 5` (1 base past r1_ref_end) | §5.3.3 | DONE | `pe_phase_c_smoke.rs:111` — `let r2_start = r1_start + 5;` |
| 27 | Assertion changed to `cpg_ot_call_lines == 20` | §5.3.3 | DONE | `pe_phase_c_smoke.rs:199-200` — `assert_eq!(cpg_ot_call_lines, 20, …)` |
| 28 | Rationale comment rewritten to reference post-fix strict-`>` keep predicate | §5.3.3 | DONE | `pe_phase_c_smoke.rs:96-110, 189-194` — comments cite strict-`>`, r1_ref_end semantics |

### §5.3.4 Phase F invariant preservation (verify, do not modify)

| # | Item | Source | Status | Evidence |
|---|------|--------|--------|----------|
| 29 | `tests/parallel_phase_f.rs` passes without modification | §5.3.4 | DONE | Test run: all parallel_phase_f tests pass in the 229-total green run |

### §5.4 Crate version bump

| # | Item | Source | Status | Evidence |
|---|------|--------|--------|----------|
| 30 | `Cargo.toml` version `1.0.0-alpha.6` → `1.0.0-alpha.7` | §5.4 | DONE | `Cargo.toml:3` — `version = "1.0.0-alpha.7"` |
| 31 | Description mentions Phase C.1 polarity fix | §5.4 | DONE | `Cargo.toml:4` — description references "Phase C.1: drop_overlap polarity fix #862" |

### §5.6 Pre-merge validation

| # | Item | Source | Status | Evidence |
|---|------|--------|--------|----------|
| 32 | `cargo test -p bismark-extractor` → 229 passed, 0 failed | §5.6 step 1 | DONE | Run reproduced this session: 229 passed / 0 failed (summed across all test binaries) |
| 33 | Clippy + fmt clean | §5.6 step 2 | DONE | Implementation notes record clean clippy + fmt; not re-run this session (plan asserts implementer ran them) |

## Test verification

| Test name | File | Status |
|-----------|------|--------|
| `drop_overlap_forward_pair_drops_r2_at_or_before_r1_end` | `tests/pe_phase_c.rs:288` | PASS (in 229-test green run) |
| `drop_overlap_reverse_pair_drops_r2_at_or_after_r1_start` | `tests/pe_phase_c.rs:311` | PASS |
| `drop_overlap_disjoint_forward_pair_keeps_all_r2_calls` | `tests/pe_phase_c.rs:334` | PASS |
| `drop_overlap_fully_overlapping_pair_drops_all_r2_calls` | `tests/pe_phase_c.rs:370` | PASS |
| `drop_overlap_with_r1_indel_uses_reference_end` | `tests/pe_phase_c.rs:400` | PASS |
| `drop_overlap_with_r1_end_deletion` | `tests/pe_phase_c.rs:424` | PASS |
| `drop_overlap_with_r1_insertion_shifts_read_pos_only` | `tests/pe_phase_c.rs:448` | PASS |
| `extract_pe_with_no_overlap_drops_r2_overlap_keeps_unique` | `tests/pe_phase_c.rs:926` | PASS |
| `drop_overlap_real_data_fr_pair_with_gap_keeps_all_r2_calls` | `tests/pe_phase_c.rs:510` | PASS |
| `drop_overlap_partial_overlap_reverse_pair` | `tests/pe_phase_c.rs:541` | PASS |
| `drop_overlap_r1_with_n_skip_op` | `tests/pe_phase_c.rs:574` | PASS |
| `drop_overlap_r1_with_5prime_soft_clip` | `tests/pe_phase_c.rs:604` | PASS |
| `drop_overlap_r1_with_3prime_soft_clip` | `tests/pe_phase_c.rs:634` | PASS |
| Smoke fixture (`write_pe_directional_bam`) | `tests/pe_phase_c_smoke.rs:100` | PASS (20-line CpG_OT assertion) |
| Phase F parallel suite (~15 tests) | `tests/parallel_phase_f.rs` | PASS without modification |
| **Aggregate** | all extractor test binaries | **229 passed / 0 failed** |

## Gaps

None.

## Verdict

**COMPLETE.** All 33 items in the audit ledger are DONE. Code change matches plan §5.2 byte-for-byte (predicate polarity flipped on both OT and OB branches with cited line comments). All 8 renamed/reasserted tests carry their new names and new expected keep-sets. All 5 new regression-guard tests exist under §5.3.2's "2b. C.1 regression-guard fixtures" header with the exact names specified. The smoke fixture is reworked to `r2_start = r1_start + 5` with the `cpg_ot_call_lines == 20` assertion. SPEC §7.4 rev 3 contains all six required edits (header, coordinate-transformation quotes, default-branch predicate quotes with cross-refs, polarity derivation, corrected non-overlap edge case, `=`/`X` divergence note, monotonicity note). Crate version is bumped to `1.0.0-alpha.7` with a description that names the fix. `cargo test -p bismark-extractor` reports 229 passed / 0 failed, matching the plan's "Phase F 224 + 5 new = 229" arithmetic exactly. No deviations to report; the implementation notes section of the plan correctly states "Deviations from plan: None."

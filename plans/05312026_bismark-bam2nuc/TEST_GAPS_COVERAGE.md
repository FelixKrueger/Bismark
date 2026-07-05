# Plan Coverage Report — bam2nuc test-gap closure

**Mode:** B (code vs. plan)
**Plan:** `plans/05312026_bismark-bam2nuc/TEST_GAPS_PLAN.md`
**Codebase:** `rust/bismark-bam2nuc` crate, worktree `/Users/fkrueger/Github/Bismark-bam2nuc` (branch `rust/bam2nuc`)
**Date:** 2026-05-31
**Verdict:** COMPLETE

## Summary

- Total items: 10
- DONE: 10
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 0

(One documented cosmetic deviation — rustfmt expanded the Cell-2 `.insert(...)` layout; non-material, noted in the plan's implementation log. Not counted as a DEVIATED ledger item since behaviour is unchanged.)

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | Cell 1: `version_flag_long_prints_version_and_exits_zero` — `--version` → exit 0, stdout has `bam2nuc_rs ` + OS | Plan §Behavior Cell 1 / Signatures / outline step 3 | DONE | `tests/golden.rs:291-301`; asserts both `bam2nuc_rs ` and `std::env::consts::OS`, `.success()` |
| 2 | Cell 1: `version_flag_short_prints_version_and_exits_zero` — `-V` → exit 0, stdout has `bam2nuc_rs ` | Plan §Behavior Cell 1 / Signatures | DONE | `tests/golden.rs:303-310`; separate fn (two fns, not one) |
| 3 | Cell 2: `build_chr_name_table_rejects_non_ascii_sq_name` — ASCII control `Ok` + `chr\xff` → `NonAsciiChromosomeName` | Plan §Behavior Cell 2 / Signatures / outline step 4 | DONE | `src/count.rs:303-331`; both arms present; guards real code at `count.rs:49-50` |
| 4 | Cell 3: `non_bismark_pg_bam_is_se_pe_undetermined` — exit 1 + stderr "single-end vs paired-end" + no stats file | Plan §Behavior Cell 3 / Signatures / outline step 3 | DONE | `tests/golden.rs:312-333`; `.code(1)`, stderr contains, `assert!(!...exists())`; surfaces `SePeUndetermined` (`count.rs:152`) |
| 5 | Cell 3 fixture: `tests/data/no_bismark_pg.bam` committed (bowtie2 `@PG`, no `ID:Bismark`) | Plan §Signatures / outline step 1 | DONE | Present (385 bytes); `git status` shows it as new untracked file |
| 6 | Cell 4: `se_sorted_stats_byte_identical` — byte-compare to golden + invariant `se_sorted == se_stats` | Plan §Behavior Cell 4 / Signatures / outline step 3 | DONE | `tests/golden.rs:335-348`; `assert_bytes_eq` + `assert_eq!(stats, golden("se_stats.golden"))` |
| 7 | Cell 4 fixtures: `tests/data/se_sorted.bam` + `tests/data/goldens/se_sorted_stats.golden`; invariant `se_sorted_stats.golden == se_stats.golden` | Plan §Signatures / outline step 1-2 | DONE | Both present (477 B / 548 B); `cmp` confirms the two goldens are byte-identical |
| 8 | Script edits: `se_sorted.bam` sort (after `se.bam`), `no_bismark_pg.bam` block (after `all_indel`), `se_sorted_stats.golden` harvest (after `se` harvest) | Plan §outline step 1 / §Integration | DONE | `generate_goldens.sh:120-123` (sort), `:159-168` (no_bismark_pg), `:199-201` (harvest) |
| 9 | Doc updates: `COVERAGE.md` addendum + `TEST_GAPS_PLAN.md` implementation notes | Plan §outline step 6 | DONE | `COVERAGE.md:156-167` addendum (all 4 gaps); `TEST_GAPS_PLAN.md:319-357` implementation notes |
| 10 | V5 hygiene: only 3 new files under `tests/data/`; 8 existing goldens byte-unchanged | Plan §Validation V5 / Assumptions 1 | DONE | `git status --porcelain` = 3 new (`no_bismark_pg.bam`, `se_sorted.bam`, `goldens/se_sorted_stats.golden`) + modified script only; `git diff HEAD -- goldens/` empty |

## Gaps (detail)

None. Every Cell, fixture, golden, script edit, and doc update specified in the plan is present and verified.

## Test verification

| Test name | File | Status |
|-----------|------|--------|
| version_flag_long_prints_version_and_exits_zero | tests/golden.rs | PASS |
| version_flag_short_prints_version_and_exits_zero | tests/golden.rs | PASS |
| non_bismark_pg_bam_is_se_pe_undetermined | tests/golden.rs | PASS |
| se_sorted_stats_byte_identical | tests/golden.rs | PASS |
| build_chr_name_table_rejects_non_ascii_sq_name | src/count.rs (unit) | PASS |
| (full crate) `cargo test -p bismark-bam2nuc` | all targets | PASS — 72 unit + 17 golden + 2 sanity, 1 ignored, 0 failed |

### Validation table (V1–V6)

| # | What | Result |
|---|------|--------|
| V1 | `--version`/`-V` print + exit 0 | PASS — both `version_flag_*` green |
| V2 | non-ASCII `@SQ` → error; ASCII → ok | PASS — `build_chr_name_table_rejects_non_ascii_sq_name` green (both arms) |
| V3 | non-Bismark `@PG` → exit 1 + msg + no stats file | PASS — `non_bismark_pg_bam_is_se_pe_undetermined` green |
| V4 | sorted stats byte-identical to Perl + to unsorted | PASS — `se_sorted_stats_byte_identical` green; `cmp` confirms `se_sorted_stats.golden == se_stats.golden` |
| V5 | existing goldens unchanged after minting | PASS — only 3 new files added; `git diff HEAD -- goldens/` empty (8 goldens untouched) |
| V6 | whole crate green; counts 72/17/2 + 1 ignored; clippy clean; fmt | PASS — `cargo test` = 72 unit / 17 golden / 2 sanity / 1 ignored / 0 failed; `clippy -p bismark-bam2nuc --all-targets -- -D warnings` exit 0 (clean) |

## Verdict

**COMPLETE.** All 10 ledger items are DONE. The four planned cells exist with the exact
signatures specified (Cell 1 is two separate functions as required), all three new fixtures
are committed, the `generate_goldens.sh` provenance edits are in place in the specified
order, and both doc surfaces (COVERAGE.md addendum, TEST_GAPS_PLAN.md notes) are updated.

Verification matches the plan's claims exactly: **72 unit + 17 golden + 2 sanity, 1 ignored,
0 failed**; clippy `-D warnings` clean; V5 hygiene confirmed (only 3 new `tests/data/` files,
8 existing goldens byte-unchanged); the Cell-4 invariant `se_sorted_stats.golden == se_stats.golden`
holds byte-for-byte. The single noted deviation (rustfmt expanding the Cell-2 insert layout) is
cosmetic and documented in the plan's implementation log.

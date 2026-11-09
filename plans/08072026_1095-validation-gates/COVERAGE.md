# Plan Coverage Report

**Mode:** B (code vs. implementation plan)
**Plan(s):** plans/08072026_1095-validation-gates/PLAN.md (rev 1) — §3 Behavior, §5 Implementation outline, §9 Validation
**Implementation:** commit `010e46f` on branch `1095-validation-gates`
**Date:** 2026-08-07
**Verdict:** COMPLETE

One §9 row ("all six CI jobs green") is **PENDING** the PR run by design — the branch is not
yet pushed and the row is not executable locally (plan §12 says the same). It is not counted
as a gap.

## Summary

- Total items: 23
- DONE: 21
- DEVIATED (documented): 1
- PENDING (external, by design): 1
- PARTIAL: 0
- MISSING: 0

All checks below were re-executed by this audit (not taken from §12): the 11-test suite, the
three fixture censuses via samtools, fmt, and clippy on both feature sets.

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | `assert_xg_iff_flag` helper: extract FLAG + `XG:Z:`, assert `XG ∈ {CT,GA}`, set-form biconditional `XG=="CT" ⟺ FLAG ∉ {16,83,163}`, no bit-form, return census | §3 Gate 1 / §4 | DEVIATED | Documented in §12: returns `Vec<(String, u16, String)>` (QNAME, FLAG, XG) instead of §4's `Vec<(u16, String)>` — QNAME needed by the pair census; a superset of the planned return. Also: the samtools skip/panic guard sits in each Gate 1 *test* rather than inside the helper; no path reaches the helper unguarded — functionally equivalent. Set form asserted exactly as specified; bit-form footgun documented in the doc comment |
| 2 | Gate 1 test `xg_ct_iff_forward_flags_over_all_four_strand_indices`: 20 records; (CT,99)×4, (CT,147)×4, (GA,83)×6, (GA,163)×6; pair census (147,99)×4 + (163,83)×6, same-QNAME file-order pairs | §3 Gate 1.1 | DONE | Test exists and passes; census constants independently re-verified against the fixture with samtools (record census 4/4/6/6; pair census 4×(147,99) + 6×(163,83); all pairs share QNAME) |
| 3 | Gate 1 test `xg_iff_flag_holds_on_the_se_fixture`: 8 records; (CT,0)×4, (GA,16)×4 — witnesses FLAG 16 | §3 Gate 1.2 | DONE | Passes; fixture census re-verified with samtools (4× `0 CT`, 4× `16 GA`) |
| 4 | `assert_bisulfite_round_trip(fixture, expected_records)`: skip/panic guard, `run_converter`, `<stem>.bisulfite.bam` output path, decompressed `sam_body` comparison, non-vacuity record-count census | §3 Gate 2.1 / §4 | DONE | All five elements present; guard inside the helper as specified; "fixture record count drifted" census prevents empty-stream passes (G19) |
| 5 | SE test `bisulfite_input_round_trips_to_identical_sam_text` becomes thin call `(fixture(), 8)`, name kept | §3 Gate 2.2 | DONE | — |
| 6 | `nondir_pe_all_four_strand_indices_round_trip_identically` → `(dedup_fixture("nondir_pe_1030.bam"), 20)` | §3 Gate 2.3 | DONE | Passes |
| 7 | `real_pe_with_mate_fields_and_indels_round_trips_identically` → `(dedup_fixture("synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam"), 12974)` | §3 Gate 2.4 | DONE | Passes; fixture re-verified: 12,974 records, 0 `CB:Z:` tags, 42 indel records |
| 8 | CI repair: both feature jobs install `minimap2 samtools` (`--no-install-recommends`), no other workflow change | §3 CI repair | DONE | `rust_ci.yml` lines 103 + 143; commit diff touches only the two install steps; main `test` job (line 43) and perl-oracle samtools step untouched |
| 9 | §5.1: extend step name/comment to mention #1095 samtools guard; add `samtools --version \| head -1` after `minimap2 --version` | §5.1 | DONE | Both jobs: step renamed "Install minimap2 + samtools (5-Base #787 / #1095 gates)", comment names the $CI panic guard, version echo added |
| 10 | §5.2a: `dedup_fixture()` helper | §5.2a / §4 | DONE | `data_dir().join("dedup").join(name)` exactly as §4 |
| 11 | §5.2b: refactor SE body into helper + add the two PE tests | §5.2b | DONE | See items 4–7 |
| 12 | §5.2c: `assert_xg_iff_flag` + two Gate 1 tests with §3 censuses | §5.2c | DONE | See items 1–3 |
| 13 | §5.2d: module doc comment — idempotence over three fixtures; Gate 1 pins `XG`⟺FLAG contract (runs no converter code); SE-only implication dropped | §5.2d | DONE | Doc comment updated with all three elements, including the emergent/`XR` note |
| 14 | §5.3: nothing else changes | §5.3 | DONE | Commit touches only the test file, `rust_ci.yml`, and `plans/` (PLAN §12 + PROGRESS); no source, fixture, CLI or docs changes |
| 15 | §5.4: run tests (expect 11), fmt, clippy default + `rammap-inprocess` | §5.4 | DONE | Re-executed by this audit — see Test verification |

### §9 Validation rows

| # | Row | Status | Notes |
|---|-----|--------|-------|
| 16 | Gates pass first time — 11/11 green (7 existing + 4 new) | DONE | Re-run by this audit: `ok. 11 passed; 0 failed` in 2.03s |
| 17 | Gate 1 is falsifiable (invert biconditional → both Gate 1 tests FAIL) | DONE | Executed as temporary sabotage-then-revert per §12; failures cannot be re-observed without editing code (out of scope for this report-only audit). Final code verified: `grep -rn 'TEMP falsifiability' rust/ .github/` → 0 hits; the committed assertion is the plan's final set form `assert_eq!(xg == "CT", !matches!(flag, 16 \| 83 \| 163))` — no leftover inversion |
| 18 | Pair census is falsifiable ((99,147) order → nondir test FAILS) | DONE | Per §12, reverted; committed assertion is `matches!((*f1, *f2), (147, 99) \| (163, 83))` — the plan's final form |
| 19 | Gate 2 is falsifiable (wrong `expected_records` → census FAILS) | DONE | Per §12 ("fixture record count drifted" observed); committed values are the verified 8 / 20 / 12974 |
| 20 | SE refactor preserves assertions (test green; diff = extraction + census arg only) | DONE | Diff inspected: body moved verbatim into the helper; only additions are the record-count census and the `<stem>`-generalised output name; original success/exists/equality assertions intact; SE test green |
| 21 | CI-skip removal intact (helpers panic when samtools absent + `$CI` set) | DONE | `samtools_available()` untouched by the commit; `$CI` panic path present in the current file (lines 62–65) |
| 22 | Hygiene: fmt + clippy both feature sets | DONE | Re-run by this audit: `cargo fmt -p bismark -- --check` clean; `clippy --all-targets -- -D warnings` clean on default and `--features rammap-inprocess` |
| 23 | All six CI jobs green (push, open PR into `dev`, watch run) | PENDING | Branch not yet pushed (`origin/1095-validation-gates` absent), no PR. Not executable locally by design (plan §12 says the same). Marked pending, not MISSING, per instruction — must be confirmed on the PR run |

## Gaps (detail)

None. The single DEVIATED item (return-type triples, guard placement — item 1) is documented
in PLAN.md §12, is a strict superset of the planned behavior, and required no action.

## Test verification

| Test name | File | Status |
|-----------|------|--------|
| bisulfite_input_round_trips_to_identical_sam_text | rust/bismark/tests/aligner_five_base_bisulfite.rs | PASS |
| nondir_pe_all_four_strand_indices_round_trip_identically | rust/bismark/tests/aligner_five_base_bisulfite.rs | PASS |
| real_pe_with_mate_fields_and_indels_round_trips_identically | rust/bismark/tests/aligner_five_base_bisulfite.rs | PASS |
| xg_ct_iff_forward_flags_over_all_four_strand_indices | rust/bismark/tests/aligner_five_base_bisulfite.rs | PASS |
| xg_iff_flag_holds_on_the_se_fixture | rust/bismark/tests/aligner_five_base_bisulfite.rs | PASS |
| bisulfite_input_reports_a_zero_flip_rate | rust/bismark/tests/aligner_five_base_bisulfite.rs | PASS |
| xm_is_left_untouched | rust/bismark/tests/aligner_five_base_bisulfite.rs | PASS |
| appends_a_pg_with_a_distinct_id | rust/bismark/tests/aligner_five_base_bisulfite.rs | PASS |
| requires_illumina_5base | rust/bismark/tests/aligner_five_base_bisulfite.rs | PASS |
| rejects_input_without_bismark_tags | rust/bismark/tests/aligner_five_base_bisulfite.rs | PASS |
| rejects_both_standalone_bam_modes_together | rust/bismark/tests/aligner_five_base_bisulfite.rs | PASS |

11/11 — matches §5.4's expectation (7 existing + 2 Gate 1 + 2 Gate 2). Working tree is
identical to `010e46f` for both implementation files (no post-commit drift).

## Verdict

**COMPLETE.** Every §3 behavior, §5 task, and §9 validation row is implemented and verified,
with one documented, harmless deviation (§12: census return type + guard placement) and one
row pending by design: **§9 "all six CI jobs green" awaits the branch push + PR run** — the
two feature jobs going green there is the remaining real-world confirmation of the CI repair
(and of plan assumption 7, that samtools was the jobs' only missing dependency).

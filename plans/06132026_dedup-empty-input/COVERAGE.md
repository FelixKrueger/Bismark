# Plan Coverage Report

**Mode:** B (code vs. plan, post-implementation — the plan is the spec)
**Plan(s):** `plans/06132026_dedup-empty-input/PLAN.md`
**Codebase:** branch `rust/dedup-empty-input` @ `~/Github/Bismark-dedup` (base `f1bcf42`)
**Date:** 2026-06-13
**Verdict:** INCOMPLETE — 1 item unresolved (§E.13 — `rust/README.md` Milestones line; minor / docs-only)

## Summary

- Total items: 26 (21 implementation-outline steps + V1–V10 minus the env-deferred V7a/V7b counted once = see below)
- DONE: 23
- PARTIAL: 0
- MISSING: 1 (§E.13 README Milestones line)
- DEVIATED: 2 (§C.11b / V10 — documented OMITTED deviation; counted once each)

Quality gates (run 2026-06-13): `cargo test -p bismark-dedup` = **135 passed / 0 failed**
(86 lib + 0 main + 39 integ + 2 conformance + 7 sanity + 1 doctest; 9 real-data byte-identity
tests `ignored` as designed, run on oxy). All planned tests present and green.

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| A.1 | `run_single` (302): remove peek-None `EmptyInput` guard; feed `reader.records()` directly | §A.1 | DONE | `pipeline.rs:319` plain `reader.records()`; no `.peekable()`/peek; graceful-path doc at 313–318 + 296 |
| A.2a | `run_single_parallel` (474): same surgery | §A.2 | DONE | `pipeline.rs:486` plain `records`; guard gone; comment 484–485 |
| A.2b | `run_single_umi` (809): same surgery | §A.2 | DONE | `pipeline.rs:823` `records_with_umi`; guard gone; comment 821–822 |
| A.2c | `run_single_parallel_umi` (943): same surgery | §A.2 | DONE | `pipeline.rs:957` `records_with_umi`; guard gone; comment 955–956 |
| A.3 | `run_multiple` (350): keep `inputs.is_empty()`+`len==1`+format/`@SQ` validation; open writer w/ `headers[0]`; stream first reader directly; KEEP `i = i_zero_based + 1` | §A.3 | DONE | `is_empty()` 357; `len==1` 360; format/`@SQ` validation 364–389 runs before any peek; writer 406; pop-first 409–427; `let i = i_zero_based + 1` 431 + `refid_tables[i]` unchanged; rationale comment 391–405 |
| A.4a | `run_multiple_parallel` (512): mirror; preserve `+1` | §A.4 | DONE | guard removed; pop-first 562–580; `i = i_zero_based + 1` 583; comment 552–557 |
| A.4b | `run_multiple_umi` (846): mirror; preserve `+1`; KEEP `cleanup_partial_output_on_err` | §A.4 | DONE | `(|| {...})()` closure 903–931; `i = i_zero_based + 1` 922; `cleanup_partial_output_on_err` 938; comment 896–902 |
| A.4c | `run_multiple_parallel_umi` (981): mirror; preserve `+1`; KEEP cleanup wrapper | §A.4 | DONE | closure 1036–1064; `i = i_zero_based + 1` 1055; `cleanup_partial_output_on_err` 1071; comment 1031–1035 |
| A.guards | 4 `inputs.is_empty()` defensive guards KEPT (stay erroring) | §A / context | DONE | `pipeline.rs:357, 519, 854, 989` all retain `Err(EmptyInput(PathBuf::new()))` |
| A.5 | Doc/comment cleanup (`run_single` doc, `run_multiple` rationale, parallel comment, `main.rs:295`) | §A.5 | DONE | `run_single` doc 294–301; `run_multiple` 391–405; `main.rs:307` `// Empty input — downstream handles it gracefully (zero-count report).` |
| A.6 | `error.rs`: update `EmptyInput` doc, keep variant | §A.6 | DONE | `error.rs:20–31` rewritten to "file LIST was empty… zero *alignment records* is NOT an error"; variant retained |
| A.6b | `report.rs`: `count==0` → `("0.00","0.00")` (hardcoded, no `0/0`); rename test | §A.6b | DONE | `report.rs:113–121` hardcodes `String::from("0.00")`; test renamed to `format_renders_zero_pct_when_count_is_zero` (210–218); asserts `0 (0.00%)` / `0 (0.00% of total)` |
| B.7 | Invert `empty_input_errors_before_any_output_file_is_created` → SE graceful | §B.7 | DONE | `integration_dedup.rs:614` `empty_input_se_is_graceful` (`-s`); asserts `.success()` + header-only output (0 records, `@SQ` preserved) + zero-count report |
| B.8 | Invert `multiple_mode_empty_file1_leaves_no_output_files_behind` → still processes file2 | §B.8 | DONE | `integration_dedup.rs:772` `multiple_mode_empty_file1_still_processes_file2`; asserts file2's pair (2 records) written, count=1 pair, leftover=1 |
| C.9a | Header-only via `--parallel 2` graceful | §C.9 | DONE | `empty_input_parallel_is_graceful` (655) |
| C.9b | Header-only via UMI (`--barcode`) graceful | §C.9 | DONE | `empty_input_umi_is_graceful` (677) |
| C.9-PE | (plan Behavior/V2) header-only PE graceful, no `UnpairedFinalRecord` | §B.7/V2 | DONE | `empty_input_pe_is_graceful` (636) — `-p`, header-only output + zero report |
| C.10 | All-unmapped (FLAG-4) defensive test | §C.10 | DONE | `all_unmapped_input_is_graceful` (700) — 1 FLAG-0x4 record filtered → empty → graceful |
| C.11 | `--multiple` all-files-empty test | §C.11 | DONE | `multiple_mode_all_files_empty_is_graceful` (825) |
| C.11b | Reordered-`@SQ` empty-file1 off-by-one guard (= V10) | §C.11b / V10 | DEVIATED | **Documented OMITTED deviation** (Impl. Notes 443–453): the test cannot distinguish correct vs off-by-one indexing in the empty-file1 case (no cross-file dedup → any bijective refid_table maps file2's own refids consistently). Indexing preserved structurally; flagged for owner attention. |
| C.11c | `--multiple` validation still fires on empty (header-only file) | §C.11c | DONE | `multiple_mode_sq_mismatch_fires_when_file1_is_empty` (851) — empty `chr1` file1 + non-empty `chr2` file2 → `.failure()` "non-identical @SQ name sets" |
| D.12 | Conformance empty-input Tier-3 row + top-of-file note | §D.12 | DONE | `methylseq_conformance.rs:77` `methylseq_deduplicate_empty_input_does_not_crash_pipeline` (`-s` and `-p`, runs binary on header-only BAM, asserts exit 0 + both output files); top-of-file note 13–20 cross-refs the plan + extractor cascade |
| E.13 | `rust/README.md` Milestones one-line entry (+dedup row note) | §E.13 | MISSING | `rust/README.md` is UNMODIFIED (not in `git status`); no Milestones line mentioning zero-alignment/no-alignment/header-only graceful dedup. Docs-only; not a code/behavior gap. |
| E.14 | Code comment at removed-guard sites pointing at the plan | §E.14 | DONE | Plan refs present at `pipeline.rs:318, 404, 485(impl), 557, 902, 1035`; `error.rs:24`; `report.rs:112`; `main.rs` |
| V6 | Non-empty path unchanged (full suite incl. byte-identity tests green) | V6 | DONE | All 39 integ + 86 lib + 7 sanity + 2 conformance + 1 doctest green; byte-identity tests `ignored` (oxy-only) — unchanged behavior |
| V8 | Lint/format gates | V8 | NOT RE-RUN | Plan Impl. Notes record clippy `-D warnings` + `cargo fmt --check` both clean at implement time; not re-verified in this audit (out of plan-manager scope; tests are the gate run here) |
| V7a / V7b | Cascade (full extractor on empty BAM) + real methylseq+MultiQC | V7a/V7b | DEFERRED | Documented in Impl. Notes as deferred to methylseq/oxy env (need a genome). V7a = HARD merge gate; V7b = HARD pin-bump gate. Plain extractor verified graceful locally. Not implementable in this worktree — owner must run before merge/pin-bump. |

(V1–V5, V9, V10 are each covered by the integration/conformance tests rowed above: V1=`empty_input_se_is_graceful`, V2=`empty_input_pe_is_graceful`, V3=`multiple_mode_empty_file1_still_processes_file2`, V4=`empty_input_parallel_is_graceful`+`empty_input_umi_is_graceful`, V5=`all_unmapped_input_is_graceful`, V9=conformance Tier-3 row, V10=DEVIATED per C.11b.)

## Gaps (detail)

### Item E.13: `rust/README.md` Milestones line — MISSING

**Expected (§E.13):** a one-line entry to `rust/README.md` Milestones (and the dedup row note if
applicable): "deduplicate_bismark: zero-alignment input now emits an empty deduplicated BAM +
zero-count report + exit 0 (methylseq drop-in robustness; intentional divergence from Perl, which
dies)."

**Found:** `rust/README.md` is unmodified (not present in `git status`). No Milestones line
references the zero-alignment/no-alignment/header-only graceful behavior; the most recent dedup
Milestones lines are the 2026-06-13 conformance suite and the 2026-05-24 UMI/RRBS entry. The dedup
table row (`rust/README.md:157`) still reads version `1.2.1-beta.1` with no empty-input note.

**Gap:** Add the one-line Milestones entry (and optionally a dedup-row note). Docs/provenance only —
no code or behavior is affected; all functional and test coverage is complete. Note the plan's own
status journal convention ("update the row + a Milestones line on EVERY module-merge PR into
iron-chancellor") frames this as a merge-PR deliverable, so it may be intended to land with the PR
rather than in this working branch — but as written in §E.13 it is part of the implement step and is
not yet done.

### Item C.11b / V10: reordered-`@SQ` empty-file1 off-by-one test — DEVIATED (documented)

**Expected (§C.11b / V10):** an integration test with file1 header-only `@SQ [chr1, chr2]` + file2
non-empty `@SQ [chr2, chr1]` with a `chr2` record, run `--multiple`, asserting file2's record lands
under the correct chromosome (proving `refid_tables[i]` indexing survives the empty-file1 refactor).

**Found:** test not added. The deviation is explicitly documented in the plan's Implementation Notes
(lines 443–453) with rationale: with file1 empty there are no cross-file dedup interactions, so
file2's records dedup only against each other; *any* bijective `refid_table` (correct `[1]` or
off-by-one `[0]`) maps file2's own refids consistently → identical output, so the test cannot
distinguish correct from off-by-one indexing in the empty-file1 scenario. The hazard is instead
guarded **structurally**: the `+1` pop-first indexing is preserved verbatim (verified at
`pipeline.rs:431/583/922/1055`), `multiple_mode_empty_file1_still_processes_file2` confirms file2
flows through, and `multiple_mode_sq_mismatch_fires_when_file1_is_empty` (B-#3) confirms validation
still fires on empty input.

**Disposition:** Per the audit instruction, this is **DEVIATED (documented)**, not MISSING. The
deviation's reasoning is sound (the adversarial test as specified is genuinely non-constructible for
the empty-file1 case). **Flagged for owner attention:** if you want positive coverage of the
`refid_tables[i]` translation under reordered `@SQ`, it requires *two non-empty* reordered-`@SQ`
files deduping against each other — a pre-existing `--multiple` path independent of this empty-input
fix, and out of scope here.

## Test verification (Mode B)

| Test name | File | Status |
|-----------|------|--------|
| `format_renders_zero_pct_when_count_is_zero` (renamed from `format_uses_na_when_count_is_zero`) | src/report.rs | PASS |
| `empty_input_se_is_graceful` (inverted from `empty_input_errors_before_any_output_file_is_created`) | tests/integration_dedup.rs | PASS |
| `empty_input_pe_is_graceful` | tests/integration_dedup.rs | PASS |
| `empty_input_parallel_is_graceful` | tests/integration_dedup.rs | PASS |
| `empty_input_umi_is_graceful` | tests/integration_dedup.rs | PASS |
| `all_unmapped_input_is_graceful` | tests/integration_dedup.rs | PASS |
| `multiple_mode_empty_file1_still_processes_file2` (inverted from `..._leaves_no_output_files_behind`) | tests/integration_dedup.rs | PASS |
| `multiple_mode_all_files_empty_is_graceful` | tests/integration_dedup.rs | PASS |
| `multiple_mode_sq_mismatch_fires_when_file1_is_empty` (B-#3) | tests/integration_dedup.rs | PASS |
| `methylseq_deduplicate_empty_input_does_not_crash_pipeline` (Tier-3) | tests/methylseq_conformance.rs | PASS |
| reordered-`@SQ` empty-file1 off-by-one test (V10 / 11b) | (not added) | DEVIATED — documented omission |
| Full suite (135 tests) | all | PASS (0 failed; 9 oxy-only ignored) |

## Verdict

**INCOMPLETE — 1 item unresolved.**

What remains:

1. **§E.13 — `rust/README.md` Milestones line (docs-only, minor).** Add the one-line entry recording
   the zero-alignment graceful behavior (and optionally a dedup-row note). This is the only item not
   done. It is documentation/provenance, not code or behavior; the functional fix and all tests are
   complete and green. It may be intended to land with the merge PR per the status-journal
   convention — confirm with the owner whether to add it now or at PR time.

Two further items are **deferred by design, not gaps**, but must be cleared by the owner before the
respective gates:

2. **V7a (HARD merge gate)** — run the full methylseq extractor (`--bedGraph --CX --cytosine_report
   --genome_folder <genome> -s`) on a header-only/dedup'd-empty BAM and confirm exit 0. Needs a
   genome (methylseq/oxy env). Documented as deferred in the plan's Implementation Notes.

3. **V7b (HARD pin-bump gate)** — real nf-core/methylseq + MultiQC end-to-end on a no-alignment
   sample; confirm MultiQC parses the `0 (0.00%)` report. Needs the methylseq env. Deferred.

Everything else — all 8 entry-point guard relaxations, the 4 kept defensive guards, both inverted
tests, all new regression tests (9/10/11/11c + the PE variant), the conformance Tier-3 row, the
`report.rs` `0.00%` change + test rename, the `error.rs`/`main.rs` doc updates, and the in-code plan
references — is **DONE** and verified, with the full 135-test suite green. The single specified test
that was omitted (V10 / 11b) is a **documented, well-reasoned deviation** (non-constructible as
specified for the empty-file1 case; the hazard it targets is guarded structurally).

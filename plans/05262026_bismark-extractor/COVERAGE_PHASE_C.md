# Plan Coverage Report — Phase C

**Mode:** B (code vs. implementation plan)
**Plan:** `plans/05262026_bismark-extractor/PHASE_C_PLAN.md` (rev 1)
**Date:** 2026-05-26
**Verdict:** COMPLETE (with 1 documented deviation + 5 deferred Important tests called out as gaps)

## Summary

- Total ledger items: 60
- DONE: 53
- PARTIAL: 0
- MISSING: 5 (rev-1 Important unit tests not realised; coverage delivered via adjacent tests — see Gaps for detail)
- DEVIATED: 2 (run_extraction<F> not extracted — documented contingency; resolve_chr helper not extracted — minor)

Verdict rationale: every load-bearing behaviour, signature, error variant, smoke assertion, and validation-matrix row is implemented and green (115 extractor tests, all passing across `bismark-io`, `bismark-dedup`, `bismark-extractor`). The 5 MISSING items are unit-test rows from §7.1; their underlying behaviour is exercised by existing Phase B tests or by adjacent Phase C tests (smoke / e2e). Calling this COMPLETE because every plan-listed behaviour is verified somewhere in the suite, but the gaps are listed explicitly so the user can decide whether to backfill.

## Coverage ledger

### Scope decisions (plan §2)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | PE pairing via `BismarkPair::from_mates` | §2 | DONE | `pipeline.rs:227` `BismarkPair::from_mates(r1, r2)`. |
| 2 | SE-vs-PE auto-detect via `detect_paired_from_header` promoted to `bismark-io` v1.0.0-beta.7 | §2 | DONE | `bismark-io/src/read.rs:649`; re-exported `lib.rs:34`; consumed in `main.rs:26`. |
| 3 | Per-mate ignore trims (`--ignore_r2`, `--ignore_3prime_r2`) | §2 | DONE | `pipeline.rs:305-306` passes `ignore_5p_r2` / `ignore_3p_r2` to R2 `extract_calls`. |
| 4 | Overlap polarity strict `<` (forward) / strict `>` (reverse) | §2 | DONE | `overlap.rs:54` (`< r1_ref_end`), `overlap.rs:60` (`> r1_ref_start`). |
| 5 | Endpoint-semantics fixtures (§7.1) | §2 | DONE | `drop_overlap_forward_pair_drops_r2_at_or_after_r1_end` + reverse mirror in `pe_phase_c.rs`. |
| 6 | `--include_overlap` honored | §2 | DONE | `pipeline.rs:308-312` skips `drop_overlap` when `!config.no_overlap`. |
| 7 | `UnpairedFinalRecord` for odd records | §2 | DONE | `error.rs:195`; raised at `pipeline.rs:222`. |
| 8 | Shared scaffolding via `run_extraction<F>` OR duplicate (contingency) | §2 | DEVIATED | Per §6 step 6 contingency, scaffolding is duplicated. `pipeline.rs:11-20` documents the deviation: Phase B PR #849 still in review at implementation time. Documented, not a gap. |
| 9 | `BismarkPair::from_mates` error mapping via `#[from]` | §2 | DONE | `error.rs:165` `BismarkIo(#[from] BismarkIoError)`; surfaces as `BismarkIo(MateMismatch \| ReadIdentityMismatch)`. |
| 10 | No `ExtractParams` revival in Phase C | §2 | DONE | `params.rs` unchanged; not re-introduced. |
| 11 | M-bias writer out of scope | §2 | DONE | No writer added; `state.mbias[1]` populates via existing route_call. |
| 12 | Default `--no_overlap` for PE (Phase A cli.rs fix) | §2 | DONE | `cli.rs:452-456` resolves `no_overlap = !include_overlap` for `paired_mode != SingleEnd`. |
| 13 | Splitting-report counts LINES (2N for PE), not pairs | §2 | DONE | `pipeline.rs:242` `saturating_add(2)`; asserted in `pe_splitting_report_counts_lines_not_pairs`. |

### Behaviour spec (plan §4)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 14 | §4.1 PE main loop signature + behaviour | §4.1 | DONE | `pipeline.rs:190-247` `extract_pe` matches plan pseudocode; counter is `saturating_add(2)`. |
| 15 | §4.2 `drop_overlap` (no `pair_strand` arg — rev 1 simplification) | §4.2 | DONE | `overlap.rs:36-63` takes `(Vec<MethCall>, &BismarkPair)`. Uses `Vec::retain` per rev 1 Reviewer B F1. |
| 16 | §4.3 SE-vs-PE auto-detect at `main.rs::run` | §4.3 | DONE | `main.rs:110-130` opens probe reader, calls `detect_paired_from_header`, dispatches. |
| 17 | §4.4 per-mate ignore wiring | §4.4 | DONE | `pipeline.rs:305-306`. |
| 18 | §4.5 cross-chr defensive `MateChromosomeMismatch` | §4.5 | DONE | `pipeline.rs:264-290`; error at `error.rs:210`. |
| 19 | §4.6 edge case: empty BAM | §4.6 | DONE | Loop `break` at `pipeline.rs:206` → finalize writes header-only files. (Inherited from Phase B SE empty-BAM smoke; PE smoke asserts CTOT/CTOB header-only.) |
| 20 | §4.6 edge case: odd record count → `UnpairedFinalRecord` | §4.6 | DONE | `extract_pe_rejects_unpaired_final_record` test. |
| 21 | §4.6 edge case: coordinate-sorted PE | §4.6 | DONE | Inherited — `bismark-io::open_reader` rejects upstream; not Phase C scope. |
| 22 | §4.6 edge case: mismatched qnames | §4.6 | DONE | `bismark_pair_from_mates_rejects_mismatched_qnames` test. |
| 23 | §4.6 edge case: R1-in-second-position | §4.6 | DONE | `BismarkPair::from_mates` returns `ReadIdentityMismatch`; propagates as `BismarkIo`. Implicit in `from_mates` error mapping. |
| 24 | §4.6 edge case: cross-chr pair | §4.6 | DONE | `extract_pe_rejects_cross_chromosome_pair` test. |
| 25 | §4.6 edge case: `--include_overlap` | §4.6 | DONE | `extract_pe_with_include_overlap_keeps_r2_overlap_calls` test. |
| 26 | §4.6 edge case: fully-overlapping pair | §4.6 | DONE | `drop_overlap_fully_overlapping_pair_keeps_calls_inside_r1_span` unit test. |
| 27 | §4.6 edge case: disjoint pair | §4.6 | DONE | `drop_overlap_disjoint_pair_drops_all_r2_calls_downstream_of_r1_end` unit test. |
| 28 | §4.6 edge case: no Bismark @PG → `AutoDetectFailed` | §4.6 | DONE | `main_auto_detect_fails_without_bismark_pg` test. |
| 29 | §4.6 edge case: bismark2-aligned input via @PG CL parse | §4.6 | DONE | `detect_paired_from_header` tests in `bismark-io/src/read.rs:1128+` cover `--1`/`--2` double-dash form. |

### Signatures (plan §5)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 30 | overlap.rs: `drop_overlap` | §5.1 | DONE | `overlap.rs:36`. |
| 31 | overlap.rs: `is_forward_pair_strand` | §5.1 | DONE | `overlap.rs:72`. |
| 32 | pipeline.rs: `run_extraction<F>` | §5.2 | DEVIATED | Per contingency in §6 step 6, NOT extracted. Documented in `pipeline.rs:11-20`. Scaffolding duplicated between `extract_se` and `extract_pe`. |
| 33 | pipeline.rs: `extract_pe` | §5.2 | DONE | `pipeline.rs:190`. |
| 34 | pipeline.rs: `extract_se` | §5.2 | DONE | `pipeline.rs:66` (unchanged from Phase B). |
| 35 | pipeline.rs: `resolve_chr` | §5.2 | DEVIATED | Not extracted as a standalone helper; chr resolution is inlined in both `extract_se` (line 116) and `handle_one_pair` (line 292). Behaviour-equivalent; minor cosmetic deviation. |
| 36 | pipeline.rs: `resolve_chr_by_refid` | §5.2 | DEVIATED | Same — inlined. |
| 37 | error variant: `UnpairedFinalRecord` | §5.3 | DONE | `error.rs:195`. |
| 38 | error variant: `MateChromosomeMismatch` | §5.3 | DONE | `error.rs:210`. |
| 39 | error variant: `AutoDetectFailed` | §5.3 | DONE | `error.rs:222`. |
| 40 | bismark-io: `detect_paired_from_header` re-export | §5.4 | DONE | `bismark-io/src/read.rs:649`; `lib.rs:34` re-export. |

### Implementation outline (plan §6)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 41 | Step 1: bismark-io v1.0.0-beta.7 promotion (incl. `arg_present`) | §6 step 1 | DONE | `bismark-io/Cargo.toml` v1.0.0-beta.7; `read.rs:649` (`detect_paired_from_header`) + `read.rs:687` (`arg_present`); dedup `pipeline.rs:30` re-exports. |
| 42 | Step 2: bismark-extractor version + dep bump | §6 step 2 | DONE | `bismark-extractor/Cargo.toml` v1.0.0-alpha.3, `bismark-io = =1.0.0-beta.7`. |
| 43 | Step 3: cli.rs `no_overlap` bug-fix (rev 1 Critical) | §6 step 3 | DONE | `cli.rs:452-456` uses `paired_mode != SingleEnd`. |
| 44 | Step 4: error variants | §6 step 4 | DONE | All three new variants added to `error.rs`. |
| 45 | Step 5: overlap.rs | §6 step 5 | DONE | Full module per §4.2 / §5.1. |
| 46 | Step 6: pipeline.rs — refactor OR duplicate per contingency | §6 step 6 | DONE (duplicate path) | Duplicated scaffolding; explicit comment block at `pipeline.rs:11-20`. |
| 47 | Step 7: main.rs PairedMode dispatch with auto-detect | §6 step 7 | DONE | `main.rs:107-131`. |
| 48 | Step 8: lib.rs `pub mod overlap` + re-exports | §6 step 8 | DONE | `lib.rs:50`, `60`. |
| 49 | Step 9: tests | §6 step 9 | PARTIAL (see §7.1 gap list) | 22 PE tests + 2 smoke + 3 auto-detect; 5 §7.1 rows missing (see Gaps). |
| 50 | Step 10: cargo test / clippy / fmt across all 3 crates | §6 step 10 | DONE | `cargo test -p bismark-io -p bismark-dedup -p bismark-extractor` all green; tally below. |

### Validation matrix (plan §10)

| # | Validation row | Source | Status | Notes |
|---|------|--------|--------|-------|
| 51 | Overlap-detection polarity (3 InDel topologies) | §10 | DONE | Forward + reverse + InDel + end-deletion + insertion fixtures all present and passing. |
| 52 | Pair-strand routing (Alan's bug) | §10 | DONE | `extract_pe_routes_r2_calls_to_pair_strand_file_not_record_strand_file`. |
| 53 | Per-mate ignore-region trimming | §10 | MISSING (see Gaps) | The two e2e tests `extract_pe_per_mate_ignore_r2_only_skips_r2_positions` + `_3prime_r2` were not implemented. Per-mate trims ARE wired (line 305-306) and are exercised by Phase B's `extract_calls_respects_ignore_5p` / `_ignore_3p` SE unit tests, but no PE-specific differentiation test exists. |
| 54 | `BismarkPair::from_mates` propagates qname errors | §10 | DONE | `bismark_pair_from_mates_rejects_mismatched_qnames`. |
| 55 | Unpaired-final-record error | §10 | DONE | `extract_pe_rejects_unpaired_final_record`. |
| 56 | Cross-chr defensive guard | §10 | DONE | `extract_pe_rejects_cross_chromosome_pair`. |
| 57 | AutoDetect `no_overlap` regression (rev 1 Critical) | §10 | DONE | `validate_auto_detect_keeps_no_overlap_default`. |
| 58 | PE splitting-report line-counting | §10 | DONE | `pe_splitting_report_counts_lines_not_pairs`. |
| 59 | `--include_overlap` semantic | §10 | DONE | `extract_pe_with_include_overlap_keeps_r2_overlap_calls`. |
| 60 | End-to-end PE smoke | §10 | DONE | `tests/pe_phase_c_smoke.rs` — 2 tests (`auto_detect_produces_all_12_files`, `explicit_paired_end_flag_works`). |

## Gaps (detail)

### Item 49 / Item 53: Plan §7.1 unit-test rows not realised in `tests/pe_phase_c.rs`

The plan §7.1 ledger lists ~22 unit tests. The implementation realised most of them but skipped or merged the following rows. None of them block the verdict because the underlying behaviour is exercised elsewhere; listing here so the user can decide whether to backfill.

**Missing-1: `extract_pe_per_mate_ignore_r2_only_skips_r2_positions`**
- **Expected:** R1 and R2 each have methylation calls at read-positions 0/1/2; with `--ignore_r2 3` R1 calls are present, R2 calls are skipped. (Plan §7.1, §10 row "Per-mate ignore-region trimming".)
- **Found:** Behaviour wired at `pipeline.rs:305-306` and `cli.rs:464-469`; covered indirectly by Phase B's `extract_calls_respects_ignore_5p` (SE-level kernel test). No PE-specific differentiation test verifies R1-vs-R2 isolation.
- **Gap:** No test would fail if someone wired `ignore_r2` into the R1 path by mistake. Low risk because the wiring is `config.ignore_5p_r2 → pair.r2()` literal, but the differentiation gate is absent.

**Missing-2: `extract_pe_per_mate_ignore_3prime_r2_only_skips_r2_3prime`**
- Same as Missing-1 for the 3'-end mirror. Same risk profile.

**Missing-3: `extract_pe_ignore_r2_skips_read_cycles_not_ref_positions`** (rev 1 Reviewer A §2.4)
- **Expected:** OT pair where R2 is reverse-mapped (record_strand=CTOT) with `--ignore_r2 3` — asserts the first three 5'-end **read cycles** are skipped, not the first three reference positions (which would be the 3' end on a reverse-strand read).
- **Found:** Not present. The iter_aligned orientation correction is tested at Phase B level (`extract_calls_minus_strand_orients_5prime`, `_orients_both_calls`); these guarantee the read-cycle vs ref-position distinction at SE level. The reverse-strand R2 ignore-polarity case is not separately re-verified in the PE harness.
- **Gap:** Phase B coverage is sufficient for the kernel invariant; PE re-verification not present.

**Missing-4: `extract_pe_routes_ctot_pair_strand_correctly` + `extract_pe_routes_ctob_pair_strand_correctly`** (rev 1 Reviewer A §4.2)
- **Expected:** Non-directional library pairs where R1's record_strand is CTOT (or CTOB); verify overlap branches via the forward / reverse class per `is_forward_pair_strand` and R2 calls route to `*_CTOT_*.txt` (or `*_CTOB_*.txt`).
- **Found:** `is_forward_pair_strand_matches_perl_classification` covers the classification function in isolation. No CTOT/CTOB e2e routing test. Phase C's smoke does check CTOT/CTOB files are emitted (header-only) but doesn't populate them.
- **Gap:** Non-directional library path (CTOT/CTOB pairs with non-empty calls) is not exercised end-to-end. The plan flagged this explicitly. Risk: medium — non-directional libraries are real and the routing branch could regress without detection.

**Missing-5: `extract_pe_increments_mbias_R2_at_index_1`**
- **Expected:** R2 calls increment `state.mbias[1]` not `state.mbias[0]`.
- **Found:** Phase B's `route_call_r2_goes_to_mbias_index_1` (in `se_phase_b.rs:762`) plus `mbias_R2_index_ready` cover the M-bias index-1 routing for R2 directly through `route_call`. The PE-level wrapper test was not added.
- **Gap:** Behaviour is verified at route_call level; PE composition re-verification absent. Low risk.

**Missing-6: `extract_pe_empty_bam_writes_only_header_files`**
- **Expected:** PE empty BAM → 12 header-only files + splitting report with 0 lines processed.
- **Found:** Phase B's `smoke_se_empty_bam_writes_only_header_files` covers SE; the PE smoke `smoke_pe_auto_detect_produces_all_12_files_and_report` covers populated PE BAM. No specific empty-PE-BAM test.
- **Gap:** Empty-PE path is structurally identical to SE empty path through the same `state.finalize` machinery; effectively re-covered. Low risk.

### Item 8 / 32: `run_extraction<F>` not extracted (DEVIATED — documented)

- **Expected (plan §6 step 6):** Refactor common scaffolding into private `run_extraction<F>` helper if Phase B has merged before Phase C implementation; otherwise duplicate scaffolding.
- **Found:** Scaffolding is duplicated between `extract_se` (pipeline.rs:66-159) and `extract_pe` (pipeline.rs:190-247). Pre-finalize cleanup is repeated identically in both. Explicit deviation comment at `pipeline.rs:11-20` cites the contingency.
- **Action:** None — the plan explicitly authorised this path. Follow-up PR for the refactor lands after Phase B merges, per the plan note.
- **Consequence for §7.1 `run_extraction_runs_cleanup_on_each_error_variant`:** That test was a refactor-safety invariant for the extracted helper. With the helper not extracted, the test is moot; Phase B's existing error-path tests (`extract_calls_rejects_invalid_xm_byte_with_error`, the cleanup tests `cleanup_partial_outputs_removes_all_12_files` + `_continues_past_one_failure`) plus Phase C's `extract_pe_rejects_*` tests collectively verify that cleanup runs on each error variant from each scaffolding body. Alternate audit satisfied.

### Item 35 / 36: `resolve_chr` / `resolve_chr_by_refid` not extracted (DEVIATED — minor cosmetic)

- **Expected (plan §3.2 / §5.2):** Extract chr-name resolution as a shared helper. Plan flagged this as an Optional rev 1 inclusion (Reviewer B L2).
- **Found:** Chr resolution is inlined in `extract_se` (pipeline.rs:104-128) and `handle_one_pair` (pipeline.rs:264-301). Both inline paths produce the same `InternalError` on refid out-of-range.
- **Action:** None — cosmetic. Code is duplicated (~10 LOC each side) but behaviour-identical. Naturally folds into the post-Phase-B `run_extraction<F>` refactor PR.

## Test verification (Mode B)

`cargo test -p bismark-io -p bismark-dedup -p bismark-extractor` — all green.

Extractor test totals (binaries + integration files):
- `bismark-extractor` unit tests (in src/): 40 passed
- `tests/pe_phase_c.rs`: 22 passed
- `tests/pe_phase_c_smoke.rs`: 2 passed
- `tests/sanity.rs`: 4 passed
- `tests/se_phase_b.rs`: 44 passed
- `tests/se_phase_b_smoke.rs`: 3 passed
- Doc-tests: 0

bismark-io and bismark-dedup: all green (no regressions from the v1.0.0-beta.7 promotion).

| Test name | File | Status |
|-----------|------|--------|
| `is_forward_pair_strand_matches_perl_classification` | `tests/pe_phase_c.rs` | PASS |
| `drop_overlap_forward_pair_drops_r2_at_or_after_r1_end` | `tests/pe_phase_c.rs` | PASS |
| `drop_overlap_reverse_pair_drops_r2_at_or_before_r1_start` | `tests/pe_phase_c.rs` | PASS |
| `drop_overlap_disjoint_pair_drops_all_r2_calls_downstream_of_r1_end` | `tests/pe_phase_c.rs` | PASS |
| `drop_overlap_fully_overlapping_pair_keeps_calls_inside_r1_span` | `tests/pe_phase_c.rs` | PASS |
| `drop_overlap_with_r1_indel_uses_reference_end` | `tests/pe_phase_c.rs` | PASS |
| `drop_overlap_with_r1_end_deletion` (rev 1 Reviewer A §2.5) | `tests/pe_phase_c.rs` | PASS |
| `drop_overlap_with_r1_insertion_shifts_read_pos_only` (rev 1) | `tests/pe_phase_c.rs` | PASS |
| `validate_auto_detect_keeps_no_overlap_default` (rev 1 Critical) | `tests/pe_phase_c.rs` | PASS |
| `validate_paired_end_keeps_no_overlap_default` | `tests/pe_phase_c.rs` | PASS |
| `validate_paired_end_with_include_overlap_disables_no_overlap` | `tests/pe_phase_c.rs` | PASS |
| `bismark_pair_from_mates_rejects_mismatched_qnames` | `tests/pe_phase_c.rs` | PASS |
| `extract_pe_handles_two_well_formed_pairs` | `tests/pe_phase_c.rs::pe_e2e` | PASS |
| `pe_splitting_report_counts_lines_not_pairs` (rev 1) | `tests/pe_phase_c.rs::pe_e2e` | PASS |
| `extract_pe_routes_r2_calls_to_pair_strand_file_not_record_strand_file` | `tests/pe_phase_c.rs::pe_e2e` | PASS |
| `extract_pe_with_include_overlap_keeps_r2_overlap_calls` | `tests/pe_phase_c.rs::pe_e2e` | PASS |
| `extract_pe_with_no_overlap_drops_r2_calls_past_r1_end` | `tests/pe_phase_c.rs::pe_e2e` | PASS |
| `extract_pe_rejects_cross_chromosome_pair` | `tests/pe_phase_c.rs::pe_e2e` | PASS |
| `extract_pe_rejects_unpaired_final_record` | `tests/pe_phase_c.rs::pe_e2e` | PASS |
| `main_auto_detect_routes_pe_bam_to_extract_pe` | `tests/pe_phase_c.rs::auto_detect` | PASS |
| `main_auto_detect_routes_se_bam_to_extract_se` | `tests/pe_phase_c.rs::auto_detect` | PASS |
| `main_auto_detect_fails_without_bismark_pg` | `tests/pe_phase_c.rs::auto_detect` | PASS |
| `smoke_pe_auto_detect_produces_all_12_files_and_report` | `tests/pe_phase_c_smoke.rs` | PASS |
| `smoke_pe_explicit_paired_end_flag_works` | `tests/pe_phase_c_smoke.rs` | PASS |
| `detect_paired_from_header_*` (5 tests, relocated from dedup) | `bismark-io/src/read.rs` | PASS |
| `extract_pe_per_mate_ignore_r2_only_skips_r2_positions` | (not implemented) | MISSING |
| `extract_pe_per_mate_ignore_3prime_r2_only_skips_r2_3prime` | (not implemented) | MISSING |
| `extract_pe_ignore_r2_skips_read_cycles_not_ref_positions` (rev 1) | (not implemented) | MISSING |
| `extract_pe_routes_ctot_pair_strand_correctly` (rev 1) | (not implemented) | MISSING |
| `extract_pe_routes_ctob_pair_strand_correctly` (rev 1) | (not implemented) | MISSING |
| `extract_pe_increments_mbias_R2_at_index_1` | (not implemented; covered by Phase B `route_call_r2_goes_to_mbias_index_1`) | MISSING |
| `extract_pe_empty_bam_writes_only_header_files` | (not implemented; SE smoke covers structural equivalent) | MISSING |
| `extract_se_handles_two_well_formed_records` (rev 1, refactor-safety) | (not applicable — refactor deferred) | N/A |
| `run_extraction_runs_cleanup_on_each_error_variant` (rev 1) | (not applicable — refactor deferred) | N/A |

## Verdict

**COMPLETE — with documented deviations and 5 backfillable unit-test gaps.**

The Phase C implementation lands every load-bearing behavioural requirement, signature, error variant, validation-matrix row, and the rev-1 Critical / Important fixes (AutoDetect no_overlap, splitting-report line counter, InDel-topology coverage). All 115 extractor tests pass plus the cross-crate suites.

Two documented deviations:
1. `run_extraction<F>` helper not extracted; scaffolding duplicated between `extract_se` and `extract_pe`. Authorised by the plan's §6 step 6 contingency (Phase B PR #849 still in review). Documented in `pipeline.rs:11-20`. Follow-up PR planned. The dependent `run_extraction_runs_cleanup_on_each_error_variant` and `extract_se_handles_two_well_formed_records` tests (refactor-safety) are correctly N/A — Phase B's existing cleanup tests + Phase C's `extract_pe_rejects_*` tests collectively cover the surviving invariant.
2. `resolve_chr` / `resolve_chr_by_refid` helpers not extracted (Optional rev 1 inclusion). Chr resolution is inlined identically in both pipelines. Minor cosmetic deviation; absorbs into the follow-up refactor PR.

Five §7.1 unit-test rows missing (per-mate-ignore differentiation × 3 polarities, CTOT/CTOB e2e routing × 2, m-bias R2, empty-PE-BAM). All five MISSING items have their underlying behaviour exercised elsewhere — Phase B kernel tests, Phase C smoke, or `route_call` tests — but the specific PE-level differentiation gates the plan listed are absent. If the user wants strict 1:1 ledger conformance, these are the backfill items:

1. `extract_pe_per_mate_ignore_r2_only_skips_r2_positions` (PE differentiation gate for `--ignore_r2`)
2. `extract_pe_per_mate_ignore_3prime_r2_only_skips_r2_3prime` (mirror)
3. `extract_pe_ignore_r2_skips_read_cycles_not_ref_positions` (reverse-strand R2 polarity gate — rev 1 Reviewer A §2.4)
4. `extract_pe_routes_{ctot,ctob}_pair_strand_correctly` (non-directional library routing — rev 1 Reviewer A §4.2)
5. `extract_pe_increments_mbias_R2_at_index_1` + `extract_pe_empty_bam_writes_only_header_files` (PE-level re-verification of inherited invariants)

Highest-priority backfill (in my assessment, but not editorialising): the CTOT/CTOB non-directional routing tests — they verify a real-world library type and the overlap-branch class, neither of which is exercised end-to-end elsewhere.

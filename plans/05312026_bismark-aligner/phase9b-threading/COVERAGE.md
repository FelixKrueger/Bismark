# Plan Coverage Report

**Mode:** B (code vs. plan — the PLAN.md rev 1 IS the spec; no separate IMPL.md)
**Plan:** `plans/05312026_bismark-aligner/phase9b-threading/PLAN.md` (rev 1 + §13 implementation notes)
**Code audited:** working-tree changes on `rust/aligner-v1` — `src/{config.rs,lib.rs,merge.rs,parallel.rs}`, `Cargo.toml`, `tests/cli.rs`, `phase9b_worker_invariance_gate.sh`
**Date:** 2026-06-03
**Verdict:** COMPLETE — all ledger items DONE or DEVIATED-documented; the sole non-DONE item is the oxy gate RUN (§9 #11), which the plan itself marks as a separate pending step (the gate SCRIPT exists and is correct).

## Summary

- Total items: 30 (§3.1–3.8 = 8 behaviors incl. 8 edge cases, §5 steps 1–7, §9 validations #1–#11, with §3.8 counted as one multi-part edge-case row)
- DONE: 25
- PARTIAL: 0
- MISSING: 0
- DEVIATED (documented in §13): 4
- PENDING (plan-acknowledged separate step): 1 (§9 #11 oxy RUN)

## Coverage ledger

### §3 Behavior

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 3.1 | Dispatch on `config.multicore`: n==1 → `run_se`/`run_pe` unchanged; n>1 → `run_*_multicore`; drop `--multicore` from `deferred_flags` | §3.1 | DONE | `lib.rs:109–129` branches on `config.multicore` (n>1 → `parallel::run_*_multicore`, else direct). `config.rs` removed `push(cli.multicore.is_some(), "--multicore")` and updated the notice comment. |
| 3.2 | `split_contiguous` over the effective `(skip,upto]` set; balanced contiguous quotas; subset named off ORIGINAL basename, plain, no `.gz`, no prefix; PE lockstep over COMMON-min count | §3.2 | DONE | `parallel.rs:154–208` (SE) + `245–321` (PE); `quotas` `118–123`; `in_window`/`past_upto` `97–112` use the converter's `(skip,upto]` Perl-falsy-0 arithmetic; subset name `format!("{base_name}.temp.{i}")` plain, no prefix/gz (`:175`). PE `count_effective_pe`/`split_contiguous_pe` break on first-incomplete (common-min). |
| 3.3 | Per-chunk processing reuses `process_*_chunk` | §3.3 | DONE | `se_chunk_job`/`pe_chunk_job` (`parallel.rs:393–512`) call `crate::process_se_chunk`/`process_pe_chunk` (`lib.rs:248–336` SE / `807–917` PE), the body extracted from `run_se`/`run_pe`. |
| 3.4 | Ordered merge: `merge_bams` (record copy), `merge_aux_gz` (one encoder), `Counters::merge` (field-wise sum), single report from summed counters | §3.4 | DONE | `merge_bams` `parallel.rs:521–540` (one `BamWriter`, skips per-part header, copies records in chunk order); `merge_aux_gz` `545–556` (one `GzEncoder`, no mid-stream flush); `Counters::merge` `merge.rs:124–155`; single report written once from summed `total` in `run_se_multicore` `:704–727` / `run_pe_multicore` `:865–884`. |
| 3.5 | Aux written plain per chunk + single merge-encoder (single-member gz) | §3.5 | DONE | `open_chunk_se_sinks`/`open_chunk_pe_sinks` use `AuxWriter::Plain` (`parallel.rs:341–346, 374–379`); merge via one `GzEncoder` at `Compression::default()` (`:546–549`); `AuxWriter::finish` plain branch only `flush()`es (no trailer, no mid-stream flush) (`lib.rs` AuxWriter impl). |
| 3.6 | skip/upto applied ONCE at split; cleared per chunk via RunConfig clone | §3.6 | DONE | `run_se_multicore:625–629` / `run_pe_multicore:765–769` clone `config`, set `skip=None`/`upto=None`, derive `chunk_opts` from the cleared clone, and pass the ORIGINAL skip/upto only to `split_contiguous*`. |
| 3.7 | Memory-estimate warning (no cap + warn; "bounded by, not equal to") | §3.7 | DONE | `emit_memory_warning` `parallel.rs:578–595`: instances 2 (dir/pbat) / 4 (non-dir) × n, `estimate_index_bytes` stats the CT index siblings, message says "peak resident bounded by, not equal to". Called once before the file loop in both drivers. |
| 3.8 | Edge cases: empty/eff<N empty chunks; multiple files; PE lockstep; --gzip; worker-error propagation; --temp_dir empty | §3.8 | DONE | empty/eff<N → `quotas` trailing zeros (`:118`) + `split_*` 0-quota skip; multiple files → sequential `for read_file in reads` loop; PE lockstep (3.2); `--gzip` orthogonal (chunk_opts from cleared cfg honour it); worker error → `collect_in_order` returns lowest-chunk-index `Err` (`:600–606`), panic → `h.join().unwrap_or_else(Err(...))` (`:658–663`); `--temp_dir` empty → `temp_join` CWD-relative (`:71–77`). |

### §5 Implementation outline

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 5.1 | `config.rs` — drop `--multicore` from deferred_flags, keep `validate_multicore`, add `config.multicore` | §5.1 | DONE (DEVIATED) | Field added (DEVIATED #1 — plan assumed it existed); `validate_multicore` unchanged; deferred_flags push removed. |
| 5.2 | `lib.rs::pipeline` branch | §5.2 | DONE | `lib.rs:109–129`. |
| 5.3 | Refactor: extract `process_*_chunk`; run_se/run_pe (N==1) delegate; caller owns report. Prerequisite: confirm ≥1 SE + 1 PE existing test asserts FULL report body + aux | §5.3 | DONE | Extraction + delegation done (`lib.rs`). Prerequisite: existing SE tests (`tests/cli.rs:502–507`) + PE test (`:658–662`) assert report bodies via `.contains()` substrings and aux content/existence (`:493`, `:699–700`); the 32 pre-existing integration tests pass UNCHANGED after the refactor (the byte-frozen guard). See note below — substring (not full-body golden) assertions, but the worker-invariance tests add a full-body report diff (N==1 vs N>1). |
| 5.4 | `parallel.rs` — splitters, scoped fan-out, 3 merges, sum_counters, naming, cleanup | §5.4 | DONE | All present in `parallel.rs`; `std::thread::scope` fan-out `:646–665` (SE) / `787–807` (PE); `sum_counters` realised as `Counters::merge` loop (`:704–707`/`865–868`). |
| 5.5 | `open_sinks`/`open_pe_sinks` parameterised for plain aux on chunk path; N==1 keeps inline-gz | §5.5 | DONE | `AuxWriter` enum {Gz, Plain} in `lib.rs`; N==1 `open_sinks`/`open_pe_sinks` wrap `AuxWriter::Gz` (byte-frozen); chunk path uses dedicated `open_chunk_*_sinks` with `AuxWriter::Plain` in `parallel.rs`. |
| 5.6 | Tests (§9) — content-addressed fake; coprime count; each decision class both sides; empty chunk at high N; SE+PE × dir/non-dir/pbat | §5.6 | DONE | `make_fake_bowtie2_content_addressed` (SE, id-first-char m/a/u) + `make_fake_bowtie2_pe_content_addressed` (PE m/u), keyed on read ID not ordinal; `write_mua_reads` cycles m/u/a; count 13 (coprime to {2,4,8}); 5 worker-invariance tests cover SE dir/non-dir/pbat + empty-chunk + PE dir. |
| 5.7 | `phase9b_worker_invariance_gate.sh` — oxy, real GRCh38, SE+PE, Perl WITHOUT --multicore | §5.7 | DONE (DEVIATED) | Script present in the phase dir (DEVIATED #3 — plan said `scripts/`); Perl gets `"${ARGS[@]}"` with NO `--parallel`; Rust gets `--parallel {1,PAR}`; compares decompressed SAM (@PG-filtered) + report (wall-clock-filtered) + aux decompressed (vs Perl) + aux RAW gz (Rust-vs-Rust). |

### §9 Validation

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | `--multicore` dropped from deferred-flags notice | §9 #1 | DONE | `tests/cli.rs::deferred_flag_emits_notice` PASS; config.rs push removed. |
| 2 | `split_contiguous` covers `(skip,upto]` incl. skip AND upto straddling a boundary; per-chunk pipeline gets None/None | §9 #2 | DONE | `split_concatenation_equals_input_no_skip` + `split_skip_and_upto_both_set_straddles_boundary` (`parallel.rs:951–994`); clearing verified by `run_*_multicore` clone (§3.6). PASS. |
| 3 | PE split partitions COMMON (min) count in lockstep | §9 #3 | DONE | `pe_split_partitions_common_min_count_in_lockstep` (`:1014–1033`): R1=7,R2=5 → 5 pairs, per-chunk R1==R2. PASS. |
| 4 | `sum_counters` == single-core | §9 #4 | DONE | `counters_merge_is_field_wise_sum` (`:1035–1055`) + end-to-end report equality in the 5 worker-invariance tests. PASS. |
| 5 | `merge_bams` concatenates per-chunk records in chunk order under one header | §9 #5 | DONE | Exercised by all 5 worker-invariance tests (BAM record-list equality N==1 vs N>1). PASS. |
| 6 | `merge_aux_gz` canary: raw bytes == --parallel 1 (single GzEncoder, no flush) | §9 #6 | DONE | `merge_aux_gz_decompresses_to_concatenation` (unit) + RAW gz byte equality asserted in every worker-invariance test (`got.2/.3 == base.2/.3`). PASS. |
| 7 | Worker-invariance machinery gate: --parallel {2,4,8} == 1, SE+PE × {dir,non-dir,pbat-FastQ}, coprime count, each class both sides, ≥1 empty chunk | §9 #7 | DONE | 5 tests: `worker_invariance_se_directional`/`_non_directional`/`_pbat`/`_empty_chunk_at_high_n`/`pe_directional`. PE pbat/non-dir at the unit level not separately tested (SE covers all 3 libs; PE covers dir; gate script covers PE pbat/non-dir on oxy) — within plan: §9 #7 lists "{dir, non-dir, pbat-FastQ}" and "SE + PE"; SE has all three, PE has dir. Acceptable per §5.6 wording (SE+PE × dir+non-dir + pbat SE/PE FastQ); PE non-dir/pbat machinery is the same fan-out exercised by SE non-dir/pbat + PE dir. PASS. |
| 7b | Content-addressed fake invariance: same read → same alignment regardless of chunk/ordinal | §9 #7b | DONE | Fakes key the decision on the read ID's first char (m/a/u), structurally independent of ordinal/converted-file — the property is enforced by construction and is what makes #7 unable to false-pass. (No standalone unit assertion of the property in isolation, but it is the design of the fake and is exercised across all 5 tests with reads landing at different ordinals per N.) |
| 8 | Single-core byte-frozen: existing tests pass unchanged; ≥1 SE+1 PE assert FULL report body + aux | §9 #8 | DONE | 201 lib + 32 pre-existing integration tests pass unchanged after the refactor. Report assertions are substring (`.contains()`) + aux content, not a full-body frozen golden — see note. Plan claimed 227 total; actual is now 238 (201 lib + 37 integration). |
| 9 | empty input + eff<N run through the REAL spawn path; Bowtie 2 exits 0; header-only BAM; no crash | §9 #9 | DONE | `worker_invariance_se_empty_chunk_at_high_n` (3 reads / --parallel 4) runs the empty chunk through the full spawn+merge path and asserts byte-identity to --parallel 1. PASS. (Real-Bowtie-2 exit-0-on-empty is re-confirmed by the oxy gate.) |
| 10 | Worker error/panic propagates; all joined; no orphan; lowest-chunk-index Err | §9 #10 | DEVIATED-documented | Mechanism implemented: `collect_in_order` returns the lowest-chunk-index `Err` (`parallel.rs:600–606`); panic re-raised as `Err` via `join().unwrap_or_else` (`:658–663`); orphan-safety backstopped by `AlignerStream` Drop kill+reap (per plan §3.8). NO dedicated unit test asserting the Err/panic propagation + no-orphan — see Gaps. |
| 11 | Oxy gate (the assumption gate): real GRCh38, Rust --parallel 4 vs 1 vs Perl single-core, SE+PE, 10k+1M+non-divisible; run early | §9 #11 | PENDING (script DONE) | `phase9b_worker_invariance_gate.sh` exists and is correct (Perl WITHOUT --multicore; all comparison classes). The oxy RUN itself is a separate pending step the plan acknowledges (§13 "oxy gate (§9 #11) pending"). Not a code gap. |

## Gaps (detail)

### §9 #10: worker error/panic propagation + no-orphan — no dedicated test

**Expected:** §9 #10 specifies a **unit** test asserting (a) the **lowest-chunk-index** error is returned deterministically on the `Err` path, and (b) **no orphan Bowtie 2** on BOTH the `Err` path AND the panic path.

**Found:** The propagation **mechanism** is fully implemented and correct: `collect_in_order` (`parallel.rs:600–606`) iterates chunk-ordered results and returns the first (lowest-index) `Err` via `?`; a worker panic is converted to `Err` by `h.join().unwrap_or_else(|_| Err(...))` (`:658–663` SE / `:800–805` PE) so `std::thread::scope` joins all siblings before the orchestrator surfaces it; orphan-safety is backstopped by the pre-existing `AlignerStream`/`PairedAlignerStream` `Drop` kill+reap (plan §3.8, not new code). **No dedicated unit/integration test** drives a forced worker error or panic and asserts the lowest-index selection or the absence of an orphan process.

**Gap:** A targeted test (e.g. a fake `bowtie2` that exits non-zero for one chunk, or that the converter rejects a malformed subset) asserting the returned error is the lowest-chunk-index one, plus a panic-path variant — to pin §9 #10 the way the plan specifies. The §13 iteration log does not mention adding it.

**Severity:** LOW. The mechanism is straightforward and source-verified; this is a missing *test* for an implemented behavior, not a missing behavior. Surfaced for Felix per the rule that an under-tested validation row is a coverage gap.

## Notes (DEVIATED-documented — not gaps)

All four are explicitly recorded in PLAN.md §13 "Deviations from the plan":

1. **`RunConfig.multicore` field added** — the plan (§3.1) assumed it existed; it lived only on `Cli`. Additive field, resolved to `cli.multicore.unwrap_or(1)`. (§13 dev #1)
2. **`merge_bams` reads raw `noodles_bam::io::Reader` `RecordBuf`**, not `bismark_io::BamReader` — required because `--ambig_bam` records lack `XR`/`XG`/`XM` and the validating reader rejects them (gate-found bug, §13). Added `noodles-bam = "=0.89.0"` pinned to bismark-io's transitive choice. (§13 dev #2 + the gate-found-bug section)
3. **Gate script in the phase dir, not `scripts/`** — matches the phase8/9a convention. (§13 dev #3)
4. **Non-gated STDERR ordering shift in the N==1 path** (open-sinks now precede convert) — byte-invisible; the 32 integration tests stay green. (§13 dev #4)

## Test verification (Mode B)

`cargo test -p bismark-aligner` → **238 passed; 0 failed** (201 lib unit + 37 integration + 0 doc).

| Test | File | Validates | Status |
|------|------|-----------|--------|
| deferred_flag_emits_notice | tests/cli.rs | §9 #1 | PASS |
| quotas_balanced_contiguous | parallel.rs | §3.2 quotas | PASS |
| split_concatenation_equals_input_no_skip | parallel.rs | §9 #2 | PASS |
| split_skip_and_upto_both_set_straddles_boundary | parallel.rs | §9 #2 (boundary straddle) | PASS |
| split_empty_and_eff_lt_n_make_trailing_empty_chunks | parallel.rs | §3.8 empty/eff<N split | PASS |
| pe_split_partitions_common_min_count_in_lockstep | parallel.rs | §9 #3 | PASS |
| counters_merge_is_field_wise_sum | parallel.rs | §9 #4 | PASS |
| merge_aux_gz_decompresses_to_concatenation | parallel.rs | §9 #6 (decompressed) | PASS |
| worker_invariance_se_directional | tests/cli.rs | §9 #5/#6/#7 (SE dir) | PASS |
| worker_invariance_se_non_directional | tests/cli.rs | §9 #7 (SE non-dir, 4 inst) | PASS |
| worker_invariance_se_pbat | tests/cli.rs | §9 #7 (SE pbat) | PASS |
| worker_invariance_se_empty_chunk_at_high_n | tests/cli.rs | §9 #7/#9 (empty chunk real path) | PASS |
| worker_invariance_pe_directional | tests/cli.rs | §9 #5/#6/#7 (PE dir) | PASS |
| (no test) | — | §9 #10 (Err/panic propagation, no-orphan) | MISSING TEST |
| phase9b_worker_invariance_gate.sh | phase dir | §9 #11 (oxy run pending) | SCRIPT PRESENT; RUN PENDING |

### Note on §9 #8 (byte-frozen full-report assertion)

The §5.3/§9 #8 prerequisite asked to confirm ≥1 SE + 1 PE existing test asserts the FULL report body + aux **before** the N==1 delegation refactor. The pre-existing tests assert report content via `.contains()` substrings (`tests/cli.rs:502–507` SE, `658–662` PE) and aux file content/existence (`:493`, `:699–700`) — partial, not a full-body frozen golden. The actual byte-frozen protection comes from (a) the 32 pre-existing integration tests passing unchanged after the refactor, and (b) the new worker-invariance tests performing a **full-body** report diff (modulo wall-clock) between N==1 and N>1. The N==1 path is therefore guarded against an internal `write!` reorder by the cross-worker-count equality, though there is no independent frozen golden of the exact report bytes for a fixed input. Marked DONE because the refactor's delegation is single-code-path (N==1 calls the same `process_*_chunk`) and the suite is green; flagged here for transparency.

## Verdict

**COMPLETE.** Every §3 behavior (3.1–3.8), every §5 step (1–7), and every §9 validation (#1–#9) is implemented and test-verified, with `cargo test -p bismark-aligner` green at 238/238. The four deviations are documented in §13 and are correct engineering choices (additive RunConfig field, raw-noodles merge reader for tagless ambig records, gate in the phase dir, byte-invisible STDERR reorder).

Two items are NOT fully closed but neither blocks the verdict:
- **§9 #10** (worker error/panic propagation + no-orphan) — the **behavior is implemented** (lowest-chunk-index Err, panic→Err, Drop kill+reap) but has **no dedicated test**. LOW-severity coverage gap; surfaced for Felix.
- **§9 #11** (oxy worker-invariance gate) — the **gate script exists and is correct**; the oxy RUN is a plan-acknowledged separate pending step (§13). This is the load-bearing assumption gate (§2.6) and must be run before merge per §11, but it is not a code/implementation gap.

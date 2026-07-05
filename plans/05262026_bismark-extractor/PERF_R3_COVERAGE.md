# Plan Coverage Report

**Mode:** B (code vs. plan)
**Plan(s):** `PERF_R3_DECODE_PLAN.md` (rev 1)
**Code:** worktree `/Users/fkrueger/Github/Bismark-extractor`, branch `perf-r3-fixed2-decode` @ `6f182f8`
**Net diff:** `git -C /Users/fkrueger/Github/Bismark-extractor diff b2af4e5 -- rust/` (274 lines; `parallel.rs` +76/-, `parallel_phase_f.rs` +126/-35)
**Date:** 2026-05-30
**Verdict:** COMPLETE

## Summary

- Total items: 11 (7 ledger items, several with sub-parts)
- DONE: 10
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 0
- PENDING (non-gap, by plan): 1 — colossal byte-identity smoke (Validation #6), recorded as in-progress per task instructions

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | `DECODE_THREADS: NonZeroUsize = 2` const with rationale doc | Behavior #1 / Impl-outline #2 | DONE | `parallel.rs:114`; full doc-comment (oxy sweet-spot, 3/4 no gain, decoupled from `--parallel`, parallels `GZIP_COMPRESS_THREADS`). `NonZeroUsize::new(2).unwrap()` const-form per rev-1 [A-O3]. |
| 2 | `ProducerReader` enum {Any, Threaded} + `header()`/`records()` | Behavior #3 / Impl-outline #3 | DONE | `parallel.rs:397-415`. `header() -> &noodles_sam::Header`; `records() -> Box<dyn Iterator<Item = Result<BismarkRecord, BismarkIoError>>>`. |
| 3 | Reader selection: BAM→`ThreadedBamReader::from_path(input, DECODE_THREADS)` always (no n_workers gate); SAM/CRAM→`open_reader`; `is_bam` via `AlignmentKind::from_path` | Behavior #2 / Impl-outline #4 | DONE | `is_bam` at `:224` (`AlignmentKind::from_path`); selection at `:246-250`, unconditional for BAM. |
| 4 | `producer_loop` takes `ProducerReader`; module doc updated | Behavior #4 / Impl-outline #6,#7 | DONE | `producer_loop` param `mut reader: ProducerReader` at `:430-431`; module doc updated `:11-12` (notes BAM threaded-decode path). Header call sites `:251` (chr_table) + `:260` (provenance log) unchanged via `ProducerReader::header()`. |
| 5 | Floor-at-2 fallback (`config.parallel.max(if is_bam {2} else {1})`) — pre-authorized per Validation #7 | Validation #7 (pre-authorized) | DONE (as planned) | `n_workers` at `:251` (`config.parallel.max(if is_bam { 2 } else { 1 })`). Comment documents the `--parallel 1` BAM measure-miss (~18.5 s) and notes byte-identity preserved across worker counts. Matches the plan's pre-authorized fallback — NOT an unplanned deviation. |
| 6a | Test `sam_input_matches_bam_through_r3_dispatch` (SAM dispatch else-arm) | Validation #3 | DONE | `parallel_phase_f.rs`; compares CpG/CHG/CHH split files BAM (threaded) vs SAM (Any). PASS. |
| 6b | `se_directional_records` shared fixture | rev-1 [B-I4] support | DONE | `parallel_phase_f.rs`; 5 SE-directional records shared by BAM + SAM writers. |
| 6c | `bismark-io::threaded_bam_reader_preserves_record_order` (ordering guard) | Validation #2 / rev-1 [B-I2] | DONE | `bismark-io/tests/integration_fixture_bam.rs:261`; 203-record real Perl BAM, `worker_count=4`, asserts qname order == single-threaded. PASS. |
| 7a | Coord-sort rejection via `from_path` (not `from_path_without_sort_check`) | Behavior #5 / Validation #4 | DONE | `ThreadedBamReader::from_path` (`read.rs:318`) calls `check_not_coordinate_sorted` (`read.rs:326`); shared `check_not_coordinate_sorted` unit-tested at `read.rs:899`. Code uses `from_path` (`parallel.rs:247`). |
| 7b | In-repo byte-identity suite green (parallel_phase_f N=1/N=4) + clippy/fmt | Validation #1, #5 | DONE | `parallel_phase_f` = 19 passed; `lib` = 105 passed; all extractor binaries green (see test table). clippy/fmt per main-session report. |
| 7c | Colossal byte-identity smoke SE+PE plain AND --gzip | Validation #6 | PENDING (not a gap) | Recorded as in-progress per task instructions; binding gate runs off-repo. |

## Test verification

| Test name | File | Status |
|-----------|------|--------|
| `parallel_phase_f` (whole binary, incl. N=1/N=4 byte-identity + 8199-record multibatch) | `tests/parallel_phase_f.rs` | PASS (19/19) |
| `sam_input_matches_bam_through_r3_dispatch` | `tests/parallel_phase_f.rs` | PASS |
| `bismark_extractor` lib unit tests | `src/lib.rs` | PASS (105/105) |
| `threaded_bam_reader_preserves_record_order` | `bismark-io/tests/integration_fixture_bam.rs:261` | PASS |
| All other extractor test binaries (se_phase_b_smoke, etc.) | `tests/*.rs` | PASS |

Full suite: `cargo test -p bismark-extractor` — every binary `ok`, 0 failed. Counts match plan expectation (`parallel_phase_f` 19, `lib` 105).

## Gaps (detail)

None. All Behavior, Implementation-outline, and Validation items are implemented as specified.
The floor-at-2 worker fallback is explicitly pre-authorized by the plan (Validation #7 HARD
gate + Assumptions section: "if it fails, the pre-authorized `config.parallel.max(2)` BAM-worker
floor applies"), and the code comment documents the measured `--parallel 1` miss (~18.5 s) that
triggered it. This is DONE-as-planned, not a deviation.

## Verdict

**COMPLETE.** Every Behavior item (1-5), every Implementation-outline step (1-7), and the
in-repo Validation items (#1-#5, #7) are present in the code and pass their tests. The only
outstanding plan item is the colossal byte-identity smoke (Validation #6), which the task
instructions direct to record as pending/in-progress rather than as a coverage gap. No code
changes are required for plan coverage.

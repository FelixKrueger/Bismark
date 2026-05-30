# Plan — R3 productionized: fixed 2-thread parallel BGZF decode (#884)

## Goal
Make BAM decode use a **fixed 2-thread `MultithreadedReader`, always-on, decoupled from
`--parallel`**, capturing the ~12–14 % wall-clock win on the real `.txt` path (and ~30 % on the
decode floor) measured on oxy. Crucially, the **default `--parallel 1` must benefit** — the gap in
the original #887 (which tied decode threads to `--parallel` and fell back to a single-threaded
reader at N=1, so the default got nothing).

## Revision history
- **rev 1 (2026-05-30):** Folded dual plan-review (`PERF_R3_DECODE_PLAN_REVIEW_reviewer-{A,B}.md`).
  Both APPROVE; both independently verified the byte-identity linchpin in the noodles source
  (A also trial-applied the edits → 18/18 tests + clippy clean). Changes:
  - **`--parallel 1` win [both flagged; B Critical] — RESOLVED: measure-first + pre-authorized
    floor-at-2 fallback.** Ship `--parallel N = N` workers + fixed 2 decode threads; Validation #5
    is now a HARD gate — if `--parallel 1` plain doesn't reach ~17.9s, apply `config.parallel.max(2)`
    on the BAM path and re-measure before merge (pre-authorized).
  - **[both] Validation #2** mis-cited a coord-sort test — it lives in `bismark-io`
    (`read.rs` `check_not_coordinate_sorted`, unit-tested ~`:899`), not the extractor. Re-cited.
  - **[B-I2] Multi-block ordering guard:** the extractor's 8199-record fixture may collapse to ≤2
    BGZF blocks → add `bismark-io::threaded_bam_reader_preserves_record_order` (203-record real
    BAM, worker_count=4) to the validation as the real parallel-inflate-ordering guard.
  - **[B-I4] Add an extractor-level SAM-input test** to guard the new BAM-vs-SAM/CRAM dispatch.
  - **[A-O3]** `NonZeroUsize::new(2).unwrap()` compiles as a const on rustc 1.95 — drop the hedge.
  - Line numbers post-R1-batching are ~`:226` (reader open) / `:235` (header) / `:442`
    (`producer_loop`), not the rev-0 `:210/:211/:365`.

## Context

**Measured (oxy, 10M PE, idle, 3 reps earlier + a focused trial):**
| config | wall | CPU-cores |
|---|---:|---:|
| current (single-thread decode) `--mbias_only` | 18.8s | 1.8 |
| current plain `.txt` | ~20.0s | 2.7 |
| **2 decode threads** `--mbias_only` | **13.0s** | 3.1 |
| 3 / 4 decode threads `--mbias_only` | 13.3 / 13.8s (no gain, slight regress) | ~3.0 |
| **2 decode threads** plain `.txt` | **17.9s** (→17.4 at 4) | 3.2 |

So: decode+extract (~18.8s) is the wall (writes only ~1s on fast disk); 2 decode threads is the
**sweet spot** (3/4 add nothing). Net real-path win ≈ 12–14 %.

**Prior art:** #887 (`d3dd289`, branch `perf-r3-parallel-decode`, CLOSED) wired
`ThreadedBamReader` into the producer at `n_workers >= 2 && is_bam`, with a single-threaded
fallback at N=1 — so `--parallel 1` (the default) got no benefit. d3dd289 was **`parallel.rs`-only
(+50/−3)**. We adapt it with two design changes: **always-BAM** (drop the `n_workers >= 2` gate)
and **fixed 2 threads** (a const, not `n_workers`).

**Already present in current `b2af4e5` — no new dependency:**
- `bismark_io::ThreadedBamReader` (`read.rs:305`, exported `lib.rs:33`):
  `from_path(path, parallel: NonZeroUsize)` → `noodles_bgzf::io::MultithreadedReader::with_worker_count`
  (`read.rs:318/323`), and `from_path_without_sort_check` (`read.rs:332`). We use `from_path`
  (keeps the coordinate-sort rejection).
- `AlignmentKind::from_path` (BAM/SAM/CRAM classification).
- Current `parallel.rs`: `run_pipeline` opens via `open_reader(input, None)` (`:210`), uses the
  header at `:211`/`:220`; `producer_loop` takes `AnyReader` (`:365`). These are the exact
  adaptation points (identical to d3dd289's "before").

## Behavior
1. New const `DECODE_THREADS: NonZeroUsize = 2` (doc: oxy-measured sweet spot; 3/4 add nothing;
   decoupled from `--parallel` so the default benefits — same rationale shape as
   `output.rs::GZIP_COMPRESS_THREADS`).
2. In `run_pipeline`, classify input via `AlignmentKind::from_path(input)`:
   - **BAM** → `ProducerReader::Threaded(ThreadedBamReader::from_path(input, DECODE_THREADS))`
     — **always**, regardless of `--parallel`.
   - **SAM / CRAM** (not BGZF) → `ProducerReader::Any(open_reader(input, None))` (single-threaded, unchanged).
3. `ProducerReader` enum `{ Any(AnyReader<…>), Threaded(ThreadedBamReader) }` with `header()` +
   `records() -> Box<dyn Iterator<Item = Result<BismarkRecord, BismarkIoError>>>` (verbatim from d3dd289).
4. `producer_loop` takes `ProducerReader` instead of `AnyReader`.
5. **Edge:** `ThreadedBamReader::from_path` keeps the same coordinate-sort rejection as `open_reader`
   (bismark BAMs are read-ordered; coord-sorted breaks PE adjacent-pairing). Use `from_path`, NOT
   `from_path_without_sort_check`.

## Implementation outline (`parallel.rs` only — adapt d3dd289)
  *(Line numbers below are post-R1-batching, b2af4e5 — verify at impl time; rev-0 cited stale ones.)*
1. Imports: extend the `bismark_io` use with `AlignmentKind, BismarkIoError, ThreadedBamReader`.
2. Add `const DECODE_THREADS: std::num::NonZeroUsize = NonZeroUsize::new(2).unwrap();` with the
   rationale doc-comment. (Confirmed const-evaluable on rustc 1.95 — no `match`/`expect` hedge needed.)
3. Add the `ProducerReader` enum + `impl` (`header`, `records`) — copy from d3dd289.
4. Replace `let reader = open_reader(input, None)?;` (~`:226`) with the `is_bam` selection:
   `AlignmentKind::from_path(input)? == Bam` → `ProducerReader::Threaded(ThreadedBamReader::from_path(
   input, DECODE_THREADS))`, else `ProducerReader::Any(open_reader(input, None)?)`. **Always** threaded
   for BAM (NO `n_workers` gate); worker count stays `--parallel` (floor-at-2 only if Validation #7 fails).
5. `reader.header()` call sites (~`:235` chr_table, header-provenance log) work unchanged via
   `ProducerReader::header()`.
6. Change `producer_loop`'s `reader` param type (~`:442`) `AnyReader<…>` → `ProducerReader`.
7. Refresh the module doc (~`:11` "drives `open_reader().records()`") to note the BAM threaded-decode path.

## Efficiency
- +1 decode worker thread always-on → ~+1 CPU-core (1.8→3.1 on `--mbias_only`; 2.7→3.2 on plain).
  Bounded; acceptable. Capped at 2 (3/4 measured no-gain — do NOT scale with `--parallel`).
- Decode floor 18.8→13.0s; plain `.txt` ~20→~17.9s (~12 %). Makes Rust ≈ 7–8× over Perl's best
  (vs ~5× today).
- Memory: `MultithreadedReader` holds a small bounded set of in-flight BGZF blocks.

## Integration
- BAM decode parallelized; **record order preserved** (MultithreadedReader decompresses blocks in
  parallel but emits records sequentially) → byte-identity to the single-threaded reader holds.
- SAM/CRAM, M-bias, splitting-report, split-file output, gzip (R2), empty-sweep — all unchanged.
- The byte-identity tests now exercise the threaded reader at **all** N (incl. N=1), since BAM
  always uses it.

## Assumptions
- **[VERIFIED rev 1 — both reviewers, in noodles 0.47.0 source]**
  `MultithreadedReader::with_worker_count(2)` yields records in the SAME order as the
  single-threaded `bgzf::Reader` (reader thread enqueues per-block one-shot receivers FIFO in frame
  order; consumer pops FIFO + blocks per block → inflate parallel, emission strictly file-order) →
  byte-identical output. Still gated by the byte-identity suite (now runs the threaded path at N=1)
  + the bismark-io ordering test.
- `ThreadedBamReader::from_path` applies the same `check_not_coordinate_sorted` as `open_reader`
  (shared code) → identical coord-sort rejection.
- **1 worker (`--parallel 1`) + 2 decode threads reaches ~17.9s** — i.e. the extract worker is not
  the bottleneck (CPU probe showed workers idle / decode-bound). **NOT directly measured (the trial
  was 2 workers + 2 decode).** Resolution: Validation #7 is a hard gate; if it fails, the
  pre-authorized `config.parallel.max(2)` BAM-worker floor applies → "default benefits" holds either way.
- `AlignmentKind::from_path` classifies BAM correctly (magic/extension) for the gating.

## Validation
1. `cargo test -p bismark-extractor` byte-identity suite — esp. `parallel_phase_f`
   legacy(single-threaded `extract_se/pe`) vs parallel at **N=1 and N=4** (parallel now uses the
   threaded BAM reader at all N) → must be byte-identical; + the 8199-record multibatch test.
2. **Ordering guard (rev 1, B-I2):** the in-repo guarantee that parallel inflate preserves record
   order is `bismark-io::threaded_bam_reader_preserves_record_order` (203-record real Perl BAM,
   worker_count=4) — confirm it passes (`cargo test -p bismark-io`). The extractor's 8199-record
   `BamWriter` fixture may collapse to ≤2 BGZF blocks, so it is NOT a robust parallel-inflate
   ordering test on its own. *(If cheap, also add an extractor fixture that spans ≥3 BGZF blocks.)*
3. **SAM-input dispatch (rev 1, B-I4):** add/confirm an extractor-level test that **SAM** input
   still runs end-to-end (exercises the new `is_bam ? Threaded : Any` branch's else-arm — SAM must
   NOT take the threaded path). CRAM stays end-to-end-unsupported as today.
4. **Coord-sort rejection** is enforced by the SHARED `check_not_coordinate_sorted` in
   `ThreadedBamReader::from_path` == `open_reader` (unit-tested in `bismark-io` `read.rs` ~`:899`;
   there is NO extractor-level coord-sort test — do not claim one). Confirm `from_path` (not
   `from_path_without_sort_check`) is used.
5. `cargo clippy --all-targets -- -D warnings` + `cargo fmt --check`.
6. Colossal `phase_h_smoke` SE+PE, **plain AND `--gzip`** — binding byte-identity gate.
7. **Perf re-measure (oxy) — HARD GATE (rev 1):** `--parallel 1` plain `.txt` must drop to
   **~17.9s** (from ~20s) ⇒ the default benefits; `--mbias_only` ≈ 13s. **If `--parallel 1` does
   NOT reach ~17.9s** (single extract worker bottlenecks), apply the **pre-authorized fallback**:
   floor BAM extract workers at 2 (`config.parallel.max(2)` on the BAM path — `std::thread`
   workers, no N=1 deadlock) and re-measure before merge. Record vs the ~20s baseline.

## Questions or ambiguities
- (Open, non-critical) `DECODE_THREADS` as a fixed const vs a hidden tuning flag/env — recommend
  fixed const (matches `GZIP_COMPRESS_THREADS`); revisit only if a future workload wants it.
- (Open, non-critical) SAM/CRAM stay single-threaded (not BGZF) — correct, no threaded path there.
- No **Critical** ambiguities: mechanism + adaptation points + sweet-spot value are all measured/known.

## Self-Review
- **Logic:** the only functional change is the reader behind the producer; the pipeline, ordering
  (`batch_seq`), and all output paths are untouched. ✓
- **Edge cases:** empty BAM / single-record BAM (MultithreadedReader handles); coord-sorted BAM
  (from_path rejects); SAM/CRAM (AnyReader path); `--mbias_only` (no writes — still decodes via
  threaded reader, biggest beneficiary). ✓
- **Byte-identity risk** (order preservation) is THE risk — mitigated by the existing suite now
  running the threaded path at N=1 + the colossal smoke. If any test diverges, STOP (do not
  loosen the assertion). ✓
- **Efficiency:** capped at 2 (measured); always-on cost is ~+1 core — documented, acceptable. ✓
- **Remaining risk:** the "1 worker + 2 decode threads ≈ 17.9s" assumption — if `--parallel 1`
  doesn't realize the win (extract-bound at 1 worker), the default wouldn't benefit and we'd
  reconsider (e.g. floor workers at 2). Flagged as the key perf-gate (Validation #5).

## Implementation notes (2026-05-30 — IMPLEMENTED, all gates GREEN)
**Shipped on branch `perf-r3-fixed2-decode`, `parallel.rs` + `tests/parallel_phase_f.rs` only:**
- `40f3670` — always-on fixed-2-thread parallel BGZF decode for BAM (`DECODE_THREADS = 2` const,
  decoupled from `--parallel`; `ProducerReader { Any, Threaded }` enum; SAM/CRAM keep the
  single-threaded reader; `sam_input_matches_bam_through_r3_dispatch` test + `se_directional_records`
  shared fixture).
- `6f182f8` — floor BAM extract workers at 2 (`config.parallel.max(if is_bam {2} else {1})`): the
  **pre-authorized fallback fired** (see Deviation below).

**Validation results (mapped to the Validation list above):**
- **#1 byte-identity suite — GREEN.** `cargo test -p bismark-extractor`: `parallel_phase_f` **19/19**,
  lib **105**. Legacy vs parallel byte-identical at N=1 *and* N=4 (BAM now uses the threaded reader at all N).
- **#2 ordering guard — PASS.** `bismark-io::threaded_bam_reader_preserves_record_order` (203-record
  real BAM, worker_count=4).
- **#3 SAM-input dispatch — PASS.** Added `sam_input_matches_bam_through_r3_dispatch` (SAM takes the
  `Any` else-arm, not the threaded path).
- **#4 coord-sort rejection — confirmed.** `from_path` (shared `check_not_coordinate_sorted`), not
  `from_path_without_sort_check`.
- **#5 clippy `-D warnings` + `fmt --check` — clean.**
- **#6 real-data smoke — PASS (all 4).** oxy `phase_h_smoke` real 10M, SE+PE × {default, gzip},
  finished 2026-05-30 12:39:58Z: every mode `exit=0`, all 8 files match (2 raw-identical +
  6 sorted-equivalent). This is the multi-block parallel-inflate ordering proof the single-block unit
  fixtures structurally cannot give.
- **#7 perf re-measure (HARD gate) — PASSED, with the floor-at-2 fallback.** Final (oxy 10M PE):
  `--parallel 1` plain `20.0→17.6 s`, `--mbias_only` `18.8→12.3 s`.

**Deviation from plan — the pre-authorized floor-at-2 fallback fired (Validation #5/#7).** Without
the floor, `--parallel 1` reached only ~18.5 s plain / ~16 s `--mbias_only` — a single extract worker
cannot drain 2 decode threads, exactly the contingency both plan-reviewers flagged and Felix
pre-authorized ("measure-first + floor fallback"). Applied `config.parallel.max(2)` on the BAM path
(`6f182f8`) and re-measured → the default benefits fully. The floor is **byte-identity-invariant**
(output reordered deterministically by `batch_seq` regardless of worker count), so it changes timing
only, never bytes.

**Reviews:** dual code-review **APPROVE** (no Critical/High/Medium; one Low — A-Low-2: normalize the
three perf-number comment blocks in `parallel.rs` to one consistent story — addressed). plan-manager
**COMPLETE** (0 gaps).

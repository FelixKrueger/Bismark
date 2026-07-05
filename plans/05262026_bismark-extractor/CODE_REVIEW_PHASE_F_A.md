# Code Review — Phase F (Reviewer A)

**Reviewer:** A (independent, fresh context)
**Target:** `extractor-phase-e` branch, Phase F implementation (producer/worker/collector pipeline for `--multicore N`).
**Files reviewed:**
- `rust/bismark-extractor/src/parallel.rs` (NEW, ~940 LOC incl. tests)
- `rust/bismark-extractor/src/mbias.rs` (`add` helper + 4 tests)
- `rust/bismark-extractor/src/output.rs` (`SplittingReport::add` + 1 test)
- `rust/bismark-extractor/src/route.rs` (`compute_yacht_columns` refactor)
- `rust/bismark-extractor/src/main.rs::run` (dispatch to `extract_*_parallel`)
- `rust/bismark-extractor/src/lib.rs` (re-exports + module wiring)
- `rust/bismark-extractor/Cargo.toml` (rayon + crossbeam-channel deps, version bump)
- `rust/bismark-extractor/tests/parallel_phase_f.rs` (15 byte-identity tests)
- `rust/bismark-extractor/tests/se_phase_b.rs` (one renamed test)

**Verdict:** **Solid** — the byte-identity invariant is well-defended, the deviation from the rev 1 plan (rayon → std::thread::spawn) is sound and clearly documented, and the 15-test parallel suite covers the load-bearing surface. I have **0 Critical** correctness findings against the byte-identity goal. The issues below are concurrency/perf/maintainability concerns that won't gate Phase F merge but should be tracked.

---

## Summary

Phase F replaces the (planned but never implemented in Rust) fork+modulo Perl model with a 3-actor pipeline: **producer** (reads BAM, assigns monotonic `input_idx`) → **N workers** (extract calls, accumulate per-worker M-bias + SplittingReport deltas, route per-call) → **collector** (BTreeMap reorder, single-writer to OutputFileMap, lowest-idx Err selection). EOS is signalled by sender-side channel-disconnect (no sentinel messages). The byte-identity tests at N=1/2/4/8 pass, mode coverage is comprehensive (Default / Comprehensive / MergeNonCpG / Yacht / MbiasOnly / Gzip), and error-propagation paths (invalid XM, orphan PE record, mbias_only silence) are exercised at N=4.

The implementer's deviation from the rev 1 plan — using `std::thread::spawn` instead of `rayon::ThreadPool::scope` for workers — is correct and well-reasoned. The rayon-scope-deadlock-at-N=1 they describe in the module docstring is real (scope() consumes a pool thread for its closure). Forward-looking work that wants rayon (e.g. par_iter over per-mode collector files) can still pull it in.

---

## Critical

*(none — byte-identity holds across N for every test, error selection is deterministic, no double-counting, EOS protocol is watertight under the failure modes I traced)*

---

## High

### H1. Persistent write errors trigger O(N) wasted `update_best_err` calls + ignored downstream writes

**Location:** `parallel.rs:766-783` (`collector_loop` inside `Ok(WorkerOutput::Ok)` arm).

**Issue:** When `write_routed_call` fails (e.g., disk full), the collector records the error in `best_err` and **continues draining**. The next call also fails, `update_best_err` is called again (no-op because `best_err` is locked to the lower-idx), and so on. For a disk-full mid-extraction this loops over every subsequent call in every subsequent record until producer EOS.

**Impact:** Performance only on already-erroring runs; not correctness. But on a 100M-record BAM with a disk-full at idx=5, the collector burns wall-clock + I/O on N×millions of failed write attempts before reporting the original error.

**Why not Critical:** byte-identity isn't affected (`cleanup_partial_outputs` removes the files regardless of how many bytes were written before/after the error).

**Recommendation:** Short-circuit when `best_err.is_some()` — skip the `write_routed_call` call but keep advancing `next_emit_idx` to drain the reorder buffer.

---

### H2. Worker panic is silently swallowed when ANY other (non-panic) error is present

**Location:** `parallel.rs:241-257` (post-collector join sequence).

**Issue:** `merge_results` prefers `collector_result.Err` over a producer panic. The worker-panic join loop below only synthesises an `InternalError` if `worker_panic.is_none()` **after** `merge_results` returns Ok. So if a real error came back via `best_err` (e.g., one record had invalid XM) **and** a different worker panicked (e.g., on an indexing bug), only the real error is reported — the panic vanishes.

**Impact:** Future-debugging hazard. The byte-identity contract isn't violated (the panic was caused by a real input that the legacy path would also have errored on, just differently), but a panic indicates a Rust-level bug that should surface even if a less-interesting user error is also present.

**Recommendation:** Even when `pipeline_result` is `Err(non_panic)`, also `eprintln!` the worker panic message if any. Or build a multi-error type. Or — simpler — promote `worker_panic` precedence over `best_err` for `InternalError` shapes only.

---

### H3. Reorder buffer is unbounded in the worst case

**Location:** `parallel.rs:761-784` (`reorder_buf: BTreeMap<u64, Vec<RoutedCall>>`).

**Issue:** If worker 0 is slow on `input_idx=0` (e.g., a record with thousands of XM bytes), workers 1..N-1 race ahead. They push `Ok` messages for idx=1..K into `tx_output`. Collector drains the output channel into `reorder_buf` but cannot emit anything until idx=0 lands. The output channel is N×8 deep, but the **reorder buffer is unbounded** — it can hold all of producer-EOS-worth of post-idx-0 work.

**Quantitative bound:** worst case = (input_channel_size + N in-progress + output_channel_size + reorder_buf_max). Input channel is N×32 and producer blocks on full, so reorder_buf is bounded by `(N×32 + N + N×8) = 41N + ...` messages **at steady state**. But if the slow record stalls **after** the producer has read everything (file fits in OS cache), the reorder buffer can grow to `total_records - 1` entries.

**Why High, not Critical:** byte-identity isn't affected and on typical inputs (10M reads, per-record μs latency) this won't happen. But the upcoming Phase H real-data test (55.7M PE reads) may hit it if any record is pathological (e.g., super-long CIGAR).

**Recommendation:** Document the memory model in the module docstring (currently underspecified). Optionally add a soft-cap that signals back-pressure (e.g., if `reorder_buf.len() > THRESHOLD`, sleep briefly to let the slow worker catch up). Not blocking for Phase F.

---

## Medium

### M1. `rayon` is a declared dependency but unused

**Location:** `Cargo.toml:33-35` + `parallel.rs:42-60` (deviation docstring).

**Issue:** The plan committed to rayon. The implementer correctly identified the rayon-scope-deadlock and switched to `std::thread::spawn`. But the rayon dep stays in `Cargo.toml` "because the plan committed to it and a future Phase F polish (e.g. `par_iter` over the per-file collector workers) may need it." `grep -rn "use rayon\|rayon::"` shows zero non-comment references to rayon.

**Impact:** Cargo compile time + binary size. ~30 transitive crates pulled in for nothing.

**Recommendation:** Remove the rayon dep. If Phase F polish later wants `par_iter`, add it then — it's a 3-line Cargo.toml change. Speculative deps are noise.

---

### M2. `qname_arc_for(record)` allocates under `--mbias_only` even though no `RoutedCall` is built

**Location:** `parallel.rs:553, 652, 653` (process_se + process_pe).

**Issue:** Each call to `qname_arc_for` does `Arc::from(bytes)`, which allocates an Arc + copies the QNAME bytes. Under `--mbias_only` the loop body skips RoutedCall construction (`if mbias_only { continue }`), so the Arc is never used.

**Impact:** One extra heap allocation + copy per SE record (or per pair × 2 mates for PE) when running `--mbias_only`. For 100M PE records that's ~200M wasted alloc+free. Pure waste.

**Recommendation:** Guard the Arc construction:
```rust
let qname_arc: Arc<[u8]> = if mbias_only {
    Arc::from(&[][..])  // sentinel; never read
} else {
    qname_arc_for(record)
};
```
Or move the Arc construction inside the `if !mbias_only` branch of the call loop.

---

### M3. `mbias_only_silence` and `mbias_only` are identical bindings — confusing duplication

**Location:** `parallel.rs:432-433`:
```rust
let mbias_only_silence = config.is_mbias_only();
let mbias_only = config.is_mbias_only();
```

**Issue:** Two named bindings to the same value, passed as separate args through process_se/process_pe. The names imply they're conceptually distinct (one silences `InvalidXmByte`, the other gates RoutedCall emission), but in this codebase they're identical.

**Impact:** Code clarity. A future reader has to verify whether they're truly identical or if it's a bug. The function signatures `process_se(... mbias_only_silence: bool, mbias_only: bool, ...)` are confusing.

**Recommendation:** Collapse to a single binding `mbias_only`, drop one of the `bool` args from `process_se` / `process_pe`. If the two ever do need to diverge, reintroduce both at that point.

---

### M4. No early-termination on first error — full BAM scan even when idx=0 errors

**Location:** `parallel.rs:233` (collector return) + producer loop.

**Issue:** Legacy `extract_se` halts at the first error. Parallel pipeline reads the entire BAM even if idx=0 errors. For a 100M-record BAM that's gigabytes of wasted I/O and CPU.

**Impact:** Performance only. Byte-identity is unaffected (cleanup_partial_outputs removes the files; the bytes written by post-error records are discarded).

**Recommendation:** Add a "stop flag" `Arc<AtomicBool>` that the collector sets when it observes an Err. The producer checks this flag at each `next()` iteration and bails out. Same for workers (skip processing if flag set). Phase F is feature-complete without this, so it can be a follow-up perf optimisation.

---

### M5. Worker thread leak on producer-spawn failure

**Location:** `parallel.rs:222-229`.

**Issue:** If `std::thread::Builder::new()...spawn(...)` fails for the producer, the function returns `Err(InternalError)` immediately. By this point N worker threads are already running (spawned at lines 199-214) and `rx_input`, `tx_output` have been dropped from the main thread. The worker JoinHandles are also dropped (since the `Vec<JoinHandle>` goes out of scope). The workers will eventually exit cleanly when the main thread's `tx_input` is dropped at function-return — but we never **join** them, so any panic in cleanup is lost.

**Impact:** Edge case (thread spawn failure is rare). The workers do exit on their own via channel-disconnect. Minor resource-leak concern in test scenarios that might depend on full cleanup.

**Recommendation:** On producer-spawn failure, drop `tx_input` first to force worker disconnect, then join the worker handles. Not blocking.

---

## Low

### L1. Stale "PE arrives in Phase C" error message persists in `process_se`

**Location:** `parallel.rs:525-530`:
```rust
return Err(BismarkExtractorError::PhaseNotYetImplemented {
    feature: "paired-end extraction (input has PAIRED flag set); \
              PE arrives in Phase C"
        .to_string(),
});
```

**Issue:** This message references Phase C, which shipped 2 phases ago (commit 75d11e9). The text is misleading after PE support has been merged. Legacy `extract_se` has the same stale text (so this isn't a Phase F regression — Phase F faithfully copied it).

**Recommendation:** Update both sites (`parallel.rs:525` and `pipeline.rs:98-102`) to something like: `"SE pipeline received a record with the PAIRED flag set — pass --paired-end or rely on auto-detect"`. Pure cosmetic, low priority.

---

### L2. `n_workers = config.parallel.max(1)` is defensively redundant

**Location:** `parallel.rs:182`.

**Issue:** `Cli::validate` (cli.rs:404-407) rejects `--parallel 0`. So `config.parallel` is guaranteed `>= 1`. The `.max(1)` is defensive but adds nothing.

**Recommendation:** Add a debug_assert or a comment, OR drop the `.max(1)` and let the validation be the single source of truth. Lowest priority.

---

### L3. Documentation for the reorder-buffer memory bound is missing from `parallel.rs` module docs

**Location:** `parallel.rs:1-60` (module docstring).

**Issue:** The module docs explain the pipeline architecture and byte-identity invariants beautifully. They mention "bounded MPMC channels (N×32 / N×8)" but **don't quantify the reorder-buffer growth** — which is the only unbounded buffer in the pipeline (see H3). A reader trying to reason about worst-case memory will miss it.

**Recommendation:** Add a paragraph under "Byte-identity invariant" or a new "Memory model" section that notes: reorder_buf is bounded above by `total_records - next_emit_idx` and in practice by the slow-record latency. Cheap docs work.

---

### L4. `normalize_report` test helper strips path-dependent lines — verify it doesn't mask real diffs

**Location:** `tests/parallel_phase_f.rs:262-278`.

**Issue (Brief item 10):** The helper strips `Input file:` and `Output directory:` lines before comparing splitting reports. This is **correct** for the helper's purpose (the legacy and parallel runs use different temp dirs by design), but it's worth verifying the stripping is narrowly scoped — only those two prefixes, never anywhere else. I traced the helper: it strips lines whose **leading** characters match the prefixes. The splitting report writes those lines verbatim once (output.rs:321-322), no other line begins with `Input file:` or `Output directory:`, so the stripping is correctly scoped.

**Recommendation:** None — flagged for transparency. Note for Phase H authors: real-data byte-identity (Phase H §X) uses the **same** output_dir for both N=1 and N=K runs (snapshot the first, then run the second over a copy), so this normalisation helper is test-harness-specific and won't shadow a Phase H gate.

---

### L5. `Box<BismarkPair>` boxing in `WorkerInput::Pe` is fine but undocumented at the call site

**Location:** `parallel.rs:97-101` (struct def comment ✓) + `parallel.rs:406` (`Box::new(pair)` at the producer).

**Issue:** The Box is correctly placed (silences `clippy::large_enum_variant`). The struct comment justifies it. But the producer's `Box::new(pair)` allocation cost — one per pair — is not surfaced. For 100M pairs that's 100M heap allocations. The worker receives the box transparently via `&pair` in `process_pe`.

**Impact:** Minor perf — Box of BismarkPair is small (~200 bytes worst-case) so the allocation overhead is negligible vs the BAM-read I/O. Not worth optimising.

**Recommendation:** None. Flagged for awareness.

---

## What the test suite covers well

- ✅ Legacy vs parallel N=1, N=4 (SE + PE) — direct byte-identity oracle.
- ✅ Cross-N (1/2/4/8 SE; 1/4/8 PE) — confirms order-independent merging.
- ✅ Every output mode (Default, Comprehensive, MergeNonCpG, Yacht, MbiasOnly).
- ✅ Yacht reverse-strand polarity at N=4 (the Critical-1 invariant from Phase E carries through).
- ✅ Gzip at N=4 decompressed-equals-plain.
- ✅ Error propagation: invalid XM at N=4, orphan PE at N=4, mbias_only silence at N=4.
- ✅ Edge case: empty BAM produces header-only files.
- ✅ N=1 belt-and-suspenders (`parallel_n1_via_extract_se_parallel_matches_legacy_extract_se_pe`).

## What's missing (test-coverage gaps)

- ❌ **Worker panic propagation** — no test simulates a worker panic. The H2 finding (panic swallowed when other error exists) wouldn't be caught.
- ❌ **Producer panic propagation** — no test simulates a producer panic.
- ❌ **Multi-error byte-identity** — no test injects 2+ errors at known input_idx to verify lowest-idx wins. The `update_best_err` unit tests are good but they don't cover the end-to-end pipeline.
- ❌ **Stress / property test** — no randomised input at N=8/16 to shake out scheduling-dependent bugs.
- ❌ **Producer-spawn failure** (M5) — untestable without unsafe injection but the cleanup path is unexercised.

For Phase H the property-test gap is worth closing (the byte-identity gate would benefit from a randomised oracle); for now the deterministic byte-identity-across-N tests cover the practical surface.

---

## Concurrency correctness — what I verified

I traced these invariants in detail:

1. **EOS protocol (producer → workers).** Producer returns ⇒ `producer_tx_input` drops ⇒ workers see `Err(RecvError)` ⇒ each emits exactly one FinalDelta ⇒ workers drop their `tx_output` clones ⇒ all `tx_output` eventually gone (main's clone dropped at line 218 already). ✓
2. **EOS protocol (workers → collector).** Collector exits when `finaldeltas_received >= n_workers`. If fewer FinalDeltas arrive (some worker exited without emitting one), the `Err(RecvError)` arm synthesises an `InternalError`. ✓
3. **Records-processed double-count concern (Brief item 4).** I verified worker increments report.records_processed (+1 SE / +2 PE) and collector ONLY calls `state.report.add(&report)` once per FinalDelta. No double-counting. The cross-N byte-identity tests catch this regardless. ✓
4. **Determinism of err selection (Brief item 1).** `update_best_err` keeps lowest input_idx; ties keep the first arrival. Tested directly (`update_best_err_picks_lowest_input_idx`, `update_best_err_equal_idx_keeps_existing`). Worker → collector ordering is racy but the **selection is order-independent** by construction. ✓
5. **Reorder buffer correctness on Err (my own probe).** When `WorkerOutput::Err` arrives at input_idx=K, no entry is inserted into `reorder_buf` for K. Subsequent Oks at K+1, K+2, ... accumulate in the buffer but `next_emit_idx` cannot advance past K, so they're never written to disk. On Err return, `cleanup_partial_outputs` removes any partial files. ✓ — matches legacy "halt + cleanup" semantics.
6. **`MbiasTable::add` / `SplittingReport::add` commutativity + associativity.** Tested directly (3 mbias tests + 1 report test). Sums use `u64::saturating_add` which is associative/commutative below saturation; saturation at u64::MAX is unreachable at realistic counts. ✓
7. **`compute_yacht_columns` factoring preserved legacy behaviour.** The legacy `route_call` now calls `compute_yacht_columns` instead of inline code; the inline code from Phase E was extracted verbatim into the new fn. The yacht-N4-byte-identical-to-legacy test confirms semantic equivalence. ✓
8. **Plan §2 worker-under-mbias_only emits empty `routed_calls`.** Verified at `parallel.rs:568-571` and `693-694`: the loop body `continue`s under `mbias_only`, so the `Vec<RoutedCall>` returned by process_se/process_pe is empty. M-bias and counter mutations still happen. ✓

---

## Recommendation

**Merge Phase F.** The implementation hits the rev 1 plan's byte-identity goal, the deviation is well-reasoned, and the test suite is solid for the load-bearing surface. None of the H/M findings block correctness against the byte-identity invariant.

**Track as follow-ups (do NOT block merge):**
- H1 (short-circuit collector writes on persistent error) — perf hardening
- H2 (worker panic visibility) — debugging hardening
- M1 (drop unused rayon dep) — cleanup, smallest diff
- M2 (skip Arc allocation under mbias_only) — perf
- M4 (early termination on first error) — perf hardening for Phase H

**Track as Phase H prerequisites:**
- Property-test / stress-test infrastructure for randomised inputs at high N.
- Worker panic propagation test.
- Multi-error byte-identity test (2+ errors at known idx).

**Reviewer A signing off.**

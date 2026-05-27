# Plan Review — Phase F (`bismark-extractor` `--multicore N`) — Reviewer B

**Plan file:** `plans/05262026_bismark-extractor/PHASE_F_PLAN.md` (rev 0, 2026-05-27)
**Reviewer:** B (independent of Reviewer A)
**Verdict:** Approve **with the Critical issues fixed before implementation**. The architecture is sound, the byte-identity invariant is articulated with the right primitives, and the SPEC alignment is good. But several gaps in the deadlock-safety story, a couple of API-shape mismatches with the existing `mbias.rs`/`output.rs` code, and a missed-opportunity rejection of the dedup-style simpler model need to be addressed.

---

## Logic review

### What's right

- **Three-stage pipeline shape** (producer / N workers / single collector) cleanly maps to SPEC §6.4 / §9.1.
- **`input_idx` per message + collector `BTreeMap` reorder** is exactly the canonical primitive for in-order emission from out-of-order workers. The math (40N max-in-flight, §4.4) is correct and within memory budget.
- **Per-worker M-bias delta + sum-merge at EOS** correctly exploits the commutativity/associativity of position-wise sums; the property is testable (and the plan plans to test it). Same for `SplittingReportDelta`.
- **PE pair formation in the producer** (not in workers) is the right call: it keeps workers pure-functional, avoids any shared mutable state across workers, and matches Phase C's adjacency assumption with no semantic change.
- **`compute_routed_call` factoring** (pure helper + side-effecting writer) is the right separation of concerns; the legacy `route_call` re-wrapping it preserves the single-threaded reference path.
- **Owned `qname`/`chr` in `RoutedCall`** is correct — references can't cross the channel once the source record is dropped on the worker side.
- **Keeping legacy `extract_se`/`extract_pe` as the byte-identity reference, exercised by the existing test suite**, is a defensible rev-0 architecture: drift risk exists, but the reference role gives those paths a continuing purpose.
- **Test surface includes byte-identity at N ∈ {1, 2, 4, 8}** for SE-default, PE-default, comprehensive, merge_non_CpG, yacht (with the Critical-1 col6/col7 regression guard), mbias_only, and gzip. That's the right cross-product.

### Logic gaps and inconsistencies

#### G1 (Critical) — Producer-crash → worker-deadlock vector is hand-waved

Plan §4.1 step 1 says "At end-of-input: sends `WorkerInput::EndOfStream { worker_count: N }` (N sentinels — one per worker) and closes." But the §12 R3 mitigation says **"producer always sends N EOS sentinels in a `Drop` impl on its sender side"** — this is not how `crossbeam_channel::Sender::drop` works. `Sender::drop` simply disconnects; it cannot synthesise N messages, because sending may block (the channel is bounded) and `drop` is not allowed to panic-unwind through a bounded send.

The real deadlock-safety mechanism for crossbeam channels is: when **all** `Sender` handles drop, `Receiver::recv()` returns `Err(RecvError)` (i.e. `Disconnected`). Workers MUST treat `Err(RecvError)` from the producer channel as a clean EOS — equivalent to receiving the `EndOfStream` sentinel — and emit their `FinalDelta` and exit.

**Required fixes:**
1. Remove the `Drop`-impl mitigation language from §12 R3 — it's misleading and structurally not what the code will do.
2. Pick exactly one EOS mechanism and state it precisely. Options:
   - **(A) Channel-disconnect-as-EOS**: workers loop `match rx.recv() { Ok(msg) => …, Err(_) => emit FinalDelta + break }`. Producer just drops the sender at end-of-input. No sentinel messages. Simpler, robust to producer panics.
   - **(B) Explicit N sentinels + disconnect**: producer sends N `EndOfStream` messages then drops. Workers treat `EndOfStream` OR `Err(RecvError)` identically. Belt-and-braces.

   Pick (A) — it's the standard idiom and structurally immune to "did the producer send exactly N sentinels?" off-by-one errors. The plan currently leans toward (B); please switch or justify.
3. Add an explicit test: `producer_panic_does_not_deadlock_workers`. Force the producer to panic mid-stream (e.g. via a synthetic reader that returns `Err` then panics on the next call). Assert all workers exit cleanly within a bounded time window.

#### G2 (Critical) — Producer thread join not specified

The plan never says where the producer thread is joined or how its panic/error is surfaced. `std::thread::spawn` returns a `JoinHandle<T>`; if it's never joined, the thread is detached and its panic is silent.

**Required fix:** specify the lifecycle precisely:
- Collector finishes normally OR encounters a worker error.
- Collector calls `producer_handle.join()`. On `Err(panic_payload)`, surface as `BismarkExtractorError::InternalError` (or re-panic — your choice, but pick one).
- On `Ok(producer_result)`, propagate any producer-side `Err` that wasn't already surfaced via the channel.

Similarly: the rayon `scope(|s| { for _ in 0..N { s.spawn(…) } })` guarantees worker joining at scope-exit. Document this explicitly so future contributors know the join is automatic.

#### G3 (Critical) — `MbiasTable::add` pseudocode does not match the existing API

The plan's §4.3 snippet calls `other.max_position(ctx)`, `self.ensure_capacity(ctx, other_max)`, `self.get(ctx, pos)`, `self.set(ctx, pos, …)` — but the actual `MbiasTable` (`src/mbias.rs`) has:

- `max_position(&self) -> u32` (no `ctx` parameter)
- direct `pub` fields: `cpg: Vec<MbiasPos>`, `chg: Vec<MbiasPos>`, `chh: Vec<MbiasPos>`
- no `ensure_capacity` / `get` / `set`

This is a plan-execution risk: implementing exactly what the pseudocode says will fail to compile and force a mid-implementation API-design detour. **Required fix:** rewrite the §4.3 snippet to operate on the actual fields (iterate `[cpg, chg, chh]` pairs of `(self_vec, other_vec)`, `if self_vec.len() < other_vec.len() { self_vec.resize(other_vec.len(), MbiasPos::default()) }`, then `for i in 0..other_vec.len() { self_vec[i].meth = self_vec[i].meth.saturating_add(other_vec[i].meth); … }`). Same shape, but matched to the real types.

#### G4 (Important) — `SplittingReport::add_delta` signature mismatch with existing types

§5.5 declares `pub fn add_delta(&mut self, other: &SplittingReportDelta)`. But `SplittingReport` in `output.rs:246` already holds the same fields as `SplittingReportDelta` would; the plan introduces a `SplittingReportDelta` in §5.2 but doesn't explain why it's a separate type from `SplittingReport`. Two options:
- **Drop `SplittingReportDelta` and use `SplittingReport` directly** as the worker-local accumulator and the global aggregator (same struct shape). Simpler.
- **Keep `SplittingReportDelta`** because it omits the `Vec<…>` / non-copyable fields. But `SplittingReport` is `Default + Debug`, no non-copyable fields — they're all `u64`.

Pick one. The plan currently maintains the distinction with no rationale.

#### G5 (Important) — `WorkerInput::EndOfStream` semantics under MPMC channel

Plan §4.1 step 1 says "sends N EOS sentinels — one per worker". But with **MPMC**, EOS messages are received by **whichever worker happens to call `recv` next**. There is no "this EOS is for worker k" routing. So:

- Worker 1 could receive 3 record messages + 1 EOS + 4 more record messages — but the channel is FIFO, so once an EOS arrives, the worker that grabbed it will exit on it, and *other* workers may still grab subsequent record messages.

This is fine IF the producer sends ALL records BEFORE the N EOS messages (the plan implies this with "At end-of-input: sends … N sentinels"). But the wording "At end-of-input" is ambiguous about ordering. **Required fix:** explicitly state "producer sends all record messages first, then exactly N `EndOfStream` messages, then drops the sender." And note that with channel-disconnect-as-EOS (G1), the N-sentinel issue disappears entirely.

If the channel-disconnect-as-EOS option is chosen (G1), this concern is moot — please pick that option.

#### G6 (Important) — Reorder buffer FinalDelta interleaving with `next_emit_idx` blocking

§4.1 step 3 collector logic: "On `WorkerOutput::Ok { input_idx, … }`: insert into reorder_buf. While `reorder_buf.first_key() == Some(&next_emit_idx)`: pop, write, increment."

But what if `WorkerOutput::FinalDelta` from worker 1 arrives **before** worker 2's `Ok { input_idx: 5 }` that would advance `next_emit_idx` from 4 to 6? The collector receives FinalDelta, sums it, but `next_emit_idx == 4` and idx 5 is missing — collector is stuck waiting for idx 5 but worker 1 has exited.

**This is fine** because the channel is MPMC and the receive side just keeps reading messages; worker 2 will eventually push idx 5 and the collector will unblock. But the plan should explicitly state: **"the collector continues `recv()`ing after each FinalDelta until it has received exactly N FinalDeltas (one per worker), at which point all input has been consumed and `next_emit_idx` should match the producer's last assigned idx + records-in-message."**

Add a defensive `debug_assert!(next_emit_idx == last_seen_input_idx + records_in_message_for_last)` after all N FinalDeltas received. Currently §4.1 step 3 is silent on this.

#### G7 (Important) — `--mbias_only` accumulation site

§4.3 / §4.7 say workers still produce `RoutedCall { key: None, … }` under `--mbias_only` "so M-bias still accumulates." But M-bias is accumulated in the **worker's `MbiasDelta`**, not at the collector. So producing the `RoutedCall { key: None }` carries the routing info to the collector for **no reason** (the collector immediately discards it).

**Refine:** in `--mbias_only` mode, the worker:
- Accumulates into its `MbiasDelta` (correct).
- Increments its `SplittingReportDelta` counters (correct — matches Perl's behaviour where counters increment even under `--mbias_only`).
- **Does NOT push a `RoutedCall`** into the output `Vec`.

This is a wire-size and CPU win at no semantic cost. The collector still receives the empty `WorkerOutput::Ok { routed_calls: vec![], records_in_message }` and increments `next_emit_idx` correctly. Update §4.3 / §4.7 / §5.2 (RoutedCall doesn't need `key: Option<OutputKey>` if mbias_only never produces routed calls — the field becomes `key: OutputKey`).

If you prefer to keep the `key: None` short-circuit for code uniformity, document the trade-off (CPU/wire-size cost for "the worker always pushes a RoutedCall per call regardless of mode").

#### G8 (Important) — N=1 path overhead claim is unquantified

§4.6 / §8.1 claim "5-10% slower than Phase B's linear loop at N=1." There's no measurement backing this. The dedup precedent at N=1 with the same `ThreadedBamReader` should give you a real number; running `cargo bench` or even a quick timing comparison would either confirm the 5-10% or surface that the overhead is materially worse.

**Important fix:** acknowledge in §8.1 that the 5-10% is a **predicted** ceiling, not measured. Add an N=1 timing comparison to the §6 step 10 profiling task (compare against the legacy `extract_se` running the same input) and define a **threshold** — e.g. "if N=1 path is > 15% slower than legacy on the 10M dataset, add the `if config.parallel == 1 { return extract_se(…) }` short-circuit in §9.2 Q1 before merging."

Right now §9.2 Q1 defers this to "post-Phase-F profiling, polish PR." But the regression hits everyone running `--multicore 1` (the default in current Perl Bismark workflows). A perceived regression in v1.0-beta could erode user trust. Better to gate the merge on the threshold.

#### G9 (Important) — Channel library choice not validated with `cargo tree`

§2 and §6 step 1 say "pin verified by `cargo tree` at implementation time". The plan-write time would have been the perfect moment to run `cargo tree -p bismark-extractor --depth 5 | grep -E 'crossbeam|rayon'` and capture the actual transitive deps. The choice between crossbeam-channel, flume, and `std::sync::mpsc` would benefit from knowing whether crossbeam-channel adds a new transitive crate to the workspace or rides for free.

`bismark-io` and `noodles` already pull in crossbeam-utils transitively. Whether they pull in `crossbeam-channel` is the question. **Important fix:** run `cargo tree` at plan-revision time and lock the answer. If crossbeam-channel is already in the tree, prefer it; if not, consider `flume` (zero unsafe, smaller surface) or `std::sync::mpsc` (zero new deps; but only SPSC on receiver — see below).

Note that `std::sync::mpsc` actually supports MPMC receiver-side as of Rust 1.81+ via `mpmc::channel`. That's an option worth at least mentioning in §2.

#### G10 (Important) — Rayon global-pool contamination guard not documented

§2 chooses `ThreadPoolBuilder::new().num_threads(N).build()` (scoped pool). Good — but **any code path inside the worker that happens to call `par_iter`, `par_chunks`, or `rayon::scope` will execute on the GLOBAL rayon pool, not the scoped one**, silently defeating the isolation. Currently no code in `bismark-extractor` or `bismark-io` uses rayon, so it's safe at the moment.

**Important fix:** document this constraint in §2 (or as a comment in `parallel.rs`'s module doc):

> "Workers must not call any rayon iterator (`par_iter` etc.) — those would run on the global pool, defeating the scoped-pool isolation. If you need parallelism inside `extract_calls` or `route_call`, install it via `pool.install(|| …)`."

Add a `#[deny(…)]` clippy lint or grep-based CI check if practical. At minimum, leave the comment.

#### G11 (Important) — Dedup-style alternative rejection is under-argued

§2 rejects the dedup-style `ThreadedBamReader`-only model with: "Extractor is CPU-bound (CIGAR walk + XM classification + M-bias) per CLAUDE.md profile, so fanning the per-record work across rayon workers is what unlocks the ≥ 4× target."

The CLAUDE.md profile shows extractor at 12.3 min single-core, dedup at 8.7 min single-core. That's a 1.4× ratio. Dedup at 4.88× on N=4 means the I/O was carrying ~half its time; if extractor's I/O carries proportionally similar weight, even simpler decompression-only parallelism could push significant speedup. **The rejection is plausible but not proven.**

A 30-minute spike on the actual 10M PE WGBS dataset — call `extract_se` from `bismark-extractor` with just `ThreadedBamReader` swapped in (no rayon, no producer/collector) — would either confirm the rejection (e.g. only 1.5× at N=4) or invalidate it (e.g. 3.5× at N=4, "good enough").

**Important fix:** acknowledge in §2 that the rejection is **architectural-judgment**, not benchmark-driven, and either (a) do the spike, or (b) frame Phase F's full pipeline as "the safe path to ≥ 4×" while listing dedup-style as a Phase-F-could-fall-back-to fallback if profiling shows the full pipeline doesn't materially beat dedup-style.

#### G12 (Optional) — Empty BAM edge case under N>1

§4.7 says "Empty BAM → producer sends N EOS sentinels immediately; workers all send FinalDelta with zero deltas; collector finalizes with empty state. Output = header-only files."

This is correct, but worth verifying: when the producer reads from `ThreadedBamReader::from_path(…)` and the BAM is empty (no records past the header), the reader's `records()` iterator returns `None` immediately. Producer sends 0 record messages, then N EOS, then exits. Workers each receive 1 EOS each, emit FinalDelta with default-zeroed mbias/report, exit. Collector receives N FinalDeltas, sums them into zeros, calls `state.finalize` which writes 12 header-only files.

The test `pipeline_empty_bam_produces_header_only_files` in §7.1 covers this. Confirm the smoke test also exercises N=4 on empty BAM to catch any "N EOS in the channel with N workers blocking on the same recv" race. Add `smoke_empty_bam_n4` to §7.2.

#### G13 (Optional) — `cleanup_partial_outputs` race under error

§4.7 "`cleanup_partial_outputs` race" says "Only the collector touches the OutputFileMap, so no race." Correct — but worth noting: at error time, workers are still emitting messages into the worker→collector channel. The collector must drain the channel completely (until all N workers send FinalDelta or Err) BEFORE calling `cleanup_partial_outputs`. Otherwise the collector's `recv` could race with the cleanup's `fs::remove_file` on the same path (no actual race because the files are unlinked, not actively written, but cosmetic).

This is implicit in the §4.5 "drain remaining messages" semantics. Just add a one-liner: "cleanup happens AFTER full channel drain."

---

## Assumptions

### Surfaced and validated

- **MPMC channels with bounded back-pressure** — locked.
- **`extract_calls` is deterministic** per input record — true by inspection (no RNG, no thread-local state).
- **`OutputFileMap` is owned by the collector** — locked (§9.1 in plan), and matches Phase E's `+ Send` bound rationale.
- **`BismarkRecord` is `Send`** — implicit; needs verification (bismark-io should already guarantee this, since dedup sends them across threads via `ThreadedBamReader::records()` to its own iterator consumer; double-check by looking at the `BismarkRecord` type or just trust the dedup precedent).

### Implicit / under-stated

- **`gzip` footer determinism**: §4.7 says "gzip footer write happens on collector OutputFileMap drop — single thread, so no race." Correct. But: is the GzEncoder's CRC32 + ISIZE footer deterministic given byte-identical input bytes? **Yes**, because RFC 1952 §2.3.1 defines CRC32 + ISIZE as functions of the input bytes only. Worth a one-liner in §4.4 stating "gzip is deterministic per RFC 1952: same input bytes → same compressed bytes → same footer."
- **Phase E's `Box<dyn Write + Send>` bound**: §3.1 mentions "Phase E §11 (integration with Phase F)" but Phase E's plan §11 actually called out RFC 1952 §2.2 stream concatenation as a possible Phase F implementation strategy. **The current plan does NOT use stream concatenation** — collector is single-writer per file, so there's no concatenation needed. **Important clarification:** add to §3.1 that "gzip stream concatenation per RFC 1952 §2.2 is NOT relevant to Phase F's architecture because the collector is single-writer per file; the stream is contiguous." Otherwise reviewers (and future you) may search for non-existent concatenation logic.
- **`crossbeam_channel` does not pull in `crossbeam-deque` or `crossbeam-epoch` unless needed**: not stated. (It actually does pull in `crossbeam-utils` which is already transitive.) Verify via `cargo tree`.
- **The `BTreeMap<u64, WorkerOutput>::first_key_value()` API exists in stable Rust**: yes, since 1.66. Worth pinning MSRV.

### Risk vs. plan claims

- §12 R3's "`Drop` impl on sender side" mitigation is structurally wrong — see G1.
- §12 R2 "M-bias merge could allocate large arrays if max_position is high" — true, but in practice max_position is bounded by read length (~150bp typical, ~300bp max for Bismark inputs). At N=8 workers × 300 positions × 3 contexts × 16 bytes/pos = ~115 KB. Trivial.
- §12 R5 "Box<dyn Write + Send> static-dispatch deferred" — fine to defer, but the deferral should be **conditional** on profiling showing it's not the bottleneck. If profiling shows it is, Phase F's PR needs to include the static-dispatch switch (otherwise the ≥ 4× target may not be met).

---

## Efficiency

### What's good

- 40N max-in-flight memory bound is correct and small (~320 KB at N=8 even with the worst-case BismarkRecord size).
- Per-record overhead estimate (channel send/recv ~50-100 ns, qname/chr clones ~175 bytes, BTreeMap O(log 40N) insert) is plausible.
- Worker-local accumulators eliminate cross-worker contention on the M-bias / counters.

### Concerns

- **Per-call qname clone**: §4.2 declares `qname: Vec<u8>` per `RoutedCall`. For ~5 calls/record, that's 5 clones of the qname per record. The qname is the same for all calls in one record. **Optimisation worth considering at plan time, not deferred:** use `Arc<[u8]>` or `Rc<[u8]>` (no, can't cross threads — `Arc` it is) for the qname, cloned once per record, shared across all calls in the record. Same for `chr: String` (these are typically short, fixed strings — `Arc<str>` or even `&'static str` if interned).

  At ~5 calls/record × 5M PE records × ~30-byte qname = ~750 MB of qname clones across the dataset. Even with the channel-bounded in-flight cap, the **allocation pressure** matters — `Vec<u8>` allocations from the per-record arena can dominate for short reads.

  **Important:** consider `qname: Arc<[u8]>` and `chr: Arc<str>` for `RoutedCall`, cloned once per record at the worker, shared across all calls. This trades 8 bytes/call (Arc pointer) for the qname/chr duplication.

- **`extract_calls` return type**: it currently returns `Vec<MethCall>` (small, copy values). Confirm this allocates per-record; if so, a per-worker scratch `Vec` reused across iterations could amortise. Not in scope for Phase F (it's a Phase B optimisation) but worth noting in §8.

- **Collector serial gzip write**: at high N, the collector becomes a serial bottleneck on `GzEncoder::write_all`. Plan §8.2 mitigates by "parallel-write per file in collector (each file is independent)." But §9.2 Q4 defers this. At N=8, with 12 split files all being written serially by one thread, the I/O rate is bounded by single-thread gzip compression bandwidth (~150 MB/s plain → ~30 MB/s compressed × 12 files = ~360 MB/s aggregate, but **serialised** through one thread = ~30 MB/s actual). That's a real risk for hitting the ≥ 4× target.

  **Important:** §8.2 should escalate "parallel-write per file in collector" from "post-Phase-F" to "Phase F if profiling shows ≥ 4× is not met on N=4." Otherwise the speedup target may slip.

- **CPU oversubscription at N > physical cores**: §4.7 documents but does not enforce a cap. Consider clamping `config.parallel` to `num_cpus::get()` with a stderr warning at startup. Or at least logging the comparison. Currently the plan punts on this; reasonable but worth a CLI warning in the `cli.rs::validate` stage. Optional.

---

## Validation sufficiency

### Coverage assessment

- **Byte-identity at N ∈ {1, 2, 4, 8}** for SE-default, PE-default, comprehensive, merge_non_CpG, yacht (with col6/col7 regression), mbias_only, gzip — comprehensive.
- **Error propagation tests** at N=4 (`smoke_parallel_invalid_xm_byte_propagates_error_at_n4`, `smoke_parallel_pe_unpaired_final_record_err_at_n4`) — good.
- **Collector reorder behaviour** (`collector_reorders_out_of_order_arrivals`, `collector_blocks_until_next_emit_idx_arrives`) — good.
- **M-bias commutativity / associativity** — good.

### Gaps

#### V1 (Critical) — No "high-contention" or "out-of-order arrival" stress test at N=4 with realistic worker imbalance

The plan tests `collector_reorders_out_of_order_arrivals` with a hand-crafted send order (2, 0, 1). That's a 3-message test. A real stress test would:
- Feed 10,000 SE records through the parallel path at N=4.
- Use a synthetic worker that introduces randomised sleep jitter (e.g. `thread_rng().gen_range(0..100µs)`) before emitting each WorkerOutput.
- Assert the resulting output is byte-identical to the legacy path.

This catches reordering bugs that a 3-message test cannot. **Important fix:** add `pipeline_n4_stress_with_worker_jitter_byte_identical` to §7.1.

#### V2 (Critical) — No deadlock-detection test

If the producer-disconnect-as-EOS is mis-implemented (G1), workers can deadlock indefinitely. The test suite has no upper-bound timeout on the parallel-path tests. **Important fix:** wrap each `smoke_parallel_*` test in a `timeout(Duration::from_secs(30))` (e.g. via a helper or via `cargo test`'s `--timeout` flag if available). Better: add an explicit `pipeline_terminates_within_5s_on_empty_bam_at_n8` test that asserts liveness.

#### V3 (Important) — No worker-count-edge-case tests

What about N=2 specifically (smallest non-trivial N), N=16 (high N), N=64 (extreme over-subscription)? The plan tests {1, 2, 4, 8} but doesn't stress the extremes. Add at least:
- `smoke_parallel_n16_byte_identical_to_legacy` — verifies behaviour above the typical core count.
- `smoke_parallel_n2_byte_identical_to_legacy` — smallest N>1.

#### V4 (Important) — Profiling target threshold is documented but not enforced in CI

§7.3 "Profiling smoke (NOT in CI)" — fine for the dataset to be local-only, but the **decision criterion** ("≥ 4× at N=4") has no automated enforcement. **Important fix:** add to §10's validation table a row: "Speedup target met on 10M dataset" → manual check → ≥ 4×. And add a Phase H "DoD" line that this manual check must pass before Phase H starts.

#### V5 (Important) — No assertion that producer thread joined cleanly

Tests don't verify the producer thread joined without panic. Add at least one test that asserts no producer-thread panic propagated past `extract_se_parallel`'s return: `producer_panic_surfaces_as_error_not_silent`. Use a fault-injection hook to make the producer panic; assert the caller sees a real error.

#### V6 (Optional) — No test of `--gzip` byte-identity at N=8

§7.2 has `smoke_parallel_gzip_n4_decompresses_to_identical_plain` but only at N=4. Add N=1 and N=8 variants to ensure gzip determinism holds at the extremes too.

#### V7 (Optional) — No test of `--mbias_only` × `--gzip` × N>1

This combination is exotic but Phase E supports it. Add `smoke_parallel_mbias_only_gzip_n4` to ensure the mbias-only short-circuit doesn't somehow leak a write to a gzip-mode OutputFileMap that was never opened.

---

## Alternatives

### Alt 1 — Dedup-style `ThreadedBamReader`-only model (rejected, but under-evaluated)

See G11. The plan's rejection is plausible but should either be benchmarked at plan-spike time (30-min cost) or kept as an explicit fallback. If the spike shows dedup-style hits ≥ 4× alone, the entire 400-LOC `parallel.rs` module is unnecessary, replaced by 30 LOC swap of `open_reader` → `ThreadedBamReader::from_path`.

### Alt 2 — Pre-formatted bytes in `RoutedCall`

§9.2 Q2 defers this. But pre-formatting at the worker is **conceptually cleaner**: workers do all the row-building work (CPU-bound) and ship `Vec<u8>` rows, collector just does `write_all`. The current shape (workers ship `MethCall + metadata`, collector re-derives the row format) duplicates `write_call`'s formatting logic across the worker and the legacy path, OR forces the collector to call `write_call` on `OutputFileMap` per call (which the plan implies but doesn't fully spec).

**Worth reconsidering at plan time, not deferring.** If the collector's write path calls `OutputFileMap::write_call(qname, chr, call, strand, col6, col7)` per RoutedCall, then the collector is doing the **same per-call work as the legacy single-threaded path** (formatting + write). That's fine, but it means the only parallelism is in `extract_calls` + M-bias accumulation, not in row formatting.

Pre-formatting in the worker would let the collector be a pure I/O loop (much faster), at the cost of larger channel messages.

**Recommendation:** measure both at plan time via a quick spike, or commit to one in Phase F (probably "collector calls write_call" for rev 0 simplicity), and defer the pre-formatted optimisation conditional on profiling.

### Alt 3 — Per-file collector mini-pool

§9.2 Q4 mentions this. With 12 split files and 8 workers, having 12 small collector threads (one per file) writing in parallel could lift the I/O bottleneck **at the cost** of also needing per-file ordering — each mini-collector needs to see only the calls destined for its file, in input order.

The trick: the producer assigns `input_idx`, workers route to `OutputKey`, but each mini-collector needs its **own** `next_emit_idx` per file. Since most calls don't go to any given file, that file's mini-collector would have gaps — the gaps need to be tracked somehow (e.g. via "input_idx ≤ X has been fully accounted-for across all files" barriers from the main coordinator). Complex.

Better alternative: write all routed calls in input-idx order to a single in-memory queue, then **batch-flush per file** every K records. Same effect, simpler. But all of this is deferred — listing here so reviewers know the plan is aware.

### Alt 4 — `flume` instead of `crossbeam_channel`

Slightly simpler API, no transitive crossbeam-utils growth, similar perf. Either is fine; just lock the choice with `cargo tree` evidence.

### Alt 5 — `std::sync::mpmc` (Rust 1.81+)

Newer stable addition. Zero new deps. Worth checking workspace MSRV (probably 1.70+ given recent Rust adoption). If MSRV permits, this is the cheapest option.

---

## Action items (prioritised)

### Critical (fix before implementation)

1. **G1**: Replace the "Drop impl sends N EOS" mitigation in §12 R3 with the correct mechanism — channel-disconnect-as-EOS (preferred) OR explicit "send all records, then N sentinels, then drop sender" with workers treating both EOS and `RecvError` identically. Add a `producer_panic_does_not_deadlock_workers` test. See G1.
2. **G2**: Specify producer thread join semantics — where `producer_handle.join()` is called, how panics surface, how producer-side errors not surfaced via the channel are propagated. See G2.
3. **G3**: Rewrite §4.3's `MbiasTable::add` pseudocode to match the actual `MbiasTable` API (direct field access on `cpg`/`chg`/`chh` Vecs; no `ensure_capacity`/`get`/`set` since those don't exist). See G3.
4. **V1**: Add a high-contention reorder stress test (`pipeline_n4_stress_with_worker_jitter_byte_identical`) — 10K records, 100µs jitter, N=4, byte-identity assert. See V1.
5. **V2**: Add liveness/deadlock test with bounded timeout (e.g. `pipeline_terminates_within_5s_on_empty_bam_at_n8`). See V2.

### Important (address before implementation, OR commit explicitly to deferral)

6. **G4**: Justify `SplittingReportDelta` as a separate type from `SplittingReport`, or merge them. See G4.
7. **G5**: Specify EOS message ordering precisely ("producer sends all records THEN N sentinels"). Moot under G1's recommended channel-disconnect option. See G5.
8. **G6**: Specify collector's FinalDelta interleaving + add the `next_emit_idx == last_input_idx + records_in_message` debug_assert. See G6.
9. **G7**: Drop the `RoutedCall { key: None }` for `--mbias_only` — workers should not emit RoutedCalls in mbias-only mode. Refine §4.3 / §4.7 / §5.2. See G7.
10. **G8**: Add an N=1 overhead threshold to §6 step 10's profiling task ("if > 15% slower than legacy, ship the short-circuit"). See G8.
11. **G9**: Run `cargo tree -p bismark-extractor` at plan-revision time; record the actual transitive-dep impact of `crossbeam-channel` + `rayon`. See G9.
12. **G10**: Document rayon-global-pool-isolation constraint in §2 (and in `parallel.rs` module doc when implemented). See G10.
13. **G11**: Either run a 30-min dedup-style spike to validate the architecture choice, OR explicitly downgrade the rejection language to "rev 0 conservative choice — dedup-style is the documented fallback if profiling shows the full pipeline doesn't materially beat it." See G11.
14. **Efficiency (qname Arc)**: Consider `Arc<[u8]>` / `Arc<str>` for `RoutedCall.qname` / `chr` to amortise the per-call clone. Defer if profiling shows allocation isn't the bottleneck — but **note the option in §8** so it's not lost.
15. **Efficiency (collector serial I/O)**: Escalate "parallel-write per file in collector" from "post-Phase-F deferred" to "Phase F gating decision based on N=4 profile." If profile misses ≥ 4×, ship it. See §8 concerns.
16. **V3**: Add `smoke_parallel_n16_byte_identical_to_legacy` and `smoke_parallel_n2_byte_identical_to_legacy`. See V3.
17. **V4**: Add a DoD line: "Speedup ≥ 4× at N=4 confirmed on 10M PE WGBS dataset, witnessed by maintainer" must hold before Phase H starts. See V4.
18. **V5**: Add `producer_panic_surfaces_as_error_not_silent`. See V5.
19. **Phase E reference clarification**: state explicitly in §3.1 that gzip stream concatenation (RFC 1952 §2.2) is NOT used in Phase F since the collector is single-writer per file. See assumptions section.

### Optional (could land as Phase F polish)

20. **G12**: Add `smoke_empty_bam_n4`. See G12.
21. **G13**: Add "cleanup happens AFTER full channel drain" one-liner. See G13.
22. **V6**: Add `smoke_parallel_gzip_n1_decompresses_to_identical_plain` and the N=8 variant.
23. **V7**: Add `smoke_parallel_mbias_only_gzip_n4`.
24. **CPU oversubscription warning**: Optionally clamp/log `config.parallel > num_cpus::get()` in `cli.rs::validate`. See §4.7.
25. **`std::sync::mpmc` consideration**: If MSRV permits (Rust 1.81+), evaluate as a zero-dep alternative to crossbeam-channel. See Alt 5.
26. **`tests/common/mod.rs` refactor**: The plan acknowledges Phase E's deferred test-helper consolidation. Decide: bundle into Phase F (since Phase F adds new test files) OR keep as a separate polish PR. See §16.

---

## Verdict summary

The plan's **architecture is sound** and the **byte-identity invariant is articulated with the right primitives**. The Critical items are real but mechanical: fix the deadlock-safety story (G1/G2), correct the pseudocode-to-API mismatch (G3), and add the missing stress/liveness tests (V1/V2). These are all addressable in a rev 1 of the plan without changing the design.

The Important items mostly concern hedge-betting against profiling surprises (G8, G11, G14, G15, V4) and tightening loose ends (G4, G5, G6, G7, G9, G10). Address them in rev 1 to avoid mid-implementation course-correction.

Once these are in: ship it. The Phase F design is solid.

---

**Reviewer:** B
**Date:** 2026-05-27
**Status:** Approve conditional on Critical items addressed in rev 1.

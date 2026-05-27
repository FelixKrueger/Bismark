# Plan Review — Phase F (`PHASE_F_PLAN.md`, rev 0) — Reviewer A

**Reviewed file:** `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PHASE_F_PLAN.md` (rev 0, 2026-05-27).
**Reviewer:** A (independent — no contact with Reviewer B).
**Skill:** plan-reviewer.
**Verdict:** the plan is largely sound and ambitious; the producer/worker/collector architecture is the right choice given the workload profile. However there are several concrete logic gaps, two missing-API mismatches, and a few unproven efficiency claims that should be addressed before implementation. None are deal-breakers; the plan is close to implementation-ready.

---

## 1. Logic review

### 1.1 Critical: `MbiasTable::add` signature in §4.3 does not match the existing `MbiasTable` API

The §4.3 pseudocode uses `other.max_position(ctx)`, `other.get(ctx, pos)`, `self.set(ctx, pos, ...)`, `self.ensure_capacity(ctx, other_max)`. But the existing `mbias.rs` exposes:

- `pub cpg: Vec<MbiasPos>`, `pub chg: Vec<MbiasPos>`, `pub chh: Vec<MbiasPos>` (public fields, no `get`/`set`).
- `max_position(&self) -> u32` — **no `ctx` arg**, returns the max across all three contexts.
- `accumulate(...)` is the only mutator. No `ensure_capacity`, no `set`, no per-context max.

So the pseudocode is aspirational against an API surface that doesn't exist yet. Two consequences:

1. The plan needs to either (a) introduce the missing `ensure_capacity` / per-context `max_position` helpers as part of Phase F, or (b) rewrite `add` to iterate the three `Vec` fields directly (which is actually simpler and matches the existing design). Option (b) is what the existing code idiom suggests:

   ```rust
   pub fn add(&mut self, other: &MbiasTable) {
       fn merge(dst: &mut Vec<MbiasPos>, src: &[MbiasPos]) {
           if src.len() > dst.len() { dst.resize(src.len(), MbiasPos::default()); }
           for (d, s) in dst.iter_mut().zip(src.iter()) {
               d.meth = d.meth.saturating_add(s.meth);
               d.unmeth = d.unmeth.saturating_add(s.unmeth);
           }
       }
       merge(&mut self.cpg, &other.cpg);
       merge(&mut self.chg, &other.chg);
       merge(&mut self.chh, &other.chh);
   }
   ```

2. The proof of associativity / commutativity becomes trivially obvious under (b) because it's just element-wise `saturating_add` on independent `Vec`s. The §4.3 pseudocode obscures this by introducing fake getters/setters.

**Action**: rewrite §4.3 against the existing struct (per-Vec iteration) and drop the `ensure_capacity`/`get`/`set`/per-context-max apparatus. The `~15 LOC + 2 unit tests` budget in §3.2 / §6.2 still fits.

### 1.2 Critical: `SplittingReport::add_delta` — existence + shape unverified

§5.5 declares `pub fn add_delta(&mut self, other: &SplittingReportDelta)` on `SplittingReport`. The plan does NOT spec what `SplittingReport`'s current public surface looks like (Phase D ground truth). The fields enumerated in `SplittingReportDelta` (§5.2: 8 counter fields) need to be verified to match the live `SplittingReport` struct exactly — if Phase D added more counters (e.g. `calls_unknown`, `lines_with_no_calls`, `total_aligned_reads`), the delta will silently under-merge.

**Action**: before implementation, the implementer must read `output.rs::SplittingReport` and either (a) make `SplittingReportDelta` a structural mirror with **every** counter, or (b) make `SplittingReportDelta = SplittingReport` (just re-use the type as its own delta) and define `add` on the report itself. Option (b) is cheaper and dodges the "did I miss a field?" risk entirely.

### 1.3 Critical: producer/EOS-sentinel ownership story is muddled

§4.1 step 1 says: "At end-of-input: sends `WorkerInput::EndOfStream { worker_count: N }` (N sentinels — one per worker) and closes."

§5.2 declares `WorkerInput::EndOfStream` as a unit variant (no fields). The "N sentinels" pattern needs N actual messages on the channel, not one tagged with `worker_count`. Then §4.1 step 2 says workers break on `WorkerInput::EndOfStream` — but with a MPMC channel and N sentinels, the producer must send N distinct sentinel messages so each worker pops exactly one.

This is fixable in 5 lines but the plan's text reads as if `EndOfStream { worker_count: N }` is one message that fan-outs — which is **not** how `crossbeam-channel::Receiver::recv()` works. Producer MUST loop `for _ in 0..n { tx.send(WorkerInput::EndOfStream)?; }`.

Also: workers should treat `RecvError::Disconnected` as EOS (which R3 already mentions). If they do that, the explicit EOS sentinel becomes redundant — workers can just drain until disconnect, then emit `FinalDelta`. This is the more idiomatic crossbeam pattern. Pick one and document.

**Action**: pick one model — either (a) N explicit `EndOfStream` sentinels with `worker_count` removed from the variant; or (b) drop sentinels entirely and rely on channel-disconnect (worker emits `FinalDelta` on `RecvError`). Then update §4.1, §5.2, and R3 to match.

### 1.4 Important: collector cannot distinguish "first delta" from "last delta" without a sentinel

§4.1 step 3 says "After all `FinalDelta`s received: emit final outputs (`state.finalize()` equivalent)". But how does the collector know how many `FinalDelta` to wait for? The plan implies N (one per worker), but §5.2 carries no "i'm-the-last-one" flag. The collector must count down `final_deltas_received` from N to 0 — that's fine but should be explicit in §4.1.

Edge case: if a worker errors out, it emits `WorkerOutput::Err` and breaks **without** sending `FinalDelta` (per §4.1 step 2 / §4.5 row 4). Then the collector waiting for N FinalDeltas will block forever. Either:
- Workers must always send a final message (Err or FinalDelta) before exit — guaranteed by the worker loop's structure (`break` always implies one message sent).
- OR the collector counts "worker exits" via channel-disconnect detection on a per-worker channel.

Crossbeam MPMC has only one shared receiver, so per-worker disconnect isn't observable. The collector must rely on "every worker sends exactly one terminal message (Err xor FinalDelta) before exiting". The plan should make this invariant explicit.

**Action**: add to §4.1 / §4.5: "INVARIANT: every worker sends exactly one terminal message (`Err` or `FinalDelta`) before exit. Collector decrements N→0 to know when stream is drained."

### 1.5 Important: reorder buffer + Err interleaving — non-determinism risk

§4.5 row 4: "Stash Err, drain channel until all workers send EOS, then return Err."

But which Err? If worker A sends Err(InvalidXmByte at idx=10) and worker B sends Err(BadCigar at idx=8) concurrently, the collector's `recv()` order is non-deterministic — crossbeam's MPMC delivery order is FIFO per-sender but arbitrary across senders. So `cargo run --parallel 4` on an input with multiple errors could produce different stderr messages on different runs. **This breaks byte-identity of stderr output**, which Phase H may want to assert.

Mitigations (any one suffices):
- Stash ALL errs, then pick the one with the lowest `input_idx` (deterministic — requires tagging Err with input_idx).
- Stash only the first Err but require errors to be input-ordered (impossible — workers process in parallel).
- Accept stderr non-determinism for multi-error inputs but byte-identity of successful output is preserved.

Plan should either pick the deterministic-by-input_idx scheme or explicitly accept the non-determinism and document it.

**Action**: add `WorkerOutput::Err(input_idx: u64, error: ...)` (tagging the Err with the failing record's idx) and have the collector keep the lowest-idx Err. Otherwise document the stderr non-determinism explicitly.

### 1.6 Important: `--gzip` parallel writes — finalize semantics

§4.7 says `--gzip` "footer-on-drop semantics preserved; collector is single-writer per file". Good. But the collector's reorder buffer holds onto records until prerequisite indices arrive. If the producer/workers fail mid-stream, the collector's reorder buffer may contain partial records that were written to one .gz file but not another (because earlier records got written and later ones are still buffered).

Phase E's `cleanup_partial_outputs` removes ALL .gz files on error — that's the right behaviour. Plan correctly references this. But should make explicit: collector calls `cleanup_partial_outputs` on the OutputFileMap when handling stashed Err, BEFORE returning.

**Action**: §4.5 row 5 ("Collector write error") and row 4 ("Worker extract_calls error") should both explicitly say "collector runs cleanup_partial_outputs before returning". Currently row 5 says it for write errors but not for upstream errors.

### 1.7 Important: PE pairing assumes strictly adjacent — Phase F should document, not validate

Phase C's `extract_pe` (verified — `pipeline.rs:202+`) pulls records in stream order and pairs R1/R2 as adjacent. The Phase F producer (§4.1) inherits this — pairing the next-record. The plan correctly mirrors Phase C.

Worth being explicit: the plan does NOT introduce robustness against interleaved R1/R2 (no qname-buffer for non-adjacent mates). That's a Bismark contract — output is name-sorted from the alignment phase. If anyone runs the extractor on a coordinate-sorted BAM, they'd hit `MateChromosomeMismatch` or qname-mismatch errors. This is the existing contract and Phase F preserves it.

**Action**: add to §4.7 / §9.1 (locked assumptions): "Input BAM is assumed name-sorted with R1/R2 strictly adjacent — same contract as Phase C `extract_pe`. Coordinate-sorted BAMs are not supported and will produce errors. Phase F does NOT introduce a qname-buffer for non-adjacent mates."

### 1.8 Important: `cargo bench` / CI-checkable speedup target?

§7.3 documents the ≥ 4× target as a **manual** profiling step on a non-CI dataset. SPEC §9.7 is the source of the target. This means the speedup target is never enforced — a future refactor could silently regress to 2× and CI would pass.

Options:
- Add a `cargo bench` benchmark on a tiny synthetic BAM that measures speedup at N=1 vs N=4 and asserts a threshold. The number won't match 4× on a 1 MB BAM (overheads dominate), but a regression-safe ratio like ≥ 1.5× at N=4 would still catch catastrophic regressions.
- Document the target in §16 (follow-up) as a tracked deferred item for Phase H to gate at scale, with no CI assertion.

The plan currently lands somewhere in between. Recommend the latter (cheaper) but make it explicit.

**Action**: clarify §7.3: "Speedup is NOT CI-asserted. Phase H §… owns the at-scale ≥ 4× gate. Phase F documents the manual measurement as a sign-off step before merging the PR."

### 1.9 Important: collector single thread is the documented bottleneck — no early-warning test

§8.2 explicitly calls out collector I/O as the most likely bottleneck for missing the 4× target. The plan defers the mitigation ("parallel-write per file in collector" — §9.2 #4) as a post-merge optimisation. But the plan doesn't add ANY test that would surface a collector-bound regression. If the implementation lands with the collector spending 80% of wall-clock in `write_all`, the manual profile (§6 step 10) is the only signal.

Suggested cheap micro-instrumentation: emit a stderr debug line in `--debug` builds reporting per-thread wall-time at end of run (producer, worker-avg, collector). The implementer would learn IMMEDIATELY at the first run if collector is saturating.

**Action**: add an optional instrumentation step (e.g. `BISMARK_PROFILE_THREADS=1` env var prints per-thread wall-time at exit). Cheap to add, expensive to lack when troubleshooting.

### 1.10 Minor: §5.6 main.rs dispatch — auto-detect probe outside of pipeline

The dispatch code in §5.6 shows `detect_paired_from_header_via_probe(&input)?` for `AutoDetect`. This **re-opens the BAM** before the parallel pipeline opens it again. Phase C already pays this cost in the single-threaded path so it's a pre-existing minor inefficiency, but it's worth flagging — could be folded into the producer's first record peek instead.

**Action**: not blocking. Note as a §16 follow-up.

---

## 2. Assumption review

### 2.1 Assumption surfaced but not validated: per-record CPU cost dominates channel overhead

The plan asserts (§2 row 1, §8.1) that CPU work per record is high enough to justify worker fan-out. Back-of-envelope:

- Per-call extraction (CIGAR walk + XM byte classification): O(read_length) = ~150 bytes scanned per record.
- Channel `send` (crossbeam bounded): ~50-100 ns.
- Clone of qname (~30 bytes) + chr (~5 bytes) × ~5 calls/record: ~5 × `Vec<u8>::clone()` ≈ ~5 × 50 ns = 250 ns/record.
- BTreeMap insert/lookup at the collector: O(log 40N) — for N=8, ~6 cmps × ~10 ns = 60 ns/record.

If the per-record extract work is, say, 5 µs (a 150-byte CIGAR walk + classifications + ~5 hash updates is plausible at ~5-10 µs on M1), then the parallel overhead is **~10%** at N=1. That tracks with the plan's "5-10%" estimate in §4.6.

The 4× speedup at N=4 then requires the collector to keep up. Collector per-record work is: pop from BTreeMap, write ~5 lines × ~50 bytes = ~250 bytes to OutputFileMap. At ~1 GB/s write throughput (gzipped output is ~30 MB/s typically), that's ~8 µs per record for the I/O alone — uncomfortably close to the worker's 5 µs at N=4 (worker batch produces ~5 µs × 4 = 20 µs of records every 5 µs, then collector needs 8 µs × 5 calls = 40 µs to write them).

**Implication**: the plan's 4× target is plausible but NOT obviously safe — the collector could become the bottleneck for `--gzip` workloads. The plan's R1 + §9.2 #4 + §8.2 already flag this. The back-of-envelope above is worth adding to the plan to make the risk concrete.

**Action**: add §8.5 with a per-record cost breakdown (CPU vs collector I/O) to make the 4× claim defensible.

### 2.2 Assumption: cloned qname/chr cost is "negligible" — sketchy at scale

§4.2 says ~80-120 bytes/call, "Acceptable." But scaled to a real run: 50M reads × ~5 calls/read × 100 bytes = **25 GB of allocation throughput**. The plan flags this as a deferred optimisation. At Apple M1 with the system allocator, this is probably fine (jemalloc-style arenas), but it's an additional pressure point.

The plan's note "Phase F polish opportunity" (§9.2 #2) covers pre-formatting in the worker. Better still would be to pass `chr` by **index** (the `chr_index` already mentioned in `WorkerInput::Se { chr_index }` per §4.1) — workers would carry a `chr_id: u32` and the collector resolves to a string when writing. Saves the ~5-byte clone × 5 calls × 50M = ~1.25 GB.

`qname` is harder to elide (per-record unique), but Bismark's qnames are short (~30 bytes typical) and could be a `Box<[u8]>` instead of `Vec<u8>` (saves the spare-capacity allocation overhead — typically 16 bytes per allocation).

**Action**: §4.2 mentions `chr_index` in the SE WorkerInput but `RoutedCall` re-clones the chr String. Plan a `chr_id: u32` on `RoutedCall` and have the collector hold a `chr_table: Vec<String>` (already built once at startup). This is cheap to add to rev 0 and saves ~1+ GB allocation per real run.

### 2.3 Assumption: N=1 short-circuit is the right rev-0 choice

The plan's rev 0 chooses "threaded pipeline at N=1" with a 5-10% overhead. This is defensible for code-uniformity. But two scenarios make me less comfortable:

- **Future legacy-path removal**: §4.6 says "the Phase B/C/D/E single-threaded `extract_se` / `extract_pe` remain in the codebase as legacy paths but become unreachable from main.rs::run". A future cleanup could remove these (dead code) and Phase F would lose its byte-identity reference. This needs a sticky comment / `#[cfg(test)]` guard / explicit "DO NOT DELETE" doc-comment on `extract_se` / `extract_pe`.
- **`--parallel 1` users**: batched single-core jobs (e.g. running 50 samples in 50 LSF jobs, each with `--parallel 1`) would pay the 5-10% overhead unnecessarily. Estimated impact: ~7 min over 100 minutes — not catastrophic but real.

**Action**:
1. Add `#[doc = "DO NOT DELETE — byte-identity reference for parallel.rs"]` or similar on `extract_se` / `extract_pe`. Make it test-only via `#[cfg(any(test, feature = "legacy"))]` if you want strict gating.
2. Document the "batched N=1 jobs pay 5-10% penalty" trade-off in §4.6 explicitly so users know.

### 2.4 Assumption: channel sizing ratio (32 producer→worker vs 8 worker→collector) is correct

The plan asserts §2 row 5 / §9.2 SPEC: "Worker→collector kept smaller because collector is the I/O bottleneck." This is **backwards from typical pipeline buffer-sizing wisdom**. Standard rule: the buffer in front of the slowest stage should be the LARGER one to absorb upstream bursts.

If collector is the bottleneck, the worker→collector buffer is what fills up when workers complete in bursts. Making it smaller (N×8) means workers will block more frequently when the collector is busy — exactly the wrong direction for utilisation.

Counter-argument: if collector is processing in-order and workers can complete out-of-order, the collector also has to **wait** for the next-emit-idx to arrive. So a small buffer doesn't matter — the buffer fills with "future" records that the collector can't emit anyway. Hmm, this changes the calculus: the buffer needs to be large enough to hold the **dispersion window** of input_idx values.

Worst case: worker 0 stalls on a slow record (e.g. one that takes 50ms because of malloc pressure), workers 1..N-1 keep producing. If they produce ~N messages/ms each and the collector blocks on `next_emit_idx = worker_0`, the buffer fills with ~N×50 messages waiting. For N=4, that's 200 — already 5× the planned `N×8 = 32`-message buffer.

**Action**: increase the worker→collector buffer to at least `N × 32` (i.e. **same** size as producer→worker), and possibly more. Cost is trivial (~32 × 80 bytes × N = 10 KB at N=4). The plan's `N×8` looks under-sized.

### 2.5 Assumption: BGZF reader threads = worker count is optimal

§2 / §6 step 10: `ThreadedBamReader::from_path(input, config.parallel)` shares the parallel count for the BGZF decode pool. This was dedup's choice. But for the extractor:
- Producer thread only feeds N workers. If BGZF decode is faster than the workers, extra BGZF threads are wasted.
- A common rule: BGZF decode is roughly 1 thread = 200 MB/s decompressed. For N=4 workers each at ~50K records/s = 200K records/s × ~500 bytes = 100 MB/s.

So a single BGZF thread can saturate 4 workers. Plan should consider `min(parallel, 2)` or `min(parallel, 4)` for the reader pool.

**Action**: not critical — `ThreadedBamReader` may already cap internally. Worth checking + documenting. Could be a §16 polish item ("Tune BGZF reader thread count separately from worker count").

---

## 3. Efficiency review

### 3.1 Reorder buffer cost is bounded — calculation passes

§4.4 + §8.3: max 40N entries × ~500 bytes (claimed) = ~160 KB at N=8.

Re-deriving: `WorkerOutput::Ok` contains `Vec<RoutedCall>` (each ~80-120 bytes per §4.2), call count per record varies. For ~5 calls/record × 100 bytes = 500 bytes/message body — matches the plan's claim. Bounded.

### 3.2 M-bias merge cost — bounded but worth thinking about

§8.1 says O(max_position × 3 × N). For 150-bp reads × 3 contexts × N=8 = 3600 entries summed. Trivial.

Worst case: read_length = 300 bp (Illumina 2×150 mode). 300 × 3 × 8 = 7200 entries. Still trivial.

### 3.3 Collector single-thread I/O — actual bottleneck risk

Already covered in §1.9 + §2.1.

### 3.4 Allocation pressure from per-call clones — see §2.2

### 3.5 Producer holding two records concurrently (PE)

§4.1 says producer pairs adjacent records before sending. So at peak, producer holds R1 + R2 + the "next record being read" simultaneously = 3 × ~500 bytes. Negligible.

---

## 4. Validation sufficiency review

Strong points:
- 22 unit tests + 13 smoke tests = solid coverage.
- Byte-identity at multiple N values (1, 2, 4, 8) is correct invariant to test.
- M-bias commutativity + associativity tests address the most subtle correctness invariant.
- Critical-1 regression guard (yacht col-6/col-7 at N=4) explicitly tested — good defensive measure.
- Error-propagation under N=4 is tested.

Gaps:

1. **No deadlock-safety test**: what happens if the producer panics mid-stream? §4.5 row 6 says "thread joins surface the panic via `JoinHandle::join().expect(...)`", but if the producer panics, workers will see channel-disconnect and exit cleanly. If a worker panics, the producer fills the channel and blocks; the collector is starved. Test: inject a panic in the worker (via a feature-gated `BISMARK_INJECT_WORKER_PANIC=true` env var) and assert the process exits within a reasonable timeout (not hangs forever).

2. **No stress test at very high N**: §4.7 mentions `--parallel 64` is "fine" but there's no test. Even a smoke test at N=16 on a synthetic BAM would catch issues with thread fan-out scaling.

3. **No concurrent flag-combination test**: e.g. `--gzip --mbias_only --comprehensive --parallel 8`. Individual modes are tested at N=4 separately. The matrix isn't exhaustive — a single combined test would catch most interaction bugs.

4. **No malformed-channel-message test**: easy to test that an internal `RoutedCall { key: Some(K), call: ..., chr: "" }` with empty chr doesn't silently produce a malformed line — defensive guard.

5. **No "panic in collector" test**: if the OutputFileMap write fails partway through a `WorkerOutput::Ok`, what happens to remaining workers? The collector must still drain channels or workers block forever. Test: inject a write failure on the 5th message and assert workers exit cleanly.

6. **M-bias merge over-3-context interaction**: §4.3's tests cover one context. Should test "merge a table with only CpG data into one with only CHH data" — exercises the grow-from-empty path in both directions.

7. **Counter-equivalence test for SplittingReport**: the plan asserts `SplittingReport` counters are sum-reducible but doesn't add a test asserting that the **collector's** final counters match what the legacy single-threaded path produces. The `smoke_parallel_splitting_report_counts_match_across_n` test compares N=1 to N=8 but doesn't compare N=anything to the legacy `extract_se` counts.

**Action**: add the 7 tests above (or a subset). The deadlock-safety, very-high-N, and combined-flags tests are highest value.

---

## 5. Alternatives

### 5.1 Alternative architecture: ThreadedBamReader-only (dedup-style)

The plan rejects this in §2 row 1 with the reasoning "extractor is CPU-bound per CLAUDE.md profile". I think this rejection is **almost certainly correct** but worth a back-of-envelope:

- Dedup is I/O-bound: it does one hash insert per record, no per-call work. So BGZF decode + write throughput dominates → ThreadedBamReader-only delivers 4.88×.
- Extractor is per-call CPU-bound: ~5 calls per record × CIGAR walk per call × M-bias accumulate. The per-record CPU is 5-10× higher than dedup's. So fanning across workers should help — unless I/O dominates regardless.

The plan's choice is sound. But I'd recommend adding a **fallback escape hatch** to §16: "If Phase F profiling shows < 4× at N=4 AND collector I/O is < 60% of wall-clock, the rayon worker fan-out is justified. Otherwise reconsider ThreadedBamReader-only as a simpler fallback (~100 LOC vs ~700)." This makes the architectural choice testable post-merge.

### 5.2 Alternative: pre-format `Vec<u8>` rows in workers

§9.2 #2 — covered as deferred optimisation. Strong recommendation: implement this for rev 0. Reasoning:

- Workers already iterate over `MethCall`s. Formatting the output line (~50 bytes) inside the worker is ~200 ns of work.
- Collector then does `write_all(&row)` directly — no per-call formatting work, just I/O.
- Channel message size grows from ~80 bytes/call to ~130 bytes/call — marginal.
- But the collector single-thread bottleneck (§1.9 + §8.2) is mitigated by ~30%.

The trade is: more work for the implementer (~50 LOC) for a tangibly lower bottleneck risk. I think rev 0 should include this — the "deferred to post-merge" punt risks merging a 3× speedup version of Phase F and then immediately needing the optimisation.

### 5.3 Alternative: chr-index instead of chr-string clone

Already covered in §2.2 action. Should be in rev 0.

### 5.4 Alternative: workers write directly to per-worker scratch files; collector merges

Phase E reportedly considered this. Plan §11 doesn't mention it. Briefly: each worker writes to a worker-local file; collector concatenates at end (with chr/idx-aware merge). Pros: no in-flight reorder buffer, no channel for output. Cons: file-system contention, post-merge step. Probably worse than the current plan, but worth a 2-line acknowledgement.

---

## 6. Other observations

### 6.1 The plan is honest about its risks

R1–R5 in §12 are well-stated and pre-empt the obvious objections. The "Open questions" §9.2 is properly scoped.

### 6.2 Deps verification is appropriately rigorous

§6.1 / §9.1 pin both `rayon = "=1.10.x"` and `crossbeam-channel = "=0.5.x"` with `cargo tree` verification. The Phase E flate2 version-correction precedent (the `=1.1.9` story in `Cargo.toml`) is good discipline.

Confirmed by my own check: `crossbeam-channel` is **already a transitive dep** via noodles (in `Cargo.lock` lines 298, 303, 573, 584). So adding it as a direct dep doesn't introduce a new crate — just pins the version. No conflict risk.

### 6.3 Branching strategy (§15) is clean

Targets `rust/iron-chancellor` directly, off `extractor-phase-f`. Matches the cascade pattern established by Phase B–E.

### 6.4 §16 follow-ups properly tracked

Phase E deferreds + Phase F polish + Phase H scale-test all tracked.

---

## 7. Action items (prioritized)

### Critical (must address before implementation)

1. **§1.1**: Rewrite §4.3 `MbiasTable::add` against the existing `MbiasTable` API (3 public `Vec<MbiasPos>` fields, single-arg `max_position()`). Use per-Vec iteration with `saturating_add`. Drop the fictional `ensure_capacity`/`get`/`set`/per-context-max helpers.
2. **§1.2**: Verify `SplittingReportDelta` field set matches `SplittingReport`'s actual fields against Phase D's code. Prefer using `SplittingReport` as its own delta (`SplittingReport::add(&mut self, other: &SplittingReport)`) to avoid "missed field" drift.
3. **§1.3**: Fix the EOS-sentinel story. Either (a) producer sends N explicit unit-variant `EndOfStream` messages, OR (b) drop sentinels and have workers treat channel-disconnect as EOS. Update §4.1, §5.2, R3 to match.
4. **§1.5**: Make Err selection deterministic — tag `WorkerOutput::Err` with `input_idx`, collector keeps the lowest. OR explicitly document stderr non-determinism for multi-error inputs.

### Important (address before implementation if possible)

5. **§1.4**: Add explicit invariant to §4.1/§4.5: "every worker sends exactly one terminal message (Err xor FinalDelta) before exit; collector counts N→0 to detect drain". Without this, the worker-error path deadlocks the collector.
6. **§1.6**: §4.5 row 4 ("Worker extract_calls error") should mention `cleanup_partial_outputs` runs before collector returns. Currently only row 5 does.
7. **§2.4**: Increase worker→collector channel size from `N×8` to at least `N×32`. The "smaller buffer in front of slower stage" reasoning is backwards; the buffer should absorb worker burst-completion when collector blocks on next-emit-idx.
8. **§2.2 + §5.3**: Use `chr_id: u32` (already mentioned for WorkerInput) in `RoutedCall` too, instead of cloning `String`. Saves ~1+ GB allocation per real run.
9. **§5.2**: Implement the **pre-format `Vec<u8>` row in worker** optimisation in rev 0, not as a deferred polish item. The collector I/O is the documented bottleneck and this is its primary mitigation.
10. **§1.7**: Add to §4.7 / §9.1 the explicit "input BAM must be name-sorted with R1/R2 adjacent — same as Phase C; no qname-buffer in Phase F".
11. **§2.3**: Mark `extract_se` / `extract_pe` (legacy single-threaded) with `#[doc = "DO NOT DELETE — byte-identity reference for parallel.rs"]` or `#[cfg(any(test, feature = "legacy"))]`. Prevents future cleanup from removing the byte-identity reference.
12. **§4 missing tests**: add tests for (a) worker-panic deadlock-safety, (b) collector write-failure mid-stream, (c) `--gzip --mbias_only --comprehensive --parallel 8` combined flags, (d) PE-orphan + invalid-XM race, (e) M-bias merge across all 3 contexts including empty-into-non-empty.
13. **§2.1**: Add §8.5 with the per-record CPU vs collector I/O cost back-of-envelope to make the 4× claim defensible.

### Optional (polish / future)

14. **§1.8**: Clarify §7.3 that speedup is NOT CI-asserted; Phase H owns the at-scale gate. Optionally add a `cargo bench` regression-guard at ≥ 1.5× at N=4 on a synthetic 50K-record BAM.
15. **§1.9**: Add `BISMARK_PROFILE_THREADS=1` env-var instrumentation that prints per-thread wall-time at exit. Cheap to add, valuable when troubleshooting the 4× target.
16. **§2.5**: Document whether BGZF reader threads should be capped lower than the worker count (e.g. `min(parallel, 4)`). Probably a §16 follow-up.
17. **§5.4**: Add a 2-line acknowledgement of the "per-worker scratch file + merge" alternative for completeness.
18. **§1.10**: Note in §16 that the `detect_paired_from_header_via_probe` opens the BAM separately from the producer; could be folded into the producer's first-record peek.

---

## 8. Verdict

**Phase F is implementation-ready after addressing the 4 Critical items.** The architectural choices are well-reasoned; the dedup-style rejection is sound; the test surface is broad; the integration with later phases is clear. The Important items (especially the channel sizing direction, chr-id refactor, and pre-format-in-worker optimisation) would substantially improve the chance of hitting the ≥ 4× speedup target on first profile-pass without a follow-up rev.

Estimated rev-1 work: ~1-2 hours of plan edits.

---

**File written:** `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PLAN_REVIEW_PHASE_F_A.md`

# `bismark-extractor` Phase F — rayon `--multicore N` (byte-identical invariant)

**Status:** rev 1 — awaiting implementation trigger.
**Date:** 2026-05-27 (rev 1 same-day after dual plan-review absorption).
**Slug:** `plans/05262026_bismark-extractor/PHASE_F_PLAN.md`.
**Phase target:** SPEC §10 row F + §6.4 + §9 — ~700 LOC.
**GitHub sub-issue:** [#860](https://github.com/FelixKrueger/Bismark/issues/860) (filed at work-start).
**Depends on:** Phases A→E (all merged to `rust/iron-chancellor` via #847, #849, #856, #853, #855 respectively). Branch: `extractor-phase-f` off `rust/iron-chancellor`.

## Epic linkage

- **Design contract:** `rust/bismark-extractor/SPEC.md` (in-repo, rev 3+). Phase F covers SPEC §6.4 (rayon producer/worker/collector), §9 (parallelism model — byte-identity invariant, channel sizing, M-bias merge, output ordering, error propagation, N=1 path, speedup expectation), §10 row F.
- **GitHub umbrella:** issue [#798](https://github.com/FelixKrueger/Bismark/issues/798) (extractor epic, In Progress).
- **Prior phases:** A (#847), B (#849), C (#856), D (#853), E (#855) — all merged.

## 1. Goal

Replace Perl's fork+modulo `--multicore N` model with a Rust producer/worker/collector pipeline that achieves **byte-identical output to `--multicore 1` for any N ≥ 1** while delivering ≥ 4× speedup at N=4 (SPEC §9.7 target, dedup's 4.88× as a precedent point).

The Perl model decompresses the BAM N times for N processes — that's the 16% of pipeline time identified in CLAUDE.md profiling. Phase F replaces it with:
1. **Single BGZF decompression** via `bismark_io::ThreadedBamReader`.
2. **Producer thread** assigns monotonic `input_idx` to each record (SE) or pair (PE) and pushes to a bounded MPMC channel.
3. **N rayon worker threads** run `extract_calls` + `drop_overlap` + per-worker M-bias scratch, push `WorkerOutput { input_idx, calls, mbias_delta, ... }` to a second bounded channel.
4. **Output collector** (main thread) reorders by `input_idx` via a sliding-window buffer, writes split files in input order, sum-reduces M-bias deltas.

After Phase F the only remaining work for v1.0 is Phase G (bedGraph/cytosine_report subprocess chain) and Phase H (real-data byte-identity gate + release).

## 2. Scope decisions (locked at plan-write time)

| Decision | Choice | Reasoning |
|----------|--------|-----------|
| Producer/worker/collector vs dedup-style ThreadedBamReader-only | **Full producer/worker/collector model** (SPEC §6.4) | Dedup's 4.88× at N=4 with ThreadedBamReader alone works because dedup is I/O-bound. Extractor is CPU-bound (CIGAR walk + XM classification + M-bias) per CLAUDE.md profile, so fanning the per-record work across rayon workers is what unlocks the ≥ 4× target. Documented as the rev 0 architecture decision; revisit only if Phase F profiling proves the simpler model meets the target. |
| Rayon scoped pool vs global pool | **Scoped via `rayon::ThreadPoolBuilder::new().num_threads(N).build()`** | Avoids interfering with the global rayon pool (any future caller could use it). Pool drops at end of `extract_se_parallel`/`extract_pe_parallel`. **Constraint (rev 1 per Reviewer B G10):** worker code MUST NOT call into any API that uses rayon's global pool (e.g. `par_iter`, `rayon::spawn` without explicit pool). Such calls would run on the global pool not our scoped one — defeating isolation and potentially deadlocking if the global pool is exhausted. Currently nothing in `bismark-io` or `bismark-extractor` uses rayon, so the constraint is satisfied; document as a comment in `parallel.rs` for future maintainers. |
| Channel library | **`crossbeam-channel = "=0.5.x"`** (latest 0.5 line, pin verified via `cargo tree` at implementation time) | Battle-tested MPMC channel. `flume` is the alternative; both work. Crossbeam wins on ecosystem mindshare + dedup's noodles internal use of crossbeam-utils. |
| Channel sizing | **Producer→worker: `bounded(N × 32)`; Worker→collector: `bounded(N × 8)`** (SPEC §9.2) | Producer→worker provides back-pressure when workers fall behind; worker→collector kept smaller because collector is the I/O bottleneck. For N=8 max in-flight = 320 records (40N — SPEC §9.4). |
| PE pair formation | **In producer thread** (workers receive pre-formed `BismarkPair` messages) | Workers can't pair across messages (would need shared mutable state); pairing in producer keeps worker pure-functional. Producer runs `BismarkPair::from_mates(r1, r2)?` and propagates errors via the channel. |
| Worker output format (rev 1) | **`WorkerOutput::Ok { input_idx: u64, routed_calls: Vec<RoutedCall>, records_in_message: u64 }`**; `RoutedCall { key: Option<OutputKey>, call: MethCall, strand: BismarkStrand, yacht_col6: u32, yacht_col7: u32, qname: Arc<[u8]>, chr_id: u32 }`. | Rev 1 ownership story per Reviewer A.Important + Reviewer B.Important: (1) **`qname: Arc<[u8]>`** — record's calls share one Arc clone (pointer-sized; clones are atomic-inc not byte-copy). Worker constructs the Arc once per record, then every call from that record clones the Arc handle. (2) **`chr_id: u32`** — the noodles `reference_sequence_id`. Collector resolves to name via shared `Arc<[String]> chr_table` (built once at pipeline start; read-only from then on). Cuts ~1+ GB allocation pressure on 50M-read runs vs the rev 0 cloned-`String` design. |
| Collector reorder buffer | **`BTreeMap<u64, WorkerOutput>` keyed by input_idx + `next_emit_idx: u64`** | Strict in-order emission (SPEC §9.4). Buffer size bounded by `40N` (channel-bound sum). |
| `input_idx` granularity | **One idx per SE record / per PE pair** | Matches `state.report.records_processed` increment pattern from Phase B/C (SE: +1 per record; PE: +2 per pair). |
| M-bias merge | **Per-worker `MbiasDelta = [MbiasTable; 2]` accumulated incrementally; sum-reduced at end-of-stream** (SPEC §9.3) | `MbiasTable::accumulate()` is commutative + associative → byte-identical regardless of merge order. End-of-stream sentinel message from each worker carries the final delta; collector applies via `MbiasTable::add(&other)`. |
| `add(&other)` helper on `MbiasTable` | **New method** — position-wise sum of meth/unmeth counts | Needed by collector's M-bias merge. ~20 LOC including unit test. |
| Splitting-report counter merge | **Per-worker `SplittingReport` (reused; not a separate `Delta` type)** | Rev 1 simplification per Reviewer A C2 / Reviewer B G4: the live `SplittingReport` already has the 8 sum-reducible fields (`records_processed`, `calls_total`, `calls_cpg_meth`, …). A separate `SplittingReportDelta` would drift over time. Add `SplittingReport::add(&mut self, other: &Self)` — 8 saturating sums. |
| `--mbias_only` interaction (rev 1) | **Worker accumulates M-bias + counters locally but emits empty `routed_calls: Vec<RoutedCall>` (still emits `WorkerOutput::Ok` with `records_in_message` so the collector advances its `next_emit_idx`)** | Rev 1 per Reviewer B G7 — saves channel traffic and the collector's mode-check branch. Workers know about `config.is_mbias_only()` (passed at thread spawn). The M-bias merge in `FinalDelta` is unchanged. |
| Error propagation | **First `WorkerOutput::Err(e)` wins; collector drains remaining messages to let workers terminate; cleanup_partial_outputs then propagate** (SPEC §9.5) | Matches Phase B–E's existing error path. Workers send `Result<WorkerOutput, BismarkExtractorError>`; collector picks first Err. |
| N=1 path | **Same threaded pipeline, channels sized at `1 × 32` / `1 × 8`** (SPEC §9.6) | Byte-identity reference is "single worker through the same pipeline", not "linear loop". Keeps the code path uniform; the per-record cost of channel-send is dwarfed by the per-record work. The Phase B/C/D/E single-threaded `extract_se` / `extract_pe` remain in the codebase as legacy paths but become **unreachable from `main.rs::run`** for `--parallel >= 1` (i.e. always); they're kept for the existing test suite to exercise. Decision point — see §9.2 #1. |
| **Alternative: short-circuit N=1 to legacy** | **Rejected for rev 0** | Adds branching; risks the legacy path and parallel path drifting. Trade: ~5-10% per-record overhead at N=1 vs simpler architecture. Phase F profiling may reopen. |
| `+ Send` bound on `Box<dyn Write>` | **Still required** — workers send `Vec<u8>` of pre-formatted rows to collector OR routed calls with owned strings to write at collector. Since collector is single-threaded, writers themselves never cross threads after construction — but the `OutputFileMap` itself moves between thread setup and finalize merge, so `+ Send` stays. (Locked in Phase E for this exact reason.) | Plan reviewers in Phase E approved the bound as forward-looking. Phase F validates it. |
| Producer batch size | **1 record (SE) or 1 pair (PE) per channel message** | Simpler reorder buffer; back-pressure already handled by channel bounding. Batching could be a Phase F post-merge optimisation if profiling shows channel overhead dominates. |
| Worker count selection (`config.parallel`) | **Use `config.parallel as usize`** unchanged — Phase A already validates `>= 1`. | No new validation needed. |
| BGZF reader threads | **`ThreadedBamReader::from_path(input, config.parallel)`** matching the worker count | Reader's internal BGZF decode pool shares the worker count. Same as dedup precedent. |
| Test surface | **Byte-identity invariant tests** at N ∈ {1, 2, 4, 8} comparing against the legacy `extract_se` / `extract_pe` output | The legacy path stays in the codebase as the reference. Each smoke test runs the same BAM under legacy and parallel paths; asserts every split file is byte-identical. |
| Legacy `extract_se` / `extract_pe` stability commitment (rev 1) | **Do not delete without replacement reference.** Per both reviewers: the legacy paths are the byte-identity oracle for all Phase F tests. Removing them silently in a future cleanup PR would leave nothing to verify against. Document this as a `// PHASE F INVARIANT: do not delete — byte-identity reference` comment at the top of each function; mention in `lib.rs` rustdoc. | A future "v1.0 cleanup" might be tempted to remove "dead code" if the legacy paths look unused. They're used by the test suite, not main; the comment makes the dependency explicit. |
| `--multicore` alias of `--parallel` | **No change** (Phase A already aliased them) | — |

## 3. Context

### 3.1 Source documents read end-to-end

- **SPEC.md** §6.4 (rayon/single-BGZF design), §7.7 `ExtractState` data structure, §9 (full parallelism model: pipeline shape, channel sizing, M-bias merge, output ordering, error propagation, N=1 path, speedup expectation), §10 row F.
- **`bismark-dedup` precedent**: `rust/bismark-dedup/src/pipeline.rs:447-612` — `run_single_parallel`, `run_multiple_parallel`, UMI variants. Confirmed: dedup uses `ThreadedBamReader`/`ThreadedBamWriter` only, no rayon worker-pool fan-out. Phase F's full producer/worker/collector model is new in the workspace.
- **`bismark-io`**: `ThreadedBamReader::from_path(path, parallel)` available since v1.0.0-beta.5; widely used in dedup.
- **CLAUDE.md profiling**: extractor 12.3 min single-core → 5.4 min 4-core (Perl), expected target ≥ 4× at N=4 in Rust.
- **Phase E plan §11** (integration with Phase F): per-worker `OutputFileMap` merge model; gzip stream concatenation via RFC 1952 §2.2; `Box<dyn Write + Send>` bound was deliberately forward-looking for this phase.

### 3.2 Code placement

All Phase F code lands inside `rust/bismark-extractor/`. **No bismark-io / bismark-dedup touches** (bismark-io already exposes everything needed).

- **New module:**
  - `rust/bismark-extractor/src/parallel.rs` (~400 LOC) — producer, worker, collector loops. Public entry points `extract_se_parallel(input, config)` and `extract_pe_parallel(input, config)`. Internal types `WorkerInput`, `WorkerOutput`, `RoutedCall`, `MbiasDelta`, `SplittingReportDelta`.
- **Modified modules:**
  - `rust/bismark-extractor/src/main.rs::run` — dispatch on `config.parallel`: if `>= 1`, route to `extract_se_parallel` / `extract_pe_parallel`. The Phase B/C single-threaded `extract_se` / `extract_pe` become callable only from the test suite (no longer from `main`).
  - `rust/bismark-extractor/src/mbias.rs` — add `MbiasTable::add(&mut self, other: &MbiasTable)` (position-wise sum). ~15 LOC + 2 unit tests.
  - `rust/bismark-extractor/src/output.rs::SplittingReport` — add `SplittingReport::add(&mut self, other: &SplittingReport)`. ~10 LOC + 1 unit test.
  - `rust/bismark-extractor/src/route.rs` — extract a pure helper `compute_routed_call(state_mode, record, chr, strand, call, read_identity) -> RoutedCall` that returns the routing decision (OutputKey + yacht col6/col7 + cloned qname/chr/xm) WITHOUT writing. Workers call this; collector consumes and writes. Phase E's `route_call` becomes a thin wrapper that calls the helper + writes inline (legacy single-threaded path).
  - `rust/bismark-extractor/src/lib.rs` — `pub mod parallel`; re-export `extract_se_parallel`, `extract_pe_parallel`.
  - `rust/bismark-extractor/Cargo.toml` — version bump `1.0.0-alpha.5` → `1.0.0-alpha.6`; add `rayon = "=1.10.x"` (pin verified via `cargo tree`) + `crossbeam-channel = "=0.5.x"`. Both pins resolved at implementation time.
- **Tests:**
  - `rust/bismark-extractor/tests/parallel_phase_f.rs` (NEW) — unit-level tests on producer/worker/collector pieces using fake channels + mocked workers.
  - `rust/bismark-extractor/tests/parallel_phase_f_smoke.rs` (NEW) — end-to-end byte-identity tests at N ∈ {1, 2, 4, 8} against the legacy single-threaded path on synthetic BAMs covering SE-directional + PE + each output mode + `--gzip`.
  - Phase B-E existing tests **unchanged** — they exercise the legacy single-threaded path which Phase F preserves as the byte-identity reference.

### 3.3 Crate versions

- `bismark-extractor`: `1.0.0-alpha.5` → `1.0.0-alpha.6`.
- `bismark-io`: unchanged (`1.0.0-beta.7`).
- `bismark-dedup`: unchanged.

### 3.4 Binary behaviour

After Phase F, `bismark-methylation-extractor-rs --parallel N` runs the rayon pipeline. For any N ≥ 1:
- Output split files are byte-identical to `--parallel 1` output on the same input.
- M-bias.txt is byte-identical.
- Splitting-report counts are identical (commutative sum reduces by N workers → same total).
- `--gzip` output decompresses to identical bytes (footer-on-drop semantics preserved; collector is single-writer per file).
- Wall-clock target: ≥ 4× speedup at N=4 vs single-core, on the 10M PE WGBS profiling baseline.

## 4. Behaviour specification

### 4.1 Pipeline phases (matches SPEC §9.1)

```
                    ┌─ worker 1 ─┐
input BAM ──▶ producer ──▶ worker 2  ──▶ collector ──▶ write split files + M-bias.txt + report
                    └─ worker N ─┘                       (single thread)
```

1. **Producer** runs on a dedicated thread, drives `ThreadedBamReader::records()`:
   - SE: takes each record, assigns next `input_idx`, sends `WorkerInput::Se { input_idx, record, chr_id }` (chr_id is the noodles `reference_sequence_id` — collector resolves to name via the shared `Arc<[String]> chr_table`; rev 1 per Reviewer A.Important).
   - PE: takes two adjacent records, runs `BismarkPair::from_mates(r1, r2)?`, sends `WorkerInput::Pe { input_idx, pair, chr_id }`.
   - On reader error: sends `WorkerInput::Err { input_idx: producer_next_idx, error: e }` then drops the sender (which disconnects the channel — see EOS model below).
   - **At end-of-input: drops the sender** (no `EndOfStream` sentinel needed). Workers detect `Err(RecvError::Disconnected)` and exit.

   **EOS model — rev 1 commitment (per Reviewer A C3 / Reviewer B G1):** channel-disconnect-as-EOS, NOT N sentinels. Rationale:
   - `Sender::drop` is the canonical Rust idiom for "no more messages"; channel automatically disconnects.
   - Survives producer panic correctly: `panic → Sender::drop → channel disconnects → workers see Disconnected → exit cleanly`. No deadlock.
   - The rev 0 "Drop impl sends N sentinels" wording was structurally wrong — `Drop` can't synthesise messages.

2. **Workers** (N threads in the scoped rayon pool):
   - Each owns a worker-local `[MbiasTable; 2]` (init: `[Default::default(); 2]`) and a worker-local `SplittingReport` (init: `Default::default()`). Rev 1 reuses `SplittingReport` directly instead of a separate Delta type.
   - Loop: `recv()` from producer channel:
     - `Ok(WorkerInput::Se { input_idx, record, chr_id })`:
       - Resolve `chr` via `&chr_table[chr_id]`.
       - Run `extract_calls(record, ignore_5p, ignore_3p, mbias_only_silence)`.
       - Per call: accumulate M-bias (`mbias[idx].accumulate(...)`); increment counters.
       - If `config.is_mbias_only()`: **do NOT emit `RoutedCall`s** (rev 1 per Reviewer B G7 — saves channel traffic; collector skips routing anyway).
       - Else: per call, run `compute_routed_call(...)` and collect into `Vec<RoutedCall>`.
       - Send `WorkerOutput::Ok { input_idx, routed_calls, records_in_message: 1 }` (or empty `routed_calls` under mbias_only).
     - `Ok(WorkerInput::Pe { ... })`: same with `extract_calls` on both mates + `drop_overlap` if `config.no_overlap` + pair-strand routing; `records_in_message: 2`.
     - `Ok(WorkerInput::Err { input_idx, error })`: forward as `WorkerOutput::Err { input_idx, error }`; continue draining until `Err(Disconnected)` (so all in-flight messages are processed for byte-identity).
     - `Err(RecvError::Disconnected)` → EOS. Emit `WorkerOutput::FinalDelta { mbias, report }` carrying this worker's accumulated state; exit loop.
   - **Invariant:** every worker sends exactly one `FinalDelta`, regardless of error paths. Collector knows it has all workers when it has received N `FinalDelta`s.

3. **Collector** runs on the main thread:
   - Owns the `OutputFileMap`, the global `MbiasTable[2]`, the global `SplittingReport`.
   - Loop: `recv()` from worker channel. Maintain `reorder_buf: BTreeMap<u64, WorkerOutput>` and `next_emit_idx: u64`.
     - On `WorkerOutput::Ok { input_idx, ... }`:
       - Insert into reorder_buf.
       - While `reorder_buf.first_key() == Some(&next_emit_idx)`: pop, write all `RoutedCall`s in order to OutputFileMap, increment `next_emit_idx` by 1 (SE) or 2 (PE — matches Perl's "Processed N lines" counting).
     - On `WorkerOutput::FinalDelta { mbias, report }`: sum-reduce into globals.
     - On `WorkerOutput::Err(e)`: stash, continue draining the channel until all workers send `FinalDelta` (or error out), then return Err(stashed).
   - After all `FinalDelta`s received: emit final outputs (`state.finalize()` equivalent).

### 4.2 `RoutedCall` shape (what crosses worker→collector channel)

Workers cannot send borrowed references (the record is dropped on the worker thread). Rev 1 ownership pattern uses `Arc` for the per-record qname and an index for chr (instead of cloned bytes/string):

```rust
use std::sync::Arc;

pub(crate) struct RoutedCall {
    /// Pre-routed key per the active OutputMode (Phase E). `None` is
    /// unreachable at the collector under rev 1's mbias_only worker
    /// short-circuit, but kept for future modes that might emit None.
    pub key: Option<OutputKey>,
    /// The call itself (16 bytes, Copy).
    pub call: MethCall,
    /// Pair-strand (PE) or record-strand (SE) — used for write path.
    pub strand: BismarkStrand,
    /// Yacht col-6 / col-7 (zeros when mode != Yacht).
    pub yacht_col6: u32,
    pub yacht_col7: u32,
    /// QNAME bytes shared across all calls from the same record via Arc.
    /// Cloning is an atomic-inc, not a byte-copy — amortises ~30-byte
    /// qname over ~5 calls/record (rev 1 per Reviewer B's optimisation).
    pub qname: Arc<[u8]>,
    /// Reference sequence ID; collector resolves to chromosome name via
    /// the shared `Arc<[String]> chr_table` built once at pipeline start.
    /// Saves ~5-byte+heap String clone per call (rev 1 per Reviewer A).
    pub chr_id: u32,
}
```

**Memory cost rev 1 (24 bytes fixed + ~16 bytes/Arc-on-stack):** ~50 bytes/call vs rev 0's ~100 bytes/call. At ~5 calls/record × 32 records × 8 workers = ~64 KB in-flight (was ~128 KB). Per-run allocation pressure savings on a 50M-read full WGBS run: ~25 GB → ~5 GB (5 calls/read × 50M reads × 100 bytes/call → same × 20 bytes/call accounting for the Arc-handle-only cost).

**Worker constructs the qname Arc once per record:**

```rust
// Inside worker_loop on receipt of WorkerInput::Se { record, ... }:
let qname_bytes: &[u8] = record.inner().name()
    .map(|n| n.as_ref())
    .unwrap_or(b"<unnamed>");
let qname_arc: Arc<[u8]> = Arc::from(qname_bytes);

for call in extract_calls(&record, ...)? {
    routed_calls.push(compute_routed_call(
        config.output_mode, &record, chr_id, strand, call, /* qname */ Arc::clone(&qname_arc),
    )?);
}
```

**Pre-format-row in worker (deferred):** the §9.2 #2 open question still applies — workers could pre-format the output line bytes (`Vec<u8>`) and ship that instead of structured RoutedCall, letting the collector do raw `write_all`. Reviewer A flagged this as worth promoting to rev 1, but the rev 1 ownership refactor (Arc + chr_id) already addresses the dominant allocation pressure (the original justification). Pre-formatting is left as a post-implementation measurement; if profiling shows collector serialisation is the ≥ 4× target bottleneck, do it in a Phase F polish PR.

### 4.3 M-bias merge (SPEC §9.3)

Rewritten in rev 1 against the actual `MbiasTable` shape — which is three public `Vec<MbiasPos>` fields (`cpg`, `chg`, `chh`), not the abstract `get`/`set`/`ensure_capacity`/per-context-`max_position` API rev 0 imagined. The actual implementation is direct per-vec resize-then-zip:

```rust
impl MbiasTable {
    /// Position-wise sum of another table into this one. Commutative and
    /// associative; per-position counts are `u64::saturating_add` sums.
    /// Used by Phase F's M-bias delta merge at end-of-stream.
    pub fn add(&mut self, other: &Self) {
        Self::add_one(&mut self.cpg, &other.cpg);
        Self::add_one(&mut self.chg, &other.chg);
        Self::add_one(&mut self.chh, &other.chh);
    }

    fn add_one(dst: &mut Vec<MbiasPos>, src: &[MbiasPos]) {
        // Grow dst if src is larger (extra positions in src land at the
        // existing default-zero entries we resize-in).
        if dst.len() < src.len() {
            dst.resize(src.len(), MbiasPos::default());
        }
        // zip iterates min(dst.len(), src.len()) — for dst.len() > src.len()
        // the surplus dst entries stay as-is, which is exactly what we want
        // (they only had self's contribution, no contribution from src).
        for (s, o) in dst.iter_mut().zip(src.iter()) {
            s.meth = s.meth.saturating_add(o.meth);
            s.unmeth = s.unmeth.saturating_add(o.unmeth);
        }
    }
}
```

**Why commutative + associative:** `u64::saturating_add` is commutative + associative when summing N values whose total stays ≤ `u64::MAX` (saturation breaks associativity only when overflow occurs; for M-bias counts on a single run, total calls fit comfortably in u64 — typical 50M-read run is ~10⁹ calls, far below `u64::MAX = 1.8 × 10¹⁹`).

Tests assert:
- `mbias_table_add_is_commutative` — `a.clone().add(&b)` byte-equals `b.clone().add(&a)`.
- `mbias_table_add_is_associative` — `(a+b)+c == a+(b+c)`.
- `mbias_table_add_grows_when_other_larger` — `a.cpg.len() == 50` adding `b.cpg.len() == 100` → result has `cpg.len() == 100`, slots 50–99 = b's slots 50–99.
- `mbias_table_add_self_larger_keeps_self_tail` — `a.cpg.len() == 100` adding `b.cpg.len() == 50` → slots 50–99 unchanged (no contribution from b's empty tail).

### 4.4 Output ordering (SPEC §9.4)

The reorder-by-input_idx ensures the collector writes records in **strict producer-input order** — same order as a single-threaded linear loop would. Combined with: (a) a single collector thread writing each file, (b) `BufWriter` already in place — this gives byte-identity to the legacy single-threaded path.

Memory bound: at most `producer_channel_size + worker_channel_size = N × 32 + N × 8 = 40N` entries in the reorder buffer at any time. For N=8 = 320 entries. For N=16 = 640. Bounded.

### 4.5 Error propagation (SPEC §9.5; rev 1 tightened per Reviewer A C4 + Reviewer B G2)

**Deterministic Err selection rule (rev 1):** the collector maintains `best_err: Option<(u64 /* input_idx */, BismarkExtractorError)>`. On each incoming `WorkerOutput::Err { input_idx, error }`, replace `best_err` iff `input_idx < best_err.0` (or `best_err` is `None`). After the drain completes (all N `FinalDelta`s received), if `best_err.is_some()`, run `cleanup_partial_outputs` and return `Err(best_err.1)`. The lowest-`input_idx` Err wins regardless of arrival order → byte-identical stderr across N and across runs.

For errors not tied to a specific message (worker write-channel send error, producer reader-init error before any record dispatched), assign `input_idx = u64::MAX` so they sort below all message-tied errors. **`u64::MAX` errors are still surfaced** if no smaller-idx error exists.

**Producer thread lifecycle (rev 1 per Reviewer B G2):**

```rust
let producer_handle: JoinHandle<()> = thread::spawn(move || {
    producer_loop(producer_tx, reader, paired_mode);
    // Sender drops here when closure returns (panic-safe via unwind).
});

// ... collector loop runs on main thread, recv()s from worker_rx ...

// After collector exits its loop (all N FinalDeltas received):
match producer_handle.join() {
    Ok(()) => { /* clean exit */ }
    Err(panic_payload) => {
        // Producer panicked. Workers already saw channel-disconnect and exited
        // (their FinalDeltas were received above). Synthesise an InternalError
        // so the user sees the panic info; cleanup partial outputs.
        cleanup_partial_outputs();
        return Err(BismarkExtractorError::InternalError {
            message: format!("producer thread panicked: {:?}", panic_payload),
        });
    }
}
```

| Error site | Handling |
|---|---|
| Producer read error (e.g. truncated BAM) | Send `WorkerInput::Err { input_idx: producer_next_idx, error: e }`; drop sender. Workers forward as `WorkerOutput::Err { input_idx, error }`, continue draining until `Err(Disconnected)`, then emit `FinalDelta` and exit. Collector applies deterministic-Err-selection. |
| Producer pair-formation error (`UnpairedFinalRecord`, `MateChromosomeMismatch`) | Same as above. `input_idx` is the pair index where the error occurred. |
| Producer thread panic | `Sender::drop` (via unwind) disconnects channel; workers see Disconnected and exit cleanly with `FinalDelta`. Main thread's `producer_handle.join()` returns `Err(panic_payload)` → propagated as `InternalError`. |
| Worker `extract_calls` error (`InvalidXmByte` without `--mbias_only`) | Worker sends `WorkerOutput::Err { input_idx, error }` (idx of the message being processed); continues draining (rev 1 — does NOT short-circuit, so byte-identity for other in-flight Errs is preserved). On next `Err(Disconnected)`, emits `FinalDelta` and exits. |
| Worker `drop_overlap` error | Same. |
| Collector write error | Stashed into `best_err` with `input_idx: u64::MAX` (the write was attempted at no specific idx — but the in-flight record's idx is actually known; rev 1 uses that). Drain remaining `FinalDelta`s, run `cleanup_partial_outputs`, return Err. |
| Worker thread panic | `JoinHandle::join()` returns `Err(panic_payload)`; main thread propagates as `InternalError` after the collector loop exits. |

### 4.6 N=1 path (SPEC §9.6)

When N=1:
- Producer thread + 1 worker thread + collector thread = 3 threads (collector is main).
- Channels sized at `1 × 32` and `1 × 8` (or just 32 + 8 — they're already absolute sizes).
- Producer pushes, worker processes, collector writes. Effectively synchronous but in-flight by ~32+8 records.
- The byte-identity check at N=1 (vs the legacy `extract_se`/`extract_pe`) is the reference test for Phase F.

Why not short-circuit N=1 to legacy? See §9.2 #1 — keeping the threaded path uniform avoids drift between two code paths.

### 4.7 Edge cases

| Case | Handling |
|---|---|
| Empty BAM (no records) | Producer sends N `EndOfStream` sentinels immediately; workers all send `FinalDelta` with zero deltas; collector finalizes with empty state. Output = header-only files (matches legacy). |
| Single record SE | input_idx = 0; one worker processes; collector writes in order. |
| Reader error mid-stream (e.g. truncated BAM) | Producer sends `Err`; cascades per §4.5. Partial outputs cleaned up. |
| `--mbias_only` | Worker produces `RoutedCall { key: None, ... }` per call (so M-bias still accumulates); collector skips writes for `key == None`. Counters still accumulate (per Phase E counter-equivalence test). |
| `--gzip` + cleanup-on-error | Same as Phase E §4.6: clean-error-path cleanup works; panic-path may leak partial .gz files. |
| PE first/last record | Producer pairs adjacent records via `from_mates`; if last record is orphan R1, send `Err(UnpairedFinalRecord)`. |
| `--parallel` very large (e.g. 64) | Channel size = 64×32=2048 + 64×8=512 = 2560 entries × ~80 bytes/entry ≈ 200 KB. Fine. Worker oversubscription beyond physical core count makes context-switching dominant; document but don't enforce a cap. |
| Workers see calls with `record.alignment_start() == None` (defensive) | Worker emits `WorkerOutput::Err(InternalError)` per Phase E semantics. |
| `cleanup_partial_outputs` race | Only the collector touches the OutputFileMap, so no race. Cleanup runs on the collector thread when the error is processed. |

## 5. Signatures (proposed)

### 5.1 `parallel.rs` (NEW) — top-level entry points

```rust
//! Phase F: rayon-based --multicore N pipeline (byte-identical to --parallel 1).

use std::path::Path;
use crate::cli::ResolvedConfig;
use crate::error::BismarkExtractorError;

/// SE extraction with N rayon workers. `config.parallel` selects N
/// (Phase A validates >= 1). Byte-identical to single-threaded
/// [`crate::extract_se`] for any N.
pub fn extract_se_parallel(
    input: &Path,
    config: &ResolvedConfig,
) -> Result<(), BismarkExtractorError>;

/// PE extraction with N rayon workers. Same byte-identity guarantee.
/// Pair-formation happens in the producer thread (workers receive
/// pre-formed `BismarkPair` messages).
pub fn extract_pe_parallel(
    input: &Path,
    config: &ResolvedConfig,
) -> Result<(), BismarkExtractorError>;
```

### 5.2 `parallel.rs` (NEW) — internal types

```rust
/// Producer → worker channel message. EOS is signaled by the producer
/// dropping its sender (channel-disconnect-as-EOS, rev 1) — no sentinel
/// variant needed.
enum WorkerInput {
    Se { input_idx: u64, record: BismarkRecord, chr_id: u32 },
    Pe { input_idx: u64, pair: BismarkPair, chr_id: u32 },
    Err { input_idx: u64, error: BismarkExtractorError },
}

/// Worker → collector channel message.
enum WorkerOutput {
    Ok {
        input_idx: u64,
        routed_calls: Vec<RoutedCall>,
        records_in_message: u64, // 1 for SE, 2 for PE
    },
    /// Sent exactly once by each worker at its exit (after recv() returns
    /// `Err(Disconnected)` from the producer→worker channel — rev 1 EOS
    /// model, see §4.1). Carries this worker's accumulated counters for
    /// the collector to sum into the globals.
    FinalDelta {
        mbias: [MbiasTable; 2],
        /// Reuse the live `SplittingReport` type — rev 1 per Reviewer A C2
        /// / Reviewer B G4. No separate `SplittingReportDelta`.
        report: SplittingReport,
    },
    /// Carries an `input_idx` for deterministic Err selection at the
    /// collector (rev 1 per Reviewer A C4). For errors NOT tied to a
    /// specific message (e.g. producer read-error), the producer sets
    /// `input_idx = u64::MAX` so worker errors at known indices always
    /// rank first.
    Err {
        input_idx: u64,
        error: BismarkExtractorError,
    },
}

/// Pre-routed call ready for collector to write. See §4.2.
/// Rev 1: qname shared via Arc across record's calls; chr by id-lookup.
pub(crate) struct RoutedCall {
    pub key: Option<OutputKey>,
    pub call: MethCall,
    pub strand: BismarkStrand,
    pub yacht_col6: u32,
    pub yacht_col7: u32,
    pub qname: std::sync::Arc<[u8]>,
    pub chr_id: u32,
}

// Rev 1: `SplittingReportDelta` was dropped — the per-worker delta IS the
// live `SplittingReport` struct (Default::default() at worker start; sum-
// merged at collector finalize via `SplittingReport::add`). One type, one
// source of fields, zero drift surface.
```

### 5.3 `route.rs` — extract pure helper (NO state mutation)

```rust
/// Pure routing helper: compute the [`RoutedCall`] for one extracted
/// `MethCall` under the given mode + strand + record metadata. Does NOT
/// write anywhere or mutate state.
///
/// Used by Phase F workers (single-threaded `route_call` is rewritten in
/// terms of this helper for code-share).
pub(crate) fn compute_routed_call(
    mode: OutputMode,
    record: &BismarkRecord,
    chr: &str,
    strand: BismarkStrand,
    call: MethCall,
) -> Result<RoutedCall, BismarkExtractorError>;
```

### 5.4 `mbias.rs` — add merge helper

```rust
impl MbiasTable {
    /// Sum `other` into `self` position-wise. Commutative + associative.
    /// Used by Phase F's M-bias delta merge at end-of-stream.
    pub fn add(&mut self, other: &MbiasTable);
}
```

### 5.5 `output.rs::SplittingReport` — add merge helper

Rev 1 (per Reviewer A C2 / Reviewer B G4): no separate `SplittingReportDelta` type. The live `SplittingReport` already has the 8 sum-reducible fields and is its own delta.

```rust
impl SplittingReport {
    /// Sum `other` into `self` field-wise. Commutative + associative.
    /// Used by Phase F's collector to merge per-worker `SplittingReport`
    /// deltas at end-of-stream.
    pub fn add(&mut self, other: &Self) {
        self.records_processed = self.records_processed.saturating_add(other.records_processed);
        self.calls_total       = self.calls_total.saturating_add(other.calls_total);
        self.calls_cpg_meth    = self.calls_cpg_meth.saturating_add(other.calls_cpg_meth);
        self.calls_cpg_unmeth  = self.calls_cpg_unmeth.saturating_add(other.calls_cpg_unmeth);
        self.calls_chg_meth    = self.calls_chg_meth.saturating_add(other.calls_chg_meth);
        self.calls_chg_unmeth  = self.calls_chg_unmeth.saturating_add(other.calls_chg_unmeth);
        self.calls_chh_meth    = self.calls_chh_meth.saturating_add(other.calls_chh_meth);
        self.calls_chh_unmeth  = self.calls_chh_unmeth.saturating_add(other.calls_chh_unmeth);
    }
}
```

### 5.6 `main.rs::run` — dispatch on parallel

```rust
// Phase F (this build): --parallel >= 1 always uses the parallel pipeline.
// Single-threaded extract_se/extract_pe remain in lib.rs as the legacy
// byte-identity reference exercised by the existing test suite.
match config.paired_mode {
    PairedMode::SingleEnd => extract_se_parallel(&input, &config),
    PairedMode::PairedEnd => extract_pe_parallel(&input, &config),
    PairedMode::AutoDetect => {
        let is_paired = detect_paired_from_header_via_probe(&input)?;
        if is_paired {
            extract_pe_parallel(&input, &config)
        } else {
            extract_se_parallel(&input, &config)
        }
    }
}
// REMOVED: the previous `if config.parallel != 1 { PhaseNotYetImplemented }` reject.
```

## 6. Implementation outline

1. **Add deps** (`Cargo.toml`):
   - `rayon = "=1.10.x"` (latest 1.10 release; pin verified by `cargo tree -p bismark-extractor | grep rayon` showing single version).
   - `crossbeam-channel = "=0.5.x"` (same verification).
   - Version bump `1.0.0-alpha.5` → `1.0.0-alpha.6`.

2. **`mbias.rs::MbiasTable::add`** — implement + unit-test commutativity, associativity, growth-on-empty.

3. **`output.rs::SplittingReport::add_delta`** — implement + unit-test.

4. **`route.rs::compute_routed_call`** — extract the pure routing helper. Adapt existing Phase E `route_call` to call it then do the write inline (legacy single-threaded path keeps working).

5. **Create `src/parallel.rs`** with the producer/worker/collector loops:
   - Module structure: `mod parallel { fn extract_se_parallel(...) { ... } fn extract_pe_parallel(...) { ... } fn run_pipeline<P>(...) }` where `P` is a producer trait or function.
   - Use `crossbeam_channel::bounded(N * 32)` for producer→worker; `bounded(N * 8)` for worker→collector.
   - Spawn producer via `std::thread::spawn` (one thread).
   - Spawn workers via `rayon::ThreadPoolBuilder::new().num_threads(N).build().unwrap().scope(|s| { for _ in 0..N { s.spawn(|_| worker_loop(...)) } })`.
   - Collector runs inline on the main thread.

6. **Update `main.rs::run`**: drop the `--parallel != 1` reject; dispatch SE/PE to `_parallel` variants.

7. **Update `lib.rs`**: `pub mod parallel`; re-export `extract_se_parallel`, `extract_pe_parallel`.

8. **Tests** (§7).

9. **`cargo test -p bismark-extractor && cargo clippy --all-targets -- -D warnings && cargo fmt --check`**.

10. **Profiling pass** (per CLAUDE.md profiling discipline): run the binary on the 10M PE WGBS dataset at N ∈ {1, 2, 4, 8}, record wall-clock + RSS. Assert ≥ 4× speedup at N=4. If not met, diagnose: collector I/O bottleneck? Reorder buffer thrashing? GzEncoder per-file serialisation?

## 7. Tests

**Universal timeout guard (rev 1 per Reviewer B V2):** every test in `tests/parallel_phase_f.rs` and `tests/parallel_phase_f_smoke.rs` runs inside a 30-second timeout (via `std::thread::spawn` + `JoinHandle::join` with manual timeout, or a `tokio::time::timeout`-equivalent for sync code). A deadlocked test must NOT hang CI indefinitely — it fails the timeout, surfaces "deadlock detected" in the test output, and lets the next test continue.

### 7.1 Unit tests (`tests/parallel_phase_f.rs`)

| Test | Asserts |
|------|---------|
| `mbias_table_add_is_commutative` | `a.clone().add(&b)` == `b.clone().add(&a)`. |
| `mbias_table_add_is_associative` | `(a.add(&b)).add(&c)` == `a.add(&(b.add(&c)))`. |
| `mbias_table_add_grows_when_other_larger` | Adding a table with positions up to 100 into one with positions up to 50 grows the receiver to 100. |
| `splitting_report_add_delta_field_wise_sum` | Field-wise sums match. |
| `compute_routed_call_default_mode_returns_some_key` | Default mode produces `Some(OutputKey::Default(...))`. |
| `compute_routed_call_mbias_only_returns_none_key` | MbiasOnly mode → `key == None`. |
| `compute_routed_call_yacht_includes_strand_conditional_col6_col7` | OB strand → col6 > col7 (Critical-1 regression guard via the parallel path). |
| `producer_se_assigns_monotonic_input_idx` | Mock producer over fixture records emits idx 0, 1, 2, …. |
| `producer_pe_pairs_records_and_assigns_one_idx_per_pair` | input_idx is +1 per pair (records_processed counter still +2 per pair via `records_in_message`). |
| `producer_pe_orphan_r1_emits_unpaired_final_record_err` | Odd-record-count BAM → `WorkerInput::Err(UnpairedFinalRecord)`. |
| `worker_loop_se_processes_record_emits_routed_calls` | Synthetic SE record + worker → expected `Vec<RoutedCall>`. |
| `worker_loop_pe_handles_drop_overlap` | PE pair where R2 has calls past R1's reference_end → drop_overlap removes them in worker. |
| `worker_loop_emits_final_delta_on_eos` | Worker receives `EndOfStream` → emits `FinalDelta { mbias, report }` with its accumulated values. |
| `worker_loop_propagates_extract_calls_err` | Invalid XM byte → worker emits `WorkerOutput::Err`. |
| `collector_reorders_out_of_order_arrivals` | Send WorkerOutputs with idx 2, 0, 1; collector writes in order 0, 1, 2. |
| `collector_blocks_until_next_emit_idx_arrives` | If idx 1 is in buffer but idx 0 missing, collector doesn't emit idx 1. |
| `collector_sums_final_deltas_correctly` | N=4 workers each contribute deltas summing to a known total. |
| `collector_drains_after_err` | First Err received; collector reads remaining channel messages until all workers send EOS, then returns Err. |
| `pipeline_n1_synchronous_handoff` | N=1 path completes without deadlock; produces identical output to legacy. |
| `pipeline_n4_byte_identical_to_legacy_se` | Same input + N=4 → byte-identical split files vs `extract_se`. |
| `pipeline_n4_byte_identical_to_legacy_pe` | Same input + N=4 → byte-identical split files vs `extract_pe`. |
| `pipeline_empty_bam_produces_header_only_files` | Empty input → 12 header-only files (or per-mode equivalent). Matches Phase B empty-BAM test. |
| `producer_panic_does_not_deadlock_workers` (rev 1, C3/G1) | Synthetic producer panics on second record; workers see channel-disconnect; emit FinalDelta; collector receives N FinalDeltas; main `JoinHandle::join()` returns Err with panic info; cleanup_partial_outputs runs. Wrapped in 30s timeout. |
| `collector_picks_lowest_input_idx_err_on_multiple_worker_errors` (rev 1, C4) | Inject Errs at input_idx 5, 3, 7 in arrival order; assert the returned Err carries the data from idx 3 (lowest). Byte-identical across 100 runs with shuffled arrival order. |
| `worker_mbias_only_emits_empty_routed_calls` (rev 1, G7) | --mbias_only worker processes records; emitted `WorkerOutput::Ok` has `routed_calls.is_empty()` even though M-bias counters accumulate. |
| `worker_qname_arc_shared_across_record_calls` (rev 1) | One synthetic record with 5 calls; all 5 `RoutedCall.qname` share the same Arc (verified via `Arc::ptr_eq`). |
| `collector_resolves_chr_id_via_shared_chr_table` (rev 1) | Synthetic chr_table = ["chr1", "chr2"]; RoutedCalls with chr_id 0 / 1 → output rows correctly contain "chr1" / "chr2". |
| `producer_thread_panic_propagates_as_internal_error` (rev 1, G2) | Force producer panic; main collector loop completes; `producer_handle.join()` Err is converted to `BismarkExtractorError::InternalError` with panic payload in message. |
| `worker_thread_panic_propagates_as_internal_error` (rev 1) | Force one worker to panic on its 3rd message; remaining N-1 workers complete; main thread surfaces panic via JoinHandle. |
| `pipeline_n8_reorder_buffer_property_test` (rev 1, V1) | Generate 1000 records; randomise worker arrival order across 50 trial seeds; assert collector's emitted-record order matches sorted input_idx for every seed. |

### 7.2 End-to-end smoke (`tests/parallel_phase_f_smoke.rs`)

Synthetic ~50-record SE-directional + ~30-pair PE BAMs (reuse Phase B/C/D helpers; possibly factor into `tests/common/mod.rs` per Phase E deferred TODO M3/L-4).

| Smoke test | Asserts |
|------------|---------|
| `smoke_se_parallel_n1_byte_identical_to_legacy_extract_se` | Run binary with `--parallel 1`; run legacy `extract_se` programmatically; compare all 12 split file bytes. |
| `smoke_se_parallel_n4_byte_identical_to_legacy_extract_se` | Same at N=4. |
| `smoke_se_parallel_n8_byte_identical_to_legacy_extract_se` | Same at N=8. |
| `smoke_pe_parallel_n4_byte_identical_to_legacy_extract_pe` | PE byte-identity at N=4. |
| `smoke_parallel_comprehensive_mode_n4_byte_identical` | `--comprehensive --parallel 4` → 3 files byte-identical to single-threaded. |
| `smoke_parallel_merge_non_cpg_n4_byte_identical` | `--merge_non_CpG --parallel 4` → 8 files identical. |
| `smoke_parallel_yacht_n4_byte_identical_including_reverse_strand_col6_col7` | Yacht mode at N=4; reverse-strand rows still have col-6 > col-7 (Critical-1 regression guard across parallel path). |
| `smoke_parallel_mbias_only_n4_byte_identical` | `--mbias_only --parallel 4` → M-bias.txt + report byte-identical. |
| `smoke_parallel_gzip_n4_decompresses_to_identical_plain` | `--gzip --parallel 4` → gz decompressed == plain N=1 output. |
| `smoke_parallel_mbias_table_byte_identical_at_n_in_1_2_4_8` | M-bias.txt content identical across N values. |
| `smoke_parallel_splitting_report_counts_match_across_n` | Splitting-report counters identical across N values. |
| `smoke_parallel_invalid_xm_byte_propagates_error_at_n4` | `--parallel 4` + invalid XM → exit 1 with InvalidXmByte message; partial outputs cleaned. |
| `smoke_parallel_pe_unpaired_final_record_err_at_n4` | Odd-record-count PE BAM + `-p --parallel 4` → exit 1 with UnpairedFinalRecord. |
| `smoke_parallel_combined_flags_at_n8` (rev 1, A.test-gap) | `--comprehensive --gzip --parallel 8` on synthetic BAM → output byte-identical to `--comprehensive --gzip --parallel 1` (decompressed). |
| `smoke_parallel_write_failure_mid_stream_cleans_up` (rev 1, A.test-gap) | Set output_dir read-only mid-record (or use a contrived I/O error injector); assert exit 1; assert no partial files remain. Wrapped in timeout. |
| `smoke_parallel_gzip_byte_identical_at_n1_and_n8` (rev 1, B.optional) | `--gzip` decompressed output byte-identical at N=1 and N=8 (covers both edges of the N range). |
| `smoke_parallel_empty_bam_n4` (rev 1, B.optional) | Empty PE BAM + `--parallel 4` → header-only files + zero counters in M-bias.txt + report. |

### 7.3 Profiling smoke (NOT in CI)

Documented as a manual step in §6 step 10. Asserts ≥ 4× speedup at N=4 on the 10M PE WGBS dataset at `~/Desktop/TrimG_Bismark_test/`. Not in CI because the dataset is large + local-only.

### 7.4 Phase B-E regression

`cargo test -p bismark-extractor` should pass all 201 prior Phase B-E tests plus the new Phase F ones. The Phase B-E tests exercise the legacy single-threaded path which Phase F preserves untouched.

## 8. Efficiency

### 8.1 Per-record cost vs Phase B-E

Phase B's `extract_se` body is roughly: `extract_calls` + per-call `route_call` (M-bias accumulate + counter + write). Phase F adds:
- **Channel send/recv overhead** (~50-100 ns per record). Negligible vs CIGAR walking + per-call I/O.
- **Cloned qname + chr per call** (~30-byte qname + ~5-byte chr clone). For ~5 calls/record, ~175 bytes/record extra allocation.
- **Reorder buffer insert/lookup** (`BTreeMap<u64, _>` O(log n) where n ≤ 40N). For N=8 = ~6 comparisons per record.
- **M-bias merge at finalize**: O(max_position × 3 contexts × N workers). For 150bp reads × N=8 = ~3600 entries summed once.

Net overhead at N=1: ~5-10% slower than Phase B's linear loop (channel + clone costs). Acceptable for the simpler architecture per §9.2 #1.

### 8.2 N=4 target

Per CLAUDE.md profile:
- Single-core extraction: 12.3 min on 10M PE WGBS
- Perl `--multicore 4`: 5.4 min (2.3× — limited by N× decompression)
- Phase F target N=4: ≤ 3.1 min (≥ 4× over single-core)

If profiling shows N=4 is < 4×, candidate diagnoses:
- Collector I/O is the bottleneck (12 gzip streams serialised through one writer). Mitigation: parallel-write per file in collector (each file is independent; collector could split into 12 writer-mini-threads).
- Reorder buffer holds too long because one worker is slow. Mitigation: increase producer→worker channel size.
- `Box<dyn Write + Send>` vtable cost compounds at high write rates. Mitigation: switch to enum static dispatch (Phase E §9.2 #2 deferred this).

### 8.3 Memory

In-flight max at N=8:
- Producer→worker: 8 × 32 = 256 records × ~500 bytes (BismarkRecord size) = ~128 KB.
- Worker→collector: 8 × 8 = 64 messages × ~80 bytes/RoutedCall × ~5 calls/record = ~32 KB.
- Reorder buffer: 320 entries × ~500 bytes/WorkerOutput = ~160 KB.

Total ~320 KB in-flight at N=8 on top of the existing per-record structures. Within reasonable bounds.

### 8.4 Profile target

Per §6 step 10 — profile post-implementation and assert ≥ 4×.

## 9. Assumptions + open questions

### 9.1 Locked assumptions

- **`MbiasTable::add` is commutative + associative**: scalar sum on each `MbiasPos { meth, unmeth }`. Tests assert.
- **`SplittingReport` counters are sum-reducible**: field-wise scalar sum. Tests assert.
- **`ThreadedBamReader` is the right BGZF reader**: dedup precedent (bismark-dedup pipeline.rs:483) confirms availability + behavior. No additional reader threading work needed.
- **PE pair-formation is producer's responsibility**: workers receive `BismarkPair` not individual mates. Producer's pairing logic mirrors Phase C's `extract_pe` loop.
- **input_idx is monotonic per message** (not per record). For PE, idx increments by 1 per pair but `records_in_message = 2` so `records_processed` counter increments by 2.
- **`rayon = "=1.10.x"` and `crossbeam-channel = "=0.5.x"`**: pinned to specific minor versions; `cargo tree` verification at implementation time will resolve to a single patch version per workspace dedup convention (Phase E established this).
- **Collector single-threaded writes to OutputFileMap**: no shared-mutable-state across threads on the write path. `+ Send` bound on the boxed writer satisfies "OutputFileMap value moves into the collector thread once at startup".

### 9.2 Open questions

1. **(Open, architecture)** N=1 path: keep threaded (rev 0 choice, §4.6) or short-circuit to legacy? Threaded keeps code-path uniform (no drift risk); short-circuit avoids ~5-10% N=1 overhead. Defer to post-Phase-F profiling — if N=1 overhead is a real concern (e.g. people running batched single-core jobs), add a `--parallel 1` short-circuit as a Phase F polish.
2. **(Open, optimization)** Pre-formatted output bytes in `RoutedCall`: currently workers send call+metadata; collector re-derives the row string. Could pre-format in worker (`Vec<u8>` row) and have collector do raw `write_all`. ~30% fewer collector cycles, +60% larger channel messages. Defer to post-merge profiling.
3. **(Open, optimization)** `Box<dyn Write + Send>` vs static-dispatch enum. Phase E deferred this to Phase F profiling. Resolve based on §6 step 10 measurements.
4. **(Open, infrastructure)** Per-file parallel-write in collector: each of the 12 split files is independent — collector could split into 12 mini-threads if writing is a bottleneck. Strictly a post-Phase-F optimization.
5. **(Resolved)** Channel library: `crossbeam-channel`. Resolved at plan-write time.
6. **(Resolved)** PE pair-formation site: producer thread. Resolved at plan-write time.

### 9.3 Critical questions

**None.** All architecture choices are locked or default to a sensible answer per SPEC §6.4/§9. The `dedup-style ThreadedBamReader-only` alternative is explicitly considered + rejected for rev 0 (see §2 first row) but listed as a fallback if profiling fails the ≥ 4× target.

## 10. Validation

| What to verify | How | Expected |
|----------------|-----|----------|
| `MbiasTable::add` commutativity | `mbias_table_add_is_commutative` unit test | `a + b == b + a` byte-identical. |
| `MbiasTable::add` associativity | `mbias_table_add_is_associative` unit test | `(a + b) + c == a + (b + c)` byte-identical. |
| `SplittingReport::add_delta` | unit test on field-wise sum | each field sums correctly. |
| Byte-identity at N=1 vs legacy `extract_se` | `pipeline_n1_synchronous_handoff` + `smoke_se_parallel_n1_byte_identical_to_legacy_extract_se` | All 12 split files byte-identical. |
| Byte-identity at N=4 vs legacy | `smoke_se_parallel_n4_byte_identical_to_legacy_extract_se` + PE counterpart | Byte-identical at N=4. |
| Byte-identity at N=8 | Same shape | Identical at N=8. |
| Output ordering correctness | `collector_reorders_out_of_order_arrivals` + `collector_blocks_until_next_emit_idx_arrives` | Records written in strict input order regardless of worker completion order. |
| Critical-1 regression in parallel path | `smoke_parallel_yacht_n4_byte_identical_including_reverse_strand_col6_col7` | OB-strand yacht rows have col-6 > col-7 even at N=4. |
| `--mbias_only` counters match across N | `smoke_parallel_mbias_table_byte_identical_at_n_in_1_2_4_8` + `smoke_parallel_splitting_report_counts_match_across_n` | M-bias.txt + splitting-report bytes identical for N ∈ {1, 2, 4, 8}. |
| Error propagation | `worker_loop_propagates_extract_calls_err` + `smoke_parallel_invalid_xm_byte_propagates_error_at_n4` | Errors surface at N=4 same as N=1; partial outputs cleaned. |
| Phase B-E regression | `cargo test -p bismark-extractor` | All 201 prior tests + new Phase F tests pass. |
| Speedup target | Manual profile on 10M PE WGBS (per §6 step 10) | ≥ 4× wall-clock at N=4 vs N=1. |
| Clippy + fmt | `cargo clippy -- -D warnings && cargo fmt --check` | Clean. |

## 11. Integration with later phases

| Phase | What Phase F leaves for it |
|-------|----------------------------|
| **G** (bedGraph + cytosine_report) | Phase F's `OutputFileMap` writes the same split-file format. Phase G consumes those files as subprocess input to `bismark2bedGraph` (Perl). `--parallel N` doesn't change the file format Phase G consumes. |
| **H** (real-data byte-identity gate) | Phase H runs the full 55.7M PE WGBS dataset at `--parallel 1` and `--parallel 8`. Phase F's invariant ("`--parallel N` output == `--parallel 1` output") is exactly what Phase H asserts at scale. |

## 12. Self-review

**Efficiency.** Channel send/recv ~50-100 ns per record. Cloned qname + chr ~175 bytes/record. Reorder buffer O(log 40N) per insert. M-bias merge once at finalize. Net N=1 overhead 5-10%, N=4 target ≥ 4× speedup. Profiling at §6 step 10 validates.

**Logic.** Producer → workers → collector is the canonical 3-stage pipeline. Worker statelessness (per-message function call) makes byte-identity straightforward. Collector's `BTreeMap<u64, _>` reorder buffer preserves input order. M-bias commutative+associative sum makes worker-count-independent. PE pair-formation in producer keeps workers pure. Error propagation via first-Err-wins + drain matches the existing Phase B-E error model.

**Edge cases.** Empty BAM, single record SE, PE orphan R1, mid-stream read error, invalid XM, `--mbias_only`, `--gzip` (gzip footer write happens on collector OutputFileMap drop — single thread, so no race), large N. All covered.

**Integration.** Phase B-E single-threaded `extract_se` / `extract_pe` remain untouched in lib.rs (called from tests as the byte-identity reference). New Phase F entry points are `extract_se_parallel` / `extract_pe_parallel`. `main.rs::run` is the only production dispatch change.

**Risks remaining.**

- **R1**: Collector I/O could be the bottleneck (single thread writing 12 gzip streams). Mitigation listed in §8.2.
- **R2**: M-bias merge could allocate large arrays if max_position is high. Mitigation: pre-allocate per-worker tables with the expected max read length (150bp typically); growth is rare.
- **R3** (rewritten rev 1): Channel deadlock if producer crashes mid-stream. **Mitigation by design**, not by code: the EOS model is channel-disconnect (rev 1 per Reviewer A C3 / Reviewer B G1). When the producer panics or returns Err, its Sender is dropped (panic unwinds Drop normally); workers see `Err(RecvError::Disconnected)` on next `recv()`, emit their `FinalDelta`, and exit cleanly. **No N-sentinel synthesis** (which `Drop` can't do anyway). Regression-guarded by `producer_panic_does_not_deadlock_workers` test under a 30s timeout.
- **R4**: PE pair-formation in producer means the producer holds **two** records concurrently. Memory is bounded; no leak risk.
- **R5**: `Box<dyn Write + Send>` static-dispatch is still deferred (Phase E §9.2 #2). Phase F profiles whether it's a real cost.

## 13. Revision history

- **rev 0** (2026-05-27): initial Phase F plan written. Awaiting manual review → dual plan-reviewer pass → implementation trigger.
- **rev 1** (2026-05-27, same-day): absorbed dual plan-review findings (`PLAN_REVIEW_PHASE_F_A.md` + `PLAN_REVIEW_PHASE_F_B.md`). Six Critical fixes: (C1/G3) `MbiasTable::add` pseudocode rewritten against the actual `Vec<MbiasPos> × 3` struct (rev 0 cited non-existent `get`/`set`/`ensure_capacity`/`max_position(ctx)` API); (C2/G4) `SplittingReportDelta` separate type dropped — reuse `SplittingReport` directly with new `add(&Self)` method; (C3/G1) committed to channel-disconnect-as-EOS (worker `Err(RecvError::Disconnected)` → emit FinalDelta + exit), dropped the structurally-impossible "Drop impl sends N sentinels" model from §12 R3; (C4) `WorkerOutput::Err` now carries `input_idx`, collector picks lowest-idx Err for deterministic stderr byte-identity; (G2) producer thread join + panic propagation lifecycle spelled out in §4.5 with code sketch. Importants absorbed: (G7) `--mbias_only` workers emit empty `routed_calls` rather than `key: None` calls — saves channel traffic; `RoutedCall.qname` becomes `Arc<[u8]>` (atomic-inc clone across record's calls); `RoutedCall.chr` becomes `chr_id: u32` resolved via shared `Arc<[String]> chr_table` (~5 GB allocation pressure saved on 50M-read run); (G10) rayon scoped-pool isolation constraint documented (workers must NOT call into APIs using rayon's global pool); legacy `extract_se`/`extract_pe` get an explicit "DO NOT DELETE — byte-identity reference" stability commitment. Tests strengthened: 30-second universal timeout guard on every parallel test (Reviewer B V2); 8 new unit tests covering producer panic, deterministic Err selection, mbias_only worker short-circuit, qname Arc identity, chr_id resolution, producer/worker panic propagation, and a property-based N=8 reorder buffer test (Reviewer A V1 / Reviewer B V1); 4 new smoke tests covering combined-flag matrix at N=8, write-failure mid-stream cleanup, gzip byte-identity at N=1 + N=8, and empty-BAM at N=4. Channel-sizing flip (Reviewer A.Important) and pre-format-row optimisation (Reviewer A.Important) left as in-implementation measurements with documented thresholds — neither overrides SPEC §9.2 in rev 1.

## 14. Sub-issue (already filed)

[#860](https://github.com/FelixKrueger/Bismark/issues/860) (filed at work-start, child of #798).

## 15. Branching strategy

- **Branch:** `extractor-phase-f` off **current** `rust/iron-chancellor` (which now contains all Phase A→E squash commits).
- **PR target:** `rust/iron-chancellor` directly (Phase E established the cascade-merge pattern; no stacking needed for Phase F since it's at the head of the queue).
- **Rebase risk:** low — Phase F touches `route.rs` (pure-helper extraction), `mbias.rs` + `output.rs` (additive `add` methods), `main.rs::run` (single-line dispatch change), `pipeline.rs` (no change — legacy `extract_se`/`extract_pe` stay). New module `parallel.rs` + new test files. Conflicts only possible if a parallel Phase G or polish PR touches the same files concurrently — unlikely given the merge ordering.

## 16. Follow-up tasks

Tracked as deferred items (NOT in scope for Phase F):

- **Phase E code-review deferreds** (CODE_REVIEW_A/B.md): M3/L-4 test helper duplication via `tests/common/mod.rs`, M-1 yacht col-4 byte-pin, L1–L5 hygiene items. Could be batched into a "Phase E polish" PR before or after Phase F.
- **Phase F polish opportunities** (§9.2): N=1 short-circuit, pre-formatted output bytes in `RoutedCall`, static-dispatch writer enum, per-file parallel-write in collector. All defer to profiling-driven decisions post-Phase-F merge.
- **SPEC §9.7 speedup target verification**: profile on the full 55.7M PE WGBS dataset at Phase H, not just the 10M baseline at Phase F.

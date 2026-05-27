# Phase F Code Review — Reviewer B

**Target:** `rust/bismark-extractor` Phase F implementation against `plans/05262026_bismark-extractor/PHASE_F_PLAN.md` rev 1.

**Scope read end-to-end:** `parallel.rs` (940 LOC), `mbias.rs::add`, `output.rs::SplittingReport::add`, `route.rs::compute_yacht_columns`, `main.rs::run`, `lib.rs` re-exports, `tests/parallel_phase_f.rs` (851 LOC), `Cargo.toml`, plus the relevant call-sites in `pipeline.rs`, `state.rs`, `call.rs`.

**Verdict:** Implementation is solid. Byte-identity invariant is mechanically sound. Most of the brief's probed concerns are handled correctly. I found **one substantive issue** (worker thread panic propagation has a latent hang risk) and several minor/clarity items.

---

## Summary

The producer/worker/collector architecture matches the plan; deviations are documented. The deterministic-Err selection (`update_best_err` by lowest `input_idx`) plus the `BTreeMap`-keyed reorder buffer make byte-identity to the legacy path mechanically straightforward, and the test surface exercises N ∈ {1, 2, 4, 8} on every output mode. Test count claim (223 pass / 0 fail) is consistent with the test file structure I reviewed; I did not re-run the suite.

The biggest concern is what happens on worker panic mid-stream: the collector waits for N FinalDeltas, and a panicked worker emits zero. The code has a fallback (`Err(RecvError)` on the worker→collector channel triggers a synthesised InternalError), but the channel-disconnect only fires when **all** worker `tx_output` clones drop. See Critical-1 below.

---

## Critical

### C1 — Worker panic mid-stream can hang the collector indefinitely (latent — only triggers on panic)

`run_pipeline` clones `tx_output` into each worker. The main thread drops its own `tx_output` clone at line 218 immediately after spawning. So `tx_output` is live as long as **at least one** worker thread holds its clone.

If one worker panics, that worker's `tx_output` is dropped by unwind (good). The remaining N-1 workers continue running and will each eventually emit their `FinalDelta` on EOS. The collector receives N-1 FinalDeltas total and is waiting for the Nth — which never comes. **The collector's `rx_output.recv()` does NOT return Disconnected until ALL tx_output clones are dropped.** So the collector blocks until the surviving workers themselves disconnect — which only happens when the producer drops `tx_input` (channel disconnects → workers see Disconnected → emit FinalDelta → exit → drop tx_output). 

So actually: this resolves itself **as long as the producer eventually exits**. The producer always exits when the BAM stream ends. So in practice the collector unblocks. But the message count is off: the collector receives only N-1 FinalDeltas (one worker panicked before emitting), then receives `Err(RecvError)` when the surviving workers exit. The fallback at lines 802-819 catches this:

```rust
if best_err.is_none() && finaldeltas_received < n_workers {
    best_err = Some((u64::MAX, BismarkExtractorError::InternalError {
        message: format!(
            "collector received only {finaldeltas_received} of \
             {n_workers} FinalDeltas before channel disconnect — \
             a worker likely panicked or exited unexpectedly"
        ),
    }));
}
```

So **on the normal exit-path-after-panic**, the error surfaces. Good.

**Where it bites:** if the producer ITSELF panics or stalls, AND a worker also panics, the surviving workers never see channel-disconnect on `rx_input` (since other workers still hold rx_input clones — actually no, `rx_input` is dropped in the producer's `tx_input` clones only). Wait — let me re-trace. `rx_input` is cloned into each worker. Main drops its rx_input at line 217. Surviving workers hold N-1 rx_input clones. Producer panics → its `tx_input` drops → `rx_input` sees Disconnected (only when all tx_input clones drop — and only the producer holds tx_input clones now). So workers see Disconnected, emit FinalDelta, exit, drop tx_output. Channel disconnects. Collector unblocks.

**Conclusion:** the cascading-drop semantics make this self-healing in all paths I can model. No actual deadlock. The fallback at L802-819 catches the missing FinalDelta case. **Demote to High** — but worth a comment in the code that documents this reasoning, because the read of the code suggests a hang and the proof-of-non-hang is non-local.

### C2 — `worker_panic` detection is silently overridden by `merge_results` on collector Err

`run_pipeline` does:

```rust
let pipeline_result = match merge_results(collector_result, producer_join_result) {
    Err(e) => Err(e),
    Ok(()) => match worker_panic {
        Some(msg) => Err(BismarkExtractorError::InternalError { message: msg }),
        None => Ok(()),
    },
};
```

If a worker panics AND the collector also reports an Err (which it will, per the L802-819 fallback: synthesised `InternalError` with "received only N-1 of N FinalDeltas"), `merge_results` returns the collector Err — which is the *synthesised* InternalError, NOT the actual panic payload. The actual `worker_panic` string (which contains the panic message via `{panic_payload:?}`) is **discarded**.

The user gets:
> error: internal error: collector received only 3 of 4 FinalDeltas before channel disconnect — a worker likely panicked or exited unexpectedly

…but not:
> worker thread panicked: "called `Result::unwrap()` on an `Err` value: ..."

The actually-informative message is dropped on the floor. Precedence is wrong: a real panic payload is more diagnostically valuable than the "I noticed something is off" synthesised message.

**Fix:** invert the precedence when the collector error is the synthesised "missing FinalDeltas" one — or, more simply, format both messages into the resulting Err when both are present. A simple structural improvement:

```rust
let pipeline_result = match (collector_result, producer_join_result, worker_panic) {
    (_, _, Some(panic_msg)) => Err(BismarkExtractorError::InternalError { message: panic_msg }),
    (Err(e), _, None) => Err(e),
    (Ok(()), Err(p), None) => Err(InternalError { message: format!("producer panicked: {p:?}") }),
    (Ok(()), Ok(()), None) => Ok(()),
};
```

— worker_panic wins because it's the root cause; the collector's "missing FinalDelta" is a symptom of it.

**Brief item 1 verification:** the brief asks about collector-Err-vs-producer-panic precedence. The plan trade-off there is actually OK (collector Err is more specific than a generic panic — see L283-284 fallback). But worker-panic precedence is wrong as described above.

---

## High

### H1 — `chr_id: u32` silently truncates `usize` reference_sequence_id (defensive guard missing)

Producer L309 and L392:

```rust
let chr_id_opt = record.inner().reference_sequence_id().map(|r| r as u32);
// ...
let chr_id = match pair.r1().inner().reference_sequence_id() {
    Some(r) => r as u32,
    ...
};
```

`reference_sequence_id` returns `Option<usize>`. For BAMs with > 2^32 reference sequences (~4.3 billion contigs), `r as u32` silently truncates. Human/mouse genomes are nowhere near this — but assemblies with a sea of small contigs (some plant/fungal/de-novo) can blow past tens of thousands and theoretically (with bizarre inputs) more.

The collector resolves chr_id via `chr_table.get(chr_id as usize)` — if the original chr_id was > 2^32, the truncated u32 might map to a DIFFERENT chr in the table, producing silently-wrong output rows.

**Fix:** use `u32::try_from` instead of `as u32` and emit `InternalError` on overflow:

```rust
let chr_id = u32::try_from(refid).map_err(|_| BismarkExtractorError::InternalError {
    message: format!("chr_id {refid} overflows u32; recompile with chr_id: u64"),
})?;
```

Or just use `u64` for chr_id throughout — 8 extra bytes per `RoutedCall` is negligible vs the Arc-pointer it already carries. (`route.rs::compute_yacht_columns` already does `u32::try_from(...)` for alignment_start and reference_end — precedent supports the defensive guard.)

**Severity:** High because it's silent miscompute, not crash. Practical impact is currently nil. Worth fixing now before the bug-shape ossifies.

### H2 — `worker_loop` does not enforce the "Err then exit" invariant from producer side

The brief raised this. Producer always sends `WorkerInput::Err` and then **immediately returns** (line 336, 351, 364, 374, 386, 399 — all are `return` after `let _ = tx_input.send(WorkerInput::Err {...})`). So in practice no `Se`/`Pe` messages follow an `Err`. Good.

But `worker_loop` doesn't enforce this — on `Ok(WorkerInput::Err)` it forwards and continues looping (line 489-498). If the producer were ever modified to send Err and continue, the worker would happily process subsequent messages and emit `Ok` outputs after the Err — likely fine for byte-identity (lowest-idx Err wins anyway), but the invariant should be documented.

**Recommendation:** add a comment at the worker's `WorkerInput::Err` branch:

```rust
// Producer always returns immediately after sending Err (see producer_loop
// for the policy). The worker forwards the Err and continues draining
// only so that any in-flight messages already in the channel are processed
// for deterministic byte-identity.
```

### H3 — `rayon` is a dead dependency

Cargo.toml line 35 pins `rayon = "=1.10.0"` but no source file imports it. The module docstring (parallel.rs L42-60) explicitly documents the deviation from rayon to `std::thread::spawn`. The brief flagged this.

**Recommendation:** either drop the dep (one-line Cargo.toml change, ~150 KB of compile dependencies saved) or actually use it for the per-file collector parallelism that the plan §8.2 contemplates as a future optimisation. **Drop now is the better choice** — re-adding is trivial when the optimisation lands, and dead deps are noise.

### H4 — `mbias_only_silence` and `mbias_only` are bound to the same value, redundantly

Lines 432-433:

```rust
let mbias_only_silence = config.is_mbias_only();
let mbias_only = config.is_mbias_only();
```

These are the same value, used at different sites for different purposes (`mbias_only_silence` → `extract_calls`; `mbias_only` → routing skip). The variable redundancy is a clarity issue, not a bug — but a future reader will wonder why both exist.

**Fix:** bind once: `let mbias_only = config.is_mbias_only();` and use it at both sites. The `extract_calls` parameter name is `mbias_only_silence` but you're passing the same `bool` value either way — no semantic difference for this caller.

---

## Medium

### M1 — `update_best_err` "equal idx keeps existing" path is unreachable in practice

Lines 836-842: on equal-idx, keeps existing. The brief asks about this. By construction, each `input_idx` is owned by exactly one message: the producer assigns idx monotonically per record/pair, and a worker either processes one Ok or forwards one Err at that idx. Two distinct errors at the same `input_idx` cannot occur. The `Some(_) => {}` branch is defensive but unreachable; the unit test `update_best_err_equal_idx_keeps_existing` exercises a synthetic case.

Not a bug. Worth a comment explaining the invariant so the dead branch isn't optimised away by a future cleanup that breaks the invariant.

### M2 — Reorder buffer can grow beyond the brief's claimed 40N bound

The brief's item 7 estimates `producer_channel + worker_channel = N × 32 + N × 8 = 40N` as the buffer bound. This is an **upper bound on simultaneously in-flight messages**, but the reorder buffer also holds messages whose `input_idx > next_emit_idx`. If one worker is slow and gets stuck on idx 5, every higher idx arriving from other workers piles up.

In practice the producer back-pressure bounds the worker-channel queue. Once the buffer holds K entries, the worker→collector channel is empty, so the workers can keep producing only until *their* output channel fills (which is bounded). Net effect: the buffer can grow to ~`worker_channel_size = 8N` entries beyond `next_emit_idx` (each worker produces ~8 idx, all pending in collector). For N=64 (an extreme case the plan considers acceptable), that's ~512 entries × ~50 bytes/RoutedCall × ~5 calls/record = ~125 KB. Bounded.

But — the **worst case** is one worker stalled (e.g. on a `write_all` that's blocking on the OS) while N-1 workers produce. The producer→worker channel won't drain because the stalled worker holds rx_input. So producer blocks. The other workers drain rx_input and dump to rx_output. The rx_output channel fills up. They block on `tx_output.send`. Total in-flight: bounded by channel sizes. The reorder buffer specifically: bounded by `tx_output` channel size = 8N entries. Fine.

Bottom line: the brief's 40N estimate is correct as a rough cap. No unbounded growth. Note as documentation.

### M3 — Collector's "drain after best_err" doesn't actually drain remaining `Ok` messages cleanly

After the first Err arrives, the collector continues looping until N FinalDeltas. But: if the Err was at `input_idx=0`, `next_emit_idx` is 0 and stays stuck there (no Ok ever arrives at 0). Subsequent Ok arrivals at idx 1, 2, ... are inserted into the reorder buffer but never drained out. They accumulate until N FinalDeltas come in, at which point the loop exits and the buffer is dropped.

This wastes work (the workers processed those records pointlessly) but is correct — the cleanup path drops everything.

**Note:** there's a minor optimisation opportunity here: if `best_err.is_some()`, the collector could send a "shutdown" signal to the producer to make it stop. But there's no producer→collector backchannel, so this would need a second crossbeam channel. Plan §9.2 #1 already calls out the "ignore — keep architecture uniform" trade-off. Acceptable.

### M4 — Test `legacy_vs_parallel_n4_se_default_byte_identical` does not catch interleaved-write hazards (Phase H concern but worth noting)

The test fixture (`write_se_directional_bam`) has only 5 records. With 4 workers, you'd get at most ~1 record per worker. Most idxs land in one worker. The race conditions that the reorder buffer protects against can't manifest. The cross-N test (`parallel_se_byte_identical_across_n_1_2_4_8`) is similarly small.

To stress-test reorder semantics, you'd want hundreds of records with interleaved completion order. This is mostly a Phase H concern — Phase H will run the 55.7M-read dataset and would surface any ordering bug at scale. For Phase F, the unit-test `pipeline_n8_reorder_buffer_property_test` listed in the plan §7.1 row "rev 1, V1" would have provided that synthetic stress test — but I do not see it in the implemented `tests/parallel_phase_f.rs`. **Plan-vs-implementation gap.**

Specifically these planned tests are MISSING from the implemented test file:
- `pipeline_n8_reorder_buffer_property_test` (V1)
- `producer_panic_does_not_deadlock_workers` (rev 1, C3/G1)
- `collector_picks_lowest_input_idx_err_on_multiple_worker_errors` (rev 1, C4) — covered partly by the in-`parallel.rs::tests::update_best_err_picks_lowest_input_idx` unit test but not at the pipeline level
- `worker_qname_arc_shared_across_record_calls` (rev 1)
- `worker_thread_panic_propagates_as_internal_error` (rev 1)
- `producer_thread_panic_propagates_as_internal_error` (rev 1)
- `smoke_parallel_write_failure_mid_stream_cleans_up` (rev 1, A.test-gap)
- `smoke_parallel_combined_flags_at_n8` (rev 1, A.test-gap)

The brief mentions "15 tests" in `parallel_phase_f.rs`. The plan §7.1 lists ~25 unit tests + ~17 smoke tests. The implementation has 15. **Coverage is materially short of the plan.** Some are deferred (timeout guard) or covered indirectly, but the panic-propagation tests and the property-based reorder test are load-bearing for the byte-identity invariant and should not be deferred.

### M5 — `normalize_report` strips ONLY `Input file:` and `Output directory:` — verified complete

Brief item 8 asks whether these are the only two path-dependent lines. Cross-checked against `output.rs::write_splitting_report` (L306-385):
- `Input file:` (L321) — stripped ✓
- `Output directory:` (L322) — stripped ✓
- `Bismark methylation extractor version v0.25.1` — hardcoded, no normalization needed
- `--fasta`, `--ignore`, `--ignore_3prime` — config-dependent but identical across the two compared configs
- counter lines — load-bearing, must match

No third path-dependent line exists. Normalization is correct.

### M6 — Cross-N gzip byte-identity not tested at the actual byte level

`parallel_gzip_n4_decompresses_identical_to_legacy_plain` decompresses the gz output and compares to plain. That's good. But for cross-N gzip byte-identity (i.e. is the raw .gz file byte-identical at N=1 vs N=4?), there's no test. Since each split file is written by a single collector thread (single-writer), and `flate2`'s output is deterministic for a given input byte stream + compression level, the raw .gz should also be byte-identical. The plan called this out in `smoke_parallel_gzip_byte_identical_at_n1_and_n8` (V1). **Missing test.**

This matters because Phase H asserts byte-identity at scale, and if Phase H is checking raw bytes (likely faster than decompress), a divergence in gz block boundaries would be caught only there.

---

## Low

### L1 — `is_paired` mode is passed via `bool` argument all the way through `run_pipeline`

Not a bug. Could be cleaner via `PairedMode` or a dedicated enum. The `bool` is fine for now.

### L2 — Worker name format `bismark-extractor-worker-{i}` doesn't include the PID/session

If a user has multiple bismark-extractor-rs processes running, thread names collide in `top -H`. Trivial. Tools generally show PID context anyway.

### L3 — `Arc::clone(&qname_arc)` per call inside `process_se`/`process_pe` loop

Brief item 6. Yes, ~5 atomic-inc per record (acq+rel). At 10⁹ calls/run that's 5×10⁹ atomic ops. The atomic-inc on Arc is ~2-3 ns each on contemporary x86. Total ~10-15 seconds spent in Arc cloning over a 30-minute run. Negligible. Even halving via a pre-built `Vec<Arc<[u8]>>` shared across calls would save < 1% of runtime.

Not a concern. Profile-driven optimization if profiling shows it.

### L4 — Test helpers duplicated from earlier phases

`synth_record`, `header_with_chr1`, `write_se_directional_bam`, `write_pe_directional_bam`, etc. are copied from `tests/se_phase_b_smoke.rs` and `tests/output_modes_phase_e_smoke.rs`. Plan §7.1 acknowledges this and defers `tests/common/mod.rs` to a polish PR. Acceptable.

### L5 — `flate2 = "=1.1.9"` is workspace-wide but worth a workspace-deps lift

If three crates pin the same version, a `[workspace.dependencies]` entry would centralise it. Not a Phase F concern — broader refactor.

### L6 — `merge_results` is unit-test-untested

The L275-286 function has no dedicated unit test (`parallel.rs::tests` only tests `update_best_err`). The function is small but worth a quick test, especially given the C2 concern about precedence. Would catch the worker-panic-vs-collector-err interaction.

### L7 — `n_workers = config.parallel.max(1)` is redundant — `Cli::validate` already enforces `>= 1`

Line 182. The validation is documented at the `extract_se_parallel` rustdoc. The `.max(1)` is belt-and-suspenders, but if `Cli::validate` accepts `0`, we'd silently coerce. Either rely on validate or add a debug_assert. Trivial.

### L8 — `process_pe` recomputes `pair.r1().inner().reference_sequence_id()` to check vs `chr_id`

Brief item 4. The producer at L391 grabs R1's refid into `chr_id`. The worker at L603 checks R2's refid. Different reference — no duplication. Correct cross-chr defensive check.

The brief also asks: "could an R2-unmapped pair slip past producer + only get caught in worker?" Yes — and it does get caught in the worker at L603-607 (`r2_refid` is unwrap-or-Err). Good.

### L9 — `process_se`'s defensive PAIRED-flag check (L522-530) only fires if a PE record is somehow routed to SE

Mirrors `pipeline.rs::extract_se`'s defensive check. Returns `PhaseNotYetImplemented` with the message "paired-end extraction (input has PAIRED flag set); PE arrives in Phase C". That message is now stale — Phase C shipped. The message string should be updated to something like "SE pipeline received a PAIRED record; use `-p` or auto-detect to route to PE". Minor.

### L10 — `RoutedCall` does not derive `Debug`

Hampers debug printing during test failures. `#[derive(Debug)]` would help future debugging at near-zero cost (Arc<[u8]> is printable as bytes via Vec-like printing). Trivial.

---

## Items from the brief — verification table

| Brief item | Verdict |
|---|---|
| 1. `merge_results` precedence | C1 + C2 above. Worker-panic precedence wrong (C2). Collector-Err-vs-producer-panic OK. |
| 2. `chr_id u32` truncation | H1. Confirmed silent-truncation risk; fix with `u32::try_from`. |
| 3. `worker_loop` Err-then-continue invariant | H2. Doc-only fix. |
| 4. PE pair-formation refid checks | OK. Producer checks R1; worker checks R2. No duplication; covers cross-chr + R2-unmapped. |
| 5. `records_processed` double-counting | **Verified clean.** Only worker `process_se/pe` increments `report.records_processed`; collector `state.report.add(&report)` sums. No second increment site exists for the collector's `state.report`. |
| 6. `Arc::clone` overhead | L3. Negligible. |
| 7. Reorder buffer bound | M2. Roughly 40N upper bound; correct for practical N; no unbounded path. |
| 8. `normalize_report` completeness | M5. Confirmed complete; only the 2 path lines are dynamic. |
| 9. Byte-identity meaningful given normalize | OK for split files (strict compare); marginal for report. Phase H will use same-dir replay anyway. |
| 10. Error-string variation across N | OK. `InvalidXmByte`'s Display is the same regardless of N; deterministic-Err selection makes the surfaced error byte-identical. |
| 11. Test thread disposal | OK. `std::thread::spawn` returns JoinHandle; all joined before run_pipeline returns. No leaks. |
| 12. N=1 vs N=4 byte-identity for empty input | M4 (test gap). `parallel_empty_bam_at_n4_produces_header_only_files` only tests N=4 against expected literal content; no cross-N comparison. Add it. |
| 13. Rayon necessity | H3. Dead dep; drop. |

---

## Test surface assessment

**Implemented:** 15 tests in `tests/parallel_phase_f.rs` + 2 small unit tests in `parallel.rs::tests`. Covers:
- Legacy vs parallel (N=1, N=4) for SE Default + PE Default
- Cross-N (N=1,2,4,8 SE; N=1,4,8 PE) for Default
- Mode-specific at N=4: Comprehensive, MergeNonCpG, Yacht (with Critical-1 col-6/col-7 invariant check), MbiasOnly, Gzip
- Error propagation at N=4: InvalidXmByte, UnpairedFinalRecord, mbias_only_silence
- Edge case: empty BAM at N=4
- N=1 PE parallel matches legacy PE

**Planned but absent (per plan §7.1):** 8 specific tests listed above in M4. Most importantly:
- No panic-propagation test (`producer_panic_does_not_deadlock_workers`, `worker_thread_panic_propagates_as_internal_error`) → C1/C2 are unprotected against regressions
- No property-based reorder test (`pipeline_n8_reorder_buffer_property_test`) → reorder semantics rely on small synthetic fixtures
- No `smoke_parallel_write_failure_mid_stream_cleans_up` → cleanup-on-error path under parallel is untested
- No combined-flag matrix test at N=8

The Critical-1 (yacht reverse-strand) regression IS guarded by `parallel_yacht_n4_byte_identical_to_legacy` plus the in-test assertion. Good.

**Timeout guard:** Plan §7 rev 1 mandated a 30s timeout on every parallel test. **Not implemented.** Tests use `std::thread::spawn` indirectly via the pipeline but the outer test body has no timeout. A deadlocked test will hang CI indefinitely. This is the rev 1 commitment from Reviewer B V2 that was dropped.

---

## Recommendations (priority-ordered)

1. **C2** — Fix worker_panic precedence in `merge_results`; surface the panic payload over the collector's synthesised "missing FinalDelta" error. ~5-line fix.
2. **H1** — Replace `as u32` with `u32::try_from(...)?` for chr_id at L309, L392. ~10-line fix.
3. **M4** — Add the planned-but-missing tests, especially the two panic-propagation ones and the property-based reorder test. These are the load-bearing regression guards for the byte-identity invariant.
4. **§7 timeout guard** — implement the 30s test timeout that rev 1 committed to. Otherwise CI is vulnerable to indefinite hangs on regression.
5. **H3** — Drop the unused `rayon` dependency or stub it with a real call-site.
6. **H4** — De-duplicate `mbias_only_silence` and `mbias_only` bindings.
7. **C1** — Add a `// Why this doesn't deadlock on worker panic:` comment block at the collector's `Err(RecvError)` branch. The reasoning is non-local and load-bearing.
8. **M6** — Add raw-bytes gz cross-N comparison test.

---

## Verdict

**Approve with required follow-ups.** The implementation is functionally correct on the happy path; the byte-identity machinery is mechanically sound. The two issues that matter for production are: (a) chr_id truncation (silent miscompute risk on pathological inputs), and (b) worker-panic precedence in error reporting (lose-the-real-error-on-panic). Both are small fixes.

The test gap is the bigger concern strategically — Phase H is the last gate before release, and Phase H expects to validate the byte-identity invariant at scale. Phase F's tests don't currently exercise the panic-propagation path or the reorder buffer's correctness at scale; if Phase H surfaces a regression there, debugging will be harder than it should be.

---

**Reviewer:** B (independent context window)
**File:** `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/CODE_REVIEW_PHASE_F_B.md`

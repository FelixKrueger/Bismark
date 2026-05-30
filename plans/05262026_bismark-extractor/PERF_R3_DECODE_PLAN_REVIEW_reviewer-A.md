# Plan Review — PERF_R3_DECODE_PLAN.md (Reviewer A)

**Target:** `/Users/fkrueger/Github/Bismark-extractor/plans/05262026_bismark-extractor/PERF_R3_DECODE_PLAN.md`
**Code base:** worktree `/Users/fkrueger/Github/Bismark-extractor` (detached @ `b2af4e5`)
**Reviewer:** A (independent, fresh context)
**Verdict:** **APPROVE WITH MINOR CHANGES.** The single load-bearing risk (record-order preservation under the multithreaded reader) is *proven safe* by the noodles 0.47.0 source + an empirical test of the exact adaptation. No Critical findings. One Important finding (the "default benefits at 1 worker" perf assumption is the real open question and the plan correctly flags it as a perf-gate, not a correctness gate). A few Optional accuracy fixes to the plan text.

I verified the plan by (a) reading the noodles `MultithreadedReader` source, (b) actually applying the plan's three edits to `parallel.rs` and running the full `parallel_phase_f` byte-identity suite + clippy, then reverting. **All 18 byte-identity tests passed; clippy clean.**

---

## 1. Logic review

### 1a. Load-bearing risk: record-order preservation — RESOLVED (safe)
This is THE risk and the plan rests its byte-identity contract on it. I did not take the "noodles contract" claim on faith — I read the pinned source.

`noodles-bgzf =0.47.0`, `src/io/multithreaded_reader.rs`:
- `spawn_reader` (`:364`) reads BGZF frames **sequentially** from the inner `File`. For each frame it creates a *per-block* one-shot channel `(buffered_tx, buffered_rx)` (`:383`), hands the compressed buffer to a shared inflate pool, and pushes `buffered_rx` onto the `read_tx` FIFO **in frame order** (`:385–386`).
- The consumer side `recv_buffer` (`:352`) pops `buffered_rx` handles off `read_rx` in FIFO (= frame) order and blocks on that *specific* block's `buffered_rx.recv()`.

So although inflaters (`spawn_inflaters` `:393`) run in parallel and may *finish* out of order, the consumer pulls inflated blocks back in **strict block order** via the per-block one-shot channels. The decompressed byte stream feeding `noodles_bam::io::Reader` is byte-for-byte identical to the single-threaded `bgzf::io::Reader`. Therefore record order is identical. **The plan's load-bearing assumption is correct by construction, not just by contract.**

### 1b. Empirical confirmation of the adaptation
I applied the plan's exact three edits (imports + `const DECODE_THREADS` + always-BAM selection + `ProducerReader` enum + `producer_loop` signature) and ran:
- `cargo test -p bismark-extractor --test parallel_phase_f` → **18 passed, 0 failed**, incl. `legacy_vs_parallel_n1_*` (the new threaded-path-at-N=1 comparison), `parallel_se_byte_identical_across_n_1_2_4_8`, `parallel_pe_byte_identical_across_n_1_4_8`, the 8199-record multibatch, and the gzip-multibatch test.
- `cargo clippy --all-targets` → clean (no warnings/errors).
Then reverted via `git checkout`. The working tree is clean.

This is the strongest possible evidence for "adaptation correctness" (plan scrutiny item 4): the change compiles, the const-init form works, and byte-identity holds at N=1 (now running the threaded reader) through N=8.

### 1c. Adaptation cleanliness vs R1 batching divergence — CONFIRMED CLEAN
Scrutiny item 4 asks whether R1 batching touches the reader in ways the plan misses. It does not. Grep of `parallel.rs` shows the reader is touched at exactly three sites:
- `parallel.rs:211` `reader.header()` (chr_table)
- `parallel.rs:220` `logger.header_provenance(reader.header())`
- `parallel.rs:406` `let mut records_iter = reader.records();`

All R1 batching logic (`flush_batch!`, `batch_seq`, PE pairing, error-slot retention) operates on `records_iter` (the `Box<dyn Iterator>`), never on the reader itself. The `ProducerReader` enum's `header()` + `records()` cover all three sites. The d3dd289 enum applies verbatim; the only delta from #887 is dropping the `n_workers >= 2` gate and using `DECODE_THREADS` instead of `n_workers`. **No hidden coupling.**

### 1d. Sort-check + SAM/CRAM exclusion — CORRECT
`ThreadedBamReader::from_path` (`read.rs:318`) calls `check_not_coordinate_sorted` (`:326`) — the **same** function `BamReader::new`/`open_reader` use (`:237`). Coordinate-sort rejection is preserved on the BAM path. The plan correctly mandates `from_path` (not `from_path_without_sort_check`). SAM/CRAM are correctly routed to `open_reader` (single-threaded) — they are not BGZF, so there is no threaded path to use. `AlignmentKind::from_path` (magic-byte sniff, `read.rs:111`) classifies BAM via the BGZF+`BAM\x01` payload check, so the `is_bam` gate is content-based, not extension-based — robust.

### 1e. `records() -> Box<dyn Iterator>` boxing cost — NEGLIGIBLE
Scrutiny item 6. The box is allocated **once** per `run_pipeline` invocation (one `reader.records()` call at `:406`), not per record. Per-record dispatch is one vtable indirection (~5–10 ns), already the status quo: `AnyReader::records()` (`read.rs:527`) already returns `Box<dyn Iterator>`, so the SAM/CRAM path is unchanged, and the new `ThreadedBamReader` arm adds the identical single-box pattern. No per-record allocation. Not a concern.

---

## 2. Assumptions

| # | Assumption | Status |
|---|---|---|
| A1 | `MultithreadedReader::with_worker_count(2)` preserves record order | **VERIFIED SAFE** from source (§1a). The byte-identity suite at N=1 now exercises it (proven §1b). |
| A2 | `ThreadedBamReader::from_path` applies the same sort-check | **VERIFIED** — same `check_not_coordinate_sorted` (§1d). |
| A3 | **1 worker + 2 decode threads reaches ~17.9s** (default benefits) | **UNVERIFIED — the real open question.** See Important I1. The plan correctly flags this as the key perf-gate (Validation #5) and does NOT claim it as proven. |
| A4 | `AlignmentKind::from_path` classifies BAM correctly | **VERIFIED** — magic-byte sniff + unit tests `from_path_detects_bam_via_bgzf_payload_on_fixture` etc. (`read.rs:765`). |

The implicit assumption worth surfacing: **the win at 2 decode threads is independent of worker count.** #887 measured 2 decode threads coupled to 2 workers (`n_workers >= 2` ⇒ `with_worker_count(n_workers)`). The oxy table in the plan (`--mbias_only` 13.0s at "2 decode threads") was measured with `--mbias_only`, which has **no extract worker pipeline at all in the bottleneck sense** (no RoutedCall emission, no collector writes — it just decodes + accumulates). So the 13.0s `--mbias_only` number is genuinely a decode-floor measurement and transfers cleanly. The **plain `.txt` 17.9s** number is the one that depends on whether 1 extract worker keeps up — that is A3/I1.

---

## 3. Efficiency

- **Always-on +1 decode thread:** bounded, ~+1 core (plan's 1.8→3.1 / 2.7→3.2). Acceptable. The `MultithreadedReader` holds `worker_count`-bounded in-flight buffers (`:185–191`: three `bounded(worker_count)` channels + `worker_count` recycled buffers), so memory is small and fixed at 2 — plan's claim is accurate.
- **Capped at 2, not scaled with `--parallel`:** correct per the measured 3/4-no-gain data, and it avoids the over-subscription that would otherwise occur at high `--parallel` (e.g. `--parallel 8` would spawn 8 extract workers + 8 decode threads under #887's coupling; the plan's fixed-2 avoids that). This is a genuine improvement over #887, not just a default-benefit fix.
- **`#887`'s "0.69× at worker_count=1" caveat:** This is a *perf* (not correctness) note and applies to `worker_count=1`, where `MultithreadedReader` still spawns a reader thread + 1 inflater thread (2 extra threads) for no parallelism gain — strictly worse than the single-threaded `bgzf::Reader`. The plan uses `worker_count=2`, which is the *measured-faster* configuration. So the 0.69× caveat does **not** apply. See Optional O2 for the one place this still matters (tiny CI fixtures).

---

## 4. Validation sufficiency

**Strong overall.** The crucial property — that the byte-identity suite now genuinely exercises the threaded reader — is **confirmed**: the fixtures (`write_se_large_bam`, `write_pe_directional_bam`, etc. in `parallel_phase_f.rs:127+`) are written via `BamWriter::from_path` (real BGZF/BAM), and the legacy reference path (`extract_se`/`extract_pe` in `pipeline.rs:74/221`) uses single-threaded `open_reader`→`BamReader`. Under the always-BAM design the parallel path uses `ThreadedBamReader`. So `assert_dirs_byte_identical(legacy, parallel-n1)` is a true single-threaded-decode-vs-2-thread-decode comparison. An ordering regression *would* be caught. (Empirically confirmed: I ran it post-adaptation, all green.)

**Gaps / inaccuracies:**
- **V-gap-1 (Important→Optional):** Validation #2 claims "Coordinate-sorted-BAM **rejection** test still errors." There is **no such test at the extractor/integration level** — grep of `rust/bismark-extractor/tests/` for `UnsortedInput`/`SO:coordinate`/`b"coordinate"` returns nothing. The rejection is unit-tested only in `bismark-io` (`read.rs:899` `check_not_coordinate_sorted_rejects_coordinate`). Because `ThreadedBamReader::from_path` and `open_reader`/`BamReader` share the identical `check_not_coordinate_sorted` call, the behavior is preserved regardless — this is a **plan-text accuracy** issue, not a correctness risk. Either (a) soften Validation #2 to "the shared `bismark-io` sort-check unit test still applies (no extractor-level test exists; behavior is shared-code-guaranteed)", or (b) add a small extractor-level coord-sorted-BAM test that asserts `extract_se_parallel` returns `UnsortedInput`. (a) is sufficient.
- **V-gap-2 (Optional):** Validation #5 (perf re-measure of `--parallel 1` plain `.txt` ≈ 17.9s) is the only check for A3/I1 and it is on **oxy**, not in CI. If oxy is unavailable at implementation time, A3 stays unverified and the "default benefits" claim is unproven. Recommend: make this a hard gate before claiming the headline win, and record the *baseline* re-measure too (the plan does say "Record vs the ~20s baseline" — good).
- **V-good:** The empty-BAM fixture (`write_empty_bam`, `:234`) plus the `producer_panic_does_not_deadlock_workers` test cover the small-input/edge paths; the threaded reader on a header-only BAM is exercised. The 8199-record multibatch crosses `BATCH_SIZE` with the threaded reader. Good coverage.

---

## 5. Alternatives considered

- **Floor workers at 2 (plan §self-review "remaining risk").** This is the most important design alternative and directly addresses A3/I1. If the perf re-measure shows `--parallel 1` does NOT realize ~17.9s (extract-bound at 1 worker), the cheapest fix is to floor the extract worker count at 2 *when input is BAM* (decode is parallel, so feeding 2 workers costs ~1 more core, total ~4 cores — still within the plan's measured 3.2-core envelope plus one). The plan defers this to "reconsider" — I agree it should NOT be pre-emptively adopted (it changes the default thread count and could regress tiny inputs), but the plan should state the **fallback decision rule** explicitly: "if Validation #5 shows <~5% improvement at `--parallel 1`, floor BAM workers at 2 and re-measure." Right now the fallback is vague ("reconsider e.g. floor workers at 2"). See Important I1.
- **Hidden env/flag for `DECODE_THREADS`** (plan Q1): correctly rejected in favor of a fixed const matching `GZIP_COMPRESS_THREADS`. Agree.
- **Reuse `AnyReader` by adding a `Threaded` variant** instead of a separate `ProducerReader` enum: rejected implicitly (the d3dd289 design adds a local enum). Correct — `AnyReader` is generic over `<R: BufRead, RC: Read+Seek>` and `ThreadedBamReader` is a concrete non-generic type wrapping `File`; bolting it onto `AnyReader` would pollute a shared `bismark-io` public type for an extractor-internal optimization. The local enum is the right scope.

---

## 6. Action items (prioritized)

### Critical
*(none)*

### Important
- **I1 — Make the A3 "default benefits" perf-gate a hard, decision-ruled gate.** The entire stated *Goal* ("the default `--parallel 1` must benefit") hinges on 1 extract worker keeping up with 2 decode threads on the plain `.txt` path — and this is **unverified**. The plan flags it (Validation #5, Self-Review) but the fallback is vague. Add an explicit decision rule to the plan: *if the oxy re-measure shows `--parallel 1` plain `.txt` does not drop to ≈17.9s (say <5% off the ~20s baseline), then floor BAM extract workers at 2 and re-measure before merging.* Without this rule the plan can "succeed" on tests yet miss its headline goal. (Plan §Validation #5, §Self-Review remaining-risk; `parallel.rs:206` `n_workers = config.parallel.max(1)`.)

### Optional
- **O1 — Fix Validation #2 wording (no extractor-level coord-sort test exists).** Either soften the claim to reference the shared `bismark-io` unit test (`read.rs:899`) or add an `extract_se_parallel` coord-sorted-BAM `UnsortedInput` test. Behavior is shared-code-safe either way. (Plan §Validation #2.)
- **O2 — Note the tiny-fixture thread-spawn overhead is accepted, not free.** For 6-record CI fixtures, `with_worker_count(2)` spawns 1 reader + 2 inflater threads (`multithreaded_reader.rs:189–194`) vs the single-threaded reader's zero. This is correctness-neutral (proven: all 18 tests pass, suite runs in 0.37s) and perf-irrelevant on real workloads, but the plan's "MultithreadedReader handles empty/single-record BAM" (Self-Review edge cases) should add one clause: "thread-spawn overhead on tiny inputs is accepted (correctness-neutral; CI suite unaffected — measured 0.37s)." (Plan §Self-Review edge cases.)
- **O3 — `const DECODE_THREADS` init form: `NonZeroUsize::new(2).unwrap()` compiles as const on the toolchain in use (rustc 1.95.0; `Option::unwrap` const-stable since 1.83).** I verified this directly. The plan's parenthetical "(or a match/expect const-init)" is unnecessary hedging — the simple `.unwrap()` form works. Either is fine; just don't waste an implementation cycle on it. (Plan §Implementation outline #2.)
- **O4 — Update the module doc at `parallel.rs:11`** ("drives `open_reader().records()`") to mention the threaded-BAM decode path — the plan already lists this as outline step #7; just confirming it's a real stale-comment site (`parallel.rs:11`).

---

## 7. Summary

The mechanism is sound and *proven* (noodles source + empirical test). The adaptation is mechanically clean and I ran it green. The only genuine open question is a **performance** one — whether 1 extract worker bottlenecks the plain-`.txt` default — and the plan correctly identifies it but should harden it into an explicit decision rule (I1). No correctness Critical findings. Approve once I1's fallback rule is written in and O1's validation wording is corrected.

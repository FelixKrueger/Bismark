# Plan Review — PERF_R3_DECODE_PLAN.md (Reviewer B)

**Plan:** `plans/05262026_bismark-extractor/PERF_R3_DECODE_PLAN.md`
**Code @ `b2af4e5`** (worktree `/Users/fkrueger/Github/Bismark-extractor`)
**Reviewer:** B (independent; conclusions drawn from source, not the plan's self-assessment)
**Verdict:** APPROVE WITH CHANGES. The core mechanism is sound and the load-bearing
byte-identity claim is verifiable in the noodles source — but two of the plan's five
validation items reference tests that **do not exist in this crate**, and the
"`--parallel 1` benefits" success criterion rests on an unverified single-extract-worker
throughput assumption that the plan itself flags but does not de-risk. Address the
Critical/Important items below before implementation.

---

## 1. Logic review

### 1.1 Record order under `MultithreadedReader` — VERIFIED SOUND (the linchpin)

I traced the ordering contract directly in
`~/.cargo/.../noodles-bgzf-0.47.0/src/io/multithreaded_reader.rs` (bismark-io pins
`noodles-bgzf = "=0.47.0"`, `rust/bismark-io/Cargo.toml:24`):

- A **single** `spawn_reader` thread reads raw BGZF frames *sequentially* and, per frame,
  creates a per-block oneshot `(buffered_tx, buffered_rx)`, sends the work to the shared
  inflater pool via `inflate_tx`, and pushes the **`buffered_rx` onto `read_tx` in file
  order** (`multithreaded_reader.rs` `spawn_reader`, lines ~363–388).
- The consumer (`recv_buffer` → `read_block`) pops `read_rx` strictly **FIFO** and blocks
  on each block's oneshot `buffered_rx.recv()` (lines ~352–360, ~234–252).

So decompression is parallel, but **emission to the BAM record reader is strict file
order regardless of `worker_count`**. The decompressed byte stream handed to
`noodles_bam::io::Reader` is byte-identical to the single-threaded `bgzf::io::Reader`.
This is the noodles contract the plan's load-bearing assumption (§Assumptions bullet 1)
depends on, and it holds. The plan is **not** unsound on this axis.

Corroborating test already in-repo: `rust/bismark-io/tests/integration_fixture_bam.rs:261`
`threaded_bam_reader_preserves_record_order` asserts qname-sequence equality between
`BamReader` (single-threaded) and `ThreadedBamReader` at `worker_count=4` on the **real
Perl-produced `tiny_pe_bismark.bam` (203 records, genuine multi-block)**. Plus
`:295` strand-classification equality and `:231` record-count equality. These already
exercise multi-worker decode against a real multi-block BAM and pass.

**Judgment on the plan's "N=1 suite now runs the threaded path" sufficiency (its stated
mitigation):** Necessary but *weak on its own*. See §4.1 — the extractor's own large
fixture likely collapses into 1–2 BGZF blocks, so the extractor suite does **not** add a
genuine multi-block ordered-content assertion; the only real multi-block ordering guard
lives in bismark-io. That guard exists and is strong, but a plan that productionizes
always-on threaded decode for the extractor should not *silently* rely on a sibling
crate's fixture for its single most-important invariant. Recommend an explicit
acknowledgement (cheap) — see Important I2.

### 1.2 `producer_loop` touches `reader` only via `.records()` / `.header()` — VERIFIED

`grep "reader\." parallel.rs` returns exactly three sites:
- `:226` `build_chr_name_table(reader.header())`
- `:235` `logger.header_provenance(reader.header())`
- `:442` `let mut records_iter = reader.records();` (inside `producer_loop`)

Both are covered by the `ProducerReader::{header, records}` enum methods copied from
d3dd289. **No `AnyReader`-typed access leaks past the enum boundary**, no batching
internal touches the reader. The enum + signature swap integrates cleanly with the
post-R1-batching body. Confirmed for item §Independently-scrutinize #4.

### 1.3 Sort-check / SAM / CRAM / CRAM-ref — VERIFIED CORRECT

- `ThreadedBamReader::from_path` (`read.rs:318`) calls `check_not_coordinate_sorted`
  (`:326`) — identical rejection to `BamReader::from_path` → `BamReader::new`
  (`:237`). Same `BismarkIoError::UnsortedInput`. The plan correctly mandates `from_path`
  (NOT `from_path_without_sort_check`). ✓
- SAM/CRAM are routed to `ProducerReader::Any(open_reader(input, None))`, single-threaded,
  unchanged. ✓ There is no threaded SAM/CRAM path (correct — SAM is text, CRAM is its own
  container). ✓
- CRAM-ref: the plan keeps `open_reader(input, None)` for the SAM/CRAM branch, so the
  `cram_ref=None` → `MissingCramReference` path is **unchanged** (`read.rs:569–572`).
  Note this is a pre-existing limitation, not introduced here: the extractor never wires a
  `cram_ref`, so CRAM input has always errored with `MissingCramReference`. The plan does
  not regress it. ✓ (If anything, worth a one-line note that CRAM remains unsupported
  end-to-end — orthogonal to this PR.)

### 1.4 Edge cases

- **Empty BAM:** `write_empty_bam` (header + EOF block only). `MultithreadedReader`'s
  reader thread hits `read_frame_into → Ok(None)` on the EOF block → loop breaks →
  `recv_buffer` returns `None` → zero records. Then `producer_loop` emits zero batches
  (the `if !items.is_empty()` guard at `:556`). Matches N=1 header-only finalize. Covered
  by `parallel_empty_bam_at_n4_produces_header_only_files` (`:1065`), which will now run
  through the threaded reader. ✓
- **Single-record BAM:** one data block + EOF block → one buffered_rx in order → fine.
- **Invalid-XM / unpaired-final / cross-chr:** these are post-decode kernel errors,
  independent of the reader; unaffected. Covered at N=4 (`:976`, `:1003`, `:1029`).

---

## 2. Assumptions

| # | Assumption (plan) | Status | Note |
|---|---|---|---|
| A1 | `MultithreadedReader(2)` emits records in single-threaded order | **VERIFIED** (noodles source, §1.1) | Strongest claim; holds at *any* worker count. |
| A2 | `from_path` applies identical sort-check / errors | **VERIFIED** (`read.rs:326`) | Same `UnsortedInput`. |
| A3 | 1 worker + 2 decode threads reaches ~17.9s (default benefits) | **UNVERIFIED — load-bearing for the *goal*** | Plan flags it (Validation #5 + Self-Review remaining-risk) but ships the design that bets on it. See §3.1 + I1. |
| A4 | `AlignmentKind::from_path` classifies BAM correctly | **VERIFIED** (`read.rs:111`, magic-byte sniff incl. BGZF-payload `BAM\x01` check) | Robust; also handles mis-named files. |

Hidden assumption the plan does **not** surface:

- **H-1 (Critical-adjacent): the always-on threaded reader is never *slower* than the
  current single-threaded reader on small inputs.** The d3dd289 commit message itself
  records "0.69× at worker_count=1" and `read.rs:296–298` warns "For N==1 prefer
  `BamReader::from_path` — the threaded constructor always spawns at least one worker
  thread." The plan ships `worker_count=2` (not 1), and on every CI fixture + every
  `bismark-io` integration test that now flows through the extractor. The plan asserts
  "MultithreadedReader handles" small inputs but offers **no measurement that
  worker_count=2 ≥ single-threaded** on tiny BAMs. With 2 inflater threads + 1 reader
  thread spawned/torn-down per `run_pipeline`, the fixed thread-pool spawn cost is paid on
  *every* invocation including `--mbias_only` on a 5-record BAM. This is almost certainly
  fine for wall-clock correctness (tests assert *output*, not speed) but is an unstated
  efficiency regression on the smallest inputs and on the test-suite runtime. See O1.

---

## 3. Efficiency

### 3.1 Does `--parallel 1` actually get the win? (the recommended-stance item, #2)

The measured win (17.9s plain `.txt`) was **2 extract workers + 2 decode threads**. The
plan ships **`worker_count` = `--parallel` (default 1) + fixed 2 decode threads**, so the
default = **1 extract worker + 2 decode threads**, a config that was *not* in the measured
table. The plan's own Self-Review (lines 116–118) explicitly flags this as "the remaining
risk."

This is a real gap. The 17.9s figure decomposed as: decode floor ~13s (`--mbias_only`,
2 decode threads), plain `.txt` ~17.9s. Under `--mbias_only` there is no write contention
and the single extract worker does almost nothing → decode-bound → 1 worker is plausibly
fine. But for plain `.txt` the single extract worker must also run `extract_calls` +
routing + feed the collector. If extract+route per record exceeds decode-per-record, a
single worker bottlenecks and the default lands above 17.9s — possibly close to today's
~20s, i.e. **the headline "default benefits" goal partially fails** even though byte-
identity is preserved.

**My recommended stance (differs from the plan):** *Floor the extract workers at 2 when
input is BAM*, i.e. `n_workers = config.parallel.max(2)` **only on the threaded-BAM path**
(or unconditionally — the deadlock analysis in the module docs is about rayon, not
`std::thread`; `std::thread` workers have no N=1 deadlock, and N=2 is strictly safer for
the bounded-channel topology). Rationale:
- The measured sweet spot was explicitly **2+2**, never **1+2**.
- The plan already decouples decode threads from `--parallel` "so the default benefits" —
  applying the same logic to the *extract* side (floor at 2) is the consistent move and is
  what was actually measured.
- Cost: one extra `std::thread` + the bounded channels at `n*4` (tiny).

If the implementer instead keeps 1 worker, then **Validation #5 must be a hard gate, not
a "re-measure"**: if `--parallel 1` plain `.txt` does not reach ≤ ~18s, the design must
change before merge. As written, the plan treats #5 as a measurement to "record vs
baseline" — it should be a *pass/fail* gate with the floor-at-2 fallback pre-authorized so
the implementer doesn't stall.

### 3.2 Decode-thread cap at 2 — SOUND

3/4 threads measured no gain / slight regress; fixed const matches the
`GZIP_COMPRESS_THREADS` precedent. Memory is bounded (`MultithreadedReader` channels are
`bounded(worker_count)` = 2 in-flight buffers, `multithreaded_reader.rs` resume()). ✓

### 3.3 Triple-sniff on the auto-detect path — MINOR

In `--paired auto` mode (`main.rs:104`), the BAM is already opened twice today: `probe =
open_reader` (`:108`, which internally calls `AlignmentKind::from_path`) then re-opened
inside `run_pipeline` (`:210` `open_reader`, again sniffing). The plan **adds a third
sniff**: an explicit `AlignmentKind::from_path(input)` in `run_pipeline` to choose the
reader, *plus* — on the SAM/CRAM fallback — `open_reader` sniffs a 4th time. Each sniff is
one `open(2)` + first-block inflate (~100–700µs per `read.rs:91–95` doc). Negligible vs a
multi-second run, identical in shape to d3dd289, but worth a one-line acknowledgement that
BAM is now sniffed 2–3× per run. Not a blocker. (O2)

---

## 4. Validation sufficiency

### 4.1 The 8199-record "multibatch" test may NOT exercise multi-block decode ordering — IMPORTANT

`write_se_large_bam` (`parallel_phase_f.rs:127`) writes 8199 tiny synthetic records via
`BamWriter::from_path`. `BamWriter`/noodles flushes a BGZF block at ~64 KiB of *compressed*
payload; 8199 records of ~5bp seq + minimal tags will very likely fit in **1–2 BGZF
blocks**. So while this test will now *run through* `ThreadedBamReader`, it does **not
reliably exercise the parallel-inflate-of-many-blocks ordering path** — with ≤2 blocks and
2 workers, there's little reorder pressure. The plan's claim that the suite "now exercises
the threaded reader at all N" is *technically true but thin* for the multi-block ordering
case.

The genuinely strong multi-block ordering guard is `bismark-io`'s
`threaded_bam_reader_preserves_record_order` on the real 203-record Perl BAM (which has
real multiple blocks) at worker_count=4. That exists and passes. But it is a *different
crate's* test and is not part of the plan's Validation list.

**Recommendation (I2):** Either (a) add the bismark-io order-preservation test to the
plan's Validation list explicitly as the multi-block ordering guard, or (b) add one
extractor-level assertion that runs the **real Phase-H fixture BAM** (or a deliberately
many-block BAM) through `extract_se_parallel` at N=1 and N=2 and byte-compares to legacy
`extract_se`. Option (a) is nearly free and sufficient given §1.1.

### 4.2 Validation #2 references a NON-EXISTENT test — IMPORTANT (factual error in the plan)

Validation #2: "Coordinate-sorted-BAM **rejection** test still errors (sort-check
preserved)." I searched the extractor crate:
`grep -rn "UnsortedInput|coordinate|SO:coordinate" rust/bismark-extractor/tests/` — **no
such test exists.** Every extractor fixture uses `SO:unsorted` (e.g.
`pe_phase_c.rs:1367/1388`, `mbias_writer_phase_d_smoke.rs:36`). Coordinate-sort rejection
is tested only at the `bismark-io` unit level
(`read.rs:899 check_not_coordinate_sorted_rejects_coordinate`) and in `bismark-dedup`.

Since this PR **changes which reader BAM flows through** (`open_reader` → `ThreadedBamReader::from_path`),
there is currently **no extractor-level regression guard** that a coord-sorted BAM is
still rejected after the swap. The plan lists this as if it were an existing passing test
to re-run. **Fix the plan**: either downgrade #2 to "covered by bismark-io unit test
`check_not_coordinate_sorted_rejects_coordinate` (the threaded reader shares the same
`check_not_coordinate_sorted`)" — which is honest — or *add* an extractor integration
test that runs a `SO:coordinate` BAM through `extract_se_parallel` and asserts the error.
Given that `ThreadedBamReader::from_path` and `BamReader::from_path` call the identical
function, the bismark-io unit test is *technically* sufficient coverage; the plan just
mis-describes where the coverage lives. (I prefer adding the 1 extractor test — it's the
only place the new dispatch decision is made.)

### 4.3 SAM input still works — NOT in the Validation list — IMPORTANT

The plan changes `run_pipeline` to branch BAM-vs-(SAM/CRAM). The plan asserts SAM is
"unchanged," but I found **no SAM-input integration test in the extractor `tests/`** that
would catch a mistake in the `AlignmentKind::from_path` dispatch (e.g. accidentally
routing SAM into the threaded BAM reader, which would then fail BGZF-detection). The
extractor's SAM coverage is at the bismark-io reader level, not at the extractor pipeline
level. The plan's Self-Review says "SAM/CRAM (AnyReader path)" as if covered. Recommend
adding a one-record SAM-input smoke through `extract_se_parallel` to confirm the non-BAM
branch survives. (I3 — lower than I2 because the branch logic is simple and the bismark-io
SAM tests are solid, but the *dispatch* is new code here.)

### 4.4 What IS sufficient

- N=1 and N=4 legacy-vs-parallel byte-identity on **real on-disk BGZF BAMs** (fixtures use
  `BamWriter::from_path` → real BGZF). Once always-on, these run the threaded reader. ✓
- Empty / invalid-XM / unpaired-final / mbias-only at N=4. ✓
- Colossal `phase_h_smoke` SE+PE plain **and** `--gzip` (Validation #4) — the binding
  real-data byte-identity gate against Perl. Strong. ✓
- clippy `-D warnings` + fmt. ✓ (Note: the implementer must add `use std::num::NonZeroUsize`
  — not currently imported outside tests; `grep NonZero parallel.rs` returns only test
  usages. Trivial but the plan's import list (§Implementation-outline #1) lists only the
  `bismark_io` additions and not the `NonZeroUsize` import. Confirm both.)

### 4.5 Perf gate sufficiency

Validation #5 (`--parallel 1` plain ≈ 17.9s) is the **only** check on the headline goal,
and as written it's a "record vs baseline," not a pass/fail. Given §3.1, this is the
single most important gate and should block merge if unmet, with the floor-at-2 fallback
pre-authorized. (Folds into I1.)

---

## 5. Alternatives

1. **Floor extract workers at 2 on BAM (RECOMMENDED, see §3.1).** Reproduces the measured
   2+2 sweet spot for the default. Smallest deviation from the plan's intent; one extra
   `std::thread`. This is my top alternative and I'd make it the default design rather than
   a fallback.

2. **Min-size guard for the threaded reader.** Skip `ThreadedBamReader` (use
   `open_reader`) when the BAM is below some byte threshold, to avoid the worker-pool spawn
   on tiny CI fixtures (H-1). Trade-off: adds a `std::fs::metadata` stat + a magic number,
   and risks a *different* code path on small vs large inputs (so the byte-identity suite
   would test the single-threaded path for small fixtures and *not* the threaded path —
   defeating the plan's "suite now runs threaded at N=1" mitigation). **Reject** — the
   plan's always-on choice is better for test coverage; just accept the µs-scale small-
   input overhead (O1).

3. **Hidden env/flag for `DECODE_THREADS`.** The plan already considered + rejected this in
   favor of a fixed const (matches `GZIP_COMPRESS_THREADS`). Agree — fixed const is right;
   3/4 measured no-gain.

4. **Keep d3dd289's `n_workers>=2` gate but add a `--parallel 2` default.** Changes the CLI
   default rather than the decode coupling. Rejected: changes user-visible default
   parallelism + CPU footprint for all users; the plan's decouple-decode approach is
   cleaner and bounds the extra cost to ~1 core.

---

## 6. Action items (prioritized)

### Critical
- **C1 — Resolve the `--parallel 1` throughput bet before implementation (§3.1, A3).**
  The headline goal ("default benefits, ~17.9s") rests on an *unmeasured* 1-worker+2-decode
  config; the measured sweet spot was 2+2. Either (a) change the design to floor extract
  workers at 2 on the threaded-BAM path (`config.parallel.max(2)`), which reproduces what
  was measured, **or** (b) make Validation #5 a hard pass/fail gate (≤ ~18s) with the
  floor-at-2 fallback pre-authorized so a failed measurement doesn't strand the
  implementer. Do not ship the 1+2 design on the unverified assumption alone.

### Important
- **I1 — Promote Validation #5 to a blocking perf gate** (folds into C1): record the actual
  `--parallel 1` plain `.txt` number and *fail the PR* if it doesn't beat the ~20s baseline
  meaningfully.
- **I2 — Fix the multi-block ordering coverage story (§4.1).** The extractor 8199-record
  "multibatch" fixture likely produces ≤2 BGZF blocks, so it does NOT robustly exercise
  parallel-inflate ordering. Add bismark-io's `threaded_bam_reader_preserves_record_order`
  (real 203-record multi-block BAM, worker_count=4) to the plan's Validation list as the
  named multi-block ordering guard, or add an extractor-level N=1-vs-legacy byte-compare on
  a genuinely many-block BAM.
- **I3 — Correct Validation #2 (§4.2).** No coordinate-sort rejection test exists in the
  extractor crate. Either re-cite it to the bismark-io unit test
  `check_not_coordinate_sorted_rejects_coordinate` (honest, and sufficient since both
  readers share `check_not_coordinate_sorted`), or add an extractor integration test that
  runs a `SO:coordinate` BAM through `extract_se_parallel` and asserts `UnsortedInput`.
- **I4 — Add a SAM-input smoke through `extract_se_parallel` (§4.3)** to guard the new
  BAM-vs-SAM/CRAM dispatch branch (the dispatch decision is *new code* this PR introduces;
  nothing at the extractor pipeline level currently exercises non-BAM input through the
  parallel path).

### Optional
- **O1 — Document the small-input overhead (H-1).** Note in the plan that worker_count=2 is
  always-on incl. tiny CI fixtures + `--mbias_only`, paying a fixed 3-thread spawn/teardown
  per run (d3dd289 noted 0.69× at worker_count=1). Confirm — even informally — that
  worker_count=2 is not *slower* than single-threaded on the smallest fixtures, or simply
  accept it (output correctness is unaffected; only µs-scale wall-time).
- **O2 — Note the BAM is now sniffed 2–3× per run (§3.3)** (probe + run_pipeline classify +
  open_reader fallback). Negligible, but the plan should say so.
- **O3 — Stale line numbers.** Plan cites `:210/:211/:220/:365`; current `b2af4e5` has
  header at `:226/:235`, `producer_loop` at `:364`, `reader.records()` at `:442` (R1
  batching shifted them). d3dd289's diff was against a pre-R1-batching tree (`:207/:361`).
  The mechanical adaptation still holds (the touched surface is localized), but the
  implementer must re-locate rather than blind-apply.
- **O4 — Confirm the `NonZeroUsize` import (§4.4).** `parallel.rs` does not import
  `std::num::NonZeroUsize` outside `#[cfg(test)]`; the plan's import list (§Impl #1) lists
  only `bismark_io` additions. Trivial, but make it explicit.
- **O5 — One-line note that CRAM remains end-to-end unsupported** (extractor never passes
  `cram_ref` → `MissingCramReference`). Pre-existing, not a regression; just clarity.

---

## Confidence notes
- The byte-identity linchpin (record order under parallel decode) is **independently
  verified in noodles source AND in an existing passing test** — high confidence the plan
  is *correct* (won't produce wrong output).
- The plan's *goal* (default `--parallel 1` realizes the win) is the genuine risk, and the
  plan ships the one config that wasn't measured. That's why this is APPROVE-WITH-CHANGES,
  not a clean APPROVE.
- Two validation items (#2 rejection test, implied SAM coverage) describe coverage that
  isn't where the plan implies — factual, fixable, not fatal.

# PERF R2 Plan Review — Reviewer B (#884 worker-side gzip)

Plan: `PERF_R2_WORKER_OUTPUT_PLAN.md` (rescoped to the `--gzip` path only).
Reviewed against: `rust/bismark-extractor/src/{output.rs,parallel.rs,state.rs,output_mode.rs}`,
`bismark-extractor/Cargo.toml`, `Cargo.lock`, and the two `.gz` test sites
(`tests/parallel_phase_f.rs`, `tests/output_modes_phase_e_smoke.rs`).

The Phase-0 spike is sound and the rescope (gzip-only, leave `.txt` alone) is
the right call: the data (`.gz` flat at ~75 s, ~52 s of which is single-threaded
`GzEncoder`) is decisive and `.txt`'s ~6 s isn't worth the byte-identity churn.
My concerns below are about the *mechanics* of the gzip-only rewrite, where I
think the plan under-specifies real risks that block byte-identity or erode the
win.

---

## 1. Logic review

### C1 — The existing `.gz` tests use single-member `GzDecoder` and will SILENTLY pass on truncated output (CRITICAL)
This is the biggest gap. The plan's whole correctness story for `.gz` is
"concatenated per-batch members = valid multi-member gzip; smoke `zcat|sort|md5`
accepts it." But **the repo's actual `.gz` validators do not use `zcat` and do
not handle multi-member streams**:

- `tests/parallel_phase_f.rs:346` `decompress_gz` → `flate2::read::GzDecoder`
- `tests/output_modes_phase_e_smoke.rs:180` `read_gz` → `flate2::read::GzDecoder`

`flate2::read::GzDecoder` decodes **only the first gzip member** and stops at its
trailer, silently ignoring every subsequent member. With R2's multi-member
output, a file containing batches [0,1,2,…] would decode to **only batch 0's
content**, and the assertion `decoded == plain_peer` would **fail** — but for the
wrong reason (truncation), and worse, a *single-batch* test input (< 4096
records ⇒ exactly one member) would **pass while proving nothing** about the
multi-member path. The plan says "confirm no test asserts raw `.gz` identity"
but the real risk is the inverse: the tests assert *decompressed* identity using
a decoder that can't see past member 1.

**Required before implementation:** swap both helpers to
`flate2::read::MultiGzDecoder` (also `read` module, drop-in), AND add a test
whose input spans **≥ 2 batches under `--gzip`** so the multi-member path is
actually exercised (the 8199-record boundary input already used elsewhere is a
natural fit: 8199 > 2×4096). Without this, R2 could ship truncated `.gz` files
and the suite would stay green. The plan's verification step 2 ("add a `--gzip`
N=1≡N=4 sorted-equivalence test if not present") must be upgraded to "swap to
MultiGzDecoder + multi-batch input" — sorted-equivalence is not even needed here
because content order IS preserved (see C2).

### C2 — "sorted-equivalent, not raw-identical" framing is wrong for this design (IMPORTANT, also de-risks C1)
The plan repeatedly hedges that R2 changes `.gz` from "raw-identical" to
"sorted-content-equivalent" (lines 99-102, 121-122). But the design concatenates
per-key chunks **strictly in `batch_seq` order**, and within a batch in `Vec`
order — exactly the `(batch_seq, within_idx)` total order the collector already
uses (parallel.rs:996-1033). So the **decompressed byte stream is identical to
N=1's**, not merely sorted-equivalent. The only thing that differs from a
single-stream `.gz` is the *container framing* (N members vs 1), which is
transparent to any spec-compliant gzip reader. This matters because:
- The test should assert **exact decompressed-byte identity** (via
  MultiGzDecoder), which is stronger and catches ordering bugs that a
  `sort`-based check would mask.
- The "sorted-equivalence" language invites a weaker test than the design
  actually supports, and would hide a real ordering regression.

Recommend the plan drop the sorted-equivalence framing for the R2 path and
commit to **decompressed byte-identity** as the gate.

### C3 — Single-source-of-truth for line formatting: the plan acknowledges drift risk but the current code makes the extraction non-trivial (IMPORTANT)
`OutputFileMap::write_call` (output.rs:170-231) does two things the plan wants to
split: (a) `route_to_key(mode, context, strand)` and (b) format the 5-col line
**or** dispatch to `write_yacht_row`. The plan's `format_call_line(...)` must
reproduce *both* the 5-col branch and the full yacht 8-col branch
(`write_yacht_row`, output_mode.rs:191) byte-for-byte, including the strand-
conditional col6/col7 already carried on `RoutedCall`. The plan's proposed
signature `format_call_line(mode, qname, chr, call, strand, yacht6, yacht7)` is
adequate, but note `write_yacht_row` is generic over `W: Write` and writes
incrementally; refactoring it to append into a `Vec<u8>` is fine (Vec: Write),
but the plan should explicitly state that `write_call` becomes
`format_call_line` + `append_bytes` so the **single-threaded path is exercised
by the same formatter** (the plan says this — good; just flag that yacht is in
scope of the move, not only the 5-col branch).

### C4 — Header line + empty-sweep interaction (IMPORTANT)
Today the version header (`SPLIT_FILE_HEADER`, output.rs:38/127) is written at
`OutputFileMap::new` time, and `records_written` starts at 0; the empty-sweep
(output.rs:283) unlinks a file iff `records_written == 0` even though it has the
header. Under R2:
- The header is still written by the collector at open time (good — keep it
  collector-side, NOT in worker chunks, or every batch member would re-emit it).
- **Under `--gzip`, the header is currently part of the same `GzEncoder`
  stream.** If R2 makes the collector write the header as its own gzip member
  (or raw bytes before the first worker member), the resulting file is
  header-member + data-members. That decodes fine with MultiGzDecoder, but the
  plan must specify *who* compresses the header and that it counts as member 0.
  Easiest: collector writes a header gzip member at open; `append_bytes` then
  appends worker members. The plan's "append_bytes(key, &[u8])" must take
  **already-compressed** bytes for `.gz` (a full member) — make that explicit.
- `records_written == 0` detection: the plan says "a file is empty if no batch
  wrote bytes to it." But a CTOT/CTOB file in a directional run will have the
  header member and zero data members. The sweep must key off *data* records,
  not "bytes written" (the header is bytes). The plan's wording "no batch wrote
  bytes" is therefore wrong — it must track a **per-key record/data counter**
  threaded back from workers (e.g. each `WorkerOutput::Batch` carries per-key
  record counts, or the collector infers "had ≥1 data member"). This is a
  concrete invariant the current `records_written` bump (output.rs:229) enforces
  per-call; R2 must reconstruct it at batch granularity or the empty-sweep will
  **keep files Perl deletes** (Phase H file-set-match gate, FinalizationReport
  feeds bismark2bedGraph argv — state.rs:144-191). This is under-specified and
  is a byte-identity / file-set risk.

---

## 2. Assumptions

- **A1 (validated): flate2 backend is `zlib-rs` + `miniz_oxide`** (Cargo.lock:
  flate2 1.1.9 deps `crc32fast`, `miniz_oxide`, `zlib-rs`). Per-worker
  `GzEncoder::new(Vec::new(), level)` is already available — no new dep needed.
  Good; the plan implicitly assumes flate2 stays the encoder and that holds.
- **A2 (flagged): "N members concatenated = valid multi-member gzip" is true per
  RFC 1952**, BUT only readers using `MultiGzDecoder`/`zcat -d`/`gunzip` honor it.
  `bismark2bedGraph` (Phase G subprocess) reads these `.gz` files downstream —
  the plan must confirm **the downstream consumer reads multi-member gzip**.
  Perl's own `--multicore` produces multi-member `.gz` (forked gzip streams), so
  the Perl toolchain already tolerates it (this is actually evidence FOR the
  approach), but the plan should state this explicitly rather than leave it
  implied. If any Rust-side reader (future bismark-bedgraph, epic #797) uses
  single-member `GzDecoder`, it breaks.
- **A3 (flagged): per-batch member count.** 188M calls / (4096 records/batch ×
  ~avg calls/record). At ~10–20 calls/record and 4096 records that's a chunk of
  raw bytes ~ 4096×15calls×~30 bytes/line ≈ **~1.8 MB raw per key per batch** for
  busy keys, but most of the 12 keys are near-empty per batch. Member count per
  file = number of batches that routed ≥1 call to that key ≈ up to
  188M/4096 ≈ **~46,000 batches** → up to ~46k members in the busiest file. See
  E1 for the compression-ratio consequence.

---

## 3. Efficiency analysis

### E1 — Compression-ratio regression from dictionary reset per member (IMPORTANT — quantify before shipping)
This is the risk Reviewer A is most likely to underweight. Each gzip member
starts a **fresh deflate dictionary** (32 KB sliding window resets to empty) and
carries its own ~18-byte header+trailer (10-byte header, 8-byte CRC+ISIZE). The
concern is not the ~18 bytes/member overhead (~46k members × 18 B ≈ 0.8 MB —
trivial against a multi-GB corpus). The real concern is **lost cross-member
back-references**: Bismark methylation lines are *highly* repetitive (same chr
string, same qnames clustered, `+`/`-`/XM bytes from a tiny alphabet), so a
single long stream lets deflate reference matches hundreds of KB back. Per-batch
members cap match distance at the batch boundary.

**Quantification needed (cheap, do it in the spike tail):** compress one busy
file two ways — (a) one stream, (b) split into 4096-record members and
concatenate — and compare sizes. If a busy key's batch chunk is ~1.8 MB raw,
the 32 KB window is already small relative to the chunk, so *intra-chunk*
matching is mostly preserved and the ratio loss is likely **single-digit %**.
But near-empty keys (CTOT/CTOB, or CHH-rare) get tiny members where the
header/trailer overhead and cold dictionary dominate — those files could grow
noticeably in *relative* terms (though they're small absolutely). Net: I expect
a few-% total size increase, acceptable, **but the plan must measure and record
it**, because "we made `.gz` 8% bigger" is a real user-visible regression that
should be a conscious tradeoff, not a surprise. Add to verification: "total
`.gz` byte size N=4 vs N=1 baseline, accept if < ~10% larger."

**Mitigation if ratio loss is unacceptable:** coalesce — workers emit *raw*
formatted bytes per key (not compressed), collector compresses each key's
concatenated stream with a **single** `GzEncoder`. But that puts compression
back on the collector (the thing we're trying to parallelize) → defeats R2. The
honest tradeoff is exactly the one in §5 (gzp): parallelize the *single stream*
instead of fragmenting it.

### E2 — Granularity: per-worker-per-batch-per-key encoder is the right call, with a caveat (matches plan)
The plan's "compress each key's batch buffer into a member" means up to
12 short-lived `GzEncoder<Vec<u8>>` per batch per worker. Creating/destroying a
`GzEncoder` per (batch,key) is cheap relative to the compression itself, and it
keeps the ordered-concat-by-batch model intact (which is what preserves
byte-identity). The **alternative** — per-worker-per-key *streaming* encoders
that span batches — breaks the ordered-concat model entirely: a worker only sees
a non-contiguous subset of batches for a key, so its single stream can't be
slotted into the global `batch_seq` order. So per-batch is **forced** by the
reorder design, not just a choice. The plan is right; just document that
streaming-across-batches is incompatible with the collector's `batch_seq`
reorder (parallel.rs:1006-1033), so it's not a live alternative.

The caveat: 12 encoders/batch where most keys are empty — **skip empty keys**
(don't emit a zero-record member; emit nothing for that key this batch). The
plan's `HashMap<OutputKey, Vec<u8>>` (sparse) naturally does this; a "fixed array
indexed by key" (the plan's other suggestion) risks emitting empty members.
Prefer the sparse map, and have the collector treat "key absent from this
batch's per_key_bytes" as "append nothing."

### E3 — Collector write() floor after R2 (the Phase-0 question, finished)
Phase 0 measured `.gz` ≈ 75 s = 16.7 floor + ~6 `.txt`-equivalent + ~52
compression. After R2 the ~52 s parallelizes to ~52/N. The collector still does:
(a) memcpy worker bytes, (b) `write_all` compressed bytes to 12 files. The
*compressed* corpus is ~5–8× smaller than the 5 GB `.txt` (so ~0.7–1 GB of
writes), and the ~6 s `.txt`-write figure was for the **full 5 GB**; writing
~1 GB of already-compressed bytes is ~**1–1.5 s**. So the post-R2 `.gz` floor ≈
16.7 (decode) + ~1.5 (compressed write) + ~52/N (compression) + format. At N=4:
≈ 16.7 + 1.5 + 13 + (~4 format) ≈ **~35 s**; at N=8 ≈ **~28 s**. The plan's "~30 s"
target is plausible at N≥4–8 but the **16.7 s decode floor is now ~half the
wall** and does NOT parallelize beyond what R3's ThreadedBamReader already gave.
So R2 gets `.gz` from 75→~30 s (2.5×) — real and worth it — but the plan should
state that decode (16.7 s) becomes the next dominant term, capping further
gains. Don't promise linear scaling past N=4.

### E4 — Channel payload size grows (memory) (note)
`WorkerOutput::Batch` changes from `Vec<RoutedCall>` (pointers/Arc-shared
qnames, ~tens of bytes/call) to per-key **compressed** byte buffers (~1.8 MB
raw → ~0.3 MB compressed per busy key per batch). With channel capacity
`n_workers*4` (parallel.rs:255) and N=8, that's 8*4=32 in-flight batches ×
~0.5 MB ≈ **~16 MB** — fine, *smaller* than shipping raw RoutedCalls would be.
But note the plan says "mind the size … reuse buffers if profiling shows alloc
churn": with compression, the *raw* per-key buffer (1.8 MB) is the churn source,
allocated+freed per batch per worker. A per-worker reusable scratch buffer
(cleared, not freed) per key is the obvious fix; the plan flags it — good.

---

## 4. Validation sufficiency

Gaps beyond C1/C2 (the decoder swap is the #1 validation hole):

- **V1 — multi-batch `.gz` input is mandatory.** Current `.gz` tests likely use
  small inputs (single member). A test with input > 2×BATCH_SIZE under `--gzip`
  comparing MultiGzDecoder output to the plain peer is the load-bearing new test.
  (Ties to C1.)
- **V2 — empty-sweep under `--gzip` with a never-written key.** Add/confirm a
  test that a directional run still **deletes** the CTOT/CTOB `.gz` files (only a
  header member, zero data members). This guards C4. The smoke already checks
  file-set match (`output_modes_phase_e_smoke.rs:513`) for plain; add the gzip
  variant with a key that receives zero data across all batches.
- **V3 — gzip level interaction (see §below).** If R2 changes the compression
  level, byte-output changes but decompressed content is identical → tests that
  assert decompressed identity still pass. Confirm **no test hashes the raw
  `.gz` bytes** (I found none — both helpers decompress first), so a level change
  is test-safe. State this in the plan.
- **V4 — Phase H gate is decompressed-content, not raw `.gz` bytes.** Confirm the
  colossal Phase H harness compares `zcat | …` and not `md5 file.gz`. If it
  hashes raw `.gz`, multi-member framing (and any level change) breaks it. The
  plan asserts the smoke accepts it but does not name the Phase H comparison
  method — pin this down (it's the binding gate, verification step 4).

### gzip level (Reviewer-B focus item)
The plan does NOT mention changing the level; `open_writer` uses
`Compression::default()` (level 6, output.rs:393). TG-OE dropped to level 1 for
−23% wall. **Recommendation:** keep level 6 for R2's first cut (parallelism
already buys the big win; don't conflate two variables). Level is a *separate*,
trivially-revertible knob: lowering it changes raw `.gz` bytes but not
decompressed content, so it's byte-identity-safe given V3. If post-R2 the ~52/N
compression term is still the bottleneck at the target N, drop to level 1 as a
follow-up and re-measure size (E1 ratio concern compounds with low level on
small members). Do not bundle it into R2 — it muddies the byte-identity
attribution and the size regression measurement.

---

## 5. Alternatives

### ALT-1 — `gzp::ParCompress` in the collector (single-writer, no worker rewrite) (STRONGLY worth comparing)
This is the simpler win the plan should explicitly weigh and reject-with-reasons.
`gzp` (parallel gzip) keeps the **single-stream** model: the collector writes
formatted bytes into a `gzp::ParCompress<Gzip>` writer per file, which fans
compression of *blocks of one logical stream* across its own thread pool and
emits a **single coherent multi-block gzip** (BGZF-style or standard). Tradeoffs
vs the plan's worker-side members:

| Axis | Plan (worker members) | gzp in collector |
|---|---|---|
| Compression ratio | fragmented per 4096-rec member (E1 risk) | near single-stream (blocks are large, tuned) |
| Code change | invasive: WorkerOutput payload, format move, append_bytes, empty-sweep rework (C4) | localized: wrap the 12 collector writers |
| Byte-identity surface | new (multi-member, per-key counts) | smaller (still single logical stream) |
| Parallelism source | the N extractor workers (shared with decode/format) | gzp's own pool (extra threads, separate from N) |
| New dep | none (flate2 already in) | adds `gzp` (+ its deflate backend) |
| `.txt` path | untouched | untouched |
| Decode/format still serial? | format moves to workers (bonus) | format stays collector-serial (~the 6 s) |

**My read:** the plan's approach has one genuine advantage gzp lacks — it also
moves **formatting** off the collector (the original R3 finding: ~8 s of
format+route+write was collector-serial). gzp only parallelizes *compression*,
leaving format/route on the collector. Since the spike attributes ~52 s to
compression and only ~6 s to format+write, **gzp captures the dominant 52 s with
far less code churn and better compression ratio**, at the cost of leaving ~6 s
of format on the collector and adding a dep + its own threads (which contend
with the N workers for cores). Given byte-identity is the expensive part of this
project, **the plan should at minimum run a half-day `gzp`-in-collector spike and
compare wall + `.gz` size + diff-size against the worker-member approach before
committing to the invasive rewrite.** This is the same "measure before invasive
rewrite" discipline the plan applied in Phase 0 — apply it to the *mechanism*
choice too, not just the go/no-go.

### ALT-2 — Bigger batches for the gzip path only
If E1 shows meaningful ratio loss, raise BATCH_SIZE on the `.gz` path (e.g.
32768) so each member is larger → fewer dictionary resets, better ratio, fewer
members. BATCH_SIZE is documented as a pure throughput knob with no correctness
impact (parallel.rs:92-101), so this is safe. Tradeoff: larger in-flight memory
and coarser reorder granularity (more latency before a batch can be emitted).
Worth keeping as a tuning lever; the plan should note BATCH_SIZE may want to
differ for gzip.

---

## 6. Interaction with R1's batched WorkerOutput (the explicit focus item)

Changing `WorkerOutput::Batch { batch_seq, results: Vec<WorkerOutputItem> }`
(parallel.rs:146-160) to carry per-key compressed bytes is **more invasive than
the plan implies** and touches R1's hard-won invariants:

- **C6a — Err handling must survive (CRITICAL to preserve).** Today
  `WorkerOutputItem::Ok { routed_calls }` and `::Err { error }` live **per
  within-batch slot**, and the collector's deterministic Err selection
  (`update_best_err`, parallel.rs:1070) depends on `(batch_seq, within_idx)`.
  If the batch payload becomes "per-key compressed bytes," the **per-item Err
  slots are erased** — you can't fold formatting+compression of Ok items into a
  per-key blob *and* keep per-item Err positions. The plan's design must keep a
  **parallel `results: Vec<WorkerOutputItem>` (or at least the Err list with
  their within_idx)** alongside `per_key_bytes`, OR move Err selection to the
  worker. Simplest: `WorkerOutput::Batch { batch_seq, per_key_bytes:
  HashMap<OutputKey, Vec<u8>>, errors: Vec<(usize, BismarkExtractorError)> }`
  where `errors` carries the within-idx of each failed item. The plan does NOT
  mention preserving Err-slot information at all — this is a real omission that
  would regress R1's byte-identical stderr on error inputs (guarded by
  `worker_preserves_order_and_keeps_err_slots_in_partial_batch`,
  parallel.rs:1302, which would then fail or need rewriting).

- **C6b — FinalDelta unchanged (good).** M-bias + SplittingReport still
  accumulate per-worker and merge at EOS (parallel.rs:1035-1047). R2 doesn't
  touch this; counters are computed in `process_se`/`process_pe` regardless of
  whether calls are shipped as RoutedCalls or compressed bytes. Confirm the
  worker still runs `increment_counters` + `mbias.accumulate` **before**
  formatting/compressing (it does today, lines 754-776) — keep that order.

- **C6c — mbias_only fast path (good, but verify).** Under `--mbias_only`,
  workers emit no RoutedCalls today (parallel.rs:748,762). Under R2 they must
  emit no per_key_bytes (empty map) — the plan says so (line 124). The collector
  must then do nothing per batch, only merge FinalDeltas. Confirm the empty-map
  batch still advances `next_emit_seq` so the reorder doesn't stall (it must:
  even an all-empty batch occupies a `batch_seq`).

- **C6d — collector reorder loop (parallel.rs:1006-1033) gets simpler, good.**
  The `for routed in &routed_calls { write_routed_call(...) }` inner loop is
  replaced by `for (key, bytes) in per_key_bytes { state.fhs.append_bytes(key,
  bytes) }`. This is strictly less work on the collector (no per-call route/
  format) — the intended win. `write_routed_call` (parallel.rs:1086) and the
  per-call `write_call` routing in the collector are dropped; `write_call` stays
  only for the single-threaded `extract_se`/`extract_pe` path. Confirm those
  legacy paths still exist and still use `write_call` (they're the byte-identity
  reference) — the plan keeps them (outline step 1) — good.

---

## Action items

### Critical
1. **C1/V1 — Swap `.gz` test decoders to `MultiGzDecoder` and add a ≥2-batch
   `--gzip` input test.** Current `decompress_gz`/`read_gz` use single-member
   `GzDecoder` (parallel_phase_f.rs:346, output_modes_phase_e_smoke.rs:180) and
   will silently validate only batch 0 — truncated multi-member output would
   pass on small inputs and fail-for-wrong-reason on large ones. This is the #1
   pre-implementation fix; without it the suite cannot detect a broken R2.
2. **C6a — Preserve per-item Err slots in the new `WorkerOutput::Batch`.** Folding
   Ok items into per-key compressed blobs erases the within-idx that
   `update_best_err` + the order/slot test depend on. Add an explicit
   `errors: Vec<(within_idx, error)>` (or equivalent) to the batch payload, or
   the R1 byte-identical-stderr-on-error invariant regresses.
3. **C4 — Re-specify the empty-sweep "data vs bytes" counter.** The header is
   bytes; the sweep deletes iff **zero data records** (output.rs:229/315). "No
   batch wrote bytes" is wrong — thread a per-key **record count** back from
   workers so directional CTOT/CTOB `.gz` files are still deleted (Phase H
   file-set + bismark2bedGraph argv gate). Specify who writes the header member.

### Important
4. **ALT-1 — Spike `gzp::ParCompress` in the collector before the invasive
   rewrite.** It captures the dominant ~52 s compression with localized code, no
   Err/empty-sweep rework, and better compression ratio (single logical stream);
   it leaves only ~6 s format on the collector. Compare wall + `.gz` size + diff-
   size vs worker-members. Apply the Phase-0 "measure before rewrite" discipline
   to the mechanism choice.
5. **E1 — Measure `.gz` size regression** from per-member dictionary resets
   (one-stream vs concatenated-4096-rec-members on a busy file). Add an accept
   threshold (e.g. < ~10% larger) to verification. Consider per-gzip-path
   BATCH_SIZE bump (ALT-2) if ratio loss is material.
6. **C2 — Commit to decompressed byte-identity (not "sorted-equivalent") for the
   R2 `.gz` path.** The design preserves order, so the stronger assertion holds
   and catches ordering bugs the sorted check would mask.
7. **C3 — State that `format_call_line` must cover BOTH the 5-col and the yacht
   8-col branches**, with `write_call` re-expressed as format+append so the
   single-threaded path exercises the same formatter (drift guard).
8. **V4/A2 — Pin down the downstream/Phase-H reader.** Confirm `bismark2bedGraph`
   and the colossal Phase H harness consume **multi-member** gzip
   (`zcat`/`gunzip`/MultiGzDecoder), not single-member, and compare decompressed
   content (not raw `.gz` md5). Perl's `--multicore` already emits multi-member
   `.gz`, so the toolchain likely tolerates it — but state it.

### Optional
9. **Defer the gzip level change.** Keep `Compression::default()` (level 6) for
   R2; treat level as a separate, byte-identity-safe follow-up knob (no test
   hashes raw `.gz` bytes — both helpers decompress). Don't bundle it.
10. **E2 — Use the sparse `HashMap<OutputKey, Vec<u8>>` (not a fixed array)** so
    empty keys emit no member; have the collector treat "key absent this batch"
    as "append nothing." Avoids zero-record members.
11. **E4 — Reuse a per-worker scratch raw buffer per key** (clear, don't free) to
    avoid per-batch alloc churn of the ~1.8 MB raw buffers.
12. **E3 — Don't promise linear scaling past N≈4** in the success criterion: the
    16.7 s decode floor becomes ~half the post-R2 `.gz` wall (~30 s), capping
    gains. Record the result against the 22.3/23.5/23.9 mimalloc baseline as
    planned, but frame the `.gz` target as ~30 s at N=4–8, not "N=4 < N=1" alone.

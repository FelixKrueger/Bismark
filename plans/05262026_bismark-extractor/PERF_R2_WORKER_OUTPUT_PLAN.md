# Plan — R2: worker-side output (#884)

## Context

After mimalloc (merged) eliminated the allocator anti-scaling and R1 (merged)
batched the channels, the extractor's `default` run is **flat ~22 s across
N=1/4/8** — producer/collector-bound, no positive scaling. R3 (parallel decode,
closed #887) showed the residual `default` serial cost is **not** decode but the
**single-threaded collector writes** (~8 s = `default` 22 s − `mbias_only` 14 s):
the collector calls `write_routed_call` → `OutputFileMap::write_call` for every
one of ~188M calls, formatting each line + routing by context×strand to one of
12 `BoxedWriter`s, all on one thread.

**R2** moves the per-call **formatting (and `--gzip` compression)** into the
workers; the collector only **concatenates pre-formatted byte chunks** per output
file in batch order. Goal: give `default` positive N>1 scaling by parallelizing
the formatting/compression that currently serializes in the collector.

## The open question R2 MUST answer first (Phase 0)

The ~8 s collector cost is `format + route + write()`. R2 parallelizes
`format`/compress but the **raw `write()` to 12 files stays collector-serial**.
**If the ~8 s is dominated by formatting/gzip → R2 wins; if by disk write() →
R2 is marginal (another R3).** We do not yet know the split. **Phase 0 measures
it before the invasive rewrite** (R3 taught us: measure, don't assume).

## Phase 0 — de-risk spike (throwaway, before any byte-identity rewrite)

Cheapest decisive probes on colossal (10M PE, shipped R1+mimalloc build), **no
code change** — just run modes × N=1/4/8 and decompose by subtraction:
- **0a. Measure THREE output modes** (Felix): `mbias_only` (no writes),
  `default` (`.txt`), and `default --gzip` (`.gz`) at N=1/4/8.
  - `(.txt − mbias_only)` ≈ format + raw 5 GB `write()` (the `.txt` floor).
  - `(.gz − .txt)` ≈ gzip-compression cost (single-threaded `GzEncoder` in the
    collector today — likely the biggest serial wall; TG-OE: ~60% of runtime).
  - Whether `.gz` *anti-scales* or is flat at N>1 shows how much R2's worker-side
    compression would recover.
- **0b. Formatting-vs-raw-write split for `.txt`** (only if `.txt` is the focus):
  a throwaway variant that formats but `write()`s to a `Sink`/`/dev/null`.
  `default_to_devnull − mbias_only` = formatting (parallelizable);
  `default − default_to_devnull` = raw write() (serial).
- **0c. (if favourable)** worker-formats/compresses prototype at N=1/4/8 to
  confirm `default N=4 < N=1` (and `.gz N=4 < N=1`).
- **Gate / likely outcomes:** R2 is clearly worth it for **`.gz`** if its
  compression is a large serial/flat cost (expected). For **`.txt`**, proceed
  only if 0b shows formatting ≫ raw-write; if raw `write()` dominates `.txt`,
  scope R2 to the gzip path (or accept the `.txt` floor — already 6.7× past
  Perl). Append `## Spike Results` here.

## Spike Results (Phase 0 — 2026-05-29) — RESCOPES R2 TO THE GZIP PATH

Shipped R1+mimalloc baseline, 10M PE, colossal:
| mode | N=1 | N=4 | N=8 |
|---|---|---|---|
| mbias_only | 16.7 | 16.7 | 17.1 |
| `.txt` | 22.9 | 25.0 | 26.4 |
| **`.gz`** | **75.2** | **77.5** | **78.2** |

Decomposition: extract/decode floor ~16.7 s; `.txt` write ≈ **~6 s** (format +
raw 5 GB write); **`.gz` compression ≈ ~52 s** (single-threaded collector
`GzEncoder`, dead **flat** across N).

**Verdict:**
- **R2 is worth it for `--gzip` ONLY** — the ~52 s serial compression wall is
  huge and fully parallelizable (per-worker `GzEncoder`). Expected: ~52 s/N at
  N≥4 ⇒ `.gz` N=4 ≈ 75 s → ~30 s.
- **`.txt` path: do NOT pursue** — only ~6 s, mostly serial raw `write()` R2
  can't help. Phase-0b (/dev/null probe) unnecessary; the `.txt` floor stays.
- **Urgency:** Perl's `--multicore` forks parallelize *its* gzip, so on the
  common `--gzip` path we are likely at parity/slower than Perl today; R2 is
  what keeps us ahead there.

**RESCOPE:** R2 = **worker-side gzip compression only** (per-worker `GzEncoder`
per batch → collector concatenates ordered multi-member gzip members per file).
The plain-`.txt` write path stays collector-side (cheap). This narrows the
byte-identity surface to `.gz` (sorted-content-equivalent, already accepted) and
leaves the raw-`.txt` path — and its raw-byte-identity — entirely untouched.

## R2 FINAL approach — gzp-in-collector (ALT-1), SUPERSEDES the worker-side-members design below

The dual plan-review (`PERF_R2_PLAN_REVIEW_{A,B}.md`) independently recommended
**ALT-1: `gzp::ParCompress` in the collector** over the invasive worker-side-
members design described in `## Design` / `## Implementation outline` below. A
throwaway spike (`spike-gzp` @ `65e5ff1`) confirmed it. **The worker-side-members
design below was NOT implemented; it is retained for context only.**

### Spike result (gzp-in-collector)
10M PE, colossal, R1+mimalloc baseline:

| mode | before | after | speedup |
|---|---|---|---|
| `.gz` (default `--parallel 1`) | ~75 s | **~18 s** | **~4.1x** |

Valid gzip (`gunzip -t` OK), works at all N. gzp parallelizes compression on its
own pool, independent of `--parallel`.

### What was implemented (R2 PR, base `rust/iron-chancellor`)
1. **`output.rs::open_writer`** — `--gzip` writers swapped from a single-threaded
   `flate2::write::GzEncoder` to `gzp::par::compress::ParCompress<gzp::deflate::Gzip>`
   (`deflate_rust` pure-Rust backend, no cmake). ~10-line change; the writer stays
   a single `Box<dyn Write + Send>`, so the collector's per-key write path,
   ordering (`batch_seq`), empty-sweep, version header, and the plain `.txt` path
   are all UNCHANGED.
2. **`GZIP_COMPRESS_THREADS = 4`** (named const) — decoupled from `--parallel` by
   design, so the common `--parallel 1 --gzip` default still gets the ~4.1x win
   (tying it to `--parallel` would leave the default single-threaded ~75 s).
3. **Cargo**: add `gzp = "=0.11.3"` (default-features off, `deflate_rust`); move
   `flate2` to `[dev-dependencies]` (now test-only — tests decode `.gz` via
   `GzDecoder`); drop the dead `flate2::write::GzEncoder`/`Compression` imports.
4. **Test**: add `parallel_gzip_multibatch_decompresses_identical_across_n_and_to_plain`
   (8199 records > 2×BATCH_SIZE) — guards single-member framing + N=1≡N=4
   decompressed-byte identity + identity-to-plain.

### Single-member framing — the dual-review Criticals are MOOT for this design
gzp's `Gzip` format emits a **single** gzip member (one header + sync-flushed
DEFLATE blocks + one stream-wide CRC32/ISIZE footer — gzp `par/compress.rs:218`
writes the header once, `:226-227` the footer once). Proven by source AND
empirically (the multibatch test decodes with single-member `GzDecoder` and
matches the plain peer). Therefore:
- **No `MultiGzDecoder` migration** (dual-review C1/C3) — those applied to the
  worker-side *multi-member* design, not gzp. Existing `GzDecoder` tests stay valid.
- **No multi-member downstream risk** (dual-review C2) — single-member `.gz` is
  what Perl already consumes.
- **Err-slot / empty-sweep rework** (Reviewer B C6a/C4) — N/A: the collector path,
  `WorkerOutput` payload, and `records_written` accounting are untouched.
- Behavioural nuance carried forward: the gzip footer is written on **Drop**
  (gzp's `Drop`→`finish()`), like flate2's `GzEncoder`. Caveat: a footer-flush
  I/O error *panics* on drop (gzp `.unwrap()`s) vs flate2's silent swallow;
  mid-stream write errors still surface as `io::Error`. Flagged for code-review.
- `.gz` raw bytes differ from the old flate2 output (framing + no cross-block
  dict on `deflate_rust`), but decompressed content is byte-identical; no test
  hashes raw `.gz`, and the colossal smoke compares `zcat | sort | md5`.

### Verification status
- Local: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test -p bismark-extractor` (102+ tests) — all PASS (2026-05-29).
- Colossal `--gzip` SE/PE `phase_h_smoke` (real-data byte-identity) — **PASS**
  (10M, `--parallel 4`): SE Perl 72s → Rust 8s (9.0×), PE Perl 167s → Rust 19s
  (8.7×); 6 `.gz` sorted-equivalent vs Perl + M-bias/splitting_report byte-identical.
- Dual code-review: **APPROVE** (no Critical/High). Shipped as PR #888 (clean
  single commit `b17dd6e`, base `rust/iron-chancellor`).

---

## Design (rescoped: gzip path only) — SUPERSEDED (worker-side-members; not shipped)

- Worker, per batch: instead of emitting `Vec<RoutedCall>`, format each call's
  line into a **per-`OutputKey` byte buffer** (`HashMap<OutputKey, Vec<u8>>` or a
  fixed array indexed by key); under `--gzip`, compress each key's batch buffer
  into a gzip **member** (`flate2 GzEncoder` per worker). Emit
  `WorkerOutput::Batch{batch_seq, per_key_bytes}`.
- Collector, per batch in `batch_seq` order: for each key, **append the bytes**
  to that key's file (raw `write_all`, no per-record formatting/routing). The
  format/route/compress work is now parallel; the collector does memcpy+write.
- `OutputFileMap` gains a raw `append_bytes(key, &[u8])` path; the line-formatting
  logic in `write_call` moves to a worker-callable function (shared, so SE/PE/
  yacht/mode dispatch stays one source of truth — guard against drift).
- M-bias + splitting_report stay collector-side (tiny; already correct).
- The empty-sweep "kept/deleted" logic still runs at finalize (a file is "empty"
  if no batch wrote bytes to it).

## Byte-identity
- Plain `.txt`: final file = concatenation of per-batch per-key chunks in
  `batch_seq` order = exactly N=1's record order → **raw byte-identical**.
- `.gz`: per-batch gzip members concatenated = valid multi-member gzip; decode-
  sorted-equivalent (smoke `zcat|sort|md5`) — allowed for data files. (Note:
  this changes `.gz` from raw-identical to sorted-equivalent vs N=1; the smoke
  already accepts that for `.gz`, but confirm no test asserts raw `.gz` identity.)
- M-bias.txt + splitting_report.txt: unchanged (collector-written, strict-byte).

## Implementation outline (parallel.rs + output.rs)
1. Extract the per-call line-formatting from `OutputFileMap::write_call` into a
   pure `format_call_line(mode, qname, chr, call, strand, yacht6, yacht7) ->
   bytes` (+ the key-routing) usable by workers; `write_call` becomes
   format-then-append (keeps the single-threaded path working/ tested).
2. Worker: per batch, format calls into per-key buffers (gzip-compress per key
   if `--gzip`); emit `WorkerOutput::Batch{batch_seq, per_key_bytes}` (replace
   `Vec<RoutedCall>`).
3. Collector: `OutputFileMap::append_bytes(key, bytes)` per key per batch in
   `batch_seq` order. Drop `write_routed_call`/per-call routing in the collector.
4. Channel payload changes from `Vec<RoutedCall>` to per-key byte buffers — mind
   the size (a batch's formatted bytes); reuse buffers if profiling shows alloc churn.

## Edge cases / risks
- **Byte-identity is the gate** (Phase H matrix). The ordering proof holds only
  if per-key chunks are concatenated strictly in batch_seq order.
- `--gzip` multi-member vs raw-identical: confirm tests + the smoke's `.gz`
  sorted-equivalence accept it.
- Memory: per-batch per-key buffers in flight × workers; bound it.
- `--mbias_only`: no split files → workers emit no bytes (fast path unchanged).
- Mode dispatch (comprehensive/merge/yacht): the moved formatter must reproduce
  the exact per-mode line + key routing — single source of truth, no drift.

## Verification
1. Phase 0 spike gate (above) — proceed only if favourable.
2. `cargo test` byte-identity suite (incl. the 8199-record boundary test) — must
   pass; add a `--gzip` N=1≡N=4 sorted-equivalence test if not present.
3. clippy `-D warnings` + fmt.
4. Phase H matrix on colossal (SE+PE, N=1&N=4) — binding byte-identity gate.
5. Perf re-measure: `default N=4 < N=1` (positive scaling) — the R2 success
   criterion; record vs the 22.3/23.5/23.9 mimalloc baseline.

## Out of scope
- Further allocator tuning; SAM/CRAM-specific paths; the `--bedGraph`/cytosine
  subprocess chain (Phase G, separate).

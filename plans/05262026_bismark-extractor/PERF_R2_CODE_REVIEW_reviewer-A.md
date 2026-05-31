# PERF R2 Code Review — Reviewer A

**Scope:** R2 of Bismark extractor perf work (#884) — swap the `--gzip` split-file
writers from `flate2::write::GzEncoder` to gzp's parallel-gzip
`gzp::par::compress::ParCompress<gzp::deflate::Gzip>` (the "gzp-in-collector" /
ALT-1 design).

**Diff reviewed:** `git -C /Users/fkrueger/Github/Bismark-extractor diff 8a2a147 -- rust/`
(5 files: `output.rs`, `lib.rs`, `Cargo.toml`, `Cargo.lock`, `tests/parallel_phase_f.rs`).

**Verification performed:**
- Read gzp 0.11.3 source: `par/compress.rs`, `deflate.rs`, `lib.rs`, `Cargo.toml`.
- Read the full `output.rs`, `state.rs` finalize sequence, `parallel.rs` collector
  ownership, `output_mode.rs` file-count, and the test file + helpers.
- Ran `cargo test -p bismark-extractor --test parallel_phase_f` → **17 passed, 0 failed**
  (incl. the new multibatch test; sweep output confirms empty `.gz` members are
  deleted under gzip).
- Ran `cargo clippy --all-targets -- -D warnings` → **clean**.

---

## Summary / Verdict

**APPROVE.** The change is correct, well-scoped, and the central claims hold up
against the gzp source. Every byte-identity-critical path (plain `.txt`, collector
ordering, version header, empty-sweep accounting, M-bias / splitting-report) is
genuinely untouched — the diff only swaps the `gzip == true` arm of
`open_writer`. The single-member-gzip claim is verified directly in gzp's source
(one `header()` + Sync/Finish DEFLATE blocks + one `footer()` per stream), and
gzp's own `test_simple`/`test_simple_drop` decode `Gzip` output with a plain
single-member `GzDecoder`, exactly matching this code's decode path. `deflate_rust`
correctly avoids `any_zlib`, so `needs_dict()==false` (no cross-block dictionary,
no cmake/C). No `MultiGzDecoder` migration is needed.

The only findings are non-blocking: a thread-oversubscription concern at high
`--parallel` whose magnitude is understated by the code comment (Medium), the
inherited panic-on-drop-footer-error behavior (Low, documented + accepted), one
stale doc comment (Low), and two test-coverage gaps (Low). None block merge.

All claims 1–6 from the brief were checked and **verified**; details inline below.

---

## Claims verification (per the brief)

1. **Single-member gzip — VERIFIED.** `ParCompress::run` (`par/compress.rs:218`)
   writes `format.header()` exactly once before the chunk loop, and
   `format.footer(&running_check)` exactly once after (`:226-227`).
   `Gzip::encode` (`deflate.rs:96-100`) uses `FlushCompress::Sync` for non-last
   chunks and `FlushCompress::Finish` for the last — i.e. sync-flushed DEFLATE
   blocks within a single member. `Gzip::footer` (`deflate.rs:136-142`) emits
   `CRC32` + `ISIZE` once. gzp's own `test_simple`/`test_simple_drop`/`test_regression`
   (`deflate.rs:716-780, 917-960`) decode `Gzip` output with plain
   `GzDecoder` — authoritative confirmation. The test fixture's 8199 records
   cross multiple internal blocks and still decode with single-member
   `GzDecoder`. Claim holds.

2. **Panic-on-drop — VERIFIED, accepted.** gzp `Drop` (`par/compress.rs:310-315`)
   calls `self.finish().unwrap()` when the handle/channels are still live. A
   footer-flush I/O error at drop → panic, vs flate2 `GzEncoder::Drop`'s silent
   swallow. Mid-stream errors still surface as `io::Error` via `Write::write`
   (`:341-370`). See Low-1 — acceptable for a CLI tool; hardening deferred.

3. **`GZIP_COMPRESS_THREADS = 4`, decoupled from `--parallel` — correct rationale,
   understated cost.** Decoupling is the right call (the common `--parallel 1
   --gzip` path would otherwise stay single-threaded). But see Medium-1: in
   Default mode 12 files are eagerly opened and held open for the whole run, each
   spawning 4 compressor + 1 writer threads → ~60 gzip threads concurrently,
   independent of `--parallel`. The doc comment ("a fixed pool of 4") describes
   per-file, not aggregate, behavior.

4. **`flate2` → `[dev-dependencies]` — VERIFIED.** No functional `src/` use of
   flate2 remains (`grep` shows only doc/comment mentions; both `use flate2::*`
   imports removed). gzp pulls flate2 transitively with `rust_backend`. Workspace
   pin `=1.1.9` satisfies gzp's `>=1.0.25` requirement; the lock has a single
   flate2 (1.1.9). Tests decode via `flate2::read::GzDecoder` (dev-dep). Correct.

5. **Byte-identity preservation — VERIFIED.** The diff touches only the
   `gzip == true` branch of `open_writer` (`output.rs:419-431`). Plain `.txt`
   path, `write_call` body + `batch_seq` ordering (single-collector writer,
   `parallel.rs:1056`), version-header write (`output.rs:126-128`),
   `records_written` accounting, M-bias.txt, and splitting-report writing are
   unchanged. Raw `.gz` bytes change (different framing + no dictionary);
   decompressed content is identical, as the passing tests assert.

6. **The new test — VERIFIED, with gaps.** `parallel_gzip_multibatch_…` (8199 >
   2×4096) genuinely guards single-member framing (single-member `GzDecoder`
   decode of a multi-block stream — a multi-member regression would truncate and
   fail `== plain`) and N-independence (n1 vs n4 decompressed-byte identity). The
   sweep log in the run confirms it also exercises empty-key `.gz` deletion
   (e.g. `CpG_CTOT_large.txt.gz was empty -> deleted`). Gaps: SE-only, no PE; no
   explicit small-multibatch fixture beyond this one. See Low-3.

---

## Issues by area

### Logic
- **No correctness defects found.** The single-writer-per-file invariant
  (collector owns the only `OutputFileMap`) means each `.gz` file is produced by
  exactly one `ParCompress` → exactly one gzip member. Empty-sweep semantics are
  unchanged: `records_written` counts call rows (not bytes), so a header-only
  gzip member is still classified empty and unlinked — verified live in the test
  output.

### Efficiency
- **Medium-1 (below): thread oversubscription** is the one efficiency concern.
- Minor: `OutputFileMap::flush_all` → `BufWriter::flush` → `ParCompress::flush` →
  `flush_last(false)` always enqueues at least one chunk even when the internal
  buffer is empty, producing an extra empty Sync DEFLATE block per flush. Harmless
  (valid DEFLATE, footer CRC unaffected) and flush_all is called once per run.
  Not worth changing.

### Errors
- **Low-1 (below): panic-on-drop-footer-error**, and the related observation that
  `cleanup_all` (error path) drops gzp writers whose `Drop` may panic via
  `finish().unwrap()` if the underlying disk error that triggered cleanup also
  fails the footer flush — a panic there would mask the original error. Low
  severity (error path only, CLI process about to exit). gzp also `.unwrap()`s
  `core_affinity::get_core_ids()` inside its writer thread (`par/compress.rs:181`);
  inherited, returns Some on Linux/macOS, not fixable here.

### Structure
- **Low-2 (below): stale doc comment** at `output.rs:366` still says "the inner
  GzEncoder, if any" inside `cleanup_all` — it's now a gzp `ParCompress`. Every
  other doc/comment in the file was updated; this one was missed.
- The `.expect("GZIP_COMPRESS_THREADS is nonzero")` on the builder's
  `num_threads()` result (`output.rs:426`) is appropriate — it can only fire if
  the const is set to 0, a compile-time-visible programmer error. Good.
- Cargo.toml comments are thorough and accurate (deflate_rust rationale, flate2
  pin reconciliation, dev-dep move). gzp pulls a second `thiserror 1.0.69` into
  the lock (transitive); acceptable, no action needed.

---

## Findings (priority-ranked)

### Medium-1 — Thread oversubscription at high `--parallel`; comment understates aggregate thread count
`output.rs:384-395` frames `GZIP_COMPRESS_THREADS = 4` as "a fixed pool of 4". In
reality, `OutputFileMap::new` eagerly opens **one `ParCompress` per split file**
and holds them all open for the entire run (until `finalize_with_empty_sweep`).
In Default mode that's 12 files; each `ParCompress::from_writer` spawns
`num_threads (4)` compressor threads **plus 1 writer thread** (`par/compress.rs:104-121,
182-215`). So `--gzip` Default mode runs **~60 gzip threads** (12 × 5)
concurrently, on top of the pipeline's N workers + producer + collector —
**independent of `--parallel`**. At `--parallel 8` on a typical 8–16-core box this
is meaningful oversubscription.

Mitigating facts: the gzp compressor threads block on `rx.recv()` and only one
file is actively receiving heavy traffic at a time in practice, so they are not
all CPU-hot simultaneously; per-writer buffered memory is bounded (~1 MiB:
`num_threads*2 = 8`-slot channels × 128 KiB `BUFSIZE`), ~12 MiB total. The spike's
~4.1x win was measured, so the net effect is positive. **No correctness impact.**

Recommendation (non-blocking): update the `GZIP_COMPRESS_THREADS` doc to state the
aggregate (e.g. "Default mode opens 12 files, so ~48 compressor + 12 writer
threads run concurrently; bounded because most block on empty input channels").
Optionally consider lowering to 2–3, or revisiting once the colossal multi-file
`--gzip` smoke measures real CPU contention. Defer the value change to a measured
follow-up; just fix the comment now.

### Low-1 — Panic-on-drop footer error not propagated as Result (documented, accepted)
gzp `Drop` calls `finish().unwrap()` (`par/compress.rs:312`). A footer-flush I/O
error at drop becomes a panic, unlike flate2's silent swallow. The code documents
this honestly (`output.rs:410-413`). Hardening (explicit `finish()` to propagate
`Result`) would require de-type-erasing `Box<dyn Write + Send>` — not worth it now.
For a CLI tool, panicking with a non-zero exit on a disk-full-at-footer condition
is arguably *better* than flate2 silently producing a truncated `.gz`.
**Recommendation:** keep as-is; track as a documented follow-up only if a
graceful-error contract is later required. Note in passing: `cleanup_all` (error
path, `output.rs:354-381`) can hit this panic-on-drop and mask the original error
— acceptable since the process is already terminating on error.

### Low-2 — Stale "GzEncoder" doc comment in `cleanup_all`
`output.rs:366`: `// Explicitly close the writer (and the inner GzEncoder, if any)`
— should read "gzp `ParCompress`". Cosmetic; every other reference was updated.
**Recommendation:** one-line comment fix.

### Low-3 — Test coverage gaps: no PE+gzip, mid-write-failure smoke still skipped
The gzip integration tests are SE-only (no `extract_pe_parallel` + `--gzip`).
Risk is low because `open_writer` and the single-collector writer are
SE/PE-agnostic — PE differs only in record routing, which the existing PE
byte-identity tests already cover (plain). The
`smoke_gzip_cleanup_on_write_failure_removes_gz_files` smoke remains intentionally
skipped (portability; `/dev/full` is Linux-only, per
`output_modes_phase_e_smoke.rs:17-24`), so the panic-on-drop / cleanup-under-error
path has no automated coverage. **Recommendation (optional):** add one PE+gzip
multibatch decode test mirroring the new SE one; consider a Linux-gated
`/dev/full` write-failure test in a follow-up. Non-blocking.

---

## Fixes I would apply
None applied (review-only, per skill instructions). The only change I'd recommend
applying before merge is the trivial Low-2 doc fix; Medium-1's comment update and
the test additions can ride along or follow up.

## Bottom line
Approve for merge. The gzp-in-collector design is correctly implemented and
byte-identity-safe; the single-member and decode claims are verified against gzp
source and pass the new regression test. The dual plan-review's MultiGzDecoder /
multi-member Criticals were written against the superseded worker-side-members
design and **do not apply** to this gzp-in-collector implementation (single writer
per file ⇒ single member). Address Medium-1's comment and Low-2 at your
convenience; Low-1 and Low-3 are documented/optional follow-ups.

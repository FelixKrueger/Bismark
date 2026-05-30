# PERF R2 Code Review — Reviewer B

**Scope:** R2 of Bismark extractor perf work (#884) — swap the `--gzip` split-file
writers from single-threaded `flate2::write::GzEncoder` to gzp's parallel-gzip
`gzp::par::compress::ParCompress<gzp::deflate::Gzip>` ("gzp-in-collector" / ALT-1).

**Diff reviewed:** `git -C /Users/fkrueger/Github/Bismark-extractor diff 8a2a147 -- rust/`
Files: `rust/bismark-extractor/src/output.rs`, `src/lib.rs`, `Cargo.toml`,
`rust/Cargo.lock`, `tests/parallel_phase_f.rs`.

**Independent verification performed:**
- Read gzp 0.11.3 source: `par/compress.rs`, `deflate.rs`.
- Read surrounding `output.rs`, `state.rs`, `output_mode.rs`, `parallel.rs`,
  and the test helpers in `parallel_phase_f.rs`.
- Re-ran `cargo test -p bismark-extractor --test parallel_phase_f` (17 passed),
  `cargo clippy --all-targets -- -D warnings` (clean), and `cargo tree -i flate2`
  / `-e features -i gzp`.

---

## Summary

**Verdict: APPROVE.** The change is correct, the byte-identity-of-decompressed-content
contract holds, the dependency wiring is clean, and the new regression test
genuinely guards single-member framing + N-independence. All six claims I was
asked to scrutinize hold up against the gzp source. There are **no Critical or
High issues**. The findings below are Medium/Low — the only one worth a decision
before merge is the eager thread-spawn footprint (Medium), which is a resource
tradeoff, not a correctness bug.

This is materially lower-risk than the SUPERSEDED worker-side-members design the
plan reviewers flagged: their multi-member "Criticals" do **not** apply to gzp,
because gzp emits one stream-wide member (verified below), not one member per
worker/batch.

---

## Claim-by-claim verification

### 1. Single-member gzip — CONFIRMED (was the load-bearing Critical risk)
`ParCompress::run` (`par/compress.rs:217-228`) writes `format.header()` **once**
before the receive loop, streams each compressed chunk in order, then writes
`format.footer()` **once** with a single stream-wide `running_check` (CRC32 +
ISIZE). `Gzip::header` (`deflate.rs:112-133`) is the standard 10-byte gzip header;
`Gzip::footer` (`deflate.rs:135-142`) is CRC32 + ISIZE. Internal chunks use
`FlushCompress::Sync` for non-last and `Finish` for the last block
(`deflate.rs:96-100`) — sync-flushed DEFLATE blocks inside **one** member. gzp's
own `test_simple` (`deflate.rs:716`) and `test_simple_drop` (`:750`) decode
`ParCompress<Gzip>` output with a plain single-member `GzDecoder`. The diff's
claim is accurate. The existing `GzDecoder`-based tests do **not** silently
truncate.

### 2. Panic-on-drop — CONFIRMED, acceptable as-is (Low, documented)
`Drop for ParCompress` (`par/compress.rs:310-315`) calls `self.finish().unwrap()`.
`finish()` → `flush_last(true)` → joins the writer thread, which is where the
footer is written (`run()` `:226-228`); any footer-flush I/O error becomes a
panic. flate2's `GzEncoder::Drop` swallows the equivalent error silently.
This is acceptable: the only realistic footer-flush failure is ENOSPC at the very
end of writing an output file, and a panic (loud, with a non-zero exit) is a
*safer* failure mode than flate2's silent swallow (which could leave a
truncated-but-uncomplained `.gz`). It is correctly documented at
`output.rs:410-412`. See L1 below for the one residual nuance (panic mid-sweep).

### 3. `GZIP_COMPRESS_THREADS = 4` decoupled from `--parallel` — sound rationale, but see M1
The reasoning at `output.rs:384-395` is correct: the default run is `--parallel 1`
and single-threaded gzip is the dominant serial wall, so tying the pool to
`--parallel` would leave the common path single-threaded. A fixed pool captures
the win on every `--gzip` path. **However**, the resource footprint at high
`--parallel` is underweighted — see M1.

### 4. `flate2` → dev-deps — CONFIRMED clean
- No non-test `src/` code references `flate2` (grep shows only doc-comment
  mentions). The `use flate2::...` imports were removed from `output.rs`.
- `cargo tree -i flate2` resolves to a **single** `flate2 v1.1.9`, shared by
  gzp, noodles-bgzf, and noodles-cram. The `=1.1.9` pin in `[dev-dependencies]`
  matches and does not create a second version. No duplicate-crate bloat.
- Note (not a defect): `flate2` is still a **runtime** transitive dependency via
  gzp's `deflate_rust` feature (`cargo tree -e features -i gzp` shows
  `deflate_rust → flate2`). So flate2 still compiles into the binary; moving it
  to dev-deps only changes the *direct* dependency edge, not whether flate2 is
  linked. The Cargo.toml comment is accurate ("the src `--gzip` writer is now
  gzp, not flate2"), but a reader could misread it as "flate2 no longer linked."
  Minor (L2).
- `default-features = false` + `deflate_rust` correctly avoids the
  `libdeflate`/`zlib` C backends (no cmake). `needs_dict()` returns
  `cfg!(feature = "any_zlib")` (`deflate.rs:80`), which is **false** here, so the
  cross-block dictionary is skipped — confirming the diff's claim that compressed
  bytes differ from flate2 but decompressed content is identical.

### 5. Byte-identity — CONFIRMED
Only the gzip writer **construction** changed (`open_writer` `output.rs:417-433`).
Verified unchanged: plain `.txt` branch (`Box::new(file)`), `write_call` row
formatting (`output.rs:208-223`, the Phase-B-locked 5-col format), the
`SPLIT_FILE_HEADER` write (`output.rs:127`), `records_written` bump-after-success
(`output.rs:229`), the empty-sweep gating on `records_written == 0`
(`output.rs:316`), and collector `batch_seq` ordering (`parallel.rs`). The
`flush_all`→`drop` sequencing is correct: gzp `Write::flush` calls
`flush_last(false)` (sync block, **no footer**, `par/compress.rs:379-381`); the
footer is written once at `drop`/`finish`. So one header + one footer per file.
Decompressed content is byte-identical to the plain peer and N-independent.

### 6. New test quality — GOOD, with minor gaps (L3)
`parallel_gzip_multibatch_decompresses_identical_across_n_and_to_plain`
(`parallel_phase_f.rs:813`) uses 8199 records = 2×`BATCH_SIZE`(4096)+7, so the
stream spans multiple internal sync-flushed blocks — exactly the case that would
expose a multi-member regression. `decompress_gz` (`:346-350`) uses a
single-member `GzDecoder` + `read_to_end`: on a hypothetical multi-member stream
that reads only member 0 and stops (no error), producing truncated output that
fails `assert_eq!(decoded, plain)`. So the guard is real. It also asserts
cross-N (`n1` vs `n4`) decompressed-byte identity. This is a meaningful addition
over the existing single-batch `parallel_gzip_n4_...` test. Gaps noted in L3.

---

## Issues by area

### Logic
- None. The framing, ordering, flush/drop sequencing, and empty-sweep
  interaction are all correct.

### Efficiency
- **M1 (Medium) — eager thread-spawn footprint scales with file count × 4, not with `--parallel`.**
  `from_writer` (`par/compress.rs:104-131`) spawns **1 writer thread immediately**,
  and `run()` (`:182`) spawns `num_threads` (=4) **compressor threads**. So each
  gzipped writer = **5 OS threads**, spawned **eagerly at `OutputFileMap::new`
  time** (`output.rs:125`), before any data is written. In **Default mode (the
  common case)** there are **12 split files** (`output_mode.rs:97-110`), i.e.
  **~60 threads** spun up on every `--gzip` run regardless of `--parallel`.
  - This includes strands that receive **zero records** (e.g. CTOT/CTOB in a
    directional library): those 5 threads spin up, sit blocked on `recv()`, and
    are torn down at the empty-sweep `drop`. Confirmed live in my test run — the
    log shows 8 of 12 files `was empty -> deleted` while their gzp pools had
    nonetheless been allocated.
  - The threads are idle-blocked (not busy-spinning), so steady-state CPU is fine.
    The cost is thread-creation/teardown overhead and ~60 thread stacks (default
    8 MiB virtual reservation each on Linux → ~480 MiB *virtual*, far less RSS).
    For a long-running extraction this is negligible; the concern is purely the
    "60 threads for 12 files, most receiving little/no data" smell, which gets
    worse if MergeNonCpG (8 files) or future modes add files.
  - **Recommendation:** This is acceptable for v1 (correctness is fine and the
    perf win is real), but consider one of: (a) lower `GZIP_COMPRESS_THREADS` to
    2–3 (the spike's 4.1x came from 4 threads against *one* serialized wall; with
    12 concurrent files the per-file pool oversubscribes the box anyway); or
    (b) add a one-line note in the `GZIP_COMPRESS_THREADS` doc acknowledging that
    the **total** gzip thread count is `4 × open_file_count`, not 4. The current
    doc frames "4 threads" as the whole story, which understates the footprint
    at 12 files. **Do not block merge on this** — flag for the author's decision.

### Errors
- **L1 (Low) — a panic during the `finalize_with_empty_sweep` drop loop aborts remaining files.**
  `finalize_with_empty_sweep` (`output.rs:283-343`) iterates entries and
  `drop(writer)` each (`:304`). If one writer's footer-flush panics (gzp
  `.unwrap()`), the loop unwinds and the remaining entries' files are neither
  swept nor logged. With flate2 this couldn't happen (silent swallow). In
  practice a footer-flush panic means the FS is already failing (ENOSPC), so a
  hard abort is defensible and arguably better than limping on. The other
  writers' gzp threads would still be joined via *their* `Drop` during unwind
  (each `OutputFileEntry` owns its writer; unwinding drops the not-yet-iterated
  `entries` Vec). No thread leak. **No action required**; documenting that "a
  footer panic aborts the sweep" alongside the existing `output.rs:410-412`
  caveat would be a nice-to-have.

### Structure
- **L2 (Low) — Cargo.toml dev-dep comment could be misread.** The comment at
  `Cargo.toml` (dev-deps `flate2`) says "the src `--gzip` writer is now gzp, not
  flate2," which is true, but flate2 is **still linked transitively** via gzp's
  `deflate_rust`. A future reader auditing "can we drop flate2?" might wrongly
  conclude it's test-only. One clause ("flate2 remains a runtime transitive dep
  via gzp's deflate_rust backend; this entry only adds the direct test edge")
  would prevent that. Cosmetic.

- **L3 (Low) — test coverage gaps in the new test.**
  - **PE not covered.** The new test is SE-only. This is *low* risk because
    `extract_pe`/`extract_pe_parallel` route through the identical
    `OutputFileMap`/`open_writer` — the gzip writer construction is mode-agnostic,
    so there's no SE-vs-PE divergence in the gzip path. Existing PE gzip coverage
    exists elsewhere in `parallel_phase_f.rs` (e.g. `extract_pe_parallel` at
    `:442`). Acceptable.
  - **Empty `.gz` sweep (CTOT/CTOB) is exercised but not asserted.** With a
    directional fixture, CTOT/CTOB files are eager-opened (header + gzp pool),
    written 0 records, and unlinked by the sweep (confirmed in the run log). The
    test's `for entry in read_dir` only sees *surviving* files, so it never
    asserts anything about the empty-then-deleted gz files (correct — they're
    gone). There is no test that an empty-but-kept `.gz` (header-only, no records)
    decodes to the header line, but that path doesn't occur in practice (sweep
    removes `records_written==0`), so this is a non-gap. No action.

---

## Fixes I would apply
None required for correctness. If the author wants to act on the optional items:
1. (M1) Reduce `GZIP_COMPRESS_THREADS` to 2–3 **or** extend its doc to note the
   `4 × file_count` total. Decision, not a defect.
2. (L2) One clarifying clause in the Cargo.toml dev-dep comment about flate2
   remaining a runtime transitive dep.

(I did not edit source, per instructions.)

---

## Recommendations with priority

| Priority | Item | Ref |
|----------|------|-----|
| Critical | none | — |
| High | none | — |
| Medium | M1: gzip pool = `4 × open_file_count` threads (≈60 in Default mode), spawned eagerly incl. zero-record strands; consider lowering const to 2–3 or documenting the true total | `output.rs:384-395`, `output.rs:125`, `par/compress.rs:104-131,182`, `output_mode.rs:97-110` |
| Low | L1: footer-flush panic in the sweep `drop` loop aborts remaining files (FS-failure-only; defensible) | `output.rs:304`, `par/compress.rs:310-315` |
| Low | L2: Cargo.toml dev-dep comment understates that flate2 is still a runtime transitive dep via gzp `deflate_rust` | `Cargo.toml` dev-deps |
| Low | L3: new test is SE-only (mode-agnostic writer → low risk); empty-gz sweep exercised but not asserted (non-gap) | `parallel_phase_f.rs:813` |

**Build/test status (re-verified locally):** `parallel_phase_f` 17/17 pass;
`clippy --all-targets -D warnings` clean; `cargo tree` confirms single flate2
v1.1.9 and `deflate_rust` feature active.

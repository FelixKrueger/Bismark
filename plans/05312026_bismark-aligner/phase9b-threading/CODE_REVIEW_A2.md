# CODE REVIEW A2 — Phase 9b fix-pass DELTA re-review

**Reviewer:** A (independent, fresh context)
**Scope:** ONLY the post-first-review fix-pass delta on `rust/aligner-v1` @
`~/Github/Bismark-aligner` — i.e. the changes folded after `CODE_REVIEW_A.md`/
`CODE_REVIEW_B.md` (both APPROVE, no Critical/High) + plan-manager `COVERAGE.md`.
Not a re-litigation of the whole phase (the prior reviews + `PLAN.md` §13 cover it).

The delta:
1. `parallel.rs` — new `panic_message()` + the two `thread::scope` join sites surfacing
   the panic payload instead of a fixed string (documented deviation from PLAN §3.8:
   panic→clean `Err` rather than re-panic).
2. `tests/cli.rs` — `canon_bam` rewritten to a RAW `noodles_bam` reader; SE/PE helpers
   now also read back + assert the `.ambig.bam` across N (closes A-M3/B-M1); PE refactor
   + new `worker_invariance_pe_non_directional` (B-M2); new `make_fake_bowtie2_align_fails`
   + `worker_error_propagates_no_hang` (§9 #10).

## Verdict

**APPROVE — the delta is clean. No Critical, no High, no Medium.** Every changed line
is correct and the new tests genuinely exercise their target paths and cannot
false-pass on the points the prompt asked me to check. Two Low notes (one carry-over
hygiene item, one pre-existing test-strength observation), neither blocking.

Build/lint/test (run locally, sandbox disabled, from `~/Github/Bismark-aligner/rust`):
- `cargo test -p bismark-aligner -- --test-threads=2` → **201 lib + 39 integration = 240 green**, 0 failed
  (was 238; +2 = `worker_invariance_pe_non_directional` + `worker_error_propagates_no_hang`).
- `cargo clippy -p bismark-aligner --all-targets -- -D warnings` → clean (exit 0).
- `cargo fmt -p bismark-aligner -- --check` → clean (exit 0).

---

## 1. `panic_message` + panic→Err mapping (`parallel.rs:597-611`, join sites `:674`/`:817`)

**Downcast correct & exhaustive — CONFIRMED.** `JoinHandle::join()` returns
`Result<T, Box<dyn Any + Send + 'static>>`. The standard library only ever boxes a panic
payload as either `&'static str` (`panic!("literal")`) or `String`
(`panic!("{}", …)` / `std::panic::panic_any(String)`). `panic_message` downcasts to
`&str` then `String`, falling back to `"unknown panic"`. This is the idiomatic and
practically-exhaustive pair; anything else (a `panic_any` of a custom type) degrades to
the safe fallback rather than misbehaving. Correct.

**`e.as_ref()` is the right hand-off — CONFIRMED.** In `unwrap_or_else(|e| …)`,
`e: Box<dyn Any + Send>`. `e.as_ref()` yields `&(dyn Any + Send)`, which is exactly the
`panic_message(payload: &(dyn std::any::Any + Send))` parameter type. `downcast_ref` on
the borrowed trait object is the correct read-only inspection (no move/clone of the box,
no `downcast` consuming it). Compiles + clippy-clean.

**Mapping panic→Err loses nothing that matters — CONFIRMED.** I traced orphan-safety
through the scope unwind:
- The `.map(|h| h.join()…)` chain runs **inside** the `std::thread::scope` closure, so
  every worker thread is already `join`ed (the panicking one included) by the time the
  payload is observed. A panicking worker unwinds its own stack first; its
  `AlignerStream`/`PairedAlignerStream` `Drop` (`align.rs:255-261`/`461-465`,
  kill-then-wait) runs during that unwind, reaping its Bowtie 2 child **before** `join`
  returns the box. So no orphan on the panic path — the same guarantee the `Err` path
  has, and exactly what the in-code deviation comment (`:599-602`) and the prior reviews
  (A-M2 / B-L-2) claim.
- The only thing surrendered vs. PLAN §3.8's "re-panic on join" is process-abort
  semantics; in exchange the CLI gets a deterministic exit 1 with the panic text
  preserved in the error message. This is the strictly-friendlier behaviour the prior
  reviewers flagged and is now documented as a deviation in-code. Functionally sound.

No finding.

## 2. `canon_bam` raw-reader rewrite (`tests/cli.rs:1551-1559`)

**Raw reader is correct/equivalent for the MAIN BAM — CONFIRMED.** I verified from
`drive_merge` (`lib.rs:653-729`) that the main BAM sink (`sinks.bam`) receives a record
**only** on `Decision::UniqueBest`; `NoAlignment`→`--unmapped` aux, ambiguous→ambig
aux/`.ambig.bam`, `Rejected`→nowhere. So the main BAM never contains a flag-4 unmapped
record, and the old reader's dropped unmapped-filter is **inert** for it. The switch
from `bismark_io::BamReader::records()` (which validates XR/XG/XM) to raw
`noodles_bam::io::Reader` + `record_bufs` is required for the `.ambig.bam` (tagless raw
records — the exact `MissingTag{XR}` that the prompt notes panicked the first ambig
assertion) and is harmless for the main BAM (whose records carry the tags but the raw
reader simply doesn't check them).

**`Debug`-per-record is a sound canonical equality — CONFIRMED.** `RecordBuf`'s derived
`Debug` is deterministic and renders every decoded field (name, flags, ref-id, pos, mapq,
cigar, seq, qual, data/aux tags). Two BAMs with identical *decompressed* content yield
identical `Vec<String>`; any reorder, drop, or single-field difference changes the vector
and fails the `assert_eq!`. This matches the gate's stated semantics (byte-identical
**decompressed** content — not raw BGZF block bytes, which is correct since block framing
is not the invariant). It also uses the **same** raw read path the production merge
(`merge_bams`, `parallel.rs:527-534`) uses, so the test mirrors production decoding.

No finding.

## 3. New `.ambig.bam` invariance assertions (SE `:1629`/`:1658`, PE `:1774`/`:1800`)

**Right files, would catch a reorder — CONFIRMED (SE).** `run_se_parallel` reads back
`{stem}_bismark_bt2.ambig.bam` and `assert_se_worker_invariant` compares element 4 across
N∈{2,4,8} vs N==1. The SE fake routes `a`-class reads (AS==XS on CT) to the ambig BAM and
`write_mua_reads` cycles `m`/`u`/`a` so `a` records straddle chunk boundaries — so the
SE ambig assertion compares a **non-empty, multi-chunk** ambig merge. A wrong chunk order,
a dropped ambig record, or a header-skip bug in the ambig merge would flip the `Debug`
vector and fail. This is the real closure of A-M3/B-M1 and genuinely exercises the
raw-tagless merge path that was the gate-found bug.

**PE ambig assertion is structurally correct but content-empty — see L2.** The PE fake
(`make_fake_bowtie2_pe_content_addressed`) emits only `m`/`u` (no ambiguous class), so the
PE `.ambig.bam` is header-only at every N. The assertion still reads back a real file
(the merge always creates it when `--ambig_bam` is set, even with empty parts — verified
via `open_chunk_pe_sinks`/`merge_bams` always-create) and compares empty-vs-empty, so it
is non-vacuous but does not prove PE ambig **records** merge in order. The SE cells cover
the real ambig-record merge; PE ambig records are left to the oxy gate. Noted as L2
(carry-over of B-M2's residual), not a delta defect.

## 4. PE refactor + `worker_invariance_pe_non_directional` (`:1839-1853`)

`run_pe_parallel` / `assert_pe_worker_invariant` / `write_pe_mu_reads` are a clean
extraction; the new non-directional cell passes `--non_directional` (4-slot PE fan-out)
and asserts the full tuple (PE BAM records, report-minus-wallclock, `_1`/`_2` unmapped raw
gz, pe.ambig.bam) invariant across N∈{2,4,8}. This is exactly the B-M2 gap (the 4-slot PE
merge under chunking) and it passes. Correct and a real coverage gain.

## 5. `worker_error_propagates_no_hang` (`:1858-1898`) — does NOT false-pass

This was the headline concern; I traced it end to end:

- **Fails at alignment, NOT detection.** `make_fake_bowtie2_align_fails` returns **exit 0
  on `--version`** (`*--version*) … exit 0`) and exit 1 otherwise. Detection
  (`aligner::detect_bowtie2`, `aligner.rs:50` runs `--version` and requires
  `status.success()`) happens inside `resolve()` (`config.rs:167`) **before** `pipeline()`
  dispatches to `run_se_multicore`. So detection **succeeds**; the first non-zero exit is
  the chunk worker's Bowtie 2 alignment call inside `process_se_chunk`. The test therefore
  exercises the chunk-worker error path, not an early detection failure. ✓
- **It's the clean `Err` path (returned), not the panic path.** With `exit 1` + no stdout,
  `AlignerStream::spawn` reads an empty stream (`read_line` → 0 → `current = None`,
  `align.rs:202-203` — no parse error, no panic), the merge sees an empty stream, and
  `AlignerStream::finish` (`align.rs:241-251`) observes the non-zero exit →
  `Err(Validation("Bowtie 2 exited unsuccessfully …"))`. That `Err` returns from
  `se_chunk_job` → is collected by `collect_in_order` (`?` on the lowest-index error) →
  `pipeline` → `run` → `main` exits 1. So this validates the §9 #10 **error** propagation
  (deterministic clean error → exit 1), which is the higher-value path. (The `panic_message`
  branch itself remains covered only by inspection — see L1; it is trivial pure logic.)
- **No hang.** All 4 chunks fail fast (immediate `exit 1`); `std::thread::scope` joins all
  workers before returning; each worker's stream `Drop` reaps its child. assert_cmd has no
  built-in timeout, so a deadlock would hang the harness rather than pass — the test
  completing in ~12-15 s confirms no deadlock. The `.failure().code(1)` assertion pins both
  the no-hang and the exact exit code. ✓

No finding — the test is sound and cannot false-pass on the routes the prompt asked about.

---

## Findings

### L1 (Low) — `panic_message`'s panic→Err branch has no dedicated test
`worker_error_propagates_no_hang` exercises the **returned-`Err`** chunk path, not the
**panic** path (the fake errors via non-zero exit, which `finish()` turns into a clean
`Err` *without* unwinding a worker). So `panic_message` + the `unwrap_or_else(|e| …Err…)`
mapping is validated by inspection only. The logic is trivial (two `downcast_ref`s + a
fallback) and orphan-safety on the panic path is structurally guaranteed (§1 above), so
risk is negligible. *Optional:* a unit test that `panic!`s inside a `thread::scope` worker
and asserts `panic_message` returns the payload — but this is gold-plating, not required.

### L2 (Low) — PE `.ambig.bam` assertion is content-empty (carry-over of B-M2)
The PE fake emits no ambiguous class, so the PE ambig-BAM invariance check compares
header-only-vs-header-only across N — non-vacuous (a real file is read back) but it does
not prove PE ambig **records** merge in chunk order. The SE cells DO cover the real
multi-chunk ambig-record merge (which shares `merge_bams` with PE), and the oxy gate
(§9 #11) covers the full PE matrix with real Bowtie 2. Acceptable as-is; noting for the
plan-manager that the PE-ambig-records path is SE-covered + oxy-deferred, not directly
asserted PE-side.

### L3 (Low, carry-over, OUTSIDE the delta) — stray crate-dir artifacts still present
`rust/bismark-aligner/reads_bismark_bt2.bam` and `…_SE_report.txt` (first review's
A-L3/B-L5) are **still** in the working tree (regenerated 13:12, more recent than the
11:52 the first review saw — likely a manual re-run during the fix-pass), still NOT
gitignored (`git check-ignore` → not ignored). They'd be swept up by `git add .`. Not
produced by the suite (all tests use `TempDir`). *Recommend:* delete before committing the
9b work. (Flagged only because it persists into the fix-pass; not part of the reviewed
delta.)

---

## Bottom line
The fix-pass is **clean**. The `panic_message` downcast is correct and exhaustive,
`e.as_ref()` is the right hand-off, and panic→`Err` preserves orphan-safety via the
streams' `Drop` during scope unwind (a sound, documented deviation from PLAN §3.8). The
`canon_bam` raw-reader rewrite is correct and equivalent for the main BAM (no flag-4
records there) and is the right tool for the tagless `.ambig.bam`; `Debug`-per-record is a
sound decompressed-content equality. The new SE `.ambig.bam` assertions genuinely exercise
the multi-chunk raw-tagless merge (closing A-M3/B-M1), the PE non-dir cell closes B-M2, and
`worker_error_propagates_no_hang` truly exercises the chunk-worker alignment-error path
(not detection, no hang) and cannot false-pass. 240 tests green, clippy/fmt clean. Only two
non-blocking Low notes (one a carry-over hygiene item outside the delta).

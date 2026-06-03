# Code Review B2 — Phase 9b fix-pass DELTA (post-review)

**Reviewer:** B2 (independent, fresh context) — FOCUSED re-review of the post-review
fix-pass DELTA only. The full phase already passed dual code-review
(`CODE_REVIEW_A.md`/`CODE_REVIEW_B.md`, both APPROVE, no Critical/High) +
plan-manager COMPLETE (`COVERAGE.md`). I did **not** re-litigate the whole phase.
**Scope:** the four delta items in `rust/bismark-aligner/src/parallel.rs`,
`rust/bismark-aligner/tests/cli.rs`, and the `Cargo.toml`/`Cargo.lock`
`noodles-bam` addition that supports them.
**Worktree:** `/Users/fkrueger/Github/Bismark-aligner`, branch `rust/aligner-v1`, uncommitted.
🔴 **RECOMMEND ONLY — no source modified.**

## Verdict

**APPROVE — the delta is clean. No Critical, no High, no Medium.** Three Low /
informational notes, all non-blocking and none affecting gated output bytes. The
fix-pass correctly closes the convergent Medium findings (A-M3/B-M1 ambig-BAM
cross-N, B-M2 PE non-dir matrix, A-M1/B-M3 §9 #10 worker-error, A-M2/B-L2 panic
payload) without regressing N==1 byte-frozen behaviour or the prior invariants.

Build/lint/test (run locally from `~/Github/Bismark-aligner/rust`, sandbox disabled):
- `cargo fmt -p bismark-aligner -- --check` → **clean**.
- `cargo clippy -p bismark-aligner --all-targets -- -D warnings` → **clean**.
- `cargo test -p bismark-aligner -- --test-threads=2` → **201 lib + 39 integration = 240 green**, 0 failed (incl. all 6 new/changed 9b tests). Matches the PLAN's claimed 240.

---

## Delta item review

### 1. `panic_message` + the two `scope` join sites (`parallel.rs:597–611`, `:674–679`, `:817–822`)

**Downcast correct & exhaustive — YES.** `JoinHandle::join()` returns
`Result<T, Box<dyn Any + Send + 'static>>`. `panic_message(payload: &(dyn Any + Send))`
downcasts `&str` then `String`. These are exactly the two concrete payload types the
standard panic machinery produces: `panic!("literal")` → `&'static str`,
`panic!("{}", x)` / `unwrap`/`expect`/`format!`-based panics → `String`. The
`downcast_ref::<&str>()` idiom resolves the concrete type to `&'static str` (the only
`&str` that satisfies the `Any: 'static` bound), so string-literal panics ARE caught —
this is the canonical std-panic-hook idiom. Anything else (custom `panic_any`, never
used in this crate) falls to `"unknown panic"`. Exhaustive for all realistic panics.

**`e.as_ref()` correct — YES.** `e: Box<dyn Any + Send + 'static>`; `.as_ref()` yields
`&(dyn Any + Send + 'static)`, which coerces to the `&(dyn Any + Send)` parameter (the
`Any: 'static` bound makes the lifetime elision sound). Compiles clean (`-D warnings`).

**panic→Err vs re-panic loses nothing that matters — CONFIRMED.** Orphan-safety holds
independently of the orchestrator's choice: when a worker panics, it unwinds **its own**
stack first, running `Drop` for that worker's `AlignerStream`/`PairedAlignerStream`
(kill+reap the Bowtie 2 child, `align.rs:255–262`/`461–`). The panic is captured in that
worker's `JoinHandle`; `std::thread::scope` then guarantees **every** sibling is joined
(all handles are explicitly `.join()`ed in the `.map(...)`), so every sibling's Drop runs
too → no orphan on the panic path, same as the `Err` path. Mapping to a clean
`Validation` error (CLI exit 1) only changes the *exit mode* (deterministic exit 1 vs
SIGABRT), not orphan-safety. The deviation from PLAN §3.8 ("re-panics on join") is
explicitly documented in the fn doc-comment AND in PLAN §13 "Deviations". Sound and the
cleaner behaviour.

### 2a. `canon_bam` rewrite to raw `noodles_bam::io::Reader` + `record_bufs` + `Debug` (`cli.rs:1551–1559`)

**Correct/equivalent for the MAIN BAM — YES.** The old reader's two behaviours both fall
away harmlessly: (i) the **unmapped-filter** is inert because the main Bismark
`_bismark_bt2.bam` never contains unmapped (flag-4) records — unmapped reads route to the
separate `_unmapped_reads.fq.gz` aux, verified end-to-end by the green
`unmapped_routing_*` tests; (ii) the **XR/XG/XM validation** is a *validator* the cross-N
diff doesn't need — `RecordBuf`'s derived `Debug` serialises ALL fields including the
`data` aux map (where XR/XG/XM live), so a tag drop/reorder across N would still surface
in the string comparison. The new reader is a weaker absolute-correctness validator but an
equally-strong cross-N **differ** — which is precisely this test's job (absolute
correctness is owned by the 32 byte-identity tests + the oxy gate). Required because the
raw `--ambig_bam` holds tagless records that the validating reader rejects (the exact
trap that broke the first impl, PLAN §13).

**`Debug`-per-record is sound canonical equality — YES.** Two records with byte-identical
*decompressed* content decode to identical `RecordBuf`s → identical `Debug`. The merge
writes via `write_raw_record` (same `write_alignment_record` path as single-core
`write_record`), so decompressed content is byte-identical N>1 vs N==1; the Debug Vec
captures order + every field. `worker_invariance_se_empty_chunk_at_high_n` empirically
proves `canon_bam` returns identical Vecs across N on tagged main-BAM records.

### 2b. New `.ambig.bam` cross-N assertions (`run_se_parallel`/`run_pe_parallel`, `cli.rs:1629/1774`)

**Compares the right files & catches a reorder — YES (SE).** SE reads
`{stem}_bismark_bt2.ambig.bam`, which matches the production
`derive_output_path(read, cfg, "_bismark_bt2.ambig.bam", ".ambig.bam")` for `reads.fq`
(`stem=reads`, no prefix/basename → `reads_bismark_bt2.ambig.bam`). `write_mua_reads(13)`
emits 4 `a`-class reads (`a0003/a0006/a0009/a0012`) with **distinct IDs**, which land in
different chunks at `--parallel {2,4,8}`; a merge-order bug would reorder these 4 records →
the `canon_bam` Vec changes → `got.4 != base.4` fires (`assert_se_worker_invariant`). The
SE cell genuinely exercises the raw-merge reorder path (and earns its keep — it's the path
the original missing-XR bug lived in). PE reads `reads_1_bismark_bt2_pe.ambig.bam`,
matching `derive_output_path(read_1, ..., "_bismark_bt2_pe.ambig.bam", "_pe.ambig.bam")`.

### 2c. PE refactor + `worker_invariance_pe_non_directional` (`cli.rs:1743–1853`)

`run_pe_parallel` / `assert_pe_worker_invariant` / `write_pe_mu_reads` are a clean
extraction; the new `worker_invariance_pe_non_directional` adds `--non_directional` →
exercises the 4-slot PE fan-out under chunking (closes B-M2). 13 `m`/`u` pairs over
`--parallel {2,4,8}`; `m`-pairs produce distinct mate records, so a PE merge reorder is
detectable in `got.0` (PE BAM). Green.

### 2d. `make_fake_bowtie2_align_fails` + `worker_error_propagates_no_hang` (`cli.rs:1858–1898`)

**Genuinely exercises the chunk-worker alignment-error path — YES; no false-pass.** I
traced the SE multicore order: `--parallel 4` → `config.multicore=4>1` → `run_se_multicore`
(`lib.rs:118–119`) → `read_genome_into_memory` (valid FASTA `>chr1\nACGTACGT`) →
`split_contiguous` (valid 13-read file) → spawn workers → each worker
`open_chunk_se_sinks` → `process_se_chunk` (spawns Bowtie 2). The fake's `--version`
branch exits 0, so **detection passes** (it does NOT false-pass by dying at detection);
the first failure is the per-worker Bowtie 2 `exit 1`, which `AlignerStream::finish`
(`align.rs:241–251`) maps to `Err(Validation("Bowtie 2 exited unsuccessfully"))` →
propagated by `?` out of `process_se_chunk`/`se_chunk_job` → returned as the chunk's
`Result` → `collect_in_order` `?`s it → CLI exits 1. The assertion is `.failure().code(1)`
— a panic-abort would exit via SIGABRT (signal, not code 1) and FAIL this assertion, so
the test also confirms the clean-Err (not re-panic) behaviour. "No hang" is real: if the
scope deadlocked, `assert_cmd` would block and the test would time out rather than return.
Green.

### 3. `noodles-bam = "=0.89.0"` dep + `Cargo.lock` (`Cargo.toml:40`)

Pin matches bismark-io's transitive choice; `Cargo.lock` shows a single shared
`noodles-bam 0.89.0` (added under bismark-aligner's deps). Same crate used by the
production `merge_bams` and the test `canon_bam` — no version skew. Comment documents why
(raw tagless records). Correct.

---

## Findings (all Low / informational — non-blocking)

### L-1 (Low) — `panic_message` and the actual worker-panic join path are untested
No test forces a worker to *panic* (the error test exercises the `Err` path via Bowtie 2
exit-1, not a panic). `panic_message` (the `&str`/`String`/fallback downcast) has no
direct unit test, and the `h.join().unwrap_or_else(...)` panic branch is never taken in
the suite. The function is trivial and exhaustive on inspection, the realistic worker
failure is `Err` (covered), and a worker panic is a should-never-happen internal bug — so
risk is minimal. *Optional:* a 3-line unit test asserting
`panic_message(&Box::new("x") as &(dyn Any+Send))` etc., or a fake that makes a worker
panic, would close the only untested line in the delta.

### L-2 (Low) — the PE `.ambig.bam` cross-N assertion is vacuous (header-only)
`run_pe_parallel` passes `--ambig_bam`, but `make_fake_bowtie2_pe_content_addressed` emits
only `m`/`u` (no `a` class), so `reads_1_bismark_bt2_pe.ambig.bam` is a **header-only**
BAM (`merge_bams` still writes the header + BGZF EOF for zero parts). `canon_bam` returns
an empty Vec, so `got.4 == base.4` (empty == empty) at every N regardless of correctness —
the PE 5th-element assertion cannot catch a PE-ambig reorder. This is **not a false-fail
and not a regression** — it's exactly the design PLAN §13 documents ("PE ambiguous/
`--ambig_bam` merge path is format-agnostic → covered by the SE `a` cells"), and the SE
cells DO exercise a multi-record ambig merge (L-2 in CODE_REVIEW_B's M-1, now closed for
SE). Noting only that the PE ambig assertion is presently inert; the SE coverage +
format-agnostic merge + oxy gate back it. *Optional:* add an `a` class to the PE fake to
make the PE ambig assertion load-bearing too.

### L-3 (Low) — stray untracked artifacts (pre-existing, NOT delta)
`rust/bismark-aligner/reads_bismark_bt2.bam` and `_SE_report.txt` still present — flagged
by both prior reviewers (A-L3/B-L5) and PLAN §13 "Open" as a leaked CWD run, not 9b
output. `rm` before committing so `git add .` doesn't sweep them in. Outside this delta;
the fix-pass tests all use `TempDir`.

---

## Bottom line
The fix-pass DELTA is **correct and clean**. The panic-payload downcast is exhaustive for
real panics, `e.as_ref()` is the right hand-off, and panic→Err preserves orphan-safety via
Drop-during-unwind (documented deviation). The `canon_bam` raw-reader rewrite is
sound-and-stronger for the cross-N differ (unmapped-filter inert on the main BAM; `Debug`
captures all fields incl. tags). The new SE `.ambig.bam` assertion genuinely catches a
reorder; `worker_error_propagates_no_hang` exercises the real chunk-worker alignment-error
path (detection passes, alignment fails, clean exit 1 — no false-pass). 240 tests green,
clippy/fmt clean with `--test-threads=2`. The three Low notes (untested panic line,
vacuous PE-ambig assertion, stray artifacts) are non-blocking and consistent with the
documented design + the pending oxy gate (§9 #11).

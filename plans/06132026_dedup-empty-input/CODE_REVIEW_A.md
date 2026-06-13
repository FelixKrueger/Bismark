# Code Review A — `deduplicate_bismark_rs` graceful zero-alignment handling

**Reviewer:** A (independent) · **Date:** 2026-06-13
**Branch / worktree:** `rust/dedup-empty-input` @ `~/Github/Bismark-dedup`
**Spec:** `plans/06132026_dedup-empty-input/PLAN.md` (rev 1)
**Verdict:** **APPROVE** (0 Critical / 0 High / 0 Medium / 3 Low)

---

## Summary

The change relaxes the zero-records guard at all **8** `pub fn run_*` entry points in
`pipeline.rs` so a header-only (or all-unmapped → FLAG-4-filtered-to-empty) input is handled
**gracefully**: a valid header-only deduplicated output + a zero-count `deduplication_report.txt`
rendering `0 (0.00%)` + exit 0. This is a deliberate, well-documented divergence from Perl v0.25.1
(which dies on empty input), motivated by nf-core/methylseq pipeline robustness, and confirmed
correct by the project owner.

I verified the diff against the live source, traced the data flow through every entry point and
the streaming/report/state internals, and ran the full gate. The implementation is **correct,
isolated, and faithful to the rev-1 plan**:

- All 8 entry points: the empty record iterator is a genuine no-op (`stream_se`/`stream_pe`
  `for`/`loop` is never entered → no PE `UnpairedFinalRecord` on empty), `open_writer` + `finish`
  emit a valid header-only file, `into_report` yields `count=0`.
- The 4 `--multiple` variants open the writer with `headers[0]` first, stream the first reader
  directly with `refid_tables[0]`, and the subsequent-files loop **preserves `let i = i_zero_based
  + 1` / `refid_tables[i]` unchanged** — no off-by-one introduced.
- The UMI/parallel `--multiple` variants **retain** their `(|| {...})()` closure +
  `cleanup_partial_output_on_err(output, …)` wrapper. The writer is opened *before* the closure, so
  the empty path returns `Ok` → cleanup is NOT triggered → the header-only output is correctly
  retained; genuine mid-stream errors still unlink the partial output.
- `report.rs`: `count == 0` renders `("0.00", "0.00")` (hardcoded, no `0/0` NaN). The change is
  isolated to the `count == 0` arm; the non-empty division branch is byte-for-byte unchanged.
- `error.rs`: `EmptyInput` retained; the 4 defensive `inputs.is_empty()` (empty file-LIST) guards
  still error (`pipeline.rs:358/520/855/990`). Doc corrected.
- `main.rs`: an info stderr line on `report.count() == 0` in both `process_one`/`process_multiple`,
  positioned before `write_to`/`format_stderr` → exit 0 unchanged. The bclconvert empty-peek comment
  de-staled and is functionally inert on empty (opens its own reader, returns `Ok(())`, does not
  disturb the pipeline's fresh reader).

**Gate (run locally, sandbox-escalated for this sibling worktree):**
- `cargo test -p bismark-dedup` → **all green**: 86 lib + 39 integ + 2 conformance + 7 sanity + 1
  doctest; 9 real-data byte-identity tests `ignored` (oxy-only) as expected. 0 failed.
- `cargo clippy -p bismark-dedup --all-targets -- -D warnings` → clean.
- `cargo fmt -p bismark-dedup -- --check` → clean.

No fixes were applied (no defects warranted them). The three findings below are all Low-severity
documentation-accuracy nits.

---

## Issues by area

### Logic — none

- **8-entry-point refactor (verified correct).** Read every `run_*` in full
  (`pipeline.rs:303-335` single, `350-442` multiple, `474-502`/`512-594` parallel,
  `809-843`/`846-939` UMI, `943-977`/`981-1072` parallel-UMI). Each single-file variant feeds
  `reader.records()` / `records_with_umi(...)` straight into `stream_*`; each `--multiple` variant
  opens the writer with `headers[0].clone()`, streams `first_reader` with `refid_tables[0]`, then
  loops `readers_iter.enumerate()` with `i = i_zero_based + 1` → `refid_tables[i]`. The `+1` is
  correct precisely because `readers_iter` was pre-advanced by `.next()` for file1.
- **PE empty safety (verified).** `stream_pe` (`pipeline.rs:251-286`) and `stream_pe_umi` enter
  their `loop` only on `iter.next()` returning `Some`; an empty iterator breaks immediately →
  **no `UnpairedFinalRecord`**. Confirmed by the passing `empty_input_pe_is_graceful` test.
- **`count == 0` is reachable ONLY on truly-empty input (verified).** `DedupState::observe`
  (`dedup.rs:140`) and `UmiDedupState::observe` (`dedup.rs:308`) **always** `self.count += 1` per
  record/pair. So `count == 0` ⟺ `observe` never called ⟺ zero records streamed. The `0.00%`
  rendering therefore cannot leak into any non-empty output — it is structurally confined to the
  graceful-empty path. (`format_removed_zero_no_duplicates`, count=100/removed=0, correctly hits the
  *else* branch and already renders `0 (0.00%)`.)
- **`leftover()` on empty (verified).** `leftover = count - removed = 0 - 0 = 0` — no u64
  underflow on the empty path.
- **bclconvert pre-check on empty (verified).** `check_bclconvert_format_conflict`
  (`main.rs:293-308`) opens its OWN reader, reads one record for autodetection, and returns
  `Ok(())` on `None`. The pipeline opens a fresh reader later, so there is no stream-consumption
  interaction; on empty it correctly falls through to the graceful path.

### Efficiency — none

- The change *removes* work (the `.peekable()` peek / `first_record` stash). Empty input is now
  `O(1)` after header clone + writer open/finish. Streaming loops unchanged at `O(records)`.

### Errors — none

- `EmptyInput` variant retained and still used by the 4 defensive empty-file-LIST guards
  (`pipeline.rs:358/520/855/990`), each preserved verbatim. `NoInputFiles` (referenced by the
  updated `EmptyInput` doc) exists (`error.rs:111`) and is the real upstream guard
  (`cli.rs:201`). Genuine mid-stream errors in the UMI/parallel `--multiple` paths still propagate
  through the `(stream_result, finish_result)` match → `cleanup_partial_output_on_err` unlinks the
  partial output (`pipeline.rs:932-938`, `1065-1071`).

### Structure — 3 Low nits (documentation only)

- **L-1 (stale doc comments — peek removed).** Two comments still reference the now-removed
  peek-before-writer-open mechanism for the parallel functions:
  - `pipeline.rs:453` — `// - Reuse all of: peek-before-writer-open, chr-name interning, …`
  - `pipeline.rs:468` — `/// - Peek-before-writer-open empty-input detection`
  These are factually wrong after this change (the peek is gone). PLAN step A.5 explicitly listed
  doc/comment de-staling (it named line ~619, but these two sibling sites carry the same stale
  claim and were missed). Purely cosmetic — no functional impact. Suggest: drop the
  "peek-before-writer-open" bullet / reword to "graceful empty handling (header-only output +
  count=0 report)".
- **L-2 (test docstring vs fixture count).** `all_unmapped_input_is_graceful`
  (`integration_dedup.rs:585-602`) docstring says "build a BAM with 1-2 FLAG-4 records" but the
  fixture writes exactly **one** (`build_record("r1", …, 0x4, …)`). The test is valid either way
  (one FLAG-4 record is sufficient to exercise the read-side unmapped filter); the docstring is just
  slightly loose. Cosmetic.
- **L-3 (block-comment "v1.1: … Reuse all of:").** Same root as L-1, in the section banner at
  `pipeline.rs:444-461`. Listed separately only because it's a module-section comment vs a `///`
  doc-comment; the fix is the same one-line reword.

---

## Targeted answers to the review prompts

1. **Did relaxing each guard preserve all OTHER semantics?** Yes. Only the peek-None / first-record
   peek-stash `Err(EmptyInput)` arms were removed. Chr-interning, refid-table build, `@SQ`/format
   validation (which runs *before* any record access — see `run_multiple` 364-389, fired by the new
   `multiple_mode_sq_mismatch_fires_when_file1_is_empty` test), the `+1` subsequent-files indexing,
   the `cleanup_partial_output_on_err` wrappers, and `finish()` are all untouched. No borrow/lifetime
   issues (compiles + clippy clean); `BismarkRecord` import (`pipeline.rs:25`) is still used widely;
   no `.peekable()` left dangling.

2. **Is the `all_unmapped_input_is_graceful` fixture valid?** Yes. `build_record(…, 0x4, …)` →
   `write_bam` → `BismarkRecord::from_noodles_record` (write side does NOT filter unmapped), so the
   BAM physically contains the FLAG-4 record. On read-back, `bismark_io`'s reader drops FLAG & 0x4
   (`rust/bismark-io/src/read.rs:637/657`, with its own dedicated test at 1122-1128) before dedup
   sees it → zero records → graceful path. It genuinely exercises the read-side unmapped filter.

3. **Is the V10/11b (reordered-`@SQ` off-by-one guard) omission SOUND?** **Yes — concur.** Two
   independent reasons: (a) with file1 empty, `refid_tables[0]` is never *used* (no file1 records
   call `compute_*_key`), and file2's records key via `refid_tables[1]` regardless; (b) more
   fundamentally, dedup writes the **original record bytes verbatim** (`writer.write_one(&record)`)
   — the refid_table only feeds the dedup *key* (drop/keep decision), never the on-disk refid. So a
   hypothetical wrong table for file2 would change *which* records are kept, not their bytes; with a
   single/non-colliding file2 record the keep/drop set is identical under any bijective table → the
   adversarial test cannot distinguish correct from off-by-one. The `+1` is **structurally
   preserved** (verified at `pipeline.rs:431, 583, 922, 1055`) and guarded by the new
   `multiple_mode_empty_file1_still_processes_file2` test (asserts file2's pair flows through with
   the right qname). The intent of V10 is met structurally; the specific test as specified is not
   constructible. Reasonable, well-documented deviation.

4. **Does `0.00%` risk any non-empty byte-identity behavior?** No. It is confined to the
   `self.count == 0` arm (`report.rs:113-114`), which is reachable only on zero-record input (see
   L/Logic above). The non-empty `%.2f` division branch is byte-for-byte unchanged, and
   `format_matches_perl_byte_for_byte_typical_case` + the 10M real-data test still pass. Perl emits
   no zero-count report (it dies), so there is no oracle to contradict — `0.00%` is a strictly
   safer, numeric token for downstream parsers (MultiQC).

---

## Recommendations (prioritized)

- **Critical:** none.
- **High:** none.
- **Medium:** none.
- **Low (optional, non-blocking):**
  - L-1 / L-3: reword the two stale "peek-before-writer-open" comments at `pipeline.rs:444-461`
    (banner) and `pipeline.rs:453`, `468` so the parallel-function docs match the new graceful
    behavior.
  - L-2: tighten the `all_unmapped_input_is_graceful` docstring ("1-2" → "one") to match the fixture.

These are documentation-accuracy nits only; none affect correctness, output bytes, or the gate. The
change is ready to merge as-is.

---

## Verdict

**APPROVE.** The 8-entry-point relaxation is correct and faithful to rev-1: empty input is a
genuine no-op producing valid header-only output + a `count=0`/`0.00%` report + exit 0 across SE/PE
× single/`--multiple` × parallel × UMI; the `--multiple` `+1` refid indexing and the
`cleanup_partial_output_on_err` wrappers are preserved; `0.00%` is provably isolated to the empty
path. Full gate (test + clippy + fmt) green. The V10-omission rationale is sound (dedup writes
verbatim record bytes; the refid_table only drives the key, so the adversarial test is structurally
unconstructible with an empty file1). Only 3 Low documentation nits, all optional.
**0 Critical / 0 High / 0 Medium / 3 Low.**

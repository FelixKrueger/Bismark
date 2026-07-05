# Code Review B — `deduplicate_bismark_rs` graceful zero-alignment handling

**Reviewer:** B (independent) · **Date:** 2026-06-13 · **Branch:** `rust/dedup-empty-input`
**Scope:** the diff making all 8 dedup entry points graceful on a zero-record (header-only) BAM.

---

## Summary

The change is **correct, minimal, and well-targeted**. It removes the zero-records `EmptyInput`
guard from all 8 `pub fn run_*` entry points and lets the empty record iterator flow through the
existing `stream_*` / `finish` / `into_report` machinery as a no-op, producing a valid header-only
output + a `count=0` report + exit 0. The `--multiple` refactor correctly removes **only** the
first-record peek-stash error path while preserving the pop-first + `i = i_zero_based + 1` /
`refid_tables[i]` indexing — the single highest-risk hazard the plan flagged (A-I1/B-#4) is handled
exactly as specified. The `cleanup_partial_output_on_err` wrappers in the UMI/parallel `--multiple`
variants are preserved, so genuine mid-stream errors still delete partial output while the empty
case (now `Ok`) retains the header-only file. `report.rs` switches the `count==0` rendering to
`0.00%` in a strictly count-gated branch; non-empty rendering is byte-unchanged. The defensive
`inputs.is_empty()` (empty **file list**) guards stay erroring, and `EmptyInput` remains a live
variant.

All quality gates pass locally: **135 tests** (86 lib + 39 integ + 2 conformance + 7 sanity + 1
doctest, 0 failed; 9 real-data byte-identity ignored as expected), `clippy --all-targets -D
warnings` clean, `cargo fmt --check` clean.

**Adversarial verdict: I could not break the change.** Every item on the checklist holds. The only
findings are two stale comments (Low) and one documentation/test-coverage observation re: V10
(Low/informational). No Critical, no High.

---

## Adversarial checklist results

1. **PE empty path — PASS.** `stream_pe` (`pipeline.rs:251-286`) / `stream_pe_umi` (`770-801`) enter
   a `loop` whose first action is `iter.next()`; on an empty iterator that returns `None → break`
   immediately. The `UnpairedFinalRecord` arm only fires when `r1` was `Some` but `r2` is `None`,
   which cannot happen on an empty iterator (the loop body is never reached after the first `break`).
   No panic. Confirmed by the passing `empty_input_pe_is_graceful` test.

2. **`--multiple` refid indexing — PASS (correct).** In `run_multiple` (`pipeline.rs:409-438`) the
   first reader is consumed via `readers_iter.next()` and streamed with `refid_tables[0]`; the
   subsequent loop does `let i = i_zero_based + 1; … refid_tables[i]`. Because `readers_iter` has
   already had its first element popped, `enumerate()` restarts at `0` for the **second** reader, so
   `+1` maps it back to `refid_tables[1]`. The `+1` is preserved verbatim. `refid_tables` has exactly
   `headers.len() == readers.len()` entries (`386-389`), so `refid_tables[i]` for `i ∈ [1, n)` is
   in-bounds. No out-of-bounds, no mis-map. Identical structure verified in
   `run_multiple_parallel` (`562-590`), `run_multiple_umi` (`891-929`), `run_multiple_parallel_umi`
   (`1026-1062`).

3. **UMI/parallel `--multiple` cleanup wrapper — PASS.** The `(|| { … })()` closure capturing
   `stream_result` is intact in `run_multiple_umi` (`903-931`) and `run_multiple_parallel_umi`
   (`1036-1064`), as is the `finish()` + `cleanup_partial_output_on_err(output, final_result)` tail
   (`932-938` / `1065-1071`). On the empty path: `stream_result = Ok(())`, `finish_result = Ok(())`
   → `final_result = Ok(report)` → cleanup is a no-op → header-only output **retained** (correct).
   On a genuine mid-stream error: `stream_result = Err(e)` → `final_result = Err(e)` → cleanup
   `remove_file(output)` (correct). The refactor moved the `writer`-open above the
   `readers_iter.next()` but did **not** change the finish/cleanup ordering or the closure boundary.
   The single-file UMI variants (`run_single_umi` `831-842`, `run_single_parallel_umi` `965-976`)
   keep their own `match (stream_result, finish_result)` + cleanup tail unchanged.

4. **Resource/finish on empty path — PASS.** Every variant reaches `writer.finish()` on the empty
   path: the single-file non-UMI variants call it unconditionally after the `if is_paired { … }`
   block (`run_single:333`, `run_single_parallel:500`, `run_multiple:440`,
   `run_multiple_parallel:592`); the UMI variants call `writer.finish()` as `finish_result` before
   building `final_result`. No early-return now skips `finish()` — the only former early-return (the
   `EmptyInput` guard) was the one removed. So the BGZF EOF block is always written → valid BAM. The
   passing `assert_header_only_output` (opens the output via `bismark_io::open_reader`, asserts 0
   records + `@SQ` preserved) confirms the trailer is valid across SE/PE/parallel/UMI/`--multiple`.

5. **`0.00%` isolation — PASS.** `report.rs:113` gates the literal `("0.00", "0.00")` strictly on
   `if self.count == 0`; the `else` branch is the unchanged `sprintf("%.2f")` math. `count == 0`
   ⟺ zero analysed records (a header-only input), so no non-empty output can hit it. The renamed
   unit test `format_renders_zero_pct_when_count_is_zero` (`report.rs:210-218`) asserts the exact
   bytes `0 (0.00%)` and `0 (0.00% of total)`. The unchanged `format_removed_zero_no_duplicates`
   (count=100, removed=0) still asserts `0 (0.00%)` removed + `100 (100.00% of total)` — proving the
   non-empty zero-removed case is **not** affected by the `count==0` branch. No NaN risk (`0/0` is
   never computed; the literals are hardcoded).

6. **Defensive `inputs.is_empty()` (4 sites) — PASS.** `pipeline.rs:358`, `:520`, `:855`, `:990`
   all still `return Err(BismarkDedupError::EmptyInput(PathBuf::new()))` on an empty **file list**.
   `EmptyInput` remains a live variant (`error.rs:31`) with a corrected doc comment that now
   describes the file-list-empty case and explicitly documents the zero-records graceful divergence.
   Empty file list (distinct from zero records) is preserved as an error.

7. **Tests — PASS.** `all_unmapped_input_is_graceful` (`integration_dedup.rs`) builds a FLAG `0x4`
   record via `build_record("r1", …, 0x4, …)` and `write_bam`. Verified `bismark_io`'s reader
   filters FLAG `0x4` **before** classification (`read.rs:636-637`: `if (flags & 0x4) != 0 { drop }`
   inside `filter_unmapped_then_classify`), so the record never reaches the XR/XG-dependent
   classify step — the reader presents zero records and the graceful path fires. The fixture's XR/XG
   tags (added by `build_record`) are therefore irrelevant; the filter strips the record regardless.
   Test passes. The inverted tests assert `.success()` + output existence + zero-count report via the
   two path-agnostic helpers `assert_header_only_output` (re-opens BAM, asserts 0 records + non-empty
   `@SQ`) and `assert_zero_count_report` (`.contains(...)` substring matches, not full-path equality)
   — robust and path-agnostic. `multiple_mode_empty_file1_still_processes_file2` additionally asserts
   `read_records(out).len() == 2`, `read_qnames == ["u0"]`, count `:\t1`, and `1 (100.00% of total)`
   — proving file2 actually flows through `refid_tables[1]` correctly. `multiple_mode_all_files_empty_is_graceful`
   and `multiple_mode_sq_mismatch_fires_when_file1_is_empty` (B-#3, validation-before-peek still
   fires) both pass.

8. **V10 omission — argument is CORRECT.** I independently reconstructed the reasoning. With file1
   empty there is **no cross-file dedup interaction**: file2's records only dedup against each other.
   `compute_*_key` translates each record's own `reference_sequence_id()` through the per-file table
   it is streamed with. File2 is streamed with `refid_tables[1]` (its own table). For any input
   record refid `r`, `refid_tables[1][r]` is a deterministic bijection of file2's `@SQ` order onto
   the workspace intern. Since (a) dedup keys only need to be **self-consistent** within the analysed
   set, and (b) the **output records are byte-identical to the input records** (dedup writes the
   record verbatim, not a re-encoded refid — `stream_*` calls `writer.write_one(&record)` with the
   original `BismarkRecord`), a hypothetical off-by-one that fed `refid_tables[0]` instead of
   `refid_tables[1]` would still map file2's refids through *some* bijective table and yield the
   **same retained set and same output bytes** — so an empty-file1 test genuinely **cannot**
   distinguish correct from off-by-one. The off-by-one is only observable when **two non-empty files
   with reordered `@SQ` cross-dedup**, which is a pre-existing `--multiple` path unrelated to this
   fix. The implementation **structurally** preserves the `+1` (verified at all 4 multiple sites), so
   the regression cannot occur. **Net: V10's intent is met structurally; the specific adversarial
   test as specified is genuinely not constructible in the empty-file1 scenario.** The omission is
   acceptable and honestly documented (PLAN.md "DEVIATION" note). See Low-3 for a non-blocking
   suggestion that *would* guard the indexing more directly if a regression guard is wanted.

---

## Issues by area

### Logic
- **None blocking.** The empty-iterator-as-no-op approach is sound across all 8 paths. PE
  `UnpairedFinalRecord` cannot fire on empty input. `finish()` always runs. `count==0` gating is
  airtight.

### Efficiency
- **None.** The change removes work (the peek/peek-stash). Empty input is `O(1)` after header
  clone + writer open/finish. No new allocations on any path.

### Errors
- **None blocking.** Error semantics preserved: empty file list still errors; mid-stream errors
  still clean up partial output; genuine read/format/UMI errors still propagate via `?`.

### Structure
- **Low-1 (stale comment):** `pipeline.rs:453` — the v1.1 block comment still reads
  `"- Reuse all of: peek-before-writer-open, chr-name interning, stream_se / stream_pe, …"`. The
  peek-before-writer-open behavior was removed by this change; this line now describes code that no
  longer exists.
- **Low-2 (stale doc):** `pipeline.rs:468` — `run_single_parallel`'s doc bullet still reads
  `"- Peek-before-writer-open empty-input detection"`. Same staleness. (The plan's step A.5 called
  out updating "the `run_single_parallel`'s comment at 619", but these two earlier sites at 453/468
  carry the same now-false claim and were not updated.) Both are cosmetic — they do not affect
  behavior, tests, or the public contract — but they contradict the (correct) inline comments added
  elsewhere and could mislead a future reader. The module-level doc at `pipeline.rs:12`
  (`"empty-input detection"`) is borderline but still technically accurate (the file *does* detect
  empty file lists), so I'd leave it or soften it.

### Documentation / provenance
- The `error.rs` doc, `report.rs` comment, the 8 inline `pipeline.rs` comments, the inverted-test
  rustdoc, and the conformance-suite top-of-file note are all accurate, cross-reference the plan, and
  correctly frame this as a deliberate divergence from Perl. Good provenance discipline.
- The PLAN.md mentions step E.13 (a `rust/README.md` Milestones line) — **not present in this diff**.
  That is an out-of-crate doc touch and may be intended for the release/PR step (PLAN §F); flagging
  only so the plan-manager pass can confirm it lands before merge. Not a code defect.

---

## Fixes applied
None. (All findings are Low/cosmetic; per the review brief I apply only low-risk fixes and note
them — I judged even the stale-comment edits better left to the author to keep the review
non-mutating, since they are pure prose and the author may want to reword the whole v1.1 block.)

---

## Recommendations (prioritized)

- **Critical:** none.
- **High:** none.
- **Medium:** none.
- **Low-1:** Update the stale `pipeline.rs:453` block comment (`"peek-before-writer-open"`) to
  reflect graceful empty handling.
- **Low-2:** Update the stale `run_single_parallel` doc bullet at `pipeline.rs:468`
  (`"Peek-before-writer-open empty-input detection"`).
- **Low-3 (optional, informational):** If a *direct* regression guard for the `refid_tables[i]`
  `+1` indexing is desired (since V10 was correctly judged non-constructible with an empty file1),
  add a separate test with **two non-empty files whose `@SQ` orders are reversed and that share a
  cross-file duplicate position** — that is the only configuration that can distinguish correct vs
  off-by-one. This is a pre-existing `--multiple` concern, not introduced by this change, so it is
  out of scope for this fix; note it for a future `--multiple` hardening pass.
- **Low-4 (tracking):** Confirm the `rust/README.md` Milestones line (PLAN E.13) lands before
  merge (likely deferred to the PR/release step).

---

## Verdict

**APPROVE-WITH-CHANGES** (Critical: 0 · High: 0). The change is correct, minimal, and fully
test-gated; all 8 adversarial checklist items hold and I could not break it. The only requested
changes are two **Low** stale comments (`pipeline.rs:453`, `:468`) that still claim
"peek-before-writer-open" behavior the refactor removed — cosmetic, non-blocking, fix before merge.
The V10 test omission is correctly reasoned and acceptably documented. All quality gates (135 tests,
clippy `-D warnings`, fmt) are green locally.

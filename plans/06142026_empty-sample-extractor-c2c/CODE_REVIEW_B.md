# CODE REVIEW B — graceful no-alignment sample through extractor + coverage2cytosine

**Reviewer:** B (independent) · **Date:** 2026-06-14 · **Branch:** `rust/extractor-empty-outputs` @ `~/Github/Bismark-dedup`
**Scope:** uncommitted working-tree diff (9 files, +639/−63). Plan: `plans/06142026_empty-sample-extractor-c2c/PLAN.md`.

## Summary

The change makes two Rust tools survive a no-alignment (zero methylation call) sample so it flows
through nf-core/methylseq instead of crashing:

- **Extractor:** on `calls_total == 0` *and* `--bedGraph`, the empty-file sweep **force-creates +
  keeps** the per-context `.txt.gz` files (instead of deleting), and the bedGraph/cov writer is no
  longer skipped — `write_outputs_from_sorted([])` emits a valid empty `*.bedGraph.gz` (track line +
  0 rows) + 0-row `*.bismark.cov.gz`. Exit 0.
- **c2c:** on a cleanly-read but empty coverage file, the `EmptyCoverageInput` error is relaxed
  **only on the standard report path** (`threshold == 0 && !nome && !gc_context`) so the
  uncovered-chromosome pass produces a genome-wide all-zero report + summary. All other paths
  (nome / gc / threshold>0) and all genuine read errors still error.

**The change is correctly gated.** I verified that the non-empty path is provably byte-identical
(both new behaviours short-circuit to `false` for any run with ≥1 call), that the c2c relax is
provably unreachable on a genuine read error, that the gc/nome exclusion is logically complete, and
that the rewritten tests gained — not lost — coverage. All gates pass (tests/clippy `-D warnings`/fmt
`--check`, both crates). No Critical or High issues. A handful of Low/Medium notes below; none block.

## Verification performed

- **Gates (all green, sandbox-off):**
  - `cargo test -p bismark-extractor -p bismark-coverage2cytosine` — every binary `ok`, **0 failed**.
    The 15 new/rewritten tests (`empty_input_*`, `empty_coverage_*`, `methylseq_*_empty/no_alignment`)
    all pass; the canary `default_mode_no_cpg_calls_skips` stays green.
  - `cargo clippy -p <each> --all-targets -- -D warnings` — clean, both crates.
  - `cargo fmt -p <each> -- --check` — clean (exit 0), both crates.
- **Single-routing audit:** `finalize_with_empty_sweep` and `run_downstream_chain` each have exactly
  ONE production call site, both inside `ExtractState::finalize`; all three drivers (pipeline.rs:178,
  pipeline.rs:284, parallel.rs:403 = methylseq's `--multicore`) route through that one `finalize`.

## Issues by area

### 1. Byte-identity regression (checklist #1) — NONE FOUND ✅

- `state.rs:162` `is_empty_run = self.report.calls_total == 0` → `false` for any run with ≥1 call.
- `state.rs:183` `force_create_empty = is_empty_run && config.bedgraph` → `false` on every non-empty
  run AND on every run without `--bedGraph` → the output.rs sweep falls to the **original**
  `maybe_writer` catch-all arm (`output.rs:558`), byte-for-byte the prior delete behaviour.
- `downstream_filenames.rs:291` `if !usable && !is_empty_run` → with `is_empty_run == false` this
  reduces to the original `if !usable`. The has-calls path is untouched.
- The new force-create arm (`output.rs:537`) is ordered AFTER `Some(w) if records_written > 0`
  (`output.rs:525`), so even if `force_create_empty` were somehow true on a file with data, the data
  arm still wins. Arm ordering is safe.
- output.rs:1153 (the only other `finalize_with_empty_sweep` caller) is a unit test, updated to pass
  `false`.

### 2. Gate boundary `calls_total` (checklist #2) — CORRECT, minor doc drift (see M-1)

`calls_total` is `Z+z+X+x+H+h` (output.rs:717 doc; bumped per call-character at route.rs:100 /
parallel.rs:963), i.e. **individual methylation calls, not records, not call strings**. This is the
right counter for the bedGraph/cov purpose (bedGraph is call-driven; a zero-call run produces empty
output regardless). A run with records-but-zero-calls (e.g. all-`.` XM strings) → `calls_total == 0`
→ treated as empty; under the OLD code that case was also `!usable` → skipped, so the only
behavioural delta is skip→graceful-empty, consistent with intent. The state.rs:155 comment and code
agree. See M-1 for the stale PLAN prose.

### 3. Force-create resource safety (checklist #3) — SAFE ✅

- The None branch opens via `open_split_writer(&path, gzip)` then `SplitWriter::finish()`
  (output.rs:548). `finish()` is the #889-hardened path: it flushes, calls gzp `get_mut().finish()`,
  and on error `mem::forget`s the writer to **disarm gzp's panicking Drop**. No double-finish (each
  writer is finished exactly once, then moved/forgotten). No panic risk.
- A force-create **failure** does not abort the loop: it is captured into `first_err` and surfaced
  after the loop completes (same fail-open-then-return-Err philosophy as the kept-finish path). No
  leftover temp; `File::create` truncates in place.
- No `expect`/`unwrap` on the empty path except `ParCompressBuilder…expect("GZIP_COMPRESS_THREADS
  is nonzero")` (a compile-time constant invariant, pre-existing).

### 4. c2c relax (checklist #4) — CORRECT & complete ✅

- **`None` arm provably unreachable on a genuine read error.** Verified the read path:
  `open_cov` (cov.rs:22) does `File::open` eagerly → a **missing file** errors before the loop.
  `MultiGzDecoder` errors surface on first `read_until` → a **corrupt `.gz`** errors via `?` in the
  loop. `parse_cov_line` returns `Err(MalformedCovLine)` for a **bad line** → `?` in the loop. All
  three propagate out of `run_single`/`run_split` BEFORE the post-loop `cur_chr.take()` match. The
  `None` arm is reached only when the loop completed cleanly with zero data lines → empty-but-valid.
- **`gc_context` is the correct field** (cli.rs:132; set by `--gc` OR `--nome-seq` at cli.rs:210, so
  `--nome-seq` is doubly excluded via both `!nome` and `!gc_context`).
- **Exclusion is logically complete vs the uncovered-pass gate.** I confirmed `report::run_report`
  (and thus `run_single`/`run_split`) runs **unconditionally**, even on the `--gc` path, BEFORE
  `gpc::run_gpc` (lib.rs:66 then lib.rs:75). So an empty `--gc` run still hits the error at
  `run_single` line 462 (because `gc_context == true` ⟹ `empty_standard_path == false`), exactly as
  `gpc.rs:39` documents it relies on. The uncovered-pass gate (`threshold == 0 && !nome`,
  report.rs:478) is a SUPERSET of `empty_standard_path` (which also excludes `gc_context`), so any
  fall-through reaches the uncovered pass — consistent.
- **`run_split`'s `Option<PathBuf>` handles every branch:** empty-standard ⟹ `None` then the
  uncovered pass overwrites it to `Some` for a ≥1-chromosome genome (the realistic case); non-empty
  ⟹ `Some(flush…)`; post-loop summary write guarded by `if let Some` (report.rs:575). The only
  `None`-survives case is the degenerate empty-cov + zero-chromosome genome (no genome at all) →
  summary skipped. See L-1.

### 5. Test coverage (checklist #5) — STRENGTHENED, not lost ✅

- The old `empty_input_skips_downstream_exit_zero` (asserted skip + no-files) is **rewritten** in
  BOTH `phase2_inline.rs` (→ `empty_input_emits_graceful_outputs_exit_zero`) and
  `phase3a_streaming.rs` (→ `empty_input_emits_graceful_outputs_with_cytosine_report`), now asserting
  the graceful outputs incl. 0-data-row bedGraph/cov and (phase3a) the inline c2c all-zero CpG report.
- **Legitimate-skip case still covered:** `default_mode_no_cpg_calls_skips` (phase2_inline.rs:975)
  uses a CHG/CHH-only BAM (`calls_total > 0`, `is_empty_run == false`) and asserts the warn+skip+no
  bedGraph path — the exact V3b boundary proving the gate is `calls_total == 0`, not `!usable`.
  Plus a new `empty_input_no_bedgraph_keeps_perl_faithful_delete` proves the `config.bedgraph` gate.
- **c2c error-regression tests genuinely assert `Err`:** `empty_coverage_missing_file_errors` and
  `empty_coverage_corrupt_gzip_with_gz_name_errors` assert `.failure()`;
  `empty_coverage_{threshold,nome,gc}_still_errors` assert `.failure().code(1).stderr("no data
  found")`. All confirmed passing.
- **Binary-level methylseq conformance:** `methylseq_extractor_no_alignment_runtime_emits_required_outputs`
  drives the EXACT methylseq command (`--bedGraph --counts --gzip --report -s --CX`) on a header-only
  BAM and asserts all 5 required globs; `methylseq_coverage2cytosine_empty_runtime_emits_required_outputs`
  drives the c2c shape on an empty `.cov.gz` and asserts `*report.txt.gz` + summary. These are the
  real end-to-end guards.

### 6. Consistency / downstream readers (checklist #6) — OK ✅

- On the empty path the bedGraph step reads the in-memory `sorted` slice (empty), **not** the
  per-context `.txt.gz` files — confirmed: `run_downstream_chain(…, sorted: &[…], …)` operates on
  `sorted`, and state.rs builds `sorted` independently. So a header-less empty `.txt.gz` cannot
  confuse the bedGraph step. methylseq only needs the `*.txt.gz` glob to MATCH (existence), which it
  does. See L-2 on the header asymmetry.

## Recommendations

### Critical — none.

### High — none.

### Medium

- **M-1 (doc drift, PLAN only — not code):** the PLAN §Behavior prose still says "empty = zero
  methylation call **strings** processed (`Total number of methylation call strings processed: 0`)",
  which is `call_strings_processed`. The implementation (correctly) uses `calls_total` (Z+z+X+x+H+h),
  and the rev-1 notes + `state.rs:155` comment agree with the code. The two counters differ for a
  records-but-zero-calls run. The CODE is right (and arguably safer); only the PLAN §Behavior text is
  stale. Recommend a one-line PLAN note so a future reader doesn't "fix" the gate to the wrong
  counter. (`PLAN.md` §Behavior vs `state.rs:162`.)

### Low

- **L-1 (degenerate-input asymmetry, c2c):** `run_single` writes the context summary
  **unconditionally** (`report.rs:494`), so an empty-cov + zero-chromosome genome on the non-split
  path emits an empty summary file; `run_split` SKIPS the summary in that same degenerate case
  (`report.rs:575` `if let Some`). Neither is on methylseq's path (a real genome always has ≥1
  chromosome, and `Genome::load` would fail on an empty index first). Cosmetic; optionally document
  the divergence or align the two. (`report.rs:494` vs `report.rs:575`.)
- **L-2 (force-created empties have no banner):** `open_split_writer` does not write the per-context
  Bismark version banner (that is lazy on first `write_call`), so a force-created empty `.txt.gz` is
  a *truly* empty stream — unlike a header-only file that an opened-but-swept strand would have had.
  Harmless for methylseq (glob existence only) and for the bedGraph step (reads `sorted`), but if any
  future MultiQC/parser expects the banner on an empty calls file it would see none. Surfaced to
  V-E2E in the plan; flag here for completeness. (`output.rs:548` / `output.rs:678`.)
- **L-3 (kept-path canonicalization asymmetry):** a force-created file is recorded in `kept` using
  the pre-create `abs_path` (non-symlink-resolved stored path, since `canonicalize` ran before the
  file existed), whereas the data-kept arm records the canonicalized path. The stored path is already
  absolute, so glob/argv use is fine; purely a path-shape nit. (`output.rs:513` + `output.rs:556`.)
- **L-4 (corrupt-gzip test is `.failure()`-only):** `empty_coverage_corrupt_gzip_with_gz_name_errors`
  asserts only `.failure()` (no `.code`/stderr), so in principle it could pass on an unrelated
  failure. Adequate as a regression guard, but a stderr assertion (e.g. a decode-error substring)
  would make it airtight. (`golden_phase_b.rs`, `empty_coverage_corrupt_gzip_with_gz_name_errors`.)

## Notes

- Inline deviation comments at all four production sites correctly cite the plan slug and explain the
  divergence + the rationale for the gating (`&& !is_empty_run`, `&& !config.gc_context`). Good
  documentation hygiene.
- The plan's remaining "outstanding" items (V6b static scout of report/summary/MultiQC; V-E2E real
  methylseq run) are out of scope for this code review and remain the correct gates before "done".

---

**Report:** `/Users/fkrueger/Github/Bismark-dedup/plans/06142026_empty-sample-extractor-c2c/CODE_REVIEW_B.md`

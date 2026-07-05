# CODE REVIEW A — graceful no-alignment sample through extractor + c2c

**Reviewer:** A (independent) · **Date:** 2026-06-14 · **Branch:** `rust/extractor-empty-outputs` @ `~/Github/Bismark-dedup`
**Scope:** the two-crate "empty-sample" change (extractor `state.rs`/`output.rs`/`downstream_filenames.rs` + c2c `report.rs` + tests)
**Verdict:** **APPROVE** (0 Critical, 0 High)

---

## Summary

The change makes a zero-methylation-call sample flow cleanly through nf-core/methylseq instead of crashing two
post-dedup modules. It is a **deliberate, correctly-gated divergence from Perl** — every new behavior fires only on
the empty condition, and I verified the non-empty paths are byte-unchanged. All gates are green:

- `cargo test -p bismark-extractor -p bismark-coverage2cytosine`: **all binaries 0 failed** (c2c lib 98, golden_phase_b 17, c2c methylseq_conformance 2; extractor lib 110, phase2_inline 15, phase3a_streaming 17, extractor methylseq_conformance 3, + all others).
- `cargo clippy ... --all-targets -- -D warnings`: clean, both crates.
- `cargo fmt -- --check`: clean, both crates.

The implementation matches the rev-1 plan faithfully, including both deviations the plan documents (the
`&& !config.gc_context` exclusion and the second inverted test in `phase3a_streaming.rs`). I found no correctness
bugs. Two Low observations below (one trigger-semantics nuance, one contrived degenerate `run_split` case), neither
blocking.

---

## Issues by area

### Logic — all six scrutiny points verified PASS

1. **Non-empty byte-identity (the #1 risk) — PASS.** Every change is gated so a run with ≥1 call / ≥1 coverage line
   is byte-unchanged:
   - `state.rs:179` `force_create_empty = is_empty_run && config.bedgraph` — false unless `calls_total==0`.
   - `output.rs:537` the new force arm is `maybe_writer if force_create_empty` — the non-force arm (`maybe_writer =>`,
     :558) is **textually unchanged** from the prior sweep/delete code; a normal run never enters the new arm.
   - `downstream_filenames.rs:291` the skip guard only *adds* `&& !is_empty_run` — for a non-empty run `is_empty_run`
     is false, so the condition reduces to the original `if !usable`.
   - `report.rs:462/554` the c2c `None` arm is reachable only when the read loop drained cleanly to zero data lines
     (`cur_chr == None`). A genuine read failure propagates earlier via `?` on `read_until`/`parse_cov_line`
     (verified in `run_single`:427/431 and `run_split`:518/522), so it never reaches `None`.

2. **The has-calls-but-no-CpG boundary — PASS.** A default-`--bedGraph` (no `--CX`) run with only CHG/CHH calls has
   `calls_total>0` ⟹ `is_empty_run=false`, and `usable=false`. The guard `if !usable && !is_empty_run`
   (`downstream_filenames.rs:291`) is therefore `true` ⟹ it still skips. The canary
   `default_mode_no_cpg_calls_skips` (`phase2_inline.rs:975`) is **intact, green, and non-vacuous**: it uses
   `write_non_cpg_only_bam` (XM `x.....h` = CHG+CHH, `calls_total=2`), default `--bedGraph`, asserts the skip warning
   **and** that no `*.bedGraph.gz`/`*.bismark.cov.gz` appear. The new guard `empty_input_no_bedgraph_keeps_perl_faithful_delete`
   (`phase2_inline.rs:777`) additionally proves the `&& config.bedgraph` half of the gate.

3. **Force-create correctness — PASS.** `open_split_writer(&path, gzip).finish()` (`output.rs:548`) is sound for both
   never-opened (lazy) and opened-but-empty entries. `open_split_writer` (`output.rs:678`) does `File::create`
   (creates/truncates) then returns a `Plain` or `Gzip` `SplitWriter`; `.finish()` (`output.rs:88`) flushes the
   `BufWriter` and, for gzip, writes the footer + joins workers — yielding a **valid empty** plain or gzip stream. No
   double-finish: the `Some(w)` branch finishes the already-open writer; the `None` branch opens a fresh one and
   finishes it exactly once. `gzip = self.gzip` (captured at :494 before the drain loop) is the correct flag — it is
   the same field `open_split_writer` is called with throughout the normal write path, so the empty file's
   compression matches what a non-empty run of the same invocation would produce. Path/canonicalization: `abs_path`
   (`output.rs:513`) falls back to the stored (already-absolute) path when a never-created file can't be canonicalized
   — unchanged from the prior sweep code and correct here.

4. **c2c empty-vs-error distinction — PASS.** Reaching the `None` arm is guaranteed to mean "cleanly read, zero data
   lines": every read error short-circuits via `?` before the post-loop `match`. The relax cannot mask corruption —
   confirmed by the regression tests (`empty_coverage_missing_file_errors`, `empty_coverage_corrupt_gzip_with_gz_name_errors`),
   which I verified stay `.failure()`. The `gc_context`/`nome`/`threshold` exclusion is **correct and necessary**:
   `lib.rs:54-76` calls `report::run_report` *unconditionally first*, then `gpc::run_gpc` only if `gc_context`. So if
   `--gc` did not set `empty_standard_path=false`, an empty `--gc` run would emit an all-zero standard report and then
   fall into `run_gpc` (which relies on the guard). The `&& !config.gc_context` term keeps `--gc` on the error arm
   (test `empty_coverage_gc_still_errors`). `--nome-seq` and `threshold>0` likewise stay guarded (tests
   `empty_coverage_nome_still_errors`, `empty_coverage_threshold_still_errors`). All three assert exit code 1 +
   `"no data found"`.

5. **`run_split` `Option<PathBuf>` restructure — PASS (with one Low note).**
   - Empty-standard: `cur_chr=None`, `empty_standard_path=true` ⟹ `None` arm sets `last_summary_path=None`; the
     uncovered pass (`report.rs:562`, gate `threshold==0 && !nome`) then flushes every genome chr and sets the path to
     `Some(..)`, so the summary is written. ✓ (On a real genome the summary always exists.)
   - Non-empty: `Some(prev)` ⟹ flushed, `Some(path)` set; uncovered pass may overwrite to a later chr — unchanged. ✓
   - Degenerate zero-chromosome genome (see Low-1) ⟹ `last_summary_path` stays `None` and the summary is skipped
     (`report.rs:575`). Off methylseq's path (it uses non-split `run_single`, where the summary at `report.rs:493-496`
     is written **unconditionally**). Documented in the code comment. See Low-1.

6. **Tests — PASS, non-vacuous.** The rewritten tests assert the NEW behavior with real content checks (not just
   exit-0): bedGraph 0 data rows (header `track` line excluded), cov 0 rows, ≥1 retained `*.txt.gz`, splitting +
   M-bias present (`phase2_inline.rs`); inline c2c all-zero CpG report rows all `\t0\t0\t` (`phase3a_streaming.rs`);
   c2c all-zero report row-count == genome cytosine count `2` with exact byte rows `chrA\t2\t+\t0\t0\tCG` /
   `chrA\t3\t-\t0\t0\tCG`, both plain and gzipped (`golden_phase_b.rs`). The four error-regression tests stay RED
   (`.failure()`). The `--multicore 2` variant proves the `parallel.rs`→`finalize` path. Old test names survive only
   in "REWRITTEN (was …)" comments — clean renames, no stale active assertions.

### Efficiency — no concerns
The empty path is O(1) extra work (open+finish empty streams); the c2c empty path still does the normal O(genome)
walk. No hot-path impact on normal runs (the new arms are gated and unreachable for non-empty input).

### Errors — handled correctly
Force-create finish errors are collected into `first_err` and surfaced after the loop (consistent with the existing
#889-item-2 kept-file error handling). c2c read errors propagate via `?` before the relax. The pre-existing
`finalize_surfaces_kept_finish_error_via_result` test was updated to pass `false` and still asserts the kept-finish
error surfaces as `Err`.

### Structure — clean
The `force_create_empty` param threads cleanly `state.rs` → `output.rs`; the c2c gate is a single named local
`empty_standard_path` reused identically in both `run_single` and `run_split`. Deviation comments at every site cite
the plan slug. The two `run_single`/`run_split` gates are kept consistent (good — reduces drift risk).

---

## Fixes applied
None (review-only; no low-risk fixes warranted — code is clean).

---

## Recommendations

### Critical
None.

### High
None.

### Medium
None.

### Low
- **Low-1 — `run_split` degenerate zero-chromosome genome skips the summary silently.**
  `report.rs:553-579`. A genome folder containing **only** a `Mus_musculus.*` file yields an empty-but-`Ok` genome
  (`genome.rs:66-82`, `MUS_SKIP` skipped inside the loop; test `mus_only_tier_yields_empty_genome_no_error`). On an
  empty cov + standard path + `--split_by_chromosome`, `last_summary_path` stays `None` ⟹ **no
  `*cytosine_context_summary.txt` is written**, yet the run exits 0. This is contrived (requires a Mus-only genome
  *and* `--split_by_chromosome` *and* empty cov), is **off methylseq's path** (methylseq uses non-split `run_single`,
  whose summary write at `report.rs:493-496` is unconditional), and the prior code would have *errored* here anyway —
  so it is not a regression of a previously-working path. The comment at `report.rs:546-551` documents it. Optional
  hardening only: if ever a concern, write the summary unconditionally in `run_split` too (it currently keys off the
  last chr path purely to mirror Perl's per-chr summary quirk). **No action required for this PR.**

- **Low-2 — trigger semantics: `calls_total==0` vs "call strings processed".**
  `state.rs:162` uses `is_empty_run = self.report.calls_total == 0` (the `Z+z+X+x+H+h` per-call count, incremented in
  `route.rs:100`), whereas parts of the plan prose reference the "Total number of methylation call strings processed"
  counter (`call_strings_processed`). These diverge in one edge case: a sample with aligned reads whose XM strings are
  *all dots* (zero cytosines in any read) has `call_strings_processed>0` but `calls_total==0` ⟹ `is_empty_run=true`.
  That run previously skipped+deleted (`usable=false`); it now force-creates empty outputs. This is **arguably more
  correct** (it is genuinely a zero-methylation-data sample, and emitting empty outputs is the desired methylseq
  behavior) and it does **not** affect any run with ≥1 call, so byte-identity is preserved. Flagging only so the
  trigger's exact meaning is on record. **No action required.**

---

## Verdict

**APPROVE** — **0 Critical, 0 High.** All six scrutiny areas verified against source; all gates green
(tests/clippy `-D warnings`/fmt, both crates). Non-empty byte-identity is preserved by construction (every new arm is
gated on the empty condition and the original code paths are textually unchanged), the has-calls-no-CpG canary holds,
force-create produces valid empty gzip for lazy entries, and the c2c relax cannot mask a genuine read error. The two
Low notes are contrived/off-path and need no change for this PR.

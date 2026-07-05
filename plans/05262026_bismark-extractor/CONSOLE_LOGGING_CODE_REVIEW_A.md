# Code Review A — extractor console progress/diagnostics logging (commit `52b05f8`)

**Reviewer:** A (fresh context)
**Branch:** `feat-extractor-console-logging`
**Commit:** `52b05f80781852f358fc591d582d8ec88879d1e5`
**Plan:** `plans/05262026_bismark-extractor/CONSOLE_LOGGING_PLAN.md`
**Verdict:** APPROVE with one fix applied (PE counter mismatch) + minor recommendations.

---

## Summary

The change is well-scoped, faithful to the (rev-1) plan, and the documented
deviations are sound. New `src/logging.rs` introduces a `Copy` `Logger { quiet,
verbose }` with pure, testable text builders; the single live read site
(`parallel.rs::producer_loop`) ticks `Processed lines: N` every 500k counting
+1/SE record and +2/PE pair; banner/params/provenance fire exactly once on the
main thread before the reader is moved into the producer. The `--quiet` gate is
applied to every informational line and the genuine warnings/errors are left
ungated. Nothing in the diff touches output **file** content, so the beta.1
byte-identity gate is unaffected.

I found and **fixed one correctness bug**: the console final-summary
`"Processed N lines in total"` line used `call_strings_processed` instead of
`records_processed`, so for PE inputs the console showed 2×pairs where the
`_splitting_report.txt` file (and Perl) show pairs. Details below.

`cargo test -p bismark-extractor` (incl. the 3 new logging tests) and
`cargo clippy --all-targets -D warnings` are green after the fix.

---

## Issues by area

### 1. Logic / correctness

**[FIXED — High] PE `"Processed N lines in total"` used the wrong counter.**
`final_summary_text` (`logging.rs:203-217`) used `report.call_strings_processed`
for *both* the `"Processed N lines in total"` line and the `"...call strings
processed: N"` line. But the actual `_splitting_report.txt` writer
(`output.rs:683`) and Perl `:2482` (`$sequences_count`) use `records_processed`
for the first line:
- `output.rs:683` → `Processed {records_processed} lines in total`
- `output.rs:690` → `Total number of methylation call strings processed:
  {call_strings_processed}`

For SE these counters are equal (no visible effect), but for PE
`records_processed` = pairs while `call_strings_processed` = 2×pairs
(see the `SplittingReport` doc comments at `output.rs:403-420`). So on any PE
run the console summary's first line would have shown **double** the file's
value — a direct contradiction of the plan's "mirror of splitting_report"
requirement and of Perl's stderr `warn` output. Fixed by binding `lines =
records_processed` and adding a separate `call_strings` binding; the existing
unit test was strengthened to give the two counters distinct values so the
distinction is locked.

**Progress counter — correct.** `producer_loop`'s `tick` (`parallel.rs:337-342`)
does `*lines_read += 1; if is_multiple_of(500_000) { progress }`. It is called:
- SE: once per yielded record (`:353`) — +1/record. ✓
- PE: once for R1 (`:409`) and once for R2 (`:424`) — +2/pair, and crucially
  *before* pairing/refid resolution, so both mates count even on the final
  malformed-pair / EOF-after-R1 paths. ✓
This matches Perl's read-side `$line_count` (+1 per SAM line). The final partial
batch (< 500k since last tick) is intentionally not emitted — acceptable and
matches Perl, which only prints at 500k boundaries; the grand total is shown by
the final summary. No mis-count: `lines_read` is local to the single producer
thread, incremented exactly once per consumed record, independent of the
`input_idx` pair index.

**Banner/params/provenance emission ordering — correct and once.**
`run_pipeline` (`parallel.rs:189-196`) builds the logger and emits banner →
parameters → header_provenance on the main thread *after* `build_chr_name_table`
and *before* the reader is moved into the producer closure (`:234`). Header +
`is_paired` are both in hand. Emitted exactly once per run (workers/producer
never call these). ✓ The final summary is correctly emitted later in
`state.finalize` after `write_splitting_report` (`state.rs:158`), reusing the
finalized `SplittingReport` (no recomputation). ✓

### 2. Quiet-gate completeness

`grep -rn 'eprintln!' src/` shows the only *direct* `eprintln!` call sites are:
- `output.rs:321` — `failed to remove empty output file` → **ungated** (genuine
  warning). ✓
- `output.rs:373` — `failed to remove partial output file` → **ungated**
  (genuine warning, `cleanup_all`). ✓
- `subprocess.rs:415` — `[bismark-extractor] spawning:` → gated via
  `if !self.quiet` on `RealRunner.quiet`. ✓ (`state.rs:188` wires
  `RealRunner { quiet: config.quiet }`.)
- `subprocess.rs:524` — `note: extractor produced no methylation calls; ...` →
  **ungated**. This is a UX advisory, not in the plan's explicit gate list. It
  is arguably "informational," but it is a genuine heads-up about a degenerate
  run and only fires in the rare empty-kept + cytosine_report case. Leaving it
  ungated is defensible; see Recommendation L1.
- `main.rs:44` — `error:` → **ungated**. ✓

All INFO lines (banner, parameters, header_provenance, progress, final_summary,
kept/deleted, trailing blanks) route through `Logger::info`/`note`, which
short-circuits on `quiet`. ✓ The quiet gate is complete per plan.

### 3. Byte-identity safety

The diff touches only stderr paths:
- `logging.rs` writes solely to `std::io::stderr()` (or to a test `Vec<u8>` via
  `info_to`).
- `output.rs` change converts the existing `kept`/`deleted` *stderr* lines from
  raw `eprintln!` to `logger.note(...)` — same channel (stderr), no output-file
  bytes touched. The `remove_file` behaviour is unchanged.
- `state.rs` / `subprocess.rs` / `cli.rs` changes thread the `quiet`/`verbose`
  flags; no writes to data/report files were altered.
`write_splitting_report` / `write_mbias_txt` / `write_call` are untouched. The
beta.1 byte-identity gate (which `cmp`s files / sorted-md5s data) is unaffected.
✓ (Recommend the planned `phase_h_smoke.sh` SE+PE+AutoDetect run as final
confirmation — still listed as Pending in the plan.)

### 4. Errors / panics

- `header_provenance_lines` (`logging.rs:120-129`): on header serialize failure
  it returns `Vec::new()`; `header_provenance` then early-returns and prints
  nothing. Acceptable — provenance is purely informational and a serialize
  failure on a header that noodles already parsed is near-impossible. No panic.
  ✓ (Minor: the failure is silently swallowed; see L2.)
- `final_summary_text` percent (`logging.rs:199-201`): uses
  `SplittingReport::percent_meth`, which returns `0.0` on a zero denominator
  (`output.rs:441-448`) — no NaN, no divide-by-zero, no panic on empty input. ✓
- Empty input / 0 reads: progress prints nothing (no 500k boundary hit); final
  summary prints all-zero counts and `0.0%`. ✓
- No `unwrap`/`expect`/indexing introduced on the hot path.

### 5. Structure / tests

The 3 `logging.rs` unit tests are meaningful and target the load-bearing
behaviours:
1. `quiet_gate_suppresses_info_but_returns_false` — proves the gate via the
   `info_to` seam without touching real stderr. ✓
2. `final_summary_matches_perl_shape_and_percent` — shape + percent rounding;
   **strengthened during this review** to use distinct
   `records_processed`/`call_strings_processed` values, locking the
   counter-selection fix. ✓
3. `provenance_drops_sq_by_default_keeps_hd_pg` — `@SQ` drop, `@HD`/`@PG`
   keep + order, verbose-includes-`@SQ`. ✓

**Test gap (Low):** there is no test that the *producer* actually emits a
progress tick at a 500k boundary, nor that `header_provenance_lines` round-trips
a real noodles `Header` through the serialize path (the provenance test exercises
only the pure `filter_header_text` on a literal string, not the
`Writer::write_header` round-trip). Both are acceptable: the producer `tick` is a
trivial wrapper over the unit-tested `is_multiple_of` arithmetic, and the
serialize idiom is already proven in `bismark-io::detect_paired_from_header`.
See R-Medium for a cheap closure of the `header_provenance_lines` gap.

---

## Fixes applied

1. **`logging.rs::final_summary_text`** — `"Processed N lines in total"` now
   binds `lines = report.records_processed` (was `call_strings_processed`); added
   a separate `call_strings = report.call_strings_processed` binding for the
   call-strings line. Added an explanatory comment citing `output.rs:683` / Perl
   `:2482`. This makes the console summary byte-match the `_splitting_report.txt`
   first line for PE (SE was already correct).
2. **`logging.rs` test `final_summary_matches_perl_shape_and_percent`** — gave
   `records_processed` (4_250_754) and `call_strings_processed` (8_501_508)
   distinct values and asserted the first line shows the former; locks the
   distinction so a future regression is caught.

Post-fix: 3/3 logging tests pass; full `cargo test -p bismark-extractor` green;
`cargo clippy -p bismark-extractor --all-targets -- -D warnings` clean.

---

## Recommendations

### Medium
- **R-M1 (test):** Add one unit test that calls `header_provenance_lines` on a
  real `noodles_sam::Header` (built with `@HD` + an `@SQ` + an `@PG` with a `CL:`
  containing spaces/quotes) and asserts the `@PG CL:` text is preserved verbatim
  and `@SQ` is dropped by default. This is the only path that actually exercises
  `Writer::write_header`; the existing test stops at the pure string filter.
  Closes the round-trip half of the plan's "header_provenance_lines test"
  acceptance item.

### Low
- **R-L1 (consistency):** Consider routing the `subprocess.rs:524`
  `"note: extractor produced no methylation calls..."` advisory through the
  `Logger` quiet-gate (it is informational, like the `spawning:` line that *was*
  gated). Counter-argument: it warns about a likely-misconfigured run, so
  leaving it on under `--quiet` is also defensible. Either way, document the
  decision in the plan's gate audit (currently it classifies this line as a
  warning to never gate — that classification is fine, just make it deliberate).
- **R-L2 (observability):** `header_provenance_lines` silently returns `[]` on a
  `write_header` error. A one-line `Logger::note` ("could not serialise header
  for provenance") on that path would aid debugging without affecting
  byte-identity. Very low priority — the failure is near-impossible post-parse.
- **R-L3 (doc nit):** `main.rs` module doc (`:10-15`) still says "Phase E (this
  build) ... `--parallel > 1` (Phase F) ... are still rejected", which is stale
  (Phase F/this build supports `--parallel N`). Not introduced by this commit,
  but adjacent; worth a sweep.

---

## Sign-off

The change is correct (after the applied PE-counter fix), the quiet gate is
complete, byte-identity is untouched, and there are no panic/error-handling
hazards. Remaining items are test-coverage and consistency polish, none
blocking.

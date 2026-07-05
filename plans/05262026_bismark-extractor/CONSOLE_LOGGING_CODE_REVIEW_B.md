# Console Logging — Code Review B

**Commit:** `52b05f8` ("feat(extractor): console progress/diagnostics logging (stderr, --quiet)")
**Branch:** `feat-extractor-console-logging`
**Reviewer:** B (fresh context)
**Plan:** `plans/05262026_bismark-extractor/CONSOLE_LOGGING_PLAN.md`

## Summary

The change adds stderr diagnostics to `bismark-methylation-extractor-rs`: a startup banner,
SE/PE mode + parameter summary, `@HD`/`@PG` header provenance (`--verbose` adds `@SQ`), a
`Processed lines: N` counter every 500k, and a final per-context methylation summary. A new
`Logger { quiet, verbose }` with pure, unit-tested text builders lives in `src/logging.rs`.
Default-on; `-q`/`--quiet` gates the info channel while genuine warnings/errors stay un-gated.

**Verdict: solid, ship-able, with one real correctness issue worth a decision before release.**

- Tests: `cargo test -p bismark-extractor --offline` → **all green** (101 lib + 26/32/38/50/… integration; 0 failed).
- Lints: `cargo clippy -p bismark-extractor --offline --all-targets -- -D warnings` → **clean**.
- MSRV: `is_multiple_of` (stabilized 1.87) ≤ workspace `rust-version = "1.89"` → **fine**.
- Threading/interleaving: **no risk** (traced below).
- The one notable issue is a PE-only semantic mismatch between the console "Processed N lines
  in total" line and the byte-identical `_splitting_report.txt`.

---

## Issues by focus area

### 1. Threading / ordering on stderr — NO RISK (verified)
Traced `run_pipeline` (`parallel.rs:186-315`):
- Banner / parameters / header_provenance are emitted on the **main thread at lines 194-196**,
  *before* the producer is spawned (line 231). They cannot interleave with progress.
- `Processed lines: N` is emitted only from the **producer thread** via `tick()` (`parallel.rs:336-345`).
- kept/deleted lines + final methylation summary are emitted on the **main thread inside
  `state.finalize`** (`state.rs`), which runs only on the `Ok(())` arm at line 313 — i.e. **after**
  `producer_handle.join()` at line 259. So the producer is guaranteed joined before the final
  summary prints. **No producer-vs-finalize interleave.**
- Each line is written as one fully-built `String` via a single `write_all` under
  `std::io::stderr().lock()` (`Logger::info`). The banner is itself one multi-line `write_all`.
  Individual lines/blocks are therefore atomic; a progress line cannot land mid-banner.

### 2. `Processed lines` counter semantics vs Perl — CORRECT for the live counter
- `bismark_io::AnyReader::records()` (`bismark-io/src/read.rs:527`) dispatches to per-format
  `records()`, each of which yields **one `BismarkRecord` per mapped alignment** (e.g. BamReader at
  `read.rs:257`: `record_bufs().filter_map(filter_unmapped_then_classify)`). It does **NOT**
  pre-pair mates.
- The PE producer loop calls `records_iter.next()` **twice per pair** (R1 at `parallel.rs:407`,
  R2 at `:422`), ticking once each → **+2 per pair**, +1 per SE record. This matches Perl's
  read-side `$line_count` (incremented once per non-header SAM line at `:1551`/`:1849`, warned every
  500k at `:1553`/`:1850`). **The progress counter matches Perl.**
- **Minor caveat (Low):** `filter_unmapped_then_classify` drops unmapped records (FLAG&0x4), so the
  Rust counter counts *mapped records that pass classification*, whereas Perl counts *every
  non-header line `samtools view` emits*. For standard Bismark BAMs (no unmapped records written)
  these agree; on a BAM that happens to carry unmapped records, the Rust 500k ticks would drift from
  Perl's. Cosmetic only (progress, not an output file). Note, don't fix.

### 3. `--quiet` + `--verbose` precedence — CORRECT (quiet wins)
`Logger::header_provenance` builds `header_provenance_lines(header, self.verbose)` unconditionally
but then routes the assembled string through `self.info(&s)`, which short-circuits on
`self.quiet` (`logging.rs:88-95`). `self.verbose` is only ever consulted to decide `@SQ`
inclusion, never to bypass the quiet gate. Under `--quiet --verbose`, nothing prints. Confirmed.

### 4. `Logger::final_summary` correctness — ONE REAL MISMATCH (PE only)
`final_summary_text` (`logging.rs:198-227`) pulls from the **same `SplittingReport`** that drives
`write_splitting_report` (`output.rs:578`), and the percent format matches exactly
(`{:.1}` in both — console `logging.rs:214-216`; file `output.rs:534-541`). The meth/unmeth/total
counts agree.

**However, the "Processed N lines in total" line diverges from the file for PE input:**
- File (`output.rs:683`): `Processed {report.records_processed} lines in total` →
  `records_processed` = **pair count** for PE (per the field doc, `output.rs:403-413`).
- Console (`logging.rs:203-204`): both `Processed {lines}` AND `methylation call strings
  processed: {lines}` use `lines = report.call_strings_processed` → **2×pairs** for PE
  (`output.rs:414-420`).

So for PE the console's first line is **double** the file's first line. The two outputs are
internally inconsistent.

Cross-checking Perl resolves which is "right": Perl sets `$counting{sequences_count} = $line_count`
(`:2459`) and prints `sequences_count` for **both** the `warn` (`:2479`) and the REPORT (`:2482`)
"Processed N lines in total" line — i.e. Perl prints the **same** value (the raw line count =
2×pairs for PE) in both places. So:
- The new **console** line matches **Perl** (`call_strings_processed` == `$line_count` for the
  intended cases), but
- The **Rust file** already diverges from Perl here (it uses `records_processed` = pairs), a
  pre-existing Phase C.2 (#864) decision that is **out of scope** for this commit.

Net: the console line is arguably the more Perl-faithful of the two, but it now visibly contradicts
the project's own splitting-report file on PE data, which will confuse users who diff the two. This
needs a product decision (see High recommendation R1). I did **not** auto-fix because either
direction (match the file vs. match Perl) is a judgment call that also touches the long-standing
file behavior.

### 5. Test constructors / RealRunner / flakiness — OK
- All `ResolvedConfig` literal constructors that needed the two new fields were updated
  (`output.rs:962-963`, `subprocess.rs:607-608`, `cli.rs:520-521`, `phase_g.rs:155-156`). The other
  test helpers (`parallel_phase_f.rs`, `mbias_writer_phase_d.rs`, `se_phase_b.rs`) build via
  `Cli::…resolve()` or `..spread`, so they needed no change. Confirmed by the green compile/test run.
- `RealRunner` gained a `quiet: bool` field; all 9 `phase_g_realrunner.rs` constructions were updated
  to `RealRunner { quiet: false }`. Green.
- Flakiness: the only process-stderr-asserting integration test
  (`output_phase_c2.rs:84` `empty_file_sweep_emits_perl_format_log_lines_on_stderr`) uses
  `.contains()` on the kept/deleted lines, which now coexist (default-on) with banner/progress
  output. `.contains()` is interleave-tolerant → not flaky; it passed. The `logging.rs` unit tests
  write to in-memory `Vec<u8>` buffers via the `info_to` seam, never real stderr → not flaky.

### 6. MSRV — OK
`u64::is_multiple_of` stabilized in Rust 1.87; workspace `rust-version = "1.89"` (`rust/Cargo.toml:7`).
No MSRV regression.

---

## Other observations (Low)
- **Dead getter:** `Logger::verbose()` (`logging.rs:46-49`) is never called anywhere in `src/`
  (`header_provenance` reads `self.verbose` directly). It's `pub`, so clippy stays silent, but it's
  unused surface. Recommend removing or `#[cfg(test)]`-gating.
- **`info_to` is test-only:** the `pub fn info_to` (`logging.rs:62-69`) exists solely as the unit-test
  seam; only the tests call it. Documented as such in the doc comment — acceptable, but worth a
  `// test seam` note or `#[doc(hidden)]` if you want to keep the public API tight.
- **Final-summary ordering vs Perl:** console summary prints *after* the kept/deleted sweep and
  *after* the file is written (`state.rs` finalize order: sweep → write_splitting_report →
  final_summary). Perl warns the summary before writing the report. Purely cosmetic (stderr only,
  outside byte-identity contract).

---

## Fixes applied
None. The single substantive issue (R1) is a behavior/judgment decision, not an unambiguous
low-risk fix; per the dual-review protocol I left it for the user. Everything else is Low-severity
nits I'm flagging rather than touching, to keep this review's footprint zero.

---

## Recommendations (by priority)

**Critical:** none.

**High**
- **R1 — Resolve the PE "Processed N lines in total" mismatch.** Console uses
  `call_strings_processed` (2×pairs); the `_splitting_report.txt` uses `records_processed`
  (pairs). For PE these differ by 2×, so console and file contradict each other. Decide one of:
  (a) make the console use `records_processed` for the "Processed N lines" line (match the file;
  keep `call_strings_processed` only for the "methylation call strings processed" line — exactly
  mirroring the file's two-line structure at `output.rs:683`/`:689`), or (b) accept the divergence
  and document it. Option (a) is the safer, less surprising choice and makes the console a faithful
  echo of the file. Either way, add a PE regression test asserting console line 1 == file line 1.

**Medium:** none.

**Low**
- **R2 —** Remove or `#[cfg(test)]`-gate the unused `Logger::verbose()` getter (`logging.rs:46`).
- **R3 —** Mark `info_to` as a test-only seam (`#[doc(hidden)]`) or move it under `#[cfg(test)]`
  helpers if you don't want it in the public API.
- **R4 —** (Doc-only) Note in `Logger::progress`'s call site that the tick counts *mapped*
  records, so it can drift from Perl's raw-line count on BAMs containing unmapped records.

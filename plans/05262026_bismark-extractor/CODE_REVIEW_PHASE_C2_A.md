# Code Review — Phase C.2 (Reviewer A)

**Branch:** `extractor-phase-c2` (off `rust/iron-chancellor` HEAD `84c6ad1`)
**Scope:** closes #864 (splitting-report format) + #865 (empty-file sweep); #863 dropped as won't-fix.
**Reviewer:** A (independent / fresh context)
**Date:** 2026-05-27

## Executive summary

Phase C.2 lands a high-fidelity Perl-byte-identity rewrite of `write_splitting_report`, a new `write_percent_or_fallback` helper that correctly varies the trailing-newline count by context position, an `OutputFileMap` refactor that wires a per-handle `records_written` counter into a new `finalize_with_empty_sweep`, and harness updates for `--multicore N` sorted-equivalence acceptance. The byte-level walk against Perl lines 4995-5047 + 2482-2556 matches step-for-step; `is_paired` routing, `records_processed += 1` per PE pair, and `call_strings_processed += 2` are correct at all four call sites; `records_written` is bumped only after a successful write; clippy/fmt/236 tests are clean. Two minor deviations from the plan: (1) no test covers Plan V1 (`--mbias_only` sweep no-op); the sweep currently emits two trailing blank stderr lines even on an empty map, which diverges from Perl (Perl skips `delete_unused_files` under `--mbias_only`); (2) Rust does not emit Perl's leading `"Deleting unused files ...\n\n"` stderr header. Neither affects on-disk byte-identity, but both are worth surfacing.

## Findings

### Critical
*(none)*

### High
*(none)*

### Medium

**M1. `finalize_with_empty_sweep` still emits two trailing stderr blank lines under `--mbias_only`, and never emits Perl's leading "Deleting unused files ..." header.**
File: `rust/bismark-extractor/src/output.rs:256-290`
File: `rust/bismark-extractor/src/state.rs:122-123` (call site)

Perl `bismark_methylation_extractor` lines 319-321:
```perl
unless ($mbias_only) {
    delete_unused_files();
}
```
…so Perl gates the *entire* sweep block (line 582 `sub delete_unused_files` → line 625 `warn "\n\n"`) behind `!mbias_only`. That includes both the leading `warn "Deleting unused files ...\n\n"; sleep(1);` at line 584 and the trailing `warn "\n\n"` at line 625.

Rust currently calls `self.fhs.finalize_with_empty_sweep()` unconditionally from `state.rs::ExtractState::finalize` (line 123). In `--mbias_only`, the `files` HashMap is empty, so the `for` loop runs zero times — but the two trailing `eprintln!()` calls at output.rs:287-288 still fire, emitting two blank stderr lines that Perl would not produce. And in all modes, the leading `Deleting unused files ...` log line that Perl emits is missing entirely.

Impact assessment:
- File-set on disk (the byte-identity contract) is unaffected.
- Captured-stderr regression for users who diff Perl vs Rust stderr logs.
- Plan §3.3 / §5.5.2 V1 specifically called for `output_file_map_empty_sweep_mbias_only_is_noop` to assert "no stderr lines emitted, no remove_file calls." The current implementation fails that intent (no `kept`/`deleted` lines, but two blank lines do go out).

Recommended fix (one of):
1. Gate the sweep in `state.rs::finalize` behind `if !config.is_mbias_only()` to skip the call entirely under MbiasOnly (matches Perl line 319 `unless` gating one-to-one). Cheap; preserves on-disk semantics.
2. Or: short-circuit inside `finalize_with_empty_sweep` — if `self.files.is_empty()`, return `Ok(())` immediately without the trailing `eprintln!()`s.
3. Optionally: emit the leading `Deleting unused files ...` line before the loop, for stderr-log parity with Perl's `warn` at line 584.

This was an explicit gap from the plan's deviations section ("§5.5 reduced from 19+6 to 5+2"). Recommend a follow-up commit that adds the gate + a regression test.

**M2. No test asserts parallel-vs-sequential `call_strings_processed` parity (Plan V3).**
File: tests directory; missing.

Plan §5.5.3 specified `extract_pe_parallel_vs_sequential_call_strings_parity`. Implementation notes acknowledge "reduced from 19+6 to 5+2" but the parity test is exactly the test that proves the `parallel.rs:776-777` PE fix is consistent with `pipeline.rs:275-276`. Without it, a future refactor that breaks one site (and not the other) silently fails to be caught — and the parallel-vs-sequential parity invariant is a Phase F load-bearing assertion.

Recommendation: add a small integration test that runs the same PE BAM at `--parallel 1` and `--parallel 4` and asserts the line `Total number of methylation call strings processed: N` is byte-identical between the two splitting reports. Trivial code.

### Low

**L1. Stale doc-comment in `parallel.rs:772`.**
File: `rust/bismark-extractor/src/parallel.rs:772`
```rust
// Records processed: +2 per PE pair (matches Phase C / Perl :2451).
// Phase C.2 (#864): split counters — records_processed counts pairs ...
report.records_processed = report.records_processed.saturating_add(1);
report.call_strings_processed = report.call_strings_processed.saturating_add(2);
```
The first comment line is stale ("+2 per PE pair") and contradicts the very next line ("counts pairs"). The corrected explanation immediately follows so this doesn't mislead readers in practice, but the stale leader line should be deleted. One-line fix.

**L2. Plan V1 + V2 unit tests still missing.**
File: tests directory.

Plan §5.5.2 specified:
- `output_file_map_empty_sweep_mbias_only_is_noop` (V1) — missing
- `output_file_map_empty_sweep_gzip_kept_file_seals_trailer` (V2) — missing
- `output_file_map_empty_sweep_deletes_zero_record_files` — missing
- `output_file_map_empty_sweep_keeps_non_empty_files` — missing
- `output_file_map_empty_sweep_stderr_log_lines` — partially covered by integration test, but not as a unit test against `OutputFileMap` directly
- `output_file_map_empty_sweep_gzip_empty_is_deleted` — missing

The integration test `empty_file_sweep_emits_perl_format_log_lines_on_stderr` provides end-to-end coverage of the kept/deleted format, but the focused unit-level coverage of the sweep's behavior (especially the gzip-trailer-on-drop interaction) is absent. Recommend re-instating at least the gzip + mbias-only cases (the others are arguably covered by the integration smoke).

**L3. `splitting_report_add_is_commutative` variable naming is inverted.**
File: `rust/bismark-extractor/src/output.rs:774-798`

`a_into_b` is initialised with `b`'s values then `.add(&a)` is called — so it actually computes `b + a`, not `a + b`. Same inversion applies to `b_into_a`. The test still proves commutativity because it asserts equality; the variable names just don't match their semantics. Cosmetic — rename to `b_then_add_a` / `a_then_add_b` or swap the inits.

**L4. `--mbias_only` no-overlap check defensive note in code-comment is slightly misaligned with Perl.**
File: `rust/bismark-extractor/src/output.rs:551-556`

Comment says "Perl's SE branch never sets it, so the check is naturally SE-safe." Correct in spirit — but the Rust check actually relies on `config.no_overlap` being false for SE (which is the case per the dispatch). If someone wires `--no-overlap` to SE via the CLI in the future (a forced-flag use case), this would emit the line in SE mode where Perl would also emit it (Perl `:5037` is unconditional on `$no_overlap` — not gated by `$paired`). So the current Rust behavior matches Perl exactly; the comment "SE-safe" is a defensive overstatement but harmless.

**L5. `write_percent_or_fallback_uses_one_decimal_precision` test is a smoke check only, not a banker's-vs-half-away rounding regression guard.**
File: `rust/bismark-extractor/src/output.rs:738-747`

The fixture `5/40 = 12.5%` is exactly representable in f64, so both Rust's banker's-rounding (`%.1`) and Perl's `sprintf("%.1f", ...)` produce `12.5`. The test only asserts the precision count is 1 decimal, not that rounding-mode divergence is correctly handled. The plan's V4 deferred this to "real-data divergence required for a sharper fixture" — fine for now, just be aware that this test is a documentation marker, not a behavioral lock.

## Verifications performed

- Walked `write_splitting_report` step-by-step against Perl `:4995-5047` (header) + `:2482-2556` (body):
  - Step 2: bare basename ✔
  - Step 7: conditional `Ignoring …` lines, SE vs PE branched correctly, only emit when `> 0` ✔
  - Step 9: `no_overlap` line emitted only when `config.no_overlap` is true ✔
  - Step 10/11: fasta + merge_non_CpG conditional lines ✔
  - Step 12: `\n\n` between header and body (= 3 consecutive `\n` total with step-11's trailing `\n` and step-13's leading line) ✔
  - Step 14: trailing `\n\n` on `methylation call strings processed: N` ✔
  - Step 16: 33 `=` (verified by grep) ✔
  - Step 17: total C's trailing `\n\n` ✔
  - Step 18: methylated trio with `\n\n` on last ✔
  - Step 19: "Total C to T conversions in {ctx} context:" phrasing ✔ + `\n\n` on last
  - Step 20: percentage trio via `write_percent_or_fallback` with last-is-`is_last=true` ✔
  - Last percentage line writes `\n\n\n` baked into output (verified by integration test `splitting_report_byte_shape_matches_perl_format` assertion on `bytes.ends_with(b"\n\n\n")` AND `bytes[len-4] != b'\n'`) ✔
- `write_percent_or_fallback` `\n` vs `\n\n\n` selection: correct (4 unit tests cover both content + fallback branches at both positions).
- STDERR routing: integration test `empty_file_sweep_emits_perl_format_log_lines_on_stderr` actively asserts stderr contains AND stdout does NOT contain `kept`/`deleted` strings. ✔
- `records_written` increment placement (`output.rs:211`): AFTER all `write_all`/`write_yacht_row` calls have succeeded. A partial-write failure propagates through the `?` operator and does not bump the counter. ✔
- `records_processed` semantics: `pipeline.rs:275` (PE) `+= 1`, `pipeline.rs:276` (PE) call_strings `+= 2`. `parallel.rs:776` (PE worker) `+= 1`, `parallel.rs:777` `+= 2`. SE sites `pipeline.rs:166-167` and `parallel.rs:649-650` both `+= 1` on both counters. All four sites consistent. ✔
- `is_paired` routing: `pipeline.rs:81` SE passes `false`; `pipeline.rs:225` PE passes `true`; `parallel.rs:190` receives the dispatch-resolved bool. `state.rs:129` passes `self.is_paired` to `write_splitting_report`, NOT `config.paired_mode`. ✔
- `OutputFileMap` refactor: `cleanup_all` (output.rs:301-328) correctly destructures the new `OutputFileEntry` shape (`records_written: _`). Error path unchanged. ✔
- Harness `case` block (oxy_phase_h_smoke.sh:209-243): three arms — strict cmp for splitting-report + M-bias, `zcat | LC_ALL=C sort | md5sum` for `*.gz`, plain sorted-md5 default. Verdict PASS at line 250-251 counts `RAW + SORTED` as success. ✔
- `#[allow(clippy::write_with_newline)]`: function-level allow at output.rs:478 with detailed rationale at lines 479-485. Choice is reasonable — converting the variadic `write!(... "...\n", arg)` lines to `write_all(b"...")` + manual `format!()` would add ~15 lines of less-readable code; the lint exists to nudge users toward `writeln!` which is wrong here (CRLF on Windows). Allow is justified. ✔
- SPEC §8.3 update: 6-point invariant preamble added (lines 657-666); row 1 relaxed to sorted-content equality for data files; strict-cmp retained as informational secondary check (line 672); file-set-match paragraph for #865 (line 679). §9 N-invariance untouched. ✔
- `cargo test -p bismark-extractor` → 236/0 (matches plan claim). ✔
- `cargo clippy --all-targets -- -D warnings` clean. ✔
- `cargo fmt -p bismark-extractor --check` clean. ✔

## No fixes applied directly

The Medium findings (M1 + M2) and Low findings are sufficiently scoped that the author / Reviewer-B comparison phase should make the call rather than this reviewer applying speculative edits. Recommend addressing M1 in this PR (small, isolated, completes plan §5.5.2 V1 intent), M2 as a small follow-up if not in this PR, and L1 alongside M1.

# Code Review — Phase B (Reviewer A)

**Scope:** Phase B implementation of `bismark-extractor` (commit on branch `rust/iron-chancellor`). Review against `PHASE_B_PLAN.md` rev 1.

**Reviewer focus:** Logic, errors, structure. Cross-referenced against Perl source `bismark_methylation_extractor` lines 2900-3060 (per-call write order/format), 5400-5500 (eager-open + header), and `bismark-io::BismarkRecord::iter_aligned` semantics.

**Validation already passing:** 50 tests green, clippy clean, fmt clean.

## Summary

Phase B is **substantially complete and correct**. The plan rev-1 critical fixes (eager-open with header, counter-before-mbias-only short-circuit, `read_pos_5p` including soft-clip in count) are all implemented faithfully. Perl byte-identity expectations are met for: (i) the literal header line, (ii) the per-call tab-separated row format including the `+`/`-` strand-char-vs-methylation column convention (verified against Perl 2911/2921/2931/2941/2951/2961), and (iii) the directional-library CTOT/CTOB header-only file invariant. The kernel correctly uses `aligned.xm_byte` from `iter_aligned`'s orientation-corrected stream; no re-indexing of `record.xm()` slipped in.

Findings are concentrated in three areas: (1) a plan↔code structural divergence (`write_call` panic vs typed `InternalError`), (2) one missing test enumerated in plan §7.1 (`cleanup_partial_outputs_continues_past_one_failure`), and (3) several minor structural items (excessive `pub` widening, `.expect()` use on filtered-upstream invariants). None of these block Phase B merging.

## Issues by area

### Logic

**L1 (Medium) — `+`/`-` column in `write_call` is correct, but the doc comment in `output.rs:122` is misleading.**

The comment says "tab-separated row: read_id<TAB>strand<TAB>chr<TAB>ref_pos<TAB>xm_byte<LF>", which is *what the docs of §4.2 say* but is **factually wrong** about the second column's semantic meaning. The Perl source at 2911/2921/2931/2941 prints `'+'` for methylated and `'-'` for unmethylated calls — **NOT** a strand character. The Rust code's `strand_char = if call.methylated { '+' } else { '-' }` is correct; only the variable name and the doc-comment label "strand" are misleading. Cite Perl 2929: `if ($methylation_calls[$index] eq 'Z') { ... print ... '+' ... } elsif ($methylation_calls[$index] eq 'z') { ... print ... '-' ... }`. This is a meth-state indicator that the Perl source happened to encode with `+`/`-`. Rename `strand_char` → `meth_char` and update the doc comment. (Plan §4.2 also propagates this same labelling error, but plan text isn't shipped — the code-level comment should be corrected.)

**L2 (Low) — `extract_calls` ignore-region check + soft-clip interaction is correct but undertested.**

The plan §7.1 `extract_calls_walks_cigar_with_soft_clips` covers the `read_pos==2` invariant for `2S8M` with `ignore_5p=0`. There is no test combining soft-clip with non-zero `--ignore`. Consider adding `extract_calls_soft_clip_plus_ignore_5p_combines_correctly`: a `5S20M` record with `ignore_5p=10` should drop first 10 read_pos values (which includes all 5 soft-clip positions + first 5 aligned positions). The current code handles this correctly via `lo=10` and `read_pos_5p < lo` filter, but it's not asserted.

**L3 (Low) — `state.report.records_processed` uses saturating_add but `state.report.calls_total` is also saturating_add.**

Saturating arithmetic at u64 means you'd need 1.8 × 10¹⁹ records to actually saturate; this is dead defence but harmless. Consistent with the codebase style (dedup uses similar pattern).

### Errors

**E1 (Medium) — `OutputFileMap::write_call` panics on missing key instead of returning `InternalError`.**

Plan §5.3 explicitly says "Missing key is an `InternalError`". The implementation at `output.rs:117-120` uses `.expect("OutputFileMap missing key — eager-open should have created all 12; this is an InternalError caller should catch")`. This is a programmer-invariant violation panic (cannot fire because eager-open creates all 12 keys), but the plan-specified contract is a typed error path. Two ways to resolve:

- **Recommended**: change the return type of `write_call` from `Result<(), std::io::Error>` to `Result<(), BismarkExtractorError>` and convert the `expect` to `.ok_or_else(|| BismarkExtractorError::InternalError { message: ... })`. This requires propagating the change through `route_call`'s signature and `extract_se`'s `route_call?` site. ~10 LOC.
- **Alternative (lower-effort)**: accept the panic as an internal-invariant guard. Document in the type-level rustdoc on `OutputFileMap` that "the 12 default-mode keys are guaranteed by eager-open" and the panic is a defence against future refactors.

For Phase B alpha (single caller, well-tested), either is defensible. The plan contract favours #1.

**E2 (Low) — `pipeline.rs:89` `.expect("mapped record must have reference_sequence_id")` is a panic-on-input.**

`bismark-io::records()` filters records with FLAG & 0x4 (unmapped) per `read.rs:7,256`. A mapped record without a `reference_sequence_id` is malformed input, not a programmer bug. Compare to `bismark-dedup/src/pipeline.rs:224` which uses `.ok_or_else(|| InternalError { ... })` for the same situation. Recommendation: replace with `.ok_or_else(|| { state.cleanup_partial_outputs(); BismarkExtractorError::InternalError { message: ... } })` for consistency with the dedup precedent and a graceful failure mode.

**E3 (Low) — `extract_se` `state.finalize(config)?` does NOT call cleanup on failure.**

This is **intentional** per `state.rs:73-78` doc + plan §5.4 invariant. Verified correct. However, the actual Perl behaviour after `finalize`-equivalent failure (e.g., disk-full during splitting-report write) is to die with already-written split files in place — same as the Rust port. No action required; flagged as positive verification.

**E4 (Low) — Cleanup ordering on error paths is consistent across all 4 sites in `pipeline.rs`.**

All 4 pre-finalize error sites (`reader_result` line 65-68, PAIRED-flag 75-82, `chr_table.get` 91-101, `extract_calls` 108-112, `route_call` 116-119) correctly call `state.cleanup_partial_outputs()` before propagating. Verified correct. The smoke test `smoke_se_rejects_record_with_paired_flag_set` exercises one of these paths end-to-end and confirms 0 files remain.

### Efficiency

**P1 (Low) — `render_qname` allocates a new `String` per record even when no `Err(InvalidXmByte)` will fire.**

`call.rs:143` renders the QNAME unconditionally at the top of `extract_calls`. For a typical 55M-record run, this is 55M small allocations (~30 bytes each, ~1.6 GiB allocator traffic) purely for error-message scaffolding. Consider lazy-rendering inside the `classify_xm_byte` error path via a closure or `Cow<str>`. Phase F (rayon multicore) will amplify this allocator pressure. **Not a Phase B blocker** — flag for Phase F profiling.

**P2 (Low) — `format_meth_line` performs 7 individual `write_all` calls instead of one buffered format.**

`output.rs:123-132` issues 7 separate `write_all` syscalls per call. With `BufWriter` buffering this is amortised, but each call still hits the BufWriter state machine. A single `writeln!(w, "{}\t{}\t{}\t{}\t{}", ...)` may inline better and reduce instruction count by ~3x. The `record_name: &[u8]` part complicates this (writeln expects `Display`); current approach is reasonable. Phase F profiling target.

### Structure

**S1 (Medium) — Plan §7.1 lists `cleanup_partial_outputs_continues_past_one_failure` but it's absent from tests.**

The test `cleanup_partial_outputs_removes_all_12_files` exists; the "continues past one failure" variant does not. Adding a test that pre-removes one of the 12 files (so `remove_file` fails on it) and asserts the other 11 are still cleaned up + `eprintln!` warning fires would close this gap. ~15 LOC.

**S2 (Low) — Several `pub(crate)` items promoted to `pub` for test visibility.**

`XmClassification`, `OutputKey`, `OutputFileMap`, `SplittingReport`, `BISMARK_VERSION`, `SPLIT_FILE_HEADER`, `MbiasPos`, `MbiasTable`, `ExtractState`, `route_call`, `extract_calls`, `build_chr_name_table`, `derive_basename` are all `pub` in the lib surface. The skill brief flagged this for audit.

Acceptable for an alpha crate (1.0.0-alpha.2) — most of these are foundational types the binary builds on, and the SemVer policy for `alpha` allows iteration. The only items that look genuinely *internal* are:
- `OutputKey` — internal map-key with no external use. Could revert to `pub(crate)` with `#[cfg(test)] pub` if integration tests need it. Low priority.
- `XmClassification` — internal enum the kernel uses. Tests use it; could keep `pub` or move classifier tests inline as `#[cfg(test)] mod tests` in `call.rs`.

Recommendation: live with the wider surface for Phase B; revisit at v1.0 stabilisation when SemVer hits.

**S3 (Low) — `derive_basename` matches Perl spec but has a subtle "default" path.**

The current code returns the filename unchanged if none of `.bam`/`.sam`/`.cram` match. Perl's regex chain also leaves the filename unchanged in that case (Perl's `s/.../.../;` is non-fatal on no-match). Verified correct. The test `derive_basename_strips_known_suffixes` covers the no-match case (`a` → `a`) and the case-sensitive case (`a.BAM` → `a.BAM`). Good coverage.

**S4 (Low) — `state.rs` field naming inconsistency.**

`ExtractState::fhs` is the file-handle map (named after Perl's `%fhs` hash). Adjacent fields use spelled-out names: `mbias`, `report`, `input_path`, `splitting_report_path`, `emit_splitting_report`. The Perl-mnemonic `fhs` is fine for someone reading both source trees side-by-side, but a future maintainer who hasn't touched the Perl will read `fhs` as cryptic. Consider `output_files` or `split_files` at a later cleanup. Non-blocking.

**S5 (Nit) — `output.rs:43-56` `DEFAULT_KEYS` is a static `[(_, _); 12]`.**

Could be a `const fn` enumeration over `CytosineContext::all() × BismarkStrand::all()` if the enums grew `all()` constructors. Mostly cosmetic; the explicit list is more grep-friendly. Keep as-is.

**S6 (Nit) — Plan §7.1 listed `extract_se_two_records_route_to_different_files` and `extract_se_empty_input_writes_only_header_files` as unit tests; both are implemented at the smoke-test level only.**

Both are covered functionally by `smoke_se_directional_produces_all_12_files_and_report` (multi-record OT+OB routing) and `smoke_se_empty_bam_writes_only_header_files` (empty BAM). Acceptable — they exercise the full pipeline rather than the unit. The smoke variant is arguably stronger coverage.

## Cross-reference vs the 10 scrutiny items

1. **xm_byte invariant** — VERIFIED. `call.rs:147-156` uses `aligned.xm_byte`; no `record.xm()[...]` reindexing. Defensive doc-comment at lines 112-117 captures the invariant.
2. **Eager-open file creation** — VERIFIED. `output.rs:83-97` creates all 12 files unconditionally; header written immediately when `!no_header`. Matches Perl 5405-5499 (read end-to-end). Test `output_file_map_eagerly_creates_all_strand_files_for_default_mode` asserts 12-files-exist-pre-write-call.
3. **`route_call` ordering** — VERIFIED. `route.rs:30-68`: mbias accumulate (line 31-40) → counter increment (43-63) → mbias_only short-circuit (66-68) → write (70-75). The order matches the plan §5.5 spec. Test `route_call_increments_counter_before_mbias_only_short_circuit` locks the invariant.
4. **Cleanup invariant** — VERIFIED. All 4 pre-finalize error sites call `state.cleanup_partial_outputs()`; `finalize` errors do NOT cleanup (plan §5.4 invariant). See E3.
5. **`derive_basename`** — VERIFIED. Case-sensitive single-suffix strip via `strip_suffix`. Test covers `a.BAM` (unchanged), `a.bam.gz` (unchanged), `/path/to/sample.bam` (→ `sample`).
6. **Phase-gate dispatch** — VERIFIED. `main.rs::run` has 6 rejections (multiple files, PE, non-default mode, gzip, parallel!=1, bedgraph/cytosine_report) before `extract_se` is invoked. Each is tested by a `main_rejects_*` test.
7. **Non-ASCII chr name** — VERIFIED. `header.rs:24-39` errors with `NonAsciiChromosomeName`. Test `build_chr_name_table_rejects_non_ascii` exercises it with `"chr_α"` UTF-8 bytes.
8. **`pub(crate)` → `pub` audit** — See S2.
9. **Test coverage vs §7.1** — Mostly covered. Gaps: `cleanup_partial_outputs_continues_past_one_failure` (see S1). The `extract_se_two_records_...` / `extract_se_empty_input_...` tests are covered at smoke level (see S6).
10. **`percent_meth` zero-denominator** — VERIFIED. `output.rs:194-201` returns 0.0 for zero total; no NaN, no panic. Test `splitting_report_percentage_handles_zero_denominator` asserts.

## Fixes applied

None — Reviewer A is a research/review role.

## Prioritised recommendations

**Critical:** None.

**High:** None.

**Medium:**
- **E1**: Decide between (a) widening `OutputFileMap::write_call`'s return type to `BismarkExtractorError` for typed-error parity with the plan, or (b) documenting `.expect()` as an internal invariant. Pick one and update either the code or the plan text.
- **L1**: Rename `strand_char` → `meth_char` in `output.rs:121` and fix the misleading doc-comment at line 122. The behaviour is byte-correct; only the label is misleading.
- **S1**: Add `cleanup_partial_outputs_continues_past_one_failure` test (~15 LOC) to close the plan §7.1 gap.

**Low:**
- **E2**: Replace `.expect("mapped record must have reference_sequence_id")` in `pipeline.rs:89` with `.ok_or_else(|| InternalError { ... })` for consistency with the dedup precedent.
- **L2**: Add `extract_calls_soft_clip_plus_ignore_5p` test combining soft-clip and ignore_5p semantics.
- **P1**: Note the `render_qname` allocation cost as a Phase F profiling target.
- **P2**: Note the 7-write-call write pattern as a Phase F profiling target.

**Nits:** S3-S6.

## Verdict

**APPROVE-WITH-NITS.**

The implementation faithfully executes plan rev 1's three critical fixes (eager-open + header, counter-before-mbias_only short-circuit, soft-clip read_pos counting). Perl byte-identity is locked at the header-line and per-call-row level for the default-mode SE subset. The plan↔code divergences are limited to one structural choice (`.expect()` vs typed `InternalError`) and one missing test (`cleanup_partial_outputs_continues_past_one_failure`). Neither blocks merging; both can be addressed before Phase C arrives.

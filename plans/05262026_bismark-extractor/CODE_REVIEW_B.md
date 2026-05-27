# Code Review B — Phase E (`bismark-extractor` v1.0.0-alpha.5)

**Reviewer:** B (independent dual review)
**Date:** 2026-05-27
**Scope:** Phase E implementation per `PHASE_E_PLAN.md` rev 1
**Verdict:** Implementation correctly executes the plan including the Critical-1 yacht polarity fix. Test suite (201 / 0) and clippy / fmt are clean. Findings below are mostly Low / nit; no Critical or High issues found.

---

## Summary

The implementation follows the rev 1 plan closely. The two SPEC fixes are in place. The Critical-1 yacht reverse-strand polarity is implemented as designed in `route_call`, dispatching on `BismarkStrand` and matching Perl `:4350, 4382, 4422-4447` byte-for-byte. `u32::try_from` is used for both `alignment_start` and `reference_end` with `InternalError` graceful failure. The `mbias_only_silence` catch-arm is correctly narrowed to `InvalidXmByte`. `is_mbias_only()` is properly centralised on `ResolvedConfig` and consulted at all three derivation sites. The gzip plumbing — `Box<dyn Write + Send>` inside `BufWriter` — is sound; gzip footers are written on Drop, and process-exit cleanup ensures external readers see complete `.gz` streams.

I confirmed yacht's 8-column format matches Perl `:4472` line-by-line. The plan text in §4.2 describes col-2 as "{+|-}" with a `Source is Perl $strand variable` note that implies the orientation byte, but the implementation correctly uses col-2 = **methylation char** (`+` for methylated `Z`/`X`/`H`, `-` for unmethylated `z`/`x`/`h`) and col-8 = orientation byte, matching Perl exactly. The plan text is mildly misleading but the implementation is right.

The 5 `main_accepts_*_no_longer_rejected` tests use `predicates::str::contains("output mode X").not()` against a tempbam that fails with a different error. They pass for the **right** reason because (a) `.failure()` would fail if the gate were removed entirely-but-the-mode-actually-worked + tempbam was valid, and (b) the asserted negative substring `"output mode Comprehensive"` would have appeared in any previous gate message. They are weak though — see Low-3 below.

---

## Critical

None.

---

## High

None.

---

## Medium

### M-1: Yacht col-4 (`ref_pos`) for `-`-strand reads — verify upstream orientation correction stays correct

`route_call` writes yacht col-4 from `call.ref_pos`, which comes from `aligned.ref_pos` (via `iter_aligned` in `bismark-io`). For Perl's `-`-strand reads in yacht mode, the `$start` variable is **mutated** at line 4433 (`$start += $1 - 1`) before the print at 4472 emits col-4 as `$start + $index + $pos_offset`. This makes Perl's col-4 the reference position of the call walking *backwards from the corrected end*.

For the Rust path: if `bismark-io::CigarExt::aligned_positions` already orientation-corrects `ref_pos` for `-`-strand reads (i.e. emits ref positions in 5′-of-the-sequenced-read order — `start + len - 1`, `start + len - 2`, …), then `call.ref_pos` matches Perl byte-for-byte. This was a Phase B / C contract not a Phase E one, so a passing Phase E test suite is some signal — but the yacht smoke does not explicitly verify col-4 byte-for-byte against a known fixture (only that col-6 > col-7 and col-7/col-8 polarity).

**Recommendation:** add a single assertion in `smoke_yacht_emits_1_file_with_8_col_rows_and_reverse_strand_swap` that the OB row's col-4 equals the expected per-Perl ref position. For the existing fixture `b"Z...."` at alignment_start=400 with the OB tagging, the first emitted call should land at ref_pos 404 (since the corrected start = `400 + 5 - 1 = 404` and the first XM byte after 5′-orientation correction maps there). One extra check adds confidence cheaply.

### M-2: `lib.rs` rustdoc status block is stale (still says Phase D)

`src/lib.rs:9-21` advertises the crate as "Phase D — SE + PE extraction loops + M-bias.txt writer (crate version: `1.0.0-alpha.4`)" and asserts that "Non-default output modes, `--gzip`, … `--mbias_only` still rejected." The Cargo.toml version is alpha.5, the description on package level says Phase E, but `rustdoc` (which is the public-facing surface) lags. Trivial fix; user-visible.

---

## Low

### L-1: Yacht col-2 wording in plan vs implementation

Plan §4.2 says col-2 is `{+|-}` and the row template reads `read_id<TAB>{+|-}<TAB>chr...`. The §4.2 table caption later clarifies col-8 is orientation but doesn't disambiguate col-2. The implementation (correctly per Perl `:4472`) makes col-2 the **methylation char** (`+` if `call.methylated`, `-` otherwise). The test `write_yacht_row_forward_strand_emits_8_cols_with_col6_lt_col7` checks `cols[1] == "+"` for a methylated `Z`, and `write_yacht_row_reverse_strand_swaps_col6_col7` checks `cols[1] == "-"` for unmethylated `z`. Behaviour is correct; plan text is ambiguous. Not actionable in code, but worth a one-line clarification if the plan is ever revised.

### L-2: `write_yacht_row` makes ~10 syscall-shaped `write_all` calls per row

The function calls `writer.write_all` 17 times per row (each separator + each field). Since the outer wrapper is `BufWriter` (8 KiB), all 17 calls coalesce into the BufWriter's internal buffer — no real syscalls — so this is fine. Plan §8 already calls this out as "format_yacht_row allocates one Vec<u8> per call" intended; the actual implementation is *better* than the plan signature (no `Vec<u8>` allocation at all, direct write_all). Nit only: a single `write!` macro or `itoa` for the integer conversions would be slightly faster, but Phase F profiling is the proper time to revisit.

### L-3: `main_accepts_*_no_longer_rejected` tests use weak negative predicates

These tests assert `predicates::str::contains("output mode Comprehensive").not()` after running the binary on a junk BAM. They pass because the run fails with a BAM-parsing error and the negative substring never appears. If a future change accidentally re-introduces a phase-gate with different wording (e.g. `"Comprehensive output mode not yet supported"`), the test would still pass — a false negative.

**Recommendation:** Pin the test more tightly. Two options:
1. Run the binary against the existing valid synth BAM (`write_se_directional_bam`) and assert exit 0. This is what `smoke_comprehensive_emits_3_files_with_context_infix` already does, so the `main_accepts_*` tests are partially redundant.
2. Replace the negative predicate with a positive one keyed on the actual BAM-parsing failure (e.g. `predicates::str::contains("noodles").or(predicates::str::contains("InvalidMagicByte"))`) so the test confirms the binary got *past* the phase-gate.

Since the smoke tests already prove the end-to-end success path, option 1 (delete these 5 tests) is the cleaner move. Defer; not a Phase E blocker.

### L-4: Helper duplication between `se_phase_b_smoke.rs` and `output_modes_phase_e_smoke.rs`

`synth_record`, `header_with_chr1`, and `write_empty_bam`-style helpers are duplicated. The plan §7.2 deviation note explicitly acknowledges this and defers a `tests/common/mod.rs` extraction. Three-file duplication is approaching the threshold where a shared helper module pays for itself; queue for the post-Phase-E cleanup PR.

### L-5: `flush_all` returns `io::Error` but doesn't propagate gzip-Drop write failures

`BufWriter::flush` propagates errors from the inner `Write::write_all`, but the **gzip footer** is only written when `GzEncoder::drop` runs. `Drop::drop` cannot return errors, so a footer-write failure (e.g. disk fills between the last call and Drop) is silently swallowed. The plan §4.6 row "panic mid-write" partially documents this for the panic path, but the same risk exists on the **normal exit path**: `state.finalize → fhs.flush_all` returns `Ok` even though the gzip stream is incomplete.

**Mitigation in the current design:** the entire BufWriter+GzEncoder chain is dropped *inside* `extract_se`'s stack frame, before `main::run` returns. If a footer-write fails, the `.gz` file is truncated. Tests (e.g. `output_file_map_gzip_writes_valid_gz_content_byte_identical_to_plain`) run in environments where Drop succeeds, so this is unobservable.

**Recommendation:** call `gz_encoder.try_finish()` explicitly in `flush_all` (would require holding the GzEncoder via a typed enum rather than `Box<dyn Write>`, or via an explicit `finish` method on the `OutputFileMap`). Defer to Phase F's `Box<dyn Write>` → enum refactor (already in plan §9.2 #2). Document this gap in `output.rs` rustdoc near `flush_all` so future readers know the failure mode.

### L-6: Defensive `route_to_key` returning `None` in `write_call`

`OutputFileMap::write_call` at line 138-141 handles the `route_to_key → None` case by returning `Ok(())`. This is unreachable in normal flow (the route_call short-circuit at line 74 of route.rs handles MbiasOnly before write_call is ever called), so the dead branch is dead-defensive. The implementation comment acknowledges this. Two options:

- Leave as-is (current): defensive, silent, no extra error noise.
- Change to `InternalError` (mirroring the `else` branch a few lines down for missing-key in the HashMap): louder, surfaces a contract-break.

The current "silent Ok" is consistent with "MbiasOnly = no per-context writes" semantically; the `InternalError` would only fire if some future refactor moved the short-circuit. Either choice is reasonable. Recommend keeping `Ok` but adding a `debug_assert!(self.mode != OutputMode::MbiasOnly)` to surface the contract in debug builds.

### L-7: `flate2 = "=1.1.9"` pin works but `cargo tree` verification not preserved in CI

The Cargo.toml comment notes the pin was verified via `cargo tree -p bismark-extractor | grep flate2`. There's no automated check that future `noodles_bgzf` bumps don't introduce a second `flate2` version (dup-deps inflate binary size). Plan §9.1 documented this as an "Implementer MUST verify before committing" instruction — fine for Phase E but it'll silently rot.

**Recommendation:** add a one-line CI check (or a `[workspace.lints]` rule via `cargo-deny`) that fails on duplicate `flate2` versions. Defer; not a Phase E blocker.

### L-8: `write_call`'s 5-column format duplicates `route_call`'s qname resolution

`route_call` resolves `qname` from `record.inner().name()`, and `OutputFileMap::write_call` accepts it via the `record_name: &[u8]` parameter. For non-yacht modes the qname is written directly (`writer.write_all(record_name)`). This is fine, but the duplication-vs-`<unnamed>` fallback only lives in `route_call`. If a future caller of `write_call` (e.g. a unit test, or Phase F's per-worker writer) passes an empty slice, the result is a row starting with `\t`. Tests use `b"read1"` explicitly so no issue today.

Nit only; document the convention on `write_call`'s `record_name` parameter ("must be non-empty; caller resolves the `<unnamed>` fallback").

### L-9: Yacht `write_yacht_row` ignores the `call.context` field

`write_yacht_row` takes `&MethCall` (which includes `call.context`) but only uses `call.methylated`, `call.ref_pos`, `call.xm_byte`. The `context` is encoded in `call.xm_byte` already (Z/z = CpG, X/x = CHG, H/h = CHH), so this is correct — but the function takes the full struct just for those three fields. Minor: clarity could be improved by passing only the needed fields (`ref_pos`, `xm_byte`, `methylated`) and avoiding the implicit "yacht is context-agnostic" coupling. Defer.

---

## Probe-list responses

I'm answering each "Things worth probing" item from the brief:

1. **Yacht reverse-strand polarity** — Correct. Forward `(alignment_start, reference_end)`, reverse `(reference_end, alignment_start)`. `BismarkStrand::{OT, CTOB} → forward`, `{OB, CTOT} → reverse`. Matches Perl `:4350, 4382, 4406, 4422-4447`. Verified against Perl line 4472's `print` statement.

2. **u32 cast safety** — Both `alignment_start` and `ref_end_usize` use `u32::try_from(...).map_err(...)` and return `BismarkExtractorError::InternalError` on overflow. Defensive: human/mouse genomes fit in u32 (max ~250M); chr1 of T2T-CHM13 maps comfortably. The guard is correct.

3. **Gzip footer + flush_all** — `flush_all` calls `BufWriter::flush`, which pushes buffered bytes into the inner `GzEncoder` but does **not** finalize the gzip stream. The footer is written by `GzEncoder::drop`, which runs when the `OutputFileMap` is dropped (i.e. when the owning `ExtractState` is dropped, i.e. at `extract_se`'s function return). Since `state.finalize` is called inside `extract_se` and `main::run` only completes after `extract_se` returns, external readers (smoke tests) see the complete `.gz` once the binary process has exited. **However**: if the gzip-footer write fails during Drop (disk-full at footer-write time), the error is silently swallowed and the `.gz` is truncated. See Low-5.

4. **`mode_keys` Vec ordering** — Matches the plan's documented order:
   - Default: CpG × {OT, CTOT, CTOB, OB}, then CHG × …, then CHH × …. (Outer loop = context, inner = strand.) ✓
   - Comprehensive: CpG, CHG, CHH. ✓
   - MergeNonCpG: CpG × {OT, CTOT, CTOB, OB}, then Non_CpG × …. ✓
   - ComprehensiveMergeNonCpG: CpG, Non_CpG. ✓
   - Yacht: any_C_context. ✓
   - MbiasOnly: empty. ✓
   - `mode_keys_default_filenames_match_perl_open_order` explicitly asserts indices 0-11.

5. **`write_yacht_row` column distinctness** — Col-2 (methylation char) and Col-8 (orientation) are derived from independent sources: col-2 from `call.methylated`, col-8 from `pair_strand` via `orient_byte`. Confirmed not aliased. Both visually use `+`/`-` bytes which can be confusing if read out of context, but they encode different semantic data. Verified by `write_yacht_row_reverse_strand_swaps_col6_col7` which asserts `cols[1] == "-"` (unmethylated) and `cols[7] == "-"` (OB orientation) — distinct sources.

6. **`route_to_key → None` defensive path in MbiasOnly** — Reachable only if `route_call`'s short-circuit at line 74 is bypassed. Currently unreachable in normal flow. The defensive `return Ok(())` is safe. See Low-6.

7. **`is_mbias_only()` sync across 3 sites** — Verified:
   - `state.rs:79`: `mbias_only: config.is_mbias_only()`.
   - `state.rs:62-68`: passes `config.output_mode` to `OutputFileMap::new`, which calls `mode_keys(mode, …)`, which returns empty Vec for `MbiasOnly`.
   - `pipeline.rs:144`: `extract_calls(…, config.is_mbias_only())` in SE.
   - `pipeline.rs:317`: `let mbias_only_silence = config.is_mbias_only()` in PE.
   All consistent.

8. **5 `main_accepts_*` tests using `.not()` predicate** — Pass for the right reason today, but weakly. See Low-3.

9. **Documented deviations**:
   - `flate2 = "=1.1.9"` — reasonable. The plan said `=1.0.34` but the actual transitive dep is now `1.1.9` (noodles_bgzf bumped). Cargo.toml comment cites the verification step.
   - `smoke_gzip_cleanup_on_write_failure_removes_gz_files` skipped — reasonable. The comment in `output_modes_phase_e_smoke.rs:18-24` cites portable-injection difficulty (Linux-only `/dev/full`). The cleanup_all behaviour is covered by unit tests on `OutputFileMap`. Mild risk: the cleanup path under gzip is *not* directly exercised, but Phase B's `cleanup_all` semantics carry over.

10. **Helper duplication** — Acknowledged in `output_modes_phase_e_smoke.rs:46-48`. Three test files now share `synth_record` + `header_with_chr1`; the `tests/common/mod.rs` extraction is overdue but a clean-up PR rather than a Phase E blocker. See Low-4.

---

## Recommendations (ordered by priority)

1. **M-2**: Update `lib.rs` rustdoc to reflect Phase E (~10 LOC).
2. **L-3**: Either delete the 5 `main_accepts_*_no_longer_rejected` tests (redundant with smoke tests) or tighten the negative predicates.
3. **M-1**: Add a yacht col-4 assertion in the smoke test (one extra `assert_eq!`) to lock the upstream `ref_pos` orientation contract.
4. **L-5**: Document the silent gzip-footer-failure risk in `output.rs`'s `flush_all` rustdoc; queue `try_finish` explicit call for Phase F.
5. **L-7**: Add a duplicate-`flate2`-version CI guard (or document the manual verification in `CONTRIBUTING.md`).
6. **L-4**: Extract `tests/common/mod.rs` in a follow-up cleanup PR.

None of these are Phase E blockers. The implementation is ready to merge.

---

## Verdict

**APPROVE.** Implementation correctly executes plan rev 1, the Critical-1 fix is in place and tested, and the test suite (201 / 0) plus clippy / fmt are clean. Six low-priority items listed above; none block merge.

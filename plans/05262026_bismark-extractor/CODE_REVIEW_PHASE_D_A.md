# Code Review — Phase D (`bismark-extractor` M-bias.txt writer)

**Reviewer:** A
**Branch:** `extractor-phase-d`
**Plan:** `plans/05262026_bismark-extractor/PHASE_D_PLAN.md` (rev 1)
**Crate version:** `1.0.0-alpha.4`
**Date:** 2026-05-26

## Summary

Phase D adds a self-contained M-bias.txt writer (`mbias_writer.rs`, ~230 LOC) that consumes the `[MbiasTable; 2]` accumulator populated in Phases B + C, plus minimal touchpoints in `mbias.rs` (`max_position` + `debug_assert!`), `state.rs` (new `is_paired` field + reordered `finalize`), `pipeline.rs` (2-line callsite update), `lib.rs` (module + re-exports), and `Cargo.toml` (version bump). Three SPEC.md prose errors surfaced across Phases B/C/D are retroactively corrected. Two new test files add 26 unit tests + 3 end-to-end smoke tests; 5 Phase B `ExtractState::new` callsites and 1 dir-entry count are updated for the signature ripple.

Implementation tracks the rev-1 plan faithfully. All 11 key plan claims I was asked to verify hold. The work is byte-identity-load-bearing for Phase H and the writer covers the full SPEC §4.2 surface. No Critical or High issues found.

## Issues by area

### Logic — clean

- **`finalize` ordering (state.rs:106-119):** correctly `flush_all → write_splitting_report → write_mbias_txt`. The `if !config.mbias_off` gate wraps the `write_mbias_txt` call only; `write_splitting_report` remains gated by its existing `self.emit_splitting_report` (Phase B behaviour preserved). Matches Perl `:2463` (splitting-report inline in `process_X_read_file`) followed by `:314` (M-bias after function returns). Failure-semantic claim is sound: a `write_mbias_txt` disk-full error leaves the splitting-report on disk.
- **Filename strip chain (mbias_writer.rs:60-74):** `["gz", "sam", "bam", "cram", "txt"]` order matches Perl `:633-637`. Each `strip_suffix` runs at most once on the running string. Traced cases:
  - `sample.bam`: `gz` no, `sam` no, `bam` yes → `sample.`, `cram` no, `txt` no → **`sample.`** ✓
  - `sample.bam.gz`: `gz` yes → `sample.bam.`, `sam` no, `bam` no (tail is `.`), `cram` no, `txt` no → **`sample.bam.`** ✓
  - `sample.txt`: only `txt` matches → **`sample.`** ✓
  - `sample.gz.gz` (defensive trace): `gz` matches once → `sample.gz`, subsequent four no-match → **`sample.gz`** (matches Perl's single-pass `s/gz$//`)
- **`MbiasTable::max_position` (mbias.rs:86-91):** `m_i = vec.len().saturating_sub(1) as u32`; with `len == 0` yields 0, with `len == 1` (slot 0 only) yields 0, with `len == 100` (slots 0..99) yields 99. Writer iterates `1..=max_position`, so position 0 is never written — matches the slot-0-unused invariant.
- **`debug_assert!` message (mbias.rs:52-55):** `"MbiasTable::accumulate: position must be 1-based (>= 1), got {position_1based}"`. The test substring match `expected = "position must be 1-based"` on line 135 of the test file is satisfied. ✓
- **Section header byte-counts (mbias_writer.rs):** SE `===========` is exactly 11 chars; PE R1 / PE R2 `================` is exactly 16 chars (confirmed via shell-counted line lengths). Matches Perl `:722, :726, :825`.
- **Zero-coverage row format (mbias_writer.rs:209):** `writeln!(w, "{pos}\t{meth}\t{un}\t\t{coverage}")` — the `\t\t` between `un` and `coverage` is byte-explicit, not f-string interpolated. Cannot accidentally be lost to a missing format arg.
- **5-col column header (mbias_writer.rs:184-186):** byte-exact `"position\tcount methylated\tcount unmethylated\t% methylation\tcoverage\n"` (via `writeln!` adding the trailing `\n`).
- **`is_paired` threading:** `extract_se` passes `false` (pipeline.rs:81), `extract_pe` passes `true` (pipeline.rs:202). The field is consumed only by `finalize` (state.rs:117 — `self.is_paired`). No accidental other readers (grep confirms no other field access in src/).
- **Phase B regression scope:** Only edits are the 5 `ExtractState::new` callsite updates in `tests/se_phase_b.rs` (lines 646, 682, 732, 768, 799) and the 1 dir-entry count change in `tests/se_phase_b_smoke.rs` (13→14, line 316). Each rippled exactly as the plan promised — no other Phase B/C surface was touched.
- **Trailing blank line (mbias_writer.rs:214):** `writeln!(w)?` is unconditional, outside the `for pos in 1..=max_position` loop. Even when `max_position == 0` (empty mbias case), the blank line is emitted. Matches Perl `:762`'s unconditional `print MBIAS "\n";`.

### Efficiency — clean

- Single 8 KiB `BufWriter<File>` for the entire write. Per-section ~9 KB at typical Illumina read lengths; total PE ~60 KB. One `flush` at end. No allocations in the hot loop beyond the per-row `writeln!` interpolation.
- `MbiasTable::max_position` is O(1) (three length lookups).
- The `for &context in &[CytosineContext::CpG, CytosineContext::CHG, CytosineContext::CHH]` iteration constructs a 3-element array on the stack per call; trivial.
- One minor observation (not a defect): `write_one_section` looks up `vec` via a `match context` then iterates positions. Could collapse with `MbiasTable::get(context)` if such a helper existed, but inlining the match is fine for a 3-arm pattern.

### Errors — clean

- All disk errors propagate via `?`: `File::create` (top of `write_mbias_txt`), each `writeln!`, and the final `flush`. Caller (`state.finalize`) wraps as `BismarkExtractorError::IoWrite` via `From<std::io::Error>` (Phase A's error module). No silent failures.
- `derive_mbias_basename` panics on `path.file_name().expect(...)` — the panic message is informative and the caller (`pipeline::extract_se`/`extract_pe`) already validates the input path via `Cli::validate`. Consistent with `derive_basename`'s contract.

### Structure — clean

- Module split — accumulator stays in `mbias.rs`; writer lives in `mbias_writer.rs` — is the right cut. Keeps Phase F's eventual reducer concerns isolated from output formatting.
- `ReadIdentitySection` enum is private to `mbias_writer`; correct visibility.
- Doc-comment cross-references between `pipeline::derive_basename` and `mbias_writer::derive_mbias_basename` (per rev-1 Reviewer A I3 / Reviewer B O5) are clear and bidirectional. ✓
- `state.rs` doc on `finalize` explicitly cites the rev-1 fix and the Perl line numbers — excellent for future maintainability.
- Test file structure: `mbias_writer_phase_d.rs` mirrors §7.1 labels verbatim; `mbias_writer_phase_d_smoke.rs` lives as a new file per rev-1 Reviewer B I2 (avoids touching in-review PRs' smoke files).

## SPEC.md edits — verified

All three retroactive prose corrections (§4.2, §7.4, §8.4) match what the rev-1 plan promised AND what the actual code does:

- **§4.2 5-col table:** new prose reads "5-col table `position<TAB>count methylated<TAB>count unmethylated<TAB>% methylation<TAB>coverage`". The Phase D writer at mbias_writer.rs:184-186 emits exactly that.
- **§7.4 disjoint-pair drop:** new prose describes the all-R2-dropped-past-`r1_ref_end` behaviour. `overlap.rs:54`'s `retain(|c| c.ref_pos < r1_ref_end)` produces this effect (`>= r1_ref_end` calls drop). The "early-exit `return`" language is a description of Perl's mechanism, not Rust's — that's appropriate for SPEC prose since the *behaviour* is equivalent.
- **§8.4 eager-open header-only:** new prose reads "CTOT/CTOB files MUST exist on disk with the literal version header line as their only content". `output.rs:71-94`'s `OutputFileMap::new` eagerly opens all 12 files and writes `SPLIT_FILE_HEADER` unless `no_header`. ✓

## Fixes applied

None — Reviewer A's role is research-only per skill spec. All findings below are recommendations.

## Recommendations (prioritized)

### Critical — none

### High — none

### Medium

1. **lib.rs status doc is stale (lib.rs:7-17).** Says "Phase C — SE + PE extraction loops (crate version: 1.0.0-alpha.3)" but the crate is now alpha.4 and Phase D added the M-bias.txt writer. Recommend a one-line bump:

   ```diff
   -//! **Phase C — SE + PE extraction loops** (crate version: `1.0.0-alpha.3`).
   +//! **Phase D — SE + PE extraction loops + M-bias.txt writer** (crate version: `1.0.0-alpha.4`).
   ```

   Plus a sentence about M-bias.txt. Low risk; cosmetic but visible to anyone running `cargo doc`.

### Low

2. **`mbias_writer.rs:144-148` array-literal style.** The 3-element `&[CytosineContext::CpG, CytosineContext::CHG, CytosineContext::CHH]` literal is constructed fresh each `write_three_sections` call. Inconsequential perf-wise but could be a `const CONTEXTS: [CytosineContext; 3] = [...];` at module scope if you ever want to share the iteration order with Phase E. Pure aesthetic; ignore unless touching the file for other reasons.

3. **`mbias_writer.rs:200` `meth.saturating_add(un)` cosmetic.** Coverage is `u64`; overflow requires `2 × u64::MAX` distinct calls per position. The `saturating_add` is defensive (matches accumulate's own saturating). Acceptable; alternative would be plain `+` for clarity, but the precedent in `accumulate` already uses saturating, so the consistency is fine.

4. **Test fixture sharing.** The `synth_record` and `header_no_pg` helpers in `mbias_writer_phase_d_smoke.rs` are near-duplicates of helpers in `se_phase_b_smoke.rs` / `pe_phase_c_smoke.rs`. Rev-1 plan explicitly chose duplication over modification (Reviewer B I2 — avoid touching in-review PR files), so this is intentional. Post-merge follow-up could extract a `tests/common/` helper module. Not Phase D's concern.

## Verdict

**APPROVE.**

Phase D implementation tracks the rev-1 plan faithfully. All 11 plan claims I was asked to verify hold: finalize ordering, filename byte-identity (including the `.bam.gz` and `.txt` edge cases), section-header widths (11 / 16 equals), zero-coverage row format, `max_position` semantics, debug_assert message + test alignment, 5-col column header, `is_paired` threading, three SPEC prose corrections, Phase B/C regression scope discipline, and unconditional trailing blank line. The only finding is a stale Phase-C status doc-comment in `lib.rs` (Medium, cosmetic). No correctness, efficiency, error-handling, or structural defects identified.

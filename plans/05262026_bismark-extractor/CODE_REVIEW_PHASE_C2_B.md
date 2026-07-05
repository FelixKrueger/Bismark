# Code Review — Phase C.2 (Reviewer B)

**Reviewer:** Reviewer B (independent fresh context).
**Scope:** branch `extractor-phase-c2` off `rust/iron-chancellor` HEAD `84c6ad1`. Closes #864 (splitting-report format) + #865 (empty-file sweep); #863 dropped as won't-fix.
**Files reviewed:**
- `rust/bismark-extractor/src/output.rs` (Phase C.2 rewrite — `OutputFileMap`, `write_splitting_report`, `write_percent_or_fallback`, `SplittingReport`)
- `rust/bismark-extractor/src/pipeline.rs:154-167, 270-277` (counter increment sites)
- `rust/bismark-extractor/src/parallel.rs:646-651, 772-778` (counter increment sites)
- `rust/bismark-extractor/src/state.rs:111-138` (finalize ordering)
- `rust/bismark-extractor/SPEC.md` §8.3 (byte-identity invariant)
- `rust/bismark-extractor/Cargo.toml` (version bump)
- `scripts/oxy_phase_h_smoke.sh` (case-block diff)
- `tests/output_phase_c2.rs` (new integration tests)
- Perl `bismark_methylation_extractor` lines 313-625, 1325-1340, 2478-2580, 4990-5048, 5135-5230
- All updated pre-existing tests

**Test result:** 236 tests pass. `cargo clippy --all-targets -- -D warnings` clean. `cargo fmt --check` clean.

---

## Executive summary

Phase C.2 implements the four Critical absorptions and eleven Important review items from the dual plan-review pass cleanly: the splitting-report rewrite matches Perl line-by-line for the SE / PE / merge-non-CpG / comprehensive modes; per-context trailing-newline variance is centralised via `write_percent_or_fallback(is_last)`; `OutputFileMap` correctly accumulates a per-handle `records_written` counter and the sweep routes its `kept`/`deleted` lines to STDERR via `eprintln!`; both `pipeline.rs` and `parallel.rs` PE sites now flip `+= 1` per pair plus add `call_strings_processed += 2`. The integration test at `tests/output_phase_c2.rs:84` correctly captures stderr (not stdout) and exercises the C3 routing. However, **one Critical byte-identity bug is unhandled**: `--yacht` mode silently falls through to `Output specified: strand-specific (default)` and omits the merge-non-CpG note, where Perl `:1331/1333` sets `$full = 1` and `$merge_non_CpG = 1` before report emission. A handful of Medium/Low issues sit around stderr-format divergence with Perl (full path vs basename, missing `Deleting unused files ...` preamble, MbiasOnly trailing-blank-lines), an internally-stale SPEC §8.4 row, and an incorrect `#[allow(clippy::write_with_newline)]` rationale (the CRLF concern doesn't apply to `std::fs::File` in Rust). None of these block the PR; the `--yacht` issue is the only finding that should be addressed before merge.

---

## Critical

### C1 — `--yacht` mode emits the wrong `Output specified:` line and skips the `merge_non_CpG` note

**Files:** `rust/bismark-extractor/src/output.rs:542-549, 564-571`
**Severity:** Critical (byte-identity divergence from Perl in a supported mode).

Perl `:1329-1333` resolves `--yacht` by setting BOTH `$full = 1` AND `$merge_non_CpG = 1` *before* report emission. Then `:5030-5044`:
- `if ($full)` → emits `Output specified: comprehensive\n`.
- `if ($merge_non_CpG)` → emits `Methylation in CHG and CHH context will be merged into "non-CpG context" output\n`.

Both fire in `--yacht` mode.

Rust `output.rs:542-549` matches only `OutputMode::Comprehensive | OutputMode::ComprehensiveMergeNonCpG`. `OutputMode::Yacht` falls through to the `_` arm → emits `Output specified: strand-specific (default)`. Similarly the merge-non-CpG-note check at `:564-571` only matches `MergeNonCpG | ComprehensiveMergeNonCpG`, missing `Yacht`. Worst of all, the percentage block at `:641-682` keys off the same `MergeNonCpG | ComprehensiveMergeNonCpG` match, so a `--yacht` splitting report would emit a 3-context CpG/CHG/CHH percentage block — Perl emits a 2-context CpG/Non-CpG block under `$merge_non_CpG = 1`.

Three byte-identity divergences in one mode. The plan does not call this out as deliberately out-of-scope (§3.5 lists `--yacht` mode as "Single `any_C_context_*` file; sweep applies" — silent on report format). The integration test fixture in `tests/output_phase_c2.rs` doesn't cover `--yacht`; the smoke test in `tests/output_modes_phase_e_smoke.rs::smoke_yacht_empty_bam_emits_header_only_file` checks file deletion but not report content.

**Recommendation:** Either (a) treat `OutputMode::Yacht` as `ComprehensiveMergeNonCpG`-equivalent in three call sites — the `Output specified:` match, the merge-note `matches!`, and the percentage-block `merged_non_cpg` boolean. A single helper like `fn emits_comprehensive_in_report(m: OutputMode) -> bool` + `fn merges_non_cpg(m: OutputMode) -> bool` would centralise the logic. Or (b) at validation time in `cli.rs::validate`, when `--yacht` is passed, also set the resolved `output_mode` accordingly (e.g. introduce a separate `is_yacht: bool` flag or store `output_mode = ComprehensiveMergeNonCpG` plus `is_yacht: bool`). Add a unit test `splitting_report_format_yacht_mode_matches_comprehensive_merge` before fixing to lock the byte-shape regression guard.

---

## High

### H1 — Stderr log lines use bare basename, not Perl's full path

**Files:** `rust/bismark-extractor/src/output.rs:272-281`
**Severity:** High (stderr-format divergence with Perl; matters if tooling parses these lines).

Perl `:607, 615` emits `$sorting_files[$index]` which is the **full path** (`$output_dir . $filename`), not the basename. See Perl `:5144` (`$cpg_ot = $output_dir . $cpg_ot;` then later `push @sorting_files, $cpg_ot`).

Rust uses `path.file_name()` to extract bare basename. This means Rust's stderr lines look like `CpG_OT_one_record.txt was empty -> deleted` whereas Perl's look like `/path/to/out/CpG_OT_one_record.txt was empty -> deleted`.

The plan rev 1 §3.3 / §4.3 wrote "{filename}" without specifying full-vs-basename. Given the plan's stated goal of mirroring Perl's stderr format for downstream tooling compatibility, this is a divergence.

**Recommendation:** Switch to `path.display()` (or `path.to_string_lossy()`) to emit the full path. This matches Perl exactly. The integration test at `tests/output_phase_c2.rs:106-122` would need updating to match (currently asserts `CpG_OT_one_record.txt contains data ->\tkept` — would become `<tempdir>/out/CpG_OT_one_record.txt contains data ->\tkept`; `.contains(...)` substring match still works).

### H2 — `finalize_with_empty_sweep` emits 2 trailing blank lines on stderr even in MbiasOnly (Perl never calls the sweep in MbiasOnly)

**Files:** `rust/bismark-extractor/src/output.rs:283-289`, `rust/bismark-extractor/src/state.rs:122-123`
**Severity:** High (gratuitous 2 blank lines on stderr in MbiasOnly mode; not in Perl).

Perl `:319-321`:
```perl
unless ($mbias_only){
    delete_unused_files();
}
```

The sweep subroutine (which emits `warn "\n\n"` at line 625) is never invoked in MbiasOnly mode.

Rust `state.rs:122-123` calls `self.fhs.finalize_with_empty_sweep()` unconditionally. In MbiasOnly the `OutputFileMap` is empty (correctly — `mode_keys` returns `Vec::new()`), so the loop body never executes; but the function still hits the two `eprintln!()` calls at output.rs:287-288 and writes `\n\n` to stderr. Perl emits nothing here.

**Recommendation:** Guard the call in `state.rs::finalize`:
```rust
if !self.mbias_only {
    self.fhs.finalize_with_empty_sweep()?;
}
```
This also covers the `Deleting unused files ...` preamble issue (M3 below) consistently — when adding the preamble, it should only fire when the sweep would do real work.

---

## Medium

### M1 — `#[allow(clippy::write_with_newline)]` rationale is technically incorrect

**File:** `rust/bismark-extractor/src/output.rs:478-485`
**Severity:** Medium (stale/misleading doc, not a behavior bug).

The function-level allow's justification states:
> `writeln!` emits CRLF on Windows targets, which would break the strict raw-byte gate against Perl's LF output.

This is **incorrect**. `writeln!` always writes `\n` (LF) regardless of platform — it never expands to CRLF. The CRLF translation that some people associate with newlines happens at the C stdio level for text-mode FILE* on Windows, but Rust's `std::fs::File` is always opened in binary mode (Rust has no text-mode I/O). `write!(w, "...\n")` and `writeln!(w, "...")` produce byte-identical output on every platform.

The actual reason to allow the lint here is just style preference — the function genuinely benefits from explicit `\n` and `\n\n` bytes inline with the variadic-format strings to keep the per-Perl-line trailing-newline counts visually unambiguous. That IS a defensible reason; the CRLF claim is not.

**Recommendation:** Update the rationale comment to reflect the real reason: byte-counts at line endings are part of the Perl-byte-identity contract, and keeping the `\n` count visually adjacent to the format string makes the Perl-line correspondence auditable. Drop the CRLF claim.

### M2 — SPEC §8.4 row "Directional library" contradicts the new §8.3 invariant

**File:** `rust/bismark-extractor/SPEC.md:694`
**Severity:** Medium (stale SPEC text contradicts the new file-set-match contract).

§8.3 rev 3 (added by this PR) now lists the file-set match: "Perl's `was empty -> deleted` sweep is mirrored... For a directional library in default mode this means 6 files exist post-run (OT/OB × CpG/CHG/CHH), not 12."

But §8.4 (Edge case fixtures) row "Directional library" still reads:
> Rust output's CTOT/CTOB files **MUST exist on disk** with the **literal version header line as their only content** (NOT 0-byte or absent).

This is the pre-C.2 (Phase B) contract. Post-#865 sweep, the CTOT/CTOB files are unlinked at finalize time. The §8.4 row contradicts the new §8.3 invariant and would cause future readers to add a fixture that fails against the C.2 binary.

**Recommendation:** Update §8.4's "Directional library" row to:
> Post-#865 sweep: directional libraries produce 6 strand-context files post-finalize (OT/OB × CpG/CHG/CHH); CTOT/CTOB files are eager-opened, receive only the version header, and are unlinked by `finalize_with_empty_sweep` because `records_written == 0`. Stderr emits `{path} was empty -> deleted` for each unlinked file.

### M3 — Missing `Deleting unused files ...` preamble on stderr

**File:** `rust/bismark-extractor/src/output.rs:256-289`
**Severity:** Medium (consistent stderr-format with Perl).

Perl `:584` emits `warn "Deleting unused files ...\n\n"; sleep(1);` BEFORE the per-file loop. The `sleep(1)` is just UX padding. The `\n\n`-terminated preamble line IS part of the visible stderr output Perl tooling sees.

Rust skips this preamble. The plan rev 1 doesn't explicitly call it out as either implemented or deliberately not implemented, so it's an undocumented omission.

**Recommendation:** Either (a) add `eprintln!("Deleting unused files ...\n");` before the loop (the trailing `\n` from `eprintln!` plus one extra blank line matches Perl's `\n\n`), or (b) document in the function doc + plan §7.3 that the preamble is deliberately omitted. Skip the `sleep(1)` — it's a Perl UX artifact, not a tooling contract. If choosing (a), also add the preamble to the `tests/output_phase_c2.rs` stderr assertion.

### M4 — `output.rs:282` `?` propagation aborts the sweep on first remove_file failure

**File:** `rust/bismark-extractor/src/output.rs:277`
**Severity:** Medium (best-effort sweep is not actually best-effort).

`std::fs::remove_file(&path)?;` propagates the first error and aborts the loop. The doc comment at output.rs:251-255 explicitly states "Subsequent entries are skipped (the map's drain consumed them; partial state is acceptable since this runs on the success path only)."

This is a behavior choice, but the contrast with `cleanup_all` at lines 320-326 is notable: `cleanup_all` logs failed `remove_file` errors and continues, treating cleanup as best-effort. The sweep doesn't. For a partial filesystem failure (e.g. permission denied on one file while others are deletable), the sweep leaves a half-finished state and the splitting report never gets written (since `finalize` returns the I/O error and `write_splitting_report` is below the sweep in `state.rs:122-131`).

**Recommendation:** Match `cleanup_all`'s pattern — log the first failure via `eprintln!("warning: failed to remove empty file {}: {}", path.display(), e);` and continue. This way the splitting report still gets written even if one CTOT file is undeletable for some reason. Alternatively, document this divergence with `cleanup_all` and accept it — but the current behavior of "first remove_file failure kills the whole finalize pipeline" surprises me; I'd lean toward making it best-effort.

### M5 — `parallel.rs:772-773` doc-comment still references the old `+= 2` semantic

**File:** `rust/bismark-extractor/src/parallel.rs:772`
**Severity:** Medium (stale doc comment).

```rust
// Records processed: +2 per PE pair (matches Phase C / Perl :2451).
// Phase C.2 (#864): split counters — records_processed counts pairs
// for PE (matches Perl `sequences_count`); call_strings_processed
// counts XM strings = 2× pairs (matches Perl `methylation_call_strings`).
```

The first line ("Records processed: +2 per PE pair") contradicts the actual code immediately below (`records_processed.saturating_add(1)`). The second sentence corrects it but the first line is misleading on its own.

**Recommendation:** Drop the first line — keep only the Phase C.2 explanation.

---

## Low

### L1 — Plan §5.5.1 enumerates 14+ unit tests, only 5 were added

**File:** `tests/output_phase_c2.rs` + `output.rs::tests`
**Severity:** Low (gap is well-justified in the plan's Implementation Notes; flagged for completeness).

The plan §5.5.1 listed 13 splitting-report-format unit tests + 6 sweep unit tests + 6 integration tests. The actual delivery is 5 unit tests + 2 integration tests in new files, plus assertion updates to existing tests. The Implementation Notes section acknowledges this reduction and explains it.

Gaps relative to the plan's enumeration:
- `splitting_report_format_se_default` / `_pe_default_no_overlap` / `_with_ignore_5p` / `_with_all_pe_ignore_flags` / `_comprehensive_mode` / `_merge_non_cpg` / `_fasta` / `_with_include_overlap` — not added as inline unit tests. The integration `tests/output_phase_c2.rs::splitting_report_byte_shape_matches_perl_format` covers SE default but not the conditional branches. The pre-existing `tests/se_phase_b.rs` covers SE+ignore. PE-specific report format isn't byte-asserted at this level.
- `splitting_report_format_round_half_away_from_zero` (V4) — the inline test `write_percent_or_fallback_uses_one_decimal_precision` covers the precision smoke check at 12.5%, partially equivalent.
- `output_file_map_empty_sweep_mbias_only_is_noop` (V1) — not added.
- `output_file_map_empty_sweep_gzip_kept_file_seals_trailer` (V2 supporting) — not added; the existing `smoke_gzip_default_emits_12_gz_files_with_byte_identical_decompression` indirectly covers it.
- `extract_pe_parallel_vs_sequential_call_strings_parity` (V3) — not added.

**Recommendation:** No action required pre-merge — the test count is sufficient for first-pass safety net. Plan-manager (next stage in workflow) should track these as follow-up coverage debt and either close them as duplicated-by-existing or add them as a separate hardening PR. The most valuable missing test in my opinion is `extract_pe_parallel_vs_sequential_call_strings_parity` (V3) — it would catch a divergence between `pipeline.rs:275-276` and `parallel.rs:776-777` if either drifts in the future. Worth adding even as a one-off.

### L2 — `tests/output_phase_c2.rs:128-135` does NOT lock in absence-from-stdout for the C3 absorption

**File:** `tests/output_phase_c2.rs:127-135`
**Severity:** Low (the assertion is correct as written, but it's a substring check that would pass on a typo).

The test asserts `!stdout.contains("contains data ->\tkept")` and `!stdout.contains("was empty ->\tdeleted")`. If a future refactor accidentally routed to stdout, this would catch it.

However: the binary may emit other unrelated content to stdout (e.g. summary lines, the splitting-report mirror in §7.3's "deliberately not implemented" path that COULD get re-added). The test doesn't sanity-check that stdout has SOMETHING expected (or is empty), so a regression that adds noise to stdout could coexist with this passing.

**Recommendation:** No action required — the test catches the regression class it was designed for. Mentioning here so future readers know it's a one-sided check.

### L3 — `write_percent_or_fallback`'s `ctx_label` parameter accepts `&str` but only ever receives string literals

**File:** `rust/bismark-extractor/src/output.rs:432-457`
**Severity:** Low (minor API tightening opportunity, not a bug).

The four call sites pass `"CpG"`, `"CHG"`, `"CHH"`, `"non-CpG"` — all string literals. The `&str` signature accepts dynamic values, opening the door for a future refactor to pass through a user-controlled name string that wouldn't match Perl. The function is private (`fn` not `pub fn`), so the blast radius is small.

**Recommendation:** No action; the current signature is fine. Mentioning for completeness.

### L4 — Hardcoded `BISMARK_VERSION` const drifts across releases

**File:** `rust/bismark-extractor/src/output.rs:33`
**Severity:** Low (documented; flagged for awareness, not a Phase C.2 regression).

`BISMARK_VERSION = "v0.25.1"` is hardcoded and the doc comment notes "Update in lockstep with Perl `bismark_methylation_extractor` at release time." This is fine for now but constitutes drift risk for future releases.

**Recommendation:** No action for Phase C.2. Track as separate hardening item (e.g. extract `$version` from the Perl source at build time, or move to a single source-of-truth file shared by both).

---

## Findings the deliberate-omission list (plan §7.3) covered correctly

Plan §7.3 deliberately documented:
- Stderr-mirror of report (Perl `:2562-2580`) NOT implemented in Rust.
- Stderr `Final Cytosine Methylation Report` block via `warn` not mirrored.

I verified Rust does NOT emit these (good). The plan's rationale (nf-core tooling doesn't parse the mirror; file-level invariant is what matters) is sound.

## Verifications passed

- ✅ Line 1 of splitting report is bare basename via `path.file_name()` (output.rs:497-501).
- ✅ Conditional `Ignoring …` lines emit only when non-zero (output.rs:519-539). SE/PE branches use the right wording per Perl `:5006-5028`.
- ✅ `No overlapping methylation calls specified` emitted only when `config.no_overlap` (output.rs:554-556).
- ✅ Two blank lines between header and body via `\n\n` at step 12 (output.rs:578) — combined with last-header-line `\n` and step-13's body-start gives the 3 consecutive `\n` bytes verified by `tests/output_phase_c2.rs:191-195`.
- ✅ Methylated trio at lines 604-618 ends with `\n\n` on the CHH line (matches Perl `:2517`).
- ✅ Unmethylated trio at lines 622-636 uses `Total C to T conversions in {ctx} context:` phrasing (NOT `Total unmethylated`), ends with `\n\n` on the CHH line (matches Perl `:2519-2521`).
- ✅ Percentages at 1 decimal place via `format!("{pct:.1}")` (output.rs:451).
- ✅ Zero-denominator fallback string matches Perl `:2528` / `:2548` / `:2556` / `:2537` exactly (output.rs:441-445).
- ✅ 33 `=` chars on the separator (output.rs:594).
- ✅ Merge-non-CpG percentage block is 2 lines (CpG + non-CpG); non-CpG ends `\n\n\n` (output.rs:645-659).
- ✅ Per-context trailing newline variance handled by `write_percent_or_fallback(is_last)` (output.rs:439).
- ✅ STDERR routing via `eprintln!` for sweep log lines (output.rs:278, 280); regression-guarded by `tests/output_phase_c2.rs:128-135`.
- ✅ `records_written` counter bumps AFTER all `write_all` calls succeed in `write_call` (output.rs:206-212); partial-write failures propagate via `?` before the counter bump.
- ✅ Both PE call sites (pipeline.rs:275, parallel.rs:776) correctly `+= 1` for records_processed and `+= 2` for call_strings_processed.
- ✅ Both SE call sites (pipeline.rs:166-167, parallel.rs:649-650) correctly `+= 1` for both counters.
- ✅ `is_paired: bool` parameter correctly routed from `ExtractState::is_paired` (state.rs:129), not `config.paired_mode`.
- ✅ `cleanup_all` correctly destructures the new `OutputFileEntry` and drops writer before unlink (output.rs:301-328).
- ✅ Harness `*.gz)` arm uses `zcat | LC_ALL=C sort | md5sum` (scripts/oxy_phase_h_smoke.sh:219-229); the PASS verdict counts SORTED matches as success (scripts/oxy_phase_h_smoke.sh:249-254).
- ✅ Cargo.toml version bumped to `1.0.0-alpha.8`.

---

## Pre-merge checklist (for the implementer)

| Item | Status | Action |
|---|---|---|
| C1 (--yacht report divergence) | OPEN | Fix before merge — 3 byte-identity divergences in supported mode. Add `splitting_report_format_yacht_mode` unit test. |
| H1 (basename vs full path on stderr) | OPEN | Decision: match Perl (full path) or document as deliberate divergence. Update test if changing. |
| H2 (MbiasOnly trailing `\n\n` on stderr) | OPEN | Guard `finalize_with_empty_sweep` call in `state.rs` behind `!self.mbias_only`. |
| M1 (CRLF rationale) | OPEN | Update doc comment to drop CRLF claim. |
| M2 (SPEC §8.4 stale row) | OPEN | Update Directional library fixture row. |
| M3 (Deleting preamble) | OPEN | Add `Deleting unused files ...\n` preamble OR document deliberate omission. |
| M4 (first-failure aborts sweep) | DECIDE | Best-effort vs propagate. |
| M5 (parallel.rs:772 stale comment) | OPEN | Drop first line of the comment block. |
| L1 (test count gap) | TRACK | Plan-manager follow-up; add V3 parity test as one-off. |

All Critical + High should be addressed before PR submission. Mediums are nice-to-have polish; flag them in PR description for reviewer awareness.

---

## Conclusion

The Phase C.2 implementation is **substantially complete and correct** for the SE / PE / merge-non-CpG / comprehensive modes. The 4 Critical absorptions (C1-C4) from the dual plan-review are faithfully implemented; the 11 Important items are addressed; clippy + fmt + 236 tests are green. The implementation is honest about its deviations from the plan (e.g. the test-count reduction is justified).

The one finding that warrants a blocking fix is the **--yacht mode report divergence** (C1 above): Perl sets `$full = 1` AND `$merge_non_CpG = 1` for yacht, and the Rust report-emit path doesn't reflect that. This produces 3 byte-identity-divergent rows in the splitting report under `--yacht`. It is fix-locally-scoped (touch the 3 `match` sites in `write_splitting_report` plus add a unit test).

The other High findings (H1 basename vs path, H2 MbiasOnly trailing blank lines) are stderr-format polish that matters for nf-core / log-parsing tooling but does not affect on-disk file correctness. Implementer can choose to fix in this PR or defer to a hardening PR with clear documentation of the divergence.

Recommend: fix C1 + H2 in this PR (they're tiny); decide H1 by checking if any downstream tool actually parses the kept/deleted log lines (basename is the safer-on-CI choice; full-path is the Perl-byte-identity choice); land medium-priority items in a follow-up unless they're trivial.

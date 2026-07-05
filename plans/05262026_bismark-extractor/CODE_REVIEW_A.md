# Code Review A — Phase E (output mode dispatch + `--gzip` + `--mbias_only`)

**Reviewer:** A (fresh context window, independent review)
**Plan reviewed against:** `plans/05262026_bismark-extractor/PHASE_E_PLAN.md` rev 1.
**Branch:** `extractor-phase-e`.
**Scope:** sources under `rust/bismark-extractor/{src,tests}` + `Cargo.toml` + `SPEC.md` deltas listed in the implementer's brief.

## Summary

The implementation is solid. The big-ticket items — mode-discriminated `OutputKey`, mode-keyed eager-open, yacht reverse-strand col-6/col-7 polarity (Critical-1 regression guard), `mbias_only_silence` narrow catch arm, and the centralised `is_mbias_only()` predicate — all land cleanly and match the plan and Perl source. Tests pass (201/0); clippy + fmt are clean; `cargo tree` confirms a single `flate2 v1.1.9`.

Verifications I performed independently:

- **Yacht col-2 / col-8 semantics** vs Perl `:1604-1613, :4472, :4485, :4498, :4511, :4524, :4537, :4572`. Col-2 is hard-coded `+`/`-` per XM-branch (methylated vs unmethylated); col-8 is the strand sigil derived from XR/XG. The Rust mapping `orient_byte(OT|CTOB) = +` and `orient_byte(OB|CTOT) = -` matches Perl's `$strand` value at lines 1604, 1607, 1610, 1613. `write_yacht_row` uses `call.methylated` for col-2 and `orient_byte(pair_strand)` for col-8 — distinct sources as required.
- **Yacht col-6/col-7 polarity** at `route.rs:108-111`: forward-class `(alignment_start, ref_end)`, reverse-class `(ref_end, alignment_start)`. Mirrors Perl `:4350, 4382, 4422-4447` byte-for-byte. The new unit test `write_yacht_row_reverse_strand_swaps_col6_col7` and the smoke `smoke_yacht_emits_1_file_with_8_col_rows_and_reverse_strand_swap` together gate this.
- **`mode_keys` ordering** vs Perl `:5082-5403`: Default (CpG → CHG → CHH, each with OT/CTOT/CTOB/OB), Comprehensive (CpG/CHG/CHH), MergeNonCpG (CpG×4 → Non_CpG×4), ComprehensiveMergeNonCpG (CpG → Non_CpG), Yacht (1). All match.
- **`is_mbias_only()` plumbing**: three derivation sites consult the predicate — `state.rs:79`, `pipeline.rs:144` (SE), `pipeline.rs:317` (PE). `OutputFileMap::new` reads `config.output_mode` and uses `mode_keys(MbiasOnly, ...) = Vec::new()` to skip eager-open. Cannot drift.
- **`mbias_only_silence` narrow catch arm** at `call.rs:184` correctly destructures `BismarkExtractorError::InvalidXmByte { .. }` only; any other variant continues to propagate.
- **`flate2 = "=1.1.9"`** — `cargo tree -p bismark-extractor | grep flate2` shows exactly one entry at 1.1.9. The pin matches the workspace transitive resolution.
- **SPEC.md fixes** at §4.1 lines 92 and 94: both Comprehensive rows now read `CpG_context_{input}.txt`. ✓

No Critical findings; no High-severity findings.

## Issues

### Critical — none.

### High — none.

### Medium

#### M1 — `cleanup_all` removes file BEFORE writer drops; comment is wrong about the order

`output.rs:205-220`:

```rust
let entries: Vec<_> = self.files.drain().collect();
for (_, (path, _writer)) in entries {
    // `_writer` drops here; file handle closes; gzip footer (if
    // any) is written into the closing file. Then we delete the
    // file outright — the in-flight gz state doesn't matter.
    if let Err(e) = std::fs::remove_file(&path) {
        ...
    }
}
```

`_writer` (underscore-prefixed, not bare `_`) is a real binding whose lifetime spans the entire iteration scope and drops at the closing brace. So the actual order is:

1. Bind `path` + `_writer` (writer is moved into the binding, still open).
2. `remove_file(&path)` runs while the writer is still open.
3. End of iteration → `_writer` drops → gzip footer is flushed into a file that has already been unlinked from the directory.

On Unix this is benign (the inode lives until last fd closes), but the inline comment claims the writer drops first and the delete runs second. **The implementation behaves correctly; the doc comment misrepresents the order.** On Windows the unlink-before-close would fail, but Bismark targets Unix.

**Recommendation:** fix the comment, OR explicitly drop the writer before the unlink (`drop(_writer); std::fs::remove_file(&path)`) so the comment and the code agree. Low-risk fix:

```rust
for (_, (path, writer)) in entries {
    drop(writer); // close handle (writes gzip footer if applicable)
    if let Err(e) = std::fs::remove_file(&path) { ... }
}
```

#### M2 — Stale module docstrings in `lib.rs` and `main.rs` claim Phase D / "non-default modes still rejected"

- `lib.rs:9-21` still calls the crate "Phase D" and pins the version to `1.0.0-alpha.4`, even though `Cargo.toml` is `1.0.0-alpha.5` and Phase E unlocks all five non-Default modes + `--gzip`.
- `main.rs:10-12` says "Non-default output modes, `--gzip`, ... are still rejected with `PhaseNotYetImplemented`" — false after this PR; the body at `main.rs:65` correctly notes Phase E supports them.

These are doc-only but materially misleading to anyone navigating the crate. Recommend bumping the lib.rs Status block to Phase E (alpha.5) and rewriting the main.rs reject list to mention only Phases F (multicore), G (bedGraph/cytosine_report), and multiple-input-files.

#### M3 — Test-file duplication of BAM helpers (acknowledged in code, flagged for follow-up)

`output_modes_phase_e_smoke.rs` lines 46-178 duplicate ~130 lines of synthetic-record/header helpers already present in `se_phase_b_smoke.rs:38-78`. The file docstring acknowledges this and defers a `tests/common/mod.rs` refactor. Reasonable for this PR (the refactor would touch every smoke file simultaneously and complicate Phase B/C/D PR rebases), but worth tracking as a tidy-up item once the stacked PRs settle.

### Low

#### L1 — `cli.rs:78` claims "Mutex with --yacht" for `--merge_non_CpG`, but validate doesn't enforce mutex

```rust
/// Collapse CHG + CHH into one "non-CpG" output.
/// Output count: 8 (or 2 with --comprehensive). Mutex with --yacht.
```

`Cli::validate` only rejects `--yacht --paired-end` and `--yacht --mbias_only`. `--yacht --merge_non_CpG` is silently absorbed (Yacht takes precedence in the `if/else` chain at `cli.rs:441-452`). This is semantically fine (Yacht "forces comprehensive + merge_non_CpG" per the cli.rs:83 comment on yacht itself), but the merge_non_cpg comment is misleading. Either tighten validate to reject the combo, or correct the comment to "absorbed by --yacht".

#### L2 — Skipped smoke test `smoke_gzip_cleanup_on_write_failure_removes_gz_files`

Plan §7.2 rev 1 lists this smoke. The implementer skipped it with a documented justification (portable I/O-fault injection is flaky). The alternative coverage cited (empty-map cleanup unit + byte-identity round-trip) does NOT actually exercise `cleanup_all` against a populated gz map, so the gzip-footer-on-drop-during-cleanup path remains unexercised.

This is a reasonable deviation — `/dev/full` is Linux-only and `tempfile`-based read-only-dir injection mid-run is fragile — but the **stated alternative coverage doesn't fully substitute**. Consider a constructed-state unit test that builds an `OutputFileMap` in Yacht+gzip mode, writes one row, then calls `cleanup_all` and asserts the file is gone; that hits the same drop+unlink path without needing I/O-fault injection.

#### L3 — `BISMARK_VERSION` and `SPLIT_FILE_HEADER` are still hardcoded "v0.25.1"

Not a Phase E regression (Phase B baked these in), but Phase E doubles the surface area touched by the constant (yacht's header line, every gzip output's first line). Phase H byte-identity will fail loudly if the Bismark Perl version bumps without these being updated. Already tracked elsewhere; flagging for awareness given Phase E expands the blast radius.

## Things checked and confirmed fine

- **`route.rs` u32 overflow guards** with `try_from` + `InternalError` propagation (Reviewer A G1 in plan): both `alignment_start_usize → u32` and `ref_end_usize → u32` are guarded.
- **`alignment_start.ok_or_else`** correctly raises `InternalError` rather than silently producing 0 (Reviewer A V4 / plan §4.6 row).
- **`write_yacht_row` zero-alloc** — writes byte-at-a-time with `write_all` calls. No `Vec<u8>` per row.
- **`route_to_key` returns `None` for `MbiasOnly`** and `write_call` short-circuits with `Ok(())` — defensive belt-and-braces given route_call already short-circuits at `route.rs:74-76`.
- **`OutputFileMap` for `MbiasOnly` still calls `create_dir_all`** so M-bias.txt + splitting-report have a destination. ✓
- **`extract_calls` `Err` match-arm narrowing** to `InvalidXmByte { .. }` only — other future error variants still propagate. ✓
- **`is_mbias_only()` field on Cli vs predicate on ResolvedConfig**: `Cli` (clap struct) has the raw `mbias_only: bool` field; `ResolvedConfig` only exposes the derived `is_mbias_only()` method (no shadow `mbias_only: bool` field), so there's no second source of truth to drift against.
- **flate2 pin** at `=1.1.9` (not `=1.0.34` as the plan tentatively listed) is confirmed correct by `cargo tree`.
- **Yacht orientation byte `+` for OT/CTOB and `-` for OB/CTOT** matches Perl `:1604, 1607, 1610, 1613` exactly.
- **All 201 tests pass; `cargo clippy --all-targets -- -D warnings` is clean; `cargo fmt --check` is clean.**

## Recommendations (priority-ordered)

1. **M1** — fix `cleanup_all`'s comment OR insert an explicit `drop(writer)` before `remove_file` so order matches the comment. (Low-risk, ~3 lines.)
2. **M2** — refresh `lib.rs` Status block and `main.rs` module docstring to Phase E / alpha.5. (Doc-only, ~10 lines.)
3. **L1** — fix the `cli.rs:78` "Mutex with --yacht" comment OR enforce the rejection. (Doc-only or 3-line validate rule; pick one.)
4. **L2** — consider a constructed-state unit test for `cleanup_all` over a populated gzipped map, even if portable I/O-fault injection isn't viable.
5. **M3** — track the smoke-test BAM-helper duplication as a follow-up PR once the stacked Phase B-E PRs settle.

## Verdict

**Ready to merge after M1 + M2 doc fixes.** No correctness bugs found. The Critical-1 regression guard (yacht reverse-strand polarity) is well-tested at both unit (`write_yacht_row_reverse_strand_swaps_col6_col7`) and smoke (`smoke_yacht_emits_1_file_with_8_col_rows_and_reverse_strand_swap`, `smoke_yacht_gzip_emits_1_gz_file_with_reverse_strand_swap_after_decode`) levels. The `mbias_only_silence` narrow catch arm matches Perl's silent-skip semantics exactly. The mode-discriminated `OutputKey` design keeps the per-call hot path branch-free except for the mode-match in `route_to_key`. Phase H byte-identity should pass for all five output modes with no further structural rework.

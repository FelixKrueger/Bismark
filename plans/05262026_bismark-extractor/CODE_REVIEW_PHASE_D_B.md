# Code Review — Phase D (`bismark-extractor`) — Reviewer B

**Branch:** `extractor-phase-d` (stacked on Phase C / PR #851)
**Plan:** `PHASE_D_PLAN.md` rev 1
**Crate version:** `1.0.0-alpha.3` → `1.0.0-alpha.4`
**Scope reviewed:** `mbias_writer.rs` (new), `mbias.rs`, `state.rs`, `pipeline.rs`, `lib.rs`, `Cargo.toml`, `SPEC.md` (3 prose fixes), `tests/mbias_writer_phase_d{,_smoke}.rs` (new), `tests/se_phase_b{,_smoke}.rs` (signature ripple + count assertion).
**Local validation:** `cargo test -p bismark-extractor` → 154 tests pass (29 + 2 + 4 + 44 + 3 + 40 + 26 + 3 across binaries/integration files, matching the user's "566 tests across 3 crates" gate). `cargo clippy --workspace --all-targets` clean. `cargo fmt --check` clean.

---

## Summary

Phase D implements the `M-bias.txt` writer that consumes the `[MbiasTable; 2]` accumulator that Phases B and C have been populating. The writer is byte-identity-targeted at Perl `bismark_methylation_extractor:628-836` (the `produce_mbias_plots` sub). The implementation is tight, well-isolated, and the test coverage is thorough — both unit-level (writer-only) and end-to-end (binary on a synthetic BAM through AutoDetect). Three retroactive SPEC.md prose corrections are also rolled in. The crate version bump is the only `Cargo.lock` delta. Verdict: **APPROVE-WITH-NITS** — no blockers, two Medium items worth tracking for Phase E, and a handful of Low-priority docstring/test-clarity nits.

---

## Logic

### Finalize order: `flush → splitting_report → M-bias.txt` matches Perl

Cross-referenced Perl source:
- `:2463` calls `print_splitting_report()` from inside `process_X_read_file`
- `:316` (after that sub returns) calls `produce_mbias_plots($filename)`

The rev-1 C1 fix that swapped the rev-0 plan's `M-bias.txt → report` order is the correct call. The "Phase B SE smoke" question raised in the review brief: `smoke_se_directional_produces_all_12_files_and_report` (`tests/se_phase_b_smoke.rs:166`) does NOT assert a directory-entry count — only that the 12 split files and the splitting report exist with expected content. The order swap doesn't affect those assertions; the test still passes for the correct reason. The dir-count assertion lives in the sibling `smoke_se_empty_bam_writes_only_header_files` (`tests/se_phase_b_smoke.rs:312`), which was updated to expect `14` entries (12 split files + 1 report + 1 M-bias.txt). Cross-checked the diff: the bump is exactly `13 → 14`, comment is accurate. ✓

### `MbiasTable::max_position() == 0` edge — writer handles empty range correctly

Rust's `1..=0` is empty (`start > end`). When `max_position == 0` the for-loop at `mbias_writer.rs:196` runs zero iterations, so each section emits exactly: header line + equals-rule + column header + trailing `\n`. Test `write_mbias_txt_empty_mbias_emits_headers_only` (lines 294-303) covers this and additionally negative-asserts `!content.contains("\n1\t")` and `!content.contains("\n2\t")`. A regression that accidentally emitted a row at position 1 would be caught. ✓

### Overlap-dropped R2 calls do NOT accumulate to mbias[1]

The brief's hypothesis is confirmed. `pipeline.rs:312-326`: `drop_overlap` filters `r2_calls_raw` BEFORE the `for call in r2_calls { route_call(...) }` loop. `route_call` is the only path that calls `state.mbias[1].accumulate(...)`. Therefore in the PE smoke test (1 pair, R2's `z` at ref_pos 104, R1's r1_ref_end == 104, strict-`<` keep), R2's only call is dropped and `mbias[1]` is entirely empty.

The PE smoke test `smoke_mbias_pe_auto_detect_produces_pe_format_mbias_txt` (lines 161-229) asserts only the presence of section headers, not row content — so its assertions remain true. The structural correctness of "PE → 6 sections regardless of mbias[1] populated-ness" is the load-bearing claim, and the `is_paired` field threading enforces it. ✓

**Medium: PE smoke fixture could be strengthened.** A 1-pair smoke where R2 is fully overlap-dropped doesn't actually exercise the R2-routing-into-mbias[1] path through the binary entrypoint. Phase C's unit test `route_call_r2_goes_to_mbias_index_1` covers the unit-level behavior, but the binary smoke leaves an end-to-end gap: a regression that wrongly routed R2 to mbias[0] (e.g. via a `ReadIdentity::R2 → 0` bug) wouldn't be caught by the current smoke fixture. Suggest: in Phase E or as a smoke addition, construct a 2-pair PE fixture where R2 falls inside R1's span (so a call survives overlap), and assert a per-position row in an R2 section.

### `is_paired` threading vs `mbias[1]` empty-check inference

The state.rs:33 doc-comment explicitly justifies threading the bool: "an empty PE BAM would yield empty `mbias[1]` and get misclassified as SE." Correct. The field's only consumer is `state.finalize` (line 117). The brief asks whether the field could be replaced by a `finalize(config, is_paired)` parameter from the caller. Yes — it could; the field is purely a deferred boolean. Both designs are valid; the current field-on-state choice is consistent with the existing `mbias_only` field on the same struct. Phase E (output-mode dispatch) is the natural place to revisit this if the dispatch shape changes.

**Low: alternative signature available for Phase E.** Document in `PHASE_E_PLAN.md` (when written) that `is_paired` can move from `ExtractState` field to `finalize` parameter if the caller naturally has the bool at finalize-time. Not a Phase D issue.

### `derive_mbias_basename` strip-each-once semantics — Perl-faithful

I traced both Perl (lines 633-637, five sequential `s/X$//` substitutions) and Rust (the `for suffix in [...]` loop, each `strip_suffix` runs once and replaces `s` if it matches). Verified for the inputs in the brief:

- `sample.bam.gz`: `gz` strips → `sample.bam.`; subsequent attempts (`sam`, `bam`, `cram`, `txt`) all check the new tail ending in `.` and don't match. Final: `sample.bam.`. ✓
- Hypothetical `foo.txt.bam`: `gz` no; `sam` no; `bam` strips → `foo.txt.`; `cram` no; `txt` checks tail ending in `.` (not `txt`) → doesn't match. Final: `foo.txt.`. **The brief's claim that "both bam and txt would strip" is incorrect** — after `bam` peels to `foo.txt.`, the trailing dot prevents the `txt` regex from matching. Rust's `strip_suffix` semantics match Perl's `s/txt$//` byte-for-byte (both require literal `txt` at end-of-string).
- Hypothetical `foo.bam.txt`: only `txt` matches (last in chain), yielding `foo.bam.`.

The behavior is Perl-faithful, but the docstring at `mbias_writer.rs:35-48` could state this property more explicitly. Currently it says "5 sequential strip attempts ... each attempt is run exactly once" — which is correct but doesn't flag the "trailing dot acts as a stop" property. A reader might initially expect `foo.txt.bam` to peel both.

**Low: strengthen `derive_mbias_basename` docstring.** Add a note: "The trailing `.` left after each strip prevents subsequent same-style strips from matching, because Perl's `s/X$//` requires a literal `X` at end-of-string. Net result: each input strips at most one suffix per `.`-bounded segment." And add a test for the `.txt.bam` case to lock the semantic against a future reader who "fixes" the loop to be `loop { let stripped = ...; if stripped == s break; s = stripped }`.

### `#[cfg(debug_assertions)]` gating on the panic test

`tests/mbias_writer_phase_d.rs:133-141`:
```rust
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "position must be 1-based")]
fn mbias_accumulate_position_zero_debug_panics() { ... }
```

`#[cfg(debug_assertions)]` conditionally compiles the entire item, so in `cargo test --release` the function doesn't exist (no compile, no run). The `debug_assert!` inside `accumulate` is also a no-op in release, so without the `cfg` gate the `should_panic` would *fail* in release. The current gating is the correct pattern. ✓

---

## Efficiency

- `BufWriter::with_capacity(8 * 1024, ...)` is fine — M-bias.txt is small (max ~300 positions × 5 cols × 6 sections ≈ 50 KB on a wide PE read). Plenty of buffer.
- `MbiasTable::max_position()` does three `len().saturating_sub(1)` reads and two `max()` calls — O(1), zero allocation. ✓
- `writeln!` to a `BufWriter` is the right pattern (no intermediate `String` allocation).
- No `clone()`s or `to_string()`s in the per-position hot loop.

No efficiency issues.

---

## Errors

- `write_mbias_txt` propagates `std::io::Error` via `?`; the caller wraps it as `BismarkExtractorError::IoWrite` (verified in `state.rs:117` via the `?` operator on `Result<(), io::Error>` returned from `write_mbias_txt`).
- The `state.finalize` docstring's "die-after-writing" invariant is preserved by ordering (`flush_all` → report → mbias). A disk-full during M-bias.txt write leaves the report on disk for diagnostics, matching Perl.
- `derive_mbias_basename` panics on a missing filename component via `.expect("input path must have a filename component")`. The pipeline guarantees a real file path is passed (CLI arg validation in `cli.rs`). Acceptable — would only fire on programmer error.

No error-handling issues.

---

## Structure / Naming / Style

- Module split (`mbias_writer.rs` separate from `mbias.rs`) is clean — accumulator stays in `mbias.rs`, formatter in `mbias_writer.rs`. ✓
- `ReadIdentitySection` enum with variants `R1OrSe { is_paired: bool }` + `R2` is a clear, lossless model of the section-header bytes.
- Public API (`derive_mbias_basename`, `mbias_txt_path`, `write_mbias_txt`) reexported from `lib.rs:62`. Consistent with how other modules expose their public surface. ✓
- The cross-reference between `pipeline::derive_basename` and `mbias_writer::derive_mbias_basename` (with the explicit "differs from" docstring on each) is excellent — exactly the kind of land-mine that would burn a future maintainer otherwise. The `derive_basename_vs_derive_mbias_basename_lock_divergence` test pins the divergence behaviorally. ✓
- `context_name` private helper is fine; could equivalently be an `impl Display for CytosineContext`, but the local helper keeps the module self-contained.

---

## Tests

- 22 new unit tests in `mbias_writer_phase_d.rs`. Coverage matrix: filename derivation (incl. divergence-pin), `max_position` (incl. slot-0-only and debug-panic), section headers SE vs PE, byte-exact column header, per-position rows (with data, zero-coverage, midpoint precision), blank-line-between-sections, empty-mbias header-only, PE-with-empty-R2.
- 3 new smoke tests in `mbias_writer_phase_d_smoke.rs`: SE format, PE-via-AutoDetect format, `--mbias_off` suppression. Each runs the `assert_cmd`-launched binary.
- Pre-existing `se_phase_b.rs` tests updated for the new `ExtractState::new` signature (4 call sites). Cleanly mechanical.
- The new file `mbias_writer_phase_d_smoke.rs` (vs extending existing smoke files) is justified by review-hygiene — Phase B/C PRs are in flight and modifying their test files would create churn. Sound reasoning.

**Low: smoke fixture realism.** Per the previous "PE smoke" note, the PE smoke uses 1 pair where R2 is entirely overlap-dropped. Pure unit-test coverage for the R2 → mbias[1] path exists (`route_call_r2_goes_to_mbias_index_1`), but binary-level coverage is essentially "PE → 6 sections, R2 sections empty." Strengthening this to "PE with overlap-surviving R2 → 6 sections, R2 section has at least one row" would be a cheap, more rigorous gate.

---

## Cross-crate impact

- `Cargo.lock` diff: ONLY the `bismark-extractor` version bump (`1.0.0-alpha.3 → 1.0.0-alpha.4`). No transitive churn. ✓
- `bismark-io` and `bismark-dedup` not touched.
- The `bismark_io::BamWriter` + `BismarkRecord` API surface used by the new smoke is the same as Phase C's smoke — no compat surprises.

---

## SPEC.md prose edits

Three edits total (grep `^+` excluding `+++` yields exactly three added lines):

1. **§4.2 (M-bias outputs)** — corrects "4-col" to "5-col" with `coverage` as the 5th column; cites Perl `:729`. Spot-checked Perl line 729 — exact column header and per-position format match. ✓
2. **§7.4 (paired-overlap edge case)** — corrects "no-op" to "drops all R2 calls downstream of `r1_ref_end`"; cites Perl `:2905-2906`. Spot-checked Perl `:2905-2906` — early-exit `return` on `$start+$index+$pos_offset >= $end_read_1` confirmed. ✓
3. **§8.4 (?) directional-library row** — corrects "0-byte or absent" to "exists with version header line"; cites Perl `:5405-5700+` + `:5140-5325`. Spot-checked Perl `:5400-5420` — eager open of all 12 strand-context FHs + immediate version header write, guarded only by `$mbias_only` / `$no_header`. ✓ The line-range cite is broad but the prose claim is accurate.

All three edits cite Perl line numbers, accurately describe current code, and don't introduce new prose errors. ✓

**Low: convention persistence.** Plan §16 establishes "fix SPEC prose errors in the same PR that surfaces them" as a going-forward convention. Codifying it in `CLAUDE.md` (the project-level instructions doc) would persist beyond Phase D's PR cycle. Optional — not a Phase D blocker.

---

## Fixes applied

None — this is a read-only review.

---

## Prioritized recommendations

### Critical
None.

### High
None.

### Medium
1. **Strengthen PE smoke fixture** so the binary-level test actually exercises an R2 call surviving overlap detection and routing to `mbias[1]`. A 2-pair PE fixture where R2 falls inside R1's span would do it. (Defer to Phase E if scope-creep concerns; track as a known-gap.)

### Low
1. **Tighten `derive_mbias_basename` docstring** to explicitly note the trailing-dot stop semantic for chained-extension inputs (`.txt.bam`, etc.), and add a test case for `foo.txt.bam` / `foo.bam.txt` to lock the behavior.
2. **Document `is_paired` field alternative** in the eventual `PHASE_E_PLAN.md`: the field could move to a `finalize(config, is_paired)` parameter if the dispatch shape supports it.
3. **Codify SPEC-fix convention in `CLAUDE.md`** so the "fix SPEC errors in the surfacing PR" rule survives beyond this branch's review cycle.
4. **Tighten Perl line-number cite in §8 directional-library SPEC edit.** "Perl `:5405-5700+`" is a 300-line range; narrowing to the actual `open`+`print` block (~5405-5430 for the default-mode path) would help future readers.

---

## Verdict

**APPROVE-WITH-NITS.**

Justification: implementation is correct, byte-identity-targeted at Perl, test coverage is thorough at both unit and smoke levels, finalize ordering is verified against Perl source, all three SPEC prose edits are accurate. No correctness blockers. Findings are all polish-grade: one Medium (smoke-fixture realism for R2 mbias path) and four Lows (docstring clarity, Phase-E forward planning, convention codification, SPEC line-cite precision). None block merging; all are appropriate as follow-up tickets or rolled into Phase E.

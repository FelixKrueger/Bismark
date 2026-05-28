# Code Review B — PR #880 (closes #879)

**Reviewer:** B (independent of A)
**Branch:** `extractor-fix-879` @ `f96f9c6` off `rust/iron-chancellor` @ `45b4c61`
**Scope:** 4 commits, 8 files (+605 / -19). Verified via `git diff rust/iron-chancellor..extractor-fix-879`.

---

## Summary

The fix correctly addresses #879: `drop_overlap` no longer uses R1's un-clipped CIGAR end/start when `--ignore_3prime N` shifts R1's effective boundary inward. The new CIGAR-trim primitive at `bismark-io/src/cigar.rs:255-308` plus the two helpers (`reference_end_after_3p_trim`, `reference_start_after_3p_trim`) faithfully mirror Perl `bismark_methylation_extractor:1756-1803`. By hand I traced 5 representative CIGARs (`90M5D` n=5, `90M5D5M` n=5, `5D90M` n=5, `5S95M` n=5, `90M5N5M` n=10) — all match Perl's expanded-@comp_cigar pop/shift semantics including the "D/N stripped only when adjacent to trimmed boundary post-pop" rule. fmt + check clean. All 178 bismark-io tests + full workspace test suite pass (0 failures, 0 ignored except 9 pre-existing).

**Verdict: APPROVE.** No Critical or High findings. One Medium and three Low items below.

---

## Issues by area

### Logic — no defects found

- **`walk_trim_from_right` / `walk_trim_from_left`** (cigar.rs:325-393): single-sweep semantics correctly absorb D/N at the trimmed boundary while preserving middle D/N. The exit condition `while remaining > 0` correctly stops without over-stripping when the boundary lands exactly on a read-consuming op (e.g., `90M5D5M` n=5 → `90M5D`, matching Perl which only fires the inner D-strip when an outer pop returned D).
- **`reference_start_after_3p_trim`** (cigar.rs:300-308): the `original_ref_span - trimmed_ref_span` formula is algebraically equivalent to Perl L1803's `ignore_3prime + D + N - I`. Verified for the `5I90M` (pure-insertion prefix → 0 shift), `5D90M` (D-only prefix → 10 shift), and `5S95M` (soft-clip prefix → 0 shift) cases.
- **`drop_overlap`** signature change (overlap.rs:84) + both call sites (pipeline.rs:354, parallel.rs:711) consistently pass `config.ignore_3p_r1`. `grep -rn drop_overlap rust/bismark-extractor/src/` confirms no third call site exists.
- The Strict-`>` (OT) and Strict-`<` (OB) keep predicates are preserved; the fix only changes the boundary value, not the predicate polarity (Phase C.1 polarity fix from #862 stays intact).

### Efficiency

- **Allocation on no-op fast-path** (cigar.rs:259-261): when `n == 0`, `trim_3p_read_positions` clones the ops vec via `to_vec()`, then wraps in `Cigar::from`. For the default cell (no `--ignore_3prime`), this is an unnecessary heap allocation per record. **However** — and this saves the day — `reference_end_after_3p_trim` calls the trim before `reference_end`, but `reference_start_after_3p_trim` (overlap.rs OB branch) does have an `n == 0 → return start` short-circuit at cigar.rs:301-303. The OT branch (`reference_end_after_3p_trim`) does NOT short-circuit at the helper level — it always trims. The docstring acknowledges this with "the allocation cost is irrelevant since this branch is hit on every default-cell record" but I'd flag this as a real (if small) perf concern: ~55M PE reads × one extra `Vec<Op>` clone per record. See Medium-1.

### Errors — no defects found

- Full-clip case (`n >= read_span`): `walk_trim_from_right` returns `(0, 0, None)`, producing an empty Cigar. `reference_end` of empty Cigar returns `start` per the pre-existing convention at cigar.rs:248. Test `reference_end_after_3p_trim_full_clip_returns_start` guards this.
- Empty-CIGAR input: outer `while kept_end > 0` immediately exits, returning an empty Cigar — graceful.
- `boundary_remaining` with `surviving_len == 0`: not reachable, because the branch is only taken when `op_len > remaining`, i.e., `surviving_len ≥ 1`.

### Structure / API

- The new trait methods follow the existing `CigarExt` naming (`reference_end` → `reference_end_after_3p_trim`); signatures consistent (`start: usize, n_read_positions: u32` matches the `start` parameter style of `reference_end`).
- Docstrings accurately describe the post-fix single-sweep semantics. The earlier "Phase 2" wording is absent. Cross-references to Perl line numbers + the issue # are present and accurate.
- The 12 existing call-site updates (`drop_overlap(r2_calls, &pair) → drop_overlap(r2_calls, &pair, 0)`) all preserve original test intent (`0` = no clipping = same boundary as before). Spot-checked `drop_overlap_with_r1_indel_uses_reference_end`, `drop_overlap_r1_with_n_skip_op`, `drop_overlap_r1_with_5prime_soft_clip` — all still test what their names claim.

---

## Recommendations

### Medium

1. **Skip the clone on `n == 0` in `reference_end_after_3p_trim`** (cigar.rs:295-298). Add an early return:
   ```rust
   fn reference_end_after_3p_trim(&self, start: usize, n_read_positions: u32) -> usize {
       if n_read_positions == 0 { return self.reference_end(start); }
       self.trim_3p_read_positions(n_read_positions, false).reference_end(start)
   }
   ```
   Mirrors `reference_start_after_3p_trim`'s existing short-circuit and removes a `Vec<Op>` allocation per record on the default cell (~55M allocations/run on the full PE dataset). Optional but cheap and symmetric.

### Low

2. **Test naming inconsistency** (pe_phase_c.rs:1505-1656): the new module is `ignore_3prime_879` but the existing call-site-updated tests live in the top-level scope. Mostly stylistic. If you want all `--ignore_3prime` tests in one module, move them — but this isn't worth churning the 12-line-update diff.

3. **`read_consumes` naming** (cigar.rs:397) returns `usize` but only ever returns 0 or 1. Could just return `bool`. Tiny readability nit.

4. **Missing edge-case test: I+D+N mixed prefix** for `reference_start_after_3p_trim`. Current tests cover M-only, D-only, S-only, and zero. A mixed prefix like `3I2D90M` (with n=3 from left) would exercise the I-vs-D-vs-N interaction in one test. The math works (`original_ref_span = 92`, `trimmed_ref_span = 90`, shift = 2; Perl: `3 + 0 + 0 - 3 + ... wait`) — actually this case is non-trivial because Perl's L1781-1793 walks I via op_len=1-per-iter while D is absorbed for free. Verify by trace, then add the test. Optional defensive cover.

---

## What I verified by hand

| CIGAR | n | side | Rust result | Perl trace result | Match |
|---|---|---|---|---|---|
| `90M5D` | 5 | right | `85M` | `85M` | ✓ |
| `90M5D5M` | 5 | right | `90M5D` | `90M5D` | ✓ |
| `90M5D5M` | 10 | right | `85M` | `85M` | ✓ |
| `5D90M` | 5 | left | `85M` | `85M` | ✓ |
| `5S95M` | 5 | left | `95M` (no shift) | matches | ✓ |
| `5I90M` | 5 | left | `90M` (no shift) | matches | ✓ |
| `90M5N5M` | 10 | right | `85M` | `85M` | ✓ |
| `100M` | 100 | right | empty Cigar, end=start | matches | ✓ |

---

## Tests claim verification

- 17 `cigar.rs` unit tests for new methods (lines 608-770): ✓ counted.
- 4 integration tests in `pe_phase_c.rs::ignore_3prime_879` (forward_pair, reverse_pair, zero_is_no_op, at_boundary): ✓ present.
- `trim_3p_middle_D_is_NOT_stripped` (R2 finding): ✓ present at cigar.rs:688.
- `trim_3p_left_with_soft_clip_prefix` (R2 finding): ✓ present at cigar.rs:702.
- Workspace test run: `test result: ok. 178 passed; 0 failed` for bismark-io + all extractor/dedup suites green. ✓

---

## Cross-crate dep audit

`grep -l bismark-io rust/*/Cargo.toml` → only `bismark-extractor` and `bismark-dedup`. Both Cargo.tomls bumped to `=1.0.0-beta.8`. No other dependents (verified by `Cargo.lock` semantics via the workspace inheritance; the two listed crates are the only consumers of `bismark-io` workspace path). ✓

---

**Report path:** `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/CODE_REVIEW_879_B.md`

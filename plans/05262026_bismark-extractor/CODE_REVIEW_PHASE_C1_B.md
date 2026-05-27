# Code Review — Phase C.1 (`drop_overlap` polarity fix, closes #862) — Reviewer B

**Branch:** `extractor-phase-c1` (uncommitted working tree).
**Scope:** SPEC §7.4 rev 3 rewrite + 4-LOC code flip in `src/overlap.rs` + 11 test edits + 5 new regression-guard tests + smoke-fixture rework + `Cargo.toml` `alpha.6 → alpha.7` bump.
**Verdict:** **APPROVE.** No Critical or High findings. Two Medium and three Low findings, all non-blocking and confined to documentation/comments (not behaviour). The core polarity flip is correct against Perl source; every test assertion I walked through matches the new predicate; the smoke fixture rework eliminates the boundary-coincidence pass; clippy + fmt + 229 tests green locally.

---

## Verification trail (load-bearing)

### Perl re-derivation (independent of the plan)

I read `bismark_methylation_extractor` at the cited lines myself and confirmed:

- **Lines 2398-2402** (`$strand eq '+'` = OT/CTOT-pair branch): R1's `$end_read_1` is set to R1's *rightmost* (`$start_read_1 + $MDN_count_1 - 1`); R2's `$start_read_2` is mutated *in place* to R2's *rightmost* via `+= $MDN_count_2 - 1`.
- **Lines 2414-2416** (`else` = OB/CTOB-pair branch): the order matters — `$end_read_1 = $start_read_1` captures R1's *leftmost* on 2415 BEFORE 2416 mutates `$start_read_1` to R1's rightmost. The SPEC §7.4 rev 3 paragraph at SPEC.md:381 calls this out correctly.
- **Lines 2434-2448** (dispatch): OT pair → R2 dispatched with `$strand='-'` (line 2440); OB pair → R2 dispatched with `$strand='+'` (line 2448). The `$end_read_1` argument passed in carries the *post-mutation* value (R1-rightmost for OT pair; R1-leftmost for OB pair).
- **Lines 3744-3747** (`$strand eq '+'` arm of `else { if ($strand eq '+') }` inside `print_individual_C_methylation_states_paired_end_files`, the default-4-context strand-specific output): predicate `if ($start+$index+$pos_offset >= $end_read_1) { return; }` — i.e. drop when `r2_pos >= r1_ref_start` (since for OB pair `$end_read_1` is R1-leftmost).
- **Lines 3825-3828** (`$strand eq '-'` arm): predicate `if ($start-$index+$pos_offset <= $end_read_1) { return; }` — i.e. drop when `r2_pos <= r1_ref_end` (for OT pair `$end_read_1` is R1-rightmost; `$start - $index` produces decreasing positions from R2-rightmost).

The Rust predicates in `src/overlap.rs:102` (`c.ref_pos > r1_ref_end` for forward) and `:109` (`c.ref_pos < r1_ref_start` for reverse) are the strict inverses of Perl's inclusive drop predicates. **Polarity matches.**

### Boundary semantics
- `r2_pos == r1_ref_end` (forward): Perl drop predicate `<= 149` ⇒ DROPPED. Rust keep predicate `> 149` ⇒ DROPPED. Match.
- `r2_pos == r1_ref_start` (reverse): Perl drop predicate `>= 200` ⇒ DROPPED. Rust keep predicate `< 200` ⇒ DROPPED. Match.

### Test geometry walk-through (verified by hand)

| Test | Geometry | Predicate | New asserted keep-set | Matches |
|---|---|---|---|---|
| `drop_overlap_forward_pair_drops_r2_at_or_before_r1_end` | OT, R1 50M@100 ⇒ end=149; R2 calls 148/149/150 | `>149` | `[150]` | ✓ |
| `drop_overlap_reverse_pair_drops_r2_at_or_after_r1_start` | OB, R1 50M@200 ⇒ start=200; R2 calls 199/200/201 | `<200` | `[199]` | ✓ |
| `drop_overlap_disjoint_forward_pair_keeps_all_r2_calls` | OT, R1 end=149; R2 at 300/310/340 | `>149` | all 3 | ✓ |
| `drop_overlap_fully_overlapping_pair_drops_all_r2_calls` | OT, R1 end=149; R2 calls 105/120/148 | `>149` | `[]` | ✓ |
| `with_r1_indel_uses_reference_end` | `50M2D50M`@100 ⇒ span=102 ⇒ end=201; R2 200/201/202 | `>201` | `[202]` | ✓ |
| `with_r1_end_deletion` | `49M2D1M`@100 ⇒ span=52 ⇒ end=151; R2 150/151/152 | `>151` | `[152]` | ✓ |
| `with_r1_insertion_shifts_read_pos_only` | `50M2I50M`@100 ⇒ span=100 ⇒ end=199; R2 198/199/200 | `>199` | `[200]` | ✓ |
| `real_data_fr_pair_with_gap` | OT, R1 64M@100 ⇒ end=163; R2 65M@171; calls 175/200/220/235 | `>163` | all 4 | ✓ |
| `partial_overlap_reverse_pair` | OB, R1 64M@200 ⇒ start=200; R2 calls 195/199/200/201 | `<200` | `[195,199]` | ✓ |
| `r1_with_n_skip_op` | `50M1000N50M`@100 ⇒ span=1100 ⇒ end=1199; R2 1198/1199/1200 | `>1199` | `[1200]` | ✓ (confirmed `reference_span()` counts `Skip` at `bismark-io/src/cigar.rs:160-161`) |
| `r1_with_5prime_soft_clip` | `10S100M`@100 ⇒ span=100 ⇒ end=199; R2 198/199/200 | `>199` | `[200]` | ✓ (confirmed `SoftClip` excluded at `cigar.rs:157-164`) |
| `r1_with_3prime_soft_clip` | `100M10S`@100 ⇒ end=199; R2 198/199/200 | `>199` | `[200]` | ✓ |
| `extract_pe_with_no_overlap_drops_r2_overlap_keeps_unique` (integration) | OT, R1 `Z....`@100 (5M, end=104); R2 `.Z.zZ`@102 (CTOT, iter_aligned reverses → R2 calls at ref 106/105/103) | `>104` | drop 103; keep 105, 106 | ✓ |

### Smoke fixture (`pe_phase_c_smoke.rs`)

New geometry: R1 5M@`r1_start` ⇒ r1_ref_end = `r1_start+4`. R2 5M@`r1_start+5`, XM `....z`, record_strand=CTOT (`-`). After `iter_aligned` reversal (record.rs:299-308), BAM-pos 4 (`z`) becomes read_pos_5p=0 with `ref_pos = r2_start + 4 = r1_start + 9`. r1_start+9 > r1_start+4 → KEPT. ⇒ 10 R1 calls + 10 R2 calls = **20 lines in CpG_OT**. Matches the new assertion at `pe_phase_c_smoke.rs:200`. The pre-fix fixture (R2 at `r1_start`) passed by boundary coincidence; the rework eliminates that.

### Phase F invariant (`parallel_phase_f.rs`)

I confirmed by reading the plan + checking that `parallel_phase_f.rs` was not in the diff — and the cargo test run shows the 15 Phase F tests pass unchanged. The invariant "parallel ≡ sequential" survives because both halves call the corrected `drop_overlap`.

### Build hygiene

- `cargo test -p bismark-extractor` → **229 tests pass** (48 unit + 26 + 3 + 32 + 12 + 15 + 34 + 2 + 4 + 50 + 3, summed across binaries). Matches the plan's claim.
- `cargo clippy -p bismark-extractor --all-targets -- -D warnings` → clean.
- `cargo fmt --check` → clean.

### Cargo.toml

- Version `1.0.0-alpha.6 → 1.0.0-alpha.7` ✓.
- Description updated to mention Phase C.1 polarity fix ✓.

---

## Findings

### Critical
*None.*

### High
*None.*

### Medium

**M1. SPEC §8.1 test enumeration is incomplete — missing the 3 CIGAR-InDel `drop_overlap` tests.**
- **Location:** `rust/bismark-extractor/SPEC.md:603-611`.
- **Plan claim** (post-impl notes, §5.1): *"Test enumeration table at §8.1 (lines 535-543) updated with all 8 + 5-new + renamed tests."*
- **Reality:** the table lists 9 `drop_overlap_*` rows (4 renamed/flipped + 5 new). The 3 CIGAR-InDel tests that exist in `tests/pe_phase_c.rs` and were assertion-flipped in C.1 — `drop_overlap_with_r1_indel_uses_reference_end`, `drop_overlap_with_r1_end_deletion`, `drop_overlap_with_r1_insertion_shifts_read_pos_only` — are NOT listed in SPEC §8.1.
- **Impact:** documentation-only. The tests run and pass; this is a SPEC-vs-source completeness gap. Pre-existing risk (those 3 tests were also absent from the rev-2 §8.1 table), so this is a missed opportunity to clean up during the C.1 rewrite, not a new defect.
- **Recommendation:** add 3 rows to SPEC §8.1 covering the InDel tests. Optional; not blocking.

**M2. Misleading comment in `extract_pe_routes_ctot_pair_strand_correctly` test claims `--include_overlap` is needed to prevent dropping.**
- **Location:** `tests/pe_phase_c.rs:1083-1086`:
  ```
  // CTOT is reverse class — R2 is upstream. Place R2 at 200 (which is
  // upstream of R1 at 230). With --no_overlap, R1 calls keep predicate
  // `r2_pos > r1_ref_start=230` would drop the upstream R2 calls.
  // Use --include_overlap so both R1 and R2 calls land regardless.
  ```
- **Issue:** the comment is *wrong* on two counts:
  1. The keep predicate for CTOT (reverse) is `r2_pos < r1_ref_start`, not `>`. So the rationale's predicate direction is inverted.
  2. With R1 at 230, R2 at 200 (CIGAR 5M), R2's calls land in [200, 204]. The keep predicate `r2_pos < 230` would KEEP all R2 calls — `--include_overlap` is not actually needed to prevent dropping.
- **Impact:** the test still passes (assertion is about strand routing, not overlap behaviour), so this is a comment-only defect. But a future reader trying to reason about polarity from this comment will be misled.
- **Recommendation:** rewrite the comment to either (a) acknowledge that `--include_overlap` is unnecessary for this geometry but kept defensively, or (b) move R2 within the overlap region so the flag is actually load-bearing.

### Low

**L1. Defensive `_ = pair;` dead-fixture pattern in `with_r1_insertion_shifts_read_pos_only` is awkward but works.**
- **Location:** `tests/pe_phase_c.rs:476` — `let _ = pair; // discard the wrong-length fixture`.
- **Issue:** the test constructs a fixture, discards it, then constructs the correct one. The inline comment block (lines 461-475) explains the reasoning but reads like rubber-duck debugging left in the source.
- **Impact:** cosmetic only; pre-existing pattern (not introduced by C.1).
- **Recommendation:** during a future cleanup pass, consolidate to a single correctly-sized fixture and drop the meta-commentary block.

**L2. `extract_pe_routes_ctot_pair_strand_correctly` would now be possible without `--include_overlap`.**
- **Location:** `tests/pe_phase_c.rs:1086-1089`.
- **Issue:** as noted in M2, the geometry (R1@230, R2@200) already places R2 entirely in its unique upstream region, so the post-fix `drop_overlap` would not drop any R2 calls. The `--include_overlap` flag is now redundant for this fixture.
- **Recommendation:** consider removing `--include_overlap` to strengthen the test (it would then also implicitly verify post-fix polarity for CTOT pairs end-to-end). Defer to a follow-up cleanup since the test passes as-is.

**L3. Plan version-bump description in `Cargo.toml` description field is descriptive but unusual.**
- **Location:** `rust/bismark-extractor/Cargo.toml:4` — `description = "Rust port of Bismark Perl's bismark_methylation_extractor script (Phase C.1: drop_overlap polarity fix #862)"`.
- **Issue:** appending the most-recent fix to the crate description means every future `alpha.N` would either need to update or drift. Most Cargo descriptions are stable across versions; phase-specific details belong in CHANGELOG.md or git log.
- **Impact:** cosmetic; the description shows up in `cargo search` / crates.io metadata if/when published.
- **Recommendation:** consider a stable description and move the per-version note to a changelog.

---

## Areas explicitly verified and clean

- Polarity direction in `src/overlap.rs:96-110` matches the Perl source at the cited lines (3745, 3826).
- Boundary semantics (strict `>` / `<` matching Perl's inclusive `<=` / `>=`).
- Monotonicity-equivalence claim (Rust `Vec::retain` ≡ Perl early-return) holds because `aligned_positions()` walks the CIGAR in a strictly monotonic order; reversal in `iter_aligned` for `-`-strand records still yields a monotonic ref_pos sequence (just descending instead of ascending).
- `reference_span()` correctly includes `Skip` (N op) per `bismark-io/src/cigar.rs:160-161`. Test `drop_overlap_r1_with_n_skip_op` exercises this with a 1000-base N skip.
- `reference_span()` correctly excludes `SoftClip` per `cigar.rs:157-164`. Tests `drop_overlap_r1_with_5prime_soft_clip` and `drop_overlap_r1_with_3prime_soft_clip` exercise both ends.
- The `=`/`X` CIGAR-op pre-existing divergence is documented in SPEC §7.4 rev 3 at `SPEC.md:449-451`. Confirmed `reference_span()` counts both at `cigar.rs:161-162`; confirmed Perl `$MDN_count` does NOT count them at Perl lines 2391, 2397, 2412 (only `M/D/N`). Dormant for Bismark-aligned BAMs; correctly flagged for future awareness.
- The Phase F byte-identity invariant is preserved: both `src/run.rs::extract_pe` and `src/parallel.rs` workers call the same `drop_overlap`. The 15-test `parallel_phase_f.rs` suite passes unchanged.
- Smoke fixture rework correctly exercises the post-fix "kept" path (R2 5'-call at ref `r1_start+9` strictly > r1_ref_end at `r1_start+4`), not boundary coincidence.
- Crate version bump correctly applied (`alpha.6 → alpha.7`); `Cargo.lock` reflects this on `rust/Cargo.lock`.
- The module doc block (`src/overlap.rs:1-40`) and function doc block (`src/overlap.rs:47-82`) cite SPEC §7.4 rev 3 + the correct default-branch Perl lines (3744-3747 / 3825-3828) + cross-references to the three byte-identical branches.

---

## Recommendation

**APPROVE for merge** pending (i) the oxy real-data harness run per plan §9.3 (the headline merge gate), and (ii) optional cleanups for M1/M2/L1-L3 (none blocking). The 4-LOC polarity flip is correct, well-documented, and the test suite is now self-consistent with the corrected predicates.

Once oxy results land showing Rust's total-C's-analysed matching Perl's 188,123,599 and M-bias.txt sha256-equal, this PR is ready to merge against `rust/iron-chancellor`.

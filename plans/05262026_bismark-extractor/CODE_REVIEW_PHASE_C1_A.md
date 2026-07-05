# Code Review — Phase C.1 (`drop_overlap` polarity fix, closes #862)

**Reviewer:** A
**Date:** 2026-05-27
**Target:** local branch `extractor-phase-c1` (uncommitted working tree on `rust/iron-chancellor`)
**Files reviewed:**
- `rust/bismark-extractor/src/overlap.rs`
- `rust/bismark-extractor/tests/pe_phase_c.rs`
- `rust/bismark-extractor/tests/pe_phase_c_smoke.rs`
- `rust/bismark-extractor/tests/parallel_phase_f.rs` (unmodified — verified)
- `rust/bismark-extractor/SPEC.md` §7.4 and §8.1
- `rust/bismark-extractor/Cargo.toml`
- Perl reference: `bismark_methylation_extractor` lines 2398-2416, 3562-3826
- `rust/bismark-io/src/cigar.rs::CigarExt`

---

## Verdict

**APPROVE** — the polarity fix is correct, test coverage is thorough, and the
SPEC rewrite accurately reflects Perl's behaviour. No Critical or High issues
found. A handful of Medium and Low observations are documented below for
optional follow-up; none block the merge.

I independently re-derived the polarity from the Perl source (without
consulting the plan) and arrived at the same predicates the Rust code
implements. I walked the geometry of all 5 new regression-guard fixtures plus
3 of the renamed/flipped tests by hand and confirmed the assertions. I
verified that lines 3744-3747 and 3825-3828 are in fact the default
4-context strand-specific branch (`elsif ($no_overlap)` → `else` arm of
`if ($full)`), not the `--merge_non_CpG` or `--comprehensive` branches.
229 tests pass; clippy `-D warnings --all-targets` is clean; rustfmt is clean.

---

## 1. Correctness of the polarity fix

### 1.1 Perl source re-derivation (independent)

**OT/CTOB pair** (forward — R1 upstream, R2 downstream)
- Pre-dispatch (Perl 2398-2402): `$end_read_1 = $start_read_1 + $MDN_count_1 - 1` (R1's rightmost). `$start_read_2 += $MDN_count_2 - 1` (R2's rightmost).
- Dispatch (line 2440): R2 passed with `$strand='-'`.
- Predicate (line 3826, in `elsif ($no_overlap)` → `else` arm of `if ($full)`): `if ($start - $index + $pos_offset <= $end_read_1) { return; }`.
- Substituting: `$start - $index + $pos_offset` = R2's iterated ref pos (decreasing from rightmost). `$end_read_1` = R1's rightmost.
- **Drop predicate:** `r2_pos <= r1_ref_end`. **Keep predicate (inverse):** strict `r2_pos > r1_ref_end`. ✓

**OB/CTOT pair** (reverse — R2 upstream, R1 downstream)
- Pre-dispatch (Perl 2414-2416): `$end_read_1 = $start_read_1` (R1's *original* leftmost, BEFORE the next line mutates it). `$start_read_1 += $MDN_count_1 - 1` (R1's rightmost, but irrelevant to R2's predicate). R2's `$start_read_2` is NOT mutated in this branch — R2 retains its natural leftmost.
- Dispatch (line 2448): R2 passed with `$strand='+'`.
- Predicate (line 3745): `if ($start + $index + $pos_offset >= $end_read_1) { return; }`.
- Substituting: `$start + $index + $pos_offset` = R2's iterated ref pos (increasing from leftmost). `$end_read_1` = R1's original leftmost = `r1_ref_start`.
- **Drop predicate:** `r2_pos >= r1_ref_start`. **Keep predicate (inverse):** strict `r2_pos < r1_ref_start`. ✓

These match `src/overlap.rs:102` (`c.ref_pos > r1_ref_end`) and `:109`
(`c.ref_pos < r1_ref_start`). Polarity is correct.

### 1.2 Branch citation (default 4-context strand-specific)

I verified by brace-balance trace that lines 3744 and 3825 are inside:
- `sub print_individual_C_methylation_states_paired_end_files` (line 2813)
- `elsif ($no_overlap) {` (line 3562) — the outer-level `elsif` chained to `if ($merge_non_CpG)` at 2890 which closes at 3556.
- `else {` arm of `if ($full)` (line 3733) — the default 4-context strand-specific path, NOT `--comprehensive`.

So the rev-3 citations of 3744-3747/3825-3828 are correct, as documented in
`overlap.rs:5-7,55-57` and `SPEC.md:387,397`. The plan's revision (which
moved off the wrong rev-2 citations at 2905/2989) is itself accurate. ✓

### 1.3 Boundary semantics

For `r2_pos == r1_ref_end` (OT) / `r2_pos == r1_ref_start` (OB):
- Perl `<=` / `>=` → fires drop on equality.
- Rust `>` / `<` → returns false on equality → element removed by `retain`.
- Behaviour: DROPPED. ✓ Matches Perl.

Locked in by `drop_overlap_forward_pair_drops_r2_at_or_before_r1_end`
(pe_phase_c.rs:287) which asserts r2_pos==149 (=r1_ref_end) is dropped, and
`drop_overlap_reverse_pair_drops_r2_at_or_after_r1_start` (:310) asserting
r2_pos==200 (=r1_ref_start) is dropped.

### 1.4 Monotonicity equivalence (`Vec::retain` ≡ Perl early-return)

For Bismark XM strings, Perl's iteration is monotonic in `ref_pos`:
- OT R2 (`$strand='-'`, `$start - $index`): strictly decreasing.
- OB R2 (`$strand='+'`, `$start + $index`): strictly increasing.

Therefore Perl's "early return at first overlap-region call" emits the same
set as Rust's `retain(|c| keep_predicate(c))`. This invariant holds for any
M/I/D/N/S CIGAR because `aligned_positions` walks ops in CIGAR order and
yields ref positions in lockstep (verified in `bismark-io/src/cigar.rs`).
Documented at `overlap.rs:35-40` and `SPEC.md:436-443`. ✓

### 1.5 `CigarExt::reference_end` correctness

`CigarExt::reference_span()` sums M, D, N, =, X op lengths
(`bismark-io/src/cigar.rs:157-164`). `reference_end(start) = start + span - 1`
for non-empty CIGARs (`:182-187`). Matches Perl's `$MDN_count` (lines
2390-2398) modulo the `=`/`X` divergence noted at `SPEC.md:449-451`. The
divergence is dormant for Bismark/Bowtie2 BAMs (M-only emission).

---

## 2. Test correctness (hand-walked geometry)

### 2.1 Eight flipped/renamed tests

| # | Test name | Geometry | Asserted result | Walk |
|---|---|---|---|---|
| 1 | `drop_overlap_forward_pair_drops_r2_at_or_before_r1_end` | R1 50M @100 → r1_ref_end=149; R2 calls at 148/149/150 | Keep [150] | 148≤149 drop; 149≤149 drop; 150>149 keep. ✓ |
| 2 | `drop_overlap_reverse_pair_drops_r2_at_or_after_r1_start` | R1 50M @200 → r1_ref_start=200; R2 calls at 199/200/201 | Keep [199] | 199<200 keep; 200≥200 drop; 201≥200 drop. ✓ |
| 3 | `drop_overlap_disjoint_forward_pair_keeps_all_r2_calls` | R1 50M @100 (end 149); R2 calls at 300/310/340 | Keep all 3 | All > 149. ✓ |
| 4 | `drop_overlap_fully_overlapping_pair_drops_all_r2_calls` | R1 50M @100 (end 149); R2 calls at 105/120/148 | Drop all | All ≤ 149. ✓ |
| 5 | `drop_overlap_with_r1_indel_uses_reference_end` | R1 50M+2D+50M @100 → span 102, end 201; R2 at 200/201/202 | Keep [202] | 200≤201 drop; 201≤201 drop; 202>201 keep. ✓ |
| 6 | `drop_overlap_with_r1_end_deletion` | R1 49M+2D+1M @100 → span 52, end 151; R2 at 150/151/152 | Keep [152] | ✓ |
| 7 | `drop_overlap_with_r1_insertion_shifts_read_pos_only` | R1 50M+2I+50M @100 → span 100, end 199; R2 at 198/199/200 | Keep [200] | ✓ |
| 8 | `extract_pe_with_no_overlap_drops_r2_overlap_keeps_unique` | R1 5M @100 → end 104; R2 5M @102 CTOT, calls at 103/105/106 | Drop 103; keep 105, 106 | 103≤104 drop; 105>104 keep; 106>104 keep. ✓ |

### 2.2 Five new regression-guard fixtures

| # | Fixture | Geometry | Walk |
|---|---|---|---|
| A | `drop_overlap_real_data_fr_pair_with_gap_keeps_all_r2_calls` (`:510`) | R1 XM 64-byte → CIGAR 64M @100 → end 163. R2 XM 65-byte → 65M @171. R2 calls at 175/200/220/235. | All > 163 → 4 kept. ✓ |
| B | `drop_overlap_partial_overlap_reverse_pair` (`:541`) | OB pair: R1 @200 (64M) → r1_ref_start=200; R2 @150 (64M). R2 calls at 195/199/200/201. | 195<200 keep; 199<200 keep; 200≥200 drop; 201≥200 drop. ✓ |
| C | `drop_overlap_r1_with_n_skip_op` (`:574`) | R1 CIGAR 50M+1000N+50M @100. `CigarExt::reference_span` sums M+D+N+=+X → 50+1000+50 = 1100. end = 1199. R2 calls at 1198/1199/1200. | 1198≤1199 drop; 1199≤1199 drop; 1200>1199 keep. ✓ |
| D | `drop_overlap_r1_with_5prime_soft_clip` (`:604`) | R1 CIGAR 10S+100M @100. SoftClip NOT in reference_span (line 157-164 only M/D/N/=/X). end = 100+100-1 = 199. R2 at 198/199/200 → keep 200. ✓ |
| E | `drop_overlap_r1_with_3prime_soft_clip` (`:634`) | R1 CIGAR 100M+10S @100. Symmetric to D. end = 199. R2 at 198/199/200 → keep 200. ✓ |

All five new fixtures' assertions match the expected geometry.

### 2.3 Smoke fixture rework

`tests/pe_phase_c_smoke.rs::write_pe_directional_bam` (`:100-136`):
- 10 OT pairs. R1 5M at `r1_start = 100 + i*200`, XM `"Z...."` → R1 CpG call at ref_pos 100.
- R2 5M at `r2_start = r1_start + 5`, XM `"....z"`. R2 is CTOT (`-` strand), `iter_aligned` reverses: BAM-pos 4 ('z') ↔ read_pos_5p=0, ref_pos = r2_start + 4 = r1_start + 9.
- r1_ref_end = r1_start + 5 - 1 = r1_start + 4. r2_call_pos = r1_start + 9. Difference = 5. 5 > 0 → kept.

Resulting CpG_OT lines: 10 (R1) + 10 (R2) = 20 (plus header = 21 file lines).
Assertion at `:200` is `cpg_ot_call_lines == 20` (computed as `lines().count() - 1`). ✓

The rework correctly exercises the post-fix "kept" path rather than passing
by boundary coincidence. The rationale comment at `:188-196` accurately
explains why.

### 2.4 Phase F invariant (`tests/parallel_phase_f.rs`)

15 tests pass without modification. I verified that this file does not
contain hardcoded golden snapshots — it asserts parallel-vs-sequential
equivalence only. Both halves run the same `drop_overlap`, so the polarity
flip is consistent across them. ✓

---

## 3. SPEC §7.4 correctness

I read SPEC.md:357-451 end-to-end. Key validations:

- **Rev-3 header (`:359`)** accurately frames rev-2's defect: missed
  coordinate pre-mutations, byte-identical predicates across branches.
- **Coordinate pre-mutations (§ "Coordinate pre-mutations in Perl")**:
  - OT block at SPEC `:365-372` quotes Perl 2398-2402 correctly. The R2
    rightmost-then-`$strand='-'` dispatch is accurate.
  - OB block at SPEC `:374-381` quotes Perl 2414-2416 correctly, with the
    correct emphasis on the load-bearing order at lines 2415-2416 (`$end_read_1`
    captures the original `$start_read_1` BEFORE it's mutated).
- **R2 predicates section (`:383-412`)**:
  - Confirms the section is the default 4-context strand-specific output
    (`elsif ($no_overlap)` → `else` arm of `if ($full)`).
  - OB/CTOT predicate at `:387-395` quotes Perl 3744-3747 correctly with
    `$strand='+'` and derives keep = `r2_pos < r1_ref_start`.
  - OT/CTOB predicate at `:397-405` quotes Perl 3825-3828 correctly with
    `$strand='-'` and derives keep = `r2_pos > r1_ref_end`.
  - Cross-references to other Perl branches (`:407-410`) match: 3576/3657
    (--comprehensive), 2905/2987 (--merge_non_CpG), ~4065. I spot-checked
    line 2905 — yes, that's the `--merge_non_CpG` predicate inside
    `if ($merge_non_CpG)` at 2890.
- **Boundary semantics (`:432-434`)**: correct — strict `>`/`<` ↔ inclusive
  `<=`/`>=`.
- **Monotonicity equivalence (`:436-443`)**: correct.
- **Edge-case section (`:445-447`)**: rev-3 statement is correct — disjoint
  pairs keep all R2 (NOT the rev-2 claim of dropping all).
- **`=`/`X` divergence note (`:449-451`)**: correctly flagged as dormant for
  Bismark/Bowtie2 BAMs; not in C.1 scope.

The SPEC rewrite is accurate and complete. No corrections needed.

### 3.1 SPEC §8.1 test-enumeration table

All 9 C.1-related rows are present at lines 603-611 of SPEC.md and the test
names match the source in `tests/pe_phase_c.rs` exactly:

| SPEC row | Test in source | Status |
|---|---|---|
| `drop_overlap_forward_pair_drops_r2_at_or_before_r1_end` | `:287` | ✓ |
| `drop_overlap_reverse_pair_drops_r2_at_or_after_r1_start` | `:310` | ✓ |
| `drop_overlap_disjoint_forward_pair_keeps_all_r2_calls` | `:333` | ✓ |
| `drop_overlap_fully_overlapping_pair_drops_all_r2_calls` | `:369` | ✓ |
| `drop_overlap_real_data_fr_pair_with_gap_keeps_all_r2_calls` | `:509` | ✓ |
| `drop_overlap_partial_overlap_reverse_pair` | `:540` | ✓ |
| `drop_overlap_r1_with_n_skip_op` | `:573` | ✓ |
| `drop_overlap_r1_with_5prime_soft_clip` | `:603` | ✓ |
| `drop_overlap_r1_with_3prime_soft_clip` | `:633` | ✓ |

---

## 4. Findings

### Critical
None.

### High
None.

### Medium

**M1. `--include_overlap` integration test asserts pre-fix-leaning content but is
correctly disabled.** `extract_pe_with_include_overlap_keeps_r2_overlap_calls`
(`pe_phase_c.rs:902-923`) asserts that under `--include_overlap`, R2 calls at
ref_pos 103/105/106 are all kept (4 lines total including R1). R2 is CTOT
record-strand; under reversal, R2 XM `.Z.zZ` produces calls at BAM-pos 1
('Z'), 3 ('z'), 4 ('Z'), giving ref positions 103/105/106. The test correctly
exercises a path that bypasses `drop_overlap` entirely. No issue — flagged
only because the `assert_eq!(call_lines, 4, ...)` was previously stable on
the buggy code, so the equivalence proof for `--include_overlap` is by
construction rather than by polarity logic. Mention only — no action needed.

**M2. Insertion test (`drop_overlap_with_r1_insertion_shifts_read_pos_only`,
`:447-503`) constructs two `pair`s, the first with a deliberately wrong-length
XM (100 bytes vs CIGAR consuming 102 read bases).** The first `pair` is
discarded via `let _ = pair`. While the code works (the construction
succeeds because `BismarkRecord` only validates `xm.len() == seq.len()`, not
CIGAR consistency), the test reads as accidentally-correct code with stale
scaffolding inline. Cleaner form would be to remove the discarded first
construction and build the correct 102-byte XM upfront. **Recommend** but do
not block. Suggested:

```rust
// Single construction with correct XM length (102 = 50M + 2I + 50M consumes 102 read bases).
let r1_xm: Vec<u8> = std::iter::repeat(b'.').take(102).collect();
let pair = helpers::ot_pair_with_r1_cigar(
    &r1_xm,
    100,
    &[(Kind::Match, 50), (Kind::Insertion, 2), (Kind::Match, 50)],
    b".....",
    150,
    b"q_ins",
);
```

**M3. SPEC §8.1 test-enumeration table does not list the 3 InDel/insertion
edge tests** (`drop_overlap_with_r1_indel_uses_reference_end`,
`drop_overlap_with_r1_end_deletion`,
`drop_overlap_with_r1_insertion_shifts_read_pos_only`). These predate C.1
and are still in the source. The enumeration table is for unit tests, and
omitting these three is a doc gap. Not introduced by C.1. **Recommend** a
3-row append on follow-up.

### Low

**L1. Cargo.toml description tag.** `version = "1.0.0-alpha.7"` plus
`description = "... (Phase C.1: drop_overlap polarity fix #862)"` — the
description tagging is unconventional but harmless. Future alphas will keep
appending phase tags or the field will need a clean-up pass. No action.

**L2. `overlap.rs:68-70` comment refers to a "Phase C rev 1 simplification per
Reviewer B L6"** that doesn't belong to C.1's review log. Carry-over from
the parent Phase C review; not a C.1 issue. Could be tightened to just
"Pair-strand is recovered internally via `BismarkPair::pair_strand`."
Optional.

**L3. Comment style at `overlap.rs:1-40` is a 40-line doc block.** Very
detailed. Justified by the load-bearing nature of the polarity decision —
not a defect, just unusual density. No action.

**L4. The `n_skip_op` test uses `r2_start = 1300`** with R2 manually
constructed at this position, but the assertion only relies on the
synthetic R2 calls placed manually at 1198/1199/1200. R2's own XM (5 dots)
contributes no calls. Geometry is correct but the `1300` is somewhat
disconnected from the test's assertions. Cosmetic — no action.

---

## 5. Run/lint results (reproduced locally)

- `cargo test -p bismark-extractor`: **229 tests pass** (verified across 11
  test binaries + 1 doctest target). Matches the plan's claim.
- `cargo clippy -p bismark-extractor --all-targets -- -D warnings`: **clean**.
- `cargo fmt -p bismark-extractor -- --check`: **clean**.
- `cargo check`: passes with no warnings.
- The 5 new tests + the 8 flipped/renamed tests all run in the same
  `pe_phase_c.rs` binary; the 12 listed `drop_overlap*` tests pass in
  isolation (`cargo test ... -- drop_overlap`).

---

## 6. Areas explicitly verified by re-derivation (not just by reading)

1. Perl line citations 3744-3747 and 3825-3828 are inside the default
   4-context strand-specific branch (`elsif ($no_overlap)` → `else` arm of
   `if ($full)`), confirmed by brace-balance trace from line 2890 through
   4226.
2. The `--merge_non_CpG` branch starts at line 2890 and closes at line 3556
   — the predicates at 2905/2987 (cited in rev 2) are inside this branch,
   NOT the default. Rev 3's correction is itself correct.
3. The 5 new fixtures' assertions match the polarity-flip predictions in §2.2
   above.
4. The smoke fixture's `r2_start = r1_start + 5` geometry produces R2's
   reversed call at `r1_start + 9`, which is strictly past `r1_ref_end =
   r1_start + 4` and is therefore kept by the post-fix predicate.
5. `CigarExt::reference_span` includes `N` ops (line 157-164 of
   `bismark-io/src/cigar.rs`), matching Perl's `$MDN_count` for the n-skip
   test fixture.

---

## 7. Recommendation

**Approve the implementation.** The 4-LOC code change, SPEC rewrite, 8 test
flips + 3 renames, 5 new regression-guard tests, and smoke-fixture rework
are all internally consistent and match Perl. The Medium-priority items
(M1/M2/M3) are cosmetic and can be addressed in a follow-up or ignored. No
blockers.

Next gate per the plan: the real-data byte-identity run on oxy (`scripts/oxy_phase_h_smoke.sh`)
to confirm `Total C's analysed`, per-file line counts, and `M-bias.txt
sha256` match Perl's outputs on the 10M PE WGBS dataset.

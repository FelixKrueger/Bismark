# Plan Review — Phase C.1 (drop_overlap polarity fix, closes #862) — Reviewer B

**Plan under review:** `plans/05262026_bismark-extractor/PHASE_C1_PLAN.md` (rev 0)
**Reviewer:** B (independent dual review pass)
**Date:** 2026-05-27

---

## Executive Summary

The plan correctly diagnoses the bug, correctly re-derives the Perl semantics
(I re-derived independently from the Perl source and arrived at the same
conclusion), and proposes a 4-LOC code fix that is the right change. The
critical claims — *OT keep `r2_pos > r1_ref_end`*, *OB keep
`r2_pos < r1_ref_start`*, *boundary handling strict*, *Phase F invariant
preserved* — all check out. The plan's main weakness is **incomplete test
enumeration**: it does not name `pe_phase_c_smoke.rs::smoke_pe_auto_detect_…`
or `pe_phase_c.rs::extract_pe_with_no_overlap_drops_r2_calls_past_r1_end` or
`drop_overlap_fully_overlapping_pair_keeps_calls_inside_r1_span` even though
all three encode the old polarity. Two of the three will fail post-fix. A
third (smoke) will pass by coincidence (boundary case) but its rationale
comment will be wrong. There is also an implicit-monotonicity assumption in
the Rust `retain` ≡ Perl-early-return equivalence that deserves an explicit
note. Otherwise the plan is implementable as written and the scope cut
(#862 only, #863–#865 deferred) is the right call.

---

## 1. Logic Review

### 1.1 Polarity derivation — INDEPENDENTLY VERIFIED ✓

I re-derived the Perl semantics from the source, line by line, **without**
trusting the plan's transcriptions:

**OT pair** (`$strand eq '+'` at line 2434):
- R1 dispatched at line 2436 with `$no_overlap=0, $end_read_1=0` (no overlap
  enforcement on R1 itself).
- R2 dispatched at line 2440 with `strand='-'`, `$start_read_2` already
  pre-mutated to **R2's rightmost** position by line 2401
  (`$start_read_2 += $MDN_count_2 - 1`).
- `$end_read_1 = $start_read_1 + $MDN_count_1 - 1` (line 2400) — **R1's
  rightmost** position.
- R2's predicate (line 3657, `strand=='-'` branch): `if ($start - $index + $pos_offset <= $end_read_1) { return; }`.
- Iteration: `$start` is R2's rightmost; `$index` increments from 0 → reduces
  `$start - $index` monotonically. So Perl emits R2 calls from R2's rightmost
  position downward until the position crosses ≤ R1's rightmost. Emitted set =
  `{ r2_pos : r2_pos > r1_ref_end }`.
- ⇒ **Rust keep predicate: `r2_pos > r1_ref_end`** ✓ matches plan §3.1.

**OB pair** (`$strand eq '-'` at line 2442, else branch):
- R1 dispatched at line 2444 with `$no_overlap=0, $end_read_1=0`.
- R2 dispatched at line 2448 with `strand='+'`, `$start_read_2` is R2's
  natural alignment start (BAM leftmost, no transformation — this branch
  only mutates `$start_read_1`).
- `$end_read_1 = $start_read_1` (line 2415) — set to the **original** R1
  leftmost BEFORE `$start_read_1` itself gets mutated by line 2416. Subtle
  but critical: had Perl reversed the order of lines 2415 and 2416, the
  semantics would be entirely different. The plan correctly identifies this
  ordering.
- R2's predicate (line 3576, `strand=='+'` branch): `if ($start + $index + $pos_offset >= $end_read_1) { return; }`.
- Iteration: `$start` is R2's leftmost; `$index` increments → increases
  `$start + $index` monotonically. Perl emits R2 calls from R2's leftmost
  upward until the position crosses ≥ R1's leftmost. Emitted set =
  `{ r2_pos : r2_pos < r1_ref_start }`.
- ⇒ **Rust keep predicate: `r2_pos < r1_ref_start`** ✓ matches plan §3.1.

### 1.2 Boundary inclusivity — VERIFIED ✓

Perl's drop predicates are `<=` (OT R2) and `>=` (OB R2) — **inclusive** on
the boundary. The Rust keep predicates `>` and `<` are **strict** — the
correct logical inverse. The plan's §3.2 edge-case table (rows
*"Boundary exact match"*) gets this right. I verified by mentally walking
both `r2_pos == r1_ref_end` (OT: should drop — Perl's `<=` fires) and
`r2_pos == r1_ref_start` (OB: should drop — Perl's `>=` fires).

### 1.3 The `merge_non_CpG` red herring — UNDERSTOOD ✓

The plan's §2.2 calls out that SPEC rev 2 mis-cited Perl lines 2905/2989
(inside the `merge_non_CpG` branch) instead of 3576/3657 (default 4-context
branch). My check: **those predicates are identical** (lines 2905 and 3576
both read `if ($start+$index+$pos_offset >= $end_read_1)`; lines 2987 and
3657 both read `if ($start-$index+$pos_offset <= $end_read_1)`). So the
*citation* was wrong but the *predicate semantics* are the same. The plan
should perhaps note this — it's not actually load-bearing for correctness,
just for documentation clarity. The plan's framing slightly overstates the
SPEC rev-2 error: the real bug was overlooking the coordinate
pre-mutations (lines 2401, 2415-2416), not the line citation.

**Optional clarification for plan rev 1:** in §2.2 add a parenthetical
remark like *"the predicates at lines 2905/2989 (merge_non_CpG branch) are
byte-identical to those at 3576/3657 (default branch), so the rev-2
citation was misleading but the predicate transcription was correct. The
actual root cause was overlooking the coordinate pre-mutations at lines
2401 and 2415-2416."*

### 1.4 Implicit monotonicity assumption — FLAG

The plan assumes Rust's set-based `Vec::retain(|c| c.ref_pos > r1_ref_end)`
produces the same set as Perl's early-return iteration. **This holds if
and only if the iteration's ref_pos sequence is monotonic.** It is — for
any well-formed CIGAR — because `aligned_positions` yields one item per
read base and:
- For `strand='+'`: `$start + $index + $pos_offset` ascends as $index ↑
  (insertions don't shift ref, deletions only jump forward, so $start+$index
  modulo pos_offset is non-decreasing).
- For `strand='-'`: `$start - $index + $pos_offset` descends as $index ↑
  (mirror).

This invariant is *load-bearing* — if it ever fails (e.g., a malformed CIGAR
that drives `$pos_offset` non-monotone in a way large enough to cross
`r1_ref_end`), Perl's early-return would emit a different set than Rust's
filter. **For the test fixtures the plan envisions, the invariant holds. For
real-world bisulfite BAMs, it should also hold.** But it is not stated in the
plan and not enforced in code.

**Action:** Plan rev 1 §3.1 should add one sentence: *"The set-based filter
is equivalent to Perl's early-return iteration because R2's
`ref_pos` sequence (as yielded by `aligned_positions`) is monotonic in
iteration order — strictly ascending for forward-strand records,
descending for reverse-strand records. This invariant is inherited from
the SAM spec's CIGAR semantics."*

### 1.5 Out-of-scope deferrals — APPROPRIATE ✓

#863 (parallel ordering), #864 (splitting-report format), #865 (empty
CTOT/CTOB files) are correctly excluded. They are independent byte-identity
gaps that don't share code or rationale with the polarity fix. Bundling them
would make the PR larger without simplifying validation: each has its own
test surface and its own failure-mode locality. The plan's
"sorted-content MD5" merge-gate accepts that strict byte-identity won't
land until #863 ships. That's a reasonable acceptance criterion for the
C.1 PR alone.

---

## 2. Assumptions Review

### 2.1 Stated assumptions A1–A10 — mostly verified

| # | Assumption | Verdict |
|---|---|---|
| A1 | `$MDN_count_X` = CIGAR-derived reference span | ✓ Verified from Perl lines 2390-2398. |
| A2 | `CigarExt::reference_end(start) = start + MDN_count - 1` | ✓ Verified at `bismark-io/src/cigar.rs:182-187`. **However see §3 below** — `reference_span()` also includes `=` and `X` ops, which Perl's `$MDN_count` does *not* include. For Bismark-emitted BAMs this is a no-op (Bowtie2 emits only `M`), but a foreign tool that emits `=`/`X` would diverge. Worth a one-line note in SPEC. |
| A3 | `MethCall::ref_pos` is 1-based reference | ✓ Verified at `bismark-io/src/record.rs:293` (`ref_pos: alignment_start + ref_offset as u32`). |
| A4 | `--no_overlap` predicate only applied to R2 | ✓ Verified at lines 2436/2440/2444/2448 (R1 dispatch passes `$no_overlap=0`). |
| A5 | Perl `$no_overlap=1` default for PE | ✓ Verified at lines 1215-1225. |
| A6 | `BismarkPair::pair_strand()` correctly classifies OT/OB/CTOT/CTOB | Trust-but-verify; covered by existing Phase C tests. Not changed by this fix. |
| A7 | One-PR scope (SPEC + code + tests) | ✓ Sensible. |
| A8 | Branch from `rust/iron-chancellor` HEAD | ✓ Sensible. |
| A9 | Phase F tests pass without changes | ✓ Verified independently — see §4.1 below. |
| A10 | Performance investigation out of scope | ✓ Sensible (the call-volume change perturbs perf numbers). |

### 2.2 Unstated assumptions surfaced

- **U1.** R2's `aligned_positions` iterator order matches Perl's `$index`
  iteration order. (Plan §1.4 above — flag.)
- **U2.** `CigarExt::reference_span()` semantics match Perl's `MDN_count`
  for Bismark-emitted BAMs. Holds because Bowtie2 emits `M` only. If a user
  pipes in a BAM with `=`/`X` ops, Rust will count them; Perl will not.
  This is a *pre-existing* divergence (predates Phase C.1) and is properly
  out of scope for #862, but worth one SPEC sentence so it's not a future
  surprise.
- **U3.** For pairs where `pair.r1().alignment_start()` returns `Some(0)` —
  i.e., a record with explicit POS=0 — the `r1_ref_start as u32 = 0`. Then
  `c.ref_pos < 0` is impossible (u32 unsigned), so the OB branch keeps zero
  R2 calls. Is this correct? It probably is (POS=0 is unmapped in SAM
  semantics and should have been filtered upstream), but the edge case
  isn't enumerated.
- **U4.** `r1_ref_end as u32` (plan §5.2) — `CigarExt::reference_end`
  returns `usize`. The cast is fine for any genomic position fitting in
  u32 (4.2 Gb), but a SPEC remark on the assumed coordinate range would
  match the SPEC's style.

---

## 3. Efficiency Analysis

- **Complexity:** `Vec::retain` is O(R2_calls). Unchanged. ✓
- **Memory:** No realloc (in-place). Unchanged. ✓
- **Branch prediction:** Negligible difference. ✓
- **Throughput:** Post-fix Rust emits ~88% more bytes (per the plan's §9.3
  pre/post table). The plan correctly defers a re-measurement (A10). I
  agree.

There's one **minor potential optimization the plan doesn't mention**: since
the predicate is strictly monotonic on `ref_pos`, R2's calls form an
already-sorted prefix-or-suffix that survives `retain`. A `binary_search` +
`split_off` would be O(log R2) instead of O(R2). This is **not worth doing**
for an ~150-element vector (typical R2 length), and the constant-factor
overhead of `binary_search` likely beats the `retain` walk only for very
large R2 lengths. **Recommendation: keep `retain` as-is.** I mention this
only so it isn't a future code-review nit.

---

## 4. Validation Sufficiency

### 4.1 Test surface — INCOMPLETE in plan ⚠️

The plan's §5.3.1 enumerates four `pe_phase_c.rs` tests to update. **It
misses three more that I found by reading the test file directly:**

#### Test 1 (MISSED, will FAIL post-fix): `extract_pe_with_no_overlap_drops_r2_calls_past_r1_end` (`pe_phase_c.rs:752`)

This integration test uses the binary, not just `drop_overlap` directly. Its
asserted geometry:
- R1 `Z....` 5M at 100 → r1_ref_end = 104.
- R2 `.Z.zZ` 5M at 102, record_strand CTOT, iter_aligned reverses → R2 calls
  at ref_pos 106, 105, 103.
- **Current (buggy) assertions:** keep `\t103\t` (the in-overlap call);
  drop `\t105\t`, `\t106\t` (the unique-region calls).
- **Post-fix assertions must invert:** drop `\t103\t`; keep `\t105\t` and
  `\t106\t`.

**This test must be renamed and re-asserted.** Suggested name:
`extract_pe_with_no_overlap_drops_r2_calls_in_overlap_keeps_unique_region`.

#### Test 2 (MISSED, will FAIL post-fix): `drop_overlap_fully_overlapping_pair_keeps_calls_inside_r1_span` (`pe_phase_c.rs:360`)

This test embeds the *backwards* polarity in its name. R1=[100,149],
R2 fully inside R1 (R2 starts at 100). Three R2 calls at 105, 120, 148.
Currently asserts all 3 kept. **Post-fix:** all 3 dropped (all r2_pos ≤
r1_ref_end). Must rename to
`drop_overlap_fully_overlapping_pair_drops_all_r2_calls` and invert
assertion to `kept.len() == 0`.

#### Test 3 (MISSED but boundary-passes, comment is wrong): `smoke_pe_auto_detect_produces_all_12_files_and_report` (`pe_phase_c_smoke.rs:131`)

The smoke fixture has 10 OT pairs with R2 starting at the same position as
R1 (`r2_start = r1_start`). R2's call lands at `ref_pos = r2_start + 4 =
r1_ref_end`. **Boundary case.**

- Under current bug (`r2_pos < r1_ref_end`): r2_pos == r1_ref_end → fails
  strict-`<` → dropped. → 0 R2 calls in output.
- Under fix (`r2_pos > r1_ref_end`): r2_pos == r1_ref_end → fails
  strict-`>` → dropped. → 0 R2 calls in output.

**Result: assertion `cpg_ot_call_lines == 10` passes both ways.** But the
test's rationale comment is now WRONG. The comment currently reads:
*"Strict-\`<\` keep predicate (r2_pos < r1_ref_end) fails for r2_pos ==
r1_ref_end → dropped"* — this references the wrong predicate direction. The
test must update its rationale comment to reference the new keep predicate
(`r2_pos > r1_ref_end`), and ideally should also be hardened by spacing R2
*just past* R1 so the test exercises the post-fix "kept" path rather than
relying on the boundary coincidence. Spacing R2 at `r1_start + 5` (one base
past r1_ref_end) would make 10 R2 calls survive, making
`cpg_ot_call_lines == 20`.

#### Note on plan §5.3.1's existing enumeration

The plan's §5.3.1 names:
1. `drop_overlap_forward_pair_drops_r2_at_or_after_r1_end` ✓ (currently named `_at_or_after_r1_end`; the test fixture and assertion happen to be correct under both polarities for the specific points 148/149/150 — under buggy `<149` keeps 148 only; under fixed `>149` keeps neither 148/149 but keeps 150. **Wait, the test keeps 148 (`kept.len() == 1, kept[0].ref_pos == 148`).** Under fixed polarity: keep `r2_pos > 149` → 150 only. Drops 148, 149. Assertion needs to flip to `kept[0].ref_pos == 150`.)
2. `drop_overlap_reverse_pair_drops_r2_at_or_before_r1_start` ✓ (mirror — assertion needs to flip from `kept[0].ref_pos == 201` to `kept[0].ref_pos == 199`).
3. `drop_overlap_non_overlapping_pair_is_noop` — **I cannot find this test in the current `pe_phase_c.rs`.** Either it was removed, renamed, or the plan is referencing a name that doesn't match the live source. Plan should be more careful with test-name fidelity.
4. `drop_overlap_disjoint_pair_drops_all_r2_calls_downstream_of_r1_end` ✓ — flip and rename. Confirmed at `pe_phase_c.rs:332`.

Plan also misses these inversions for the CIGAR-aware tests:
- `drop_overlap_with_r1_indel_uses_reference_end` (line 386): R1 `50M2D50M` at 100 → r1_ref_end=201. R2 calls at 200, 201, 202. Currently asserts `kept[0].ref_pos == 200`. Post-fix: keep `r2_pos > 201` → 202 only.
- `drop_overlap_with_r1_end_deletion` (line 409): R1 `49M2D1M` at 100 → r1_ref_end=151. R2 calls at 150, 151, 152. Currently asserts `kept[0].ref_pos == 150`. Post-fix: keep `r2_pos > 151` → 152 only.
- `drop_overlap_with_r1_insertion_shifts_read_pos_only` (line 433): R1 `50M2I50M` at 100 → r1_ref_end=199. R2 calls at 198, 199, 200. Currently asserts `kept[0].ref_pos == 198`. Post-fix: keep `r2_pos > 199` → 200 only.

**These three CIGAR tests are exactly the kind a polarity-flip review can
easily miss because they look "correct" on a CIGAR-handling axis.** They
must flip too.

#### Suggested action for plan rev 1

Replace the enumeration in §5.3.1 with a **complete grep-driven list** of
every test in `pe_phase_c.rs` + `pe_phase_c_smoke.rs` that mentions
`drop_overlap` or uses `--no_overlap` (default) without `--include_overlap`,
with the new expected keep-set for each. I count **at minimum 9 tests** that
need attention (4 unit-level + 3 CIGAR-aware + 1 integration + 1 smoke).

### 4.2 Real-data validation — APPROPRIATE ✓

§9.3 oxy harness re-run is the right merge gate. The pre/post table with
specific numbers (188,123,599 Cs analysed; 4,193,739 CpG_OT lines) gives
implementers concrete success criteria. The acceptance of *sorted-content
MD5 = PASS / raw byte-identity FAIL until #863* is honest about the
remaining gaps.

### 4.3 Edge case enumeration — STRONG but has gaps

The plan's §3.2 table covers 9 cases. Missing or under-tested:

- **EC-1.** Soft-clipped R1 CIGAR (`30S100M` at pos 100 → reference_span = 100,
  reference_end = 199, BUT `pair.r1().alignment_start()` returns the
  **leftmost-aligned-base position**, which already excludes the soft-clip.
  So `reference_end = 199` is correct. The plan's §11 R3 risk identifies
  this; recommend adding one synthetic-fixture test (mentioned in the plan
  as optional during impl — make it required).
- **EC-2.** R1 CIGAR with `N` skip (rare but possible in spliced bisulfite
  RNA-seq). `reference_span()` counts `N` (verified at
  `bismark-io/src/cigar.rs:157-164`). Perl's `$MDN_count` also counts `N`
  (line 2391). ✓ Match. **No new test needed**, but worth a comment in
  SPEC §7.4 that `N` is handled.
- **EC-3.** R1 CIGAR with `=` or `X` (CIGAR-extended ops). Rust's
  `reference_span()` includes `=`/`X`. Perl's `$MDN_count` does **not**
  (lines 2391, 2412 only count `M/D/N`). This is a **pre-existing
  divergence** between Rust and Perl that predates Phase C.1 and is
  unlikely to affect Bismark-emitted BAMs (Bowtie2 uses `M`). The plan
  does not call this out. Recommend a one-sentence note in SPEC §7.4
  rev 3.
- **EC-4.** R2 entirely before R1 in an OT pair (theoretically violates
  "R1 is upstream" — would be FR=False orientation, or "outie" geometry).
  In practice the upstream tool (Bismark aligner) ensures FR orientation,
  so this can't happen. The Perl source doesn't guard against it either.
  Worth one assertion at the Rust layer? Probably not — pre-existing
  invariant.
- **EC-5.** R2 with no methylation calls (empty Vec) — plan §3.2 covers this.
  ✓
- **EC-6.** R2 with all calls at exactly `r1_ref_end` — boundary cluster.
  Subsumed by EC-7 below.
- **EC-7.** R2 calls that include a position == r1_ref_end + 1 (just past the
  boundary). Plan §9.1 covers this. ✓

**Recommendation:** make the `30S100M` soft-clip fixture a *required*
addition to §5.3.3 (currently §11-R3 says "verify with one synthetic test
fixture during impl" — promote to a named test).

---

## 5. Alternatives Considered

The plan does not enumerate alternatives. The fix is small and forced —
once the diagnostic is correct, the predicate flip is the unique correct
change. Three alternatives worth mentioning for completeness:

### 5.1 Mirror Perl's early-return iteration exactly

Instead of `Vec::retain`, write an explicit loop with `break`:
```rust
let mut out = Vec::with_capacity(r2_calls.len());
for c in r2_calls {
    if c.ref_pos <= r1_ref_end { break; }   // OT
    out.push(c);
}
```
**Pros:** byte-for-byte semantic mirror of Perl. Easier to argue
correctness in code review.
**Cons:** Depends on the monotonicity invariant (§1.4 above) being
correct — which `retain` doesn't depend on. `retain` is more *robust*
against pathological CIGARs.

**Verdict:** `retain` is the right call given monotonicity holds for
well-formed CIGARs. The plan is correct to keep `retain`.

### 5.2 Defer the SPEC §7.4 rewrite to a separate doc PR

Instead of bundling the SPEC update with the code fix, ship a doc-only PR
first. **Verdict:** No — the plan correctly bundles. A transient state where
SPEC says one thing and code does another is worse than a slightly larger
PR. The plan's A7 assumption is right.

### 5.3 Bundle #863 (parallel ordering) into the same PR

Both bugs surface from the same Phase H harness; bundling could shorten the
total time-to-byte-identity. **Verdict:** No — the plan correctly defers.
#863's diagnostic locality (likely in `src/parallel.rs::extract_pe_parallel`'s
collector ordering) is entirely separate from #862's. Bundling would couple
test failures and slow down implementation.

---

## 6. Action Items

### Critical (must address before implementation triggers)

- **C-1.** Replace plan §5.3.1's incomplete enumeration with a **full
  grep-driven list** of every test in `pe_phase_c.rs` + `pe_phase_c_smoke.rs`
  that uses `drop_overlap` or `--no_overlap` (default). My count: 9 tests
  minimum (see §4.1). Each one's NEW expected `kept[]` values must be
  pre-computed in the plan, not deferred to "audit during impl".
- **C-2.** Specifically flag and rename the three tests that encode the
  wrong polarity in their *names*:
  - `drop_overlap_fully_overlapping_pair_keeps_calls_inside_r1_span` →
    `drop_overlap_fully_overlapping_pair_drops_all_r2_calls`.
  - `extract_pe_with_no_overlap_drops_r2_calls_past_r1_end` →
    `extract_pe_with_no_overlap_drops_r2_calls_in_overlap_keeps_unique_region`
    (or similar).
  - `drop_overlap_disjoint_pair_drops_all_r2_calls_downstream_of_r1_end` →
    `drop_overlap_disjoint_forward_pair_keeps_all_r2_calls` (plan already
    catches this one).
- **C-3.** Rework the smoke test fixture (`pe_phase_c_smoke.rs::write_pe_directional_bam`)
  to space R2 *past* R1 (e.g., `r2_start = r1_start + 5`) so the smoke
  exercises the post-fix "kept" path rather than passing by boundary
  coincidence with stale-comment rationale.

### Important (should address)

- **I-1.** Add the monotonicity-equivalence sentence to §3.1 (per §1.4
  above) — `Vec::retain` ≡ Perl-early-return because R2's iteration is
  monotonic on `ref_pos`.
- **I-2.** Add the §11-R3 soft-clip test (`30S100M` R1 CIGAR) to §5.3 as a
  *required* test, not optional. This is the highest-likelihood edge case
  not currently exercised by the existing CIGAR-aware tests.
- **I-3.** Add a one-sentence remark to SPEC §7.4 rev 3 about the
  `=`/`X` op divergence (Rust counts them in reference_span; Perl's
  $MDN_count does not — but Bismark BAMs never have them). Pre-existing
  divergence; just call it out.
- **I-4.** Clarify in plan §2.2 that the rev-2 misciting of Perl lines
  2905/2989 was a *documentation* error (those predicates are
  byte-identical to 3576/3657 in the default branch) — the real bug is
  overlooking the coordinate pre-mutations at lines 2401 and 2415-2416.
  This framing is more accurate.

### Optional (nice to have)

- **O-1.** Inline boundary tests in `src/overlap.rs` (plan §5.3.2 already
  flags this as optional — agreed).
- **O-2.** Add a `T` ("not yet sampled") assertion in the harness output:
  R2 unique-region calls should appear with the *expected ref_pos
  distribution* (a sanity histogram), not just total count match. The
  current plan asserts only line counts and totals — equal counts with
  shifted positions would still be a bug.
- **O-3.** Plan §11 (Remaining risks) is good. Consider adding R5: "If
  Perl uses `>=`/`<=` with a coordinate transformation I missed, the
  fixed Rust could still be off-by-one." This is *my* sanity-check —
  having done it independently, I'm confident the answer is no, but
  documenting the audit closes the loop.

---

## 7. Specific Source-Level Confirmations (independent verification)

For the record, I verified the following by reading the source directly,
**not** by trusting the plan's transcription:

| Claim | Source | Verdict |
|---|---|---|
| `$start_read_2 += $MDN_count_2 - 1` at OT branch | `bismark_methylation_extractor:2401` | ✓ |
| `$end_read_1 = $start_read_1` at OB branch BEFORE `$start_read_1 += $MDN_count_1 - 1` | `bismark_methylation_extractor:2415-2416` | ✓ (order is critical and confirmed) |
| OT R2 predicate `if ($start - $index + $pos_offset <= $end_read_1) { return; }` at default branch | `bismark_methylation_extractor:3657` | ✓ |
| OB R2 predicate `if ($start + $index + $pos_offset >= $end_read_1) { return; }` at default branch | `bismark_methylation_extractor:3576` | ✓ |
| Same predicates also at merge_non_CpG branch | `bismark_methylation_extractor:2905, 2987` | ✓ (byte-identical to default branch) |
| R1 dispatch passes `$no_overlap=0, $end_read_1=0` (no overlap on R1) | `bismark_methylation_extractor:2436, 2444` | ✓ |
| R2 dispatch with `strand='-'` for OT R2; `strand='+'` for OB R2 | `bismark_methylation_extractor:2440, 2448` | ✓ |
| `$no_overlap=1` is default for PE | `bismark_methylation_extractor:1215-1225` | ✓ |
| `CigarExt::reference_end(start) = start + reference_span() - 1` | `bismark-io/src/cigar.rs:182-187` | ✓ 1-based inclusive |
| `MethCall::ref_pos = alignment_start + ref_offset` | `bismark-io/src/record.rs:293` | ✓ 1-based |
| Current buggy code: OT `retain(|c| c.ref_pos < r1_ref_end)`, OB `retain(|c| c.ref_pos > r1_ref_start)` | `bismark-extractor/src/overlap.rs:54, 60` | ✓ Confirmed reversed polarity. |
| Phase F tests use only relative comparisons (parallel vs sequential), no hardcoded byte snapshots | `tests/parallel_phase_f.rs` (all 16 tests) | ✓ Pass without modification once polarity flips on both sides. |

---

## 8. Verdict

**Plan is APPROVED for implementation with the Critical and Important action
items folded into a rev 1.** The Critical items (test-enumeration
completeness, smoke-test fixture rework) are non-trivial — they should land
in the plan before code is written, not as in-flight discoveries. The
polarity derivation and code fix are correct.

The plan demonstrates strong rigor in its Perl-source citations and its
edge-case enumeration. The gaps I found are all about *test coverage
completeness*, not about the *correctness of the fix*. Once those gaps are
filled, this PR should land cleanly and the harness re-run on oxy should
report the expected ~188M Cs analysed.

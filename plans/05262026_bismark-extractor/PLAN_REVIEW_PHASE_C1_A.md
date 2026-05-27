# Plan Review — Phase C.1 (Reviewer A)

**Plan under review:** `plans/05262026_bismark-extractor/PHASE_C1_PLAN.md` rev 0
**Reviewer:** A (independent, fresh context)
**Verdict:** **Approve with fixes.** The polarity diagnosis and core 4-LOC code fix are correct. The plan contains two concrete defects that must be fixed before implementation: (1) the Perl line citations for the default-mode strand-specific output path still point at the wrong code section, and (2) the test-rewrite section omits at least one integration test whose assertions currently bake in the wrong polarity. Plus a handful of smaller cleanups noted below.

---

## 1. Logic Review

### 1.1 Polarity derivation — independently re-derived

I re-derived the Perl semantics from scratch, **without** trusting the plan's transcriptions, by reading `bismark_methylation_extractor`:

**Transformations (lines 2398-2417):**
- `$strand eq '+'` branch (OT/CTOB pair, R1 forward, R2 reverse): line 2400 sets `$end_read_1 = $start_read_1 + $MDN_count_1 - 1` (= R1's 1-based inclusive rightmost reference position). Line 2401 mutates `$start_read_2 += $MDN_count_2 - 1` so `$start_read_2` becomes **R2's rightmost ref position**.
- `$strand eq '-'` branch (OB/CTOT pair, R1 reverse, R2 forward): line 2415 sets `$end_read_1 = $start_read_1` (= R1's original leftmost) BEFORE line 2416 mutates `$start_read_1 += $MDN_count_1 - 1` (R1's rightmost). `$start_read_2` is untouched, so it remains R2's leftmost.

**Dispatch (lines 2434-2449):**
- OT/CTOB pair: R2 dispatched with `$strand='-'` (R2 is reverse-oriented).
- OB/CTOT pair: R2 dispatched with `$strand='+'` (R2 is forward-oriented).

**R2 predicate (default 4-context strand-specific output — the path the harness exercises):**

The actual predicates the user's `bismark_methylation_extractor` (no `--comprehensive`, no `--merge_non_CpG`) executes live in `print_individual_C_methylation_states_paired_end_files`, inside the `### strand-specific methylation output` branch, at:

- **Line 3744-3747** (`$strand eq '+'`, i.e. R2 of an OB/CTOT pair iterating upward via `$start + $index`):
  ```perl
  if ($start+$index+$pos_offset >= $end_read_1) { return; }
  ```
  where `$start` = R2's leftmost and `$end_read_1` = R1's **leftmost** (per line 2415).
  Substituting: drop R2 call if `r2_pos >= r1_ref_start`. **Keep predicate: `r2_pos < r1_ref_start`.** ✅ matches plan §3.1.

- **Line 3825-3828** (`$strand eq '-'`, i.e. R2 of an OT/CTOB pair iterating downward via `$start - $index`):
  ```perl
  if ($start-$index+$pos_offset <= $end_read_1) { return; }
  ```
  where `$start` = R2's rightmost (line 2401) and `$end_read_1` = R1's **rightmost** (line 2400).
  Substituting: drop R2 call if `r2_pos <= r1_ref_end`. **Keep predicate: `r2_pos > r1_ref_end`.** ✅ matches plan §3.1.

**Conclusion:** The plan's keep predicates are correct. The bug diagnosis is correct. The 4-LOC code fix is correct.

### 1.2 ❌ **The plan cites the WRONG Perl lines** (Important)

The plan §2.1 + §5.1 says the default strand-specific output predicates are at lines **3575-3578** (OB/CTOT) and **3656-3660** (OT/CTOB). I verified these line numbers myself and they are **not** the default strand-specific path — they are the `### THIS IS THE DEFAULT 3-CONTEXT OUTPUT ###` section (lines 3559-3729), which is the **`--comprehensive`** branch (single-file CpG_context / CHG_context / CHH_context output).

The actual default 4-context strand-specific output (CpG_OT/_OB/_CTOT/_CTOB) — which is what the user's harness invokes — lives at lines **3744-3747** (strand `+`) and **3825-3828** (strand `-`), inside the `### strand-specific methylation output` branch starting at line 3732.

The predicates are byte-identical across the four branches (merge_non_CpG, default-3-context-comprehensive, strand-specific, and the 4-context-comprehensive section around line 4065+), so the polarity conclusion is unaffected. But the SPEC rev 3, the module doc, and the in-code comments will memorialize the wrong line numbers if shipped as written. This is the same class of defect (mis-citation of the active Perl section) that the plan itself attributes to rev 2.

**Fix:** Replace every citation of `3575-3578` / `3656-3660` with `3744-3747` / `3825-3828` in:
- Plan §2.1 last bullet group.
- Plan §3.1 §5.1 §5.2 inline comments.
- SPEC §7.4 rev 3 (plan §5.1.2).
- `src/overlap.rs` module doc + per-branch inline comments (plan §5.2).
- Test docstring comments where they cite Perl lines (`pe_phase_c.rs` headers).

It is fine to **mention** that the polarity is identical to the other three sections (a sentence in the SPEC) — but the load-bearing citation must point at the section the harness actually runs.

### 1.3 Boundary inclusivity

Plan claim: Perl drop predicates are inclusive (`<=` / `>=`), so Rust keep predicates are strict (`>` / `<`). Verified by direct read of lines 3745 (`>=`) and 3826 (`<=`). ✅

### 1.4 Coordinate-system assumptions

- `MethCall::ref_pos: u32` — verified in `rust/bismark-extractor/src/call.rs:31`. ✅
- `CigarExt::reference_end(start)` returns `start + reference_span - 1` (1-based inclusive last reference position) — verified in `rust/bismark-io/src/cigar.rs:182-187` and reinforced by the test `reference_end_inclusive_1based` (`cigar.rs:269-272`). ✅
- `reference_span` includes `Match`, `Deletion`, `Skip`, `SequenceMatch`, `SequenceMismatch` and excludes `SoftClip`, `Insertion`, `HardClip`, `Pad` — matches Perl's `M | D | N` count (`bismark_methylation_extractor:2391, 2397, 2412`). ✅
- `pair.r1().alignment_start()` returns the BAM POS field (leftmost mapped reference position, 1-based). Identical semantics to Perl's `$start_read_1` before any mutation. ✅

The plan's coordinate-system assumptions A1-A4 are sound. I do not flag any.

### 1.5 Edge cases — what the plan covered, and what it missed

Plan §3.2 covers nine cases. I verify the proposed behaviour for each:

| Case | Plan's claim | Reviewer-A check |
|---|---|---|
| Non-overlap forward (R2 past R1) | All R2 kept | ✅ correct (`r2_pos > r1_ref_end` true for all) |
| Non-overlap reverse (R2 before R1) | All R2 kept | ✅ correct |
| Fully overlapping (R2 ⊆ R1) | All R2 dropped | ✅ correct |
| Partial overlap forward | Overlap dropped, tail kept | ✅ correct |
| Partial overlap reverse | Head kept, overlap dropped | ✅ correct |
| Boundary r2_pos == r1_ref_end | Dropped | ✅ matches Perl `<=` inclusive |
| Boundary r2_pos == r1_ref_start | Dropped | ✅ matches Perl `>=` inclusive |
| Empty R2 | No-op | ✅ |
| Missing alignment_start | InternalError | ✅ unchanged |

**Missed edge cases — recommend adding to §3.2 and to the unit-test set:**

1. **R2 entirely before R1 in an OT pair (geometrically inverted)** — possible if the BAM contains an `FR`-flipped pair that survives upstream filtering. Reviewer-A check: `drop_overlap` will compute `r1_ref_end` from R1's CIGAR; every R2 call is `< r1_ref_end`; with corrected polarity (`> r1_ref_end`), all R2 calls **dropped**. Perl: same — iteration starts from R2's rightmost downward (`$start - $index`), every position is `<= r1_ref_end`, return fires immediately and nothing emits. Agreement ✅. Recommend a one-line note in §3.2.

2. **R2 entirely past R1 in an OB pair** — symmetric of the above, same conclusion. Reviewer-A check: corrected polarity drops everything. Perl matches.

3. **R1 with CIGAR including `N` (skip op, e.g. spliced BS-RNA-seq)** — `reference_span` includes Skip; Perl `$MDN_count` includes `N`. Both compute the same `r1_ref_end`. Recommend one synthetic-fixture test with R1 CIGAR `50M1000N50M` to pin this — fast (no real-data dependency).

4. **R1 with 5'-soft-clip (e.g. `10S100M`)** — only a defensive guard since Bismark's own alignments do not soft-clip. POS in BAM is leftmost mapped base; `reference_span` excludes `SoftClip`. So `r1_ref_start` = POS, `r1_ref_end` = POS + 100 - 1. Perl matches (`$MDN_count` excludes 'S'). Recommend one fixture test as a defensive smoke; the plan's Remaining-Risks R3 already flags it but doesn't add a test.

5. **R1 with 3'-soft-clip (e.g. `100M10S`)** — same analysis, `reference_end` excludes the 3' clip from the span. Fine.

6. **R1 entirely deleted (CIGAR `100D`)** — pathological but valid SAM. `reference_span = 100`; `r1_ref_end = start + 99`. Polarity is fine; the question is whether `MethCall::ref_pos` ever exists for such a read (R1 has no XM bases). Probably handled by the XM-byte iterator upstream; flag as a non-issue but doesn't need a test.

7. **CIGAR with a leading insertion `5I100M`** — Insertions don't consume reference. `reference_span = 100`, `r1_ref_end = start + 99`. ✅ matches Perl (`$MDN_count = 100`).

The plan's §3.2 is **almost complete**. Recommend adding #3 (N-op) as a unit test and #2 (geometric inversion) as a documented edge case; the others are non-blocking.

### 1.6 Test-surface accounting

The plan §5.3 enumerates three test surfaces (`pe_phase_c.rs`, `pe_phase_c_smoke.rs`, `parallel_phase_f.rs`) but **omits at least one integration test** in `pe_phase_c.rs` whose assertions currently encode the wrong polarity:

- **`extract_pe_with_no_overlap_drops_r2_calls_past_r1_end`** (`pe_phase_c.rs:751-787`). This is an end-to-end integration test that asserts:
  - "R2 call inside R1 span (103 < 104) kept" — under corrected polarity, must **DROP** (103 ≤ 104).
  - "R2 call past r1_ref_end (105 >= 104) dropped" — under corrected polarity, must **KEEP**.
  - "R2 call past r1_ref_end (106 >= 104) dropped" — must **KEEP**.

  The test name itself is then a misnomer ("drops past r1_end" but it actually keeps unique-region calls) and should be renamed, e.g. `extract_pe_no_overlap_drops_r2_in_overlap_region_keeps_unique_region`.

- **`extract_pe_with_include_overlap_keeps_r2_overlap_calls`** (`pe_phase_c.rs:727-748`). This one passes `--include_overlap` so `drop_overlap` is not called; its assertions are independent of polarity and should remain unchanged. ✅

- **`drop_overlap_fully_overlapping_pair_keeps_calls_inside_r1_span`** (`pe_phase_c.rs:359-383`). Currently asserts all three R2 calls (105, 120, 148) are kept because each `< r1_ref_end=149`. Under corrected polarity each is `<= 149` → all **DROPPED**. Test name + assertions both need to flip; the new name should be e.g. `drop_overlap_fully_overlapping_pair_drops_all_r2_calls_in_overlap`.

- **`drop_overlap_forward_pair_drops_r2_at_or_after_r1_end`** (`pe_phase_c.rs:287-307`). Name says "drops_at_or_after" but under corrected polarity it should drop "at_or_before". The plan §5.3.1 claims this name "remains correct" — **wrong**. R1 ends at 149 (inclusive). Current code drops 149, 150 (says "at-or-after-149") and keeps 148. Corrected code drops 148, 149 (≤ 149) and keeps 150. The new behaviour is "drops at or before", not "at or after". **Test name and asserted-kept value both need to invert.**

- **`drop_overlap_reverse_pair_drops_r2_at_or_before_r1_start`** (`pe_phase_c.rs:309-329`). Symmetric. Currently keeps 201, drops 199 + 200. Corrected: drops 200 + 201 (`>= 200`), keeps 199. Rename to `drop_overlap_reverse_pair_drops_r2_at_or_after_r1_start` and flip asserts.

- **`drop_overlap_disjoint_pair_drops_all_r2_calls_downstream_of_r1_end`** (`pe_phase_c.rs:331-357`). The plan flags this one for rename and invert. ✅ correctly identified.

- **`drop_overlap_with_r1_indel_uses_reference_end`** (`pe_phase_c.rs:385-406`). R1 = 50M2D50M at 100 → r1_ref_end = 201. Calls at 200, 201, 202. Current keeps 200, drops 201, 202. Corrected: drops 200, 201 (≤ 201), keeps 202. Assertion flip required.

- **`drop_overlap_with_r1_end_deletion`** (`pe_phase_c.rs:408-430`) — same flip pattern.

- **`drop_overlap_with_r1_insertion_shifts_read_pos_only`** (`pe_phase_c.rs:432-485`) — same flip pattern.

**Action:** The plan's §5.3.1 must enumerate **every** test that currently asserts the wrong polarity, not just the four named ones. There are at least **eight** tests in `pe_phase_c.rs` whose assertions flip, plus name renames for at least three. Plus the integration test in the inner `tests {}` module. The plan should append a precise mapping table:

| Current test name | New behaviour | Rename to |
|---|---|---|
| `drop_overlap_forward_pair_drops_r2_at_or_after_r1_end` | keeps 150, drops 148+149 | `drop_overlap_forward_pair_drops_r2_at_or_before_r1_end` |
| `drop_overlap_reverse_pair_drops_r2_at_or_before_r1_start` | keeps 199, drops 200+201 | `drop_overlap_reverse_pair_drops_r2_at_or_after_r1_start` |
| `drop_overlap_disjoint_pair_drops_all_r2_calls_downstream_of_r1_end` | keeps all 3 | `drop_overlap_disjoint_forward_pair_keeps_all_r2_calls` |
| `drop_overlap_fully_overlapping_pair_keeps_calls_inside_r1_span` | drops all 3 | `drop_overlap_fully_overlapping_pair_drops_all_r2_calls` |
| `drop_overlap_with_r1_indel_uses_reference_end` | keeps 202, drops 200+201 | (name OK, only asserts flip) |
| `drop_overlap_with_r1_end_deletion` | keeps 152, drops 150+151 | (name OK, only asserts flip) |
| `drop_overlap_with_r1_insertion_shifts_read_pos_only` | keeps 200, drops 198+199 | (name OK, only asserts flip) |
| `extract_pe_with_no_overlap_drops_r2_calls_past_r1_end` | keeps 105+106, drops 103 | `extract_pe_with_no_overlap_drops_r2_overlap_keeps_unique` |

(The smoke file `pe_phase_c_smoke.rs` should be audited the same way; I did not read it line-by-line.)

### 1.7 Phase F byte-identity tests

Plan claim: `tests/parallel_phase_f.rs` will pass without modification because Phase F's invariant is parallel ≡ sequential (both running `drop_overlap`). I read the relevant assertion blocks (`parallel_phase_f.rs:709-726`). They compare:
- `decoded` (decompressed parallel output) vs `plain` (sequential output), file by file.
- Splitting reports after path normalization.

There are no hardcoded "expected" byte snapshots — only equivalence assertions. So **the plan is correct on this point.** ✅

I did spot-check the PE fixtures (`pair1`/`pair2`/`pair3` at `parallel_phase_f.rs:128-156`). They use overlapping pairs (R1 at 100, R2 at 110; R1 at 400, R2 at 410; R1 at 700, R2 at 710). With the polarity flip, the output content changes (more R2 calls kept), but both the parallel and sequential code paths change identically. Equivalence holds. ✅

### 1.8 The "non-overlap-keep-all-R2" semantic — sanity check against `--no_overlap` docs

The plan's §2.2 argues: "R2's calls in the overlap region (where R1 already has data) are dropped; R2's calls in R2's unique region are kept." This matches the docstring for `--no_overlap`: *"avoids scoring overlapping methylation calls twice"*.

I cross-checked the Perl POD docs (`bismark_methylation_extractor:5860+`):

```
--no_overlap             For paired-end reads it is theoretically possible
                         that Read 1 and Read 2 overlap. This option avoids
                         scoring overlapping methylation calls twice (only
                         methylation calls of read 1 are kept for overlapping
                         regions). This option is on by default for paired-
                         end input.
```

> *"only methylation calls of read 1 are kept for overlapping regions"*

Confirmed. The biological intent is "drop R2 in overlap, keep R2 in unique region". The current Rust code does the **opposite** (keeps R2 in overlap, drops R2 in unique region). The plan's fix aligns Rust with the documented `--no_overlap` semantic. ✅

This also retroactively invalidates the rev-3 "edge case" paragraph in SPEC.md §7.4 (line 383): *"if R1 and R2 don't overlap at all because R2 is wholly downstream of `r1_ref_end` (typical "large insert" forward pair), **all R2 calls are dropped**, NOT preserved as a no-op."* That paragraph also encodes the wrong polarity (it claims early-exit drops everything past r1_ref_end, but actually Perl's iteration starts FROM the unique region and stops on entering the overlap — so the unique region IS emitted before the `return`). The plan §5.1 step 3 correctly flags this for rewrite.

### 1.9 Scope decisions (#863/#864/#865)

The plan keeps #863 (parallel record ordering), #864 (splitting report format), and #865 (empty CTOT/CTOB file emission) explicitly out of scope. This is the right call:

- Each is **orthogonal** to the polarity bug — they affect file structure / file ordering / file presence, not call content.
- Bundling would expand the diff, complicate the dual-reviewer pass, and obscure the regression-guard test set.
- The merge gate ("Rust's total-C's-analysed and per-file line counts equal Perl's") works without the other three, as the plan correctly notes (§9.3 "still FAIL for raw byte-identity until #863 lands — that's expected").

I would **only** push back on this scope if there were a hidden coupling — e.g., if #864's report format depended on accurate counts. There isn't: the splitting-report format issue is independent of the call-volume issue. ✅ scope split is fine.

---

## 2. Assumptions

Plan §8.1 lists assumptions A1-A6 (Perl-source facts) and A7-A10 (plan-specific). My audit:

- **A1.** `$MDN_count_X` = M+D+N op-lengths. ✅ verified at lines 2391, 2397, 2412.
- **A2.** `CigarExt::reference_end(start) == start + MDN_count - 1`. ✅ verified at `cigar.rs:182-187`, test at `:269-272`.
- **A3.** `MethCall::ref_pos` = 1-based reference position. ✅ verified at `call.rs:30-31` ("1-based reference position. From `AlignedXmCall::ref_pos`.").
- **A4.** No-overlap predicate applied only to R2. ✅ verified at lines 2436, 2440, 2444, 2448 — R1 calls pass `$no_overlap=0` and `$end_read_1=0`; R2 calls pass the live values.
- **A5.** `$no_overlap=1` is the default. ✅ verified at line 1221-1226 (the `else` branch of `if ($include_overlap)`).
- **A6.** `BismarkPair::pair_strand()` returns OT/OB/CTOT/CTOB from R1's XG tag. Not re-verified here but the Phase C tests exercise this; flagged as a residual risk by the plan itself (R2). I concur with the plan's mitigation: even if classification were wrong, the symptom would be "wrong unique region kept" not "all calls dropped", and the 10M PE harness re-run would catch it.

**Hidden assumption not explicitly stated**: The plan assumes `pos_offset` (Perl's per-base CIGAR adjustment in `check_cigar_string`) is already baked into `MethCall.ref_pos` in Rust. I.e., when Perl's predicate computes `$start + $index + $pos_offset`, Rust's equivalent is just `c.ref_pos` (the running offset is pre-applied during XM iteration). If that's not true, the polarity flip alone is insufficient — Rust would also need a per-call adjustment. From my read of `call.rs:30-31` ("1-based reference position. From `AlignedXmCall::ref_pos`."), this looks correct; `AlignedXmCall::ref_pos` is computed inside `bismark-io`'s XM iterator which applies CIGAR semantics. But the plan should **state** this assumption explicitly as A11 because it's load-bearing: without it the predicate comparison is the wrong identity.

**Recommend adding to §8.1:**
> **A11.** `MethCall::ref_pos` already incorporates any per-call CIGAR position offsets (Perl's `$pos_offset`). The comparison `c.ref_pos {< | >} r1_ref_{start|end}` is therefore the complete predicate; no additional offset arithmetic is required inside `drop_overlap`. Verified by the byte-identity of SE outputs (Rust SE emits the same ref_pos values Perl does, including for CIGARs with InDels/soft-clips).

---

## 3. Efficiency

Plan §6: O(R2_calls) per pair, `Vec::retain` in-place. ✅

One nuance worth flagging: under the corrected polarity, the **common case shifts**. Today (buggy), most pairs have a moderate-to-large overlap, so `retain` typically drops a large fraction and shifts most surviving elements. Post-fix, in the typical FR PE pair geometry (R1=R2 length ≈ 100bp, overlap ≈ 50-80bp depending on insert), `retain` will keep a smaller fraction. Memory-wise both are in-place; CPU-wise the retain predicate runs once per call regardless. No real change. ✅

Plan §6's "throughput on real WGBS should be slightly better because fewer shifts on average for partial-overlap pairs" is plausible but unmeasured. The harness re-run will reveal it; mark as informational, not a planning concern.

---

## 4. Validation Sufficiency

### 4.1 Unit-level (plan §9.1) — six fixtures

The six fixtures in plan §9.1 cover non-overlap, partial overlap (forward only), and two boundary cases. **Gaps:**

1. **No partial-overlap reverse fixture.** Plan covers `OT partial overlap` but not the symmetric `OB partial overlap`. Recommend adding: R2=[150,213] (5' end of R1's range), R1=[200,263]. R2 has calls at 195, 199, 200, 201 → keep 195, 199; drop 200, 201 (≥ r1_ref_start=200). Critical for catching polarity asymmetries.

2. **No N-op (skip) fixture for R1's CIGAR.** Recommended in §1.5 above; small additional test.

3. **No soft-clip-on-R1 fixture.** Recommended in §1.5 above as a defensive guard.

4. The plan's "Read `.9` regression guard" fixture (large gap, ~7bp) is good, but the **symmetric OB version** (R2 leftmost, R1 7bp downstream, R2 carries calls in its unique region) should also exist. The polarity asymmetry is the bug; tests should be symmetric.

### 4.2 Integration-level (plan §9.2)

Plan §9.2 lists three checks: Phase F invariant, Phase C tests, clippy+fmt. All ✅ — but as noted in §1.6, the plan undercount the Phase C test surface that needs assertion flips.

### 4.3 Real-data byte-identity (plan §9.3 — oxy harness)

The expected post-fix numbers in §9.3 are well-anchored:
- Total C's: 100,652,488 → 188,123,599 (matches Perl). Reviewer-A check: ratio is 1.87×, matches the diagnostic write-up.
- Per-file line counts and sizes match Perl exactly.
- M-bias.txt 10,408 → 11,443 bytes (matches Perl).

This is a strong merge gate, and the plan correctly distinguishes what should match (call content + line counts + M-bias) from what stays broken until #863/#864/#865 land (record ordering, report format, empty-file presence). ✅

### 4.4 Read-`.9` per-pair guard (plan §9.4)

Excellent diagnostic — the pre/post-fix counts for one specific read are concrete enough to be a sharp signal. ✅

### 4.5 Missing validation: M-bias growth pattern

The plan claims M-bias.txt size will grow from 10,408 to 11,443 B (matching Perl). But the M-bias table is **per-cycle**, not per-call. The size depends on whether R2's unique region introduces new read-cycle positions that weren't already present. Since R2's read cycles are 1-N (same as R1's), R2's unique region won't *expand* the cycle range — only *increase the counts* at existing cycles. So the byte count should match Perl's exactly **only if** Perl and Rust agree on which cycles get hit. The plan assumes this; the harness will catch any divergence. **Recommend adding** to §9.3 an explicit assertion that the M-bias.txt **byte-identical** (sha256 match) not just byte-count match. Otherwise a per-cycle drift would silently pass the byte-count check.

### 4.6 Missing validation: per-pair early-exit equivalence

Perl uses early-exit (`return` from the per-call loop). Rust uses `Vec::retain` (processes all calls). These are equivalent **only if** Perl's iteration order is monotonic in `ref_pos`. For OT R2 (`$strand='-'`, iter `$start - $index`), it is monotonically decreasing. For OB R2 (`$strand='+'`, iter `$start + $index`), it is monotonically increasing. Both monotonic → early-exit and retain emit the same set. ✅ implicit; the plan doesn't state this, but the harness re-run will catch any pathological case. **Optional note** to add to SPEC §7.4 rev 3.

---

## 5. Alternatives

### 5.1 Could the fix be a one-liner via a sign flip?

```rust
r2_calls.retain(|c| c.ref_pos > r1_ref_end);  // was `<`
```
vs
```rust
r2_calls.retain(|c| c.ref_pos >= r1_ref_end + 1);
```

The plan picks the former (strict-`>`), which is correct and clearer. The alternative is functionally equivalent but obscures the boundary semantics. ✅

### 5.2 Could the fix preserve Perl's early-exit semantics for speed?

Sorting `r2_calls` by `ref_pos` and short-circuiting would mirror Perl exactly. But:
- `extract_calls` produces calls already in ref_pos order (monotonic per XM iteration, possibly reverse for strand `-`).
- `Vec::retain` is single-pass O(N) without short-circuit; sorting + early-exit doesn't save anything here.

The plan is right not to pursue this. ✅

### 5.3 Could we bundle #863/#864/#865 in a "fix all extractor byte-identity gaps" PR?

I argued no in §1.9 — different defects, different test surfaces, different review concerns. The plan's choice to ship #862 alone is correct.

### 5.4 Inline tests in `src/overlap.rs` (plan §5.3.2)

Plan marks "optional, not blocking". I concur — `tests/pe_phase_c.rs` covers the same ground at acceptable granularity. Skip unless implementation time is cheap.

---

## 6. Action Items

### Critical (must fix before approval)

- **C1.** **Replace all Perl-line citations of `3575-3578` / `3656-3660` with `3744-3747` / `3825-3828`** throughout the plan, SPEC, module doc, and code comments. The current citations point at the comprehensive (`--comprehensive` / 3-context single-file) branch, not the default 4-context strand-specific branch that the harness exercises. Polarity is identical across branches so the fix itself is unaffected, but the load-bearing citation must point at the active code path. (§1.2)
- **C2.** **Enumerate all 8+ Phase C tests** whose assertions invert under the corrected polarity, not just the four the plan calls out. At minimum: `drop_overlap_forward_pair_drops_r2_at_or_after_r1_end` (rename + invert), `drop_overlap_reverse_pair_drops_r2_at_or_before_r1_start` (rename + invert), `drop_overlap_fully_overlapping_pair_keeps_calls_inside_r1_span` (rename + invert), `drop_overlap_with_r1_indel_uses_reference_end` (invert), `drop_overlap_with_r1_end_deletion` (invert), `drop_overlap_with_r1_insertion_shifts_read_pos_only` (invert), and the integration test `extract_pe_with_no_overlap_drops_r2_calls_past_r1_end` (rename + invert). Add a precise mapping table to §5.3.1. (§1.6)

### Important (should fix; not strictly blocking)

- **I1.** Add **A11** (or extend A3) to §8.1 stating explicitly: *"`MethCall::ref_pos` already incorporates Perl's `$pos_offset` (CIGAR-derived per-base adjustment), so the `drop_overlap` predicate compares the same quantity Perl compares."* This is the implicit assumption that makes the fix work for InDel-bearing reads; it deserves to be stated. (§2)
- **I2.** Add an **OB partial-overlap fixture** to §9.1 (R2 leftmost, R1 downstream, R2 calls split across the boundary). The current §9.1 covers forward-direction partial overlap but the symmetric reverse case is missing. (§4.1)
- **I3.** Add an **N-op (skip op) R1 CIGAR fixture** — small, fast, covers a real BS-RNA-seq edge case. (§1.5)
- **I4.** Add a **soft-clipped R1 CIGAR fixture** (e.g. `30S100M`) as a defensive guard. The plan's R3 risk flags this but doesn't add the test. (§1.5)
- **I5.** Strengthen the §9.3 M-bias check from "byte count matches" to "**sha256 matches**". A byte-count match is insufficient — per-cycle drift could silently pass. (§4.5)

### Optional

- **O1.** Mention in SPEC §7.4 rev 3 that Perl's monotonic per-call iteration order makes early-exit equivalent to `Vec::retain`. (§4.6)
- **O2.** Note in §3.2 that the polarity bug, having been symmetric in both branches, is the reason Phase F's parallel ≡ sequential invariant didn't catch it. Useful future-proofing context for whoever extends the test suite.
- **O3.** Update the rev-3 "edge case" paragraph at SPEC.md:383 to also fix the incorrect claim about early-exit dropping unique-region calls — the plan §5.1 step 3 already covers this; just making sure it doesn't get lost in implementation.
- **O4.** Inline `src/overlap.rs` tests (plan §5.3.2): skip unless implementation time is cheap; not blocking.

---

## 7. Summary

The polarity diagnosis is correct. The 4-LOC code fix is correct and minimal. The plan's biggest defect is **citing the wrong Perl section** (a repeat of the same class of error rev 2 made, ironically) and **undercount the test surface** that needs flipping. Both are mechanical fixes during plan rev 1. With C1 + C2 addressed plus the I1-I5 cleanups, this plan is implementation-ready.

The merge gate (per-file line counts equal Perl's, total-C-count equal Perl's) is sharp, well-defined, and resistant to false-positives from #863/#864/#865. The harness re-run is the right validation choice.

Polarity verified by direct re-derivation from Perl source, lines 2398-2417 (transformations), 3744-3747 / 3825-3828 (default 4-context predicates — the path the harness exercises), 1215-1226 (`--no_overlap` default), and `--no_overlap` docstring. Boundary inclusivity verified. Coordinate-system assumptions verified. Phase F equivalence preservation verified.

Recommend **rev 1 with C1+C2 fixes**, then dual code-reviewer pass post-implementation.

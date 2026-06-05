# PLAN_REVIEW_A — Phase 2b: HISAT2 paired-end (read-1 `ZS` asymmetry)

- **Reviewer:** A (independent; no shared state with Reviewer B)
- **Date:** 2026-06-05
- **Plan reviewed:** `plans/06052026_bismark-aligner-v1x/phase2b-hisat2-pe/PLAN.md` (rev 0)
- **Worktree/HEAD:** `rust/aligner-v1x` @ `376a6d9` (shipped 2a, SE byte-identical)
- **Scope:** ONLY the 2b deltas (read-1 `ZS` mask + PE gate). 2a surface NOT re-litigated.

## Verdict

**APPROVE with two Important findings (test-coverage gaps) and several Optional clarifications.** The single new production change — masking `sb1 = None` for HISAT2 at `merge.rs:598` — is **correct, source-faithful, sufficient, and well-localized**. I verified the Perl asymmetry, the Rust consumer chokepoint, and the downstream MAPQ consequence against the actual source. No Critical findings. The plan is implementation-ready; the Important items are about *test coverage of a second divergence case the mask silently also fixes*, not about the fix itself.

---

## 1. Logic review — the mask is correct and sufficient

### 1.1 The Perl asymmetry is real and exactly as the plan states (verified)

- **PE read-1 loop** (`bismark` 3372–3382): `if AS:i: / elsif XS:i: / elsif MD:Z:` — **confirmed NO `ZS:i:` branch and no `else`**. For HISAT2 read-1 (emits `ZS:i:`, not `XS:i:`), `$second_best_1` stays **undef** → backfilled to `$alignment_score_1` at 3467–3468. ✓
- **PE read-2 loop** (3384–3403): `else { if($bowtie2){XS:i:} else{ZS:i:} }` — read-2 **does** capture `ZS` for HISAT2 (3398). ✓
- **SE loop** (2775–2796): `if AS / elsif ZS / elsif MD / else{ if(bowtie2){XS|ZS} }` — `ZS` is captured at **2780 for ANY aligner**. So SE must NOT be masked. ✓ (matches plan §3.1 and the shipped 2a SE gate.)

### 1.2 The Rust consumer is a single chokepoint (verified exhaustively)

`grep` of every non-test read of `.second_best` in the crate shows the **only** PE-path consumer of `r1.second_best`/`r2.second_best` is `merge.rs:598`:
```
align.rs:104        // parser (sets it)
merge.rs:265,316,330  // check_results_single_end (SE — correctly untouched)
merge.rs:598        // check_results_paired_end (the masked site)
methylation.rs:965, output.rs:1390  // hard-coded `None` constructors (not the decision path)
```
There is **no** separate PE ambiguity-detection, no read-1-alone MAPQ, no `amb_same_thread` site that re-reads `r1.second_best`. The `amb_same_thread` PE branch (L606–610) derives entirely from `sum_second`, which flows from `sb1`/`sb2`. So masking at L598 **covers every downstream use** — answering the prompt's "is there any OTHER place" question: **No.** The fix is complete at one site.

### 1.3 Per-instance coverage (verified)

The PE scan loop (`SCAN_ORDER = [0,3,1,2]`, L515–516) re-binds `let (r1, r2) = (&pair.read1, &pair.read2)` per instance (L533) and re-evaluates the `let (mut sb1, mut sb2) = …` site (L598) per instance. So masking at L598 applies to **every** instance's read-1, not just the first — plan §3.1 bullet "Per-instance" is correct. ✓

### 1.4 The mask + the `sb1.is_some() || sb2.is_some()` gate interact correctly — and fix a SECOND case the plan under-tests

This is the load-bearing interaction the prompt flags. I traced all four mate-tag combinations for HISAT2 PE (mask makes `sb1` source `None`):

| Case (HISAT2) | r1 raw | r2 raw | Perl `sb1`,`sb2` | Rust **masked** `sb1`,`sb2` | Match? |
|---|---|---|---|---|---|
| **A** mate-1 ZS, mate-2 ZS | ZS | ZS | undef→`as1`, `zs2` ⇒ sum_second=`as1+zs2` | `None`→backfill `as1`, `zs2` ⇒ `as1+zs2` | ✓ (plan V2) |
| **B** mate-1 none, mate-2 ZS | — | ZS | undef→`as1`, `zs2` ⇒ `as1+zs2` | `None`→`as1`, `zs2` ⇒ `as1+zs2` | ✓ (plan V4) |
| **C** mate-1 ZS, mate-2 none | ZS | — | undef, undef ⇒ **gate FALSE → no-second-best branch** | `None`, `None` ⇒ **gate FALSE → no-second-best branch** | ✓ (**NOT in plan's V-table**) |
| **D** both none | — | — | gate FALSE | gate FALSE | ✓ (trivial) |

**Case C is the important one.** With the mask, both Perl and Rust hit the **no-second-best** branch (gate false): raw `chr:pos1:pos2` key (`min_max_key=false`, L628) and `sum_second_best = None`. Crucially, `calc_mapq` uses a **different ladder** for `None` (max MAPQ 42, `mapq.rs:29–46`) vs `Some` (max 39, L48+). So **without the mask**, today's Rust would compute `sb1=Some(zs1)`, backfill `sb2=as2`, take the *second-best* branch, store `Some(zs1+as2)`, and emit a MAPQ from the *with-second-best* ladder — a **byte-visible BAM divergence** distinct from Case A. The mask fixes Case C correctly because masking flips the gate to false.

→ The plan's narrative ("read-2 keeps its second-best") and its V2/V4 tests focus on Cases A and B (where the gate stays true). **Case C — mate-1 ZS, mate-2 no-tag — is the case where the gate flips, the branch changes, and the MAPQ ladder changes. It is the highest-information unit test and it is missing from the V-table.** See Important-1.

### 1.5 SE and Bowtie 2 are correctly frozen (verified)

- `check_results_single_end` consumes `rec.second_best` directly (L234) — no `aligner` param, untouched. The SE parser captures `ZS` (align.rs L100–104) matching Perl SE 2780. SE HISAT2 is already gated (2a). ✓
- For `aligner == Bowtie2` the L598 source is unchanged (`r1.second_best`), so PE Bowtie 2 is bit-for-bit identical to today. ✓

---

## 2. Assumptions — validated

- **"PE convert/spawn/merge/methylation/output/report/aux are aligner-agnostic and reused unchanged"** — spot-checked **3** of these against source:
  - **convert.rs**: `grep` for `aligner|hisat2|bowtie2` returns **zero hits** — fully aligner-agnostic. ✓
  - **report.rs `write_report_header`**: the "run with {aligner}" wording uses `h.aligner.name()` (L69) and the **PE vs SE line-order** branches on `paired` (L74) — two orthogonal dimensions, so PE+HISAT2 composes correctly (PE order + "HISAT2" wording). Matches Perl PE 1845–1849 (`if($bowtie2){Bowtie 2} else{HISAT2}`, verified). ✓ — but see Important-2 (no PE+HISAT2 *unit* test exists; only SE-HISAT2 and PE-Bowtie2).
  - **methylation.rs N-CIGAR**: `b'N'` is a handled skip op (L189, L362) in the genomic-seq walker — HISAT2's spliced PE reads extract identically. ✓ (V7 covers this at the gate.)
- **`--pbat ⊕ -f` dies** — confirmed at `bismark` **8156** (`die "...only working with FastQ files..." if ($fasta)`; plan cites 8155, off-by-one, immaterial). No FastA-pbat cell is correct. ✓
- **`--multicore` + `--hisat2` hard-rejected** — confirmed at `config.rs:216`. Single-core PE gate is justified; no PE multicore cell needed. ✓
- **HISAT2 PE both mates carry `AS:i:`/`MD:Z:`** (merge dies otherwise, Perl 3405–3406 / Rust L545–568) — the fake/gate must confirm; plan §8 already flags this. ✓

---

## 3. Validation sufficiency

**Strong overall.** The plan correctly identifies (OQ-2b-3) that a mate-1-`ZS` read may not appear at 10k, so the **V2 unit test is the reliable catch** and the 1M gate is the at-scale confirmation. That reasoning mirrors 2a and is sound. The V2 arithmetic expectation (`sum_second = as1 + zs2 = -6`, NOT `zs1+zs2 = -12`) is **verified correct** against the masked trace in §1.4 Case A. The PE oxy matrix (dir/non-dir/pbat + FastA PE dir/non-dir, 10k+1M, decompressed SAM + `_PE_report.txt` + aux) matches the faithful-port precedent and is sufficient.

**Gaps (→ Important findings):**
1. **No unit test for Case C** (mate-1 ZS, mate-2 no-tag) — the gate-flip + MAPQ-ladder-change case, arguably the *most* byte-visible consequence of the mask and the one least likely to be coincidentally exercised by Case A/B. The 1M gate *might* contain such a pair but cannot be relied on (same logic the plan itself uses for OQ-2b-3).
2. **No unit test for the PE+HISAT2 report header** combination. The code is orthogonal (so low risk), but V6 only checks it at the integration/gate level; a 3-line unit test would freeze it.

**A subtle point the plan should state explicitly (Optional-1):** because the Rust parser unifies `XS:i:` and `ZS:i:` into one `second_best` field (align.rs L100–104), the V2/V3/V4 unit tests' *discriminator is the `aligner` argument passed to `check_results_paired_end`, NOT the tag string in the fixture.* A fixture emitting `ZS:i:-6` and one emitting `XS:i:-6` produce an identical `SamRecord`. So the proposed `mapped_pair_zs` helper is cosmetic; the test that actually proves the mask is "same records, `aligner=Hisat2` → masked vs `aligner=Bowtie2` → unmasked." The plan should make this the explicit test design (it currently implies the tag name carries the signal, which it does not in Rust).

**Could the gate pass while wrong?** I looked for PE-HISAT2 divergences beyond read-1 `ZS`:
- Unmapped PE marker (FLAG 77/141, align.rs L313–316) — aligner-agnostic. ✓
- `--no-mixed --no-discordant` in the pinned PE option string (2a) means HISAT2 emits only proper pairs — same shape as Bowtie 2. ✓
- `--omit-sec-seq` only blanks *secondary* SEQ; Bismark consumes only the primary record, so no effect. ✓
- Spliced N-CIGAR in PE — handled (§2). ✓
I found **no** second divergence beyond the read-1 `ZS` family (Cases A+C). The dual plan-review (2a B-L1) found only this, and the 1M PE gate would surface anything else. Residual risk: **low**.

---

## 4. Efficiency

A single `if aligner == Hisat2` at one merge site; zero hot-path cost. Everything else is reuse. No concern.

---

## 5. Alternatives

- **OQ-2b-1 (`Aligner` vs `bool` param):** Agree with the plan's lean — pass `Aligner` (clarity + minimap2-ready, and minimap2 will likely need its own merge-path branch per SPEC §3.2). One-site gate either way. Non-critical.
- **OQ-2b-2 (mask at merge-entry L598 vs a per-mate parse flag in align.rs):** Strongly agree with **merge-entry**. A parse-time flag would mean threading `aligner` into `SamRecord::parse` and forking the uniform parser (which 2a deliberately kept aligner-agnostic and SE relies on). Merge-entry is the minimal blast radius and leaves SE + Bowtie 2 + the parser byte-frozen. ✓
- **Threading note (Optional-2):** §2 + §4 say the param is "threaded from the two call sites (`lib.rs drive_merge_pe` + `parallel.rs`)." In the actual code there is **exactly one** production call site: `lib.rs:1231` inside `drive_merge_pe`. `parallel.rs` does **not** call `check_results_paired_end` directly — it invokes `crate::process_pe_chunk` (parallel.rs:487), which calls `drive_merge_pe`. So threading `config.aligner` at the single `lib.rs:1231` site covers **both** the single-core and (Bowtie2-only) multicore paths. The plan's "two call sites" phrasing is slightly inaccurate but harmless — the implementer should thread the one site. The two `merge.rs` references at 1276/1538 are unit-test call sites that will need the new arg added.

---

## 6. Action items

### Critical
- *(none)*

### Important
- **I-1 — Add a unit test for Case C (mate-1 `ZS`, mate-2 no-tag, `aligner=Hisat2`).** Assert the result takes the **no-second-best** path: `sum_of_alignment_scores_second_best == None` (and thus the no-second-best MAPQ ladder), matching Perl's gate-false branch. This is the case where the mask flips the gate and changes the MAPQ ladder — the highest-information byte-visible consequence — and it is absent from the V-table (V4 covers the inverse, mate-1 none / mate-2 ZS). Cheap to add alongside V2/V3/V4.
- **I-2 — Add a PE+HISAT2 report-header unit test** (mirror the existing SE-HISAT2 L376–389 and PE-Bowtie2 L530–545 tests): assert PE line-order + "Bismark was run with HISAT2". Freezes the orthogonal-dimension composition that V6 only checks at gate level.

### Optional
- **O-1 — State explicitly in §5/V2 that the unit-test discriminator is the `aligner` argument, not the `XS`-vs-`ZS` tag string** (the Rust parser unifies both into `second_best`, align.rs L100–104). The proposed `mapped_pair_zs` helper is cosmetic; the real signal is `aligner=Hisat2` vs `Bowtie2` over identical records.
- **O-2 — Correct §2/§4 "two call sites":** there is one production call site (`lib.rs:1231`, `drive_merge_pe`); `parallel.rs` reuses `process_pe_chunk`→`drive_merge_pe`. Thread the one site (+ update the two `merge.rs` unit-test calls at 1276/1538).
- **O-3 — Fix the Perl line cite** for `--pbat ⊕ -f`: it is `bismark` 8156, not 8155 (immaterial).
- **O-4 — V2 fixture realism:** the plan's V2 example uses `r1 AS:i:0 ZS:i:-6 / r2 AS:i:0 ZS:i:-6`. With the mask this stores `sum_second_best = Some(-6)` (sum 0 ≠ sum_second -6), correct. Just confirm the chosen scores keep `sum != sum_second` so the pair is stored (not booted as within-thread-ambiguous), so the assertion actually reaches `calc_mapq`. (The plan's numbers already satisfy this.)

---

## Summary for the orchestrator

The 2b fix (mask `sb1=None` for HISAT2 at `merge.rs:598`) is **correct, source-faithful, and the single sufficient chokepoint** — I verified the Perl read-1/read-2 asymmetry (3372–3403 vs SE 2780), the sole Rust consumer (`merge.rs:598`, no other PE use of `r1.second_best`), per-instance coverage, and the SE/Bowtie2 freeze. No Critical findings; **APPROVE**. Two Important items, both **test-coverage gaps**, not fix defects: (I-1) add a unit test for the **mate-1-ZS / mate-2-no-tag** case — the mask *also* silently fixes a second divergence here by flipping the `is_some()` gate to false and switching `calc_mapq` from the with-second-best ladder (max 39) to the no-second-best ladder (max 42), a byte-visible MAPQ change the V-table omits; (I-2) add a PE+HISAT2 report-header unit test. Optional: clarify that the unit-test discriminator is the `aligner` arg, not the XS/ZS tag string (the Rust parser unifies them), and correct the "two call sites" claim (there is one production site, `lib.rs:1231`).

# PLAN_REVIEW_B — Phase 2b: HISAT2 paired-end (read-1 `ZS` mask) + PE/non-dir/pbat/FastA gate

**Reviewer:** B (independent, fresh context). **Target:** `phase2b-hisat2-pe/PLAN.md` (rev 0).
**Scope:** the 2b deltas only (2a detection/options/discovery/naming already shipped, PR #949, SE-gated — not re-reviewed).
**Sources checked:** Perl `bismark` v0.25.1 (repo root) at the PE parse/backfill/MAPQ sites; Rust crate `rust/bismark-aligner/src/{merge,align,output,methylation,config,lib,parallel}.rs` at HEAD `376a6d9` (build confirmed green).

## Verdict: **APPROVE with two changes — one Critical (gate-matrix gap), one Important (a wrong wiring claim).**

The core fix is **correct**. I independently walked the Perl PE read-1/read-2 asymmetry against the source and the Rust merge, and the proposed mask (`sb1` source → `None` for HISAT2 at `merge.rs:598`) reproduces Perl across **all four** mate-tag cases (proof below). The plan's central thesis — "read-1 `ZS` is the *only* new production logic" — **holds**: I checked MAPQ symmetry, FLAG/TLEN/XR/XG, backfill semantics, and PE spliced extraction, and found no other HISAT2-vs-Bowtie2 PE divergence in the *byte-reconstructed* path. **But** there is exactly one PE surface the plan's "aligner-agnostic" framing misses: the **`--ambig_bam` PE raw-record passthrough**, which emits HISAT2's *raw* FLAG/TLEN/RNEXT and has **never been byte-gated** (2a gated no ambig cell at all). That belongs in the 2b gate matrix. Plus the plan miscounts the wiring sites ("2 call sites" — there is **one**).

---

## 1. Logic — the mask is correct; thesis verified adversarially

### 1.1 The read-1 `ZS` mask reproduces Perl across all four cases ✅ (independently re-derived)
Perl PE read-1 loop (`bismark` **3372–3382**): `if AS:i: / elsif XS:i: / elsif MD:Z:` — **no `ZS` branch**, and the `XS:i:` regex is a substring match that `ZS:i:-6` does not satisfy → `$second_best_1` is **always undef** under HISAT2. Read-2 loop (**3384–3403**) has the `else{ $bowtie2 ? XS : ZS }` branch → captures ZS. The backfill (**3465–3474**) is gated on `if (defined $second_best_1 or defined $second_best_2)`, then `unless defined sb1 → sb1 = as1`, `unless defined sb2 → sb2 = as2`.

Rust `merge.rs:598-602`:
```rust
let (mut sb1, mut sb2) = (r1.second_best, r2.second_best);   // plan: sb1 source → None iff Hisat2
if sb1.is_some() || sb2.is_some() { sb1 = sb1.or(Some(as1)); sb2 = sb2.or(Some(as2)); }
```
With `sb1` source forced to `None` for HISAT2, walking all four HISAT2 PE cases against Perl:

| case (HISAT2 PE) | Perl `sb1`,`sb2` | Rust masked `sb1`,`sb2` | match |
|---|---|---|---|
| (a) r1 has ZS, r2 has ZS | `undef→as1`, `zs2` (guard true via sb2) | `None→as1`, `zs2` (guard true via sb2) | ✅ |
| (b) r1 has ZS, r2 no tag | `undef`, `undef` → **guard false**, both stay undef → no-second-best branch | `None`, `None` → guard false → no-second-best branch | ✅ |
| (c) r1 no tag, r2 has ZS | `undef→as1`, `zs2` | `None→as1`, `zs2` | ✅ |
| (d) neither | both undef → guard false | both None → guard false | ✅ |

Case (b) is the subtle one and it works **because** the guard is `is_some() || is_some()` (matching Perl's `or`): masking `sb1=None` correctly *demotes* a read-1-only ZS to "no second best" — exactly what Perl does (read-1 ZS is invisible, read-2 has nothing, so the whole second-best block is skipped). **This is the case the plan does not explicitly enumerate** (it lists mate-1-ZS, mate-1-no-tag/mate-2-ZS, both-no-tag, but not mate-1-ZS-only / mate-2-no-tag). It happens to be correct, but it is the one most likely to be coded wrong if someone "fixes" the guard instead of the source. **Add case (b) as a V-test** (assert it lands in the *no-second-best* branch with `sum_second_best = None`, not a backfilled value).

### 1.2 MAPQ has no read-1/read-2 asymmetry downstream of the parse ✅
Perl `calc_mapq` call (**3876–3878**) and Rust (`merge.rs:699-706`) both take the **summed** `sum_of_alignment_scores` + summed `sum_of_alignment_scores_second_best` (plus the two read *lengths*). The second-best asymmetry lives **entirely** in how the two per-mate `second_best` values are combined into `sum_second` (merge.rs:605); MAPQ itself is symmetric in the mates. So masking read-1's contribution to `sum_second` is the **complete** fix — confirmed, the plan's claim is right.

### 1.3 FLAG / TLEN / XR / XG / mate-link are aligner-agnostic ✅
`output.rs` `paired_end_sam_output` (453+) reconstructs TLEN from POS + walked end (499-544, Perl 8890-8994), RNEXT=`=`, PNEXT=mate POS, and FLAG from the strand/index decision tables — none read an aligner-specific tag; they consume only POS/CIGAR/which-slot-won, which is precisely the "aligner-derived" subset the SPEC isolates. XR/XG/XM are Bismark-derived from the genomic sequence + index slot. So HISAT2 vs Bowtie2 differ **only** in POS/CIGAR/winner, which flow through identical machinery. The plan's "FLAG/TLEN tables are aligner-agnostic" assumption **holds** — for the byte-*reconstructed* main BAM. (Exception: the `--ambig_bam` raw-passthrough path — see §3.1, the one real gap.)

### 1.4 PE spliced (`N`-CIGAR) extraction is per-mate, no fragment interaction ✅
`methylation.rs` PE extraction (`extract_corresponding_genomic_sequence_paired_end`, 399+) processes each mate independently; the `b'N'` skip (362) is the same walker SE uses. Bismark PE does **not** span the inter-mate fragment for methylation — each mate's genomic window is its own POS+CIGAR. So there is **no PE-specific splice interaction** beyond what 2a's SE splice tests + the existing multi-N/N+D tests (merge/methylation tests 821/836/851/865) already cover. The plan's V7 (PE spliced via fake + the 1M PE gate exercising real spliced PE pairs) is the right and sufficient addition. ✅

### 1.5 SE + Bowtie 2 are structurally frozen ✅
`check_results_single_end` is a **separate function** (merge.rs ~234) reading `rec.second_best` directly — the PE mask cannot touch it; Perl SE (2780) captures ZS for any aligner, matching the uniform parser. For `aligner == Bowtie2` the masked-`sb1` source is `r1.second_best` unchanged → byte-frozen. Both claims verified.

---

## 2. Wiring — correct outcome, but the plan's site-count is WRONG

**The plan says** (§3.2, §5.3) thread `config.aligner` into `check_results_paired_end` "at 2 call sites (lib.rs + parallel.rs)." **That is inaccurate.** There is **exactly one** production call site: `lib.rs:1231`, inside `drive_merge_pe` (lib.rs:1122, which has `config: &RunConfig` in scope at 1126). `parallel.rs` does **not** call `check_results_paired_end` — it calls `crate::process_pe_chunk` (parallel.rs:487), which routes through that same `drive_merge_pe`. So:

- Threading `config.aligner` at the single `drive_merge_pe` call site covers **both** the single-core (`run_pe`→`process_pe_chunk`→`drive_merge_pe`) and multicore (parallel.rs→`process_pe_chunk`→`drive_merge_pe`) paths.
- The multicore path is **unreachable for HISAT2** (hard-rejected at `config.rs:216`), so when `drive_merge_pe` is reached via parallel.rs, `config.aligner` is *always* `Bowtie2` → mask never wrongly fires. **Not dead code** — it's the same function, byte-frozen for Bowtie2 by construction.

This is a *simplification* in the plan's favour (one site, not two, and no parallel.rs edit at all), but the plan's "thread into parallel.rs" instruction would send the implementer hunting for a call site that doesn't exist. **Correct §3.2/§5.3 to "one call site (`drive_merge_pe`, lib.rs:1231); no parallel.rs change needed — both paths route through it."** `config.aligner` (config.rs:152) is available; `Aligner` (config.rs:21, with `Hisat2`) and `.token()`/`.name()` exist. (The `aligner` plumbing into the `report.rs` header was already done in 2a — confirmed lib.rs:357/946.)

---

## 3. Validation sufficiency — one real gate gap

### 3.1 (CRITICAL) `--ambig_bam` PE for HISAT2 is the one un-gated divergent PE surface
The plan's gate matrix (§3.4 / V8) lists PE {dir, non-dir, pbat} + FastA PE {dir, non-dir} + `--unmapped`/`--ambiguous` aux — but **omits `--ambig_bam` PE**. This matters because `--ambig_bam` is the **only** PE path that emits the **raw aligner record** rather than a Bismark reconstruction. `output.rs` `write_ambig_record` (755-797) passes the raw SAM line's **FLAG (field 1), RNEXT/PNEXT/TLEN (fields 6/7/8), CIGAR, SEQ** through *verbatim* — and the explicit comment at 764-766 notes "Bowtie 2 PE lines carry `=`/<mate-pos>/<tlen>". HISAT2's raw PE FLAG/TLEN/RNEXT for a multi-mapper need not match Bowtie2's, and (unlike the main BAM) Bismark does **not** rebuild them here. This path is byte-identical to Perl *only if* Perl's single-core ambig route emits the same raw HISAT2 line — which is plausible (Perl 1575-1586 `s/sam$/ambig.bam/` on the generic `$outfile`) **but has never been gated**:

- 2a's GATE_OXY matrix contains **no `--ambig_bam` cell at all** (SE or PE) — only `se_dir/se_nondir/se_pbat/se_fasta` + the multicore reject. The 2a code-review verified the ambig *naming* (`_bismark_hisat2.ambig.bam`) by a unit test, **not** byte-identity of the ambig BAM *content*.
- Single-core `--ambig_bam` + HISAT2 is **supported** (config.rs:179, lib.rs:486 SE / lib.rs:1031 PE); only the *multicore* combination is rejected. The prompt confirms "single-core ambig is allowed."

So the raw-passthrough HISAT2 ambig path is presently **unproven against Perl in any layout**, and PE is where it's most exposed (mate-link fields). **Add a single-core `--ambig_bam` PE HISAT2 gate cell** (directional, 1M — multi-mappers needed; 10k may contain few). At minimum, fail-loud if it can't be gated, rather than shipping an un-exercised raw-passthrough path. (Optional but cheap: add an SE `--ambig_bam` cell too, closing the 2a omission — but PE is the must-have.)

### 3.2 V2 unit test design — pinned correctly, with one fixture-arithmetic caveat
The plan's V2 (r1 `AS:i:0 ZS:i:-6`, r2 `AS:i:0 ZS:i:-6`; assert `sum_second = as1 + zs2 = -6`, NOT `zs1+zs2 = -12`) is the **right** discriminator. I verified it is *reachable* through the existing harness: sum = 0, masked sum_second = 0 + (-6) = -6; since 0 ≠ -6 the entry is stored (not within-thread-ambiguous), and with a single stored entry `second_for_mapq = -6` surfaces on `UniqueBest.sum_of_alignment_scores_second_best`. The existing test `pe_unique_best_by_sum_across_slots` (merge.rs:1324) asserts exactly that field, so the pattern is proven. **The fake-binary path** (`SamPair::from_lines`→`SamRecord::parse`) uses the uniform XS-or-ZS parser (align.rs:100-104), so a `ZS:i:`-emitting `mapped_pair_zs` helper will populate `second_best` for *both* mates — which is precisely the over-capture the mask must then drop for read-1. So the V2 test cannot accidentally pass on the buggy code: with the mask absent, the assertion sees `-12`. ✅ Good — the expectation is pinned to the correct mental model (`as1 + zs2`, read-1 ZS dropped).
- **Caveat:** the test's `run_pe` harness helper (merge.rs:1262) calls `check_results_paired_end` with the *current* 9-arg signature; adding the `aligner` param means the helper must thread it (default `Bowtie2` for the existing tests to stay green, `Hisat2` for the new V2/V4 tests). The plan implies this but should state it so the regression tests aren't broken by the signature change.

### 3.3 Single-core-only gate justification ✅ + FastA-pbat exclusion ✅
- Single-core-only is **correctly justified**: HISAT2 multicore is hard-rejected (config.rs:216, 2a GATE finding) — there is no byte-identical multicore target. No PE multicore cell is right.
- FastA-pbat exclusion is **correct**, though the plan **mis-cites the line**: `--pbat ⊕ -f` dies at Perl **8156** (`"only working with FastQ files ... lose the option -f"`), not 8155 (8155 is the pbat+gzip die). Fix the citation. The behavior is already a faithful-port invariant (9a), so excluding FastA-pbat is right.

### 3.4 Other gate cells — adequate
`--unmapped`/`--ambiguous` write read FASTQ (aligner-agnostic except *which* reads, which is decision-driven and covered by the main BAM gate) — fine. Non-dir 4-instance + pbat 2-complementary-instance PE at 1M is the right at-scale proof of the strand/instance table for HISAT2 PE. Re-running the 2a SE + the Bowtie2 PE-dovetail gates as regression guards (V1) is correct and necessary (the `aligner` param touches the byte-frozen Bowtie2 PE path).

---

## 4. Efficiency
No concerns. The fix is one conditional at one merge site (already an `Option` source); zero hot-path cost. Everything else is reuse. The plan's §6 is accurate.

---

## 5. Assumptions
- "HISAT2 PE raw stream always carries `AS:i:`/`MD:Z:` on both mates" (§8) — the merge dies otherwise (Perl 3405-3406 / Rust merge.rs:545-568). HISAT2 does emit these, but it remains an *assumption* the gate confirms; the plan correctly flags it. ✅
- "PE strand-instance model identical for HISAT2" (§8) — `merge.rs` has no aligner branch; the 2/4-instance + `--norc`/`--nofw` table is reused. The 1M non-dir/pbat PE gate is the proof. ✅
- "`calc_mapq` PE form aligner-agnostic, only the second-best input is the risk" (§8) — confirmed §1.2. ✅

---

## 6. Alternatives
- **`Aligner` vs `bool` param (OQ-2b-1):** agree with `Aligner` (clarity + minimap2-ready). Either is a one-site gate; non-blocking.
- **Mask at `merge.rs:598` vs a parse-time flag (OQ-2b-2):** merge-entry masking is the **right** choice — smallest blast radius, leaves the uniform parser + SE + Bowtie2 untouched. I verified it reproduces Perl in all four cases (§1.1), which a parse-time read-1 flag would also do but with a wider edit. Keep merge-entry.
- The plan correctly does **not** generalize a `Backend` trait for one new PE delta — right call.

---

## 7. Action items — prioritized

### Critical (fix the gate matrix before the gate runs)
1. **Add a single-core `--ambig_bam` PE HISAT2 gate cell** (directional, 1M for multi-mappers). It is the only PE surface that passes **raw HISAT2 FLAG/TLEN/RNEXT** through (output.rs:755-797, byte-reconstruction bypassed), supported single-core (lib.rs:1031), and **never byte-gated** (2a gated no ambig cell). Without it the raw-passthrough PE path ships unproven against Perl. (§3.1)

### Important (correct the plan text before implementation)
2. **Fix the wiring claim:** there is **one** call site (`drive_merge_pe`, lib.rs:1231), reached by both single-core and multicore paths via `process_pe_chunk`. `parallel.rs` does **not** call `check_results_paired_end` — no parallel.rs edit is needed (the plan's "2 call sites / thread parallel.rs" is wrong and would send the implementer chasing a non-existent site). `config.aligner` is in scope at the one site. (§2)
3. **Add the case-(b) unit test** — HISAT2 PE, mate-1 has `ZS` *only*, mate-2 no tag → assert it lands in the **no-second-best** branch (`sum_of_alignment_scores_second_best = None`), NOT a backfilled value. This is the subtlest of the four cases (masking demotes a read-1-only ZS to "no second best" via the `or` guard) and the one most likely to be mis-implemented. (§1.1)
4. **State that the `run_pe` test harness helper must thread the new `aligner` param** (default `Bowtie2` for existing tests, `Hisat2` for V2/V4) so the signature change doesn't silently break the regression suite. (§3.2)

### Optional (citation / polish)
5. **Fix the Perl citation:** `--pbat ⊕ -f` dies at **8156**, not 8155 (8155 = pbat+gzip). (§3.3)
6. Consider an SE `--ambig_bam` HISAT2 gate cell too, closing the 2a omission (PE is the must-have; SE is cheap insurance). (§3.1)

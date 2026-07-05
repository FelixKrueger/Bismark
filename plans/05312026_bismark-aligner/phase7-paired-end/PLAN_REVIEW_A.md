# PLAN_REVIEW_A — Phase 7: Paired-end support (directional, FastQ)

**Reviewer:** A (independent, fresh context)
**Plan:** `phase7-paired-end/PLAN.md` (rev 0, 2026-06-02)
**Method:** Re-derived every high-risk claim directly from `bismark` (v0.25.1) and the existing Rust modules. Line citations below are to the Perl source unless prefixed `rust/…`.

**Verdict:** APPROVE-WITH-FINDINGS — 0 Critical, 5 Important, 6 Optional.

The plan is unusually accurate: I independently verified all 8 of the requested scrutiny points (the 2-instance finding, the FLAG table, the TLEN tree, the conversion, the genomic extraction, the merge, the report, the aux naming) and every load-bearing claim checks out against the Perl byte-for-byte. The findings below are gaps in *modeling/specification precision* and *validation coverage*, not factual errors in the plan's reading of Perl. None block implementation, but the Important items should be folded into rev 1 before the implement trigger so the implementer doesn't re-derive them under time pressure.

---

## Logic review (each scrutiny point independently verified)

### §0 — "2 instances, not 4" — CONFIRMED
- Directional FastQ PE sets `$fhs[0]->{inputfile_1/2}` and `$fhs[3]->{inputfile_1/2}`; slots 1/2 are explicitly `undef` (**405–412**). Verified.
- Launcher prints "Now running 2 instances" for directional **or** pbat (**6448**), `next`-skips any fh with no `inputfile_1` (**6456–6463**). Verified.
- Per-instance flag (**6466–6471**): the `--norc` list is `CTread1GAread2CTgenome` (slot 0) **or** `GAread1CTread2GAgenome` (the slot-1 name, never spawned in directional). Slot 3's name `CTread1GAread2GAgenome` is NOT in that list → falls to the `else` → `--nofw`. So directional PE = slot 0 `--norc` (OT), slot 3 `--nofw` (OB). **Confirmed.**
- The index-1|2 directional rejection (**3851–3856**) is genuinely inert in directional PE (slots 1/2 never spawned, so no index-1/2 alignment can ever reach the merge). Porting it as defensive code is the right call (it goes live in Phase 8 and the report prints the count regardless). The §0 finding is correct and the SPEC §1 wording correction is warranted.

### §3.5 step 1 — FLAG constant table — CONFIRMED (the highest-risk table, exactly right)
Decoded the default (`!old_flag`) branch (**8825–8868**):
- index 0 (OT): `flag_1=99` (1+2+0x20+0x40), `flag_2=147` (1+2+0x10+0x80). ✓
- index 1 (CTOB): `flag_1=163` (1+2+0x20+0x80), `flag_2=83` (1+2+0x10+0x40). ✓
- index 2 (CTOT): `flag_1=147`, `flag_2=99`. ✓
- index 3 (OB): `flag_1=83`, `flag_2=163`. ✓

The R1↔R2 first/second-in-pair swap for index 1/2 is real (comments **8821–8823**: "flip the R1 R2 flags around for CTOT and CTOB … We still report the first and second read in the same order and only change the actual FLAG value"). The plan's literal-table approach (not bit-assembly) is the correct and safest port.

### §3.5 step 4 — TLEN tree — CONFIRMED (verified branch-by-branch, signs included)
- **A `start_1 <= start_2`** (**8893**, `<=`):
  - A1 `end_2 >= end_1` (**8897**): dovetail iff `flag_1==83 && $dovetail` (**8899**) → `tlen_1=start_1-end_2-1`, `tlen_2=end_2-start_1+1` (**8904–8905**); normal → `tlen_1=end_2-start_1+1`, `tlen_2=start_1-end_2-1` (**8922–8923**). ✓
  - A2 `end_2 < end_1` (**8927**, R2 contained): `tlen_1=end_1-start_1+1`, `tlen_2=-(end_1-start_1+1)` (**8938–8939**). ✓
- **B `start_2 < start_1`** (**8943**, strict `<`):
  - B1 `end_1 >= end_2` (**8947**): dovetail iff `flag_1==99 && $dovetail` (**8950**) → `tlen_1=end_1-start_2+1`, `tlen_2=start_2-end_1-1` (**8957–8958**); normal → `tlen_2=end_1-start_2+1`, `tlen_1=start_2-end_1-1` (**8975–8976**). ✓
  - B2 `end_1 < end_2` (**8979**, R1 contained): `tlen_1=-(end_2-start_2+1)`, `tlen_2=end_2-start_2+1` (**8990–8991**). ✓
- `$dovetail` defaults to 1 unless `--no_dovetail` (**8047–8048**). ✓
- **Subtlety the plan handles correctly:** the `start_1==start_2` case is folded into branch A (the `<=`), and there is NO `else` after branch B — meaning if neither A nor B fires (impossible since `<=`/`<` partition the line), `tlen_1`/`tlen_2` would be `undef`. The plan's "preserve the `<=` (A) vs `<` (B) boundary exactly" captures this. The implementer must make the Rust an `if/else` (not two independent `if`s) so the partition is total. (Optional note O-2.)

### §3.5 steps 5–7 — reorientation, +2 trim, tags — CONFIRMED
- Per-mate revcomp keyed on **stored `strand_1`/`strand_2`** (`alignment_read_1/2`), NOT the swapped FLAG (**8999–9015**): revcomp `actual_seq`+`ref_seq`, double-revcomp MD-seq iff CIGAR has `D`, `reverse` qual. ✓ Matches SE `output.rs:386–393` applied per mate.
- +2 ref trim is **index-driven** (**8772–8779**): index 0/3 → R1 drop last 2 / R2 drop first 2; index 1/2 → R1 drop first 2 / R2 drop last 2. ✓ I checked all four indices against the index→read_conv map and the index-driven rule is **mathematically equivalent** to the SE per-mate-read_conv rule, so there is no parity risk either way; the plan keeps the literal index-driven form, which is what Perl does. Good.
- Tag order `NM MD XM XR XG` (**9217–9218**, default `!rg_tag !strandID !non_bs_mm` branch). ✓ XR per-mate (`read_conversion_1` vs `_2`, **9059–9060**); XG shared pair-wide (**9064**). ✓ XM reversed iff that mate's strand is `-` (**9043–9055**). ✓
- The default print branch is **9216–9218**; `--strandID` adds `YS:Z:`, `--rg_tag` adds `RG:Z:`, `--non_bs_mm` adds `XA/XB` — all correctly declared out-of-gate.

### §3.1 — PE conversion — CONFIRMED
- R1 = `tr/C/T/` on uc read (**5978**); R2 = forward `tr/G/A/` (**5982**), NO revcomp. ✓
- ID suffix: `fix_IDs`+chomp, then re-add `\n` (**5924–5926**), then **5945–5960** inserts `/1/1` (R1) or `/2/2` (R2) via `s/$/\/1\/1/` — i.e. before the just-added trailing `\n`. The plan's "insert the read-number tag before the final `\n`" is exactly right. ✓ (`$mm2` branch uses single `/1`; out of scope.)
- Filename rule `s/$/_C_to_T.fastq/` vs `_G_to_A.fastq`, `+ .gz` iff `--gzip`, `--prefix` prepends `"$prefix."`, each off its own mate basename (**5836–5852**). ✓

### §3.4 — genomic extraction — CONFIRMED (with one modeling gap, see I-1)
- No fragment span; two independent per-mate CIGAR walks (R1 **4530–4614**, R2 **4617–4702**). ✓
- 5′/3′ placement index-driven: index 1/3 → 5′ prepend (**4533/4620**); index 0/2 → 3′ append (**4606/4691**). ✓
- **Guard asymmetry — CONFIRMED and important:** mate1 5′ guard is `unless ($pos_1-2) > 0` (**4535**), strict `>`; mate2 5′ guard is `unless ($pos_2-2) >= 0` (**4622**). With `$pos = position - 1` (0-based, **4513–4514**), mate1 needs `pos_1 >= 3` and mate2 needs `pos_2 >= 2`. The plan's parenthetical "(requires `pos_1 >= 3`)" is correct **only** for the 0-based `pos_1` (Perl's `$pos_1`), NOT the SAM 1-based `position_1`. (Optional clarity note O-1 — easy to mis-read.)
- 4 strand counters + revcomp target (**4708–4775**): 0→`CT_GA_CT`, r1+/r2-, revcomp mate2; 1→`GA_CT_GA`, r1+/r2-, revcomp mate2; 2→`GA_CT_CT`, r1-/r2+, revcomp mate1; 3→`CT_GA_GA`, r1-/r2+, revcomp mate1. ✓ The revcomp'd mate is the `-`-strand one; MD-seq revcomp'd alongside iff `contains_deletion` (**4719/4735/4753/4769**). `index>3` dies (**4773**). ✓
- Counters bump only **past all four edge guards** (the early `return`s at **4538/4610/4626/4699** are before the **4708+** dispatch). ✓ Per-mate length gate in the caller (**3864/3869**) is the real "could-not-extract" trigger. ✓

### §3.3 — the merge — CONFIRMED
- Quality default `'I' x len` (**3274–3280**). ✓
- Scan order `(0,3,1,2)` (**3300**), distinct from SE's `0,1,2,3`. ✓
- No-align marker is the **pair** `flag_1==77 && flag_2==141` (**3317**); reads two lines, sets new last_line_1/2, `next` instance (**3317–3346**). ✓ **Note:** unlike SE (which dies if the next line is the same id, `rust/merge.rs:156–160`), PE has **no** die-if-same-id guard here — the implementer must NOT copy the SE guard into the PE no-align path. (Optional O-3.)
- De-convert both RNAMEs `s/_(CT|GA)_converted$//`; die unless `chr_1 eq chr_2` (**3351–3364**). ✓
- AS+MD mandatory both mates (**3405–3406**); XS_1 (R1), XS_2 with dead-ZS fallback (R2) (**3372–3403**); `sum = AS_1 + AS_2` (**3416**). ✓
- Overwrite/`best_AS_so_far`/first-ambig machinery keyed on the sum (**3422–3463**); single-mate-XS defaults the other to its own AS (**3466–3474**); `sum_2nd = XS_1+XS_2` (**3477**); within-thread tie → `amb_same_thread` if `sum==best` (**3483–3488**). ✓
- **Location-key inconsistency — CONFIRMED:** second-best branch is `chr:min:max` (`<=`→pos1:pos2 at **3527–3528**, `<`→pos2:pos1 at **3530–3531**); no-second-best branch is **raw** `chr:pos1:pos2` (**3593**). The plan preserves this verbatim. ✓
- Selection (**3750–3825**): 1 entry accept; 2–4 sort desc, top-tie → `sequence_pair_fails` (**3788–3791**); the sum_2nd conditional `defined && best.2nd > runner-up.sum ? best.2nd : runner-up.sum` (**3811–3816**); `>4` die (**3823–3824**). ✓
- Directional reject index **1|2** (**3852**); `calc_mapq(len(seq1), len(seq2), sum, sum_2nd)` (**3876–3878**) — uses the **original read** lengths, not the bowtie-seq lengths. ✓ Extract → two length guards → mapq → methylation_call ×2 → output → return 0 (**3861–3896**). ✓

### §3.6 / §3.7 — driver, naming, routing — CONFIRMED
- Lockstep readback (**2602–2674**): 4 lines per file; `last unless` checks 6 of the 8 lines (the two `+` lines are NOT in the guard, **2611**) — harmless for well-formed FastQ. `fix_IDs` both (**2620–2621**); `@`-strip R1 only (**2640**); `orig_identifier_1/2` captured before the strip (**2637–2638**, so R2's "orig" is still `@`-bearing because R2 is never stripped). `sequences_count++` per pair (**2632**). `check_results_paired_end(uc seq1, uc seq2, id1, q1, q2)` — passes the `@`-stripped R1 id (**2642**). ✓
- Routing (**2649–2674**): return 2 → AMBIG_1/_2; return 1 → UNMAPPED_1/_2; 4-line records use `orig_identifier`, raw (chomped) seq+`\n`, the verbatim un-chomped `$ident_` line (keeps its own `\n`), qual+`\n`. ✓ (See O-4: the `+` line's pre-existing newline vs the re-added newlines is a subtle byte detail.)
- BAM stem = R1 basename, fastq-suffix stripped (**1769**), default `_bismark_bt2_pe.bam` (lowercase, **1776→1807**); `--basename` → `<base>_pe.bam` (**1783**). Report = `_bismark_bt2_PE_report.txt` (uppercase, **1832**); `--basename` → `<base>_PE_report.txt` (**1839**). The lowercase-pe / uppercase-PE asymmetry is real and the plan nails it. ✓
- Two-temp cleanup: directional unlinks `$C_to_T_infile_1` + `$G_to_A_infile_2` (**2155**). ✓
- Aux filenames on the **un-stripped** R1/R2 basenames, `_unmapped_reads_1.fq`/`_2.fq` + `.gz` single-core (**1853–1938**). ✓

### §3.8 — the PE report — CONFIRMED (the trailing-space quirk is exactly right)
- Header "and" (**1843**); directional line same wording as SE (**1941**). ✓
- 7 wording swaps (**2195, 2204–2217**). ✓
- **The `% \n` quirk — CONFIRMED:** REPORT line **2205** is `…${percent}% \n` (trailing space, single `\n`); STDOUT line **2204** is `…${percent}%\n\n` (no trailing space, double `\n`). The plan correctly targets 2205 and warns not to copy 2204. This is the single fiddliest byte in the whole phase and the plan got it right. ✓
- 3-token strand labels (**2218**) — **print order is index 0, 2, 1, 3**: `CT/GA/CT` (0, top), `GA/CT/CT` (2, complementary-to-top), `GA/CT/GA` (1, complementary-to-bottom), `CT/GA/GA` (3, bottom). The plan lists them in this exact 0,2,1,3 order. ✓ (See O-5: the plan says "map index→counter" without explicitly stating the *print order* differs from both the 0,1,2,3 and the 0,3,1,2 scan order — worth a one-line note.)
- Cytosine half byte-identical to SE (**2226–2312** == SE 2052–2136). ✓

---

## Assumptions (validated / flagged)

- **`mapq.rs` unchanged — VALID.** `rust/mapq.rs:13–20` already has `calc_mapq(read1_len, read2_len: Option<usize>, as_best, as_second: Option<i64>, intercept, slope)`. The PE call `calc_mapq(len1, Some(len2), sum, sum_2nd, …)` works as-is; the `read2_len`-defined branch (lines 23–25) matches Perl 3934–3936; the `as_second == None` branch (single-`alignments`-entry case where Perl stores `undef` at **3606**) matches Perl 3947–3954. Confirmed.
- **Parallel PE types (Q3) — SOUND.** `Decision`/`BestAlignment`/`SamStream` in `rust/merge.rs:18–58` and `rust/align.rs` are SE-shaped; parallel `DecisionPaired`/`BestAlignmentPaired`/`PairedSamStream` keep the SE path untouched. Agreed this beats overloading with `Option` mate-2 fields. (See I-2 for the GenomicExtractionPaired edge-state gap.)
- **4 new PE Counters fields (Q5) — VALID.** The existing `Counters` (`rust/merge.rs:79–86`) has SE 2-token fields (`ct_ct_count`…). Perl PE uses distinct keys (`CT_GA_CT_count`…) and 3-token report labels, so adding `ct_ga_ct`/`ga_ct_ga`/`ga_ct_ct`/`ct_ga_ga` is correct and a run is SE-xor-PE so no double-count.
- **Pooled context counters — VALID.** Both mates accumulate into the same run-global `total_*` fields; Perl's `methylation_call` writes to the shared `%counting` for both calls (**3887–3888**). Confirmed.
- **Hidden assumption to surface (I-3):** the plan assumes the existing `aux_filename` signature can be "extended" with a `mate` param, but the current signature is `aux_filename(filename, prefix, basename, kind, fasta)` (`rust/aux_out.rs:40`), NOT the `(read_file, config, kind, mate)` shape in §4. Reconcile (see I-3).

---

## Efficiency

The §6 analysis is correct: no new genome passes, merge is O(pairs), two records/pair is inherent, `first_ambig` clones gated on `--ambig_bam`, two temp files + two readers per pair = same I/O shape as Perl. No concerns. One micro-note: the SE driver loads the genome once before the read loop (`rust/lib.rs:125`); the PE driver must do the same once before the pair loop (the plan §3.6 step 2 says "Load the genome once" — good).

---

## Validation sufficiency

The §7 matrix is strong, and the two dominant risks (FLAG #12, TLEN #13) correctly get a **Perl-differential harness** in addition to unit tests, mirroring the Phase-4 `calc_mapq` / Phase-5 `make_mismatch_string` differentials. Gaps:

- **V-1 (Important): the TLEN differential matrix omits the equality boundaries.** §7 #13 lists "A1/A2/B1/B2 + dovetail" but does not pin `start_1==start_2` (must take branch A, the `<=`) or `end_2==end_1` / `end_1==end_2` (the `>=` vs `<` boundaries in A1/A2 and B1/B2). A future off-by-one (`<` vs `<=`) on any of these three equalities would slip through. The differential should enumerate the equality cases explicitly.
- **V-2 (Important): no test for the dovetail FLAG-gate interaction.** The dovetail sub-cases fire only when `flag_1==83` (index 3, branch A1) or `flag_1==99` (index 0, branch B1) AND `$dovetail`. The matrix says "× dovetail" but should assert that an index-1/2 pair (flag_1 = 163/147) in a dovetailing layout does NOT take the dovetail branch (because neither 83 nor 99) — i.e. the FLAG gate is load-bearing, not just the geometry. Add a `--no_dovetail` cell too (dovetail=0 path).
- **V-3 (Optional): the per-mate length-guard test (#10/#11) should assert the strand counter is NOT bumped on an edge miss** (the read still counts in `unique_best` but in no strand bucket). The plan describes this in §3.4 step 4 but #10 only checks the could-not-extract count.
- **V-4 (Optional): no explicit test that the merge passes the `@`-stripped R1 id (not R2's) into `paired_end_sam_output`** and that both records carry that same QNAME (Perl `$id_1=$id_2=$id`, **8734–8737**). Worth a unit/integration assertion since a wrong-mate-id bug would be silent on single-pair tests where ids happen to match.
- The oxy gate (#21) is the correct final arbiter; filtering `^Bismark completed in ` + samtools `@PG` matches the SE policy.

---

## Alternatives

- **Q3 merge-seam (parallel types) — endorse.** Confirmed the SE types are SE-shaped and the parallel-types route keeps them untouched; the `Option`-overload alternative would force every SE match site to handle PE `None`s (noisier) and risks regressing the merged SE gate. Generic-over-mate-count is premature. Parallel types is right.
- **GenomicExtractionPaired modeling — recommend mirroring SE's `extracted: bool` (I-2).** The SE `GenomicExtraction` (`rust/methylation.rs:52–55`) carries a doc-only `extracted: bool` and returns a fully-populated `edge(...)` struct on a guard miss, relying on the caller's length check. The plan's `GenomicExtractionPaired` (§4) has no such flag and all-required fields — but the Perl early-returns (**4538/4610/4626/4699**) leave strand/conversion/end_position/indels **unset** for one or both mates. The clean port is: populate the struct with the strand/conversion derived up-front (as SE does) OR carry an `extracted_1`/`extracted_2` (or single `extracted`) flag and let the per-mate length guard in the driver gate. Either works; the plan must pick one and say so.

---

## Action items

### Critical
*(none — every load-bearing Perl claim verified correct)*

### Important
- **I-1 — TLEN total-partition.** State in §3.5 step 4 that the A/B branches form an `if/elsif` (total partition via `<=`/`<`), not two independent `if`s, so `tlen` is never left unset. Perl relies on this (**8893/8943**, no trailing `else`).
- **I-2 — GenomicExtractionPaired edge-state contract.** §3.4/§4 must specify the early-return representation (mirror SE's `extracted: bool` + fully-populated struct, OR per-mate `extracted_{1,2}`), since Perl's `return;` at 4538/4610/4626/4699 leaves fields unset and only the caller's per-mate `len==read_len+2` guard (**3864/3869**) catches it. As drafted, the all-required-fields struct can't represent the miss.
- **I-3 — aux_filename signature reconciliation.** The existing `aux_filename(filename, prefix, basename, kind, fasta)` (`rust/aux_out.rs:40`) differs from §4's `aux_filename(read_file, config, kind, mate: Option<u8>)`. Either append a `mate: Option<u8>` to the existing signature (SE passes `None`) or add a sibling — but say which, so the implementer doesn't churn the SE call site (`rust/lib.rs` open_sinks) and regress the SE gate.
- **I-4 — pin the FULL aligner_options string in §7 #1, not just the PE subset.** The push order interleaves PE flags with others: `--no-mixed`/`--no-discordant`/`--dovetail` are pushed in the PE-detection block (**8044–8056**) but `--maxins 500` is pushed later after `chdir` (**8135**), with `--quiet` (**8141**) and the score-min/-p/etc. potentially between. Also note **default WGBS has no `--minins`** (only pushed if `-I` given, **8123–8125**). The differential should assert the complete options string Perl emits for the gate argv, not `--minins/--maxins 500` as if both are present.
- **I-5 — TLEN/FLAG validation gaps V-1 + V-2.** Add the equality-boundary cells (`start_1==start_2`, `end_2==end_1`, `end_1==end_2`) and the dovetail FLAG-gate negative case (index-1/2 dovetailing layout must NOT take the dovetail branch) + a `--no_dovetail` cell to the §7 #13 differential. These are the cells most likely to hide an off-by-one.

### Optional
- **O-1 — clarify `pos_1 >= 3` is for the 0-based `pos_1`** (`= position_1 - 1`), not SAM 1-based `position_1`, in §3.4 step 3 (mate1 needs `position_1 >= 4`).
- **O-2 — ambig BAM filename.** §3.7 mentions the ambig BAM but doesn't pin the name: Perl derives it `$outfile =~ s/sam$/ambig.bam/` (**1788**) → `<base>_bismark_bt2_pe.ambig.bam` (`_pe.sam` → `_pe.ambig.bam`). Add it.
- **O-3 — PE no-align has no die-if-same-id guard.** Note that the PE (77,141) path (**3317–3346**) does NOT have the SE `die "…but next seq-ID was also…"` guard (`rust/merge.rs:156–160`); don't copy it across.
- **O-4 — aux `+` line newline handling.** The aux record's `+` line is the verbatim un-chomped `$ident_` (retains its own `\n`), while seq/qual are chomped+re-`\n`'d (**2651–2659**). Spell this out for the PE aux writer to avoid a double/missing newline.
- **O-5 — report strand-label print order is 0,2,1,3.** §3.8 step 3 lists the labels correctly but should state the print order differs from both the natural 0,1,2,3 and the 0,3,1,2 scan order (Perl **2218**).
- **O-6 — V-3/V-4** (edge-miss-no-counter-bump assertion; merge-passes-R1-id assertion) as described under Validation sufficiency.

---

## Verdict

**APPROVE-WITH-FINDINGS.** 0 Critical, 5 Important, 6 Optional. The plan's reading of the Perl is accurate on every one of the eight high-risk points (2-instance, FLAG, TLEN, conversion, extraction, merge, report, naming) — I re-derived each from source and found no factual error. The Important findings are modeling/spec-precision gaps (edge-state struct, aux signature, TLEN partition totality) and two genuine validation holes (the TLEN/FLAG equality + dovetail-gate cells, I-5) that a Phase-4/6 reviewer would have flagged. Fold I-1…I-5 into rev 1, then proceed to dual plan-review / implement.

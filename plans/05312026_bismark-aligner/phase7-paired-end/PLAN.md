# PLAN — Phase 7: Paired-end support (directional, FastQ) 🎯

> **Epic:** `05312026_bismark-aligner/EPIC.md`, Phase 7 — *Paired-end support* 🎯 (PE byte-identity gate)
> Depends on: **Phase 5** (`run_se_directional`/`drive_merge` driver, `genome.rs`, `methylation.rs`
> SE genomic-seq + `XM`/`XR`/`XG`, `output.rs` `single_end_sam_output`/`generate_sam_header`,
> `bismark-io` `BamWriter`), **Phase 6** (`report.rs`, `aux_out.rs`, `merge.rs` `Decision::Ambiguous{first_ambig}`,
> the `Sinks` routing). All prior gates SE-directional. 🎯 **PE byte-identity gate** (PE-directional WGBS, local + oxy).

---

## 1. Goal

Add the **paired-end directional FastQ** path to `bismark_rs`, byte-identical to Perl `bismark` v0.25.1 +
Bowtie 2 2.5.5. This is the hardest phase — `check_results_paired_end` (Perl 3269–3897) is the largest single
function in the wrapper — but structurally it is the SE spine **doubled for two mates**. Concretely, end to end:

1. **PE read conversion** — R1 → C→T, R2 → **G→A** (forward, *not* revcomp+C→T), each tagged with the doubled
   read-number ID suffix (`/1/1`, `/2/2`); two converted temp files (directional). Perl `biTransformFastQFiles_paired_end` (5810–6025).
2. **2 paired Bowtie 2 instances** (NOT 4 — see §0 finding): slot 0 = OT (`BS_CT`, `--norc`), slot 3 = OB
   (`BS_GA`, `--nofw`), each run as `-1 <CT_R1_temp> -2 <GA_R2_temp>`, emitting **two SAM lines per pair**.
3. **PE lockstep merge** — `check_results_paired_end`: peek two SAM lines per instance, select the unique best
   pair by the **sum of both mates' `AS:i`**, with the same overwrite/within-thread-ambiguity/cross-instance-tie
   machinery as SE; directional reject on chosen index 1|2; `calc_mapq(len1, Some(len2), sumAS, sumAS_2nd)`.
4. **PE genomic-seq extraction + XM call** — each mate extracted **independently** from its own POS+CIGAR
   (there is *no* fragment-span computation), the `-`-strand mate reverse-complemented; `methylation_call`
   reused verbatim per mate. Perl `extract_corresponding_genomic_sequence_paired_end` (4471–4794).
5. **PE SAM output** — **two** BAM records per pair with the mate-link fields: the four FLAG-constant pairs
   (incl. the index-1/2 R1↔R2 first/second-in-pair swap), `TLEN` (signed, with dovetail + full-containment
   sub-cases), `RNEXT='='`, `PNEXT`=other mate's POS, MAPQ shared, per-mate `XR`/shared `XG`. Output
   `<r1stem>_bismark_bt2_pe.bam`. Perl `paired_end_SAM_output` (8713–9225).
6. **PE reports + aux routing** — the `_PE_report.txt` (`print_final_analysis_report_paired_ends`, 2146–2312)
   and the `_1`/`_2` `--unmapped`/`--ambiguous` files + the two-line-per-pair `--ambig_bam`.

**Out of this phase (deferred):** non-directional + pbat (the 4-instance path) = **Phase 8**; FastA PE +
order-preserving multicore (the plain-then-merge aux path) = **Phase 9**; `--slam` PE; full-scale gate = **Phase 10**.

---

## 0. 🔴 Finding that CORRECTS the kickoff + SPEC §1: directional PE runs **2** instances, not 4

The kickoff ("Directional PE runs **all 4 instances** and rejects wrong-strand hits post-hoc") and SPEC §1
("Directional PE / non-directional / pbat → all 4") are **both wrong for directional**. Verified in source:

- **Directional FastQ PE populates only `$fhs[0]` and `$fhs[3]`** (`$C_to_T_infile_1` / `$G_to_A_infile_2`);
  `$fhs[1]` and `$fhs[2]` are set to `undef` (Perl **405–412**).
- The launcher **prints "Now running 2 instances of Bowtie 2"** for directional *or* pbat (Perl **6448**), and
  **`next`-skips any filehandle with no `inputfile_1`** (Perl **6456–6463**).
- Per-instance flag (Perl **6466–6471**): slot 0 `CTread1GAread2CTgenome` → `--norc` (OT); slot 3
  `CTread1GAread2GAgenome` → `--nofw` (OB) — slot 3's name is NOT in the `--norc` list, so it falls to `--nofw`.

So **directional PE = 2 instances (OT slot 0 `--norc`, OB slot 3 `--nofw`)**, each a *paired* `-1/-2`
alignment. The "all 4" count applies **only to non-directional** (Phase 8). The directional wrong-strand
rejection (reject chosen index 1|2, Perl **3851–3856**, `alignments_rejected_count`) survives as **defensive
code that is effectively unreachable** in directional PE (slots 1/2 are never spawned) — port it anyway
(the report prints the count, and it becomes live in Phase 8). **Recommend: build the 2-instance directional
PE path; flag SPEC §1's wording for a one-line correction** ("all 4" → "all 4 *(non-directional/pbat)*; directional = 2").
This matches SE-directional's 2-instance count (the *slots* differ: SE-dir = 0,1; PE-dir = 0,3).

---

## 2. Context

### Placement (module-by-module; all extend existing Phase 1–6 modules)

| Module | Change | Perl source |
|---|---|---|
| **`convert.rs`** | extend: `biTransformFastQFiles_paired_end` analogue — per-mate conversion selector (R1→C→T, R2→G→A) + the `/1/1`,`/2/2` ID suffix; reuse the SE gz/skip/upto/prefix/max-len/sanity plumbing | 5810–6025 |
| **`align.rs`** | extend: a **paired** stream — `PairedSamStream` trait (`current_pair`/`advance_pair`) + `PairedAlignerStream` (spawn one Bowtie 2 `-1/-2`, peek two lines, identify R1 via the trailing `/1`) | launcher 6432–6523; lockstep readback 2578–2696 |
| **`merge.rs`** | extend: `BestAlignmentPaired`, `DecisionPaired`, `check_results_paired_end`; add 4 PE per-strand `Counters` fields | 3269–3897 |
| **`methylation.rs`** | extend: `extract_corresponding_genomic_sequence_paired_end` (two independent per-mate extractions, index-driven +2, revcomp the `-` mate, 4 PE strand counters); reuse `methylation_call`/`reverse_complement` verbatim per mate | 4471–4794 |
| **`output.rs`** | extend: `paired_end_sam_output` (two records, FLAG table, TLEN, mate-link fields); reuse `make_mismatch_string`/`hemming_dist`/`revcomp`/tag machinery | 8713–9225 + wrapper 4203–4215 |
| **`report.rs`** | extend: `print_final_analysis_report_paired_ends` + PE header variant (reuse the cytosine half + wall-clock line) | 2146–2312; header 1843–1947 |
| **`aux_out.rs`** | extend: `_reads_1`/`_reads_2` mate suffix on the (un-stripped) basename | 1853–1938 |
| **`lib.rs`** | extend: `pipeline()` arm `(PairedEnd, Directional, FastQ)` → new `run_pe_directional`; PE `Sinks` (1 BAM + 1 ambig-BAM + **2** unmapped + **2** ambiguous); pair-wise driver loop; two-file lockstep readback; two-temp cleanup | 1746–1840 (driver), 2578–2696 (readback) |
| **`config.rs` / `options.rs`** | **verify only** — `ReadLayout::PairedEnd` + the PE bowtie flags (`--no-mixed`/`--no-discordant`/`--dovetail`/`--minins`/`--maxins 500`) are already modeled (Phase 1); no new options expected | 8044–8059, 8123–8137 |
| **`mapq.rs`** | **no change** — `calc_mapq(read1_len, read2_len: Option, …)` already implements the PE second-mate `sc_min` term (lines 22–25) | 3923–3936 |

### Already in place (Phase 1, verified)
- **CLI**: `-1 <mates1>` / `-2 <mates2>` (`cli.rs:31–37`), `--minins`/`--maxins`/`--dovetail`/`--no_dovetail`.
- **Config**: `ReadLayout::PairedEnd { mates1, mates2 }` + `is_paired()` + mate-count validation +
  `--single_end`-conflict guard (`config.rs:52–72, 373–402`); `build_aligner_options(cli, format, layout.is_paired())`.
- **Dispatch**: `pipeline()` matches only `(SingleEnd, Directional, FastQ)` (`lib.rs:100`); the `_ =>` arm errors.

### Perl source of truth (read these directly during implementation)
- `check_results_paired_end` **3269–3897** (the core; read in full).
- `biTransformFastQFiles_paired_end` **5810–6025** (R2 = forward G→A; `/1/1`,`/2/2` at 5945–5960).
- `paired_end_align_fragments_to_bisulfite_genome_fastQ_bowtie2` **6432–6523** (2-instance launch + first-pair peek + R1-by-`/1`).
- `process_fastQ_files_for_paired_end_methylation_calls` **2578–2696** (two-file lockstep; `fix_IDs` both; `@`-strip R1 only at 2640; return-code routing 2648–2674).
- `extract_corresponding_genomic_sequence_paired_end` **4471–4794** (two independent per-mate extractions; index dispatch + 4 counters 4708–4775).
- `paired_end_SAM_output` **8713–9225** + `print_bisulfite_mapping_results_paired_ends` **4203–4215** (phred64 only).
- `start_methylation_call_procedure_paired_ends` **1746–1962** (output/report/aux naming + per-mode temp cleanup).
- `print_final_analysis_report_paired_ends` **2146–2312** (PE report body).
- `calc_mapq` **3923–3936** (the `read2_len`-defined branch; already ported).

---

## 3. Behavior (numbered)

### 3.1 PE read conversion (`convert.rs`) — R1 C→T, R2 forward G→A, doubled ID suffix
Called **twice** per pair-file (once per mate, passing the read number), each producing **one** temp file (directional):
1. **R1** (`read_number == 1`, directional): `tr/C/T/` on the uppercased read → `<r1base>_C_to_T.fastq[.gz]` (Perl 5977–5980).
2. **R2** (`read_number == 2`, directional): `tr/G/A/` on the **forward** read (NOT revcomp) → `<r2base>_G_to_A.fastq[.gz]` (Perl 5981–5984).
   🔴 **This is the one reason a PE converter ≠ the SE converter** (which is C→T only). Add `convert_seq_g_to_a` (`uc` then `g→A`).
3. **ID suffix** (Perl 5945–5960): after `fix_id` + chomp, **insert the read-number tag before the final `\n`** —
   R1 gets **`/1/1`**, R2 gets **`/2/2`** (the *doubled* suffix; Bowtie 2 strips the outer `/1`,`/2`, leaving `/1`,`/2`
   which the merge then strips at 3312–3313). The SE converter does NOT tag IDs → this is new.
4. Everything else (gz reader/writer, `skip`/`upto` falsy-0, `--prefix`, `--maximum_length` inert guard, the
   record-1 sanity check, `id2`/`qual` verbatim, truncated-tail drop) is **identical to the SE `convert.rs`** —
   factor a shared per-record core so both paths share it.
5. Filename derivation: identical rule to SE (`s/$/_C_to_T.fastq/` vs `s/$/_G_to_A.fastq/`; `+ .gz` iff `--gzip`;
   `--prefix` prepends `"$prefix."`), each off its own mate's basename (5836–5852).

### 3.2 Paired alignment streams (`align.rs`) — 2 instances, two lines per pair
1. Spawn **2** Bowtie 2 instances (§0): slot 0 (`BS_CT`, `--norc`), slot 3 (`BS_GA`, `--nofw`), each
   `-1 <temp>/<CT_R1> -2 <temp>/<GA_R2>` (Perl 6474). Indexes per `config.genome` (CT/GA basenames).
2. Skip `^@` header lines; then read the **first pair** = two lines (Perl 6487–6521).
3. 🔴 **Identify which line is R1** by stripping a trailing `/1` from the QNAME (Perl 6500–6508): if line-1's id
   ends `/1` → R1=line-1, else if line-2's id ends `/1` → R1=line-2, else `die`. Store `last_line_1`/`last_line_2`
   = R1/R2 lines and `last_seq_id` = the `/1`-stripped id. (Bowtie 2 reports the leftmost-position mate first,
   which may be R1 or R2 — so the stream must canonicalize to R1/R2, not first/second-on-the-line.)
4. The PE merge needs a **peek-two / advance-two** primitive: a `PairedSamStream` trait
   (`current_pair() -> Option<(&SamRecord,&SamRecord)>` returning (R1,R2); `advance_pair()`), and a
   `PairedAlignerStream` impl. Reuse the Phase-3 child-pipe contract (drain stdout before wait; `Drop`=kill+wait).

### 3.3 PE merge — `check_results_paired_end` (`merge.rs`)
Mirror `check_results_single_end`, doubled. For one read pair (`identifier`, uc `sequence_1`, uc `sequence_2`,
`quality_value_1/2` — defaulted to `'I' x len` for FastA, Perl 3273–3280):

1. **Instance scan order = `(0, 3, 1, 2)`** (Perl 3300) — OT, OB, then the complementary strands; a no-C/no-G
   read lands on an original strand first (matters under `--directional`). (SE uses 0,1,2,3; PE uses 0,3,1,2.)
   🔴 **rev1 (B I-1) — scan-slot vs spawn-slot:** the merge must index streams by **Bismark slot** (directional PE
   spawns 2 children but they occupy slots **0 and 3**, NOT spawn-order 0,1), and visit them in slot order 0,3,1,2.
   `BestAlignmentPaired.index`, the strand-bucket dispatch, and the directional reject (index 1|2) all key on the
   **Bismark slot**. The driver must map the 2 spawned `-1/-2` children to slots 0 (OT) and 3 (OB) — e.g. a 4-slot
   array with slots 1/2 empty, or an explicit `(slot, stream)` pairing — so the scan/index are slot-correct.
   (SE used a 2-elem vec at slots 0,1; the PE slots differ — easy to get wrong.)
2. For each instance whose `last_seq_id == identifier`: parse both lines (fields 0,1,2,3,4,5,9,10); strip `/1`
   from `id_1`, `/2` from `id_2` (3312–3313).
3. 🔴 **No-alignment marker is the FLAG _pair_ `flag_1==77 && flag_2==141`** (Perl 3317; 77=1+4+8+64,
   141=1+4+8+128) — *not* SE's single `flag==4`. On match: advance the stream to the next pair, `next` instance.
   🔴 **rev1 (A O-3):** unlike SE's no-align path (`merge.rs:156–160`), the PE (77,141) path (Perl 3317–3346) has
   **no** "die if the next seq-id is also `$identifier`" guard — do NOT copy the SE die into the PE path.
4. De-convert both RNAMEs (`s/_(CT|GA)_converted$//`, 3351–3362); **die unless `chr_1 eq chr_2`** (3364).
5. Extract `AS:i` + `MD:Z` for both mates (mandatory; die if missing, 3405–3406) and `XS:i` (R1) / `XS:i`-or-`ZS:i`
   (R2; the HISAT2 `ZS` arm is dead on the Bowtie 2 spine). **`sum_of_alignment_scores = AS_1 + AS_2`** (3416).
6. **Overwrite / `best_AS_so_far` / `amb_same_thread`** machinery — *byte-for-byte the SE structure* (3422–3463),
   but keyed on the **sum**: first sum → `overwrite=1`, set best (+ capture both first-ambig lines if `--ambig_bam`);
   sum `>= best` → `overwrite=1`; sum `> best` → reset `amb_same_thread`, recapture first-ambig; set best.
7. **Second-best handling** (3465–3590): if either mate has `XS`, default the missing one to its own `AS`
   (3466–3474); if both → `sum_2nd = XS_1 + XS_2` (3477). If `sum == sum_2nd` (within-thread tie): if `sum == best`
   → `amb_same_thread=1` (3485–3488); discard the rest of this read's pairs (3496–3519). Else store under the
   alignment-location key, discard the rest (3524–3589). If neither mate has `XS` → store, advance (3591–3648).
8. 🔴 **`alignment_location` key differs by branch** (replicate exactly):
   - second-best branch (3527–3532): `chr:min(pos1,pos2):max(pos1,pos2)` (`<=` → pos1:pos2, else pos2:pos1).
   - no-second-best branch (3593): `chr:pos1:pos2` **raw** (no min/max sort). Preserve this Perl inconsistency.
9. After the scan: `amb_same_thread` → `alignment_ambiguous=1` (3654). If ambiguous: `unsuitable_sequence_count++`;
   if `--ambig_bam` write **both** first-ambig lines after `s|/1\t|\t|` (R1) and `s|/2\t|\t|` (R2) (3677–3681);
   return **2** (`--ambiguous`) / **1** (`--unmapped`) / **0** (3665–3694). If `%alignments` empty:
   `no_single_alignment_found++`; return 1/0 (3697–3710).
10. **Unique-best selection** (3750–3825): 1 entry → accept. 2–4 entries → sort by `sum` desc; tie at the top →
    `sequence_pair_fails=1` → `unsuitable_sequence_count++`, return 2/1/0 (3828–3846). Else store the best + the
    `sum_2nd` for MAPQ via the **3811–3816 conditional** (best's own `sum_2nd` if defined && `> runner-up sum`,
    else runner-up sum). `> 4` → die (3823–3824).
11. 🔴 **Directional rejection** (3851–3856): if chosen index `== 1 || == 2` → `alignments_rejected_count++`,
    return 0. (Inert in directional PE since slots 1/2 aren't spawned; SE rejects index 2|3.)
12. `unique_best_alignment_count++` (3860); call PE genomic extraction (§3.4); then 🔴 **the two length guards
    evaluated IN ORDER, R1 then R2, each `return 0` + `genomic_sequence_could_not_be_extracted_count += 1` on first
    failure — R1 failing short-circuits so R2 is never checked** (Perl 3864→3867 returns before 3869); note a read
    reaching here was already counted in `unique_best_alignment_count`, so an edge pair counts in `unique_best` and
    in `could_not_extract` but lands in NO strand bucket (matches SE). Then `mapq = calc_mapq(len(seq1),
    Some(len(seq2)), sum, sum_2nd)` (3876); `methylation_call` per mate (3887–3888); `paired_end_sam_output` (3895);
    return 0.

**Outcome type:** a `DecisionPaired` mirroring `Decision` (recommend; see §9 Q3):
`UniqueBestPaired(BestAlignmentPaired)` | `AmbiguousPaired { first_ambig: Option<(String,String)> }` |
`NoAlignmentPaired` | `RejectedPaired`. `BestAlignmentPaired` carries both mates' POS/CIGAR/MD/bowtie-seq/flag +
shared chr/index/sum/sum_2nd/mapq. The genomic-extraction + XM call + SAM write happen in the **driver**
(as SE does in Phase 5), not inside the merge.

### 3.4 PE genomic-seq extraction + methylation call (`methylation.rs`)
🔴 **No fragment span.** Each mate is extracted **independently** from its own POS+CIGAR — two copies of the SE
CIGAR-walk (Perl 4530–4614 for mate1, 4617–4702 for mate2), then exactly one mate revcomp'd by index.

1. Factor the SE per-mate core (`extract_corresponding_genomic_sequence_single_end`, `methylation.rs:105–236`)
   into a reusable inner that returns `(non_bis, md_seq, end_pos, indels)` for a given POS+CIGAR + the index-driven
   +2 placement (5′ prepend for index 1/3, 3′ append for index 0/2 — keyed on the **combined PE index**, same for
   both mates, Perl 4533/4606/4620/4691). Call it twice. 🔴 **rev1 (B O-3): the shared inner MUST NOT hardcode the
   SE `>= 0` 5′ guard** — pass the 5′-guard predicate as a parameter (mate1 = strict `> 0`, mate2 = `>= 0`; step 3),
   or give mate1 its own guard, or the reuse silently regresses to SE's `>= 0` and breaks byte-identity at POS 1–3.
2. 🔴 **Edge guards return early** (Perl bare `return;` at 4538/4610/4626/4699) — they do NOT set a flag and
   continue; on early return the failing mate's sequence is left **short** while the other (already-walked) mate keeps
   its full `read_len+2` sequence (the **strand counter is skipped** — it sits past all guards at 4708+). 🔴 **rev1
   CRITICAL (B C-1 / A I-2): the real "could-not-extract" gate is the caller's `len == read_len + 2` check, run
   PER MATE IN ORDER** (Perl 3864 R1, 3869 R2): **R1-failure short-circuits with `return 0` BEFORE R2 is checked**
   (3867), each failure bumps `genomic_sequence_could_not_be_extracted_count` by **exactly 1**. So on a single-mate
   edge miss exactly one count fires and the *other* mate must retain its full sequence so the length comparison
   localises which mate failed (see the §4 `GenomicExtractionPaired` edge-state contract). Do NOT collapse to a
   pair-level flag or zero both mates. (Same shape as SE's line-3127 gate — `methylation.rs:52–54`.)
3. 🔴 **Guard asymmetry to preserve:** mate1 5′ guard is strict `($pos_1-2) > 0` (Perl 4535) while mate2 5′ guard is
   `($pos_2-2) >= 0` (Perl 4622) and the SE port uses `>= 0`. With the 0-based `pos = position - 1` (Perl 4513–4514),
   **mate1 requires `pos_1 >= 3`, i.e. SAM 1-based `position_1 >= 4`** (A O-1); mate2 requires `pos_2 >= 2`
   (`position_2 >= 3`). Do not copy-paste the SE `>= 0` guard onto mate1.
4. **Index dispatch + 4 PE strand counters** (4708–4775), reached only past all four edge guards:
   | index | counter | r1 strand | r2 strand | read_conv_1 | read_conv_2 | genome_conv | revcomp |
   |---|---|---|---|---|---|---|---|
   | 0 | `ct_ga_ct` | `+` | `-` | CT | GA | CT | **mate2** |
   | 1 | `ga_ct_ga` | `+` | `-` | GA | CT | GA | **mate2** |
   | 2 | `ga_ct_ct` | `-` | `+` | GA | CT | CT | **mate1** |
   | 3 | `ct_ga_ga` | `-` | `+` | CT | GA | GA | **mate1** |
   The revcomp'd mate is the `-`-strand one (`reverse_complement`, reused). If that mate `contains_deletion`, its
   MD-seq is revcomp'd alongside (4719/4735/4753/4769). `index > 3` → die (4773).
5. `methylation_call` is **reused verbatim per mate** (Perl 3887–3888 calls it twice with each mate's seq +
   already-revcomp'd genomic window + that mate's `read_conversion`). Both mates accumulate into the **same**
   pooled context `Counters` (the 8 `total_*` fields are run-global). `--slam` → `methylation_call_slam` (out of v1).

### 3.5 PE SAM output (`output.rs`) — two records per pair
Wrapper `print_bisulfite_mapping_results_paired_ends` (4203–4215) only does phred64 (fold into the QUAL offset,
as SE does); all logic is in `paired_end_sam_output` (8713–9225). Emit **mate1 then mate2** (fixed order,
regardless of leftmost). Reuse SE's `make_mismatch_string`/`hemming_dist`/`revcomp`/tag-insertion verbatim.

1. 🔴 **FLAG = a per-index constant pair** (default `!old_flag`; Perl 8825–8868) — port as a literal table, NOT
   bit-assembly. The index-1/2 pairs are R1↔R2 *first/second-in-pair* swapped (Perl 8821–8823, SeqMonk concordance):
   | index | strand | `flag_1` | `flag_2` |
   |---|---|---|---|
   | 0 (OT) | — | **99** (1+2+0x20+0x40) | **147** (1+2+0x10+0x80) |
   | 1 (CTOB) | swapped | **163** (1+2+0x20+0x80) | **83** (1+2+0x10+0x40) |
   | 2 (CTOT) | swapped | **147** (1+2+0x10+0x80) | **99** (1+2+0x20+0x40) |
   | 3 (OB) | — | **83** (1+2+0x10+0x40) | **163** (1+2+0x20+0x80) |
2. **POS** = `position_1`/`position_2`; **MAPQ** = the single shared `mapq` for both records (8872); **CIGAR** =
   the literal Bowtie 2 `cigar_1`/`cigar_2` (8876–8877).
3. 🔴 **RNEXT = `=` always** (8881; pairs guaranteed same-chr at 3364); **PNEXT = the *other* mate's POS**
   (`pnext_1 = pos_2`, `pnext_2 = pos_1`, 8885–8886). On the noodles `RecordBuf` set `mate_reference_sequence_id`
   = own tid (→ serialises as `=`), `mate_alignment_start` = other mate's POS — fields SE never touches.
4. 🔴 **TLEN** (signed `i32`; Perl 8890–8994) — port the full tree exactly (highest-risk after FLAG):
   - **A. `start_1 <= start_2`** (read1 leftmost, `<=`):
     - A1 `end_2 >= end_1`: **dovetail** (`flag_1==83 && dovetail`) → `tlen_1 = start_1-end_2-1`,
       `tlen_2 = end_2-start_1+1`; **normal** → `tlen_1 = end_2-start_1+1`, `tlen_2 = start_1-end_2-1`.
     - A2 `end_2 < end_1` (R2 contained) → `tlen_1 = end_1-start_1+1`, `tlen_2 = -(that)`.
   - **B. `start_2 < start_1`** (read2 leftmost, strict `<`):
     - B1 `end_1 >= end_2`: **dovetail** (`flag_1==99 && dovetail`) → `tlen_1 = end_1-start_2+1`,
       `tlen_2 = start_2-end_1-1`; **normal** → `tlen_2 = end_1-start_2+1`, `tlen_1 = start_2-end_1-1`.
     - B2 `end_1 < end_2` (R1 contained) → `tlen_1 = -(end_2-start_2+1)`, `tlen_2 = end_2-start_2+1`.
   - `$dovetail` defaults to **1** (Perl 8047–8048, unless `--no_dovetail`); the dovetail sub-cases key on the
     **literal FLAG constant** (83 for index-3 R1, 99 for index-0 R1). Sign convention: leftmost `+`, rightmost `−`.
     Preserve the `<=` (A) vs `<` (B) boundary exactly.
   - 🔴 **rev1 (A I-1) — total partition:** A and B form an `if/else if` (the `<=`/`<` partition is total), NOT two
     independent `if`s — Perl has no trailing `else` and relies on this so `tlen` is never left unset.
   - 🔴 **rev1 (B O-1) — start/end basis MISMATCH (do not "normalise"):** `start_1/start_2` = the **1-based** POS
     (`position_1/2`, Perl 8786–8787); `end_1/end_2` = the **0-based-walked** `end_position_1/2` from extraction
     (Perl 8793–8794; same basis as SE `GenomicExtraction.end_position`). The `+1`/`−1` constants in the formulas
     absorb this mix — use 1-based start + walked end, NOT both 1-based.
5. 🔴 **SEQ/QUAL/ref reorientation is per-mate, keyed on the stored `strand_1`/`strand_2`** (NOT the swapped FLAG,
   NOT the index alone; Perl 8999–9015): if a mate's strand is `-`, revcomp its `actual_seq` + `ref_seq`,
   conditionally double-revcomp its MD-seq iff its CIGAR contains `D`, and `reverse` its quality. (Same as SE
   `output.rs:386–393`, applied independently per mate. The revcomp'd mate matches the one extraction revcomp'd.)
6. **+2 ref trim is index-driven for both mates** (Perl 8772–8779; differs from SE's per-mate read_conv keying):
   index 0/3 → R1 drop last 2, R2 drop first 2; index 1/2 → R1 drop first 2, R2 drop last 2. 🔴 **rev1 (B I-3) —
   lock the keying basis to the Bismark INDEX, not `read_conversion`:** SE keys this trim on read_conv
   (`output.rs:373`), but for PE index 1 (CTOB) R1 has read_conv CT yet must drop the **first** 2 (because
   index ∈ {1,2}); keying on read_conv would mis-trim. Do not copy the SE read_conv-keyed trim.
7. **Tags per mate, order `NM MD XM XR XG`** (9022–9064; identical order to SE):
   - `NM:i:` = `hemming_dist(actual, ref) + indels` per mate (9022–9028).
   - `MD:Z:` = `make_mismatch_string(actual, ref, cigar, md_seq)` per mate (9032–9033, reuse).
   - `XM:Z:` = the methcall, `reverse`d iff that mate's strand is `-` (9040–9055, per mate).
   - 🔴 **`XR:Z:` differs per mate** (`read_conversion_1` vs `_2`, e.g. OT → R1 `CT`, R2 `GA`; 9059–9060);
     **`XG:Z:` is the SAME for both** (`genome_conversion`, pair-wide; 9064).
8. Chromosome RNAME is already de-converted at parse time (3351–3361); no further stripping. `--strandID`/`--rg_tag`/
   `--non_bs_mm` extra tags default OFF (out of the v1 gate; the bare `NM MD XM XR XG` form is the target).

### 3.6 PE driver (`lib.rs`) — `run_pe_directional`
Mirror `run_se_directional` (`lib.rs:118`) over **mate pairs**, naming everything off **R1's basename**.
1. Add the dispatch arm `(PairedEnd { mates1, mates2 }, Directional, FastQ)` → `run_pe_directional` (`lib.rs:100`).
2. Load the genome once. For each mate pair `(mates1[i], mates2[i])`:
   a. Convert both mates (§3.1) → `<r1>_C_to_T.fastq`, `<r2>_G_to_A.fastq`.
   b. Spawn the **2** paired instances (§3.2).
   c. Open both **original** FastQ files; read in **lockstep** — 4 lines from each per iteration (Perl 2602–2611;
      loop ends when any needed line is empty). `fix_id` both ids (2620–2621); capture the `@`-bearing originals
      for the aux files (2637–2638) **before** stripping the leading `@` from R1 only (2640). Apply `skip`/`upto`.
      `sequences_count++` per **pair** (2632). Call `check_results_paired_end(uc seq1, uc seq2, id1, q1, q2)` (2642).
      🔴 **rev1 (B I-4) — TWO distinct R1 id strings from the same line:** the merge gets R1 **`@`-stripped**
      (`$identifier_1`, after 2640); the aux files write R1 **`@`-bearing** (`$orig_identifier_1`, copied at 2637
      *before* the strip). R2 is **never** `@`-stripped, so R2's merge id and R2's aux id are both `@`-bearing. Keep
      both R1 strings; don't reuse the stripped id for the aux record (or vice-versa).
   d. Route the `DecisionPaired` (§3.7).
   e. After the loop: write the `_PE_report.txt` (§3.8); delete **both** temps (directional: `_C_to_T_1` +
      `_G_to_A_2`, Perl 2155; best-effort, never fatal); `finish()` the BAM/ambig-BAM.
3. **Output naming** (Perl 1767–1840): BAM stem = R1 basename, FastQ-suffix stripped (`strip_fastq_suffix`),
   `--prefix`/`--basename`; default suffix **`_bismark_bt2_pe.bam`** (lowercase `_pe`), basename → `<base>_pe.bam`.
   Report = **`_bismark_bt2_PE_report.txt`** (uppercase `_PE`), basename → `<base>_PE_report.txt`. One BAM holds
   **both** mates' records.

### 3.7 PE return-code routing + aux files
`DecisionPaired` → (Perl 2648–2674 + the return codes in `check_results_paired_end`):
- **`UniqueBestPaired`** → the two records are already written to the single `_pe.bam`.
- **`AmbiguousPaired`** → (a) if `--ambig_bam` and `first_ambig.is_some()` → write **both** raw de-converted lines
  to the ambig BAM (within-thread path only, Perl 3673–3682; cross-instance-tie carries `None`, 3828-block has no
  AMBIBAM write); the two lines have `s|/1\t|\t|` (R1) / `s|/2\t|\t|` (R2) applied at write (3677–3678). 🔴 **rev1
  (A O-2) ambig-BAM name:** `$outfile =~ s/sam$/ambig.bam/` (Perl 1788) → **`<r1stem>_bismark_bt2_pe.ambig.bam`**
  (`_pe.sam` → `_pe.ambig.bam`); reuse the same `generate_sam_header`. (b) precedence: `--ambiguous` → write R1's 4
  FastQ lines to **AMBIG_1** and R2's to **AMBIG_2**; else if `--unmapped` → UNMAPPED_1/UNMAPPED_2; else nothing.
  Uses the `@`-bearing `fix_id`'d originals + raw seq + verbatim `+` line + qual (2649–2659).
- **`NoAlignmentPaired`** → if `--unmapped` → UNMAPPED_1/UNMAPPED_2; else nothing (2662–2674).
- **`RejectedPaired`** / could-not-extract → nothing (counted only).
- 🔴 **Aux filenames** (Perl 1853–1938): the **un-stripped** R1/R2 basenames with `_unmapped_reads_1.fq`/`_2.fq`
  (and `_ambiguous_reads_1.fq`/`_2.fq`) inserted, `--prefix`/`--basename` variants; **single-core = gzipped**
  (`.gz`, Perl 1889–1892) → gate on decompressed content. (The multicore plain-then-merge path is Phase 9.)
- 🔴 **rev1 (A O-4) aux FastQ record newlines:** the `+` line is the **verbatim un-chomped** `$ident_` line (it
  keeps its own `\n`/`\r\n` — do NOT chomp or re-append); seq and qual are chomped then written with an explicit
  `\n` (Perl 2651–2659). Same contract as the SE `aux_out::write_fastq_record`.

### 3.8 PE report (`report.rs`) — `print_final_analysis_report_paired_ends`
Reuse the SE report scaffolding (header library, the cytosine half, the trailing wall-clock line — all
byte-identical for PE, Perl 2226–2312 == SE 2052–2136). The PE-specific differences (Perl 2186–2224):
1. Header: `Bismark report for: <f1> and <f2> (version: …)` — note **"and"** (1843); the directional line wording
   matches SE's directional line (1941); reuse `write_report_header` with the two read files.
2. Body wording swaps (7 lines: "Sequence pairs …" not "Sequences …"):
   - `Sequence pairs analysed in total:` (2195).
   - `Number of paired-end alignments with a unique best hit:` + `Mapping efficiency:\t<%.1f>%` — 🔴 **the REPORT
     line at 2205 has a trailing space after `%` then `\n` (`…% \n`)**, absent in SE; do NOT copy the STDOUT twin
     at 2204 (which is `%\n\n`, no trailing space).
   - `Sequence pairs with no alignments under any condition:` (2214); `Sequence pairs did not map uniquely:` (2215);
     `Sequence pairs which were discarded because genomic sequence could not be extracted:` (2216);
     `Number of sequence pairs with unique best (first) alignment came from the bowtie output:` (2217).
3. 🔴 **3-token strand labels** (2218; map index→counter) — 🔴 **rev1 (B I-5): emit in EXACTLY this Perl join
   order `0,2,1,3`** (NOT field-declaration order, NOT the scan order 0,3,1,2): `CT/GA/CT:\t<n>\t((converted) top
   strand)` (index 0, `ct_ga_ct`); `GA/CT/CT:\t<n>\t(complementary to (converted) top strand)` (index 2,
   `ga_ct_ct`); `GA/CT/GA:\t<n>\t(complementary to (converted) bottom strand)` (index 1, `ga_ct_ga`);
   `CT/GA/GA:\t<n>\t((converted) bottom strand)` (index 3, `ct_ga_ga`). A struct-field-order or scan-order
   iteration reorders the lines and breaks the gate.
4. The directional `…complementary strands being rejected…` line (gated on `--directional`, 2221–2224) is placed
   **after** the strand block, **before** the Cytosine report — same as SE.
5. The cytosine half (Total C's excl. Unknown; 4 me + 4 unme; 4 `%.1f` percentages with the `(me+unme)>0` gate;
   trailing `\n\n`) is **byte-identical to SE** → share `report.rs`'s existing cytosine code.

### Edge cases
- A pair where only one mate has an `XS` second-best → the other defaults to its own `AS` (3466–3474).
- A pair mapping to the same `chr:pos1:pos2` in both instances → one `alignments` entry → unique best (the
  branch-specific keying matters; §3.3 step 8).
- Chromosome-edge pair (either mate fails the `len==read_len+2` guard) → `genomic_sequence_could_not_be_extracted_count++`,
  counted in `unique_best` but in **no** strand bucket and written nowhere (mirrors SE Phase 5).
- `sequence_pairs == 0` → mapping efficiency `0%` (no div-by-zero).
- FastA PE (no qualities → `'I' x len`) is Phase 9; on the FastQ spine qualities are always present.
- `--ambig_bam` with neither `--ambiguous`/`--unmapped` → still writes the ambig BAM (independent).
- A pair with R1 leftmost vs R2 leftmost → TLEN sign + the A/B branch (§3.5 step 4); records still mate1-then-mate2.

---

## 4. Signatures (proposed)

```rust
// merge.rs — PE outcome types (mirror Decision/BestAlignment), + 4 PE Counters fields.
pub struct BestAlignmentPaired {
    pub chromosome: String, pub index: usize,
    pub position_1: u32, pub position_2: u32,
    pub cigar_1: String,  pub cigar_2: String,
    pub md_tag_1: String, pub md_tag_2: String,
    pub bowtie_sequence_1: String, pub bowtie_sequence_2: String,
    pub flag_1: u16, pub flag_2: u16,
    pub sum_of_alignment_scores: i64,
    pub sum_of_alignment_scores_second_best: Option<i64>,
    pub mapq: u8,
}
pub enum DecisionPaired {
    UniqueBest(BestAlignmentPaired),
    Ambiguous { first_ambig: Option<(String, String)> }, // (R1, R2) de-converted; Some only within-thread
    NoAlignment,
    Rejected,
}
#[allow(clippy::too_many_arguments)]
pub fn check_results_paired_end<S: PairedSamStream>(
    identifier: &str, sequence_1: &str, sequence_2: &str,
    streams: &mut [S], directional: bool,
    score_min_intercept: f64, score_min_slope: f64,
    want_ambig: bool, counters: &mut Counters,
) -> Result<DecisionPaired>;

// align.rs — peek-two / advance-two primitive.
pub trait PairedSamStream {
    fn current_pair(&self) -> Option<(&SamRecord, &SamRecord)>; // (R1, R2)
    fn advance_pair(&mut self) -> Result<()>;
}
pub struct PairedAlignerStream { /* child + last_line_1/2 + last_seq_id */ }

// methylation.rs — two-mate genomic extraction (no fragment span).
// 🔴 rev1 EDGE-STATE CONTRACT (B C-1 / A I-2): on a per-mate chromosome-edge guard miss
// (Perl bare `return;` at 4538/4610/4626/4699) the FAILING mate's `unmodified_genomic_sequence_N`
// is left SHORT while the OTHER (already-walked) mate keeps its FULL `read_len+2` sequence.
// LENGTH is the only signal (mirrors SE `methylation.rs:52–54`): NO `extracted` bool, NO
// pair-level "could-not-extract" flag, do NOT zero both. The driver gates per mate on
// `len == read_len + 2` (see §3.3 step 12 / §3.4 step 2). So both `Vec<u8>` MUST carry their
// real (possibly short) lengths verbatim.
pub struct GenomicExtractionPaired {
    pub alignment_read_1: u8, pub alignment_read_2: u8,            // b'+' / b'-'
    pub read_conversion_1: Conversion, pub read_conversion_2: Conversion,
    pub genome_conversion: Conversion,
    pub unmodified_genomic_sequence_1: Vec<u8>, pub unmodified_genomic_sequence_2: Vec<u8>,
    pub genomic_seq_for_md_tag_1: Vec<u8>, pub genomic_seq_for_md_tag_2: Vec<u8>,
    pub end_position_1: u32, pub end_position_2: u32,
    pub indels_1: u32, pub indels_2: u32,
}
pub fn extract_corresponding_genomic_sequence_paired_end(
    best: &BestAlignmentPaired, genome: &Genome, counters: &mut Counters,
) -> Result<GenomicExtractionPaired>;

// output.rs — two records per pair (RNEXT='=', PNEXT/TLEN/mate-link set).
// 🔴 rev1 (B I-2): `dovetail` MUST be threaded in (= `!cli.no_dovetail`, Perl 8047–8048);
// the TLEN dovetail sub-cases (8899/8950) gate on it and it is NOT derivable from best/ext.
#[allow(clippy::too_many_arguments)]
pub fn paired_end_sam_output(
    id: &str, seq_1: &str, seq_2: &str, qual_1: &str, qual_2: &str,
    best: &BestAlignmentPaired, ext: &GenomicExtractionPaired,
    methcall_1: &[u8], methcall_2: &[u8], refid: &HashMap<String, usize>,
    phred64: bool, dovetail: bool,
) -> Result<(BismarkRecord, BismarkRecord)>;

// report.rs — PE header needs the SECOND read file (Perl 1843 `for: <f1> and <f2>`).
// 🔴 rev1 (B I-6): extend the existing `ReportHeader` (lib.rs:184, single `sequence_file`)
// with `sequence_file2: Option<&str>` (SE passes None) and reuse `write_report_header`.
pub fn print_final_analysis_report_paired_ends(w: &mut impl Write, c: &Counters, directional: bool) -> Result<()>;

// aux_out.rs — 🔴 rev1 (A I-3): the EXISTING signature is
//   `aux_filename(filename: &str, prefix, basename, kind: AuxKind, fasta: bool) -> String` (aux_out.rs:40).
// EXTEND it in place with a trailing `mate: Option<u8>` (SE call sites pass `None`); do NOT
// invent the `(read_file, config, …)` shape — that would churn the SE call site + risk the SE gate.
pub fn aux_filename(filename: &str, prefix: Option<&str>, basename: Option<&str>,
                    kind: AuxKind, fasta: bool, mate: Option<u8>) -> String;

// lib.rs
fn run_pe_directional(config: &RunConfig, mates1: &[String], mates2: &[String]) -> Result<()>;
// PE Sinks: 1 BAM + optional ambig-BAM + (unmapped_1, unmapped_2) + (ambiguous_1, ambiguous_2).
```

---

## 5. Implementation outline (TDD; build inner-out)

0. **Verify prerequisites** (no code if already true): `options::build_aligner_options(.., is_paired=true)`
   emits the PE flags in Perl push order; `ReadLayout::PairedEnd` resolves `-1`/`-2`. 🔴 **rev1 (A I-4): pin the
   FULL `aligner_options` string Perl emits for the gate argv, not just the PE subset** — `--no-mixed`/
   `--no-discordant`/`--dovetail` are pushed in the PE block (8044–8056) but `--maxins 500` is pushed **later**
   after `chdir` (8135), and `--quiet` after that (8141); **default WGBS has NO `--minins`** (only if `-I` given,
   8123–8125). A `--maxins`-without-`--minins` default is correct. Pin the complete string in a unit test.
1. **`mapq.rs`** — none (PE signature already present); add a PE-arg unit test (`calc_mapq(50, Some(50), sum, 2nd)`).
2. **`convert.rs`** — factor the shared per-record core; add the PE entry (R1 C→T / R2 forward G→A) + the
   `/1/1`,`/2/2` suffix. Unit-test: G→A bytes; the doubled suffix inserted before `\n`; both temp names; CRLF.
3. **`align.rs`** — `PairedSamStream` + `PairedAlignerStream` (peek two, R1-by-`/1`, child-pipe contract). Unit-test
   with a canned two-line-per-pair `VecPairStream` double (mirror the Phase-4 `VecStream`); test R1=line-2 case.
4. **`merge.rs`** — `BestAlignmentPaired` / `DecisionPaired`; `check_results_paired_end` (scan 0,3,1,2; (77,141)
   no-align; sum selection; the two-branch location keying; directional reject 1|2). Add the 4 PE strand
   `Counters` fields. Unit-test every branch with canned pair streams (unique, cross-tie, within-thread tie,
   no-align, contained-mate location key, directional-reject, >4 die, sum-2nd conditional).
5. **`methylation.rs`** — `extract_corresponding_genomic_sequence_paired_end` (two per-mate calls; index-driven +2;
   4-counter dispatch; revcomp the `-` mate; the mate1-5′-strict guard). Unit-test: each index's revcomp target +
   counter; the per-mate length-guard miss (chr edge); a deletion mate's MD double-revcomp.
6. **`output.rs`** — `paired_end_sam_output` (FLAG table, TLEN tree, RNEXT/PNEXT/mate-link, per-mate revcomp/XR,
   shared XG, +2 index-driven trim). 🔴 **Build a Perl-differential harness** (like Phase 4 `calc_mapq` /
   Phase 5 `make_mismatch_string`): feed a matrix of (index × R1/R2 layout × dovetail × containment) through both
   the live Perl `paired_end_SAM_output` and the Rust port; assert byte-identical FLAG + TLEN + the full record.
7. **`report.rs`** — `print_final_analysis_report_paired_ends` (7 wording swaps; 3-token labels; the `% \n`
   trailing-space at 2205; reuse cytosine half + header + wall-clock). Unit-test exact bytes for a canned
   `Counters` incl. 0-pairs and the trailing-space quirk.
8. **`aux_out.rs`** — `aux_filename(.., mate)` (`_reads_1`/`_reads_2`, un-stripped basename). Unit-test names + variants.
9. **`lib.rs`** — `pipeline()` PE arm; `run_pe_directional` (genome once → per pair: convert → 2 instances →
   two-file lockstep → route → report → two-temp cleanup); PE `Sinks` (2 unmapped + 2 ambiguous + 1 BAM + 1 ambig-BAM).
   Shrink `deferred_flags` (PE now active). Integration tests via the fake-bowtie2 harness (extend `tests/cli.rs`
   to emit PE SAM): mapped pair end-to-end; unmapped→`_1`/`_2`; ambiguous + `--ambig_bam` (two lines).
10. **Gate harness** — extend the oxy `scripts/` harness to a PE run (`10M_PE`): diff `samtools view -h` of the
    `_pe.bam`, the `_PE_report.txt` (filter `^Bismark completed in ` + samtools `@PG`), and `zcat` of the
    `_1`/`_2` unmapped/ambiguous, vs Perl v0.25.1 + Bowtie 2 2.5.5 (identical argv). Local goldens first, oxy gate last.

---

## 6. Efficiency
No new genome passes (extraction reuses the in-memory genome). The merge is O(pairs); two records/pair doubles the
BAM write vs SE but is inherent. The `first_ambig` clones are gated on `--ambig_bam`. Two converted temp files +
two original-FastQ readers per pair-file — the same I/O shape as Perl. mimalloc already global (output-neutral).

## 7. Validation
| # | Verify | How | Expected |
|---|--------|-----|----------|
| 1 | PE `aligner_options` | unit | `--no-mixed --no-discordant --dovetail … --maxins 500` in Perl push order |
| 2 | R2 conversion = forward G→A (not revcomp) | unit | `g→A`; `/2/2` suffix before `\n`; R1 `/1/1` + C→T |
| 3 | Paired stream R1 identification | unit: canned pairs incl. R1=line-2 | `last_line_1`=R1; `die` if neither id ends `/1` |
| 4 | Merge: unique best by **sum** of AS | unit | best pair chosen; index correct; sum_2nd per 3811–3816 |
| 5 | Merge: (77,141) no-align pair | unit | advances, contributes no alignment |
| 6 | Merge: within-thread tie vs cross-instance tie | unit | both → Ambiguous; first_ambig `Some` only within-thread |
| 7 | Merge: location key (contained mate) | unit | second-best branch min/max vs no-2nd raw pos1:pos2 |
| 8 | Merge: directional reject index 1\|2 | unit | `RejectedPaired`; `alignments_rejected_count++` |
| 9 | Extraction: per-mate, no fragment span; revcomp target per index | unit | index 0/1 → mate2 revcomp; 2/3 → mate1 |
| 10 | 🔴 **Per-mate could-not-extract short-circuit** (C-1) | unit: (a) R2 at chr-edge, R1 fine; (b) R1 at edge | (a) **exactly one** count, R1 guard passes, R2 guard fires; (b) R1 guard fires + `return 0`, **R2 guard never evaluated**; both → no strand bucket, written nowhere, still in `unique_best` |
| 11 | Extraction: mate1 5′ strict `>0` vs mate2 `>=0` | unit: 1-based `position_1==3` (→pos_1=2) vs `position_2==2` | mate1 fails (needs `position_1>=4`), mate2 passes |
| 12 | 🔴 **SAM FLAG table** (4 indices) | unit + **Perl-differential** | exact `flag_1`/`flag_2` incl. index-1/2 swap |
| 13 | 🔴 **TLEN tree** (A1/A2/B1/B2 + dovetail) | unit + **Perl-differential**, incl. 🔴 **rev1**: equality cells (`start_1==start_2`→branch A; `end_2==end_1`; `end_1==end_2`), the dovetail FLAG-gate **negative** (index-1/2 dovetailing layout must NOT take the dovetail branch), and **`--no_dovetail`** as a separate axis | sign + magnitude byte-identical on every cell |
| 14 | RNEXT/PNEXT/MAPQ | unit | RNEXT `=`; PNEXT = other mate POS; MAPQ shared |
| 15 | Per-mate revcomp keyed on stored strand; XR per-mate, XG shared | unit | `-` mate revcomp'd; XR_1≠XR_2; XG_1==XG_2 |
| 16 | Tag order `NM MD XM XR XG`; +2 index-driven trim | unit | order exact; trim per index 0/3 vs 1/2 |
| 17 | PE report bytes | unit: canned `Counters` | "Sequence pairs …"; 3-token labels; 🔴 `Mapping efficiency:\t<p>% \n` |
| 18 | Aux filenames `_reads_1`/`_reads_2` | unit | un-stripped basename; `--prefix`/`--basename`; `.fq.gz` |
| 19 | Routing: ambiguous→`_1`/`_2`; precedence; ambig-BAM two lines | integration | per §3.7 (decompressed) |
| 20 | Two-temp cleanup (best-effort) | driver unit | `_C_to_T_1` + `_G_to_A_2` deleted; failure non-fatal |
| 21 | 🎯 **oxy PE gate** | extend harness on `10M_PE`, identical argv | `_pe.bam` + `_PE_report.txt` (filter wall-clock+@PG) + `_1`/`_2` aux byte-identical (incl. `--ambig_bam` two-line output on a real ambiguous pair) |
| 22 | 🔴 **rev1** Single-mate `XS` (B O-5) | unit: R1 has `XS`, R2 none | R2 defaults to its own `AS`; `sum_2nd = XS_1 + AS_2` |
| 23 | 🔴 **rev1** Two R1 id strings (B I-4) | unit/integration | merge gets R1 `@`-stripped; AMBIG_1/UNMAPPED_1 record carries R1 `@`-bearing; both BAM records share the `@`-stripped QNAME |
| 24 | 🔴 **rev1** Report strand-label JOIN order (B I-5) | unit: canned 4 counts | lines emitted in order `0,2,1,3` (CT/GA/CT, GA/CT/CT, GA/CT/GA, CT/GA/GA) |
| 25 | 🔴 **rev1** ambig-BAM de-convert non-mangling (B O-2) | unit: QNAME contains literal `_CT_converted` | only the RNAME suffix stripped; QNAME intact (`s/_(CT|GA)_converted//` unanchored, non-global → first occ = RNAME) |

## 8. Assumptions
**From epic:** Perl v0.25.1 + Bowtie 2 2.5.5 oracle; byte-identity on **decompressed** SAM content + decompressed
aux; adjudicate on **Linux/oxy**; identical argv (the report embeds the two read paths + `genome_folder`, and the
Bismark `@PG CL:` is verbatim; samtools `@PG` normalized out). Strand-instance table fixed. **Phase-specific:**
directional only (non-dir/pbat = Phase 8); single-file/single-core (FastA + multicore plain-then-merge = Phase 9);
`--slam` PE out of scope; `mapq.rs`/`calc_mapq` unchanged; the 8 context counters are pooled (run-global) across
mates; the CLI/config PE surface already exists (Phase 1, verified in §5 step 0); `--unmapped`/`--ambiguous`/
`--ambig_bam` reuse Phase 6's mechanisms with `_1`/`_2` mate variants.

## 9. Questions or ambiguities
- **(Open Q1 — kickoff #1; SPEC §4 says yes — confirm)** v1 PE scope = **directional first**, with non-directional
  + pbat as **Phase 8**. *Recommend: yes* (matches the SE expansion order; pbat/non-dir add the other 2 instances +
  the pbat conversion swap + the pbat report wording at 1944).
- **(🔴 Finding — kickoff #2; RESOLVED by source, §0)** Directional PE runs **2** instances (slots 0/3), **not 4**.
  The kickoff and SPEC §1 ("all 4") are wrong for directional; "all 4" is non-directional only. *Recommend: build
  the 2-instance path; correct SPEC §1's wording.* The index-1|2 directional rejection is ported as defensive
  (inert until Phase 8). **Please confirm you're happy treating this as a finding rather than re-confirming "4".**
- **(Open Q3 — kickoff #3)** Merge-seam modeling. *Recommend: separate parallel types* (`DecisionPaired` /
  `BestAlignmentPaired` / `PairedSamStream`) mirroring the SE `Decision`/`BestAlignment`/`SamStream` — cleanest,
  keeps the SE path untouched, matches how Phases 4–6 layered. **Alternatives surfaced:** (a) overload the existing
  `Decision`/`BestAlignment` with `Option` mate-2 fields (less code, but every SE match site must handle PE-shaped
  `None`s — noisier); (b) generic-over-mate-count (premature; only 1 vs 2). *Recommend the parallel types.*
- **(Open Q4 — fidelity note, RESOLVED replicate-exactly)** The `alignment_location` key is `chr:min:max` in the
  second-best branch (3527–3532) but raw `chr:pos1:pos2` in the no-second-best branch (3593). This Perl
  inconsistency is preserved verbatim (§3.3 step 8); a unit test pins both branches.
- **(Open Q5 — design note, RESOLVED)** Add **4 new PE strand-counter fields** (`ct_ga_ct`/`ga_ct_ga`/`ga_ct_ct`/
  `ct_ga_ga`) to `Counters` rather than reusing the SE 2-token fields — Perl uses distinct `%counting` keys and
  the report labels are 3-token, so distinct fields keep the report unambiguous (a run is SE *xor* PE).
- **(Open Q6 — verify, low-risk)** `paired_end_SAM_output` was summarized by an orientation agent, not read line-by-line
  by the planner; the implementer **must read 8713–9225 directly** and validate the FLAG + TLEN tables with the
  Perl-differential harness (§5 step 6, §7 #12/#13) before the oxy gate — these two are the dominant byte-identity risk.

## 10. Self-Review
- **Logic:** the merge control flow (scan 0,3,1,2; (77,141); sum selection; overwrite/within-thread/cross-tie;
  directional reject 1|2; two length guards; `calc_mapq(.., Some(len2), ..)`) traced to Perl 3269–3897 (read in
  full by the planner). The SAM FLAG/TLEN/tag rules traced to 8713–9225 (via agent + line cites; flagged for direct
  re-read + differential, Q6). ✓
- **Edge cases:** chr-edge per-mate guard, contained-mate location key, single-mate `XS`, 0-pairs efficiency,
  ambig-without-aux-flags, R1-vs-R2-leftmost TLEN, FastA-quality default (Phase 9). ✓
- **Integration:** reuses `calc_mapq` (unchanged), `methylation_call`/`make_mismatch_string`/`revcomp`/`hemming_dist`,
  the Phase-5 genome loader + BAM writer + `generate_sam_header`, the Phase-6 `report.rs` cytosine half + `aux_out.rs`
  + the `Sinks` routing. New: PE types, the paired stream, the PE converter suffix, the two-record SAM, the PE report
  wording, the `_1`/`_2` aux. SE paths untouched (parallel types). ✓
- **Risks:** (1) **TLEN + FLAG** (the only genuinely new bit-twiddling) → Perl-differential harness + unit pinning;
  (2) the per-mate-vs-index revcomp/+2-trim keying (easy to mis-key on strand vs index); (3) the mate1-5′ strict
  guard asymmetry; (4) the report `% \n` trailing-space quirk; (5) aux un-stripped names. All are unit-pinnable
  before the oxy gate; the gate is the final byte-identity arbiter.

## 12. Implementation Notes (2026-06-02)

**Status: IMPLEMENTED — 192 tests green (171 lib + 21 integration); clippy `-D warnings` + `cargo fmt --check`
clean.** Local only; the oxy PE byte-identity gate (§7 #21) is pending. Awaiting dual code-review + plan-manager.

### What was built (module by module)
- **`convert.rs`** — factored a shared `convert_fastq_impl(kind, id_suffix, file_base)`; `bisulfite_convert_fastq_se`
  delegates (C→T, no suffix), new `bisulfite_convert_fastq_pe(read_number)` does R1 C→T + `/1/1` / R2 forward
  **G→A** (`convert_seq_g_to_a`) + `/2/2`, inserted before the trailing `\n`. SE byte-tests unchanged + green.
- **`align.rs`** — `SamPair` (R1 canonicalised by the `/1` suffix; `is_unmapped_pair` = flags 77 & 141),
  `PairedSamStream` trait + `PairedAlignerStream` (`-1/-2` spawn, peek-two/advance-two, same child-pipe contract).
- **`merge.rs`** — `BestAlignmentPaired`, `DecisionPaired`, `check_results_paired_end<S: PairedSamStream>` over a
  **slot-indexed `&mut [Option<S>]`** (slots 0/3 live for directional; scan order 0,3,1,2); sum-of-AS selection;
  the two-branch location key (min/max vs raw); directional reject index 1|2; 4 new 3-token PE counters.
- **`methylation.rs`** — `GenomicExtractionPaired` + `extract_corresponding_genomic_sequence_paired_end` via a
  `walk_mate` helper (the 5′-guard predicate is a **parameter** — mate1 strict `>0`, mate2 `>=0`); sequential
  walk with the edge-state contract (failing mate left short, other mate full); counter + revcomp only past all
  four guards. `methylation_call` reused verbatim per mate.
- **`output.rs`** — `paired_end_sam_output` (FLAG constant table, the total-partition TLEN tree with the dovetail
  83/99 FLAG gates, RNEXT `=` via mate-tid, PNEXT, signed TLEN, index-keyed +2 trim, per-mate revcomp/XR, shared
  XG, tag order NM MD XM XR XG) + `write_raw_pe_ambig_lines` (strips `/1\t`,`/2\t`, de-converts RNAME).
- **`report.rs`** — `print_final_analysis_report_paired_ends` (the 7 "Sequence pairs" swaps, the `% \n`
  trailing-space at 2205, the 0,2,1,3 strand-label join order) + `ReportHeader.sequence_file2` + a shared
  `write_cytosine_report` (reused by SE + PE).
- **`aux_out.rs`** — `aux_filename` gained `mate: Option<u8>` (`_reads_1`/`_reads_2`).
- **`lib.rs`** — `pipeline()` PE arm → `run_pe_directional`; `PeSinks` (1 BAM + 1 ambig-BAM + 2 unmapped + 2
  ambiguous); `drive_merge_pe` (two-file lockstep, R1 `@`-stripped merge id vs R1/R2 `@`-bearing aux ids, the
  in-order R1-short-circuit length guards, routing with precedence); `dovetail` derived from `aligner_options`.

### rev-1 findings — all addressed
C-1 per-mate could-not-extract short-circuit (lib.rs in-order guards + `pe_mate2_chr_edge_leaves_mate1_full_mate2_short`);
I-1 TLEN total `if/else` (output.rs); I-3 aux_filename extended in place; I-4 full `aligner_options` (options.rs
test already pins it); I-5 FLAG/TLEN differential matrix incl. equality + dovetail-gate-negative + `--no_dovetail`
(`pe_tlen_tree`, `pe_dovetail_gate_negative_index1_not_dovetailed`); B I-1 slot-indexed streams; B I-2 `dovetail`
param; B I-3 index-keyed +2 trim; B I-4 two R1 id strings (`drive_merge_pe`); B I-5/I-6 join order + 2nd report file;
plus the optionals (pos basis, `_pe.ambig.bam`, no-die-if-same-id, aux newline, single-mate-XS).

### Deviations / notes
- `dovetail` is derived from `config.aligner_options.contains("--dovetail")` (set by `options.rs` for paired
  && `!--no_dovetail`) rather than a new `RunConfig` field — exactly tracks Perl's `$dovetail`, zero config churn.
- The FLAG/TLEN unit tests pin **hand-derived** values from the Perl source read line-by-line (8713–9225); the
  **authoritative Perl-differential is the oxy gate** (§7 #21, Phase 10), as Phase 4/5 did for calc_mapq/MD.
- `--old_flag` remains deferred (only the default `!old_flag` FLAG path is implemented).
- Non-directional/pbat (4-instance), FastA PE, multicore plain-then-merge aux: out of phase (8/9), as planned.

### Dual code-review + plan-manager (2026-06-02)
- **Code-review A: APPROVE** (0 Critical/High). **Code-review B: REQUEST-CHANGES — 1 Critical (C-1)**, no contradiction
  (A simply missed it; both confirmed the alignment/merge/extraction/SAM-output spine byte-faithful). **Plan-manager:
  COMPLETE** (every §3/§4/§7 item + rev-1 finding implemented; one validation-only gap at §7 #25).
- 🔴 **C-1 (B, FIXED) — PE report HEADER order/newlines.** The shared `write_report_header` emitted the **SE** line
  order for PE. Verified against source: SE (Perl 1642/1712/1722) = report-for → `--directional`(`\n`) → `was run
  with`(`\n\n`); **PE (1843/1846/1941) swaps lines 2&3** → report-for → `was run with`(`\n`) → `--directional`(`\n\n`),
  and the pbat wording differs (1715 "(OT and OB) strands" vs 1944 "(OT, OB)"). Fixed: `write_report_header` now
  branches on `paired`, with a `library_line(library, paired)` helper; `pe_header_two_files` corrected. Would have
  failed the §7 #21 oxy `_PE_report.txt` gate.
- **§7 #25 (plan-manager) — CLOSED**: added `pe_ambig_lines_strip_read_tag_and_deconvert_rname_only` (the `/1`,`/2`
  qname-tag strip + RNAME-only de-convert + QNAME-`_CT_converted` non-mangling).
- B M-1 (R1/R2 aux id re-adds `@`; diverges only on `@`-less malformed FastQ) + A/B Lows: noted, no action
  (correct-by-design for real data). **Post-fix: 172 lib + 21 integration tests green; clippy -D + fmt clean.**
  Reports: `CODE_REVIEW_A.md` / `CODE_REVIEW_B.md` / `COVERAGE.md`.

### 🎯 oxy PE byte-identity gate (§7 #21) — ✅ PASSED 2026-06-02 (`GATE_OXY.md`)
`bismark_rs` built on oxy from the worktree vs Perl v0.25.1 + Bowtie 2 2.5.5 + samtools 1.23.1, real GRCh38
PE-directional WGBS (`10M_PE` subset), **identical argv + `--unmapped --ambiguous --ambig_bam`**. **10k PASS
(post-fix) + 1M PASS**: `_pe.bam` (1,703,342 rec) + `_PE_report.txt` + `_1`/`_2` unmapped (355,576 ea) +
ambiguous (237,736 ea) + `_pe.ambig.bam` (103,110 rec) ALL byte-identical (samtools `@PG` + wall-clock filtered).
- 🔴 **Gate-found defect (FIXED) — `build_raw_record` dropped RNEXT/PNEXT/TLEN.** The first 10k run was identical
  on BAM+report+all 4 aux but the **ambig BAM** rendered fields 6/7/8 as `* 0 0`: the raw-passthrough builder
  (shared with the SE `--ambig_bam` path) parsed only fields 0–5/9–10+tags, dropping the bowtie2 PE line's
  `=`/`<mate-pos>`/`<tlen>`. Invisible to unit tests (SE raw lines carry `*/0/0` there) AND to the dual
  code-review (raw passthrough bypasses the FLAG/TLEN logic) — a textbook gate-only bug. Fixed: `build_raw_record`
  preserves RNEXT (`=`→own tid)/PNEXT/TLEN; SE unchanged. Locked by the strengthened ambig unit test. Re-ran → PASS.
**Phase 7 is byte-identical at 1M-pair scale. The full 10M PE + SE + RRBS run is Phase 10.** oxy scratch cleaned up.

## 11. Revision History
- **rev 1 (2026-06-02)** — folded dual plan-review (`PLAN_REVIEW_A.md` 0C/5I/6O, `PLAN_REVIEW_B.md` 1C/5I/5O; both
  APPROVE-WITH-FINDINGS, **both independently re-derived all 8 high-risk claims from source and found zero factual
  errors**; no contradictions, only two severity splits — resolved to the stricter rating). Folded:
  - 🔴 **Critical (B C-1 / A I-2) — per-mate could-not-extract contract:** the two length guards run **in order with
    R1 short-circuit** (3864→3867 before 3869), each counts once; the non-failing mate retains its full sequence so
    LENGTH localises the failure; `GenomicExtractionPaired` carries both `Vec<u8>` at real length, no pair-level flag
    (§3.3 step 12, §3.4 steps 1–2, §4 edge-state contract, §7 #10).
  - **Important:** TLEN total `if/else if` partition (A I-1, §3.5 step 4); aux_filename = extend the **existing**
    `(filename,prefix,basename,kind,fasta)` sig with trailing `mate: Option<u8>` (A I-3, §4); pin the **full**
    `aligner_options` string — default WGBS has no `--minins`, `--maxins 500` pushed later (A I-4, §5 step 0);
    broaden the FLAG/TLEN differential — equality cells + dovetail-gate-negative + `--no_dovetail` axis (A I-5/B
    V-GAP-2, §7 #13); scan-slot vs spawn-slot — index by Bismark slots 0/3, scan 0,3,1,2 (B I-1, §3.3 step 1);
    `paired_end_sam_output` needs a `dovetail: bool` param (B I-2, §4); +2-trim keyed on **index** not read_conv
    (B I-3, §3.5 step 6); two R1 id strings — merge `@`-stripped vs aux `@`-bearing (B I-4, §3.6 step 2c, §7 #23);
    report strand-label **join order 0,2,1,3** + `ReportHeader` second-file field (B I-5/I-6, §3.8 step 3, §4, §7 #24).
  - **Optional:** 0-based `pos_1>=3`⇒1-based `position_1>=4` clarity (A O-1, §3.4 step 3); ambig-BAM name
    `_pe.ambig.bam` (A O-2, §3.7, §7 #25); PE no-die-if-same-id (A O-3, §3.3 step 3); aux `+`-line verbatim newline
    (A O-4, §3.7); TLEN 1-based-start/0-based-end basis (B O-1, §3.5 step 4); reusable-inner guard parametrisation
    (B O-3, §3.4 step 1); single-mate-XS test (B O-5, §7 #22).
  Q3 (parallel PE types) and Q6 (Perl-differential FLAG/TLEN harness) both endorsed by both reviewers; the §0
  2-instance finding independently confirmed by both. No re-architecture. Approved by Felix → proceed to implement.
- **rev 0 (2026-06-02)** — initial plan (planner-authored after orienting on the kickoff, EPIC/SPEC, Phase 4/5/6
  plans + code, and the Perl PE source). Surfaces the **2-vs-4-instance correction** (§0), recommends parallel PE
  types, and flags `paired_end_SAM_output` for direct re-read + a Perl-differential FLAG/TLEN harness. Awaiting
  manual review → (after approval) dual plan-review → implement trigger.

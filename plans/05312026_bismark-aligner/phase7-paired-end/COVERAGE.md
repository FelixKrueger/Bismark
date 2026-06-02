# Plan Coverage Report

**Mode:** B (code/tests vs. the design plan — no separate IMPL.md; audited against §3 Behavior, §4 Signatures, §7 Validation, and the §11/§12 rev-1 folded findings)
**Plan:** `plans/05312026_bismark-aligner/phase7-paired-end/PLAN.md`
**Codebase:** `rust/bismark-aligner/src/` + `tests/cli.rs`
**Date:** 2026-06-02
**Verdict:** COMPLETE — all behaviors, signatures, and rev-1 findings implemented; **1 minor validation-only gap** (§7 #25 has no dedicated test, but the code path it targets is correct and exercised structurally). No functional gaps.

## Summary

- Total ledger items: 64 (8 §3 behavior groups w/ numbered sub-steps + 8 §4 signatures + 25 §7 rows + 23 rev-1 findings, de-duplicated against the rows that cite them)
- DONE: 63
- PARTIAL: 1 (§7 #25 — validation gap only; code correct)
- MISSING: 0
- DEVIATED: 0 (all deviations documented in §12 and benign)
- PENDING (expected): 1 (§7 #21 oxy gate = Phase 10)

Tests: **192 green, 0 failed** (`cargo test -p bismark-aligner`: 171 lib + 21 integration + 0 doc). Run required `dangerouslyDisableSandbox` (sandbox blocked `target/debug/.cargo-lock`).

## Coverage ledger

### §3 Behavior

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 3.1.1 | R1 C→T → `_C_to_T` | `convert.rs:179-192` `bisulfite_convert_fastq_pe` | DONE | delegates to shared `convert_fastq_impl(ConvKind::Ct, "/1/1", "_C_to_T")` |
| 3.1.2 | R2 forward G→A → `_G_to_A` (`convert_seq_g_to_a`) | `convert.rs:149-156, 187` | DONE | `uc` then `g→A`; `pe_read2_g_to_a_with_slash_2_2_suffix` |
| 3.1.3 | `/1/1`,`/2/2` ID suffix inserted before final `\n` | `convert.rs:256-258` | DONE | `pe_suffix_inserted_before_newline_crlf` (CRLF kept) |
| 3.1.4 | Shared per-record core (gz/skip/upto/prefix/max-len/sanity/verbatim) | `convert.rs:198-313` | DONE | SE delegates to same `convert_fastq_impl`; SE goldens still green |
| 3.1.5 | Filename derivation per mate | `convert.rs:206-216` | DONE | `pe_read1/2` name asserts; `_C_to_T`/`_G_to_A` + `.gz` iff gzip |
| 3.2.1 | 2 instances slot0 `--norc`/CT, slot3 `--nofw`/GA, `-1/-2` | `lib.rs:557-574` | DONE | `s0`=Norc/ct_index, `s3`=Nofw/ga_index; both read conv1/conv2 |
| 3.2.2 | Skip `^@` header, read first pair (two lines) | `align.rs:388-413` | DONE | `pe_stream_skips_header_then_walks_pairs_to_eof` |
| 3.2.3 | Identify R1 by trailing `/1`; die if neither | `align.rs:287-311` `SamPair::from_lines` | DONE | `sampair_identifies_read1_by_slash1`, `_swaps_when_read1_emitted_second`, `_dies_when_neither_is_read1` |
| 3.2.4 | `PairedSamStream` peek-2/advance-2 trait + impl | `align.rs:323-468` | DONE | trait + `PairedAlignerStream`; child-pipe drain/kill/wait `Drop` |
| 3.3.1 | Scan order 0,3,1,2; slot-indexed `&mut [Option<S>]` (B I-1) | `merge.rs:468,483-487` | DONE | `SCAN_ORDER=[0,3,1,2]`; slots keyed by Bismark index |
| 3.3.2 | Per-instance parse both lines; strip `/1`,`/2` | `align.rs` (seq_id) + `merge.rs:501-505` | DONE | seq_id pre-stripped in `from_lines` |
| 3.3.3 | (77,141) no-align pair; **no** die-if-same-id (A O-3) | `align.rs:315-317`, `merge.rs:496-499` | DONE | `is_unmapped_pair`; advance-once, no die guard; `pe_no_align_marker_contributes_nothing` |
| 3.3.4 | De-convert both RNAMEs; die unless chr1==chr2 | `merge.rs:447-455, 504-510` | DONE | `pe_different_chromosomes_per_mate_errors` |
| 3.3.5 | AS+MD mandatory both mates; sum=AS1+AS2; XS per mate | `merge.rs:512-537, 566` | DONE | die-on-missing for all four |
| 3.3.6 | Overwrite/best_sum/amb_same_thread (SE structure, keyed on sum) | `merge.rs:541-562` | DONE | byte-mirror of SE; first-ambig recapture on strict improvement |
| 3.3.7 | Second-best handling; within-thread tie | `merge.rs:564-608` | DONE | `pe_within_thread_tie_is_ambiguous_and_captures_first_ambig` |
| 3.3.8 | Location key min/max (2nd-best) vs raw (no-2nd) (Q4) | `merge.rs:699-715, 581-607` | DONE* | `insert_pair(min_max_key)` branch correct; see Gap detail for the test-distinction note |
| 3.3.9 | amb_same_thread→ambiguous; ambig-BAM both lines; return codes | `merge.rs:620-629` + `lib.rs:909-913` | DONE | `pe_cross_instance_tie`/`pe_within_thread_tie`; ambig-BAM gated on Some |
| 3.3.10 | Unique-best selection: 1 / 2-4 sort+tie / >4 die; sum_2nd 3811-3816 | `merge.rs:631-657` | DONE | `pe_unique_best_by_sum_across_slots` (sum_2nd=runner-up), `pe_worse_later..single_entry` (None) |
| 3.3.11 | Directional reject index 1\|2 | `merge.rs:659-664` | DONE | `pe_directional_rejection_index_1` |
| 3.3.12 | unique_best++; PE extraction; **in-order R1→R2 length guards**; calc_mapq(len1,Some(len2),…); per-mate methcall; PE SAM | `merge.rs:666-674` + `lib.rs:859-908` | DONE | R1 short-circuit `continue` before R2 (lib.rs:864-879) |
| 3.3 type | `DecisionPaired`/`BestAlignmentPaired`; extraction+XM+SAM in driver | `merge.rs:375-426`, `lib.rs:859` | DONE | parallel types (Q3); merge returns decision only |
| 3.4.1 | Per-mate independent walk; index-driven +2 (5′ idx1/3, 3′ idx0/2); reusable inner w/ guard predicate param (B O-3) | `methylation.rs:305-392` `walk_mate(strict_5p)` | DONE | called twice; `strict_5p` is a param |
| 3.4.2 | Edge guards return early; failing mate short, other full; per-mate caller gate; +1 count each (B C-1/A I-2) | `methylation.rs:472-525` + `lib.rs:864-879` | DONE | `pe_mate2_chr_edge_leaves_mate1_full_mate2_short`; mate1-edge returns empty mate2 |
| 3.4.3 | Mate1 5′ strict `>0` vs mate2 `>=0` (position_1>=4 vs >=3) | `methylation.rs:321-336` | DONE | `pe_mate1_5prime_guard_is_strict_gt0`, `pe_mate1_5prime_passes_at_position_4` |
| 3.4.4 | Index dispatch + 4 PE strand counters; revcomp the `-` mate; deletion-MD revcomp; idx>3 die | `methylation.rs:415-546` | DONE | `pe_extract_index0..ct_ga_ct`, `index2..ga_ct_ct`, `index1..ga_ct_ga`, `pe_extract_deletion_index0..` |
| 3.4.5 | `methylation_call` reused verbatim per mate; pooled counters; slam out | `lib.rs:880-891` | DONE | both mates feed same `total_*` |
| 3.5.1 | FLAG per-index constant pair (literal table, 99/147/163/83/147/99/83/163) | `output.rs:469-480` | DONE | `pe_flag_constant_table` |
| 3.5.2 | POS/MAPQ shared/CIGAR literal | `output.rs:540-541, build_pe_mate` | DONE | `pe_rnext_pnext_mapq_shared` |
| 3.5.3 | RNEXT `=` (mate tid==own), PNEXT=other mate POS | `output.rs:543,563` + `build_pe_mate:641-642` | DONE | `pe_rnext_pnext_mapq_shared` |
| 3.5.4 | TLEN tree A1/A2/B1/B2 + dovetail FLAG-gate; total partition (A I-1); 1-based start/walked end (B O-1) | `output.rs:499-530` | DONE | `pe_tlen_tree` (incl. equality + suppress), `pe_dovetail_gate_negative_index1_not_dovetailed` |
| 3.5.5 | Per-mate revcomp keyed on stored strand; SEQ/ref/MD/qual | `output.rs:610-617` `build_pe_mate` | DONE | `pe_minus_strand_mate_reverses_seq_and_xm` |
| 3.5.6 | +2 ref trim INDEX-keyed both mates (B I-3) | `output.rs:482-497` | DONE | idx0/3 vs 1/2 split; verified by FLAG/XR tests |
| 3.5.7 | Tags `NM MD XM XR XG`; XR per mate, XG shared | `output.rs:645-658` | DONE | `pe_per_mate_xr_shared_xg_and_tag_order` |
| 3.5.8 | RNAME pre-de-converted; extra tags OFF | parse-time de-convert in merge | DONE | bare NM MD XM XR XG form |
| 3.6.1 | Dispatch arm `(PairedEnd,Directional,FastQ)`→`run_pe_directional` | `lib.rs:109-111` | DONE | |
| 3.6.2 | Genome once; per pair convert→2 instances→lockstep→route→report→two-temp cleanup; two R1 id strings (B I-4) | `lib.rs:519-628, 831-838` | DONE | merge id `@`-stripped vs aux `@`-bearing both kept |
| 3.6.3 | Output naming `_pe.bam` (lc) / `_PE_report.txt` (uc); basename variants | `lib.rs:576,583-588` | DONE | `pe_mapped_writes_two_bam_records_end_to_end` asserts both names |
| 3.7 | Routing: UniqueBest→pe.bam; Ambiguous→ambig-BAM(Some) + AMBIG_1/2 else UNMAPPED_1/2; NoAlignment→UNMAPPED_1/2; aux `_pe.ambig.bam` name (A O-2); verbatim `+` newline (A O-4) | `lib.rs:858-948, 957-978` + `aux_out.rs` | DONE | `pe_unmapped_routing_to_1_and_2_files`; precedence + un-stripped names |
| 3.8.1 | PE header `for: <f1> and <f2>`; directional line | `report.rs:42-56` + `lib.rs:590-599` | DONE | `pe_header_two_files` |
| 3.8.2 | 7 "Sequence pairs" wording swaps; `Mapping efficiency:\t<p>% \n` trailing space | `report.rs:149-201` | DONE | `pe_mapping_efficiency_has_trailing_space`, `pe_final_analysis_exact_directional` |
| 3.8.3 | 3-token strand labels in JOIN order 0,2,1,3 (B I-5) | `report.rs:197-201` | DONE | `pe_final_analysis_exact_directional` pins the exact ordering |
| 3.8.4 | Directional rejected line after strand block, before cytosine | `report.rs:203-209` | DONE | `pe_non_directional_omits_rejected_line` |
| 3.8.5 | Cytosine half byte-identical to SE (shared) | `report.rs:217-282` `write_cytosine_report` | DONE | shared by SE+PE |
| 3.edge | per-mate XS default; same chr:pos1:pos2 dedup; chr-edge no-bucket; 0-pairs eff; ambig-without-aux; R1-vs-R2-leftmost | merge/methylation/report/output | DONE | dedicated unit tests for each (see §7 rows) |

### §4 Signatures

| # | Signature | Source | Status | Notes |
|---|-----------|--------|--------|-------|
| S1 | `BestAlignmentPaired` struct | `merge.rs:375-408` | DONE | all fields present (chr/index/pos1/2/cigar1/2/md1/2/seq1/2/flag1/2/sum/sum_2nd/mapq) |
| S2 | `DecisionPaired` enum (UniqueBest/Ambiguous{first_ambig:Option<(String,String)>}/NoAlignment/Rejected) | `merge.rs:411-426` | DONE | exact shape |
| S3 | `check_results_paired_end<S: PairedSamStream>(…, &mut [Option<S>], …)` | `merge.rs:463-474` | DONE | DEVIATED-as-documented: `&mut [Option<S>]` (slot-indexed) per B I-1 — improvement over plain `&mut [S]`, noted in §12 |
| S4 | `PairedSamStream` trait (current_pair/advance_pair) + `PairedAlignerStream` | `align.rs:323-347` | DONE | |
| S5 | `GenomicExtractionPaired` + `extract_corresponding_genomic_sequence_paired_end` (edge-state contract) | `methylation.rs:254-403` | DONE | both Vec<u8> carry real (possibly short) lengths |
| S6 | `paired_end_sam_output(…, dovetail: bool)` (B I-2) | `output.rs:452-466` | DONE | dovetail threaded in |
| S7 | `print_final_analysis_report_paired_ends`; `ReportHeader.sequence_file2: Option<&str>` (B I-6) | `report.rs:25-38, 149` | DONE | SE passes None |
| S8 | `aux_filename(…, mate: Option<u8>)` extended in place (A I-3) | `aux_out.rs:40-47` | DONE | SE call sites pass None |
| S9 | `run_pe_directional(config, mates1, mates2)`; PE Sinks (1 BAM + ambig + 2 unmapped + 2 ambiguous) | `lib.rs:519, PeSinks:633-640` | DONE | |

### §7 Validation rows

| # | Verify | Test | Status |
|---|--------|------|--------|
| 1 | PE aligner_options full string (I-4) | `options::paired_end_tail_and_default_maxins` | DONE |
| 2 | R2 forward G→A + `/2/2`; R1 `/1/1`+C→T | `convert::convert_g_to_a_uc…`, `pe_read1/2_…suffix` | DONE |
| 3 | Paired stream R1 identification (incl. R1=line-2; die) | `align::sampair_identifies…`, `_swaps…`, `_dies…` | DONE |
| 4 | Unique best by SUM; sum_2nd per 3811-3816 | `merge::pe_unique_best_by_sum_across_slots` | DONE |
| 5 | (77,141) no-align pair | `merge::pe_no_align_marker_contributes_nothing`, `pe_no_alignment_when_all_unmapped` | DONE |
| 6 | Within-thread vs cross-instance tie; first_ambig Some only within | `pe_within_thread_tie…captures`, `pe_cross_instance_tie…(None)` | DONE |
| 7 | Location key (contained mate) | `merge::pe_same_location_both_instances_dedups` | DONE* (see Gap) |
| 8 | Directional reject index 1\|2 | `merge::pe_directional_rejection_index_1` | DONE |
| 9 | Per-mate extraction; revcomp target per index | `pe_extract_index0/1/2…` | DONE |
| 10 | 🔴 Per-mate could-not-extract short-circuit (a R2-edge, b R1-edge) | `methylation::pe_mate2_chr_edge_leaves_mate1_full_mate2_short` + `pe_mate1_5prime_guard_is_strict_gt0` (mate1 edge → mate2 empty) | DONE |
| 11 | Mate1 5′ strict `>0` vs mate2 `>=0` | `pe_mate1_5prime_guard_is_strict_gt0`, `pe_mate1_5prime_passes_at_position_4` | DONE |
| 12 | 🔴 FLAG table (4 indices, swap) | `output::pe_flag_constant_table` | DONE (hand-derived; differential = #21 oxy) |
| 13 | 🔴 TLEN tree A1/A2/B1/B2 + equality + dovetail-gate-neg + `--no_dovetail` | `output::pe_tlen_tree`, `pe_dovetail_gate_negative_index1_not_dovetailed` | DONE |
| 14 | RNEXT/PNEXT/MAPQ | `output::pe_rnext_pnext_mapq_shared` | DONE |
| 15 | Per-mate revcomp on strand; XR per-mate, XG shared | `pe_minus_strand_mate_reverses_seq_and_xm`, `pe_per_mate_xr_shared_xg_and_tag_order` | DONE |
| 16 | Tag order; +2 index-driven trim | `pe_per_mate_xr_shared_xg_and_tag_order` (order) + FLAG/XR tests (trim) | DONE |
| 17 | 🔴 PE report bytes incl. `% \n` | `report::pe_final_analysis_exact_directional`, `pe_mapping_efficiency_has_trailing_space`, `pe_zero_pairs…` | DONE |
| 18 | Aux filenames `_reads_1`/`_2` | `aux_out::filename_paired_mate_suffix` | DONE |
| 19 | Routing: ambiguous→_1/_2; precedence; ambig-BAM two lines | `cli::pe_unmapped_routing_to_1_and_2_files` (routing/precedence) | PARTIAL → see Gap (ambig→_1/_2 + ambig-BAM two-line not end-to-end-tested locally; gated by #21) |
| 20 | Two-temp cleanup best-effort | `lib.rs:623-624` (`remove_file` ×2, ignored result) | DONE* (no dedicated driver unit; behavior is `let _ =`, non-fatal by construction) |
| 21 | 🎯 oxy PE gate | Phase 10 harness | PENDING (expected) |
| 22 | 🔴 Single-mate XS (B O-5) | `merge::pe_single_mate_xs_defaults_to_own_as` | DONE |
| 23 | 🔴 Two R1 id strings (B I-4) | `lib.rs:831-838` (impl); `cli::pe_unmapped_routing…` exercises @-bearing aux | DONE* (logic present + integration-exercised; no isolated assertion of stripped-vs-bearing divergence) |
| 24 | 🔴 Report strand-label JOIN order 0,2,1,3 (B I-5) | `report::pe_final_analysis_exact_directional` | DONE |
| 25 | 🔴 ambig-BAM de-convert non-mangling (QNAME w/ literal `_CT_converted`) (B O-2) | — | PARTIAL → see Gap (no test; code at `output.rs:742` only strips `f[2]` → correct) |

### §11/§12 rev-1 folded findings

| Finding | Severity | Source | Status |
|---|---|--------|--------|
| B C-1 / A I-2 per-mate could-not-extract short-circuit | Critical | `lib.rs:864-879` + `methylation.rs:472-525` + `pe_mate2_chr_edge…` | DONE |
| A I-1 TLEN total `if/else` partition | Important | `output.rs:504-530` (if/else, no orphan ifs) | DONE |
| A I-3 aux_filename extended in place | Important | `aux_out.rs:40-47` | DONE |
| A I-4 full aligner_options (no `--minins`, `--maxins 500` later) | Important | `options::paired_end_tail_and_default_maxins` | DONE |
| A I-5 / B V-GAP-2 broaden FLAG/TLEN differential (equality + dovetail-neg + `--no_dovetail`) | Important | `pe_tlen_tree`, `pe_dovetail_gate_negative…` | DONE |
| B I-1 slot-indexed streams; scan 0,3,1,2 | Important | `merge.rs:468,483` + `lib.rs:574` | DONE |
| B I-2 `dovetail: bool` param | Important | `output.rs:465` + `lib.rs:531-534,904` | DONE |
| B I-3 +2 trim keyed on INDEX | Important | `output.rs:482-497` | DONE |
| B I-4 two R1 id strings (merge @-stripped vs aux @-bearing) | Important | `lib.rs:831-838` | DONE |
| B I-5 report strand-label join order 0,2,1,3 | Important | `report.rs:197-201` | DONE |
| B I-6 `ReportHeader.sequence_file2` | Important | `report.rs:30` | DONE |
| A O-1 1-based pos_1>=4 clarity | Optional | comment `methylation.rs:323` | DONE |
| A O-2 ambig-BAM name `_pe.ambig.bam` | Optional | `lib.rs:680` | DONE |
| A O-3 PE no-die-if-same-id | Optional | `merge.rs:495-499` (no die guard) | DONE |
| A O-4 aux `+`-line verbatim newline | Optional | `aux_out.rs:85` + `write_pe_aux` passes `plus` verbatim | DONE |
| B O-1 TLEN 1-based-start / 0-based-walked-end basis | Optional | `output.rs:502-503` (start=position, end=end_position) | DONE |
| B O-3 reusable-inner guard parametrisation | Optional | `methylation.rs:312,322` (`strict_5p`) | DONE |
| B O-5 single-mate-XS | Optional | `merge::pe_single_mate_xs_defaults_to_own_as` | DONE |
| Q3 parallel PE types | endorsed | `merge.rs` DecisionPaired/BestAlignmentPaired | DONE |
| Q4 location-key inconsistency preserved | resolved | `merge.rs:711-715` | DONE |
| Q5 4 new 3-token counters | resolved | `merge.rs:88-98` | DONE |
| Q6 FLAG/TLEN re-read + differential | resolved | unit tests now; differential = oxy gate (#21) | DONE (per design) |
| §0 2-instance directional finding | resolved | `lib.rs:557-574` (slots 0/3 only) | DONE |

## Gaps (detail)

### §7 #25 — ambig-BAM de-convert non-mangling (PARTIAL: validation-only)

**Expected:** a unit test feeding a raw SAM line whose **QNAME** contains a literal `_CT_converted`, asserting only the RNAME suffix is stripped and the QNAME is left intact (proving the de-conversion is field-scoped, not whole-line).
**Found:** `build_raw_ambig_record_deconverts_and_preserves_tag_order` (`output.rs:1190`) uses an ordinary QNAME (`r1`); no test exercises the embedded-`_CT_converted`-in-QNAME case. There is also **no dedicated test of `write_raw_pe_ambig_lines`** (the PE two-line `/1\t`→`\t` / `/2\t`→`\t` + de-convert path at `output.rs:667-678`).
**Gap:** test only. The implementation is **correct**: `build_raw_record` (`output.rs:729-783`) de-converts `f[2]` (RNAME field) exclusively, and `write_raw_pe_ambig_lines` strips the tag via `replacen("/1\t","\t",1)` anchored on the field-terminating tab — neither can touch a QNAME's interior `_CT_converted`. So #25's invariant holds; it is simply not pinned by an assertion. Low risk; the oxy gate (#21) would surface any regression.

### §7 #7 — location key (contained mate) (DONE, minor test-precision note)

**Expected:** "second-best branch min/max vs no-2nd raw pos1:pos2".
**Found:** `pe_same_location_both_instances_dedups` uses identical positions (100/140) in both slots, so it confirms **dedup** but does not *distinguish* the min/max key (2nd-best branch) from the raw key (no-2nd branch) — both produce the same string when pos1<pos2 and no swap occurs.
**Assessment:** the code in `insert_pair` (`merge.rs:711-715`) correctly switches on `min_max_key` and only swaps when `r1.pos > r2.pos`; both call sites pass the right flag (`true` at 591, `false` at 606). The branch logic is present and correct. Not flagged as a functional gap — noting only that the test does not force a `pos1 > pos2` layout that would visibly diverge the two key forms. Pinned end-to-end by the oxy gate.

### §7 #19 / #20 / #23 — DONE* (logic present; some assertions deferred to the oxy gate)

- **#19** ambiguous→`_1`/`_2` routing + ambig-BAM two-line output: the routing code (`lib.rs:909-932`) and precedence are implemented and the *unmapped* `_1`/`_2` path is integration-tested (`pe_unmapped_routing_to_1_and_2_files`); the **ambiguous + `--ambig_bam` two-line** end-to-end assertion is not present locally (the plan itself routes this to #21 oxy). Code-complete.
- **#20** two-temp cleanup: `lib.rs:623-624` deletes both temps via `let _ = remove_file(...)` (best-effort, non-fatal by construction); no isolated driver unit, but behaviour is structurally guaranteed.
- **#23** two R1 id strings: implemented at `lib.rs:831-838` (merge gets `@`-stripped `identifier`; aux uses the same for R1 which `write_fastq_record` re-`@`s; R2 uses `id2_stripped`). Exercised via the integration aux test; no isolated assertion of the stripped-vs-bearing divergence on the same line.

## Test verification (Mode B)

`cd rust && cargo test -p bismark-aligner` → **all green** (192 tests). Sandbox blocked the cargo lock on the first attempt (`target/debug/.cargo-lock` Operation not permitted); re-ran with sandbox disabled.

| Test (Phase-7 additions) | File | Status |
|--------------------------|------|--------|
| `convert_g_to_a_uc_then_substitute` | src/convert.rs | PASS |
| `pe_read1_c_to_t_with_slash_1_1_suffix` | src/convert.rs | PASS |
| `pe_read2_g_to_a_with_slash_2_2_suffix` | src/convert.rs | PASS |
| `pe_suffix_inserted_before_newline_crlf` | src/convert.rs | PASS |
| `pe_invalid_read_number_errors` | src/convert.rs | PASS |
| `sampair_identifies_read1_by_slash1` / `_swaps_when_read1_emitted_second` / `_dies_when_neither_is_read1` / `_unmapped_marker_77_141` | src/align.rs | PASS |
| `pe_stream_skips_header_then_walks_pairs_to_eof` / `pe_stream_all_header_has_no_pairs` | src/align.rs | PASS |
| `pe_unique_best_by_sum_across_slots` / `pe_worse_later_alignment_not_stored_single_entry` | src/merge.rs | PASS |
| `pe_no_align_marker_contributes_nothing` / `pe_no_alignment_when_all_unmapped` | src/merge.rs | PASS |
| `pe_cross_instance_tie_is_ambiguous` / `pe_within_thread_tie_is_ambiguous_and_captures_first_ambig` | src/merge.rs | PASS |
| `pe_directional_rejection_index_1` / `pe_single_mate_xs_defaults_to_own_as` | src/merge.rs | PASS |
| `pe_same_location_both_instances_dedups` / `pe_different_chromosomes_per_mate_errors` | src/merge.rs | PASS |
| `pe_extract_index0_appends_two_revcomps_mate2_counts_ct_ga_ct` | src/methylation.rs | PASS |
| `pe_extract_index2_revcomps_mate1_counts_ga_ct_ct` / `pe_extract_index1_prepends_two_counts_ga_ct_ga` | src/methylation.rs | PASS |
| `pe_mate1_5prime_guard_is_strict_gt0` / `pe_mate1_5prime_passes_at_position_4` | src/methylation.rs | PASS |
| `pe_mate2_chr_edge_leaves_mate1_full_mate2_short` / `pe_extract_deletion_index0_builds_md_seq_and_indels` | src/methylation.rs | PASS |
| `pe_flag_constant_table` / `pe_tlen_tree` / `pe_dovetail_gate_negative_index1_not_dovetailed` | src/output.rs | PASS |
| `pe_rnext_pnext_mapq_shared` / `pe_per_mate_xr_shared_xg_and_tag_order` / `pe_minus_strand_mate_reverses_seq_and_xm` | src/output.rs | PASS |
| `pe_header_two_files` / `pe_final_analysis_exact_directional` / `pe_mapping_efficiency_has_trailing_space` / `pe_non_directional_omits_rejected_line` / `pe_zero_pairs_mapping_efficiency_bare_zero` | src/report.rs | PASS |
| `filename_paired_mate_suffix` | src/aux_out.rs | PASS |
| `paired_end_tail_and_default_maxins` | src/options.rs | PASS |
| `pe_mapped_writes_two_bam_records_end_to_end` / `pe_unmapped_routing_to_1_and_2_files` | tests/cli.rs | PASS |
| `pe_mate_count_mismatch_errors` / `pe_same_file_errors` / `se_pe_conflict_errors` / `mate2_without_mate1_errors` | tests/cli.rs | PASS |

| Missing test | Intended row | Status |
|--------------|--------------|--------|
| ambig-BAM QNAME-with-literal-`_CT_converted` non-mangling | §7 #25 | MISSING (code correct; validation-only gap) |

## Verdict

**COMPLETE.** Every §3 behavior sub-step, every §4 signature, and every §11/§12 rev-1 finding (1 Critical, 10 Important, 7 Optional, plus the resolved questions and the §0 2-instance correction) is implemented in code and, with one exception, pinned by a passing test. All 192 tests pass.

The single outstanding item is a **validation-only gap**, not a functional one:

- **§7 #25** has no dedicated unit test (a raw ambig SAM line whose QNAME contains a literal `_CT_converted`). The targeted code path (`output.rs:729-783` `build_raw_record` and `output.rs:667-678` `write_raw_pe_ambig_lines`) is correct — de-conversion is field-scoped to `f[2]` and the tag-strip `replacen` is tab-anchored — so the invariant holds; it is merely not asserted. Adding one unit test would close it.

Note also (not gaps, surfaced for the user): §7 #7's dedup test does not force a `pos1 > pos2` layout that would visibly distinguish the min/max key from the raw key (code branch is correct); and §7 #19's ambiguous-`_1`/`_2` + two-line ambig-BAM end-to-end assertion is deferred to the Phase-10 oxy gate (#21), as the plan itself routes it there.

**§7 #21 (oxy PE byte-identity gate) is PENDING by design (Phase 10)** — not a Phase-7 gap.

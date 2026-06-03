# Plan Coverage Report

**Mode:** B (code vs. plan — the plan is the spec)
**Plan(s):** `phase8-nondirectional-pbat/PLAN.md` (rev 1)
**Code:** `rust/bismark-aligner/src/{convert,lib,methylation,output}.rs` + `tests/cli.rs` (branch `rust/aligner`)
**Date:** 2026-06-02
**Verdict:** COMPLETE

## Summary

- Total items: 7 (audit ledger) + 9 (validation rows §7)
- DONE: 7/7 ledger; §7 rows 1–8 DONE, row 9 deferred (separate oxy step, by design)
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 0 material (one documented non-material conversion entry-point shape; one byte-invisible STDERR banner change — neither byte-gated)

Build/tests: `cargo test -p bismark-aligner` → **183 lib + 28 integration = 211** green (matches §12). Diff vs Phase-7 `ca6af0a` touches ONLY the five named files (`report.rs` unchanged — confirms the "no new report code" thesis).

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | convert.rs: SE G→A entry; library-aware PE per-mate (pbat R1 G→A `/1/1` + R2 C→T `/2/2`; non-dir both-per-mate); NOT a silent reuse of directional | §3.1, §4, rev1 B I-1 | DONE | `bisulfite_convert_fastq_se_ga` (convert.rs:178). PE selector is the **explicit** `bisulfite_convert_fastq_pe_kind(.., kind)` (237) taking the `ConvKind` per (mode, mate); directional `bisulfite_convert_fastq_pe` (211) **delegates** with the fixed R1=Ct/R2=Ga map — no silent reuse. `pe_id_suffix` (197) is per-mate regardless of mode; `file_base_for` (187) follows the kind. |
| 2 | lib.rs SE: `run_se_directional`→`run_se` with §3.2 per-mode instance plan; directional byte-frozen; pbat passes `pbat=true` | §3.2, §5 step 2 | DONE | `run_se` (lib.rs:189) + `se_instance_plan(library)` (155) returns slots in Bismark order: directional `[(Norc,Ct,0),(Nofw,Ga,0)]`; pbat `[(Nofw,Ct,0),(Norc,Ga,0)]`; non-dir 4 slots `[(Norc,Ct,0),(Nofw,Ga,0),(Nofw,Ct,1),(Norc,Ga,1)]` — matches §3.2 table cell-for-cell (orient/index/file-idx). `pbat = matches!(.., Pbat)` (201) passed into `drive_merge` (271). |
| 3 | lib.rs PE: `run_pe_directional`→`run_pe` with §3.3 slot plan; directional byte-frozen | §3.3, §5 step 3 | DONE | `run_pe` (639) + `pe_instance_plan(library)` (589): directional slots {0,3}; pbat {1,2} (GA_1/CT_2); non-dir all 4 — matches §3.3 table (slot/orient/index/k1/k2). Streams placed into `vec![None;4]` at their slot (700). PE extraction keys on raw index (no `+2` — confirmed in `extract_corresponding_genomic_sequence_paired_end`). Conversions deduped (one file per distinct (mate,kind): 2 dir/pbat, 4 non-dir) via `needed`/`pe_lookup` (662–699). |
| 4 | Dispatch: 2 new arms `(SE/PE, NonDirectional\|Pbat, FastQ)`; deferred msg shrunk to FastA/threading | §3.4, §5 step 4 | DONE | `pipeline` (105) now matches `(layout, format)` ONLY → SE/PE FastQ for all 3 libraries fold into `run_se`/`run_pe`; the `_ =>` message is now "FastA input and multicore/threading… FastQ single-end/paired-end, all library types" (114–118) — matches §3.4. |
| 5 | Per-mode temp cleanup as an EXPLICIT task: SE pbat=1, non-dir=2; PE pbat=2, non-dir=4 — cleanup loops AND test "temp gone" assertions | §3.5, §5 step 5, rev1 A | DONE | SE: `for cr in &converted { remove_file }` (lib.rs:290–292) deletes every converted temp (1 dir/pbat, 2 non-dir), best-effort. PE: `for ((_,_),cr) in &converted { remove_file }` (760–762) deletes 2 (dir/pbat) / 4 (non-dir). Test "gone" assertions: `pbat_se_*` (G→A gone), `nondir_se_four_instances` (C→T + G→A gone), `pbat_pe_ga_index` (G→A_1 + C→T_2 gone), `nondir_pe_four_slots` (all 4 gone). |
| 6 | Tests (§5 step 6, §7 1–7): conversion units; SE non-dir 4-inst + pbat eff2/3; GA `methylation_call` byte XM; CTOT/CTOB FLAG/XR/XG byte; PE non-dir/pbat slots; no-rejection. 🔴 fakes emit mapped hits on `*BS_GA*`/G→A reads + byte-assert SAM/XM | §5 step 6, §7 #2 | DONE | See test-verification table. The GA-emitting fakes (`make_fake_bowtie2_ga_reads_{ct,ga}_index`, `..._pe_ga_index`, `..._pe_ct_index_ga_reads`) map ONLY the `*_G_to_A*` reads on the chosen index; every Phase-8 integration test asserts `unique best alignments: 1` + ≥1 written record (cannot false-pass on all-unmapped) and byte-asserts FLAG/POS/SEQ/XR/XG/XM. |
| 7 | Validation row 8: directional SE+PE unchanged through the generalization (regression guard) | §7 #8, rev1 A | DONE | All pre-existing SE+PE directional unit + integration tests (`happy_path…`, `mapped_read_writes_bam_record…`, `pe_mapped_writes_two_bam_records…`, `pe_unmapped_routing…`, etc.) stayed green through the `run_se`/`run_pe` generalization. directional `se_instance_plan`/`pe_instance_plan` slots reproduce the prior hardcoded layout (s0/s1 SE; s0/s3 PE); `report.rs` has zero diff. |

### Validation rows (§7)

| Row | Verify | Status | Evidence |
|---|---|---|---|
| 1 | SE G→A / non-dir / pbat-PE / non-dir-PE conversion bytes + filenames | DONE | convert.rs tests: `se_ga_entry_point_g_to_a_no_suffix`, `pe_pbat_r1_is_g_to_a_with_slash_1_1`, `pe_pbat_r2_is_c_to_t_with_slash_2_2`, `pe_nondir_makes_both_kinds_per_mate`. |
| 2 | SE non-dir 4 instances at slots 0–3; GA-branch SAM/XM byte | DONE | `nondir_se_four_instances_ctot_no_rejection` + `nondir_se_ga_index_ctob_record` (GA fakes; byte FLAG/XR/XG). |
| 3 | SE pbat eff 2/3 → CTOT/CTOB; counts ga_ct/ga_ga; FLAG/XR/XG | DONE | `extract_pbat_se_index0_eff2_ga_ct` + `..._index1_eff3_ga_ga` (counters), `sam_output_ctot_eff2…`/`…ctob_eff3…` (FLAG/SEQ/XM/XR/XG), `pbat_se_ct_index_writes_ctot_record`/`pbat_se_ga_index_writes_ctob_record` (e2e). |
| 4 | PE non-dir: 4 slots populated; all kept | DONE | `nondir_pe_four_slots_index1_no_rejection` (FLAG 163/83, `directional-rejected: 0`). |
| 5 | PE pbat slots 1,2 (GA_1/CT_2); no modifier | DONE | `pbat_pe_ga_index_writes_ctob_pair` (idx 1 → 163/83) + `pbat_pe_ct_index_writes_ctot_pair` (idx 2 → 147/99). |
| 6 | non-dir: NO wrong-strand rejection | DONE | `nondir_se_four_instances…` + `nondir_pe_four_slots…` assert a record IS written and `directional-rejected: 0`. |
| 7 | Report per mode: library line; rejected line OMITTED for non-dir/pbat | DONE | report.rs (unchanged): `library_line` 3-way incl. pbat SE/PE strings (83–98); existing byte-test `non_directional_omits_rejected_line`; `print_final_analysis_report_{single,paired}` gate the rejected line on `directional` (run passes `directional=false` for non-dir/pbat). |
| 8 | Directional SE+PE byte-frozen | DONE | (ledger #7) — full directional suite green. |
| 9 | oxy real-data gate (4 mode×layout cells) | DEFERRED | Per task instructions: a SEPARATE post-review step, NOT part of this code audit. Not a gap. |

## Gaps (detail)

None. No PARTIAL, MISSING, or DEVIATED items.

## Deviations (documented, non-material — confirmed against code)

1. **PE conversion entry-point shape** — §4 floated either an explicit per-mate fn OR a free `pe_conv_kinds(library, read_number) -> &[ConvKind]`. The code realises it as `bisulfite_convert_fastq_pe_kind(.., kind)` with the directional fn delegating, and the driver (`run_pe`) deduping conversions itself via `needed`/`pe_lookup`. Same intent (rev1 B I-1: no silent reuse of the directional read#→kind hardcoding) — verified: the directional map lives ONLY in `bisulfite_convert_fastq_pe` (218), and pbat/non-dir feed explicit `ConvKind`s from `pe_instance_plan`. Acceptable.
2. **STDERR conversion banner** — now one line per converted file (was one combined PE line). Byte-invisible (not gated; no test asserts the combined form); the directional SE "Created C->T converted" substring is preserved (the existing `happy_path…` test relies on it — verified still present at lib.rs:213). Acceptable.

## Test verification (Mode B)

| Test | File | Status |
|---|---|---|
| se_ga_entry_point_g_to_a_no_suffix | src/convert.rs | PASS |
| pe_pbat_r1_is_g_to_a_with_slash_1_1 | src/convert.rs | PASS |
| pe_pbat_r2_is_c_to_t_with_slash_2_2 | src/convert.rs | PASS |
| pe_nondir_makes_both_kinds_per_mate | src/convert.rs | PASS |
| extract_pbat_se_index0_eff2_ga_ct | src/methylation.rs | PASS |
| extract_pbat_se_index1_eff3_ga_ga | src/methylation.rs | PASS |
| methylation_call_ga_branch_contexts | src/methylation.rs | PASS |
| methylation_call_ga_branch_converted_g_to_a_unmethylated | src/methylation.rs | PASS |
| sam_output_ctob_eff3_plus_ga_ga_flag16 | src/output.rs | PASS |
| sam_output_ctot_eff2_minus_ga_ct_flag0 | src/output.rs | PASS |
| pe_per_mate_xr_xg_index_1_and_2 | src/output.rs | PASS |
| pbat_se_ct_index_writes_ctot_record | tests/cli.rs | PASS |
| pbat_se_ga_index_writes_ctob_record | tests/cli.rs | PASS |
| nondir_se_four_instances_ctot_no_rejection | tests/cli.rs | PASS |
| nondir_se_ga_index_ctob_record | tests/cli.rs | PASS |
| pbat_pe_ga_index_writes_ctob_pair | tests/cli.rs | PASS |
| pbat_pe_ct_index_writes_ctot_pair | tests/cli.rs | PASS |
| nondir_pe_four_slots_index1_no_rejection | tests/cli.rs | PASS |
| (directional regression: happy_path / mapped_read… / pe_mapped… / pe_unmapped… / pbat_genome_as_positional…) | tests/cli.rs | PASS |
| **Full suite** | lib + integration | **183 + 28 = 211 PASS** |

### 🔴 False-pass-on-all-unmapped check (the load-bearing reviewer trap)

CLEARED. The four new GA-emitting fakes gate the hit on BOTH the index (`*BS_GA*` / `*BS_CT*`) AND the reads file being the G→A-converted one (`case "$inp" in *_G_to_A*) hit=1`). Each test asserts `unique best alignments:   1` in stderr AND reads the BAM back to assert ≥1 record with byte-exact FLAG/SEQ/XR/XG/XM — so a silent all-unmapped run (0 records, header-only BAM) would fail the record-count and stderr assertions. The first-live paths are genuinely exercised: the GA `methylation_call` branch (XM byte-asserted in `methylation_call_ga_branch_contexts` + the CTOT/CTOB output tests), the SE eff-2/eff-3 CTOT/CTOB FLAG arms (FLAG 0 / 16 byte-asserted), and the PE index-1/2 records (FLAG 163/83, 147/99 + per-mate XR/XG byte-asserted).

## Verdict

**COMPLETE.** Every ledger item (1–7) is DONE with code + tests that assert real behavior; validation rows §7 1–8 are DONE and row 9 (oxy gate) is correctly deferred to the separate post-review step. The load-bearing risk (fakes that would false-pass on all-unmapped) is closed: the new fakes map the G→A reads on the chosen index and every test byte-asserts the resulting SAM/XM on the first-live CTOT/CTOB/PE-index-1/2 paths. Directional SE+PE stayed byte-frozen through the `run_se`/`run_pe` generalization (`report.rs` zero-diff; full prior suite green). The only deviations are documented and byte-invisible. No gaps require action before the oxy gate.

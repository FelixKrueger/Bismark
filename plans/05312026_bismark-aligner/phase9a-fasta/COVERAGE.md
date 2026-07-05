# Plan Coverage Report

**Mode:** B (code vs. plan — the PLAN is the spec)
**Plan:** `plans/05312026_bismark-aligner/phase9a-fasta/PLAN.md` (rev 1)
**Code:** `rust/bismark-aligner` @ `rust/aligner` `7f7d77d` + uncommitted working-tree edits (`convert.rs`, `lib.rs`, `aux_out.rs`, `tests/cli.rs`)
**Date:** 2026-06-02
**Verdict:** COMPLETE

## Summary

- Total ledger items: 7
- DONE: 6
- DEVIATED (documented, non-material): 1 (item 7 — the two deviations themselves; both documented + verified non-material)
- PARTIAL: 0
- MISSING: 0
- Validation rows §9 #1–9: all covered by tests; #10 (oxy gate) DEFERRED (separate post-audit step, per instruction).

Tests: `cargo test -p bismark-aligner` → **226 pass / 0 fail** (194 lib + 32 integration + 0 doc), confirmed by run.

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | `convert.rs` FastA 2-line core + SE C→T/G→A + PE lib-aware; `.fa` suffix; `>` prefix; per-record `^>` sanity; no max-len; PE gzip-off | §3.1, §5.1 | DONE | `convert_fasta_impl` (430–536) + `bisulfite_convert_fasta_se`/`_se_ga`/`_pe_kind`. Per-record sanity at 516 (NOT record-1-guarded). No max-length guard present. PE forces `gzip:false` (410–430). `.fa`/`.fa.gz` ext (456). 2-line write, no `+`/qual (524–525). |
| 2 | `-f` flag + `-f⊕--phred` die — VERIFY-ONLY (exist + tested) | §3.2, §5.2 | DONE | `options.rs:27` pushes `-f` FIRST; `require_fastq` (183) gates `--phred33`/`--phred64` (38/42). Tests `fasta_uses_dash_f` (292, asserts `-f` token+position) + `phred_without_fastq_errors` (311). NOT re-implemented. |
| 3 | `lib.rs`: format-branched conversion + dispatch arms folded into `run_se`/`run_pe` (`_=>` deferral gone); 2-line re-read + `'I'×len` QUAL in `drive_merge`/`drive_merge_pe` | §3.3, §5.3 | DONE | `pipeline()` now matches layout only — FastA no longer deferred (only `--multicore` via `deferred_flags`). `convert_se_ct`/`_se_ga`/`convert_pe_kind` dispatch on `fasta`; `convert_se_files` + PE conversion branch. `drive_merge` 2-line read + `qual = vec![b'I'; seq_uc.len()]` (Phred 40); `drive_merge_pe` per-mate `'I'×len`; `>`-strip on id. |
| 4 | `aux_out.rs`: 2-line FastA `>id\nseq` writer; `.fa.gz` filename | §3.4, §5.4 | DONE | `write_fasta_record` (aux_out.rs, `>`+id+`\n`+seq+`\n`, no qual). `aux_filename(..., fasta=true,...)` flips `.fq`→`.fa` (test `filename_fasta_extension`). Routed via `write_se_aux_record` / `write_pe_aux(fasta)` in lib.rs. |
| 5 | `strip_fastq_suffix` NOT extended for `.fa` | §5.3 / B I-5 | DONE | `lib.rs:452–459` lists only `.fastq.gz/.fq.gz/.fastq/.fq`. `.fa` falls through unchanged (Perl 1622). Integration test asserts output name keeps `.fa` (`reads.fa_bismark_bt2.bam`). |
| 6 | Tests §9 #1–9: conversion bytes/names; FastA-aware fakes (`NR%2`/`^>`) byte-asserting FLAG/SEQ/QUAL=Phred40/XR/XG/XM SE+PE+complementary; per-record negative test; 2-line aux; FastQ byte-frozen | §5.5, §9 | DONE | See test-verification table. FastA-aware fakes use `NR%2==1`+`sub(/^>/)` (the load-bearing C-1). QUAL asserted `&[40u8; 6]`. Record-2-dies negative + FastQ-passes contrast in one test. FastQ 28 integration + 27 convert tests untouched (byte-frozen). |
| 7 | The two documented deviations (separate `convert_fasta_impl`; pbat→non-dir strand-test swap) | §13 | DEVIATED (documented, non-material) | Both documented in §13. (a) Separate core not shared `RecordShape` — endorsed alternative, keeps `convert_fastq_impl` byte-untouched + reuses helpers; lower risk. (b) `fasta_se_nondir_…` replaces the planned pbat cell because `--pbat ⊕ -f` DIES at config (Perl 8155, which the plan itself documents in §3.1/§9 edge cases) — non-dir reaches the same eff-3 CTOB record. Both non-material. |

## §9 Validation rows

| # | Verify | Status | Evidence |
|---|--------|--------|----------|
| 1 | FastA conversion bytes + `.fa` names, all 3 libs SE+PE | DONE | `fasta_se_c_to_t_golden`, `fasta_se_g_to_a_golden`, `fasta_pe_pbat_r1_ga_r2_ct` (golden bytes + `.fa`/`_C_to_T`/`_G_to_A` + `/1/1`,`/2/2`). |
| 2 | `-f` in aligner_options; `-q` unchanged | DONE | `fasta_uses_dash_f` (exact token+position); FastQ `-q` tests untouched. |
| 3 | SE FastA E2E: BAM SEQ + QUAL=`IIIIII` + FLAG/XM/XR/XG | DONE | `fasta_se_directional_mapped_phred40_qual` (QUAL `&[40u8;6]`, XR/XG=CT, XM=`.Z...Z`, FLAG 0). |
| 4 | PE FastA E2E (both mates QUAL `I×len`) | DONE | `fasta_pe_directional_mapped_phred40` (FLAG 99/147, both mates QUAL `&[40u8;6]`). |
| 5 | FastA-aware fakes (`NR%2`/`^>`) SE+PE+strand variants | DONE | 4 fakes: `…_fasta_mapped`, `…_fasta_ga_index`, `…_fasta_unmapped`, `…_pe_fasta`, all `NR%2==1`+`sub(/^>/)`. |
| 6 | FastA non-dir + pbat (strand fakes) | DONE (with deviation 7b) | `fasta_se_nondir_ga_index_writes_ctob_phred40` (FLAG 16 CTOB, XR/XG=GA, QUAL Phred40). pbat-cell replaced by non-dir because `--pbat⊕-f` dies (documented). |
| 7 | `--unmapped`/`--ambiguous` FastA = 2-line `>id\nseq`, `.fa.gz` name | DONE | `fasta_se_unmapped_writes_2line_fa_aux` (decompresses to `>r1\nACGTAC\n`, file `reads.fa_unmapped_reads.fa.gz`) + aux_out `fasta_record_two_line_no_qual` + `filename_fasta_extension`. |
| 8 | record-2 DIES under FastA (per-record `^>`); id-strip | DONE | `fasta_per_record_sanity_record2_dies` (FastA errs; FastQ same input OK). `fasta_record1_malformed_dies`. id-strip exercised via golden (`>read1 1:N:0` → `read1_1:N:0`) + the re-read `>`-strip in drive_merge. |
| 9 | FastQ directional/non-dir/pbat byte-frozen | DONE | 28 FastQ integration + 27 FastQ convert unit tests unchanged + green (regression guard). |
| 10 | oxy gate (FastA-converted subset, byte-identical to Perl) | DEFERRED | Separate post-audit step (per instruction); not part of this code audit. |

## Test verification (Mode B)

| Test | File | Status |
|------|------|--------|
| fasta_se_c_to_t_golden | convert.rs | PASS |
| fasta_se_g_to_a_golden | convert.rs | PASS |
| fasta_pe_pbat_r1_ga_r2_ct | convert.rs | PASS |
| fasta_per_record_sanity_record2_dies | convert.rs | PASS |
| fasta_record1_malformed_dies | convert.rs | PASS |
| fasta_se_gzip_decompresses_to_plain | convert.rs | PASS |
| fasta_pe_gzip_forced_off | convert.rs | PASS |
| fasta_empty_and_crlf | convert.rs | PASS |
| fasta_skip_and_upto | convert.rs | PASS |
| fasta_record_two_line_no_qual | aux_out.rs | PASS |
| filename_fasta_extension | aux_out.rs | PASS |
| fasta_uses_dash_f | options.rs | PASS |
| phred_without_fastq_errors | options.rs | PASS |
| fasta_se_directional_mapped_phred40_qual | tests/cli.rs | PASS |
| fasta_se_nondir_ga_index_writes_ctob_phred40 | tests/cli.rs | PASS |
| fasta_pe_directional_mapped_phred40 | tests/cli.rs | PASS |
| fasta_se_unmapped_writes_2line_fa_aux | tests/cli.rs | PASS |
| (28 FastQ integration + FastQ convert/options regression suite) | tests/cli.rs, convert.rs, options.rs | PASS (byte-frozen) |

Full suite: 226 passed, 0 failed.

## Gaps (detail)

None. Every behavioral item (§3.1–§3.5, edge cases), every implementation-outline step (§5.1–§5.6), and every validation row §9 #1–9 maps to present code + a passing test. §9 #10 (oxy gate) is correctly out of scope for this code audit.

## Notes (non-blocking, not gaps)

- §13 claims `phred33_with_fasta_dies`; the actual test is named `phred_without_fastq_errors` (options.rs:311) — same behavior asserted (`--phred33-quals -f` errs). Name-only drift in the implementer notes; non-material.
- §3.5 (report): verified the report header + final-analysis body are fully format-agnostic in `report.rs` (no FastA-specific wording exists in Perl either); the `-f` token rides inside `aligner_options`, so the report is byte-correct for FastA with no Phase-9a change. No new counters. Satisfied.
- Two untracked artifacts (`rust/bismark-aligner/reads_bismark_bt2.bam`, `…_SE_report.txt`) are stray test-run outputs, not part of the code under audit.

## Verdict

**COMPLETE.** All 7 ledger items and §9 validation rows #1–9 are implemented and covered by passing tests; the two deviations are documented and non-material; FastQ paths are byte-frozen. The only remaining work is §9 #10 (the oxy byte-identity gate), a separate post-review step.

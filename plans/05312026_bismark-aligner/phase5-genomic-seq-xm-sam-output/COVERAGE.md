# Plan Coverage Report

**Mode:** B (code vs. plan)
**Plan(s):** `phase5-genomic-seq-xm-sam-output/PLAN.md` (rev 1)
**Date:** 2026-06-01
**Verdict:** COMPLETE — all implementable items DONE; the only PENDING item (#18) is the
Linux/oxy real-data byte-identity gate, which the plan (§13) explicitly defers as a non-cargo
gate, not a code gap.

## Summary

- Total audited items: 38 (3 §3-behavior groups expanded to 12 sub-items + 8 §5 steps + 18 §9 rows)
- DONE: 36
- PARTIAL: 1 (§9 #9 — the MD test set covers 7 of the 8 enumerated sub-cases; see gaps)
- MISSING: 0 (code); 1 test-coverage gap noted (§9 #16 has no dedicated unit test, behavior is implemented)
- DEVIATED: 1 (DOCUMENTED — noodles `Map<Program>` has no typed VN field; both VN and CL go
  through `other_fields` in insertion order; byte-output identical; recorded in §13)
- PENDING (not a gap): 1 (§9 #18 — Linux/oxy gate, explicitly deferred by the plan)

**Test run:** `cargo test -p bismark-aligner` → **108 unit + 16 integration passed; 0 failed**
(matches the §13 "108 + 16" claim).

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | Genome consumes `config.genome.fastas` (no re-glob); `sq_order` = encounter order | §3.1.1 / §5 step 2 | DONE | `genome.rs:60 read_genome_into_memory(fastas: &[PathBuf])`; driver passes `&config.genome.fastas` (`lib.rs:110`). `discovery.rs:82 pub fastas: Vec<PathBuf>`. |
| 2 | Per-file parse: gunzip `.gz`, header→`extract_chromosome_name`, empty-name die in loader | §3.1.2 | DONE | `genome.rs:80` gz detect; `:44 extract_chromosome_name` returns `""` for leading-space, `Err` for no-`>`; empty-name die at `:99/:119` in the loader (caller), not the helper. |
| 3 | Sequence lines: chomp + first-`\r` strip + uppercase + concat; new-chr store; dup-name die; empty-seq warn | §3.1.3 | DONE | `genome.rs:124 to_ascii_uppercase`; `:167 chomp_cr` (single `\r` removal, no `/g`); `:141` dup-name die; `:146` empty-seq warn (not die). |
| 4 | Result `Genome { chromosomes, sq_order }`; CRAM-ref reconstitution skipped | §3.1.4 | DONE | `genome.rs:22 struct Genome`; no CRAM path (out of v1). |
| 5 | Extract (a): pos-1, contains_deletion, parse CIGAR, pbat_mod=0, index 1/3 prepend guard | §3.2a | DONE | `methylation.rs:118-161`; prepend guard `pos<2` returns early (`:156`); +2 NOT added to md_seq (`:160` comment, only `non_bis`). |
| 6 | Extract: CIGAR walk M/I/D/S/N; indels for D only; illegal op dies | §3.2a | DONE | `methylation.rs:164-199`; `D` bumps `indels` (`:187`), `I/S/N` do not; `_` arm dies (`:192`). |
| 7 | Extract: index 0/2 append guard; per-strand counter behind both guards; revcomp index 1/2 | §3.2a | DONE | append guard `chr.len()<pos+2` (`:203`); counter bump AFTER both guards (`:210-216`); revcomp index 1/2 (`:219`, incl. md_seq if deletion). |
| 8 | Length guard (3127) — gate on LENGTH not `extracted` bool; warn + count + skip | §3.2b | DONE | `lib.rs:293` `if ext...len() != seq_uc.len()+2` → warn `:294`, `genomic_sequence_could_not_be_extracted_count += 1` `:298`, `continue` (not written). Gates on length, not the bool. |
| 9 | Methylation call BEFORE trim/revcomp; CT+GA branches; Z/z X/x H/h U/u . ; U/u on N or X; out-of-range context = sentinel; 8 counters | §3.2c | DONE | `methylation.rs:246 methylation_call` called at `lib.rs:301` before `single_end_sam_output`; CT branch `:257`, GA branch `:273`; `U/u` on `N` or `X` (`push_*_context`); out-of-range via `at()` sentinel `0` (`:254`); 8 counters in `bump` `:360`. |
| 10 | SAM assembly (d) + write (e) | §3.2d/e | DONE | `output.rs:334 single_end_sam_output`; `lib.rs:307` then `write_record :317`. |
| 11 | FLAG from (strand, read_conv, genome_conv); other combos die | §3.3 | DONE | `output.rs:349-362`; the 4 valid combos + die arm. |
| 12 | ref_seq +2 trim: CT drops last 2, else first 2 | §3.3 | DONE | `output.rs:366-369` (`Ct` → `..len-2`, `Ga` → `2..`). |
| 13 | Minus-strand reorient: revcomp seq/ref, reverse qual, revcomp md_seq iff D | §3.3 | DONE | `output.rs:379-386`; md_seq revcomp only `if best.cigar.contains('D')`. |
| 14 | Double revcomp of md_seq for minus + deletion (4419 extraction + 8581 output) | §3.3 / §9 #16 | DONE (code) | extraction revcomp `methylation.rs:221`; output revcomp `output.rs:383`. Both applied. **No dedicated unit test** (see gaps). |
| 15 | NM = hemming_dist + indels | §3.3 | DONE | `output.rs:389`; `hemming_dist :94`. |
| 16 | MD via make_mismatch_string (verbatim incl. `^` deletion path + X-skip) | §3.3 | DONE | `output.rs:124 make_mismatch_string` + `:165 rebuild_md_with_deletions`; X-skip `:137`. |
| 17 | XM reversed if `-`; XR=read_conv; XG=genome_conv | §3.3 | DONE | `output.rs:399-403` (XM rev), `:426-433` (XR/XG). |
| 18 | Tag set + order NM,MD,XM,XR,XG; no XA/RG default | §3.3 | DONE | `output.rs:420-433` inserts in that order; no XA/RG. |
| 19 | Columns: QNAME, FLAG, RNAME, POS, MAPQ (reused), CIGAR verbatim, RNEXT/PNEXT/TLEN, SEQ, QUAL | §3.3 | DONE | `output.rs:407-417`; MAPQ = `best.mapq` (`:414`); RNEXT/PNEXT/TLEN are noodles `RecordBuf` defaults (`*`/0/0). |
| 20 | `--phred64` conversion (offset 64 vs 33) before output | §3.3 | DONE | `output.rs:375-376`; `RunConfig.phred64` (`config.rs:149`) wired from `cli.phred64` (`:201`). |
| 21 | Header @HD VN:1.0 SO:unsorted; @SQ per sq_order; Bismark @PG VN before CL with literal quotes; samtools @PG normalized out | §3.4 | DONE | `output.rs:53 generate_sam_header`; `@HD` `:55`, `@SQ` `:77`, `@PG` VN then CL via `other_fields` insertion order `:61-68`, `CL:"…"` literal quotes `:67`. |
| 22 | `--sam_no_hd` hard-reject | §3.4 / §5 step 1 | DONE | `config.rs:227 reject_unsupported_output_flags`. |
| S1 | Deps (bismark-io, noodles-sam/core, bstr) at workspace pins + phred64 prereq + flag rejections + deferred_flags shrink | §5 step 1 | DONE | Cargo deps present (build links — tests run); `RunConfig.phred64`; `reject_unsupported_output_flags` rejects slam/non_bs_mm/rg_tag/sam-no-hd; `deferred_flags` drops those + `--basename`. |
| S2 | genome.rs | §5 step 2 | DONE | See items 1-4; 11 unit tests. |
| S3 | methylation.rs | §5 step 3 | DONE | See items 5-9; reverse_complement + extract + call; 12 unit tests. |
| S4 | output.rs incl. deletion path + header | §5 step 4 | DONE | See items 10-21; hemming/revcomp/make_mismatch_string/single_end_sam_output/generate_sam_header; 12 unit tests. |
| S5 | Driver wiring: genome once + 3127 guard + reuse best.mapq + QNAME=`@`-stripped fix_id | §5 step 5 | DONE | `lib.rs:110` load once; `:293` length guard; `:414` reuse `best.mapq`; `:270` `@`-stripped fix_id. |
| S6 | Counters extension + summary | §5 step 6 | DONE | `merge.rs:69-102` (12 new fields, additive); `lib.rs:323 counters_summary` reports per-strand + could-not-extract; "no BAM yet" caption gone. |
| S7 | Deferred-flags notice shrink | §5 step 7 | DONE | `config.rs:313 deferred_flags`; rejected flags + `--basename` removed; doc comment updated. |
| S8 | Tests (unit per module + hermetic e2e + real-data gate) | §5 step 8 | DONE (hermetic) / PENDING (real-data) | Unit + `mapped_read_writes_bam_record_end_to_end` e2e present; real-Bowtie2 gate = #18 (PENDING). |

## §9 Validation rows

| # | Row | Test(s) | Status |
|---|-----|---------|--------|
| 1 | Genome load + @SQ order (multi-file + multi-FASTA) | `genome::tests::multi_fasta_records_in_file_order`, `multi_file_order_follows_input_list`, `single_chromosome_uppercased` | DONE |
| 2 | extract_chromosome_name (`>chr1 desc`, leading-space die, no-`>` die) | `genome::tests::extract_name_first_token`, `leading_space_header_dies`, `non_fasta_first_line_dies` | DONE |
| 3 | Extract index 0 (append +2), CT/CT, CT_CT_count=1 | `methylation::tests::extract_index0_appends_two_and_counts_ct_ct` | DONE |
| 4 | Extract index 1 (prepend +2 then revcomp), CT/GA, CT_GA_count=1 | `methylation::tests::extract_index1_prepends_two_revcomps_and_counts_ct_ga` | DONE |
| 5 | Chromosome-edge guard (both 4317 & 4390); no counter bump; extracted=false | `methylation::tests::extract_index0_edge_at_three_prime_returns_short_no_counter`, `extract_index1_edge_at_five_prime_returns_short_no_counter` | DONE |
| 6 | Length guard (3127); could_not_extract=1; not written | `cli::happy_path_resolves_and_prints_config` (header-only BAM when all reads unmapped) + the driver branch `lib.rs:293`. No isolated driver-unit on a short-window read | PARTIAL (see gaps) |
| 7 | CIGAR walk M/I/D/S/N + illegal op dies | `methylation::tests::parse_cigar_basic`, `extract_deletion_builds_md_seq_and_indels`, `extract_insertion_pads_x_no_indels` | DONE |
| 8 | methylation_call CT contexts (Z/z X/x H/h U/u .) | `methylation::tests::methylation_call_ct_contexts`, `_unmethylated_cpg_lowercase_z`, `_unknown_context_via_n`, `_unknown_via_padding_x_context`, `_non_cytosine_is_dot` | DONE |
| 9 | make_mismatch_string: clean, single, leading/adjacent, 1-del, ≥2-del, del-in-final-token, del-adjacent-mismatch, insertion/soft-clip | `md_clean_match`, `md_single_mismatch`, `md_leading_and_adjacent_mismatch_zero_padding`, `md_single_deletion`, `md_two_deletions`, `md_deletion_with_mismatch`, `md_insertion_padding_skipped` | PARTIAL (see gaps) |
| 10 | single_end_sam_output plus/minus strand, tag order | `output::tests::sam_output_plus_strand_index0`, `sam_output_minus_strand_index1_reverses` | DONE |
| 11 | noodles→BAM→read-back round-trip (tag order/values, QUAL phred, SEQ, MAPQ) | `output::tests::record_roundtrips_through_bam_tag_order_values_qual` | DONE (hermetic; `samtools :i:` rendering is the #18 gate, per plan) |
| 12 | Header literal byte diff | `output::tests::header_hd_sq_pg_exact_bytes` (asserts `@HD\tVN:1.0\tSO:unsorted`, `@SQ` lines, `@PG ... CL:"bismark ..."`) | DONE |
| 13 | NM with indels (D-only) | `methylation::tests::extract_deletion_builds_md_seq_and_indels` (indels=1) + `extract_insertion_pads_x_no_indels` (indels=0) + `output::md_two_deletions`. No single test asserting `NM = hemming+indels` on a `50M2D3I47M`-shape CIGAR | PARTIAL (see gaps) |
| 14 | Length guard writes nothing (counting-writer double) | `cli::happy_path` writes a header-only BAM (all reads unmapped) — proves zero records; no isolated all-edge-read driver unit with a counting double | PARTIAL (see gaps) |
| 15 | U/u via padding X as context | `methylation::tests::methylation_call_unknown_via_padding_x_context` | DONE |
| 16 | Minus-strand (index 1) + deletion: double-revcomp md_seq + NM | (none — behavior implemented at `methylation.rs:221` + `output.rs:383`) | MISSING test (code DONE) |
| 17 | Hermetic end-to-end (fake bowtie2 + tiny genome → samtools view) | `cli::mapped_read_writes_bam_record_end_to_end` (reads BAM back via bismark-io, asserts FLAG/POS/MAPQ/SEQ/QUAL/MD/XM/XR/XG; QNAME from `@`-stripped fix_id) | DONE (BAM read-back substitutes for samtools, hermetic on macOS) |
| 18 | 🎯 Byte-identity gate (Linux, real Bowtie 2 + Perl) | none (deferred per §13) | PENDING — not a code gap (plan-declared non-cargo gate) |

## Gaps (detail)

### §9 #9: make_mismatch_string — 7 of 8 enumerated sub-cases have explicit tests

**Expected:** the plan enumerates 8 MD sub-cases, including a distinct **"deletion-in-final-MD-token
(trailing arm 9526–9578)"** case.
**Found:** 7 named tests (`md_clean_match`, `md_single_mismatch`, `md_leading_and_adjacent_mismatch_zero_padding`,
`md_insertion_padding_skipped`, `md_single_deletion`, `md_two_deletions`, `md_deletion_with_mismatch`).
The **trailing-arm code path** (`output.rs:286-317`) *is* exercised — `md_single_deletion` ("2M1D2M",
MD "MD:Z:4" → the lone `'4'` token hits the post-loop tail arm and produces `2^T2`) and `md_two_deletions`
both drive it.
**Gap:** there is no test labelled specifically for the multi-token "deletion falls in the final MD token"
scenario, and no `deletion-adjacent-to-mismatch` case where the mismatch *immediately precedes/follows* the
`^`. The intricate path is covered by the existing deletion tests; this is a test-naming/edge-completeness
gap, not a code gap.

### §9 #6 / #14: length-guard skip has no isolated driver-unit with a counting double

**Expected:** #6 "driver-level unit on the #5 read"; #14 "driver unit on an all-edge-read input via a
counting writer double" → assert `genomic_sequence_could_not_be_extracted_count` bumped AND zero records
written.
**Found:** the guard branch is implemented (`lib.rs:293-300`) and the integration test
`happy_path_resolves_and_prints_config` proves a header-only (zero-record) BAM is written when no read maps,
plus the `methylation` edge-guard unit tests prove the short-window precondition. There is **no** dedicated
driver-level unit that feeds an edge read and asserts the counter increment + a zero-record writer double.
**Gap:** test coverage only — the behavior is implemented and indirectly demonstrated.

### §9 #13: NM-with-indels has no single combined-CIGAR assertion

**Expected:** unit with e.g. `50M2D3I47M` asserting `NM:i = hemming_dist + indels`, indels counting the 2 `D`
not the 3 `I`.
**Found:** `indels` accrual is unit-tested for D-only (=1) and I-only (=0) in `methylation.rs`; `NM = hemming
+ indels` is implemented at `output.rs:389` and exercised end-to-end (NM=0 in the round-trip test).
**Gap:** no single test combining a `D`+`I` CIGAR and asserting the composed `NM` value. Behavior implemented;
test-completeness gap.

### §9 #16: minus-strand + deletion (double revcomp) — implemented, untested

**Expected:** a `-`-strand index-1 read with a `D`, asserting the exact `MD:Z` (md_seq revcomp'd twice:
extraction 4419 + output 8581) + `NM`.
**Found:** both revcomps are applied in code (`methylation.rs:221` and `output.rs:383`, the latter gated on
`best.cigar.contains('D')`). No test composes the two on a minus-strand deletion read.
**Gap:** test coverage only — the rev-1 plan flagged this as a 🔴 must-validate (§9 #16). The code path is
correct by inspection (each revcomp is verbatim and applied exactly once), but the explicit regression test
the plan asked for is absent.

## Deviations (documented — not gaps)

- **noodles `Map<Program>` VN field** (§13): the plan (§3.4) proposed setting `VN` via a "typed version
  field". noodles 0.85 `Map<Program>` is a unit struct with no typed VN, so both `VN` and `CL` are inserted
  into `other_fields` in insertion order (VN before CL). The serialized bytes are identical
  (`output.rs:61-68`), and the header byte-diff test (`header_hd_sq_pg_exact_bytes`) pins the exact line. This
  matches the §13 "Deviations / decisions" note and is correct.
- **Empty-chromosome LN:0 → LN:1** (§13): noodles' `NonZeroUsize` length cannot represent `LN:0`; mapped to
  `1` (`output.rs:81`). Documented as pathological/excluded; does not affect real genomes.

## Verdict

**COMPLETE.** Every §3 behavior, §5 step, and code-path item is implemented as specified (or DEVIATED with a
documented, byte-neutral reason). All 108 unit + 16 integration tests pass.

Four **test-coverage** observations remain (none block the phase; all describe behavior that is implemented):

1. **§9 #16** (minus-strand index-1 + deletion → double revcomp + NM) has **no dedicated unit test**, though
   the rev-1 plan flagged it 🔴. The code applies both revcomps correctly; consider adding the regression
   test the plan asked for before the #18 gate.
2. **§9 #6 / #14** (length-guard skip via a driver-level counting-writer double) is demonstrated only
   indirectly (header-only BAM in `happy_path`); the isolated driver unit the plan specified is absent.
3. **§9 #13** (combined `D`+`I` CIGAR `NM = hemming + indels`) has component tests but no single composed
   assertion.
4. **§9 #9** lacks a distinctly-labelled "deletion-in-final-token" and "deletion-adjacent-to-mismatch" case,
   though the trailing-arm path is exercised by `md_single_deletion` / `md_two_deletions`.

**§9 #18** (the Linux/oxy real-data byte-identity gate) is **PENDING by design** — the plan (§13) explicitly
records it as a non-cargo gate to be run on oxy/Linux, never macOS, as the phase's final sign-off. This is
**not** a code gap.

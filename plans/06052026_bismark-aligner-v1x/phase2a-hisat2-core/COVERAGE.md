# Plan Coverage Report

**Mode:** B (code vs. plan)
**Plan(s):** `plans/06052026_bismark-aligner-v1x/phase2a-hisat2-core/PLAN.md` (rev 1)
**Date:** 2026-06-05
**Verdict:** COMPLETE — all in-scope items DONE; V9 (oxy SE gate) is a deferred gate (explicitly the next step, not a code gap).

## Summary

- Total items: 19 (10 behavior §3.1–§3.9, 9 validation V1–V10)
- DONE: 18
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 3 (all documented in the plan's "Deviations" section → acceptable, not gaps)
- DEFERRED (not a code gap): 1 (V9 oxy SE gate — by design the next step)

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | `--hisat2`→`Aligner::Hisat2`, conflicts preserved, minimap2 deferred | §3.1 / config.rs | DONE | `resolve_aligner` returns `Hisat2` (L311-313); `--hisat2+--bowtie2`/`--hisat2+--minimap2`/`--minimap2+--bowtie2` conflict dies (L295-310); minimap2 still `Unsupported` (L314-318). 4 unit tests. |
| 2 | `detect_aligner(Hisat2, path_to_hisat2)`, `PINNED_HISAT2_VERSION="2.2.2"`, version parser reuse | §3.2 / aligner.rs | DONE | `detect_aligner(kind, path)` (L80); `PINNED_HISAT2_VERSION="2.2.2"` (L21); per-kind `binary_name`/`pinned_version`/`path_flag`; `parse_bowtie2_version` reused verbatim. `resolve` picks `path_to_hisat2` for HISAT2 (config L225-228). Tests: hisat2 banner parse, helpers. |
| 3 | SE + PE HISAT2 option strings; append LAST; **no `--dovetail`** PE | §3.3 / options.rs | DONE | `apply_aligner_specific_options` appends `--no-softclip --omit-sec-seq` to the finished string (L187-248); `--dovetail` (and `--old_flag` conflict) gated on `Bowtie2` (L150). Tests `hisat2_se_option_string` (V2), `hisat2_pe_option_string_has_no_dovetail` (V3, hard literal + `!contains("--dovetail")`). |
| 4 | Splice flags: push before softclip; both-set die; non-HISAT2 die; missing-file die | §3.4 / options.rs | DONE | Tail order `[--no-spliced-alignment][--known-splicesite-infile <f>] --no-softclip --omit-sec-seq` (L224-245); both-set die (L226-232); missing-file die (L236-241); non-HISAT2 die for both flags (L206-221). 5 unit tests (V8). |
| 5 | Per-aligner suffix list: 8 `.ht2` + `.ht2l`; Bowtie2 6 `.bt2` | §3.5 / discovery.rs | DONE | `index_suffixes(aligner, stem, large)` returns Vec (Bowtie2 6, HISAT2 `1..=8`, no `rev.*`; `.ht2l` large) (L92-106); threaded through `first_missing`+`discover_genome`. 6 HISAT2 tests + Bowtie2 byte-frozen (V4). |
| 6 | `aligner_token` in `default_suffix` at lib.rs (6) AND parallel.rs (10); NOT basename/_unmapped/_ambiguous; report line "run with HISAT2" | §3.6 / lib.rs, parallel.rs, report.rs | DONE | `Aligner::token()` (config L32-37); threaded into SE/PE bam+report+ambig `default_suffix` in lib.rs (6 sites) and parallel.rs single+multicore (10 sites); `_unmapped_reads`/`_ambiguous_reads` live in `aux_out.rs` (UNCHANGED — no token); `basename_suffix` untouched. `ReportHeader.aligner` + "Bismark was run with {HISAT2|Bowtie 2}" branch (report.rs L67-72). |
| 7 | `--ambig_bam` HISAT2 decision (resolved + implemented) | §3.7 / config.rs | DONE | OQ-2d resolved: single-core HISAT2+`--ambig_bam` supported (`_bismark_hisat2.ambig.bam` by token construction); `--multicore(>1)`+HISAT2+`--ambig_bam` hard-rejected (config L212-219). 2 integration tests (reject + single-core token). |
| 8 | Bowtie 2 byte-frozen | §3.9 / options.rs, discovery.rs, naming | DONE | Append-to-finished-string + `Bowtie2`-gated `--dovetail` + token-only-at-`default_suffix` keep it structural. `paired_end_tail_and_default_maxins` (with `--dovetail`) and `bowtie2_pe_string_byte_frozen_with_aligner_param` green; full suite (271 tests) unchanged. |
| V1 | Bowtie 2 byte-frozen (full suite incl. PE-dovetail) | §9 | DONE | Full suite 271 green; PE-dovetail cell present in options.rs. (Bowtie2 **oxy** gate re-run = part of V9 deferred step.) |
| V2 | HISAT2 SE option string | §9 / options.rs | DONE | `hisat2_se_option_string` asserts `…--ignore-quals --no-softclip --omit-sec-seq`. |
| V3 | HISAT2 PE option string, no dovetail | §9 / options.rs | DONE | `hisat2_pe_option_string_has_no_dovetail` hard literal `…--no-mixed --no-discordant --maxins 500 --no-softclip --omit-sec-seq` + `!contains("--dovetail")`. |
| V4 | `.ht2`/`.ht2l` 8-suffix discovery | §9 / discovery.rs | DONE | `hisat2_suffix_arity_is_eight_ht2`, `discovers_complete_ht2_index`, `falls_back_to_large_ht2l_index`, `incomplete_ht2_index_errors_with_hisat2_wording`, `six_ht2_files_is_not_a_complete_hisat2_index`, `bt2_index_rejected_in_hisat2_mode`. |
| V5 | SE `ZS` 2nd-best → MAPQ (tie + shift) | §9 / merge.rs, align.rs | DONE | `hisat2_se_zs_equal_as_is_ambiguous` (ZS==AS tie→ambiguous), `hisat2_se_zs_below_as_is_unique_best_with_zs_second` (ZS<AS→unique best, second=-6). `SamRecord::parse` accepts `XS:i:`/`ZS:i:` (align.rs L100-104, tests L500/508). |
| V6 | spliced `N` extraction (multi-N, N+indel, GA-strand) | §9 / methylation.rs | DONE | `extract_spliced_n_skips_intron_index0`, `extract_multi_n_spliced_index0`, `extract_n_and_deletion_counts_d_only_index0` (D counted, N not), `extract_spliced_n_on_ga_strand_index3`. (oxy 12-rec cell ⊂ V9 deferred.) |
| V7 | naming/report | §9 / cli.rs | DONE | `hisat2_se_mapped_names_and_report`: `reads_bismark_hisat2.bam` exists, `_bt2` does not; report "Bismark was run with HISAT2" + echoes `--no-softclip --omit-sec-seq`. |
| V8 | splice flags | §9 / options.rs, cli.rs | DONE | Unit dies (both-set/missing/non-HISAT2) + `hisat2_no_spliced_alignment_echoed_in_report` integration (the gate-cell-with-`--no-spliced-alignment` ⊂ V9 deferred). |
| V9 | 🎯 SE oxy gate | §9 | DEFERRED | Explicitly the next step per the plan + Implementation Notes ("NOT done here … the remaining step before commit"). Not a code gap. |
| V10 | discard arithmetic (smoke) | §9 | DONE (smoke) | Demoted to a smoke check by the plan; SE end-to-end report tests (`hisat2_se_mapped_names_and_report`) exercise the unique-best/discard counters consistently (`Sequences analysed in total:\t1`, `Mapping efficiency:\t100.0%`). Not a correctness guard by design. |

## Deviations (documented in the plan → acceptable)

### D1: `--local` + `--hisat2` experimental path not reproduced
**Plan:** §3 default endToEnd tail only; Deviation #1.
**Found:** `--local` rejected for every aligner upstream (options.rs L72-78); the Perl HISAT2+`--local` `--omit-sec-seq`-only path (Perl 8310-8312) is intentionally not reproduced.
**Assessment:** DEVIATED-but-documented (Deviation #1). Off the v1 byte-identity spine. Acceptable.

### D2: splice-flag die ordering
**Plan:** Deviation #2.
**Found:** the non-HISAT2 / both-set / missing-file dies live in `build_aligner_options` (called after `discover_genome`+`detect_aligner` in `resolve`), whereas Perl raises them earlier in `process_command_line`. All STDERR/non-gated, all fail-loud, no silent no-op.
**Assessment:** DEVIATED-but-documented. Acceptable (no gate impact; which error fires first can differ only for a malformed invocation).

### D3: error.rs / `summary()` / `RunConfig` doc comments made aligner-aware
**Plan:** Deviation #3 (these were marked "optional fidelity").
**Found:** `FaultyIndex{aligner,…}`, `AlignerNotWorking{…,path_flag}` aligner-aware (error.rs L43-68); `summary()` uses `self.aligner.name()` (config L550).
**Assessment:** DEVIATED-but-documented. All STDERR/non-gated. Acceptable (honesty improvement).

## Out-of-scope confirmation (expected absence, NOT a gap)

- **PE read-1 `ZS` asymmetry fix:** correctly NOT present. The merge.rs diff adds only SE ZS tests (`mapped_zs`, `hisat2_se_zs_equal_as_is_ambiguous`, `hisat2_se_zs_below_as_is_unique_best_with_zs_second`); no PE read-1 / first-in-pair / second-best asymmetry logic was touched (grep for `read_1`/`asymmetr`/`0x40`/`first_in_pair` in the merge diff → no matches). This is Phase 2b by design (§4 split, Implementation Notes "NOT done here").

## Test verification (Mode B)

Command: `cargo test -p bismark-aligner` (in `rust/`) → **271 passed; 0 failed** (228 lib + 43 integration; doc-tests 0).

| Test | File | Status |
|------|------|--------|
| resolve_aligner_selects_hisat2 / _minimap2_still_deferred / _rejects_conflicting_selections / _defaults_to_bowtie2 | src/config.rs | PASS |
| parses_hisat2_version_line / aligner_token_and_name | src/aligner.rs | PASS |
| hisat2_se_option_string (V2) | src/options.rs | PASS |
| hisat2_pe_option_string_has_no_dovetail (V3) | src/options.rs | PASS |
| bowtie2_pe_string_byte_frozen_with_aligner_param / paired_end_tail_and_default_maxins (V1) | src/options.rs | PASS |
| hisat2_nosplice_appends_before_softclip / hisat2_known_splices_appends / hisat2_both_splice_flags_die / hisat2_known_splices_missing_file_dies / non_hisat2_splice_flags_die (V8) | src/options.rs | PASS |
| hisat2_suffix_arity_is_eight_ht2 / discovers_complete_ht2_index / falls_back_to_large_ht2l_index / incomplete_ht2_index_errors_with_hisat2_wording / six_ht2_files_is_not_a_complete_hisat2_index / bt2_index_rejected_in_hisat2_mode (V4) | src/discovery.rs | PASS |
| hisat2_se_zs_equal_as_is_ambiguous / hisat2_se_zs_below_as_is_unique_best_with_zs_second (V5) | src/merge.rs | PASS |
| extract_spliced_n_skips_intron_index0 / extract_multi_n_spliced_index0 / extract_n_and_deletion_counts_d_only_index0 / extract_spliced_n_on_ga_strand_index3 (V6) | src/methylation.rs | PASS |
| header_hisat2_run_with_line | src/report.rs | PASS |
| hisat2_se_mapped_names_and_report (V7) | tests/cli.rs | PASS |
| hisat2_no_spliced_alignment_echoed_in_report (V8) | tests/cli.rs | PASS |
| ambig_bam_with_multicore_hisat2_is_rejected / ambig_bam_single_core_hisat2_names_hisat2_token (§3.7) | tests/cli.rs | PASS |
| hisat2_is_accepted_not_deferred | tests/cli.rs | PASS |

## Verdict

**COMPLETE.** Every in-scope §3 behavior and §9 validation item (V1–V8, V10) is implemented as specified, with tests present and green (271/271). The three deviations are all documented in the plan's "Deviations" section and are byte-neutral (STDERR/non-gated). The PE read-1 `ZS` asymmetry is correctly excised to Phase 2b. The only unfulfilled row, **V9 (oxy SE byte-identity gate)**, is explicitly the deferred next step per the plan and Implementation Notes — a gate to run on oxy, not a code gap. No PARTIAL or MISSING items.

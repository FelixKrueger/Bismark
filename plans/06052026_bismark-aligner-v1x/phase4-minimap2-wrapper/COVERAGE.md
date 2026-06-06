# Plan Coverage Report — Phase 4 (minimap2 SE wrapper)

**Mode:** B (code vs. plan) — plus a fold-back audit of the dual plan-review action items.
**Plan:** `plans/06052026_bismark-aligner-v1x/phase4-minimap2-wrapper/PLAN.md` (rev 1)
**Branch:** `rust/aligner-mm2` (uncommitted) @ `~/Github/Bismark-aligner`
**Date:** 2026-06-05
**Verdict:** **COMPLETE** — every in-scope §5 step, every code-reachable V-gate (V2–V8, V11, V5b, PE-reject), and every folded review action item is implemented and tested. The two oxy gates (V9/V10) are correctly **deferred, not dropped** (the explicit post-review step; the V9 zero-secondary harness requirement IS carried into the plan and is unimplemented-as-expected).

## Summary

- Total audited items: 35 (10 §5 steps + 11 V-gates + 5 OQ resolutions + 9 review action items; some overlap)
- DONE: 33
- DEFERRED-AS-PLANNED: 2 (V9 oxy SE gate, V10 MAPQ-by-BAM-identity — the explicit post-review gate step)
- PARTIAL / MISSING / DEVIATED-undocumented: 0
- DEVIATED-documented: 1 (PE-minimap2 → hard reject rather than "documented gap only"; documented in §12)
- Test suite: **253 lib + 47 integ = 300 green** (0 failed), matches the plan's expected count exactly.

## Coverage ledger — §5 implementation outline

| # | §5 step | Implemented? | Test name(s) | Notes |
|---|---------|--------------|--------------|-------|
| 1 | Lock Bowtie2+HISAT2 baseline | DONE | `se_argv_bowtie2_shape_frozen`, `se_argv_hisat2_same_shape_as_bowtie2`, `bowtie2_hisat2_strings_byte_frozen_alongside_minimap2` + the full pre-existing suite (300 green) | V1 unit-level guards present; the full-suite oxy gate re-run is part of V9 (deferred). |
| 2 | config: `Aligner::Minimap2`+token/name; resolve→Minimap2; preset/maxlen dies; un-defer; detection path | DONE | `resolve_aligner_selects_minimap2`, `minimap2_token_and_name`, `mm2_flags_require_minimap2_mode`, `mm2_maximum_length_range_and_default` | `token()="mm2"`, `name()="minimap2"` (lowercase). `resolve_mm2_max_length` folds BOTH Perl blocks (8329-8341 mode-die + 8344-8356 range/default-10000). Detection path arm → `cli.path_to_minimap2`. |
| 3 | aligner: PINNED const; `parse_minimap2_version`; `detect_aligner(Minimap2)` | DONE | `minimap2_detection_metadata`, `parses_bare_minimap2_version` | `PINNED_MINIMAP2_VERSION="2.31-r1302"`; bare first-non-empty-line parse; Bowtie2 banner parser asserted NOT to match the bare number. `binary_name`/`pinned_version`/`path_flag` arms added; detect dispatches parser by `kind`. |
| 4 | options: clean-slate `minimap2_options` (kind-gated); preset selection | DONE | `minimap2_default_option_string`, `minimap2_preset_selection`, `minimap2_preset_conflicts_die`, `minimap2_clean_slate_discards_bowtie2_flags`, `minimap2_still_validates_bowtie2_base`, `bowtie2_hisat2_strings_byte_frozen_alongside_minimap2` | Default `-a --MD --secondary=no -t 2 -x map-ont -K 250K`. Build-then-wipe order preserved (Bowtie2 base built+validated, then substituted) → `-N 2 --minimap2` still dies. |
| 5 | discovery: single `.mmi` suffix | DONE | `minimap2_suffix_is_single_mmi`, `discovers_complete_mmi_index`, `missing_mmi_errors_with_minimap2_wording`, `bt2_index_rejected_in_minimap2_mode` | `index_suffixes(Minimap2)=["<stem>.mmi"]`; `large` ignored (no `.mmil`) → O-4 short-circuit satisfied (`large_index` stays false). |
| 6 | align: per-aligner spawn shape (positional `.mmi`, no orient/-x/-U) | DONE | `se_argv_minimap2_positional_mmi`, `se_argv_minimap2_orientation_independent`, `minimap2_s2_tag_is_ignored`, + the frozen-shape V5b tests | Extracted pure `build_se_argv(aligner,…)`; `spawn` gained `aligner` param + aligner-neutral error strings. SamRecord parse unchanged. |
| 7 | convert: SE no-change; wire `--mm2_maximum_length` | DONE | `minimap2_max_length_drop_counts_as_no_alignment` (end-to-end) + `mm2_maximum_length_range_and_default` (config) | **SE converter byte-frozen by non-modification** (verified: `id_suffix=b""`, line 177; cutoff at convert.rs 332-333 driven from `ConvertOptions.maximum_length_cutoff` ← `read_processing` ← `resolve_mm2_max_length`). No `/1` added to SE — correct (the `/1` is PE-only, deferred). |
| 8 | naming/report + lib dispatch | DONE | `minimap2_se_mapped_names_and_report` | `_bismark_mm2.bam` / `_bismark_mm2_SE_report.txt`; report echoes "Bismark was run with minimap2" + the clean-slate option string; asserts NOT `bt2`/`hisat2`, NOT `-q --score-min`. lib.rs SE spawn site passes `config.aligner` (line 280-282). |
| 9 | minimap2-aware fakes + integration | DONE | `make_fake_minimap2_mapped` (bare-version banner, positional-`.mmi`-only — cannot false-pass on a Bowtie2-shaped argv); used by the two integration tests | Fake feeds a positive `AS:i:` + present-but-ignored `s2:i:`, UNMAPPED on GA → unique OT best. Exercises the merge-no-op end-to-end. |
| 10 | 🎯 oxy byte-identity gate | DEFERRED-AS-PLANNED | — (post-review oxy step) | §12 marks V9/V10 as the remaining gate step; the plan's V9 zero-secondary/supplementary harness assertion is documented and carried (see V9 row). Correctly NOT silently dropped. |

## Coverage ledger — §9 V-table (V1–V11 + V5b)

| V# | Gate | Status | Test(s) | Notes |
|----|------|--------|---------|-------|
| V1 | Bowtie2+HISAT2 byte-frozen | DONE (unit) / DEFERRED (oxy) | `se_argv_bowtie2_shape_frozen`, `se_argv_hisat2_same_shape_as_bowtie2`, `bowtie2_hisat2_strings_byte_frozen_alongside_minimap2`, + 300-test suite green | Unit-level freeze proven through the refactored builder + the option-string assembly. The full oxy gate re-run (incl. the review-required Bowtie2 non-dir `--nofw` + HISAT2 PE cells) is part of the V9 oxy step (deferred). |
| V2 | default option string (hard literal) | DONE | `minimap2_default_option_string` | Asserts exact `-a --MD --secondary=no -t 2 -x map-ont -K 250K`. |
| V3 | preset selection + dies | DONE | `minimap2_preset_selection` (sr/map-pb/map-ont incl. explicit `--mm2_nanopore`→`map-ont`), `minimap2_preset_conflicts_die` (3 conflicts), `mm2_flags_require_minimap2_mode` (non-mm2-preset dies) | I-5 (`--mm2_nanopore`→`map-ont`) explicitly enumerated + tested. |
| V4 | `.mmi` discovery | DONE | `minimap2_suffix_is_single_mmi`, `discovers_complete_mmi_index`, `missing_mmi_errors_with_minimap2_wording`, `bt2_index_rejected_in_minimap2_mode` | Single-file resolve + missing-`.mmi` error (asserts `aligner=="minimap2"`, missing ends `.mmi`). |
| V5 | minimap2 spawn shape | DONE | `se_argv_minimap2_positional_mmi`, `se_argv_minimap2_orientation_independent` | Positional `.mmi`; asserts NO `--norc`/`--nofw`, NO `-U`, index NOT passed as `-x <basename>` (only the `.mmi` form). |
| V5b | Bowtie2/HISAT2 argv frozen through refactored builder | DONE | `se_argv_bowtie2_shape_frozen`, `se_argv_hisat2_same_shape_as_bowtie2` | Reviewer-A action item #7 — regression guard present; `--nofw` branch exercised (hisat2 test). |
| V6 | SamRecord parse — feed a real `s2:i:` tag, assert `second_best==None` | DONE | `minimap2_s2_tag_is_ignored` | Feeds a full minimap2 tag set (`AS:i:20 … s1:i:18 s2:i:14 … MD:Z:10`); asserts AS captured, MD captured, `second_best==None`. Code comment added at the tag-scan loop flagging `s2:i:` intentionally ignored. Guards the spike's WRONG "read s2" instruction (B I-4). |
| V7 | SE convert + max-len | DONE | `mm2_maximum_length_range_and_default` (`<100`/`>100000` die; `100`/`100000` OK; absent→10000; non-mm2→None), `minimap2_max_length_drop_counts_as_no_alignment` (>cutoff dropped, still analysed/no-align) | SE-no-suffix verified by non-modification of the converter (id_suffix `b""`) + the integration mapping success. Range-die + default + count-interaction (I-2/I-3) all covered. |
| V8 | naming/report (SE) | DONE | `minimap2_se_mapped_names_and_report` | `_bismark_mm2*` + "run with minimap2" (lowercase). |
| V9 | 🎯 SE oxy gate + zero-secondary/supplementary harness assertion | DEFERRED-AS-PLANNED | — | §12 lists V9 as the remaining oxy step; the plan §9/§5.10 carries the review-A zero-flag-256/2048 harness requirement. Not yet implemented = expected. Correctly NOT dropped. |
| V10 | MAPQ parity (implied by BAM byte-identity) | DEFERRED-AS-PLANNED | — (within V9) | Same oxy step. |
| V11 | un-deferral tests updated | DONE | `resolve_aligner_selects_minimap2`, `minimap2_is_accepted_not_deferred` (integ), `mm2_flags_require_minimap2_mode`, `mm2_maximum_length_range_and_default` | Old `resolve_aligner_minimap2_still_deferred` (unit) rewritten to `resolve_aligner_selects_minimap2`; old `minimap2_is_deferred` (integ) rewritten to `minimap2_is_accepted_not_deferred` (asserts NO "deferred" string, fails later on missing read file). The `--mm2_maximum_length`-deferred error flipped (now valid in mm2 mode, still errors outside). |

## Coverage ledger — §10 OQ resolutions

| OQ | Resolution required | Status | Where |
|----|---------------------|--------|-------|
| OQ-4a | version-parse = first whole line, bare `2.31-r1302` | DONE | `parse_minimap2_version` + `parses_bare_minimap2_version` |
| OQ-4b | default `map-ont` gated at oxy; sr/map-pb unit-tested not gated | DONE (unit) / DEFERRED (gate scope) | `minimap2_preset_selection` unit-tests all three; the oxy gate scope is the V9 step |
| OQ-4c | PE-minimap2 deferred | DONE (as hard reject) | `config::resolve` PE-reject + `minimap2_paired_end_is_rejected` (integ). **DEVIATED-documented:** §12 records the choice to fail loudly (`Unsupported`) rather than only "document a gap" — faithful + safe, mirrors the HISAT2-multicore reject; prevents silently feeding a Bowtie2-shaped argv to minimap2. |
| OQ-4d | multicore minimap2 NOT hard-rejected (gated at V9) | DONE | **Confirmed:** there is NO multicore reject for Minimap2 (grep of `validate_multicore` / `resolve` shows the only aligner-specific multicore reject is the pre-existing HISAT2 one; Minimap2 falls through). The `--multicore` SE gate cell is part of V9 (deferred). |
| OQ-4e | reproduce `-t 2` verbatim | DONE | `minimap2_default_option_string` pins `-t 2` literally; the 1M multi-minibatch determinism check is the V9 oxy step (deferred). |

## Coverage ledger — folded plan-review action items (A + B)

| Item | Source | Status | Where |
|------|--------|--------|-------|
| SE convert adds NO suffix (the `/1` is PE-only) | A I-1 / B I-1 | DONE | SE converter unmodified (`id_suffix=b""`); integration `minimap2_se_mapped_names_and_report` maps with bare IDs; plan §0/§2/§5 re-scoped to PE. |
| V9 zero-secondary/supplementary harness assertion | A I-2 | CARRIED-AS-PLAN | Documented in plan §9 V9 + §5.10 + §12 ("incl. the zero-secondary/supplementary harness assertion"). Not yet implemented = the deferred oxy step (expected). |
| Update un-deferral tests | A I-3 / V11 | DONE | See V11 row. |
| max-len range-die + default-10000 + count interaction | B I-2 / B I-3 | DONE | `resolve_mm2_max_length` (range `100..=100_000`, default 10000) + `mm2_maximum_length_range_and_default` + `minimap2_max_length_drop_counts_as_no_alignment` (I-3: 2 analysed / 1 unique / 1 no-align / 50.0%). |
| Spike "read s2" flagged WRONG + V6 real-`s2` test + code comment | B I-4 / A | DONE | Code comment at align.rs tag-scan loop; `minimap2_s2_tag_is_ignored` feeds a real `s2:i:` and asserts `None`; plan §2/§11 corrects the spike. |
| `--mm2_nanopore`→`map-ont` enumerated (I-5) | B I-5 | DONE | `minimap2_options` `else` branch + `minimap2_preset_selection` explicit nanopore case. |
| O-4: short-circuit `large_index` for Minimap2 | B O-4 | DONE | `index_suffixes(Minimap2)` ignores `large`; `discovers_complete_mmi_index` asserts `!large_index`; documented in discovery.rs doc-comment. |
| PE-minimap2 multi-mapper test (I-6) | B I-6 | N/A (deferred with PE) | Folds behind the PE reject (OQ-4c). Out of Phase-4 scope. |
| OQ-4d rationale: `-p`/`--reorder` wiped → always `-t 2` | A optional / B O-2 | DONE (by construction) | The clean-slate `minimap2_options` never emits `-p`/`--reorder`; `minimap2_clean_slate_discards_bowtie2_flags` proves the wipe. |

## Test verification

| Suite | Result |
|-------|--------|
| `bismark-aligner` lib | 253 passed; 0 failed |
| `bismark-aligner` integ (`tests/cli.rs`) | 47 passed; 0 failed |
| doctests | 0; 0 failed |

Run: `cargo test --manifest-path rust/Cargo.toml -p bismark-aligner` → **300 green**, matches the expected 253 + 47.

## Gaps

**None blocking.** The only unimplemented items are V9 (oxy SE byte-identity gate, incl. the zero-secondary/supplementary harness assertion, the `--multicore` SE cell, and the 1M run-to-run determinism check) and V10 (MAPQ-by-BAM-identity, implied by V9). Both are the **explicit, plan-designated post-review oxy step** — correctly deferred, documented in §12, and carry forward the review's harness requirement. They are NOT silently dropped.

## Minor observations (non-gaps, for awareness — NOT coverage failures)

- `cli.rs` doc-comments for the `--minimap2` / `--mm2_*` flags still read "(deferred …)". These are stale cosmetics with zero behavioral effect (the flags are fully wired). Out of plan scope; flag only if a doc pass is wanted.
- The PE-minimap2 handling is a **documented deviation** from the literal plan wording ("documented known gap") to a **hard `Unsupported` reject**. This is recorded in §12 Deviations and is the safer faithful choice; not a coverage gap.

## Verdict

**COMPLETE.** Every in-scope Phase-4 task (§5 steps 1–9), every code-reachable validation gate (V1 unit / V2–V8 / V5b / V11 / PE-reject), every OQ resolution (4a–4e), and every folded dual-review action item (A I-1/I-2/I-3 + #7; B I-1/I-2/I-3/I-4/I-5/O-4) is implemented and tested, with the full 300-test suite green. V9 (oxy SE gate + zero-secondary/supplementary harness) and V10 (MAPQ) are correctly deferred to the designated post-review oxy step — present in the plan, not dropped. No remediation required before the V9 gate.

# Plan Coverage Report

**Mode:** B (code vs. implementation plan)
**Plan(s):** `plans/05312026_bismark-bam2nuc/PLAN.md` (rev 1) — cross-referenced against `SPEC.md` (rev 1)
**Date:** 2026-05-31
**Verdict:** COMPLETE — 0 items unresolved (1 documented PENDING manual step: the oxy gate RUN, which is not a code gap)

## Summary

- Total ledger items: **67** (43 checklist rows + 18 Phase A–F tasks + 6 review-fold-in / deviation items audited separately)
- DONE: **66**
- PARTIAL: **0**
- MISSING: **0**
- DEVIATED (all DOCUMENTED): **4** distinct deviations (D-impl-1..4), each acceptable per the PLAN's "Implementation notes"
- PENDING (manual, not a code gap): **1** — the oxy real-data gate RUN (harness files exist + committed)

All 18 tasks across Phases A–F are implemented. Every one of the 43 checklist rows maps to live code + a passing test. `cargo test -p bismark-bam2nuc` = **70 unit + 12 golden + 2 sanity pass, 1 `#[ignore]` real-data smoke skipped, 0 doctests** — exactly matching the PLAN's claim. `cargo clippy -p bismark-bam2nuc --all-targets -- -D warnings` is clean.

---

## Coverage ledger — 43-row Plan checklist

| # | Plan item (SPEC §) | Task(s) | Status | Evidence |
|---|---|---|---|---|
| 1 | Crate + workspace member + binary `bam2nuc_rs` + thin `main.rs` ExitCode 0/1/2 | A1,E1 | DONE | `Cargo.toml` `[[bin]] name="bam2nuc_rs"`; workspace `members` includes `bismark-bam2nuc`; `main.rs` returns `ExitCode` (0 ok, 1 err, clap 2) |
| 2 | `--version` one-liner `version_string()` | A1 | DONE | `lib.rs:96 version_string()` → `"bam2nuc_rs {ver} ({os}/{arch})"`; `sanity.rs` asserts prefix + OS |
| 3 | CLI parse all options + positional inputs | A2 | DONE | `cli.rs` `Cli` struct; tests `parses_inputs_and_genome`, `parses_all_options`, `long_genome_folder_alias_parses` |
| 4 | Path resolution (output_dir/parent_dir/genome_folder defaults + slashes) | A2 | DONE | `resolve_output_dir`; tests `output_dir_defaults_to_empty_prefix`, `...gets_trailing_slash_but_not_absolute`, `...keeps_existing_trailing_slash`, `parent_dir_defaults_to_cwd` |
| 5 | Arg-count rule (no inputs AND not GCO → error) | A2 | DONE | `cli.rs:92`; test `rejects_no_input_without_genomic_composition_only` |
| 6 | `--samtools_path` accepted-but-ignored no-op | A2 | DONE | parsed but dropped in `validate`; test `samtools_path_is_dropped_not_stored` |
| 7 | Genome reader: 4-suffix glob priority, Mus skip, uppercase, `\r` strip, dup error, first-token name, gz | A3 | DONE | `genome.rs`; 16 tests incl. `glob_priority_fa_beats_fa_gz`, `mus_*`, `crlf_*`, `duplicate_name_cross_file_errors`, `loads_plain_gzip_fa_gz`, `loads_bgzf_fa_gz` |
| 8 | Genome `seqs()` all-sequences accessor (order-agnostic) | A3 | DONE | `genome.rs:88 seqs()`; test `seqs_yields_all_pairs_order_agnostic` |
| 9 | `process_sequence` mono counting (skip N, count IUPAC) | B1 | DONE | `freqs.rs:109`; tests `process_sequence_basic_mono_and_di`, `...skips_n_*`, `...counts_iupac` |
| 10 | `process_sequence` di counting (overlapping, skip N-window, count IUPAC) | B1 | DONE | `freqs.rs:114-120`; tests `...overlapping_windows`, `...counts_iupac` |
| 11 | Genomic composition over all chromosomes (+strand, no revcomp) | B3 | DONE | `freqs.rs:128 compute_genomic`; test `compute_genomic_sums_all_chromosomes` (no cross-chr di) |
| 12 | Cache WRITE: bytewise-sorted `word\tcount\n`, count-everything | B3 | DONE | `freqs.rs:77 cache_bytes()`; tests `cache_bytes_acgtn_exact_order`, `cache_bytes_iupac_sort_placement` |
| 13 | Cache write precedence: genome_folder → output_dir → skip-with-warn | B3 | DONE | `freqs.rs:166 write_cache`; test `write_cache_falls_back_to_output_dir_when_genome_dir_readonly` (unix) |
| 14 | Cache READ + reuse-if-present; existence only vs genome_folder | B4 | DONE | `freqs.rs:142 get_genomic_frequencies` + `read_cache`; tests `...reuses_existing_cache_byte_for_byte`, `existence_checked_only_against_genome_folder`, `read_cache_round_trips_via_write` |
| 15 | chr-name table from header reference sequences | C1 | DONE | `count.rs:45 build_chr_name_table` (non-ASCII → error) |
| 16 | CIGAR `[IDSN]` skip = {Ins,Del,SoftClip,Skip}; H/P/=/X kept | C2 | DONE | `count.rs:62 cigar_has_indel`; test `cigar_has_indel_detects_idsn_keeps_others` |
| 17 | Genomic span `substr` with saturation (never panic) | C3 | DONE | `count.rs:76 extract_span`; test `extract_span_saturation` (5 cases incl. strictly-past-end + missing chr) |
| 18 | SE flag correction: 0→fwd, 16→revcomp, else→hard error | C4 | DONE | `count.rs:105 correct_se`; test `correct_se_flag_table` |
| 19 | PE flag correction: 99/147→fwd, else→revcomp, NEVER die (`or 163` bug) | C4 | DONE | `count.rs:120 correct_pe`; test `correct_pe_replicates_or163_bug` |
| 20 | revcomp = reverse + `tr/GATC/CTAG/` (N/IUPAC untouched) | C4 | DONE | `count.rs:89 revcomp`; test `revcomp_maps_gatc_leaves_others` (incl. ARGY→YCRT IUPAC vector) |
| 21 | SE/PE detection via `detect_paired_from_header`; None→error | C5 | DONE | `count.rs:140-141`; verified `bismark_io::detect_paired_from_header` returns `Option<bool>`; `SePeUndetermined` error |
| 22 | Per-read driver: raw RecordBuf, field extract, skip InDel, span, correct, accumulate | C5 | DONE | `count.rs:133 count_reads_in_file` + `count_records`; 5 driver tests |
| 23 | Unmapped-read policy: keep SE die-on-unexpected-flag faithful | C4,C5 | DONE | `count.rs:178` (None pos → 0, flag still flows); test `count_records_se_stray_flag_errors` |
| 24 | Output filename: basename + `s/(bam\|cram)$/.../`; non-bam/cram → error | D1 | DONE | `output_name.rs:25`; 7 tests incl. no-dot quirk + a.bam.bam |
| 25 | Stats header line exact | D2 | DONE | `report.rs:34 HEADER`; test `header_exact` |
| 26 | Mono A,C,G,T then 16 di rows fixed order | D2 | DONE | `report.rs:38 MONO`/`41 DI`; tests `has_header_plus_4_mono_plus_16_di_lines`, `di_words_in_fixed_order` |
| 27 | Separate mono vs di totals; `%.2f` pct, `%.3f` coverage | D2 | DONE | `report.rs:69-87` (separate totals); tests `mono_row_a_exact`, `di_populated_rows_exact` |
| 28 | Missing sample count → empty field, 0 in math | D2 | DONE | `report.rs:128 cs_field`; test `di_empty_count_field_for_absent_word` (literal `\t\t`) |
| 29 | Division-by-zero → hard error | D2 | DONE | `report.rs:105-122 ZeroDivision`; tests `zero_sample_total_errors`, `zero_genomic_word_count_errors` |
| 30 | `%.2f`/`%.3f` round-half-to-even parity | D2,F1 | DONE | `report.rs` uses Rust `{:.2}`/`{:.3}`; test `format_rounding_is_round_half_to_even`; F1 harness confirms on oxy |
| 31 | Top-level flow incl. GCO compute+write+exit; per-file loop; reuse cache | E1 | DONE | `lib.rs:44 run()`; goldens `cache_*`, `two_input_files_each_get_stats_cache_reused` |
| 32 | Input format gate: accept `.bam`; reject `.sam`+`.cram` | E1 | DONE | `lib.rs:65-69`; tests `sam_input_is_rejected`, `cram_input_is_rejected` |
| 33 | mimalloc global allocator | A1 | DONE | `main.rs:20 #[global_allocator]`; `Cargo.toml` mimalloc `=0.1.52` |
| 34 | Local Perl-oracle goldens + `generate_goldens.sh` + byte-compare | E2 | DONE | `tests/data/generate_goldens.sh` + `tests/golden.rs` (12 cells); 8 committed goldens |
| 35 | oxy real-data byte-identity gate (SE+PE+GCO+reuse; LC_ALL=C) | F1 | DONE (harness) / PENDING (run) | `scripts/bam2nuc_byte_identity.sh` (fail-closed, LC_ALL=C, cells genome_comp/se/pe/sorted) + `tests/byte_identity_real_data.rs` `#[ignore]` |
| 36 | Docs: README + CHANGELOG + de-jargoned `--help` | F2 | DONE | `README.md`, `CHANGELOG.md`, `cli.rs` plain-language `///` doc/help text |
| 37 | Multiple input files in argv order, one stats file each | C5,E1 | DONE | `lib.rs:60 for infile in &config.inputs`; test `two_input_files_each_get_stats_cache_reused` |
| 38 | `noodles-bam`/`noodles-bgzf` runtime deps; direct `record_bufs` path | A1,C5 | DONE | `Cargo.toml` lines 41-42 (regular deps); `count.rs:137-142 noodles_bam::io::Reader::record_bufs` |
| 39 | `alignment_start()==None` policy (no silent skip; POS=0 unreachable) | C3,C5 | DONE | `count.rs:178 map_or(0, ...)`; comment on NonZero unreachability in `extract_span` docs; stray-flag test |
| 40 | Allocation-free `NucCounts` (`[u64;256]` + boxed di); count==0 ⇔ absent | B1,B2,D2 | DONE (DEVIATED-doc: D-impl-1) | `freqs.rs:32 NucCounts` (boxed `Box<[u64]>` di); `cache_bytes()` omits zeros (test `cache_bytes_omits_zero_words`) |
| 41 | Case-sensitive output-name strip (`.BAM`→Err) reconciled w/ E1 | D1 | DONE | `output_name.rs:33` (lowercase token match); test `case_sensitive_uppercase_rejected`; reconcile note in module doc |
| 42 | SE/PE-detection divergence (ID:Bismark-scoped @PG) covered/documented | C5,F1 | DONE | `count.rs:140` uses `detect_paired_from_header`; divergence documented in `count.rs` doc + `bam2nuc_byte_identity.sh` `sorted` cell |
| 43 | `--samtools_path` not validated (D1a); `*`-SEQ divergence (D7) documented | A2,F2 | DONE | `cli.rs` doc notes no existence check; D7 `*`-SEQ in `README.md`/SPEC §12 + `count.rs:179` comment |

## Coverage ledger — Phase A–F tasks

| Task | Goal | Status | Evidence |
|---|---|---|---|
| A1 | Scaffold + error skeleton + mimalloc + version | DONE | `Cargo.toml`, `main.rs`, `lib.rs`, `error.rs`; deps + pins match (noodles-bam 0.89.0, noodles-sam 0.85.0, noodles-bgzf 0.47.0 in lock) |
| A2 | CLI parser + validation/resolution | DONE | `cli.rs` `Cli`/`ResolvedConfig`/`validate`; 13 cli tests |
| A3 | Genome reader + `seqs()` | DONE | `genome.rs`; 16 tests; O-4 invariant comment removed (no stale "no-iterator" text) |
| B1 | `process_sequence` mono/di counter + `NucCounts` | DONE | `freqs.rs`; allocation-free array repr (di boxed) |
| B2 | `NucCounts` cache serialization | DONE | `cache_bytes()` (D-impl-1: returns `Vec<u8>` not `to_cache_string()` — documented) |
| B3 | Genomic composition + cache write w/ precedence | DONE | `compute_genomic`, `write_cache` |
| B4 | Cache read + reuse-if-present | DONE | `get_genomic_frequencies`, `read_cache`, `MalformedCacheLine` |
| C1 | chr-name table from header | DONE | `build_chr_name_table` (non-ASCII guard) |
| C2 | CIGAR `[IDSN]` skip predicate | DONE | `cigar_has_indel` |
| C3 | Span extraction w/ saturation | DONE | `extract_span` + NonZero policy comment |
| C4 | SE/PE flag correction + revcomp | DONE | `correct_se`/`correct_pe`/`revcomp` |
| C5 | Per-read driver loop | DONE | `count_reads_in_file` + factored `count_records` (D-impl factoring documented) |
| D1 | Output filename derivation | DONE | `output_name.rs` |
| D2 | Stats report writer | DONE | `report.rs` (D-impl-4: text-fence doc to satisfy clippy) |
| E1 | `run()` flow + format gate + main wiring | DONE | `lib.rs run()`; `SamNotSupported`/`CramNotSupported`; D-impl-2 added `BamIo` variant (documented) |
| E2 | Local goldens + `generate_goldens.sh` + byte-compare | DONE | 12 golden tests; D-impl-3 (de Bruijn genome) documented |
| F1 | oxy real-data gate harness | DONE (files) | `scripts/bam2nuc_byte_identity.sh` + `tests/byte_identity_real_data.rs`; RUN is PENDING manual |
| F2 | Docs | DONE | README + CHANGELOG + help text |

## Review-fold-in / mandated-cell audit

| Item | Required by | Status | Evidence |
|---|---|---|---|
| C-1 | `noodles-bam`/`noodles-bgzf` runtime deps | DONE | `Cargo.toml:41-42` regular deps; `count.rs` uses `noodles_bam::io::Reader::record_bufs` |
| C-2 | `alignment_start()==None` policy (no silent skip) | DONE | `count.rs:178`; stray-flag test errors faithfully via `correct_se` |
| C-3 | empty-count-field MANDATORY golden cell | DONE | `golden.rs di_*`/`se_stats`/`pe_stats` goldens contain literal `\t\t` (verified in `od -c` of `pe_stats.golden`, `se_stats.golden`); unit test `di_empty_count_field_for_absent_word` |
| I-2 | allocation-free counter | DONE | `NucCounts` `[u64;256]` + boxed di array |
| I-1 | case-sensitive output name | DONE | `case_sensitive_uppercase_rejected` |
| E2 #5 | non-canonical PE flag golden cell | DONE | `pe_noncanonical.bam` (flag 65) + `pe_noncanonical_stats.golden`; test `pe_noncanonical_flag_byte_identical` |
| E2 #7 | all-InDel → ZeroDivision exit 1 cell | DONE | `all_indel.bam`; test `all_indel_sample_zerodivision_exits_one` (asserts exit 1 + partial header) |
| C4 | IUPAC-cache + IUPAC-revcomp test vectors | DONE | `cache_iupac.golden` + `cache_iupac_byte_identical`; `revcomp(b"ARGY")→b"YCRT"` |
| E2 #8 | synthetic ×1000 cache-reuse cell | DONE | `genome_reuse/genomic_nucleotide_frequencies.txt` (×1000) + `reuse_stats.golden`; test `cache_reuse_uses_planted_genomic_column` |
| E2 #9 | two-file (per-file %freqs reset) cell | DONE | `two_input_files_each_get_stats_cache_reused` (asserts `se != pe`) |
| E2 #12 | empty-genome 0-byte cache cell | DONE | `genome_mus` + `cache_mus.golden` (0 bytes); test `cache_empty_genome_is_zero_bytes` |
| E2 #13 | content-BAM-named-`.sam` reject cell | DONE | `sam_input_is_rejected` (`.sam` content sniff → `SamNotSupported`) |
| F1 oxy gate | harness files exist (RUN is PENDING) | DONE (files) | `scripts/bam2nuc_byte_identity.sh` + `tests/byte_identity_real_data.rs` committed; RUN explicitly PENDING per brief |

## Documented deviations (acceptable)

All four PLAN "Implementation notes" deviations are present in code AND documented:

- **D-impl-1** — cache serializer is `NucCounts::cache_bytes() -> Vec<u8>` (raw bytes), not `to_cache_string()`. Present at `freqs.rs:77`; tests assert on `String::from_utf8` of the bytes. Documented in PLAN. ACCEPTABLE.
- **D-impl-2** — `error::BamIo(#[from] bismark_io::BismarkIoError)` variant added (not in PLAN's initial enum list). Present at `error.rs:20`; required by the `AlignmentKind::from_path` gate in `run()`. Documented. ACCEPTABLE.
- **D-impl-3** — golden GENOME must contain all 16 di-words (chr1 = de Bruijn `AACAGATCCGCTGGTTA`) because Perl itself dies on any absent di-word's coverage division. Present in `generate_goldens.sh:47-54` + `genome_acgtn/chr.fa`. Documented (matches Perl). ACCEPTABLE.
- **D-impl-4** — `report.rs` module-doc uses a ` ```text ` fence with literal `<TAB>` markers to satisfy clippy + avoid a doctest. Present at `report.rs:8-10`. Documented. ACCEPTABLE.

SPEC deviations D1–D7 are all reflected: D1/D1a (`cli.rs` no samtools validation), D2 (`version_string`), D3 (`correct_pe` `or 163`), D4 (`output_name` case-sensitive no-dot), D5 (stderr-only progress), D6 (`genome.rs` bare-header error + test), D7 (`*`-SEQ documented in README + `count.rs` comment).

## Gaps (detail)

None. No PARTIAL or MISSING items.

The single non-code item is the **oxy real-data gate RUN** (PLAN row 35 / Task F1 / "PENDING (manual, post-review)"). This is explicitly defined in the brief and the PLAN as a manual post-implementation step, NOT a code deliverable. The harness files required for it are present, committed, and statically sound:
- `scripts/bam2nuc_byte_identity.sh` — fail-closed, `LC_ALL=C`, cells: `genome_comp` (GCO), `se`, `pe`, optional `sorted` (the @PG-divergence cell). Diffs both `*.nucleotide_stats.txt` and `genomic_nucleotide_frequencies.txt`.
- `tests/byte_identity_real_data.rs` — env-driven `#[ignore]` smoke.

This is therefore NOT counted as a gap.

## Test verification (Mode B)

`cargo test -p bismark-bam2nuc` run from `/Users/fkrueger/Github/Bismark-bam2nuc/rust` (sandbox blocked `target/` writes; re-run with sandbox disabled). Result: **all green**, matching the PLAN's stated count.

| Suite | File | Count | Status |
|---|---|---|---|
| Unit tests (cli/error/genome/freqs/count/output_name/report) | `src/*.rs #[cfg(test)]` | 70 | PASS (0 failed) |
| Golden byte-identity + behavioral | `tests/golden.rs` | 12 | PASS (0 failed) |
| Sanity | `tests/sanity.rs` | 2 | PASS (0 failed) |
| Real-data smoke | `tests/byte_identity_real_data.rs` | 1 | IGNORED (needs oxy env vars) |
| Main bin unit | `src/main.rs` | 0 | PASS |
| Doctests | — | 0 | PASS |

Notable golden cells confirmed PASS: `cache_acgtn`, `cache_iupac` (IUPAC R/CR/RG rows), `cache_empty_genome_is_zero_bytes` (0-byte), `se_stats_and_cache`, `pe_stats`, `pe_noncanonical_flag` (the `or 163` bug end-to-end), `cache_reuse_uses_planted_genomic_column`, `two_input_files_each_get_stats_cache_reused`, `all_indel_sample_zerodivision_exits_one` (exit 1 + partial header), `sam_input_is_rejected`, `cram_input_is_rejected`, `missing_genome_folder_errors`.

`cargo clippy -p bismark-bam2nuc --all-targets -- -D warnings`: **clean** (no warnings).

## Verdict

**COMPLETE.** Every one of the 43 PLAN checklist rows and all 18 Phase A–F tasks are implemented in code and covered by a passing test. All 4 documented deviations (D-impl-1..4) are present and acceptable; all 7 SPEC deviations (D1–D7) are reflected. The mandated review-fold-in cells (C-1/C-2/C-3, allocation-free counter, case-sensitive output name, non-canonical-PE-flag, all-InDel ZeroDivision, IUPAC cache + revcomp vectors, cache-reuse ×1000, two-file reset, empty-genome 0-byte, content-BAM-`.sam` reject) all exist and pass.

The only outstanding work is the **oxy real-data gate RUN**, which the PLAN and brief explicitly mark as a manual, post-review step — the harness is built and committed, so this is not a code coverage gap. No action is required for plan coverage; the work is ready for the workflow's dual code-review step.

---

## Addendum — 2026-05-31: optional test-gap closure (PR #922 reopened)

The 4 optional robustness test gaps recorded in the session handoff (§2) are now **CLOSED**
per `TEST_GAPS_PLAN.md` (drafted → manual review → dual plan-review → implemented). None was
a byte-identity risk; all are additive tests over already-correct code.

| Gap | Cell | Evidence |
|---|---|---|
| 1 — `--version`/`-V` e2e | `golden.rs` `version_flag_{long,short}_prints_version_and_exits_zero` | binary spawn → stdout `bam2nuc_rs ` + OS, exit 0 |
| 2 — non-ASCII `@SQ` error | `count.rs` `build_chr_name_table_rejects_non_ascii_sq_name` | ASCII control → `Ok`; `chr\xff` → `NonAsciiChromosomeName` |
| 3 — `SePeUndetermined` e2e | `golden.rs` `non_bismark_pg_bam_is_se_pe_undetermined` | new `no_bismark_pg.bam` (bowtie2 `@PG`) → exit 1 + msg + no stats file |
| 4 — coord-sorted golden | `golden.rs` `se_sorted_stats_byte_identical` | new `se_sorted.bam` + Perl-oracle `se_sorted_stats.golden` (== `se_stats.golden`) |

**Updated test counts:** `cargo test -p bismark-bam2nuc` = **72 unit + 17 golden + 2 sanity**
(was 71/13/2), 1 `#[ignore]` real-data smoke; `clippy -p bismark-bam2nuc --all-targets -D
warnings` clean; `fmt` clean. New fixtures + golden minted by hand (NOT a full
`generate_goldens.sh` re-run) so the 8 existing goldens are byte-unchanged; the script edits
keep them reproducible from source.

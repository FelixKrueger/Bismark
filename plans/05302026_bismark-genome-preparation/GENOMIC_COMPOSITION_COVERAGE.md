# Plan Coverage Report — `--genomic_composition`

**Mode:** B (code vs. plan)
**Plan:** `GENOMIC_COMPOSITION_PLAN.md` (rev 1)
**Codebase:** worktree `/Users/fkrueger/Github/Bismark-genomeprep`, branch `rust/genomeprep-genomic-composition`
**Implementation crate:** `rust/bismark-genome-preparation/`
**Date:** 2026-05-31
**Verdict:** **COMPLETE** — every plan §1–§5 requirement and every §4 test maps to real code/tests; full suite green (72 passed / 0 failed; 3 real-data gates correctly `#[ignore]`).

## Summary

- Total items: 27
- DONE: 27
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 0

## Coverage ledger — requirements (§1–§3, §5)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | New module `src/composition.rs` with `write_genomic_composition` | §2 | DONE | `composition.rs:48` `pub fn write_genomic_composition(files, genome_folder, logger)` exact signature |
| 2 | Array-indexed counters, no per-base allocation (`[u64;256]` mono + flat `vec![u64;65536]` di) | §2 rev-1(g) | DONE | `composition.rs:56-57` `mono = [0u64;256]`, `di = vec![0u64; 256*256]`; di flat-indexed `p*256 + u` (`:174`) |
| 3 | NOT `[^ATCGN]→N`: ambiguity codes counted; only literal `N` skipped (mono); di skipped if either base is N | §1, §3 | DONE | `count_bytes` (`:161-178`): `if u != b'N' { mono+=1 }`; di guarded by `p != b'N' && u != b'N'`. No conversion-style mapping. Verified by `ambiguity_code_counted_not_mapped_to_n` |
| 4 | First line = header unconditionally; `NotFasta` if not `>`; header NOT counted | §1, §2, §5.4 | DONE | `count_file` (`:91-95`) reads first line, `n==0`→`NotFasta`, else `check_header`; loop body never counts header lines (`:106-110` `continue`) |
| 5 | Own dup-name check erroring BEFORE the table is written (no orphan file) | §2, §5.2 | DONE | `check_header` (`:120-132`) inserts into `seen` HashSet, returns `DuplicateChromosome`; `write_table` reached only after all files counted error-free (`:66-70`). `?` propagation in `count_file` short-circuits before `write_table` |
| 6 | `s/\r//` removes the FIRST `\r` only; `chomp` strips one trailing `\n` | §1, §2 | DONE | `count_sequence_line` (`:140-155`): drops single trailing `\n`, then `position(\r)` removes only first occurrence (counts `[..i]` then `[i+1..]`, carrying same `prev`) |
| 7 | `prev` (di-carry) reset per file AND at each header; spans line boundaries within a chromosome | §1, §3 | DONE | `prev` declared per-file (`:99`), reset to `None` at each in-file header (`:108`); carries across sequence lines (loop reuses it). No reset across sequence/blank lines |
| 8 | Byte sort (NOT `fasta_name_cmp`) via array-iteration emit order | §2 rev-1(f) | DONE | `write_counts` (`:213-227`) iterates leading byte `b` ascending, emits mono `[b]` then di `[b,c]` block ascending → plain byte-lexical, no case-fold. Doc comment explicitly contrasts `fasta_name_cmp` |
| 9 | Reuse `convert::open_fasta`, made `pub(crate)` | §2 rev-1(e) | DONE | `convert.rs:111` `pub(crate) fn open_fasta`; `composition.rs:26` imports + uses it (`:86`). No duplicated gz detection |
| 10 | `Mus_musculus.NCBIM37.fa` excluded from counting | §1, §3 | DONE | `SKIP_FILENAME` const (`:35`); `write_genomic_composition` skips on byte-match of `file_name()` (`:63-65`). Verified by `mus_musculus_file_excluded_from_counting` |
| 11 | gzip input handled (MultiGzDecoder via open_fasta) | §1, §3 | DONE | Inherited from `open_fasta` (gz-aware); `perl_vs_rust_gzip_input` integration test exercises `.fa.gz` |
| 12 | Non-fatal write; empty/N-only → 0-byte file | §1, §5.1, §5.3 | DONE | `write_table` (`:183-204`) logs `note` + returns on File::create or write/flush error (no error propagated). Empty counters → `write_counts` emits nothing → 0-byte file. Verified by `n_only_genome_is_zero_byte_file`, `header_only_record_is_zero_byte_file` |
| 13 | Wiring into `pipeline.rs` Step I.5 (after create_tree, before convert_split) | §2 | DONE | `pipeline.rs:42` `create_tree` → `:47-54` Step I.5 `if config.genomic_composition { write_genomic_composition(...)? }` → `:58` `convert_split`. Error propagation via `?` |
| 14 | Accept-and-ignore note removed; cli.rs doc updated | §2 | DONE | No "deferred/ignored/not yet" note remains for the flag (grep clean except unrelated `combined.rs`). `cli.rs:82-85` doc rewritten to describe real behavior |

## Coverage ledger — tests (§4)

| # | Plan §4 test scenario | Test function | File | Status |
|---|------|--------|------|--------|
| 15 | ACGT-only (mono + di present) | `acgt_only_mono_and_di` | composition.rs | DONE |
| 16 | N-skipping (mono N skipped, di-with-N skipped, di across N split) | `n_skipped_mono_and_di` | composition.rs | DONE |
| 17 | Ambiguity code (`R`) counted as mono + in di | `ambiguity_code_counted_not_mapped_to_n` | composition.rs | DONE |
| 18 | Di across a line boundary (multi-line record) | `di_spans_line_boundary_within_chromosome` | composition.rs | DONE |
| 19 | Di NOT across chromosomes (two records in one file) | `di_does_not_span_chromosomes` | composition.rs | DONE |
| 20 | Di NOT across files (`prev` reset per file) | `di_does_not_span_files` | composition.rs | DONE |
| 21 | Blank line mid-chromosome preserves `prev` | `blank_line_preserves_di_carry` | composition.rs | DONE |
| 22 | Exact sort order (mono-before-its-di; ambiguity interleaved by byte) | `acgt_only_mono_and_di` + `ambiguity_code_...` + `stray_space_counted_as_own_key` | composition.rs | DONE | Sort order asserted in every byte-exact table test; space (0x20<'A') and `\r` (0x0D) cases prove byte ordering across non-ACGT keys |
| 23 | Error: first line not `>` → `NotFasta` + no table | `first_line_not_header_errors_and_no_file` (+ `empty_file_errors_and_no_file`) | composition.rs | DONE |
| 24 | Bare `>` first line → empty name, not counted | `bare_gt_first_line_is_not_counted` | composition.rs | DONE |
| 25 | Duplicate chromosome name → `DuplicateChromosome` AND no table file | `duplicate_chromosome_errors_and_no_orphan_file` (+ `duplicate_across_files_errors`) | composition.rs | DONE |
| 26 | `s/\r//` first-`\r`-only (`A\r\rC` → second `\r` survives) | `carriage_return_first_only_removed` | composition.rs | DONE |
| 27 | Final line with no trailing `\n` counted fully | `final_line_without_newline_counted` | composition.rs | DONE |
| — | Perl-oracle integration comparing `genomic_nucleotide_frequencies.txt` | `perl_vs_rust_genomic_composition` | tests/integration.rs:447 | DONE | Auto-skips if `perl` absent (`oracle_compare` `:334-336`) |
| — | Addition to `byte_identity_real_data.rs` `#[ignore]` gate | `byte_identity_real_data_genomic_composition` | tests/byte_identity_real_data.rs:158 | DONE | `#[ignore]`, compares `genomic_nucleotide_frequencies.txt` |
| — | CLI no longer emits the deferred note; file produced | `binary_genomic_composition_freq_table_bytes` + `binary_no_genomic_composition_flag_writes_no_table` | tests/integration.rs:111,136 | DONE | Binary E2E asserts the table bytes and that the flag-off case writes no file |

**Bonus tests beyond §4 (no gaps, extra coverage):** `lowercase_is_uppercased`, `crlf_line_terminator_stripped`, `stray_space_counted_as_own_key`, `header_only_record_is_zero_byte_file`.

## Notable implementation detail (faithful, not a gap)

- Dup-name detection inserts into `seen` at **header-read time** rather than Perl's store-on-next-header time. The plan (§2 footnote, code doc `:116-119`) accepts this: it detects the same set of duplicates (any name appearing ≥2×) and matches `crate::convert`'s behavior. Confirmed by `duplicate_chromosome_errors_and_no_orphan_file` and `duplicate_across_files_errors`.
- `prev` advances for every byte including `N` (`count_bytes:176` unconditional `*prev = Some(u)`), so `N` correctly separates neighbours (the di across an `N` is dropped, but the bases on either side do not form a di — matches Perl `index($di,'N')<0` on the concatenated sequence). Verified by `n_skipped_mono_and_di`.

## Test verification

| Test binary | running | passed | failed | ignored |
|-------------|---------|--------|--------|---------|
| unittests src/lib.rs (incl. 20 composition.rs tests) | 59 | 59 | 0 | 0 |
| unittests src/main.rs | 0 | 0 | 0 | 0 |
| tests/byte_identity_real_data.rs (real-data gates) | 3 | 0 | 0 | 3 (`#[ignore]`, by design) |
| tests/integration.rs | 13 | 13 | 0 | 0 |
| Doc-tests | 0 | 0 | 0 | 0 |
| **Total** | **75** | **72** | **0** | **3** |

Command: `cd /Users/fkrueger/Github/Bismark-genomeprep/rust && cargo test -p bismark-genome-preparation` — all pass. The 3 ignored are the real-data byte-identity gates requiring `BISMARK_GENOMEPREP_REAL_GENOME_DIR` + Perl (run on oxy, not locally).

## Verdict

**COMPLETE.** Every requirement in plan §1–§3 and §5, every unit test in §4, the Perl-oracle integration test, the `#[ignore]` real-data gate addition, and the CLI behavior change are implemented and verified. No PARTIAL, MISSING, or DEVIATED items. No source code was modified during this audit.

# CODE_REVIEW_B — Phase 9a: FastA input (bismark aligner Rust port)

**Reviewer:** Code Reviewer B (independent, fresh context)
**Date:** 2026-06-02
**Scope:** FastA input (SE + PE, all 3 library types), byte-identical to Perl bismark v0.25.1 + Bowtie 2 2.5.5. Threading = Phase 9b (out of scope).
**Files:** `src/{convert,lib,aux_out}.rs` + `tests/cli.rs` (changes UNCOMMITTED on `rust/aligner` re-based onto `origin/rust/iron-chancellor` `7f7d77d`).

## Verdict

**APPROVE.** The implementation is faithful to the Perl source on every load-bearing path I independently traced. No Critical, High, or Medium issues. Two Low/informational observations only. No fixes applied (nothing warranted a change).

Gates (run locally, sandbox-disabled):
- `cargo test -p bismark-aligner`: **226 passed** (194 lib + 32 integration), 0 failed.
- `cargo test -p bismark-aligner --lib convert::`: 36 passed (27 FastQ frozen + 9 FastA).
- `cargo clippy -p bismark-aligner --all-targets -- -D warnings`: clean.
- `cargo fmt -p bismark-aligner -- --check`: clean.

---

## Priority-by-priority findings (traced to source, not the §13 notes)

### 1. FastQ byte-freeze + the separate-core deviation — VERIFIED

- `git diff -- src/convert.rs` has **zero removed lines** (`grep -c "^-[^-]"` = 0). `convert_fastq_impl` is **literally unmodified** — only the new FastA functions/tests are appended. The FastQ conversion byte-freeze is therefore structural, not just test-asserted.
- The 28 FastQ integration tests + 27 FastQ convert-unit tests pass unchanged (confirmed by the green suite + the convert-module subset).
- The `convert_fasta_impl`-vs-shared-`RecordShape` deviation (§13 dev #1) is **justified**: the 2-vs-4-line read/write, per-record-vs-record-1 sanity, and absent max-len guard would make a merged core more branches than shared code. Shared logic is the real reuse surface (`fix_id`, `convert_one`, `temp_dir_prefix`, `pe_id_suffix`, `file_base_for`). Leaving the FastQ core untouched is the lower-risk choice and I endorse it.
- **One FastQ path was structurally restructured** (not just `else`-branched): the PE re-read guard changed from `n_qual1 == 0` to `qual1.is_empty()` (lib.rs ~1037-1045). I verified this is **byte-exact**: buffers are `.clear()`ed at loop top, `read_until` appends, so `qual1.len() == n_qual1` and `qual1.is_empty() ⟺ n_qual1 == 0`. The two `+` lines remain unguarded (Perl 2611). FastQ behaviour preserved.

### 2. The `'I'×len` QUAL synthesis — VERIFIED at the correct layer

- Synthesized in the **driver re-read** (`drive_merge` SE + `drive_merge_pe` PE), NOT in the SAM writer — exactly mirroring Perl `check_results_single_end` 2707-2709 / `check_results_paired_end` 3271-3280. I read both Perl sites: `unless ($quality_value){ $quality_value = 'I'x(length$sequence); }` (SE) and the same default **per mate** (PE 3271/3276). Confirmed factual.
- SE: `vec![b'I'; seq_uc.len()]`; PE: per-mate `vec![b'I'; seq1_uc.len()]` / `seq2_uc.len()`. Length keys off the **uppercased, chomped** seq — same as Perl's `length$sequence` after `chomp`+`uc`.
- `'I'` (0x49) − 33 = **Phred 40**, asserted in the BAM as `&[40u8; 6]` in all three mapped integration tests (SE directional, SE non-dir CTOB, PE both mates).
- Minus-strand QUAL-reverse no-op: all bytes equal `'I'`, so reversal is invisible — the non-dir CTOB test (FLAG 16) asserts QUAL `[40;6]`, confirming the reverse path is exercised and byte-correct.

### 3. `convert_fasta_impl` correctness vs the FastQ core + Perl 5256–5288 — VERIFIED

Order traced against Perl 5245-5290: read 2 lines → `last unless (header and sequence)` → `chomp`+`fix_IDs`+`\n` → `++count` → skip(`next unless count>skip`) → upto(`last if count>upto`) → `uc` → tab-detect → **per-record `^>` die** → write `header`+`tr`. The Rust order matches (count++ before fix_id vs Perl fix_id before count++ is byte-neutral — fix_id is count-independent, same arrangement as the FastQ core).

- 2-line read, `break` on truncated tail (`n1==0 || n2==0`). ✓
- `>` prefix preserved on the converted header (it is NOT stripped at conversion time — Perl strips only in the re-read). ✓
- `.fa`/`.fa.gz` suffix (not `.fastq`). ✓ (`fasta_se_c_to_t_golden` asserts `reads.fa_C_to_T.fa`.)
- **Per-record `^>` sanity** (every non-skipped record), placed AFTER skip/upto so a skipped record is not checked — matching Perl's `next` (skip) preceding the 5267 die. ✓ This correctly DIFFERS from the FastQ record-1-only `@`/`+` check.
- **No max-length guard** (correct — Perl FastA has none; the mm2 guard is FastQ-only at 5598).
- PE gzip-off: `bisulfite_convert_fasta_pe_kind` clones opts with `gzip:false` (Perl 5311-5314 warns + writes uncompressed). SE FastA honours `--gzip` (Perl 5197-5205). Both pinned by `fasta_se_gzip_decompresses_to_plain` + `fasta_pe_gzip_forced_off`. ✓
- **No off-by-one vs the FastQ core**: `uc` is done inside `convert_one` (Perl does `uc$sequence` then `tr`); the seq's own trailing `\n` is preserved through `convert_one`; tab-detect runs on the post-fix_id header (dead, matching Perl's likewise-dead 5264). `GOLDEN_FA_IN` exercises the tab→`_` fix (`>read2\tlane2` → `>read2_lane2`) and lowercase→uc (`acgt`→`ACGT`/`ATGT`). ✓

### 4. 🔴 FastA-aware fakes + real-mapped assertions — VERIFIED (no Phase-8 false-pass)

All four fakes are `NR%2==1` + `sub(/^>/,"",id)` (the 2-line FastA shape), NOT the Phase-8 `NR%4==1`/`sub(/^@/)`:
- `make_fake_bowtie2_fasta_mapped` — `*BS_CT*` → flag-0 mapped, else flag-4.
- `make_fake_bowtie2_fasta_ga_index` — flag-0 only when `*BS_GA*` index AND `*_G_to_A*` reads.
- `make_fake_bowtie2_fasta_unmapped` — all flag-4.
- `make_fake_bowtie2_pe_fasta` — `NR%2==1`, `sub(/^>/)`, `sub(/\/1\/1$/,"",id)`, flags 99/147.

The three mapped tests assert FLAG (0/16/99/147), SEQ = the original read, QUAL = `&[40u8; 6]`, and a real XM — so they **cannot false-pass on all-unmapped** (a flag-4 record would fail `flags()==0` and `sequence()==ACGTAC`). This is the twin of the Phase-8 trap and is correctly avoided.

**Independent XM re-derivation (SE directional, `fasta_se_directional_mapped_phred40_qual`):** genome `chr1 = ACGTACGT`, read `ACGTAC` mapped OT (XG=CT) at chr1:1, 6M. OT methylation is called at genomic Cs: pos2 `C` (next=`G` → CpG, read base `C` = methylated → `Z`); pos6 `C` (next=`G` → CpG, methylated → `Z`); all others `.`. → **`.Z...Z`**, which exactly matches the asserted `XM`. The call depends on the read being genuinely mapped, confirming the test has real teeth.

### 5. Correctness details — VERIFIED

- **`>`-strip in the re-read**: SE `id_bytes = fixed.strip_prefix(b">")` (Perl 2342 `s/^>//`); PE `id_prefix = b">"` for both R1 and R2. Perl PE strips `>` only on `$identifier_1`, but writes `$orig_identifier_2` (fix_IDs'd, still carrying `>`) to the R2 aux; Rust strips R2's `>` then re-prepends a fresh `>` in `write_fasta_record` → net `>r1`, byte-identical to Perl. Verified against Perl 2543/2553-2566.
- **2-line FastA aux**: `write_fasta_record` = `>` + id + `\n` + seq + `\n` (Perl 2454-2455 / 2461-2464); seq is the chomped, **non-uppercased** original. `fasta_se_unmapped_writes_2line_fa_aux` decompresses to exactly `>r1\nACGTAC\n`. ✓
- **`strip_fastq_suffix` left FastQ-only**: the FastA BAM keeps `.fa` in the stem (`reads.fa_bismark_bt2.bam`), matching Perl 1622 (which does not strip `.fa`). ✓ Asserted in the SE test.
- **The genome-dir-globs-`.fa`-reads artifact** is **test-only, not a production concern**: `discover_fastas(&genome_dir)` (discovery.rs) scans only the `--genome` directory; read files are passed positionally/`-1`/`-2` and are never globbed as references unless physically placed inside the genome folder. The mitigation (separate `reads_dir` TempDir) matches real-world layout. No production risk.
- **`--pbat -f` dies**: config.rs `resolve_library` returns the Perl-8155-exact message (`"The option --pbat is currently only working with FastQ files. Please respecify (i.e. lose the option -f)!"`). The `fasta_se_nondir_…` test confirms non-directional is the FastA complementary-strand path (dev #2). ✓
- **The `_=>` dispatch removal**: `pipeline()` now matches `&config.layout` (SingleEnd | PairedEnd) — **still exhaustive** (ReadLayout has exactly those two variants; `ReadFormat` no longer participates, so FastA routes through the same arms). `--multicore` is still surfaced as deferred via `deferred_flags()` in `run()` (independent of the removed arm), so removing `_=>` did NOT drop the deferral notice. Verified.

### 6. Structure / duplication / naming / missed paths — VERIFIED

- `convert_se_ct`/`convert_se_ga`/`convert_pe_kind` + `write_se_aux_record` are thin format-dispatch shims; naming is clear and consistent with the existing crate style. The `fasta` bool is plumbed via `matches!(config.format, ReadFormat::FastA)` at each driver entry — local, no global state.
- **`.fasta` extension**: handled identically to `.fa` — the converted suffix is hardcoded `.fa`/`.fa.gz` on the un-stripped basename (Perl 5203-5205 likewise), and gz-detection keys off `.gz` (covers `.fasta.gz`). Aux filename uses the un-stripped basename + `_unmapped_reads.fa.gz` (Perl 1644-1709). No gap.
- **Multi-file SE**: `run_se` loops `for read_file in reads`, format-dispatching per file — multi-file SE FastA works.
- **Report wording**: `report.rs` has no FASTQ/FASTA-conditional text; Perl `$sequence_file_format` only selects the methylation-call function (1737/1955), not any report-header bytes (verified the SE report function has no format branch). The format-agnostic report is correct — no changes needed.

---

## Recommendations

### Critical / High / Medium
None.

### Low (informational — no action required before the oxy gate)
- **L1 — PE FastA `--gzip` warning not surfaced to the user.** Perl `biTransformFastAFiles_paired_end` emits `"GZIP compression of temporary files is not supported for paired-end FastA data. Continuing to write uncompressed files"` (5311-5314) before forcing gzip off. The Rust `bisulfite_convert_fasta_pe_kind` silently sets `gzip:false`. The byte output (uncompressed `.fa`) is identical, and the temp file is intermediate (never gated), so this is **byte-invisible** — but a user who passed `--gzip` for PE FastA gets no notice. Cosmetic only; matches the gate's decompressed-content policy. Consider an `eprintln!` mirroring Perl if stderr fidelity is ever in scope (it is not for this gate).
- **L2 — `fasta_skip_and_upto` count comment.** The test asserts `cr.count == 5` for `skip=2, upto=4` over 5 records. This is correct (record 5 is read, count→5, then `last if 5>4`), and matches Perl's `$count` semantics, but the inline comment "upto breaks at 5" could be read as ambiguous. No behavioural issue.

---

## Summary

Phase 9a is a clean, source-faithful addition. The FastQ byte-freeze is structural (zero edits to `convert_fastq_impl`; the one PE-guard restructure is provably byte-exact). The `'I'×len` Phred-40 QUAL lands at the driver layer for SE and per-mate PE, matching Perl 2707/3271. The per-record `^>` sanity, PE-gzip-off, `.fa`/`.fa.gz` suffixes, 2-line aux, and `--pbat -f` die all trace correctly. The FastA-aware fakes (`NR%2==1`/`sub(/^>/)`) plus real-mapped FLAG/SEQ/QUAL/XM assertions close the Phase-8 false-pass trap, and I independently re-derived the SE-directional XM (`.Z...Z`). All 226 tests pass; clippy and fmt clean. **APPROVE** — ready for the oxy byte-identity gate.

**Report path:** `/Users/fkrueger/Github/Bismark-aligner/plans/05312026_bismark-aligner/phase9a-fasta/CODE_REVIEW_B.md`

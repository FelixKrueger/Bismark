# Code Review A — Phase 9a (FastA input) — Bismark aligner Rust port

**Reviewer:** Code Reviewer A (independent, fresh context)
**Scope:** Phase 9a = FastA input (SE + PE, all 3 library types), byte-identical to Perl bismark v0.25.1 + Bowtie 2 2.5.5. Threading = Phase 9b (out of scope).
**Base:** `rust/aligner` re-based onto `origin/rust/iron-chancellor` `7f7d77d`; changes are UNCOMMITTED working-tree edits.
**Files reviewed:** `src/convert.rs`, `src/lib.rs`, `src/aux_out.rs`, `tests/cli.rs` (diff); cross-checked `src/config.rs`, `src/output.rs`, `src/report.rs`, `src/discovery.rs`, `src/genome.rs`.

## Verdict: **APPROVE**

The implementation is faithful to Perl v0.25.1 on every load-bearing path I traced. Build is green: **226 tests pass** (194 lib + 32 integration), `clippy --all-targets -D warnings` clean, `cargo fmt --check` clean. No fixes applied — nothing required them. Findings below are all Low / informational.

---

## Verification performed

- `cargo test -p bismark-aligner` → 32 integration pass; `--lib` → 194 pass.
- `cargo clippy -p bismark-aligner --all-targets -- -D warnings` → clean.
- `cargo fmt -p bismark-aligner -- --check` → clean (exit 0).
- Traced every claim to the Perl source (not the §13 notes), citing line numbers.

---

## Issues by area

### 1. FastQ byte-freeze (Priority: confirmed clean)

The format branching does **not** alter any FastQ path:

- `convert_fastq_impl` is **byte-for-byte unmodified** (the diff adds a *separate* `convert_fasta_impl` below it). The 27 FastQ convert unit tests + the existing goldens are untouched and pass. This is the strongest possible freeze guarantee — by construction.
- `convert_se_files` / `convert_pe_kind` dispatch: when `fasta == false` they call the exact same `bisulfite_convert_fastq_*` functions as before. The `convert_se_ct`/`convert_se_ga`/`convert_pe_kind` helpers are thin `if fasta {…} else {…}` wrappers whose `else` arm is the prior call verbatim.
- `drive_merge` / `drive_merge_pe`: the FastQ branch reads 4 lines and computes `qual_bytes` from the chomped quality line exactly as before. I verified the PE `qual1.is_empty()` substitution for the old `n_qual1 == 0` is **byte-neutral**: `read_until` returns 0 only at EOF-before-any-byte, and the buffer is cleared at the top of the loop, so `n_qual == 0 ⟺ qual.is_empty()`. A line that is just `\n` yields `n==1`, buffer `[b'\n']` (non-empty) — identical truthiness to Perl's `$quality_value` (`"\n"` is truthy). ✓
- **28 FastQ integration tests + all FastQ unit tests pass** = the regression guard (plan §9 #9). ✓

### 2. The `'I'×len` synthesized QUAL (Priority: confirmed clean)

- **SE:** `drive_merge` synthesizes `qual_bytes = vec![b'I'; seq_uc.len()]` where `seq_uc` is the chomped+uppercased read. Matches Perl `check_results_single_end` 2707–2709 `'I'x(length$sequence)` — `$sequence` there is `uc$sequence` (2361), and `uc` does not change length, so `seq_uc.len()` is the correct length. ✓
- **PE:** `drive_merge_pe` synthesizes `qual{1,2}_bytes` **per mate** from `seq{1,2}_uc.len()`. Matches Perl `check_results_paired_end` 3271/3274–3280 — two independent `'I'x(length$sequence_N)` defaults, one per mate. ✓
- **Flow to Phred 40:** `output.rs:383/608` does `q.wrapping_sub(offset)` with `offset = 33` (phred33; `-f ⊕ --phred64` dies at config so FastA is always phred33). `b'I'` (73) − 33 = **40**. ✓
- **Minus-strand reverse no-op:** `output.rs:392/616` `scores.reverse()` on a uniform all-40 vector is a no-op — verified against the non-dir CTOB integration test (FLAG 16, minus strand) which asserts `&[40u8; 6]`. ✓
- The four integration tests byte-assert `quality_scores() == &[40u8; 6]` for SE-directional, SE-non-dir CTOB, and **both** PE mates. These cannot false-pass on all-unmapped (each also asserts `unique best: 1`). ✓

### 3. `convert_fasta_impl` fidelity (Priority: confirmed clean)

Traced against Perl `biTransformFastAFiles` 5169–5306 / `_paired_end` 5308+:

- **2-line read** (`id`, `seq`), break if either is empty → Perl 5248 `last unless ($header and $sequence)`. ✓
- **`.fa`/`.fa.gz` suffix** (not `.fastq`) → Perl 5199–5204. ✓
- **`>` prefix preserved** in output (only the prefix-`>` SANITY is checked; the byte is not stripped) → Perl prints `$header` verbatim (5276/5281/5286). ✓
- **Per-record `^>` sanity** on every non-skipped record, NOT record-1-only → Perl 5271 / 5414 (no `if ($count==1)` guard). I confirmed the FastA core does **not** copy FastQ's `count == 1 && …` pattern. The negative test `fasta_per_record_sanity_record2_dies` proves a malformed record-2 dies under FastA but the same record-2 passes under FastQ. ✓ (This is the rev1 A/B correction, correctly implemented.)
- **No max-length guard, no `+`/qual line** → matches Perl (FastA has no `+`, and the mm2 max-len guard is only in the FastQ sub). ✓
- **Skip/upto with falsy-0**, sanity sits *after* skip → matches Perl ordering (5256–5261 `next` before 5271). The `fasta_skip_and_upto` test confirms. ✓
- **PE gzip forced off** (`bisulfite_convert_fasta_pe_kind` overrides `gzip: false`) → Perl 5311–5314 warns and writes uncompressed; SE FastA honors `--gzip` (Perl 5198–5205). The `fasta_pe_gzip_forced_off` + `fasta_se_gzip_decompresses_to_plain` tests pin both. ✓
- **`/1/1`,`/2/2` tag** inserted via `pe_id_suffix` before the trailing `\n`. I verified this matches Perl 5416–5421 `s/$/\/1\/1/` (Perl `$` without `/m` inserts before the trailing `\n`), and that Perl appends the tag *after* the `^>` sanity check (5414 before 5416) — the Rust core checks sanity at line 516 *after* `extend_from_slice(id_suffix)` at 491. **Behaviorally equivalent** (see Low-1 below for the only nuance). ✓

**Deviation (separate core vs shared `RecordShape`):** sound. Leaving `convert_fastq_impl` literally unmodified is the safest possible FastQ freeze; shared helpers (`fix_id`, `convert_one`, `temp_dir_prefix`, `pe_id_suffix`, `file_base_for`) are reused, so duplication is limited to the loop skeleton. Endorsed.

### 4. FastA-aware fakes (the load-bearing plan-review C-1) — confirmed clean

This was the dominant test risk, and it is correctly addressed:

- All four new fakes (`make_fake_bowtie2_fasta_mapped`, `_fasta_ga_index`, `_fasta_unmapped`, `make_fake_bowtie2_pe_fasta`) use **`NR%2==1`** and **`sub(/^>/,"",id)`** — NOT the Phase-8 `NR%4==1`/`sub(/^@/)`. I diffed each awk block. ✓
- Each mapped test asserts `unique best alignments:   1` **and** `quality_scores() == [40;6]` — so they genuinely exercise the mapped path; a fake mis-parse would yield all-unmapped (efficiency 0) and fail the `unique best: 1` assertion. No false-pass possible. ✓
- The GA-index fake gates the hit on `*BS_GA*` **and** `*_G_to_A*` input, mirroring the Phase-8 strand-variant trap. ✓
- The PE fake strips both `^>` and `/1/1$` before emitting mates. ✓

**Independent XM sanity check:**
- `fasta_se_directional` (genome `ACGTACGT`, read `ACGTAC` @ chr1:1, OT): C@2 is followed by G@3 → CpG `Z`; C@6 is followed by genomic G@7 → CpG `Z`; no other C → XM = `.Z...Z`. **Matches.** ✓
- `fasta_se_nondir_ga_index` (CTOB, XM `H.Z...`): this is computed by the *real* `methylation.rs`/`output.rs` (the fake only emits the SAM line; the Rust code does the call), so the assertion is validated by the production code path, not hand-asserted. The `Z` at position 3 (CpG) on the complementary strand is consistent. ✓

### 5. Edge / correctness — confirmed clean

- **`>`-strip in the re-read:** `drive_merge`/`drive_merge_pe` use `id_prefix = if fasta { b">" } else { b"@" }` and `strip_prefix(id_prefix)`. Matches Perl SE 2359 `s/^>//` and PE 2539 `s/^>//`. ✓
- **PE aux byte-identity:** Perl writes `$orig_identifier_N` (captured *before* the `s/^>//`, so it retains `>`) for both mates (2549–2560). The Rust strips `>` into `identifier`/`id2_stripped`, then `write_fasta_record` re-prepends `>`. Net bytes identical. I confirmed the FastQ PE aux does the same `@`-round-trip (2640 strips only `identifier_1`; both `$orig_identifier_*` retain `@`; Rust strips+re-prepends `@`) — so the FastA path mirrors the established FastQ contract. ✓
- **FastA aux 2-line `>id\nseq`:** `write_fasta_record` writes `>` + id + `\n` + seq + `\n`, no `+`/qual. Matches Perl 2369–2376 / 2454–2466. The `fasta_se_unmapped_writes_2line_fa_aux` test decompresses the `.fa.gz` and asserts exactly `">r1\nACGTAC\n"`. ✓
- **`strip_fastq_suffix` NOT extended:** strips only `.fastq.gz|.fq.gz|.fastq|.fq` — verified against Perl 1622 (which lists the identical four). `reads.fa` keeps its name → BAM `reads.fa_bismark_bt2.bam` (test asserts). ✓
- **`--pbat ⊕ -f` dies:** `config.rs:294–300` returns the Validation error with wording byte-identical to Perl 8156. ✓
- **Report header wording:** `report.rs` has **zero** format references; Perl `print_final_analysis_report_single_end` (1964–) prints `"Final Alignment report"` + numeric counts only, with no format-dependent text. The header `"Bismark report for: {sequence_file}"` reflects the actual input filename (`.fa`) and the `aligner_options` embed `-f` — both correct (Perl does the same). ✓
- **Multi-file SE FastA:** `run_se` loops over `reads` calling the format-dispatched `convert_se_files` per file — handled. ✓
- **`.fasta` extension:** `aux_filename`'s `fasta` flag only flips `fq`→`fa` (not `fasta`); but Perl's converted/aux suffix is hard-coded `.fa` regardless of input extension (5199–5204 append `_C_to_T.fa` to the *full* basename), so a `.fasta` input yields `reads.fasta_C_to_T.fa` / `reads.fasta_unmapped_reads.fa.gz` — consistent with Perl. ✓

### 6. Structure / naming — clean

`write_se_aux_record` is a reasonable shared dispatcher reused by both SE and PE aux writers (the `plus`/`qual` args are simply ignored for FastA). `convert_se_ct`/`_ga`/`convert_pe_kind` are clear. `#[allow(clippy::too_many_arguments)]` on `write_pe_aux` is justified and pre-existing. Module docs updated.

---

## Recommendations

### Low-1 (informational — no action needed): PE FastA header-error message numbering
In Perl PE FastA the `^>` sanity die (5414) fires *before* the `/1/1` tag is appended; in the Rust core the tag is appended (line 491) *before* the sanity check (line 516). This is behaviorally identical for the **pass** case and for the **die** case (a non-`>` header still fails `starts_with(b">")` regardless of an appended `/1/1` — the tag goes at the *end*, before `\n`). The only observable difference would be in the *error message text* if Perl ever interpolated the mutated header — it does not (the die message contains only `$count`, not `$header`). No divergence. Noted for completeness.

### Low-2 (informational): reads-inside-genome-dir is a faithful Perl trait, not a regression
The iteration log notes a test fixture had to move `.fa` reads out of the genome dir because `discover_fastas` globs `*.fa` there. I confirmed this is **identical to Perl** — `read_genome_into_memory` / the Perl `<*.fa>` glob would equally pick up any `.fa` in the index dir. Real users never place reads in the Bowtie2 index directory (it holds the reference + `.bt2` indexes), so this is a **test-only artifact**, correctly handled by using a separate `reads_dir` TempDir. No production concern; no fix warranted.

### Low-3 (optional, future): SE multi-file FastA + non-dir/pbat integration coverage
The integration suite covers SE-directional, SE-non-dir-CTOB, PE-directional, and the unmapped aux for FastA. SE multi-file and PE non-directional FastA are exercised only at the unit/dispatch level, not end-to-end. The oxy gate (§9 #10: FastA-converted subsets across all 3 libraries × SE/PE) will close this; no pre-gate change needed.

---

## Summary

Phase 9a is a clean, well-traced FastA addition. The two rev1 load-bearing risks — (a) the per-record `^>` sanity vs FastQ's record-1-only, and (b) the FastA-aware fakes (`NR%2`/`^>`) preventing all-unmapped false-passes — are both correctly implemented and pinned by tests. The `'I'×len` Phred-40 QUAL is synthesized at the driver per Perl 2707/3271, flows to Phred 40 with a no-op minus-strand reverse, and is byte-asserted for SE + both PE mates. FastQ output is frozen by construction (`convert_fastq_impl` unmodified; 28 FastQ integration + all FastQ unit tests green). Build/clippy/fmt all clean.

**Verdict: APPROVE.** No blocking findings; all findings are Low/informational. The remaining gap (full-scale FastA real-data byte-identity) is the explicitly-planned Phase-10 oxy gate.

**Report path:** `/Users/fkrueger/Github/Bismark-aligner/plans/05312026_bismark-aligner/phase9a-fasta/CODE_REVIEW_A.md`

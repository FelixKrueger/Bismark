# CODE REVIEW A — Phase 2a: HISAT2 wrapper core (SE byte-identical)

- **Reviewer:** A (independent)
- **Date:** 2026-06-05
- **Scope:** uncommitted working-tree changes to `rust/bismark-aligner` on `rust/aligner-v1x` (base `fc38191`), against the plan `phase2a-hisat2-core/PLAN.md` (rev 1 + Implementation Notes) and the Perl oracle `bismark` (v0.25.1).
- **Files reviewed:** `src/{config,aligner,options,discovery,error,report,lib,parallel,merge,methylation}.rs`, `tests/cli.rs`. `align.rs` confirmed **unchanged** (no diff).

## Verdict

**APPROVE.** The change is a faithful, well-disciplined generalization of the Bowtie 2-only wrapper to a two-aligner wrapper. Every byte-identity-critical claim in the plan was verified against the Perl oracle at the cited line numbers and holds. Bowtie 2 is structurally byte-frozen. No Critical or High findings. The remaining SE oxy gate (V9) is the correct next step before commit; the PE path is correctly excised to 2b.

- `cargo test -p bismark-aligner` — **271 green** (228 lib + 43 integration), 0 failed.
- `cargo clippy -p bismark-aligner --all-targets -- -D warnings` — clean (verified with a forced rebuild, not just cache).
- `cargo fmt -p bismark-aligner -- --check` — clean (CI gates this separately).

## Verification of the focus areas

### 1. Bowtie 2 byte-frozen — VERIFIED
- **Option string:** the HISAT2 delta is appended to the *finished* joined string in `apply_aligner_specific_options(opts.join(" "), …)` (options.rs); for `aligner != Hisat2` the function returns `base` unchanged (modulo the splice-flag dies, which only fire on flags that are themselves rejected). The push loop is untouched. `bowtie2_pe_string_byte_frozen_with_aligner_param` pins the Bowtie 2 PE string incl. `--dovetail`.
- **`--dovetail` gating:** changed from `if !cli.no_dovetail` to `if aligner == Aligner::Bowtie2 && !cli.no_dovetail` — matches Perl 8051-8059 (`if($bowtie2)`), and `--no-mixed`/`--no-discordant` remain unconditional (Perl 8044-8045, `#if($bowtie2)` commented out). Confirmed against the oracle.
- **Naming:** the token enters ONLY the `default_suffix` arg of `derive_output_path` and the multicore temp names. `basename_suffix` (the `-B` path), `_unmapped.tmp`, `_ambiguous.tmp` carry no token (parallel.rs:413/416; lib.rs:544-547). `Aligner::Bowtie2.token() == "bt2"` reproduces every prior literal: `_bismark_bt2.bam`, `_bismark_bt2_SE_report.txt`, `_bismark_bt2.ambig.bam`, `_bismark_bt2_pe.bam`, `_bismark_bt2_PE_report.txt`, `_bismark_bt2_pe.ambig.bam`. No `_bismark_bt2` string literal survives in the derived-name path (grep: only one comment at lib.rs:327).

### 2. HISAT2 option assembly — VERIFIED against Perl 8286-8326 / 8044-8059
- Default SE = `-q --score-min L,0,-0.2 --ignore-quals --no-softclip --omit-sec-seq` (`hisat2_se_option_string`).
- Default PE = `… --no-mixed --no-discordant --maxins 500 --no-softclip --omit-sec-seq`, **no `--dovetail`** (`hisat2_pe_option_string_has_no_dovetail`, asserts `!contains("--dovetail")`).
- Softclip delta lands **last** as a single token `"--no-softclip --omit-sec-seq"` — matches Perl's single `push '--no-softclip --omit-sec-seq'` at 8314.
- Splice flags `[--no-spliced-alignment][--known-splicesite-infile <f>]` are emitted **before** the softclip delta (Perl 8289-8307) — `hisat2_nosplice_appends_before_softclip` / `hisat2_known_splices_appends` pin the order.
- Die conditions all match the oracle: both-splice-set die (Perl 8290 → `hisat2_both_splice_flags_die`), missing-infile die (Perl 8304 → `hisat2_known_splices_missing_file_dies`), non-HISAT2 splice die (Perl 8319-8324 → `non_hisat2_splice_flags_die`, for both `--no-spliced-alignment` and `--known-splicesite-infile`).

### 3. Index discovery arity — VERIFIED against Perl 7646-7800
- `index_suffixes(Hisat2,…)` = `(1..=8).map(|n| "{stem}.{n}.ht2")` → exactly the Perl `@CT_hisat2_index`/`@GA_hisat2_index` 8-element lists (7739/7750), no `rev.*`. `.ht2l` large fallback (7769/7779). Bowtie 2 stays at the 6 `{1,2,3,4,rev.1,rev.2}.bt2`.
- Small→large fallback preserved (the `(None,None) => false` else-arm re-checks `large=true`). `incomplete_ht2_index_errors_with_hisat2_wording` correctly asserts the fallback contract (removing a *small* `.ht2` → the error names the first missing *large* `.ht2l`, not the small file) — this is the right semantics, matching the iteration-log note #2.
- `bt2_index_rejected_in_hisat2_mode` and `six_ht2_files_is_not_a_complete_hisat2_index` guard against an arity/extension mix-up.

### 4. Naming token through parallel.rs (`--multicore`) — VERIFIED
- All 10 multicore sites tokenized (se_chunk_job 407/410, pe_chunk_job 460/463, run_se_multicore 689/701/736, run_pe_multicore 839/851/901) plus both `ReportHeader` constructions get `aligner: cfg.aligner`. With `bt2` this is byte-identical to the prior literals; with `hisat2` the multicore SE path will name correctly. The V9 `--multicore` SE gate cell will exercise this end-to-end.

### 5. `--ambig_bam` + HISAT2 (OQ-2d) — VERIFIED against Perl 1583-1586 / 650-711
- Single-core supported: the token threads `_bismark_hisat2.ambig.bam` by construction (lib.rs:486). Perl 1575-1586 reaches this via the generic `$outfile` (which already carries `_bismark_hisat2.sam` → `s/sam$/ambig.bam/`). `ambig_bam_single_core_hisat2_names_hisat2_token` confirms.
- Multicore+HISAT2+`--ambig_bam` hard-rejected (config.rs:212). I confirmed the Perl multicore temp-name builder (650-711) populates `@temp_ambig_bam` ONLY in the `if($bowtie2)` branch; the `else{ # HISAT2 }` branch pushes only `@temp_output`+`@temp_reports`, so Perl silently drops the ambiguous BAM. Failing loudly is the honest choice. The reject fires *before* `discover_genome`/`detect_aligner`, so the test (no fake binary) reaches it. `ambig_bam_with_multicore_hisat2_is_rejected` confirms exit 1 + message.

### 6. Report wording — VERIFIED against Perl 1722/1728 (SE) and 1846/1849 (PE)
- `write_report_header` uses `h.aligner.name()` → "Bowtie 2" / "HISAT2". Perl's `else` branch (1728 SE / 1849 PE) emits "HISAT2"; `Aligner::name()` returns exactly "HISAT2". `header_hisat2_run_with_line` pins the full SE line incl. the echoed option delta.

### 7. PE read-1 `ZS` asymmetry NOT touched here — VERIFIED (and a real 2b hazard confirmed)
- `merge.rs` and `methylation.rs` diffs are **test-only additions** (diff-stat: pure insertions, all inside `mod tests`). `align.rs` has **zero** diff. So the merge second-best selection path is byte-frozen by this change.
- I traced the live asymmetry in the oracle to confirm the deferral is sound *and* that the hazard is real for 2b: Perl SE parses `ZS` unconditionally (2780); Perl PE **read-1** parses ONLY `XS` (3376, no `ZS` branch) while **read-2** parses `XS`-or-`ZS` (3393-3401). The Rust `SamRecord::parse` (align.rs:100-104) captures `XS`-or-`ZS` for *every* record, both mates. For **SE this is byte-faithful** (each aligner emits only its own tag, and Perl SE captures `ZS` anyway). For **PE-HISAT2 it is a latent divergence** (Rust read-1 would capture a `ZS` second_best that Perl read-1 ignores) — exactly what 2b must address. 2a does not introduce or worsen this (the parse predates 2a and is unchanged), and 2a is SE-only-gated, so the deferral is correct. **Flag for 2b, not a 2a defect.** (See Low-1.)

### 8. V5 (ZS→MAPQ) and V6 (spliced-N) test correctness — VERIFIED
- **V5 tie** (`hisat2_se_zs_equal_as_is_ambiguous`): one mapped record with `ZS==AS`, GA unmapped → single entry → Perl 3033-3044 stores the record's own `second_best`; with `AS==ZS` the read is booted as ambiguous. Matches `merge.rs` single-entry path → Decision::Ambiguous, `unsuitable_sequence_count==1`. Correct.
- **V5 shift** (`hisat2_se_zs_below_as_is_unique_best_with_zs_second`): single entry, `AS:i:0`/`ZS:i:-6` → Perl 3041 stores the record's own `alignment_score_second_best`; Rust merge.rs:316 sets `second_for_mapq = b.second_best = Some(-6)`. Correct. (The multi-entry 3075 arm — best's-own iff `> runner_up`, else runner-up — is mirrored at merge.rs:330-333 and covered by the pre-existing `second_best_uses_best_own_when_greater_than_runner_up`.)
- **V6 spliced-N:** I re-derived all four expectations against Perl 4327-4377 (M extracts substr; N advances `$pos` only, no seq, no `$indels`; D adds to `genomic_seq_for_MD_tag` when `contains_deletion`, `$pos += len`, `$indels += len`) plus the index-0 `+2` append (4388) and index-3 `-2` prepend (4314-4322):
  - `extract_spliced_n_skips_intron_index0` → `ACG`+`TAC`+`GT` = `ACGTACGT`. ✔
  - `extract_multi_n_spliced_index0` → `AC`+`TT`+`AC`+`GT` = `ACTTACGT`. ✔
  - `extract_n_and_deletion_counts_d_only_index0` → md_seq `ACG`+`TT`+`T`(D)+`AC` = `ACGTTTAC`, `indels==1` (D only, N excluded). ✔
  - `extract_spliced_n_on_ga_strand_index3` → prepend `AC`, then `GTT`+`CGT` (no append for index 3, no revcomp) = `ACGTTCGT`, `indels==0`, strand `+`, `ga_ga_count==1`. ✔

## Issues by area

### Logic / correctness
- No defects found. Enum dispatch is exhaustive (two variants, `match` arms complete). The OQ-2d reject ordering, the splice-flag dies, and the dovetail gating all match the oracle.

### Efficiency
- Negligible. One extra `String` concat per run (option tail), a `Vec<String>` suffix list instead of a fixed array (8 short allocs at startup, off any hot path). Acceptable.

### Errors / edge cases
- `AlignerError::AlignerNotWorking` now carries `path_flag` and the message reads "specify the path with --path_to_hisat2 /path/to/dir" for HISAT2 — STDERR, non-gated, fidelity-correct.
- `FaultyIndex` is aligner-named — STDERR, non-gated. Correct.

### Structure / style
- Clean: small private helpers (`binary_name`/`pinned_version`/`path_flag` in aligner.rs; `index_suffixes` in discovery.rs; `aligner_token`/`name` on the enum). Doc comments cite Perl line numbers. fmt/clippy clean.

## Recommendations (by priority)

- **Critical:** none.
- **High:** none.
- **Medium:** none.
- **Low-1 (informational, for Phase 2b — NOT a 2a fix):** The PE read-1 `ZS` asymmetry is genuinely latent in `SamRecord::parse` (captures `ZS` for both mates, whereas Perl PE read-1 ignores `ZS` — oracle 3376 vs 3393-3401). 2a correctly does not touch this and is SE-only-gated, but 2b must reproduce the asymmetry (read-1: `XS` only; read-2: `XS`-or-`ZS`) to be PE-byte-identical. The shared `parse` will need a per-mate/per-aligner notion, or the merge will need to discard read-1's `ZS`. Recommend 2b's plan explicitly pin a fake/gate cell where HISAT2 PE read-1 emits `ZS` and read-2 emits `ZS`, asserting only read-2's feeds the MAPQ second-best.
- **Low-2 (cosmetic, optional):** `tests/cli.rs::ambig_bam_single_core_hisat2_names_hisat2_token` asserts only that the ambig BAM file *exists* (the read maps uniquely, so it's record-empty). That's a fair naming-only assertion as documented, but the byte-identity proof for the ambig content rests entirely on the V9 oxy gate. No change needed for 2a; just confirming the local test is naming-only by design.
- **Low-3 (informational):** Deviation #1 in the plan (HISAT2 `--local` not reproduced, since `--local` is globally rejected in v1) is sound — Perl's `--local`+HISAT2 pushes only `--omit-sec-seq` (8310-8312), and v1 has no `--local` path at all. The `cli.old_flag` check inside the dovetail block (options.rs:151) is effectively dead because `--old_flag` is rejected upstream in `reject_unsupported_output_flags` (config.rs:383), but it harmlessly preserves Perl's Bowtie 2-only conflict semantics. No action.

## Bottom line
The implementation is faithful to the Perl v0.25.1 oracle on every SE-relevant point, keeps Bowtie 2 structurally byte-frozen, and correctly scopes the PE `ZS` hazard out to 2b. Approve to proceed to the V9 SE oxy byte-identity gate.

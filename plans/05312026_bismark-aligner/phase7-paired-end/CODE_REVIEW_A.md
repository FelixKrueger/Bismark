# CODE REVIEW A — Phase 7 (paired-end) Rust bismark aligner port

**Reviewer:** A (independent, fresh context)
**Date:** 2026-06-02
**Scope:** the PE additions to `rust/bismark-aligner/` (align/merge/methylation/output/convert/report/aux_out/lib + tests).
**Gate:** byte-identical *decompressed* SAM vs Perl `bismark` v0.25.1 + Bowtie 2 2.5.5.
**Method:** line-by-line hand-trace against the Perl source of truth (no live Perl differential — the sub is not callable standalone, as Phase 4/5 reviewers established); plus `cargo test`/`clippy`/`fmt`.

---

## Summary

The PE implementation is a faithful, careful port. Every byte-identity-critical block I checked
(FLAG constant table, the full TLEN tree incl. dovetail FLAG-gates and containment, the per-mate
could-not-extract short-circuit with the mate1-strict/mate2-non-strict 5′ guard asymmetry, the
sum-of-AS merge with the two-branch location key and the 3811–3816 sum-2nd conditional, the R2
forward-G→A conversion with the `/1/1`/`/2/2`-before-`\n` suffix incl. the CRLF quirk, the report
wording + 0,2,1,3 strand-label join order + the `% \n` trailing-space, the two-R1-id-string
routing, and the `_pe.bam`/`_PE_report.txt`/`_1`/`_2` naming) traces correctly to the Perl. The
parallel-types design keeps the SE paths untouched; I found **no SE regression**.

- `cargo test -p bismark-aligner`: **192 passed, 0 failed** (171 lib + 21 integration).
- `cargo clippy -p bismark-aligner --all-targets -- -D warnings`: **clean**.
- `cargo fmt -p bismark-aligner -- --check`: **clean**.
- (Both cargo commands needed `dangerouslyDisableSandbox` — the sandbox blocked
  `target/debug/.cargo-lock` with "Operation not permitted". Not a code issue.)

**Verdict: APPROVE-WITH-FINDINGS** — 0 Critical, 0 High. The few findings are Low/observational;
none threaten the byte-identity gate. The oxy PE gate (§7 #21) remains the final arbiter, as planned.

---

## Verification by area (the 8 focus points)

### 1. FLAG constants (output.rs 469–480 vs Perl 8825–8868) — ✅ CORRECT
The four `!old_flag` pairs match exactly, including the index-1/2 R1↔R2 swap:
`0→(99,147)`, `1→(163,83)`, `2→(147,99)`, `3→(83,163)` (Perl 8827/8828, 8837/8840, 8849/8852,
8861/8862). Ported as a literal table, not bit-assembly, as the plan required. `index > 3` errors.
The integration test `pe_mapped_writes_two_bam_records_end_to_end` confirms (99,147) end-to-end via
the BAM.

### 2. TLEN tree (output.rs 499–530 vs Perl 8890–8994) — ✅ CORRECT
Hand-traced every branch against the Perl:
- A (`start1 <= start2`) / B (`start2 < start1`) is a total `if/else` partition (no trailing else
  needed) — matches Perl's `if/elsif` where the two conditions are exhaustive. A I-1 honoured.
- A1 (`end2 >= end1`): dovetail gate `flag_1==83 && dovetail` → `(start1-end2-1, end2-start1+1)`
  (Perl 8904/8905); normal → `(end2-start1+1, start1-end2-1)` (8922/8923). ✅
- A2 (`end2 < end1`, R2 contained): `l = end1-start1+1; (l, -l)` — Perl uses `end_read_1` for BOTH
  tlen_1 (8938) and tlen_2 (8939). ✅ (verified the Rust uses `end1`, not `end2`).
- B1 (`end1 >= end2`): dovetail gate `flag_1==99 && dovetail` → `(end1-start2+1, start2-end1-1)`
  (Perl 8957/8958); normal → `(start2-end1-1, end1-start2+1)` (8975/8976). ✅
- B2 (`end1 < end2`, R1 contained): `l = end2-start2+1; (-l, l)` — Perl uses `end_read_2` for both
  (8990/8991). ✅
- Basis mismatch preserved (B O-1): `start` = 1-based POS (`best.position_*`), `end` =
  0-based-walked `end_position_*`; the `+1`/`-1` constants absorb it. ✅
- Dovetail FLAG-gate **negative** path verified by `pe_dovetail_gate_negative_index1_not_dovetailed`
  (index-1 flag_1=163 takes the normal branch). The `pe_tlen_tree` matrix covers A1/A2/B1/B2 +
  dovetail-on/off + the `start1==start2`/`end2==end1` equality cell. Signs and magnitudes correct.
- `tlen as i32` (line 643) — the magnitudes are bounded by fragment length, no overflow risk.

### 3. Per-mate could-not-extract contract (methylation.rs walk_mate + lib.rs guards vs Perl 4535/4608/4622/4693 + 3864/3869) — ✅ CORRECT
- `walk_mate` returns `MateWalk::Edge` (bare-`return` analogue) leaving the failing mate's `non_bis`
  SHORT; mate1 edge → mate2 never walked (its `Vec`s empty), so the R1 length check fails first.
  `extract_…_paired_end` returns the `mk(...)` struct with real lengths; **no pair-level flag**,
  **no zeroing both**. Matches the plan's edge-state contract.
- `strict_5p` parameterised: mate1 uses `(pos as i64) - 2 > 0` (Perl 4535, requires SAM `position_1
  >= 4`), mate2 uses `>= 0` (Perl 4622, `position_2 >= 3`). The SE port's `>= 0` is NOT reused for
  mate1. Tests `pe_mate1_5prime_guard_is_strict_gt0` (position_1=3 → Edge) and
  `pe_mate1_5prime_passes_at_position_4` pin the boundary. ✅
- lib.rs `drive_merge_pe` 864–879: R1 guard first with `continue` (return-0 analogue) BEFORE the R2
  guard, each bumping `genomic_sequence_could_not_be_extracted_count` by exactly 1 — R1 failure
  short-circuits so R2 is never checked (Perl 3864→3867 before 3869). Both are inside the
  `UniqueBest` arm, so a counted-in-`unique_best`-but-no-strand-bucket edge pair is reproduced.
  Test `pe_mate2_chr_edge_leaves_mate1_full_mate2_short` confirms mate1 full / mate2 short / no
  counter. ✅

### 4. The merge (merge.rs check_results_paired_end vs Perl 3269–3897) — ✅ CORRECT
- Scan order `[0,3,1,2]`, slot-indexed `&mut [Option<S>]` (directional supplies slots 0/3; the
  driver builds `vec![Some(s0), None, None, Some(s3)]`). `index` keys the strand bucket + the
  directional reject. B I-1 honoured. ✅
- `(77,141)` no-align: `advance_pair()?; continue;` with NO die-if-same-id guard (A O-3; contrast
  the SE path's die at merge.rs 168). Matches Perl 3317–3346 which has no such die. ✅
- De-convert both RNAMEs; `chr1 != chr2` → error (Perl 3364). ✅
- `sum = as1 + as2` (Perl 3416). Overwrite/`best_sum_so_far`/`amb_same_thread` machinery mirrors the
  SE structure keyed on the sum (3422–3463); strictly-better resets `amb_same_thread` + recaptures
  `first_ambig`; `>=` keeps equally-good. ✅
- Single-mate-XS default: `if sb1.is_some() || sb2.is_some() { sb1 = sb1.or(as1); sb2 = sb2.or(as2) }`
  (Perl 3466–3474). Test `pe_single_mate_xs_defaults_to_own_as`. ✅ (B O-5)
- Within-thread tie (`sum == sum_second`): `amb_same_thread = true` iff `best == sum` (3483–3488),
  store nothing. ✅
- Location key: second-best branch `chr:min:max` (`min_max_key=true`, swap when `r1.pos > r2.pos`,
  matching Perl's `<=`→pos1:pos2 else pos2:pos1 at 3527–3532); no-second-best branch raw
  `chr:pos1:pos2` (`min_max_key=false`, Perl 3593). The Perl inconsistency is preserved. ✅ (Q4)
- Unique-best: 1 entry → accept; 2–4 → sort desc, top tie → `unsuitable_sequence_count++` +
  `Ambiguous{None}` (3788–3790; cross-tie has no AMBIBAM write); else sum-2nd via the 3811–3816
  conditional (`best.sum_second_best` iff `> runner_up`, else `runner_up`); `>4` → error (3823). ✅
- Directional reject index 1|2 (Perl 3851–3856): `unique_best_alignment_count` is NOT bumped
  (the `++` at line 666 is AFTER the reject return at 661–664) — matches Perl, where 3860 is after
  the 3855 return. Test `pe_directional_rejection_index_1` asserts `unique_best == 0`. ✅
- `calc_mapq(seq1.len(), Some(seq2.len()), best.sum, second_for_mapq, …)` (Perl 3876). ✅

### 5. Conversion (convert.rs vs Perl 5810–6025) — ✅ CORRECT
- R1 → `ConvKind::Ct` + `/1/1`; R2 → forward `ConvKind::Ga` (`convert_seq_g_to_a`, uc-then-`g→A`,
  NOT revcomp+C→T) + `/2/2` (Perl 5982 / 5977–5984). ✅
- Suffix-before-`\n`: chomp `\n` → `fix_id` → append suffix → re-append `\n`. This reproduces Perl's
  `s/$/\/1\/1/` on a `\n`-terminated string (non-`/m` `$` inserts before the trailing `\n`),
  **including the CRLF quirk**: `@r1\r\n` → `@r1\r/1/1\n` (test `pe_suffix_inserted_before_newline_crlf`,
  hand-verified against Perl `chomp`+`.= "\n"`+`s/$/.../`). ✅
- Shared `convert_fastq_impl` factors gz/skip/upto/prefix/max-len/sanity/verbatim-id2-qual;
  the SE entry passes `id_suffix=b""` and the SE byte tests are unchanged + green → **no SE
  regression**. ✅
- Ordering note: the Rust appends the suffix BEFORE the skip/upto check, whereas Perl applies it
  after; this is byte-neutral (skipped records are never written; the record-1 sanity tests
  `starts_with(b"@")`/`starts_with(b"+")` are unaffected by an end-appended suffix). (See L-1.)

### 6. Report (report.rs vs Perl 2186–2312) — ✅ CORRECT
- `Mapping efficiency:\t{}% \n` — the `% \n` trailing space matches REPORT line 2205 (NOT the STDOUT
  twin 2204). Test `pe_mapping_efficiency_has_trailing_space` asserts both presence of `% \n` and
  absence of `%\n`. ✅
- Strand-label join order `0,2,1,3` (Perl 2218): `ct_ga_ct_count`, `ga_ct_ct_count`,
  `ga_ct_ga_count`, `ct_ga_ga_count` with the matching 3-token labels + parentheticals. ✅ (B I-5)
- 7 "Sequence pairs …" wording swaps (2195/2207–2217) exact. ✅
- `ReportHeader.sequence_file2: Option<&str>` → `for: <f1> and <f2>` (Perl 1843); SE passes `None`.
  Shared `write_report_header`. ✅ (B I-6)
- The cytosine half is shared (`write_cytosine_report`), byte-identical SE==PE (Perl 2052–2136 ==
  2226–2312); the full `pe_final_analysis_exact_directional` golden matches. ✅
- Zero-pairs → `Mapping efficiency:\t0% \n` (bare 0, no div-by-zero). ✅

### 7. Lockstep / routing (lib.rs vs Perl 2578–2696 + 1746–1962) — ✅ CORRECT
- Two distinct R1 id strings (B I-4): `identifier` = R1 fix_id + `@`-strip (the merge key, Perl 2640);
  the aux R1 record is written via `write_fastq_record` which re-prepends `@`, so passing the
  `@`-stripped form reproduces the `@`-bearing `$orig_identifier_1` (2637/2651). R2 id likewise
  `@`-stripped for the aux (R2 is never the merge key; Perl never strips R2's `@`, but
  `write_fastq_record` re-adds it → byte-equivalent). ✅
- BAM records share the `@`-stripped QNAME (`paired_end_sam_output` uses `id` for both mates,
  `!old_flag` path appends no `/1`/`/2` — Perl 8735/8736). ✅
- Routing precedence: ambig-BAM (within-thread `Some` only) → then `--ambiguous` else `--unmapped`
  (Perl 2649/2663). `write_raw_pe_ambig_lines` strips `/1\t`/`/2\t` (`replacen(.., 1)`) + de-converts
  RNAME (Perl 3677–3682). ✅
- Naming: `_bismark_bt2_pe.bam` (lowercase, basename → `_pe.bam`), `_bismark_bt2_PE_report.txt`
  (uppercase), `_pe.ambig.bam` (A O-2), aux `_unmapped_reads_1/2.fq.gz` / `_ambiguous_reads_1/2.fq.gz`
  off the un-stripped mate basenames. One BAM holds both mates. ✅
- Aux `+`-line verbatim (un-chomped, keeps own `\n`/`\r\n`); seq chomped + explicit `\n`; seq is the
  ORIGINAL non-uc read (A O-4). ✅
- Lockstep `last unless`: guards the 6 needed lines (the two `+` lines NOT guarded — Perl 2611). ✅
- `sequences_count++` per pair after skip but the skip uses `count` (running, incl. skipped) — Perl
  2623/2625/2632. ✅

### 8. General (errors / child-pipe / overflow / SE regression) — ✅ OK
- `PairedAlignerStream`: same drain-then-wait `finish()` + kill-then-wait `Drop` as `AlignerStream`;
  header-skip then peek-two; a lone trailing line → `None` (not a complete pair, Perl 6491). ✅
- `SamPair::from_lines` canonicalises R1 by the trailing `/1`; `die` if neither id ends `/1`
  (Perl 6500–6508). Tests cover R1-first, R2-first (swap), and neither. ✅
- No `pos-2` underflow: the 5′ guards use `(pos as i64) - 2 {>,>=} 0` (signed) BEFORE the
  `chr[pos-2..pos]` slice, so the subtraction can never wrap a `usize`. ✅
- `g.len().saturating_sub(2)` / `g.get(2..).unwrap_or(&[])` in the +2 trim guard against a <2-byte
  genomic window (defensive; real windows are read_len+2). ✅
- SE paths (`single_end_sam_output`, `check_results_single_end`, `bisulfite_convert_fastq_se`,
  `print_final_analysis_report_single_end`, SE `aux_filename(None)`) are untouched in behaviour;
  the `convert`/`report`/`aux_out` refactors are additive (shared inner + extra `Option` param with
  SE call-sites passing `None`/`b""`). All SE tests green. ✅

---

## Issues by area

### Logic
None affecting correctness or the byte-identity gate. All branch logic traces to the Perl.

### Efficiency
- **L-2 (Low, observational):** `check_results_paired_end` clones the whole `SamPair`
  (`stream.current_pair().unwrap().clone()`, merge.rs 492) per matching instance per pair — two
  `SamRecord`s incl. `raw_line`/`seq`/`qual`. The SE merge does the same (`.clone()` at merge.rs
  162), so this is consistent precedent, not a regression; the extractor port found the pipeline
  decode-bound regardless. No action needed for the gate.

### Errors
- None. Error messages mirror the Perl `die`s (mandatory AS/MD, same-chromosome, too-many-hits,
  illegal CIGAR). The intentional fail-closed-on-nonzero-exit deviation (documented on
  `AlignerStream::finish`) is mirrored on `PairedAlignerStream::finish`.

### Structure
- **L-1 (Low, observational):** in `convert_fastq_impl` the `/1/1`/`/2/2` suffix is appended
  (line 257) BEFORE the skip/upto checks (262–273), whereas Perl applies `s/$/.../` AFTER skip/upto
  (5945 after 5928). Byte-neutral (skipped records never written; the count-1 sanity uses
  `starts_with`, unaffected by an end-appended suffix). Could add a one-line comment noting the
  re-ordering is deliberate-and-safe, to forestall a future "fix". Not required.
- **L-3 (Low, observational):** `GenomicExtractionPaired` has no `extracted` doc-flag (unlike SE's
  `GenomicExtraction.extracted`), correctly relying on the per-mate length check as the sole signal
  (the plan's edge-state contract). Good — flagging only so a reader doesn't expect symmetry with SE.

---

## Recommendations (prioritised)

- **Critical:** none.
- **High:** none.
- **Medium:** none.
- **Low:**
  - L-1 — add a one-line comment in `convert_fastq_impl` that the suffix-before-skip ordering is
    intentional and byte-neutral (documentation only).
  - L-2 / L-3 — no action; recorded for completeness.
- **Gate:** proceed to the oxy PE byte-identity gate (§7 #21) as the authoritative differential —
  the unit/hand-traced FLAG+TLEN values are sound but, per the Phase 4/5 precedent, the live-Perl
  run on real data + Bowtie 2 2.5.5 is the final arbiter. Recommend including at least one real
  ambiguous pair (to exercise the `--ambig_bam` two-line path) and a chromosome-edge pair on each
  mate (to exercise the per-mate could-not-extract short-circuit) in the gate sample, as the plan's
  validation matrix (#21) already notes.

---

## Verdict

**APPROVE-WITH-FINDINGS — Critical: 0, High: 0** (Medium: 0; Low: 3, all observational/documentation).
The PE port is byte-faithful to Perl v0.25.1 across every checked path; tests/clippy/fmt are green;
the SE paths are untouched (no regression). The only gate left is the planned oxy PE differential.

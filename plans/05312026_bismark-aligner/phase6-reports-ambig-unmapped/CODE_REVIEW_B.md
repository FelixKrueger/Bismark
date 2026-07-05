# Code Review B — Phase 6: Reports + ambig/unmapped + `--ambig_bam` (SE directional)

**Reviewer:** B (independent, fresh context) — recommend-only, no files modified.
**Scope:** `report.rs`, `aux_out.rs`, `output.rs` (raw-record path), `merge.rs` (`Decision::Ambiguous`),
`config.rs`, `lib.rs` (driver/`Sinks`/routing), `bismark-io/src/write.rs` (`write_raw_record`), `tests/cli.rs`.
**Oracle:** Perl `bismark` v0.25.1 (`/Users/fkrueger/Github/Bismark-aligner/bismark`).

## Summary

This is a clean, faithful, byte-conscious implementation. I traced every `print REPORT` line in
`print_final_analysis_report_single_end` (Perl 2004–2137) against `report.rs` and the report body is
**byte-identical** modulo the deliberately-normalised wall-clock line — including the three byte traps
called out in the plan: `Total C's` excludes the Unknown buckets (Perl 2053), the `(me+unme)>0`
percentage gate that prints `0.0%` rather than "Can't determine" for an all-unmethylated bucket (the
Perl `if ($percent)` truthiness trap, 2099), and the single-`\n` after `Mapping efficiency` (REPORT
2025, *not* the `\n\n` of the `warn` twin at 2024). The routing precedence, the
within-thread-only `--ambig_bam` write, the dual-arm `first_ambig` capture, the un-stripped aux
filename, and the verbatim `+`/non-uc-seq FastQ record are all correct against the Perl.

Verified locally (sandbox-disabled): **131 aligner-lib + 19 aligner-integration + 179 bismark-io**
tests pass; `cargo fmt --check` clean; `cargo clippy --all-targets` clean for both crates. The
`=1.0.0-beta.9` pin propagated to **all six** `bismark-io` dependents (aligner, bedgraph, c2c, dedup,
extractor, methylation-consistency).

**No Critical or High findings.** Everything below is Low/Medium polish and confirmations.

---

## Issues by area

### Logic

- **(Confirmed correct) Routing precedence** — `lib.rs:458–478`. `Ambiguous` routes to `ambiguous`
  else `unmapped` else nothing; `NoAlignment` (`lib.rs:481–492`) routes to `unmapped` else nothing;
  `Rejected` (`lib.rs:494`) writes nowhere. This collapses Perl's return-code indirection
  (2980/2982/2986 + 2995/2998 + 3116) into the driver correctly. The `--ambig_bam` write
  (`lib.rs:459–463`) is gated only on `first_ambig.is_some()`, independent of the FastQ flags —
  matching Perl's unconditional `print AMBIBAM` at 2976. ✓

- **(Confirmed correct) `first_ambig` capture at BOTH score-setting arms** — `merge.rs:200–202`
  (first-alignment arm, Perl 2806–2810) and `merge.rs:207–214` (strict-improvement arm,
  Perl 2818–2826). Never re-captured on an *equal* AS (the `if alignment_score > best` guard mirrors
  Perl's nested `if ($alignment_score > $best_AS_so_far)`). The cross-instance-tie site
  (`merge.rs:280`) carries `None`, matching the no-`AMBIBAM`-write 3091 block. Test
  `first_ambig_captures_strict_improvement_instance` (merge.rs:707) pins the ordering. ✓

- **(Confirmed correct) Report field order/whitespace** — every `print REPORT` from 2004 to 2137 maps
  1:1 to `report.rs:75–189`, in order, with identical separators (`=`×22 / `=`×33; the
  no-align/not-uniquely/could-not-extract block ending `\n\n` at 2042; the joined CT/CT…GA/GA block
  ending `\n\n` at 2044; the directional rejected line `\n\n` at 2048; the trailing `\n\n` at 2137).
  The STDOUT-only `print` lines (2034–2038, 2467) and the deletion `warn`s (1977/1995) are correctly
  NOT written to the report. ✓

- **(Confirmed correct) `Total C's` excludes Unknown** — `report.rs:129–134` sums only the 6
  CpG/CHG/CHH me+unme buckets, matching Perl 2053 (`total_meCHH+total_meCHG+total_meCpG +
  total_unmethylated_{CHH,CHG,CpG}`). Test `all_unknown_total_is_zero` confirms Unknown-only → `0`. ✓

- **(Confirmed correct) Percentage gate is `(me+unme)>0`, not "percentage non-zero"** —
  `report.rs:198–208`. An all-unmethylated bucket (`me==0, unme>0`) prints `0.0%`
  (test `all_unmethylated_bucket_prints_zero_point_zero`), exactly reproducing Perl's quirk where
  `$percent = "0.0"` is truthy at the `if ($percent)` guard (2099) and only `undef`
  (i.e. `me+unme==0`) is falsy. This is the single most error-prone line in the phase and it is right. ✓

- **(Confirmed correct) Mapping efficiency f64 + zero-sequences branch** — `report.rs:80–92`. f64
  division `(unique as f64)*100.0/(seq as f64)` formatted `{:.1}`, with the
  `sequences_count==0 → "0"` (bare, not "0.0") short-circuit matching Perl 2017–2025. Single `\n`
  after the efficiency line. ✓

### Errors / edge cases

- **(Low — defensiveness note) `build_raw_record` POS=0 / RNAME-not-found.** `output.rs:522–527`:
  if the de-converted RNAME isn't in `refid`, `reference_sequence_id` is left `None` (RNAME `*`),
  and if POS==0, `alignment_start` is left `None`. For a *mapped* ambiguous record (the only thing
  ever fed here — Perl writes AMBIBAM only inside the `amb_same_thread` block, where a real alignment
  exists), both are always populated, so this never fires in practice. The silent fallthrough is a
  reasonable defensive choice (an unmapped marker is never the `first_ambig`), but it means a
  genuinely malformed RNAME would be written as `*`/unmapped rather than erroring. Perl's
  `s/_(CT|GA)_converted//` would equally leave a non-suffixed RNAME intact and let samtools resolve
  the tid. Behaviour is effectively equivalent; flagging only so a future reviewer doesn't mistake
  the `if let Some` for a bug. No action needed.

- **(Low — confirm on the gate) Always-`-33` QUAL offset in `build_raw_record`** — `output.rs:536`.
  The raw aligner line's QUAL is Bowtie 2's verbatim echo of the *converted-FastQ* quality, which the
  Rust `convert.rs` writes **verbatim** (no phred64→33 conversion at convert time; that conversion
  lives only in the Bismark SAM-output path, Perl 4191). So Bowtie 2 echoes the *original* ASCII
  (phred64 chars under `--phred64`). Subtracting 33 here and letting `samtools view -h` add 33 back
  recovers the original bytes byte-for-byte — which is exactly what Perl's `samtools view -bSh` pipe
  does to the raw line (it stores the ASCII unchanged). So the always-33 offset is **correct for the
  ambig BAM** even under `--phred64`, *because* it round-trips. This differs from
  `single_end_sam_output` (output.rs:382), which correctly uses the phred64-aware offset for the
  *Bismark* record. Worth a one-line comment at output.rs:536 ("offset is immaterial: any constant
  round-trips through samtools for this raw passthrough") so a future editor doesn't "fix" it to
  honour `--phred64` and silently break byte-identity. Recommend the comment; the code is right.

- **(Confirmed correct) Temp-file deletion best-effort** — `lib.rs:214` `let _ =
  std::fs::remove_file(&converted.path)`. Matches Perl's `unlink … or warn` (never `die`,
  1974–1981). Phase-5 integration test was inverted to assert deletion. ✓

- **(Confirmed correct) `Sinks` finalisation** — `lib.rs:232–248`. BAM `finish()` (BGZF EOF),
  then optional ambig-BAM `finish()`, then the two gz `finish()` (flate2 trailers). Called at
  `lib.rs:211` after the report is flushed, before the temp unlink. Each writer is `#[must_use]` so a
  forgotten finish would be a compile warning. ✓

- **(Confirmed correct) Truncated final FastQ record** — `lib.rs:384` breaks if any of the four
  `read_until` returns 0, so a partial record is dropped (Perl `last unless (…)`, 2418). A `+` line
  with no trailing `\n` (n3>0) is still processed and written verbatim without an appended `\n`
  (`aux_out.rs:80`), matching Perl's `print AMBIG $identifier_2`. ✓

- **(Confirmed correct) Windows-reserved-name rename** — the module is `aux_out.rs`, not `aux.rs`
  (`aux` is a reserved device name on Windows and cannot be a source filename). Declared as
  `pub mod aux_out;` (lib.rs:23). ✓

### Efficiency

- **(Confirmed correct) `first_ambig` clone gated** — `merge.rs:200/211` clone `rec.raw_line` only
  under `want_ambig` (= `config.ambig_bam`, passed at `lib.rs:416`). No allocation when `--ambig_bam`
  is off (`within_thread_ambiguity_no_capture_when_flag_off` pins this). ✓

- **(Low — micro) Per-Ambiguous/NoAlignment `seq_orig` re-chomp.** `lib.rs:470` and `lib.rs:483`
  each recompute `convert::chomp_newline(&seq).to_vec()`. `seq_uc` is already computed once at
  `lib.rs:405`, but the *non-uc* original is needed here and is correctly recomputed from the raw
  `seq` buffer (you can't recover the original casing from `seq_uc`). This is a single short
  allocation only on the non-UniqueBest paths (the minority of reads) — negligible, and the duplicated
  line could be hoisted just before the `match` but only at the cost of always allocating even for
  UniqueBest reads (the common case). Current placement is the better trade-off. No action.

### Structure / naming

- **(Low — doc nit) `aux_out` CRLF comments are slightly misleading.** `aux_out.rs:138–139` test
  comment says "seq/qual were chomped of `\r` upstream and get an explicit `\n`", and the doc at
  `aux_out.rs:65` says `seq` is "chomped but NOT upper-cased". In the real driver path,
  `chomp_newline` strips only `\n` (Perl `chomp`), so a CRLF read's `seq` **retains its `\r`** and is
  written `…\r\n` — which is correct vs Perl (`chomp $sequence` keeps `\r`, then `print "$sequence\n"`
  → `…\r\n`). The test passes `b"ACGT"` (already `\r`-free) so it doesn't actually exercise a
  `\r`-bearing seq through the writer, and the comment overstates that `\r` was stripped. The
  *behaviour* is correct; only the comment is imprecise. Consider (a) tightening the comment to
  "seq/qual keep any `\r` (Perl chomp strips only `\n`)", and (b) optionally a driver-level CRLF unit
  test feeding `b"ACGT\r"` to confirm `…\r\n` end-to-end (the per-function pieces are individually
  correct, but no test pins the *composed* CRLF seq path). Low priority — §7 #7b in the plan claims a
  CRLF FastQ-record test; the existing one only covers the verbatim `+` line, not a `\r`-bearing seq.

- **(Confirmed correct) Aux filename derivation (un-stripped basename)** — `aux_out.rs:40–58`. Uses
  the read-file basename **without** `strip_fastq_suffix` (so `reads.fq.gz` →
  `reads.fq.gz_unmapped_reads.fq.gz`), matching Perl `$unmapped_file = $filename` (1645) + `s/$/…/`
  (1660). `--basename` overrides prefix+filename (1650/1684); `--prefix` → `{p}.{filename}…`
  (1647/1681). `.gz` appended for the SE single-core path (1671). Tests pin all three variants. ✓

- **(Confirmed correct) `--ambig_bam` filename** — `lib.rs:262`
  `derive_output_path(…, "_bismark_bt2.ambig.bam", ".ambig.bam")`. Perl: `$outfile` (=
  `<prefix>.<stripped-stem>_bismark_bt2.sam` or `${basename}.sam`) then `s/sam$/ambig.bam/`
  (1585–1586) → `<prefix>.<stem>_bismark_bt2.ambig.bam` / `${basename}.ambig.bam`. The Rust's stripped
  + `_bismark_bt2.ambig.bam` (and basename `.ambig.bam`) reproduces both exactly. ✓

- **(Confirmed correct) Report header** — `report.rs:38–66`. Line 1 (`Bismark report for: <file>
  (version: v0.25.1)`, Perl 1642); the directional line (1712); the
  `Bismark was run with Bowtie 2 against the bisulfite genome of <genome>/ … <opts>\n\n` line (1722)
  with the **trailing `/`** rendered at `lib.rs:132` (`format!("{}/", genome_dir.display())`),
  matching Perl's `getcwd`-after-`chdir` forced-trailing-slash absolutization (7619–7629).
  `header_directional_exact` pins it. (The pbat/non-dir arms exist but are Phase 8; harmless.) ✓

- **(Confirmed correct) `write_raw_record` API + propagation** — `bismark-io/src/write.rs:86–89`
  bypasses `BismarkRecord` validation by taking a `&RecordBuf` directly. The `#[must_use]` finish
  contract is preserved. beta.8→beta.9 bump applied; all six dependents pinned `=1.0.0-beta.9`. The
  `write_raw_record_bypasses_bismark_validation` test (write.rs:441) proves a no-XR/XG/XM record is
  rejected by `BismarkRecord` yet written + read back via raw noodles. ✓

---

## Recommendations (prioritised)

**Critical:** none.

**High:** none.

**Medium:**
1. **Add a driver-level CRLF seq test** (or downgrade the §7 #7b claim). The composed path
   "CRLF read → `chomp_newline` keeps `\r` → `write_fastq_record` appends `\n` → `…\r\n`" is correct
   but only verified piecewise; no single test feeds a `\r`-bearing *sequence* through the driver to a
   gz aux file. A one-read integration (or a `drive_merge`-level unit) with `@r1\r\nACGT\r\n+\r\nII\r\n`
   would pin it and match Perl's CRLF FastQ behaviour. (`tests/cli.rs:466`/`512` are the natural home.)

**Low:**
2. **Comment the always-33 QUAL offset** at `output.rs:536` to record *why* the offset is immaterial
   for the raw passthrough (it round-trips through samtools), so a future `--phred64` "fix" doesn't
   break byte-identity.
3. **Tighten the CRLF comments** in `aux_out.rs:65` and `aux_out.rs:138–139` to say the seq/qual
   **retain** any `\r` (Perl `chomp` strips only `\n`), since the current wording implies `\r` was
   stripped upstream.

## Files reviewed
- `/Users/fkrueger/Github/Bismark-aligner/rust/bismark-aligner/src/report.rs`
- `/Users/fkrueger/Github/Bismark-aligner/rust/bismark-aligner/src/aux_out.rs`
- `/Users/fkrueger/Github/Bismark-aligner/rust/bismark-aligner/src/output.rs`
- `/Users/fkrueger/Github/Bismark-aligner/rust/bismark-aligner/src/merge.rs`
- `/Users/fkrueger/Github/Bismark-aligner/rust/bismark-aligner/src/config.rs`
- `/Users/fkrueger/Github/Bismark-aligner/rust/bismark-aligner/src/lib.rs`
- `/Users/fkrueger/Github/Bismark-aligner/rust/bismark-aligner/src/convert.rs` (chomp/fix_id support)
- `/Users/fkrueger/Github/Bismark-aligner/rust/bismark-io/src/write.rs`
- `/Users/fkrueger/Github/Bismark-aligner/rust/bismark-aligner/tests/cli.rs`
- Perl oracle `/Users/fkrueger/Github/Bismark-aligner/bismark`
  (1559–1729, 1964–2144, 905–931, 2393–2482, 2700–3131, 8452–8484, 7615–7634)

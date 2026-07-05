# Code Review A — Phase 6 (Reports + ambiguous/unmapped + `--ambig_bam`, SE directional)

**Reviewer:** A (independent, fresh context)
**Scope:** `report.rs`, `aux_out.rs`, `merge.rs` (Decision::Ambiguous), `output.rs`
(raw ambig path), `bismark-io/src/write.rs` (`write_raw_record`), `config.rs`,
`lib.rs` (Sinks/open_sinks/derive_output_path/drive_merge routing),
`tests/cli.rs`. Reviewed against Perl v0.25.1 `bismark`.
**Verdict:** **APPROVE.** No Critical/High findings. The byte-identity-critical
surfaces (report text, routing precedence, `first_ambig` capture, filename
derivation, version-bump propagation) are all faithful to the Perl. A handful of
Low/Medium notes below — none block the merge; most are forward-safety nits or
things the oxy gate already covers.

---

## Summary

Phase 6 closes the SE-directional spine. I traced every `print REPORT` line in
`print_final_analysis_report_single_end` (Perl 2004–2137) against `report.rs`,
the routing precedence (Perl 2451–2465 + 2974–2999 + 3091–3116) against
`drive_merge`, the `first_ambig` capture (Perl 2806/2822 + write at 2976 / no-write
at 3091) against `merge.rs`, and the aux/ambig filename derivation (Perl
1559–1709) against `aux_out::aux_filename` + `derive_output_path`. All match.
`report::` unit tests pass locally (9/9). The new `bismark-io` `write_raw_record`
is a minimal, correctly-documented passthrough and the `=beta.9` bump reached all
4 real dependents (dedup / extractor / methylation-consistency / aligner; bedgraph
and c2c don't depend on bismark-io, so the "4 dependents" claim is exact).

---

## Issues by area

### Logic

**L1 (verified correct) — Routing precedence.** Perl encodes precedence in two
places: the return code inside `check_results_single_end` (2979–2987 ambiguous →
`--ambiguous?2 : --unmapped?1 : 0`; 2995–2999 no-align → `--unmapped?1:0`) and the
driver's `if ($ambiguous and $return==2) … elsif ($unmapped and $return==1)`
(2451–2465). The Rust collapses both into the driver: `Ambiguous` routes
`ambiguous.is_some() ? ambiguous : unmapped` (lib.rs 464–468), `NoAlignment`
routes to `unmapped` only (481–491). I checked the cross-product:
- Ambiguous + `--ambiguous` only → ambiguous file ✓
- Ambiguous + `--unmapped` only → unmapped file ✓
- Ambiguous + both → ambiguous file (precedence) ✓
- Ambiguous + neither → nowhere ✓
- NoAlignment + `--ambiguous` only → **nowhere** (Perl returns 0 since `--unmapped`
  is off; Rust `unmapped.as_mut()` is `None`) ✓ — this is the easy one to get
  wrong and the Rust gets it right.
- Rejected → nowhere ✓

**L2 (verified correct) — `first_ambig` capture site + ordering.** Perl
(re)assigns `$first_ambig_alignment` at 2806 (first alignment) and 2822 (strict
improvement, `>`), never on an equal AS. `merge.rs` mirrors this exactly: capture
in the `None =>` arm (197–203) and inside `if alignment_score > best` (207–214),
gated on `want_ambig`. The capture runs *before* the second-best/`amb_same_thread`
block (221–248), matching Perl's order (2806/2822 precede 2844–2850). `Some` only
flows out of the within-thread boot (258–261); the cross-instance-tie returns
`None` (280). Tests `first_ambig_captures_strict_improvement_instance`,
`cross_instance_tie_has_no_first_ambig`, and `within_thread_ambiguity_*` pin all
three. Faithful to the rev-2 Critical.

**L3 (verified correct) — Report text & arithmetic.** Line-by-line vs Perl
2004–2137: `=`×22 / `=`×33; `Total C's` excludes Unknown (report.rs 129–135 vs
2053); mapping efficiency is `f64` division with a single trailing `\n` (not the
`warn` twin's `\n\n`, 2024 vs 2025) and the bare `0%` only in the 0-sequences
branch (80–87 vs 2017–2018); the `(me+unme)>0` gate prints `0.0%` for an
all-unmethylated bucket and "Can't determine" only when `me+unme==0` (198–208 vs
2099/2103). The `seqID_contains_tabs` warning is correctly never emitted
(structurally 0). `final_analysis_exact_directional` pins the whole body byte-for-byte.

**L4 (verified correct) — Aux & ambig filename derivation.** `aux_filename`
(aux_out.rs 40–58) uses the **un-stripped** basename (Perl 1645 `$unmapped_file =
$filename` — no `s/\.fq$//`), with `--basename` overriding prefix+filename (Perl
1650) and prefix as `{p}.{filename}` (Perl 1647). `--ambig_bam` derives via
`derive_output_path(.., "_bismark_bt2.ambig.bam", ".ambig.bam")` which reproduces
Perl's `$outfile` (stripped stem + `_bismark_bt2.sam`, prefix prepended pre-suffix,
basename → `${basename}.sam`) then `s/sam$/ambig.bam/` (1585–1586). Both the
default and `--basename` cases match.

**L5 (Low) — Multi-file SE emits a wall-clock line per report; Perl emits one (last
report only).** lib.rs sets `started` once (117) and calls `write_completion_line`
inside the per-file loop (208), so every report file ends with a
`Bismark completed in …` line. Perl's `REPORT` handle is reopened per file and the
line is printed once at teardown (927), i.e. only the final file's report carries
it. The §12 notes acknowledge this and the gate normalises `^Bismark completed in `
out of both sides, so it is immaterial for the gate (which is single-file). Flagging
only so a future multi-file/multicore audit doesn't mistake the extra lines for a
real divergence. No action needed for Phase 6.

### Efficiency

**E1 (Low, fine as-is) — `want_ambig` clone gating.** The `first_ambig` clone is
correctly gated on `want_ambig` (merge.rs 200/211), so `--ambig_bam`-off runs pay
nothing. `drive_merge` passes `config.ambig_bam` (lib.rs 416). One micro-note: the
FastQ-aux `seq_orig` is recomputed via `convert::chomp_newline(&seq).to_vec()` in
both the Ambiguous (470) and NoAlignment (483) arms; this is per-routed-read (rare)
and trivially cheap, so not worth refactoring.

### Errors

**ER1 (Low) — `build_raw_record` re-parse vs Perl `samtools view -bSh`
passthrough.** The ambig record is parsed into a `RecordBuf` and re-serialised by
noodles, not text-piped. This means SEQ/QUAL/tags are re-encoded; the byte-identity
of `samtools view -h` output then depends on noodles ↔ samtools agreement (same
risk already accepted for the Phase-5 main BAM). The §7 #8d round-trip test pins the
noodles half; the oxy `samtools view -h` diff (§7 #10) is the real gate. No code
change — just confirming the gate, not the unit test, is the adjudicator here.

**ER2 (Low) — QUAL offset for the ambig record is hard-coded `33`.** `build_raw_record`
subtracts 33 (output.rs 536). The aligner's own SAM output is always Phred+33
regardless of input encoding (Bowtie 2 emits +33), so this is correct — Perl's
samtools passthrough likewise treats the line as +33. Worth a one-line comment that
the +33 here is *not* `config.phred64`-sensitive (the main-record path at
output.rs 382 *is*), to forestall a future "shouldn't this honour --phred64?" edit.
Not a bug.

**ER3 (verified correct) — Sinks finalisation order / best-effort temp unlink.**
`Sinks::finish` finalises bam → ambig_bam → unmapped → ambiguous, propagating the
first error (lib.rs 232–248); the gz encoders' `finish()` flush the trailer. The
C→T temp removal is `let _ = std::fs::remove_file(...)` (214) — best-effort, never
errors the run, matching Perl's warn-don't-die (1976–1981). The integration test
inverts the Phase-5 assertion to confirm deletion (cli.rs 178).

### Structure

**S1 (Low) — `derive_output_path` vs `aux_filename` split is the right call but
subtle.** Two filename schemes coexist: the stripped stem (BAM/report/ambig-bam)
and the un-stripped basename (unmapped/ambiguous). The doc comments on both
(lib.rs 310–313, aux_out.rs 33–39) call this out explicitly, which is exactly what
prevents the classic "I'll just reuse strip_fastq_suffix" regression. Good.

**S2 (Low) — `lib.rs` module doc is stale.** The crate-level doc (lib.rs 16–19)
still says BAM output "land[s] in Phase 5" and the pipeline "emits a per-read-file
merge counters summary" as if that's the end state — Phases 5 and 6 have since
landed the BAM, report, and aux outputs. Cosmetic; update when convenient.

---

## Recommendations (prioritised)

- **Critical:** none.
- **High:** none.
- **Medium:** none.
- **Low:**
  1. (ER2) Add a one-line comment in `build_raw_record` that the QUAL `−33` is
     intentionally encoding-agnostic (the aligner line is always Phred+33), distinct
     from the `--phred64`-aware path in `single_end_sam_output`.
  2. (S2) Refresh the `lib.rs` crate-doc to reflect that Phase 5/6 (BAM, report,
     aux files) are implemented.
  3. (L5) When multicore/multi-file lands, revisit the per-file wall-clock line so a
     non-gate consumer reading intermediate reports isn't surprised by it.
  4. (ER1) Keep the oxy `samtools view -h` ambig-BAM diff (§7 #10) as the
     adjudicator for the raw-record re-serialisation; the unit round-trip is
     necessary but not sufficient.

All four are optional polish; **the implementation is mergeable as-is pending the
§7 #10 oxy report+aux byte-identity gate.**

# Code Review B — Phase 7 (paired-end) Rust bismark aligner port

**Reviewer:** B (independent, fresh context)
**Date:** 2026-06-02
**Scope:** the PE additions in `rust/bismark-aligner/src/` (`align.rs`, `merge.rs`,
`methylation.rs`, `output.rs`, `convert.rs`, `report.rs`, `aux_out.rs`, `lib.rs`, `tests/cli.rs`)
**Oracle:** Perl `bismark` v0.25.1 (`/Users/fkrueger/Github/Bismark-aligner/bismark`)
**Gate:** byte-identical decompressed SAM + `_PE_report.txt` + `_1`/`_2` aux vs Perl + Bowtie 2 2.5.5.

## Build / test status
- `cargo test -p bismark-aligner` → **192 passed** (171 lib + 21 integration), 0 failed.
- `cargo clippy -p bismark-aligner --all-targets -- -D warnings` → clean.
- `cargo fmt -p bismark-aligner -- --check` → clean.

## Summary
The implementation is high-fidelity. I re-derived the FLAG table, the TLEN tree, the
`check_results_paired_end` control flow, the two-mate genomic extraction, the PE converter,
and the routing/aux/cleanup from the Perl source line-by-line. The FLAG constants (8825–8868),
TLEN partition + dovetail FLAG gates (8890–8994), RNEXT/PNEXT/mate-link, per-mate revcomp keyed
on stored strand, the index-keyed +2 trim, tag order `NM MD XM XR XG`, the scan order 0,3,1,2,
the (77,141) no-align marker, the sum-of-AS selection + the 3811–3816 `sum_2nd` conditional, the
two-branch location key (min/max vs raw), the directional reject 1|2, the per-mate in-order
length guards with R1 short-circuit, and the PE report body (7 wording swaps, 0,2,1,3 strand
join order, the `% \n` trailing space) all match the Perl exactly. SE paths are untouched and
the `convert_fastq_impl` / `write_cytosine_report` refactors preserve SE byte output (SE tests green).

**However, I found one CRITICAL byte-identity defect: the PE report _header_ writes its lines in
the wrong order (SE order, not PE order).** This will fail the §7 #21 oxy `_PE_report.txt` gate.

## Issues by area

### CRITICAL

#### C-1 — PE report header lines are in the wrong order (`report.rs` `write_report_header`, lib.rs:590–599)
`run_pe_directional` calls the shared `write_report_header`, which emits, for PE:
```
Bismark report for: <f1> and <f2> (version: v0.25.1)\n
Option '--directional' specified (default mode): ... (i.e. not performed)\n   <-- 2nd
Bismark was run with Bowtie 2 against the bisulfite genome of <gf> with the specified options: <opts>\n\n  <-- 3rd
```
But the **PE** Perl subroutine `start_methylation_call_procedure_paired_ends` (1746–) writes the
header in a **different order with different trailing newlines**:
- Perl **1843**: `Bismark report for: $sequence_file_1 and $sequence_file_2 (version: ...)\n`
- Perl **1846**: `Bismark was run with Bowtie 2 against the bisulfite genome of $genome_folder with the specified options: $aligner_options\n`  ← **single `\n`, emitted SECOND**
- Perl **1941** (later, after aux-file setup): `Option '--directional' specified (default mode): ... (i.e. not performed)\n\n`  ← **double `\n`, emitted THIRD**

So the correct PE header is:
```
Bismark report for: <f1> and <f2> (version: v0.25.1)\n
Bismark was run with Bowtie 2 against the bisulfite genome of <gf> with the specified options: <opts>\n
Option '--directional' specified (default mode): ... (i.e. not performed)\n\n
```
Two divergences vs the Rust output: (a) the `--directional`/library line and the `Bismark was
run with …` line are **swapped**; (b) the newline placement differs — in PE the *run-with* line
ends with a single `\n` and the *directional* line ends with `\n\n`, whereas the SE order (which
the Rust uses) puts the directional line first with a single `\n` and the run-with line last with
`\n\n`.

This is a genuine SE-vs-PE asymmetry in the Perl (SE `start_methylation_call_procedure_single_ends`
1548–: report-for 1642, directional 1712 `\n`, run-with 1722 `\n\n` — which the shared Rust header
matches correctly for SE). The shared `write_report_header` cannot serve both orders.

**The unit test `report.rs::pe_header_two_files` (lines 484–499) bakes in the wrong order**, so the
suite is green despite the defect. The PE integration test only does `report.contains("Bismark
report for: ")` (cli.rs:659), which doesn't catch ordering.

**Recommendation:** give the PE driver its own header writer (or a `paired: bool` flag on
`write_report_header`) that emits, in order: `report-for\n`, `run-with…\n` (single newline), then
the library line `\n\n`. Update `pe_header_two_files` to assert the corrected order and the single-
vs-double newline split. Verify against the §7 #21 oxy `_PE_report.txt` diff. (Non-directional PE
is Phase 8, but the directional/pbat lines all live in the same 1940–1948 block, so fixing the
ordering now is correct for all library types.)

### HIGH
_None._ (C-1 is the only byte-identity blocker; everything else below is low-risk.)

### MEDIUM

#### M-1 — R2 aux id relies on re-adding `@`, diverges on `@`-less malformed FastQ (lib.rs:836–838, 973–976)
Perl writes the R2 aux record id as `$orig_identifier_2` (fix_id'd, chomped, **NOT `@`-stripped** —
only R1 is stripped at 2640). The Rust passes `id2_stripped` (the `@`-stripped form) and relies on
`write_fastq_record` re-prepending `@`. For valid FastQ (id always begins `@`) this is byte-identical.
But if an R2 header line does **not** begin with `@` (malformed input), Perl keeps it verbatim (no `@`),
while the Rust's `strip_prefix(b"@")` no-ops then `write_fastq_record` prepends a spurious `@` → one
extra byte. The same applies to R1 (`identifier`, lib.rs:833–835). Real FastQ never triggers this, so
it won't affect the gate; flagging for completeness. **Recommendation:** accept as-is (matches the SE
aux precedent), or document the valid-FastQ assumption.

### LOW

#### L-1 — TLEN `i64 → i32` cast can wrap on pathological fragment lengths (output.rs:643)
`*rec.template_length_mut() = tlen as i32;` silently wraps for `|tlen| > 2^31`. The BAM TLEN field
is i32, so Perl's value would also be truncated on write; not byte-identity-relevant for real data.
No action needed.

#### L-2 — `Edge` variant carries `end_pos`/`indels` that are never consumed (methylation.rs:292–297, 480–489, 511–518)
On a per-mate edge miss the Rust populates `end_position`/`indels` in the returned struct, but the
caller's length guard (lib.rs:864/872) short-circuits with `continue` before TLEN/output reads them,
so they're dead — matching Perl, which `return`s without setting them. Behaviorally equivalent; the
dead fields are harmless. No action needed (the doc comments already explain the contract).

#### L-3 — `counters_summary_pe` is stderr-only diagnostic text (lib.rs:980–1001)
Free-form, not part of the gate (the gate reads `_PE_report.txt`, not stderr). The strand-label
order in this stderr summary (OT/CTOB/CTOT/OB) differs cosmetically from the report's join order,
but since it's stderr-only it doesn't matter. No action needed.

## Areas verified clean (no findings)

- **FLAG table** (output.rs:469–480) vs Perl 8825–8868: `0→(99,147) 1→(163,83) 2→(147,99) 3→(83,163)`
  incl. the index-1/2 R1↔R2 swap. Matches; unit-pinned (`pe_flag_constant_table`).
- **TLEN tree** (output.rs:504–530) vs Perl 8890–8994: total `if/else if` partition (`<=` A vs `<` B),
  the A2/B2 containment cases, the dovetail sub-cases gated on the literal FLAG constant (83 for A,
  99 for B) AND `dovetail`. 1-based start + 0-based-walked end basis preserved. The equality cells
  + dovetail-gate-negative (`pe_dovetail_gate_negative_index1_not_dovetailed`) + `--no_dovetail`
  axis are all covered in `pe_tlen_tree`. Matches.
- **`dovetail` derivation** (lib.rs:531–534): from `aligner_options.contains "--dovetail"` = Perl
  `$dovetail` (8047–8048). Threaded into `paired_end_sam_output`. Correct.
- **RNEXT/PNEXT/mate-link** (output.rs:641–643): `mate_reference_sequence_id == reference_sequence_id`
  (→ `=`), PNEXT = other mate's POS, TLEN signed. Matches Perl 8881–8886.
- **Per-mate revcomp keyed on STORED strand** (output.rs:610–617, `build_pe_mate`): the `-` mate
  revcomps actual+ref+qual and double-revcomps md on `D`, independently per mate — matches Perl
  8999–9015. The double-revcomp composes correctly with the extraction's first revcomp.
- **+2 trim index-keyed** (output.rs:487–497) vs Perl 8772–8779: index 0/3 → R1 last-2 / R2 first-2;
  index 1/2 → R1 first-2 / R2 last-2. NOT read_conv-keyed. Matches.
- **XR per-mate / XG shared** (output.rs:651–658): XR_1=read_conv_1, XR_2=read_conv_2, XG=genome_conv
  for both. Tag order `NM MD XM XR XG`. Matches Perl 9059–9064 + `pe_per_mate_xr_shared_xg_and_tag_order`.
- **id_1 == id_2 == id** (output.rs:536–574, both `build_pe_mate` get the same `id`): matches Perl
  8735–8736 (default `!old_flag`).
- **Extraction** (methylation.rs:399–549): two independent per-mate CIGAR walks, NO fragment span;
  index-driven +2 (5′ prepend index 1/3, 3′ append index 0/2); mate1 strict `(pos-2)>0` (Perl 4535)
  vs mate2 `(pos-2)>=0` (Perl 4622) via the `strict_5p` parameter; counter + revcomp ONLY past all
  four guards (4708–4775); `methylation_call` reused verbatim per mate; the 4-counter→index map and
  the revcomp target (index 0/1→mate2, 2/3→mate1) match. Edge-state contract (failing mate left
  SHORT, other mate FULL) correct.
- **Per-mate could-not-extract short-circuit** (lib.rs:864–879) vs Perl 3864→3867: R1 guard first,
  `continue` BEFORE R2 guard, each bumps the count by exactly 1; both reach here counted in
  `unique_best` (Perl 3860). Matches; covered by the C-1-rev1 unit test.
- **Merge** (merge.rs:464–693): scan slots indexed by Bismark slot (0/3 live, scan 0,3,1,2);
  (77,141) no-align with NO die-if-same-id guard; sum = AS_1+AS_2; overwrite/`best_sum_so_far`/
  `amb_same_thread` machinery; single-mate-XS defaults to own AS (3466–3474); within-thread tie sets
  `amb_same_thread` only if `sum==best`; the two-branch location key (min/max 3527–3532 vs raw 3593);
  unique-best selection (1 entry, 2–4 sort-desc + top-tie boot, >4 die); the 3811–3816 `sum_2nd`
  conditional (best's own only if `> runner_up`); directional reject 1|2 BEFORE `unique_best++`. All
  match the Perl. `unsuitable_sequence_count` incremented once on each ambiguous path.
- **PE converter** (convert.rs:179–313) vs Perl 5916–6024: R1 C→T, R2 forward G→A (`convert_seq_g_to_a`,
  NOT revcomp); `/1/1`/`/2/2` inserted before the trailing `\n` (Perl `s/$/\/1\/1/` semantics); uc
  before tr; the record-1 sanity, skip/upto, prefix, gz plumbing shared with SE. CRLF id keeps `\r`.
- **Paired stream** (align.rs:273–468): R1 canonicalised by trailing `/1` (`SamPair::from_lines`),
  swap when R1 emitted second, die if neither; `is_unmapped_pair` = (77,141); peek-two/advance-two;
  child-pipe drain-then-wait on `finish`, kill+wait on `Drop`. No resource leak / deadlock (the SE
  `early_stop_does_not_deadlock_or_zombie` precedent applies; PE uses the same contract).
- **Driver** (lib.rs:519–952): genome loaded once; 2 instances at slots 0 (`--norc`/CT) + 3
  (`--nofw`/GA); two-file lockstep with the 6-line guard (the two `+` lines NOT guarded, Perl 2611);
  fix_id both, R1 `@`-stripped for the merge id (Perl 2640) vs `@`-bearing-equivalent for aux; output
  naming `_bismark_bt2_pe.bam` / `_bismark_bt2_PE_report.txt`; both temps deleted best-effort (Perl
  2155); aux precedence `--ambiguous` else `--unmapped`; ambig-BAM `_pe.ambig.bam` with `/1\t`,`/2\t`
  strip + RNAME de-convert (within-thread path only, `first_ambig` None on cross-instance tie).
- **PE report BODY** (report.rs:149–213) vs Perl 2185–2312: "Sequence pairs …" wording (7 lines),
  the `Mapping efficiency:\t<p>% \n` trailing space (2205, not the STDOUT `%\n\n`), the strand-label
  JOIN order 0,2,1,3 (CT/GA/CT, GA/CT/CT, GA/CT/GA, CT/GA/GA — Perl 2218), the directional rejected
  line after the strand block, and the cytosine half byte-identical to SE. Matches.
- **aux_filename** (aux_out.rs:40–63): extended in place with `mate: Option<u8>` (SE passes None);
  `_unmapped_reads_1`/`_2`, `_ambiguous_reads_1`/`_2`, un-stripped basename, prefix/basename variants,
  `.gz`. Matches Perl 1853–1938.

## Recommendations (prioritised)
1. **CRITICAL C-1** — fix the PE report header line order + newline placement (swap directional/run-
   with, single-`\n` on run-with, `\n\n` on the library line); add a PE-specific header writer or a
   `paired` flag; correct the `pe_header_two_files` test to the real Perl order. **Blocks the §7 #21
   oxy `_PE_report.txt` gate.**
2. MEDIUM M-1 — accept or document the valid-FastQ `@`-prefix assumption for the R1/R2 aux ids.
3. LOW L-1/L-2/L-3 — no action required.

## Verdict
**REQUEST-CHANGES** — 1 Critical, 0 High.

The PE alignment/merge/extraction/SAM-output spine is byte-faithful to Perl v0.25.1 and exhaustively
unit-pinned. The single blocker is the PE `_PE_report.txt` header, where the shared `write_report_header`
emits the SE line order/newlines instead of the distinct PE order (Perl swaps the `--directional` and
`Bismark was run with…` lines and changes which one carries the `\n\n`). A unit test currently encodes
the wrong order, masking the defect; the oxy PE report gate would catch it. Fix the header writer +
its test, then this is APPROVE.

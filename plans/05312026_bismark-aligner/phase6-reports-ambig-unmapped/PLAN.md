# PLAN — Phase 6: Reports + ambiguous/unmapped outputs + `--ambig_bam` (SE directional)

> **Epic:** `05312026_bismark-aligner/EPIC.md`, Phase 6 — *Reports + ambig/unmapped (SE)* 🎯
> Depends on: **Phase 4** (`Decision` {UniqueBest, Ambiguous, NoAlignment, Rejected} + `Counters`),
> **Phase 5** (the BAM `run_se_directional`/`drive_merge` driver, `generate_sam_header`, the genome loader,
> the per-read original FastQ lines). SE directional. 🎯 **report-parity gate.**

## 1. Goal

Produce the three remaining SE-directional outputs and route the non-unique reads, completing the
SE-directional spine:
1. **The alignment report** (`<name>_bismark_bt2_SE_report.txt`) — the header + final analysis + cytosine
   methylation report (Perl `print_final_analysis_report_single_end`, 1964–2144, plus the report header at
   1642/1711–1729). **Byte-identical** to Perl v0.25.1.
2. **`--unmapped`** → a `<name>_unmapped_reads.fq.gz` of the original reads with no alignment.
3. **`--ambiguous`** → a `<name>_ambiguous_reads.fq.gz` of the original reads that mapped ambiguously.
4. **`--ambig_bam`** → a `<name>_bismark_bt2.ambig.bam` carrying the first ambiguous alignment's **raw**
   (RNAME-de-converted) aligner SAM line per ambiguous read.

This is everything Perl does in `process_single_end_fastQ_file_for_methylation_call` (2393–2482) *around*
the per-read `check_results_single_end` call (the return-code routing, 2451–2465), plus the report.

## 2. Context

### Placement
- **`report.rs`** (new) — `print_final_analysis_report_single_end` (the report body) + the report-header
  lines. Writes the `_SE_report.txt` text.
- **`unmapped.rs`** *(or fold into the driver)* — open/write the `--unmapped`/`--ambiguous` gzipped FastQ
  files; naming per Perl 1644–1709.
- **`output.rs`** (extend) — a **raw-record** write path for `--ambig_bam` (a parsed-but-unvalidated SAM line
  → BAM), since the ambig record is Bowtie 2's line, not a Bismark `XM`/`XR`/`XG` record.
- **`lib.rs`** (extend `run_se_directional`/`drive_merge`) — open the report + optional unmapped/ambiguous/
  ambig-bam writers; route each `Decision`; write the report after the read loop; delete the C→T temp.
- **`merge.rs`** (extend, additive) — `Decision::Ambiguous` carries the first ambiguous alignment's raw
  de-converted SAM line (only needed for `--ambig_bam`).

### Perl source of truth
- `print_final_analysis_report_single_end` (1964–2144) — report body + temp-file deletion.
- Report header: `start_methylation_call_procedure_single_ends` 1642 + 1711–1729 (the `Bismark report for:`
  line, the `--directional` line, the `Bismark was run with Bowtie 2 … options: <aligner_options>` line).
- Routing: `process_single_end_fastQ_file_for_methylation_call` 2444–2479 (return 2 → AMBIG, 1 → UNMAPPED).
- Return codes: `check_results_single_end` 2974–2999 (ambiguous → `--ambiguous` else `--unmapped` else drop;
  no-alignment → `--unmapped` else drop) + 3099–3130 (cross-instance-tie ambiguous; directional-reject →
  return 0 = dropped; could-not-extract → return 0).
- `--unmapped`/`--ambiguous` file naming + open: 1644–1709 (SE single-core = **gzipped** via `gzip -c`).
- `--ambig_bam`: open 1584–1588 (`samtools view -bSh` BAM); first-ambig capture 2806–2808
  (`$fhs->{last_line}`, `s/_(CT|GA)_converted//`); write 2976 (`print AMBIBAM "$first_ambig_alignment\n"`);
  header via `generate_SAM_header` (8456–8482, same `@HD`/`@SQ`/`@PG`).

## 3. Behavior (numbered)

### 3.1 Report (`report.rs`) — byte-identical `_SE_report.txt`
The report is written in two parts to one file (`REPORT`):

**Header** (opened in the driver, Perl 1641–1729):
1. `Bismark report for: <sequence_file> (version: v0.25.1)\n` (1642). **`<sequence_file>` is the read-file
   argument verbatim** → byte-identity needs the identical read arg (composes with the gate's identical
   argv).
2. `Option '--directional' specified (default mode): alignments to complementary strands (CTOT, CTOB) were
   ignored (i.e. not performed)\n` (1712; the pbat/non-dir variants are Phase 8).
3. `Bismark was run with Bowtie 2 against the bisulfite genome of <genome_folder> with the specified options:
   <aligner_options>\n\n` (1722). **🔴 `<genome_folder>` is NOT the raw argv** *(rev 2, both reviewers)*: Perl
   forces a trailing `/` then `chdir`+`getcwd`-absolutizes it, again trailing-`/` (7619–7629). So the report
   prints the **absolute, symlink-resolved path WITH a trailing `/`**. `discovery.rs` uses
   `std::fs::canonicalize` (absolute, but **no** trailing slash) → the Rust report must render
   `format!("{}/", config.genome.genome_dir.display())`. "Identical argv" is necessary but **not sufficient**;
   pin the trailing slash in a unit test + confirm `canonicalize` == `getcwd`-after-`chdir` on the Linux gate.
   `<aligner_options>` = `config.aligner_options` (Phase 1).

**Final analysis** (`print_final_analysis_report_single_end`, 2004–2144) — to `REPORT`:
- `Final Alignment report\n` + `=`×22 + `\n`.
- `Sequences analysed in total:\t<sequences_count>\n`.
- `Number of alignments with a unique best hit from the different alignments:\t<unique_best>\nMapping
  efficiency:\t<%.1f>%\n` (efficiency = `unique_best*100/sequences_count`, or `0` if no sequences). **Compute
  as `(unique as f64)*100.0/(seq as f64)`** (Perl floating division), and the REPORT line ends with a
  **single** `\n` (the `warn` twin at 2024 has `\n\n` — don't copy the wrong one). `%.1f` rounding parity
  (Open Q2): add a **half-boundary unit test** (e.g. `unique=1, seq=8 → 12.5%`) to pin Rust's formatter vs
  Perl/C `printf` **before** the oxy gate; carry a manual half-away rounding helper as contingency.
- `Sequences with no alignments under any condition:\t<no_single_alignment_found>\n`.
- `Sequences did not map uniquely:\t<unsuitable_sequence_count>\n`.
- `Sequences which were discarded because genomic sequence could not be extracted:\t<…>\n\n`.
- `Number of sequences with unique best (first) alignment came from the bowtie output:\n` + the 4 strand
  lines (`CT/CT:\t<n>\t((converted) top strand)` etc.) joined by `\n` + `\n\n`.
- **if directional:** `Number of alignments to (merely theoretical) complementary strands being rejected in
  total:\t<alignments_rejected_count>\n\n`.
- `Final Cytosine Methylation Report\n` + `=`×33 + `\n`.
- `Total number of C's analysed:\t<total>\n\n` where **`total = meCpG+meCHG+meCHH + unmeCpG+unmeCHG+unmeCHH`**
  (2053 — **excludes the Unknown buckets**).
- 4 methylated lines (CpG/CHG/CHH/Unknown) + 4 unmethylated lines.
- 4 percentage lines (`%.1f`): `C methylated in CpG/CHG/CHH context` + `…in Unknown context (CN or CHN)`,
  each computed+printed **iff `(me+unme) > 0`**, else the literal `Can't determine percentage of methylated
  Cs in <context> if value was 0\n`. **🔴 Gate on `(me+unme) > 0`, NOT on "the percentage is non-zero"**
  *(rev 2, both reviewers)*: an all-unmethylated bucket (`me==0, unme>0`) must print `…\t0.0%`, not
  "Can't determine" (Perl's print guard `if ($percent)` is true for the string `"0.0"`; only `undef` —
  i.e. `me+unme==0` — is false).
- trailing `\n\n`. **The `seqID_contains_tabs` warning line (2140–2143) is NEVER emitted in v1 SE-directional**
  *(rev 2, Open Q3 RESOLVED, both reviewers)*: `biTransformFastQFiles` checks for a tab AFTER `fix_IDs`
  (Perl 5585→5607), which has already collapsed tabs to `_`; Phase 2's `convert::seqid_tab_count` is
  structurally 0. Wire that counter into the conditional (forward-safe) rather than hard-coding "no warning".
- **🔴 Trailing wall-clock line** *(rev 2, Reviewer B — Critical)*: Perl appends
  `Bismark completed in ${days}d ${hours}h ${mins}m ${secs}s\n` to the SAME `_SE_report.txt` at teardown
  (926–927; `REPORT` is never `close`d). So the report's last line is **wall-clock-dependent**. The Rust port
  must (a) **emit a matching line** in the same format (timing its own run), and (b) the gate (§7 #9) must
  **normalize `^Bismark completed in ` out of BOTH sides** (exactly like the samtools `@PG` filter in Phase 5)
  — the report gate **cannot be a raw byte diff**. Unit tests pin every report line *except* this one.

### 3.2 Per-read routing (driver, replacing Phase 5's "only UniqueBest" arm)
For each `Decision` (Perl 2451–2465 + 2974–2999 precedence):
- **`UniqueBest`** → already written to the BAM (Phase 5).
- **`Ambiguous`** → (a) if `--ambig_bam`: write the first ambiguous alignment's raw de-converted SAM line to
  the ambig BAM (§3.4); (b) **routing precedence:** if `--ambiguous` → write the original FastQ record to the
  ambiguous file; **else if** `--unmapped` → write it to the unmapped file; else nothing.
- **`NoAlignment`** → if `--unmapped` → write the original FastQ record to the unmapped file; else nothing.
- **`Rejected`** (directional wrong-strand) → **nothing** (counted only; Perl returns 0, 3116).

### 3.3 Unmapped/ambiguous FastQ record (Perl 2452–2455)
Write exactly four lines: `@<fixed_id>\n` + `<original_seq>\n` + `<raw_+_line_verbatim>` + `<qual>\n`, where:
- `<fixed_id>` = the `fix_id`'d, `@`-stripped identifier (the driver already computes this).
- `<original_seq>` = the **original** read (chomped, **NOT** upper-cased — distinct from the uc seq fed to
  the merge).
- `<raw_+_line_verbatim>` = the FastQ 3rd line as read (Perl prints `$identifier_2` *with* its own newline).
- `<qual>` = the chomped quality line.
Files are **gzipped** (SE single-core, 1671–1672): `<name>_unmapped_reads.fq.gz` /
`<name>_ambiguous_reads.fq.gz` (`.fa` for FastA = Phase 9), with `--prefix`/`--basename` variants
(1645–1665). **🔴 `<name>` is the UN-stripped basename** *(rev 2, Reviewer B)*: Perl uses
`$unmapped_file = $filename` (1645) and appends `_unmapped_reads.fq` **without** stripping the FastQ suffix
(unlike the BAM/report stems) — so `reads.fq.gz` → `reads.fq.gz_unmapped_reads.fq.gz`. **Do NOT reuse
`strip_fastq_suffix`** (used for the BAM name); pin the exact derived names (+ `--prefix`/`--basename`).
**🔴 The driver currently discards the original seq** *(rev 2, both reviewers)*: Phase 5's `drive_merge`
keeps only `seq_uc` (uppercased); Phase 6 must also retain `convert::chomp_newline(&seq).to_vec()` (the
chomped, **non-uppercased** original) for `<original_seq>`. **The `+` line is written VERBATIM** (the raw
`plus` buffer with its own `\n`/`\r\n`) — no chomp, no appended `\n`; seq/qual DO get an explicit `\n`.

### 3.4 `--ambig_bam` (raw passthrough)
- Open `<outfile_stem>_bismark_bt2.ambig.bam` (Perl 1585–1588; `s/sam$/ambig.bam/`).
- Header = the **same** `generate_sam_header` output (`@HD`/`@SQ`/`@PG`, §Phase 5) — reuse `header.clone()`.
- **🔴 Written ONLY on the within-thread ambiguity path** *(rev 2, Reviewer A — Critical)*: Perl's lone SE
  `print AMBIBAM` is at **2976**, inside the `$amb_same_thread` block (2968–2988). The **cross-instance-tie**
  ambiguous block (3091–3107) writes **NOTHING** to the ambig BAM (the read still goes to the FastQ aux
  file). So the seam is `Decision::Ambiguous { first_ambig: Option<String> }` with `first_ambig = Some(line)`
  **only** from `merge.rs`'s `amb_same_thread` site (235–237) and `None` from the cross-tie site (253–255);
  the driver writes the ambig record **iff `first_ambig.is_some()`** — NOT "per ambiguous read".
- **🔴 `first_ambig` is captured at BOTH score-setting arms** *(rev 2, Reviewer B)*: Perl (re)assigns
  `$first_ambig_alignment` at 2806–2810 (first alignment seen) **and** 2822–2826 (each *strictly-better* AS),
  never on an equal AS. In `merge.rs` these are the `None =>` (line ~183) and `if alignment_score > best`
  (~190) arms — capture `rec.raw_line` at both, gated on `want_ambig`.
- The captured line is the **raw `SamRecord.raw_line`** (Phase 3 stores it **chomped, RNAME suffix intact**),
  with `_(CT|GA)_converted` removed. Perl's `s/_(CT|GA)_converted//` is **non-global, unanchored** on the
  whole line (first occurrence). For option (a) (parse to a `RecordBuf`), strip the suffix off the **RNAME
  field only** before the tid lookup (byte-equivalent for any real RNAME; avoids a pathological mid-line
  match) — do NOT `$`-anchor a whole-line `strip_suffix`.
- **This record carries Bowtie 2's tags (`AS`/`XS`/…) in Bowtie 2's order, not Bismark `XM`/`XR`/`XG`** → it
  must **bypass `BismarkRecord`** validation, and `write_raw_sam_line_to_bam` must preserve the original
  FLAG/POS/MAPQ/CIGAR + tags **verbatim, in input order** (noodles `Data` is insertion-ordered). See Open Q1
  (resolved → (a)) **and the cross-crate scope note in §4**.

### 3.5 Temp-file cleanup
After the read loop, delete the C→T temp file (Perl `print_final_analysis_report_single_end` 1974–1981).
*(Phase 5 currently leaves it; move the deletion here, or to the driver's per-file teardown.)*

### Edge cases
- `sequences_count == 0` → mapping efficiency `0` (not a div-by-zero) (2017).
- A context bucket with `me+unme == 0` → the literal "Can't determine…" line, not a percentage.
- `--ambiguous` + `--unmapped` both set → ambiguous reads go to the **ambiguous** file only (precedence).
- `--ambig_bam` without `--ambiguous`/`--unmapped` → still writes the ambig BAM (independent).
- A read that is `Ambiguous` but neither `--ambiguous` nor `--unmapped` set → counted, written nowhere.
- Report `%.1f` rounding must match Perl `sprintf("%.1f", …)` (round-half-to-even? — Perl uses C `printf`,
  round-half-away-from-zero on most libc; **verify against the oxy gate** — same `%.15g`/`%.1f` care as the
  bedgraph/c2c ports).

## 4. Signatures (proposed)

```rust
// report.rs
pub struct ReportHeader<'a> { pub sequence_file: &'a str, pub genome_folder: &'a str,
                              pub aligner_options: &'a str, pub library: LibraryType }
pub fn write_report_header(w: &mut impl Write, h: &ReportHeader) -> Result<()>;        // 1642/1711-1729
pub fn print_final_analysis_report_single_end(w: &mut impl Write, c: &Counters, directional: bool) -> Result<()>; // 1964-2144

// merge.rs (additive): Ambiguous carries the first-ambiguous raw de-converted line (for --ambig_bam)
pub enum Decision { UniqueBest(BestAlignment), Ambiguous { first_ambig: Option<String> }, NoAlignment, Rejected }

// unmapped/ambiguous writers (gzip; decompressed-content gated)
fn write_fastq_record(w: &mut impl Write, fixed_id: &str, seq: &[u8], plus_line: &[u8], qual: &[u8]) -> Result<()>;

// output.rs (extend): raw-record ambig BAM (bypasses BismarkRecord validation)
pub fn write_raw_sam_line_to_bam(writer: &mut BamWriter<W>, de_converted_line: &str, refid: &HashMap<String,usize>) -> Result<()>;
```

**🔴 Cross-crate scope (rev 2, Reviewer A — Critical):** option (a) **cannot** be implemented in `output.rs`
alone. `bismark-io::BamWriter::write_record` accepts only `&BismarkRecord` (its `inner` is private), and
**every** `BismarkRecord` constructor validates `XR`/`XG`/`XM` — which the raw Bowtie 2 line lacks. So Phase 6
must add a **new public API to the shared `bismark-io` crate**: recommend **`BamWriter::write_raw_record(&RecordBuf)`**
(keeps the unvalidated-passthrough concept out of the validated `BismarkRecord` type; smallest blast radius).
This touches a crate the shipped sibling ports depend on → **version bump** + a `bismark-io` unit test.
**RESOLVED (Open Q5, 2026-06-01): Felix chose to add `write_raw_record` to `bismark-io` and ship
`--ambig_bam` in Phase 6** (not deferred).

## 5. Implementation outline
1. **`merge.rs`:** change `Decision::Ambiguous` → `Ambiguous { first_ambig: Option<String> }`; capture the
   first ambiguous alignment's `raw_line` with `_(CT|GA)_converted` stripped (Perl 2806–2808), populated only
   when needed (gate on a `want_ambig` flag passed in, to avoid the clone when `--ambig_bam` is off). Update
   Phase-4 tests for the new variant shape.
2. **`report.rs`:** the header writer + `print_final_analysis_report_single_end` (verbatim text + `%.1f`).
   Unit-test the exact bytes for a canned `Counters` (incl. the 0-sequences and 0-context branches).
3. **Unmapped/ambiguous writers:** filename derivation (1645–1709) + the gzip writer (flate2, same as Phase 2
   `--gzip`) + `write_fastq_record` (original seq, verbatim `+` line).
4. **`output.rs`:** `write_raw_sam_line_to_bam` — parse the de-converted line into a noodles `RecordBuf`
   (qname/flag/rname→tid/pos/mapq/cigar/seq/qual + verbatim optional tags) and write it **without** the
   `BismarkRecord` XR/XG/XM validation (Open Q1).
5. **Driver (`lib.rs`):** open `REPORT` (+ optional UNMAPPED/AMBIG/AMBIBAM, with the AMBIBAM header) before
   the loop; write the report header; route each `Decision` (§3.2); after the loop write the final analysis,
   delete the C→T temp, and `finish()` the ambig BAM. Shrink `deferred_flags` (`--unmapped`/`--ambiguous`/
   `--ambig_bam` are now active).
6. **Tests** (§7) — report byte tests, routing/precedence, FastQ-record bytes, ambig-BAM raw record, +
   extend the oxy gate to diff the report + (decompressed) unmapped/ambiguous + the ambig BAM.

## 6. Efficiency
Negligible — the report is O(1) formatting; routing is O(reads); the gzip writers stream. No new genome
passes. The `first_ambig` clone is gated on `--ambig_bam`.

## 7. Validation
| # | Verify | How | Expected |
|---|--------|-----|----------|
| 1 | Report body bytes (directional) | unit: canned `Counters` → exact string (incl. `=`×22/`=`×33; `Total C's` EXCL. Unknown; the rejected line present; `Mapping efficiency` single `\n`) | matches Perl 2004–2144 verbatim, **modulo the trailing wall-clock line** |
| 2 | Report 0-sequences | unit | `Mapping efficiency:\t0%`; no div-by-zero |
| 3 | Report 0-context bucket (`me+unme==0`) | unit | the literal "Can't determine…" line |
| 3b | **All-unmethylated bucket (`me==0, unme>0`)** | unit | prints `…\t0.0%` (NOT "Can't determine") |
| 3c | **All-Unknown corner** | unit: only Unknown buckets nonzero | `Total number of C's analysed:\t0`; CpG/CHG/CHH all "Can't determine" |
| 3d | **`%.1f` half-boundary** | unit: `unique=1,seq=8 → 12.5%`; a methylation `.x5` tie | matches Perl `printf` (pin rounding before the gate) |
| 4 | Report header | unit | the 3 header lines; `aligner_options` exact; **genome path = absolute WITH trailing `/`** |
| 5 | Routing precedence | unit | Ambiguous + both flags → ambiguous file only; Ambiguous + only `--unmapped` → unmapped file |
| 6 | `Rejected`/could-not-extract written nowhere | unit | counted, no FastQ/BAM record |
| 7 | Unmapped/ambiguous FastQ record bytes | unit (decompressed) | `@<fixed_id>\n<orig_seq>\n<+line_verbatim><qual>\n`; seq NOT uc'd; `+` line verbatim |
| 7b | **FastQ record CRLF + missing-final-newline** | unit | `+` line retains `\r\n`; seq/qual `\r`-chomped + `\n`; EOF record correct |
| 7c | **Unmapped/ambiguous filename derivation** | unit | un-stripped basename (`reads.fq.gz_unmapped_reads.fq.gz`); `--prefix`/`--basename` variants |
| 8a | **Ambig BAM — within-thread ambiguous** | unit: read back | exactly **one** record; RNAME de-converted; Bowtie tags preserved verbatim + in order |
| 8b | **Ambig BAM — cross-instance-tie ambiguous** | unit | **zero** ambig records (but the read IS in the FastQ aux file) |
| 8c | **`first_ambig` capture ordering** | unit: instance-1 beats instance-0 then ties | the ambig record is instance-1's line (strict-improvement arm), not instance-0's |
| 8d | **`write_raw_record` (bismark-io)** | unit in `bismark-io`: a multi-tag Bowtie 2 line round-trips | FLAG/POS/MAPQ/CIGAR + tag order/values intact through `samtools view -h` |
| 9 | temp C→T file unlinked (best-effort) | driver unit | deleted after the report; a failed unlink does NOT error the run |
| 10 | **🎯 oxy report+aux gate** | extend the §18 harness: diff the `_SE_report.txt` (**filter `^Bismark completed in ` both sides**), `samtools view -h` of the ambig BAM (samtools `@PG` filtered), and `zcat` of the unmapped/ambiguous files, vs Perl (identical argv) | byte-identical |

## 8. Assumptions
**From epic:** Perl v0.25.1 + Bowtie 2 2.5.5 oracle; byte-identity on decompressed content; adjudicate on
Linux/oxy; identical argv (so the report's embedded `<sequence_file>`/`<genome_folder>` + the `@PG CL:` match).
**Phase-specific:** SE directional; unmapped/ambiguous files are **gzipped** (SE single-core) → gate on
decompressed content (flate2 ≠ Perl gzip, same as Phase 2); the report counters all already exist
(Phase 4/5 `Counters`); `--basename`/`--prefix` honored; pbat/non-dir report lines + the multicore
plain-then-merge path are Phase 8/9.

## 9. Questions or ambiguities
- **(RESOLVED 2026-06-01 → (a))** **`--ambig_bam` raw-record write path.** Felix chose **(a)**: construct a
  bare noodles `RecordBuf` from the de-converted SAM line and write it via a thin raw path (bypassing the
  `BismarkRecord` XR/XG/XM validation), since the ambig record is Bowtie 2's raw line (`AS`/`XS` tags).
  `--ambig_bam` ships **in** Phase 6 (not deferred). §3.4/§4/§5 step 4 reflect this.
- **(Open Q2 — refined, rev 2)** `%.1f` rounding parity (half-away vs half-even). Add a **local
  half-boundary unit test** (§7 #3d) to pin it BEFORE the oxy gate (so a gate diff isn't ambiguous between
  rounding and arithmetic); carry a manual half-away helper as contingency. Glibc `printf` + Rust `{:.1}`
  likely agree (both ties-to-even) but the bedgraph/c2c ports were bitten here.
- **(Open Q3 — RESOLVED never-fires, rev 2)** The `seqID_contains_tabs` warning (2140–2143) never fires in v1
  SE-directional (`biTransformFastQFiles` checks for a tab *after* `fix_IDs`, 5585→5607; `convert::seqid_tab_count`
  is structurally 0). Wire that existing counter into the conditional (forward-safe), don't hard-code.
- **(Open Q5 — RESOLVED 2026-06-01 → add to `bismark-io`)** Felix chose to implement (a) **fully**: add
  `BamWriter::write_raw_record(&RecordBuf)` to the shared `bismark-io` crate (with its own unit test + a
  version bump), and ship `--ambig_bam` in Phase 6. The `write_raw_sam_line_to_bam` helper in the aligner
  crate builds a bare `RecordBuf` from the de-converted line and writes it via this new API.
- **(Open Q4)** Module split (`report.rs` + `unmapped.rs`) vs folding into the driver/`output.rs`.
  *Assumption:* `report.rs` separate (byte-tested in isolation); unmapped/ambiguous helpers in the driver.

## 10. Self-Review
- **Logic:** routing precedence (ambiguous→ambiguous-else-unmapped; no-align→unmapped; rejected→drop) traced
  to 2974–2999; report fields + `%.1f` + the "Total C's excludes Unknown" subtlety traced to 1964–2144. ✓
- **Edge cases:** 0 sequences, 0-context buckets, both-flags precedence, ambig-bam-without-flags, rejected
  drop. ✓
- **Integration:** reuses Phase-4 `Counters` (no new counters), Phase-5 `generate_sam_header` + the driver +
  the original FastQ lines; the only merge change is the additive `Ambiguous { first_ambig }`. ✓
- **Risks:** (a) the `--ambig_bam` raw record (Open Q1) is the one genuinely new mechanism; (b) gzip-content
  vs raw-bytes gating for the FastQ aux files; (c) the report's env-specific paths need identical argv (the
  gate already does this). The report `%.1f`/`Total C's` arithmetic is the byte-identity-critical part →
  unit-pinned + oxy-gated.

## 12. Implementation Notes (2026-06-01)

**Status: COMPLETE + GATED — 131 unit + 19 integration tests; clippy `-D warnings` + `cargo fmt --check`
clean; workspace builds.** Dual code-review both **APPROVE** (no Critical/High); plan-manager **COMPLETE**.

### §7 #10 oxy byte-identity gate — ✅ ALL-PASS 2026-06-01
`bismark_rs` (built on oxy from the worktree) vs Perl Bismark v0.25.1 + Bowtie 2 2.5.5 + samtools 1.23.1,
100k real GRCh38 WGBS SE-directional reads, identical argv + `--unmapped --ambiguous --ambig_bam`. All five
outputs byte-identical (decompressed/normalised): **BAM** PASS; **`_SE_report.txt`** PASS (filtering
`^Bismark completed in ` + the samtools `@PG`); **`--unmapped`** PASS (16,480 lines); **`--ambiguous`** PASS
(44,988 lines); **`--ambig_bam`** PASS (10,069 records — the raw-record round-trip confirmed via
`samtools view -h`). The full-scale SE+PE+RRBS gate is Phase 10.

### What was built
- **`bismark-io` `BamWriter::write_raw_record(&RecordBuf)`** (the resolved Open-Q1(a) shared-crate API) —
  unvalidated passthrough, bypasses the `BismarkRecord` XR/XG/XM check. `bismark-io` bumped **beta.8 → beta.9**;
  the 4 dependent pins (aligner/dedup/extractor/methylation-consistency) updated. +1 unit test.
- **`merge.rs`** — `Decision::Ambiguous { first_ambig: Option<String> }`; `first_ambig` captured at BOTH the
  first-alignment arm and the strict-improvement arm (gated on a new `want_ambig` param), `Some` only on the
  within-thread path, `None` on cross-tie. +4 capture tests; existing Ambiguous matches updated.
- **`report.rs`** (new) — `write_report_header` + `print_final_analysis_report_single_end` (byte-exact;
  `Total C's` excludes Unknown; `(me+unme)>0` percentage gate; `f64` mapping efficiency; genome path rendered
  with a trailing `/`) + `write_completion_line` (the wall-clock line). 11 unit tests incl. the exact-report,
  0-seq, all-unmethylated-`0.0%`, all-Unknown, and `%.1f` half-boundary (`12.5%`) cases.
- **`aux_out.rs`** (new) — `aux_filename` (UN-stripped basename) + `write_fastq_record` (non-uc seq, verbatim
  `+` line). 5 unit tests (filename variants, CRLF).
- **`output.rs`** — `write_raw_sam_line_to_bam` / `build_raw_record` (de-convert RNAME field, FLAG/POS/MAPQ/
  CIGAR/SEQ/QUAL + tags verbatim+in-order; rejects unsupported tag types). +2 unit tests.
- **`config.rs`** — additive `RunConfig.{unmapped, ambiguous, ambig_bam}`; `deferred_flags` shrunk (those 3
  now active).
- **`lib.rs`** — `Sinks` (BAM + optional ambig-BAM/unmapped-gz/ambiguous-gz); `open_sinks`; `derive_output_path`
  (report/BAM/ambig names) vs `aux_out::aux_filename` (aux names); driver opens the report, writes the header,
  routes each `Decision` (UniqueBest→BAM; Ambiguous→ambig-BAM-if-Some + ambiguous-else-unmapped; NoAlignment→
  unmapped; Rejected→drop), writes the final analysis + wall-clock line, deletes the C→T temp (best-effort).

### Deviations / notes
- The wall-clock line value is timed via `Instant` (gate normalises `^Bismark completed in `); for multi-file
  SE every report ends with one (Perl writes it only to the last report) — immaterial since the gate strips it.
- `aux.rs` was named **`aux_out.rs`** (`aux` is a Windows-reserved filename — the classic Rust footgun).
- The Phase-5 integration tests were updated: the C→T temp is now **deleted** (assertion inverted), the report
  file is asserted, and `deferred_flag_emits_notice` switched to `--nucleotide_coverage` (still deferred;
  `--unmapped` is now active).
- Two integration tests added: unmapped-routing+report end-to-end (decompress the `.fq.gz`, check the report)
  and ambiguous+`--ambig_bam` end-to-end.
- `report.rs` allows `clippy::write_with_newline` (explicit `\n`s keep the byte-exact text auditable).

## 11. Revision History
- **rev 2 (2026-06-01)** — folded dual plan-review (`PLAN_REVIEW_A.md`/`PLAN_REVIEW_B.md`; both verified the
  routing/report layout, found **3 Criticals + several Importants**, no contradictions). All accepted:
  - 🔴 **Trailing wall-clock line** (B, Perl 926–927): the SE report ends with `Bismark completed in …`;
    Rust must emit a matching line + the gate must filter `^Bismark completed in ` both sides (§3.1, §7 #1/#10).
  - 🔴 **`<genome_folder>` absolute + trailing `/`** (both, Perl 7619–7629): `canonicalize` drops the slash;
    render `format!("{}/", genome_dir)` (§3.1, §7 #4).
  - 🔴 **Ambig BAM written ONLY on within-thread ambiguity** (A, Perl 2976 vs no-write at 3091): `first_ambig
    = Some` only from the `amb_same_thread` site, `None` from cross-tie; write iff `Some` (§3.4, §7 #8a/#8b).
  - 🔴 **Open-Q1(a) needs a new `bismark-io` API** (A): `BamWriter::write_raw_record(&RecordBuf)` — cross-crate
    change + version bump; (b)/defer is the fallback (§4, §9 Open Q5).
  - **`first_ambig` captured at BOTH 2806 + 2822** (B, strict-improvement arm) (§3.4, §7 #8c).
  - **`(me+unme)>0` percentage gate**, all-unmethylated → `0.0%` not "Can't determine" (both) (§3.1, §7 #3b).
  - **Unmapped/ambiguous filename = UN-stripped basename** (B): no `strip_fastq_suffix` (§3.3, §7 #7c).
  - **Retain the non-uc original seq + verbatim `+` line** (both): the driver currently keeps only `seq_uc`
    (§3.3, §7 #7).
  - **Open Q3 RESOLVED** (never fires; wire `seqid_tab_count`); **`%.1f` half-boundary unit test** added (#3d);
    `Mapping efficiency` single `\n` + f64 division; CRLF/EOF FastQ test (#7b); best-effort temp unlink (#9).
- **rev 0 (2026-06-01)** — initial plan. Manual review: Felix resolved **Open Q1 → (a)** (`--ambig_bam`
  ships via a raw `RecordBuf` write path, not deferred). Then dual plan-review (folded into rev 2).

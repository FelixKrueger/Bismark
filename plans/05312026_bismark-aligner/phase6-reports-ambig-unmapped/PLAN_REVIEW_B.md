# PLAN_REVIEW_B — Phase 6 (Reports + ambig/unmapped + `--ambig_bam`, SE directional)

**Reviewer:** B (independent, fresh context)
**Plan:** `plans/05312026_bismark-aligner/phase6-reports-ambig-unmapped/PLAN.md` (rev 0)
**Oracle:** Perl `bismark` v0.25.1 (`/Users/fkrueger/Github/Bismark-aligner/bismark`)
**Verdict:** Solid plan, correct on the routing precedence and most field details, but it has **two
byte-identity gaps that will fail the §18-style report gate as written** (the wall-clock "Bismark completed
in…" trailing line, and the `<genome_folder>` trailing-slash/absolutization). One **logic gap** in the
`first_ambig` capture (only the first-alignment arm is cited; the strict-improvement arm 2822 is missing).
None are deep design flaws — all are localizable fixes — but they are load-bearing for the gate.

---

## 1. Logic review

### 1.1 Routing precedence — CORRECT, and the plan got the subtle part right
I initially suspected a discrepancy because the Perl *caller* (`process_single_end_fastQ_file_for_methylation_call`,
2451/2460) routes with `if ($ambiguous and $return == 2) … elsif ($unmapped and $return == 1)` — two
conditions on *different* return values, which on its face would NOT send an ambiguous read to the unmapped
file. But the precedence is actually resolved **inside** `check_results_single_end` via the return code:

- Ambiguous (same-thread, 2979–2987 **and** cross-instance tie, 3098–3106): `return 2` if `--ambiguous`,
  **`elsif ($unmapped) return 1`**, else `return 0`.
- No-alignment (2995–2999): `return 1` if `--unmapped`, else `0`.
- Directional reject (3112–3117): `return 0`.
- Could-not-extract (3127–3130): `return 0`.
- Mapped+written (3147–3149): `return 0`.

So an ambiguous read with only `--unmapped` set DOES get `return 1` → written to the unmapped file. **The
plan's §3.2 precedence (ambiguous→ambiguous-else-unmapped; no-align→unmapped; rejected/extract-fail→drop) is
faithful, and validation #5 ("Ambiguous + only `--unmapped` → unmapped file") matches Perl.** ✓

One thing the plan should state explicitly (it is implied but never written): in Rust, `Decision::Ambiguous`
/ `NoAlignment` carry **no flag state**, so the driver — not the merge — must encode this precedence
(it cannot be a straight transliteration of the Perl caller's `$return == 2`/`== 1` test). The §3.2 driver
arms do this correctly, but make the "precedence lives in the driver, not the merge" point explicit so the
implementer doesn't accidentally re-create the Perl caller's two-different-return-values shape and lose the
ambiguous→unmapped case.

### 1.2 `--ambig_bam` first-ambiguous capture — **citation gap (Important)**
The plan (§3.4, §5 step 1) says capture "the first ambiguous alignment's `raw_line` … (Perl 2806–2808)".
That cite is **incomplete**. `$first_ambig_alignment` is (re)assigned in **two** places inside the
per-instance loop:

- **2806–2810** — the `!defined $best_AS_so_far` arm (first alignment seen across all instances).
- **2822–2826** — the **strict-improvement** arm (`$alignment_score > $best_AS_so_far`).

It is **not** re-assigned on an *equal* alignment (the merge's `>=`-but-not-`>` case). So `first_ambig` tracks
"the SAM line of the alignment that established the current best score." If the implementer only mirrors 2806,
a read whose best score is set by instance 0 and then **beaten** by instance 1 (which then ties → ambiguous)
will emit instance 0's line to the ambig BAM, but Perl emits instance 1's. In `merge.rs` these are exactly the
two `overwrite`/`best_as_so_far` arms (`None =>` line 183 and `if alignment_score > best` line 190). **The
plan must direct capture at BOTH arms, gated on `want_ambig`.** Add this to §5 step 1 and cite 2822–2826.

Cross-instance-tie subtlety to call out for the test: two equally-good alignments in different instances →
the *first* one sets the score (captured), the second is `==` (no re-capture) → `first_ambig` = the first
instance's line. Validation #8 should pin this ordering (capture is at first-set/strict-improve, never on a
tie), not just "RNAME de-converted; tags preserved."

### 1.3 RNAME de-conversion of the ambig line — verify the regex shape
Perl (2808/2824): `$first_ambig_alignment =~ s/_(CT|GA)_converted//` — a **non-global**, **non-anchored**
substitution on the **whole chomped line**, removing the *first* `_CT_converted`/`_GA_converted` occurrence
anywhere. In practice that is the RNAME field. The plan §3.4 phrases it as "stripped from RNAME," which is
*operationally* right but understates the mechanism. Two concrete reproduction notes the plan should add:

1. Operate on the **raw `SamRecord.raw_line`** (which Phase 3 stores **chomped + pre-de-conversion** — confirmed
   in `align.rs`: `raw_line: trimmed.to_string()`, RNAME kept raw incl. the suffix). So the line still has
   `chrX_CT_converted` — good, that is what Perl's `s///` sees.
2. Reproduce the **first-occurrence-only, unanchored** semantics. If the chosen implementation
   parse-and-rebuilds the line (Open Q1 → option (a): bare `RecordBuf`), strip the suffix off the **RNAME field
   only** before the tid lookup — that is byte-equivalent for any real RNAME and avoids a pathological match
   inside QNAME/SEQ. Either way, do **not** use a `$`-anchored `strip_suffix` on the whole line (the suffix is
   mid-line, not at end-of-line). State which approach §5 step 4 takes.

### 1.4 Report field order/content — mostly correct, but watch the STDOUT-vs-REPORT split
I diffed §3.1 against 2004–2143 line by line. The body content and order are right, including:
- `Mapping efficiency` line uses a **single** trailing `\n` to REPORT (2025), whereas the `warn` twin (2024)
  has `\n\n`. The plan must emit `…Mapping efficiency:\t<…>%\n` (one newline) to REPORT. The plan text in
  §3.1 writes "`Mapping efficiency:\t<%.1f>%\n`" — correct (one `\n`), but flag it because 2024 vs 2025 differ
  and it is an easy copy-from-the-wrong-line bug.
- The four strand lines are `join("\n", …)` then `,"\n\n"` (2044) → the block ends with the 4th line + `\n\n`.
  Plan §3.1 says "joined by `\n` + `\n\n`." ✓
- "Total number of C's analysed" **excludes Unknown** (2053) — plan correct (§3.1, validation present
  indirectly). ✓
- The four `%.1f` percent lines guard on `(me+unme) > 0` *implicitly* via Perl's `if ($percent_meCpG)` truthiness
  on the sprintf result (2099 etc.), NOT a direct `>0` test. **This matters**: if `me>0` but the computed
  percentage rounds to `0.0`, Perl's `if ($percent_meCpG)` is **false** for the string `"0.0"` (Perl numeric
  truthiness: `"0.0"` is **true** as a string! only `"0"`, `""`, `0`, `undef` are false). Actually `"0.0"` is a
  true string in Perl, so the percentage prints. The only false case is when the variable is **undef** —
  which happens exactly when `(me+unme) == 0` (the sprintf was skipped). **So the plan's "only if `(me+unme) > 0`"
  gate is the correct reduction** ✓ — but the implementer must gate on `(me+unme) > 0`, NOT on "percentage != 0",
  or a genuine `0.0%` context (all-unmethylated bucket) would wrongly print the "Can't determine…" line. Add a
  validation row for the `me=0, unme>0` → `0.0%` printed case (distinct from `me+unme=0` → "Can't determine").
  This is a **silent-wrong-result trap** the current validation table does not cover (see §4).

### 1.5 Report HEADER ordering & sources — one **byte-identity gap (Critical)** + one OK
Header lines (opened in the driver, written across 1642 / 1711–1719 / 1721–1729), in REPORT-write order:
1. `Bismark report for: $sequence_file (version: $bismark_version)\n` (1642). `$bismark_version='v0.25.1'`
   (line 28) → matches `crate::BISMARK_VERSION`. `$sequence_file` is the **full read-file argument as given**
   (the path incl. any dir prefix, NOT the basename — confirmed: it is the arg passed into
   `start_methylation_call_procedure_single_ends`, used verbatim at 1642). The driver has this as the
   `read_file` string. ✓ — composes with identical argv.
2. `Option '--directional' specified …\n` (1712). ✓ (pbat/non-dir variants = Phase 8.)
3. `Bismark was run with Bowtie 2 against the bisulfite genome of $genome_folder with the specified options:
   $aligner_options\n\n` (1722).

**The `<genome_folder>` value is NOT the raw argv.** Perl absolutizes it (`chdir`+`getcwd`, 7623–7629) **and
guarantees a trailing `/`** (7619–7621, 7625–7627). The Rust discovery uses `std::fs::canonicalize(genome_arg)`
(`discovery.rs:112`) → an absolute `PathBuf` **without a trailing slash**. So `config.genome.genome_dir.display()`
renders `/abs/path/to/genome` where Perl renders `/abs/path/to/genome/`. **The report's bowtie2 line will differ
by exactly the trailing slash even with identical argv.** The plan's "identical genome arg required" (§3.1) is
**necessary but not sufficient** — it never specifies that `ReportHeader.genome_folder` must be
`genome_dir` rendered **with a trailing `/`**. Fix: §3.1/§4 must state the report uses
`format!("{}/", config.genome.genome_dir.display())` (or push a trailing separator), and the implementer must
confirm `canonicalize` matches Perl's `getcwd`-after-`chdir` on the gate platform (symlink resolution; the
epic's "adjudicate on Linux" rule covers this). Pin the trailing slash in a unit test (validation #4).
`$aligner_options` = base option string (no per-instance `--norc`/`--nofw`; those are added downstream at
6282/6465) → `config.aligner_options`. ✓

### 1.6 **The wall-clock "Bismark completed in…" trailing line — Critical, UNADDRESSED**
Lines **926–927**: `print REPORT "Bismark completed in ${days}d ${hours}h ${mins}m ${secs}s\n"`. `REPORT` is
**never explicitly closed** (no `close REPORT` anywhere in `bismark`), so this line is appended to the SAME
`_SE_report.txt` at parent-process teardown, **after** `print_final_analysis_report_single_end` finished
(which ends at `print REPORT "\n\n"`, 2137). **Therefore the SE report file's last content line is a
wall-clock-dependent timing line.** The plan's §3.1 ends the report at the `\n\n` + the tab-warning and never
mentions this. Two consequences, both must be in the plan:
- The Rust report will be **structurally short by one line** unless it emits an equivalent "Bismark completed
  in …" line (analogous to the bismark2report port's "modulo the one localtime line").
- The §18-style report gate (validation #9) **cannot be a raw byte diff** — it must normalize/strip the
  "Bismark completed in" line on **both** sides (exactly as Phase 5 greps out `@PG.*ID:samtools`). The plan
  #9 currently says "byte-identical (report = identical argv)" with **no** normalization — as written the
  report gate will never pass. Add: (a) emit a matching timing line (format `${d}d ${h}h ${m}m ${s}s`), (b) the
  gate filters `^Bismark completed in ` from both sides, (c) unit tests pin every report line **except** that
  one. This is the single biggest gate risk in the phase.

### 1.7 Temp-file cleanup (§3.5)
Perl deletes the C→T temp inside `print_final_analysis_report_single_end` (1974–1981). Phase 5's driver
currently does NOT delete it (it lives in `config.output.temp_dir`). Moving the deletion to the driver's
per-file teardown (after the report write) is fine and matches Perl's *effect* (the warn lines 1977/1980 are
STDERR, not REPORT, so not gated). One note: Perl unlinks `"$temp_dir$C_to_T_infile"` — the Rust must delete
the **same** path Phase 2 created (`converted.path`), which the driver already holds. The deletion should be
**best-effort** (Perl warns, does not die, on failure) — do not propagate an error if the unlink fails. State
that in §3.5.

### 1.8 Unmapped/ambiguous FastQ record (§3.3) — correct, with two implementer notes
Perl 2452–2455 / 2461–2464 writes four parts:
- `"\@$identifier\n"` — `$identifier` is post-`fix_IDs` and post-`s/^\@//` (2442) → fresh `@` prepended. The
  driver already computes `identifier` (the `@`-stripped `fix_id`). ✓
- `"$sequence\n"` — `$sequence` is **chomped (2438) but NOT uppercased** (the `uc` is only in the 2444 call
  arg). **The current driver keeps only `seq_uc`** (uppercased) + `qual_bytes`; it does **not** retain the
  original chomped (non-uc) seq. §3.3 correctly says "original NOT uc'd," but the plan must explicitly tell
  the implementer to retain `convert::chomp_newline(&seq).to_vec()` (the raw chomped seq) for this path — it
  is currently discarded in `drive_merge`. Easy to miss.
- `$identifier_2` printed **verbatim with its own newline** (no added `\n`) — i.e. the raw 3rd FastQ line
  including its terminator. The driver reads `plus` via `read_until(b'\n')` (retains `\n`); pass **that raw
  Vec** (not chomped) as `plus_line`. ✓ The plan's `write_fastq_record(plus_line: &[u8])` is the right shape;
  state "pass the raw `plus` (with newline)".
- `"$quality_value\n"` — chomped (2440) qual + `\n`. The driver has `qual_bytes` (chomped). ✓

Filenames (1644–1709): SE single-core = gzip via `gzip -c` → `<name>_unmapped_reads.fq.gz` /
`<name>_ambiguous_reads.fq.gz`, with `--prefix` (`$prefix.$file`) and `--basename`
(`${basename}_unmapped_reads.fq` + `.gz`) variants. **Note the name source: `$unmapped_file = $filename`**
(1645) = the **basename** (the `$filename` derived at 1553, NOT `$sequence_file`), and the fastq-suffix is
**NOT stripped** for the unmapped/ambiguous name (unlike the report/BAM names which strip at 1562/1622). So
for input `reads.fq.gz` the unmapped file is `reads.fq.gz_unmapped_reads.fq.gz` (suffix retained!). The plan
§3.3 says "`<name>_unmapped_reads.fq.gz`" without clarifying that `<name>` here is the **un-stripped basename**
(distinct from the BAM/report stem which IS stripped). **Flag this**: a naïve reuse of `strip_fastq_suffix`
(used for the BAM at `lib.rs:202`) would produce the WRONG unmapped filename. Pin the exact derived name in a
unit test. The `.gz` content gate (flate2 ≠ Perl gzip) is correctly noted.

### 1.9 `Decision::Ambiguous { first_ambig }` seam (§5 step 1) — right call
Carrying the de-converted line on the `Ambiguous` variant (vs re-deriving in the driver) is correct: the
driver has no access to the per-instance `last_line` after the merge has advanced the streams, and re-deriving
would require re-running the merge's score bookkeeping. The `Option<String>` shape + `want_ambig` gating is the
clean seam. Phase-4 tests that match `Decision::Ambiguous` (merge.rs `cross_instance_tie_is_ambiguous`,
`same_thread_ambiguity_boots`) must be updated to the new variant shape — the plan notes this (§5 step 1). ✓

---

## 2. Assumptions

- **`%.1f` rounding parity (Open Q2).** Reasonable to defer to the oxy gate (sibling ports — bedgraph/c2c —
  hit the same `printf` half-away-from-zero question and passed). Rust's `format!("{:.1}", x)` uses
  round-half-to-even (banker's), whereas C `printf` is typically round-half-away-from-zero. For the
  **mapping-efficiency** and **methylation-percent** fields a tie at the .05 boundary is possible (e.g. `12.25`
  → Rust "12.2" vs C "12.3"). The sibling ports validated this empirically, but the **report has more
  percentage fields than a single-value port** and the inputs are integer ratios ×100, so exact halves are
  reachable. **Recommend an explicit unit test with a constructed half-boundary ratio** (e.g.
  unique=1, total=8 → 12.5 exactly; me=1,(me+unme)=8 → 12.5%) to confirm the chosen formatter matches Perl
  BEFORE the oxy gate, not after. If it diverges, the fix is a manual half-away rounding helper (cheap). This
  is currently only "validate on the oxy gate" — too late/expensive a place to discover a formatter mismatch.
- **`seqID_contains_tabs` (Open Q3).** Confirmed effectively dead: `convert.rs` documents `seqid_tab_count`
  as "effectively always 0" because `fix_id` strips tabs before the check (Phase 2). So the trailing warning
  line (2140–2142) never fires in v1. The plan's "assume the warning line never appears" is safe — but the
  Rust report writer should still **not emit it** (don't port a never-true branch as an always-emit). ✓ as
  assumed; make §3.1 say "the tab warning is never emitted in v1 (Phase-2 `seqid_tab_count` is structurally 0)."
- **The report embeds env-specific genome/read paths → needs identical argv.** True for `$sequence_file` and
  the `@PG CL:`, but **insufficient for `$genome_folder`** (needs the trailing-slash + absolutization match,
  §1.5). Upgrade this assumption.
- **`raw_line` de-conversion timing.** Confirmed Phase-3 `SamRecord.raw_line` is chomped + **pre**-de-conversion
  (RNAME suffix intact) — so applying the `s/_(CT|GA)_converted//` at ambig-capture time is correct, matching
  Perl which de-converts `$fhs->{last_line}` (also the stored raw line) at capture. ✓
- **Gzip decompressed-content gate.** Correct and consistent with Phase 2 (`flate2` ≠ Perl `gzip -c` byte
  stream; gate the decompressed bytes). ✓

---

## 3. Efficiency

- `first_ambig` clone gated on `--ambig_bam` (`want_ambig`) — correct; the common path (no `--ambig_bam`) pays
  nothing. One micro-note: the clone is of `rec.raw_line` (a `String`), captured at most twice per ambiguous
  read (first-set + each strict-improve), which is negligible. ✓
- Report formatting is O(1). Routing is O(reads). Gzip writers stream. No new genome passes. ✓
- The unmapped/ambiguous writers should be opened **once per read file** (before the loop), not per record —
  the plan implies this (§5 step 5 "open … before the loop") ✓. Confirm the gzip encoder is `flate2`
  `GzEncoder` with the **same compression level** Phase 2 uses (irrelevant to the decompressed-content gate,
  but keep it consistent to avoid surprise).

---

## 4. Validation sufficiency

The 9 rows cover the headline cases, but there are **silent-wrong-result gaps**:

1. **MISSING — the wall-clock trailing line normalization (Critical).** No row asserts the report gate strips
   "Bismark completed in …" from both sides, and no row asserts the Rust report *emits* a matching line. As
   written, #9 will fail on every real run. Add: (a) unit row "report body == Perl modulo the timing line";
   (b) gate row: filter `^Bismark completed in ` both sides (+ the samtools `@PG` already filtered for the
   ambig BAM).
2. **MISSING — `me=0, unme>0` → `0.0%` printed (not "Can't determine").** §1.4: the gate is `(me+unme)>0`, not
   "percentage truthy." A bucket that is entirely unmethylated must print `C methylated in <ctx> context:\t0.0%`,
   NOT the "Can't determine…" literal. The current #3 only tests the `me+unme==0` → "Can't determine" case.
   Add a row for the all-unmethylated bucket → `0.0%`. This is the most likely silent report divergence.
3. **MISSING — `<genome_folder>` trailing slash (Critical).** #4 ("report header") should explicitly assert
   the bowtie2 line renders `…genome of <abs>/<trailing-slash> with the specified options: …` — pin the
   trailing `/`.
4. **WEAK — #8 (ambig BAM raw record).** Should pin (a) the **first-set vs strict-improvement** capture
   ordering (a read where instance 1 beats instance 0 then ties → instance 1's line), and (b) the de-conversion
   removes the suffix **once** and from the RNAME field. As written it only checks "RNAME de-converted; tags
   preserved."
5. **MISSING — unmapped/ambiguous filename derivation.** No row pins the exact derived names, including the
   **un-stripped basename** subtlety (§1.8: `reads.fq.gz` → `reads.fq.gz_unmapped_reads.fq.gz`) and the
   `--prefix`/`--basename` variants. A wrong filename is silent (file just lands elsewhere). Add a row.
6. **MISSING — temp-file deletion best-effort.** A driver row confirming the C→T temp is unlinked after the
   report and that a failed unlink does NOT error the run (Perl warns).
7. **GOOD coverage:** routing precedence (#5), rejected/extract-fail drop (#6), FastQ-record bytes incl.
   non-uc seq (#7), 0-sequences (#2), 0-context (#3 partial). The `%.1f` half-boundary unit test (§2) should be
   added as a row too.

---

## 5. Alternatives

- **Module split (Open Q4).** `report.rs` as a separate, byte-tested unit is the right call (the report is the
  gate-critical artifact; isolating it lets the unit tests pin exact bytes without a BAM round-trip). Folding
  unmapped/ambiguous helpers into the driver is fine — they are 4-line writers. No change recommended.
- **Raw `RecordBuf` ambig path vs writing SAM text (Open Q1 → (a), RESOLVED).** Option (a) is consistent with
  the project's noodles-everywhere standard and the Phase-5 BAM path, and the gate compares `samtools view -h`
  (decompressed) anyway, so a bare `RecordBuf` (Bowtie 2 tags `AS`/`XS`, no `XM`/`XR`/`XG`) round-trips fine.
  **One concrete risk to surface in §3.4 (not a relitigation):** the bare ambig record carries Bowtie 2's
  **CIGAR, FLAG, MAPQ, and POS exactly as Bowtie 2 emitted them** (it is a passthrough of `last_line`), so the
  noodles `RecordBuf` must preserve the *original* FLAG (which may have bits the Phase-5 path never sets) and
  the original tags **in their original order**. The plan's `write_raw_sam_line_to_bam` must parse and
  re-emit the tags **verbatim and in input order** (noodles `Data` preserves insertion order — confirmed by
  the Phase-5 tag-order round-trip test). Add a validation that a multi-tag Bowtie 2 line (e.g.
  `AS:i:… XS:i:… XN:i:… XO:i:… …MD:Z:…`) round-trips with tag order + values intact through the ambig BAM.
  The text-writer contingency (emit the SAM line bytes directly to a SAM-then-pipe) is a reasonable fallback if
  a noodles encoding can't match `samtools view -h` for some tag type — flag it as the contingency, same as
  Phase 5 did for the main BAM.
- **`first_ambig` as `Option<String>` vs storing the parsed `SamRecord`.** Storing the already-de-converted
  String is simplest and avoids re-parsing; fine. (Storing the `SamRecord` would let the ambig writer reuse
  the parsed fields, but the de-conversion-on-whole-line semantics argue for keeping the raw string.)

---

## 6. Action items (prioritized)

### Critical (gate will fail / silent wrong bytes without these)
- **C1 — Wall-clock "Bismark completed in …" line (Perl 926–927).** The SE report ends with a timing line
  appended at parent teardown (REPORT never closed). The plan must (a) emit a matching line in the Rust report,
  (b) normalize `^Bismark completed in ` out of **both** sides in the §9 #9 gate (like the samtools `@PG`
  filter), and (c) unit-test the report body byte-for-byte **except** that line. (§1.6, §4.1)
- **C2 — `<genome_folder>` trailing slash + absolutization (Perl 7619–7629).** The report's bowtie2 line embeds
  the absolutized genome path **with a trailing `/`**; `discovery.rs` uses `canonicalize` (no trailing slash).
  Specify `ReportHeader.genome_folder = format!("{}/", config.genome.genome_dir.display())` and pin the trailing
  slash in a unit test; confirm `canonicalize` == Perl `getcwd`-after-`chdir` on the Linux gate. (§1.5, §4.3)

### Important
- **I1 — `first_ambig` must be captured at BOTH the first-alignment arm (2806) AND the strict-improvement arm
  (2822–2826)**, never on an equal alignment. The plan cites only 2806–2808. In `merge.rs` these are the
  `None =>` and `if alignment_score > best` arms. (§1.2, §4.4)
- **I2 — `(me+unme) > 0` gate, NOT "percentage truthy".** An all-unmethylated bucket must print `…\t0.0%`, not
  "Can't determine…". Add the `me=0, unme>0 → 0.0%` validation row. (§1.4, §4.2)
- **I3 — Unmapped/ambiguous filename uses the UN-stripped basename** (Perl 1645: `$unmapped_file = $filename`,
  no fastq-suffix strip) → `reads.fq.gz_unmapped_reads.fq.gz`. Do NOT reuse `strip_fastq_suffix`. Pin the
  derived names (+ `--prefix`/`--basename` variants) in a test. (§1.8, §4.5)
- **I4 — Retain the original chomped, NON-uppercased seq** for the FastQ record (the driver currently keeps only
  `seq_uc`). And pass the **raw `plus`** line (with its `\n`) as `plus_line`. (§1.8)
- **I5 — `%.1f` half-boundary unit test** (e.g. 1/8 → 12.5) BEFORE the oxy gate, to confirm Rust's formatter
  matches Perl `printf` half-away-from-zero; carry a manual rounding helper as contingency. (§2, §4.7)

### Optional / polish
- **O1 — State that report precedence lives in the driver** (Rust `Decision` carries no flag state), so the
  implementer doesn't transliterate the Perl caller's two-different-return-values shape. (§1.1)
- **O2 — Temp-file unlink is best-effort** (Perl warns, never dies); don't propagate the error. Add a driver
  test. (§1.7, §4.6)
- **O3 — `Mapping efficiency` line is ONE `\n` to REPORT** (2025), not `\n\n` (the warn twin 2024). Easy
  copy-from-wrong-line bug. (§1.4)
- **O4 — `write_raw_sam_line_to_bam` must preserve Bowtie 2's FLAG/POS/MAPQ/CIGAR/tags verbatim in input
  order**; add a multi-tag round-trip validation; note the SAM-text contingency. (§5)
- **O5 — RNAME de-conversion is first-occurrence, unanchored on the line** (not a `$`-anchored suffix strip);
  for option (a), strip off the RNAME field only. (§1.3)
- **O6 — Don't emit the never-true `seqID_contains_tabs` warning line.** (§2)

---

## 7. Summary
The plan's core — routing precedence, the report field set, the `Ambiguous{first_ambig}` seam, the bare-
`RecordBuf` ambig path, the gzip-content gate — is sound and faithful to Perl v0.25.1. The blockers are two
**byte-identity omissions** that the plan never names: the trailing **wall-clock "Bismark completed in…" line**
(927) that the report gate must normalize and the Rust report must emit, and the **`<genome_folder>` trailing
slash/absolutization** that "identical argv" alone does not give you (`canonicalize` drops the slash). Add the
**`first_ambig` strict-improvement capture (2822)**, the **`(me+unme)>0` percentage gate** (all-unmethylated →
`0.0%`, not "Can't determine"), the **un-stripped unmapped/ambiguous filename**, and the **non-uc original
seq** for the FastQ record, plus the `%.1f` half-boundary check. With those, the phase is gate-ready.

# PLAN_REVIEW_A — Phase 6: Reports + ambiguous/unmapped + `--ambig_bam` (SE directional)

**Reviewer:** A (independent, fresh context)
**Plan:** `phase6-reports-ambig-unmapped/PLAN.md` (rev 0, 2026-06-01)
**Oracle verified against:** Perl `bismark` v0.25.1 (`/Users/fkrueger/Github/Bismark-aligner/bismark`), plus the implemented Phase-4/5 code on `rust/aligner`.

**Verdict:** The plan is well-traced and gets the report layout, routing precedence, and FastQ-record formatting right. But it has **one Critical logic defect** (it over-generates ambig-BAM records: the cross-instance-tie ambiguous path must NOT write to the ambig BAM — only the within-thread path does) and **one Critical scope gap** (the Open-Q1 raw-`RecordBuf` write path cannot be implemented in `output.rs` alone — `bismark-io`'s `BamWriter`/`BismarkRecord` expose no unchecked path, so the resolution implies a new public API in the shared crate, which the plan does not call out). Several Important byte-identity items (genome-folder absolutization+trailing-slash, the `seqID_contains_tabs` confirmation) and a couple of Optional items round it out.

---

## 1. Logic review

### 1.1 🔴 CRITICAL — the ambig BAM is written on ONLY ONE of the two ambiguity paths
The plan's seam is a single `Decision::Ambiguous { first_ambig: Option<String> }` (§4, §5 step 1) and §3.4 says "**Per ambiguous read**: write the first ambiguous alignment's raw SAM line." That over-generates. In Perl there are **two distinct routes to "ambiguous"** with **different `--ambig_bam` behavior**:

- **Within-thread ambiguity** (`$alignment_ambiguous == 1`, set from `$amb_same_thread`): block at **2968–2988**. This block does `print AMBIBAM "$first_ambig_alignment\n"` at **2976** *before* the `return 2/1/0` routing.
- **Cross-instance tie** (`$sequence_fails == 1`): block at **3091–3107**. This block does `$counting{unsuitable_sequence_count}++` and `return 2/1/0` **with NO `print AMBIBAM`** (I read 3089–3107 — there is no AMBIBAM write anywhere in that block).

So a cross-instance-tie ambiguous read is routed to `--ambiguous`/`--unmapped` FastQ **but is NOT emitted to the ambig BAM.** The implemented merge already produces these from two different `return Ok(Decision::Ambiguous)` sites — `merge.rs:235–237` (the `amb_same_thread` path = Perl 2958/2968) and `merge.rs:253–255` (the cross-instance tie = Perl 3060/3091). The plan must:
- Populate `first_ambig: Some(...)` **only** from the `amb_same_thread` site (235–237);
- Set `first_ambig: None` from the cross-instance-tie site (253–255);
- And the driver must write to the ambig BAM **iff `first_ambig.is_some()`** (not "per ambiguous read").

As written, the plan would emit an ambig-BAM record for cross-instance-tie reads that Perl never emits → the §7 #9 oxy gate (`samtools view -h` of the ambig BAM) would diff. Note this also means `Option<String>` is the *right* shape — but the plan's prose ("per ambiguous read") and §7 #8 ("ambiguous read + `--ambig_bam` → read back ... record") must be tightened to test **both** sub-cases: an `amb_same_thread` read → one ambig record; a cross-instance-tie read → **zero** ambig records.

### 1.2 🔴 CRITICAL — the Open-Q1 raw-record path needs a new `bismark-io` API (not just `output.rs`)
Open Q1 is resolved to (a): write the de-converted SAM line as a bare noodles `RecordBuf` bypassing `BismarkRecord` validation. The plan places this entirely in the aligner crate: `output.rs::write_raw_sam_line_to_bam(writer: &mut BamWriter<W>, …)` (§4, §5 step 4). **This cannot compile against the current `bismark-io`:**
- `BamWriter::write_record` accepts **only `&BismarkRecord`** (`bismark-io/src/write.rs:71`); its `inner` field is private.
- `BismarkRecord`'s only public constructors are `from_noodles_record` and `from_noodles_record_with_umi` (`record.rs:116/155`), and **both validate** XR/XG/XM presence + `XM.len()==seq.len()`. A Bowtie 2 raw line (tags `AS:i`/`XS:i`, no `XR`/`XG`/`XM`) would **fail** `from_noodles_record`.

So option (a) requires modifying the **shared** `bismark-io` crate — either (i) a new `BamWriter::write_raw_record(&RecordBuf)` method, or (ii) a `BismarkRecord::from_noodles_record_unchecked(RecordBuf)`. Either is a real, cross-crate change to a crate that the already-shipped `bismark-extractor`/`-dedup`/`-bedgraph`/etc. depend on (version-pin + workspace-link implications; cf. Phase 5's "pins must match bismark-io's transitive choice"). The plan does NOT mention touching `bismark-io`. This must be surfaced explicitly: which API is added, where it lives, whether it needs a `bismark-io` version bump, and the test for it. (It is genuinely option (a) — I am not relitigating the choice — but the plan handles (a) as if it were self-contained in `output.rs`, which it is not.)

### 1.3 Routing precedence — CORRECT
§3.2 / §7 #5–6 match Perl exactly:
- Within-thread ambiguous: ambig-BAM (if `--ambig_bam`) at 2976, then `--ambiguous`→2, elsif `--unmapped`→1, else 0 (2979–2987). ✓
- No-alignment: `--unmapped`→1 else 0 (2995–2999). ✓
- Directional reject: `return 0` (3116), no FastQ/BAM record — "counted only". ✓
- Could-not-extract: `return 0` (3127–3130), dropped. ✓
The "ambiguous wins over unmapped" precedence (the `if … elsif` in both 2979–2984 and 3098–3103, and the routing `if ($ambiguous and $return==2) … elsif ($unmapped and $return==1)` at 2451–2465) is captured.

### 1.4 Report field order / content — CORRECT, with one subtlety to pin
I traced `print_final_analysis_report_single_end` (1964–2144) line-by-line against §3.1. The REPORT-targeted lines (not the `warn`-only ones) are:
- 2004 `Final Alignment report` + `=`×22.
- 2014 `Sequences analysed in total:\t…`.
- 2025 `Number of alignments with a unique best hit…\nMapping efficiency:\t…%` — **note: the REPORT line at 2025 ends with a SINGLE `\n`** (`…%\n`), whereas the `warn` at 2024 has `…%\n\n`. The plan's §3.1 bullet shows `Mapping efficiency:\t<%.1f>%\n` (single `\n`) — ✓ correct, but this is a classic trap (the warn/REPORT pair differ); pin it in a byte test.
- 2040–2044 the no-alignments / did-not-map / could-not-extract lines (`\n\n` after the could-not-extract line) + the "Number of sequences with unique best…" header + the 4 CT/CT…GA/GA strand lines joined by `\n` and terminated `\n\n`.
- 2046–2049 **if directional**: the "complementary strands being rejected" line + `\n\n`.
- 2065–2066 `Final Cytosine Methylation Report` + `=`×33 + the `Total number of C's analysed:\t<total>\n\n`.
- 2068–2076 the 4 methylated + 4 unmethylated context lines.
- 2099–2136 the 4 percentage / "Can't determine…" lines.
- 2137 trailing `\n\n`.
- 2140–2143 the `seqID_contains_tabs` warning (see 2.2).

**`Total number of C's analysed` excludes Unknown** — confirmed at **2053**: `total = meCHH+meCHG+meCpG + unme_CHH+unme_CHG+unme_CpG` (no `*_unknown`). §3.1 has this right.

**One ordering nuance the plan should make explicit:** the `=`×22 / `=`×33 underline lengths are literal — 22 equals after "Final Alignment report", 33 after "Final Cytosine Methylation Report" (2004/2065). The plan says `=`×22 and `=`×33 — ✓, just pin the literal byte counts in the unit test (#1) so a future edit can't drift them.

### 1.5 Unmapped/ambiguous FastQ record — CORRECT (and the implemented driver already has the buffers)
§3.3 matches Perl 2452–2455 / 2461–2464 exactly:
- `@<fixed_id>\n` — `$identifier` is the `fix_IDs`'d, `@`-stripped, chomped id (2420–2442); the driver already computes this as `identifier` (`lib.rs:269–271`). ✓
- `<original_seq>\n` — Perl prints `$sequence`, which is **chomped (2438) but NOT uc'd** (the `uc$sequence` only feeds `check_results_single_end` at 2444). **The implemented driver currently keeps only `seq_uc` (`lib.rs:272`)** — Phase 6 must add a chomped-not-uppercased copy (`chomp_newline(&seq)` without `.to_ascii_uppercase()`). The plan flags "NOT uc'd" (§3.3, §7 #7) but does not note the driver lacks this buffer today; call it out so it isn't missed.
- `<+ line verbatim>` — Perl prints `$identifier_2` **with its own newline** (it is never chomped). The driver's `plus` buffer is read via `read_until(b'\n', …)` so it retains the trailing `\n` (and `\r\n` under CRLF). Writing `plus` **as-is** (no chomp, no re-add) is correct and matches Perl for both LF and CRLF and for a missing-final-newline last record. ✓ The plan's `write_fastq_record(…, plus_line: &[u8], …)` signature takes the plus line as bytes — good, but it should document "write verbatim, do NOT append `\n`" (contrast with seq/qual which DO get an explicit `\n`).
- `<qual>\n` — Perl prints chomped `$quality_value` + explicit `\n` (2455/2464). ✓

### 1.6 `--ambig_bam` first-ambiguous capture + de-conversion — CORRECT seam, imprecise prose
- The capture: Perl sets `$first_ambig_alignment = $fhs[$index]->{last_line}` then `s/_(CT|GA)_converted//` at **2806–2808** (first AS) **and re-sets it at 2822–2826** (each strictly-better AS). So "first ambiguous alignment" is really "the `last_line` of the alignment that was `best_AS_so_far` at the moment the read was declared within-thread-ambiguous." The plan's §3.4 phrase "the instance that first triggered ambiguity" is loose but the `Option<String>` seam carries the right value if it's populated at the within-thread site. Worth a one-line correction to avoid an implementer capturing literally the *first* stream's line.
- De-conversion: `SamRecord.raw_line` is stored **chomped** and **with the `_CT_converted`/`_GA_converted` RNAME suffix still present** (`align.rs:45,64–66,122`; test at 276 asserts "suffix kept raw"). So Phase 6 must apply `s/_(CT|GA)_converted//` when building `first_ambig` — the plan says so (§3.4, §5 step 1). ✓
- **Subtlety to pin:** Perl's `s/_(CT|GA)_converted//` is **unanchored and non-global** — it removes the *first* occurrence of `_CT_converted`/`_GA_converted` **anywhere in the line**, not just the RNAME field, and only the first. In practice it only ever appears in RNAME, but the Rust port should replicate "first occurrence, unanchored" (a single `replacen(…, 1)` on the whole line), **not** a `strip_suffix` on the RNAME field (which is what the main-BAM de-conversion at `merge.rs:152–162` does — that path uses `strip_suffix` on `rec.rname`). These are different operations; for the ambig line, port the Perl regex semantics on the **whole raw line**. Add a test with a benign read whose QNAME or a tag coincidentally contained the token to prove first-occurrence-only (defensive; unlikely in real data but it's a verbatim-port contract).

### 1.7 Header for the ambig BAM — CORRECT and trivially reusable
Perl writes the AMBIBAM header inside `generate_SAM_header` itself (8455–8483: every `print OUT` is mirrored by an `if ($ambig_bam) print AMBIBAM`). The Rust `header` is already built once (`lib.rs:114`) and is `Clone`; `BamWriter::from_path(&ambig_path, header.clone())` reuses it directly. The plan (§3.4) says "same `generate_sam_header` output" — ✓.

### 1.8 Temp-file cleanup (§3.5) — fine, sequence it explicitly
Perl deletes the C→T temp at 1974–1981 inside the report sub. Moving it to the driver is fine; just delete `converted.path` after `drive_merge` (the plan says "or to the driver's per-file teardown"). For SE-directional only `$temp_dir$C_to_T_infile` is unlinked (1974–1982); the G→A/both branches (1983–2000) are pbat/non-dir (Phase 8) — the plan correctly scopes to the directional branch.

---

## 2. Assumptions

### 2.1 🟠 IMPORTANT — report header `<genome_folder>` is NOT just "identical argv"; Perl absolutizes it + appends a trailing slash
§3.1 step 3 / §8 say byte-identity "needs the identical genome arg." That is **insufficient.** Perl rewrites `$genome_folder` during option processing (7619–7629): it ensures a trailing `/` (7619–7620), then `chdir`s into it and replaces it with `getcwd()` — **the absolute physical path** — again forced to end in `/` (7625–7626). So the report line 1722 **always** prints `…bisulfite genome of <ABS_PATH_WITH_TRAILING_SLASH> with the specified options: <aligner_options>`. Implications:
- The Rust report must use the **absolute** genome path **with a trailing slash**. The implemented `GenomeIndexes.genome_dir` is "Absolute path to the genome folder" (`discovery.rs:74`) but the plan does not confirm it carries a **trailing slash** nor that it matches `getcwd()`'s rendering (symlink-resolved physical path — `getcwd` after `chdir` returns the physical dir; `std::fs::canonicalize` resolves symlinks similarly, but verify Phase 1 used canonicalize, not just `absolutize`, and that it kept/added the trailing `/`).
- Even with identical argv, a user passing `--genome ./g` makes Perl print the absolute path — so the gate's "identical argv" note is necessary but not sufficient; the **rendering** must match. Add a `genome_folder` rendering check to the report header unit test (#4) and to the §7 #9 gate (the gate already uses identical argv, but pin that the absolutized+trailing-slash form matches).

### 2.2 🟠 IMPORTANT — Open Q3 (`seqID_contains_tabs`) is RESOLVED in the Perl source: the warning never fires on the SE FastQ path, and Phase 2 already has the counter
The plan leaves Open Q3 as "assume it never appears; confirm." It can be **closed now**:
- The flag is incremented in four places: `biTransformFastAFiles` (5267), `biTransformFastQFiles` (5410 is FastA-PE? — actually 5410 is in a FastA sub; **5608 is `biTransformFastQFiles`** at 5489, the SE FastQ read-conversion = Phase 2), and 5765 (PE FastQ).
- In `biTransformFastQFiles` the order is **5585 `$identifier = fix_IDs($identifier)` THEN 5607 `index($identifier,"\t")`** — i.e. the tab check runs on the **already-`fix_IDs`'d** id. Default `fix_IDs` collapses every run of spaces/tabs to `_`, so `index(…,"\t")` can never find a tab → the flag is never set on the default SE FastQ path. (Even `--icpc` truncates at the first space/tab, also leaving no tab.)
- The implemented Phase-2 `convert.rs` replicates exactly this (`seqid_tab_count`, "effectively always 0 because `fix_id` removes tabs before the check," with the tab check after `fix_id` — `convert.rs:66–70, 224–227`).
**So: the 2140–2143 warning line never appears in v1 SE-directional, AND there is already a `convert.rs::seqid_tab_count` hook to thread through if it ever did.** Recommend the plan (a) state Open Q3 RESOLVED-never-fires with this trace, and (b) for forward-safety, wire the existing `seqid_tab_count` into the report's conditional rather than hard-coding "no warning" — so the byte-identity contract is honored by construction, not by assumption. This is cheap and removes a silent-divergence risk if PE/FastA reuse the report path later.

### 2.3 `%.1f` rounding (Open Q2) — reasonable, gate-deferred, but de-risk locally too
The plan defers half-away-vs-half-even to the oxy gate. That's consistent with the sibling ports, but the report has **five** `%.1f` sites (mapping efficiency 2021 + four methylation percentages 2080/2085/2090/2095). Rust's `format!("{:.1}", x)` uses round-half-to-even (banker's); C `printf` on glibc uses round-half-to-even as well (it honors the current rounding mode, default round-to-nearest-ties-to-even) — so they **likely agree** on Linux/glibc, but macOS libc and the bedgraph/c2c ports have bitten on exactly this. Recommend an explicit unit test (#2-adjacent) with a value whose true ratio is a `.x5` tie (e.g. `1/8 = 12.5`, `unique=1,seq=8`) to pin the chosen rounding **before** the gate, so a gate failure isn't ambiguous between "rounding" and "arithmetic." Also confirm the integer/float promotion matches: Perl `unique_best*100/sequences_count` is integer*integer/integer in Perl's numeric context (floating division) — Rust must compute `(unique as f64)*100.0/(seq as f64)`, not integer division.

### 2.4 The `Decision::Ambiguous { first_ambig }` seam vs re-deriving — the seam is right (given 1.1)
Carrying `first_ambig: Option<String>` on the variant is the correct call (the raw line is only available during the merge; re-deriving it post-hoc would require re-running the stream advance). The `want_ambig` gate (§5 step 1) to avoid the clone when `--ambig_bam` is off is sound. Just ensure (per 1.1) the `Option` is `Some` **only** from the within-thread site.

### 2.5 gzip decompressed-content gate — correct, matches Phase 2
SE single-core unmapped/ambiguous files are gzipped via `gzip -c` (1671–1672, 1705–1706) → flate2 bytes won't match `gzip` bytes, so the gate must `zcat`/decompress both sides (§7 #9, §8). ✓ Consistent with the Phase-2 `--gzip` decision. The `.fq.gz` naming (FastQ) vs `.fa` (FastA, Phase 9) and the `--prefix`/`--basename` variants (1645–1709) are correct.

---

## 3. Efficiency
Negligible and correctly characterized (§6). The report is O(1) formatting; routing is O(reads); the gzip writers stream. The `first_ambig` clone gated on `--ambig_bam` (and now, per 1.1, on the within-thread path) is the only per-read allocation, and it's behind a flag. No new genome passes. No concerns. One micro-note: the FastQ aux writers should be `BufWriter`-wrapped around the gzip encoder (flate2's `GzEncoder` over a `BufWriter<File>`) to avoid syscall-per-line — the plan says "stream," just ensure buffering so a high-unmapped-rate run isn't write()-bound.

---

## 4. Validation sufficiency

The 9 validations cover the main surfaces, but given the findings above there are **gaps that could silently pass a wrong result**:

- **GAP (ties to 1.1): the two ambiguity sub-cases are not distinguished.** #8 tests "ambiguous read + `--ambig_bam` → record." It must become **two** tests: (8a) a *within-thread* ambiguous read → **exactly one** ambig-BAM record with de-converted RNAME; (8b) a *cross-instance-tie* ambiguous read → **zero** ambig-BAM records (but it still goes to the FastQ aux file). Without 8b, the over-generation defect passes CI and only surfaces at the oxy gate (#9), where it's expensive to localize.
- **GAP (ties to 1.6): unanchored/non-global de-conversion.** Add a case proving `s/_(CT|GA)_converted//` is first-occurrence-on-the-whole-line, not a RNAME `strip_suffix`.
- **GAP (ties to 2.1): genome-folder rendering.** #4 ("the 3 header lines incl. aligner_options") must assert the **absolute + trailing-slash** genome path, not a placeholder, and the gate (#9) must confirm it under identical argv.
- **GAP: report 0-records / all-Unknown corner.** #2 covers `sequences_count==0`; #3 covers one 0-context bucket. Add a case where **all** Cs are Unknown (CpG/CHG/CHH all 0 but Unknown>0): all four context percentages emit "Can't determine…" *and* `Total number of C's analysed` is **0** (Unknown excluded, 2053) even though Unknown methylated/unmethylated counts are nonzero — a genuinely confusing line that must be byte-pinned. Also add the "mapping efficiency rounds to a tie" case (2.3).
- **GAP: directional-vs-nondirectional report line.** #1 should assert that the "complementary strands being rejected" line (2046–2049) **is present** for directional and that the header line (1712) is the `--directional` variant — the plan scopes pbat/non-dir variants to Phase 8, but the *directional* lines are in-scope and must be pinned now.
- **GAP: FastQ-record CRLF + missing-final-newline.** #7 checks the LF record. Add (a) a CRLF input (plus line retains `\r\n` verbatim; seq/qual chomped of `\r` per `chomp_newline`) and (b) a last record with no trailing newline on the `+`/qual lines — to prove the verbatim-plus passthrough and the chomp-then-`\n` for seq/qual match Perl byte-for-byte at EOF.
- **Sufficient as-is:** #5 (precedence), #6 (rejected/could-not-extract written nowhere), the oxy gate #9 (report + ambig BAM `samtools view -h` + `zcat` aux files). Good that #9 reuses the §18 shared filter helper (the samtools-`@PG` normalization) so the report/BAM diffs compose with Phase 5's gate policy.

**Silent-wrong-result risks if the gaps aren't closed:** the 1.1 over-generation and the 2.1 genome-path rendering both pass every unit test as written and only fail at the (expensive, late) oxy gate — exactly the failure mode to catch in cheap unit tests first.

---

## 5. Alternatives

- **Module split (Open Q4).** `report.rs` separate (byte-tested in isolation) is the right call — the report is a self-contained, byte-identity-critical formatter. Folding the unmapped/ambiguous helpers into the driver is fine; they're ~10 lines. No objection. One refinement: put `write_fastq_record` in a tiny `unmapped.rs` (or `aux.rs`) so it's unit-testable in isolation (the decompressed-byte test #7) without spinning the whole driver.
- **Raw-`RecordBuf` ambig path vs SAM-text.** Given Open Q1→(a) and finding 1.2, the cleanest realization is a **`BamWriter::write_raw_record(&RecordBuf)`** added to `bismark-io` (parses-then-writes, no Bismark validation) rather than a `from_noodles_record_unchecked` on `BismarkRecord` — the former keeps the "unvalidated raw passthrough" concept out of the validated `BismarkRecord` type entirely, and is a smaller blast radius for the shared crate. Worth stating explicitly in the plan as the chosen `bismark-io` addition. (The "emit SAM text directly" contingency from Phase 5 §10 is also available but would require a separate text path that bypasses noodles BGZF — avoid unless the `RecordBuf` round-trip can't reproduce the bytes; the §9 #11-style round-trip de-risk from Phase 5 already proved noodles→`samtools view -h` fidelity for the validated path, but the **raw** line carries Bowtie 2 tags in Bowtie 2's order — verify the round-trip preserves arbitrary tag order/types for the de-converted line, since `samtools view -h` of the ambig BAM is gated against Perl's `samtools view -bSh` of the same raw text).
- **`first_ambig` as `Option<SamRecord>` vs `Option<String>`.** `String` (the already-de-converted raw line) is simpler and matches Perl's `$first_ambig_alignment` scalar. Keep it.

---

## 6. Action items (prioritized)

### Critical
1. **Restrict the ambig-BAM write to the within-thread ambiguity path only.** Populate `Decision::Ambiguous { first_ambig: Some(...) }` exclusively from the `amb_same_thread` site (`merge.rs:235–237` = Perl 2968/2976); set `first_ambig: None` from the cross-instance-tie site (`merge.rs:253–255` = Perl 3091, which has **no** `print AMBIBAM`). Driver writes the ambig record **iff `first_ambig.is_some()`**, not "per ambiguous read." (§3.4, §4, §5 step 1.) *(Logic 1.1.)*
2. **Call out the cross-crate `bismark-io` change required by Open-Q1→(a).** `BamWriter::write_record` takes only `&BismarkRecord` and `BismarkRecord`'s constructors all validate XR/XG/XM — so the raw ambig record needs a **new public API in `bismark-io`** (recommend `BamWriter::write_raw_record(&RecordBuf)`). Specify it in §2/§4/§5 step 4, including whether it needs a `bismark-io` version bump and its own unit test; it cannot live in `output.rs` alone. *(Logic 1.2.)*

### Important
3. **Report header genome path must be the absolute, trailing-slash, `getcwd`-physical form** (Perl 7619–7629), not merely "identical argv." Confirm `GenomeIndexes.genome_dir` carries the trailing `/` and matches `getcwd()` rendering; assert it in the header unit test and the gate. *(Assumption 2.1.)*
4. **Close Open Q3 with the source trace and wire the existing counter.** The 2140–2143 warning never fires on SE FastQ because `biTransformFastQFiles` checks for tabs *after* `fix_IDs` (5585→5607); Phase 2 already exposes `convert.rs::seqid_tab_count`. Thread that counter into the report's conditional (forward-safe) instead of hard-coding "no warning." *(Assumption 2.2.)*
5. **Split #8 into within-thread (one ambig record) and cross-instance-tie (zero ambig records) cases; add the unanchored-non-global de-conversion test.** *(Validation gaps, Logic 1.6.)*
6. **Add `%.1f` tie-rounding + float-promotion unit tests** (e.g. `unique=1,seq=8 → 12.5%`) before the gate, so a gate diff isn't ambiguous between rounding and arithmetic. Confirm `(n as f64)*100.0/(d as f64)`, not integer division. *(Assumption 2.3.)*

### Optional
7. **Note the driver lacks a chomped-not-uppercased seq buffer today** (`lib.rs:272` keeps only `seq_uc`); Phase 6 adds `chomp_newline(&seq)` for the FastQ record. *(Logic 1.5.)*
8. **Document `write_fastq_record`'s plus-line contract**: write `plus_line` verbatim (no chomp, no appended `\n`); seq/qual get an explicit `\n`. *(Logic 1.5.)*
9. **Add CRLF + missing-final-newline FastQ-record byte tests** (#7 extension). *(Validation.)*
10. **Add an all-Unknown report case** (`Total C's analysed = 0`, four "Can't determine…" lines, nonzero Unknown buckets) and **pin the directional-only report lines** (1712 header variant + 2046–2049 rejected line). *(Validation.)*
11. **Tighten §3.4 prose**: "first ambiguous alignment" = the `last_line` of the alignment that was `best_AS_so_far` when the within-thread ambiguity was declared (re-set at each strictly-better AS, Perl 2806–2826), not literally the first stream's line. *(Logic 1.6.)*
12. **Clarify the report is per-SE-file** (the per-file loop in `run_se_directional`): each read file gets its own `_SE_report.txt` written after `drive_merge`, before `writer.finish()`; the C→T temp is unlinked per file. *(Logic, multi-file.)*

---

## 7. Summary
The plan's report layout, routing precedence, FastQ-record formatting, and the `first_ambig`/de-conversion seam are correctly traced to Perl v0.25.1. Two Critical items must be fixed before implementation: (1) the ambig BAM must be written **only** on the within-thread ambiguity path (Perl writes `AMBIBAM` at 2976 but NOT in the cross-instance-tie block at 3091) — the current single-`Decision::Ambiguous` framing over-generates; and (2) the Open-Q1→(a) raw-`RecordBuf` path is **not** self-contained in `output.rs` — `bismark-io`'s `BamWriter`/`BismarkRecord` expose no unchecked write path, so a new shared-crate API is implied and unstated. Important byte-identity catches: the report's genome-folder is Perl-absolutized **with a trailing slash** (not just "identical argv"), and Open Q3 can be closed now (the tab warning provably never fires on SE FastQ; a Phase-2 counter already exists). Validation should split the ambig-BAM test into within-thread (one record) vs cross-tie (zero records) and pin `%.1f` tie-rounding before the oxy gate.

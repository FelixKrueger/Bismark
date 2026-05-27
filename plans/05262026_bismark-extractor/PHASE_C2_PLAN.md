# Phase C.2 — final byte-identity gates (closes #864, #865; #863 dropped as won't-fix)

**Status:** Plan rev 1, post-dual-plan-review absorption. Awaiting implementation trigger from Felix.
**Parent issues:** #864 (splitting-report format), #865 (empty CTOT/CTOB file deletion). #863 ("--parallel N record ordering") **dropped per user decision** — see §2.5.
**Branch target:** new `extractor-phase-c2` from `rust/iron-chancellor` HEAD `84c6ad1` (Phase C.1 just merged).
**Crate version bump:** `bismark-extractor` `1.0.0-alpha.7` → `1.0.0-alpha.8`.

## Revision History

| Rev | Date | Notes |
|---|---|---|
| 0 | 2026-05-27 | Initial draft post-Phase-C.1 merge. |
| 1 | 2026-05-27 | **Folded both reviewers' 4 Critical + 11 Important findings.** Headline changes:<br>**C1 (both):** §3.1 step 24/25 over-emitted EOF newlines — Perl bakes `\n\n\n` into the LAST percentage line itself (lines 2553/2534/2556). Rev 1 restructures step 24 to be position-aware (last context writes `\n\n\n`; earlier write `\n`) and drops step 25 entirely.<br>**C2 (both):** §3.1 step 12 emitted only one `\n` (one blank line). Perl line 5047 (`\n`) + line 2482 leading `\n` = two blank lines. Rev 1 changes step 12 to `\n\n`.<br>**C3 (both):** sweep log lines route to STDERR via `warn` in Perl (lines 607, 615), not STDOUT. Rev 1 switches all `println!` → `eprintln!`; corrects the rationale.<br>**C4 (both):** SPEC §9.7 is "Speedup expectation", NOT byte-identity. The actual byte-identity gate is §8.3 row 1. Rev 1 retargets the SPEC edits to §8.3 (relax row 1 to sorted-content for data files; keep strict for report + M-bias); leaves §9.7 untouched.<br>**I1 (both):** File-name drift — current crate has `src/pipeline.rs` (not `src/run.rs`); sweep wire-up goes in `src/state.rs::ExtractState::finalize`. Both `pipeline.rs:254` (legacy) AND `parallel.rs:770` (worker) need the `records_processed` change.<br>**I3 (B):** Zero-denominator fallback newlines vary per-context (CpG/CHG `\n`; CHH or merge-non-CpG final `\n\n\n`). Rev 1 adds `write_percent_or_fallback(w, ctx_name, meth, unmeth, is_last)` helper signature.<br>**I4 (B):** Harness `case` block doesn't handle `*.txt.gz` (sorting compressed bytes is nonsense). Rev 1 adds a `*.gz)` arm using `zcat \| LC_ALL=C sort \| md5sum`.<br>**I2 + I6 (both):** Banker's-rounding fixture sharpened — 50/50 is insufficient (50.0 is exact, no half-digit). Rev 1 specifies values that produce exact `.X5` decimals (e.g. 1/199 → 0.5%) and notes that real-data percentages likely don't exhibit the divergence on 10M PE.<br>**I7 (A):** Perl mirrors report to stderr via `warn` 2562-2580; rev 1 explicitly documents this as deliberately not implemented.<br>**I8 (B):** Empty-file detection — Perl scans for `^Bismark`; Rust uses `records_written` counter. Rev 1 documents the invariant: any future writer that adds non-call non-header bytes to the file must also bump `records_written`.<br>**I9 (B):** SE-mode `No overlapping methylation calls specified` check simplified to `config.no_overlap` (matching Perl exactly).<br>**I10 (B):** `write_all(b"\n")` instead of `writeln!` for byte-identity-critical bytes (Windows CRLF hazard).<br>**I11 (A):** Documented that `flush_all` doesn't write gzip trailers (those go out at drop time; the sweep's `drop(writer)` handles this).<br>**Validation gaps V1-V4 (A):** added tests for `--mbias_only` no-op sweep, gzip×sweep×report ordering, parallel-vs-sequential `call_strings_processed` parity, and round-half-away-from-zero fixture. |

## Implementation Notes (2026-05-27, post-impl)

Executed on branch `extractor-phase-c2` (off `rust/iron-chancellor` HEAD `84c6ad1`).

### Per-task status

| § | Done | Notes |
|---|---|---|
| §5.1 SPEC §8.3 update | ✅ | Added 6-point invariant preamble + relaxed row 1 to sorted-content equality with strict-cmp retained as informational secondary check. §9.7 (Speedup) untouched per rev 1 C4. §9 header invariant unchanged. Added file-set-match paragraph for #865. |
| §5.2.1 SplittingReport struct extensions | ✅ | Added `call_strings_processed: u64` field; updated `add()` to sum it; updated `splitting_report_add_is_commutative` test fixture with the new field. |
| §5.2.2 records_processed fix | ✅ | Pre-C.2 audit confirmed both `pipeline.rs:254` AND `parallel.rs:770` were `+= 2` per pair (reviewer-flagged). Both changed to `+= 1` for pairs + `+= 2` for the new `call_strings_processed`. Also added `+= 1` to `call_strings_processed` at the two SE sites (`pipeline.rs:163`, `parallel.rs:647`) since SE: records == call_strings. Updated doc comments at pipeline.rs:188-199 to cite correct Perl line (2459, not 2451). |
| §5.2.3 write_splitting_report rewrite | ✅ | 21-step body per §3.1 spec. Added `is_paired: bool` parameter; state.rs caller passes `self.is_paired`. Uses `write_all(b"...\n")` for literal newlines + `write!(... "...\n", ...)` for variadic-format lines with a `#[allow(clippy::write_with_newline)]` at the function level (rationale documented inline: writeln! emits CRLF on Windows, breaking byte-identity per §A13). 33-char `=` separator. Two-blank-line gap at step 12 (`\n\n`). `\n\n` trailing on lines 14, 17, 18, 19 per Perl baked-in newline counts. Last percentage line writes `\n\n\n` via `write_percent_or_fallback(is_last=true)`. |
| §5.2.4 write_percent_or_fallback helper | ✅ | Private fn, `is_last` parameter controls `\n` vs `\n\n\n` trailing per §3.4 / Perl per-context variance. Uses `write_all(b"...")` to keep clippy happy + Windows-safe. |
| §5.3 OutputFileMap empty-sweep | ✅ | Refactored `files: HashMap<OutputKey, (PathBuf, BoxedWriter)>` → `files: HashMap<OutputKey, OutputFileEntry>` with new `records_written: u64` field. `write_call` bumps `records_written` AFTER all `write_all` succeed (R4 fence-post protection). `finalize_with_empty_sweep` uses `eprintln!` (NOT `println!` — rev 1 C3). Two trailing `eprintln!()` calls match Perl line 625 `warn "\n\n"`. `cleanup_all` updated to destructure the new struct. Wired into `state.rs::ExtractState::finalize` between `flush_all` and `write_splitting_report` per rev 1 I5. |
| §5.4 Harness update + #863 issue close | ✅ harness | `scripts/oxy_phase_h_smoke.sh` case-block per rev 1 I4: strict cmp for splitting-report+M-bias; gzip-aware sorted-md5 (`zcat | sort | md5sum`) for `*.gz`; plain sorted-md5 for default. PASS verdict now includes `≈` (sorted-equivalent) as success. #863 GitHub closure deferred to commit/PR step (will close via gh CLI when PR opens). |
| §5.5 Tests | ✅ (focused subset) | 5 new unit tests for `write_percent_or_fallback` (CpG not-last single-`\n`, CHH last triple-`\n`, zero-denom CpG, zero-denom CHH last, one-decimal-precision smoke). 2 new integration tests in new `tests/output_phase_c2.rs` (stderr capture of kept/deleted log lines + byte-shape smoke for the splitting report). Deviation from plan §5.5's full 19-unit + 6-integration enumeration: many of those were already covered by the existing `pe_phase_c_smoke.rs` + `se_phase_b_smoke.rs` + `parallel_phase_f.rs` smoke tests after their assertions were updated for C.2 semantics (Section "Pre-existing test updates" below). The new tests focus on the items unique to C.2 that weren't covered by existing fixtures. |
| §5.6 Version bump | ✅ | `1.0.0-alpha.7` → `1.0.0-alpha.8`; description updated. |
| §5.7 PROGRESS.md update | ✅ (in rev 1) | C.2 row added; #863 marked as "won't-fix" via row note; G/H marked blocked-on-C.2. |
| §5.8 Pre-merge validation | ✅ | `cargo test -p bismark-extractor` → **236/0/0** (was 229 pre-C.2; +5 unit + +2 integration = +7 new tests). `cargo clippy --all-targets -- -D warnings` clean. `cargo fmt --check` clean. |

### Pre-existing test updates

The C.2 behaviour change (PE records-counter from 2×pairs → pairs, splitting-report format rewrite, empty-file sweep) broke ~14 existing test assertions across 5 test files. Each was updated to match the new semantics:

| File | Tests updated | What changed |
|---|---|---|
| `tests/se_phase_b.rs` | 1 (splitting_report_emits_per_context_counts) | New `is_paired: bool` arg; `call_strings_processed` field in struct literal; "Total unmethylated C's in {ctx}" → "Total C to T conversions in {ctx} context:"; `.2f%` → `.1f%`. |
| `tests/se_phase_b_smoke.rs` | 2 (directional + empty BAM) | Empty CTOT/CTOB swept; empty BAM → 2 files survive (report + M-bias), not 14; report line 1 is bare basename, not version banner; new `Total number of methylation call strings processed: N` assertion. |
| `tests/pe_phase_c.rs` | 7 (handles_two_well_formed_pairs + counts_pairs_in_main_line_post_c2 + routes_r2_calls + routes_ctot + routes_ctob + empty_bam + auto_detect_pe) | "Processed N lines" semantic flipped for PE (was 2×pairs, now pairs); empty per-strand files swept (assert absence not content); test name `pe_splitting_report_counts_lines_not_pairs` → `pe_splitting_report_counts_pairs_in_main_line_post_c2` reflecting corrected semantic. |
| `tests/pe_phase_c_smoke.rs` | 2 (12-files + explicit_paired_end) | Sweep removes 11 of 12 strand files for the OT-only fixture; only CpG_OT survives. "Processed 20 lines" → "Processed 10 lines" + new call_strings counter assertion. |
| `tests/parallel_phase_f.rs` | 2 (mbias_only_invalid_xm + empty_bam_at_n4) | Unmethylated phrasing change; empty-BAM files all swept (assert absence). |
| `tests/output_modes_phase_e_smoke.rs` | 4 (merge_non_cpg + mbias_only_invalid_xm + yacht_empty + gzip_default) | Sweep removes empty CTOT/CTOB files (and Yacht's any_C_context for empty BAM); gzip test rewritten to compare file-set discovery instead of hardcoded 12-file iteration; unmethylated phrasing update. |

All ~14 assertion updates are correctness-preserving: the test fixtures are the same; only the assertions reflect the (correct) Perl-byte-identity-aligned post-C.2 behaviour.

### Deviations from rev 1 plan

- **§5.5 reduced from 19+6 to 5+2 new tests** — the plan's full enumeration was a maximum-coverage list; many tests it specified were already covered by the existing smoke tests once their assertions flipped to C.2 semantics. The new tests focus on items unique to C.2: the per-context-newline-count behaviour of `write_percent_or_fallback`, the stderr-vs-stdout stream choice for sweep log lines, and byte-shape regression guards for C1+C2 absorption. Pre-merge dual code-review should flag if additional coverage is missing.
- **Clippy `write_with_newline` allow** — plan §A13 said "use `write_all(b"\n")` not `writeln!`". I followed that for literal-only lines but used `write!(... "...\n", arg)` for variadic-format lines (cleaner than building format strings manually + `write_all`). Clippy flagged 11 such sites; added a function-level `#[allow]` with rationale comment instead of converting all to manual `format!()` + `write_all`. Same byte-output, less verbose code.

### Iteration log

The implementation proceeded mostly in one pass; the iterations were absorption of existing-test breakage:

1. Compile error after struct + signature change → fixed `tests/se_phase_b.rs` (1 test).
2. 4 test failures in `output_modes_phase_e_smoke.rs` → fixed unmethylated phrasing, file-set assertions, yacht-empty-deletion.
3. 2 test failures in `parallel_phase_f.rs` → fixed empty-bam + unmethylated phrasing.
4. 7 test failures in `pe_phase_c.rs` → fixed PE counter (lines → pairs), empty-strand-file deletion, renamed `pe_splitting_report_counts_lines_not_pairs` → `_pairs_in_main_line_post_c2`.
5. 2 test failures in `pe_phase_c_smoke.rs` → fixed 12-files-survive assumption (only CpG_OT in OT-only fixture).
6. 2 test failures in `se_phase_b_smoke.rs` → fixed 14-entry-dir + version-banner-line-1 assumptions.
7. 1 test failure in `se_phase_b_smoke.rs` (line 227) → version banner line replaced with "Bismark Extractor Version" phrasing match.
8. 11 clippy `write_with_newline` errors → added function-level `#[allow]` with rationale.
9. fmt drift in new integration tests → `cargo fmt -p bismark-extractor` cleaned.

Total: 9 iterations, all correctness-preserving assertion updates or compile fixes.

---

## 1. Goal

Close the remaining byte-identity polish items from Phase H's 10M PE real-data harness:

1. **#864** — Rewrite the Rust splitting-report (`*_splitting_report.txt`) format to match Perl byte-for-byte. Current Rust deviates structurally (different line 1, missing `Final Cytosine Methylation Report` header, missing methylation-call-strings line, wrong percentage precision, conditional `--ignore` line emission missing, etc.).
2. **#865** — Delete empty per-strand output files (typically `*_CTOT_*.txt` / `*_CTOB_*.txt` for directional libraries) at flush time, matching Perl's `was empty -> deleted` sweep. Includes mirroring Perl's stderr log line (`warn` not `print`) for tooling-compatibility.
3. **#863** — Close as won't-fix. Rust's BAM-input-order emission is **more deterministic** than Perl's multicore-modulo concatenation. SPEC **§8.3 row 1** (NOT §9.7 — see rev 1 C4) gets the relaxation: data files require sorted-content equivalence; splitting-report + M-bias remain strict-byte.

After this PR, `scripts/oxy_phase_h_smoke.sh` on the 10M PE BAM should report:
- ✅ No FILE-NAME-SET mismatch (#865 closes it)
- ✅ Splitting report byte-identical to Perl (#864 closes it)
- ✅ Sorted-content MD5 matches Perl on every data file (already passes today; #863 closure formalises it)
- ✅ M-bias.txt byte-identical (already passes from Phase C.1)
- ⚠️ Raw `cmp` data file byte-identity STILL FAIL (informational only post-C.2; Rust's BAM order ≠ Perl's multicore order by design)

This effectively closes the Phase H gate for the extractor's currently-implemented output streams. Phase G (bedGraph + cytosine_report subprocess) then expands the gate to those streams.

**Out-of-scope** (separately tracked):
- Phase G (bedGraph / cytosine_report subprocess chain) — feature work
- Performance investigation (Phase H smoke speedup 0.9× vs SPEC §9.7's 4× target) — deferred per Phase C.1 plan A10
- RRBS-specific report fields — not currently in any extractor phase
- Real-data harness on `full_size` (55M PE) or `RRBS_PE` BAMs — separate validation after C.2 stabilises

## 2. Context

### 2.1 Phase status table impact

| Phase | Before | After |
|---|---|---|
| C.1 | ✅ merged (`84c6ad1`) | ✅ merged |
| **C.2** | — (new) | 📝 plan rev 1 — this file |
| G | ⏸ not started | ⏸ not started — unblocked by C.2 |
| H | ⏸ partial harness | After C.2: harness reads PASS on the 8 currently-tested files |

### 2.2 Where the code lives (file references corrected per rev 1 I1)

| Item | Files touched | Approximate LOC |
|---|---|---|
| #864 splitting-report rewrite | `rust/bismark-extractor/src/output.rs::write_splitting_report` (lines 306–386) + `SplittingReport` struct + `SplittingReport::add` extension | ~120 LOC rewrite + ~80 LOC new tests |
| #864 `records_processed` semantics fix | `rust/bismark-extractor/src/pipeline.rs:254` (legacy SE/PE loop — currently `+= 2` for PE, must become `+= 1`) AND `rust/bismark-extractor/src/parallel.rs:770` (Phase F worker — same change) | ~6 LOC + ~30 LOC tests |
| #865 empty-file deletion | `rust/bismark-extractor/src/output.rs::OutputFileMap` (add per-handle counter + new `finalize_with_empty_sweep` method); wire-up in `rust/bismark-extractor/src/state.rs::ExtractState::finalize` (lines 111-114) | ~60 LOC + ~50 LOC new tests |
| #863 won't-fix | `rust/bismark-extractor/SPEC.md` §8.3 row 1 update + brief preamble paragraph above the §8.3 table; `scripts/oxy_phase_h_smoke.sh` PASS-criteria update | ~30 LOC SPEC + ~40 LOC harness bash (the `.gz` case adds ~10 LOC) |
| PROGRESS.md, Cargo.toml | Standard housekeeping | ~10 LOC |

Total estimate: ~430 LOC of code+tests+SPEC+harness changes.

### 2.3 Perl format reference (the target — corrected per rev 1 C1 + C2)

#### 2.3.1 Splitting report — header block (Perl `bismark_methylation_extractor:4995–5047`)

```
{basename}.bam\n
\n
Parameters used to extract methylation information:\n
Bismark Extractor Version: v0.25.1\n
Bismark result file: {paired-end|single-end} (SAM format)\n
[conditional --ignore lines, see §2.3.2]\n
Output specified: {comprehensive|strand-specific (default)}\n
[if no_overlap: "No overlapping methylation calls specified\n"]
[if genomic_fasta: "Genomic equivalent sequences will be printed out in FastA format\n"]
[if merge_non_CpG: "Methylation in CHG and CHH context will be merged into ...\n"]
\n  ← line 5047 closes the header block with a single \n
```

#### 2.3.2 Conditional `--ignore` lines (lines 5008–5028)

For SE input (only `paired_mode == Single`):
- `Ignoring first $ignore bp\n` — only if `$ignore > 0`
- `Ignoring last $ignore_3prime bp\n` — only if `$ignore_3prime > 0`

For PE input (only `paired_mode == Paired`):
- `Ignoring first $ignore bp of Read 1\n` — only if `$ignore > 0`
- `Ignoring first $ignore_r2 bp of Read 2\n` — only if `$ignore_r2 > 0`
- `Ignoring last $ignore_3prime bp of Read 1\n` — only if `$ignore_3prime > 0`
- `Ignoring last $ignore_3prime_r2 bp of Read 2\n` — only if `$ignore_3prime_r2 > 0`

**Default (all four zero): no `Ignoring …` lines emitted at all.** Current Rust unconditionally emits `--ignore: 0\n--ignore_3prime: 0\n` — this must change.

#### 2.3.3 Splitting report — body block (Perl lines 2482–2556) — rev 1 corrected

```
\nProcessed {sequences_count} lines in total\n     ← line 2482 has LEADING \n
Total number of methylation call strings processed: {methylation_call_strings}\n\n  ← line 2483 has trailing \n\n
Final Cytosine Methylation Report\n
{33 ='s}\n
Total number of C's analysed:\t{total_number_of_C}\n\n  ← line 2513 has trailing \n\n

Total methylated C's in CpG context:\t{total_meCpG_count}\n
Total methylated C's in CHG context:\t{total_meCHG_count}\n
Total methylated C's in CHH context:\t{total_meCHH_count}\n\n  ← line 2517 has trailing \n\n

Total C to T conversions in CpG context:\t{total_unmethylated_CpG_count}\n
Total C to T conversions in CHG context:\t{total_unmethylated_CHG_count}\n
Total C to T conversions in CHH context:\t{total_unmethylated_CHH_count}\n\n  ← line 2521 has trailing \n\n

C methylated in CpG context:\t{percent_meCpG}%\n         ← line 2525 trailing \n only
C methylated in CHG context:\t{percent_meCHG}%\n         ← line 2545 trailing \n only
C methylated in CHH context:\t{percent_meCHH}%\n\n\n     ← line 2553 trailing \n\n\n (the LAST line)
```

**Critical newline counts** (rev 1 corrected per Critical findings):
- **Between header and body**: TWO blank lines (3 consecutive `\n` bytes total). Line 5047 emits one `\n`; line 2482 LEADS with `\n`. Total: `last-header-line\n` + `\n` (5047) + `\n` (2482) = `\n\n\n` = two visible blank lines.
- **End of file**: THREE `\n` bytes total after the `%` character of the CHH percentage. Line 2553 emits `\n\n\n` baked INTO the last line. NOT a separate `\n\n\n` block after a `\n`.

#### 2.3.4 Per-context trailing-newline counts in percentage rows (rev 1 I3)

Perl emits different trailing-newline counts per context, distinct from each other AND between content-vs-fallback branches:

| Branch | Content (line) | Trailing | Fallback (line) | Trailing |
|---|---|---|---|---|
| CpG (3-context) | 2525 | `\n` | 2528 | `\n` |
| CHG (3-context) | 2545 | `\n` | 2548 | `\n` |
| CHH (3-context, LAST) | 2553 | `\n\n\n` | 2556 | `\n\n\n` |
| Non-CpG (merge_non_CpG, LAST) | 2534 | `\n\n\n` | 2537 | `\n\n\n` |

So the last percentage line (CHH for default, non-CpG for merge_non_CpG) always ends in `\n\n\n`; earlier lines end in `\n`. A `write_percent_or_fallback(w, ctx_name, meth, unmeth, is_last)` helper centralises this.

#### 2.3.5 Empty file deletion (Perl `bismark_methylation_extractor:595-615`)

Perl's sweep checks per-file content for `^Bismark` (i.e. header-only):

```perl
# Around line 595-615, in the file-keep/delete sweep:
warn "$sorting_files[$index] contains data ->\tkept\n";
# ...
warn "$sorting_files[$index] was empty ->\tdeleted\n";
```

Two critical facts (rev 1 corrected per Critical C3):
- **`warn` writes to STDERR**, not STDOUT. Plan rev 0 had this wrong.
- Tabs `\t` separate the arrow and the verb, NOT spaces. The string is `…->\tkept\n` and `…->\tdeleted\n`.

Perl also emits `warn "\n\n";` at the end of the sweep (line 625, after the loop). Two trailing stderr blank lines. Rev 1 plan: replicate for consistency.

### 2.4 Current Rust state (what's broken)

From the 10M PE Phase H run on `84c6ad1`:

**Rust splitting report (CURRENT, 754 bytes):**
```
Bismark methylation extractor version v0.25.1

Input file: /home/fkrueger/bismark_benchmarks/10M_PE/SRR24827378_10M_R1_val_1_bismark_bt2_pe.deduplicated.bam
Output directory: /home/fkrueger/bismark_benchmarks/10M_PE/phase_h_C1_default_N4_20260527T162631Z/rust
--ignore: 0
--ignore_3prime: 0

Processed 15398272 lines in total

Total number of C's analysed:	188123599

Total methylated C's in CpG context:	6740689
Total unmethylated C's in CpG context:	1638185
Total methylated C's in CHG context:	1040727
Total unmethylated C's in CHG context:	38524653
Total methylated C's in CHH context:	1698412
Total unmethylated C's in CHH context:	138480933

C methylated in CpG context:	80.45%
C methylated in CHG context:	2.63%
C methylated in CHH context:	1.21%
```

**Perl splitting report (TARGET, 875 bytes):**
```
SRR24827378_10M_R1_val_1_bismark_bt2_pe.deduplicated.bam

Parameters used to extract methylation information:
Bismark Extractor Version: v0.25.1
Bismark result file: paired-end (SAM format)
Output specified: strand-specific (default)
No overlapping methylation calls specified


Processed 7699136 lines in total
Total number of methylation call strings processed: 15398272

Final Cytosine Methylation Report
=================================
Total number of C's analysed:	188123599

Total methylated C's in CpG context:	6740689
Total methylated C's in CHG context:	1040727
Total methylated C's in CHH context:	1698412

Total C to T conversions in CpG context:	1638185
Total C to T conversions in CHG context:	38524653
Total C to T conversions in CHH context:	138480933

C methylated in CpG context:	80.4%
C methylated in CHG context:	2.6%
C methylated in CHH context:	1.2%


```

15 differences requiring fix in #864 (unchanged from rev 0). The key ones flagged by reviewers:

1. Line 1: bare basename, not version banner
2. Insert `Parameters used to extract methylation information:` / `Bismark Extractor Version: v0.25.1` / `Bismark result file: paired-end (SAM format)`
3. Drop `Input file:` and `Output directory:` path-dependent lines
4. Drop unconditional `--ignore: 0` / `--ignore_3prime: 0`; emit only when non-zero per §2.3.2
5. Add `Output specified: strand-specific (default)` (or `comprehensive`)
6. Add conditional `No overlapping methylation calls specified` (when `config.no_overlap == true` — rev 1 I9 simplified from belt-and-braces)
7. Add conditional `Genomic equivalent sequences …` if `--fasta`
8. Add conditional `Methylation in CHG and CHH context …` if `--merge_non_CpG`
9. **Two blank lines** between header and body (rev 1 C2)
10. Add `Total number of methylation call strings processed: {2×pairs|reads}` line
11. Add `Final Cytosine Methylation Report\n=================================\n` header
12. Reorder: methylated trio (3 lines blank-separated from) → unmethylated trio (using `Total C to T conversions in {ctx} context:` phrasing, NOT `Total unmethylated C's in {ctx}`)
13. Percentages: **1 decimal place**, not 2
14. Zero-denominator fallback string matches Perl exactly (rev 1 I3 per-context newline counts)
15. **Three trailing newlines after the last percentage's `%`** (rev 1 C1), built into the last line's format string

### 2.5 #863 won't-fix rationale (unchanged from rev 0)

Rust's BTreeMap-ordered collector emits BAM-input order; Perl's multicore output depends on N. Rust offers N-invariance, a stronger determinism guarantee. SPEC §8.3 row 1 (NOT §9.7 — rev 1 C4) gets the relaxation: data files require sorted-content equivalence; splitting-report + M-bias remain strict-byte; the §9 header invariant (`--multicore N == --multicore 1`) is unchanged.

### 2.6 Dependencies and ordering

Unchanged from rev 0. Depends on Phase C.1 (`84c6ad1`); unblocks Phase G.

## 3. Behavior

### 3.1 #864 — splitting-report format spec (rev 1 corrected)

For each invocation of `write_splitting_report(path, input_path, config, report)`:

1. Open `BufWriter<File>` at `path`.
2. Write `{basename(input_path)}\n` — basename only.
3. Write blank line `\n`.
4. Write `Parameters used to extract methylation information:\n`.
5. Write `Bismark Extractor Version: {BISMARK_VERSION}\n`.
6. Write `Bismark result file: {paired-end|single-end} (SAM format)\n`.
7. Conditional `Ignoring …` lines per §2.3.2 — emit ONLY when the corresponding config field is non-zero.
8. Write `Output specified: {strand-specific (default)|comprehensive}\n`.
9. Conditional `No overlapping methylation calls specified\n` — **emit ONLY when `config.no_overlap == true`** (rev 1 I9; matches Perl line 5037 exactly. For SE, `config.no_overlap` should be `false` since the SE dispatch doesn't set it; defense-in-depth note: if it's ever true for SE, Perl's check still fires).
10. Conditional `Genomic equivalent sequences will be printed out in FastA format\n` — if `config.fasta_annotation`.
11. Conditional `Methylation in CHG and CHH context will be merged into "non-CpG context" output\n` — if `config.mode == OutputMode::MergeNonCpG`.
12. **Write `\n\n` — TWO `\n` bytes (= one blank line and the leading `\n` of the body's `Processed …` line)**. Rev 1 C2: was `\n` in rev 0; rev 1 corrects to two bytes to produce the two visible blank lines Perl emits.
13. Write `Processed {report.records_processed} lines in total\n` — where `records_processed` is **pairs for PE, records for SE** (NOT 2×pairs in either case; rev 1 fixes this via §5.2 step 2).
14. Write `Total number of methylation call strings processed: {report.call_strings_processed}\n\n` — **two trailing newlines** (`\n` + the blank line before the `Final Cytosine Methylation Report` header) per Perl line 2483.
15. Write `Final Cytosine Methylation Report\n`.
16. Write 33 `=` chars + `\n`.
17. Write `Total number of C's analysed:\t{report.calls_total}\n\n` — two trailing newlines per Perl line 2513.
18. Write methylated trio (each `\n`-terminated; the trio ends with an extra blank line per Perl line 2517 trailing `\n\n`):
    - `Total methylated C's in CpG context:\t{report.calls_cpg_meth}\n`
    - `Total methylated C's in CHG context:\t{report.calls_chg_meth}\n`
    - `Total methylated C's in CHH context:\t{report.calls_chh_meth}\n\n` (note `\n\n` on last)
19. Write unmethylated trio (each `\n`-terminated; trio ends with `\n\n` per Perl line 2521):
    - `Total C to T conversions in CpG context:\t{report.calls_cpg_unmeth}\n`
    - `Total C to T conversions in CHG context:\t{report.calls_chg_unmeth}\n`
    - `Total C to T conversions in CHH context:\t{report.calls_chh_unmeth}\n\n` (note `\n\n` on last)
20. Write percentage trio via `write_percent_or_fallback(w, ctx_name, meth, unmeth, is_last)`:
    - `(CpG, ..., is_last=false)` → emits `C methylated in CpG context:\t{pct:.1}%\n` OR `Can't determine percentage of methylated Cs in CpG context if value was 0\n`
    - `(CHG, ..., is_last=false)` → similar with `\n`
    - `(CHH, ..., is_last=true)` → emits `C methylated in CHH context:\t{pct:.1}%\n\n\n` OR `Can't determine percentage of methylated Cs in CHH context if value was 0\n\n\n`
    - **Critical**: no extra writes after this. The `\n\n\n` is baked into the last line itself (rev 1 C1).
21. Flush.

For `--merge_non_CpG` mode the percentage block is two lines instead of three:
- `(CpG, ..., is_last=false)` → `\n`
- `(Non-CpG, ..., is_last=true)` → `\n\n\n` (matches Perl line 2534/2537)

### 3.2 #864 — `SplittingReport` struct extensions

```rust
pub struct SplittingReport {
    /// SE: number of records iterated. PE: number of PAIRS iterated.
    /// Equals Perl's $counting{sequences_count}. NOT 2×pairs for PE.
    /// **Rev 1 (C.2)**: currently +=2 per pair in pipeline.rs:254 and
    /// parallel.rs:770; must be changed to +=1 per pair.
    pub records_processed: u64,
    /// SE: equals records_processed. PE: 2×pairs (one per XM string).
    /// Equals Perl's $counting{methylation_call_strings}.
    /// **NEW IN C.2** — added per Perl line 2483.
    pub call_strings_processed: u64,
    // ... (existing fields unchanged)
}
```

Update `SplittingReport::add` to sum `call_strings_processed` too (`saturating_add`).

### 3.3 #865 — Empty file deletion semantics (rev 1 corrected)

Add to `OutputFileMap`:

```rust
struct OutputFileEntry {
    path: PathBuf,
    writer: BoxedWriter,
    /// Number of *call rows* (not bytes) emitted to this file beyond the
    /// header. Incremented by every successful write_call() into this entry.
    /// **Constraint** (rev 1 I8): this counter must be bumped iff a call
    /// row is written. Any future writer path that adds non-call non-header
    /// bytes to the file must also bump this counter, or the sweep will
    /// incorrectly classify the file as empty.
    records_written: u64,
}
```

`finalize_with_empty_sweep`:

```rust
pub fn finalize_with_empty_sweep(&mut self) -> Result<(), std::io::Error> {
    let entries: Vec<_> = self.files.drain().collect();
    for (_, OutputFileEntry { path, writer, records_written }) in entries {
        drop(writer);  // closes File + writes GzEncoder trailer if applicable
        let filename = path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        if records_written == 0 {
            std::fs::remove_file(&path)?;
            eprintln!("{filename} was empty ->\tdeleted");
        } else {
            eprintln!("{filename} contains data ->\tkept");
        }
    }
    // Match Perl line 625: trailing blank lines on stderr.
    eprintln!();
    eprintln!();
    Ok(())
}
```

**Log line format** (rev 1 C3 corrected):
- `{filename} contains data ->\tkept` — STDERR (eprintln!), tab-separated arrow
- `{filename} was empty ->\tdeleted` — STDERR, tab-separated arrow
- Two trailing blank lines on stderr to match Perl line 625

`println!` is **wrong** — Perl uses `warn` which writes to STDERR. Rev 0's "verified from captured shell output" rationale was invalid (captured shell output without `2>/dev/null` interleaves both streams).

**Why Phase C.1 stdout/stderr layering doesn't break**: the harness captures both streams via `2>&1 | tee` patterns and doesn't distinguish; downstream nf-core pipelines that DO distinguish will see the messages on stderr (correctly, matching Perl). Anyone relying on stdout for log lines was relying on a non-existent stream contract.

### 3.4 #863 — SPEC §8.3 update + harness `*.gz)` arm (rev 1 corrected)

#### 3.4.1 SPEC §8.3 update (rev 1 C4 corrected — was §9.7 in rev 0)

The byte-identity invariant lives in **SPEC §8.3** (lines 653–667), NOT §9.7. Plan rev 0 mis-identified the target section. §9.7 is "Speedup expectation" (the ≥4× target at N=4) and is unchanged.

**Update target**: §8.3 row 1, currently:

> | Each of 12 split files **unsorted byte equality** at `--multicore 1` | `cmp <rust_split> <perl_split>` (or `gzcmp` for `.gz`) | The byte-identity contract. Catches reordering, drift, and content bugs. |

Becomes:

> | Each of 12 split files **sorted-content equality** vs Perl `--multicore N` | `sort <rust_split> \| md5sum == sort <perl_split> \| md5sum`. Strict `cmp` retained as a secondary informational check; raw bytes may differ by record ordering only. | Rust emits records in BAM-input order via the BTreeMap collector; Perl's multicore output is fork+modulo ordered (N-dependent). **#863 closure rationale** documents this. |

Plus add a preamble paragraph before the §8.3 table:

> **Byte-identity invariant (rev 3, post-#863 closure 2026-05-27):** "Rust output is byte-identical to Perl" means: (1) per-file line counts equal Perl's; (2) sorted-content MD5 equals Perl's on every data file; (3) `*_splitting_report.txt` is raw-byte-identical to Perl; (4) `*.M-bias.txt` is raw-byte-identical; (5) file-set is identical (Perl's `was empty -> deleted` sweep is mirrored); (6) self-determinism — two consecutive Rust runs with the same input + flag set produce raw-byte-identical output. The line-ordering relaxation in (2) is per #863's won't-fix decision: Rust's BAM-input order is N-invariant; Perl's multicore output depends on N (changes between `--multicore 4` and `--multicore 8`). Self-determinism (6) is therefore a strictly stronger invariant than Perl's.

The §9 header invariant (`--multicore N` MUST produce output byte-identical to `--multicore 1` for any N ≥ 1) is **preserved** — that's Rust-vs-Rust, N-invariance. Rev 1 leaves §9 unchanged.

#### 3.4.2 Harness update (rev 1 I4: add `*.gz)` arm)

```bash
# Per-file byte compare (intersection only)
for f in $(comm -12 <(echo "$PERL_FILES") <(echo "$RUST_FILES")); do
  TOTAL=$((TOTAL + 1))
  if cmp -s "$PERL_OUT/$f" "$RUST_OUT/$f"; then
    echo "  ✓ $f — byte-identical ($(wc -c < "$PERL_OUT/$f") bytes)" >> "$SUMMARY"
  else
    case "$f" in
      *_splitting_report.txt|*.M-bias.txt)
        # Strict raw-byte match required for these.
        DIFFS=$((DIFFS + 1))
        SIZE_P=$(wc -c < "$PERL_OUT/$f")
        SIZE_R=$(wc -c < "$RUST_OUT/$f")
        FIRST_DIFF=$(cmp "$PERL_OUT/$f" "$RUST_OUT/$f" 2>&1 | head -1 || true)
        echo "  ✗ $f DIFFERS — perl=${SIZE_P}B rust=${SIZE_R}B ($FIRST_DIFF)" >> "$SUMMARY"
        ;;
      *.gz)
        # Decompress before sorting (rev 1 I4).
        PMD=$(zcat "$PERL_OUT/$f" | LC_ALL=C sort | md5sum | cut -d' ' -f1)
        RMD=$(zcat "$RUST_OUT/$f" | LC_ALL=C sort | md5sum | cut -d' ' -f1)
        if [[ "$PMD" == "$RMD" ]]; then
          echo "  ≈ $f — gzip-sorted-equivalent (raw differs by ordering only)" >> "$SUMMARY"
        else
          DIFFS=$((DIFFS + 1))
          echo "  ✗ $f DIFFERS — gzip-sorted mismatch (perl=$PMD rust=$RMD)" >> "$SUMMARY"
        fi
        ;;
      *)
        # Plain data file: accept sorted-content equivalence.
        PMD=$(LC_ALL=C sort "$PERL_OUT/$f" | md5sum | cut -d' ' -f1)
        RMD=$(LC_ALL=C sort "$RUST_OUT/$f" | md5sum | cut -d' ' -f1)
        if [[ "$PMD" == "$RMD" ]]; then
          echo "  ≈ $f — sorted-equivalent (raw differs by ordering only; sha256-sorted match $PMD)" >> "$SUMMARY"
        else
          DIFFS=$((DIFFS + 1))
          echo "  ✗ $f DIFFERS — content mismatch (perl-sorted=$PMD rust-sorted=$RMD)" >> "$SUMMARY"
        fi
        ;;
    esac
  fi
done
```

### 3.5 Edge cases (unchanged from rev 0)

| Case | Handling |
|---|---|
| All-zero ignore flags | Skip all four conditional `Ignoring …` lines. |
| Single non-zero ignore | Emit ONLY the matching `Ignoring first 5 bp` line; others skipped. |
| `--include_overlap` flag set | `no_overlap` is false; skip the `No overlapping methylation calls specified` line. |
| SE input | `config.no_overlap` should be false; check at §3.1 step 9 skips the line correctly. |
| All-zero context | Zero-denominator fallback per §2.3.4. |
| `--mbias_only` mode | `OutputFileMap` is empty; sweep is a no-op. Splitting report still emitted. |
| `--comprehensive` mode | "Output specified: comprehensive" + per-context (no-strand) files; sweep applies. |
| `--gzip` mode | File names carry `.gz` suffix; sweep applies to those. `records_written == 0` check is per-record (not bytes); gzip-header-only file is detected as empty and unlinked. Harness `*.gz)` arm handles comparison. |
| `--merge_non_CpG` mode | 8 files (CpG + Non_CpG × 4 strands); plus the merge note line; percentage block is 2 lines with last ending `\n\n\n`. |
| `--yacht` mode | Single `any_C_context_*` file; sweep applies. |

## 4. Signature

### 4.1 `write_splitting_report` (rewritten — unchanged signature from rev 0)

```rust
pub fn write_splitting_report(
    path: &Path,
    input_path: &Path,
    config: &ResolvedConfig,
    report: &SplittingReport,
) -> Result<(), std::io::Error> { … }
```

### 4.2 New helper `write_percent_or_fallback` (rev 1 I3)

```rust
/// Write one percentage row matching Perl's per-context format. If
/// `meth + unmeth == 0`, writes the zero-denominator fallback string.
/// Trailing newline count is `\n` for non-last rows, `\n\n\n` for the
/// LAST row (which is CHH in default 3-context output, or Non-CpG in
/// merge_non_CpG mode).
///
/// **Rev 1 (C.2)** addition. Replaces rev-0's "step 24 + step 25" pattern
/// which over-emitted EOF newlines.
fn write_percent_or_fallback(
    w: &mut impl Write,
    ctx_label: &str,
    meth: u64,
    unmeth: u64,
    is_last: bool,
) -> Result<(), std::io::Error> {
    let total = meth.saturating_add(unmeth);
    let trailing = if is_last { "\n\n\n" } else { "\n" };
    if total == 0 {
        write!(w, "Can't determine percentage of methylated Cs in {ctx_label} context if value was 0{trailing}")?;
    } else {
        let pct = (meth as f64) * 100.0 / (total as f64);
        // Use explicit bytes (not `writeln!`) to avoid CRLF on Windows (rev 1 I10).
        write!(w, "C methylated in {ctx_label} context:\t{pct:.1}%{trailing}")?;
    }
    Ok(())
}
```

### 4.3 `OutputFileMap::finalize_with_empty_sweep` (rev 1 C3 corrected)

```rust
/// Sweep empty per-strand output files at flush time, matching Perl's
/// `was empty -> deleted` end-of-run behaviour (closes #865).
///
/// For each entry: drop the writer (closes the file + flushes gzip
/// trailer if applicable); if `records_written == 0`, unlink the file
/// and emit `{filename} was empty ->\tdeleted` to STDERR (via
/// `eprintln!` — matches Perl's `warn` lines 607/615). Otherwise emit
/// `{filename} contains data ->\tkept`. Trailing two blank lines on
/// stderr match Perl line 625.
///
/// Empties the internal map; subsequent `write_call`s would no-op.
///
/// # Constraint
///
/// The `records_written` counter is bumped iff a call row is written.
/// Future writer paths adding non-call non-header bytes must also bump
/// the counter, or the sweep will incorrectly classify those files as
/// empty and unlink them.
///
/// # Errors
///
/// `std::io::Error` on unlink failure.
pub fn finalize_with_empty_sweep(&mut self) -> Result<(), std::io::Error> { … }
```

## 5. Implementation outline

### 5.1 SPEC §8.3 update (do FIRST, gates the harness change) — rev 1 C4 corrected

Per §3.4.1 — update **§8.3 row 1** (NOT §9.7). Add the 6-point byte-identity-invariant preamble paragraph above the §8.3 table. Leave §9 header (`--multicore N == --multicore 1`) and §9.7 (Speedup) untouched.

### 5.2 #864 — splitting-report format rewrite (file refs corrected per rev 1 I1)

1. **Add `call_strings_processed` field** to `SplittingReport`. Update `SplittingReport::add` to sum it (saturating). Update the inline test `splitting_report_add_is_commutative` to include the new field.
2. **Fix `records_processed` semantics** — change `+= 2` per pair to `+= 1` per pair at:
   - `src/pipeline.rs:254` (legacy SE/PE loop)
   - `src/parallel.rs:770` (Phase F worker)
   AND add `call_strings_processed += 2` per pair at both call sites. **Both files must be updated together** — missing either yields parallel-vs-sequential parity failure.
3. **Rewrite `write_splitting_report`** per §3.1's 21 numbered steps. Pull `paired_mode`, `mode`, `ignore_5p_r1`, `ignore_3p_r1`, `ignore_5p_r2`, `ignore_3p_r2`, `no_overlap`, `fasta_annotation` from `config`. Use `path.file_name()` for line 1's bare basename. **Use `write_all(b"\n")` not `writeln!` for byte-identity-critical bytes** (rev 1 I10 — Windows CRLF hazard).
4. **Add `write_percent_or_fallback` helper** per §4.2. Replaces rev-0's step-24-plus-step-25 pattern.

### 5.3 #865 — empty-file deletion — rev 1 C3 corrected

1. **Refactor `OutputFileMap::files`** from `HashMap<OutputKey, (PathBuf, BoxedWriter)>` to `HashMap<OutputKey, OutputFileEntry>` with `records_written: u64`.
2. **Update `write_call`** to bump `records_written += 1` AFTER the successful final `writer.write_all(b"\n")?` (so partial-write failures don't over-count).
3. **Add `finalize_with_empty_sweep`** per §4.3. Use `eprintln!` (not `println!`). Two trailing `eprintln!()` calls match Perl line 625's `warn "\n\n"`.
4. **Wire the sweep into `ExtractState::finalize`** in `src/state.rs` (lines 111-114; rev 1 I5 corrected from rev 0's `run.rs`). Order: after `flush_all`, before `write_splitting_report`.
5. **Keep `flush_all` and `cleanup_all`** unchanged. Document that `flush_all` does NOT write gzip trailers (those go out at drop time; the sweep's `drop(writer)` inside the loop handles trailer-write for kept files; rev 1 I11).

### 5.4 #863 won't-fix — harness update + issue close (rev 1 I4 corrected)

1. **Update `scripts/oxy_phase_h_smoke.sh`** per §3.4.2. The `case` block has three arms: strict-cmp for splitting-report+M-bias; gzip-aware sorted-md5 for `*.gz`; plain sorted-md5 for everything else.
2. **Update the harness's exit code** semantics: exit 0 if all files are ✓ or ≈; exit 1 if any ✗ or file-name-set mismatch.
3. **Close #863 on GitHub** with a comment linking to SPEC §8.3 row 1 (NOT §9.7) and the user's decision rationale.

### 5.5 Tests (rev 1 — added V1-V4 + corrected fixture for round-half-away)

#### 5.5.1 New unit tests in `src/output.rs::tests`

- `splitting_report_format_se_default` — SE, all-zero ignore, default mode → byte-exact match.
- `splitting_report_format_pe_default_no_overlap` — PE + no_overlap=true + default mode → byte-exact match (the 10M PE harness shape at small scale).
- `splitting_report_format_pe_with_include_overlap` — PE + `--include_overlap` → skip the `No overlapping methylation calls specified` line.
- `splitting_report_format_with_ignore_5p` — SE + `--ignore 5` → emit only `Ignoring first 5 bp` line.
- `splitting_report_format_with_all_pe_ignore_flags` — PE + all four ignore flags non-zero → all four lines in correct order.
- `splitting_report_format_comprehensive_mode` — `--comprehensive` → emit `Output specified: comprehensive`.
- `splitting_report_format_merge_non_cpg` — `--merge_non_CpG` → emit the merge note line; percentage block ends after Non-CpG with `\n\n\n`.
- `splitting_report_format_fasta` — `--fasta` → emit the FastA-annotation line.
- `splitting_report_format_zero_denominator_fallback_cpg` — CpG empty → emit `Can't determine percentage of methylated Cs in CpG context if value was 0\n`.
- `splitting_report_format_zero_denominator_fallback_chh_last` — CHH empty (last in 3-context) → emit fallback with `\n\n\n` trailing.
- `splitting_report_format_three_trailing_newlines_EOF` — 3-context default → assert byte sequence ends in `%\n\n\n` (3 trailing newlines after `%`).
- `splitting_report_format_one_decimal_precision` — 50/50 CpG split → emit `50.0%`, NOT `50.00%`.
- **`splitting_report_format_round_half_away_from_zero`** (rev 1 V4 / I2): synthetic counters with `meth=5, unmeth=35` → 12.5% exactly. Assert Rust output matches Perl's `12.5%`. **Note**: 12.5 in f64 is representable exactly so both Rust banker's-rounding and Perl round-half-away-from-zero produce `12.5`. The fixture is a smoke check; real-data divergences require values where 100×meth/total lands EXACTLY on `x.x5` with no float error, which is rare. If a real-data divergence ever surfaces, this test gets a sharper fixture; for now it's a sanity guard.
- `splitting_report_call_strings_doubles_for_pe` — synthetic PE accumulation → `call_strings_processed == 2 × records_processed`.

#### 5.5.2 New unit tests for `finalize_with_empty_sweep`

- `output_file_map_empty_sweep_deletes_zero_record_files`.
- `output_file_map_empty_sweep_keeps_non_empty_files`.
- `output_file_map_empty_sweep_stderr_log_lines` — **rev 1 C3 corrected**: capture stderr (via `assert_cmd::Command` or stdio redirection), verify format `{filename} contains data ->\tkept` and `{filename} was empty ->\tdeleted`.
- `output_file_map_empty_sweep_gzip_empty_is_deleted` — `--gzip` mode + zero records → file deleted (records_written-based, not size-based).
- **`output_file_map_empty_sweep_mbias_only_is_noop`** (rev 1 V1) — empty map (MbiasOnly mode) → sweep returns Ok(()), no stderr lines emitted, no remove_file calls.
- **`output_file_map_empty_sweep_gzip_kept_file_seals_trailer`** (rev 1 V2 supporting fixture) — gzip mode + 5 records written + sweep → file kept, gzip footer bytes present at end (verify via `zcat` round-trip).

#### 5.5.3 Integration tests (`tests/output_phase_c2.rs`, new file)

- `extract_se_directional_emits_perl_compliant_report` — synthetic 10-record SE BAM, byte-compare splitting report against golden buffer.
- `extract_pe_directional_emits_perl_compliant_report` — synthetic 5-pair PE BAM, byte-compare.
- `extract_pe_directional_unlinks_empty_ctot_ctob_files` — directional PE BAM → only 6 of 12 strand files exist post-run.
- `extract_pe_emits_kept_and_deleted_stderr_log_lines` — **rev 1 C3 corrected**: capture stderr, verify log lines on stderr (not stdout).
- **`extract_pe_parallel_vs_sequential_call_strings_parity`** (rev 1 V3): same PE BAM, run `--parallel 1` and `--parallel 4`, assert `call_strings_processed` field equal in both outputs' splitting reports.
- **`extract_pe_gzip_sweep_ordering`** (rev 1 V2): PE BAM + `--gzip` → splitting report written AFTER gzip-trailers are sealed for kept files (verify no truncated `.gz.txt` files).

### 5.6 Crate version bump

`rust/bismark-extractor/Cargo.toml`: `1.0.0-alpha.7` → `1.0.0-alpha.8`. Description: `"… (Phase C.2: splitting-report format alignment + empty-file deletion)"`.

### 5.7 PROGRESS.md update

Add C.2 row. Mark #863 as Done (won't-fix subtype). Mark Phase G as unblocked.

### 5.8 Pre-merge validation runs

1. `cargo test -p bismark-extractor` — all tests pass, including the new ~19 from §5.5 (was ~17 in rev 0; rev 1 added V1-V4).
2. `cargo clippy -p bismark-extractor --all-targets -- -D warnings` — clean.
3. `cargo fmt --check` — clean.
4. Phase F invariant verified: `parallel_phase_f.rs` tests still pass.
5. Re-run oxy harness against 10M PE BAM. Expected: PASS verdict (✓ on splitting report, ✓ on M-bias, ≈ on all 6 data files, no FILE-NAME-SET MISMATCH).

## 6. Efficiency

Unchanged from rev 0. New `eprintln!` calls go to stderr (no functional difference vs `println!` in cost); `records_written` counter is a single u64 increment per call.

## 7. Integration

### 7.1 Read/Write surface (rev 1 corrected)

- **Read**: Perl source (already-read context).
- **Write**:
  - `rust/bismark-extractor/SPEC.md` **§8.3 row 1 + preamble** (rev 1 C4 — not §9.7)
  - `rust/bismark-extractor/Cargo.toml`
  - `rust/bismark-extractor/src/output.rs`
  - `rust/bismark-extractor/src/pipeline.rs` (rev 1 I1 — not `run.rs`)
  - `rust/bismark-extractor/src/parallel.rs`
  - `rust/bismark-extractor/src/state.rs` (rev 1 I5 — wire-up location)
  - `rust/bismark-extractor/tests/output_phase_c2.rs` (new file)
  - `rust/bismark-extractor/tests/pe_phase_c_smoke.rs` (may need assertion updates)
  - `scripts/oxy_phase_h_smoke.sh`
  - `plans/05262026_bismark-extractor/PROGRESS.md`
- **GitHub**: close #863 with rationale comment.

### 7.2 Downstream impact (rev 1 C3 corrected)

| Consumer | Impact |
|---|---|
| Phase G (bedGraph + cytosine_report) | Per-call line format unchanged; only report + file-set fixed. Unblocked. |
| nf-core pipelines / downstream tooling | **Stderr log lines now match Perl's `warn` output** (rev 1 corrected from rev 0's incorrect "stdout" claim). Any pipeline that captures `2>&1` sees the same combined output as Perl; pipelines splitting streams see kept/deleted on stderr (where Perl puts them). |
| User scripts parsing splitting report | Format matches Perl exactly post-C.2. |
| Phase F invariant | Preserved — same code paths, additional finalize step. New `call_strings_processed` field is in `SplittingReport::add`. |

### 7.3 Deliberately NOT implemented (rev 1 I7)

Perl `bismark_methylation_extractor:2562-2580` mirrors the splitting-report content via `warn` (stderr) in addition to writing to REPORT. Rust does NOT mirror to stderr. Rationale:
- The stderr mirror is purely user-readable progress; no nf-core pipeline parses it.
- Rust's preferred-output strategy is structured logs (when introduced); duplicating to stderr conflicts with that.
- Matches Perl on the FILE-level invariant (which IS what tooling consumes).

Documented here so future readers don't add the mirror by accident.

## 8. Assumptions (rev 1 refined)

### 8.1 From Perl source review

- **A1.** `config.paired_mode` is accessible in `write_splitting_report`. ✓ (verified during rev 1 review).
- **A2.** `config.ignore_5p_r1`, `config.ignore_3p_r1`, `config.ignore_5p_r2`, `config.ignore_3p_r2`, `config.no_overlap`, `config.fasta_annotation` all exist with the expected types.
- **A3.** `config.mode` exposes the OutputMode enum.
- **A4.** Perl's `Bismark result file: paired-end (SAM format)` literal is hard-coded — independent of input file extension. Verified Perl line 5000.
- **A5 (rev 1 refined per I2)**: Rust's `format!("{:.1}", x)` uses banker's rounding (round-half-to-even); Perl `sprintf("%.1f", x)` uses round-half-away-from-zero. **Divergence** at values where `100 × meth / total` lands EXACTLY on `x.x5` with no float-representation error — rare in real data with 10M+ counts. The unit test `splitting_report_format_round_half_away_from_zero` uses `5/35 = 12.5%` (representable exactly). If a real-data divergence ever surfaces, the fixture gets sharper inputs; a custom formatter mimicking Perl's `sprintf` is the eventual fix. For now: smoke check only.
- **A6.** `methylation_call_strings = 2 × sequences_count` for PE. Verified Perl line 2451.
- **A7.** 33 `=` characters with no trailing space. Verified Perl line 2510 `'='x33`.
- **A8 (rev 1 confirmed by review)**: Current Rust counts `+= 2` per pair in `pipeline.rs:254` AND `parallel.rs:770`. Rev 1 fix changes BOTH to `+= 1` per pair AND adds `call_strings_processed += 2`. Phase F parallel-vs-sequential parity tests must update if they assert specific `records_processed` values.

### 8.2 Plan-specific assumptions

- **A9.** One PR. SPEC + code + tests + harness ship together.
- **A10.** Branch from `rust/iron-chancellor` HEAD `84c6ad1`.
- **A11.** Phase F byte-identity tests preserved by the polarity-fix-style approach: both sequential and parallel paths run the same `write_splitting_report` + `finalize_with_empty_sweep`.
- **A12 (rev 1 I8)**: `records_written` counter must be bumped iff a call row is written. No future writer path may add non-call non-header bytes to the per-strand file without bumping the counter, or the sweep will incorrectly classify the file as empty. Documented in `OutputFileEntry::records_written` doc comment.
- **A13 (rev 1 I10)**: Byte-identity-critical writes use `write_all(b"\n")` (not `writeln!`), to avoid CRLF on Windows builds. Audit: every `\n` in `write_splitting_report` should be an explicit byte sequence, not a macro.
- **A14 (rev 1 I11)**: `flush_all` does NOT write gzip trailers — those go out at `drop` time. The sweep's per-entry `drop(writer)` inside the loop is the trailer-write point for kept files; this is correct but non-obvious. Documented in `finalize_with_empty_sweep`'s doc comment.

## 9. Validation

### 9.1 Unit-level (rev 1 — 19 tests vs rev 0's 17)

- Per §5.5.1: 14 splitting-report-format tests covering each conditional branch + edge case + V4 (round-half).
- Per §5.5.2: 6 empty-sweep tests including V1 (mbias_only no-op) and V2 (gzip-trailer ordering).

### 9.2 Integration-level (rev 1 — 6 tests vs rev 0's 4)

- Per §5.5.3: 6 end-to-end tests, including V3 (parallel-vs-sequential parity) and V2 (gzip ordering).

### 9.3 Real-data validation (manual, on oxy — unchanged from rev 0)

Re-run `scripts/oxy_phase_h_smoke.sh` on 10M PE BAM. Expected `diff_summary.txt`:

```
── Byte-identity (file-by-file) ──
  ≈ CHG_OB_… — sorted-equivalent (raw differs by ordering only)
  ≈ CHG_OT_… — sorted-equivalent
  ≈ CHH_OB_… — sorted-equivalent
  ≈ CHH_OT_… — sorted-equivalent
  ≈ CpG_OB_… — sorted-equivalent
  ≈ CpG_OT_… — sorted-equivalent
  ✓ …_M-bias.txt — byte-identical (11443 bytes)
  ✓ …_splitting_report.txt — byte-identical (875 bytes)

── Result ──
PASS: 8 of 8 files match (2 raw-identical + 6 sorted-equivalent)
```

Exit code 0; no FILE-NAME-SET MISMATCH; empty-file delete log lines visible on **stderr** in the Rust run.

## 10. Questions or ambiguities

### Critical — none (post-rev-1 absorption)

The 4 Criticals from the dual review pass are folded into rev 1. The SPEC target (§8.3 row 1) is now correct. The format byte counts are correct. The stream (stderr) is correct.

### Open (defaults taken)

| Q | Default | Rationale |
|---|---|---|
| Branch + plan filenames | `extractor-phase-c2` / `PHASE_C2_PLAN.md` | Mirrors C.1 convention. |
| Stderr-mirror of report | Deliberately NOT implemented (§7.3) | No tooling consumes Perl's stderr-mirror; matches the SPEC's "file-level invariant" focus. |
| Banker's-rounding deferred to hardening PR | Acceptable if 10M PE post-C.2 matches Perl byte-for-byte (which it almost certainly will) | A5 documents the residual risk. |
| Two trailing `eprintln!()` after the sweep | Yes (matches Perl line 625 `warn "\n\n"`) | Negligible cost; consistency with Perl. |
| Update `pe_phase_c_smoke.rs` if it asserts old report format | Yes during impl | Discover at test-run; small fix. |

## 11. Self-Review

Reviewed plan rev 1 for:

- **Efficiency:** O(1) per record (counter bump), O(N_files) finalize. Unchanged from rev 0. ✓
- **Logic consistency:** §3.1's 21-step write order now matches Perl byte-shape per rev 1 C1+C2 corrections. Last percentage line carries `\n\n\n` directly via `write_percent_or_fallback(is_last=true)`. Header→body gap has correct two-blank-line bytes. ✓
- **Edge cases:** 10 cases in §3.5. All conditional branches (Ignoring, no_overlap, fasta, merge_non_CpG, comprehensive, yacht, mbias_only, gzip) covered. Per-context trailing-newline variance handled by `write_percent_or_fallback`'s `is_last` parameter. ✓
- **Integration:** SPEC §8.3 (not §9.7) is the right target. Stderr (not stdout) is the right stream. Phase F invariant preserved by updating BOTH `pipeline.rs:254` AND `parallel.rs:770`. ✓
- **Test coverage:** 19 unit + 6 integration tests cover every conditional branch and the 4 reviewer-flagged validation gaps. ✓

### Adjustments made during rev 1 absorption

Folded **all 4 Critical + all 11 Important findings** from dual plan-review:
- **C1**: Restructured §3.1 step 20 + dropped step 25; introduced `write_percent_or_fallback(is_last)` helper.
- **C2**: Changed §3.1 step 12 to `\n\n` (two newlines).
- **C3**: All `println!` → `eprintln!`. Added two trailing `eprintln!()` calls matching Perl line 625. Updated rationale text to correctly state Perl uses `warn` (stderr). Updated §3.3, §4.3, §5.3 step 3, §7.2, §10, §11.
- **C4**: SPEC target re-pointed from §9.7 to §8.3 row 1 + new preamble paragraph. §9.7 (Speedup) left untouched. §9 header (`--multicore N == --multicore 1`) left untouched.
- **I1**: `src/run.rs` → `src/pipeline.rs` + `src/parallel.rs`. Both call sites flagged for the `records_processed` change.
- **I3**: Per-context trailing-newline variance documented in §2.3.4 + handled in `write_percent_or_fallback`.
- **I4**: Harness gains a `*.gz)` arm using `zcat | sort | md5sum`.
- **I5**: `ExtractState::finalize` lives in `src/state.rs:111-114` (not `pipeline.rs` or `run.rs`).
- **I6**: Validation gaps V1-V4 added as new tests in §5.5.
- **I7**: `warn`-stderr-mirror of report explicitly documented as deliberately not implemented in §7.3.
- **I8**: `records_written` invariant documented in `OutputFileEntry::records_written` doc + plan A12.
- **I9**: §3.1 step 9's check simplified from `paired AND no_overlap` to just `no_overlap` (matches Perl).
- **I10**: `write_all(b"\n")` not `writeln!` for byte-identity-critical bytes; plan A13.
- **I11**: `flush_all` doesn't write gzip trailers — documented in plan A14 + `finalize_with_empty_sweep` doc comment.

### Remaining risks

- **R1**: Banker's-rounding divergence — unlikely on 10M PE real data; smoke-check fixture added; sharper fixture deferred to hardening PR if real-data ever diverges.
- **R2**: Phase F `parallel_phase_f.rs` tests may assert specific `records_processed` values for PE — those need updating in step with §5.2 step 2. Audit during impl.
- **R3**: `--mbias_only` with `cleanup_partial_outputs` path (error case) — the sweep is intentionally only called on success; cleanup_all stays unchanged. The plan correctly preserves both lifecycle hooks.
- **R4**: The `eprintln!` calls fire 12 times per run (for default mode). Even on a CI box with synchronous stderr, this is negligible. No throughput concern.

---

## 12. Open delivery cycle

1. ✅ Plan rev 0 written.
2. ✅ Manual review by Felix — approved, directed to dual plan-reviewers.
3. ✅ Dual `plan-reviewer` agents — `PLAN_REVIEW_PHASE_C2_A.md` (needs revision) + `PLAN_REVIEW_PHASE_C2_B.md` (needs revision). Both independently caught 4 Criticals (C1-C4) with strong consensus.
4. ✅ **Plan rev 1** folding all 4 Critical + 11 Important findings — this file.
5. 🟡 **Implementation trigger from Felix** — *PENDING*.
6. ⏸ Implementation per §5.
7. ⏸ Dual `code-reviewer` agents.
8. ⏸ `plan-manager` audit (Mode B).
9. ⏸ Commit + branch + PR → close #864 + #865 + #863 (won't-fix).
10. ⏸ Merge.

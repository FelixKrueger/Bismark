# SPEC — `filter_non_conversion` Rust port

**Status:** rev 1 — dual plan-review folded in (PLAN_REVIEW_A.md / _B.md). Ready to implement.
**Date:** 2026-05-31.
**Branch / worktree:** `rust/filter-non-conversion` @ `~/Github/Bismark-filternonconv`
(off `origin/rust/iron-chancellor` @ `63d589c`).
**Crate:** `rust/bismark-filter-nonconversion` (binary `filter_non_conversion_rs`),
version `1.0.0-alpha.1`. Added to `rust/Cargo.toml` `members`.
**Part of:** the Bismark Rust rewrite ([[project_rust_rewrite]]). The **last per-read
data tool** of the post-alignment suite.
**Perl source:** `filter_non_conversion` (repo root, v0.25.1, 724 lines).

---

## 1. Purpose & scope

Port Perl `filter_non_conversion` to Rust with a binding byte-identity contract. The
tool reads a Bismark BAM, walks each read's **XM** methylation-call tag, and removes
reads (SE) or read-pairs (PE) that look like incomplete bisulfite conversion (too much
**non-CpG** methylation). It is a **verbatim pass-through filter**: records are written
unchanged; only their routing (kept vs removed) is decided. It does **not** read the
genome, computes **no** strand, and needs **only** the XM tag.

**In scope (v1.0):**
- Three decision modes: `--threshold` (default 3), `--consecutive`, and
  `--percentage_cutoff` + `--minimum_count` (default 5).
- SE (`-s`) and PE (`-p`) modes + `@PG`-based auto-detection.
- BAM input only (matches the Perl `=~ /bam$/` gate).
- Three outputs per input file: kept BAM, removed BAM, filtering report.
- Multiple positional input files, each processed **independently** (no `--multiple`).

**Out of scope (v1.0; deferred to v1.x):**
- SAM / CRAM input (Perl rejects non-BAM at the top gate — see §4.1).
- `--parallel` BGZF threading (single-threaded + mimalloc only — see §10.4).
- Replicating the niche `--samtools_path` resolution machinery (accepted, ignored).
- Secondary/supplementary alignments are **not** an expected Bismark input (see §11 A6).

---

## 2. Resolved decisions (Felix, 2026-05-31)

| # | Decision | Resolution |
|---|----------|------------|
| D1 | **BAM byte-identity gate scope** | **Records body only.** Compare `samtools view` (alignment records: same set kept/removed, same order, same per-read tags). The `@PG` header divergence is ignored (samtools appends a `@PG` line per invocation — verified: a 3-`@PG` input becomes 5-`@PG` through the Perl pipe; noodles adds none — headers can never match). Report compared separately, byte-for-byte (modulo the timing line, D2). |
| D2 | **Report timing line** | **Emit + normalize.** Rust writes the same `filter_non_conversion completed in {d}d {h}h {m}m {s}s` line (faithful, incl. only the LAST file's report getting it in multi-file runs). The gate compares everything before the timing line byte-for-byte, and matches the timing line by prefix/format only. NB the Rust port omits the Perl's two `sleep(1)` calls (in `test_positional_sorting`), so its durations legitimately differ — expected under D2. |
| D3 | **Input formats** | **BAM only.** Match the Perl gate; reject non-BAM with the same `Please provide a BAM file to continue!` error. |
| D4 | **noodles round-trip spike** | **Done + PASS** (`spikes/SPIKE_noodles_roundtrip.md`). noodles `RecordBuf`→BAM→`samtools view` body byte-identical to original AND Perl-pipe across 203 real Bismark PE records. Verbatim-passthrough design validated. |

---

## 3. CLI surface

clap-derived `Cli` → `validate()` → `ResolvedConfig` (mirrors dedup/bam2nuc/c2c scaffolding).

| Flag | Perl `GetOptions` | clap type | Default | Behaviour |
|------|-------------------|-----------|---------|-----------|
| `<files>...` | `@ARGV` | positional | — | One or more BAM files; each processed independently. |
| `-s`, `--single` | `s\|single` | bool | — | Force SE. Mutually exclusive with `-p`. |
| `-p`, `--paired` | `p\|paired` | bool | — | Force PE. Either mate failing removes the pair. |
| `--threshold` | `threshold=i` | **i64** | 3 | Remove if methylated-non-CG count ≥ N. Must be > 0 (validated, not parse-rejected). |
| `--consecutive` | `consecutive` | bool | off | Count CONSECUTIVE methylated non-CG; any `z`/`h`/`x` resets. Mutually exclusive with `--percentage_cutoff`. |
| `--percentage_cutoff` | `percentage_cutoff=i` | **i64** | unset | Remove if non-CG methylation % ≥ P **and** total non-CG ≥ `minimum_count`. Range 0–100 (validated). |
| `--minimum_count` | `minimum_count=i` | **i64** | 5 (only when `--percentage_cutoff` set) | Min non-CG cytosines before the % filter applies. Must be > 0 (validated). |
| `--samtools_path` | `samtools_path=s` | path | — | **Accepted, ignored** (noodles is pure-Rust; no samtools requirement). |
| `--version` | `version` | bool | — | Print version info, exit 0. |
| `--help` | `help` | bool | — | Print help, exit (see §10.1 — clap-style exit 0). |

**Signed integer types (rev 1 / Reviewer B I2):** `--threshold`, `--percentage_cutoff`,
`--minimum_count` are parsed as **signed** `i64`. Perl `GetOptions(...=i)` accepts negatives,
then rejects them in *validation* with a specific die message (e.g. `--threshold -1` →
"Please use a sensible value for -1…"). Using `i64` (not `u32`) ensures the **validation
check** fires (matching the Perl error path), not a clap parse error.

### 3.1 Validation order (matches Perl `process_commandline`, lines 469–606)

1. clap parse failure → exit 2 (clap convention; Perl: `die "Please respecify command line options"`).
2. `--help` → print help, exit (§10.1).
3. `--version` → print version, exit 0.
4. **No positional files → print "Please provide one or more Bismark output files…", help, exit.**
   This precedes option validation: `--percentage_cutoff 200` with **no files** yields the
   no-files exit, NOT the range error (Reviewer A O3). Do not reorder.
5. If `--percentage_cutoff` defined:
   - `die` if `--consecutive` also set (mutually exclusive).
   - `die` unless `0 ≤ percentage_cutoff ≤ 100`.
   - if `--minimum_count` defined: `die` unless `> 0`; else `minimum_count = 5`.
6. `-s` + `-p` together → `die` ("select either -s … or -p …, but not both").
7. `--samtools_path`: accepted, **not** validated/required (Perl deviation — §10.3).
8. `--threshold`: if defined, `die` unless `> 0` (message interpolates the value, Reviewer B I3);
   else `threshold = 3`.

`minimum_count` is left unset when `--percentage_cutoff` is not given (irrelevant then).
`--threshold` supplied **alongside** `--percentage_cutoff` is accepted and **ignored** (the
per-char threshold check is guarded out in percentage mode) — faithful; unit-tested (Reviewer I4).

---

## 4. Input handling (per file, matches main loop lines 34–75)

Reset SE/PE state per file, then:

### 4.1 BAM filename gate (line 37)
`unless ($file =~ /bam$/) { die "Please provide a BAM file to continue!\n" }`. Replicate the
**`bam$` suffix** check (no dot anchor — `foobam` passes, faithfully). Non-BAM → that exact
error, exit 1.

### 4.2 Truncation check (`bam_isTruncated`, lines 632–653) — **dotted gate**
**Gated on `\.bam$`** (line 42, dotted — NOT the top `bam$` gate). Perl runs `samtools view
2>&1` and dies if an early line starts with `[` (the regex `/[EOF|truncated]/` is a buggy
**character class** that fires on almost any bracketed samtools error — empirically confirmed).
**Rust:** for a `\.bam$`-named file, detect truncation natively via noodles (BGZF/EOF errors)
and emit a comparable scary message (exact bytes not gated). Emit the `Checking file >>$file<<
for signs of file truncation...` notice on STDERR first. A `bam`-but-not-`.bam` file **skips**
this check.

### 4.3 Emptiness check (`bam_isEmpty`, lines 608–630) — **dotted gate; N/A IS reachable**
**Gated on `\.bam$` (and `\.sam$`)** (line 47, dotted). Perl dies "File appears to be empty…"
if `samtools view` (no header) yields no alignment record. **Rust:** for a `\.bam$`-named
file, peek the first alignment record; if none, die with the same message, exit 1, **leaving
no output files** (Perl dies before `process_file` opens them — Reviewer B A-4).

**CRITICAL (rev 1 / Reviewer B C1):** a header-only BAM named `*bam` **without** a literal
dot (e.g. `emptyfoobam`) passes the top `bam$` gate but **skips** the emptiness check → reaches
`process_file` → `count == 0` → emits a real **`N/A` report**. So the `count==0 → "N/A"`
branch is **REACHABLE, not dead code** — it must be implemented and tested with such a fixture.
(The rev-0 SPEC wrongly called it dead.) Order: truncation check runs **before** emptiness
(Reviewer B I1). Emptiness peeks the first **alignment** record (post-header) — `record_bufs`
is already post-header (Reviewer B I4).

### 4.4 SE/PE determination (lines 52–67)
- `-s`/`-p` explicit wins.
- else `determine_file_type` (lines 360–402) = **`bismark_io::detect_paired_from_header`**:
  find the `@PG ID:Bismark` line, PE iff it has both `-1`/`--1` AND `-2`/`--2` (verified faithful).
- Neither flag nor a Bismark `@PG` → `die "Please specify either -s … or -p …, or provide a
  SAM/BAM file that contains the \@PG header line\n\n"` (exit 1; error-path integration test).

### 4.5 PE positional-sort check (`test_positional_sorting`, lines 404–466)
The Perl's `@SO` header check (`/^\@SO/`, line 430) is **dead code** (real SAM uses
`@HD…SO:`, never a `@SO` line — empirically: `@HD SO:coordinate` did NOT trigger it). Its only
effective check is **read-ID pairing**: adjacent records' qnames must match after stripping
legacy `/1`,`/2` suffixes (lines 447–459). **Rust:** fold an adjacent-qname-equality check into
the PE streaming loop (mismatch → die). See §10.5 for the documented partial-output divergence
(Perl's pre-pass dies before any output; the fold-in may leave partial output on malformed PE
input). Additionally reject `@HD SO:coordinate` from the header **before** opening writers
(cheap, no-output, catches the common position-sorted case faithfully).

---

## 5. Output files (naming — matches lines 85–98)

Strip **only** a trailing `.bam` (anchored `\.bam$`), with **NO directory strip** — outputs
land next to the input, full path preserved. Distinct from dedup's basename strip (verified).

| Output | Perl derivation | Result for input `/p/foo.bam` |
|--------|-----------------|-------------------------------|
| Kept BAM | `s/\.bam$//; s/$/.nonCG_filtered.bam/` | `/p/foo.nonCG_filtered.bam` |
| Removed BAM | `s/\.bam$//; s/$/.nonCG_removed_seqs.bam/` | `/p/foo.nonCG_removed_seqs.bam` |
| Report | `s/\.bam$//; s/$/.non-conversion_filtering.txt/` | `/p/foo.non-conversion_filtering.txt` |

If the input doesn't end `.bam` (e.g. `foobam`), `s/\.bam$//` strips nothing →
`foobam.nonCG_filtered.bam` etc. Replicate exactly.

**Header:** both output BAMs get the **input header written verbatim** (noodles default —
original `@PG` chain preserved, nothing appended). Perl re-prints `@`-lines to **both** OUT and
REMOVED (lines 121–125); noodles writes the header once per writer — **equivalent under the
body-only gate D1** (Reviewer A O2).

---

## 6. Core algorithm (`filter.rs`)

Pure, heavily-unit-tested decision over the XM byte string. Per read:

```
nonCpG_count = 0; total_nonCG = 0; fails = false
for each byte c in XM:
    if c == 'H' or c == 'X':            # methylated non-CG (CHH, CHG)
        nonCpG_count += 1; total_nonCG += 1
    elif c == 'h' or c == 'x':          # unmethylated non-CG
        total_nonCG += 1
    if consecutive and (c == 'z' or c == 'h' or c == 'x'):
        nonCpG_count = 0                # reset (only z/h/x; NOT Z, u, U, .)
    if not percentage_mode:
        if nonCpG_count >= threshold: fails = true; break   # early exit
# percentage mode decides AFTER the loop:
if percentage_mode and total_nonCG >= minimum_count:
    perc = round_1dp(nonCpG_count / total_nonCG * 100)      # sprintf("%.1f")
    if perc >= percentage_cutoff: fails = true
```

**Character semantics (load-bearing, both reviewers confirmed):** only `H`/`X` increment
`nonCpG_count`; `H`/`X`/`h`/`x` increment `total_nonCG`; CpG (`Z`/`z`), unknown (`U`/`u`), and
no-call (`.`) are **ignored** for counting. Consecutive reset chars are exactly `z`/`h`/`x` —
`Z` (meth CpG), `u`/`U`, `.` are **transparent**. Increment → (maybe reset) → threshold-check
ordering is exact with an early `break` (lines 138–160).

**Percentage rounding is decision-affecting:** the comparison is on the **`%.1f`-rounded**
value, so e.g. 19.96 → "20.0" ≥ cutoff 20 fails. Use round-half-to-even (`format!("{:.1}")`,
matches C printf — verified agreeing on the 12.25 halfway case). A genuine half-to-even **tie at
the cutoff boundary** (e.g. 5/40 = 12.5%) is in the test matrix (Reviewer V3) and re-verified on
the real-data gate.

### 6.1 SE path (lines 126–185)
- XM extracted from the record's tag (`Value::String`). **Absent XM → empty string → never
  fails → kept** (faithful: Perl `split //, undef` = empty, no error — Perl also emits a
  `Use of uninitialized value` STDERR warning, not reproduced, not gated; Reviewer A O4).
  `count += 1` per read.
- `fails` → write to REMOVED, `kicked += 1`; else write to OUT.

### 6.2 PE path (lines 186–306)
- Read records two-at-a-time (R1 then R2). **Both mates must have a non-empty XM** or `die
  "Failed to extract methylation calls from Read 1 or Read 2…"`. Perl's guard is
  `unless($meth_call_1 and $meth_call_2)` = **truthiness**, so absent **OR empty** XM (Perl
  falsy) triggers the die (Reviewer B A5). `count += 1` per **pair**.
- Apply §6 to R1; if R1 fails, the pair fails (R2 **not examined**). Else reset counters and
  apply to R2; R2 failing → pair fails. **Either mate failing → both mates → REMOVED**,
  `kicked += 1`; else both → OUT.
- Adjacent-qname check per §4.5.
- **CRITICAL — lone trailing R1 (rev 1 / both reviewers C2):** when in PE mode the second
  `record_bufs().next()` returns `None` at a pair boundary (odd record count), Perl dies at
  line 194 with the same message (Read 2 rendered empty), having **already written all prior
  complete pairs** and **before** the SUMMARY → the report file is **0 bytes**, exit nonzero.
  Rust must replicate: die on `None`-second-mate, prior pairs already flushed, **no report
  written**. We do **not** byte-match the die message (error path; Reviewer B A-3) — a
  comparable message suffices. The lone R1's pair is **not** counted (die precedes `++count`).

### 6.3 Unmapped / header / secondary reads
Perl streams `samtools view -h` (every line incl. unmapped). **Rust reads raw `RecordBuf` via
`noodles_bam::io::Reader::record_bufs` — which yields all records incl. unmapped** (unlike
`bismark_io`'s reader, which drops FLAG&0x4 — verified `read.rs:580–594`). Routing:
- **Unmapped in SE:** no XM → kept → OUT, written **verbatim**, same relative order (must be an
  explicit golden assertion, not just "include an unmapped read" — Reviewer A I5 / B V2).
- **Unmapped in PE:** an unmapped mate has no XM → falls into the missing-XM **die** path (§6.2).
  In scope; tested (Reviewer A I2 / B V2).
- **Secondary/supplementary (FLAG 0x100/0x800):** not emitted by Bismark in normal operation.
  `record_bufs` yields them; SE routes them individually; in PE they would break the
  two-at-a-time pairing exactly as the Perl's would (likely a qname-mismatch die). Documented
  assumption (§11 A6), not a supported input.

---

## 7. Report format (`report.rs`) — byte-exact (gated)

Written to `{stem}.non-conversion_filtering.txt`. `$percent` = `"N/A"` if `count == 0` else
`sprintf("%.1f", kicked/count*100)`. `$insert` = `"consecutive "` if `--consecutive` else `""`.

**Line A — count** (SE/PE space difference, lines 314 & 318 — both reviewers reproduced):
- PE: `Analysed read pairs (paired-end) in file >> {infile} <<  in total:\t{count}\n` — **two** spaces before `in total`.
- SE: `Analysed sequences (single-end) in file >> {infile} << in total:\t{count}\n` — **one** space.

**Line B — removed** (ends with `\n\n`; four variants, lines 336/341/347/351):
- PE + %: `Sequences removed because of apparent non-bisulfite conversion (at least {pct}% methylation and {min} non-CG calls in total in at least one of the reads):\t{kicked} ({percent}%)\n\n`
- PE + threshold: `…(at least {threshold} {insert}non-CG calls in one of the reads):\t{kicked} ({percent}%)\n\n`
- SE + %: `…(at least {pct}% methylation and {min} non-CG calls in total per read):\t{kicked} ({percent}%)\n\n`
- SE + threshold: `…(at least {threshold} {insert}non-CG calls per read):\t{kicked} ({percent}%)\n\n`

**Line C — timing** (after the whole `@ARGV` loop; only the **last** file's report; line 664,
single `\n` — the STDERR `warn` at line 663 has `\n\n` but the report gets one):
- `filter_non_conversion completed in {d}d {h}h {m}m {s}s\n`

`{infile}` is echoed verbatim as supplied on the CLI — the gate must invoke the Rust binary with
the same path string the Perl baseline used (gate invariant). STDERR `warn` messages (lines
102–117, 311–353, 663, 77) are emitted comparably but are **not** gated.

---

## 8. Byte-identity contract & gate methodology

**Contract:** for a given input + flags, the **decompressed alignment-record body** of both
output BAMs (kept + removed) is byte-identical to Perl v0.25.1 — same records, same order, same
per-read tags — AND the report text is byte-identical (modulo the normalized timing line).

**Methodology (mirrors dedup/bam2nuc; pin `LC_ALL=C`; never diff raw BGZF; use temp files, not
`<(...)` process substitution — blocked by sandbox):**
1. Run Perl `filter_non_conversion v0.25.1` and `filter_non_conversion_rs` on the same input + flags.
2. `samtools view <perl>.nonCG_filtered.bam` vs `samtools view <rust>.nonCG_filtered.bam` → `cmp`.
3. Same for `.nonCG_removed_seqs.bam`.
4. Report: `cmp` up to the timing line; verify the timing line matches the format.
5. **Bodies only** (`samtools view`, not `-H`) per D1.

### 8.1 Hermetic CI fixtures (local — Perl runs on macOS, proven via `perl -c`)
Tiny synthetic Bismark BAMs; expected outputs generated by real Perl + samtools 1.21. Cover:
- **SE × {threshold default, threshold N≠3, consecutive, percentage}** — every char `. H X Z h x z u U`.
- **Boundary counts:** exactly N-1 / N / N+1 methylated non-CG (keep/keep/remove).
- **Consecutive reset** across `Z`/`.`/`u` (transparent) and `z`/`h`/`x` (reset).
- **Percentage:** min-count gating (total < min → kept even at 100%); a genuine **half-to-even
  tie** at the cutoff (e.g. 5/40 = 12.5%); the rounding-tips-over case (19.96→20.0).
- **PE happy path:** a **properly-paired (even-count)** fixture — NOT `tiny_pe_bismark.bam`.
- **PE code paths:** R1-fails-so-R2-not-examined; R1-passes-R2-fails-pair-removed.
- **PE odd-record die (C2):** lone trailing R1 → exit nonzero, prior pairs in both BAMs,
  **0-byte report**. (`tiny_pe_bismark.bam` is exactly this case — use it here, or a trimmed twin.)
- **N/A branch (C1):** header-only BAM named `*bam` (no dot) → N/A report, exit 0.
- **Empty `.bam`:** dies, **no output files** created.
- **Unmapped:** SE (kept→OUT, verbatim, same order) vs PE (dies, no XM).
- **`--percentage_cutoff` + `--threshold` co-supplied:** identical to `--percentage_cutoff` alone.
- **`@PG`-absent + no `-s`/`-p`:** exit 1 + the "specify either -s … or -p" message.
- **Multi-file (2 files):** file 1 report has **no** timing line; file 2 report **has** it.
- **Report-line rounding:** a `kicked/count` that rounds non-trivially (e.g. 1/3 → 33.3%).

### 8.2 Real-data gate (`#[ignore]`, env-gated)
On colossal/oxy ([[reference_colossal_access]]), 10M SE + 10M PE Bismark BAMs. Cells:
**default (threshold 3)**, **`--threshold 5`**, **`--consecutive`**, **`--percentage_cutoff`** —
each for **SE and PE** (≈6–8 cells). Compare kept/removed bodies + report. Input path string
**identical** to the Perl baseline.

---

## 9. Architecture & module layout

Read raw noodles `RecordBuf` (tag-agnostic, all reads) + write via `noodles_bam::io::Writer` —
**NOT** `bismark_io::BismarkRecord` (drops unmapped, requires XR/XG). `bismark-io` still used for
`detect_paired_from_header`. Mirrors the bam2nuc C-1 decision.

| Module | Responsibility |
|--------|----------------|
| `cli.rs` | clap `Cli`, `validate()` → `ResolvedConfig`, mutual-exclusion + range checks (§3), **signed i64** option types. |
| `error.rs` | `thiserror` `BismarkFilterError` with Perl-echoing messages. |
| `filename.rs` | Output/report name derivation — `.bam`-strip only, no dir strip (§5). |
| `filter.rs` | Pure XM-walk decision (`read_fails(xm, &FilterMode) -> bool`) + per-read tally (§6). |
| `report.rs` | Byte-exact report formatting incl. SE/PE space quirk + timing + N/A (§7). |
| `pipeline.rs` | noodles read/write orchestration; SE + PE streaming; empty/truncation/sort checks; PE lone-R1 die. |
| `lib.rs` | `run()` + re-exports. |
| `main.rs` | Thin entry, `ExitCode` (0 ok / 1 error / 2 clap). |

**Deps (exact-pinned):** `bismark-io = "=1.0.0-beta.8"` (auto-detect helper), `noodles-bam`,
`noodles-sam`, `noodles-bgzf` (pins matching bismark-io), `clap`, `thiserror`,
`mimalloc` (global allocator). Dev: `assert_cmd`, `tempfile`, `noodles-core`, `bstr`.
(rev 1 fold-in: `anyhow` dropped — `thiserror` covers all error handling; `predicates`
dropped — unused.)

---

## 10. Deviations from Perl (documented)

1. **`--help` exit code.** Perl's `print_helpfile` ends `exit 1`. The Rust port uses clap-style
   help, **exit 0**, with mirrored text (sibling-suite consistency; not byte-gated; a
   `$?`-checking harness could observe the difference — flagged). **Confirmed (Felix 2026-06-01): keep exit 0 as an intentional, documented deviation (sibling-suite consistency).**
2. **`--version` text.** Sibling crates use a TG-style provenance string; the Rust port emits one
   (exit 0; not byte-gated).
3. **`--samtools_path` not required/validated.** Perl dies if samtools is absent; the Rust port
   is pure-Rust (noodles), so it accepts and ignores the flag and never requires samtools.
4. **No `--parallel`, single-threaded + mimalloc.** Perl has no parallelism. v1.0 ships
   single-threaded with the workspace mimalloc allocator. BGZF `--parallel` deferred to v1.x
   (re-evaluate after the real-data gate, as dedup did).
5. **PE sort detection (corrected, rev 1 / both reviewers).** Perl's `test_positional_sorting`
   is a **pre-pass** that dies **before any output** on a sort/pairing mismatch. The Rust port
   (a) rejects `@HD SO:coordinate` before opening writers (no output, faithful for the common
   case) and (b) folds an adjacent-qname check into the PE loop. On a malformed PE input with no
   `SO:coordinate` but mis-ordered records, the fold-in dies **mid-stream with partial output**,
   whereas Perl writes none. This is an accurate divergence on **malformed** input (error path,
   not byte-gated) — the rev-0 "matches Perl's no-rollback" claim was wrong.
6. **Truncation detection is noodles-native** (§4.2), not a samtools-stderr regex scrape.
7. **Empty-XM-value regex divergence (rev 1 / Reviewer A I1).** Perl extracts XM via
   `XM:Z:(.+?)\s` (regex on SAM text); on a degenerate **empty** XM value it skips and captures
   the next tag as garbage, while Rust's structured tag-read yields `""`. Equivalent for all real
   data (XM length == read length ≥ 1); documented as **accepted** (Rust is arguably more correct).
8. **PE die message not byte-matched** (error path; Reviewer B A-3) — a comparable message is emitted.
9. **Two-`@PG`-line SE/PE auto-detect (code-review B-1, accepted).** If a BAM carries **two**
   `@PG ID:Bismark` lines (a PE-style then an SE-style), Perl `determine_file_type` picks the
   **last** match (→ SE) whereas the shared `bismark_io::detect_paired_from_header` returns on the
   **first** (→ PE, then dies on the qname-adjacency check). Pathological input (a BAM re-aligned
   by Bismark in two modes); the divergence lives in the shared `bismark-io` crate (identical for
   dedup/extractor), so it is **documented as accepted** here rather than patched from this port.
   A cross-crate fix in `detect_paired_from_header` (scan for the *last* Bismark `@PG`) is a
   possible future follow-up.
10. **Mid-stream truncation → generic I/O error (code-review L2, accepted).** A `.bam` that
    truncates partway through streaming surfaces as a plain `Io` error with partial output already
    written, rather than Perl's up-front `bam_isTruncated` die-before-output. Only the initial
    header/first-record read maps to the `Truncated` variant (§4.2). Error path on a corrupt file,
    not byte-gated.

---

## 11. Assumptions (validated in review)

- **A3.** Multi-file: each file independent (no `--multiple`); only the last report carries the
  timing line — **empirically confirmed** (REPORT bareword reused; only last stays open at exit).
- **A4.** PE reads are query-name-grouped (R1 adjacent to R2) — the Perl's input contract.
- **A5.** XM is `Z` (string); absent **or empty** in PE → die (truthiness); absent in SE → kept.
- **A6.** No secondary/supplementary alignments in normal Bismark input; if present, behaviour =
  whatever the two-at-a-time pairing produces (matching Perl). Documented, not supported.
- **A7.** `--help`/`--version` exit codes + text per §10.1–2 (clap-style) — **Confirmed (Felix 2026-06-01): clap-style exit 0.**
- **A8.** Crate version `1.0.0-alpha.1` (matches bam2nuc's start).

---

## 12. Test plan

- **Unit (`filter.rs`):** every char class; boundary counts (N-1/N/N+1); consecutive reset across
  `Z`/`.`/`u` (transparent) and `z`/`h`/`x` (reset) + early-break; percentage rounding at the
  cutoff incl. a half-to-even tie; min-count gating; empty/absent XM; `--threshold` ignored under
  `--percentage_cutoff`.
- **Unit (`filename.rs`):** `.bam`-strip-only, no dir strip, `foobam` no-strip, path preserved.
- **Unit (`report.rs`):** all 4 Line-B variants; SE one-space vs PE two-space Line-A; `N/A`
  branch; `consecutive ` insert; timing-line format + last-file-only placement; report-line
  `kicked/count` rounding.
- **Unit (`cli.rs`):** mutual exclusions; range checks incl. **negative** values (signed-type
  path); defaults; `--samtools_path` ignored; `--threshold` co-supplied with `--percentage_cutoff`.
- **Integration (hermetic, Perl-generated goldens):** the full §8.1 matrix, incl. the N/A
  non-dotted fixture, odd-PE-die, unmapped-SE-kept vs unmapped-PE-die, empty-`.bam`-no-output,
  `@PG`-absent error, 2-file timing placement.
- **Real-data gate (`#[ignore]`, env-gated):** §8.2 cells on colossal/oxy.

---

## 13. Review fold-in (rev 1)

Both plan-reviewers (PLAN_REVIEW_A.md / _B.md) independently re-derived every byte-identity claim
against live Perl 5.34 + samtools 1.21 and **confirmed the rev-0 core contract correct** (report
strings, XM semantics, consecutive reset, percentage rounding, SE/PE, filename, CLI order, body-
only gate, architecture). Folded-in corrections: **C1** (emptiness/truncation gated on dotted
`\.bam$`; `N/A` branch reachable — was wrongly called dead); **C2** (PE lone-trailing-R1 die +
partial output + 0-byte report; `tiny_pe_bismark.bam` is this case, not a clean PE golden);
signed `i64` CLI types (B I2); §10.5 sort-check divergence corrected; PE truthiness absent-or-empty
(B A5); empty-XM-value divergence documented (A I1); PE-unmapped-mate dies (A I2/B V2);
secondary/supplementary assumption (B); expanded fixture matrix + real-data cells. The one open
user decision — A1/A7 (`--help` exit code) — is **RESOLVED (Felix 2026-06-01): keep clap-style
exit 0 as a documented deviation.**

---

## 14. References

- Spike: `spikes/SPIKE_noodles_roundtrip.md` (round-trip fidelity PASS).
- Reviews: `PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md`.
- Siblings: `rust/bismark-dedup` (closest twin), `rust/bismark-bam2nuc` (raw `RecordBuf` C-1),
  `rust/bismark-io` (`detect_paired_from_header`, reader/writer/tags).
- Memory: [[project_bismark_bam2nuc_port]], [[project_bismark_bedgraph_port]],
  [[reference_colossal_access]], [[feedback_extractor_parallel_cpu_messaging]],
  [[feedback_local_workspace_clippy_untracked]].

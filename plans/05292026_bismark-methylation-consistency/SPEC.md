# SPEC — `bismark-methylation-consistency` (Rust port of Perl `methylation_consistency`)

**Status:** REVISED rev 1 (2026-05-29) — manual review ✅, dual plan-review ✅ (`PLAN_REVIEW_A.md`/`_B.md`), spikes resolved ✅ (`spikes/RESULTS.md`). Awaiting implementation trigger (do not implement)
**Date:** 2026-05-29
**Branch / worktree:** `rust/methylation-consistency` @ `~/Github/Bismark-methcons` (off `rust/iron-chancellor`)
**Perl source of truth:** `methylation_consistency` (repo root, 556 lines, `$VERSION = "0.25.1"`, last modified 28 03 2022)
**Acceptance gate:** **byte-identical output vs the Perl original** (Phase-H byte-identity contract), where "output" is defined precisely in §7.

---

## 1. Purpose & one-paragraph summary

`methylation_consistency` reads a Bismark alignment BAM and splits its reads into **three** output BAMs by the **read-level** consistency of their CpG (or, experimentally, CHH) methylation calls: reads that are *consistently methylated* (`>= upper_threshold%`), *consistently unmethylated* (`<= lower_threshold%`), and *mixed* (in between). Reads with too few cytosine calls (`< min-count`) are discarded. It also writes a small text report summarising the four bucket counts and their percentages. The Rust port must reproduce the three split BAMs (at the record level — see §7) and the report text **byte-for-byte**.

This is the **simplest** of the post-alignment ports: the algorithm is "count `Z`/`z` (or `H`/`h`) bytes in the `XM:Z:` tag, classify by a rounded percentage, route the record(s) to one of three BAMs." All complexity is in faithfully matching Perl's formatting and edge-case behavior, not in the algorithm.

---

## 2. Perl behavior — the contract (derived from source)

### 2.1 Inputs
- **One or more BAM files** as positional args (`@files = @ARGV`). Each file is processed **independently** (its own outputs + report). No input files → `die` with usage (`split_bismark_by_consistency [--min-count=5] [bam file]`).
- Per record, the **only** datum read is the `XM:Z:(\S+)` methylation-call string (regex capture of non-whitespace). The read **name** (field 0) is used for PE mate-ID matching. Everything else in the record is passed through to output unchanged.
- The script is written for `.bam` input: the empty/truncation/auto-detect helpers all branch on `$file =~ /\.bam$/` and shell out to `samtools view`. Non-`.bam` input is not a supported path in practice (auto-detect would read from an unopened filehandle).

### 2.2 CLI options (`process_commandline`, lines 14–127)

| Perl option | Type / default | Behavior |
|---|---|---|
| `--min-count` (`-m`*) | int, default **5** | Min number of cytosine calls (Z+z or H+h) for a read to be considered. Validated `^\d+$` (so **0 is allowed**). Fewer → discarded. |
| `--chh` | flag, default OFF | **Experimental.** Count `H`/`h` (CHH) instead of `Z`/`z` (CpG). Prints a warning + `sleep(3)` at startup. |
| `-s` / `--single_end` | flag | Force SE. `die` if combined with `-p`. |
| `-p` / `--paired_end` | flag | Force PE (R1+R2 counts simply added). |
| `--lower_threshold` | int, default **10** | Upper bound (inclusive) of "unmethylated". Validated **0–49** else `die`. |
| `--upper_threshold` | int, default **90** | Lower bound (inclusive) of "methylated". Validated **51–100** else `die`. |
| `--samtools_path` | string | Path to samtools. Validated for existence; used for all I/O in Perl. |
| `--version` | flag | Prints the version banner (contains `v0.25.1`) and exits. |
| `--help` | flag | Prints the `__DATA__` help block and exits. |

\* `-m`: **not** explicitly declared in `GetOptions`, but Perl `Getopt::Long` `auto_abbrev` makes `-m` an unambiguous abbreviation of `min-count` (only option starting with `m`). The same mechanism allows arbitrary unambiguous prefixes (`--low`, `--up`, etc.). The Rust port will **not** replicate full `auto_abbrev`; see §6.

Startup always emits (STDERR) `Upper and lower methylation thresholds given as:\nUpper: <u>\nLower: <l>\n\n`.

### 2.3 SE/PE determination (`determine_mapping_type`, lines 354–393)
1. If `--single_end` → SE. If `--paired_end` → PE. (Both → `die`.)
2. Else **auto-detect** from the SAM header: walk `@PG` lines; for the line whose ID matches `ID:Bismark`, treat as **PE** iff the command line contains both `-1`/`--1` **and** `-2`/`--2` (`/\s+--?1\s+/` and `/\s+--?2\s+/`); otherwise SE.
3. **If no Bismark `@PG` line is found at all → `$single`/`$paired` remain undefined → the script falls through to SE** (the report prints `single-end`). **This is critical: auto-detect failure is NOT an error — it silently defaults to SE.**

### 2.4 Pre-flight file checks (BAM only)
- `bam_isEmpty` (lines 395–420): reads the first alignment line via `samtools view`; if there are **zero** alignment lines, the file is **skipped entirely** (no output files written, `return` before opening outputs).
- `bam_isTruncated` (lines 422–443): reads up to 10 lines of `samtools view 2>&1`; if any line starts with `[` and matches `EOF`/`truncated`, `die` with a scary message.
- `test_positional_sorting` (PE only, lines 445–509): intends to reject coordinate-sorted input and verify R1/R2 adjacency. **Two review caveats (2026-05-29):** (1) its `die`-if-`/^\@SO/` check (line 471) is **dead code** — SAM declares sort order in `@HD …\tSO:`, so no header line ever starts with `@SO`; the *real* protection is the per-pair name-equality `die` in the main loop. (2) It also scans up to ~100 000 reads checking adjacent R1/R2 names match (with `/1`,`/2` suffix stripping). **SE input is NOT sort-checked.** The Rust port implements the *correct* guard (reject `@HD SO:coordinate` for PE) — see §4.6.

### 2.5 Core loop (`process_file`, lines 203–302)
For each record (PE: a record **pair**):
1. R1: match `XM:Z:(\S+)`. **If absent → `warn` + `last` (stop processing this file entirely)**, then fall through to the summary with whatever was accumulated. Capture R1 name (`$id1`).
2. Count into `$meth_count`/`$unmeth_count`:
   - CpG (default): `meth += tr/Z//`, `unmeth += tr/z//`.
   - CHH (`--chh`): `meth += tr/H//`, `unmeth += tr/h//`.
3. **PE only:** read the next line as R2. If R2 lacks `XM:Z:` → `warn` + `last` (R1's counts are discarded). If R2 name ≠ R1 name → `die` ("READ IDs … did not match"). Add R2's `Z`/`z` (or `H`/`h`) counts to the same `$meth_count`/`$unmeth_count`.
4. **Discard gate:** `if (meth + unmeth) < min_count` → `++$discarded_count; next`.
5. **Zero gate:** `if (meth + unmeth) == 0` (only reachable when `min_count == 0`) → `next` (skipped, counted in **no** bucket).
6. **Percent (rounded first!):** `$percent_methylated = sprintf("%.1f", meth/(meth+unmeth)*100)`. This `%.1f` **string** is then compared numerically.
7. **Classify:**
   - `$percent_methylated <= $lower_threshold` → **all_unmeth** bucket.
   - `elsif $percent_methylated >= $upper_threshold` → **all_meth** bucket.
   - `else` → **mixed** bucket.
8. **Write:** to the chosen output handle (`UNMETH`/`METH`/`MIXED`), the SAM **header** is printed **lazily on the first record of that bucket** (`samtools view -H $file`), then R1 (and, for PE, R2) is written.

> **Round-then-compare is load-bearing.** Because step 6 rounds to one decimal *before* step 7 compares, the **rounded** value — not the raw fraction — decides the bucket. Illustratively, a read near 10.04% rounds to `"10.0"` → **unmethylated**, while near 10.05% it rounds to `"10.1"` → **mixed**. The Rust port must compute `format!("{:.1}", meth as f64 / total as f64 * 100.0).parse::<f64>()` then compare — never compare the raw fraction.
>
> **Validated (Spike 1 / Reviewer B, 2026-05-29):** Rust `{:.1}`/`{:.2}` is *decision-identical* to Perl `sprintf` because both round-half-to-even on the same `f64` — **including exact representable ties at power-of-two totals** (`1/16 → 6.25`, `1801/2000 → 90.05 → "90.0" → all_meth`), *provided* the f64 is computed in the exact op-order above. (The SPEC's earlier "ties unreachable" claim was wrong; the conclusion holds for the right reason.) See `spikes/RESULTS.md`.

### 2.6 Summary / report (lines 306–344)
`$total = all_meth + all_unmeth + mixed + discarded`. For PE this counts **pairs** (one increment per pair), even though 2 BAM records are written per pair. Percentages: `sprintf("%.2f", bucket/total*100)`; if `total == 0`, each percentage is the literal string `N/A`.

Both STDERR (`warn`) and `${file_root}${chh}_consistency_report.txt` receive the same body. The report's exact bytes (see §5) are the **hard** acceptance target; the STDERR copy is **not** (see §7).

### 2.7 Output filenames (lines 185–199)
`$file_root = $file; $file_root =~ s/\.bam$//;` (strip a single trailing `.bam` only). `$chh_status = $chh ? '_CHH' : ''`. Outputs (same directory as input):
- `${file_root}${chh_status}_all_meth.bam`
- `${file_root}${chh_status}_all_unmeth.bam`
- `${file_root}${chh_status}_mixed_meth.bam`
- `${file_root}${chh_status}_consistency_report.txt`

---

## 3. Reuse map — what comes from the existing workspace

`bismark-methylation-consistency` is the **closest sibling of `bismark-dedup`** (read a Bismark BAM → classify records → write BAM(s) + a text report) and reuses `bismark-io` for all I/O. Confirmed APIs (verified against source this session):

| Need | Reuse | Notes |
|---|---|---|
| Open BAM, header, record stream | `bismark_io::BamReader::without_sort_check(BufReader<File>)` (BAM-only for v1.0; §2.1) then `.records()` | Open **no-sort-check** so SE isn't rejected (Perl never sort-checks SE); apply `@HD SO:coordinate` rejection manually for PE (§4.6/§4.11). `.records()` yields `BismarkRecord`; **silently drops unmapped** (FLAG&0x4) — see §4. |
| Threaded BGZF read/write (perf, optional) | `ThreadedBamReader` / `ThreadedBamWriter` | Deferred to a perf follow-up; v1.0 single-threaded. |
| Write 3 BAMs with passed-through header | `bismark_io::BamWriter::from_path(path, header)` ×3 → `write_record(&BismarkRecord)` → `finish()` | **Eager-open all three** at start → empty buckets become valid empty BAMs (§5.2). Header written eagerly; `finish()` writes BGZF EOF (mandatory). Ensure `finish()` on **all** paths (incl. errors) or the BAM is EOF-less/undecodable. No `@PG` injection. |
| XM tag bytes | `BismarkRecord::xm()` (or `bismark_io::tags::xm`) | Returns `&[u8]`; length == seq length guaranteed by `BismarkRecord` construction. |
| Read name (PE mate match) | `record.inner().name()` | `Option<&BString>`; compare R1 vs R2. |
| SE/PE auto-detect from header | `bismark_io::detect_paired_from_header(&header) -> Option<bool>` | **`None` ⇒ SE** (see §4) — *not* an error, unlike dedup. |
| CLI / exit codes / `--version` / `version_string()` | mirror `bismark-dedup` `cli.rs` + `main.rs` + `lib.rs::version_string()` | `env!("CARGO_PKG_VERSION")`, not the Bismark `0.25.1` constant (see §6.4). |
| Output-filename derivation | mirror `bismark-dedup/src/filename.rs` style | But methcons appends fixed suffixes (`_all_meth.bam` …) — see §5. |
| Report text formatting | mirror `bismark-dedup/src/report.rs` style | `format!("{:.2}", …)`, `N/A` when count==0. |
| STDERR diagnostics + `--quiet` | mirror `bismark-extractor/src/logging.rs` `Logger` | All diagnostics to STDERR; `--quiet` gates them; stdout stays clean. |
| Fixtures | `bismark-io/test_files/tiny_pe_bismark.bam`; synthetic BAMs via `BamWriter` in `tests/` | dedup-style. |

**Crate name:** `bismark-methylation-consistency`. **Binary name:** `methylation_consistency_rs` (mirrors dedup's `deduplicate_bismark_rs` = Perl-name + `_rs`; reads as a drop-in for `methylation_consistency`). *(Confirm in review — alt: `methylation-consistency-rs` per extractor's hyphen style.)*

---

## 4. Known divergences from Perl (documented & accepted)

All of these are **no-ops on genuine Bismark BAMs**; they only surface on malformed/pathological input. Listed so reviewers can accept or challenge them.

1. **Stricter record validation (RESOLVED: keep `BismarkRecord`).** `BismarkRecord::from_noodles_record` requires valid `XR:Z:`/`XG:Z:` strand tags **and** `XM.len() == seq.len()`. Perl reads **only** `XM`. The Rust port's behavior on a malformed record (made precise per code review 2026-05-29): **missing `XM`** → graceful stop (below); **missing/invalid `XR`/`XG`, an invalid strand combo, or `XM.len() != seq.len()`** → **FATAL** — the reader's `Err` aborts the *whole file* (nonzero exit, no report), which is *stronger* than Perl (Perl would happily process such a record, since it only reads `XM`). Genuine Bismark BAMs always carry all three tags with matching lengths → this never triggers on the acceptance datasets. Decision (both plan-reviewers): keep `BismarkRecord` (max reuse); the strictness is pinned by the `malformed_record_missing_xr_is_fatal` test.
   - **Missing-XM is a graceful STOP in Perl, not a fatal error** (lines 224–227, 250–253): on a record (R1 *or* R2) lacking `XM:Z:`, Perl `warn`s + `last` — it **stops** the file's loop, **finalizes the partial BAMs**, writes a report tallying only the records seen so far, and **exits 0** (for R2-missing-XM, R1's counts for that pair are discarded). The strict reader surfaces this as an `Err` from `.records()`; **the pipeline must catch that error and reproduce the stop-and-finalize**, not abort. (PLAN B3/C2 + a dedicated test.)
2. **Unmapped reads filtered.** `.records()` drops FLAG&0x4 records; Perl does not. Bismark emits only mapped reads (PE: only concordant pairs where both mates map), so this never changes output. For PE it would also be *dangerous* if it triggered (breaks R1/R2 adjacency) — but it cannot on real Bismark data.
3. **`--samtools_path` is accepted but unused** (noodles does all I/O). *Open question:* validate-existence-to-mirror-Perl's-`die`, or accept-and-ignore. **Recommendation: accept-and-ignore with a one-line note; it has no effect on output.**
4. **`--help` / `--version` text** is clap/Rust-generated, **not** byte-identical to Perl's `__DATA__` block / version banner. These are not part of the acceptance gate (§7). dedup set this precedent.
5. **`Getopt::Long` `auto_abbrev`** (arbitrary unambiguous prefixes) is **not** replicated; only the documented flags + `-p`/`-s`/`-m` are accepted.
6. **PE sort guard: implement the *correct* one (decision 2026-05-29); drop the 100k pre-flight.** Perl's `/^\@SO/` coordinate-sort check is **dead code** (no real header line starts with `@SO`; §2.4). The Rust port implements what Perl intended — reject `@HD SO:coordinate` for **PE** input (a small manual header check, since the reader is opened no-sort-check; §4.11 below). This is an **intentional, output-equivalent fix** (coordinate-sorted PE breaks R1/R2 adjacency → Perl dies later at the per-pair name mismatch → no valid output either way). The Perl 100k-read adjacency pre-flight is dropped: the per-pair name-equality `die` in the main loop is output-equivalent (well-formed files never trip either; malformed files trip both → no output).
7. **`--chh` `sleep(3)`** is dropped (UX artifact, not output).
8. **STDERR diagnostic text** mirrors Perl's `warn`s in spirit but is **not** byte-matched and is suppressible with `--quiet` (extractor precedent).
9. **samtools provenance `@PG` lines omitted (Spike 2 bonus).** Perl's `samtools view -H` (header extraction) and `samtools view -b -S -` (BAM write) each auto-append a `@PG ID:samtools*` line, so the Perl output header carries provenance lines the noodles-based Rust port does not. Output BAM headers therefore differ; the §7 gate excludes these lines (compares records + `@HD`/`@SQ` + `@PG ID:Bismark`). This is the intended consequence of removing the samtools subprocess; it is exactly why `bismark-dedup`'s byte-identity test compares only a qname set.
10. **Empty buckets emit valid empty BAMs, not Perl's 0-byte files (decision 2026-05-29).** Perl yields a 0-byte, unreadable file for a bucket with zero records; the Rust port writes a valid (header + BGZF EOF) empty BAM. Diverges for empty buckets only; harness compares them at the record level (both = zero records). See §5.2, `spikes/RESULTS.md`.
11. **Sort-check is applied PE-only, via the reader's no-sort-check path.** The Rust port opens the reader **without** the coordinate-sort rejection (so SE — which Perl never sort-checks — is accepted), then for PE applies a manual `@HD SO:coordinate` rejection (§4.6). This requires `BamReader::without_sort_check` (already public) rather than `open_reader` (which always rejects) — Open Decision #1, option (a); **no `bismark-io` change needed.**

---

## 5. Output contract — exact bytes

### 5.1 `${file_root}${chh}_consistency_report.txt`
Copy these templates **verbatim** from `methylation_consistency:334-343` (do **not** retype by hand). `<type>` = `paired-end` | `single-end`. Separator = **exactly 49 hyphens**. `\t` = literal tab. Trailing `\n` on every line.

```
Total <type> records     -\t<total>\n
-------------------------------------------------\n
All methylated    [ >= <upper>% ] -\t<all_meth> (<perc_meth>%)\n
All unmethylated  [ <= <lower>% ] -\t<all_unmeth> (<perc_unmeth>%)\n
Mixed methylation [ <lower>-<upper>% ] -\t<mixed> (<perc_mixed>%)\n
Too few CpGs   [min-count <min>] -\t<discarded> (<perc_discarded>%)\n
```
- Line 1: `"Total "` + `<type>` + `" records     -"` (5 spaces before `-`) + tab + total.
- Line 3 label: `"All methylated"` + **4 spaces**; line 4: `"All unmethylated"` + **2 spaces**; line 5: `"Mixed methylation"` + **1 space** (`[` column aligned at 18). Line 6: `"Too few CpGs"` + **3 spaces** (CHH: `"Too few CHHs"` + **3 spaces**).
- Percentages: `format!("{:.2}", bucket as f64 / total as f64 * 100.0)`. When `total == 0`: the literal string `N/A` → renders as `(N/A%)`.
- **Byte-validated (Spike 2):** a real Perl run matched these templates exactly — the file starts directly at `Total …` (**no leading `\n`**, unlike dedup's report) and ends with the `Too few …` line + its `\n` (**no trailing blank line**).

### 5.2 The three BAMs
- For each populated bucket: a BAM whose decompressed content is **the input header followed by exactly the bucket's records, in input order** (PE: R1 immediately followed by R2 per pair).
- Acceptance is at the **decompressed record level** (see §7), not raw BGZF bytes — the Perl pipes through `samtools view -b -S -` whose BGZF block layout/compression differs from noodles. (Same situation `bismark-dedup` already handles.)
- **Header is written verbatim** (input header → output, via noodles, matching `bismark-dedup`'s `reader.header().clone()` convention). The Rust port adds **no** `@PG` line.
- **samtools provenance `@PG` divergence (Spike 2 bonus finding, RESOLVED):** the Perl output header gains **extra `@PG ID:samtools*` lines** that samtools auto-appends for each subprocess (`view -H` to extract the header, `view -b -S -` to write the BAM). The noodles-based Rust port emits none. **Output BAM headers therefore cannot be byte-identical.** The contract excludes these provenance lines — see §7. (This is precisely why dedup's byte-identity test compares only a qname set.)
- **Empty-bucket behavior (Spike 2, RESOLVED — decision 2026-05-29):** Perl produces a **0-byte, unreadable** file for an empty bucket (empty stdin → `samtools view -b -S -` writes nothing; `samtools view` then errors "fail to read the header"). The Rust port instead **emits a valid empty BAM** (header + BGZF EOF) for empty buckets — readable and downstream-usable. This **diverges from Perl's 0-byte output for empty buckets only**; all meaningful output (records + report) stays identical. **Implementation: eager-open all three `BamWriter`s** at start with the input header (populated → header+records; empty → valid empty BAM). The §7 harness compares empty buckets at the **record level** (both = zero records), not raw bytes. See `spikes/RESULTS.md`.

---

## 6. CLI surface (clap derive)

Long flag names **must keep Perl's underscores** (`--paired_end`, `--single_end`, `--lower_threshold`, `--upper_threshold`, `--samtools_path`) for drop-in compatibility; `--min-count` keeps its hyphen.

```
methylation_consistency_rs [OPTIONS] <FILES>...

<FILES>...                 one or more Bismark BAM files (≥1 required)
-p, --paired_end           force PE (conflicts with -s)
-s, --single_end           force SE (conflicts with -p)
    --chh                  count CHH (H/h) instead of CpG (Z/z); experimental
    --lower_threshold <N>  0–49, default 10
    --upper_threshold <N>  51–100, default 90
-m, --min-count <N>        ≥0 integer, default 5
    --samtools_path <P>    accepted for compatibility; unused (noodles I/O)
    --quiet                suppress STDERR diagnostics (NEW; not in Perl)
-V, --version              print version (CARGO_PKG_VERSION) and exit
-h, --help                 print help and exit
```

### 6.1 Validation (mirror Perl, same error → nonzero exit)
- `-s` **and** `-p` together → error ("cannot select both …").
- `upper_threshold` provided and not in `51..=100` → error (Perl's message).
- `lower_threshold` provided and not in `0..=49` → error (Perl's message).
- `min-count` is a non-negative integer (clap `u32`/`u64`; `0` allowed). Negative/non-numeric → clap parse error.
- zero input files → error with the Perl usage string.

### 6.2 Defaults applied post-parse: `lower=10`, `upper=90`, `min=5`.
### 6.3 Startup: emit (unless `--quiet`) the `Upper:`/`Lower:` banner; if `--chh`, emit the experimental warning (no `sleep`).
### 6.4 `--version`: `version_string()` from lib.rs using `env!("CARGO_PKG_VERSION")` (dedup precedent). The Bismark `0.25.1` string is **not** reproduced and is **not** injected into any header.

---

## 7. Acceptance / definition of "byte-identical output"

The comparison is **stronger than `bismark-dedup`'s** (which compares only an *unordered qname set* — Reviewer A): methcons is a pure pass-through splitter, so per-record fidelity through the noodles round-trip **and** R1-before-R2 ordering are exactly its risk. We do NOT diff raw BGZF (Perl's samtools BGZF differs from noodles).

**HARD gate (must be byte/record identical to Perl):**
1. `*_consistency_report.txt` — **byte-for-byte** identical (templates byte-validated against a real Perl run — Spike 2; note **no leading `\n`**, unlike dedup's report).
2. Each populated output BAM — identical at the **decompressed record level**: same set **and order** of records per bucket (PE: R1 then R2), compared by reading both Perl and Rust outputs back via `bismark_io::open_reader` and asserting equality of each record's fixed fields (qname, FLAG, RNAME, POS, MAPQ, CIGAR, RNEXT, PNEXT, TLEN, SEQ, QUAL) **in order**, plus the optional **tags compared as a set** (SAM tag order is not semantically significant, and Perl's vs noodles' writers may order tags differently).
3. **Header:** `@HD` + `@SQ` records identical; the `@PG ID:Bismark` line present/identical. **Excluded from the gate:** `@PG ID:samtools*` provenance lines (Perl's samtools subprocesses inject them; the Rust rewrite intentionally omits them — §5.2, §4.9). `@CO` lines compared informationally only.
4. **Empty buckets** compared at the record level: both Perl (0-byte) and Rust (valid empty BAM) yield **zero records** — assert that, not raw bytes (§5.2).
5. Bucket counts in the report match the per-bucket record counts.

**NOT in the gate:** raw BGZF bytes; samtools provenance `@PG`; STDERR diagnostics; `--help`/`--version` text; empty-bucket raw bytes.

**Real-data validation (Phase D, colossal, `#[ignore]`):** run Perl `methylation_consistency` and `methylation_consistency_rs` on the **same input path** for `10M_SE`, `10M_PE`, and a `--chh` run; assert the report verbatim and the three BAMs per the record/header rules above. Data at `/weka/projects/bioinf/Data/Felix/bismark_benchmarks/` (genome not needed — methcons reads only the BAM). A `samtools view`-text diff (records only, header `@PG ID:samtools*` filtered) is an acceptable secondary cross-check where samtools is available.

---

## 8. Gotchas & spikes (call-outs)

1. **Number formatting parity (`%.1f`, `%.2f`) — Spike 1, VALIDATED (Reviewer B, 2026-05-29).** Rust `{:.N}` is *decision-identical* to Perl `sprintf` (both round-half-to-even on the same `f64`), **including** exact representable ties at power-of-two totals (`6.25`, `12.5`, `90.05`) — *provided* the f64 is computed as `meth as f64 / total as f64 * 100.0` (**pin this op-order**). Correction: the earlier "ties unreachable" claim was false; the conclusion holds for the right reason. **Action:** formalize as committed unit tests over the tie grid + threshold boundaries (PLAN A3). See `spikes/RESULTS.md`.
2. **Empty-bucket BAM — Spike 2, DONE (2026-05-29).** Perl produces a **0-byte, unreadable** file (empty stdin → `samtools view -b -S -` writes nothing; `samtools view` then errors). **Decision: Rust emits a valid empty BAM** (eager-open all three writers); diverges from Perl for empty buckets only; harness compares empties at the record level (§5.2, §7). See `spikes/RESULTS.md`.
3. **Header round-trip fidelity.** The header is copied input→output via noodles (parse + re-serialize), not byte-copied. This is the **same** round-trip `bismark-dedup` already validates against Perl, so it's a solved concern — but Phase D must still confirm on real data (esp. multi-`@SQ`, `@PG`, `@CO`, custom tags).
4. **Line endings:** Unix `\n` throughout (Perl writes `\n`).
5. **Chromosome / record ordering:** preserved trivially — methcons streams records in input order and never sorts. No chromosome-ordering logic exists (unlike bedgraph/c2c).
6. **`@PG` provenance asymmetry (Spike 2 bonus, §4.9):** the Rust port writes the header **verbatim** (no `@PG` added — confirmed `BamWriter` doesn't auto-inject; matches dedup). But Perl's samtools subprocesses **do** append `@PG ID:samtools*` provenance lines, so output headers differ; the §7 gate excludes those lines. methcons does **not** add its own `@PG` (drop-in, consistent with dedup).
7. **PE counts pairs, not records:** `total` and bucket counts increment **once per pair**; verify the report shows pair counts while BAMs hold 2× records.
8. **`min_count == 0`:** zero-call reads hit the "zero gate" (`next`, counted in no bucket), not the discard bucket. Easy to get wrong.

---

## 9. Out of scope for v1.0
- Multi-threaded BGZF (`ThreadedBam*`) / `mimalloc` — perf follow-up; v1.0 is single-threaded and correctness-first (matches dedup).
- SAM/CRAM **input** beyond what `open_reader` gives for free (Perl is BAM-only in practice); output is always BAM (Perl's `samtools view -b`).
- Replicating `Getopt::Long` `auto_abbrev`.
- Byte-matching STDERR / `--help` / `--version`.

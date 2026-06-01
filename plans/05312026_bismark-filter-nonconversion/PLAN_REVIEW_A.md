# PLAN_REVIEW_A — `filter_non_conversion` Rust port SPEC (rev 0)

**Reviewer:** A (independent, fresh context).
**Date:** 2026-05-31.
**Target:** `plans/05312026_bismark-filter-nonconversion/SPEC.md`.
**Ground truth:** Perl `filter_non_conversion` v0.25.1 (724 lines), read line-by-line and
exercised empirically with real `perl 5.34.1` + `samtools 1.21` on macOS.

**Verdict:** The SPEC is unusually accurate. Every byte-identity claim I re-derived against the
live Perl **held** (report format strings, SE/PE space quirk, all four removed-line variants,
the `consecutive ` insert, percentage rounding-before-`>=`, multi-file timing placement, the
`.bam`-strip-only filename derivation, the buggy truncation char-class). The verbatim-passthrough
architecture is sound. **However, the SPEC's own primary test fixture cannot be processed by the
Perl PE path** (it has an odd record count and dies), the empty-XM-value regex divergence is
undocumented, and several SE-vs-PE counting/`count` subtleties need explicit fixture coverage.
None of these block the design; they are gaps to close before/inside implementation.

---

## 1. Logic review (re-derived against the live Perl)

### 1.1 Report format strings — VERIFIED byte-for-byte (empirically)

I ran the real Perl tool and dumped reports with `sed -n l` / `cat -A`. Confirmed:

- **SE Line-A** (line 318): `Analysed sequences (single-end) in file >> {infile} << in total:\t{count}\n`
  — **one** space before `in total`. ✔
- **PE Line-A** (line 314): `Analysed read pairs (paired-end) in file >> {infile} <<  in total:\t{count}\n`
  — **two** spaces before `in total`. ✔ (Empirically reproduced: `<<··in·total`.)
- **SE + threshold** (line 350/351): `...(at least 3 non-CG calls per read):\t{kicked} ({percent}%)\n\n`. ✔
- **SE + percentage** (line 346/347): `...(at least 20% methylation and 5 non-CG calls in total per read):\t{kicked} (...%)\n\n`. ✔
- **PE + threshold** (line 340/341): `...(at least 3 non-CG calls in one of the reads):\t{kicked} (...%)\n\n`. ✔
- **PE + percentage** (line 336/337): `...in total in at least one of the reads...`. ✔ (Not directly run but string is unambiguous in source.)
- **`${insert}` = `'consecutive '`** with trailing space, placed between `{threshold} ` and `non-CG`
  → `at least 3 consecutive non-CG calls per read`. ✔ (Empirically reproduced.)
- **`$percent` = `sprintf("%.1f", kicked/count*100)`**, `"N/A"` iff `count==0` (lines 323–328). ✔
- **Trailing `\n\n`** on the removed line. ✔ (the second `\n` produces the blank line before timing.)
- **Timing line** (line 664): `filter_non_conversion completed in {d}d {h}h {m}m {s}s\n` — written to
  REPORT with a **single** `\n` (the STDERR `warn` at line 663 has `\n\n`; the SPEC §7 correctly
  uses one `\n` for the report). **Only the LAST file's report** carries it. ✔ **Empirically
  reproduced**: a 2-file run left f1's report with no timing line and f2's with the timing line.

SPEC §7 matches all of the above. **No discrepancies found in the report contract.**

> Note for implementers: in my runs the timing seconds were non-zero (`0d 0h 0m 2s`) purely
> because `test_positional_sorting` calls `sleep(1)` twice on the PE path. The Rust port omits
> those sleeps, so its raw timings differ — but D2 normalizes the timing line by prefix/format,
> so this is fine. Worth a one-line note in the gate methodology that the Rust port will not
> reproduce the Perl's `sleep`-inflated durations and that this is expected.

### 1.2 XM character semantics — VERIFIED (lines 138–176, 205–293)

- `H`/`X` → `++nonCpG_count; ++total_nonCG`. ✔
- `h`/`x` → `++total_nonCG` only. ✔
- `Z`/`z`/`U`/`u`/`.` → ignored for counting. ✔
- Consecutive reset set is **exactly** `z`/`h`/`x` (line 149/217/265). `Z`, `u`, `U`, `.` do NOT
  reset. ✔ SPEC §6 states this correctly.
- Ordering: **increment → (maybe consecutive-reset) → threshold-check with early `last`** (lines
  140–158). ✔ I re-simulated the loop in Perl and the SPEC §6 pseudocode reproduces it exactly,
  including that the threshold-check is **inside** the per-char loop and `last`s on first
  satisfaction.

### 1.3 Percentage mode — VERIFIED (lines 162–176 / 230–245 / 278–293)

- Decision happens **after** the loop, gated on `total_nonCG >= minimum_count`. ✔
- `perc = sprintf("%.1f", nonCpG_count/total_nonCG*100)`, compared `>= percentage_cutoff`. ✔
- The per-char threshold check is **skipped** in percentage mode (the `unless (defined
  $percentage_cutoff)` guard at line 154). **Empirically confirmed**: a read with `XM:Z:HHH`
  (100% methylated) but `total_nonCG=3 < minimum_count=5` is KEPT — proving the absolute
  threshold never fires in percentage mode and the min-count gate dominates.
- **Rounding-before-comparison confirmed empirically**: `--percentage_cutoff 20` on a read with
  `1 H + 4 h` (= 20.0%) **removes** it (`>=` on the rounded value). SPEC §6's "19.96 → 20.0 ≥ 20
  fails" framing is correct.
- I verified `format!("{:.1}")` (Rust) vs `sprintf("%.1f")` (Perl) agree on the true 1-dp halfway
  case `12.25 → 12.2` (both round-half-to-even). The SPEC's rounding approach is sound. (Caveat:
  these ratios have small denominators, so exact halfway cases are rare; still worth one unit
  test at a constructed halfway ratio, e.g. 49/400 → 12.2.)

### 1.4 SE vs PE — VERIFIED with one CRITICAL fixture caveat (see §1.5)

- **SE**: missing XM → `meth_call` undef → `split //, undef` → 0 chars → never fails → **kept**,
  and `count += 1`. **Empirically confirmed** it does NOT die (it emits a `Use of uninitialized
  value` *warning* to STDERR, which is not gated). ✔ SPEC §6.1 is correct. *(Minor: the SPEC
  could note that the Perl emits this STDERR warning; the Rust port's STDERR will differ, which
  is fine since STDERR is not gated.)*
- **PE**: both mates must have XM or `die` (lines 194–196). ✔ **Empirically confirmed the exact
  die path** (see §1.5).
- **PE**: R1 fails → R2 not examined (`unless ($sequence_fails)` at line 247). ✔
- **PE**: either mate fails → both mates → REMOVED; counters reset between mates (lines 249–250). ✔
- **`count`** is per-read (SE) / per-pair (PE, `++$count` once at line 202). ✔

### 1.5 ★ CRITICAL: the SPEC's own primary fixture DIES on the Perl PE path

`rust/bismark-io/test_files/tiny_pe_bismark.bam` has **203 alignment records — an ODD number**.
The last record (`115_..._R1`, FLAG 83) is **unpaired**. The Perl PE loop reads two-at-a-time
(`$_ = <IN>` at line 189); on the dangling final record, `$_` is undef, `meth_call_2` is undef,
and the tool **dies** at line 194–196 with `Failed to extract methylation calls from Read 1 or
Read 2 for sequence pair`, **exit code 255**, leaving an **empty (0-byte) report** and a
**partial** `.nonCG_filtered.bam` on disk (no rollback).

I reproduced this exactly:
```
Read 2:                       <- undef
Failed to extract methylation calls from Read 1 or Read 2 for sequence pair
(exit 255; test.non-conversion_filtering.txt is 0 bytes)
```

Implications:
1. The spike (`SPIKE_noodles_roundtrip.md`) reports "203 records" as a success — but that spike
   only round-trips records; it never **pairs** them. The fixture is therefore unusable for the
   PE *filter* golden. The SPEC must either (a) trim the fixture to an even count for the PE
   golden, or (b) generate a purpose-built even-record PE fixture, or (c) explicitly use the
   odd-record case as the **PE-odd-record-die** golden.
2. The "odd number of PE records" case is one the SPEC §11 flags as an open edge but does not
   resolve. **Resolve it**: the faithful behavior is *die mid-stream with a partial kept-BAM and
   an empty report*. The Rust port reading `RecordBuf` two-at-a-time must replicate the die when
   the second `next()` returns `None` on a pair boundary. **This is the single highest-value
   missing test in the SPEC.**

### 1.6 Output filename derivation — VERIFIED (lines 85–98)

Strips **only** a trailing `.bam` (anchored `s/\.bam$//`), **no directory strip** — outputs land
next to the input with the full path preserved (empirically: `/tmp/.../test.nonCG_filtered.bam`).
This is **distinct** from dedup's `s/.*\///` basename strip (confirmed by reading
`bismark-dedup/src/filename.rs`). SPEC §5 is correct, including the `foobam` no-strip case. ✔

### 1.7 Input gating — VERIFIED (lines 37, 42–49, 52–72)

- `/bam$/` gate (no dot anchor): `foobam` passes. ✔ SPEC §4.1 correct.
- Truncation (lines 632–653): the inner `/[EOF|truncated]/` is a **character class**, not an
  alternation. **Empirically confirmed it fires on almost any bracketed line** (`[main] random`
  and `[xyz] hello world` both trigger the die). SPEC §4.2's "buggy character class" framing is
  correct; the noodles-native replacement with a non-gated error path is the right call. ✔
- Emptiness (lines 608–630): dies "File appears to be empty…" on header-only input. ✔ SPEC §4.3
  correctly notes this makes the `count==0 → "N/A"` branch dead-but-kept-for-faithfulness. ✔
- SE/PE auto-detect via `@PG` (lines 360–402): the SPEC routes this through
  `bismark_io::detect_paired_from_header`. I read that function and its tests — it serializes the
  header to SAM text and checks `ID:Bismark` + `-1`/`--1` AND `-2`/`--2` token presence with the
  same strict whitespace-boundary semantics as Perl's `/\s+--?1\s+/`. **This is a faithful
  match** (and already battle-tested in dedup/extractor). ✔
- PE positional-sort (lines 404–466): the `@SO` header check at line 430 IS dead code (real SAM
  uses `@HD…SO:`); the live check is adjacent-qname equality after stripping legacy `/1`,`/2`
  (lines 447–459). SPEC §4.5 is correct, and folding the check into the PE loop is a sound,
  stronger-but-faithful-spirit deviation (documented as Deviation 5). ✔

### 1.8 CLI validation — VERIFIED (lines 469–606)

- Defaults: `threshold=3` (line 602), `minimum_count=5` only when `--percentage_cutoff` set
  (line 534). ✔
- Mutual exclusions: percentage vs consecutive (line 521), `-s` vs `-p` (line 545). ✔
- Ranges: percentage `0–100` inclusive (line 523), threshold `>0` (line 597), minimum_count `>0`
  (line 529). ✔
- SPEC §3.1 ordering matches the Perl `process_commandline` flow. ✔

**One ordering subtlety the SPEC under-specifies:** in the Perl, the `@ARGV`-empty check (line
513) fires **before** the percentage/min-count validation (line 520) and the samtools check —
so `filter_non_conversion --percentage_cutoff 200` with **no files** prints the
"Please provide one or more…" message and exits, it does NOT reach the range check. The SPEC §3.1
lists "no files" as step 4 and percentage validation as step 5, which is correct, but the prose
should make explicit that a *bad* option value combined with *no files* yields the no-files exit,
not the option error. Low-risk (error path, not gated), but worth a one-liner to prevent an
implementer from reordering.

---

## 2. Assumptions

### 2.1 Stated assumptions — validated

- **A4** (PE reads are qname-grouped, R1 adjacent to R2): correct and is the Perl's input
  contract. The fixture confirms R1(FLAG 99)/R2(FLAG 147) adjacency with identical qnames. ✔
- **A5** (XM is `Z`-string; absent legal in SE, fatal in PE): confirmed. ✔
- **A3** (multi-file independent, timing only on last report): **empirically confirmed**. ✔

### 2.2 Implicit assumption the SPEC does NOT surface — empty XM VALUE divergence

The SPEC §6.1 says "Absent XM → empty string → never fails → kept." That is correct for a
**fully absent** XM. But the Perl extracts XM via the regex `XM:Z:(.+?)\s` (line 127/191/192),
**not** by reading the tag value. I empirically established:

| SAM text around XM | Perl `(.+?)\s` captures | Rust direct-tag-read would get |
|---|---|---|
| `XM:Z:Hhh\t...` (normal) | `Hhh` | `Hhh` ✔ identical |
| `XM:Z:\tXR:Z:CT` (empty value, tab, next tag) | `\tXR:Z:CT` (**garbage!**) | `""` (empty) — **DIVERGES** |
| `XM:Z:\n` (empty value at EOL) | undef (no match) | `""` |

For real Bismark data this never happens (XM length == read length ≥ 1), so the divergence is
**theoretical**. But the SPEC's blanket "absent XM → empty string" elides that:
1. The Perl path is **regex-on-text** (greedy-skip behavior on degenerate values), whereas the
   Rust path is **structured-tag-read**. These are equivalent **only** for the normal case where
   the XM value is non-empty and followed by whitespace.
2. The SPEC should document the empty-XM-value case as a **known, accepted divergence** (Rust is
   arguably *more correct*), not be silent about it. **Important** to add to §10 (Deviations) so a
   future reader doesn't treat it as a bug.

Also note: the regex stops at the **first** `\s` after the value. For a normal record that is the
tab following the XM value, so the **full** value is captured — confirming Rust direct-tag-read
== Perl regex for all real records. (I verified XM-as-last-tab-then-`\n` also captures the full
value.) ✔ — equivalence holds for production inputs.

### 2.3 Assumption about `samtools view -h` header duplication

Perl writes header lines (`^@`) to **both** OUT and REMOVED (lines 121–125). The Rust port writes
the header once via `write_header` to each of the two writers. These are **equivalent under the
body-only gate (D1)** because the gate compares `samtools view` (no `-H`) output. The SPEC §5/§6.3
implies this but never states it directly — add a sentence confirming "Perl re-prints `@`-lines
to both streams; noodles writes the header once per writer; equivalent under body-only gate."
**Optional** (the conclusion is correct).

---

## 3. Efficiency analysis

- **Single-threaded streaming** with `record_bufs` is O(records), O(1) memory (one or two records
  resident). This matches the Perl's streaming model and is appropriate. ✔
- The PE path holds at most 2 records — fine.
- mimalloc global allocator: free ~10% win, byte-neutral, consistent with siblings. ✔
- Deferring `--parallel` is justified: the Perl has no parallelism, and the
  [[project_bismark_bedgraph_port]] / [[feedback_extractor_parallel_cpu_messaging]] memories show
  threading is often counterproductive and complicates the per-core messaging story. **No concern.**
- The decision function is a tight per-byte loop over XM (≤ read length). With the early `last`
  on threshold satisfaction, worst case is full XM scan. Negligible. ✔
- One micro-note: the SPEC's pseudocode runs the consecutive-reset check on **every** char even in
  non-consecutive mode via the `if consecutive and (...)` guard — that's a single boolean test,
  trivial, and matches Perl's structure. No optimization needed; keep it faithful.

No efficiency concerns. The design will comfortably exceed the Perl (which pays two
`samtools view` subprocess hops per file + per-char Perl interpretation).

---

## 4. Validation sufficiency

### 4.1 What the proposed validation catches well

- The body-only `samtools view | cmp` gate (D1) is the correct methodology and is immune to BGZF
  block-boundary and BAM integer-width re-encoding differences (the spike proved this). ✔
- The hermetic fixture matrix in §8 enumerates the right char classes, boundary counts, the
  consecutive reset, and percentage rounding boundary. ✔
- The real-data gate (10M SE + PE, default/consecutive/percentage) is the production-scale proof,
  consistent with sibling crates. ✔

### 4.2 ★ Gaps that could let a silent divergence through

1. **PE odd-record-count die path (CRITICAL).** Not in the test matrix. The faithful behavior is
   a *mid-stream die with partial output + empty report*. Must be a golden (and the chosen PE
   fixture must NOT accidentally be odd unless that's the intent). See §1.5.
2. **PE pair where R1 fails (so R2 is never examined).** The §8 matrix says "exercise every
   char" but doesn't explicitly require a pair whose R1 fails and whose R2 *would have passed* —
   needed to prove the Rust replicates the "R2 not examined" short-circuit (otherwise a bug where
   Rust examines R2 anyway would still produce the same keep/remove outcome and **hide**, until a
   crafted case where examining R2 changes nothing — so this is about code-path coverage, but a
   targeted unit test in `filter.rs` is cheap and worthwhile).
3. **PE pair where R1 passes, R2 fails.** Confirms the pair is removed on R2. (I built this case
   manually and the Perl removes the pair — `p2` removed because its R2 had `HHH`.) Add to matrix.
4. **The `kicked/count` percentage in the report** (`$percent`) at fractional values. The matrix
   exercises XM percentage rounding but should also pin a report where `kicked/count` is itself a
   non-trivial `%.1f` (e.g. 1/3 → 33.3%) to lock the **report-line** rounding, distinct from the
   per-read percentage-mode rounding. Currently the SE default fixture gives `2/203 → 1.0%` and PE
   gives `1/2 → 50.0%`; add at least one `*.3%`/`*.7%` report case.
5. **Unmapped read through SE.** The spike explicitly flags it never tested unmapped round-trip.
   §8 lists "an unmapped read" in the fixture — good — but the SPEC should state the **expected
   routing**: SE keeps it (no XM → kept → OUT), and the **decoded body must be byte-identical**
   to what `samtools view` of the Perl output yields. Make this an explicit assertion, not just
   "include an unmapped read." Also: confirm `record_bufs` yields the unmapped record in the same
   relative order (it does, per the spike's read path notes, but the golden must prove it).
6. **PE with an unmapped mate.** The spike notes "for PE an unmapped mate would need an XM or it
   dies." If a real PE BAM has an unmapped mate with no XM, the Perl **dies**. The SPEC should
   decide: is this in-scope (replicate the die) or out-of-scope (documented limitation)? Currently
   silent. **Important** — at minimum document it.
7. **`--threshold` supplied together with `--percentage_cutoff`.** The Perl accepts both (threshold
   is just ignored in percentage mode because the per-char check is guarded out). The CLI section
   doesn't say whether the Rust rejects or ignores a co-supplied `--threshold`. Faithful = accept
   and ignore. Add a unit test pinning that `--percentage_cutoff 20 --threshold 7` behaves
   identically to `--percentage_cutoff 20` (threshold has no effect). **Important.**
8. **`@PG`-absent + no `-s`/`-p`** → the "Please specify either -s … or -p …" die (line 65–66).
   In the matrix this is an error-path test; not byte-gated, but worth one integration test
   asserting exit 1 + the message on STDERR.

### 4.3 Things that are fine as-is

- Truncation and emptiness error paths are correctly marked non-gated. ✔
- The `count==0 → N/A` branch is dead (emptiness check dies first); keeping it for faithfulness is
  fine and the SPEC says so. A unit test on `report.rs` for the `N/A` formatting (even though the
  pipeline can't reach it) is still worthwhile and the SPEC §12 includes it. ✔

---

## 5. Alternatives

1. **SAM-text passthrough vs structured RecordBuf.** The spike proved structured `RecordBuf`
   round-trip is body-byte-identical, so the chosen approach is correct and simpler than a
   SAM-text fallback. No change. (The empty-XM-value divergence in §2.2 is the only behavioral
   gap, and it's strictly *more correct* — not a reason to switch to text.)
2. **Reuse `bismark-io`'s `BamReader` instead of raw `record_bufs`.** Correctly rejected: that
   reader silently drops unmapped reads and requires XR/XG via `BismarkRecord`. The SPEC's choice
   to read raw `record_bufs` (the bam2nuc C-1 pattern) is the right one and is the *only* way to
   pass unmapped reads through verbatim. ✔ (Verified by reading `bismark-io/src/read.rs`:
   `filter_unmapped_then_classify` drops FLAG&0x4, and `string_tag` errors on missing XR.)
3. **Extracting XM by regex-on-text (to bit-for-bit match the Perl, including the empty-value
   quirk).** Not worth it — it would intentionally reproduce a Perl bug for a case that cannot
   occur in real data, at the cost of a slower, fragile code path. Document the divergence (§2.2)
   instead. ✔
4. **PE pairing via flag-based mate matching vs strict adjacency.** The SPEC folds adjacent-qname
   equality into the loop (faithful). An alternative (group by qname) would be more robust but
   would *change behavior* on malformed inputs and break byte-identity ordering. Keep the
   faithful adjacency model. ✔
5. **Version/help exit codes (Deviations 1–2).** Reasonable to use clap-style exit 0 for
   `--help`/`--version` rather than the Perl's `exit 1` for help. These are explicitly flagged for
   confirmation (A1) and are not gated. Recommend: **confirm exit 0 is acceptable** — it diverges
   from Perl's `exit 1` on `--help`, which a strict shell harness checking `$?` could notice.
   Sibling crates already chose clap-style, so consistency favors exit 0. Just get Felix's
   explicit sign-off (already an open question A1).

---

## 6. Action items (prioritized)

### Critical

- **C1. Resolve the PE odd-record-count behavior and add it as a golden.** The SPEC's primary
  fixture (`tiny_pe_bismark.bam`, 203 records) **dies** on the Perl PE path (exit 255, empty
  report, partial kept-BAM). Decide the faithful contract (mid-stream die + partial output + empty
  report) and either trim the fixture to even for the "happy PE" golden or use the odd case as the
  explicit PE-die golden. The Rust loop must die when the second `next()` is `None` at a pair
  boundary. (§1.5)

### Important

- **I1. Document the empty-XM-value divergence** in §10 (Deviations). Perl's `XM:Z:(.+?)\s` regex
  skips an empty value and captures the *next* tag as garbage; the Rust direct-tag-read yields
  `""`. Equivalent for all real data; document as accepted (Rust is more correct). (§2.2)
- **I2. Decide + document PE-unmapped-mate behavior.** A PE pair with an unmapped, XM-less mate
  makes the Perl die. State whether the Rust replicates the die or treats it as a documented
  limitation. (§4.2.6)
- **I3. Add PE keep/remove code-path goldens:** (a) R1-fails-so-R2-not-examined, (b)
  R1-passes-R2-fails-pair-removed. Proves the short-circuit and the either-mate-fails routing.
  (§4.2.2–3)
- **I4. Add a `--percentage_cutoff` + `--threshold` co-supplied test** asserting threshold is
  ignored (faithful). (§4.2.7)
- **I5. Make the unmapped-read assertion explicit in §8**: expected routing = kept→OUT, decoded
  body byte-identical, same relative order. Not just "include an unmapped read." (§4.2.5)
- **I6. Add a report-line rounding golden** where `kicked/count` itself rounds non-trivially
  (e.g. 1/3 → 33.3%), distinct from the per-read percentage rounding. (§4.2.4)

### Optional

- **O1.** Note in §7/§8 that the Rust port omits the Perl's `sleep(1)` calls (in
  `test_positional_sorting`), so its timing line durations legitimately differ — expected under
  D2's prefix/format normalization. (§1.1)
- **O2.** State the header-duplication equivalence explicitly: Perl re-prints `@`-lines to both
  OUT and REMOVED; noodles writes the header once per writer; equivalent under the body-only gate.
  (§2.3)
- **O3.** Clarify in §3.1 that "no files" + a bad option value yields the **no-files** exit
  (the `@ARGV`-empty check precedes option validation), to prevent an implementer reordering it.
  (§1.8)
- **O4.** Note that SE absent-XM emits a `Use of uninitialized value` STDERR warning in the Perl;
  the Rust STDERR will differ (not gated), expected. (§1.4)
- **O5.** Confirm A1 (`--help` exit 0 vs Perl's exit 1) with Felix, since a `$?`-checking harness
  could observe it. (§5.5)

---

## 7. Summary of empirical verifications performed

All against `perl 5.34.1` + `samtools 1.21` on macOS:

- Ran the Perl tool SE + PE + percentage + consecutive + multi-file and dumped reports with
  `sed -n l` — all four removed-line variants, the SE/PE space quirk, the `consecutive ` insert,
  the trailing `\n\n`, and last-file-only timing placement reproduced exactly.
- Simulated the XM decision loop in Perl: char semantics, consecutive reset set, threshold-skip
  in percentage mode, and rounding-before-`>=` all confirmed.
- Verified Rust `{:.1}` == Perl `%.1f` on the 12.25 halfway case (round-half-to-even).
- Reproduced the PE odd-record die (exit 255, empty report) on the SPEC's own fixture.
- Reproduced the empty-XM-value regex garbage-capture divergence.
- Reproduced the truncation char-class firing on arbitrary bracketed lines.
- Read `bismark-io/src/{read.rs,tags.rs,write.rs}` and `bismark-dedup/src/{filename.rs,cli.rs,
  report.rs,pipeline.rs}` to confirm the architecture decision (raw `record_bufs` to pass unmapped
  reads; `string_tag` errors on missing XR; `detect_paired_from_header` faithful) and the
  filename-derivation difference from dedup.

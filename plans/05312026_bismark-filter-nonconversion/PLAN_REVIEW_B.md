# PLAN REVIEW B — `filter_non_conversion` Rust port SPEC

**Reviewer:** B (independent, fresh context)
**Date:** 2026-05-31
**Target:** `plans/05312026_bismark-filter-nonconversion/SPEC.md` (rev 0)
**Ground truth:** `/Users/fkrueger/Github/Bismark/filter_non_conversion` (v0.25.1, 724 lines)
**Method:** Re-derived every byte-identity claim against the LIVE Perl, running
`perl 5.34.1` + `samtools 1.21` on hand-built fixtures and the committed
`tiny_pe_bismark.bam`. Empirical results cited inline.

---

## Verdict in one line

The SPEC is **mostly accurate and well-grounded** — the report format strings, XM
character semantics, consecutive-reset, percentage rounding, SE/PE die-vs-keep, filename
derivation, and CLI validation order all reproduced **exactly** under live Perl. But there
are **two correctness-grade defects** that will cause silent divergence or wrong behaviour
if implemented as written, plus several important gaps. Do NOT proceed to implementation
until C1 and C2 are resolved.

---

## 1. Logic review (re-derived against live Perl)

### What I verified as CORRECT (empirically)

| Claim | SPEC ref | Perl ref | Empirical result |
|-------|----------|----------|------------------|
| SE Line A = **one** space `<< in total` | §7 | 318 | `>> ./se.bam << in total:` ✅ one space |
| PE Line A = **two** spaces `<<  in total` | §7 | 314 | `>> ./pe.bam <<  in total:` ✅ two spaces |
| 4 Line-B variants (PE/SE × %/threshold) | §7 | 336/341/347/351 | All 4 reproduced byte-for-byte ✅ |
| `$insert='consecutive '` placement | §6/§7 | 332/340/350 | `at least 3 consecutive non-CG calls per read` ✅ |
| `\n\n` trailing the removed line | §7 | 341 etc. | confirmed (blank line then timing) ✅ |
| Timing line on the single/last report | §7/D2 | 664 | present; value varied `0s`→`2s` (the `sleep(1)`s) ✅ |
| `%.1f` percent + `N/A` when count==0 | §7 | 323-327 | `(33.3%)`, `(N/A%)` reproduced ✅ |
| H/X → nonCpG & total; h/x → total only | §6 | 140-146 | confirmed via boundary fixtures ✅ |
| consecutive reset = `z`/`h`/`x` only; `Z` transparent | §6 | 148-152 | `ZHHHZ`→removed, `HHzHH`→kept ✅ |
| increment→reset→threshold-check + early `last` | §6 | 138-160 | `HHHzH`→fail@3rd, `HHzHH`→keep ✅ |
| percentage decides AFTER loop on **rounded** value, `>=` | §6 | 162-176 | `4/20=20.0%`≥20 fail; `3/20=15.0%` pass ✅ |
| percentage mode SKIPS per-char threshold | §6 | 154/222/270 | `mincount4` (4 consecutive H) kept ✅ |
| min_count gate (total non-CG ≥ min) | §6 | 165 | `mincount4` total=4<5 → kept ✅ |
| SE missing XM → kept, no error | §6.1 | 127,136 | `noxm` kept ✅ |
| PE missing XM (either mate) → die | §6.2 | 194-196 | dies "Failed to extract methylation calls…" ✅ |
| Either-mate-fails → whole pair removed | §6.2 | 247,297-305 | pairB (R2 HHH) → both removed ✅ |
| Filename: strip only `.bam`, NO dir strip | §5 | 85-98 | (logic-verified; matches Perl regex) ✅ |
| `foobam` passes top gate, no `.bam` to strip | §4.1/§5 | 37,86 | `foobam.nonCG_filtered.bam` produced ✅ |
| `@PG`-based PE auto-detect | §4.4 | 360-402 | `-1`/`-2` in CL → PE ✅ |
| PE sort detection real-check = adjacent qname equality; `@SO` is dead | §4.5/§10.5 | 430(dead)/447-459 | `@HD SO:coordinate` did NOT trigger; qname mismatch did ✅ |
| CLI validation order (% range before s/p) | §3.1 | 520-546 | `%101 + -s -p` → range error first ✅ |
| `--help` exits 1; `--version` exits 0 | §10.1 | 723/510 | confirmed ✅ |
| Body-only gate mandatory (samtools adds @PG) | D1 | pipe | input 3 @PG → Perl out 5 @PG ✅ |
| Tag order preserved through pipe | spike | — | `NM,MD,XM,XR,XG` identical ✅ |

That is a strong rev-0. The verbatim-passthrough body comparison held: on my clean PE
fixture the Perl kept-BAM body was **byte-identical** to the input records that were kept.

### CRITICAL DEFECTS

#### C1 — The `count==0` / `N/A` branch is **NOT dead code**; the SPEC's emptiness-gate model is wrong.

The SPEC (§4.3, §6) asserts: *"This makes the `count == 0 → "N/A"` report branch dead but
it is kept for faithfulness."* **This is false, and the reasoning behind it is wrong.**

The Perl emptiness/truncation checks are **not gated on the top `/bam$/` filename gate**
(line 37). They are gated on a DIFFERENT, **dotted** regex:

- line 42: `if ($file =~ /(\.bam$)/){ bam_isTruncated($file); }` — literal `.bam`
- line 47: `if ($file =~ /(\.bam$|\.sam$)/){ bam_isEmpty($file); }` — literal `.bam`/`.sam`

The top gate at line 37 is `/bam$/` (no dot). So a file named `emptyfoobam` (or any name
ending in `bam` but **not** `.bam`) **passes the top gate but is never checked for
emptiness/truncation**. Empirically:

```
$ perl filter_non_conversion -s ./emptyfoobam      # header-only BAM, renamed
Analysed sequences (single-end) in file >> ./emptyfoobam << in total:    0
# report written:
Analysed sequences (single-end) in file >> ./emptyfoobam << in total:    0
Sequences removed … (at least 3 non-CG calls per read):    0 (N/A%)
filter_non_conversion completed in 0d 0h 0m 0s
```

So the **N/A branch IS reachable** (via a header-only BAM whose name lacks a literal
`.bam`/`.sam`), AND the empty `.bam` path dies *before* `process_file` (so for a properly
`.bam`-named empty file, no report is written at all — I confirmed `./empty.bam` produces a
**0-byte report** and exit 255).

Consequences if implemented as the SPEC describes (run emptiness check for "BAM input"):
1. The Rust port would **reject `emptyfoobam` as empty** where Perl processes it and emits
   the N/A report → divergence, and the N/A branch becomes genuinely unreachable in Rust
   (the opposite of Perl).
2. For a real empty `*.bam`, Perl dies with **no report file**; the SPEC's §4.3 says "die
   with the same message" which is right, but §6 still claims the N/A branch is "kept for
   faithfulness" — it should be kept because it IS reachable, via the non-dotted path.

**Action:** Gate truncation/emptiness on a **literal `\.bam$`** (and `\.sam$` for empty)
check, NOT on the top `bam$` gate. Keep and TEST the N/A branch with a header-only,
non-`.bam`-suffixed input. This is a silent-wrong-output path the fixture matrix must cover.

#### C2 — PE odd-record-count behaviour is under-specified, and the committed fixture exposes it.

The committed `tiny_pe_bismark.bam` has **203 records = 101 pairs + 1 lone trailing R1**
(record 115's mate is absent). Running Perl `-p` on it:

```
… (101 pairs processed) …
Failed to extract methylation calls from Read 1 or Read 2 for sequence pair
Read 1: 115_…_R1   83   …   XM:Z:hh.....  …
Read 2:
# exit 255; 202 records already written to the two output BAMs; report = 0 bytes
```

So Perl's PE loop reads two-at-a-time; on a final lone R1 it does `$_ = <IN>` → `undef`,
the `$meth_call_2` regex matches nothing, and **it dies** — but only AFTER writing the 202
preceding records and **before** the SUMMARY, so the report file is **empty (0 bytes)**.

The SPEC's edge-case list (§ review-prompt, §8) says it will cover "a PE file with an odd
number of records" but **nowhere specifies the exact Perl behaviour**: (a) the die message,
(b) partial output of all complete preceding pairs, (c) **zero-byte report**, (d) exit
nonzero. The SPEC §6.2 says "Both mates must have XM or die" but conflates "mate missing
XM tag" with "mate record absent (EOF)". In Perl these are the **same code path** (both
yield `$meth_call_2 = undef`), but the Rust port must explicitly handle the `record_bufs`
iterator returning `None` on the second `next()` — and must replicate "die with partial
output already flushed, report empty."

**Action:** Specify the lone-trailing-R1 path explicitly: when PE and the second
`record_bufs().next()` is `None`, die with the Perl message (Read 2 rendered as empty),
having already written the prior pairs, and write **no** report. Add this to the fixture
matrix. NB this directly affects whether the committed `tiny_pe_bismark.bam` can be used as
a clean PE golden — it CANNOT without trimming to 202 records, or it must be the
odd-count death fixture.

### IMPORTANT issues

#### I1 — `bam_isTruncated`/`bam_isEmpty` ordering and the non-dotted skip (related to C1).

Beyond C1, note the Perl runs truncation FIRST (line 42-44) then emptiness (47-49), each on
the dotted regex. The SPEC §4.2/§4.3 present them as unconditional "for BAM input" steps and
do not mention that a `bam`-but-not-`.bam` file skips **both**. Spell out the dotted gating
and the order (truncation before emptiness) for faithfulness, even though both are error
paths (not byte-gated).

#### I2 — `--percentage_cutoff` / `--minimum_count` accept **negative** integers in Perl (`=i`).

Perl `GetOptions('percentage_cutoff=i')` parses `--percentage_cutoff -5` as `-5`, then the
range check (`>=0 and <=100`) rejects it. Empirically confirmed: `--percentage_cutoff -5` →
"range of 0-100" error. With clap, a bare `-5` after a value-taking long flag is usually
consumed as the value (clap supports negative-number values), but the SPEC should (a) ensure
the clap type is **signed** (`i64`/`i32`, not `u32`) so the range check — not a parse error —
is what fires, matching Perl's error message and exit path; (b) add a unit test for negative
`--percentage_cutoff` and negative `--minimum_count`. `--threshold -1` likewise: Perl's `=i`
accepts it, then `unless ($threshold > 0)` fires "sensible value". Use signed types.

#### I3 — `--threshold 0` error message interpolates the **value**, not a label.

Perl line 598: `die "Please use a sensible value for $threshold …"` → with `--threshold 0`
this prints `Please use a sensible value for 0 (positive numbers only, default: [3])`.
This is an error path (not byte-gated), but the SPEC's §3.1 step 8 just says "die unless >0"
— if any test asserts the message, it must include the interpolated value. Minor; flag so a
reviewer doesn't "fix" it to read more sensibly and thereby diverge.

#### I4 — `bam_isEmpty` "one line is enough" includes **header** lines? No — verify.

`bam_isEmpty` opens `samtools view $file` (NO `-h`), so it only sees alignment records, not
headers (line 613). The SPEC §4.3 correctly says "Header-only BAM = empty." Good — but make
sure the Rust peek skips the header (uses `record_bufs`, which is post-header) and that a
BAM with a header + zero alignments is treated as empty. Confirmed Perl behaviour: `empty.bam`
(header only) → dies. Just ensure the Rust peek semantics match (peek first **alignment**
record).

#### I5 — `count` per-read (SE) vs per-pair (PE) and `++$count` placement.

SPEC §6.1/§6.2 say count is per-read / per-pair. Confirmed: PE increments `$count` once per
pair (line 202), and on the lone-R1 death the pair is **not** counted (die happens at 194,
before 202). The SPEC should note that the trailing-lone-R1 pair is never counted (relevant
only to STDERR, since report is empty in that case). Minor.

---

## 2. Assumptions

- **A4 (PE qname-grouped):** Validated as the input contract; the Perl's only real
  enforcement is adjacent-qname equality, which the SPEC folds into the loop (§4.5). Sound.
  But note: the Perl pre-pass (`test_positional_sorting`) runs `samtools view -h` a SECOND
  time over the file (lines 414-461) and the death there happens **before** any output is
  written; the SPEC's fold-into-loop approach moves the death to mid-stream, so **partial
  output may exist** on a sort-mismatch in Rust where Perl writes none. The SPEC acknowledges
  this in §10.5 ("Partial output may exist… matches Perl's no-rollback `process_file`") — but
  that's slightly wrong: for a SORT mismatch specifically, Perl dies in the pre-pass with NO
  output, whereas Rust would die mid-process_file with partial output. This is a **behaviour
  divergence on malformed PE input** that §10.5 mischaracterises. It is defensible (error
  path, malformed input) but should be documented accurately, not as "matches Perl."

- **A5 (XM is `Z` string; absent legal in SE, dies in PE):** Validated. One subtlety the
  SPEC omits: Perl's PE guard is `unless($meth_call_1 and $meth_call_2)` — Perl
  **truthiness**, not definedness. An XM value of literal `"0"` would be falsy and trigger
  the die even when present. Real XM values are never `"0"` (always methylation chars or
  empty), so this is inert, but the Rust port should treat **absent OR empty** XM as the
  failing condition for PE to match `and`-truthiness exactly (empty string is also falsy in
  Perl). Worth a one-line note + test.

- **Implicit assumption the SPEC should surface — secondary/supplementary alignments.** Perl
  streams `samtools view -h` (every record, including FLAG 0x100/0x800). It does NOT skip
  secondary/supplementary, and `record_bufs` also yields them. For SE these are routed
  individually like any read. **For PE, a secondary/supplementary alignment breaks the
  strict two-at-a-time pairing** — Perl would pair it with whatever follows and likely die on
  qname mismatch or mis-route. The SPEC says nothing about secondary/supplementary. Bismark
  BAMs don't emit them in normal operation, so this is low-risk, but the SPEC should state
  the assumption (no secondary/supplementary; if present, behaviour = whatever the two-at-a-
  time pairing produces, matching Perl) rather than leave it unaddressed.

- **Unmapped reads (§6.3):** The SPEC's claim that `record_bufs` yields unmapped reads while
  bismark-io's reader drops them is **correct** — I verified `read.rs:580-594`
  (`filter_unmapped_then_classify` drops FLAG&0x4) and the spike's read path uses raw
  `record_bufs`. Sound. But the fixture has NO unmapped reads (spike §7 limitation), so this
  is unproven end-to-end. The SPEC commits to a synthetic unmapped-read fixture (§8) — good,
  but see V2: an unmapped read in a **PE** file is a hazard (unmapped mate has no XM → PE
  dies), which the SPEC half-notes via the spike but doesn't pin down.

---

## 3. Efficiency analysis

- Single-threaded + mimalloc (§10.4) is the right call; the workload is a linear pass with
  trivial per-record work (XM byte scan, early `last`). Memory is O(1) per record (one
  `RecordBuf` at a time for SE, two for PE). No concerns.
- The early `last` on threshold (§6) is faithful AND efficient (stops scanning XM once the
  count is hit). Percentage mode must scan the whole XM (no early exit) — correct, matches
  Perl.
- Deferring `--parallel` to v1.x is reasonable; dedup showed ~4.9× at N=4 but the bottleneck
  here is BGZF decode/encode, identical to dedup. Re-evaluate post real-data gate as the
  SPEC says. No issue.
- `detect_paired_from_header` serializes the header to SAM text and substring-scans (read.rs
  `arg_present`). That's O(header size) per file, negligible. Fine.

---

## 4. Validation sufficiency

The proposed gate (body-only `samtools view` cmp + report cmp modulo timing) is the right
shape and matches dedup/bam2nuc. But it has **blind spots for the highest-risk silent paths**:

#### V1 — The fixture matrix omits the two reachable-but-easy-to-miss states from C1/C2.

Add explicit fixtures:
- **Header-only BAM named `*bam` (not `*.bam`)** → exercises the N/A report branch (C1).
  This is the ONLY way to prove the N/A formatting and that the emptiness check is correctly
  dotted-gated. Without it, a Rust impl that over-eagerly rejects empties would pass every
  other test and silently diverge here.
- **Odd-count PE (lone trailing R1)** → exercises the partial-output + empty-report + die
  path (C2). Must assert: (a) exit nonzero, (b) the N complete pairs ARE in the output BAMs,
  (c) report is 0 bytes. The committed `tiny_pe_bismark.bam` is itself this case (203 recs)
  — so the gate must NOT naively use it as a "clean PE happy path" golden.

#### V2 — Unmapped-in-PE is a silent hazard not pinned down.

A PE file with an unmapped mate: the unmapped record has no XM → Perl PE **dies**. But if
the unmapped read is the R2 of a pair, you get the C2 die; if it's interleaved oddly you get
a qname-mismatch die. The SPEC must specify and test: unmapped read in SE (kept verbatim,
routed to OUT — the spike notes this) vs unmapped read in PE (dies, since no XM). Add both.

#### V3 — Percentage rounding: the gate should pin a half-to-even tie case, not just `.96→20.0`.

The SPEC §6 says use `format!("{:.1}")` (round-half-to-even) "matches C printf". Perl's
`sprintf("%.1f")` uses the C library's rounding (round-half-to-even on glibc/macOS).
`format!("{:.1}")` in Rust also rounds half-to-even. These AGREE for `.x5` ties, but the
fixture matrix should include an actual tie (e.g. `1/8 = 12.5` exact, or a value landing on
`x.x5`) to prove parity, since this is the one place a 1-ulp rounding difference flips a
keep/remove decision. The `19.96→20.0` example is NOT a tie (it rounds up unambiguously);
add a genuine half-way case (e.g. construct counts giving `*.?5` exactly, like 1 meth of 40
total = 2.5%, or 5 of 40 = 12.5%) at the cutoff boundary.

#### V4 — Multi-file timing-line placement (A3) needs a 2-file fixture.

The SPEC commits to it (§8 "a multi-file run (timing line only on the last report)") — good.
Confirm the gate asserts file 1's report has **no** timing line and file 2's report **does**.
I verified the Perl mechanism (REPORT bareword filehandle is reused per file; only the last
stays open at script exit for line 664) — A3's reasoning is correct.

#### V5 — Real-data gate cells.

§8 lists default + `--consecutive` + `--percentage_cutoff`. Add a `--threshold N` (non-
default, e.g. 5) cell and an SE+PE for each mode; the current "10M SE + PE, 3 cells" is a
bit thin given threshold/consecutive/percentage are three distinct code paths × SE/PE = 6.
Also: run with the input path string **identical** to the Perl baseline (the report echoes
`$infile` verbatim — §7 notes this; make it a gate invariant).

#### V6 — What's adequately covered.

Char-class unit tests, boundary counts, filename derivation, report variants, CLI validation
— all listed (§12) and sufficient. The body-only comparison correctly sidesteps BAM integer-
width and BGZF-block differences (spike §4) — validated.

---

## 5. Alternatives worth considering

- **A-1: Trim the committed fixture to 202 records for a clean PE happy-path golden, and keep
  the 203-record version as the odd-count death fixture.** Strongly recommended given C2 —
  otherwise every "happy path PE" gate using `tiny_pe_bismark.bam` will hit the die.

- **A-2: Reconsider whether to fold the sort check into the loop (§10.5) vs a pre-pass.** The
  SPEC's fold-in is "stronger" but changes the partial-output behaviour on sort-mismatch (see
  §2 A4). A faithful alternative: keep a cheap qname-adjacency check in the loop (you need it
  anyway for pairing) AND accept that the failure mode differs from Perl's pre-pass only in
  whether partial output exists. Since this is an error path on malformed input, the fold-in
  is fine — just document the divergence accurately (it does NOT "match Perl's no-rollback"
  for the *sort* case; Perl writes nothing for sort errors because it dies in the pre-pass).

- **A-3: For the PE missing-XM / lone-R1 die, render Read 2 exactly as Perl does** — Perl's
  message embeds `Read 1: <full SAM line>\nRead 2: <full SAM line or empty>`. If any test or
  user-facing parity matters here, reconstruct the SAM-text rendering of the offending
  records. Likely overkill (error path, not gated) — recommend NOT byte-matching this message,
  just emitting a comparable one, and saying so explicitly in §10.

- **A-4: Empty-input behaviour.** Consider matching Perl's "die before report, leave 0-byte
  or no report" precisely vs. a cleaner "die, no files created." Perl actually creates the
  three output files (via `open` in process_file) only if it reaches process_file; for a
  dotted-`.bam` empty file it dies in bam_isEmpty **before** process_file, so **no output
  files at all**. Pin this in the SPEC + test (no partial `.nonCG_filtered.bam` for an empty
  `.bam`).

---

## 6. Action items (prioritised)

### CRITICAL (block implementation)
- **C1.** Fix §4.2/§4.3/§6: the truncation/emptiness checks are gated on **literal `\.bam$`
  / `\.sam$`**, NOT the top `bam$` gate. The `count==0`/`N/A` report branch is **reachable**
  (header-only BAM named `*bam` without a dot) — it is NOT dead code. Implement the dotted
  gating and TEST the N/A path with such a fixture. (Empirically: `emptyfoobam` → N/A report;
  `empty.bam` → die, no report.)
- **C2.** Specify the **PE odd-record-count / lone-trailing-R1** path exactly: die with the
  Perl message, partial output of prior complete pairs already flushed, **0-byte report**,
  exit nonzero. The committed `tiny_pe_bismark.bam` (203 recs) IS this case — do not use it
  as a clean PE golden. Add it (or a trimmed twin) to the fixture matrix accordingly.

### IMPORTANT
- **I2.** Use **signed** CLI integer types for `--percentage_cutoff` / `--minimum_count` /
  `--threshold` so the Perl range/sensibility checks (not clap parse errors) are what reject
  negatives; add negative-value tests.
- **V1/V2.** Fixture matrix must include: header-only non-`.bam` (N/A), odd-count PE
  (partial+empty-report), unmapped-in-SE (kept→OUT), and unmapped-in-PE (die, no XM).
- **A4 / §10.5 correction.** Re-word §10.5: the fold-in sort check does NOT match Perl's
  no-output behaviour for *sort* errors (Perl dies in the pre-pass, writes nothing); document
  the partial-output divergence accurately.
- **A5 truthiness.** Treat absent OR empty XM as the PE failing condition (Perl `and`
  truthiness); add a test.
- **V3.** Add a genuine half-to-even rounding tie at the percentage cutoff boundary.

### OPTIONAL
- **I1/I3/I4/I5.** Document truncation-before-emptiness order; note `--threshold` error
  interpolates the value; confirm emptiness peeks first **alignment** (post-header) record;
  note the lone-R1 pair is uncounted.
- Surface the **secondary/supplementary** assumption (none expected; behaviour = two-at-a-
  time pairing as Perl).
- **A-3.** Decide NOT to byte-match the PE-die message (error path); state so in §10.
- **A-4.** Pin: an empty `.bam` produces **no** output files (dies before process_file).

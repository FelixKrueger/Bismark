# PLAN_REVIEW_B — `bismark-methylation-consistency` (Rust port)

**Reviewer:** B (independent, fresh context). **Date:** 2026-05-29.
**Targets reviewed:**
- `plans/05292026_bismark-methylation-consistency/SPEC.md`
- `plans/05292026_bismark-methylation-consistency/PLAN.md`
**Cross-checked against:**
- Perl `methylation_consistency` (556 lines, v0.25.1)
- `bismark-io/src/{read,write,record,tags,pair,lib}.rs`
- `bismark-dedup/src/{main,cli,pipeline,filename,report,lib}.rs`

**Overall verdict:** The plan is unusually well-grounded — most API-reuse claims, the round-then-compare contract, and the `None→SE` divergence from dedup are all *correct* against the actual source. I confirmed several load-bearing claims empirically (Perl/Rust rounding parity; Perl's sprintf-string-vs-int coercion). There are, however, **three Critical issues** that would either silently break byte-identity or are stated incorrectly in the SPEC, plus several Important items. None are fatal to the approach; all are fixable before implementation.

---

## 1. Logic review

### 1.1 Round-then-compare (SPEC §2.5, PLAN A3 step 3) — CORRECT, but the SPEC's confidence rationale is WRONG (Critical-adjacent)

I verified the Perl semantics directly. `$percent_methylated = sprintf("%.1f", ...)` yields a **string**; the subsequent `$percent_methylated <= $lower_threshold` forces Perl numeric coercion of that string back to a double, then compares to the integer threshold. So the effective operation is *round-to-1-decimal, then numeric compare* — exactly what PLAN A3 step 3 reproduces with `format!("{:.1}", …).parse::<f64>().unwrap()`. Verified examples (Perl):
- `10.04 → "10.0" → <= 10 → unmeth`
- `10.05 → "10.1" → NOT <= 10 → mixed`

So the **decision logic is byte/decision-identical**. Good.

**BUT** SPEC §8 Spike 1 justifies "Confidence: high" with the claim that *"exact-halfway ties are essentially unreachable for computed ratios."* **This is false.** `meth/total*100` lands on exact halfway ties whenever the ratio is exactly representable in binary — which happens routinely for power-of-two-ish totals. I enumerated them:
- `meth=1, total=16 → 6.25` (exactly representable) → both Perl and Rust round-half-to-even → `"6.2"`.
- `meth=3/16 → 18.75 → "18.8"`, `5/16 → 31.25 → "31.2"`, … (8 ties just in total=16).

I compiled a Rust probe (`format!("{:.1}", m as f64/t as f64*100.0)`) and diffed against Perl for all eight total=16 ties: **all 8 match** (`6.2/18.8/31.2/43.8/56.2/68.8/81.2/93.8`). They match because **both** glibc `printf` and Rust's Grisu/Ryū formatter round-half-to-even on the **same stored f64**, and both compute the f64 with the identical IEEE-754 op sequence. So the *conclusion* (high confidence) is correct, but the *stated reason* ("ties unreachable") is wrong and should be corrected — otherwise Spike 1 may be scoped to skip the very cases that matter.

The real invariant that guarantees parity is: **Rust must compute the f64 in the exact same order as Perl** — `meth / (meth+unmeth) * 100` with the denominator summed as integers first. PLAN A3 step 3 does this (`meth as f64 / total as f64 * 100.0`, `total = meth + unmeth` as `u32`). If anyone "optimizes" this to `(meth*100) as f64 / total as f64` or factors differently, parity can break on ties. **Action: pin the exact expression form in the plan and in a code comment, and re-scope Spike 1 to assert it specifically on the power-of-two-total ties + the threshold-boundary ties I found below.**

Threshold-boundary ties also exist and are reachable (total=2000 family):
- `199/2000 = 9.950000000000001` (note: the `*100` nudges it just *above* 9.95) → `"10.0"` → unmeth. Not a true tie; rounds up because the stored double exceeds 9.95.
- `201/2000 = 10.05` (exact) → `"10.1"` → mixed.
- `1801/2000 = 90.05` (exact) → `"90.0"` (even-digit) → **meth** (`>= 90`).
- `1803/2000 = 90.149… → "90.1"`.

These are exactly the decisions that flip a read between buckets. Rust matched Perl on all of them in my probe. The plan's boundary unit tests (A3) mention "the 10.04 vs 10.05 edge" — **good, but add the `90.05 → 90.0 → meth` tie and at least one power-of-two-total tie (e.g. `1/16`) to the unit suite**, since those exercise round-half-to-even, which the 10.04/10.05 pair does not.

### 1.2 `None → SE` auto-detect fallback (SPEC §2.3, PLAN B1) — CORRECT

I confirmed the Perl `determine_mapping_type` (lines 354–393) returns `($single,$paired)` both **undefined** when no Bismark `@PG` line is found (the `while` loop simply never assigns). Back in `process_file` (lines 169–177), `unless ($single)` / `elsif ($paired)` both fall through silently, and `$type = $paired ? "paired-end" : "single-end"` (line 321) resolves the undef `$paired` to false → **single-end**. So Perl genuinely defaults to SE on auto-detect failure (no error). PLAN B1 correctly maps `None → SE` and **explicitly avoids inheriting dedup's `CannotAutoDetectMode` error** (which I confirmed exists at `dedup/src/main.rs:376`). This is a real, correctly-identified divergence from the sibling. Good.

One nuance to pin: Perl's auto-detect uses `$_ =~ /\s+--?1\s+/ and $_ =~ /\s+--?2\s+/` on the raw `@PG` line. `bismark_io::detect_paired_from_header` re-serializes the header via noodles and applies `arg_present` with the same `\s … \s` strict-boundary semantics (read.rs:687–696). I verified the `arg_present` tests assert the strict boundary (a `-1` at end-of-line is NOT present). This matches Perl. **Low risk, but note: a real Bismark PE `@PG` line always has a path after `-1`/`-2`, so the trailing-boundary requirement is satisfied; this is the same code dedup/extractor already rely on.**

### 1.3 SE not sort-checked, PE sort-checked (SPEC §2.4, PLAN B2, Open Decision #1) — analysis CORRECT; recommended fix is sound but heavier than needed (Critical)

Confirmed against `bismark-io`:
- `open_reader` (read.rs:562) → `AlignmentKind::from_path` → `BamReader::from_path` → `BamReader::new` → `check_not_coordinate_sorted` **always**. There is **no** `open_reader_without_sort_check` exported (verified `lib.rs:32-39` exports only `open_reader`).
- `BamReader::without_sort_check(reader: R)` exists (read.rs:244) but takes a `BufRead`, **not a path**, and does **not** do magic-byte format dispatch.

So the plan's premise is correct: to honor Perl's "SE is not sort-checked" you cannot use `open_reader` for SE. **However**, the SPEC's framing slightly overstates the problem. Perl's `test_positional_sorting` (lines 445–509) runs **only for PE** (`if ($paired)` at line 180). And the SO check inside it is `die if /^\@SO/` — which is **NOT** the same as noodles' check.

This is a **subtle Critical mismatch the plan does not call out**: Perl tests `/^\@SO/` — i.e. a header **line literally starting with `@SO`**. There is no such SAM header line; the real sort-order tag is `@HD … SO:coordinate`. So **Perl's `@SO` guard never fires on any real SAM/BAM** (the regex is effectively dead code — a long-standing Bismark bug). noodles' `check_not_coordinate_sorted` inspects the `@HD` `SO:` subfield and **does** reject `SO:coordinate`. Therefore:

- PLAN B2 says "Replicate `test_positional_sorting`'s `@HD SO:coordinate` check" — but that is **stricter than Perl**, which never rejects coordinate-sorted PE input via that path (its `@SO` regex misses). The actual Perl PE protection comes from the **per-pair qname adjacency check** (lines 489–502): coordinate-sorted PE data interleaves mates from different pairs, so adjacent qnames differ → `die`. The `@SO` line is a red herring.

**Implication for byte-identity:** For a *coordinate-sorted PE* file, Perl dies via the qname-adjacency mismatch, not via `@SO`. Rust-via-noodles dies via `UnsortedInput` *before reading any record*. **Both die → no output → identical observable outcome** (no output files). So the gate is not broken. But the plan's stated rationale ("replicate the `@SO SO:coordinate` check") is factually wrong about what Perl does, and the **error message/exit path differs**. Since error text is explicitly out-of-gate (SPEC §7), this is acceptable — but the plan should **correct the claim** so the implementer doesn't waste time trying to byte-match a guard that Perl never actually triggers.

**Simpler path than Open Decision #1's recommended bismark-io addition:** Because SE must *accept* coordinate-sorted input but PE can *reject* it (with no output-gate consequence, since Perl also produces no output on coordinate-sorted PE), the cleanest approach is:
1. **PE:** use `open_reader` as-is. Its `UnsortedInput` rejection of coordinate-sorted PE is *output-equivalent* to Perl's qname-adjacency `die` (both → no files). No bismark-io change needed for PE.
2. **SE:** needs a no-sort-check reader. Rather than adding `open_reader_without_sort_check` to bismark-io (Open Decision #1 option (b)), methcons can call `BamReader::without_sort_check(BufReader::new(File::open(path)?))` directly — option (a) — **plus a 4-line local format sniff** (or just hard-require BAM, since SPEC §2.1 already says the Perl is BAM-only in practice). The SPEC's own §2.1 and §9 note SAM/CRAM input is "BAM-only in practice." So **SE can simply open BAM via `BamReader::without_sort_check`** and skip the `AnyReader` dispatch entirely.

I lean toward option (b) anyway (a symmetric `open_reader_without_sort_check` in bismark-io is small, testable, and benefits future SE-tolerant callers), but the plan should acknowledge that **option (a) needs no upstream change and is viable given the BAM-only reality** — so this need not block methcons on a bismark-io PR. **Action: pick (a) to decouple, or (b) if you want the shared helper; either way, correct the §2.4/B2 claim about the `@SO` guard.**

### 1.4 PE pairing & the dropped 100k pre-flight (SPEC §4.6, PLAN B3) — mostly CORRECT, one real desync risk (Important)

The argument that dropping `test_positional_sorting`'s 100k pre-flight is output-equivalent is **largely** watertight: a well-formed file passes both; a coordinate-sorted/interleaved file trips the per-pair qname check in the main loop and `die`s with no further output. Since error text is out-of-gate, the observable output is identical (no/partial files — see §1.6 below on partial output). I accept the drop.

**Three sub-points need attention:**

(a) **Odd trailing R1.** Perl (lines 230–253): for PE, after reading R1, it does `$_ = <IN>` to get R2. If R1 is the last line, `<IN>` returns `undef`, the `if (/XM:Z:/)` fails (undef doesn't match), so it hits the `else { warn; last; }` branch → **R1's counts are discarded, loop stops**. PLAN B3 says "If `r2` is `None` → stop (drop the dangling R1, uncounted) — mirrors Perl's `$_ = <IN>` → undef → `last`." **Correct.** Note this is **different** from dedup's `stream_pe`, which returns `UnpairedFinalRecord` *error* (pipeline.rs:268) — methcons must **not** error here; it must silently stop (like Perl's `last`). The plan correctly diverges from the sibling. Make sure the implementer doesn't copy dedup's `UnpairedFinalRecord` behavior. **Flag explicitly.**

(b) **R2 missing XM → discard R1, `last`.** Perl lines 250–253: if R2 lacks `XM:Z:`, `warn + last` → R1's already-counted `$meth_count`/`$unmeth_count` for *this iteration* are simply dropped (the `next`/classify block is never reached because `last` exits the loop). **But note:** with `bismark-io`, a record that reaches the iterator already passed `BismarkRecord::from_noodles_record`, which **requires** XM (record.rs:123). So a "missing XM" R2 would surface as a `BismarkIoError::MissingTag{tag:"XM"}` **error**, not a silent stop. **This is a divergence the plan does not address.** Perl treats missing-XM as a soft `last` (stop, keep prior buckets, write the report); Rust-via-BismarkRecord treats it as a hard error (no report, nonzero exit, possibly partial BAMs). On genuine Bismark data every record has XM, so this never fires — but it **is** a behavioral divergence in the same class as SPEC §4.1, and should be **listed in §4 as an accepted divergence** (or handled by iterating raw `RecordBuf` + `tags::xm` for the XM-presence soft-stop, if exactness is wanted). Currently §4.1 only covers the *first* R1; the R2-missing-XM and R1-missing-XM soft-`last` cases deserve the same explicit treatment. **Action: extend §4.1/§4 to cover the missing-XM soft-stop → hard-error divergence for both R1 and R2.**

(c) **Unmapped-read filtering desyncing R1/R2 adjacency.** SPEC §4.2 claims this "cannot happen on real Bismark data." I agree it cannot on **concordant-pair** Bismark BAMs (both mates mapped). But the plan should note one more guard: `bismark-io`'s `.records()` filters `FLAG & 0x4` **silently** (read.rs:586). If a pathological PE BAM had one mate unmapped, the iterator would drop it and the *next* mapped record (the following pair's R1) would be paired against the surviving mate → qname mismatch → the PLAN B3 qname check fires → error. So the failure is *caught* (not a silent wrong-output), which is the safe direction. Perl, by contrast, does NOT filter unmapped (it reads raw `samtools view` lines), so Perl would pair the unmapped mate's line (which has no XM) → R2-missing-XM → soft `last`. **So on this pathological input, Perl and Rust diverge (soft-stop vs error), but neither produces silently-wrong output.** Acceptable; document alongside (b).

### 1.5 Empty-bucket BAM (SPEC §5.2, Open Decision #6, Spike 2) — lazy→no-file is RISKY as a default; Spike is correctly scoped but should run BEFORE Phase A (Important)

Perl opens all three `samtools view -b -S -` pipes **eagerly** (lines 196–198), but only prints the header into a bucket on its **first** record (`if ($all_meth_count == 0) { print … samtools view -H }`). So for a bucket that receives **zero** records, Perl pipes **empty stdin** into `samtools view -b -S -`. What that produces is genuinely ambiguous (likely an error to stderr and a 0-byte or header-less BAM, or a BAM with just the BGZF EOF) and **samtools-version-dependent**. The plan's lazy→no-file default means **Rust produces no file where Perl produces *something*** (even if that something is a 0-byte/garbage file). If the byte-identity harness (PLAN D1) checks all three bucket files unconditionally, a Perl-emits-0-byte / Rust-emits-nothing mismatch **fails the gate** — or worse, the harness's "read both back via open_reader" step **errors** on Perl's 0-byte file and is mis-scored.

This is exactly the kind of silent gate-breaker the review should catch. **Recommendations:**
1. **Pull Spike 2 to the very start (pre-Phase-A or first thing in A)** — it's cheap and it determines a contract that A7's `LazyBucketWriters` design depends on. The PLAN's "Estimated sequencing" note already allows pulling spikes forward; make it mandatory for Spike 2.
2. The byte-identity harness (D1) must **define empty-bucket handling explicitly**: e.g. "if Perl's bucket file is absent or decodes to zero records, Rust's bucket file must be absent or decode to zero records" — i.e. compare at the **record-set** level (empty == empty) and **tolerate file-existence asymmetry**. SPEC §7 already excludes "the empty-bucket file's existence/contents (pending Spike 2)" from the gate — **good** — but D1's concrete assertions must implement that exclusion, or the test will trip. **Action: make D1 explicitly skip/normalize empty buckets.**
3. If Spike 2 finds Perl reliably emits a header-only BAM (plausible — `samtools view -b -S -` on empty input with a header already printed… but the header is NOT printed for empty buckets, so it's likely truly empty/0-byte), then lazy→no-file is the cleaner choice and the gate-exclusion stands.

### 1.6 Partial output on mid-stream error (NOT addressed — Important)

Perl's failure modes (R2 mate-name mismatch `die` at line 239; missing-XM `last`) leave the already-opened `samtools` pipes and any already-written buckets **on disk**. The Rust port's `LazyBucketWriters` (PLAN A7.4) will likewise leave partially-written BAMs if it errors mid-stream (mate mismatch). Worse: a `BamWriter` that is dropped **without `finish()`** writes its BGZF EOF only via `Drop`, which "silently swallows I/O errors" (write.rs:38-39 `#[must_use]` note) — so on the error path the bucket BAMs may be **missing their EOF marker** (truncated/un-decodable). dedup's UMI path added `cleanup_partial_output_on_err` (pipeline.rs:622) precisely for this. methcons should decide: on a fatal mid-file error (mate mismatch), does it (a) leave whatever was written (Perl-like), or (b) clean up? Either is defensible, but the plan is silent and the **`finish()`-on-every-opened-writer-even-on-error-path** requirement (so files are at least valid BAMs) must be stated. PLAN A7.6 says "`finish()` every opened writer" but only on the **success** path. **Action: specify the error-path finalization/cleanup policy; ensure no un-`finish()`ed `BamWriter` is dropped (it's `#[must_use]` — clippy/compile will nag, but the *logic* must route every opened writer through `finish()` on all paths).**

### 1.7 `BismarkRecord` strictness divergence (SPEC §4.1) — acceptable for the gate, with one caveat (Important)

`from_noodles_record` (record.rs:116-141) enforces: XR present+valid, XG present+valid, valid strand combo, **and XM.len() == seq.len()**. Perl reads **only** XM (and only its `Z/z/H/h` counts). On genuine Bismark data all three tags are present and XM length matches seq — so "no-op on real data" holds for the **acceptance tests** (synthetic fixtures built via `BamWriter` will be made well-formed, and the 10M real datasets are genuine Bismark output). I accept the strictness for the gate.

**Caveat for the 10M real-data runs (Phase D):** if *any* record in the real BAMs has an XR/XG that `BismarkStrand::from_xr_xg` rejects, or an XM/seq length mismatch (e.g. hard-clipped reads where seq is shorter — does Bismark ever emit `H` CIGAR ops?), the Rust port **errors and produces no/partial output**, while Perl sails through (it never checks XR/XG/length). That would **fail Phase D loudly** — which is the *safe* direction (caught, not silent), but could be a surprise. **Mitigation: before the formal Phase D gate, run a quick `methylation_consistency_rs` pass over the 10M BAMs and confirm zero `MissingTag`/`InvalidStrandTags`/`XmSeqLengthMismatch` errors.** Worth adding as an explicit Phase D pre-check.

### 1.8 Truncation handling (SPEC §4.6 / PLAN C2) — Perl's check is a buggy character-class; relevant to test scoping (Optional/Important)

I verified Perl line 436: `if ($_ =~ /[EOF|truncated]/)`. That is a **character class** `[EOFtruncated|]`, **not** an alternation. So Perl `die`s on **any** `samtools` stderr/header line that starts with `[` and contains *any* of the letters E,O,F,t,r,u,n,c,a,d (or `|`). I confirmed `[main] zzzzz`, `[E::some] …`, and a real `[W::bam_hdr_read] EOF…` all match. In practice samtools' first lines under `samtools view 2>&1` are warnings/errors that almost always contain one of those letters, so Perl's `bam_isTruncated` effectively **dies on the first bracketed diagnostic of any kind**. The plan (C2) maps noodles' truncated-BGZF I/O error to "a clear error (text not byte-matched)". That's fine for the gate (truncation text is out-of-gate), but the plan should note that **Perl's truncation detection is far broader (and buggier) than "EOF/truncated"** — so a "best-effort truncation test" (C2) need not try to reproduce Perl's exact trigger set. **Low priority; just don't waste effort matching Perl's character-class bug.**

### 1.9 `min_count == 0` zero gate (SPEC §2.5 step 5, PLAN A3 step 2) — CORRECT

Perl line 259: `if (meth+unmeth < min_count) → ++discarded; next`. Line 265: `if (meth+unmeth) > 0 → compute pct; else next`. So a zero-call read with `min_count == 0` passes the discard gate (`0 < 0` is false) and hits the `else { next }` → counted in **no** bucket and **not** in `discarded`. PLAN A3 step 1→2 (`total < min_count → Discard`; `total == 0 → Skip`) reproduces this exactly, in the right order. **Correct, and the order matters** (discard-gate first, then zero-gate). Good that the SPEC §8 gotcha #8 calls it out.

### 1.10 Report format (SPEC §5.1, PLAN A5) — CORRECT; one byte-precision risk

I diffed the SPEC §5.1 templates against Perl lines 334–343 character-by-character: the 49-hyphen separator, the `"Total $type records     -\t"` (5 spaces), the `[ >= $upper% ]` / `[ <= $lower% ]` / `[ ${lower}-${upper}% ]` brackets, and the `Too few CpGs   [min-count $min]` (3 spaces) / `Too few CHHs   ` variants all match. The `sprintf("%.2f", …)` percentages and the `N/A` (whole-string, rendered as `(N/A%)`) when `total==0` match dedup's proven `report.rs` pattern (which I confirmed uses `format!("{:.2}", …)` and `"N/A"`).

**Risk:** PLAN A5 says "Copy the literal format strings out of the Perl source, not by hand." **Strongly endorse** — the internal spacing is irregular and easy to fumble (e.g. `"All methylated"` + 4 spaces vs `"All unmethylated"` + 2 spaces to align the `[` at column 18; `"Mixed methylation"` + 1 space). Note the `[ ${lower_threshold}-${upper_threshold}% ]` on line 327/338 uses **no spaces around the hyphen** (`10-90`), unlike the `>= ` / `<= ` lines which have a space. SPEC §5.1 line 134 renders this as `[ <lower>-<upper>% ]` — correct. **Action: the A5 unit test must assert the FULL multi-line string byte-for-byte (the plan says it does); make sure the fixture includes both the `total>0` and `total==0`/`N/A` and CHH-label variants, and that the percentages are computed `bucket as f64 / total as f64 * 100.0` (same op order as Perl lines 311–314).**

---

## 2. Assumptions — surfaced & validated

| # | Assumption (plan) | Verdict |
|---|---|---|
| A | `detect_paired_from_header` returns `Option<bool>`, `None`⇒SE for methcons | **TRUE** (read.rs:649). Methcons maps `None→SE` (PLAN B1) ≠ dedup's `None→error` (main.rs:376). Correct. |
| B | `open_reader` always rejects coordinate-sort | **TRUE** (read.rs:562→from_path→new→check). No `open_reader_without_sort_check` exists. |
| C | `BismarkRecord::from_noodles_record` enforces XR/XG + XM-length | **TRUE** (record.rs:117-130). Stricter than Perl. |
| D | `.records()` silently drops unmapped (FLAG&0x4) | **TRUE** (read.rs:586). |
| E | `BamWriter` writes header eagerly at construction | **TRUE** (write.rs:59-62). Implication: **lazy** bucket creation is required to avoid header-only empty files — PLAN A7.4 gets this right. |
| F | `finish()` mandatory for BGZF EOF | **TRUE** (write.rs:84; `#[must_use]`). Must finish on **all** paths (see §1.6). |
| G | A `without_sort_check` path is reachable through `open_reader` | **FALSE** — only via the concrete `BamReader::without_sort_check(reader)` taking a `BufRead`, not via `open_reader`. The plan (Open Decision #1) already knows this. |
| H | dedup's `filename.rs` is the pattern to mirror | **PARTIALLY MISLEADING** — see §3.1. dedup **strips the directory** (`s/.*\///`, filename.rs:63) and writes to `--output_dir`. Methcons Perl does **NOT** strip the directory (only `s/\.bam$//` on the full path, line 186) and writes **next to the input**. The plan (SPEC §2.7, PLAN A4) correctly states "keep directory + rest verbatim" — but A4's "mirror `bismark-dedup/src/filename.rs` style" is the wrong sibling pattern. **Critical — see §3.1.** |
| I | PE counts **pairs** not records (`total`/buckets ++ once per pair, 2× BAM records) | **TRUE** (Perl lines 276/286/296 increment once; `print … $read1` + `print` R2 write two). SPEC §8 #7 + PLAN B3 correct. |
| J | `samtools_path` accepted-and-ignored is harmless | **TRUE** for output. Perl uses it only for I/O subprocess; noodles replaces that. No output effect. |
| K | Real-data needs no genome (methcons reads only the BAM) | **TRUE** — Perl only ever reads `XM:Z:` + qname; no reference. |

---

## 3. Critical correctness gaps

### 3.1 Output-filename directory handling — DO NOT mirror dedup's `filename.rs` (Critical)

This is the single biggest implementation-trap. **dedup** derives the stem with `s/.*\///` (basename only, filename.rs:63) and joins to `--output_dir` (default `.`), so dedup writes to the **current directory**. **methcons Perl** does:
```perl
my $file_root = $file;          # full path, incl. directory
$file_root =~ s/\.bam$//;       # strip ONLY trailing .bam, directory KEPT
open(METH, "… > \"${file_root}…_all_meth.bam\"");   # writes NEXT TO INPUT
```
So for input `/data/sample.bam`, Perl writes `/data/sample_all_meth.bam` etc. — **into the input's directory, with the directory prefix preserved.** If the Rust port mirrors dedup's `derive_output_stem` (basename-strip + `output_dir`), it will write `./sample_all_meth.bam` into the CWD — **wrong location, and the byte-identity harness (which compares files at the Perl-produced paths) will not find them / will mismatch.**

The SPEC §2.7 and PLAN A4 text are **correct** ("keep directory + rest verbatim", "strip a single trailing `.bam`"). The bug risk is purely the PLAN A4 instruction to "mirror `bismark-dedup/src/filename.rs` style" — which is the wrong sibling behavior. **Action: PLAN A4 must explicitly say "do NOT use dedup's basename-stripping `derive_output_stem`; methcons keeps the full input path minus the trailing `.bam`." Add a unit test: `output_root("/data/sub/x.bam") == "/data/sub/x"` and `bucket_path(...) == "/data/sub/x_all_meth.bam"`.** Also note there is **no `--output_dir`** flag in the Perl (and none should be added for v1.0 byte-identity).

Edge: Perl `s/\.bam$//` strips only `.bam`. If the input is `sample.BAM` (uppercase) or `sample.sam`, Perl does **not** strip it, so `$file_root` retains the extension and outputs become `sample.BAM_all_meth.bam`. Real Bismark inputs are `.bam`, so this is academic, but the unit test should pin "only lowercase `.bam`, only once" (PLAN A4 already says "single trailing `.bam`"; add the case-sensitivity note).

### 3.2 Spike 1's confidence rationale is factually wrong (Critical-to-fix-text, see §1.1)

Not a logic bug in the algorithm, but a wrong justification in the SPEC that could cause Spike 1 to be under-scoped and miss the round-half-to-even ties that *are* reachable. Fix the rationale and add power-of-two-total ties to both the spike grid and the A3 unit tests.

### 3.3 Empty-bucket gate handling must be made explicit in the harness (Critical-if-unaddressed, see §1.5)

If D1 unconditionally reads back all three bucket BAMs, a Perl-vs-Rust file-existence asymmetry for empty buckets will break the test (or error the read-back). Must be normalized.

---

## 4. Efficiency analysis

The algorithm is O(n) over records, single pass, O(read-length) per record for the `tr`/byte-count. Memory is O(1) (streaming; three writers + four counters). This is appropriate and matches Perl's complexity. No scaling concerns at 10M reads.

Minor notes:
- **`count_xm` (A3):** a single pass counting `Z`+`z` (or `H`+`h`) is correct and fast. Use a straight byte loop or two `bytecount`/`iter().filter().count()` passes; either is fine. No need for SIMD.
- **Lazy writers (A7.4):** correct and necessary (avoids header-only empty files given eager-header `BamWriter`). Cost: an `Option<BamWriter>` check per record — negligible.
- **v1.0 single-threaded** (SPEC §9) matches dedup's correctness-first stance. `ThreadedBam*` deferral is reasonable; methcons is even less I/O-bound than extractor since it only re-emits a subset of records. Fine.
- **No `--quiet` perf concern.** STDERR diagnostics are negligible.

One real concern: **PLAN A7.2 "peek first record" empty-check.** dedup uses `reader.records().peekable()` then `peek()` (pipeline.rs:313). For methcons SE via `BamReader::without_sort_check`, the same `peekable().peek()` works. **But** note Perl's `bam_isEmpty` (lines 395–420) checks emptiness by reading the **first `samtools view` line — which includes unmapped reads** (no filter). The Rust `.records()` peek sees only **mapped** records (unmapped filtered). So a BAM containing **only unmapped reads** would be "non-empty" to Perl (it reads a line) but "empty" to Rust (peek is `None`). Perl would then proceed, write headers lazily on first mapped record (never), and produce a report with `total==0` (all `N/A`) and **possibly empty/0-byte bucket files**. Rust would treat it as empty → skip file → **no outputs at all**. **This is a divergence** (Perl: report-with-N/A + maybe-empty-buckets; Rust: nothing). Real Bismark BAMs always contain mapped reads, so academic — but it's in the same "documented divergence" class and should be listed. **Action: add to §4 divergences: "all-unmapped input → Perl produces an N/A report; Rust skips the file."**

---

## 5. Validation sufficiency

**What the plan validates well:**
- Classification boundaries incl. round-then-compare (A3) — strong, *if* the tie cases from §1.1 are added.
- Report byte-exactness incl. `N/A` and CHH label (A5).
- PE pairing, auto-detect table incl. `None→SE` (B4).
- Real-data record-level + report byte-identity for SE/PE/CHH (D1).

**Highest-risk paths that could SILENTLY produce wrong output and slip through:**

1. **f64 expression-order drift in `classify` (highest risk).** If the implementer writes the percentage computation in any order other than Perl's `meth / (meth+unmeth) * 100`, ties can flip a read's bucket with **no test catching it** unless the test grid includes the exact tie ratios. The synthetic A3 tests use hand-picked values; **they must include the power-of-two-total ties and the 90.05/10.05 boundary ties** (§1.1), or a subtle reorder ships silently. **Mitigation: add those exact tie cases; pin the expression in a code comment.**

2. **Output directory (§3.1).** A wrong-directory bug would be caught by D1 *only if* D1 compares files at the Perl-produced (input-adjacent) paths. If D1 instead points both tools at a temp CWD and compares basenames, the directory bug hides until production. **Mitigation: D1 must invoke both tools on the *same absolute input path* and compare the *input-adjacent* output files (SPEC §7 already says "same path arg" — make D1 assert the input-directory location).**

3. **Empty/edge buckets (§1.5).** Covered above.

4. **Header round-trip fidelity (SPEC §8 #3).** The header is parsed+re-serialized by noodles, not byte-copied. dedup validates this against Perl, so it's "solved" — **but methcons writes the **same** header into up to three files**, and Perl's `samtools view -H` header may differ subtly from noodles' re-serialization (tag ordering, `@PG` of samtools itself). Since the gate is at the **decoded record + decoded header content** level (SPEC §7), and dedup already proves noodles round-trips Bismark headers, this is low risk — **but D1's header comparison should compare the *parsed* header (reference sequences, etc.), not raw `@`-line bytes**, because `samtools view -H` (Perl) vs noodles may order/spell header lines differently while being semantically identical. **Confirm D1 compares parsed header equality, not raw header bytes.** (SPEC §7 says "same header content" — make "content" mean parsed, not byte.)

5. **Multiple-input-files independence (PLAN C2).** Perl loops files independently; a fatal `die` on file 2 (mate mismatch) aborts the **whole run**, but files already fully processed (file 1) keep their outputs. Rust must replicate: process each file; on a fatal error, stop (nonzero exit) but leave completed files' outputs intact. The `for input in &config.files` loop (dedup main.rs:122) does this. **But** the empty-file case: Perl `return`s (skip) and **continues** to the next file (line 146). Rust must **skip-and-continue** on empty, **abort** on fatal. PLAN C2 states this correctly; **add a test: [empty.bam, good.bam] → empty skipped, good.bam fully processed.**

**Gaps to add:**
- Tie-ratio classification tests (§1.1) — **must-have**.
- Output-directory location test (§3.1) — **must-have**.
- Error-path writer finalization (no truncated BAM left) test (§1.6).
- All-unmapped input divergence — document + (optional) test.
- Multi-file: empty-then-good, and good-then-fatal (file1 outputs survive).

---

## 6. Alternatives & trade-offs

1. **SE reader (Open Decision #1):** Given SPEC's BAM-only reality (§2.1), **option (a)** — `BamReader::without_sort_check(BufReader::new(File::open(path)?))` directly in methcons — is the lowest-coupling choice and needs **zero** bismark-io changes, unblocking methcons from an upstream PR. Option (b) (add `open_reader_without_sort_check`) is nicer long-term but couples the schedule. **Recommend (a) for v1.0; file (b) as a follow-up if/when a second SE-tolerant caller appears.** Either way, **PE can keep using `open_reader`** (its coordinate-sort rejection is output-equivalent to Perl's qname-adjacency die).

2. **PE pairing (Open Decision #3):** **Manual qname-equality is correct, not `BismarkPair::from_mates`.** I verified `from_mates` (pair.rs:40-50) additionally requires `r1.read_identity()==R1` and `r2.read_identity()==R2` (FLAG 0x40/0x80). Perl checks **only qname** (`$id1 eq $id2`, line 238) — it ignores FLAG bits entirely. A Bismark PE BAM where, say, both mates lack the 0x40/0x80 bits (unusual but Perl wouldn't care) would `die` under `from_mates` but pass under Perl. Manual qname compare is faithful. **Recommend manual** (matches the plan). Also: Perl's `die` message is `"READ IDs of R1 ($id1) and R2 ($id2) did not match…"` — the error text is out-of-gate, so any clear message is fine.
   - *Subtle:* Perl's main-loop qname check (line 238) does **NOT** do the `/1`,`/2` suffix stripping that `test_positional_sorting` does (lines 494–497). So the main loop requires **exact** qname equality. Modern Bismark doesn't append `/1`,`/2`, so this never bites — but the manual compare should be **exact** (no suffix stripping), matching the main loop, not the pre-flight. **Pin this.**

3. **`BismarkRecord` strictness (Open Decision #2):** **Accept strictness (use `BismarkRecord`).** The leniency-via-raw-`RecordBuf`+`tags::xm` alternative buys nothing for the gate (real data is well-formed) and loses the reuse + the safety net that *catches* malformed input loudly. The only case where leniency would matter is the missing-XM soft-`last` (§1.4b) — and reproducing Perl's soft-stop there is **not** worth abandoning `BismarkRecord`; just **document** the divergence. **Recommend strict + documented (matches plan).**

4. **`--samtools_path` (Open Decision #4):** **Accept-and-ignore** (matches plan). Validating-to-mirror-Perl's-`die` would *add* a failure mode Rust doesn't need (noodles needs no samtools) and could **diverge** if the user's `samtools` path is bogus — Perl would `die`, Rust-accept-ignore would succeed. Since this is a non-output behavior and the gate excludes exit-on-bad-samtools, accept-and-ignore is cleaner. (dedup also silently ignores it, cli.rs:230.) **Recommend accept-and-ignore.**

5. **Binary name (Open Decision #5):** **`methylation_consistency_rs`** (matches dedup's `deduplicate_bismark_rs`). The hyphenated `methylation-consistency-rs` (extractor style) is inconsistent with the closest sibling. **Recommend `methylation_consistency_rs`.** (Cosmetic; not gate-relevant.)

6. **Empty-bucket (Open Decision #6):** **lazy → no file**, *conditional on Spike 2* confirming Perl emits nothing-meaningful, AND with D1 normalizing empty-bucket comparison (§1.5). If Spike 2 finds Perl emits a header-only valid BAM, reconsider — but eager header writing would then re-introduce the "header-only file for buckets that get records later" problem; the clean fix would be "lazy, but if a bucket ends empty, optionally write a header-only BAM to match." Decide post-spike. **Recommend lazy → no file + D1 normalization, pending Spike 2.**

---

## 7. Phasing assessment

A (SE end-to-end) → B (PE) → C (CHH + edges + spikes) → D (real-data gate) is **sensible** and mirrors dedup's proven ordering. Concerns:

- **Pull Spike 2 (empty-bucket) to the front of A (or pre-A).** A7.4's `LazyBucketWriters` contract depends on the empty-bucket decision; designing it before the spike resolves the contract risks rework. (Spike 1 can stay in C, but is cheap enough to pull forward too — and resolving the tie-rationale (§1.1) early de-risks A3's test design.) The plan's "Estimated sequencing" note already flags this as optional; **make Spike 2 pre-A mandatory.**
- **Phase A delivers "the entire algorithm except PE."** Good — SE is the bulk. But A7.1's reader-construction (the no-sort-check SE path, Open Decision #1) is a prerequisite for A to even run; resolve Open Decision #1 **before** A starts, not during.
- **No missing phase.** Docs/CI in D are appropriate.
- **One sequencing nit:** the f64 expression-order pin (§1.1) and the output-directory pin (§3.1) are A-phase concerns (classify.rs, filename.rs) but are currently buried in spikes/decisions — surface them as explicit A acceptance criteria.

---

## 8. Action items (prioritized)

### Critical (fix before implementation)
- **C1 — Output directory.** PLAN A4 must NOT mirror dedup's basename-stripping `derive_output_stem`. methcons keeps the **full input path minus trailing `.bam`** and writes outputs **adjacent to the input** (no `--output_dir`). Add unit tests pinning directory preservation. (§3.1, Assumption H)
- **C2 — f64 expression order + tie tests.** Pin the percentage computation to Perl's exact order `meth as f64 / (meth+unmeth) as f64 * 100.0` (with integer denominator sum) in code + comment. Add A3 unit tests for round-half-to-even ties reachable from real ratios: `1/16→6.2`, `3/16→18.8`, and the boundary ties `201/2000→10.1→mixed`, `1801/2000→90.0→meth`. Correct SPEC §8 Spike-1's false "ties unreachable" rationale and re-scope the spike grid to include power-of-two totals. (§1.1, §3.2)
- **C3 — Empty-bucket gate normalization.** Pull Spike 2 to pre-Phase-A. Make PLAN D1 explicitly tolerate empty-bucket file-existence asymmetry (compare at record-set level; empty==empty), so Perl-emits-0-byte vs Rust-emits-nothing does not break/​error the harness. (§1.5)
- **C4 — SE reader path.** Resolve Open Decision #1 **before** Phase A. Recommend option (a) (`BamReader::without_sort_check` directly, BAM-only) to avoid blocking on a bismark-io PR; keep `open_reader` for PE. Correct the SPEC §2.4 / PLAN B2 claim that Perl's `@SO`/`@HD SO:coordinate` guard is what protects PE — it does **not** (Perl's `/^\@SO/` regex is dead code; the real protection is the per-pair qname die). (§1.3)

### Important
- **I1 — Missing-XM soft-stop divergence.** Extend SPEC §4 to document that Perl's missing-XM `last` (soft stop, keep prior buckets, write report) becomes a hard `BismarkIoError::MissingTag` error in Rust (for R1 first-record AND R2). Decide whether to accept (recommended, document) or reproduce via raw-`RecordBuf` iteration. (§1.4b)
- **I2 — Odd-trailing-R1 / PE soft-stop ≠ dedup error.** PLAN B3 must explicitly NOT reuse dedup's `UnpairedFinalRecord` error; the trailing R1 is silently dropped and the loop stops (Perl `last`). Pin exact (non-suffix-stripped) qname equality for the mate check. (§1.4a, §6.2)
- **I3 — Error-path writer finalization.** Specify that every opened bucket `BamWriter` is routed through `finish()` on ALL paths (incl. mid-file error), so no un-`finish()`ed (EOF-less, un-decodable) BAM is left on disk. Decide leave-vs-cleanup of partial outputs on fatal error. (§1.6)
- **I4 — D1 must invoke both tools on the same absolute input path and compare input-adjacent outputs**, and compare **parsed** header content (not raw `@`-line bytes). Add multi-file tests: [empty, good] (empty skipped, good processed) and [good, fatal] (good's outputs survive). (§5.2, §5.4, §5.5)
- **I5 — All-unmapped-input divergence.** Document in §4: Perl (reads unmapped lines) → non-empty → N/A report; Rust (`.records()` filters unmapped) → empty → skip file. Academic on real data; list it. (§4)

### Optional
- **O1 — Truncation check.** Note Perl's `/[EOF|truncated]/` is a buggy character class that dies on essentially any bracketed samtools diagnostic; the Rust "best-effort truncation error" need not match Perl's trigger set (text is out-of-gate). (§1.8)
- **O2 — Binary name** `methylation_consistency_rs` (dedup style). (§6.5)
- **O3 — `--samtools_path` accept-and-ignore** (matches plan; cleaner than validate-and-die). (§6.4)
- **O4 — Phase-D pre-check:** run `methylation_consistency_rs` over the 10M BAMs first to confirm zero `MissingTag`/`InvalidStrandTags`/`XmSeqLengthMismatch` errors before the formal gate. (§1.7)
- **O5 — Spike 1** may be pulled forward with Spike 2 to de-risk A3 test design (cheap). (§7)

---

## 9. Bottom line

The plan's **algorithm and most divergence analysis are correct** — notably the round-then-compare contract (which I verified is decision-identical), the `None→SE` fallback (correctly distinguished from dedup), and the BAM-I/O reuse claims. The approach is sound and the phasing is reasonable. Before implementation, fix the four Critical items — chiefly the **output-directory behavior** (do not mirror dedup's basename-strip), the **f64-expression-order + round-half-to-even tie tests** (the SPEC's "ties unreachable" rationale is empirically false), the **empty-bucket gate normalization**, and the **SE no-sort-check reader decision** (plus correcting the inaccurate `@SO`-guard claim). With those addressed, byte-identity is achievable.

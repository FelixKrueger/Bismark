# SPEC Review A — `bismark-extractor`

**Reviewer:** A (independent, fresh context)
**Target:** `rust/bismark-extractor/SPEC.md` (683 lines, rev 0, 2026-05-26)
**Reference Perl:** `bismark_methylation_extractor` v0.25.1 (6,050 LOC)

## Verdict: **NEEDS-REVISIONS**

Two critical correctness issues invalidate the design as written:
1. **§6.5 reverse-strand XM/CIGAR reversal claim is factually wrong** vs Perl — and it isn't decoration; M-bias positions for `-` strand records will be reversed end-to-end relative to Perl.
2. **§7.4 overlap-filter comparator is off-by-one** vs Perl (`>` vs `>=`).

There are also structural issues: a duplicate §8 / §9 (lines 589-628 repeat earlier content), an unresolved CIGAR-after-ignore start-shift requirement, and a real-data gate that uses sorted-md5 — which **masks** several of the bugs the design intends to prevent. Fix the §6.5 + §7.4 + duplicate-section + sorted-md5 issues before approving.

---

## Critical findings

### C1. §6.5 — Perl DOES reverse XM and CIGAR for `-` strand reads; the SPEC says otherwise

**SPEC line 207:** "The XM tag is already stored as-aligned by Bismark itself."

**Perl lines 1619-1621:**
```perl
if ($strand eq '-') {
    $meth_call = reverse $meth_call;
}
```
**Perl lines 2880-2882:**
```perl
if (@comp_cigar){
    @comp_cigar = reverse @comp_cigar;
}
```

For reverse-strand reads, Perl reverses the XM string and the expanded CIGAR before iterating. It then walks indices `$start + $index + $pos_offset` (or `$start - $index + $pos_offset`) — meaning the M-bias `$index + 1` position 1 corresponds to the **reference-5'-most** position of the read, which after reversal is the **read's 3' end** of the original sequencing read.

In BAM, the SEQ for `is_reverse_complemented` records is reverse-complemented relative to reference, so the XM tag (written in SEQ orientation by Bismark) is too. Without reversal in the Rust extractor:

- **M-bias position 1 for `-` strand reads will be the original read 5' end**, not Perl's reference-5' end.
- This is **exactly the byte-identity drift** the design promises to prevent.

This is a **structural design bug**, not a nit. Fix options:
- (a) Bismark-extractor performs the reversal explicitly when `record_strand` ∈ {CTOT, OB} (or use noodles' `Flag::REVERSE_COMPLEMENTED`). Mirror Perl precisely.
- (b) `bismark-io` returns an XM accessor that returns reference-aligned orientation (`xm_ref_aligned()`), and the extractor consumes that. This relocates the responsibility but doesn't eliminate it — SPEC §6.5's "MUST NOT reverse" must then specify "because the reader returns ref-aligned XM."

Pick one explicitly. The current text — "reader's responsibility" — is contradicted by the actual `bismark-io::BismarkRecord::xm()` implementation, which returns the raw tag bytes (see `rust/bismark-io/src/record.rs:166-169`).

**Action:** rewrite §6.5 with the actual reversal contract. Add a unit test that takes a `-` strand record with known XM and verifies M-bias position 1 corresponds to the reference-5' base.

---

### C2. §7.4 — overlap-filter comparator is `>` but Perl uses `>=`

**SPEC line 335:**
```rust
r2_calls.into_iter().filter(|c| c.ref_pos > r1_ref_end).collect()
```

**Perl line 2905:**
```perl
if ($start+$index+$pos_offset >= $end_read_1) {
    return;   # drop all remaining R2 calls
}
```

Where `$end_read_1 = $start_read_1 + $MDN_count_1 - 1` (Perl line 2400) — that is, **inclusive 1-based last reference base of R1**. `CigarExt::reference_end` is also inclusive 1-based (`rust/bismark-io/src/cigar.rs:269-272`). The Rust comparator therefore must be `>=` (i.e., drop R2 call when `ref_pos >= r1_ref_end`), not `>`.

Same for the reverse branch (SPEC line 339): `c.ref_pos < r1_ref_start` should be `c.ref_pos <= r1_ref_start` (Perl line 2987 uses `<=`).

This is a one-character bug that will produce one extra R2 call per overlapping pair — a byte-identity violation invisible to sorted-md5 only because the diff is small but **definitely** visible to byte-equal comparison of the unsorted output.

**Action:** fix the comparators in §7.4 and add a unit test for "R2 call exactly at R1's reference_end is dropped."

---

### C3. §7.4 uses `filter()` over already-emitted Vec; Perl `return`s from the function

Perl drops the entire tail of the R2 read once overlap starts (it `return`s from the per-pair function, skipping all subsequent positions including their **M-bias accumulation**). SPEC §7.4 filters AFTER `extract_calls` runs to completion — so the dropped-overlap positions still increment the M-bias counters via `route_call`.

This is a second M-bias bug: Perl does NOT accumulate M-bias for dropped-overlap R2 positions; SPEC's design does, because M-bias is accumulated in `route_call` (§7.5) after `drop_overlap` has filtered.

Wait — re-reading §7.3 main loop: `drop_overlap` runs before `for call in r2_calls: route_call(...)`. OK so M-bias is only accumulated post-filter. Good.

BUT the `filter()` semantics still differ from Perl's `return`: Perl stops at the first overlap index. For forward strand the calls are emitted in ascending ref_pos, so `return` ≡ `filter > end`. For reverse strand Perl walks with `$start - $index` (descending), so `return` ≡ "first call with ref_pos ≤ end (which is start_read_1 here)" — but **subsequent calls in the Vec have smaller ref_pos and ALSO satisfy `≤ end`** , so `filter()` still works. OK, semantically equivalent given the call-order invariant. Document the invariant explicitly: `extract_calls`'s output is in CIGAR-walk order, which for `-` strand reads (after the C1 reversal fix) is descending ref_pos.

**Action:** add an invariant comment to `extract_calls` that the output Vec is in monotonic ref-position order per the CIGAR walk direction, and reference this from `drop_overlap`.

---

### C4. §8.3 real-data gate uses **sorted-md5**, masking the bugs above

**SPEC line 528, 610:**
> Each of 12 split files sorted-md5 equal: `gzcat <rust_split> \| sort \| md5 == gzcat <perl_split> \| sort \| md5`

Sorting the lines destroys row-order information. Perl emits methylation calls **in BAM read order within each per-strand file** (since it processes records sequentially and writes per call). Sorted-md5 will agree even if the Rust port (a) emits calls in wrong order, (b) routes one record's calls to two strand files (Alan's exact bug!), or (c) emits the off-by-one extra R2 overlap call.

The dedup port's sorted-md5 gate is appropriate for dedup because the dedup decision is set-membership (sort then compare). The extractor's per-call output is **stream-ordered**; the test surface must reflect that.

**Action:** strengthen §8.3 to also assert **unsorted byte equality** (or row-prefix line-by-line equality with diff snippet on mismatch) on the per-strand split files. Sorted-md5 stays as a fast smoke check; unsorted is the real gate.

---

### C5. Duplicate §8 / §9 in the SPEC (lines 589-628 repeat earlier content)

The SPEC ships two §8 ("Test surface") and two §9 ("Parallelism model"). The duplicates are LESS detailed than the earlier sections (lines 473-547, 548-587). Looks like a draft-merge artifact. **Strip the duplicates** before any further review.

---

## Important findings

### I1. §6.2 — M-bias `[MbiasTable; 2]` with SE at index 0 — Perl-faithful but document the asymmetry

Perl writes M-bias for SE records into the same `%mbias_1` hash it uses for PE R1 (lines 2913-2914 etc. always increment `$mbias_1` or `$mbias_2` based on `$read_identity`, with SE callers passing `1`). SPEC matches this. Good.

But: the resulting `M-bias.txt` has **3 sections for SE** (only the index-0 contexts are non-empty) and **6 for PE**. The writer must check whether index 1 has any data — for SE input it will be empty. SPEC §10 Phase D "M-bias writer" should note this dispatch explicitly. As written the SPEC says "6 sections (PE) or 3 (SE)" (§4.2) but doesn't tie this to the data structure.

**Action:** add an explicit sentence to §6.2: "The writer emits a section only if the table is non-empty; SE mode never populates index 1, so its M-bias.txt has 3 sections."

### I2. §7.1 — `--ignore` semantics omit the start-position adjustment for `+` strand

Perl applies `--ignore N` by **(a) trimming N from the meth_call, (b) trimming N CIGAR ops from the start, and (c) shifting `$start` by `N + D_count + N_count - I_count`** (Perl lines 1648-1673). Failing to shift `$start` would emit calls at the wrong reference position.

SPEC §7.1 just does `lo = ignore_5p; hi = seq_len - ignore_3p` as read-coordinate bounds, and the CIGAR walker advances normally. **This is fine for emitting calls at correct ref_pos**, because the CIGAR walker still tracks both read_pos and ref_pos correctly, and the boundary check just suppresses output for the first N read positions. BUT it produces a different effective `start` for the read — which is consequential for **`--no_overlap` overlap detection**, because Perl's overlap detection uses the post-ignore-adjusted `$start_read_1`.

So: when `--ignore N` is combined with `--no_overlap`, the R1 ref_start used for overlap drop must account for the ignored 5' positions on `+` strand and the ignored 3' positions on `-` strand. SPEC §7.4 reads `pair.r1().alignment_start()` directly — that's the raw BAM start, NOT the post-ignore start.

**Action:** add §7.4 note: when `--ignore` or `--ignore_3prime` is active, overlap bounds must be computed from the **effective** R1 reference span (post-ignore), matching Perl's adjusted `$start_read_1` / `$end_read_1`. Add a fixture: `--ignore 5 --no_overlap` PE with R1 overlapping R2 in the trimmed region.

### I3. §6.4 — output collector reordering claims memory bound `40N`; doesn't bound on producer backlog

§9.4: "at most `N × 32 + N × 8 = 40N` entries in flight." This counts in-channel records. The `BTreeMap<u64, WorkerOutput>` reorder buffer, however, can grow **unboundedly** under worker skew — if worker 3 produces input_idx 999 while worker 1 is still on input_idx 0, the BTreeMap holds 999 entries waiting for index 0.

Realistic skew on a BGZF stream is small, but a chromosome boundary or a very-CIGAR-complex region could cause it. With N=8 and a per-record `WorkerOutput` of ~1 KB (`Vec<MethCall>` + `MbiasDelta`), 100K-entry backlog = 100 MB. Tolerable, but the SPEC should state a **collector watermark**: producer back-pressures when reorder buffer exceeds, say, `N × 1024` entries.

**Action:** §9.2 should add: "collector enforces a watermark on its reorder buffer; if `BTreeMap.len() > N × 1024` the collector signals the producer to pause."

### I4. §9.3 M-bias merge — claims commutativity but ignores `Vec::resize_with` alignment

Each worker grows its `Vec<MbiasPos>` lazily as it sees per-position calls. To merge worker tables, lengths must be aligned — extend the shorter Vec to the longer Vec's length with `MbiasPos::default()` before summing. The SPEC says "summed position-wise" but doesn't address the resize. Trivial in implementation but should be noted explicitly to avoid an off-by-one in the merge.

**Action:** §9.3 should add: "merge step first extends each worker's per-context Vec to the global max length with `MbiasPos::default()`, then sums."

### I5. §8.4 missing test: directional library → only OT/OB populated

Alan's port emitted CTOT/CTOB files for directional data that should only have OT/OB. SPEC §8.4 lists "Read at chromosome start" + "Soft-clipped boundary" but NOT a "directional library produces only OT/OB and CTOT/CTOB are empty/absent" fixture. This is the **exact failure mode the design claims to prevent**.

**Action:** add a §8.4 fixture: "Directional-library PE BAM (no CTOT/CTOB reads): assert CTOT/CTOB output files are byte-empty after the header" (Perl writes the header even for empty files).

### I6. §11 — `--genome_folder` rejection without explicit value

SPEC: "Reject without explicit value when `--cytosine_report` is set." Fine, BUT also need to validate **before launching the subprocess**: directory exists, contains `.fa`/`.fasta`, and (if FAI required by `coverage2cytosine`) `.fai` is present. SPEC doesn't say where this happens. Add to §6.6 or a new §6.7.

**Action:** add a §6 subsection on subprocess-precondition validation: FASTA exists, .fai exists if required, output dir writable, `bismark2bedGraph` / `coverage2cytosine` resolvable on `PATH` or relative to Rust binary.

### I7. §6.6 — Where does the Rust binary find `bismark2bedGraph`?

Perl uses `$RealBin/bismark2bedGraph` (the script's directory, line 377). The Rust port is in `rust/target/release/`, which is **NOT** colocated with the Perl scripts. The SPEC says "subprocess to Perl `bismark2bedGraph`" but doesn't address resolution.

Options:
- (a) Require `bismark2bedGraph` on `PATH` (user installs Bismark properly).
- (b) Pass it as `--bismark_path /path/to/bismark/dir`.
- (c) Search common locations + the Rust binary's parent dir.

**Action:** pick (a) and document. Add a `which("bismark2bedGraph")` check at CLI-resolve time with a clear "install Bismark" error message.

### I8. §10 Phase G "subprocess chain ~400 LOC" feels high

A `std::process::Command::new("bismark2bedGraph").args(...).output()?` is ~30 LOC. Even with stderr capture, error propagation, `--ucsc` post-processing of bedgraph output, and `chromosome_sizes.txt` generation, 400 LOC is overestimated unless the phase also includes the precondition validation (I6) and the path resolution (I7). Either trim the estimate or list what's inside.

**Action:** §10 row G should list sub-items: "subprocess invocation, args wiring (cutoff, output_dir, gzip, ucsc, zero_based), error propagation, FASTA precondition validation, `chromosome_sizes.txt` writer." 400 LOC is plausible only if all of these are bundled.

---

## Minor findings / nits

### N1. §3 flag table row 4 — `--fasta` "variable never read in 6050 LOC"

Confirmed by grepping the Perl: `$genomic_fasta` is declared (line 33) and assigned by GetOptions but never used downstream. SPEC is right. Keep the accepted-no-op + deprecation warning.

### N2. §3 row 27 — `--samtools_path`: "Accepted-no-op in Rust port"

Confirm against `bismark-dedup`'s precedent — yes, dedup accepts it but does nothing with it. OK.

### N3. §7.5 — `pos = call.read_pos + 1` for M-bias

Matches Perl `$index + 1` (lines 2913, 2995). Correct **assuming C1 is fixed** — i.e., assuming `call.read_pos` is in Perl-equivalent (post-reverse) read-coordinate space.

### N4. §10 Phase B size — splitting_report skeleton in Phase B

Phase B at ~800 LOC bundles "core SE extraction loop + XM routing + output-file map + splitting_report skeleton." Compare to dedup's Phase D, which was dedicated to the report. The extractor's `_splitting_report.txt` is more complex than dedup's (parameter summary + per-context counts + first-occurrence examples). Splitting it into its own phase would mirror dedup more closely and avoid a 800-LOC PR.

**Optional:** consider a Phase B' for splitting_report between B and C.

### N5. §11 — auto-trigger of `--bedGraph` when `--cytosine_report`

Perl line 387-388 confirms `--cytosine_report` auto-engages bedGraph processing. SPEC §3 row 21 says "Auto-triggers `--bedGraph`" — correct. Make sure §6.6 reflects this: subprocess `bismark2bedGraph` then chain `coverage2cytosine` with the `.bismark.cov.gz` output of the first.

### N6. §3 row 33 — `--parallel` vs `--multicore`

Both alias to the same Perl variable. SPEC correctly notes this. Make sure the Rust `clap` definition uses one as canonical and the other as alias (the dedup `--parallel`/`--multicore` precedent applies).

### N7. §8.4 — "Mixed SE+PE in same BAM: currently undefined; either auto-detect per-record or reject"

Perl behavior should be checked. From session memory, Bismark always tags `@PG` with the mode it ran in. If a user mixes BAMs and the @PG line says PE but the records aren't paired, Perl will likely produce nonsense. **Recommendation: reject with a clear error**, matching dedup's stance.

**Action:** decide before Phase A. "Reject" is the safer default.

### N8. §11 — auto-detection of SE/PE — "reuse bismark-dedup's pattern"

`bismark-dedup` uses `bismark-io::detect_paired_from_header`. Check the SPEC says "reuse" explicitly, and verify the function is `pub` in `bismark-io` (or needs hoisting). Add to §11 as a concrete decision: "Yes, use `bismark-io::detect_paired_from_header`."

### N9. §3 row 22 — hardcoded mouse default for `--genome_folder`

Reject-without-explicit-value is the right call. Add to §8.1: `cli_validate_rejects_cytosine_report_without_genome_folder`.

### N10. §3 — `--no_header` is listed but Phase B's "splitting_report skeleton" and other writers must consult it

Plumb `no_header` into `ExtractState`. SPEC's struct (line 444-453) doesn't explicitly list it; add to the struct or document that `ResolvedConfig` is also threaded.

---

## Summary of action items (prioritized)

### Critical (block approval)
1. Fix §6.5: document the actual XM/CIGAR reversal contract for `-` strand reads. Pick option (a) extractor reverses, or (b) `bismark-io` returns ref-aligned XM.
2. Fix §7.4 comparators: `>=` for forward, `<=` for reverse, matching Perl.
3. Fix §8.3 real-data gate: add **unsorted** byte-equality on split files. Sorted-md5 alone masks the bugs the design intends to prevent.
4. Strip duplicate §8 / §9 sections (lines 589-628).

### Important (must address before Phase A)
5. §6.2: writer skips empty mbias sections; document SE → only index 0.
6. §7.4 + §7.1: overlap detection must use **post-ignore-adjusted** R1 reference span.
7. §9.2: add collector reorder-buffer watermark for back-pressure.
8. §9.3: explicitly document the per-Vec resize-then-sum merge step.
9. §8.4: add directional-library "CTOT/CTOB empty" fixture.
10. §6.6: subprocess precondition validation (FASTA exists, .fai if required, output dir writable).
11. §6.6: subprocess resolution — `which("bismark2bedGraph")` on PATH; clear install-Bismark error.

### Optional / nits
12. Phase B → consider splitting `_splitting_report.txt` into its own Phase B'.
13. §11 decision: reject mixed SE+PE in same BAM.
14. §11 decision: confirm `bismark-io::detect_paired_from_header` is reused.
15. Add `no_header` to `ExtractState` or document the `ResolvedConfig` thread.

---

## Spot-check audit of cited Perl lines

| SPEC cite | Perl line | Verified |
|-----------|-----------|----------|
| §3 row 1 → Perl 959 (`--help`) | line 989-993 range plausible (Perl GetOptions block) | not exact line, but block matches |
| §6.1 → Perl 2891-2906 (`drop_overlap`) | confirmed at 2890-2906 | YES |
| §6.5 → Perl 1619-1621 (CIGAR reversal) | confirmed at 1619-1621 (XM reversal!) | YES — but SPEC mislabels as "CIGAR reversal"; it's the XM reversal site |
| §6.5 → Perl 2877-2886 (CIGAR reversal) | confirmed at 2877-2886 | YES |
| §7.4 → Perl 2891-2906 + 2976-2990 | confirmed | YES |
| §7.5 → Perl 2821-2822 | confirmed (read_identity die) | YES |
| §8.1 mutex → Perl 1037-1038 | confirmed | YES |
| §12 → Perl 30, 294-304 (global `%fhs`) | not spot-checked here, but plausible | unverified |

**Notable:** the §6.5 citations are correct, but the SPEC's prose describing what those lines DO is wrong (they reverse the meth_call/XM, not just CIGAR for output formatting). See C1.

---

## Reviewer's read on the design overall

The structural pitfalls catalog (§12) is excellent — it correctly identifies the bug classes that hit Alan's port and maps them to design choices. The argument-struct approach (§6.3), per-pair strand (§6.1), and explicit context iteration (§6.2) are all genuine structural preventions, not relocations.

But §6.5 is **the prevention that doesn't actually prevent** — it asserts a property of `bismark-io::xm()` that the reader doesn't provide, and the resulting design will fail the very M-bias byte-identity it promises. Combined with the sorted-md5 gate, this would ship a port that passes CI and fails on first byte-equal comparison.

Fix C1 + C2 + C4 and the design is sound. The remaining items are tightening, not redesigns.

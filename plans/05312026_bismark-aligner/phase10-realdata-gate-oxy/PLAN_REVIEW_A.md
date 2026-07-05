# Plan Review A — Phase 10 (full-scale real-data gate on oxy)

- **Reviewer:** A (independent)
- **Date:** 2026-06-03
- **Plan reviewed:** `phase10-realdata-gate-oxy/PLAN.md` (rev 0)
- **Verdict:** Methodologically sound and feasible. The hybrid oracle is the right call and the
  order-vs-content split is internally consistent. I found **one factual error** in the plan
  (the `--ambig_bam` comparison mechanism is misattributed) and **several validation-sufficiency
  gaps** where the gate could PASS while masking a real divergence. None are fatal; all are fixable
  in the harness/procedure before the run. Details below, cited by section.

---

## Logic review

**The central split is correct (§2 "central tension", §3.1).** Strict *ordering* is an algorithmic
property already proven at 1M (9b, coprime N) and re-proven at 10M (Gate A); full scale tests
*content* + chromosome diversity + edge cases, which is order-independent and therefore validly
compared against a reordered Perl `--parallel P` BAM. This is sound. The plan correctly identifies
that a strict full-scale byte-identity diff would need a fresh single-core Perl 84M run (tens of
hours, recycle-risk) and reasonably defers that.

**Logic gap L1 — the `--ambig_bam` comparison is misattributed (factual error).** §3.3, §5, and V3
all state the `--ambig_bam` is compared "with the raw `noodles_bam` reader path **as the 9b harness
already does**." The 9b harness (`phase9b_worker_invariance_gate.sh:51,79–89`) does **no such
thing** — it compares *every* `*.bam` (main and ambig alike) with `filter_sam() { samtools view -h
... }`, i.e. via **samtools**, not a noodles reader. The raw-`noodles_bam`-reader requirement was a
fix in the **production merge code** (`merge_bams` in `parallel.rs`, because `bismark_io::BamReader`
validates XR/XG/XM and rejects tagless ambig records) — it is a property of the *binary under test*,
not of the *comparison harness*. The harness reads the already-written `.bam` with samtools, which
has no such tag-validation. This conflation matters for two reasons:
  1. The Gate B procedure (§3.4, step 5) will sort the *text* output of `samtools view` for the ambig
     BAM. That is fine — but the plan's stated rationale ("raw noodles reader, no XR/XG/XM") is the
     wrong reason, and an implementer following §5 literally may try to build a noodles-based
     comparator that is unnecessary and not what 9b did.
  2. More importantly: a tagless ambig record fed to `samtools view` is still a valid SAM line, so
     samtools-based comparison works. **But** confirm `samtools view` does not choke on or silently
     reformat tagless records at scale. (Low risk — 9b proved it at 1M — but the plan should cite the
     9b harness *as it actually is*, not as a noodles reader.)
  → **Fix:** correct §3.3/§5/V3 to say the comparison is `samtools view`-based (as 9b actually does);
     the raw-noodles-reader caveat applies to the *production merge path*, already shipped/proven.

**Logic gap L2 — Gate B's primary oracle is freshly regenerated, but the plan never states the
Perl `--parallel P` value used to make the FRESH oracle must equal the value used for the cross-check
BAM, nor that B2 compares Rust-vs-fresh-Perl (same env) as PRIMARY and Rust-vs-old-`--parallel-4`-BAM
as SECONDARY.** §3.1 "Oracle provenance" says the fresh Perl `--parallel P` run is primary and the
old `--parallel 4` BAMs are a cross-check. Good. But V9 ("Cross-check vs pre-existing `--parallel 4`
BAM") and step 5 ("Also cross-check Rust content vs the pre-existing `--parallel 4` BAM") leave it
ambiguous whether a **fresh-vs-old Perl/Perl** sanity comparison is also done. The strongest
corroboration of the load-bearing assumption (§7) would be **Perl-fresh-`--parallel P` content ==
Perl-old-`--parallel 4` content** (both Perl, different worker counts → if these two multisets match,
that *directly* proves Perl `--multicore` is multiset-invariant across worker counts on real
84M data — the exact assumption Gate B rests on). The plan only compares **Rust-vs-old** and
**Rust-vs-fresh**, never **Perl-vs-Perl**. See Validation Sufficiency below — this is the single
highest-value addition.

**Logic gap L3 — RRBS oracle has no `--parallel-4` cross-check (correctly), but its fresh Perl
oracle and Rust both run at `--parallel P`, so RRBS's content gate has NO order-independent Perl
self-consistency check at all.** For WGBS the old `--parallel 4` BAM gives a second Perl data point;
RRBS regenerates from scratch (§3.1, "For RRBS always regenerate") with a single Perl run. If RRBS
stays hybrid (likely, O1), its full-scale content gate is Rust-`pP` vs one Perl-`pP` run — a single
oracle, no Perl cross-check. The 10M-strict RRBS subset (Gate A) covers it against single-core Perl,
which is the real protection; but flag that RRBS full-scale leans entirely on the Gate-A subset for
order/oracle confidence. Acceptable if Gate A RRBS runs; risky if RRBS is large and Gate A RRBS is
skipped.

**Logic gap L4 — V11 (genomic-seq-extraction-failure count) is checked only via the REPORT count,
not per-record.** §8 V11 verifies the report's "discarded because genomic sequence could not be
extracted" count is identical (SE oracle shows 36). That is a *count*, which B1 (report identity)
already guarantees byte-for-byte. So V11 is **subsumed by B1** and adds no new assurance — it does
NOT verify that the *same 36 reads* were discarded (only that *36* were). If Rust discards a
*different* set of 36 reads (same count, different reads), B1 passes, V11 passes, **and** the main
BAM multiset differs by ≤36 records on each side → B2's sort+md5 *would* catch it (the discarded
reads are absent from the BAM). So the safety net is B2, not V11. V11 as written is redundant; the
real edge-case probe is B2's multiset equality. Either reframe V11 as "the discarded reads are the
same set (implied by B2 main-BAM multiset match)" or note it is a B1-derived consistency check, not
an independent probe.

**Logic gap L5 — no explicit check that read COUNTS match before content hashing.** B1 compares the
report (which includes total-sequences-analysed), so a gross read-count mismatch is caught. But the
plan never asserts that `samtools view -c` on Rust == Perl for the main BAM as a cheap pre-gate
*before* the 21–42 GB sort. A record-count mismatch is the cheapest possible signal of drift (one
extra/missing alignment) and should run between B1 and B2. The plan implies it (B1 covers counts) but
should make it explicit as a guard so a sort isn't launched on mismatched inputs.

---

## Assumptions

**A1 (the load-bearing one, §7 bullet 2): "Perl `--multicore`/`--parallel P` produces the same
multiset of record lines as single-core, only reordered."** The plan corroborates this three ways:
B1 report identity, the 9b in-order proof, and V9 (`--parallel 4` cross-check). Scrutinizing whether
all three could *jointly* miss a divergence:

  - **B1 (report identity)** compares *aggregate counts*. A per-read divergence that preserves all
    counts (e.g. a read that maps to position X under single-core but position Y under `--multicore`
    *with the same MAPQ/strand classification* — possible if a chunk-boundary read's Bowtie2
    pseudo-random tie-break differs because Bowtie2 seeds per-read-content, NOT per-read-*position*)
    would pass B1. **However** — and this is the key insight that rescues the gate — Bowtie2 seeds its
    multimap tie-break from the *read sequence + name*, NOT from the read's ordinal position in the
    file or its chunk assignment (SPEC §3 "pseudo-random but seeded per read → reproducible"; 9b
    GATE_OXY explicitly proved per-read content seeding). So a read produces the *same* alignment
    regardless of which chunk it lands in. This is exactly why 9b's contiguous-chunk model is
    byte-identical to single-core. The assumption A1 is therefore *true for the same reason 9b's gate
    passed* — and 9b proved it at N=1,000,003 with a straddled boundary. **But note the subtlety:**
    9b proved *Rust contiguous chunks* == *Perl single-core*. It did NOT directly prove *Perl
    fork+modulo `--multicore`* == *Perl single-core* (Perl was always run single-core in 9b — see
    harness line 66, no `--parallel`). Perl's fork+modulo assigns reads to chunks by `read_index %
    P`, a *different* partition than Rust's contiguous chunks, but **the same per-read-content-seeding
    argument applies to Perl too** — each Perl child runs single-threaded Bowtie2 on its stripe, and
    each read's alignment depends only on its sequence, not its stripe. So A1 holds by the same
    mechanism. The plan's reasoning is correct but **never spells out the per-read-content-seeding
    mechanism as the reason A1 holds** — it leans on "alignment is per-read independent" (true) plus
    three empirical corroborations. Recommend adding the mechanistic justification (Bowtie2 seeds from
    read content, proven in 9b) so a reader doesn't have to take A1 on faith.
  - **The genuine residual risk:** a read at a *chunk seam* that Perl's `--multicore` *drops or
    duplicates* (off-by-one at the modulo split, or a final partial chunk). If `--multicore` silently
    lost 1 read in a corner, single-core would have it → multiset differs by 1 → **B2's sort+md5
    catches it** (one extra line on the single-core/Rust side). And B1 catches it via the
    total-sequences count. So drop/dup at seams IS caught by B1+B2 — *provided the fresh Perl oracle
    is the `--parallel P` run* (which has the same potential seam bug as the old `--parallel 4` BAM).
    The one scenario all three miss: Perl `--multicore` has a seam bug that drops read R, AND Rust's
    contiguous-chunk merge *also* drops read R (a shared bug). That is implausible (different
    partition schemes) and would also have shown at 9b's straddled boundary. **Conclusion: A1's
    corroboration is sufficient, but the Perl-fresh-vs-Perl-old cross-check (L2) would make it
    airtight** by directly observing two different Perl worker counts producing the same multiset.

**A2 (§7): "The pre-existing `full_size` Perl BAMs are v0.25.1 + samtools 1.23.1 + Bowtie 2 2.5.5
(`@PG` confirms first two; Bowtie 2 version inferred from pinned env)."** This is the weakest stated
assumption and the plan acknowledges it. The Bowtie 2 version of the *old* BAM is genuinely unknown
(not in `@PG`). The mitigation — "verified by the fresh Perl `--parallel P` content cross-check
matching" — is circular-ish: IF fresh-Perl (known Bowtie2 2.5.5) content == old-BAM content, then the
old BAM was made with a Bowtie2 that produces identical alignments (effectively 2.5.5 or a
byte-compatible version). That is actually a *sound* inference and is the right way to retire the
unknown. But it means **V9 (Rust vs old `--parallel 4`) is NOT the right comparison to retire A2** —
the right one is **fresh-Perl vs old-BAM** (Perl/Perl, controls for any Rust-side issue). As written,
if V9 fails, you can't tell whether Rust diverged or the old BAM used a different Bowtie2. Splitting
into Perl-fresh-vs-old (retires A2) and Rust-vs-Perl-fresh (the real gate) disambiguates. See L2.

**A3 (§7): "`full_size` SE = 83,985,631 reads."** Sourced from its `_SE_report.txt`. Fine, but the
plan should re-derive it at run time (`zcat | wc -l`/4) for the SE input too, not just PE/RRBS
(§4 step 2 only measures PE + RRBS). A stale/mismatched report count vs the actual fq.gz would
silently skew the perf framing and the B1 baseline. Cheap; add SE to the measure step.

**A4 (implicit, §3.5 / §7 last bullet): "`/var/tmp` survives a single gate run (no recycle
mid-run)."** Explicitly flagged as not guaranteed, mitigated by detach + per-cell off-box capture +
idempotent re-run (O3). Reasonable. But note: an 84M PE alignment at P=16 could be *hours* (§5
parallelism budget), and the perf B4 numbers are *invalidated* if a recycle forces a mid-cell
restart (partial run + restart skews wall-clock). The idempotent re-run protects *correctness* but
NOT *perf measurement* — a recycled perf cell must be re-run clean from scratch, not resumed. Make
that explicit in O3/B4.

**A5 (implicit): the genome indexes on oxy (`genome/` GRCh38, `RRBS_PE/genome/` GRCm39) were built
by the same genome-prep (Perl or Rust?) that both Perl-bismark and Rust-bismark_rs will consume.**
The gate compares aligners, not genome-prep; both sides MUST read the *identical* `BS_CT`/`BS_GA`
index + FASTA, or a divergence is attributable to the index, not the aligner. The plan assumes shared
inputs (§6 "Reads: ... GRCh38 + GRCm39") but never asserts "both Perl and Rust point at the same
`--genome` dir." The harness does (9b uses one `$GENOME` for all runs) — but for RRBS the plan says
"own `genome/` + built index" and "regenerate" the BAM; confirm the RRBS index is present and
identical for both sides. Add an explicit V0: "Perl and Rust consume the same `--genome` path per
cell."

---

## Efficiency analysis

**E1 — Gate B sort+md5 reasoning is correct.** Two independently `sort`-ed streams hash-equal **iff**
they are the same multiset, regardless of locale, *provided both sides use the identical `sort`
invocation* (same `LC_ALL`, same field/byte ordering). The plan must pin `LC_ALL=C` (or identical
locale) on both `sort` calls — otherwise a locale difference between two invocations on the same box
is a non-issue (same env), but if the harness ever runs the two sides under different shells/locales
the total order diverges and md5 differs *even for equal multisets*. Low risk on one box, but
**pin `LC_ALL=C sort`** explicitly for robustness and reproducibility. (The reasoning "both sides
independently sorted → identical iff multisets equal" holds only under a *deterministic, identical*
total order, which `LC_ALL=C` guarantees.)

**E2 — `sort -S 50% --parallel=N -T /var/tmp` on ~21 GB (SE) / ~42 GB (PE) is feasible** given 678 G
free `/var/tmp` and 128 cores. `-S 50%` of pod RAM is the right ballpark; external merge-sort spills
to `-T /var/tmp` which has ample room. Two concerns:
  - **`-S 50%` is 50% of *physical RAM*** — on a 128-core pod RAM is likely large, but if two sorts
    run concurrently (Rust side + Perl side) each grabbing 50% → OOM. Run the two sorts
    **sequentially** (the plan runs cells sequentially §5, but does not say the two *within-cell*
    sorts are sequential). Make the two sorts sequential or set `-S 25%`.
  - **Disk headroom:** at peak you hold, per cell: 2 BAMs (Rust + fresh Perl, ~5.5 G + ~5.5 G SE / ~11
    G ×2 PE) + the old cross-check BAM + 2× decompressed-sorted SAM text spill (sort tempfiles can be
    ~1–1.5× input) + the GRCh38 index copy. PE worst case ≈ 11+11+11 (BAMs) + ~42+42 (sort temp) ≈
    160 G+, well under 678 G. Fine, but the plan's "comfortably holds" (§3.4) is worth a back-of-
    envelope line so a reader trusts it. Add the headroom arithmetic.

**E3 — md5 of a 21–42 GB sorted stream is single-threaded and slow** (~minutes), but trivial next to
the sort and the alignment. Acceptable. A faster commutative per-line hash (mentioned in §3.4 as an
optional pre-check) is a nice-to-have, not needed.

**E4 — Gate A streaming `cmp` is the right fix for `diff`-buffering at 10M.** Correct call. One note:
`cmp` exits at the *first* byte difference and prints byte offset — useless for diagnosis on its own,
so the plan's "on mismatch, `diff` a bounded window" (§3.4) is necessary. Ensure the bounded-window
diff seeks near the `cmp`-reported offset, not from file start (a `diff` from start re-buffers). Spell
out the diagnosis path (e.g. `sed -n` a window around the offset, or `cmp -l | head`).

**E5 — Perf comparison fairness (B4, §1.4, O2).** Matched P=16 for both Perl `--multicore 16` and
Rust `--parallel 16`, same staged inputs, same box, sequential cells. This is fair *for wall-clock
scaling*. Two fairness traps:
  - **Bowtie2 thread count per instance.** Both Perl and Rust run single-threaded-per-instance
    Bowtie2 (the byte-identity constraint). At P=16 directional that's 2×16=32 Bowtie2 processes on
    128 cores — fair on both sides *only if both spawn the same instance topology*. Perl
    `--multicore 16` forks 16 pipelines × 2 instances = 32; Rust `--parallel 16` = 16 chunks × 2 = 32.
    Symmetric. Good — but state it, so the perf number isn't questioned.
  - **I/O staging (O4).** The plan stages inputs off the S3 FUSE mount to `/var/tmp` for the timed
    runs — essential, else mount jitter pollutes wall-clock asymmetrically. Make O4 a *requirement*
    for B4 cells, not an open "assumption: yes." Both sides must read from `/var/tmp`-staged inputs.

**E6 — building on oxy (§4 step 1).** `cargo build --release` of the workspace member is ~23 s (9b
precedent). Copying the binary to `/home` against recycle (§4 step 1) is a good touch. Fine.

---

## Validation sufficiency

This is the crux for a gate plan. Could Gate B PASS while the port is wrong?

**VS1 — Multiset equality (B2 sort+md5) catches any per-line difference.** Correct. Any wrong field
on any record changes that line's bytes → the sorted stream differs → md5 differs. A wrong field "on
a small fraction of reads in a way that preserves counts" still changes those lines → caught. So B2
is robust against per-record field errors. ✅

**VS2 — Could order-normalization mask a real ORDERING bug that only manifests at full scale?** Yes,
in principle — Gate B is order-blind by construction. The plan defers strict ordering to Gate A
(10M) + 9b (1M coprime). The question (per the kickoff focus): **is 10M enough head-room over
real-data ordering pathologies?** Analysis:
  - The ordering invariant is *algorithmic*: Rust emits records in input-read order (contiguous
    chunks merged in chunk order); this is independent of read content or scale. A bug here would be a
    merge-ordering bug (chunk boundaries mis-stitched), which 9b's coprime-N=1,000,003 straddled-
    boundary test was *specifically designed* to expose, and it passed. 10M (Gate A) re-exercises it
    on real chromosome diversity. **The ordering bug class is scale-invariant** — it triggers at *any*
    N that straddles a chunk boundary, which 1M and 10M both do. So 10M *is* enough head-room: an
    ordering bug does not "wake up" only at 84M; it wakes up at the first straddled boundary.
  - **The one full-scale-only ordering risk** the plan does NOT address: at P=16 on 84M, chunk sizes
    are ~5.25M reads — *larger than any chunk tested at 10M/P (≤10M) or 1M/P*. If the merge has a
    per-chunk *counter/offset overflow or buffer-size* bug that only triggers when a single chunk
    exceeds (say) some 32-bit or buffer threshold, full scale would expose it but 10M would not. This
    is a *capacity* bug, not an *ordering* bug, and Gate B's content gate WOULD catch a resulting
    record loss/corruption (B1 count + B2 multiset). But a *reordering-only* capacity bug (records all
    present, mis-ordered within a giant chunk) would slip both Gate A (smaller chunks) and Gate B
    (order-blind). **Mitigation:** Gate A at 10M with P sized so a chunk ≈ the full-scale chunk size
    is impossible (10M < 84M). Recommend Gate A also run **one cell at P=16 on the full 10M** (chunk ≈
    625k) AND note that the 84M/P chunk (~5.25M) is genuinely larger than anything ordering-tested.
    The residual is small (a reordering-only-at-huge-chunk bug with no record loss is exotic), but the
    plan should *name* it as the one ordering risk Gate B cannot see, rather than implying full
    coverage. **Optionally**: run Gate A's worker-invariance leg (Rust pP vs p1) at the *same P* used
    for Gate B, so the merge-ordering path under test is the literal Gate-B configuration at 10M.

**VS3 — B1 (report identity) is necessary but the plan should assert it runs on the FRESH same-env
Perl, not the old BAM** (the old BAM's report may not be present/comparable, and was made at
`--parallel 4`). §3.1 says B1 is "the alignment report ... Perl and Rust reports must be byte-
identical modulo wall-clock." Confirm the fresh Perl `--parallel P` run emits a report and that's the
B1 comparand. (The old `--parallel 4` BAM directory may or may not retain its report.) Minor; spell
out.

**VS4 — Aux identity (B3) at full scale.** `--unmapped`/`--ambiguous` are large FastQ.gz at 84M.
Sort-normalized decompressed comparison is correct (order differs by worker count). Same sort+md5
machinery. The plan covers this (§3.1 B3, §3.4). One gap: the **`--unmapped` records have no
canonical sort key** — they're raw FastQ (4-line records). Sorting *lines* of a FastQ breaks the
4-line record grouping → a line-sorted FastQ is NOT a multiset of *records*, and two different
record-multisets could collide to the same line-multiset (pathological but possible: same set of
seq/qual lines, different pairing). **Fix:** for FastQ aux, collapse each 4-line record to one line
(`paste - - - -`) BEFORE sort+md5, so the unit of comparison is the *record*, not the line. The 9b
harness compared aux with `diff <(zcat) <(zcat)` (order-preserved, so line grouping was implicit) —
at full scale order differs, so you MUST record-ize before sorting. **This is a real harness bug the
plan would otherwise ship.** (Applies to B3 `--unmapped`/`--ambiguous`; `--ambig_bam` is one
record-per-line via `samtools view`, so it's fine.)

**VS5 — Header comparison (B2).** `@HD`/`@SQ` compared with `cmp` after `@PG` filter — correct, and
`@SQ` is where GRCh38/GRCm39 scaffold coverage (alt contigs, unplaced) actually gets validated
byte-for-byte. Good. Note: `@SQ` order is genome-prep-determined and identical for both sides (same
index) → byte-identical, not just multiset. ✅ This is the one place the "full chromosome diversity"
goal (§1) is *directly* gated; everything else gates it indirectly (a read mapping to a rare scaffold
shows up in the record multiset). Worth calling out that `@SQ` byte-identity is the explicit scaffold-
coverage gate.

**VS6 — The "full chromosome diversity" justification for Phase 10 (§1) is only partially gated.**
The plan's stated *new information* is full scaffold coverage + rare CIGARs + the genomic-seq-
extraction path. Of these:
  - Rare scaffolds: gated via `@SQ` byte-identity (VS5) + any read mapping there appearing in the B2
    multiset. ✅
  - Rare CIGARs / coordinate-limit positions (very long chromosomes, POS near 2^31): a read aligning
    near a chromosome-end with a long deletion/soft-clip produces a rare CIGAR. Gate B's multiset
    catches a *wrong* CIGAR (line differs). But neither gate *targets* the coordinate-limit edge
    (chr1 is ~249 Mb < 2^31, so no 32-bit POS overflow on GRCh38 — but the plan should note POS fits
    in i32/i64 as noodles uses, so this is not a real risk; *say so* rather than leave it implied).
  - Genomic-seq-extraction failure (the "could not be extracted" path): the SE oracle shows 36 such
    discards. Gate B's main-BAM multiset (those reads absent) + B1 count cover it; V11 is redundant
    (see L4). ✅ but reframe V11.

**VS7 — Cells coverage (kickoff focus 5).** "Realistic cells only" (SE-dir, PE-dir, mouse RRBS
PE-dir) leaves non-directional/pbat *uncovered at full scale*. The plan justifies this: they land ~0
reads on directional libraries, and are covered by 1M per-phase + 9b gates. This is **defensible** —
non-dir/pbat differ from directional only in the 4-instance topology + wrong-strand rejection, which
is content-logic proven byte-identical at 1M (9b) and 10k. Full scale adds chromosome diversity, but
non-dir/pbat traverse the *same* genome/scaffolds as the directional cells already do at full scale —
so the scaffold-coverage *new information* is not re-gained by running non-dir at 84M. **Accept.** The
one residual: a non-dir-specific code path (wrong-strand rejection counter, 4-instance merge seam)
that only mis-behaves at a chunk boundary unique to 4-instance mode at large chunk size — but 9b
proved 4-instance worker-invariance at coprime 1M, same scale-invariance argument as VS2. Accept, but
the plan should state explicitly "non-dir/pbat full-scale adds no *new* scaffold coverage beyond the
directional full-scale cells (same genome traversed)" as the justification, rather than only "they
land ~0 reads on directional libraries" (which is about *test data*, not coverage logic).

**VS8 — RRBS strict-vs-hybrid adaptive (O1).** Acceptable. The decision rule (hybrid unless
< ~20M, then strict full) is sound; measure-first (§4 step 2) is right. The only risk: if RRBS turns
out, say, 25M, it falls to hybrid and its full-scale gate is content-only against a single fresh Perl
oracle (no Perl cross-check, L3) + a 10M strict subset. That is the *weakest* cell. Acceptable for a
secondary-genome diversity check; flag it as the lowest-confidence cell in `GATE_OXY.md`.

**Bottom line on validation sufficiency:** the gate is sound and B2's multiset equality is the
backbone that catches per-record errors. The **two real harness bugs that would let a wrong port (or
a flawed gate) slip** are: **VS4** (FastQ aux must be record-ized before sorting — currently would
sort raw lines and could mask a real aux divergence) and **L1/the `--ambig_bam` misattribution**
(cosmetic-but-could-mislead-implementation). The **highest-value addition** is **L2** (a
Perl-fresh-vs-Perl-old self-consistency check that directly validates the load-bearing A1 assumption,
rather than corroborating it only indirectly).

---

## Alternatives

**ALT1 — Direct multiset comparison without full sort (counting bloom / `sort -u` + count, or
`comm`).** The sort+md5 is fine, but an alternative that also *localizes* a mismatch: `sort` both
sides, then `comm -3 <(a) <(b) | head` shows the *differing lines* directly (not just "md5 differs").
At full scale `comm` streams (no buffering) and gives instant diagnosis. Consider `comm -3` as the
*primary* (it both gates AND diagnoses) with md5 as a fast confirmatory. The plan's "on mismatch,
re-diff" is harder at 42 GB; `comm -3` after the sort you already did is nearly free.

**ALT2 — Skip the old `--parallel 4` cross-check entirely; rely on Perl-fresh single-source +
Perl-fresh-vs-Perl-old (L2).** If you add the Perl/Perl self-consistency check (L2), the old BAM's
value is fully captured there and V9 (Rust-vs-old) becomes redundant with V6/V7 (Rust-vs-fresh).
Streamlines the matrix.

**ALT3 — A small *strict* full-scale run on ONE cheap cell.** Rather than deferring strict-full
entirely, consider running Perl **single-core** on RRBS *if* RRBS is small (the O1 path already does
this). That gives one strict-full data point on real data, materially strengthening the "ordering
holds at scale" claim beyond 10M for at least one genome. The plan already allows this via O1 —
elevate it from "if feasible" to "preferred, and the RRBS cell is the designated strict-full
candidate precisely because it's the smallest." (Already mostly there; just make it the explicit
intent.)

**ALT4 — Capture per-cell md5 + record-count + report into a machine-checkable results file**, not
just prose in `GATE_OXY.md`, so a re-run after a recycle can `diff` against the prior cell's captured
numbers (idempotency O3 becomes verifiable, not just "re-run the cell"). Minor.

---

## Action items (prioritized)

### Critical (fix before running the gate — would let a wrong result slip or mislead implementation)
- **C1 (VS4):** For FastQ aux (`--unmapped`/`--ambiguous`) in Gate B, **collapse each 4-line record
  to a single line (`paste - - - -`) before `sort`+`md5sum`.** Sorting raw FastQ lines breaks record
  grouping → the comparison is not a record-multiset and can mask a real divergence. (The 9b harness
  got away with line-`diff` only because order was preserved; at full scale order differs.) Fix §3.1
  B3 / §3.4.
- **C2 (L1):** Correct §3.3 / §5 / V3: the `--ambig_bam` is compared via **`samtools view`** (as the
  9b harness actually does, `phase9b_worker_invariance_gate.sh:51,79`), **not** a raw `noodles_bam`
  reader. The raw-noodles-reader caveat applies to the *production merge path* (already shipped/
  proven), not the gate harness. Prevents an implementer building an unnecessary noodles comparator.
- **C3 (E1):** Pin **`LC_ALL=C`** on every `sort` in Gate B so the total order is deterministic and
  identical across both sides; the "sorted-iff-multiset-equal" guarantee requires an identical total
  order. Add to §3.4.

### Important (materially strengthens the gate / fairness)
- **I1 (L2 / A2):** Add a **Perl-fresh-`--parallel P` vs Perl-old-`--parallel 4` content
  cross-check** (Perl/Perl) for WGBS SE+PE. This directly validates the load-bearing A1 assumption
  ("`--multicore` is multiset-invariant across worker counts") AND retires the unknown-Bowtie2-version
  A2 in one comparison, instead of inferring both indirectly via Rust-vs-old. Reframe V9 accordingly.
- **I2 (VS2):** Name the **one ordering risk Gate B cannot see** — a reordering-only bug that triggers
  only at the full-scale chunk size (~5.25M reads/chunk at P=16/84M), larger than any chunk
  ordering-tested at 10M/1M. Mitigate by running Gate A's worker-invariance leg (Rust pP vs p1) at the
  **same P used for Gate B** on the full 10M, and state the residual explicitly rather than implying
  full ordering coverage.
- **I3 (L5):** Add an explicit **`samtools view -c` record-count pre-gate** (Rust vs fresh Perl main
  BAM) between B1 and B2 — the cheapest drift signal, run before launching the 21–42 GB sort.
- **I4 (E2 / E5):** Make the two within-cell sorts **sequential** (or `-S 25%`) to avoid concurrent
  50%-RAM OOM; promote O4 (stage inputs off S3 FUSE to `/var/tmp`) from "assumption: yes" to a
  **requirement** for all timed B4 cells; state both Perl and Rust spawn the identical 2P-instance
  topology so the perf comparison is unimpeachable. Add the disk-headroom arithmetic to §3.4.
- **I5 (A4):** State in O3/B4 that a **recycle mid-perf-cell invalidates that cell's wall-clock/RSS**
  → the perf cell must be re-run **clean from scratch**, not resumed (idempotency protects correctness,
  not timing).
- **I6 (A5 / V0):** Add **V0: "Perl and Rust consume the identical `--genome` path (same
  `BS_CT`/`BS_GA` index + FASTA) per cell"** — especially for RRBS (confirm the GRCm39 index exists
  and is shared), so any divergence is attributable to the aligner, not the index.

### Optional (polish / efficiency / diagnosis)
- **O-opt1 (L4 / V11):** Reframe V11 — the genomic-seq-extraction-failure *count* is already covered
  by B1 (report identity); the *same-reads-discarded* property is covered by B2's main-BAM multiset.
  V11 as written adds nothing independent; note it as a B1-derived consistency check.
- **O-opt2 (ALT1):** Use `comm -3 <(sort a) <(sort b) | head` as the Gate B primary (gates AND
  localizes a mismatch in one pass) with md5 as the fast confirmatory; cheaper diagnosis than re-diff
  at 42 GB.
- **O-opt3 (A3):** Add SE to the run-time read-count measure step (§4 step 2 only measures PE+RRBS);
  don't trust the stored `_SE_report.txt` count blindly for the perf baseline.
- **O-opt4 (E4):** Spell out the Gate-A `cmp`-mismatch diagnosis path (seek a window around the
  `cmp`-reported byte offset; don't re-`diff` from file start).
- **O-opt5 (VS7):** State the non-dir/pbat full-scale-exclusion justification as "they traverse the
  same genome/scaffolds as the directional full-scale cells → no *new* scaffold coverage" (coverage
  logic), not only "land ~0 reads on directional libraries" (test-data argument).
- **O-opt6 (VS5):** Call out explicitly that `@SQ` byte-identity is *the* direct scaffold-coverage
  gate (the rest is indirect via the record multiset).
- **O-opt7 (ALT4):** Capture per-cell md5/count/report into a machine-checkable results stanza so a
  post-recycle re-run is verifiably idempotent.
- **O-opt8 (doc):** §2 references `PHASE10_KICKOFF_PROMPT.md` (per SESSION_HANDOFF) but it is absent
  from the phase dir — minor doc drift, not load-bearing.

---

## Summary

The plan is a well-reasoned, feasible validation design. The hybrid oracle and the order-vs-content
split are correct and the per-read-content-seeding mechanism (proven in 9b) genuinely makes the
load-bearing A1 assumption true, not merely plausible. B2's sort+md5 multiset equality is a sound
backbone that catches any per-record field error. The two issues that could let a wrong result or a
flawed gate slip are **C1** (FastQ aux must be record-ized before sorting — a real harness bug) and
**C3** (`LC_ALL=C` for deterministic sort order); **C2** is a factual misattribution that could
misdirect implementation. The single highest-value *addition* is **I1** (a Perl-fresh-vs-Perl-old
self-consistency check) which validates the load-bearing assumption directly instead of three times
indirectly. With C1–C3 fixed and I1–I6 folded in, the gate will reliably catch a regression and is
robust on the ephemeral pod.

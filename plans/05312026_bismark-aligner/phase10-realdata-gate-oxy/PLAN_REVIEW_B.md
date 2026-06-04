# Phase 10 — Plan Review B (independent)

**Reviewer:** B · **Date:** 2026-06-03 · **Plan:** `phase10-realdata-gate-oxy/PLAN.md` (rev 0)
**Verdict:** Methodologically sound and well-grounded; the order-vs-content split is correct and the
ephemeral-pod operational discipline is good. But there are **two Critical gaps that can let the gate
silently PASS while wrong** (an unpinned `sort` locale, and the absence of a record-count assertion on
the sort-then-hash path), plus several Important holes in the header comparison, the cross-check oracle's
real value, and resumability. None of these are showstoppers — they are tightenings to the gate
*definition*, which is exactly what must be airtight in a final acceptance phase.

This is a validation plan, so I judged it as a test design: *will it catch a real regression, and can it
PASS only when the port is actually correct?*

---

## 1. Logic review

**The central split is internally consistent and I agree with it.** §2 (central tension) + §3.1 correctly
separate (a) *ordering* — an algorithmic property already proven at N=1,000,003 coprime (9b) and re-proven
at 10M (Gate A), from (b) *content* — order-independent, validly compared against Perl `--parallel P`.
The reasoning that Perl `--multicore` emits the same per-read record multiset as single-core (per-read
alignment independence) is sound and is the same property 9b proved in-order. No logical flaw in the
architecture.

**L1 — Gate B's multiset comparison has no record-count guard, so two genuinely different multisets can
both PASS.** (§3.4, §8 V6/V7/V8.) `sort | md5sum` proves *set-with-multiplicity* equality of the lines
that survive the pipeline — but if a bug **drops or duplicates** records *and the report B1 check happens
to also be wrong or insensitive to that specific drop*, the md5 path alone won't independently catch it,
because there is no explicit "Perl record count == Rust record count == report-implied count" assertion
before/around the hash. The 9b harness implicitly got this for free (in-order `diff` fails on any
length delta). The plan should add, as a hard gate line: `samtools view -c` on each side **must be equal**
AND must equal the count derived from the alignment report (mapped + the relevant categories). Right now
V5 (report identity) and V6 (content md5) are presented as independent checks; make the **count
reconciliation explicit and three-way** (report ⇄ Perl BAM ⇄ Rust BAM) so a count drift cannot hide
behind a coincidentally-matching report.

**L2 — `cmp` for Gate A is correct, but the fallback to `diff` "a bounded window" is underspecified.**
(§3.4.) `cmp` reports only the first differing byte/line offset. On a real divergence at 10M, the plan
says "`diff` a bounded window for diagnosis" — but it does not say *how* the window is located from
`cmp`'s byte offset, nor that the window must be taken from the **decompressed, @PG-filtered** stream
(not the raw file). This is a diagnosis-ergonomics gap, not a correctness gap, but at 10M a naive
`diff` of the full stream is exactly the buffering hazard the plan is trying to avoid — so "bounded
window" needs a concrete recipe (e.g. `cmp` → line number → `sed -n 'start,endp'` on both filtered
streams). Otherwise the on-FAIL path re-introduces the OOM risk it claims to have removed.

**L3 — The "report-identity runs first / fail fast" ordering (§3.1 B1) is good, but B1 alone is not a
sufficient content gate and the plan should say so explicitly.** The Bismark alignment report is a
*count summary*. Two different alignment outcomes can produce **identical counts** (e.g. read X maps to
locus A in Perl and locus B in Rust, both "uniquely mapped" — the count is identical, the records
differ). The plan does treat B2 as the rigorous check, so this is fine — but §8 V5 phrases B1 as
"every count matches" with an "Expected: byte-identical" that could be misread as sufficient. Add one
sentence: *B1 passing is necessary but not sufficient; B2 is the content authority.* (It's implied by
§3.1 but a final gate doc should not leave it implied.)

**L4 — Re-base step 0 has an unstated failure mode.** (§4 step 0.) "confirm the aligner subtree equals
`origin/rust/iron-chancellor`" — but the rebase is `--onto origin/rust/iron-chancellor <old-aligner-v1-head>`.
If `rust/aligner-v1` has *any* commit that is not already squash-absorbed into iron-chancellor (e.g. a
stray local plan/doc commit), the rebase will replay it and the built binary will **not** be pure
iron-chancellor code. V1 ("re-based binary == merged code") checks `--version`, but `--version` will be
identical regardless of stray commits — it does not prove subtree equality. The verification for V1
should be a **`git diff origin/rust/iron-chancellor -- rust/bismark-aligner` is empty** (or a tree-hash
compare of the crate dir), not just `--version`.

**L5 — RRBS "promote to strict full if < ~20M" branch (§3.2, O1) interacts with the harness design.**
The Gate A harness (`phase10_subset_strict_gate.sh`) runs Perl **single-core**; the Gate B harness runs
Perl `--parallel P`. If RRBS is promoted to strict-full, it must run through the *Gate A* harness at full
size (single-core Perl), not Gate B — and the plan's step 3/5 split assigns RRBS-10M to Gate A and
RRBS-full to Gate B. The promotion path therefore needs the Gate A harness to accept a full-size RRBS
input (not just a 10M head-subset). Confirm the harness is parameterized on input path + optional
subset-N so the same script serves both, rather than hard-coding a 10M head.

---

## 2. Assumptions

**A1 — "Perl `--multicore` = same multiset as single-core" is the load-bearing assumption and is
adequately corroborated, with one residual.** (§7, §2.) It's backed three ways (B1 report, 9b in-order
proof, V9 cross-check). The residual: 9b proved this property *for the Rust merge vs Perl single-core*,
and for *Rust* `--parallel` invariance — it did **not** directly prove *Perl's own* `--multicore`
multiset == Perl single-core multiset, because 9b ran Perl single-core only (the harness comment is
explicit: "Perl gets the SAME argv MINUS --parallel"). So at full scale the plan is, for the first time,
trusting Perl `--multicore` to be multiset-equal to Perl single-core **without ever having directly
checked it**. The cheapest possible direct check: at **10M**, run Perl `--multicore P` (in addition to
the existing Perl single-core Gate A run) and confirm `sort|md5` equals the Perl single-core `sort|md5`.
That converts the load-bearing assumption into a *measured fact at 10M* before relying on it at 84M, at
near-zero marginal cost (one extra Perl 10M run, reusing the Gate B comparison machinery). Strongly
recommend adding this as a Gate A bonus alongside the existing worker-invariance bonus.

**A2 — GNU `sort` ordering is reproducible across the two runs — but the plan never pins `LC_ALL=C`.**
(§3.4, §8.) Confirmed by inspection: the plan's only `sort` invocation is `sort -S 50% --parallel=<N>
-T /var/tmp` (§3.4) — **no locale fixed.** For a *multiset* comparison this is subtler than it looks:
`md5sum` of a sorted stream is identical between the two sides **only if both sorts impose the same total
order**. If the locale is a UTF-8 collation (oxy default is likely `C.UTF-8` or `en_US.UTF-8`), `sort`
collation of SAM lines containing arbitrary sequence/quality bytes can be (a) slower, (b) potentially
non-total / order-unstable on equal-collating-but-byte-different lines, and (c) most importantly,
*if the locale ever differs between the two timed runs* (e.g. a pod recycle resets the env between the
Perl run and the Rust run), the two md5s diverge **for the wrong reason** → false FAIL; conversely a
locale that collates two distinct byte-strings as "equal" with an unstable tie-break could in principle
emit them in different positions and mask. The robust fix is trivial and standard: **`LC_ALL=C sort`** on
both sides (byte-wise total order, fastest, reproducible). This is a Critical omission for a byte/content
gate — add `LC_ALL=C` to every `sort`, `cmp`, `md5sum`, and `grep` in both harnesses.

**A3 — md5 collision is correctly dismissed, but the plan must guarantee the md5 *input is canonical*.**
The skill prompt's probe is right: md5 collision is negligible, but *is the input to md5 guaranteed
canonical?* Two failure routes to a false PASS: (i) the `samtools view` field formatting differs but
collapses under sort (it won't — sort is line-wise, formatting differences change the line and thus the
hash); (ii) **trailing-newline / final-line handling** differs between the Perl and Rust streams (a
missing final `\n` changes the md5). With `LC_ALL=C` and identical `samtools view` invocation on both
sides this is controlled, but the plan should assert the two streams have **equal line counts** (ties
back to L1) so a truncation can't pass. Recommend `wc -l` equality as a pre-hash assertion.

**A4 — "`@PG` filtered, everything else in the header is identical" — header filtering may be both too
aggressive AND incomplete.** (§3.3, §8 V6.) The probe is apt. Concerns:
  - **Too aggressive:** filtering the *whole* `@PG` block (9b precedent) is correct for worker-invariance
    (argv is the variable). But Phase 10 also wants to *cross-check* against the pre-existing
    `--parallel 4` BAM (V9). The whole-block filter discards the one piece of evidence the plan cites for
    that BAM's provenance (`@PG` shows v0.25.1 + samtools 1.23.1). That's fine for the *content* compare,
    but it means provenance is asserted out-of-band (read the `@PG` manually once), not gated. State that.
  - **Possibly incomplete:** the plan compares `@HD`/`@SQ` and expects byte-identity. Two things to
    verify explicitly: (1) the **`@HD SO:` sort-order tag** — Bismark output is unsorted; if Perl and
    Rust differ on whether `SO:`/`GO:` is emitted (or its value), `@HD` diverges legitimately-or-not and
    the gate should know which. (2) **`@SQ` order** — depends on FASTA load/glob order; the Phase-1
    review already flagged glob-order as `@SQ`-byte-relevant and a macOS/Linux flip-flop. At full scale
    on GRCh38 *and* GRCm39 with possibly different temp/`-o` paths, confirm `@SQ` order is genome-derived
    (deterministic) and not path-derived. (3) **`@CO` lines** — does Bismark emit any? If so are they
    argv/path-bearing? The plan doesn't mention `@CO`. Recommend: compare the header as
    `grep -v '^@PG'` (as now) but **enumerate in `GATE_OXY.md` exactly which header lines remain and that
    they are byte-identical**, so an unexpected legitimately-varying line (e.g. a path in `@CO`) is caught
    as a finding rather than silently passing or failing.

**A5 — Cross-check vs `--parallel 4` BAM (V9) adds little independent assurance.** (§3.1, §8 V9.) Its
stated purpose is "corroborates oracle provenance." But the *primary* Gate B oracle is **also** Perl,
freshly run in the same pinned env. So V9 compares Rust against *a second Perl run of the same version*.
If Perl v0.25.1 + Bowtie2 2.5.5 had a content bug, **both** Perl runs (the pre-existing `--parallel 4`
and the fresh `--parallel P`) would share it, and Rust faithfully reproducing it would PASS all of V6/V9.
This is the inherent circularity of a faithful port — it is *by design* (the Perl run is the oracle) — but
the plan should be honest that V9 does **not** add an independent-correctness signal; it only adds (a) a
regression check that the pre-existing artifact wasn't generated with a *different* Bowtie2/version (a
provenance smoke), and (b) a check that `--parallel 4` vs `--parallel P` Perl layouts are multiset-equal
(which is actually a useful corroboration of A1!). Reframe V9's value as "Perl multicore layout-invariance
corroboration," not "oracle provenance" — that's its real contribution.

**A6 — RRBS untrimmed-read artifacts are fine for byte-identity but the plan should note they don't
mask.** (§7.) Aligning raw (adapter-bearing) RRBS reads will produce more unmapped/soft-clipped reads
than trimmed input — but this is *identical input to both Perl and Rust*, so any artifact appears on both
sides and the byte/content gate is unaffected. The plan's reasoning ("alignment logic identical; the gate
tests alignment, not trimming") is correct. One subtle point worth a line: untrimmed RRBS may push more
reads into the **genomic-seq-extraction-failure** path and the **multi-mapper/ambiguous** path — which is
*good* (more coverage of exactly the edge paths V11 targets), not bad. Note it as a positive.

**A7 — `/var/tmp` capacity assumption for the PE sort.** (§5, §10.) PE full ≈ "~2× SE" ≈ ~42 GB SAM
text; `sort -S 50%` with `-T /var/tmp` plus BAM copies on 678 G is "ample" — agreed numerically. But
`-S 50%` of a 128-core box's RAM is a large memory reservation; if a Perl `--multicore P` run is *still
resident* (or a recycle-orphan), the two could collide. Minor; the "run cells sequentially" rule (§5)
mostly handles it. Confirm the sort runs *after* both alignments complete and are flushed, not concurrently.

---

## 3. Efficiency analysis

- **Streaming `cmp` (Gate A) + `sort|md5sum` (Gate B) is the right call** and is a genuine improvement
  over the 9b harness's `diff <( ) <( )` which would OOM at 84M. Good catch in §3.4/§10.
- **B1-first ordering** (report identity before the 21 GB scan) is the correct cheap-gate-first design.
- **The commutative per-line-hash "fast pre-check" (§3.4) is mentioned but not specified** — and it is
  actually a *better* primary than sort-then-hash for this job: an order-independent hash (e.g. sum of
  per-line `md5`/`xxhash` mod 2^k, or `sort -u` count + line-hash-sum) is **O(1) memory and avoids the
  42 GB sort entirely**, with the same multiset-equality guarantee *if you also assert equal line counts*
  (which L1/A3 already demand). The plan keeps sort-then-hash as "the rigorous primary" — but sort-then-
  hash's rigor over a commutative-hash-plus-count is marginal (both prove multiset equality; the
  collision risk of a good 128-bit commutative accumulator is the same negligible order as md5). Consider
  **promoting the commutative hash to primary** and dropping the 42 GB sort to a diagnostic-only step —
  it removes the single largest resource demand and the locale hazard (A2) in one move. (Optional, but
  high-value: it materially de-risks the most fragile part of the run.)
- **Perf framing (§1.4, B4, V10)** correctly forbids per-core Rust-vs-Perl claims (`feedback_extractor_
  parallel_cpu_messaging`). Sound. One addition: capture the **Bowtie2 subprocess share** of wall-clock if
  cheaply possible — since the handoff notes the aligner is "decode/align-bound" and Bowtie2 (unchanged) is
  ~74%, the honest perf story is *wrapper + multicore-scaling overhead only*, and the report should make
  clear most wall-clock is the (identical) external aligner, so the Rust win is bounded by Amdahl. The
  plan says "scaling win only" — good — but spell out the Bowtie2-dominates caveat in `GATE_OXY.md`.

---

## 4. Validation sufficiency

**What Phase 10 actually de-risks beyond 9b / per-phase gates:** scale (10⁷→10⁸), full chromosome/scaffold
diversity (every GRCh38/GRCm39 contig, not just the subset's chromosomes), the mouse genome (GRCm39 — a
*different* reference build, first time), and real-data rare CIGARs / the genomic-seq-extraction-failure
path at volume. That delta is **real and worth a phase** — the answer to the skill's probe is *not* "only
re-proving proven properties." Coverage of that specific delta:

- **Chromosome diversity:** ✅ implicitly covered — full datasets touch all contigs. But the plan does not
  *assert* it. A regression in, say, the `_GA_converted` de-conversion (`s/_(CT|GA)_converted$//`) on an
  unusual scaffold name (alt contigs, `chrUn_*`, `_random`, mitochondria `chrM`) would only surface if a
  read maps there. **Recommend** a cheap assertion: confirm the set of distinct `RNAME`s in the Rust BAM
  equals the set in the Perl BAM (a `cut -f3 | sort -u` compare) — this directly gates "every chromosome
  the oracle uses, the port emits identically," which is precisely Phase 10's headline new information and
  is currently only *implied* by the full-line md5.
- **Rare CIGAR / indels / soft-clips:** covered by the full-line md5 (any CIGAR diff changes the line).
  Adequate. No separate assertion needed beyond V6.
- **Genomic-seq-extraction failures (V11):** ✅ good — the plan gates the report's "discarded because
  genomic sequence could not be extracted" count (SE oracle shows 36). One gap: V11 gates the *count*,
  but the *identity of which reads* were discarded is what matters for content. If Perl discards reads
  {A,B,C} and Rust discards {A,B,D} (same count, 3), the count matches but the BAM content differs — which
  B2's md5 *would* catch (the discarded reads simply aren't in the BAM, so the surviving multiset differs).
  So V11-count + V6-content together are sufficient *as long as L1's count reconciliation is added*.
  Confirm V11 is understood as a *fail-fast diagnostic*, with B2 as the authority. Fine once L1 is in.
- **PE-specific content hazards:** the multiset compare treats each SAM line independently. For PE, a
  read1/read2 **pairing** bug (correct lines, mis-paired) where both lines individually still appear in
  the multiset would NOT be caught by line-wise md5 — but it *would* be caught by FLAG/TLEN/RNEXT/PNEXT
  fields being part of each line (a mis-pairing changes those fields → changes the lines → changes the
  hash). So it is covered, but only because the mate-pointer fields are in-line. Worth a sentence
  confirming the comparison is on **full** records (all fields), not a projected subset — the plan does
  say "full record lines" (§3.4), so ✅; just don't ever reduce to a field subset.
- **Could a true divergence be indistinguishable from a true negative?** With L1 (count reconciliation) +
  A2 (`LC_ALL=C`) + A3 (line-count assertion) added, no: a real content divergence changes ≥1 line →
  changes the multiset → changes the md5 (or the commutative hash) → FAIL, and a count drift is caught
  independently. **Without** those three additions, the gaps in L1/A2/A3 are exactly the routes to a
  silent PASS. That is why I rate them Critical/Important.

**Strict-ordering coverage:** Gate A at 10M is in-order `cmp` vs Perl single-core — strong. Combined with
9b's coprime-N=1,000,003 straddle proof, ordering is well covered. No gap.

**Reproducibility of the gate itself:** §3.5 detach/poll/off-box-capture is good operational hygiene. But
the plan does not pin **how to re-derive the exact binary + dataset + env** if someone re-runs Phase 10 in
six months (binary drift). Recommend `GATE_OXY.md` record: the `bismark_rs --version` + the
iron-chancellor commit it was built from, the Bowtie2/samtools/Bismark versions (`--version` outputs
captured verbatim), the dataset md5s (at least the read files' sizes + read counts), and the exact argv of
each run. Without this, the gate "passes once and is never reproducible" — the skill's exact concern.

---

## 5. Alternatives

- **A5/A1 combined → make V9 earn its keep at 10M.** Instead of using the `--parallel 4` BAM only as a
  full-scale cross-check, run the **Perl `--multicore` vs Perl single-core multiset compare at 10M** (A1).
  That is the single highest-value addition: it directly measures the load-bearing assumption cheaply
  before betting the full-scale gate on it.
- **Commutative order-independent hash as primary (§3 efficiency).** Replaces the 42 GB sort, killing
  both the largest resource demand and the locale hazard. Keep sort-then-hash as the on-FAIL diagnostic.
- **Per-chromosome content sharding for diagnosis.** On a Gate B FAIL, a single 84M md5 mismatch gives
  zero locality. Cheap improvement: compute the per-`RNAME` md5 (group by col 3) so a FAIL points at the
  offending chromosome(s) immediately — turns a "somewhere in 84M reads" failure into "chr14 differs,"
  dramatically shortening the diagnose loop. (Diagnostic-only; doesn't change the PASS criterion.)
- **Consider a single mid-scale strict full run for RRBS regardless of size.** If RRBS measures, say,
  25–30M (just over the 20M threshold), a single-core Perl run is plausibly a few hours — and a *strict*
  full RRBS gate on a *different genome* would be the strongest single new datapoint Phase 10 could
  produce (strict byte-identity at full scale on GRCm39). Worth keeping the threshold flexible / asking
  Felix rather than auto-defaulting to hybrid at 20M.
- **Don't skip a non-directional/pbat smoke entirely.** §1.2 drops non-dir/pbat from Phase 10 because they
  "land ~0 reads on directional libraries." True for *these* libraries — but it means **no** non-dir/pbat
  path is exercised at full scale ever. The 9b/1M gates cover them on subsets; that's probably enough for
  a faithful port. But if any non-dir/pbat-only code path has a scale-dependent bug (e.g. a counter
  overflow, a buffer sizing), it's untested at 10⁸. Low risk (the per-read logic is scale-free), but worth
  a one-line explicit risk acceptance in §10 rather than silent omission.

---

## 6. Action items (prioritized)

### Critical (a gate that can silently PASS while wrong, or build the wrong binary)
- **C1 (A2).** Pin **`LC_ALL=C`** on every `sort`, `cmp`, `md5sum`, `grep` in both harnesses. An unpinned
  locale makes the multiset md5 non-reproducible across the two timed runs (recycle/env drift) and risks a
  non-total/unstable collation order → false FAIL or, in the pathological collation case, a mask. Standard
  byte-gate hygiene; trivial to add. (§3.4, §8.)
- **C2 (L1 + A3).** Add an explicit **three-way record-count reconciliation** as a hard gate line: Perl
  `samtools view -c` == Rust `samtools view -c` == report-implied count, **plus** `wc -l` equality of the
  two `samtools view` streams *before* hashing. Without it, a drop/duplicate/truncation can pass the
  sort|md5 path if the report check is coincidentally insensitive. (§3.1, §3.4, §8 V5–V8.)
- **C3 (L4).** Strengthen V1: verify the re-based binary by **`git diff origin/rust/iron-chancellor --
  rust/bismark-aligner` is empty** (tree-hash compare), not just `bismark_rs --version` — `--version`
  cannot detect a stray replayed commit in the crate. (§4 step 0, §8 V1.)

### Important (assurance gaps / diagnosis / header completeness)
- **I1 (A1).** At **10M**, add a Perl `--multicore P` vs Perl single-core **multiset compare** (reuse the
  Gate B machinery). This converts the load-bearing "multicore == single-core multiset" assumption into a
  measured fact before relying on it at 84M. Near-zero marginal cost; highest assurance-per-effort.
- **I2 (A4).** Enumerate in `GATE_OXY.md` **exactly which header lines remain after the `@PG` filter** and
  assert byte-identity on them; explicitly check `@HD SO:`/`GO:`, `@SQ` order (genome-derived, not path-
  derived), and any `@CO`. Catch a legitimately-varying header line as a finding, not a silent pass/fail.
  (§3.3, §8 V6.)
- **I3 (validation §4).** Add a **distinct-`RNAME` set equality** assertion (Perl vs Rust, `cut -f3 |
  LC_ALL=C sort -u`) — directly gates Phase 10's headline new information (full chromosome/scaffold
  diversity, incl. GRCm39 alt/Un/random/M contigs and the `_(CT|GA)_converted` de-conversion). Currently
  only implied by the full-line md5.
- **I4 (L2).** Specify the on-FAIL **diagnosis recipe** concretely: from `cmp`'s offset → line number →
  bounded `sed -n` window on **both decompressed `@PG`-filtered streams** (never a full `diff` of the
  84M/10M stream — that re-introduces the OOM hazard). (§3.4.)
- **I5 (A5).** Reframe V9's stated value from "oracle provenance" to "Perl multicore-layout-invariance
  corroboration + provenance smoke" — be honest it adds no independent-correctness signal (both oracles
  are the same Perl version; faithful-port circularity is by design). (§3.1, §8 V9.)

### Optional (efficiency / robustness / scope honesty)
- **O1 (efficiency §3).** Promote the **commutative order-independent hash** to the primary content check
  (with the C2 count assertion); demote the 42 GB sort to on-FAIL diagnostic. Removes the largest resource
  demand and the locale hazard in one move.
- **O2 (alternatives).** On a Gate B FAIL, compute **per-`RNAME` md5** to localize the divergence to a
  chromosome instead of "somewhere in 84M reads."
- **O3 (reproducibility §4).** Have `GATE_OXY.md` record the full reproduction tuple: `bismark_rs
  --version` + iron-chancellor build commit, Bowtie2/samtools/Bismark `--version` verbatim, dataset read
  counts/sizes (md5 if cheap), and each run's exact argv — so the gate is re-derivable, not pass-once.
- **O4 (L5).** Confirm the Gate A harness is parameterized on input-path + optional subset-N so the same
  script serves both 10M-subset RRBS and a promoted strict-full RRBS (O1 in the plan).
- **O5 (alternatives).** Add a one-line **explicit risk acceptance** in §10 that non-directional/pbat code
  paths are not exercised at full scale (covered only at 1M/9b); low risk (per-read logic is scale-free)
  but currently a silent omission.
- **O6 (A6).** Note that untrimmed RRBS *increases* coverage of the unmapped/clipped/genomic-seq-failure
  edge paths (a positive for the gate), so the artifact concern is a feature, not a risk.
- **O7 (A7/perf).** State in `GATE_OXY.md` that wall-clock is Bowtie2-dominated (~74%, unchanged) so the
  Rust win is wrapper + multicore-scaling only (Amdahl-bounded) — reinforces the honest perf framing.

---

## Summary

The plan's core logic (order-vs-content split, hybrid oracle, ephemeral-pod discipline) is sound and the
new information Phase 10 buys (scale + full chromosome diversity + GRCm39 + edge paths) is real, not
re-proving. The risks are concentrated in the **content-gate mechanics**: the sort/hash path is missing
`LC_ALL=C` (C1) and a record-count reconciliation (C2) — together the two routes by which a real
divergence could PASS — and V1 verifies the binary too weakly (C3). Beyond those, the highest-value
addition is measuring the load-bearing "Perl multicore == single-core multiset" assumption directly at
10M (I1) rather than only inferring it, and tightening the header comparison (I2) + asserting chromosome-
set equality (I3) so Phase 10's headline claim is gated rather than implied. With the three Critical items
fixed the gate cannot silently PASS while wrong.

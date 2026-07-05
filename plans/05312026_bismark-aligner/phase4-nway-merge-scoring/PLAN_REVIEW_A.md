# PLAN_REVIEW_A — Phase 4: N-way lockstep merge + scoring + strand + MAPQ

**Reviewer:** A (independent, fresh context)
**Target:** `phase4-nway-merge-scoring/PLAN.md`
**Grounding read:** Perl `bismark` `check_results_single_end` (2702–3151), `calc_mapq` (3923–4180),
`extract_corresponding_genomic_sequence_single_end` (4289–4454), `reset_counters_and_fhs`
(7092–7244), main SE loop `process_single_end_fastQ_file_for_methylation_call` (2393–2482),
`fix_IDs` (6235–6246); Rust `align.rs`, `convert.rs`, `config.rs`, `options.rs`, `lib.rs`.

Verdict: **the merge logic and `calc_mapq` plan are largely faithful and the float-semantics risk is
lower than the plan fears (verified bit-identical, see below). But there is one Critical structural
error — the strand counters belong to Phase 5, not Phase 4 — and one Critical lockstep-key gap (the
`@`-strip).** Both will silently diverge from Perl if implemented as written.

---

## Critical

### C1 — Strand counters (`CT_CT_count`…) are a Phase-5 increment, gated by genome-edge logic Phase 4 cannot see. The plan puts them in Phase 4.
The plan (§3.2 last bullet; §4 `Counters`; §9 row 8) says Phase 4 increments
`CT_CT_count`/`CT_GA_count`/`GA_CT_count`/`GA_GA_count` and cites **"7113–7120"**. That citation is the
**hash *initialisation*** inside `reset_counters_and_fhs` (every key set to 0), **not** the increment.
The actual increments live in **`extract_corresponding_genomic_sequence_single_end`** at lines **4402
(`CT_CT_count`), 4411 (`CT_GA_count`), 4426 (`GA_CT_count`), 4441 (`GA_GA_count`)** — which is a
**Phase 5** function (genomic-seq extraction). Worse, they are **gated behind chromosome-edge
early-returns** that need the loaded genome:
- index 1/3: `return` at **4320** (when `pos-2 < 0`) — *before* the counter block.
- index 0/2: `return` at **4393** (when chromosome shorter than `pos+2`) — *before* the counter block.

So in Perl the order for a unique-best read is: `unique_best_alignment_count++` (3121) →
`extract_corresponding_genomic_sequence` (3124) → [maybe edge-return, bumping
`genomic_sequence_could_not_be_extracted_count` at 3129 and **never** the strand counter] → otherwise
strand counter at 4400+. A read at a chromosome edge is therefore counted in
`unique_best_alignment_count` but **not** in any `*_count` strand bucket.

**Consequence if implemented as the plan states:** Phase 4 would increment a strand counter purely from
the selected `index`, with no genome and no edge check. For any edge-of-chromosome unique-best read the
Rust strand totals would be **+1 over Perl**, and the invariant `sum(strand counts) ==
unique_best - genomic_sequence_could_not_be_extracted` would break. This is a silent count divergence
that the Phase-6 report-parity gate would catch only later, after wasted Phase-4 effort.

**Action:** Remove the four strand counters from Phase 4's `Counters` and from §9 row 8. Phase 4's
`Decision::UniqueBest` already carries `index`; the strand-counter increment is correctly Phase 5's job
(emitted from the genomic-seq extraction, after the edge check). If a Phase-4 inspectable summary is
wanted now (Open Q3), report only the counters Phase 4 genuinely owns (C3 below). Keep §9 row 8 as a
test that the *selected index* is 0→OT / 1→OB (a `Decision.index` assertion), not a counter assertion.

### C2 — The lockstep key needs the leading-`@` strip; the plan only says "reuse Phase-2 `fix_id`".
Perl builds the matching `$identifier` in the main loop (2414–2444):
`chomp` → `fix_IDs($identifier)` (2421) → re-append/`chomp` `\n` (2422/2439) → **`$identifier =~ s/^\@//`
(2442)** → `check_results_single_end(uc$sequence, $identifier)`. The stream key it matches against
(`last_seq_id` / Rust `SamRecord.qname`) is the Bowtie 2 QNAME, which has **no** `@`.

Phase-2 `convert::fix_id` (convert.rs 76) operates on the FastQ header line that **still contains the
`@`** (the converted temp file keeps `@…`). So "reuse Phase-2 `fix_id`" alone produces a key like
`@R_1`, which will **never** equal the stream's `R_1`. Lockstep would match zero reads and every read
would fall through to `NoAlignment`.

**Action:** §2/§3.1/§5 step 4 / §8 must state the join-point identifier = `fix_id(chomp(header))` **then
strip a single leading `@`** before matching `qname`. Add an edge case + a unit test (header with and
without `@`; whitespace-collapsed id) to §9. Note Perl strips exactly one `^\@` (anchored, single),
not all `@`.

---

## Important

### I1 — Second-best assignment (3066–3080): the plan's "max(...)" glosses a `defined` guard and a strict `>`.
§3.2 says, for the 2-entry-best case, set `alignment_score_second_best = max(that alignment's own
second-best, the next entry's AS)`. Perl (3075) is:
`if (defined best.second_best AND best.second_best > next.AS) { use best.second_best } else { use next.AS }`.
That is **not** a plain `max`: (a) it requires `best.second_best` to be **defined** (it is `undef` when
the best alignment had no XS/ZS), and (b) the comparison is **strict `>`** — on a tie it takes
`next.AS`. The two differ only when `best.second_best == next.AS` (tie → Perl picks `next.AS`; a naive
`max` is equal anyway) **or** when `best.second_best` is undef (a `max(None, x)` must yield `x`, which a
careless implementation could get wrong). Since `alignment_score_second_best` feeds `calc_mapq` and thus
the BAM MAPQ byte, this must be transcribed as the exact `defined && >` form.
**Action:** Restate §3.2 with the explicit `defined && strict-`>`` semantics; add a unit case where
`best.second_best` is `None` and another where `best.second_best == next.AS`.

### I2 — Missing `sequences_count` (and clarify `genomic_sequence_could_not_be_extracted_count` is Phase 5).
The main SE loop increments `$counting{sequences_count}` (2433) once per input read — this is the
denominator of the alignment report and is owned by the **Phase 4 driver** (it re-reads the originals).
The plan's `Counters` (§4) omits it. Conversely `genomic_sequence_could_not_be_extracted_count` (3129)
is Phase 5 and should not appear here.
**Action:** Add `sequences_count` to `Counters` (incremented in the driver, post-`skip`/pre-`upto`
exactly as 2426–2433); explicitly defer `genomic_sequence_could_not_be_extracted_count` and the four
strand counters to Phase 5 (ties to C1).

### I3 — `skip`/`upto` must be replicated in the driver, with Perl's falsy-0 ordering.
Perl's main loop applies `if ($skip){ next unless $count > $skip }` and `if ($upto){ last if $count >
$upto }` **after** `++$count` and **before** `sequences_count++` (2424–2433). Phase 2's converter only
converts the reads it writes, but Phase 4 re-reads the **original** FastQ independently — so it must
apply the *same* skip/upto, or the re-read stream and the converted/aligned stream will desync (the
streams were produced from a skip/upto-filtered conversion). The plan (§5 step 4) says "re-read the
original FastQ … call the merge per read" without mentioning skip/upto.
**Action:** Spell out skip/upto in the driver, matching Phase-2 `ConvertOptions` falsy-0 semantics
(convert.rs already encodes `s > 0` / `u > 0`). Confirm the converted temp the streams read was itself
skip/upto-filtered so counts line up. Add a skip/upto driver test.

### I4 — The `flag==4` "next id is also me → die" guard (2747–2749) is not in the plan.
§3.1 maps unmapped to "advance once, skip the instance (2738–58)" but omits Perl's safety `die`: after
advancing past an unmapped record, if the *new* `last_seq_id` still equals the current identifier, Perl
dies (2748: "did not produce any alignment, but next seq-ID was also …"). This is a real divergence in
behaviour (Perl aborts; a silent Rust skip would mask a malformed stream).
**Action:** Add this guard to §3.1 and an edge-case row. (Low data-impact but it's an explicit-failure
contract per the global "fail loudly" principle.)

---

## Optional / precision

### O1 — Float semantics: VERIFIED bit-identical Perl↔Rust; downgrade the §11(c) risk.
I empirically compared `scMin = intercept + slope*readLen` and the `bestOver == diff` /
`bestOver >= diff*k` forms in Perl 5 vs `rustc 1.95` f64 for readLen ∈ {36,50,51,75,76,99,100,101,123,
150,151,200,251,300}. Every value is the same f64 (e.g. len=51 → `-10.20000000000000107` in both;
len=50 → exactly `-10`). `bestOver == diff` (exact-equality on floats) holds identically for AS_best=0
across the fractional-scMin lengths too. So the plan's biggest self-declared risk (§11 risk c) is
**well-mitigated** by "f64 throughout + identical comparison forms" — there is no observed trap as long
as the port computes `intercept + slope*(readLen as f64)` (no premature rounding, no integer scMin) and
keeps the `==`/`>=` operators verbatim. Worth stating this explicitly so the implementer doesn't
over-engineer (e.g. epsilon comparisons would *break* parity).
*(Reproduction is trivial; harness left at `/tmp/claude/mapqcmp.rs` if useful.)*

### O2 — `--score_min` numeric parse: match Perl's GREEDY regex, not first-comma split.
The plan reuses the existing `valid_score_min_l` (options.rs 197, a `split_once(',')` shape check) and
adds an f64 extract. Perl parses via `^L,(.+),(.+)$` (7917), whose first `(.+)` is **greedy** → splits
at the **last** comma. `split_once` splits at the **first**. They agree for the only realistic form
`L,a,b` (one comma after `L,`), but diverge on pathological multi-comma input (`L,0,-0.2,x` → Perl
`$1="0,-0.2", $2="x"`; Rust `"0","-0.2,x"`). Impact is near-zero (no sane `--score_min` has >2 numbers),
but if the plan wants strict parity note it, or accept the divergence explicitly.
**Action:** One sentence in §5 step 1: parse the intercept/slope by splitting at the **last** comma of
the post-`L,` remainder (greedy-equivalent), parse both as f64.

### O3 — `Decision::Ambiguous` cannot carry `first_ambig_alignment`, but §3.2 says to "record" it.
§3.2 mentions recording `first_ambig_alignment` (de-converted, for `--ambig_bam`), yet the §4 enum
variant `Ambiguous` is unit (no payload). `--ambig_bam` output is Phase 6 (EPIC Phase 6). Either give
`Ambiguous` an optional `first_ambig_alignment: String` field now (cheap, future-proofs Phase 6) or
state in §3.2 that capture is **deferred to Phase 6** and drop the "record" wording to avoid an
implementer building dead state.
*(Note the de-conversion at 2808/2824 strips `_CT_converted`/`_GA_converted` **without** an anchored
`$`, unlike the RNAME `s/…_converted$/` at 2763 — i.e. `s/_(CT|GA)_converted//` matches anywhere. If
Phase 6 reproduces this line, mirror the un-anchored form. Out of Phase-4 scope; flag for the file.)*

### O4 — Same-thread vs cross-instance ambiguity: the `>=`/`>` and `overwrite` interplay is correct but under-tested.
§3.1's transcription of 2802–2834 (`>=` keeps equals → `overwrite`; `>` resets `amb_same_thread`;
`best_AS_so_far` updated *after* the reset, per the 26-06-2017 comment at 2828) is faithful. But §9
rows 3/4 test only the simplest tie shapes. Two higher-risk orderings are untested: (a) instance 0 sets
best, instance 1 is **strictly better** (must reset `amb_same_thread` to 0 even if instance 0 had set
it); (b) a within-thread second_best tie at a **worse-than-best** score (2851 else-branch: must NOT set
`amb_same_thread`). Add these two unit cases so a `>=`-vs-`>` or ordering slip can't pass silently.

### O5 — Same-location dedup key: confirm string form `chr:pos` exactly.
§3.1/§3.2 key `alignments` by `chromosome:position`. Perl joins `(":", $chromosome, $position)` (2877,
2917) where `$position` is the raw SAM POS string and `$chromosome` is the de-converted RNAME. The Rust
`SamRecord.pos` is a parsed `u32`; keying by `(String, u32)` is equivalent (no leading-zero POS in SAM),
but if the implementer keys by a formatted string, ensure it matches `pos.to_string()` (no padding).
Minor; note it so the dedup test (row 5) pins the exact-collision behaviour. Also worth a row: a
same-location collision where the **second** instance has a *better* AS — Perl still overwrites the same
key (so `index` ends up the later one) but it remains a single entry → `UniqueBest`. Confirms the
"assign to first indexes 0/1" comment (2882) is *not* enforced by code (the later index can win the key);
the plan's §3.1 dedup bullet says "overwrites one entry" which is right, but the resulting `index` is the
**last writer's**, not necessarily 0/1 — make the test assert the actual Perl outcome.

---

## Logic / assumptions / efficiency (summary)

- **Merge core (§3.1/§3.2):** faithful to 2702–3151 except C1 (counters location) and the precision
  notes I1/O3/O4. The `overwrite`/`best_AS_so_far`/`amb_same_thread` machine, the two
  advance-until-qname-changes discard loops (2858/2897/2936), the `1 entry → accept`, `2–4 → sort-desc
  & equal-top → boot`, `>4 → die`, and the directional index-2/3 rejection (inert on SE-dir) are all
  correctly mapped. The sort comparator's instability among equals is harmless because only the **top
  two** are compared for the boot decision and only the single best is taken otherwise. ✓
- **`calc_mapq` (§3.3):** the end-to-end ladder (3945–4076) is the right scope; `--local` correctly
  guarded (rejected in v1). Verbatim transcription is the right call; O1 shows f64 parity holds. The
  no-second-best ladder hand-comp in §9 row 9 is correct (len50→diff10: AS0→42, AS-3→40). ✓
- **Lockstep assumption (§3.4 edge cases):** "every read → one line per stream incl. flag==4" is the
  correct reading of Bowtie 2 default output and the plan rightly defers final confirmation to Phase 10.
  The `qname != identifier` branch ("instance contributed nothing, leave its stream untouched") matches
  Perl's `next unless last_seq_id eq identifier` (the `foreach` simply skips that index). ✓ — but pair
  this with C2 (the key must actually be comparable) and I4 (the unmapped-then-same-id die).
- **`Decision`/wiring (§4/§7):** sound seam to Phase 5; reusing Phase-3 streams + Phase-2 conversion is
  right. Open Q3 (wire into `run()` now, counters-only) is reasonable given C1/I2/I3 corrections.
- **Efficiency (§6):** linear, ≤4-entry map, buffered streams — fine; no concern.
- **Config prereq (§5 step 1):** additive `score_min_intercept/slope: f64` is correct; the existing
  `valid_score_min_l` + `--score-min` string assembly stay untouched (parity-preserving). See O2.

## Validation sufficiency (§9)
Adequate for the happy paths and the MAPQ ladder *if* extended with: I1 (None/tie second-best), I3
(skip/upto), I4 (unmapped-then-same-id die), O4 (strict-better reset; worse-than-best second_best tie),
O5 (same-location with better second instance), and C2 (the `@`-strip key). For the **silent MAPQ-integer
divergence** risk specifically: row 10 ("pinned (AS_best, AS_2nd, readLen) cases") is too vague — it
must enumerate **at least one pinned case per ladder leaf** (the with-second-best tree has ~30 distinct
return values across the `bestDiff` × `bestOver` branches incl. the `bestOver == diff` exact-equality
arms at 3959/3967/…). A single transposed integer in any leaf is exactly the failure mode §11(a) fears;
only branch-covering pinned cases catch it. Recommend a small table-driven test that pins every distinct
return value of the `!local` ladder.

## Top action items (priority-ordered)
1. **C1** — move strand counters to Phase 5; drop them from Phase-4 `Counters` & §9 row 8; fix the
   bogus "7113–7120" citation (those are inits; real increments are 4402/4411/4426/4441, gated by
   edge-returns 4320/4393).
2. **C2** — join-point identifier must strip a single leading `@` after `fix_id`; add tests.
3. **I1/I2/I3/I4** — exact `defined && >` second-best; add `sequences_count`; replicate skip/upto;
   add the unmapped-then-same-id `die`.
4. **§9** — branch-cover the `calc_mapq` `!local` ladder (one pinned case per leaf), plus the O4/O5/C2
   cases.
5. **O1/O2/O3/O5** — document f64 parity (don't epsilon-compare); greedy `--score_min` split;
   resolve the `Ambiguous` payload vs deferred wording; pin same-location dedup `index` outcome.

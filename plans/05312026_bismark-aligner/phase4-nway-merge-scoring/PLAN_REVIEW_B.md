# PLAN_REVIEW_B — Phase 4: N-way lockstep merge + best-alignment scoring + strand assignment + MAPQ

**Reviewer:** B (independent, fresh context)
**Target:** `phase4-nway-merge-scoring/PLAN.md`
**Grounding read in full:** Perl `bismark` `check_results_single_end` (2702–3151), `calc_mapq` (3923–4180),
`reset_counters_and_fhs` (7100–7244), `extract_corresponding_genomic_sequence_single_end` (4273–4454),
the SE read loop (2400–2449), `fix_IDs` (6235–6246), `process_command_line` score-min block (7894–7955).
Phase-1/2/3 Rust: `convert::fix_id`, `align::{AlignerStream,SamRecord}`. EPIC + SPEC.

Verdict: **the plan is structurally sound and faithful in its core (merge/overwrite/ambiguity/unique-best/
calc_mapq).** But it contains **two concrete faithfulness defects** that would silently diverge from Perl —
one a scope/over-count error (strand counters), one a lockstep-key bug (the `@`-strip). Both are fixable
with small edits. Details below.

---

## Top findings (summary)

1. **CRITICAL — strand counters (`CT_CT_count` … `GA_GA_count`) are NOT incremented in
   `check_results_single_end`.** They are incremented inside `extract_corresponding_genomic_sequence_single_end`
   (4402/4411/4426/4441) — i.e. **Phase 5** — and only *after* a chromosome-edge early-return guard
   (4390/4317). The plan (§3.2 last bullet, §9 row 8) puts them in the Phase-4 merge at the `UniqueBest`
   decision point and cites 7113–7120 (which is the counter **init** table, not increment sites). This will
   **over-count** strand counters for edge reads and misplaces Phase-5 logic into Phase 4.
2. **CRITICAL — the lockstep key is `fix_id(header)` *with the leading `@` stripped*** (Perl 2442
   `s/^\@//` runs *after* `fix_IDs`). The plan's driver (§2, §5 step 4, §8) reuses Phase-2 `convert::fix_id`
   (which operates on the id **including `@`**) and `uc`s the sequence, but never strips `@`. As written the
   key never matches the SAM `qname` → every read falls into the "instance contributed nothing" branch →
   100% NoAlignment. Must `fix_id` **then** strip a single leading `@`.
3. **IMPORTANT — `calc_mapq` `bestOver == diff` exact-float-equality** is only robust if `diff` and
   `bestOver` are computed from the *same* `scMin` f64 binding. The plan flags float semantics (§11c) but
   should pin this explicitly. Pinned-value tests (§9 rows 9/10) are correct but too few to lock the ladder.
4. **IMPORTANT — `unique_best_alignment_count++` (3121) IS in Phase 4** and happens *before* extraction +
   *before* the edge/length-extraction failures — so it is correctly a Phase-4 counter, but it diverges from
   the strand counters' timing. The plan conflates the two; they must be split (one Phase-4, the strand four
   Phase-5).
5. **OPTIONAL** — the `flag==4` "next-seq-id also == identifier ⇒ die" guard (2747–49), Perl-sort
   non-stability (harmless), the `Counters` ownership boundary, and `--ambig_bam` first-ambig capture all
   merit a sentence; none block.

---

## Logic review (vs Perl 2702–3151)

### §3.1 per-instance scan — FAITHFUL, with two precise checks

- **`flag==4` skip + advance** (plan 3.1 bullet 1 ↔ Perl 2739–58): correct. **But** Perl has an extra guard
  (2747–2749): after pulling the next line, if `seq_id eq identifier` it **dies** ("did not produce any
  alignment, but next seq-ID was also …"). The plan's edge-case list mentions "stream already at EOF" and
  "multi-line discard" but **omits this die**. With the Rust `advance()` primitive the equivalent is: after
  `advance()` past an unmapped record, if `current().qname == identifier` → error. Worth adding to §3.1 and a
  test, since silently *not* dying here would let a malformed stream pass. **Important.**
- **de-convert RNAME `s/_(CT|GA)_converted$//`** (2763–68, die if absent): faithful. Note Phase-3 keeps
  `rname` raw (good) so the de-conversion + die belongs here — confirmed.
- **`overwrite`/`best_AS_so_far`/`amb_same_thread`** (plan 3.1 bullet 3 ↔ Perl 2802–2834): faithful in full
  detail — first alignment sets `best_AS_so_far`+`overwrite`; subsequently `AS >= best` ⇒ `overwrite`, and
  `AS > best` ⇒ also reset `amb_same_thread=0`; then `best_AS_so_far = AS` (moved *after* the reset, Perl
  2828 comment "moved this down so that amb_same_thread gets a chance to reset"). The plan captures the
  `>=` vs `>` distinction explicitly — good. ✔
- **second-best / same-thread ambiguity** (plan 3.1 bullet 4 ↔ Perl 2840–2913): faithful. `second_best`
  defined **and** `AS == second_best` **and** `AS == best_AS_so_far` ⇒ `amb_same_thread=1`; else store-if-
  overwrite at `chr:pos`. The "discard the rest of this read's lines" = advance-until-qname-changes — matches
  the three `until ($fhs[$index]->{last_seq_id} ne $identifier)` loops (2858/2897/2936). ✔ One nuance the
  plan should state: in the `AS==second_best && AS!=best` case (2851 else), Perl **still** runs the discard
  loop (2857–2872) but does **not** store the alignment — i.e. a worse-but-internally-ambiguous alignment is
  silently dropped and its stream advanced. The plan's bullet folds this into "discard the rest" but doesn't
  make the no-store explicit; please spell it out so the implementer doesn't accidentally store it.
- **same-location dedup** (`alignments` keyed `chr:pos`, 2877/2917): faithful — the comment about a read
  aligning to the same locus in both instances overwriting one entry is correctly captured. ✔

### §3.2 outcome classification — MOSTLY FAITHFUL, one scope error

- **`amb_same_thread` ⇒ Ambiguous** (2958/2968): faithful; `unsuitable_sequence_count++` at 2969. The plan's
  note about capturing `first_ambig_alignment` for `--ambig_bam` is correct (set at 2807/2823, de-converted
  `s/_(CT|GA)_converted//` — **note: no `$` anchor**, unlike the RNAME de-conversion at 2763 which *is*
  anchored). The plan should carry `first_ambig_alignment` on the `Ambiguous` decision for Phase 6, but the
  signature (§4) drops all payload from `Ambiguous`. **Optional now / Important for Phase 6** — flag that the
  Ambiguous variant will need to carry the (de-converted, unanchored-substitution) raw line.
- **empty `alignments` ⇒ NoAlignment** (2991, `no_single_alignment_found++`): faithful. ✔
- **unique-best selection** (3033–3088): faithful.
  - 1 entry ⇒ accept, carrying `alignment_score_second_best` from the stored entry (3040) — note this can be
    a *defined* value (the single instance's `XS`/`ZS`), not necessarily `None`. The plan's §3.4 field list
    carries it; good.
  - 2–4 ⇒ sort by AS desc; equal top-two ⇒ boot (`sequence_fails`, `unsuitable_sequence_count++`, 3091).
  - else store best + set `alignment_score_second_best = (best.second_best if defined and > next.AS) else
    next.AS` (3075–3080). The plan glosses this as `max(best's own second-best, next entry's AS)` — **result-
    equivalent** (undef ⇒ treated as −∞; `==` falls to the else, same value). ✔ but please reproduce the
    *exact* conditional form in code (not a `max`) so the undef/`>` boundary can't drift.
  - `>4` ⇒ die (3087). ✔
- **`--directional` index-2/3 rejection** (3112–3118): faithful + correctly noted inert on the 2-instance
  SE-dir spine (only idx 0/1 spawned). Including it now is the right call. ✔
- **`unique_best_alignment_count++`** (3121): **this is genuinely a Phase-4 counter** — it fires in
  `check_results_single_end` *before* `extract_corresponding_genomic_sequence_single_end` (3124) and before
  the genomic-length check (3127). Even reads that later fail extraction (3129 `genomic_sequence_could_not_be
  _extracted_count++`, return 0) have already been counted as unique-best. The plan is right to put this one
  in Phase 4. ✔
- **strand counters** — **SCOPE ERROR (Critical #1).** See dedicated section below.

### §3.3 MAPQ — FAITHFUL

`calc_mapq(len(sequence), None, AS_best, AS_2nd)` ↔ Perl 3133–3136. `scMin = intercept + slope*readLen`
(end-to-end, `!local`); `diff = |scMin|`; `bestOver = AS_best − scMin`; the no-second-best ladder
(42/40/24/23/8/3/0) and the nested `bestDiff/diff × bestOver/diff` ladder match 3945–4076. Verbatim
transcription is the right strategy. The plan correctly notes `read2Len = None` for SE so the PE second
`scMin +=` term (3934–3936) is dormant. v1 `!local` only — correct (`--local` already rejected in options.rs,
verified). ✔

---

## CRITICAL #1 — strand counters belong to Phase 5, not Phase 4 (over-counts edge reads)

**Plan claim (§3.2 final bullet, §9 row 8):** on `UniqueBest`, increment the strand counter for the index
(`CT_CT_count` idx0 / `CT_GA_count` idx1 / `GA_CT_count` idx2 / `GA_GA_count` idx3), "7113–7120".

**Perl reality:**
- `check_results_single_end` **never** touches `CT_CT_count`/`CT_GA_count`/`GA_CT_count`/`GA_GA_count`.
  Grep confirms the only increment sites are **4402 / 4411 / 4426 / 4441**, all inside
  `extract_corresponding_genomic_sequence_single_end` — i.e. **Phase 5** (EPIC: "Phase 5 — Genomic-seq
  extraction + XM/XR/XG + SAM/BAM"). Lines 7113–7120 cited by the plan are the **counter-initialization
  table** in `reset_counters_and_fhs` (all `=> 0`), not increments.
- The increment is keyed on `index + $pbat_index_modifier` (4400/4409/4424/4439), not bare `index`. For the
  SE-directional spine `pbat_index_modifier = 0` so it's the same, but the plan's index→counter table will
  be **wrong for pbat-SE** (Phase 8), where `pbat_index_modifier = 2` shifts idx 0→2, 1→3. Building the table
  on bare `index` now bakes in a latent Phase-8 bug.
- **Crucially, the increment is conditional on the chromosome-edge guard NOT firing.** For idx 0/2 the guard
  at **4390** (`length(chr) >= pos+2`) `return`s **before** the 4400-block increments; for idx 1/3 the guard
  at **4317** (`pos-2 >= 0`) `return`s before reaching 4409. So an edge read that wins unique-best is counted
  in `unique_best_alignment_count` (3121, pre-extraction) but **NOT** in its strand counter, and instead
  trips `genomic_sequence_could_not_be_extracted_count` at 3129.

**Consequence if implemented as planned:** Phase 4 would increment the strand counter at the `UniqueBest`
decision (pre-extraction), so every edge-failing unique-best read inflates the strand counter by 1 vs Perl.
On real genomes this is rare but non-zero, and it lands in the alignment **report** (Perl 2038/2044 prints
`CT/CT:`, `CT/GA:`, …) — a Phase-6 report byte-divergence that would only surface at the Phase-10 real-data
gate, exactly the kind of silent late-stage failure §9 is meant to catch.

**Recommended fix:**
- **Keep** `unique_best_alignment_count++` (3121), `unsuitable_sequence_count++` (2969/3092),
  `no_single_alignment_found++` (2992), `alignments_rejected_count++` (3115) in the Phase-4 merge — these are
  all genuinely in `check_results_single_end`.
- **Move** the four strand-counter increments to Phase 5, gated on successful genomic extraction, keyed on
  `index + pbat_index_modifier`. Phase 4 may still *define* the `Counters` struct fields and an
  `index→strand` helper, but must NOT increment the strand four here.
- Delete/relabel §9 row 8 accordingly (or make it a Phase-5 test), and fix the §3.2 citation (4400–4445, not
  7113–7120). Add a Phase-4 counter test that asserts `CT_CT_count` etc. are **untouched** by the merge.

---

## CRITICAL #2 — the lockstep key must strip the leading `@`

**Plan (§2 "applies the same `fix_id`", §5 step 4 "reuse Phase-2 `fix_id` for the identifier; `uc` the
sequence", §8 "the driver's `fix_id` MUST be the same one Phase 2 used"):** the driver re-reads originals,
applies `convert::fix_id`, uses the result as the per-read identifier / lockstep key.

**Perl reality (SE read loop 2420–2444):**
```
chomp $identifier;
$identifier = fix_IDs($identifier);   # 2421  — does NOT touch '@'
$identifier .= "\n"; ... chomp ...;   # 2422/2439
$identifier =~ s/^\@//;               # 2442  — strips the leading '@' SEPARATELY
my $return = check_results_single_end (uc$sequence, $identifier, ...);  # 2444
```
`fix_IDs` (6235–6246) only does whitespace→`_` (or icpc truncation); it leaves `@`. The `@`-strip is a
distinct step. The SAM `qname` Bowtie 2 reports has **no** `@` (it's the FastQ record marker), and Phase-2
wrote converted headers via `fix_id(<id including '@'>)` so the temp-file headers are `@<fixed>` → Bowtie 2
emits qname `<fixed>` (no `@`).

**Consequence:** if the driver keys on `fix_id(raw_header)` directly, the key is `@<fixed>` while every
stream's `current().qname` is `<fixed>` → `qname == identifier` is **never** true → every instance hits the
"contributed nothing" branch → `alignments` always empty → **100% NoAlignment**. This is not a subtle byte
drift; it breaks the whole phase. (It would be caught by the very first integration test, but the *plan*
should specify it so the implementer doesn't have to rediscover it.)

**Recommended fix:** in the driver, derive the key as `strip_leading_at(fix_id(chomp(raw_header)))` where
`strip_leading_at` removes **one** leading `@` (`s/^\@//`, not global). Add a sentence to §2/§5 step 4 and an
assertion in test row 1 that the canned stream qnames match `<fixed-without-@>`. (Phase-2's `fix_id` is still
the right helper — it just isn't the *whole* transform.)

---

## IMPORTANT — `calc_mapq` validation sufficiency + the exact-equality trap

- **`bestOver == diff` (3959 et al.) is exact f64 equality.** It is reachable only when `AS_best == 0`
  (because `bestOver = AS_best − scMin` and `diff = |scMin| = −scMin` for `scMin ≤ 0`, so `bestOver == diff`
  ⟺ `AS_best == 0`, and `0.0 + x == x` holds exactly in f64). This is robust **iff** Rust computes `diff`
  and `bestOver` from the **same** `scMin` binding (don't recompute `scMin` or re-derive `diff` via a
  different expression). The plan flags "use f64 / mirror comparison forms" (§11c) but should pin this
  concretely: *one* `let sc_min = ...;` then `let diff = sc_min.abs(); let best_over = as_best as f64 -
  sc_min;`. **Important** — a refactor that recomputes `diff` independently could silently flip the
  39-vs-33 / 35-vs-25 branches.
- **Pinned tests (§9 rows 9/10) are correct but thin.** Row 9 values check out: readLen=50 ⇒ scMin=−10,
  diff=10; AS=0 ⇒ bestOver=10 ≥ 8 ⇒ 42; AS=−3 ⇒ bestOver=7 ≥ 7 ⇒ 40. ✔ But the ladder has **7 + ~30
  branches**; two pinned points per branch family is the minimum to catch a transposed integer (Risk a in
  §11). **Recommend** a table-driven test that pins ≥1 value in **every** terminal `return` of the
  no-second-best ladder (7 branches) and ≥1 in each `bestDiff` tier of the second-best ladder (the 39/33,
  38/27, …, 6/2, 1/0 pairs incl. the `bestOver == diff` vs `>= 0.84/0.68` sub-branches). A non-integer
  `scMin` case (e.g. readLen=51 ⇒ scMin=−10.2, diff=10.2) should be included to exercise the float boundary.
  Hand-computing from the verbatim ladder (Open Q2's assumption) is fine and deterministic — endorse it.
- **`as_best`/`as_second` types:** Perl AS are integers but `calc_mapq` does float arithmetic
  (`AS_best − scMin`). Plan signature uses `i64` for scores, cast to `f64` inside — correct; just ensure the
  cast happens before subtraction (matches Perl's numeric context).

---

## §3 lockstep assumption + the `qname != identifier` branch — SOUND, verify-at-Phase-10 is acceptable

- The assumption "every read yields a line per stream incl. `flag==4`, so streams stay in lockstep" is the
  same invariant Perl relies on (the whole `last_seq_id eq identifier` machinery). It is correct for Bowtie 2
  with one alignment-per-read-default output and `--norc`/`--nofw`. Deferring empirical confirmation to
  Phase 10 (and the Phase-0 spike already showing per-read determinism) is reasonable. ✔
- **The "instance contributed nothing → leave stream untouched" branch (§3 edge case 1)** is the correct
  reading of Perl: the `foreach $index` loop only enters the body `if last_seq_id eq identifier` (2735);
  otherwise it does nothing and the stream stays put. So a stream positioned at a *later* read (because the
  current read produced no line in that instance) is simply skipped this iteration — faithful. ✔ The one
  thing the plan must guarantee: the streams are *only* advanced inside the per-instance body (never globally
  per read), exactly as Perl. The signature (`streams: &mut [AlignerStream]`, "advances the matching streams
  past it") implies this; make it explicit in the §5 step 3 prose.
- **Subtle Perl behaviour to preserve:** within the body, the `flag==4` branch and the three discard loops
  advance the stream until `last_seq_id ne identifier` **or EOF** (setting `current=None`). The Rust
  `advance()` returning `current=None` at EOF is the faithful analogue; ensure the discard loop terminates on
  *either* qname-change *or* `current().is_none()`.

---

## §4 `Decision`/`Counters` design + join-point wiring — SOUND, minor reshaping

- `Decision::UniqueBest(BestAlignment{chromosome, position, index, AS, AS_2nd, md, cigar, bowtie_seq, mapq})`
  is a clean Phase-5 seam (mirrors `methylation_call_params->{$id}` 3035–3042). ✔
- **`Ambiguous` carries no payload** — fine for Phase 4's counters, but Phase 6 needs the
  `first_ambig_alignment` (de-converted raw line) for `--ambig_bam`. Note now so the variant grows a field
  later without churn. **Optional.**
- **`Rejected`** correctly distinct from `NoAlignment` (different counter). ✔
- **Join-point wiring (Open Q3, §5 step 4):** wiring into `run()` to emit a **counters summary, no BAM** is
  the right scope for the genuine join point and gives an inspectable deliverable. Endorse the assumption. ✔
  Two cautions: (a) the driver must `finish()` each `AlignerStream` (Phase-3 drains stdout + checks exit) —
  don't just drop them, or a non-zero Bowtie 2 exit goes unnoticed; (b) the counters summary format is
  throwaway (Phase 6 owns the real report), so don't invest in matching Perl's report text here — and don't
  let a reviewer later mistake it for a gate.

---

## Efficiency (§6) — adequate

Linear in reads × instances; ≤4-entry map; buffered streams. No concern. The re-read of the original FastQ
(a second pass over the input, in addition to Phase-2's conversion pass) is inherent to Perl's architecture
(Perl also re-reads originals in the main loop while the converted temp drives Bowtie 2) — faithful, not a
regression. ✔

---

## Edge cases & validation sufficiency (§9) — good coverage, four gaps

Covered well: unique-best (1-instance + cross-instance), cross-instance tie, same-thread ambiguity,
same-location dedup, no-alignment, RNAME de-conversion + missing-suffix error, calc_mapq both ladders,
directional rejection.

**Gaps:**
1. **(Critical) row 8 tests the wrong layer** — strand counters aren't a Phase-4 increment (see Critical #1).
   Replace with a test asserting the merge leaves strand counters at 0, and move the strand-counter test to
   Phase 5.
2. **(Critical) no lockstep-key test** — add a test that the identifier the driver computes equals the
   stream qname for a header like `@READ_1 1:N:0:ACGT` (verifies `fix_id` + `@`-strip together).
3. **(Important) no `flag==4`-then-same-id die test** (Perl 2747–49) — add a canned stream where an unmapped
   record is followed by *another* line with the same qname → expect error.
4. **(Important) calc_mapq branch coverage** — current two rows under-test the 30+ branches; add the
   table-driven per-branch pinned test (see MAPQ section), incl. a non-integer-`scMin` case.
5. **(Optional) missing-AS / missing-MD die on a mapped record** — §3 edge case mentions it (Perl 2838) but
   §9 has no row. Add: a mapped (`flag!=4`) record with no `AS:i:`/`MD:Z:` → error. (Phase-3 leaves them
   `None`; Phase 4 must enforce.)

---

## Alternatives considered

- **Strand-counter ownership:** rather than splitting init (Phase 4) from increment (Phase 5), one could keep
  the whole `Counters` increment logic in Phase 5 and have Phase 4 own only the four "always-in-
  check_results" counters. Cleaner boundary, fewer cross-phase fields touched in Phase 4. Recommend this.
- **calc_mapq as a pure table vs nested ifs:** a data-driven threshold table is tempting but the
  `bestOver == diff` exact-equality short-circuits and the asymmetric sub-thresholds (0.84/0.68 vs
  0.88/0.67) make a faithful table awkward; the verbatim nested-if transcription is the lower-risk choice the
  plan already picks. Endorse.
- **Driver re-read vs threading originals through Phase 2:** Phase 2 could have retained originals to avoid a
  re-read, but matching Perl's re-read architecture (and keeping Phase 2 a pure converter) is the right call.

---

## Action items (prioritized)

**Critical**
- **C1.** Remove strand-counter increments (`CT_CT_count`/`CT_GA_count`/`GA_CT_count`/`GA_GA_count`) from the
  Phase-4 merge (§3.2, §9 row 8). They live in `extract_corresponding_genomic_sequence_single_end`
  (4400/4409/4424/4439) = **Phase 5**, keyed on `index + pbat_index_modifier`, and are gated by the
  chromosome-edge guards (4317/4390). Fix the citation (currently the wrong lines 7113–7120). Add a Phase-4
  test that the merge leaves these four at 0.
- **C2.** Specify the lockstep key as `fix_id(chomp(header))` **then strip one leading `@`** (Perl 2442,
  separate from `fix_IDs`). Update §2/§5-step-4/§8 and add a key-vs-qname test. Without this the phase
  produces 100% NoAlignment.

**Important**
- **I1.** Pin `calc_mapq` to one `scMin` f64 binding feeding both `diff` and `bestOver`; document the
  `bestOver == diff ⟺ AS_best == 0` invariant. (§3.3, §11c.)
- **I2.** Expand calc_mapq tests to ≥1 pinned value per terminal `return` (both ladders) + a non-integer
  `scMin` case. (§9 rows 9/10.)
- **I3.** Add the `flag==4`-then-same-identifier **die** (Perl 2747–49) to §3.1 edge cases + a test.
- **I4.** Make explicit (§3.1) that the `AS==second_best && AS!=best_AS_so_far` case advances the stream but
  does **NOT** store the alignment; and reproduce the second-best assignment (3075–3080) as the exact
  `if (defined best_2nd && best_2nd > next_AS)` conditional, not a `max`.

**Optional**
- **O1.** Note that `Decision::Ambiguous` will need to carry the `first_ambig_alignment` (de-converted via
  the **unanchored** `s/_(CT|GA)_converted//`, 2808) for Phase 6 `--ambig_bam`.
- **O2.** Driver must `finish()` each `AlignerStream` (exit-status check), not drop it.
- **O3.** Add a §9 row for the missing-AS/MD die on a mapped record (Perl 2838).
- **O4.** One line acknowledging Perl's `sort` is not stable but that equal-top-two always boots, so the
  winner + assigned second-best value are deterministic regardless of sort order (Rust `sort_by` is stable —
  harmless).
- **O5.** Build the `index→strand` helper on `index + pbat_index_modifier` (not bare `index`) now, to avoid
  a latent pbat-SE bug in Phase 8.

# PLAN — Phase 4: N-way lockstep merge + best-alignment scoring + strand assignment + MAPQ

> **Epic:** `05312026_bismark-aligner/EPIC.md`, Phase 4 — *N-way merge + scoring + MAPQ*
> Depends on: **Phase 1** (`RunConfig`), **Phase 2** (`fix_id` + the converted reads), **Phase 3**
> (`AlignerStream` peek/advance + `SamRecord`). SE directional, 2 instances. **No** genomic-seq, `XM`,
> or BAM output — those are Phase 5.

## 1. Goal

Implement the bisulfite **best-alignment merge** for single-end directional reads: drive the 2
`AlignerStream`s (Phase 3) in **read-ID lockstep** against a re-read of the original FastQ, and for each
read decide its outcome — **unique best alignment** (with strand/index + MAPQ), **ambiguous**,
**no-alignment**, or **directional-rejected** — exactly as Perl `check_results_single_end` (2702–3151) +
`calc_mapq` (3923–4186). Output is a per-read **`Decision`** + the strand/outcome **counters**; this phase
does **not** extract the genomic sequence, make the `XM` call, or write the BAM (Phase 5).

## 2. Context

- **New modules** `rust/bismark-aligner/src/merge.rs` (the `check_results_single_end` port → `Decision`) and
  `rust/bismark-aligner/src/mapq.rs` (`calc_mapq`, a self-contained ~260-line ladder, separately testable).
- **This is the join point.** A driver re-reads the original FastQ (Phase 2 did not retain it) and, for
  each read, derives the **match identifier the Perl way (2442): `fix_id(header)` THEN strip one leading
  `@`** (`s/^\@//`). 🔴 The `@`-strip is essential — Phase 2's `fix_id` keeps the `@`, but the Bowtie 2 SAM
  `qname` has none, so without stripping it the key `@<id>` never matches `qname` `<id>` → **100%
  NoAlignment** (rev-1 correction — both reviewers Critical). Reuse `convert::fix_id` + the strip. The
  driver then calls the merge against the streams. Wires into a pipeline for the SE-directional spine
  (convert → spawn 2 streams → merge), but still emits only a **summary/counters**, no BAM.
- **Perl source of truth:** `check_results_single_end` (2702–3151), `reset_counters_and_fhs` (7124–7243,
  the index→strand table + counters), `calc_mapq` (3923–4186).
- **Config prereq:** `calc_mapq` needs `--score_min` as numbers — extend `RunConfig` (additive) with
  `score_min_intercept: f64` + `score_min_slope: f64` (default `0.0` / `-0.2`), parsed in `options.rs`/
  `resolve()` (mirrors Perl `$score_min_intercept`/`$score_min_slope`). `--local` is rejected in v1, so only
  the end-to-end (`L`) form is needed.

## 3. Behavior (numbered) — mirrors `check_results_single_end` (2702–3151)

**3.1 Per-read merge** — for one original read `(identifier, uc(sequence))`, iterate the instances
(index 0 = CTreadCTgenome, 1 = CTreadGAgenome for SE-directional). For each instance whose stream
`current()` has `qname == identifier`:
- **unmapped (`flag == 4`)** → advance that stream once, skip the instance (2738–58).
- **de-convert RNAME** `s/_(CT|GA)_converted$//` → `chromosome` (die if the suffix is absent, 2763–68).
- **`AS`-based `overwrite` + `best_AS_so_far`** (2798–2834): first alignment sets `best_AS_so_far`,
  `overwrite=1`; thereafter `AS >= best_AS_so_far` → `overwrite=1`, and `AS > best_AS_so_far` also resets
  `amb_same_thread=0`; then `best_AS_so_far = AS`. (`>=` so equally-good alignments are kept and flagged
  ambiguous later.)
- **second-best / same-thread ambiguity** (2840–2913): if `second_best` is defined and `AS == second_best`
  and `AS == best_AS_so_far` → `amb_same_thread = 1`; discard the rest of this read's lines in the stream.
  Else store the alignment (if `overwrite`) at key `chromosome:position` in `alignments` and discard the
  rest of this read's lines (the `until last_seq_id ne identifier` loop → our `advance()`-until-qname-changes).
- **same-location dedup:** `alignments` is keyed by `chromosome:position`, so a read aligning to the *same*
  locus in both instances overwrites one entry (2877–2894) — not treated as ambiguous.

**3.2 Outcome classification** (post-loop, 2957–3121):
- `amb_same_thread` → `alignment_ambiguous` → **Ambiguous** (`unsuitable_sequence_count++`; record the
  `first_ambig_alignment` for `--ambig_bam`, RNAME de-converted; 2807/2974).
- empty `alignments` → **NoAlignment** (`no_single_alignment_found++`, 2991).
- **unique-best selection:** 1 entry → accept (3033). 2–4 entries → sort by `AS` desc; if the top two have
  **equal `AS`** → **Ambiguous** (`sequence_fails`, `unsuitable_sequence_count++`, 3048–3107); else take the
  best, and set `alignment_score_second_best` via the **exact conditional (3075–3080)**: *if* the best
  alignment's own `second_best` is **defined AND strictly `>` the runner-up entry's `AS`** → use the best's
  own `second_best`; **else** use the runner-up entry's `AS`. (NOT a plain `max()`.) `> 4` entries → die (3087).
- **`--directional` rejection** (3112–3118): if the chosen `index` is **2 or 3** → **Rejected**
  (`alignments_rejected_count++`). *NB: for the SE-directional v1 spine only indexes 0/1 are spawned, so
  this never fires here — include it for faithfulness + the PE/non-dir phases.*
- otherwise **UniqueBest** → `unique_best_alignment_count++` (3121, genuinely Phase 4). **🔴 The per-strand
  counters (`CT_CT`/`CT_GA`/`GA_CT`/`GA_GA`) are NOT incremented here** — Perl increments them inside
  `extract_corresponding_genomic_sequence_single_end` (~4400–4441, **Phase 5**), keyed on `index`, and
  **gated behind the chromosome-edge early-returns** (so an edge read counts in `unique_best` but in no
  strand bucket). They must be deferred to Phase 5 (rev-1 correction — both reviewers Critical).

**3.3 MAPQ** (`calc_mapq`, 3133–3136 → 3923–4186): `mapq = calc_mapq(len(sequence), None, AS_best,
AS_second_best)`. Port the **end-to-end (`!local`) ladder verbatim** — `scMin = intercept + slope·readLen`;
`diff = |scMin|`; `bestOver = AS_best − scMin`; then the `bestOver/diff` ladder (no second-best → 42/40/24/
23/8/3/0) or the nested `bestDiff/diff` × `bestOver/diff` ladder (with second-best). v1 needs only `!local`.
- **Float semantics (rev-1, Reviewer A verified):** use plain `f64` and the **exact** comparison forms
  (`bestOver == diff`, `>= diff * 0.8`, …) — Perl 5 and `rustc 1.95` produce **bit-identical** `f64` for
  this arithmetic across read lengths, so an epsilon/`approx` comparison would **break** parity. Derive
  `diff` and `bestOver` from the **same** `scMin` binding (do not recompute `scMin`).

**3.4 `Decision`** carries, for `UniqueBest`: `{ chromosome, position, index, alignment_score,
alignment_score_second_best, md_tag, cigar, bowtie_sequence, mapq }` — the equivalent of Perl's
`methylation_call_params->{$id}`. Phase 5 adds the genomic sequence + `XM` from this.

### Edge cases
- read absent from a stream's `current()` (`qname != identifier`) → that instance contributed no line for
  this read; leave its stream untouched (it's positioned at a later read). *(Confirm against real Bowtie 2:
  every read yields a line per stream incl. `flag==4`, so streams stay in lockstep — verify in Phase 10.)*
- multiple alignment lines for one read in a stream → discarded after the first via advance-until-qname-changes.
- stream already at EOF (`current() == None`) → skip the instance (2730).
- `AS`/`MD` missing on a mapped record → Perl `die` (2838); enforce here (Phase 3 left them `None`).
- **`flag==4` then the *next* line has the same read-id → `die`** (2747–49): an unmapped marker must not be
  followed by another line for the same read.
- **no-store ambiguous path** (2840–2853): when `second_best` is defined and `AS == second_best` but
  `AS != best_AS_so_far`, the alignment is **not stored** (only `amb_same_thread` may be set, conditionally)
  and the rest of the read's lines are discarded.

## 4. Signature (proposed)

```rust
pub enum Decision {
    UniqueBest(BestAlignment),  // chromosome, position, index, AS, AS_2nd, md, cigar, bowtie_seq, mapq
    Ambiguous,                  // -> Phase 6 routes to --ambiguous/--unmapped/none
    NoAlignment,
    Rejected,                   // directional wrong-strand (index 2/3)
}
pub struct Counters { /* Phase 4: unique_best_alignment, unsuitable_sequence, no_single_alignment_found,
                         alignments_rejected, sequences_count. The per-strand CT_CT/CT_GA/GA_CT/GA_GA
                         counters are added in Phase 5 (incremented in genomic extraction, behind the
                         chromosome-edge guards). */ }

/// Drive the instances for one read; advances the matching streams past it.
pub fn check_results_single_end(
    identifier: &str, sequence: &str,
    streams: &mut [AlignerStream], directional: bool,
    score_min: ScoreMin, counters: &mut Counters,
) -> Result<Decision>;

// mapq.rs
pub fn calc_mapq(read1_len: usize, read2_len: Option<usize>, as_best: i64, as_second: Option<i64>, sm: ScoreMin) -> u8;
```

## 5. Implementation outline

1. **Config prereq:** add `score_min_intercept`/`score_min_slope` (f64) to `RunConfig`, parsed in
   `options.rs` (default 0 / −0.2; reuse the `--score_min` validation) and populated in `resolve()`.
2. `mapq.rs`: `calc_mapq` — transcribe the end-to-end ladder verbatim from 3945–~4078; unit-test against
   Perl-computed values. (Local branch: `unimplemented!()`/guard — `--local` already rejected.)
3. `merge.rs`: `Decision`, `BestAlignment`, `Counters`, and `check_results_single_end` — the §3 logic
   (per-instance scan, `alignments` map keyed `chr:pos`, ambiguity, unique-best, directional rejection,
   `calc_mapq`). Uses Phase-3 `AlignerStream::current()`/`advance()`.
4. The driver: re-read the original FastQ (gz/plain), and per read derive the identifier =
   `convert::fix_id(header)` **then strip one leading `@`** (Perl 2442), `uc` the sequence, `sequences_count++`,
   and replicate `--skip`/`--upto` (Perl 2433-ish). Call the merge per read; tally `Counters`. Wire into the
   SE-directional pipeline in `lib::run` (convert → spawn 2 streams → drive merge → print a counters
   summary; **no BAM**).
5. Tests (see §9) — merge units with fake/canned streams + `calc_mapq` units.

## 6. Efficiency

Linear in reads × instances; the streams are already buffered (Phase 3). `alignments` map has ≤ 4 entries.
Not optimizing further this phase (alignment CPU is Bowtie 2's).

## 7. Integration

- **Consumes** Phase-3 `AlignerStream`s + Phase-2 converted reads + the re-read originals; **produces**
  per-read `Decision` + `Counters`.
- **Feeds Phase 5:** `Decision::UniqueBest` is the input to genomic-seq extraction + `XM` + BAM output.
- **Feeds Phase 6:** the `Counters` + the Ambiguous/NoAlignment outcomes drive the reports + the
  unmapped/ambiguous output files.
- **Wires into `run()`** for the SE-directional spine (the first time streams + originals join); output is a
  counters summary only until Phase 5 adds the BAM.

## 8. Assumptions

**From epic (shared):** Perl v0.25.1 oracle + Bowtie 2 2.5.5; output fully Bismark-generated; byte-identity
gate is the Phase-5 BAM (Phase 4 is verified by selection + counters matching Perl on known inputs); gate
adjudicated on Linux/oxy.

**Phase-specific:**
- SE directional, 2 instances (index 0/1). Non-dir/pbat (4 instances) + PE = later phases; the directional
  index-2/3 rejection is included but inert here.
- `--score_min` numeric intercept/slope threaded into `RunConfig` (additive; default 0 / −0.2).
- `calc_mapq` end-to-end branch only (`--local` rejected); ported **verbatim** (the exact MAPQ integers are
  byte-identity-critical — they land in the BAM's MAPQ column in Phase 5).
- The driver's `fix_id` MUST be the same one Phase 2 used (lockstep keys) — reuse `convert::fix_id`.

## 9. Validation

| # | Verify | How | Expected |
|---|--------|-----|----------|
| 1 | unique best (1 instance aligns, other `flag==4`) | merge unit, canned streams | `UniqueBest`, correct index/chr/pos; `unique_best_alignment_count=1` |
| 2 | unique best across 2 instances (different AS) | unit | higher-AS instance wins; second-best set |
| 3 | cross-instance tie (equal AS, diff loci) | unit | `Ambiguous`; `unsuitable_sequence_count=1` |
| 4 | same-thread ambiguity (`AS==second_best==best`) | unit | `Ambiguous` |
| 5 | same-location dedup (both instances, same chr:pos) | unit | one `alignments` entry → `UniqueBest`, not ambiguous |
| 6 | no alignment (both `flag==4`) | unit | `NoAlignment`; `no_single_alignment_found=1` |
| 7 | RNAME de-conversion | unit | `chr1_CT_converted` → `chr1`; missing suffix → error |
| 8 | Phase-4 counters | unit | `unique_best_alignment_count`/`unsuitable_sequence_count`/`no_single_alignment_found`/`alignments_rejected_count` correct. **Strand counters (CT_CT…) are NOT touched in Phase 4** (asserted absent here; tested in Phase 5). |
| 9 | **lockstep key (`@`-strip)** | unit: original header `@r1 1:N` vs SAM qname `r1_1:N` | identifier matches the stream qname (would be 100% NoAlignment without the strip) |
| 10 | `flag==4` then same-id next line | unit | `die`/error (2747) |
| 11 | `calc_mapq` — **per-branch pinned cases** | unit, table-driven: one case per leaf of BOTH ladders (no-second-best 7 leaves: 42/40/24/23/8/3/0; with-second-best ~30 leaves incl. every `bestOver == diff` arm) + a **non-integer `scMin`** case (e.g. readLen=51) | each = the exact Perl integer |
| 12 | directional rejection (PE-style index 2/3) | unit with a 4-stream canned setup | `Rejected`; `alignments_rejected_count=1` (documents the inert-on-SE path) |

## 10. Questions or ambiguities

- **(Open Q1)** Module split — `merge.rs` + `mapq.rs` (recommended) vs one module. *Assumption:* split.
- **(Open Q2)** `calc_mapq` validation goldens — hand-compute from the Perl ladder, or capture from a Perl
  run? *Assumption:* hand-compute pinned cases from the verbatim ladder (deterministic); the end-to-end
  Perl check rides Phase 10. Confirm.
- **(Open Q3)** Does Phase 4 wire into `run()` now (emit a counters summary) or stay a tested primitive
  until Phase 5? *Assumption:* wire it (this is the genuine join point; a counters summary is a real,
  inspectable deliverable) — but **no BAM** until Phase 5. Confirm.

## 11. Self-Review

- **Efficiency:** linear; ≤ 4-entry map; buffered streams. ✓
- **Logic:** per-instance scan + `overwrite`/`best_AS`/`amb_same_thread` + `alignments` dedup + unique-best
  sort-and-tie + directional rejection + `calc_mapq` all traced to 2702–3151 / 3923–4186. ✓
- **Edge cases:** unmapped, EOF stream, multi-line discard, same-location dedup, missing AS/MD die,
  cross-instance vs same-thread ambiguity. ✓
- **Integration:** `Decision` is the clean seam to Phase 5 (BAM/XM) + Phase 6 (reports/unmapped); driver
  reuses Phase-2 `fix_id` (no key drift) + Phase-3 streams. ✓
- **Risks:** (a) `calc_mapq` is a long verbatim ladder — a single transposed integer is a silent
  MAPQ-byte divergence at the Phase-5 gate → transcribe carefully + per-branch pinned tests (§9 #11). (b) the
  lockstep assumption (every read → a line per stream) must hold for real Bowtie 2 — verified at Phase 10.
  (c) floating-point in `calc_mapq` — **Reviewer A verified Perl 5 ≡ rustc 1.95 bit-identical f64**; use
  plain `f64` + exact comparisons (epsilon would BREAK parity), one `scMin` binding.

## 12. Revision History

- **rev 1 (2026-06-01)** — folded in dual plan-review (`PLAN_REVIEW_A.md`/`PLAN_REVIEW_B.md`; both found the
  same **2 Criticals**, no rework). Source-verified:
  - 🔴 **Strand counters → Phase 5.** `CT_CT`/`CT_GA`/`GA_CT`/`GA_GA` are incremented in
    `extract_corresponding_genomic_sequence_single_end` (~4400–4441) behind chromosome-edge guards, NOT in
    the merge. Phase 4 keeps only `unique_best`/`unsuitable`/`no_single_alignment`/`alignments_rejected`/
    `sequences_count`. (§3.2, §4, §9 #8.)
  - 🔴 **Lockstep key `@`-strip.** Match identifier = `fix_id(header)` THEN `s/^\@//` (Perl 2442); without
    the strip every read → NoAlignment. (§2, §5, §9 #9.)
  - **Second-best assignment** = the exact `defined && strict >` conditional (3075), not `max()`. (§3.2.)
  - **Driver** carries `sequences_count` + `--skip`/`--upto`; added the `flag==4`-then-same-id `die` (2747)
    and the no-store ambiguous path (2840–53). (§3 edge cases, §5, §9 #10.)
  - **`calc_mapq` float semantics** confirmed: plain `f64` + exact comparisons + one `scMin` binding;
    validation expanded to per-branch pinned cases + a non-integer-`scMin` case. (§3.3, §9 #11, §11.)
- **rev 0 (2026-06-01)** — initial plan.

## 13. Implementation Notes (2026-06-01)

**Status: IMPLEMENTED & verified — 67 unit + 15 integration tests green; clippy `-D warnings` + fmt clean.**

- New modules: `mapq.rs` (`calc_mapq` — the end-to-end ladder transcribed verbatim; `#[allow(clippy::float_cmp)]`
  with a comment, since exact f64 `==` matches Perl) and `merge.rs` (`check_results_single_end` →
  `Decision`/`Counters`, generic over a new `align::SamStream` trait so it's unit-testable with a canned
  `VecStream` double — no subprocess needed). `align.rs` gained the `SamStream` trait (impl'd by
  `AlignerStream`). The driver (`pipeline`/`run_se_directional`/`drive_merge` in `lib.rs`) **wires the
  SE-directional spine into `run()`**: convert → spawn 2 instances (CT/`--norc`, GA/`--nofw`) on the C→T
  file → re-read originals → merge → counters summary (no BAM).
- **Both Criticals implemented:** strand counters are NOT touched in Phase 4 (deferred to Phase 5);
  lockstep key = `convert::fix_id(header)` then strip leading `@`. Plus the exact 3075 second-best
  conditional, the `flag==4`-same-id die, `sequences_count`/`skip`/`upto` in the driver.
- **Config prereq done:** `RunConfig.score_min_intercept`/`score_min_slope` (default 0 / −0.2) via
  `options::score_min_params` (splits on the last comma, Perl-greedy).
- **Tests:** `merge` units (unique-best, cross-instance tie, same-thread ambiguity, same-location dedup,
  no-alignment, missing-suffix error, `flag==4`-die, directional index-2 rejection) via `VecStream`;
  `mapq` per-branch pinned cases (no-2nd / with-2nd / non-integer-scMin / user-slope). `happy_path`
  integration now drives the full pipeline and asserts the merge summary.
- **Deviation (documented):** the `tests/cli.rs` fake `bowtie2` was upgraded to emit one unmapped SAM
  record per input read (in addition to `--version`), because `run()` now actually aligns — the prior
  version-only fake would have produced an unparseable "SAM" line.
- **Carried forward:** Phase 5 adds genomic-seq extraction + `XM`/`XR`/`XG` + BAM output + the per-strand
  CT_CT/… counters (behind the chromosome-edge guards); `is_unmapped()==flag==4` is SE-only (PE = Phase 7).

### Post-review (2026-06-01)

Dual code-review (both **APPROVE**, no Critical/High — Reviewer A even ran a Perl-vs-Rust differential
harness over the `calc_mapq` inner leaves: identical) + plan-manager (**COMPLETE**). Felix authorised the
recommended test additions (both reviewers converged on the same gap; pure test-strengthening):
- **`mapq::inner_threshold_leaves_pinned`** — a 30-case table pinning *every* with-second-best leaf
  (the 0.84/0.68 vs 0.88/0.67 sub-thresholds + the `==diff` arms + the 6/2 and 1/0 branches), so a future
  `0.88`↔`0.84` typo can't pass green (the silent-MAPQ-divergence risk).
- **`merge`**: `second_best_uses_best_own_when_greater_than_runner_up` (the 3075 arm),
  `three_instances_picks_highest` (3 stored entries), `too_many_hits_errors` (`>4` → die).
- Low items left as documented (fail-closed `finish()`, `score_min_params` strictness, CRLF-in-QNAME → Phase 10).
- **Final totals: 71 unit + 15 integration tests; clippy `-D warnings` + fmt clean.**

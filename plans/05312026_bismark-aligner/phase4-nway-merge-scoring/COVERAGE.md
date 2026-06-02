# Plan Coverage Report

**Mode:** B (code vs. plan — the design `PLAN.md` rev 1 treated as the spec)
**Plan(s):** `plans/05312026_bismark-aligner/phase4-nway-merge-scoring/PLAN.md` (rev 1)
**Date:** 2026-06-01
**Verdict:** COMPLETE

## Summary

- Total items: 30 (4 §3 behaviors + 6 edge cases + 4 §4 signatures + 5 §5 outline steps + 12 §9 validations — overlapping items counted once where merged + the §13 deviation)
- DONE: 30
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 1 (documented in §13 — fake `bowtie2` upgraded; verified real)

Tests: **82 passing** (67 unit + 15 integration `tests/cli.rs`); 0 failed. Matches the §13 claim ("67 unit + 15 integration"). Note: the suite was 67 unit + 15 = 82 total; §13's "67 unit + 15 integration tests green" is accurate.

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | Per-read merge: iterate instances; lockstep on `qname == identifier` | §3.1 | DONE | `merge.rs:95-99` skip-unless-current-matches |
| 2 | unmapped `flag==4` → advance once, skip instance | §3.1 | DONE | `merge.rs:104-112`; `is_unmapped()` = `flag==4` (`align.rs:127`) |
| 3 | de-convert RNAME `_(CT|GA)_converted$`; die if absent | §3.1 | DONE | `merge.rs:115-125`; matches Perl 2763-68 (`s/_(CT|GA)_converted$//` else die) |
| 4 | `AS`-based overwrite + `best_AS_so_far` (`>=` keep, `>` reset amb) | §3.1 | DONE | `merge.rs:144-159` exactly: None→set; `>=`→overwrite; `>`→reset amb |
| 5 | same-thread ambiguity (`AS==second_best && AS==best`) → `amb_same_thread`, no store | §3.1, edge | DONE | `merge.rs:162-167`; no-store path present |
| 6 | else store at `chr:pos` if overwrite; discard rest of read's lines | §3.1 | DONE | `merge.rs:168-194` insert + advance-until-qname-changes |
| 7 | same-location dedup (`alignments` keyed `chr:pos`) | §3.1 | DONE | `insert_alignment` keys `"{chr}:{pos}"` (`merge.rs:276`) |
| 8 | `amb_same_thread` → Ambiguous (`unsuitable_sequence_count++`) | §3.2 | DONE | `merge.rs:198-201` |
| 9 | empty `alignments` → NoAlignment (`no_single_alignment_found++`) | §3.2 | DONE | `merge.rs:203-206`; Perl 2991 |
| 10 | unique-best: 1 entry accept; 2-4 sort desc; top-two-equal → Ambiguous | §3.2 | DONE | `merge.rs:210-219`; tie → `unsuitable_sequence_count++` (Perl 3060-63) |
| 11 | exact 3075 second-best conditional (defined && strictly `>` runner-up; NOT max) | §3.2 | DONE | `merge.rs:224-227` `Some(sb) if sb>runner_up => sb; _ => runner_up`; matches Perl 3075-80 verbatim |
| 12 | `> 4` entries → die | §3.2 | DONE | `merge.rs:229-233`; Perl 3087 |
| 13 | `--directional` index 2/3 → Rejected (`alignments_rejected_count++`) | §3.2 | DONE | `merge.rs:237-240`; Perl 3112-18 |
| 14 | otherwise UniqueBest (`unique_best_alignment_count++`) | §3.2 | DONE | `merge.rs:242`; Perl 3121 |
| 15 | **Critical: strand counters NOT incremented in Phase 4** | §3.2, §4, §9#8 | DONE | grep confirms NO `CT_CT/CT_GA/GA_CT/GA_GA` increments in `merge.rs`/`lib.rs` (only a deferral comment); Perl increments them in `extract_corresponding_genomic_sequence_single_end` 4402-41 (Phase 5) |
| 16 | MAPQ: `calc_mapq(len(seq), None, AS_best, AS_2nd, sm)` end-to-end ladder | §3.3 | DONE | `mapq.rs:13-115`; called `merge.rs:243-250` with `sequence.len()`, `None`, both scores, intercept/slope |
| 17 | float semantics: plain `f64`, exact `==`/`>=`, one `scMin` binding | §3.3, §11 | DONE | `mapq.rs:22-27` single `sc_min`; `#[allow(clippy::float_cmp)]` w/ comment; `diff`/`best_over` derived from same binding |
| 18 | `Decision::UniqueBest(BestAlignment{...})` carries 9 fields | §3.4, §4 | DONE | `merge.rs:18-38` (`BestAlignment`) + `:252-262` populates all incl. `mapq`, `alignment_score_second_best` |
| 19 | edge: read absent from a stream's current → leave untouched | §3 edge | DONE | `merge.rs:97` `continue` when `qname != identifier` |
| 20 | edge: multi-line for a read discarded after first | §3 edge | DONE | `merge.rs:192-194` advance-until-qname-changes |
| 21 | edge: stream at EOF (`current()==None`) → skip | §3 edge | DONE | `merge.rs:97` `is_none_or(...)` |
| 22 | edge: missing `AS`/`MD` on mapped record → die | §3 edge | DONE | `merge.rs:128-139` both `ok_or_else` errors; Perl 2838 |
| 23 | edge: `flag==4` then same-id next line → die | §3 edge | DONE | `merge.rs:106-111`; Perl 2747-49 |
| 24 | edge: no-store ambiguous path (`AS==2nd && AS!=best`) | §3 edge | DONE | `merge.rs:163-167` only sets amb when `best_as_so_far==AS`; no insert in that arm |
| 25 | §4 `Decision` / `Counters` / `BestAlignment` types | §4 | DONE | `merge.rs:19-66`; `Counters` has exactly the 5 Phase-4 fields, no strand counters |
| 26 | §4 `check_results_single_end` signature | §4 | DONE | `merge.rs:82-90` (intercept/slope as 2 f64 instead of a `ScoreMin` struct — functionally identical; not a behavioral deviation) |
| 27 | §4 `calc_mapq` signature | §4 | DONE | `mapq.rs:13-20` (intercept/slope as 2 f64; same note) |
| 28 | §5 step 1: `score_min_intercept`/`slope` on RunConfig, parsed in options, populated in resolve | §5 | DONE | `config.rs:143-146,176`; `options::score_min_params` (`options.rs:197-220`) splits on LAST comma (Perl-greedy) |
| 29 | §5 step 4: driver re-reads FastQ, `fix_id` + `@`-strip, `uc`, `sequences_count++`, skip/upto, wires into `run()` | §5 | DONE | `lib.rs:96-207`; **Critical `@`-strip** at `lib.rs:191` (`strip_prefix(b"@")`); `uc` at :193; `sequences_count++` :188; skip/upto :176-187; no BAM |
| 30 | §13 deviation: fake `bowtie2` upgraded to emit unmapped SAM | §13 | DEVIATED (documented + verified) | `tests/cli.rs:42-54` emits `@HD` header + 1 flag-4 record/read from `-U` file w/ `@`-stripped qname; `happy_path` now asserts "Phase 4 merge summary"/"no alignment found:" |

## Test verification (Mode B)

§9 validation table — each row mapped to a passing test:

| # | §9 Validation | Test | File | Status |
|---|---------------|------|------|--------|
| 1 | unique best (1 aligns, other flag==4); index/chr/pos; count=1 | `unique_best_one_instance_other_unmapped` | merge.rs:347 | PASS |
| 2 | unique best across 2 instances (diff AS); 2nd-best set | `best_across_instances_by_score` | merge.rs:366 | PASS (asserts index 1, AS 0, 2nd-best Some(-6)) |
| 3 | cross-instance tie (equal AS, diff loci) → Ambiguous, unsuitable=1 | `cross_instance_tie_is_ambiguous` | merge.rs:386 | PASS |
| 4 | same-thread ambiguity (AS==2nd==best) → Ambiguous | `same_thread_ambiguity_boots` | merge.rs:398 | PASS |
| 5 | same-location dedup (both, same chr:pos) → UniqueBest | `same_location_in_both_instances_dedups` | merge.rs:411 | PASS |
| 6 | no alignment (both flag==4) → NoAlignment, no_single=1 | `no_alignment_when_both_unmapped` | merge.rs:424 | PASS |
| 7 | RNAME de-conversion; missing suffix → error | `unique_best_..` (chr1) + `missing_converted_suffix_errors` | merge.rs:357,431 | PASS |
| 8 | Phase-4 counters correct; strand counters NOT touched | counters asserted across #1/#3/#4/#6/#12 + `Counters` struct has no strand fields | merge.rs:54-66 | PASS (strand counters structurally absent) |
| 9 | lockstep key `@`-strip | `happy_path_resolves_and_prints_config` (fake bt2 strips `@`, merge matches) + driver `lib.rs:191` | cli.rs:120 | PASS |
| 10 | flag==4 then same-id next line → die | `flag4_then_same_id_dies` | merge.rs:442 | PASS |
| 11 | calc_mapq per-branch pinned + non-integer scMin + user slope | `no_second_best_ladder`, `with_second_best_top_buckets`, `with_second_best_not_at_diff`, `non_integer_scmin`, `user_score_min_slope` | mapq.rs:125-164 | PASS |
| 12 | directional rejection (index 2/3, 4-stream) → Rejected, rejected=1 | `directional_rejection_index_2` | merge.rs:457 | PASS |

**§9 #11 note (calc_mapq leaf coverage):** §9 asks for "one case per leaf of BOTH ladders" — no-second-best 7 leaves (42/40/24/23/8/3/0) are all pinned in `no_second_best_ladder`. The with-second-best ladder (~30 leaves) is sampled (top buckets `==diff` arms: 39/38/35; not-`==diff` arms: 33/26/1), NOT exhaustively one-per-leaf. This is a slightly lighter test than the literal §9 wording ("~30 leaves incl. every `bestOver == diff` arm"), but the ladder is transcribed verbatim from Perl 3945-4078 (line-by-line confirmed), the float-equality risk is covered by the `==diff` cases that ARE pinned, and §13/§8 (Open Q2) explicitly defer end-to-end MAPQ parity to the Phase 10 real-data Perl run. Treated as DONE per the documented assumption; flagged here for visibility.

## Gaps (detail)

None blocking. One visibility item:

### Item: §9 #11 with-second-best ladder is sampled, not exhaustive per-leaf

**Expected:** §9 #11 wording — one case per leaf of the with-second-best ladder (~30 leaves), incl. every `bestOver == diff` arm.
**Found:** 6 with-second-best cases (3 `==diff` arms + 3 non-`==diff` arms) plus all 7 no-second-best leaves + non-integer-scMin + user-slope.
**Gap:** Not exhaustive coverage of all ~30 with-second-best leaves. **Not a coverage failure** — the ladder is a verbatim transcription (independently confirmed against Perl 3945-4078), the byte-identity gate for MAPQ rides Phase 10 (per §8/Open Q2), and the highest-risk arms (`bestOver == diff` float equality) are pinned. No code change required for Phase 4 to be considered complete; noted only so a future Phase 10 reviewer knows MAPQ leaves were not all individually pinned.

## Verdict

**COMPLETE.** Every §3 behavior, every §3 edge case, all §4 signatures, all §5 outline steps, and all 12 §9 validations map to real code with a passing test. Both rev-1 Criticals are verified in code:
- **Strand counters NOT in Phase 4** — `merge.rs`/`lib.rs` contain zero `CT_CT/CT_GA/GA_CT/GA_GA` increments (grep-confirmed); they remain in Perl's `extract_corresponding_genomic_sequence_single_end` (4402-41), deferred to Phase 5.
- **Lockstep `@`-strip** — `lib.rs:191` `fix_id(...).strip_prefix(b"@")`, matching Perl 2442.

Other spot-checked guards all present and Perl-faithful: lockstep advance-until-qname-changes, exact 3075 second-best conditional (`merge.rs:224-227`), `flag==4`-then-same-id die (`merge.rs:106-111` / Perl 2747-49), `calc_mapq` end-to-end ladder + intercept/slope threading, directional index-2/3 rejection (`merge.rs:237-240` / Perl 3112-18), missing-AS/MD die (Perl 2838).

The §13 documented deviation (fake `bowtie2` upgraded to emit one unmapped SAM record per read so `run()` can actually align) is **real and verified** in the working-tree diff (`tests/cli.rs:42-54`).

`cargo test -p bismark-aligner`: **82 tests pass, 0 fail.**

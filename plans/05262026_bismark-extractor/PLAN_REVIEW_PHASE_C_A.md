# Phase C Plan Review — Reviewer A

**Reviewer:** A (fresh context window, no shared state)
**Plan reviewed:** `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PHASE_C_PLAN.md` rev 0
**Date:** 2026-05-26
**Verdict (preview):** see end of file.

---

## Summary

Phase C is a focused, well-scoped extension of Phase B's SE pipeline into PE, with a header-level auto-detect and per-mate ignore-trim wiring. The polarity work in `drop_overlap` is well-grounded against both Perl source and the SPEC §7.4 rev 2 locking, and the new error variants are appropriate. **One critical defect found** (silent `--no_overlap` regression in AutoDetect mode), a handful of important nits, and a few optional cleanups.

---

## 1. Logic review

### 1.1 Critical — `no_overlap` default is wrong when AutoDetect resolves to PE

Phase A's `Cli::validate` (verified at `rust/bismark-extractor/src/cli.rs:445-449`) computes:

```
let no_overlap = if paired_mode == PairedMode::PairedEnd {
    !self.include_overlap
} else {
    false
};
```

This is `false` whenever `paired_mode != PairedEnd`. With Phase C's AutoDetect dispatch, the flow is:

1. User runs `bismark-extractor input.bam` with no mode flag.
2. `Cli::validate` resolves `paired_mode = AutoDetect`, so `no_overlap = false`.
3. `main::run` opens the header, finds PE, calls `extract_pe(input, &config)`.
4. `extract_pe` sees `config.no_overlap == false` → **skips `drop_overlap`** → R2 overlap calls leak into output.

This **silently violates the locked plan decision** in §2 ("Default `--no_overlap` for PE: ON — matches Perl `--no_overlap` default") **only for the AutoDetect path**. Explicit `--paired-end` is fine because Phase A's branch covers it. The plan does not call this out anywhere.

**Required fix (one of):**

- (a) Change the Phase A resolution to `paired_mode != PairedMode::SingleEnd`. Then SE keeps `no_overlap=false` (irrelevant for SE), explicit `--paired-end` keeps current behaviour, AutoDetect inherits the PE default. Cleanest.
- (b) In `main::run`'s AutoDetect arm, after `detect_paired_from_header` returns `Some(true)`, mutate `config.no_overlap = !cli_include_overlap` before calling `extract_pe`. Requires keeping the raw `include_overlap` flag (currently consumed by `Cli::validate`).
- (c) In `extract_pe`, treat `no_overlap` as a synonym for "PE AND NOT include_overlap" by re-checking at runtime. Hides the Phase A bug rather than fixing it.

This must be resolved before merge. Recommend (a) — touches one branch in cli.rs and adds one test (`validate_auto_detect_keeps_no_overlap_default`).

### 1.2 Important — `is_forward_pair_strand` classification is correct but worth a one-line citation

§5.1 declares `is_forward_pair_strand` returns `true` for `OT | CTOB`. This matches both:
- The SPEC `route_call` strand-idx table (OT=0, CTOT=1, CTOB=2, OB=3 — but that's routing, not strand-orientation).
- The Perl PE control flow: forward branch is taken when R1's strand-tag indicates OT or CTOB.

The reasoning is that for **directional + non-directional libraries**, R1's `record_strand` tells us whether R1's mapped reference position is the upstream end of the insert (OT/CTOB: R1 is upstream, R2 is downstream → use R1's `reference_end` as the cutoff for R2) or the downstream end (OB/CTOT: R1 is downstream, R2 is upstream → use R1's `alignment_start` as the cutoff for R2). This is correct, but it's worth a one-line code comment citing the Perl line numbers (2400/2415) where this branching is set up so a future reader doesn't have to reconstruct it.

### 1.3 Important — `run_extraction<F>` closure signature is under-specified

§5.2 declares:

```rust
F: FnOnce(&mut bismark_io::AnyReader, &[String], &mut ExtractState) -> Result<(), BismarkExtractorError>
```

`AnyReader` is generic: `AnyReader<R: BufRead, RC: Read + Seek>`. The closure signature as-written won't compile without bounds; it needs the concrete `AnyReader<BufReader<File>, File>` returned by `open_reader`, or generic params propagated through `run_extraction`. Either is fine, but a generic `run_extraction` raises monomorphisation cost; the concrete-type approach is simpler. Pin the choice in the plan or in code.

Also: passing `&mut AnyReader` + `&mut ExtractState` simultaneously is fine since they're disjoint, but the closure body needs to call `reader.records()` which mutably borrows `reader`. That's compatible with `FnOnce(&mut reader, ..., &mut state)`. Just noting the borrow choreography is non-trivial and should be exercised by `cargo check` early in implementation.

### 1.4 Important — refactor risk in `extract_se` is non-zero

Phase B's `extract_se` has 5 distinct early-return + cleanup sites (record-iter error, PAIRED-flag rejection, refid-missing, refid-out-of-range, route_call err) plus the per-iteration `records_processed +=`. The plan's `run_extraction<F>` closure body for SE must reproduce all five with identical cleanup-on-err semantics. The risk is not catastrophic (Phase B's 44 SE tests gate it), but the plan's framing ("behaviour-preserving refactor") understates the cost.

**Alternative considered:** keep `extract_se` exactly as-is and add a parallel `extract_pe` that duplicates the open/build_chr_table/state/cleanup scaffolding (~30 LOC duplication). Trade-off: small code duplication vs. zero refactor risk to a Phase B path that's already merged-pending-review. Given Phase C is on the critical path and Phase B isn't merged yet, my recommendation is:

- **If Phase B merges before Phase C implementation starts:** the `run_extraction<F>` refactor is fine; Phase B's tests are the safety net.
- **If Phase C is implemented while Phase B is still in review:** duplicate the scaffolding in `extract_pe` and defer the refactor to a follow-up PR. Phase B's review will not have to re-verify the SE path on top of a still-changing helper.

Plan §15 says Phase C is stacked on Phase B and will rebase. Acceptable either way, but call out the contingency in §6 step 5.

### 1.5 Important — counter ordering in PE under `--mbias_only`

Plan §4.1's pseudocode increments `state.report.records_processed += 1` **after both `route_call` batches**. That's pair-counting, not record-counting — consistent with Open Q #2's "default plan: count pairs (one increment per pair)". Good.

But `route_call`'s own splitting-report counters (`calls_by_context` / `calls_by_context_meth`) are incremented twice per pair (once per R1 call, once per R2 call). That's correct (per-call counts), but worth verifying the splitting-report's final line wording. Perl writes `Processed N lines in total` (verified at Perl line 2479) — for PE input N is the **line count** (= 2 × pairs). So if Phase C reports `records_processed = pair_count`, the splitting report will say half the lines that Perl reports.

**This contradicts Plan Open Q #2's tentative answer.** Look at Perl line 2479 + the rest of the PE branch:

```
warn "\nProcessed $counting{sequences_count} lines in total\n";
```

`sequences_count` is incremented per BAM line read. For PE that's 2 lines per pair. **Phase C's record count must increment by 2 per pair, not 1**, or the splitting-report's "Processed N lines in total" will be half Perl's value → byte-identity fails at Phase H.

**Recommendation:** change §4.1 pseudocode to `state.report.records_processed = state.report.records_processed.saturating_add(2)` (or rename to `lines_processed` for clarity), AND add a Phase C test `pe_splitting_report_counts_lines_not_pairs` that asserts the report value is 2 × pair count for a PE input with N pairs. Resolve Q #2 in the plan as "lines, per Perl line 2479", not deferred.

### 1.6 Optional — cross-chr pair check placement

§4.5 inlines the cross-chr check in `handle_one_pair`. Defensible because it's an extractor-specific concern (dedup doesn't care). However, `BismarkPair::from_mates` already validates qname-eq + R1/R2-identity as PE-pair structural invariants. Cross-chr is also a PE-pair structural invariant (Bismark never emits cross-chr pairs). The architectural question: should `from_mates` enforce it too, for symmetry?

**Recommendation:** keep extractor-side for Phase C (don't expand the Phase C diff), but file a small follow-up issue in `bismark-io` to add it to `from_mates` in v1.0.0-beta.8 — then the extractor can drop its inline check. Note this in §11.

### 1.7 Optional — error variant naming consistency

`CrossChromosomePair` is a bit verbose. Other errors in the variant list are `MateMismatch`, `ReadIdentityMismatch`, `UnpairedFinalRecord`. `MateChromosomeMismatch` or `PairChromosomeMismatch` would match the naming pattern. Minor.

---

## 2. Assumptions review

### 2.1 SPEC §7.4 polarity — verified independently

I cross-checked the Perl source:

- Line 2905: `if ($start+$index+$pos_offset >= $end_read_1) { return; }` — forward, **inclusive skip**.
- Line 2987: `if ($start-$index+$pos_offset <= $end_read_1) { return; }` — reverse, **inclusive skip**.

The inverse keep predicate is strict `<` and `>`. Plan §4.2 + SPEC §7.4 rev 2 pseudocode agree. **Polarity is correct.**

I also verified `$end_read_1` semantics by tracing back through Perl lines 2400, 2415, 1944. For forward pairs, `$end_read_1 = $start_read_1 + $MDN_count_1 - 1` (1-based inclusive last reference position — matches `CigarExt::reference_end`). For reverse pairs, `$end_read_1 = $start_read_1` BEFORE Perl shifts `$start_read_1 += MDN_count - 1` — i.e. `$end_read_1` is the **original R1 alignment_start** (1-based leftmost). Plan §4.2's `r1_ref_start = pair.r1().alignment_start()` for the reverse branch matches this. **Endpoint semantics are correct.**

### 2.2 `CigarExt::reference_end` invariant — verified

Read `rust/bismark-io/src/cigar.rs:182`: `if span == 0 { start } else { start + span - 1 }`. 1-based inclusive. For R1 `50M2D50M` at start=100: `reference_span = 50 + 2 + 50 = 102`, `reference_end = 100 + 102 - 1 = 201`. Plan §7.1 test `drop_overlap_with_r1_indel_uses_reference_end` asserts this. **Correct.**

### 2.3 PE record adjacency — appropriately stated as assumption

§9.1 locks this. Bismark always emits PE BAM in adjacent R1/R2 order (QNAME-sorted by alignment); `BismarkPair::from_mates` validates qname-equality so a sort breakage produces a loud `MateMismatch`. Acceptable.

### 2.4 `iter_aligned`'s R2 5'-orientation — assumed implicitly

§4.4 says "the kernel applies the trims in 5'-oriented read coordinates ... iter_aligned's orientation correction handles this transparently". This is load-bearing for `--ignore_r2` and `--ignore_3prime_r2` correctness on reverse-strand R2 reads (i.e. the OT-pair case where R2 is reverse-mapped). The plan asserts it without citation. I did not re-verify; recommend that the implementation step include a quick `cargo test -p bismark-io test_iter_aligned_reverse_strand` re-read to confirm.

Plan tests `extract_pe_per_mate_ignore_r2_only_skips_r2_positions` and `_3prime_r2` will catch a regression here, but they're written at the call-routing level, not the iter_aligned level. **Recommend adding one extra fixture**: an OT-pair where R2 is reverse-mapped + `--ignore_r2 3`, asserting the **first three read-cycles** (not the first three reference-positions) are skipped. This is the polarity check.

### 2.5 Open Q #1's deferral to a single test fixture — insufficient

§9.2 #1 says the in-test fixture `drop_overlap_with_r1_indel_uses_reference_end` covers endpoint semantics. One test with one InDel topology (mid-read deletion) is thin. Recommend adding:

- `drop_overlap_with_r1_end_deletion`: R1 `49M2D1M` at 100 → reference_end=151. R2 at 150, 151, 152.
- `drop_overlap_with_r1_insertion_shifts_read_pos_only`: R1 `50M2I50M` at 100 → reference_span=100, reference_end=199 (insertion does NOT consume reference). R2 at 198, 199, 200.

These cover three of the four CIGAR-relevant cases (mid-read del, end-of-read del, insertion). N+1 doesn't really cost anything and shores up the Phase H byte-identity invariant.

---

## 3. Efficiency review

### 3.1 AutoDetect double-open is negligible

Plan §8 quantifies ~50 ms for header re-open. For BAM, opening a noodles reader reads the BGZF header (a few KB) and parses `@SQ`/`@PG` lines. At 55M-record scale (~30-60 min runtime), 50 ms is invisible. Don't optimize.

The cleaner alternative — peek the header from the first reader and keep it for the loop — would require either threading a pre-opened `AnyReader` into `run_extraction` (changes the signature) or making `run_extraction` accept an `Option<AnyReader>`. Not worth the API complexity for ~50 ms.

### 3.2 `Vec::collect` in `drop_overlap` — acceptable for Phase C

§8 quantifies ~14 GiB total allocation at 27M pairs. Phase F may want to switch to in-place filtering or extend a worker-local buffer, but that's a Phase F concern. Phase C is fine.

### 3.3 `BismarkPair::from_mates` allocates `BismarkRecord` twice — actually doesn't (just consumes by value)

Reading `rust/bismark-io/src/pair.rs:40`: `from_mates(r1: BismarkRecord, r2: BismarkRecord)` consumes by value, stores by move. No extra allocation. Plan §8 says "BismarkPair::from_mates allocates 2 BismarkRecords" — minor inaccuracy; it just **stores** two records that were already allocated by the iterator. Cosmetic; doesn't affect plan validity.

---

## 4. Validation sufficiency

### 4.1 Phase B regression coverage — covered by existing tests

Plan §10's "Phase B regression" row leans on `cargo test -p bismark-extractor` showing all 91 tests green. Good. Recommend adding `extract_se_handles_two_well_formed_records` as an explicit Phase C-added regression test (the SE counterpart to `extract_pe_handles_two_well_formed_pairs`) so the test directory has matched coverage.

### 4.2 PE-specific coverage gaps to fill

| Gap | Test to add |
|-----|-------------|
| Non-directional library PE (CTOT/CTOB pair_strand): plan only tests OT and OB | `extract_pe_routes_ctot_pair_strand_correctly` + `..._ctob` mirror. Verify R1's record_strand for a non-directional aligner output is CTOT or CTOB, and that overlap detection branches correctly (CTOT is forward-class? — verify against `is_forward_pair_strand`'s OT|CTOB classification). |
| Zero-length pair (both reads soft-clipped fully): `extract_calls` returns empty Vec for each; pair iteration must not panic | `extract_pe_handles_fully_soft_clipped_pair` |
| PE input with `--ignore_r2 3 --ignore_3prime_r2 3` simultaneously (compound trim) | `extract_pe_compound_ignore_trims` |
| Lines-vs-pairs counting (per §1.5 above) | `pe_splitting_report_counts_lines_not_pairs` |
| Reverse-strand R2 ignore polarity (per §2.4 above) | `extract_pe_ignore_r2_skips_read_cycles_not_ref_positions` |

R1/R2 with different XM lengths is impossible in well-formed Bismark output (XM length always equals SEQ length, and R1/R2 SEQ lengths are independent), so no test needed.

### 4.3 Auto-detect coverage

Tests cover: PE→PE, SE→SE, no-Bismark→error. Missing: **PE BAM where the @PG line is malformed** (e.g. truncated `-1` arg). `detect_paired_from_header` returns `Some(false)` in that case (treats as SE) — verify that's the desired behaviour vs returning `None`. If it's intended as "fall back to SE", document in the plan; if it's a sharp edge, add a stricter probe or surface a warning. Minor.

---

## 5. Alternatives

### 5.1 Inline overlap check vs. extract_calls-level filter

The plan does overlap-drop as a post-processing step: extract all R2 calls, then filter. An alternative is to pass `Option<r1_ref_end>` into `extract_calls` itself and skip early. The post-processing approach is simpler and aligns with Perl's structure (Perl's overlap check is also a per-call test inside the per-call loop). Stick with the plan.

### 5.2 `extract_se` + `extract_pe` shared scaffolding via `run_extraction<F>` vs. duplicate

Discussed in §1.4. The plan's choice is fine if Phase B is merged first. If not, duplicate.

### 5.3 Cross-chr check at `from_mates` vs. extractor — discussed in §1.6

---

## 6. Phase-F readiness

§11 calls out pair-adjacency as a Phase F requirement. One additional snag: `run_extraction<F>` takes a `FnOnce` body. Under Phase F's producer/consumer split, the body becomes "spawn producer thread + spawn N consumer threads + join". That doesn't compose with `FnOnce(&mut reader, ..., &mut state)` because the reader and state can't be `&mut`-shared across threads. Phase F will likely have to rework `run_extraction` into something like `run_extraction_serial` + `run_extraction_parallel`, or drop the abstraction entirely. **Not a Phase C blocker**, but worth noting in §11 so Phase F's plan-writer doesn't inherit an awkward refactor.

---

## 7. Action items

### Critical (must fix before merge)

1. **`no_overlap` resolution in AutoDetect path** (§1.1). Either fix `Cli::validate` to set `no_overlap = !include_overlap` when `paired_mode != SingleEnd`, or re-resolve in `main::run` after auto-detect. Add a regression test.
2. **Lines-vs-pairs counting in splitting report** (§1.5). Per Perl line 2479, increment `records_processed += 2` per PE pair (or rename to `lines_processed`). Add a test asserting Perl-compatible count. Resolve Open Q #2 in the plan.

### Important (should fix)

3. **Clarify `run_extraction<F>` closure signature** (§1.3) — pin generic vs. concrete `AnyReader<BufReader<File>, File>`.
4. **Extend the InDel-aware endpoint test** to 3 fixtures (mid-read del, end-of-read del, insertion) (§2.5).
5. **Add `is_forward_pair_strand` code comment** citing Perl line 2400/2415 (§1.2).
6. **Decide refactor-vs-duplicate based on Phase B merge status** (§1.4) and document the contingency in §6 step 5.
7. **Add reverse-strand R2 ignore-polarity test** (§2.4 + §4.2 row).
8. **Add non-directional library PE tests** (§4.2 row).
9. **Add `pe_splitting_report_counts_lines_not_pairs` test** (§4.2 row, ties to action item 2).

### Optional (nice to have)

10. File a follow-up issue to move cross-chr check into `BismarkPair::from_mates` for v1.0.0-beta.8 (§1.6).
11. Consider renaming `CrossChromosomePair` to `MateChromosomeMismatch` (§1.7).
12. Document the malformed-PG-line behaviour of `detect_paired_from_header` (§4.3).
13. Note Phase F's `FnOnce` snag in §11 (§6).
14. Cosmetic correction to §8's "from_mates allocates 2 BismarkRecords" line (§3.3).

---

## Verdict

**NEEDS-REVISIONS.**

Two critical issues — the AutoDetect `no_overlap` regression (§1.1) and the PE pair-vs-line counter mismatch (§1.5) — both silently produce wrong output and will block byte-identity at Phase H. Both have small fixes; once those land plus a small number of additional tests, the plan is ready to implement.

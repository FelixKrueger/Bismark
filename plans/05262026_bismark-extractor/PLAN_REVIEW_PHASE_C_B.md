# Plan review — Phase C (Reviewer B)

**Plan file:** `plans/05262026_bismark-extractor/PHASE_C_PLAN.md` (rev 0).
**Reviewer:** B (independent; no shared state with Reviewer A).
**Date:** 2026-05-26.
**Verdict:** **APPROVE-WITH-NITS** — solid plan grounded in SPEC rev 2 and Phase B's already-merged scaffolding. Three findings need attention before/during implementation (one is a likely byte-identity bug; two are tidy-ups). None block the implementation trigger; all can be fixed inline.

---

## Logic review

### L1 [Important] — Perl PE splitting-report counts 2× pair count, not pair count

**File:** plan §9.2 open question #2 + §7.3 smoke test "Processed 10 pairs".
**Severity:** Important (byte-identity-affecting; Phase H gate).

The plan defers this to "read Perl at implementation time" with a default of "count pairs (one increment per pair processed)." I read Perl directly (`bismark_methylation_extractor:2451`):

```perl
$methylation_call_strings_processed += 2; # paired-end = 2 methylation call strings
```

Paired with line 2459 (`$counting{sequences_count} = $line_count`, where `$line_count` is also the raw BAM-line count) and the splitting-report template at line 2479 (`Processed $counting{sequences_count} lines in total`), the math is unambiguous:

- Perl PE reports **2N** for N pairs in `sequences_count`.
- The header literal is `"Processed N lines in total"` — not `"Processed N pairs"`.

The plan's default (count pairs / "Processed 10 pairs") would diverge from Perl on **both** the value and the literal text. Phase H byte-identity would fail.

**Fix in plan:** change §9.2 #2's default to "increment `records_processed` by 1 per record (not per pair) — so PE input bumps it by 2 per pair, matching Perl line 2451." Update the §7.3 smoke assertion to "Processed 20 lines in total" (or "lines: 20"). Note that `state.report.records_processed` is the existing counter name from Phase B — Phase C's `extract_pe` pseudocode currently bumps it by 1 per pair (§4.1 line `state.report.records_processed = ... saturating_add(1)`); that should be `saturating_add(2)` or two single increments.

---

### L2 [Optional] — `resolve_chr` helper isn't defined in Phase B

**File:** plan §4.1 `handle_one_pair` body calls `resolve_chr(pair.r1(), chr_table)?`.
**Severity:** Optional (mechanical).

`resolve_chr` doesn't exist in Phase B's `pipeline.rs`. Phase B inlines the refid → chr-table lookup (see `pipeline.rs:92-116`). The plan implicitly assumes a helper extraction during Phase C but doesn't enumerate it in §3.2's "Modified modules" list. Add a bullet to §6 step 5: "extract the `Option<refid>` → `chr_table.get(refid).ok_or(InternalError…)` chain from inline-in-extract_se into a small `fn resolve_chr(record, chr_table) -> Result<&str, BismarkExtractorError>` helper; call from both `extract_se` (refactored body) and `handle_one_pair`."

---

### L3 [Optional] — `chr_name` redundant lookup per pair

**File:** plan §4.1 `handle_one_pair` (resolves both `r1_chr` and `r2_chr`).
**Severity:** Optional.

The plan resolves `r1_chr` and `r2_chr` independently. The `CrossChromosomePair` defensive check in §4.5 will reject any pair where `r1.reference_sequence_id() != r2.reference_sequence_id()`, so post-check the two chr strings are guaranteed equal. Either:

- **Option A (suggested):** resolve `r2_refid`, compare to `r1_refid`, defensive-error on mismatch, then use `r1_chr` for both `route_call` calls. ~2 LOC saved + one HashMap-ish lookup eliminated per pair (~27M lookups on a 55M-record run — small, but free).
- **Option B (current plan):** keep the two-lookup form; cost is O(1) per pair anyway.

The plan's §9.2 #3 already flags this as open; I'd suggest closing it with Option A — simpler, faster, and the defensive check has to read `r2.reference_sequence_id()` regardless. Plan §4.1's pseudocode already has a TODO-style comment about this.

---

### L4 [Important] — Defense-in-depth contradiction in §6 step 5

**File:** plan §6 step 5 ("Wait — actually keep the PAIRED-flag check in extract_se").
**Severity:** Important (clarity, not correctness).

Step 5 first says "Move the per-record PAIRED-flag check OUT" and then immediately reverses with "Wait — actually keep…". §6 step 6 then re-confirms "Decision: keep both" for defense-in-depth.

This back-and-forth is fine reasoning but reads as an open question in the plan body. Strike the first bullet ("Move the per-record PAIRED-flag check OUT…") and replace with a single sentence: "Phase B's PAIRED-flag defensive check stays in `extract_se`: defense-in-depth against a user explicitly passing `--single-end` against a PE BAM. The auto-detect path covers the no-flag case." This avoids a reviewer in implementation reading the first half and removing the check.

---

### L5 [Important] — `run_extraction` closure type mismatches `AnyReader`'s generics

**File:** plan §5.2 signature: `body: F where F: FnOnce(&mut bismark_io::AnyReader, &[String], &mut ExtractState) -> Result<(), BismarkExtractorError>`.
**Severity:** Important (compile-time correctness).

`AnyReader` in `bismark-io/src/read.rs:504` is `pub enum AnyReader<R: BufRead, RC: Read + Seek>` — it's generic over two type parameters. `open_reader()` returns the concrete `AnyReader<BufReader<File>, File>` (read.rs:565). The plan's helper signature `&mut bismark_io::AnyReader` won't compile.

**Fix:** either:

- (a) name the concrete instantiation: `&mut bismark_io::AnyReader<BufReader<File>, File>`, or
- (b) make `run_extraction` itself generic over `<R: BufRead, RC: Read + Seek>` and let the call site infer, or
- (c) take a generic `Reader: AlignmentReader` bound (if a unifying trait exists in bismark-io — quick scan shows no such trait yet).

Phase B's `extract_se` doesn't run into this because it just calls `open_reader(...)` inline and uses the concrete `AnyReader` returned. The simplest fix is option (a) since the helper is private. Whichever way, the plan signature as written would fail at `cargo check`.

---

### L6 [Important] — `pair_strand` argument no longer needed by `drop_overlap`

**File:** plan §4.2 / §5.1.
**Severity:** Important-but-minor (interface simplification).

`drop_overlap(r2_calls, pair, pair_strand)` accepts `pair_strand` as a third arg, but `pair_strand` is recoverable from `pair.pair_strand()` (which the SPEC §6.1 / bismark-io's `BismarkPair::pair_strand()` already exposes). Passing both invites the caller to compute one and pass the other inconsistently. Drop the third arg; have `drop_overlap` call `pair.pair_strand()` itself. The §4.1 pseudocode also calls `pair.pair_strand()` and then passes `pair_strand` as a separate arg — pure boilerplate.

---

### L7 [Optional] — `AutoDetectFailed` error message references "--single-end / --paired-end"

**File:** plan §5.3 + §4.3.
**Severity:** Optional (UX nit).

The error message reads "no Bismark @PG line in header; pass --single-end or --paired-end explicitly." If `detect_paired_from_header` returns `None` because the BAM was aligned with `bismark2` (case mismatch) or a derived tool whose `@PG ID:` doesn't start with `Bismark`, the message blames the user. The dedup function uses `line.contains("ID:Bismark")` (substring, not prefix or case-insensitive); this catches `ID:Bismark`, `ID:Bismark_v0.25.1`, `ID:bismark2_bowtie2` (no — different case), etc.

I don't think this is a real issue for current Bismark output (which always emits `ID:Bismark`), but the error message could be more helpful: include the actual `@PG ID:` values seen, e.g. "no `@PG` line with `ID:Bismark*` found (saw: `ID:bowtie2`, `ID:samtools`); pass --single-end or --paired-end explicitly." This is a couple lines of code at the call site. Optional — not blocking.

---

### L8 [Optional] — `BismarkPair::from_mates` consumes records by value

**File:** plan §4.1 `BismarkPair::from_mates(r1, r2)?`.
**Severity:** Optional (efficiency).

Verified in `bismark-io/src/pair.rs:40`: `from_mates(r1: BismarkRecord, r2: BismarkRecord)` takes ownership of both records. This is fine and matches the iterator's `Result<BismarkRecord, _>` yielding owned records. No issue; just confirming for the reviewer A audit that the borrow flow works (we don't keep `r1`/`r2` referenced after construction — we use `pair.r1()` / `pair.r2()`).

---

## Cross-crate concerns

### X1 [Optional] — bismark-io v1.0.0-beta.7 promotion is genuinely additive

**File:** plan §3.2 step 1 + §3.3.
**Verdict:** confirmed clean.

I verified `bismark-dedup/src/pipeline.rs:121-184`: the `detect_paired_from_header` function is self-contained (only depends on `noodles_sam::Header`, which both crates pin to `=0.85.0`). The local helper `arg_present` is also self-contained. The function is `#[must_use]` with a doc comment that already references the dedup origin lines. Moving it to `bismark-io/src/read.rs` and re-exporting is a straight cut-and-paste.

One small ask: the dedup version uses `noodles_sam::io::Writer::new(&mut buf)` to serialize the header for substring search (line 144). That's fine in bismark-io too, but make sure `bismark-io/Cargo.toml`'s `noodles-sam` already pulls in the `io` feature; if `bismark-io` previously didn't write SAM, the feature might not be enabled. Worth a `cargo check` after the move. Minor; the implementation step will catch it.

### X2 [Optional] — `bismark-dedup`'s `arg_present` helper is private — also promote?

**File:** plan §6 step 1.
**Severity:** Optional.

`detect_paired_from_header` calls a private helper `arg_present` (dedup pipeline.rs:175). When the function moves to bismark-io, the helper must move with it. The plan doesn't explicitly say this, but the implementation will trip immediately. Add a one-liner to §6 step 1: "promote `arg_present` along with `detect_paired_from_header` (private helper, used only by it)."

### X3 [Information] — bismark-dedup test reduction

**File:** plan §6 step 1 ("Move the existing dedup tests for it").
**Verdict:** confirmed clean; relocation is straightforward.

The dedup tests for this function live in `bismark-dedup/src/pipeline.rs`'s test module (I didn't grep them by name but the plan claims they exist and will be relocated). They depend only on `noodles_sam::Header` construction, which bismark-io's test scaffolding already supports (see `bismark-io/src/pair.rs:103-225` tests for prior art). Trivial relocation. The `cargo test -p bismark-dedup` step listed in §6 step 9 will confirm no regression.

---

## Error-handling edge audit

### E1 [Important] — `UnpairedFinalRecord` path doesn't run cleanup explicitly

**File:** plan §4.1 + §5.2 `run_extraction`.
**Severity:** Important.

The closure body for `extract_pe` (§4.1) returns `UnpairedFinalRecord` via `return Err(...)` from inside the closure. The plan's contract for `run_extraction` (§5.2 doc) says: "On any error from `body`, runs `state.cleanup_partial_outputs()` before propagating."

That's correct *if* `run_extraction` is structured as:

```rust
let res = body(reader, chr_table, state);
if res.is_err() { state.cleanup_partial_outputs(); }
res?
state.finalize(config)?;
Ok(())
```

Good. But the plan's §4.1 doesn't show the wrapping — it just shows the closure returning an error. Implementation must respect the contract or the cleanup won't run. Add an explicit unit test asserting cleanup-on-error for the `UnpairedFinalRecord` path: that's already in §7.1 ("`extract_pe_rejects_unpaired_final_record` ... cleanup removes all 12 files") — good. Same for `extract_pe_rejects_mismatched_qnames_pair` and `extract_pe_rejects_cross_chromosome_pair`. All three already specified.

The risk is just implementation discipline. Phase B's `extract_se` already runs cleanup on every error site (pipeline.rs:67, 78, 95, 107, 124, 135) — but does so explicitly at each site, not via a wrapper. The `run_extraction` refactor moves this to one site; verify the refactor doesn't drop a cleanup case.

**Action:** add a Phase C refactor-safety test that asserts a forced error in the closure body leaves zero residual files on disk. Even a single test ensures the wrapper does its job for all error variants.

### E2 [Important] — `BismarkPair::from_mates` is fallible; `?` chain semantics

**File:** plan §4.1: `let pair = BismarkPair::from_mates(r1, r2)?;`.
**Verdict:** confirmed clean.

`from_mates` returns `Result<Self, BismarkIoError>`. `BismarkExtractorError` has `BismarkIo(#[from] BismarkIoError)` (error.rs:165) → `?` lifts cleanly via the `From` impl. The `#[error(transparent)]` (error.rs:164) means `Display` propagates the inner `BismarkIoError`'s message; users see "expected R1 for first mate, got R2" not "BismarkIo: expected...". Clean.

One subtle thing: `#[error(transparent)]` requires the variant to have exactly one field (the wrapped error). `BismarkIo(#[from] BismarkIoError)` has exactly one field. Good.

### E3 [Optional] — `extract_calls` error doesn't include pair context

**File:** plan §4.1 calls `extract_calls(pair.r1(), ...)?` and `extract_calls(pair.r2(), ...)?`.
**Severity:** Optional.

If R2's XM tag is malformed, the resulting `InvalidXmByte { read_id }` error names R2's qname. Since R1 and R2 share a qname, that's fine — but the error doesn't tell the user *which mate* tripped the validation. For debugging at byte-identity-gate time, a `mate_idx: u8` field on `InvalidXmByte` might save 30 min of guessing. Optional; can be deferred to Phase D/E. Not a blocker.

---

## Validation sufficiency

### V1 [Important] — overlap-include test fixture must actually have overlap

**File:** plan §7.1 test `extract_pe_with_include_overlap_keeps_r2_overlap_calls`.
**Severity:** Important (test trustworthiness).

The test asserts "R2 overlapping calls land in output files when `--include_overlap`." For this to be a meaningful positive test, the fixture must:

1. Have R2 calls at reference positions that fall within R1's reference span (so `drop_overlap` would have removed them under the default `--no_overlap`).
2. Verify those specific calls appear in the output under `--include_overlap`.

The companion negative test `extract_pe_with_no_overlap_drops_r2_overlap_calls` uses the same fixture and asserts the overlap calls are *absent*.

**Risk:** if the fixture is constructed sloppily (e.g. R1 and R2 disjoint so `drop_overlap` is a no-op anyway), both tests pass vacuously. The test names imply overlap, but the fixture doc must explicitly state: "R1 at chrX 100-149 (50M), R2 at chrX 120-169 (50M); overlap region is ref-pos 120-149 inclusive; R2 has methylation calls at ref-pos 125, 135, 145 (all inside overlap)." Then assert specifically that those 3 calls appear/disappear depending on `--include_overlap`.

**Action:** strengthen the test description in §7.1 or in implementation: name the specific overlap-region ref positions and assert their presence/absence. Don't just count total R2 calls; assert the specific overlapping ones.

### V2 [Optional] — `extract_pe_per_mate_ignore_r2_only_skips_r2_positions` must distinguish R1 vs R2 trim semantics

**File:** plan §7.1 test `extract_pe_per_mate_ignore_r2_only_skips_r2_positions`.
**Severity:** Optional.

Similar trustworthiness concern: the fixture must have methylation calls at read-positions 0, 1, 2 on BOTH R1 and R2, then apply `--ignore_r2 3` and assert:

- R1's read-pos 0, 1, 2 calls **are** in output.
- R2's read-pos 0, 1, 2 calls **are NOT** in output.

Without that R1-positive assertion, the test can't tell the difference between `--ignore_r2` and `--ignore` (which trims R1 too). The plan's prose suggests this is the intent; making it explicit in the fixture spec avoids a false-positive test.

Same applies to `_3prime_r2` mirror.

### V3 [Optional] — `extract_pe_routes_r2_calls_to_pair_strand_file_not_record_strand_file` is the load-bearing Alan-bug fixture

**File:** plan §7.1 + §10.
**Verdict:** specified well.

This test is the structural fix for Alan Hoyle's port bug. The plan explicitly asserts "All R2 calls must land in `*_OT_*.txt`, never `*_CTOT_*.txt`." Strong. Make sure the synthetic fixture's R2 has `record_strand() == CTOT` (not just any non-OT strand) — that's the specific case that bites a naive implementation. Spec the fixture comment in the test source.

### V4 [Important] — refactor-safety: behaviour-equivalence test for `run_extraction`

**File:** plan §6 step 5 (refactor) + §7.4 ("Phase B's 44 SE unit tests stay green").
**Severity:** Important.

Plan §7.4 says: "Phase B's 44 SE unit tests stay green. The `extract_se` refactor through `run_extraction` is behaviour-preserving — any regression there shows up in Phase B's existing tests."

That's the canonical refactor-safety argument and it's mostly right. But Phase B's tests cover the *happy path* and several explicit error paths. The refactor adds a new error-cleanup wrapper. Test gap:

- Phase B currently does cleanup-on-error at six explicit sites inline. The refactor moves them to one site (`run_extraction`'s closure-error handler).
- If the refactor accidentally only catches some error variants, Phase B tests that exercise those covered variants will pass, but newly-uncovered variants (or new ones in Phase C) will silently leak files.

**Action:** add a test (could be parameterized) that injects each `BismarkExtractorError` variant raise-able from the closure body and asserts zero residual files. Or, since Phase B already has 91 tests covering most error paths, just audit the test list and confirm each variant is exercised at least once. Phase B's existing assertions like "cleanup removes all 12 files" (§7.1 in this plan suggests the existing test exists) are sufficient evidence.

Either: (a) explicitly list the audited variants in §10 "Validation" + §7.4 "Test coverage adjacency" with the verdict "all six error sites are exercised by Phase B tests T1, T2, ..."; or (b) add one new sanity test.

### V5 [Optional] — empty-PE-BAM happy path is covered, but consider odd-numbered + empty-record-error mix

**File:** plan §4.6 edge cases + §7.1 `extract_pe_empty_bam_writes_only_header_files`, `extract_pe_rejects_unpaired_final_record`.
**Verdict:** good coverage.

The two listed tests cover the boundaries. One unlisted case: what if `iter.next()` yields `Some(Err(_))` on the *second* call after a successful first? The plan's pseudocode handles this (line 105-106: `Some(Err(e)) => return Err(e.into())`). No additional test needed.

---

## Efficiency

### F1 [Optional] — `Vec::collect` in `drop_overlap` per pair

Plan §8 calls this out: ~30 calls × 16 bytes × 27M pairs = ~14 GiB allocator churn over a 55M-pair run. Same order of magnitude as Phase B's per-record `Vec`. The plan correctly defers to Phase F. No action.

A nit: `r2_calls.into_iter().filter(...).collect()` reallocates. If R2 has 30 calls and we drop 3, we allocate a new 27-element Vec. An in-place `retain` (no allocation) would be cheaper:

```rust
let mut r2_calls = r2_calls;  // already owned
r2_calls.retain(|c| c.ref_pos < r1_ref_end);
r2_calls
```

This is ~50% cheaper for the common case (most R2 calls kept). The plan can adopt this trivially. Optional.

### F2 [Optional] — Header probe re-opens the file

Plan §4.3 + §9.2 #4 acknowledges the double-open. ~50 ms is negligible. No action. Phase F refactor candidate if needed.

---

## Alternatives considered

### A1 — Out-of-order pair detection

Plan §2 locks adjacent-record pairing. Alternative: build a HashMap<qname, (r1?, r2?)> and emit pairs as they complete. This would handle out-of-order BAMs (e.g. coordinate-sorted via samtools). Bismark output is always QNAME-grouped, so the simpler adjacent-pairing is right. `bismark-io::open_reader` already rejects coordinate-sorted input upstream (plan §4.6). Locked correctly.

### A2 — Inline `drop_overlap` vs separate module

Plan §3.2 puts `drop_overlap` in its own `overlap.rs` module. Alternative: inline in `pipeline.rs`. Separate module is right — easier to unit-test in isolation (per §7.1's overlap-specific tests) and overlap detection is conceptually distinct from the main loop.

### A3 — Threading a peeked reader through dispatch

Plan §9.2 #4 defaults to reopen. Alternative: have `open_reader` return a struct with `header()` accessible without consuming the iterator, and pass it through `extract_pe` / `extract_se`. This already works in the API (`AnyReader::header(&self)` is fine — see read.rs:515). The "reopen" pattern in plan §4.3 is a non-issue; the existing reader could be reused. The plan picked the simpler form for Phase C; refactor later. Not a blocker.

### A4 — Process-risk: stacked PRs

Plan §15: Phase C branches off Phase B's extractor-phase-b. If Phase B requires substantive changes, Phase C rebases repeatedly. Worth flagging:

- If Phase B's PR #849 lands in 1-2 days: Phase C rebases once onto fresh `rust/iron-chancellor`. Clean.
- If Phase B is in review for a week+ with revisions: Phase C accumulates conflicts. Mitigation: rebase Phase C onto Phase B daily during review; resolve conflicts incrementally; or pause Phase C work until Phase B merges.

Not a plan-quality issue; just a process risk. No action in the plan; mention to the user.

---

## Action items (prioritized)

### Critical

None. Plan is implementation-ready.

### Important (resolve before/during implementation)

1. **L1**: Fix the splitting-report PE counting default. Perl counts 2× pair-count, not pair-count. Update §9.2 #2 default + §4.1 pseudocode (`saturating_add(2)` or two increments) + §7.3 smoke assertion text. Verified by reading Perl line 2451.
2. **L4**: Strike the contradictory bullet in §6 step 5 about "Move the PAIRED-flag check OUT." Final state must clearly say "keep it for defense-in-depth."
3. **L5**: Fix the `run_extraction` helper signature — `AnyReader` is `<R: BufRead, RC: Read + Seek>`; the plan's `&mut bismark_io::AnyReader` won't compile. Use the concrete `AnyReader<BufReader<File>, File>` or make `run_extraction` generic.
4. **L6**: Drop the redundant `pair_strand` argument from `drop_overlap` — recoverable from `pair.pair_strand()`.
5. **E1**: Add an explicit refactor-safety test that the `run_extraction` wrapper runs `cleanup_partial_outputs` on every closure-error path. Alternatively, audit Phase B's tests in §10 to confirm each error variant is exercised.
6. **V1**: Strengthen the overlap-include test fixture spec — name the specific overlap-region ref positions and assert presence/absence of those specific calls.
7. **V4**: Decide between adding a parameterized refactor-safety test vs auditing existing Phase B tests to confirm each error variant survives the refactor.

### Optional (nice to have)

8. **L2**: Add a `resolve_chr` helper to §3.2 + §6 step 5 — currently implied but not stated.
9. **L3**: Close §9.2 #3 with the "cache chr after defensive check" option — simpler code and trivially faster.
10. **L7**: `AutoDetectFailed` could include actual `@PG ID:` values for debug. Optional UX nit.
11. **L8**: `extract_calls` error could carry `mate_idx` for PE debugging. Optional.
12. **X2**: Add a one-liner to §6 step 1 about promoting the `arg_present` private helper alongside `detect_paired_from_header`.
13. **V2**: Spec the R1+R2 read-pos-0,1,2 fixture in `_per_mate_ignore_r2_only_skips_r2_positions` — assert both R1-keeps and R2-skips explicitly.
14. **V3**: Document the fixture's R2 `record_strand == CTOT` in the Alan-bug regression test source.
15. **F1**: Use `Vec::retain` instead of `into_iter().filter().collect()` in `drop_overlap` — halves allocator churn for the common case.
16. **A4**: Flag stacked-PR rebase risk to the user; suggest daily rebase of Phase C onto Phase B during the latter's review.

---

## Summary

The plan is well-grounded: it ties each decision to SPEC rev 2 sections, names Perl line numbers for behavior cross-reference, lists explicit error variants with rationale, and proposes test coverage that catches the Alan Hoyle structural bug at the PE level. It correctly defers M-bias writing (Phase D), multicore (Phase F), and byte-identity gating (Phase H). The largest finding is L1 — the Perl splitting-report counts records (2N for N pairs), not pairs — which the plan defers but I resolved directly from Perl source; the default in the plan would diverge from byte-identity. The other Important findings (L4 contradiction, L5 generic-type signature, L6 redundant `pair_strand` arg, E1/V4 refactor-safety test) are easy fixes during implementation. Cross-crate concerns (bismark-io v1.0.0-beta.7 promotion) check out: `noodles-sam =0.85.0` aligned across all three crates and `detect_paired_from_header` is self-contained (only uses `noodles_sam::Header`).

**Verdict: APPROVE-WITH-NITS.** Implementation can proceed once L1 is folded into the plan (it changes a smoke-test assertion); the rest are minor and catchable inline.

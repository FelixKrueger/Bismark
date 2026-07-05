# `bismark-extractor` Phase C — PE extraction + overlap detection + per-mate ignore

**Status:** rev 2 — implementation complete + dual code-review folded. 537 tests pass across 3 crates, clippy + fmt clean. Ready to commit + PR.
**Date:** 2026-05-26.
**Slug:** `plans/05262026_bismark-extractor/PHASE_C_PLAN.md`.
**Phase target:** SPEC §10 row C — ~600 LOC (rev 1 estimate stable; eager-open + 2 Phase A helper changes net out).
**GitHub sub-issue:** [#850](https://github.com/FelixKrueger/Bismark/issues/850) (filed at work-start).
**Depends on:** [PR #849](https://github.com/FelixKrueger/Bismark/pull/849) (Phase B). Stacked branch `extractor-phase-c` is based on `extractor-phase-b`; rebases onto `rust/iron-chancellor` once Phase B merges. **Process risk (rev 1, Reviewer B A4):** if Phase B's review extends into a week+ with substantive changes, Phase C may need daily rebase. Mitigation: rebase Phase C onto Phase B daily during the latter's review, or pause Phase C implementation until #849 merges.

## Revision history

- **rev 0** (2026-05-26): initial Phase C plan.
- **rev 2** (2026-05-26): post-implementation close-out folding 5 plan-manager MISSING items + cosmetic nits from both code-reviewers.
  - **Plan-manager MISSING #1-#7 (7 tests added in `tests/pe_phase_c.rs::pe_e2e`)**:
    - `extract_pe_routes_ctot_pair_strand_correctly` — non-directional CTOT pair (R1.record_strand=CTOT, pair_strand=CTOT, reverse class); all calls route to `*_CTOT_*.txt`, never `*_OT_*.txt` (which is R2's per-record strand). Closes Alan-Hoyle split-across-files bug at non-directional library level.
    - `extract_pe_routes_ctob_pair_strand_correctly` — mirror for CTOB pair (forward class). New helpers `helpers::ctot_pair` + `helpers::ctob_pair` added.
    - `extract_pe_per_mate_ignore_r2_only_skips_r2_positions` — `--ignore_r2 3` skips R2's 5'-end read cycles but leaves R1's intact. Verified via OT pair with `--include_overlap` so R2's calls can be observed independently.
    - `extract_pe_per_mate_ignore_3prime_r2_only_skips_r2_3prime` — 3'-end mirror.
    - `extract_pe_ignore_r2_skips_read_cycles_not_ref_positions` — reverse-strand polarity gate (rev 1 Reviewer A §2.4). Pins read-cycle vs ref-position semantics on a CTOT-strand R2.
    - `extract_pe_increments_mbias_R2_at_index_1` — PE-level re-verification of R2→mbias[1] routing. Phase B's `route_call_r2_goes_to_mbias_index_1` already locks it at unit level; this exercises the PE composition through the binary.
    - `extract_pe_empty_bam_writes_only_header_files` — empty PE BAM → 12 header-only files + "Processed 0 lines" report.
  - **Reviewer A L1 / Reviewer B Err2 — RESOLVED.** Dropped `#[allow(unused_imports)]` from `bismark-dedup/src/pipeline.rs:30` `pub use bismark_io::detect_paired_from_header`. The `pub use` re-export is itself a use; the allow was unnecessary.
  - **Reviewer A L2 / Reviewer B L2 — RESOLVED.** Added Perl line citations (`bismark_methylation_extractor:2905 / :2989`) to the polarity-discovery test bodies (`drop_overlap_disjoint_pair_drops_all_r2_calls_downstream_of_r1_end` + `extract_pe_with_no_overlap_drops_r2_calls_past_r1_end`).
  - **Reviewer B L1 — RESOLVED.** Tightened `pe_phase_c_smoke.rs` assertion from `call_lines >= 1` to `call_lines == 10` (pins overlap polarity at smoke level). Comment now explains R2's reversed call at BAM-pos 4 lands at ref `r1_start + 4 = r1_ref_end`, so strict-`<` keep drops it; all 10 R1 calls survive.
  - **Reviewer B Err3 — RESOLVED.** Added cleanup-completion assertion to `extract_pe_rejects_unpaired_final_record` (`fs::read_dir(&outdir).count() == 0` after failure). Sister test `extract_pe_rejects_cross_chromosome_pair` already had it.
  - **Deferred for follow-up PR (post-Phase-B merge):**
    - Reviewer A M1: dead-fixture preamble in `drop_overlap_with_r1_insertion_shifts_read_pos_only` — cosmetic, tracked.
    - Reviewer A L3: `run_extraction<F>` scaffolding refactor — already documented in `pipeline.rs:11-20` as Phase B-merge follow-up.
    - Reviewer A L4: cross-chr error to include chr names — minor ergonomic.
    - Reviewer A M2: nf-core/methylseq parser owners need to update for "reads" → "lines" — flag in PR description.
    - Reviewer B E2: `render_qname` dedup into helper — Phase D opportunity.
    - Reviewer B S2: scratch-work mid-test comment cleanup in insertion fixture — combined with A's M1.
  - **Verification (rev 2):** `cargo test -p bismark-io -p bismark-dedup -p bismark-extractor` → all green. `cargo clippy --all-targets -- -D warnings` (3 crates) → clean. `cargo fmt --check` (3 crates) → clean. Test totals (extractor): 40 lib + 4 sanity + 44 se_phase_b + **29 pe_phase_c** (was 22 in rev 1) + 3 se_phase_b_smoke + 2 pe_phase_c_smoke = 122 extractor tests. Plus bismark-io's 5 new `detect_paired_from_header` tests + bismark-dedup regression-free.

- **rev 1** (2026-05-26): folded both plan-review reports' findings.
  - **Reviewer A §1.1 (Critical) — RESOLVED.** AutoDetect `no_overlap` regression. Phase A's `Cli::validate` only sets `no_overlap = !include_overlap` for `paired_mode == PairedEnd`, leaving AutoDetect at `false`. With Phase C's AutoDetect→PE dispatch, this silently leaks R2 overlap calls. **Fix in Phase C scope:** change `cli.rs` to set `no_overlap = !include_overlap` for `paired_mode != SingleEnd`. Added to §3.2 "Modified modules" + new regression test `validate_auto_detect_keeps_no_overlap_default`.
  - **Reviewer A §1.5 / Reviewer B L1 (Important, byte-identity-affecting) — RESOLVED.** Splitting-report PE counter. Both reviewers independently grepped Perl: line 2451 `$methylation_call_strings_processed += 2; # paired-end = 2 methylation call strings`; line 2479 `"Processed $counting{sequences_count} lines in total"` reports raw BAM-line count, not pair count. Closes §9.2 open question #2: **count lines (2N for N pairs), not pairs**. Plan §4.1 pseudocode now `state.report.records_processed.saturating_add(2)` per pair (or two `+=1` increments alongside R1 and R2 routing). §7.3 smoke assertion updated to "Processed 20 lines in total" (10 pairs × 2). New test `pe_splitting_report_counts_lines_not_pairs`.
  - **Reviewer A §1.3 / Reviewer B L5 (Important) — RESOLVED.** `run_extraction<F>` signature. `AnyReader` is generic over `<R: BufRead, RC: Read + Seek>`; plan rev 0's `&mut bismark_io::AnyReader` won't compile. Pin the concrete instantiation `AnyReader<BufReader<File>, File>` (matches Phase B's `extract_se` use).
  - **Reviewer A §2.5 (Important) — RESOLVED.** Single InDel-aware overlap test fixture is thin. Added 2 more tests: `drop_overlap_with_r1_end_deletion` (R1 `49M2D1M`, R2 at the cliff edge) and `drop_overlap_with_r1_insertion_shifts_read_pos_only` (R1 `50M2I50M` — insertion consumes read but not reference, so reference_end is 199 not 201). Together with the existing mid-read deletion test these cover the three CIGAR-relevant InDel topologies.
  - **Reviewer A §2.4 + §4.2 (Important) — RESOLVED.** Reverse-strand R2 ignore-polarity test missing. Added `extract_pe_ignore_r2_skips_read_cycles_not_ref_positions`: an OT pair where R2 is reverse-mapped (record_strand == CTOT) with `--ignore_r2 3` — asserts the first three 5'-end **read cycles** are skipped, not the first three reference positions.
  - **Reviewer A §4.2 (Important) — RESOLVED.** Non-directional library PE coverage. Added `extract_pe_routes_ctot_pair_strand_correctly` + `..._ctob_pair_strand_correctly` — exercises overlap detection branching for CTOT (forward class per `is_forward_pair_strand`) and CTOB (reverse class).
  - **Reviewer A §1.4 (Important) — DECISION RECORDED.** Refactor-vs-duplicate contingency in §6 step 5 — refactor if Phase B has merged when Phase C implementation begins; else duplicate scaffolding to avoid concurrent-edit conflict.
  - **Reviewer A §1.2 (Important) — RESOLVED.** Added Perl line citation (2400/2415) for `is_forward_pair_strand` branching to §5.1.
  - **Reviewer B L4 (Important) — RESOLVED.** Stripped the contradictory "Move the PAIRED-flag check OUT … Wait, actually keep" bullet from §6 step 5. Final state: "Phase B's defensive PAIRED-flag check stays in `extract_se` for defense-in-depth."
  - **Reviewer B L5 (Important)** — same as Reviewer A §1.3 above.
  - **Reviewer B L6 (Important) — RESOLVED.** Dropped redundant `pair_strand` argument from `drop_overlap` (recoverable from `pair.pair_strand()`). §4.2 + §5.1 signatures updated.
  - **Reviewer B X2 (Important) — RESOLVED.** Added a bullet to §6 step 1: promote private `arg_present` helper alongside `detect_paired_from_header` (used only by it).
  - **Reviewer B E1 + V4 (Important) — RESOLVED.** Added `run_extraction_runs_cleanup_on_each_error_variant` test to §7.1 — injects each `BismarkExtractorError` variant raise-able from the closure body and asserts zero residual files. Plus an audit row in §10 validation cross-referencing Phase B's existing tests to confirm each error site survives the refactor.
  - **Reviewer B V1 (Important) — RESOLVED.** Strengthened the overlap-include test fixture spec. `extract_pe_with_include_overlap_keeps_r2_overlap_calls` now names specific ref positions: R1 50M at chrX:100-149, R2 50M at chrX:120-169, overlap region 120-149, R2 has calls at 125/135/145; assertion checks those specific calls appear under `--include_overlap` and disappear under default `--no_overlap`.
  - **Reviewer A §1.7 (Optional, included)** — renamed `CrossChromosomePair` → `MateChromosomeMismatch` for naming consistency with sibling variants (`MateMismatch`, `ReadIdentityMismatch`).
  - **Reviewer B L2 (Optional, included)** — added `resolve_chr` helper extraction to §3.2 + §6 step 5 (used by both `extract_se` refactored body and `handle_one_pair`).
  - **Reviewer B L3 (Optional, included) — RESOLVED §9.2 #3.** Cache chr name once per pair after the cross-chr defensive check; pass to both routing calls. Simpler code + one fewer HashMap lookup per R2 call.
  - **Reviewer B F1 (Optional, included)** — use `Vec::retain` in `drop_overlap` instead of `into_iter().filter().collect()`. ~50% cheaper allocator churn for the common case (most R2 calls kept).
  - **Reviewer A §6 / Phase F readiness note (Optional, included)** — added note to §11 about `run_extraction<F>`'s `FnOnce` body not composing with Phase F's producer/consumer split. Heads up for future planner.
  - **Reviewer B A4 (Optional, surfaced)** — process-risk note about stacked-PR rebases added to the top status block.
  - **Deferred (out of rev 1 scope):** Reviewer B L7 (AutoDetectFailed UX), Reviewer B L8 / Reviewer A §3.3 (cosmetic), Reviewer B E3 (mate_idx in InvalidXmByte), Reviewer A §1.6 (cross-chr to bismark-io v1.0.0-beta.8) — tracked as v1.x polish.

## Epic linkage

- **Design contract:** `rust/bismark-extractor/SPEC.md` (in-repo, rev 2). Sections covered in this plan: §6.1 (pair-strand once per pair), §7.3 (PE main loop), §7.4 (paired-overlap detection), §11 row "auto-detect" (SE vs PE).
- **GitHub umbrella:** issue [#798](https://github.com/FelixKrueger/Bismark/issues/798) (bismark-extractor port).
- **Prior phases:** Phase A merged (commit `144ca2d`, PR #847, closes #846). Phase B in review (PR #849, closes #848); Phase C builds on Phase B.

## 1. Goal

Light up the **paired-end extraction path**. Pair adjacent records into a `BismarkPair`, run the existing Phase B kernel + routing on each mate, drop R2 calls overlapping R1's reference span when `--no_overlap` is set (default for PE), apply per-mate ignore-region trims (`--ignore_r2`, `--ignore_3prime_r2`), and remove the SE pipeline's per-record `PAIRED`-flag rejection in favor of a header-level SE-vs-PE auto-detect from the `@PG ID:Bismark` line.

After Phase C, the binary handles SE + PE in `OutputMode::Default` on a single core; the remaining phases (D-H) widen this along orthogonal axes (M-bias writer, non-default modes, gzip, multicore, subprocess chain, byte-identity gate).

## 2. Scope decisions (locked)

| Decision | Choice | Reasoning |
|----------|--------|-----------|
| PE pairing scheme | **Adjacent-record pairing** via `BismarkPair::from_mates` (R1 then R2, qname-eq enforced) | Matches Perl `bismark_methylation_extractor`'s loop: Bismark always emits PE records in adjacent R1/R2 pairs. Out-of-order pair detection is unnecessary and would mask upstream sort bugs. |
| SE-vs-PE auto-detect | **Promote `detect_paired_from_header` from `bismark-dedup` to `bismark-io v1.0.0-beta.7`**, call once at reader-open time | SPEC §11; Reviewer A plan-review Optional #14 + Reviewer B confirmation. Removes the per-record PAIRED-flag check Phase B used. |
| Per-mate ignore trims | `--ignore_r2` / `--ignore_3prime_r2` map to `config.ignore_5p_r2` / `config.ignore_3p_r2` (already in `ResolvedConfig`) | No new CLI work; Phase A already parsed these. Phase C wires them into the per-mate `extract_calls` calls. |
| Overlap-detection polarity | **Strict `<` / `>` keep-predicate** (Perl writes inclusive `>=` / `<=` *skip* predicates; inverse) | Locked in SPEC §7.4 rev 2. Both reviewers verified against Perl 2905 + 2989. |
| Overlap-detection edge-cases | **Test fixture covering R2 calls at `r1_ref_end - 1`, `r1_ref_end`, `r1_ref_end + 1`** for both forward (OT/CTOB) and reverse (OB/CTOT) pair-strand groups | SPEC §7.4 explicitly deferred "endpoint semantics verification" to Phase C. |
| `--include_overlap` (keep R2 calls overlapping R1) | **Honored**; `config.no_overlap` is `false` when user passes `--include_overlap` (already resolved by Phase A) | Plain CLI plumbing. Inverts the drop_overlap call. |
| Final-record-unpaired error | **`BismarkExtractorError::UnpairedFinalRecord { qname }`** (new variant) | Pair iteration assumes even record count; an odd count means the BAM is malformed or sort got out of order. Loud failure with cleanup. |
| `extract_se` ↔ `extract_pe` shared scaffolding | **Extract a private `run_extraction<F>` helper** (Reviewer B PC4) for open_reader / build_chr_name_table / derive_basename / state.new / cleanup-on-err / state.finalize | Phase B's `extract_se` and the new `extract_pe` share ~80% of body code. Refactor in Phase C keeps both bodies thin. |
| `BismarkPair::from_mates` error mapping | Wrap as `BismarkExtractorError::BismarkIo(BismarkIoError::MateMismatch \| ReadIdentityMismatch)` — already `#[from]` | No new error variants needed for the pairing failure mode. |
| `ExtractParams` revival | **Not in Phase C.** Per-mate args (5-6) are still below the 14-arg threshold the SPEC §6.3 struct was designed to prevent. | Defer to Phase D/E if arg count grows further. |
| M-bias writer | **Out of scope.** Phase B already accumulates `state.mbias[1]` for R2; Phase D adds the writer. | SPEC §10 row D. |
| Default `--no_overlap` for PE | **ON** (matches Perl `--no_overlap` default) — fix Phase A's `Cli::validate` to resolve as `paired_mode != SingleEnd` (was `== PairedEnd` only, leaving AutoDetect at false). | Rev 1 Reviewer A §1.1 Critical: AutoDetect→PE dispatch would silently leak R2 overlap calls without this fix. |
| **Splitting-report PE counter** | **Lines, not pairs.** Increment `state.report.records_processed` by 2 per pair (or two `+=1` calls — one per R1 routing, one per R2 routing). Matches Perl `bismark_methylation_extractor:2451` (`$methylation_call_strings_processed += 2`) + line 2479 splitting-report literal `"Processed N lines in total"`. | Rev 1 Reviewer A §1.5 + Reviewer B L1 (both independently grepped Perl). Closes §9.2 open question #2. |

## 3. Context

### 3.1 Source documents read end-to-end

- `rust/bismark-extractor/SPEC.md` §§6.1, 6.5, 7.1 (referenced — kernel unchanged), **7.3**, **7.4**, 7.5 (referenced — route_call unchanged), 7.6 (ignore semantics already in extract_calls), 8.1 (test surface), 8.4 (edge case fixtures), 10, 11, 12.
- `rust/bismark-extractor/src/{call,mbias,output,state,route,header,pipeline,error,main,lib}.rs` — Phase B implementation (current PR #849).
- `rust/bismark-io/src/pair.rs` — `BismarkPair::from_mates`, `pair_strand()`, `r1()`, `r2()`.
- `rust/bismark-io/src/record.rs` — `BismarkRecord::record_strand`, `iter_aligned`.
- `rust/bismark-io/src/cigar.rs` — `CigarExt::reference_end(start: usize) -> usize` (returns 1-based inclusive last ref position).
- `rust/bismark-dedup/src/pipeline.rs:121-150ish` — `detect_paired_from_header` (to be promoted to `bismark-io v1.0.0-beta.7`).
- Perl `bismark_methylation_extractor` lines 1932-1944 (PE R1 reverse), 2877-2906 (PE forward overlap skip), 2976-2990 (PE reverse overlap skip), 1983-2330 (per-mate ignore application via CIGAR rewriting — Rust handles via boundary check in extract_calls).

### 3.2 Code placement

All Phase C code lands inside `rust/bismark-extractor/` (with one small change in `rust/bismark-io/`):

- **New modules**:
  - `rust/bismark-extractor/src/overlap.rs` — `drop_overlap` per SPEC §7.4 + helper `is_forward_pair_strand`.
- **Modified modules**:
  - `rust/bismark-extractor/src/cli.rs` — **rev 1 critical fix (Reviewer A §1.1):** change `Cli::validate`'s `no_overlap` resolution from `paired_mode == PairedEnd` to `paired_mode != SingleEnd`, so AutoDetect inherits the PE default. SE actual extraction ignores the field anyway (no overlap concept for SE).
  - `rust/bismark-extractor/src/pipeline.rs` — add `extract_pe`; refactor common scaffolding (open_reader, chr_table, basename, state, cleanup-on-error, finalize) into private `run_extraction` helper; SE main loop becomes `extract_se` calling `run_extraction(..., |reader, chr_table, state| { se_loop_body })`. Also extract a small `resolve_chr(record, chr_table) -> Result<&str, BismarkExtractorError>` helper (rev 1 Reviewer B L2) used by both SE and PE.
  - `rust/bismark-extractor/src/main.rs::run` — replace the SE-only path with config-dispatch on `PairedMode`: `SingleEnd → extract_se`, `PairedEnd → extract_pe`, `AutoDetect → call detect_paired_from_header at open_reader time → dispatch`. Keep Phase B's defensive per-record PAIRED-flag check inside `extract_se` for belt-and-suspenders (catches "user passed --single-end against a PE BAM" case).
  - `rust/bismark-extractor/src/error.rs` — add `UnpairedFinalRecord { qname: Option<String> }`, `MateChromosomeMismatch { qname, r1_refid, r2_refid }` (renamed from rev 0's `CrossChromosomePair` per Reviewer A §1.7 — matches `MateMismatch` / `ReadIdentityMismatch` naming), and `AutoDetectFailed { message }`.
  - `rust/bismark-extractor/src/route.rs` — **no signature change.** Phase B's `route_call` already takes `strand: BismarkStrand` + `read_identity: ReadIdentity`; PE callers pass `pair.pair_strand()` and `R1`/`R2` respectively.
  - `rust/bismark-extractor/src/lib.rs` — `pub mod overlap;` + re-export `drop_overlap`, `extract_pe`.
  - `rust/bismark-extractor/Cargo.toml` — version bump `1.0.0-alpha.2` → `1.0.0-alpha.3`; bump `bismark-io` dep from `=1.0.0-beta.6` to `=1.0.0-beta.7`.
- **In `bismark-io` (separate-but-coupled change in same PR)**:
  - `rust/bismark-io/src/read.rs` — promote `detect_paired_from_header` from `bismark-dedup/src/pipeline.rs:137` into `bismark-io` (public function). **Also promote the private `arg_present` helper** that `detect_paired_from_header` calls (dedup pipeline.rs:175 — used only by it; rev 1 Reviewer B X2). Move tests too.
  - `rust/bismark-io/src/lib.rs` — `pub use read::detect_paired_from_header;`.
  - `rust/bismark-io/Cargo.toml` — version `1.0.0-beta.6` → `1.0.0-beta.7`. Verify `noodles-sam` already pulls in the `io` feature (dedup version uses `noodles_sam::io::Writer` for header serialization in the substring search); `cargo check` after the move will catch any feature-gap.
  - `rust/bismark-dedup/src/pipeline.rs` — replace the local `detect_paired_from_header` + `arg_present` definitions with `use bismark_io::detect_paired_from_header;` (or call-site fully-qualified). Bump `bismark-io` dep in `bismark-dedup/Cargo.toml` to `=1.0.0-beta.7`. No behaviour change.
- **Tests**:
  - `rust/bismark-extractor/tests/pe_phase_c.rs` — new unit tests (overlap fixture, PE pairing, per-mate ignore, multi-record PE loop).
  - `rust/bismark-extractor/tests/pe_phase_c_smoke.rs` — end-to-end smoke (synthetic PE BAM via `bismark-io::BamWriter`, run binary, assert 12-file emission + splitting-report shape).
  - `rust/bismark-io/src/read.rs` — extend existing test module with the promoted `detect_paired_from_header` tests (cherry-picked from dedup's test suite).

### 3.3 Crate versions

- `bismark-extractor`: `1.0.0-alpha.2` → `1.0.0-alpha.3` (additive within the alpha line).
- `bismark-io`: `1.0.0-beta.6` → `1.0.0-beta.7` (additive — new public function, no breaking changes).
- `bismark-dedup`: no version bump; dep update only.

### 3.4 Binary behaviour

After Phase B (PR #849): SE Default mode works; PE rejected per-record with `PhaseNotYetImplemented`.

After Phase C:
- `--paired-end` → `extract_pe` (real PE extraction).
- No mode flag → auto-detect via `detect_paired_from_header` at reader-open.
- `--single-end` → `extract_se` (unchanged from Phase B).
- Phase-gate rejections still fire for: non-default output modes, `--gzip`, `--parallel > 1`, `--bedGraph` / `--cytosine_report`, multiple input files. Unchanged.

## 4. Behaviour specification

### 4.1 PE main loop

```rust
fn extract_pe(input: &Path, config: &ResolvedConfig) -> Result<(), BismarkExtractorError> {
    run_extraction(input, config, |reader, chr_table, state| {
        let mut iter = reader.records();
        loop {
            let r1 = match iter.next() {
                Some(Ok(r)) => r,
                Some(Err(e)) => return Err(e.into()),
                None => break Ok(()),                  // clean end of BAM
            };
            let r2 = match iter.next() {
                Some(Ok(r)) => r,
                Some(Err(e)) => return Err(e.into()),
                None => return Err(BismarkExtractorError::UnpairedFinalRecord {
                    qname: render_qname_opt(&r1),
                }),
            };
            let pair = BismarkPair::from_mates(r1, r2)?;   // qname-eq + R1/R2 ids
            handle_one_pair(&pair, state, chr_table, config)?;
            // Rev 1 fix (Reviewer A §1.5 / Reviewer B L1): Perl line 2451 increments
            // by 2 per pair (one per BAM line). Splitting report says "Processed N lines",
            // not pairs. Two singleton +=1 calls is cleaner than +=2 and matches the
            // structure of Phase B's SE loop (one +=1 per record).
            state.report.records_processed = state.report.records_processed.saturating_add(2);
        }
    })
}

fn handle_one_pair(
    pair: &BismarkPair,
    state: &mut ExtractState,
    chr_table: &[String],
    config: &ResolvedConfig,
) -> Result<(), BismarkExtractorError> {
    // Rev 1 (Reviewer B L3): cache chr once. Cross-chr defensive check first;
    // post-check r1 and r2 share a chr by construction.
    let r1_refid = pair.r1().inner().reference_sequence_id()
        .ok_or_else(|| BismarkExtractorError::InternalError {
            message: "PE R1 missing reference_sequence_id".to_string(),
        })?;
    let r2_refid = pair.r2().inner().reference_sequence_id()
        .ok_or_else(|| BismarkExtractorError::InternalError {
            message: "PE R2 missing reference_sequence_id".to_string(),
        })?;
    if r1_refid != r2_refid {
        return Err(BismarkExtractorError::MateChromosomeMismatch {
            qname: render_qname_str(pair.r1()),
            r1_refid,
            r2_refid,
        });
    }
    let chr = resolve_chr_by_refid(r1_refid, chr_table)?;   // shared by both mates

    let pair_strand = pair.pair_strand();
    let r1_calls = extract_calls(pair.r1(), config.ignore_5p_r1, config.ignore_3p_r1)?;
    let r2_calls_raw = extract_calls(pair.r2(), config.ignore_5p_r2, config.ignore_3p_r2)?;

    let r2_calls = if config.no_overlap {
        drop_overlap(r2_calls_raw, pair)?     // rev 1 (Reviewer B L6): pair_strand dropped from sig
    } else {
        r2_calls_raw
    };

    for call in r1_calls {
        route_call(state, pair.r1(), chr, pair_strand, call, ReadIdentity::R1)?;
    }
    for call in r2_calls {
        route_call(state, pair.r2(), chr, pair_strand, call, ReadIdentity::R2)?;
    }
    Ok(())
}
```

### 4.2 Overlap detection (`drop_overlap`)

```rust
/// SPEC §7.4: drop R2 calls overlapping R1's reference span when --no_overlap.
/// Polarity locked in SPEC rev 2: Perl skips inclusively (>= for forward,
/// <= for reverse against R1's ref_end / ref_start); the keep predicate is
/// the strict inverse (< / >).
///
/// Rev 1 (Reviewer B L6): `pair_strand` is recoverable from `pair.pair_strand()`;
/// dropped from the signature to remove caller-side boilerplate.
///
/// Rev 1 (Reviewer B F1): use `Vec::retain` instead of `into_iter().filter().collect()`
/// to avoid reallocating a new Vec on the common case (most R2 calls kept).
pub fn drop_overlap(
    mut r2_calls: Vec<MethCall>,
    pair: &BismarkPair,
) -> Result<Vec<MethCall>, BismarkExtractorError> {
    let r1_start = pair.r1().alignment_start().ok_or_else(|| {
        BismarkExtractorError::InternalError {
            message: "R1 of PE pair missing alignment_start".to_string(),
        }
    })? as usize;
    if is_forward_pair_strand(pair.pair_strand()) {
        // OT/CTOB pair: R1 upstream, R2 downstream. Skip R2 calls AT OR AFTER r1_ref_end.
        let r1_ref_end = pair.r1().cigar().reference_end(r1_start) as u32;
        r2_calls.retain(|c| c.ref_pos < r1_ref_end);
    } else {
        // OB/CTOT pair: R2 upstream, R1 downstream. Skip R2 calls AT OR BEFORE r1_ref_start.
        let r1_ref_start = r1_start as u32;
        r2_calls.retain(|c| c.ref_pos > r1_ref_start);
    }
    Ok(r2_calls)
}

/// Forward-class pair strands: R1's mapped position is the upstream end of the
/// insert. OT and CTOB are forward; OB and CTOT are reverse.
///
/// Cites Perl `bismark_methylation_extractor:2400` (forward branch entry) +
/// line 2415 (reverse branch entry) where the per-pair direction selection lives.
fn is_forward_pair_strand(strand: BismarkStrand) -> bool {
    matches!(strand, BismarkStrand::OT | BismarkStrand::CTOB)
}
```

### 4.3 SE-vs-PE auto-detect (header-level)

`detect_paired_from_header` lives in `bismark-io` after promotion. Returns `Option<bool>` per the existing dedup signature:

- `Some(true)` — PE Bismark @PG line found (has both `-1`/`--1` and `-2`/`--2`).
- `Some(false)` — SE Bismark @PG line found.
- `None` — no Bismark @PG line; main dispatch errors with a clear message asking for explicit `--single-end` / `--paired-end`.

In `main.rs::run`:

```rust
match config.paired_mode {
    PairedMode::SingleEnd => extract_se(input, &config),
    PairedMode::PairedEnd => extract_pe(input, &config),
    PairedMode::AutoDetect => {
        // Open reader once for header inspection. The reader is then either
        // (a) re-opened in extract_se/extract_pe (cheap; BAM index is cached
        // by the OS) OR (b) threaded into the dispatch as a peeked reader.
        // For Phase C: re-open is simpler and matches dedup's pattern.
        let header_probe_reader = open_reader(input, /*cram_ref=*/ None)?;
        let is_paired = detect_paired_from_header(header_probe_reader.header())
            .ok_or_else(|| BismarkExtractorError::AutoDetectFailed {
                message: "no Bismark @PG line in header; pass --single-end or --paired-end explicitly".to_string(),
            })?;
        drop(header_probe_reader);
        if is_paired { extract_pe(input, &config) } else { extract_se(input, &config) }
    }
}
```

Adds one new error variant `AutoDetectFailed { message }` for the `None` case.

### 4.4 Per-mate ignore-region trims

Phase B's kernel `extract_calls(record, ignore_5p, ignore_3p)` already accepts the trims as plain `u32` parameters. Phase C wires:

- R1 mate: `extract_calls(pair.r1(), config.ignore_5p_r1, config.ignore_3p_r1)`.
- R2 mate: `extract_calls(pair.r2(), config.ignore_5p_r2, config.ignore_3p_r2)`.

The kernel applies the trims in 5'-oriented read coordinates (Phase B's invariant). For R2, the 5' end is the sequencing-cycle start of the R2 read — `iter_aligned`'s orientation correction handles this transparently (the `-`-strand R2 case is what `iter_aligned` was designed for).

No kernel changes. Phase A's `ResolvedConfig` already carries `ignore_5p_r2` and `ignore_3p_r2`.

### 4.5 Cross-chr pair handling

`BismarkPair::from_mates` validates qname-equality + R1/R2 identity but does NOT check that R1 and R2 share a reference. Bismark never emits cross-chr PE alignments (paired Bowtie2 wouldn't produce them), but defensive handling is per SPEC §8.4 edge case row.

**Decision (rev 1):** add a check in `handle_one_pair` — if `r1.reference_sequence_id() != r2.reference_sequence_id()`, return `BismarkExtractorError::MateChromosomeMismatch { qname, r1_refid, r2_refid }` (renamed from rev 0's `CrossChromosomePair` per Reviewer A §1.7 for naming consistency with `MateMismatch` / `ReadIdentityMismatch`). Don't add to `bismark-io` for Phase C (extractor-specific concern); file a follow-up issue for `bismark-io v1.0.0-beta.8` to promote it to `BismarkPair::from_mates` for symmetry. Rev 1 also caches `chr` once after the defensive check (Reviewer B L3) — no double lookup.

### 4.6 Edge cases

| Case | Handling |
|------|----------|
| Empty input BAM | `extract_pe` exits the loop cleanly; finalize writes the 12 header-only files + splitting report (same as Phase B SE empty). |
| Odd number of records | `extract_pe` returns `UnpairedFinalRecord { qname }` after cleanup. |
| Records out of order (e.g. coordinate-sorted) | `bismark-io::open_reader` rejects upstream with `UnsortedInput`; extractor propagates. |
| Pair with mismatched qnames | `BismarkPair::from_mates` returns `MateMismatch`; extractor propagates as `BismarkIo`. |
| R1 in second-position / R2 in first | `BismarkPair::from_mates` returns `ReadIdentityMismatch`. Propagated. |
| Cross-chromosome pair (R1 chr1, R2 chr2) | Defensive `CrossChromosomePair { qname }` in `handle_one_pair`. |
| `--include_overlap` set | `config.no_overlap == false` → `drop_overlap` is skipped → all R2 calls kept. |
| R2 partially overlapping R1 | `drop_overlap` drops R2 calls in overlap region; non-overlapping R2 calls kept. |
| R1 + R2 fully overlapping (mate-pair span < read length, "innie" with insert smaller than read) | `drop_overlap` drops every R2 call. Output is R1-only for that pair. |
| R1 + R2 disjoint (mate-pair span > read length) | `drop_overlap` is a no-op; all R2 calls trivially pass the strict comparison. |
| Auto-detect: no `@PG ID:Bismark` line | `AutoDetectFailed` error with actionable message. |
| Auto-detect: bismark2-aligned input | `detect_paired_from_header` returns the right answer (looks for `-1`/`--1` AND `-2`/`--2` in the bismark `@PG` CL field). |

## 5. Signatures (proposed)

### 5.1 `overlap.rs`

```rust
use bismark_io::{BismarkPair, BismarkStrand};
use crate::call::MethCall;
use crate::error::BismarkExtractorError;

/// Drop R2 calls overlapping R1's reference span. Per SPEC §7.4 + Perl
/// 2891-2906 (forward) + 2976-2990 (reverse).
///
/// Pair-strand is read from `pair.pair_strand()` internally (rev 1 simplification
/// per Reviewer B L6).
///
/// # Errors
///
/// `InternalError` if R1 lacks an alignment_start (filtered upstream by
/// `bismark-io`, so this should not fire in practice).
pub fn drop_overlap(
    r2_calls: Vec<MethCall>,
    pair: &BismarkPair,
) -> Result<Vec<MethCall>, BismarkExtractorError>;

/// Cites Perl `bismark_methylation_extractor:2400` (forward branch entry) and
/// line 2415 (reverse branch entry) where R1's strand-tag selects the
/// pair-direction in the per-pair Perl loop.
pub(crate) fn is_forward_pair_strand(strand: BismarkStrand) -> bool;
```

### 5.2 `pipeline.rs` additions / refactors

```rust
use std::fs::File;
use std::io::BufReader;
use bismark_io::AnyReader;

/// Phase C: shared scaffolding for SE and PE extraction loops.
///
/// Opens the reader, builds the chr-name table, derives the basename,
/// constructs `ExtractState` (which eager-opens 12 split files), runs
/// the caller's `body` closure (which drives record-by-record or
/// pair-by-pair logic), then calls `state.finalize()`. On any error
/// from `body`, runs `state.cleanup_partial_outputs()` before propagating.
///
/// Rev 1 (Reviewer A §1.3 / Reviewer B L5): `AnyReader` is generic over
/// `<R: BufRead, RC: Read + Seek>`. The concrete instantiation returned by
/// `open_reader` is `AnyReader<BufReader<File>, File>`; the closure signature
/// pins that instantiation rather than introducing generic propagation
/// through `run_extraction`.
fn run_extraction<F>(
    input: &Path,
    config: &ResolvedConfig,
    body: F,
) -> Result<(), BismarkExtractorError>
where
    F: FnOnce(
        &mut AnyReader<BufReader<File>, File>,
        &[String],
        &mut ExtractState,
    ) -> Result<(), BismarkExtractorError>;

/// Helper: resolve `record.inner().reference_sequence_id()` against the
/// per-file chr-name table. Rev 1 (Reviewer B L2): extracted from
/// Phase B's inline `extract_se` code so both SE and PE bodies share it.
fn resolve_chr(record: &BismarkRecord, chr_table: &[String])
    -> Result<&str, BismarkExtractorError>;

/// Variant of `resolve_chr` that takes a pre-resolved refid (avoids a second
/// `reference_sequence_id()` call when the caller has already extracted it,
/// as `handle_one_pair` does for the cross-chr defensive check).
fn resolve_chr_by_refid(refid: usize, chr_table: &[String])
    -> Result<&str, BismarkExtractorError>;

/// Phase C: PE main loop.
pub fn extract_pe(input: &Path, config: &ResolvedConfig) -> Result<(), BismarkExtractorError>;

/// Phase B: SE main loop. Refactored in Phase C to delegate to `run_extraction`.
pub fn extract_se(input: &Path, config: &ResolvedConfig) -> Result<(), BismarkExtractorError>;
```

Both `extract_se` and `extract_pe` become thin wrappers around `run_extraction` that differ only in the closure body (single-record loop vs paired-record loop). Phase B's defensive per-record PAIRED-flag check stays inside `extract_se`'s body (defense-in-depth against `--single-end` against a PE BAM).

### 5.3 New error variants

```rust
/// PE input had an odd number of records — the final R1 has no R2 mate.
#[error("unpaired final record in PE BAM: qname={qname:?}; PE input must contain pairs of adjacent R1/R2 records")]
UnpairedFinalRecord {
    /// QNAME of the orphan R1 (or `None` if the record had no name).
    qname: Option<String>,
},

/// R1 and R2 of a PE pair aligned to different chromosomes. Bismark
/// never produces this; defensive guard against tooling corruption.
///
/// Rev 1 (Reviewer A §1.7): renamed from `CrossChromosomePair` for
/// naming consistency with `MateMismatch` / `ReadIdentityMismatch`.
#[error("PE pair {qname} has R1 and R2 on different chromosomes (R1=refid {r1_refid}, R2=refid {r2_refid}); Bismark does not emit cross-chr pairs")]
MateChromosomeMismatch {
    /// Pair QNAME (shared by R1 and R2 per `BismarkPair`'s qname-eq guarantee).
    qname: String,
    /// R1 reference ID.
    r1_refid: usize,
    /// R2 reference ID.
    r2_refid: usize,
},

/// `--paired-end` / `--single-end` auto-detect failed because the input's
/// SAM header does not contain a recognised Bismark `@PG` line.
#[error("library-mode auto-detection failed: {message}")]
AutoDetectFailed {
    /// Diagnostic message naming the next user step.
    message: String,
},
```

### 5.4 `bismark-io::detect_paired_from_header` (promoted)

```rust
// In rust/bismark-io/src/read.rs:
/// Auto-detect single-end vs paired-end from a Bismark BAM header.
///
/// Walks the `@PG` lines and looks for one whose `ID` starts with
/// `Bismark`. If found, inspects the command line in its `CL` field for
/// `-1`/`--1` AND `-2`/`--2` arguments — present in PE mode, absent in
/// SE mode.
///
/// Returns:
/// - `Some(true)` — PE.
/// - `Some(false)` — SE.
/// - `None` — no Bismark `@PG` line; caller should error.
///
/// Promoted from `bismark-dedup/src/pipeline.rs:137` (was hardcoded there).
/// Mirrors Perl `deduplicate_bismark` lines 90-116.
pub fn detect_paired_from_header(header: &noodles_sam::Header) -> Option<bool>;
```

## 6. Implementation outline (ordered, rev 1)

1. **bismark-io v1.0.0-beta.7 promotion** (do first; bismark-extractor + bismark-dedup depend on it):
   - Move `detect_paired_from_header` body from `bismark-dedup/src/pipeline.rs:137` to `bismark-io/src/read.rs`. Re-export via `bismark-io/src/lib.rs`.
   - **Rev 1 (Reviewer B X2): also promote the private `arg_present` helper at `bismark-dedup/src/pipeline.rs:175`** — used only by `detect_paired_from_header`. Move it alongside.
   - Move the existing dedup tests for the function to `bismark-io/src/read.rs`'s test module.
   - Bump `bismark-io/Cargo.toml` to `1.0.0-beta.7`. Verify `noodles-sam` already pulls the `io` feature (the function uses `noodles_sam::io::Writer` to serialize the header for substring search) — `cargo check` after the move catches any feature-gap.
   - Update `bismark-dedup/Cargo.toml` bismark-io dep to `=1.0.0-beta.7`; update `bismark-dedup/src/pipeline.rs` to `use bismark_io::detect_paired_from_header` and drop the local definition. **Verify** `cargo test -p bismark-dedup` still green (no behaviour change).
2. **bismark-extractor: bump dep + version**:
   - `Cargo.toml` bismark-io `=1.0.0-beta.6` → `=1.0.0-beta.7`.
   - `Cargo.toml` version `1.0.0-alpha.2` → `1.0.0-alpha.3`.
3. **Phase A bug-fix (rev 1 Reviewer A §1.1 Critical)** in `src/cli.rs::validate`: change `no_overlap` resolution from `paired_mode == PairedMode::PairedEnd` to `paired_mode != PairedMode::SingleEnd`. Add a regression test `validate_auto_detect_keeps_no_overlap_default`.
4. **Add error variants** in `src/error.rs`: `UnpairedFinalRecord`, `MateChromosomeMismatch` (rev 1 renamed from `CrossChromosomePair`), `AutoDetectFailed`.
5. **Create `src/overlap.rs`** with `drop_overlap` + `is_forward_pair_strand`. Pair-strand recovered internally via `pair.pair_strand()` (rev 1 Reviewer B L6). Use `Vec::retain` (rev 1 Reviewer B F1) not `into_iter().filter().collect()`. Use `pair.r1().cigar().reference_end(start)` from `bismark-io::CigarExt`.
6. **Refactor `src/pipeline.rs`** (rev 1 Reviewer A §1.4 contingency — see below):
   - **If Phase B has merged** before Phase C implementation starts: extract common scaffolding into `run_extraction<F>(input, config, body)`. Pin the closure signature on `AnyReader<BufReader<File>, File>` (rev 1 Reviewer A §1.3 / Reviewer B L5). Rewrite `extract_se` as a `run_extraction` wrapper. Add `resolve_chr` + `resolve_chr_by_refid` helpers (rev 1 Reviewer B L2) used by both SE and PE bodies.
   - **If Phase B is still in review**: duplicate the scaffolding in `extract_pe` (~30 LOC) — don't refactor `extract_se` concurrently with Phase B's review. The `run_extraction` helper extraction lands as a follow-up PR after Phase B merges.
   - **Either way**: Phase B's defensive PAIRED-flag check stays in `extract_se`'s body for defense-in-depth (rev 1 Reviewer B L4 — replaces rev 0's contradictory "move OUT … wait, keep" bullet).
   - Add `extract_pe` with the pair-loop body. Use `BismarkPair::from_mates` for each (R1, R2) tuple; propagate `MateMismatch` / `ReadIdentityMismatch` via `?`.
   - Add `handle_one_pair` helper. Resolve `chr` once after the `MateChromosomeMismatch` defensive check (rev 1 Reviewer B L3).
7. **Update `src/main.rs::run`**:
   - Dispatch on `config.paired_mode`: `SingleEnd → extract_se`, `PairedEnd → extract_pe`, `AutoDetect → header-probe → dispatch`.
   - `AutoDetect` path: open reader once for header inspection, call `detect_paired_from_header`, dispatch.
8. **Update `src/lib.rs`** with `pub mod overlap;` + re-exports.
9. **Write tests** (§7 below).
10. **Run `cargo test -p bismark-extractor && cargo test -p bismark-io && cargo test -p bismark-dedup && cargo clippy && cargo fmt --check`** until all three crates green.

## 7. Tests

### 7.1 Unit tests (in `tests/pe_phase_c.rs`)

| Test | Asserts |
|------|---------|
| `drop_overlap_forward_pair_drops_r2_at_or_after_r1_end` | OT pair (R1 chrX 100-149, 50M), R2 calls at refpos 148, 149, 150 → keeps 148; drops 149 (== r1_ref_end) and 150. **Endpoint verification per SPEC §7.4.** |
| `drop_overlap_reverse_pair_drops_r2_at_or_before_r1_start` | OB pair (R1 chrX 200-249, 50M), R2 calls at refpos 199, 200, 201 → drops 199 and 200 (≤ r1_ref_start); keeps 201. |
| `drop_overlap_disjoint_pair_is_noop` | R1 50M at 100, R2 50M at 300 (mate-pair span 200, read length 50 — no overlap) → no R2 calls dropped. |
| `drop_overlap_fully_overlapping_pair_drops_all_r2_calls` | R1 50M at 100, R2 50M at 100 → all R2 calls in overlap; all dropped. |
| `drop_overlap_with_r1_indel_uses_reference_end` | R1 `50M2D50M` at 100 → `reference_end == 201`. R2 calls at 200, 201, 202 → keeps 200; drops 201 and 202. Closes the InDel-aware ref-position invariant (SPEC §7.4 "decision at reference-position level"). |
| `drop_overlap_with_r1_end_deletion` | **Rev 1 (Reviewer A §2.5):** R1 `49M2D1M` at 100 → `reference_span == 52`, `reference_end == 151`. R2 calls at 150, 151, 152 → keeps 150; drops 151, 152. End-of-read deletion topology. |
| `drop_overlap_with_r1_insertion_shifts_read_pos_only` | **Rev 1 (Reviewer A §2.5):** R1 `50M2I50M` at 100 → insertion consumes read but not reference, so `reference_span == 100`, `reference_end == 199`. R2 calls at 198, 199, 200 → keeps 198; drops 199, 200. Insertion topology. |
| `is_forward_pair_strand_matches_perl_classification` | OT, CTOB → true; OB, CTOT → false. |
| `bismark_pair_from_mates_rejects_mismatched_qnames` | (smoke-level; mostly bismark-io's responsibility) |
| `extract_pe_handles_two_well_formed_pairs` | Two adjacent PE pairs in a synthetic BAM → both routed correctly; `state.report.records_processed == 2`. |
| `extract_pe_rejects_unpaired_final_record` | Synthetic BAM with 3 records (one R1 missing R2) → returns `UnpairedFinalRecord { qname }`; cleanup removes all 12 files. |
| `extract_pe_rejects_mismatched_qnames_pair` | Synthetic BAM with R1 qname `foo` + R2 qname `bar` adjacent → `BismarkPair::from_mates` errors with `MateMismatch`; propagated as `BismarkIo`; cleanup runs. |
| `extract_pe_rejects_cross_chromosome_pair` | Synthetic BAM with R1 chr1 + R2 chr2 → `CrossChromosomePair { qname }`; cleanup runs. |
| `extract_pe_with_include_overlap_keeps_r2_overlap_calls` | **Rev 1 fixture spec (Reviewer B V1):** R1 50M at chrX:100-149, R2 50M at chrX:120-169 (overlap region 120-149 inclusive). R2 has methylation calls at ref_pos 125, 135, 145 (all inside overlap). With `--include_overlap`: assert all three calls present in output. |
| `extract_pe_with_no_overlap_drops_r2_overlap_calls` | Same fixture as above; default `--no_overlap` → assert calls at 125, 135, 145 absent (specific ref positions; not a count). |
| `extract_pe_per_mate_ignore_r2_only_skips_r2_positions` | **Rev 1 fixture spec (Reviewer B V2):** R1 and R2 each have methylation calls at read-positions 0, 1, 2. With `--ignore_r2 3`: assert R1's pos-0/1/2 calls present; R2's pos-0/1/2 calls absent. Distinguishes R2-specific trim from `--ignore` (which would skip R1 too). |
| `extract_pe_per_mate_ignore_3prime_r2_only_skips_r2_3prime` | Mirror for `--ignore_3prime_r2`. |
| **`extract_pe_ignore_r2_skips_read_cycles_not_ref_positions`** | **Rev 1 (Reviewer A §2.4):** OT pair where R2 is reverse-mapped (record_strand == CTOT). Apply `--ignore_r2 3`. Assert the first three **5'-end read cycles** are skipped — not the first three reference positions (which would be the 3' end on a reverse-strand read). Locks the iter_aligned-orientation-correction → ignore-region-trim path for R2. |
| `extract_pe_routes_r2_calls_to_pair_strand_file_not_record_strand_file` | **Fixture comment:** R1's record_strand=OT, R2's record_strand=CTOT (explicit; the specific case that bites the naive port). All R2 calls must land in `*_OT_*.txt`, never `*_CTOT_*.txt`. Closes Alan's split-across-files bug at PE unit level. |
| **`extract_pe_routes_ctot_pair_strand_correctly`** | **Rev 1 (Reviewer A §4.2):** non-directional library — pair where R1's record_strand=CTOT (forward class per `is_forward_pair_strand`). Assert overlap-detection branches via the forward path; R2 calls route to `*_CTOT_*.txt`. |
| **`extract_pe_routes_ctob_pair_strand_correctly`** | **Rev 1 (Reviewer A §4.2):** mirror — pair where R1's record_strand=CTOB. Forward class. R2 calls route to `*_CTOB_*.txt`. |
| `extract_pe_increments_mbias_R2_at_index_1` | R2 calls increment `state.mbias[1]` not `state.mbias[0]`. |
| `extract_pe_empty_bam_writes_only_header_files` | Empty PE BAM → 12 header-only files + splitting report with 0 lines processed. |
| **`pe_splitting_report_counts_lines_not_pairs`** | **Rev 1 (Reviewer A §1.5 / Reviewer B L1):** synthetic 10-pair PE BAM → splitting report contains "Processed 20 lines in total" (2 × 10 pairs). Matches Perl `bismark_methylation_extractor:2451` (`$methylation_call_strings_processed += 2`) + line 2479 report literal. |
| **`run_extraction_runs_cleanup_on_each_error_variant`** | **Rev 1 (Reviewer B E1/V4):** parameterized test that injects each `BismarkExtractorError` variant raise-able from the closure body (`InvalidXmByte`, `MateMismatch`, `UnpairedFinalRecord`, `MateChromosomeMismatch`, `IoWrite`) into `run_extraction`'s closure and asserts zero residual files on disk after each. Locks the refactor-safety invariant. |
| **`extract_se_handles_two_well_formed_records`** | **Rev 1 (Reviewer A §4.1):** SE counterpart to `extract_pe_handles_two_well_formed_pairs` — guards Phase B regression after the `run_extraction` refactor. |
| **`validate_auto_detect_keeps_no_overlap_default`** | **Rev 1 (Reviewer A §1.1 Critical):** `Cli::validate` on flags `[input.bam]` (no `--paired-end`, no `--single-end`, no `--include_overlap`) resolves `no_overlap = true`. Without the rev-1 fix, AutoDetect mode would resolve `no_overlap = false`, silently leaking R2 overlap calls when auto-detect routes to PE. |

### 7.2 SE-vs-PE auto-detect tests (in `tests/pe_phase_c.rs` or a small new file)

| Test | Asserts |
|------|---------|
| `detect_paired_from_header_returns_some_true_for_pe_bismark_pg` | (in `bismark-io/src/read.rs` test module — relocated from dedup) |
| `detect_paired_from_header_returns_some_false_for_se_bismark_pg` | (same; relocated) |
| `detect_paired_from_header_returns_none_for_no_bismark_pg` | (same) |
| `main_auto_detect_routes_pe_bam_to_extract_pe` | Spawn binary on synthetic PE BAM, no `--single`/`--paired` → succeeds; output files match PE expectation. |
| `main_auto_detect_routes_se_bam_to_extract_se` | Spawn binary on synthetic SE BAM → succeeds; SE behaviour. |
| `main_auto_detect_fails_without_bismark_pg` | Spawn on BAM with no `@PG ID:Bismark` line → `AutoDetectFailed`; stderr contains "pass --single-end or --paired-end explicitly". |

### 7.3 End-to-end smoke (`tests/pe_phase_c_smoke.rs`)

Synthetic ~10-pair PE BAM (R1 + R2 alternating). Mix of OT pairs and OB pairs. Assertions mirror Phase B's smoke:

- Exit 0.
- All 12 split files exist with version header.
- At least one call on each of CpG_OT, CHG_OT, CHH_OT (OT pairs routed correctly).
- At least one call on CpG_OB / CHH_OB.
- CTOT/CTOB files header-only (directional library).
- **Rev 1 (Reviewer A §1.5 / Reviewer B L1):** splitting report contains `"Processed 20 lines in total"` (10 pairs × 2 lines per pair). NOT "10 pairs" — matches Perl line 2479 report literal exactly.

### 7.4 Test coverage adjacency

Phase B's 44 SE unit tests stay green. The `extract_se` refactor through `run_extraction` is behaviour-preserving — any regression there shows up in Phase B's existing tests.

## 8. Efficiency

- One additional `Vec::collect()` per pair in `drop_overlap` (filter on R2 calls). At a typical 100-bp PE WGBS read with ~30 calls/read, this is ~30 × 16-byte `MethCall` = ~500 bytes per pair, ~14 GiB total at 27M pairs. Allocator churn comparable to Phase B's per-record `Vec`. **Phase F concern, not Phase C.**
- `BismarkPair::from_mates` allocates 2 `BismarkRecord`s (Phase B already does this per-record; PE doesn't change the per-mate cost).
- Header auto-detect adds **one** extra `open_reader` call per run for the AutoDetect path. ~50 ms overhead — negligible for the 55M-record runtime. Single-end and explicit `--paired-end` paths skip it entirely.

Profile target (informational): PE extract on 10M PE WGBS at parallel=1 reaches ≥ 1.5× Perl. Hard gate: byte-identity (Phase H).

## 9. Assumptions + open questions

### 9.1 Locked assumptions

- **Adjacent-record pairing** (no out-of-order detection). Bismark BAM is QNAME-grouped; Phase B verified records arrive R1 then R2.
- **`config.no_overlap == true` is the default for PE.** Phase A's `Cli::validate` sets this; `--include_overlap` flips it.
- **`pair_strand` from R1.** SPEC §6.1 locked.
- **Endpoint polarity is strict-`<` / strict-`>`.** SPEC §7.4 rev 2.
- **Cross-chr pair is always wrong** — defensive `CrossChromosomePair` error.
- **PE BAM is QNAME-sorted (Bismark's default).** Coordinate-sorted PE input would break adjacent-record pairing; `bismark-io::open_reader` already rejects this.
- **`detect_paired_from_header` in `bismark-io v1.0.0-beta.7`** is a pure additive promotion; no behaviour change vs dedup's local copy.

### 9.2 Open questions (rev 1)

1. **(Open, low-risk)** SPEC §7.4 endpoint-semantics — the strict-`<` polarity assumes `CigarExt::reference_end` returns 1-based **inclusive**. SPEC §7.4 cites the dedup-tested invariant; rev 1 tests `drop_overlap_with_r1_indel_uses_reference_end` + `_end_deletion` + `_insertion_shifts_read_pos_only` cover all three CIGAR-relevant topologies. If Phase H gate uncovers an off-by-one, the suspect is `reference_end` semantics or 1-based-vs-0-based mismatch in `read_pos → ref_pos`.
2. **(Resolved rev 1)** Splitting-report PE counting: **lines (2N for N pairs), not pairs**. Verified directly by both reviewers at Perl `bismark_methylation_extractor:2451` (`$methylation_call_strings_processed += 2`) and line 2479 (`"Processed N lines in total"`).
3. **(Resolved rev 1)** Chr-name resolution per pair: **cache once after the `MateChromosomeMismatch` defensive check**; pass the single `chr` to both R1 and R2 `route_call` invocations. Reviewer B L3.
4. **(Open, deferred)** `AutoDetect`'s open-reader-twice pattern. **Default plan:** open twice. ~50 ms overhead, negligible at 55M-record scale. Phase F may consolidate as a side-effect of the producer/consumer pipeline refactor.
5. **(Resolved)** `ExtractParams` revival deferred to Phase D/E (same as Phase B).
6. **(Resolved rev 1)** `drop_overlap` API: `pair_strand` dropped from signature (Reviewer B L6); pair-strand recovered internally via `pair.pair_strand()`. Locked.
7. **(Resolved rev 1)** `run_extraction<F>` closure signature: pinned to concrete `AnyReader<BufReader<File>, File>` (Reviewer A §1.3 / Reviewer B L5). Locked.

### 9.3 Critical questions

**None.** All design choices have defaults; no item changes the goal/scope/behaviour such that pausing is mandatory.

## 10. Validation

| What to verify | How | Expected |
|----------------|-----|----------|
| Overlap-detection polarity (the load-bearing Phase H byte-identity invariant) | `drop_overlap_forward_pair_drops_r2_at_or_after_r1_end` + `drop_overlap_reverse_pair_drops_r2_at_or_before_r1_start` + `drop_overlap_with_r1_indel_uses_reference_end` | All three edge-cases pass; strict `<` / `>` matches Perl `>= ` / `<=` skip semantics exactly. |
| Pair-strand routing (Alan's split-across-files bug at PE level) | `extract_pe_routes_r2_calls_to_pair_strand_file_not_record_strand_file` | R2 calls route to `*_OT_*.txt`, not `*_CTOT_*.txt`. |
| Per-mate ignore-region trimming | `extract_pe_per_mate_ignore_r2_only_skips_r2_positions` + `_3prime_r2` mirror | R1 trims apply to R1 only; R2 trims apply to R2 only. |
| `BismarkPair::from_mates` propagates qname / identity errors | `extract_pe_rejects_mismatched_qnames_pair` | Returns `BismarkIo(MateMismatch)`; cleanup runs. |
| Unpaired-final-record error | `extract_pe_rejects_unpaired_final_record` | Returns `UnpairedFinalRecord`; cleanup runs. |
| Cross-chr pair defensive guard | `extract_pe_rejects_cross_chromosome_pair` | Returns `MateChromosomeMismatch` (rev 1 rename); cleanup runs. |
| **AutoDetect `no_overlap` regression (rev 1 Critical)** | `validate_auto_detect_keeps_no_overlap_default` | `Cli::validate` resolves `no_overlap = true` for AutoDetect (was `false` in Phase A). |
| **PE splitting-report line-counting (rev 1 Important)** | `pe_splitting_report_counts_lines_not_pairs` | "Processed 20 lines in total" for 10 pairs, matches Perl line 2479. |
| **Refactor safety (rev 1 Important)** | `run_extraction_runs_cleanup_on_each_error_variant` | Each error variant from the closure body leaves zero residual files. |
| **Reverse-strand R2 ignore (rev 1 Important)** | `extract_pe_ignore_r2_skips_read_cycles_not_ref_positions` | First three 5'-end read cycles skipped on a CTOT-strand R2 with `--ignore_r2 3`. |
| **Non-directional library PE (rev 1 Important)** | `extract_pe_routes_{ctot,ctob}_pair_strand_correctly` | CTOT pair routes to `*_CTOT_*.txt`; CTOB pair routes to `*_CTOB_*.txt`. Overlap branches via forward path. |
| **InDel-aware endpoint coverage (rev 1 Important)** | `drop_overlap_with_r1_end_deletion` + `_insertion_shifts_read_pos_only` | Three InDel topologies (mid-del, end-del, insertion) all verified. |
| `--include_overlap` semantic | `extract_pe_with_include_overlap_keeps_r2_overlap_calls` | R2 overlap calls present in output. |
| Auto-detect routes to extract_pe / extract_se | `main_auto_detect_routes_pe_bam_to_extract_pe` + SE mirror | Binary dispatch via `paired_mode == AutoDetect` works. |
| Auto-detect failure mode | `main_auto_detect_fails_without_bismark_pg` | Clear error message, no partial output. |
| M-bias R2 index | `extract_pe_increments_mbias_R2_at_index_1` | Phase B's `[MbiasTable; 2]` index 1 actually populates. |
| Phase B regression | `cargo test -p bismark-extractor` shows all 91 Phase B tests still green | The `run_extraction` refactor is behaviour-preserving. |
| Cross-crate regression | `cargo test -p bismark-io` + `cargo test -p bismark-dedup` green after the v1.0.0-beta.7 promotion | `detect_paired_from_header` moved without behaviour change. |
| End-to-end PE smoke | `tests/pe_phase_c_smoke.rs` | Binary runs end-to-end on synthetic PE BAM; produces 12 files + report; pair count correct. |
| Clippy + fmt | `cargo clippy -p bismark-extractor -p bismark-io -p bismark-dedup --all-targets -- -D warnings && cargo fmt --check` | All clean. |

## 11. Integration with later phases

(Carries forward Phase B's table; deltas only.)

| Phase | What Phase C leaves for it |
|-------|----------------------------|
| **D** (M-bias writer) | `state.mbias[1]` now populated by PE R2 (Phase C). Phase D's writer emits 6 sections (CpG/CHG/CHH × R1/R2) for PE input; 3 sections for SE. |
| **E** (modes + gzip) | PE doesn't change mode dispatch; Phase E widens both `extract_se` and `extract_pe` via `OutputFileMap`'s mode-aware variant. |
| **F** (multicore) | The producer/consumer pipeline at Phase F needs to preserve **pair adjacency**. Pairs are the unit of work, not individual records. The bounded MPMC channel feeds `Vec<BismarkPair>` (or `BismarkRecord` chunks where each chunk is pair-aligned). **Rev 1 (Reviewer A §6):** `run_extraction<F>`'s `FnOnce(&mut reader, &[String], &mut state)` doesn't compose with the producer/consumer split (reader + state can't be `&mut`-shared across threads). Phase F will either rework `run_extraction` into `run_extraction_serial` + `run_extraction_parallel`, or drop the abstraction entirely. Plan accordingly. |
| **G** (bedGraph + cytosine_report) | No PE-specific work; subprocess chain operates on the per-context split files (same as SE). |
| **H** (byte-identity gate) | PE 10M+55M WGBS gates Phase B + C + D + E + F together. Endpoint-semantics verification (SPEC §7.4) gets its real-data validation here. |

## 12. Self-review

**Efficiency.** PE's per-pair cost is ~2× per-record (two `extract_calls` invocations + two `iter_aligned` allocations + `drop_overlap`'s filter+collect). Acceptable at parallel=1; Phase F profiling will quantify under rayon. The header-probe reopen on AutoDetect adds ~50 ms once per run.

**Logic.** PE pair-loop is structurally identical to dedup's PE iteration (which Phase A merged successfully). `BismarkPair::from_mates` does the qname-eq + R1/R2 dance. `drop_overlap` uses InDel-aware `reference_end` from bismark-io's CigarExt.

**Edge cases.** Empty input, odd record count, cross-chr pair, mismatched qnames, fully-overlapping pair, disjoint pair, R1-with-InDel — all covered. Header auto-detect failure mode names the next user step.

**Integration.** bismark-io v1.0.0-beta.7 is a pure additive bump (one new pub function); bismark-dedup gets a small refactor to use the re-export with no behaviour change. bismark-extractor 1.0.0-alpha.3 carries the new pub functions (`extract_pe`, `drop_overlap`) without breaking the alpha-line surface.

**Risks remaining.**

- **R1**: SPEC §7.4 endpoint-semantics verification — the in-test fixture covers it, but the real Phase H gate is the ultimate truth. Documented as Open question #1.
- **R2**: PE pair-counting in splitting report (pairs vs records). Resolve at implementation time by reading Perl.
- **R3**: AutoDetect's open-reader-twice pattern is wasteful in the abstract. Phase F may consolidate (single-pass with header peek), but only if profiling justifies the refactor.

## 13. Revision history

See the consolidated revision-history block at the top of this document (after the status line).

## 14. Sub-issue (already filed)

[#850](https://github.com/FelixKrueger/Bismark/issues/850).

## 15. Branching strategy

- **Branch:** `extractor-phase-c` (created off `extractor-phase-b` while Phase B PR #849 is in review).
- **PR target:** `rust/iron-chancellor`. If Phase B merges first, rebase onto fresh iron-chancellor before opening Phase C's PR. If Phase B is still in review when Phase C is ready, open the Phase C PR with base `extractor-phase-b` (stacked PRs) and rebase when Phase B merges.

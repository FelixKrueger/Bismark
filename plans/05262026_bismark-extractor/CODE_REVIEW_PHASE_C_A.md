# Phase C — Code review (Reviewer A)

**Target:** Phase C of `bismark-extractor` Rust port — PE extraction loop, overlap
detection, per-mate ignore trims, SE-vs-PE auto-detect via promoted
`detect_paired_from_header`, and a Phase A `no_overlap` regression fix.

**Branch:** `extractor-phase-c` (stacked on `extractor-phase-b` / PR #849).

**Plan:** `plans/05262026_bismark-extractor/PHASE_C_PLAN.md` rev 1.

**Files reviewed:**
- `rust/bismark-extractor/src/{overlap,pipeline,main,cli,error,output,lib}.rs`
- `rust/bismark-extractor/tests/{pe_phase_c,pe_phase_c_smoke,se_phase_b,se_phase_b_smoke}.rs`
- `rust/bismark-io/src/{read,lib}.rs`
- `rust/bismark-dedup/src/pipeline.rs`
- `Cargo.toml`s for all three crates.

## Summary

Phase C lands clean. The PE pair loop, overlap predicate, error-cleanup
ordering, chr-caching, and AutoDetect dispatch all match the rev 1 plan
literally. I verified the load-bearing polarity (`drop_overlap`'s strict
`<` / `>` keep predicates) by re-reading Perl
`bismark_methylation_extractor` lines 2400/2415/2451/2479/2905/2987 myself,
and the Rust matches Perl exactly: forward branch keeps R2 calls strictly
below `r1_start + reference_span - 1`; reverse branch keeps R2 calls
strictly above R1's raw `alignment_start`. The Phase A `no_overlap`
regression fix is correctly applied (`paired_mode != SingleEnd`), and the
new regression test exercises it on the AutoDetect path. Cross-crate
promotion of `detect_paired_from_header` + `arg_present` into
`bismark-io v1.0.0-beta.7` is a pure-additive move with the dedup callsite
preserved through a `pub use` re-export, so existing PE dedup compiles
without behavioural change.

Two minor findings, neither blocking. No bugs.

## Findings

### Critical — none.

### High — none.

### Medium

**M1.** `pe_phase_c.rs::drop_overlap_with_r1_insertion_shifts_read_pos_only`
(lines 363–416) is awkward but correct. The first attempt deliberately
constructs an underlength fixture, throws it away with `let _ = pair`, and
rebuilds with a 102-byte XM. The fixture path that's actually exercised
DOES match the claim (R1 `50M 2I 50M` → `reference_span = 100`,
`reference_end = 199`, R2 calls at 198/199/200 → keep 198, drop 199/200),
so the test is trustworthy. Recommendation: collapse the dead-fixture
preamble into a single correctly-sized builder call so the test reads in
linear order and the inline commentary isn't load-bearing. Low priority,
purely cosmetic.

**M2.** Splitting-report literal in `output.rs::write_splitting_report`
uses `"Processed N lines in total"` matching Perl line 2479 — verified
against the actual Perl source on disk
(`bismark_methylation_extractor:2479` reads
`warn "\nProcessed $counting{sequences_count} lines in total\n";` and
`:2482` writes the same to the report). The Phase B SE test fixtures
(`se_phase_b.rs:611`, `se_phase_b_smoke.rs:247`, `:327`) have been
updated to the new literal, which is correct. Note: this is a literal
**change** from Phase B's earlier "reads" wording. If any external
consumer (e.g. nf-core/methylseq report parser) is grepping for
`"Processed N reads"`, the fix breaks them — but it brings byte-identity
closer to Perl, which is the Phase H gate. Worth flagging in the PR
description; no code change needed.

### Low

**L1.** `bismark-dedup/src/pipeline.rs:30` has
`#[allow(unused_imports)] pub use bismark_io::detect_paired_from_header;`.
The `#[allow]` is unnecessary — `pub use` is itself a use, not unused. It
compiles fine without the attribute. Recommend removing the `#[allow]` to
avoid suggesting the import is dead when it's actually a deliberate
public re-export.

**L2.** `drop_overlap_disjoint_pair_drops_all_r2_calls_downstream_of_r1_end`
(lines 261–287) intentionally pins the documented-as-incorrect SPEC §7.4
"disjoint pair → no-op" prose. The test comment says exactly what the
algorithm does ("Perl 2905-2906 uses early-exit") and asserts the
correct behaviour. Phase H byte-identity gate will confirm against real
data. Suggestion: file a follow-up issue to fix SPEC §7.4 prose so future
readers don't get bitten by the same intuition. Tracked in the test
itself, so non-blocking.

**L3.** `pipeline.rs::extract_pe`'s body duplicates `extract_se`'s
scaffolding (open_reader, build_chr_name_table, derive_basename,
ExtractState::new, cleanup-on-error, finalize). The module doc comment
(lines 11–19) calls this out as deliberate per plan §6 step 6 contingency
since Phase B PR #849 is still in review. The duplication is ~30 LOC.
Confirm follow-up PR removes the duplication once Phase B merges.
Documented; non-blocking.

**L4.** The cross-chr error message contains both refids but doesn't
include chr names. Operator debugging would benefit from
`r1_refid (chr1) vs r2_refid (chr2)` rendering. The `chr_table` is in
scope at the raise site but is intentionally not used for naming because
the resolve happens AFTER the cross-chr check. Tiny ergonomic miss; the
qname + refids are sufficient. Non-blocking.

## Detailed audit against scrutiny list

### 1. `drop_overlap` polarity vs Perl

Verified directly. Re-read of
`bismark_methylation_extractor`:

- Line 2400 (forward branch): `$end_read_1 = $start_read_1 + $MDN_count_1 - 1;`
  — R1's reference END (1-based inclusive last).
- Line 2415 (reverse branch): `$end_read_1 = $start_read_1;`
  — R1's reference START (the variable is named `$end_read_1` but in the
  reverse branch it carries R1's start; this is Perl's slot-reuse, not a
  semantic mismatch).
- Line 2905 (forward skip): `if ($start+$index+$pos_offset >= $end_read_1) { return; }`
  → keep predicate strict `<` r1_ref_end. Rust:
  `r2_calls.retain(|c| c.ref_pos < r1_ref_end)` — matches.
- Line 2987 (reverse skip): `if ($start-$index+$pos_offset <= $end_read_1) { return; }`
  → keep predicate strict `>` r1_ref_start. Rust:
  `r2_calls.retain(|c| c.ref_pos > r1_ref_start)` — matches.

The SPEC §7.4 "disjoint pair → no-op" prose was indeed wrong; the Rust
implementation matches Perl, not the SPEC. Plan rev 1 acknowledges this
in the §2 decisions table.

### 2. `extract_pe` error-cleanup ordering

All five pre-finalize error sites correctly invoke
`state.cleanup_partial_outputs()` before propagating:

- `pipeline.rs:202-205` — `records.next() → Some(Err)` (r1 iter error). OK.
- `pipeline.rs:211-215` — `records.next() → Some(Err)` (r2 iter error). OK.
- `pipeline.rs:216-223` — `records.next() → None` (UnpairedFinalRecord). OK.
- `pipeline.rs:229-232` — `BismarkPair::from_mates` error. OK.
- `pipeline.rs:235-238` — `handle_one_pair` error. OK.

The `state.report.records_processed += 2` increment correctly fires
AFTER `handle_one_pair` returns Ok, so failed pairs don't bump the
counter (line 242 only reached on success path).

### 3. `handle_one_pair` chr-name caching

Verified at `pipeline.rs:264-301`. R1 refid + R2 refid resolved once each
(necessary for the cross-chr check). After the check, `chr_table.get(r1_refid)`
runs once, and the resulting `&str` is reused for both R1 and R2
`route_call` invocations at lines 315/318. No redundant chr_table lookup
per R2. Matches Reviewer B L3 rev 1 fix.

### 4. `MateChromosomeMismatch` formatting + rename consistency

- `error.rs:200-217` — definition uses `qname: String, r1_refid: usize, r2_refid: usize`
  and message contains all three. OK.
- `pipeline.rs:285-289` — raise site populates all three. OK.
- `pe_phase_c.rs:770` — test asserts stderr contains `"different chromosomes"`
  (substring match on the message). OK.
- Rev 0's `CrossChromosomePair` name eradicated: `git grep CrossChromosomePair`
  returns no results in the repo. Rename complete.

### 5. `AutoDetectFailed` UX

`main.rs:115-122` builds the message
`"no \`@PG\` line with \`ID:Bismark*\` found in {input}'s header; pass \`--single-end\` or \`--paired-end\` explicitly"`.
Mentions next step. The `probe` reader is `drop()`-ed explicitly on line
124 (cosmetic — would be dropped on scope exit anyway). The probe
reader holds an open `File` only, no temp resources. No leak.

The `?` on `open_reader` (line 114) propagates as `BismarkIo(BismarkIoError)`,
which is fine — a malformed BAM at probe time will surface as the same
error the real reader would emit.

### 6. Counter ordering + "lines in total" literal

- `pipeline.rs:242` — `state.report.records_processed = state.report.records_processed.saturating_add(2);`
  fires AFTER `handle_one_pair?`. OK.
- `output.rs:270` — `writeln!(w, "Processed {} lines in total", report.records_processed)?;`
  matches Perl `:2479` and `:2482` literally (verified against on-disk
  Perl source). OK.
- Phase B SE tests updated: `se_phase_b.rs:611`, `se_phase_b_smoke.rs:247`/`:327`
  all assert `"Processed N lines in total"`. OK.

### 7. Phase A `no_overlap` bug-fix

`cli.rs:452-456`:
```rust
let no_overlap = if paired_mode != PairedMode::SingleEnd {
    !self.include_overlap
} else {
    false
};
```
Matches the plan literally. `paired_mode != SingleEnd` covers both
`PairedEnd` and `AutoDetect`.

Regression test at `pe_phase_c.rs:422-441`
(`validate_auto_detect_keeps_no_overlap_default`) constructs a Cli with
NO `--paired-end` / `--single-end` / `--include_overlap` flags, asserts
`paired_mode == AutoDetect && no_overlap == true`. Exercises the actual
fix path. OK.

The complementary test `validate_paired_end_keeps_no_overlap_default`
(line 444) plus `validate_paired_end_with_include_overlap_disables_no_overlap`
(line 458) plus Phase A's existing `validate_se_no_overlap_is_false`
(`cli.rs:920`) cover all three `paired_mode` × `include_overlap`
permutations.

### 8. `is_forward_pair_strand`

`overlap.rs:72-74`:
```rust
pub fn is_forward_pair_strand(strand: BismarkStrand) -> bool {
    matches!(strand, BismarkStrand::OT | BismarkStrand::CTOB)
}
```
Doc comment at lines 65-71 cites Perl `bismark_methylation_extractor:2400`
(forward) + `:2415` (reverse). Correct: OT and CTOB are forward-class
(R1's mapped position is the upstream end). Test at `pe_phase_c.rs:204-211`
covers all four strands.

### 9. bismark-dedup re-export

`bismark-dedup/src/pipeline.rs:30` —
`#[allow(unused_imports)] pub use bismark_io::detect_paired_from_header;`.
Verified `bismark_dedup::main.rs:364` calls
`pipeline::detect_paired_from_header(reader.header())` which resolves
through the re-export. Validation context confirms `cargo test -p bismark-dedup`
green.

### 10. `arg_present` promotion

`bismark-io/src/read.rs:687-696` — defined as a private fn alongside
`detect_paired_from_header` (line 649). `git diff rust/bismark-dedup/src/pipeline.rs`
confirms the original `arg_present` in dedup is fully removed
(comments on lines 124-126 mark the removal). Not duplicated. OK.

### 11. `Vec::retain` in drop_overlap

`overlap.rs:54` and `:60`:
```rust
r2_calls.retain(|c| c.ref_pos < r1_ref_end);
// ...
r2_calls.retain(|c| c.ref_pos > r1_ref_start);
```
In-place retain on the owned-moved `r2_calls: Vec<MethCall>`. No
`into_iter().filter().collect()`. Matches rev 1 Reviewer B F1.

### 12. Phase B sibling preservation

Verified by diffing `extract_se` in `pipeline.rs:66-159` against the
description in Phase B's PR #849. The body is unchanged except for the
defensive PAIRED-flag check which Phase B already had (lines 88-96).
Crucially, no `run_extraction<F>` helper was extracted — the duplication
between `extract_se` and `extract_pe` is acknowledged in the module
doc-comment (lines 11-19) and tracked as a Phase B-merge follow-up.
Plan §6 step 6 contingency honored.

### 13. Test trustworthiness — InDel endpoint fixtures

- `drop_overlap_with_r1_indel_uses_reference_end` (lines 316-336):
  R1 `50M 2D 50M` at start=100. `reference_span = 50 + 2 + 50 = 102`
  (deletion consumes ref). `reference_end = 100 + 102 - 1 = 201`. R2
  calls at 200, 201, 202: keep 200 (`< 201`); drop 201, 202 (`>= 201`).
  Asserted: `kept.len() == 1 && kept[0].ref_pos == 200`. Correct.

- `drop_overlap_with_r1_end_deletion` (lines 339-360):
  R1 `49M 2D 1M` at start=100. `reference_span = 49 + 2 + 1 = 52`.
  `reference_end = 100 + 52 - 1 = 151`. R2 calls at 150, 151, 152:
  keep 150 (`< 151`); drop 151, 152. Asserted: `kept.len() == 1 && kept[0].ref_pos == 150`.
  Correct.

- `drop_overlap_with_r1_insertion_shifts_read_pos_only` (lines 363-416):
  R1 `50M 2I 50M` at start=100. `reference_span = 50 + 0 + 50 = 100`
  (insertion does NOT consume ref). `reference_end = 100 + 100 - 1 = 199`.
  R2 calls at 198, 199, 200: keep 198 (`< 199`); drop 199, 200.
  Asserted: `kept.len() == 1 && kept[0].ref_pos == 198`. Correct.

All three CIGAR-relevant InDel topologies (mid-read deletion, end deletion,
insertion) covered. Fixtures correctly construct CIGAR + alignment_start
to produce the claimed `reference_end` values.

## Cross-cutting observations

**Smoke tests:** Both `pe_phase_c.rs::pe_e2e::*` and
`pe_phase_c_smoke.rs::*` build BAMs via `BamWriter::from_path` and run
the binary via `assert_cmd`. `smoke_pe_auto_detect_produces_all_12_files_and_report`
in particular exercises the load-bearing AutoDetect → PE → 12-file
emission path end-to-end on a 10-pair synthetic BAM.

**Test coverage of cleanup:** `extract_pe_rejects_cross_chromosome_pair`
(line 720) asserts `fs::read_dir(&outdir).count() == 0` after the binary
failure — pins the cleanup invariant. Other error sites
(`UnpairedFinalRecord`, `MateMismatch` from `from_mates`) only check
stderr substring + exit failure but not residual-file count; non-critical
because cleanup is identical across error paths.

**Doc accuracy:** `overlap.rs`'s doc-comment header is excellent —
both the polarity inversion (skip `>=` vs keep `<`) and the rev-1
simplification (dropping `pair_strand` from the signature) are explained
with Perl line citations. `pipeline.rs::extract_pe`'s doc explains the
splitting-report counter rationale. Future maintainers won't be guessing.

## Recommendations (prioritized)

| Priority | Recommendation |
|----------|----------------|
| Low | Drop `#[allow(unused_imports)]` from the dedup re-export (L1). |
| Low | Collapse the dead-fixture preamble in `drop_overlap_with_r1_insertion_shifts_read_pos_only` so the test reads top-to-bottom (M1). |
| Low | Flag the "reads" → "lines" splitting-report literal change in the PR description so downstream parser owners (nf-core/methylseq) know to update (M2). |
| Low | File a follow-up to fix SPEC §7.4's "disjoint pair → no-op" prose (L2). |
| Low | Track the Phase B → C scaffolding refactor (`run_extraction<F>`) as a follow-up issue once #849 merges (L3 — already documented in module doc). |

No Critical or High items.

## Verdict

**APPROVE-WITH-NITS.** The Phase C implementation correctly executes the
rev 1 plan, the polarity-critical `drop_overlap` predicate matches Perl
verbatim (verified line-by-line), and the Phase A regression fix is
exercised by a real AutoDetect test. The findings above are cosmetic
polish — none should block PR merge.

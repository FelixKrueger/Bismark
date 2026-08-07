# Plan — close the two worthwhile #1095 validation gaps (rev 0)

**Date:** 2026-08-07 · **Branch:** `1095-validation-gates` (off `dev` @ `26eb28a`) · **Parent feature:** `plans/08062026_five-base-bisulfite-bam/` (#1095, shipped on `dev`)

## 1. Goal

Codify two properties of `--five_base_bisulfite_bam` that four reviews verified but no committed
test asserts (handoff §5 G9; parent `PLAN.md` §9.8 + §10b; `CODE_REVIEW_A` §"Recommendation";
`CODE_REVIEW_B` H-suggestion + L10):

1. **Gate 1 — §9.8 `XG` ⟺ FLAG (R8).** Assert `XG == "CT" ⟺ FLAG ∉ {16, 83, 163}` over
   `tests/data/dedup/nondir_pe_1030.bam` — the only committed fixture with all four `XG`/FLAG
   combinations. The equivalence is **load-bearing and invisible to the idempotence gate**
   (property G1(2) is *emergent*: `methylation_call` branches on `XR`, not `XG`).
2. **Gate 2 — PE / four-strand idempotence.** Parameterise the round-trip gate over the two
   committed PE fixtures: `dedup/nondir_pe_1030.bam` (20 records, all four strand indices) and
   `dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam` (12,974 records, 42 with `I`/`D`).
   #1095 is a PE-only feature whose committed gates are currently SE-only, covering 2 of 4
   strand indices.

Both codify already-verified results — Reviewer A ran the parameterised round-trip ("zero risk —
I have run it"); Reviewer B measured byte-identity on both PE fixtures (12974/12974 re-encoded,
flip rate `0.000000`, `masked 0`) — so they should pass first time. Test-only change: **no source
edits**, no fixture edits, no behaviour change.

## 2. Context

- **File:** `rust/bismark/tests/aligner_five_base_bisulfite.rs` — the existing 7 integration
  gates. New tests go here (they are #1095 driver gates; same helpers, same conventions).
- **Helpers to reuse:** `bismark_bin()`, `data_dir()`, `samtools_available()` (panics under
  `$CI` — gates must not skip silently, G8), `sam_body()`, `run_converter()`.
- **Fixtures (committed, untouched):**
  - `rust/bismark/tests/data/dedup/nondir_pe_1030.bam` — 20 records; FLAGs {99, 147, 83, 163};
    both `XG` values; the #1030 CTOT/CTOB FLAG-swap pairs.
  - `rust/bismark/tests/data/dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam` — 12,974
    records incl. indels and `CB:Z:`/barcode aux tags.
  - `rust/bismark/tests/data/five_base_bisulfite/softclip_indel_se.bam` — the existing SE
    fixture; its dedicated test remains as-is.
- **CI:** `rust_ci.yml` `test` job installs samtools (added by #1095), so the new gates run
  there for real; `RUSTFLAGS: -D warnings` at workflow scope (G26).

## 3. Behavior

### Gate 1 — `xg_ct_iff_forward_flags_over_all_four_strand_indices`

1. Skip (locally) / panic (CI) via `samtools_available()`.
2. `sam_body(dedup/nondir_pe_1030.bam)` → iterate lines; extract FLAG (field 2) and the
   `XG:Z:` tag.
3. Every record must have an `XG` tag, and its value must be `CT` or `GA` — anything else fails.
4. Assert the biconditional per record: `XG == "CT"` ⇔ `FLAG & 0x10 == 0` for this fixture's
   FLAG set — concretely `FLAG ∈ {99, 147}` for `CT` and `FLAG ∈ {83, 163}` for `GA`. Assert
   via the §9.8 formulation (`FLAG ∉ {16, 83, 163}`) so the test text matches the spec.
5. **Non-vacuity census:** assert exactly 20 records and that **all four** (XG, FLAG) pairs
   (CT,99), (CT,147), (GA,83), (GA,163) occur — a truncated or regenerated fixture must fail
   loudly rather than weaken the gate to a subset of the table.

### Gate 2 — PE round-trip idempotence (two tests)

1. Extract the body of `bisulfite_input_round_trips_to_identical_sam_text` into a helper
   `assert_bisulfite_round_trip(fixture: &Path, expected_records: usize)`:
   - skip/panic via `samtools_available()`;
   - `run_converter(fixture, tmp)`; assert success;
   - output path = `<stem>.bisulfite.bam` (Open-2 naming), assert it exists;
   - compare `sam_body(fixture) == sam_body(produced)` (decompressed text — BGZF blocks are
     writer-dependent and `NM` is re-encoded as i32);
   - **non-vacuity:** assert the input body has `expected_records` lines, so two empty streams
     can never satisfy the gate (G19).
2. Keep the existing SE test as a thin call: `assert_bisulfite_round_trip(fixture(), 8)`.
3. Add `nondir_pe_all_four_strand_indices_round_trip_identically` →
   `assert_bisulfite_round_trip(dedup/nondir_pe_1030.bam, 20)`.
4. Add `real_pe_with_mate_fields_and_aux_tags_round_trips_identically` →
   `assert_bisulfite_round_trip(dedup/synth_barcode_10k_..._pe.bam, 12974)`.

Edge cases: none new — inputs are committed static fixtures; failure modes are the assertions
themselves. The helper must not change the SE test's assertions (pure extraction).

## 4. Signatures

```rust
fn xg_flag(line: &str) -> (u16, String)          // parse FLAG + XG:Z: from one SAM line (Gate 1, local)
fn assert_bisulfite_round_trip(fixture: &Path, expected_records: usize)
```

## 5. Implementation outline

1. `rust/bismark/tests/aligner_five_base_bisulfite.rs`:
   a. Add `fn dedup_fixture(name: &str) -> PathBuf { data_dir().join("dedup").join(name) }`.
   b. Refactor: move the body of `bisulfite_input_round_trips_to_identical_sam_text` into
      `assert_bisulfite_round_trip(&Path, usize)`; the three `#[test]`s call it (SE keeps its
      current name so history/reports line up; the two PE tests get the names in §3).
   c. Add the Gate 1 test with the per-record biconditional + the four-pair census + `== 20`.
   d. Update the module doc comment: the idempotence gate now runs over three fixtures; delete
      the "committed tests are SE-only" implication if present.
2. No changes outside this one file.
3. Run: `cargo test -p bismark --test aligner_five_base_bisulfite` (all gates, expect 10 tests
   green first time), then `cargo fmt -p bismark -- --check` (G27) and
   `RUSTFLAGS="-D warnings" cargo clippy -p bismark --all-targets -- -D warnings` plus the
   `--features rammap-inprocess` variant (G26).

## 6. Efficiency

12,974 records through a debug-build converter plus two `samtools view` invocations — seconds.
No fixture is added; repository size unchanged.

## 7. Integration

- Test-only; no source, fixture, CLI or docs changes. The dedup fixtures are read-only inputs —
  already shared with `dedup_rs` tests; nothing writes next to them (`run_converter` writes to a
  `tempfile::tempdir()`).
- CI: the gates join the existing #1095 gate set in the `test` job; samtools present; the CI
  panic guard keeps them non-skippable there.
- Downstream: closes two of the five G9 items. The remaining three (unmapped pass-through
  fixture, lower-case `MD`, fixture letter census) stay documented-not-closed — see §10.

## 8. Assumptions

1. `nondir_pe_1030.bam` has exactly 20 records, FLAGs ⊆ {99,147,83,163}, every record tagged
   `XG:Z:CT` or `XG:Z:GA`, all four pairs present. **Verified empirically 2026-08-07** (G20 —
   constants read off the fixture, not review prose): (CT,99)×4, (CT,147)×4, (GA,83)×6,
   (GA,163)×6; the biconditional holds on all 20.
2. `synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam` has exactly 12,974 records (**verified
   empirically 2026-08-07**) and round-trips byte-identically (CODE_REVIEW_B rows; byte-identity
   itself is re-proven by the gate, not assumed).
3. The converter's output name for `X.bam` is `X.bisulfite.bam` (Open-2; existing SE test).
4. `--illumina_5base --five_base_bisulfite_bam` accepts PE Bismark BAMs with no further flags
   (Reviewer A/B both ran exactly this).
5. Fixed rule, not configurable: the §9.8 equivalence is a property of Bismark-produced BAMs;
   the test pins it on this fixture only.

## 9. Validation

| What | How | Expected |
|---|---|---|
| Gates pass first time | `cargo test -p bismark --test aligner_five_base_bisulfite` | 10/10 green (7 existing + 3 new) |
| Gate 1 is falsifiable | temporarily invert the biconditional (`==` → `!=`) locally, run, revert | test FAILS — never-observed-failing is not a check (G19) |
| Gate 2 is falsifiable | temporarily point one PE test at a wrong `expected_records`, run, revert | test FAILS on the census assertion |
| SE refactor is pure | existing SE test still green, same assertions | green; `git diff` shows extraction only |
| CI-skip removal intact | helper still panics when samtools absent + `$CI` set | code path unchanged (inherited from helper reuse) |
| Hygiene | `cargo fmt -p bismark -- --check`; clippy both feature sets (G26/G27) | clean |

## 10. Questions or ambiguities

**No critical questions.** Non-critical, with assumptions taken:

| # | Priority | Question | Assumption taken |
|---|---|---|---|
| Open-1 | Open | samtools-text or noodles for the new gates? Reviewer B suggested noodles `RecordBuf` comparison (no samtools, no skip) | **samtools text**, matching every other gate in the file; the CI panic guard already prevents silent skips. Noodles is a coherent future refactor of the whole file, not something to mix in piecemeal |
| Open-2 | Open | Also assert report contents (flip rate `0.000000`) for the PE fixtures? | **No** — the SE report test already pins the report format; the PE gates' job is byte-identity + strand coverage. Adding report assertions would duplicate coverage and couple two more tests to report wording |
| Open-3 | Open | Close the other three G9 gaps here too? | **Out of scope.** Unmapped pass-through needs a fixture change that invalidates existing counts; lower-case `MD` is unreachable via Bismark-produced BAMs; the letter census is unit-covered. This plan is the two gaps the handoff called "worth closing" |

## 11. Self-Review

- **Logic:** Gate 1's biconditional is asserted per record *and* backed by a completeness census —
  without the census, a fixture regenerated with only OT pairs would render the gate vacuous
  (the G19 failure mode). Gate 2's `expected_records` serves the same role for the round trip.
- **Edge cases:** static fixtures, so the interesting "edges" are fixture drift — both gates fail
  loudly on record-count or combination-set change. Empty-stream comparison is excluded by the
  census. No new error paths.
- **Efficiency:** trivial; largest fixture is 12,974 records.
- **Integration:** pure extraction refactor of the SE test is the only touch to existing code;
  validation row 4 checks it. Names chosen to read as properties, matching the file's style.
- **Adjusted during self-review:** added assumption 1's "verify empirically before writing
  assertions" (constants must be read off the fixture, not off review prose — G20: do not take
  agent reviewers at face value); added the falsifiability rows in §9 (observe each new gate
  failing once before trusting it — G19).
- **Remaining risk:** if Reviewer B's byte-identity result does not reproduce (e.g. environment
  drift since 2026-08-07), Gate 2 fails honestly — that would be a finding, not a test bug.

# Plan — close the two worthwhile #1095 validation gaps (rev 1)

**Date:** 2026-08-07 · **Branch:** `1095-validation-gates` (off `dev` @ `26eb28a`) · **Parent feature:** `plans/08062026_five-base-bisulfite-bam/` (#1095, shipped on `dev`)

## Revision history

- **rev 1 (2026-08-07)** — folds in the dual plan review (`PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md`;
  every contested claim re-verified at source before adoption):
  - **CI fix now in scope** (both Criticals-1): `dev`'s Rust CI is red at `7be9dde` — the
    `rammap-inprocess` and `binseq-input` jobs run the #1095 gates but install only minimap2, so
    the samtools `$CI` panic guard fails 3 tests per job (verified: run 31204223600 + yaml). Two
    line-edits to `rust_ci.yml` mirror samtools into those jobs; this also repairs `dev`.
  - **Gate 1 bit-gloss deleted** (both Criticals-2): `FLAG & 0x10 == 0` is false on 10/20 records
    (147 is reverse *and* `CT`). Only the §9.8 **set form** `FLAG ∉ {16,83,163}` remains.
  - **Uniqueness claim corrected + pair-structure census added** (A-3/B): `synth_barcode…pe.bam`
    also has all four record-level (XG, FLAG) pairs; what is unique to `nondir_pe_1030.bam` is
    the pair-level #1030 FLAG swap — now asserted directly.
  - **Gate 1 parameterised over the SE fixture** (B): witnesses FLAG `16`, otherwise unwitnessed
    on a PE-only fixture. SE census verified: (CT,0)×4, (GA,16)×4.
  - **`CB:Z:` claim removed, test renamed** (A-4): the synth fixture has zero `CB:Z:` tags
    (verified) — the barcode lives in the QNAME.
  - **"Pure extraction" reworded** (A-5/B): the helper adds one non-vacuity census to the SE test.
  - **Honesty note** (B): Gate 1 runs no converter code; it pins the data contract the converter
    relies on, on committed fixtures.
  - Open-3 rationale softened.
- **rev 0 (2026-08-07)** — initial plan.

## 1. Goal

Codify two properties of `--five_base_bisulfite_bam` that four reviews verified but no committed
test asserts (handoff §5 G9; parent `PLAN.md` §9.8 + §10b; `CODE_REVIEW_A` §"Recommendation";
`CODE_REVIEW_B` H-suggestion + L10), and make the gates actually runnable in CI:

1. **Gate 1 — §9.8 `XG` ⟺ FLAG (R8).** Assert `XG == "CT" ⟺ FLAG ∉ {16, 83, 163}` per record
   over `dedup/nondir_pe_1030.bam` **and** `five_base_bisulfite/softclip_indel_se.bam`. The
   equivalence is **load-bearing and invisible to the idempotence gate** (property G1(2) is
   *emergent*: `methylation_call` branches on `XR`, not `XG`). Gate 1 executes no converter code:
   it pins the Bismark-BAM data contract the converter's encoding table relies on, so fixture
   drift or a future flag-table change breaks a test rather than the science.
2. **Gate 2 — PE / four-strand idempotence.** Parameterise the round-trip gate over the two
   committed PE fixtures: `dedup/nondir_pe_1030.bam` (20 records, the four strand indices via
   #1030-swapped pairs) and `dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam` (12,974
   records, 42 with `I`/`D`). #1095 is a PE-only feature whose committed gates are currently
   SE-only.
3. **CI repair (prerequisite for 1–2, fixes `dev`'s red CI).** Mirror the samtools install into
   the `rammap-inprocess` and `binseq-input` jobs of `.github/workflows/rust_ci.yml`, which run
   the #1095 gates and currently panic on the samtools `$CI` guard (3 tests each). Without this,
   the branch cannot go green and the new gates raise the panic count to 6 per job.

Both gates codify already-verified results — Reviewer A ran the parameterised round-trip ("zero
risk — I have run it"); Reviewer B re-measured byte-identity on both PE fixtures (20/20 and
12974/12974 re-encoded, flip rate `0.000000`, `masked 0`) — so they should pass first time.
No source edits, no fixture edits, no behaviour change; the only non-test change is the CI
install step.

## 2. Context

- **Files touched (2):**
  - `rust/bismark/tests/aligner_five_base_bisulfite.rs` — the existing 7 integration gates.
    New tests go here (same helpers, same conventions).
  - `.github/workflows/rust_ci.yml` — the two feature jobs' install steps (currently minimap2
    only, lines ~96–103 and ~134–141; the main `test` job's step at ~35–45 is the model).
- **Helpers to reuse:** `bismark_bin()`, `data_dir()`, `samtools_available()` (panics under
  `$CI` — gates must not skip silently, G8), `sam_body()`, `run_converter()`.
- **Fixtures (committed, untouched; all censuses verified empirically 2026-08-07):**
  - `rust/bismark/tests/data/dedup/nondir_pe_1030.bam` — 20 records; record census
    (CT,99)×4, (CT,147)×4, (GA,83)×6, (GA,163)×6; **pair census (file order, same-qname):
    (147,99)×4, (163,83)×6** — every pair #1030-swapped, which is what makes this fixture the
    non-directional guard (`synth_barcode` also has all four record-level pairs; a directional
    regeneration of *this* fixture must fail the pair census).
  - `rust/bismark/tests/data/dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam` — 12,974
    records incl. 42 with indels; barcode is a **QNAME suffix** (zero `CB:Z:` tags).
  - `rust/bismark/tests/data/five_base_bisulfite/softclip_indel_se.bam` — 8 records;
    (CT,0)×4, (GA,16)×4; witnesses FLAG `16` for Gate 1; its round-trip test remains as-is.
- **CI:** `rust_ci.yml` sets `RUSTFLAGS: -D warnings` at workflow scope (G26). The main `test`
  job installs minimap2 + samtools; the two feature jobs install only minimap2 and are **red on
  `dev` today** (run 31204223600 @ `7be9dde`) — the dual-driver trap: the minimap2 guard was
  mirrored when added, the samtools one was not.

## 3. Behavior

### Gate 1 — `XG` ⟺ FLAG, two tests over one helper

Helper `assert_xg_iff_flag(bam: &Path) -> Vec<(u16, String)>`: skip (locally) / panic (CI) via
`samtools_available()`; `sam_body(bam)`; per line extract FLAG (field 2) and the `XG:Z:` tag;
assert every record has `XG ∈ {CT, GA}`; assert the **set-form biconditional** per record:

```
XG == "CT"  ⟺  FLAG ∉ {16, 83, 163}
```

(no bit-form: `FLAG & 0x10` does **not** track `XG` on PE — FLAG 147 is reverse *and* CT).
Return the (FLAG, XG) list for the caller's census.

1. `xg_ct_iff_forward_flags_over_all_four_strand_indices` — over `nondir_pe_1030.bam`:
   - census: exactly 20 records; record pairs (CT,99)×4, (CT,147)×4, (GA,83)×6, (GA,163)×6;
   - **pair-structure census:** consecutive file-order pairs share a QNAME and their FLAGs are
     exactly (147,99)×4 then/and (163,83)×6 — pins the #1030 swap, so a regenerated directional
     fixture fails loudly instead of silently weakening the gate.
2. `xg_iff_flag_holds_on_the_se_fixture` — over `softclip_indel_se.bam`:
   - census: exactly 8 records; (CT,0)×4, (GA,16)×4 — witnesses the `16` element of the set.

### Gate 2 — PE round-trip idempotence (two new tests)

1. Extract the body of `bisulfite_input_round_trips_to_identical_sam_text` into
   `assert_bisulfite_round_trip(fixture: &Path, expected_records: usize)`:
   - skip/panic via `samtools_available()`;
   - `run_converter(fixture, tmp)`; assert success;
   - output path = `<stem>.bisulfite.bam` (Open-2 naming), assert it exists;
   - compare `sam_body(fixture) == sam_body(produced)` (decompressed text — BGZF blocks are
     writer-dependent and `NM` is re-encoded as i32);
   - **non-vacuity:** assert the input body has `expected_records` lines, so two empty streams
     can never satisfy the gate (G19). This is an **extraction plus one added census** — the SE
     test's existing assertions are otherwise unchanged.
2. SE test becomes a thin call: `assert_bisulfite_round_trip(fixture(), 8)`.
3. `nondir_pe_all_four_strand_indices_round_trip_identically` →
   `assert_bisulfite_round_trip(dedup_fixture("nondir_pe_1030.bam"), 20)`.
4. `real_pe_with_mate_fields_and_indels_round_trips_identically` →
   `assert_bisulfite_round_trip(dedup_fixture("synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam"), 12974)`.

### CI repair

In `.github/workflows/rust_ci.yml`, change both feature jobs' install step to
`sudo apt-get install -y --no-install-recommends minimap2 samtools` (+ the `samtools --version`
echo, mirroring the main `test` job's step and comment). No other workflow change.

Edge cases: none new — inputs are committed static fixtures; failure modes are the assertions
themselves.

## 4. Signatures

```rust
fn dedup_fixture(name: &str) -> PathBuf                 // data_dir()/dedup/<name>
fn assert_xg_iff_flag(bam: &Path) -> Vec<(u16, String)> // per-record biconditional; returns census input
fn assert_bisulfite_round_trip(fixture: &Path, expected_records: usize)
```

## 5. Implementation outline

1. `.github/workflows/rust_ci.yml`: add ` samtools` to the two feature jobs' `apt-get install`
   lines; extend each step name/comment to mention the #1095 samtools guard; add
   `samtools --version | head -1` after the existing `minimap2 --version`.
2. `rust/bismark/tests/aligner_five_base_bisulfite.rs`:
   a. Add `dedup_fixture()`.
   b. Refactor: move the body of `bisulfite_input_round_trips_to_identical_sam_text` into
      `assert_bisulfite_round_trip(&Path, usize)`; the SE test keeps its name and calls it with
      `(fixture(), 8)`; add the two PE tests named as in §3.
   c. Add `assert_xg_iff_flag()` and the two Gate 1 tests (censuses as in §3, constants from §2).
   d. Update the module doc comment: idempotence now runs over three fixtures; Gate 1 pins the
      `XG`⟺FLAG data contract (runs no converter code); drop any SE-only implication.
3. Nothing else changes.
4. Run: `cargo test -p bismark --test aligner_five_base_bisulfite` (expect 11 green: 7 existing
   + 2 Gate 1 + 2 Gate 2), then `cargo fmt -p bismark -- --check` (G27) and
   `RUSTFLAGS="-D warnings" cargo clippy -p bismark --all-targets -- -D warnings` plus the
   `--features rammap-inprocess` variant (G26).

## 6. Efficiency

12,974 records through a debug-build converter plus a handful of `samtools view` invocations —
seconds. No fixture is added; repository size unchanged. CI: one extra apt package in two jobs.

## 7. Integration

- Test file + CI yaml only; no source, fixture, CLI or docs changes. The dedup fixtures are
  read-only inputs shared with `dedup_rs` tests; `run_converter` writes to `tempfile::tempdir()`.
- **The CI step also repairs `dev`'s currently-red pipeline** once merged — worth stating in the
  PR body. Until merged, `dev` stays red for the pre-existing reason.
- Downstream: closes two of the five G9 items. The remaining three (unmapped pass-through,
  lower-case `MD`, fixture letter census) stay documented-not-closed — see §10.

## 8. Assumptions

1. `nondir_pe_1030.bam`: exactly 20 records; record census (CT,99)×4, (CT,147)×4, (GA,83)×6,
   (GA,163)×6; file-order same-qname pairs (147,99)×4, (163,83)×6. **Verified empirically
   2026-08-07** (G20 — constants read off the fixture, not review prose).
2. `synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam`: exactly 12,974 records, zero `CB:Z:` tags
   (**verified empirically 2026-08-07**); round-trips byte-identically (re-proven by the gate,
   not assumed — and independently re-measured by both plan reviewers).
3. `softclip_indel_se.bam`: 8 records, (CT,0)×4, (GA,16)×4 (**verified empirically 2026-08-07**).
4. The converter's output name for `X.bam` is `X.bisulfite.bam` (Open-2; existing SE test).
5. `--illumina_5base --five_base_bisulfite_bam` accepts PE Bismark BAMs with no further flags
   (Reviewers A and B both ran exactly this).
6. Fixed rule, not configurable: the §9.8 equivalence is a property of Bismark-produced BAMs;
   the tests pin it on these committed fixtures only.
7. The two feature jobs' only missing dependency is samtools (their minimap2 guard already
   passes; the `test` job with both tools installed is green on `7be9dde`).

## 9. Validation

| What | How | Expected |
|---|---|---|
| Gates pass first time | `cargo test -p bismark --test aligner_five_base_bisulfite` | 11/11 green (7 existing + 4 new) |
| Gate 1 is falsifiable | temporarily invert the biconditional (`==` → `!=`) locally, run, revert | both Gate 1 tests FAIL — never-observed-failing is not a check (G19) |
| Pair census is falsifiable | temporarily assert (99,147) order, run, revert | nondir Gate 1 test FAILS |
| Gate 2 is falsifiable | temporarily point one PE test at a wrong `expected_records`, run, revert | test FAILS on the census assertion |
| SE refactor preserves assertions | existing SE test still green; `git diff` shows extraction + the census arg only | green |
| CI-skip removal intact | helpers still panic when samtools absent + `$CI` set | code path unchanged (inherited) |
| Hygiene | `cargo fmt -p bismark -- --check`; clippy both feature sets (G26/G27) | clean |
| **All six CI jobs green** | push branch, open PR into `dev`, watch the run | test, clippy, fmt, perl-oracle, **rammap-inprocess, binseq-input** all green — the two feature jobs are red on `dev` today, so green here is a real signal, not a rubber stamp |

## 10. Questions or ambiguities

**No critical questions.** Non-critical, with assumptions taken:

| # | Priority | Question | Assumption taken |
|---|---|---|---|
| Open-1 | Open | samtools-text or noodles for the new gates? Reviewer B (and the parent reviews) note noodles would drop the samtools dependency entirely | **samtools text**, matching every other gate in the file; the CI panic guard prevents silent skips and the CI repair (§3) makes the dependency real in all jobs. A noodles port of the *whole file* is a coherent future refactor, not something to mix in piecemeal |
| Open-2 | Open | Also assert report contents (flip rate `0.000000`) for the PE fixtures? | **No** — byte-identity implies zero flips; the SE report test already pins report wording. Both reviewers concur |
| Open-3 | Open | Close the other three G9 gaps here too? | **Out of scope.** Unmapped pass-through would need either a fixture change or (B) a separate tiny committed fixture — feasible later, deliberately not here; lower-case `MD` is unreachable via Bismark-produced BAMs; the letter census is unit-covered. This plan is the two gaps the handoff called "worth closing" |
| Open-4 | Open | Land the CI repair separately on `dev` first? | **No — carried in this branch** (Felix accepted this recommendation): it is two line-edits, belongs with the #1095 follow-up, and the PR's green feature jobs then demonstrate both the repair and the new gates at once |

## 11. Self-Review

- **Logic:** Gate 1's set-form biconditional is asserted per record and backed by three censuses
  (record pairs, pair structure, record counts); the bit-form footgun is called out inline so it
  cannot resurface at implementation. Gate 2's `expected_records` excludes empty-stream passes.
- **Edge cases:** static fixtures — the "edges" are fixture drift, and every census fails loudly
  on count, combination-set, or pair-order change. No new error paths.
- **Efficiency:** trivial; largest fixture is 12,974 records.
- **Integration:** the CI change is copy-of-existing-step, lowest-risk; the SE test refactor is
  extraction plus one census; validation row 5 checks it. Names read as properties, matching the
  file's style.
- **Adjusted in rev 1:** everything under "Revision history" above; both Critical findings were
  verified against the live CI run and the fixtures before adoption, per G20.
- **Remaining risks:** (1) if byte-identity fails to reproduce in CI's environment, Gate 2 fails
  honestly — a finding, not a test bug; (2) the feature jobs may hide a *second* missing
  dependency behind the samtools panic — assumption 7 addresses this (their minimap2-dependent
  gates already pass), and the PR run will settle it.

## 12. Implementation notes (2026-08-07)

Implemented exactly per §5 — **no deviations**. Single pass, no failed iterations.

- The two feature-job install steps were textually identical, so one `replace_all` edit
  mirrored samtools into both (`rust_ci.yml`).
- `assert_xg_iff_flag` returns `(QNAME, FLAG, XG)` triples (QNAME needed by the pair census).
- **11/11 gates green first time**, as both plan reviewers' pre-runs predicted.
- **Falsifiability observed (G19), then reverted:** inverted biconditional → both Gate 1 tests
  FAIL; pair order flipped to `(99,147)` → nondir test fails on the pair census with the
  intended message; `expected_records` off by one → `real_pe` test fails on "fixture record
  count drifted". `grep -c 'TEMP falsifiability'` = 0 after reverts, full suite re-run green.
- Hygiene: `cargo fmt -p bismark -- --check` clean; `clippy --all-targets -- -D warnings` clean
  on default **and** `rammap-inprocess` (G26/G27).
- §9's "all six CI jobs green" row pends the PR run (the only validation not executable locally).

**Post-review fixes (2026-08-07)** — dual code review verdicts: **A APPROVE, B APPROVE** (no
Critical/High/Medium); coverage audit **COMPLETE** (21 DONE, 1 documented DEVIATED, 1 PENDING =
the PR run). The three Low findings both reviewers agreed on were applied: record-count census
moved *before* the converter run (cleaner drift attribution); duplicated `count` closure hoisted
to `count_of()`; `XG` scan tag-scoped with `.skip(9)`. Re-verified: 11/11 green, fmt clean,
clippy `-D warnings` clean on default + `rammap-inprocess`. Not adopted (recommendations, out of
plan scope): Gate 1 over the synth fixture (B verified 0 violations across its 12,974 records —
free coverage if ever wanted); PE header/`@PG` preservation gate (A; body-only comparison is the
§9.1 contract, header pinned on SE). Notable extra evidence from review: A ran the suite with
`CI=1` and samtools off PATH — all five samtools-gated tests fail loudly, proving the guard
live; B pulled dev run 31204223600's logs — both feature jobs fail with exactly 3 guard panics
and nothing else, verifying assumption 7.

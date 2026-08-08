# Code Review A — #1095 validation gates (commit `010e46f`)

**Reviewer:** A (independent, fresh context) · **Date:** 2026-08-07
**Scope:** commit `010e46f` on branch `1095-validation-gates` — `rust/bismark/tests/aligner_five_base_bisulfite.rs` (4 new tests + helper refactor), `.github/workflows/rust_ci.yml` (samtools mirrored into the two feature jobs). Plan: `plans/08072026_1095-validation-gates/PLAN.md` rev 1.

## Summary

**Verdict: APPROVE.** The implementation matches the plan exactly (the one signature deviation — `assert_xg_iff_flag` returning `(QNAME, FLAG, XG)` triples instead of the plan's `(FLAG, XG)` pairs — is documented in PLAN.md §12 and is required by the pair census). Every empirical claim in the code and commit message that I could reach was re-verified independently and held. No Critical or High findings; four Low observations, all cosmetic or scoped-out coverage notes.

### What I verified empirically (not trusted from comments)

| Claim | Method | Result |
|---|---|---|
| `nondir_pe_1030.bam`: 20 records; (99,CT)×4, (147,CT)×4, (83,GA)×6, (163,GA)×6 | `samtools view` + awk census | **Matches exactly** |
| Pair structure: file-order adjacent same-QNAME pairs, FLAGs (147,99)×4 and (163,83)×6 | `awk | paste - -` | **Matches** — all 10 pairs adjacent, same QNAME, higher-flag record first |
| All 20 records carry `XG:Z:` | awk tag count | 20/20 |
| `softclip_indel_se.bam`: 8 records, (0,CT)×4, (16,GA)×4 | census | **Matches** |
| `synth_barcode_10k…pe.bam`: 12,974 records, 0 `CB:Z:` tags, 42 indel records | `samtools view -c`, grep, CIGAR awk | **Matches** (12974 / 0 / 42) |
| §9.8 biconditional holds on both Gate-1 fixtures | census cross-check: {0,99,147}→CT, {16,83,163}→GA | Holds on every record |
| 11/11 gates green | `cargo test -p bismark --test aligner_five_base_bisulfite` | 11 passed, 1.91 s |
| fmt / clippy | `cargo fmt -p bismark -- --check`; `clippy --all-targets -- -D warnings` on default **and** `rammap-inprocess` | All clean |
| $CI panic guard fires for the new tests | ran the test binary with `CI=1 PATH=/usr/bin:/bin` (samtools absent) | Both Gate-1 tests **and** all 3 round-trip tests FAIL loudly (0 passed); without `CI` they skip green |
| No leftover falsifiability instrumentation | grep for `TEMP falsifiability` in `rust/` | none |
| No other workflow runs `cargo test` | grep `.github/workflows/` | only `rust_ci.yml`; the `perl-oracle` job already installs samtools |

### Vacuous-pass analysis (per assignment)

- **Gate 1 (both tests):** cannot pass vacuously. `assert_xg_iff_flag` over an empty body would return an empty census, but each caller pins `census.len()` (20 / 8) and the four/two `(FLAG, XG)` counts sum to the pinned total, so the census is fully determined — any drift in count, combination, or pair order fails. `sam_body` asserts `samtools view` exit status, so a broken read cannot masquerade as an empty-but-passing census.
- **Gate 2 (all three round trips):** `expected_records` is asserted on the *input* body before the equality, so two empty streams cannot satisfy the comparison (plan G19). Converter exit status and output-file existence are asserted first.
- **Pair census:** asserts each adjacent pair is `(147,99) | (163,83)` with shared QNAME; the per-shape counts (4 and 6) are not asserted directly but are forced by the record census — no gap (see Low-4).
- **CI-red repair claim:** I could not reach the GitHub API from this environment (TLS interception, known off-infra limitation), so run `31204223600` stands on the plan reviews' verification. The fix is nonetheless correct by construction: both feature jobs run `cargo test -p bismark`, which includes this test file, whose guard I demonstrated panics under `$CI` without samtools; the mirrored install step is textually the main `test` job's (verified against lines 35–45 of the yaml).

## Issues by area

### Logic
None. The set-form biconditional is the correct one (147 is reverse-strand *and* CT — the bit-form footgun is called out in the doc comment and correctly absent from the code). The `chunks(2)` pair walk is sound given the empirically-verified adjacency, and the `else panic!("odd record count")` arm is defensively correct even though `len()==20` makes it unreachable.

### Efficiency
Trivial — 12,974 records through a debug converter in under 2 s total suite time. No concerns.

### Errors
None found. All failure paths are loud: missing `XG` panics with the offending line, unexpected `XG` values fail the domain assert, samtools absence fails hard under `$CI` and skips visibly otherwise.

### Structure
Two cosmetic points (Low-1, Low-2 below). Naming follows the file's property-style convention; doc comments explain *why* each gate exists (emergence of the XG⟺FLAG agreement, the #1030 pair structure as the non-directional guard) at appropriate length.

## Fixes applied

None — per the review instructions, this shared worktree must not be edited concurrently; everything below is a recommendation for the caller.

## Recommendations

| # | Priority | Recommendation |
|---|---|---|
| 1 | Low | **Deduplicate the `count` closure** shared by the two Gate-1 tests (`aligner_five_base_bisulfite.rs` lines 192–197 and 224–229) into a small helper (e.g. `fn count(census: &[(String, u16, String)], flag: u16, xg: &str) -> usize`). Pure cosmetics; fine to leave. |
| 2 | Low | **Move the input census before the converter run** in `assert_bisulfite_round_trip` (lines 102–119): compute `before` and assert `expected_records` *before* `run_converter`, so a drifted fixture fails on the drift message without first executing (and potentially mis-attributing a failure to) the converter. Behaviourally identical today. |
| 3 | Low | **Tag scan breadth (informational):** `fields.find_map(|f| f.strip_prefix("XG:Z:"))` scans mandatory fields 3–11 as well as the tag region; a pathological RNAME beginning `XG:Z:` would be misread. Zero risk on these fixtures and consistent with the file's existing `xms` closure — note only, no change needed. |
| 4 | Low | **Coverage note, scoped out by the plan:** the PE round trips compare `samtools view` *body* only; header preservation (@SQ/@PG) on PE inputs is unasserted — the duplicate-@PG-ID test runs only over the SE fixture. If a PE header regression ever matters, extend `appends_a_pg_with_a_distinct_id` over `nondir_pe_1030.bam`; not required now. Relatedly, the pair census's per-shape counts are forced by the record census — if anyone ever deletes the record-count asserts, the pair census alone would accept e.g. 10×(163,83). |

## Plan conformance

- §3 Gate 1: implemented as specified (set form only, both fixtures, record + pair + count censuses). ✓
- §3 Gate 2: SE test extracted to `assert_bisulfite_round_trip` with the non-vacuity census; two PE tests with exactly the planned names and constants. ✓
- §3 CI repair: both feature jobs' install steps now mirror the main `test` job (packages, version echoes, comment updated to name both guards). ✓
- §5.d module doc: updated — three fixtures named, Gate-1 contract explained, SE-only implication dropped. ✓
- §12 implementation notes are accurate against the diff (single `replace_all` plausible — the two replaced steps were byte-identical pre-change; triple-return deviation documented).
- Only remaining unvalidated row from §9 is "all six CI jobs green", which requires the PR run — correctly flagged as pending in §12.

**Report:** `/Users/fkrueger/Github/Bismark/plans/08072026_1095-validation-gates/CODE_REVIEW_A.md`

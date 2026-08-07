# Code Review B — #1095 validation gates (commit `010e46f`)

**Reviewer:** B (independent, fresh context) · **Date:** 2026-08-07
**Scope:** `rust/bismark/tests/aligner_five_base_bisulfite.rs` (4 new tests + round-trip helper refactor), `.github/workflows/rust_ci.yml` (samtools mirrored into `rammap-inprocess` + `binseq-input`), against `plans/08072026_1095-validation-gates/PLAN.md` rev 1.
**Note:** per instruction, no fixes were applied — everything is a recommendation; the worktree was left untouched except this report.

## Verdict

**APPROVE.** No Critical, High, or Medium findings. The implementation matches the plan exactly (one documented, correct deviation), every census constant reproduces independently from the fixtures, no assertion can pass vacuously, and the CI repair is confirmed to be complete and sufficient against the actual failed run on `dev`.

## What I verified empirically (not taken from comments or the plan)

| Claim | Method | Result |
|---|---|---|
| 11/11 gates green | `cargo test -p bismark --test aligner_five_base_bisulfite` | 11 passed, 0 failed (2.11s) |
| nondir_pe_1030 census | `samtools view` + awk, independent of the test code | 20 records; (99,CT)×4, (147,CT)×4, (83,GA)×6, (163,GA)×6 — matches lines 198–201 |
| nondir pair structure | qname/flag `paste - -` pairing | (147,99)×4, (163,83)×6, zero QNAME mismatches — the #1030 swap census (lines 202–211) is correct and every pair is adjacent in file order, so `chunks(2)` is sound |
| SE fixture census | same | 8 records; (0,CT)×4, (16,GA)×4 — matches lines 230–231 |
| synth_barcode fixture | same | 12,974 records; 42 CIGARs with `I`/`D`; 0 `CB:Z:` tags; barcode is a QNAME suffix (`…:1000:TTAGTTGT`); mate fields (`RNEXT`=`=`, PNEXT, TLEN ±215) present — every doc-comment claim on `real_pe_…` (lines 143–144) is true |
| SE fixture is the only committed FLAG-16 witness | flags in synth are 99/147/83/163 only | confirmed — doc claim on line 214–215 holds |
| XG⟺FLAG contract | independent awk re-implementation of the biconditional | 0 violations on nondir, SE, **and** synth (12,974 records) |
| dev's red CI is exactly the samtools guard | `gh run view 31204223600 --log-failed` | both feature jobs: 3 failures each, all panics at `aligner_five_base_bisulfite.rs:52` ("samtools not found but $CI is set"); the minimap2 gates and all 1475 unit tests pass — **plan assumption 7 verified: samtools is the only missing dependency, so the two-line mirror fully repairs `dev`** |
| Hygiene | `cargo fmt -p bismark -- --check`; `cargo clippy -p bismark --all-targets -- -D warnings` | both clean |
| No leftover falsifiability instrumentation | `grep TEMP` on the test file; `git diff 010e46f` | no matches; worktree identical to the commit |

## Vacuous-pass audit (every new assertion)

- **Gate 1 (both tests):** `census.len() == 20/8` blocks the empty-stream pass; the per-combination counts block a degenerate distribution; the pair loop blocks a directional regeneration (pairs (99,147)/(83,163) fail `matches!`). Combined with the record census, the pair counts (147,99)×4 / (163,83)×6 are fully pinned even though the loop itself only checks membership.
- **The biconditional is total, not census-limited:** a hypothetical (147, GA) record fails the `assert_eq!` at line 169, not just the counts — a corrupted fixture cannot slip through on an unobserved FLAG value.
- **Gate 2:** `expected_records` is asserted on the *input* body before the equality, so two empty streams can never satisfy the comparison; a zero-record output fails the equality against a pinned-non-empty `before`.
- **Missing-XG records** panic (line 166) rather than being skipped — a tag-stripped fixture fails loudly.
- **CI skip-removal intact:** all five samtools-gated tests route through `samtools_available()`, whose `$CI` panic is unchanged; Gate 1's two tests guard before calling the helper.

## Plan conformance

- Test names, fixture choices, censuses, module-doc update, CI step name/comment/`samtools --version | head -1` — all exactly per PLAN.md §3/§5.
- **One deviation, documented and correct (§12):** `assert_xg_iff_flag` returns `(QNAME, u16, String)` triples instead of the plan's `Vec<(u16, String)>` (§4). The plan was internally inconsistent — its own §3 pair census needs QNAMEs — so the implementation resolved it the only sensible way.
- SE round-trip refactor is a pure extraction plus the one added census; the original test's three assertions (converter success + stderr, output exists, body equality) are all preserved in the helper.
- Only §9 validation row not executable locally: "all six CI jobs green" pends the PR run. Given the failed-run analysis above, the residual risk is negligible.

## Issues by area

### Logic
None. The set-form biconditional (deliberately not the `0x10` bit form) is correct for PE — independently confirmed: FLAG 147 records carry `XG:Z:CT` in both PE fixtures.

### Errors
None found.

### Efficiency
None. Full suite runs in ~2s; the 12,974-record debug-build conversion is well inside test-time norms. CI adds one apt package to two jobs.

### Structure
Two Low-grade nits, below.

## Recommendations

| # | Priority | Recommendation |
|---|---|---|
| R1 | Low | The 6-line `count` closure is duplicated verbatim between the two Gate 1 tests (lines 192–197 and 224–229). A free function `fn xg_flag_count(census: &[(String, u16, String)], flag: u16, xg: &str) -> usize` next to `assert_xg_iff_flag` would remove it. Borderline — fine to leave. |
| R2 | Low | `assert_xg_iff_flag` scans every field after FLAG for the `XG:Z:` prefix, so RNAME/SEQ/QUAL are searched too. Any accidental shadow fails loudly on the `CT|GA` assert (and the fixtures are static), so this is precision, not correctness: skipping to the tag region (`fields.nth(8)` past RNAME…QUAL before the `find_map`) would make the parse strictly tag-scoped. |
| R3 | Low (optional) | Gate 1 could be parameterised over the synth fixture for one more line — I verified 0 contract violations across its 12,974 records, which carry all four record-level (FLAG, XG) combinations at real scale. The plan deliberately scoped Gate 1 to the two fixtures, so this is an offer, not a gap. |
| R4 | Low | In `assert_bisulfite_round_trip`, the input record-count census runs *after* the converter; moving it before `run_converter` would fail faster on fixture drift and keep drift from being reported mid-round-trip. Cosmetic — the failure message ("fixture record count drifted") is unambiguous either way. |

## Fixes applied

None — deliberately, per the shared-worktree instruction. All four recommendations are safe to defer or drop; none blocks merge.

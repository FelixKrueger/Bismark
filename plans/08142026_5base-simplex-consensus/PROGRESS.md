# Progress: 5-Base simplex consensus output (#1104)

**Type:** standalone plan
**Issue:** [#1104](https://github.com/FelixKrueger/Bismark/issues/1104) (requested via #1095, @Danielsm8)
**Plan:** [PLAN.md](PLAN.md) (rev 1 + notes §11b, §11c)
**Branch:** `1104-simplex-consensus` @ `2564fa9`, pushed (off `dev` @ `53744d5`)

| Step | Status | Notes |
|---|---|---|
| Design decisions | ✅ done | 2026-08-14 with Felix: separate BAM + `mx` tag; enum flag; emit all family sizes; deterministic emission order |
| Plan written | ✅ done | rev 0 → rev 1 |
| Manual review (Felix) | ✅ done | determinism decision taken at rev 0 |
| Agent plan review (dual) | ✅ done | A + B both REQUEST CHANGES; all findings folded into rev 1 |
| Implementation | ✅ done | `50f1eef` step 0 (deterministic order) + `a80aa3a` feature |
| Code review (dual) | ✅ done | **APPROVE ×2** ([A](CODE_REVIEW_A.md), [B](CODE_REVIEW_B.md)). 1 real defect (both found it independently, same patch) + 1 vacuous gate + 2 test holes + 1 dead link → all fixed in `790e0ae` |
| Coverage audit | ✅ done | [COVERAGE.md](COVERAGE.md): 0 MISSING of 63; V5/V7 partial → closed in `2564fa9`. The audit independently ran V1a/V1b by building the dev and step-0 binaries — both pass |
| Scale validation (oxy) | ✅ done | 6.85M-record PE BAM: **+83 MB peak RSS for 3.42M simplex families**; duplex byte-identical modulo `mx`; count identity exact; extractor clean. §11c |
| Biological V9 (real 5-Base) | ⛔ blocked | The Illumina 5-Base demo dataset is not on oxy. Needs re-acquiring; correctness meanwhile rests on the synthetic groundtruth gates + prior DRAGEN concordance |
| PR into `dev` | ⏳ pending | Felix's call |

## Test inventory

| Suite | Count | Covers |
|---|---|---|
| `aligner_five_base_simplex.rs` | 11 | determinism, default-mode invariants, own-strand + exact XM both orientations, MAPQ orphan, single fragment, histogram buckets/clamp, mode matrix, both CLI guards, non-CpG-G no-call |
| `aligner_five_base_groundtruth.rs` | 8 | +1: in-run `both` split + `_pe.5base_simplex.bam` suffix + duplex-TXT `singletons` cross-check |
| `five_base_duplex.rs` units | +9 | `consensus_base` with one strand absent (3), `SimplexLedger` (6, all error directions) |

## Sabotages run (G19 — an unfailed check is not yet a check)

| Sabotage | Outcome |
|---|---|
| Remove the step-0 sort | determinism test FAILED ✓ |
| Emit both flags for simplex families | own-strand test FAILED on record count ✓ |
| In-run simplex suffix (primary pattern) | in-run gate FAILED, printing the wrong filename ✓ |
| In-run simplex suffix (fallback only) | **PASSED** — the fallback is unused, so this proved nothing. Aim a sabotage at the live path |
| `.min(5)` → `.min(4)` (by reviewer A) | histogram test FAILED ✓ |
| Drop duplex families in `both` mode (by reviewer B) | whole suite PASSED before B's fix; FAILS after ✓ |

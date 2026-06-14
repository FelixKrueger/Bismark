# PROGRESS — `06142026_empty-sample-extractor-c2c`

**Feature:** graceful no-alignment sample through extractor + coverage2cytosine (methylseq drop-in robustness, follow-on to the beta.6 dedup fix).
**Crates:** `rust/bismark-extractor` + `rust/bismark-coverage2cytosine`. **Branch:** `rust/extractor-empty-outputs` @ `~/Github/Bismark-dedup` (off iron-chancellor `b97a8e2`).
**Plan:** [`PLAN.md`](./PLAN.md)

## Pipeline status
| Step | Status | Notes |
|---|---|---|
| Diagnose (both walls) | ✅ done | extractor: skips bedGraph/cov + deletes `.txt.gz` → 3/5 methylseq outputs missing. c2c: errors on empty `.cov`. |
| Perl oracle | ✅ done | BOTH walls: Perl also fails (extractor skips/deletes; c2c dies exit 255). → fix = deliberate divergence, not byte-identity. |
| Strategy decision | ✅ done | Felix: **Rust-side emit empty outputs** (2026-06-14). |
| Plan (PLAN.md) | ✅ written (rev 1) | rev 0 → manual-approved → rev 1 folds dual-review findings. |
| Manual review | ✅ done | Felix: "approve" (2026-06-14). |
| Dual plan-review | ✅ done | A APPROVE-WITH-CHANGES (2C/4I), B APPROVE-WITH-CHANGES (0C/4I/5O); no contradictions. All folded → rev 1. `PLAN_REVIEW_{A,B}.md`. |
| Implement | ✅ done | rev 1 implemented (extractor + c2c). All gates green (tests/clippy/fmt both crates). **Full local cascade verified**: dedup→extractor→c2c all exit 0 + all methylseq-required outputs present. 2 sound deviations documented. |
| Verify (dual code-review + plan-manager) | 🔄 in progress | Launching dual /code-reviewer + /plan-manager. |
| V6b scout (report/summary/MultiQC) | ⏳ pending | Static check for a 3rd wall before V-E2E. |
| V-E2E (methylseq end-to-end, HARD gate) | ⏳ blocked | Felix's Seqera env on the beta.7 image; proves no further wall. |
| beta.7 + methylseq pin bump | ⏳ blocked | On explicit go (PLAN §D/10). |

## Key facts
- Two crates: extractor (emit empty bedGraph.gz/cov.gz + retain empty `.txt.gz` on zero calls) + c2c (all-zero report on empty `.cov`).
- Both deliberate divergences from Perl (verified); non-empty paths byte-identical (gated on empty condition).
- Risk: a possible 3rd wall (report/summary/MultiQC) — made a hard end-to-end gate (V-E2E), not assumed.

## Log
- **2026-06-14:** Real Seqera run confirmed wall 1 (`Missing *.bedGraph.gz`). Mapped both walls + Perl oracle on tiny fixtures; Felix chose Rust-side emit-empty; PLAN rev 0 written.

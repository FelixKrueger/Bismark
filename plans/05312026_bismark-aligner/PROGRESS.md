# Progress: Rust port of `bismark` (the aligner wrapper)

**Last updated:** 2026-06-01 (Phase 1 complete)

## Phase Status

| # | Phase | Status | Directory | Notes |
|---|-------|--------|-----------|-------|
| 0 | Determinism spike | ✅ Complete | `phase0-determinism-spike/` | Done 2026-06-01 — premise HOLDS; 2 gate refinements (decompressed-content gate; 2-line @PG). `SPIKE_determinism.md` |
| 1 | CLI + options + discovery | ✅ Complete | `phase1-cli-options-discovery/` | Done 2026-06-01. Coverage COMPLETE; dual code-review findings folded in (case-sensitive FASTA match lockstep w/ genome-prep, pbat/multicore validation, deferred-flag notice, README). 18 unit + 15 integration tests, clippy/fmt clean. Depends on #0 |
| 2 | Read conversion (FastQ SE directional) | 📋 Planned | `phase2-read-conversion/` | Depends on #1 |
| 3 | Single-instance align + SAM parse | 📋 Planned | `phase3-single-instance-align-parse/` | Depends on #1, #2 |
| 4 | N-way merge + scoring + MAPQ | 📋 Planned | `phase4-nway-merge-scoring/` | Depends on #3 |
| 5 | Genomic-seq + XM/XR/XG + SAM/BAM (SE dir) 🎯 | 📋 Planned | `phase5-genomic-seq-xm-sam-output/` | First byte-identity gate; depends on #4 |
| 6 | Reports + ambig/unmapped (SE) 🎯 | 📋 Planned | `phase6-reports-ambig-unmapped/` | Depends on #5 |
| 7 | Paired-end support 🎯 | 📋 Planned | `phase7-paired-end/` | PE byte-identity gate; depends on #5, #6 |
| 8 | Non-directional + pbat 🎯 | 📋 Planned | `phase8-nondirectional-pbat/` | All library types; depends on #7 |
| 9 | FastA + order-preserving threading 🎯 | 📋 Planned | `phase9-fasta-threading/` | Worker-invariance gate; depends on #8 |
| 10 | Real-data gate on oxy 🎯 | 📋 Planned | `phase10-realdata-gate-oxy/` | Full-scale byte-identity; depends on #9 |

## History

- 2026-06-01: Phase 1 → ✅ Complete (coverage COMPLETE; dual code-review fix-pass applied + tested; 33 tests local / 34 Linux CI).
- 2026-06-01: Phase 1 → 🚧 Implementing (crate `bismark-aligner` implemented; 24 tests green, clippy/fmt clean).
- 2026-06-01: Phase 1 dual plan-review complete (A+B); 2 Critical `aligner_options`-order fixes + 5 other corrections folded into PLAN.md rev 1.
- 2026-06-01: Phase 1 → 📝 Planning (PLAN.md written).
- 2026-06-01: Phase 0 → ✅ Complete (determinism spike: premise HOLDS; gate refined to decompressed-SAM content + 2-line @PG policy pending).
- 2026-05-31: Epic created (SPEC rev 1 approved, all forks settled); all phases → 📋 Planned.

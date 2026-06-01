# Progress: Rust port of `bismark` (the aligner wrapper)

**Last updated:** 2026-06-01 (Phase 4 complete)

## Phase Status

| # | Phase | Status | Directory | Notes |
|---|-------|--------|-----------|-------|
| 0 | Determinism spike | ✅ Complete | `phase0-determinism-spike/` | Done 2026-06-01 — premise HOLDS; 2 gate refinements (decompressed-content gate; 2-line @PG). `SPIKE_determinism.md` |
| 1 | CLI + options + discovery | ✅ Complete | `phase1-cli-options-discovery/` | Done 2026-06-01. Coverage COMPLETE; dual code-review findings folded in (case-sensitive FASTA match lockstep w/ genome-prep, pbat/multicore validation, deferred-flag notice, README). 18 unit + 15 integration tests, clippy/fmt clean. Depends on #0 |
| 2 | Read conversion (FastQ SE directional) | ✅ Complete | `phase2-read-conversion/` | Done 2026-06-01. Coverage COMPLETE; dual code-review (both APPROVE) findings folded (deferred-flags, --mm2_maximum_length die, --prefix dot-trim, seqid_tab_count, double-uc). 36 unit + 15 integration tests, clippy/fmt clean. NOT yet committed. Depends on #1 |
| 3 | Single-instance align + SAM parse | ✅ Complete | `phase3-single-instance-align-parse/` | Done 2026-06-01. Coverage COMPLETE; dual code-review both APPROVE; +4 tests added. `align.rs` (SamRecord + AlignerStream peek/advance; not wired into run()). 53 unit + 15 integration tests, clippy/fmt clean. NOT yet committed. Depends on #1, #2 |
| 4 | N-way merge + scoring + MAPQ | ✅ Complete | `phase4-nway-merge-scoring/` | Done 2026-06-01. Coverage COMPLETE; dual code-review both APPROVE (calc_mapq verified Perl-vs-Rust bit-identical); +4 tests (full MAPQ-leaf pinning + 3075/3-instance/>4). `merge.rs`+`mapq.rs`, driver wired into run() (convert→2 instances→merge→counters, no BAM). 71 unit + 15 integration tests, clippy/fmt clean. NOT yet committed. Depends on #3 |
| 5 | Genomic-seq + XM/XR/XG + SAM/BAM (SE dir) 🎯 | 📋 Planned | `phase5-genomic-seq-xm-sam-output/` | First byte-identity gate; depends on #4 |
| 6 | Reports + ambig/unmapped (SE) 🎯 | 📋 Planned | `phase6-reports-ambig-unmapped/` | Depends on #5 |
| 7 | Paired-end support 🎯 | 📋 Planned | `phase7-paired-end/` | PE byte-identity gate; depends on #5, #6 |
| 8 | Non-directional + pbat 🎯 | 📋 Planned | `phase8-nondirectional-pbat/` | All library types; depends on #7 |
| 9 | FastA + order-preserving threading 🎯 | 📋 Planned | `phase9-fasta-threading/` | Worker-invariance gate; depends on #8 |
| 10 | Real-data gate on oxy 🎯 | 📋 Planned | `phase10-realdata-gate-oxy/` | Full-scale byte-identity; depends on #9 |

## History

- 2026-06-01: Phase 4 → ✅ Complete (coverage COMPLETE; dual code-review both APPROVE, calc_mapq Perl-vs-Rust differential-verified; +4 tests; 71 unit + 15 integration).
- 2026-06-01: Phase 4 → 🚧 Implementing (`merge.rs` + `mapq.rs` + driver wired into run(); 67 unit + 15 integration tests green, clippy/fmt clean).
- 2026-06-01: Phase 4 dual plan-review complete (A+B; both caught the SAME 2 CRITICAL — strand-counters belong to Phase 5; lockstep key needs `@`-strip); folded into PLAN rev 1 (+ exact 2nd-best conditional, flag==4 die, calc_mapq float verified bit-identical).
- 2026-06-01: Phase 4 → 📝 Planning (PLAN.md written; N-way lockstep merge + scoring + strand assignment + calc_mapq, SE directional).
- 2026-06-01: Phase 3 → ✅ Complete (coverage COMPLETE; dual code-review both APPROVE; +4 tests added; 53 unit + 15 integration).
- 2026-06-01: Phase 3 → 🚧 Implementing (`align.rs`: SamRecord + AlignerStream; 49 unit + 15 integration tests green, clippy/fmt clean).
- 2026-06-01: Phase 3 dual plan-review complete (A+B, both APPROVE/no Critical); findings folded into PLAN rev 1 (chomped raw_line, child kill+wait/drain-before-wait, tag-scan field-order/i64/short-line, +6 validations).
- 2026-06-01: Phase 3 → 📝 Planning (PLAN.md written; single-instance align + SAM-parse lockstep primitive).
- 2026-06-01: Phase 2 → ✅ Complete (coverage COMPLETE; dual code-review both APPROVE; fix-pass applied + tested; 36 unit + 15 integration).
- 2026-06-01: Phase 2 → 🚧 Implementing (`convert.rs`; 34 unit + 15 integration tests green, clippy/fmt clean).
- 2026-06-01: Phase 2 dual plan-review complete (A+B; B re-run after a session-limit cutoff); findings folded into PLAN rev 1 (--icpc Phase-1 mislabel, ReadProcessing seam, flate2 dep, synthetic golden, Perl loop order + skip-bypasses-sanity).
- 2026-06-01: Phase 2 → 📝 Planning (PLAN.md written; read conversion C→T FastQ SE directional).
- 2026-06-01: Phase 1 → ✅ Complete (coverage COMPLETE; dual code-review fix-pass applied + tested; 33 tests local / 34 Linux CI).
- 2026-06-01: Phase 1 → 🚧 Implementing (crate `bismark-aligner` implemented; 24 tests green, clippy/fmt clean).
- 2026-06-01: Phase 1 dual plan-review complete (A+B); 2 Critical `aligner_options`-order fixes + 5 other corrections folded into PLAN.md rev 1.
- 2026-06-01: Phase 1 → 📝 Planning (PLAN.md written).
- 2026-06-01: Phase 0 → ✅ Complete (determinism spike: premise HOLDS; gate refined to decompressed-SAM content + 2-line @PG policy pending).
- 2026-05-31: Epic created (SPEC rev 1 approved, all forks settled); all phases → 📋 Planned.

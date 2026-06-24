# PROGRESS — `--add_barcode` / `--add_umi` (Rust aligner UMI/barcode SAM tags)

**Plan:** `PLAN.md`
**Branch / worktree:** `rust/umi-barcode` @ `/Users/fkrueger/Github/Bismark-umi` (off `origin/rust/iron-chancellor` @ `61446e7`)
**Last updated:** 2026-06-24

## Pipeline status

| Step | Status | Notes |
|------|--------|-------|
| Plan written | ✅ done | `PLAN.md` drafted 2026-06-24 |
| Manual review (user) | ✅ approved | 2026-06-24 |
| Agent review (dual `plan-reviewer`) | ✅ done | A: APPROVE w/ changes; B: APPROVE w/ minor fixes. `PLAN_REVIEW_A.md` / `PLAN_REVIEW_B.md`. Findings folded into PLAN rev 1. |
| Contract confirmation vs real data | ✅ done | Supplied S3 FastQ was the **pre-extraction fastp stage** (standard Illumina names). Contract instead **verified against `altos-labs/SeekSoulMethyl` source**: real name = `barcode_umi_alt_origname` (4-field), barcode=field0/umi=field1 (`barcode.py:393`+`addtag.py:63`); `step2.nf` runs PE dir+pbat `--add_barcode --add_umi`; dedup `--umi-tag UR --paired`. Plan rev 2 folds this in. |
| Implementation | ✅ done | 2026-06-24 on `rust/umi-barcode`. cli/config/output/merge/lib + 6 new unit tests. Local: 426 lib + full suite green, fmt + clippy(-D) clean, `--help` shows both flags. Notes in PLAN. |
| Code review (dual `code-reviewer`) + `plan-manager` | ✅ done | 2026-06-24. A+B both APPROVE (no Critical/High); plan-manager COMPLETE (0 missing/partial). Only Low polish + deferred oxy smoke. `CODE_REVIEW_A/B.md`, `COVERAGE.md`. |
| Optional polish applied | ✅ done | PE notice noun → "read pair(s)" (B Low-1) + reworded to "empty barcode/UMI field (QNAME field N)" (A nit); `___` all-underscores parse test (B Low-3). Re-ran: 426 tests + fmt + clippy(-D) green. |
| Committed + pushed (rust/umi-barcode) | ✅ done | `ef4c5ee` (feature) + plan-update follow-up; pushed to `origin/rust/umi-barcode`. NOT merged to iron-chancellor. |
| Validation gate — oxy real-data smoke | ✅ PASS | `GATE_OXY.md`: real GRCh38 + 2000 WGBS pairs (synthetic SeekSoul QNAMEs). dir single==--parallel 2 (3406/3406 CB+UR, both mates, CB/UR==field0/1); pbat tags correct; `--ambig_bam` 0 tags. |

## Key decisions

- Fidelity gate = **downstream-correct tags**, not byte-identity vs the SeekGene fork.
- Scope = **aligner crate only** (`bismark_rs`); no suite/container surface.
- Malformed QNAME → skip tag **+ one never-silent STDERR notice per run** (Reviewer A; user chose it).
- SE `$XA_tag` fork bug intentionally **not** replicated.
- `/1` mate suffix is a non-issue (adjudicated: builder QNAME = R1 FastQ header; `/1/1` stripped pre-builder).

## Next action

Confirm real QNAME shape against the supplied S3 FastQ, then await the implementation trigger. Do not edit source until "implement" / `/code-implementation`.

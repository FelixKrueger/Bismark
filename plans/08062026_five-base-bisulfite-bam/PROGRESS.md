# PROGRESS — `--five_base_bisulfite_bam` (5-Base → wgbs_tools/UXM interop)

**Issue:** [#1095](https://github.com/FelixKrueger/Bismark/issues/1095) · **Origin:** [#787](https://github.com/FelixKrueger/Bismark/issues/787) · **Plan:** [`PLAN.md`](PLAN.md)
**Base:** `dev` @ `5bf8b55` · **Branch:** not yet cut
**Last updated:** 2026-08-06

---

## Pipeline

| Step | State | Notes |
|---|---|---|
| Diagnosis | ✅ Done | Verified against wgbs_tools source, not just inferred. `patter` derives calls from `SEQ`, never `XM` |
| Spike | ⏭️ Skipped | Not needed — the three load-bearing properties were confirmed by reading `methylation.rs` / `record.rs` / `output.rs` directly (PLAN §3.1) |
| Design decisions | ✅ Done | Converter over `.pat`; aligner flag over subcommand; all cytosine contexts. Recorded in #1095 |
| Reporter input | ✅ Done | Answered 2026-08-03 — will re-run through Bismark, so the XM-driven design stands (PLAN §8.7-9) |
| Plan written | ✅ Done | `PLAN.md` rev 0 |
| Manual review | ✅ Done | Felix approved proceeding to agent review |
| Agent plan review | ✅ Done | Dual independent reviewers — `PLAN_REVIEW_A.md` (242 ln), `PLAN_REVIEW_B.md` (320 ln). Both verified the algorithm across all 4 SE indices + 8 PE combinations; **2 Critical each**, 3 contradictions resolved by re-checking against source |
| **Plan rev 1** | ✅ Done | 19 changes (`PLAN.md` §0). Algorithm unchanged; all fixes in validation, I/O plumbing, and the §3.6 masking hole |
| **Implementation** | ⏳ **NEXT** | Requires the explicit "implement" trigger. ⚠️ Blocked on generating a **soft-clip fixture** first (§9.1) — none exists in the repo |
| Code review | ⬜ Not started | Dual reviewers + coverage audit |
| PR → `dev` | ⬜ Not started | |

## Parallel track

| Item | State | Notes |
|---|---|---|
| Reply to reporter | ✅ **Posted 2026-08-06** ([comment `5203512462`](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5203512462)) | Gave him the `patter` two-constant workaround — works on the DRAGEN BAMs he already has, no re-alignment. **Awaiting his result** |
| Upstream `nloyfer/wgbs_tools` PR | ⏳ Drafted, unsent | `--five_base` / inverted-polarity flag on `patter`. **Blocked on** the reporter's result above (validation check 1) + reading the CLI plumbing |

## Open questions (none critical)

| # | Question | Assumption |
|---|---|---|
| Open-1 | Coordinate-sort + index the output? | Read-order + a loud `Note:`, per the `ubam.rs` precedent |
| Open-2 | Output naming | `<stem>.bisulfite.bam` (1:1 transform, so no fixed name) |
| Open-3 | Mask deconvolution variant sites to `N`? | Deferred — not needed at the reporter's 10X |
| Open-4 | Pursue the upstream `patter` patch too? | Yes, in parallel; does not replace this plan |

## Risks being tracked

1. `MD` re-emission is the only non-trivial algorithm — tested against an independent from-genome oracle (PLAN §9.5), not against itself.
2. The idempotence gate (PLAN §9.1) is load-bearing: a bisulfite BAM in must give a byte-identical BAM out. Do not let it weaken to "mostly identical".
3. End-to-end concordance (PLAN §9.7) needs real 5-Base data and cannot run in CI.

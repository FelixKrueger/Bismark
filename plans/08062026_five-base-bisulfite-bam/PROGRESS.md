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
| Plan rev 1 | ✅ Done | 19 changes (`PLAN.md` §0). Algorithm unchanged; all fixes in validation, I/O plumbing, and the §3.6 masking hole |
| Targeted §3.6 review | ✅ Done | `PLAN_REVIEW_36.md` (272 ln) — §3.6 was rev 1's only new design and so its only unreviewed part. 2 Critical + 11 Important |
| **Plan rev 2** | ✅ Done | 9 changes (T1–T9). **§3.6's mechanism replaced outright**: keys on the reference base `reconstruct_ref` already provides, so the CLI flag, `@PG` parser, conflict rule and fail-loud fallback are all deleted. Now has **no tunable behaviour at all** |
| Soft-clip fixture | ✅ **Done 2026-08-06** | `tests/data/five_base_bisulfite/` — 8 SE records over pUC19, leading **and** trailing clips, both indel kinds, both `XG` values. Byte-identical Rust vs live Perl v0.25.1. Confirmed 4 of the plan's invariants empirically |
| **Implementation** | ⏳ **NEXT — no gates left** | Requires the explicit "implement" trigger |
| Code review | ⬜ Not started | Dual reviewers + coverage audit |
| PR → `dev` | ⬜ Not started | |

## Parallel track

| Item | State | Notes |
|---|---|---|
| Reply to reporter (r1) | ✅ Posted 2026-08-06 ([`5203512462`](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5203512462)) | Gave him the `patter` two-constant workaround |
| Reporter's result | ⚠️ **Negative, but confounded** ([`5210728023`](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5210728023), 2026-08-07) | No change after the patch; 50 % atlas-marker capture, reference-independent. Two candidates: a **silent build failure** (`setup.py`'s `raise` is commented out) or **DRAGEN's `SEQ` not being the raw read**. He will re-run through Bismark and is reacquiring the FASTQs |
| **`patter` swap experiment** | ✅ **Done 2026-08-07 — swap CONFIRMED** | `EXPERIMENT_patter_swap.md`. Unpatched 0.0 % vs patched 100.0 % methylation against a 100 %-methylated ground truth; **both strands independently** (OT 68 calls, OB 84). So the hypothesis is sound and the reporter's negative result is a local problem |
| Reply to reporter (r2) | ✅ **Posted 2026-08-07** ([`5216492876`](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5216492876)) | The experiment result, the byte-identical-`.pat` check, the DRAGEN-`SEQ` question, and confirmation that Bismark→converter is the path. **Awaiting his two answers** — neither blocks #1095 |
| Upstream `nloyfer/wgbs_tools` PR | ⏳ Drafted, unsent | Validation **1 and 4 now pass**. Outstanding: check 2 (EM-seq default path byte-identical) + check 3 (real paired data) + reading the CLI plumbing |

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

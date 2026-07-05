# Plan Coverage Report

**Mode:** B (executed gate vs. plan spec — "Mode B-style": the implementation under audit is the two bash harnesses + the captured `GATE_OXY.md` results, not Rust production code)
**Plan(s):** `phase10-realdata-gate-oxy/PLAN.md` (rev 2)
**Implementation audited:** `phase10_subset_strict_gate.sh`, `phase10_fullscale_content_gate.sh`, `GATE_OXY.md`
**Date:** 2026-06-04
**Verdict:** COMPLETE — every plan-specified cell, gate leg, and V0–V15 check is exercised by a harness and has a result in `GATE_OXY.md`. 2 DEVIATED items (both documented + re-verified); 1 cosmetic doc defect (stale TBD stub).

## Summary

- Total items: 38
- DONE: 35
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 2 (B1.5 formula bug — fixed+re-verified; Gate A run at P=16 then corrected to P=8 — documented, P-invariant)
- Cosmetic defect: 1 (GATE_OXY.md trailing "TBD" verdict stub contradicts the completed verdict above it)

## Coverage ledger

### Cells × Gate A / Gate B (§3.2, rev 2 §9 O5)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | `se_dir` Gate A (10M strict+worker+assumption) | §3.2 | DONE | All 3 legs ✅; main BAM 8,501,508 recs `cc92a5bb…` |
| 2 | `se_dir` Gate B (full content+perf, 84.0M) | §3.2 | DONE | B2 md5 `ec132828…`, 71,325,696 recs, 173 contigs, V13 ✅ |
| 3 | `pe_dir` Gate A | §3.2 | DONE | All 3 legs ✅; 17,084,770 recs `945e9d73…` |
| 4 | `pe_dir` Gate B (full, 84.0M pairs) | §3.2 | DONE | B2 md5 `ef33d791…`, 143,434,086 recs, 181 contigs, V13 ✅ |
| 5 | `rrbs_pe_dir` Gate A (GRCm39) | §3.2 | DONE | All 3 legs ✅; 12,558,088 recs `420ffb5e…` (10M head-subset) |
| 6 | `rrbs_pe_dir` Gate B (full, 46.7M pairs) | §3.2 | DONE | B2 md5 `1ea3c26b…`, 55,387,646 recs, 52 contigs; V13 n/a (regenerated) |
| 7 | `pbat_pe` Gate A (rev 2 O5, R1↔R2 swap) | rev 2 §9 O5 | DONE | All 3 legs ✅; 17,084,760 recs `b5b41c41…`, ambig 1,030,546 |
| 8 | `pbat_pe` Gate B (full, 84.0M pairs) | rev 2 §9 O5 | DONE | B2 md5 `6ff34d1f…`, 143,434,062 recs, 181 contigs; V13 skipped (no pbat oracle) |
| 9 | Non-directional NOT gated at full scale | rev 2 §9 O5 | DONE (scoped out) | Accepted residual; covered at 1M/9b + 10k Phase 8; documented in GATE_OXY verdict |
| 10 | RRBS hybrid-not-strict (O1) | §3.2, §9 O1 | DONE (scoped) | Gate A run as 10M head-subset; Gate B full content. Strict-full not taken — harness is parameterized to allow it; GATE_OXY records the hybrid choice |

### Gate A legs (§3.1)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 11 | A-strict (Rust --p1 == Perl single-core, in-order byte `cmp`) | §3.1 | DONE | harness `compare_dirs … ordered strict` (l.187); covers main BAM + ambig.bam + report + unmapped/ambiguous aux; all 4 cells byte-identical |
| 12 | A-worker (Rust --pP == Rust --p1, in-order) at Gate-B P | §3.1, §4.3 | DONE | `compare_dirs … ordered worker` (l.189); run at same P as Gate B (P=16 in the as-run; P-invariant) |
| 13 | A-assumption (Perl --multicore P == Perl single-core, multiset) | §3.1, §4.4 | DONE | `compare_dirs … content assume` (l.191) via sort→md5; ✅ all 4 cells — the A1-unlock |

### Gate B checks (§3.1)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 14 | B0 same-input/`--genome` guard | §3.1, V0 | DONE | harness l.105-106 asserts both runs get the identical `$genome` path; echoed per cell |
| 15 | B1 report identity (LC_ALL=C cmp, wall-clock filtered) | §3.1 | DONE | l.124-128; ✅ all cells |
| 16 | B1.5 count reconciliation (view -c Perl==Rust==implied; wc -l) | §3.1, V7 | DEVIATED | l.130-146; essential guard Perl==Rust ✅ all; implied-formula bug fixed+re-verified — see Gaps |
| 17 | B2 content multiset (per-RNAME sort→md5, header separate) | §3.1 | DONE | l.148-158 whole-body sort→md5 + per-RNAME md5vec on FAIL (`rname_md5vec` l.88-95); ✅ all cells |
| 18 | B2h header identity (@PG filtered) + enumerate surviving lines | §3.1, V12 | DONE | l.160-166; enumerates @HD/@SQ/@CO/@RG counts + @HD line; GATE_OXY records @HD VN:1.0 SO:unsorted, @SQ 194/61, no @CO/@RG |
| 19 | B2.5 distinct-RNAME set equality (cut -f3 \| sort -u) | §3.1, V10 | DONE | l.168-172; ✅ 173/181/52/181 contigs |
| 20 | B3 aux identity (FastQ record-ized + ambig samtools view) | §3.1, V11 | DONE | l.174-192; `paste - - - -` before sort, ambig via `samtools view` suffix-matched; ✅ all cells |
| 21 | B4 perf (/usr/bin/time -v wall + maxRSS, matched P) | §3.1, V15 | DONE | `/usr/bin/time -v -o …time` l.109/113, reported l.194-197; GATE_OXY perf tables (10M P=16 + full P=8) |
| 22 | V13 layout-invariance (fresh Perl --pP vs old --p4, WGBS only) | §3.1, V13 | DONE | l.199-206; ✅ same md5 SE+PE; correctly skipped RRBS (regenerated) + pbat (no oracle) |

### Validations V0–V15 (§8)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 23 | V0 same index per cell | §8 | DONE | B0 guard (item 14); RRBS GRCm39 index present + used |
| 24 | V1 re-based binary == merged code (tree-diff, not --version) | §8 | DONE | GATE_OXY l.8: `git diff origin/rust/iron-chancellor -- rust/bismark-aligner` empty; commit 15a34f1 |
| 25 | V2 Gate A A-strict SE-dir 10M (streaming cmp) | §8 | DONE | item 1/11; byte-identical incl report + aux |
| 26 | V3 Gate A A-strict PE-dir 10M (incl --ambig_bam via samtools view) | §8 | DONE | item 3/11; byte-identical |
| 27 | V4 Gate A A-worker 10M at Gate-B P | §8 | DONE | item 12; ✅ all cells |
| 28 | V5 Gate A A-assumption 10M (Perl mc vs sc, sort/hash) | §8 (rev-1) | DONE | item 13; ✅ all 4 cells — directly validates A1 |
| 29 | V6 Gate B B1 report identity, all cells | §8 | DONE | item 15; ✅ |
| 30 | V7 Gate B B1.5 count reconciliation | §8 | DEVIATED | item 16; essential Perl==Rust guard ✅; implied formula fixed (see Gaps) |
| 31 | V8 Gate B B2 content identity SE 84M | §8 | DONE | item 2; md5 `ec132828…` |
| 32 | V9 Gate B B2 content identity PE full | §8 | DONE | item 4; md5 `ef33d791…` (NB: §8 table V9 = the PE-content row; the layout-invariance check is V13, not §8-V9) |
| 33 | V10 Gate B B2.5 distinct-RNAME set | §8 | DONE | item 19 |
| 34 | V11 Gate B B3 aux identity (record-ized + ambig) | §8 | DONE | item 20 |
| 35 | V12 header completeness (enumerate; @HD SO:/@SQ order/@CO) | §8 | DONE | item 18; @SQ genome-derived (Linux/oxy adjudicated), no path-bearing lines survive |
| 36 | V13 Perl layout-invariance + provenance | §8 | DONE | item 22 |
| 37 | V14 content identity RRBS (mouse, GRCm39) | §8 | DONE | item 6; md5 `1ea3c26b…`, 52 contigs |
| 38 | V15 perf framed honestly (no per-core Rust-vs-Perl claim) | §8 | DONE | item 21; GATE_OXY l.74/l.100 Bowtie2-dominated/Amdahl framing, explicit "no per-core figure" |

### rev-1 / rev-2 critical fixes (must be present in harnesses)

| Fix | Source | Status | Where |
|---|---|---|---|
| LC_ALL=C everywhere | rev 1 §3.4 | DONE | `export LC_ALL=C` both harnesses (l.30 / l.34); explicit on every sort/md5/cmp |
| B1.5 count reconciliation incl. discard subtraction | rev 1 / rev 2 | DEVIATED→fixed | fullscale l.130-146; formula corrected to `(ubh-disc)×mult`, re-verified |
| FastQ record-ization (`paste - - - -`) before sort | rev 1 §3.4 / B3 | DONE | subset `n_fq_sorted` (l.102); fullscale B3 (l.180-181) |
| --ambig_bam via `samtools view` suffix-match | rev 1 §3.3 | DONE | subset `partner()` suffix-match (l.111-117); fullscale l.184-191 |
| tree-diff V1 binary verify (not --version) | rev 1 §4.0 | DONE | GATE_OXY l.8 |
| A-assumption V5 measured at 10M | rev 1 | DONE | item 13/28 |
| header enumeration V12 | rev 1 | DONE | item 18/35 |
| distinct-RNAME V10 | rev 1 | DONE | item 19/33 |
| V0 same-genome guard | rev 1 | DONE | item 14/23 |
| V13 reframe (layout-invariance/provenance, not independent correctness) | rev 1 | DONE | harness comment l.15-17; GATE_OXY l.63/l.99 states "NOT an independent-correctness signal" |
| P=8 / -S 16G resource correction | rev 2 | DONE | fullscale `SORTOPT="-S 16G"` (l.63) + comment l.59-62; GATE_OXY l.9 P=8; Gate A note P-invariant |

## Gaps (detail)

No MISSING or PARTIAL items. Two DEVIATED items, both documented and re-verified — neither affects the verdict.

### Item 16 / 30 (V7): B1.5 implied-count formula bug — DEVIATED, fixed + re-verified

**Expected (plan §3.1 B1.5, V7):** `samtools view -c` Perl == Rust == report-implied count, with the report-implied count accounting for the genomic-seq-extraction discards (the SE oracle's 36).
**Found (as-run):** the harness initially computed `implied = unique_best_hit × mate-factor`, omitting the discard subtraction → over-counted by exactly the discard count, false-flagging se_dir (off by 36) and pe_dir (off by 74 = 37 pairs × 2). RRBS reconciled outright (0 discards).
**Resolution:** The **essential guard `Perl view -c == Rust view -c` PASSED on all cells**, B2 content md5 is identical, and the discrepancy was exactly the documented discard count (benign, self-validating). The formula was corrected to `implied = (unique_best_hit − discarded) × mate-factor` (now present in `phase10_fullscale_content_gate.sh` l.139-141), re-verified against the finished BAMs (se 71,325,732−36=71,325,696 ✅; pe (71,717,080−37)×2=143,434,086 ✅), and the fix was confirmed *in situ* on the first run of the fixed harness (the pbat_pe cell, which reconciled cleanly — GATE_OXY l.80). Documented in GATE_OXY l.65 and PLAN rev 2 §8/§10. Verdict-neutral.

### Item (Gate A P=16): DEVIATED — Gate A run at P=16, plan corrected to P=8

**Expected (plan rev 2 §9 O2 / Revision History):** gates run at P=8 (16 Bowtie2 processes) under the 32c/256G cgroup.
**Found:** Gate A was executed at P=16 *before* the 32c/256G envelope was discovered (GATE_OXY l.30). Gate B + both pbat gates ran at the corrected P=8.
**Resolution:** Documented in PLAN rev 2 Revision History (c) and GATE_OXY l.30/l.41. The correctness verdict is **worker-count-invariant** (the entire point of the 9b worker-invariance proof + Gate A A-worker), so P does not affect the byte/content result — only Gate A's *perf row* is at P=16. Verdict-neutral.

### Cosmetic defect (not a coverage gap): GATE_OXY.md trailing TBD stub

**Found:** `GATE_OXY.md` carries a completed "Verdict — ✅ PASS (Phase 10 complete)" section (l.85-104), but a stale second "## Verdict" heading at l.108-109 still reads `_TBD — pending Gate B 3-cell completion + pbat_pe cell + B1.5 recheck._`. All three of those preconditions are now satisfied above it. The header banner (l.4) also still says "🚧 IN PROGRESS … Gate B 3-cell run + pbat_pe cell pending" despite both being complete.
**Impact:** Cosmetic only — the authoritative PASS verdict and full results table are present and supported by evidence. The trailing stub and stale banner should be removed/updated for a clean final doc, but this is not a coverage gap (every required result exists).

## Verdict

**COMPLETE.** Every plan-specified cell (`se_dir`, `pe_dir`, `rrbs_pe_dir`, `pbat_pe`) is gated at **both** Gate A (10M) and Gate B (full scale); the two intentionally-scoped-out cells (non-directional, RRBS strict-full vs O1 hybrid) are documented risk-acceptances, not gaps. Every Gate A leg (A-strict/A-worker/A-assumption) and Gate B check (B0/B1/B1.5/B2/B2h/B2.5/B3/B4/V13) is exercised by the harnesses and has a recorded result. All V0–V15 checks are present and PASS. Every rev-1 critical fix (LC_ALL=C, count reconciliation incl. discard subtraction, FastQ record-ization, ambig suffix-match, tree-diff V1, A-assumption V5) and rev-2 correction (P=8, -S 16G) is in the harnesses.

The PASS verdict is supported by the evidence: B2 sorted-multiset md5 identical Perl-vs-Rust on all four cells, B1.5 essential guard (Perl view -c == Rust view -c) PASS on all, header byte-identical (@PG filtered, no path-bearing lines), distinct-RNAME sets identical, V13 four-layout convergence on WGBS. The two DEVIATED items are both documented and re-verified and do not affect the byte/content verdict.

**Recommended (non-blocking) cleanup before merge:** delete the stale trailing "## Verdict — TBD" stub (GATE_OXY.md l.108-109) and update the "🚧 IN PROGRESS" status banner (l.4) to ✅ COMPLETE, so the final results doc is internally consistent.

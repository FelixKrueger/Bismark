# Phase E PLAN review — Reviewer B

**Target:** `plans/05292026_bismark-coverage2cytosine/phase-e-byte-identity-gate/PLAN.md` (rev 0)
**Reviewer:** B (independent; no shared state with Reviewer A)
**Date:** 2026-05-30
**Method:** read PLAN + SPEC (§2/§3/§5/§9/§12.3/§13/§15/P10) + EPIC + the `phase_h_se_matrix.sh` house pattern, then ran **live Perl `coverage2cytosine` v0.25.1** (repo-root binary, verified `Version: v0.25.1`) over 8 of the 9 matrix cells on the committed tiny fixture (`rust/bismark-coverage2cytosine/tests/data/phase_b/{genome,in.cov}`) to verify every claim cheaply.

---

## TOP-LINE VERDICT: **REQUEST-CHANGES**

The plan is well-structured, faithfully models the proven `phase_h_se_matrix.sh` house pattern (fail-CLOSED discipline, exit 0/1/2, SIGINT trap, pre-flight gates), and the differential-check idea is sound. But live-Perl testing surfaced **two correctness bugs that would make the harness mis-behave on the real run** — a wrong expected-filename derivation for the merged/discordant cov (verified against Perl source line 1766-1782) and a non-empty guard that false-FAILs legitimately-empty per-chr split reports and a potentially-empty `merge_disc` merged-cov. There is also a genuine gzip-stream-compare fail-open hole and a CX disk estimate that is ~50-90% too low. None are design-fatal; all are fixable in a rev 1. Because the filename bug guarantees a false-FAIL on the very first real run and the disk estimate could blow oxy's cap, I land on REQUEST-CHANGES rather than APPROVE-WITH-CHANGES.

**Finding counts:** Critical 2 · Important 4 · Minor 4 · Nit 2

---

## CRITICAL

### C1 — Merged/discordant cov filename derivation is wrong (`{stem}` vs report-stem). Guaranteed false-FAIL on the real run.
**Plan sections:** §3.2 (cells `merge`/`merge_disc`/`merge_gzip`), §3.4, §3.6.5.
**Issue:** The plan names the merged/discordant outputs `{stem}.merged_CpG_evidence.cov` and `{stem}.discordant_CpG_evidence.cov`, where `{stem}` (SPEC §5) = `-o` with `.CpG_report.txt`/`.CX_report.txt` stripped — i.e. for `-o out`, `{stem} = out`, giving `out.merged_CpG_evidence.cov`. **That file never exists.** Perl derives the merged/discordant base from the **CpG report filename** (`$global_cyt_report`) with only `.gz` and `.txt` stripped, NOT from `{stem}`:

```
coverage2cytosine:1766-1782
  $CpG_report_file =~ s/\.gz$//;  $CpG_report_file =~ s/\.txt$//;   # out.CpG_report.txt -> out.CpG_report
  $pooled_CG =~ s/$/.merged_CpG_evidence.cov/;                       # out.CpG_report.merged_CpG_evidence.cov
  $disco_CpG_report =~ s/$/.discordant_CpG_evidence.cov/;            # out.CpG_report.discordant_CpG_evidence.cov
```

**Live Perl (verified):** with `-o out`, the files are `out.CpG_report.merged_CpG_evidence.cov`, `out.CpG_report.merged_CpG_evidence.cov.gz`, and `out.CpG_report.discordant_CpG_evidence.cov` — the `.CpG_report` infix is retained. The plan's `{stem}.merged_CpG_evidence.cov` would resolve to `out.merged_CpG_evidence.cov`, which the existence-guard (§3.4 step 1) would flag as a missing required output ⇒ **false-FAIL of the `merge`/`merge_disc`/`merge_gzip` cells on the very first real oxy run** (a long, expensive run). (SPEC §5 carries the same imprecision; the *plan* is what wires the harness and must get it right.)
**Why it matters:** Wastes a multi-hour oxy run on a harness bug, not a Rust bug — and risks masking it as a "Rust failed" signal.
**Fix:** Derive the merged/discordant expected names from the actual CpG-report filename the cell produced: `<report_basename_minus_.txt[.gz]>.merged_CpG_evidence.cov[.gz]`. Concretely, if `-o` is unsuffixed (`out`), expected = `out.CpG_report.merged_CpG_evidence.cov`. State the rule explicitly in §3.2 and §3.4 and pin it with a fixture assertion in V8 before trusting a green run.

### C2 — CX-report disk estimate is ~50-90% too low; the disk model may not fit oxy even with the mitigations.
**Plan sections:** §3.2 note ("~1B+ lines / ~40 GB plain"), §6 ("~40 GB plain → ~10 GB gz per side", "Peak disk ≈ ~20 GB"), §10 Q1, SPEC §12.3.
**Issue:** A CX report emits **every cytosine on both strands** (every genome `C` as `+` and every genome `G` as bottom-strand `-` C). hg38 is ~3.1e9 bp at ~41% GC ⇒ ~1.27e9 C **plus** ~1.27e9 G ≈ **~2.5 billion lines**, not "~1B+". At ~25-30 bytes/line that is **~65-75 GB plain**, not ~40 GB. The headline figure is low by roughly 1.6-1.9×.
**Why it matters:** The entire Q1 risk ("does it fit oxy's ~99 GB?") hinges on this number. The mitigations (gzip + stream-compare + purge) still hold *qualitatively* — the plain CX is never materialized, and gzip-per-side is what lands on disk — but the **peak-disk claim "≈ ~20 GB" is built on the wrong base**. Real gz of a ~70 GB text CX at ~5-6× ≈ ~12 GB per side ⇒ ~24 GB for the two sides written concurrently, plus the cov input + genome + small verdict files. That is *probably* still within ~99 GB with the 30 GB floor, but the plan should re-derive it from the corrected base, not assert ~20 GB from a ~40 GB plain figure.
**Fix:** Recompute the CX line/byte estimate (~2.5e9 lines, ~65-75 GB plain, ~12 GB gz/side ⇒ ~24-26 GB concurrent peak). Re-state Q1's headroom against the corrected peak. Keep the Q1 fallback (chr20-22 subset genome for the `cx` cell) but tie the *decision threshold* to the corrected number, and have the first-session disk-headroom pre-flight print the **measured** CX gz size so the estimate is validated, not assumed.

---

## IMPORTANT

### I1 — Non-empty guard false-FAILs legitimately-empty per-chr split reports.
**Plan sections:** §3.4 step 1 (non-empty guard), §3.5 (split-cell handler).
**Issue:** §3.4 makes a missing-or-zero-byte output a FAIL on either side, exempting only the discordant file. §3.5 says "Compare each per-chr report file byte-for-byte" with no empty-exemption. **Live Perl (verified):** the split cell produced `out.chrscaf_short.CpG_report.txt` at **0 bytes** — legitimately empty (the `scaf_short` chromosome is `CG`, 2 bp; its only CpG's `-` partner is the last genome base, dropped by the §7.2 guard-2 last-base exclusion). A 0-byte per-chr report is a **valid, expected** output for any short/CpG-free scaffold. The §3.4 non-empty rule applied to split per-chr files would FAIL the `split` cell on hg38, which has unplaced/short contigs.
**Why it matters:** hg38 has many short/unplaced contigs (`chrUn_*`, `*_random`, `chrM` edges) → near-certain false-FAIL of the `split` cell on the real run.
**Fix:** In §3.5, exempt per-chr split reports from the non-empty rule — use **existence + byte-equality** only (both sides 0 bytes ⇒ PASS; one side 0, other non-empty ⇒ the `cmp` already FAILs). The whole-genome `default`/`cx` reports keep the non-empty rule (they cannot legitimately be empty on a real cov). The split file-**name-set** equality check (§3.5) remains the structural guard.

### I2 — `merge_disc` merged-cov can be legitimately empty; §3.4/§3.6.5 non-empty rule mis-scoped.
**Plan sections:** §3.4 step 1, §3.6.5 ("`merge` merged-cov non-empty").
**Issue:** §3.6.5 and §3.4 require the merged-cov to be non-empty. **Live Perl (verified):** on the fixture, `--merge_CpGs --discordance_filter 10` routed the only CpG pair (50.19% vs 100%, Δ>10) to the discordant file and left `out.CpG_report.merged_CpG_evidence.cov` at **0 bytes**. So in the `merge_disc` cell the merged-cov can legitimately be empty (every measured pair discordant). On the real 10M dataset the plain `merge` cell's merged-cov will be non-empty, but the `merge_disc` merged-cov could be empty for a chromosome-poor subset or aggressive filter.
**Why it matters:** Risks a false-FAIL of `merge_disc` if §3.4's non-empty rule is applied uniformly.
**Fix:** Scope the merged-cov non-empty assertion to the plain `merge` cell only. For `merge_disc`, require **existence + Rust≡Perl byte-equality** for *both* merged-cov and discordant-cov (either may be empty; both-empty-and-equal ⇒ PASS). Add an explicit per-cell "may-be-empty" stream attribute rather than a single global discordant-only exemption.

### I3 — gzip stream-compare via process substitution can false-PASS on a corrupt/truncated gz (decompress exit status is swallowed).
**Plan sections:** §3.4 step 2 (`cmp -s <(gzip -dc R) <(gzip -dc P)`), §6, V3.
**Issue:** `cmp -s <(gzip -dc R) <(gzip -dc P)` compares only the **byte streams** — the `gzip -dc` exit codes inside the process substitutions are NOT visible to `cmp` (no pipefail across `<(...)`). **Live test (verified):** good-vs-corrupt correctly DIFFERs (different lengths), but **two identically-truncated gz files decode to identical partial bytes ⇒ `cmp` says IDENTICAL = false PASS.** More generally, if a Rust gz is truncated such that its decompressed prefix equals the full Perl stream up to that length AND lengths coincide, `cmp` passes while the Rust output is actually corrupt/short. This is precisely the fail-open class the harness's `count_mbias_rows` lesson (`phase_h_se_matrix.sh:387-403`) was written to kill, transplanted into the gzip path.
**Why it matters:** The gzip cells (`cx`, `gzip`, `merge_gzip`) are the heaviest and most disk-stressed; a truncated-on-disk-full Rust gz is the *most likely* real failure on oxy, and it is exactly the one this compare can swallow.
**Fix:** Make decompress status explicit. Either (a) decompress to a checked temp file with `gzip -t` integrity-test first (`gzip -t R && gzip -t P` before the byte-compare), or (b) capture `${PIPESTATUS[@]}`/`wait` on the substitutions, or (c) compare decompressed **byte counts** as a secondary assertion (`gzip -dc R | wc -c` == `gzip -dc P | wc -c`) so a truncation that happens to prefix-match still FAILs. Add a dedicated self-test to V1/V3: feed a deliberately-truncated Rust gz and assert the matrix exits 1.

### I4 — `cx` line-count differential (§3.6.1) has a cross-cell + purge ordering hazard and a unit mismatch.
**Plan sections:** §3.6.1 ("`cx` line count > `default` line count"), §3.7 (purge-on-pass), §5 implementation step 8.
**Issue:** Two problems. (a) **Unit/decompress mismatch:** the `cx` cell is `--CX --gzip` (gzipped) while `default` is plain. The differential must compare `gzip -dc cx_CX_report | wc -l` against `wc -l default_CpG_report` — the plan says "Compare decompressed CX vs CpG report line counts" but does not state the CX side needs decompression *and* that the CX cell's own purge (§3.7) may have deleted the gz before the post-loop differential pass runs. (b) **Ordering hazard:** §3.7 purges large outputs on a cell's PASS; §3.6 runs "after the cell loop". Step 8 acknowledges this and says stash counts during the loop — good — but the cell table and §3.6 read as if they re-open files post-loop. The two sections are in tension and a naive implementer following §3.6 literally would read deleted files ⇒ count 0 ⇒ either a false-FAIL (`0 > 18` is false) or, worse, `0 < default` quietly satisfying the `thr` check direction.
**Why it matters:** A differential that reads a purged file silently computes against 0 — a textbook fail-open (count defaults that make the inequality vacuously true/false).
**Fix:** Make §3.6 authoritative that **all differential inputs (line counts, non-empty flags) are captured during the cell loop, before purge**, and the post-loop pass only compares the stashed scalars. Remove the "reading the recorded (or retained) outputs" phrasing in step 8. Explicitly note the `cx` count is taken from the decompressed stream.

---

## MINOR

### M1 — Perl `--version` assertion string differs from the extractor's; the plan's "adapt the extractor's grep" needs the exact c2c string.
**Plan sections:** §3.1.5, §5 step 2, Assumption A10.
**Issue:** The extractor harness greps `Bismark Extractor Version: v0.25.1`. **Live Perl (verified):** c2c's banner line is `Version: v0.25.1` (indented; the word "coverage2cytosine" is on a *separate* line above it). A copy-paste of the extractor's grep pattern will never match ⇒ the pre-flight version gate would always exit 2 (or, if written fail-open, never assert). The plan flags this (A10) but should pin the exact greppable token now: e.g. `grep -E 'Version: v0\.25\.1'` (and assert the surrounding `coverage2cytosine` banner line to avoid matching a stray "Version:" from another tool).
**Fix:** Specify the literal assertion against the verified string `Version: v0.25.1` plus a `coverage2cytosine` banner-line check; note STDOUT vs STDERR (the banner prints to STDOUT here).

### M2 — `LC_ALL=C` rationale overstated: Perl's internal `sort` is already bytewise.
**Plan sections:** §3.1.8, Assumption (§8 "any sort-dependent step must be bytewise").
**Issue:** `coverage2cytosine` does NOT `use locale`, so its `sort keys` (lines 66/67 context summary, 722 uncovered chromosomes — verified) is plain bytewise `cmp`, locale-independent. `LC_ALL=C` in the harness therefore guards only the **harness's own** `ls | sort` / `comm` in the split file-name-set diff (§3.5) and any shell sorting — NOT Perl's output ordering. The setting is correct and harmless, but the rationale should be scoped so a future maintainer doesn't think it changes Perl's behavior.
**Fix:** Reword §3.1.8 to "guards the harness's own `sort`/`comm`/`ls` ordering (the split file-name-set diff); Perl's internal ordering is already bytewise."

### M3 — v1.x flag rejections (SPEC §3 ⛔) silently omitted from scope.
**Plan sections:** §2, §3.2, §7 ("Exercises every Phase B/C/D code path").
**Issue:** The plan never states whether `--gc`/`--nome-seq`/`--drach`/`--ffs` CLI rejections (SPEC §3, P9) are in or out of this gate's scope. They are Phase-A CLI behavior, not output-stream byte-identity, so out-of-scope is defensible — but the omission is silent. A reader can't tell if it was a deliberate decision or a gap.
**Fix:** Add one line to §2 or §7: "v1.x flag rejections (SPEC §3 ⛔) are validated by Phase A unit tests; this gate asserts byte-identity only on the v1.0 valid-flag streams." (Optionally a trivial 10th smoke cell asserting Rust exits non-zero on `--gc`, but unit tests already cover it — not required.)

### M4 — `merge`/`merge_gzip` cells drop the report + summary streams from comparison.
**Plan sections:** §3.2 (`merge_gzip` lists only the merged cov; `merge` lists report+cov+summary but `merge_gzip` omits both report.gz and summary).
**Issue:** **Live Perl (verified):** `--merge_CpGs --gzip` produces `out.CpG_report.txt.gz` **and** `out.cytosine_context_summary.txt` **and** the merged cov.gz — three files. The `merge_gzip` cell compares only the merged cov.gz, leaving the gzipped CpG report and the (always-plain) summary unasserted in that cell. They *are* asserted in the `gzip` and `default`/`merge` cells, so coverage isn't lost globally, but the cell is under-specified vs what Perl emits and an existence-guard that doesn't know about the extra files won't notice if Rust drops one.
**Fix:** Either add `report.txt.gz` + summary to the `merge_gzip` cell's compared streams (cheap — they're small relative to CX), or add an explicit note that `merge_gzip` intentionally asserts only the merged-cov.gz because the other two are covered by `gzip`/`default`. Prefer the former for completeness.

---

## NIT

### N1 — `default` plain CX path is never byte-asserted (CX only ever runs with `--gzip`).
**Plan sections:** §3.2 note, scrutiny item.
**Issue:** The `cx` cell carries `--gzip` purely for disk, so the **plain** `CX_report.txt` (the common user path) is never directly byte-compared — only its decompressed form. The decompressed-compare is a faithful substitute for *content* (SPEC P10 gates decompressed bytes, not the gz container), so byte-identity of the report content is not weakened. But the *plain-file writer code path* (BufWriter<File> for CX, vs GzEncoder) is exercised only on `default`/`zero`/`thr` (all CpG), never on CX. A CX-specific plain-writer bug (e.g. a flush/newline difference that only manifests without the gz layer) would be missed. Very low risk given the writer is shared, but worth one line.
**Fix:** Note the residual gap, or (if Q1 disk allows) run one small plain-CX assertion on the chr20-22 subset to cover the plain CX writer.

### N2 — Empty-cov die leaves partial files (informational; not a matrix cell).
**Plan sections:** §3.4, §11 self-review (empty-input edge).
**Issue:** **Live Perl (verified):** an empty cov input makes Perl die (rc=255) **after** opening (and thus creating) `out.CpG_report.txt` + `out.cytosine_context_summary.txt` as 0-byte files (filehandles open before the `$last_chr`-undefined die). Not a matrix cell (the real cov is non-empty), so no gate impact — but if a cell's input were ever unexpectedly empty, the harness would see a non-zero exit AND two 0-byte files; §3.4's "missing/empty = FAIL" handles it correctly. Mention it so the Rust port's empty-cov behavior (SPEC §7.6 `EmptyCoverageInput` error — does Rust also leave partial files? It uses `cleanup_partial_output_on_err`, so it likely does NOT) is a *known* accepted divergence in partial-file artifacts on the error path (STDERR/error path is not gated, so fine).
**Fix:** One line in §8/§11 noting empty-cov is out-of-matrix and the error-path partial-file divergence is not gated.

---

## What I verified against live Perl v0.25.1 (repo-root binary, `Version: v0.25.1`)

| Claim | Result |
|---|---|
| Differential §3.6.1 `cx > default` | ✅ cx=25 lines, default=18 |
| Differential §3.6.2 `zero ≠ default` | ✅ differs (coords −1); **same line count (18)** — line-count diff would NOT catch it; plan correctly uses full-file `cmp`, not a count, for `zero` |
| Differential §3.6.4 `thr < default` | ✅ thr=2, default=18 |
| Differential §3.6.6 `split > 1 file` | ✅ 4 per-chr reports |
| §3.5 last-chr summary non-empty, rest empty | ✅ only `scaf_short` (last) summary = 1310 B; chr1/chr2/chr3uncov = 0 B |
| §3.4 merged-cov filename | ❌ **`out.CpG_report.merged_CpG_evidence.cov`**, not `{stem}=out.merged…` (C1) |
| §3.6.5 merged-cov non-empty | ⚠️ empty in `merge_disc` when all pairs discordant (I2) |
| §3.4 split per-chr report non-empty | ❌ `scaf_short` report = 0 B, legitimately (I1) |
| §3.4 gzip stream-compare fail-closed | ⚠️ false-PASS on identically-truncated gz (I3) |
| CX disk size ~40 GB / ~1B lines | ❌ ~2.5e9 lines, ~65-75 GB plain (C2) |
| `df -Pk \| awk int($4/1024/1024)` | ✅ correct GB arithmetic |
| Perl `--version` string | ⚠️ `Version: v0.25.1` (not extractor's "Bismark Extractor Version:") (M1) |
| Empty-cov ⇒ Perl die rc=255 + partial files | ✅ confirmed (N2) |

## Strengths (keep as-is)
- Fail-CLOSED existence/non-empty guard placed FIRST (§3.4 step 1) — correctly mirrors the `count_mbias_rows` lesson.
- V1 deliberate-diff self-test made a **mandatory** checklist step (§5.12, §11 risk d) — exactly the right defense against the dual-driver fail-open trap; keep it mandatory and extend it to the gzip-truncation case (I3).
- Cross-cell differential checks (§3.6) close the "both binaries no-op a flag" hole that per-cell `cmp` can't see — the right second axis.
- Exit codes 0/1/2 (no spurious perf gate) correctly match SPEC §10.7 (perf advisory for c2c v1.0).
- SIGINT/TERM trap preserving partial evidence + purge-on-pass/keep-on-fail disk discipline are well-judged for the oxy constraint.
- Q1/Q2/Q3 correctly classified as operational-resolved-first-session (the disk-headroom pre-flight is the clean refusal mechanism), not design-blocking — agreed.

## Recommendation
Fold C1, C2, I1, I2, I3, I4 into a rev 1 (all are precise, fixture-verified, and local to the harness spec). M1-M4 and N1-N2 are quick clarity edits. After rev 1, this is a solid APPROVE — the design is sound; the gaps are filename/edge/fail-open details that only live-Perl testing exposes, which is why catching them here (not on a multi-hour oxy run) is the whole point of the gate.

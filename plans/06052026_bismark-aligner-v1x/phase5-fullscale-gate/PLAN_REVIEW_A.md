# PLAN_REVIEW_A — Phase 5: combined v1.x real-data gate (10M strict) + README bump + PR

**Reviewer:** Plan Reviewer A (independent, fresh context)
**Plan:** `plans/06052026_bismark-aligner-v1x/phase5-fullscale-gate/PLAN.md` (rev 1)
**Date:** 2026-06-05
**Verdict:** **APPROVE-WITH-CHANGES**

---

## Verdict summary

This is a sound, well-scoped gate plan. The load-bearing claims — HISAT2-single-core-only is the *only* faithful comparison, the 10M in-order streaming-`cmp` methodology, and the minimap2 worker-invariance leg — are all **correct** and I re-derived them against the crate code and the per-phase harnesses. The four open-question resolutions are sound, and I verified OQ-5a directly on oxy (the mouse indexes exist). No production Rust code changes → near-zero regression surface.

The changes I'm requesting are **harness-construction precision** items (the right comparator, the right naming-token assertion at 10M, the FastQ-aux-record-ization wording, and one honestly-surfaced-but-under-specified caveat about the RRBS dataset's library type). None block the gate's *logic*; all should be fixed before the harness is written so a cell can't pass-while-wrong or false-FAIL.

- **Critical:** 0
- **Important:** 4
- **Optional:** 5

**Single most important change:** §4 step 3 says the harness "reuses the `run_cell` skeleton from `phase4_minimap2_se_gate.sh`", but that Phase-4 skeleton compares via `diff <(filter_sam a) <(filter_sam b)` — which **buffers both ~2–3 GB SAM streams** at 10M (the exact OOM hazard the Phase-10 plan explicitly engineered around). The plan's own §3.1/§5 correctly call for **streaming `cmp`** — so the harness must adapt the Phase-10 `cmp_files` comparator (+ the bounded `sed`-window diagnosis), **not** Phase-4's `diff`. Make §4 step 3 cite the Phase-10 `cmp_files` skeleton, not the Phase-4 `diff` one.

---

## Re-derivation of the two load-bearing claims (done independently)

### Claim 1 — "HISAT2 single-core-vs-single-core is the ONLY faithful comparison; the Phase-10 content-multiset shortcut is INVALID for HISAT2." ✅ CONFIRMED

I verified this from the code, not the prose:

- `rust/bismark-aligner/src/config.rs:226` hard-rejects `aligner == Hisat2 && multicore > 1` with the splice-discovery rationale. The reject is real and load-bearing.
- The mechanism is correct and is **not** the Bowtie 2 / minimap2 per-read-independence: HISAT2 discovers splice sites **batch-globally across the whole input read set**, so chunking the reads changes the discovered splices → changes alignments. The Phase-2a finding is quantified in the code comment (config.rs:221): *single-core 1310 spliced vs `--multicore 8` 1219 spliced on the 1M oxy subset.* This is direct evidence that **Perl HISAT2 `--multicore` is NOT multiset-invariant** — so the Phase-10 A1 assumption (Perl `--multicore P` content == Perl single-core content) **does not hold for HISAT2**, and any content-multiset gate against a Perl `--multicore` HISAT2 oracle would compare against a *different* (worker-count-dependent) multiset. The plan is exactly right to demand Perl-single-core ↔ Rust-single-core, strict, with no worker leg for HISAT2.
- Corollary the plan states correctly: there is no Rust HISAT2 worker leg either (the binary rejects it), so the only HISAT2 axis is single-core ↔ single-core.

This is the crux of the phase and it is **correct**.

### Claim 2 — "All single-core backend outputs are in INPUT ORDER, so a streaming `cmp` is valid (not a false-FAIL on reordering)." ✅ CONFIRMED

I traced the single-core path in `lib.rs`:

- `run_alignment` dispatches `multicore == 1` (the Phase-5 case for HISAT2; and `--parallel 1` for Bowtie 2/minimap2) to the **direct** `process_*_chunk` → `drive_merge` path (lib.rs:112–126, 250, 577).
- `drive_merge` (lib.rs:587–603+) opens the **original read file** and reads it **sequentially line-by-line** (`read_until` in a `loop`), driving the lockstep merge per read and emitting one BAM record per read **in that read-file order**. `merge.rs` advances each aligner stream in lockstep with the driving read order.
- Therefore Rust single-core output is strictly in input order, for **all three** backends (the merge/output core is aligner-agnostic — that was the whole point of the Phase-4 faithful-port abstraction).
- Perl single-core is the same single-threaded streaming model (it is the oracle the Rust path was built to reproduce in-order; the 9b/Phase-10 gates already proved Rust `--parallel 1` == Perl single-core byte-identical at 1M/10M for Bowtie 2, and the per-phase 2a/2b/4 gates proved it for HISAT2/minimap2 at 10k/1M with `diff` of `samtools view -h`, which only passes if the *order* matches).

So **streaming `cmp` is valid** for the A-strict leg and the A-worker leg at 10M. No sort/content-multiset is needed for any strict leg. The plan's methodology is sound. (minimap2 worker-invariance, see below, is also in-order on both sides → body-`cmp` is valid.)

### Minimap2 worker-invariance leg — VALID

- minimap2 PE is rejected (config.rs:245); SE is per-read-independent (no batch-global state), so Rust `--parallel P` is order-preserving (Phase 9b contiguous-chunk + ordered merge) and byte-identical to `--parallel 1`. The Phase-4 harness already proved this at 1M via a **body-only** `samtools view` (no `-H`) compare — correct, because the `@PG CL:` embeds `--parallel P` and would legitimately differ. The Phase-5 plan reproduces this (§3.1 "minimap2 only — A-worker leg ... SAM body ... `@PG` CL embeds `--parallel` → body-only"). Sound. With `mm2_se_dir` (Rust `--p1` == Perl) this gives Rust `--pP` == Perl transitively at 10×.

### Bowtie 2 anchor — meaningful but modest

The Bowtie 2 cells re-prove Rust `--p1` == Perl single-core at 10M, which Phase 10 already gated at full scale (84M). At 10M it is strictly a **regression anchor** (did the v1.x multi-backend generalization of `aligner.rs`/`options.rs`/`discovery.rs` perturb the frozen Bowtie 2 path?). That is a legitimate and cheap thing to verify — the v1.x diff *did* touch shared seams. I'd keep it. (See Optional O-3 on whether `bt2` mouse is redundant.)

---

## Logic review (findings)

### IMPORTANT I-1 — §4 step 3 cites the wrong comparator skeleton (the buffering hazard)
**§4 step 3** says the harness "reuses the `run_cell` skeleton from `phase4_minimap2_se_gate.sh`." That skeleton (lines 65, 74) compares with `diff <(filter_sam "$pbam") <(filter_sam "$rbam")` and `diff <(zcat ...) <(zcat ...)` — process-substitution `diff` **buffers both streams in memory**. At 10M that is ~2–3 GB of SAM text per side for SE, ~2× for PE — the precise hazard Phase 10 §3.4 engineered around by switching to streaming `cmp`. The plan's §3.1 and §5 already say "streaming `cmp` (O(1) memory)" + "on mismatch, map the byte offset to a line window with `sed` — never re-`diff` the full stream" — i.e. the plan *intends* the Phase-10 `cmp_files` comparator. **Fix:** make §4 step 3 say it adapts the **Phase-10 `cmp_files`** comparator (+ the `sed`-window-on-mismatch diagnosis recipe) and uses the Phase-4 only for its *run* skeleton (identical-argv-into-same-`-o`, Perl-moved-aside, `@PG`/wall-clock filters). As written, an implementer following step 3 literally would copy the OOM-prone `diff`. This is purely a wording/citation fix but it is the highest-value one.

### IMPORTANT I-2 — `@PG` filter regex differs between the cited skeletons; pin "whole-`@PG`-block"
The Phase-4/2a/2b harnesses filter with `grep -v 'ID:samtools'` (drops only the *samtools* `@PG` line). The Phase-10 harness filters with `grep -v '^@PG'` (drops the **whole `@PG` block**). The Phase-5 plan §3.1/§3.3 correctly demands the **whole `@PG` block** filtered (because the Bismark `@PG CL:` embeds the per-run argv). **But** the per-phase skeletons it names use the narrower `ID:samtools` filter. At 10M with *identical* argv on both sides, the Bismark `@PG CL:` lines are actually identical (same argv), so `ID:samtools`-only filtering *happens to* pass for the Perl-vs-Rust strict legs — **except** the abs-path-in-the-samtools-pipe `@PG` and any tool-version drift. More importantly, the minimap2 **A-worker** leg varies `--parallel` in the Bismark `@PG CL:`, so it *must* use a body-only (`samtools view`, no `-H`) compare (Phase-4 does this correctly in `run_invariance_cell`) — the whole-`@PG`-block filter is a header-leg concern. **Fix:** in §3.1/§4 step 3, explicitly state the comparator uses `samtools view -h | grep -v '^@PG'` (whole block) for the header-bearing strict legs and body-only `samtools view` for the worker leg — and don't inherit the narrower `ID:samtools` filter from the 2a/2b/4 skeletons. (Plan already says "whole `@PG` block filtered" in §3.1 — this is making the *harness construction* match the prose so an implementer doesn't copy `ID:samtools`.)

### IMPORTANT I-3 — the naming-token check is weaker at 10M-strict than the plan implies; assert it explicitly
§3.1 says: *"Naming check is implicit: a basename match between Perl/Rust outputs proves the `_bismark_{hisat2,mm2,bt2}` token."* That is true **only if the harness drives off the Perl (reference) glob and asserts the role-matched Rust file exists** (the Phase-4/2a/2b harnesses do: `for pbam in "$HOLD"/*.bam; ... rbam="$OUT/$b"; if [ ! -f "$rbam" ]; then ... MISSING`). The Phase-10 `compare_dirs` also drives off the reference glob — but the Phase-10 harness then added an explicit **non-empty backstop** (`phase10_subset_strict_gate.sh:186–197`) precisely because *"compare_dirs drives off the REFERENCE glob and skips an empty match WITHOUT failing, so an empty-but-exit-0 main BAM would pass the strict gate vacuously."* Phase 5 inherits this exact pass-while-wrong risk and the plan does **not** mention the backstop. **Fix:** carry the Phase-10 non-empty/`samtools view -c` backstop (main BAM non-empty AND Perl==Rust record count) into §3/§4 — it is the cheap guard that turns the "implicit naming check" into a real one and blocks a vacuous PASS if the v1.x backend silently produced an empty BAM for a cell. (Especially relevant for the non-dir/pbat-on-directional-data cells in I-4, which legitimately land *near-zero* reads on a strand and could, if a routing bug existed, land *exactly* zero everywhere.)

### IMPORTANT I-4 — the non-dir/pbat-on-directional-data caveat is surfaced but its *consequence for the gate* is under-stated
§2/§3.2/§7 honestly flag the Phase-8 caveat ("~0 reads on the complementary strand for a directional dataset"). Good — it is **not hidden**. But two refinements:
1. For PE pbat, the plan offers the **R1↔R2-swap trick** ("via R1↔R2 swap if a real pbat signal is wanted") — this is the Phase-10-proven way to get a *non-empty* pbat signal at scale. For SE there is **no swap equivalent** (a single read can't be swapped), so `ht2_se_pbat` / `mm2_se_pbat` / `ht2_pe_pbat`-without-swap genuinely exercise the 2-of-4 (or near-zero-on-complement) routing on a directional dataset, mostly proving *routing + byte-identity-at-scale*, not new strand coverage. The plan says this. **The risk the plan does not quite close:** a cell that lands ~0 reads on a strand, combined with the absence of the I-3 backstop, can pass *vacuously* if both sides emit a (near-)empty BAM. With I-3's backstop in place, this is fully mitigated — so I-3 and I-4 are coupled. **Fix:** either (a) adopt the R1↔R2 swap for `ht2_pe_pbat` (the plan already entertains this) so it's a genuine non-empty pbat test, and explicitly state the SE-pbat/SE-nondir cells are "routing + byte-identity-at-scale, expected near-empty on the complementary strand, guarded by the I-3 non-empty backstop on the *populated* strand", or (b) demote the near-empty cells to "informational, may legitimately be ~0" and lean on the swap-pbat cell for the real signal. Right now §8 V3/V4 say "byte-identical" with no acknowledgement that a near-empty BAM is the *expected* and *gate-passing* outcome for some of these — an implementer could read a near-empty PASS as suspicious or, worse, a near-empty-on-both-sides vacuous PASS as success.

### (Sub-finding under I-4) — RRBS dataset library type
The mouse cells are `rrbs_ht2_pe_dir` / `rrbs_bt2_pe_dir` (directional). RRBS is conventionally directional, so this is correct. I confirmed the RRBS reads are present (S3 symlinks, ≥10M reads). No change needed beyond staging-to-`/var/tmp` (which the plan requires) — but worth a one-line assertion in `GATE_OXY.md` that the RRBS BAM is non-empty and lands on GRCm39 scaffolds (ties to the I-3 backstop + the scaffold-diversity claim).

---

## Assumptions review

- **"the `rust/aligner-mm2` binary == the #950 merge payload."** The plan asserts the gated binary (commit `21bac5d` on `rust/aligner-mm2`, = `49a1518` HISAT2 + minimap2) is exactly what #950 squash-merges. I confirmed `git log` shows `rust/aligner-mm2` @ `21bac5d` → `49a1518` (#949) → `fc38191` (#948, iron-chancellor). This is a clean fast-forward-able lineage. **However**, Phase 10's lesson (and the MEMORY note on the freshen that "textually auto-merged but didn't compile") is that a *re-base/freshen* before the PR can introduce drift. Phase 5 says "fold into #950" (push to the existing PR branch), so there may be **no** re-base — but if #950 has to be freshened against iron-chancellor (e.g. a README.md Milestones union-conflict, which is *exactly* what bit #947 and #948), the gated binary may diverge from the merged one. **Recommend (Optional O-1):** add a Phase-10-style tree-diff verification (`git diff origin/rust/iron-chancellor -- rust/bismark-aligner` should show only the intended v1.x delta, or be empty after a clean freshen) so the gate provably tests the merge payload. The plan currently relies on "no re-base needed" which is plausible but not asserted.

- **OQ-5a (mouse indexes present).** ✅ **Verified on oxy by me**: `~/bismark_benchmarks/RRBS_PE/genome/Bisulfite_Genome/{CT,GA}_conversion/` contain both `BS_{CT,GA}.{1..8}.ht2` (8-suffix, no `rev.*` — correct HISAT2 arity) and `BS_{CT,GA}.{1..4}.bt2` + `rev.{1,2}.bt2` (correct Bowtie 2 arity). The mouse `.mmi` is absent — which is fine, since `rrbs_mm2_se` was dropped (OQ-5c). So OQ-5a is **not** a risk; the step-0 sanity check will pass. The plan's fallback ("surface a miss, fall back to `rrbs_bt2_pe_dir` only") is correct defensive design but won't trigger.

- **Environment pins.** ✅ Verified on oxy: Bismark v0.25.1, HISAT2 2.2.2, minimap2 2.31-r1302 — all match the plan. Human `10M_SE`/`10M_PE` directional reads + human `.mmi`/`.ht2`/`.bt2` all present. The repro-tuple capture (§4 step 0/5) is the right place to record these.

- **"`/var/tmp` survives a single cell."** Honestly flagged. The MEMORY note on the ephemeral-pod recycle (lost a multi-hour Phase-4 matrix) makes this the real operational risk. The plan's mitigation (detach + per-cell off-box capture + idempotent re-run) matches the Phase-10 robustness model. Adequate.

---

## Efficiency review

- **10M in-order → streaming `cmp` (O(1) memory)** is the right call and is materially cheaper than the 84M content-multiset machinery. Correct.
- **`LC_ALL=C` everywhere** — the plan says it (§3.1/§5). For a *strict in-order* `cmp` the locale matters less than for a `sort` (cmp is byte-wise regardless), but pinning it is harmless and protects the `grep`/any incidental `sort`. Keep it.
- **Runtime feasibility.** Single-core HISAT2 at 10M is the runtime driver. There are **8 HISAT2 single-core cells** (`ht2_se_{dir,nondir,pbat}` + `ht2_pe_{dir,nondir,pbat}` + `rrbs_ht2_pe_dir`; that's 7 distinct + dir is listed twice in the matrix as SE+PE) and **each needs both a Perl and a Rust single-core run**. At single-core, HISAT2 on 10M reads is plausibly tens of minutes to a couple hours per run (HISAT2 is fast, but single-threaded × 2–4 instances × 10M is the cost) → the full HISAT2 matrix is the dominant wall-clock and is **hours**. The plan's "hours/cell" + detach/poll model is honest. **Optional O-2:** consider running the *Perl single-core* HISAT2 alignments in the background **concurrently** (they're independent cells, the box is 32c/256G per the cgroup), capturing each as it finishes — the sequential ordering in §4 step 4 is conservative for recycle-insurance but leaves cores idle. The plan's sequential-by-default is *safe*; concurrency is an optional speedup, not a requirement.

---

## Validation sufficiency

The V1–V7 table maps cleanly onto the cells. Gaps relative to Phase 10 that I checked:

- **No B1.5-style count reconciliation / `wc -l` guard is called out.** At 10M-strict-in-order, a streaming `cmp` *does* catch a truncation/drop (the streams would differ at the truncation point), so the explicit count guard is **subsumed** by the in-order `cmp` — *provided* the I-3 non-empty backstop exists to block the vacuous-empty case. So this is genuinely subsumed (unlike Phase 10's order-normalized Gate B, which needed B1.5 because a sorted-multiset compare can mask a drop+dup that nets to the same count — not a concern for in-order cmp). ✔ No separate count-reconciliation needed *given I-3*.
- **Header enumeration / distinct-RNAME / `@SQ`-order** (Phase 10 V10/V12) are likewise **subsumed by the in-order `samtools view -h` `cmp`**: the header (`@HD`/`@SQ`, `@PG`-filtered) is part of the `-h` stream and is compared byte-for-byte, and the RNAME column is part of every record line. So distinct-RNAME-set equality and `@SQ`-order are *implied* by a passing whole-stream `cmp`. ✔ The plan is right not to re-add the order-normalized machinery. **One caveat:** the *scaffold-diversity claim* (the headline "new info" of the mouse cell) is only as strong as the reads that actually map to diverse scaffolds. A 10M head-subset of an RRBS run will hit GRCm39 scaffolds proportional to their CCGG density; it will **not** necessarily touch every alt/`Un`/`random`/`M` contig (Phase 10's *full* 46.7M run hit 52 contigs). **Optional O-3:** the plan should temper the "strongest new scaffold-diversity datapoint" wording to "second-genome byte-identity at 10× the per-phase scale, with whatever GRCm39 scaffold subset the 10M RRBS reads touch" — and optionally record `cut -f3 | sort -u | wc -l` in `GATE_OXY.md` so the *actual* contig count is documented rather than claimed.
- **FastQ-aux record-ization (§3.1 "FastQ record-ized").** At in-order streaming `cmp`, the aux FastQ comparison does **not** need record-ization (`paste - - - -`) — that was a Phase-10 *content-sort* fix (sorting raw FastQ lines breaks 4-line grouping). For an in-order `cmp`, a plain `zcat | cmp` is correct and record-ization is unnecessary (and the Phase-10 `n_fq_ordered` normalizer is exactly `zcat`, no paste, for the *ordered* mode). **Minor wording fix (folded into I-1):** §3.1's "FastQ record-ized" is misleading for the in-order legs — it should say "decompressed (`zcat`) in-order `cmp`; no record-ization needed for the in-order legs (record-ization is only a content-sort concern, not used here)." As written it could lead an implementer to add an unnecessary `paste|sort` and accidentally turn a strict in-order check into a content check.

---

## Optional items

- **O-1 (binary == merge-payload tree-diff):** add a Phase-10-style `git diff origin/rust/iron-chancellor -- rust/bismark-aligner` check so the gate provably tests #950's payload, in case #950 needs a freshen before merge. (See Assumptions.)
- **O-2 (concurrency):** Perl single-core HISAT2 cells are independent — optionally run them concurrently within the cgroup budget to cut wall-clock, capturing per cell. Sequential is safe; this is a speedup only.
- **O-3 (scaffold-diversity wording + contig count):** temper the mouse "scaffold diversity" claim to match what 10M RRBS reads actually touch, and log the observed distinct-RNAME count in `GATE_OXY.md`.
- **O-4 (Bowtie 2 mouse anchor redundancy):** `rrbs_bt2_pe_dir` re-proves what Phase 10 already gated on GRCm39 at *full* scale (46.7M). At 10M it's a cheap regression anchor for the v1.x shared-seam changes — keep it, but the plan could note it adds *no new* mouse information beyond Phase 10 (the *new* mouse info is the HISAT2 mouse cell).
- **O-5 (`set -uo pipefail` + exit-code propagation):** the reused skeletons use `set -uo pipefail` (not `-e`) and accumulate `FAILED`. That's correct for a multi-cell gate (don't abort on the first cell). Just confirm the Phase-5 harness keeps `FAILED`-accumulation + a non-zero final exit, and that each cell's verdict is captured to `GATE_OXY.md` *before* the next cell (recycle insurance) — the plan says this in §4 step 4; make sure the harness echoes per-cell PASS/FAIL to stdout (the skeletons do).

---

## What the plan got right (worth stating)

- The per-backend gate-shape table (§2) is **correct** and is the heart of the phase; I re-derived HISAT2-single-core-only from the code and it holds.
- In-order streaming `cmp` is valid for every strict leg (re-derived from `drive_merge`).
- OQ-5a/5b/5c/5d resolutions are all sound; OQ-5a verified present on oxy; OQ-5c (drop `rrbs_mm2_se`) is correct (RRBS is PE, minimap2 is SE-only, and SE minimap2 is already gated on human).
- The "writes no production Rust code → zero regression surface" framing is accurate; the only durable artifacts are the gate doc/harness + the README bump riding #950.
- The non-dir/pbat-on-directional-data caveat is **surfaced, not hidden** (§2, §3.2, §7, §10 risk (c)) — the I-3/I-4 asks are about making the harness *enforce* the implied non-empty guard, not about a hidden assumption.

---

## Action items (prioritized)

**Critical:** none.

**Important:**
1. **I-1** — §4 step 3 must adapt the **Phase-10 `cmp_files`** streaming comparator (+ `sed`-window diagnosis), NOT the Phase-4 `diff`-buffering skeleton (the 10M OOM hazard). [highest value]
2. **I-2** — pin the `@PG` filter to **whole-block** (`grep -v '^@PG'`) for header-bearing strict legs and **body-only** (`samtools view`, no `-H`) for the minimap2 worker leg; don't inherit the narrower `grep -v 'ID:samtools'` from the 2a/2b/4 skeletons.
3. **I-3** — carry the **Phase-10 non-empty / `samtools view -c` backstop** into the harness so a vacuously-empty BAM can't pass the "implicit naming check"; couple it with I-4.
4. **I-4** — make the non-dir/pbat-on-directional-data *consequence* explicit: adopt the R1↔R2 swap for a genuine non-empty `ht2_pe_pbat`, and label the inherently near-empty SE cells "routing + byte-identity-at-scale, near-empty-on-complement expected, guarded by the I-3 backstop on the populated strand."

**Optional:** O-1 (tree-diff binary == merge payload), O-2 (concurrent Perl HISAT2), O-3 (scaffold-diversity wording + logged contig count), O-4 (note Bowtie 2 mouse anchor redundancy), O-5 (FAILED-accumulation + per-cell capture confirmation; and drop the "FastQ record-ized" wording for the in-order aux legs).

---

*Report written to: `/Users/fkrueger/Github/Bismark-aligner/plans/06052026_bismark-aligner-v1x/phase5-fullscale-gate/PLAN_REVIEW_A.md`*

# Phase 10 full-scale real-data gate — Code Review B

**Reviewer:** B (independent, fresh context)
**Scope:** the two bash gate harnesses that ran on oxy and produced the Phase-10 PASS verdict —
- `phase10_subset_strict_gate.sh` (Gate A: 10M strict byte-identity + worker-invariance + A1-assumption)
- `phase10_fullscale_content_gate.sh` (Gate B: full-scale content multiset + report/count/RNAME/aux/perf + V13)

**Context read (not reviewed):** `GATE_OXY.md` (verdict = PASS), `PLAN.md` (rev 2, §3.4 + §8 V0–V15), `phase9b_worker_invariance_gate.sh` (precedent).

**Central question I judged against:** *could this gate report PASS while the Rust port is actually wrong?* My answer: **the gate is fundamentally sound and the PASS is trustworthy**, but it carries **two real vacuous-pass hazards** (one Critical to the result's claimed scope, one High) and several lower-severity robustness gaps. None of them, given the *reported* artifact counts in `GATE_OXY.md`, appears to have actually fired — but a future re-run (or a typo'd path) could pass silently.

---

## Summary

The harnesses are unusually careful for bash gates: `LC_ALL=C` exported globally (so the "two independent sorts equal iff multisets equal" guarantee holds — verified line 30 / line 34), `@PG`-block filtering, wall-clock-line filtering, FastQ record-ization via `paste - - - -` before sort, a count + `wc -l` guard *before* hashing, and a deliberate `-S 16G` absolute sort buffer (not `-S 25%`) sized for the 256 GB cgroup cap. The A-assumption leg (Perl `--multicore` == single-core multiset) is the right way to convert the load-bearing A1 premise from faith into measurement, and it is genuinely the unlock for Gate B's validity.

The design is correct. The risk is concentrated in **glob/empty hazards** and **two findings that weaken the *scope* of the PASS rather than its core correctness**:

1. **(Critical-to-scope)** Every BAM-, report-, and aux-comparison loop in **both** harnesses iterates the **reference (Perl) directory** and looks up a partner on the Rust side. **An artifact that Rust produces but Perl does *not* is never compared** — and, more importantly, the loops `continue` (Gate A) or skip silently (Gate B `for pa`) on an *empty glob* with **no failure recorded**. If a whole class of output (e.g. the `--ambig_bam`, or the `--unmapped`/`--ambiguous` aux) were *absent on the reference side*, that artifact class would contribute **zero comparisons and zero failures** — a vacuous pass for that artifact.
2. **(High)** The B1.5 count formula was wrong as-run (over-counted by the genomic-seq-extraction discards) and only *didn't* sink the gate because the essential `cP == cR` guard is separate. The verdict doc says it's "fixed + re-verified", but the **fix is folded into the harness that the doc describes the *old* run with** — i.e. the reported se_dir/pe_dir Gate-B numbers came from the buggy formula, and the corrected formula was reconciled *against the finished BAMs after the fact*, not re-run. That's defensible (the essential guard passed, B2 md5 matched) but it means **V7's "report-implied" leg was never green in an actual run for SE/PE** — it was hand-reconciled. Worth flagging as a scope caveat.

Everything else is Medium/Low.

---

## Issues by area

### Area 1 — Statistical reality of the multiset md5 at 10⁸ records (probe 1)

**Sound.** The sort is a byte-wise total order (`LC_ALL=C`), identical on both sides (both inherit the global export; Gate B re-pins `LC_ALL=C sort` inline at lines 151/152/180/181/188/189/203 belt-and-suspenders). `md5sum < file` (Gate A line 88) and `... | md5sum` (Gate B) both hash the **whole canonicalized stream including the trailing newline**, and `samtools view` always terminates every record with `\n`, so there is no "final record missing newline" asymmetry — both sides go through the identical `samtools view | sort` pipe. The "two independent sorts equal iff multisets equal" claim is valid here.

- **One real subtlety (Low, disclosed-correctly):** the multiset md5 is **blind to record order by construction** — that is the entire point of Gate B, and the PLAN (§9 O6) and `GATE_OXY.md` both *explicitly* concede that a "reordering only at huge chunk size" bug is the one thing Gate B cannot see, mitigated by Gate A worker-invariance at the same `P` + 9b coprime. I concur the mitigation is adequate: Gate A runs A-worker at the **same `P`** as Gate B (line 62 of PLAN, line 189 of Gate A), so the full-scale merge *configuration* is exercised in-order at 10M. The residual is honestly bounded.
- **md5 collision at 71M–143M records:** statistically irrelevant (128-bit over a sorted text stream; a false PASS would require a sort-output collision, ~2⁻¹²⁸). Not a concern.
- **`wc -l` + `view -c` guard precedes the hash (Gate B B1.5)** — this is the right belt: a truncation that happened to leave a colliding md5 is independently caught by the count. Good.

**Verdict: no false-PASS path from the md5 mechanics themselves.**

### Area 2 — Empty / skip hazards (probe 2) — the main vacuous-pass surface

**CRITICAL (to claimed scope) — reference-directory iteration means Rust-only or absent-on-both artifacts are silently uncompared.**

Gate A `compare_dirs` (lines 124, 137, 143) iterates `"$ref"/*.bam`, `"$ref"/*_report.txt`, `"$ref"/*.fq.gz` and on empty glob does `[ -e "$rb" ] || continue` — **no failure, no comparison**. Gate B B3 `for pa in "$d/perl"/*_reads*.fq.gz` (line 176) does `[ -e "$pa" ] || continue` likewise. So:

- If the Perl side produced **no** `--ambiguous`/`--unmapped` aux for a cell (e.g. a flag silently dropped, or a future refactor changed Perl's behavior), the **AUX comparison is skipped with zero failures** — the gate passes vacuously for that artifact class. The `GATE_OXY.md` "B3 aux+ambig ✅" cell would be a green checkmark over **zero comparisons**.
- The asymmetry direction matters: because the loop is over the **reference (Perl)** dir, an artifact **Rust emits but Perl does not** is *never even looked at*. A Rust port that erroneously emitted an extra BAM, or a spurious unmapped file, would not be flagged. (The reverse — Perl has it, Rust missing — *is* caught via `partner()` MISSING / `[ -f "$d/rust/$b" ]`.)

This does not invalidate the *reported* PASS — `GATE_OXY.md` shows non-zero record counts for main BAM, ambig BAM, aux, and reports in every cell, so we know those comparisons did run with real data. But the harness has **no positive assertion that the expected artifact set is non-empty / complete**. The headline "`--unmapped`/`--ambiguous`/`--ambig_bam` identical" claim rests on those globs having matched — which the harness never asserts.

> **Recommendation (Critical):** add a per-cell **manifest assertion** before comparison: count expected artifacts on *both* sides, assert each class is non-empty and the Perl-set ⊇ Rust-set (and vice-versa, modulo the known `--ambig_bam` name divergence). At minimum, increment `FAILED` (or emit a loud `WARN-EMPTY`) when a glob loop body executes **zero** iterations for a class that should exist. As-is, a "PASS" is consistent with "compared nothing."

**HIGH — `ls … | head -1` on empty glob feeds an empty/garbage path downstream (Gate B).**

Lines 120–122:
```bash
PB=$(ls "$d/perl"/*_bismark_bt2*.bam | grep -v '\.ambig\.bam$' | head -1)
RB=$(ls "$d/rust"/*_bismark_bt2*.bam | grep -v '\.ambig\.bam$' | head -1)
rep=$(basename "$(ls "$d/perl"/*_report.txt | head -1)")
```
With `set -uo pipefail` (no `errexit`), if a glob matches nothing `ls` prints an error to stderr (swallowed into the log) and `PB`/`RB`/`rep` become **empty**. Downstream:
- `samtools view -c "$PB"` with empty `$PB` → samtools reads **stdin** or errors; `cP`/`cR` become empty; `[ "$cP" = "$cR" ]` is `[ "" = "" ]` → **TRUE** → "counts reconcile ()" prints and **passes**.
- `rep=$(basename "")` → `.`; then `grep ... "$d/perl/."` errors but `cmp_files` on two error-empty files can compare equal.

So a **missing main BAM (path typo, a cell that produced no BAM, a recycle that truncated output) can cascade into a vacuous PASS** rather than a hard failure. The exit-code guard at line 117 *does* catch a non-zero `bismark` exit, which is the most likely cause of a missing BAM — so in practice this is mitigated upstream. But it is not defensively closed: a zero-exit run that nonetheless wrote the BAM to an unexpected name (or a stale `$d` from an interrupted prior run) slips through.

> **Recommendation (High):** after the three `ls … | head -1` lines, assert `[ -f "$PB" ] && [ -f "$RB" ] && [ -n "$rep" ]` (and `[ -f "$d/perl/$rep" ]`), else `FAILED=1; return`. Prefer a nullglob-safe array pick (`shopt -s nullglob; arr=("$d/perl"/*_bismark_bt2*.bam)`) over `ls | head` to avoid the empty-string-on-no-match trap entirely. The same applies to the ambig globs at line 185 — those *are* guarded (`if [ -n "$pAmb" ] && [ -n "$rAmb" ]`), but note the **else branch only prints a note, never fails** (see Area 3).

**MEDIUM — `set -e` is intentionally absent, so the FAILED-accumulator is the *only* gate.** This is a deliberate and reasonable choice (you want all cells to run and accumulate `FAILED`), but it means **every** comparison must route through something that sets `FAILED=1` on the negative path. The two findings above are exactly the spots where a negative outcome (empty glob) routes to `continue`/silent-pass instead of `FAILED=1`. The pattern is correct where used (`cmp_files`/`md5_eq`/`partner` MISSING all set `FAILED=1`); the gaps are only the empty-glob entry points.

### Area 3 — The V13 cross-check gating + NOTE-vs-FAIL (probe 3)

Gate B lines 200–206:
```bash
if [ "$old_bam" != "-" ] && [ -f "$old_bam" ] && [ -z "$SUBSET_N" ]; then
  mOld=$(samtools view "$old_bam" | LC_ALL=C sort $SORTOPT | md5sum | cut -d' ' -f1)
  [ "$mOld" = "$mP" ] && echo "ok ... layout-invariant" \
    || echo "NOTE old(--p4)=$mOld fresh(--p$P)=$mP (investigate ...)"
fi
```

- **Gating is correct:** runs only when an old BAM is configured (`!= "-"`), the file exists, **and** not in subset mode (`-z "$SUBSET_N"`) — so a smoke-subset run doesn't compare a subset against the full old BAM. RRBS passes `"-"` (line 215) and pbat passes `"-"` (line 219), correctly skipping V13 where no oracle exists. ✔
- **NOTE-not-FAIL is the *right* call** and the PLAN agrees (§3.1, §7-A2, V13: "**NOT an independent-correctness signal**"). The old BAM's Bowtie 2 version is *unknown*; a mismatch there could legitimately be a Bowtie-version artifact, not a Rust bug. Making it a hard fail would let an irrelevant provenance difference sink a gate whose real authority is B2 (fresh Perl vs Rust, same env). I concur it should be a NOTE. The verdict doc correctly frames V13 as "corroboration + provenance retirement", and the SE/PE cells reported `✅ same md5` — a *bonus* confirmation, not load-bearing.
- **Minor (Low):** `mP` is compared, but `mR` (Rust) is **not** compared against `mOld`. Since B2 already proved `mP == mR`, transitively `mOld == mP ⟹ mOld == mR`. Fine — but if B2 had *failed*, V13 still only references `mP`. Harmless given ordering.

**Verdict: V13 is correctly scoped and correctly soft.**

### Area 4 — `partner()` / ambig suffix-match + temp-file re-glob (probe 4)

**`partner()` (Gate A, lines 111–117) is correct.** Exact-name match first; the `*.ambig.bam` suffix fallback handles the one documented Perl name divergence (single-core `<base>_bismark_bt2.ambig.bam` vs multicore `<base>.fq_..ambig.bam`). The fallback uses an **array** (`arr=("$od"/*.ambig.bam); [ -e "${arr[0]}" ]`) — nullglob-safe, no `ls | head` trap here. Good. The "unique `*.ambig.bam`" assumption holds because each cell produces exactly one ambig BAM.

**Temp-file re-glob: I checked this exhaustively and it is CLEAN.** The normalized temp files written *next to* the artifacts (`$ref/$b.cmp.$tag`, `.srt.$tag`, `.hdr.$tag`, `.f.$tag`, `.txt.$tag`, `.rec.$tag`, and the `$op.*` siblings) all **append** a suffix after the original extension, e.g. `foo_bismark_bt2.bam` → `foo_bismark_bt2.bam.cmp.strict`. The driving globs are `*.bam`, `*_report.txt`, `*.fq.gz`, `*.ambig.bam` — all require the *exact* extension at end-of-string. A name ending `.cmp.strict` / `.srt.assume` / `.hdr.worker` matches **none** of them. Verified by enumerating every written suffix (lines 128–151). The three legs (strict/worker/assume) use distinct `$tag` values so within-cell temp files never collide either. **No corruption path.** ✔

- **Low note:** the temp files accumulate in `$d/{perl_sc,rust_p1,...}` and are never cleaned within a cell, but `run_cell` does `rm -rf "$d"` at entry (line 160) so a *re-run* of the same cell starts clean. Idempotent. Fine.

### Area 5 — Perf measurement fairness (probe 5)

- **Matched `P`:** both sides timed at the **same `--parallel $P`** (Gate B lines 109/113). ✔ The 2P-instance topology is identical on both sides (directional: P chunks × 2 Bowtie 2 instances) — the perf number measures wrapper overhead in the ~26% non-Bowtie tail, which is exactly how `GATE_OXY.md` frames it. Honest.
- **Inputs staged to `/var/tmp` off the S3 mount:** `stage()` (lines 72–77) in full mode does `cp "$src" "$dst"` into `$BASE/in` (= `/var/tmp/aligner_p10_gateB/in`). ✔ — both sides then read the **same staged path** (the staged path is passed once into `run_cell` args), so neither side eats mount jitter and both read the identical local file. Good.
  - **Low:** the `[ -f "$dst" ] || cp` (line 75) means a **stale staged input from a prior interrupted run is silently reused** (no md5/size check). On an ephemeral pod that's wiped on recycle this is low-risk, but a partial `cp` that left a truncated `$dst` would be reused as-is. Recommend `cp` to a temp name + atomic `mv`, or assert `wc -l`/size post-stage.
- **`/usr/bin/time -v` parsing:** Gate B uses `-o "$d/perl.time"` then `grep -E 'Elapsed|Maximum resident'` (lines 196–197). Correct field names for GNU `time -v` ("Elapsed (wall clock) time", "Maximum resident set size"). ✔ Gate A uses `/usr/bin/time -v` but **redirects into the run log** (`> "$d/perl_sc.log" 2>&1`) and never parses it — Gate A perf is transcribed manually into `GATE_OXY.md`, which is fine (Gate A perf is a bonus).
- **maxRSS per-launcher disclosure:** `GATE_OXY.md` line 74 explicitly states "(maxRSS is per-launcher-process, not aggregate.)" ✔ — the caveat *is* disclosed. Good. (`/usr/bin/time -v` measures only the direct child `bismark`/`bismark_rs` launcher, not the forked Bowtie 2 children — so the 3.4 GB figure is the *launcher's* RSS, not the cohort's. Disclosed.)

**Verdict: perf is fair and honestly framed.** One Low (stale-stage reuse).

### Area 6 — Resource correctness of the sort (probe 6)

- **`-S 16G` absolute (not `-S 25%`):** confirmed line 63. The header comment (lines 59–62) correctly explains the cgroup-vs-node-RAM trap (`/proc/meminfo` shows ~991 G node RAM, cgroup caps ~256 G, so `25%` would target ~248 G and risk OOM on the 40 GB PE sort). 16 G is safely under the cap; external merge-sort spills to `-T`. ✔
- **`--parallel=$P` on the sort with P=8:** line 63. Fine — sort thread count, independent of buffer.
- **Sorts run sequentially:** Within a cell, B2's two sorts (`mP=$(... sort ...)` line 151 then `mR=$(... sort ...)` line 152) are **separate command substitutions executed in sequence** — the first completes (and frees its 16 G) before the second starts. So **two 16 G buffers never coexist**. ✔ Same for B3 aux (180→181) and ambig (188→189) and V13 (203). The only place multiple sorts *could* stack is the `rname_md5vec` per-RNAME loop (line 93) — but that's the **on-FAIL diagnostic only** (runs inside the B2-mismatch branch), each shard sort is sequential in the `for` loop, and shards are tiny. No stacking. ✔
- **`-T "$BASE/sorttmp"`:** points at `/var/tmp/...` (678 G free) — ample for the ~42 GB decompressed PE spill. ✔

> **Note (Gate A discrepancy, Medium):** Gate A's `n_sam_sorted`/`n_fq_sorted` (lines 98, 102) use **`sort -S 25%`** — *not* the `-S 16G` absolute that Gate B was so careful about. Gate A runs on the **10M subset** (bodies ~2–4 GB), so `25%` of a 256 GB cgroup (~64 GB target, never reached because the input is small) is harmless *at 10M*. **But** Gate A is **parameterized to run full-size** too (the `SUBSET_N` empty path, used for the "RRBS strict-full candidate" the PLAN §3.2 floats). If anyone ever runs Gate A on a full 84M dataset (the script explicitly supports `SUBSET_N=""`), the `-S 25%` content sorts in the A-assumption leg would target ~64 GB and could stack with nothing (sequential) but still risk the same OOM trap Gate B was hardened against. The hardening was applied to Gate B only. Recommend mirroring `-S 16G` into Gate A's two `n_*_sorted` normalizers for consistency, since the comment in Gate B argues it matters whenever the sort can get large.

### Area 7 — The pbat cell (probe 7)

- **`--pbat -1 pe_2 -2 pe_1`** (Gate A line 203, Gate B line 219): the R1/R2 swap **plus** `--pbat` is the documented Felix trick — directional data run plain `--pbat` lands ~0 reads; the swap re-frames the directional library so it aligns as genuine CTOT/CTOB. This **is** a different test than directional (complementary-strand search), and `GATE_OXY.md` confirms it: pbat produced **143,434,062** records vs directional pe's **143,434,086** (24 fewer) — proof the pbat path makes genuinely different per-read decisions, and Rust matches Perl on that distinct count. ✔
- **Same swapped input to both sides:** both Perl and Rust get the identical `--pbat -1 pe_2 -2 pe_1` argv (run_cell passes `${ARGS[@]}` to both). ✔ — a fair comparison.
- **No V13 for pbat:** `old_bam="-"` (line 219) → V13 correctly skipped (no pre-existing pbat oracle). ✔
- **PE mate-factor in B1.5:** pbat passes `pe=1` (the 4th positional arg to `run_cell`, line 219) → `mult=2` → `implied=(ubh - disc)*2`. Correct for paired-end. ✔ `GATE_OXY.md` notes pbat was "the first full-scale run on the *fixed* harness" and reconciled cleanly — so the B1.5 formula fix *was* exercised live for pbat (unlike SE/PE; see Area 8 finding).

**Verdict: pbat is correctly constructed, correctly distinct, correctly compared.**

### Area 8 — B1.5 formula provenance (cross-cuts probe 3 / the verdict)

**HIGH (scope/provenance) — the corrected B1.5 formula was *not* run for SE/PE; it was hand-reconciled post-hoc.** `GATE_OXY.md` (lines 65, the B1.5 callout) is admirably transparent: the as-run SE/PE Gate-B used `implied = ubh × mult` (no discard subtraction), which false-flagged se_dir (off by 36) and pe_dir (off by 74). The doc says the **essential guard `cP == cR` passed** and B2 md5 matched, then the corrected formula was "re-verified against the finished BAMs" — i.e. arithmetic on paper, not a re-run. The fix is now in the script (lines 138–141), and pbat (run later) exercised it live.

This is **not a correctness hole** — the load-bearing assertions (`cP == cR`, B2 multiset md5, `wc -l`) all passed independently of the buggy `implied` line, and the discard-off-by is *exactly* the documented benign edge path. But it means:
- The reviewed `phase10_fullscale_content_gate.sh` is **not byte-for-byte the script that produced the SE/PE rows** in `GATE_OXY.md` (the SE/PE rows came from the pre-fix script).
- V7's "report-implied" sub-assertion was **never green in an actual SE/PE run** — it was reconciled by hand.

> **Recommendation (High → really a documentation/repro-tuple fix, not a code fix):** ensure the repro tuple / verdict explicitly states which commit of the harness produced which cell, so a re-runner doesn't assume the committed script reproduces the exact SE/PE log. Given the essential guard + B2 carried the verdict, I do **not** recommend re-running 84M SE/PE just to green the cosmetic `implied` line — but the provenance should be unambiguous. (If you want V7 fully green from a single script version, a cheap re-run of *only* B1.5 against the already-finished BAMs with the fixed formula would do it without re-aligning.)

---

## Could the gate pass while the port is wrong? — bottom line

**For the cells that actually ran with the reported non-zero record counts: no.** The combination of (a) in-order byte `cmp` vs Perl single-core at 10M (Gate A A-strict — the strongest signal), (b) worker-invariance at the Gate-B `P`, (c) the *measured* A1 assumption (Perl mc == sc multiset), (d) full-scale B2 sorted-multiset md5 with a preceding `view -c`/`wc -l` count guard, and (e) distinct-RNAME-set equality, closes essentially every "wrong-but-equal-counts" loophole. The md5 is canonical (`LC_ALL=C`), deterministic, and identical on both sides.

**The residual false-PASS surface is entirely in the *empty/missing-artifact* direction, not the *wrong-content* direction:**
- A whole artifact class absent on the reference side → silently uncompared (Area 2, Critical-to-scope).
- A missing main BAM via empty `ls | head -1` → `[ "" = "" ]` vacuous reconcile (Area 2, High) — though mitigated by the exit-code guard.

These don't undermine the *reported* PASS (the counts in `GATE_OXY.md` prove the comparisons ran on real data), but they mean the harness lacks a **positive completeness assertion**, so its green checkmarks are "no diff found" not "the expected comparison was performed." For a final epic-closing gate, that distinction is worth hardening.

---

## Recommendations (prioritized)

**Critical**
1. **Add a per-cell artifact-manifest assertion** (both harnesses): before comparing, count expected artifacts on *both* sides and `FAILED=1` (or loud `WARN-EMPTY`) if any class (main BAM, ambig BAM, report, unmapped/ambiguous aux) yields **zero** comparisons or if the Rust set isn't a superset-modulo-known-name-divergence of the Perl set. Closes the "compared nothing → PASS" and "Rust-only extra artifact never seen" hazards (Area 2).

**High**
2. **Guard the `ls … | head -1` picks** (Gate B lines 120–122): assert `[ -f "$PB" ] && [ -f "$RB" ] && [ -n "$rep" ] && [ -f "$d/perl/$rep" ]` else `FAILED=1; return`. Better: `shopt -s nullglob` + array pick to eliminate the empty-string-on-no-match path. Without this, a missing BAM from a zero-exit-but-misnamed run yields `[ "" = "" ]` → vacuous reconcile (Area 2).
3. **Make the empty-glob loop bodies fail-loud** for classes that must exist (Gate A lines 124/137/143 `… || continue`; Gate B line 176 `… || continue`; line 192 ambig `else echo note`): for the cells under test all of these artifacts are *expected*, so an empty glob is a defect, not a no-op.
4. **Clarify B1.5 provenance** in the verdict/repro tuple: the committed harness ≠ the script that produced the SE/PE rows (formula was fixed after). Optionally re-run *only* B1.5 against the finished BAMs to green V7 from a single script version (no re-alignment needed) (Area 8).

**Medium**
5. **Mirror `-S 16G` into Gate A's `n_sam_sorted`/`n_fq_sorted`** (lines 98/102), which still use `-S 25%`. Harmless at 10M, but Gate A is explicitly parameterized to run full-size (the RRBS-strict-full candidate), where `-S 25%` reintroduces the exact OOM trap Gate B was hardened against (Area 6).

**Low**
6. **Atomic / verified staging** (Gate B `stage()` line 75): `[ -f "$dst" ] || cp` silently reuses a possibly-truncated stale staged input on a re-run; `cp` to temp + `mv`, or post-stage `wc -l`/size assertion (Area 5).
7. **V13 references only `mP`, not `mR`** (line 204): transitively fine given B2 passed first, but a one-line `&& [ "$mOld" = "$mR" ]` would make it self-contained if B2's ordering ever changes (Area 3).
8. **Disclose the Gate-A-perf-not-parsed** asymmetry (Gate A buries `/usr/bin/time -v` in the run log and transcribes by hand; Gate B parses `-o *.time`) — cosmetic, already effectively handled in `GATE_OXY.md`.

---

## Final message to caller

I reviewed both Phase-10 gate harnesses against the "could a wrong port pass?" lens. **The gate design is sound and the reported PASS is trustworthy** for the cells that ran with the non-zero record counts shown in `GATE_OXY.md`: the `LC_ALL=C` sorted-multiset md5 is canonical/deterministic/symmetric, the in-order `cmp` vs Perl single-core at 10M is the real authority, worker-invariance runs at the Gate-B `P`, the A1 assumption is *measured* not assumed, and `view -c`/`wc -l` guards precede every hash. No wrong-content false-PASS path survives.

The residual risk is entirely in the **empty/missing-artifact** direction: (1) **Critical-to-scope** — every comparison loop iterates the *reference* dir and `continue`s on an empty glob with no failure, so an absent artifact class (or a Rust-only extra) is silently uncompared; the green checkmarks mean "no diff found," not "the comparison was performed." (2) **High** — Gate B's `PB/RB/rep=$(ls … | head -1)` yields an empty string on no-match, and `[ "" = "" ]` then *passes* count reconciliation vacuously (mitigated only by the upstream exit-code guard). I recommend a per-cell artifact-manifest assertion + guarding the `ls|head` picks. (3) **High (provenance)** — the committed B1.5 formula was fixed *after* the SE/PE run; those rows were hand-reconciled, so V7's report-implied leg was never green live for SE/PE (the essential `cP==cR` guard and B2 carried the verdict). Medium: Gate A still uses `-S 25%` (not the hardened `-S 16G`) and is parameterized to run full-size. Temp-file re-glob, V13 gating/soft-fail, pbat construction, and perf fairness all checked **clean**.

Report: `/Users/fkrueger/Github/Bismark-aligner/plans/05312026_bismark-aligner/phase10-realdata-gate-oxy/CODE_REVIEW_B.md`

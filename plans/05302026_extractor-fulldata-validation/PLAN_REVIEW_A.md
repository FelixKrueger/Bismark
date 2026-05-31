# Plan Review A — Full-dataset benchmark + byte-identity validation + resource-footprint docs (bismark-extractor)

**Reviewer:** A (independent, fresh context)
**Plan:** `plans/05302026_extractor-fulldata-validation/PLAN.md`
**Code under test:** `rust/bismark-extractor` @ `rust/iron-chancellor a7aaf61` (#884 R3)
**Date:** 2026-05-30

**Verdict:** Solid, well-structured campaign plan. The byte-identity methodology is sound; the
thread-footprint model is **verified accurate against source**; the phasing (correctness gate
before the multi-hour Perl perf runs) is correct. There are a handful of real issues — most
importantly an **internal inconsistency in the dry-run validation baseline numbers** and a
**dedup-parity gap that the plan flags but does not bind into a hard gate** — that should be
fixed before implementation. None are show-stoppers.

---

## Source verification (claims checked, not taken on faith)

I read the actual source. Every threading claim in the plan checks out:

| Plan claim | Source | Verified |
|---|---|---|
| `DECODE_THREADS = 2`, fixed, BAM-only, always-on | `parallel.rs:114`, gated by `is_bam` at `:224,246` | ✅ exact |
| Worker floor `config.parallel.max(2)` for BAM, `max(1)` SAM/CRAM | `parallel.rs:233` `config.parallel.max(if is_bam { 2 } else { 1 })` | ✅ exact |
| `GZIP_COMPRESS_THREADS = 4`, per open gzip file, eager | `output.rs:406` + `open_writer:428-444` (`from_writer` per file) | ✅ exact |
| `(4+1) × N_files` real footprint | `output.rs:398-405` doc confirms "1 writer + N compressor per file, eager at from_writer" | ✅ exact |
| Default mode opens **exactly 12** split files | `output_mode.rs:97-110` (3 contexts × 4 strands), `OutputKey::Default` doc `:44` | ✅ exact (not "≈12") |
| Per-file model: `(5)×12 = 60` gzip threads | follows from the two above | ✅ |
| Output worker-count-invariant (deterministic `batch_seq` reorder) | `parallel.rs:31-49` invariant block; collector `BTreeMap` reorder `:1018-1052` | ✅ |
| Splitting report / M-bias contain **no** timestamp/path/cwd/version-drift | grep of crate: no `chrono`/`current_dir`/`getcwd`/`SystemTime` in any writer path; version hardcoded `v0.25.1` matching Perl `:32` | ✅ |

**The "≈12 / ≈60" hedging in the plan is unnecessarily soft.** Default mode opens *exactly* 12 files
unconditionally at eager-open (`OutputFileMap::new` inserts every key from `mode_keys`, including the
CTOT/CTOB strands that are later empty-swept). So the gzip-thread count is exactly **(4+1)×12 = 60**
present from process start, plus 2 decode + `max(parallel,2)` workers + producer + collector (main).
At `--parallel 1`: `60 + 2 + 2 + 1(producer) + 1(collector/main) = 66`. At `--parallel 16`: `80`.
The plan's "~65 / ~71" figures are slightly low because they appear to undercount the worker floor
and/or the producer thread. Minor, but since deliverable 3 *documents* this number, the plan should
state the exact model and let the benchmark confirm it (which it already says it will).

---

## Logic review

**Phase ordering is correct.** Prepare → (idle gate) → byte-identity → (idle gate) → perf → docs.
Putting byte-identity *before* the multi-hour Perl perf runs is the right call: a correctness break
stops the campaign before burning a night on Perl serial runs. ✓

**The Rust-vs-Rust `--parallel` identity sweep (step 5) is well-founded.** The `batch_seq`
reorder invariant (`parallel.rs:31-49`) genuinely makes output worker-count-invariant, so this
is a real, cheap, high-value check. The HARD GATE framing is correct. ✓

**Gaps / logic issues:**

1. **(Important) Dry-run baseline numbers are internally inconsistent with the committed bench.**
   Validation check #1 says the 10M dry-run should reproduce "~17.6s plain / ~12.3s mbias_only at
   the floor." Those numbers come from the **R3 `DECODE_THREADS` doc-comment** (`parallel.rs:108-113`,
   a separate oxy trial). But the plan's Context (line 32) also says to "reuse this session's
   `bench_results/` timing harness (10M)" — and that committed harness (`results.csv`, `FINDINGS.md`)
   was run at **`b2af4e5` = PRE-R3** and shows Rust plain at **~19–21 s across all core counts**,
   never 17.6 s, with **no `mbias_only` column at all**. So:
   - The 17.6/12.3 targets are R3 numbers (correct for `a7aaf61`), *not* what the committed
     `bench_results/` would lead a reader to expect.
   - If the dry-run reuses the committed harness verbatim it won't even *measure* `mbias_only`.
   - A naive reader could treat a 19–21 s plain dry-run result as a regression vs the 17.6 s target.

   **Fix:** state explicitly that the dry-run baseline is the **R3** doc-comment trial (17.6/12.3),
   that the committed `bench_results/` is PRE-R3 and is being *generalized* (not used as the numeric
   oracle), and widen check #1 to a tolerance band (e.g. "plain ≤ 20 s, mbias_only ≤ 14 s at the
   floor; flag if outside") rather than two point values. Point-value reproduction across a shared
   box at variable load (FINDINGS notes load 3–19 during runs) is not realistic.

2. **(Important) Thread-count expectations in the plan are pre-R3 in spirit.** The committed
   `cpu_usage.csv` shows **3 threads at `--parallel 1` plain** — that is the PRE-R3 shape
   (producer + 1 worker + collector). At `a7aaf61` the same run will show **~6 threads** plain
   (producer + collector/main + **2** workers via the floor + **2** decode). The plan's own model
   (line 70) is the R3 model and is correct, but the harness's "known numbers" sanity check must use
   R3 thread counts, or the dry-run's thread-count assertion will spuriously "fail." Make the
   harness *report* thread count, not assert a hard-coded pre-R3 value.

3. **(Important) Dedup-parity is flagged but not bound to a gate.** The plan repeatedly notes the
   risk (WGBS-PE is `.deduplicated.bam`; SE "should be" dedup'd; RRBS dedup status "open, minor")
   and validation check #4 says to "confirm before SE runs." But the *driver* spec (impl outline
   step 4) only says "verify all 3 BAMs exist + staged" — it does **not** list a dedup-state assertion
   as a blocking pre-flight. Dedup mismatch skews **both** byte-identity (different read multiset →
   the sorted-md5 comparison still PASSES if Perl and Rust read the *same* BAM, so byte-identity is
   actually safe) **and** throughput/counts interpretation across datasets. The real risk is not a
   false byte-identity FAIL — both tools read the same staged BAM so they agree — it is **drawing
   cross-dataset perf/footprint conclusions from non-comparable inputs** and **reporting SE methylation
   numbers that aren't dedup-comparable to PE**. Make "dedup state recorded for all 3 BAMs, and SE
   dedup-matched-to-PE-or-explicitly-annotated" a **blocking Phase-0 checklist item**, not a footnote.

4. **(Optional) The SE-incoming guard's correctness depends on detecting "merge complete," not just
   "file exists."** The plan says wait for the merged `temp.{1..4}` → final BAM. A naive
   `[[ -f $SE_BAM ]]` test can fire mid-write (bismark creates the final file then writes/sorts).
   The guard should verify the file is **complete** (e.g. `samtools quickcheck` passes, EOF block
   present) and **stable** (size unchanged across two samples), not merely present. The plan mentions
   `samtools view -c` for read counts in Phase 0; fold a `samtools quickcheck` into the SE-existence
   guard explicitly.

5. **(Optional) `mbias_only` is in the perf matrix but is structurally exempt from byte-identity.**
   Phase 1 compares M-bias.txt under default/gzip modes (where it's a side output). Phase 2 perf-runs
   `mbias_only` but Phase 1 does not byte-identity-validate the `mbias_only`-mode invocation
   specifically. Since `mbias_only` shares the same M-bias accumulation path, this is low risk, but a
   one-line note ("M-bias.txt byte-identity under `--mbias_only` is covered transitively by the
   default-mode M-bias.txt comparison; the accumulator path is identical") would close the gap
   explicitly.

---

## Assumptions

- **"Output is worker-count-invariant" — VALID.** Confirmed in source (`batch_seq` reorder +
  commutative/associative M-bias and SplittingReport merges). The Rust-vs-Rust sweep is a genuine
  guard, not theater. ✓
- **"Splitting report / M-bias are deterministic, raw-byte-comparable" — VALID.** No timestamps,
  paths, cwd, or version drift in the writers. Splitting report uses bare `file_name()`
  (`output.rs:636-639`), so even differing staged paths won't leak into the report. Version is a
  shared hardcoded constant. ✓ This is the load-bearing assumption behind strict-`cmp` and it holds.
- **"gzp bytes differ from Perl gzip; decompressed content matches" — VALID.** `output.rs:419-427`
  documents the `deflate_rust` backend produces different *compressed* bytes but identical
  *decompressed* content; the smoke's `zcat | sort | md5` handles this. ✓
- **"Perl `--multicore N` fork+modulo produces different per-context line order than Rust" —
  PLAUSIBLE and the comparison is robust to it.** `sort | md5sum` compares the full line multiset,
  so ordering differences are masked (intended) while any *real* content divergence (a miscalled
  base, a missing/extra line, a changed count) changes the multiset and breaks the md5. **A real
  divergence cannot hide behind the sort** — verified by reasoning through the comparison. ✓
- **"GZIP_COMPRESS_THREADS=4 × ~12 files holds at scale" — VALID, but say *exactly 12*** (see source
  verification). The benchmark-confirms-actuals stance is good. ✓
- **(Risky, under-stated) "Full-WGBS Perl `--multicore 1` ≈ 1–2 h."** The committed 10M PE bench
  shows Perl plain `--multicore 1` ≈ **650 s (~11 min)** for 10M. Full WGBS is 55.7M reads (5.57×),
  and CLAUDE.md states extraction scales **superlinearly** with read count. Linear scaling → ~60 min;
  superlinear → plausibly **2–3 h**. The plan's upper bound may be optimistic. Since this is *the*
  long pole and the whole "fits one night" claim hinges on it, the plan should (a) widen the estimate
  to "~1–3 h" and (b) make the Perl `--multicore 1` anchor **the first perf run scheduled after
  byte-identity**, with a hard wall-clock budget/timeout, so a 3 h serial run doesn't starve the rest
  of the matrix. The resumable-CSV design mitigates this but the scheduling order matters.
- **(Unstated) The idle-gate threshold "scaled to 128 logical CPUs."** No concrete number is given.
  On a sole-tenant box the c2c 10M run + a 4×bowtie2 SE alignment can push 1-min load well above any
  naive "load < N" threshold, yet those are exactly what we wait for — so the gate keys on *process
  presence* (good) AND load (redundant/fragile). A load threshold on a 128-thread box is nearly
  meaningless for gating; lean on the cmdline-presence check and treat load as advisory only.

---

## Efficiency analysis

- **Run budget is broadly sound but front-loaded on one number.** ~70 Rust runs at 2–4 min each ≈
  3–5 h of Rust. The Perl anchors are correctly minimized (serial reused from byte-identity; only
  {1,12}×1). The single biggest schedule risk is the full-WGBS Perl serial (see above). The plan's
  resumable-CSV-append + per-config skip is the right mitigation and means a short night still
  yields partial data. ✓
- **Replication is adequate-but-asymmetric.** WGBS-PE gets 3 reps; SE/RRBS get 2; Perl gets 1.
  Given the FINDINGS show Rust wall-clock variance is small (~19–21 s band) but the box is *shared*
  (load 3–19 observed), 2–3 reps with **median** reporting is reasonable. **Recommend reporting
  min (not just median)** alongside, since on a contended box the minimum is the closest estimate of
  true compute time (the metric users actually want to size against); median absorbs contention noise
  into the headline number.
- **The `(user+sys)/real` CPU-cores metric is sound for *attributing* compute but is contamination-
  sensitive.** It is process-scoped (`/usr/bin/time` measures the child's CPU, not the box's), so
  *sibling* load does NOT inflate it — good. But it *will* be depressed if the run is **I/O-stalled
  on a cold/S3 read** (real time inflates while CPU time doesn't), which is exactly the staging
  gotcha the plan already guards. As long as staging-to-local + warm-up is enforced (it is), the
  metric is valid. ✓ Worth stating explicitly that `(user+sys)/real` is immune to sibling load but
  sensitive to I/O stalls, hence the staging requirement is load-bearing for *this metric*, not just
  for wall-clock.
- **0.2 s thread/RSS sampling for a ~2–4 min run is adequate for steady-state peaks but can miss a
  startup transient.** gzp spawns its pools **eagerly at writer-open** (`output.rs:398`), so the
  60 gzip threads exist within the first few ms — a 0.2 s sampler will catch the plateau (it persists
  the whole run), so peak-thread capture is fine. **Peak RSS** is the riskier one: if RSS peaks
  briefly (e.g. a large batch flush, `BATCH_SIZE=4096` payloads fanning to 12 files), 0.2 s could
  undersample. **Recommendation:** rely on `/usr/bin/time -v` "Maximum resident set size" as the
  authoritative peak-RSS (the kernel tracks the true high-water mark, sampling-free) and use the
  0.2 s `/proc` sampler only for the **thread-count** peak (which is a stable plateau, not a spike).
  The plan lists both sources for RSS (`time -v` Max RSS OR VmHWM) — make `time -v` Max RSS the
  primary and drop reliance on the sampler for RSS. As written, the plan *can* get the numbers; this
  just hardens it against an RSS undersample.
- **Disk headroom for 12 large `.gz` per run is flagged but unquantified.** Full WGBS split files are
  large; 12 of them × concurrent reps is real scratch pressure. Add a concrete pre-flight
  `df`-based free-space assertion (e.g. require N×expected-output-size before a config) to the driver,
  not just "ensure headroom; clean between."

---

## Validation sufficiency

**The byte-identity methodology catches the highest-risk failure modes and does NOT produce false
FAILs.** Concretely:

- **No false FAIL from timestamps/paths/versions:** verified absent from all writers; splitting
  report uses bare basename; version is a shared constant. Both tools read the **same staged BAM**,
  so even input-path text can't diverge. ✓
- **No false FAIL from gzip byte differences:** handled by `zcat | sort | md5`. ✓
- **No false FAIL from per-context line ordering:** handled by `sort`, intended. ✓
- **Real divergences are caught, not masked:** a changed methylation call, a dropped/extra line, or a
  miscount changes the sorted multiset → md5 breaks → FAIL. The splitting report and M-bias.txt are
  strict-`cmp`, so any count or percent divergence is caught raw. ✓

**Where validation is *thin*:**

1. **Could a real count divergence hide in the report comparison? No — but verify the comparison is
   actually strict.** The smoke routes `*_splitting_report.txt` and `*.M-bias.txt` to strict `cmp`
   (`phase_h_smoke.sh:262`). That's correct. The plan inherits this. ✓ No gap, but the plan should
   restate that the report is strict-`cmp` (not sorted), so the reader knows counts are
   raw-validated.
2. **M-bias `%.2f` / splitting-report `%.1f` float rounding at full scale.** This is the single most
   likely *legitimate* raw-byte divergence at production scale (large counts → different
   half-way-rounding edge cases between Perl `sprintf` and Rust `{:.2}`). I checked: both compute the
   *same* expression (`meth*100/(meth+un)`, Perl `:742`; Rust `mbias_writer.rs:217`) and both use
   round-half-to-even, so they *should* agree — and they did at 10M. But full-scale counts hit far
   more values, so this is the divergence most likely to surface for the *first* time at scale. **This
   is a strength of the plan, not a weakness:** strict-`cmp` will catch it as a real (correctly-failing)
   divergence. The plan should *anticipate* it: add a note that a M-bias.txt / splitting-report
   strict-`cmp` FAIL should first be triaged as a possible float-format rounding edge case
   (compare with a numeric-tolerance diff before declaring a logic bug), so a 1-ULP `%.2f` rounding
   difference isn't misclassified as a methylation-calling regression. Without this, a real-but-cosmetic
   rounding FAIL could halt the whole campaign (it's a HARD GATE) over a formatting artifact.
3. **File-name-set match is validated (the smoke does a name-set diff), including the empty-sweep
   contract** (CTOT/CTOB deleted for directional libraries). Good — the plan inherits this. But the
   plan should assert the **expected kept-file count per dataset/library** explicitly (12 for PE,
   6 for directional SE post-sweep) so a silent over/under-sweep at scale is caught, not just a
   name-set *diff* (which only catches it if Perl and Rust disagree — if *both* wrongly sweep, the
   diff passes). The existing matrix drivers already encode 6-vs-12 expectations; carry that forward.
4. **The "CPU-cores ≪ thread count" check (validation #6) is a documentation-claim validator, not a
   correctness validator.** Fine as-is, but its threshold ("if cores≈threads, re-examine") is vague.
   Pin it: gzip-mode cores should be ≤ ~5 (FINDINGS measured 4.6 at parallel-24 gzip) against
   ~60–80 threads; plain/mbias ~2.8 cores. State the numeric expectation.

---

## Alternatives

1. **(Worth considering) Anchor Perl perf to `--multicore 12` first, serial second.** The plan reuses
   the byte-identity Perl `--multicore 1` run as the serial anchor (good, avoids a duplicate multi-hour
   run). But the byte-identity Phase runs Perl at `--multicore 1`? The plan (step 4) says Phase 1 Perl
   runs are timed and double as the serial anchor — confirm Phase 1 Perl uses `--multicore 1` (the
   smoke defaults to `--multicore 4`/the passed `--parallel`). **If Phase 1 byte-identity runs Perl at
   the sweep's `--parallel` value rather than 1, the "reuse as serial anchor" claim breaks.** Pin
   Phase 1 Perl to `--multicore 1` explicitly, or accept a separate serial run. This is a concrete
   logic dependency that the plan glosses.
2. **(Worth considering) Drop full-WGBS Perl serial entirely and extrapolate from 10M.** The 10M Perl
   serial is already measured (~650 s). If the night is tight, a *measured-at-10M + documented
   superlinear-scaling-factor* estimate for full-WGBS Perl serial may be more valuable than spending
   2–3 h of a shared box reproducing a number whose only purpose is the headline speedup ratio
   (which is already ~31× at 10M serial and will only grow). The plan's "open, non-critical" tunable
   already gestures at this; promote it: **Perl serial on full WGBS is optional/extrapolatable**, Perl
   `--multicore 12` (the realistic baseline) is the one to actually run.
3. **(Minor) Use `hyperfine` for the Rust wall-clock matrix.** It handles warmup, replication, min/
   median/stddev, and JSON export in one tool, reducing harness surface and giving better statistics
   than a hand-rolled `date +%s.%N` × reps loop. The `/usr/bin/time -v` resource capture would still
   need a separate wrapper run, so this is a convenience, not a necessity — and the existing harness
   is proven. Optional.
4. **(Minor) Capture `/proc/<pid>/status` VmHWM at process exit** (just before reap) as a
   sampling-free RSS high-water alongside `time -v`, as a cross-check. Cheap belt-and-suspenders.

---

## Action items

### Critical
- *(none — no correctness-of-campaign blocker; the methodology is sound and source-verified.)*

### Important
1. **Fix the dry-run baseline inconsistency.** State that 17.6 s/12.3 s are the **R3** (`a7aaf61`)
   doc-comment targets, that the committed `bench_results/` is **PRE-R3** (`b2af4e5`, ~19–21 s, no
   mbias_only column) and is being *generalized* not used as the numeric oracle, and convert
   validation check #1 from two point-values to a **tolerance band**. (Logic #1)
2. **Make thread-count sanity checks use the R3 model and *report* rather than hard-assert.** Pre-R3
   measured 3 threads at `--parallel 1`; R3 will show ~6 (2 decode + 2 worker floor + producer +
   collector). Don't let a stale pre-R3 thread-count assertion spuriously fail the dry-run. (Logic #2)
3. **Bind dedup-parity into a blocking Phase-0 checklist**, not a footnote: record dedup state for
   all 3 BAMs; require SE dedup-matched-to-PE or an explicit non-parity annotation in the report;
   resolve RRBS dedup status before its perf runs. The risk is non-comparable cross-dataset
   perf/count conclusions, not a byte-identity FAIL. (Logic #3)
4. **Pin Phase 1 Perl to `--multicore 1`** if it is to double as the serial perf anchor — otherwise
   the "reuse, no duplicate Perl run" efficiency claim breaks. (Alternatives #1)
5. **Anticipate float-format rounding in the byte-identity HARD GATE.** A M-bias.txt / splitting-report
   strict-`cmp` FAIL at full scale should be triaged as a possible `%.2f`/`%.1f` half-way-rounding
   artifact (numeric-tolerance diff) *before* halting the campaign as a methylation-calling
   regression. (Validation #2)

### Optional
6. State the thread model as **exactly 12 files → exactly 60 gzip threads** (not "≈"); update the
   plan's "~65/~71" totals to include the worker floor + producer (≈66 at p1, ≈80 at p16); let the
   benchmark confirm. (Source verification)
7. Harden the SE-existence guard with `samtools quickcheck` + size-stability, not bare `-f`. (Logic #4)
8. Make `/usr/bin/time -v` Max RSS the **authoritative** peak-RSS; use the 0.2 s sampler only for the
   (stable-plateau) thread-count peak. Add a `df` free-space pre-flight per config. (Efficiency)
9. Report **min** alongside median for Rust wall-clock (min ≈ true compute time on a contended box). (Efficiency)
10. Treat the idle-gate load threshold as advisory; rely on cmdline-process-presence as the real gate
    (load is near-meaningless on a 128-thread box). (Assumptions)
11. Assert **expected kept-file counts** (12 PE / 6 directional SE) per dataset, not just a Perl-vs-Rust
    name-set diff (which misses a both-wrong sweep). (Validation #3)
12. Consider extrapolating full-WGBS Perl serial from the measured 10M (~650 s) × documented superlinear
    factor instead of running it, if the night is tight; run Perl `--multicore 12` as the real baseline.
    Widen the Perl-serial estimate to ~1–3 h. (Alternatives #2, Assumptions)
13. Add a one-line note that `--mbias_only`-mode M-bias.txt byte-identity is covered transitively by the
    default-mode comparison (identical accumulator path). (Logic #5)

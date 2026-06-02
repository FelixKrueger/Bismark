# Plan Review B — Full-dataset benchmark + byte-identity validation + resource-footprint docs

**Reviewer:** B (independent, fresh context)
**Plan:** `plans/05302026_extractor-fulldata-validation/PLAN.md`
**Code under test:** `rust/bismark-extractor` @ `rust/iron-chancellor a7aaf61` (verified: HEAD is `a7aaf61`)
**Verdict:** Strong, well-structured campaign plan. The phase ordering, idle-gate, resumability, and
edge-case handling are sound. However there is **one Critical false-FAIL risk** in the byte-identity
methodology (M-bias PNG file-name-set mismatch) that the plan does not mention, plus several Important
methodology gaps in the perf measurement and dedup-parity logic. None are show-stoppers; all are fixable
pre-implementation.

---

## Source verification (claims checked, not taken on faith)

| Plan claim | Source | Verdict |
|---|---|---|
| `DECODE_THREADS = 2`, fixed, BGZF decode, BAM-only | `parallel.rs:114` `NonZeroUsize::new(2)`; `:224` `is_bam` sniff; `:247` `ThreadedBamReader::from_path(input, DECODE_THREADS)` | **TRUE** |
| Worker floor `config.parallel.max(2)` for BAM, `max(1)` SAM/CRAM | `parallel.rs:233` `let n_workers = config.parallel.max(if is_bam { 2 } else { 1 })` | **TRUE** |
| `GZIP_COMPRESS_THREADS = 4`, per open gzip file, gzp `ParCompress` | `output.rs:406` const; `:434-438` `ParCompressBuilder...num_threads(4).from_writer` per `open_writer` call | **TRUE** |
| gzp pool spawned **eagerly** at writer-open (so present even at `--parallel 1`) | `output.rs:123-137` `new()` loops `mode_keys`, calls `open_writer` for every key up-front; doc comment `output.rs:396-405` confirms eager + `(GZIP_COMPRESS_THREADS+1)×open_files` | **TRUE** |
| Default mode opens **12** split files ⇒ ~60 gzip threads | `output_mode.rs:43-45` `OutputKey::Default(context, strand)` = 3 contexts × 4 strands = **12** keys, all eager-opened (`output.rs:120-137`). `(4+1)×12 = 60`. | **TRUE** |
| `--mbias_only` opens **0** split files (light) | `output.rs:104-106` "When `mode == MbiasOnly` returns an empty map" — no gzip pools | **TRUE** |
| Version string `v0.25.1` (matches Perl baseline) | `output.rs:31` `BISMARK_VERSION = "v0.25.1"`; `:36` `SPLIT_FILE_HEADER = "Bismark methylation extractor version v0.25.1\n"` | **TRUE — no version-string false FAIL** |
| Splitting report is deterministic (no timestamps) | `bismark_methylation_extractor:2476-2534` + `output.rs:630-739` — only counts/params/version, **no `localtime`/date** in the report. grep for `localtime|time()|Date:` in report writer = none. | **TRUE — deterministic, safe for strict `cmp`** |
| bedGraph/cov files would NOT appear (out of scope) | Perl `:109 if ($bedGraph)`, default OFF; only auto-set under `--cytosine_report` (`:1281-1284`). Campaign passes neither. | **TRUE — no extra bedGraph files** |
| Perl multicore leaves no stray intermediate files | Perl `:502`/`:577` `unlink` per-chunk splitting/M-bias; `:582 delete_unused_files` empty-sweep. | **TRUE — merged dir is clean** |

**The thread-footprint model in the plan is accurate against source.** The `~12 files → ~60 gzip
threads` figure is not a guess — it is `3 contexts × 4 strands` eager-opened, each with `4+1` threads.
The plan correctly notes the benchmark should *confirm* the live count rather than trust the static
figure; that is the right posture.

---

## Logic review

### L1 (CRITICAL) — M-bias PNG file-name-set mismatch is an unguarded false-FAIL
Perl's `bismark_methylation_extractor` writes **`*M-bias_R1.png`** (and `*M-bias_R2.png` for PE) via
`GD::Graph::lines` **whenever that Perl module is installed** (`:639-640`, `:677-716`). The Rust port
**does not** emit PNGs (`mbias_writer.rs:6` "the optional PNG plots ... are deferred"). The reused
comparison harness `phase_h_smoke.sh` builds a **file-name set diff** (`:233-242`) and any name mismatch
sets `NAME_DIFF`, which forces the overall verdict to **FAIL** (`:302-305`, `:310-314`) regardless of
whether every shared file is byte-identical.

Consequence: if oxy's Perl env has GD::Graph (very common — it is a standard Bismark dependency), **every
full-scale byte-identity run will FAIL on file-name-set mismatch** even when the extractor output is
perfect. Because Phase 1 is a **HARD GATE that STOPS the campaign** (plan Behavior §5, Validation §5),
this would abort the entire night for a non-issue. The 10M smoke may have passed only because that env
lacked GD::Graph, or because someone eyeballed past the name diff — at full scale, unattended, it stops.

The plan never mentions PNGs. **Fix required before implementation:** either (a) the harness must exclude
`*.png` from the name-set diff (compare only the shared text/data files, and assert the PNG-vs-no-PNG
difference is *expected*), or (b) pass `--mbias_off` to Perl for the byte-identity runs (but note that
also suppresses the M-bias.txt comparison — undesirable), or (c) document and verify at Phase 0 whether
oxy's Perl has GD::Graph and codify the expected file-name delta. Option (a) is cleanest. Phase 0 should
explicitly enumerate the expected file-name set per dataset/mode/library and treat *known* deltas
(PNGs) as PASS while still catching *unknown* deltas (a missing split file = real bug).

### L2 (IMPORTANT) — Phase 0 dry-run uses a harness the plan still calls "generalize phase_h_smoke.sh"
The plan's Phase 0 step 3 dry-runs "the harness" on 10M, but the harness (`byteid_run.sh`,
`bench_run.sh`, `overnight_driver.sh`) **does not exist yet** — only `phase_h_smoke.sh` does. The plan
is a benchmark campaign whose *only code deliverable is docs*, yet the harness scripts are net-new code
of non-trivial size (PID-scoped sampler, idle gate, resumable CSV append, SE-wait guard). The dry-run is
the right idea, but the plan should be explicit that **writing + dry-running the harness IS the bulk of
the implementation work** and is itself gated by the implement trigger. As written, a reader could
mistake "adapt existing" for "trivial." Recommend the dry-run also assert the **file-name-set logic**
from L1 on both a PE and an SE 10M run (SE has fewer split files post-empty-sweep — 6, not 12).

### L3 (IMPORTANT) — "Rust-vs-Rust identity across --parallel" is the wrong axis to stress for the gzip pool
Plan Behavior §5 sweeps Rust-vs-Rust identity over `--parallel ∈ {1,2,4,8,16}`. That correctly tests
**worker-count invariance** of the *content*. But the gzip thread pool (`GZIP_COMPRESS_THREADS`) and
decode pool (`DECODE_THREADS`) are **decoupled from `--parallel`** (verified above), so varying
`--parallel` does **not** vary the gzip/decode concurrency. The content-determinism risk that *could*
bite is gzp's parallel-compression block boundaries vs a single-member decode — but the plan already
relies on `zcat | sort | md5` (decompressed-content identity), which is correct (`output.rs:415-427`
confirms single-member `Gzip`, `GzDecoder`-readable). No change needed to the assertion, but the plan
should note the sweep proves *worker* invariance only; the gzip-pool determinism is covered by the
gz-content comparison, not by the `--parallel` axis. Minor clarity gap.

### L4 (IMPORTANT) — sorted-equivalence can mask a real divergence in *per-context data files*, but the report cannot
The plan's three-tier comparison is: raw `cmp` for splitting-report + M-bias; `sort | md5` for data
files; `zcat | sort | md5` for `.gz`. Question (a) from the brief asks whether a real divergence could
hide. Analysis:
- **Splitting report / M-bias.txt:** strict `cmp` (`phase_h_smoke.sh:262-270`). A real count divergence
  here is caught. **Safe.**
- **Per-context call files (sorted):** `sort | md5` will catch any added/dropped/changed line (a wrong
  call, a missing read, a duplicated read) because sorting is a bijection-preserving check on the
  multiset of lines. It will **NOT** catch a divergence that is purely **line-order** — but line order
  is explicitly *not* part of Bismark's output contract (Perl multicore fork+modulo order ≠ Rust
  BAM order), so order-insensitivity is correct here. **Safe** *provided* the files are truly order-free.
- **Hidden risk:** if both tools dropped the *same* read (e.g. both mishandle an edge BAM record), the
  comparison passes while both are wrong. That is a *Perl-parity* check, not an *absolute-correctness*
  check — acceptable for this campaign's stated goal ("byte-identity to Perl"), but the plan should state
  that the gate proves **parity with Perl v0.25.1**, not absolute correctness, so nobody over-claims.

### L5 (MINOR) — splitting report embeds the input basename; staging must preserve it
`output.rs:636-641` and Perl `:4995` both echo the **input file basename** into the splitting report.
The plan stages each S3 BAM "to local disk." If staging **renames** the BAM (or copies under a different
basename for Perl vs Rust), the splitting report basename line diverges → strict-`cmp` FAIL. The plan
must stage to a **single local path** and run *both* tools against *that same path* (as `phase_h_smoke.sh`
already does — it takes one `$BAM` and feeds it to both). Call this out in Phase 0 so the staging step
doesn't accidentally give Perl and Rust different input filenames.

---

## Assumptions

### A1 (IMPORTANT) — "GZIP_COMPRESS_THREADS=4 × ~12 files holds at scale — benchmark confirms"
Correct and verified. But the plan should note that the **CTOT/CTOB strands are still eager-opened even
when they contain zero records** (`output.rs:401` "including zero-record CTOT/CTOB strands later swept").
So a *directional* library (the common case) opens all 12 gzip writers — 60 threads — even though 4 of
those files end up empty and swept. The footprint doc must state this: the thread peak is driven by
*eager open of all mode keys*, **not** by how many strands actually receive data. This is exactly the
"threads ≫ cores" story, and it is real. Good.

### A2 (IMPORTANT) — "WGBS-SE should be dedup'd for parity" is correctly flagged but the *consequence* is understated
The plan flags dedup parity (Context, Phase 0, Validation §4). Good. But the brief's concern (c) is
sharper than the plan's phrasing: a non-dedup'd SE BAM doesn't just "skew counts" — it makes the SE
byte-identity run **meaningless as a Rust-vs-Perl check is unaffected** (both tools see the same input,
so byte-identity still holds), **but the perf and footprint numbers** are measured on a different
read-count workload than WGBS-PE, breaking the PE-vs-SE comparison the campaign implies. The real risk is
the *opposite* of what the plan says: byte-identity is fine on a non-dedup BAM (same input to both); it's
the **cross-dataset performance narrative** that breaks. Recommend: dedup the SE BAM for parity, OR
report SE perf with an explicit "non-dedup, ~N reads" caveat and do **not** put PE/SE throughput in the
same table without normalizing per-read.

### A3 (MINOR) — Perl `--multicore 1` ≈ 1–2 h is an assumption, not a measurement
The plan budgets the Perl serial WGBS run as the "multi-hour long pole" and keeps it to 1 rep. This is
prudent. But the actual time is unknown until Phase 0/1 measures it. If it is >2 h, the {1,12}×1 Perl
matrix across **three** datasets plus the byte-identity Perl runs may not fit one night. The plan's
resumability mitigates this, but the budget line ("Fits an overnight window") is optimistic. See E1.

### A4 (verified-OK) — "Output is worker-count-invariant" — TRUE by construction
`parallel.rs` reorders by `batch_seq` (`:136-140`, `:1063`), so output is deterministic across worker
counts. The Rust-vs-Rust assertion is sound.

---

## Efficiency analysis

### E1 (IMPORTANT) — the overnight budget arithmetic is fragile; Perl is unbounded
Plan Efficiency: "~70 Rust runs @ 2–4 min + Perl {1,12}×1." Rust side ≈ 2–4.5 h — fine. **Perl side is
the unbounded risk:** 3 datasets × {multicore 1, multicore 12}. The `--multicore 1` WGBS-PE run is the
1–2 h pole; if SE and RRBS serial Perl runs are also 1 rep each, that's **up to 3 serial Perl WGBS-class
runs** plus the byte-identity Perl runs (which the plan *reuses* as the serial anchor — good). Realistic
worst case: 3–6 h of Perl serial alone. The "(Open, non-critical) drop SE/RRBS Perl serial if the night
runs short" escape hatch is good, but it should be a **hard ordering rule in the driver**, not a manual
judgment call: run all Rust + all byte-identity + Perl-WGBS-{1,12} **first**, then SE/RRBS Perl serial
**last** (lowest value, first to be skipped on timeout). The resumable CSV makes this safe. Recommend the
driver encode this priority explicitly.

### E2 (IMPORTANT) — peak-thread sampling at 0.2 s may miss the true peak; but here it is adequate
Brief concern (e). The gzip thread pool is spawned **eagerly at writer-open** (`output.rs:123-137`),
i.e. within the first few ms of the run and held for the **whole run** (`:396-399` "for the whole run").
So the ~60-thread plateau is **not** a transient spike — it persists for minutes. A 0.2 s sampler will
catch it easily. **The 0.2 s rate is adequate for the gzip-mode thread peak.** The one genuinely
transient moment is *startup/teardown* (writers dropping → footer flush → threads joining), but those
are lower than the plateau, so 0.2 s captures the true max. **Verdict: sampling rate is fine** —
contrary to a naive worry. The plan could note *why* it's adequate (held-for-whole-run, not spiky) so a
reviewer doesn't reflexively demand a faster sampler.

### E3 (IMPORTANT) — `CPU-cores = (user+sys)/real` mixes the gzip pool, decode pool, AND I/O wait
The metric `(user+sys)/real` from `/usr/bin/time` measures **CPU utilization of the whole process tree**.
For the extractor this includes: decode threads (CPU-bound deflate), worker threads (CPU-bound
extraction), and gzip pool (CPU-bound compression). It does **NOT** count I/O wait (that inflates `real`
without `user+sys`). So:
- If the staged BAM read is **warm/local** (plan enforces this — good), `real` is not inflated by input
  I/O, and `(user+sys)/real ≈ true CPU cores`. **Valid.**
- **But output write to local scratch** (12 large `.gz`) can still cause `real` to exceed CPU time under
  write-back pressure, *deflating* the cores number. The plan should note the cores metric is a
  *lower bound* under heavy gzip output, and that the `--mbias_only` mode (no output files) gives the
  cleanest cores reading. The "≈3 cores" doc claim is most defensible from `--mbias_only` + plain, less
  so from gzip mode where I/O wait deflates it. Recommend reporting cores **per mode** and being explicit
  that gzip-mode cores are a floor.
- Attribution "extractor vs I/O": `/usr/bin/time` cannot separate them. If the docs want "the extractor
  uses ~3 cores," that statement is cleanest from the `--mbias_only`/plain runs and should be qualified
  for gzip mode. The plan's Validation §6 ("CPU-cores ≪ thread count, ≈3 vs ≈60") is the right check.

### E4 (MINOR) — replication is uneven and no variance reporting is specified
WGBS-PE gets 3 reps, SE/RRBS get 2, Perl gets 1. The plan says "median tables" in the driver output but
never specifies a **variance / spread** column (min-max, or CV). With 2 reps you cannot compute a
meaningful median (it's the mean of 2) or detect an outlier. Recommend: report min/median/max (or
both raw reps) so a contaminated run (e.g. a cold-read sneaking through, or oxy contention) is visible
rather than silently medianed-in. For the 1-rep Perl anchor, flag explicitly that it is unreplicated.

### E5 (MINOR) — disk headroom for gzip mode is hand-waved
"~12 large `.gz` per run — ensure local scratch headroom; clean between configs." For full WGBS, the
decompressed split files are tens of GB; even gzipped, 12 files could be several GB per run. With 70 Rust
runs, "clean between configs" is essential and should be a **driver invariant with a pre-run free-space
check**, not a note. A mid-night `ENOSPC` would crash a gzip writer — and recall `output.rs:421-423`
warns a **footer-flush I/O error surfaces as a panic on drop** (gzp `.unwrap()`s). So an out-of-disk
condition in gzip mode **panics** the Rust binary, which the harness must treat as a run failure (not a
silent skip). Add a free-space precondition and panic-detection to the harness.

---

## Validation sufficiency

| Risk | Covered? | Gap |
|---|---|---|
| S3 cold read contaminates timing | Yes (stage + `stat` + warm-up) | None — well handled |
| SE BAM not yet existing | Yes (wait+verify guard) | Sound; see V1 for the *dedup* half |
| Renamed-binary sampler deadlock | Yes (PID-scoped, not pgrep) | Good — matches the memory gotcha |
| Worker-count invariance | Yes (Rust-vs-Rust sweep) | Sound |
| Perl-parity of counts | Yes (strict cmp on report) | Sound; states *parity* not *correctness* (see L4) |
| **M-bias PNG file-set delta** | **NO** | **L1 — Critical false-FAIL** |
| CPU-cores ≪ threads | Yes (Validation §6) | Good, but qualify per-mode (E3) |
| Idle-gate fires | Yes (Validation §7) | Good |

### V1 (IMPORTANT) — the SE-wait guard checks existence but the dedup-parity check is manual
Validation §4 says "WGBS-SE BAM exists + dedup parity confirmed before SE runs." The *existence* check is
automatable (and is, via the wait guard). The *dedup parity* check is described as a manual Phase 0 step
("Prefer a deduplicated ... if Felix's SE run isn't deduplicated, either run `deduplicate_bismark` ... or
explicitly note"). In an **unattended overnight driver**, "explicitly note" cannot happen — there is no
human at 3 a.m. The driver must make a **deterministic decision**: either (a) auto-run
`deduplicate_bismark` if the SE BAM is not `.deduplicated`, or (b) refuse SE runs and log a clear skip.
Pick one and encode it; do not leave a human-in-the-loop step inside the unattended path. (Same for the
RRBS dedup-status open question — codify, don't defer.)

### V2 (MINOR) — no assertion that the read counts of all three BAMs are non-trivially large
Validation §2 records `samtools view -c`. Good, but it only "records" — it should **assert** the count
exceeds a floor (e.g. > 100M for WGBS) so a truncated/partial S3 stage (interrupted symlink pull) is
caught before timing. A half-staged BAM would otherwise run fast and pollute the "performance" numbers.

### V3 (MINOR) — Phase 1 byte-identity is run in `--multicore 1` for Perl; but the doc/perf story uses `--multicore 12`
The plan reuses the Phase-1 Perl `--multicore 1` run as the serial anchor (good). But it does **not**
byte-identity-check Perl `--multicore 12` output. That is fine *if* Perl's multicore output is
content-identical to its serial output — which it is by design (fork+modulo, merged). Worth a one-line
assertion in Phase 0's 10M dry-run: confirm Perl `--multicore 1` and `--multicore 12` produce
sorted-equivalent output, so the perf anchor and the byte-identity anchor are the same workload.

---

## Alternatives

### Alt1 — Use `--mbias_off` for the Perl byte-identity runs to sidestep the PNG problem
Trade-off: removes the PNG file-set mismatch (L1) but also drops M-bias.txt from the comparison. Since
M-bias.txt is one of the two strict-`cmp` files (high-value), this is the *wrong* trade. Prefer the
harness-side PNG exclusion (L1 option a). Documented here so it is consciously rejected.

### Alt2 — Drive perf with `hyperfine` instead of a hand-rolled `/usr/bin/time` loop
`hyperfine` gives warm-up runs, statistical replication, outlier detection, and JSON export for free, and
would address E4 (variance) cleanly. The catch: it doesn't sample peak threads / peak RSS (the
footprint deliverable needs those). Hybrid: `hyperfine` for wall-time + variance, the PID sampler for
threads/RSS on one representative rep. Worth considering; not mandatory.

### Alt3 — Measure cores with `pidstat`/`/proc/<pid>/stat` deltas instead of `(user+sys)/real`
`pidstat -p <pid> 1` gives **instantaneous** %CPU over time, which distinguishes the steady-state core
usage from startup/teardown and is robust to I/O-wait deflation (E3). `(user+sys)/real` is a fine
*aggregate* but a time-series from `pidstat` would let the doc say "steady-state 3.0 cores, peak 3.4"
with confidence. Optional enhancement.

### Alt4 — Capture the live open-file count via `/proc/<pid>/fd` to *prove* the 12-file claim
The plan says "the benchmark confirms the actual open-file/thread count." A direct way:
`ls /proc/<pid>/fd | wc -l` sampled alongside the thread sampler. This *empirically* validates the
"12 gzip writers eager-open" claim against the source-derived figure, closing the loop the plan wants.
Cheap to add to the existing PID sampler. Recommended.

---

## Action items

### Critical
1. **(L1) Guard the M-bias PNG file-name-set mismatch.** Perl writes `*M-bias_R[12].png` when
   `GD::Graph::lines` is installed; Rust does not. The reused `phase_h_smoke.sh` treats any name-set
   delta as a hard FAIL (`:233-242`, `:302-314`), and Phase 1 is a campaign-stopping gate. Before any
   full run: enumerate the expected file-name set per dataset/mode/library, exclude `*.png` from the
   name-set diff, and assert PNG-vs-no-PNG as an *expected* delta while still catching *unexpected*
   deltas (a missing split file = real bug). Verify in Phase 0 whether oxy's Perl has GD::Graph and
   record the resulting expected delta. **This is the single most likely cause of a false-FAIL night.**

### Important
2. **(V1) Make the dedup-parity decision deterministic in the unattended driver.** "Explicitly note" is
   not executable at 3 a.m. Either auto-`deduplicate_bismark` the SE (and RRBS) BAM if not already
   dedup'd, or auto-skip-and-log. Encode one; remove the human-in-the-loop step from the overnight path.
3. **(A2/L4) Separate the parity claim from the cross-dataset perf claim.** Byte-identity on a non-dedup
   SE BAM still passes (same input to both tools); it is the **PE-vs-SE throughput comparison** that
   breaks on a read-count mismatch. Dedup for parity, or report SE perf per-read-normalized with an
   explicit caveat. Do not co-table raw PE and SE throughput without normalization.
4. **(E1) Encode Perl-anchor priority as a driver ordering rule.** Run all Rust + byte-identity +
   WGBS-PE Perl {1,12} first; SE/RRBS Perl serial last (first to drop on timeout). Don't leave it a
   manual judgment call.
5. **(E3) Report CPU-cores per mode and qualify gzip-mode cores as a lower bound.** Output write-back
   can inflate `real` and deflate `(user+sys)/real`. The clean "≈3 cores" figure comes from
   `--mbias_only`/plain; gzip-mode cores are a floor. State this in both the FINDINGS and the docs.
6. **(E5) Add a pre-run free-space check + panic detection.** gzip-mode `ENOSPC` panics the Rust binary
   (gzp footer-flush `.unwrap()`, `output.rs:421-423`). The driver must check scratch headroom before
   each gzip config and treat a panic as a run failure, not a silent skip.
7. **(L2) Scope the harness work honestly.** The "only deliverable is docs" framing undersells that
   `byteid_run.sh`/`bench_run.sh`/`oxy_idle_gate.sh`/`overnight_driver.sh` are net-new code requiring
   the implement trigger. The 10M dry-run should exercise the L1 file-set logic on **both PE and SE**
   (SE = 6 kept split files post-empty-sweep, not 12).

### Optional
8. **(Alt4 / E2) Sample `/proc/<pid>/fd | wc -l` alongside threads** to empirically prove the
   "12 eager-open gzip writers" claim the plan wants the benchmark to confirm. (Thread peak is a held
   plateau, so 0.2 s sampling is already adequate — note this rationale so reviewers don't demand a
   faster sampler.)
9. **(E4) Add variance reporting (min/median/max or both raw reps).** With only 2 reps for SE/RRBS, a
   single bare "median" can hide a contaminated run. Flag the 1-rep Perl anchor as unreplicated.
10. **(V2) Assert (not just record) a read-count floor** per BAM to catch a truncated S3 stage before
    timing.
11. **(V3) In the 10M dry-run, assert Perl `--multicore 1` ≡ `--multicore 12`** (sorted-equivalent) so
    the byte-identity anchor and the perf anchor are confirmed to be the same workload.
12. **(Alt2/Alt3) Consider `hyperfine` for wall-time + variance and `pidstat` for steady-state cores**,
    keeping the PID sampler for threads/RSS. Strengthens the footprint doc; not mandatory.

---

## Summary
The plan is technically sound and its load-bearing thread-footprint model is **verified accurate against
source** (`DECODE_THREADS=2`, `GZIP_COMPRESS_THREADS=4` per file, 12 eager-opened Default-mode files →
~60 gzip threads, all decoupled from `--parallel`). The phase ordering (byte-identity gate before the
multi-hour Perl perf runs), idle-gate, PID-scoped sampler, and resumable CSV are all good engineering.
The **one Critical gap** is an unguarded M-bias-PNG file-name-set mismatch that would false-FAIL the
campaign-stopping byte-identity gate if oxy's Perl has GD::Graph. The Important items harden the
unattended path (deterministic dedup decision, Perl-anchor ordering, ENOSPC/panic handling) and sharpen
the perf methodology (per-mode cores, variance, parity-vs-throughput separation). None block planning;
all are fixable before the implement trigger.

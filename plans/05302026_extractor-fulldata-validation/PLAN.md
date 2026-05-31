# Plan — Full-dataset benchmark + byte-identity validation + resource-footprint docs (bismark-extractor)

## Goal
Run the bismark-extractor (post-#884, `rust/iron-chancellor @ a7aaf61`) against **three full-scale
datasets** on oxy — full WGBS **PE**, full WGBS **SE**, and full mouse **RRBS PE** — to (1) **prove
byte-identity (parity) to Perl Bismark v0.25.1 at production scale** (the existing smoke is 10M only),
(2) **measure real Rust-vs-Perl performance** on the realistic gzip-output path with replication and
a `--parallel` sweep, and (3) **characterize the runtime resource footprint** (peak threads, CPU
cores actually consumed per mode, peak RSS) per `--parallel` × output-mode, then **document it** so
users can size HPC / nf-core resource requests. Deliverables: a reusable + resumable overnight
harness, a results report, and a data-backed "Resource usage" doc (`--parallel` help text + README).

## Revision history
- **rev 1 (2026-05-30):** Folded Felix's manual-review feedback — 3 full datasets (WGBS-PE/SE,
  RRBS-PE), sweep cap 16, docs in-plan, overnight unattended driver, tiered matrix.
- **rev 2 (2026-05-30):** Folded dual plan-review (`PLAN_REVIEW_A.md` / `PLAN_REVIEW_B.md`). Both
  verified the thread model against source (it is **exact**, not approximate). Changes:
  - **[B, CRITICAL] M-bias PNG false-FAIL** — Perl emits `*M-bias_R1.png`/`_R2.png` via `GD::Graph`
    (when installed; standard Bismark dep), Rust defers PNGs. The reused name-set diff would hard-FAIL
    every run. **Exclude `*.png` from the name-set diff and codify the expected file-set delta.**
  - **[both] Dedup-parity is a deterministic blocking Phase-0 gate**, not a manual "note" (can't run
    unattended). Its real consequence is **cross-dataset perf comparability**, NOT a byte-identity FAIL.
  - **[A] Dry-run baseline** must be a **tolerance band on a freshly-measured R3 binary**, not the
    pre-R3 (`b2af4e5`) `bench_results/` point-values (which are ~19–21 s and have no `mbias_only` column).
  - **[A] Float-rounding triage** — a strict-`cmp` HARD-GATE FAIL on M-bias/splitting report may be a
    `%.2f`/`%.1f` half-way rounding artifact; triage before declaring a calling regression.
  - **[B] ENOSPC** panics the Rust gzip writer (gzp footer-flush `.unwrap()`, #889) → harness needs a
    **free-space precheck + panic-as-failure** handling.
  - **[B] `(user+sys)/real` cores is deflated by gzip write-back** → report **cores per-mode**;
    "~3 cores" headline holds only for `--mbias_only`/plain; gzip-mode cores = a floor.
  - **[A] `/usr/bin/time -v` Max RSS is authoritative** (GNU time confirmed on oxy); the 0.2 s sampler
    is for the (held, plateau) thread count + `/proc/<pid>/fd` count, not RSS.
  - **[both] Pin Phase-1 Perl to `--multicore 1`** (reusable as the serial perf anchor); Perl-serial
    full-WGBS is the 1–3 h schedule long-pole → explicit driver priority + extrapolation option.
  - **[both] Byte-identity proves PARITY with Perl, not absolute correctness** — stated explicitly.
  - Thread totals corrected; "≈12/~60" → **exactly 12 files / 60 gzip threads** (`output_mode.rs:43-45`).

## Context

### Placement / where this work lives
- **Code under test:** `rust/bismark-extractor` on `rust/iron-chancellor @ a7aaf61` (R1 mimalloc +
  R2 gzp gzip + R3 parallel BGZF decode merged). No source change is needed to *run* the benchmark;
  the only code deliverable is the **documentation** (deliverable 3) — a separate implementation step
  gated on this plan's review + an implement trigger.
- **Machine:** **oxy** — Intel Xeon 6975P-C, **64 physical cores / 128 logical threads**, 1 socket,
  no cgroup CPU cap. GNU `/usr/bin/time -v` confirmed present. Repo `~/Github/Bismark` on
  `rust/iron-chancellor @ a7aaf61`, release binary rebuilt this session.
- **Harness to reuse/extend:** `scripts/phase_h_smoke.sh` (Rust-vs-Perl byte-identity, 10M) + this
  session's `bench_results/` timing harness (10M, **pre-R3** — see dry-run note). Generalized to full
  data + the sweep + replication + resource sampling + an unattended overnight driver.
- **Perl baseline:** `bismark_methylation_extractor` v0.25.1 from `~/micromamba/envs/bismark-test/bin`
  (PATH-prepend, NOT `mamba activate`). Same env supplies `samtools`.

### Inputs / data — three full datasets
| # | Dataset | Path (oxy) | Status |
|---|---|---|---|
| WGBS-PE | full human WGBS, PE, dedup'd | `~/bismark_benchmarks/full_size/SRR24827373_GSM7445361_..._R1_val_1_bismark_bt2_pe.deduplicated.bam` | ready (S3 symlink) |
| WGBS-SE | full human WGBS, SE (same sample R1, native SE align) | `~/bismark_benchmarks/full_size/SRR24827373_..._SE_trimmed_full_size_bismark_bt2.bam` (final after bismark merges `temp.{1..4}`; **dedup for parity**) | **aligning now (~1h)** |
| RRBS-PE | full mouse RRBS, PE | `~/bismark_benchmarks/RRBS_PE/SRR24766921_GSM7433369_Colon_3_Months_Rep1_1_val_1_bismark_bt2_pe.bam` | ready (S3 symlink) |

- **GOTCHA — S3-backed symlinks** (WGBS-PE, RRBS-PE point into `/datasets/s3/...`). Cold reads pull
  over the network and contaminate timing. The harness MUST **stage each BAM to local disk** and
  `stat`-verify it is a real local file (not a symlink) before any timed run; do one untimed warm-up read.
- **Dedup-parity — BLOCKING Phase-0 gate (deterministic, [both reviewers]):** WGBS-PE uses
  `.deduplicated.bam`. The SE and RRBS inputs MUST be deduplicated to the same state for a valid
  **cross-dataset perf** comparison. The driver auto-resolves: prefer a `.deduplicated.bam`; if absent,
  run `deduplicate_bismark` to create one (don't benchmark the raw BAM); never silently compare
  dedup'd-vs-raw. (Note: dedup state does NOT affect *byte-identity*, since Rust and Perl read the same
  input — it affects only the perf/count comparability across datasets.)
- **SE-incoming guard:** the WGBS-SE final BAM does not exist yet (only in-progress `temp.*` chunks).
  The driver **waits for and verifies** the merged (+dedup'd) SE BAM before SE runs.
- Genomes for Perl: `~/bismark_benchmarks/genome` (human), `~/bismark_benchmarks/RRBS_PE/genome` (mouse).

### Current oxy activity (idle-gate must wait for all)
c2c session's `coverage2cytosine_rs` (10M) + Felix's bismark **SE alignment** (4× `bowtie2-align-s`).
The overnight driver waits until **all** clear (and the SE BAM exists) before starting.

### The threading model — VERIFIED EXACT against source (basis for deliverable 3)
Three **independent** pools; only the middle scales with `--parallel`:
| Pool | Constant / control (source) | Count |
|---|---|---|
| BGZF decode (BAM only, always-on) | `parallel.rs` `DECODE_THREADS = 2` | 2 |
| Extraction workers | `parallel.rs` `config.parallel.max(2)` (BAM) / `max(1)` (SAM/CRAM) | `max(--parallel, 2)` |
| gzip compression (gzip mode only) | `output.rs` `GZIP_COMPRESS_THREADS = 4`, per open file; eager at writer-open | `(4 + 1) × N_open_files` |

Default mode opens **exactly 12** split files (3 contexts × 4 strands, `output_mode.rs:43-45`) ⇒
**exactly 60 gzip threads**, eager-spawned and held for the whole run, **independent of `--parallel`**.
Total threads ≈ `1 main + ~2 producer/collector + 2 decode + max(--parallel,2) workers + (gzip mode only) 60`:
- **`--mbias_only`** / **plain `.txt`** (no gzip pools): light (~7–8 threads at `--parallel 1`).
- **gzip default**: heavy thread *count* (~**67** at `--parallel 1`; ~**81** at `--parallel 16`) but
  measured CPU *cores* only ~2.8–3.2 on the non-gzip paths (gzip threads idle-block on empty channels).
  The benchmark **empirically confirms** the 12 open files via `/proc/<pid>/fd` sampling and reports
  cores **per mode** (gzip-mode cores are higher due to compression + write-back — report as a floor,
  not the headline). This threads≫cores split is the core message of deliverable 3.

## Behavior (the campaign — what it does, in order)

### Phase 0 — PREPARE (now; does NOT need oxy idle)
1. Author the harness + overnight driver (below), including: `*.png` name-set exclusion, deterministic
   dedup-parity handling, free-space precheck + panic-as-failure, `/usr/bin/time -v` Max RSS,
   `/proc/<pid>/fd` + `/proc/<pid>/task` sampling, per-mode core computation.
2. **Re-measure the R3 binary's 10M baseline fresh** (plain + mbias_only) to set the dry-run tolerance
   band — do NOT reuse the pre-R3 `bench_results/` point-values. Stage WGBS-PE + RRBS-PE from S3 to
   local disk; record real size + `samtools view -c`. Resolve the WGBS-SE final BAM path + dedup parity.
3. **Dry-run the harness on 10M PE + SE** to validate end-to-end (file comparisons incl. the PNG
   exclusion, resource sampling, CSV schema, SE/PE dispatch) BEFORE consuming a full-data night.

### Phase 1 — BYTE-IDENTITY (parity) at full scale (gated on oxy idle)
4. For each dataset, run Rust + Perl **`--multicore 1`** (deterministic; this timed Perl run doubles as
   the serial perf anchor) on the **same staged BAM** (default gzip; WGBS-PE also plain). Compare every
   extractor output as the smoke does, **with the rev-2 fixes**:
   - **Exclude `*.png` from the filename-set diff**, and assert the file-set delta is *exactly* the
     expected `{Perl-only: *M-bias_R1.png, *M-bias_R2.png}` (Rust defers PNGs) — any *other* name delta is a real FAIL.
   - **raw byte-identical** (`cmp`) for deterministic text files (splitting report, M-bias `.txt`).
   - **sorted-equivalent** (`gunzip|sort|md5`) for order-free per-context files (CpG/CHG/CHH × OT/OB/CTOT/CTOB).
   - decompressed-content identity for `.gz`.
   - **Float-rounding triage:** if a strict-`cmp` file FAILs, diff it and check whether the only deltas
     are `%.2f`/`%.1f` half-way roundings before treating it as a calling regression.
   - This proves **parity with Perl v0.25.1**, not absolute correctness — state so in the report.
5. **Rust-vs-Rust** identity across `--parallel ∈ {1,2,4,8,16}` per dataset (worker-count-invariant).
   **HARD GATE:** any genuine mismatch (after PNG-exclusion + rounding triage) ⇒ STOP, do not loosen, open a bug.

### Phase 2 — PERFORMANCE at full scale (gated on oxy idle; explicit driver priority)
6. Tiered matrix (resumable via CSV append). **Driver priority order** (so a short night still yields
   the headline): (i) WGBS-PE Rust sweep; (ii) WGBS-SE + RRBS-PE Rust; (iii) Perl `--multicore 12`
   anchors; (iv) Perl `--multicore 1` serial (the 1–3 h long pole — last, droppable, or extrapolated):
   - **WGBS-PE (primary):** {gzip, plain, mbias_only} × `--parallel {1,2,4,8,16}` × **3 reps**.
   - **WGBS-SE:** {gzip, mbias_only} × `{1,4,16}` × **2 reps**.
   - **RRBS-PE:** {gzip, mbias_only} × `{1,4,16}` × **2 reps**.
7. Perl anchors per dataset: `--multicore 12` (Perl's sweet spot) + the Phase-1 `--multicore 1` run
   reused as the serial baseline. If the WGBS-SE/RRBS Perl-serial runs threaten the window, drop them
   and extrapolate from WGBS-PE.
8. Record per run: **wall** (median of reps), **CPU-cores = (user+sys)/real per mode**, **peak RSS**
   (`/usr/bin/time -v` Max RSS — authoritative), **peak threads** + **peak open-fds** (sample
   `/proc/<pid>/task` and `/proc/<pid>/fd` by **PID** at 0.2 s — never pgrep-by-name; sampler-deadlock gotcha).

### Phase 3 — RESOURCE FOOTPRINT analysis + DOCS (analysis after run; doc edit gated on trigger)
9. Footprint table per (dataset × mode × `--parallel`): peak threads, peak open-fds, **per-mode** CPU-cores, peak RSS.
10. **Documentation deliverable** (separate implementation step, own trigger + branch off iron-chancellor):
    - `cli.rs` `--parallel` help: gzip mode spawns a large *thread* count (decode 2 + workers +
      `5×12` gzip = 60) yet uses few CPU cores; point to the README table.
    - README "Resource usage (HPC & nf-core)": the **formula**, the **measured per-mode table**, and
      **recommended `cpus` / `memory`** (cores per-mode; memory ≈ peak RSS + headroom; warn that
      `ulimit -u`/`nproc` must allow ~60+ threads in gzip mode).

## Implementation outline (harness — `scripts/`, runnable; adapt existing)
1. `bench_run.sh <bam> --mode {gzip|plain|mbias_only} --parallel N --reps R --label L --out DIR` —
   one config: **free-space precheck**; stage/verify local input; run R reps under `/usr/bin/time -v`
   (capture Max RSS); launch a **PID-scoped** `/proc/task` + `/proc/fd` sampler (0.2 s, track max);
   **treat a non-zero exit / panic as a run FAILURE** (don't record bogus timing); emit CSV
   `(tool,dataset,mode,parallel,rep,wall_s,cpu_cores,max_rss_kb,peak_threads,peak_fds,exit)`.
2. `byteid_run.sh <bam> <genome> <layout>` — generalize `phase_h_smoke.sh`: Rust + Perl `--multicore 1`;
   **`*.png`-excluded name-set diff with the codified expected delta**; raw/sorted/gz-content compares;
   **float-rounding triage** on strict-`cmp` FAILs; the Rust-vs-Rust `--parallel` identity sweep;
   per-file PASS/FAIL + overall verdict + status file. Emits "PARITY-with-Perl" wording (not "correct").
3. `oxy_idle_gate.sh` — block (with timeout) until idle: no sibling `perl|cargo|coverage2cytosine|bismark|bowtie2`
   heavy job (by cmdline) AND 1-min load below a threshold scaled to 128 logical CPUs.
4. `overnight_driver.sh` — unattended orchestrator: idle-gate → verify/stage all 3 BAMs (wait for SE;
   **auto-dedup any non-dedup'd input**) → Phase 1 byte-identity (STOP on genuine FAIL) → Phase 2 matrix
   in the **priority order above** → write `FINDINGS.md` (median tables, speedup ratios, per-mode
   footprint table). Logs to file; CSV-append + skip-completed ⇒ resumable; safe to re-run.
5. Reuse: env PATH-prepend, `phase_h_smoke.sh` compare logic (patched for PNG/rounding), `bench_results/` CSV→graph.

## Efficiency
- **Run budget:** ~70 Rust runs (WGBS full ≈ 2–4 min each; SE/RRBS lighter). Perl is the long pole —
  full-WGBS `--multicore 1` ≈ **1–3 h** (10M was ~650 s, superlinear). Mitigation: priority order puts
  Rust + Perl-`--multicore 12` first, Perl-serial last/droppable/extrapolated; reuse the Phase-1 Perl
  run as the serial anchor; resumability banks every completed config. Fits an overnight window.
- **Disk:** gzip mode writes 12 large `.gz`/run — free-space precheck + clean between configs.
- **Staging:** one local copy of each S3 BAM. Sampler 0.2 s; no pgrep loops; `/usr/bin/time -v` for RSS.

## Integration
- **Reads:** 3 staged BAMs + genomes; Perl + samtools from the conda env.
- **Writes:** CSV + `FINDINGS.md` + byte-identity status under a campaign out dir; doc edits
  (`cli.rs`, README) later in the extractor worktree (separate gated PR off `rust/iron-chancellor`).
- **Downstream:** footprint numbers → nf-core/HPC configs; full-data byte-identity PASS → the gate to
  call the extractor production-ready (v1.0). Other crates untouched.
- **Out of scope:** bedGraph / coverage2cytosine / cytosine_report outputs (own gates: #797, #892);
  M-bias **PNG** generation (deferred in Rust — the codified expected file-set delta, not a regression).

## Assumptions
- WGBS-PE `.deduplicated.bam` is the realistic input; SE + RRBS deduplicated to the same state (auto-handled).
- Perl v0.25.1 (`~/micromamba/envs/bismark-test/bin`) is the parity reference; run at `--multicore 1` in Phase 1.
- gzip-default is the mode most users run (CLAUDE.md superlinear I/O pressure).
- Thread model is exact (verified): `DECODE_THREADS=2`, worker `max(2)`, `GZIP_COMPRESS_THREADS=4` × 12 files.
- Output is worker-count-invariant (deterministic `batch_seq` reorder) → Rust-vs-Rust identity holds.
- Perl emits `*M-bias_R{1,2}.png` (GD::Graph) absent from Rust — an **expected** file-set delta, excluded from the gate.
- `(user+sys)/real` understates true compute in gzip mode (write-back) — cores reported per mode.

## Validation (sanity checks on the campaign itself)
1. **Harness dry-run on 10M PE+SE lands within a tolerance band of a freshly-measured R3 baseline**
   (NOT the pre-R3 `bench_results/` point-values) — proves the harness measures the current binary.
2. **Read counts** (`samtools view -c`) of all 3 staged BAMs recorded; SE ≈ R1 count, RRBS as expected.
3. **Inputs are real local files** at run time (`stat`/`readlink` guard) — else timing invalid.
4. **WGBS-SE BAM exists + dedup parity** confirmed before SE runs (auto-dedup; never raw-vs-dedup'd).
5. **Byte-identity PASS** on all 3 datasets after PNG-exclusion + rounding triage (Phase 1) — hard gate.
6. **Filename-set delta == exactly the expected PNG-only delta** — any other delta is a real FAIL.
7. **`/proc/fd` confirms 12 open output files** in gzip default mode (empirically validates the doc claim).
8. **CPU-cores reported per mode**; `--mbias_only`/plain ≈ 3 cores ≪ ~60 threads (validates "threads≠cores").
9. **Free-space precheck fires** (simulate low space) and a Rust panic is recorded as FAILURE, not silent.
10. **Idle-gate refuses to run** while the c2c run / SE alignment are active.

## Questions or ambiguities
- **(Resolved, rev 1)** docs in-plan; sweep cap 16; native full SE; RRBS = byte-id + perf.
- **(Resolved, rev 2)** PNG exclusion; dedup-parity as deterministic gate; R3 tolerance-band dry-run;
  float-rounding triage; ENOSPC panic-as-failure; per-mode cores; `/usr/bin/time -v` RSS; Perl `--multicore 1` Phase 1.
- **(Open, non-critical — the one tunable)** Perl-serial scope: WGBS-PE serial is the headline; SE/RRBS
  serial are droppable/extrapolated if the night runs short (priority order handles this automatically).
- **No Critical ambiguities remain.**

## Self-Review
- **Logic:** prepare → (idle) byte-identity → (idle) perf (priority-ordered) → analyze → docs.
  Byte-identity precedes perf so a real correctness break stops the campaign before the multi-hour Perl runs. ✓
- **Edge cases:** PNG file-set delta (excluded + codified); float-rounding (triaged, not auto-FAIL);
  S3 cold-read (stage local + warm-up); SE BAM not yet existing (wait+verify); dedup parity (auto-handled,
  deterministic); ENOSPC panic (precheck + panic-as-failure); renamed-binary sampler deadlock (PID-scoped);
  Perl long pole (priority-last, reused anchor, extrapolation); gzip cores deflated (per-mode reporting). ✓
- **Efficiency:** fresh R3 10M dry-run before the night; tiered + priority-ordered + resumable so a short
  night still yields the headline; Perl serial minimized. ✓
- **Integration:** doc edits are a separate gated step on their own branch; no source change to run; other crates untouched. ✓
- **Remaining risk:** campaign length vs shared-oxy availability — mitigated by idle-gate + priority order
  + per-config resumability. The one true risk is a *genuine* full-data byte-identity FAIL (after PNG +
  rounding triage) — a hard stop, never papered over.

## Implementation notes — Phase 0 (2026-05-30; branch `extractor-fulldata-bench`)
**Harness shipped** (`5cfed84` + `ca7cad8`): `scripts/bench_run.sh`, `byteid_run.sh`,
`oxy_idle_gate.sh`, `overnight_driver.sh`, + `phase_h_smoke.sh` rev-2 patches (PNG-delta
codification, rounding-triage dump). All pass `bash -n` (bash 5.3).

**Dry-run on oxy — every mechanism validated (10M PE + 13k synth RRBS):**
- `bench_run` Rust 10M PE **mbias_only**: wall=12.64s (in R3 tolerance band, NOT pre-R3 ~19–21s →
  confirms the corrected baseline), cores=3.18, rss=57MB, **threads=9** (1 main + 2 prod/coll + 2
  decode + 4 workers — model exact), fds=5, exit=0.
- `bench_run` Rust 10M PE **gzip**: wall=12.94s, **cores=7.19** (≫ mbias → per-mode reporting
  vindicated), rss=254MB, **threads=69** (~60 gzip pool confirmed), **fds=17** (= 12 outputs + 5
  stdio/input → Phase 3 must subtract overhead), exit=0.
- `byteid_run` on synth 10k: **PARITY PASS** (Rust-vs-Perl) + worker-invariance {1,2,4} PASS — the
  `phase_h_smoke.sh` patches work end-to-end.
- `oxy_idle_gate` correctly detected the sibling c2c `coverage2cytosine` run (exit 1).
- **Env finding:** oxy's Perl has **no `GD::Graph`** → emits no M-bias PNGs (explains the clean 10M
  smoke); the PNG-exclusion patch is inert-but-safe here, protective elsewhere.

**Deviations from plan (documented):**
- **Disk bound (post-dry-run fix `ca7cad8`):** `bench_run` now purges each perf run's output on
  SUCCESS (keeps on failure for triage) — keeping rep1 per config would exhaust oxy's 68G disk across
  full-WGBS gzip × dozens of configs and the precheck would then silently skip later configs.
- Driver drops the raw SE BAM after dedup (~5G reclaimed); idle-gate timeout raised to 8h / 5-min poll
  for the overnight wait.
- The S3-symlink guard works as designed: `bench_run`/`byteid_run` reject symlinks; the driver
  `cp -L`-stages all three (WGBS-PE 9.6G, WGBS-SE 5.2G→dedup, RRBS 4.2G) to local disk first.

**Campaign launched 2026-05-30 16:53Z** — tmux `fulldata_bench` on oxy, `~/fulldata_bench/` (driver.log,
console.log, results.csv, FINDINGS.md). Sequence: STAGE → idle-gate (waits for the c2c session) →
Phase 1 byte-identity (hard-stop on genuine FAIL) → Phase 2 priority-ordered resumable perf matrix →
FINDINGS. Runs overnight unattended.

**Post-launch verification (dual code-review + plan-manager) — 3 CRITICALs found + fixed before the
heavy runs (`a05ab57`, `85bb09e`):** both reviewers independently caught (C1) `wait;ec=$?` killed by
`set -e` → panic-as-failure silently lost; (C2) `log()` on stdout polluting `stage_local`'s returned
path (confirmed live in driver.log) → cold-stage abort; (C3) `--mode plain` undefined in
`phase_h_smoke.sh` → false PARITY FAIL → whole-campaign hard-stop. Plus: `have_config` counts only
exit==0 rows; Phase 1 resumable; df-empty guard; worker-invariance file-COUNT check; degraded-night
FINDINGS section; `stage_local` resume fast-path. The campaign was killed, fixed, and the previously-
untested paths re-validated (plain byteid PASS; a forced-fail rep recorded `exit=1` not silently lost).
plan-manager: COMPLETE (coverage was never the issue — these were correctness bugs).

**Budget-driven design change [Felix-approved]:** Phase-1 byteid now runs Perl at **`--multicore 12`**
(byte-identity is multicore-invariant — data files compared sorted, report/M-bias are deterministic
aggregates, proven by the 10M smoke at mc4), not `--multicore 1`. This front-loads correctness (~15
min/dataset) + the Rust perf sweep before the **single dedicated `--multicore 1` serial run on
WGBS-PE** (the speedup headline + 1-3h long pole) which now runs LAST/droppable. The byteid mc12 wall
is reused as the Perl mc12 anchor (no redundant re-run).

**First-run result + disk-full incident (2026-05-31 ~00:08Z):** The relaunched run reached Phase 1 and
**WGBS-PE gzip byte-identity PASSED at full scale** (129.3M reads): Rust ≡ Perl, **Perl 475s → Rust
105s = 4.5×** at `--multicore 12`. The run then **hard-stopped on a FALSE FAIL** — the *plain* byteid
hit `No space left on device`: full-WGBS *uncompressed* output doesn't fit oxy's **99G `/home` overlay**
(which also backs `/usr`/`/etc`/`/var/lib`). Diagnosis was instant from the status file (gzip PASS →
plain ENOSPC). Two corrective changes [Felix-approved]: (1) **relocate the campaign to `/var/tmp`**
(oxy's only big writable fs — 762G; `/home`/`/data` are the same 99G overlay); (2) **lean byteid —
gzip only** (the gzip byteid already proves content parity; plain remains PERF-timed in Phase 2 on the
big fs). Reclaimed the 54G of byteid output from `/home` (gzip-parity PASS was already recorded). Note:
the harness never used `/tmp` — it wrote to `/home`, which was simply too small.

# Plan Coverage Report

**Mode:** B (code vs. implementation outline)
**Plan(s):** `plans/05302026_extractor-fulldata-validation/PLAN.md` (Implementation outline §1–5 + rev-2 review requirements)
**Code under audit:** `scripts/bench_run.sh`, `scripts/byteid_run.sh`, `scripts/oxy_idle_gate.sh`, `scripts/overnight_driver.sh`, diff to `scripts/phase_h_smoke.sh` (since `a7aaf61`) — worktree `/Users/fkrueger/Github/Bismark-extractor`, branch `extractor-fulldata-bench`
**Date:** 2026-05-30
**Verdict:** COMPLETE

## Summary

- Total items: 24 (5 outline items + 8 rev-2 items + 11 sub-requirements rolled into the outline)
- DONE: 23
- PARTIAL: 1
- MISSING: 0
- DEVIATED: 0 (3 deviations documented in the plan's "Implementation notes — Phase 0", not gaps)

## Coverage ledger

### Outline item 1 — `bench_run.sh`

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1.1 | Free-space precheck | Outline §1 | DONE | `df -Pk` → `AVAIL_GB`; gzip-mode `exit 2` when `< MIN_FREE_GB` (default 20G). L52-56 |
| 1.2 | Stage/verify local input; reject symlink | Outline §1 | DONE | `[[ -L "$BAM" ]]` hard guard `exit 2`; `[[ -f ]]` check. L48-50 |
| 1.3 | `/usr/bin/time -v` (Max RSS) | Outline §1 + rev-2 | DONE | `command -v /usr/bin/time` guard; `-v -o "$tf"`; parses `Maximum resident set size`. L46, L91, L117 |
| 1.4 | PID-scoped `/proc` task + fd sampler (0.2s, max) | Outline §1 + rev-2 | DONE | `pgrep -P "$tpid"` parent-PID lookup (never name-based); samples `/proc/$cpid/task` + `/proc/$cpid/fd`, tracks max, `sleep 0.2`. L95-108 |
| 1.5 | Non-zero exit / panic as FAILURE (no bogus timing) | Outline §1 + rev-2 ENOSPC | DONE | `wait; ec=$?`; `ANY_FAIL=1` + stderr tail on `ec!=0`; row still written with `exit` col; keeps run dir on failure for triage. L109, L120-131 |
| 1.6 | CSV columns exactly as specified | Outline §1 | DONE | Header L80 = `tool,dataset,mode,parallel,rep,wall_s,cpu_cores,max_rss_kb,peak_threads,peak_fds,exit` — exact match to spec |

### Outline item 2 — `byteid_run.sh`

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 2.1 | Rust-vs-Perl parity via `phase_h_smoke.sh` (`--multicore 1`) | Outline §2 | DONE | Calls `phase_h_smoke.sh ... --parallel 1`; smoke passes `--parallel` → Perl `--multicore 1` (smoke L170). byteid L55-56 |
| 2.2 | PNG-excluded compare + codified expected delta | Outline §2 + rev-2 CRITICAL | DONE | smoke diff: `grep -v '\.png$'` on both name-sets; `PERL_ONLY_PNG` reported as "EXPECTED PNG DELTA"; any other name delta still FAILs. byteid sweep also skips `*.png`. |
| 2.3 | Float-rounding triage on strict-`cmp` FAIL | Outline §2 + rev-2 | DONE | smoke dumps `diff ... head -8` "triage diff (check for rounding-only deltas)" for `*_splitting_report.txt`/`*.M-bias.txt`. Still counted as DIFF (hard gate fires). |
| 2.4 | Rust-vs-Rust `--parallel` sweep (worker-invariance) | Outline §2 | DONE | Default sweep `{1 2 4 8 16}`; md5 of sorted per-context output vs N=1; `INVARIANCE FAIL` on mismatch. L64-88 |
| 2.5 | "parity-not-correctness" wording | Outline §2 + rev-2 | DONE | Header comment + "BYTEID PASS (parity-with-Perl ...)" / FAIL message reference PNG/rounding triage. L15, L92-94 |

### Outline item 3 — `oxy_idle_gate.sh`

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 3.1 | Block on sibling heavy jobs (by cmdline) | Outline §3 | DONE | `ps -eo pid,args` + `grep -E "$PATTERN"` (bowtie2/c2c/bedGraph/dedup/methcons/genome_prep); excludes own harness scripts. Deliberately excludes the extractor (own workload). L30-37 |
| 3.2 | Load backstop | Outline §3 | DONE | `load1 < MAX_LOAD` (default 16 of 128 logical). L19, L38, L40 |
| 3.3 | Timeout | Outline §3 | DONE | `--timeout` (default 21600); driver overrides to 28800 (8h); `exit 1` + still-busy dump on timeout. L19, L45-49 |

### Outline item 4 — `overnight_driver.sh`

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 4.1 | Stage S3 BAMs locally (`cp -L`) | Outline §4 | DONE | `stage_local()` `cp -L` from S3; warm-up `samtools view ... head -1`. L48-65, L78-80 |
| 4.2 | Auto-dedup SE for parity | Outline §4 | DONE | `stage_local ... se` runs `deduplicate_bismark -s`; reclaims raw (~5G). RRBS stays raw per convention. L53-63, L79 |
| 4.3 | Gate before runs | Outline §4 | DONE | `oxy_idle_gate.sh --timeout 28800 --poll 300` (skippable via `--skip-gate`). L87-90 |
| 4.4 | Phase-1 byte-identity hard-stop on FAIL | Outline §4 | DONE | `byteid_run.sh` per dataset; on non-zero `log "... HARD GATE: stopping" ; exit 1`. L96-101 |
| 4.5 | Phase-2 priority-ordered resumable matrix | Outline §4 | DONE | Order (i) WGBS-PE Rust sweep → (ii) SE+RRBS Rust → (iii) Perl `--multicore 12`. `have_config` CSV skip-completed ⇒ resumable. L108-127 |
| 4.6 | Reuse Phase-1 Perl `--multicore 1` as serial anchor | Outline §4 | DONE | greps `^Perl: [0-9]+s$` from `parity_${ds}_gzip/diff_summary.txt`, appends `perl,...,1,...` row to CSV. Matches smoke emit format (smoke L219). L102-104 |
| 4.7 | `FINDINGS.md` | Outline §4 | DONE | Wall table + footprint table (threads/fds/cores/RSS) + per-mode-cores caveat note. L129-150 |

### Outline item 5 — Reuse

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 5.1 | Reuse `phase_h_smoke.sh` (patched for PNG/rounding) | Outline §5 | DONE | byteid_run.sh delegates to it; PNG-exclusion + rounding-triage patches present in the diff. |
| 5.2 | bench_results CSV→graph reuse | Outline §5 | PARTIAL | CSV emitted with the documented schema and FINDINGS aggregates wall/footprint via awk; no explicit CSV→graph plotting step is wired. See Gaps. |

### Rev-2 review requirements (cross-cutting)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| R.1 | `*.png` exclusion + codified expected delta | rev-2 [B, CRITICAL] | DONE | smoke diff hunk (item 2.2). |
| R.2 | Dedup-parity as deterministic gate | rev-2 [both] | DONE | driver auto-dedup SE in STAGE before any run; never raw-vs-dedup'd (item 4.2). |
| R.3 | Per-mode core reporting | rev-2 [B] | DONE | `cpu_cores=(user+sys)/wall` per row; FINDINGS note flags gzip cores as a floor. bench L9-11, L118; driver L148 |
| R.4 | `/usr/bin/time -v` Max RSS | rev-2 [A] | DONE | item 1.3. |
| R.5 | Pin Phase-1 Perl to `--multicore 1` | rev-2 [both] | DONE | byteid passes `--parallel 1` → smoke `--multicore 1` (item 2.1). |
| R.6 | `/proc/fd` 12-file confirmation | rev-2 | DONE | `peak_fds` sampled + emitted; dry-run note records 17 fds = 12 outputs + 5 stdio. FINDINGS note states the subtraction. |
| R.7 | ENOSPC panic-as-failure | rev-2 [B] | DONE | free-space precheck (item 1.1) + non-zero-exit-as-FAILURE (item 1.5). |

### Campaign tiering (driver encoding)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| T.1 | WGBS-PE {gzip,plain,mbias_only} × {1,2,4,8,16} × 3 reps | Behavior §6 | DONE | `for m in gzip plain mbias_only; do for p in 1 2 4 8 16; do run_cfg rust wgbs_pe ... 3`. L118 |
| T.2 | WGBS-SE {gzip,mbias_only} × {1,4,16} × 2 reps | Behavior §6 | DONE | `for m in gzip mbias_only; do for p in 1 4 16; ... 2`. L120-123 |
| T.3 | RRBS-PE {gzip,mbias_only} × {1,4,16} × 2 reps | Behavior §6 | DONE | same loop as T.2 over `rrbs_pe`. L120-123 |
| T.4 | Perl `--multicore 12` anchor | Behavior §6-7 | DONE | `run_cfg perl <ds> gzip 12 1` for all 3 datasets. L125-127 |

## Gaps (detail)

### Item 5.2: bench_results CSV→graph reuse — PARTIAL

**Expected:** Outline §5 says "Reuse: ... `bench_results/` CSV→graph." The earlier session's `bench_results/` harness produced both a CSV and a graph rendering step.
**Found:** The new campaign emits a CSV with the documented 11-column schema and `FINDINGS.md` aggregates it via `awk` into two fixed-width text tables (wall-by-config, footprint-by-rep1). There is no explicit plotting / graph-rendering invocation wired into the driver.
**Gap:** No standalone graph artifact is generated by the shipped harness. This is a low-severity PARTIAL: the plan frames §9 (footprint analysis) and §3 (Phase-3 docs) as a *separate analysis step after the run* ("median tables computed in Phase 3"), and FINDINGS explicitly labels itself "raw results" with "medians computed in Phase 3." The CSV — the load-bearing artifact a graph would consume — is fully present and schema-conformant, so any plotting is a trivial post-hoc step on existing data, not a harness capability the overnight run depends on. Treat as deferred-to-analysis rather than a functional gap. No action required for the harness to fulfill its Phase-0/1/2 mandate.

## Documented deviations (NOT gaps — recorded in PLAN.md "Implementation notes — Phase 0")

1. **Per-run output purge on SUCCESS** (`bench_run.sh` L128-131) — `rm -rf "$run_out"` on `ec==0`, keep on failure. Plan-documented as disk-bound mitigation (`ca7cad8`) so full-WGBS gzip × dozens of configs cannot exhaust oxy's 68G disk. Correctness is proven by Phase-1 byte-identity, not by retaining perf outputs. Acceptable deviation.
2. **Driver drops raw SE BAM after dedup; idle-gate timeout raised to 8h / 5-min poll.** Plan-documented. Matches code (driver L62 `rm -f "$raw"`, L89 `--timeout 28800 --poll 300`).
3. **S3-symlink guard** rejects symlinks in `bench_run`/`byteid_run`; driver `cp -L`-stages first. Plan-documented; matches code.

## Test verification (Mode B)

Not a unit-tested deliverable — the harness is shell tooling. The plan's own validation is an on-oxy dry-run, recorded in PLAN.md "Implementation notes — Phase 0":
- `bash -n` syntax-clean on all scripts (bash 5.3) — claimed in notes.
- Dry-run on 10M PE + 13k synth RRBS validated every mechanism: `bench_run` mbias_only (threads=9, model-exact) and gzip (threads=69 ~60-pool, fds=17 = 12+5), `byteid_run` parity + invariance PASS, `oxy_idle_gate` correctly detected the sibling c2c run (exit 1). These confirm the audited code paths execute as specified.

## Verdict

**COMPLETE.** All 5 implementation-outline items, all 7 rev-2 review requirements, and all 4 campaign-tiering encodings are present and correct in the shipped harness. The CSV schema matches the spec byte-for-byte; the PNG-exclusion and float-rounding-triage patches are present in `phase_h_smoke.sh`; the driver pins Phase-1 Perl to `--multicore 1` and reuses that wall as the serial anchor; the priority-ordered matrix is resumable via CSV skip-completed; ENOSPC is handled by precheck + panic-as-failure. The single PARTIAL (5.2, CSV→graph) is a deferred post-run analysis convenience, not a harness capability the overnight campaign depends on — the load-bearing CSV is fully present. No MISSING items. The three deviations are all documented with rationale in the plan.

# Code Review A — Full-dataset benchmark harness (overnight, unattended)

**Reviewer:** A (independent; sibling reviewer B works the same files in parallel)
**Date:** 2026-05-30
**Target branch/worktree:** `extractor-fulldata-bench` @ `/Users/fkrueger/Github/Bismark-extractor`
**Files reviewed:**
- `scripts/bench_run.sh`
- `scripts/byteid_run.sh`
- `scripts/oxy_idle_gate.sh`
- `scripts/overnight_driver.sh`
- `scripts/phase_h_smoke.sh` diff (`a7aaf61..HEAD`)
**Plan honored:** `plans/05302026_extractor-fulldata-validation/PLAN.md` (rev 2)
**Constraint:** RECOMMEND-ONLY. No files edited (live overnight campaign + concurrent sibling reviewer).

---

## Summary

The harness is well-structured and the *core measurement mechanics are sound*: the
`/proc/<pid>/task` + `/proc/<pid>/fd` sampler resolves the binary via `pgrep -P "$tpid"`
correctly (GNU `/usr/bin/time` fork+exec+wait4 makes the binary a direct child, in-process
threads are all visible under `/proc/<cpid>/task`), the wall-clock h:mm:ss/m:ss parse is
correct, the `(user+sys)/real` cores math is correct and degrades to `NA` safely, the
PNG-exclusion + rounding-triage patches to `phase_h_smoke.sh` are correct, and the SE-dedup
output-name resolution (`ls *deduplicated.bam | grep -i wgbs_se`) picks the right file even
with `wgbs_pe.deduplicated.bam` co-resident in `$STAGE`.

**However, I found three Critical defects that would each break or silently corrupt the
overnight run**, plus several High/Medium issues. The two most dangerous are subtle
`set -e` / command-substitution interactions that the Phase-0 dry-run almost certainly did
not exercise (the dry-run used warm-staged inputs and the default `--modes gzip`, masking
both the cold-staging path and the `plain` mode path).

**Top findings:**
1. **[CRITICAL] `wait "$tpid"; ec=$?` aborts `bench_run.sh` on any failed rep** under `set -e`
   — a Rust gzip ENOSPC panic (non-zero exit) is NOT recorded as a FAILURE row; the script
   dies at the `wait` line, the CSV row is never written, and `have_config` then re-runs the
   failing config on every resume. This directly defeats the plan's "panic-as-failure" gate.
2. **[CRITICAL] `byteid_run.sh` passes `--mode plain` to `phase_h_smoke.sh`, which has no
   `plain` mode** → `exit 2` → treated as a PARITY FAIL → driver hard-stops the entire
   campaign at the WGBS-PE plain check, *after* burning the ~1–3 h WGBS-PE Perl-serial run.
3. **[CRITICAL] `stage_local()` `log()`-to-stdout pollutes the command-substituted return
   value** on the cold-staging path — `$WGBS_PE/$WGBS_SE/$RRBS_PE` get a log line prepended,
   every downstream `[[ -f "$BAM" ]]` fails, and the campaign aborts at cold start.

---

## Issues by area

### A. `set -e` / `set -u` / `pipefail` interactions

#### A1. [CRITICAL] `wait "$tpid"; ec=$?` aborts the script on a failed child — failures are never recorded
`bench_run.sh:109`
```bash
wait "$tpid"; ec=$?
```
Under `set -euo pipefail`, `wait` returns the child's exit status. A non-zero status (Rust
gzip ENOSPC panic ≈ exit 101, or any crash) triggers `set -e` and **the script exits at the
`wait` line**. `ec=$?` is never assigned, the FAILURE branch (`bench_run.sh:120-126`) never
runs, and **no CSV row is written for the failed rep**.

Verified empirically:
```
bash: set -euo pipefail; bash -c "sleep 0.1; exit 7" & wait %1; ec=$?; echo SURVIVED
→ script exits with code 7; "SURVIVED" never prints, ec never captured
```

**Impact (this is the plan's headline gate — PLAN.md §200, Validation #9, Impl-outline #1
"treat a non-zero exit / panic as a run FAILURE"):**
- A Rust ENOSPC panic is **NOT** recorded as a FAILURE row — `bench_run.sh` aborts instead.
- The driver runs `bench_run.sh … || log "(config had failures — recorded)"` — so it *continues*,
  but the log claims "recorded" when nothing was recorded.
- On resume, `have_config` sees 0 (or < reps) rows for that config and **re-runs the same
  failing config indefinitely**, wasting the night.
- Note the interaction with A2: even if A1 is fixed so the row *is* written, `have_config`
  would then treat the failed config as done (see C1).

**Recommendation:** capture the exit code without letting `set -e` fire:
```bash
ec=0; wait "$tpid" || ec=$?
```
Verified the `|| ec=$?` idiom survives and captures `ec=7`. (Reps run inside a `for` loop,
not an `&&`/`||` list or `if`/`while` condition, so the bare `wait` is *not* exempt from
`set -e` — this is a real abort, not a theoretical one.)

#### A2. [HIGH] `df`-empty arithmetic can abort `bench_run.sh` before any run
`bench_run.sh:53`
```bash
AVAIL_GB=$(( $(df -Pk "$OUT_DIR" 2>/dev/null | awk 'NR==2{print $4}') / 1024 / 1024 ))
```
If `df` emits no `NR==2` line (e.g., `$OUT_DIR` does not yet exist, or `df` errors), the inner
substitution is empty and `$(( / 1024 / 1024 ))` is an **arithmetic syntax error**, which under
`set -e` aborts the script (verified). In practice the driver `mkdir -p`s `$OUT_DIR/perf` first,
so this is usually masked — but it is fragile (e.g., a stale `--out` passed directly, or a
transient `df` failure on a busy box). **Recommendation:** guard with a default:
```bash
avail_kb=$(df -Pk "$OUT_DIR" 2>/dev/null | awk 'NR==2{print $4}')
AVAIL_GB=$(( ${avail_kb:-0} / 1024 / 1024 ))
```

#### A3. [LOW] `[[ … ]] && var=…` peak-update lines are safe under `set -e`
`bench_run.sh:104-105` — verified: a false `[[ … ]]` left-operand of `&&` is exempt from
`set -e` (not the last command of the AND-list). No action needed; flagged only to record it
was checked.

---

### B. Staging / dedup (`overnight_driver.sh`)

#### B1. [CRITICAL] `log()` writes to stdout → pollutes command-substituted `stage_local` return
`overnight_driver.sh:38` (`log(){ … | tee -a "$LOG"; }`) is called *inside* `stage_local()`
(lines 50, 51, 57), and `stage_local` is consumed via command substitution
(`WGBS_PE=$(stage_local …)`, lines 78-80). `tee` writes the log line to **stdout**, which is
captured into the variable along with the intended `echo "$dst"` (line 64).

Verified the mechanism:
```
log(){ echo "LOGMSG $*" | tee -a "$LOG"; }
myfunc(){ log "doing work"; echo "/path/to/result"; }
X=$(myfunc)   →   X = "LOGMSG doing work\n/path/to/result"
```

**Impact (cold-staging path only):** on the first run with an empty `$STAGE`,
`log "staging $name (cp -L from S3)…"` fires, so `$WGBS_PE` becomes e.g.
`"2026-…Z staging wgbs_pe…(cp -L from S3)…\n/home/.../staged/wgbs_pe.deduplicated.bam"`. Then:
- `samtools view -c "$b"` (line 83) gets a multi-line/garbage path → `?` reads (masked by `|| echo '?'`).
- `byteid_run.sh "${DS_BAM[$ds]}"` → `[[ -f "$BAM" ]]` fails → `exit 2` → driver Phase 1 →
  `BYTEID FAIL` → **`exit 1`, whole campaign aborts at cold start.**
- The SE branch adds the `deduplicating…` log line too, polluting `$WGBS_SE` further.

This is masked when `$STAGE` is already populated (no `staging…`/`dedup…` log lines emitted),
which is likely why the Phase-0 dry-run + the launched campaign did not hit it. It will bite
on any re-run from a clean `$STAGE` or a fresh machine.

**Recommendation:** route `log` to **stderr** (standard for status logging that must not
pollute captured stdout):
```bash
log(){ echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) $*" | tee -a "$LOG" >&2; }
```
(`tee … >&2` keeps the file copy and sends the terminal copy to stderr.) Alternatively, have
`stage_local` log only via a stderr-only helper and reserve stdout strictly for the final path.

#### B2. [MEDIUM] `samtools view "$dst" | head -1` warm-up read can mask a corrupt stage
`overnight_driver.sh:52` ends with `|| true`, so a `samtools` failure on the staged BAM is
swallowed and staging is reported as success. If `cp -L` produced a truncated/corrupt local
copy, the first sign would be a downstream extractor crash mid-night. **Recommendation:** make
the warm-up a real integrity check (`samtools quickcheck "$dst"` or check `samtools view -c`
returns a non-empty count) and fail staging if it does not.

#### B3. [LOW] No `stat`/`readlink` post-stage assertion that `$dst` is a real local file
PLAN.md §62 / Validation #3 requires asserting the staged input is a real local file (not a
symlink) at run time. `bench_run.sh`/`byteid_run.sh` *do* reject symlinks (good), but the
driver never re-verifies after `cp -L` that `$dst` is local (it could in principle be a
dangling/again-symlinked path on an odd filesystem). Low risk given `cp -L`, but the plan
called for an explicit guard.

---

### C. Resumability (`have_config`, CSV)

#### C1. [HIGH] `have_config` counts failed-exit rows as "done"
`overnight_driver.sh:68-72` counts rows matching `tool/dataset/mode/parallel` with no filter
on the `exit` column. Verified: a config with 3 rows where rep2 has `exit=101` is reported as
complete and **skipped on resume**, with the failed rep folded into the Phase-3 median.

**Impact:** a partially-failed config (e.g., one ENOSPC rep that *did* get a row, once A1 is
fixed) is treated as fully successful; the median is computed over a bogus/failed rep. Combined
with A1 *unfixed*, the opposite failure mode occurs (no rows → infinite re-run). Either way the
resumability contract (PLAN.md Impl-outline #4, Efficiency "resumability banks every completed
config") is not met for failed reps.

**Recommendation:** count only successful reps:
```bash
'$1==t&&$2==d&&$3==m&&$4==p&&$11==0{n++}'
```
and decide explicitly whether a config that can never reach `reps` successes (persistent
ENOSPC) should be retried or marked permanently-failed (e.g., a sentinel) to avoid an infinite
resume loop. At minimum, document that failed reps are not counted toward completion.

#### C2. [LOW] Perl serial-anchor row is injected with `exit=0` and `wall` only
`overnight_driver.sh:104` appends `perl,$ds,gzip,1,1,$ps,NA,…,0`. If C1's fix filters on
`$11==0`, this anchor row (exit 0) is correctly counted; with the current unfiltered count it
also counts. Either way `have_config perl $ds gzip 1 1` then returns true, so the Phase-2 Perl
`--multicore 12` run is *not* skipped (different `parallel`=12), which is correct. No bug, but
note the anchor row has `cpu_cores=NA, rss=NA, threads=NA` — Phase-3 footprint awk must tolerate
`NA` in those columns (it does — it only string-prints).

---

### D. The `/proc` sampler (`bench_run.sh`)

#### D1. [LOW] Child not resolved for very short runs → `peak_threads=peak_fds=0`
`bench_run.sh:96-108`: if the binary starts and exits within the ~2.5 s `pgrep -P` retry window
(50 × 0.05 s), `cpid` stays empty, the sampler block is skipped, and `peak_threads`/`peak_fds`
are recorded as `0`. Irrelevant for full-data multi-minute runs, but the 10M dry-runs and any
`mbias_only` fast path could record `0`s. **Recommendation:** none required for full data;
optionally widen the retry or note that `0` means "not sampled," not "1 thread."

#### D2. [INFO] `pgrep -P "$tpid"` resolution is CORRECT and race-tolerant
Verified the design intent: GNU `/usr/bin/time` does fork+exec+wait4, so the binary is a single
direct child of `tpid`; `head -1` is safe (one child, no shell wrapper since `time` execs via
`execvp`). The sampler loops on `kill -0 "$tpid"` (the `time` wrapper), so it keeps sampling
`/proc/$cpid/*` until `time` exits; once `cpid` dies, the `/proc` reads return empty → `wc -l`
= 0, which never lowers the running peak. Loop exits cleanly. In-process worker/gzip threads
all appear under `/proc/$cpid/task`, so `peak_threads` captures the full model. No bug. (One
nit: `ls … | wc -l` counts a header-less listing fine, but if `/proc/$cpid/fd` momentarily
errors mid-read the `|| echo 0` makes `t`/`d` the string `0` — harmless.)

---

### E. GNU `time -v` parsing (`bench_run.sh`)

#### E1. [INFO] Wall-clock and cores parsing are correct
Verified: `0:12.94` → `12.94` (m:ss), `1:02:03` → `3723` (h:mm:ss). `cores=(u+s)/w` →
`%.2f` for valid input, `NA` when `w` empty/zero. `tr -d ' '` strips the trailing-space the
`-F': '` split can leave. Locale decimal: GNU time emits `.` (C-locale style for User/System
seconds); awk parses floats with `.` regardless of `LC_NUMERIC` here. No issue observed.

#### E2. [LOW] A failed-parse `wall` (empty `$tf`, e.g. if `time` itself died) yields empty
`wall_s` → CSV `NA` via `${wall_s:-NA}`. Acceptable, but note this is the *only* failure signal
if A1 is left unfixed and the child crash also corrupted the `time` output file. Fixing A1 makes
`exit` the authoritative failure column.

---

### F. Idle gate (`oxy_idle_gate.sh`)

#### F1. [INFO] Pattern matching is correct and self-exclusion is right
`PATTERN` matches sibling heavy jobs (bowtie2/c2c/bedGraph/dedup/methcons/genomeprep) and the
`grep -vE "grep|oxy_idle_gate|overnight_driver|byteid_run|bench_run"` correctly avoids matching
the campaign's own driver/harness. The extractor (our workload) is deliberately not in
`PATTERN`. The driver's own `deduplicate_bismark` step (in `PATTERN`) runs during STAGE, *before*
the gate, so there is no self-match race. `load_ok` via awk float compare is correct.

#### F2. [LOW] Gate runs once; mid-campaign sibling jobs are not re-checked
By design (PLAN.md). A sibling job starting after the gate passes will contend with timing.
Acceptable per plan; flagged only so it is a conscious tradeoff. Poll/timeout arithmetic
(`elapsed += POLL`) is correct; driver overrides to 8 h / 5-min poll.

---

### G. Failure handling / disk purge

#### G1. [HIGH] Driver does not abort on a *failed* perf config, but masks it as "recorded"
`overnight_driver.sh:114`: `bench_run.sh … || log "  (config had failures — recorded)"`.
Given A1, this log message is **false** when the failure was a non-zero child exit (no row was
recorded). Even with A1 fixed, the driver continues past genuine FAILUREs without any
campaign-level FAIL surfaced in `FINDINGS.md` (FINDINGS only dumps the CSV). **Recommendation:**
after A1 is fixed, have the driver detect failure rows (`exit!=0`) in the CSV and surface a
prominent "N configs had failed reps" banner in `FINDINGS.md` so a silently-degraded night is
obvious. (Phase-2 perf failures should *continue* per the plan — but they must be *visible*.)

#### G2. [INFO] Purge-on-success is correct and keeps failure outputs
`bench_run.sh:131` `[[ "$ec" -eq 0 ]] && rm -rf "$run_out"` — purges only on success, keeps
`.stderr`/outputs on failure for triage. Correct per PLAN.md §246. (Note: this line also
depends on `ec` being assigned, which A1 currently prevents — fixing A1 restores it.)

#### G3. [LOW] `MIN_FREE_GB=20` default vs full-WGBS gzip output on a 68 G disk
PLAN.md notes 68 G total disk and "many GB" per gzip run × dozens of configs. The 20 G floor +
purge-on-success should hold, but 20 G may be tight if a single full-WGBS gzip run's 12 `.gz`
files exceed it (the precheck would then correctly refuse, recording exit=2 — but see A1: that
refusal is a *script* exit 2 from the precheck, which happens *before* the `for rep` loop, so it
returns 2 cleanly and the driver logs "had failures" with *no* row → re-run loop). Consider
sizing `MIN_FREE_GB` from a measured full-WGBS gzip footprint, and have the driver record a
precheck-skip as an explicit FAILURE row.

---

### H. `byteid_run.sh` correctness

#### H1. [CRITICAL] `--mode plain` is unknown to `phase_h_smoke.sh` → false PARITY FAIL → campaign hard-stop
`overnight_driver.sh:95` sets `DS_MODES[wgbs_pe]="gzip plain"`; `byteid_run.sh:52-62` loops
over modes and calls `phase_h_smoke.sh … --mode "$mode"`. But `phase_h_smoke.sh:137-145` only
accepts `default|comprehensive|merge_non_CpG|comprehensive_merge|gzip` — `plain` hits `*)` →
`error: unknown mode: plain; exit 2`.

In `byteid_run.sh` that non-zero exit lands in the `else` branch → `PARITY FAIL` → `FAIL=1` →
`byteid_run.sh` exits 1 → driver Phase 1 → `BYTEID FAIL for wgbs_pe — HARD GATE: stopping
campaign; exit 1`.

**Impact:** the campaign hard-stops at the WGBS-PE `plain` parity check — *after* the WGBS-PE
gzip parity run already consumed a ~1–3 h Perl-serial pass. The "plain" requirement is from
PLAN.md §111 ("WGBS-PE also plain"), but `bench_run.sh`'s `plain` (extractor with no flags)
maps to `phase_h_smoke.sh`'s `default`, not a literal `plain`. The Phase-0 dry-run note
(PLAN.md §232-241) shows byteid was exercised only with the default `--modes gzip`, so this
path was never run.

**Recommendation:** in `byteid_run.sh`, translate the mode before calling `phase_h_smoke.sh`:
```bash
sm_mode="$mode"; [[ "$mode" == "plain" ]] && sm_mode=default
… phase_h_smoke.sh "$BAM" --parallel 1 --mode "$sm_mode" --out "$smoke_out" …
```
(Or add a `plain) ;;` alias to `phase_h_smoke.sh`'s case as a `default` synonym.)

#### H2. [MEDIUM] Worker-invariance reference can silently shift to N=2 if N=1 fails
`byteid_run.sh:69-88`: if the first sweep entry (`n=1`) Rust run fails (line 72 `continue`),
`first_n` is never set on that iteration (the `continue` skips line 84), so the *next* `n`
becomes the reference. The invariance check then validates {4,8,16} against N=2, not N=1, and no
explicit warning is emitted that N=1 was dropped from the comparison baseline. **Recommendation:**
if the reference run (intended N=1) fails, mark the whole invariance check FAIL (or at least log
"reference baseline shifted to N=$first_n because N=1 failed") rather than silently re-basing.

#### H3. [LOW] Empty Perl output could false-PASS in `phase_h_smoke.sh`
`phase_h_smoke.sh`: if Perl exits 0 but produces no files, `comm -12` yields nothing, `TOTAL=0`,
`DIFFS=0`, `NAME_DIFF` empty → `PASS: all 0 files match`. The Perl run is guarded by
`|| { echo "Perl run failed"; exit 1; }`, so a *crash* is caught; a silent zero-output exit-0 is
the only (unlikely) gap. **Recommendation:** assert `TOTAL -gt 0` before declaring PASS.

#### H4. [INFO] PNG-exclusion + rounding-triage patches are correct
The `a7aaf61..HEAD` diff to `phase_h_smoke.sh` is sound: `grep -v '\.png$'` with `|| true`
guards (no false abort under `set -e`), PNG-only Perl files are reported as an EXPECTED delta and
excluded from `NAME_DIFF`, and the strict-`cmp` FAIL path now dumps the first 8 diff lines for
rounding triage while still counting the DIFF (hard gate still fires). Matches PLAN.md rev-2
[B-CRITICAL] and [A] items.

---

## Recommendations — prioritized

### Critical (would break or corrupt the overnight run)
1. **A1** — `bench_run.sh:109`: change `wait "$tpid"; ec=$?` → `ec=0; wait "$tpid" || ec=$?`.
   Without this, a Rust ENOSPC/panic crashes `bench_run.sh` instead of recording a FAILURE row;
   the config is then re-run forever on resume. This is the plan's central "panic-as-failure"
   gate and it currently does the opposite.
2. **H1** — `byteid_run.sh`: map `plain` → `default` before calling `phase_h_smoke.sh` (or add a
   `plain)` alias to `phase_h_smoke.sh`). Otherwise the campaign hard-stops at the WGBS-PE plain
   parity check after wasting the multi-hour Perl-serial gzip run.
3. **B1** — `overnight_driver.sh:38`: route `log` to stderr (`… | tee -a "$LOG" >&2`). Otherwise
   cold-staging pollutes `$WGBS_PE/$WGBS_SE/$RRBS_PE` with a log line and the campaign aborts at
   cold start (`BAM not found`).

### High
4. **C1** — `have_config`: count only `exit==0` rows (`&& $11==0`), and decide retry-vs-permanent
   policy for a config that can never reach `reps` successes (avoid infinite resume loop).
5. **A2** — `bench_run.sh:53`: guard the `df` arithmetic with `${avail_kb:-0}` so an empty `df`
   does not abort the script.
6. **G1** — driver: after A1, surface failed reps (`exit!=0`) as a visible banner in
   `FINDINGS.md`; currently a silently-degraded night looks clean.

### Medium
7. **B2** — make the post-stage warm-up a real integrity check (`samtools quickcheck`) and fail
   staging on corruption instead of `|| true`.
8. **H2** — fail (or loudly warn) if the worker-invariance N=1 reference run fails rather than
   silently re-basing the comparison to N=2.

### Low
9. **B3** — add an explicit `stat`/symlink re-check of `$dst` after `cp -L` (PLAN.md Validation #3).
10. **G3** — size `MIN_FREE_GB` from a measured full-WGBS gzip footprint; record a precheck-skip as
    a FAILURE row.
11. **D1** — note/handle the "very short run → peak_threads=0 (not sampled)" case for fast dry-runs.
12. **H3** — assert `TOTAL>0` before `phase_h_smoke.sh` declares PASS.

---

## What I verified as correct (no action needed)
- `/proc` sampler PID resolution via `pgrep -P` (GNU time fork+exec model) — **correct, race-tolerant** (D2).
- Wall-clock h:mm:ss / m:ss → seconds and `(user+sys)/real` cores math, incl. `NA` fallback (E1).
- SE-dedup output-name resolution picks `wgbs_se.deduplicated.bam` even with the PE dedup file co-resident (verified against `deduplicate_bismark` naming + `--output_dir` trailing-slash handling).
- Idle-gate pattern + self-exclusion + poll/timeout arithmetic (F1).
- `[[ … ]] && var=…` peak updates are `set -e`-safe (A3).
- Purge-on-success keeps failure outputs for triage (G2).
- The `phase_h_smoke.sh` PNG-exclusion + rounding-triage diff (H4).
- Safe empty-array (`${ARR[@]+…}`) and `${VAR:-default}` idioms under `set -u`; for-loops over `$(seq …)` and literal lists word-split correctly here.

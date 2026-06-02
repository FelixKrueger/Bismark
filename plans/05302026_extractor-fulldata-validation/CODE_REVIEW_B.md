# Code Review B — Full-dataset benchmark harness (extractor-fulldata-bench)

**Reviewer:** B (independent, fresh context)
**Date:** 2026-05-30
**Target:** `scripts/bench_run.sh`, `scripts/byteid_run.sh`, `scripts/oxy_idle_gate.sh`,
`scripts/overnight_driver.sh`, and the `phase_h_smoke.sh` rev-2 diff (`git diff a7aaf61`).
**Worktree:** `/Users/fkrueger/Github/Bismark-extractor` @ `extractor-fulldata-bench`
**Plan:** `plans/05302026_extractor-fulldata-validation/PLAN.md`
**Mode:** RECOMMEND ONLY — no files edited (live scripts, sibling reviewer + overnight campaign reading them).

---

## Summary / verdict

The harness is well structured and most of the tricky machinery is correct: GNU `time -v`
field parsing, the h:mm:ss/m:ss→seconds conversion, the per-mode `(user+sys)/real` cores math,
the `df -Pk` precheck arithmetic, the PNG-exclusion / rounding-triage patches in
`phase_h_smoke.sh`, the idle-gate pattern (matches siblings, excludes the campaign's own
scripts and the extractor binary), the serial-anchor grep (format + path both match), and the
Rust-vs-Rust worker-invariance md5 loop all behave as intended under test.

**However, I found three Critical bugs that would each independently break or silently corrupt the
overnight run, plus one High resumability gap.** All three Criticals fire on the *first cold launch*
or on *any failed rep* — i.e. exactly the unattended overnight path — and were not exercised by the
dry-run (which used pre-staged inputs, `--mode gzip` only, and never hit a failing rep). I recommend
the run NOT be trusted until C1–C3 are fixed.

Most important findings:

- **[C1] `wait "$tpid"; ec=$?` dies under `set -e` on a non-zero child** — the panic-as-failure
  requirement is defeated; a Rust ENOSPC panic kills `bench_run.sh` instead of recording a FAILURE row.
- **[C2] `log()` pollutes the captured staged path via command substitution** — on the cold run,
  `$WGBS_PE/$WGBS_SE/$RRBS_PE` become multi-line garbage, breaking every downstream use.
- **[C3] `--mode plain` is not a valid `phase_h_smoke.sh` mode** — WGBS-PE Phase-1 plain parity
  exits 2 → driver treats it as a genuine byte-identity FAIL → HARD-STOPS the whole campaign before perf.
- **[H1] Phase 1 is NOT resumable** — a re-run re-executes all three multi-hour Perl `--multicore 1`
  serial runs from scratch and double-appends the serial-anchor rows; only Phase 2 is resumable.
- **[M] `have_config` counts failed reps as completed**, and a mid-config crash leaves stale rows
  that pollute later medians.

---

## Critical (would break or corrupt the overnight run)

### C1 — `bench_run.sh`: `wait "$tpid"; ec=$?` is killed by `set -e` on a non-zero child (panic-as-failure defeated)

`bench_run.sh` line 109:
```bash
wait "$tpid"; ec=$?
```
The script runs under `set -euo pipefail` (line 24). `wait` is the **head of a `;`-separated list**,
not the condition of an `if`/`while`/`&&`/`||`, so when the child exits non-zero, `set -e` aborts the
script *immediately at the `wait`* — `ec=$?` never runs, the FAILURE CSV row is never written, and the
rep loop never continues.

Verified:
```
$ wait "$tpid"; ec=$?       # child exited 3, set -e on
# script dies, "REACHED" never prints, outer exit=3
$ ec=0; wait "$tpid" || ec=$?   # fix
# REACHED: captured ec=3 ; outer exit=0
```

**Impact (directly contradicts PLAN §Validation 9, §Implementation 1, §rev-2 ENOSPC bullet):** a Rust
gzip ENOSPC `.unwrap()` panic (the exact #889 scenario the precheck is meant to backstop) exits
non-zero → `bench_run.sh` dies on the first failed rep → **no FAILURE row is recorded** (silently
"lost" rather than "recorded as FAILURE"), the remaining reps of that config never run, and the
config is left partially written. The plan's headline guarantee — "treat a non-zero exit / panic as a
run FAILURE (don't record bogus timing)" — is inverted: it records *nothing*. Any genuinely failing
config (OOM, ulimit -u thread cap, disk) behaves this way.

**Recommended fix:**
```bash
ec=0; wait "$tpid" || ec=$?
```

---

### C2 — `overnight_driver.sh`: `log()` inside `stage_local` corrupts the captured BAM path (cold-run only)

`stage_local()` returns the staged path on **stdout** via `echo "$dst"` (line 64), and the result is
captured: `WGBS_PE=$(stage_local ...)` (lines 78-80). But `stage_local` also calls `log()` (lines 50,
51, 56, 57), and `log(){ echo ... | tee -a "$LOG"; }` (line 38) writes to **stdout**. Under command
substitution, those `tee`-to-stdout log lines are captured into the path variable.

On a **cold run** (no `$dst` yet → line 51 `log "staging ..."` fires; SE also fires line 56
`log "deduplicating ..."`), the captured value becomes multi-line, e.g.:
```
[2026-05-30T...Z staging wgbs_pe.bam (cp -L from S3)…
/home/.../staged/wgbs_pe.deduplicated.bam]
```
Verified with a reduced repro: `[[ -f "$WGBS_PE" ]]` → "POLLUTED". (`cp -L` itself correctly
dereferences the S3 symlink — that part is fine.)

**Impact:** the entire campaign is launched cold and unattended. With a polluted `$WGBS_PE` etc.:
- line 81/83 `samtools view -c "$b"` gets a multi-line/garbage path → error;
- `byteid_run.sh "${DS_BAM[$ds]}"` receives a corrupt BAM arg → `-f` guard fails → exit 2 → driver
  HARD-STOPS as a "byteid FAIL" (line 99-101). The night is wasted at STAGE/Phase-1.

This is masked on *re-runs* (when `$dst` already exists no `log` fires, so the captured path is
clean) and on the dry-run (pre-staged), which is why it slipped through. It WILL bite the first real
overnight launch.

**Recommended fix:** send `log()` output to stderr (or to the log file only), e.g.
`log(){ echo "$(date -u ...) $*" | tee -a "$LOG" >&2; }`, OR make `stage_local` log to stderr and
reserve stdout for the path. Sending all human-facing logging to stderr is the standard fix and also
keeps `tee` console output intact. (Note: `console.log` is captured at the tmux/redirect level per
the PLAN notes, so stderr still lands in the campaign log.)

---

### C3 — `byteid_run.sh` → `phase_h_smoke.sh`: `--mode plain` is undefined → HARD-STOPS the campaign

`overnight_driver.sh` line 95 sets `DS_MODES[wgbs_pe]="gzip plain"`. `byteid_run.sh` loops
`for mode in $MODES` (line 52) and invokes `phase_h_smoke.sh ... --mode "$mode"` (line 56). But
`phase_h_smoke.sh`'s mode dispatch (lines 137-145) defines `default | comprehensive | merge_non_CpG
| comprehensive_merge | gzip` — there is **no `plain` arm**; the no-extra-flags mode is named
`default`. `--mode plain` hits the `*)` arm → `echo "unknown mode: plain"; exit 2`.

**Impact:** `byteid_run.sh` sees that non-zero exit → "PARITY FAIL: wgbs_pe plain" → `FAIL=1` → exits
1 → `overnight_driver.sh` lines 97-101 interpret it as a **genuine full-data byte-identity FAIL** and
`exit 1` the whole campaign — **before any perf data is collected**. The PLAN explicitly requires
"WGBS-PE also plain" parity (Phase 1, step 4), so this path is mandatory and currently fatal. The
dry-run only ran `gzip`/`default`, so it was never exercised.

**Recommended fix (choose one):**
- In `byteid_run.sh`, translate `plain` → `default` before forwarding to `phase_h_smoke.sh`
  (e.g. `smode=$mode; [[ "$mode" == plain ]] && smode=default;` then pass `--mode "$smode"`), keeping
  `plain` as the campaign-facing label; **or**
- add a `plain) ;;` arm (no flags) to `phase_h_smoke.sh`'s mode case as a synonym for `default`.

The first is preferable (doesn't touch the shared smoke harness another sub-issue depends on). Note
`bench_run.sh` correctly maps `plain` → empty `MODE_FLAGS` (its own dispatch, lines 64-69), so only
the byteid→smoke path is affected.

---

## High

### H1 — `overnight_driver.sh`: Phase 1 is not resumable; re-run repeats the multi-hour Perl serial runs and double-appends anchor rows

The PLAN's central efficiency claim is "CSV-append + skip-completed ⇒ safe to re-run … resumability
banks every completed config." That is true for Phase 2 (each `run_cfg` calls `have_config`), but
**Phase 1 (lines 96-105) has no skip logic.** On any re-run after a crash, `byteid_run.sh` is invoked
unconditionally for all three datasets, and `byteid_run.sh` itself unconditionally re-runs Perl
`--multicore 1` (the 1–3 h long pole, per PLAN §Efficiency) and the full Rust `{1,2,4,8,16}` sweep
for every dataset.

Consequences of a re-run (e.g. after a Phase-2 crash, oxy reboot, or `--skip-gate` resume):
1. Up to **three multi-hour Perl serial runs are redone from zero** — the single most expensive part
   of the campaign — even though Phase 2 then skips its completed configs.
2. The serial-anchor append at line 104 has **no `have_config` guard**, so each re-run appends another
   `perl,$ds,gzip,1,1,$ps,...` row → duplicate anchor rows accumulate in the CSV and skew any
   per-config median that includes parallel=1 Perl.

**Recommended fix:** gate Phase 1 on a status file or on the presence of a passing
`byteid_<ds>.status` (e.g. `grep -q '^BYTEID PASS' "$OUT_DIR/byteid/byteid_${ds}.status" && skip`),
and guard the anchor append with `have_config perl "$ds" gzip 1 1 || echo ... >> "$CSV"`. At minimum,
guard the anchor append so re-runs don't duplicate it.

---

## Medium

### M1 — `overnight_driver.sh`/`bench_run.sh`: `have_config` treats failed reps as "done"; a mid-config crash leaves stale rows

`have_config` (lines 68-73) counts **all** CSV rows matching tool/dataset/mode/parallel and returns
true at `>= reps`, regardless of the `exit` column. Verified: a 3-rep config where rep3 has `exit=1`
yields 3 rows → `have_config` returns true → the config is skipped on resume even though a rep failed
(no good replication for that cell).

Worse, in combination with **C1**: when `bench_run.sh` dies mid-config (which C1 guarantees on the
first failing rep), it writes *fewer* than `reps` rows. `have_config` then returns false → the config
re-runs and **appends a fresh rep1..repR**, leaving the earlier partial rows as stale duplicates that
pollute the Phase-3 medians (which the PLAN computes by hand off this CSV).

**Recommended fix:** count only successful reps in `have_config`
(`$1==t&&$2==d&&$3==m&&$4==p&&$11==0{n++}`) so failed/partial cells are re-attempted; and on resume,
de-duplicate or clear prior rows for a config before re-running it (e.g. rewrite the CSV without that
config's rows, or have `bench_run.sh` refuse to append if rows already exist). Fixing C1 first
removes the partial-write case; the failed-rep-as-done case still needs the `$11==0` guard.

### M2 — `byteid_run.sh`: worker-invariance is asymmetric — a file dropped at higher `--parallel` is not detected

The sweep loop (lines 69-88) iterates only over files **present in the current N's** output dir. A
file that exists at N=1 but is **missing** at N=8 is never compared → silent pass. (The reverse — a
file present at N=n but absent from the N=1 reference — is caught via the `MISSING` sentinel.) For a
true worker-count-invariance gate, a dropped output file at higher parallelism is a real regression
and should FAIL.

**Recommended fix:** after the sweep, assert each N's file *set* equals N=1's set (compare the
captured `REF_MD5` keys against the current dir's basenames), or accumulate the per-N basename list
and diff the sets.

### M3 — `bench_run.sh`: `df` precheck has no guard for empty output (arithmetic abort under `set -e`)

Line 53 `AVAIL_GB=$(( $(df -Pk "$OUT_DIR" ... | awk 'NR==2{print $4}') / 1024 / 1024 ))`. If `df`
ever returns no data line (race, OUT_DIR transiently absent), the inner substitution is empty and the
`$(( / 1024 / 1024 ))` throws `arithmetic syntax error` → `set -e` aborts the config (verified). In
normal operation `$OUT_DIR/perf` exists (driver pre-creates it), so impact is low, but the precheck
is supposed to be the *robust* backstop. **Recommended:** capture into a var first and default to 0,
e.g. `avail_kb=$(df -Pk "$OUT_DIR" 2>/dev/null | awk 'NR==2{print $4+0}'); avail_kb=${avail_kb:-0}`.

---

## Low

- **L1 — `bench_run.sh` Perl peak_threads/peak_fds undercount.** The sampler reads only
  `/proc/$cpid/{task,fd}` for the single PID resolved via `pgrep -P "$tpid"`. Perl `--multicore N`
  uses *forked child processes*, which are not threads of the master and (for re-exec'd children) may
  not even be `time`'s grandchildren visible there. So Perl-anchor rows report misleadingly low
  threads/fds. Wall and cores remain valid (GNU `time -v` folds waited-children rusage into the
  master totals). Footprint analysis targets the Rust thread model, so this is cosmetic — but the
  FINDINGS footprint table should annotate that Perl threads/fds are not meaningful, to avoid a
  reader mistaking "Perl threads=1" for a real measurement.

- **L2 — `bench_run.sh` cpid-resolution timeout on very fast runs.** The `pgrep -P` retry loop is
  50×0.05 s = 2.5 s (line 97). A config finishing in <~2.5 s would leave `cpid` empty → sampler
  skipped → threads/fds recorded as 0. Irrelevant for full-data (minutes-long) runs; could surface in
  any future short-input smoke. Consider falling back to recording threads/fds as `NA` rather than
  `0` when `cpid` never resolves, so a real 0 is distinguishable from "not sampled."

- **L3 — `byteid_run.sh` reference is "first sweep value," not literally N=1.** Lines 79/84 capture
  `REF_MD5` from the first token of `$SWEEP`. The comment and intent say "to N=1," but if a caller
  passes `--sweep "2 4 8"` the reference silently becomes N=2. The driver always passes `"1 2 4 8 16"`
  so this is latent only. Optional: assert the sweep begins with `1` or rename the comment.

- **L4 — `overnight_driver.sh` FINDINGS wall aggregation includes failed reps.** The Phase-REPORT
  awk (lines 139-140) lists `$6` (wall_s) for every row regardless of `$11` (exit). A failed rep's
  bogus/`NA` wall is shown in the "reps:" list. The footprint table does surface `exit`, and Phase 3
  computes medians by hand, so a human can filter — but excluding `$11!=0` from the wall aggregation
  would prevent a misread. Optional.

- **L5 — `overnight_driver.sh stage_local` uses `$2` instead of `$name` for `dst`** (line 49:
  `dst="$STAGE/$2"`). Harmless (both are the same value) but a readability nit; should be
  `dst="$STAGE/$name"`.

- **L6 — Dedup-name resolution relies on a grep substring** (line 60:
  `ls "$STAGE"/*deduplicated.bam | grep -i "${name%.bam}"`). Currently safe because `wgbs_se` does
  not substring-match `wgbs_pe`/`rrbs_pe`. If dataset basenames ever share a prefix this could pick
  the wrong file. The `head -1` + `echo "$dd"` fallback also masks a true dedup-name mismatch as a
  guess. Low risk given the current fixed names; consider constructing the expected name directly
  (`dd="$STAGE/${name%.bam}.deduplicated.bam"`) and asserting `-f "$dd"` rather than globbing.

---

## Things verified correct (no action needed)

- GNU `time -v` field parsing (`-F': '`): `User time`, `System time`, `Maximum resident set size`,
  `Elapsed (wall clock)` all parse correctly against the real format; the embedded `(h:mm:ss …)`
  colons don't break the `: ` split.
- Wall conversion (`bench_run.sh` line 114): `0:12.64`→12.64, `1:02:03`→3723, `m:ss`→correct; empty
  wall → empty → `cores` awk safely prints `NA` (verified).
- `(user+sys)/real` cores math (line 118): `w>0` guard prints `NA` on zero/empty wall; empty `usr`/`sys`
  treated as 0 (acceptable). Locale: GNU time emits `.`-decimals and awk parses floats fine on oxy.
- `df -Pk` arithmetic (line 53) in the normal case: `/1024/1024` GB conversion correct; GNU `df -P`
  guarantees a single data line.
- Thread/fd count comparisons (`[[ "$t" -gt "$peak_threads" ]]`) tolerate `wc -l` whitespace; the
  `ls | wc -l` pipe always yields a clean numeric (the `|| echo 0` is effectively dead but harmless).
- `pgrep -P "$tpid"` correctly resolves the binary as `/usr/bin/time`'s direct child (Rust path).
- Sampler loop exits cleanly: `while kill -0 "$tpid"` ends when `time` exits; the `kill -0` failure is
  a `while` condition so `set -e` is not tripped.
- `set -e` does NOT abort on the `[[ ... ]] && cmd` statement-level lists in `byteid_run.sh`
  (lines 84, 87) — verified.
- Idle-gate pattern (`oxy_idle_gate.sh`): matches the c2c `coverage2cytosine`(`_rs`) sibling and the
  other heavy jobs, excludes `grep`/the campaign's own scripts, and deliberately omits the extractor
  binary (the campaign's own workload). `load1<MAX_LOAD` awk float compare and the poll/timeout
  arithmetic are correct.
- Serial-anchor grep (driver line 103): `grep -E '^Perl: [0-9]+s$'` matches the smoke's exact
  `Perl: <int>s` line; the file path `$OUT_DIR/byteid/parity_${ds}_gzip/diff_summary.txt` matches
  where `byteid_run.sh` writes it.
- The serial-anchor row (parallel=1) doesn't collide with the Phase-2 Perl `--multicore 12`
  `have_config` check (distinct parallel).
- `phase_h_smoke.sh` rev-2 patches: PNG exclusion (`grep -v '\.png$'` with `|| true`), the codified
  Perl-only PNG delta report, and the strict-cmp rounding-triage `diff | head -8` dump are all
  correct and `set -e`-safe (each pipeline guarded).
- FINDINGS awk aggregation tolerates the `NA`-filled serial-anchor row.

---

## Recommended priority order for the fix pass

1. **C2** (path pollution) — without it the cold launch never gets past STAGE.
2. **C3** (`--mode plain`) — without it Phase 1 hard-stops the campaign.
3. **C1** (`wait`/`set -e`) — without it any failing rep silently aborts a config and loses the
   FAILURE record (defeats the whole panic-as-failure design).
4. **H1 + M1** (resumability + failed-rep accounting) — needed for the "safe to re-run" guarantee and
   clean medians.
5. M2/M3 and the Low items as time permits.

C1/C2/C3 are each independently fatal to an *unattended cold* run; I'd hold the campaign until at
least those three are fixed and re-dry-run on 10M with (a) a forced `--mode plain` byteid pass and
(b) a deliberately failing rep (e.g. tiny `--min-free-gb` or a `false` stub binary) to confirm the
FAILURE row is recorded.

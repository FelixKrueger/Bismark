# Phase E harness — Code Review (Reviewer B)

**Target:** `scripts/c2c_byte_identity_matrix.sh` + `RELEASE_CHECKLIST_c2c.md` (uncommitted).
**Worktree:** `/Users/fkrueger/Github/Bismark-c2c` on `rust/coverage2cytosine`.
**Reviewer:** B (independent; no coordination with Reviewer A).
**Mode:** RECOMMEND-ONLY (no tracked files edited).
**Date:** 2026-05-30.

---

## TOP-LINE VERDICT: **APPROVE-WITH-CHANGES**

The harness is **fail-CLOSED on the failure modes that matter most for this gate** and I verified it empirically:
the three mandatory self-tests pass (V12 exit 0, V1 byte-diff exit 1, V11 truncated-gz exit 1), the
empty-set vacuous-pass hole is plugged by the require-nonempty backstop, the `split` zero-file no-op is
caught by check (d), and the purge scope is confined to the per-cell `{rust,perl}` dirs. The rev-1
consensus Critical (the `cmp <(gzip -dc)` swallow) is genuinely closed by the `gzip -t` pre-check.

I found **no Critical false-PASS** that lets a real Rust≠Perl divergence slip through as exit 0 on the
in-scope streams. I found **one Important fail-open** (non-zero binary exit codes are recorded but never
gated) and a handful of Minor robustness/spec-fidelity gaps. None block the oxy run, but the exit-code
fail-open and the misleading exit code on a non-numeric `--disk-floor-gb` should be addressed before the
tag run trusts the harness blindly.

Counts: **Critical 0 · Important 1 · Minor 4 · Nit 2.**

---

## Validation performed (all on the `phase_b` fixture, bash 5.3.9, repo Perl v0.25.1)

| Test | Command essence | Result |
|------|-----------------|--------|
| **V12** full matrix | 9 cells on fixture, `--disk-floor-gb 1` | **exit 0**; 9/9 PASS; 7/7 differentials (cx=25>default=18, zero≠default, gzip==default, thr=2<default, merge non-empty, merge_gzip==merge, split=4>1) |
| **V1** 1-byte diff | wrapper appends `X` to Rust CpG report | **exit 1**, `BYTE-DIFF: c2c.CpG_report.txt` |
| **V11** truncated gz | wrapper `head -c5` the Rust CX `.gz` | **exit 1**, `GZIP-INTEGRITY FAIL` (NOT false-PASS) |
| Total Rust no-op (0 files, rc 0) | `--rust-c2c` empty stub | **exit 1**, file-name-set mismatch |
| **Both** sides 0 files | both stubs emit nothing | **exit 1**, "required output absent" (require-nonempty backstop) |
| Both sides empty CpG report | wrappers truncate report to 0 B | **exit 1**, "required output empty" |
| `split` both-empty | both stubs emit nothing, `--cells split` | **exit 1** (check (d) `SPLIT_FILE_COUNT<1`) |
| `merge_disc` empty merged-cov | real binaries | **exit 0** (correctly existence-only; discordant non-empty, merged-cov 0 B) |
| Non-zero rc, identical output | Rust wrapper runs real binary, `exit 1` | **exit 0** ← *Important finding* |
| Disk floor > free | `--disk-floor-gb 99999999` | **exit 2** at pre-flight |
| Non-numeric floor | `--disk-floor-gb foo` | **exit 1** (uncaught `set -u` abort) ← *Minor finding* |
| Wrong Perl version | stub prints `Version: v0.24.0` | **exit 2** |
| Empty cov input | truly-empty `.cov.gz` | **exit 1** (Rust omits summary; file-set mismatch caught it) |
| `--help`, `bash -n` | — | exit 0, syntax clean |
| Purge scope | full run, no `--keep-all` | only reports/`.cov*` removed under `cell_*/{rust,perl}`; summary kept; fixtures untouched |

---

## IMPORTANT

### I1 — Binary exit codes are recorded but never gated → fail-OPEN on a crashing-but-coincidentally-matching binary
**`scripts/c2c_byte_identity_matrix.sh:236-243, 325, 384-445`**

`run_cell` captures `prc`/`rrc` (Perl/Rust return codes) and prints them in the stderr log line
(`(perl_rc=$prc rust_rc=$rrc)`), but **the verdict never consults them.** A cell's verdict is derived
purely from file-name-set + byte-compare + require-nonempty. Confirmed empirically: a Rust wrapper that
runs the *real* binary (producing byte-identical output) and then `exit 1` yields:

```
perl=3s rust=0s (perl_rc=0 rust_rc=1) → PASS
Verdict: all cells byte-identical ... (exit 0)
```

**Why it matters (false-PASS surface on oxy):** the whole reason this gate exists is to "survive the oxy
disk-full failure mode" (PLAN §3.4.2). If a binary runs out of disk mid-write, the most likely outcome is
a *non-zero exit with a partially-written file*. For gz streams the `gzip -t` pre-check catches truncation,
but for **plain** streams (the `default`/`zero`/`thr`/`merge`/`merge_disc` reports and the always-plain
context summary) a binary that dies after writing a complete-looking file — or whose partial write happens
to match the other side's partial write — would PASS despite signalling failure. The sibling
`phase_h_se_matrix.sh` routes through `phase_h_smoke.sh`, which (per the `SMOKE_RC` plumbing at :228-244)
*does* map a non-zero smoke rc to `USAGE`; this harness drops that signal entirely. This is precisely the
class of "the script trusted a clean-looking output from a process that actually failed" that the
`count_mbias_rows` fail-open lesson warns against.

**Suggested fix:** treat `prc != 0` (and `rrc != 0`) as a hard FAIL — or at minimum a USAGE/exit-2 — unless
the cell is the documented empty-cov "Perl dies 255" case (which is out of scope for the gate anyway). E.g.
after the runs: `if (( prc != 0 || rrc != 0 )); then verdict="FAIL"; detail="${detail:+$detail; }non-zero exit (perl=$prc rust=$rrc)"; fi`. At a minimum, surface a loud warning in `matrix_verdict.txt` (currently the rc is only in the transient stderr stream, lost once the terminal scrolls).

---

## MINOR

### M1 — Non-numeric `--disk-floor-gb` aborts via `set -u` with the *wrong* exit code (1, not 2)
**`scripts/c2c_byte_identity_matrix.sh:60, 132-145`**

`DISK_FLOOR_GB="$2"` accepts any string; later `(( g < DISK_FLOOR_GB ))` (line 136) on a non-numeric value
errors `line 136: foo: unbound variable` under `set -u` and the script dies. Empirically this exits **1**,
which an operator reads as "byte-identity FAIL" rather than "you typo'd an argument." A usage error should
exit **2** (the script's own convention, and what `--cells bogus` / missing `--genome` correctly return).

**Suggested fix:** validate at parse time, e.g. in the arg loop or just after:
`[[ "$DISK_FLOOR_GB" =~ ^[0-9]+$ ]] || usage_err "--disk-floor-gb must be a non-negative integer: $DISK_FLOOR_GB"`.

### M2 — Perl `--version` regex is unanchored → `v0.25.10` / `v0.25.1-dev` false-accept
**`scripts/c2c_byte_identity_matrix.sh:111`**

`grep -qE 'Version: v0\.25\.1'` matches `Version: v0.25.10`, `v0.25.11`, `v0.25.1-dev`, etc. (verified). On
oxy this is unlikely to bite (v0.25.1 is the only release in that range today), and a wrong-but-close
version that behaves differently would FAIL the byte-compare rather than false-PASS the gate — so this is
not a false-PASS of byte-identity. But it weakens the "the contract assumes v0.25.1" pre-flight that the
comment (lines 105-107) advertises.

**Suggested fix:** anchor it — `grep -qE 'Version: v0\.25\.1([^0-9]|$)'` or `'Version: v0\.25\.1\b'`.
Note the two greps also match across *different lines* (one finds `coverage2cytosine`, the other finds the
version anywhere in the blob); acceptable, but tightening to a single-line match would be cleaner.

### M3 — PLAN §3.5 "only the last-chr summary non-empty" assertion is NOT implemented
**`scripts/c2c_byte_identity_matrix.sh:300-307` (split handler)**

PLAN §3.5 / V7 call for asserting that *only the last-processed chromosome's* context-summary is non-empty
(the Phase-C quirk; the rest empty on both sides). The harness does **not** assert this — it relies solely
on the per-file Rust≡Perl byte-compare. That compare *does* pin content (if Rust and Perl agree on which
summary is non-empty, fine), so there is **no false-PASS of byte-identity**. The residual gap is the
differential class: if *both* binaries regressed identically (e.g. both populated every per-chr summary, or
both emptied all of them), the byte-compare would still PASS and nothing flags the lost invariant. This is
the same "both no-op a flag" hole the §3.6 differentials exist to close, but it's missing for the split
summary quirk specifically. Low likelihood (Rust and Perl regressing in lockstep on this is implausible),
hence Minor, but it is a real deviation from the plan's stated split assertion.

**Suggested fix (optional):** add a cheap differential — count non-empty `*.cytosine_context_summary.txt`
in the split Perl dir and assert it equals 1; or document the deviation explicitly in the PLAN/Impl-notes
(D2) the way D1 is documented for the cx line-count.

### M4 — Hardcoded merged/discordant/report filenames are correct only because `-o c2c` is fixed internally
**`scripts/c2c_byte_identity_matrix.sh:173-183 (REQUIRE_NONEMPTY), 311-321 (stash)`**

The require-nonempty globs and the differential stash hardcode `c2c.CpG_report.merged_CpG_evidence.cov[.gz]`,
`c2c.CpG_report.txt[.gz]`, `c2c.CX_report.txt.gz`, etc. This is **correct** for the shipped binary (the
filenames are report-derived per rev-1 A-I1/B-C1, verified live: `c2c.CpG_report.merged_CpG_evidence.cov`),
but only because `run_cell` always invokes both binaries with the fixed stem `-o c2c` (lines 236, 241). The
coupling is silent: if `-o` were ever parameterised, every hardcoded name in `REQUIRE_NONEMPTY` and the
stash would break and the require-nonempty backstop (the thing that prevents the empty-set vacuous PASS,
see I1's relatives) would silently stop matching → vacuous PASS. Not a current defect; flagging the hidden
invariant. **Suggested fix:** a one-line comment at the `-o c2c` invocation noting that the hardcoded
filenames downstream depend on this exact stem.

---

## NIT

### N1 — SIGINT trap references `OUT_DIR` before it is canonicalised / before `--out` is parsed
**`scripts/c2c_byte_identity_matrix.sh:43-44, 95-103`**

The trap is armed at line 44 with the default `./c2c_byte_identity_out`. A Ctrl-C during arg-parse (before
`--out` is consumed) or before line 103 canonicalises it would print the *default* relative path even when
the user passed `--out /somewhere/else`, or a relative path the message claims is preserved. Cosmetic — the
evidence is wherever `mkdir -p` actually created it — but the message could mislead. Acceptable as-is.

### N2 — `now_s()` wall-clock has 1-second granularity; sub-second cells show `rust=0s`
**`scripts/c2c_byte_identity_matrix.sh:222, 234-243`**

`date +%s` gives integer seconds, so fast cells report `rust=0s` (seen throughout the fixture runs). Perf is
explicitly **not gated** (PLAN §3.8, SPEC §10.7), so this only affects the informational `perf_table.md`. On
the multi-hour oxy genome it's a non-issue. No action needed.

---

## Things I checked that are CORRECT (no finding)

- **gzip fail-open (rev-1 Critical):** closed. `gzip -t` on BOTH sides before decompress-compare; V11
  truncation → exit 1 (verified). The decompress-compare streams (plain CX never materialised). ✔
- **Empty-set vacuous PASS:** plugged. Both-empty / both-zero-files / both-empty-report all caught by the
  require-nonempty backstop or check (d). The require-nonempty list correctly matches SPEC §5 (report +
  summary always required; merged-cov required *only* in plain `merge`; discordant / merge_disc-merged-cov /
  split per-chr reports are existence-only). Verified `merge_disc` merged-cov legitimately 0 B → still PASS. ✔
- **Stash-before-purge ordering (rev-1 A-I4/B-I4):** airtight. Stash (lines 310-321) precedes purge (328-330);
  differentials read the stash, not the purged files. V12 reported correct counts post-purge. ✔
- **Deviation D1 (cx line-count via separate Perl decompress):** sound reasoning (avoids the
  `tee >(wc -l)` process-sub flush race) and counts the **right** file (`c2c.CX_report.txt.gz`, verified). ✔
- **Differential checks (§3.6):** all 7 fire correctly and non-spuriously on the fixture; gated on `ran` +
  non-empty stash so a cell subset doesn't false-FAIL. ✔
- **Purge `find -delete` scope:** confined to `$rdir`/`$pdir` (always under `$OUT_DIR/cell_*`); cannot reach
  fixture inputs or the genome; correctly preserves the context summary. ✔
- **bash 5 `set -euo pipefail` correctness:** empty-array expansions (`${DIFF_RESULTS[@]:-}`,
  `${REQUIRE_NONEMPTY[$name]:-}`) are guarded; `comm`/`sort` run under exported `LC_ALL=C` (bytewise,
  consistent); `set +e`/`set -e` brackets the binary invocations so a non-zero exit doesn't abort the loop. ✔
- **9 cell flag strings + mutex compliance:** correct — no `--CX`/`--split`/`--threshold` paired with
  `--merge_CpGs`; `cx` carries `--gzip` (disk); `merge_disc` adds `--discordance_filter 10`. ✔
- **Pre-flight gates:** bash≥4, cov `.gz` suffix + readable + canonicalised, genome four-suffix glob,
  `--out` empty-or-absent, Perl version, Rust build/locate, disk floor, `LC_ALL=C`, tmux advisory, trap. All
  exercised; wrong-version and disk-floor-exceeded both → exit 2. ✔

## Checklist (`RELEASE_CHECKLIST_c2c.md`) review

- §0 mandatory pre-trust self-tests (V12/V1/V11) are correct and I reproduced all three. The framing
  ("a green matrix is only trustworthy if it FAILS on a real diff") is exactly right and matches the
  dual-driver fail-open lesson. ✔
- §2 `bismark2bedGraph --CX -o ... CpG_context_* CHG_context_* CHH_context_*` recipe is the right
  independent-producer cov source (SPEC §13). The output name `sample.cov_input.bismark.cov.gz` matches the
  positional `<COV_GZ>` contract. ✔
- §5 disk fallback (chromosome-subset genome for `cx` only) is consistent with the harness's `--cells cx
  --genome <subset>` interface and PLAN Q1. ✔
- §6 tag step is correctly gated "only on a clean full-genome exit 0." ✔
- **One checklist gap (Minor, tracked under M1/M2 above):** the self-test block does not tell the operator to
  also confirm a *non-zero-exit-but-matching-output* case fails — i.e. it would not have surfaced I1. If I1
  is fixed, add a fourth self-test (V13: a wrapper that exits non-zero with correct output must FAIL).

---

## Summary for orchestrator

VERDICT: APPROVE-WITH-CHANGES. The gate is fail-CLOSED on every false-PASS path I could construct for the
in-scope streams; V1/V11/V12 reproduced green/1/1. One Important fail-open: non-zero binary exit codes are
recorded but never gated (a crashing-but-matching binary PASSes). Fix that + the non-numeric-floor exit-code
and the unanchored version regex before trusting an unattended oxy tag run.

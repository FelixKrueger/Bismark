# Code Review A — Phase 4 byte-identity gate extension (`c2c_byte_identity_matrix.sh` + `RELEASE_CHECKLIST_c2c.md`)

**Reviewer:** Code Reviewer A (independent; no shared state with Reviewer B)
**Date:** 2026-06-01
**Worktree:** `/Users/fkrueger/Github/Bismark-c2c` (branch `rust/c2c-v1x`, uncommitted working tree)
**Target:** `git diff scripts/c2c_byte_identity_matrix.sh RELEASE_CHECKLIST_c2c.md`
**Spec:** `plans/05312026_bismark-c2c-niche-modes/phase4-byte-identity-gate/PLAN.md` (rev 1)
**Mode:** recommend-only (no files edited except this report)

---

## TOP-LINE VERDICT: **APPROVE-WITH-CHANGES**

The harness extension is **correct and fail-CLOSED**. I built a `--CX`-style fixture and ran the
full new-cell matrix plus five independent fail-CLOSED probes (V1 + V2 + four no-op/abort probes):
**all passed**. Every one of the six new cells emits exactly the filenames the harness expects, the
`REQUIRE_NONEMPTY`/existence-only split is correct (incl. the NOMe-GpC existence-only downgrade and
the `ffs_nome` 0-byte `.NOMe.CpG.cov` correctly excluded), every new stash var is `""`-initialised
(no `set -u` abort), every stash is captured **before** the purge (survives purge-on-pass), and each
of the five differentials FAILS the matrix (exit 1) when its flag is silently no-op'd. The Perl source
confirms the suppression mechanic that diff #5 pins. **No fail-OPEN, no wrong require-nonempty name,
no stash-after-purge, no `set -u` abort.**

The one substantive change: the **`RELEASE_CHECKLIST_c2c.md` §0 mandatory pre-trust self-test
recipe is now stale** — as written (no `--cells`), it runs all 15 cells against the non-`--CX`
`phase_b` fixture and **FALSE-FAILS (exit 1)** on the `gc` + `drach` require-nonempty guards, which
would trigger the §0 "STOP — the gate is fail-open" instruction on a *correct* harness. The harness
itself is right; the documented self-test recipe must be updated to match the `--CX`-cov dependency
the same change introduces. (Medium.)

---

## Self-test results (empirical — all run via `/opt/homebrew/bin/bash` 5.3.9)

**Fixture:** a 51 bp `chrTest` genome with ACG/TCG CpGs (NOMe-pass), GCG/CCG CpGs (NOMe-drop),
six GpC dinucleotides, and a `GAACA` DRACH motif; a gzipped Perl-cov-shaped file covering every
C/G position. Perl oracle = repo-root `./coverage2cytosine` (v0.25.1); Rust = `rust/target/debug/coverage2cytosine_rs`.

| Probe | What | Result |
|-------|------|--------|
| **V1** | full matrix `--cells "default gc nome drach ffs ffs_cx ffs_nome"` on correct outputs | **exit 0**; 7/7 cells byte-identical; **all 5 differentials PASS** (`gc==default`; `nome=13 < default=18`; `drach standalone`; `ffs all10col=1 & 18==18`; `ffs_nome .NOMe.CpG.cov 0-byte both sides`) |
| **V1-purge** | same matrix WITHOUT `--keep-all` (real oxy purge-on-pass mode) | **exit 0**; all 5 differentials STILL PASS — confirms every stash is captured **before** the purge (the 0-byte `c2c.NOMe.CpG.cov` is itself purged as `*.cov`, so a post-purge stat would have fail-OPENed; the during-loop boolean does not) |
| **V2** | corrupt a NEW-cell Rust output (append a byte to `c2c.GpC_report.txt` + `c2c.NOMe.CpG_report.txt`) | **exit 1**, `FAIL [byte-diff: c2c.GpC_report.txt]` / `[byte-diff: c2c.NOMe.CpG_report.txt]` — new cells route through the per-cell byte-compare |
| **V3a** (diff #5) | un-suppress `ffs_nome` `.NOMe.CpG.cov` (append a row) on BOTH sides | **exit 1** via `FAIL: ffs_nome .NOMe.CpG.cov present-and-0-byte both sides` — cell still byte-PASSes (0+row==0+row would differ; here both filled identically and PASS the byte-compare), differential catches the un-suppression. **Genuine independent check.** |
| **V3b** (diff #4) | force the `ffs` report to 7 columns on BOTH sides | **exit 1** via `FAIL: ffs report 10-col ... (all10col=0 ...)` |
| **V3c** (diff #3) | make `--drach` also leak a `c2c.CpG_report.txt` on BOTH sides | **exit 1** via `FAIL: drach standalone (...)` |
| **awk all-lines** | unit-test `awk -F'\t' 'NF!=10{exit 1}'` on a 10-col-then-7-col file AND a 7-col-then-10-col file | both → `all10col=0` (correct) — checks ALL lines, robust to a degenerate first line (B-Opt-1 satisfied) |
| **V3d / V3e** | run `gc` only / `nome` only (no `default`) | **exit 0**, differentials cleanly `(none ran — cell subset)` — `ran default && [[ -n "$VAR" ]]` guards skip, no `set -u` abort |

Conclusion: V1 exit-0 + all 5 differentials confirmed; the fail-CLOSED probe (V2) and the
both-sides-no-op probes (V3a/b/c) all exit 1. The gate fails when it should.

---

## Findings by area

### Focus 1 — the 6 new cells + `REQUIRE_NONEMPTY` (verified live against Perl + Rust output)

**No issues.** I ran the Rust binary `-o c2c` with each flag and listed the emitted files; every
`REQUIRE_NONEMPTY` glob matches a real filename, and the existence-only classification is correct:

| cell | emitted (Perl) | `REQUIRE_NONEMPTY` | verdict |
|------|----------------|--------------------|---------|
| `gc` | `c2c.GpC_report.txt`, `c2c.GpC.cov`, `c2c.CpG_report.txt`, `…summary.txt` | all four | ✅ matches; all non-empty on a `--CX`-style cov |
| `nome` | `…NOMe.CpG_report.txt`, `…NOMe.CpG.cov`, `…NOMe.GpC_report.txt`, `…NOMe.GpC.cov`, `…summary.txt` | core report + `.NOMe.CpG.cov` + summary | ✅ NOMe **GpC** streams correctly **existence-only** (not listed) |
| `drach` | `c2c_DRACH_report.txt`, `c2c_DRACH.cov` ONLY (no CpG report/summary) | both | ✅ standalone shape; bare `.cov` (no `.gz` — `--drach` doesn't force gzip), `nonempty()` `-s` branch applies |
| `ffs` | `c2c.CpG_report.txt`, `…summary.txt` | both | ✅ |
| `ffs_cx` | `c2c.CX_report.txt.gz`, `…summary.txt` | both | ✅ (`--CX --gzip` → `.CX_report.txt.gz`, Perl L130) |
| `ffs_nome` | `c2c.NOMe.CpG.cov` **= 0 bytes**, `…NOMe.CpG_report.txt`, `…NOMe.GpC_*`, `…summary.txt` | core report + summary | ✅ the **suppressed 0-byte `.NOMe.CpG.cov` correctly EXCLUDED** — listing it would FALSE-FAIL a correct gate |

The Perl source confirms the suppression mechanic (the focus's worst-case): under `--ffs` (`$tetra`),
the `if ($tetra){ print CYT …10cols }` branch (`coverage2cytosine` L398-400 / L413-415) writes the
10-col report but **never prints to `CYTCOV`**, while `--nome-seq` still *opens* `CYTCOV` (L142/148)
→ present-and-0-byte. Verified live (0B both sides).

### Focus 2 — the 5 differentials + stash capture before purge

**No issues.** Each new stash var (`HASH_GC_CORE`, `LINES_NOME_CORE`, `DRACH_STANDALONE_OK`,
`FFS_ALL_10COL`, `LINES_FFS`, `FFSNOME_COV_EMPTY`) is (a) `""`-initialised at the declaration block
(L264), (b) captured in `run_cell`'s `case` (L385-402) **before** the purge (L410-412), (c) read in a
`ran <cells> && [[ -n "$VAR" ]]`-guarded `diff_check` (L467-486). The **V1-purge** run is the load-bearing
proof: with purge-on-pass active, all 5 differentials still PASS, which is only possible if the stash
predates the deletion.

`FFSNOME_COV_EMPTY` (the one the focus singled out) is implemented exactly as the rev-1 plan demands:
a **present-AND-0-byte-on-both-sides boolean** captured during the loop (L400-402: `-f` on both
`$pdir` and `$rdir` AND `! -s` on both), **not** a post-purge `stat`/`[[ -s ]]`. The V3a probe confirms
it fires correctly on un-suppression, and V1-purge confirms it does not fail-OPEN on the purged file.

### Focus 3 — fail-CLOSED-ness of each differential

**No issues.** Each differential exits 1 when both binaries silently no-op the flag (V3a/b/c proven
empirically; #1/#2 are equality/inequality on hashes/line-counts that flip on a no-op):
- #1 (`gc` core == default): correctly framed in the comment as a **regression check, NOT a no-op
  detector** (the "did `--gc` emit a GpC stream" job belongs to `REQUIRE_NONEMPTY[gc]`). This is the
  weaker of the five (it cannot detect a `--gc` that secretly does nothing to the *core*, because no-op
  is the expected pass-state), but it is not a fail-OPEN and the comment is honest about scope. Acceptable.
- #2 (`nome` lines `!=` AND `<` default): fail-CLOSED — a no-op (equal lines) trips the `!=` clause.
- #3 (`drach` standalone): `DRACH_STANDALONE_OK=1` iff DRACH report present AND no CpG report AND no
  summary — V3c proves a leaked CpG report → FAIL.
- #4 (`ffs` 10-col EVERY line): `awk -F'\t' 'NF!=10{exit 1}'` checks ALL lines (unit-tested both ways)
  AND `LINES_FFS == LINES_DEFAULT` — V3b proves a 7-col no-op → FAIL.
- #5 (`ffs_nome` 0-byte both sides): V3a proves un-suppression → FAIL.

### Focus 5 — `RELEASE_CHECKLIST_c2c.md`

The retitle, the stale-branch fix (`rust/coverage2cytosine` → `rust/c2c-v1x`, L64), the 15-cell list,
the require-nonempty/existence-only split, the 5 differentials, the `--CX`-cov dependency (L9-12),
and the v1.x tag + `→ iron-chancellor` merge are all present and accurate. **One defect (M-1 below).**

---

## Action items (prioritized)

### Critical
- **(none)**

### High
- **(none)**

### Medium
- **M-1 — the §0 mandatory pre-trust self-test recipe in `RELEASE_CHECKLIST_c2c.md` (L26-44) is now
  stale and FALSE-FAILS on a correct harness.** As written, the V12 command runs `scripts/c2c_byte_identity_matrix.sh
  /tmp/in.bismark.cov.gz --genome "$G" --out /tmp/c2c_self_ok --disk-floor-gb 1` with **no `--cells`**,
  so it now runs **all 15 cells** (the change adds 6) against the **non-`--CX` `phase_b` fixture**. I ran
  it: **exit 1**, with `gc FAIL [required output empty: c2c.GpC_report.txt; c2c.GpC.cov]` and
  `drach FAIL [required output empty: c2c_DRACH_report.txt; c2c_DRACH.cov]` — purely the
  require-nonempty guard tripping (the per-cell byte-compares PASS; all 12 differentials PASS). This is
  the *documented* `--CX`-cov dependency (L9-12) biting the documented self-test fixture. Per L44, a
  user following §0 literally would hit this and **STOP, believing the gate is fail-open** — a false
  alarm on a correct harness. **Fix (pick one):** (a) restrict the §0 V12 command to the cells the
  `phase_b` cov supports — `--cells "cx default zero gzip thr split merge merge_disc merge_gzip nome ffs ffs_cx ffs_nome"`
  (drops `gc`/`drach`); or (b) ship a tiny `--CX`-derived self-test fixture (like the implementer's own,
  per the Implementation notes) and point §0 at it; or (c) add an explicit note in §0 that, on the
  non-`--CX` `phase_b` fixture, `gc`/`drach` are *expected* to require-nonempty-fail and the self-test
  must therefore be scoped with `--cells`. The implementer's local self-test used a hand-built `--CX`
  fixture with `--cells` restricted to the new cells, so the documented §0 recipe was never exercised
  with the new cells included.

### Low
- **L-1 — `drach`/`gc` require-nonempty depend on the gate cov being `--CX` AND on the real genome
  having covered DRACH/GpC positions (Assumption 8c/8d).** This is correctly documented (PLAN §8,
  checklist L9-12) and is fine for full hg38, but it is the reason M-1 happens. No code change; called
  out so the M-1 fixer understands the root cause rather than weakening the guard. Do NOT downgrade the
  `gc`/`drach` require-nonempty to existence-only to "fix" M-1 — that would weaken the real-data gate;
  fix the self-test recipe instead.
- **L-2 — diff #1's no-op-detection value is limited** (see Focus 3). Not actionable — the comment
  already documents it as a regression check, and `REQUIRE_NONEMPTY[gc]` covers the "GpC stream
  produced" assertion. Noted only so a future reader doesn't over-trust it.

---

## Summary

**Verdict: APPROVE-WITH-CHANGES. Critical: 0, High: 0** (Medium: 1, Low: 2).

The harness extension is correct and genuinely fail-CLOSED: I built a `--CX`-style fixture, ran the
full new-cell matrix (V1 → exit 0, all 5 differentials PASS), a byte-corruption probe (V2 → exit 1),
and four no-op/abort probes (V3a-e) — every differential exits 1 when its flag is silently no-op'd,
the stashes survive purge-on-pass (V1-purge proves capture-before-purge), no `set -u` abort, and every
new-cell filename + the `REQUIRE_NONEMPTY`/existence-only split (incl. the `ffs_nome` 0-byte cov
exclusion and the NOMe-GpC existence-only downgrade) is verified against live Perl + Rust output. The
Perl source confirms the `--ffs`-suppresses-`CYTCOV` mechanic that diff #5 pins.

**The single most important finding (M-1):** the `RELEASE_CHECKLIST_c2c.md` §0 mandatory pre-trust
self-test recipe is now stale — run as written (no `--cells`) it executes all 15 cells against the
non-`--CX` `phase_b` fixture and FALSE-FAILS (exit 1) on the `gc`/`drach` require-nonempty guards,
which would trigger the §0 "STOP — gate is fail-open" instruction on a *correct* harness. Fix by
scoping the §0 command with `--cells` (excluding `gc`/`drach`), or by pointing it at a `--CX` fixture.
The harness code is correct; only the documented self-test recipe needs to catch up to the
`--CX`-cov dependency the same change introduces.

# Phase E harness — Code Review (Reviewer A)

**Target:** `scripts/c2c_byte_identity_matrix.sh` + `RELEASE_CHECKLIST_c2c.md` (uncommitted).
**Contract:** `phase-e-byte-identity-gate/PLAN.md` (rev 1), `SPEC.md` §12.3/§5/§13.
**Reviewer:** A (independent; no coordination with Reviewer B). **Recommend-only — no tracked file edited.**
**Interpreter:** `/opt/homebrew/bin/bash` 5.3.9. **Worktree:** `/Users/fkrueger/Github/Bismark-c2c`.

---

## TOP-LINE VERDICT: APPROVE-WITH-CHANGES

The harness is **fail-CLOSED on the failure modes that matter for a release gate**: I empirically
confirmed it FAILs (exit 1) on a byte-diff, a truncated gz (`gzip -t` catches it), a valid-but-different
gz, a missing required output, an empty-on-one empty-tolerant stream, and a Rust crash with no output;
and it exits 2 on disk-floor, non-gz input, unknown cell, and (by inspection) wrong Perl version. V12
passes (exit 0) on the `phase_b` fixture with all 9 cells byte-identical and all 7 differentials satisfied.
The core comparison logic has **no false-PASS hole** that I could construct.

However, there is **one real robustness defect (Important)**: a no-match `*.CpG_report.txt` glob in the
`split` cell handler aborts the *entire matrix* mid-run under `set -euo pipefail`, before any verdict/summary
file is written and before remaining cells run. It is fail-CLOSED (exit 1, never a false PASS) but it
destroys the evidence trail and the "partial output preserved" promise on a multi-hour oxy run. Plus a
couple of Minors (cx-not-first for custom `--cells`; documented but un-enforced split last-chr-summary rule).
None is a false-PASS risk; fix the Important one before the long oxy run so a split-cell regression yields
a diagnosable FAIL instead of a bare abort.

Counts: **Critical 0 · Important 1 · Minor 3 · Nit 2.**

---

## Important

### I1 — `split` empty-glob aborts the whole matrix under `set -euo pipefail`; no verdict/evidence written
**File:** `scripts/c2c_byte_identity_matrix.sh:303**
```bash
SPLIT_FILE_COUNT="$(cd "$pdir" && ls -1 *.CpG_report.txt 2>/dev/null | wc -l | tr -d ' ')"
```
**Defect.** `nullglob` is OFF here (turned on only for the genome FASTA glob at :89-91, then `shopt -u`).
When `$pdir` contains **zero** `*.CpG_report.txt` files, the glob passes the literal `*.CpG_report.txt`
to `ls`, which exits 1. Under `set -o pipefail` the whole pipeline reports failure, so the
command-substitution-into-assignment fails, and under `set -e` **the script aborts immediately** — before
`if [[ "${SPLIT_FILE_COUNT:-0}" -lt 1 ]]` ever runs, before the cell verdict is recorded, before the
post-loop verdict/summary/perf files are written, and before any later cell runs.

**Reproduced (bash 5.3.9):**
- Isolated: `X="$(ls -1 *.nomatch 2>/dev/null | wc -l | tr -d ' ')"` under `set -euo pipefail` → script exits 1 at the assignment (never echoes `X`). `ls` rc=1 on no-match; pipefail propagates it.
- In-harness, `--cells split` with both binaries stubbed to crash (no output): `EXIT=1`, but **`matrix_verdict.txt` was NOT written** (`cell_split/` only).
- In-harness, `--cells "default split merge"` with both binaries crashing **only** on `--split_by_chromosome`: `default` ran, `split` aborted the script, **`merge` never ran**, **no `matrix_verdict.txt`/summary/perf** written. Bare `exit 1` with the last line being `==> cell 'split'`.

**Why it matters for the oxy run.** The trigger on real hg38 is a `split`-cell regression that empties the
Perl OR Rust output dir (a crash, an OOM, a disk-full mid-`split`, or a Rust bug that drops all per-chr
files). The exit code is correct (1, not a false PASS), but the operator gets **zero diagnostics**: no
`diff.txt`, no `matrix_verdict.txt`, no per-cell summary — and on the documented full run `split` is cell 6
of 9, so the run also **loses the `merge`/`merge_disc`/`merge_gzip` verdicts** and the whole evidence trail
after a multi-hour walk. This directly contradicts the script's own SIGINT trap promise ("partial matrix
output … preserved for evidence") and the PLAN's fail-CLOSED-*with-evidence* intent.

**Suggested fix** (any one):
```bash
# find exits 0 on no match:
SPLIT_FILE_COUNT="$(find "$pdir" -maxdepth 1 -name '*.CpG_report.txt' -type f 2>/dev/null | wc -l | tr -d ' ')"
# or scope nullglob + a no-pipefail count:
SPLIT_FILE_COUNT="$( shopt -s nullglob; set -- "$pdir"/*.CpG_report.txt; echo $# )"
# or tolerate the no-match ls:
SPLIT_FILE_COUNT="$(cd "$pdir" && ls -1 *.CpG_report.txt 2>/dev/null | wc -l | tr -d ' ' || true)"
```
The `find` form is cleanest and matches the existing `find … -delete` style at :329.

---

## Minor

### M1 — `cx`-first invariant is silently broken for custom `--cells` orderings
**File:** `scripts/c2c_byte_identity_matrix.sh:186-194`
The subset loop appends cells in the **user's** argument order, not the canonical `ALL_CELLS` order:
```bash
for want in $CELLS_ARG; do
  for c in "${ALL_CELLS[@]}"; do [[ "${c%%|*}" == "$want" ]] && CELLS+=("$c"); done
done
```
**Reproduced:** `--cells "default cx"` runs `default` **before** `cx`. The help text says "cx runs first"
and PLAN §3.7 makes cx-first a *disk-discipline* requirement ("the heavy `cx` cell FIRST so it runs against
maximum free space"). The checklist §3 even advertises `--cells "cx default merge"`. For the default full
run cx *is* first (it leads `ALL_CELLS`), so the gate-as-documented is fine — but the `--cells` feature can
silently violate the documented invariant and starve `cx` of headroom on oxy.
**Fix:** iterate `ALL_CELLS` as the outer loop and select those in `$CELLS_ARG` (preserves canonical order,
cx first), or explicitly sort the selected set so `cx` leads.

### M2 — Split "only last-chr summary non-empty" rule (PLAN §3.5) is not independently asserted
**File:** `scripts/c2c_byte_identity_matrix.sh:300-307`
PLAN §3.5 calls for asserting "only the last-processed chromosome's summary is non-empty … the rest empty
on both sides." The harness does **not** check this directly; it relies on the per-file Rust≡Perl byte-compare
(b) to transitively catch any divergence. In practice this is adequate (if Rust emits a non-empty summary on
the wrong chr, (b) flags a byte-diff vs Perl's empty file), and the code comment at :300-301 acknowledges the
reliance. Flagging only as a *plan-vs-implementation* gap: the property is enforced **only against Perl**, not
as an independent invariant, so a hypothetical Perl-side change to the quirk would pass unnoticed. Acceptable
for v1.0; document the reliance in the verdict notes or add a one-line assert if cheap.

### M3 — Disk re-check measures `$OUT_DIR` only; retained FAIL evidence under `--keep-all` is the stated risk but `df` granularity is coarse
**File:** `scripts/c2c_byte_identity_matrix.sh:132-145, 341-347`
`free_gb()` uses `df -Pk "$OUT_DIR" | awk 'NR==2 {print int($4/1024/1024)}'`. This is correct and conservative
(integer-truncates GiB). The per-cell re-check (:342) is present and cx is first. Confirmed exit 2 when floor
> free. One caveat: the floor is checked **before** each cell but the heavy `cx` cell can itself consume tens
of GB *during* the run (peak = Perl + Rust CX simultaneously), and there is no mid-cell abort — only a
pre-cell gate. PLAN §6 accepts this (gzip + stream-compare keep peak ~2×gz), but if oxy is near the floor a
single cell could still exhaust disk mid-write. Low likelihood given the mitigations; surfacing because the
gate's whole reason to exist is surviving oxy disk pressure. No code change required; the Q1 subset-genome
fallback in the checklist is the right escape hatch.

---

## Nit

### N1 — Cosmetic double-print possible in the differential summary
**File:** `scripts/c2c_byte_identity_matrix.sh:406-407`
When no differentials ran, :406 prints "(none ran — cell subset)" and the `for d in "${DIFF_RESULTS[@]:-}"`
loop at :407 expands `:-` to one empty element that `[[ -n "$d" ]]` then filters. Harmless; the guard works.

### N2 — `--version` grep is order-independent across the multi-line banner (intended, worth a comment)
**File:** `scripts/c2c_byte_identity_matrix.sh:110-116`
The two `grep -q` checks (`coverage2cytosine` and `Version: v0\.25\.1`) run over the whole `$PERL_VERS_OUT`
independently, so they'd also pass if the two strings appeared on unrelated lines. Verified against the real
Perl banner — both strings are present and the assertion correctly accepts v0.25.1 (confirmed: banner shows
"coverage2cytosine" + "Version: v0.25.1"). Not exploitable in practice; a `grep -Pzo`/single-line check would
be marginally tighter but unnecessary.

---

## What I verified empirically (harness runs, `/opt/homebrew/bin/bash` 5.3.9, `phase_b` fixture)

| Scenario | Construction | Result | Verdict-correctness |
|---|---|---|---|
| **V12** full matrix | fixture cov.gz + phase_b genome, `--disk-floor-gb 1` | exit **0**, 9/9 byte-identical, 7/7 differentials | ✅ correct PASS |
| **V11** truncated Rust CX `.gz` | wrapper truncates rust gz post-run | exit **1** `gzip-integrity failed` | ✅ fail-CLOSED (no false PASS) |
| Valid-but-different gz | wrapper appends a line + re-gzips | exit **1** `byte-diff (gz)` | ✅ decompress-compare leg works |
| Rust crash, no output | stub exits 101 | exit **1** `file-name-set mismatch` | ✅ |
| **Both** crash, no output (`default`) | stub both sides | exit **1** `required output absent` | ✅ require-nonempty closes it |
| **Both** crash, no output (`split`) | stub both sides | exit **1** but **no verdict file** | ⚠️ I1 (silent abort) |
| Crash only on `split` in `default split merge` | stub both on `--split` | exit **1**, `merge` never ran, no verdict | ⚠️ I1 |
| Empty-on-one merged-cov (`merge_disc`) | Rust merged-cov made non-empty vs Perl empty | exit **1** `byte-diff` | ✅ empty-on-one ⇒ FAIL |
| merge_gzip merged-cov.gz truncated | wrapper truncates | exit **1** `gzip-integrity failed` | ✅ gz integrity reached for merge cell |
| Disk floor > free | `--disk-floor-gb 999999999` | exit **2** | ✅ |
| Non-gz input | `plain.cov` | exit **2** | ✅ |
| Unknown `--cells bogus` | — | exit **2** | ✅ |
| `--cells "merge"` | — | matches **only** `merge` (exact, not prefix) | ✅ |
| `--cells "default cx"` ordering | — | `default` ran before `cx` | ⚠️ M1 |

**Bash-landmine checks (passed):** unquoted `$flags` expands to *nothing* when empty (no spurious empty arg)
and word-splits correctly when multi (`--CX --gzip`); `comm -12` inputs are `ls -1 | sort` under `LC_ALL=C`
(sorted, consistent ordering); the gz path runs `gzip -t` on **both** sides *before* the decompress-compare
on every gz file (cx, gzip, merge_gzip — all confirmed); differential inputs are stashed (:310-321) **before**
purge-on-pass (:327-330); the `default`/`merge` reference hashes survive purge (V12 ran with purge ON and the
gzip==default + merge_gzip==merge differentials still passed). The only `ls -1 *glob*`-in-`$( )` in the file
is the I1 site (:303); :253-254 use globless `ls -1` (safe on empty dirs) and :329 `find … || true`.

## Checklist accuracy
- §0 V12 recipe is correct and runnable (I used the identical `gzip -c … in.cov > …bismark.cov.gz` + phase_b
  genome path; it produced the green matrix). V1/V11 are described as manual operator steps and are flagged
  STOP-if-not-exit-1 — appropriately mandatory.
- §1 `coverage2cytosine --version` expectation matches the real banner.
- §2 `bismark2bedGraph` cov recipe is plausible (not runnable here without the extractor output) and correctly
  marked Q2.
- §6 tag step is gated on a clean exit 0 and uses an annotated tag — safe.
- Note: §0 is "any bash ≥4 box," but the harness's own bash-version gate (exit 2 on <4) and the brew hint make
  the dev-machine path explicit; consistent.

---

### Recommendation
APPROVE-WITH-CHANGES. Fix **I1** (one-line `find`/`nullglob`/`|| true` change) so a `split`-cell crash or
empty-dir regression on oxy produces a diagnosable FAIL with intact verdict/evidence instead of a silent
mid-run abort. M1 (cx-first for `--cells`) is worth fixing since the checklist advertises `--cells` subsets.
M2/M3/Nits are advisory. No false-PASS hole found.

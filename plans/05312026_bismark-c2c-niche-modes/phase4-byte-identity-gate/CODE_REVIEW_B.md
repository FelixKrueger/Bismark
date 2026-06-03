# Code Review B — Phase 4 c2c byte-identity gate (niche modes)

**Reviewer:** Code Reviewer B (independent; no shared state with Reviewer A)
**Date:** 2026-06-01
**Worktree:** `/Users/fkrueger/Github/Bismark-c2c` (branch `rust/c2c-v1x`)
**Under review (uncommitted working tree):**
- `scripts/c2c_byte_identity_matrix.sh` (6 niche cells + 5 differentials + header doc)
- `RELEASE_CHECKLIST_c2c.md` (v1.x retitle + cells/differentials/`--CX`-cov dependency)
**Spec:** `plans/05312026_bismark-c2c-niche-modes/phase4-byte-identity-gate/PLAN.md` (rev 1)

---

## Top-line verdict: APPROVE

The harness extension is **correct and fail-CLOSED**. I independently ran all 6 niche
modes (Rust debug + Perl v0.25.1) on my own tiny `--CX` fixture, enumerated every output
filename, cross-checked every `REQUIRE_NONEMPTY` glob, ran the full extended matrix
(clean exit 0 on the niche subset, all 5 differentials PASS), and ran **three independent
fail-CLOSED probes** — all caught (exit 1) at the layer the plan claims. **No fail-OPEN
found. No false-FAIL found. 0 Critical, 0 High.** One Low (doc-completeness) and two
Info notes below.

**Critical: 0   High: 0   Medium: 0   Low: 1   Info: 2**

---

## Self-test + probe results

All run with `/opt/homebrew/bin/bash` (5.3.9), Rust `rust/target/debug/coverage2cytosine_rs`,
Perl repo-root `./coverage2cytosine` (confirmed `Version: v0.25.1`).

**Fixture:** a 70-bp single-chr `--CX`-style genome with ACG/TCG CpGs, non-ACG/TCG CpGs
(so the NOMe filter drops some), GpC dinucleotides, and a `GAACA`/DRACH motif; a gzipped
cov covering positions 1..70.

| Test | Command | Result |
|------|---------|--------|
| **V1 — full 15-cell matrix** | all cells | exit 1 ONLY because the `thr`/`split` **v1.0** differentials false-fail on a 1-chr/no-threshold fixture (fixture artifact, NOT a Phase-4 defect); all 15 cells PASS byte-identity; **all 5 niche differentials PASS** |
| **V1' — niche subset** (`--cells "default gc nome drach ffs ffs_cx ffs_nome"`) | clean | **exit 0**, 7/7 PASS, all 5 niche differentials PASS |
| **Probe A — corrupt Rust `c2c.GpC_report.txt`** (`gc` cell) | wrapper appends a line | **exit 1** `byte-diff: c2c.GpC_report.txt` |
| **Probe B — un-suppress Rust `c2c.NOMe.CpG.cov`** (`ffs_nome`) | wrapper writes a cov line | **exit 1** — caught at BOTH layers: per-cell `byte-diff: c2c.NOMe.CpG.cov` AND differential `ffs_nome .NOMe.CpG.cov present-and-0-byte both sides` FAIL |
| **Probe C — truncate Rust `c2c.CX_report.txt.gz`** (`ffs_cx`) | wrapper truncates gz | **exit 1** `gzip-integrity failed` (fail-CLOSED gz path) |
| **set -u safety** (`--cells cx` only; niche cells skipped) | clean | exit 0, no unbound-var abort — the `""`-init + `ran` guards hold |
| `bash -n` syntax | — | OK |

Probe B is the headline result: the **rev-3 Critical** (`--ffs` suppresses the NOMe cov to
0 bytes) is pinned by **defense in depth** — the per-cell 0-vs-non-0 byte-compare AND the
explicit differential both fail loudly if a future change un-suppresses the cov.

---

## Findings by focus area

### 1. Niche-mode output filenames vs `REQUIRE_NONEMPTY` globs — VERIFIED CORRECT

Observed Perl-side file sets (all non-empty unless noted):

- `gc`: `c2c.CpG_report.txt`, `c2c.GpC.cov`, `c2c.GpC_report.txt`, `c2c.cytosine_context_summary.txt` — glob `[gc]` lists all 4, all non-empty. ✓
- `nome`: `c2c.NOMe.CpG.cov`, `c2c.NOMe.CpG_report.txt`, `c2c.NOMe.GpC.cov`, `c2c.NOMe.GpC_report.txt`, `c2c.cytosine_context_summary.txt`. Glob `[nome]` requires only the **CpG** report + `.NOMe.CpG.cov` + summary. The two NOMe **GpC** streams are correctly **existence-only** (validated by file-set match + byte-compare). ✓ On my dense fixture they happened to be non-empty, but a hard require would FALSE-FAIL a sparser cov — exactly the rev-1 A-I1/B-Imp-1 fix.
- `drach`: ONLY `c2c_DRACH.cov` + `c2c_DRACH_report.txt` (standalone). Glob `[drach]` requires both. ✓
- `ffs`: `c2c.CpG_report.txt` (10-col) + summary. Glob `[ffs]` requires both. ✓
- `ffs_cx`: `c2c.CX_report.txt.gz` + summary. Glob `[ffs_cx]` requires both; gz content decompresses byte-identical (size differs 347 vs 346 due to gzip header — handled by the decompress-compare path). ✓
- `ffs_nome`: `c2c.NOMe.CpG.cov` is **present-and-0-byte on both sides**; `c2c.NOMe.CpG_report.txt` (422 B, 10-col), GpC streams, summary. Glob `[ffs_nome]` requires the **report** + summary and deliberately does NOT require `.NOMe.CpG.cov` (the suppressed 0-byte file). ✓

**Confirmed:** the NOMe GpC streams are existence-only, and `ffs_nome`'s `.NOMe.CpG.cov` is
NOT required-nonempty (a require would FALSE-FAIL the correct gate).

### 2. `FFSNOME_COV_EMPTY` stash (highest-risk addition) — VERIFIED CORRECT (loop-time, both-sides)

- Captured in `run_cell`'s `case` (lines 397–402), **before** the purge at lines 410–412. ✓
- Captures a **present-AND-0-byte-on-BOTH-sides** boolean: `[[ -f "$pdir/..." && -f "$rdir/..." && ! -s "$pdir/..." && ! -s "$rdir/..." ]]`. NOT a post-loop `stat`. ✓
- **I confirmed the purge deletes the 0-byte cov:** the purge `find ... \( -name '*.cov' ... \) -delete`
  matches `c2c.NOMe.CpG.cov`; after the purge the file is gone. A post-loop stat would read
  it as absent → would mis-classify, fail-OPEN. The loop-time capture is therefore **required
  and correct** (the rev-1 B-Imp-2 fix is properly implemented). ✓
- Checks **both** `$pdir` and `$rdir`, so a Rust-only un-suppression (Perl 0-byte, Rust non-0)
  also FAILs. Verified live in Probe B.

### 3. Stash-var init under `set -u` — VERIFIED (all 6 init `""`)

Line 264 initialises all 6 new vars `""`: `HASH_GC_CORE`, `LINES_NOME_CORE`,
`DRACH_STANDALONE_OK`, `FFS_ALL_10COL`, `LINES_FFS`, `FFSNOME_COV_EMPTY`. The `--cells cx`
run (niche cells skipped) confirmed the `[[ -n "$VAR" ]]` guards never trip `set -u`. ✓

### 4. Differential rigor — "would it FAIL if both binaries dropped the flag?"

| # | Differential | Drops-flag → ? | Verdict |
|---|--------------|----------------|---------|
| 1 | `gc` core == `default` core | PASS (by design — regression check, NOT a no-op detector) | **Correctly scoped, not over-claimed.** The comment + PLAN §3.3.1 explicitly disclaim no-op detection; the "did `--gc` emit a GpC stream" guard is `REQUIRE_NONEMPTY[gc]`'s GpC entry (which DOES catch a dropped `--gc`: no GpC report → require-absent → FAIL). ✓ |
| 2 | `nome` lines `!=` AND `<` default | nome==default → `!=` fails → FAIL | catches no-op ✓ (the `!=` is redundant-but-harmless belt-and-suspenders alongside `<`, matching PLAN wording) |
| 3 | `drach` standalone (DRACH report present AND CpG report absent AND summary absent) | normal run → CpG report present → FAIL | catches no-op ✓ |
| 4 | `ffs` 10-col **every line** (`awk 'NF!=10{exit 1}'`) AND lines==default | 7-col → NF!=10 → `FFS_ALL_10COL=0` → FAIL | catches no-op ✓. I confirmed `awk 'NF!=10{exit 1}'` flags a **mixed** 10/7 file (robust against a degenerate non-first line), and the empty-file edge (awk rc0) is covered because `LINES_FFS=0 != LINES_DEFAULT` fails the AND. ✓ |
| 5 | `ffs_nome` cov present-and-0-byte both sides | cov non-empty → `! -s` false → `FFSNOME_COV_EMPTY=0` → FAIL | catches no-op ✓ (verified live in Probe B) |

**None tautological / self-comparing / pass-on-no-op** beyond #1, which is deliberately a
regression check and is honestly labelled as such. `FFS_ALL_10COL` checks ALL lines;
`DRACH_STANDALONE_OK` requires the report present AND both CpG-report/summary absent; `nome`
is `!=`+`<`; diff #1 is a regression check (not over-claimed).

### 5. Reuse of v1.0 fail-CLOSED machinery — VERIFIED

The new cells route through the unchanged §3.4 guards: file-set match, per-file byte-compare,
gz integrity-test-then-decompress, binary exit codes, disk-discipline. Probes A/B/C exercised
the byte-compare, the differential, and the gz-integrity paths respectively — all fail-CLOSED.

### 6. Checklist + disk reasoning — VERIFIED (one Low gap)

`RELEASE_CHECKLIST_c2c.md` correctly adds: the v1.x title/tag (`v1.0.0-beta.2`), the
`rust/coverage2cytosine → rust/c2c-v1x` checkout fix, the 15-cell list, the
require-nonempty/existence-only split, the 5 differentials, the **`--CX`-cov dependency**
warning (GpC require depends on covered GpC-context Cs), the `ffs_cx` gzipped, and the
disk-fallback subset recipe. ✓

---

## Issues

**LOW-1 (doc completeness; non-blocking).** PLAN §5 step 2 (folding A-O2) calls for noting in
the checklist that **the `nome` cell is the 2nd-largest disk consumer**. That specific note is
**absent** from `RELEASE_CHECKLIST_c2c.md` (grep for "2nd-largest"/"largest consumer"/
"nome.*disk" → no match). This is operational guidance only — it does NOT affect the gate's
correctness, since the disk gate is fail-CLOSED per-cell (`disk_check "before cell ..."`)
regardless of which cell is largest. Recommend a one-line add to the disk section; not a
release blocker.

**INFO-1.** Differentials #1–#4 read only the **Perl** (`$pdir`) side (the established v1.0
pattern). This is NOT a fail-OPEN: Rust≡Perl is enforced separately by the per-cell file-set +
byte-compare (section b), which sets `verdict=FAIL` before the stash `case` runs. The
differentials are Perl-internal consistency checks; the byte-compare is the Rust-vs-Perl
backstop. `ffs_nome` correctly checks both sides because the 0-byte invariant must hold on
both producers.

**INFO-2.** The `thr`/`split` v1.0 differentials false-fail on a single-chromosome / no-droppable
-position fixture (observed in my full-matrix run). This is a **fixture limitation, not a defect**
— at full-hg38 scale they pass (and they correctly fail-CLOSED on a degenerate input, which is
the desired direction). Reviewers running a local self-test should use `--cells` to exclude
`thr`/`split` or use a multi-chr fixture with a threshold-droppable position.

---

## Bottom line

The Phase-4 extension faithfully implements the rev-1 PLAN, including all four plan-review
folds that matter for fail-CLOSED behaviour (existence-only NOMe GpC, loop-time
`FFSNOME_COV_EMPTY`, stash-var `""`-init, `awk` all-lines column check). Every new cell and
differential is fail-CLOSED in the safe direction; three independent corruption probes were
caught at the claimed layer; the suppressed-cov Critical is double-pinned. The single Low is a
missing operational disk note in the checklist. **APPROVE — clear to proceed to the oxy
full-genome run (V5).**

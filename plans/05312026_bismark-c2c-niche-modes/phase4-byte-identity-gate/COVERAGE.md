# Plan Coverage Report

**Mode:** B (code vs. plan, post-implementation)
**Plan(s):** `plans/05312026_bismark-c2c-niche-modes/phase4-byte-identity-gate/PLAN.md` (rev 1)
**Worktree:** `/Users/fkrueger/Github/Bismark-c2c` (branch `rust/c2c-v1x`), uncommitted working tree
**Audited files:** `scripts/c2c_byte_identity_matrix.sh` (M), `RELEASE_CHECKLIST_c2c.md` (M)
**Date:** 2026-06-01
**Verdict:** COMPLETE — all in-scope implementation items DONE; the oxy run (V5) + tag + merge are PENDING-by-design (operational, per the plan's Implementation notes)

## Summary

- Total items: 22 (19 implementation + 3 operational-PENDING)
- DONE: 19
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 0
- PENDING (by design, out-of-scope for this implementation): 3

## Coverage ledger

### §5 step 1 — `scripts/c2c_byte_identity_matrix.sh`

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | 6 niche cells appended to `ALL_CELLS` after the 9 v1.0 cells; `cx` stays first | §5.1a / §3.1 | DONE | `gc\|--gc`, `nome\|--nome-seq`, `drach\|--drach`, `ffs\|--ffs`, `ffs_cx\|--ffs --CX --gzip`, `ffs_nome\|--ffs --nome-seq` (lines 180–186). cx still first. |
| 2 | 6 `REQUIRE_NONEMPTY` entries with the existence-only split | §5.1b / §3.2 | DONE | Lines 206–215. Exactly the 6 globs in §3.2. NOMe-GpC streams NOT listed; `ffs_nome` `.NOMe.CpG.cov` NOT listed (required-EMPTY). |
| 3 | 6 stash vars initialised `""` at the declaration block | §5.1c / §3.3 (B-Imp-4) | DONE | Line 263: `HASH_GC_CORE="" LINES_NOME_CORE="" DRACH_STANDALONE_OK="" FFS_ALL_10COL="" LINES_FFS="" FFSNOME_COV_EMPTY=""` alongside the v1.0 vars (lines 260–261). |
| 4 | 6 captures in `run_cell`'s `case`, BEFORE purge | §5.1d / §3.3 (B-Imp-2) | DONE | Lines 384–402, inside the `case "$name"` block (line 373) which precedes the purge `find ... -delete` at lines 410–411. |
| 4a | `gc → HASH_GC_CORE` (hash of `c2c.CpG_report.txt`) | §5.1d | DONE | Line 385, `hash_plain`. |
| 4b | `nome → LINES_NOME_CORE` (lines of `c2c.NOMe.CpG_report.txt`) | §5.1d | DONE | Line 386, `lines_plain`. |
| 4c | `drach → DRACH_STANDALONE_OK` (DRACH report present AND no CpG report+summary) | §5.1d | DONE | Lines 387–390: `[[ -f DRACH_report && ! -e CpG_report && ! -e summary ]]`. |
| 4d | `ffs → FFS_ALL_10COL` (`awk -F'\t' 'NF!=10{exit 1}'` over ALL lines) + `LINES_FFS` | §5.1d / §3.3.4 (B-Opt-1, A-I3) | DONE | Lines 391–396; awk checks all lines; both vars set. |
| 4e | `ffs_nome → FFSNOME_COV_EMPTY` (present-AND-0-byte on BOTH sides, NOT post-purge stat) | §5.1d / §3.3.5 (B-Imp-2) | DONE | Lines 397–402: tests `-f` on both `$pdir` and `$rdir` AND `! -s` on both — captured during loop. |
| 5 | 5 `diff_check` calls, each `ran`-guarded + stash-non-empty-guarded | §5.1e / §3.3 | DONE | Lines 465–488. |
| 5.1 | diff #1: gc core == default (regression, NOT no-op detector) | §3.3.1 (B-Imp-3/A-I3) | DONE | Lines 466–468; `ran gc && ran default`; `HASH_GC_CORE == HASH_DEFAULT`. |
| 5.2 | diff #2: nome lines `!=` AND `<` default | §3.3.2 (A-I2) | DONE | Lines 470–472; both `!=` and `-lt` asserted. |
| 5.3 | diff #3: drach standalone | §3.3.3 | DONE | Lines 474–476; `DRACH_STANDALONE_OK -eq 1`. |
| 5.4 | diff #4: ffs 10-col every line AND lines == default | §3.3.4 | DONE | Lines 478–480; `FFS_ALL_10COL -eq 1 && LINES_FFS -eq LINES_DEFAULT`. |
| 5.5 | diff #5: ffs_nome `.NOMe.CpG.cov` present-and-0-byte both sides (reads stash, no post-purge stat) | §3.3.5 (B-Imp-2) | DONE | Lines 482–488; reads `FFSNOME_COV_EMPTY`, does NOT stat the purged file. |
| 6 | Header doc updated (15 cells, `--CX`-cov dependency, NOMe-GpC existence-only) | §5.2 / §3.1 | DONE | Lines 3–20: "15-cell", v1.0/v1.x split, the `--CX` must-be-all-context note, NOMe-GpC existence-only; design refs both PLANs. |

### §5 step 2 — `RELEASE_CHECKLIST_c2c.md`

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 7 | Stale branch fix `rust/coverage2cytosine → rust/c2c-v1x` | §5.2 / §3.3 reframe | DONE | The `git checkout` line now reads `rust/c2c-v1x`. |
| 8 | Title retitled (Phase E + v1.x Phase 4); "v1.0 (Phase E)" → v1.x | §5.2 | DONE | Header now "real-data release checklist (Phase E + v1.x Phase 4)". |
| 9 | 6 new cells + expected streams listed | §5.2 | DONE | "The 15 cells" section: v1.0 core (9) + v1.x niche (6). |
| 10 | require-nonempty / existence-only split documented | §5.2 / §3.2 | DONE | Explicit paragraph: core+summary, gc GpC, nome NOMe-core+`.NOMe.CpG.cov`, drach required; NOMe-GpC existence-only; `ffs_nome` `.NOMe.CpG.cov` suppressed-0-byte. |
| 11 | 5 new differentials documented | §5.2 / §3.3 | DONE | "5 new cross-cell differentials" paragraph mirrors §3.3.1–5. |
| 12 | `--CX`-cov dependency (Assumption 8) documented | §5.2 / §8 | DONE | The ⚠️ all-context block + the "What this gate proves" niche-mode addendum. |
| 13 | v1.x tag (`v1.0.0-beta.2`) + `→ iron-chancellor` merge | §5.2 / §5.5 | DONE | §6 "v1.x (Phase 4)" `git tag -a …v1.0.0-beta.2` + "merge `rust/c2c-v1x → rust/iron-chancellor`". |

### §5 step 3 — local self-test (§3.5 / V1–V3)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 14 | V1: full niche matrix on a tiny `--CX` fixture → exit 0, all 5 differentials PASS | §3.5 / V1 / §9 | DONE | Re-run live by this audit (see Test verification). exit 0; 7/7 cells byte-identical; all 5 differentials PASS. |
| 15 | V2: a broken Rust output is caught → exit 1 (fail-CLOSED) | §3.5 / V2 / §9 | DONE | Re-run live: corrupted Rust `c2c.GpC_report.txt` → `byte-diff` FAIL, exit 1. |
| 16 | V3: differential fails on a no-op (non-empty `ffs_nome` cov) → exit 1 | §3.5 / V3 / §9 | DONE | Re-run live: un-suppressed `.NOMe.CpG.cov` on both sides → diff #5 FAIL, exit 1 (the B-Imp-2 fail-OPEN scenario is correctly closed). |

### §8 Assumption 8 — the `--CX`-cov dependency

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 17 | The gc-cell GpC require-nonempty is justified by an all-context `--CX` gate cov, and this dependency is pinned in the checklist + the script header | §8 Assumption 8 | DONE | Script header lines 14–20 + checklist ⚠️ block both state the gate cov MUST be `bismark2bedGraph --CX`; NOMe-GpC downgraded to existence-only. |

### Operational steps (PENDING by design — plan's Implementation notes, NOT gaps)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 18 | V5: the oxy full-genome 15-cell matrix → exit 0 | §5.4 / §9 V5 | PENDING | Multi-hour, needs oxy access; plan's Implementation notes (2026-06-01) explicitly mark this remaining-operational. |
| 19 | Tag `bismark-coverage2cytosine-v1.0.0-beta.2` on a clean oxy exit 0 | §5.5 | PENDING | On confirmation, after V5. |
| 20 | Merge `rust/c2c-v1x → rust/iron-chancellor` (epic close) | §5.5 / §7 | PENDING | On confirmation, after the tag. |

## Gaps (detail)

None. No PARTIAL / MISSING / DEVIATED items.

The 3 PENDING items (oxy run, tag, merge) are explicitly designated as the remaining
operational steps in the plan's "Implementation notes (2026-06-01)" and §5 steps 4–5
("done on confirmation"). They are not implementation gaps in the harness extension.

## Test verification (Mode B)

The §3.5 self-test (PLAN V1–V3) was re-run live by this audit using `/opt/homebrew/bin/bash`
(GNU bash 5.3.9) on a freshly-built `--release` Rust binary (the checked-in
`target/release/coverage2cytosine_rs` was a stale pre-niche-mode build — rebuilt during the
audit; no Rust source was touched, per the plan).

Fixture: a single combined `--CX` genome (`>chr1 TTTGAACATTTACGTTGCGCATCGTTAGCGGCATTAGC`)
exercising a GAACA DRACH motif, ACG/TCG NOMe-keep CpGs, a plain CpG (NOMe-drop), GpC
dinucleotides, and CHG/CHH context; an 8-line gzipped cov over those positions.

| Test | Command | Expected | Result |
|------|---------|----------|--------|
| V1 (correct → exit 0) | `c2c_byte_identity_matrix.sh test.bismark.cov.gz --genome genome --cells "default gc nome drach ffs ffs_cx ffs_nome"` | exit 0; 7/7 cells byte-identical; all 5 differentials PASS | PASS — exit 0; PASS=7 FAIL=0; diff_fail=0. diffs: gc core==default; nome 2<8 (filter fired); drach standalone; ffs 10-col + lines==default; ffs_nome `.NOMe.CpG.cov` present-and-0-byte both sides |
| V2 (broken Rust output → exit 1) | same, `--cells "default gc"`, Rust wrapper appends a line to `c2c.GpC_report.txt` | exit 1, not a false-PASS | PASS — exit 1; `gc FAIL [byte-diff: c2c.GpC_report.txt]` |
| V3 (no-op differential → exit 1) | `--cells ffs_nome`, both Perl+Rust wrappers un-suppress `.NOMe.CpG.cov` identically | exit 1 (diff #5 fires even though per-cell byte-compare passes) | PASS — exit 1; per-cell `ffs_nome PASS` but `diff_fail=1`, `FAIL: ffs_nome .NOMe.CpG.cov present-and-0-byte both sides` — confirms the rev-1 B-Imp-2 fix closes the fail-OPEN |

Fixture-level Perl probes (pre-run, confirming each differential precondition is non-trivial):
default core = 8 lines; NOMe core = 2 lines (`< 8`); gc GpC report = 4 non-empty lines;
gc core report `diff`-identical to default; nome `.NOMe.CpG.cov` = 2 lines (non-empty);
drach report = 1 line, standalone (only `_DRACH_report.txt` + `_DRACH.cov`);
ffs core report = 10 columns; `ffs_nome` `s.NOMe.CpG.cov` = 0 bytes with a 10-col NOMe core report.

## Verdict

**COMPLETE.** Every §5 implementation item (step 1 a–e, step 2, step 3), every §3.1 cell,
every §3.2 require-nonempty entry (with the correct existence-only split), every §3.3
differential (incl. the rev-1 reframes: diff #1 regression, diff #2 `!=`+`<`, diff #4
all-lines column count, diff #5 stash-during-loop), and §8 Assumption 8 are present in the
working tree and verified. The §3.5 local self-test (V1–V3) passes live: correct outputs →
exit 0 with all 5 differentials satisfied; a corrupted Rust output → exit 1; a no-op
un-suppression → exit 1 via the differential.

The oxy run (V5) + the `v1.0.0-beta.2` tag + the `rust/c2c-v1x → iron-chancellor` merge are
PENDING by design (operational, on confirmation) per the plan's Implementation notes — NOT gaps.

One audit-time observation (not a plan gap): the checked-in
`rust/target/release/coverage2cytosine_rs` was a stale pre-niche-mode build; the harness's
own pre-flight (step 6) rebuilds `--release` on demand, so an oxy run from a clean checkout is
unaffected, but a stale local binary will FAIL the niche cells with `rust_rc=1` until rebuilt.

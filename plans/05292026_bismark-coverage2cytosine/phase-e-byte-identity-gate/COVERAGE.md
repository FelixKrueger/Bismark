# Plan Coverage Report — Phase E (Real-data byte-identity gate)

**Mode:** B (code vs. plan — harness script + checklist, post-implementation)
**Plan(s):** `phase-e-byte-identity-gate/PLAN.md` (rev 1); contract `../SPEC.md` §12.3
**Audited artifacts:** `scripts/c2c_byte_identity_matrix.sh`, `RELEASE_CHECKLIST_c2c.md`
**Date:** 2026-05-30
**Verdict:** **COMPLETE** — 0 items unresolved. 1 documented deviation (D1), present and matching its note.

## Summary

- Total items audited: **60** (10 pre-flight gates + 9 matrix cells + 4 fail-CLOSED sub-rules + 7 differential checks + 4 disk-discipline items + 3 exit codes + 12 folded findings + 12 validations V1–V12 + 1 deviation, with overlaps counted once per row below)
- DONE: 59
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 1 (D1 — documented, matches the note; not a gap)

Self-tests were **re-run live** during this audit on bash 5.3.9 against the `phase_b` fixture (+ repo Perl v0.25.1): **V12** exit 0 (9/9 cells, 7/7 differentials: cx=25>default=18, thr=2<18, split=4>1); **V1** exit 1 (`default FAIL [byte-diff: c2c.CpG_report.txt]`); **V11** exit 1 (`gzip FAIL [gzip-integrity failed: c2c.CpG_report.txt.gz]`). Exit-2 gates all fire (disk floor, missing genome, non-gz cov, wrong Perl version). Results reproduce the PLAN Implementation-notes claims exactly.

## Coverage ledger

### §3.1 Pre-flight gates (10)

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | bash ≥ 4 hard-fail + brew hint | §3.1.1 | DONE | L36-41; `(( BASH_VERSINFO[0] < 4 ))`, exit 2, brew + `/opt/homebrew/bin/bash` hint |
| 2 | `<COV_GZ>` readable + `.gz` suffix + abs-canonicalize | §3.1.2 | DONE | L80-83 |
| 3 | `--genome` readable + four-suffix FASTA glob | §3.1.3 | DONE | L86-92; globs `*.fa/*.fa.gz/*.fasta/*.fasta.gz` (matches SPEC §6.1) |
| 4 | `--out` empty-or-absent + canonicalize | §3.1.4 | DONE | L94-103 |
| 5 | Perl present + `Version: v0.25.1` on the c2c line | §3.1.5 (A-M1/B-M1) | DONE | L105-116; requires BOTH `coverage2cytosine` AND `Version: v0\.25\.1` (regex). Verified against live binary: `--version` emits both tokens. NOT the extractor's "Bismark Extractor Version:" format |
| 6 | Rust binary discoverable / build --release + record crate ver + git HEAD | §3.1.6 | DONE | L118-129; `cargo build --release -p bismark-coverage2cytosine`, records `RUST_VERSION` + `GIT_HEAD` |
| 7 | Disk-headroom gate (`df -Pk` → GiB), floor default 30, exit 2 | §3.1.7 (Q1) | DONE | L131-145; `free_gb`/`disk_check`; verified exit 2 when floor>free |
| 8 | `export LC_ALL=C` (belt-and-suspenders) | §3.1.8 (B-M2) | DONE | L148 |
| 9 | tmux/screen advisory | §3.1.9 | DONE | L150-154; warns when `$TMUX`/`$STY` unset |
| 10 | SIGINT/TERM trap → preserve partial, exit 130 | §3.1.10 | DONE | L44; trap set before any work, references `$OUT_DIR` |

### §3.2 Matrix cells (9) — flags + report-derived merged/discordant names

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 11 | `cx` = `--CX --gzip`; CX_report.txt.gz + summary; runs FIRST | §3.2/§3.7 | DONE | L159 (first in array), L174 require-nonempty |
| 12 | `default` = (none); CpG_report.txt + summary | §3.2 | DONE | L160, L175 |
| 13 | `zero` = `--zero_based`; CpG_report.txt + summary | §3.2 | DONE | L161, L176 |
| 14 | `gzip` = `--gzip`; CpG_report.txt.gz + summary | §3.2 | DONE | L162, L177 |
| 15 | `thr` = `--coverage_threshold 5`; CpG_report.txt + summary | §3.2 | DONE | L163, L178 |
| 16 | `split` = `--split_by_chromosome`; per-chr set | §3.2 | DONE | L164; require-nonempty empty (existence-only per §3.4.1) |
| 17 | `merge` = `--merge_CpGs`; report + merged-cov + summary | §3.2 | DONE | L165, L180 (`c2c.CpG_report.merged_CpG_evidence.cov`) |
| 18 | `merge_disc` = `--merge_CpGs --discordance_filter 10` | §3.2 | DONE | L166, L181 (merged-cov existence-only; discordant existence-only) |
| 19 | `merge_gzip` = `--merge_CpGs --gzip`; merged-cov.gz + report.gz + summary | §3.2 (A-M3/B-M4) | DONE | L167, L182 |
| 20 | **Merged/discordant filenames are REPORT-derived** | §3.2 (A-I1/B-C1) / V10 | DONE | Names hard-coded as `c2c.CpG_report.merged_CpG_evidence.cov[.gz]` — matches `report::merged_cov_name` (report.rs:490 → `merge.CpG_report.merged_CpG_evidence.cov`). The `-o c2c` report is `c2c.CpG_report.txt`, so derived name = `c2c.CpG_report.merged_…`, NOT `c2c.merged_…`. Confirmed against binary source |

### §3.3–§3.5 Per-cell execution + fail-CLOSED comparison

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 21 | Per-cell Perl→`perl/`, Rust→`rust/`, identical flags, `-o c2c --dir`, allow non-zero, time both | §3.3 | DONE | L224-247; `set +e/-e` brackets both runs; records `perl_s`/`rust_s`/`prc`/`rrc` |
| 22 | file-name-set match (missing/extra ⇒ FAIL) | §3.4.1/§3.5 | DONE | L252-258; `ls -1 \| sort` both sides, diff into `diff.txt` |
| 23 | gz `gzip -t` integrity pre-check BOTH sides BEFORE decompress-compare | §3.4.2 (A-C1/B-I3) | DONE | L266-272; the fail-open fix — verified live (V11 exit 1) |
| 24 | Plain `cmp -s`; gz decompress-compare after integrity | §3.4.2 | DONE | L273-282 |
| 25 | Empty-tolerant rule: only `REQUIRE_NONEMPTY` files asserted non-empty; per-file compare is empty-on-both⇒PASS via `cmp` | §3.4.1 | DONE | L286-298; discordant / merge_disc merged-cov / split per-chr are existence-only (not in REQUIRE_NONEMPTY lists) |
| 26 | Per-cell require-nonempty (report+summary always; merged-cov only plain `merge`) | §3.4.1 | DONE | L173-183; `merge` lists merged-cov, `merge_disc`/`merge_gzip` scope correctly |
| 27 | split: file-name set (a/b cover it) + per-file empty-tolerant + ≥1 per-chr report | §3.5 | DONE | L300-307; file-set+per-file via (a)/(b); SPLIT_FILE_COUNT≥1 guard. Last-chr-summary content validated by per-file byte compare in (b) |

### §3.6 Cross-cell differential checks (7) + stash-before-purge

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 28 | `cx` lines > `default` lines | §3.6.1 / V5 | DONE | L361-363; LINES_CX vs LINES_DEFAULT. Live: 25>18 |
| 29 | `zero` ≠ `default` | §3.6.2 / V4 | DONE | L364-366; hash inequality |
| 30 | `gzip` decompressed == `default` | §3.6.3 | DONE | L367-369; hash equality |
| 31 | `thr` lines < `default` lines | §3.6.4 / V6 | DONE | L370-372. Live: 2<18 |
| 32 | `merge` merged-cov non-empty (plain merge only) | §3.6.5 / V8 | DONE | L373-375; MERGE_COV_NONEMPTY |
| 33 | `merge_gzip` decompressed == `merge` merged-cov | §3.6.5 | DONE | L376-378; hash equality |
| 34 | `split` file count > 1 | §3.6.6 | DONE | L379-381. Live: files=4 |
| 35 | Differential inputs STASHED during cell loop BEFORE purge | §3.6/§3.7 (A-I4/B-I4) | DONE | L309-321 stash block runs before the purge at L328-330; all of HASH_DEFAULT/LINES_DEFAULT/HASH_ZERO/HASH_GZIP_DECOMP/LINES_CX/LINES_THR/HASH_MERGE_COV/MERGE_COV_NONEMPTY/HASH_MERGEGZIP_COV_DECOMP/SPLIT_FILE_COUNT captured pre-purge |

### §3.7 Disk discipline + §3.8 exit codes

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 36 | Per-cell disk re-check before each cell | §3.7 (A-I3) | DONE | L341-347; `disk_check "before cell …"` → exit 2 on starvation |
| 37 | `cx` ordered FIRST (max free space) | §3.7 | DONE | L159 first array element |
| 38 | Purge-on-PASS / keep-on-FAIL; `--keep-all` override | §3.7 | DONE | L327-330 (`-delete` only on PASS && KEEP_ALL==0); FAIL keeps everything for diff.txt + outputs |
| 39 | Verdict + summary + perf files written | §3.8 | DONE | L389-434; matrix_verdict.txt, byte_identity_summary.md, perf_table.md |
| 40 | Exit 0 / 1 / 2; no perf gate | §3.8 | DONE | L440-447; usage→2, fail→1, diff_fail→1, else 0. No exit-3. Verified live: 0/1/2 all reproduce |

### §9 Validations V1–V12

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| V1 | Harness fails CLOSED on deliberate byte-diff → exit 1, names cell+stream | §9 | DONE | Re-run live via `--rust-c2c` wrapper appending 1 byte: `default FAIL [byte-diff: c2c.CpG_report.txt]`, exit 1 |
| V2 | Missing-required output ⇒ FAIL (not vacuous pass) | §9 | DONE | L286-298 require-nonempty + L252-258 file-set mismatch both FAIL; supported |
| V3 | gzip decompress-compare on valid data; container ignored | §9 | DONE | L273-282 (compare after `gzip -dc`); V12 cx/gzip/merge_gzip cells PASS |
| V4 | `--zero_based` differential | §9 | DONE | diff-check #29; V12 PASS |
| V5 | `--CX` > CpG differential; CX count from gz pass | §9 | DONE | diff-check #28; counted via `lines_gz` (see D1) |
| V6 | `--coverage_threshold` differential | §9 | DONE | diff-check #31; V12 thr=2<18 |
| V7 | split: file-name set + per-file + empty-tolerant + last-chr summary | §9 (A-C2/B-I1) | DONE | items 22/25/27; `phase_b` genome includes `scaf_short` (2bp CG) → exercises empty-on-both per-chr report; V12 split PASS (4 files) |
| V8 | merge + discordance incl. empty merged-cov | §9 (A-I2/B-I2) | DONE | `merge` merged-cov required non-empty; `merge_disc` merged-cov existence-only (not in REQUIRE_NONEMPTY[merge_disc]); V12 both PASS |
| V9 | disk pre-flight + per-cell re-check; keep-all starvation | §9 (A-I3) | DONE | items 7/36; verified exit 2 on `--disk-floor-gb 999999999`. Per-cell re-check L341-345 |
| V10 | merged/discordant filename report-derived | §9 (A-I1/B-C1) | DONE | item 20; confirmed against report.rs |
| V11 | gzip-integrity fail-CLOSED on truncated gz → exit 1, not false PASS | §9 (A-C1/B-I3) | DONE | Re-run live via wrapper truncating the Rust `.gz`: `gzip FAIL [gzip-integrity failed: c2c.CpG_report.txt.gz]`, exit 1. The rev-1 Critical is closed |
| V12 | full matrix green = release gate (exit 0 ⇒ eligible to tag) | §9 | DONE | Re-run live: 9/9 PASS, 7/7 differentials, exit 0 |

### §5 Implementation outline (12 steps) — all reflected

| Step | Item | Status | Notes |
|------|------|--------|-------|
| 1 | Scaffold from phase_h_se pattern (shebang, set -euo, bash≥4, trap, argparse, REPO_ROOT) | DONE | L1-73 |
| 2 | Pre-flight gates in order incl. disk + `--version` assertion | DONE | L75-154 |
| 3 | Build/locate Rust + record version + git HEAD | DONE | L118-129 |
| 4 | `MATRIX_CELLS` name\|flags, cx first, report-derived names | DONE | L158-183 |
| 5 | Per-cell loop, stash differentials before purge | DONE | L224-331 |
| 6 | compare helper (existence + nonempty + gz-integrity + decompress) | DONE | L249-298 (inline in run_cell, equivalent) |
| 7 | split handler (file-set + per-file + last-chr) | DONE | L300-307 + (a)/(b) |
| 8 | cross-cell differentials from stash | DONE | L349-381 |
| 9 | disk discipline (re-check, purge/keep, keep-all) | DONE | L327-345 |
| 10 | verdict + summaries + exit | DONE | L383-447 |
| 11 | RELEASE_CHECKLIST_c2c.md (oxy setup, cov.gz recipe, pass criteria, disk fallback, tag-only-on-0, mandatory §0 self-tests) | DONE | checklist §0-§7 (see checklist coverage below) |
| 12 | Self-test fails-closed before trusting green | DONE | checklist §0 mandates V1+V11; re-verified live |

### Rev-1 folded findings (12) — all reflected in code

| Folded item | Source | Status | Where |
|-------------|--------|--------|-------|
| C1/I3 gzip fail-OPEN → `gzip -t` pre-check + V11 | A+B | DONE | L266-272; V11 live exit 1 |
| I1/C1 merged/discordant filename report-derived | A+B | DONE | L180/L182, matches report.rs |
| C2/I1 split per-chr reports can be empty (empty-tolerant) | A+B | DONE | L179 (no require-nonempty) + per-file `cmp` empty-on-both pass |
| I2/I2 merge_disc merged-cov can be empty (non-empty scoped to plain merge) | A+B | DONE | L180 vs L181; only `merge` lists merged-cov |
| I4/I4 cx line-count + stash before purge | A+B | DONE | stash L309-321 before purge L328; cx count via lines_gz (D1) |
| M1/M1 Perl `--version` string = `Version: v0.25.1` (not extractor format) | A+B | DONE | L111 regex; verified live |
| I3 per-cell disk re-check + cx first | A | DONE | L341-345 + L159 |
| C2 CX disk estimate corrected/de-loaded | B | DONE | §6 PLAN text + checklist §5 fallback; harness measures via disk gate |
| I5/N1 plain-CX real-data gap documented + accepted | A+B | DONE | PLAN §3.2 note; harness gzips cx by design (L159) |
| M3/M4 merge_gzip streams (+summary +report.gz) | A+B | DONE | L182 require-nonempty merged-cov.gz + summary; report.gz compared via file-set |
| M2 LC_ALL=C softened to belt-and-suspenders | B | DONE | L147-148 |
| M3 v1.x flag rejections out of scope | B | DONE | not in matrix (correct); no v1.x cells |

### RELEASE_CHECKLIST_c2c.md (§5 step 11)

| Item | Status | Where |
|------|--------|-------|
| Mandatory §0 fail-CLOSED self-tests FIRST (V12+V1+V11), STOP if not exit 1 | DONE | checklist §0 (L11-39); names the fixture + bash≥4 note |
| oxy setup (dcli ssh, tmux, micromamba bismark-test, `--version` check, rustup, git checkout, build) | DONE | §1 (L43-59) |
| cov.gz generation recipe (Perl bismark2bedGraph on 10M extractor output) | DONE | §2 (L61-71) |
| Matrix invocation + distinct out-dir | DONE | §3 (L73-82) |
| Pass criteria (exit 0/1/2 meaning; 9 cells listed) | DONE | §4 (L84-94) |
| Disk fallback (chr-subset genome for cx only) | DONE | §5 (L96-101) |
| Tag step ONLY on clean exit 0 + push + update PR/epic | DONE | §6 (L103-110) |
| What the gate proves (two-producer sub-gate 2) | DONE | §7 (L112-118) |

### Documented deviation D1

| Item | Status | Notes |
|------|--------|-------|
| D1 — cx line-count is a separate Perl-side decompress (`gzip -dc \| wc -l`), NOT a tee-single-pass | DONE / DEVIATED-as-documented | PLAN Implementation-notes "Deviation D1"; code = `lines_gz()` (L208) = `gzip -dc \| wc -l`. Rationale (tee process-sub flush race unreliable for a release gate; perf not gated per SPEC §10.7) matches the note. The §3.6.1/V5 plan text asks for a single-pass count; the deviation is explicitly recorded and the byte-identity assertion is unaffected. Not a gap |

## Gaps (detail)

None. No MISSING or PARTIAL items.

The single deviation (D1) is documented in the PLAN's own Implementation notes with a stated rationale and matches the shipped code (`lines_gz` does a standalone `gzip -dc | wc -l`). Per plan-manager rules, a deviation that is documented with rationale is not a coverage gap.

## Test verification (Mode B — self-tests re-run live)

| Self-test | Command (this audit) | Status |
|-----------|----------------------|--------|
| V12 full matrix on phase_b fixture | `bash5 …matrix.sh /tmp/in.bismark.cov.gz --genome phase_b/genome --out … --disk-floor-gb 1` | PASS — exit 0, 9/9 cells, 7/7 differentials (cx=25>default=18, thr=2<18, split=4>1) |
| V1 deliberate byte-diff | `--rust-c2c` wrapper appends 1 byte to CpG report | PASS — exit 1, `default FAIL [byte-diff: c2c.CpG_report.txt]` |
| V11 truncated gz | `--rust-c2c` wrapper truncates Rust `.gz` | PASS — exit 1, `gzip FAIL [gzip-integrity failed: c2c.CpG_report.txt.gz]` (not a false PASS) |
| Exit 2 — disk pre-flight | `--disk-floor-gb 999999999` | PASS — exit 2 |
| Exit 2 — missing genome | omit `--genome` | PASS — exit 2 |
| Exit 2 — non-gz cov | plain `.cov` positional | PASS — exit 2 |
| Exit 2 — wrong Perl version | `--perl-c2c` fake v0.24.0 | PASS — exit 2, names v0.25.1 |
| Help | `-h` | PASS — exit 0, prints usage block |
| Syntax | `bash -n` | PASS — clean |

Note: V12/V1/V11 were also already RUN locally per the PLAN Implementation-notes; this audit independently reproduces all three plus the exit-2 gates, confirming the harness supports them.

## Verdict

**COMPLETE.** Every §3.1 pre-flight gate, all 9 matrix cells with correct flags + report-derived merged/discordant filenames, the fail-CLOSED comparison (file-name-set match, `gzip -t` integrity pre-check, empty-tolerant rules, per-cell require-nonempty), all 7 cross-cell differential checks with stash-before-purge, per-cell disk re-check + cx-first + purge-on-pass/keep-on-fail, exit codes 0/1/2 (no perf gate), all 12 rev-1 folded findings, all V1–V12, the full §5 implementation outline, and the RELEASE_CHECKLIST_c2c.md content are present and verified. The one deviation (D1, cx line-count) is documented with rationale and does not weaken byte-identity. No gaps require action before the harness is trusted to gate the `bismark-coverage2cytosine-v1.0` tag on oxy.

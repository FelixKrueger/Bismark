# Phase E PLAN — Real-data byte-identity gate (oxy)

**Epic:** `05292026_bismark-coverage2cytosine/EPIC.md`, Phase E — Real-data byte-identity gate.
**Design contract:** `../SPEC.md` (rev 3) — §12.3 (real-data gate, now oxy), §5 (output topology), §13 (sub-gate-2 fit), §10.5/§15/P10 (gzip post-decompression).
**Depends on:** Phases B + C + D (the full `coverage2cytosine_rs` binary across CpG/CX/zero/threshold/gzip/split/merge/discordance).
**Status:** rev 1 — dual plan-review folded (A APPROVE-WITH-CHANGES, B REQUEST-CHANGES; both live-Perl-verified). Awaiting the implement trigger.

> ⚠️ **MACHINE DEVIATION (Felix directive, 2026-05-30): this gate runs on `oxy`, NOT `colossal`.** SPEC §12.3 + EPIC §3 + the `reference_colossal_access` memory are now synced to record the oxy retarget (the 2026-05-28 migration moved real-data testing oxy→colossal *because oxy's home is capped at ~99 GB*). The disk cap is the dominant design constraint here (§6, Q1).

## Implementation notes (2026-05-30)

**Built:** `scripts/c2c_byte_identity_matrix.sh` (the 9-cell driver, all rev-1 design points) + `RELEASE_CHECKLIST_c2c.md` (oxy setup, cov.gz recipe, pass criteria, the V1/V11 fail-CLOSED self-tests as mandatory pre-trust, disk fallback, tag step). `bash -n` syntax-clean. No crate source touched.

**Fail-CLOSED implemented:** file-name-set match (missing/extra ⇒ FAIL) + per-file compare with empty-on-both⇒PASS / empty-on-one⇒FAIL; gz streams `gzip -t` integrity-tested **before** decompress-compare (closes the `cmp <(gzip -dc)` swallow); per-cell require-nonempty lists (report+summary always; merged-cov only in plain `merge`); differential checks stashed during the loop (content hashes via `shasum`, line counts) and evaluated post-loop; per-cell disk re-check; `cx` first; purge-on-pass/keep-on-fail; exit 0/1/2.

**Deviation D1 (documented) — `cx` line-count is a separate Perl-side decompress, not a tee-single-pass.** PLAN §3.6.1 called for folding the CX line count into the single decompress-compare pass. Implemented instead as `gzip -dc <perl CX> | wc -l` (one extra Perl-side decompress beyond the two in the byte-compare). Rationale: the `tee >(wc -l > f)` single-pass races on the process-substitution flush (the count file may be unread when `cmp` returns) — unreliable for a release gate. On the full-genome CX this adds ~one extra streaming decompress (minutes); perf is not gated (SPEC §10.7), so reliability wins. The byte-identity assertion is unaffected.

**Self-test status — PASSED locally (bash 5.3.9 installed via brew, Felix's call).** All three run against the `phase_b` fixture (which includes the `scaf_short` short scaffold → the empty-split-report edge) + the repo's Perl v0.25.1:
- **V12 (full matrix green):** 9/9 cells byte-identical; 7/7 cross-cell differentials satisfied (cx=25 > default=18 lines; zero≠default; gzip==default; thr=2 < default; merge non-empty; merge_gzip==merge; split=4 files >1). Exit 0.
- **V1 (deliberate byte-diff):** appended a byte to the Rust CpG report → `default FAIL [byte-diff: c2c.CpG_report.txt]`, exit 1. Fail-CLOSED on byte-diff confirmed.
- **V11 (truncated gz):** truncated the Rust `.gz` → `gzip FAIL [gzip-integrity failed: c2c.CpG_report.txt.gz]`, exit 1. **Confirms the rev-1 Critical (the `gzip -dc` fail-open) is closed** — `gzip -t` catches the truncation rather than false-PASSing.

**Iteration log:**
- #1: wrote the driver + checklist; `bash -n` clean. Discovered macOS=bash 3.2 blocks the behavioral self-test (declare -A needs bash ≥4); surfaced rather than refactoring to 3.2 (the plan mandates bash ≥4 for the oxy target).
- #2: `brew install bash` → bash 5.3.9 at `/opt/homebrew/bin/bash`. Ran V12/V1/V11 — all as expected (exit 0 / 1 / 1). Harness proven fail-CLOSED before oxy.
- #3: dual code-review (both **APPROVE-WITH-CHANGES**, 0 Critical, no false-PASS on in-scope streams) + plan-manager (**COMPLETE**, 0 coverage gaps) folded. Fixed **A-I1** (split `ls`-glob aborted the matrix under `set -e` on no-match → `find -maxdepth 1`), **B-I1** (binary exit codes weren't gated → a non-zero exit with matching output now FAILs), **B-M1** (numeric `--disk-floor-gb` guard → clean exit 2 not a `set -u` exit 1), **B-M2** (anchored `--version` regex `Version: v0\.25\.1[[:space:]]*$` — rejects `v0.25.10`/`-dev`), **A-M1** (iterate cells in canonical cx-first order regardless of `--cells` arg order + reject unknown cell names), **A-M2/B-M3** (independent "exactly 1 non-empty per-chr summary" split assertion — the last-chr quirk the byte-compare only caught transitively), **B-M4** (commented the `-o c2c`↔hardcoded-filename coupling). Left **A-M3** (single-cell mid-write disk exhaustion — plan-accepted §6). Re-verified on the fixture: V12 exit 0 (9/9 cells, 7/7 differentials, split still PASS), V1/V11 exit 1, a NEW rc-gate negative test (correct output + `exit 1` → `default FAIL [nonzero exit]`) exit 1, cx-first ordering, and bad-disk-floor / unknown-cell → exit 2. `bash -n` clean.

## 1. Goal

A reproducible **driver script** (`scripts/c2c_byte_identity_matrix.sh`) + a **RELEASE checklist** that runs the Rust `coverage2cytosine_rs` and Perl `coverage2cytosine` (v0.25.1) over a **representative flag matrix** on **oxy**, against a **Perl-`bismark2bedGraph`-generated `.bismark.cov.gz`** + the genome FASTA, and asserts **raw-byte-identity Rust≡Perl** (gzip compared after decompression) on every in-scope output stream. A clean pass **gates the `bismark-coverage2cytosine-v1.0` tag** and validates the crate end-to-end as a *genuinely independent* producer for the extractor's Phase H **sub-gate 2** (SPEC §13).

This phase ships **no crate code** — it is a harness + a checklist run. Its "implementation" is the driver script; its "tests" are the matrix cells; its acceptance is a green matrix on oxy.

## 2. Context

- **Where the new code lives:** `scripts/c2c_byte_identity_matrix.sh` in the c2c worktree (`../Bismark-c2c`), a sibling of `scripts/phase_h_se_matrix.sh` / `phase_h_pe_matrix.sh` (the extractor's Phase H harnesses, inherited from `rust/iron-chancellor`). **Model the structure on `phase_h_se_matrix.sh`** — same house pattern: `set -euo pipefail`, bash≥4 guard, SIGINT/TERM trap preserving partial output, ordered pre-flight gates, a `MATRIX_CELLS` array, per-cell run+record, a markdown verdict + summary, and **fail-CLOSED** comparison logic with explicit exit codes. **Do not re-introduce the fail-open bug that script's `count_mbias_rows` fixed (lines ~387-403)** — see §3.4 C1 below for the gzip analogue.
- **Release checklist:** a new sibling **`RELEASE_CHECKLIST_c2c.md`** at the worktree root (kept separate from the extractor's `RELEASE_CHECKLIST.md` so the two crates' release gates stay independent — Q4 resolved). Documents the oxy setup, the cov.gz generation recipe, the matrix invocation, pass criteria, and the tag step.
- **Inputs (both pre-generated, passed as args — the script does NOT generate them, mirroring how `phase_h_*` take the BAM):**
  1. `<COV_GZ>` — a `*.bismark.cov.gz` produced by **Perl `bismark2bedGraph`** from the 10M PE dataset's methylation-extractor output (SPEC §12.3). A *Perl*-generated cov keeps the two c2c producers genuinely independent (SPEC §13; contrast the extractor's Phase G subprocess tautology).
  2. `--genome <DIR>` — the FASTA genome folder (full hg38 for the real-data gate).
- **Binaries:** Perl `coverage2cytosine` (repo-root script, run via oxy's `bismark-test` micromamba env, **v0.25.1**) and Rust `coverage2cytosine_rs` (built `--release` on oxy via rustup — A4).
- **Dependencies on prior phases:** the matrix exercises every Phase B/C/D code path end-to-end; a regression in any surfaces here as a byte-diff. This is the integration test of A–D against Perl.
- **References:** `phase_h_se_matrix.sh` (structure + the fail-open `count_mbias_rows` lesson), SPEC §12.3/§5/§13, `reference_colossal_access` (oxy historical access + the disk-cap rationale), `feedback_dual_driver_back_port` + `feedback_parallel_agent_worktree_isolation` memories.

## 3. Behavior

### 3.1 Pre-flight gates (fail-fast, exit 2 on any) — mirror `phase_h_se_matrix.sh:90-175`
1. **bash ≥ 4** (associative arrays + modern `set -u` idioms). oxy is Linux bash 5.x; macOS dev shells are 3.2 → hard-fail with the brew hint.
2. **`<COV_GZ>` readable**; canonicalize to absolute. Assert the `.gz` suffix (the gate's input contract is a gzipped Perl cov).
3. **`--genome <DIR>` readable** and contains at least one `*.fa`/`*.fa.gz`/`*.fasta`/`*.fasta.gz` (the four-suffix set c2c globs, SPEC §6.1).
4. **`--out <DIR>` empty-or-absent** (never clobber prior evidence; canonicalize).
5. **Perl `coverage2cytosine` present + version == v0.25.1** — assert via its `--version` output. **rev 1 (A-M1 / B-M1):** the string is `Version: v0.25.1` on a `coverage2cytosine`-labelled line — **NOT** the extractor's `Bismark Extractor Version: v0.25.1` format; verify the exact text against the local binary when wiring the grep. Resolve the binary via `PERL_C2C` env or oxy's micromamba `bismark-test` env (Q3).
6. **Rust binary discoverable** — `cargo build --release -p bismark-coverage2cytosine` then locate `target/release/coverage2cytosine_rs` (or accept `RUST_C2C` env). Record crate version + `git rev-parse HEAD`.
7. **Disk-headroom gate (the oxy cap, Q1):** measure free space on the `--out` filesystem (`df -Pk` → GiB via integer division, conservative-safe); **hard-fail (exit 2) if below a configurable floor (`--disk-floor-gb`, default 30)**, with a message explaining the genome-wide-output footprint. Turns "oxy ran out of disk mid-run" from a confusing crash into a clear pre-flight refusal.
8. `export LC_ALL=C` — **rev 1 (B-M2):** belt-and-suspenders. Perl's `sort` and the Rust uncovered-chromosome sort are both already bytewise; `LC_ALL=C` is cheap insurance against any locale-sensitive step (e.g. a stray `sort`/`comm` in the harness itself), not a correctness load-bearer.
9. **tmux/screen advisory** (full-genome matrix is long-running; SSH disconnect would orphan subprocesses).
10. **SIGINT/TERM trap** → message that partial output is preserved in `--out` for evidence; `exit 130`.

### 3.2 Matrix cells (representative subset, NOT full cross-product — SPEC §12.3)

Each cell = `name|rust+perl flags|streams-to-compare`. The `--merge_CpGs` cells respect the Phase-A mutexes (no `--CX`/`--split`/`--threshold` with merge).

**rev 1 (A-I1 / B-C1) — merged/discordant filenames are REPORT-derived**, not stem-derived: Perl strips `.gz` then `.txt` from the report filename and appends the suffix, so for a (non-split, non-CX, plain-or-gz) merge run the report is `{stem}.CpG_report.txt[.gz]` → merged = `{stem}.CpG_report.merged_CpG_evidence.cov[.gz]`, discordant = `{stem}.CpG_report.discordant_CpG_evidence.cov[.gz]`. **The shipped binary is already correct (Phase D `report::merged_cov_name`); only the rev-0 plan's filenames were wrong.** The harness must glob/derive the real names, not hard-code `{stem}.merged_CpG_evidence.cov`.

| Cell | Flags | Output streams compared (Rust≡Perl) |
|------|-------|-------------------------------------|
| `default` | (none) | `{stem}.CpG_report.txt`, `{stem}.cytosine_context_summary.txt` |
| `cx` | `--CX --gzip` | `{stem}.CX_report.txt.gz` (**gzip-integrity + stream-decompress-compare**, §3.4), summary |
| `zero` | `--zero_based` | `{stem}.CpG_report.txt`, summary |
| `gzip` | `--gzip` | `{stem}.CpG_report.txt.gz` (gzip-integrity + decompress-compare), summary |
| `thr` | `--coverage_threshold 5` | `{stem}.CpG_report.txt` (uncovered skipped → small), summary |
| `split` | `--split_by_chromosome` | per-chr `{raw-o}.chr<NAME>.CpG_report.txt` **file-name set + each file** (empty-tolerant, §3.5), last-chr summary |
| `merge` | `--merge_CpGs` | `{stem}.CpG_report.txt`, `{stem}.CpG_report.merged_CpG_evidence.cov`, summary |
| `merge_disc` | `--merge_CpGs --discordance_filter 10` | `…CpG_report.merged_CpG_evidence.cov` + `…CpG_report.discordant_CpG_evidence.cov` + report + summary |
| `merge_gzip` | `--merge_CpGs --gzip` | `…CpG_report.merged_CpG_evidence.cov.gz` (gzip-integrity + decompress-compare), report.gz, summary |

> **Why `--CX` carries `--gzip`:** the full-hg38 CX report is huge (§6); gzipping it + stream-comparing is what keeps the cell within oxy's cap. The decompressed bytes are still asserted identical, so byte-identity is not weakened. **rev 1 (A-I5 / B-N1) — known gap:** this leaves the *plain* (un-gzipped) CX report path un-asserted *on real data*. That path is covered by the §12.2 integration fixtures, and `--gzip` only wraps the writer in a `GzEncoder` (Phase C) — the decompressed-CX byte-identity already pins the report-generation code, so the residual risk is just the gz wrapper, which the fixtures + the `gzip` (CpG) cell exercise. Documented, accepted.

### 3.3 Per-cell execution
For each cell: run **Perl** into `$OUT/cell_<name>/perl/` and **Rust** into `$OUT/cell_<name>/rust/` with identical flags + the same `<COV_GZ>` + `--genome`, both `-o <stem> --dir <celldir>`. Allow non-zero exit (record + continue, like `phase_h_se_matrix.sh:220-229`). Record wall-clock for both (informational perf table, §3.8).

### 3.4 Comparison (fail-CLOSED — the core correctness logic)
For each expected stream in the cell:
1. **Existence + (conditional) non-empty guard FIRST.** A file a cell *should* produce, missing on **either** side ⇒ **FAIL** (never a vacuous pass). The **non-empty** sub-rule is conditional (rev 1 A-C2/B-I1 + A-I2/B-I2):
   - **Required-and-non-empty:** the CpG/CX report + the context summary in `default`/`cx`/`zero`/`gzip`/`thr`; the merged-cov **only in the plain `merge` cell**.
   - **Existence-only (may legitimately be empty):** the `discordant` file (no discordant pairs); the **`merge_disc` merged-cov** (all pairs may route to discordant → 0 bytes — verified live); and **`split` per-chr reports** (a short scaffold's report is validly 0 bytes — verified live: `scaf_short.CpG_report.txt` = 0 B). For these: empty-on-**both** ⇒ PASS, empty-on-**one** ⇒ FAIL.
   This is the c2c analogue of the extractor harness's fail-CLOSED discipline; a naive `cmp` of two missing files would false-PASS.
2. **Byte compare.** Plain → `cmp -s "$R" "$P"`. **gzip (rev 1 A-C1/B-I3 — the fail-open fix):** `cmp -s <(gzip -dc R) <(gzip -dc P)` **does NOT propagate the `gzip -dc` exit status** — two identically-*truncated* `.gz` files PASS as "identical" (both reviewers demonstrated this; disk-full mid-write is exactly the oxy failure mode this gate exists to survive). So: **(a) `gzip -t "$R" && gzip -t "$P"` first** (integrity-test both; any failure ⇒ FAIL), **then (b)** decompress-compare. Optionally also capture `${PIPESTATUS[@]}` / compare decompressed byte-counts as a second belt. The plain CX is never materialized — the decompression is streamed through the pipe.
3. Record per-stream PASS/FAIL into the cell verdict.

### 3.5 `--split_by_chromosome` cell specifics
- Assert the **file-name set** matches between Rust and Perl (`comm`/`diff` of `ls`), like `phase_h_se_matrix.sh:315-320` — a missing/extra per-chr file is a FAIL.
- Compare **each** per-chr report file with the **empty-tolerant** rule (§3.4.1): existence + byte-equality; empty-on-both ⇒ PASS, empty-on-one ⇒ FAIL. (hg38's hundreds of short/unplaced contigs produce legitimately-empty per-chr reports.)
- Context-summary files: assert only the **last-processed chromosome's** summary is non-empty (SPEC §5 Phase-C quirk) and matches Perl; the rest empty on both sides.

### 3.6 Cross-cell differential checks (fail-CLOSED — catch "both binaries silently no-op a flag")
A per-cell Rust≡Perl `cmp` passes even if **both** implementations ignore a flag identically. Guard the flags that must *change* the output (the c2c analogue of the extractor's row-count differential):
1. **`cx` line count > `default` line count** — CX reports every context; CpG-only is a strict subset. **rev 1 (A-I4/B-I4):** compute the CX line count **inside the single decompress-compare pass** of §3.4.2 (`gzip -dc | tee >(wc -l)` or count while streaming) — do **NOT** re-decompress the ~tens-of-GB CX a second time. Compare against the `default` report's line count.
2. **`zero` report ≠ `default` report** — `--zero_based` shifts every coordinate by 1; an *identical* file ⇒ the flag was ignored on both sides ⇒ **FAIL**.
3. **`gzip` decompressed == `default` report** (cross-check: gzip must not alter content).
4. **`thr` line count < `default` line count** — a positive threshold drops uncovered positions/chromosomes.
5. **`merge` merged-cov non-empty** (the plain `merge` cell only — NOT `merge_disc`, whose merged-cov may be empty) and **`merge_gzip` decompressed == `merge` merged-cov**.
6. **`split` file count > 1** (the genome is multi-chromosome).

**rev 1 (A-I4/B-I4) — ordering hazard:** every differential input (line counts, the `default`/`merge` reference bytes) **MUST be stashed during the cell loop, before any purge-on-pass (§3.7) runs.** §3.6 is authoritative: a purged file reading as 0 lines would silently satisfy or break an inequality. The cell loop records `lines_default`, `lines_cx`, `lines_thr`, `merge_cov_nonempty`, and retains the small reference files needed for the equality cross-checks.

### 3.7 Disk discipline + evidence (the oxy constraint)
- After a cell's verdict + its differential inputs are recorded: on **PASS**, purge that cell's large outputs (`*report*.txt*`, `*.cov*`) to free space; on **FAIL**, **keep everything** for investigation. (`--keep-all` overrides.)
- **rev 1 (A-I3) — per-cell disk re-check:** re-run the §3.1.7 free-space check **before each cell** (retained FAIL outputs under `--keep-all` can starve a later cell — especially `cx`). If below floor, fail with a clear message naming the retained evidence consuming space. **Additionally, order the heavy `cx` cell FIRST** so it runs against maximum free space.
- The disk-headroom pre-flight + per-cell re-check + gzip-stream-compare + purge-on-pass are the mechanisms that fit the full-genome matrix into oxy's ~99 GB.

### 3.8 Verdict + exit codes
Write `$OUT/matrix_verdict.txt` (per-cell + per-stream breakdown), `$OUT/byte_identity_summary.md`, and `$OUT/perf_table.md` (informational wall-clock; **not gated** — perf advisory per SPEC §10.7).
- **0** — every cell + every differential check byte-identical / satisfied.
- **1** — any byte-diff, any missing/empty-where-required output, any failed gzip-integrity test, or any differential violation.
- **2** — pre-flight / usage error (bad args, wrong Perl version, insufficient disk).

(No exit-3 perf gate — c2c v1.0 gates byte-identity only.)

## 4. Script interface
```
scripts/c2c_byte_identity_matrix.sh <COV_GZ> --genome <DIR> [options]

  <COV_GZ>            Perl-bismark2bedGraph-generated *.bismark.cov.gz (required, positional)
  --genome <DIR>      FASTA genome folder (required)
  --out <DIR>         output dir (default ./c2c_byte_identity_out; must be empty/absent)
  --cells "a b c"     subset of cell names to run (default: all 9; cx runs first)
  --disk-floor-gb N   pre-flight + per-cell free-space floor (default 30)
  --keep-all          keep large outputs even on PASS (default: purge on pass)
  --perl-c2c PATH     Perl coverage2cytosine (default: $PERL_C2C or repo-root ./coverage2cytosine)
  --rust-c2c PATH     Rust binary (default: build --release + target/release/coverage2cytosine_rs)
  -h|--help

Outputs: <OUT>/cell_<name>/{perl,rust}/, matrix_verdict.txt, byte_identity_summary.md, perf_table.md
Exit: 0 all byte-identical · 1 mismatch/missing/integrity/differential · 2 usage/pre-flight
```

## 5. Implementation outline
1. **Scaffold the script** from `phase_h_se_matrix.sh`: shebang, `set -euo pipefail`, bash≥4 guard, SIGINT/TERM trap, arg parse, `REPO_ROOT` discovery.
2. **Pre-flight gates** §3.1 in order (each exit 2 with an actionable message): the disk-headroom gate (`df -Pk "$OUT" | awk 'NR==2{print int($4/1024/1024)}'` ≥ floor) and the **c2c `--version` assertion** (grep `Version: v0.25.1` on the coverage2cytosine line — verify the exact text against the local binary FIRST).
3. **Build/locate the Rust binary**; record crate version + git HEAD.
4. **Define `MATRIX_CELLS`** (§3.2) as `name|flags|streams`, `cx` first (§3.7). Identical flags for the Perl + Rust invocation of a cell. Derive merged/discordant filenames from the report name (§3.2), not the stem.
5. **Per-cell loop** (§3.3): Perl then Rust into separate dirs; time both; allow non-zero exit. **Stash differential inputs here** (line counts, reference bytes) before any purge.
6. **`compare_stream <rust> <perl> <gz?> <empty-policy>` helper** (§3.4): existence + conditional-non-empty guard; for gz → `gzip -t` both, then stream decompress-compare; record.
7. **Split-cell handler** (§3.5): file-name-set diff + per-file empty-tolerant compare + last-chr-summary rule.
8. **Cross-cell differential checks** (§3.6) from the stashed inputs (CX count from the §3.4.2 pass).
9. **Disk discipline** (§3.7): per-cell re-check; purge-on-pass / keep-on-fail; `--keep-all`.
10. **Verdict + summaries + exit code** (§3.8).
11. **`RELEASE_CHECKLIST_c2c.md`**: oxy setup (rustup install, `micromamba activate bismark-test`, `git pull`); the cov.gz generation recipe (Perl `bismark2bedGraph` on the 10M extractor output, Q2); the matrix invocation; pass criteria; the `bismark-coverage2cytosine-v1.0` tag step (only on exit 0); **the V11 deliberate-diff + truncated-gz self-test as a mandatory pre-trust step.**
12. **Self-test the harness fails-closed** (§9 V11) before trusting any green run.

## 6. Efficiency
- **Disk is THE constraint on oxy (~99 GB).** Mechanisms: (a) `--gzip` on the heavy `cx` cell; (b) **stream-decompress-compare** via process substitution so the plain CX is **never materialized**; (c) **purge-on-pass + per-cell disk re-check** (§3.7); (d) the disk-headroom pre-flight.
- **CX size (rev 1 B-C2 — estimate corrected + de-loaded):** the CX report has one line per genomic C or G ≈ #(C∪G) in hg38 ≈ **~1.2–1.3e9 lines** (Reviewer B estimated ~2.5e9; my rev-0 said ~1B+ — **treat ~1–2.5e9 / ~40–75 GB plain as an *approximate range* and measure it first-session**). The exact plain figure is **largely moot**: with gzip + stream-compare the plain file is never written, so the binding numbers are the **gz size (~10–20 GB) + ~2× peak (Rust+Perl) ≈ ~20–40 GB**, gated by the disk pre-flight + per-cell re-check. If first-session measurement shows it won't fit, the Q1 fallback (chromosome-subset genome for `cx` only) applies.
- **CPU/time:** ~9 cells × (Perl + Rust) full-genome walks; Perl is single-threaded, Rust matches (SPEC §10.7). Hours; run in tmux. Perf reported, not gated. Comparison is O(output bytes) streamed.

## 7. Integration
- **Reads:** the Perl-generated `<COV_GZ>` + `--genome` (unmodified). **Writes:** only under `--out`.
- **Gates:** the `bismark-coverage2cytosine-v1.0` tag (tag only on exit 0).
- **Exercises:** every Phase B/C/D code path end-to-end against Perl — the integration test of A–D.
- **Closes (with #797):** the extractor's Phase H sub-gate 2 becomes a real two-producer comparison once this crate + `bismark-bedgraph` land and the extractor calls them inline (SPEC §13; the inline switch is out of scope — parallel session).
- **Touches no crate source** and no sibling crates.

## 8. Assumptions
**From epic (shared):**
1. Byte-identity to Perl **v0.25.1** is the binding contract for every in-scope stream; STDERR is exempt.
2. Input cov is Perl-`bismark2bedGraph`-generated, 1-based, tab-separated, sorted by chr then pos (SPEC §4).
3. gzip is compared **after decompression**, with a `gzip -t` integrity pre-check (SPEC §15/P10; rev 1).
4. All work in the `../Bismark-c2c` worktree on `rust/coverage2cytosine`; never touch sibling crates.

**Phase-E specific:**
5. **Machine = oxy** (Felix directive 2026-05-30); SPEC §12.3/EPIC §3/memory synced.
6. **oxy access (historical — VERIFY first session, Q3):** `dcli ssh oxy`; micromamba env **`bismark-test`** (Perl Bismark v0.25.1 + `bismark2bedGraph` + samtools/bowtie2); data `~/bismark_benchmarks/`; home ~99 GB. Deprecated 2026-05-28 → may have drifted.
7. **Rust toolchain on oxy:** install via rustup if absent (MSRV 1.89); `cargo build --release`.
8. **The `<COV_GZ>` exists or is generated once** by Perl `bismark2bedGraph` from the 10M PE extractor output (Q2). The script assumes it as input.
9. **Merged/discordant filenames are report-derived** (`{stem}.CpG_report.merged_CpG_evidence.cov[.gz]` etc.) — the harness derives, never hard-codes (rev 1 A-I1/B-C1).
10. **v1.x flag rejections (`--gc`/`--nome`/`--drach`/`--ffs`) are OUT of scope for this real-data gate** (rev 1 B-M3) — they are CLI-validation behavior, covered by Phase A unit tests (SPEC §12.1), not a byte-identity-against-Perl concern.
11. The Perl c2c `--version` string is `Version: v0.25.1` (verify exact text when wiring the assertion).

## 9. Validation
| # | Verify | How | Expected |
|---|--------|-----|----------|
| **V1** | **Harness fails CLOSED (deliberate diff)** | Inject a 1-byte diff into one Rust output (or a line-dropping stub) and run one cell | matrix exits **1**, names cell+stream |
| **V11** | **gzip-integrity fail-CLOSED (truncated gz)** — rev 1 A-C1/B-I3 | Truncate one side's `.gz` and run a gz cell | matrix exits **1** (`gzip -t` catches it), NOT a false PASS |
| V2 | Missing-required output is a FAIL | Stub a cell to emit no report | exit 1 "missing required output", not pass |
| V3 | gzip decompress-compare on valid data | `cx`/`gzip`/`merge_gzip` | decompressed bytes identical; container ignored |
| V4 | `--zero_based` differential | `zero` cell | `zero` report **differs** from `default`; both Rust≡Perl |
| V5 | `--CX` > CpG differential (single-pass count) | `cx` cell | CX line count > CpG; counted in the §3.4.2 pass (no 2nd decompress) |
| V6 | `--coverage_threshold` differential | `thr` cell | `thr` line count < `default`; Rust≡Perl |
| V7 | split: file-name set + per-file + **empty-tolerant** + last-chr summary — rev 1 A-C2/B-I1 | `split` cell incl. a short scaffold | file-name sets match; each file Rust≡Perl; an empty-on-both per-chr report ⇒ PASS; only last-chr summary non-empty |
| V8 | merge + discordance, incl. **empty merged-cov** — rev 1 A-I2/B-I2 | `merge`/`merge_disc` | `merge` merged-cov non-empty + Rust≡Perl; `merge_disc` merged-cov may be empty (existence-only) + discordant Rust≡Perl |
| V9 | disk-headroom pre-flight + per-cell re-check | run with `--disk-floor-gb` above free space; and with `--keep-all` + a forced FAIL to starve a later cell | exit 2 before the starved cell runs |
| V10 | merged/discordant **filename** is report-derived — rev 1 A-I1/B-C1 | `merge` cell on real/fixture data | harness finds `{stem}.CpG_report.merged_CpG_evidence.cov`, not `{stem}.merged_…` |
| V12 | full matrix green = release gate | all 9 cells on the real oxy dataset | exit 0 ⇒ eligible to tag `bismark-coverage2cytosine-v1.0` |

## 10. Questions or ambiguities
| Priority | Question | Resolution / default |
|----------|----------|----------------------|
| **Critical / Open (Q1)** | Does the full-hg38 matrix fit oxy's ~99 GB cap? | **Proceed full-genome with the mitigations (§6); measure CX gz size + peak first-session via the disk pre-flight.** The plain CX is never materialized, so the binding figure is gz (~10–20 GB) + ~2× peak. Fallback if it doesn't fit: a **chromosome-subset genome for the `cx` cell only** (documented in the checklist). |
| **Critical / Open (Q2)** | Does a Perl-`bismark2bedGraph` `.bismark.cov.gz` already exist on oxy, or must it be generated (10M extractor output → `bismark2bedGraph`)? | Assume generate-once; document the recipe in the checklist. Confirm whether a prior oxy run left one. |
| **Open (Q3)** | oxy access details post-deprecation (connection/env/paths). | Use the historical values; **verify first session** before the matrix run. |
| Resolved (Q4) | Checklist placement. | **Separate `RELEASE_CHECKLIST_c2c.md`** — independent crate release gate. |
| Resolved | gzip byte-identity | Decompress-compare **+ `gzip -t` integrity pre-check** (rev 1; SPEC §15/P10). |
| Resolved | Perf gating | **Not gated** for c2c v1.0 (byte-identity only; SPEC §10.7). |
| Resolved | v1.x flag-rejection in scope? | **No** — CLI-validation, covered by Phase A unit tests (rev 1 B-M3). |

**No Critical ambiguity blocks implementation** — the matrix design + comparison contract are fully specified by SPEC §12.3/§5. Q1/Q2 are *operational* (resolved first-session on oxy via the pre-flight + a documented cov-generation recipe); flagged Critical because a wrong assumption wastes a long oxy run.

## 11. Self-Review
- **Logic:** per-cell Rust≡Perl compare + cross-cell differentials cover both "the two disagree" and "the two agree but both no-op a flag". Fail-CLOSED existence guards + the `gzip -t` integrity pre-check close the missing-output and truncated-gz false-PASS holes (rev 1). Empty-tolerant rules (split per-chr, merge_disc merged-cov, discordant) prevent false-FAILs on legitimately-empty outputs. Exit 0/1/2.
- **Edge cases:** empty `discordant` / `merge_disc` merged-cov / short-scaffold split reports (existence-only); truncated gz (V11); the giant CX (gzip + stream-compare, never materialized); split file-name-set drift; disk exhaustion (pre-flight + per-cell re-check + cx-first); wrong Perl version (pre-flight); SIGINT mid-run (trap preserves evidence); empty-cov input (Perl dies exit 255 leaving partial files — V2/informational).
- **Efficiency:** disk is the binding constraint — mitigated four ways (§6); CX line-count folded into the single decompress pass (no double-decompress); comparison O(bytes) streamed.
- **Integration:** touches no crate source; gates the v1.0 tag; exercises all of A–D; independent-producer property holds (Perl-generated cov input).
- **Risks:** (a) oxy disk genuinely too small even with mitigations → Q1 fallback (subset genome for `cx`); (b) oxy access drifted → Q3 first-session verification; (c) the `cx` differential is coarse (confirms the flag *did something*; the same-cell byte-compare pins correctness); (d) the dual-driver fail-open trap — closed by V1 + V11 being **mandatory** before trusting a green run.

## Folded from dual plan-review (rev 1, 2026-05-30 — A APPROVE-WITH-CHANGES, B REQUEST-CHANGES; both live-Perl-verified; core design affirmed by both)
- **C1/I3 (A+B): gzip compare fail-OPEN** — `cmp <(gzip -dc …)` swallows decompress failure; truncated gz false-PASS. Added `gzip -t` integrity pre-check (§3.4.2) + V11.
- **I1/C1 (A+B): merged/discordant filename is report-derived** (`{stem}.CpG_report.merged_CpG_evidence.cov`), not stem-derived. Binary already correct (Phase D); fixed the plan (§3.2, §8.9, V10).
- **C2/I1 (A+B): `split` per-chr reports can be legitimately empty** (short scaffolds). Empty-tolerant rule (§3.4.1, §3.5, V7).
- **I2/I2 (A+B): `merge_disc` merged-cov can be empty.** Non-empty guard scoped to the plain `merge` cell (§3.4.1, §3.6.5, V8).
- **I4/I4 (A+B): `cx` differential** — fold the line-count into the single decompress pass; stash differential inputs before purge (§3.6, §3.7).
- **M1/M1 (A+B): Perl `--version` string** is `Version: v0.25.1`, not the extractor's format (§3.1.5).
- **I3 (A): per-cell disk re-check** + run `cx` first (§3.7).
- **C2 (B): CX disk estimate** corrected to an approximate range + de-loaded (plain never materialized; measure first-session) (§6, Q1).
- **I5/N1 (A+B): plain-CX real-data gap** documented + accepted (§3.2).
- **M3 (A) / M4 (B): `merge_gzip` streams** — added summary + report.gz (§3.2).
- **M2 (B): `LC_ALL=C`** — softened to belt-and-suspenders (§3.1.8).
- **M3 (B): v1.x flag rejections** out of scope (§8.10, Q10).

## Revision history
- **rev 0** (2026-05-30): initial Phase E plan from SPEC §12.3/§5/§13 + the `phase_h_se_matrix.sh` house pattern; retargeted oxy per Felix directive; 9-cell matrix; fail-CLOSED compare + differentials; exit 0/1/2.
- **rev 1** (2026-05-30): dual plan-review folded (A APPROVE-WITH-CHANGES, B REQUEST-CHANGES; both ran live Perl v0.25.1 on the fixtures). 2 consensus Criticals (gzip fail-open; merged/discordant filename) + split-empty + merge_disc-empty + cx-differential ordering + per-cell disk re-check + CX-estimate correction + the `--version` string + plain-CX gap + minors. Added V10/V11; Q4 resolved (separate checklist); v1.x rejection scoped out.

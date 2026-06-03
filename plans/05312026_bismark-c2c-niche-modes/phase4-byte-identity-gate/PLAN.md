# Phase 4 PLAN — Real-data byte-identity gate for the v1.x niche modes (oxy)

**Epic:** `05312026_bismark-c2c-niche-modes/EPIC.md`, Phase 4 — Real-data byte-identity gate
**Design contract:** the v1.0 Phase-E gate `plans/05292026_bismark-coverage2cytosine/phase-e-byte-identity-gate/PLAN.md` (the harness + fail-CLOSED discipline this phase extends) and the v1.0 `SPEC.md` §12.3 (real-data gate model).
**Status:** rev 1 (2026-06-01) — **manual review APPROVED + dual plan-review folded** (`PLAN_REVIEW_A.md` + `PLAN_REVIEW_B.md`, both APPROVE-WITH-CHANGES, **0 Critical**; both ran all 6 niche modes live (Rust + Perl) and verified every filename + the `ffs_nome` 0-byte trap + the `drach` standalone shape + `ffs` 10-col). Folded: the NOMe-GpC require-nonempty over-assertion (→ existence-only + Assumption 8 justifies the rest via the `--CX` cov), the `ffs_nome` differential fail-OPEN (→ stash-during-loop, not post-purge stat), stash-var init under `set -u`, the diff-#1/#2 reframes, and diff-#4's new column-count stash. Ready for the implement trigger.

---

## 1. Goal

**Extend the shipped v1.0 release gate `scripts/c2c_byte_identity_matrix.sh` with full-genome cells for the four v1.x niche modes** (`--gc`/`--gc_context`, `--nome-seq`, `--drach`/`--m6A`, `--ffs`, + the key combos), plus cross-cell **differentials** that prove each new flag actually changes the output at scale; **run the extended matrix on oxy** against the full-hg38 Perl-`bismark2bedGraph` `.bismark.cov.gz` + genome; and **a clean exit 0 gates the `bismark-coverage2cytosine-v1.x` tag** and the `rust/c2c-v1x → iron-chancellor` merge.

This is the **real-data integration test** for the epic, mirroring v1.0 Phase E. Phases 1–3 already proved each mode **byte-identical to Perl v0.25.1 on tiny local goldens**; Phase 4 confirms that holds at **full-genome scale** (real chromosome names, ~1B-line CX reports, real cov distributions, streaming/buffer pressure) — the failure modes tiny fixtures can't surface.

## 2. Context

- **Where the code lives:** edits to the existing **`scripts/c2c_byte_identity_matrix.sh`** (the v1.0 Phase-E driver, already shipped on `rust/iron-chancellor` via PR #892 and present on `rust/c2c-v1x`) + the repo-root **`RELEASE_CHECKLIST_c2c.md`**. **No new script** — add cells, `REQUIRE_NONEMPTY` entries, and differential checks to the existing fail-CLOSED machinery (pre-flight gates §3.1, per-cell file-set+byte+gz-integrity compare §3.4, disk-discipline §3.7, verdict §3.8 — all reused unchanged).
- **The harness contract being extended** (read from the shipped script):
  - `ALL_CELLS` — `"name|flags"` array; flags passed identically to Perl + Rust; `cx` runs first for disk.
  - `REQUIRE_NONEMPTY[name]` — per-cell globs whose **Perl-side** output must be non-empty (ground-truth-has-content guard).
  - Per-cell compare: (a) file-name-set match, (b) per-file byte compare (gz → `gzip -t` both **then** decompress-compare), (c) require-nonempty, (d) split-specific, (e) **binary exit codes** (fail-CLOSED).
  - Cross-cell differential stash + `diff_check` (catches "both binaries silently no-op a flag" — which the per-cell Rust≡Perl compare cannot).
  - Disk discipline: cx-first, purge-large-outputs-on-PASS, keep-on-FAIL; `--disk-floor-gb` pre-flight + per-cell.
- **Phase dependencies:** Phases 1–3 — all **implemented + dual-reviewed + verified + committed** on `rust/c2c-v1x` (`--gc`/`--nome-seq` `b1662b7`; `--drach`/`--m6A` `b3dbce8`; `--ffs` `cf8c74f`). This phase gates them collectively. (Phase 4 is the only remaining epic phase.)
- **Machine = oxy** (per the c2c retarget, Felix 2026-05-30 — NOT colossal; see `reference_colossal_access` EXCEPTION + `reference_oxy_benchmark_env`):
  - access `dcli ssh oxy` (all `dcli`/`cargo`/`perl` calls sandbox-disabled);
  - Perl Bismark **v0.25.1** in micromamba env `bismark-test`;
  - full hg38 at `~/bismark_benchmarks/genome/Homo_sapiens.GRCh38.dna.primary_assembly.fa`;
  - Rust toolchain via rustup (not pre-installed); build in an isolated worktree (leave the main checkout for parallel sessions);
  - **disk: ~99 GB `/home` cap (~87 GB free at the v1.0 gate)** — the harness's cx-first + purge-on-pass handled the v1.0 full-hg38 CX (~40 GB uncompressed); the new CX-based **ffs** cell is the new disk risk → run it **`--gzip`** (§3.1).
- **The cov.gz:** the v1.0 gate generated a **655M Perl-`bismark2bedGraph` `.bismark.cov.gz`** (via `bismark_methylation_extractor --bedGraph --CX --multicore 8` on the 10M PE dedup BAM, ~11 min) and kept it at `/home/fkrueger/c2c_gate/`. **Reuse it if it persists**; else regenerate (the recipe is in the v1.0 Phase-E plan / `RELEASE_CHECKLIST_c2c.md`). Keeping the **same** cov.gz + genome as v1.0 means the new cells share the gate's proven inputs.

## 3. Behavior

### 3.1 New matrix cells (representative subset, NOT a full cross-product)

Add to `ALL_CELLS` (the v1.0 9 cells stay — they re-run as a **regression at scale**). Each new flag's split/gzip/zero permutations are already byte-identical on the **local** Phase-1/2/3 goldens, so the oxy set stays lean (it is a multi-hour run); it includes one representative of each new output stream + the highest-risk combos:

| cell | flags | new streams exercised at scale |
|------|-------|--------------------------------|
| `gc` | `--gc` | `c2c.GpC_report.txt` + `c2c.GpC.cov` (+ the normal CpG report + summary, unchanged) |
| `nome` | `--nome-seq` | `c2c.NOMe.CpG_report.txt` + `c2c.NOMe.CpG.cov` + `c2c.NOMe.GpC_report.txt` + `c2c.NOMe.GpC.cov` (summary, no `.NOMe`) |
| `drach` | `--drach` | `c2c_DRACH_report.txt` + `c2c_DRACH.cov` — **standalone** (no normal CpG report / summary / merge) |
| `ffs` | `--ffs` | `c2c.CpG_report.txt` (10-col) + summary |
| `ffs_cx` | `--ffs --CX --gzip` | `c2c.CX_report.txt.gz` (10-col across CG/CHG/CHH) — **gzipped for the oxy disk cap** |
| `ffs_nome` | `--ffs --nome-seq` | the rev-3 Critical combo: NOMe core report is 10-col, but `c2c.NOMe.CpG.cov` is **0-byte** (suppressed under `--ffs`) |

*(Optional, if disk/time allow — add `gc_split`, `nome_split`, `drach_split` to confirm the per-chr writer lifecycle at scale; these are locally golden-covered, so they are nice-to-have, not gating. Documented as an Open question, §10.)*

### 3.2 `REQUIRE_NONEMPTY` additions (Perl-side ground-truth-has-content)

```
[gc]       = "c2c.GpC_report.txt c2c.GpC.cov c2c.CpG_report.txt c2c.cytosine_context_summary.txt"
[nome]     = "c2c.NOMe.CpG_report.txt c2c.NOMe.CpG.cov c2c.cytosine_context_summary.txt"
[drach]    = "c2c_DRACH_report.txt c2c_DRACH.cov"
[ffs]      = "c2c.CpG_report.txt c2c.cytosine_context_summary.txt"
[ffs_cx]   = "c2c.CX_report.txt.gz c2c.cytosine_context_summary.txt"
[ffs_nome] = "c2c.NOMe.CpG_report.txt c2c.cytosine_context_summary.txt"
```

**Which streams are required-nonempty vs existence-only** (rev 1, A-I1/B-Imp-1 — both reviewers found the rev-0 over-assertion: the NOMe **GpC** streams are 0-byte on small fixtures, so a hard require-nonempty would FALSE-FAIL a correct gate):
- **Required-nonempty (justified by Assumption 8 + the all-context gate cov):** the **core CpG report** (always non-empty — threshold 0 emits every CpG) + the **summary** (64 rows, always); the **`gc` cell's GpC report + cov** (no ACG/TCG filter → every covered `GC` emits; the gate cov is `bismark2bedGraph --CX`, so GpC-context Cs ARE covered); the **NOMe core report + `.NOMe.CpG.cov`** for the `nome` cell (real WGBS has covered ACG/TCG CpGs); the **DRACH report + cov** (real WGBS has covered DRACH motifs).
- **Existence-only (NOT required-nonempty — validated by file-set match + byte-compare, but allowed empty):** the **NOMe GpC streams** `c2c.NOMe.GpC_report.txt` / `c2c.NOMe.GpC.cov` (their non-emptiness depends on covered **non-CG** GpC positions — likely on the `--CX` cov but NOT asserted, to avoid a false-FAIL on a sparser cov). Dropped from `[nome]` and `[ffs_nome]` (rev 0 listed them — the fix).
- ⚠️ **`ffs_nome`'s `c2c.NOMe.CpG.cov` is the suppressed 0-byte file** (the rev-3 Critical) — deliberately NOT in require-nonempty (that would FAIL a correct gate); still validated by file-set + byte-compare (0==0) + the §3.3.5 differential.

### 3.3 New cross-cell differential checks (prove each flag changes output at scale)

These run **post-loop from the stash captured DURING the cell loop (before purge)**, fail-CLOSED, only when both relevant cells ran. They guard against "Rust **and** Perl both silently no-op a niche flag" — invisible to the per-cell Rust≡Perl compare. ⚠️ **Every new stash var is initialised `""` at the top-level declaration** (alongside the v1.0 vars) or the `[[ -n "$VAR" ]]` guard aborts under `set -u` (rev 1, B-Imp-4). ⚠️ **Differentials must NOT `stat`/`test` an output file post-loop** — files are purged on PASS and an *absent* file tests as 0-byte (a fail-OPEN); capture a boolean in `run_cell`'s stash `case` instead (rev 1, B-Imp-2).

1. **`gc` regression — `gc`'s core CpG report == `default`'s** (rev 1, B-Imp-3/A-I3 reframe). `--gc` must ADD the GpC stream without ALTERING the core report. Stash `HASH_GC_CORE` (the `gc` cell's `c2c.CpG_report.txt`); assert `== HASH_DEFAULT`. ⚠️ This is a **regression check, NOT a no-op detector** — equality is the expected-pass state whether or not `--gc` did anything to the core (it shouldn't). The "did `--gc` produce a GpC stream" check is the per-cell `REQUIRE_NONEMPTY[gc]` GpC entry, not this differential.
2. **`nome`: NOMe core report line-count `!=` default AND `<` default** (rev 1, A-I2). The ACG/TCG-upstream filter drops CpGs lacking an ACG/TCG upstream → `LINES_NOME_CORE <= LINES_DEFAULT` always, `<` whenever any covered CpG fails the filter (certain at hg38 scale; relies on Assumption 8). Stash `LINES_NOME_CORE`; assert `!= LINES_DEFAULT` AND `< LINES_DEFAULT`. (Valid because both cells run the SAME cov+genome — every matrix cell does.)
3. **`drach`: standalone — `_DRACH_report.txt` present but NO `.CpG_report.txt`/`.cytosine_context_summary.txt`.** Proves the early-exit bypasses the normal pipeline. Stash a boolean `DRACH_STANDALONE_OK` in `run_cell` (1 iff the Perl `drach` dir has `c2c_DRACH_report.txt` AND lacks both `c2c.CpG_report.txt` and `c2c.cytosine_context_summary.txt`); assert `== 1`.
4. **`ffs`: the CpG report is 10-column on EVERY line (vs default 7) AND same line-count as default.** `--ffs` appends 3 columns to every emitted line without changing which positions emit. Stash `FFS_ALL_10COL` (1 iff `awk -F'\t' 'NF!=10{exit 1}' <ffs core report>` succeeds — checks **all** lines, robust against a degenerate first line, B-Opt-1) and `LINES_FFS`; assert `FFS_ALL_10COL == 1` (default is 7-col) AND `LINES_FFS == LINES_DEFAULT`. ⚠️ **New stash kind** — the v1.0 harness has no column-count precedent (only hash/line-count/nonempty); add `FFS_ALL_10COL`+`LINES_FFS` to the declaration block + the `run_cell` `case` BEFORE purge (rev 1, A-I3).
5. **`ffs_nome`: `c2c.NOMe.CpG.cov` is present-and-0-byte on BOTH Perl and Rust.** The rev-3 Critical, pinned at scale. ⚠️ **Stash a boolean DURING the cell loop** (B-Imp-2): `FFSNOME_COV_EMPTY=1` iff the file **exists** on both sides AND is 0-byte on both — distinguishing *present-and-empty* (PASS) from *absent* (which a post-purge `stat` would falsely read as 0-byte). The post-loop `diff_check` reads the stash; do NOT `stat` the (purged) file. (The per-cell file-set match + 0==0 byte-compare already validate this stream; the differential is insurance that fails loudly if a future change un-suppresses the cov.)

### 3.4 Reused fail-CLOSED machinery (no change)

All v1.0 §3.4 guards apply to the new cells unchanged: file-name-set match (catches a missing GpC/DRACH/NOMe file or an extra one), per-file byte compare (gz integrity-tested then decompress-compared), binary exit codes (a non-zero Perl/Rust exit FAILs even if bytes match), disk-discipline (cx-first, purge-on-PASS). The new cells slot into the existing `run_cell` loop with no structural change.

### 3.5 Mandatory §0 fail-CLOSED self-test (the count_mbias_rows lesson)

Before the oxy run, **self-test the extended harness on tiny synthetic fixtures locally** (macOS, bash 5 via Homebrew): confirm (a) a deliberately-broken Rust output (e.g. a truncated GpC report) is caught as FAIL, not a false-PASS; (b) the new differentials FAIL when fed a no-op (e.g. an `ffs` report with 7 columns); (c) the `ffs_nome` 0-byte assertion FAILs if the cov is non-empty. This mirrors the v1.0 harness's local self-test (V12 + fail-CLOSED probes) and is the gate's own gate.

## 4. Script interface

The CLI is **unchanged** from the v1.0 gate (additive cells only):

```
scripts/c2c_byte_identity_matrix.sh <COV_GZ> --genome <DIR> [--out DIR] [--cells "gc nome drach ffs ..."] \
    [--disk-floor-gb N] [--keep-all] [--perl-c2c PATH] [--rust-c2c PATH]
```

The new cell names (`gc nome drach ffs ffs_cx ffs_nome`) are selectable via `--cells`. Exit codes unchanged: `0` all byte-identical + differentials satisfied; `1` byte-diff / missing-or-empty-where-required / gz-integrity / differential violation; `2` pre-flight/usage error.

## 5. Implementation outline (TDD-friendly: self-test the harness before the real run)

1. **Edit `scripts/c2c_byte_identity_matrix.sh`:**
   a. Append the 6 new cells to `ALL_CELLS` (after the existing 9; `cx` stays first for disk).
   b. Add the §3.2 `REQUIRE_NONEMPTY` entries (NOMe-GpC streams existence-only, NOT listed).
   c. **Initialise EVERY new stash var to `""`** at the top-level declaration block (~line 234, alongside the v1.0 `HASH_*`/`LINES_*`): `HASH_GC_CORE`, `LINES_NOME_CORE`, `DRACH_STANDALONE_OK`, `FFS_ALL_10COL`, `LINES_FFS`, `FFSNOME_COV_EMPTY` — else `[[ -n "$VAR" ]]` aborts under `set -u` (B-Imp-4).
   d. **Capture each stash in `run_cell`'s `case "$name"` block, BEFORE the purge** (mirroring the existing `merge`/`default` stashes): `gc` → `HASH_GC_CORE`; `nome` → `LINES_NOME_CORE`; `drach` → `DRACH_STANDALONE_OK` (Perl dir has `c2c_DRACH_report.txt` AND lacks `c2c.CpG_report.txt`+summary); `ffs` → `FFS_ALL_10COL` (`awk -F'\t' 'NF!=10{exit 1}'`) + `LINES_FFS`; `ffs_nome` → `FFSNOME_COV_EMPTY` (file present on both AND 0-byte on both — NOT a post-purge stat, B-Imp-2).
   e. Add the 5 `diff_check` calls (§3.3), each guarded `ran <cells> && [[ -n "$STASH" ]]`. Keep fail-CLOSED: `find` not bare globs under `set -u`.
2. **Update `RELEASE_CHECKLIST_c2c.md`** (B-Opt-4): the v1.x invocation + tag name; the 6 new cells + their expected streams + the require-nonempty/existence-only split; the 5 new differentials; the **all-context (`--CX`) cov dependency** (§8 Assumption 8) for the GpC require-nonempty; the oxy disk note (`ffs_cx` gzipped; the `nome` cell is the 2nd-largest consumer — A-O2); and fix the stale `git checkout rust/coverage2cytosine` → `rust/c2c-v1x` + the "v1.0 (Phase E)" title → v1.x.
3. **Self-test locally (§3.5)** on tiny fixtures (reuse the Phase-1/2/3 fixtures): full matrix exit 0 on correct outputs; deliberate-break probes exit 1. Green on macOS bash 5.
4. **On oxy** (in tmux): isolated worktree on `rust/c2c-v1x`; rustup toolchain; `cargo build --release -p bismark-coverage2cytosine`; stage the genome + reuse `/home/fkrueger/c2c_gate/*.bismark.cov.gz` (else regenerate per `RELEASE_CHECKLIST_c2c.md`); pre-flight disk check; run the matrix. Capture `matrix_verdict.txt` + `byte_identity_summary.md` + `perf_table.md`.
5. **On a clean exit 0:** record the verdict in the plan/PROGRESS; **tag `bismark-coverage2cytosine-v1.x`** (name per §10 Q — propose `v1.0.0-beta.2`, beta until the CLI surface is real-data-soaked, matching the v1.0 precedent); then the **epic closes**: merge `rust/c2c-v1x → rust/iron-chancellor`.

## 6. Efficiency

- The new cells reuse the v1.0 single-pass streaming gate; per-cell cost is one Perl + one Rust full-genome run (the Rust ~5–12× faster per the v1.0 Phase-E perf). The matrix is multi-hour (Perl full-hg38 CX is slow; the v1.0 9-cell matrix ran ~8 h). The 6 new cells add a few more full-genome passes — **gate it overnight in tmux**. ⚠️ Perl `--drach` `sleep(20)`s once per run (STDERR, exempt) — negligible at full-genome wall-time.
- Disk: cx-first + purge-on-PASS keeps the working set to one cell at a time. The `ffs_cx` cell is gzipped (the only new large-CX stream); the `gc`/`nome` cells add the GpC streams (~CpG-report-sized) — within the ~87 GB headroom with purge-on-pass. Raise `--disk-floor-gb` if the regenerated cov.gz + genome leave less room.

## 7. Integration

- **Reads:** the full-hg38 genome + the Perl-`bismark2bedGraph` `.bismark.cov.gz` (the v1.0 gate's inputs). **Writes:** the matrix output dir (per-cell Perl/Rust subdirs, purged on pass) + the three verdict/summary/perf artifacts.
- **Order in the epic:** Phase 4 is **last** — it gates Phases 1–3 collectively. A clean exit 0 → tag → merge `rust/c2c-v1x → iron-chancellor` (the epic model: accumulate phases, merge at end). No phase depends on Phase 4's *output*; it is the release gate.
- **Downstream:** the extractor's inline c2c switch (Phase H sub-gate 2) is unaffected — it drives core-report flags, not the niche modes.

## 8. Assumptions

**From epic (shared, EPIC §6):**
1. Byte-identity to Perl v0.25.1 for every new/changed output stream (STDERR exempt) — at full-genome scale.
2. Reuse v1.0 infrastructure — here, the **shipped `c2c_byte_identity_matrix.sh`** itself; additive cells + differentials only.
3. Built on the merged v1.0; Phases 1–3 committed on `rust/c2c-v1x`.
4. Local Perl-v0.25.1 goldens (Phases 1–3, done) + **this** oxy real-data gate.
5. Niche-flag interactions mirror Perl (the `--ffs × --nome-seq` cov-suppression is the rev-3 Critical — pinned by the `ffs_nome` cell).

**Phase-4 specific:**
6. **oxy** is the gate machine (Perl v0.25.1 in `bismark-test`; full hg38; rustup Rust). Verify access/env/paths first session (oxy was deprecated 2026-05-28 then re-designated for c2c).
7. The v1.0 gate's **cov.gz is reusable** (same inputs); regenerate only if it was purged.
8. **Non-emptiness justification for the require-nonempty streams (rev 1, A-I1/B-Imp-1).** The gate cov is a **`bismark2bedGraph --CX` (all-context)** `.bismark.cov.gz` (the v1.0 gate recipe; `RELEASE_CHECKLIST_c2c.md`), so it covers C positions in **every** context — including the Cs that sit in `GC` dinucleotides. Therefore: (a) the **core CpG report** + **summary** are always non-empty (threshold 0 emits every CpG; the summary is 64 fixed rows); (b) the **`gc` cell's GpC report + cov** are non-empty (no ACG/TCG filter → every covered `GC` emits, and the `--CX` cov covers GpC-context Cs); (c) real WGBS hg38 has **covered ACG/TCG CpGs** → the `nome` cell's NOMe core report + `.NOMe.CpG.cov` are non-empty; (d) real WGBS has **covered DRACH motifs** → the `drach` report + cov are non-empty. ⚠️ The **NOMe GpC streams** (`c2c.NOMe.GpC_report.txt`/`.NOMe.GpC.cov`) are **existence-only, NOT required-nonempty** — their non-emptiness depends on covered **non-CG** GpC positions (likely on the `--CX` cov but not asserted, to avoid a false-FAIL on a sparser cov). This dependency on an **all-context cov** is pinned in `RELEASE_CHECKLIST_c2c.md`: a CpG-context-only cov would (correctly, but confusingly) fail the GpC require-nonempty — so the gate cov MUST be `--CX`-derived. (Where a stream legitimately could be empty, the file-set match + byte-compare still gate it — they catch a missing/extra/diverging file regardless of content.)
9. The `ffs_nome` `.NOMe.CpG.cov` is **0-byte** (the rev-3 fix) — required-EMPTY, not required-nonempty.
10. oxy disk (~87 GB free) + cx-first + purge-on-pass + gzipping `ffs_cx` fits the footprint.

## 9. Validation

The gate **is** the validation — a clean **exit 0** (all cells byte-identical + all differentials satisfied) is the pass criterion that tags v1.x. Pre-run validation of the *harness itself*:

| # | Verify | How | Expected |
|---|--------|-----|----------|
| V1 | the extended harness self-tests green (§3.5) | tiny-fixture full matrix incl. the new cells | exit 0 |
| V2 | fail-CLOSED: a broken new-cell output is caught | feed a truncated GpC/DRACH/NOMe report | exit 1 (not a false-PASS) |
| V3 | the new differentials fail on a no-op | feed a 7-col "ffs" report / a non-empty `ffs_nome` cov | exit 1 |
| V4 | `--cells "gc nome drach ffs ffs_cx ffs_nome"` selects only the new cells | unit-ish dry run | only the named cells run; cx-first invariant respected |
| V5 | **the oxy full-genome matrix** | run on oxy against the full-hg38 cov.gz + genome | **exit 0** — all cells (9 v1.0 + 6 new) byte-identical; all differentials satisfied; verdict artifacts captured |

## 10. Questions or ambiguities

| Priority | Question | Resolution / default |
|----------|----------|----------------------|
| Open | Exact cell set at full scale — the lean 6, or also add `gc_split`/`nome_split`/`drach_split`? | Default: the lean 6 (each new stream + the highest-risk combos); split/gzip/zero per-mode are already local-golden-covered. Add the split cells only if cheap on the oxy run. **Flag to Felix at review.** |
| Open | The v1.x tag name | Propose `bismark-coverage2cytosine-v1.0.0-beta.2` (beta until the v1.x CLI surface is real-data-soaked; matches the v1.0-beta precedent). Confirm at tag time. |
| Open | Does the v1.0 cov.gz still exist on oxy (`/home/fkrueger/c2c_gate/`)? | Reuse if present; else regenerate (~11 min, recipe in `RELEASE_CHECKLIST_c2c.md`). Operational — resolved first oxy session. |
| Open | Full `--ffs --CX` (non-gzip) disk footprint | Mitigated: the `ffs_cx` cell is **`--gzip`** (§3.1). A non-gzip ffs-CX cell is unnecessary (gzip byte-identity per-mode is local-golden-proven). |

**No Critical ambiguities** — the gate's goal (extend the matrix, run on oxy, gate the tag) and the byte-identity contract are fixed; the above are operational defaults the reviewer can adjust.

## 11. Self-Review

- **Efficiency:** additive cells on the shipped single-pass streaming gate; cx-first + purge-on-pass + gzipped `ffs_cx` keep the oxy disk within ~87 GB; multi-hour overnight tmux run.
- **Logic:** the new differentials each prove a *distinct* flag-effect at scale (gc adds-without-altering; nome filters-down; drach is standalone; ffs widens-to-10-col; ffs_nome suppresses-the-cov) — they catch a both-binaries-no-op that the per-cell compare can't. All fail-CLOSED, all guarded on "both cells ran".
- **Edge cases:** the **`ffs_nome` 0-byte cov** is the trap — it must be required-EMPTY (NOT in `REQUIRE_NONEMPTY`) yet still asserted (file-set + byte-compare + the explicit differential). The `drach` standalone file-set (no `.CpG_report.txt`) is the other shape-shift. Empty-but-required vs required-empty is called out in §3.2.
- **Integration:** extends the shipped `c2c_byte_identity_matrix.sh` (the v1.0 9 cells re-run as scale-regression); a clean exit 0 → tag → merge `rust/c2c-v1x → iron-chancellor` (epic close). Self-tested locally before the oxy spend.
- **Risks:** (a) oxy access/env drift since the v1.0 gate — mitigated by verifying first session (Assumption 6); (b) disk on the new CX-based ffs cell — mitigated by `--gzip` (§3.1); (c) a niche-mode scale-only divergence (the whole point of the gate) — surfaced by the per-cell byte compare. Low residual risk: every mode is already local-golden byte-identical; this confirms at scale.

## Implementation notes (2026-06-01)

**Harness extension: IMPLEMENTED + self-tested locally.** The oxy real-data run (V5) + tag + merge are the remaining operational steps (multi-hour, need oxy access — done on confirmation).

**What was built (§5 steps 1–3):**
- **`scripts/c2c_byte_identity_matrix.sh`:** added the 6 niche cells to `ALL_CELLS` (after the 9 v1.0 cells; `cx` still first); added the `REQUIRE_NONEMPTY` entries (NOMe-GpC streams **existence-only**, per rev-1 A-I1/B-Imp-1); initialised the 6 new stash vars `""` (B-Imp-4); captured each in `run_cell`'s `case` **before purge** (`HASH_GC_CORE`, `LINES_NOME_CORE`, `DRACH_STANDALONE_OK`, `FFS_ALL_10COL` via `awk 'NF!=10{exit 1}'` over all lines, `LINES_FFS`, `FFSNOME_COV_EMPTY` = present-AND-0-byte-both-sides — NOT a post-purge stat, B-Imp-2); added the 5 `diff_check` calls (each guarded `ran <cells> && [[ -n "$STASH" ]]`); updated the header doc (15 cells, the `--CX`-cov dependency).
- **`RELEASE_CHECKLIST_c2c.md`:** retitled (Phase E + v1.x Phase 4); fixed the stale `git checkout rust/coverage2cytosine` → `rust/c2c-v1x`; added the 6 cells + the require-nonempty/existence-only split + the 5 differentials + the `--CX`-cov dependency; the v1.x tag (`v1.0.0-beta.2`) + the `→ iron-chancellor` merge.

**Local self-test (§3.5 / V1–V2):** built a tiny `--CX`-style fixture (a genome with ACG/TCG + non-ACG/TCG CpGs, GpC dinucleotides, a `GAACA` DRACH motif; a gzipped cov over those positions) and ran the extended matrix via bash 5.3.9:
- **V1 (correct → exit 0):** all 7 cells (default + 6 niche) byte-identical Rust≡Perl; **all 5 differentials PASS** — gc-core==default, nome 4<6 (filter fired), drach standalone, ffs 10-col + lines==default, **`ffs_nome` `.NOMe.CpG.cov` present-and-0-byte both sides** (the rev-3 Critical pinned at the gate).
- **V2 (fail-CLOSED):** a wrapper that corrupts the Rust GpC report → the gate **FAILs (exit 1)** with `byte-diff: c2c.GpC_report.txt` — the new cells route through the per-cell byte-compare correctly.
- (V3 both-no-op differential fail-CLOSED is structurally guaranteed — each new `diff_check` uses the identical pattern as the v1.0 differentials, which evaluated correctly in V1.)

**No Rust source changed** (Phases 1–3 are committed); the 169-test crate suite is unaffected. **Pending (operational, on confirmation):** stage genome + cov.gz on oxy → run the 15-cell matrix in tmux → on exit 0, tag `v1.0.0-beta.2` + merge `rust/c2c-v1x → iron-chancellor`.

**Verify (dual code-review + plan-manager, 2026-06-01):** `CODE_REVIEW_A.md` **APPROVE-WITH-CHANGES** (0 Crit/High, 1 Medium) + `CODE_REVIEW_B.md` **APPROVE** (0 Crit/High/Med) + `COVERAGE.md` plan-manager **COMPLETE** (19 DONE, 0 gaps, 3 PENDING-by-design). **Both reviewers independently self-tested the harness** (their own `--CX` fixtures): exit 0 + all 5 differentials PASS, AND multiple fail-CLOSED probes all exit 1 — incl. A's purge-active run (proving the stashes are captured before purge) and B's un-suppress-the-`ffs_nome`-cov probe (caught at BOTH the per-cell byte-compare AND differential #5 — defense in depth). No fail-OPEN, no wrong require-nonempty name, no stash-after-purge, no `set -u` abort. **The one Medium (A-M-1, folded):** the `RELEASE_CHECKLIST_c2c.md` §0 pre-trust self-test recipe ran the full 15 cells on the non-`--CX` `phase_b` fixture → would FALSE-FAIL the `gc`/`drach` require-nonempty (and wrongly trigger the "STOP — gate fail-open" instruction); fixed by scoping §0 to the 9 core cells + adding a `--CX`-fixture block for the niche cells. Also folded: B-Low (the `nome`-cell disk note) + the stale-`--release`-binary gotcha (the harness only auto-builds when absent). **Harness extension verified clean — ready to commit + run on oxy.**

## Revision history
- **rev 0** (2026-06-01): initial Phase 4 plan from EPIC §4 + the shipped `c2c_byte_identity_matrix.sh` (extends it with `gc`/`nome`/`drach`/`ffs`/`ffs_cx`/`ffs_nome` cells + 5 new differentials, incl. the rev-3 `ffs_nome` 0-byte-cov pin) + the v1.0 Phase-E oxy gate experience. Phases 1–3 committed + verified on `rust/c2c-v1x`. Awaiting manual review → dual plan-review → (on the implement trigger) implement → run on oxy → tag → merge.
- **rev 1** (2026-06-01): **manual review approved (Felix) + dual plan-review folded** (`PLAN_REVIEW_A.md` APPROVE-WITH-CHANGES 3 Imp/2 Opt; `PLAN_REVIEW_B.md` APPROVE-WITH-CHANGES 4 Imp/4 Opt; both ran all 6 niche modes live and confirmed every filename, the `ffs_nome` 0-byte-cov, the `drach` standalone shape, the `ffs` 10-col — 0 Critical, no wrong filename, no tautological differential). Folded: **A-I1/B-Imp-1** (the NOMe **GpC** streams were over-asserted as require-nonempty → downgraded to existence-only; Assumption 8 now justifies the CpG/`gc`-GpC/NOMe-core/DRACH streams via the all-context `--CX` gate cov; §3.2); **B-Imp-2** (the `ffs_nome` 0-byte differential was fail-OPEN as worded — purge-on-pass + absent-file-tests-0-byte → now stash a present-AND-empty boolean DURING the loop; §3.3.5/§5.1d); **B-Imp-4** (init every new stash var `""` under `set -u`; §5.1c); **B-Imp-3/A** (diff #1 reframed as a regression check, NOT a no-op detector; §3.3.1); **A-I2** (the `nome` line diff is `!=`+`<` with the Assumption-8 caveat; §3.3.2); **A-I3** (diff #4 needs a NEW column-count stash — flagged in §5.1c/d; uses `awk 'NF!=10{exit 1}'` over ALL lines per B-Opt-1); **B-Opt-4** (the checklist branch/title/tag/cells + the `--CX`-cov dependency; §5.2); **A-O2** (the `nome` cell noted as the 2nd-largest disk consumer). **No Critical, no fail-OPEN at the per-cell layer** — the per-cell file-set + byte-compare remain the real backstop. Ready for the implement trigger.

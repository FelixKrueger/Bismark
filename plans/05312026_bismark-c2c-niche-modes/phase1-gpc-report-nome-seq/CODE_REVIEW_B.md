# Code Review B — Phase 1: GpC report (`--gc`/`--gc_context`) + NOMe-Seq (`--nome-seq`)

**Reviewer:** Code Reviewer B (independent, fresh context, no shared state with Reviewer A)
**Date:** 2026-05-31
**Worktree:** `/Users/fkrueger/Github/Bismark-c2c` (branch `rust/c2c-v1x`), crate `rust/bismark-coverage2cytosine`
**Target:** the **uncommitted working tree** — new `src/gpc.rs`, new `tests/golden_phase1.rs` + `tests/data/phase1/`, modified `src/report.rs`, `src/merge.rs`, `src/cli.rs`, `src/lib.rs`.
**Oracle:** local Perl `./coverage2cytosine` v0.25.1, compared on **my own from-first-principles fixtures** (not the committed goldens).

## Top-line verdict: **APPROVE**

Zero Critical, zero High, zero Medium. The implementation is byte-identical to Perl v0.25.1 across every mode I could devise, including the highest-risk arithmetic (GpC `pos=j+2` walk, chromosome-edge guards, `GCGC` overlap, N-context, chromosome-end GC), the NOMe ACG/TCG upstream filter + `.cov` companion, the documented raw-`-o` filename divergence, the non-contiguous chromosome re-appearance contract (single re-emit / split re-truncate, including a 3× repeat), and the `pct6` promotion (no Phase-D regression). I actively hunted for an off-by-one, a filename mismatch, or a Phase-D regression and found none. The few Low items below are nits/observations only.

## Verification performed

- `cargo test -p bismark-coverage2cytosine` → **131 green** (80 lib + 18 golden_phase1 + 11 B + 7 C + 10 D + 5 sanity). 0 failed.
- `cargo clippy -p bismark-coverage2cytosine --all-targets -- -D warnings` → **clean**.
- `cargo fmt -p bismark-coverage2cytosine -- --check` → **clean** (exit 0).
- Read the Perl ground truth directly: `generate_GC_context_report:751-1073`, the core NOMe hooks `:340-475` + `:620-745`, `handle_filehandles:86-165`, `process_commandline` NOMe block — and confirmed every Rust arithmetic/ordering/filename claim against it line-by-line.

### Independent byte-identity claims re-verified against live Perl v0.25.1

All fixtures below were **built by me from scratch** in `$TMPDIR/c2c_advB`, run through both `perl ./coverage2cytosine` and `target/debug/coverage2cytosine_rs` into separate dirs, gzip-decompressed, and `diff -r`'d. Every one was **byte-identical (file set + contents)**:

| Fixture (genome / cov) | Modes verified | What it stresses | Result |
|---|---|---|---|
| `TGCAGCNAGC`, cov 3,5,6,9,10 | `--gc`, `--nome-seq`, `--gc --zero_based`, `--gc --gzip` | **GC at chromosome END** (top tri <3 drop), **GC near start** (bottom negative-wrap drop), **N-context** (`CNA`→CHH, revcomp `N` passthrough), interior GC | IDENTICAL |
| `GCATGC`/`AGGCGCAATGCAA`/`TTTAGCGC` (3 chr), 9 cov lines | `--gc`, `--gc --split`, `--nome-seq`, `--nome-seq --split`, `--gc --gzip`, `--gc --split --gzip`, `--nome-seq --gzip`, `--gc --coverage_threshold 2` | 3-chr split lifecycle; short scaffold; per-chr writer reopen | IDENTICAL (all 8) |
| `chr1=AGCGCAT` + cov line for `chrZ` (not in genome) | `--gc`, `--nome-seq`, `--gc --split` | **cov chromosome absent from genome** → no bytes | IDENTICAL |
| `AGGCGCAATGCAAGCGC`/`TTACGTTAGCATCGTT`, cov order chr1→chr2→chr1 | `--gc`, `--gc --split`, `--nome-seq`, `--nome-seq --split` | **non-contiguous chr re-appearance** (single re-emit / split re-truncate to last segment) | IDENTICAL |
| same, cov order chr1→chr2→chr1→chr2→chr1 (3× chr1) | `--gc`, `--gc --split` | **triple** non-contiguous re-appearance; split file = last segment only | IDENTICAL |
| `-o sample.CpG_report.txt` (suffixed `-o`) | `--gc`, `--nome-seq`, `--nome-seq --split` | **the documented raw-`-o` filename divergence** (report stem-stripped, all `.cov`/GpC raw-based) | IDENTICAL (file-set divergence confirmed, see below) |
| `AACGTTACGTTACGCATCGAA`, tricky cov (408/400, 11/9, 1/1, 9/0) | `--merge_CpGs`, `--merge_CpGs --discordance_filter 5`, `… 20`, `--merge_CpGs --zero_based`, `--merge_CpGs --gzip`, `--gc --zero_based` | **Phase-D regression from the `pct6` move** + discordance boundary (Δ=4.504950 ≤ 5 merges) + zero-based half-open + the gc-zero discriminator | IDENTICAL (all 6) |
| `NNNNNNNNNN` (no C/G) | `--gc`, `--nome-seq` | all-N genome, empty walk | IDENTICAL |
| `AAATTTAAA`/`GGCGCAA` split, chrA GC-less | `--gc --split`, `--nome-seq --split` | **empty per-chr GpC files still created** with matching names | IDENTICAL |
| `agcAGCgcat` (mixed case) | `--gc` | lowercase genome (upper-cased on load) | IDENTICAL |

Additional point checks:
- **`--gc --zero_based` asymmetry (V20):** confirmed against live Perl the **core** report shifts (`3→2`, `4→3`) while the **GpC** report/cov stay frozen 1-based (`14`). The GpC function has no `$zero` branch — matches.
- **Raw-`-o` NOMe `.cov` divergence (the deviation):** `-o sample.CpG_report.txt --nome-seq` yields, in **both** Perl and Rust: report `sample.NOMe.CpG_report.txt` (stem), but cov `sample.CpG_report.txt.NOMe.CpG.cov`, GpC `sample.CpG_report.txt.NOMe.GpC_report.txt` + `.NOMe.GpC.cov` (all RAW), summary `sample.cytosine_context_summary.txt` (stem, no `.NOMe`). The implementation's deviation from the §3.2.3 prose is **correct** and verified against `handle_filehandles:96-122` (Perl never suffix-strips `$cytosine_coverage_file`).
- **NOMe summary == Perl NOMe summary:** byte-identical on every NOMe fixture. (Note: the A-nit's "NOMe summary can differ from a plain summary" is theoretical — the summary's numeric columns are unaffected by including/excluding uncovered `0,0` positions, and the row enumeration is fixed, so plain-vs-NOMe summaries coincide in practice. Not a correctness concern; Rust reproduces Perl exactly either way.)
- **`pct6` ⇄ Perl `%.6f` parity:** compiled a standalone Rust `format!("{:.6}", …)` and compared to `perl -e 'printf "%.6f"'` over 11 values incl. `50.495050`, `55.000000`, `46.666667`, `0.000100` — **all OK**.

## Focus-area findings

### 1. GpC coordinate arithmetic (`gpc.rs::emit_gpc_dinucleotide`) — CORRECT
`pos = j+2`, top tri `perl_substr(seq, pos-1, 3) = seq[j+1..j+4]`, bottom tri `revcomp(perl_substr(seq, pos-4, 3))` with negative-wrap at chr start, both `len<3` guards skipping the whole dinucleotide, both-context classify-or-skip, bottom-before-top emit order, non-overlapping `j+=2` scan — all match Perl `:848-940`/`:966-1060` exactly. Verified on the GCGC-overlap, chr-start, and chr-end fixtures. The collapse of the two Perl blocks into one kernel is sound (the only Perl difference is coverage-lookup-vs-classify ordering, which is output-identical because every guard is a side-effect-free skip).

### 2. NOMe filtering (`report.rs::emit_position`) — CORRECT
ACG/TCG upstream filter runs **after** `context_reporting` (summary accumulation) and after the CpG-only filter, matching Perl `:361`(threshold)→`:381`(summary)→`:388`(NOMe filter). The summary therefore still counts filtered positions — confirmed by reading the Perl and by the byte-identical NOMe summaries. The `.NOMe.CpG.cov` companion is a POINT coord (`out_pos out_pos`) honouring `--zero_based`, written only under `nome`; no division by zero (threshold ≥1 guarantees `m+u>0`). GpC CG-skip under `nome` verified.

### 3. Filenames — CORRECT (incl. the deviation)
`report_name` uses stem (non-split) / raw+`.chr` (split) for the report; `nome_cov_path` and the GpC paths use raw-`-o` throughout. Independently confirmed against live Perl with a suffixed `-o` that forces report/cov to diverge. The deviation is more faithful than the plan prose and is well-documented in both the code and the plan's rev-2 note.

### 4. `pct6` promotion — BEHAVIOUR-PRESERVING
The body moved verbatim from `merge.rs` to `report.rs` (`pub(crate)`); `merge::round6` and the merge cov writes now call `report::pct6`. All 10 Phase-D goldens green, and my 6 independent merge fixtures (incl. the discordance Δ=5 boundary with 408/400 and 11/9) are byte-identical to Perl. No regression.

### 5. Lifecycle — CORRECT
`lib::run` order is report → merge → gpc (Perl `:44/:58/:82`). Non-contiguous re-appearance: single re-emits, split re-truncates (no per-name writer/buffer caching) — verified to 3× repeats. Uncovered pass gated `threshold==0 && !nome` (matches Perl `:708/:714/:717`). GpC re-reads the cov via a fresh `open_cov` independently of the core report.

### 6. Logic / structure / naming — CLEAN
Clippy/fmt clean. The GpC byte-pushers (`push_gpc_cov`/`push_gpc_report`) mirror the core kernel's manual byte assembly (no `write!`), consistent with the crate's byte-identity discipline. `#[allow(clippy::too_many_arguments)]` is justified and consistent with `emit_position`.

## Issues

### Low
- **L1 (test naming, not a bug):** `golden_phase1.rs::v5_gc_core_report_unaffected` compares two **golden** files to each other (`gc_primary` vs `plain_primary`), not the Rust output to a golden. It's a meaningful invariant check, but it can't catch a Rust core-report regression under `--gc` — `v4` (whole-dir match) is what actually pins the Rust core report. Consider, in a future pass, asserting the Rust `--gc` core report equals the Rust plain core report. Not blocking (v4 + the Phase-B/C goldens cover the Rust core path).
- **L2 (duplication, cosmetic):** the `.cov`-line byte assembly is now written out three times (`gpc::push_gpc_cov`, `report::emit_position`'s NOMe `.cov` block, `merge::write_cov_line`) with the same `chr\tstart\tend\tpct\tm\tu\n` shape. A shared `write_cov_line(out, chr, start, end, pct, m, u)` helper in `report.rs` would remove the triplication. Pure cleanliness — behaviour is identical and each call site is small.
- **L3 (doc nit):** the crate-level doc comment in `lib.rs` still says "**Phase C**" / "`--merge_CpGs` (Phase D) … land next" (lib.rs:14-26), now stale after Phases D + Phase 1. Cosmetic; update when convenient.

## Summary

The Phase-1 port is faithful, well-tested (131 green + an out-of-tree 16-mode scratch sweep, now corroborated by my own independent 30+-comparison adversarial sweep), and byte-identical to Perl v0.25.1 across every edge I probed — including the documented raw-`-o` deviation, which I confirmed is the correct (more-faithful) behaviour. **APPROVE.** The three Low items are optional cleanups; none affect byte-identity or correctness.

# Code Review A — HISAT2 `--multicore N` (Approach B-faithful)

**Reviewer:** A (fresh context)
**Branch:** `rust/aligner-hisat2-multicore` @ worktree `/Users/fkrueger/Github/Bismark-hisat2mc`
**Diff base (true merge-base):** `f1bcf42` (#981). NOTE: the `git diff origin/rust/iron-chancellor` shown in the brief compares against an *advanced* origin (two commits ahead: #982 dedup-empty-input + #983 beta.6 bump) → see Finding L-1.
**Scope:** `config.rs`, `options.rs`, `lib.rs`, `cli.rs`, `tests/cli.rs`, `tests/methylseq_conformance.rs`, `rust/README.md`.

## Verdict: APPROVE

The feature is correct, faithful, well-tested, and matches the Perl oracle byte-for-byte at the position that matters. No Critical or High findings. Three Low/informational notes below; none block merge. I did not mutate any source (shared worktree).

---

## What I verified (the 7 probe targets)

### 1. Byte-identity correctness — PASS (the load-bearing check)
The remap injects `-p N` then `--reorder` at **step 10** of `build_aligner_options` (options.rs:154-166): **after** `--rdg`/`--rfg` (steps 8/9) and **before** `--ignore-quals` (step 11). This is the exact Perl order:
- Perl `bismark` 7998-7999: `push -p $parallel; push --reorder` — gated by `if ($parallel)`, sits after `--rdg`/`--rfg` pushes (7968/7984) and before `--ignore-quals` (8012). The HISAT2 `--no-softclip --omit-sec-seq` delta is pushed much later (8311-8314), so it correctly lands *after* the `-p`/`--reorder` pair.
- Perl mapping confirmed: `-p` → `$parallel` (`'p=i'`, 7348); `--parallel`/`--multicore` → `$multicore` (7361). So the byte-identity oracle for this feature is genuinely Perl `--hisat2 -p N`, which drives the identical `-p $parallel --reorder` push. The Rust remap reproduces that exactly.
- The unit test `hisat2_multicore_remap_emits_p_reorder` asserts the literal `-q --score-min L,0,-0.2 -p 4 --reorder --ignore-quals --no-softclip --omit-sec-seq` — correct position, correct tail.
- The end-to-end test asserts the `_SE_report.txt` contains `-p 2 --reorder`; the report writes `config.aligner_options` verbatim (report.rs:71), matching Perl's report behavior.

### 2. Dispatch route — PASS
`pipeline()` (lib.rs:129-201) branches on `n = config.multicore`. For the remap, `config.multicore` is forced to `1` (config.rs:398-402), so non-combined-index HISAT2 takes the `run_se`/`run_pe` **direct** path (lib.rs:161/197), NOT `parallel::run_*_multicore`. The new end-to-end test `multicore_with_hisat2_routes_to_p_threading` confirms single-instance output (`reads_bismark_hisat2.bam`, no multicore-merge rename) via a fake hisat2 that actually emits a mapped read. `config.multicore` has exactly **one** consumer (lib.rs:133, grep-verified) — no reporting/temp-naming path reads it, so forcing it to `1` is safe.

### 3. `hisat2_multicore_remap` field vs `multicore=1` — PASS (field justified, not harmful)
Sole downstream consumer is the never-silent notice (lib.rs:116-118). `config.summary()` (config.rs:888-935) prints `aligner_options` but never `multicore`/`hisat2_multicore_remap`, so the `multicore==1 && Some(N)` combination is invisible everywhere except the notice. The field is mildly redundant with parsing `aligner_options`, but a typed signal is the right call (cleaner + robust). Not a defect.

### 4. Q3 ambiguity guard is HISAT2-only — PASS
The guard (config.rs:282) is gated on `hisat2_multicore_remap.is_some()`, which is `Some` only for `Aligner::Hisat2 && cli.multicore > 1` (config.rs:247-251). For Bowtie 2 the remap is always `None`, so Bowtie 2 `--multicore` (fork) + `-p` (per-instance threads) does **not** trip it. The negative test `bowtie2_multicore_plus_p_is_not_rejected_by_the_hisat2_guard` is real and asserts the message is NOT "ambiguous". Confirmed legitimate.

### 5. Frozen-path safety — PASS
- Bowtie 2: `hisat2_multicore_remap.is_none()` → `multicore = cli.multicore.unwrap_or(1)` (the Phase-9b fork path, unchanged), and `build_aligner_options` receives `None`.
- minimap2 + single-core HISAT2: `hisat2_multicore_threads` returns `None`.
- Only **one** production caller of `build_aligner_options` (config.rs:353); every other call site (all tests) passes `None` — grep-verified across `src/` and `tests/`. No missed call sites in combined.rs/parallel.rs (they thread `config.aligner_options`).

### 6. `.or()` precedence — PASS
`cli.bowtie_threads.or(hisat2_multicore_threads)` = explicit `-p` wins. For HISAT2 the Q3 guard makes the "both set" case **unreachable in production** (resolve errors first), so the `.or()` precedence branch is documentary-only — the test `hisat2_multicore_remap_emits_p_reorder` notes exactly this. The `< 2` guard (options.rs:159) is inherited from the same block and harmless (the remap only fires for N>1).

### 7. Style / never-silent notice — PASS
Notice text (lib.rs:95-102) is accurate: states the remap, `-p N --reorder`, non-chunk-invariance, byte-identity to Perl `--hisat2 -p N`, and the NOT-equal-to-single-core caveat. Errors use `AlignerError::Validation` (ambiguity guard) — exit code 1, consistent with all resolve rejects (error.rs:4 maps both Validation/Unsupported → 1). Local gates already green (clippy -D, fmt, 392+96+3 tests).

---

## Findings

### L-1 (Low / merge hygiene, informational) — stale base; the `origin`-diff is misleading
The branch sits on merge-base `f1bcf42`; `origin/rust/iron-chancellor` has since advanced by `0d2462a` (#982 dedup empty-input) + `b97a8e2` (#983 beta.6 bump). Consequently the `git diff origin/...` in the brief shows **reverse hunks** that look like this branch downgrades `rust/VERSION` (beta.6→beta.5) and deletes the 2026-06-13 dedup milestone line — it does **not**. Diffing against the true base `f1bcf42` confirms `rust/VERSION` is untouched and the README edits are purely this feature's (methylseq note flip + aligner-row). **Recommendation:** before merge, freshen/rebase onto current iron-chancellor and re-apply the README aligner-row edit + the stop-gap-note relaxation cleanly on top of beta.6, preserving the dedup milestone line. (Matches the project's known freshen-conflict trap on the rust/README Milestones block — build after freshen.)

### L-2 (Low, informational) — error-message priority for `--hisat2 --combined_index --multicore N -p M`
The Q3 ambiguity guard (config.rs:282) runs at resolve before `reject_combined_index_unsupported` (config.rs:334). So for the (nonsensical) combo `--hisat2 --combined_index --multicore N -p M`, the user sees the "ambiguous `-p`/`--multicore`" message rather than the combined-index reject. Both are fail-loud exit-1; this is only a message-precedence nicety, not a correctness issue. `--hisat2 --combined_index --multicore N` (no `-p`) correctly hits the combined-index reject (it reads `cli.multicore` directly, not the forced field). No change required.

### L-3 (Low, informational) — Perl emits extra stderr warnings the Rust notice doesn't mirror
Perl's `-p`/`--reorder` block also emits a `-p > 4` "diminishing returns" warning (7995-7996) and a per-instance "$parallel threads" warning (7998-8004). These are stderr-only (not in `aligner_options`), so they do not affect byte-identity of the BAM/report. The Rust never-silent notice is sufficient and arguably clearer. No action needed — noted only for completeness.

---

## Tests
The two GAP-2 reject tests were correctly flipped to accept/route:
- `tests/cli.rs::multicore_with_hisat2_routes_to_p_threading` — real end-to-end (fake hisat2 emits a mapped CT read), asserts success + `-p 2 threading` notice + `reads_bismark_hisat2.bam` + report `-p 2 --reorder`, and the `--parallel` alias route. Genuine, not a no-op.
- `tests/methylseq_conformance.rs::methylseq_align_hisat2_multicore_now_accepted_via_p_threading` — asserts the GAP-2 reject is gone and `build_aligner_options(.., Some(2))` emits `-p 2`/`--reorder` (fixture-free, mirrors the GAP-1 `--local` flip pattern).
- New config unit tests cover the pure helper truth-table, the Q3 reject, and the Bowtie 2 negative.

Coverage is appropriate and the assertions are load-bearing.

---

**Report path:** `/Users/fkrueger/Github/Bismark-hisat2mc/plans/06132026_aligner-hisat2-multicore/CODE_REVIEW_A.md`

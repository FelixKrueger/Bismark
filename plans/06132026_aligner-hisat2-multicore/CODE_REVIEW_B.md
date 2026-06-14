# CODE REVIEW B — HISAT2 `--multicore N` via Approach B-faithful (`-p N` remap)

**Reviewer:** B (fresh context) · **Date:** 2026-06-14
**Worktree:** `/Users/fkrueger/Github/Bismark-hisat2mc` · **Branch:** `rust/aligner-hisat2-multicore`
**Diff base used by the prompt:** `origin/rust/iron-chancellor` (see Finding H-1 — base mismatch)
**Verdict:** **APPROVE-WITH-NITS**

The core feature is correct, minimal, and faithful to the spike's contract. The route is config-level
(`hisat2_multicore_threads` helper → `build_aligner_options` + forced `multicore=1`), it never silently
swallows the flag (loud stderr notice + the resolved `aligner_options` echoed in both `summary()` and the
report), and the Bowtie 2 fork path + single-core HISAT2 are byte-frozen (every one of the ~31 call sites
passes `None`). No Critical findings. The findings below are quality / coverage / release-hygiene nits.

---

## Probe-by-probe results

### 1. Test quality / non-vacuity — PASS (with one nit)
The e2e `multicore_with_hisat2_routes_to_p_threading` (cli.rs:2078) is **not vacuous on the option path**.
The fake `hisat2` ignores unknown flags, so "the run succeeded" alone would be weak — but the test also
asserts `report.contains("-p 2 --reorder")`, and that string is **load-bearing**:
- The SE run site (`lib.rs:351`, `run_se`) passes `&config.aligner_options` to `SingleAlignerStream::spawn`,
  and the report header (`lib.rs:426`) is built from the **same** `&config.aligner_options`. So asserting the
  report's `aligner_options` line == asserting the exact string handed to the aligner argv (`build_*_argv`).
  The report echo therefore *does* prove `-p 2 --reorder` reached the spawn argv, not merely that exit==0.
- The unit tests (`options.rs::hisat2_multicore_remap_emits_p_reorder`, `config.rs::hisat2_multicore_threads_*`)
  prove the helper + option assembly independently and as hard literals.
- The conformance flip (`methylseq_align_hisat2_multicore_now_accepted_via_p_threading`) is **meaningful**: it
  asserts the GAP-2 reject message is GONE *and* positively asserts `-p 2 --reorder` in the built options —
  not just "no longer rejects." Good.

Nit (Low, N-1): the e2e cannot observe the *fork-vs-single-instance dispatch* directly (the fake doesn't
count invocations). It infers single-instance from the output naming (`reads_bismark_hisat2.bam`, no
multicore-merged rename). That is an acceptable proxy, but a stronger assertion (e.g. an invocation counter in
the fake, or asserting NO per-chunk temp artifacts) would harden it. Covered indirectly by `multicore==1`
being unit-asserted at resolve elsewhere.

### 2. Never-silent notice — PASS
- Fires for **both SE and PE**: emitted in `run()` (`lib.rs:116`) before `pipeline()` dispatch, gated only on
  `config.hisat2_multicore_remap.is_some()` — layout-agnostic. Correct.
- The wrapped format string IS a single line: the `\` line-continuations strip the trailing newline + the
  next line's leading whitespace, so the literal concatenates to `…instance with -p 2 threading (--reorder), …`.
- The e2e asserts `contains("-p 2 threading")`; the emitted string contains exactly `-p 2 threading (--reorder)`
  → substring matches. Verified by hand-tracing `hisat2_multicore_remap_notice(2)`.
- Text accuracy: claims "deterministic and byte-identical to Perl `--hisat2 -p N`" and "NOT identical to
  single-core HISAT2" — matches the spike (`-p N` deterministic-per-N, ≠ single-core). Accurate.

### 3. Edge values — PASS
- `--multicore 0`: `validate_multicore` (config.rs:729, `m < 1`) rejects it *before* the remap is computed
  (called at :265, remap at :279). The cli e2e `multicore_zero_errors` still guards this. Good.
- `--multicore 1`: `hisat2_multicore_threads(Hisat2, Some(1))` → `None` (the `> 1` guard) → no remap →
  single-core. Unit-asserted. Correct.
- The `options.rs` `p < 2` guard can never trip under the remap: remap only produces `Some(N)` for N>1, and
  `cli.bowtie_threads.or(Some(N))` is `Some(≥2)`. (It still protects the explicit `-p 1` user path. Fine.)
- Very large N: passed through verbatim to `-p N` (HISAT2's own ceiling applies). No overflow path.

### 4. PE + the matrix — PASS (coverage nit, see C-1)
The route is config-level and layout-agnostic. PE `--multicore N` → `run_pe` (single instance) with
`-p N --reorder` injected at step 10 (before `--ignore-quals`, the PE `--no-mixed/--no-discordant/--maxins`
tail, and the appended HISAT2 `--no-softclip --omit-sec-seq`). The resulting PE string mirrors Perl
`--hisat2 -p N` exactly (the `-p` lands in Perl's step-10 position; the softclip delta still lands last). The
PE spawn (`lib.rs:2376/2379`) + PE report (`lib.rs:2450`) both read `&config.aligner_options`, same as SE.
`--dovetail` is correctly **not** emitted for HISAT2 (driven by `config.dovetail`, not a scan of
`aligner_options`). No PE-specific correctness concern.

### 5. `--ambig_bam` interaction — PASS
Because the remap forces `config.multicore = 1`, `pipeline()` takes the single-instance `run_se`/`run_pe`
path — `parallel::run_*_multicore` is never reached, so the Perl-Bowtie-2-only multicore ambig temp-name
machinery is never exercised under HISAT2. `--ambig_bam` works exactly as it does for single-core HISAT2 (the
existing `ambig_bam_single_core_hisat2_names_hisat2_token` test covers that path; the route lands on the
identical dispatch). No code assumed `multicore>1` for ambig naming. Confirmed.

### 6. IMPL.md / coverage cross-check — mostly complete, one gap
All 12 plan-coverage rows are implemented. **Gap vs IMPL Task 6:** Task 6 explicitly asks for
"**SE + PE** e2e tests … incl. `--ambig_bam`" under the route. The delivered e2e covers **SE only** (both
`--multicore` and the `--parallel` alias) and there is **no e2e exercising `--ambig_bam` together with the
multicore route**, nor a **PE** `--hisat2 --multicore N` e2e. See C-1 (Low). The behaviour is otherwise
covered by unit/option tests + the (single-core) ambig test + the layout-agnostic dispatch, so this is a
test-coverage nit, not a correctness defect.

### 7. oxy-gate / byte-identity risks — PASS, with release-hygiene findings
- @PG line: the gate must filter the whole @PG block (Rust argv carries `--multicore N`, Perl carries `-p N`);
  IMPL §Final already calls this out. No code issue.
- Report version line: filtered in the gate per IMPL. No issue.
- Ordering: `-p N --reorder` is injected at the identical option-string position as Perl, so per-N parity holds.
- Interaction not in the prompt but worth recording: `--combined_index --hisat2 --multicore N` is **still
  correctly rejected** — `reject_combined_index_unsupported` reads the *raw* `cli.multicore.unwrap_or(1) > 1`
  (config.rs ~:568), not the post-remap `config.multicore`. The remap forcing `multicore=1` does NOT mask the
  combined-index reject. Verified — no regression.

---

## Findings

### High

**H-1 (release hygiene / not a code bug) — branch is based on the stale `f1bcf42`; the diff vs
`origin/rust/iron-chancellor` shows a spurious `rust/VERSION` beta.6→beta.5 "revert" + a dropped Milestones
line.**
The worktree HEAD is `f1bcf42` (the merge-base). `origin/rust/iron-chancellor` has since advanced two commits
(`0d2462a` #982 dedup-empty-input, `b97a8e2` #983 beta.6 bump). So the diff the prompt shows against origin
includes `rust/VERSION` 2.0.0-beta.6→**beta.5**, the README `beta.6`→`beta.5` literals, and the **removal** of
the 2026-06-13 dedup-empty-input Milestones line. **These are NOT changes this feature made** — `git status`
confirms the only working-tree-modified files are the 7 source/test/README files. They are pure base-skew
artifacts. **Action:** rebase the feature branch onto current `origin/rust/iron-chancellor` (or merge it in)
**before** the merge PR, otherwise the squash would silently revert the beta.6 bump and drop the dedup
milestone. This is the exact class of base-skew trap flagged in prior phases — verify on the freshen.
*(Reviewer note: I did not mutate anything — read-only git.)*

### Low

**C-1 — IMPL Task 6 coverage gap: no PE e2e and no `--ambig_bam`-under-route e2e.**
The new e2e is SE-only. IMPL Task 6 asked for SE **+ PE** and a `--ambig_bam` cell on the multicore route.
Recommend adding (a) a PE `--hisat2 --multicore 2` e2e (reuse `make_fake_hisat2_pe`) asserting the report's
`-p 2 --reorder` + the `_PE_report.txt` naming, and (b) a `--hisat2 --multicore 2 --ambig_bam` cell asserting
`*_bismark_hisat2.ambig.bam` is produced via the single-instance path. The behaviour is sound (layout-agnostic
config route), so this is hardening, not a fix.

**N-1 — e2e dispatch assertion is a naming proxy.** `multicore_with_hisat2_routes_to_p_threading` infers
single-instance dispatch from output naming rather than an invocation count. Optional hardening (see Probe 1).

**N-2 — stray untracked test-output files in the crate dir.** `rust/bismark-aligner/reads_bismark_bt2.bam`
and `…_SE_report.txt` are present (untracked). Same litter that bit Phase 9b. Ensure they are NOT staged into
the feature commit (they aren't tracked, so `git add -p`/explicit staging is fine, but a blanket `git add -A`
would catch them). Remove before commit.

---

## Summary
Correct, minimal, faithful, never-silent. The route, the Q3 `-p`/`--multicore` ambiguity guard, the
forced-`multicore=1` single-instance dispatch, the `--ambig_bam` interaction, and the combined-index reject
ordering are all sound. **APPROVE-WITH-NITS.** The one item that must be actioned before merge is **H-1**
(rebase onto current origin so the beta.6 bump + dedup milestone are not reverted by the squash). C-1/N-1/N-2
are quality nits.

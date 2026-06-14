# PROGRESS — HISAT2 multi-core support in the Bismark aligner (v1.x)

**Slug:** `06132026_aligner-hisat2-multicore` · **Plan:** [PLAN.md](PLAN.md)
**Crate:** `rust/bismark-aligner` · **Base:** `rust/iron-chancellor` (`f1bcf42`)
**Worktree:** `~/Github/Bismark-hisat2mc` · **Branch:** `rust/aligner-hisat2-multicore`
**Goal:** Allow `--hisat2 --multicore N>1` (currently rejected at `config.rs:254`). Closes conformance GAP-2.

## Pipeline status

| Step | Status | Notes |
|------|--------|-------|
| Scoping plan written | ✅ Done | `PLAN.md` rev 1 — **Approach B locked**. |
| Manual review | ✅ Done | Felix locked **Approach B** (route `--multicore N` → single instance `-p N --reorder`). Folded findings: existing Perl `--hisat2 -p N` mode + existing Rust `-p`/`--reorder` plumbing → B is cheap; B-strong vs B-faithful gate split; never-silent semantic remap. |
| Dual plan-review | ✅ Done | Both APPROVE-WITH-CHANGES; all findings folded → rev 2. `PLAN_REVIEW_A.md`/`_B.md`. |
| Phase 0 spike | ✅ Done | **B-strong REJECTED, B-faithful CONFIRMED.** `-p N` is deterministic run-to-run but NOT == single-core (HISAT2 threading perturbs splice discovery). No multicore mode is node-independent. `spikes/SPIKE_hisat2_p_determinism.md`. |
| Re-decision (escalated) | ✅ Done | Felix chose **B-faithful (opt-in speedup)** — ship as a loudly-documented opt-in; gate vs Perl `--hisat2 -p N` per N. |
| Implementation plan | ✅ Done | `IMPL.md` (TDD, 6 tasks + oxy B-faithful gate). Single stream. |
| Dual review of IMPL | ⏭️ Waived | Felix triggered `implement` directly after reviewing IMPL.md (scoping plan was dual-reviewed + spike-validated). Documented deviation. |
| Implement | ✅ Done | All 6 IMPL tasks landed; 392 lib + 96 integ + 3 conformance tests green; clippy -D + fmt clean. |
| Dual code-review + plan-manager | ✅ Done | Code-review A APPROVE, B APPROVE-WITH-NITS (0 Critical); plan-manager COMPLETE (17/18, 0 gaps). `CODE_REVIEW_A/B.md`, `COVERAGE.md`. Only nits: freshen-at-merge (stale base #982/#983) + stray files (cleaned). |
| oxy B-faithful gate | ✅ PASS | **7/7 cells byte-identical** Rust `--hisat2 --multicore N` == Perl `--hisat2 -p N` (1M GRCh38, HISAT2 2.2.2): se_dir N∈{2,4,8} (md5s == spike), se_nondir, se_pbat, se_dir+`--ambig_bam` (main+ambig BAM), pe_dir. `GATE_OXY.md`. |
| Merge to iron-chancellor | ☐ Awaiting "merge for me" | Freshen onto current iron-chancellor first (now beta.6 / #982-#983); re-apply README aligner-row+note on beta.6, keep dedup milestone. Then beta.6→beta.7 bump + methylseq pin. |

## Implementation notes (2026-06-13, branch `rust/aligner-hisat2-multicore`, NOT committed)
- **Route (config.rs):** new pure helper `hisat2_multicore_threads(aligner, cli_multicore) -> Option<u32>`
  (`Some(N)` only for `Hisat2 && N>1`); in `resolve`, replaced the `:254` reject with: compute the remap,
  fail-loud if `-p M` also set (Q3), thread the value into `build_aligner_options`, and set the `multicore`
  field to `1` when remapped (single-instance dispatch) + store `hisat2_multicore_remap: Option<u32>` on RunConfig.
- **options.rs:** `build_aligner_options` gained a `hisat2_multicore_threads: Option<u32>` param; the step-10
  `-p` block now uses `cli.bowtie_threads.or(hisat2_multicore_threads)` → `-p N --reorder` lands at the exact
  Perl position. All ~31 existing call sites pass `None` (behaviour-preserving).
- **lib.rs:** never-silent notice (`hisat2_multicore_remap_notice`, pure+tested) emitted when the remap fires.
- **Tests:** +3 config unit (helper mapping, Q3 reject, Bowtie2-not-tripped), +2 options unit (emits `-p N
  --reorder`, Bowtie2 unaffected); flipped conformance `..._now_accepted_via_p_threading` + cli.rs e2e
  `multicore_with_hisat2_routes_to_p_threading` (asserts success + report `-p 2 --reorder` + the notice).
- **Docs:** README stop-gap relaxed (no cpus-cap needed) + aligner-row + 2 stale src doc-comments fixed.
- **Frozen:** Bowtie 2 `--multicore` (fork, Phase 9b) + single-core HISAT2 + minimap2 — all callers pass
  `None`; no behaviour change (full suite green proves it).
- **Deviation:** dual-review-of-IMPL waived (Felix triggered implement directly). Batched test+impl per task
  (Rust compile model) rather than strict per-test RED runs, but every behaviour is asserted by a real test.

## Implementation shape (IMPL.md)
Route in `resolve()`: when `aligner==Hisat2 && multicore>1` → compute `hisat2_p_threads=Some(N)`, fail-loud
if `-p M` also set (Q3), pass into `build_aligner_options` (emits `-p N --reorder`), set `config.multicore=1`
(so `lib.rs:144/180` takes the single-instance `run_se`/`run_pe` path, NOT `parallel::run_*_multicore`).
Remove the `config.rs:254` reject; never-silent stderr notice; flip the conformance test; relax README.
Bowtie 2 multicore (Phase 9b) + single-core HISAT2 byte-frozen. Gate: Rust `--multicore N` == Perl `-p N` per N (BAM @PG-filtered + report).

## ⚠️ Spike overturned B's premise (2026-06-13)
- `-p N --reorder` (single instance) is **deterministic per-N** ✅ but **≠ single-core** ❌ — record count
  844,267→844,316 and spliced 1310→1298 as N grows; HISAT2 threading itself perturbs alignments.
- **B-strong (== single-core, node-independent) is impossible.** Only **B-faithful** (== Perl `--hisat2 -p N` per N) remains — deterministic + Perl-faithful but **N-dependent** (node-size-dependent under methylseq's auto-derived N).
- Perf: `-p 8` ~24% faster than single-core; `--multicore 4` (fork) fastest (1:14) but most variant (1237 spliced).
- A is also variant + fragile (Rust contiguous vs Perl modulo chunking). Stop-gap (single-core) is the only node-independent path.

## The decision (LOCKED — Approach B)
Route `--hisat2 --multicore N` → **one** HISAT2 instance with `-p N --reorder` (whole read set,
threaded) instead of fork+chunk-split. Deterministic, node-independent. Two candidate gates, spike picks:
- **B-strong:** byte-identical to single-core `--hisat2` (needs `-p N` content == `-p 1`). Node- AND N-independent.
- **B-faithful:** byte-identical to Perl `--hisat2 -p N` per N (needs only Rust↔Perl `-p N` parity).
- If `-p N` is non-deterministic run-to-run → no clean gate → escalate/defer (stop-gap suffices).

The fork+chunk path (Approach A, Perl `--multicore N --hisat2`, worker-variant 1310 vs 1219) is
**rejected as the target** — it stays Bowtie 2-only. B is a documented `--multicore`→`-p` semantic remap.

## Key enrichments (manual review, vs rev 0)
- Perl already ships faithful `--hisat2 -p N` (`bismark:7998-7999`, pushes `-p N` + `--reorder`).
- Rust `-p`/`--reorder` plumbing exists + is NOT Bowtie 2-gated (`options.rs:149`) → `--hisat2 -p N` already runs.
- So B reuses existing machinery (route, not rebuild) and has a faithful Perl oracle.

## Interim stop-gap (already shipped)
`rust/README.md:64-72` documents capping the align step below 6 CPUs so methylseq doesn't auto-add
`--multicore` (keeping `bismark_hisat` single-core). Unblocks users until this lands.

## Flip-detector handoff
When shipped, `bismark-aligner/tests/methylseq_conformance.rs::methylseq_align_hisat2_multicore_known_unsupported`
flips → move to accept + add the chosen gate; relax the README cpus-cap note.

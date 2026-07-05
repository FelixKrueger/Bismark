# PROGRESS — HISAT2 `--local` alignment in the Bismark aligner (v1.x)

**Slug:** `06142026_aligner-hisat2-local` · **Plan:** [PLAN.md](PLAN.md)
**Crate:** `rust/bismark-aligner` · **Base:** latest `rust/iron-chancellor` (incl. beta.8 / #986)
**Goal:** Support `--hisat2 --local` byte-identical to Perl v0.25.1 `--hisat2 --local` (currently rejected at `config.rs:295`). minimap2-`--local` stays rejected (non-mode).

## Pipeline status

| Step | Status | Notes |
|------|--------|-------|
| Scoping plan written | ✅ Done | `PLAN.md` rev 2 (decisions locked + dual-review findings folded). |
| Manual review | ✅ Done | Felix: Q1 **advance now**, Q2 **SE+PE together**, Q3 **keep minimap2-local rejected + "local by design"**. |
| Dual plan-review | ✅ Done | Both APPROVE-WITH-CHANGES (`PLAN_REVIEW_A/B.md`). 🔴 1 Critical (both): the load-bearing edit is `score_min_params(cli, aligner)` (G/L branch), NOT calc_mapq — folded into rev 2. Important: operationalize soft-clip non-vacuity + mandatory `(0,−0.2)` Perl-cross-checked MAPQ test; flip stale docs/reject-test. Confirmed: reframing sound, Assumption 6 sound, no spike needed. |
| Implementation plan | ✅ Done | `IMPL.md` (TDD, 6 tasks + oxy gate). Critical (score_min_params aligner branch) = Task 2. |
| Dual IMPL-review | ✅ Done | Both APPROVE-WITH-CHANGES (`IMPL_REVIEW_A/B.md`); all folded into IMPL "IMPL-review delta". 🔴 A1: Task 1 RED was unachievable (assert "not the local-reject err", not Ok). A4+B1: Task 4 MAPQ test targeted the wrong (vacuous) buckets → now the secBest exact-equality leaf + PE summed-ln. B2+B3: gate adds local-vs-end-to-end cross-check + concrete soft-clip dataset. **B4: base = origin/rust/iron-chancellor beta.9 (478c974), NOT worktree HEAD; next beta = beta.10.** |
| Implement | ✅ Done | Fresh worktree `~/Github/Bismark-hisat2local`, branch `rust/aligner-hisat2-local` off `origin/rust/iron-chancellor` (beta.9, `478c974`). All 6 tasks landed; **394 lib + 97 integ + 3 conformance green; clippy -D + fmt clean.** |
| Dual code-review + plan-manager | ✅ Done | Code-review A+B APPROVE-WITH-NITS (0 Crit/High); plan-manager INCOMPLETE→**RESOLVED** (the 1 PARTIAL = SE-only e2e; closed by adding the PE soft-clip e2e). 5 nits fixed: PE e2e, HISAT2-local G-form error msg (M1), 3 stale comments (A-L1/B-L1/A-L2). Re-verified: 394 lib + 98 integ + 3 conformance green, clippy -D + fmt clean. `CODE_REVIEW_A/B.md`, `COVERAGE.md`. |
| oxy byte-identity gate | ✅ PASS 7/7 | Rust `--hisat2 --local` byte-identical to Perl `--hisat2 --local` (1M GRCh38, HISAT2 2.2.2): se_dir/se_nondir/se_pbat + se_dir_mc (`--local --multicore 4`==Perl `--local -p 4`) + pe_dir, all md5-match (28k+ soft-clips/SE cell). **Non-vacuity decisive: end-to-end=0 soft-clips vs local=28,222.** Q4 determinism PASS. `GATE_OXY.md`. |
| Merge to iron-chancellor | ☐ Awaiting "merge for me" | Branch off beta.9 (no freshen needed unless iron-chancellor moved). On merge: cut beta.10 + bump methylseq pin. |

## Implementation notes (2026-06-14, branch `rust/aligner-hisat2-local`, NOT committed)
- **config.rs:** `--local` reject lifted `aligner != Bowtie2` → `aligner == Minimap2` (Bowtie 2 + HISAT2 pass; minimap2 rejects "…local (soft-clipping) alignment **by design**…"); `score_min_params(cli)`→`(cli, aligner)`; `score_min_local` doc + reject-block comment (A3) de-staled.
- **options.rs (🔴 the Critical):** `score_min_params(cli, aligner)` — G-form `(20,8)` only for `cli.local && Bowtie2`, else L-form `(0,−0.2)`. The `cli.local` options block narrowed to `&& Bowtie2` (HISAT2-local falls into the L-form `else`, no `--local`). HISAT2 tail: `--omit-sec-seq` only when local (drop `--no-softclip`).
- **mapq.rs:** no production change (`calc_mapq` sign-agnostic); module doc de-staled; new `local_hisat2_default_params_mapq` — the `(0,−0.2)` sub-unity-`diff` regime incl. the ln-ULP-sensitive PE leaf (150+150 → **34**) + 44/22/40/0, Perl-hand-computed (A4+B1).
- **Tests:** `score_min_params_aligner_and_mode_defaults`, `hisat2_local_option_string` (exact SE+PE strings, Bowtie2-local byte-frozen regression), `resolve_local_aligner_scope` (A1/A2), `hisat2_local_softclip_roundtrip_and_options` e2e (2S4M round-trips) — all pass.
- **Docs:** README local note + cli.rs `--local` help flipped. **Frozen (green suite proves):** Bowtie 2-local #981, HISAT2 end-to-end, single/multi-core HISAT2, minimap2.
| Phase-0 spike | ⏭️ Not needed | The only transcendental risk (`ln()` Perl≡Rust parity) was already retired by the #981 Bowtie2-`--local` spike (0 ULP); HISAT2-local uses the same `ln()`. |
| Implementation plan | ☐ Blocked | After review. |
| Implement | ☐ Blocked | Explicit trigger only. |
| oxy byte-identity gate | ☐ Blocked | vs Perl `--hisat2 --local`, SE+PE × {dir,non-dir,pbat}, must include soft-clipped reads (non-vacuous). |

## The key reframing (PLAN)
HISAT2-local is **NOT** a `--local` passthrough (Perl doesn't push `--local` to HISAT2). It is: same
`--score-min L,0,-0.2` + `--omit-sec-seq` as end-to-end, but **drops `--no-softclip`** (allows soft-clips),
+ local `ln(readLen)` MAPQ scMin with `(0,−0.2)` L-form. The local MAPQ ladder + ln scMin + soft-clip-as-`I`
all already exist (#981); the new work = the HISAT2-local options/validation branch + lifting the reject.

## minimap2-`--local` — stays rejected (correct)
minimap2 has no end-to-end-vs-local distinction (inherently soft-clipping); Perl has no minimap2-`--local`
handling. The fail-loud reject is the right never-silent behavior; not in scope to "support".

## Reuse from #981 (Bowtie 2 `--local`)
local MAPQ ladder + `ln()` scMin (`mapq.rs`, byte-exact + parity-proven), soft-clip-as-`I`
(`methylation.rs:174`), `--non_bs_mm` ⊕ local mutex. New = HISAT2-local option delta (drop `--no-softclip`,
L-form score-min, no `--local`) + reject-lift (`config.rs:295`: `aligner != Bowtie2` → `aligner == Minimap2`).

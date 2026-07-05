# Plan Coverage Report

**Mode:** B (code vs implementation plan)
**Plan(s):** `plans/06142026_aligner-hisat2-local/IMPL.md`
**Date:** 2026-06-14
**Verdict:** INCOMPLETE — 1 item unresolved (Task 5 PE arm)

## Summary

- Total items: 11
- DONE: 9
- PARTIAL: 1 (Task 5 / checklist item 8 — SE present, PE arm missing)
- MISSING: 0
- DEVIATED: 0
- PENDING (not a gap): 1 (item 10 — oxy gate, not yet run by design)

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | Reject lift (Minimap2-only) + minimap2 "local by design" msg | Task 1 / checklist 1 | DONE | `config.rs:298-306`: gate is `aligner == Aligner::Minimap2`; HISAT2 falls through. Message contains "by design". |
| 2 | Amend reject test (HISAT2 OK / minimap2 Err) — A1 shape, A2 msg assert | Task 1 / checklist 2 | DONE | `config.rs:1064` `resolve_local_aligner_scope`: HISAT2 path asserts NOT a `--local`/minimap2 reject (accepts Ok or non-local Err, never asserts `Ok`); minimap2 asserts Err `.contains("--minimap2")` && `.contains("by design")`. Matches A1 + A2. |
| 3 | `score_min_params(cli, aligner)` G/L branch (🔴 Critical) | Task 2 / checklist 3 | DONE | `options.rs:353` new signature; branch `if cli.local && aligner == Aligner::Bowtie2 { ("G,",(20,8)) } else { ("L,",(0,-0.2)) }`. Call site `config.rs:364` passes `aligner`. |
| 4 | Update `score_min_params` test (signature + L-form HISAT2-local) | Task 2 / checklist 4 | DONE | `options.rs:521` `score_min_params_aligner_and_mode_defaults`: Bowtie2-local→(20,8) + G parse; HISAT2-local→(0,-0.2), accepts `L,0,-0.6`, rejects `G,20,8`; end-to-end (both aligners)→(0,-0.2). |
| 5 | options local block: HISAT2 L-form, no `--local` | Task 3 / checklist 5 | DONE | `options.rs:82` `if cli.local && aligner == Bowtie2` pushes `--local`+G-form; else L-form, no `--local`. `debug_assert_eq!` removed. New test `hisat2_local_option_string` (`:598`) asserts no `--local`. |
| 6 | HISAT2 softclip tail: drop `--no-softclip` for local | Task 3 / checklist 6 | DONE | `options.rs:322-328`: `if cli.local { push "--omit-sec-seq" } else { push "--no-softclip --omit-sec-seq" }`. |
| 7 | Mandatory `(0,−0.2)` Perl-cross-checked MAPQ test (sub-unity diff) — A4+B1 | Task 4 / checklist 7 | DONE | `mapq.rs:392` `local_hisat2_default_params_mapq`: SE no-secBest 44/22 (B1 reachability note); SE `best_over==diff` equality leaf (0,Some(-1))→40; **PE summed-ln interior leaf** `calc_mapq(150,Some(150),0,Some(-1),..,true)→34` (the ln()-ULP-sensitive case, A4+B1); cross-check loop vs `calc_mapq_local` over lengths 40/50/75/100/150. Expectations are hand-applied Perl ladder, not self-consistency. |
| 8 | e2e fake-HISAT2-local soft-clip round-trip (SE + PE) | Task 5 / checklist 8 | **PARTIAL** | **SE present, PE arm missing.** See Gaps. |
| 9 | Docs flips (README/cli/config) — incl. A3 config:291-294 comment, B5 byte-frozen tests | Task 6 / checklist 9 | DONE | `README.md:58-66` flipped to Bowtie 2 **and** HISAT2 supported, minimap2 "by design"; `cli.rs:169-171` `--local` help aligner-conditional; `config.rs:178-181` `score_min_local` doc aligner-dependent; `config.rs:292-297` reject-block comment (A3) updated; `options.rs:343-352` docstring updated. B5: byte-frozen Bowtie2-local + HISAT2 end-to-end option-string regression assertions retained (`hisat2_local_option_string` includes the Bowtie2-local `--local`+`G,20,8` regression; existing end-to-end tests unchanged). |
| 10 | oxy gate SE+PE × dir/non-dir/pbat + soft-clip non-vacuity + `--multicore` cell | Final | PENDING | Not yet run — per prompt, mark Pending, not a gap. `GATE_OXY.md` absent (expected). |
| 11 | `--non_bs_mm` / hard-clip orthogonal — documented no-ops | — | DONE (no-op) | No code action required; consistent with plan. |

## Gaps (detail)

### Item 8 (Task 5): e2e soft-clip round-trip — PE arm missing

**Expected:** IMPL Task 5 title reads "fake-HISAT2-local soft-clip round-trip **(SE + PE)**"; the task body ends "**SE + PE.**" (IMPL.md:98, :104); coverage-checklist item 8 reads "(SE + PE)" (IMPL.md:44). The plan mandates both a single-end and a paired-end fake-HISAT2-local soft-clip end-to-end test.

**Found:** A single SE test, `hisat2_local_softclip_roundtrip_and_options` (`tests/cli.rs:2020`), driven by a SE-only fake aligner `make_fake_hisat2_local_softclip` (`:1996`, handles only `-U`). It asserts: exit 0, the SE report echoes `-q --score-min L,0,-0.2 --ignore-quals --omit-sec-seq` with no `--local`/`--no-softclip`, and the `2S4M` soft-clip CIGAR round-trips into the BAM. No PE counterpart exists (no `-1`/`-2` fake, no PE report/BAM assertion). A PE HISAT2 fake harness already exists in the file (`make_fake_hisat2_pe`, `:2260`; `hisat2_pe_mapped_names_and_report`, `:2288`), so the PE arm was feasible.

**Gap:** Add a paired-end fake-HISAT2-local soft-clip e2e test — a PE fake aligner emitting a soft-clipped CIGAR (`2S4M`) for `-1`/`-2`, run `--hisat2 --local` PE, and assert the PE report echoes the same HISAT2-local delta (no `--local`, no `--no-softclip`) and the soft-clipped CIGAR round-trips into the PE BAM.

## Test verification (Mode B)

Test pass status trusted per prompt (394 lib + 97 integ + 3 conformance pass; clippy + fmt clean). Existence/shape verified by reading source.

| Test name | File | Status |
|-----------|------|--------|
| resolve_local_aligner_scope | src/config.rs:1064 | PRESENT (Task 1, A1+A2 shape) |
| score_min_params_aligner_and_mode_defaults | src/options.rs:521 | PRESENT (Task 2) |
| hisat2_local_option_string | src/options.rs:598 | PRESENT (Task 3 + B5 Bowtie2 regression) |
| local_hisat2_default_params_mapq | src/mapq.rs:392 | PRESENT (Task 4, A4+B1 SE equality leaf + PE summed-ln) |
| hisat2_local_softclip_roundtrip_and_options (SE) | tests/cli.rs:2020 | PRESENT (Task 5 SE) |
| hisat2_local_softclip_roundtrip PE variant | tests/cli.rs | **MISSING** (Task 5 PE arm) |

## Verdict

**INCOMPLETE — 1 item unresolved.**

Everything in Tasks 1–4 and 6, plus the SE half of Task 5 and all folded IMPL-review delta items (A1, A2, A3, A4+B1, B5) and the documented no-ops (item 11), are DONE as specified. The codebase matches the plan's seam table and coverage checklist.

The single remaining gap: **Task 5 / checklist item 8 specifies an SE *and* PE soft-clip e2e round-trip, but only the SE test was added.** The paired-end arm (PE fake-HISAT2-local emitting a soft-clipped CIGAR, plus PE report/BAM assertions) is missing.

The oxy byte-identity gate (item 10, Final verification) is correctly Pending — not counted as a gap.

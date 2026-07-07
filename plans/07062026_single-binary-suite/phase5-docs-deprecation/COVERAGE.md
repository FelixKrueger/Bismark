# Plan Coverage Report

**Mode:** B (committed code vs. plan)
**Plan(s):** `07062026_single-binary-suite/phase5-docs-deprecation/PLAN.md` (Rev 2)
**Implementation:** branch `rust/phase5-docs`, commit `43a3e93` (26 files) — `git diff master...rust/phase5-docs`
**Date:** 2026-07-07
**Verdict:** INCOMPLETE — 1 minor journaling item (all user-facing/substantive scope DONE)

## Summary

- Total ledger items: 33
- DONE: 31
- PARTIAL: 1 (step 9 — the new dated Milestones journal entry is missing)
- MISSING: 0
- DEVIATED (documented / benign): 1 (rust/README.md L6 header — deliberate retirement note; acceptable)

All Behaviors A–H, every resolved decision (OD-1/2/3), both Criticals (C1/C2), all Importants (I1–I7), and A's unique catches are implemented. The sole open item is a one-line historical-journal entry that is conventionally added at merge time. No user-facing docs gap.

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | Invocation pivot — runnable code-fence commands → canonical `bismark <sub>` | §3.A r1 | DONE | V2 clean: zero classic/`_rs` tokens inside code fences across all touched pages |
| 2 | First-mention alias in parens `(classic alias: X)` | §3.A r2 | DONE | usage/{concordance,coverage-report,dedup,filtering,genome-prep,extraction,processing,summary}.md |
| 3 | Proper-noun prose readable | §3.A r3 | DONE | Minor: overview.md L78/85/87 keep `coverage2cytosine` as a module/proper-noun — permitted by r3, slightly under-pivoted (non-blocking) |
| 4 | Flags/args/paths/values unchanged | §3.A r4 | DONE | Only the executable token changed |
| 5 | Output filenames unchanged | §3.A r5 | DONE | `.cov.gz`, `*_splitting_report.txt`, assets untouched |
| 6 | `_rs`-suffixed commands → canonical | §3.A r6 | DONE | usage/alignment.md + rust/README.md `bismark_rs`/`*_rs` commands all pivoted; residual `_rs` = 2 (both in the rust/README binary-naming *explanation*, not commands) |
| 7 | Heading rename + inbound anchors in lockstep (C1) | §3.A r7 / C1 | DONE | See #10; V7 anchor scan passes |
| 8 | Invocation pivot coverage: options/*, usage/*, faq/*, quick-reference, index.mdx, rust/* | §3.A / §4.4–6 | DONE | index.mdx, usage/library-types.md, faq/index.md, faq/low-mapping.md verified to carry **zero** invocation hits → correctly untouched |
| 9 | installation.md: opener + frontmatter `description:` → GA | §3.B-1 / §3.G | DONE | Both the body opener and the frontmatter description rewritten to the Rust-GA framing |
| 10 | installation.md: rename `## Bismark Rust suite (beta)` → drop beta + update 2 inbound anchors | §3.B-2 / C1 | DONE | `## The Bismark Rust suite` (#the-bismark-rust-suite); overview.md:6 + benchmarks.md:6 both updated; zero lingering `#bismark-rust-suite-beta` |
| 11 | installation.md: remove `_rs` suffix scheme entirely | §3.B-3 | DONE | Prebuilt + container sections no longer mention `_rs`; the "at v1.0 the suffix is dropped" note removed (now realized) |
| 12 | installation.md: 12-crate `cargo install` → one-crate `cargo install bismark` | §3.B-4 / V4 | DONE | One `cargo install bismark` block + `--git … --branch master --locked bismark` dev build; prereqs preserved |
| 13 | installation.md: dead `--branch rust/iron-chancellor` → `--branch master` | §3.B-5 | DONE | |
| 14 | installation.md: container tags `:beta`/`:2.0.0-beta.13` → `:latest`/`:<version>` version-agnostic | §3.B-6 / OD-2 | DONE | `:latest` + `:<version>` with `<!-- TODO(phase6): version -->` marker |
| 15 | installation.md: prebuilt tarball ships `bismark` + classic-name symlinks | §3.B-7 | DONE | L32 |
| 16 | Legacy section + self-anchor | §3.B-1 | DONE | New `## Legacy: the Perl Bismark` (#legacy-the-perl-bismark); intro self-link resolves |
| 17 | rust/overview.md — beta→GA + single-crate/modules reframe + anchor fix | §3.C | DONE | "public beta"→GA present tense; "shared bismark-io library" → "single `bismark` crate … `bismark::io` module"; item 6 reframed; `#the-bismark-rust-suite` |
| 18 | rust/benchmarks.md — anchor fix + invocation pivot + beta framing dropped | §3.C | DONE | Tool table pivoted to canonical names + alias note; `2.0.0-beta.11` reference removed; `#the-bismark-rust-suite` |
| 19 | rust/choosing-an-alignment-mode.md — pivot | §3.C | DONE | `bismark prepare --combined_genome` |
| 20 | usage/alignment.md — 3 beta headings reframed to GA + `_rs` commands (C2) | §3.C / C2 | DONE | Combined-index / uBAM / BINSEQ headings de-beta'd; `bismark_rs`/`bismark_genome_preparation_rs` → canonical; renamed headings have no inbound anchors (V7 clean) |
| 21 | quick-reference.md — add subcommand↔classic mapping table | §3.D-1 | DONE | 12-row "Command reference" table + "recommended interface / classic names supported" note |
| 22 | quick-reference.md — pivot its own body (A6) | §3.D-2 | DONE | genome-prep/dedup/extract/report/summary code blocks + prose pivoted |
| 23 | README.md — version reconciliation (v2.0.0 GA literals → version-agnostic) | §3.E / I4 / OD-2 | DONE | "v2.0.0 is generally available" → "generally available"; "v2.0.0+" removed; docker `:2.0.0` → `:latest`/`:<version>`; `cargo install bismark` kept; L65 proper-noun light-touch → `bismark report`/`bismark summary` |
| 24 | README.md — iron-chancellor link → master | §3.E | DONE (n/a) | master's README had **no** iron-chancellor ref (the §3.E "L13" was a stale Rev-2 snapshot); branch README verified clean |
| 25 | CONTRIBUTING.md — beta→GA reframe + iron-chancellor → master | §3.E / I2 | DONE | L16 "currently in beta…" → "generally available, supported default; Perl now archived"; L33 branch → `master` |
| 26 | rust/README.md — current-state pivot (header, `_rs` install table, `_rs` usage examples, per-tool `_rs` column, `on iron-chancellor` refs) | §3.E / I1 | DONE | Binary-naming section rewritten; install table → single `bismark` crate; combined-index examples → `bismark …`; per-tool table binary column de-`_rs`'d; state key → "shipped on master"; status-line + "keeping current" note → master |
| 27 | rust/README.md — KEEP historical Milestones log as-written | §3.E / I1 | DONE | Reverse-chron log preserved verbatim (only in-line `_rs`/branch nouns within entries normalized) |
| 28 | rust/README.md — add the new journal entry noting the docs pivot + GA framing | §3.E / §4.9 | **PARTIAL** | Newest Milestones entry is still **2026-07-06**; **no new dated entry for the Phase-5 docs pivot**. Also the Milestones intro (L179) still reads "merges into `rust/iron-chancellor`" while the parallel "Keeping this journal current" note (L168) was updated to `master` — small internal inconsistency |
| 29 | CHANGELOG.md — 3.0.0 scaffold | §3.F | DONE | `## Bismark 3.0.0 (unreleased) — the Rust suite` + TODO(phase6) marker; bullets: one crate/binary, subcommand+aliases byte-identical, `cargo install bismark`, Perl→legacy; body deferred to Phase 6 |
| 30 | Frontmatter `description:` policy | §3.G / I5 | DONE | installation.md description changed (was factually wrong); coverage-report/processing-report/methylation-extraction descriptions neutralized to `bismark <sub>`; filtering/genome-prep/summary descriptions had no invocation phrasing → correctly left |
| 31 | docs/astro.config.mjs — editLink → master | §3.H / I3 | DONE | `…/edit/rust/iron-chancellor/docs/` → `…/edit/master/docs/` |
| 32 | A-BIOCONDA — confirm install-story W3 carries the bioconda deliverable | §4.11 | DONE | install-story `EPIC.md` W3 (lines 27–29) owns the bioconda `bismark`→Rust recipe; not silently dropped |
| 33 | PROGRESS.md Phase-5 row + epic sub-plan table row 5 | §4.14 | DONE | PROGRESS row 5 → "Implemented → PR #1060 (`43a3e93`)"; epic table row 5 → `phase5-docs-deprecation/PLAN.md` |

## Decisions (verified)

| Decision | Requirement | Status |
|---|---|---|
| OD-1 (docs-only; no runtime notice) | No production `src/` change | DONE — commit touches **0** `.rs` files / **0** `rust/*/src/` files; step-8 deprecation-notice correctly dropped |
| OD-2 (version-agnostic + Phase-6 markers) | No hardcoded GA version except behind a marker | DONE — V6 clean: only literal is `3.0.0` in CHANGELOG on its `TODO(phase6)` line; container tags version-agnostic |
| OD-3 (Phase-5 vs Phase-6 split) | `.github/workflows/docs.yml` NOT touched | DONE — `docs.yml` absent from the diff (correctly deferred to Phase 6) |

## Validation (§7)

| # | Check | Result |
|---|---|---|
| V2 | No classic/`_rs` name in a runnable code fence | PASS — every code-fence command uses canonical `bismark <sub>`; remaining classic-name text is table/alias/proper-noun prose only |
| V3 | No beta/`_rs`/iron-chancellor residue outside the historical Milestones log | PASS (with note) — residue confined to (a) the historical Milestones log [allowed], (b) old 2011-era CHANGELOG entries [Bowtie2 beta7 / Bismark v0.6.beta2 — out of scope], and (c) 3 deliberate GA-transition explanations in rust/README.md current-state sections (L6 branch-retirement note, L17 "during the beta the suffix…", L80–81 "crates from the beta remain frozen"). (c) are intentional + factually correct, not stale residue |
| V4 | One-crate install block | PASS — `cargo install bismark`; no 12-crate list; `--branch master` |
| V5 | Doc map matches cli.rs (both copies) | PASS — quick-reference table is an exact 12-row match to both `run_subcommand()` and `print_top_level_help()` (incl. `cov2cyt` + the `align` synonym) |
| V6 | Version-agnostic | PASS — see OD-2 |
| V7 | Anchor integrity | PASS — full internal-anchor scan resolves; the renamed installation heading + the Legacy self-anchor + both inbound links all resolve. (One scan flag — `#correctness--concordance` in choosing-an-alignment-mode.md — is a slugger false positive: github-slugger doubles the hyphen where `/` was removed; that link was not changed this phase) |
| V1 / V8 / V9 | Docs build / `bismark <sub> --help` / dev `cargo install` | NOT re-run in this audit (runtime gates; PROGRESS records V2/V3/V5/V6/V7 PASS locally). Their code-level premise holds: V5 confirms every documented subcommand token is a real `cli.rs` route, so the rewritten examples reference valid subcommands |

## Gaps (detail)

### Item 28: rust/README.md — new Milestones journal entry (PARTIAL)

**Expected (§3.E, §4 step 9):** "The journal-status/Milestones line gets the usual new entry noting the docs pivot + GA framing (per the journal convention)."
**Found:** The current-state sections, the per-tool table binary column, the state key, the status-line, and the "Keeping this journal current" note were all pivoted to `master`/GA. The historical Milestones log was correctly preserved. But **no new dated Milestones entry** was added for the Phase-5 docs pivot — the newest entry remains `2026-07-06` (BINSEQ CBQ). Additionally the Milestones intro line (`rust/README.md:179`) still reads "merges into `rust/iron-chancellor`" while the parallel note at L168 was updated to `master`.
**Gap:** Add a `2026-07-07` (or PR-#1060 merge-date) Milestones entry noting the docs pivot to `bismark <subcommand>` + the beta→GA reframe, and normalize the L179 intro "merges into `rust/iron-chancellor`" → `master` for consistency with L168.
**Weight:** Minor / non-blocking. Journaling only; no user-facing docs impact. Conventionally this line is added at merge time (PR #1060 is not yet merged), so it may close naturally at Phase 6 / merge.

## Verdict

**INCOMPLETE — 1 minor item.**

Every user-facing and substantive deliverable in the plan is fully implemented and verified: the mechanical invocation pivot (rules 1–7), all 7 installation.md reframe items, the Rust-feature pages + the 3 `usage/alignment.md` beta headings (C2), the quick-reference table + body pivot, the README version reconciliation, CONTRIBUTING, the rust/README current-state pivot with the historical log preserved, the CHANGELOG scaffold, the frontmatter policy, and the astro.config editLink. OD-1 (docs-only, zero `src/` change), OD-2 (version-agnostic + markers), and OD-3 (`docs.yml` untouched) are all satisfied. Both Criticals (C1 anchor-lockstep, C2 alignment beta headings) and every Important are closed. Validations V2–V7 pass; A-BIOCONDA is confirmed tracked in install-story W3; PROGRESS + the epic table are updated.

**To reach COMPLETE**, close the single open item:
1. Add the new dated Milestones journal entry to `rust/README.md` for the Phase-5 docs pivot, and normalize the `rust/README.md:179` intro line ("merges into `rust/iron-chancellor`" → `master`) to match the already-updated L168 note.

This is a documentation-journal nicety, not a functional or user-facing gap; the team may reasonably close it now or waive it to the PR-#1060 merge / Phase 6.

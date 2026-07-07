# Code Review A — Phase 5 (Docs & deprecation)

**Reviewer:** A (code-reviewer skill, fresh context)
**Target:** commit `43a3e93` "docs(suite): pivot to `bismark <subcommand>` + beta→GA reframe (Phase 5)" on `rust/phase5-docs`
**Base:** `master` (Phase 5 diff = 26 files; the `master...` 3-dot diff also contains prior phases 3/4 — those are NOT reviewed here)
**Spec:** `plans/07062026_single-binary-suite/phase5-docs-deprecation/PLAN.md` (Rev 2)
**Source of truth for the map:** `rust/bismark/src/cli.rs`

---

## Summary / Verdict

**APPROVE WITH CHANGES.** The core mechanical pivot is correct and thorough. Every runnable command in a code fence uses `bismark <subcommand>` (or bare `bismark` for the aligner); the quick-reference mapping table matches `cli.rs` exactly (all 12 rows, incl. `cov2cyt`); every subcommand token in the docs is a real subcommand (zero mis-mappings — no `coverage`, no `bedGraph`, `nome`/`cov2cyt`/`bedgraph` all correct); anchors were renamed in lockstep and all inbound links resolve; version literals are agnostic behind Phase-6 markers; the installation.md `description:` no longer says "written in Perl"; and the three `usage/alignment.md` beta headings (C2) are reframed to GA.

**No Critical or High findings for the user-facing docs site.** All defects are Medium/Low and concentrated in (a) the contributor-facing `rust/README.md` status journal, which was NOT fully reconciled with the Phase-2 crate fold, and (b) three frontmatter `description:` fields that were left with stale/mangled classic invocations while their page bodies were pivoted.

---

## Validation matrix (from PLAN §7)

| Check | Result | Notes |
|---|---|---|
| V2 — no classic name in a runnable code fence | ✅ PASS | Remaining classic names are all prose "(classic alias: …)", the mapping table, or proper-noun prose |
| V3 — no beta/`_rs`/iron-chancellor residue | ⚠️ MOSTLY | User-facing docs clean. Hits confined to the deliberately-retained Milestones log + historical CHANGELOG (2011-era) entries. See M1/M2/L1 for current-state residue in rust/README.md |
| V5 — map matches `cli.rs` (both copies) | ✅ PASS | quick-reference.md table == `run_subcommand()` == `print_top_level_help()` (12 rows) |
| V6 — version-agnostic | ✅ PASS | No hardcoded `2.0.0`/`3.0.0`/`beta.N` outside `<!-- TODO(phase6) -->` markers or historical logs |
| V7 — anchor integrity | ✅ PASS | `#the-bismark-rust-suite` (renamed) + inbound overview.md:6 / benchmarks.md:6 in lockstep; `#legacy-the-perl-bismark` self-anchor resolves; all other internal `#slug` links resolve |
| Frontmatter (reviewer focus 7) | ✅ PASS (installation) / ⚠️ (M3) | installation.md description fixed; 3 others missed |
| C2 — usage/alignment.md beta headings | ✅ PASS | 3 headings reframed to GA; `_rs` commands → bare `bismark` |
| Subcommand correctness (reviewer focus 1) | ✅ PASS | zero mis-mappings |

---

## Issues by area

### Medium

**M1 — `rust/README.md:8–11` "## Layout" section is stale (post-fold).**
The section still describes the pre-fold multi-crate layout:
> - `bismark-io/` — shared library: BAM/SAM/CRAM I/O via noodles. See `bismark-io/DESIGN.md`…
> - Per-binary crates are added incrementally (`bismark-dedup/`, `bismark-bedgraph/`, `bismark-extractor/`, …). Phase 1 priorities are tracked on the project board.

Verified against the branch: `rust/` now contains **only** `rust/bismark/` — the individual crate directories no longer exist (folded in Phase 2). This directly contradicts the GA single-crate reality the rest of the doc now asserts (and the beta→GA reframe elsewhere in the same file). "Phase 1 priorities are tracked on the project board" is also beta-era framing. PLAN §3.E pivots the current-state sections but did not enumerate this one (plan under-scope + implementation miss). **Recommend:** rewrite to describe the single `bismark` crate with modules (`bismark::io`, `bismark::aligner`, …), consistent with the overview.md wording that was updated.

**M2 — `rust/README.md:173` "Versions are the crate manifests on `master`" is now inaccurate.**
The per-tool Status table (L157–171) lists 12 separate `bismark-<tool>` crates each with its own `…beta.N`/`alpha.1` version. Post-fold, those manifests do not exist on `master` (only `rust/bismark/Cargo.toml` does). The versions shown are in fact the **frozen crates.io** versions, which the same line's second sentence and the install section (L80–81) correctly describe. The phrase "the crate manifests on `master`" contradicts the fold. **Recommend:** reword to "the last published crates.io versions of the now-frozen per-tool crates" (and consider labelling the table as the historical/frozen per-tool record, since it is no longer the current build layout).

**M3 — Three frontmatter `description:` fields left with stale/mangled classic invocations (PLAN §3.G).**
The page *bodies* were pivoted, but these SEO/search-preview descriptions were not, leaving them inconsistent (and showing broken commands):
- `docs/src/content/docs/usage/deduplication.md:3` → `description: "./deduplicatebismark [options] filename(s)"` (mangled; body pivoted to `bismark dedup`)
- `docs/src/content/docs/options/genome-preparation.md:3` → `"…by typing: bismarkgenomepreparation --help"` (mangled; body pivoted to `bismark prepare --help`)
- `docs/src/content/docs/options/methylation-extraction.md:3` → `"…by typing bismarkmethylationextractor --help"` (mangled; body pivoted to `bismark extract --help`)

§3.G puts descriptions explicitly in scope and asks to neutralize invocation-phrasing descriptions; Q8 asks the reviewer to flag descriptions that read wrong. These do. **Recommend:** neutralize to `bismark <sub> …` (e.g. `bismark dedup [options] filename(s)`, `bismark prepare --help`, `bismark extract --help`) to match the enumerated usage/*.md descriptions that WERE fixed.

### Low

**L1 — `rust/README.md:17` self-contradicting example.**
> During the beta the host binaries carried an `_rs` suffix (`deduplicate_bismark`, …)…

The parenthetical example omits the very `_rs` suffix the sentence is about. Should read `(`deduplicate_bismark_rs`, …)`.

**L2 — `rust/overview.md:78/85/87` bare "coverage2cytosine" in prose.**
These are proper-noun prose (numbered feature list), so acceptable under §3.A rule 3, but slightly inconsistent with the pivot elsewhere. Optional: first use → "`bismark cov2cyt` (coverage2cytosine)". Not blocking.

**L3 — `rust/README.md:1–3` "Active rewrite of Bismark from Perl to Rust" header.**
Borderline stale now that the suite is GA/complete; defensible as a living status journal (ongoing v1.x work). Optional tidy.

---

## What is correct (verified, not just assumed)

- **Subcommand map (V5):** quick-reference.md table byte-matches `cli.rs` — `dedup`/`extract`/`bedgraph`/`cov2cyt`/`prepare`/`bam2nuc`/`nome`/`filter`/`consistency`/`report`/`summary`, plus bare `bismark` / `bismark align` for the aligner.
- **No mis-mappings:** grep for `bismark {coverage|cov2cytosine|bedGraph|deduplicate|methylation|nome_filtering|…}` → empty. Every `bismark <word>` token in the docs is one of the 12 valid subcommands.
- **Anchors (V7):** installation.md `## The Bismark Rust suite` → `#the-bismark-rust-suite`; both inbound links (rust/overview.md:6, rust/benchmarks.md:6) updated in the same commit. `## Legacy: the Perl Bismark` → `#legacy-the-perl-bismark` self-link resolves. benchmarks.md targets (`#combined-index-modes`, `#rammap-experimental`, `#methylation-extractor`, `#profiling-the-perl-pipeline`) and choosing-an-alignment-mode.md `#correctness--concordance` all resolve; no headings in those files were renamed.
- **usage/alignment.md (C2):** `## Combined-index alignment (opt-in)`, `## Unaligned BAM (uBAM) input`, `## BINSEQ (.vbq + .cbq) input` — all `(Rust suite beta)` labels dropped; the BINSEQ heading correctly now advertises `.cbq` too (matches merged CBQ support, commit `085d690`); code fences use bare `bismark`. No inbound anchors pointed at the old BINSEQ heading, so the rename is safe.
- **Version-agnostic (V6/OD-2):** README.md `:2.0.0` → `:latest`/`:<version>`; "v2.0.0 GA" claims neutralized; rust/README.md `@2.0.0`→`@<version>` + marker; overview.md dropped "available from 2.0.0-beta.11" (×2); CHANGELOG 3.0.0 scaffold carries the `<!-- TODO(phase6) -->` marker.
- **beta→GA reframe:** CONTRIBUTING.md ("currently in beta" → GA present tense; iron-chancellor → master), overview.md/benchmarks.md ("public beta"/"at the Rust general release" → present-tense GA; `bismark-io` library → `bismark::io` module), astro.config.mjs editLink → master, installation.md Legacy section added and reads well.
- **Unchanged pages verified clean:** index.mdx, faq/index.md, faq/low-mapping.md, usage/library-types.md have no tool refs / residue — correctly untouched.
- **Body prose pivots:** faq/* , options/*, usage/methylation-extraction.md all pivoted consistently; the obsolete "this script needs to reside in the same folder as the bismark_methylation_extractor" notes were correctly dropped (accurate now that it's one binary); faq/conversion-efficiency.md fixed a stale external Docs link to the internal `/Bismark/usage/concordance/`.

---

## Notes on scope

- V1 (astro build), V8 (`bismark <sub> --help` exit 0), V9 (dev cargo install) are runtime/build gates the implementer runs; not re-executed in this docs review. Example flags were inspected statically and route correctly (the dispatcher strips the token and forwards the rest to each tool's `run_from_args`), so no example uses a flag foreign to its subcommand.
- The per-tool Status table version oddities (e.g. `bismark-aligner … 1.0.0-alpha.1`) are pre-existing journal content; M2 covers the reconciliation the fold now requires.

# Code Review B — Phase 5: Docs & deprecation

**Reviewer:** B (independent, fresh context)
**Target:** commit `43a3e93` on `rust/phase5-docs` (26 files) — the docs pivot to `bismark <subcommand>` + beta→GA reframe.
**Spec:** `plans/07062026_single-binary-suite/phase5-docs-deprecation/PLAN.md` (Rev 2).
**Source of truth (subcommand map):** `rust/bismark/src/cli.rs` on this branch.
**Verdict:** ✅ **APPROVE WITH CHANGES** — no Critical/High. All high-risk dimensions (subcommand mapping, code-fence purity, anchor integrity) are correct. Findings are Medium/Low consistency + a §3.G frontmatter gap.

> Note on the diff: `git diff master...` is inflated by a stale local `master` (it pulls in the whole GA takeover). The actual Phase-5 change is the single commit `43a3e93` (26 files), which is what this review covers.

---

## What passed (verified, not assumed)

**1. Subcommand mapping — CORRECT (highest-risk dimension).**
The `quick-reference.md` "Command reference" table (12 rows) matches `cli.rs` `run_subcommand()` **and** `print_top_level_help()` exactly:
`dedup`→deduplicate_bismark · `extract`→bismark_methylation_extractor · `bedgraph`→bismark2bedGraph · `cov2cyt`→coverage2cytosine · `prepare`→bismark_genome_preparation · `bam2nuc`→bam2nuc · `nome`→NOMe_filtering · `filter`→filter_non_conversion · `consistency`→methylation_consistency · `report`→bismark2report · `summary`→bismark2summary · bare/`align`→aligner. **No mismatches** (checked specifically for the easy-to-swap ones: `coverage2cytosine`→`cov2cyt`, `bismark2bedGraph`→`bedgraph`, `bismark2report`→`report`, `bismark2summary`→`summary`, `filter_non_conversion`→`filter`, `methylation_consistency`→`consistency`, `NOMe_filtering`→`nome`). Every pivoted body command uses the right subcommand.

**2. V2 — no classic name in a runnable code fence.** All fenced commands are pivoted (`bismark prepare`, `bismark extract`, `bismark dedup`, `bismark bedgraph`, `bismark report`, `bismark summary`, `cargo install bismark`, etc.). The only remaining classic-name occurrences in `docs/src/content` are legitimate: the quick-reference *alias column*, and proper-noun prose ("coverage2cytosine" in `rust/overview.md:78/85/87`, rule 3). None inside code fences.

**3. V7 — anchor integrity holds.** `installation.md` renamed `## Bismark Rust suite (beta)` → `## The Bismark Rust suite` (`#the-bismark-rust-suite`) and both inbound links (`rust/overview.md:6`, `rust/benchmarks.md:6`) were updated in lockstep. The new `## Legacy: the Perl Bismark` (`#legacy-the-perl-bismark`) matches its inbound intro link. The 3 reframed `usage/alignment.md` headings ("Combined-index…", "uBAM…", "BINSEQ…") orphaned **no** inbound anchors (grep for the old beta slugs returns zero). All 8 cross-page + 3 same-page `#`-anchors resolve to real headings. (The one apparent mismatch, `choosing-an-alignment-mode.md:30` → `#correctness--concordance`, is a *pre-existing, untouched* link that is correct under github-slugger's double-hyphen-for-"& " rule — not a Phase-5 regression.)

**4. V3 residue — clean where it matters.** `iron-chancellor`: only the allowed retirement note (`rust/README.md:6`) + the Milestones preamble (`:179`). `_rs`: only the intentional "during the beta" note (`rust/README.md:17`) + historical Milestones. `beta`: only historical CHANGELOG entries (Bowtie2 beta7, Bismark v0.6.beta2). No residue in the primary user surfaces (`docs/**`, `README.md`, `CONTRIBUTING.md`, `astro.config.mjs`).

**5. V4 install block, OD-2 version-agnostic, §3.H editLink, factual de-Perl-ing** — all correct:
- `installation.md`: one-crate `cargo install bismark` + dev `--branch master`; dropped the 12-crate block and the `--branch rust/iron-chancellor`; "No `samtools` is required (BAM/SAM I/O is pure-Rust)"; removed the false "requires a working of Perl" from Dependencies; added the Legacy section.
- `README.md`: dropped the wrong `v2.0.0` GA literals (now "generally available" with no version); container `:2.0.0` → `:latest`.
- `astro.config.mjs`: editLink → `edit/master/docs/`.
- `CHANGELOG.md`: `## Bismark 3.0.0 (unreleased)` scaffold behind a `<!-- TODO(phase6) -->` marker.
- No "stand-alone script / external module / must reside in the same folder" phrasing survives to contradict the single binary (the two grep hits — `alignment.md:88` "post-processing scripts are not part of the Bismark package" and `methylation-extraction.md:40` "comes with a few options" — are benign and correct).

---

## Issues by area

### Medium

**M1 — `rust/README.md:17`: self-contradicting `_rs` example.**
> "During the beta the host binaries carried an `_rs` suffix (`deduplicate_bismark`, …) so they could sit alongside the Perl Bismark scripts…"

The parenthetical illustrates "carried an `_rs` suffix" with the **unsuffixed** name. It should read `` (`deduplicate_bismark_rs`, …) `` — otherwise the example directly contradicts the sentence's own claim. (Keeping the `_rs` mention here is fine and useful for returning beta users; it's just the wrong token.)
**Fix:** `rust/README.md:17` → `carried an \`_rs\` suffix (\`deduplicate_bismark_rs\`, …)`.

**M2 — §3.G frontmatter miss: three `description:` fields still phrase a stale classic-name invocation.**
The plan makes frontmatter `description:` a first-class surface ("a decision, not an omission") and asks that descriptions phrasing an *invocation* be pivoted/neutralized. Three bodies were pivoted but their descriptions were not, so the SEO/preview text now contradicts the page:
- `docs/src/content/docs/usage/deduplication.md:2` — `description: "./deduplicatebismark [options] filename(s)"` (a literal, now-wrong invocation; body is `bismark dedup [options] filename(s)`).
- `docs/src/content/docs/options/genome-preparation.md:2` — `description: "…by typing: bismarkgenomepreparation --help"` (body is `bismark prepare --help`).
- `docs/src/content/docs/options/methylation-extraction.md:2` — `description: "…by typing bismarkmethylationextractor --help"` (body is `bismark extract --help`).
(These are auto-generated-from-first-line artifacts, so the classic names appear mangled — underscores stripped — which is why V3's `_rs`/classic-name greps don't catch them. This is exactly the "flag at code review" case the plan's §8 anticipated.)
**Fix:** update each to the canonical form, e.g. `bismark dedup [options] <filename(s)>` / `…by typing: bismark prepare --help` / `…by typing bismark extract --help` (or neutral prose). Contrast with the descriptions that *were* correctly updated: `coverage-report.md`, `methylation-extraction.md` (usage), `processing-report.md`, and `installation.md` — so the intent was clearly to do this; three were missed.

### Low

**L1 — Milestones log partially rewritten (deviation from plan's "keep as-written").**
§3.E says the reverse-chron Milestones log is "legitimately historical — keep as-written." But the commit rewrote `_rs`/`bismark_rs` → canonical in a handful of historical entries (2026-06-27 beta.13, 2026-06-26 non-dir PE, 2026-06-19 rammap-exposed, 2026-06-11 multi-input extractor) while leaving others untouched (e.g. the 2026-06-26 BINSEQ entry `rust/README.md:185` still reads `bismark_rs <.vbq>`). Net effect: the log is now internally inconsistent and a touch revisionist (a "beta.13" milestone that historically shipped `deduplicate_bismark_rs` now reads `deduplicate_bismark`). Direction is defensible, but pick one: either revert those edits (true to plan) or finish canonicalizing the whole log. Non-blocking.

**L2 — `rust/README.md:179` Milestones preamble still says "merges into `rust/iron-chancellor`".** Historically accurate (that's where they merged), so arguably intended-historical; but it's a lingering dead-branch ref in a non-Milestones-body line. If aiming for zero live iron-chancellor refs, reword to "…(the retired `rust/iron-chancellor` integration branch, now `master`)". Non-blocking.

**L3 — `README.md` container-tag placeholder lacks the Phase-6 marker.** `installation.md` tags its pinned example `` :<version>     # pinned  <!-- TODO(phase6): version --> ``, but `README.md`'s `# or pin a specific release, e.g. :<version>` has no marker. Add the same marker for a clean Phase-6 sweep. Cosmetic.

**L4 — `rust/README.md` Status table still framed per-crate with per-crate beta versions.** The table lists `bismark-io 1.0.0-beta.9`, `bismark-dedup 1.2.1-beta.2`, etc., which sits in mild tension with the "single `bismark` crate / one binary" narrative asserted two sections above (and in `overview.md`). The header note ("the per-tool `bismark-<tool>` crates from the beta remain on crates.io frozen") covers it, and §3.E only asked to drop `_rs` from the binary column (done), so this is within scope-as-executed — flagging only as a readability tension a future pass may want to reconcile. Non-blocking.

**L5 — `README.md` bioconda badge still points at the Perl v0.25.1 recipe.** The top-of-file `install with bioconda` badge links to the Perl bismark recipe, which would install v0.25.1, not the Rust suite. Pre-existing and explicitly descoped (bioconda deferred to install-story W3), so **out of Phase-5 scope** — noted only so it isn't silently forgotten at GA.

---

## Recommendations (priority order)
1. **M1** — fix the `_rs` example token in `rust/README.md:17` (one word).
2. **M2** — pivot the three stale frontmatter descriptions (`usage/deduplication.md`, `options/genome-preparation.md`, `options/methylation-extraction.md`).
3. **L1** — decide the Milestones-log policy (revert the partial edits, or finish the canonicalization) so the historical log is internally consistent.
4. **L2/L3** — optional tidy of the `:179` branch ref and the README container-tag marker for a clean Phase-6 residue sweep.

None of these block the pivot: the subcommand map, code-fence purity, and anchor integrity — the three ways this change could have shipped something *broken* — are all correct.

# Plan Coverage Report

**Mode:** B — code vs. plan (post-implementation)
**Plan(s):** `plans/08102026_1099-five-base-genome-discovery/PLAN.md` (rev 1)
**Date:** 2026-08-10
**Audited:** uncommitted working tree on branch `1099-five-base-genome-discovery` (`git diff`, not `HEAD`). `SESSION_HANDOFF.md` excluded as an unrelated edit.
**Verdict:** COMPLETE — 0 items unresolved (1 DEVIATED, documented in §10, coverage intact)

## Summary

- Total items: 33
- DONE: 32
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 1 (§5d.9 — documented in §10; the coverage it intended exists)

Files changed (source/test/docs, excluding `SESSION_HANDOFF.md`): `rust/bismark/src/aligner/discovery.rs`,
`rust/bismark/src/aligner/config.rs`, `rust/bismark/src/aligner/mod.rs`,
`rust/bismark/tests/aligner_cli.rs`, `rust/bismark/tests/aligner_five_base_groundtruth.rs`,
`CHANGELOG.md`, `docs/src/content/docs/rust/illumina-5-base.md`.

## Coverage ledger

### §0 — Open decisions

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | D1 — Task 4 (fail early on missing `--five_base_index`): cut, filed separately | §0 D1 / §10 | DONE | Cut per the recorded lean. Verified via `gh issue view 1100`: **#1100 exists, OPEN**, titled "5-Base: validate --five_base_index at resolve time instead of failing late as a desync", and carries all three reviewer hazards (private `first_missing`/`index_suffixes`, the missing small→large two-arm probe, `BOWTIE2_INDEXES`/`HISAT2_INDEXES` resolution) **plus** the `-o`-not-created defect. Not silently dropped. |
| 2 | D2 — branch `summary()` so a 5-Base run stops printing non-existent `CT index:`/`GA index:` | §0 D2 / Task 3 | DONE | `config.rs` `RunConfig::summary()` builds `index_lines` from `self.five_base`. Confirmed live (§6 run): stderr prints `reference:      …/g/pUC19.fa` and `5-Base index:   (none: minimap2 reads the FASTA)`; no `CT index:`/`GA index:`/`large index:` line. Non-5-Base branch reproduces the three original lines with the same field widths. |
| 3 | D3 — repoint the existing 5-Base test-helper call sites at a FASTA-only genome | §0 D3 / §5d.10 | DONE | See item 22. |
| 4 | D4 — fold `run_five_base_consensus_standalone` in as the sibling's second caller | §0 D4 / §10 | DONE | `mod.rs` (was `:543-546`): the hand-rolled `canonicalize` + `discover_fastas` pair is replaced by `discover_genome_fasta_only(genome_arg)?.fastas`. Gains the `is_dir` check; loses the bespoke `consensus: --genome …` stderr text, as §10 records. |

### §4 — Tasks

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 5 | Extract `discovery.rs:132-138` into `fn absolute_genome_dir`, `GenomeFolder` preserved on **both** the canonicalize failure and `!is_dir`; called from `discover_genome` | Task 1.1 | DONE | `discovery.rs:140-147`; both arms return `AlignerError::GenomeFolder(genome_arg.to_path_buf())`, `map_err` not `?`. `discover_genome` (`:149`) calls it. See observation O1 on the doc comment. |
| 6 | Add `pub(crate) fn discover_genome_fasta_only`: `absolute_genome_dir` → `discover_fastas` → `GenomeIndexes` with conventional CT/GA basenames, `large_index: false`, `combined_index_basename: None` | Task 1.2 | DONE | `discovery.rs:209-235`. `pub(crate)` as specified; reuses `discover_fastas` as-is (no duplicated sort order). |
| 7 | Doc the two basename fields with **why** they are safe — name `mod.rs:943` as guarantor, `parallel.rs:597` as nearest reader; no `clippy::doc_lazy_continuation` breakage | Task 1.3 | DONE | `discovery.rs:76-86`: `ct_index_basename` doc names both; `ga_index_basename` and `large_index` docs reference it. `clippy --all-targets -D warnings` clean (both feature sets). See observation O2 on line drift. |
| 8 | `config.rs`: add `discover_genome_for_run` and call it at the discovery site | Task 2 | DONE | `config.rs:624-636` (the named function, so the branch is unit-testable without an aligner binary); call site now `discover_genome_for_run(cli.illumina_5base, aligner, &genome_arg)?`. |
| 9 | Comment the `820-830`-before-`850` ordering (why the unprobed `None` is safe) | Task 2 / §3 O1 | DONE | Comment at the `--combined_index` presence guard: "5-Base cannot reach here with --combined_index (rejected above), so its unprobed None is safe." States the ordering dependency without hardcoding line numbers. |
| 10 | Task 3 — `summary()` prints the reference FASTA + `--five_base_index` in place of `CT index:`/`GA index:`/`large index:`; no new field | Task 3 | DONE | Same as item 2. No `RunConfig` field added; `run_config_stub` untouched. |
| 11 | Task 4 — cut unless Felix says keep | Task 4 | DONE | Cut; see item 1. No `pub(crate)` was applied to `first_missing`/`index_suffixes` (correct for a cut). |
| 12 | Task 5 — the desync error must name the actual engine, not hardcoded `"minimap2"` | Task 5 | DONE | `mod.rs` (shared 5-Base PE path): `format!("{} produced fewer PE records than read pairs (desync)", config.aligner.name())`. `RunConfig.aligner: Aligner` (`config.rs:404`) and `Aligner::name` (`config.rs:57`) both confirmed. |
| 13 | Task 6 — `CHANGELOG.md` at repo root, `## Unreleased` → `### bismark (aligner)`; covers both routes, `--genome` still required, non-5-Base unchanged, credits @Danielsm8 | Task 6 | DONE | One bullet at the top of `### bismark (aligner)` (line 14) under `## Unreleased` (line 4). Covers the `--five_base_index` route and the `BS_CT.mmi` default route explicitly, states `--genome` is still required and must contain the FASTA, states the D2 summary change, states "Nothing changes for any bisulfite run" with the sabotage-gated test, mentions Task 5, credits @Danielsm8. |
| 14 | Task 6 — one sentence in the 5-Base docs page ("Running it") saying the genome folder needs only the FASTA | Task 6 | DONE | `docs/src/content/docs/rust/illumina-5-base.md:49-51`, immediately after the engine paragraph in "Running it": `--genome` needs only the FASTA, no `bismark_genome_preparation` step. (Repo path is lowercase `docs/`; the plan wrote `Docs/`.) See observation O3 on `rust/README.md`. |

### §5 — Tests

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 15 | `fasta_only_needs_no_bisulfite_index` — FASTA-only dir → `Ok`, with `fastas.len()==1`, `fasta_kind==Fa`, `combined_index_basename.is_none()`, `!large_index` | §5a.1 | DONE | `discovery.rs` tests. All four assertions present, plus a paired `discover_genome(Bowtie2, …) → FaultyIndex` assertion in the same test — the two halves of the fix side by side. |
| 16 | `fasta_only_still_requires_a_fasta` — empty dir → `NoFasta` | §5a.2 | DONE | Typed variant asserted (`matches!(…, Err(AlignerError::NoFasta(_)))`), not `is_err()`. |
| 17 | `fasta_only_rejects_a_non_directory` / `_nonexistent_path` → `GenomeFolder` | §5a.3 | DONE | Implemented under one name, `genome_folder_errors_survive_the_shared_prologue`, which loops over both probes (missing path, path-to-a-file). Behaviour fully covered; test names differ from the plan's. |
| 18 | `discover_genome` mirrors of §5a.3 — nonexistent path *and* path-to-a-file → `GenomeFolder` (closes the zero-coverage `GenomeFolder` hole / the §7 `?`-downgrade risk) | §5a.4 | DONE | Same test: the loop asserts both entry points for both probes, i.e. all four combinations. Mechanism verified independently — `AlignerError::Io(#[from] std::io::Error)` exists at `error.rs:16`, so a bare `?` on `canonicalize` would produce `Io` and the missing-path probe would go red. |
| 19 | `.fa.gz`-only genome → `fasta_kind == FaGz` | §5a.5 | DONE | `fasta_only_accepts_a_gzipped_fasta`. |
| 20 | `discover_genome_for_run(five_base=true)` over a FASTA-only dir → `Ok`; `(false)` → `FaultyIndex` | §5b.6 | DONE | `discover_genome_for_run_skips_the_index_check_for_five_base` (in `config.rs` tests). Adds a `Minimap2` arm beyond the plan's two. No aligner binary involved. |
| 21 | Resolve-level 5-Base test in the `five_base_duplex_guards` shape: assert the error **cannot** be `FaultyIndex` (install-agnostic), with real read files | §5c.7 | DONE | `five_base_resolve_does_not_require_the_converted_index`; `if let Err(e) = resolve(…) { assert!(!matches!(e, FaultyIndex{..}), …) }`, looped over both shapes (default minimap2, and `--bowtie2 --five_base_index /nonexistent/idx`). Fixture helper `fasta_only_genome_and_reads` writes real (empty) `r1`/`r2` so `check_exists` passes, and always supplies `-1`/`-2`. All three §5 traps handled. |
| 22 | `faithful_run_still_rejects_unprepared_genome` — no `--illumina_5base` → `matches!(…, FaultyIndex)` | §5c.8 | DONE | `faithful_resolve_still_requires_the_converted_index` (name differs). Uses `.unwrap_err()` + typed `matches!`; SE positional read, so it never trips the `--illumina_5base` needs `-1` guard. |
| 23 | Two hermetic integration tests in `tests/aligner_cli.rs` — one per engine route, incl. the minimap2 `BS_CT.mmi` case — asserting the `XM` tag, not just exit status; plus `make_genome_fasta_only` | §5d.9 | **DEVIATED** | Documented in §10. Implemented by **repointing** the two existing route tests instead of adding near-clones: `five_base_pe_end_to_end_inverts_polarity` (minimap2 — the `BS_CT.mmi` case, §2 row 2) and `five_base_bowtie2_unconverted_index_end_to_end` (bowtie2 + `--five_base_index`) now call `make_genome_fasta_only`. Both are hermetic (fake `minimap2`/`bowtie2` via `--path_to_*`) and both assert `xm == b".Z...z"`. `make_genome_fasta_only` added at `aligner_cli.rs:43`. **The coverage §5d.9 intended exists in full** — see Gaps detail. |
| 24 | D3 — drop the dummy CT/GA index writes from the existing 5-Base helper call sites; keep `make_genome*` where a prepared genome is the point | §5d.10 | DONE | All four locations the plan named were changed: `aligner_cli.rs` (2 direct sites) and `aligner_five_base_groundtruth.rs` `write_genome` / `write_genome_multi`, whose 7 callers (`:159, :281, :439, :580` and `:810, :923, :987`) are repointed with them — 9 test sites, all scaffolding **deleted** rather than added. 5-Base-on-a-prepared-genome stays covered at the four remaining 5-Base sites (`make_genome_mmi` at `:6234`, `:6353`; `make_genome` at `:6327`, `:6407`). |
| 25 | §5e — over-reach double-covered: the new guard **and** the existing `missing_index_errors`, both required red under sabotage | §5e | DONE | `missing_index_errors` (`aligner_cli.rs:431-452`) is **unmodified** by this diff and still asserts exit 1 + `"faulty or non-existant"` against a genome with `BS_CT.3.bt2` removed, with no fake bowtie2. Sabotage not re-run (would require editing tracked files); the §10 claim is specific and mechanically credible — assessed in Gaps detail. Independent live confirmation: the §6 negative control reproduced exit 1 + that exact message from the real binary. |

### §6 — Verification

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 26 | `cargo test -p bismark --lib aligner::discovery` | §6 | DONE | **25 passed**, 0 failed (baseline 21 + the 4 new = §10's claimed 21→25). |
| 27 | `cargo test -p bismark --lib aligner::config` | §6 | DONE | **45 passed**, 0 failed (42 + 3 new = §10's claimed 42→45). |
| 28 | `cargo test -p bismark --test aligner_cli five_base` | §6 | DONE | **6 passed**, 0 failed — includes both repointed route tests. |
| 29 | `cargo test -p bismark --test aligner_five_base_groundtruth` | §6 | DONE | **7 passed**, 0 failed. All 7 ran for real, not skipped: `minimap2` is on PATH (`/opt/homebrew/bin/minimap2`) and both control fixtures exist (`test_files/lambda_NC_001416.fa.gz`, `test_files/pUC19.fa.gz`), so `have_minimap2()`/`load_controls_or_skip()` did not early-return. |
| 30 | `cargo test -p bismark` (full crate) | §6 | DONE | Green. The run completed and reached the final `Doc-tests bismark` stage (8 passed) with every stage `ok` and no `failures:`/`FAILED`/`error` line — and `cargo test` is fail-fast by default, so reaching Doc-tests means every preceding test binary passed. A confirmatory re-run capturing per-binary totals was killed at the 10-minute mark by `rust/target` lock contention with the two concurrent code reviewers, after **2043 passed / 0 failed across 59 test binaries**. |
| 31 | `cargo fmt -p bismark -- --check` | §6 | DONE | Clean. |
| 32 | `cargo clippy -p bismark --all-targets -- -D warnings` and the same with `--features rammap-inprocess` | §6 | DONE | Both clean, no warnings (so Task 1.3's `doc_lazy_continuation` hazard is closed). |
| 33 | End-to-end recipe + the "sharpest manual check" (placeholders deleted must not change output) | §6 | DONE | Run in a fresh, unique scratch dir (`$TMPDIR/pm1099_audit/e2e_$$`) — **not** `$TMPDIR/g1099`, per §10's iteration-log warning. Results below. |

**Item 33 detail.** FASTA-only genome (`pUC19.fa`, no `Bisulfite_Genome/`), `test_files/test_R{1,2}.fastq.gz`:

- **Positive (default minimap2):** exit **0**; `test_R1_bismark_mm2_pe.bam` + `test_R1_bismark_mm2_PE_report.txt` written; stderr contains **no** `faulty or non-existant`; summary shows `reference:` + `5-Base index:   (none: minimap2 reads the FASTA)`.
- **Negative control (same genome, no `--illumina_5base`):** exit **1**, `error: the Bowtie 2 index of the C->T->converted genome seems to be faulty or non-existant ('BS_CT.1.bt2l'). Please run the bismark_genome_preparation before running Bismark`.
- **Sharpest check:** the same 5-Base run against a copy of that genome carrying **14 empty placeholder index files** (6+6 `.bt2` + 2 `.mmi`) produced output identical to the placeholder-free run — SAM identical except the two `@PG CL:` lines, which differ only in the genome/output paths passed on the command line; `*_PE_report.txt` byte-identical modulo the genome path. Caveat: with the plan's own recipe (pUC19 reference, human/lambda test reads) 0/5000 pairs map, so the compared BAMs carry header only. The mapped-record form of the same property is covered by item 29 — those 7 tests now align real reads with real minimap2 against genome dirs that contain no bisulfite index at all.

## Gaps (detail)

### Item 23 (§5d.9): two integration tests were repointed rather than added — DEVIATED, documented

**Expected:** two new hermetic tests in `tests/aligner_cli.rs`, `five_base_bowtie2_needs_no_bisulfite_index` and `five_base_minimap2_needs_no_bisulfite_index`, modelled on `five_base_bowtie2_unconverted_index_end_to_end`, asserting the `XM` tag; plus `make_genome_fasta_only`.

**Found:** `make_genome_fasta_only` exists. No tests with those names. Instead the two existing engine-route end-to-end tests were repointed at it (§10 records this as A's I1, chosen over "adding near-clones alongside repointed originals").

**Assessment — the intended coverage exists.** §5d.9's four substantive requirements are all met by the repointed pair:

1. *Both engine routes.* `five_base_pe_end_to_end_inverts_polarity` covers the default minimap2 route (the `BS_CT.mmi` case that §2 row 2 calls the larger and more common half); `five_base_bowtie2_unconverted_index_end_to_end` covers `--bowtie2 --five_base_index`.
2. *No bisulfite index present.* Both now build the genome with `make_genome_fasta_only`, which writes `genome.fa` and nothing else — so the run provably passes discovery with no `Bisulfite_Genome/` directory at all. Previously they used `make_genome_mmi` / `make_genome`, i.e. they would have passed either way; the property is newly load-bearing in both.
3. *Hermetic / needs no real aligner.* Both use the fake `minimap2`/`bowtie2` scripts via `--path_to_*`, so they run on any CI runner.
4. *Asserts the `XM` tag, not just exit status.* Both assert `xm == b".Z...z"` on the R1 record read back out of the BAM, and additionally assert the report's option string and, for bowtie2, the absence of `--norc`.

Both pass (item 28). The only thing lost versus the plan's letter is a test *name* that says `needs_no_bisulfite_index`; the discovery property is instead carried by the repointed helper call and its `#1099` comment. §10 also records the deliberate retention of `make_genome_mmi` at the other 5-Base sites so 5-Base-on-a-prepared-genome stays covered — the gap both reviewers warned about is explicitly closed, and item 24 confirms four such sites remain.

**No action required.** Documented deviation, coverage intact.

### Item 25 (§5e): sabotage claim — audited, not re-run

§5e requires that making the branch unconditional turns **both** the new resolve-level guard **and** the pre-existing `missing_index_errors` red, and that the former fails *because the variant stops being `FaultyIndex`* rather than merely because something changed. §10's table claims all three sabotage observations. Re-running would require editing tracked files, which this audit must not do, so the claim was assessed for specificity and mechanism instead:

- **`faithful_resolve_still_requires_the_converted_index` → red, non-vacuously.** Credible and checkable by reading the code. Under an unconditional `discover_genome_fasta_only`, discovery on a FASTA-only dir succeeds and `resolve` proceeds to `detect_aligner`, which execs `bowtie2 --version`. With no bowtie2 the error becomes `AlignerNotWorking` and the `matches!(err, FaultyIndex{..})` assertion fails, printing the actual variant; with a working bowtie2, `resolve` returns `Ok` and `.unwrap_err()` panics. Red either way, and in the CI shape for exactly the reason §5e demands.
- **`missing_index_errors` → red.** Credible. It runs the real binary with no `--path_to_bowtie2`; with discovery no longer probing the index, the `"faulty or non-existant"` stderr predicate cannot match whatever failure comes later (missing/failing bowtie2, or bowtie2 rejecting a nonexistent `-x`), so the predicate fails regardless of what is installed.
- **`genome_folder_errors_survive_the_shared_prologue` → red on `canonicalize(genome_arg)?`.** Credible, and §10's wording ("caught the `Io` downgrade exactly") matches the mechanism: `AlignerError::Io(#[from] std::io::Error)` at `error.rs:16` means `?` yields `Io`, so the missing-path probe fails the `GenomeFolder` match while the path-to-a-file probe still passes via the `!is_dir` arm. This is the §7 risk the plan called rev 0's non-existent mitigation.

Each claim names a specific test, a specific sabotage, and a specific reason — not "tests went red". Accepted as recorded.

## Observations (outside the ledger — not plan gaps)

These are factual notes on things the plan did not specify. Recorded for the reviewers/Felix, not counted against coverage.

- **O1 — `discover_genome` lost its doc comment in the extraction.** `discovery.rs:134-135` still holds the original "Discover the genome folder, validate the bisulfite indexes for `aligner` … and inventory the raw FASTA file(s)." — but those lines now sit directly above `absolute_genome_dir` (`:140`) and so document *it*, followed by that function's own two-paragraph doc. `discover_genome` at `:149` now has no doc comment at all. Task 1.1's functional requirement (extraction, `GenomeFolder` on both arms, called from `discover_genome`) is fully met; this is a doc-attachment artifact.
- **O2 — the `mod.rs:943` reference in the new field docs is off by three lines.** Task D4's edit removed 3 net lines earlier in `mod.rs`, so the `if config.five_base` short-circuit the docs name as the guarantor is now at `mod.rs:940` (`:943` currently lands on the `ReadLayout::SingleEnd` arm inside the same block). `parallel.rs:597` is still exact. Task 1.3 named the line the plan told it to name; the plan's own number went stale within the same change.
- **O3 — `rust/README.md` is unmodified.** Task 6's two bullets require only `CHANGELOG.md` and the docs page, and both are done. But Task 6 cites `6a3ea0d` as precedent for "this page + `rust/README.md` alongside the CHANGELOG", and that commit did touch `rust/README.md`. The README's own rule (`rust/README.md:175`) binds "every module-merge PR into `master`", which this bug-fix branch into `dev` arguably is not. Flagging the ambiguity rather than scoring it.
- **O4 — two counts in the plan/§10 differ from the tree.** D3/§5d.10 says "13 call sites"; the actual repointed test sites are 9 (2 direct in `aligner_cli.rs` + 7 callers of the two groundtruth helpers) — all four *locations* the plan named were changed. §10 says `make_genome_mmi` was retained at "the other three 5-Base sites"; there are four (`make_genome_mmi` at `:6234`, `:6353`; `make_genome` at `:6327`, `:6407`). Neither affects coverage.
- **O5 — `make_genome_fasta_only` is the seventh `make_genome*` helper**, not "the fifth" as §5d.9 says (six pre-existed: `make_genome`, `_chr1`, `_ht2`, `_mmi`, `_combined`, `_combined_hisat2`). It was added as specified.

## Test verification (Mode B)

| Test name | File | Status |
|-----------|------|--------|
| `fasta_only_needs_no_bisulfite_index` | `rust/bismark/src/aligner/discovery.rs` | PASS |
| `fasta_only_accepts_a_gzipped_fasta` | `rust/bismark/src/aligner/discovery.rs` | PASS |
| `fasta_only_still_requires_a_fasta` | `rust/bismark/src/aligner/discovery.rs` | PASS |
| `genome_folder_errors_survive_the_shared_prologue` | `rust/bismark/src/aligner/discovery.rs` | PASS |
| `discover_genome_for_run_skips_the_index_check_for_five_base` | `rust/bismark/src/aligner/config.rs` | PASS |
| `five_base_resolve_does_not_require_the_converted_index` | `rust/bismark/src/aligner/config.rs` | PASS |
| `faithful_resolve_still_requires_the_converted_index` | `rust/bismark/src/aligner/config.rs` | PASS |
| `five_base_pe_end_to_end_inverts_polarity` (repointed, minimap2 route) | `rust/bismark/tests/aligner_cli.rs` | PASS |
| `five_base_bowtie2_unconverted_index_end_to_end` (repointed, bowtie2 route) | `rust/bismark/tests/aligner_cli.rs` | PASS |
| `missing_index_errors` (pre-existing over-reach guard, unmodified) | `rust/bismark/tests/aligner_cli.rs` | PASS |
| 7 × `five_base_*` groundtruth (FASTA-only genome dirs, real minimap2) | `rust/bismark/tests/aligner_five_base_groundtruth.rs` | PASS |
| `five_base_bowtie2_needs_no_bisulfite_index` | `rust/bismark/tests/aligner_cli.rs` | ABSENT — coverage supplied by the repointed tests (item 23) |
| `five_base_minimap2_needs_no_bisulfite_index` | `rust/bismark/tests/aligner_cli.rs` | ABSENT — coverage supplied by the repointed tests (item 23) |

Suite totals actually observed: `aligner::discovery` 25/25, `aligner::config` 45/45,
`--test aligner_cli five_base` 6/6, `--test aligner_five_base_groundtruth` 7/7,
`cargo fmt --check` clean, `cargo clippy --all-targets -D warnings` clean with and without
`--features rammap-inprocess`. Full-crate `cargo test -p bismark`: green — completed through
`Doc-tests bismark` with no failures (item 30).

## Verdict

**COMPLETE.** Every §0 decision, §4 task, §5 test stream and §6 verification step is accounted for.
All four §0 decisions were taken as the plan's leans recorded, including D1's cut, which is filed as
a real, open issue (#1100) carrying the three implementation hazards and the `-o` defect rather than
being dropped. The one deviation (§5d.9) is documented in §10 and, on inspection, delivers the
coverage the plan asked for: both engine routes, no bisulfite index present, hermetic, `XM` asserted.
The §5e both-guards-red requirement is met as recorded and the claim survives a mechanism audit.
The user-level fix was reproduced live — a FASTA-only genome now runs 5-Base to exit 0 with no
`faulty or non-existant`, while the same folder is still rejected for a bisulfite run — and the
14-placeholder identity check confirms the converted indexes are never read.

Nothing must be addressed before this is considered implemented. The five items under
**Observations** are optional polish outside the plan's scope; O1 (the doc comment now attached to
the wrong function) and O3 (`rust/README.md`) are the two most likely to matter to a reviewer.

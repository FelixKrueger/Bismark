# Session Handoff — 2026-08-08

**Everything from the last handoff's menu is closed or waiting on outsiders.** `dev` is at **`31bae51`**, clean, pushed, in sync — and its CI is **fully green for the first time since `7be9dde`**.

> 🚫 **STILL NO RELEASE.** `dev`→`master` = the release signal; the 3.2.0 cut stays a deliberate act (reopen PR [#1096](https://github.com/FelixKrueger/Bismark/pull/1096) when wanted). `dev` is now ~24 commits ahead of `master`. The five version literals reading `3.1.0` remain correct — leave them.

**Next session: nothing is urgent.** Watch two external threads:

1. **[nloyfer/wgbs_tools#120](https://github.com/nloyfer/wgbs_tools/pull/120)** — the `--five_base` polarity flag, **submitted 2026-08-08**, awaiting the maintainer. Fork `FelixKrueger/wgbs_tools`, branch `five-base-polarity`, commit `fd16352`. If revisions are requested, see §5 (wgbs_tools gotchas) before touching anything.
2. **Bismark #787** — @Danielsm8 still hasn't answered the contig-intersection diagnostic (last comment remains ours, `5220352348`). Not a blocker.

---

## 1. What we accomplished

Three commits on `dev` (`a33ac4e..31bae51`, 12 files, +1303/−59); PR #1097 squash-carried five feature-branch commits.

| Commit | What |
|---|---|
| `26eb28a` | **wgbs_tools check 2 closed** — flag-gated `--five_base` patch written (plumbing read first), byte-identity proven at 3 levels, draft finalized, patch file saved |
| `fc265ba` | **PR [#1097](https://github.com/FelixKrueger/Bismark/pull/1097) merged** — the two #1095 validation gates + the CI samtools repair (full pipeline: plan rev 0→1, dual plan review, implement, dual code review A+B APPROVE, coverage COMPLETE, 3 agreed Low fixes, CI 6/6 green) |
| `31bae51` | **Upstream PR submitted** as nloyfer/wgbs_tools#120 |

**wgbs_tools evidence (check 2, "we broke nothing"):** patch-present-flag-off `.pat` is **byte-identical** to stock for (a) `patter` standalone on synthetic mixed-methylation PE bisulfite, (b) `patter` standalone on **real WGBS chr21** — 59,250 pat lines, 0 differences, (c) a full `wgbstools bam2pat` run (patched vs stock `bam2pat.py`, decompressed `.pat.gz`). Positive controls: flag-on = exact per-line C↔T mirror in all three settings, and **byte-identical to the old unconditional-swap binary** — carrying checks 1/4 over by observation. All four validation checks now pass; full record in `plans/08062026_five-base-bisulfite-bam/EXPERIMENT_patter_flag_gated.md` + `five_base_wgbs_tools.patch`.

**#1097 (branch `1095-validation-gates`, G23 branch-first honored):**
- **Gate 1 (§9.8):** `XG == "CT" ⟺ FLAG ∉ {16,83,163}` per record over `nondir_pe_1030.bam` (record census 4/4/6/6 + **pair-structure census** pinning the #1030 swap `(147,99)×4/(163,83)×6`) and the SE fixture (`(CT,0)×4/(GA,16)×4` — the only committed FLAG-16 witness).
- **Gate 2:** idempotence round-trip parameterised over both PE dedup fixtures (20 and 12,974 records, byte-identical).
- **CI repair:** samtools mirrored into `rammap-inprocess` + `binseq-input` jobs — they run the #1095 gates and had been red on `dev` since `7be9dde` (exactly 3 samtools-guard panics each, verified from the run logs).
- Every census constant measured off the fixtures; every new assertion observed failing once under sabotage before being trusted; 11/11 green, fmt + clippy clean (default + `rammap-inprocess`).

## 2. What's still pending

| Item | State |
|---|---|
| wgbs_tools#120 | Submitted, awaiting nloyfer. Memory entry `project_wgbs_tools_upstream_pr.md` tracks it |
| #787 | Awaiting @Danielsm8's contig-intersection result |
| Release 3.2.0 | Deliberately deferred (banner above) |
| Remaining G9 gaps (3 of 5) | Deliberately out of scope, recorded in `plans/08072026_1095-validation-gates/PLAN.md` Open-3: unmapped pass-through fixture, lower-case `MD`, fixture letter census |
| Declined review extensions | Gate 1 over the synth fixture (reviewer verified 0 violations — free if wanted); PE header/`@PG` preservation gate. Both in PLAN §12 |
| PR #1091 (rapidgzip) | Open, not ours, untouched |

## 3. Key decisions (with rationale)

| Decision | Rationale |
|---|---|
| **CI samtools fix carried in the gates branch**, not a separate `dev` hotfix | Two line-edits that belong with the #1095 follow-up; the PR's green feature jobs then demonstrate repair + new gates in one run (Felix accepted this) |
| **Gate 1 set-form only** (`FLAG ∉ {16,83,163}`); bit gloss deleted | `FLAG & 0x10` is provably false on PE — FLAG 147 is reverse *and* `XG:CT` (10/20 fixture records). Both plan reviewers caught my rev-0 self-contradiction |
| **Pair-structure census added to Gate 1** | "Only fixture with all four XG/FLAG combinations" was false — the synth PE fixture has all four at *record* level; `nondir_pe_1030.bam`'s uniqueness is the *pair-level* #1030 swap, which a record census can't guard |
| samtools-text gates, not noodles | Idiom-consistency with the whole file; the CI panic guard already prevents silent skips. A noodles port is a coherent future whole-file refactor |
| Only both-reviewer Low fixes applied post-review | Census-before-converter, hoisted `count_of`, tag-scoped XG scan. Scope extensions declined and recorded rather than silently absorbed |
| **#120 posture: friendly paragraph + folded AI-assisted details** | Felix asked for friendly/human; global upstream rules demand the short visible paragraph, reasoning in the commit message. `add_cpg_counts.h` sibling constants disclosed as optional follow-up |
| wgbs_tools patch overrides public `OT`/`OB` in `main.cpp`, no header/constructor change | The only construction site is `main.cpp`; the entire behaviour change sits inside the flag's `if`, making "default path untouched" reviewable by inspection and provable by the byte gate |

## 4. Files modified

| Path | |
|---|---|
| `rust/bismark/tests/aligner_five_base_bisulfite.rs` | 7 → 11 gates: `assert_xg_iff_flag` + 2 contract tests, `assert_bisulfite_round_trip` + 2 PE tests, `dedup_fixture`, `count_of` |
| `.github/workflows/rust_ci.yml` | samtools in both feature jobs (identical blocks — one `replace_all`) |
| `plans/08072026_1095-validation-gates/` | **New feature dir**: PLAN (rev 1 + §12), PROGRESS, PLAN_REVIEW_A/B, CODE_REVIEW_A/B, COVERAGE (Verdict COMPLETE) |
| `plans/08062026_five-base-bisulfite-bam/` | DRAFT (now marked SUBMITTED #120), **new** `EXPERIMENT_patter_flag_gated.md`, **new** `five_base_wgbs_tools.patch` |
| `SESSION_HANDOFF.md` | This document |

**Outside the repo:** fork `FelixKrueger/wgbs_tools` created (branch `five-base-polarity`, commit `fd16352`); memory `project_wgbs_tools_upstream_pr.md` + MEMORY.md line added; Bismark PRs #1097 (merged) and nloyfer/wgbs_tools#120 (open) created.

## 5. Gotchas and constraints

### New this session

**H1 — 🔑 `patter` flags must FOLLOW the two positionals** (`patter DICT REGION [flags]`). A leading flag becomes the dict path and the error surfaces as `tabix: unrecognized option '--five_base'` — and the run produces an **empty** output file, which makes a "differs from stock" check pass vacuously. `bam2pat.py` appends flags after positionals, so the CLI patch is consistent.

**H2 — `wgbstools init_genome` silently filters non-chromosome contig names** (`is_valid_chrome`, `init_genome.py:278`: `^(chr)?(\d+|[XYM]|MT)$`). Symptom: "Invalid input argument / No objects to concatenate". Rename the contig (e.g. `pUC19` → `chr1`) for end-to-end runs.

**H3 — `gh pr checks --watch` can miss late-registering jobs.** It reported 5 job types green while `binseq-input` was absent from the snapshot; the run was actually 6/6. **Declare CI green only from `gh run view <id> --json jobs`.**

**H4 — the dual-driver CI trap, concretely:** any tool-availability panic guard added to tests must be mirrored into **both** feature jobs (`rammap-inprocess`, `binseq-input`) — they run `cargo test -p bismark` too. The minimap2 guard was mirrored when added; samtools' wasn't, and `dev` sat red from `7be9dde` until #1097.

**H5 — shell traps:** a bare `===` token in a compound command aborts it in this zsh env (`=word` triggers =command expansion). A Bash call containing `rm -rf` is denied by the permission layer — use fresh directory names instead of cleanup.

**H6 — `wgbstools bam2pat --no_beta`** skips the beta step, which otherwise exits 127 unless `stdin2beta` is also compiled (`setup.py -t` builds only named targets). The `.pat.gz` is complete before the beta step either way.

**H7 — Bismark drops a PE pair whose mate sits at POS 1** from the BAM while the report still counts it as a unique alignment (fixture `m002_OB_0`, pUC19 fragment [0,180)). Boundary quirk, not chased; don't let it confuse a record-count reconciliation.

**H8 — `chunks(2)` pair logic on Bismark PE BAMs is sound** only because R1/R2 are file-order adjacent with shared QNAME — assert both (the Gate 1 pair census does).

### Carried forward (verbatim-critical)

**G13 — 🔑 `wgbs_tools` `setup.py` does NOT fail on a compile error** (the `raise` is commented out, `setup.py:33`). A broken module prints red `FAIL`, exits 0, and the old binary stays. Always `cmp` binaries after a rebuild.

**G19/G20 — 🔑 vacuous verification + reviewer fallibility.** Both bit again this session: the empty-output "DIFFERS" (H1) and the checks-list missing job (H3) were caught only by looking at *what kind* of result appeared; and my own rev-0 plan carried a provably-false bit-form that only the dual review caught. A check whose failure you have never observed is not yet a check; verify contested reviewer claims against source, never pick a reviewer.

**G21/G22 — `gh` traps:** `gh api` doesn't paginate by default (truncated page ≙ absent record); quote URLs containing `?` (zsh globs); `gh`/`git push`/`curl` need `dangerouslyDisableSandbox`; `$TMPDIR` differs between sandboxed and unsandboxed calls.

**G23 — BRANCH FIRST** — honored this session (`1095-validation-gates` → PR #1097 → squash into `dev`); keep it that way. **G25** — merging to `dev` does not close linked issues. **G28/G29** — check `git status` before `gh pr merge --delete-branch`; verify a squash-merge by `git diff <tip> origin/dev` (expect 0 lines) + PR state, never ancestry.

**G26/G27 — Rust CI gates:** workflow-scope `RUSTFLAGS: -D warnings` means feature-gated warnings fail CI while local default gates stay green — run `cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings`; `cargo fmt --check` is its own job.

### #1095 design facts

The 12 design facts (G1–G12 of the previous handoff: `iter_aligned()` ban, `NM` counting soft-clips, the §3.6 masking rule, `Drop`-writes-EOF refusal contract, etc.) now live in `plans/08062026_five-base-bisulfite-bam/PLAN.md` §9–§10b and its reviews — consult those before touching `five_base_bisulfite.rs`; do not re-derive them.

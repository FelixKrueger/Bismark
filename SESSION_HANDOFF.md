# Session Handoff — 2026-08-17 (eight merges; **release explicitly deferred by Felix**)

**`dev` is `8a6dc9b`, 58 ahead of `master`, clean tracked tree, no open PRs.** Thirteen commits — #1106 `3a6aab8`, #1107 `04c3069`, #1108 `b502110`, #1109 `b2e0a12`, #1110 `e8ab722`, #1111 `2c69a5e`, **#1112 `d823311`**, #1113 `182e221` — plus five handoff commits. Seven were docs/CI/plan housekeeping; **#1112 is the one Rust source change.**

> 🛑 **DO NOT PROPOSE A 3.2.0 RELEASE.** Felix, verbatim: *"I don't want a 3.2.0 in the middle of work, please don't suggest that until everything looks calm and finished. I still see 4 open issues for the 5-base work."* The four (#1095, #1099, #1100, #1104) are **fully implemented on `dev`** and open only because merging into `dev` closes nothing (G25) — that does not matter. Open issues are his signal a feature area is still moving. Report readiness as fact if asked; never propose the cut. Release-path chores inherit this (version bumps, `dev`→`master` PRs, the `docs.yml` flip). Saved as the `feedback_no_release_mid_flight` memory.

> ✅ **The pending list contains no work items.** What remains is the deferred release, two facts about it, and one externally-blocked validation. `npm audit` in `docs/` on `dev`: clean. 0 broken internal docs links. CI green on every merge.

> 🔎 **Four of this session's "pending" items turned out to be stale, not outstanding** — the Milestones "undecided" split, the `COVERAGE.md` verdict, and two of #1104's three follow-ups. **Check whether a carried item is still real before doing it** (§5 N41).

## 1. What we accomplished

| Item | Outcome |
|---|---|
| **#1103 resolved** (`3a6aab8`, PR #1106) | `master`'s two `docs/` manifests taken verbatim onto `dev`, then **js-yaml 4.3.0→4.3.1** and **nanoid 3.3.16→3.3.18**. `dev`'s lockfile had been 4 bumps behind. `npm audit`: **11 high / 10 moderate → 0**. Verified: 34 `libc` entries preserved, exactly 2 version entries differ from `master`, `npm ci` clean, 27 pages build. #1103 closed with an explanation of why it could not self-close |
| **The alert-count discrepancy settled** | Alerts API says **1 open**; `npm audit` and the `git push` banner both say **2 high**. The second is **nanoid, which has no Dependabot alert at all**. The **API under-reports** — the banner was right |
| **`dependabot.yml` added** (`04c3069`, PR #1107) | npm, `/docs`, `target-branch: dev`, weekly, grouped, limit 3. GitHub's own config validator passed as a check-run. ⚠️ It does **not** do what it was asked for — §3 and §5 N25 |
| **Milestones gap closed** (`b502110`, PR #1108) | #1099 and #1100 both shipped **2026-08-11** and had never touched `rust/README.md`, leaving a dated hole. Two entries added; also fixed the journal's own policy line, which said *"into `master`"* |
| **EpiGnome kit retired** (`b2e0a12`, PR #1109) | The discontinued TruSeq DNA-Methylation kit is out of `usage/library-types.md`: table row, prose section, dead `http://` illumina.com link. **0 insertions / 13 deletions.** The mispriming sentence keeping EpiGnome as a *class* example was left byte-for-byte |
| **PBAT → flat 8 bp** (`e8ab722`, PR #1110) | Table `6N / 9N` → **`8 bp` / `(8 bp)`**, prose rewritten because it derived the number from the oligo. Carried the retired EpiGnome section's argument across so it was not lost. Knock-ons: the PBAT FAQ's `--clip_r1 6 --clip_r2 6` → `8 8`, and **the dead Perl-manual link fixed in TWO FAQs**. Verified on the live site |
| **#1100 coverage verdict resolved** (`2c69a5e`, PR #1111) | It read `INCOMPLETE` because it audited an **uncommitted working tree** vs `688d919`, before the Phase-5 fixes; the merged squash is `7596b0d`. All four gaps verified closed, tests **run not just located**, recorded as a Resolution section with the ledger left intact |
| **#1104 follow-ups closed** (`d823311`, PR #1112) | One real fix of three filed items. `simplex`-only mode now **reports the duplex families it excludes** — the skip site claimed they were *"counted for the report"* while nothing counted or printed them, so that mode gave a numerator with no denominator. Clause prints **only when no duplex line does**. Plus `debug_assert!(n >= 1)` beside the family-size bucket subtraction. **Three sabotages, each red for its own reason**, incl. the assert firing at `mod.rs:2576` ahead of the subtraction. The other two items were **already declined** and **already covered** — §3 |
| **#1104's release note updated** (`182e221`, PR #1113) | The new report figure is noted as **a clause on #1104's existing `Unreleased` bullet**, not a thirteenth bullet — the reader deciding whether an upgrade affects them is already in that entry for the feature. I had left this as Felix's call and he took it; 1 insertion / 1 deletion, bullet count unchanged at 12 |
| **The 4 open 5-Base issues audited** | All four implemented on `dev`, absent from `master`. #1099/#1100/#1104 have **zero comments** (self-filed trackers); #1095 has 9 with the **last one ours** (14 Aug) — ball is with @Danielsm8. **Nothing owed, nothing half-done** |
| **`docs.yml` resolution reversed** | See §3. The earlier call (deploy from `master`) was wrong; keep **`dev`'s** copy |

## 2. What's still pending

**No work items remain.** What is left is the deferred release, two consequences of it, and one externally-blocked validation.

| Item | State |
|---|---|
| **Release 3.2.0** | 🛑 **Deferred by Felix — do not raise.** Gate: the four 5-Base issues closed, or he brings it up. `rust/VERSION` still `3.1.0` on both branches; #1096 closed, so a cut needs a fresh `dev`→`master` PR |
| **`docs.yml` conflict** | ⚠️ **`dev` and `master` conflict on `.github/workflows/docs.yml`, which blocks ANY `dev`→`master` merge.** Resolution **decided: keep `dev`'s copy** (`push: [dev]`, `pull_request: [dev, master]`). Shelved with the release; applying it now would freeze the published docs |
| **`dependabot.yml` is inert** | Read from the **default branch only**, so it does nothing until it reaches `master`. Activates itself at the next release merge |
| **Security PRs still target `master`** | `target-branch` governs *version* updates only, so the next advisory opens on `master` and puts `dev` behind again. The resync manoeuvre is `3a6aab8`. `master` currently shows **2 high** (js-yaml + nanoid), clearing when it takes `dev`'s lockfile |
| **Biological V9 for #1104** | ⛔ **Not actionable here** — the Illumina 5-Base demo dataset is gone from oxy. Correctness rests on synthetic groundtruth gates + prior DRAGEN concordance. PLAN §11c |
| One cosmetic call, undecided | Whether the June Milestones date-order blip (position 19: `2026-06-27` after `2026-06-25`) is worth straightening. Pre-existing, harmless, and below my insertions |
| Pre-existing, deliberately untouched | The header row's `</tr>` in `library-types.md` is under-indented vs the body rows. A `<td colspan="2">` row in `legacy_perl/plotly/bismark_bt2_PE_report.html` — **correct HTML**, pinned by the Rust `embedded_assets_match_repo_plotly_files` test, so do not "tidy" it |

**Not this repo's item: TrimGalore#440.** Felix, 2026-08-16: *"The TrimGalore side is handled elsewhere."* The `--library pbat` preset work is tracked outside this repo — **do not re-add it as pending here.** What Bismark owed it is done: `usage/library-types.md` is now the clean upstream source those presets would be built from (PBAT 8 bp, no retired kit).

## 3. Key decisions (with rationale)

| Decision | Rationale |
|---|---|
| **Close #1103 instead of re-targeting it at `dev`** | A live `compare/dev...<branch>` showed **diverged, 3 files**, not the 1 file / 3 lines the PR still advertised, and a test merge **conflicted** on `docs.yml`. Re-targeting would have settled a docs-deploy question inside a Dependabot branch that may force-push over it |
| **Take `master`'s manifests verbatim rather than `npm audit fix` on `dev`** | `npm audit fix` on `dev` resolves to astro 7.2.2 / postcss 8.5.26 — **past `master`'s** — reversing the divergence instead of ending it |
| **Surgical 3-line edits, not a regeneration** | `npm audit fix` on macOS **stripped the `libc` constraints from all 34 platform-specific entries**; the ubuntu-latest docs build needs them for sharp's libvips. Dependabot's js-yaml diff was Linux-generated, so it was reused |
| **Hand-written integrity hash is safe** | npm's `integrity` is content-addressed — `npm ci` downloads and compares SHA-512, so a wrong hash fails loudly. The *regeneration* is the risky path: it silently rewrites metadata nothing local checks |
| **🔄 REVERSED: keep `dev`'s `docs.yml`, not `master`'s** | Deploy-from-`dev` was deliberate (#1072) because **Pages serves one "latest"**, so a later `master` push of older docs regresses the site. #1089's title and comment are about adding a **PR build** for master-targeted Dependabot PRs — the deploy flip reads as collateral |
| **Wrote `dependabot.yml` despite it not achieving the stated goal** | Felix asked for `target-branch: dev` to stop bumps landing on `master`. It cannot do that (N25). Delivered with both limits stated, because scheduled version updates on `dev` are what stops the drift recurring |
| **#1099/#1100 get Milestones entries — the Medium/Low split was moot** | The log had a **dated hole**: every other fix of comparable weight has a line, including #1092, also "a flag accepted and ignored". Consistency decided it, not priority |
| **Kept EpiGnome in the mispriming sentence** | Felix's explicit instruction, and the reason is worth preserving: it is a statement about random priming **as a class**, true of a retired kit |
| **PBAT prose rewritten, not just the table cell** | Felix named this as the most likely half-done outcome. A cell saying 8 with a paragraph saying "it depends on the oligo" is worse than neither change |
| **Ecosystem rationale in the commit message, not the page** | The brief asked to carry the nf-core/methylseq + TrimGalore#440 reasoning "into the prose" but also forbids referencing the unshipped `--library` flag; an upstream source citing a downstream consumer inverts the dependency being protected |
| **Recorded the coverage resolution instead of rewriting the ledger** | The audit's findings were real when taken; "four gaps found and closed" is the fact worth keeping. Editing the rows away would have left no trace they existed |
| **#1104: the family-size tag stays declined** | `PLAN.md:118` already declined it with a reason (*"additive later, no one asked"*, B-alt 3) and nobody has asked since. Doing it would reverse a documented decision unbidden and widen the aux-tag surface in non-default modes |
| **#1104: §3.5-vs-§3.7 resolved in favour of §3.5** | The duplex figure was genuinely missing **and the code claimed otherwise** — the skip comment said "counted for the report". Printing it (only where no duplex line prints) makes the comment true and gives `simplex` users their denominator; correcting the plan instead would have left the mode reporting a numerator alone |
| **#1112's release note is a clause, not a new bullet** | I shipped #1112 with no CHANGELOG change and flagged it rather than deciding; Felix asked for the clause, so #1113 added it **inside #1104's existing bullet**. A second bullet would have split one behaviour across two entries in a section already carrying five output-changing ones |
| **No dual code-review agents all session** | The session instruction forbids the Agent tool unless requested. For #1112 the substitute was three sabotages plus the full 14-run matrix; for the docs work, `npm ci`, the CI build, a rendered-page parse and a live fetch. Flagged rather than skipped silently |

## 4. Files modified

| File | Change |
|---|---|
| `docs/package-lock.json`, `docs/package.json` | `master`'s copies + js-yaml 4.3.1 + nanoid 3.3.18 (`3a6aab8`) |
| `.github/dependabot.yml` | **New** — npm `/docs`, `target-branch: dev`, weekly, grouped (`04c3069`) |
| `rust/README.md` | Milestones: two 2026-08-11 entries + policy-line fix (`b502110`); one 2026-08-17 entry (`d823311`) |
| `docs/src/content/docs/usage/library-types.md` | EpiGnome row/section/link removed (`b2e0a12`); PBAT row + prose to a flat 8 bp (`e8ab722`) |
| `docs/src/content/docs/faq/single-cell-pbat.md` | PBAT clip 6→8, dead link, unbalanced paren (`e8ab722`) |
| `docs/src/content/docs/faq/low-mapping.md` | Dead link — the second instance (`e8ab722`) |
| `plans/08112026_1100…/COVERAGE.md` | Resolution section, +38/−1, ledger untouched (`2c69a5e`) |
| **`rust/bismark/src/aligner/mod.rs`** | Simplex report clause + corrected skip comment + `debug_assert!(n >= 1)` (`d823311`) |
| **`rust/bismark/tests/aligner_five_base_simplex.rs`** | Two assertions — the figure in simplex-only mode, its absence in `both` (`d823311`) |
| `plans/08142026_5base-simplex-consensus/PLAN.md` | §11d follow-up disposition (`d823311`) |
| `CHANGELOG.md` | One clause on #1104's `Unreleased` bullet naming the simplex-mode duplex figure (`182e221`) |
| `SESSION_HANDOFF.md` | `6230532`, `e3d0666`, `1c6697d`, `3471ece`, `fce4541`, and this refresh |

**Untracked:** 93 entries. Note **535 files under `plans/` ARE tracked** — check before assuming. Also stray test artifacts in the source tree (`rust/bismark/reads.fq_C_to_T.fastq`, `test_R1.fastq.gz_C_to_T.fastq`, `.nf-test.log`): a test writes into the crate dir, so **never `git add -A`** — every commit this session used explicit paths.

| Outside the repo | Change |
|---|---|
| GitHub | PRs #1106–#1113 opened and squash-merged; **#1103 closed** with an explanatory comment; all eight branches deleted both sides |
| `~/.claude/…/memory/` | **New:** `feedback_no_release_mid_flight.md`. **Corrected:** `feedback_git_workflow_dev_master.md` (docs-deploy claim marked violated), `MEMORY.md` index |
| Live docs site | Redeployed from each `dev` push; verified by fetching the page that the retired kit is gone and PBAT reads 8 bp / (8 bp) |

## 5. Gotchas and constraints

### New this session

**N24 — 🔑 `npm audit fix` is a re-resolution, not a patch, and resolution is platform-specific.** On darwin it drops the `libc: ["glibc"|"musl"]` arrays from every platform-specific entry — 34 here. Nothing local fails; the loss only bites on the Linux runner that picks sharp's libvips binaries. **After any `docs/` lockfile change, assert the `libc` count is still 34.** Prefer a surgical edit, or reuse Dependabot's own Linux-generated diff.

**N25 — Dependabot is two products sharing one name.** *Version updates* are file-driven, schedulable, `target-branch`-aware, and **do not exist without a `dependabot.yml`**. *Security updates* are settings-driven, advisory-triggered, and **hardwired to the default branch — `target-branch` cannot move them**. Also **`dependabot.yml` is read from the default branch only**, so a config on `dev` is inert.

**N26 — The Dependabot alerts API under-reports; `npm audit` is the per-ref ground truth.** Alerts are curated advisory records attached to the default branch; `npm audit` resolves the tree live. **"0 open alerts" ≠ "0 vulnerabilities"**, and on a non-default branch it means nothing — `dev` sat at 11 high / 10 moderate while the API reported 1. Audit the ref you care about: `git show <ref>:docs/package-lock.json` into a scratch dir.

**N27 — `npm audit fix --dry-run --json` is not JSON in npm 11**, and with no `node_modules` a dry run lists the **entire tree** as `add …`. **Never infer delta size from a dry run** — diff the actual lockfile.

**N28 — 🔑 The empty-or-broken-check-as-verdict shape struck five more times.** (a) `gh pr merge` prints **nothing** on success. (b) `git push … | tail -6; rc=$?` captured **`tail`'s** status. (c) `git grep -E '\b(6N|9N)\b'` matched nothing — git's ERE has no `\b`. (d) A link check looked for `dist/Bismark/usage/...` when `dist/` **is** `/Bismark/`. (e) `git status --porcelain rust/` reported "dirty" after a sabotage revert that had in fact worked — the entries were untracked build dirs. **Every one was caught by a control**: a known-good link, a known-present string, an independent SHA comparison, `git diff HEAD -- <path>`. **Add a positive control to any search whose emptiness you are about to believe, and check reverts with `git diff HEAD`, not `git status`.** Also: `gh pr view` has **no `merged` field** — use `state` / `mergedAt`.

**N29 — Any path under `rust/` triggers the full Rust matrix.** A one-line `rust/README.md` edit ran **14 check-runs** plus the pre-push hook's local `fmt`+`clippy`; `.github/dependabot.yml` got 2, `docs/` 4, `plans/` 2. Neither CI nor the hook distinguishes Markdown from `.rs` — the filter is the path prefix.

**N30 — GitHub validates `.github/dependabot.yml` itself**, as a check-run named after the file. Green means every key is a *recognised* option — better than a local YAML parse, which cannot tell a valid-but-ignored key from a real one.

**N31 — 🔑 A defect described in a brief may not exist, and it was asked for twice.** Two briefs asked me to preserve a malformed one-cell `<tr>` near line 18 of `library-types.md`. It is not there and never was: **555 tracked `.md`/`.mdx`/`.html` files scanned** and `git log --all -S` over all three revisions the file has ever had. **Verify a described defect before preserving or fixing it, and say so** — silence reads as the instruction being ignored, and inventing a fix is worse.

**N32 — A cell-count linter flags every `colspan` row as irregular.** Cell count and column count are different things; a structural check must read `colspan`/`rowspan` before calling a row malformed.

**N33 — For "remove/keep some of N mentions", anchor on structure, not the term.** #1109's three removals were a `<tr>` block and a `### ` section while the survivor is prose in a different section: anchoring on the *next* structural element made the survivor unreachable by construction. Gate with `git diff --numstat` showing **0 insertions**, which also proves no formatter reflowed a neighbour.

**N34 — A multi-part brief's premise can be stale.** The second library-types brief asked for two edits; the first had already shipped as #1109 earlier the same session. Its instruction to reuse the deleted section's reasoning only worked because that text was recoverable (`git show b2e0a12^:<path>`).

**N35 — Internal docs links are base-prefixed absolute paths** (`/Bismark/usage/library-types/`); `astro.config.mjs` sets `base: '/Bismark/'`. To resolve one against `dist/`, **strip the base** — `dist/` is the site root. A whole-site check is cheap after any link edit; 0 broken internal links is the current state.

**N36 — Process substitution is blocked in the sandbox.** `diff <(cmd) <(cmd)` fails with `/dev/fd/12: Operation not permitted`, and the surrounding test then reads as a failure. Write to `$TMPDIR` files and use `cmp -s`.

**N37 — 🔑 A coverage audit taken on an uncommitted working tree has no stable referent.** #1100's `COVERAGE.md` read `INCOMPLETE` for five days after its gaps closed, because it audited a tree that no longer existed; the fixes landed in `7596b0d`. **Audit a commit, or at minimum record the SHA the fixes land in.** Resolve a stale audit with a Resolution section, not by editing the ledger.

**N38 — "The test exists" and "the test passes" are different claims.** Closing #1100's gaps meant locating `hisat2_env_fallback_accepted` *and running it*; that plan's §11 records a committed test that existed and was vacuous. `cargo test` takes only **one** positional filter — a second name is rejected as an unexpected argument, so run them separately.

**N39 — 🔑 A comment that narrates intent conceals the gap it describes.** #1112's skip site said *"simplex-only mode: counted for the report, never stored"*. Nothing counted them and nothing reported them — the duplex line is gated on `dpx_path`, `None` in that mode. The comment read as documentation of working behaviour, so the missing half stayed invisible through implementation, dual review and a coverage audit. This is the concrete cost of the CLAUDE.md rule that a comment must state **current fact**, not intent: an aspirational comment does not merely go stale, it actively hides the defect.

**N40 — Commit before sabotaging, then verify the revert by content.** With the work committed, a sabotage reverts with `git checkout -- <path>` (the #1100 plan records a sabotage that could not be reverted because the work was uncommitted, and `git checkout` on uncommitted work destroys it — N2). Confirm the revert with `git diff HEAD -- <path>`, since `git status --porcelain <dir>` also reports untracked build artifacts and reads as a failed revert. And **aim the sabotage at the live path**: forcing `n = 0` proved the `debug_assert` fires *ahead of* the bucket subtraction, which is the only thing that distinguishes it from dead code behind a panic that would happen anyway.

**N41 — 🔑 A carried "pending" item may already be done or already declined.** Four of this session's went that way: the Milestones Medium/Low split (a dated hole made it moot), the `COVERAGE.md` verdict (gaps closed five days earlier), and two of #1104's three follow-ups — the family-size tag was **declined with a reason in `PLAN.md:118`** and the in-run `both`/UMI test **already existed**. **Before implementing a carried optional, grep for the test and read the plan's declined list.** A handoff records what was true when written; "reviewer optional not taken" does not distinguish *not yet* from *decided against*.

### Environment

- **`gh` needs `dangerouslyDisableSandbox`** — sandboxed it fails TLS: `x509: OSStatus -26276`. **`git fetch/push` over SSH** likewise: `nc: authentication method negotiation failed`.
- **A scratch command containing `rm -rf` was blocked by the permission classifier.** Rewrite without it rather than retrying.
- **The pre-push hook works as documented** — docs-only pushes print *"no rust/ changes … skipping the Rust gates"* and are instant; a `rust/` push runs `fmt` + `clippy` (~3 min) and blocks on failure.
- **samtools IS on this machine** (`/opt/homebrew/bin/samtools`), so the 5-Base suites run for real rather than skipping (N21).
- `python3` f-strings reject a backslash inside the expression part — keep regex literals out of the `{…}`.

### Carried forward (still verbatim-critical)

**N1** — a run's `conclusion: success` ≠ its jobs finished; **enumerate check-runs and assert zero NULL conclusions** (done for all eight merges). **N3** — `gh run list --commit` needs the **full 40-char SHA**.

**N11/N12** — an unpushed base rides along in a PR and the PR object lies about it; `files`/`changedFiles` are snapshots refreshed on a **head** push, so moving the **base** never refreshes them (#1103 still advertised 1 file / 3 lines against `dev`). Use `compare/<base>...<head>`, and pass `--subject/--body-file` to `gh pr merge` explicitly.

**N16/N23** — verify a squash by **content**, never ancestry; `git branch -d` refuses a squash-merged branch forever, so `-D` after a content check is the only route. A merged branch can hold the only copy of something.

**N17/N18/N21** — a sabotage aimed at dead code proves nothing; a sabotage only tests the gate it is aimed at; `samtools_available()` skips silently and still reports `ok`, so the `$CI` panic guard is what makes those gates mean anything.

**G21/G22/G25** — credentialed tools need unsandboxing; `gh api` does not paginate by default; **`timeout` is not installed** (macOS); **`dev` merges close nothing**. **G23** BRANCH FIRST — all eight merges went through a branch + PR.

**Style** — one phrase is banned outright (see `feedback_never_say_load_bearing`). **Session `grep` is a ugrep wrapper** — use `command grep` for CI-shell logic. **`plans/` is off-limits unless Felix provides a file or asks** (he provided `08112026_1100…/COVERAGE.md`, and asking for the #1104 follow-ups authorised that plan dir). **Comments/CHANGELOG/PR replies: brevity is the default** (CLAUDE.md — three venues, one piece of feedback).

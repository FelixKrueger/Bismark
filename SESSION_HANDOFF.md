# Session Handoff — 2026-08-16 (six housekeeping merges; **release explicitly deferred by Felix**)

**`dev` is `2c69a5e`, 54 ahead of `master`, clean tree, no open PRs.** Nine commits — `3a6aab8` (#1106), `04c3069` (#1107), `b502110` (#1108), `b2e0a12` (#1109), `e8ab722` (#1110), `2c69a5e` (#1111), plus three handoff commits. **All docs/CI/plan housekeeping; no Rust source was touched.**

> 🛑 **DO NOT PROPOSE A 3.2.0 RELEASE.** Felix, verbatim: *"I don't want a 3.2.0 in the middle of work, please don't suggest that until everything looks calm and finished. I still see 4 open issues for the 5-base work."* The four (#1095, #1099, #1100, #1104) are **fully implemented on `dev`** and open only because merging into `dev` closes nothing (G25) — that does not matter. Open issues are his signal a feature area is still moving. Report readiness as fact if asked; never propose the cut. Release-path chores inherit this (version bumps, `dev`→`master` PRs, the `docs.yml` flip). Saved as the `feedback_no_release_mid_flight` memory.

> ✅ **The pending list is empty except for one non-urgent item.** Everything that did not need Felix is done: #1103 resolved, `dependabot.yml` added, the Milestones gap closed, the EpiGnome kit retired, PBAT moved to a flat 8 bp, the #1100 coverage verdict resolved. `npm audit` in `docs/` on `dev`: **clean**. 0 broken internal docs links.

## 1. What we accomplished

| Item | Outcome |
|---|---|
| **#1103 resolved** (`3a6aab8`, PR #1106) | `master`'s two `docs/` manifests taken verbatim onto `dev`, then **js-yaml 4.3.0→4.3.1** and **nanoid 3.3.16→3.3.18**. `dev`'s lockfile had been 4 bumps behind. `npm audit`: **11 high / 10 moderate → 0**. Verified: 34 `libc` entries preserved, exactly 2 version entries differ from `master`, `npm ci` clean, 27 pages build. #1103 closed with an explanation of why it could not self-close |
| **The alert-count discrepancy settled** | Alerts API says **1 open**; `npm audit` and the `git push` banner both say **2 high**. The second is **nanoid, which has no Dependabot alert at all**. The **API under-reports** — the banner was right |
| **`dependabot.yml` added** (`04c3069`, PR #1107) | npm, `/docs`, `target-branch: dev`, weekly, grouped, `build(deps)` prefix, limit 3. GitHub's own config validator passed as a check-run. ⚠️ It does **not** do what it was asked for — §3 and §5 N25 |
| **Milestones gap closed** (`b502110`, PR #1108) | #1099 (`1a4f8fa`) and #1100 (`7596b0d`) both shipped **2026-08-11** and had never touched `rust/README.md`, leaving a dated hole between the 08-15 and 08-07 entries. Two entries added, Milestones only, no Status-row change. Also fixed the journal's own policy line — it said *"into `master`"* |
| **EpiGnome kit retired** (`b2e0a12`, PR #1109) | The discontinued TruSeq DNA-Methylation kit is out of `usage/library-types.md`: table row, prose section, dead `http://` illumina.com link. **0 insertions / 13 deletions.** The mispriming sentence keeping EpiGnome as a *class* example was left byte-for-byte |
| **PBAT → flat 8 bp** (`e8ab722`, PR #1110) | Table `6N / 9N` → **`8 bp` / `(8 bp)`**, and the prose rewritten because it derived the number from the oligo, which contradicts a flat 8. Biology and QCFail link kept. Carried the retired EpiGnome section's argument across ("the bias extends to 7 or 8 bp") so it was not lost with that section. Knock-ons: the single-cell/PBAT FAQ's `--clip_r1 6 --clip_r2 6` → `8 8`, and **the dead Perl-manual link fixed in TWO FAQs**, not just the noticed one. Verified on the live site |
| **#1100 coverage verdict resolved** (`2c69a5e`, PR #1111) | The audit read `INCOMPLETE` because it was taken on an **uncommitted working tree** vs `688d919`, before the Phase-5 review fixes; the squash that merged is `7596b0d`. All four gaps verified closed, and the three named tests plus the two #1099 guard tests were **run, not just located**. Recorded as a Resolution section — the ledger is not rewritten |
| **The 4 open 5-Base issues audited** | All four implemented on `dev`, absent from `master`. #1099/#1100/#1104 have **zero comments** (self-filed trackers); #1095 has 9 with the **last one ours** (14 Aug) — ball is with @Danielsm8. **Nothing owed, nothing half-done** |
| **`docs.yml` resolution reversed** | See §3. The earlier call (deploy from `master`) was wrong; keep **`dev`'s** copy |
| **Prior handoff's counts corrected** | It said "nine Unreleased CHANGELOG groups" — actually **3 `###` groups / 12 bullets**, and **five** entries change output, not four (**#1092** was missed: `--rammap --mm2_short_reads` now takes effect, so alignments *and* mapping rate change) |

## 2. What's still pending

| Item | State |
|---|---|
| **Release 3.2.0** | 🛑 **Deferred by Felix — do not raise.** Gate: the four 5-Base issues closed, or he brings it up. `rust/VERSION` still `3.1.0` on both branches; #1096 closed, so a cut needs a fresh `dev`→`master` PR |
| **`docs.yml` conflict** | ⚠️ **`dev` and `master` conflict on `.github/workflows/docs.yml`, which blocks ANY `dev`→`master` merge** — independent of Dependabot. Resolution **decided: keep `dev`'s copy** (`push: [dev]`, `pull_request: [dev, master]`). Shelved with the release; applying it now would freeze the published docs |
| **`dependabot.yml` is inert** | Read from the **default branch only**, so it does nothing until it reaches `master`. Activates itself at the next release merge |
| **Security PRs still target `master`** | `target-branch` governs *version* updates only, so the next advisory opens on `master` and puts `dev` behind again. The resync manoeuvre is `3a6aab8`. `master` currently shows **2 high** (js-yaml + nanoid), clearing when it picks up `dev`'s lockfile |
| **#1104 follow-ups** — the only open work item | All non-urgent reviewer optionals: per-record family-size tag; in-run `both`-mode test for `--five_base_umi_qname`; `debug_assert!(n >= 1)` beside the histogram subtraction. Plus PLAN §3.5 promises a duplex figure in `simplex` mode that §3.7/V7 says is absent — plan self-inconsistency, code matches §3.7 |
| **Biological V9 for #1104** | ⛔ Still blocked: the Illumina 5-Base demo dataset is gone from oxy. Correctness rests on synthetic groundtruth gates + prior DRAGEN concordance. PLAN §11c |
| Pre-existing, deliberately untouched | One Milestones date out of order (position 19: `2026-06-27` after `2026-06-25`). The header row's `</tr>` in `library-types.md` is under-indented vs the body rows. A `<td colspan="2">` row in `legacy_perl/plotly/bismark_bt2_PE_report.html` — **correct HTML**, and pinned by the Rust `embedded_assets_match_repo_plotly_files` test, so do not "tidy" it |

**Not this repo's item: TrimGalore#440.** Felix, 2026-08-16: *"The TrimGalore side is handled elsewhere."* The `--library pbat` preset work is tracked outside this repo — **do not re-add it as pending here.** What Bismark owed it is done: `usage/library-types.md` is now the clean upstream source those presets would be built from (PBAT 8 bp, no retired kit).

## 3. Key decisions (with rationale)

| Decision | Rationale |
|---|---|
| **Close #1103 instead of re-targeting it at `dev`** | A live `compare/dev...<branch>` showed **diverged, 3 files**, not the 1 file / 3 lines the PR still advertised, and a test merge **conflicted** on `docs.yml`. Re-targeting would have settled a docs-deploy question inside a Dependabot branch that may force-push over it |
| **Take `master`'s manifests verbatim rather than `npm audit fix` on `dev`** | `npm audit fix` on `dev` resolves to astro 7.2.2 / postcss 8.5.26 — **past `master`'s** 7.1.3 / 8.5.25 — reversing the divergence instead of ending it |
| **Surgical 3-line edits, not a regeneration** | `npm audit fix` on macOS **stripped the `libc` constraints from all 34 platform-specific entries**; the ubuntu-latest docs build needs them for sharp's libvips. Dependabot's js-yaml diff was Linux-generated, so it was reused |
| **Hand-written integrity hash is safe** | npm's `integrity` is content-addressed — `npm ci` downloads and compares SHA-512, so a wrong hash fails loudly. The *regeneration* is the risky path: it silently rewrites metadata nothing local checks |
| **🔄 REVERSED: keep `dev`'s `docs.yml`, not `master`'s** | Deploy-from-`dev` was deliberate (#1072, 13 Jul) because **Pages serves one "latest"**, so a later `master` push of older docs regresses the site. #1089 (1 Aug, master-only) flipped it, but its title and comment are about adding a **PR build** for master-targeted Dependabot PRs — the deploy flip reads as collateral |
| **Wrote `dependabot.yml` despite it not achieving the stated goal** | Felix asked for `target-branch: dev` to stop bumps landing on `master`. It cannot do that (§5 N25). Delivered with both limits stated, because scheduled version updates on `dev` are what stops the drift recurring |
| **#1099/#1100 get Milestones entries — the Medium/Low split was moot** | The log had a **dated hole**: every other fix of comparable weight has a line, including #1092, also "a flag accepted and ignored". Consistency decided it, not priority |
| **Fixed the journal policy line beyond the literal ask** | It claimed a row update **and** a Milestones line on every module-merge PR *into `master`*. Both halves were wrong |
| **Kept EpiGnome in the mispriming sentence** | Felix's explicit instruction, and the reason is worth preserving: it is a statement about random priming **as a class**, true of a retired kit. #1109's body asks future readers not to "finish the job" on that line |
| **PBAT prose rewritten, not just the table cell** | Felix named this as the most likely half-done outcome. A cell saying 8 with a paragraph saying "it depends on the oligo" is worse than neither change |
| **The ecosystem rationale went in the commit message, not the page** | Felix asked to carry the nf-core/methylseq + TrimGalore#440 reasoning "into the prose", but the same brief forbids referencing the unshipped `--library` flag, and having the upstream source cite a downstream consumer inverts the dependency being protected |
| **Fixed the dead FAQ link in both places** | Only one instance had been noticed. Leaving the identical dead link a page away is the same half-done failure at page scale |
| **Recorded the coverage resolution instead of rewriting the ledger** | The audit's findings were real when taken; four gaps found and closed is the interesting fact. Editing the rows away would have destroyed the record and left no trace that the gaps ever existed |
| **No CHANGELOG or Milestones entry for any docs commit** | Docs-only commits here have never carried one — `fcab091`, `f98bc52`, `a1b2837`, `fb1bc72`, 4 for 4 |
| **No dual code-review agents all session** | The session instruction forbids the Agent tool unless requested, and a lockfile diff / journal entry / docs table is verified better by `npm ci`, the CI build, a rendered-page parse and a live fetch than by a reviewer. Flagged rather than skipped silently |

## 4. Files modified

| File | Change |
|---|---|
| `docs/package-lock.json` | `master`'s copy + js-yaml 4.3.1 + nanoid 3.3.18 (`3a6aab8`) |
| `docs/package.json` | `master`'s copy — astro `^7.1.3`, sharp `^0.35.3` (`3a6aab8`) |
| `.github/dependabot.yml` | **New** — npm `/docs`, `target-branch: dev`, weekly, grouped (`04c3069`) |
| `rust/README.md` | Two 2026-08-11 Milestones entries (#1099, #1100) + policy-line fix (`b502110`) |
| `docs/src/content/docs/usage/library-types.md` | EpiGnome row/section/link removed (`b2e0a12`); PBAT row + prose to a flat 8 bp (`e8ab722`) |
| `docs/src/content/docs/faq/single-cell-pbat.md` | PBAT clip 6→8, dead link, unbalanced paren (`e8ab722`) |
| `docs/src/content/docs/faq/low-mapping.md` | Dead link — the second instance (`e8ab722`) |
| `plans/08112026_1100-five-base-index-validation/COVERAGE.md` | Resolution section, +38/−1, ledger untouched (`2c69a5e`) |
| `SESSION_HANDOFF.md` | `6230532`, `e3d0666`, `1c6697d`, and this refresh |

**Untracked:** 93 entries (`plans/` — note **535 files under `plans/` ARE tracked**, so check before assuming — `.claude/`, `CLAUDE.md`, `.nf-test.log`). Unchanged from session start, none staged.

| Outside the repo | Change |
|---|---|
| GitHub | PRs #1106–#1111 opened and squash-merged; **#1103 closed** with an explanatory comment; all six feature branches deleted both sides |
| `~/.claude/…/memory/` | **New:** `feedback_no_release_mid_flight.md`. **Corrected:** `feedback_git_workflow_dev_master.md` (docs-deploy claim marked violated), `MEMORY.md` index |
| Live docs site | Redeployed from each `dev` push; verified by fetching the page that the retired kit is gone and PBAT reads 8 bp / (8 bp) |

## 5. Gotchas and constraints

### New this session

**N24 — 🔑 `npm audit fix` is a re-resolution, not a patch, and resolution is platform-specific.** On darwin it drops the `libc: ["glibc"|"musl"]` arrays from every platform-specific entry — 34 here. Nothing local fails; the loss only bites on the Linux runner that picks sharp's libvips binaries. **After any `docs/` lockfile change, assert the `libc` count is still 34** and that only the intended versions moved. Prefer a surgical edit, or reuse Dependabot's own Linux-generated diff.

**N25 — Dependabot is two products sharing one name.** *Version updates* are file-driven, schedulable, `target-branch`-aware, and **do not exist without a `dependabot.yml`**. *Security updates* are settings-driven, advisory-triggered, and **hardwired to the default branch — `target-branch` cannot move them**. Also **`dependabot.yml` is read from the default branch only**, so a config on `dev` is inert. Verified against GitHub docs, not assumed.

**N26 — The Dependabot alerts API under-reports; `npm audit` is the per-ref ground truth.** Alerts are curated advisory records attached to the default branch; `npm audit` resolves the tree live. nanoid exists in the second and not the first. **"0 open alerts" ≠ "0 vulnerabilities"**, and on a non-default branch it means nothing — `dev` sat at 11 high / 10 moderate while the API reported 1. Audit the ref you care about: `git show <ref>:docs/package-lock.json` into a scratch dir.

**N27 — `npm audit fix --dry-run --json` is not JSON in npm 11**, and with no `node_modules` a dry run lists the **entire tree** as `add …`, which reads as a wholesale rewrite when the real delta is two lines. **Never infer delta size from a dry run** — diff the actual lockfile.

**N28 — 🔑 The empty-or-broken-check-as-verdict shape struck four more times.** (a) `gh pr merge` prints **nothing** on success. (b) `git push … | tail -6; rc=$?` captured **`tail`'s** status. (c) `git grep -E '\b(6N|9N)\b'` matched nothing because git's ERE has no `\b`, nearly certifying "no contradictions elsewhere" from a pattern that could not match. (d) A link check looked for `dist/Bismark/usage/...` when `dist/` **is** `/Bismark/`, printing a false `TARGET MISSING`. **Every one was caught by a control** — a known-good link, a known-present string, an independent SHA comparison. **Add a positive control to any search whose emptiness you are about to believe.** Also: `gh pr view` has **no `merged` field** — use `state` / `mergedAt`.

**N29 — Any path under `rust/` triggers the full Rust matrix.** A one-line `rust/README.md` edit ran **14 check-runs** plus the pre-push hook's local `fmt`+`clippy`; `.github/dependabot.yml` got 2, a `docs/` change 4, a `plans/` change 2. Neither CI nor the hook distinguishes Markdown from `.rs` — the filter is the path prefix.

**N30 — GitHub validates `.github/dependabot.yml` itself**, as a check-run named after the file. Green means every key is a *recognised* option — better than a local YAML parse, which cannot tell a valid-but-ignored key from a real one.

**N31 — 🔑 A defect described in a brief may not exist, and it was asked for twice.** Two briefs asked me to preserve a malformed one-cell `<tr>` near line 18 of `library-types.md`. It is not there and never was: **555 tracked `.md`/`.mdx`/`.html` files scanned** (per-row cell counts vs each table's modal column count) and `git log --all -S` over all three revisions the file has ever had. The nearest real thing is a `<td colspan="2">` row in a frozen Perl report template. **Verify a described defect before preserving or fixing it, and say so** — silence reads as the instruction being ignored, and inventing a fix is worse.

**N32 — A cell-count linter flags every `colspan` row as irregular.** Cell count and column count are different things; a structural check must read `colspan`/`rowspan` before calling a row malformed.

**N33 — For "remove/keep some of N mentions", anchor on structure, not the term.** #1109's three removals were a `<tr>` block and a `### ` section while the survivor is prose in a different section: anchoring each deletion on the *next* structural element made the survivor unreachable by construction. A find-and-replace on "EpiGnome" would have taken all four. Gate with `git diff --numstat` showing **0 insertions**, which also proves no formatter reflowed a neighbour.

**N34 — A multi-part brief's premise can be stale.** The second library-types brief asked for two edits; the first had already shipped as #1109 earlier the same session. **Check the repo state before executing a multi-part brief** — and note that its instruction to reuse the deleted section's reasoning only worked because that text was still recoverable (`git show b2e0a12^:<path>`).

**N35 — Internal docs links are base-prefixed absolute paths** (`/Bismark/usage/library-types/`), the form ~12 existing links use; `astro.config.mjs` sets `base: '/Bismark/'`. To resolve one against `dist/`, **strip the base** — `dist/` is the site root at `/Bismark/`. A whole-site check is cheap after any link edit; 0 broken internal links is the current state.

**N36 — Process substitution is blocked in the sandbox.** `diff <(cmd) <(cmd)` fails with `/dev/fd/12: Operation not permitted`, and the surrounding test then reads as a failure. Write to `$TMPDIR` files and use `cmp -s`.

**N37 — 🔑 A coverage audit taken on an uncommitted working tree has no stable referent.** #1100's `COVERAGE.md` read `INCOMPLETE` for five days after its gaps were closed, because it audited a tree that no longer existed anywhere; the fixes landed in `7596b0d`. **Audit a commit, or at minimum record the SHA the fixes land in.** When resolving a stale audit, add a Resolution section rather than editing the ledger — the findings were real, and "four gaps were found and closed" is the fact worth keeping.

**N38 — "The test exists" and "the test passes" are different claims.** Closing #1100's gaps meant locating `hisat2_env_fallback_accepted` *and running it*; this plan's own §11 records a committed test that existed and was vacuous. `cargo test` takes only **one** positional filter — a second name is rejected as an unexpected argument, so run them separately.

### Environment

- **`gh` needs `dangerouslyDisableSandbox`** — sandboxed it fails TLS: `x509: OSStatus -26276`. **`git fetch/push` over SSH** likewise: `nc: authentication method negotiation failed`.
- **A scratch command containing `rm -rf` was blocked by the permission classifier.** Rewrite without it rather than retrying.
- **The pre-push hook works as documented** — a docs-only push prints *"no rust/ changes in this push — skipping the Rust gates"* and is instant.
- `python3` f-strings reject a backslash inside the expression part — keep regex literals out of the `{…}`.

### Carried forward (still verbatim-critical)

**N1** — a run's `conclusion: success` ≠ its jobs finished; **enumerate check-runs and assert zero NULL conclusions** (done for all six merges). **N3** — `gh run list --commit` needs the **full 40-char SHA**.

**N11/N12** — an unpushed base rides along in a PR and the PR object lies about it; `files`/`changedFiles` are snapshots refreshed on a **head** push, so moving the **base** never refreshes them (#1103 still advertised 1 file / 3 lines against `dev`). Use `compare/<base>...<head>`, and pass `--subject/--body-file` to `gh pr merge` explicitly.

**N16/N23** — verify a squash by **content**, never ancestry; `git branch -d` refuses a squash-merged branch forever, so `-D` after a content check is the only route. A merged branch can hold the only copy of something.

**G21/G22/G25** — credentialed tools need unsandboxing; `gh api` does not paginate by default; **`timeout` is not installed** (macOS); **`dev` merges close nothing**. **G23** BRANCH FIRST — all six merges went through a branch + PR.

**Style** — one phrase is banned outright (see `feedback_never_say_load_bearing`). **Session `grep` is a ugrep wrapper** — use `command grep` for CI-shell logic. **`plans/` is off-limits unless Felix provides a file or asks** (he provided `08112026_1100…/COVERAGE.md` this session). **Comments/CHANGELOG/PR replies: brevity is the default** (CLAUDE.md — three venues, one piece of feedback).

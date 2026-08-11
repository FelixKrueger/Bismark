# Session Handoff — 2026-08-11b (#1102 merged; `dev` green at `7596b0d`; no release)

**#1100 is merged.** PR [#1102](https://github.com/FelixKrueger/Bismark/pull/1102) squash-merged into `dev` as **`7596b0d`** (13 files, +2732/−8 — content-identical to `ccd5931`, G29 verified). `dev` is **32 ahead of `master`** and **CI-green: 9 jobs, 0 pending, 0 failures**.

> 🚫 **STILL NO RELEASE — and the handoff's stated gate no longer exists.** **PR #1096 is CLOSED**, not open; there are **zero open PRs**. A 3.2.0 cut now needs a *fresh* `dev`→`master` PR plus the version bump — `rust/VERSION` still reads **3.1.0**.
>
> ⚠️ **#1095, #1099 and #1100 all remain OPEN** (G25 — merging to `dev` closes nothing). They close at the release.

> 📄 **The live docs site is now ahead of every released binary.** `docs.yml` deploys on a **push to `dev`** (build-only on PRs, `docs.yml:4-5,53-56`). The merge therefore published `docs/src/content/docs/rust/illumina-5-base.md` describing `--five_base_index` validation that **no released version has**. By design and true of every prior `dev` merge — but say so if a user reports docs/behaviour mismatch on 3.1.0.

## 1. What we accomplished

| Item | Outcome |
|---|---|
| **The stuck `cargo clippy` job (last session's first job)** | **Resolved itself** — now `pass, 30s` (run `31503448311`, job `93818984284`). No rerun was needed; it was a stuck runner, as N1 suspected. Verified per **job**: 16 check-runs on `ccd5931`, **0 null conclusions** |
| **🔑 Found `dev` had never been pushed** (not in the handoff) | Last session committed the handoff to *local* `dev` as `688d919` and branched #1100 from it, but never pushed `dev`. `688d919` reached GitHub **only via the feature branch**, so #1102's diff was 14 files / +2803, not the 13 / +2732 recorded — contradicting §4's own "the branch must stay #1100-only" |
| **Fast-forwarded `origin/dev`** | `1a4f8fa` → `688d919` (docs-only, ancestor-verified). This moved #1102's merge-base, collapsing its live diff to the #1100 change alone — no rebase, no force-push |
| **Squash-merged #1102** | `7596b0d`. Explicit `--subject`/`--body-file` from `ccd5931`'s message (see N13). Verified: `git diff ccd5931 origin/dev` = **0 lines**; squash-vs-parent = **13 files, +2732/−8** — the docs commit stayed separate |
| **`dev` post-merge CI** | **GREEN** at `7596b0d`: 9/9 jobs success (`BismarkCI`, `cargo {clippy,fmt,test}`, both feature builds, `perl-oracle byte-identity`, docs `build`+`deploy`) |
| **Local `dev` restored** | Fast-forwarded to `7596b0d`; this file carried across the branch switch intact (backed up first, per N2) |

## 2. What's still pending

| Item | State |
|---|---|
| **Release 3.2.0** | The only real gate. Needs a **new** `dev`→`master` PR (#1096 is closed) + `rust/VERSION` 3.1.0 → 3.2.0 + the 3 mirror literals + retitling the `Unreleased` CHANGELOG section. **Not started — deliberately deferred** |
| **`Unreleased` carries 8 user-visible groups, not 3** | `legacy_perl/` move · short-option case (#1084) · #1099 · #1100 · `--output_dir` · `--five_base_bisulfite_bam` (#1095) · `--rammap` presets (#1092) · the three MAPQ fixes (#1079/#1080/#1081). **Three of these retire a byte-identity claim** — minimap2 SE is explicitly "no longer byte-identical to Perl v0.25.1 + minimap2", and both `--local` MAPQ paths emit values Perl never produced. The release note's framing of *that* is a judgment call, not a mechanical bump |
| **Dependabot: 1 open HIGH** | `js-yaml < 4.3.1` in `docs/package-lock.json` ([alert 9](https://github.com/FelixKrueger/Bismark/security/dependabot/9)) — quadratic CPU on `!!omap`, fixed in **4.3.1**. Transitive dev dependency of the Astro docs site, so exposure is build-time parsing of *our own* content; a DoS needs attacker-controlled YAML, which the build never sees. **Low practical risk**, fixed by `npm audit fix` in `docs/`. Surfaced by the `git push` banner, not by CI |
| **`1100-five-base-index-validation` branch** | **Still present locally and on the remote** at `ccd5931`. Merged and safe to delete (G28: `git status` first) — left in place on purpose |
| **`COVERAGE.md` in `plans/08112026_1100…/`** | Still reads **INCOMPLETE** though its 4 gaps were closed. Not rewritten. Read PLAN §12 alongside it |
| **#1095 reply** | With @Danielsm8 (latest `5254660916`). If he wants simplex output it is a new issue + plan, not a tweak |
| **Simplex consensus** | Not filed. `mod.rs:2519-2521` skips singleton families; counts already reported per run |
| `rust/README.md` Milestones | Still undecided from #1099/#1100 (reviewers split Medium/Low) |
| Declined #1100 review items | Test for `mod.rs`'s fail-loud `create_dir_all`; absolutising probed paths in the error; a `TempDir`-rooted pathological-basename case. Reasons in PLAN §12 |

## 3. Key decisions (with rationale)

| Decision | Rationale |
|---|---|
| **Push `dev` *before* merging, rather than squashing both commits** | Moving the base forward is a fast-forward of a docs-only commit and makes GitHub recompute the merge-base, so the squash commit maps 1:1 to #1100. The alternative folded last session's handoff into the #1100 commit — harmless in content, but the commit would no longer correspond to the issue |
| **Re-poll the stuck job before rerunning it** | `gh run rerun` was the standing recommendation, but a stuck runner may simply finish. It had — a rerun would have burned CI minutes and produced a second, confusing run on the same SHA |
| **Trust `compare/base...head` over the PR object** | The two disagreed (13 files vs 14). The compare API recomputes the merge-base per request; the PR's `changedFiles`/`commits` are snapshots. See N12 |
| **Supply the squash message explicitly** | `gh` builds the default from the PR's *stale* commit list, which still showed 2 commits — the default body would have carried last session's handoff message |
| **Defer the release rather than prep it** | Felix's call. The retired byte-identity claims need his framing, and the CHANGELOG scope is 8 groups rather than the 3 the previous handoff assumed |

## 4. Files modified

**Committed this session:** none by hand. One merge commit `7596b0d` (GitHub squash of `ccd5931`).

**Uncommitted:** this file only — now on `dev`, which is where it belongs.

| Outside the repo | Change |
|---|---|
| GitHub | `origin/dev` `1a4f8fa`→`688d919`→`7596b0d`; PR #1102 **MERGED**; docs site **re-deployed from `dev`** |
| Remote branch | `1100-five-base-index-validation` still present at `ccd5931` |

## 5. Gotchas and constraints

### New this session

**N11 — 🔑 A pushed feature branch can publish its own unpushed base, and every local check will agree with itself.** `git log dev` showed `688d919`, `git rev-list --count origin/master..origin/dev` returned the expected 30, and the branch was correctly "1 ahead of `dev`" *locally* — all true, all consistent, all wrong about the remote. Only `git branch -r --contains <sha>` (returned the feature branch **and not `origin/dev`**) exposed it. **Before opening or merging a PR, check that its base is actually pushed**, and cross-check the PR's own file count against the commit you think it contains. Same failure shape as N1: an assertion made at the wrong layer.

**N12 — 🔑 Two GitHub read paths give different diffs, and only one is live.** `compare/dev...<sha>` recomputes the merge-base every request (reported `merge_base=688d919, commits=1, files=13`); the PR object's `changedFiles`/`commits` **and `pulls/N/files`** are snapshots refreshed on a *head*-branch push (`synchronize`). Moving the **base** forward fires no such event, so they stayed at 14/2 right up to the merge. The squash itself used the live merge-base and was correct. **Use `compare/` to verify a PR's real scope; treat the PR object's counts as possibly stale.**

**N13 — 🔑 `$TMPDIR` differs between sandboxed and unsandboxed Bash calls.** A squash body written to `$TMPDIR` in a sandboxed call was invisible to the unsandboxed `gh` call (`open /var/folders/…/squash_1102_body.txt: no such file`) — the merge silently did not happen. **Hand files across the sandbox boundary via the fixed scratchpad path, and `[ -s "$f" ]` in the consuming shell before using it.**

**N14 — 🔑 N3 recurred within the same session, on the merge itself.** `gh pr merge … | tail -10; echo "exit=$?"` printed `exit=0` — that was **`tail`'s** status while `gh` had failed. Had I not checked `gh pr view 1102`, I would have reported a merge that never happened. **`$?` after a pipe is the LAST stage's. Use `if gh …; then`** — as N3 already said, which is exactly why it is worth repeating.

**N15 — Pushing `dev` publishes the docs site.** `docs.yml` deploys on push to `dev`, build-only on PRs. A `deploy` job that is `skipped` on a PR run and `success` on the `dev` push is normal, not an anomaly — but it means docs land publicly at `dev`-merge time, ahead of any release.

### Carried forward (verbatim-critical)

**N1** — 🔑 A run's `conclusion: success` does NOT mean its jobs finished. **Enumerate jobs AND assert every job has a non-null conclusion** (`commits/<sha>/check-runs`, count `select(.conclusion==null)`). A poller keying on run status reports GREEN over a pending job.

**N2** — 🔑 `git checkout -- <file>` DESTROYS uncommitted work. Back up with `command cp -f` to a scratch path and restore from that. **`cp` is aliased to `cp -i`**, so a plain `cp` restore prompts and hangs.

**N3** — 🔑 Silence is not a result. Assert on the command's own exit code (`if cargo …; then`), never on a pipeline's last stage; pair a sabotage with a control run. (N14 is a fresh instance.)

**N4** — `$f="--features x"` does not word-split in this shell; use literal arguments. **N5** — anchor edits on doc comments/attributes, never on the signature (`missing_docs` is off for aligner/genome_prep/report, `lib.rs:8-12`).

**N6** — 🔑 The two-layer index resolution: bowtie2/hisat2 resolve a basename in the **wrapper** (`glob(<basename>*.bt2{,l})`, then `$…_INDEXES`) and again in the **binary** (`adjustEbwtBase`: `<basename>.1.<ext>`, then `$…_INDEXES`). **The binary decides.** hisat2's wrapper glob is malformed but `gfm.cpp` does bowtie2's lookup, so `$HISAT2_INDEXES` works.

**N7/N8** — a vacuous test needs a **relative** basename to observe the env gate ⇒ cwd control ⇒ child process only. `resolve`-level tests cannot assert `Ok` (CI installs minimap2 + samtools only); use `if let Err(e) = resolve(…)` + `!matches!(e, <variant>)`, with real (empty) read files since `check_exists` runs first.

**N9** — never put a fixed scratch path in a plan other agents will read; use `$TMPDIR/<name>_$$`. **N10** — when adding a check to the 5-Base area, ask what silently-wrong input it still admits.

**G13/G19/G20** — vacuous verification; a check whose failure you have never observed is not yet a check; verify contested reviewer claims at source.

**G21/G22** — `gh` / `git push` / `curl` need `dangerouslyDisableSandbox`; `gh api` does not paginate by default; quote URLs with `?`. The first `--features binseq-input` cargo run in a session may die `Operation not permitted`; retry unsandboxed once.

**G23** — BRANCH FIRST. **G25** — merging to `dev` closes no issues. **G28/G29** — `git status` before deleting a branch; verify a squash by `git diff <tip> origin/dev` = 0 lines + PR state, never ancestry.

**H4** — mirror any tool-availability guard into BOTH feature jobs. **`gh run list --commit` needs the FULL 40-char SHA.** **Session `grep` is a ugrep wrapper** — use `command grep`. **`plans/` is off-limits** unless Felix provides a file or asks.

**Style** — one phrase is banned outright (Felix, 2026-08-11); the memory `feedback_never_say_load_bearing` names it. Say what the thing does, or name the consequence, instead.

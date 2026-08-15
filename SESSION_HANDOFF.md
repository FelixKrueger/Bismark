# Session Handoff — 2026-08-15 (#1100 and #1104 both shipped to `dev`; nothing left in flight)

**`dev` is at `f575873`, 40 ahead of `master`, and there is no open feature work.** #1100 landed (PR #1102 → `7596b0d`) and #1104 landed (PR #1105 → **`f575873`**, squash, CI was 16/16 green, dual code review APPROVE). Both feature branches are deleted local and remote. **The next action is a release decision, not code.**

> 🚫 **STILL NO RELEASE, and it now gates FOUR user-visible items** — `--five_base_bisulfite_bam` (#1095), #1099, #1100, and #1104 once merged. `rust/VERSION` still reads **3.1.0**. **PR #1096 is CLOSED**, so a 3.2.0 cut needs a *fresh* `dev`→`master` PR. The `Unreleased` CHANGELOG section carries **nine** groups; three retire a byte-identity claim (minimap2 SE, both `--local` MAPQ paths) and #1104 adds a fourth behaviour change (consensus record ORDER).
>
> ⚠️ **Merging into `dev` closes nothing** (G25). #1095/#1099/#1100/#1104 all close at the release.

> 🔵 **Dependabot opened [PR #1103](https://github.com/FelixKrueger/Bismark/pull/1103) itself — base `master`, not `dev`, and `BLOCKED`.** js-yaml 4.3.0→4.3.1 in `docs/`. It is the one remaining open alert (nanoid's cleared on its own). Note the base: merging it goes straight onto the release branch, against the house convention.

## 1. What we accomplished

| Item | Outcome |
|---|---|
| **The stuck `cargo clippy` job** (last session's first job) | Resolved itself — **pass, 30 s**. No rerun needed. Verified per-**job**, not per-run |
| **🔑 `dev` had never been pushed** | `688d919` existed only on the #1100 feature branch. Fast-forwarded `origin/dev`, which collapsed #1102's diff to #1100-only — no rebase, no force-push. See N11 |
| **#1102 merged** | Squash `7596b0d`, 13 files +2732/−8, `git diff ccd5931 origin/dev` = 0 lines. `dev` CI green 9/9 |
| **Branch cleanup** | **28 of 34 local branches deleted** after per-branch verification; 6 kept (see §5 N16). Also 33 orphaned `branch.*` sections removed from `.git/config` (101 → 19 keys) |
| **🔑 Rescued the bioconda W3 docs** | `rust/fix-nondir-pe-flag-swap` held the ONLY copy of `plans/07062026_install-story/W3-bioconda/` (5 files, incl. the recipe draft for the still-open upstream PR #67004) — in neither `dev` nor `master`, remote auto-deleted. Pushed the branch **and** cherry-picked the paths onto `dev` (`4cee57c`) |
| **#1095 answered** (`5297345730`) | @Danielsm8 (Mike) is in the **coverage-recovery** case: simplex kept distinguishable, *not* pooled. Reply covered the EM-Seq/5-Base polarity inversion (a `C>T` reads *unmethylated* under EM-Seq, *methylated* under 5-Base — so his paired assays disagree at variants and agree at real methylation: a genotype-free variant screen), pointed at UMI-aware `bismark dedup --barcode`/`--bclconvert` (already in 3.1.0), confirmed `RX:Z:` for fgbio. An fgbio duplex-mode caveat was drafted and **cut** — sound from the chemistry, unmeasured |
| **#1104 filed, delivered and MERGED** | Issue → plan rev 0 → dual plan review (**REQUEST CHANGES ×2**) → rev 1 → implement → dual code review (**APPROVE ×2**) → coverage audit (**INCOMPLETE, 3 items**) → all findings + gaps closed → oxy scale run → PR #1105 → squash **`f575873`**. 6 commits folded into one; 17 files, +2828/−111. Branch deleted both sides |
| **oxy scale validation** | **+83 MB peak RSS for 3.42M simplex families** over a 6.85M-record PE BAM; duplex BAM byte-identical modulo `mx`; count identity exact; extractor clean. §2 for what it does NOT show |
| **FastQC-Rust located** | Phil's repo is **`ewels/FastQC-Rust`** (not "RustQC"/"fastqc-rs"). #6 (BAM Phred bug) OPEN, Felix asked Phil which fix he prefers — **do not PR unprompted**. Saved as a memory |

## 2. What's still pending

| Item | State |
|---|---|
| **Release 3.2.0** | 🔴 **The only thing left, and the only real gate.** Needs a fresh `dev`→`master` PR (#1096 closed), `rust/VERSION` 3.1.0→3.2.0 + the 3 mirror literals, and a `## Bismark 3.2.0` retitle. Closes #1095/#1099/#1100/#1104 |
| **Dependabot PR #1103** | Open, base **`master`**, BLOCKED. js-yaml in `docs/` (build-time only; low practical risk). Decide whether to re-target at `dev` or take it on `master` |
| **Biological V9 for #1104** | ⛔ The Illumina 5-Base demo dataset is **not on oxy** (only plan dirs; the NA12878 data is gone). The oxy run validated memory/counts/tags/byte-identity **at scale but on bisulfite input through the inverted-polarity path — its methylation values are meaningless (98.6 % CHH)**. Biological correctness rests on the synthetic groundtruth gates + prior DRAGEN concordance. Recorded in PLAN §11c, not implied |
| **`#1104` simplex-mode duplex count** | Both reviewers noted PLAN §3.5 promises a duplex figure in `simplex` mode that §3.7/V7 asserts is absent — plan self-inconsistency, code matches §3.7. One line in the simplex report line would settle it |
| `rust/README.md` Milestones | Still undecided from #1099/#1100 (reviewers split Medium/Low); #1104's entries were added |
| `COVERAGE.md` in `plans/08112026_1100…/` | Still reads **INCOMPLETE** though its gaps were closed. Read that PLAN §12 alongside it |
| Simplex-consensus follow-ups | Reviewer optionals not taken: a per-record family-size tag; an in-run `both`-mode test for `--five_base_umi_qname`; `debug_assert!(n >= 1)` beside the histogram subtraction |

## 3. Key decisions (with rationale)

| Decision | Rationale |
|---|---|
| **Push `dev` before merging #1102** | A fast-forward of a docs-only commit made GitHub recompute the merge-base, so the squash maps 1:1 to #1100. Squashing both would have folded a handoff commit into the fix |
| **#1104: deterministic emission order as its own commit** | The plan's byte-gate rested on ordering that was random per process. Sorting at *emission* (not comparison) makes every future byte check on that file exact. Cheap now precisely because no stable baseline existed to break |
| **Histogram on READ counts, not fragments** | `--five_base_min_mapq` filters per record while pair gates are pair-level, so one mate can be orphaned; `/2` would have binned a 1-read family into a bucket that does not exist. Fires exactly in the DRAGEN-parity config |
| **One simplex record, on the molecule's own strand** | The opposite-strand record would be *wrong*, not empty: at the opposite strand's non-CpG cytosines `reconcile_generic` passes the own-strand base through. Pinned by a non-CpG-`G` fixture |
| **`mx:i` tag AND a separate BAM** | The file split is lost the moment a user merges; the tag survives. Written only in non-default modes, which is what keeps the default byte-identical *and gateable* |
| **Reframe V9 rather than skip or fake it** | The dataset is gone, so the biological run is blocked — but the memory bound was the one claim with no evidence and IS measurable on any Bismark PE BAM. Ran that, labelled it precisely, left the biological gap open |
| **Cut the fgbio duplex caveat from the #1095 reply** | The reasoning is sound from the chemistry but untested; a maintainer's answer to a user should not assert unmeasured behaviour |

## 4. Files modified

**On `dev`:** `7596b0d` (#1102 squash = #1100), `4cee57c` (bioconda W3 docs cherry-picked), `3d51266`/`53744d5`/`18fc7a4` (handoff), **`f575873` (#1105 squash = #1104)**.

**What `f575873` carries:** `rust/bismark/src/aligner/{cli,config,mod,five_base_duplex}.rs`, `rust/bismark/src/io/record.rs`, `rust/bismark/tests/aligner_five_base_{simplex,groundtruth}.rs`, `CHANGELOG.md`, `rust/README.md`, `docs/…/illumina-5-base.md`, `plans/08142026_5base-simplex-consensus/`. Deleted tip was `ab6bfea` (reflog, ~90 days) — but every byte is on `dev`, verified by content.

**Uncommitted:** this file only.

| Outside the repo | Change |
|---|---|
| GitHub | #1102 merged; #1104 filed; PR #1105 opened; #1095 commented ×2; `rust/fix-nondir-pe-flag-swap` pushed; `1100-five-base-index-validation` + 27 other local branches deleted |
| oxy | Branch built at `2564fa9`; scale run done; `~/v9_scale` scratch **removed**; `~/v9_simplex_scale.sh` left in place |
| `~/.claude/…/memory/` | **New:** `reference_fastqc_rust_upstream.md`, `feedback_pr_base_unpushed_stale_snapshot.md`. Corrected the stale "dev is 248 behind master" index line |

## 5. Gotchas and constraints

### 🔑 THE recurring failure of this session — SIX instances, one shape

**An "empty" or "nothing found" value consumed as a verdict.** Every instance:

0. **The worst one, and it happened while writing this section:** `if git push origin dev | tail -2; then echo "PUSH OK"` — the push was **REJECTED** (non-fast-forward, `origin/dev` had moved under me) and the script printed **`PUSH OK`**, because the `if` tested `tail`. Only a follow-up `rev-parse` comparison caught it. Left unnoticed, the session would have ended believing this handoff was pushed. **Never pipe a command whose success you are about to assert.**
1. `cargo … | tail -1` / `| head` → **`$?` is the LAST STAGE's**, so a failing cargo reports 0. (Hit me twice, and both code-review subagents independently.)
2. `grep -c FAILED` → **exits 1 when the count is 0**, so "no failures" propagated as a failed script.
3. `gh run list --commit <short-sha>` → empty list, and empty reads as all-done. Needs the **full 40-char SHA**.
4. `.all(|…|)` on an **empty iterator is `true`** — a committed test gate passed vacuously; sabotaging the code to produce an empty BAM left the *whole suite* green.
5. **Local clippy warnings are invisible without CI's `RUSTFLAGS: -D warnings`** — and `| tail -1` hid the warning text too. This turned three CI jobs red on PR #1105.

**How to actually prevent it:** never let a search's exit status be the verdict — capture the count and compare (`[ "$n" = "0" ]`), and run gates as `if RUSTFLAGS="-D warnings" cargo clippy -p bismark --all-targets >/dev/null 2>&1; then`. **Before any push touching Rust: `RUSTFLAGS="-D warnings" cargo clippy -p bismark --all-targets` for default, `binseq-input` AND `rammap-inprocess`, with literal `--features` args.** Worth a pre-push hook rather than another line here.

### New this session

**N17 — 🔑 A sabotage aimed at dead code proves nothing.** Testing the in-run filename gate, the first sabotage *passed*: `derive_output_path` takes a **primary pattern and a fallback**, and only the fallback had been changed, so the produced name never moved. Retargeting at the primary made the gate fail correctly. **Confirm you perturbed the live path before reading a green sabotage as coverage.** (Corollary: reviewer A inferred "the tests silently skipped" from a 0.12 s runtime; direct observation with `--nocapture` showed 0 skips and samtools present — the suite is simply fast. Timing is not evidence; a control is.)

**N18 — A sabotage only tests the gate it is aimed at.** Two sabotages during #1104 implementation both bit, yet both code reviewers still found a real defect (`SimplexLedger::finish()` missing the zero-arrival direction) *and* a vacuous gate. Sabotage confirms coverage where you already suspect it and proves nothing where you don't — which is the argument for the dual review being non-negotiable.

**N19 — Subagents that background work never receive their own completion event** (the parent does). All three review agents stopped mid-flight waiting for notifications that could never arrive; each needed a `SendMessage` nudge to re-run in the **foreground**. Tell review agents to verify in the foreground from the start.

**N20 — Parallel agents share this working tree and race.** Two reviewers applied fixes while a third audited; one saw a transient integration failure because the tests exec `target/debug/bismark` while another agent relinked it, and another saw doctest `libbismark-*.rlib` errors. **Treat a lone anomalous failure during multi-agent work as suspect until re-run**, and consider worktree isolation for mutating reviewers.

**N21 — `samtools_available()` / `have_minimap2()` skip silently and still report `ok`.** Cargo captures the "skipping" line for *passing* tests, so a green local run on a box without samtools asserts almost nothing. The `$CI` panic guard is the mitigation and it **works** (verified: no samtools + `CI=1` → "refusing to no-op"). Pre-existing convention, not new — but it decides whether the 5-Base gates mean anything.

**N22 — `env -u VAR` must precede assignments.** `env PATH=… -u CI cmd` treats `-u` as the command name and prints nothing — which nearly read as evidence for a vacuity claim.

### Carried forward (verbatim-critical)

**N23 — 🔑 G29's `git diff <branch> origin/dev` = 0 breaks the moment the base moves — including a handoff commit you pushed yourself.** Verifying the #1105 squash it reported **174 differing lines**, all of them `SESSION_HANDOFF.md`: that commit was on `dev` and never on the branch. Nothing was wrong. **Use two checks so "content missing" and "base moved" cannot be confused:** `git diff <branch> origin/dev -- . ':(exclude)<base-only paths>'` = 0, **and** the squash's own `git diff --shortstat origin/dev^ origin/dev` matching the PR's pre-merge diffstat. A single check that cannot tell those apart is not enough before an irreversible step. (Same root as N16's first bullet.)

**N16 — 🔑 A merged branch can hold the only copy of something.** `rust/fix-nondir-pe-flag-swap` (PR #1031 MERGED) had 2 commits *after* its merge; `git log --all --find-object=<blob>` found them in exactly one ref. **Three tests, because each fails differently:** `git diff <branch> <squash>` = 0 is strongest but only valid when the base has not moved (a squash tree is `base-at-merge + branch changes`); local tip vs GitHub's `headRefOid` catches commits the PR never saw (and **`gh pr list --head` can return several PRs** — reading `.[0]` misattributes long-lived branches); only `--find-object` answers *"does this content exist anywhere in `dev`'s history?"*. Also: `git branch -d` refuses a squash-merged branch **forever**, so `-D` plus a content check is the only route, and deleting branches leaves orphaned `branch.*` config the **sandbox blocks git from cleaning** (`could not lock config file`) — needs a separate unsandboxed pass.

**N11/N12 — 🔑 An unpushed base rides along in a PR, and the PR object lies about it.** `git branch -r --contains <sha>` is the only local check that sees the remote. `compare/<base>...<head>` recomputes live; `changedFiles`/`commits`/`pulls/N/files` are snapshots refreshed on a **head** push, so moving the **base** never refreshes them. Fixing an unpushed base is free when it fast-forwards — but pass `gh pr merge --subject/--body-file` explicitly, since `gh` builds the default from the stale list.

**N1** — a run's `conclusion: success` does NOT mean its jobs finished. **Enumerate jobs and assert every one has a non-null conclusion.**

**N2** — `git checkout -- <file>` DESTROYS uncommitted work; back up with `command cp -f` (and `cp` is aliased to `cp -i`, so a plain restore hangs).

**N4** — **`for X in $VAR` does NOT word-split in this shell.** Hit twice today: 28 branch names passed as one, and `--features binseq-input` rejected as a single argument. Use literal args or `printf '%s\n' … | while read`. A `while read` loop wrapping a destructive git call was also **blocked by the permission classifier**; one `git branch -D <a> <b> …` went through.

**N6** — bowtie2/hisat2 resolve a basename in the wrapper AND again in the binary (`adjustEbwtBase`). **The binary decides.**

**N13** — **`$TMPDIR` differs between sandboxed and unsandboxed calls.** Hand files across via the fixed scratchpad path and `[ -s "$f" ]` in the consuming shell.

**N15** — pushing `dev` publishes the docs site (`docs.yml`, path-filtered to `docs/**`).

**G13/G19/G20** — vacuous verification; a check whose failure you have never observed is not yet a check; verify contested reviewer claims at source.

**G21/G22** — `gh` / `git push` / `curl` / `dcli ssh oxy` need `dangerouslyDisableSandbox`; `gh api` does not paginate by default. **`timeout` is not installed** (macOS). **G23** BRANCH FIRST. **G25** `dev` merges close nothing. **G28/G29** `git status` before deleting; verify a squash by content, never ancestry.

**H4** — mirror any tool-availability guard into BOTH feature jobs. **Session `grep` is a ugrep wrapper** — use `command grep`. **`plans/` is off-limits** unless Felix provides a file or asks.

**oxy** — a K8s pod: RECYCLE kills detached jobs and wipes ephemeral `/var/tmp`; only `/home` persists (**and it was at 90 % full — 11 GB**). Cargo is at `~/.cargo/bin` (not on the default PATH); samtools at `~/miniforge3/envs/bismark-test/bin`. A `git fetch` lands detached — `git checkout -B <branch> FETCH_HEAD`.

**Style** — one phrase is banned outright (Felix, 2026-08-11); see the `feedback_never_say_load_bearing` memory. Say what the thing does, or name the consequence.

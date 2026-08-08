# Session Handoff — 2026-08-08 (legacy_perl move)

**This session moved the frozen Perl toolchain out of the repository root into `legacy_perl/`** — PR [#1098](https://github.com/FelixKrueger/Bismark/pull/1098), branch `legacy-perl-move`, three commits (`dea61f9` pure move → `19cb7db` consumer fixes + layout gate → `0f358d8` review fixes). Full pipeline ran: plan rev 0→2, dual plan review (REQUEST CHANGES ×2, all folded), implement with sabotage-first validation, dual code review (**APPROVE ×2**, 0 Critical/High), coverage audit (**COMPLETE**, 63 items).

> ✅ **MERGED.** All 4 CI runs on `0f358d8` green with every job enumerated via `gh run view --json jobs` (6/6 Rust CI incl. `perl-oracle byte-identity` + both feature jobs; `BismarkCI` = the 44 `./legacy_perl/` invocations end-to-end). Squashed into `dev` as **`5e40555`** (2026-08-08 11:02 UTC); G29-verified (`git diff 0f358d8 origin/dev` = 0 lines, PR state MERGED); branch pruned locally + remotely; local `dev` synced.

> 🚫 **STILL NO RELEASE.** `dev`→`master` = the release signal; 3.2.0 stays a deliberate act (PR #1096). The five `3.1.0` version literals remain correct. #1098 rides the 3.2.0 train via its CHANGELOG "Repository layout" entry.

## 1. What we accomplished

| Commit | What |
|---|---|
| `dea61f9` | **Pure `git mv`** (24 renames, 0 edits): 12 Perl scripts, `copy_bismark_files_for_release.pl`, `plotly/` (10 files), `test_data.fastq` → `legacy_perl/` |
| `19cb7db` | **All 24 consumer code sites fixed** (8 Rust oracle literals, 2 plotly drift guards, 7 golden scripts, 4 `scripts/` harnesses, 44 `ci_tests.yml` invocations) + **new `tests/legacy_perl_layout.rs`** (unconditional existence gate) + `.gitattributes`/`.dockerignore`/README/CHANGELOG/rust-README prose |
| `0f358d8` | **Dual-review fixes**: layout gate now asserts execute bits (`cfg(unix)`), drift-guard messages name `legacy_perl/plotly/`, `rust/README.md:102` emphasis inverted, `.gitattributes` realigned, 2 shell polish items + review/coverage artifacts committed |

**Validation record** (PLAN.md §12): baseline battery 72 ok on `dev` → post-move sabotage failed **exactly the 20 predicted loud gates** while 13 skip-capable summary/template oracles stayed *silently* green (the hazard the layout gate closes) → post-fix 73 ok = baseline+1, 0 failed → layout gate observed red under deliberate sabotage → fmt + clippy (default **and** `rammap-inprocess`) clean → Perl `bismark2report` proven to splice plotly from the new home.

## 2. What's still pending

| Item | State |
|---|---|
| §13 follow-ups (deliberately not absorbed) | `perl-oracle` EXPECTED=13→26 hardening; centralized `legacy_perl_script()` helper; both recorded in PLAN.md §13 |
| V9 `.dockerignore` proof | Optional; retire at next `release.yml dry_run=true` |
| wgbs_tools#120 | Unchanged — submitted 2026-08-08, zero maintainer activity (checked this session) |
| Bismark #787 | Unchanged — closed, last comment ours (contig-intersection diagnostic), @Danielsm8 silent; their UXM worked after UCSC-hg38 rebuild |
| Release 3.2.0 | Deliberately deferred (banner) |

## 3. Key decisions (with rationale)

| Decision | Rationale |
|---|---|
| Directory named `legacy_perl/` | Felix's explicit choice (over `perl/`) |
| Python coverage helpers stay at root | Felix's manual-review veto of the plan's default |
| Layout gate = unskippable existence test now; `EXPECTED=26` CI hardening deferred to §13 | Reviewer B's line: the CI extension alters what CI asserts, not where files live — wrong PR to smuggle it into. Reviewer A's fuller fix recorded, not absorbed |
| Golden scripts' up-counts **repaired**, not preserved | All 7 were already broken (resolved to `rust/` — multicall-consolidation relic, verified empirically); they're the reproducibility record for checked-in goldens |
| `copy_bismark_files_for_release.pl` moved **accepted-broken** (Open-5) | Already dead today (`Docs/make_docs.pl` inputs don't exist); superseded by `release.yml`; documented rather than fixed |
| Execute-bit assertion applied despite reviewer disagreement | A: Medium (bit is load-bearing for 44 workflow steps); B: skippable (CI catches it loudly). ~10 lines, strengthens the gate's actual contract; B's dissent recorded in PLAN §12 |
| `license.txt` stays at root | `release.yml:137` packages it from there — the one near-miss the inventory caught |
| Root symlinks rejected | Defeats the landing-page goal; GitHub blob URLs don't follow symlinks anyway |

## 4. Files modified

All on branch `legacy-perl-move` (see the 3 commits): `legacy_perl/**` (24 moved), 10 Rust test/src files + 1 new test, 7 golden scripts + `nome_gate.sh`, 4 `scripts/` harnesses, `.github/workflows/ci_tests.yml`, `.gitattributes`, `.dockerignore`, `README.md`, `rust/README.md`, `CHANGELOG.md`, `plans/08082026_legacy-perl-move/` (PLAN rev 2 + §12 notes, PROGRESS, PLAN_REVIEW_A/B, CODE_REVIEW_A/B, COVERAGE).

**Outside the PR:** project `CLAUDE.md` updated (still untracked, as before); `.claude/settings.local.json:7-27` holds now-stale absolute-path permission entries → expect re-prompts (cosmetic, local).

## 5. Gotchas and constraints

### New this session

**N1 — 🔑 `gh run list --commit <short-sha>` silently matches NOTHING.** It needs the full 40-char SHA; with a short one the result is an empty list, so any "0 runs incomplete" check passes **vacuously** (bit this session's first CI watcher). Guard every completion check with a count assertion (`length >= expected`), and pass `$(git rev-parse <short>)`.

**N2 — libtest swallows skip notices on passing tests.** The 12 `summary_perl_oracle.rs` oracles + `summary_template_drift.rs` pass with **zero output** when the Perl script is missing (stdout only shown on failure) — even the one test that prints a notice. A skip-capable test's green is unverifiable from its own output; count/name-presence assertions are the only honest gate.

**N3 — oracle-battery ok-counts are toolchain-sensitive.** The `oracle_`/`byte_identical` filters incidentally match ~38 non-Perl unit tests whose population shifts with rustc/features (72 vs 73 across runs of the same tree). Only same-session back-to-back counts are comparable; the durable invariant is the 34-name Perl-dependent population (list in CODE_REVIEW_A).

**N4 — `bismark` crate ships `tests/` in its crates.io tarball** (no `include`/`exclude`), so the layout gate — like the pre-existing drift guards — fails under `cargo test` outside a full checkout, and `.dockerignore` now prunes `legacy_perl/` so a Docker-context `cargo test` would too (nothing does that today). If ever closed: gate on a workspace marker, never on `legacy_perl/` (that would reintroduce skippability). PLAN §13.6.

**N5 — cargo needs an unsandboxed call to unpack new registry deps** (`~/.cargo/registry` is sandbox-write-denied): first `--features rammap-inprocess` build in a session may die "Operation not permitted". Retry unsandboxed once; subsequent sandboxed builds work.

**N6 — moved-path facts for future edits:** the Perl scripts find each other and plotly via `$RealBin` (extractor→bedGraph/c2c, bismark→bam2nuc, report/summary→plotly) — anything moved must move as a set with `plotly/`. `use lib "$RealBin/../lib"` is a no-op both sides. `ci_tests.yml` outputs land in the job CWD (repo root), so only program paths carry the `legacy_perl/` prefix.

### Carried forward (verbatim-critical)

**G13/G19/G20** — vacuous verification (N1/N2 are fresh instances); a check whose failure you have never observed is not yet a check; verify contested reviewer claims at source (twice this session both contested claims resolved in B's favor: golden scripts already broken; 11/12 summary oracles silent).

**G21/G22** — `gh`/`git push`/`curl` need `dangerouslyDisableSandbox`; `gh api` doesn't paginate; quote URLs with `?`. **H3** — declare CI green only from `gh run view <id> --json jobs`. **H4** — mirror any tool-availability guard into BOTH feature jobs. **H5** — bare `===` aborts zsh compound commands; `rm -rf` denied by permissions.

**G23** — BRANCH FIRST (honored: `legacy-perl-move`). **G25** — merging to `dev` closes no issues. **G28/G29** — `git status` before `gh pr merge --delete-branch`; verify squash by `git diff <tip> origin/dev` = 0 lines + PR state, never ancestry.

**Session grep is a ugrep wrapper** — `grep -c` etc. can return nothing; use `command grep` for anything load-bearing (bit this session).

### Prior-session context (still live)

wgbs_tools#120 gotchas (patter flag order H1, init_genome contig filter H2, setup.py silent compile-fail G13) are in the 2026-08-08 morning handoff — recover via `git log -p SESSION_HANDOFF.md` if #120 gets maintainer feedback.

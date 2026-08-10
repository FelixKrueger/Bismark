# Session Handoff — 2026-08-10 (#1099 shipped; #1091 parked; splitter measured)

**`dev` is at `1a4f8fa`, 30 ahead of `master`, CI fully green (14/14 jobs enumerated).** One commit landed this session — the #1099 fix, via PR [#1101](https://github.com/FelixKrueger/Bismark/pull/1101) squash-merged. The rest of the session was evaluation and support work: a third parallel-gzip-decode attempt parked and closed, the `--parallel` splitter measured and deliberately left alone, and a user-reported 5-Base failure turned out to be a real bug.

> 🚫 **STILL NO RELEASE.** `dev`→`master` = the release signal; 3.2.0 stays a deliberate act (PR #1096). It now carries **two** things a user is waiting on: `--five_base_bisulfite_bam` (#1095) and the #1099 fix. The five `3.1.0` literals remain correct.

> ⚠️ **Both #1095 and #1099 are still OPEN** — merging to `dev` closes nothing (G25). They close on the release.

## 1. What we accomplished

| Item | Outcome |
|---|---|
| **#1099 — `--illumina_5base` required indexes it never opens** | Diagnosed from a user report, filed, fixed, reviewed, merged as **`1a4f8fa`**. Full pipeline: plan rev 0→1, dual plan review (**REQUEST CHANGES ×2**), implement, dual code review (**A APPROVE / B APPROVE WITH CHANGES**, 0 correctness defects), coverage audit (**COMPLETE**, 33 items, 0 gaps), agreed fixes applied, CI 14/14 |
| **#1095 — closed out** | Confirmed nothing left to do; posted usage guidance to @Danielsm8 ([5225856369](https://github.com/FelixKrueger/Bismark/issues/1095#issuecomment-5225856369)), then the bug diagnosis + workaround ([5244801261](https://github.com/FelixKrueger/Bismark/issues/1095#issuecomment-5244801261)) |
| **PR #1091 (rapidgzip-core) — parked and closed** | Third attempt at parallel gzip input decode (after #1004, #1007, #1034). Data comment [5225986467](https://github.com/FelixKrueger/Bismark/pull/1091#issuecomment-5225986467), then closed; fork branch `feat/rapidgzip-input` @ `c075877cfb88` preserved, so it reopens cleanly |
| **`count_effective` double-inflate investigated** | The follow-up lead from the #1091 review. Removable, but measured **worse** than the PR just parked. Left alone |
| **[#1100](https://github.com/FelixKrueger/Bismark/issues/1100) filed** | The `--five_base_index` early-validation work deliberately cut from #1099, carrying all three implementation hazards the reviewers found |
| **rapidgzip upstream context** | Read COMBINE-lab/rapidgzip-rust#6 — Rob Patro's answer closes Felix's own "capped at 16?" question (see N3) |

**#1099 in one paragraph:** 5-Base aligns to a plain unconverted index (`--five_base_index`) or, under minimap2, the genome FASTA — never the CT/GA indexes — but `config::resolve` validated them unconditionally. The fix is an additive `discover_genome_fasta_only` sibling plus a `discover_genome_for_run` branch; `discover_genome` is behaviourally untouched, so `perl-oracle` cannot regress. Ride-alongs: the 5-Base summary stopped printing non-existent index paths, `run_five_base_consensus_standalone`'s third copy of the prologue was folded in, a PE desync error stopped hard-coding `"minimap2"`, and `--genome`'s help text stopped telling 5-Base users to prepare the genome.

## 2. What's still pending

| Item | State |
|---|---|
| **Release 3.2.0** | The only real gate. Closes #1095 **and** #1099 |
| **`rust/README.md` Milestones bullet** | 🔸 **Open decision, recorded not dropped** (merged plan §11). Reviewer B: Medium (established for `16f65f6`, `11efbab`, `ddc7633`); Reviewer A: Low; the README's own rule binds *module-merge PRs into `master`*, which #1101 was not. One line either way |
| **Note to @Danielsm8 on #1095** | Worth sending post-release: the placeholder-index workaround becomes unnecessary and the help text no longer misleads. **Nothing is owed now** — our `5244801261` is the last comment. Their stated plan (`5226631117`) is to compare two routes: (1) Bismark → *patched* `wgbs_tools` → UXM, (2) Bismark converter → *stock* `wgbs_tools` → UXM. Route 2 is unblocked on `dev`; route 1 still waits on nloyfer merging wgbs_tools#120. Their "same ref I use for EM-Seq" is what tripped #1099 |
| #1100 | Open. Do **not** implement a naive presence check — see N12 |
| wgbs_tools#120 | Submitted 2026-08-08, still zero maintainer activity |
| #787 | Closed; @Danielsm8 may re-engage via #1095 |
| `SESSION_HANDOFF.md` | This file, uncommitted (as it was at session start) |
| Declined #1099 review items | Tests for the D4 consensus path and Task 5's desync message; narrowing the resolve-level test to a variant allow-list. Reasons in merged plan §11 |

## 3. Key decisions (with rationale)

| Decision | Rationale |
|---|---|
| **Park + close #1091** | Measured **~0.1 % of wall clock**. Third run at the same target; crate had 133 total downloads and the PR pinned `=0.1.0`, which contains a since-fixed deadlock. Closed keeping the fork branch, per #1034's precedent |
| **Leave the `count_effective` double-inflate alone too** | Removable (byte-identity does not depend on chunk boundaries — N5), but worth **~12 s** on a 638-min sample: *less* than the PR just parked, because it eliminates 2 of 4 PE inflates while rapidgzip sped up all 4. Deleting code beats adding a dependency — but not on a byte-frozen path for 12 s |
| **#1099: additive sibling, not `Option<PathBuf>`** | Four of the five readers sit on **byte-frozen** alignment paths, where the refactor means `expect()` panics or new `Result` plumbing through code whose value is being unchanged. Both reviewers agreed; the truthful shape (a `GenomeRefs` enum) is a separate PR |
| **Branch `summary()` on `five_base` (D2)** | Reviewer A found `config.rs:1609` is `RunConfig::summary()` printed unconditionally, **not** a `Debug` impl — so the fabricated basenames *were* user-visible. ~6 lines satisfied the issue's "should not be silently fabricated" without the field refactor |
| **Repoint 9 existing test call sites instead of adding two (D3)** | They wrote dummy index files *solely* to pass the old check (`// CT/GA .bt2 (for discovery)`). Repointing yields both-engine end-to-end gates and **deletes** scaffolding. Documented as a deviation from plan §5d.9 |
| **Cut D1 → #1100** | A bare `--five_base_index` basename can resolve via `BOWTIE2_INDEXES`, so a naive check would introduce a fresh spurious rejection while fixing one. Wrong thing to bundle |
| **Reply shape: short visible paragraph + folded `<details>`** | Felix's ask. Visible part = a decision the contributor can act on; folded part = auditable data marked AI-assisted |

## 4. Files modified

**Committed** (`1a4f8fa`, 14 files, +2469/−38): `rust/bismark/src/aligner/{discovery,config,mod,cli}.rs`, `rust/bismark/tests/{aligner_cli,aligner_five_base_groundtruth}.rs`, `CHANGELOG.md`, `docs/src/content/docs/rust/illumina-5-base.md`, `plans/08102026_1099-five-base-genome-discovery/` (PLAN rev 1 + §10 notes + §11 review fixes, PLAN_REVIEW_A/B, CODE_REVIEW_A/B, COVERAGE).

**Uncommitted:** `SESSION_HANDOFF.md` only — deliberately kept out of `1a4f8fa` (it travelled across from `dev` before the branch existed).

| Outside the repo | Change |
|---|---|
| GitHub | #1099 + #1100 opened; #1101 opened→merged; #1091 commented + **closed**; #1095 commented ×2 |
| `~/.claude/…/memory/project_faster_inputs_1025.md` | #1091 park record, measured ceilings, the 5 critical-path facts, the `count_effective` verdict, temp-materialization figures |
| `~/.claude/…/memory/MEMORY.md` | Pointer line updated (3× rejected; `count_effective` also rejected) |

## 5. Gotchas and constraints

### New this session

**N1 — 🔑 The measured denominator for ALL input-decode work; do not re-derive it.** Bismark pins `flate2` with `features = ["zlib-rs"]` (`rust/bismark/Cargo.toml:31`) — the *same* engine as `rapidgzip-core` (libz-rs-sys) — so rapidgzip's 1-thread number **is** Bismark's sequential rate. From Felix's own oxy run (rapidgzip-rust#6, 2.1 GB → 16.6 GB, 64-core): 1 thread **10.2 s**, 16 threads **4.7 s**, system `gzip -dc` 55.6 s (that box's gzip is slow — **not** Bismark's baseline). ⇒ a 10.9 GB mate inflates in ~6.7 s; a PE production sample does 4 serial inflates ≈ **27 s of a 638-min run = 0.07 %**.

**N2 — Apple Silicon distorts decode benchmarks.** Apple's zlib hits 1.7–1.9 GB/s here, *above* zlib-rs's ~1 GB/s. Laptop decode numbers understate a Linux/x86 box, and a 0.1 % effect is unresolvable against ~30 % run-to-run variance. Benchmark decode on oxy.

**N3 — rapidgzip's `decoder_threads(N)` is a CEILING, not a request** (Rob Patro, rapidgzip-rust#6 comment `5207994879`). Adaptive bootstrap ≈ `ceil(2*sqrt(min(configured, affinity-visible CPUs)))`, so 64 on 64 cores *starts* at 16, then probes 16→32→48→64 only with enough calibration waves and keeps a wider width only past a 3 % noise tolerance. **`DecoderStats::spawned_workers` is an instantaneous sample, not a high-water mark.** Felix's "capped at 16?" question is **answered — expected bootstrap, no bug.**

**N4 — rapidgzip 0.2.0 added non-seekable/streaming input** (validated by Felix on oxy, byte-identical via pipe/FIFO/`/dev/stdin`/process substitution; ~2.3× slower than the parallel path). So #1091's third dispatch branch is no longer needed on 0.2.x.

**N5 — 🔑 `quotas`' doc comment is the authority on what byte-identity requires** (`aligner/parallel.rs:114-117`): *"Any contiguous partition is byte-identical after the ordered merge."* Worker-invariance needs contiguous + in-order + exactly-once, **not** particular boundaries, and `eff` feeds nothing but `quotas`. Read the comments around an invariant before deriving it from the call graph.

**N6 — the suite materializes read data on disk 2–6×**: splitter subsets (1/mate, **serial**, before `thread::scope`) + converted temps (1 directional / 2 non-directional per mate, already N-way parallel). PE non-directional full sample ≈ **65 GB** of temps; the write is the bigger half of the splitter. The lever, if I/O ever matters, is Bowtie 2 stdin (`-U -`) — aligner-v2 work.

**N7 — 🔑 SHARED SCRATCH PATHS BETWEEN AGENTS CAUSE VACUOUS VERIFICATION.** My first #1099 end-to-end run "passed" against `$TMPDIR/g1099` — the exact path written in the plan's own §6 recipe, which **Reviewer B had already populated with placeholder index files**. Both the positive result *and* the negative control were meaningless (the control reached bowtie2 instead of erroring). **Never put a fixed scratch path in a plan other agents will read; use `$TMPDIR/<name>_$$`.** A live instance of G19.

**N8 — 🔑 Edit anchoring: inserting a function above an existing one ORPHANS its doc comment.** My `old_string` began at `pub fn discover_genome`, so its doc stayed put and the new helper landed *between* doc and function — leaving a `pub` item undocumented and the helper describing something else. All three phase-5 agents caught it. **Anchor the edit on the doc comment, not the signature.** And note `missing_docs` is deliberately off for the aligner/genome_prep/report modules (`lib.rs:8-12`), so **neither clippy nor the build can catch a doc regression there.**

**N9 — `head -N` on `cargo test` output truncates before the integration suites report.** My "full suite green" claim came from 8 grep hits that were the lib result plus empty ones; the 116-test `aligner_cli` suite hadn't printed yet. Grep the whole stream (`^(test result|error|failures:)`) — and note **`${PIPESTATUS[0]}` returns empty in this shell** after a `command grep` pipeline, so assert on positive evidence (`Finished`, `test result: ok`) rather than an exit code.

**N10 — `resolve` execs `<aligner> --version`** via `detect_aligner` (`config.rs:864`), *after* genome discovery, and `rust_ci.yml` installs **minimap2 + samtools only** in all three `cargo test` jobs — never bowtie2/hisat2. So a resolve-level test **cannot assert `Ok`**; use the `if let Err(e) = resolve(…)` + `!matches!(e, <variant>)` idiom (`config.rs`'s `five_base_duplex_guards`). Also `check_exists` runs *before* discovery, so such tests need real (empty) read files or they exit at `InputFileMissing`. This killed two of rev-0's six tests.

**N11 — line-number citations in doc comments go stale inside the same commit.** My `mod.rs:943` guarantor reference was invalidated by this very change's own hunk (it landed on an `unreachable!` arm). **Anchor on function names.**

**N12 — a bare `--five_base_index` basename can resolve via `BOWTIE2_INDEXES`/`HISAT2_INDEXES`.** Any presence check for #1100 must handle that, and must replicate `discover_genome`'s **two-arm** small→large probe (`first_missing` takes `large: bool` and has no fallback of its own), or it will falsely reject a valid `.bt2l`. `first_missing`/`index_suffixes` are private.

**N13 — multi-line Rust string literals defeat single-line grep.** Searching for the full error sentence returned nothing and nearly sent me down a "the user is running the Perl script" path; the string is in `error.rs:45`, wrapped across two lines. Search a distinctive fragment of one line.

**N14 — `gh` GraphQL fails TLS under the sandbox** (`x509: OSStatus -26276`), so `gh issue/pr view --json` needs `dangerouslyDisableSandbox`. An invalid `--json` field makes `gh` print the full valid-field list — read it rather than guessing. **`gh pr close --comment "$(cat file)"`** works and attaches the note to the close event (there is no `--body-file` on `gh pr close`).

**N15 — a gzip "parallel-encoder" byte probe is noise-dominated.** In 67 MB, `00 00 FF FF` has an expected chance count ≈0.016 and `1f8b08` ≈4; single-digit hits prove nothing (a real sync-flush encoder shows hundreds). Felix's Trim Galore outputs are plain single-stream.

**N16 — `rm -rf` inside a compound command is permission-denied**; plain `rm -f <explicit paths>` works, and `git update-ref -d` cleans test-merge refs.

**N17 — when handed one comment URL, list the whole thread before replying.** Felix linked #1095's `5244413281`; I answered it without noticing `5226631117` had landed two days earlier. It happened to need no reply, but that was luck. `gh api repos/.../issues/<n>/comments --jq '.[] | "\(.created_at) \(.user.login) \(.id)"'` costs one call.

**N18 — @Danielsm8 has no name on their GitHub profile** (`name`, `company`, `bio`, `location` all null; 0 public repos). "Mike Daniels" is unverified — the handle parses either way. Address them as **@Danielsm8** (both existing comments do) and use **they/them** until they sign off otherwise.

### Carried forward (verbatim-critical)

**G13/G19/G20** — vacuous verification (N7/N9 are fresh instances); a check whose failure you have never observed is not yet a check; verify contested reviewer claims at source.

**G21/G22** — `gh` / `git push` / `curl` need `dangerouslyDisableSandbox`; `gh api` does not paginate by default; quote URLs containing `?`.

**G23** — BRANCH FIRST (honoured: `1099-five-base-genome-discovery`). **G25** — merging to `dev` closes no issues. **G28/G29** — `git status` before deleting a branch; verify a squash by `git diff <tip> origin/dev` = **0 lines** plus PR state, never ancestry (both done; the merge command itself printed nothing, so its silence proved nothing).

**H3** — declare CI green only from `gh run view <id> --json jobs` (done: 14/14 enumerated, incl. `perl-oracle byte-identity` and **both** feature jobs). **H4** — mirror any tool-availability guard into BOTH feature jobs. **`gh run list --commit` needs the FULL 40-char SHA** or it matches nothing and "0 incomplete" passes vacuously.

**Session `grep` is a ugrep wrapper** — use `command grep` for anything load-bearing.

**`plans/` is off-limits** unless Felix provides a file or asks — honoured; the #1034/#1095 evidence came from memory and PR threads instead.

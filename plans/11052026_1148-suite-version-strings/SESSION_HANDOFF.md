# Session Handoff — 2026-11-05

The next task is **#1148** (the suite version in outputs, not `v0.25.1`). Plan rev 0 is written. Felix has not reviewed it yet; **wait for his feedback, then run the dual plan review, then wait for "implement"**.

## 1. What we accomplished

- **#1147 MERGED into `dev` as `94faf8d`** (squash, 2026-11-04): 5-Base `--bowtie2` defaults to `--maxins 800`.
  - CI was green on all 15 checks, including the perl-oracle gate.
  - **`--dovetail` was dropped.** A 10M dry run showed that the dovetail overhangs are not genomic: 35% mismatches, CHG/CHH 22–26%.
  - An `-I` above the cap now fails early.
  - The pre-registered confirmation passed every check:
    - NA12878: byte-equal to #1146 B-X;
    - HG002: mapping 90.5% → 94.5%, CpGs at cov ≥ 10 +30%.
  - Everything is in `plans/11012026_5base-bowtie2-defaults/` on `dev`: `RESULTS.md`, `PREREGISTRATION.md` rev 2, the reviews and `COVERAGE.md`.
- **Wiki update for #1147**, committed on branch `wiki/update-2026-11-04` (`e33686b`) in worktree `~/Github/Bismark-wiki-update`. **Parked by Felix: not pushed, no PR.**
  - It updates `reference/aligner-cli.md`, `usage/align-reads.md`, `troubleshooting.md` and `overview.md`.
  - It adds the `log.md` line `## [2026-11-04] update | bismark @ 94faf8d, …`.
- **#1148, plan rev 0**, written (uncommitted) in worktree `~/Github/Bismark-1148`, branch `fix/1148-version-strings` from `dev` `94faf8d`:
  - `plans/11052026_1148-suite-version-strings/PLAN.md`
  - `plans/11052026_1148-suite-version-strings/PROGRESS.md`
  - this file

  Felix chose **option 1**: emit the suite version (`rust/VERSION`, now 3.1.0) everywhere, and let the Perl-oracle and golden comparisons map only that token back to `v0.25.1`.

## 2. What's still pending

1. **#1148:** Felix reviews plan rev 0 and answers its questions:
   - **Q1:** drop the genome-prep banner's "(last modified: 19 May 2022)"? Default: yes.
   - **Q2:** do the nf-core/methylseq or nf-core/modules bismark snapshots md5 any version-bearing file? If so, they will change at every suite bump. This one is important.
   - **Q3:** reply on the issue now, or when the PR opens? Default: when the PR opens.

   Then the dual plan review, then wait for "implement".
2. **Wiki branch `wiki/update-2026-11-04`:** push it and open a PR into `dev` when Felix says so. After #1148 merges, it also needs the `wiki/reference/methylation-call-files.md` header line (it quotes `v0.25.1`); a `/tool-wiki update` can add that.
3. **Follow-ups from #1147, not filed:**
   - Does minimap2, the 5-Base default engine, admit the same non-genomic overhang pairs?
   - Does bisulfite `--dovetail` carry the same artefact?
   - Check the methylseq 5-Base snapshots before the next release.
   - minimap2 `-K` throughput.
4. **Cleanup, waiting on Felix:**
   - oxy scratch `/var/tmp/5base_engines`: the BAMs were kept until #1147 merged, so it can now be deleted;
   - the remote branch `feat/5base-bowtie2-defaults`;
   - the worktrees `~/Github/Bismark-5base-bt2` and `~/Github/Bismark-5base-default`. The latter holds the only copies of the #1146 raw logs and tarball.

## 3. Key decisions (with rationale)

| Decision | Rationale |
|---|---|
| #1148 option 1: the suite version everywhere | A 3.x run claiming to be 0.25.1 misleads users, and MultiQC (which parses `(version: v…)` and `Bismark Extractor Version: v…`) labels runs 0.25.1. |
| Keep the `v<digits>` shape (`v3.1.0`); summary HTML bare `3.1.0` | MultiQC's regexes need `v([\d\.]+)`. The summary template already prints "version {{bismark_version}}". |
| One source, `meta::VERSION_TAG` = `concat!("v", env!("BISMARK_SUITE_VERSION"))` | `--version` already uses `BISMARK_SUITE_VERSION`. One const keeps the tools from drifting. |
| Normalise only the version token in Perl comparisons; leave the goldens untouched | Keeps byte-identity for everything else, like the existing `localtime` exceptions in the report and summary gates. |
| No compatibility flag to restore `v0.25.1` | Nothing needs it, and it would make the Perl gate depend on configuration. |
| #1147: `--dovetail` dropped; explicit `--dovetail` is a no-op on 5-Base | The overhangs read as false methylation. Felix's call, 2026-11-02. |

## 4. Files modified

| File | Change |
|---|---|
| `~/Github/Bismark-1148/plans/11052026_1148-suite-version-strings/{PLAN.md,PROGRESS.md,SESSION_HANDOFF.md}` | new, uncommitted |
| `~/Github/Bismark-wiki-update/wiki/*` | committed `e33686b`, unpushed |
| memory `project_5base_default_engine.md`, `MEMORY.md` | #1147 merged; `--dovetail` dropped and why |
| oxy `~/5base_results_bt2`, `~/5base_harness_bt2_r6`, `/var/tmp/5base_engines/{bin5,src5,runs}` | #1147 run artefacts |

## 5. Gotchas and constraints

- **The #1148 emitters** (all `pub const` today):
  - `aligner::BISMARK_VERSION` (`aligner/mod.rs`): report line 1 (`aligner/report.rs`), `@PG VN` (`aligner/output.rs` `generate_sam_header`, and `aligner/mod.rs` for `--five_base_bisulfite_bam`), and the bisulfite-BAM run summary;
  - `extractor::output::{BISMARK_VERSION, SPLIT_FILE_HEADER}`;
  - `summary::BISMARK_VERSION` (`"0.25.1"`, no `v`);
  - `genome_prep::BISMARK_VERSION` (the banner in `genome_prep/pipeline.rs`).

  `report::BISMARK_VERSION` is unused, and the report HTML echoes its input.
- **Oracles:**
  - The `perl-oracle` CI job runs exactly 13 tests (`rust_ci.yml`) and fails unless all 13 pass.
  - `perl_vs_rust_nondirectional_pe_extractor` byte-compares the splitting report and split files. The summary oracle compares HTML.
  - Run the oracles locally with `BISMARK_REQUIRE_PERL=1`, and run the full suite on Linux (oxy).
- **Parser tests with `v0.25.1` input stay as they are:** `report/reports/alignment.rs`, `summary/parse.rs`, `nome_filtering/nome.rs`. They test parsing Perl-written reports.
- **macOS:** about 31 bisulfite FIFO-streaming tests in `aligner_cli` failed on 2026-11-04, for environmental reasons (busy XProtect). Linux passed 2329/0 on the same code. Use oxy or CI for the full suite.
- **GitHub from Claude:** SSH is unavailable. Fetch and push with `git -c credential.helper='!gh auth git-credential' <fetch|push> https://github.com/FelixKrueger/Bismark.git …`. GitHub has returned transient 500s on push; retry. The pre-push hook runs fmt and Clippy.
- **Sandbox:** cargo and git writes in sibling worktrees need `dangerouslyDisableSandbox`, and `sed -i` in worktrees is blocked, so use the Edit tool.
- **oxy:** reach it with `dcli ssh oxy`. Write files with base64 plus `mv`, never in place, while a bash script is running from them. `pkill -f <pattern>` matches its own ssh command line, so use `pgrep` with a bracketed pattern from a separate call. The pod can recycle and wipe `/var/tmp`.
- **Workflow:** the exact word "implement" is required before any source edit. Plans get the dual plan review; implementations get the dual code review plus plan-manager. CHANGELOG entries are one sentence, source comments one line, and current state only.

# Session Handoff — 2026-11-02

Next task: **implement plan rev 3** in `/Users/fkrueger/Github/Bismark-5base-bt2` (branch `feat/5base-bowtie2-defaults`, from `dev` at `d1764d0`).

Plan: `plans/11012026_5base-bowtie2-defaults/PLAN.md`. Felix approved rev 2 subject to a quick recheck; rev 3 applies that recheck. **Felix has not yet said "implement"; wait for that word.**

## 1. What we accomplished (this session, 2026-10-28 → 11-02)

- **#1134: 5-Base honours Bowtie 2/HISAT2 tuning options.** Finished Codex's unfinished worktree: fixed the test fixtures, closed the audit gaps, ran dual review twice.
  - PR **#1138** merged into `dev` as `9b5c15b`. Issue #1134 stays open until the `dev`→`master` release.
- **Docs-site `undici` 8.12.3 security bump.** #1141 (Dependabot) merged into `master` as `8eccde05d` with `--admin`, at Felix's request. Mirrored into `dev` by #1142, merged as `60c495b`.
- **5-Base default-engine benchmark** (minimap2 vs Bowtie 2 / B-loc / B-X / HISAT2 against DRAGEN, NA12878 100M pairs, pre-registered).
  - **Outcome (a): minimap2 stays the default.** Bowtie 2 `-X 800` ties minimap2 but fails the gain-set guard and the +2% margin.
  - PR **#1146** merged into `dev` as `d1764d0`. It contains:
    - a docs "Choosing an engine" table;
    - the start-up notice naming the engine actually used (it used to say minimap2 and "SE + PE" for every run);
    - the harness `validation/five_base_engine_bench.sh` and `validation/five_base_compare.py`;
    - all plan, review and coverage files under `plans/10292026_5base-default-engine/`.
- **This task's plan**, the 5-Base Bowtie 2 defaults `--dovetail --maxins 800`, went through four rounds:
  - rev 0: written;
  - rev 1: Felix's answers;
  - rev 2: both plan reviews applied;
  - rev 3: the quick recheck applied.

  The reviews are `PLAN_REVIEW_reviewer-A.md`, `PLAN_REVIEW_reviewer-B.md` and `PLAN_REVIEW_R2.md`. The plan folder is not committed yet.

## 2. What's still pending

1. **Implement plan rev 3, in its outline order:**
   1. tests and the builder change in `options.rs`, then commit and record the hash;
   2. harness: redefine the #1146 arms with explicit flags; add B-old and B-new; add the `rescued` helper (`validation/five_base_rescued.py`, with a self-test); add the duplex pass and the `RX` check; record tool versions;
   3. commit `PREREGISTRATION.md` **before any B-new cell runs**;
   4. stage on oxy via symlinks;
   5. 10M dry run;
   6. screen on NA12878 and HG002;
   7. `RESULTS.md`;
   8. **stop and present the result to Felix.**
2. **Output storage (resolved by Felix, 2026-11-02):**
   - Large outputs (BAMs, CpG reports, consensus BAMs) stay on oxy's scratch, `/var/tmp/5base_engines`. `/tmp` and `/var/tmp` are the same disk, roughly 1 TB with plenty free.
   - Only small results (metrics, scores, reports, M-bias, logs) go to the persistent `~/5base_results`, because `/home` has about 8 GB free.
   - Scratch can be wiped by an idle restart, as on 2026-10-31. If that happens mid-run, re-run the affected cells; the inputs are on the read-only S3 mount.
   - Keep the large outputs until the PR merges; don't delete scratch early.
3. **Follow-ups, not filed as issues:**
   - minimap2 `-K 250K` throughput: minimap2 kept about 3.8 of 32 threads busy, from noisy wall times.
   - The HISAT2 CpG loss is unexplained.
4. **Housekeeping:**
   - `/Users/fkrueger/Github/Bismark-5base-default` (now on `dev`) holds the only copies of the untracked raw `bismark.log` files and `bench_results/oxy_5base_results.tgz`. Delete it when no longer needed.
   - Issue #1134 closes at the next `dev`→`master` merge.

## 3. Key decisions (with rationale)

| Decision | Rationale |
|---|---|
| `--dovetail` becomes a default, not a gated option | Felix: it is the bisulfite Bismark default and generally desirable. The run is a no-harm confirmation; on a failure, stop and report, never drop it silently. |
| Primary check on the **rescued pairs** (R1–R6), not genome-wide | They are about 0.7% of pairs, so genome-wide guards dilute any harm about 140×. The likely harm is R1 overhang bases, which the extractor's overlap rule keeps and which deconvolution (R1-only) also reads. |
| The gain-set guard is reported, not deciding | Its sites sit at cov 10–11, which lowers r by construction; #1146's B-X failed it for that reason. |
| Duplex pass via `--five_base_consensus_from_bam` on the B-old and B-new BAMs | No re-alignment. It needs `--five_base_umi_qname` at alignment so `RX:Z:` exists; the dual UMI is in the read names (`…:CAGTAGT+CACCTAC`). |
| One PR for `-X 800` and `--dovetail` | Felix, 2026-11-02. |
| HISAT2 out of scope | It has no `--dovetail`, and its loss is not explained by the insert cap (TLEN matches minimap2). |
| Inputs are symlinked from the S3 mount, not synced | Felix. The mount `/datasets/s3/altos-lab-bioinf/CI/Illumina_5base_test_data/` is **read-only**, so symlink each file into writable directories, because prep writes `dragen.cpg.txt` into `dragen/`. |
| Shared reference indexes | `~/references/GRCh38/{Bowtie2Index,Hisat2Index,WholeGenomeFasta}` on oxy link to `/datasets/s3/altos-lab-reference-data/genomes/Homo_sapiens/Ensembl/GRCh38/`, prefix `Homo_sapiens.GRCh38`. Verified: 194 sequences matching the FASTA; the HISAT2 index is plain. |
| BaseSpace access | Felix's personal account is private domain `gp8e3fxj`, region EUW2. Use `bs auth --force --timeout 1800 --api-server https://api.euw2.sh.basespace.illumina.com` and approve at `https://gp8e3fxj.euw2.sh.basespace.illumina.com/oauth/device?code=…`. The Altos domain `v0pjcta4` has the CLI disabled. The data is already in S3, so no re-download is needed. |

## 4. Files modified

| File | Change |
|---|---|
| `plans/11012026_5base-bowtie2-defaults/{PLAN.md,PROGRESS.md,PLAN_REVIEW_reviewer-A.md,PLAN_REVIEW_reviewer-B.md,PLAN_REVIEW_R2.md,SESSION_HANDOFF.md}` | new, uncommitted, in this worktree. This handoff is kept here, not at the repo root, because the tracked root `SESSION_HANDOFF.md` belongs to another session. |
| oxy `~/5base_harness/` | the #1146 harness, matching `dev` (`five_base_engine_bench.sh`, `five_base_compare.py`, `concordance.py`) |
| oxy `~/references/GRCh38/` | symlinks to the shared indexes |
| oxy `~/bin/bs`, `~/.basespace/default.cfg` | BaseSpace CLI 1.7.1 with the personal EUW2 login. The Babraham token was deleted. |
| oxy `~/Downloads` | symlink to `/var/tmp/5base_engines/download`, now dangling after the scratch wipe |
| memory `project_5base_default_engine.md` | the benchmark outcome, BaseSpace recipe, #1146 merge and this pending follow-up |

## 5. Gotchas and constraints

- **oxy scratch is ephemeral.** An idle restart around 2026-10-31 wiped `/var/tmp`, losing the #1146 BAMs and CpG reports.
  - Large outputs stay on scratch anyway (§2.2). Copy small results to `~/5base_results` as each cell finishes.
  - `dcli ssh oxy` may take 5–10 minutes when the workspace is starting.
- **oxy tools:** `~/micromamba/envs/bismark-test/bin` (Bowtie 2 2.5.5, HISAT2 2.2.2, minimap2 2.31, Trim Galore 2.2.0 Oxidized, cutadapt 2.6, samtools 1.23.1); cargo in `~/.cargo/bin`. `awk` is **mawk**: no `and()`. NUMA node 0 is CPUs 0–31.
- **Build Bismark on oxy** with a `git clone --shared ~/Github/Bismark` into `/var/tmp/5base_engines/src`, then `cargo build --release -p bismark` (about 1 minute).
- **Moving files to oxy:** `dcli scp` rejects paths. Use `echo <base64> | base64 -d > file` over `dcli ssh`, writing to a temp file and then `mv`, so running scripts aren't truncated.
- **Trim Galore** keeps the input's compression. The #1146 screen trims were plain `.fq`, and their md5s are of plain files, so compare `zcat` output against them.
- **5-Base `NM`/`MD`** count every called C→T/G→A as a mismatch. The R4 mismatch rate must exclude XM-called positions.
- **The `@PG` CL** is the user's command line, so defaults appear only in the report's options line.
- **`--no_dovetail`** also flips the driver's TLEN sign on equal-extent OB pairs. No flag combination reproduces old 5-Base Bowtie 2 BAMs byte-for-byte, so never promise that in the docs.
- **Bisulfite token order** is `--no-mixed --no-discordant --dovetail --minins --maxins --quiet` (`options.rs:56-92`). The 5-Base builder must emit `--dovetail` before `--maxins`. `-p N --reorder` are appended in `mod.rs` (about line 1900), not by the builder.
- **Sandbox:** cargo, git writes and `dcli` in sibling worktrees need `dangerouslyDisableSandbox`. `sed -i` in this worktree is blocked inside the sandbox; use the Edit tool. `ps` and `mktemp -d` are blocked on the laptop (use `mktemp -d "$TMPDIR/x.XXXXXX"`).
- **Workflow:** Felix's CLAUDE.md requires the exact word "implement" before source or test edits. After implementation, run dual code review plus plan-manager. Merges to `master` need `--admin`; the auto-mode classifier may block that unless Felix asks explicitly.

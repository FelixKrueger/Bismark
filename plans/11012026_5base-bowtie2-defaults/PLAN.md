# 5-Base Bowtie 2 default: `--maxins 800`

## Rev 6 (2026-11-03): rev 5 with both rev 5 reviews applied

Rev 6 changes rev 5 only where stated below; everything else in rev 5 (and in rev 4's motivation, Goal and Behavior) still applies. The reviews are `PLAN_REVIEW_R5_A.md` and `PLAN_REVIEW_R5_B.md`.

### Behavior

- **The `-I` check is Bowtie 2-only.** It errors when `-I` is greater than the explicit `-X` or the 800 default. HISAT2 is out of scope, as in rev 4, and keeps Bowtie 2's silent behaviour. The docs say so in one line.
- **Wording fix:** `--no_dovetail` flips the TLEN sign only on OB pairs whose two mates share the same start (`output.rs:575`), not on "equal-extent" pairs. This applies wherever rev 4 or rev 5 says equal-extent.

### Tests (additions to rev 5's list)

- **`options.rs`:**
  - `-I 900 -X 800` errors, and the message names 800;
  - `-I 600 -X 500` errors, and the message names 500;
  - `-I 801` errors;
  - `-I 800` and `-I 900 -X 1000` pass;
  - the exact error text is asserted;
  - `five_base_local_scoring_is_engine_specific` returns to strings without `--dovetail`, keeping `--maxins 800`.
- **`aligner_cli.rs`:** the `-I 900` case joins `five_base_1134_invalid_tuning_fails_before_output_creation`, which also asserts that no output directory and no engine call happen. Bowtie 2 only.
- **Renames**, so names match what the tests assert:
  - `five_base_bowtie2_defaults_to_dovetail_and_maxins_800` becomes `five_base_bowtie2_defaults_to_maxins_800`;
  - `five_base_bowtie2_dovetail_and_maxins_defaults_reach_argv` becomes `five_base_bowtie2_maxins_default_reaches_argv`.
- **Real-Bowtie 2 dovetail test:** `align()` takes several named pairs, and the "both records are the control" check is by QNAME.

### Harness

- **Isolation on oxy:**
  - The rev 6 harness goes to its own directory (`~/5base_harness_bt2_r6`), and the binary to `$ROOT/bin5` with `BIN` overridable. The rev 3 harness and its running streams are never touched.
  - `cell … B-old` refuses to run with the message "rev 3 cross-check arm; not re-runnable", so its DONE key can never trigger the `rm -rf`.
  - The rev 3 streams end at NA12878 `repro B-old` and HG002 `cell B-old`. Neither runs a rev 3 B-new, so `runs/<sample>/screen/B-new.rep1` is free for rev 6.
  - Stream CPU sets are disjoint: 0-31 and 32-63.
- **`versions`** writes `versions_<commit>.txt`, so rev 3's record survives.
- **`repro <sample> <tag> <arm> <ref-dir>`** writes `repro_<arm>.tsv` and `repro_score_<arm>.tsv`, and the score label is the arm name.
- **New `repro-compare <sample> <tag> <arm1> <arm2>`.** It compares the two cells over `repro`'s column list, the deconvolution counts and the own-set cov ≥ 10 n and r, not the whole `metrics.tsv`, and writes `repro_compare_<arm1>_<arm2>.tsv`. Any difference fails.
- **`rescued`** (rev 6 definitions; pre-registered in rev 2):
  - **Draws:** one draw from `random.Random(1146)` per B-new pair, in FASTQ order, whatever the pair's set.
  - **Control sample:** rest pairs with a draw < 0.01 are the per-base control.
  - **Admitted sample:** admitted pairs with a draw < 0.10 are the per-base admitted sample for A4. This replaces rev 5's tally over every admitted base.
  - **Pool:** rest pairs with a draw < 0.05 are the A5 rest pool. The equal-size sample and its abort are removed (Reviewers A and B, C1).
  - **Length-matched subset (report only):** rest pairs with 450 ≤ |TLEN| ≤ 500 are also tallied as `rest_long`.
  - **M-bias tallies:** R1 CpG over cycles 0–9 and cycles 60–69 from the 3′ end. Reads shorter than 70 bp count only toward the 0–9 window.
  - **TLEN reporting:** admitted and rest |TLEN| p10/p50/p90, plus the fraction of admitted pairs with 500 < |TLEN| ≤ 800.
- **`verdict`** takes `--old-arm`/`--new-arm` for the score keys and cell directories, and evaluates A1–A7 below.
- **Self-test coverage**, specified:
  - `--all-bases` admitted tallies, including the sample draw;
  - the |TLEN| quantiles and the 500–800 fraction;
  - A5 matched-site scoring on a hand-built case;
  - the A6 difference-of-differences;
  - `criteria()` with arbitrary arm names;
  - a pass/fail boundary case for each of A1–A7;
  - the bench self-test covers the B-old refusal and the per-arm `repro` filenames.

### HG002 criteria, replacing rev 5's A1–A6

- **A1.** Admitted pairs are 1–15% of B-new's pairs.
- **A2 (mechanism).** At least 95% of admitted pairs have 500 < |TLEN| ≤ 800. Failing A1 or A2 means investigate: the mechanism is not the insert cap.
- **A3.** Admitted CHG and CHH are each ≤ the rest's + 0.5 pt.
- **A4.** Admitted CpG is within ±5 pt of the rest's. The length-matched `rest_long` comparison is reported alongside it.
- **A5 (mismatches).**
  - The admitted sample's mismatch rate over aligned bases whose XM character is `.` is ≤ 2× the cycle-matched control's.
  - The admitted sample's CHG and CHH are each ≤ the cycle-matched control's + 1.0 pt.
  - Each comparison needs n ≥ 10,000.
- **A6 (per-call DRAGEN agreement, at matched sites).**
  - A site counts if DRAGEN has cov ≥ 10 there and it has at least one call from the admitted pairs and at least one from the rest pool.
  - At each site, score each set by its mean per-call score (p if methylated, 1 − p if not).
  - The mean over sites of (admitted − rest) must be ≥ −0.02, and n ≥ 10,000 sites is needed.
  - Matching the sites removes the composition effect Reviewer B raised (I3).
- **A7 (M-bias).** Per set, D = CpG over R1 3′ cycles 0–9 minus CpG over cycles 60–69. Pass when |D(admitted) − D(rest)| ≤ 3 pt. This compares a within-read 3′ shift, not an absolute level.

### NA12878 gate and cross-checks

- **Gate:** `repro NA12878 screen B-new` against #1146 B-X, as in rev 5.
- **Cross-checks**, failable:
  - `repro-compare NA12878 screen B-old B-new`: rev 3's B-old cell, built at `72bbf7b`, must equal B-new;
  - `repro-compare HG002 screen B-old B-new`: same check, which gives the HG002 B-old cell a use.

### Declined rev 4 review items, with reasons

- **10M equivalence check instead of the 100M cells (R4-A O1).** Declined. The gate is reproduction of #1146's committed 100M B-X, and no 10M reference exists. HG002 needs a full-depth B anyway.
- **Index and genome checksums (R4-A O3).** Declined. The NA12878 gate can only pass on the same index and genome, so it checks them implicitly. The paths are recorded in `bismark.log`.
- **CLI help text (R4-A O7).** No change needed: `-X`'s help names no default.
- **Adopted instead:** the `rust/README.md` status-journal row (R4-B). It is added in the docs step: the aligner row and a dated Milestones line.

### Docs additions (to rev 5's list)

- one line saying that `-I` above the maximum insert is an error with Bowtie 2;
- one line on `--no_dovetail`: in 5-Base it only flips the TLEN sign of OB pairs whose mates share a start;
- "two libraries" (NA12878 and HG002) once HG002 has passed.

### `PREREGISTRATION.md` rev 2 (additions to rev 5's list)

- **Rev 1 is withdrawn.** Its void-if-changed clause lapses with it, because rev 1's outcome was never applied. Rev 2 carries its own clause: if a rev 2 B-new or B cell has produced data and rev 2 is then changed, that run is void.
- **Rev 6 definitions** are recorded: draws, samples, A1–A7, `repro-compare`.
- **Seen at freeze time:** which rev 3 B-old figures had been seen. If HG002 B-old has finished, its metrics are listed. A1–A7 and the secondary checks do not use B-old.

### Implementation outline

The outline is rev 5's, with these changes:
- **Step 4 (oxy):**
  - deploy to `~/5base_harness_bt2_r6` and `bin5`;
  - after the rev 3 B-old cells finish, stream 1 (CPUs 0-31) runs NA12878 `cell B-new`, `repro B-new`, then `repro-compare B-old B-new`;
  - stream 2 (CPUs 32-63) runs HG002 `cell B`, `cell B-new`, `repro-compare B-old B-new`, `rescued --old-arm B --new-arm B-new --all-bases`, `score HG002 screen B B B-new`, then `verdict`.
- **Estimated cost:** about 6 h of wall time.

---

## Rev 5 (2026-11-02): rev 4 with both plan reviews applied

Superseded in part by rev 6.

This section replaces rev 4's Validation and Implementation outline. Rev 4's motivation, Goal and Behavior still apply, with the additions below. The reviews are `PLAN_REVIEW_R4_A.md` and `PLAN_REVIEW_R4_B.md`, and the two agree on the main points.

### Behavior additions

- **`-I` above the cap fails early.** If `-I/--minins` is greater than the maximum insert in effect, which is `-X` or the 800 default, the 5-Base Bowtie 2 builder errors. Bowtie 2 accepts that combination, returns every pair non-concordant, and the driver silently drops them all (Reviewer B, checked against Bowtie 2 2.5.5).
  - The error reads: `-I/--minins (<I>) is larger than the maximum insert size (<X>); raise -X/--maxins`.
  - The check is on the 5-Base path only; the bisulfite path is unchanged.
- **Unchanged from rev 4:**
  - `--dovetail` is a no-op (Q5).
  - `--no_dovetail` emits no token but still sets the driver's TLEN sign on equal-extent OB pairs.

### Tests

- **`options.rs`:**
  - exact strings for the default, `-X 500`, `-X 1200`, `-I 600`, `--no_dovetail`, `--dovetail`, `--dovetail --no_dovetail`, `--old_flag` and `--local` (`-q --local --score-min G,20,8 --maxins 800`); none contains `--dovetail`;
  - `-I 900` errors, while `-I 900 -X 1000` and `-I 800` pass;
  - HISAT2 has no default `--maxins`, and `-I 900` on HISAT2 is unchanged (no new check);
  - minimap2 is unchanged.
- **Existing `options.rs` tests:** `--dovetail` goes back on the forbidden list in `five_base_omits_bisulfite_policy_and_runtime_threads`. `five_base_defaults_and_explicit_defaults` and `five_base_forwards_tuning_in_bisulfite_token_order` return to strings without `--dovetail`, keeping `--maxins 800`/`1000`.
- **`mod.rs`:** the stub and the input-order test return to strings without `--dovetail`, keeping `--maxins 800`.
- **`aligner_cli.rs`:**
  - the exact default argv `-q --score-min L,0,-0.6 --maxins 800 -p 4 --reorder`;
  - the #1134 tuning test asserts `--dovetail` count 0 for both engines;
  - the defaults test asserts no `--dovetail` in every case and `--maxins 800` (or the `-X` value) exactly once;
  - an `-I 900` CLI case fails before any output is created;
  - the end-to-end report assertion becomes `-q --score-min L,0,-0.6 --maxins 800`.
- **`aligner_five_base_bowtie2.rs`:**
  - The insert-cap gate is unchanged: 1,000 bp is excluded at the default and admitted with `-X 1200`; 600 bp is admitted at the default and excluded with `-X 500`.
  - The dovetail test is renamed `five_base_pe_bowtie2_drops_dovetailed_pairs`. Each run aligns three pairs:
    - a non-dovetailed control pair, which must be kept (flags 99/147, TLEN ±200);
    - the OT and OB dovetailed pairs, which must be dropped.
  - Runs: no flag, `--dovetail` (pins Q5) and `--no_dovetail`. Every run asserts exactly 2 records, both from the control.

### Harness

**Arms**, with `--five_base_umi_qname` dropped everywhere:

| Arm | Flags | Options it must show |
|---|---|---|
| B | `--bowtie2 -X 500` | `--maxins 500` once; no `--dovetail`, no `--local` |
| B-loc | `--bowtie2 --local -X 500` | `--local`, `G,20,8`, `--maxins 500` once; no `--dovetail` |
| B-X | `--bowtie2 -X 800` | `--maxins 800` once; no `--dovetail`, no `--local` |
| B-new | `--bowtie2` | `--maxins 800` once; none of `--dovetail`, `--local`, `--no-mixed`, `--no-discordant`; `@PG` sets none of `-X`, `--maxins`, `--dovetail`, `--no_dovetail` |
| B-old | rev 3 definition, kept | only to identify the running rev 3 cells |

- B is #1146's B invocation with its old 500 bp default made explicit.
- B-loc gains `-X 500`, fixing a latent bug Reviewer B found: it silently ran at 800.
- B-X is #1146's B-X invocation.
- B-new is the shipped default.

**Other harness changes:**
- `BIN` is overridable, so the rev 5 binary builds into `$ROOT/bin5` and leaves the running rev 3 cells alone (Reviewer A C2).
- `repro <sample> <tag> <arm> <ref-dir>` takes the arm.
- `rescued` takes `--old-arm/--new-arm` and an `--all-bases` mode, which tallies every aligned base of the admitted pairs for the mismatch check, not only the overhang. It also reports admitted-vs-rest |TLEN| quantiles.
- `verdict` takes `--old-arm/--new-arm` and the criteria set `admitted`.
- The duplex/consensus code and D0–D3 are removed from `five_base_rescued.py` and the bench script. They tested what `--dovetail` admits, and rev 5 has no consumer for them; removing them avoids shipping dead code.

**Self-test cases:** every arm above, with pass/fail on `--dovetail` (denied), the `--maxins` value and exactly-once counts, and the B-new `@PG` denials.

### Validation (pre-registered in `PREREGISTRATION.md` rev 2)

1. **NA12878 reproduction, the gate.**
   - `repro NA12878 screen B-new` against #1146 B-X: every committed metric, the deconvolution counts, and the own-set cov ≥ 10 n and r.
   - What it tests: that the shipped default reproduces the configuration #1146 measured, and that the dev commits since `9b5c15b` (#1121, #1145, #1115) are output-neutral for it.
   - **Expectations:**
     - the rev 3 NA12878 B-old cell's `repro` (built at `72bbf7b`) gives the same verdict as B-new's;
     - B-new and B-old match on every non-timing metric, since only TLEN signs of equal-extent OB pairs can differ and no metric reads them.
   - **On a failure, stop and report.** If B-old fails in the same way, the cause is drift from the intervening dev commits, not the builder.
2. **HG002: the pairs the 800 bp cap admits** (`rescued --old-arm B --new-arm B-new --all-bases`).
   - The **admitted** set is pairs proper in B-new and absent from B; the changed and rest sets are as in rev 3.
   - Criteria:
     - **A1.** Admitted pairs are 1–15% of B-new's pairs (mechanism check; #1146 predicts about 7%).
     - **A2.** Admitted CHG and CHH are each ≤ the rest's + 0.5 pt (rev 3 R2).
     - **A3.** Admitted CpG is within ±5 pt of the rest's (R3).
     - **A4.** The admitted mismatch rate over all aligned uncalled bases is ≤ 2× the cycle-matched rest control's, with n ≥ 10,000 (replaces R4).
     - **A5.** Per-call DRAGEN agreement is ≥ the rest sample's − 0.02, with n ≥ 10,000 (R5).
     - **A6.** R1 3′ M-bias CpG is within ±3 pt of the rest's (R6).
   - Reported: admitted vs rest |TLEN| p10/p50/p90, dovetailed/contained counts (dovetailed expected near 0), changed and lost counts.
3. **HG002 genome-wide, B-new vs B** (rev 3 secondary checks):
   - common-set Δr ≥ −0.001;
   - CHG and CHH ≤ B + 0.2 pt;
   - MAPQ-0 ≤ B + 2 pt;
   - deconvolution Δ ≥ −1 pt.

   Reported: ΔCpG, Δmapping, gain-set r.
4. **Outcome.** The PR proceeds only if item 1 passes and A1–A6 plus the secondary checks pass on HG002. Any FAIL or INSUFFICIENT stops the work and goes to Felix.

`PREREGISTRATION.md` rev 2 is a new section at the top. It records:
- rev 1's outcome: the dry run, which rev 1 said would decide nothing, led to dropping `--dovetail`;
- that rev 1's R1–R6 and D0–D3 are withdrawn;
- which rev 3 figures had been seen at freeze time, in particular the HG002 and NA12878 B-old cells (same option string as B-new), if finished, and that no rev 2 threshold was chosen with them;
- the rev 5 binary commit and the harness commit.

### Implementation outline

1. **Builder and tests.** Implement the above; fmt, the focused tests (`--lib aligner::`, `aligner_cli`, `aligner_five_base_bowtie2`, the 5-Base suites), release Clippy. Commit and record the hash.
2. **Harness.** Implement the above; both self-tests pass. Commit.
3. **`PREREGISTRATION.md` rev 2**, committed before any rev 5 B-new or B cell runs.
4. **oxy**, all in `$ROOT/bin5`:
   1. Build the step 1 commit.
   2. Let the rev 3 B-old cells finish. Stream 1 runs `repro` on NA12878 B-old as the cross-check.
   3. Stream 1: `cell NA12878 screen B-new`, then `repro NA12878 screen B-new`.
   4. Stream 2: `cell HG002 screen B`, then `cell HG002 screen B-new`.
   5. Then HG002 `rescued`, `score HG002 screen B B B-new`, and `verdict HG002 screen`.

   Large outputs stay on scratch; small results go to `~/5base_results_bt2`.
5. **Commit results.** `RESULTS.md`, plus the dry-run and screen small results under `bench_results/`.
6. **Present to Felix.**
7. **On a pass:**
   - **docs** in `docs/src/content/docs/rust/illumina-5-base.md`:
     - lines 75–77: the 5-Base Bowtie 2 default maximum insert is 800 bp, and HISAT2's stays 500 bp;
     - the engine table: relabel the "Bowtie 2 `-X 800`" row as the default and the plain Bowtie 2 row as "before this change (500 bp cap)", and caveat the `--local` row as measured at 500 bp;
     - lines 113–116: rewrite the "raise the cap" advice;
     - one line: `--dovetail` is a no-op in 5-Base mode, because dovetail overhangs carry non-genomic bases;
   - **docs** in `docs/src/content/docs/options/alignment.md`: a 5-Base exception for the `-X` default (500) and for "Bismark always passes `--dovetail`";
   - a one-sentence CHANGELOG entry;
   - the full suite, dual code review, the plan-manager audit, and the PR into `dev`;
   - squash commit message: "5-Base: Bowtie 2 defaults to `--maxins 800`", with the dovetail history in the body only.

### Follow-ups (not in this PR; for Felix to file)

- Does minimap2 (the 5-Base default engine) admit the same non-genomic overhang pairs? Not yet checked (Reviewer B).
- Does bisulfite `--dovetail` carry the same overhang artefact? Reviewer A raised it; it would need its own check.
- nf-core/methylseq 5-Base snapshots: check before the release that ships this.

---

## Rev 4 (2026-11-02): `--maxins 800` only

Superseded in part by rev 5. Rev 4's motivation, Goal and Behavior still apply; its Validation and outline do not.

### Why `--dovetail` is out

The rev 3 10M dry run (implementation notes #5) showed the following.

- **Who `--dovetail` lets in.** It admits about 2% more pairs, and nearly all of them are dovetailed with a median overhang of 12 bp.
- **The overhang bases are not genomic:**

  | Overhang bases vs cycle-matched controls | Overhang | Control |
  |---|---|---|
  | Mismatch rate | 35.4% | 0.61% |
  | CHG | 26.1% | 1.9% |
  | CHH | 22.0% | 1.4% |

- **They read as false methylation.** In 5-Base polarity a T at a genomic C is methylated. The rescued pairs' R1 3′ CpG is 27.6% vs 50.8% in the rest.
- **Genome-wide checks cannot see it.** Every one of them passed.

Felix decided on 2026-11-02: drop `--dovetail` and ship only `--maxins 800`.

### Goal

Make `--illumina_5base --bowtie2` run with `--maxins 800` unless the user sets `-X`/`--maxins`. Nothing else in the 5-Base option string changes. HISAT2 and minimap2 stay unchanged.

### Behavior

**Builder strings:**
- Default: `-q --score-min L,0,-0.6 --maxins 800`.
- Engine argv: `-q --score-min L,0,-0.6 --maxins 800 -p N --reorder`.
- `-X N`: `--maxins N`.
- `-I N`: `--minins N --maxins 800`.

**Dovetail flags:**
- `--dovetail` and `--no_dovetail` emit no token on the 5-Base path, as before this branch.
- `--no_dovetail` still flips the driver's TLEN sign on equal-extent OB pairs, as on `dev` today.

**Same as #1146's B-X.** The new default is byte-for-byte the option string of #1146's B-X arm, which ran without `--no_dovetail`. Its alignments, TLENs and metrics are therefore those of B-X, and #1146 already measured that configuration:

| Metric | 500 bp cap | 800 bp cap |
|---|---|---|
| Mapping efficiency | 87.9% | 94.5% |
| CpGs at cov ≥ 10 | 554,655 | 822,432 |
| Common-set r | 0.96313 | 0.96434 |

### Validation

1. **Unit, CLI and real-Bowtie 2 tests.** Rev 3's tests, with `--dovetail` taken out:
   - the exact default string, and every listed case (`-X 500`, `-X 1200`, `-I 600`, `--no_dovetail`, `--dovetail`, `--old_flag`, `--local`);
   - each case shows no `--dovetail`;
   - HISAT2 and minimap2 unchanged.

   The real-Bowtie 2 dovetail test flips: dovetailed OT and OB pairs are **dropped** by default, which pins the rev 4 decision. The 800 bp insert-cap gate stays as it is.
2. **NA12878 reproduction (gate).** The new default (`--bowtie2` alone, with `--five_base_umi_qname`) must reproduce #1146 B-X exactly, through the harness's `repro` check:
   - every committed metric;
   - the deconvolution counts;
   - the own-set cov ≥ 10 n and r.

   This shows the shipped default is the configuration #1146 measured.
3. **HG002, the second sample.** The new default against `-X 500` (the old default), with the rev 3 secondary checks as the pass criteria:
   - common-set Δr ≥ −0.001;
   - CHG and CHH ≤ the `-X 500` arm's + 0.2 pt;
   - MAPQ-0 ≤ +2 pt;
   - deconvolution Δ ≥ −1 pt.

   Reported only: ΔCpG and Δmapping.
4. The rescued-pair and duplex analyses are dropped. They tested what `--dovetail` admits, and the new default admits no dovetailed pairs.

`PREREGISTRATION.md` gets a rev 2 section with items 2 and 3, committed before either new-default cell runs. Rev 1 stays as the record of the dry run.

### Implementation outline

1. **Builder.** In `options.rs`, remove the `--dovetail` emission from `build_five_base_aligner_options`; keep the `--maxins 800` default. Update the unit tests, the `mod.rs` stub, the `aligner_cli.rs` exact-argv and default tests, and flip the real-Bowtie 2 dovetail test. Then fmt, the focused tests and release Clippy. Commit and record the hash.
2. **Harness.**
   - Redefine B-new as "`--maxins 800` exactly once, no `--dovetail`".
   - Add a B-500 arm (`-X 500 --five_base_umi_qname`) for HG002.
   - `verdict` gets a `--secondary-only` mode for HG002.
   - Commit.
3. **`PREREGISTRATION.md` rev 2**, committed.
4. **oxy.**
   - Rebuild at the step 1 commit.
   - Run NA12878 B-new, then `repro` against #1146 B-X (gate).
   - Run HG002 B-new and B-500, then `score` and `verdict --secondary-only`.

   The rev 3 B-old cells now running on oxy are kept only as a cross-check: their builder string is identical, but the binary is not the shipped one.
5. **`RESULTS.md`**, then present to Felix.
6. **On a pass:**
   - docs: the tuning section and the "Choosing an engine" table get a row for the new default;
   - a one-sentence CHANGELOG entry;
   - the full suite, dual code review, the plan-manager audit, and the PR into `dev`.

### Resolved question

- **Q5** (Felix, 2026-11-02): an explicit `--dovetail` stays a no-op on the 5-Base Bowtie 2 path, and the docs say so in one line.

---

## Goal (rev 3, superseded)

Make `--illumina_5base --bowtie2` run with `--maxins 800` unless the user sets `-X`/`--maxins`, and with `--dovetail` unless the user sets `--no_dovetail` (as bisulfite Bismark does). Both changes go in one PR (Felix, 2026-11-02).

Before the PR, a pre-registered confirmation run on NA12878 and HG002 checks that the new defaults do no harm, with the check centred on the pairs that `--dovetail` newly admits. A duplex/consensus pass runs on NA12878. HISAT2 and minimap2 stay unchanged.

Worktree: `/Users/fkrueger/Github/Bismark-5base-bt2`, branch `feat/5base-bowtie2-defaults`, from `dev` at `d1764d0`.

## Evidence

Source: `plans/10292026_5base-default-engine/RESULTS.md`, NA12878 100M-pair screen.

- **`-X 800`.** B (Bowtie 2 at its 500 bp default) vs B-X (`-X 800`):

  | Metric | B | B-X |
  |---|---|---|
  | Mapping efficiency | 87.9% | 94.5% |
  | CpGs at cov ≥ 10 | 554,655 | 822,432 |
  | Common-set r | 0.96313 | 0.96434 |

  minimap2's common-set r was 0.96464. The TLEN p90 is ~478 bp, so the 500 bp cap cuts into the fragment distribution.
- **`--dovetail` (hypothesis).** B-X's mapping is still 0.7 points below minimap2's.
  - Without `--dovetail`, Bowtie 2 marks dovetailed pairs non-concordant, and the 5-Base driver keeps only proper pairs.
  - The harness's `readthrough` metric (|TLEN| < read length) cannot measure this. An end-to-end Bowtie 2 dovetail has |TLEN| ≥ read length (Reviewer A), so this run adds a dovetail-shape count.

## Context

- **Builder:** `rust/bismark/src/aligner/options.rs::build_five_base_aligner_options` emits `--minins`/`--maxins` only from explicit `-I`/`-X`, and never emits `--dovetail`.
  - The bisulfite builder (step 12) emits `--no-mixed --no-discordant --dovetail` and then `--minins`/`--maxins`. See its test at about `options.rs:751`.
  - This plan adds only `--dovetail` and the `--maxins` default to the 5-Base path; `--no-mixed`/`--no-discordant` stay out.
- **Driver:** `mod.rs::five_base_emit_pe_record` keeps only proper pairs (`flag & 0x2`, both mates mapped). It receives `config.dovetail`, which is true unless `--no_dovetail`, and uses it for TLEN. `--no_dovetail` therefore changes two things:
  - the `--dovetail` token;
  - TLEN signs on equal-extent OB pairs.

  So no flag combination reproduces old 5-Base Bowtie 2 BAMs byte-for-byte. Dedup and the extractor don't read TLEN.
- **Extractor overlap removal:** the default `--no_overlap` keeps all R1 calls and drops R2 calls past R1's end. R2's dovetail overhang is removed, but R1's 3′ overhang is kept. Deconvolution reads R1 records only. If the overhang bases are wrong, those are the bases that reach the results.
- **Duplex/consensus:**
  - The family key uses the fragment's outer span, so an overhang on one strand partner can split a family.
  - Consensus adds both mates' bases with no overlap removal.
  - `--five_base_consensus_from_bam <BAM>` (with `--five_base_umi_qname`) replays the collapse over existing BAMs without re-aligning.
- **Harness from #1146:**
  - Its arms B, B-loc and B-X assume the old defaults. B would now fail its `--maxins` deny, and B-loc/B-X would silently gain `--dovetail`.
  - The `@PG` CL is the user's command line, so it cannot show defaults.
- **Data:** read-only S3 mount `/datasets/s3/altos-lab-bioinf/CI/Illumina_5base_test_data/`, holding both samples' `fastq/` and `dragen/` plus `MD5SUMS`.

## Behavior

### 1. Builder (shipped change)

- With no `-X`/`--maxins`, 5-Base Bowtie 2 emits `--maxins 800`. An explicit `-X N` emits `--maxins N`.
- 5-Base Bowtie 2 emits `--dovetail` unless `--no_dovetail` is given. An explicit `--dovetail` (a CLI no-op today) emits it once.
- **Token order follows the bisulfite builder:** `--dovetail`, then `--minins N`, then `--maxins N`, then `--quiet`.
- **The exact default:**
  - the builder string becomes `-q --score-min L,0,-0.6 --dovetail --maxins 800`;
  - the engine argv, after `mod.rs` appends threads and ordering, becomes `-q --score-min L,0,-0.6 --dovetail --maxins 800 -p N --reorder`.
- **Unchanged:** HISAT2 (no default `--maxins`, and no `--dovetail`, since HISAT2 has no such option); minimap2; single-end (5-Base is PE-only).
- **Edge cases:**
  - `--old_flag` is a deferred no-op on this path, so the bisulfite `--old_flag`/`--dovetail` conflict does not apply.
  - `-X` smaller than `-I` gets no new validation.

### 2. Pre-registered confirmation (`PREREGISTRATION.md`)

Committed before any B-new cell runs, and after the builder change and its tests are committed. The commit is recorded.

**Arms**, all on the same build:

| Arm | Flags | Options it must show | Role |
|---|---|---|---|
| B-old | `--bowtie2 -X 800 --no_dovetail --five_base_umi_qname` | `--maxins 800`, no `--dovetail` | old behaviour, by its alignment metrics |
| B-new | `--bowtie2 --five_base_umi_qname` | `--dovetail` and `--maxins 800` exactly once each, and none of `--local`, `--no-mixed`, `--no-discordant` | the shipped default |

- The options are verified from the report's options line.
- For B-new, `@PG` must show `--bowtie2` and must **not** show `-X`, `--dovetail` or `--no_dovetail`.
- `--five_base_umi_qname` writes the dual UMI from the read name (e.g. `…:1098:CAGTAGT+CACCTAC`, confirmed in both samples' raw FASTQs) into `RX:Z:`, which the duplex pass needs.
  - It changes no alignment or metric, so the B-old reproduction still applies.
  - The harness asserts that `RX` is present on the first 1,000 records of each BAM, and that the trimmed FASTQs keep the `A+B` name field.

**Samples:** NA12878 and HG002, using the same 100M-pair screen subsets as #1146.

**Reproduction checks.** A failure stops the work.

1. **Tool versions match #1146 and are recorded:**
   - Trim Galore 2.2.0 (Oxidized), Bowtie 2 2.5.5, cutadapt 2.6, samtools 1.23.1;
   - the Rust dedup and extractor from the same build.
2. **Trimmed reads match.** `zcat sub_R*_val_*.fq.gz | md5sum` must equal #1146's plain-`.fq` md5s:
   - NA12878 R1 `337ffa049ef5cff27fa5e30a555eb50a`, R2 `5229b07724d84db29bc6ae9b36276649`;
   - HG002 R1 `d055be973c675edbeee5897aa7cc6341`, R2 `f0770c478ceb160f5d50edeec7619bf8`.
3. **NA12878 B-old reproduces #1146 B-X on every committed metric:**
   - pairs_in, unique_pairs, map_eff, cpg/chg/chh %, dup %;
   - records, mapq0, r1_records, readthrough, TLEN p10/p50/p90;
   - deconvolution precision and recall, with their numerators and denominators;
   - own-set cov ≥ 10 n 822,432 and r 0.965834, from scoring B-old alone against DRAGEN.

   HG002 has no Bowtie 2 baseline, so its B-old is a new baseline.

**Primary check: the rescued pairs.**

*Sets.* Each is computed per sample on the raw, pre-dedup `_pe.bam` of each arm. All counts are of pairs or of XM calls on both mates, with no overlap removal, applied identically to both sets.
- **Rescued:** read names that are proper pairs in B-new and absent from B-old.
- **Changed:** names in both arms whose POS or CIGAR differ. Reported only.
- **Rest:** B-new pairs that are neither rescued nor changed.

*Geometry*, in reference coordinates.
- Let F be the forward mate (flag `0x10` unset), with span [Fs, Fe], and R the reverse mate, with [Rs, Re].
- A pair is **dovetailed** if Rs < Fs or Fe > Re.
- A pair is **contained** if one span lies wholly within the other without dovetailing. Contained pairs are counted separately and are not part of the overhang analysis.
- **Overhang bases** are F's bases at positions > Re plus R's bases at positions < Fs.

The overhang-length distribution (p10/p50/p90/max) and the counts of dovetailed and contained pairs are reported for every set.

*Criteria:*
- **R1.** The rescued pairs are between 0.1% and 3% of B-new's raw pairs. Outside that range, stop and investigate: the mechanism differs from the hypothesis.
- **R2.** The rescued set's CHG and CHH methylation are each ≤ the rest's + 0.5 pt.
- **R3.** The rescued set's CpG methylation is within ±5 pt of the rest's.
- **R4.** Overhang bases against cycle-matched controls. A cycle is the read position counted from the read's 3′ end. Controls are non-overhang bases at the same cycles, from rest pairs.
  - CHG/CHH methylation of overhang bases ≤ the control's + 1.0 pt.
  - Mismatch rate of overhang bases ≤ 2× the control's. A mismatch here is an `MD` mismatch at a position without an XM methylation call. In 5-Base, `NM`/`MD` count every called C→T/G→A, so called positions are excluded.
  - Each comparison needs n ≥ 10,000 overhang bases. Below that, the result is "insufficient", and the run stops for review.
- **R5.** Per-call agreement with DRAGEN at sites where DRAGEN has cov ≥ 10.
  - For each CpG call, the score is p if the call is methylated and 1 − p if not, where p is DRAGEN's methylated fraction at that site.
  - The rescued set's mean score must be ≥ the mean score of a random rest sample of equal size (seed 1146) − 0.02.
  - It needs n ≥ 10,000 calls.
- **R6.** M-bias, rescued vs rest. CpG methylation by R1 cycle from the 3′ end, averaged over the last 10 cycles, must be within ±3 pt between the two sets.

**Secondary check: genome-wide.** B-new vs B-old on their own common set with DRAGEN, computed from full-precision values:
- common-set Δr ≥ −0.001;
- CHG and CHH ≤ B-old + 0.2 pt;
- MAPQ-0 fraction ≤ B-old + 2 pt;
- deconvolution precision and recall each Δ ≥ −1 pt.

**Not deciding:**
- The gain-set r guard (cov ≥ 10 sites that B-old lacks) is not used: its sites sit at cov 10–11, which lowers r by construction, as #1146 showed. Its value is still reported.
- ΔCpG, Δmapping and the changed-set size are reported.

**Duplex/consensus pass, NA12878.**

*Invocation.* `bismark --illumina_5base --genome G --five_base_umi_qname --five_base_emit_multiplicity both --five_base_consensus_from_bam <arm raw _pe.bam>`, separately for B-old and B-new.
- `--five_base_min_mapq` is left at its default of 0 for both arms, and the value is recorded.
- `both` makes the tool report duplex and simplex family counts, writing `.5base_consensus.bam` and `.5base_simplex.bam`.

*Reported:* duplex families, simplex families, consensus records, and per-strand mean CpG methylation of the duplex consensus compared with DRAGEN (the GA-contract basis).

*Family-split check (failable).* A small helper recomputes each pair's family key the way `five_base_duplex.rs` does: outer fragment span, strand and `canonical_umi` of `RX`. It then reports, for the rescued set and for the rest, the fraction of pairs whose family has a partner on the opposite molecule strand.
- The helper is validated first: its duplex-family count over all of B-new must match the tool's to within 0.1%. Otherwise the helper is wrong and the run stops.
- **D1.** The rescued set's partnered fraction must be ≥ the rest's − 5 pt.

*Consensus criteria:*
- **D2.** Consensus mean CpG methylation is within ±0.5 pt of B-old's.
- **D3.** Consensus CHG/CHH ≤ B-old's + 0.2 pt.

*Not deciding:* B-new's duplex-family count compared with B-old's. B-new's pairs are close to a superset of B-old's, so a rise is expected; it is reported only.

**Outcome.** The PR proceeds only if every criterion passes: R1–R6 and the secondary checks on both samples, and D1–D3 on NA12878. If any fails, stop and bring the numbers to Felix. Nothing is tuned or dropped silently.

### 3. Docs and CHANGELOG

- **Tuning section:** state the 5-Base Bowtie 2 defaults (`--dovetail`, `--maxins 800`). Note that `-X` and `--no_dovetail` change them, without promising byte-identical old output.
- **"Choosing an engine" table:** relabel its Bowtie 2 row "Bowtie 2 (before this change; 500 bp cap)" and add a row for the new default from NA12878 B-new.
- **CHANGELOG:** one sentence.

## Implementation outline

1. **Tests first, then the builder.**
   - **`options.rs` units:**
     - the exact default builder string `-q --score-min L,0,-0.6 --dovetail --maxins 800` (Bowtie 2);
     - explicit `-X 500` and `-X 1200`;
     - `-I 600` alone;
     - `--no_dovetail`;
     - explicit `--dovetail` (emitted once);
     - `--dovetail --no_dovetail` (none);
     - `--old_flag` (accepted);
     - `--local` exact string;
     - HISAT2 has no default `--maxins`/`--dovetail`;
     - minimap2 unchanged.
   - Update `five_base_defaults_and_explicit_defaults`, `five_base_forwards_tuning_in_bisulfite_token_order`, `five_base_omits_bisulfite_policy_and_runtime_threads` (which now allows `--dovetail` for Bowtie 2) and the `mod.rs` stub.
   - **`aligner_cli.rs`:** the exact default argv for Bowtie 2; tuning and threads exact strings. Check that the fake-engine stubs accept the new tokens.
   - **`aligner_five_base_bowtie2.rs`** (real Bowtie 2):
     - a 1,000 bp fragment is excluded at the default and admitted with `-X 1200`;
     - an explicit `-X 500` excludes a 600 bp fragment;
     - dovetailed pairs in **both** orientations (OT and OB) are kept by default and dropped with `--no_dovetail`, asserting the proper-pair flag, TLEN sign and size, and XM length.
   - Implement the builder change, with one-line comments.
   - Run fmt, the focused tests and release Clippy. Commit, and record the commit hash.
2. **Harness:**
   - Redefine the #1146 arms with explicit flags so they keep their meaning (B: `-X 500 --no_dovetail`; B-loc and B-X: add `--no_dovetail`).
   - Add B-old and B-new with the option and `@PG` checks from Behavior 2, plus self-test cases. Each new token must appear exactly once.
   - Add a version-recording step.
   - Add a `rescued` subcommand: a stdlib Python helper (`validation/five_base_rescued.py`) reading `samtools view` output.
     - It builds the rescued, changed and rest sets.
     - It computes the geometry, the overhang distribution and R1–R6 as defined in Behavior 2.
     - It recomputes duplex family keys for D1, with the tool cross-check.
     - Its self-test uses a hand-built SAM with known dovetailed, contained and normal pairs, XM calls, MD mismatches at called and uncalled positions, and `RX` UMIs including a swapped partner pair.
   - Add the duplex pass to the harness (Behavior 2 invocation) and an `RX`-presence assertion per BAM.
   - Keep each cell's M-bias file.
   - Large outputs (BAMs, CpG reports, consensus BAMs) stay on scratch, `/var/tmp/5base_engines` (Felix, 2026-11-02). As each cell finishes, copy its small results (metrics, scores, reports, M-bias, logs) to `~/5base_results`.
3. **Write and commit `PREREGISTRATION.md`** (Behavior 2) before any B-new cell runs.
4. **Stage on oxy:**
   - Create real directories `/var/tmp/5base_engines/{NA12878,HG002}/{fastq,dragen}` and symlink each file from the S3 mount.
   - Build Bismark at the commit recorded in step 1.
   - Record the tool versions (reproduction check 1).
5. **10M dry run:** NA12878 B-old and B-new at 10M pairs, plus the `rescued` and duplex steps. It checks the harness end to end:
   - `RX` is present;
   - the duplex helper matches the tool's family count to within 0.1%;
   - the R4/R5 minimum-n figures are projected to 100M pairs.

   No decision is taken from it.
6. **Screen:**
   - prep both samples, then reproduction check 2;
   - NA12878 B-old, then reproduction check 3;
   - B-new for NA12878, and B-old and B-new for HG002, as two concurrent streams with timings excluded;
   - the `rescued` analysis per sample;
   - the duplex pass on NA12878;
   - scoring;
   - `RESULTS.md`, applying the pre-registered criteria by script rather than by hand.
7. **Present the result to Felix.** On a pass: docs and CHANGELOG, then fmt, `--lib aligner::`, `aligner_cli`, `aligner_five_base_bowtie2`, release Clippy and the full suite, then dual code review, the plan-manager audit, and the PR into `dev`. On a fail: stop.

## Efficiency

- **Staging and trimming:** about 1 h per sample, plus a 10M dry run of about 1 h.
- **Bowtie 2 cells:** four of about 2.5 h, in two streams, so about 5–6 h of wall time.
- **Rescued-set and duplex steps:** about 1 h each.
- **Disk:** about 30 GB per cell on scratch, and scratch (about 1 TB) holds all four cells. Only small results go to `/home`, which has about 8 GB free. If an idle restart wipes scratch, re-run the affected cells from the S3-mounted inputs.
- **Run-time cost of the change:** small. In #1146 the aligner CPU was 6% higher for B-X than for B.

## Integration

**Changes:** 5-Base Bowtie 2 output gains more proper pairs. That is a user-visible behaviour change, recorded in the CHANGELOG.

**Unchanged:** bisulfite paths and the Perl oracle; 5-Base minimap2 and HISAT2.

**Results:** committed under this plan's `bench_results/`. Large outputs are kept on oxy's scratch until the PR merges.

## Assumptions

- 800 bp matches minimap2's `-F` default and covers the TLEN distribution (p90 ~478 bp).
- The S3 mount holds the same bytes as the #1146 inputs; reproduction check 2 confirms it.
- Read names are stable across arms and runs; Bowtie 2 runs with `--reorder`.
- `--five_base_consensus_from_bam` gives the same families as an in-pipeline consensus run. The two B-old and B-new runs are compared like for like.

## Validation

1. **Reproduction:** tool versions, trimmed-read md5s, and the full NA12878 B-old metric match.
2. **Options:** the report options for both arms, B-new's `@PG` absence check, and the harness self-test.
3. **Rescued-pair checks** R1–R6.
4. **Genome-wide secondary checks.**
5. **Duplex/consensus checks.**
6. **Unit, CLI and real-Bowtie 2 tests** as listed, written and passing before the run.
7. **No collateral change:** bisulfite option-string tests and the Perl-oracle CI stay green.
8. **`rescued` helper self-test** on a hand-built SAM containing known overhangs, XM calls and MD strings.

## Questions or ambiguities

Resolved by Felix:
- **Q1** (S3 symlinks), **Q2** (`--dovetail` is a default), **Q3** (both samples) and **Q4** (HISAT2 out of scope): 2026-11-01.
- Duplex/consensus pass included; one PR: 2026-11-02.

Open: none.

## Self-Review

- **Logic:**
  - The primary check looks at the ~0.7% of pairs that change, so genome-wide dilution can't hide a defect.
  - The gain-set guard that failed by construction in #1146 is reported but doesn't decide.
  - B-new tests the shipped default directly.
- **Fixed from review:**
  - the token order now follows the bisulfite builder;
  - there is no "restore the old output" promise;
  - the reproduction check covers every metric;
  - the `@PG` check is adapted to defaults;
  - the #1146 arms are redefined;
  - tests come before the run;
  - there is a dry run;
  - outputs are copied off the scratch disk;
  - the build is pinned.
- **Remaining risks:**
  - The R1–R6 thresholds are judgement calls, fixed before any data are seen.
  - Duplex is checked on one sample only.
  - The run covers WGS gDNA, not cfDNA.

## Implementation notes

**Commits** on `feat/5base-bowtie2-defaults`:

| Commit | What |
|---|---|
| `72bbf7b` | builder change and tests (outline 1); the binary for the run |
| `909182d` | harness and `five_base_rescued.py` (outline 2) |
| `1284243` | plan folder and `PREREGISTRATION.md` (outline 3) |
| `a9ee747` / `d6f66ae` | version-check fix; pre-registration repointed to it. No cell had run. |
| `9549d7a` | executable mode on the bench script; mode only, no content change |

**Tests (outline 1).**
- `options.rs` has a new `five_base_bowtie2_defaults_to_dovetail_and_maxins_800` covering every listed case, and the existing 5-Base tests are updated.
- `aligner_cli.rs` has a new `five_base_bowtie2_dovetail_and_maxins_defaults_reach_argv`; the exact default argv and the tuning test are updated.
- `aligner_five_base_bowtie2.rs` replaces the 800 bp test with two real-Bowtie 2 gates:
  - `default_max_insert_is_800`: 1,000 bp excluded at the default and admitted with `-X 1200`; 600 bp admitted at the default and excluded with `-X 500`;
  - `keeps_dovetailed_pairs_unless_no_dovetail`: OT and OB, asserting flags, TLEN ±120 and XM length.
- Results: `--lib aligner::` 613, `aligner_cli` 140, the four 5-Base suites, release Clippy and fmt all pass.

**Deviations and decisions.**
- **Consensus output names:** `--five_base_consensus_from_bam` writes `five_base_consensus.bam` and `five_base_simplex.bam` into `-o`, not `<stem>.5base_consensus.bam`.
- **Duplex family counts:** with `both`, the tool prints no duplex-family count directly. The harness uses duplex = emitted + skipped (one per family) and takes simplex families from the simplex line.
- **D0:** the helper-vs-tool check is listed as its own criterion, D0.
- **Random subsets of rest pairs:** the R4 control is the rest pairs with a seeded draw < 0.01, and the R5 sample is drawn from those with a draw < 0.05. Per-base work on all ~99M rest pairs would be too slow in Python; the subsets are fixed in `PREREGISTRATION.md`.
- **Dovetail definition:** taken literally from the plan, so F containing R with Fe > Re counts as dovetailed; its F bases past Re are overhang. The self-test pins this case.
- **Small results** go to `~/5base_results_bt2`, not `~/5base_results`, so #1146's copies there (including its `score.tsv`) are not overwritten.
- **Code to oxy:** sent as git bundles over `dcli ssh`, not pushed. The harness lives in `~/5base_harness_bt2`, and #1146's committed B-X results in `~/5base_ref_1146/screen`.

**Iteration log.**
- #1 `options.rs` self-review: none needed; the focused tests passed first time.
- #2 helper self-test: the "contained" fixture had F ending past R. Under the plan's definition that pair is dovetailed, so the fixture was corrected and the case pinned.
- #3 oxy launch:
  - the archive dropped the executable bit;
  - the driver PATH lacked `samtools`;
  - the Bowtie 2 stderr warning broke `versions`.

  All three were fixed before any cell ran.
- #4 dry run, `duplex`: the `--five_base_min_mapq` default check read `bismark --help`, which is the suite overview, instead of `bismark align --help`. Fixed in `f3e860f`, a one-word sanity-check change with no criterion touched.
  - It landed after the dry-run B-new cell had produced data. `PREREGISTRATION.md` is left unchanged, so it still names `a9ee747`; this note records the difference.
  - The dry run resumed from `duplex`, with its cells and `rescued` kept.
  - Dry-run figures so far:
    - B-old mapping 94.3%, B-new 96.3%;
    - rescued 194,825 of 9,432,773 B-new pairs (2.07%), changed 17,996, lost 3.
- #5 dry-run verdict (NA12878 10M, scored with `MIN_JOINED=0.2` for the low depth; informational only). It shows a strong harm signal from `--dovetail`.
  - The harness worked end to end: D0 found 1,179 duplex families vs the tool's 1,179.
  - Rescued pairs are almost all dovetailed (194,718 of 194,825), with a median overhang of 12 bp (p90 15).
  - The overhang bases are not genomic:
    - mismatch rate 35.4% vs 0.61% in cycle-matched controls (n 2.49M);
    - CHG 26.1% vs 1.9%;
    - CHH 22.0% vs 1.4%.
  - The rescued set as a whole: CHG 2.13% vs rest 1.30%, CHH 1.90% vs 1.16%, and R1 3′ M-bias CpG 27.6% vs 50.8%. Its per-call DRAGEN agreement is 0.761 vs 0.784.
  - Failed: R2, R4, R5 and R6. R1, R3, every genome-wide secondary check and D0–D3 pass.
  - This is the dilution the plan predicted: the genome-wide checks cannot see the harm.
  - **Stopped before any screen B-new cell, and brought to Felix.** The B-old streams (NA12878 reproduction, HG002 baseline) keep running.

## Revision history

- **2026-11-01 rev 0:** initial plan.
- **2026-11-01 rev 1:** Felix's Q1–Q4 answers.
- **2026-11-02 rev 2:** both plan reviews applied (`PLAN_REVIEW_reviewer-A.md` / `-B.md`), plus Felix's choices (duplex/consensus pass, one PR).
  - primary check on the rescued pairs (R1–R6) and a dovetail-shape count;
  - gain-set guard demoted;
  - full-metric reproduction and recorded tool versions;
  - token order corrected;
  - TLEN/`--no_dovetail` caveat;
  - `@PG` check for defaults;
  - #1146 arms redefined;
  - tests and commit before the run;
  - 10M dry run;
  - off-scratch copies built into the harness;
  - duplex/consensus via `--five_base_consensus_from_bam`.
- **2026-11-02 rev 3:** quick recheck applied (`PLAN_REVIEW_R2.md`).
  - **N1:** `--five_base_umi_qname` on both arms, so `RX` exists for the duplex pass; the dual UMI is confirmed in the raw read names.
  - **N2:** duplex run with `--five_base_emit_multiplicity both` on the raw pre-dedup BAMs, `--five_base_min_mapq` fixed at 0.
  - **N3:** a failable rescued-vs-rest family-partner check (D1), with a helper cross-checked against the tool.
  - **N4:** R4 excludes XM-called positions from mismatches, compares cycle-matched controls and needs a minimum n.
  - **N5:** reference-coordinate definitions of dovetail, contained and overhang.
  - **N6:** R5 rewritten as per-call agreement against a matched rest sample.
  - **N7:** R6 compares rescued with rest.
  - **Optional items:** set denominators and dedup state named, changed-set reported, full precision, overhang distribution reported, builder-string vs argv distinction.
- **2026-11-02 rev 4:** the dry run showed that the `--dovetail` overhangs are non-genomic, so Felix chose `--maxins 800` only. Validation becomes the NA12878 reproduction of #1146 B-X plus HG002 against `-X 500`. Q5 was resolved by Felix the same day: an explicit `--dovetail` stays a no-op.
- **2026-11-02 rev 5:** both rev 4 plan reviews applied.
  - harness re-aimed: `repro` takes an arm; separate `bin5`; `--five_base_umi_qname` dropped; B-loc fixed; duplex code removed;
  - HG002 admitted-pair checks A1–A6;
  - `-I` above the cap fails early;
  - the dovetail test gets a control pair and pins Q5;
  - docs edits specified;
  - PREREGISTRATION rev 2 disclosure.
- **2026-11-03 rev 6:** both rev 5 reviews applied (`PLAN_REVIEW_R5_A/B.md`).
  - Sampling and pools: per-pair draws for every B-new pair; the equal-size A5 sample replaced by a matched-site comparison; a 10% admitted sample for the per-base checks.
  - Criteria: an A2 mechanism check on |TLEN|; A7 as a within-read 3′ shift.
  - Harness: per-arm `repro` outputs plus `repro-compare`; a separate harness directory; `cell` refuses B-old; disjoint CPU sets.
  - Tests: explicit-`-X` `-I` cases; the missed test and two renames.
  - Plan text: the declined rev 4 items listed with reasons; the `rust/README.md` row added.

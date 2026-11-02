# 5-Base Bowtie 2 defaults: `-X 800` and `--dovetail`

## Goal

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

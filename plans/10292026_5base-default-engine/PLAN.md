# Settle the Illumina 5-Base default aligner (benchmark, then decide)

## Goal

Decide which engine `--illumina_5base` should use by default: minimap2 (current), Bowtie 2 or HISAT2. The decision comes from a comparison against DRAGEN whose rules are fixed before any challenger runs. Then open one PR into `dev` that records the decision and the numbers behind it.

The PR takes one of three forms:
- **(a)** keep minimap2, with an evidence-backed reason in the docs;
- **(b)** keep minimap2 and fix a minimap2 configuration problem the benchmark exposes;
- **(c)** move to the winning engine by the non-breaking route in Behavior 9, approved by Felix at decision time.

- **Worktree:** `/Users/fkrueger/Github/Bismark-5base-default`, branch `feat/5base-default-engine`, at `dev` `9b5c15b` (includes #1138, the #1134 tuning options).
- **Machine:** oxy (Felix, 2026-10-29).

## Context

- **Where the default lives:** `rust/bismark/src/aligner/config.rs::resolve_aligner` (~line 1319).
  - `--illumina_5base` with no engine flag returns minimap2.
  - `--bowtie2`/`--hisat2` require `--five_base_index`; `--five_base_index` without an engine flag is an error today.
  - The default is pinned by `illumina_5base_resolves_to_minimap2` (`config.rs:2305`).
  - The code comment calls 5-Base "a minimap2 run (v1)". minimap2 was the only engine in #787, so the default reflects history, not a comparison.
- **Engine options at `9b5c15b`:**
  - Bowtie 2/HISAT2: `-q --score-min L,0,-0.6` (+ HISAT2 `--no-spliced-alignment`) `-p N --reorder`, plus forwarded tuning.
  - Defaults that matter for the comparison:
    - Bowtie 2 aligns end-to-end, so it does not soft-clip.
    - HISAT2 soft-clips.
    - Both cap inserts at 500 bp.
    - 5-Base omits `--dovetail`.
  - minimap2 (`options.rs:318`): `-a --MD --secondary=no -t N -x sr -K 250K`. The `-K 250K` batch is much smaller than the sr preset's default.
- **Filtering in the 5-Base driver** (`mod.rs` around `five_base_emit_pe_record`):
  - Any proper pair is emitted, including MAPQ 0 (`--five_base_min_mapq` applies only to duplex/consensus).
  - Each engine sets the proper-pair flag its own way.
- **The published numbers came from default minimap2.** `validation/VALIDATION_REAL_DATA.md` records it:
  - NA12878 Sample8, 8 lanes, about 384M pairs: r 0.9878 at cov ≥ 10 without dedup, 0.998 with `deduplicate_bismark -p`.
  - HG002 Sample40: r 0.996 at cov ≥ 10; deconvolution 94.3% / 95.7% (NA12878 90.3% / 93.4%).
  - The NA12878 download was partial (one R1 `.part`). That explains why the 384M pairs it used sit below DRAGEN's ~438–490M.
  - The HG002 run used an Ensembl-named genome, NA12878 a UCSC-named one.
- **Harness:**
  - `validation/fetch_dragen.sh` (DRAGEN references from BaseSpace project 471431965): NA12878 DRAGEN dataset `ds.258e74420ab8417a89de572ec1571b55`, HG002 `ds.48b7596730dd47ef97699b59ccd3641d`. Raw FASTQs are separate `illumina.fastq.v1.8` datasets.
  - `validation/concordance.py methyl|deconv` scores each input against DRAGEN on its own intersection. It strips `chr` prefixes and has `--selftest`.
- **oxy (re-verified 2026-10-29):**
  - 128 logical CPUs, ~1 TB RAM.
  - `/home` 99 GB persistent with only 8 GB free; `/` overlay 790 GB free, ephemeral (wiped if the pod recycles, which also kills running jobs).
  - Tools in `~/micromamba/envs/bismark-test/bin`: bowtie2, hisat2, minimap2, samtools, Perl `deduplicate_bismark`, Trim Galore, cutadapt; cargo is in `~/.cargo/bin`.
  - BaseSpace CLI 1.7.1 installed at `~/bin/bs`; authentication is pending Felix's browser approval.
  - Genome on hand: `~/bismark_benchmarks/genome/Homo_sapiens.GRCh38.dna.primary_assembly.fa` (Ensembl names).
  - Other Claude sessions share oxy, so the idle gate (`scripts/oxy_idle_gate.sh`) runs before any timed cell.

## Behavior

### 1. Frozen decisions (committed to `PREREGISTRATION.md` before any challenger runs)

Whatever is in this file is what the decision follows. Changing it after a challenger has produced data voids the run.

**Arms** (engine defaults unless stated):
- **A:** minimap2.
- **B:** Bowtie 2 end-to-end.
- **B-loc:** Bowtie 2 `--local`. This controls for soft-clipping, now possible after #1138.
- **B-X:** Bowtie 2 with `-X 800`. This matches minimap2's `-F` cap, which is to be confirmed from the installed binary and recorded; it controls for the 500 bp insert cap.
- **C:** HISAT2.

B-loc and B-X are diagnostic controls that explain gaps between engines. They can still be the winning challenger; the docs would then state the switch as "Bowtie 2 with option X".

**Decision rule** (Felix accepted these thresholds, 2026-10-29):
- Switch to challenger X only if all of the following hold on both samples at full depth:
  - Common-set r at cov ≥ 10 is non-inferior: Δr ≥ −0.001.
  - CpGs at cov ≥ 10 are non-inferior: ΔCpG ≥ −1%.
  - X is clearly better on one of those two: Δr ≥ +0.002 or ΔCpG ≥ +2%.
  - X passes the specificity guards below.
  - Deconvolution precision and recall are each Δ ≥ −1 pt.
- Δr ≥ +0.002 is practically unreachable from a baseline near 0.998. A switch therefore effectively rests on ΔCpG ≥ +2%, and the specificity guards stop over-mapping from earning it.
- Common-set mean |Δ%| and mean signed Δ% are reported next to r but do not decide.
- Cost (wall time, RSS) breaks ties only.

**Specificity guards** (on any CpG gain):
- The challenger's MAPQ-0 fraction is no more than arm A's + 2 pt.
- Apparent CHG and CHH methylation are each no more than arm A's + 0.2 pt.
- The CpGs it gains, scored against DRAGEN, have r ≥ (common-set r − 0.02).

**Choosing the challenger:** from the screen, the arm with the largest ΔCpG among those that are non-inferior on r and pass the guards. Ties go to the larger Δr, then to B over C, because plain Bowtie 2 is the simpler default.

**Outcome (b):** if the only qualifying challenger is B-X or B-loc, and its advantage is explained by a minimap2 configuration gap (for example the `-K` batch or the insert cap), outcome (b) is considered before any switch.

### 2. Prerequisites (fail fast; each recorded in RESULTS.md)

1. Build Bismark from `9b5c15b`; record the commit and `bismark --version`. Record the minimap2, Bowtie 2, HISAT2, Trim Galore and cutadapt versions, and minimap2's `-F` default.
2. **One reference for both samples.**
   - Contigs: GRCh38 primary assembly, autosomes and X, plus the lambda/pUC19 controls if DRAGEN's reference includes them.
   - Naming must match DRAGEN's CX report after `concordance.py`'s `chr` normalisation.
   - Use the same FASTA to build the Bowtie 2 and HISAT2 indexes, recording build time and peak RSS as one-off costs. Bismark does not check that an index matches `--genome`.
   - **Reference check:** at least 99.9% of the CpG positions in DRAGEN's CX report must also be CpGs in our FASTA. Sample 1M positions, report the fraction and abort below the threshold.
3. **Input.**
   - Download each sample's full FASTQ datasets. Verify every lane has both mates, with matching read counts and no `.part` files. Record md5s.
   - Compare total input pairs against DRAGEN's `methyl_metrics.csv` total, and record the ratio.
   - Every arm uses the **same trimmed input**: Trim Galore in paired mode with default adapter auto-detection, run once per sample. Record its report.
   - Trimming removes the soft-clipping advantage of untrimmed read-through, so it matches the normal workflow.
4. **Screen subset.** Take the first ~100M pairs from the trimmed input, deterministically by lane order. Record the exact lanes and pair count. Every screen arm reads the same files; verify by md5.

### 3. Per-arm pipeline (identical for every arm)

1. `bismark --illumina_5base <engine flags> --five_base_deconvolution -p 32`.
   - Run under `taskset` on 32 physical cores on one NUMA node.
   - Wrapped in a process-tree peak-RSS sampler (`scripts/bench_run.sh` style) with `/usr/bin/time -v`.
   - The aligner process's CPU time is recorded separately, because the single-threaded Bismark 5-Base driver can bound the fastest engine.
   - Deconvolution runs in the same alignment: it has no from-BAM mode.
2. `deduplicate_bismark -p`, the same as the published methodology.
3. The methylation extractor with a CpG-only cytosine report (smaller than CX).
4. Three `concordance.py methyl` runs against DRAGEN:
   - the arm's own set (for comparability with the published numbers);
   - the **common set**, the CpGs covered at cov ≥ 10 by every compared arm and by DRAGEN, produced by a small filter step;
   - the **gain set**, the challenger's cov ≥ 10 CpGs that arm A lacks.
5. `concordance.py deconv` against the DRAGEN VCF.

### 4. Option verification (per arm, before scoring)

Each arm's report header and BAM `@PG` must show:
- the expected engine;
- `-p 32` (or `-t 32` for minimap2);
- `--reorder` for Bowtie 2/HISAT2;
- the arm's extra options (`--local`, `-X 800`);
- HISAT2's single `--no-spliced-alignment`.

A mismatch fails the arm.

### 5. Metrics per arm

**Accuracy:**
- r, coverage-weighted r, mean |Δ%|, mean signed Δ% and call agreement at cov ≥ 1/5/10;
- on the own set, the common set and the gain set.

**Sensitivity:**
- CpGs at cov ≥ 10;
- per-contig joined counts;
- the fraction of DRAGEN CpGs joined.

**Read fate:**
- mapping efficiency and proper-pair rate;
- the MAPQ-0 fraction;
- the duplicate rate;
- the fraction of pairs whose insert is shorter than the read length (read-through);
- TLEN quantiles.

**Specificity:** apparent CHG/CHH methylation.

**Cost:**
- wall time;
- peak RSS for the whole process tree;
- aligner CPU time;
- minimap2's per-run index build, included in its wall time because users pay it on every run and also reported separately;
- the one-off Bowtie 2/HISAT2 index builds.

### 6. Design: screen, then confirm

**Screen**, all arms, ~100M pairs:
- NA12878: arms A, B, B-loc, B-X and C.
- HG002: arms A and the leading challenger from NA12878, so a sample-specific effect is caught early.
- The screen only chooses the challenger; it never switches the default.

**Confirm**, full depth: arm A and the challenger on both samples.
- If no challenger passes the screen, do not confirm. Outcome (a) is decided from the screen, and the PR says so.

### 7. Robustness

**Run bookkeeping:**
- One output directory per sample, arm and depth.
- A `DONE` marker keyed on the commit, the arguments and the input md5.
- An existing marker skips the cell only if all three match.

**Results off-box:**
- After each cell, copy `metrics.tsv`, the logs and the concordance outputs to `/home` (small).
- Copy them into this plan's `bench_results/` on the laptop, because a pod recycle wipes `/`.

**Disk management:**
- BAMs and cytosine reports stay on `/` until the PR merges.
- If the free space on `/` falls below 150 GB, the harness stops before starting a new cell.

**Pod recycles:**
- A recycle that wipes inputs means re-download and re-trim; the md5s confirm identity.
- An arm interrupted by a recycle is re-run, never dropped.

**Guards that abort a cell:**
- the joined DRAGEN fraction is < 50%;
- any per-contig count is zero for a contig DRAGEN covers;
- the input read counts differ between arms of the same depth.

### 8. Outcomes (a) and (b): minimap2 stays

**Docs:**
- Name minimap2 as the engine behind the published numbers.
- Add a short engine-comparison table: common-set r and CpGs at cov ≥ 10, wall time, RSS.
- Revise the "when to choose `--bowtie2`/`--hisat2`" sentence to match the data.

**CHANGELOG:** one sentence.

**Tests:** the existing `illumina_5base_resolves_to_minimap2` already pins the default; no new pin test.

**Outcome (b) only:**
- Change the minimap2 option (for example `-K`), with a unit test.
- Re-run arm A on the screen to confirm the gain.

**Fix in either case:** the `mod.rs:223` startup notice says minimap2 even for `--bowtie2`/`--hisat2`. Make it engine-correct.

### 9. Outcome (c): the default moves (Felix approves at decision time)

**Non-breaking route (recommended):**
- `--five_base_index` without an engine flag selects the winning engine (Bowtie 2 or HISAT2). That combination is an error today, so no existing command changes meaning.
- Plain `--illumina_5base` with a FASTA-only genome keeps running minimap2.
- The docs recommend the `--five_base_index` path.
- Changes:
  - `resolve_aligner`, its errors and `--help`;
  - a unit test that `--five_base_index` alone selects the engine;
  - the existing pin test, kept for the FASTA-only path;
  - the startup notice;
  - the docs;
  - a CHANGELOG entry.

**Breaking route (only if Felix prefers it):** plain `--illumina_5base` switches engine. The PR must also handle:
- output filenames (`_bismark_mm2_pe` becomes `_bismark_bt2_pe`);
- `--mm2_*` flags with bare `--illumina_5base`;
- the FASTA-only CI control gates in `aligner_five_base_groundtruth.rs`;
- error messages pointing to `--minimap2`;
- the stability-contract text;
- a `**Breaking:**` CHANGELOG entry.

Consensus and duplex modes are covered by the stability contract and are not measured here. For either route, the contract must say which engine their basis was validated on.

No release cut is proposed while the work is in flight.

## Implementation outline

1. **Authenticate BaseSpace on oxy** (pending Felix's browser approval).
   - List the project's FASTQ datasets for Sample8 and Sample40.
   - Download them and the DRAGEN references (`fetch_dragen.sh`) to `/var/tmp/5base_engines/`.
   - Verify completeness and md5s (Behavior 2.3).
2. **Write `PREREGISTRATION.md`** with Behavior 1 verbatim. Commit it to the branch before any challenger cell runs.
3. **Write `validation/five_base_engine_bench.sh`**, POSIX sh like `fetch_dragen.sh`. It:
   - stages and trims;
   - runs the per-arm pipeline with verification and guards;
   - runs the common/gain-set filter (a small `validation/five_base_common_set.py`, stdlib-only);
   - writes `metrics.tsv`.
   - Test it on the 10M-pair subset first; see Validation.
4. **Reference and indexes** (Behavior 2.2).
5. **Run the screen** (Behavior 6), then present the screen table and the chosen challenger to Felix, and continue to the confirm runs unless he stops it.
6. **Run the confirm runs.** Write `RESULTS.md` with the tables, the rule applied, and caveats: both samples are GIAB WGS libraries of one protocol; no cfDNA.
7. **Stop and present the decision to Felix.** The code and docs change (Behavior 8 or 9) is implemented only after the implementation trigger.
8. **Implement.** Then run fmt, the focused tests, release Clippy, the full suite, dual code review and the plan-manager audit. Open the PR into `dev` with one short paragraph and an AI-assisted `<details>` block.

## Efficiency

**Screen:** 7 cells at ~100M pairs. At 32 cores that is roughly 1–2 h of alignment per cell, plus dedup and extraction.

**Confirm:** 4 cells at full depth, at ~438–490M pairs.

**Cells are sequential**, so timings don't compete. Accuracy is independent of timing, so if oxy is busy, screen cells may run two at a time on separate NUMA nodes. Cells run that way are marked, and their timing is excluded.

**Disk:**
- FASTQs: about 150–200 GB per sample, compressed, at full depth.
- Trimmed copies: about the same again.
- BAMs plus CpG reports: about 50–80 GB per full-depth cell.
- Total peak: about 700 GB, close to `/`'s 790 GB free. Mitigation: delete each sample's untrimmed FASTQs once trimming is verified (md5s kept, re-downloadable). The 150 GB guard in Behavior 7 enforces headroom.

## Integration

**Inputs:** BaseSpace project 471431965 (FASTQs, DRAGEN CX/VCF/metrics), one GRCh38 FASTA and its indexes.

**Committed to the branch:**
- `PREREGISTRATION.md` and `RESULTS.md` (plan dir);
- the harness and the common-set filter (`validation/`);
- the docs and CHANGELOG;
- for (b): the minimap2 options change and its test;
- for (c): the `resolve_aligner` changes and their tests;
- the startup-notice fix.

**Unchanged:** bisulfite behaviour and the Perl oracle. Under (a) and (b), plain `--illumina_5base` output stays minimap2.

## Assumptions

- **Defaults:** the question is which defaults to ship. Each arm runs its engine's 5-Base defaults; B-loc and B-X are the declared controls.
- **Reference data:** DRAGEN's CX report (a deduplicated full-depth pileup) is the reference, as in the existing contract. Its own aligner and calls are not ground truth. The specificity guards and the gain-set score limit any reward for merely resembling DRAGEN.
- **Trimming:** trimmed input is the representative user workflow. Trim Galore defaults are adequate for 5-Base adapters; check this on the trimming report's adapter detection.
- **Samples:** Sample8 (NA12878, 100 ng) and Sample40 (HG002, 50 ng) are representative 5-Base WGS. Short-fragment/cfDNA libraries are out of scope; that is a stated limitation.
- **Margins:** with tens of millions of CpGs per set, per-CpG sampling error is negligible. The margins guard against pipeline effects, not sampling noise. No bootstrap.
- **Tools:** `concordance.py` is used unchanged; its `--selftest` passes before use.

## Validation

1. **Harness dry run:** run arm A and arm B on a 10M-pair subset twice each. Concordance metrics must be identical across repeats; wall time must agree within 10%. Option verification must pass, and an injected wrong option must fail it.
2. **Reference check:** the DRAGEN-CpG-in-FASTA fraction is ≥ 99.9% (Behavior 2.2).
3. **Same input:** read counts in the alignment reports are identical across the arms of one depth. Input md5s are recorded per cell.
4. **Common-set filter:** a toy test with three hand-built reports (arm A, a challenger, DRAGEN) checks that the common set and gain set contain exactly the expected CpGs. Recompute r at cov ≥ 10 for one real arm independently on a 100k-CpG random sample, and match `concordance.py` to 3 decimal places.
5. **Baseline sanity (setup check, not engine identification):**
   - Full-depth arm A on NA12878 own-set r at cov ≥ 10 should land within ±0.005 of the published 0.998.
   - HG002 should land within ±0.005 of 0.996.
   - Outside that range, the setup differs from the published validation (for example in trimming or input size): stop and investigate before applying the rule.
6. **Guards fire:** truncating one contig in a test report must trigger the per-contig abort.

## Questions or ambiguities

- **Q1 (Critical, in progress):** BaseSpace authentication on oxy is waiting for Felix's browser approval (device code sent 2026-10-29).
- **Q2 (Open):** if outcome (c) is reached, the non-breaking route is the default proposal. Felix chooses at decision time.
- **Q3 (Open):** the lambda/pUC19 controls in the reference. Include them only if DRAGEN's reference has them. Decide after seeing DRAGEN's CX contig list.

Resolved on 2026-10-29:
- machine: oxy;
- data: download;
- screen: ~100M pairs;
- thresholds: as proposed;
- the published numbers came from minimap2 (`validation/VALIDATION_REAL_DATA.md`).

## Self-Review

**Logic:** each confound the reviewers named now has a control.
- **Soft-clipping:** trimmed input plus the B-loc arm.
- **Insert caps:** the B-X arm.
- **Multimappers:** MAPQ-0 and CHG/CHH guards.
- **Dedup and soft-clip duplicates:** duplicate rate reported; the gain-set score catches misplaced gains.
- **Per-arm denominators:** scoring on the common set.
- **Options not applied:** verified from `@PG`.
- **Reference mismatch:** the CpG-position check.

The decision rule and challenger choice are frozen in a committed file before any challenger runs.

**Edge cases:**
- Interrupted and failed cells are re-run.
- Disk-headroom and per-contig guards abort a cell.
- Read-count parity is asserted.
- The ±5% join check was removed because it would abort a genuinely better arm.

**Cost:** the screen-then-confirm design cuts full-depth alignments from about 8 to 4.

**Integration:** a switch defaults to the non-breaking `--five_base_index` route. The breaking route lists every affected surface.

**Remaining risks:**
- oxy pod recycles; mitigated by off-box copies after each cell and by md5-verified re-download.
- Disk is tight at full depth; mitigated by deleting untrimmed FASTQs and by the headroom guard.
- The conclusion covers 5-Base WGS only, not cfDNA.

## Revision history

- **2026-10-29 rev 0:** initial plan; manually approved by Felix.
- **2026-10-29 rev 1:** applied both plan reviews (`PLAN_REVIEW_reviewer-A.md`, `PLAN_REVIEW_reviewer-B.md`) and Felix's decisions (oxy, download, ~100M-pair screen, thresholds as proposed). Changes:
  - trimmed input;
  - arms B-loc and B-X (800);
  - common-set and gain-set scoring;
  - specificity guards;
  - pre-registration file;
  - option verification from `@PG`;
  - one reference with a CpG-position check;
  - deconvolution in the alignment run;
  - process-tree RSS and aligner CPU;
  - screen-then-confirm;
  - BAM retention;
  - outcome (b) and the non-breaking switch route;
  - the existing pin test reused;
  - the startup-notice fix.
  - Q2 answered from `VALIDATION_REAL_DATA.md`.
  - Not adopted: block bootstrap (sampling error is negligible at this scale), a simulated-truth arm, extra NA12878 libraries and a `--dovetail` arm. These are optional extras that would add cost without changing the decision rule.

## Implementation notes — 2026-10-29

Felix gave the implementation trigger for the benchmark (outline steps 2–5). Step 8, the code and docs change, waits for the decision.

- **Data:**
  - Screen inputs were downloaded to oxy `/var/tmp/5base_engines` via the personal BaseSpace account (EUW2) and verified: R1 = R2 per lane, `gzip -t`, `MD5SUMS`.
  - The durable copy is `s3://altos-lab-bioinf/CI/Illumina_5base_test_data/`.
  - Reads: NA12878 L001+L002 = 131.4M pairs; HG002 = 163.2M pairs.
  - DRAGEN NA12878 total input: 489.9M pairs.
- **Facts fixed in `PREREGISTRATION.md`** (committed `38e5f2d` before any challenger ran):
  - minimap2 2.31 `-F` default is 800, so B-X uses `-X 800`;
  - DRAGEN's lambda/pUC19 controls are in a separate `control.CX_report`, so the human reference stays control-free (answers Q3);
  - NUMA node 0 has physical cores 0–31, so `taskset -c 0-31`.
- **Harness:**
  - `validation/five_base_engine_bench.sh` (prep / cell / score) and `validation/five_base_compare.py` (compare / refcheck / `--selftest`; stdlib only, reusing `concordance.py`'s parser).
  - Self-tests pass locally and on oxy.
  - Copies live in `~/5base_harness` on oxy's persistent disk; results go to `~/5base_results`.
- **Deviations:**
  1. The harness is bash, not POSIX sh, because it needs arrays.
  2. `mawk` is oxy's `awk`, so flag tests use arithmetic rather than `and()`.
  3. `dcli scp` rejects paths, so files go through `dcli ssh` with base64.
  4. oxy's host shows a load average of ~50–70 from other pods. Pinning cannot isolate that, so timings are noisy and remain tie-breakers only, as planned.
  5. The reference and indexes are built into the ephemeral `/var/tmp`. They can be rebuilt with `setup_ref.sh`.
- **Confirm-stage prerequisite:** full depth needs all 8 lanes per sample (about 100 GB each), downloaded later with the same `bs download file` loop.

### Iteration log

1. **Index builds collided.** Bowtie 2 and HISAT2 were built into one prefix in one directory, and HISAT2's temporary `.sa` files clobbered Bowtie 2's (`Could not open file for reading a suffix array`). Both are now rebuilt in separate directories (`index/bt2`, `index/ht2`).
2. **Trim Galore output names.** Trim Galore 2.2 keeps the input's compression, so the plain subset produced `sub_R*_val_*.fq`, not `.fq.gz`, and prep aborted at its own output check. Prep now reuses a finished trim; `trimmed()` accepts either extension.
3. **`--gzip` for future trims** (Felix's suggestion). Full-depth confirm input would otherwise be about 350 GB of plain FASTQ per sample. The dry-run and screen trims stay plain; compression does not affect the comparison.
4. **Reference check (NA12878):** 1,000,000 of 1,000,000 sampled DRAGEN CpGs are CpGs in the Ensembl FASTA (100.0000%), so the reference matches DRAGEN's coordinates.
5. **Shared reference indexes** (Felix's direction). The benchmark uses the shared prebuilt indexes rather than its own builds. They are linked from oxy's persistent `~/references/GRCh38/{Bowtie2Index,Hisat2Index,WholeGenomeFasta}`, which point to `/datasets/s3/altos-lab-reference-data/genomes/Homo_sapiens/Ensembl/GRCh38/`; prefix `Homo_sapiens.GRCh38`.
   - Both indexes match our FASTA: 194 sequences with identical names and lengths, checked via `bowtie2-inspect -s` and `hisat2-inspect -s` against the `.fai`.
   - The HISAT2 index is plain: `hisat2-inspect --ss/--exon/--snp` each report 0.
   - Our own index builds were stopped. The harness reads `REFS` (default `~/references/GRCh38`).
   - The index files are S3-backed, so the first Bowtie 2/HISAT2 run may include a cold read. Timing only breaks ties, so this is acceptable.
6. **Dry run (NA12878, 10M pairs, arms A and B, two repeats each).**
   - **Accuracy is deterministic.** Each arm's two repeats give byte-identical CpG reports (A `f1d707dc…`, B `62a0bf32…`) and identical read metrics.
   - **Wall time is not repeatable within 10%:** arm A 1073 s vs 924 s, arm B 566 s vs 542 s. The host load average reached 1070, from other tenants. This confirms that timing cannot decide anything here.
   - **Scoring works end to end.** Own-set r is about 0.77–0.79 at ~1× coverage, consistent with the published 0.5× value of 0.77. The common set (7,049 CpGs) is identical for both arms, and B's gain set (1,408 CpGs) scores lower (r 0.67).
   - **The joined-fraction guard fired** (A 0.34, B 0.32), as expected at ~1×. The threshold is now `MIN_JOINED` (default 0.5, meant for screen depth).
7. **Per-cell cleanup.** A 10M cell uses about 8 GB, so seven 100M cells would need about 550 GB against 645 GB free. Each cell now deletes the per-context call files and the deduplicated BAM, and gzips the deconvolution table.
   - It keeps the raw BAM, the CpG report and the `.cov`/bedGraph, about 30 GB per screen cell.
   - Deviation from "keep BAMs": the deduplicated BAM is re-creatable from the raw BAM in minutes.
8. **The screen runs as two concurrent streams.**
   - Stream 1 (NUMA node 0, CPUs 0–31): NA12878 A, then B-loc, then C.
   - Stream 2 (node 1, CPUs 32–63): NA12878 B, then B-X, then HG002 A.
   - The CPU set is recorded per cell. As Efficiency allows, concurrent screen timings are excluded; cost is measured in the sequential confirm stage.

## Implementation notes — outcome (a), 2026-10-30

The screen gave outcome (a) under `PREREGISTRATION.md` (see `RESULTS.md`). Felix gave the implementation trigger for Behavior 8:
- **Docs** (`docs/src/content/docs/rust/illumina-5-base.md`):
  - the stability contract now names the default minimap2 engine as the one behind the published numbers;
  - "Running it" states why minimap2 is the default, with a link to the evidence;
  - a new "Choosing an engine" section has the screen table and advises raising Bowtie 2's 500 bp insert cap (e.g. `-X 800`) on 5-Base libraries;
  - the tuning section points to it.
- **Startup notice:** `five_base_notice(aligner)` in `mod.rs` names the engine that runs and says paired-end only. Before, it said minimap2 and "SE + PE" for every run.
  - Unit test: `five_base_notice_names_the_engine_that_runs`.
  - CLI assertion in `five_base_1134_default_argv_is_exact`.
- **CHANGELOG:** one sentence.
- **No pin test added:** `illumina_5base_resolves_to_minimap2` already pins the default.
- **Committed results:** the harness, `RESULTS.md` and the small text results (`bench_results/`, excluding `bismark.log` and the tarball). The pre-registration commit is `425d42b` after the rebase onto `dev` (`5da54cd`); it was `38e5f2d` before.
- **Not done (outside the pre-registration):** a 5-Base Bowtie 2 default insert cap (and `--dovetail`), and a larger minimap2 `-K` for 5-Base throughput. Both are proposed as separate issues in `RESULTS.md`.
- **Wall time:** dropped from the docs table and the decision, because the screen ran concurrently on a shared, heavily loaded host (Efficiency section).
- **Checks:** fmt, `--lib aligner::` 612, `aligner_cli` 139, release Clippy `-D warnings` and the full `bismark` suite (2324 passed, 0 failed, 20 ignored) pass.

### Review round — 2026-11-01

Dual code review (`CODE_REVIEW_reviewer-A.md`, `CODE_REVIEW_reviewer-B.md`) and the coverage audit (`COVERAGE.md`, INCOMPLETE: 2 MISSING, 4 PARTIAL) both confirm outcome (a). Felix gave the implementation trigger for the fixes.

- **Docs:**
  - the input is described as 100M pairs before trimming (98.1M after);
  - the table lead-in separates the shared r set from each engine's own CpG count;
  - memory is given in GiB;
  - a note explains why screen r is below the full-depth contract figure;
  - the untested causal claims (memory, "typical fragments") are removed.
- **`RESULTS.md`:**
  - B-loc and recall statements corrected;
  - HISAT2's loss is not attributed to the insert cap;
  - minimap2's index build (~69 s) and thread use (~3.8 of 32 busy, from noisy wall times) are reported from its log;
  - follow-ups revised (Bowtie 2 `-X 800`/`--dovetail`; minimap2 `-K`).
- **`five_base_compare.py`:**
  - the newline is stripped from the last field, so 6-column reports load;
  - duplicate sites and duplicate arm names fail;
  - the empty-common-set, per-contig and joined-fraction guards moved here (`--min-joined`);
  - the self-test now checks r against `concordance.py`'s reference functions and covers each guard (Validation 6), plus a failing refcheck.
- **`five_base_engine_bench.sh`:**
  - the process tree is sampled from the launched PID's descendants (no `setsid`);
  - a `@PG` command-line check (`check_pg_line`/`verify_pg`) is added;
  - arm A requires `-x sr` and B-loc requires `--reorder`;
  - a missing commit hash fails, and the DRAGEN CX report must be unique;
  - `score` requires each cell's `DONE` marker, exactly one CpG report and equal read counts;
  - `selftest` passes the real options for every arm and rejects 8 injected wrong ones (Validation 1).
- **CLI test:** asserts that the Bowtie 2/HISAT2 notice does not mention minimap2.
- **Data loss on oxy:** an idle restart between 2026-10-30 and 2026-11-01 wiped `/var/tmp`, including the screen BAMs (kept per plan "until the PR merges") and the dry-run score file.
  - Durable results survive: `~/5base_results` on oxy, `bench_results/` here, and the inputs in S3.
  - The retroactive `@PG` check could therefore not be run on the screen BAMs; `RESULTS.md` says so.
  - Dry-run scorer value, for the record: A own-set r at cov ≥ 10 = 0.789631, n = 9,492 (`concordance.py`: 0.7899, n = 9,501).

### Re-review round — 2026-11-01

The re-reviews (`CODE_REVIEW_R2_reviewer-A.md` / `-B.md`) and the re-audit (`COVERAGE_R2.md`: COMPLETE) found no regressions. Felix gave the implementation trigger for the remaining items:

- **`RESULTS.md`:**
  - a data-retention caveat covering the lost BAMs and CpG reports, the fact that `score.tsv` cannot be recomputed without re-aligning, and that it was produced by the pre-guard scorer;
  - thread use stated as an observation from noisy wall times, with `-K` as an untested hypothesis;
  - minimap2 index build given as ~69 s;
  - the dry-run scorer value's source stated;
  - "raw outputs" changed to "retained outputs".
- **Docs:** "needs no prebuilt index".
- **Harness:**
  - the `@PG` check matches each arm's flags as one string;
  - the header is read before grepping, to avoid a `pipefail`/SIGPIPE false failure;
  - `score` checks that the `DONE` key matches `metrics.tsv` and that read counts are non-empty;
  - a control-only CX report gets a message;
  - a zero memory peak aborts;
  - five new failing self-test cases cover `-x sr`, B-loc `--reorder`, the B-X/C `--local` ban and the B-X insert value. The self-test now has 18 cases (5 pass, 13 rejected), on the laptop and on oxy.
- **Scorer:** the self-test asserts the empty-common-set message.
- **CLI test:** asserts that no stderr line says "aligned with minimap2".
- **Checks:** fmt, `--lib aligner::` 612, `aligner_cli` 139, release Clippy and both self-tests pass.

# Plan review (Reviewer B): Settle the Illumina 5-Base default aligner

Plan: `plans/10292026_5base-default-engine/PLAN.md`
Worktree checked: `/Users/fkrueger/Github/Bismark-5base-default` at `77d2b60` (origin/dev), plus the #1138 diff at `/Users/fkrueger/Github/Bismark-issue1134` HEAD `346be2d`.

Verdict: the overall approach is sound: arms fixed in advance, a non-inferiority rule, replication on a second sample, and a stop before implementation. As written, though, the experiment does **not** isolate the engine. Three pipeline properties that differ by engine are left uncontrolled: adapter read-through on untrimmed input, the engine-defined proper-pair flag that the 5-Base driver filters on, and soft-clip-dependent dedup. The decision rule also has a ceiling problem that reduces it to a single gameable metric. Q2 can already be answered from a committed file.

---

## 1. Logic review

### 1.1 The design does not isolate the engine

**(a) The proper-pair filter is defined by the engine.** `five_base_emit_pe_record` (`rust/bismark/src/aligner/mod.rs:2098`) keeps a pair only if `rec1.flag & 0x2` is set and both mates are mapped. Each engine decides 0x2 differently:
- Bowtie 2/HISAT2 use the hard `-I/-X` window (default 500 bp). After #1138 they run without `--dovetail`, `--no-mixed` or `--no-discordant`, so dovetailed and contained pairs are not concordant.
- minimap2 infers the pair distance from the data and has no hard 500 bp limit.

B' (`-X 1000`) controls for the long-insert side only. It does not cover the short-insert side, where one mate runs past the other's start, which is the side #1134/cfDNA is about. Any ΔCpG or mapping-rate difference is therefore partly "how each engine sets 0x2", not alignment quality.

**(b) Untrimmed input conflates the engine with clipping behaviour.** The plan never says whether reads are adapter/quality trimmed. The prior validation explicitly ran untrimmed (`validation/VALIDATION_REAL_DATA.md`: "applies no adapter/quality trimming").
- On untrimmed reads, minimap2 `-x sr` is local and soft-clips adapter read-through.
- Bowtie 2/HISAT2 run end-to-end (`-q --score-min L,0,-0.6`) and must absorb the adapter as mismatches, or fail to align.

That favours minimap2 on mapping efficiency and CpG count for reasons that disappear in the recommended Bismark workflow (Trim Galore first). DRAGEN also trims adapters by default, so untrimmed input also widens the gap to the reference. The plan must pick one: trimmed input for all arms (my recommendation, since it matches the user workflow), or both, with the decision taken on trimmed input.

**(c) Dedup depends on soft-clipping.** `deduplicate_bismark -p` keys on aligned positions. minimap2's soft-clipped reads shift POS and end, so true PCR duplicates can escape dedup more often under minimap2 than under end-to-end Bowtie 2. Retained duplicates inflate "CpGs at cov ≥ 10", which is the one metric that can actually trigger a switch (see 1.2). The scale is not small: the record shows dedup moved r from 0.988 to 0.998 at cov ≥ 10, five times the plan's switch margin. Report the dedup removal fraction per arm. If it differs by more than about 1 pt, treat ΔCpG as confounded.

**(d) MAPQ.** `--five_base_min_mapq` defaults to 0 (`cli.rs:148`), so multimappers are kept with an arbitrary primary placement. The engines use different MAPQ scales (minimap2 0–60, Bowtie 2 0–42, HISAT2 0/1/60), so no single threshold means the same thing across arms. DRAGEN's counting set is "uniquely-mapped pairs" (record: 438.5M uniquely mapped). Defaults-vs-defaults at MAPQ 0 is a legitimate question. But the extra CpGs one engine gains in repeats may be false placements that still meet cov ≥ 10. Add a secondary analysis with an engine-appropriate uniqueness filter, e.g. MAPQ ≥ 20 for minimap2 and Bowtie 2, MAPQ 60 for HISAT2. The decision should hold under both.

**(e) Reference and spike-ins.** DRAGEN's reference contains the lambda/pUC19 controls; its methyl_metrics report their conversion. If our GRCh38 lacks them, control reads are forced onto human. A local aligner (minimap2) is more likely to place them partially than an end-to-end aligner, which again differs by engine. Also, the earlier HG002 run used an Ensembl-named genome while NA12878 used UCSC names (record, HG002 section). Use **one** FASTA for both samples, ideally DRAGEN's own (alt-masked hg38 plus controls), and build both indexes from it.

### 1.2 The decision rule has a ceiling and reduces to one gameable metric

- At r = 0.998 (cov ≥ 10), "clearly better" by Δr ≥ +0.002 needs a perfect r = 1.000. It is unreachable, so the rule in practice becomes **ΔCpG ≥ +2%**. That is the metric most inflated by retained duplicates (1.1c), multimapper placement (1.1d) and the proper-pair definition (1.1a).
- Each arm's r is computed on its **own** shared set (`concordance.py cmd_methyl` intersects ours with DRAGEN per arm). An arm that adds hard CpGs slightly lowers its own r, and an arm that drops them raises it. Accuracy and sensitivity are coupled.
  - Fix: compute accuracy on the **common set**: CpGs at cov ≥ 10 in every arm and in DRAGEN.
  - Separately, score the **gained CpGs**: those at cov ≥ 10 in X but not in A, compared against DRAGEN with MAD and call agreement. Only gained CpGs that agree with DRAGEN count as real sensitivity.
- r alone hides systematic bias, because a constant +1 pt shift barely moves r. Add **mean signed Δ%** (ours − DRAGEN) and mean |Δ%| on the common set as non-inferiority metrics. `concordance.py` already computes MAD, but the rule does not use it.
- Unspecified outcomes the rule must pre-register:
  - B' qualifies and B does not. Is the outcome "switch to Bowtie 2 with a non-default `-X`" (a different default), "keep minimap2", or "document `-X 1000`"?
  - Both B and C qualify: which one wins (cost, then simplicity)?
  - How "best challenger" is chosen on NA12878 for the HG002 replication: by which metric?
- "Cost is a tie-breaker only" lets a +2% CpG gain at 3× wall time plus a mandatory index build carry the switch. Pre-register a cost ceiling, or at least state that Felix weighs cost at decision time.
- Pre-registration is not enforced. Implementation step 1 resolves Q1–Q2 only, but Q3 (thresholds) and Q5 (arms) must also be locked **before** any data is seen. Otherwise "pre-registered" is not true.

### 1.3 Q2 is already answered, outside `plans/`

`validation/VALIDATION_REAL_DATA.md` is committed and is not under `plans/`, so the read restriction does not apply.
- It records the full-depth NA12878 run as `bismark --illumina_5base --genome <GRCh38>` with no engine flag. Its prose states "despite a different aligner (minimap2 vs DRAGEN)".
- The TAPS run names "the default minimap2 `-x sr` unconverted path".
- It also documents HG002 at full depth: r 0.988 at cov ≥ 1, 0.996 at cov ≥ 10.

So minimap2 produced the published numbers. Validation 5's empirical answer to Q2 is then also logically flawed: reproducing 0.998 does not identify the engine, since Bowtie 2 may also reach 0.998. It is still a useful **setup** check (Section 4).

### 1.4 Smaller logic points

- Behavior 7's "add a unit test pinning the default" already exists: `illumina_5base_resolves_to_minimap2` at `config.rs:2256`, which also asserts that `--illumina_5base --minimap2` resolves to minimap2. Nothing to add. Behavior 8 must flip it.
- Deconvolution "as a separate `--five_base_deconvolution` pass": there is no standalone deconvolution-from-BAM mode. `--five_base_deconvolution` is an align-time flag that post-processes the BAM it just wrote (`run_pe_five_base`), and only consensus has `_from_bam`. A "separate pass" therefore means **two to four extra full-depth alignments**. See Efficiency.
- Deconvolution runs over the **non-deduplicated** BAM while methylation concordance uses the deduplicated one. That is fine, but say so, so the precision/recall numbers stay comparable with the published 90.3/93.4.
- The record shows the earlier NA12878 run used about 384M pairs, while DRAGEN reports 438.5M uniquely mapped pairs. The earlier input was probably incomplete (the record also notes a partial BaseSpace download). Read-count parity across arms (Behavior 9) does not catch an incomplete FASTQ set. Add a check of total pairs against DRAGEN's mapping metrics.

## 2. Assumptions

| Assumption | Status |
|---|---|
| Arms differ only by engine | **False as written**: trimming, 0x2 definition, MAPQ scale and soft-clip dedup all co-vary (1.1) |
| DRAGEN concordance = accuracy | Partly. It measures *DRAGEN-likeness*. The engine whose placement resembles DRAGEN's (alt handling, segdups) wins. Add the lambda/pUC19 known-truth controls per arm, and optionally stratify Δ by GIAB mappability/segdup stratification BEDs to see *where* arms differ |
| `concordance.py` correct as-is | Mostly yes. Its `chr` stripping does not reconcile `chrM`/`MT` or alt/random contig names (`chr1_KI270706v1_random` vs `KI270706.1`). A partial mismatch drops sites silently, and the `>0` join guard misses it |
| Engines are deterministic | Probably true (Bowtie 2 with `--reorder` is seeded per read, minimap2 is thread-invariant). This also means re-running cannot estimate variance (Section 3) |
| Input is trimmed / untrimmed | **Unstated**. Must be explicit (1.1b) |
| Same thread budget is fair | `taskset` 32 cores is shared by the aligner (`-p 32`) and Bismark's serial driver plus BAM compression. Equal across arms, but see Efficiency on driver-bound timing |
| Data "staged on local disk" | Contradicts Implementation 3, which stages on `/weka`, a network FS. State which it is, or accept /weka and say timing includes it equally |
| `scripts/bench_run.sh` is reusable | It is extractor-specific (`--tool rust|perl --mode gzip|plain`). Reuse only its `time -v` parsing and `/proc` sampling |

## 3. Statistical sufficiency of 2 samples

- Per-CpG sampling error on more than 50M sites is negligible (the SE of r is about 1e-5), so every Δ will be "significant". The thresholds are effect-size margins, and that is the right framing.
- The real uncertainty is library-to-library and sample-to-sample. Two GIAB WGS gDNA libraries with the same protocol and similar input (100 vs 50 ng) are a *conservative-but-narrow* test. "Must hold on both" is a sound guard against false switches. It is not a basis for claims about short-fragment or cfDNA libraries (the plan already says so).
- Add a cheap uncertainty estimate: **block bootstrap by chromosome (or 10 Mb blocks)** for Δr, ΔCpG and Δbias on each sample. That gives a CI to compare against the margins at little cost.
- Calibrate the margins against pipeline-choice noise. Dedup alone moved r by 0.010, and trimming will move it too. Margins of 0.001 to 0.002 are smaller than pipeline-choice effects, which is another reason to pin the pipeline (1.1).

## 4. Validation sufficiency

What exists: join > 0, ±5% join size, read-count parity, a reproducibility rerun, metric re-derivation, and the published-number check.

Gaps that could let a wrong comparison through silently:

1. **Options actually applied.** #1134 was precisely a bug where 5-Base Bowtie 2/HISAT2 silently ignored `-I/-X` and tuning options. Assert from each arm's report ("was run with" line, which #1138 makes share one string with argv) and the BAM `@PG` that:
   - B' contains `-X 1000`;
   - every arm has `-p 32` / `-t 32`;
   - HISAT2 has `--no-spliced-alignment`.
2. **The ±5% join-size guard contradicts the decision rule.** A legitimate +6% sensitivity gain would abort the arm. Replace it with:
   - per-chromosome join counts compared with arm A (catches partial contig mismatches);
   - the fraction of our CpG keys present in DRAGEN's set, which should be ≥ 99% on primary contigs.
3. **Input completeness.** Check total input pairs against DRAGEN's `mapping_metrics` total, not only parity across arms.
4. **Reference identity.** Use an md5/sequence-dictionary check that the Bowtie 2 and HISAT2 indexes were built from the exact FASTA passed as `--genome` (`bowtie2-inspect -n`, compared with the FASTA headers). Bismark does not validate index/FASTA contig consistency (`discovery::validate_unconverted_index` checks file presence only).
5. **Per-arm diagnostics to interpret differences:**
   - dedup removal %;
   - the fraction of pairs dropped by the 0x2 filter (`no_single_alignment_found`);
   - soft-clipped base fraction;
   - MAPQ distribution;
   - lambda/pUC19 methylation (the known-truth floor).
6. **Validation 5 reframed:** arm A on NA12878 should reproduce about 0.998 (dedup, cov ≥ 10) and HG002 about 0.996 (record). A miss means the setup diverged (FASTA, trimming, input set, version). It does not tell us which engine was used. Note that the earlier run may have used about 384M pairs, so a *better* match at true full depth is plausible.
7. **Validation 3:** a 10M-pair rerun checks determinism of the harness, not correctness at depth. It is fine as is, but say that it is a determinism check.

## 5. Efficiency, cost and feasibility

- **No time estimate in the plan; add one.**
  - The record's own figure is 10M pairs in 13 min with minimap2 (plus consensus), which extrapolates to roughly 10 h for about 490M pairs.
  - Bowtie 2 end-to-end is typically 1.5 to 3 times slower than minimap2 on 2×150.
  - Rough total: 4 NA12878 arms + 2 HG002 arms + 2 to 4 deconvolution re-alignments + 6 to 8 dedup/extract/CX passes. That is on the order of **100 to 150 machine-hours** on a shared box.
  - State it, and state the go/no-go on colossal availability.
- **Driver-bound timing.** The 5-Base driver (`five_base_align_and_call_pe`) parses SAM, calls methylation and writes BAM on one thread. At 32 aligner threads the fastest engine may be limited by Bismark, not by the engine, so wall-time differences would understate engine speed differences. Sample the aligner child's CPU separately (`/proc/<pid>` as `bench_run.sh` does) and report whether the driver thread is saturated.
- **Deconvolution passes.** Instead of separate re-alignments, add `--five_base_deconvolution` to the arm's single alignment run, either for all arms (avoiding the problem of not knowing the challenger yet) or for A and B. Report the deconvolution time separately via stderr timestamps. The deconvolution pass is small next to alignment, and this saves 2 to 4 full alignments.
- **Do not delete BAMs before the decision.**
  - At about 128 GB per arm (record), 6 arms come to about 0.8 TB of 28 TB on /weka.
  - Deleting them means any follow-up costs about 10 to 25 h per arm: the common-set analysis, MAPQ stratification, dedup comparison, or a reviewer's question.
  - Keep BAMs and CX reports until the PR merges.
- **Q4 (depth).** A 100M-pair subsample is about 9× depth, so "CpGs at cov ≥ 10" becomes a very different and much more threshold-sensitive quantity. If subsampling for screening, use lane subsets (as the record does: 1, 4 and 8 lanes) and decide **only** on full depth. Note that arm A at full depth is the most expensive "known" quantity, and an exact rerun is needed anyway for timing parity.
- `concordance.py` holds two pure-Python dicts of about 55M tuple-keyed entries (roughly 20 to 40 GB RAM, tens of minutes per call), and the plan needs at least 6 calls plus the common-set analysis. That is feasible on colossal. A sort-merge or pandas/polars variant for the new multi-arm common-set metric would be faster. Keep `concordance.py` itself unchanged as the reference.
- `taskset` on a 128-core box: pin to one NUMA node's cores to reduce variance, and record the CPU list.

## 6. Breaking-change handling (if the default switches)

What the plan misses:

- **Output filenames change.** The token is `aligner.token()` (`mm2` vs `bt2`/`hs2`), giving `_bismark_mm2_pe.bam` and `_bismark_mm2_PE_report.txt`. Downstream globs and pipelines break even for users who add an index. This must go in the breaking note.
- **`--mm2_*` flags** used with a bare `--illumina_5base` start failing with "only valid with --minimap2".
- **CI and tests.** `aligner_five_base_groundtruth.rs` (including the `five_base_controls_*` CI gates), `aligner_five_base_simplex.rs` and `aligner_five_base_bisulfite.rs` run the FASTA-only default with no `--five_base_index`. Each needs either an explicit `--minimap2` or a committed Bowtie 2 fixture index plus bowtie2 on the CI runner. The test diff is larger than "update the tests that rely on the default".
- **Non-breaking alternative worth listing:** keep minimap2 for FASTA-only runs, and let `--five_base_index` **without** an engine flag select Bowtie 2. That combination is currently an error (`config.rs`: "`--five_base_index` only applies to … `--bowtie2/--hisat2`"), so no existing command changes meaning. It is still explicit, because the user passed an index, but weigh it against the project's never-silent-precedence principle. A softer alternative is a docs-only recommendation of `--bowtie2` for users with an index.
- **The escape hatch already exists:** `--illumina_5base --minimap2` resolves to minimap2 (tested). After a switch, assert that it still needs no index and that `--minimap2 --five_base_index` still errors.
- The docs' "Running it" paragraph from #1138 ("minimap2 is the default because it indexes the genome FASTA at run time …") must be rewritten in either branch, and the stability contract should name the engine.

## 7. Alternatives

1. **Control first, then default.** Run a pinned-pipeline comparison (trimmed input, the same dedup, the same FASTA plus controls) as the primary analysis, and defaults-as-shipped as a secondary one. If the engines tie on the pinned pipeline, the remaining "default vs default" gap is about default *options*, such as `--dovetail` or `-X` for Bowtie 2, and that may be fixed better by changing those options than by changing the engine.
2. **Add a Bowtie 2 `--dovetail -X 1000` arm** (or make it B') to cover both sides of the insert-size window, instead of `-X` alone. After #1138 the 5-Base path omits `--dovetail`, so check whether it can be passed through. If it cannot, that is itself a finding.
3. **Known-truth control:** the lambda/pUC19 methylation per arm, and chrM or GIAB high-confidence regions, as a DRAGEN-independent sanity axis.
4. **Short-fragment sample (Q6):** even one cfDNA-like library, or an *in-silico* short-insert subset of NA12878 selected by TLEN < 150, would address the use case that triggered #1134. The decision could be restricted to "WGS gDNA" explicitly if this is skipped.

---

## Action items

### Critical
1. **Specify and pin input trimming.** Use trimmed input for all arms, with an untrimmed result secondary if wanted. Untrimmed input confounds local vs end-to-end engines (1.1b).
2. **Measure accuracy on the common CpG set**, and score the gained CpGs separately. Per-arm own-set r couples accuracy to sensitivity (1.2).
3. **Fix the decision rule's ceiling.** Δr ≥ +0.002 from 0.998 is unreachable, so the rule collapses to ΔCpG ≥ +2%, which is inflated by duplicates, multimappers and the 0x2 definition. Add bias (mean signed Δ) and MAD on the common set, and require that the gained CpGs agree with DRAGEN (1.2).
4. **Pre-register everything before data:** Q3 thresholds, Q5 arms, the selection metric for "best challenger", what happens if only B' qualifies, B vs C ties, and a cost ceiling. Move Q3/Q5 into Implementation step 1 (1.2).
5. **Assert that the applied options match each arm** from the report/`@PG` (`-X 1000`, thread counts, `--no-spliced-alignment`). Silently ignored options is exactly the bug class #1134 fixed (4.1).

### Important
6. Report the dedup removal % per arm and treat ΔCpG as confounded if it differs by more than about 1 pt. Consider a soft-clip-aware dedup check (1.1c).
7. Add an engine-calibrated MAPQ-filtered secondary analysis; the decision must hold under both (1.1d).
8. Use one FASTA for both samples, matching DRAGEN's (alt-masking, lambda/pUC19 controls, naming). Verify that the indexes come from it (1.1e, 4.4).
9. Replace the ±5% join guard, which would abort a legitimate gain, with per-chromosome join counts and a coverage-fraction check (4.2).
10. Check input completeness against DRAGEN's total pairs, since the prior run probably had about 384M of about 490M (1.4, 4.3).
11. Answer Q2 from `validation/VALIDATION_REAL_DATA.md`, which is committed outside `plans/`: it was minimap2. Reframe Validation 5 as a setup check with the expected values of 0.998 for NA12878 and 0.996 for HG002 (1.3).
12. Fold deconvolution into the arm's alignment run. There is no standalone deconvolution-from-BAM mode, so "separate pass" means extra full alignments (1.4, 5).
13. Keep BAMs and CX reports until the PR merges (5).
14. Add a compute-hours estimate and a driver-saturation measurement (5).
15. Breaking-change list: add the output-filename token change, `--mm2_*` breakage and the CI-fixture impact, and list the non-breaking "`--five_base_index` implies Bowtie 2" alternative for Felix (6).
16. Behavior 7's test already exists (`config.rs:2256`). Drop "add", and in Behavior 8 flip it (1.4).

### Optional
17. Block-bootstrap CIs (by chromosome) for Δr, ΔCpG and Δbias (3).
18. Lambda/pUC19 and GIAB-stratified Δ as DRAGEN-independent axes (7.3).
19. A Bowtie 2 `--dovetail -X 1000` control, or check whether `--dovetail` can be passed through in 5-Base (7.2).
20. A short-insert in-silico subset, or a cfDNA sample, for Q6. Otherwise scope the conclusion to WGS gDNA (7.4).
21. NUMA-pinned `taskset`, recorded CPU list; resolve the /weka vs local-disk contradiction (2, 5).
22. A faster multi-arm common-set script alongside the unchanged `concordance.py` (5).

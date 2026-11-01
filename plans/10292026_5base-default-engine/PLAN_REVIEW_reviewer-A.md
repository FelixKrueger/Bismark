# Plan review (Reviewer A): Settle the Illumina 5-Base default aligner

Plan: `plans/10292026_5base-default-engine/PLAN.md`
Worktree checked: `/Users/fkrueger/Github/Bismark-5base-default` at `77d2b60` (origin/dev), plus the #1138 diff (`Bismark-issue1134` HEAD `346be2d`).
Date: 2026-10-29

## Verdict

The goal is sound and the guard rails (pre-registration, a second sample, explicit approval for a breaking switch) point the right way. But in its current form the experiment does **not** isolate the engine. Five non-engine factors differ between the arms and would decide the result on their own: soft-clipping vs end-to-end with untrimmed reads, the max-insert cap, MAPQ/multimapper acceptance, a dedup key that ignores soft clips, and a 200x-shrunk minimap2 batch size. The decision rule also has a ceiling problem: Δr ≥ +0.002 cannot be reached from 0.998, so the rule reduces to "≥ 2 % more CpGs", and an arm that over-maps can game that. Q2 can be answered now from a committed file. Fix the Critical items before any run.

---

## 1. Logic review

### 1.1 Facts checked in the code (confirmed / corrected)

| Plan claim | Finding |
|---|---|
| Default chosen in `resolve_aligner` (~1319) | Confirmed. The 5-Base block is at `config.rs:1311-1349`; with no engine flag it returns `Aligner::Minimap2`. |
| "A unit test pinning the default may already exist" | **It exists:** `config.rs:2255 illumina_5base_resolves_to_minimap2` (also covers `--illumina_5base --minimap2`). Behaviour 7's new test is redundant, so reuse this one. |
| Bowtie 2/HISAT2 run `-q --score-min L,0,-0.6 … -p N --reorder`, 500 bp max insert | Confirmed for pre-#1138 (`mod.rs:1536-1568`). After #1138 it is `<resolved 5-Base options> -p N --reorder`, so `-X` reaches the engine only on a post-#1138 build. |
| HISAT2 "is unspliced here, so it is close to Bowtie 2" (Q5) | **Not accurate.** The 5-Base HISAT2 string has no `--no-softclip` (the bisulfite path adds it; the 5-Base builder does not, and #1138 says "HISAT2 keeps its soft-clipping linear threshold"). HISAT2 here soft-clips like minimap2, and its MAPQ scale is {0,1,60}. It is a distinct arm, not a near-duplicate of Bowtie 2. |
| minimap2 runs "the resolved `-x sr` options with `-t N`" | Incomplete. The string is `-a --MD --secondary=no -t N -x sr -K 250K` (`options.rs:252-265`). `-K 250K` comes after `-x sr` and overrides the preset's `-K50m`, so minimap2 processes ~800-pair mini-batches. See 1.2(e). |
| Q2: which engine produced the published numbers | **Answerable now without reading `plans/`.** `validation/VALIDATION_REAL_DATA.md` (committed, under `validation/`) records the core run as plain `--illumina_5base` with no engine flag, i.e. minimap2 ("despite a different aligner (minimap2 vs DRAGEN)"). It also has **HG002 baselines** the plan omits: core r 0.988 / 0.996 at cov ≥ 10, call agreement 98.6 %, deconvolution 94.3 % / 95.7 %. |
| Startup notice | `mod.rs:223-229` says reads are "aligned with minimap2" for **every** 5-Base run, including `--bowtie2`/`--hisat2`. This is wrong today and must be fixed in either branch. The plan does not list it. |

### 1.2 Confounds: the arms differ in more than the engine

**(a) Soft-clipping vs end-to-end on untrimmed reads (Critical).** minimap2 `-x sr` and 5-Base HISAT2 soft-clip, while Bowtie 2 default is end-to-end. The plan never says whether the input FASTQs are adapter-trimmed. The validation record says the 5-Base path "applies no adapter/quality trimming", and DRAGEN trims internally. On raw reads, Bowtie 2 end-to-end fails or penalises every read-through fragment (insert < read length), and the 5-Base Bowtie 2 path omits `--dovetail` (per #1138), so those pairs also lose 0x2 and are dropped. This alone can produce a several-percent ΔCpG. It is a preprocessing × engine interaction, not engine quality. The plan must fix the input condition (raw, which is what the docs tell users to run; trimmed; or both). It should report the fraction of pairs with TLEN < read length per arm. A Bowtie 2 `--local` arm (#1138 enables it with G,20,8) is the like-for-like soft-clip control. That arm is far more informative than B' on its own.

**(b) Max-insert cap mismatch (Important).** Bowtie 2 and HISAT2 default to `-X 500`. minimap2's sr fragment cap is its `-F`/max-fragment-length, which I believe defaults to 800, not unbounded (please verify against the installed minimap2's `-h`). B' at `-X 1000` therefore over-corrects, and B' vs A still differs in the cap. For the stated aim, "separate engine quality from the insert-size default", B' should use the same cap as minimap2. Alternatively, pick the cap from the measured TLEN distribution of arm A.

**(c) MAPQ and multimapper acceptance differ by engine (Critical).** `five_base_emit_pe_record` (`mod.rs:2120-2125`) accepts any proper pair with both mates mapped and applies **no MAPQ filter**. `--five_base_min_mapq` affects only duplex/consensus. So MAPQ-0 multimappers count toward methylation in every arm. But Bowtie 2 places them at random, minimap2 picks a deterministic best, and HISAT2's MAPQ is near-binary. An engine that places more repeat-region reads gains "CpGs at cov ≥ 10" (the deciding metric) while barely moving r on the shared set. The plan should report, per arm: the fraction of accepted pairs with MAPQ 0 and with MAPQ < 10; CHG/CHH apparent methylation, a free mis-mapping indicator since mis-placed reads raise the non-CpG floor; and the concordance restricted to MAPQ ≥ 10 pairs, or to a mappability-stratified CpG set.

**(d) Dedup key ignores soft clips (Important).** The Bismark dedup key uses POS plus the reference end, and soft clips do not count (`dedup/pipeline.rs:1300` test `…with_soft_clip_does_not_count_s_op`). Two PCR copies clipped differently get different keys and both survive. That deflates the minimap2/HISAT2 duplicate rate and inflates their coverage, and so their CpG-at-cov ≥ 10 count, relative to end-to-end Bowtie 2. DRAGEN (Picard-style) dedups on unclipped 5' ends. The plan lists duplicate rate as a metric but does not treat it as a confound. At minimum, report the duplicate rate per arm and flag a gap of more than about 0.5 pt. Ideally, add a sensitivity run that dedups on unclipped positions, or compare at matched post-dedup depth.

**(e) minimap2 `-K 250K` (Important for cost, possibly for accuracy).** With 250 kb per mini-batch, a 32-thread minimap2 starves: the published timing would measure Bismark's minimap2 configuration, not the engine. minimap2 also estimates the PE fragment distribution per mini-batch, so a batch of about 800 pairs gives noisier pairing decisions than the preset's 50 Mb batch would. Record CPU utilisation, `(user+sys)/wall`, for every arm. Consider a one-off subsample check of minimap2 at `-K 50m` to see whether output and throughput move. If they do, that is a separate fix, and the default decision should not penalise minimap2 for it.

**(f) Reference identity with DRAGEN (Critical, under-specified).** The plan checks "same chromosome names", but `concordance.py::_norm_chrom` already strips `chr`. So name matching neither proves nor disproves identity, and `chrM` vs `MT` still silently drop out. The record shows the **previous HG002 run used an Ensembl-named genome** and NA12878 presumably a UCSC one. Both samples must use one FASTA, and the plan should say which. A cheap, strong identity check is available: for every DRAGEN CX CpG row, check that our FASTA also has `CG` at that position. The rate should be essentially 100 %, and the contig set and maximum positions should match. That check catches alt masking, PAR masking, decoy differences and assembly-patch differences, which a name check cannot. Also fix the evaluated contig set: the record used autosomes + X, while `concordance.py` uses every contig. Exclude chrY, chrM and unplaced contigs explicitly, since HG002 is male and the PAR treatment differs.

**(g) Input set vs DRAGEN.** The record says the NA12878 download was partial (L007 R1.part), says "~384M pairs", and DRAGEN reports 438.5M uniquely mapped pairs (~490M alignments in the dedup line). The comparison is relative across arms, so this does not bias the decision. It does make Validation 5, "reproduce r ≈ 0.998", unreliable as a setup check unless the read set is pinned. Verify every lane's md5 or size against BaseSpace and record the total pairs next to DRAGEN's.

### 1.3 Gaps in the step logic

- **Deconvolution cannot be "a separate pass" cheaply.** `--five_base_deconvolution` runs inside the aligner run, on the just-written BAM (`mod.rs:1828-1836`). There is no from-BAM deconvolution option; only consensus has one. As written, the plan needs **extra full-depth re-alignments**, and HG002 needs deconvolution too because rule 6 requires it "on both samples", which Behaviour 5 does not say. Either turn on `--five_base_deconvolution` in the main arm run for A and the challenger and time the deconvolution segment separately from the log, or add a small from-BAM deconvolution entry point first.
- **"Best challenger" selection is not pre-registered.** Behaviours 3 and 5 pick "the best non-minimap2 arm" / "the leading challenger" from NA12878 without a rule. Pre-register it, for example: the challenger that passes all non-inferiority conditions with the largest ΔCpG, tie broken by Δ mean |Δ%|.
- **The status of B' is ambiguous.** The Assumptions say B' is "not a candidate default". But if Bowtie 2 became the default, Bismark could set its own 5-Base `-X` default, so B' *is* a plausible candidate. The plan needs a rule for "B' wins, B does not".
- **The duplex/consensus GA contract is outside the experiment.** The stability contract covers core, duplex, consensus, consensus_from_bam and deconvolution, all validated with minimap2. A default switch changes the engine behind all of them, but the plan measures only core and deconvolution. Either re-validate consensus (mean-methylation agreement plus the lambda/pUC19 gate) under the new default, or state that the switch is conditional on it.

---

## 2. Assumptions

| Assumption | Status |
|---|---|
| Arms differ only by engine | **False as designed.** See 1.2 (a)-(e). |
| Concordance to DRAGEN is accuracy | Not stated, and it matters. It measures *DRAGEN-likeness*, and DRAGEN's own placement (soft-clipping, its own multimapper policy) is not ground truth. An engine that shares DRAGEN's biases wins. Name this caveat, and add one truth-based arm (see Alternatives). |
| `concordance.py` correct as-is | Its maths is fine (selftest), but it (i) evaluates r on each arm's own shared set (see 4), (ii) covers every contig, (iii) only prints text, so the harness must parse it, and (iv) holds two dicts of about 55M tuple keys each, roughly 30-40 GB of Python memory and a long parse when given full CX reports. |
| Thresholds "configurable until Q3" | Fine, but they must be frozen and committed, with the commit SHA recorded in RESULTS.md, before any challenger arm runs. Otherwise the pre-registration has no force. |
| 32 pinned cores = fair | The pinned set also runs the Bismark driver (single-threaded SAM parse, methylation call and BAM write). At 32 threads the driver may be the bottleneck for fast engines, which compresses cost differences. Record per-process CPU. |
| `/usr/bin/time -v` peak RSS = run peak | **No.** GNU time reports the max RSS of the single largest descendant, not the sum. In a 5-Base run the Bismark process (whole genome in memory) and the aligner (minimap2's in-RAM index, or the Bowtie 2 index) are alive at the same time. You need summed process-tree RSS: sample `/proc` for the whole tree, or use a cgroup `memory.peak` (`systemd-run --scope`). `scripts/bench_run.sh` samples one PID and is extractor-specific, so it can be adapted but not reused as-is. |
| Data staged | Q1 remains open. The memory notes say NA12878's 8 lanes and HG002 were already downloaded and aligned, likely under `/weka/projects/bioinf/Data/Felix/bismark_benchmarks/` on colossal. Check there before planning a `bs` download. |

---

## 3. Efficiency and feasibility

Rough budget. The record's 10M pairs took 13 min (with consensus) on default threads, and NA12878 is about 384-490M pairs. That gives about 6-10 h of alignment per minimap2 arm, plus dedup of a ~128 GB BAM, extraction and concordance (about 1 h or more each). Four NA12878 arms, two HG002 arms and the deconvolution re-runs come to **about 8 full-depth alignments, roughly 3-5 days of sequential colossal time**, plus about 1.5 TB of transient disk. Feasible, but much of it can be saved:

1. **Separate accuracy runs from timing runs.** Run the full-depth arms concurrently on disjoint core sets (128 cores = 3-4 × 32). Measure cost on a fixed 20M-pair subsample, 3 repetitions per arm, under the idle gate. Full-depth single-shot timing on a shared box is noisy anyway; Validation 3 already admits ±10 %. This cuts the calendar time by about 3x.
2. **Use a CpG-only cytosine report**, not CX. `concordance.py` reads only CG rows, and a whole-genome CX report is about 20x larger, which dominates extractor output time and the Python parse.
3. **Fold deconvolution into the main A and challenger runs** (1.3), which removes 2-4 re-alignments.
4. **Two-stage depth (Q4):** screen all arms (A, B, B'/B-local, C) at about 100M pairs, then run only A vs the leading challenger at full depth on both samples. Pre-register that the decision is taken on the full-depth pair only.
5. minimap2 re-indexes the FASTA on every run (minutes), which is correctly charged to it. Note in the docs that 5-Base minimap2 cannot take a prebuilt `.mmi` today.

---

## 4. Validation sufficiency

**The decision rule's ceiling and gaming (Critical).**
- At baseline r = 0.998 (cov ≥ 10), Δr ≥ +0.002 means r ≥ 1.000. That is unreachable, so the "clearly better" clause in practice is only **ΔCpG ≥ +2 %**.
- That metric is the one that rewards over-mapping (1.2c), under-deduplication (1.2d) and soft-clip rescue of adapter reads (1.2a).
- r is computed on **each arm's own** shared CpG set. An arm that recovers harder CpGs, in repeats or low-mappability regions, pays for it with lower r, and an arm that covers fewer CpGs is flattered. The rule therefore trades two quantities measured on different denominators.

Fix:
- Compute accuracy on a **common CpG set**: the intersection of all arms with DRAGEN at cov ≥ 10.
- Report sensitivity separately, as CpGs at cov ≥ 10.
- Express accuracy as a discordance quantity with headroom: mean |Δ%|, the call-disagreement fraction, or 1 − r as a relative change. For example, "challenger's discordance ≤ 1.1x minimap2's".
- Add a **specificity guard** to the switch rule. The challenger's CHH apparent methylation and its accepted-MAPQ-0 fraction must not exceed minimap2's by more than a stated margin. Without it, extra CpGs from mis-placed reads count as a win.
- Non-inferiority at Δr ≥ −0.001 allows the discordance (1 − r) to grow by 50 %, which is too loose as stated. The discordance-ratio form above makes the margin interpretable.

**Statistical sufficiency of two samples.** Within-sample sampling error is negligible at 53M CpGs: the SE of r is about 1e-5. So the thresholds are practical-significance margins, and that is acceptable. The real uncertainty is between-sample and between-library. Two GIAB cell lines from one project, one chemistry lot and one DRAGEN version (4.4.6), at 100/50 ng, form a narrow base. Adequate replication for a "keep" decision; thin for a breaking "switch". Cheap strengthening:
- Run block bootstraps over chromosomes to put intervals on each Δ.
- Check the *direction* of Δ on 2-3 extra NA12878 libraries from the same project (Sample1-36) at subsample depth.
- Require the switch to hold on all of them.

**Checks that would not catch a silently wrong comparison:**
- **The engine and options that actually ran are never asserted.** Before #1138, `-X 1000` was silently ignored on the 5-Base path, which is exactly the B' arm. The harness must parse the PE report's "was run with" line and the BAM `@PG` line and assert the engine plus the expected option tokens (`-X 1000`, `-p 32`, `--reorder`, etc.). It should also assert that the build contains #1138 (commit ancestry check, not only `--version`).
- **Join size within ±5 % of arm A** confuses a real sensitivity difference with a breakage. If the challenger is genuinely 6 % better, the arm aborts. Replace it with: fraction of DRAGEN CpGs joined ≥ X % and **per-contig** join counts within tolerance. That catches a dropped `chrX`/`MT`, which a global ±5 % does not.
- **Stale or crossed artefacts.** BAM names carry the engine token (`_bismark_mm2_pe.bam` vs `_bismark_bt2_pe.bam`), so a glob over a shared directory can pick up the wrong arm's BAM. Use one output directory per arm, assert exactly one `*_pe.bam`, and record the md5 of each BAM and cytosine report consumed at every stage in `metrics.tsv`. The `DONE` marker should embed the commit SHA plus an arguments hash, so a resume after a rebuild cannot reuse old arms.
- **Read-count parity** uses the report's sequence count. Also assert that the dedup input count equals the aligner's output count, so no stage silently read a truncated BAM after an OOM or a recycle.
- **Validation 5** needs a numeric tolerance (e.g. |Δr| ≤ 0.002 and shared-CpG count within 2 % of the record's 53.16M) and a pinned read set (1.2g). Otherwise "large gap" is a judgement call.
- Validation 4 (independent re-derivation) is good. Extend it to the common-set computation.

---

## 5. Alternatives worth considering

1. **A truth-based arm next to the DRAGEN concordance.** Simulate 5-Base PE reads from the same GRCh38: inject known 5mC→T at CpGs, plus the GIAB HG002 variants, with a realistic insert distribution that includes inserts shorter than the read length, and adapters. Run every engine. You get placement accuracy, multimapper behaviour and per-CpG truth error per engine in hours, not days, with no DRAGEN-likeness bias. The `aligner_five_base_groundtruth.rs` machinery is a starting point.
2. **Score deconvolution against GIAB v4.2.1 truth** (HG001, HG002) rather than DRAGEN's own hard-filtered VCF. That removes the circularity.
3. **A non-breaking path if a challenger wins.** Keep minimap2 as the FASTA-only default. Make `--five_base_index` without an engine flag select Bowtie 2, which is an error today, so adding it breaks nothing, and have it announce the engine (the never-silent rule). Or deprecate first: a release that prints "the default will change to Bowtie 2; pass `--minimap2` to keep it", then switch. That honours "no release cut mid-flight" and spares downstream (methylseq/nf-core) a surprise.
4. **Treat engine-specific fixes as their own outcome.** If the gap turns out to be `-K 250K`, a 5-Base `-X` default, a MAPQ floor or unclipped-position dedup, the right PR may be "fix X, keep minimap2", not an engine switch. The plan should allow that outcome explicitly.

---

## 6. Breaking-change handling (if the default switches)

Missing from Behaviour 8:
- **Output filenames change** (`_bismark_mm2_pe.bam` / `_PE_report.txt` → `_bismark_bt2_…`). That breaks downstream globs: methylseq/nf-core modules, MultiQC patterns, users' scripts. It needs its own CHANGELOG clause and docs line.
- **The startup notice and help text:** `mod.rs:223` (hard-codes minimap2), the `cli.rs` help for `--genome`/`--illumina_5base`/`--five_base_index`, and both `resolve_aligner` error strings that say "(Without an engine flag, 5-Base uses minimap2…)".
- **The test pin:** `illumina_5base_resolves_to_minimap2` flips. CI's 5-Base ground-truth gates (lambda/pUC19) would need a Bowtie 2 index fixture and bowtie2 installed in every job that runs them.
- **The stability contract numbers** (core, deconvolution, consensus) must all be re-measured under the new default, not only core.
- **Index build cost for users:** bowtie2-build on GRCh38 takes hours and needs `--threads`. Document the one-off cost the plan measures.

---

## Action items

### Critical
1. **Pin the input condition (trimmed vs raw)** and add a **Bowtie 2 `--local`** arm, or explain why not. Report the fraction of pairs with TLEN < read length per arm. Without this, any Bowtie 2 deficit is a trimming × end-to-end artefact. (1.2a)
2. **Add MAPQ and specificity metrics and a specificity guard to the rule:** per-arm MAPQ-0 and MAPQ < 10 fractions, CHG/CHH apparent methylation, and concordance on MAPQ ≥ 10. Switching requires no specificity regression. (1.2c, 4)
3. **Fix the decision metric:** compute accuracy on the common CpG intersection of all arms, use a discordance measure with headroom instead of Δr ≥ +0.002, which is unreachable from 0.998, and freeze and commit the thresholds and the challenger-selection rule before the first challenger run. (4, 1.3)
4. **Prove reference identity with DRAGEN** with the CX-context check (DRAGEN CpG positions are CpG in our FASTA), use one FASTA for both samples (HG002 previously used an Ensembl-named genome), and fix the evaluated contig set. (1.2f)
5. **Assert the engine and options that actually ran** (report "was run with" line, `@PG`, #1138 ancestry), with per-arm directories, one BAM per arm, md5 lineage through every stage, and a `DONE` marker keyed on the commit and arguments. (4)

### Important
6. Resolve Q2 now from `validation/VALIDATION_REAL_DATA.md` (minimap2), add the HG002 baselines to Context, and give Validation 5 a numeric tolerance and a pinned, md5-verified read set. (1.1, 1.2g)
7. Match B''s insert cap to minimap2's actual cap (verify `-F`), or take it from arm A's TLEN; decide whether "Bowtie 2 + a 5-Base `-X` default" is a candidate. (1.2b, 1.3)
8. Handle the dedup soft-clip asymmetry: report per-arm duplicate rate as a confound, and ideally add an unclipped-position dedup sensitivity run. (1.2d)
9. Measure **summed process-tree** peak RSS (cgroup or tree sampling), not `time -v`; record CPU utilisation; investigate minimap2 `-K 250K` on a subsample. (2, 1.2e)
10. Make deconvolution feasible: fold it into the main A and challenger runs, or add a from-BAM entry point; state explicitly that HG002 runs deconvolution. (1.3)
11. Replace the ±5 % join check with a joined-fraction check plus per-contig join counts. (4)
12. Add the missing breaking-change items: output filename token, the startup notice (already wrong for `--bowtie2`), help and error text, CI fixtures, and re-validation of consensus/duplex under a new default. (6)
13. Fix the `mod.rs:223` notice in either branch, since it is already wrong for `--bowtie2`/`--hisat2` runs; and drop the "new test" item in Behaviour 7, because the pin already exists at `config.rs:2255`.

### Optional
14. Run accuracy arms concurrently on disjoint cores and measure cost on a 20M-pair subsample with 3 repetitions; use a CpG-only cytosine report; two-stage depth for Q4. (3)
15. Add a simulated-truth arm and GIAB-truth deconvolution scoring to break the DRAGEN-likeness circularity. (5)
16. Use chromosome block bootstraps for Δ intervals, plus direction checks on 2-3 extra NA12878 libraries at subsample depth. (4)
17. Allow a "fix X, keep minimap2" outcome, and consider the non-breaking alternatives (`--five_base_index` alone selects Bowtie 2; a deprecation cycle) before a hard switch. (5)
18. Correct the Q5 rationale: 5-Base HISAT2 soft-clips and is not "close to Bowtie 2". (1.1)

# Results: 5-Base default engine (screen)

**Decision under `PREREGISTRATION.md`: outcome (a). minimap2 stays the default.**

No challenger qualified at the screen, so no confirm stage runs ("If no arm qualifies, there is no confirm stage, and outcome (a) is decided from the screen").

- **Run:** 2026-10-29/30 on oxy, Bismark `9b5c15b`.
- **Input:** NA12878 (Sample8, 100 ng), the first 100,000,000 pairs of L001+L002. After Trim Galore 2.2.0, 98,082,139 pairs remain, identical in every arm.
- **Retained outputs:** `bench_results/NA12878/screen/`: scores, per-arm metrics and reports. The BAMs and CpG reports were not retained (see Caveats).

## Screen table (NA12878, ~100M pairs)

| Arm | Common-set r (cov ≥ 10, n = 400,760) | Δr vs A | CpGs at cov ≥ 10 | ΔCpG | Rule 1 (Δr ≥ −0.001) | MAPQ-0 % | CHG / CHH % | Gain-set r (guard ≥ common r − 0.02) | Deconv P / R % | Qualifies |
|---|---|---|---|---|---|---|---|---|---|---|
| **A** minimap2 | 0.96464 | — | 817,312 | — | — | 3.69 | 1.3 / 1.1 | — | 94.2 / 61.3 | baseline |
| B Bowtie 2 | 0.96313 | −0.00150 | 554,655 | −32.1% | ✗ | 0.96 | 1.3 / 1.2 | 0.920 ✗ | 93.2 / 58.3 | no |
| B-loc `--local` | 0.96354 | −0.00109 | 580,613 | −29.0% | ✗ | 0.16 | 1.3 / 1.2 | 0.878 ✗ | 88.1 / 56.2 | no |
| B-X `-X 800` | 0.96434 | −0.00030 | 822,432 | +0.6% | ✓ | 0.92 | 1.3 / 1.2 | 0.937 ✗ (< 0.944) | 93.5 / 61.6 | no |
| C HISAT2 | 0.96312 | −0.00152 | 664,790 | −18.7% | ✗ | 0.19 | 1.2 / 1.1 | 0.933 ✗ | 95.2 / 57.7 | no |

B-X would also miss rule 3 ("clearly better": ΔCpG ≥ +2% or Δr ≥ +0.002).

## Read fate and cost

| Arm | Mapping eff. | Duplicates | Read-through R1 (\|TLEN\| < read length) | TLEN p10 / p50 / p90 | Process-tree peak RSS |
|---|---|---|---|---|---|
| A | 95.2% | 3.47% | 801,819 (0.86%) | 241 / 352 / 478 | 15.5 GiB |
| B | 87.9% | 3.18% | 2,937 | 242 / 346 / 445 | 8.4 GiB |
| B-loc | 91.2% | 3.37% | 919,122 (1.03%) | 235 / 344 / 446 | 8.4 GiB |
| B-X | 94.5% | 3.40% | 2,929 | 246 / 353 / 479 | 8.4 GiB |
| C | 91.3% | 3.34% | 446,094 (0.50%) | 243 / 352 / 478 | 8.3 GiB |

Aligner CPU divided by wall time: minimap2 54,957 s over 14,611 s (about 3.8 busy threads of 32), against 11.8–12.7 for the Bowtie 2 arms and HISAT2. These ratios use the same noisy wall times as above, so they are indicative only. minimap2's log shows batches of about 1,780 sequences; whether Bismark's `-K 250K` is the cause is untested. minimap2 built its index in about 69 s.

The screen ran as two concurrent streams, one per NUMA node, on a host whose load average from other tenants peaked at about 1070 during the work. Wall times are therefore excluded, as the plan allows. RSS is per process tree and is not affected by concurrency.

Every arm is concordant with DRAGEN: all-site r ≈ 0.90, joined fraction 96–98% of DRAGEN's 55.6M CpGs. CpG methylation is 46–49% across arms (DRAGEN: 49.7%).

## What the screen shows

- **Bowtie 2's 500 bp insert cap is its main handicap on this library.** The TLEN p90 is about 478 bp, so the cap cuts into the fragment distribution. B loses 7.3 points of mapping efficiency and 32% of the cov ≥ 10 CpGs. `-X 800` (minimap2's own `-F` default) recovers essentially all of it: B-X is within 0.0003 r and +0.6% CpGs of minimap2.
- **Soft-clipping is not the explanation.** `--local` (B-loc) recovers less than `-X 800` and has the lowest own-set and gain-set accuracy and the lowest deconvolution precision.
- **minimap2 has the most MAPQ-0 records (3.7% vs ≤ 1%)** but the best common-set accuracy, and deconvolution recall within 0.3 pt of the best (61.3% vs B-X's 61.6%). Its MAPQ-0 placements don't degrade the methylation calls.
- **HISAT2** loses 19% of the cov ≥ 10 CpGs. Its TLEN distribution matches minimap2's, so the insert cap does not explain the loss; the cause was not investigated.
- **minimap2 uses about twice the peak memory** (15.5 vs 8.4 GiB). The cause was not measured.
- **minimap2 kept far fewer threads busy than Bowtie 2 or HISAT2** (see above). This suggests, but does not show, that its configuration limits its speed.

## Validation checks

| Check | Result |
|---|---|
| Reference: DRAGEN CpGs that are CpGs in our FASTA | NA12878 100.0000%, HG002 100.0000% (1M sampled each) |
| Same input across arms | 98,082,139 pairs analysed in every arm's report |
| Harness determinism (10M dry run, 2 repeats) | byte-identical CpG reports for A and B |
| Scorer vs `concordance.py` (A, 10M) | r agrees to 3 decimals at cov ≥ 10: `concordance.py` 0.7899 (`bench_results/validation4_concordance_A_dry10M.txt`) vs scorer 0.7896. The dry-run score file was lost when oxy's scratch disk was wiped; the scorer value (0.789631, n 9,492, against `concordance.py`'s n 9,501) was observed in the session before the loss and is recorded in the plan notes. The n difference is consistent with the scorer excluding chrY/MT/scaffolds, but can no longer be checked directly. The scorer's self-test now also checks r against `concordance.py`'s reference functions. |
| Options actually applied | each arm's report options line was verified by the harness during the run (the committed `*_PE_report.txt` files show them). The `@PG` command-line check was added after the screen; the screen BAMs were lost with oxy's scratch disk before it could be applied. |
| Joined fraction / per-contig guards | all arms ≥ 0.96; no empty contig |

## Caveats

- **Data retention.** An idle restart of oxy wiped its scratch disk after the screen. That removed the screen BAMs and per-arm CpG reports, which were never copied off the scratch disk.
  - `score.tsv` therefore cannot be recomputed without re-aligning all five arms from the trimmed inputs. The source reads are in `s3://altos-lab-bioinf/CI/Illumina_5base_test_data/`.
  - The committed `score.tsv` was produced by `five_base_compare.py` before the review-round guards were added. Those guards only add aborts and do not change any reported metric.

- **Screen depth.** ~98M pairs is about 4.7× per strand, so cov ≥ 10 covers only about 1.5% of CpGs (0.8M of 55.6M), the high-coverage tail. At cov ≥ 5 (about 22M CpGs) the picture is the same:
  - B-X: r 0.9540 vs A 0.9537, CpGs +1.5%;
  - B: −12%;
  - C: −10%.

  No arm clears +2% at either threshold.
- **The gain-set guard is strict.** Sites that minimap2 under-covers are harder by construction. B-X's gain set scores r 0.937 against a 0.944 threshold. Even without that guard, B-X fails the "clearly better" margin.
- **Scope.** One library type: GIAB WGS gDNA, 100 ng, unaligned PE 2×151. Short-fragment (cfDNA) libraries are not covered.
- **HG002.** Arm A ran on HG002 (95.2% mapping efficiency, 63.2% CpG methylation). It isn't needed now that no challenger qualified, and was not scored against challengers.

## Follow-ups (outside the pre-registered decision)

- **Docs (part of outcome (a)):**
  - name minimap2 as the engine behind the published numbers;
  - add this table;
  - tell users who choose `--bowtie2` to raise the insert cap (for example `-X 800`) on 5-Base libraries.
- **Startup notice fix** (part of outcome (a)): done. The notice names the engine that runs and says paired-end only.
- **Possible separate change, Bowtie 2:** a 5-Base Bowtie 2 default of `-X 800`, perhaps with `--dovetail`. 5-Base omits `--dovetail`, and B and B-X keep only about 2.9k read-through pairs against 0.80M for minimap2, which likely explains most of B-X's remaining 0.7-point mapping gap. This changes an engine default, needs its own decision, and is not part of this pre-registration. The HISAT2 loss has a different cause (see above).
- **Possible separate change, minimap2 throughput:** test a larger `-K` batch for 5-Base runs. In PE mode minimap2 estimates fragment sizes per batch, so a different batch size can change alignments; it needs its own concordance check.

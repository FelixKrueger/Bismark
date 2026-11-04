# Results: 5-Base Bowtie 2 default `--maxins 800`

**Outcome: PASS.** Every check in `PREREGISTRATION.md` rev 2 passes:
- the NA12878 gate;
- both B-old cross-checks;
- HG002 A1–A7;
- the HG002 genome-wide checks.

The criteria were applied by script (`verdict`), not by hand.

The run was on oxy, 2026-11-03 16:13Z to 2026-11-04 00:14Z, with binary `d27b11e` and harness `1c126c3`. Tool versions are in `bench_results/versions_d27b11e.txt`: Bowtie 2 2.5.5, Trim Galore 2.2.0, cutadapt 2.6 and samtools 1.23.1.

## NA12878: the shipped default is #1146's B-X

`repro NA12878 screen B-new` against #1146 B-X, and `repro-compare B-old B-new`. Both are identical on every row (`bench_results/NA12878/screen/repro_B-new.tsv` and `repro_compare_B-old_B-new.tsv`).

| Metric | B-new = #1146 B-X = rev 3 B-old |
|---|---|
| pairs in / unique | 98,082,139 / 92,666,181 (94.5%) |
| CpG / CHG / CHH | 48.6 / 1.3 / 1.2% |
| records / MAPQ-0 | 185,331,762 / 1,697,864 |
| TLEN p10 / p50 / p90 | 246 / 353 / 479 |
| deconvolution | precision 171,619/183,475, recall 100,908/163,702 |
| own-set cov ≥ 10 | 822,432 CpGs, r 0.965834 |

So the default reproduces exactly the configuration #1146 benchmarked. The dev commits since #1146's binary (`9b5c15b`) did not change its output, and neither did `--no_dovetail` (TLEN signs) or `--five_base_umi_qname` (`RX`), as B-old shows.

## HG002: B-new vs the old 500 bp default

**Cross-check:** `repro-compare HG002 screen B-old B-new` is identical on every row, with deconvolution 168,052/179,708 and 95,785/156,944, and an own-set of 704,973 CpGs at r 0.958502.

**Whole-run figures:**

| | B (`-X 500`) | B-new (800) |
|---|---|---|
| unique pairs | 86,108,237 (90.5%) | 89,955,665 (94.5%) |
| CpGs at cov ≥ 10 (own set) | 542,072 | 704,973 (+30.1%) |
| own-set r at cov ≥ 10 | 0.954250 | 0.958502 |
| common-set r (539,826 CpGs) | 0.954633 | 0.955545 |

The gain set (cov ≥ 10 in B-new but not in B) has 165,147 CpGs at r 0.967721.

### Admitted pairs (`rescued B B-new --all-bases`)

The pair sets:

| Set | Pairs | \|TLEN\| p10 / p50 / p90 | CpG % |
|---|---|---|---|
| admitted | 3,847,640 | 506 / 535 / 610 | 63.458 |
| rest | 85,748,254 | 238 / 337 / 435 | 63.587 |
| rest 450–500 bp | 5,946,010 | 453 / 470 / 493 | 63.470 |
| changed | 359,431 | 263 / 478 / 731 | 61.58 |

205 pairs were lost: in B, absent from B-new. Of the admitted pairs, none are dovetailed and 5 are contained.

The criteria:

| Criterion | Value | Threshold | Status |
|---|---|---|---|
| A1 admitted % of B-new pairs | 4.28 | 1–15 | PASS |
| A2 admitted with 500 < \|TLEN\| ≤ 800 | 99.91% | ≥ 95% | PASS |
| A3 CHG (admitted vs rest) | 1.336 vs 1.209 | ≤ rest + 0.5 | PASS |
| A3 CHH | 1.186 vs 1.076 | ≤ rest + 0.5 | PASS |
| A4 CpG | 63.458 vs 63.587 | within ±5 | PASS |
| A5 mismatch % (sample vs control) | 0.742 vs 0.502 (n 86.4M) | ≤ 2× | PASS |
| A5 CHG | 1.333 vs 1.206 (n 4.9M) | ≤ + 1.0 | PASS |
| A5 CHH | 1.188 vs 1.077 (n 16.8M) | ≤ + 1.0 | PASS |
| A6 matched-site agreement, admitted − rest | −0.00042 (1,670,876 sites) | ≥ −0.02 | PASS |
| A7 R1 3′ M-bias shift | 0.38 vs 0.44 | within ±3 | PASS |

The A5 admitted sample was 385,180 pairs and the control 857,768. No cycle was unmatched.

### Genome-wide (B-new vs B)

| Check | Value | Threshold | Status |
|---|---|---|---|
| common-set Δr | +0.000912 | ≥ −0.001 | PASS |
| CHG | 1.2171 vs 1.2133 | ≤ B + 0.2 | PASS |
| CHH | 1.0836 vs 1.0807 | ≤ B + 0.2 | PASS |
| MAPQ-0 | 1.182% vs 1.207% | ≤ B + 2 | PASS |
| deconvolution precision Δ | +0.15 pt | ≥ −1 | PASS |
| deconvolution recall Δ | +1.99 pt | ≥ −1 | PASS |

## Reading

- **The 800 bp cap adds about 4% of pairs on HG002 and about 7% on NA12878.** These are ordinary long fragments, 99.9% of them with 500 < |TLEN| ≤ 800, and they behave like the rest of the library:
  - their non-CpG methylation is within 0.13 pt;
  - their mismatch rate is 0.74% vs 0.50%, well inside the 2× bound;
  - their DRAGEN agreement at shared sites is the same (−0.0004);
  - they have no extra 3′ M-bias.

  This is not the dovetail artefact the rev 3 dry run found, where overhang bases had a 35% mismatch rate.
- **Coverage and accuracy both improve.** HG002 gains 30% more CpGs at cov ≥ 10, and the common-set and own-set r both rise. Deconvolution recall rises 2 pt, because more homozygous CpG-SNVs are covered.
- **Not covered:** cfDNA, other kits and HISAT2, which keeps 500 bp. As in #1146, minimap2 stays the default engine.

## Files

- `bench_results/{NA12878,HG002}/screen/`: per-cell metrics, reports, deconvolution, M-bias, logs and score files; `rescued_B_B-new/rescued.json`; `verdict_B_B-new.tsv`; the `repro*` files.
- `bench_results/NA12878/dry10M/`: the rev 3 dry run (rev 1 criteria; withdrawn, kept as the record of the `--dovetail` decision).
- `bench_results/NA12878/screen/{repro,repro_score}.tsv`: rev 3's B-old `repro`, produced by the rev 3 harness.
- Large outputs (BAMs, CpG reports) stay on oxy's `/var/tmp/5base_engines` until the PR merges.

# Pre-registration: 5-Base default engine

Frozen before any challenger arm has run. The decision follows this file. If a challenger has produced data and this file is then changed, that run is void. Source: `PLAN.md` rev 1, Behavior 1, accepted by Felix on 2026-10-29.

## Fixed setup

- **Bismark:** `dev` at `9b5c15b`, which includes #1138. Built on oxy into `/var/tmp/5base_engines/bin`.
- **Engines:** minimap2 2.31-r1302, Bowtie 2 2.5.5, HISAT2 2.2.2.
- **Trimming:** Trim Galore 2.2.0 (Rust), paired mode with default adapter auto-detection. Run once per sample; every arm uses the same trimmed files.
- **Reference:** Ensembl GRCh38 primary assembly (`Homo_sapiens.GRCh38.dna.primary_assembly.fa`; md5 recorded in `logs/`). The plain Bowtie 2 and HISAT2 indexes are built from this same file. Scoring uses autosomes and X only.
- **DRAGEN references:** `*.CX_report.txt.gz` and `*.hard-filtered.vcf.gz` from the "Illumina 5-Base DNA" demo project (11420409, EUW2). DRAGEN's lambda/pUC19 controls are in a separate `control.CX_report`, so the human reference needs no control contigs.
- **Samples:**
  - NA12878: Demo-5base-gDNA-Sample8-NA12878-100ng-C-D02.
  - HG002: Demo-5base-gDNA-Sample40-NA24385-50ng-B-C02.
- **Screen input:** the first 100,000,000 read pairs of L001 followed by L002, in file order, taken before trimming. Both mates come from the same records. Input md5s are recorded per cell.
- **Threads:** `-p 32`, under `taskset -c 0-31` (32 physical cores on NUMA node 0).
- **Per-arm pipeline:**
  1. `bismark --illumina_5base <arm flags> --five_base_deconvolution -p 32`
  2. `deduplicate_bismark -p --bam`
  3. `bismark_methylation_extractor -p --gzip --cytosine_report` (CpG-only)
  4. scoring with `validation/five_base_compare.py`

## Arms

| Arm | Flags added to `--illumina_5base` | Role |
|---|---|---|
| A | none (minimap2 `-x sr`, FASTA) | current default |
| B | `--bowtie2 --five_base_index <idx>` | challenger |
| B-loc | B + `--local` | soft-clipping control |
| B-X | B + `-X 800` (minimap2 `-F` default 800, confirmed from `minimap2 --help`) | insert-cap control |
| C | `--hisat2 --five_base_index <idx>` | challenger |

B-loc and B-X are diagnostic controls, but they can still win. If one does, the docs would state the switch as "Bowtie 2 with that option".

## Metrics and sets

- **Coverage:** CpG coverage is meth + unmeth on one strand. The thresholds are cov ≥ 1, 5 and 10 in both the arm and DRAGEN.
- **Own set:** sites covered at the threshold in the arm and DRAGEN.
- **Common set:** sites at cov ≥ 10 in every arm of the comparison and in DRAGEN.
- **Gain set** (challenger X): sites at cov ≥ 10 in X and DRAGEN where arm A is below 10.
- **Primary metrics:**
  - common-set Pearson r at cov ≥ 10;
  - CpGs at cov ≥ 10 jointly with DRAGEN (own set).
- **Reported, not deciding:**
  - coverage-weighted r, mean |Δ%|, mean signed Δ% (arm − DRAGEN) and call agreement at 50%;
  - mapping efficiency, proper-pair rate, MAPQ-0 fraction and duplicate rate;
  - read-through fraction, TLEN quantiles and CHG/CHH apparent methylation;
  - wall time, process-tree peak RSS and aligner CPU.

## Decision rule

Switch to challenger X only if all of the following hold on both samples at full depth:

1. Common-set Δr at cov ≥ 10 is ≥ −0.001 (non-inferior).
2. ΔCpG at cov ≥ 10 is ≥ −1% (non-inferior).
3. Δr ≥ +0.002 or ΔCpG ≥ +2% (clearly better).
4. X passes the specificity guards:
   - MAPQ-0 fraction ≤ arm A's + 2 pt;
   - CHG and CHH apparent methylation each ≤ arm A's + 0.2 pt;
   - gain-set r ≥ common-set r − 0.02.
5. Deconvolution precision and recall each have Δ ≥ −1 pt.

Otherwise minimap2 stays. Cost (wall time, RSS) only breaks ties.

## Choosing the challenger from the screen

The challenger is the arm with the largest ΔCpG among those that are non-inferior on r (rule 1) and pass the guards (rule 4).

- Ties go to the larger Δr, then to B over C.
- If no arm qualifies, there is no confirm stage, and outcome (a) is decided from the screen.
- If the only qualifier is B-X or B-loc and its advantage traces to a minimap2 configuration gap, outcome (b) is considered before any switch.

The screen chooses the challenger; it never switches the default.

## Confirm

The confirm stage runs arm A and the chosen challenger on each sample at full depth (all 8 lanes). The rule is applied to these results only.

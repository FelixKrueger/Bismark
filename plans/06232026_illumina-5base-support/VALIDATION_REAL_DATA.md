# 5-Base real-data validation

## Real Illumina 5-Base data (NA12878, BaseSpace) — END-TO-END PE RUN (2026-06-24)

Ran the full pipeline on the **real Illumina 5-Base demo** (NA12878 100ng, BaseSpace),
PE, against the **whole GRCh38** (not chr20-only):

```sh
bismark_rs --illumina_5base --five_base_umi_qname --five_base_consensus \
           --genome <GRCh38> -1 L007_R1 -2 L007_R2   # 10M read pairs (~0.5x)
```

**Results (10M real PE pairs vs full GRCh38; 13 min wall, 60.8 GB peak RAM):**
- **93.7% mapping efficiency** (9,368,173 unique PE alignments) — real 5-Base reads align
  cleanly to the UNCONVERTED human genome, confirming the core design.
- Methylation signature: **CpG 48.2%** vs **CHG 1.3% / CHH 1.1%** — the correct 5-Base
  directional signal (CpG ≫ non-CpG; non-CpG at the ~1% conversion-quality floor, much
  cleaner than the chr20-only run's 3.8% because full-genome mapping removes mismap noise).
- **Duplex pairing OBSERVED on real data:** of 9,147,285 fragment families, **1,123 were
  duplex-paired** (both swapped-UMI partner read-pairs co-occurred at this ~0.5x depth) —
  e.g. `1  6961630-6961939  AAGACAT+ACTAGAT  2+2`. The qname dual-UMI + PE fragment-span
  keying works on real reads.
- **Consensus collapse:** **1,123 consensus reads emitted** (one per paired family, 0
  skipped) into `*_pe.5base_consensus.bam`, each carrying real Bismark `XM` calls (e.g.
  consensus read `dpx:1:6961630-6961939:AAGACAT+ACTAGAT`).

### Concordance vs DRAGEN (the actual gate — DRAGEN output WAS available)

Earlier notes said a DRAGEN comparison was impossible (no reference output). **That was
wrong:** the BaseSpace project ships the **DRAGEN 5-Base complete** AppResult per sample
(`illumina.dragen.complete.v0.4.5`), including the per-CpG `*.CX_report.txt.gz` and
`*.methyl_metrics.csv`. Fetched Sample8's DRAGEN metrics via the `bs` CLI and compared
(DRAGEN = full depth ~490M pairs; ours = 10M-pair subsample):

| Metric | **DRAGEN** | **bismark_rs 5-Base** |
|---|---|---|
| % CpG methylated | 49.73 % | **48.2 %** |
| % CHG methylated | 1.30 % | **1.3 %** |
| % CHH methylated | 1.16 % | **1.1 %** |
| Mapping efficiency | 89.51 % | 93.7 % |
| Strand model | OT/OB only (CTOT/CTOB = 0) | OT/OB only (directional by design) |

The global methylation numbers match DRAGEN closely — CHG/CHH within ~0.06 pt, CpG within
1.5 pt (ours slightly lower: 0.5x subsample, no base-Q masking, no full UMI dedup). The
non-CpG rate sits at DRAGEN's own **lambda unmethylated-control floor (1.35 % CpG /
1.23 % CHH)** — i.e. our noise floor equals DRAGEN's, confirming the 5mC→T polarity and
base handling are right. DRAGEN's **directional-only** strand profile (CTOT/CTOB = 0)
matches our design's directional-only rejection. DRAGEN's puc19 methylated control caps
at 96.91 % CpG (the chemistry's sensitivity ceiling).

### Per-CpG concordance vs DRAGEN — FULL DEPTH (~44×, all 8 lanes, 2026-06-25)

Aligned all 8 lanes (~384M pairs ≈ DRAGEN's depth) to whole GRCh38, extracted a genome-wide
CpG cytosine report (`bismark_methylation_extractor_rs --cytosine_report`), and diffed it
per-CpG against DRAGEN's `CX_report` over **55 million shared CpGs** (autosomes + X). This is
the CORE per-read SE+PE 5-Base path (no duplex/consensus/deconvolution flags).

| cov ≥ (both) | shared CpGs | Pearson r | cov-weighted r | mean \|Δ%\| | call agreement @50% |
|---|---|---|---|---|---|
| 1  | 55,492,552 | 0.9812 | 0.9886 | 2.60 % | 97.16 % |
| 5  | 54,596,475 | 0.9863 | 0.9891 | 2.32 % | 97.40 % |
| 10 | 53,163,111 | 0.9878 | 0.9896 | 2.16 % | **97.53 %** |

Mean % methylation: ours 49.7–50.1 % vs DRAGEN 50.0–50.5 % (within ~0.3 pt). The 5×5
methylation-level confusion is overwhelmingly diagonal (at ≥10×: 16.9M low-low, 17.5M
high-high, ~4.8M on each intermediate diagonal cell, small off-diagonal). At 0.5× this was
r=0.77 (sampling-noise-limited); at full depth it converges to **r≈0.99, 97.5 % call
agreement** — i.e. `bismark_rs --illumina_5base` produces per-CpG methylation calls
**equivalent to DRAGEN** on real Illumina 5-Base NA12878, despite a different aligner
(minimap2 vs DRAGEN). This substantiates the core path at GA grade. (Experimental
duplex/consensus/deconvolution modes are validated separately — see below.)

This run also surfaced + fixed the qname-whitespace desync (commit 4e4f3d4). A deeper run
(more lanes) would yield proportionally more duplex pairs, but this confirms the whole
chain — real FASTQ → unconverted GRCh38 alignment → inverted 5mC call → qname dual-UMI
duplex pairing → per-molecule reconciliation → consensus collapse — works on real data.
The BaseSpace download is still partial (no lane has both R1+R2 complete; used L007 R2 +
R1.part), so a full-coverage run awaits the finished download.

---

## TAPS vs matched WGBS (public-data mechanical equivalent)

**Date:** 2026-06-23. **Honest status:** the pipeline is **validated as correct and
biologically sound on real public data**; it is **not yet a polished publication-grade
per-CpG benchmark** (that needs whole-genome depth + base-quality filtering — see
Findings).

## Why TAPS

There is no public raw **Illumina 5-Base** dataset (launched Oct 2025). The faithful
mechanical equivalent is **TAPS** (TET-Assisted Pyridine borane Sequencing, Liu et al.,
*Nat Biotechnol* 2019) — the same **5mC→T** chemistry. The chosen study is ideal because
it contains **TAPS and matched WGBS of the SAME E14 mESC sample**, so the TAPS calls can
be compared against the field-standard methylation reference.

## Data + commands

- Study **GSE112520 / SRP136786**. TAPS run **SRR8145389** (5,000,000 read-pair subset);
  matched WGBS run **SRR6918157** (2,000,000 read-pair subset). Reference: **mouse GRCm39
  chromosome 19** (61.4 Mb, Ensembl release-110).
- TAPS: `bismark_rs --illumina_5base --five_base_deconvolution --genome <chr19> -1 -2`
  (the default minimap2 `-x sr` unconverted path).
- WGBS: `bismark_genome_preparation_rs` then `bismark_rs --bowtie2` (the faithful
  bisulfite path), same chr19.
- Both extracted with `bismark_methylation_extractor_rs -p --comprehensive --bedGraph`.

## Results

| | TAPS (`--illumina_5base`) | matched WGBS (faithful Bismark) |
|---|---|---|
| unique pairs → chr19 | 523,453 | 45,001 |
| **CpG methylation** | **56.4 %** | **62.7 %** |
| CHG methylation | 2.1 % | 0.5 % |
| CHH methylation | 2.1 % | 0.4 % |
| deconvolution | 1,184 variant CpGs, 37,188 methylation sites | — |

**Concordance (TAPS vs WGBS, shared CpGs):**
- Global CpG mean: **56–61 % (TAPS) vs 61–63 % (WGBS)** — concordant.
- Per-CpG Pearson r: 0.27 at depth ≥5 (n=492 shared CpGs).
- Regional (windowed) Pearson r: **0.50–0.63** (10–100 kb windows), MAD ≈ 9–11 pp.

## What this validates (real data)

1. **The pipeline runs at scale** on real public 5mC→T reads (5M pairs), SE+PE, with the
   deconvolution pass, and the downstream Bismark extractor consumes the BAM unchanged.
2. **The inverted-polarity caller is correct.** Non-CpG methylation is ~2 % (not ~98 %),
   i.e. unmethylated cytosines are correctly called unmethylated; only CpG carries the
   methylation signal — exactly the expected biology. A wrong polarity would invert this.
3. **Global methylation is concordant with matched WGBS** on the same sample (~56–61 %
   vs ~61–63 %), reproducing the TAPS-vs-WGBS agreement the method's paper reports.

## Findings / why it is not yet publication-grade per-CpG

1. **Depth-limited.** Aligning a whole-genome library to a single chromosome wastes ~97 %
   of reads, so per-CpG depth is ~5× at best → binomial sampling noise dominates the
   per-site correlation. A true per-CpG r>0.9 (as the TAPS paper shows) needs
   **whole-genome alignment + high depth**, not a single-chromosome subset.
2. **A real ~2 % non-CpG noise floor** (vs WGBS 0.4 %). MAPQ filtering does not remove it
   (56→57.5 % across MAPQ 0–40), so it is per-base error within otherwise-fine alignments:
   the v1 5-Base caller is **base-quality-naive** (it has no `--methylation-baseq` filter,
   unlike DRAGEN) and applies no adapter/quality trimming. This inflates both the non-CpG
   rate and the per-CpG noise.

## Update: base-quality masking does NOT move this data (honest null result)

`--five_base_baseq 20` was implemented and re-run on the same chr19 TAPS data:
CpG 56.3 %, CHG 2.0 %, CHH 2.1 % — **identical to the no-filter run**. The TAPS reads
are high-quality (Phred ~40 throughout), so a Q<20 mask removes nothing. Therefore the
~2 % non-CpG floor is **NOT low-base-quality sequencing error** (and MAPQ filtering also
did not move it). It is **high-quality-base mapping/chemistry noise** — reads placed with
real mismatches (forced cross-mapping when a whole-genome library is aligned to one
chromosome) and the TAPS chemistry's own false-positive rate. `--five_base_baseq` remains
a sound, DRAGEN-precedented option, but it is not the lever for this dataset.

The real lever is **whole-genome alignment** (reads land at their true locus instead of
being forced onto chr19) plus possibly extending the deconvolution beyond CpG. The
single-chromosome shortcut is the dominant artifact here.

## Concrete next steps for a publication-grade benchmark

- **Whole-genome** alignment (all reads map to their true locus) at adequate depth.
- A **base-quality threshold** in `methylation_call` (skip low-Q read bases; DRAGEN's
  `--methylation-baseq-threshold` precedent) + adapter/quality trimming, to cut the ~2 %
  noise floor.
- Then per-CpG concordance vs matched WGBS at depth ≥10, targeting r>0.9.

*Reproducible: subsets pulled from ENA (`ftp.sra.ebi.ac.uk/vol1/fastq/...`), real
minimap2 2.31-r1302 + Bowtie 2 2.5.5; see the commands above.*

## Experimental modes vs DRAGEN (NA12878, real 5-Base)

### Deconvolution vs DRAGEN's germline VCF

Ran `--five_base_deconvolution` on the real NA12878 lanes and cross-checked our per-CpG
`variant` verdicts against DRAGEN's PASS germline SNVs (a + CpG variant = ref `C` → alt
`T`; a − CpG variant = `G` → `A`):

| depth | our `variant` calls | precision (any DRAGEN C>T/G>A) | recall (DRAGEN HOM, covered) |
|---|---|---|---|
| ~6× (1 lane)   | 120,730 | 92.9 % | 43.4 % |
| ~24× (4 lanes) | 298,041 | 92.0 % | 82.1 % |
| **~40× (8 lanes)** | **409,043** | **90.3 %** | **93.4 %** |

Precision is **~90 %** (when we exclude a CpG as a variant, DRAGEN has a disrupting SNV
there ~90 % of the time); recall climbs steeply with depth (43 % → 82 % → **93.4 %** from
6× to 40×) because the two-strand rule needs the opposite strand covered. So at full depth
the deconvolution **reproduces DRAGEN's variant exclusions** — 90 % precision, 93 % recall
over 167,978 DRAGEN homozygous CpG-disrupting SNVs. **PROVEN** (GA-candidate).

### Consensus — RAM-bounded rewrite (was OOM at WGS depth)

The full-depth run exposed a real scalability bug: the consensus pass held every read's
per-position (base,phred) map for ALL families in memory (~hundreds of GB at WGS depth) →
OOM-killed the process and, run in parallel, panicked the machine twice. Fixed (commit
83ff030) as **two passes** keyed by a compact heap-free key: pass 1 counts OT/OB per family
→ the duplex-PAIRED set; pass 2 builds covered maps ONLY for paired families (~0.1 % of
reads). Output unchanged (3 consensus ground-truth gates still green). **Confirmed at full
depth:** a real lane (~61M pairs) ran the consensus to completion with **peak RSS 26.6 GB**
(was OOM/machine-panic before) — the fix holds.

### Consensus — two reconciliation bugs found vs DRAGEN, both fixed

The first full-depth comparison exposed a **systematic ~2× UNDER-estimate**: mean CpG
methylation **23 % (ours) vs ~48 % (DRAGEN)**, Pearson r = 0.45. Diffing the real reads vs
DRAGEN pinned the cause: the collapse bucketed reads by **coverage strand** (FLAG 0x10), but
at a `+` CpG the OB-molecule's forward-covering mate reads the bottom strand and shows the
unmethylated `C` (measured: **0.8 % T** vs **49 %** for the OT mate) — mixing it into the
"own" bucket halved the 5mC signal. Fix: bucket by **molecule strand** (OT/OB); `consensus_base`'s
`ot`/`ob` params were already molecule-strand by design, only the caller was wrong. Verified
on real reads (chr1:0–5 Mb): `+` CpG methylation **23 % → 47.9 %** (DRAGEN ~48 %).

A second bug surfaced when lifting the old "+ strand only" limitation (the consensus now
emits a **reverse** record too, so the `-`-strand CpG of each dinucleotide is scored): the
reverse record was passed the `+`ref-oriented `cons_seq`, but the emit path expects a
reverse read in **read (5′→3′) orientation** (it revcomps back to `+`ref for the BAM), so
the `-`-strand calls landed at random (chr1 `-` strand r = 0.05). Fix: pass
`revcomp(cons_seq)` + reversed qual for the reverse record.

**After both fixes, the consensus reproduces DRAGEN on BOTH strands** (24×, L001–L004
pooled via `--five_base_consensus_from_bam`, no re-alignment; 707 k duplex-paired families):

| chr1 strand | n | ours | DRAGEN | r | call agree |
|---|---|---|---|---|---|
| `+` | 147,348 | 49.7 % | 50.0 % | **0.769** | 85.0 % |
| `-` | 147,219 | 49.7 % | 49.9 % | **0.769** | 85.1 % |

Per-CpG (both strands merged) vs DRAGEN's CX: r = **0.77** at cov ≥ 1 (1.80 M CpGs), rising
to r = **0.86** at cov ≥ 3 (66,886 CpGs — 18× more than the `+`-only run, since merging
brings each CpG to cov ≥ 2). The cov ≥ 1 r stays ~0.77 because the two cytosines of one CpG
in one molecule encode the SAME methylation event (symmetric) — merging averages measurement
noise but adds no independent biological sampling; the lever for higher r is depth.

So the **consensus is now correct and DRAGEN-validated on both strands** (bias gone, means
match, r 0.77–0.86 at the single-molecule resolution it provides). It went from broken
(r = 0.45, 2× bias, `+` strand only) to validated. The high-resolution methylation path is
still the **core per-read BAM (r ≈ 0.99)**; the consensus is the per-molecule duplex view.

## Reproducible control gate (lambda / pUC19) — no proprietary data

The DRAGEN `CX_report` concordance above is strong evidence but cannot become a CI test:
DRAGEN's output is not redistributable. The reproducible gate that DOES ship uses the
5-Base kit's own **spike-in controls** — **unmethylated lambda** (NC_001416) +
**fully-CpG-methylated pUC19** (L09137) — whose methylation truth is KNOWN (lambda ~0 %
5mC, pUC19 ~100 % CpG 5mC) and whose sequences are PUBLIC. This is the same control standard
the whole methylation-seq field uses (EM-seq/NEB; DRAGEN reports their conversion in
`methyl_metrics.csv`).

`tests/five_base_groundtruth.rs` adds three gates (committed fixtures in `test_files/`,
fail-loud in CI, no DRAGEN):

- **`five_base_controls_core_recovers_lambda_and_puc19`** — the per-read 5-Base call must
  read lambda as **< 2 % 5mC** and pUC19 as **> 95 %** CpG 5mC.
- **`five_base_controls_consensus_preserves_methylation_state`** — the duplex consensus
  collapse must preserve the state on BOTH strands: lambda **< 5 %**, pUC19 **> 90 %**.
- **`five_base_controls_deconvolution_no_false_variants`** — the variant/5mC deconvolution
  must call **zero `variant`** on the controls (they carry 5mC, not C>T SNVs) and recover
  pUC19 ≈ 100 % / lambda ≈ 0 % methylation.

This is the concordance gate the experimental modes (consensus / duplex / deconvolution)
need to graduate out of preview: it locks a known-truth floor with no proprietary data.

## GA graduation: reproducible validation runbook (two real samples)

To back the GA graduation of core + duplex + consensus + deconvolution, the per-CpG
concordance vs DRAGEN is now reproducible via a committed script,
`validation/concordance.py` (pure stdlib, reads plain or `.gz`; `--selftest` checks the
math on a synthetic pair). Run it over TWO real samples: **NA12878** (already local, 8
lanes, ~44x) and **HG002** (the second BaseSpace demo sample, an independent genome).

The runs themselves are operator-driven (BaseSpace credentials + GRCh38 + compute); the
binary is the canonical `bismark` (post #1038).

### 1. Align + call each sample (whole GRCh38, paired-end)

```sh
GENOME=/path/to/GRCh38            # bismark_genome_preparation'd
S=NA12878                         # then repeat with S=HG002
OUT=/scratch/5base/$S

# core per-read path (for the methylation concordance)
bismark --illumina_5base --genome "$GENOME" -1 ${S}_R1.fastq.gz -2 ${S}_R2.fastq.gz \
        --output_dir "$OUT" --temp_dir "$OUT/tmp" --multicore 8
# duplex + consensus (real dual-UMI in the read name)
bismark --illumina_5base --five_base_umi_qname --five_base_duplex   --genome "$GENOME" \
        -1 ${S}_R1.fastq.gz -2 ${S}_R2.fastq.gz --output_dir "$OUT"
bismark --illumina_5base --five_base_umi_qname --five_base_consensus --genome "$GENOME" \
        -1 ${S}_R1.fastq.gz -2 ${S}_R2.fastq.gz --output_dir "$OUT"
# deconvolution (variant vs 5mC)
bismark --illumina_5base --five_base_deconvolution --genome "$GENOME" \
        -1 ${S}_R1.fastq.gz -2 ${S}_R2.fastq.gz --output_dir "$OUT"

# genome-wide CpG cytosine report for the methylation diff (core BAM, and the
# consensus BAM for the per-molecule view)
bismark_methylation_extractor -p --comprehensive --cytosine_report \
        --genome_folder "$GENOME" "$OUT"/*_pe.bam
```

### 2. Fetch the matching DRAGEN reference outputs (BaseSpace `bs` CLI)

Each demo sample ships a DRAGEN `illumina.dragen.complete` AppResult. Pull the per-CpG
report, the global metrics, and the germline VCF:

```sh
bs download dataset --id <ds.id-for-$S> --extension CX_report.txt.gz    -o "$OUT/dragen"
bs download dataset --id <ds.id-for-$S> --extension methyl_metrics.csv  -o "$OUT/dragen"
bs download dataset --id <ds.id-for-$S> --extension hard-filtered.vcf.gz -o "$OUT/dragen"
```

### 3. Compute concordance

```sh
# methylation: core (and again with the consensus cytosine report) vs DRAGEN CX
python3 validation/concordance.py methyl \
    --ours "$OUT"/*_pe.CX_report.txt --dragen "$OUT"/dragen/*.CX_report.txt.gz

# deconvolution: our per-CpG variant verdicts vs DRAGEN's germline SNVs
python3 validation/concordance.py deconv \
    --variants "$OUT"/*_pe.5base_deconvolution.txt --vcf "$OUT"/dragen/*.vcf.gz
```

`methyl` prints, at coverage >= 1/5/10: Pearson r, coverage-weighted r, mean |delta %|,
call-agreement at 50%, and a 5x5 confusion matrix. `deconv` prints precision (our
`variant` CpGs coinciding with a DRAGEN C>T/G>A SNV) and recall (DRAGEN homozygous
CpG-disrupting SNVs we cover and flag). The reference numbers to reproduce are the
existing NA12878 full-depth results above (core r ~ 0.98 / 97.5% call agreement;
deconvolution 90.3% / 93.4% at ~40x).

### GA graduation evidence: two real samples

Paste the per-sample, per-mode results here once the runs land (NA12878 first as the
self-consistency check, then HG002 as the independent second sample):

| sample | mode | depth | Pearson r (cov>=10) | call agree @50 | precision | recall |
|---|---|---|---|---|---|---|
| NA12878 | core | ~44x | _pending_ | _pending_ | n/a | n/a |
| NA12878 | consensus | ~44x | _pending_ | _pending_ | n/a | n/a |
| NA12878 | deconvolution | ~40x | n/a | n/a | _pending_ | _pending_ |
| HG002 | core | _pending_ | _pending_ | _pending_ | n/a | n/a |
| HG002 | consensus | _pending_ | _pending_ | _pending_ | n/a | n/a |
| HG002 | deconvolution | _pending_ | n/a | n/a | _pending_ | _pending_ |

Two independent real samples reproducing DRAGEN to this level is the evidence the GA
concordance contract stands on (alongside the deterministic lambda/pUC19 gates above).

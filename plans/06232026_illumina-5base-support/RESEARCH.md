# Illumina 5-Base support in Bismark: deep-research feasibility study

**Issue:** [FelixKrueger/Bismark#787](https://github.com/FelixKrueger/Bismark/issues/787) ("Illumina 5-Base Support")
**Date:** 2026-06-23
**Branch:** `research/illumina-5base`
**Status:** research only (no code). Input for a future design/EPIC decision.
**Method:** deep-research pipeline (ARS), 3 parallel source-gathering agents (chemistry, DRAGEN data format, non-DRAGEN tooling) + synthesis + devil's-advocate pass. Every factual claim is linked to a primary source.

---

## 0. TL;DR for the maintainer

1. **Illumina 5-Base is the chemical inverse of bisulfite.** A bespoke Illumina enzyme converts **5mC -> T** in one step and leaves **unmethylated C as C** (bisulfite/EM-seq convert *unmethylated* C -> T). Because only the sparse methylated cytosines change, **4-base library complexity is preserved**, so reads align to the **unconverted reference** with ordinary aligners.
2. **The issue author's "plug the reference into Bismark" failed** because Bismark's entire model (3-letter C->T / G->A converted genome + converted reads + "read C at ref C = methylated") assumes the opposite chemistry. It is a *semantic inversion*, not a parameter mismatch.
3. **But DRAGEN already emits Bismark-convention tags.** DRAGEN performs the inversion internally and writes `XM:Z` / `XR:Z` / `XG:Z` with **identical Bismark semantics and polarity** (uppercase `Z` = methylated CpG, lowercase `z` = unmethylated; `XR`/`XG` = `CT`/`GA`). It also writes a **`CX_report` byte-shaped like `coverage2cytosine --CX`**. So a Bismark-style **extractor/reporting** path can consume DRAGEN 5-Base BAMs with **no polarity flip**. The flip is only needed if re-deriving calls from raw reads.
4. **The aligner is a solved, external problem.** DRAGEN and the published TAPS stack both align with standard aligners (`bwa-mem`) to the native genome. Bismark's converted-genome aligner is the wrong tool here and should not be touched.
5. **A mature open-source precedent exists: TAPS** (same 5mC->T direction). It is analyzed without DRAGEN via `bwa-mem` + **rastair** (Rust, does variant-vs-methylation deconvolution, integrated in nf-core/methylseq `--aligner bwamem --taps`). **Caveat: rastair's official distribution is non-commercial / academic-only licensed.**
6. **Recommended Bismark contribution shape:** a 5-Base **ingestion/reporting mode** that reuses Bismark's downstream (extractor -> bedGraph -> coverage2cytosine -> report) over an already-tagged 5-Base BAM, as a **concordance-gated, never-silent, opt-in** path (the `--rammap` / `--combined_index` precedent), leaving the byte-frozen bisulfite aligner untouched. The full no-DRAGEN caller (raw-read 5mC-vs-variant deconvolution) is a larger, separate effort better served by leaning on / interoperating with rastair than by rebuilding it.

---

## 1. The chemistry

### 1.1 Mechanism

- A **single proprietary enzyme engineered in-house at Illumina** performs **direct, single-step conversion of 5mC -> sequencer-ready thymine** (reaction ~30 min), leaving **unmodified C as C**. [Illumina 5-base solution](https://www.illumina.com/science/genomics-research/articles/5-base-solution.html)
- Vendor-reported performance (control genomes): **~95-96% conversion of 5mC** (methylated pUC19) and **~0.4-0.9% off-target conversion** of unmethylated C (lambda). [Illumina 5-base solution](https://www.illumina.com/science/genomics-research/articles/5-base-solution.html)
- Mechanistic class is **inferred** to be a 5mC-selective cytidine deaminase (cf. the NEB 5mC-selective deaminase family, Molecular Cell 2026 / [bioRxiv 2024.12.05.627091](https://www.biorxiv.org/content/10.1101/2024.12.05.627091v1), [NEB patent US20250115953A1](https://patents.google.com/patent/US20250115953A1/en)). Illumina has **not** publicly named or published its enzyme, so treat "deaminase" as a well-supported inference, not a confirmed identity.

### 1.2 5mC vs 5hmC: 5-Base is effectively 5mC-only

- Illumina states the enzyme is "**very specific to converting 5mC, and has minimal activity on 5hmC**" -> 5hmC largely **reads as C**. [Illumina 5-Base FAQ 000009939](https://knowledge.illumina.com/library-preparation/multiomics-library-prep/library-preparation-multiomics-library-prep-faq-list/000009939)
- **This is a real divergence from TAPS.** TAPS (and Watchmaker TAPS+) convert **both 5mC and 5hmC -> T** (a combined readout). [Liu et al. 2019, Nat Biotechnol](https://www.nature.com/articles/s41587-019-0041-2), [Watchmaker TAPS+ note](https://www.watchmakergenomics.com/media/wg/asset//m/4/m417_taps_data_analysis_tn_wmtn003_v1-0-1125.pdf)
- **Consequence:** the *calling mechanics* transfer from TAPS tooling, but the *biological meaning* differs (5-Base ~ 5mC; TAPS ~ 5mC+5hmC; classic WGBS/EM-seq ~ 5mC+5hmC combined). Any Bismark 5-Base output should be labeled 5mC, and the exact residual-5hmC fraction is unpublished (open question).

### 1.3 The dual-strand variant-vs-methylation logic

Because a converted 5mC reads as T, it is ambiguous with a true **C>T SNV**. Resolution uses the complementary strand:

> "For a methylated cytosine, the pipeline expects the complementary base to be a **guanine**, whereas for a C>T variant, the complementary base is **adenosine**." [Illumina 5-base solution](https://www.illumina.com/science/genomics-research/articles/5-base-solution.html)

- Genuine 5mC: base pair is still C:G, so opposite strand reads **G** (T-opposite-G = methylation).
- True C>T SNV: genotype is T:A, so opposite strand reads **A** (T-opposite-A = variant).

This is structurally similar to Bismark's own-strand vs pair-strand routing (OT/OB/CTOT/CTOB), but the *decision rule is inverted* and *requires the complementary base* -> a duplex/pair-aware caller, not a single-strand C->T model.

### 1.4 Comparison table

| Property | Bisulfite (WGBS) | EM-seq | **Illumina 5-Base** | TAPS / TAPS+ |
|---|---|---|---|---|
| Conversion direction | unmethylated C -> T | unmethylated C -> T | **5mC -> T** | 5mC+5hmC -> T |
| What stays C | 5mC(+5hmC) | 5mC(+5hmC) | **unmethylated C** | unmethylated C |
| Reports | 5mC+5hmC | 5mC+5hmC | **5mC only** (5hmC ~ C) | 5mC+5hmC |
| Library complexity | low | low | **high** | high |
| Same-assay variants | poor | poor | **high (built-in)** | yes (rastair) |
| Aligner | 3-letter (Bismark) | 3-letter | **standard, unconverted** | standard, unconverted |

Sources: [Illumina 5-Base flyer M-GL-03401](https://www.illumina.com/content/dam/illumina/gcs/assembled-assets/marketing-literature/illumina-5-base-methylation-flyer-m-gl-03401/illumina-5-base-methylation-flyer-m-gl-03401.pdf), [Illumina methylome-genome](https://www.illumina.com/techniques/sequencing/methylation-sequencing/methylome-genome.html).

### 1.5 Lineage

Three independent chemistries all achieve mC->T: **TAPS** (TET + pyridine borane; Oxford -> Exact Sciences / Watchmaker), **biomodal duet evoC** (copy-strand + protect-then-deaminate, 6-base; [Nat Biotechnol 2023](https://www.nature.com/articles/s41587-022-01652-0)), and **Illumina 5-Base** (single proprietary enzyme). No public evidence Illumina licensed TAPS or evoC; the launch and FAQ name neither. Illumina 5-Base launched **2025-10-15** ([PRNewswire](https://www.prnewswire.com/news-releases/illumina-fuels-multiomic-discovery-with-launch-of-5-base-solution-unlocking-simultaneous-genomic-and-epigenomic-insights-302584803.html)), so the independent-benchmark and community-tooling ecosystem is nascent.

---

## 2. The data format (DRAGEN)

### 2.1 Pipeline activation

Master flag **`--methylation-conversion=illumina`** (5-base mode) auto-sets a bundle:
`--enable-cpg-methylated-mapping=true`, `--enable-methylation-calling=true`, `--vc-enable-methylation=true`, `--umi-enable-methylation=true`, `--umi-library-type=nonrandom-duplex`, `--methylation-protocol=directional`, `--methylation-generate-mbias-report=true`. [DRAGEN 5-base v4.5](https://help.dragen.illumina.com/dragen-v4.5/product-guides/dragen-v4.5/dragen-5-base/dragen-5base-pipeline.md), [v4.4](https://help.dragen.illumina.com/product-guide/dragen-v4.4/dragen-methylation-pipeline/dragen-5base-pipeline)

A dedicated **`methyl_cg` reference hash** (Reference Builder v4.4.4+, "Include Methylation Data") is mandatory. [Analysis launch FAQ 000009950](https://knowledge.illumina.com/software/dragen/software-dragen-faq-list/000009950)

DRAGEN explicitly warns its **bisulfite** Methylation pipeline is wrong for 5-Base ("where methylated C are converted to T"). [FAQ 000009950](https://knowledge.illumina.com/software/dragen/software-dragen-faq-list/000009950)

### 2.2 Mapping & calling

- Maps to the **unconverted reference** via the `methyl_cg` hash; C>T tolerated at **seed mapping and alignment scoring**; **local alignment, soft-clipping, graph genomes ON by default** (unlike the bisulfite pipeline). Directional protocol = 2 alignment runs. [DRAGEN 5-base v4.5](https://help.dragen.illumina.com/dragen-v4.5/product-guides/dragen-v4.5/dragen-5-base/dragen-5base-pipeline.md)
- Calling rule: "**Methylation is primarily identified by reference C>T mismatches on the + strand, or G>A mismatches on the - strand**," then variant calling "deconvolutes methylation and variant status." [DRAGEN 5-base v4.4](https://help.dragen.illumina.com/product-guide/dragen-v4.4/dragen-methylation-pipeline/dragen-5base-pipeline)
- Read1/Read2 overlap bases are reported in the BAM but **excluded from quantification** (same principle as Bismark's extractor overlap handling). [DRAGEN 5-base v4.5](https://help.dragen.illumina.com/dragen-v4.5/product-guides/dragen-v4.5/dragen-5-base/dragen-5base-pipeline.md)

### 2.3 BAM tags (the load-bearing finding)

Tags "**follow Bismark conventions**" ([Methylation BAM tags, DRAGEN v4.2](https://support-docs.illumina.com/SW/dragen_v42/Content/SW/DRAGEN/MPipelineMethBAM_fDG.htm)):

| Tag | Meaning |
|---|---|
| `XM:Z` | byte-per-base methylation string |
| `XR:Z` | read conversion: `CT` or `GA` |
| `XG:Z` | reference conversion: `CT` or `GA` |

`XM` character set is **identical to Bismark**: `.` non-C; `z`/`Z` un/methylated CpG; `x`/`X` CHG; `h`/`H` CHH; `u`/`U` unknown; **uppercase = methylated** (same polarity as Bismark). [DRAGEN Methylation v4.5](https://help.dragen.illumina.com/dragen-v4.5/product-guides/dragen-v4.5/dragen-methylation-pipeline/dragen-methylation-pipeline.md)

- Tags written for **MAPQ > `--methylation-mapq-threshold`, proper-pair, uniquely-aligned** reads.
- **Duplex consensus reads carry XM for both + and - strands in one string** (a 5-base extension beyond Bismark; strand ordering of that combined string is **undocumented** = reproducibility risk).

**Key reconciliation (corrects the earlier quick brief): a Bismark-style extractor reading these tags needs NO polarity flip. The flip is only required when re-deriving calls from the raw read vs reference.**

### 2.4 Other outputs

- **`CX_report`** (`--methylation-generate-cytosine-report=true`): columns chrom, pos, strand, meth-count, unmeth-count, context (CG/CHG/CHH), tri-nucleotide context. **Same shape as Bismark `coverage2cytosine --CX`.** Optional gzip. [DRAGEN 5-base v4.5](https://help.dragen.illumina.com/dragen-v4.5/product-guides/dragen-v4.5/dragen-5-base/dragen-5base-pipeline.md)
- **M-bias report** (`*.m-bias.txt`), **`*.methyl_metrics.csv`** (CpG/CHG/CHH counts+percentages), analogous to Bismark.
- Methylation also folded into **VCF/gVCF** (`INFO/FORMAT:M5mC`, `FORMAT:DPM5mC`) - Bismark has no analogue.
- **Silent** on any `.bedGraph`/`.cov` equivalent (CX_report is the closest).

### 2.5 FASTQ / read structure

- **7 bp inline UMI + 1 spacer base at the start of BOTH R1 and R2**, dual 10 bp indexes, Nextera adapter `CTGTCTCTTATACACATCT`. SampleSheet: `OverrideCycles: U7N1Y#;I10;I10;U7N1Y#`, `TrimUMI: 1`. [DRAGEN 5-base v4.5](https://help.dragen.illumina.com/dragen-v4.5/product-guides/dragen-v4.5/dragen-5-base/dragen-5base-pipeline.md), [UMI/index FAQ 000009942](https://knowledge.illumina.com/library-preparation/multiomics-library-prep/library-preparation-multiomics-library-prep-faq-list/000009942)
- A non-DRAGEN pipeline must demux dual indexes, strip/record the 7 bp UMI + 1 N, then treat chemistry as 5mC->T, and (for the duplex path) pair read-pairs with swapped UMIs + complementary orientation at the same locus. [DRAGEN UMI v4.5](https://help.dragen.illumina.com/dragen-v4.5/product-guides/dragen-v4.5/dragen-dna-pipeline/unique-molecular-identifiers.md)

---

## 3. Non-DRAGEN tooling landscape

| Tool | Role | Aligner | Convention | Contexts | Lang | License | Status |
|---|---|---|---|---|---|---|---|
| **rastair** ([bsblabludwig](https://bitbucket.org/bsblabludwig/rastair)) | meth + variant caller (deconvolution) | bwa-mem (unconverted) | 5mC->T | **CpG only** | Rust | **NonCommercial/academic** (GH mirror says AGPL-3.0) | Active (v2.1.1; [2026 preprint](https://www.biorxiv.org/content/10.64898/2026.03.19.712983v1)) |
| **asTair** ([repo](https://github.com/1156054203/astair)) | TAPS caller, `mCtoT`/`CtoT` modes | any BAM/CRAM | 5mC->T | CpG/CHG/CHH | Python | GPL-3.0 | unmaintained, slower |
| **MethylDackel** ([repo](https://github.com/dpryan79/MethylDackel)) | BS-seq extractor | any | **bisulfite only** | all | C | MIT | active; TAPS only via external "flip" step |
| **jknightlab/TAPS-pipeline** ([repo](https://github.com/jknightlab/TAPS-pipeline/)) | bwa-mem + MethylDackel + flip script | bwa-mem | 5mC->T (via flip) | - | Bash | n/s | active |
| **modkit** ([repo](https://github.com/nanoporetech/modkit)) | nanopore MM/ML tags | - | n/a | - | Rust | - | not applicable |
| DRAGEN | reference (proprietary) | self | 5mC->T | all | - | proprietary | - |

Key points:
- **`bwa-mem` against the unconverted genome is the proven aligner** for 5mC->T data (Watchmaker TAPS+ workflow: `bwa mem | samtools sort | gatk MarkDuplicates | rastair`). [Watchmaker note](https://www.watchmakergenomics.com/media/wg/asset//m/4/m417_taps_data_analysis_tn_wmtn003_v1-0-1125.pdf), [nf-core/methylseq](https://nf-co.re/methylseq/usage)
- **rastair** does the variant-vs-methylation deconvolution, reports beta in `M5mC`, F1 > 0.99 at >=30x and ~ parity with DRAGEN ([preprint](https://www.biorxiv.org/content/10.64898/2026.03.19.712983v1)). It is **CpG-only** (a gap vs Bismark's CpG/CHG/CHH).
- **No tool exists built specifically for Illumina 5-Base** re-analysis; all open-source effort targets TAPS and is reused by chemistry equivalence. No one has publicly run nf-core/methylseq on real 5-Base FASTQ yet (launched Oct 2025).
- **Minimal-change inversion model in the wild:** MethylDackel + a column-flip script (jknightlab `8_filter-and-flip-methylation-calls.sh`).

### 3.1 License caveat (decision-relevant)

rastair's **official** distribution (rastair.com / bioconda v2.1.1) is **"free for academic and non-commercial use; commercial use -> contact Oxford"** ([rastair.com](https://www.rastair.com/), [bioconda](https://anaconda.org/bioconda/rastair) license `LicenseRef-NonCommercial`); the GitHub mirror's README says AGPL-3.0. Bismark is GPL-3.0. **Borrowing rastair code or hard-depending on it has licensing implications** that must be cleared with Oxford before any vendoring. asTair (GPL-3.0) is the license-compatible but unmaintained, slower alternative.

---

## 4. Feasibility for the Bismark suite

### 4.1 Split the problem

| Stage | 5-Base prospect | Why |
|---|---|---|
| Genome prep (C->T/G->A index) | **N/A / skip** | 5-Base aligns to the unconverted genome |
| Aligner (`bismark_rs`) | **out of scope** | solved by standard external aligners; Bismark's converted aligner is the wrong model; stays byte-frozen |
| Methylation extractor | **reusable** | reads XM/XR/XG; DRAGEN tags are Bismark-convention, no flip |
| bedGraph / coverage2cytosine / report | **reusable** | DRAGEN CX_report already matches `coverage2cytosine --CX` |
| Raw-read 5mC-vs-variant caller | **new, hard** | needs duplex/pair complementary-base deconvolution (rastair territory) |

### 4.2 Three contribution options

**Option A - Extractor ingestion of tagged 5-Base BAMs (smallest, highest-confidence).**
A never-silent `--five_base` (or `--illumina_5base`) mode in `bismark-extractor` that accepts a BAM already carrying Bismark-convention XM/XR/XG (e.g., DRAGEN output, or any future tagger), and runs the existing extractor -> bedGraph -> coverage2cytosine -> report unchanged. Because the tags are already Bismark-polarity, this is mostly **validation + guardrails** (detect 5-Base provenance, refuse silent misuse, label output as 5mC). Gives users a no-DRAGEN **reporting** path immediately and validates the downstream half. Fits the byte-frozen constraint trivially (the aligner is untouched).
- Risk: depends on a 5-Base BAM that has the tags. DRAGEN BAMs have them; a pure-FASTQ user still needs an aligner+tagger (Option C).

**Option B - Standalone tagger: align + tag, no Bismark aligner.**
A thin pre-step (or new subcommand) that takes a standard-aligner BAM (bwa-mem/bowtie2/minimap2 to the unconverted genome) and writes Bismark-convention XM/XR/XG by comparing read vs reference with the **inverted** rule (C>T on + / G>A on - = methylated), feeding Option A. This is the "Bismark without DRAGEN" core. The hard part is the **variant-vs-methylation deconvolution** (needs the complementary-strand / duplex logic). Without deconvolution it over-calls methylation at C>T SNVs.
- This is essentially re-implementing rastair's core. Strongly consider interoperating with / wrapping rastair instead, subject to the license caveat.

**Option C - Full first-class 5-Base pipeline (largest).**
UMI handling (7bp+spacer), duplex consensus, deconvolution, multi-context (CpG/CHG/CHH, which rastair lacks), reports. This is an EPIC, overlaps heavily with DRAGEN and rastair, and should only be scoped after Option A proves the downstream and a real dataset confirms the format.

### 4.3 Constraint compliance (CLAUDE.md)

- **Byte-frozen bisulfite paths untouched:** all options leave the Bowtie2/HISAT2/minimap2 faithful aligner and the bisulfite extractor logic byte-identical. The 5-Base mode is a **separate, opt-in, never-silent, concordance-gated** path, exactly the `--rammap` / `--combined_index` precedent.
- **Oracle:** there is **no Perl-v0.25.1 oracle for 5-Base** (Perl Bismark never supported it). So byte-identity-to-Perl does not apply; the validation target shifts to **concordance with DRAGEN** (and/or rastair) on a shared dataset, documented as a gate, the same way `--rammap` is concordance-gated rather than byte-frozen.

---

## 5. Validation data

- **Illumina 5-Base (gated):** BaseSpace demo data, NA12878/HG001 + NA24385/HG002 on NovaSeq X, DRAGEN v4.4.6. Requires a free Illumina account; **no SRA/ENA/GEO accession** as of June 2026. [BaseSpace datasets](https://blog.basespace.illumina.com/category/datasets/), [download help](https://help.basespace.illumina.com/manage-data/download/download-datasets)
- **TAPS public fallback (recommended for byte/concordance dev now):** GEO **GSE112520** / SRA **SRP136786** / BioProject **PRJNA448064** (Liu et al. 2019, mESC WG-TAPS). [GEO GSE112520](https://www.ncbi.nlm.nih.gov/geo/query/acc.cgi?acc=GSE112520)
- NA12878 genotype truth for variant-deconvolution checks: GIAB / ENA ERR194147.

**Caveat:** TAPS converts 5mC+5hmC; 5-Base converts 5mC only. TAPS data validates the *pipeline mechanics* but not the 5mC-only biology. A real 5-Base dataset is needed before claiming 5-Base concordance.

---

## 6. Devil's-advocate / risks

1. **"Just read DRAGEN tags" assumes users have DRAGEN.** The accessibility complaint in #787 is precisely that 5-Base is locked behind DRAGEN. Option A alone (BAM ingestion) does not free users from DRAGEN; only Option B/C do. Be honest about this in any reply: Option A is a quick win for the *reporting* half, not the liberation the user asked for.
2. **Duplex/UMI is not optional in DRAGEN's design** (`nonrandom-duplex` required) and the deconvolution depends on it. A naive single-read caller will mis-handle C>T SNVs and hemi-methylation. Underestimating this is the main technical trap.
3. **rastair license** could block code reuse; the only open multi-context caller (asTair) is unmaintained. Building from scratch is real work.
4. **No independent 5-Base benchmark yet** (Oct-2025 launch) and **no public raw dataset**; the on-disk FASTQ/BAM specifics (duplex XM string ordering, residual 5hmC) are partly undocumented. Committing to a byte-level design before holding a real dataset is premature.
5. **Vendor-self-reported numbers** (95% conversion, >99% variant precision) are not independently verified here.
6. **Maintenance scope:** 5-Base is one of several mC->T chemistries (TAPS, evoC). A `--five_base` flag may invite "support TAPS / evoC too." Consider naming/architecting the mode around the **conversion convention (mC->T)** rather than the vendor, so it generalizes.

---

## 7. Recommendation

1. **Reply to #787** summarizing this study (the chemistry inversion, the DRAGEN-tags-are-Bismark-convention finding, the TAPS/rastair precedent, and the honest accessibility caveat). Benjamin posts it (only `pull` on upstream).
2. **Acquire a real artifact** before any code: pull a BaseSpace 5-Base demo BAM (NA12878) to confirm tag presence/format, and grab TAPS GSE112520 for mechanics dev.
3. **If green-lit, start with Option A** (extractor ingestion of tagged 5-Base BAMs) as a concordance-gated, never-silent opt-in. It is small, testable against DRAGEN's CX_report, and risk-free for the byte-frozen paths.
4. **Defer Option B/C** (the real no-DRAGEN caller) to a separate EPIC, and first evaluate interoperating with rastair (resolve license with Oxford) vs building a multi-context deconvolution caller in-house.
5. **Architect around the mC->T convention**, not the Illumina brand, so TAPS/evoC can share the path.

---

## 8. Open questions to resolve before design freeze

- Exact byte layout of a real 5-Base FASTQ header/UMI and BAM (duplex XM string strand ordering).
- Residual 5hmC conversion fraction (affects 5mC-only labeling accuracy).
- rastair licensing for any reuse/interop (Oxford).
- Whether Felix wants vendor-specific (`--illumina_5base`) or convention-generic (`--mc_to_t`) framing.
- Concordance gate definition: against DRAGEN CX_report? rastair BED? what tolerance?

---

## Appendix: full source list

See the three agent dossiers' Sources sections (chemistry, DRAGEN format, tooling). Primary anchors:
- [Illumina 5-base solution](https://www.illumina.com/science/genomics-research/articles/5-base-solution.html) · [5-Base FAQ 000009939](https://knowledge.illumina.com/library-preparation/multiomics-library-prep/library-preparation-multiomics-library-prep-faq-list/000009939) · [Analysis FAQ 000009950](https://knowledge.illumina.com/software/dragen/software-dragen-faq-list/000009950)
- [DRAGEN 5-base v4.5](https://help.dragen.illumina.com/dragen-v4.5/product-guides/dragen-v4.5/dragen-5-base/dragen-5base-pipeline.md) · [DRAGEN Methylation v4.5](https://help.dragen.illumina.com/dragen-v4.5/product-guides/dragen-v4.5/dragen-methylation-pipeline/dragen-methylation-pipeline.md) · [BAM tags v4.2](https://support-docs.illumina.com/SW/dragen_v42/Content/SW/DRAGEN/MPipelineMethBAM_fDG.htm) · [UMI v4.5](https://help.dragen.illumina.com/dragen-v4.5/product-guides/dragen-v4.5/dragen-dna-pipeline/unique-molecular-identifiers.md)
- [TAPS, Liu et al. 2019](https://www.nature.com/articles/s41587-019-0041-2) · [rastair](https://www.rastair.com/) · [rastair preprint 2026](https://www.biorxiv.org/content/10.64898/2026.03.19.712983v1) · [nf-core/methylseq](https://github.com/nf-core/methylseq) · [Watchmaker TAPS+ note](https://www.watchmakergenomics.com/media/wg/asset//m/4/m417_taps_data_analysis_tn_wmtn003_v1-0-1125.pdf) · [MethylDackel](https://github.com/dpryan79/MethylDackel) · [asTair](https://github.com/1156054203/astair) · [jknightlab/TAPS-pipeline](https://github.com/jknightlab/TAPS-pipeline/)
- [GSE112520 (public TAPS)](https://www.ncbi.nlm.nih.gov/geo/query/acc.cgi?acc=GSE112520)

*AI-assisted research: gathered and synthesized with AI tooling; every factual claim is linked to a primary source for independent verification. Vendor performance figures are self-reported and not independently validated here.*

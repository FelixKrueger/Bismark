# 5-Base v1 (`--illumina_5base`) — implementation status + concordance gate

**Issue:** [FelixKrueger/Bismark#787](https://github.com/FelixKrueger/Bismark/issues/787)
**Branch:** `research/illumina-5base`
**Research dossier:** `RESEARCH.md` (same folder)
**Status:** v1 walking skeleton implemented (FROM FASTQ, SE, directional). Concordance gate vs DRAGEN: **PENDING** (needs a real dataset).

## What v1 ships

A new opt-in, never-silent, concordance-gated `--illumina_5base` (alias `--five_base`) path in `bismark-aligner`:

1. Aligns the RAW reads with **minimap2 `-x sr`** against the **unconverted** reference FASTA (no C→T/G→A conversion, no `.mmi` build; multi-FASTA genomes concatenated once to a temp).
2. Derives strand from the SAM FLAG (0 → OT / 0x10 → OB), reuses `extract_corresponding_genomic_sequence_single_end` + `single_end_sam_output`.
3. Calls methylation with **inverted polarity** via `methylation_call(..., five_base = true)`: a read `T` at a genomic C (or `A` at a genomic G on the GA branch) is **methylated** (5mC→T), the chemical inverse of bisulfite.
4. Emits a standard Bismark-convention BAM (`XM`/`XR`/`XG`) + SE report, consumed unchanged by the extractor / bedGraph / coverage2cytosine.

### Commits (on `research/illumina-5base`)

- `feat(aligner): add five_base polarity flag to methylation_call (#787)`
- `feat(aligner): add --illumina_5base flag + config + v1 scope guards (#787)`
- `feat(aligner): 5-Base SE driver — align unconverted + inverted call (#787)`
- `test(aligner): 5-Base SE end-to-end FROM FASTQ + scope guards (#787)`

### Tests (all green, hermetic)

- Unit: `methylation_call` CT/GA inversion (Z↔z, X↔x, H↔h); `five_base=false` byte-identical to the frozen path.
- Unit: `five_base_emit_record` (forward me/unme CpG, unmapped) + the primary-line reader.
- Config: flag/alias parse, resolves to minimap2, rejects other engines.
- End-to-end (fake minimap2): FASTQ → unconverted align → inverted call → BAM `XM = .Z...z`; `mm2` naming; `-x sr` option string; scope-guard rejection.

## NOT byte-identical (by design)

Perl Bismark v0.25.1 has no 5-Base path, so byte-identity-to-Perl does not apply. The faithful Bowtie2/HISAT2/minimap2 bisulfite paths stay byte-frozen (every existing `methylation_call` site passes `five_base = false`; 431 lib + 100 integ + 3 conformance unchanged).

## Concordance gate (PENDING)

Target: per-CpG methylation concordance with **DRAGEN's 5-Base `CX_report`** on a shared dataset, within a documented tolerance.

- **Data:** Illumina 5-Base is currently only in BaseSpace demo (NA12878/HG002, gated); no public SRA/ENA raw yet (launched 2025-10-15). Public TAPS (GSE112520) validates the mechanics (5mC→T) but not the 5mC-only biology (TAPS = 5mC+5hmC).
- **Procedure (to run when a dataset is in hand):**
  1. `bismark_rs --illumina_5base --genome <ref> reads.fq` → BAM.
  2. `bismark_methylation_extractor_rs --cytosine_report` → CX report.
  3. Diff per-CpG % methylation vs DRAGEN's CX report; record divergence here.

## Deferred follow-up phases (rejected loudly in v1)

Paired-end; non-directional / PBAT; UMI extraction + duplex-consensus collapsing (7 bp inline UMI + 1 spacer, `OverrideCycles U7N1Y#`); variant-vs-methylation deconvolution (SNP-aware calling, rastair/DRAGEN territory — v1 is SNP-naive); bowtie2/hisat2 unconverted-index support; `--multicore`; FASTA input. Architect a later phase around the mC→T *convention* (TAPS/evoC share it), not the Illumina brand.

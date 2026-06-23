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

## Synthetic ground-truth gate (GREEN, real minimap2)

`tests/five_base_groundtruth.rs::five_base_groundtruth_real_minimap2_recovers_known_methylation`.

A true DRAGEN-concordance gate is impossible here (DRAGEN is proprietary FPGA with no reproducible reference output, and there is no public raw 5-Base dataset). The achievable, stronger substitute is a **synthetic ground-truth** gate against the **real minimap2** (pinned 2.31-r1302, present locally):

1. Synthesize reads from a known reference with a KNOWN methylation pattern (5mC→T injected at chosen CpGs; 12 bp exact anchors so minimap2 does not soft-clip the core).
2. Run `bismark_rs --illumina_5base` with the REAL minimap2 against the unconverted FASTA.
3. Walk each BAM record's POS+CIGAR to map read→genomic positions and assert the `XM` call at **every aligned CpG** matches ground truth (methylated → `Z`, unmethylated → `z`), tolerating soft-clipped edges.

**Result: PASS** — no wrong-polarity call at any aligned CpG; ≥70% of CpGs recovered; several methylated (`Z`) positively confirmed through the real aligner. This validates the whole FROM-FASTQ chain (real alignment to the unconverted genome + the inverted 5-Base call + extraction), not just the hermetic fake-minimap2 path in `cli.rs`. The test is a no-op when `minimap2` is absent, so CI without minimap2 stays green.

**Paired-end** (`five_base_pe_groundtruth_real_minimap2`): FR fragment pairs (R1 forward 5' end with injected 5mC→T, R2 = revcomp of the 3' end) aligned with real minimap2 PE via `--illumina_5base -1 -2`. **PASS** — every pair emits two records (R1 FLAG 0x40 forward / R2 0x80 reverse), and R1's CpG calls match ground truth at every aligned position (the OT/index-0 inverted call through real minimap2 PE).

## DRAGEN concordance gate (PENDING — external)

Target: per-CpG methylation concordance with **DRAGEN's 5-Base `CX_report`** on a shared dataset, within a documented tolerance.

- **Data:** Illumina 5-Base is currently only in BaseSpace demo (NA12878/HG002, gated); no public SRA/ENA raw yet (launched 2025-10-15). Public TAPS (GSE112520) validates the mechanics (5mC→T) but not the 5mC-only biology (TAPS = 5mC+5hmC).
- **Procedure (to run when a dataset is in hand):**
  1. `bismark_rs --illumina_5base --genome <ref> reads.fq` → BAM.
  2. `bismark_methylation_extractor_rs --cytosine_report` → CX report.
  3. Diff per-CpG % methylation vs DRAGEN's CX report; record divergence here.

## Done since v1

- **Paired-end** (`run_pe_five_base`): one minimap2 PE instance over the unconverted genome; OT/OB index from R1's strand; reuses the PE extract + `paired_end_sam_output` with the inverted call. Ground-truth gated. Proper pairs only.
- **Variant/methylation deconvolution** (`--five_base_deconvolution`, module `five_base_deconv.rs`): post-alignment two-strand pileup over the BAM; a CpG whose opposite strand also lost the cytosine is a C>T/G>A variant (excluded from methylation), the rule DRAGEN uses. Writes `<out>.5base_deconvolution.txt`. Ground-truth gated (homozygous C>T → `variant`; 5mC → `methylation`).
- **bowtie2/hisat2 backends** (`--bowtie2`/`--hisat2` + `--five_base_index <basename>`): align the raw reads to a user-provided NORMAL (unconverted) index with a plain option profile; same per-read inverted call. Hermetic-tested (fake bowtie2).
- **UMI dedup** (`--five_base_umi_len N`): drop PCR/optical duplicates by (UMI, chrom, pos, strand) SE / (R1 UMI, R2 UMI, chrom, R1 pos, strand) PE. Hermetic-tested.

## Permanent non-goal

- **Non-directional / PBAT**: DRAGEN documents 5-Base as **directional-only** (`--methylation-protocol=directional`), so this is rejected by design, not deferred.

## Deferred follow-up

Full DRAGEN-style **duplex-consensus** base reconciliation (the asymmetric mC>T two-strand consensus that forms a single consensus read from a duplex pair — distinct from the UMI-position dedup already shipped); `--multicore`; FASTA input. Architect a later phase around the mC→T *convention* (TAPS/evoC share it), not the Illumina brand.

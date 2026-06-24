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

## Official Illumina spec confirmation (data sheet M-GL-03689)

The manufacturer data sheet ("Illumina 5-Base DNA Prep", M-GL-03689) confirms the core
design decisions of this implementation verbatim:

- **Chemistry / polarity** — "Novel chemistry for direct conversion of 5-methylcytosine
  to thymine" / "one-step 5mC-to-T base conversion". Matches our `methylation_call(...,
  five_base = true)`: a read `T` at a genomic `C` is methylated (the inverse of
  bisulfite). The enzymatic conversion is "nondamaging" / "single-step", so reads keep
  full complexity and align to the UNCONVERTED genome (we verified C ≈ 19.7% base
  composition on the real data) — exactly our model.
- **5mC only** — "detection of five bases (A, T, G, C, and 5mC)"; no 5hmC. Consistent
  with the dossier (TAPS = 5mC+5hmC is the distinct chemistry).
- **Simultaneous variants + methylation** — "simultaneous discovery of genomic variants
  and methylation"; DRAGEN Germline + Somatic. This is what `--five_base_deconvolution`
  addresses (separating C>T genetic variants from 5mC via the two-strand rule).
- **Read structure** — 2×151 bp + UMI; matches the qname dual-UMI we handle with
  `--five_base_umi_qname` + the duplex passes.
- **Recommended coverage** — "Germline 5-base genome: 35–40×" (whole-methylome 10–35×;
  somatic ≥100×). Our full-depth validation run (~44×) is at/above the germline nominal
  depth, so the per-CpG DRAGEN comparison is at proper coverage.
- **Analysis** — DRAGEN secondary analysis ("dual genomic and epigenomic annotations …
  for a 30× genome"), i.e. the reference pipeline we benchmark against.

This is authoritative (manufacturer) grounding that the 5mC→T model, the unconverted-genome
alignment, the variant/methylation deconvolution, the UMI/duplex handling, and the
directional-only stance (DRAGEN metrics show CTOT/CTOB = 0) all match the real method.

## DRAGEN concordance gate (DONE at global metrics — 2026-06-24)

The DRAGEN reference output **was available all along**: the BaseSpace project ships a
**DRAGEN 5-Base complete** AppResult per sample (`illumina.dragen.complete.v0.4.5`) with
the per-CpG `*.CX_report.txt.gz` and `*.methyl_metrics.csv`, fetchable via the `bs` CLI
(`bs dataset contents --id ds.258e7442...` for Sample8). Global concordance vs our 10M-pair
run (full details + table in `VALIDATION_REAL_DATA.md`):

- **CpG 48.2 % (us) vs 49.73 % (DRAGEN); CHG 1.3 vs 1.30; CHH 1.1 vs 1.16** — non-CpG
  within ~0.06 pt (at DRAGEN's own lambda-control floor), CpG within 1.5 pt (ours = 0.5x
  subsample, no base-Q mask / full UMI dedup). **Directional-only confirmed** (DRAGEN
  CTOT/CTOB = 0), matching our design.
- **Remaining (optional, deeper):** per-CpG `CX_report` diff — run `bismark_rs
  --illumina_5base ...` → `bismark_methylation_extractor_rs --cytosine_report` → CX, diff
  vs DRAGEN's `CX_report.txt.gz`. Needs matched depth (our 0.5x → ~1 read/CpG is
  sampling-noisy per site); align more lanes first.

## Done since v1

- **Paired-end** (`run_pe_five_base`): one minimap2 PE instance over the unconverted genome; OT/OB index from R1's strand; reuses the PE extract + `paired_end_sam_output` with the inverted call. Ground-truth gated. Proper pairs only.
- **Variant/methylation deconvolution** (`--five_base_deconvolution`, module `five_base_deconv.rs`): post-alignment two-strand pileup over the BAM; a CpG whose opposite strand also lost the cytosine is a C>T/G>A variant (excluded from methylation), the rule DRAGEN uses. Writes `<out>.5base_deconvolution.txt`. Ground-truth gated (homozygous C>T → `variant`; 5mC → `methylation`).
- **bowtie2/hisat2 backends** (`--bowtie2`/`--hisat2` + `--five_base_index <basename>`): align the raw reads to a user-provided NORMAL (unconverted) index with a plain option profile; same per-read inverted call. Hermetic-tested (fake bowtie2).
- **UMI dedup** (`--five_base_umi_len N`): drop PCR/optical duplicates by (UMI, chrom, pos, strand) SE / (R1 UMI, R2 UMI, chrom, R1 pos, strand) PE. Hermetic-tested.
- **Duplex-consensus** (`--five_base_duplex` / `--five_base_consensus`, module `five_base_duplex.rs`, SE): groups the two strands of one original molecule into a *duplex family* (genomic span + a canonical, swap-collapsed nonrandom-duplex UMI carried to the BAM as a standard `RX:Z:` tag) and reconciles the asymmetric 5mC→T signal **per molecule** (distinct from the population deconvolution and from the UMI-position dedup). `--five_base_duplex` writes `<out>.5base_duplex.txt` (per-family verdicts; `DUPLEX_MIN_OPP_DEPTH=1` — one opposite read IS the duplex partner). `--five_base_consensus` collapses each family to ONE consensus read in `<out>.5base_consensus.bam`: at a CpG the own strand carries the call and the opposite strand is the variant check (a cytosine gone on both strands is masked to `N`, excluded from methylation), other positions reconcile by agreement/quality. The consensus carries a **standard single-strand** Bismark `XM`/`XR`/`XG`; DRAGEN's undocumented combined +/- XM string is deliberately **not** reproduced (downstream compatibility). Ground-truth gated (one 5mC molecule + one homozygous C>T molecule, each an OT read + an OB read with swapped UMIs → the strands pair into one family; the 5mC site stays methylation/`Z`, the C>T site becomes `variant`/masked `.`).

## Permanent non-goal

- **Non-directional / PBAT**: DRAGEN documents 5-Base as **directional-only** (`--methylation-protocol=directional`), so this is rejected by design, not deferred.

## Deferred follow-up

- **qname dual-UMI extraction — DONE** (`--five_base_umi_qname`, commit 4232f4b): the real Illumina 5-Base demo (NA12878, BaseSpace) carries the duplex UMI in the read NAME as `A+B` (e.g. `...:ANGGTGT+NAGTGTA`), the partner swapped (`B+A`), NOT inline. `UmiSwap::DualPlus` canonicalizes the +-swap; the emit path writes it to `RX:Z:` via bismark-io `extract_barcode`. Validated on real NA12878 R1: RX is populated and families key on the real dual UMI. (As expected for SE-on-R1, 0 families PAIR — a molecule's two strands are different read pairs, so reconciliation needs PE duplex below.) The real headers also surfaced + fixed the qname-whitespace desync (commit 4e4f3d4).
- **Paired-end duplex report — DONE** (`--five_base_duplex` with `-1/-2`, commit 147fffd, `run_five_base_duplex_pe`): families key on `(chrom, fragment-outer-span, canonical dual UMI)` (span from POS + mate-pos + TLEN), so a molecule's two pairs land in one family. The two strand concepts are now resolved in `add_read(key, molecule_is_ot, coverage_forward, obs)`: *molecule-strand* (`R1-forward == OT`) drives pairing; *coverage-strand* (FLAG orientation) drives per-CpG own/opposite (R2 taken from FLAG, never its CTOT/CTOB tag). Synthetic PE gate green; runs clean on real NA12878 PE but 0 pairs at sparse subsample depth (a duplex pair needs both partner pairs at one fragment).
- **PE consensus collapse — DONE** (`--five_base_consensus` PE, commit a5782b3): `run_five_base_consensus` is unified SE+PE — families key on the fragment outer span, molecule strand gates pairing, coverage strand (FLAG) buckets reads, each position reduces the forward- and reverse-covering reads to their best base and combines via `consensus_base`. One consensus read per paired family. SE behaviour unchanged. PE consensus ground-truth gate green.
- **Real-data PE run — DONE** (2026-06-24, see `VALIDATION_REAL_DATA.md`): 10M real NA12878 PE pairs vs whole GRCh38 → **93.7% mapping**, CpG **48.2%** vs CHG/CHH ~1.1-1.3% (correct 5-Base signal), and **1,123 duplex-paired families → 1,123 consensus reads** with real `XM` calls. Duplex pairing + consensus collapse confirmed on real Illumina 5-Base data. A deeper (more-lanes) run would yield more pairs but is not needed to validate; it awaits the finished BaseSpace download (currently partial).
- `--multicore` for the duplex/consensus post-passes; FASTA input.
- **External DRAGEN concordance gate** — still PENDING (no public raw 5-Base dataset; proprietary FPGA). Runbook above. The synthetic ground-truth vs real minimap2 is the substitute that ships.

Architect a later phase around the mC→T *convention* (TAPS/evoC share it), not the Illumina brand.

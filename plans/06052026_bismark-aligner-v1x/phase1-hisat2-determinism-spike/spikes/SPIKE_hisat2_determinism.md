# SPIKE — HISAT2 determinism (v1.x Phase 1)

- **Date:** 2026-06-05 · **Epic:** `06052026_bismark-aligner-v1x` Phase 1.
- **Box:** oxy `bismark-test` env — Perl Bismark **v0.25.1**, **HISAT2 2.2.2** (`hisat2-align-s version 2.2.2`), samtools 1.23.1, human GRCh38 (`.ht2` index present).
- **Script:** `spikes/spike_hisat2_determinism.sh` (run on oxy: `bash spike_hisat2_determinism.sh 10000`). Throwaway.

## 1. Question / success / scope
- **Question:** Is Perl Bismark v0.25.1 + HISAT2 2.2.2 byte-deterministic run-to-run on real bisulfite reads (→ a byte-identity Rust HISAT2 wrapper is reachable)? What exact `aligner_options` does Bismark assemble; any reorder/seed flags; the SAM tag set (ZS vs XS); do spliced (N-CIGAR) records appear?
- **Success:** two independent `bismark --hisat2` runs on the same 10k SE reads → byte-identical decompressed SAM (`@PG` filtered).
- **Scope:** SE directional, 10k, human GRCh38, HISAT2 only. NOT the Rust port (Phase 2), NOT PE/non-dir/pbat, NOT minimap2 (Phase 3), NOT full scale.

## 2. Results (single iteration — passed first try)

| Q | Result |
|---|---|
| **Q1 Determinism** | ✅ **run1 == run2 byte-identical** (decompressed SAM, `@PG` filtered) — **8360 records**. HISAT2 2.2.2 is run-to-run deterministic at the Bismark level. |
| **Q2 `aligner_options`** | `-q --score-min L,0,-0.2 --ignore-quals **--no-softclip --omit-sec-seq**` + per-instance `--norc`/`--nofw`. **2 instances** (CTreadCTgenome `--norc`, CTreadGAgenome `--nofw`) — the **same strand-instance model as Bowtie2**. No `--seed`/`-p`/`--reorder` in the assembled options. |
| **Q3 SAM tags** | Final BAM record tags = `NM MD XM XR XG` (standard Bismark). **No `ZS:i:`/`XS:i:` in the final BAM** (0/2000) — the secondary-score tag lives in the **raw HISAT2 stream** (parsed during merge for 2nd-best scoring), not emitted to the Bismark BAM. |
| **Q4 Spliced N-CIGAR** | **12 of 8360** records carry an `N` CIGAR op (HISAT2 produces spliced alignments even on WGBS reads). The extraction's `N`-op path is genuinely exercised. |
| **Q5 Output / counts** | `se_bismark_hisat2.bam` + `se_bismark_hisat2_SE_report.txt`. Report: 10000 analysed, **8361 unique-best**, 83.6% mapping eff, 484 no-align, 1155 multi, **1 genomic-seq discard** → 8361 − 1 = **8360 BAM records** (same discard arithmetic as Bowtie2). |

## 3. Findings
- **The byte-identity premise HOLDS for HISAT2** — deterministic run-to-run, no reordering/seed knobs in play. Phase 2 can target a byte-identity gate (no concordance fallback needed for HISAT2).
- **HISAT2 = thin wrapper, confirmed.** Same 2-instance directional strand model + `--norc`/`--nofw` as Bowtie2; the option delta is precisely **`--no-softclip --omit-sec-seq`** appended to the Bowtie2 base (`-q --score-min L,0,-0.2 --ignore-quals`). The Rust `options.rs` HISAT2 assembly is therefore the Bowtie2 string + those two flags.
- **ZS handling is a *merge/parse* concern, not an output concern.** The final BAM never carries ZS; `align.rs` must read `ZS:i:` (vs Bowtie2's `XS:i:`) from the **raw HISAT2 stdout** for the 2nd-best score — the parser already does (`XS`-or-`ZS`). Verify in Phase 2 against a multi-mapper.
- **Spliced reads are real (12/8360).** Phase 2 must confirm the `N`-CIGAR genomic-seq extraction byte-matches Perl (the path exists from the Bowtie2 port but was rarely exercised there).

## 4. Reference snippets (carry to Phase 2)
- **HISAT2 invocation (Perl `single_end_align_fragments_..._hisat2`):** `hisat2 <aligner_options> --norc|--nofw -x <BS_index> -U <reads>` piped; `aligner_options = -q --score-min L,0,-0.2 --ignore-quals --no-softclip --omit-sec-seq`.
- **Index:** `BS_CT`/`BS_GA` `*.ht2` (8 files); discovery extension `.ht2`.
- **Naming:** `<base>_bismark_hisat2{,_pe}.bam`, `<base>_bismark_hisat2_{SE,PE}_report.txt`.
- **Report line:** `Bismark was run with HISAT2 against the bisulfite genome of <genome> with the specified options: <aligner_options>`.

## 5. Recommendation
**PROCEED to Phase 2 (HISAT2 wrapper) targeting a byte-identity gate.** No concordance fallback needed. The wrapper is: generalize `aligner.rs` → multi-backend (HISAT2 binary resolve + version pin 2.2.2); `options.rs` HISAT2 assembly = Bowtie2 base + `--no-softclip --omit-sec-seq`; `.ht2` discovery; naming/report wording; verify the `ZS` raw-stream parse + the spliced-`N` extraction against Perl. Bowtie2 stays byte-frozen.

## 6. Limitations
- SE directional only at 10k — PE / non-dir / pbat determinism + the spliced-N extraction parity are Phase-2 gate items (not re-spiked; HISAT2 determinism is aligner-level, not layout-dependent).
- The raw-stream `ZS` parse wasn't directly exercised here (no multi-mapper inspected) — Phase 2 must confirm against a HISAT2 multi-mapper.
- minimap2 determinism + its both-strand selection is the separate, higher-risk Phase 3 spike.

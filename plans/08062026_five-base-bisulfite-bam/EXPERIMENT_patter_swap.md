# Experiment — does the `patter` two-constant swap fix 5-Base calls?

**Date:** 2026-08-07 · **Context:** [#787](https://github.com/FelixKrueger/Bismark/issues/787), [#1095](https://github.com/FelixKrueger/Bismark/issues/1095) · **Verdict: YES, decisively, on both strands.**

Run because @Danielsm8 reported the swap made **no difference** on his DRAGEN BAMs. That result is confounded two ways (his build may not have taken; DRAGEN's `SEQ` representation is unverified), so the hypothesis needed testing on data where `SEQ` provably holds the raw 5-Base read.

## Result

| Input | `patter` | METH (`C`) | UNMETH (`T`) | % methylated |
|---|---|---|---|---|
| all records | **unpatched** | 0 | 152 | **0.0 %** |
| all records | **patched** | 152 | 0 | **100.0 %** |
| OT only (FLAG 99/147) | unpatched | 0 | 68 | 0.0 % |
| OT only | patched | 68 | 0 | 100.0 % |
| OB only (FLAG 83/163) | unpatched | 0 | 84 | 0.0 % |
| OB only | patched | 84 | 0 | 100.0 % |

**Ground truth: 100 % methylated** — Bismark's own `XM` on the same BAM gives `Z=168, z=0`.

Unpatched `patter` is **perfectly inverted**; patched `patter` is **exactly right**. Line counts (23) and unknown counts (43) are identical between the two runs, so the swap changes polarity and nothing else — consistent with `ref_chr`/`unmeth_seq_chr` being read in exactly two places (`patter.cpp:153`, `:159`).

**Both strand branches are correct, independently.** This mattered: OB is the harder branch (`shift = 1`, plus `is_cpg`'s requirement that `seq[j-1] == 'C'`). A one-sided patch would have shown a partial flip of roughly 50 %; the complete 0 % → 100 % flip on *each* strand separately rules that out.

## The patch

```diff
--- a/src/pipeline_wgbs/patter.h
+++ b/src/pipeline_wgbs/patter.h
-    ReadOrient OT{'C', 'T', 0, 0};
-    ReadOrient OB{'G', 'A', 1, 1};
+    ReadOrient OT{'T', 'C', 0, 0};
+    ReadOrient OB{'A', 'G', 1, 1};
```

## Method

1. **wgbs_tools** cloned at `master`; `patter` built twice — once unmodified, once with the patch — via `python3 setup.py -t patter`. **The two binaries differ** (`cmp`), confirming a `patter.h` edit does propagate through that build path.
2. **5-Base PE reads** generated from pUC19 (2686 bp) with every CpG cytosine methylated, so every CpG C → T and every non-CpG C untouched. 59 pairs, alternating OT- and OB-origin molecules; 24 pairs aligned uniquely (22 `XG:Z:CT` + 26 `XG:Z:GA` records).
3. **Aligned** with `bismark --illumina_5base` (minimap2 `-x sr`, unconverted genome). Bismark's `XM` is the ground truth: `Z=168, z=0`.
4. **CpG dictionary** for pUC19 built directly — 173 CpGs as `chrom, 1-based locus, index`, bgzipped and `tabix -s 1 -b 2 -e 2` indexed. This is the shape `load_genome_ref` expects (`tabix ref | cut -f2-3`); `wgbstools init_genome` was not needed.
5. **Ran** `samtools view -q 10 -F 1796 -f 3 <bam> | match_maker | patter <dict> pUC19:1-2686 --min_cpg 1 --clip 0` for each binary, then per-strand with `bam2pat`'s own selectors.

152 of 168 calls survive `patter`'s filters (`min_cpg`, `strip_pat` trimming, reads past the last CpG, the view filters) — expected, and identical between the two runs.

### Generator caveat worth recording

The first attempt gave a meaningless 47 % because the read generator conflated **mate** with **strand**: it built R2 as the opposite genomic strand converted independently, when R1/R2 are two ends of *one* converted molecule (R2 = reverse complement of the converted strand R1 came from). OT vs OB are two *different* molecules. Bismark correctly emitted `XM = '.'` for those malformed mates rather than calling them — the mixed figure was the generator's fault, not Bismark's.

## What this does and does not establish

**Does:** the two-constant swap makes `patter` correct on 5-Base data on both strands, and `setup.py -t patter` does pick up a `patter.h` edit. The upstream PR's validation checks 1 and 4 now pass.

**Does not:** explain @Danielsm8's negative result. Two candidates remain, and they are distinguishable by him:
- **His build did not take.** `setup.py` prints `FAIL` in red for a broken module and **continues** — the `raise RuntimeError` is commented out (`setup.py:33`) — so a failed compile exits 0 among ~15 other modules' output and leaves the old binary in place. Test: are his pre- and post-patch `.pat` files byte-identical? If yes, the build never happened.
- **DRAGEN's `SEQ` is not the raw read.** Everything here assumes `SEQ` carries the unmodified 5-Base read, which is true of Bismark output. If DRAGEN normalises `SEQ` toward the reference or carries methylation in a tag, `patter` produces garbage regardless of polarity — which would also explain his reference-independence and his abnormal 50 % marker capture.

**Neither affects #1095.** The converter reads Bismark's `XM` and is unaffected by either.

## Reproducing

Scratch artefacts are session-local. The reusable pieces: `generate_reads.py` in `rust/bismark/tests/data/five_base_bisulfite/` for the bisulfite analogue, and the method above. Total runtime a few minutes on a laptop; needs `g++`, `bgzip`, `tabix`, `minimap2`, `samtools`.

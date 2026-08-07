# `five_base_bisulfite` fixtures — soft clips and indels with authentic tags

Fixtures for [#1095](https://github.com/FelixKrueger/Bismark/issues/1095) (`--five_base_bisulfite_bam`).
Plan: `plans/08062026_five-base-bisulfite-bam/PLAN.md` §9.1, §9.3, §9.5.

## Why this exists

**No other fixture in the repo contains a single soft clip.** Bowtie 2 end-to-end never emits
one, so no `bismark_bt2` fixture ever will — but minimap2 `-x sr` is the default 5-Base engine
and `--five_base_umi_len` *depends* on the aligner soft-clipping the UMI prefix, which makes
`nS`-prefixed reads the dominant real-world 5-Base CIGAR shape. That shape is also exactly where
the `NM` identity matters, since Bismark counts soft-clipped bases as mismatches.

## Contents

| File | What |
|---|---|
| `softclip_indel_se.bam` | 8 single-end records, bisulfite convention, real `XM`/`XR`/`XG`/`MD`/`NM` |
| `pUC19.fa` | The reference (2686 bp). Present so a test can build an **independent** from-genome oracle rather than re-deriving the reference from `MD` (PLAN §9.5) |
| `softclip_indel_se.fq` | The input reads |
| `generate_reads.py` | The read generator — regenerates `softclip_indel_se.fq` from `pUC19.fa` |
| `softclip_indel_se_report.txt` | The aligner's own report |

## The eight records

| QNAME | FLAG | CIGAR | `XG` | Covers |
|---|---|---|---|---|
| `ot_plain` | 0 | `80M` | `CT` | control, all `M` |
| `ot_softclip` | 0 | `9S81M` | `CT` | **leading** soft clip |
| `ot_ins` | 0 | `40M1I40M` | `CT` | insertion |
| `ot_del` | 0 | `40M2D40M` | `CT` | deletion (`MD` `^` path) |
| `ob_plain` | 16 | `80M` | `GA` | control, reverse strand |
| `ob_softclip` | 16 | `81M9S` | `GA` | **trailing** soft clip |
| `ob_ins` | 16 | `40M1I40M` | `GA` | insertion, reverse strand |
| `ob_del` | 16 | `35M4D45M` | `GA` | deletion, reverse strand |

`ot_softclip` (`9S81M`) and `ob_softclip` (`81M9S`) are a deliberately **asymmetric pair**: the
foreign 10 bp prefix sits at each read's 5′ end, and the output revcomp moves it to the opposite
end of `SEQ` for the reverse record. A converter that indexes `SEQ` by `read_pos_5p` instead of
BAM position writes to the wrong end on `ob_*` and this pair catches it (PLAN §9.3, G2).

Both `XG` values are present, so both encoding pairs — `(C,T)` for `XG:Z:CT` and `(G,A)` for
`XG:Z:GA` — are exercised, each with a clip and both indel kinds. Upper- and lower-case `XM`
letters both occur on both strands (`ob_ins` is the one record with no methylated call).

## Verified properties

Checked on these records at generation time. All eight pass:

1. **`len(XM) == len(SEQ)`** on every record, including the clipped and inserted ones.
2. **`NM` = mismatches at `M` + inserted + soft-clipped + deleted bases.** Verified against `MD`
   and the CIGAR, e.g. `ot_softclip` `NM=20 == 11 + 0 + 9 + 0`. This fixture is the empirical
   proof that Bismark counts **soft-clipped** bases in `NM` — a non-standard behaviour, and one
   an earlier revision of the plan had stated backwards.
3. **`XM` is `'.'` at every `S` and `I` position** — 9 gap positions on each `*_softclip`, 1 on
   each `*_ins`, all dots. Confirms that a positional zip cannot touch a clipped or inserted base.
4. **At every `XM`-letter position, `SEQ[i] ∈ {meth, unmeth}`** per the `XG` pair. This is the
   invariant the whole re-encode rests on (PLAN §3.1.2, §3.3.2), here confirmed on real data
   across both strands with clips and indels present.

## Provenance

Generated with Bowtie 2 2.5.5 in `--local` mode (which is what produces the soft clips):

```bash
gzcat test_files/pUC19.fa.gz > genome/pUC19.fa
bismark_genome_preparation --bowtie2 genome/
python3 generate_reads.py genome/pUC19.fa reads.fq
bismark --genome genome/ --local --output_dir out/ reads.fq
```

Reads are built from pUC19 substrings with `C→T` applied outside CpG context (so CpG calls are
methylated and CHG/CHH calls are not), a 10 bp non-genomic prefix for the clipped records, and a
1 bp insertion / 2 bp deletion for the indel records. The `ob_*` reads are built from the
reverse complement, so Bismark aligns them to the GA index and emits FLAG 16.

**Oracle-validated.** The output is **byte-identical** between the Rust aligner and the live Perl
`bismark` v0.25.1 on the same inputs — all fields including MAPQ, not merely the tags the
converter reads. So `MD`/`NM`/`XM` here are authentic, not Rust-only artefacts.

## Scope

These are **bisulfite** records, not 5-Base. That is deliberate: they are the input to the
idempotence gate (PLAN §9.1), which requires that converting a bisulfite BAM returns identical
SAM text. A 5-Base soft-clip fixture is a separate artefact and is not required by §9.1.

The bowtie2 index is **not** committed — regenerate it with the commands above if needed.

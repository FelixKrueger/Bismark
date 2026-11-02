# Pre-registration: 5-Base Bowtie 2 defaults (`--dovetail --maxins 800`)

Frozen before any B-new cell has run, including the 10M dry run. The outcome follows this file. If a B-new cell has produced data and this file is then changed, that run is void. Source: `PLAN.md` rev 3, Behavior 2, approved by Felix on 2026-11-02.

## Fixed setup

- **Bismark binary:** branch `feat/5base-bowtie2-defaults` at `72bbf7b` (the builder change and its tests), built on oxy with `cargo build --release -p bismark`. `bismark --version` must name `72bbf7b`. Dedup and the extractor are the same multicall build.
- **Harness:** `validation/five_base_engine_bench.sh`, `five_base_rescued.py`, `five_base_compare.py` and `concordance.py`, at the commit that adds this file.
- **Tools** (checked by `five_base_engine_bench.sh versions`; a mismatch stops the work): Trim Galore 2.2.0 (Oxidized), Bowtie 2 2.5.5, cutadapt 2.6, samtools 1.23.1.
- **Reference and indexes:** as #1146: Ensembl GRCh38 primary assembly, the shared plain Bowtie 2 index `Homo_sapiens.GRCh38`; scoring on autosomes and X.
- **Samples:** NA12878 and HG002, the same 100M-pair screen subsets as #1146 (first 100,000,000 pairs of L001 then L002, before trimming), inputs symlinked from the read-only S3 mount.
- **Threads:** `-p 32` under `taskset`. B-new NA12878 and the HG002 cells run as two concurrent streams on separate 32-CPU sets; timings are not compared.
- **Per-arm pipeline:** `bismark --illumina_5base <arm flags> --five_base_deconvolution -p 32`, then `deduplicate_bismark -p --bam`, then `bismark_methylation_extractor -p --gzip --cytosine_report`, as in #1146.

## Arms

| Arm | Flags added to `--illumina_5base` | Options it must show (report options line) |
|---|---|---|
| B-old | `--bowtie2 --five_base_index <idx> -X 800 --no_dovetail --five_base_umi_qname` | `--maxins 800` exactly once; no `--dovetail`, no `--local` |
| B-new | `--bowtie2 --five_base_index <idx> --five_base_umi_qname` | `--dovetail` and `--maxins 800` exactly once each; none of `--local`, `--no-mixed`, `--no-discordant` |

- B-new's `@PG` command line must show `--bowtie2` and must not show `-X`, `--maxins`, `--dovetail` or `--no_dovetail`.
- For both arms, the first 1,000 trimmed R1 names must end in an `A+B` UMI field and the first 1,000 BAM records must carry `RX:Z:A+B`.
- `--five_base_umi_qname` only adds `RX`; it changes no alignment or metric.

## Reproduction checks

A failure stops the work.

1. **Tools** match the versions above (`versions`).
2. **Trimmed reads** (`repro-input`): `zcat -f` md5s equal #1146's plain-`.fq` md5s:
   - NA12878 R1 `337ffa049ef5cff27fa5e30a555eb50a`, R2 `5229b07724d84db29bc6ae9b36276649`;
   - HG002 R1 `d055be973c675edbeee5897aa7cc6341`, R2 `f0770c478ceb160f5d50edeec7619bf8`.
3. **NA12878 B-old reproduces #1146 B-X** (`repro`, against `plans/10292026_5base-default-engine/bench_results/NA12878/screen/`), with exact equality on:
   - pairs_in, unique_pairs, map_eff, cpg/chg/chh %, dup %;
   - records, mapq0, r1_records, readthrough, TLEN p10/p50/p90;
   - deconvolution precision and recall, and their numerators and denominators;
   - own-set cov ≥ 10 n and r (822432, 0.965834), scoring B-old alone against DRAGEN.

   HG002 B-old is a new baseline; it has no reproduction check.

B-new runs only after checks 1–3 pass on NA12878.

## Primary check: the rescued pairs

`five_base_engine_bench.sh rescued` runs `five_base_rescued.py rescued` on the raw, pre-dedup `_pe.bam` of each arm, walking both BAMs in the order of the trimmed R1 FASTQ.

### Sets

- **Rescued:** pairs in B-new that are absent from B-old.
- **Changed:** pairs in both arms whose POS or CIGAR differs on either mate. Reported only.
- **Rest:** B-new pairs that are neither rescued nor changed.
- Pairs in B-old but absent from B-new are counted as "lost" and reported.

All counts are pairs or XM calls on both mates, with no overlap removal, applied the same way to every set.

### Geometry

In 1-based reference coordinates:
- F is the mate with flag `0x10` unset, spanning [Fs, Fe]; R is the other mate, spanning [Rs, Re].
- **Dovetailed:** Rs < Fs or Fe > Re.
- **Contained:** not dovetailed, and one span lies wholly within the other.
- **Overhang bases:** F's aligned bases at positions > Re, plus R's aligned bases at positions < Fs.
- **Cycle:** a base's read position counted from its read's 3′ end; 0 is the last sequenced base.

Every set reports its dovetailed, contained and normal counts and the overhang-length distribution (p10, p50, p90, max) over its dovetailed pairs.

### Random draws

- The draws come from `random.Random(1146)`, one draw per rest pair, in FASTQ order.
- Rest pairs with a draw < 0.01 form the **R4 control**.
- Rest pairs with a draw < 0.05 form the **R5 pool**. The R5 rest sample is the pool's k lowest draws, where k is the number of rescued pairs.
- If k is larger than the pool, the run stops.

### Criteria

Each criterion is applied per sample by `five_base_engine_bench.sh verdict`.

- **R1.** The rescued pairs are between 0.1% and 3% of B-new's pairs. Outside that range, stop and investigate.
- **R2.** The rescued set's CHG and CHH methylation are each ≤ the rest's + 0.5 pt.
- **R3.** The rescued set's CpG methylation is within ±5 pt of the rest's.
- **R4.** The rescued set's overhang bases against the R4 control's non-overhang bases.
  - **Methylation:** overhang CHG and CHH methylation are each ≤ the control's + 1.0 pt.
  - **Mismatches:** the overhang mismatch rate is ≤ 2× the control's.
    - A mismatch is an `MD` mismatch at a base whose XM character is `.`.
    - Its denominator is the bases whose XM character is `.`.
  - **Cycle matching:** the control rate is the per-cycle control rate weighted by the overhang denominator at each cycle.
    - Cycles with no control denominator are dropped from both sides, and their count is reported.
  - **Minimum n:** each comparison needs n ≥ 10,000 overhang bases (its matched denominator). Below that, the result is INSUFFICIENT and the run stops for review.
- **R5.** Per-call agreement with DRAGEN at sites where DRAGEN has cov ≥ 10.
  - The site of a CpG call is (chrom, position, +) when R1 is forward, and (chrom, position, −) when R1 is reverse.
  - Each call scores p if methylated and 1 − p if not, where p is DRAGEN's methylated fraction at that site.
  - The rescued mean score must be ≥ the R5 rest sample's mean − 0.02.
  - It needs n ≥ 10,000 rescued calls at such sites; below that, the result is INSUFFICIENT.
- **R6.** M-bias on R1 records only.
  - For each of the last 10 cycles, take CpG methylation (Z / (Z + z)).
  - Average over the cycles that have calls.
  - The rescued set's average must be within ±3 pt of the rest's.

## Secondary check: genome-wide

B-new against B-old, using `score <sample> screen B-old B-old B-new`. Each value comes from full-precision counts.

- Common-set Δr at cov ≥ 10 is ≥ −0.001.
- CHG and CHH, from the report's methylated and unmethylated counts, are each ≤ B-old's + 0.2 pt.
- The MAPQ-0 fraction (mapq0 / records) is ≤ B-old's + 2 pt.
- Deconvolution precision and recall, from their numerators and denominators, each have Δ ≥ −1 pt.

**Reported, not deciding:**
- the gain-set r and its n;
- ΔCpG at cov ≥ 10;
- Δ mapping efficiency;
- the changed-set and lost-pair counts.

## Duplex/consensus pass (NA12878)

`five_base_engine_bench.sh duplex NA12878 screen <arm>` for B-old and for B-new runs:

```
bismark --illumina_5base --genome <G> --five_base_umi_qname \
  --five_base_emit_multiplicity both --five_base_consensus_from_bam <arm raw _pe.bam> -o <dir>
```

- It writes `five_base_consensus.bam` (duplex) and `five_base_simplex.bam`.
- `--five_base_min_mapq` is left at its default of 0 (the harness checks the default) and is recorded in `settings.tsv`.

**Reported:**
- duplex families (emitted + skipped) and simplex families, from the tool's summary lines;
- consensus records;
- CpG/CHG/CHH methylation of the duplex consensus, overall and per strand;
- DRAGEN's per-strand mean CpG methylation.

**Family-split check.** `five_base_rescued.py` recomputes each B-new pair's family key the way `run_five_base_consensus` does. The key is:
- the reference;
- the start, min(POS₁, POS₂) − 1;
- the end, start + |TLEN|;
- the canonical dual UMI from `RX`, the smaller of `A+B` and `B+A`.

The molecule strand is OT when R1 is forward. A family is partnered when it has both molecule strands.

**Criteria:**
- **D0.** The helper's duplex-family count over all of B-new is within 0.1% of the tool's. Otherwise the helper is wrong and the run stops.
- **D1.** The rescued set's partnered fraction is ≥ the rest's − 5 pt.
- **D2.** B-new's consensus CpG methylation is within ±0.5 pt of B-old's.
- **D3.** B-new's consensus CHG and CHH are each ≤ B-old's + 0.2 pt.

**Not deciding:** B-new's duplex-family count against B-old's. A rise is expected, so it is reported only.

## Dry run

NA12878 at 10M pairs (tag `dry10M`) runs B-old, B-new, `rescued`, `duplex` for both arms and `verdict`. It checks the harness end to end:
- `RX` is present;
- D0 holds;
- the R4 and R5 n values, scaled ×10, reach the minimums.

No decision is taken from it, and its verdict is not an outcome.

## Outcome

The PR proceeds only if every criterion is PASS:
- R1–R6 and the secondary checks on both samples;
- D0–D3 on NA12878.

Any FAIL or INSUFFICIENT stops the work, and the numbers go to Felix. Nothing is tuned, re-run for a better number, or dropped silently. If oxy's scratch is wiped mid-run, the affected cells are re-run unchanged; that is not a re-roll.

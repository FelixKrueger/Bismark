# Plan review R2 (single, quick recheck): PLAN.md rev 2

Scope: resolution of rev-1 review items, new issues in rev 2, cheap code checks. Code read at worktree `d1764d0`.

## Resolution of rev-1 items

| Item | Resolved | Note |
|---|---|---|
| A-C1 full reproduction + versions | Yes | Every committed B-X metric and own-set n/r; versions recorded. |
| A-C2 rescued-pair checks | Partly | R1–R6 + dovetail count added; overhang-length distribution not listed; R4/R5/R6 have definition problems (N4–N7). |
| A-C3 token order / no restore recipe | Yes | Order verified in code (below); restore promise removed. |
| A-I1 duplex/consensus coverage | Partly | Pass added, but as described it runs without UMIs (N1). |
| A-I2 gain-set interpretation | Yes | Demoted to reported. |
| A-I3 extractor wording | Yes | Matches `overlap.rs:115` and the OB mirror. |
| A-I4 real-Bowtie 2 OT/OB gate + unit edges | Yes | |
| A-I5 off-scratch copies | Yes | |
| A-I6 commit before run binary | Yes | |
| B-C1 targeted new-pair check | Yes | Caveats in N4–N7. |
| B-C2 gain-set guard | Yes | Report-only. |
| B-C3 `@PG` cannot show defaults | Yes | Options line + CL absence check. |
| B-I4 token order | Yes | |
| B-I5 "metric-equivalent", TLEN caveat | Yes | |
| B-I6 tighter reproduction; HG002 new baseline | Yes | |
| B-I7 version pre-check before prep | Yes | Step 4 precedes prep. |
| B-I8 tests before run, frozen commit | Yes | |
| B-I9 redefine #1146 arms | Yes | |
| B-I10 10M dry run | Yes | |
| B-I11 gate asserts 0x2/TLEN/XM; `-X 500` case | Yes | |
| B-I12 duplex/UMI | Partly | Same as A-I1 (N1). |
| B-I13 docs table relabel | Yes | |

## Code checks

- **Token order:** correct. Bisulfite emits `--dovetail` (step 12, `options.rs:56-67`), then `--minins`/`--maxins` (70-88), then `--quiet` (91); test at `options.rs:751`. The 5-Base builder (`options.rs:282-290`) is where the change goes.
- **`-p N --reorder`** is appended by `mod.rs::five_base_aligner_options` (`mod.rs:1900`), not the builder. The builder unit test should expect `-q --score-min L,0,-0.6 --dovetail --maxins 800`; only the CLI/report test sees the full string.
- **`config.dovetail`:** `config.rs:1112` (`!cli.no_dovetail`), passed to `five_base_emit_pe_record` at `mod.rs:2370` for TLEN. Plan's Context is correct.
- **Extractor:** the default PE path calls `drop_overlap` (`pipeline.rs:403`, `parallel.rs:1166`); `drop_overlap_generic` is only for `--allow_discordant`. Plan's description is correct.

## New issues

### Critical

- **N1. The duplex pass would run with no UMIs.** `--five_base_consensus_from_bam` reads the UMI from the record's `RX:Z:` tag (`mod.rs` `key_of`, `crate::io::tags::rx`; missing tag → empty UMI). The driver writes `RX` only when the *alignment* ran with `--five_base_umi_qname` (`mod.rs:2376`). #1146 arms did not pass it, and B-old/B-new as defined don't either. Passing `--five_base_umi_qname` only to the from-BAM step keys every family on span alone, so unrelated molecules merge and the counts mean nothing.
  Fix: add `--five_base_umi_qname` to both NA12878 alignments (it adds only `RX`, so the reproduction metrics are unaffected; say so). Assert that `RX` is present on a sample of records before the pass, and check that the trimmed qnames still carry the `A+B` UMI.

### Important

- **N2. The duplex outputs named in the plan are not printed in default mode.** In `duplex` mode stderr gives only "consensus read(s) emitted, family(ies) skipped" (`mod.rs:~3261`). The paired-family count and the single-strand family count are printed only in `simplex`/`both` modes. Run with `--five_base_emit_multiplicity both`, or count from the BAM, and define "duplex families", "paired" and "fraction that pair" from those outputs. Also state that the input is the **pre-dedup** `_pe.bam`, and fix `--five_base_min_mapq` (the default is 0).
- **N3. The duplex thresholds pass almost by construction.** B-new's proper pairs are close to a superset of B-old's. A split caused by overhangs stops *rescued* pairs from joining a family; it does not remove B-old families. So "paired families ≥ B-old × 0.99" barely constrains anything, and the consensus ±0.5 / +0.2 pt checks are diluted by about 140×. Add the check that can fail: of the rescued pairs, the fraction in a paired family ≥ the rest's fraction minus a fixed margin.
- **N4. NM/MD count methylation in 5-Base.** `NM` = Hamming distance(raw read, unconverted reference) + indels (`output.rs:702`), and MD is built the same way. Every 5mC C→T (OT) or G→A (OB) is therefore a "mismatch". R4's mismatch rate has to exclude positions with an XM call (non-`.`), or the strand's conversion mismatch, or it measures CpG density × methylation. Two more gaps:
  - Overhang bases sit at read 3′ ends, where the error rate is higher. Compare them with the same read cycles in non-dovetailed pairs, not with the whole body, or the 2× limit can fail on noise.
  - Give R4 a minimum n (overhang bases, and overhang C calls).
- **N5. The dovetail-shape definition matches ordinary overlaps.** "One mate's alignment extends past the other's start" is true of every overlapping FR pair, because the forward mate's end passes the reverse mate's start. Define it in reference coordinates from POS + CIGAR, with F the forward-strand mate and R the reverse-strand mate:
  - **Dovetail:** `R.start < F.start` or `R.end < F.end`.
  - **Overhang bases:** F bases at reference positions > `R.end`, and R bases at reference positions < `F.start`. Insertion bases follow their neighbour.

  Containment with `R.end < F.end` may already be concordant without `--dovetail`. State whether those pairs count, and put both shapes in the helper self-test.
- **N6. R5 is ill-posed.** About 0.7M rescued pairs give roughly 0.07× genome depth, so sites at cov ≥ 3 from rescued pairs alone are mostly pile-ups (repeats, artefact hotspots). Two consequences:
  - n ≥ 2,000 may not be reached, and the plan says nothing about that case.
  - Where it is reached, r is biased low against B-old's genome-wide cov ≥ 3 band, which is a foreseeable false fail.

  Replace it with a per-call comparison at DRAGEN cov ≥ 10 sites: mean |Δβ| or agreement at 50% for rescued-pair calls against an equal-size random sample of rest-pair calls. Make n < threshold report-only.
- **N7. R6 has no power.** M-bias is compared genome-wide between B-new and B-old, so the 0.7% of rescued pairs is diluted and R6 passes by construction. Compute R1 3′ M-bias from XM by read cycle in the `rescued` helper, rescued against rest.

### Optional

- R1: name the denominator (B-new proper pairs, pre-dedup). For R2/R3, state that the rest set is also not deduplicated.
- Report the pairs that are proper in both arms but whose POS or CIGAR changed. `--dovetail` can change which alignment is best, and these pairs fall outside both sets.
- Compute the secondary ±0.2 pt and −1 pt checks from counts at full precision, not from 1-decimal report figures (rev-1 B-16, not addressed).
- Run the 10M dry run with `--five_base_umi_qname` and the from-BAM pass, so that N1 and N2 surface there. Check consensus memory at 100M pairs.
- List the overhang-length distribution among the reported outputs (A-C2).

## Verdict

Fix first: N1 makes the duplex pass meaningless as written, and N2–N7 are edits to the pre-registration text. All are cheap, but they must land before `PREREGISTRATION.md` is frozen.

# Plan review (Reviewer A): 5-Base Bowtie 2 defaults `-X 800` and `--dovetail`

Plan: `plans/11012026_5base-bowtie2-defaults/PLAN.md` rev 1. Checked against `dev` `d1764d0` in
`/Users/fkrueger/Github/Bismark-5base-bt2`, the #1146 evidence (`plans/10292026_5base-default-engine/`),
the harness, and the aligner, extractor and duplex code.

**Verdict:** the code change is small and correct in shape. The confirmation run as written has low
power to detect the one realistic harm (bad calls in dovetail overhang bases). Its reproduction check
is weaker than the committed data allows. Two factual claims in the plan are wrong. None of this blocks
the approach, but items C1–C3 should be fixed before the pre-registration is committed.

---

## 1. Logic

### 1.1 How dovetailed pairs flow through the 5-Base driver (checked in code)

- **Proper-pair gate** (`mod.rs::five_base_emit_pe_record`, `flag & 0x2` plus both mates mapped).
  Bowtie 2 with `--dovetail` sets 0x2 on dovetailed concordant pairs, so they now pass. OT/OB comes
  from R1's 0x10, which dovetailing does not change. The +2 flank extraction and the CIGAR-covers-read
  check are geometry-independent. **OK.**
- **TLEN** (`output.rs::paired_end_sam_output`). With `dovetail = true`, the two dovetail sub-branches
  (`flag_1 == 83` with R1 leftmost; `flag_1 == 99` with R2 leftmost) give R1 a TLEN sign that follows
  R1's orientation. The magnitude is the outer span in every branch. **OK**, and it agrees with the
  Bowtie 2 flag now, as the plan says.
- **Extractor overlap** (`extractor/overlap.rs::drop_overlap`, default `--no_overlap` for PE). It uses
  half-plane predicates. OT keeps R2 calls with `ref_pos > r1_ref_end`; OB keeps those with
  `ref_pos < r1_ref_start`. For a dovetailed OT pair, R2 lies wholly at or below R1's end, so **every
  R2 call is dropped**, including R2's unique overhang upstream of R1's start. OB is the mirror case.
  There is no double counting, but:
  - R2's overhang is silently discarded. That is conservative, and probably desirable if the overhang
    is artefact.
  - **R1's overhang (R1's 3' end past R2's 5' start) is always kept.** If dovetailing comes from
    untrimmed adapter or read-through remnants, which is what minimap2's 0.8M soft-clipped records
    suggest, these are the bases most likely to be wrong, and the extractor keeps them.
  - `overlap.rs`'s own doc comment says the half-plane retain "would silently mangle" dovetailed-past
    pairs. That is safe here only because the outcome is dropping, not double counting. The plan's
    assumption that the extractor "handles dovetailed mates correctly" should say exactly this.
- **Deconvolution** (`run_five_base_deconvolution`). R2 records carry XR `GA` / XG `CT|GA`, so
  `from_xr_xg` classifies them CTOT/CTOB and they are skipped. **Only R1 records feed the
  deconvolution pileup.** The pre-registered deconvolution P/R guard therefore cannot see R2 effects,
  and sees R1 overhang effects only at CpGs.
- **Duplex / consensus** (`run_five_base_duplex_pe`, `run_five_base_consensus`):
  - The family key is `(chrom, min(pos, mate_pos), +|TLEN|, UMI)`. Because |TLEN| is the outer span,
    a dovetailed pair's key includes both overhangs. If the overhangs are artefactual, its span differs
    from the partner strand's, so the family does not pair and becomes a singleton.
  - Both mates' CpG observations are added with **no overlap removal**, so the overhang bases enter the
    duplex pileup and the simplex/duplex consensus reads.
  - **The confirmation run does not exercise duplex or consensus at all**, although both are GA-gated
    features in the stability contract.
- **UMI dedup** keys on R1 position and strand. Unaffected.

### 1.2 Factual errors in the plan

1. **Token order claim (Behavior 3).** The bisulfite builder (options.rs step 12, then steps 13/14)
   emits `--dovetail` **before** `--minins`/`--maxins` (`... --no-discordant --dovetail --maxins 500`).
   The plan's order puts `--maxins 800` before `--dovetail` and says this matches the bisulfite order.
   It does not. Bowtie 2 ignores the order, so either change the order to `--minins`, `--dovetail`,
   `--maxins`... (to genuinely match: `--dovetail --minins N --maxins N`) or drop the claim. Decide
   before the exact-string tests are written.
2. **"B-old gives exactly #1146's B-X options, so it reproduces the old behaviour."** The Bowtie 2
   argv matches, but `--no_dovetail` also sets `config.dovetail = false`, which #1146 B-X did not
   (`config.rs:1112`). That flips the TLEN sign in the `flag_1 == 83 && dovetail` branch for OB pairs
   with `start1 == start2` and `end1 == end2`. These are equal-extent containments, which Bowtie 2
   accepts without `--dovetail`, and they are common for fully trimmed short fragments. So B-old's BAM
   is **not** B-X's BAM, only its alignments.
   - Consequence for the run: harmless. Dedup, the extractor and |TLEN| metrics ignore the sign.
   - Consequence for users: worse. **No flag combination reproduces pre-change 5-Base Bowtie 2 output.**
     `-X 500 --no_dovetail` restores the Bowtie 2 argv but changes the TLEN sign on those pairs. The plan
     and CHANGELOG should not promise a restore recipe without saying this. Alternatively, accept it
     explicitly as a negligible cosmetic change.

### 1.3 The dovetail hypothesis rests on a metric that cannot test it

`readthrough` in the harness is `|TLEN| < length(SEQ)` on R1. Under end-to-end Bowtie 2, a dovetailed
pair's |TLEN| (the outer span) is at least the R1 length. **B-new's `readthrough` will stay near 2.9k
even if `--dovetail` rescues every one of the ~0.7M pairs.** minimap2's 801,819 and B-loc's 919,122
counts measure soft-clipped records, not dovetails. The evidence is therefore suggestive, not
mechanistic:
- B-loc rescues roughly the same number of pairs by soft-clipping.
- That points to un-alignable tails, such as adapter remnants or the R2 poly-C trimming Trim Galore
  2.2.0 does (11.7M R2 poly-C bases removed on HG002). Either would create dovetail geometry under
  end-to-end alignment.

The plan needs a metric that actually counts dovetail-geometry proper pairs (see V2). It should also
state that, under end-to-end mode, the rescued pairs' tails are **aligned with mismatches**, not clipped.

### 1.4 Edge cases

- `--dovetail --no_dovetail` together: clap accepts both, and `--no_dovetail` should win as in the
  bisulfite path. Not stated, not tested.
- Explicit `--dovetail`: must not emit the token twice. The builder change as described is safe, but it
  is untested.
- `--old_flag` with 5-Base Bowtie 2: confirmed a deferred no-op (`config.rs::deferred_flags`), and the
  5-Base builder does not run the bisulfite conflict check. The plan is right, but there should be a
  test that `--illumina_5base --bowtie2 --old_flag` still succeeds and emits `--dovetail`.
- `-I N` with no `-X`, for 500 < N ≤ 800: now valid (Bowtie 2 previously rejected `-I` > 500). For
  N > 800 it is still rejected by Bowtie 2. Fine, but worth one unit test (`--minins 600 --maxins 800`).
- `--local` plus the new defaults: the exact string is not covered.

## 2. Assumptions

| Assumption | Status |
|---|---|
| Dovetailing explains most of the 0.7-pt gap | **Untested, and the cited metric cannot test it** (1.3). |
| The extractor handles dovetailed mates correctly | Partly. No double count, but R2's overhang is dropped and R1's overhang is kept (1.1). |
| No-harm on NA12878 and HG002 is enough for a flag bisulfite Bismark already applies | The analogy is weak. In bisulfite, dovetails come mostly from deliberate 5' clipping (RRBS / `--clip_R2`), where overhang bases are genomic. Here they plausibly come from un-trimmed tails, whose overhang bases are not genomic. |
| `-X 800` is confirmed | Only on NA12878 (#1146). The new run has `-X 800` in both arms, so HG002 gets no `-X` check. HG002 A's TLEN p90 is 455, so the risk is low, but say so. |
| Builder change "costs nothing at run time" | Not quite. B → B-X aligner CPU went 77,951 → 82,975 s (+6%) on the same input, partly from mapping more pairs. It is small; state it as small. |
| Same toolchain as #1146 (Bowtie 2, Trim Galore, extractor, dedup) | Not recorded in the #1146 outputs. The md5 check covers Trim Galore. Nothing covers Bowtie 2 except the B-old repro. Which `deduplicate_bismark` / `bismark_methylation_extractor` (Rust symlink in `$BIN`, or Perl from `bismark-test`) runs is unstated. |
| `bismark --version` identifies the build | The harness DONE key uses the commit hash. If the builder change is uncommitted when the binary is built, B-new's key carries the wrong commit. Commit the builder change before building. |

## 3. Efficiency

- Four cells in two streams is reasonable. The B-old rerun is unavoidable because the #1146 BAMs and
  CpG reports are gone.
- The extra checks proposed below (gain-pair sub-BAM, M-bias diff, dovetail counts) are samtools and
  awk passes over BAMs the run already produces. That is minutes, not hours.
- Copying BAMs off scratch:
  - About 185M records per cell, so roughly 15–20 GB each and 60–80 GB in all, plus the CpG reports.
  - Check the `/home` capacity on oxy, or S3 write access, **before** starting.
  - Put the copy in the harness, per cell, not in a manual step. The current harness copies only
    metrics, reports and logs to `$RESULTS`; step 4 of the plan does not add BAM copying.

## 4. Validation sufficiency

**V1. The reproduction check uses 3 of about 20 deterministic numbers.** The committed
`NA12878/screen/B-X.rep1/metrics.tsv`, `deconv.txt` and `score.tsv` hold values B-old must reproduce
exactly under `--reorder`. All of them are unaffected by the TLEN sign:
- records 185,331,762; mapq0 1,697,864; r1_records 92,665,881;
- readthrough 2,929; TLEN p10/p50/p90 246/353/479; dup 3.40%;
- CpG/CHG/CHH 48.6/1.3/1.2;
- deconv 183,475 variants, 171,619 precision hits, 100,908 recall hits;
- own-set cov ≥ 10: n 822,432, r 0.965834. This is per-arm and independent of the other arms.

Matching these also checks dedup, the extractor and the deconvolution toolchain, not only Bowtie 2.
Pre-register them all, exact.

**V2. The no-harm criteria cannot detect the realistic harm.** Expected scale: about 0.7% more pairs,
with artefact confined to a few overhang bases per pair.
- Common-set Δr ≥ −0.001, CHG/CHH ≤ +0.2 pt, MAPQ-0 ≤ +2 pt and deconvolution ≥ −1 pt are all
  dominated by the 99.3% of unchanged pairs. A back-of-envelope estimate: 0.7% of pairs × ~5/300 bases
  × a garbage-call rate shifts CHH by about 0.001 pt.
- ΔCpG ≥ 0 and Δmapping ≥ 0 are close to guaranteed by construction, since `--dovetail` only relaxes
  concordance.
- Deconvolution sees R1 only (1.1).

Add pre-registered checks with power on the rescued pairs themselves:
- **Dovetail count.** Count proper pairs in B-new and B-old whose geometry is dovetailed (OT: R2 POS <
  R1 POS; OB: R1 end > R2 end, from POS + CIGAR). Report the overhang-length distribution. Expectation:
  B-old ≈ 0, B-new ≈ the mapping gain. If the gain is not dovetails, the mechanism claim in the
  docs/CHANGELOG must go.
- **Gain-pair analysis.** Take qnames proper in B-new but not in B-old and build a sub-BAM. Report:
  - its CpG/CHG/CHH from the extractor;
  - its pooled concordance with DRAGEN at DRAGEN cov ≥ 10 sites;
  - mismatch density (NM/MD) in the overhang bases against the rest of the read.

  Guard: the gain pairs' CHH ≤ B-old's CHH + a stated margin, and the overhang mismatch rate is not
  grossly above the read body's.
- **M-bias.** The extractor already writes `*M-bias.txt`, but the harness does not keep it. Copy it and
  compare B-new with B-old on the R1 3'-end positions, where kept overhangs sit, and the R2 3'-end
  positions.

**V3. The gain-set guard is likely to false-alarm.** B-X failed exactly this guard in #1146 (0.937
against a 0.944 threshold), and RESULTS.md attributes that to construction ("harder by construction").
B-new's gain set (sites that crossed cov 10 because a few reads were added) has the same marginal-site
bias. Pre-register how a gain-set failure is read, for example jointly with the gain-pair analysis
above. Otherwise the run has a foreseeable stop that says nothing about harm.

**V4. Duplex and consensus are untested.** They are GA-contract features and the most exposed to
overhang bases (1.1). Either:
- add a `--five_base_duplex` (and consensus) pass with `--five_base_umi_qname` to the B-old and B-new
  cells, if the screen FASTQs carry UMIs in the read name, and pre-register duplex-paired fraction and
  verdict deltas; or
- state explicitly that duplex/consensus is out of scope for Bowtie 2 and say so in the docs.

**V5. Unit and real-Bowtie 2 tests.**
- Real Bowtie 2 dovetail gate. Use genome-matching dovetailed pairs for **both OT and OB**. Assert:
  - kept by default and dropped with `--no_dovetail`;
  - TLEN sign and magnitude for R1 and R2;
  - the XM strings, which also guard the overhang calls' 5-Base polarity.

  Consider a Bowtie 2 variant of the `aligner_five_base_groundtruth.rs` synthetic gate, which is
  minimap2-only today and has no 5-Base Bowtie 2 ground truth.
- Add unit tests for:
  - `--dovetail --no_dovetail`;
  - explicit `--dovetail` (one token);
  - `--old_flag` (succeeds, emits `--dovetail`);
  - `-I 600` alone;
  - `--local` exact string;
  - HISAT2 and minimap2 unaffected by `--no_dovetail`.
- Grep every fake-`bowtie2` stub in `aligner_cli.rs` (`make_fake_bowtie2_five_base_pe` and its
  callers) for argv parsing that rejects unknown tokens, not just the exact-string tests listed in the
  plan.
- Long-fragment gate: 1,000 bp excluded at the default 800 and admitted at `-X 1200` is fine. Also keep
  one explicit `-X 500` real-Bowtie 2 case so that "explicit `-X` wins, including 500" is tested
  end-to-end and not only in the builder.

**V6. Harness self-test.**
- B-new must also deny `--local`, `--no-mixed` and `--no-discordant`.
- `--maxins 800` and `--dovetail` must each appear exactly once, as the C arm does for no-splice.
- Add a self-test that B-old fails when `--dovetail` is present, and that B-new fails when its `@PG`
  command line carries `--no_dovetail`.

## 5. Alternatives

- **Gate `--dovetail` behind the run's result, rather than shipping it as a default that the run only
  confirms.** Felix's Q2 answer already settled this, so it is listed only for completeness. If V2's
  gain-pair checks show garbage overhang calls, the better default may be `--dovetail` plus clipping
  the overhang in the driver (mask the XM calls for bases outside the other mate's span). That needs
  its own decision.
- **Split the PR.** `-X 800` has strong evidence and essentially no risk. `--dovetail` carries the open
  questions. Shipping `-X 800` first decouples them if the dovetail run stops.
- **Cheaper HG002 `-X` sanity check.** Compare HG002 B-new's mapping efficiency and TLEN quantiles
  with the committed HG002 arm A metrics (95.2%, p90 455), at no extra cost.

---

## Action items

### Critical
- **C1.** Strengthen the B-old reproduction check to every deterministic committed value: metrics.tsv
  read-fate, TLEN, dup, context %, the deconv counts and the B-X own-set cov ≥ 10 n/r (V1). Record the
  Bowtie 2, Trim Galore, dedup and extractor versions and binaries in RESULTS.
- **C2.** Add pre-registered checks with power on the rescued pairs (V2): a dovetail-geometry count and
  overhang-length distribution, gain-pair sub-BAM context % and DRAGEN concordance, overhang mismatch
  density, and an M-bias comparison. Replace the `readthrough` metric as the mechanism indicator, since
  it cannot see end-to-end dovetails (1.3).
- **C3.** Fix the two factual claims: the token order does not match bisulfite (1.2.1), and B-old is not
  byte-equivalent to B-X, nor is there a flag recipe for pre-change output, because of
  `config.dovetail`'s TLEN sign effect (1.2.2). Decide the token order before writing exact-string tests.

### Important
- **I1.** State how duplex/consensus is covered. Add a duplex/consensus pass, or declare it out of scope
  for Bowtie 2 (V4, 1.1).
- **I2.** Pre-register the interpretation of a gain-set guard failure, given #1146's construction-driven
  failure (V3).
- **I3.** Rewrite the extractor assumption to say exactly what happens: R2's overhang is dropped and
  R1's overhang is kept (1.1).
- **I4.** Real-Bowtie 2 dovetail gate for OT and OB, asserting TLEN and XM. Add the missing unit edge
  cases (V5).
- **I5.** Put per-cell BAM, CpG-report and M-bias copying into the harness. Check oxy `/home` or S3
  capacity first (3).
- **I6.** Commit the builder change before building the run binary, so the harness commit key
  identifies it (2).

### Optional
- **O1.** Split `-X 800` and `--dovetail` into separate PRs (5).
- **O2.** HG002 B-new against the committed HG002 A metrics as a free `-X 800` sanity check (5).
- **O3.** Restate "no run-time cost" as "small" (+6% aligner CPU, B → B-X) (2).
- **O4.** Harness self-test additions (V6).
- **O5.** Consider a Bowtie 2 variant of the synthetic ground-truth gate (V5).

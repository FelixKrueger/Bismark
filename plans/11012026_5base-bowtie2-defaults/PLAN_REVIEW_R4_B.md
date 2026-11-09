# Plan review, rev 4 (`--maxins 800` only): Reviewer B

Target: `PLAN.md` "Rev 4 (2026-11-02)" section, read against rev 3, the implementation notes, `PREREGISTRATION.md` rev 1, the branch code at `f3e860f` (builder at `72bbf7b`), and the #1146 artifacts under `plans/10292026_5base-default-engine/`.

## Verdict

The decision itself (drop `--dovetail`, keep `--maxins 800`) is sound and conservative. Dropping `--dovetail` puts 5-Base Bowtie 2 back to its pre-branch behaviour, so it needs no pre-registered evidence. The central claim checks out: #1146 B-X's report shows `-q --score-min L,0,-0.6 --maxins 800 -p 32 --reorder`, which matches rev 4's engine argv token for token. #1146 B-X ran without `--no_dovetail` (the `d1764d0` harness has `B-X) --bowtie2 ... -X 800`), so `config.dovetail` was true, as it will be in the new default.

The weak points are elsewhere:
- The evidence for the change that actually ships, the 500 → 800 cap, is only genome-wide. That is the same kind of check the dry run showed to be blind to per-pair harm.
- Several harness pieces the outline relies on are hard-wired to rev 3's arm names.
- Some rev 3 and #1146 artefacts are left dangling: B-loc's meaning, the rescued/duplex tooling, the docs text and the stale Q5 line.

## 1. Logic review

**Central claim (option string identical to B-X, so the results are B-X's).** True for the builder string and the driver's `dovetail` flag. It holds only if the binary is output-equivalent to the one #1146 used:
- #1146 ran at `9b5c15b` (RESULTS.md line 7);
- the branch base `d1764d0` adds `509d91c` (#1121, streaming converted reads to the aligner over FIFOs), `5da54cd` (#1145, CRLF rejection and a stalled-merge failure), and the extractor `--allow_discordant` / orphan-warning commits.

So the `repro` gate is really a gate on "#1121 + #1145 + extractor changes are output-neutral on 100M real pairs". That is worth having, but the plan should say so. It should also give a rule for a failure (see C2). The rev 3 B-old NA12878 cell (same builder string, `72bbf7b` binary) already tests the same thing. If it has finished, its `repro` result should be stated in rev 4.

**`--five_base_umi_qname` is neutral.** I checked this. It only sets `RX` (`mod.rs` ~2376), the qname is unchanged, and nothing outside `aligner/` reads `RX` (`io::tags::rx` has no callers), so dedup and the extractor are unaffected. The claim holds. Even so, rev 4 no longer needs `RX`, because the duplex pass is dropped. Dropping `--five_base_umi_qname` from B-new and B-500 would make B-new literally "#1146 B-X minus `-X 800`" and remove one assumption from the gate. Optional.

**B-old ≡ B-new.** The rev 3 B-old cells and the rev 4 B-new cells differ only in:
- `--no_dovetail`, which in rev 4 changes nothing but the TLEN sign of equal-extent OB pairs (`output.rs` 571-597);
- a binary whose diff from `72bbf7b` is only the `--dovetail` push, which B-old never reached.

Every `metrics.tsv` column except timing should therefore be identical, because the TLEN percentiles use `|TLEN|`. The plan calls B-old "a cross-check only" but does not say what it checks. Make it a stated expectation: identical non-timing metrics and an identical `score.tsv`, and any difference is a stop.

**Validation 1, the real-Bowtie 2 dovetail test flip.** After the flip, "dovetailed OT and OB pairs are dropped by default" passes vacuously if the fixture stops aligning for any reason, for example a seed or genome change. There is no positive control any more. Add one of:
- (a) a non-dovetailed control pair in the same `SmallGenome` that is kept;
- (b) a check that the report counts the pair as aligned by Bowtie 2 but not as a proper pair;
- (c) a direct `bowtie2 --dovetail` call on the same fixture that shows `YT:Z:CP`.

Also assert that explicit `--dovetail` drops them too. That is the test that pins Q5; the outline only lists the default and `--no_dovetail`.

**Validation 1, unit tests.** At `d1764d0`, `five_base_omits_bisulfite_policy_and_runtime_threads` had `--dovetail` in its deny list. `72bbf7b` removed it and added an engine-dependent count. Rev 4 should restore the deny entry rather than set the count to 0. That leaves the shortest diff from `dev` and the clearest regression guard.

**Validation 3, HG002.** "New default vs `-X 500`" is the right comparison, and B-500 = `-X 500` is alignment-equivalent to the old default, because Bowtie 2's own default is 500. But see I1: the comparison uses only the genome-wide checks.

**Edge case not covered: `-I` > the effective maxins.** I tested Bowtie 2 2.5.5 locally. `--minins 900 --maxins 800` is accepted silently, and the pair comes back `YT:Z:DP`, i.e. not concordant. The 5-Base driver then drops every pair, so the user gets a near-zero mapping rate with no error.
- This is not a regression: `-I 900` alone failed the same way against Bowtie 2's 500 default.
- Now that Bismark sets the effective `--maxins`, it can validate it: `-I` alone above 800 should die with a message to raise `-X`.
- At minimum, add the case to the unit table and the docs. Rev 3 said "no new validation"; rev 4 widens the window in which `-I` alone is valid, so the boundary should be stated.

## 2. Assumptions

| Assumption | Status |
|---|---|
| New default ≡ #1146 B-X | Verified for the option string and the `dovetail` flag. It depends on #1121/#1145 being output-neutral, which the gate tests. |
| `--five_base_umi_qname` changes no metric | Verified in code (RX only; dedup and the extractor do not read it). |
| `-X 500` reproduces the old default's alignments | Holds (Bowtie 2's default `-X` is 500). Only `@PG` and the report options line differ. |
| A larger cap only adds pairs | Mostly. A wider window also changes mate rescue and which concordant pair is reported, so existing pairs can move or lose MAPQ. That is the "changed" set from rev 3, and rev 4 drops it (see I1). |
| HG002 is an independent second sample | Partly. It is the same kit and chemistry, WGS gDNA, from the same Illumina demo, so it says little about other insert distributions (cfDNA, FFPE, low-input). |
| `--local` users are covered | No. `--local` also gains `--maxins 800`, but #1146 measured B-loc only at 500 and rev 4 measures nothing with `--local`. Its likely effect is the same direction, but the docs table row "Bowtie 2 `--local`" becomes a 500 bp figure under a label that now means 800. |
| HISAT2 parity | HISAT2 stays at 500, so 5-Base Bowtie 2 and HISAT2 now differ in default cap, and 5-Base Bowtie 2 differs from bisulfite Bowtie 2 (500). Docs line 75 ("the engine's default maximum insert is 500 bp", said of both engines) becomes wrong for Bowtie 2. |

## 3. Efficiency

- NA12878 B-new and HG002 B-new reruns (about 2.5 h each) add provenance, not information, given the B-old ≡ B-new argument above. Only HG002 B-500 carries new information. Keeping the reruns is reasonable, since they prove the shipped binary, but HG002 B-500 can start in parallel right after the step 1 commit.
- Reusing `five_base_rescued.py rescued` with `--old B-500 --new B-new` on HG002 (I1) costs about 1 h and reuses tested code.

## 4. Alternatives

1. **Per-pair check on the pairs the cap admits (recommended, I1).** Run the existing `rescued` subcommand with B-500 as "old" on HG002:
   - "Rescued" becomes the 500–800 bp pairs, about 7% of pairs on NA12878: B 86.2M → B-X 92.7M unique pairs.
   - R2, R3, R5 and R6 apply as they are.
   - R4 (overhang) does not apply. Swap it for a per-pair uncalled-MD mismatch rate, rescued vs rest, which the helper already computes per base.
   - R1's 0.1–3% band must change to about 3–12%, or become report-only.
   - Pre-register the thresholds in rev 2.

   This is the lesson of the dry run applied to the change that ships.
2. **Repoint `repro` at B-old instead of re-running NA12878 B-new.** It is valid if B-old ≡ B-new is accepted. It saves one cell but weakens "shipped binary" provenance, so I do not recommend it as the only evidence.
3. **Document instead of validate `-I` > 800.** It is cheaper but leaves a silent-zero failure mode in place.

## 5. Dangling items from rev 3 and #1146

- **B-loc is silently 800 bp in the harness (a latent bug, also present in rev 3).**
  - HEAD defines `B-loc) --bowtie2 ... --local --no_dovetail` with no `-X 500`, and `verify_options` for B-loc denies only `--dovetail`, not `--maxins`.
  - On any binary from this branch, B-loc therefore runs `--local --maxins 800` and passes verification, so it no longer means #1146's B-loc.
  - Add `-X 500` to B-loc, and `--maxins 500` exactly once to its checks.
  - Similarly, B, B-loc and B-X carry `--no_dovetail`, which in rev 4 only flips OB TLEN signs relative to #1146. Either drop it, which restores #1146's exact meaning, or say why it stays.
- **`repro` is hard-wired to B-old** (`cmd_repro`: `d=$ROOT/runs/$1/$2/B-old.rep1`, score label `B-old`). Rev 4 step 4 runs "`repro` against B-X" on B-new, but outline step 2 does not mention parameterising it by arm.
- **`verdict --secondary-only` needs more than a flag.**
  - `criteria()` hard-codes the score keys `("B-old","common","10")` / `("B-new",…)` and the "B-old" labels.
  - `cmd_verdict` reads `$r/B-old.rep1`.
  - `--rescued` is a required argument.
  - `cmd_verdict` adds the consensus files automatically when the sample is NA12878.

  The outline should name the comparator-arm parameter, for example `--old-arm B-500`.
- **B-new's self-test** (`cmd_selftest` lines 497-507) encodes `--dovetail` as required, and `once=(--maxins --dovetail)`. Both need flipping, and B-500 needs `verify_options`, `once`, `engine_of` and self-test cases. The outline mentions only "redefine B-new".
- **The rescued/duplex tooling** (`five_base_rescued.py`, 813 lines, plus the `rescued`, `duplex` and `verdict` subcommands) will ship in the PR to `dev` and nothing in rev 4 uses it. Decide explicitly whether to:
  - keep it, as the evidence behind the dovetail decision and reused for I1;
  - or move it out of `validation/`.

  Either is fine; leaving it implicit is not.
- **Plan text.**
  - Revision history line 460 says "Q5 ... is open", which contradicts the resolved Q5 at line 95.
  - `PROGRESS.md` does not record rev 4 or the decision.
  - The plan title still frames rev 3's goal.
- **`PREREGISTRATION.md` rev 2** should state:
  - (a) rev 1's outcome (R2/R4/R5/R6 FAIL on the 10M dry run) and that the dovetail decision came from that dry run, which rev 3 said would take "no decision". That is acceptable because dropping a default is the conservative direction, but it should be recorded as a deviation;
  - (b) which cells (NA12878 and HG002 B-old) had already produced data, and which of their figures had been seen at freeze time, so readers can judge blinding. HG002 B-old is effectively the HG002 new default;
  - (c) the binary commit, which is not known until step 1.
- **Docs.**
  - The "Choosing an engine" table already has a "Bowtie 2 `-X 800`" row with exactly the new default's numbers. "Add a row" would duplicate it. Relabel instead:
    - "Bowtie 2 (default, 800 bp cap)";
    - "Bowtie 2 `-X 500` (before this change)";
    - "Bowtie 2 `--local` (500 bp cap)".
  - Also rewrite:
    - line 75-77 ("the engine's default maximum insert is 500 bp", which is now Bowtie 2 800 and HISAT2 500);
    - line 115-116 ("raise the cap when you use Bowtie 2"), which is now the default.
  - Add the Q5 line (`--dovetail` has no effect; dovetailed pairs are not kept), plus the fact that `--no_dovetail` still changes the TLEN sign of equal-extent OB pairs.
  - The `-X` CLI help does not name any default, and `--no_dovetail`'s help ("Disable the automatic `--dovetail`") is misleading on this path. Optional.
- **Status journal.** Per the project convention, update the `rust/README.md` row and Milestones on merge. Optional.

## 6. What could still go wrong for users

- **Long-fragment pairs are not checked per pair.** See I1. About 7% of the new default's pairs are 500–800 bp pairs that no check has looked at pair by pair. #1146's gain-set guard, the only check aimed at the gained region, failed (r 0.937). It was explained away as a cov 10–11 effect, which is plausible but never confirmed for this set.
- **Other libraries.**
  - cfDNA (about 167 bp) and short-insert libraries: no effect.
  - Long-insert libraries: strictly better than 500.
  - Repeat-rich regions: a wider mate-rescue window can place more mates wrongly. The MAPQ-0 (+2 pt) check covers only the extreme case. Reporting the changed-set size (POS/CIGAR changed between B-500 and B-new) costs nothing with the helper.
- **The default engine may carry the same artefact.** minimap2, the 5-Base default, keeps pairs on its own proper-pair flag (`five_base_emit_pe_record`, the `flag & 0x2` check), and it has no dovetail filter that I can find. If minimap2 flags these overhang pairs as proper, its users already get the non-genomic 3′ overhang bases. The dry run measured a 35% mismatch rate in those bases and an R1 3′ M-bias of 27.6% vs 50.8%. Rev 4 does not mention this. It is outside this PR, but it belongs in RESULTS.md and a follow-up issue. The `rescued` helper can measure it directly on an A-arm BAM against B-new. Whether minimap2 admits these pairs is a hypothesis I have not verified.
- **Downstream snapshots.** Any nf-core or methylseq test that runs `--illumina_5base --bowtie2` will see a new report options line and different counts. Check before release.

## 7. Action items

### Critical
- **C1. Make the harness pieces the outline depends on explicit.**
  - Parameterise `repro` by arm (it is B-old-only today).
  - Spell out `verdict --secondary-only`: the comparator arm, the score keys and labels, an optional `--rescued`, and no automatic consensus.
  - Give B-500 its `verify_options`, `once` and self-test cases.
  - Flip B-new's self-test.

  Without these, step 4 on oxy cannot run as written.
- **C2. Give the `repro` gate a failure rule and state what it really tests:**
  - that #1121, #1145 and the extractor commits since `9b5c15b` are output-neutral;
  - what happens if it fails: diagnose against the rev 3 B-old cell, and do not treat it as a `--maxins` failure;
  - the rev 3 B-old NA12878 `repro` result, if available.

  Also state the B-old ≡ B-new cross-check as an expected exact match, so any difference stops the work.

### Important
- **I1. Add a pre-registered per-pair check of the pairs the 800 cap admits.** On HG002, run `rescued` with B-500 as old and B-new as new: R2, R3, R5 and R6, a mismatch-rate replacement for R4, R1 widened or report-only, and the changed-set size reported. Rev 3's own lesson is that genome-wide checks cannot see per-pair harm, and rev 4 drops the only per-pair tool for the change that ships.
- **I2. The real-Bowtie 2 flipped dovetail test needs a positive control** so that "dropped" is not vacuous. It should also assert that explicit `--dovetail` drops the pairs too, which pins Q5.
- **I3. Fix B-loc's meaning in the harness.** Add `-X 500`, require `--maxins 500` exactly once, and decide on `--no_dovetail` for B, B-loc and B-X. This bug was latent in rev 3 too.
- **I4. Docs.**
  - Relabel the existing rows rather than adding a duplicate.
  - Correct lines 75-77 and 115-116.
  - Mark the `--local` row as measured at 500.
  - Note that HISAT2 stays at 500.
  - Add the Q5 line and the `--no_dovetail` TLEN caveat.
  - Scope the claim to two 5-Base WGS gDNA libraries.
- **I5. Decide and record the fate of the rescued/duplex tooling in the PR.** Also restore `--dovetail` to the deny list in `five_base_omits_bisulfite_policy_and_runtime_threads` instead of using an engine-dependent count.
- **I6. `PREREGISTRATION.md` rev 2** records rev 1's outcome and the dry-run deviation, the cells and figures already seen (especially HG002 B-old), and the binary commit.

### Optional
- **O1.** Validate `-I` > effective `--maxins` on 5-Base Bowtie 2 (a clear error), or at least document it. Bowtie 2 accepts it silently and the driver then drops every pair.
- **O2.** Drop `--five_base_umi_qname` from B-new and B-500 now that the duplex pass is gone, so B-new is exactly #1146 B-X minus `-X 800`.
- **O3.** Record a follow-up on whether minimap2, the default engine, admits the same non-genomic dovetail overhangs; the helper can measure it on an A-arm BAM.
- **O4.** Fix the stale "Q5 open" line in the revision history, update `PROGRESS.md` for rev 4, and retitle the plan heading if needed.
- **O5.** Add a test that pins `--no_dovetail`'s remaining effect, the TLEN sign of equal-extent OB pairs, since that is now its only effect on this path. Clarify the CLI help.
- **O6.** Check downstream nf-core/methylseq 5-Base snapshots before release.

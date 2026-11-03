# Plan review, rev 5: Reviewer B

Target: `PLAN.md` "Rev 5" section, read with rev 4's Goal and Behavior and the implementation notes. I checked claims against the code at `f3e860f` (the rev 3 builder plus harness) and against the #1146 committed results. I formed this assessment before reading `PLAN_REVIEW_R4_A.md` and `PLAN_REVIEW_R4_B.md`; the cross-check is in section 7.

## Verdict

Rev 5 is sound on the shipped change. The builder edit, the test plan and the NA12878 gate are right. I verified the NA12878 gate's equivalence claims in code. One problem in the pre-registered harness will crash the HG002 gate as written (C1). Four Important items are harness and criteria-design issues that should be fixed **before** `PREREGISTRATION.md` rev 2 is frozen, because changing them afterwards would be a deviation.

## 1. Logic review

### 1.1 The `-I` > cap early error: verified, with gaps in the test list

- **The failure mode is real.** I ran Bowtie 2 2.5.5 locally on a 600 bp pair with `-I 900 -X 800`. It exits 0, reports "aligned concordantly 0 times; 1 aligned discordantly", and emits flags 97/145 (no `0x2`). `five_base_emit_pe_record` drops every such pair (`mod.rs:2479-2483`). That is silent total loss.
- **Placement is right.** `build_five_base_aligner_options` runs at `config.rs:1028`, before `resolve_output` creates the output directory (`config.rs:1714`). The planned "fails before any output is created" CLI assertion is therefore achievable. It runs after `discover_genome_for_run` and `detect_aligner`, so the CLI test needs the usual genome fixture and fake engine; `FiveBaseOptionsFixture` already provides both.
- **The explicit-`-X` branch has no test.** Behavior says the cap in effect is "`-X` or the 800 default". The tests cover `-I 900` (default cap), `-I 900 -X 1000` (pass) and `-I 800` (equal, pass), but nothing for `-I 600 -X 500`, an explicit `-X` below `-I`. That branch is half of the new code. Add it, with an exact match on the error string, including the `(<X>)` value.
- **Rev 3 Behavior §1 is now contradicted.** It still says "`-X` smaller than `-I` gets no new validation" (PLAN.md:288). Rev 5 says rev 4 Behavior applies, and rev 4 does not repeat that line, so nothing is technically wrong. A one-line "superseded" note avoids an auditor flagging it.
- **HISAT2 has the same silent zero.** On 5-Base HISAT2, `-I 600` alone runs against HISAT2's own 500 bp default and drops everything, and so does `-I 900 -X 500`. Rev 5 excludes HISAT2 by design. That is defensible, because nothing about HISAT2 changes in this PR, but the docs line about the new error should say it is Bowtie 2-only. Alternatively, extend the check to HISAT2 with a cap of `-X` or 500. That is a one-line change with no effect on valid runs.
- **No existing user or test breaks.** Every `-I` in the test suite is ≤ 10 (`aligner_cli.rs:7258,7366,7619-7656`; `options.rs` tuning test). The docs example is `-I 10 -X 1000`. Before this branch, `-I 600` alone on 5-Base Bowtie 2 already lost every pair at Bowtie 2's 500 default, so the new error only turns an existing silent failure into a loud one.
- **Wording.** "raise -X/--maxins" fits the default-cap case. When `-X` was explicit, "lower -I/--minins or raise -X/--maxins" is more accurate. This is optional.

### 1.2 Builder and unit-test list

- The code change is simply reverting `options.rs:282-286` (the `--dovetail` push) and keeping the `match cli.maxins` default. That is correct.
- **Tests the list misses:**
  - `five_base_local_scoring_is_engine_specific` (`options.rs:679-690`) carries `--dovetail` in two exact strings. It is not in the "Existing `options.rs` tests" list, though the test run would catch it.
  - The test names `five_base_bowtie2_defaults_to_dovetail_and_maxins_800` (`options.rs:625`) and `five_base_bowtie2_dovetail_and_maxins_defaults_reach_argv` (`aligner_cli.rs:7575`) describe rev 3's behaviour. Rename both, as the real-Bowtie 2 test is renamed.
  - The rev 3 code comment at `options.rs:282` goes with the push.
- **Real-Bowtie 2 dovetail test.** `SmallGenome::align` (`aligner_five_base_bowtie2.rs:184`) writes exactly one pair, named `pair`. "Each run aligns three pairs" needs a multi-pair variant with distinct names, so that "both records from the control" can be asserted by QNAME. This is only an implementation detail, but the outline should say the helper changes.

### 1.3 NA12878 gate and the B-old cross-check: claims verified

- **B-new is byte-for-byte #1146 B-X's invocation.** #1146's B-X report shows `-q --score-min L,0,-0.6 --maxins 800 -p 32 --reorder`, with no `--no_dovetail` and no `--five_base_umi_qname`. Rev 5 B-new emits the same string and has `config.dovetail = true`. Dropping `--five_base_umi_qname` is what makes this exact.
- **"Only TLEN signs of equal-extent OB pairs can differ" is exactly right.** I checked it against Bowtie 2's pair classifier (fw mate's right end past the rc mate's right end counts as a dovetail) and the driver's TLEN branches (`output.rs:571-597`).
  - The `flag_1 == 99 && dovetail` branch needs `start2 < start1`, which is a dovetail and unreachable without the token.
  - The `flag_1 == 83 && dovetail` branch needs `Fs == Rs` and `Fe == Re` once dovetails are excluded, which is equal extent.
  - Magnitudes are identical in both branches, so `readthrough` and the TLEN quantiles, which take `|TLEN|`, cannot differ.
- **`RX` touches no metric.** `--five_base_umi_qname` only calls `set_rx` (`mod.rs:2376-2381`). Nothing under `dedup/` or `extractor/` reads `RX` or TLEN (grep: no `rx()`, `template_length` or `tlen` hits). Deconvolution reads neither. So "B-new and B-old match on every non-timing metric" is a sound expectation.
- **Failure branch.** "If B-old fails in the same way, the cause is drift from the intervening dev commits" overstates the case. B-old shares with B-new the binary lineage (`d1764d0` base), the inputs (`repro-input` md5s), the reference staging and the oxy environment. A shared failure points to *any* shared factor, not only commits. Tool versions are checked, but the Bowtie 2 index and genome FASTA are not checksummed: Reviewer A's rev 4 O3 was not adopted. Say "drift shared by both binaries (dev commits or staged references)", or adopt O3.

### 1.4 HG002 admitted-pair criteria A1–A6

**Definitions are implementable with `five_base_rescued.py` as it stands**, given `--old-arm/--new-arm`:
- **"Absent from B"** is the existing `o is None` branch (`five_base_rescued.py:316-317`). Both BAMs hold only proper pairs, because the driver drops the rest, so "proper in B-new" is the same as "present in B-new".
- **Comparator.** B (`-X 500`) is #1146's B: Bowtie 2's own `-X` default is 500, and B-X's and B's reports differ only by the token.
- **A1's 1–15% range is well placed.** NA12878 #1146 gives (92,666,181 − 86,206,533) / 92,666,181 = 6.97%. HG002's minimap2 TLEN p90 is 455 against NA12878's 478 (`10292026_5base-default-engine/bench_results/HG002/screen/A.rep1/metrics.tsv`), so expect roughly 4–6% on HG002. That is inside the range. It is also close to C1's ~5% crash threshold, and NA12878's 7% is clearly above it.
- **A4's and A5's n ≥ 10,000 are trivially reachable.** That means millions of bases and calls. So INSUFFICIENT can only result from a bug. That is fine, but n is no longer a meaningful guard.

**Three design problems:**
- **C1 (crash): the A5 rest pool is too small.** See Critical C1 below.
- **I3: regional confounding.** A3, A5 and A6 compare absolute levels between long-insert pairs and the rest. Long fragments come preferentially from different sequence contexts. GC-rich fragments, and so CpG islands, skew short. So the admitted set can legitimately be more methylated and score lower on per-call DRAGEN agreement:
  - Agreement rewards near-0 or near-1 sites, and CGI sites are near 0.
  - The dry run's dovetail harm moved A5's metric by only −0.023 (0.761 vs 0.784). A composition shift of the same size is plausible.
  - A6 uses the **absolute** CpG level over the R1 3′ last 10 cycles with a ±3 pt window, while A3 allows ±5 pt on the overall level. A genuine 3–5 pt composition difference passes A3 and fails A6, even with no M-bias at all. The criteria are inconsistent with each other.
- **I4: all-bases cost.** See Important I4.

**A4's "cycle-matched" is well defined in all-bases mode.** `matched()` re-weights the control's per-cycle rate to the admitted cycle mix. With all bases on both sides, the mixes are nearly identical, so the matching is close to a no-op. One asymmetry: the control tally keeps only rest bases that are *not* overhang (`ctrl_t.add(m, lambda i: i not in ob)`), while `--all-bases` tallies every admitted base. Make the control all-bases too in that mode. The difference is negligible, but it is a free fix and keeps the definition symmetric.

**Mechanism check.** A1 is a fraction range. A sharper and equally cheap mechanism check is "≥ 95% of admitted pairs have 500 < |TLEN| ≤ 800". The plan already reports admitted |TLEN| quantiles, so pre-register this as a threshold rather than leave it as a figure to eyeball. Pairs admitted with |TLEN| ≤ 500 would mean the `-X` change altered Bowtie 2's search beyond the cap, which is worth knowing.

### 1.5 Harness and oxy ordering

- **I1: `repro` output paths are not per arm.** `cmd_repro` writes `$RESULTS/$s/$tag/repro.tsv` and `$ROOT/runs/$s/$tag/repro_score.tsv` (bench `:383,395`), and copies `repro_score.tsv` into `$RESULTS`. Step 4.2 runs `repro NA12878 screen B-old`, then step 4.3 runs `repro NA12878 screen B-new`, which **overwrites the B-old cross-check**, the very result RESULTS.md needs to state the "same verdict" expectation. Key the files by arm (`repro_<arm>.tsv`, `repro_<arm>_score.tsv`) and make the header column the arm name.
- **I2: B-old can still be wiped, and the harness is shared.**
  - The `cell` DONE key includes the commit from `bismark --version` (`:217-220`). Any `cell <s> screen B-old` run under `bin5` gets a different key and runs `rm -rf "$d"` on the cross-check BAMs (`:221`). Rev 5 keeps B-old "only to identify the running rev 3 cells" but does not stop that. Make `cmd_cell` die for arm `B-old`; `repro` and `verify_options` can keep the definition.
  - `BIN` is isolated, but the **harness script and `runs/` tree are not**. Deploy the rev 5 harness to its own directory (for example `~/5base_harness_bt2_r5`) rather than overwriting `~/5base_harness_bt2` while rev 3 streams are executing from it.
  - Confirm on oxy what the rev 3 stream scripts do after their B-old cells. Rev 3's outline continued into NA12878 B-new, `rescued` and `duplex`. A rev 3 B-new would write to `runs/NA12878/screen/B-new.rep1`, the same directory rev 5's B-new uses. The implementation note says work stopped before any screen B-new cell, but it does not say the stream scripts were edited. Check `ps`/the stream scripts before step 4.3.
- **HG002 B-old has no consumer in rev 5.** Its option string equals B-new's. Either kill it to free oxy before stream 2 starts, or use it like NA12878's B-old: it is a free cross-check of HG002 B-new, with the same expectation of matching non-timing metrics. As written, step 4.2 waits for it and then ignores it. Also, freezing PREREGISTRATION rev 2 before it finishes avoids the disclosure problem entirely.
- **Two concurrent streams.** Give them disjoint `CPUS` (for example 0-31 and 32-63, as #1146 did). Timings are excluded, but contention can roughly double wall time on an ephemeral pod.
- **Removing the duplex code loses nothing rev 5 needs.** The family key's overhang-split concern only applied to dovetailed pairs, and the dry-run D0–D3 results remain reproducible from `909182d`/`f3e860f`. RESULTS.md should cite that commit for the dry-run figures, because HEAD will no longer contain the code that produced them. The removal also saves the all-pairs `families.unsorted` write and sort.

## 2. Assumptions

| Assumption | Status |
|---|---|
| Bowtie 2 silently accepts `-I` > `-X` | Verified locally (2.5.5). |
| B-new ≡ #1146 B-X invocation | Verified from #1146's committed report. |
| B-old vs B-new differ only in TLEN sign of equal-extent OB pairs | Verified (Bowtie 2 classifier and `output.rs:571-597`). |
| No metric reads `RX` or TLEN sign | Verified (dedup, extractor, deconv: no reads; harness uses `\|TLEN\|`). |
| B `-X 500` ≡ #1146 B | True (Bowtie 2's own default is 500). |
| B-loc at rev 5 needs `-X 500` to match #1146 B-loc | True: rev 3/5 builders default to 800. |
| Admitted pairs ≈ 7% (NA12878), 4–6% (HG002) | Consistent with #1146; this is what triggers C1. |
| Long-insert pairs have the same methylation make-up as the rest | **Not stated, and probably false** (I3). |
| Output-neutrality of the commits since `9b5c15b` | Tested by the gate, not assumed. Good. |
| Docs line references 75–77 and 113–116 | Verified at HEAD. |

## 3. Efficiency

- **`rescued` on HG002 at 100M pairs** parses roughly 180M + 170M SAM records in pure Python. That is already hours.
- **`--all-bases` adds a lot on top.** About 4.5–6.5M admitted pairs × ~290 aligned bases is about 1.5–2 × 10⁹ iterations of `CycleTally.add`, each with a `cycle()` call, an `xm` index, a set lookup and a dict increment. That is plausibly another 1–3 h. The implementation notes already sub-sampled the rest control to 1% for exactly this reason. Do the same for the admitted A4 tally with the seeded draw, for example 5% (n is still ~10⁷). Keep A2, A3 and A6 on all admitted pairs, because they use cheap per-pair counts.
- **Memory.** `rescued_calls` plus the larger pool C1 needs, plus `needed`/`p` in `per_call_agreement`, comes to a few GB of Python ints and tuples. That is acceptable on oxy, but if the pool is raised, store pool entries as parallel `array`s rather than a list of tuples.

## 4. Alternatives

1. **A5 site-matched.** Restrict both the admitted and the rest-sample call sets to CpG sites that carry at least one call in each. Or score each admitted call against the rest calls at the same site, not against DRAGEN alone. This removes regional composition (I3) at negligible cost.
2. **A6 as difference-in-differences.** Compare (3′ last-10-cycle CpG − mid-read CpG) between admitted and rest. This isolates M-bias from the overall level, which A3 already covers. The dry run's dovetail signal (27.6% vs 50.8% at the 3′ end against a similar mid-read level) would still fail it loudly.
3. **NA12878 B cell.** One extra ~2.5 h cell would give A1–A6 on two samples, as rev 3 had. #1146's B BAMs were lost with scratch, so NA12878 currently gets only the reproduction gate. This is optional. It is the only per-pair check on the sample the docs table describes.
4. **Extend the early error to HISAT2** (cap = `-X` or 500), versus documenting it as Bowtie 2-only. Either is fine; choose explicitly.

## 5. Validation sufficiency

- **Covered well:** the option strings, both sides of the insert cap, dropped dovetails with a positive control, Q5 pinned, the shipped-default reproduction, and a per-pair check on the pairs the change actually admits.
- **Gaps:**
  - The explicit `-X` error branch (1.1).
  - Overwritten cross-check results (I1).
  - A cross-check cell that can be wiped (I2).
  - A pool crash (C1).
  - Criteria that can FAIL from composition, not harm (I3). A spurious FAIL is not silent, because it goes to Felix, but it costs a cycle. A composition effect in the other direction could mask a small real harm.
- **Nothing in rev 5 could silently produce a wrong verdict**, beyond I3's masking direction. Every failure path is a die or a FAIL.

## 6. Action items

### Critical

1. **C1. The A5 rest pool is smaller than the admitted set, so `rescued` exits.** `cmd_rescued` samples rest pairs with `pr < POOL_FRAC = 0.05` (`five_base_rescued.py:41,333`), then dies if `k = admitted pairs > len(pool)` (`:344-346`).
   - On NA12878, admitted is ~7% of B-new's pairs, while the pool is ~5% of the rest (~4.3M) against ~6.5M admitted. That is a certain exit.
   - On HG002 at ~4–6%, it is a coin flip.
   - The pool fraction is a pre-registered number (implementation notes, "Random subsets"), so it must change in rev 5 and in PREREGISTRATION rev 2, not after the freeze. Either raise `POOL_FRAC` to ≥ 0.25 and keep the equal-size sample, or drop the equal-size requirement and score the whole pool. Mean score does not depend on sample size. Add a self-test case where admitted > 5% of pairs.

### Important

2. **I1.** Key `repro`'s outputs (`repro.tsv`, `repro_score.tsv` and their `$RESULTS` copies) by arm, so the B-new gate does not overwrite the B-old cross-check (1.5).
3. **I2.** Make `cmd_cell` refuse arm `B-old`, because the commit-keyed DONE check runs `rm -rf` on the cross-check otherwise. Deploy the rev 5 harness to its own directory. Before step 4.3, confirm that no rev 3 stream script will go on to a rev 3 B-new in the shared `runs/NA12878/screen/B-new.rep1` (1.5).
4. **I3.** De-confound A5 and A6 before the freeze: A5 site-matched, and A6 as 3′-minus-mid-read difference-in-differences. At minimum, make A6's window consistent with A3, because ±3 on an absolute level is tighter than A3's ±5 on the same level. State the composition assumption explicitly (1.4, 4).
5. **I4.** Sub-sample the admitted set for the `--all-bases` A4 tally with the seeded draw, and tally the control's bases symmetrically in that mode. Pre-register the fraction (3, 1.4).
6. **I5.** Add the explicit-`-X` early-error test (`-I 600 -X 500` errors, with an exact message), and update `five_base_local_scoring_is_engine_specific`. Rename the two rev 3 dovetail-named tests (1.1, 1.2).

### Optional

7. **O1.** Pre-register an A1-style mechanism threshold: ≥ 95% of admitted pairs have 500 < |TLEN| ≤ 800.
8. **O2.** Decide on HG002 B-old: kill it, or use it as HG002 B-new's cross-check. Freeze PREREGISTRATION rev 2 before it finishes.
9. **O3.** Give the two oxy streams disjoint `CPUS`.
10. **O4.** Reword the repro-failure branch to "shared drift (dev commits or staged references)", or checksum the Bowtie 2 index and the genome FASTA in `versions` (R4 Reviewer A O3).
11. **O5.** Say in the docs that `-I` above the cap now errors, and on which engines. Extend the check to HISAT2 (cap `-X` or 500), or state that it is Bowtie 2-only. Use the message "lower -I/--minins or raise -X/--maxins" when `-X` was explicit.
12. **O6.** Mark rev 3 Behavior's "`-X` smaller than `-I` gets no new validation" as superseded.
13. **O7.** Cite `f3e860f` (or `909182d`) in RESULTS.md as the code that produced the dry-run D0–D3 and R1–R6 figures, since HEAD drops it.
14. **O8.** Have `cell` assert that `bismark --version`'s commit equals the pre-registered rev 5 commit (`EXPECT_COMMIT`), so a wrong `BIN` fails loudly instead of only changing the key.
15. **O9.** Optionally add an NA12878 B cell for a two-sample A1–A6 check.

## 7. Cross-check against the rev 4 reviews

All the R4 items rev 5 claims to address are present:
- Reviewer A: C1 (repro takes an arm), C2 (`bin5`), I1 (A1–A6), I2 (no umi_qname), I3 (failure branch), I4 (control pair, three runs), I5 (forbidden list, tuning counts, end-to-end report), I6 (docs), I7 (PREREGISTRATION disclosure, R1–R6/D0–D3 withdrawn), I8 (duplex removed, verdict parametrised), O2 (B arms reverted to #1146 flags, B as the HG002 comparator), O4 (dry-run results committed), O5 (squash message) and O6 (follow-ups).
- Reviewer B: the `-I` check, the B-loc bug, the minimap2 follow-up and the selftest rewrite.

Not adopted, and not flagged in rev 5 as declined:
- A O1 (10M equivalence instead of 100M cells). Fine, since keeping the cells is the stronger provenance.
- A O3 (reference md5s; see O4 above).
- A O7 (CLI help). The help text states no default, so there is nothing to fix there.
- B's `rust/README.md` status-journal row. This is a merge-time chore; add it to step 7.

C1, I1, I2's harness-sharing part, I3 and I4 are new in rev 5. They were not raised in rev 4 because rev 4 had no admitted-pair analysis, and `repro` was not yet parametrised.

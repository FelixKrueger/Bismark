# Plan review R4 — Reviewer A

Target: `PLAN.md` § "Rev 4 (2026-11-02): `--maxins 800` only", read against rev 3, the implementation notes (dry run #5), the code at `f3e860f` (builder at `72bbf7b`), the harness, and #1146's committed B-X results.

## Verdict

The direction is sound and the core claim mostly holds: the new default reproduces #1146 B-X's option string exactly. Five things need fixing before the plan runs. Three are about evidence. The NA12878 gate, as the harness is written, would test the wrong cell. HG002 is judged only by the genome-wide checks that dry run #5 showed cannot see set-level harm. And `--five_base_umi_qname` is still on the arms, though nothing reads it any more. The other two are operational: rebuilding the binary while the rev 3 B-old cells are still running, and the dangling rev 3 harness and pre-registration machinery.

## 1. Logic review

### 1.1 "Byte-for-byte B-X" — checked

- **Option string: true.** #1146 B-X's committed report (`bench_results/NA12878/screen/B-X.rep1/*_PE_report.txt`) shows `-q --score-min L,0,-0.6 --maxins 800 -p 32 --reorder`. With `--dovetail` removed, the builder emits `--maxins 800` in the same position as an explicit `-X 800` did (`options.rs:290-294`). The argv is therefore identical.
- **`config.dovetail`: matches, and better than rev 3 did.** #1146 ran B-X as `--bowtie2 -X 800` with no `--no_dovetail` (`git show d1764d0:validation/five_base_engine_bench.sh:41`), so `config.dovetail = true`. That is what the rev 4 default has.
  - Rev 3's B-old adds `--no_dovetail`. That flips TLEN on equal-start OB pairs (`output.rs:571-577`, `flag_1 == 83 && dovetail`).
  - No harness metric sees this. TLEN quantiles use `|TLEN|` (bench `:263`), and dedup and the extractor never read TLEN (grep of `src/extractor`, `src/dedup`, `io/pair.rs`: no TLEN/ISIZE use).
  - So the plan's line "the rev 3 B-old cells… builder string is identical, but the binary is not the shipped one" undersells it. They are metric-equivalent to the new default on every committed metric. The only BAM difference is that TLEN sign. See 4.1.
- **`--five_base_umi_qname`: harmless to metrics.** It only sets `RX` (`mod.rs:2376-2381`). Nothing in dedup or the extractor reads `RX`, and the harness runs dedup without `--barcode`. But it now has no consumer. See 1.4.
- **The binary has changed since #1146's `9b5c15b`.** It has:
  - #1121 FIFO streaming (`stream.rs` +1.9k lines, `align.rs`, `convert.rs`);
  - #1145 CRLF / stalled merge;
  - the extractor's #1115 `--allow_discordant` and the orphan-heavy change (`pair_class.rs`, `overlap.rs`, `parallel.rs`, `pipeline.rs`).

  Nothing guarantees these leave 5-Base Bowtie 2 metrics unchanged. "Its alignments, TLENs and metrics are therefore those of B-X" is a **prediction the gate tests, not a fact**. Word it that way, and say what a failure means (1.2).

### 1.2 The NA12878 gate as specified

- `cmd_repro` is hard-wired to `$ROOT/runs/$s/$tag/B-old.rep1` (bench `:376`). Rev 4 says "the new default… must reproduce #1146 B-X… through the harness's `repro` check". Outline step 2 doesn't list making `repro` take an arm argument. Run as written, the gate checks the rev 3 B-old cell, not the shipped default. **Critical**, because it silently tests the wrong thing.
- Rev 4 doesn't say what a gate failure does. Rev 3 said "A failure stops the work". Rev 4 needs a pre-declared branch.
  - If the rev 3 B-old repro (on `72bbf7b`) also fails with the same numbers, the drift comes from the intervening `dev` commits (#1121, #1115). That is a separate regression finding, not a verdict on `-X 800`.
  - If only B-new fails, the rev 4 builder change is wrong.

  The two cases have different owners. Decide now.

### 1.3 HG002: is the evidence enough?

- **The comparator is right.** `-X 500` without `--no_dovetail` is exactly the `dev` default. The old default emitted no `--maxins` token, Bowtie 2's own `-X` default is 500, and `config.dovetail` was true. A minor difference: B-500's report shows `--maxins 500` where `dev` showed nothing. Note it so nobody reads it as a different arm.
- **The pass criteria are the ones dry run #5 discredited.** The lesson of #5, in the plan's own words: "the genome-wide checks cannot see the harm". The dovetail-admitted set had CHG +0.8 pt and an M-bias drop of 23 pt, and every secondary check passed.
  - `-X 800` admits about 6.6 pt of pairs on NA12878 (87.9 → 94.5%), so the dilution is about 3× weaker than for `--dovetail`'s 2%. It is still large.
  - At about 7% of pairs, "CHG ≤ +0.2 pt" only fires if the admitted set is about 3 pt worse per pair. A defect the size of the dovetail one would pass.
- **The admitted-set check costs almost nothing here.** HG002 B-500 vs B-new is exactly an old/new pair of raw BAMs, and `five_base_rescued.py rescued` already computes the admitted set and these metrics:
  - CHG/CHH (R2) and CpG (R3);
  - per-call DRAGEN agreement (R5);
  - 3′ M-bias (R6);
  - the cycle-matched mismatch rate (the R4 machinery: run over all admitted-pair bases, not just overhang bases, it is a generic "non-genomic bases" detector).

  Rev 4 item 4 drops all of this as "tested what `--dovetail` admits". That is only true of the overhang geometry and of R1's 0.1–3% range. The admitted pairs should also show `|TLEN|` mostly in 500–800, and the "changed" set (pairs whose placement moves because a longer concordant pair is now allowed) should be reported.
  - **Recommendation:** keep R2/R3/R5/R6 plus the all-bases mismatch check for HG002, B-500 → B-new. Replace R1 with a pre-registered range for the admitted fraction (for example 2–15%) plus a check that it is mostly 500–800 bp. Gate on them, or at least pre-register them as reported with thresholds.
  - Optionally, add an NA12878 B-500 cell (about 2.5 h) for the same analysis on the primary sample. #1146's B BAMs were lost with scratch.

### 1.4 `--five_base_umi_qname` on the arms (and B-500)

- It existed only to feed the duplex pass (rev 3, N1), and rev 4 drops the duplex/consensus pass. Keeping it has three costs:
  - B-new is no longer "`--bowtie2` alone", so Validation 2's "(`--bowtie2` alone, with `--five_base_umi_qname`)" contradicts itself;
  - the arm isn't the literal #1146 B-X invocation;
  - `verify_umis` stays as a dependency.
- Drop it from B-new and B-500. Then B-new is literally both the shipped default command and #1146's B-X invocation, minus `-X`. If the rescued analysis is kept (1.3), it doesn't need `RX` either: only D1 used it.

### 1.5 The flipped real-Bowtie 2 dovetail test

- The test is well defined, and the evidence is already in. Rev 3's `keeps_dovetailed_pairs_unless_no_dovetail` showed, against real Bowtie 2 at `72bbf7b`, that both fixtures come back empty when no `--dovetail` token is passed (its `--no_dovetail` branch). That is exactly the rev 4 default.
- **It loses its positive control.** In rev 3 the "kept with `--dovetail`" branch showed that the fixture aligns at all. In rev 4 no CLI path admits a dovetailed pair, because Q5 makes `--dovetail` a no-op. A bare "empty" assertion would also pass if the fixture broke: a bad seed, a revcomp error, or a genome change.
  - Add a non-dovetailed control from the same genome region, built like the OT fixture (R1 at 2000..2100, R2 = revcomp of 2020..2120). It must come back as two records with flag 0x2 and `|TLEN|` 120, which proves the mates align.
  - Assert empty for `[]`, `["--dovetail"]` **and** `["--no_dovetail"]`. The `--dovetail` case pins Q5 against the real engine, not just in the builder string.
  - Optionally, assert that the report counts the pair under "no alignment" and that the run exits 0, so it is dropped rather than an error.
- Rename the tests. `keeps_dovetailed_pairs_unless_no_dovetail`, `five_base_bowtie2_defaults_to_dovetail_and_maxins_800` and `five_base_bowtie2_dovetail_and_maxins_defaults_reach_argv` all now describe the opposite of what they assert.

### 1.6 Unit and CLI test specifics the outline doesn't spell out

- `five_base_omits_bisulfite_policy_and_runtime_threads`: rev 3 took `"--dovetail"` out of the forbidden-token list and added a per-engine `dovetails == usize::from(engine == Bowtie2)` count (`options.rs` around `:700-713`). Rev 4 must **put `--dovetail` back on the forbidden list for every engine** and delete the count. Otherwise that test still accepts a `--dovetail` token.
- `five_base_1134_tuning_reaches_each_aligner_and_report`: rev 3 added a `--dovetail` count equal to `usize::from(engine == "bowtie2")`. Flip it to 0.
- The CLI `..._reach_argv` cases `("bowtie2", &["--dovetail"], true, …)` and the others must expect 0. Keep the `--dovetail` case, since it pins Q5 at the argv level.
- `five_base_bowtie2_unconverted_index_end_to_end` asserts `report.contains("-q --score-min L,0,-0.6 --dovetail --maxins 800")`. Update it, and add `!report.contains("--dovetail")`.

### 1.7 Smaller logic points

- The revision history (rev 4 line) says "Q5… is open", but "Resolved question" says it is resolved. Fix the history line.
- The `72bbf7b` commit message says "defaults to --dovetail --maxins 800". If the PR is squash-merged, rewrite the squash message. A merge commit would leave the reverted rationale in history, which is fine but worth a sentence in the PR.
- The harness header comment (bench `:22-24`) and the `verify_options`/selftest B-new entries (`:78`, `:92`, `:496-507`) all encode "B-new has `--dovetail`". The outline mentions the redefinition but not the selftest rewrite. List it explicitly: B-new must now **deny** `--dovetail`, and need `--maxins 800` exactly once.

## 2. Assumptions

| Assumption | Status |
|---|---|
| The new default's argv equals #1146 B-X's | ✔ verified from the committed report |
| `config.dovetail` matches B-X | ✔ (B-X had no `--no_dovetail`). Rev 3 B-old does not, but no metric reads TLEN sign |
| `--five_base_umi_qname` changes no metric | ✔ for dedup, extractor and deconvolution (only `RX` is written). Now unnecessary (1.4) |
| Intervening commits (#1121, #1145, #1115) leave 5-Base Bowtie 2 metrics unchanged | **Unverified**: this is what the gate tests. State the failure branch (1.2) |
| Same Bowtie 2 index bytes and genome as #1146 (`~/references/GRCh38/Bowtie2Index`, `$ROOT/genome`) | Implicit. oxy recycles wipe `/var/tmp`, and if `$ROOT/genome` was re-staged, a different FASTA would break the repro. Record the index and genome md5s in `versions` |
| `-p 32` + `--reorder` makes output thread-invariant | Fine. THREADS=32 matches #1146 anyway |
| Secondary thresholds can tell good from bad for a ~7% admitted set | **Weak**, per dry run #5 (1.3) |
| PREREGISTRATION rev 2 is blind | **Compromised for HG002.** The rev 3 HG002 B-old cell is metric-equivalent to the new default (1.1) and is running now. If its numbers are seen before rev 2 is committed, rev 2 is not blind. The thresholds are inherited from rev 1, which mitigates this. Rev 2 must say that no threshold changed, and list which rev 3 results had been seen when it was written |
| `-I N > 800` with no `-X` | Bowtie 2 rejects it, naming a `--maxins 800` the user never typed. Before this branch the same happened at 500, so this is not a regression, but the error is now more confusing. Optional docs line |

## 3. Efficiency

- **Planned new compute:** NA12878 B-new, HG002 B-new and HG002 B-500. That is 3 × ~2.5 h, about 5 h wall in two streams.
- **The HG002 B-new cell is nearly redundant.** The running rev 3 HG002 B-old cell has the identical builder string, the identical intervening commits, and differs only in TLEN sign. The same holds for NA12878 B-new against the rev 3 NA12878 B-old repro.
- **A cheaper, equally rigorous alternative:** run a 10M B-new on the rev 4 binary. Compare it record-for-record with the dry-run 10M B-old BAM, ignoring `@PG` and the TLEN sign. If they are equal, the rev 3 100M B-old cells stand in for B-new on both samples, and only HG002 B-500 is new: about 2.5 h plus about 0.5 h instead of 7.5 h.
  - If Felix wants the shipped-binary cells regardless, that is defensible. But the plan should say why it spends the extra 5 h, and should add the equivalence check either way, as a free determinism assertion.
- **Operational hazard: Critical.** `BIN=$ROOT/bin` is shared (bench `:36`). Step 4 says "Rebuild at the step 1 commit" while "the rev 3 B-old cells [are] now running".
  - Each cell runs `deduplicate_bismark` and `bismark_methylation_extractor` from `PATH` after alignment finishes (`:249-251`). A rebuild mid-cell gives a mixed-binary cell.
  - Overwriting a running executable in place can also fail or corrupt it.
  - Separately, re-invoking `cell … B-old` after the rebuild changes the DONE key (the commit is part of it, `:221-223`), and the code runs `rm -rf "$d"`. That deletes the cross-check BAMs.
  - Fix: build into a separate `BIN` (for example `$ROOT/bin_r4`, with `BIN` overridable by an environment variable), or wait until the B-old cells finish. Never re-run `cell B-old`.

## 4. Alternatives

1. **Use the rev 3 B-old cells plus a 10M equivalence proof** instead of the 100M B-new cells (section 3).
2. **Retire `B-new` and reuse the reverted B-X arm.** Rev 3 pinned `--no_dovetail` onto B, B-loc and B-X only to stop them silently gaining `--dovetail`. That can't happen any more. Revert those pins and the arms are #1146's flags again.
   - Then `cell NA12878 screen B-X` plus `repro` is literally #1146's B-X re-run.
   - A B-new arm with no `-X` is still needed to prove the default path (the `@PG` "no `-X`" check). It and B-X should produce byte-identical BAMs apart from `@PG`, which is a stronger, free assertion.
   - B (`-X 500`, no `--no_dovetail`) becomes the HG002 comparator, so no new B-500 arm is needed.
3. **Fix the overhang rather than drop `--dovetail`** (future, not this PR). Soft-clip each mate's bases past the partner's outer end, as minimap2 and samtools `--clip-overlapping`-style tools do, and that would recover the ~2% of pairs without the non-genomic bases. A 35% mismatch rate on a 12 bp median overhang looks like an adapter or linker remnant Trim Galore misses on 5-Base. That would also matter to the bisulfite path, whose default *is* `--dovetail`. Worth one follow-up issue; out of scope here.

## 5. Validation sufficiency: gaps

- The gate checks the wrong cell unless `repro` is parametrised (1.2).
- HG002 has no admitted-set check (1.3).
- The real-engine dovetail test has no positive control (1.5).
- No test asserts the TLEN/`config.dovetail` behaviour on the default path for an equal-start OB pair. It is the same as on `dev`, so this is optional, but rev 4 changes no driver code anyway.
- There is no explicit "no collateral change" item in rev 4. Rev 3 Validation 7 (bisulfite option-string tests and Perl-oracle CI green) should carry over; step 6 runs "the full suite", which covers it implicitly. Say so.

## 6. Docs and CHANGELOG coverage

The rev 4 outline names only "the tuning section and the 'Choosing an engine' table" and the CHANGELOG. More needs touching:

- `docs/src/content/docs/rust/illumina-5-base.md`:
  - **`:75` (tuning):** "the engine's default maximum insert is 500 bp" must become "Bowtie 2 defaults to 800 bp in 5-Base mode; HISAT2 keeps its own 500 bp default". HISAT2 is unchanged, and users will assume symmetry. Put the one-line Q5 note (`--dovetail` has no effect on 5-Base Bowtie 2; dovetailed pairs are not reported) here.
  - **Table `:104-109`:** "add a row for the new default" would **duplicate** the existing "Bowtie 2 `-X 800`" row, which already holds exactly those numbers.
    - Relabel instead: "Bowtie 2 (default, 800 bp cap)" with the B-X numbers, and "Bowtie 2 `-X 500`" (the earlier default) with the B numbers.
    - The **`--local` row** also needs a caveat. #1146's B-loc ran at the 500 bp cap, so it no longer describes default `--local`, which now gets 800. Relabel it "`--local -X 500`" or footnote it.
    - Optionally, add an HG002 column or footnote from this run.
  - **`:113-116` prose:** "raise the cap when you use Bowtie 2 on 5-Base data" is now wrong. Rewrite it.
- `docs/src/content/docs/options/alignment.md`:
  - `:98-100` says `-X` "Default: 500". Add the 5-Base Bowtie 2 exception.
  - `:265` says "by default Bismark calls Bowtie 2 with `--dovetail`". Add "not in `--illumina_5base` mode", or point to the 5-Base page.
- CLI help (`aligner/cli.rs:332`, `:353-357`): optional. `-X` help could note the 5-Base default, and `--no_dovetail`'s "Disable the automatic --dovetail" is misleading on 5-Base.
- CHANGELOG: one sentence, following the file's format, saying that 5-Base Bowtie 2 now caps inserts at 800 bp unless `-X` is given. No rationale.
- `rust/README.md` status journal: check whether the aligner row or the Milestones line needs a dated entry (per the project convention).

## 7. Leftover rev 3 artefacts (dangling)

- **`validation/five_base_rescued.py` and the harness `rescued`, `duplex` and `verdict` subcommands.** Under the redefined B-new, `rescued` would find about 0 pairs and the default `verdict` would fail R1. Pick one:
  - (a) keep them and re-aim them at B-500 → B-new for HG002 (recommended, see 1.3), with `--old`/`--new` arm names parametrised;
  - (b) keep them only to reproduce the dry-run evidence and say so in the header;
  - (c) remove them from the PR.

  The PR into `dev` shouldn't ship about 800 lines of dead harness without a stated purpose.
- **`criteria()` hard-codes the score keys `("B-old","common","10")` / `("B-new",…)`** and the "B-old" labels (`five_base_rescued.py:584-617`). `verdict --secondary-only` with a B-500 baseline needs the arm names passed in, or it will raise a `KeyError`. The outline should say so.
- **The dry-run results** (10M, `MIN_JOINED=0.2`) are the evidence for rev 4's main decision. Commit them under `bench_results/` and cite them in `RESULTS.md`, marked informational, so the decision can be audited.
- **`PREREGISTRATION.md` rev 2** must say that rev 1's R1–R6 and D0–D3 are **withdrawn**, not failed or passed, before the plan-manager audit. Otherwise the audit will flag them as unaddressed criteria.

## Action items

### Critical

1. **C1.** Parametrise `cmd_repro` (and its labels and `repro.tsv` header) by arm, and point the rev 4 gate at the new-default cell. As written, it checks rev 3's B-old (1.2).
2. **C2.** Don't rebuild `$ROOT/bin` while the rev 3 B-old cells run: their dedup and extract steps take binaries from `PATH`. Build into a separate `BIN`, or wait. Never re-invoke `cell B-old` after the rebuild, because the commit-keyed DONE check does `rm -rf` of the cross-check BAMs (3).

### Important

3. **I1.** Add an admitted-set check for HG002 (B-500 → B-new), reusing `rescued`: R2, R3, R5, R6, the all-bases cycle-matched mismatch check, a new pre-registered admitted-fraction range, the admitted `|TLEN|` distribution, and the changed-set count. Gate on these or pre-register them with thresholds. Dry run #5 showed the secondary checks alone cannot see set-level harm (1.3).
4. **I2.** Drop `--five_base_umi_qname` from B-new and B-500. It has no consumer now, and without it B-new is literally the shipped default and #1146's B-X invocation (1.4).
5. **I3.** Declare the repro-failure branch: intervening-commit drift, shown by rev 3 B-old also failing, versus a builder fault. Reword "therefore those of B-X" as a prediction (1.1, 1.2).
6. **I4.** Give the flipped real-Bowtie 2 test a positive control (a non-dovetailed pair from the same region is kept). Assert dropped for `[]`, `--dovetail` and `--no_dovetail`. Rename the dovetail-named tests (1.5).
7. **I5.** Restore `--dovetail` to the forbidden list in `five_base_omits_bisulfite_policy_and_runtime_threads`, flip the `--dovetail` counts in `five_base_1134_tuning…` and in the CLI `…reach_argv` test, and update `five_base_bowtie2_unconverted_index_end_to_end` (1.6). Rewrite the harness selftest and `verify_options` so B-new denies `--dovetail` (1.7).
8. **I6.** Fix the docs plan. Don't add a duplicate table row; relabel the B and B-X rows. Caveat the `--local` row (500 bp cap). Rewrite the "raise the cap" prose and the 500 bp tuning sentence (Bowtie 2 800, HISAT2 500). Add the 5-Base exceptions to `options/alignment.md` for `-X` and `--dovetail`. State where the Q5 line goes (6).
9. **I7.** PREREGISTRATION rev 2: state that the inherited thresholds are unchanged, disclose which rev 3 results (HG002 B-old, which is metric-equivalent to the new default) had been seen when it was written, and mark rev 1's R1–R6 and D0–D3 as withdrawn (2, 7).
10. **I8.** Decide the fate of `five_base_rescued.py` and the `rescued`/`duplex`/`verdict` subcommands: re-aim, keep as dry-run reproduction, or remove. Parametrise `criteria()`'s hard-coded arm keys for `--secondary-only` (7).

### Optional

11. **O1.** Replace the 100M B-new cells with a 10M rev 4 vs rev 3 BAM equivalence check, and let the rev 3 B-old cells stand in. This saves about 5 h. Or keep the cells, but add the equivalence check as a free assertion (3).
12. **O2.** Revert rev 3's `--no_dovetail` pins on the B, B-loc and B-X arms so they are #1146's flags again. Then use B as the HG002 comparator instead of a new B-500, and assert that B-X and B-new are byte-identical apart from `@PG` (4.2).
13. **O3.** Record md5s of the Bowtie 2 index and the genome FASTA in `versions`, so a re-staged reference can't masquerade as a code regression.
14. **O4.** Commit the 10M dry-run results and cite them in `RESULTS.md` as the basis for dropping `--dovetail`.
15. **O5.** Fix the rev 4 revision-history line ("Q5 … is open"), and rewrite the squash commit message, which still says "defaults to --dovetail".
16. **O6.** File a follow-up issue on the overhang finding: is it a 5-Base adapter or linker remnant? Does bisulfite `--dovetail` carry the same artefact? Would soft-clipping overhangs recover the pairs safely? (4.3)
17. **O7.** Update the CLI help for `-X` and `--no_dovetail` to mention 5-Base behaviour, and note in the docs that `-I` > 800 now needs an explicit `-X`.

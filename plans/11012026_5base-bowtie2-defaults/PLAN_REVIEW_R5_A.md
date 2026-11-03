# Plan review R5: Reviewer A

Target: `PLAN.md` § "Rev 5 (2026-11-02)", read against rev 4, `PLAN_REVIEW_R4_A.md`, `PLAN_REVIEW_R4_B.md`, the implementation notes (#5), `PREREGISTRATION.md` rev 1, the branch at `f3e860f` (builder `72bbf7b`), the harness (`validation/five_base_engine_bench.sh`, `five_base_rescued.py`), the #1146 committed results, and the docs. I did not re-argue the settled decisions (drop `--dovetail`, `--maxins 800` only, explicit `--dovetail` is a no-op).

## Verdict

Rev 5 resolves nearly every Critical and Important item from both rev 4 reviews, and the `-I` check is a good addition. I confirmed it locally with Bowtie 2 2.5.5: `-I 900 -X 800` exits 0 and returns `YT:Z:DP`.

One new problem blocks the HG002 run. The R5/A5 rest pool in `five_base_rescued.py` is sized for a rescued set of at most 3%. At the roughly 7% admitted fraction rev 5 expects, `rescued` exits before it writes `rescued.json`. Four smaller harness and test gaps also need closing before the pre-registration is frozen.

## 1. Did rev 5 resolve the rev 4 items?

| Rev 4 item | Rev 5 | Status |
|---|---|---|
| A-C1 / B-C1: `repro` hard-wired to B-old | `repro <sample> <tag> <arm> <ref-dir>` | Resolved, but its output files are not arm-keyed (see I2) |
| A-C2: shared `$ROOT/bin` while rev 3 cells run | `BIN` overridable, `$ROOT/bin5` | Resolved for the binary. Not yet for the harness files or for a re-run of `cell B-old` (I3) |
| B-C1: `verdict`, B-500 checks, B-new self-test | `--old-arm/--new-arm`, criteria set `admitted`, arm table, self-test list | Resolved. B (`-X 500`) replaces B-500, as R4-A O2 proposed |
| B-C2 / A-I3: what the gate tests, and the failure branch | Validation 1 "What it tests", the expectations, "On a failure, stop" | Resolved |
| A-I1 / B-I1: no per-pair check of the admitted pairs | A1–A6 on HG002 | Resolved in design; A5 cannot run as the helper is written (C1) |
| A-I2 / B-O2: `--five_base_umi_qname` | dropped everywhere | Resolved |
| A-I4 / B-I2: positive control, pin Q5 | control pair; runs with no flag, `--dovetail` and `--no_dovetail` | Resolved (see O3 for limits) |
| A-I5 / B-I5: forbidden list, counts, e2e string | listed | Resolved; one more test to update (O4) |
| A-I6 / B-I4: docs | line-specific edits in both pages | Resolved; small gaps (O7) |
| A-I7 / B-I6: PREREGISTRATION rev 2 disclosure | listed | Resolved |
| A-I8 / B-I5: fate of the rescued and duplex tooling | re-aimed; duplex and D0–D3 removed | Resolved |
| B-I3: B-loc silently at 800 | `-X 500`, `--maxins 500` once | Resolved |
| B-O1: `-I` above the cap | now an error | Resolved. Tests miss the explicit `-X` branch (I4) |

## 2. Logic review

### 2.1 The A5 rest pool is too small for the admitted set (new, Critical)

- `cmd_rescued` draws R5 pool pairs only from rest pairs with `rng.random() < POOL_FRAC`, which is 0.05 (`five_base_rescued.py:41`, `:333-336`).
- It then aborts if the rescued count k is larger than the pool: `if k > len(pool): raise SystemExit(...)` (`:344-347`).
- #1146's numbers predict the HG002 sizes:
  - NA12878 B has 86,206,533 unique pairs and B-X has 92,666,181.
  - The admitted set is therefore at least 6.46M pairs, about 7% of B-new.
  - Rest is at most about 86M pairs, so the 5% pool is about 4.3M pairs.
- k (about 6.5M) is larger than the pool (about 4.3M), so `rescued` exits and A5 never runs. A1's own upper bound of 15% would need a pool of about 18% of rest.
- The 0.05 fraction was frozen in rev 1 for a rescued band of 0.1–3%. Rev 5 widens the band but keeps the fraction.
- **Fix (pick one, then pre-register it in rev 2):**
  - (a) **Recommended:** subsample the admitted set as well. Give every admitted pair a draw from the same seeded RNG, use admitted pairs with draw < f_a for A5, and size the rest sample to match.
    - f_a = 0.05 still gives about 300k pairs and well over 10,000 calls.
    - This also keeps the `rescued_calls` and `pool_calls` arrays small.
  - (b) Raise `POOL_FRAC` to ≥ 0.2. This is simpler, but the pool list becomes about 18M tuples plus their calls, roughly 2–3 GB of Python heap. That is fine on oxy, but slow.
  - Either way, the helper self-test needs a case where k is larger than 5% of rest, so this cannot regress.

### 2.2 `repro` outputs are not arm-keyed (Important)

- `cmd_repro` writes `$RESULTS/$s/$tag/repro.tsv` and `$ROOT/runs/$s/$tag/repro_score.tsv`, with fixed names (`bench:383`, `:395`).
- It labels the arm `B-old` inside the score call and in the `awk` key (`:397-399`).
- Rev 5 runs `repro` twice on NA12878 screen: B-old as the cross-check, then B-new as the gate. As written, the second run overwrites the first.
  - The expectation "B-old gives the same verdict as B-new's" can then no longer be audited from the committed results.
  - The rev 3 stream may already have written `repro.tsv` for B-old.
- **Fix:** name the files `repro_<arm>.tsv` and `repro_score_<arm>.tsv`, and use the arm as the score label, the `awk` key and the header column.
- With both files kept, the "B-new and B-old match on every non-timing metric" expectation becomes a mechanical diff of two files.
  - It should cover the `repro` column list. A diff of the whole `metrics.tsv` is wrong, because `cpus`, `load_*` and `key` legitimately differ between streams.
  - State that this is how the check is done.

### 2.3 Harness deployment while rev 3 cells still run (Important)

- `bin5` protects the binary, but not the harness. The rev 3 cells run `~/5base_harness_bt2/validation/five_base_engine_bench.sh`.
- Bash reads a script incrementally while it runs. Overwriting that file in place (`cp`, or an editor save that keeps the inode) can make the running cell execute garbage at its next read.
- The running cell also invokes `$HERE/concordance.py` after alignment. That file is unchanged by rev 5, so it is safe, but it is the same class of hazard.
- Rev 5 says to wait for the rev 3 cells (step 4.2) only before the new *cells*. It does not say to wait before deploying the *harness*.
- **Fix:** deploy the rev 5 harness to a new directory (for example `~/5base_harness_bt2_r5`), or deploy only after the rev 3 cells finish.
- Also restate R4-A C2's second half: never run `cell <s> screen B-old` with `BIN=$ROOT/bin5`. The DONE key includes the commit, so the cell would `rm -rf` the cross-check BAMs (`bench:221-223`).
- An optional guard makes forgetting `BIN` fail at start-up instead of after 2.5 h: `EXPECT_COMMIT` is checked against `bismark --version` at cell start.

### 2.4 `-I` check: the explicit-`-X` branch is untested (Important, small)

- Behavior: "greater than the maximum insert in effect, which is `-X` or the 800 default". That covers two code paths.
- The listed tests (`-I 900` errors; `-I 900 -X 1000` and `-I 800` pass) exercise only the default branch, plus one explicit-`-X` pass.
- An implementation that checks only `None => 800` would pass every listed test.
- **Add:**
  - `-I 900 -X 800` errors, with `(800)` in the message;
  - `-I 801` errors (the boundary);
  - optionally `-I 500 -X 500`, which passes.
- The CLI case fits the existing `five_base_1134_invalid_tuning_fails_before_output_creation`: Bowtie 2 only, no `output/` or `argv`.
- The builder runs after `detect_aligner` (`config.rs:1026-1031`), so the fake engine must be present. That fixture already provides it.

### 2.5 Helper self-test coverage for the new modes (Important)

- Rev 5's "Self-test cases" lists only the bench self-test (arms, `--maxins`, `@PG`).
- `five_base_rescued.py --selftest` must also cover:
  - `--all-bases` tallies;
  - the |TLEN| quantiles;
  - A1–A6 PASS/FAIL/INSUFFICIENT at their boundaries;
  - the arm-name parametrisation in `criteria()`, which today hard-codes `("B-old","common","10")` (`:592`).
- A wrong threshold constant, such as A1 left at `0.1-3`, would otherwise go unnoticed until the verdict.

### 2.6 A3/A6 compare different fragment-length strata (Optional)

- The admitted set is selected by length (about 500–800 bp). The rest set is mostly 250–480 bp.
- CpG level (A3) and R1 3′ CpG (A6) can differ by fragment length for reasons that are not defects:
  - regional composition;
  - for short inserts, the R1 3′ end sits near the fragment end and end-repair fill-in.
- A FAIL from that would be a false alarm. Rev 5 sends any FAIL to Felix, so it fails safe.
- A cheap aid: also report the same metrics for rest pairs with |TLEN| 450–500 as a length-near control. Report it only, so the criteria stay as pre-registered.

### 2.7 A4 `--all-bases` cost (Optional)

- The admitted set is about 6.5M pairs × about 290 aligned bases, so about 1.9 × 10⁹ iterations of the per-base Python loop in `CycleTally.add`: two method calls and a dict increment per base. That is plausibly 1–2 h on its own.
- n will be in the hundreds of millions, so subsample the admitted pairs for A4 with the same seeded draw as in 2.1(a). Pre-register the fraction.
- In all-bases mode, cycle matching is close to a no-op, because both sides cover every cycle. That is fine, but say so, so nobody reads it as an overhang-style check.

### 2.8 Equal-start, not equal-extent (Optional, wording)

- The TLEN branch (`output.rs:574-577`) fires for `flag_1 == 83 && start1 <= start2 && end2 >= end1`.
- Without `--dovetail`, Bowtie 2 reports no `start1 < start2` reverse-R1 pairs, so the reachable case is **equal start**. The ends need not be equal.
- Rev 5 says "equal-extent" twice. R4-A says "equal-start". Use "equal-start" in the plan, and in any docs line about `--no_dovetail`.

## 3. Assumptions

| Assumption | Status |
|---|---|
| B-new argv equals #1146 B-X | ✔ Re-checked: `B-X.rep1` report has `-q --score-min L,0,-0.6 --maxins 800 -p 32 --reorder`; B-new now also drops `RX` |
| B-old matches B-new on every committed metric | ✔ The diff from `72bbf7b` to the rev 5 builder only removes `--dovetail` (which B-old never had) and adds the `-I` check. `config.dovetail` changes only the TLEN sign. Nothing in `dedup/`, `extractor/` or `io/` reads TLEN (grep) |
| `-X 500` reproduces the old default | ✔ Bowtie 2's default is 500 |
| A1 range 1–15% | Reasonable: #1146 predicts about 7% on NA12878. HG002's insert distribution is unknown |
| The A5 pool can supply k rest pairs | **✘ False at about 7% (2.1)** |
| `-I > X` is silent in Bowtie 2 | ✔ Verified locally on 2.5.5 (exit 0, `YT:Z:DP`) |
| HISAT2 unchanged | ✔ By scope. The same silent `-I` > 500 failure exists there, so a docs line is worth it (O6) |
| Same genome and index bytes as #1146 | Implicit. R4-A O3 (md5s in `versions`) was not taken up. It would help tell a reference re-stage apart from code drift if the gate fails |

## 4. Efficiency

- Compute is unchanged from what the reviews accepted: NA12878 B-new, HG002 B and HG002 B-new, about 7.5 h in two streams.
- The `rescued` step grows with `--all-bases` and a widened pool. Fix 2.1(a) and 2.7 together, with one seeded admitted-pair draw, and the step stays at about rev 3's cost.
- `cmd_versions` writes the fixed path `$RESULTS/versions.txt`. Re-running it for `bin5` overwrites rev 3's record. Make the path BIN-keyed, or copy rev 3's file aside first (O8).

## 5. Alternatives

- **Admitted-set subsample instead of a larger pool (2.1).** Smaller memory and runtime, the same statistical power, and one draw serves A4 and A5.
- **A free B-X ≡ B-new check (R4-A O2's second half, not taken up).** Not needed now that the gate compares with #1146 directly. Skip it.
- **Extend the `-I` check to HISAT2 with its 500 default.** It is a one-line generalisation and removes the same silent failure there. Rev 5 scopes HISAT2 out explicitly, so this is Felix's call; a docs line is the minimum.

## 6. Action items

### Critical

1. **C1.** Fix the A5 rest-pool sizing in `five_base_rescued.py`. At the expected about 7% admitted fraction, `k > len(pool)` aborts `rescued` (`POOL_FRAC = 0.05`, `:41`, `:344`).
   - Preferred: subsample the admitted pairs with a seeded draw for A5, and for A4 (2.7).
   - Pre-register the fractions in rev 2, and add a self-test case with k above 5% of rest.

### Important

2. **I1.** Specify helper self-test coverage for `--all-bases`, the |TLEN| quantiles, the arm-parametrised `criteria()`, and A1–A6 PASS/FAIL/INSUFFICIENT boundaries (2.5).
3. **I2.** Arm-key `repro`'s outputs and labels: `repro_<arm>.tsv`, `repro_score_<arm>.tsv`, and the arm as score label and `awk` key. State that the B-old vs B-new expectation is checked by diffing the two `repro` files over the `repro` column list (2.2).
4. **I3.** Deploy the rev 5 harness to a separate directory, or only after the rev 3 cells finish. Restate "never run `cell … B-old` under `bin5`" (DONE-key `rm -rf`). Optionally add an `EXPECT_COMMIT` guard (2.3).
5. **I4.** Add `-I 900 -X 800` (errors, message names 800) and `-I 801` (errors) to the unit tests, and add the `-I 900` CLI case to `five_base_1134_invalid_tuning_fails_before_output_creation` (2.4).

### Optional

6. **O1.** Report A3/A6 also against a length-near rest control (|TLEN| 450–500), as reported only (2.6).
7. **O2.** Subsample for A4 `--all-bases`, and note that cycle matching is close to a no-op there (2.7).
8. **O3.** For the three-pair real-Bowtie 2 test:
   - the `align()` helper takes one pair named `pair`, so it must take several named pairs, and the "both from the control" assertion must use the QNAME;
   - put the control in a region that does not overlap 2000–2120 or 4000–4120;
   - assert XM length 100 on the control.

   The control proves that the run aligns, not that the OT/OB fixtures still do. R4-B's option (c), one direct `bowtie2 --dovetail` call on each fixture showing `YT:Z:CP`, would make that per fixture.
9. **O4.** Also update `five_base_local_scoring_is_engine_specific` (`options.rs:679-687`, two `--dovetail` strings).
   - Rename `five_base_bowtie2_defaults_to_dovetail_and_maxins_800` and `five_base_bowtie2_dovetail_and_maxins_defaults_reach_argv`, which now describe the opposite of what they assert.
   - Keep `!report.contains("--dovetail")` in the e2e test.
10. **O5.** Use "equal-start" instead of "equal-extent" for the `--no_dovetail` TLEN effect (2.8).
11. **O6.** In the docs, add one line that `-I` above the cap is now an error on 5-Base Bowtie 2. Optionally note that HISAT2 keeps a 500 bp cap, where `-I` > 500 needs `-X`.
12. **O7.** Docs: R4-B I4's `--no_dovetail` TLEN caveat and "scope to the libraries tested" were not carried into rev 5. Once HG002 runs, the "one 5-Base WGS gDNA library" sentence (`illumina-5-base.md:115-116`) can say two.
13. **O8.** Make `versions` BIN-keyed, or preserve rev 3's `versions.txt`. Optionally take up R4-A O3 (index and genome md5s) so a failed gate can be diagnosed.
14. **O9.** In PREREGISTRATION rev 2, say explicitly that rev 1's "changed after a B-new cell produced data ⇒ void" clause applies to the withdrawn rev 1 criteria and the dry run only, so the rev 2 edit is not read as voiding anything.

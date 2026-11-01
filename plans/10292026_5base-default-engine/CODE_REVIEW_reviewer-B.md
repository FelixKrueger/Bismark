# Code review — `feat/5base-default-engine` (Reviewer B)

Target: `origin/dev..HEAD` (`425d42b`, `69b6b90`) plus the uncommitted changes to
`docs/src/content/docs/rust/illumina-5-base.md`, `CHANGELOG.md`,
`rust/bismark/src/aligner/mod.rs` and `rust/bismark/tests/aligner_cli.rs`.
Spec: `PLAN.md` (Behavior 8 / outcome (a)), `PREREGISTRATION.md`, `RESULTS.md`.

## Summary

**Verdict: APPROVE with minor fixes.** None of the issues affects the decision. Outcome (a)
follows correctly from the pre-registered rule and the committed `score.tsv` and `metrics.tsv`.
I recomputed every delta and guard and they match `RESULTS.md`. The new `five_base_notice` is
correct and its tests pass. The scorer's Pearson accumulators match a reference
implementation. The problems are in the prose and in a few guards:

- **Docs:** one causal claim is not supported by the data (minimap2's memory "because it
  builds its index"). "Short for typical 5-Base fragments" overgeneralises. The RSS figures
  are in an inconsistent unit, and "100M trimmed read pairs" is wrong (100M pairs before
  trimming; 98.08M after).
- **`RESULTS.md`:** two claims about minimap2 and B-loc are not true of the data. One
  validation row overstates what the harness checks.
- **Harness:** `score` does not check that each cell finished, and the `setsid`/`wait` pairing
  could fail silently if the shell ever forks `setsid`.

Checks run:
- `cargo test -p bismark --lib aligner::tests::five_base_notice`: 1 passed.
- `cargo test -p bismark --test aligner_cli five_base_1134_default_argv`: 1 passed.
- `python3 -I validation/five_base_compare.py --selftest`: OK.
- `bash -n validation/five_base_engine_bench.sh`: OK.
- Independent check of `Acc`, comparing plain and weighted Pearson with
  `statistics.correlation` and a two-pass weighted formula on 5,000 random sites: they agree
  to 6 decimals.

## Issues by area

### 1. `five_base_notice` and tests

- `rust/bismark/src/aligner/mod.rs:164-173`: correct. `Aligner::name()` gives
  "minimap2" / "Bowtie 2" / "HISAT2" (`config.rs:58-66`). `--illumina_5base --rammap` is
  rejected in `config.rs:1416-1420`, so the `rammap` name cannot reach this notice.
  "Directional, paired-end only" matches the guards in `config.rs:928-950`.
- No stale copy of the old wording ("aligned with minimap2", "SE + PE") remains in `src/`,
  `tests/`, `docs/src` or `validation/`.
- `mod.rs:8456-8473`: the unit test covers all three engines. `aligner_cli.rs:7551-7560` adds a
  stderr assertion for bowtie2 and hisat2. There is no minimap2 CLI assertion, but the unit
  test covers that engine. Fine.
- Comment at `mod.rs:252`, `// Never-silent opt-in notice (#787): 5-Base has no Perl oracle, so
  it is not byte-identical.`: one line and factual. It largely restates the notice text, so it
  could be dropped. Optional.

### 2. Benchmark harness

**`validation/five_base_compare.py`**
- Key packing (`:34`): `idx<<33 | pos<<1 | strand` is collision-free for GRCh38
  (pos < 2^32).
- Value packing (`meth<<32 | cov`) is fine.
- Own, common and gain sets (`:118-148`) match the pre-registration exactly:
  - **own:** the arm and DRAGEN are both at ≥ t;
  - **common:** DRAGEN ≥ 10 and every arm ≥ 10;
  - **gain:** X ≥ 10, DRAGEN ≥ 10 and baseline < 10.
- Signed Δ is arm − DRAGEN, as pre-registered.
- Pearson and weighted Pearson (`:62-99`): verified against a reference implementation (see
  above). Single-pass cancellation is not a concern at n ≈ 55M with values in [0, 100].
- Strand convention: keys include the strand. An off-by-one between the arm and DRAGEN would
  collapse the joined fraction, which `MIN_JOINED` would catch. `refcheck` confirms DRAGEN's
  '-' rows sit on the CpG G, which is Bismark's convention.
- **Low:** `load_cpg` (`:53`) overwrites a duplicate (chrom, pos, strand) without a warning. A
  duplicated `--arm` name (`:105-110`) overwrites `data[...]` but adds duplicate rows. An empty
  common or gain set gives `n=0, NA` rows and exits 0. Each of these is unlikely, but none fails
  loudly.
- **Low:** the selftest asserts set sizes and the mean signed Δ but no r or weighted-r value.
  The accumulators are the core of the decision. A Pearson assertion against a hand-computed
  value would lock them in.

**`validation/five_base_engine_bench.sh`**
- **Medium, `cmd_score` (`:222-227`):** picks
  `$ROOT/runs/$s/$tag/$arm.rep1/*CpG_report.txt* | head -1` without checking `$arm.rep1/DONE`.
  The CpG report is written before read-fate, deconvolution and metrics, so a cell that dies
  after extraction would still be scored. Add `[[ -f $d/DONE ]] || die`.
- **Medium, `cmd_cell` (`:138-146`):** `setsid taskset ... &` followed by
  `pid=$!; wait "$pid"` is only correct because `setsid` does not fork when its caller is not
  a process-group leader. This holds in a non-interactive bash script. Under job control (for
  example `bash -m`, or a wrapper that makes the background job a group leader), `setsid`
  forks, its parent exits at once, and the consequences are silent:
  - `wait` returns 0 while bismark is still running;
  - `sample_tree` records nothing;
  - the script proceeds to `ls *_pe.bam` on a partial run.

  In this run it worked, because `tree.tsv` has sane values. Use `setsid -w` (util-linux) to
  make it robust, and/or die if `tree.tsv`'s peak is 0.
- **Low, `verify_options` (`:52-72`):**
  - arm A checks only `-t $THREADS`, not `-x sr`;
  - B-loc does not check `--reorder` or deny `--maxins`.

  `RESULTS.md` says options were verified via "report + `@PG`", but the harness reads only
  the report line (see §3).
- **Low, `cmd_prep` (`:110`):** `cx=$(ls .../*.CX_report.txt.gz | grep -v '\.control\.')`
  would silently concatenate two CX reports if more than one non-control file were present.
  Assert exactly one.
- **Low:** the two `( zcat | head ) &` subshells are reaped by a bare `wait`, which always
  returns 0. The line-count check at `:93-96` covers truncation, so this is fine as is.
- Comments are one line each and factual. No narrative.

### 3. `RESULTS.md` vs the pre-registered rule

I recomputed everything from `score.tsv` (identical to `bench_results/NA12878_screen_score.tsv`)
and the per-arm `metrics.tsv`.

| Arm | Δr (common) | ΔCpG (own, cov ≥ 10) | MAPQ-0 % | Gain r vs common r − 0.02 | Rule 1 |
|---|---|---|---|---|---|
| B | 0.963133 − 0.964638 = −0.001505 | 554,655 / 817,312 − 1 = −32.14% | 0.959 | 0.9198 < 0.9431 ✗ | ✗ |
| B-loc | −0.001094 | −28.96% | 0.155 | 0.8777 < 0.9435 ✗ | ✗ (just) |
| B-X | −0.000296 | +0.63% | 0.916 | 0.9374 < 0.9443 ✗ | ✓ |
| C | −0.001516 | −18.66% | 0.194 | 0.9330 < 0.9431 ✗ | ✗ |

Arm A's MAPQ-0 is 6,901,510 / 186,834,412 = 3.694%. The CHG/CHH guards pass for every arm.
No arm is non-inferior on r **and** passes the guards, so there is no challenger, and
outcome (a) follows correctly from "If no arm qualifies…". The B-X rule-3 remark is also
correct.

The other figures in `RESULTS.md` also check out:
- mapping efficiencies;
- read-through percentages (801,819 / 93,417,206 = 0.86%, and so on);
- the cov ≥ 5 caveat (B-X +1.53%, B −12.1%, C −9.8%);
- joined fractions (0.962–0.976);
- "cov ≥ 10 ≈ 1.5% of CpGs" (817,312 / 55.57M = 1.47%).

Inaccurate claims:
- **Medium, "What the screen shows", B-loc bullet:** "has the lowest accuracy and
  deconvolution precision". On the **primary** metric, common-set r, B-loc (0.963544) is
  *above* B (0.963133) and C (0.963122). It is lowest only on own-set cov ≥ 10 r (0.9487) and
  gain-set r. The precision claim (88.1%) is correct. Say "the lowest own-set and gain-set r".
- **Medium, minimap2 bullet:** "the highest deconvolution recall". B-X has 61.6% against A's
  61.3%, so this is false. A does have the best common-set r. Reword, for example "best
  common-set r, and deconvolution recall within 0.3 pt of the best".
- **Low, validation table:** "Options actually applied (report + `@PG`)". The harness checks
  only the report's options line (`five_base_engine_bench.sh:52`). Either drop "+ `@PG`" or
  say it was checked by hand.
- **Low, Follow-ups:** "Startup notice fix … `mod.rs:223` says minimap2" is now done, and the
  line number is stale. Mark it done, or drop the line reference.
- **Low, read-fate table:** read-through has a percentage for A, B-loc and C but not for B
  and B-X (2,937 and 2,929 ≈ 0.003%). This is cosmetic.
- **Observation (not a defect):** B and B-X show about 2.9k read-through R1s against 0.80M
  for A. This fits the 5-Base Bowtie 2 argv carrying no `--dovetail`
  (`-q --score-min L,0,-0.6 -p N --reorder`), so dovetailed pairs that minimap2 keeps cannot
  be concordant under Bowtie 2. That probably explains much of the 0.7 pt mapping gap left
  between B-X and A. It is worth one line next to the `-X` follow-up, because a future
  Bowtie 2 default change might want both.

### 4. Docs: "Choosing an engine" (`illumina-5-base.md:97-115`) and surrounding edits

These numbers match the data:
- mapping efficiency;
- common-set r rounded to 4 dp (0.9646, 0.9631, 0.9643, 0.9635, 0.9631);
- own-set CpGs at cov ≥ 10;
- n = 400,760.

Issues:
- **Medium, `:117-118`:** "minimap2 needs about twice the memory because it builds its index
  from the FASTA on every run". The benchmark measured that minimap2 uses about 2× the RSS. It
  did not test why: no arm used a prebuilt `.mmi`. A loaded `-x sr` human index plus 32
  threads' buffers is large anyway, so a prebuilt index may not change peak RSS. State the
  fact without the cause, for example "minimap2 needs about twice the memory". The same causal
  claim appears in `RESULTS.md` ("because it builds its index from the FASTA in every run").
- **Medium, `:75-76`:** "the engine's default maximum insert is 500 bp, which is short for
  typical 5-Base fragments". On this library the TLEN median is about 350 and p90 is about
  478, so *typical* fragments fit under 500 bp. The cap truncates the upper tail, roughly 7%
  of pairs here, and this is one library. Suggest "which cuts into the fragment-length tail
  of 5-Base WGS libraries".
- **Low/Medium, `:99`:** "On 100M trimmed read pairs". The input was the first 100M pairs
  **before** trimming; 98,082,139 remain after Trim Galore. Say "On the first 100M read pairs
  (98.1M after trimming)".
- **Low/Medium, `:99-100` with the table:** the lead-in says the scoring is "on the 400,760
  CpGs every engine covered at cov ≥ 10". The "CpGs at cov ≥ 10" column, however, is each
  engine's own count (817,312 and so on, larger than 400,760). A reader can take both columns
  to refer to the 400,760 set. Suggest moving the set to the column header, for example
  "Pearson r vs DRAGEN (400,760 shared CpGs, cov ≥ 10)" and "CpGs at cov ≥ 10 with DRAGEN".
  The common set also requires DRAGEN cov ≥ 10, which the lead-in omits.
- **Low, Peak memory column:** `tree_peak_rss_kb` comes from `ps -o rss`, which reports KiB.
  For A, 16,295,504 KiB is 15.5 GiB or 16.7 GB, and the docs' "16.3 GB" is neither; it
  treats KiB as 1000 bytes. The ratios are unaffected. Either use GiB, or convert to GB:

  | Arm | GiB | GB |
  |---|---|---|
  | A | 15.5 | 16.7 |
  | B | 8.4 | 9.0 |
  | B-loc | 8.4 | 9.0 |
  | B-X | 8.4 | 9.0 |
  | C | 8.3 | 8.9 |

  The same unit issue is in `RESULTS.md`.
- **Low, consistency:** the stability contract quotes r ≈ 0.998 at cov ≥ 10 at full depth;
  the new table shows 0.9646 at cov ≥ 10 for the same engine. "On 100M … read pairs" explains
  this implicitly. A half-clause ("lower than the full-depth figure above because of the
  screen depth") would stop readers from reading it as a contradiction.
- The remaining claims are accurate:
  - "No alternative was clearly better" (`:48-49`) matches the rule.
  - "`-X 800` matches minimap2's own limit and brings Bowtie 2 level with minimap2" is fair:
    Δr −0.0003, CpGs +0.6%, mapping −0.7 pt.
  - "cost Bowtie 2 about 7 points … and a third of its well-covered CpGs" is right: B against
    B-X is −6.6 pt and −32.6%. It is also correctly limited to Bowtie 2, since HISAT2 with
    `-X 800` was not tested.
  - "short-fragment libraries such as cfDNA were not tested" is correctly scoped.
- `:23`, "with the default minimap2 engine": consistent with the plan's implementation notes.

### 5. CHANGELOG (`CHANGELOG.md:18`)

The entry is one sentence, bold, and matches the neighbouring entries in this section, several
of which also carry no link. Once the PR exists, append its link, `([#NNNN](…))`, as the other
entries do. Low.

### 6. Comment style

- Rust: the `five_base_notice` doc line and the call-site comment are one line each, factual,
  and current-state only.
- Harness: all comments are one line.
- No historical narrative was found in committed source.

## Fixes applied

None (report-only review).

## Prioritised recommendations

**Critical:** none.

**High:** none.

**Medium**
1. Docs `illumina-5-base.md:117-118` and `RESULTS.md`: drop the unsupported cause ("because it
   builds its index from the FASTA on every run").
2. Docs `illumina-5-base.md:75-76`: replace "short for typical 5-Base fragments" with wording
   that reflects tail truncation (median about 350, p90 about 478).
3. `RESULTS.md`, "What the screen shows":
   - fix "highest deconvolution recall" for minimap2 (B-X is higher);
   - fix "lowest accuracy" for B-loc (not true on common-set r).
4. `five_base_engine_bench.sh` `cmd_score`: require each arm's `DONE` before scoring.
5. `five_base_engine_bench.sh` `cmd_cell`: use `setsid -w`, or guard on a zero `tree.tsv`
   peak, so a forking `setsid` cannot make `wait` return early in silence.

**Low**
6. Docs: "100M trimmed read pairs" should say 100M before trimming (98.1M after).
7. Docs: put the 400,760 shared-set label on the r column, not in the lead-in. Note that DRAGEN
   cov ≥ 10 is also required.
8. Docs and `RESULTS.md`: correct the RSS units (KiB was read as kB). Use GiB (15.5 / 8.4 /
   8.4 / 8.4 / 8.3) or GB (16.7 / 9.0 / 9.0 / 9.0 / 8.9).
9. Docs: add a half-clause explaining why the screen r (0.9646) is below the published
   full-depth r (0.998).
10. `RESULTS.md`:
    - drop "+ `@PG`" from the options-verified row;
    - mark the notice follow-up as done, without the stale `mod.rs:223`;
    - give B and B-X read-through percentages;
    - optionally note the missing `--dovetail` as a second Bowtie 2 handicap.
11. `five_base_compare.py`:
    - fail on a duplicate site or a duplicate arm name;
    - fail on an empty common set;
    - add an r assertion to the selftest.
12. `five_base_engine_bench.sh`:
    - assert exactly one non-control CX report;
    - check `-x sr` for arm A;
    - check `--reorder` for B-loc.
13. `CHANGELOG.md`: add the PR link when opened.
14. Optional: drop the call-site comment at `mod.rs:252`, which restates the notice.

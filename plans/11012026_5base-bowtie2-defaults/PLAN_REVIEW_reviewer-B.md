# Plan review (Reviewer B): 5-Base Bowtie 2 defaults `-X 800` + `--dovetail`

- **Plan:** `plans/11012026_5base-bowtie2-defaults/PLAN.md` rev 1
- **Code checked at:** `d1764d0` (worktree `/Users/fkrueger/Github/Bismark-5base-bt2`)
- **Evidence read:** `plans/10292026_5base-default-engine/RESULTS.md`, `bench_results/NA12878/screen/*`, `bench_results/*/screen/input/*`, `validation/five_base_engine_bench.sh`, `validation/five_base_compare.py`
- **Verdict:** sound in direction, and the code change is small and correctly scoped. The confirmation run is the weak part. Its aggregate no-harm guards are too insensitive to see an artefact confined to the ~0.8% of newly admitted pairs. One guard (gain-set r) is likely to fail by construction. Two of the plan's factual claims are wrong: the token order, and that `@PG` carries the defaults. None of this blocks the approach. All of it should be fixed before `PREREGISTRATION.md` is committed, because the criteria cannot change after that.

---

## 1. Logic

### 1.1 Builder change: correct scope, but the token-order claim is wrong

`build_five_base_aligner_options` (options.rs:261) only runs for `--illumina_5base` with Bowtie 2 or HISAT2 (config.rs:1026-1031). 5-Base is paired-end only: SE is rejected at config.rs:927 and :940. So an unconditional Bowtie 2 `--maxins 800` / `--dovetail` can never reach a single-end run. Good.

Behavior 3 says the order `--minins`, `--maxins`, `--dovetail` "matches the bisulfite builder's relative order". **It does not.** The bisulfite builder pushes step 12 (`--no-mixed --no-discordant --dovetail`) **before** steps 13/14 (`--minins`, `--maxins`), at options.rs:54-87. The pinned test `paired_end_tail_and_default_maxins` (options.rs:751) shows `... --dovetail --maxins 500`. There is no functional consequence, because Bowtie 2 doesn't care. But the plan states a rationale that is false. Either:
- emit `--dovetail` before `--minins`/`--maxins`, giving `-q --score-min L,0,-0.6 --dovetail --maxins 800 -p N --reorder`, which really matches bisulfite; or
- keep the proposed order and drop the "matches bisulfite" justification.

I'd take the first. Whichever is chosen, it fixes the exact strings in all the tests below, so decide it now.

### 1.2 Dovetailed pairs downstream of the 5-Base driver

I traced each consumer of a dovetailed pair.

| Consumer | Finding |
|---|---|
| **Proper-pair gate** (`five_base_emit_pe_record`, mod.rs:2471) | Keys on Bowtie 2's `0x2`. With `--dovetail`, Bowtie 2 sets `0x2` on dovetails, so they pass. Correct. |
| **Output FLAGs** | Bismark rewrites the FLAG from the index constant (99/147, 83/163) in `paired_end_sam_output` (output.rs:529). Downstream FLAG semantics are unchanged. |
| **TLEN** (output.rs:570-597) | The dovetail sub-cases give the outer span with the conventional sign: R1-forward positive, R1-reverse negative. They are gated on `config.dovetail`, which is already `true` by default on this path (config.rs:1112). The *new* behaviour is only that Bowtie 2 now emits such pairs. Correct. |
| **Extractor `--no_overlap`** (overlap.rs `drop_overlap`) | For a dovetailed OT pair (R2 starts upstream of R1), every R2 call is dropped (`keep r2_pos > r1_end`), including R2's overhang upstream of R1's start. OB is the mirror. There is no double-counting. `pair_class.rs` has `forward_dovetail_overlap_is_concordant`, so dovetails take this path. Correct. The R2 overhang is discarded, which is the right fate for adapter-remnant bases. |
| **R1's overhang** | **R1's overhang (its 3' end past R2's 5' start, i.e. past the fragment end) is kept by the extractor and by deconvolution.** See 1.3. |
| **Deconvolution** (mod.rs:2565ff) | Keeps only OT/OB-tagged records. R2 carries CTOT/CTOB (XR=GA for an OT pair, `from_xr_xg`), so only R1 is piled up. That avoids double-counting, but it does include R1's overhang. |
| **Duplex PE** (mod.rs:2685ff) | Uses `frag_start = min(pos, mate)` and `frag_end = frag_start + \|TLEN\|`. With the dovetail TLEN rule, \|TLEN\| is the outer span, so the key is self-consistent. However, the two strand-partner pairs of one molecule get independent adapter remnants. If one partner dovetails by k bp and the other doesn't, their outer spans differ, so they **fall into different families** and the duplex yield drops. Pure containment doesn't have this problem. The plan doesn't mention duplex/consensus at all, and the benchmark doesn't run it. |
| **UMI dedup** (driver, mod.rs:2347) | Keys on R1 `pos` + strand. For an OB dovetail, R1's leftmost POS includes the overhang, so the same molecule with and without a remnant gets different keys and under-deduplicates. Minor. |
| **Genomic +2 extraction** | Per mate, so dovetails are fine. The only exception is a dovetail at a chromosome start, which is counted in `genomic_sequence_could_not_be_extracted` rather than failing. Fine. |

### 1.3 The plausible harm is local and the plan does not look for it

Look at the screen library: Nextera adapter `CTGTCTCTTATA`, Trim Galore stringency 1, 11.9% of R1 adapter-trimmed. Under end-to-end Bowtie 2, a dovetail almost always means each mate kept a few bases past the other's 5' end. Those bases are an untrimmed adapter remnant (a sequencing error defeats the 1-bp match) or poly-G/quality-trim asymmetry. End-to-end alignment forces them onto the genome as mismatches beyond the fragment end.

In 5-Base the polarity is inverted: a read T at a reference C is called *methylated*. Random adapter bases at genomic C positions therefore produce spurious calls. R2's overhang is removed by `--no_overlap`, but R1's survives into the CpG report and into deconvolution (1.2). minimap2 soft-clips these ends, so its 0.80M read-through pairs are not evidence that end-to-end Bowtie 2 dovetails are clean.

Bisulfite Bismark accepts the same risk with its own `--dovetail` default, so this is not a reason to reject the change. It is a reason to **measure the new pairs directly**, not only through genome-wide aggregates (see 4.1).

### 1.4 B-old is not "exactly the old behaviour"

The plan says B-old (`-X 800 --no_dovetail`) "gives exactly #1146's B-X options, so it reproduces the old behaviour". The *aligner string* is identical. But #1146 B-X ran with `config.dovetail = true` (no `--no_dovetail`), and B-old runs with `config.dovetail = false`. That flips the TLEN sign on concordant pairs that hit the dovetail sub-cases even without Bowtie 2's `--dovetail`. The commonest case is a **coincident OB pair** (`start1 == start2`, R2 end ≥ R1 end, flag_1 = 83), which is typical of trimmed short fragments:
- with dovetail on, R1 gets −L;
- with dovetail off, R1 gets +L.

I checked that dedup and the extractor never read TLEN (`grep template_length` in `dedup/` and `extractor/` is empty). The read-through metric and TLEN quantiles use |TLEN|. So every pre-registered metric should still reproduce, but the **BAM is not byte-equivalent** to old output, and duplex keys use |TLEN| so they are unaffected.

Two consequences:
- **Wording.** The plan should say "metric-equivalent", not "exactly the old behaviour".
- **User docs.** The escape hatch for existing users (`-X 500 --no_dovetail`) does **not** reproduce pre-change BAMs exactly: TLEN signs differ on coincident OB pairs. The docs must not promise byte-identical old output. There is no flag combination on the new build that gives "no Bowtie 2 `--dovetail` but dovetail TLEN rule", which is what every 5-Base Bowtie 2 user got before. If byte-reproducibility of old 5-Base Bowtie 2 BAMs matters to anyone, say so in the CHANGELOG/docs.

Also, Integration says "`--no_dovetail` gains an effect on this path". It already had one, on TLEN. Now it affects both the aligner flag and TLEN.

### 1.5 `@PG` cannot show defaults

Behavior 4 and Validation 2 say the harness verifies `--maxins 800 --dovetail` "in the report and `@PG`". `check_pg_line` reads the Bismark `@PG CL:`, which is the user's command line. B-new passes only `--bowtie2 --five_base_index …`, so the defaults **never appear in `@PG`**. Only the report options line (`verify_options`) can verify them. Fix:
- the plan text;
- the harness: B-new's `@PG` check must assert that `-X`/`--maxins`/`--dovetail`/`--no_dovetail` are *absent* from CL, which proves the defaults came from the build and not the command line;
- the self-tests to match.

### 1.6 The existing harness arms change meaning on the new build

`verify_options` B denies `" --maxins "`, so arm **B dies** on the new build. Arm **B-X** silently gains `--dovetail`, so it stops being #1146's B-X. `B-loc` also gains both defaults. The plan adds B-old/B-new but leaves these three as traps in a committed harness whose purpose is reproducibility. Either:
- redefine them with explicit flags: B → `-X 500 --no_dovetail` (and require `--maxins 500`), B-X → `-X 800 --no_dovetail`, B-loc → `--local -X 500 --no_dovetail`; or
- mark them as `d1764d0`-and-earlier only, and make `cell` refuse them on newer commits.

### 1.7 Ordering of steps: the run would use an untested builder

The implementation outline changes the builder in step 1, runs on oxy in step 5, and only writes the unit/CLI tests in step 7. If a test written after the run finds a builder defect (e.g. wrong token, `--dovetail` emitted twice when the no-op `--dovetail` CLI flag is also given, `--local` interaction), the 10-12 h run is void, because the DONE key includes the commit. Move these before the run, then freeze and record the run's commit SHA:
- the exact-default unit test;
- the `--no_dovetail` and explicit `-X` unit tests;
- the CLI argv test;
- the stub update.

The real-Bowtie 2 gate can stay later, though it is cheap and should also go first.

### 1.8 Edge cases the plan doesn't list

- `--dovetail` given explicitly (a CLI no-op) must not emit the token twice. `--dovetail --no_dovetail` together: `no_dovetail` wins, as in bisulfite. Test both.
- `-I N` without `-X`, with N > 800: now emits `--minins N --maxins 800` and Bowtie 2 errors. Before, it errored at N > 500. The behaviour is the same class, but `-I 600` alone now works where it used to fail. Say so, or leave it, but list it.
- `--local` (Bowtie 2) now also gets `--maxins 800 --dovetail`. The exact-string test at options.rs:622 (`"-q --local --score-min G,20,8"`) changes. Local mode is where dovetails are most frequent (B-loc read-through 919k).
- `--old_flag` on 5-Base Bowtie 2: bisulfite *dies* on `--old_flag` without `--no_dovetail` (options.rs:59). On 5-Base it is a deferred no-op (config.rs:1595), so the new default `--dovetail` goes through silently, apart from the deferred-flag warning. The plan's reasoning is acceptable. Pin it with a one-line test so the divergence is deliberate.

## 2. Assumptions

| # | Assumption | Status |
|---|---|---|
| A1 | `-X 800` covers the library (TLEN p90 ≈ 478) | Supported by #1146 for this library. cfDNA and long-insert libraries are unknown; users can override. OK. |
| A2 | Dovetails explain B-X's 0.7-pt gap to minimap2 | Untested hypothesis. The comparison is confounded: minimap2's 0.80M "read-through" records come from soft-clipping, and B-loc's 919k suggests soft-clipping alone admits them. Bowtie 2 end-to-end `--dovetail` may recover far fewer. Pre-register it as a *secondary, non-gating* expectation (B-new read-through count, Δ unique pairs) so the RESULTS can say whether the motivation held. The decision is unaffected (Felix: Q2). |
| A3 | Extractor overlap removal handles dovetails | True for the R2 overhang (1.2). **False for the R1 overhang**, which is kept. "Tested indirectly by CpG/r/CHG/CHH" is not adequate (4.1). |
| A4 | Same inputs from S3 | The md5-of-decompressed check is right: the committed md5s name plain `sub_R?_val_?.fq`, while the current `prep` gzips and md5s the `.gz`, so the new `input.md5` will **not** be directly comparable. The plan handles this. It also depends on the **same Trim Galore build** (2.2.0 Oxidized; a 2.3.0 exists) and on deterministic output under `--cores 8`. Check `trim_galore --version` before the ~1 h prep, not after. |
| A5 | Same Bowtie 2 build as #1146 | Not stated. Record `bowtie2 --version` and assert it matches the #1146 environment, or B-old reproduction will fail after hours. |
| A6 | Nothing between `9b5c15b` and the branch changes 5-Base PE output | #1146 changed only the notice and docs, so plausible. The B-old reproduction check catches it either way. |
| A7 | Bowtie 2 `--dovetail` only *adds* concordant pairs | Not quite. It changes the concordant search space, so some already-concordant pairs can be re-placed onto a dovetail alignment. The common-set Δr covers this. Note it rather than assuming pure addition. |

## 3. Efficiency

- Four ~2.5 h cells plus prep, in about 5-6 h wall, is reasonable. Dropping minimap2 is right.
- **Add a cheap dry run first:** B-old + B-new at 10M pairs on NA12878, ~15 min each, as #1146 did with `dry10M`. It exercises the new arm definitions, `verify_options`, `check_pg_line`, the read-through diagnostic and the copy-off-scratch step before committing to 100M. A harness bug found at hour 4 on a pod that recycles is the expensive failure mode here.
- Copying BAMs to `~/5base_results` on oxy: four 100M-pair BAMs plus CpG reports is tens of GB. Check that `/home` has room before starting, or go straight to S3.
- The builder change has no runtime cost. Bowtie 2 with a larger `-X` does a little more mate search, which doesn't matter since wall time is excluded.

## 4. Validation sufficiency

### 4.1 The no-harm criteria can't see harm confined to the new pairs (main gap)

The change adds roughly 0.7% of pairs. Each genome-wide guard is diluted ~140× before it could move:
- CHG/CHH ≤ +0.2 pt, from the alignment report at 1-decimal resolution;
- common-set Δr ≥ −0.001;
- deconvolution P/R ≥ −1 pt, at 1 decimal.

A complete artefact on every new pair would shift CHH by ≪ 0.1 pt. Two guards are close to tautological for an additive change: mapping efficiency Δ ≥ 0 and ΔCpG ≥ 0. As written, the run is very likely to "pass" whether or not dovetailed pairs are clean.

**Add a targeted, pre-registered check on the new pairs.** It needs no extra alignment.
- **Target set:** B-new R1 records with |TLEN| < read length, the same definition as `readfate.tsv`'s `readthrough`. Ideally, B-new pairs whose QNAME is absent from B-old (`samtools view` both BAMs, then `comm` on sorted names).
- **Report for that set:**
  - (a) its size;
  - (b) the overhang length distribution;
  - (c) CHH and CHG methylation from the `XM` tags (`h/H`, `x/X` counts), against the same figure for all other B-new pairs;
  - (d) CpG methylation for the same pair;
  - (e) a mismatch rate (`NM` or XM-masked) for overhang bases against body bases.
- **Criterion, for example:** new-pair CHH ≤ 2× the rest-of-library CHH, *and* ≤ rest + 1 pt. Fix the threshold before the run.

This turns the "Dovetailed pairs could carry overlap artefacts" risk in the Self-Review into something the run actually tests.

### 4.2 The gain-set guard is biased toward failure

The gain set here is sites where B-old has cov < 10 and B-new ≥ 10. Almost all of them sit at cov 10-11, pushed over by a <1% depth increase. The common set has a much higher mean coverage. A site at cov 10 has more binomial noise than the common set's average, so gain-set r < common r − 0.02 is *expected* even if the new pairs are perfect. #1146 B-X failed the same guard (0.937 vs 0.944) for exactly this reason, and RESULTS.md calls it "strict".

Pre-register instead:
- a **coverage-matched** comparator: B-old's r on its own cov 10-11 sites, against B-new's gain-set r;
- a **minimum n**, e.g. gain n < 2,000 means report only, not gate.

Otherwise the outcome is pre-determined and "stop and bring to Felix" fires on noise.

### 4.3 Reproduction checks: good, but tighten them

- **NA12878 B-old.** Currently compares only `pairs_in` (input-determined, not alignment-determined), `unique_pairs` and `map_eff`, the last to 1 decimal only. The committed B-X `metrics.tsv` gives much stronger alignment-dependent fingerprints for free. Require exact equality on:
  - `records` 185,331,762;
  - `mapq0` 1,697,864;
  - `r1_records` 92,665,881;
  - `readthrough` 2,929;
  - TLEN p10/p50/p90 246/353/479;
  - `cpg_pct`/`chg_pct`/`chh_pct`/`dup_pct` 48.6/1.3/1.2/3.40;
  - deconvolution 93.5/61.6;
  - CpGs at cov ≥ 10 (own set), 822,432.

  All of these should reproduce despite the TLEN-sign difference in 1.4, because none reads TLEN's sign. If any one doesn't, that is a real finding.
- **HG002** has no Bowtie 2 baseline from #1146 (arm A only), so md5 is its only reproduction check. That's acceptable, but say so: HG002 B-old is a fresh baseline, not a reproduction.
- **Precondition checks.** Assert the Trim Galore and Bowtie 2 versions (A4/A5) *before* prep.
- **Mismatch stops the work.** Good. Also say what happens on an md5 match but B-old mismatch, i.e. a toolchain change: stop, and do not re-baseline silently.

### 4.4 Test coverage

The plan's list is incomplete and partly over-specific. Exact-string 5-Base Bowtie 2 assertions that will change:
- `options.rs`: `five_base_defaults_and_explicit_defaults`, `five_base_forwards_tuning_in_bisulfite_token_order` (the `-I 10 -X 1000` strings at ~600-613), the `--local` strings at ~622-626, and `five_base_omits_bisulfite_policy_and_runtime_threads`. In the last, `--dovetail` must stay forbidden for HISAT2 and for Bowtie 2 with `--no_dovetail`, and `--ignore-quals`/`--no-mixed`/`--no-discordant` must stay forbidden for both.
- `mod.rs` `five_base_stub` (~8421-8437).
- `aligner_cli.rs` `five_base_1134_default_argv_is_exact` and the tuning/threads tests.

Use `grep -rn 'score-min L,0,-0.6\|score-min G,20,8' rust/` to find them all, rather than an enumerated list.

Missing tests:
1. Explicit `--dovetail` gives exactly one token; `--dovetail --no_dovetail` gives none.
2. `--local` gives `… --maxins 800 --dovetail` (or the bisulfite order, 1.1).
3. `-X 500` explicit gives `--maxins 500`, with no default appended.
4. HISAT2 `--no_dovetail` gives an unchanged string.
5. `--old_flag` on 5-Base Bowtie 2 is accepted and keeps `--dovetail` (pins 1.8).
6. **Real Bowtie 2 dovetail gate, strengthened.** "Kept by default, dropped with `--no_dovetail`" doesn't check what the pair looks like. Also assert:
   - TLEN magnitude = outer span, and sign (R1-forward positive, R2 negative), for both an OT-type and an OB-type dovetail (R1 forward and R1 reverse);
   - `0x2` set on both mates;
   - the XM strings match a hand-computed call on a reference with a CpG in the overhang.

   Build the pair so the overhang matches the reference, e.g. R1 = ref[100..250], R2 = rc(ref[90..240]), or Bowtie 2's end-to-end scoring decides the case rather than `--dovetail`.
7. **Extractor on the dovetail BAM** (optional but cheap): run the extractor on the gate's output and assert that R2's overhang calls are absent and R1's are present. That pins the 1.2 behaviour.
8. The long-fragment gate. Use a fragment ≥ 1,000 bp (the plan says 1,000) on the existing 10 kb reference with 90 bp mates. Also keep an explicit `-X 500` case that *excludes* the existing 800 bp fragment, so the "explicit `-X` wins even below the default" path is tested on real Bowtie 2.

### 4.5 Duplex/consensus not validated

The new pairs reach `--five_base_duplex` and UMI dedup, and 1.2 identifies a specific family-splitting mechanism. The benchmark runs neither. Either:
- add a small duplex smoke comparison on the demo data (10M pairs with `--five_base_umi_qname --five_base_duplex`), B-old against B-new, reporting family count, paired-family fraction and duplex CpG calls; or
- scope it out explicitly in the plan with that rationale, so the docs don't imply it was checked.

## 5. Alternatives

- **`--dovetail` before `--maxins`** (1.1): genuinely the bisulfite order, at no cost.
- **Gate `--dovetail` on the run result rather than defaulting first.** Felix chose default + no-harm (Q2), and the bisulfite convention supports it, so I don't re-open it. But the no-harm run must be able to fail (4.1), or the pre-registration is ceremony.
- **Add `--no-mixed --no-discordant` to the 5-Base Bowtie 2 path** as well. These are irrelevant to output, since the driver discards anything without `0x2`, but they save Bowtie 2 search time and match bisulfite. The plan keeps them out deliberately (#1134). That's fine; noted as a possible later throughput tweak.
- **A soft-clip-aware alternative:** if 4.1 shows R1-overhang contamination, an `--ignore_3prime_r2`-style or overhang-masking step in the driver (mask R1 bases past R2's 5' end when the pair dovetails) would keep the mapping gain without the junk calls. Only worth considering if the data demand it.

---

## Action items

### Critical
1. **Add a targeted, pre-registered check on the newly admitted pairs** (4.1): the read-through/new-QNAME set's size, overhang lengths, CHH/CHG/CpG from XM, and mismatch rate, with a fixed threshold. Without it the no-harm run cannot detect the most plausible harm (R1 overhang calls under inverted polarity, 1.3).
2. **Replace or condition the gain-set r guard** (4.2) with a coverage-matched comparator and a minimum n. As written it is likely to fail on noise.
3. **Fix the `@PG` claim** (1.5): defaults are only verifiable in the report options line. B-new's `@PG` check should assert that `-X`/`--dovetail`/`--no_dovetail` are absent from CL.

### Important
4. Correct the token-order claim (1.1). Prefer `--dovetail` before `--minins`/`--maxins`.
5. Reword "B-old reproduces the old behaviour" to "metric-equivalent". Document in the docs/CHANGELOG that `-X 500 --no_dovetail` restores old alignments but not old TLEN signs on coincident OB pairs (1.4).
6. Tighten the NA12878 B-old reproduction to the full committed `metrics.tsv` fingerprint plus own-set CpGs at cov ≥ 10 (4.3). State that HG002 B-old is a new baseline.
7. Pre-check the Trim Galore (2.2.0 Oxidized) and Bowtie 2 versions before prep (A4/A5).
8. Write the unit/CLI tests and stub update **before** the oxy run, and freeze and record the run commit (1.7).
9. Redefine or fence the old harness arms B / B-X / B-loc, which break or silently change on the new build (1.6).
10. Add a 10M dry run of B-old/B-new before the 100M cells (3).
11. Extend the real-Bowtie 2 dovetail gate to assert TLEN sign and magnitude, `0x2`, and XM, for both orientations, and keep an explicit `-X 500` exclusion case (4.4 items 6 and 8).
12. Address duplex/UMI dedup (4.5): a smoke comparison, or an explicit scope-out.
13. Update the docs table: the "Bowtie 2" row (87.9%) is now the *old* default and needs relabelling (e.g. `-X 500 --no_dovetail`), with a row for the new default.

### Optional
14. Test the explicit `--dovetail`, `--dovetail --no_dovetail`, `--local` and `--old_flag` edge cases (1.8, 4.4 items 1, 2, 5).
15. Pre-register the A2 hypothesis as a secondary outcome: the B-new read-through count and the Δ unique pairs against minimap2.
16. Compute mapping efficiency and CHG/CHH at full precision (`unique/pairs_in`; extractor splitting-report counts) rather than the report's 1-decimal figures before applying ± thresholds.
17. Script the verdict (criteria against the two `metrics.tsv` files and `score.tsv`) instead of applying it by hand.
18. Check `/home` free space on oxy for the BAM copies, or copy straight to S3.
19. An extractor test on a dovetailed BAM that pins "R2 overhang dropped, R1 overhang kept" (4.4 item 7).

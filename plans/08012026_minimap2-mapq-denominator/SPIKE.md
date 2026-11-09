# SPIKE — does minimap2's `-x sr` `end_bonus` break `perfect = 2 × read_length`?

**Issue:** [#1081](https://github.com/FelixKrueger/Bismark/issues/1081) — minimap2/rammap MAPQ denominator
**Date:** 2026-08-01 · **Branch:** `plan/mapq-minimap2-denominator` (from `dev` `172da96`)
**Verdict:** **`end_bonus` does not reach `AS:i:`. `perfect = 2 × read_length` is a true upper bound for every preset Bismark can select. Proceed with the fix.**

---

## 1. Question, criteria, strategy

**Question.** `-x sr` sets `mo->end_bonus = 10` (`options.c:157`) and `end_bonus` is passed into minimap2's ksw alignment routines. If it lands in the reported `AS:i:`, then `AS` can exceed `2 × len`, `bestOver > diff` again, and the MAPQ saturation #1081 exists to remove comes straight back for short reads — and `-x sr` is what Bismark selects for `--mm2_short_reads` **and** `--illumina_5base`.

**Success criteria.** Both halves must agree:

| # | Criterion |
|---|---|
| C1 | Every write to the field that `AS:i:` prints is traceable, and none of them adds `end_bonus` |
| C2 | A real minimap2 run under `-x sr` never reports `AS > 2 × len` — and a **perfect** read reports **exactly** `2 × len` (a `+10`/`+20` leak would be visible in that one number) |
| C3 | The match score is `2` for **every** preset Bismark can reach, so a single `perfect` constant suffices |

**Strategy.** Two independent halves, because either alone is refutable: mechanical assertions over upstream source (minimap2 **and** rammap), plus an empirical run of the real binary with Bismark's verbatim option string, including cases built so that `end_bonus` is the term that decides the alignment.

**Out of scope.** Whether the fix should ship, which MAPQ ladder to use, and the byte-identity parity call — all plan questions. §6 prices the ladder choice because the spike produced the numbers cheaply, but does not decide it.

---

## 2. Script

```bash
cd plans/08012026_minimap2-mapq-denominator/spikes
python3 spike_end_bonus.py --mm2-src <dir with fetched minimap2 *.c> \
                           --rammap-src ~/.cargo/git/checkouts/rammap-*/5ea62cd/rammap-core/src
```

- Script: `plans/08012026_minimap2-mapq-denominator/spikes/spike_end_bonus.py`
- Captured output: `plans/08012026_minimap2-mapq-denominator/spikes/run.log`
- Upstream sources fetched from `raw.githubusercontent.com/lh3/minimap2/master/` (`options.c`, `align.c`, `format.c`, `ksw2_extz2_sse.c`, `minimap.h`, `map.c`) into the session scratchpad — **not** checked in.
- Binary used: **minimap2 2.31-r1302** on PATH (`/opt/homebrew/bin/minimap2`) — the same version the shipped byte-identity gate was established against. (The previous session recorded minimap2 as absent; it is present now.)

**Result: 16/16 checks passed.**

---

## 3. Iterations

**#1 — source + empirical, as designed.** 16 checks, all green: `end_bonus` never enters `AS`, and a perfect read scores exactly `2 × len` under all three reachable presets. Criteria C1–C3 met on the first run; no fix or retry needed.

**#2 — added ladder pricing (§6) and a second-best axis.** Not needed to answer the `end_bonus` question, but the spike had already reconstructed both Perl ladders, so pricing the #1088 tripwire (local floor 22 vs end-to-end floor 0) and the with-second-best branch cost almost nothing. This produced the two findings that change the plan most (F6, F7).

---

## 4. Findings

### F1 — `end_bonus` cannot reach `AS:i:` (C1 ✅)

`AS:i:` prints `r->p->dp_score` (`format.c:403`; `ms:i:` prints `dp_max0`, a different field). Every write to `dp_score` in `align.c`:

| Line | Write | Context |
|---|---|---|
| 794 | `r->p->dp_score += ez->max;` | left extension |
| 861 | `r->p->dp_score += ez->max;` | Z-dropped gap fill |
| 869 | `r->p->dp_score += ez->score;` | gap fill |
| 886 | `r->p->dp_score += ez->max;` | right extension |
| 950 | `r_inv->p->dp_score = ez->max;` | inversion (separate record) |

Five writes, two distinct right-hand sides, **no `end_bonus` term**. `opt->end_bonus` is passed at exactly two call sites (`align.c:791`, `:883` — the left and right extensions, both `KSW_EZ_EXTZ_ONLY`); gap fill passes a hard-coded `-1`. Inside ksw2 it appears in exactly **one** executable line:

```c
} else if (!ez->zdropped && (flag&KSW_EZ_EXTZ_ONLY) && ez->mqe + end_bonus > (int)ez->max) {
    ez->reach_end = 1;                                   // ksw2_extz2_sse.c:304-306
```

So `end_bonus` decides **which alignment is backtracked** — extend to the end of the query (`mqe`) or stop at the local DP max (`ez->max`) — and the score added to `dp_score` is `ez->max` either way. It is a traceback tie-breaker, not a score term.

### F2 — `AS ≤ 2 × len` is structural, not just observed

`ez->max` is a maximum over DP cells whose recursion starts at 0 and gains at most `a` per query base, so `ez->max ≤ a × (query bases in that block)`; `ez->score` is that block's global score, same bound. The blocks partition the aligned query span without overlap, and the aligned span is `≤` the read length (minimap2 soft-clips with `-a`). Hence `AS ≤ a × len = 2 × len`.

This is the same relationship Bowtie 2-local has (`perfectScore(rdlen) = rdlen × match`, `scoring.h:310-316`, using the **full** read length even when the alignment soft-clips), so #1079's arithmetic carries over unchanged.

The single mechanism that *could* push a minimap2 DP score above `a × len` is the splice junction bonus (`opt->junc_bonus`, added inside `ksw_exts2_sse`) — and splice presets are unreachable from Bismark.

### F3 — empirically confirmed, including the sharpest case (C2 ✅)

64 primary alignments across `{map-ont, map-pb, sr}` × `len ∈ {50, 100, 150, 250, 1000}` × {perfect, terminal mismatch, both-ends mismatch, 1 interior mismatch, 3 interior mismatches}, Bismark's verbatim option string (`-a --MD --secondary=no -t 2 -x <preset> -K 250K`):

- **0 cells with `AS > 2 × len`.**
- Every perfect read scored **exactly** `2 × len` — `100`, `200`, `300`, `500`, `2000`. A `+10` or `+20` leak would be a one-digit change in these five numbers under `sr`; it is not there.

### F4 — `end_bonus`'s real effect is visible in the CIGAR, and it makes `AS` *overstate* its own alignment

The same read, terminal mismatch at the last base, 100 bp:

| preset | `end_bonus` | CIGAR | `AS` | actual score of that CIGAR |
|---|---|---|---|---|
| `map-ont` | −1 | `99M1S` | 198 | 198 ✓ |
| `sr` | 10 | **`100M`** | 198 | 2·99 − 8 = **190** |

Under `sr` the bonus makes `mqe + 10 > ez->max`, so the reported alignment covers all 100 bases — but the score added is still `ez->max = 198`. So `AS` can **exceed the score of the CIGAR it is attached to** by up to roughly `a + b`. It still cannot exceed `2 × len`, so the fix is unaffected; worth knowing because it means `AS` and the CIGAR are not mutually derivable under `-x sr`, and any future test that recomputes `AS` from a CIGAR would be wrong for that preset.

### F5 — one `perfect` constant is enough (C3 ✅), and rammap agrees

| preset | reachable via | `a` | `end_bonus` |
|---|---|---|---|
| `map-ont` | default, `--mm2_nanopore` | 2 (inherits `opt->a = 2`; "same as the default", `options.c:96`) | −1 |
| `map-pb` | `--mm2_pacbio` | 2 (branch sets index options only, `:103`) | −1 |
| `sr` | `--mm2_short_reads`, `--illumina_5base` | 2 (explicit, `:155`) | **10** |

Nothing else is reachable: `--mm2_*` are selectors only, and the emitted string is closed (`options.rs:288`) — no `-A`/`-x` passthrough. `sc_ambi = 1` is forced negative when the matrix is built (`align.c:16`), so an `N` can never out-score a match.

**rammap** (`rammap-core` @ `5ea62cd`, the pinned in-tree dep) is a faithful reimplementation on all three counts: `sr` sets `match_score = 2` and `end_bonus = 10` (`api.rs:690,692`), `dp_score` is fed only `ez.max`/`ez.score` (`align/extend.rs`, 4 writes), and `end_bonus` appears only in the traceback-start decision (`align/dp/common.rs:495`).

**Asymmetry found in passing (not a spike question, plan-relevant):** the **in-process** rammap backend — the `--rammap` default — hard-codes `rammap::Preset::MapOnt` (`mod.rs:968`, `inprocess.rs:702,754`) and never reads the preset out of the option string. So `--rammap --mm2_short_reads` runs `map-ont` in-process and `-x sr` via `--rammap_subprocess`. For this issue that only *reduces* exposure (in-process `end_bonus` is always −1), but it is a silently-ignored flag and should be reported as its own issue.

### F6 — the defect is more structured than "MAPQ 42 for everything"

The handoff's verified claim ("42 in every cell") is the **no-second-best** branch. Both branches are live, and they fail differently:

| branch | today | why |
|---|---|---|
| no second best | **42**, degenerate at every length and every score | `bestOver/diff = AS/(0.2·len) + 1 > 1` always ⇒ the `≥ 0.8·diff` top rung |
| with second best | **33** for almost everything, **not** pinned | the rung is set by `bestDiff/diff` with `diff = 0.2·len` — ~11× too small — so `bestDiff ≥ 0.9·diff` is met by any `ΔAS ≳ 0.18·len`; and every `bestOver` sub-threshold is trivially satisfied because `bestOver ≫ diff` |

For minimap2/rammap the second-best score can only come from the **cross-instance runner-up** (`merge.rs:344-350`): minimap2 runs `--secondary=no` and emits no `ZS:i:`, and the in-process rammap builder sets `second_best: None` explicitly. So a read that aligns in only one strand instance takes the 42 path; one that aligns in both takes the 33 path.

Priced at 100 bp (`perfect = 200`, `scMin = −20`):

| `AS` | `AS₂` | today | new + local ladder | new + e2e ladder |
|---|---|---|---|---|
| 200 | 190 | 25 | **11** | 6 |
| 200 | 100 | 33 | **34** | 34 |
| 200 | 20 | 33 | **39** | 38 |
| 120 | 110 | 25 | **11** | 2 |
| 120 | 40 | 33 | **18** | 3 |

The with-second-best values move in **both** directions, and the near-tie case (200 vs 190 — a read that maps almost equally well to two strands) drops from 25 to 11. That is the correctness win, and it is a bigger behavioural change than the ceiling move. **A plan that only enumerates "42 → a ladder" understates the blast radius** — exactly the failure #1080's code review caught (a risk note that names one instance licenses ignoring the class).

### F7 — the #1088 ladder tripwire, priced

`local_ladder()` is now derived as `match_bonus > 0.0`, so any nonzero `match_bonus` moves minimap2/rammap onto the **local** ladder. No second best:

| `bestOver/diff` | local | end-to-end |
|---|---|---|
| 1.00 | 44 | 42 |
| 0.85 | 44 | 42 |
| 0.75 | 42 | 40 |
| 0.65 | 41 | 24 |
| 0.55 | 36 | 23 |
| 0.45 | 28 | 8 |
| 0.35 | 24 | 3 |
| 0.25 | 22 | **0** |
| 0.10 | 22 | **0** |

Across 100 % → 20 % of perfect, at every length: **local = 44/44/41/28/22**, **end-to-end = 42/42/24/8/0**. The handoff's predicted ladder (44/44/41/28/22) is the **local** one, reproduced exactly.

The decision is a floor decision, not a ceiling decision: the local ladder's floor is 22, the end-to-end ladder's is **0**, and MAPQ 0 is discarded by `samtools view -q 1` and by methylseq's filters. The spike does not decide this; it establishes that the two options differ by up to 28 MAPQ points in the middle of the range and by "kept vs discarded" at the bottom.

### F8 — scope re-verifications

| Claim | Status |
|---|---|
| **5-Base never calls `calc_mapq`** | ✅ Re-verified: `five_base_emit_record` passes the aligner's own MAPQ through verbatim (`mod.rs:1319` `mapq: rec.mapq`; PE `:1662` `rec1.mapq.min(rec2.mapq)`). `--five_base_min_mapq` filters on that same column (`:2060`), so a DRAGEN-style `20` threshold is being applied to minimap2's 0–60 scale, which is what it was designed for. **5-Base is unaffected by this issue in either ladder.** |
| **minimap2/rammap are SE-only** | ✅ `reject_unsupported_paired_aligner` (`config.rs`) rejects PE for both. No PE behaviour to write into the plan. |
| **`--local` is rejected** for both | ✅ `config.rs:674`; only the end-to-end path is in play. |
| **`--score_min` is a fiction for minimap2** | ⚠️ **New.** The minimap2 clean slate (`options.rs:233`) throws away the base option string, so minimap2 **never receives `--score-min`** — yet `score_min_params` still returns `(0.0, −0.2)` and `calc_mapq` uses it (`config.rs:793-802`). So `scMin` is a number no aligner ever saw, and a user's `--score_min L,0,-5` changes minimap2 MAPQ without changing a single alignment. Pre-existing; the fix inherits it (Bowtie 2's formula is `perfect − scMin`, so keeping `scMin` is the faithful analogue). Must be an explicit assumption in the plan, not a silent inheritance. |

---

## 5. Reference snippets worth carrying to implementation

**The upper bound, as a citable chain** (for the code comment and the CHANGELOG):

```
AS:i: == r->p->dp_score                                  format.c:403
dp_score += ez->max | ez->score                          align.c:794,861,869,886
ez->max, ez->score <= a * (query bases in the block)      ksw2 recursion from 0
a == 2 for map-ont / map-pb / sr                          options.c:47,96,103,155
=> AS <= 2 * read_length                                  (soft clips only lower it)
```

**`end_bonus`'s single use** — quote this, not the five call-site lines, if a reviewer asks why the bonus is irrelevant:

```c
ez->mqe + end_bonus > (int)ez->max      /* ksw2_extz2_sse.c:304 — traceback start, not score */
```

**The empirical one-liner** that discriminates a leak from no leak, if this ever needs re-checking against a new minimap2:

```bash
minimap2 -a --MD --secondary=no -t 2 -x sr -K 250K ref.fa perfect_100bp.fq | grep -o 'AS:i:[0-9]*'
# AS:i:200 == 2*len  → no leak.   AS:i:210 / AS:i:220 → end_bonus leaked.
```

---

## 6. Recommendation

**Proceed to the plan with `perfect = 2 × read_length` and a single `2.0` constant.** The blocker is cleared on both source and empirical grounds, at every reachable preset, and rammap behaves identically.

Three things the spike changed about what the plan must contain:

1. **Price the with-second-best branch, not just the 42 ceiling** (F6). It is live, it is not currently saturated, and it moves in both directions — the near-tie case drops 25 → 11. Any validation that only covers unique reads repeats #1080's miss.
2. **The ladder choice is a floor decision** (F7). Local floor 22 vs end-to-end floor 0 = "kept" vs "discarded by `samtools -q 1`". Decide it on that, and be explicit that neither ladder is "what minimap2 would compute" — minimap2 has no ladder.
3. **State the `scMin` fiction as an assumption** (F8). minimap2 never receives `--score-min`, so the denominator mixes a real `perfect` with a synthetic `scMin`, and `--score_min` moves minimap2 MAPQ without moving an alignment.

---

## 7. Limitations

- **Read simulation, not real bisulfite data.** Reads were cut from a 30 kb random reference with synthetic mismatches. It exercises the scoring arithmetic, which is the question; it does not exercise Bismark's converted-genome instances, indels, or repeats. `AS ≤ 2·len` is argued structurally in F2 precisely because sampling cannot establish a bound.
- **Indels untested empirically.** Gaps only subtract (`q`, `e` > 0), so they cannot lift `AS` above `2 × len`; not separately measured.
- **One binary, one version** (2.31-r1302, macOS arm64). The source assertions run against minimap2 `master`, so a future release that starts adding `end_bonus` into `dp_score` would break the assertion, not silently pass — but this is not pinned in CI.
- **rammap checked by source reading only.** No rammap binary was run; the in-process backend is behind a default-OFF feature on this platform. The claim "rammap mirrors minimap2" rests on the three source facts in F5, not on measurement.
- **Not tested: whether the fix should ship.** The byte-identity parity call (minimap2 SE is byte-identical to Perl v0.25.1 + minimap2 2.31-r1302) is Felix's, and the spike deliberately does not pre-empt it. *(Settled 2026-08-01: ship unconditionally.)*

---

## 8. Post-review corrections (2026-08-01)

Dual plan review of the plan this spike fed (`PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md`) audited the spike too. Three corrections; the **verdict is unchanged** — `end_bonus` does not reach `AS`, and `perfect = 2 × read_length` holds.

### F2 — the partition step was asserted, not shown (Reviewer B supplied it)

F2 claimed `Σ ez->max ≤ a·qlen` from "the blocks partition the aligned query span" without demonstrating the partition. It does hold: `mm_align1` scores the left extension over `[qs0, qs)` (`align.c:779-798`), the gap-fill loop over `[qs, qe)` advancing `rs = re, qs = qe` each iteration (`:800-866`), the right extension over `[qe, qe0)` (`:871-889`), with `assert(qe1 <= qlen)` at `:890`. No overlap. Additionally: Z-drop cannot double-add (the second pass at `:842` overwrites `ez` before the single `+=`), a Z-drop split is a **separate record** with its own `dp_score`, and `mm_update_dp_max` (`:1022-1044`, `:1092-1094`) writes `dp_max` — not `dp_score`.

### F5 — the rammap evidence pointed at the wrong field (**the one real hole**)

F5 traced the field *named* `dp_score` in `rammap-core/src/align/extend.rs`. **That is not what rammap prints as `AS:i:`.** rammap's naming is transposed relative to minimap2:

| rammap field | minimap2 analogue | printed as |
|---|---|---|
| `align_score` (`pipeline.rs:2483`, `:2598`; carried by `Mapping.score`, `api.rs:158-159`) | `dp_score` | **`AS:i:`** |
| `dp_score` (what F5 traced) | `dp_max` | `ms:i:`-equivalent; receives `update_dp_max`, the splice bonus, jump adjustments |

Consequences:

1. **One additive non-match term is on the real AS path and is not splice-gated** — `pipeline.rs:1070-1074`: `align_score = ez.max + gap_open + gap_extend * stripped_gap_len` when a leading gap was stripped. Its own comment says *"Add back the cost of the stripped leading gap"*, i.e. it restores a penalty that was definitely subtracted, which preserves the bound — **but this spike never made that argument, because it never looked at this field.** Carried as the plan's A3 residual, rammap only.
2. **The `match_score` citation was for the wrong preset.** `api.rs:690,692` is the **`sr`** branch. The in-process backend hard-codes `Preset::MapOnt`, and `map-ont` sets only `k`/`w` (`api.rs:617-619`), so the load-bearing constant for the default path is the struct default `match_score: 2` at **`align/map.rs:215`** (verified: 2). The fact holds; the citation did not support it.
3. rammap's splice bonus lands on `dp_score` where minimap2 puts it on `dp_max` — a genuine divergence from minimap2, unreachable from Bismark (gated on `AlignFlags::SPLICE` + a jump DB, `pipeline.rs:2113-2121`). Same escape hatch as `junc_bonus`.

**Method lesson:** the mechanical assertions in `spike_end_bonus.py` matched field *names* across two codebases. For minimap2 that was sound (the name was checked against `format.c`'s printf); for rammap it was not, because no equivalent "what does the SAM writer actually print" check was run. A grep for `AS:i:` in rammap — one line — would have caught it.

### F6 — "aligns in both instances ⇒ the 33 path" is wrong (both reviewers)

F6 said a read aligning in one instance takes the 42 path and one aligning in both takes the 33 path. The merge stores only the **running maxima** in slot order (`merge.rs:254-310`), ties are `Ambiguous` (`:338-342`), and same-`chromosome:pos` pairs collapse (`:399`) — so the runner-up branch is reached only when a **later** slot **strictly** out-scores every earlier one, i.e. for SE-directional roughly **half** of the reads that align in both. The priced table in F6 is correct; its stated *reachability* was not. Corrected in the plan's §1 and §3.5.

### Unchanged and independently confirmed

F1, F3, F4, F7, F8 all stood, and Reviewer B attempted to break `AS ≤ 2·len` through indels, Z-drop, inversions, `mm_update_dp_max`, the `sr` ungapped path, ambiguous bases and short reads without success. Both reviewers reproduced the 44/44/41/28/22 ladder and the local-22 vs end-to-end-0 floor from an independently extracted Perl oracle.

**One thing this spike should have done and did not:** gate A1 in CI. minimap2 is already installed in the CI workflow (`rust_ci.yml:35-42`) and the repo already has a skip-locally / panic-if-`$CI` pattern (`aligner_five_base_groundtruth.rs:55-67`), so §5's one-liner could have been a test rather than a note — which would also extend the evidence to a second minimap2 version. Now the plan's V15.

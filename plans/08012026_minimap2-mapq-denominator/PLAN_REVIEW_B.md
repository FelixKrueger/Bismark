# PLAN_REVIEW_B — minimap2/rammap MAPQ denominator (rev 1)

**Reviewer:** B (independent; no coordination with Reviewer A)
**Target:** `plans/08012026_minimap2-mapq-denominator/PLAN.md` rev 1 (+ `SPIKE.md`)
**Tree reviewed:** `plan/mapq-minimap2-denominator` (based on `dev` `172da96`)
**Verdict:** **Not ready as-is — one Critical.** The design is sound and I could not break the load-bearing bound for minimap2. But §7 step 7's cell **(d)** cannot be built the way it is specified, and the *reason* it cannot is a mischaracterization of when the second-best branch is reached — the same claim the plan uses in §1 and in the CHANGELOG requirement. Fix those two and the plan is implementable.

D-PARITY and D-LADDER are treated as locked. I found no factual error undermining either premise.

---

## 0. What I checked, and what held

Positive results first, because most of the plan is right and the implementer should know which parts not to re-litigate.

**A1 (`AS ≤ 2 × read_length`) — I tried to break it and could not, for minimap2.** Re-derived against upstream `master` (fetched `align.c`/`format.c`/`options.c`), not from the spike:

- `AS:i:` ← `r->p->dp_score`; `ms:i:` ← `dp_max0` (`format.c:403`). Confirmed.
- Exactly five writes to `dp_score`: `align.c:794, 861, 869, 886, 950`. Confirmed — the spike's table is exact.
- **The partition argument holds, which is the part the spike asserts but does not show.** `mm_align1` scores the left extension over query `[qs0, qs)` (`align.c:779-798`), the gap-fill loop over `[qs, qe)` advancing `rs = re, qs = qe` each iteration (`:800-866`), and the right extension over `[qe, qe0)` (`:871-889`), with `assert(qe1 <= qlen)` at `:890`. No block overlaps another, so `Σ ez->max ≤ a·qlen`.
- **Z-drop cannot double-add**: the second pass (`:842`) overwrites `ez` before the single `+=` at `:861`/`:869`. The split (`mm_split_reg`, `:857`) produces a *separate record* with its own `dp_score` over its own query sub-range.
- **`mm_update_dp_max` (`:1022-1044`) and `:1092`/`:1094` write `dp_max`, not `dp_score`** — so the team-lead's specific concern is cleared: they cannot move `AS`. (`mm_recal_max_dp` returns `a·(mlen − b2·n_mis − gap_cost)` ≤ `a·mlen`, so even where rammap *does* apply it to a score field the bound survives — see I5.)
- **The `sr` ungapped path** (`:820-829`) accumulates `+a` per match / `−b` per mismatch / `−sc_ambi` per ambiguous base over `qe−qs` bases → ≤ `a·(qe−qs)`. Inversions (`:950`) are a separate record. `junc_bonus` is splice-only (`options.c:176`). Indels only subtract.
- `a == 2` for **every** reachable preset: default `options.c:47`; `map-ont`/`lr` = "same as the default" `options.c:96`; `map-pb` sets index options only `options.c:103`; `sr` sets it explicitly `options.c:155`. The plan's line citations are exact. `map-hifi`/`asm*` set `a = 1` but are unreachable — `minimap2_options` (`options.rs:256-289`) emits a closed string with three preset selectors only.

**Every predicted MAPQ value is correct, and so is the stated reasoning per row.** I extracted Perl `calc_mapq` (`bismark:3923-4186`) verbatim, injected `scMin`/`diff` (nothing else touched), and drove both ladders:

| cell set | oracle result |
|---|---|
| §3.3 table 1 (len 100, no 2nd) | old 42×5; new/local **44 / 44 / 41 / 28 / 22**; new/e2e 42 / 42 / 24 / 8 / **0** ✅ |
| §3.3 table 2 (with 2nd) | old 25/33/33/25/33; new/local **11 / 34 / 39 / 11 / 18**; new/e2e 6/34/38/2/3 ✅ |
| §7 step 7 (len 6) | (a) 42→**44** (b) 42→**28** (c) 42→**22** (d) 27→**11** ✅ |
| V1 length-invariance | identical at 50/100/150/250/1000 ✅ |
| V3 floor at 20 % of perfect | local **22**, e2e **0**, at every length ✅ |

I also re-derived each row's rung/leaf by hand (`10 < 0.1·220` ⇒ `best_diff > 0` terminal leaf, `bestOver ≥ 0.5·diff` ⇒ 11; `0.4` rung with `bestOver == diff` ⇒ 34; the flat `0.8` rung ⇒ 39; etc.) — the plan's "why" column is right, not just its numbers. (#1080's "right number, wrong reasoning" trap is not repeated here.)

**Scope claims re-verified independently:**

- 5-Base never calls `calc_mapq`: `mod.rs:1319` `mapq: rec.mapq`, PE `:1662` `rec1.mapq.min(rec2.mapq)`, and `--five_base_min_mapq` filters that same passed-through BAM column (`mod.rs:2060`, `inner.mapping_quality()`). SE 5-Base is `unreachable!()` at `mod.rs:591`, rejected at `config.rs:703` + `:714`. §5 is correct and stronger than "likely".
- `s2:i:` is ignored **by construction** (`align.rs:144-152` matches `XS:i:`/`ZS:i:` only; pinned by `align.rs:971-995`), so for these aligners `second_for_mapq` can only be the cross-instance runner-up. Correct.
- No MAPQ assertion exists in `aligner_methylseq_conformance.rs` (CLI-shape only, no checksums), `aligner_five_base_groundtruth.rs`, or `genome_prep_byte_identity_real_data.rs`. The `perl-oracle` job is `EXPECTED=13` with **no aligner cell** (`rust_ci.yml:194-208`) — confirmed, CI gives zero protection here.
- `aligner_rammap_inprocess_crosscheck.rs` asserts field-identity on the **aligner's own** `mapq` *and on `alignment_score`* — so V12 is right, and the AS-identity assertion is a stronger (indirect) protection than the plan claims.
- `--ambig_bam` copies MAPQ from field 4 of the raw aligner line (`output.rs:819`, `:833`) → unaffected (see O3).
- Version literals are `3.1.0` in all three files. `local_ladder()` is `self.match_bonus > 0.0` (`config.rs:174-176`) → D-LADDER genuinely needs no code change.

**Line numbers sampled and found accurate:** `config.rs` 80 / 82-225 / 123-127 / 131-143 / 161-176 / 793-802; `mapq.rs` 1-19 / 29-43 / 48-135 / 143-220 / 661-699 / 711-736; `options.rs` 233 / 256-289 / 376-404; `merge.rs` 332-350 / 367 / 740; `aligner_cli.rs` 2671-2688 / 2811; `docs/…/alignment.md:111`; `rust/README.md:161`; `CHANGELOG.md:14`. Only cosmetic drift found (O5).

**§6 (one PR, both aligners) is sound.** `rust/README.md:161` does frame rammap as "concordance-gated (NOT byte-identical to minimap2)" with "same `--mm2_*` knobs (apples-to-apples vs `--minimap2`)", so moving rammap alone would break the gate rammap actually has, for every read. The mechanical argument (one `match` arm) is also right. I found one unenumerated consequence of moving both at once (O4), but nothing that argues for splitting.

---

## 1. Logic review

### C1 — CRITICAL: §7 step 7 cell (d) / V8 cannot be built as specified, and the natural failure mode is a *documented false negative*

The plan specifies: "a fake that also maps on the **GA** index (e.g. CT `AS:i:12`, GA `AS:i:11`) ⇒ `bestDiff 1` ⇒ old **27**, new **11**". With slot order `[CT genome, GA genome]` (`mod.rs:769`, `se_instance_plan` → `Directional => vec![(Norc, Ct, 0), (Nofw, Ga, 0)]`), that fixture yields **44**, not 11.

Why (`merge.rs:254-310`):

```rust
match best_as_so_far {
    None => { best_as_so_far = Some(alignment_score); overwrite = true; … }
    Some(best) => { if alignment_score >= best { overwrite = true; … } }
}
…
} else if overwrite { insert_alignment(…) }
```

`alignments` only ever receives the **running maxima** in stream order. With scores `[12, 11]` the GA record is never inserted → `entries.len() == 1` → `merge.rs:332-335` takes the single-entry arm → `second_for_mapq = b.second_best`, which is `None` for minimap2/rammap (no `ZS:i:`, and in-process rammap sets `second_best: None` at `inprocess.rs:224`/`:277`). So `calc_mapq(6, None, 12, None, …)` → the no-second-best top rung, **44**.

Two further constraints the plan does not mention:

1. **The HashMap key is `"{chromosome}:{pos}"`** (`merge.rs:399`). CT and GA de-convert to the *same* chromosome name (`merge.rs:227-237`), so two instances reporting the same POS collapse to one entry regardless of score.
2. **A cross-instance tie is `Ambiguous`, not a runner-up** (`merge.rs:338-342`) — the later instance must score **strictly** higher.

**The fixture that does work**, with the existing 8 bp `chr1 = ACGTACGT`:

- CT arm: `chr1_CT_converted`, POS 1, `AS:i:11`
- GA arm: `chr1_GA_converted`, **POS 3**, `AS:i:12`

POS ≥ 3 is required, not optional: index 1 prepends two genomic bases and bails out with `extracted: false` when `pos < 2` (`methylation.rs:155-158`). At POS 3 the window is `chr[0..2] + chr[2..8]` = 8 bytes = `read_len + 2`, so the length guard passes and the record is written. `best.index == 1` is not rejected by the directional guard (only 2/3 are, `merge.rs:361`). The oracle then gives exactly the plan's numbers: **old 27 → new 11**.

**Why this is Critical rather than a typo.** The plan's escape hatch — "if that cannot be built reliably, **say so and cover the branch at unit level only**" — combined with a wrong recipe makes the likely outcome: implementer builds cell (d), gets 44, concludes the cross-instance branch is unreachable through a fake, and writes that conclusion into a comment. The result is a green suite in which **the only end-to-end coverage of §1's larger-blast-radius half is absent, with a false justification recorded in the code**. The plan also flags the wrong obstacle (the `read_len + 2` guard) — real, but secondary to the score-ordering rule that actually blocks it.

Also note the ⚠️ in step 7 should say *which* guard: it is the index-1 **prepend** guard (`pos < 2`), not the index-0 trailing guard.

### C2 — IMPORTANT (root cause of C1): "aligns in both instances ⇒ the 33 path" is wrong

§1's table row 2, §3.4's framing and SPIKE F6 all say: *"a read that aligns in only one strand instance takes the 42 path; one that aligns in both takes the 33 path."* Per the `overwrite` gate above, the second-best branch is reached only when a **later**-slot instance **strictly out-scores every earlier one**. For SE-directional (2 instances) that is roughly **half** of the reads that align in both — plus ties are dropped as ambiguous and same-POS pairs collapse. So today's population is: unique-instance reads **and** "CT wins" reads at 42; only "GA strictly wins" reads at 33/25.

Consequences the plan should absorb:

- §1's "the second branch … is the larger behavioural change" is overstated by ~2× and should be re-expressed as a condition, not a population.
- §7 step 9's CHANGELOG requirement ("reads with a cross-instance runner-up move in both directions") is fine, but the plan should not imply that is "every read that maps to both strands".
- Non-directional SE has 4 slots (`mod.rs:777`), so the running-maxima rule bites differently there — worth one sentence, since `--non_directional --minimap2` is a supported shape.

This is not a defect in the fix. It is a defect in the plan's model of the code, and it produced C1.

### I1 — IMPORTANT: V9 does not guard what §3.5 and §9 say it guards, and cannot be written where the plan puts it

Two separate problems.

1. **It cannot compile as specified.** `perfect()` is private to `config.rs` (`config.rs:196`, no `pub`/`pub(crate)`), so a test in `mapq.rs::tests` cannot call it. Either express the invariant through the `pub(crate)` seam — `let (bo, d) = model.normalize(len, None, as_best); assert!(bo <= d);` (equivalent to `AS ≤ perfect`) — or put the test in `config.rs`'s existing `mod tests` (`config.rs:1536`).
2. **The claim about it is false.** §3.5 says `AS > perfect` is "guarded by V9's assertion instead" of a runtime clamp, and V9's expected column says "**If a future minimap2 breaks it, this fails** instead of MAPQ silently re-saturating". A unit test over the spike's hardcoded `(len, AS)` pairs cannot fail because of a future aligner: it never runs an aligner. Nothing in the plan detects a runtime violation.

**Recommendation (cheap, and it turns A1 into a real gate).** minimap2 is **already installed in CI** — `rust_ci.yml:35-42`, `:93-100`, `:131-138` — and `aligner_five_base_groundtruth.rs:55-67` already implements the "skip locally, **panic if `$CI` is set**" pattern so such gates cannot pass vacuously. Port SPIKE §5's one-liner into a test: a perfect read against a small reference under each of `map-ont`/`map-pb`/`sr` must report `AS == 2·len`, and no alignment may report `AS > 2·len`. That costs a few dozen lines, gives A1 a live gate, and — because CI runs Ubuntu's packaged minimap2, not 2.31-r1302 — extends the empirical evidence to a **second version**, which is precisely the spike's stated limitation ("one binary, one version… not pinned in CI").

If that is judged out of scope, then §3.5 and V9 must be restated honestly and A1's residual added to §11's risk list.

### I2 — IMPORTANT: §7 step 2's stale-comment list is incomplete (three more sites, all in `config.rs`)

§11 risk 6 predicts this ("must be re-grepped at implementation time"), so here is the grep. Beyond `end_to_end()`'s doc and `local_ladder()`'s note (covered by step 1) and the four `mapq.rs` sites (step 2), these become **false**:

| site | text | why false |
|---|---|---|
| `config.rs:90-91` | "only Bowtie 2-local has a nonzero perfect score, and only Bowtie 2-local is emitted the `G` form" | first clause false; the second stays true and must be kept |
| `config.rs:98` | "Perfect-alignment score per base; **nonzero only for Bowtie 2 `--local`**" | false — and it is a one-line field doc, i.e. the "reads as authoritative, survives a diff review" class |
| `config.rs:117-119` | "Only Bowtie 2 --local scores matches positively; every other mode's best possible score is 0 — including HISAT2…" | false; sits **immediately above** the edited arm, so it will look like context rather than a claim |

Reproducible check for the implementer:

```
command grep -rn "Bowtie 2 \`--local\` only\|nonzero only for Bowtie 2\|only Bowtie 2-local\|Only Bowtie 2 --local\|sole mode with a nonzero" rust/bismark/src rust/bismark/tests
```

Six hits today; all six must be gone or rewritten.

### I3 — IMPORTANT: `rust/README.md:161` has a **second** false sentence, and a stale gate claim

§7 step 8 names only the minimap2 byte-identity claim. The same row also says:

> "⚠️ **Byte-identity is the END-TO-END contract:** … **End-to-end (every aligner) is unaffected and stays byte-identical.**"

Under D-PARITY that general sentence is false — end-to-end minimap2/rammap is exactly what diverges. It is the more dangerous of the two because it is the *general* statement a reader trusts. (Its twin at `mapq.rs:18-19` *is* covered by step 2 — the README copy is not.)

The same row also asserts "**Phase-5 combined 10M gate: all 13 cells byte-identical** (Bowtie 2 + HISAT2 SE+PE + **minimap2 SE** × {dir, non-dir, pbat} + mouse GRCm39 RRBS)". Those minimap2 cells stop being byte-identical, so that claim needs a "(pre-#1081)" qualification or a restatement too.

### I4 — IMPORTANT: §7 step 8 misreads the docs sentence it is telling the implementer to preserve

`docs/src/content/docs/options/alignment.md:111`:

> "Reported MAPQ values are normalised against that best possible score, so local-mode MAPQ differs from end-to-end MAPQ for the same alignment. **This applies to Bowtie 2 only:** HISAT2 exposes no `--local` option of its own and always scores matches as 0, so with `--hisat2 --local` the best possible score is 0…"

The antecedent of "This" is the **normalisation against a positive best-possible score** — the sentence immediately before — not `--local` mode. The justification that follows is about match bonuses, which confirms the reading. After this fix, "This applies to Bowtie 2 only" is simply **false**: minimap2/rammap are normalised against `2·len` too.

The plan says it "is about `--local` **mode** (still true — minimap2 has no `--local`), not about the ladder". That framing invites the wrong edit: leave the sentence, append a minimap2 note, ship a page that contradicts itself. The sentence must be rewritten. (The plan's *goal* — don't let "no `--local` mode" and "no local ladder" get conflated — is right; only its reading of the current text is wrong.)

### I5 — IMPORTANT: A3/F5's rammap evidence is attributed to the wrong field, and there is one **reachable** additive term on rammap's AS path

`AS:i:` for rammap is **`r.align_score`** (`pipeline.rs:2483`, `:2598`), which is also what `Mapping.score` carries (`api.rs:516`) and therefore what Bismark's in-process backend emits (`inprocess.rs:252,258`). It is **not** the field named `dp_score` that SPIKE F5 traced ("`dp_score` is fed only `ez.max`/`ez.score` (`align/extend.rs`, 4 writes)").

rammap's naming is effectively transposed relative to minimap2:

- rammap `align_score` ≈ minimap2 `dp_score` → printed as `AS:i:`; set from the raw DP accumulation at `pipeline.rs:851` ("*Use raw dp_score from DP segments (for AS tag)*").
- rammap `dp_score` ≈ minimap2 `dp_max` → receives `update_dp_max` (`pipeline.rs:1448-1453`, `:1890`), the splice bonus (`:1771-1777`), and the jump adjustments (`jump.rs:347-352`, `:462-467`).

So the conclusion survives, but for a different reason than the spike gives, and the enumeration missed four files. Two specific consequences:

1. **One additive non-match term is on the AS path and is NOT splice-gated**: `pipeline.rs:1070-1074`

   ```rust
   let align_score = if stripped_gap_len > 0 {
       ez.max + opt.scoring.gap_open + opt.scoring.gap_extend * stripped_gap_len
   } else { ez.max };
   ```

   It looks like "add back a penalty that was definitely subtracted" (which preserves `AS ≤ a·qlen`), but the spike never made that argument because it never looked at this field. This is the one place I could not close the bound by inspection alone. Blast radius if it *is* loose: a handful of reads land at `bestOver > diff` → the local ladder's top rung 44, i.e. today's behaviour, no panic (§3.5 already covers that shape).
2. **The `match_score` citation is for the wrong preset.** A3/F5 cite `api.rs:690,692`, which is the **`sr`** branch. The in-process rammap default hard-codes `Preset::MapOnt` (`mod.rs:968`), and `map-ont` sets only `k`/`w` (`api.rs:617-619`) — so the load-bearing constant for the default path is the struct default `match_score: 2` at **`align/map.rs:215`**. (I verified it: 2. The fact holds; the citation does not support it.)

   For completeness: rammap's splice bonus lands on `dp_score` where minimap2 puts it on `dp_max` (`pipeline.rs:1771-1777` vs `align.c:1092-1094`) — a genuine divergence from minimap2, unreachable from Bismark because it is gated on `AlignFlags::SPLICE` + a jump DB (`pipeline.rs:2113-2121`). Same escape hatch as `junc_bonus`. Worth one line in F5 so a future reader does not re-derive it.

**Recommendation:** restate A3 with the correct field and citation, name the splice gate as the reason the additive sites are unreachable, and either (a) measure once (the in-process crosscheck harness exists and already compares `alignment_score`) or (b) record `pipeline.rs:1070` explicitly as the unquantified residual for rammap only. "✅ Source-verified" currently overstates what the source shows.

### I6 — IMPORTANT: V4 is vacuous under fault injection (iii) if it is wired the obvious way

V10(iii) injects `MINIMAP2_MATCH_BONUS = 1.0` and claims "V1/V4/V6 fail". V1 and V6 do (I checked: at `len 100`, bonus 1.0 gives `diff = 120` → 44/44/44/44 instead of 44/44/41/28/22; at `len 6` cell (b) gives 44 instead of 28). **V4 does not, if the generalized `bowtie2_local_reference` is handed `MINIMAP2_MATCH_BONUS`** — the injection propagates into both sides and cancels. The reference must take a **literal `2.0`** for the minimap-like cells, exactly as it takes literal `20.0, 8.0` today (`mapq.rs:466-468`).

Related: the reference calls `calc_mapq_local` unconditionally (`mapq.rs:449`). Once it is parameterized by `match_bonus`, passing `0.0` would silently compare an end-to-end model against the local ladder. Assert `match_bonus > 0.0` in the reference, or keep the ladder selection derived inside it.

This is the #1079 12b lesson landing a second time: the plan quotes it ("a `match_bonus` injection missed a wrong-*form* bug entirely") and then leaves the *constant-value* injection with a similar hole.

### I7 — IMPORTANT: §5 miscounts the MAPQ assertions, and V11's frozen list omits the two that matter most

§5 says "the 4 MAPQ assertions in the file are Bowtie 2 (L165, L241, L602) and HISAT2 PE (L2630)". There are at least **seven** in `tests/aligner_cli.rs`: `165`, `241`, `602`, **`885`**, **`886`** (Bowtie 2 PE, 42/42), **`2269`**, **`2367`**, `2630`.

The load-bearing claim ("no minimap2/rammap MAPQ assertion exists anywhere") **holds** — all seven are Bowtie 2/HISAT2. But two uncounted ones are the #1080 **BAM-level** ladder gates:

- `hisat2_local_softclip_roundtrip_and_options` (`:2214`) — asserts 42 with the message *"44 means the local ladder is still selected (its ceiling)"* (`:2269`)
- `hisat2_local_pe_softclip_roundtrip` (`:2305`) — asserts 38 with *"39 means the local one"* (`:2367`)

These are the only end-to-end checks that a mis-scoped `match` arm has not leaked the local ladder to HISAT2. V11 names unit tests plus the two Bowtie 2 `aligner_cli` tests but not these. Add them by name.

### I8 — IMPORTANT: two §3.5 edge-case rows are derived only for the default `--score_min`

- **`diff` clamp row**: "Reachable only for `perfect − scMin ∈ [0,1)`, i.e. `2·len + 0.2·len < 1` ⇒ `len = 0`. **Effectively dead here**." That derivation hardcodes `(0, −0.2)`. `--score_min` is shape-validated only (`options.rs:410-414`) and `score_min_params` returns whatever the user passed (`options.rs:376-404`), so `--score_min L,100,-0.2 --minimap2` gives `scMin = +98.8` at len 6 → `perfect − scMin = −86.8` → the clamp **fires**, `bestOver` goes negative, and every read floors at 22. The clamp is live for minimap-like, not dead.
- **`len = 0` row**: "`scMin = 0`" also assumes intercept 0.

This matters because A6 makes non-default `--score_min` an explicit, legal, MAPQ-moving input for minimap2, yet **no cell in §9 exercises it for the minimap-like model** — every listed cell uses `(0, −0.2)`. The frozen sweeps already have the pattern (`SCORE_MIN_CELLS`, `mapq.rs:233-240`). Add at least one non-default cell (a positive intercept is the interesting one, since it is the only way `perfect − scMin` can approach the clamp). Note the shared-denominator fault I looked for here — `perfect + |scMin|` substituted for `perfect − scMin` — *is* caught, but only by the pre-existing Bowtie 2-local reference at `(20.0, 8.0)` where `scMin > 0`; nothing in the new gates would catch it.

### I9 — IMPORTANT: §9 has no row for the documentation deliverables

Steps 2, 8 and 9 are the deliverables that both precedent reviews actually caught defects in (#1079 M5/E4 = the README row not restated; #1080 M2 = a competing CHANGELOG bullet). §9 gates the code and gates nothing else. Add a V13 that is run and recorded at implementation time — e.g. the I2 grep returning zero hits, plus "`CHANGELOG.md` `## Unreleased` → `### bismark (aligner)` contains exactly one #1081 bullet and the placeholder is gone", plus a `command grep -c "byte-identical to Perl v0.25.1 + minimap2" rust/README.md` check. Cheap, and it is the one class of miss this plan lineage has repeated twice.

---

## 2. Assumptions

| # | Verdict |
|---|---|
| **A1** | ✅ **Independently confirmed for minimap2** (see §0). Structural argument is stronger than the spike states — the block partition + the z-drop single-add are the missing steps and they hold. Guard claim is wrong (I1). |
| **A2** | ✅ Confirmed: `minimap2_options` emits a closed string (`options.rs:288`), three preset selectors only, no `-A`/`-x` passthrough, no `allow_hyphen_values`. |
| **A3** | ⚠️ **Weaker than stated, and its evidence is misattributed** (I5). Direction-of-error hedge is correct. |
| **A4** | 🟠 Fairly stated: decided ≠ verified. The `monotone` analogy argument is the only rule in play, and applying it consistently with #1080 is a real (if not conclusive) reason. No objection. |
| **A5** | ⚠️ Correctly promoted to top residual. One refinement: the old values are **not** uniformly degenerate — the 33/25 branch already carries a little information (see C2), so "a constant carries no information" understates what a byte-comparing consumer could have depended on. Minor. |
| **A6** | ⚠️ Confirmed and load-bearing: `options.rs:233` discards the base string; `score_min_params` (`options.rs:376-381`) still returns `(0.0, −0.2)`. See I8 for the edge-case rows this invalidates. |
| **A7** | ✅ Confirmed (`mod.rs:1319`, `:1662`, `:2060`). |
| **A8** | ✅ Both ladders untouched; I re-checked every leaf of `calc_mapq_local` against Perl `4082-4178` while building the oracle — they agree. |
| **A9** | ✅ For the primary record. Worth noting *why* hard clips never appear: only the **first** record per read per instance is considered (`merge.rs:312-315` discards the rest), and minimap2/rammap emit the primary first — which is also why `parse_cigar`'s rejection of `H` (`methylation.rs:192-197`) never fires. Pre-existing, unaffected, but it is the mechanism A9 actually rests on. |
| **A10** | ✅ Inherited non-goal. |
| **A11** | ✅ Confirmed — no oracle exists; the honest framing in §1 ("not agreement with minimap2") is the right call and the CHANGELOG requirement enforces it. |

Two assumptions the plan makes implicitly and should state:

- **A12 (implicit): the SE merge's runner-up selection is order-dependent.** Made explicit by C1/C2. It is the difference between a buildable and an unbuildable test.
- **A13 (implicit): MAPQ never exceeds `u8`.** True (ladder max 44, `MappingQuality::new` at `output.rs:478`), but if A1 were ever violated the ratio only saturates the rung — worth one clause so a reader does not wonder about overflow.

---

## 3. Efficiency

Nothing to add. One extra `match` arm at resolve time (once per run); `normalize`'s multiply/compare per accepted alignment already exists from #1079; `ScoreModel` stays `Copy` and the same size (no field added — D-LADDER's "no code change" makes this literally true). §11's efficiency paragraph is accurate.

---

## 4. Validation sufficiency — the fault I was asked to construct

The best "ships green through every §9 gate" fault is **C1's own failure mode**, and it is not a code fault at all:

> the implementer builds cell (d) as written → gets 44 → invokes the plan's own escape hatch → records "the cross-instance branch cannot be built through a fake" → covers it with a unit test that passes `as_second` explicitly.

Result: every gate green; `second_for_mapq → calc_mapq` is **never exercised for a minimap-like model anywhere**; and the code carries a comment asserting something false. That is worse than an untested branch, because it inoculates the next reviewer.

Runner-up faults, ranked:

1. **`MINIMAP2_MATCH_BONUS = 1.0` survives V4** if the reference is passed the constant (I6). V1/V6 still fail, so the fault is caught — but V10's stated teeth are wrong, and the *reason* V4 exists (independence) is defeated.
2. **A wrong `ScoreMinForm` inside `minimap_like`** (`Log`, copied from `bowtie2_local`) is caught by V5's `== minimap_like(0.0,−0.2)` **only because** the matrix compares against `from_emitted(…, Linear, …)`. Keep that comparison exactly as specified — it is the whole gate on the #1079 wrong-form class.
3. **Conflating the two 2.0 constants** ships green through everything and always will (O2).
4. **A clamp/denominator-shape fault for minimap-like under a non-default `--score_min`** is caught only incidentally, by a Bowtie 2 test (I8).
5. **Every doc/comment deliverable** is ungated (I9).

Gates I judge adequate as written: V5 (including the `local`-irrelevance assertion — that is a genuinely good catch, since `--local` is rejected upstream and would otherwise be untestable), V6/V7 cells (a)(b)(c) with their off-boundary margins (I re-checked: 0.92 above the `0.4` rung, 0.4 below `0.5` — comfortable against any f64 wobble), V11, V12.

---

## 5. Alternatives

Nothing to reopen. For the record:

- **The `end_to_end` → `monotone` rename** (§10 Open): agree with the plan's own answer — keep the name, fix the doc, rely on `assert_ne!`. With I2 fixed, the doc surface is coherent without a 29-site churn.
- **`--local`-style gating for minimap-like** was never on the table (rejected upstream) and V5's assertion is the right way to pin it.
- **A runtime guard for `AS > perfect`.** §3.5 rejects a *clamp* — correct, a clamp hides the regression. But a non-mutating never-silent warning is not a clamp, and it is the only thing that would actually detect a future aligner change at runtime. If the CI gate in I1 is adopted, skip this; if not, it is the fallback.

---

## 6. Action items

### Critical

1. **Rewrite §7 step 7 cell (d) / V8 with a buildable fixture.** CT arm `AS:i:11` at `chr1_CT_converted:1`; GA arm `AS:i:12` at `chr1_GA_converted:`**`3`**. State the three constraints explicitly — later slot must score **strictly** higher (`merge.rs:254-310`), the `chromosome:position` keys must differ (`merge.rs:399`), and index 1 needs `POS ≥ 3` (`methylation.rs:155-158`). Predicted values are unchanged (old **27** → new **11**, oracle-confirmed). Keep the escape hatch, but make it conditional on the *corrected* recipe failing, and require the written-down reason to name the mechanism.

### Important

2. **Correct C2 everywhere it appears** (§1 table row 2, §3.4, SPIKE F6, §7 step 9's CHANGELOG wording): the second-best branch is reached when a later slot strictly out-scores every earlier one — not whenever a read aligns in both instances. Add a sentence for the 4-slot non-directional case.
3. **Fix V9** (I1): express it through `normalize` (`bestOver <= diff`) or move it into `config.rs::tests`; and either add the real-aligner CI gate (minimap2 is already installed — `rust_ci.yml:35-42`; reuse `aligner_five_base_groundtruth.rs:55-67`'s skip-or-panic pattern) or restate §3.5/V9 honestly and add A1's residual to §11.
4. **Extend §7 step 2 to the three `config.rs` sites** (`:90-91`, `:98`, `:117-119`) and record the grep (I2).
5. **§7 step 8: restate the README row's second false sentence** ("End-to-end (every aligner) is unaffected and stays byte-identical") and qualify the "all 13 cells byte-identical" gate claim (I3).
6. **§7 step 8: correct the reading of `alignment.md:111`** — "This applies to Bowtie 2 only" is about the normalisation, not `--local` mode, and must be rewritten rather than preserved (I4).
7. **Restate A3 / SPIKE F5 with the correct field and citations** (`align_score` at `pipeline.rs:2483`/`api.rs:516`; `align/map.rs:215` for the default `match_score`), name the splice gate, and record `pipeline.rs:1070-1074` as rammap's one reachable additive term (I5).
8. **V4 must take a literal `2.0`**, and V10(iii)'s "V4 fails" claim depends on it (I6).
9. **§5: correct the MAPQ-assertion count (≥7, not 4) and add `hisat2_local_softclip_roundtrip_and_options` + `hisat2_local_pe_softclip_roundtrip` to V11's frozen list** (I7).
10. **§3.5: fix the clamp and `len = 0` rows for non-default `--score_min`, and add one non-default `--score_min` cell to the minimap-like gates** (I8).
11. **Add a V13 for the documentation deliverables** (I9).

### Optional

12. **V10(iv) is under-specified**: the production construction site is `from_emitted` (`config.rs:796`), so "use `end_to_end()` there" would break Bowtie 2-local and fail V11 rather than isolate V5. Replace with `minimap_like` built with `ScoreMinForm::Log` (the #1079 wrong-form class), and add a fifth injection: drop `| Aligner::Rammap` → V7 fails.
13. **Say that nothing distinguishes the two 2.0 constants.** The "can drift independently" rationale is right, but it is doc-only — no gate tests it, and V10 cannot.
14. **State that `--ambig_bam` is unaffected** (`output.rs:819`, `:833` copy MAPQ from field 4 of the raw line). It is an output surface a reader will assume moves, and it means a `--minimap2 --ambig_bam` run legitimately emits two MAPQ scales in two files.
15. **Note in the filed rammap-preset issue that this fix slightly worsens it**: `--rammap --mm2_short_reads` already silently runs `map-ont` in-process vs `sr` in the subprocess; after this change that silently-ignored flag also moves the MAPQ column. §1 currently frames the bug as only *reducing* exposure — true for `end_bonus`, not for backend concordance.
16. **Cosmetic:** the `--local` reject is `config.rs:673` + `:677-684` (the plan cites `:674`, a comment line). SPIKE F5's `sc_ambi` claim holds by two mechanisms — the matrix build *and* the inline `opt->sc_ambi > 0 ? -opt->sc_ambi : opt->sc_ambi` in the reachable ungapped-`sr` path (`align.c:825`); either way `sc_ambi ≤ 1 < a`.

---

## 7. Method notes (so these findings can be re-checked)

- Upstream minimap2 `master` (`align.c`, `format.c`, `options.c`) fetched to the session scratchpad; all `dp_score`/`dp_max` writes enumerated by grep and read in context.
- Perl oracle: `bismark:3923-4186` extracted verbatim; only two substitutions (`$scMin` and `$diff` read from globals; the PE accumulation removed for an SE-only driver). Both ladders driven unmodified. Scratch only — nothing written into the repo.
- rammap read at the pinned checkout `~/.cargo/git/checkouts/rammap-aa480586e5eab817/5ea62cd` (`rammap-core/src/{api.rs, align/{pipeline,extend,jump,map,align_simple}.rs}`).
- `PLAN.md`, `SPIKE.md`, `PROGRESS.md` and all source files left untouched.

# PLAN REVIEW B — Local-mode MAPQ denominator (#1079)

**Reviewer:** B (independent, fresh context)
**Plan:** `/Users/fkrueger/Github/Bismark/plans/07292026_local-mapq-denominator/PLAN.md`
**Date:** 2026-07-29
**Verdict:** the *diagnosis* is correct and well-sourced; the *fix design* has one provably-wrong invariance claim (C1) and one unachievable validation gate (C2), plus a step-ordering contradiction (I1) and a materially incomplete site enumeration (I2). Recommend a rev-1 before implementation.

Everything below was verified against the actual source and against upstream Bowtie 2 v2.5.5 — not taken from the plan.

---

## 0. What I verified as CORRECT

Worth stating plainly, because most of the plan holds up:

| Claim | Status |
|---|---|
| A1 — Bowtie 2-local perfect = `2 × len`, summed per mate | ✅ `scoring.h:310-316`: `perfectScore(rdlen) = monotone ? 0 : rdlen * match(30)`; `unique.h:207-209` sums `perfectScore(ordlen)` for pairs. `match(30) = 2` under Bismark's always-on `--ignore-quals` / constant cost model. |
| A2 — `G` form is logarithmic; Bowtie 2-local `ln()` scMin is **correct** and must stay | ✅ `simple_func.h:88-107`: `SIMPLE_FUNC_LOG` ⇒ `X = log(x)`, `ret = C + L*X`. The plan's catch here is real and important. |
| A4 — `--ma` never reaches the aligner | ✅ Verified structurally, not just by grep: `Cli` (`cli.rs`) has **no** free-form aligner-option vector (`positional: Vec<String>` is genome/reads), no `trailing_var_arg`/`allow_hyphen_values`/external-subcommand escape, and `options.rs` emits exactly 23 curated `opts.push` sites — none is `--ma`. `2.0` is safe as a constant. |
| A5 — both ladders' thresholds/returns are correct | ✅ Bismark's `calc_mapq_local` (`mapq.rs:143-220`) is byte-for-byte Bowtie 2's non-monotone ladder (`unique.h:333-380`), including the uniform `diff*0.5` sub-threshold. End-to-end ladder matches `unique.h:223-332`. |
| A7 — no `--local` cell in the perl-oracle CI gate | ✅ …and **stronger than the plan says** — see I5. |
| Read length == the length Bowtie 2 scored | ✅ No `--trim5`/`--trim3`/`-5`/`-3`/`--clip_r*` anywhere in `options.rs`/`cli.rs`; `sequence` at all four call sites is the original uc read; soft-clipped records retain full SEQ (asserted at `aligner_cli.rs:2073`); Bowtie 2 also passes the **full** `rdlen` to `mapq()`, not the unclipped length. Nothing to fix here. |
| minimap2/rammap + `--local` unreachable | ✅ `config.rs:517-529`. |
| `--local` + `--combined_index` unreachable | ✅ `config.rs:531-537` rejects **all four** variants — see I2c, this is stronger than the plan's edge-case row implies. |
| §1 impact table arithmetic (len 250) and V3's expected `24` | ✅ Recomputed independently: 42/36/24/22 vs 44/44/44/44, and len 100 / AS 100 → 24. |

---

## 1. Logic review

### C1 (Critical) — "end-to-end is byte-identical **by construction**" is false, and D-EDGE is scoped to the wrong variable

§7 asserts invariance "by construction (`perfect = 0`)"; §3 narrows the risk to `len < 5`. Both rest on the default `--score_min L,0,-0.2`. The actual identity is:

```
max(1, 0 − scMin) ≡ abs(scMin)   ⟺   scMin ≤ −1
```

Two independent ways that fails, **neither** of which is a `len` question:

**(a) sign.** `abs(scMin)` and `−scMin` diverge for *every* `scMin > 0`. `--score_min` is shape-validated only (`valid_score_min_l` checks non-empty fields, mirroring Perl's permissive `^L,(.+),(.+)$`), so a positive intercept parses fine.

**(b) magnitude.** The `len ≥ 5` threshold is `1/|slope|`. A user slope of `-0.05` pushes it to `len < 20`; `-0.001` to `len < 1000`.

Measured (frozen ladder vs. plan's proposed universal clamp, end-to-end, AS = 0):

```
--score_min      len   scMin    old diff  new diff   MAPQ old → new
L,0,-0.2           1   -0.20      0.20      1.00       42 →  0
L,0,-0.2           2   -0.40      0.40      1.00       42 →  8
L,0,-0.2           3   -0.60      0.60      1.00       42 → 24
L,0,-0.2           4   -0.80      0.80      1.00       42 → 42   (same value, different diff)
L,0,-0.05         10   -0.50      0.50      1.00       42 → 23
L,10,-0.2         50    0.00      0.00      1.00       42 →  0
L,0,0             50    0.00      0.00      1.00       42 →  0
```

So the plan's D-EDGE "Preferred: keep the clamp universal **if** V1 proves no end-to-end divergence over `len ∈ 1..=500`" is a trap: **V1 as specified sweeps `len × as_best × second-best` but has no `--score_min` dimension**, so it will come back green on the default params and green-light a change that silently alters end-to-end MAPQ for any user with a non-default `--score_min`. methylseq emits `--score_min L,0,-N` (`aligner_methylseq_conformance.rs:15,88-96`), so non-default score-min is a live production configuration.

Compounding it: with an intercept-only user override the divergence is not a rounding wobble — `L,10,-0.2` at 50 bp is **42 → 0**, i.e. every read in the run drops below a `-q 20` filter.

**Required:**
1. **Pre-decide D-EDGE = clamp is NOT universal.** Do not leave it to V1.
2. Gate the whole new denominator on `match_bonus > 0.0` (equivalently a `perfect_is_zero` flag), *not* on `local`:
   ```rust
   let diff = if self.match_bonus > 0.0 {
       (self.match_bonus * total_len as f64 - sc_min).max(1.0)   // Bowtie 2-local
   } else {
       sc_min.abs()                                              // frozen: all e2e + HISAT2-local
   };
   ```
   This is strictly better than the plan's "clamp only when `local`" fallback, because it also keeps **HISAT2-local** clamp-free. HISAT2-local values already move under D2 (scMin form); gating on `local` would move them for a *second*, unrelated reason (the clamp) in the same commit, and HISAT2's perfect score is admittedly unknown (A3) — so `abs()` is the honest placeholder there, not `max(1, −scMin)`.
3. Extend V1 with a `--score_min` dimension: at minimum `{(0,−0.2), (0,−0.05), (0,−0.6), (10,−0.2), (0,0), (−1,−0.2)}` × `len ∈ 1..=500`, both SE and PE, both ladders.

**Bonus argument for (2):** the clamp is nearly dead code in Bowtie 2-local anyway. A *reported* alignment satisfies `AS ≥ minsc` and `AS ≤ perfect`, hence `perfect − scMin ≥ 0`; only the `[0,1)` sliver clamps. And with `G,20,8` reads below ~23 bp have `perfect < scMin` outright (len 22 → perfect 44, scMin 44.73), so Bowtie 2 cannot report them at all. The clamp's only *behaviour-changing* reach is end-to-end — the exact opposite of its intent.

### C2 (Critical) — V2 and V8 cannot pass as written: Bowtie 2's `scMin`/`diff`/`bestOver` are **integers**

The plan quotes `unique.h:207-217` verbatim but doesn't register that `TAlScore` is `int64_t`:

```cpp
TAlScore scMin = scoreMin_.f<TAlScore>((float)rdlen);   // simple_func.h:106 → return (T)ret  ⇒ TRUNCATION
TAlScore diff  = std::max<TAlScore>(1, scPer - scMin);
TAlScore bestOver = best - scMin;
```

Bismark (Perl and Rust) keeps `scMin` as `f64`. For `G,20,8` the log form is essentially never integral, so a **faithful** transcription of `unique.h` disagrees with `calc_mapq` at every bucket boundary. Concrete, verified:

```
Bowtie 2-local, len 100, G,20,8, AS = 128, no second best
  Bismark (f64): scMin 56.8414  diff 143.1586  bestOver 71.1586  ratio 0.4972 → MAPQ 28
  Bowtie 2 (i64): scMin 56      diff 144       bestOver 72       ratio 0.5000 → MAPQ 36
```

Consequences:
- **V2** ("Transcribe `unique.h:206-222` … Expected: **Agreement across the grid**") will fail on a large fraction of cells. The reference fn must be the deliberate **f64 analogue** of `unique.h`, with a comment saying so; otherwise the implementer either weakens the grid until it passes (destroying the gate's value) or "fixes" `calc_mapq` by truncating.
- **Truncating is not an option** and the plan should say so explicitly: `mapq.rs:259-262` (`non_integer_scmin`) pins end-to-end `len 51 → scMin −10.2`. Adopting int semantics would change end-to-end MAPQ — the one thing that must not move.
- **V8**'s expected result ("Agreement") is therefore wrong even for a perfectly restricted comparison set. V8 must be reframed: *"expect agreement except where int-vs-float truncation moves a bucket; every disagreement must be explained by `floor(scMin)`, and the rate quantified."* As written, V8 will read as a failure and burn a debugging cycle on a non-bug.

Add int-truncation as an **explicit, documented non-goal** in §1, next to the HISAT2 perfect-score deferral. It is the same class of decision and it will otherwise resurface in every future MAPQ review.

### I1 (Important) — step 2 is not a no-op: **D2 lands inside it**, so V7 cannot hold

Step 1 defines the constructor with `log_score_min = local && aligner == Bowtie2`. Step 2 then claims "Refactor-only, zero behaviour change" and V7 claims "All green with **zero test edits**". But step 4 itself admits D2 "falls out of step 1 automatically" — so HISAT2-local's scMin flips from `ln` to linear *during* the supposedly-inert step. Recomputed against `mapq.rs:390-419`, **4 of the 6 hand-computed HISAT2-local expectations change**:

| Assertion (mapq.rs) | current expect | after D2 |
|---|---|---|
| `:396` `calc_mapq(50, None, 0, None)` | 44 | 44 (same) |
| `:397` `calc_mapq(50, None, -1, None)` | 22 | **44** |
| `:398` `calc_mapq(150, None, 0, None)` | 44 | 44 (same) |
| `:401` `calc_mapq(50, None, 0, Some(-1))` | 40 | **31** |
| `:402` `calc_mapq(50, None, -1, Some(-1))` | 0 | **1** |
| `:410` `calc_mapq(150, Some(150), 0, Some(-1))` | 34 | **11** |

Step 5 only names the two tautologies (`:376`, `:416`) and step 4 only says "add a targeted test" — the plan never says these four must be **recomputed**. An implementer hitting four "unexpected" failures inside a step advertised as a no-op is being set up to re-baseline them, which is exactly the failure mode step 5 exists to prevent.

**Fix the ordering:** step 2's constructor sets `log_score_min = local` (verbatim current behaviour, no aligner term); D2 becomes its own step *after* D1, flipping to `local && aligner == Bowtie2` **and** recomputing those four expectations with the new values shown above.

Also restate V7 honestly: the `calc_mapq` signature change forces edits at **~28 unit call sites** in `mapq.rs` plus `combined.rs:1205`, so "zero test edits" is impossible by construction. The gate you want is **"no expected-*value* edits — call-shape edits only"**. Leaving it as "zero test edits" invites the implementer to conclude the gate is unachievable and skip it.

### I2 (Important) — site enumeration is incomplete; §11's "traced all three call sites" is wrong

Verified counts:

**`calc_mapq` call sites — 4, not 3.** `merge.rs:370` (SE), `merge.rs:747` (PE), `combined.rs:349` (SE combined), **`combined.rs:774` (PE combined — never named)**. Plus the test call at `combined.rs:1205`. All four do carry the read lengths, so the plan's conclusion survives — but the missing one is a *PE* site, precisely where a single-mate perfect-score bug would hide.

**Trio-carrying signatures in `combined.rs` — 8, not 6.** Plan lists 188, 246, 388, 437, 501, 543. Missing: **`select_pe_pbat` (596-598)** and **`select_core_pe` (655-657)**.

**`mod.rs` is absent from the plan's Files table entirely.** It holds:
- **6 config-expansion call sites**: `2599`, `3146`, `3528`, `4647`, `5255`, `5705`.
- **2 function-pointer type aliases that hardcode the trio** — `SelectFn` (`mod.rs:2751-2758`) and `SelectFnPe` (`mod.rs:2767-2775`), both literally `fn(…, f64, f64, bool, &mut Counters) -> …`. These *must* change in lockstep with all eight `combined.rs` selectors or the refactor won't compile, and they are the least obvious edit in the whole change.

Corrected blast radius: **10 fn signatures + 2 fn-pointer aliases + 6 config-expansion sites + 4 call sites + `RunConfig` struct/literal + ~29 test call sites.** The plan's "~10 signatures / 3+ call sites" understates it by roughly half.

**I2c — the combined-index edge-case row is misleading.** "Non-directional / combined index — all `combined.rs` call sites take the same `ScoreModel`" reads as though combined + local is a live configuration. `config.rs:531-537` rejects `--local` with **all four** `--combined_index` variants, so `combined.rs` can only ever see `match_bonus = 0`. Say that: all 8 combined sites are **refactor-only** for this fix, which is a meaningful risk reduction and should be stated as such rather than left ambiguous.

### I3 (Important) — the plan drops the reporter's suggested case #2 (perfect local alignment)

Issue #1079 suggests three regression cases. V3 covers #1, V4 covers #3, and **#2 ("Bowtie2 local perfect alignment: expected top local MAPQ") has no counterpart in §9.** This is not decoration: under the new formula `best_over == diff ⟺ as_best == perfect`, so the exact-equality leaves (`35/34/33/32/31` in the local ladder, `mapq.rs:174,182,190,198,206`) become reachable-and-meaningful for the first time. Verified: len 100, AS 200 → `best_over == diff == 143.1586` exactly (same f64 subtraction on both sides) → 44 with no second best; with a second best it lands on the `==diff` leaves. Add it, with and without a second best.

### I4 (Important) — no wiring validation: every proposed gate is a pure unit test

V1–V7 all exercise `calc_mapq`/`ScoreModel` directly. Not one of them would catch a `config::resolve` wiring bug — e.g. `ScoreModel::new(i, s, false, aligner)` (hardcoded `local`), or the intercept/slope swapped in the new 4-positional constructor. The unit suite would be fully green and every production `--local` BAM wrong.

There is a ready-made harness: `aligner_cli.rs` already has `make_fake_bowtie2_mapped` and asserts MAPQ out of a real BAM (`aligner_cli.rs:422`). Add one `--bowtie2 --local` integration test asserting a MAPQ **value** read back from the BAM.

**Critical detail for whoever writes it:** the existing 8 bp genome / 6 bp read fixture is useless for this. At len 6, `G,20,8` gives `scMin = 34.33` and `perfect = 12`, so raw `diff = −22.33` → clamped/negative either way and old and new both return **22**. The fixture must use a read long enough that `2·len > scMin`, i.e. **len ≥ 23** (len 25 → scMin 45.75, perfect 50). Without this note the test gets written against the existing fixture, passes, and discriminates nothing.

### I5 (Important) — A7 is true but understates the exposure; there is **no aligner Perl-oracle in CI at all**

Verified the 13 cells in `.github/workflows/rust_ci.yml:194-217`: 7 are genome-prep (`genome_prep_integration.rs`), 4 methylation_consistency, 1 extractor, 1 dedup. **None is the aligner.** So it isn't only local mode that's unguarded — end-to-end aligner MAPQ has no Perl oracle in CI either.

The total automated protection for the plan's "critical gate" is therefore: V1 (a unit sweep the plan under-specifies, per C1) plus **four** MAPQ value assertions across all aligner integration tests (`aligner_cli.rs:422`, `705`, `706`, `2429`) — all end-to-end, all 6 bp, all default `--score_min`. That is thin enough that C1's fix (adding the `--score_min` dimension to V1) is load-bearing, not nice-to-have. §7 should say this plainly instead of implying the absence of a `--local` oracle is the only gap.

### I6 (Important) — D2 silently destroys the `ln()`-boundary regression coverage, and the plan doesn't replace it

`mapq.rs:386-389` and `:403-409` document that test's purpose explicitly: it is the *only* coverage of the sub-unity-`diff`, `ln()`-derived-bucket-boundary regime, deliberately added because "the `(20,8)` tests (diff=10/integer boundaries) never reach" it. After D2, HISAT2-local is linear, so **no configuration produces a sub-unity `diff` in local mode any more** and that regime becomes unreachable. The plan retires the tautology at `:416` and recomputes nothing else — the ln()-ULP guard just evaporates.

Replace it with a **Bowtie 2-local** ln()-boundary case (that's now the only ln() consumer): pick a `len` where `diff·0.5` or `diff·0.3` sits close to an integer `best_over`, and pin it with the same "why this cell" comment style.

### I7 (Important) — step 7's version bump is incomplete and will fail `cargo test`

"Bump `rust/VERSION` (3.1.0 → 3.2.0)" touches one of three lockstepped locations. Two guard tests enforce the others:
- `rust/bismark/VERSION` (vendored) — `meta/mod.rs:51` `vendored_version_matches_repo_version`
- `rust/bismark/Cargo.toml:3` `version = "3.1.0"` — `meta/mod.rs:73` `cargo_pkg_version_matches_suite_version`, which the comment marks "**load-bearing for a GA cut**" and never skips

All three must move together. (On the 3.1.0 → 3.2.0 question in §10: minor is right — a behaviour change in a non-default mode is not a patch.)

### I8 (Important) — single-source the score-min *form* so D2 cannot regress

D2 exists because two independent expressions decide the same thing: `options.rs:82` (`cli.local && aligner == Bowtie2` → emit `G`) and `mapq.rs:31` (`local` → use `ln`). The plan fixes the *values* but keeps two independent expressions — `options.rs:82` and the new `log_score_min = local && aligner == Bowtie2`. That is the same latent defect, relocated.

Have `score_min_params` (`options.rs:371`) return the form it actually selected — it already computes exactly that at `options.rs:372` (`prefix = "G," | "L,"`) — and build `ScoreModel.log_score_min` from that return value. Then the emitted option and the MAPQ form are the *same* fact by construction, and a future `S,`/`C,` form or a `--ma` passthrough can't desynchronize them again. This is a small change that closes the class of bug, not just the instance.

### Minor logic notes

- **§7 "no internal consumer reads MAPQ" is imprecise.** `--five_base_min_mapq` filters on MAPQ at `mod.rs:2052`. The conclusion still holds (5-Base is minimap2-only, `--local` rejected there, and 5-Base MAPQ comes from minimap2 — `calc_mapq` is never called on that path: no `config.score_min_*` site exists in the 5-Base code). Qualify the sentence rather than deleting it.
- **`len = 0`.** The plan says "assert current behaviour is preserved". Under the C1 recommendation (`abs()` for `match_bonus == 0`) end-to-end `len = 0` is genuinely unchanged. For Bowtie 2-local, `ln(0) = −inf` ⇒ `scMin = −inf` ⇒ `diff = +inf` ⇒ `best_over = +inf` ⇒ `inf >= inf*0.8` is **true** ⇒ 44 (previously `diff = abs(−inf) = inf`, same answer). So it happens to be invariant — but state the reasoning, don't just assert preservation.
- **PE totals.** `perfect = match_bonus × (len1 + len2)` matches Bowtie 2's per-mate sum exactly (`2·l1 + 2·l2`, no per-mate truncation issue since `2·len` is integral). ✅

---

## 2. Assumptions

| # | Verdict |
|---|---|
| A1 | ✅ Confirmed at `scoring.h:310-316` + `unique.h:207-209`. |
| A2 | ✅ Confirmed at `simple_func.h:88-107`. Good catch by the plan. |
| A3 | ⚠️ Defensible but probably over-cautious. HISAT2 has **no `--local` mode** — Bismark's "HISAT2-local" is HISAT2's normal soft-clip-permitting mode with `--no-softclip` dropped (`options.rs:341-346`), and its `--ma` is documented as unused outside a local mode HISAT2 doesn't have. So `perfect = 0` is very likely *correct*, not merely provisional. Keep the deferral (it's Felix's call and needs backend confirmation), but the separate issue should record this reasoning so it isn't re-derived from scratch. |
| A4 | ✅ Verified structurally (see §0). |
| A5 | ✅ Verified against `unique.h:223-380`. |
| A6 | ✅ `bestOver = best − scMin` matches `unique.h:222`; correctly identified as must-not-change. |
| A7 | ✅ True, but understated — see I5. |

**Unstated assumptions the plan should surface:**

1. **`--score_min` is always the default.** Load-bearing for the entire end-to-end invariance argument (C1) and nowhere stated.
2. **`scMin`/`diff`/`bestOver` stay `f64` while Bowtie 2 uses `int64`.** The single biggest unstated assumption; it determines whether V2/V8 can pass (C2).
3. **Read length == the length Bowtie 2 scored.** Stated implicitly via "each call site already has the read lengths". It is true (verified in §0), but it is the assumption on which `perfect = 2 × len` rests and deserves to be A8 with its evidence.
4. **`Aligner` in the constructor introduces a module cycle.** `Aligner` is defined in **`config.rs:21`**, not in `aligner.rs`. Putting `ScoreModel` in `mapq.rs` makes `mapq → config` and `config → mapq`. Rust permits this, but see Alternatives.
5. **`--local` is unreachable on every combined-index path.** True (`config.rs:531-537`) and materially de-risking; state it as a fact rather than leaving the edge-case row ambiguous.

---

## 3. Efficiency

Correct in substance: O(1) added arithmetic per accepted alignment, no allocation, net arity reduction of 2 at ten sites. Two nits:

- **Size.** `3 × f64 + 2 × bool` = **32 bytes** (24 + 2, padded to 32), not 40.
- **`Copy` but "passed by reference"** is self-inconsistent. For 32 bytes, **by value** is at least as fast and avoids introducing an elided lifetime into the two `fn`-pointer aliases (`mod.rs:2751`, `2767`) — which the plan hasn't accounted for at all (I2). Pass by value.

The real efficiency question the plan doesn't ask is **change-cost**, not run-cost: per I2 the true blast radius is ~50 edit sites in a module with no CI Perl-oracle (I5). See Alternatives for splitting it.

---

## 4. Alternatives

**A. Gate the denominator on `match_bonus > 0`, not on `local`.** ← recommended, see C1. Strictly narrower than either D-EDGE branch: only Bowtie 2-local's `diff` changes; end-to-end *and* HISAT2-local keep `abs()` verbatim. Also means HISAT2-local moves for exactly one reason (D2's scMin form), which is the honest scope given A3.

**B. Land the refactor as its own PR.** The plan's step-2/step-3 split is the right instinct; given I5 (no aligner CI oracle) and I2 (~50 sites), consider making the boundary a **PR** boundary, not just a commit boundary. PR 1 = pure `ScoreModel` refactor, `diff = sc_min.abs()`, zero expected-value changes, reviewable as mechanical. PR 2 = D1 + D2, small diff, all attention on the semantics. Also gives a clean revert target if a full-scale oxy run surprises.

**C. Skip the struct; pass a single `perfect_per_base: f64`.** Minimal-diff alternative: no arity reduction, doesn't fix D2, keeps the `too_many_arguments` debt. Mention and reject — the struct is better and it's what the reporter asked for. Not worth reopening.

**D. Derive the form from `score_min_params`' own choice.** ← see I8. Should be folded into the design, not treated as an alternative.

**E. Put `ScoreModel` in `config.rs` instead of `mapq.rs`.** `Aligner` already lives there (`config.rs:21`), as does `RunConfig`, so dependencies run one way (`mapq`/`merge`/`combined` → `config`) instead of `config ⇄ mapq`. Compiles either way; this is a cleanliness call the plan should make explicitly rather than by default.

**F. Named constructors.** `ScoreModel::new(0.0, -0.2, false, Aligner::Bowtie2)` is four positional args, two of which are `f64` and one a `bool` — swappable without a compile error, across ~29 test call sites. `ScoreModel::end_to_end(i, s)` / `::bowtie2_local(i, s)` / `::hisat2_local(i, s)`, with the general `from_mode(i, s, local, aligner)` used only by `config::resolve`, makes every test call site self-documenting and removes the "which aligner do I pass in an end-to-end test?" ambiguity the plan leaves open.

**G. Adopt Bowtie 2's int truncation.** Reject explicitly and in writing (C2) — it would break end-to-end (`mapq.rs:259-262`).

---

## 5. Validation sufficiency

| # | Sufficient? | Gap |
|---|---|---|
| V1 | ❌ **No** | No `--score_min` dimension ⇒ blind to C1's dominant failure mode, and it's the gate the plan relies on to *decide* D-EDGE. |
| V2 | ❌ **No** | "Agreement across the grid" is unachievable against a faithful `unique.h` transcription (C2). Must specify the f64 analogue, and must include the exact-perfect cell (I3). |
| V3 | ✅ | Verified: 24. |
| V4 | ✅ | Good — the "differs from the SE-only value" clause is what makes it a real single-mate guard. |
| V5 | ✅ | Add the four recomputed HISAT2 expectations from I1 alongside it. |
| V6 | ⚠️ | Under recommendation A the clamp only exists in Bowtie 2-local, where `scMin → 0⁻` is unreachable with `G,20,8` (scMin ≥ 20). Restate as: `perfect − scMin ∈ [0,1)` ⇒ `diff == 1.0`, plus an explicit assertion that **end-to-end** with `scMin == 0.0` still yields `diff == 0.0` (frozen). |
| V7 | ⚠️ | "Zero test edits" is impossible (signature change). Restate as "no expected-*value* edits", and fix the ordering per I1 or it fails outright. |
| V8 | ⚠️ | Constructible in principle, but "Expected: Agreement" is wrong (C2). Reframe as truncation-explained-disagreement accounting. The plan's instinct to pre-authorize triage is right; the expectation just needs correcting. |
| — | ❌ **Missing** | Wiring/integration test asserting a local MAPQ value from a real BAM, with a **≥23 bp** read (I4). |
| — | ❌ **Missing** | `(local, aligner)` → `(log_score_min, local_ladder, match_bonus)` construction matrix — all 8 combinations, including that minimap2/rammap + local is unreachable. Cheap, and it's the direct unit-level guard on the one new piece of logic. |
| — | ❌ **Missing** | Replacement ln()-boundary case for the coverage D2 removes (I6). |

Net: as written, the fix could ship with `config::resolve` mis-wired, or with end-to-end MAPQ altered for non-default `--score_min` users, and every listed validation would still pass.

---

## 6. Action items

### Critical — resolve before implementation

1. **C1** — Pre-decide D-EDGE: the `max(1, …)` clamp and the new denominator apply **only when `match_bonus > 0`** (Bowtie 2-local). End-to-end and HISAT2-local keep `sc_min.abs()` verbatim. Correct §3's edge-case row (the real condition is `scMin ≤ −1`, not `len ≥ 5`) and §7's "by construction" claim.
2. **C1** — Add a `--score_min` dimension to V1: `{(0,−0.2), (0,−0.05), (0,−0.6), (10,−0.2), (0,0), (−1,−0.2)}` × `len ∈ 1..=500`, SE and PE.
3. **C2** — Record in §1 that Bowtie 2 computes `scMin`/`diff`/`bestOver` in `int64` while Bismark uses `f64`; declare matching it an explicit **non-goal** (it would break `mapq.rs:259-262`). Respecify V2's reference as the deliberate f64 analogue, and V8's expectation as "agreement except at truncation boundaries, each disagreement explained".

### Important — fold into rev 1

4. **I1** — Reorder: step 2's constructor uses `log_score_min = local` (verbatim); D2 becomes its own step after D1, and explicitly recomputes `mapq.rs:397 → 44`, `:401 → 31`, `:402 → 1`, `:410 → 11`.
5. **I1** — Restate V7 as "no expected-**value** edits; call-shape edits only".
6. **I2** — Complete the enumeration: `combined.rs` **598** + **657**; `calc_mapq` call site **`combined.rs:774`**; add **`mod.rs`** to the Files table with its 6 config-expansion sites (2599, 3146, 3528, 4647, 5255, 5705) and the two fn-pointer aliases **`SelectFn` (2751)** / **`SelectFnPe` (2767)**. Correct §11's "all three call sites" → four.
7. **I2c** — State that `--local` is rejected on all four `--combined_index` variants (`config.rs:531-537`), so all 8 `combined.rs` sites are refactor-only.
8. **I3** — Add the reporter's case #2: Bowtie 2-local perfect alignment (len 100, AS 200 → 44), plus a `==diff` leaf with a second best.
9. **I4** — Add a `--bowtie2 --local` integration test asserting a MAPQ value from the BAM, using the existing `make_fake_bowtie2_mapped` harness and a **≥23 bp** read (the current 6 bp fixture returns 22 both before and after — it discriminates nothing).
10. **I4** — Add a `ScoreModel` construction-matrix test over all `(local, aligner)` combinations.
11. **I5** — Correct §7: the perl-oracle job has **no aligner cell at all**, so V1 plus four thin integration assertions are the only automated protection for end-to-end MAPQ.
12. **I6** — Replace the sub-unity-`diff` ln() coverage that D2 removes with a Bowtie 2-local ln()-boundary case.
13. **I7** — Step 7 must bump all three: `rust/VERSION`, `rust/bismark/VERSION`, `rust/bismark/Cargo.toml:3` (guarded by `meta/mod.rs:51` and `:73`).
14. **I8** — Have `score_min_params` return the form it selected and build `log_score_min` from it, so the emitted option and the MAPQ form are one fact.

### Optional

15. Put `ScoreModel` in `config.rs` (where `Aligner` lives) to avoid the `config ⇄ mapq` cycle.
16. Named constructors (`end_to_end` / `bowtie2_local` / `hisat2_local`) — worthwhile given ~29 test call sites and a 4-positional signature with two `f64`s.
17. Pass `ScoreModel` **by value** (32 bytes, `Copy`); avoids an elided lifetime in the two `fn`-pointer aliases.
18. Collapse `score_min` + `diff` into one method returning `(sc_min, diff)`, so a caller can't pass a `sc_min` computed from different lengths.
19. Consider making step 2 / step 3 a **PR** boundary, not just a commit boundary (Alternative B) — attractive given I5.
20. Fix §6's "40 bytes" → 32, and the `Copy`-but-by-reference inconsistency.
21. §10 answers: **3.2.0** (minor) is right; crediting @9xg is right — the analysis in #1079 is accurate and independently reproducible.
22. Record in the deferred HISAT2 issue that HISAT2 has no `--local` mode at all, so `perfect = 0` is likely correct rather than merely provisional (A3).

---

## 7. Bottom line

The plan diagnoses #1079 correctly, sources it properly against upstream, and its two best catches are genuinely good — Bowtie 2-local's `ln()` is *correct* and must be kept (A2), and the `max(1, …)` clamp needed a proof rather than an assertion (D-EDGE). It also gets the right structural answer (`ScoreModel`) for the right reason.

But the invariance proof it demands of itself is scoped to the wrong variable — the condition is `scMin ≤ −1`, not `len ≥ 5`, and V1 as specified cannot see the difference. The gate meant to *decide* D-EDGE would green-light the unsafe branch. Alongside that, the plan's two upstream-comparison gates (V2, V8) cannot pass as specified because Bowtie 2's MAPQ arithmetic is integer and Bismark's is float — a distinction visible in the very snippet the plan quotes. Fix those two, correct the step ordering so the "no-op" step really is one, complete the site list, and add a single wiring test, and this is ready to implement.

**Report:** `/Users/fkrueger/Github/Bismark/plans/07292026_local-mapq-denominator/PLAN_REVIEW_B.md`

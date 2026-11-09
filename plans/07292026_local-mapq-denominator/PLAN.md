# PLAN — Local-mode MAPQ denominator: use Bowtie 2's `perfectScore − scMin`

**Issue:** [FelixKrueger/Bismark#1079](https://github.com/FelixKrueger/Bismark/issues/1079) (reported by @9xg, confirmed 2026-07-29)
**Type:** correctness fix (Bowtie 2 `--local` only) + enabling refactor
**Revision:** **rev 1** — dual plan-review folded (see §12)

**Decisions locked (Felix):**
1. Fix **unconditionally** — no compat flag.
2. Scope = Bowtie 2 denominator (**D1**) + HISAT2 `scMin` form (**D2**). HISAT2 *perfect score* deferred.
3. **Two PRs** — mechanical refactor, then semantics.
4. `ScoreModel` lives in **`config.rs`** (next to `Aligner`).
5. minimap2/rammap defect (**I5**) → assumption row + separate issue, not fixed here.

---

## 1. Goal

Make Bowtie 2 `--local` MAPQ match Bowtie 2's own normalization. Two defects, both faithfully inherited from Perl v0.25.1:

| # | Defect | Fix |
|---|---|---|
| **D1** | `diff = abs(scMin)` assumes perfect score 0 — true only end-to-end. Bowtie 2-local awards `--ma 2` per base, so perfect = `2 × len`. | `diff = max(1, perfect − scMin)`, **Bowtie 2-local only** |
| **D2** | HISAT2 `--local` is passed a **linear** `--score-min L,0,-0.2` but MAPQ recomputes `scMin` with `ln(len)`. | HISAT2-local `scMin` becomes linear, derived from the emitted form |

### Explicit non-goals (documented so they don't resurface every review)

| Non-goal | Why |
|---|---|
| **HISAT2 perfect score** stays `0` | Bismark's docs (`bismark:9731`) say it "is currently not exactly known". Likely *correct* rather than provisional — HISAT2 has **no `--local` mode** at all (Bismark's "HISAT2-local" is just dropping `--no-softclip`, `options.rs:341-346`) and its scoring is monotone. Record that reasoning in the deferred issue so it isn't re-derived. |
| **Matching Bowtie 2's integer arithmetic** | Bowtie 2 computes `scMin`/`diff`/`bestOver` in `int64_t` (`unique.h:213`, `simple_func.h:106` `return (T)ret`); Bismark uses `f64`. Adopting truncation would change **end-to-end** MAPQ (`mapq.rs:259-262` pins `len 51 → scMin −10.2`). **Considered and rejected.** Drives V2/V8's expectations (§9). |
| **minimap2/rammap** `perfect = 0` | Same defect class as D1 in a *default* path (**I5**) — but minimap2 SE is byte-frozen vs Perl v0.25.1. Separate issue. See A9. |

**Outcome:** end-to-end **and** HISAT2-local denominators are byte-frozen; only Bowtie 2-local `diff` changes. Bowtie 2-local MAPQ stops saturating at 44.

### Why this is a fix, not a behaviour change

Bismark's `--local` help text already **documents** the correct formula (`bismark:9728-9730`):

> "For Bowtie 2, the match bonus `--ma` (default: 2) is used in this mode, and the best possible alignment score is equal to the match bonus (`--ma`) times the length of the read."

Upstream Bowtie 2 v2.5.5 `unique.h:206-218` confirms:

```cpp
TAlScore scPer = (TAlScore)sc_.perfectScore(rdlen);
if(s.paired()) scPer += (TAlScore)sc_.perfectScore(ordlen);
TAlScore diff = std::max<TAlScore>(1, scPer - scMin); // scores can vary by up to this much
```

Perl copied that trailing comment nearly verbatim while substituting `abs $scMin`, and its own comment concedes the assumption (*"since max AS is 0 for end-to-end alignment"*).

### Measured impact (reproduced by both reviewers)

`abs(scMin)` grows logarithmically; the true denominator grows linearly, so `bestOver/diff` exceeds 1.0 for most local alignments and the ladder saturates:

```
read len 250, G,20,8, no second best
    AS  %perfect | current | bowtie2 | delta
   400     80%   |    44   |   42    |  +2
   300     60%   |    44   |   36    |  +8
   200     40%   |    44   |   24    | +20
   150     30%   |    44   |   22    | +22
```

---

## 2. Context

### Files (corrected — rev 0 missed `mod.rs` entirely)

| File | Role |
|---|---|
| `rust/bismark/src/aligner/config.rs` | **`ScoreModel` lives here** (next to `Aligner`, L21). Build point **L637-638**; `RunConfig` fields L231-238. `--local` rejections: minimap2/rammap L517-529, all four `--combined_index` variants L531-537. |
| `rust/bismark/src/aligner/mapq.rs` | `calc_mapq` (L19-136) — `diff` at **L43**; `calc_mapq_local` (L143-220); ~28 unit call sites from ~L325. |
| `rust/bismark/src/aligner/options.rs` | `score_min_params` (**L371-410**) — emits `G,20,8` (Bowtie 2-local) vs `L,0,-0.2` (HISAT2-local + all end-to-end); **shape-only validation** (L405-410). |
| `rust/bismark/src/aligner/merge.rs` | Call sites **L370** (SE), **L747** (PE); trio params L198, L528. `check_results_paired_end` (L530) **already takes `aligner`**. |
| `rust/bismark/src/aligner/combined.rs` | **8** trio signatures: 188, 246, 388, 437, 501, 543, **598**, **657**. Call sites **L349** (SE) + **L774** (PE). Test call L1205. `too_many_arguments` allow L535. |
| **`rust/bismark/src/aligner/mod.rs`** | **6** config-expansion sites: 2599, 3146, 3528, 4647, 5255, 5705. **2 fn-pointer aliases hardcoding the trio: `SelectFn` (L2751-2758), `SelectFnPe` (L2767-2775)** — must change in lockstep with all 8 `combined.rs` selectors or it won't compile. `--five_base_min_mapq` filter at L2052. |
| `CHANGELOG.md` (repo root — the only one), `rust/README.md`, 3× version literals | release mechanics (§5 step 8) |

**True blast radius:** ~10 fn signatures + 2 fn-pointer aliases + 6 config-expansion sites + 4 production call sites + `RunConfig` + ~29 test call sites. Rev 0's "~10 signatures / 3+ call sites" understated it by roughly half.

### Design rationale (rev 0's premise was false)

Rev 0 claimed "no call site knows which aligner is running". **False** — `merge.rs:530` already takes `aligner`. The `ScoreModel` decision stands on two *correct* grounds:
1. **Net arity reduction** — replaces 3 params with 1 at ~10 sites, retiring the `#[allow(clippy::too_many_arguments)]` at `combined.rs:535` (`select_pe_nondir` 8→6; clippy's threshold is >7) and at `merge.rs:190` (9→7). `merge.rs:519` stays at 11→9 and keeps its allow.
2. **One construction point** — `config.rs:637`, where `aligner` is fully resolved (all overrides and 5-Base/combined guards run at L602-628).

### What `local` conflates today

| Mode | `scMin` form | Ladder | perfect/base | `diff` after fix |
|---|---|---|---|---|
| Bowtie 2 end-to-end | linear | end-to-end | 0 | `abs(scMin)` — **frozen** |
| Bowtie 2 `--local` | **ln** (correct — `G` *is* logarithmic) | local | **2.0** | `max(1, perfect − scMin)` ← **only change** |
| HISAT2 end-to-end | linear | end-to-end | 0 | `abs(scMin)` — **frozen** |
| HISAT2 `--local` | **linear** ← D2 (was `ln`) | local | 0 (deferred) | `abs(scMin)` — **frozen** |
| minimap2 / rammap | linear | end-to-end | 0 (**wrong**, see A9) | `abs(scMin)` — **frozen** |
| `--combined_index` (any) | — | — | — | `--local` rejected ⇒ refactor-only |

Bowtie 2-local's `ln()` is **correct** and stays: Bismark emits `G,20,8`, and `G` is Bowtie 2's logarithmic form (`simple_func.h:88-107`). Only HISAT2's `ln()` is wrong, because it is emitted `L` (linear).

---

## 3. Behavior

1. **Prerequisite** — `score_min_params` returns the **form it selected** alongside the coefficients (it already computes `prefix = "G," | "L,"` at `options.rs:372`). `ScoreModel` is built once in `config::resolve` from that form + `aligner` + `cli.local`, and stored on `RunConfig` in place of the three scalars.
2. **`scMin`** — `intercept + slope × f(len)`, `f = ln` iff `form == Log`; summed over mates. **Derived from the emitted form, never re-decided** (closes D2's root cause — see §5 step 1).
3. **Perfect score** — `match_bonus × (len1 + len2.unwrap_or(0))`. `match_bonus = 2.0` **only** for Bowtie 2 + local, else `0.0`.
4. **Denominator — gated on `match_bonus > 0.0`, NOT on `local`:**
   ```rust
   let diff = if self.match_bonus > 0.0 {
       (self.perfect(len1, len2) - sc_min).max(1.0)   // Bowtie 2-local (unique.h)
   } else {
       sc_min.abs()                                   // frozen: all end-to-end + HISAT2-local
   };
   ```
5. **`bestOver`** — `as_best − scMin`. **Unchanged.**
6. **Ladder** — `local_ladder` selects it; both ladders' thresholds and return values untouched.

### Why gate on `match_bonus`, not `local` (rev-1 correction — this was the plan's most serious flaw)

Rev 0 promised end-to-end byte-identity "by construction (`perfect = 0`)" and narrowed the risk to `len < 5`. **Both were wrong.** The identity is:

```
max(1, 0 − scMin) ≡ abs(scMin)   ⟺   scMin ≤ −1
```

which fails two independent ways, **neither** a `len` question:

- **(a) sign** — `abs(scMin)` and `−scMin` diverge for every `scMin > 0`. `--score_min` is **shape-validated only** (`options.rs:405-410` mirrors Perl's permissive `^L,(.+),(.+)$` — no numeric check), so a positive intercept parses fine.
- **(b) magnitude** — the `len ≥ 5` threshold is really `1/|slope|`: a slope of `−0.05` pushes it to `len < 20`; `−0.001` to `len < 1000`.

Measured divergences under a *universal* clamp (end-to-end, AS = 0):

| `--score_min` | len | old MAPQ | new MAPQ |
|---|---|---|---|
| `L,0,-0.2` | 1 | 42 | **0** |
| `L,0,-0.2` | 3 | 42 | **24** |
| `L,0,-0.05` | 10 | 42 | **23** |
| `L,10,-0.2` | 50 | 42 | **0** |
| `L,0,0` | 50 | 42 | **0** |
| `L,0,0.2` | 100 | 2 | **39** ← also diverges under a *local-only* clamp |

`L,10,-0.2` at 50 bp is 42 → 0: **every read in the run** drops below a `-q 20` filter. And methylseq emits non-default `--score_min L,0,-N` (`aligner_methylseq_conformance.rs:15,88-96`), so this is a live production configuration, not a hypothetical.

Gating on `match_bonus > 0.0` is strictly narrower than gating on `local`: it also keeps **HISAT2-local** clamp-free, so HISAT2 values move for exactly **one** reason (D2's `scMin` form) rather than two unrelated ones in the same change — honest given that HISAT2's perfect score is admittedly unknown. `abs()` is the right placeholder there.

**Corollary:** the clamp is near-dead code where it *does* apply. A reported alignment satisfies `scMin ≤ AS ≤ perfect`, so `perfect − scMin ≥ 0`; only the `[0,1)` sliver clamps. With `G,20,8`, reads below ~23 bp have `perfect < scMin` outright (len 22 → perfect 44, scMin 44.73) and Bowtie 2 cannot report them at all. So the clamp's only *behaviour-changing* reach would have been end-to-end — the exact opposite of its intent.

### Edge cases

| Case | Handling |
|---|---|
| **End-to-end / HISAT2-local invariance** | Byte-identical **by construction** — the `match_bonus == 0.0` branch is the unmodified `sc_min.abs()`. No sweep needed to prove it; V1 is a regression net, not the proof. |
| Non-default `--score_min` | Covered by the gating above. V1 carries a `--score_min` axis regardless. |
| `len = 0` | End-to-end: unchanged (same branch). Bowtie 2-local: `ln(0) = −inf` ⇒ `scMin = −inf` ⇒ `diff = +inf`, `best_over = +inf`, and `inf >= inf*0.8` is **true** ⇒ 44, same as before (`abs(−inf) = inf`). Invariant *by reasoning*, not by assertion. |
| `diff` clamp | `max(1.0)` per Bowtie 2. Reachable only for `perfect − scMin ∈ [0,1)` — practically unreachable in real Bowtie 2-local data, trivially reachable in fake-aligner fixtures (V10). |
| `diff == 0` failure shape | **There is no division** in either ladder — every comparison is `best_over >= diff * k`. A zero `diff` collapses every threshold to `>= 0` so the **top rung silently wins**; it does not panic. (Rev 0 mis-described this.) |
| Bowtie 2-local `scMin` sign | **Guaranteed positive**: `bt2_search.cpp:1861` makes Bowtie 2 *die* if the match bonus is >0 and `--score-min` can be ≤0. So `abs(scMin) ≡ scMin` there, and `diff` is the only thing that changes. |
| PE | perfect sums **both** mates: `2·(l1+l2)`, matching `unique.h:207-209`. No per-mate truncation concern (`2·len` is integral). |
| `--combined_index` (all 4 variants) | `--local` **rejected** (`config.rs:531-537`) ⇒ `match_bonus` is always 0 there ⇒ all 8 `combined.rs` sites are **refactor-only**. Material risk reduction. |
| 5-Base | Never calls `calc_mapq` (verified: no `config.score_min_*` site in `five_base_*`); its MAPQ comes from minimap2, and `--local` is rejected for minimap2. |

---

## 4. Signature

Lives in `config.rs` (with `Aligner`), so dependencies run one way: `mapq`/`merge`/`combined` → `config`.

```rust
/// Which score-min function form was emitted to the aligner. Single source of
/// truth for both the emitted `--score-min` option and the MAPQ `scMin` form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreMinForm {
    /// `L,<i>,<s>` — `scMin = i + s·len`.
    Linear,
    /// `G,<i>,<s>` — `scMin = i + s·ln(len)`.
    Log,
}

/// How alignment scores are normalized for MAPQ. Fields are private so an
/// inconsistent model (e.g. a match bonus without the Bowtie 2-local form)
/// cannot be constructed. Built once in `config::resolve`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoreModel {
    intercept: f64,
    slope: f64,
    form: ScoreMinForm,
    local_ladder: bool,
    /// Perfect-alignment score per base. `BOWTIE2_LOCAL_MATCH_BONUS` for
    /// Bowtie 2 `--local`; 0 everywhere else. See A9 for minimap2/rammap.
    match_bonus: f64,
}

/// Bowtie 2's `--ma` default. Not settable in Bismark (A4) — if a passthrough is
/// ever added, this must read it.
pub const BOWTIE2_LOCAL_MATCH_BONUS: f64 = 2.0;

impl ScoreModel {
    /// The only general constructor — `form` comes from `score_min_params`, so the
    /// emitted option and the MAPQ form are the same fact by construction.
    pub fn from_emitted(intercept: f64, slope: f64, form: ScoreMinForm,
                        local: bool, aligner: Aligner) -> Self;

    // Named constructors — self-documenting at ~29 test call sites, and they
    // remove the "which aligner for an end-to-end test?" ambiguity.
    pub fn end_to_end(intercept: f64, slope: f64) -> Self;
    pub fn bowtie2_local(intercept: f64, slope: f64) -> Self;
    pub fn hisat2_local(intercept: f64, slope: f64) -> Self;

    fn score_min(&self, len1: usize, len2: Option<usize>) -> f64;
    fn perfect(&self, len1: usize, len2: Option<usize>) -> f64;

    /// `(best_over, diff)` together, so a caller cannot pass a `sc_min` computed
    /// from different lengths, and `best_over`'s formula stays structurally fixed.
    fn normalize(&self, len1: usize, len2: Option<usize>, as_best: i64) -> (f64, f64);
}

/// Bismark MAPQ. `read2_len` is `Some` only for paired-end.
pub fn calc_mapq(
    read1_len: usize,
    read2_len: Option<usize>,
    as_best: i64,
    as_second: Option<i64>,
    model: ScoreModel,   // 32 bytes, Copy — by value
) -> u8;
```

---

## 5. Implementation outline

### PR 1 — mechanical refactor (zero expected-value changes)

1. **Add `ScoreMinForm` + `ScoreModel`** to `config.rs`. Change `score_min_params` (`options.rs:371`) to return the form it already selects at L372. **In this PR the constructor sets `form` verbatim from the emitted value and `local_ladder = local`, with `match_bonus = 0.0` for *everything*** — so behaviour is bit-identical, including HISAT2-local's current `ln()`.
   > ⚠️ **Do NOT** set `match_bonus`/the Bowtie 2 form condition here. Rev 0 put `log_score_min = local && aligner == Bowtie2` in this step, which lands **D2 inside the supposedly-inert refactor** and changes 4 HISAT2 expectations. That is what broke rev 0's ordering.
   >
   > Concretely: HISAT2-local must still receive `ScoreMinForm::Log` in PR 1 even though it is emitted `L`. The desynchronization *is* D2 and it is repaired in PR 2 step 5.
2. **Thread `ScoreModel`** in place of the trio: `calc_mapq`; `merge.rs:198,528`; **all 8** `combined.rs` signatures (188, 246, 388, 437, 501, 543, 598, 657); the **2 fn-pointer aliases** `SelectFn` (`mod.rs:2751`) and `SelectFnPe` (`mod.rs:2767`); the 6 `mod.rs` config-expansion sites (2599, 3146, 3528, 4647, 5255, 5705); `RunConfig` (`config.rs:231-238`) and its literal; the 4 production call sites (`merge.rs:370,747`; `combined.rs:349,774`); ~29 test call sites (shape only).
   - Pass **by value** (`Copy`, 32 bytes) — avoids an elided lifetime in the two fn-pointer aliases.
   - Delete the now-unneeded `#[allow(clippy::too_many_arguments)]` at `combined.rs:535` and `merge.rs:190`; keep `merge.rs:519`.
3. **Gate:** V7 — full suite green with **no expected-value edits** (call-shape edits only).

### PR 2 — semantics

4. **D1.** Replace `mapq.rs:43` with the `match_bonus > 0.0` dispatch (§3.4) via `model.normalize(...)`, and set `match_bonus = BOWTIE2_LOCAL_MATCH_BONUS` for Bowtie 2 + local in `from_emitted`. Rewrite the stale comment (`// scores vary by up to this much (max AS = 0)`) to one line, noting the `float_cmp` allow still holds (O4).
5. **D2.** Fix the desynchronization: `form` now comes straight from `score_min_params`, so HISAT2-local gets `Linear`. **Recompute by hand** the affected expectations in `local_hisat2_default_params_mapq` (`mapq.rs:390-419`):

   | Line | old | new |
   |---|---|---|
   | `:397` `calc_mapq(50, None, -1, None, …)` | 22 | **44** |
   | `:401` `calc_mapq(50, None, 0, Some(-1), …)` | 40 | **31** |
   | `:402` `calc_mapq(50, None, -1, Some(-1), …)` | 0 | **1** |
   | `:410` `calc_mapq(150, Some(150), 0, Some(-1), …)` | 34 | **11** |

   (`:396` and `:398` stay 44.) **Re-derive from the linear `scMin` by hand — do not re-baseline from the implementation.** These are genuinely independent assertions; the test's own doc-comment stresses they are "the Perl local ladder hand-applied … NOT self-consistency".
6. **Retire the tautologies.** `mapq.rs:376` and `:416` compute `expect` by calling the function under test with `sc.abs()` — they cannot fail regardless of the denominator. Rewrite against V2's independent reference.
7. **Re-home the `ln()`-boundary coverage.** `mapq.rs:386-389,403-409` document that test as the *only* sub-unity-`diff`, `ln()`-derived-boundary coverage. After D2, HISAT2-local is linear and **no configuration produces a sub-unity `diff` in local mode**, so that regime becomes unreachable. Add a **Bowtie 2-local** `ln()`-boundary case (now the only `ln()` consumer), with the same "why this cell" comment style.
8. **Docs.** (Version literals are **not** touched here — see step 10.)
   - `mapq.rs` header (L1-17) and `config.rs:235` describe `local` as selecting "`ln()` scMin + the local ladder" — now form- and aligner-dependent. One line each.
   - `CHANGELOG.md` (repo root) under a new **`## Unreleased`** heading: Bowtie 2-local MAPQ now matches Bowtie 2; local values differ from v0.25.1/3.1.0; end-to-end **and HISAT2-local** unaffected; HISAT2 perfect score still open; credit **@9xg**. Add O4's note (the `bestOver == diff` rungs now mean `AS == perfect`, which is why 39/35/34/… start appearing).
     > ⚠️ The file currently has **no `Unreleased` convention** — every heading is a released version. PR 2 introduces one; the release cut renames it to `## Bismark <version> (released <date>)`. Writing the entry now (rather than reconstructing it at cut time from `git log`) is what keeps the "HISAT2 is self-consistent but not yet proven correct" nuance from being lost.
   - `rust/README.md`: aligner row + dated Milestones line — its own rule (L175) is **per module-merge PR**, so this belongs in PR 2, not the cut.
9. **File the two deferred issues:** HISAT2 perfect score/ladder (carrying O3's reasoning), and minimap2/rammap positive-AS (A9).

### Release cut — NOT part of either PR (Felix, 2026-07-29)

10. The version bump belongs to the **release cut**, not this fix. At cut time, move **all three** literals together — `rust/VERSION`, `rust/bismark/VERSION`, `rust/bismark/Cargo.toml:3` — and rename the `## Unreleased` CHANGELOG heading to the cut version/date.
    - Two guard tests enforce the lockstep: `meta/mod.rs:51` (`vendored_version_matches_repo_version`) and `meta/mod.rs:73` (`cargo_pkg_version_matches_suite_version`, commented "load-bearing for a GA cut", never skipped). Because PR 1 and PR 2 leave all three at `3.1.0`, they stay mutually consistent and both guards pass — **no version work is needed in either PR**.
    - Magnitude when cut: **minor** (3.1.0 → 3.2.0) — a behaviour change in a non-default mode is not a patch.

---

## 6. Efficiency

O(1) added arithmetic per accepted alignment (one multiply, one compare); no allocation. `ScoreModel` is `3×f64 + 2×bool + enum` ≈ **32 bytes**, `Copy`, passed **by value**. Arity drops by 2 at ~10 sites.

The real cost is **change-cost, not run-cost**: ~50 edit sites in a module with no aligner Perl-oracle in CI (I5) — which is precisely why PR 1 is separated and gated on V7.

---

## 7. Integration

- **Reads:** `cli.local`, resolved `aligner`, and `score_min_params`' `(intercept, slope, form)` — all available at `config.rs:637`.
- **Writes:** the BAM MAPQ column, **Bowtie 2 `--local` only**.
- **Downstream:** dedup and the extractor don't filter on MAPQ. `--five_base_min_mapq` (`mod.rs:2052`) *is* an internal MAPQ filter, but 5-Base is minimap2-only, `--local` is rejected there, and 5-Base MAPQ comes from minimap2 rather than `calc_mapq` — so it is unaffected. External `samtools view -q` users in Bowtie 2-local mode see stricter (correct) filtering: the point of the fix.
- **CI exposure — worse than rev 0 said.** The `perl-oracle` job's 13 cells (`rust_ci.yml:194-217`) are 7 genome-prep, 4 methylation_consistency, 1 extractor, 1 dedup: **no aligner cell at all**. So end-to-end aligner MAPQ has no Perl oracle either. Total automated protection today = V1 plus **four** MAPQ value assertions across all aligner integration tests (`aligner_cli.rs:422, 705, 706, 2429`) — all end-to-end, all 6 bp, all default `--score_min`. That thinness is why V1's `--score_min` axis and V9/V10 are load-bearing, not nice-to-have.
- The two existing `--local` integration tests (`aligner_cli.rs:2034`, `:2116`) assert the option string and soft-clip CIGAR only — **no MAPQ** — so they will not break.

---

## 8. Assumptions

| # | Assumption | Status |
|---|---|---|
| A1 | Bowtie 2-local perfect = `2 × len` per mate, summed for PE | ✅ Both reviewers verified: `scoring.h:310-316` (`perfectScore = monotone ? 0 : rdlen × match(30)`), `unique.h:207-209` |
| A2 | `G` is logarithmic ⇒ Bowtie 2-local `ln()` is **correct**, keep it | ✅ `simple_func.h:88-107` |
| A3 | HISAT2-local perfect = `0` pending investigation | ⚠️ Probably **correct**, not provisional — HISAT2 has no `--local` mode; scoring is monotone. Deferred; reasoning goes in the issue |
| A4 | `--ma` unreachable ⇒ `2.0` is a constant | ✅ Verified structurally: no free-form aligner-option vector, no `trailing_var_arg`/`allow_hyphen_values`, 23 curated `opts.push` sites, none `--ma` |
| A5 | Both ladders correct and untouched | ✅ Diffed leaf-by-leaf against `unique.h:223-380` |
| A6 | `bestOver = as_best − scMin` unchanged | ✅ `unique.h:222`; structurally enforced by `normalize()` |
| A7 | Local mode not covered by perl-oracle | ✅ …and **no aligner cell at all** (see §7) |
| **A8** | **Read length == the length Bowtie 2 scored** — the basis for `perfect = 2 × len` | ✅ Verified: no `--trim5/--trim3/-5/-3/--clip_r*` in `options.rs`/`cli.rs`; `sequence` at all 4 call sites is the original uc read; soft-clipped records retain full SEQ (`aligner_cli.rs:2073`); Bowtie 2 also passes the full `rdlen` to `mapq()` |
| **A9** | **minimap2/rammap perfect = 0 is WRONG but held frozen** | 🟠 They emit **positive** `AS:i:` (`aligner_cli.rs:2463`) through `local = false`, so `bestOver/diff = AS/(0.2·len) + 1 > 1` ⇒ top rung for essentially every unique hit — **the same defect class as #1079, in a default path, today.** Byte-frozen vs Perl v0.25.1 + minimap2 (`rust/README.md:161`), so deferred to its own issue. **`match_bonus = 0.0` for these aligners is a freeze, not a verified value.** |
| **A10** | `scMin`/`diff`/`bestOver` stay `f64` while Bowtie 2 uses `int64` | ✅ Deliberate non-goal (§1); drives V2/V8 |
| **A11** | `--score_min` may be **non-default** and is numerically unvalidated | ✅ Load-bearing for §3's gating; methylseq uses non-default forms |

---

## 9. Validation

| # | Verify | How | Expected |
|---|---|---|---|
| **V1** | End-to-end **and HISAT2-local** frozen (regression net; the *proof* is now structural) | Sweep `len ∈ 1..=500` × **`--score_min ∈ {(0,−0.2), (0,−0.05), (0,−0.6), (10,−0.2), (0,0), (−1,−0.2)}`** × `as_best` grid × `{None, Some(second)}` × **SE and PE**, against a frozen reference fn (`abs(scMin)` + the pre-change ladders) | Identical in every cell. The `--score_min` and PE axes are mandatory — without them V1 is blind to the whole C1 class |
| **V2** | D1 against an **independent** reference (kills the tautology) | Transcribe `unique.h:206-222`'s **structure** (`scPer`, `max(1, scPer − scMin)`) but with **Bismark's `f64` `scMin`** — a deliberate f64 analogue, commented as such. Grid over `len × AS × second-best` | Agreement. **Not** a faithful int transcription: Bowtie 2 truncates `scMin` (A10), so a faithful copy disagrees at ~every bucket boundary on a *correct* fix (562 cells over `len ∈ 25..300` SE alone; e.g. len 100/AS 128 → f64 28 vs int 36) |
| **V3** | Reporter's case #1 | `len=100, G,20,8, AS=100`, no second best | **24** (was 42) |
| **V4** | PE sums both mates | `len1=len2=100` local; assert `diff = 2×200 − scMin(both)` and that it **differs** from the SE-only value | Matches; differs (guards a single-mate bug) |
| **V5** | D2 — HISAT2-local `scMin` matches what is emitted | Assert `scMin == −0.2 × len` **and the converse** `scMin != −0.2 × ln(len)` (so a revert to `ln` fails loudly); cross-check the emitted option is still `L,0,-0.2`. Include the four recomputed values from §5 step 5 | Linear used; option unchanged |
| **V6** | The clamp | Bowtie 2-local `perfect − scMin ∈ [0,1)` (e.g. `len=6, G,20,8` → perfect 12, scMin 34.3) ⇒ `diff == 1.0`, and assert **which rung** results (risk is a silent top rung, not a panic). Plus: end-to-end with `scMin == 0.0` still yields `diff == 0.0` (frozen) | As stated |
| **V7** | **PR 1 is a true no-op** | Full suite after PR 1, before PR 2 | Green with **no expected-value edits** (call-shape edits only — the signature change forces ~29 of them, so "zero test edits" is impossible by construction) |
| **V8** | Real Bowtie 2 oracle (bonus gate) | `bismark --local` on a fixture; for SE reads mapping uniquely in one strand instance with no second-best, compare Bismark's MAPQ to Bowtie 2's own | **Agreement except where `floor(scMin)` moves a bucket** — every disagreement must be explained by the int/float difference, and the rate quantified. Triage MAPQ 255 (non-primary) and cross-instance second-best out of the comparison set. **If the restricted set can't be built reliably, say so and rely on V2** — do not quietly drop it |
| **V9** | **Wiring, unit level** — the `(local, aligner)` → model matrix | All 8 combinations: `--local --bowtie2` → `match_bonus==2.0, form==Log, local_ladder`; `--local --hisat2` → `0.0, Linear, local_ladder`; each aligner's default → `0.0, Linear, !local_ladder`; minimap2/rammap + local unreachable | As stated. Cheap, and the direct guard on the only new logic |
| **V10** | **Wiring, end-to-end** — a local MAPQ **value out of a real BAM** | `--bowtie2 --local` integration test via the existing `make_fake_bowtie2_mapped` harness (which already asserts MAPQ from a BAM, `aligner_cli.rs:422`). **Two cells:** (a) a **≥23 bp** read with default `G,20,8` (len 25 → scMin 45.75, perfect 50) — realistic params; (b) the existing 6 bp fixture with `--score_min G,1,0` (constant `scMin=1`, `alwaysPositive` ✓, perfect 12, diff 11) — no new fixture needed | A MAPQ that **differs old vs new**. ⚠️ The existing 6 bp fixture at default `G,20,8` returns **22 both before and after** — it discriminates nothing. Either cell must be built deliberately |
| **V11** | Reporter's case #2 (dropped in rev 0) | Bowtie 2-local **perfect** alignment: `len=100, AS=200` ⇒ `best_over == diff == 143.1586` exactly (same f64 subtraction both sides) | 44 with no second best; with a second best it lands on the `==diff` leaves (`mapq.rs:174,182,190,198,206`) — reachable and meaningful for the first time |

**Load-bearing:** V7 (isolates the refactor), V1 (protects the frozen paths, *with* its new axes), V2 (replaces a test that cannot fail), V9+V10 (without them a mis-wired `aligner` makes the fix a silent no-op with a fully green suite — the same failure shape that let #1079 ship).

---

## 10. Questions or ambiguities

| Priority | Item |
|---|---|
| **Resolved** | Gating policy (unconditional); scope (D1 + D2, HISAT2 perfect deferred); **D-EDGE — now decided, not in-flight**: the clamp and new denominator apply only when `match_bonus > 0`; two PRs; `ScoreModel` in `config.rs`; minimap2 → assumption + issue; 3.2.0 minor; credit @9xg |
| **Resolved** | Version bump → **release cut**, not PR 2 (Felix, 2026-07-29). Neither PR touches the three literals; they stay consistent at 3.1.0 so both guard tests pass. CHANGELOG entry lands in PR 2 under `## Unreleased`. See step 10. |
| **Deferred (issue)** | HISAT2 perfect score / correct ladder — carry O3's reasoning (no `--local` mode ⇒ likely genuinely 0) |
| **Deferred (issue)** | minimap2/rammap positive-AS ⇒ top-rung MAPQ (A9) — same defect class, default path, currently byte-frozen |
| **Noted** | Do **not** add a `--local` cell to `perl-oracle` — it would codify the bug. A **Bowtie 2**-oracle cell (V8) is the right analogue |

---

## 11. Self-Review

**Efficiency** — O(1) added arithmetic; `Copy` by value; net arity reduction. Change-cost (~50 sites) is the real risk and is mitigated by the PR split + V7.

**Logic** — traced all **4** production call sites (rev 0 said 3) and confirmed each carries the read lengths, so no data plumbing is needed beyond the model. Confirmed `bestOver` must not move — only the denominator — else the ladder shifts twice; `normalize()` enforces that structurally.

**Edge cases** — `scMin ≤ −1` (the real condition, now structural), non-default `--score_min`, `len = 0` (reasoned, both branches), the clamp's actual trigger, `diff == 0`'s true failure shape (silent top rung, no division), PE two-mate summation, combined-index (`--local` rejected ⇒ refactor-only), 5-Base (never calls `calc_mapq`), minimap2/rammap (A9).

**Integration** — no internal `calc_mapq` MAPQ consumer; `--five_base_min_mapq` narrowed and cleared; CI exposure corrected (no aligner oracle at all).

**Remaining risks**
1. **V8 may not be cleanly constructible** — Bismark's cross-instance second-best has no Bowtie 2 counterpart, and int-vs-float boundaries add noise. V2 is the real gate; V8 must report rather than silently drop.
2. **HISAT2-local becomes self-consistent but not yet provably correct** (perfect score still 0). The CHANGELOG must say exactly that.
3. **minimap2/rammap ship with a known-wrong denominator** (A9), deliberately frozen. Documented so `match_bonus = 0.0` is never read as verified.
4. **Bowtie 2-local users see MAPQ change** — intended and approved; needs a clear release note.

---

## 12. Implementation Notes (2026-07-29)

**Branch:** `rust/local-mapq-denominator`, based on `origin/dev` (`0a9eeb7`). Not `master`: `config.rs`/`options.rs`/`mod.rs` differ there (the #1075 `--threads` work), and `origin/dev` is both the PR target and byte-identical to the tree the plan's line numbers were verified against (5 of 6 aligner files; `mod.rs` shifted +10, re-verified).

**Status:** both PRs implemented in one working tree. fmt clean, **0 clippy warnings** (`--all-targets`), **2107 tests pass / 0 fail** (was 2101; +6 net). Not committed — awaiting review.

### PR 1 — mechanical refactor

Site counts came out **exactly** as the reviewers' corrected figures (and not the plan rev-0 figures): 2 `merge.rs` + 8 `combined.rs` trio declarations, 6 `mod.rs` config-expansion sites, 2 fn-pointer aliases, and — the compiler's own count — **4** production `calc_mapq` call sites, confirming `combined.rs:774` was real.

**V7 was verified as a proof, not a green run.** A script inverse-transformed the new `ScoreModel` calls back to the trio and compared whitespace-normalized test modules against `HEAD`: `mapq.rs`, `merge.rs`, `combined.rs`, `mod.rs` all came back **byte-identical** (e.g. `combined.rs` 44999 chars both sides). So PR 1 is provably call-shape-only.

**Arity payoff confirmed:** removing the two `#[allow(clippy::too_many_arguments)]` (the `combined.rs` one whose own comment blamed the trio, plus `merge.rs`'s SE one) leaves clippy silent — exactly Reviewer A's prediction. `merge.rs`'s PE allow stays, as predicted.

**Deviation (documented):** plan step 1 said PR 1 should have `score_min_params` return the form and the constructor take it "verbatim from the emitted value" — that is self-contradictory with the warning box immediately after it, because reading the emitted form *is* D2. Resolved in favour of the warning box: PR 1 leaves `score_min_params` untouched and sets `form = if local { Log } else { Linear }` (verbatim current behaviour); PR 2 introduces the emitted form. Without this, D2 would have landed inside the "inert" step and V7 could not have held.

**Deviation:** `match_bonus`, `perfect()` and `BOWTIE2_LOCAL_MATCH_BONUS` were held back to PR 2. In PR 1 they would be written-but-never-read → `dead_code` → a `-D warnings` failure. PR 1 carries only the fields it reads.

### PR 2 — semantics

`ScoreMinForm` is now returned by `score_min_params` and consumed by `ScoreModel::from_emitted`, so the emitted `--score-min` option and the MAPQ `scMin` are one fact (Reviewer B's I8 — closes D2's class, not just the instance).

**Exactly one existing test changed values** — `local_hisat2_default_params_mapq`, as both reviewers predicted. All four predicted values were **hand-derived from the linear `scMin` before touching the file** and matched the predictions exactly: `:397` 22→**44**, `:401` 40→**31**, `:402` 0→**1**, `:410` 34→**11**. Renamed to `local_hisat2_uses_the_linear_form_it_was_emitted`.

**A latent trap the reviewers' analysis exposed:** the tautological test `local_calc_mapq_uses_ln_scmin_and_local_ladder` **still passed** after D1 — its six cells happen not to cross a rung boundary between `diff = 51.30` and `diff = 48.70`. It was asserting a now-wrong denominator and surviving on luck. Replaced (not re-baselined) with `bowtie2_local_reference`, an explicit **f64 analogue** of `unique.h:206-222`, swept over `len × as_best × second-best` plus PE.

**Structural extraction:** the end-to-end ladder is now `calc_mapq_end_to_end`, mirroring `calc_mapq_local`. Needed so the frozen-path reference can call the real ladder — the first attempt used an `unreachable!()` stub, which the code-quality rule forbids; extracting the ladder removed the need for a stub at all.

### Both new gates were verified to have teeth

A regression test that cannot fail is the defect this whole issue is about, so each load-bearing gate was checked against the failure it exists to catch:

| Gate | Injected fault | Result |
|---|---|---|
| **V1** `frozen_paths_match_the_pre_fix_formula` | made the clamp universal (rev 0's "preferred" branch) | **FAILED** at `e2e SE i=0 s=-0.2 len=1` — the exact `len < 5` case |
| **V10** `bowtie2_local_mapq_..._end_to_end` | forced `match_bonus = 0.0` (simulated lost wiring) | **FAILED** `left: 44, right: 28` with the diagnostic naming the cause |

Both faults were reverted and re-verified green; no `TEMP` markers remain.

**V10 fixture:** Reviewer B was right that the existing 6 bp fixture at default `G,20,8` returns 22 both before and after. Reviewer A's `--score_min G,1,0` route also needed `AS:i:0` → `AS:i:6` to discriminate (a swept table showed `AS ∈ 2..9` discriminate; `AS:i:6` gives old **44** → new **28**). Added `make_fake_bowtie2_local_partial_score`. Worth noting the old model returns 44 for *every* `AS ≥ 2` on that fixture — the saturation pathology in miniature.

**Not done (by decision):** V8 (real Bowtie 2 oracle) — needs a Bowtie 2 binary and a restricted comparison set; V2's independent reference is the standing gate, per §9. Version literals untouched (release cut). The two deferred issues (HISAT2 perfect score; minimap2/rammap positive-AS) are **not yet filed**.

## 12b. Post-review fixes (2026-07-29)

Dual code review (`CODE_REVIEW_A.md` / `CODE_REVIEW_B.md`) + coverage audit (`COVERAGE.md`) → **all findings applied**. Both reviewers APPROVED with no correctness defect; coverage was INCOMPLETE on 6 items, all now closed. Final state: fmt clean, **0 clippy warnings**, **2109 tests pass / 0 fail**.

### The one High finding — V10 was form-blind (B's H1)

V10 used `--score_min G,1,0`. With **slope 0**, `i + s·ln(len)` and `i + s·len` are both exactly `1.0`, so the test could not observe the `scMin` **form** — meaning D2's wiring had no end-to-end gate. My own teeth-check had missed this: I injected a lost `match_bonus` (D1's wiring) and concluded "V10 has teeth", never injecting a wrong *form*.

Fixed by moving to `--score_min G,1,1`, which separates all three states: **24** correct · **22** wrong form · **44** lost match bonus. The failure message now names which fault a wrong value implies.

**Re-verified by injecting each fault** (all reverted, no `TEMP` markers left):

| Injected fault | V10 cell (b) | V10 cell (a) | frozen-path |
|---|---|---|---|
| `form` forced to `Linear` | **FAIL** 22≠24 | **FAIL** 22≠36 | — |
| `match_bonus` forced to `0.0` | **FAIL** | **FAIL** | — |
| clamp made universal | — | — | **FAIL** at `len=1` |

### The most valuable doc finding — I shipped the refuted reasoning (B's M1)

The CHANGELOG and README both justified end-to-end safety with *"(there the perfect score is 0, so the two expressions agree)"*. **They do not agree** — that is the exact claim rev 0 made, which plan-review C1 refuted and which caused the whole gating redesign (`max(1, −scMin) ≠ abs(scMin)` whenever `scMin > −1`). As B noted, this is precisely the reasoning a future maintainer would cite to "simplify" the branch away — i.e. to reinstate the bug that `end_to_end_matches_the_pre_fix_formula` exists to catch. Both texts now say end-to-end is safe **because the code branches**, and state explicitly that the expressions are *not* equivalent.

### Everything else applied

| Finding | Fix |
|---|---|
| **E2** (A) `mapq.rs` header claimed "All other modes remain byte-identical to Perl" — false, HISAT2-local isn't | Header now names **both** deliberate deviations; byte-identity claimed for end-to-end only |
| **M4/E3** two stale `options.rs` comments asserting the pre-D2 "`ln()` MAPQ scMin", one *inside* the doc comment the change edited | Both rewritten |
| **M5/E4** `rust/README.md` aligner **row** untouched (only Milestones) | Row now carries the `--local` divergence caveat; byte-identity framed as the end-to-end contract |
| **M2** CHANGELOG self-contradiction on HISAT2 `--local` | Bullet 1 scoped to the denominator, pointing at bullet 2 |
| **O4** missing note that `==diff` rungs (39/35/34/33/32/31) become reachable | Added — users diffing local BAMs will see MAPQ values that never occurred before |
| **M3/E5** release notes promised trackers that didn't exist | **Filed [#1080](https://github.com/FelixKrueger/Bismark/issues/1080)** (HISAT2 perfect score, carrying O3's reasoning) and **[#1081](https://github.com/FelixKrueger/Bismark/issues/1081)** (minimap2/rammap positive-AS); both now cited by number |
| **V6/M6/S5** clamp test asserted `diff == 1.0` but never called `calc_mapq` — the gate's stated risk is a *silent top rung* | Added the rung assertion (**22**, the floor). Also fixed "len ≲ 22" → the clamp is active through **len 23** |
| **V1/item 13** `len` axis was an 11-point sample, not the specified `1..=500` | Widened to `1..=500`; `--score_min` cells hoisted to a shared `SCORE_MIN_CELLS` const |
| **V10/item 22** cell (a) missing | Added `bowtie2_local_mapq_at_default_score_min_end_to_end` — 25 bp read at **default** `G,20,8` (needs ≥23 bp, else `scMin > perfect`), expects **36** |
| **S1/S3** `frozen()`'s `local` branch dead; name overstated HISAT2 freezing | Split into `end_to_end_matches_the_pre_fix_formula` + `hisat2_local_denominator_is_abs_of_the_linear_scmin` (shape frozen, not values) |
| **S2/L4** loop-invariant HISAT2 assertion ran 990× for 66 facts | Hoisted into its own test |
| **S4/L1** four `ln(25)` figures wrong from the 4th digit, in a test whose purpose is hand-derivation | Corrected to 45.7510066 / 4.2489934 / 2.2489934 / 2.1244967 |
| **B-L1** the `31` cell rides a zero-margin edge (`10.0*0.1 == 1.0` exactly) | Kept (deterministic, and Perl computes the same double) + added three off-boundary cells at 100 bp with margin 1 |
| **S7/L2** `float_cmp` allow on `calc_mapq` now vacuous (no float compare left after the extraction) | Removed; both ladders keep theirs |
| **S6** `normalize`/`local_ladder` needlessly `pub` on a published crate | → `pub(crate)` |
| **L1** (A) "never settable independently" wasn't enforced | Reworded to what is actually guaranteed (private fields + one general constructor) |
| **L2** (A) NaN asymmetry: `.max(1.0)` swallows NaN, `abs()` propagates it | Documented on `normalize` |
| **S8/L3** `end_to_end` hardcodes an aligner | Commented why it is sound, with a pointer to revisit under #1081 |
| **L9** upstream multiplies by `(double)0.8f`, not `0.8` — a second int/float divergence | Noted on `bowtie2_local_reference` so nobody tries to make it exact |
| **L6** docs site had no HISAT2 MAPQ caveat | Added to `docs/.../options/alignment.md`, citing #1080 |
| **S9** CI step name said "prove all 12 ran" while `EXPECTED=13` (pre-existing) | Fixed to 13 |

**Reviewer findings deliberately not acted on:** none. **V8** (real Bowtie 2 oracle) remains the one plan item not built — §9 V8 authorizes this explicitly ("if the restricted set can't be built reliably, say so and rely on V2"), and A quantified the residual exposure at **0.40 %** (361 of 89,976 cells differ from a faithful int64 Bowtie 2 analogue, i.e. 99.6 % agreement).

## 13. Revision History

**rev 1 (2026-07-29)** — dual plan-review (`PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md`) folded. ~12 findings were reached independently by both reviewers. Two contradictions resolved **in B's favour**:

| Contradiction | Resolution |
|---|---|
| Step ordering — A called it sound; B showed **D2 lands inside the "no-op" step** (the constructor's `local && aligner == Bowtie2` flips HISAT2-local to linear when threaded), so V7 could not hold | **B correct.** PR 1's constructor is verbatim-behaviour; D2 moved to PR 2 step 5 with hand-recomputed expectations |
| Denominator gating — A: dispatch on `!local_ladder`; B: dispatch on **`match_bonus > 0`** | **B correct** (strictly narrower) — also keeps HISAT2-local frozen, so its values move for one reason, not two |

Substantive corrections to rev 0:
- **C1** — end-to-end invariance was asserted from an incomplete condition (`len ≥ 5`); the real condition is `scMin ≤ −1`, breakable by legal non-default `--score_min` at any length. Now structural, and `D-EDGE` is retired as a decided question. V1 gained `--score_min` + PE axes.
- **C2** — Bowtie 2's `int64` arithmetic recorded as an explicit non-goal; V2 respecified as an f64 analogue; V8's pass criterion reframed.
- **C3/I4** — added V9 + V10 (wiring), the highest-risk untested path.
- **Site enumeration** — added `mod.rs` (6 sites + 2 fn-pointer aliases), `combined.rs` 598/657, call site `combined.rs:774`; "~10 signatures / 3 call sites" → ~50 edit sites.
- **False premise removed** — `merge.rs:530` already takes `aligner`; `ScoreModel` rejustified on arity + single construction point.
- **New assumptions** — A8 (read length == scored length, verified), A9 (minimap2/rammap same defect, frozen), A10 (f64 vs int64), A11 (`--score_min` unvalidated).
- **Design upgrades** — score-min form single-sourced from `score_min_params` (closes D2's *class*); private fields + named constructors; `normalize()`; `ScoreModel` in `config.rs`; by-value.
- **Test fallout corrected** — 4 hand-derived HISAT2 assertions must be recomputed (not just the 2 tautologies); the `ln()`-boundary regime disappears after D2 and needs re-homing; V11 (reporter's case #2) restored; V6/V7 restated; 3 version literals + `rust/README.md`.

**rev 0 (2026-07-29)** — initial plan.

# PLAN REVIEW A — Local-mode MAPQ denominator (issue #1079)

**Plan:** `plans/07292026_local-mapq-denominator/PLAN.md`
**Reviewer:** A (independent, fresh context)
**Date:** 2026-07-29
**Verdict:** **Sound diagnosis, correct core fix, but the validation plan does not yet protect the invariant the plan promises.** Three Critical items must be resolved before implementation; the design itself needs only sharpening.

Everything below was checked against the actual source, Bowtie 2 v2.5.5 upstream, and numeric evaluation — not taken from the plan.

---

## 0. What I verified as correct

Credit where due; these did not need changing and I re-derived each independently.

| Claim | Status |
|---|---|
| **D1 diagnosis** (`abs(scMin)` assumes `perfect = 0`) | ✅ `mapq.rs:43` / Perl `bismark:3938` |
| **Measured-impact table** (len 250: 44/42, 44/36, 44/24, 44/22) | ✅ reproduced all four rows exactly |
| **V3's expected value** (len 100, `G,20,8`, AS 100 → 24, was 42) | ✅ reproduced |
| **A1** perfect = `2 × len`, PE sums both mates | ✅ `scoring.h:310-316` — `perfectScore(rdlen) = rdlen * match(30)` when `!monotone`; `monotone = (matchConst == 0)` (`scoring.h:168`) |
| **A2** `G` form is logarithmic ⇒ Bowtie 2-local `ln()` is **correct** and must stay | ✅ `simple_func.h:99-100` (`SIMPLE_FUNC_LOG → X = log(x)`). This catch (recorded in §11) is the plan's most valuable one — the naive reading of the issue would have broken Bowtie 2-local `scMin`. |
| **A4** `--ma` never reaches the aligner | ✅ absent from Perl `bismark` (only help text at 9729-9730) **and** the Rust CLI; no arbitrary-option passthrough (`cli.positional: Vec<String>` is input files only, no `allow_hyphen_values` / `trailing_var_arg`). `2.0` is safe **today** |
| **A5** local ladder is untouched and correct | ✅ diffed every leaf of `calc_mapq_local` (`mapq.rs:143-220`) against `unique.h`'s `else // Local alignment` branch — verbatim match, including the uniform `0.5` sub-threshold |
| **A7** no `--local` cell in the perl-oracle gate | ✅ no `local` / `score_min` token anywhere in `.github/workflows/rust_ci.yml` |
| §7's read of the two `--local` integration tests | ✅ `hisat2_local_softclip_roundtrip_and_options` (`aligner_cli.rs:2034`) and `hisat2_local_pe_softclip_roundtrip` (`:2116`) assert the option string + soft-clip CIGAR only — **no MAPQ assertion**. They will not break. |
| `len ≥ 5` algebra for D-EDGE | ✅ correct — **but only for the default `(0, −0.2)`**; see C1 |
| Ordering (no-op refactor → semantic fix) | ✅ sound; I found nothing that breaks the isolation (one wording fix in O7) |
| `config.rs:637` is a valid build point | ✅ `aligner` is fully resolved by then (all overrides, 5-Base and combined-index guards, run above at 602-628) |
| `--local` unreachable for minimap2 / rammap / combined-index | ✅ `config.rs:517-537` |

---

## 1. Logic review

### 🔴 C1 — The end-to-end invariance claim is false, and **neither** D-EDGE option repairs it

This is the most serious finding. The plan's headline promise is *"end-to-end output unchanged (byte-identical)"*, resting on `perfect = 0 ⇒ max(1, 0 − scMin) ≡ abs(scMin)`. That identity needs **two** conditions, not one:

1. `−scMin ≥ 1` (the plan's D-EDGE — it caught this), **and**
2. `scMin ≤ 0` (the plan does **not** mention this at all).

`--score_min L,<i>,<s>` is validated **shape-only** (`options.rs:405-410`: non-empty strings, no numeric check) and parsed into arbitrary `f64` (`options.rs:390-396`). So both conditions are user-breakable at *any* read length. I evaluated old vs. new over a grid of legal parameter sets:

| `--score_min` | len | AS | 2nd | old | new (universal clamp) | new (**local-only clamp** = plan's fallback) |
|---|---|---|---|---|---|---|
| `L,0,0` | 100 | 0 | none | 42 | **0** | 42 ✅ |
| `L,0,0` | 100 | −3 | −3 | 33 | **0** | 33 ✅ |
| `L,0,-0.005` | 100 | 0 | none | 42 | **0** | 42 ✅ |
| `L,-0.5,0` | 100 | 0 | none | 42 | **23** | 42 ✅ |
| `L,0,0.2` | 100 | 0 | −1 | 2 | **33** | **39** ❌ |
| `L,5,0.1` | 100 | 0 | −3 | 0 | **33** | **39** ❌ |
| `L,0,0.2` | 100 | −1 | −5 | 0 | **33** | **33** ❌ |

Two distinct families:

- **`−1 < scMin ≤ 0`** (e.g. `L,0,0` = "require a perfect alignment" — an entirely plausible user request; `L,-0.5,0`; any tiny slope). The clamp fires → the **universal** clamp diverges. The plan's local-only fallback *does* fix this family.
- **`scMin > 0`** (positive intercept/slope). Here `0 − scMin` is negative while `abs(scMin)` is positive — the two expressions differ **regardless of the clamp**. The plan's fallback does **not** fix this family.

So the plan's D-EDGE resolution rule is incomplete: *both* of its branches can break end-to-end byte-identity on legal input. And **V1 as specified would not catch it** — V1 sweeps `len ∈ 1..=500 × as_best × {None, Some}` with (implicitly) the default params only. It would go green and the divergence would ship.

**Recommendation** (follows from the plan's own stated priority, "end-to-end byte-identity outranks upstream mimicry"): stop treating D-EDGE as an open in-flight decision and make the denominator mode-dispatched:

```rust
let diff = if model.local_ladder {
    (model.perfect(read1_len, read2_len) - sc_min).max(1.0)   // Bowtie 2 unique.h
} else {
    sc_min.abs()                                              // frozen end-to-end
};
```

That is byte-identical end-to-end **by construction** — no sweep needed to prove it — and it is exactly as faithful to Bowtie 2 in local mode. Keep V1 anyway as a regression net, but add the `(intercept, slope)` and `read2_len` axes to it.

If instead you *want* end-to-end to follow Bowtie 2 for these exotic parameters, that is a defensible second option — but it is a **second behaviour change** outside the locked scope and needs saying out loud in the CHANGELOG, not discovering during V1.

### 🔴 C2 — V2 and V8 cannot reach "agreement": Bowtie 2's `scMin` is an **integer**

`unique.h:213`:

```cpp
TAlScore scMin = scoreMin_.f<TAlScore>((float)rdlen);
```

`TAlScore` is `int64_t`, and `SimpleFunc::f<T>` ends with `return (T)ret;` (`simple_func.h:110`) — so Bowtie 2 **truncates** `scMin` (and note the `(float)rdlen` cast before `log`). Bismark and Perl both keep `f64`. `diff` and `bestOver` inherit the difference.

I quantified it. Bowtie 2-local `G,20,8`, SE, no second-best, `len ∈ 25..300` × every legal `AS`: **562 cells disagree** between the f64 model and a faithful integer transcription — always 1–2 rungs, always at a bucket boundary:

```
len=25 AS=47  bismark=22  bowtie2=28
len=25 AS=49  bismark=42  bowtie2=44
len=28 AS=50  bismark=24  bowtie2=28
len=28 AS=53  bismark=41  bowtie2=42
```

Consequences:

- **V2** says "Transcribe `unique.h:206-222` into a test-only reference fn … Expected: Agreement across the grid." A *faithful* transcription will fail on a *correct* fix. V2 must state explicitly that the reference reproduces `unique.h`'s **structure** (`scPer`, `max(1, scPer − scMin)`) while keeping **Bismark's f64 `scMin`** — otherwise the load-bearing anti-tautology test is testing the wrong thing.
- **V8** ("Expected: Agreement") will show real disagreements that are **not** bugs in this fix. Its pass criterion needs to be either "agrees except where `trunc(scMin) ≠ scMin` moves a bucket boundary", or "agrees after truncating `scMin` to `i64`". Otherwise V8 produces noise the implementer must relitigate mid-flight.

Adopting Bowtie 2's truncation to make V8 clean is **not** an option — `scMin = −0.2 × len` is fractional for most end-to-end lengths, so truncating would break the byte-frozen default path. Record it as considered-and-rejected so V2/V8's expectations are principled rather than improvised.

### 🔴 C3 — Nothing, at any level, exercises Bowtie 2 `--local` MAPQ — so the `aligner` wiring is unvalidated

This is the plan's biggest silent-failure hole. The fix's entire effect hinges on `ScoreModel::new` receiving `aligner == Bowtie2` together with `local == true`. But:

- The only `--local` integration tests are **HISAT2** (`aligner_cli.rs:2034`, `:2116`) and they assert no MAPQ.
- `aligner_methylseq_conformance.rs:196-203` checks only the emitted **option string**.
- `combined.rs` can never be local (`--local` + `--combined_index` rejected, `config.rs:535`), so `match_bonus` there is always `0`.
- V3 is a unit test on `calc_mapq` — it proves the formula, not the plumbing.

If the wrong `aligner` reaches `ScoreModel::new` (wrong enum arm, model built too early, a future refactor that drops the argument), `match_bonus` silently becomes `0.0`, the fix becomes a **no-op**, and **V1, V3, V7 and the whole suite still pass**. That is exactly the failure shape that let #1079 ship in the first place.

**Add two validations:**

- **V9** — `config::resolve` assertions on the resolved model: `--local --bowtie2` → `match_bonus == 2.0 && log_score_min && local_ladder`; `--local --hisat2` → `0.0 && !log_score_min && local_ladder`; default (each of the 4 aligners) → `0.0 && !log_score_min && !local_ladder`.
- **V10** — one fake-Bowtie 2 `--local` CLI test asserting the **BAM MAPQ column**. Constructible with the existing 8 bp fixture genome by passing `--score_min G,1,0` (constant `scMin = 1`; `alwaysPositive` ✓ so it is a legal Bowtie 2-local function): perfect `= 12`, `diff = 11`, so the ladder is exercised on a 6 bp read. This is the only test that would prove config → merge → output carries a `match_bonus = 2.0` model.

### 🟠 I1 — `mod.rs` is missing from the plan entirely (6 sites, 2 of them function pointers)

The Files table lists `mapq.rs`, `config.rs`, `options.rs`, `merge.rs`, `combined.rs`, CHANGELOG, VERSION. `mod.rs` passes the trio at **six** places:

`mod.rs:2599` (SE merge) · `3146` (SE combined, via `select_fn`) · `3528` (`select_nondir`) · `4647` (PE merge) · `5255` (PE combined, via `select_fn`) · `5705` (`select_pe_nondir`)

Two of those (`3146`, `5255`) go through **`select_fn` function pointers**, so collapsing the trio changes a fn-pointer *type*, not just an argument list. Compiler-caught, but an implementer working the plan's site list will be surprised by the largest file in the module.

### 🟠 I2 — `combined.rs` enumeration is incomplete: 8 threading signatures, not 6; 4 call sites, not 3

Actual parameter declarations: `188` (`select`) · `246` (`select_core`) · `388` (`select_nondir`) · `437` (`select_pbat`) · `501` (`select_pe`) · `543` (`select_pe_nondir`) · **`598` (`select_pe_pbat`)** · **`657` (`select_core_pe`)**.

The plan lists the first six and calls them "5 further threading sites". The two it misses matter: `select_core_pe` is the **shared PE core**, and it holds `calc_mapq` **call site `combined.rs:774`** — which the plan never names (§2 says "call sites L349 etc."; §11 says "all three call sites"). Production call sites are **four**: `merge.rs:370`, `merge.rs:747`, `combined.rs:349`, `combined.rs:774`. Plus a test call site at `combined.rs:1205`.

Arity check on the plan's one concrete clippy claim: `select_pe_nondir` is 8 args → 6 after collapsing. Clippy's threshold is >7, so deleting the `#[allow]` at `combined.rs:535` is correct ✅. (`merge.rs:190`'s allow also becomes unnecessary: 9 → 7. `merge.rs:519` stays at 11 → 9 and still needs it.)

### 🟠 I3 — §2's design premise is factually wrong

> "**No call site knows which aligner is running**"

`merge::check_results_paired_end` **already takes `aligner: Aligner`** (`merge.rs:530`), fed from `config.aligner` at `mod.rs:4650`. The claim holds for the SE and combined paths only.

The `ScoreModel` decision is still right — it drops arity instead of raising it, and hand-threading `aligner` into the SE + 8 combined signatures would be worse. But the justification of record should be "net arity reduction + one construction point", not a false premise. (It also means the cheap alternative was cheaper than the plan implies — see Alternatives.)

### 🟠 I4 — Step 4 under-scopes D2's test fallout

The plan flags only `mapq.rs:376` and `:416` (the tautological routing loops). But `local_hisat2_default_params_mapq` (`mapq.rs:390-419`) contains **five hand-derived, genuinely independent** assertions — its own doc-comment stresses they are "the Perl local ladder hand-applied … **NOT** self-consistency" — and **all five change** once HISAT2-local `scMin` goes linear:

| Line | Call | old | new |
|---|---|---|---|
| 397 | `calc_mapq(50, None, -1, None, 0, -0.2, true)` | 22 | **44** |
| 401 | `calc_mapq(50, None, 0, Some(-1), …)` | 40 | **31** |
| 402 | `calc_mapq(50, None, -1, Some(-1), …)` | 0 | **1** |
| 410 | `calc_mapq(150, Some(150), 0, Some(-1), …)` | 34 | **11** |
| 412-418 | routing loop | tautological | rewrite |

These must be **re-derived by hand from the linear `scMin`** (preserving what the test was built to do), not re-baselined from the implementation — the plan's own §11 rationale for the tautology applies verbatim here. Note also that after D2 this test's stated purpose ("targets the `ln()`-ULP-sensitive regime the `(20,8)` tests never reach") **evaporates**: with HISAT2 linear and Bowtie 2 the only `ln()` consumer, the `ln()`-derived-boundary coverage needs re-homing onto a Bowtie 2-local case.

### 🟠 I5 — The same D1 defect exists, unfixed, in the **default** path for `--minimap2` / `--rammap`

minimap2 and rammap emit **positive** `AS:i:` — the repo's own fixtures say so (`aligner_cli.rs:2463`: "minimap2-style tags: a **positive** `AS:i:`"). They route through the same `run_se`/`run_pe` → `calc_mapq` with `local = false`, so `perfect = 0` and `diff = abs(scMin) = 0.2·len`, giving

```
bestOver/diff = (AS + 0.2·len) / (0.2·len) = AS/(0.2·len) + 1  >  1
```

i.e. the **top rung for essentially every unique minimap2/rammap hit** — the identical defect class as #1079, in a non-`--local` path, today.

`ScoreModel::new`'s `else { 0.0 }` doesn't just leave this alone; it **codifies** `perfect = 0` for minimap2/rammap in the new type. Since minimap2 SE is byte-frozen against Perl v0.25.1 + minimap2 (per `rust/README.md:161`), deferring is legitimate — but it deserves the same explicit treatment as A3/HISAT2: an assumption row and a deferred issue, not silence. Otherwise the next person reads `match_bonus = 0.0` for `Minimap2` as a verified fact.

### 🟠 I6 — `ScoreModel` can be built in a state the aligners cannot produce

`log_score_min` and `match_bonus != 0` are **the same predicate** (`local && aligner == Bowtie2`). With all five fields `pub`, a hand-built test model — and tests *will* hand-build them — can set one without the other, and nothing catches it. Prefer an enum:

```rust
enum ScoreModel { EndToEnd { i, s }, Bowtie2Local { i, s }, Hisat2Local { i, s } }
```

or private fields with `new()` as the only constructor.

Related, and worth recording in the type's docs: after D2, HISAT2-local is `local_ladder = true, match_bonus = 0.0` — a combination **Bowtie 2 itself can never produce**, because Bowtie 2 selects the ladder from `sc_.monotone`, i.e. *from the match bonus* (`unique.h:335` `if(sc_.monotone) … else // Local alignment`). Monotone scoring ⇒ end-to-end ladder, always. Bismark's split is the faithful-to-Perl choice and is exactly what deferred item A3 is about — so make it the in-code marker for that deferred issue rather than an implicit oddity.

### 🟠 I7 — V6 tests the wrong quantity, and there is no division to protect

The clamp fires when **`perfect − scMin ≤ 1`**, not when `scMin ≈ 0`. And "scMin → 0⁻ in local mode" is not a reachable Bowtie 2-local state at all: `bt2_search.cpp:1861` makes Bowtie 2 **die** —

> "Error: the match penalty is greater than 0 … but the `--score-min` function can be less than or equal to zero. Either let the match penalty be 0 or make `--score-min` always positive."

— whenever the match bonus is >0. So Bowtie 2-local `scMin` is **guaranteed positive** (a useful corollary: the old `abs(scMin) ≡ scMin` there, and the new `diff` is the only thing that changes).

The real clamp case is a **short read**: `len = 6, G,20,8` → `perfect = 12`, `scMin = 34.3` → `max(1, −22.3) = 1`. Practically it is unreachable in real data (Bowtie 2 reports nothing when the minimum score exceeds the perfect score, which for `G,20,8` means `len ≲ 22`), but it is trivially reachable in fake-aligner fixtures — which is precisely where V10 above will live.

Also: §3's *"Guards a division-by-~0"* and V6's *"no div-by-zero, no panic"* mis-describe the code. There is **no division** in either ladder — every comparison is `best_over >= diff * k`. The `diff = 0` failure mode is that every threshold collapses to `>= 0` and the **top rung always wins** (silently wrong MAPQ), not a panic. Worth fixing, because it shows the edge case was reasoned about in the wrong shape.

### 🟠 I8 — Release-mechanics gaps in step 7

- **Three** literals mirror the version, not one: `rust/VERSION`, `rust/bismark/VERSION`, `rust/bismark/Cargo.toml:3`. The plan bumps only the first.
- **`rust/README.md` is absent from the plan.** Its own rule (`rust/README.md:175`) is that *every* module-merge PR into `master` updates that tool's row **and** adds a dated Milestones line. A user-visible MAPQ change in the aligner clearly qualifies.
- There is exactly one `CHANGELOG.md` (repo root) — no `rust/CHANGELOG.md`. Worth pinning the path.
- **Question worth asking before implementing:** should a fix PR bump the version at all, or does the bump belong to the release cut? (3.1.0 → 3.2.0 is the right *magnitude* if it is bumped here.)

---

## 2. Assumptions

| # | Plan's assumption | My finding |
|---|---|---|
| A1 | perfect = `2 × len`, PE summed | ✅ verified upstream |
| A2 | `G` is logarithmic; keep `ln()` | ✅ verified; the plan's best catch |
| A3 | HISAT2 perfect = 0 pending investigation | ⚠️ Probably *not* actually unknown — see O3. Locked as deferred; just don't over-hedge the CHANGELOG if one command settles it |
| A4 | `--ma` unreachable ⇒ `2.0` constant | ✅ verified today. The plan's caveat is right; consider a one-line doc note on the constant so a future `--ma` passthrough trips over it |
| A5 | Both ladders correct, untouched | ✅ local ladder diffed leaf-by-leaf against `unique.h` |
| A6 | `bestOver` unchanged | ✅ and correct to insist on — see O4 for why it now *means* something |
| A7 | Local not covered by perl-oracle | ✅ verified |
| — | **Unstated:** `scMin ≤ 0` end-to-end | 🔴 **C1** — false for legal `--score_min` |
| — | **Unstated:** the `len ≥ 5` threshold assumes `(0, −0.2)` | 🔴 folded into **C1**. `0.2 × len ≥ 1` silently hardcodes the default slope |
| — | **Unstated:** Bowtie 2's `scMin` is `f64` | 🔴 **C2** — it is `int64_t` |
| — | **Unstated:** minimap2/rammap perfect = 0 | 🟠 **I5** — false; positive AS, same defect |
| — | **Unstated:** 5-Base paths don't recompute MAPQ | ✅ verified (`five_base_deconv.rs` / `five_base_duplex.rs` never call `calc_mapq`; 5-Base is minimap2, where `--local` is rejected). Worth one line so a reader needn't check |

---

## 3. Efficiency

Nothing to worry about, and the plan's read is right: `calc_mapq` runs once per accepted alignment, and `diff` adds one multiply + one compare. Arity drops by 2 at ~10 sites. Two nits:

- **O1** — `ScoreModel` is 3×`f64` + 2×`bool` = **32 bytes** with padding, not 40. More to the point, it derives `Copy` but the signature takes `&ScoreModel`; pick one. By value is simpler and the same cost at this size (clippy's `trivially_copy_pass_by_ref` only fires ≤ 8 bytes, so neither form is linted — this is a readability call, not a lint).
- **O2** — `diff(&self, sc_min, read1_len, read2_len)` makes the caller feed back a value the model can derive, and leaves a caller free to pass a `sc_min` computed some other way. A single `fn normalize(&self, len1, len2, as_best) -> (f64 /*best_over*/, f64 /*diff*/)` is harder to misuse and keeps §3.5's "`bestOver` unchanged" structurally enforced.

---

## 4. Validation sufficiency

The three load-bearing validations are correctly identified (V1/V2/V7) but **two of the three do not currently do their job**, and the highest-risk failure mode has no test at all.

| # | Assessment |
|---|---|
| **V1** | 🔴 Insufficient. Add the `(intercept, slope)` axis and a `read2_len` axis, or it goes green while shipping the C1 divergence. Better: make the divergence unreachable (C1's mode-dispatched `diff`) and keep V1 as a net |
| **V2** | 🔴 Expectation unachievable as worded (C2). Must say "structure of `unique.h`, Bismark's f64 `scMin`" |
| **V3** | ✅ Correct value, verified. But unit-level only — does not prove the wiring (C3) |
| **V4** | ✅ Good, and it guards the right bug (single-mate perfect score). `2 × 200 = 400` is right |
| **V5** | ✅ Good. Also assert the **converse**: `scMin != −0.2 × ln(len)`, so a revert to `ln` fails loudly |
| **V6** | 🟠 Tests an unreachable state and mis-names the failure mode (I7). Restate as `len = 6, G,20,8 → diff == 1.0`, and assert *which rung* results (the risk is a silently-wrong top rung, not a panic) |
| **V7** | ✅ Right instinct, wrong wording — see O7 |
| **V8** | 🟠 Constructible (the plan's triage caveats about MAPQ 255 and cross-instance second-best are the right ones), but its pass criterion must account for C2's integer `scMin` or it will report false failures. The plan's "report if it can't be built rather than quietly drop it" is exactly the right discipline |
| **V9/V10** | 🔴 **Missing.** The config→model and model→BAM wiring (C3) |

**O7** — V7 says "All green with **zero test edits**." Literally false: `combined.rs:1205` and the `mapq.rs` local tests must construct a `ScoreModel` instead of passing three scalars. Reword to "**zero expected-value changes**" — the distinction is the whole point of the step, and an implementer who reads it literally will think the refactor went wrong.

---

## 5. Alternatives

| Option | Trade-off | Verdict |
|---|---|---|
| **A. Enum `ScoreModel`** (`EndToEnd`/`Bowtie2Local`/`Hisat2Local`) | Makes the invalid `log_score_min ⊻ match_bonus` state unrepresentable; the three-way match reads as the §2 table | **Recommended** over the 5-pub-field struct (I6) |
| **B. Keep the trio, add `aligner`** | Cheaper than the plan implies — PE already has it (I3) — but pushes SE + 8 combined signatures up and *deepens* the `too_many_arguments` debt | Correctly rejected |
| **C. Mode-dispatched `diff`** (`abs()` when `!local`) | Removes C1 entirely; end-to-end identity by construction, no sweep needed | **Recommended** — supersedes D-EDGE's either/or |
| **D. `fn perfect(&self, len) -> f64` instead of a `match_bonus` field** | Lets minimap2/rammap (I5) plug in their own match score later with no signature churn; costs nothing now | Worth taking |
| **E. Truncate `scMin` to `i64` to match Bowtie 2 exactly** | Would make V8 clean, but breaks end-to-end byte-identity (`−0.2 × len` is fractional) | Reject — but **record it as considered-and-rejected** so V2/V8's expectations are principled |
| **F. Compat flag / opt-in** | Explicitly locked out by Felix | Not revisited |

---

## 6. Action items

### Critical — resolve before implementation

1. **C1** — Make `diff` mode-dispatched (`sc_min.abs()` when `!local_ladder`); demote D-EDGE from an open in-flight decision to a decided one. **Or** accept that end-to-end changes for `scMin > 0` and `−1 < scMin ≤ 0` and say so in the CHANGELOG. Either way, add `(intercept, slope)` and `read2_len` axes to V1 — as written it cannot see this class of divergence. Concrete counterexamples: `L,0,0` @100bp AS 0 no-2nd (42→0 universal) and `L,0,0.2` @100bp AS 0 2nd −1 (2→39 even local-only).
2. **C2** — Restate V2's reference as "`unique.h` **structure** + Bismark's f64 `scMin`", and give V8 a pass criterion that tolerates Bowtie 2's `int64_t` truncation (562 boundary cells disagree over `len ∈ 25..300` SE alone). Record option E as rejected.
3. **C3** — Add **V9** (`config::resolve` → resolved-model assertions for all four aligners × `--local`) and **V10** (fake-Bowtie 2 `--local` CLI test asserting the BAM MAPQ; `--score_min G,1,0` makes this work on the existing 8 bp fixture). Without these, a mis-wired `aligner` makes the fix a silent no-op with a fully green suite.

### Important — fix in the plan before handing to an implementer

4. **I1** — Add `mod.rs` to the Files table with its 6 sites (2599, 3146, 3528, 4647, 5255, 5705); call out the two `select_fn` **function-pointer types** at 3146/5255.
5. **I2** — Correct the `combined.rs` enumeration to **8** signatures (add 598, 657) and **4** production call sites (add `combined.rs:774`) + the test call site at 1205. Fix §11's "all three call sites".
6. **I3** — Drop the false "no call site knows which aligner" premise (`merge.rs:530` already takes `aligner`); rejustify `ScoreModel` on arity reduction + single construction point.
7. **I4** — Extend step 5 to the **five hand-derived** HISAT2 assertions at `mapq.rs:397, 401, 402, 410` (with the new expected values), and re-home the `ln()`-boundary coverage that test provided onto a Bowtie 2-local case.
8. **I5** — Add an assumption row + a deferred issue for **minimap2/rammap**: positive AS through `perfect = 0` is the same D1 defect in a default path, currently held byte-frozen. Do not let `else { 0.0 }` read as verified.
9. **I6** — Make the invalid `ScoreModel` state unrepresentable (enum, or private fields + `new()`); document `local_ladder && match_bonus == 0` as the HISAT2 deferred marker.
10. **I7** — Rewrite V6 around `perfect − scMin ≤ 1` (`len = 6, G,20,8`), note that Bowtie 2-local `scMin` is guaranteed positive (`bt2_search.cpp:1861`), and drop the "division by zero" framing — there is no division; the failure mode is a silent top rung.
11. **I8** — Bump **all three** version literals (`rust/VERSION`, `rust/bismark/VERSION`, `rust/bismark/Cargo.toml:3`); add `rust/README.md` (aligner row + dated Milestones line, per its own rule at :175); pin the CHANGELOG path (repo root, the only one). Confirm whether the bump belongs in this PR or the release cut.

### Optional

12. **O1/O2** — 32 bytes not 40; pick `Copy`-by-value or `&`, not both; consider `normalize()` over `diff(sc_min, …)`.
13. **O3** — A3 may be settleable in one command: HISAT2 has **no `--local` mode** (Bismark's "HISAT2-local" is just dropping `--no-softclip`) and its scoring is monotone, so `perfect = 0` is likely *correct* rather than provisional. `hisat2 --help | grep -- '--ma'` would confirm. Scope stays deferred — just don't over-hedge the CHANGELOG if it turns out to be knowable.
14. **O4** — Worth one CHANGELOG line: the fix makes the `bestOver == diff` rungs (39/35/34/…) *meaningful* — they now mean `AS == perfect` instead of `AS == 2·scMin` — which is why they start appearing in local BAMs. The exact-`f64` comparison stays sound (integer `AS`/`perfect`, common `scMin`, minimum gap 1, result ULP ~1e-14), so the `float_cmp` allow keeps its justification; say so in the comment you are rewriting at `mapq.rs:43`.
15. **O5** — §7's "no internal consumer reads MAPQ" is too broad: `--five_base_min_mapq` **is** an internal MAPQ filter. The conclusion still holds (5-Base is minimap2, `--local` rejected) — narrow the sentence.
16. **O6** — One line noting 5-Base never recomputes MAPQ, verified, so the reader needn't check.
17. **O7** — V7: "zero **expected-value** edits", not "zero test edits".

---

## 7. Bottom line

The diagnosis is right, the upstream reading is right, the `ScoreModel` direction is right, and the no-op-then-fix ordering is right. Two of the plan's self-review catches (Bowtie 2-local `ln()` is *correct*; the local tests are tautological) are genuinely good and I confirmed both.

What is not yet safe is the **promise**. "End-to-end is byte-identical" is asserted from an incomplete algebraic condition (C1) and guarded by a sweep that cannot see the counterexamples; the two tests meant to replace the tautology are measured against an oracle that differs from Bismark for reasons unrelated to the fix (C2); and the one thing most likely to go wrong — the aligner never reaching the model — has no test anywhere (C3).

Fix those three in the plan text and this is ready to implement.

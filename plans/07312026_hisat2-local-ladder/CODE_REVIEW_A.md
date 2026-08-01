# CODE REVIEW A — HISAT2 `--local` uses the end-to-end MAPQ ladder (#1080)

**Reviewer:** A (independent, fresh context)
**Target:** branch `rust/hisat2-local-ladder`, commit `875d6b8` (local), base `dev` = `ddc7633`
**Files reviewed:** `rust/bismark/src/aligner/config.rs`, `rust/bismark/src/aligner/mapq.rs`, `CHANGELOG.md`, `docs/src/content/docs/options/alignment.md`, `bismark` (Perl help), `plans/07312026_hisat2-local-ladder/PLAN.md`
**Upstream cross-checked:** HISAT2 `hisat2.cpp`, `scoring.h`, `unique.h`, `simple_func.h` (master); Bowtie 2 `unique.h` @ `v2.5.5`

**Verdict: the change is correct and I would ship it.** The central claim holds under independent verification, the derivation is exactly upstream's, the nine recomputed expectations are right, and the scope claim ("only HISAT2-local changes") is right. No Critical or High *correctness* findings. One High finding is a **documentation + test gap** around the ladder's *floor*, which is the largest user-visible consequence of the commit and is currently unmentioned and unexercised. The rest is documentation accuracy and naming.

**Reproduced green state:** `cargo fmt -p bismark -- --check` clean; `cargo clippy -p bismark --all-targets` 0 warnings; `cargo test -p bismark` **2110 passed / 0 failed**.

---

## 1. Independent verification of the load-bearing claims

Everything in this section I re-derived from upstream source or by hand, not from the diff's own comments.

### 1.1 Does HISAT2 *always* have match bonus 0? — **Yes, verified**

| Fact | Evidence |
|---|---|
| `--local` is not a live option | `hisat2.cpp:639` — the option-table entry is commented out. `case ARG_LOCAL` survives at `:1420` but is unreachable. |
| The four local presets are not live either | `hisat2.cpp:665-668` commented out (their `case` arms at `:1444-1458` are unreachable). |
| `-P/--preset` **is** live (`:629`) but cannot set `localAlign` | `presets.cpp` `sensitive-local` etc. only emit `-M/-N/-L/-i` policy tokens; `localAlign` is a separate static bool set only by `ARG_LOCAL`/`ARG_BWA_SW_LIKE`/the dead presets. So even `-P sensitive-local` leaves `localAlign == false`. |
| `--ma` **is** live (`:649` → `polstr ;MA=`) | but `hisat2.cpp:3916-3919` forces `bonusMatch = 0` with a warning whenever `!localAlign`, *before* `Scoring` is constructed at `:3922`. |
| `--bwa-sw-like` (`:647`) is the only live path to a nonzero bonus | `:1136-1137` sets `bwaSwLike = true; localAlign = true`, `:1143` appends `;MA=1`. |
| Bismark never passes it | `options::build_aligner_options` composes a closed option string; there is no arbitrary-passthrough flag. Confirmed by grep: no occurrence of `bwa-sw-like` anywhere in the repo. |

A3 in the plan is sound, and the code comment at `config.rs:117-119` citing `hisat2.cpp:3916` is accurate.

### 1.2 Is `monotone` really equivalent to `match_bonus > 0.0`? — **Yes, and more strongly than the plan claims**

The prompt flags `monotone = matchType == COST_MODEL_CONSTANT && matchConst == 0` (HISAT2 `scoring.h:181`) as a possible hole — a non-constant `matchType` would make `monotone` false even at zero bonus. It cannot happen:

- `scoring.h:163` — the `Scoring` constructor **hard-codes** `matchType = COST_MODEL_CONSTANT`, *silently ignoring* the `mat`-type notion entirely (there is no match-type parameter; `mat` is only the constant).
- `scoring.h:197` — `setMatchBonus()` likewise re-asserts `matchType = COST_MODEL_CONSTANT`.
- `scoring.h:227` — `assert_geq(matchConst, 0)`.

So `monotone ⟺ matchConst == 0 ⟺ !(match_bonus > 0)`. Deriving from `match_bonus` alone is not merely "close enough" — it is exact, and no caller can construct a counterexample. Same reduction holds for Bowtie 2.

Corroborating: `scoring.h:340-346` `perfectScore()` returns `0` iff `monotone`, else `rdlen * match(30)` = `rdlen * matchConst` — which is precisely `ScoreModel::perfect()` (`config.rs:190-192`).

### 1.3 Is the ladder really selected by `monotone`, and is it *this* ladder? — **Yes**

- HISAT2 `unique.h:236` `if(sc_.monotone)` → end-to-end rungs; `else` → local rungs. Bowtie 2 v2.5.5 `unique.h:223`, identical structure.
- HISAT2's default `mapqv = 2` (`hisat2.cpp:480`) selects `BowtieMapq2` (`unique.h:170`), i.e. the ladder class — **not** `BowtieMapq3` (`unique.h:95`), which is a bin-table scheme. So the ladder really is what HISAT2 would run.
- I diffed HISAT2's `BowtieMapq2::mapq` against Bowtie 2 v2.5.5's line by line: **every rung value is identical in both ladders**; the only differences are the `equalSecbest` gate, the `60`-vs-`255` early return, the missing `max(1,…)` on `diff`, and accessor names. So "the end-to-end ladder HISAT2 would use" is the same ladder Perl/Rust already implements.
- `mapq.rs:43-130` (end-to-end) matches Bowtie 2 `unique.h:226-330` rung for rung; `mapq.rs:138-215` (local) matches `unique.h:336-378`. Both untouched by this commit, as claimed (A4).

### 1.4 The nine recomputed expectations — **all nine correct**

Re-derived by hand from `scMin = -0.2·len` (summed over mates), `diff = |scMin|`, `bestOver = AS + 0.2·len`, applying the end-to-end ladder:

| Cell | diff | bestOver | bestDiff | route | **after** | claimed | `(was …)` | verified |
|---|---|---|---|---|---|---|---|---|
| `50,None,0,None` | 10 | 10 | — | `≥0.8·diff` | 42 | 42 ✓ | 44 | ✓ |
| `50,None,-1,None` | 10 | 9 | — | `≥0.8·diff` (0.9) | 42 | 42 ✓ | 44 | ✓ |
| `150,None,0,None` | 30 | 30 | — | `≥0.8·diff` | 42 | 42 ✓ | 44 | ✓ |
| `50,None,0,Some(-1)` | 10 | 10 | 1 | `≥0.1·diff`, `==diff` | 30 | 30 ✓ | 31 | ✓ |
| `50,None,-1,Some(-1)` | 10 | 9 | 0 | `bestDiff==0`, `9≥6.7` | 1 | 1 ✓ | 1 | ✓ |
| `150,Some(150),0,Some(-1)` | 60 | 60 | 1 | `bestDiff>0`, `60≥40.2` | 6 | 6 ✓ | 11 | ✓ |
| `100,None,0,Some(-3)` | 20 | 20 | 3 | `≥0.1·diff=2`, `==diff` | 30 | 30 ✓ | 31 | ✓ |
| `100,None,0,Some(-5)` | 20 | 20 | 5 | `≥0.2·diff=4`, `==diff` | 31 | 31 ✓ | 32 | ✓ |
| `100,None,0,Some(-1)` | 20 | 20 | 1 | `bestDiff>0`, `20≥13.4` | 6 | 6 ✓ | 11 | ✓ |

Every `(was N)` annotation in the test comments is also correct — I re-ran each cell through the local ladder and got 44/44/44/31/1/11/31/32/11.

**The `1` control cell claim is right.** `(50, None, -1, Some(-1))`: `bestDiff = 0` in both ladders; end-to-end takes `bestOver ≥ diff·0.67 = 6.7` → 1 (`mapq.rs:125-126`), local takes `bestOver ≥ diff·0.5 = 5` → 1 (`mapq.rs:210-211`). `bestOver = 9` clears both. Genuinely ladder-invariant, and a good control.

**The float knife-edge comment is right.** `10.0 * 0.1` is exactly `1.0` in IEEE-754 (the product `1.0000000000000000555…` is within half an ULP of 1.0). Likewise `20.0*0.1 == 2.0` and `20.0*0.2 == 4.0`, so cells 7/8 have their stated integer margins.

### 1.5 Is anything other than HISAT2-local affected? — **No**

- `config.rs:667-689` rejects `--local` for `Minimap2`/`Rammap` and for all four `--combined_index` variants, so `match_bonus` is structurally 0 on those paths ⇒ end-to-end ladder, unchanged. The plan's edge-case table is accurate.
- `calc_mapq` has exactly four callers: `merge.rs:367`, `merge.rs:740`, `combined.rs:339`, `combined.rs:711`. No 5-Base call site — A4/§3's "5-Base never calls `calc_mapq`" verified by grep, not assumed.
- Bowtie 2-local keeps `match_bonus = 2` ⇒ local ladder; every end-to-end model has `match_bonus = 0` ⇒ same branch, same arithmetic. `end_to_end_matches_the_pre_fix_formula` (6 `--score_min` cells × `len 1..=500` × SE/PE) and the three Bowtie 2-local tests pass untouched, so V2/V3 hold.
- **The deleted field had no other readers.** `local_ladder` was only ever consumed through the accessor, and the accessor is called at exactly one non-test site (`mapq.rs:33`). Grep for `local_ladder`/`match_bonus`/`from_emitted` outside the two changed files returns nothing but unrelated test *names*.
- The only MAPQ-threshold filter inside the suite is `--five_base_min_mapq` (`mod.rs:2060`), which is 5-Base-only and therefore untouched.

### 1.6 Is deleting the field the right call? — **Yes, keep it**

The stored flag could represent `(local ladder, perfect score 0)` — a combination neither upstream aligner can produce, and precisely the state that *was* the bug. Deriving makes it unrepresentable, and it mirrors `unique.h` exactly rather than paraphrasing it. The `#1081` coupling is documented in three places (accessor doc, commit message, plan A2) and hands #1081 a decision instead of pre-empting it silently. I agree with the call; see MEDIUM-4 for the one cheap hardening I would add.

---

## 2. Issues

### HIGH-1 — The ladder's *floor* moves 22 → 0, and that is neither documented nor tested (Logic / Errors)

This is the finding I would act on before merge.

For the **no-second-best** branch — the common unique-alignment case — the two ladders have different floors:

- local: `else { 22 }` (`mapq.rs:153-155`)
- end-to-end: `else { 0 }` (`mapq.rs:58-60`)

So a uniquely-aligned HISAT2-local read whose `bestOver/diff < 0.3` goes from **22 to 0**. Worked example (default `L,0,-0.2`, 50 bp, `AS:i:-8`, no `XS`): `diff = 10`, `bestOver = 2`, ratio 0.2 → end-to-end returns **0**, local returned **22**. Mid-ladder moves too: `AS:i:-5` → ratio 0.5 → **23** where local gave **36**.

Why it matters more than the ceiling change the CHANGELOG does describe:

1. **MAPQ 0 is semantically special downstream.** `samtools view -q 1`, nf-core/methylseq's MAPQ filters, and most user pipelines treat 0 as "discard". Before this commit, a uniquely-aligned HISAT2-local read could *never* be 0; now it can. That is a stronger version of exactly the risk recorded as A5 — and A5's discussion only mentions "the 44 ceiling".
2. **Nothing tests it.** All three no-second-best cells in `local_hisat2_uses_the_linear_form_and_end_to_end_ladder` sit on the top rung (42). The whole no-second-best sub-ladder that changed — `40/24/23/8/3/0` replacing `42/41/36/28/24/22` — is unexercised for HISAT2. The most consequential rung in the commit has no test.

**Recommended change** — two cells appended to `local_hisat2_uses_the_linear_form_and_end_to_end_ladder` (`mapq.rs`, after the `150,None,0,None` cell):

```rust
        // A mid rung and the FLOOR, the two most consequential moves: the local ladder's
        // no-second-best floor was 22, the end-to-end one is 0.
        // as_best -5 → best_over 5 = 0.5·diff → 23  (local ladder gave 36)
        assert_eq!(
            calc_mapq(50, None, -5, None, ScoreModel::hisat2_local(i, s)),
            23
        );
        // as_best -8 → best_over 2 = 0.2·diff, below every rung → 0  (local ladder gave 22).
        // A unique HISAT2-local alignment can now be MAPQ 0, which it never could before.
        assert_eq!(
            calc_mapq(50, None, -8, None, ScoreModel::hisat2_local(i, s)),
            0
        );
```

Both hand-derived, not read back from the implementation.

**And one clause in `CHANGELOG.md:13`**, after "instead of capping at 44 for no scoring reason":

> …instead of capping at 44 for no scoring reason. The floor moves the same way: a uniquely-aligned read that scored poorly used to bottom out at 22 and can now be 0, so `-q`/MAPQ filters downstream of `--hisat2 --local` will drop reads they previously kept.

### MEDIUM-2 — The CHANGELOG headline overclaims, and contradicts both the plan and the commit message (Structure / docs)

`CHANGELOG.md:13`: *"**HISAT2 `--local` MAPQ now matches what HISAT2 itself would compute**"*, and later *"Its perfect alignment score is confirmed to be 0 … so this is no longer provisional."*

The plan (A1, §1 "Honest limitation") and the commit message both say the opposite in substance: *"an argument from consistency with the score regime rather than from measurement"*. The previous bullet's hedge — "not yet claimed to match a HISAT2 reference" — has been dropped and replaced with a positive equivalence claim that no oracle backs. Beyond the missing measurement, Bismark's HISAT2-local MAPQ still differs from HISAT2's own `mapq()` in four concrete ways I verified in upstream source:

| Divergence | Upstream |
|---|---|
| HISAT2 computes `scMin` as an **integer** — `scoreMin_.f<TAlScore>()` and `simple_func.h:117` `return (T)ret;` truncates toward zero. At 51 bp HISAT2's `scMin` is `-10`; Bismark's is `-10.2`. Rungs shift. | `simple_func.h:86-118` |
| HISAT2's `diff = (scPer - scMin)` has **no `max(1,…)` clamp** (Bowtie 2 has one). Bismark's `abs(scMin)` coincides only while `scMin ≤ 0`. | hisat2 `unique.h:229` |
| HISAT2's "didn't look for a second-best" early return is **60**, not a ladder rung, and is gated on an extra `equalSecbest` condition Bismark has no analogue for. | hisat2 `unique.h:200-218` |
| Bismark aggregates best/second-best **across the 2–4 strand instances**, so the ladder inputs are not the ones HISAT2 saw. | `merge.rs:367`, `combined.rs:339` |

None of these is a defect of this commit — they are inherent to Bismark recomputing MAPQ — which is exactly why the headline should not promise equivalence. Suggested rewrite of the bold lead-in:

> **HISAT2 `--local` now uses the MAPQ ladder HISAT2 itself would select ([#1079](…), [#1080](…)).**

and change the closing sentence to:

> Its perfect alignment score is confirmed to be 0 — previously documented as "not exactly known". Bismark still recomputes MAPQ across the 2–4 strand instances rather than using HISAT2's own value, so this is consistency with HISAT2's scoring regime, not a byte-match against HISAT2 output.

The body of the bullet is otherwise accurate: I verified `-0.92` vs `-20` at 100 bp (`-0.2·ln(100) = -0.9210`, `-0.2·100 = -20`), the 44 ceiling, and the **22 → 42** example (3.1.0 gave 22: log `scMin = -0.782`, `bestOver = -0.218`, local floor → 22; the commit gives 42). Using 3.1.0 as the baseline for the example is the right choice for a release note.

### MEDIUM-3 — The Perl help edit describes behaviour the Perl script does not implement (Logic / docs)

`bismark:9731-9732` now reads:

> HISAT2 has no local mode of its own and always scores matches as 0, so its best possible alignment score is 0 and **MAPQ stays on the end-to-end scale.**

The Perl script is the *legacy, maintenance-freeze* implementation (README.md:13, :58-60; CHANGELOG.md:31 — still installable as `bismark=0.25.1`), and it keys **both** the `scMin` form and the ladder off `$local` alone, with no aligner test:

- `bismark:3934` — `my $scMin = … * ($local ? log $read1Len : $read1Len);`
- `bismark:4078` — `else{ ## Local alignment` (reached whenever `$local`, regardless of aligner)

So Perl `--hisat2 --local` still uses the logarithmic `scMin` **and** the local ladder — MAPQ does *not* stay on the end-to-end scale there. The frozen script now ships help text contradicting its own code. Note that #1083 (`ddc7633`), which made the structurally identical change, deliberately **did not touch** `bismark` at all (`git show ddc7633 -- bismark` is empty).

The first half of the sentence is a fact about HISAT2 and is fine to keep. Recommended edit at `bismark:9731-9732`:

```
                         end-to-end alignments. HISAT2 has no local mode of its own and always scores matches as 0,
                         so its best possible alignment score is 0.
```

…and put the MAPQ sentence where 3.x users actually read it — the Rust CLI's `--local` doc comment, `rust/bismark/src/aligner/cli.rs:303-306`, which currently says nothing about MAPQ at all:

```rust
    /// Local-alignment mode (soft-clipped ends). Bowtie 2 (`--local` + `--score-min G,20,8`)
    /// and HISAT2 (drops `--no-softclip` + L-form `--score-min L,0,-0.2`, no `--local` flag).
    /// minimap2 rejects it (local by design); `--combined_index` rejects it too.
    /// HISAT2 always scores matches as 0, so its MAPQ is identical to end-to-end (#1080).
```

As it stands the shipped help is silent on the change while the frozen legacy help asserts it — an inversion worth fixing while the sentences are being written anyway.

### MEDIUM-4 — One predicate, two meanings, now spelled twice (Structure)

`self.match_bonus > 0.0` appears at:

- `config.rs:169` — *"scoring is non-monotone"* ⇒ choose the local ladder
- `config.rs:212` — *"the perfect score is nonzero"* ⇒ use `max(1, perfect - scMin)` instead of `abs(scMin)`

These are two different upstream facts that happen to share a test today. #1081 cannot change one without silently changing the other, which is the coupling the commit deliberately accepts — but the coupling is currently *implicit in a duplicated literal* rather than named. The concrete trap: if #1081 decides minimap2 needs a nonzero perfect score for the **denominator** while keeping its ladder byte-frozen for parity, that shape is not expressible without re-splitting the field, and nothing in the code will say so at the point of edit.

Cheap hardening — name it once:

```rust
    /// Scores can only ever go down (Bowtie 2/HISAT2 `Scoring::monotone`, `scoring.h:181`,
    /// which reduces to a zero match bonus because `matchType` is always
    /// `COST_MODEL_CONSTANT`). Drives BOTH the ladder choice and the denominator.
    fn is_monotone(&self) -> bool {
        self.match_bonus == 0.0
    }
```

then `local_ladder()` becomes `!self.is_monotone()` and `normalize` branches on `if !self.is_monotone()`. Same behaviour, but the shared fact has a name and a single definition, and re-splitting later is a one-line change at a place a reader will look.

### LOW-5 — `unique.h:236` is HISAT2's line, not Bowtie 2's

`config.rs:160-161`, `mapq.rs:539-540` and the commit message all say *"exactly as Bowtie 2 and HISAT2 both do (`unique.h:236`, `if(sc_.monotone)`)"* — attributing one line number to two different files. In Bowtie 2 **v2.5.5** (the pinned parity version) `if(sc_.monotone)` is at `unique.h:223`; `:236` is HISAT2's. Since the surrounding comments are careful to cite `hisat2.cpp:3916` and `unique.h:218` precisely, this one should disambiguate too:

```
    /// Derived from the match bonus, exactly as Bowtie 2 and HISAT2 both do
    /// (`if(sc_.monotone)` — hisat2 `unique.h:236`, bowtie2 v2.5.5 `unique.h:223`): a zero
```

### LOW-6 — Stale test-name reference introduced by this commit's rename

`mapq.rs:718` still points at the old name:

> `/// values moved with D2, which is what `local_hisat2_uses_the_linear_form_it_was_emitted` pins.`

That test was renamed in this commit to `local_hisat2_uses_the_linear_form_and_end_to_end_ladder`. Update the reference (a grep confirms `local_hisat2_uses_the_linear_form_it_was_emitted` no longer exists anywhere).

### LOW-7 — `score_model_construction_matrix`'s HISAT2 row no longer distinguishes `--local`; pin the new invariant explicitly

After the change, `ScoreModel::hisat2_local(i, s) == ScoreModel::end_to_end(i, s)` for the same coefficients: `options.rs:377-381` gives HISAT2-local the *same* `L`-form and *same* `(0.0, -0.2)` default as end-to-end, and both have `match_bonus = 0`. Consequences:

1. `assert_eq!(m, ScoreModel::hisat2_local(0.0, -0.2))` at `mapq.rs:659` would pass identically if the model had been built with `local = false` — so that row of the matrix no longer has teeth against a mis-wired `--local`. (Not a regression in coverage of the *change* — `assert!(!m.local_ladder())` on the line above is the real assertion — but the `assert_eq!` reads stronger than it is.)
2. The user-visible fact is stronger than the docs currently say: **HISAT2 `--local` is now MAPQ-inert**, i.e. it yields *identical* MAPQ to end-to-end for the same `AS`/lengths, not merely "the same scale". That is also self-consistent with reality — the only thing `--local` changes in the emitted HISAT2 command line is dropping `--no-softclip` (`options.rs:342`, and `hisat2_local_option_string` pins that no `--local` is passed).

Two small changes:

```rust
        // HISAT2 --local is now MAPQ-inert: same L form, same default coefficients, zero
        // perfect score, end-to-end ladder ⇒ the model is *identical* to end-to-end. Pinned
        // so a future change to HISAT2-local's score-min default fails loudly here.
        assert_eq!(
            ScoreModel::hisat2_local(0.0, -0.2),
            ScoreModel::end_to_end(0.0, -0.2)
        );
```

and sharpen `docs/src/content/docs/options/alignment.md:111` from "MAPQ is reported on the same scale as end-to-end" to "MAPQ is identical to what end-to-end would report for the same alignment score". A reader told "the same scale" may still expect different numbers.

### LOW-8 — PLAN §8's efficiency claim is wrong (the struct does not shrink)

> "**Efficiency:** removes a field (32 → 24 bytes plus padding)"

`size_of::<ScoreModel>()` is **32 both before and after**. Three `f64`s (24 B) plus a one-byte `ScoreMinForm` at align 8 rounds to 32 either way; the removed `bool` sat in existing padding. Verified by compiling both layouts (`before=32 after=32`). The change is size- and runtime-neutral, not a win — worth correcting so the plan does not carry a measurement that a future reader might trust. (This is a plan-artifact fix only; nothing in the code claims it.)

### LOW-9 — The "make the invariant unrepresentable" rationale is now applied asymmetrically (design note, no action needed)

After the deletion, `form == Log ⟺ match_bonus > 0.0` in every reachable state — so `form` is as redundant *today* as `local_ladder` was, yet it is (correctly) kept independent. The distinction is real and worth one clause in the `ScoreModel` doc comment (`config.rs:88-92`): the **form** is Bismark's *emission* choice — which string was actually handed to the aligner — while the **ladder** follows upstream's own `monotone` test. Deriving the form would wrongly bind #1081's minimap2 case to the `G` form. Without that sentence, the rationale reads as inconsistently applied.

Relatedly, that doc comment says "keeping them consistent is `from_emitted`'s job" — `from_emitted` does not actually validate that `Log` only ever arrives with Bowtie 2-local; it takes `form` on trust from `score_min_params`. Pre-existing, and the construction matrix covers the reachable combinations, but the comment slightly oversells the enforcement.

### LOW-10 — `ScoreModel::hisat2_local` is `pub` but in-crate-test-only

Now that it constructs a model equal to `end_to_end`, its only value is naming intent in tests. `pub(crate)` would match `local_ladder()`'s visibility. Cosmetic, pre-existing.

---

## 3. Efficiency

Nothing to report. `local_ladder()` becomes one `f64` comparison instead of a bool load, inlined, evaluated once per record — immaterial. No allocation, no change in call graph. See LOW-8 for the corrected size claim.

---

## 4. Recommendations, prioritised

| Priority | Item | Where |
|---|---|---|
| **High** | Add the floor + mid-rung HISAT2 cells (`-8 → 0`, `-5 → 23`) and one CHANGELOG clause: the no-second-best floor moves 22 → 0, so unique alignments can now be MAPQ 0 | `mapq.rs` (in `local_hisat2_…_end_to_end_ladder`), `CHANGELOG.md:13` |
| **Medium** | Soften the CHANGELOG headline to "uses the MAPQ ladder HISAT2 itself would select" + restore the "consistency, not measurement" hedge the plan and commit message both carry | `CHANGELOG.md:13` |
| **Medium** | Trim "MAPQ stays on the end-to-end scale" from the frozen Perl help (Perl still uses the local ladder there); add the MAPQ sentence to the Rust CLI `--local` help instead | `bismark:9731-9732`, `rust/bismark/src/aligner/cli.rs:303-306` |
| **Medium** | Name the shared predicate once (`is_monotone()`), used by both `local_ladder()` and `normalize()` | `config.rs:169`, `config.rs:212` |
| **Low** | Disambiguate the `unique.h:236` citation (hisat2 `:236` / bowtie2 v2.5.5 `:223`) | `config.rs:160`, `mapq.rs:539` |
| **Low** | Fix the dangling old test name left by the rename | `mapq.rs:718` |
| **Low** | Pin `hisat2_local == end_to_end` explicitly; sharpen the docs from "same scale" to "identical" | `mapq.rs:659`, `docs/…/alignment.md:111` |
| **Low** | Correct PLAN §8's "32 → 24 bytes" (no size change; verified 32 → 32) | `PLAN.md:163` |
| **Low** | One clause on why `form` is *not* derived, unlike the ladder | `config.rs:88-92` |
| **Low** | `pub` → `pub(crate)` for `hisat2_local` | `config.rs:148` |

No source, test, doc or plan file was modified by this review.

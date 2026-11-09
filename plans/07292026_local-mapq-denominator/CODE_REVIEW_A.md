# CODE REVIEW A — local-mode MAPQ denominator (#1079)

**Reviewer:** A (independent, fresh context — no shared state with Reviewer B)
**Target:** uncommitted working-tree changes on `rust/local-mapq-denominator` (9 files, +730/−317), base `0a9eeb7`
**Plan:** `plans/07292026_local-mapq-denominator/PLAN.md` rev 1
**Report-only:** no source, test or plan file was modified by this review.

---

## 1. Summary

**Verdict: APPROVE for merge after the documentation fixes in §3 (E1–E5).** I found **no correctness
defect** in the shipped arithmetic, the ladder extraction, or the `(local, aligner)` → `ScoreModel`
wiring. Everything I could re-derive independently, I did — and it all held.

The findings are concentrated in **documentation and test hygiene**, and two of them are user-facing:
the CHANGELOG contradicts itself about HISAT2 `--local` (E1), the `mapq.rs` header now makes a false
byte-identity claim (E2), a comment in `options.rs` still describes the defect that was fixed (E3),
`rust/README.md`'s aligner **row** was not updated as the plan and the file's own rule require (E4),
and the CHANGELOG asserts two follow-ups are "tracked separately" while §12 records that neither
issue has been filed (E5).

### What I verified independently (green tests were the starting point, not the finding)

| # | Claim under review | How I checked it | Result |
|---|---|---|---|
| 1 | `calc_mapq_end_to_end` is a faithful extraction | whitespace-normalised text compare of the ladder block, `git show HEAD:…/mapq.rs` vs the new fn body | **byte-exact** — 1607 chars both sides, `IDENTICAL` |
| 2 | Both ladders untouched and correct | leaf-by-leaf vs upstream `unique.h` v2.5.5 L262–380 (fetched) | every rung + every sub-threshold matches (incl. e2e-0.3 → `32`, which *is* what upstream has) |
| 3 | **A1** `perfect = 2 × len`, PE sums both mates | `scoring.h:310-316` `perfectScore = monotone ? 0 : rdlen*match(30)`; `#define DEFAULT_MATCH_BONUS_LOCAL 2`; `unique.h:206-209` | confirmed |
| 4 | **A4** `--ma` unreachable ⇒ `2.0` is a constant | 23 `opts.push` sites, no `--ma`; no `trailing_var_arg`/`allow_hyphen_values` in `cli.rs` | confirmed |
| 5 | The 6 hand-derived HISAT2 expectations | re-derived from the linear `scMin` in an independent model | **44/44/44/31/1/11 all correct**; old values 44/22/44/40/0/34 → exactly the 4 predicted cells moved |
| 6 | Reporter's case, perfect-score case, `ln`-boundary, clamp, V10 | independent model | `24` ✓, `44` ✓, `44`/`36` ✓, `diff==1.0` ✓, `28` (old `44`) ✓ |
| 7 | CHANGELOG's "250 bp at 30 % scored 44, Bowtie 2 gives 22" | independent model **plus** a full Bowtie 2 analogue (int64 `scMin`/`diff`/`bestOver` + `(double)0.8f` thresholds) | old **44**, new **22**, bowtie2 **22** — claim is exactly right |
| 8 | How much the deliberate f64-vs-int64 divergence (A10) actually costs | 89 976-cell sweep, `len 25..300` × `AS 0..2L`, SE, no second best, `G,20,8` | **361 cells differ = 0.40 %** — the new Bismark values agree with the int64 Bowtie 2 analogue in **99.6 %** of cells. A10/V8's "noise" worry is quantitatively small |
| 9 | **V7** — PR 1 is call-shape-only | reverse-transformed the new calls back to the trio, then diffed against `HEAD` | test modules of `merge.rs`/`combined.rs`/`mod.rs` **identical modulo rustfmt line-wrapping**; production code has **zero** non-mechanical hunks beyond 2 import lines and the 2 intended `allow` deletions |
| 10 | §12's claim that the deleted tautological test would still have passed after D1 | recomputed all 6 cells at `diff 51.296` vs `48.704` | **all 6 identical** — the test was genuinely blind; replacing it was necessary, not cosmetic |
| 11 | **V1** has teeth | injected rev 0's universal clamp into my model and re-ran the exact sweep grid | **127 / 990 cells** would fail, first at `i=0 s=-0.2 len=1 best=0` (frozen 42 → clamped 0) |
| 12 | Gating airtight | `options.rs:82` pushes `--local` under *the same* predicate `cli.local && aligner == Aligner::Bowtie2` that sets `match_bonus`; exactly **one** `RunConfig` construction (`config.rs:789`); `--local` rejected for minimap2/rammap + all 4 combined-index variants; 5-Base has its own driver (`calc_mapq(` occurs only at `merge.rs:367,740` and `combined.rs:339,711`) | no path reaching `calc_mapq` can get the wrong `match_bonus` |
| 13 | Arity payoff / allow removals | `check_results_single_end` now 7 args, `select_pe_nondir` 6 (clippy fires at >7); `check_results_paired_end` still 9 and keeps its allow | both removals correct |
| 14 | Claimed toolchain state | reproduced: `cargo fmt -p bismark --check` clean; `cargo clippy -p bismark --all-targets` **0 warnings**; `cargo test -p bismark` **2107 passed / 0 failed** | confirmed |
| 15 | Does any CI gate break? | `rust_ci.yml:194-220` `perl-oracle` has **no aligner cell** and no `--local` | no |

### Answers to the specific questions in the brief

- **Is `match_bonus > 0.0` airtight?** Yes — see row 12. The predicate is also *incidentally
  future-proof*: Bowtie 2 `--ma 0` sets `monotone = true` ⇒ `perfectScore = 0`, so `match_bonus == 0`
  ⇒ `abs(scMin)` would be the **right** branch. (The *ladder* would still key off `cli.local` and be
  wrong under `--ma 0` — but `--ma` is unreachable, row 4.) `f64 > 0.0` is exact here because the
  field is private and only ever set to the literals `2.0` or `0.0`.
- **`perfect()` for PE:** correct vs `unique.h:207-209` — `scPer = perfectScore(rdlen) + perfectScore(ordlen)`
  and Bismark's `score_min` sums the same two mates, so numerator and denominator use the same pair.
  "SE with `read2_len = Some` by mistake" is not reachable (the four call sites are fixed) and would
  in any case stay *internally consistent* (both `score_min` and `perfect` include the second mate).
  PE-with-one-mate-unmapped does not exist in Bismark's faithful PE path.
- **Is the frozen-path claim true?** Yes, structurally and by measurement (rows 1, 9, 11). The
  `match_bonus == 0.0` arm *is* `sc_min.abs()` and the end-to-end ladder is a byte-exact extraction.
- **The hand-derived HISAT2 expectations:** all six re-derived correctly (row 5). The
  `calc_mapq(50, None, 0, Some(-1)) → 31` cell rides an exact knife edge (`best_diff = 1` vs
  `diff*0.1`, and `10.0 * 0.1 == 1.0` exactly in IEEE-754) — deterministic, but the comment's
  "= 1 exactly" is load-bearing and correctly flagged as such.
- **The refactor:** type-safe by construction — replacing three primitives with one distinct type
  makes a silent argument swap impossible, and row 9 proves nothing else moved. The two fn-pointer
  aliases reverse-transform cleanly with argument order preserved.
- **`normalize()` misuse:** the `scMin`↔`diff` pairing is now structurally enforced. Lengths-vs-`as_best`
  consistency still can't be enforced by the signature (inherent), which is fine.
- **The V10 fixture:** acceptable. MAPQ reads only `AS:i:`, the decoupling is stated in the doc
  comment, and I confirmed the discriminating power (old **44** → new **28**; `AS ∈ 2..9` all
  discriminate; `AS ≥ 2` returns 44 under the old model for *every* value — the saturation pathology
  in miniature). It proves wiring, which is exactly what it claims. See S5 for the one thing it
  doesn't cover.
- **`#[allow(clippy::float_cmp)]`:** the justification still holds for both ladders, and
  `best_over == diff` is now *more* meaningful in Bowtie 2-local (it means `AS == perfect`, with both
  sides produced by the same f64 subtraction ⇒ bit-exact). But the copy on `calc_mapq` itself is now
  dead — see S7.
- **The two removed `too_many_arguments` allows:** both correct (row 13).

---

## 2. Issues — Logic

### L1 (Medium) — `from_emitted`'s documented invariant is not actually enforced

`rust/bismark/src/aligner/config.rs:81-86`:

> Fields are private: `match_bonus` and `form` must agree (only Bowtie 2-local has a nonzero perfect
> score, and only Bowtie 2-local is emitted the `G` form), **so they are never settable independently.**

That last clause is false. `from_emitted` is `pub`, takes `form` from the caller, and derives
`match_bonus` from `(local, aligner)` — so

```rust
ScoreModel::from_emitted(20.0, 8.0, ScoreMinForm::Linear, true, Aligner::Bowtie2)
```

builds `match_bonus = 2.0` with `form = Linear`: exactly the incoherent model the comment says cannot
exist. Field privacy prevents *post-hoc mutation*, not *inconsistent construction*. This matters
slightly more than usual because `bismark::aligner::config` is a `pub mod` of a published crate, so
`from_emitted` is external API.

The invariant *is* true of the only production caller (`score_min_params` picks `G` iff
`cli.local && aligner == Bowtie2`, the same predicate `from_emitted` uses for `match_bonus`), so this
is a documentation defect, not a live bug.

**Recommended fix** — reword to what is enforced, and optionally assert the rest:

```rust
/// Fields are private so the model cannot be mutated after construction. `form` is supplied
/// by the caller (from `options::score_min_params`) rather than re-derived here — that
/// re-derivation was #1079 D2.
```

plus, inside `from_emitted`:

```rust
// The emitted form and the match bonus are two views of the same condition today.
debug_assert_eq!(
    matches!(form, ScoreMinForm::Log),
    local && aligner == Aligner::Bowtie2,
    "score-min form and match bonus disagree"
);
```

### L2 (Low) — `.max(1.0)` silently swallows NaN, `abs()` propagates it

`config.rs:200-204`. Rust's `f64::max` **ignores NaN**, so a NaN `sc_min` yields `diff = 1.0` on the
Bowtie 2-local branch but `diff = NaN` on the frozen branch. Reachable two ways, both narrow:

- `--score_min` is shape-validated only (A11) and `"nan".parse::<f64>()` succeeds, so `G,nan,8` gets
  through Bismark's validator;
- `len == 0` with `slope == 0.0` gives `0.0 * -inf = NaN`.

I checked the resulting MAPQ in both cases: every ladder comparison is `false` either way, so the
returned rung is the bottom one under old and new code alike — **outcome-neutral today**. Worth one
line noting the asymmetry (or a numeric `--score_min` validation as a separate follow-up, which A11
already anticipates).

### L3 (Low, observation) — one live config gets `match_bonus = 2.0` and never reads it

`--illumina_5base --bowtie2 --local` resolves (the `--local` gate only rejects minimap2/rammap and
combined-index; `resolve_aligner` returns `Aligner::Bowtie2` for `--illumina_5base --bowtie2`), so its
`ScoreModel` carries `match_bonus = 2.0`. Harmless: `run_pe_five_base` is a separate driver and
`calc_mapq` is called from nowhere else (row 12), so the field is dead there. Recorded only so
"`match_bonus == 2.0` ⇒ a Bowtie 2-local MAPQ was computed" isn't read as an invariant.

---

## 3. Issues — Errors / user-facing documentation

### E1 (Medium) — the CHANGELOG contradicts itself about HISAT2 `--local`

`CHANGELOG.md:8` ends:

> The **default end-to-end path is unaffected and stays byte-identical** (there the perfect score is
> 0, so the two expressions agree), **as does HISAT2 `--local`**.

`CHANGELOG.md:9` says:

> The emitted form is now the single source of truth for both, **so HISAT2 `--local` MAPQ changes.**

In context bullet 1 is scoped to D1 (where HISAT2-local genuinely is unaffected), but as written a
`--hisat2 --local` user reading bullet 1 concludes their MAPQ is unchanged. This is the single
highest-impact finding in the review: the whole reason for writing the entry now (plan step 8) was to
preserve exactly this nuance.

**Recommended fix** for the tail of line 8:

> …so the two expressions agree). HISAT2 `--local` keeps the same denominator, but its `score_min`
> **form** changes — see the next entry.

### E2 (Medium) — `mapq.rs` header makes a now-false byte-identity claim

`rust/bismark/src/aligner/mapq.rs:10-12`:

> Bowtie 2 `--local` deviates from Perl deliberately … **All other modes remain byte-identical to
> Perl.**

HISAT2 `--local` does not (D2). In the header of a module whose doc-comment calls its own return
values "byte-identity-critical", this is the wrong sentence to leave stale.

**Recommended fix:** "…HISAT2 `--local` also deviates: its `scMin` is now the linear form it is
actually emitted (D2). End-to-end (every aligner) remains byte-identical to Perl."

### E3 (Medium) — `options.rs` still describes the bug that was just fixed

`rust/bismark/src/aligner/options.rs:79-81`:

> HISAT2-local uses the SAME L-form as end-to-end (Perl 7912/7947) — its local-ness is the dropped
> `--no-softclip` in the HISAT2 tail **+ the ln() MAPQ scMin**, NOT this option.

The `ln()` MAPQ `scMin` is precisely what D2 removed, and this comment sits ~5 lines above the
function the change touched. The plan's step 8 listed only `mapq.rs`'s header and `config.rs:235` for
doc updates, so this one slipped through.

**Recommended fix:** "…its local-ness is the dropped `--no-softclip` in the HISAT2 tail + the local
MAPQ ladder, NOT this option. Its MAPQ `scMin` is linear, matching this L-form (#1079 D2)."

### E4 (Medium) — `rust/README.md`: the Milestones line landed, the aligner **row** did not

Plan step 8 requires "`rust/README.md`: aligner row + dated Milestones line", and the file states the
rule itself immediately under the table (`rust/README.md:175`): *"every module-merge PR into `master`
should update that tool's row above **and** add a dated line to Milestones."* Only the Milestones
line was added.

The row still reads, unqualified: *"byte-identical to Perl v0.25.1 + Bowtie 2 2.5.5 at 1M
reads/pairs"* and *"HISAT2 … byte-identical to Perl v0.25.1 + HISAT2 2.2.2"*. Those gates were run
end-to-end (`--local` is non-default), so the claims are true of the default path — but nothing in
the row now flags that `--local` is a deliberate divergence, and the row is the "at a glance" surface.

**Recommended fix:** append to the aligner row, e.g. *"(byte-identity is the **end-to-end** contract:
Bowtie 2 and HISAT2 `--local` MAPQ deliberately diverge from Perl v0.25.1 — #1079)"*.

### E5 (Medium) — the CHANGELOG claims tracking that does not exist yet

`CHANGELOG.md:9` — "…is **not** yet claimed to match a HISAT2 reference — **that is tracked
separately**." `CHANGELOG.md:10` — "Related, unchanged and **tracked separately**: minimap2/rammap…".

PLAN §12 "Not done (by decision)" records: *"The two deferred issues (HISAT2 perfect score;
minimap2/rammap positive-AS) are **not yet filed**."* Plan step 9 required filing them in PR 2.

Either file both issues before merge (preferred — the reasoning to carry is already written up in
plan §1 non-goals and A3/A9) or soften the wording. Shipping a release note that points at
non-existent tracking is the kind of thing that gets rediscovered a year later, which is the failure
mode #1079 itself illustrates.

---

## 4. Issues — Structure / tests

### S1 (Low–Medium) — `frozen()`'s `local == true` branch is dead

`mapq.rs:634-657`. The helper's doc says *"the pre-#1079 formula, verbatim: diff = abs(scMin), linear
or ln by `local`"*, but both call sites (`:672`, `:677`) pass `false`, so the `ln` term **and** the
`calc_mapq_local` arm are unreachable. The helper advertises a capability the test never exercises —
because HISAT2-local's *values* are deliberately not frozen.

**Recommended fix:** drop the `local` parameter and the two branches (the helper becomes the
end-to-end frozen reference, which is what it is), and let the HISAT2 assertion stand on its own.

### S2 (Low) — a loop-invariant assertion nested three loops too deep

`mapq.rs:682-687`. The HISAT2 assertion uses only `hd`, which depends on `(i, s, len)` — not on
`best` (it feeds the discarded `bo`) or `sec`. It therefore runs 6 × 11 × 5 × 3 = **990 times to
assert 66 distinct facts**. Hoist it to the `len` loop.

### S3 (Low) — the test name and first doc line overstate what is frozen for HISAT2

`fn frozen_paths_match_the_pre_fix_formula` / *"End-to-end and HISAT2-local denominators are
byte-frozen against the pre-#1079 formula."* The pre-fix HISAT2-local denominator was
`abs(ln scMin)`; what is frozen is the **shape** (`abs(...)`), not the value. The comment's second
sentence explains this correctly, so this is a naming/first-line fix — e.g. split into
`end_to_end_matches_the_pre_fix_formula` + `hisat2_local_denominator_is_abs_of_the_linear_scmin`.

### S4 (Low) — the hand-derived figures in `local_bowtie2_ln_derived_bucket_boundary` are wrong from the 4th digit

`mapq.rs:568-585`. Correct values:

| comment says | actual |
|---|---|
| `scMin = 20 + 8·ln(25) = 45.7527…` | **45.751007** |
| `diff = 4.2473…` | **4.248993** |
| "the 0.5/0.4 boundary sits at 2.1237" | **2.1244967** |

The conclusions are right (rung 36; `bo = 2.248993`, ratio `0.52930`, margin `0.1245` ≈ "~0.12"), and
the test passes. But this is a test whose stated purpose is independent hand-derivation, so the digits
should be exact — otherwise the next reader can't use them to check the code.

### S5 (Low) — V6 is only half met: the clamp's *rung* is unasserted

`local_diff_is_clamped_to_at_least_one` (`mapq.rs:501-509`) asserts `diff == 1.0` and the frozen
`diff == 0.0`, but not the resulting MAPQ — and the plan's V6 called that out explicitly ("assert
**which rung** results (risk is a silent top rung, not a panic)"). No test currently exercises the
ladder with a clamped `diff`.

**Recommended fix:** `assert_eq!(calc_mapq(6, None, 0, None, ScoreModel::bowtie2_local(20.0, 8.0)), 22);`

Also: the doc's "len ≲ 22" is off by one — the clamp is active through **len 23** (`perfect − scMin =
0.916`); len 24 gives `2.576`. (For the record I checked the "silent top rung" risk under a clamped
`diff`: any `AS ≥ 36` on a 6 bp read would return 44 — but `AS ≤ perfect = 12` in any
self-consistent input, so the clamped path always lands on 22 in practice.)

### S6 (Low) — new public API surface is wider than needed

`ScoreModel::normalize` and `::local_ladder` are `pub` on a `pub mod` of a published crate. Both
callers live in `crate::aligner`, so `pub(crate)` suffices and keeps `normalize` — an internal
computation — off the crate's API. `ScoreModel` itself must stay `pub` (it is a `RunConfig` field and
appears in public `merge`/`combined` signatures).

### S7 (Low) — the `#[allow(clippy::float_cmp)]` on `calc_mapq` is now dead

`mapq.rs:22`. After the extraction `calc_mapq` contains no float comparison at all — the `>` in
`normalize` is not what `float_cmp` fires on, and both ladders carry their own allow. The attribute
now misdirects a reader into looking for a comparison that isn't there. (For completeness: the crate
has no `[lints]` table and `lib.rs` deliberately sets no crate-level lint attributes, so
`clippy::float_cmp` — a pedantic, allow-by-default lint — never fires in CI anyway; all three
attributes are documentation. That's fine, but then they should be accurate.)

### S8 (Low) — `ScoreModel::end_to_end` hardcodes `Aligner::Bowtie2` while documented "any aligner"

`config.rs:127-135`. Correct (with `local = false` the aligner is irrelevant, and
`score_model_construction_matrix` locks the equality across all four variants), but it reads as
"Bowtie 2 end-to-end". A trailing `// aligner is irrelevant when local == false` on that line would
close the gap.

### S9 (Low, drive-by, outside this diff) — a CI step name that lies

`.github/workflows/rust_ci.yml:194`: *"prove all **12** ran"* while the next lines set
`EXPECTED=13` and list 13 tests. Cosmetic and pre-existing (the 12→13 growth came with #1030), but
it's the label on a fail-loud counter.

---

## 5. Efficiency

Nothing to raise. One multiply and one compare added per accepted alignment; `perfect()` is only
evaluated inside the `match_bonus > 0.0` arm, so the default end-to-end path does strictly no extra
work beyond one `f64` compare. `ScoreModel` is a 32-byte `Copy` struct passed by value, which is what
lets the two fn-pointer aliases stay lifetime-free. Net arity **−2** at ~10 sites, retiring two
`too_many_arguments` allows. The `frozen_paths_…` sweep's 990 iterations are trivial (the whole
`aligner::mapq` module runs in <0.01 s).

---

## 6. Recommendations, by priority

| Priority | # | Action | Where |
|---|---|---|---|
| **Medium** | E1 | Fix the CHANGELOG self-contradiction about HISAT2 `--local` | `CHANGELOG.md:8` |
| **Medium** | E5 | File the two deferred issues (or soften "tracked separately") | plan step 9 / `CHANGELOG.md:9-10` |
| **Medium** | E4 | Update the `rust/README.md` **aligner row** with the `--local` divergence caveat | `rust/README.md:161` |
| **Medium** | E2 | "All other modes remain byte-identical to Perl" is false — HISAT2-local isn't | `mapq.rs:10-12` |
| **Medium** | E3 | Stale comment: HISAT2-local's "ln() MAPQ scMin" no longer exists | `options.rs:79-81` |
| **Medium** | L1 | `from_emitted`'s "never settable independently" is not enforced — reword (+ optional `debug_assert!`) | `config.rs:81-86`, `:92-116` |
| Low | S1 | Drop `frozen()`'s dead `local` parameter and its two unreachable branches | `mapq.rs:634-657` |
| Low | S5 | Assert the rung under a clamped `diff`; fix "len ≲ 22" → 23 | `mapq.rs:501-509` |
| Low | S4 | Correct the three hand-derived figures (45.751007 / 4.248993 / 2.1244967) | `mapq.rs:570-585` |
| Low | S3 | Rename / re-word so HISAT2-local isn't described as value-frozen | `mapq.rs:626-632` |
| Low | S2 | Hoist the loop-invariant HISAT2 assertion out of the `best`/`sec` loops | `mapq.rs:682-687` |
| Low | S7 | Remove the now-dead `#[allow(clippy::float_cmp)]` on `calc_mapq` | `mapq.rs:22` |
| Low | S6 | `normalize`/`local_ladder` → `pub(crate)` | `config.rs:154`, `:196` |
| Low | L2 | Note (or guard) that `.max(1.0)` converts NaN→1.0 while `abs()` propagates it | `config.rs:200-204` |
| Low | S8 | Comment why `end_to_end` may hardcode an aligner | `config.rs:127-135` |
| Low | S9 | CI step name says 12, `EXPECTED=13` (pre-existing, outside the diff) | `rust_ci.yml:194` |
| — | L3 | No action; recorded for the record | — |

### Optional, not blocking

- A `config::resolve`-level assertion (`assert_eq!(cfg.score_model, ScoreModel::bowtie2_local(20.0, 8.0))`)
  would guard the wiring more directly and cheaply than V10's integration test. It is currently
  impractical: `config.rs`'s tests have no genome/index fixture at all (all 11 `resolve` tests
  exercise pre-I/O rejects only), which is presumably why V10 went end-to-end. Reasonable as it
  stands; worth a fixture helper if `resolve`-level assertions are wanted later.
- V10's fixture would be strictly better as a **≥24 bp** read at default `G,20,8` (the plan's cell
  (a)) — self-consistent AS *and* an unclamped realistic `diff`. That needs a new genome fixture
  (`make_genome`'s chr1 is 8 bp), so the `G,1,0` route was the right trade. Not worth blocking.
- On the strength of row 8 (99.6 % agreement with a full int64 Bowtie 2 analogue over ~90 k cells),
  V8's "real Bowtie 2 oracle" is now well-substantiated in principle even though it was not run. If
  it is ever revived, the expected disagreement rate to gate on is ~0.4 % for the SE no-second-best
  ladder at `G,20,8`, `len 25..300`.

# CODE REVIEW B — HISAT2 `--local` uses the end-to-end MAPQ ladder (#1080)

**Reviewer:** B (independent; no shared state with Reviewer A)
**Commit:** `875d6b8` on `rust/hisat2-local-ladder`, base `dev` = `ddc7633`
**Files reviewed:** `rust/bismark/src/aligner/config.rs`, `rust/bismark/src/aligner/mapq.rs`, `CHANGELOG.md`, `docs/src/content/docs/options/alignment.md`, `bismark` (Perl help), `plans/07312026_hisat2-local-ladder/PLAN.md`
**Verdict:** the code change is **correct and minimal**. No Critical issues. Two High items (one missing test assertion, one user-facing overclaim), four Medium items (all stale/imprecise prose *inside the changed files*), five Low items.

---

## 1. Summary

The change deletes `ScoreModel::local_ladder` (a stored `bool`) and derives it as `self.match_bonus > 0.0`, so HISAT2 `--local` — whose match bonus is 0 — now takes the end-to-end MAPQ ladder. Nine test expectations move.

I independently verified the three load-bearing claims:

| Claim | Verified? | How |
|---|---|---|
| HISAT2's match bonus is *always* 0 as Bismark invokes it | ✅ | `hisat2.cpp` fetched from upstream master: `--local` and all four `*-local` presets are commented out of the long-option table (639, 665–668); `DEFAULT_MATCH_BONUS 0` (`scoring.h:31` — *not* `DEFAULT_MATCH_BONUS_LOCAL 2`); `--ma` forced to 0 at 3916; the only `localAlign = true` route is `--bwa-sw-like` (1133–1144), which Bismark never emits — the Rust option builder is a closed list with **no passthrough** (`command grep -n "allow_hyphen\|trailing_var_arg\|passthrough" cli.rs` → empty) |
| HISAT2 selects the ladder from `monotone`, and its monotone ladder is the same V3 ladder Bismark ports | ✅ | `mapqv` defaults to 2 (`hisat2.cpp:480`) → `BowtieMapq2` (`unique.h:170`, `new_mapq` 525); `if(sc_.monotone)` at 236; its 42/40/24/23/8/3/0 + 39/33/38/27/… rungs are byte-for-byte the Perl `calc_mapq` end-to-end branch |
| The nine recomputed expectations | ✅ **all nine, plus all nine "(was …)" annotations** | Ran Bismark's **own Perl `calc_mapq`** (extracted `bismark:3923-4186`) as an oracle — see Appendix. Independent of the Rust implementation |

`cargo test -p bismark --lib` 1439 pass / 0 fail, `cargo fmt -p bismark -- --check` clean, `cargo clippy -p bismark --all-targets` 0 warnings (reproduced locally).

**Scope claim "only HISAT2-local changes" holds.** Bowtie 2-local keeps `match_bonus = 2` → local ladder; every end-to-end path keeps `match_bonus = 0` → same branch, same arithmetic (`end_to_end_matches_the_pre_fix_formula` sweeps 6 `--score_min` cells × len 1..=500 × SE/PE); `--local` is rejected for minimap2/rammap (`config.rs:667-676`) and `--combined_index` (`config.rs:685`); the 5-Base modules reference neither `mapq` nor `ScoreModel` (`command grep -rn "mapq\|ScoreModel" five_base_*.rs` → empty). The deleted field had no other reader — `local_ladder` now appears only at `config.rs:168` and `mapq.rs:33` plus four test sites.

**The coupling (A2) is the right call, and — better than the plan claims — it is not silent.** `score_model_construction_matrix` (`mapq.rs:662-676`) loops all four aligners end-to-end asserting `!m.local_ladder()`. #1081 giving minimap2 a match bonus (which it must do unconditionally, since minimap2 rejects `--local`) would fail that assertion loudly with `"Minimap2 must use the end-to-end ladder"`. That tripwire is the strongest argument for the derivation and is worth naming in the code comment (see L4).

---

## 2. Issues by area

### Logic

**H1 — the behaviour change is unasserted at the BAM level, and two existing integration tests silently absorb it.** (High)

`rust/bismark/tests/aligner_cli.rs` already contains two `--hisat2 --local` end-to-end tests that read the output BAM, and **both of their MAPQ values move with this commit** — neither asserts MAPQ, so the change passes through invisibly:

- `hisat2_local_softclip_roundtrip_and_options` (`aligner_cli.rs:2214`): 6 bp read, fake HISAT2 emits `AS:i:0`, no second-best → scMin −1.2, diff 1.2, bestOver 1.2 → **44 before, 42 after**.
- `hisat2_local_pe_softclip_roundtrip` (`aligner_cli.rs:2296`): 6+6 bp, `AS:i:0`/`ZS:i:-2` per mate, summed (`merge.rs:420`) → best 0, second −4, diff 2.4 → **40 before, 39 after**.

This matters because #1079 established the opposite standard for exactly this class of change. `bowtie2_local_mapq_uses_the_perfect_score_denominator_end_to_end` (`aligner_cli.rs:114-119`) carries the rationale verbatim: *"the only thing that would catch the resolved `ScoreModel` losing its Bowtie 2-local match bonus (which would silently turn the fix into a no-op while every unit test still passed)"*. #1080's ladder flip has no equivalent, and the tests that would host one already exist.

Concrete fix — one assertion in each, no new fixture:

```rust
// aligner_cli.rs, inside hisat2_local_softclip_roundtrip_and_options, after `recs`:
let mapq = u8::from(r.mapping_quality().unwrap());
// 6 bp, AS:i:0, no second-best: scMin = -1.2, diff = 1.2, bestOver = 1.2 → e2e top rung.
assert_eq!(
    mapq, 42,
    "HISAT2-local must use the END-TO-END ladder (#1080); 44 means the local ladder \
     is still selected"
);
```

```rust
// aligner_cli.rs, inside hisat2_local_pe_softclip_roundtrip:
// 6+6 bp, summed AS 0 / second -4: diff 2.4, bestDiff 4 ≥ 0.9·diff, bestOver == diff → 39.
assert_eq!(u8::from(r1.mapping_quality().unwrap()), 39, "…44/40 means the local ladder");
```

Both values were produced by the Perl oracle (Appendix, run 3), not read back from the Rust.

Note the risk here is asymmetric to #1079's: a *mis-wiring* of HISAT2 now yields the same result as the fix (both leave `match_bonus` at 0), so nothing can regress silently in that direction. What can regress is the opposite — a future edit dropping the `aligner == Aligner::Bowtie2` guard in `from_emitted` would flip HISAT2-local's denominator **and** ladder together. `score_model_construction_matrix` catches that at the unit level; the BAM-level assertion is what pins the resolved path.

**No other logic defects found.** `local_ladder()`'s predicate is safe: `match_bonus` is only ever `0.0` or the `BOWTIE2_LOCAL_MATCH_BONUS` constant `2.0`, never NaN (a NaN `--score_min` reaches `intercept`/`slope`, not the bonus), so `> 0.0` is total. `normalize`'s `if self.match_bonus > 0.0` and `local_ladder()`'s `self.match_bonus > 0.0` now test the same predicate in two places — see L5, and note they should **not** be merged.

### Errors / correctness of the claims

**M1 — `mapq.rs`'s module header is now wrong about its own subject.** (Medium) `mapq.rs:1-3`:

> `//! **--local** branches (the local ladder, 4082-4178, ships for Bowtie 2 since #981 and`
> `//! HISAT2 since the HISAT2---local work).`

HISAT2 no longer uses the local ladder — this is the one sentence in the file a reader hits first, and it now states the pre-#1080 behaviour. Second, `mapq.rs:12-14` says *"Two `--local` paths deviate from Perl deliberately (#1079)"* and enumerates the Bowtie 2 denominator and HISAT2's `scMin` form. There are now **three** deviations; the ladder flip (#1080) is the one that changes the most values and it is absent. Suggested replacement for both:

```rust
//! **`--local`** branches (the local ladder, `4082-4178`, is reached by Bowtie 2 `--local`
//! only, since #981; HISAT2 `--local` takes the end-to-end ladder — #1080).
…
//! Three `--local` paths deviate from Perl deliberately: Bowtie 2-local's denominator
//! (Perl normalized by `abs(scMin)`, which assumes a perfect score of 0 — true only
//! end-to-end) and HISAT2-local's `scMin` form (Perl evaluated it logarithmically while
//! HISAT2 is emitted the linear `L` form), both #1079; and HISAT2-local's **ladder**
//! (Perl used the local ladder although HISAT2's zero match bonus selects the end-to-end
//! one, #1080). **End-to-end, for every aligner, remains byte-identical to Perl.**
```

**M2 — `calc_mapq_local`'s doc claims an aligner-dependence it no longer has.** (Medium) `mapq.rs:132-136` ends: *"`best_over`/`diff` come from [`ScoreModel::normalize`] — the `scMin` form and the denominator are both aligner-dependent in local mode."* After this commit `calc_mapq_local` is reachable **only** from Bowtie 2-local, so at that point the form is always `Log` and the denominator always `max(1, perfect − scMin)` — nothing is aligner-dependent any more. Replace the last sentence with:

```rust
/// Reached only by Bowtie 2 `--local` — the only mode with a nonzero match bonus, hence the
/// only non-monotone one (#1080). `best_over`/`diff` therefore always come from the
/// logarithmic `scMin` and the perfect-score denominator.
```

**M3 — dangling test-name reference.** (Medium) `mapq.rs:718` (doc of `hisat2_local_denominator_is_abs_of_the_linear_scmin`) points at `local_hisat2_uses_the_linear_form_it_was_emitted`, which **this commit renamed** to `local_hisat2_uses_the_linear_form_and_end_to_end_ladder`. Same for `PLAN.md:141`'s companion prose. One-word fix; `rg` for the old name finds only this site in source.

### Documentation (user-facing)

**H2 — the CHANGELOG headline overclaims, and contradicts the plan's own A1.** (High)

> **HISAT2 `--local` MAPQ now matches what HISAT2 itself would compute**
> … Its perfect alignment score is confirmed to be 0 … **so this is no longer provisional.**

`PLAN.md:32` (A1) is explicit and correct: *"There is no ground-truth oracle … 'match HISAT2's ladder choice' is an argument from consistency with the score regime, not from measurement."* The CHANGELOG asserts the measured claim. Three concrete gaps I confirmed in `unique.h`:

1. HISAT2's `BowtieMapq2` returns **60** for a unique alignment where a second-best was never sought (`unique.h:216`, `if(!flags.canMax() && !s.exhausted(mate1) && (!hasSecbest || !equalSecbest)) return 60;`). Bismark never returns 60 — it recomputes across the 2–4 strand instances. So "matches what HISAT2 itself would compute" is literally false for the commonest case.
2. HISAT2 evaluates `scMin` as an **integer**: `scoreMin_.f<TAlScore>((float)rdlen)` truncates toward zero. Bismark keeps it `f64` (−10.2 vs −10 at 51 bp), which can move a rung boundary.
3. HISAT2's monotone `diff` is `scPer − scMin` = `−scMin`, not `abs(scMin)` (`h2_unique.h:230`; note it also lacks Bowtie 2 v2.5.5's `max(1, …)` at `bt2_unique.h:217`). These differ whenever `scMin > 0`, reachable with a shape-only-validated `--score_min` such as `L,10,-0.2` at len < 50 — one of the very cells in `SCORE_MIN_CELLS`.

(2) and (3) are pre-existing, shared with the byte-frozen end-to-end path, and out of scope here — which is precisely why the *headline* should claim only what was established. The previous bullet models the right register ("matches Bowtie 2's own **normalization**"). Suggested rewrite of the heading and last sentence:

> **HISAT2 `--local` now uses the MAPQ ladder HISAT2 itself would select ([#1079](…), [#1080](…)).**
> … Its perfect alignment score is confirmed to be 0 — previously documented as "not exactly known" — so the ladder choice is no longer a guess. (Bismark still recomputes MAPQ across its 2–4 strand instances rather than reading HISAT2's value, so this is consistency with HISAT2's score regime, not a value-for-value match.)

Everything else in the bullet checks out: `−0.92` vs `−20` at 100 bp is right (`−0.2·ln 100 = −0.9210`), and the **22 → 42** example is confirmed by the Perl oracle (pre-#1079 log-scMin + local ladder gives 22; new path gives 42).

**M4 — the Perl help and docs page now read as if `--local` were a no-op for HISAT2.** (Medium) `bismark:9731-9732` and `docs/…/alignment.md:110` say *"HISAT2 has no local mode of its own and always scores matches as 0, so its best possible alignment score is 0 and MAPQ stays on the end-to-end scale."* Directly above it, the same paragraph says `--local` "is mutually exclusive with end-to-end alignments". A user running `--hisat2 --local` is left asking what the flag does. It does something real: Bismark **drops `--no-softclip`** (`options.rs:342-348`), so HISAT2 may soft-clip; only the MAPQ scale is shared with end-to-end. Add one clause to both:

> … so its best possible alignment score is 0 and MAPQ stays on the end-to-end scale. `--local` still enables soft-clipping for HISAT2 (Bismark drops `--no-softclip`); only the MAPQ scale is unchanged.

Also, strictly, HISAT2 *has* local DP — `--bwa-sw-like` sets `localAlign = true`; what it lacks is a `--local` **option**. "HISAT2 exposes no `--local` option of its own" is the accurate phrasing and costs nothing.

### Structure / style

Clean. The derivation removes a field and a construction argument; nothing else moved. Comment density matches the surrounding module (which is unusually heavy by repo standards, but consistently so, and this file's comments are load-bearing byte-identity provenance).

---

## 3. Low-priority observations

**L1 — `ScoreModel::hisat2_local(i, s) == ScoreModel::end_to_end(i, s)` is now `true`.** With `local_ladder` gone the struct is `(intercept, slope, form, match_bonus)`, and HISAT2-local is `(i, s, Linear, 0.0)` — identical to end-to-end. This is semantically honest (the two now compute the same MAPQ for every input, which is exactly what the docs claim), but two `assert_eq!`s read stronger than they now are: `mapq.rs:659` (`assert_eq!(m, ScoreModel::hisat2_local(…))` would also pass against an end-to-end model) and the `Hisat2` iteration of `mapq.rs:673`. Both are backed up by the adjacent `local_ladder()`/`diff` assertions, so no coverage is actually lost — worth a one-line note at `mapq.rs:651-654` so a future reader doesn't over-trust the equality: `// NB after #1080 this model is *equal* to end_to_end(0,-0.2) — same MAPQ for every input.` Also note `Debug` output no longer distinguishes the two modes, should that ever be logged.

**L2 — pre-existing error in the #1079 CHANGELOG bullet, adjacent to this edit.** It lists *"the ladder rungs that require `best_over == diff` (39/35/34/33/32/31)"*. In the local ladder (`mapq.rs:160-207`) those rungs are **35/34/33/32/31**; `39` is the unconditional `best_diff >= diff*0.8` rung. Since this commit is already editing that section for the same release, drop the `39/`.

**L3 — latent risk if a `--ma` or `--bwa-sw-like` passthrough is ever added.** `from_emitted` hard-codes `local && aligner == Bowtie2` and `BOWTIE2_LOCAL_MATCH_BONUS = 2.0`. A future HISAT2 `--bwa-sw-like` passthrough would give HISAT2 `matchConst = 1` and `localAlign = true` upstream (`hisat2.cpp:1133-1144`, `polstr += ";MA=1"`) while Bismark still assumed 0 — silently wrong denominator *and* ladder. Similarly, a `--ma` passthrough accepting a **negative** value would make upstream non-monotone (`matchConst != 0`) while Bismark's `> 0.0` says monotone. Neither is reachable today (verified: no passthrough in `cli.rs`). The right guard is at the CLI if that day comes (`--ma` must be `>= 0`, `--bwa-sw-like` unsupported) — **not** loosening `> 0.0` to `!= 0.0`, which would desynchronise `local_ladder()` from `normalize`'s predicate and produce the local-ladder-with-`abs(scMin)` combination `PLAN.md:28` rightly calls impossible upstream. Worth one line in `from_emitted`'s comment.

**L4 — the `local_ladder()` comment undersells its own safety net, and overstates the minimap2 case.** `config.rs:165-167` says the #1081 coupling "is what Bowtie 2's own logic implies for a non-monotone aligner". Bowtie 2's logic binds Bowtie-family scorers; minimap2 doesn't use a V3-style ladder at all (it derives MAPQ from chain scores `f1`/`f2`), so nothing about Bowtie 2's ladder is *implied* for it — the coupling is a defensible default that forces the question, which is what the second half of the sentence says. More usefully, point at the tripwire: `// score_model_construction_matrix asserts !local_ladder() for all four aligners end-to-end, so #1081 cannot give minimap2 a bonus without failing a test.` That converts A2 from "documented" to "enforced".

**L5 — the same predicate now appears twice.** `normalize` (`config.rs:212`) and `local_ladder()` (`config.rs:169`) both test `self.match_bonus > 0.0`. Tempting to fold, but **don't**: they encode different decisions that merely coincide (upstream Bowtie 2 clamps `max(1, scPer − scMin)` in *both* regimes; Bismark keeps `abs(scMin)` for the monotone case as a deliberate byte-freeze, #1079). If anything, mirror upstream's vocabulary with a private helper used by both call sites *for reading*, e.g. `fn monotone(&self) -> bool { self.match_bonus == 0.0 }`, and keep each branch's comment. Optional.

**L6 — non-issue, checked and dismissed.** With soft-clipping enabled and a zero match bonus, a mostly-clipped HISAT2 alignment can reach `AS:i:0` and hence MAPQ 42. That is HISAT2's own behaviour under the same scoring (soft-clip penalty defaults 2/2, `scoring.h:527-528`, so the score is still bounded above by 0), so it is faithful, not a regression introduced here. Mentioning it in the docs sentence from M4 would be over-explaining.

---

## 4. Recommendations, prioritized

| # | Priority | Action | Location |
|---|---|---|---|
| H1 | **High** | Assert the changed MAPQ in the two existing HISAT2-local BAM tests (42 SE / 39 PE) | `rust/bismark/tests/aligner_cli.rs:2214`, `:2296` |
| H2 | **High** | Soften the CHANGELOG headline to the claim that was verified (ladder choice), keep the confirmed perfect-score = 0 | `CHANGELOG.md:13` |
| M1 | Medium | Fix the `mapq.rs` module header: local ladder is Bowtie 2-only; **three** deliberate `--local` deviations | `rust/bismark/src/aligner/mapq.rs:1-14` |
| M2 | Medium | `calc_mapq_local` is Bowtie 2-only now — drop the "aligner-dependent" sentence | `rust/bismark/src/aligner/mapq.rs:132-136` |
| M3 | Medium | Update the renamed-test reference | `rust/bismark/src/aligner/mapq.rs:718`, `PLAN.md:141` |
| M4 | Medium | Say that `--local` still enables soft-clipping for HISAT2; "exposes no `--local` option" | `bismark:9731-9732`, `docs/…/options/alignment.md:110` |
| L1 | Low | Note that `hisat2_local == end_to_end` now holds | `rust/bismark/src/aligner/mapq.rs:651-654` |
| L2 | Low | Drop `39/` from the `best_over == diff` rung list | `CHANGELOG.md:12` |
| L3 | Low | One line on the `--ma`/`--bwa-sw-like` assumption | `rust/bismark/src/aligner/config.rs:117-119` |
| L4 | Low | Name the construction-matrix tripwire in the #1081 note | `rust/bismark/src/aligner/config.rs:165-167` |
| L5 | Low (optional) | Mirror upstream's `monotone` vocabulary; do not merge the two predicates | `rust/bismark/src/aligner/config.rs:169`, `:212` |

None of these block the change. H1 is the only one touching behaviour coverage; H2/M4 are what users read.

---

## Appendix — independent verification (Perl oracle)

Bismark's own `calc_mapq` (`bismark:3923-4186`) extracted verbatim into a standalone script, driven with the globals it reads. `$local` selects the ladder; a `sed` variant substitutes the `log $readLen` term with `$readLen` to reach the "local ladder over a linear `scMin`" combination (= post-#1079, pre-#1080).

```
# run 1 — new behaviour: end-to-end ladder + linear scMin   ($local = 0)
# run 2 — pre-#1080:     local ladder    + linear scMin     (sed'd, $local = 1)
# run 3 — pre-#1079:     local ladder    + log scMin        ($local = 1)

cell (len, mate2, AS, second)   run1 (after)  run2 (was)  run3 (pre-#1079)
50,  -,   0,  -                      42           44            44
50,  -,  -1,  -                      42           44            22   ← CHANGELOG "22 to 42"
150, -,   0,  -                      42           44            44
50,  -,   0,  -1                     30           31            40
100, -,   0,  -3                     30           31            40
100, -,   0,  -5                     31           32            40
100, -,   0,  -1                      6           11            40
50,  -,  -1,  -1                      1            1             0   ← control cell, 1 under both
150, 150, 0,  -1                      6           11            34
6,   -,   0,  -                      42           44            —    ← aligner_cli.rs:2214 (H1)
6,   6,   0,  -4                     39           40            —    ← aligner_cli.rs:2296 (H1)
```

Run 1 reproduces all nine test expectations (42/42/42/30/30/31/6/1/6) and run 2 reproduces all nine "(was …)" annotations in the test comments (44/44/44/31/31/32/11/1/11). Both columns match the diff exactly, including the control-cell claim.

Upstream sources used: `hisat2.cpp`, `scoring.h`, `unique.h` @ `DaehwanKimLab/hisat2` master; `unique.h` @ `BenLangmead/bowtie2` v2.5.5.

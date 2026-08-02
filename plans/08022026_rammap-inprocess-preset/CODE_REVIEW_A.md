# CODE REVIEW A — #1092 in-process rammap must honour the `--mm2_*` preset

**Reviewer:** A (independent; no coordination with Reviewer B)
**Target:** uncommitted working tree on `dev` @ `11efbab` — `rust/bismark/src/aligner/{config,options,mod}.rs`, `rust/bismark/tests/aligner_rammap_inprocess_crosscheck.rs`, `CHANGELOG.md`, `docs/src/content/docs/options/alignment.md`
**Plan:** `PLAN.md` rev 3 (+ `SPIKE.md`, `PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md`)
**Date:** 2026-08-02

> ⚠️ **The working tree moved during this review.** Reviewer B was editing the same
> uncommitted tree concurrently. All measurements below were taken in an **isolated git
> worktree** (`11efbab` + the patch as it stood when I started) so my numbers are not
> contaminated; where B has since fixed something I say so explicitly. The two source fixes I
> applied went into the shared tree via targeted edits and are verified there.

**Verdict: APPROVE with one High and two Medium items outstanding.** The design is right, the
extraction is inert, and the wiring gate is genuinely a gate — I reproduced injection (i)
failing at `left: 282, right: 270`. But I found **one CI-breaking defect** (now fixed, twice
over) and **a seventh fault that shipped green through every gate**, which I fixed and verified
in both directions.

---

## 1. Summary of what I verified independently

| # | Claim | Result |
|---|---|---|
| **The 282 / 270 arithmetic** | Re-derived from rammap `5ea62cd`, not taken on trust | ✅ **Correct.** `MapOptions::default()` has `match_score: 2`, `mismatch_penalty: 4` (`rammap-core/src/align/map.rs:216`); `map-ont` sets only `k=15, w=10` so scoring stays default → `2·150 − 3·(2+4) = 282`. `sr` sets `match_score = 2, mismatch_penalty = 8` (`api.rs:~690`) → `2·150 − 3·(2+8) = 270`. Per-mismatch delta is `match + mismatch`, exactly as commented |
| **`sr`'s `end_bonus = 10` does not appear in this cell** | Checked against `rammap_sr_respects_the_as_upper_bound` and the preset body | ✅ **Correct.** `sr` does set `opt.alignment.end_bonus = 10`, and the new bound test asserts a perfect 150 bp read scores exactly **300** under both presets — which passes, so the bonus is not a term in the reported score. Consistent with #1081's Milestones note ("a ksw2 traceback tie-breaker, not a score term") |
| **Injection (i) — revert `:968` to `Preset::MapOnt`** | Reproduced in the isolated worktree | ✅ **FAILS the gate**, `left: 282, right: 270`, with the diagnostic message naming the cause. Bonus: it also produces `warning: unused variable: preset`, which is a hard error under CI's `RUSTFLAGS: "-D warnings"` — so option (a)'s structural guarantee has teeth too |
| **Injection (iii) — reorder the conflict dies** | Reproduced (swapped the two inner checks in the `mm2_short_read` arm) | ✅ **FAILS**, and *only* on the triple case: both pair cases still pass under the swap. The plan's "only a triple observes the order" is exactly right, and the triple is doing all the work |
| **V1 — the extraction is inert on the emitted string** | Read the diff character by character + ran the suite | ✅ **Holds.** The `format!` is identical apart from `-x {preset}` → `-x {}` + `as_option_str()`, which returns the same three literals. No pre-existing expected string was edited; `minimap2_preset_selection` still pins all three exact strings, so injection (iv) is caught |
| **Placement of `resolve_mm2_preset` in `resolve`** | Traced call order | ✅ **Sound, and stronger than the plan argues.** `resolve_mm2_max_length` is at `config.rs:717`, the new call at `:884`. For a non-minimap aligner `:717` dies first for *any* `--mm2_*` flag; for Minimap2/Rammap `build_aligner_options` (`:865`) already produced the identical message. So the new `?` can never change any user-visible error text for any input — the placement test pins the ordering that would |
| **`map-pb` is *near*-inert, not inert** | Re-derived the rev-2 contradiction | ✅ **Rev 2 / Reviewer A of the plan review was right.** `build_options` (`api.rs:455-460`) seeds `k = index_k` then lets `apply_preset_str` overwrite it, and `api.rs:756` computes `chn_pen_gap = chain_gap_scale · 0.01 · k` from the **preset's** k *before* k is discarded: `0.8·0.01·15 = 0.12` vs `0.8·0.01·19 = 0.152`. `chn_pen_gap` is consumed by chain scoring (`chain_simd.rs:222`, `chain_rmq.rs:607`), so it is a live mapping option, not an index parameter. **This is the basis of finding M1 below** |
| **A5 — 5-Base never reaches the SE in-process builder** | Checked the guard | ✅ **Stronger than assumed.** `--illumina_5base --rammap` is rejected outright (`config.rs:1262`), so the resolver's `illumina_5base → Sr` arm can never reach a rammap backend at all — not merely "PE-only" |
| **Report bytes / existing notice test** | Checked `report.rs` + `aligner_cli.rs:3085` | ✅ The report echoes `config.aligner_options`, which is frozen, so report bytes do not move; the pre-existing notice assertions straddle the insertion point and still match |
| **Gates** | Re-ran | ✅ full `cargo test -p bismark --features rammap-inprocess` **exit 0** (isolated worktree, my fixes applied); `cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings` with `RUSTFLAGS=-D warnings` **clean**; `cargo clippy --workspace --all-targets -- -D warnings` **clean**; the 7 preset tests all *run* (not skipped) and pass |

---

## 2. Issues

### 🔴 Critical — C1: the `rammap-inprocess` CI job fails to build (`unused_mut`)

`mod.rs` (the gate, `let mut streams = build_se_inprocess_streams(...)`) declared a `mut` that
is not needed — `InProcessAlignerStream::current()` takes `&self`. Under default features the
code is `#[cfg(feature = "rammap-inprocess")]`-gated and therefore invisible, which is why
every gate the plan lists came back clean.

It is **not** invisible to CI. `.github/workflows/rust_ci.yml:15` sets `RUSTFLAGS: "-D warnings"`
at **workflow** scope, so all three steps of the `rammap-inprocess` job fail:

```
error: variable does not need to be mutable
    --> bismark/src/aligner/mod.rs:6569:17
error: could not compile `bismark` (lib test) due to 1 previous error
```

Reproduced with the exact CI invocation (`RUSTFLAGS="-D warnings" cargo clippy -p bismark
--all-targets --features rammap-inprocess -- -D warnings`) — it breaks `:90` (`cargo build
--all-targets --features rammap-inprocess`), `:92` (clippy) and `:102` (`cargo test`).

This is **precedent failure mode (b) reproducing exactly**: `plans/08012026_minimap2-mapq-denominator/`
shipped a CI break invisible to the default `cargo test`, and so did this. The lesson to carry
forward: for any change touching feature-gated code, the local gate must be
`RUSTFLAGS="-D warnings" cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings`,
not `cargo clippy --workspace --all-targets` (which compiles the feature **off**) and not a bare
`cargo test --features rammap-inprocess` (which does not set `RUSTFLAGS`).

**Status: fixed.** Reviewer B removed the `mut` in the shared tree independently; I verified the
CI invocation is clean afterwards. *No action needed beyond knowing it happened.*

### 🟠 High — H1: the wiring gate covers one of the two index loads, so a per-index preset fault ships green

The gate asserts on `streams[0]` only. Directional SE builds **two** streams —
`se_instance_plan(Directional) = [(Norc, Ct, 0), (Nofw, Ga, 0)]` — and `build_se_inprocess_streams`
calls the same `load` closure twice, once per index. A fault that gives only the **GA** load a
different preset is therefore unobservable.

I built it and ran it (isolation: my own worktree):

```rust
let load = |basename: &Path, preset: ::rammap::Preset| -> Result<Arc<::rammap::Aligner>> { … };
let ct = … load(&config.genome.ct_index_basename, preset)?          // resolved
let ga = … load(&config.genome.ga_index_basename, ::rammap::Preset::MapOnt)?   // literal
```

**Result: 1456/1456 lib tests pass, and `clippy -D warnings --features rammap-inprocess` is
clean.** `preset` is still used, so no `unused variable` either. This is the seventh fault the
brief asked me to hunt for, and it is worse than injection (i) in production: for
`--rammap --mm2_short_reads` the OT instance would run `sr` while the CTOB instance ran
`map-ont`, and since `map-ont` scores mismatched reads ~higher (mismatch penalty 4 vs 8) the
GA/OB instance would win the merge's best-instance comparison more often than it should —
**silently biased strand assignment, hence wrong methylation calls**, with the report showing
`-x sr` throughout.

**Status: fixed by me** (see §3, fix 1). Verified in both directions: red under the injection
(`Sr: the CT and GA index loads disagree — left: 270, right: 282`), green on the clean tree.

### 🟡 Medium — M1: `--mm2_pacbio` is documented as differing "*only* in index-build parameters", which the plan's own rev 2 established is false

Three places say it, and B's new integration test now pins the stderr wording:

| Location | Text |
|---|---|
| `mod.rs` (stderr notice) | "`--mm2_pacbio` differs from the default only in index-build parameters, which the pre-built index overrides" |
| `CHANGELOG.md` | "`--mm2_pacbio` differs from the default *only* in those index-build parameters" |
| `docs/.../alignment.md:331` | "this preset differs from the default *only* in index-build parameters (k-mer size and homopolymer compression)" |
| `tests/aligner_cli.rs` (B's new test) | asserts the notice contains that exact clause |

`PLAN.md` §3.5 is unambiguous about this: the reviewers' contradiction was **resolved in A's
favour by measurement**, `chn_pen_gap` moves `0.12 → 0.152`, and the plan says *"State it that
way; do not claim it is provably identical (defensible only until someone hits a gap-heavy
read)"*. I re-derived the mechanism from source (see §1) and confirmed `chn_pen_gap` is a
chaining-score input, not an index parameter. "Differs *only* in index-build parameters" is
therefore the rev-1 / Reviewer-B claim that rev 2 corrected — a **regression toward an earlier
revision** in the user-facing text.

Practically it is one chaining-gap coefficient with no seeding or scoring change, so the
*conclusion* ("near-inert, expect map-ont-equivalent alignments") stands. Only the *reason*
overstates. Suggested minimal wording, preserving brevity:

> `--mm2_pacbio` differs from the default in index-build parameters (k-mer size and
> homopolymer compression) plus one k-derived chaining coefficient, so against a pre-built
> index — which supplies its own k — it is near-inert.

Note that changing the notice means updating B's assertion in `tests/aligner_cli.rs` to match
(shorten it to `"--mm2_pacbio differs from the default"`, which is what the second half of
that test already uses for the negative case).

**Not fixed by me** — three user-facing prose sites plus a test assertion, and the trade-off
between accuracy and brevity is Felix's call.

### 🟡 Medium — M2: `rust/README.md` was not updated, though the plan and the repo's own convention require it

- `PLAN.md` §5 step 6: *"the Milestones line should record the fix"*.
- `rust/README.md:175`: *"every module-merge PR into `master` should update that tool's row above **and** add a dated line to Milestones"*.
- The immediate precedent `11efbab` (#1081) did exactly that (`rust/README.md | 5 +-`).

`rust/README.md` is absent from `git diff --name-only`. The aligner **row** genuinely needs no
edit (its "same `--mm2_*` knobs (apples-to-apples vs `--minimap2`)" claim only becomes true with
this change, as the plan notes), but the dated Milestones line is missing. Left for Felix
because that journal is curated release prose.

### 🟡 Medium — M3: `cargo fmt -p bismark -- --check` is currently **red** in the shared tree

```
Diff in /Users/fkrueger/Github/Bismark/rust/bismark/src/aligner/mod.rs:78:
-use crate::aligner::config::{
-    Aligner, LibraryType, Mm2Preset, ReadFormat, ReadLayout, ScoreModel,
-};
+use crate::aligner::config::{Aligner, LibraryType, Mm2Preset, ReadFormat, ReadLayout, ScoreModel};
```

This arrived from a concurrent edit during the review (merging the two `use` lines from
`config`, itself a good change — it fits in 98 columns, so rustfmt wants it on one line). It is
a separate CI job (`fmt`), so flagging it rather than touching someone else's in-flight edit:
**whoever finalises must re-run `cargo fmt -p bismark`.** My own added lines are fmt-clean —
this is the only diff rustfmt reports.

### 🔵 Low — L1: the `RunConfig` field assignment *was* ungated in a way the compiler cannot see, and is now closed by B's test

`PLAN.md` §12 documents injection (v) as caught by the compiler (`unused variable: mm2_preset`)
rather than by a test. That is true for the exact shape stated, but **shape-specific** — I found
a variant that escapes everything:

```rust
let _ = options::resolve_mm2_preset(cli)?;   // validation kept, result discarded
…
mm2_preset: Mm2Preset::MapOnt,               // hard-coded
```

Measured: **1456/1456 lib tests pass and `clippy -D warnings --features rammap-inprocess` is
clean.** So "the compiler gates it" should not be written down as a durable guarantee.

**Now closed** by B's `rammap_notice_names_the_resolved_preset` in `tests/aligner_cli.rs`, which
drives the real binary (hence real `resolve`) and asserts the notice contains `preset -x sr`
plus `!contains("preset -x map-ont")`. Under this injection the notice would name `map-ont` and
the test fails. Worth keeping: it is the only observation of `RunConfig::mm2_preset` as
populated by `resolve`, and it lands in the **default** (feature-off) test job.

### 🔵 Low — L2: the V7c clipped-CIGAR assertion is close to a tautology, and its comment over-claims

The comment says the assertion catches records the methylation length guard would *silently
skip*, and names `sr`'s local extension as a newly reachable source of clipped CIGARs. Two
reasons it cannot do that job:

1. `consumed_read_len` counts **`S`** as query-consuming (`inprocess.rs:152`), so a
   soft-clipped record consumes the full read and passes. `reconstruct_cigar`'s own doc says as
   much: *"A swapped flank keeps `consumed_read_len == read_len` (so the methylation guard
   would NOT skip it)"*.
2. `reconstruct_cigar` pads to `read_len` by construction, so equality holds for any mapped
   record with a clip-free core — and this cell is `150M` under both presets (implied by
   282/270 being exactly `2·150 − 3·penalty`), so no clipping is exercised at all.

It is still a worthwhile check of the builder's soft-clip arithmetic. Suggest re-labelling it
as that, so a future reader does not believe clipped-CIGAR skipping is gated here. (Retained
verbatim in my edit, just moved inside the per-stream loop.)

### 🔵 Low — L3: `#[allow(clippy::too_many_arguments)]` on `run_config_stub` is a no-op

The stub takes 5 arguments; `clippy::too_many_arguments` fires at 8. The `allow` does nothing
today and would silently suppress the real lint if the stub grows. Suggest deleting it.

### 🔵 Low — L4: `run_config_stub`'s doc does not say which fields are load-bearing

Placing it outside `mod tests` in `config.rs` is **correct and necessary** — `mod tests` is
private, so `mod.rs`'s tests could not reach it otherwise — and writing all 37 fields with no
`..Default::default()` is the right call for exactly the reason stated. I checked the values
against the path it is used on and they are inert: `build_se_inprocess_streams` reads only
`library` (Directional → the 2-stream plan), `genome.{ct,ga}_index_basename`, `mm2_preset` and
`rammap_inprocess_threads`; `score_model` never enters (the in-process stream copies rammap's
own `m.score`/`m.mapq`; Bismark's MAPQ is recomputed later in the merge), and `layout`,
`aligner_options`, `format`, `output`, `detected_aligner` are unread here.

The residual risk is **reuse**: a future MAPQ or PE test picking this stub up would silently
inherit `score_model: minimap_like(0.0, -0.2)`, `library: Directional` and
`rammap_inprocess_threads: 1` as if they were meaningful. One line naming the five
load-bearing fields would head that off. (`library` being hard-coded while `layout` is a
parameter is also a slightly odd asymmetry for a stub whose only caller needs Directional.)

### 🔵 Low — L5: two comments are far over the project's comment budget

`CLAUDE.md`: *"Default to a single line. Two is the maximum and needs a reason."* The block
above `let mm2_preset = options::resolve_mm2_preset(cli)?;` in `resolve` ran to 11 lines in the
state I reviewed (a concurrent edit has since trimmed it — `config.rs` shrank by 9 lines), and
most of it duplicates `resolve_mm2_preset`'s own doc comment, which already states the
placement constraint. The surrounding file is verbose, so this is a judgement call rather than
a defect; noting it because the plan-lineage documents keep growing and the reviewers of
[nf-core/modules#12451] flagged exactly this.

### 🔵 Low — L6: the bound test uses `from_seqs`, not the production hybrid

`rammap_sr_respects_the_as_upper_bound` builds its index with `from_seqs(…, preset)`, which
applies the preset's **own** `k`/`w` (k=21/w=11 for `sr`). Production is the hybrid the
CHANGELOG describes: `sr` options over an index built at the genome-preparation `k`. The bound
`AS ≤ 2·len` is DP-derived and holds either way, so this is not a correctness gap — but the
main gate went to the trouble of driving `save_index` → `from_index` precisely to avoid this
substitution, and this sibling test quietly reverts to it. Cheap to align if desired (reuse the
gate's `.mmi`).

---

## 3. Fixes applied

Both in the shared working tree, by targeted edit (no other region touched):

**Fix 1 — `rust/bismark/src/aligner/mod.rs`, `inprocess_rammap_honours_the_resolved_preset`
(closes H1).** The `score_for` closure now asserts on **every** stream instead of `streams[0]`:
it pins `streams.len() == 2`, loops the CIGAR-consumption assertion per stream (message now
carries the stream index), and adds `assert_eq!(scores[0], scores[1], "…the CT and GA index
loads disagree — one of them did not get the resolved preset")`. Both `.mmi` files hold the same
synthetic reference, so both streams score identically on the clean tree.

Verification:
- clean tree → `inprocess_rammap_honours_the_resolved_preset` **passes** (main tree and worktree)
- GA-only literal injected → **fails**: `Sr: the CT and GA index loads disagree … left: 270, right: 282`
- injection (i) still fails as before (both assertions fire)
- `cargo fmt` clean after rustfmt normalised my `assert_eq!`
- `RUSTFLAGS="-D warnings" cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings` → clean
- full `cargo test -p bismark --features rammap-inprocess` in the isolated worktree → **exit 0**

**Fix 2 — `docs/src/content/docs/options/alignment.md:350`** (stale claim contradicted by this
change, 12 lines below the text the change edits):

```diff
-Single-end only, and like `--minimap2` uses the `map-ont` preset.
+Single-end only, and like `--minimap2` defaults to the `map-ont` preset and honours the `--mm2_*` selectors above.
```

Two further edits I had prepared were made independently by Reviewer B before I applied mine
and are **already in the tree**: moving the `rammap_preset` bridge out of the middle of
`build_se_inprocess_streams`' doc block (it had stolen an 18-line doc comment, leaving the
stream builder undocumented), and adding `--rammap` to `--mm2_nanopore`'s "only works in
conjunction with" line at `:325`. I also had the `unused_mut` fix (C1) and the merged `use`
(which introduced M3).

---

## 4. Recommendations, in priority order

1. **Re-run `cargo fmt -p bismark`** before committing (M3 — currently red).
2. **Soften the three `--mm2_pacbio` "only in index-build parameters" claims** and B's matching
   test assertion (M1) — the plan's rev 2 exists because that claim is false.
3. **Add the dated `rust/README.md` Milestones line** (M2).
4. Delete the no-op `#[allow(clippy::too_many_arguments)]` (L3); re-label the clipped-CIGAR
   assertion for what it checks (L2); name `run_config_stub`'s load-bearing fields (L4).
5. **Record the CI lesson somewhere durable** (C1): the gate for a feature-gated change is
   `RUSTFLAGS="-D warnings" cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings`.
   Two consecutive plans in this lineage have now shipped a warning that only the feature job
   sees; the plan's step 8 already warns about feature-gating placement, but not about
   `RUSTFLAGS`.
6. Follow-up (c) remains the only thing that closes A4. `RAMMAP_PRESET` is a good hook; note
   its `map-ont`/`map-pb`/`sr` mapping is itself untested (env-gated), so a typo there would be
   silent — cheap to cover if (c) lands.

## 5. Things I checked and found correct — no action

- `Mm2Preset` is unconditionally compiled and names no rammap type; the bridge is feature-gated;
  the notice reads the field on both builds, so there is no feature-off dead field. Default
  `clippy --workspace --all-targets -- -D warnings` clean.
- The bridge `match` is exhaustive with no `_` arm, so a fourth variant fails to compile — the
  #1081 lesson applied. `rammap_preset_bridge_maps_every_variant` is a hardcoded 3-case loop,
  but the exhaustiveness guarantee comes from the compiler and the doc comment says so, so the
  precedent's "claimed exhaustiveness that isn't" failure mode does **not** repeat here.
- `resolve_mm2_preset` being `pub` matches the `score_min_params` precedent (`options.rs:396`).
- `minimap2_options` has exactly one production caller (`options.rs:236`); the conflict dies
  still run before `-t` is computed, so die ordering inside the function is unchanged.
- The gate's FastQ fixture is well-formed (`@r1\n<read>\n+\n<quals>\n`), the LCG reference is
  platform-stable, and 150 bp / 3 mismatches sits far from every `sr` threshold — the spike's
  three constraints (score not flip, ≥150 bp, `Sr` only) are all honoured, and V7's description
  does say it covers `Sr` only.
- `RAMMAP_PRESET` defaults to `map-ont` and panics loudly on an unknown value — correct
  never-silent shape, and it does remove the "in-process `map-ont` vs subprocess `-x sr`"
  mismatch the plan identified.
- The CHANGELOG entry is under `## Unreleased` → `### bismark (aligner)`, and every claim in it
  except the `map-pb` one (M1) checks out against the code, including that only
  `--mm2_short_reads` changes alignments, that mapping *rate* moves, that in-process `sr` is
  unmeasured, and that it is a hybrid.

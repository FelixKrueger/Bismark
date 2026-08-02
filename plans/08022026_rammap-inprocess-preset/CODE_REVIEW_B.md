# CODE REVIEW B — #1092 in-process rammap must honour the `--mm2_*` preset

**Reviewer:** B (independent; no coordination with A)
**Target:** uncommitted working tree on `dev` @ `11efbab` — `rust/bismark/src/aligner/{config,options,mod}.rs`, `rust/bismark/tests/aligner_rammap_inprocess_crosscheck.rs`, `CHANGELOG.md`, `docs/src/content/docs/options/alignment.md`
**Read:** `PLAN.md` (rev 3), `SPIKE.md`, `PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md`, precedent `plans/08012026_minimap2-mapq-denominator/`

---

## Verdict

**The fix itself is correct and the design is right** — one production site, typed single-sourcing, the default path provably does not move, the emitted option string provably does not change. I traced all four of that claim's parts independently and they hold.

**But it would not have merged green, and one gate that the plan says is protective is not.** Two blocking findings, both now fixed:

1. **C1 — the `rammap-inprocess` CI job fails.** An `unused_mut` in the new wiring gate. All three of that job's steps (`build`, `clippy`, `test`) run under `-D warnings`; none of the gates §12 reports covers that configuration.
2. **H1 — the "fault that ships green" the lead asked me to hunt exists, and I injected it.** §12's honest caveat (v) — the `RunConfig` field stubbed while the resolver is still called — is described as caught by the compiler. That catch is contingent on the binding having exactly one use. I added a second use, stubbed the field, and got **0 clippy warnings and a fully green suite with the fix a complete no-op**. Step 4b had accidentally created the observable that closes this and nobody connected the two; I added the test.

Nine issues total (1 Critical, 2 High, 3 Medium, 5 Low). **Eight fixed in place**; the ninth (`rust/README.md` Milestones line) was picked up by Reviewer A while I was running gates. After the fixes: **ready to ship**, subject to one re-run of the gates on the union of both reviewers' edits (see the concurrency note at the end).

I also record one **miss of my own** below: the `RAMMAP_PRESET` change threaded the preset into only one of the crosscheck's two arms, which I accepted on the strength of its comment. Reviewer A caught it.

---

## Fixes applied

| # | File | What I changed |
|---|---|---|
| C1 | `rust/bismark/src/aligner/mod.rs:6570` | `let mut streams` → `let streams` |
| H1 | `rust/bismark/tests/aligner_cli.rs:3130` | **new test** `rammap_notice_names_the_resolved_preset` |
| H2 | `rust/bismark/src/aligner/mod.rs:946-957` | moved `rammap_preset` above `build_se_inprocess_streams`' doc block |
| M2 | `docs/src/content/docs/options/alignment.md` | 3 factual corrections (below) |
| M3 | `rust/bismark/src/aligner/config.rs:873-875` | 12-line comment → 3 lines; removed a claim my new test falsified |
| L2 | `rust/bismark/src/aligner/{options,config}.rs` | 2 stale/duplicated cross-references after the extraction |
| L3 | `rust/bismark/src/aligner/mod.rs:81-83` | merged the split `use crate::aligner::config::…` |
| — | `rust/bismark/src/aligner/mod.rs:979-983` | comment/statement order (see H2) |

---

## Critical

### C1 — `unused_mut` in the wiring gate breaks the `rammap-inprocess` CI job (FIXED)

`mod.rs:6570` (was `let mut streams = build_se_inprocess_streams(&config, &converted).unwrap();`). `streams[0].current()` takes `&self`, so the `mut` is unused.

```
error: variable does not need to be mutable
    --> bismark/src/aligner/mod.rs:6569:17
     |
6569 |             let mut streams = build_se_inprocess_streams(&config, &converted).unwrap();
     |                 ----^^^^^^^
     = note: `-D unused-mut` implied by `-D warnings`
```

**Why every gate §12 lists missed it.** §12 reports `cargo clippy --workspace --all-targets -- -D warnings` (clean) and `cargo test -p bismark --features rammap-inprocess` (2133 pass). The first never compiles this test — it is feature-gated. The second compiles it but `cargo test` locally does not apply `-D warnings`. CI applies it three ways:

- `rust_ci.yml:15` — `RUSTFLAGS: "-D warnings"` at **workflow** level, so `cargo build -p bismark --all-targets --features rammap-inprocess` (`:88`) fails;
- `rust_ci.yml:90` — `cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings`, the job's own clippy step, fails;
- `rust_ci.yml:102` — `cargo test -p bismark --features rammap-inprocess`, also under the workflow `RUSTFLAGS`, fails.

This directly answers the lead's question 4: **no**, clippy was not clean under both configurations. It is now — I verified all three configurations CI runs:

| invocation | result |
|---|---|
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings` | clean (was: 1 error) |
| `cargo clippy -p bismark --all-targets --features binseq-input -- -D warnings` | clean (sanity) |

---

## High

### H1 — the fault that ships green: injection (v) has a variant that escapes clippy (FIXED, with a test)

**The claim.** §12 and rev 3 record fault (v) — stub `RunConfig.mm2_preset` to a literal while still calling `resolve_mm2_preset` — as "no test; caught by `unused variable: mm2_preset` under `clippy -D warnings`", and `config.rs`' comment asserted "No test can see it."

**The escape.** That catch is not a property of the code; it is an accident of the binding having exactly **one** use. Any second use — a future validation, a log line, a second struct field, a `--dry_run` print — and `unused_variables` cannot fire, with nothing else observing the assignment.

I ran that variant. Keeping the resolver call, adding one extra use of the binding, and setting `mm2_preset: Mm2Preset::MapOnt` in the struct literal:

```
$ cargo clippy -p bismark --all-targets -- -D warnings
    Finished `dev` profile           # 0 warnings, 0 errors
```

`--rammap --mm2_short_reads` silently runs `map-ont` again — the exact #1092 bug, restored, with CI green and every one of the seven new tests passing. **This is the most severe thing I found; a green CI would have certified a no-op fix.**

**Step 4b had already created the closure and nobody joined the dots.** The notice now prints `config.mm2_preset.as_option_str()` (`mod.rs:236`) — the `RunConfig` field exactly as `resolve` populated it — and `tests/aligner_cli.rs` already carries a full end-to-end fake-rammap harness (`make_fake_rammap_mapped:3016`, used by `rammap_se_mapped_names_report_and_notice:3050`) that **does** reach `RunConfig` construction, on the **default** build, with no rammap binary needed. `PLAN_REVIEW_B.md` I1 concluded it "cannot be closed the obvious way" by enumerating only `config.rs`' `resolve` unit tests (all of which assert errors); rev 3 inherited that conclusion. The never-silent notice made the field observable and turned I1 from unclosable into a six-line assertion.

**Added** `rammap_notice_names_the_resolved_preset` (`tests/aligner_cli.rs:3130`). Two runs against the fake binary, `--rammap_subprocess` to pin the backend on both builds:

- `--mm2_short_reads` → stderr contains `preset -x sr` and **not** `preset -x map-ont`; report contains `-x sr -K 250K`; the `map-pb` note does **not** fire.
- `--mm2_pacbio` → stderr contains `preset -x map-pb` **and** the near-inertness note; report contains `-x map-pb -K 250K`.

Verified: **passes** on the clean tree, **fails** under the injection above. It also asserts the two derivations agree end-to-end (notice from the typed field, report from the emitted string) — `emitted_preset_is_the_resolved_preset` proves that at unit level, this proves it survives `resolve`.

**Bonus: it is the only gate on the new `--mm2_pacbio` notice line**, which had none.

### H2 — `build_se_inprocess_streams` lost its entire doc comment (FIXED)

`rammap_preset` was inserted **between** the pre-existing 14-line `///` block and the function it documents. Result before the fix:

- `rammap_preset`, a 3-arm enum bridge, carried "Build the SE in-process rammap streams (epic 06152026 Phase 2): load each converted `.mmi` … EXACTLY ONCE … the EPIC's 'construct 2 `Arc<Aligner>`' … the per-`IndexChoice` gating below is future-proofing";
- `build_se_inprocess_streams` — the function the whole in-process backend rests on — had **no documentation at all**.

Compiles clean, no lint sees it, all tests pass: it ships green. Moved `rammap_preset` above the block (`mod.rs:946-957`), restoring the doc to its function.

Same class one line down: `let preset = …` had been inserted between the `load` closure's 3-line explanatory comment and `load` itself, so a comment reading "Load each `.mmi` the plan references, exactly once" sat above a statement that loads nothing. Reordered, and the new comment trimmed to one line.

---

## Medium

### M1 — `rust/README.md` Milestones line missing (subsequently added by Reviewer A — see the concurrency note at the end)

`PLAN.md` §5 step 6 requires it ("the Milestones line should record the fix"), it is the project's canonical status journal, and every comparable change has one: #1081 (`README.md:181`), #1079 (`:182`), #1018 (`:184`), rammap-default (`:185`). `git status` showed `rust/README.md` untouched when I reviewed. I chose not to write it — it is release prose in the author's voice, not a mechanical fix — and drafted it instead; **Reviewer A has since added an entry**, so compare theirs against this and keep whichever reads better rather than applying both:

> - **2026-08-02** — `bismark` aligner **`--rammap` now honours `--mm2_short_reads` / `--mm2_pacbio`** ([#1092](https://github.com/FelixKrueger/Bismark/issues/1092); `plans/08022026_rammap-inprocess-preset/`) — `--rammap` defaults to the in-process backend, which hard-coded `Preset::MapOnt`, so `--rammap --mm2_short_reads` silently ran `map-ont` while `--rammap_subprocess` correctly ran `sr`: a flag accepted and ignored, defeating the apples-to-apples comparison the `--mm2_*` knobs were accepted for. The preset is now resolved **once** into a typed `Mm2Preset` that both renders the emitted `-x` and reaches `rammap::Aligner::from_index`, the same single-sourcing #1079 applied to `ScoreMinForm`. **`--rammap --mm2_short_reads` therefore produces different alignments than 3.1.0** — `sr` moves the chain/DP thresholds as well as the penalties, so **mapping rate changes, not just scores**, shifting mapping efficiency, `--unmapped` and the unique/ambiguous split. In-process `sr` is newly reachable and **unmeasured**: the existing rammap concordance figures are all `map-ont`, and in-process `sr` is a **hybrid** — `sr` mapping options over an index built at the genome-preparation `k`, because a pre-built index overrides a preset's index parameters. `--mm2_pacbio` differs from the default *only* in those index parameters, so it is near-inert against a pre-built index on every backend; the run now says so instead of leaving it to be inferred, and the backend notice names the resolved preset. Emitted option string, report bytes, plain `--rammap`, `--rammap_subprocess` and `--minimap2` all unchanged. Per-preset in-process-vs-subprocess concordance (`RAMMAP_PRESET`) is the follow-up that closes the measurement.

### M2 — the docs overclaimed the notice and the pre-built-index premise (FIXED)

Three corrections in `docs/src/content/docs/options/alignment.md`:

1. **"The run says so on stderr"** was written under `--mm2_pacbio`, whose line the same diff had just widened to "`--minimap2` or `--rammap`". The notice block is gated on `config.aligner == Aligner::Rammap` (`mod.rs:207`), so **`--minimap2 --mm2_pacbio` prints nothing**. Scoped to `--rammap`.
2. **"Because Bismark aligns against a pre-built index"** is false for one reachable path. Bisulfite minimap2/rammap do load a pre-built `.mmi` (`discovery.rs:117`), but 5-Base minimap2 "reads the genome FASTA directly and needs no index" (`config.rs:1268-1270`) — so under `--illumina_5base --mm2_pacbio` (accepted; `resolve_mm2_preset_maps_every_selector` asserts it → `MapPb`) minimap2 builds the index itself and `map-pb`'s `k=19` + HPC **do** apply. Same overreach in the `sr` paragraph's `-k21 -w11` claim. Both scoped to the bisulfite path, with the 5-Base exception named.
3. `--mm2_nanopore`'s line still said "Only works in conjuntion with `--minimap2`" while `--mm2_pacbio`'s had been updated to add `--rammap`. Made consistent. (The pre-existing "conjuntion" typo left alone — out of scope, and it appears elsewhere.)

The **notice text itself is sound**: `--illumina_5base --rammap` is rejected outright (`config.rs:1259-1264`), so the notice can never fire on the FASTA-indexing path. Only the docs generalised it too far.

### M3 — a 12-line comment in `resolve`, and after H1 it was factually wrong (FIXED)

`config.rs:872-883` was a 12-line inline comment, against the project's "one line default, two maximum, state the fact not the evidence". Its second paragraph was a reasoning chain about clippy and CI runner contents ending in **"No test can see it"** — which H1's test falsifies. Trimmed to 3 lines carrying the two facts that matter (single-sourcing, and the placement constraint); the compiler/CI reasoning belongs in the commit message.

---

## Low

### L1 — comment volume (NOT fixed; recommendation)

~117 of ~454 added lines are comments (26 %). Blocks over the two-line limit, and what I would do:

| Site | Now | Recommendation |
|---|---|---|
| `options.rs:253-255` (inline, on `let preset = …`) | 3 lines | 1 line: `// Single-sourced with the in-process rammap backend (#1092); do not re-derive here.` |
| `options.rs:269-276` (`resolve_mm2_preset` doc) | 8 lines | Keep the "check order is behaviour" sentence — load-bearing, it is what the new triple-conflict case pins. Drop the third paragraph: it restates `resolve`'s placement constraint, so the fact now lives in two files. |
| `mod.rs:6501-6513` (wiring-gate doc) | 12 lines | Keep the score derivation (`2·150 − 3·(2+4)` / `−3·(2+8)`, and that `end_bonus` does not appear) — genuinely load-bearing for the next rammap bump, and A's O12 asked for it. Drop the "A test that built its own aligner would gate the bridge…" paragraph: plan material. |
| `crosscheck.rs:146-148` | 3 lines | 1 line. |

**Counter-consideration, so this is a decision and not an oversight:** the surrounding code is uniformly this verbose (the pre-existing 14-line block at `mod.rs:944-957`, the 16-line one at `config.rs:910-925`), and "match the surrounding comment density" is also a project rule. My read: trim the **inline** comments, keep the **doc** comments. Nothing here is wrong, only long.

The `CHANGELOG.md` entry is one ~1500-character paragraph — but so is every neighbouring entry, and its content is accurate and properly hedged (`sr` changes mapping rate; `sr` unmeasured **and** a hybrid; `map-pb` near-inert "against a pre-built index"). No change. One nuance, if you are editing it anyway: its closing "Nothing changes for a plain `--rammap` run, for `--rammap_subprocess`, or for `--minimap2`" is true of output and report bytes but not of stderr — a plain `--rammap` run does gain `, preset -x map-ont`. The sentence two clauses earlier ("The run's backend notice names the resolved preset") lets a reader join them, so this is a nicety rather than a correction.

### L2 — duplicated documentation of moved logic (FIXED)

Two copies of a fact that had just moved — the same drift shape #1092 is about:

- `minimap2_options`' doc still listed the three-way preset mapping and named the conflict dies after both moved to `resolve_mm2_preset`;
- `resolve_mm2_max_length`'s doc (`config.rs:1314`) still said they "live in `options::minimap2_options`" — now a wrong turn for a future reader.

Both replaced with pointers to `resolve_mm2_preset`.

### L3 — split import (FIXED)

`mod.rs` had `use crate::aligner::config::Mm2Preset;` immediately above `use crate::aligner::config::{Aligner, …};`. Stable `rustfmt` has no `imports_granularity`, so `cargo fmt` cannot merge these and it would have stayed. Merged.

### L4 — `rammap_sr_respects_the_as_upper_bound` does not test the production seam

It builds via `from_seqs` (`mod.rs:6624`), so `sr`'s `k=21/w=11` **do** apply; production loads a `k=20` `.mmi` through `from_index`, which is precisely the "hybrid" the plan flags as residual risk 1. The property it checks (AS ≤ 2·len, i.e. `end_bonus` not reaching the reported score) is scoring-parameter-only, so the test is valid as written — but the wiring gate three tests up already writes a real `.mmi` with `save_index`, and reusing it would make this cover the configuration users actually get. Optional; would also let the two tests share a fixture.

### L5 — `run_config_stub`'s parameter list is inverted relative to what matters

`layout` is a parameter that the function under test never reads; `library` — which it **does** read (`se_instance_plan(config.library)`, `mod.rs:977`) — is hard-coded `Directional`. No masking today: the gate is directional and the preset is library-independent. But a future non-directional in-process test has to edit the stub. Swap `layout` for `library`, or take both.

**On the broader masking question the lead raised:** I checked every one of the 37 fields against what `build_se_inprocess_streams` reads. Only three are live — `library` (plan), `genome.{ct,ga}_index_basename` (parameterised), `rammap_inprocess_threads` (`1`, and the pool is documented + tested thread-count-invariant). `score_model` is inert because the gate asserts `alignment_score`, not MAPQ. **No field supplies an inert value where production supplies a meaningful one for the property under test.** The no-`..Default::default()` decision (§12 deviation 2) is the right call and I would keep it.

---

## Verified — no action needed

**Blast radius, each part traced independently.**

- **The emitted option string is byte-identical for every input.** The if/else chain's conditions and their order are unchanged and `as_option_str` returns the same three literals, so the extraction is inert by construction. Confirmed mechanically too: `git diff | grep '"-a --MD'` matches only the producer — **no test's expected option string was edited**, which is V1's inertness signal intact. So report bytes do not change; the pre-existing `rammap_se_mapped_names_report_and_notice` still asserts `-x map-ont -K 250K` and passes.
- **Plain `--rammap` does not move.** `resolve_mm2_preset` → `MapOnt` → `Preset::MapOnt`, the value previously hard-coded. Alignments bit-identical; the only change is stderr gaining `, preset -x map-ont`.
- **`--rammap_subprocess` and `--minimap2` are untouched.** The only two reads of `mm2_preset` in the tree are the notice (`mod.rs:242`) and the bridge (`mod.rs:980`); the notice block is gated on `config.aligner == Aligner::Rammap`, and the bridge is in the SE in-process builder.
- **No new error path, for any CLI.** `resolve_mm2_preset` can only fail with ≥2 of {short, pacbio, nanopore}. For a non-minimap aligner, `resolve_mm2_max_length` (`config.rs:1329-1357`) already dies on any *single* one, so it always fires first. For minimap2/rammap, `build_aligner_options` at `:865` → `minimap2_options` → the same resolver dies first. The new call at `:875` is therefore provably never the first error for any input — and `mm2_flags_on_bowtie2_report_the_wrong_aligner_not_a_preset_conflict` pins it with B's discriminating triple, which a single selector could not have.
- **`--illumina_5base`.** PE-only, rejected for SE at `config.rs:782` and `:794`, so it never reaches the SE in-process builder — A5 confirmed independently, not assumed. `--illumina_5base --rammap` is rejected at `config.rs:1259-1264`, so the resolver's shared `Sr` arm has no rammap consequence at all. The 5-Base minimap2 path reads `config.aligner_options`, which is byte-frozen.

**The never-silent notice.** Fires on **both** feature configurations (the block is not feature-gated; only the FastA sub-notice at `:253` is). `Mm2Preset` is imported un-gated and *is used* un-gated at `:242`, so a default build has no unused import — correct call to un-gate it. It cannot fire misleadingly on `--minimap2` because the block requires `Aligner::Rammap`. **No existing test needed updating**: `aligner_cli.rs:3085` asserts the substring `"--rammap uses the rammap pure-Rust minimap2 reimplementation"`, which the interpolation preserves, and no test in the tree asserts the notice's full text or uses `stderr(predicate::eq …)` / `is_empty` for a rammap run. `--mm2_pacbio`'s "expect map-ont-equivalent alignments" is the right strength — "expect" is a prediction, so it does not overclaim §3.5's one-differing-coefficient finding.

**Feature-gating coherence, both directions.** `run_config_stub` is `#[cfg(test)] pub` inside `pub mod config`, so it is crate-root-reachable and does not trip `dead_code` on a default build even though its only consumer is feature-gated — verified empirically by the clean default-workspace clippy, not just by reading. Each of the three new feature-gated tests carries its own `#[cfg(feature = "rammap-inprocess")]`, which is required because `mod.rs`'s `#[cfg(test)] mod tests` is not gated as a whole (step 8's trip hazard, correctly avoided).

**Risk 5 (`set_simd_cap`) retires to perf-only.** §12 leaves AVX2-vs-AVX-512 kernel score-equivalence unconfirmed, and the new `Sr` tests are the first to flip that process-global cap — a plausible exact-score flake on the x86 CI runner. It is confirmed **not** a correctness risk, upstream: the cap is documented as an AVX-512 *license-throttling* measure (`rammap-core/src/align/dp/mod.rs:22-29`), it feeds only `use_avx512()`/`use_avx2()` kernel **selection** (`:74`, `:86`), and rammap's own tests assert SSE-vs-AVX-512 equality of `score`, `max` and CIGAR-consumed lengths (`dp/mod.rs:~886`, `:908`, `:927`) plus scalar-vs-SIMD equality (`:370`, `:393`, `:680`). So the exact-score gates cannot flake on kernel choice; the exposure is timing only. **Risk 5 can be closed.**

**Test quality, per test.** `resolve_mm2_preset_maps_every_selector` pins the mapping; `emitted_preset_is_the_resolved_preset` derives its expectation *from* the resolver, so alone it would pass with both sides wrong together — the former closes exactly that, and the division of labour is right. `minimap2_preset_conflicts_die`'s triple case is the only thing that can observe the check order, correctly identified, and the messages were landed against pre-extraction code (§12), which is what makes them a baseline rather than a snapshot. `rammap_preset_bridge_maps_every_variant` + the wiring gate together catch (i) and (ii); the wiring gate's `consumed_read_len` assertion is a real catch of the silent-skip mode, not decoration. The `RAMMAP_PRESET` parser panics on an unrecognised value rather than falling back — correct fail-loud.

**One thing I got wrong here, corrected:** I judged the `RAMMAP_PRESET` change sound on the strength of its own comment. It was not. As submitted, `RAMMAP_PRESET` reached only the **in-process** arm; `subprocess_sam_by_qname` still hard-coded `-x map-ont` (`HEAD:…crosscheck.rs:73-74`), and the comment discharged the problem onto the operator — "must match the `-x` the subprocess arm was invoked with" — a condition no operator could satisfy, because the subprocess `-x` was a literal with no knob. So `RAMMAP_PRESET=sr` would have compared in-process `sr` against subprocess `map-ont` and reported it as backend divergence: precisely the failure the comment claimed to prevent. **Reviewer A caught and fixed this** (threading `preset_name` into `subprocess_sam_by_qname`); I read the comment and did not check the other arm. It is a High, and it is fixed in this tree.

---

## Gates (post-fix, this working tree)

| Gate | Result |
|---|---|
| `cargo fmt -p bismark -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 warnings |
| `cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings` | 0 warnings (**was 1 error**) |
| `cargo clippy -p bismark --all-targets --features binseq-input -- -D warnings` | 0 warnings |
| `cargo test -p bismark` | **2124 passed / 0 failed** (was 2123; +1 = the new test) |
| `cargo test -p bismark --features rammap-inprocess` | **2134 passed / 0 failed** (was 2133; +1) |
| Fault injection: (v)-variant escaping clippy | clippy clean, **new test fails** — closed |

All three feature-gated #1092 gates were confirmed **running, not skipped**, in the feature build:

```
test aligner::tests::rammap_preset_bridge_maps_every_variant ... ok
test aligner::tests::rammap_sr_respects_the_as_upper_bound ... ok
test aligner::tests::inprocess_rammap_honours_the_resolved_preset ... ok
test rammap_notice_names_the_resolved_preset ... ok
```

and the new one is the only #1092 test in the **default** build, which is the point — it gates `resolve`'s field assignment on the configuration CI runs everywhere:

```
test rammap_notice_names_the_resolved_preset ... ok
```

---

## Before merge

1. Consider L1's inline-comment trims, and L4/L5.
2. **Re-run the gates once, on the final tree** — see the concurrency note.
3. Everything else is fixed in this tree.

---

## Concurrency note (read this before trusting the numbers above)

Reviewer A and I worked in the **same working tree**, and we both applied fixes. My gate numbers were measured on the tree as it stood **mid-review**; A subsequently touched `rust/README.md`, `docs/…/alignment.md` (the `--rammap/--ram` entry, correctly noting it now honours the `--mm2_*` selectors), and `aligner_rammap_inprocess_crosscheck.rs`. Those changes came in after my run.

Nothing I saw suggests a conflict — A's edits are complementary and in different places from mine, and no Edit of mine failed on stale text — but **the full suite has not been run against the union of both reviewers' fixes.** One `cargo fmt` + the three clippy forms + both suites on the final tree before committing.

For the record, my edits were: `mod.rs` (one `mut`, one item move, one comment reorder, one import merge), `config.rs` (one comment trim, one stale cross-reference), `options.rs` (one duplicated doc block), `docs/…/alignment.md` (three factual corrections), and `tests/aligner_cli.rs` (one new test). No production logic was changed by me — the only non-comment, non-test edit is removing an unused `mut`.

# Plan Coverage Report — #1092 in-process rammap preset

**Mode:** B (code vs. plan; the design plan's §5 + §9 serve as the implementation spec)
**Plan(s):** `PLAN.md` rev 3 (+ `SPIKE.md`, `PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md`)
**Codebase:** `/Users/fkrueger/Github/Bismark`, branch `dev` at `11efbab`, **uncommitted working tree**
**Date:** 2026-08-02
**Verdict:** **INCOMPLETE — 2 items unresolved** (1 MISSING, 1 PARTIAL). The correctness fix itself is fully implemented and genuinely gated; what remains is one status-journal line and one half-finished test parameterization.

---

## Summary

- Total items: **29**
- **DONE: 25**
- **PARTIAL: 2** (§5 step 5 crosscheck parameterization; §11 risk-5 SIMD mitigation)
- **MISSING: 1** (§5 step 6 — `rust/README.md` Milestones line)
- **DEVIATED: 1** (V8 injection (v) lands on the compiler, not V7 — documented in §12)

### Gates — all green, measured here

| Gate | Command | Result |
|---|---|---|
| fmt | `cargo fmt -p bismark -- --check` | **clean** |
| default suite (V5) | `cargo test -p bismark` | **2123 passed / 0 failed / 20 ignored** — matches §12's claim exactly |
| feature suite (V6) | `cargo test -p bismark --features rammap-inprocess` | **2134 passed / 0 failed / 20 ignored** (§12 says 2133 — off by one, immaterial) |
| CI clippy | `cargo clippy --workspace --all-targets -- -D warnings` | **0 warnings** |
| feature-job clippy | `cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings` | **0 warnings** (a real CI step, `rust_ci.yml:92`, that plan step 8 does not list) |

`rust/Cargo.toml` has exactly one workspace member (`bismark`), so CI's `cargo test --workspace` and the plan's `-p bismark` are the same run.

---

## Coverage ledger

### §5 Implementation outline

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | `Mm2Preset` + `as_option_str()` + `RunConfig.mm2_preset`, doc'd per §4 | step 1 | DONE | `config.rs:77-104` (enum + impl), `:469-474` (field). Doc comment carries §4's `--illumina_5base --bowtie2` → `Sr` caveat verbatim, so A's I5 is answered |
| 2 | Extract `resolve_mm2_preset`, dies moved **verbatim**, `minimap2_options` renders from `as_option_str()` | step 2 | DONE | `options.rs:258-311`. Strings **and** order verified byte-identical to `HEAD`'s `options.rs:262/267/274`; `minimap2_options` now *calls* the resolver (one derivation, verified by reading) |
| 3 | `resolve` calls it, placed after `resolve_mm2_max_length` and after `build_aligner_options` | step 3 | DONE | `config.rs:884`, i.e. after `:717` (max-length die) and `:865` (`build_aligner_options`). Pinned by the new `mm2_flags_on_bowtie2_report_the_wrong_aligner_not_a_preset_conflict`, which uses B's discriminating **pair** (`--mm2_short_reads --mm2_nanopore`) on the default (Bowtie 2) aligner |
| 4 | `#[cfg(feature)] rammap_preset` bridge + use at the call site | step 4 | DONE | Bridge `mod.rs:944-953`; `let preset = rammap_preset(config.mm2_preset)` at `:983`, consumed by the `load` closure at `:993`. Option (a) satisfied in substance — the closure closes over a binding rather than taking a formal parameter, so no literal can reappear; the comment's wording ("Taken as a parameter") is looser than the code |
| 4b | Never-silent notice names the resolved preset + a `map-pb` clause | step 4b | DONE | `mod.rs:232-246`. Preset appended to the existing backend notice; the `MapPb` clause is its own `eprintln!`. `Mm2Preset` imported un-feature-gated (`mod.rs:81`) because the notice runs on both builds |
| 5 | **Parameterize the crosscheck's preset** | step 5 | **PARTIAL** | See Gap 2. In-process arm reads `RAMMAP_PRESET` (`crosscheck.rs:146-157`); the **subprocess arm still hard-codes `-x map-ont`** (`:74`) |
| 6 | Docs / CHANGELOG entry **+ a Milestones line** | step 6 | **PARTIAL → the Milestones half is MISSING** | CHANGELOG ✅ and `docs/.../options/alignment.md` ✅ (see the user-facing block below). `rust/README.md` untouched: no Milestones line, and §12 does not record the omission. See Gap 1 |
| 7 | No version bump | step 7 | DONE | `rust/VERSION` = `rust/bismark/VERSION` = `rust/bismark/Cargo.toml:3` = **3.1.0**. CHANGELOG entry sits under `## Unreleased` → `### bismark (aligner)` |
| 8 | Gates (fmt, workspace clippy, both test configs) + per-test feature gating | step 8 | DONE | All green (table above). Every new test naming `::rammap::*` carries a per-test `#[cfg(feature = "rammap-inprocess")]`, so the default jobs still compile — B's I8 trip hazard avoided |

### §9 Validation

| # | Item | Status | Notes |
|---|------|--------|-------|
| V1 | Emitted option string unchanged for every input | DONE | No expected-string assertion was edited anywhere in the diff (checked with `git diff -U0`). `minimap2_default_option_string`, `minimap2_p_lifts_t`, `rammap_subprocess_p_lifts_t`, `rammap_default_option_string`, `minimap2_preset_selection` (all three selectors), `minimap2_clean_slate_discards_bowtie2_flags` all pass untouched. The integration-level `-x sr` assertion (`tests/aligner_cli.rs:6199`, in `five_base_pe_end_to_end_inverts_polarity`) also passes untouched — A5's verification target |
| V2 | Conflict dies unmoved, **with teeth** | DONE | `minimap2_preset_conflicts_die` now `assert_eq!`s the exact message per pair **plus** the triple `[short, pacbio, nanopore]` → the short⊕nanopore text, which is the only case that observes the order. The "landed before the extraction" sequencing cannot be re-verified from an uncommitted tree, but the property it protects can and does hold: the three asserted strings and their order are byte-identical to pre-change `HEAD` |
| V3 | Resolver maps every selector | DONE | `resolve_mm2_preset_maps_every_selector`, 6 cases incl. `--illumina_5base` → `Sr` and explicit-preset-beats-5-Base |
| V4 | Bridge exhaustive and correct | DONE | `rammap_preset_bridge_maps_every_variant`; confirmed **running** (not skipped) in the feature run |
| V5 | Default-feature build unaffected | DONE | 2123 / 0 |
| V6 | Feature build compiles and passes | DONE | 2134 / 0 |
| V7 | **Wiring gate, end-to-end through the production constructor** | DONE — in the required shape | `inprocess_rammap_honours_the_resolved_preset` (`mod.rs:6483-6598`): (1) real `.mmi` for **both** `BS_CT` and `BS_GA` via `rammap::Aligner::save_index`; (2) `RunConfig` hand-built by the new `config::run_config_stub` (no `resolve`, no `detect_aligner`); (3) calls **`build_se_inprocess_streams(&config, &converted)`** — production function, real `load` closure, real `from_index`; (4) drains the stream and asserts `alignment_score` **282** (`MapOnt`) / **270** (`Sr`) on a 150 bp / 3-mismatch read. Full derivation in the comment, incl. that `end_bonus = 10` does not enter this cell (O12), and the `Sr`-only scope is stated. `config.mm2_preset` is the **only** input that varies between the two calls (same `.mmi`, same FastQ), so reverting the call site to a literal necessarily collapses 270 → 282 and fails the assertion — injection (i) is genuinely caught |
| V7b | Single-sourcing gated | DONE | `emitted_preset_is_the_resolved_preset` over the 5 clis. Note its teeth are "the emitted `-x` agrees with the resolved preset", so a *divergent* second derivation fails it; a byte-identical duplicate would not (see Observation 3). The shipped code has one derivation |
| V7c | `sr` does not silently drop reads via a clipped CIGAR | DONE | `consumed_read_len(&rec.cigar) == Some(read.len())` asserted inside `score_for`, i.e. for **both** presets, with a message naming the silent-skip consequence |
| V7d | rammap honours #1081's `AS <= 2·len` under `sr` | DONE | `rammap_sr_respects_the_as_upper_bound`: perfect 150 bp read scores exactly **300** under `MapOnt` and `Sr` |
| V8 | Teeth — 6 injections | **DEVIATED (documented)** | §12 reports all six run, 5 detected where the plan said. **(v)** — stub the `RunConfig` field while still calling the resolver — is caught by `unused variable` under `clippy -D warnings`, **not** by V7 as V8 specified, because the gate must build its `RunConfig` directly. §12 states this plainly ("no test … that is a legitimate gate but a *different kind*, and it is documented at the call site rather than claimed as test coverage") and the `config.rs:876-882` comment says the same. Honest, and the deviation is documented. I did **not** re-run the injections — that would mean mutating a working tree two other reviewer agents are reading concurrently; (i)'s detectability is instead established analytically above |

### §3 behaviour, §7 integration, §8 assumptions

| # | Item | Status | Notes |
|---|------|--------|-------|
| B1 | §3.3 behaviour table matches the code | DONE | Default and `--mm2_nanopore` → `MapOnt` (unchanged); `--mm2_short_reads` → `Sr`; `--mm2_pacbio` → `MapPb`; subprocess and minimap2 untouched (`minimap2_options` output frozen by V1) |
| B2 | §3.4 edge cases | DONE | Default path bit-identical (resolver returns the previously hard-coded `MapOnt`); default-feature build compiles with the bridge absent; FastA fallback notice untouched; PE/combined/local still rejected upstream; bridge `match` exhaustive with no `_` arm |
| B3 | §3.5 — `map-pb` described as near-inert, never as changing alignments | DONE | Notice, docs and CHANGELOG all say index-build-parameters-only / near-inert. Minor nuance: all three say "differs *only* in index-build parameters", where §3.5 established that one *k-derived* option (`chn_pen_gap` 0.12 → 0.152) survives the discard. "near-inert" preserves the hedge and no text claims provable identity, so the correction rev 2 demanded is applied |
| B4 | The three `MapPb` corrections (§3.3, §7, CHANGELOG) | DONE | All three present in rev 3 and in the shipped CHANGELOG; nothing promises a pacbio alignment change |
| B5 | §7 — report bytes do not change | DONE | Follows from V1 (option string frozen); the SE report keeps echoing the `-x` it always did, and now the run actually uses it |
| B6 | A5 — 5-Base never reaches the SE in-process builder | DONE (verified, not assumed) | `mod.rs:600-609`: `five_base` + `SingleEnd` is `unreachable!()`, PE routes to `run_pe_five_base`. The resolver's `illumina_5base → Sr` arm is reproduced in position and covered by V3 |
| B7 | A6 — the dies' messages/order are behaviour and are now actually asserted | DONE | See V2 |
| B8 | §11 risk 5 — process-global `set_simd_cap` | **PARTIAL (documented)** | See Gap 3. The plan offered "confirm kernel equivalence, **or** keep the preset assertions inside a single test function"; neither was done, and §12 discloses the first half only |

### User-facing claims (CHANGELOG + docs)

| # | Claim required | Status | Where |
|---|---|---|---|
| U1 | `sr` changes **which reads map**, not just scores | DONE | CHANGELOG: "**mapping rate changes too, not just alignment scores**… mapping-efficiency percentages, `--unmapped` output and the unique/ambiguous split all shift". Docs: "it changes **which** reads map, not only their alignment scores" |
| U2 | In-process `sr` is **unmeasured** and a **hybrid** (D-A4) | DONE | CHANGELOG: concordance "**not** yet measured (the existing rammap concordance figures were all established on `map-ont`)" + "it is also a hybrid, applying `sr` mapping options over an index built with the genome-preparation `k`". The docs page carries the hybrid half only — not required by step 6, worth a line if you want parity |
| U3 | `map-pb` near-inert | DONE | CHANGELOG + docs + run notice (see B3) |

### §12's three documented deviations — audited

1. **`Mm2Preset` stayed non-`Option`** — this matches **§4's own signature**; it deviates only from reviewer A's I5 *offer*, and the doc comment carries the truth A asked for. Not a plan deviation.
2. **`run_config_stub` in `config.rs`** — an addition §4 did not enumerate, documented in §12, and required by V7's "hand-build a `RunConfig`, not via `resolve`". 37 fields written out with no `..Default::default()`, so a new field breaks it loudly. Consistent with the plan's intent.
3. **A5 verified rather than assumed** — confirmed independently (B6).

---

## Gaps (detail)

### Gap 1 — `rust/README.md` has no Milestones line for this fix (MISSING, undocumented)

**Expected (§5 step 6):** "…the README rammap row … needs no edit, **but the Milestones line should record the fix**." `rust/README.md:175` states the same convention for itself: "every module-merge PR into `master` should update that tool's row above **and** add a dated line to Milestones."
**Found:** `rust/README.md` is not in `git diff` at all. The two immediately preceding fixes in this same unreleased CHANGELOG block each shipped their Milestones line in the same commit (`11efbab` for #1081 → `README.md:181`; `ddc7633` for #1079 → `:182`).
**Gap:** one dated Milestones line (2026-08-02, `bismark` aligner, #1092). §12's "Everything else applied" lists "Docs + CHANGELOG" and its Deviations/Not-done sections never mention the README, so this is a dropped plan item rather than a recorded decision.

### Gap 2 — the crosscheck is only half parameterized: the subprocess arm still hard-codes `-x map-ont` (PARTIAL)

**Expected (§5 step 5):** give the crosscheck "a `RAMMAP_PRESET` env var defaulting to `map-ont` — which both makes (c) a small delta later and **stops it comparing in-process `map-ont` against a subprocess run someone launched with `-x sr`**."
**Found:** `tests/aligner_rammap_inprocess_crosscheck.rs:149-157` routes `RAMMAP_PRESET` into the **in-process** aligner (with a fail-loud `panic!` on an unknown value — good). But the test **builds the subprocess command itself**, and that command still contains the literal `"-x", "map-ont"` at `:74`. Nothing reads `RAMMAP_PRESET` there.
**Gap:** `RAMMAP_PRESET=sr` now *creates* the mismatch step 5 exists to prevent — in-process `sr` vs subprocess `map-ont` — and reports it as backend divergence. The in-line comment at `:146-148` ("must match the `-x` the subprocess arm was invoked with") describes an externally-launched subprocess that this test does not have. Follow-up (c) is therefore not the "small delta" step 5 promised. Threading the same variable into the `Command::args` at `:74` closes it. Default behaviour (variable unset) is unchanged and correct, so this is a latent foot-gun, not a live failure.

### Gap 3 — risk 5's SIMD mitigation: neither branch taken (PARTIAL, half-documented)

**Expected (§11 risk 5 / A's I6):** `apply_preset_str` sets a **process-global** SIMD cap (`sr` pins AVX2, other presets reset to `Auto`). "Perf-only **if** the AVX2 and AVX-512 DP kernels are score-identical — **confirm, or keep the preset assertions inside a single test function**."
**Found:** kernel equivalence was **not** confirmed (§12 "Not done" says so). And the assertions are spread over **two** concurrently-runnable test functions in the same binary — `inprocess_rammap_honours_the_resolved_preset` (constructs `MapOnt` **and** `Sr`) and `rammap_sr_respects_the_as_upper_bound` (constructs `MapOnt` **and** `Sr`) — so each can flip the global cap while the other is mapping.
**Gap:** the cheap mitigation the plan itself offered is unapplied. Both tests assert exact scores (282/270/300), and the only CI job that compiles them is `rammap-inprocess` on `ubuntu-latest` (x86_64), which is precisely where the cap matters; these runs were green on aarch64 here, where the concern does not arise. §12 discloses the unconfirmed-equivalence half but not that the single-function alternative was also skipped. Either confirm equivalence, serialize the two tests, or state the residual explicitly.

---

## Test verification

Every test below was observed **running** (not skipped) with the status shown.

| Test | File | Default run | Feature run |
|---|---|---|---|
| `resolve_mm2_preset_maps_every_selector` (V3) | `src/aligner/options.rs:936` | PASS | PASS |
| `emitted_preset_is_the_resolved_preset` (V7b) | `src/aligner/options.rs:963` | PASS | PASS |
| `minimap2_preset_conflicts_die` (V2, rewritten with teeth) | `src/aligner/options.rs:985` | PASS | PASS |
| `minimap2_preset_selection` (V1, untouched) | `src/aligner/options.rs:917` | PASS | PASS |
| `mm2_flags_on_bowtie2_report_the_wrong_aligner_not_a_preset_conflict` (step 3 placement) | `src/aligner/config.rs:1740` | PASS | PASS |
| `rammap_preset_bridge_maps_every_variant` (V4) | `src/aligner/mod.rs:6473` | n/a (feature-gated) | PASS |
| `inprocess_rammap_honours_the_resolved_preset` (V7 + V7c) | `src/aligner/mod.rs:6497` | n/a (feature-gated) | **PASS** |
| `rammap_sr_respects_the_as_upper_bound` (V7d) | `src/aligner/mod.rs:6602` | n/a (feature-gated) | PASS |
| `five_base_pe_end_to_end_inverts_polarity` (V1 / A5 end-to-end `-x sr`) | `tests/aligner_cli.rs:6163` | PASS | PASS |
| whole suite | — | **2123 / 0 / 20 ignored** | **2134 / 0 / 20 ignored** |

---

## Observations (not plan gaps)

1. **`docs/.../options/alignment.md:331`** — "The run says so on stderr" is true for `--rammap --mm2_pacbio` only. The `map-pb` notice lives inside the `--rammap` branch (`mod.rs:241`), so a `--minimap2 --mm2_pacbio` run gets no such line, and this sentence sits in the **MINIMAP2-SPECIFIC OPTIONS** section. The plan only asked for the rammap notice, so the code is per spec; the sentence over-reaches.
2. **`docs/.../options/alignment.md:325`** — `--mm2_nanopore` still reads "Only works in conjuntion with `--minimap2`" while the adjacent `--mm2_pacbio` line was updated to "…or `--rammap`". Inconsistent now that both are rammap-reachable (pre-existing text, outside the plan's ask; the typo "conjuntion" is also pre-existing).
3. **V7b's reach.** It asserts the emitted `-x` *agrees with* the resolved preset, which fails a **divergent** second derivation. A byte-identical duplicated `if` chain would pass it, so §12's fault-table row (vi) ("`minimap2_options` re-derives its own preset → FAILED") holds for a differing re-derivation, not for a semantically identical copy. Substance is fine — `minimap2_options` verifiably calls the resolver, so there is exactly one derivation.
4. **`mod.rs:981-982`'s comment** says the preset is "Taken as a parameter"; it is a captured binding, not a formal parameter. §9a's option (a) property (no literal at the call site) holds either way.
5. **Cross-architecture exactness.** V7 and V7d pin exact scores (282 / 270 / 300) and were verified here on aarch64; the only CI job that compiles them is x86_64. Related to Gap 3, and not a claim the plan made.

---

## Verdict

**INCOMPLETE — 2 items to address, both small and mechanical.** Everything the fix itself needed is in place: the preset is resolved once, `minimap2_options` renders from it, `resolve` stores it at a placement pinned by a test, the bridge is exhaustive and feature-gated, the call site reads it, the run says which preset it used, and the wiring gate really does drive `build_se_inprocess_streams` → `from_index` so a reverted call site fails it. Both plan-review Criticals are closed in substance, and all five gates (fmt, both test configurations, both clippy invocations CI runs) are green here.

What remains:

1. **Add the dated Milestones line to `rust/README.md`** (§5 step 6, Gap 1). The plan asks for it, the README's own convention asks for it, and the two preceding fixes in this same unreleased block each shipped one. The rammap row needs no edit.
2. **Thread `RAMMAP_PRESET` into the crosscheck's subprocess invocation** (`tests/aligner_rammap_inprocess_crosscheck.rs:74`, Gap 2). Right now setting the variable produces exactly the in-process-vs-subprocess preset mismatch step 5 was written to prevent, and follow-up (c) is not the small delta step 5 promised.

And one item to decide rather than fix:

3. **Risk 5 / `set_simd_cap`** (Gap 3). The plan offered "confirm AVX2 ≡ AVX-512 scoring, **or** keep the preset assertions inside a single test function". Neither was done, and the two `Sr`-constructing tests can run concurrently on the x86 feature job — the only job that compiles them. §12 discloses the unconfirmed-equivalence half; if the decision is to accept the risk, say so there too.

Nothing here requires re-running the fault injections or re-measuring the gate; the report's basis is the shipped code, the two full test runs, and the two clippy runs recorded above.

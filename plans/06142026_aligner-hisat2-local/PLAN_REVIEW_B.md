# PLAN_REVIEW_B — HISAT2 `--local` in the Bismark aligner (rev 1)

**Reviewer:** B (fresh context) · **Date:** 2026-06-14
**Target:** `plans/06142026_aligner-hisat2-local/PLAN.md` (rev 1, scoping)
**Verdict:** **APPROVE WITH CHANGES.** The reframing (HISAT2-local = drop `--no-softclip` + L-form + `ln()` MAPQ, no `--local` flag) is correct against the Perl oracle, the MAPQ ladder is genuinely aligner-agnostic and PE-ready, and the plan is honest about the soft-clip-vacuity risk. But the implementation outline **under-specifies the single load-bearing edit** — `score_min_params` (and the `options.rs` local block) key off `cli.local` ALONE and will feed HISAT2-local the **wrong Bowtie 2 `(20,8)` G-form defaults** and reject a correct user `L,…` override. This is the one place the port silently produces wrong output if the outline is followed literally. Fix it explicitly before implementation.

---

## Source verification (different angle: completeness + byte-identity threats)

I traced what actually has to change to go from "HISAT2 end-to-end" to "HISAT2-local," against the Perl oracle and the Rust source.

**Perl oracle (confirmed):**
- `bismark:3932` — `$scMin = $intercept + $slope * ($local ? log $read1Len : $read1Len)`; the `$local` *boolean* (NOT the L/G letter) selects `log`. `bismark:3935` adds the second mate (`+= … log $read2Len`) for PE. `diff = abs $scMin`; `bestOver = $AS_best - $scMin`.
- `bismark:7907-7913` — HISAT2-local with a user `--score_min` requires **`L,…`** form, dies with `"In HISAT2 --local mode, the option '--score_min <func>' needs to be in the format <L,value,value>"` (the message is at ~`7910`, not `7908-7909` as the plan cites — cosmetic).
- `bismark:7946-7948` — HISAT2-local **default** is `($intercept,$slope) = (0,-0.2)` and pushes `--score-min L,0,-0.2`; **no `--local` flag** (the `push '--local'` is Bowtie 2-only, `:7906`/`:7943`, "this option does not work with HISAT2").
- `bismark:8309-8314` — HISAT2 softclip delta: `if ($local) { push '--omit-sec-seq' } else { push '--no-softclip --omit-sec-seq' }`. The `[EXPERIMENTAL]` warn is commented out (`:8310`).

**Rust source (confirmed):**
- `config.rs:295-302` — reject `aligner != Bowtie2` ⇒ lift to `aligner == Minimap2` (plan step 1, correct).
- `options.rs:82-120` — local block: `debug_assert_eq!(aligner, Bowtie2)`, pushes `--local` + `valid_score_min_g` + default `G,20,8`. Needs an aligner branch (plan step 2, correct).
- `options.rs:327-328` — `apply_aligner_specific_options` unconditionally pushes `--no-softclip --omit-sec-seq` for HISAT2; the doc comment `:284-287` even says the experimental `--omit-sec-seq`-only path is "intentionally not reproduced." Needs `if local { --omit-sec-seq } else { --no-softclip --omit-sec-seq }` (plan step 2, correct).
- `methylation.rs:174` — `b'I' | b'S'` share an arm; soft-clip handled aligner-agnostically (plan assumption 3, **confirmed**).
- `merge.rs:736-744` — PE `calc_mapq(len1, Some(len2), …, score_min_local)` already wired; PE local ln branch fires for both mates once the inputs are right (plan point 4, **confirmed**).

---

## 1. Logic / completeness — is the touch-point list complete?

Mostly yes, but **one touch-point is mislabeled as a no-op and one is missing**:

### CRITICAL — `score_min_params` (options.rs:347-375) is Bowtie 2-only and MUST gain an aligner branch
The plan's implementation outline **step 3** says: *"Confirm `calc_mapq`'s local branch is intercept/slope-driven (aligner-agnostic) — it should need no change."* That is true of `calc_mapq`. But the function that **resolves the `(intercept, slope)` fed to it** — `score_min_params` (called at `config.rs:360`) — is not aligner-aware:

```rust
// options.rs:347-352
pub fn score_min_params(cli: &Cli) -> Result<(f64, f64)> {
    let (prefix, default) = if cli.local {
        ("G,", (20.0, 8.0))      // <-- Bowtie 2 local defaults
    } else {
        ("L,", (0.0, -0.2))
    };
    …
    let rest = s.strip_prefix(prefix) …   // <-- demands "G," when local
```

For `--hisat2 --local` this produces two byte-breaking wrong results:
1. **Wrong default:** returns `(20.0, 8.0)` instead of Perl's `(0.0, -0.2)` (`bismark:7947`). The MAPQ `scMin = 20 + 8·ln(len)` (≈51 for len 50) instead of `-0.2·ln(len)` (≈-0.78). Every HISAT2-local MAPQ would be wrong.
2. **Rejects a valid user override:** a user `--score_min L,0,-0.4` (the *correct* HISAT2-local form) is rejected because `strip_prefix("G,")` fails, while a bogus `G,…` would be accepted. The inverse of the Perl validation (`bismark:7908` wants `L`).

`score_min_params` therefore **needs the same aligner branch** as the `options.rs:82` block: `local && bowtie2 ⇒ ("G,", (20,8))`, `local && hisat2 ⇒ ("L,", (0,-0.2))`, `!local ⇒ ("L,", (0,-0.2))`. The plan *gestures* at this in its **Context step 5** ("the resolution must feed those, not the Bowtie 2 `(20,8)` G-form defaults, when `local && hisat2`") but the **Implementation outline step 3 contradicts that** by saying calc_mapq "should need no change" without naming the `score_min_params` edit as a required change. This is the single highest-risk omission: follow the outline literally and HISAT2-local MAPQ is silently wrong. **Promote `score_min_params` to a named, first-class edit in the implementation outline, with its own test.**

Note also that `score_min_params` will need the `aligner` argument threaded in (it currently takes only `&Cli`), and `config.rs:360` updated to pass it. Minor plumbing, but it must be called out.

### IMPORTANT — stale `Config` doc comment (config.rs:178-180) will become wrong
`score_min_local`'s doc says: *"the `--score_min` defaults are `(20.0, 8.0)` (G-form)."* After this change that is only true for Bowtie 2-local; HISAT2-local defaults are `(0,-0.2)` L-form. Update the doc comment (byte-neutral, but the plan's "edge cases" self-review should track it so a code-reviewer doesn't flag a contradiction).

### Completeness items the plan got RIGHT (verified, no action):
- **Report `aligner_options` echo** (`report.rs:71`): the report echoes `config.aligner_options` verbatim, so the `--omit-sec-seq`-only string flows into the report automatically — **no separate edit needed**, but the report byte-identity is fully load-bearing on the option string being exactly right. Worth a one-line note that the report needs no code change but IS covered by the gate.
- **`Cli::validate()`:** there is no separate `Cli::validate()` gate to update for `--local`/`--score_min` (validation lives in `config::resolve` + `build_aligner_options`); confirmed nothing missed there.
- **PE second-mate term** (`merge.rs:736`, `mapq.rs:35`): already wired; no new code.
- **`--non_bs_mm` ⊕ local mutex, splice flags, `--multicore` remap composition:** orthogonal, confirmed.

---

## 2. Byte-identity threats — the negative-scMin regime (review point #2)

This is the subtlest risk and the plan treats it too lightly ("no new spike needed"). I dug into `mapq.rs` and ran the numbers.

**Structural validity (good):** the ladder is sign-agnostic. `diff = sc_min.abs()` (`mapq.rs:42`) and `best_over = as_best - sc_min` (`mapq.rs:43`) mirror Perl `abs $scMin` / `$AS_best - $scMin` exactly. A **negative** scMin (HISAT2-local) does NOT break the arithmetic the way the prompt worried — `diff` is always positive via `.abs()`, and `best_over` correctly *grows* (AS − negative). So the plan's claim "aligner-agnostic, just needs the right (intercept, slope)" is **correct at the formula level.** No `diff = scMin` (unsigned) assumption, no `bestOver = scMin - AS` sign trap.

**But the regime is genuinely untested and numerically delicate.** Every existing `mapq.rs` local test (`local_no_second_best_ladder`, `local_second_best_ladder`, `local_calc_mapq_uses_ln_scmin_and_local_ladder`, `mapq.rs:329-383`) uses the Bowtie 2 `(20,8)` **positive** scMin with `diff=10`. The HISAT2-local default `(0,-0.2)` gives (verified numerically):

| readLen | scMin | diff (abs) | 0.8·diff | 0.5·diff | 0.3·diff |
|---|---|---|---|---|---|
| 50 | -0.782 | 0.782 | 0.626 | 0.391 | 0.235 |
| 100 | -0.921 | 0.921 | 0.737 | 0.461 | 0.276 |
| 150 | -1.002 | 1.002 | 0.802 | 0.501 | 0.301 |

i.e. `diff < 1.0` and the bucket thresholds are sub-unity fractions, while `best_over = AS − scMin` lands near these fractional cutoffs. **This is exactly the regime where a 1-ULP `ln()` divergence flips a `>=` bucket** — and it is the regime the #981 spike (Bowtie 2 `(20,8)`, large positive scMin) did NOT directly exercise. The #981 spike's 152k-case `ln()` bit-identity is reassuring but was over a different input distribution; the *consequence* of any residual ULP wobble is amplified when `diff < 1`.

**Mitigation (the plan already has the right instinct, make it mandatory):** the plan's test step 4 does call for "a MAPQ unit test for the HISAT2-local `(0,−0.2)` ln ladder" — **good, but require it to (a) use the actual `(0,-0.2)` defaults at several real read lengths (50/75/100/150), (b) cross-check each against a Perl `calc_mapq` computation (the spike harness can emit the oracle values), and (c) include a second-best case so the `best_diff` sub-ladder is also exercised in the negative-scMin regime.** The existing `local_calc_mapq_uses_ln_scmin_and_local_ladder` self-consistency test (`mapq.rs:370`) is NOT sufficient — it only proves the Rust local branch is internally consistent, not that it matches Perl at `(0,-0.2)`. This is an Important, not Critical, item because the byte-identity gate would ultimately catch a divergence — but only if the gate is non-vacuous (see §3), and a unit test fails faster and localizes better.

---

## 3. Gate non-vacuity — the soft-clip coverage gap (review point #3)

The plan **correctly identifies** this risk in its Validation section ("the gate set must actually contain soft-clipped HISAT2 alignments … confirm `S` appears … else the gate is vacuous") and Self-Review. That is the right instinct and is the strongest part of the plan. **But it stops at "confirm `S` appears" without specifying HOW to guarantee it** — and short, clean, directional WGBS reads (the #981/HISAT2-gate matrix) rarely soft-clip even in local mode, so a gate that just reuses those datasets risks **0 soft-clipped reads → passing vacuously** while testing nothing the headline change touches.

**Make this a hard, named requirement in the gate, not an after-the-fact confirmation:**
1. **Assert non-zero `S` CIGARs** in the HISAT2-local gate BAM (`samtools view | grep -c 'S'` or a CIGAR-op scan) and **fail the gate if zero** — never let it pass green on a vacuous corpus.
2. **Engineer soft-clip coverage** rather than hoping for it: at least one gate cell should use a soft-clip-prone input — e.g. reads with adapter/contaminant tails or terminal mismatches that local mode clips but end-to-end would reject, or a synthetic dataset constructed to force terminal clips. The directional clean-WGBS set alone is insufficient evidence for the one behavior that distinguishes this mode.
3. Because soft-clip exercises the `b'S'` arm of `methylation.rs:174` AND the negative-scMin MAPQ ladder simultaneously, a soft-clip-rich cell is the *only* end-to-end proof that the two new behaviors compose byte-identically. Without it, both §2 and the headline change are unvalidated at the integration level.

This is the **most important validation gap** after the `score_min_params` Critical.

---

## 4. PE specifics (review point #4)

Confirmed against source — the plan's claims hold:
- The PE local scMin second-mate term (`mapq.rs:35-41`, Perl `:3935`) is already present and is reached via `merge.rs:736-744` which passes `Some(sequence_2.len())` + `score_min_local`. So PE HISAT2-local needs **no new MAPQ code** — it inherits the same fix as SE (the `score_min_params` aligner branch). Plan point 4 is correct.
- **No PE soft-clip ⊕ overlap interaction in the aligner:** verified `methylation.rs` does the CIGAR walk per record (`S` → padding X, no pos change, `:174-180`); there is no fragment-overlap dedup in the aligner (that lives downstream in the extractor). The plan's "PE has no fragment-overlap work in the aligner (#981 verified)" is consistent with the code. No action.
- One nuance to fold into the gate: a PE soft-clip cell should confirm `S` appears on **mates independently** (one mate clipped, the other not) since each mate's XM/MD is extracted separately — covered if the §3 soft-clip-rich requirement is applied to a PE cell too.

---

## 5. Effort realism — is it actually "small"? (review point #5)

**Mostly yes, with one caveat the plan downplays.** The reject lift (1 conjunct) and the softclip-delta toggle are genuinely tiny. The risk to scrutinize is branching the **shared, byte-frozen** `options.rs:82` local block and `score_min_params`:

- The Bowtie 2-local byte-identity (#981) is pinned by `accepts_local_for_bowtie2_emits_local_and_g_score_min` (`options.rs:478`) and `score_min_params_local_defaults_and_parses_g` (`options.rs:516`). Adding the HISAT2 sub-branch must **keep the Bowtie 2 arm byte-identical** — the existing tests guard this, so the risk is low IF the implementer preserves the Bowtie 2 branch literally and only ADDS the HISAT2 case. **Recommend the implementation plan explicitly state "Bowtie 2-local arm unchanged; HISAT2 is a new `else if` arm" and re-pin the Bowtie 2 assertions** (the `bowtie2_*_byte_frozen` test pattern already used for the Minimap2 branch, `options.rs:776`).
- The `debug_assert_eq!(aligner, Bowtie2)` at `options.rs:83` MUST be removed (not just the reject) — the plan mentions this (step 2) but it's easy to miss; flag it as a discrete sub-step (a leftover debug_assert would panic in debug builds on HISAT2-local and is otherwise invisible in release).

So: small, but two shared byte-frozen functions get surgically branched — bounded, not zero-risk. The "no spike" call is reasonable; the residual risk is the negative-scMin MAPQ regime (§2), addressed by a unit test, not a spike.

---

## 6. Q4 — deferring the oracle-stability check to Phase-1 setup (review point #6)

**Acceptable.** Perl `--hisat2 --local` determinism is a property of HISAT2 single-core (deterministic) + Bismark's deterministic option assembly; the `[EXPERIMENTAL]` label is about *biological* validity, not run-to-run reproducibility, and the shipped HISAT2 end-to-end gates already proved HISAT2 single-core is a stable oracle. Folding the "run-twice = same md5" check into Phase-1 gate setup (rather than a standalone spike) is proportionate. **One guard:** make it a *blocking* Phase-1 step (run Perl `--hisat2 --local` twice, assert identical md5) BEFORE generating the reference BAMs — if it somehow is non-deterministic, the whole gate is unsound and you want to know on day one, not after building the reference. The plan says "expected YES"; just make the check a gate-prerequisite, not an optional confirm.

---

## Assumptions audit

| # | Plan assumption | Verdict |
|---|---|---|
| 1 | HISAT2-local emits `--score-min L,<i>,<s>` + `--omit-sec-seq`, no `--local`/`--no-softclip` | **Verified** (`bismark:7946-7948`, `8311`). |
| 2 | local MAPQ uses `ln()` scMin `(0,-0.2)`; `ln()` parity proven by #981 → no spike | **Verified** the formula + #981 broad parity; **but** the `(0,-0.2)` negative-scMin/`diff<1` regime is untested → require the unit test (§2). |
| 3 | soft-clip→methylation aligner-agnostic | **Verified** (`methylation.rs:174`, `S`≡`I` arm). |
| 4 | minimap2-local stays rejected | Fine (Q3-locked); reject message + README note are the only edits. |
| 5 | non-byte-identical to end-to-end by design | Correct. |
| 6 | hard-clip `H` / supplementary orthogonal | Correct — the CIGAR walk has no `H` arm and falls to the catch-all error (`methylation.rs:192-197`); local only adds `S`. Note: this means a *supplementary* HISAT2-local record with `H` would still error out, same as end-to-end — consistent, not a regression. |

**Unstated assumption to surface:** the plan assumes `score_min_params` is "the resolution" but never states that it currently takes only `&Cli` and is Bowtie 2-shaped — make the "needs an `aligner` argument" plumbing explicit (see §1 Critical).

---

## Action items

### Critical (fix before implementation trigger)
1. **Promote `score_min_params` (options.rs:347) to a named, first-class edit with an aligner branch.** The implementation outline step 3 ("calc_mapq … should need no change") is misleading: `calc_mapq` is fine, but `score_min_params` currently returns the **wrong `(20,8)` G-form defaults** and **rejects a valid `L,…` override** for HISAT2-local. Add `local && hisat2 ⇒ ("L,", (0.0,-0.2))`; thread an `aligner` arg in; update the `config.rs:360` call. Without this, HISAT2-local MAPQ is silently wrong. Add a dedicated unit test (`score_min_params` returns `(0,-0.2)`+L for HISAT2-local; accepts `L,…`, rejects `G,…`).

### Important
2. **Make the gate non-vacuous for soft-clipping (§3).** Add a *blocking assertion* that `S` CIGARs appear in the HISAT2-local gate BAM (fail if zero), AND engineer a soft-clip-prone gate cell (adapter-tailed / terminal-mismatch / synthetic reads) so the headline behavior is actually exercised — for both an SE and a PE cell. The clean directional-WGBS matrix alone risks 0 soft-clips → vacuous pass.
3. **Require a Perl-cross-checked HISAT2-local MAPQ unit test at `(0,-0.2)` (§2)** across read lengths 50/75/100/150 with and without a second-best, NOT just the existing self-consistency test (`mapq.rs:370`). The negative-scMin / `diff<1` regime is delicate and untested; a unit test fails faster and localizes better than the gate.
4. **Remove the `debug_assert_eq!(aligner, Bowtie2)` at options.rs:83** as a discrete sub-step (a leftover would panic in debug on HISAT2-local) and re-pin the Bowtie 2-local byte-frozen assertions alongside the new HISAT2 arm (mirror the `bowtie2_*_byte_frozen_alongside_minimap2` pattern, options.rs:776).
5. **Update the stale `score_min_local` doc comment (config.rs:178-180)** which hardcodes "(20.0, 8.0) (G-form)" — true only for Bowtie 2-local after this change.
6. **Make the Q4 determinism check a blocking Phase-1 prerequisite** (run Perl `--hisat2 --local` twice → identical md5) before generating reference BAMs, not an optional confirm.

### Optional
7. Note in the plan that the report `_*_report.txt` `aligner_options` echo (report.rs:71) needs **no code change** but its byte-identity is fully load-bearing on the option string — so a report-text assertion belongs in the gate (cheap, catches an option-order regression that the BAM might not).
8. Fix the cosmetic line-number cite: the HISAT2-local `L,value,value` die message is at `bismark:~7910`, not `7908-7909`.
9. The non-goal section is thorough on minimap2; the Q3 reject message + README note are well-specified — no change needed.

---

## Bottom line
The reframing and the reuse-of-#981 thesis are **correct and verified against the oracle**: HISAT2-local is genuinely "drop `--no-softclip` + L-form + `ln()` MAPQ, no `--local` flag," the MAPQ ladder is sign-agnostic and PE-ready, and soft-clip handling is already aligner-neutral. The plan is **NOT yet safe to implement as written** because of one Critical: the implementation outline mislabels `score_min_params` as a no-op when it is the single edit that, if skipped, makes every HISAT2-local MAPQ silently wrong (and rejects valid user overrides). Combined with the gate-vacuity gap (Important #2), those are the two things that could make the output NOT byte-identical without the gate catching it. Resolve Critical #1 and Important #2-#3, and this is a sound, genuinely small follow-up.

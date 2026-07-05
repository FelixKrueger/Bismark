# PLAN_REVIEW_A — HISAT2 `--local` in the Bismark aligner (rev 1, scoping)

**Reviewer:** A (fresh context) · **Date:** 2026-06-14
**Target:** `plans/06142026_aligner-hisat2-local/PLAN.md` (rev 1)
**Verified against:** Perl oracle `bismark` (repo root) + Rust source in this worktree (`rust/bismark-aligner/`).

## Verdict

**APPROVE WITH CHANGES.** The plan's central thesis — *HISAT2-local is defined by subtraction (drop `--no-softclip`) + a MAPQ-formula switch, NOT a `--local` passthrough* — is **fully correct and verified against the Perl source**. All four load-bearing oracle claims check out, and all three Rust reuse claims are accurate. The scope, the SE+PE-together decision, and the minimap2-stays-rejected decision are sound.

There is **one Critical correctness issue**: the plan under-specifies the single piece of code that actually carries a latent bug — `options::score_min_params` (`options.rs:347-352`) **hardcodes the G-form + `(20.0, 8.0)` defaults on `cli.local` alone**, with **no `aligner` parameter**. For HISAT2-local this is wrong (must be L-form / `(0.0,-0.2)`), and the function signature must change. The plan's wording ("confirm... should need no change") risks an implementer leaving this broken. Plus two Important doc/validation gaps.

---

## 1. Logic review — load-bearing claims verified

All confirmed against the actual source in this worktree:

| # | Claim | Verified |
|---|-------|----------|
| 1 | Perl does NOT push `--local` to HISAT2 | ✅ `bismark:7904`/`7943` (`'--local'; # this option does not work with HISAT2` — guarded `if ($bowtie2)`); the HISAT2 `else` branches (`7907-7914`, `7946-7949`) push only `--score-min L,...`. |
| 2 | HISAT2-local score-min = L-form `L,0,-0.2` (same as end-to-end), NOT G-form | ✅ `bismark:7908-7913` (override must match `^L,(.+),(.+)$`, dies otherwise) + `7947-7948` (default `(0,-0.2)`, pushed as `L,...`). Doc confirms it: `bismark:9961` "end-to-end mode (default), **and --local mode for HISAT2 only**, --score_min is set as a linear function". |
| 3 | HISAT2-local drops `--no-softclip` | ✅ `bismark:8309-8315`: `if ($local){ push '--omit-sec-seq' }` else `push '--no-softclip --omit-sec-seq'`. |
| 4 | MAPQ uses `ln(readLen)` with `(0,−0.2)` | ✅ `bismark:3932` `$local ? log $read1Len : $read1Len`; `3935` PE second mate; the `$local` local ladder `4078-4178`. `$local` is the *boolean*, so HISAT2-local (L-form, `$local`=1) STILL takes the `log` path — the ladder branches on the flag, never on the score-min letter. |
| 5a | Local MAPQ ladder + `ln()` scMin reusable, aligner-agnostic | ✅ `mapq.rs:18-46` `calc_mapq` is driven purely by `(intercept, slope, local)`; `calc_mapq_local` (`mapq.rs:142-...`) has **no hardcoded `(20,8)`** — only caller-supplied `best_over`/`diff`/`best_diff` and ratio thresholds. |
| 5b | soft-clip-as-`I` at `methylation.rs:174` | ✅ `b'I' \| b'S' =>` shared arm (pad `X`, no pos advance); aligner-agnostic. |
| 5c | `--local` reject at `config.rs:295` (`aligner != Bowtie2`) | ✅ exactly as described. |
| 5d | Bowtie2-shaped local block at `options.rs:82` (`debug_assert Bowtie2`, G-form, pushes `--local`) | ✅ exactly as described. |
| 6 | `ln()` Perl≡Rust parity proven by #981 spike; HISAT2-local reuses same `ln()` w/ `(0,−0.2)` | ✅ reasoning is sound — see §1.1 below. |

### 1.1 Q6 residual-risk on the `ln()` parity (same transcendental, different constants)

**The parity genuinely carries.** I traced the arithmetic for both aligners:
- Bowtie2-local: `scMin = 20 + 8·ln(len)` → **positive** (e.g. len 100 → ~56.8).
- HISAT2-local: `scMin = 0 + (-0.2)·ln(len)` → **negative** (len 100 → ~−0.921).

Both then compute `diff = |scMin|` (`mapq.rs:42` ≡ Perl `3938`) and `best_over = as_best − scMin` (`mapq.rs:43` ≡ Perl `3939`), and feed `calc_mapq_local`. The ladder compares `best_over`/`best_diff` against *fractions of `diff`* — it is **sign-agnostic and constant-free**, so the `(0,−0.2)` vs `(20,8)` difference is **fully captured by the parameters**, with no hidden G-form bucket boundary. The `ln()` call is byte-identical to Bowtie2-local's (#981 spike: 0 ULP, both arches) → **no new Phase-0 spike is warranted.** I concur with the plan.

**One coverage subtlety (not a parity break, a test note):** the ladder's exact-equality branch `best_over == diff` (`mapq.rs:173`, with `#[allow(clippy::float_cmp)]`) is triggered by *different inputs* under HISAT2-local than under Bowtie2-local. With negative `scMin`, `best_over == diff` ⇔ `as_best == 0`. That is a legitimate HISAT2-local case (HISAT2 local AS can be ≥0), and Perl computes it identically, so it is bit-safe — but the existing `mapq.rs` local unit tests were written around the Bowtie2 `(20,8)` positive-scMin regime. A HISAT2-local-specific `(0,−0.2)` unit test (already in the plan, step 4) **must include an `as_best == 0` case** to exercise this branch. Minor; flagged so it is not dropped.

### 1.2 CRITICAL — `score_min_params` hardcodes G-form on `cli.local` and has no `aligner` arg

This is the one place the plan is dangerously soft. Current source:

```rust
// options.rs:347-352
pub fn score_min_params(cli: &Cli) -> Result<(f64, f64)> {
    let (prefix, default) = if cli.local {
        ("G,", (20.0, 8.0))      // <-- WRONG for HISAT2-local
    } else {
        ("L,", (0.0, -0.2))
    };
    ...
```

- It takes **only `cli`** — it cannot tell Bowtie2-local from HISAT2-local. For `--hisat2 --local` with no `--score_min`, it returns `(20.0, 8.0)` (Bowtie2 G defaults) instead of `(0.0, −0.2)`, and for a user L-form `--score_min` it would *reject* it (expects `G,` prefix). Both are wrong vs Perl `7908-7913`/`7947`.
- Call site `config.rs:360` `score_min_params(cli)` → feeds `score_min_intercept/slope` → `calc_mapq`. So this directly poisons MAPQ for HISAT2-local.
- **`aligner` is already in scope at `config.rs:360`** (used at `355`), so the fix is mechanical: change the signature to `score_min_params(cli, aligner)` and branch `(prefix, default)` on `local && bowtie2 ⇒ ("G,", (20,8))` vs `(local&&hisat2) || end-to-end ⇒ ("L,", (0,-0.2))`.
- The existing test `score_min_params_local_defaults_and_parses_g` (`options.rs:516-524`) **calls `score_min_params(&cli)` with no aligner** and asserts `(20.0, 8.0)` for `--local` — it **will not compile** after the signature change and must be updated (add a Bowtie2-local arg + a new HISAT2-local `(0,−0.2)` assertion + an L-form-accept / G-form-reject case for HISAT2-local).

The plan's Context-5 and Behavior-4 *do* describe the right end state ("feed `(0,−0.2)` not `(20,8)` when `local && hisat2`"), but Implementation-outline step 3 frames it as **"Confirm `calc_mapq`'s local branch is intercept/slope-driven... it should need no change."** That sentence is true of `calc_mapq` but is the wrong altitude — the **bug is in `score_min_params`, not `calc_mapq`**, and it is a required signature + test change, not a "confirm." The implementation plan must call this out explicitly so it is not glossed.

---

## 2. Assumptions

- **A1 (option string).** Verified — `bismark:7907-7913`/`7946-7949`/`8311`. ✅
- **A2 (MAPQ `ln` + parity).** Verified; concur no new spike. ✅ (see §1.1.)
- **A3 (soft-clip→methylation aligner-agnostic).** Verified `methylation.rs:174`. ✅
- **A4 (minimap2 stays rejected — local by design).** Sound and well-supported: `bismark:8359` `@aligner_options = ();` wipes any prior `--score-min`/`--local` for minimap2 (clean slate), and minimap2 has no end-to-end-vs-local toggle. ✅
- **A5 (non-byte-identical to end-to-end by design).** ✅
- **A6 (hard-clip `H`/supplementary orthogonal to `--local`).** **Sound.** `--local` newly allows only soft-clipping (`S`, handled). `H` appears only on supplementary/`0x800` records, which HISAT2 end-to-end can *already* emit and which unique-best selection drops before calling; the CIGAR walk (`methylation.rs:192`) has no `H` arm and **errors loudly** on any unexpected op (`"illegal CIGAR operations"`) rather than silently mishandling. So `--local` does not widen `H` exposure, and any surprise `H` fails loud, not silent. ✅
- **Context-6 / "`--non_bs_mm` ⊕ local mutex — already ported (#981); HISAT2 inherits it."** **Imprecise but harmless.** In the Rust port, `--non_bs_mm` is **globally rejected** (`config.rs:589` "not yet supported in v1"), which is *stricter* than the Perl mutex (`bismark:8433-8438`, end-to-end only). So there is no live local-vs-non_bs_mm interaction to port; the broader reject pre-empts it. Recommend rewording to "`--non_bs_mm` is globally rejected in v1, so the Perl local mutex is moot" so a future implementer doesn't go hunting for a mutex that isn't there.

---

## 3. Validation sufficiency

**Mostly strong, two gaps.**

- **Gate matrix (SE+PE × {dir,non-dir,pbat}) vs Perl `--hisat2 --local`, HISAT2 2.2.2 pinned, @PG-filtered decompressed BAM + filtered report.** Correct and matches the #981/HISAT2 matrix. ✅
- **IMPORTANT — soft-clip non-vacuity is named but not *operationalized*.** The plan rightly says "the gate set must actually contain soft-clipped alignments... else the gate is vacuous," but does **not say how to guarantee that**. The whole headline behavioral change is "drop `--no-softclip`." On clean / well-trimmed reads against a matched genome, local and end-to-end **converge to zero soft-clips**, so a naive directional dataset can produce a gate that is byte-identical to *both* Perl-local and Perl-end-to-end while testing nothing. The implementation plan must:
  1. Use (or construct) a dataset with reads that have unalignable ends (untrimmed adapter / error-rich termini / Sherman-simulated terminal mismatches), and
  2. assert a **minimum soft-clip count** (e.g. `≥ N` records with `S` in CIGAR, N>0) in the gate harness — not merely "`S` appears." A diff-only gate can pass vacuously; make the non-vacuity an explicit machine-checked assertion.
- **`--multicore` cell** (`--hisat2 --local --multicore N`). Good — proves local-delta composes with the #986 `-p N --reorder` remap. The plan should also note the **option-assembly ordering** to assert in a unit test: splice flags → softclip-delta vs `-p N --reorder` (`options.rs:158-165` pushes `-p/--reorder` *before* `apply_aligner_specific_options` appends the softclip tail at `:328`), so the HISAT2-local string is `... -p N --reorder --ignore-quals ... --omit-sec-seq`. Add an options-string assertion for the `local + multicore` combination, not only an e2e cell.
- **Determinism confirm folded into the gate.** Reasonable; HISAT2 single-core is deterministic (proven by the shipped HISAT2 gates). ✅
- **Regression set** (Bowtie2-local, HISAT2 end-to-end ±multicore, minimap2). ✅ The `score_min_params` signature change is the main regression surface — the existing Bowtie2-local default `(20,8)` MUST stay green; the updated unit test covers it.
- **No conformance/methylseq flip.** ✅ Confirmed there is no `KnownUnsupported` row for HISAT2-local; it is not in methylseq's command surface.

---

## 4. Alternatives / observations

- **Doc drift (IMPORTANT).** Two checked-in docs will become **wrong** the moment HISAT2-local ships and are not in the plan's doc step:
  - `rust/README.md:61-62` currently states "`--local` alignment ... is supported for **Bowtie 2** ... **HISAT2/minimap2 local alignment ... [unsupported]**." The plan's step 5 mentions README only for the minimap2-is-local-by-design note; it must **also flip the HISAT2 clause** to "supported (byte-identical to Perl `--hisat2 --local`)."
  - `cli.rs:169-172` `--local` help: "Bowtie 2 local-alignment mode (soft-clipped ends; `--score-min G,20,8` ...)". Now aligner-dependent (HISAT2-local = L-form, no `--local` flag). Update the help string. The `config.rs:178-180` doc on `score_min_local` ("defaults are `(20.0, 8.0)` (G-form)") is likewise now aligner-conditional. Optional-to-Important (accuracy of the user-facing + maintainer-facing surface).
- **Reject-message wording (Q3).** The proposed minimap2 reject text is good. Note the **current** reject message (`config.rs:298-300`) lumps "HISAT2/minimap2"; after the flip it must mention **only minimap2** — the plan's step 1 says this, just make sure the test `resolve_rejects_local_with_non_bowtie2` (`config.rs:1060-1066`) is **split**: the `--local --hisat2` case must flip to `is_ok()` and the `--local --minimap2` case must stay `is_err()` with the new message. The plan's step-4 "config.rs reject flips for HISAT2 + stays for minimap2" covers it; just ensure the *existing* test is amended, not only a new one added.

---

## 5. Action items

### Critical
1. **`score_min_params` must change signature + branch on aligner** (`options.rs:347-352`). It currently hardcodes `("G,", (20.0,8.0))` on `cli.local` alone and takes no `aligner`. For `--hisat2 --local` it must yield L-form / `(0.0,−0.2)` (and accept a user L-form, reject G-form). Pass `aligner` (already in scope at `config.rs:360`). Reframe Implementation step 3 from "confirm `calc_mapq` needs no change" (true of `calc_mapq`, irrelevant to the bug) to "**fix `score_min_params` to be aligner-aware; `calc_mapq` is unchanged**." The existing test `score_min_params_local_defaults_and_parses_g` (`options.rs:516-524`) will not compile after the signature change — update it (Bowtie2-local `(20,8)` kept green + HISAT2-local `(0,−0.2)` + L-accept/G-reject).

### Important
2. **Operationalize soft-clip non-vacuity** in the gate (Validation). Specify a dataset that *produces* soft-clips (untrimmed/error-rich ends or Sherman terminal mismatches) and assert a **machine-checked minimum soft-clip count (>0)**, not just "`S` appears." Otherwise the byte-identity gate can pass vacuously (clean directional reads → 0 soft-clips → local ≡ end-to-end).
3. **Flip the HISAT2 clause in `rust/README.md:61-62`** (currently states HISAT2-local unsupported) and update the **`cli.rs:169` `--local` help** + the **`config.rs:178-180` `score_min_local` doc** to be aligner-conditional. The plan's doc step currently only covers the minimap2 note.
4. **Amend, don't just add, the reject test** `resolve_rejects_local_with_non_bowtie2` (`config.rs:1060-1066`): `--local --hisat2` must flip to OK; `--local --minimap2` stays Err with the new "local by design" message.

### Optional
5. Add the **`local + multicore` options-string assertion** (`... -p N --reorder --ignore-quals ... --omit-sec-seq`, no `--local`, no `--no-softclip`) alongside the e2e cell, to lock the assembly order at `options.rs:158-165` vs `:328`.
6. Add an **`as_best == 0` case** to the new HISAT2-local MAPQ unit test to exercise the `best_over == diff` exact-equality branch (`mapq.rs:173`), which negative-scMin HISAT2-local reaches via different inputs than Bowtie2-local.
7. **Reword Context-6 / the `--non_bs_mm` assumption**: it is *globally rejected* in v1 (`config.rs:589`), which pre-empts the Perl local mutex — so there is no mutex to "inherit." Avoids a wild-goose chase.

---

## Summary

The plan's reframing is **correct and verified** end-to-end against the Perl oracle and the Rust source, and its decision to skip a new spike is justified (the `ln()` parity transfers; the constants ride in the parameters, and `calc_mapq_local` is constant-free). The reuse story holds. The one real correctness hazard is that the actual fix lives in `score_min_params` (aligner-unaware, G-form hardcoded) — the plan describes the end state but mislabels the locus as a "confirm `calc_mapq`" no-op. Tighten that, operationalize the soft-clip non-vacuity, and refresh the two stale docs, and this is ready to implement.

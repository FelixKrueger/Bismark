# PLAN (scoping) — HISAT2 `--local` alignment in the Bismark aligner (v1.x)

**Date:** 2026-06-14
**Crate:** `rust/bismark-aligner` · **Base:** latest `rust/iron-chancellor` (incl. beta.8 / HISAT2-multicore #986)
**Status:** PLAN rev 2 (scoping) — manual review ✅ + **dual plan-review ✅** (both APPROVE-WITH-CHANGES,
`PLAN_REVIEW_A/B.md`, all findings folded). Awaiting implementation plan → implement trigger. **No Phase-0
spike** (the `ln()` parity risk was retired by #981; but see the rev-2 MAPQ-test requirement).

**rev-2 delta (dual plan-review, both reviewers source-verified the reframing):**
- 🔴 **The load-bearing edit is `score_min_params` (`options.rs:347-352`), NOT `calc_mapq`.** It resolves
  `(intercept, slope)` from `cli.local` *alone* and hardcodes the Bowtie 2 **G-form `(20,8)`** → for
  `--hisat2 --local` it returns the WRONG constants AND rejects a valid user L-form. Fix = thread `aligner`
  in (`score_min_params(cli, aligner)`; aligner is in scope at `config.rs:360`) + the same G/L branch as the
  `options.rs:82` block. The existing test `score_min_params_local_defaults_and_parses_g` (`options.rs:516`)
  won't compile after the signature change → **update it** (rev-1 step 3 wrongly said "calc_mapq needs no change").
- **Soft-clip non-vacuity must be OPERATIONALIZED:** clean directional reads rarely soft-clip even in local
  mode → the gate can pass vacuously. Mandate a **soft-clip-prone dataset** + a **blocking `S`-CIGAR-count > 0
  assertion**, SE *and* PE.
- **Mandatory Perl-cross-checked MAPQ unit test at `(0,−0.2)`** across read lengths: the HISAT2-local default
  gives a sub-unity `diff ≈ 0.78–1.0` with fractional bucket thresholds (vs Bowtie 2-local's `diff=10`), the
  most `ln()`-sensitive regime. The ladder is sign-agnostic (`diff=|scMin|`, verified) so no spike — but the
  existing `(20,8)`-only test doesn't cover this; add a cross-checked `(0,−0.2)` test.
- **Flip (not just add) the stale surfaces:** `rust/README.md:61-62` (says HISAT2-local unsupported),
  `cli.rs:169` `--local` help, `config.rs:178` doc; **amend** the reject test `resolve_rejects_local_with_non_bowtie2`
  (`config.rs:1060`) → HISAT2 OK / minimap2 Err.
- Remove `debug_assert_eq!(aligner, Bowtie2)` (`options.rs:83`) as a discrete step. `--non_bs_mm` is
  **globally** rejected in v1 (`config.rs:589`) — there is no local-mutex to "inherit" (Assumption 1-rewording).
  Q4 (Perl `--hisat2 --local` determinism) → a **blocking** Phase-1 prerequisite. Report `aligner_options` echo
  needs no code change. Assumption 6 (hard-clip orthogonality) CONFIRMED sound (CIGAR walk fails loud, `methylation.rs:192`).
**Origin:** the Bowtie 2 `--local` epic (#981) shipped Bowtie 2-only and **deferred HISAT2-`--local`** (the
`config.rs` reject comment: "HISAT2-`--local` is experimental in Perl"). This plan scopes that follow-up.
**Sibling arc:** mirrors `plans/06132026_aligner-local-mode/` (Bowtie 2 `--local`) and
`plans/06132026_aligner-hisat2-multicore/` (HISAT2 multicore) — scope → review → impl → oxy gate.

## Decisions locked (Felix, 2026-06-14)
- **Q1 → advance now** ("it's cheap"). HISAT2-`--local` will be implemented (faithful port of Perl's
  experimental HISAT2-local mode), not parked.
- **Q2 → SE + PE together** in one implementation phase (not SE-first), like the #981 Bowtie 2 `--local` epic.
- **Q3 → minimap2-`--local` stays REJECTED**, and the reject **must explicitly state that minimap2 is
  local by design** (no end-to-end vs local distinction) — surfaced in the reject message AND a docs note
  (`rust/README.md`). Implementation requirement, not just rationale.

---

## Goal

Support `--hisat2 --local` in the Rust `bismark` aligner, producing methylation-call output
**byte-identical to Perl Bismark v0.25.1 `--hisat2 --local`** for at least Bowtie 2's sibling matrix
(SE + PE × directional / non-directional / pbat). Today `--local` is **hard-rejected for any non-Bowtie 2
aligner** at `config.rs:295-300` (`AlignerError::Unsupported`).

**Non-goal (stays rejected):** **minimap2 `--local`** — minimap2 has no end-to-end-vs-local distinction
(it is inherently soft-clipping), and Perl has *no* minimap2-specific `--local` handling (the score-min
push is wiped by minimap2's clean-slate options; `--local` would only incidentally flip the MAPQ scMin to
log-form — an untested quirk). The current fail-loud reject is the **correct never-silent behavior**; this
plan keeps it (and optionally sharpens the message/docs). See Q3.

---

## Why it's rejected — and the key reframing

The Bowtie 2 `--local` epic (#981) put the *single authoritative* `--local` scope gate in `config::resolve`
(`config.rs:295`, `aligner != Bowtie2 ⇒ Unsupported`) so that `build_aligner_options` may assume
`local ⟹ Bowtie 2` (`options.rs:82`, `debug_assert_eq!(aligner, Bowtie2)`). So the local option-assembly +
MAPQ path is currently **Bowtie 2-shaped** (pushes `--local`, G-form `--score-min`, default `G,20,8`).

**The reframing (verified against the Perl oracle):** HISAT2 "local" is **NOT** a `--local` flag passthrough.
Perl explicitly does *not* pass `--local` to HISAT2 (`bismark:7904`/7943 inline comment: "this option does
not work with HISAT2"). HISAT2-local is defined by **subtraction + a MAPQ-formula switch**:

| Aspect | HISAT2 end-to-end (default) | HISAT2 `--local` | Source |
|--------|------------------------------|-------------------|--------|
| aligner score-min | `--score-min L,0,-0.2` | `--score-min L,0,-0.2` (**identical**; user override must be **L-form**) | `bismark:7916-7922` vs `7907-7913`/`7946-7948` |
| soft-clip delta | `--no-softclip --omit-sec-seq` | **`--omit-sec-seq` only** (drops `--no-softclip` → HISAT2 may soft-clip) | `bismark:8314` vs `8311` |
| `--local` flag to aligner | not pushed | **not pushed** (HISAT2 has no `--local`) | `bismark:7904` comment |
| MAPQ `scMin` | `intercept + slope·readLen` (linear) | `intercept + slope·ln(readLen)` (**log**), intercept/slope = **(0, −0.2)** L-form | `bismark:3932` (`$local ? log $read1Len : $read1Len`) + `7912/7947` |

So the **only alignment difference** from HISAT2 end-to-end is "soft-clipping is allowed" (drop
`--no-softclip`); the **only call/QC difference** is the local MAPQ `ln()` ladder. ⚠️ Perl flags this path
`[EXPERIMENTAL]` (a commented-out warn at `bismark:8310`) — but it **runs deterministically** (HISAT2
single-core is deterministic; proven by the Phase-2a/2b gates), so it is a **valid byte-identity oracle**.

**Most of the machinery already exists from #981 (Bowtie 2 `--local`):**
- the local MAPQ ladder + `ln()` `scMin` in `mapq.rs` (driven by `score_min_local` + intercept/slope —
  aligner-agnostic: it just needs the right `(intercept, slope)`);
- the **`ln()` Perl≡Rust bit-identity was already PROVEN on the gate arch** (#981 spike: 0 ULP / 152,709
  cases, both arm64 + x86_64) — HISAT2-local uses the *same* `ln()` with `(0, −0.2)` → **no new Phase-0
  spike needed** (the transcendental risk is already retired);
- soft-clip-as-`I` handling in `methylation.rs:174` (treats CIGAR `S` like `I`, aligner-agnostic).

So this is a **faithful port of a different aligner's local mode**, reusing #981's core, with a new
HISAT2-local *option/validation branch* — not new MAPQ or soft-clip machinery.

---

## Context — the pieces HISAT2-`--local` touches (all in `bismark-aligner`)

1. **`config.rs:295` reject** — currently `aligner != Bowtie2 ⇒ Unsupported`. Must become
   `aligner == Minimap2 ⇒ Unsupported` (Bowtie 2 **and** HISAT2 fall through; minimap2 still rejected).
2. **`options.rs:82-103` local block** — currently `debug_assert_eq!(aligner, Bowtie2)` + pushes `--local`
   + G-form `--score-min` (default `G,20,8`). Needs an **aligner branch**: HISAT2-local must NOT push
   `--local`, must validate/emit **L-form** `--score-min` (default `L,0,-0.2`), and must NOT add the
   end-to-end `--no-softclip` (only `--omit-sec-seq`).
3. **The HISAT2 softclip delta** (`options.rs` ~`apply_aligner_specific_options`, the `--no-softclip
   --omit-sec-seq` append, Perl `bismark:8313-8315`) — must become **`--omit-sec-seq` only** when
   `local && hisat2`.
4. **Score-min validation** — `valid_score_min_g` (Bowtie 2 local) vs `valid_score_min_l` (HISAT2 local +
   end-to-end). Branch the local validation on aligner.
5. **`score_min_params` / `score_min_local` / `calc_mapq`** — the local MAPQ branch already exists; confirm
   it consumes the HISAT2-local `(0, −0.2)` L-form intercept/slope (the resolution must feed those, not the
   Bowtie 2 `(20, 8)` G-form defaults, when `local && hisat2`).
6. **`--non_bs_mm`** — **globally rejected in v1** (`config.rs:589`), so there is NO Perl local-mutex
   (`bismark:8434`) to port/inherit here (rev-1 said "already ported, HISAT2 inherits" — imprecise; the
   combination simply can't arise while `--non_bs_mm` is unsupported). No action.

**Interactions to verify (not necessarily new code):**
- **HISAT2-local + `--multicore N`** (the just-shipped #986 `-p N` remap): orthogonal — local changes the
  option *delta* + MAPQ; the `-p N --reorder` route is independent. Confirm the option-assembly order
  (local delta + `-p N --reorder` + splice flags) and that the gate covers it.
- **HISAT2-local + splice flags** (`--no-spliced-alignment` / `--known-splicesite-infile`): orthogonal —
  pushed before the softclip delta; local only changes the softclip delta. Should compose.
- **PE HISAT2-local:** the PE local `scMin` adds the second mate's `ln(read2Len)` term (`bismark:3935`,
  already in `mapq.rs`); per-mate XM extraction + soft-clip-as-`I` already handle soft-clipped mates
  (#981 verified PE has no fragment-overlap work in the aligner). So PE reuses, like #981.

---

## Behavior (Perl-faithful, the testable spec)

For `--hisat2 --local`:
1. **Reject gate:** `aligner == Minimap2 && local ⇒ Unsupported`; Bowtie 2 + HISAT2 pass.
2. **Score-min:** if `--score_min` given, require **L-form** (`L,<i>,<s>`) — else die with the HISAT2-local
   L-form message (`bismark:7908-7909`); emit `--score-min L,<i>,<s>`. If absent, default `L,0,-0.2`
   (`bismark:7947-7948`). **No `--local` flag is emitted.**
3. **Softclip delta:** emit `--omit-sec-seq` (NOT `--no-softclip --omit-sec-seq`) for HISAT2-local.
4. **MAPQ:** `score_min_local = true` → `scMin = intercept + slope·ln(readLen)` with `(intercept, slope) =
   (0, −0.2)` (or the user's L-form values) → the local MAPQ ladder (`mapq.rs`, already byte-exact).
5. **Everything else** (strand instances, XM/XR/XG, BAM/report, `--ambig_bam`, `--unmapped`/`--ambiguous`)
   identical to HISAT2 end-to-end — local only toggles soft-clipping + the MAPQ formula.

---

## Implementation outline (single phase; no Phase-0 spike — `ln()` parity already proven by #981)

1. **`config.rs`** — change the `--local` reject from `aligner != Bowtie2` to `aligner == Minimap2`
   (Bowtie 2 + HISAT2 fall through; minimap2 fail-loud). The minimap2 reject message **must state minimap2
   is local by design** (Q3), e.g. *"--local is not supported with --minimap2: minimap2 performs local
   (soft-clipping) alignment by design — there is no end-to-end vs local distinction to toggle. Use
   --bowtie2 or --hisat2 for --local."* Update the doc comment to match.
2. **`options.rs`** — replace the `debug_assert_eq!(aligner, Bowtie2)` in the local block with an aligner
   branch:
   - Bowtie 2 local: unchanged (push `--local`, G-form, default `G,20,8`).
   - HISAT2 local: push **L-form** `--score-min` (validate `valid_score_min_l`, default `L,0,-0.2`), do
     **not** push `--local`.
   - In the HISAT2 softclip-delta assembly: emit `--omit-sec-seq` only when `local`, else
     `--no-softclip --omit-sec-seq` (end-to-end).
3. **🔴 `score_min_params` (`options.rs:347`) — the load-bearing MAPQ edit (NOT `calc_mapq`).** It currently
   keys on `cli.local` alone → hardcodes G-form `(20,8)`. Change the signature to `score_min_params(cli,
   aligner)` (aligner in scope at `config.rs:360`) and branch: Bowtie 2-local → G-form default `(20,8)`;
   HISAT2-local → **L-form default `(0,−0.2)`**, validate `valid_score_min_l`. The resolved `(intercept,
   slope)` + `score_min_local` feed `calc_mapq`, whose local branch is constant-free/sign-agnostic
   (`diff=|scMin|`, `bestOver=AS−scMin`) → **no `calc_mapq` change**. Update the now-uncompilable test
   `score_min_params_local_defaults_and_parses_g` (`options.rs:516`) + add the `(0,−0.2)` cross-checked test (below).
4. **Tests (TDD):** options-string assertions (HISAT2-local emits `--score-min L,0,-0.2 --omit-sec-seq`, no
   `--local`, no `--no-softclip`; L-form override accepted, G-form rejected); a MAPQ unit test for the
   HISAT2-local `(0,−0.2)` ln ladder; e2e fake-HISAT2 soft-clip round-trip; the `config.rs` reject flips for
   HISAT2 + stays for minimap2.
5. **Docs:** note HISAT2-local support in `rust/README.md`, and that **minimap2-`--local` stays rejected
   because minimap2 is local by design** (Q3 — the user-facing rationale, not just a bare reject).

---

## Validation

- **Implementation gate = byte-identical to Perl v0.25.1 `--hisat2 --local`** (decompressed BAM,
  @PG-filtered, + report wall-clock/version-filtered) on the oxy real-data harness, **SE + PE ×
  {directional, non-directional, pbat}** (the #981 + HISAT2-gate matrix). HISAT2 2.2.2 pinned.
- **Determinism confirm (folds into the gate, not a separate spike):** Perl `--hisat2 --local` run twice =
  same md5 (standard; the `[EXPERIMENTAL]` label is about biological validity, not determinism).
- **Soft-clip coverage:** the gate set must actually contain soft-clipped HISAT2 alignments (the local-only
  behavior) — confirm `S` appears in the gate BAM CIGARs (else the gate is vacuous for the headline change).
- **`--multicore` interaction:** one gate cell `--hisat2 --local --multicore N` (== Perl `--hisat2 --local
  -p N`) — proves local + the #986 remap compose.
- **Regression:** Bowtie 2 `--local` (#981), HISAT2 end-to-end, single-core + multicore HISAT2, minimap2 —
  all untouched/green. No conformance flip needed (HISAT2-local is **not** in methylseq's command surface,
  so it is not a `KnownUnsupported` row).

---

## Assumptions

1. **(Verified, Perl source)** HISAT2-local emits exactly `--score-min L,<i>,<s>` (default `L,0,-0.2`) +
   `--omit-sec-seq`, **no `--local`, no `--no-softclip`** (`bismark:7907-7913`/`7946-7948`/`8311`).
2. **(Verified)** HISAT2-local MAPQ uses the local `ln()` scMin with `(0, −0.2)` L-form intercept/slope
   (`bismark:3932` + `7912/7947`); the `ln()` Perl≡Rust bit-identity is already proven (#981 spike) → no new spike.
3. **(Verified)** soft-clip→methylation mapping is already aligner-agnostic (`methylation.rs:174`, `S` like `I`).
4. **(Fixed)** minimap2-local stays rejected (non-mode; the reject is the correct behavior).
5. **(Fixed)** This is non-byte-identical to HISAT2 *end-to-end* by design — `--local` is a distinct mode;
   the gate is Perl-`--hisat2 --local`; the HISAT2 end-to-end default path is untouched.
6. **(Fixed) Hard-clip `H` / supplementary (`0x800`) is orthogonal to `--local` and introduces no new work.**
   `--local` only newly allows **soft-clipping** (`S`, already handled `methylation.rs:174`). Hard-clips
   appear only on **supplementary/chimeric** records — which HISAT2 end-to-end can already emit and which
   Bismark's unique-best selection drops before methylation calling (the caller's CIGAR walk handles
   `M/I/S/D/N`, no `H` arm — H falls to the catch-all). HISAT2 end-to-end's supplementary handling is
   already covered by the shipped Phase-2a/2b gates, so `--local` does not widen hard-clip exposure. (This
   is the minimap2-specific wall that makes minimap2 unsuitable — minimap2 under `map-ont` is far more
   chimeric-prone and `--secondary=no` does NOT suppress `0x800` — reinforcing Q3's keep-rejected.)

---

## Questions or ambiguities

- **Q1 (priority) → RESOLVED: advance now** (Felix, "it's cheap"). Implement, don't park.
- **Q2 (scope) → RESOLVED: SE + PE together** (Felix), one implementation phase (PE adds only the second
  `calc_mapq` length arg, already handled — the aligner extracts each mate's XM independently, no overlap work).
- **Q3 (minimap2) → RESOLVED: keep REJECTED + state "minimap2 is local by design"** (Felix) in both the
  reject message and `rust/README.md` (see implementation outline steps 1 + 5).
- **Q4 (oracle stability — Open, low risk):** confirm Perl `--hisat2 --local` is a stable run-to-run oracle
  before the gate (folded into Phase-1 setup; expected YES — HISAT2 single-core is deterministic).

---

## Self-Review

- **Logic:** the touch-points are derived from the Perl source (the option delta = drop `--no-softclip`;
  L-form not G-form; no `--local` push; `(0,−0.2)` MAPQ) and from #981's reusable core. The reject lift is a
  one-conjunct change mirroring the HISAT2-multicore reject-lift pattern (#986).
- **Edge cases:** L-form vs G-form validation branch (Bowtie 2 G / HISAT2 L); soft-clip CIGARs (already
  handled); `--non_bs_mm` mutex (already ported); `--multicore` + local composition (one gate cell); PE
  second-mate `ln` term (already in `mapq.rs`).
- **Integration:** opt-in branch; Bowtie 2-local + HISAT2-end-to-end + minimap2 untouched; no methylseq
  conformance impact (not in its surface).
- **Spike:** none needed — the only transcendental risk (`ln()` parity) was retired by the #981 spike, and
  HISAT2-local uses the same `ln()`. The remaining unknowns are bounded engineering + the oxy byte-identity gate.
- **Remaining risk:** low — the headline change is "drop `--no-softclip` + L-form + local MAPQ", all on
  proven machinery. The main risk is a missed option-assembly-order detail (mitigated by the byte-identity
  gate, which must include soft-clipped reads to be non-vacuous). The `[EXPERIMENTAL]` Perl label is a
  *priority* consideration (Q1), not a correctness blocker (the mode is deterministic → byte-gateable).

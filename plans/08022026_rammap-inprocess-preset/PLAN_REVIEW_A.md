# PLAN_REVIEW_A — #1092 in-process rammap preset (PLAN.md rev 1)

**Reviewer:** A (independent) · **Date:** 2026-08-02 · **Base:** `dev` `11efbab`, clean tree
**Target:** `plans/08022026_rammap-inprocess-preset/PLAN.md` rev 1 + `SPIKE.md`
**Verdict:** **NOT ready as written — 2 Critical items, both in the validation section, not in the design.**

The design is right: one production site, a typed single-sourced enum, exhaustive bridge, default path unmoved. §1's scope claim is correct — I verified it independently. What does not hold up is the *teeth*: **V7 cannot fail V8's injection (i)**, and **V2 cannot fail V8's injection (iii)**. Both are claimed as green-means-safe in rev 1. Separately, the plan (and `SPIKE.md`) rest on a premise I found to be false in a way that *helps*: a real `.mmi` **can** be produced hermetically, so a genuine end-to-end wiring gate is buildable.

---

## 0. What I checked (and how)

| Claim | Method | Result |
|---|---|---|
| `mod.rs:968` is the only production `Preset::` site | `grep -rn "Preset::" rust --include='*.rs'` | ✅ 4 sites, 1 production (see Important 7 — the census is one site short) |
| `--mm2_maximum_length` honoured backend-independently | read `convert.rs:325-336` + `mod.rs:836` | ✅ convert-stage `continue`, before either backend |
| `-p` honoured in-process | read `config.rs:541-559`, `:868-877` | ✅ `inprocess_rammap_threads` takes `bowtie_threads` first |
| No third defective `--mm2_*` knob | `grep -n "mm2\|maximum_length" cli.rs` | ✅ exactly 4 flags, all accounted for |
| Conflict-die ordering after a new `config::resolve` call | read `config.rs:683`, `:831`, `:1281-1310` | ✅ safe, but only under a placement constraint the plan states as a to-confirm (Important 4) |
| Spike's `from_seqs` ⇒ `from_index` assumption | **re-ran it**, scratchpad crate against `rammap-core` `5ea62cd` | ✅ numbers identical under both constructors (details below) |
| `MapPb` distinguishability on the production path | same run + read `api.rs:233-243, 455-473, 617-619, 756` | ⚠️ near-inert, and *not* for the reason `SPIKE.md` gives (Important 3) |
| Existing tests protect the die strings/order | read `options.rs:897-929` | ❌ they do not (Critical 2) |

**My verification run** (throwaway crate in the session scratchpad, `rammap-core =1.1.1` rev `5ea62cd`, `--release`; the repo was not touched):

```
save_index: OK -> /…/presetcheck_idx/BS_CT.mmi (107471 bytes)

--- from_index (PRODUCTION constructor), index built k=15/w=10 ---
preset     mm3_150                      perfect_150
MapOnt     AS=282 cig=150M mapq=60      AS=300 cig=150M mapq=60
MapPb      AS=282 cig=150M mapq=60      AS=300 cig=150M mapq=60
Sr         AS=270 cig=150M mapq=60      AS=300 cig=150M mapq=60

--- from_seqs (SPIKE constructor) ---   identical to the above, all three rows

MapOptions identity under from_index:  MapOnt == MapPb : false   (one field: chn_pen_gap 0.12 → 0.152)
                                       MapOnt == Sr    : false   (19 differing lines)
index k/w seen by from_index: k=15 w=10
mm3_150 Sr from_index determinism (3 runs identical): true
```

Two things follow immediately: **`SPIKE.md` §7's unverified `from_seqs`/`from_index` limitation is now closed** (the 282/270 delta is constructor-independent, so V7's numbers are safe either way), and **`Aligner::save_index` exists and round-trips** (`api.rs:425-428` → `from_index`, `api.rs:249`), which removes the obstacle the whole §9a debate was built on.

---

## 1. Critical

### C1 — V7 as specified cannot fail V8 injection (i). The plan's decisive fault still escapes.

§9a's "one structural requirement" prescribes extracting `inprocess_preset(config: &RunConfig) -> ::rammap::Preset`, having `build_se_inprocess_streams` call it, and having V7 **drive that function** from a `RunConfig`; `SPIKE.md` §5 then builds a `from_seqs` aligner from the returned preset and asserts `score`.

Trace injection (i) — "revert `mod.rs:968` to the `Preset::MapOnt` literal" — against that test:

1. `inprocess_preset(config)` still exists and still returns `Sr`.
2. V7 still calls it, still hands `Sr` to its own `from_seqs`, still gets **270**.
3. `dead_code` does not fire — the test references the function.
4. **V7 is green. The fix is a no-op in production.**

The seam that matters is the `from_index` **call**, not the preset **computation**. §9a correctly diagnoses that a test building its own aligner "proves the bridge, not the wiring" — and then prescribes a fix that still only proves the bridge, because the extracted function is upstream of the call it is meant to protect. V8's "Injection (i) is the one that matters, and after the spike it is genuinely caught" is not true of any test the plan describes. This is precisely the #1079 silent-no-op shape §9a says this lineage keeps hitting.

**The good news: the blocker that forced this compromise does not exist.** §2/§9a assert that "`from_index` needs a real `.mmi` while `make_genome_mmi` writes a 1-byte placeholder" (the placeholder is the *test fixture*, `discovery.rs:476-477`). But `rammap::Aligner::save_index` (`api.rs:425`) writes an RMMI file that `from_index` reads back by magic detection — I produced a 107 KB `.mmi` from a 20 kb synthetic reference and reproduced the spike table through it exactly. So a hermetic **end-to-end** gate is buildable. Two shapes, in preference order:

**(1) Preferred — reuse the #1079/#1081 harness.** `tests/aligner_cli.rs:113-170` already drives the real binary → `config::resolve` → merge → BAM with a *fake* aligner binary; that pattern is the direct precedent, and `detect_aligner` only needs `rammap --version` to exit 0 (`aligner.rs:103-110`), so a 2-line script suffices — the in-process FastQ path never execs it. Then:
- genome fixture whose `Bisulfite_Genome/{CT,GA}_conversion/BS_{CT,GA}.mmi` are **real** `save_index` outputs over `chr1_CT_converted` / `chr1_GA_converted` (directional SE loads *both*, `mod.rs:973-985`, so both must be real);
- a 150 bp read with 3 mismatches; run `--rammap --mm2_short_reads`;
- assert `AS` in the output BAM: **270**, and **282** for the same run without the flag.

Injection (i) then flips 270 → 282 at exactly `mod.rs:968`. Price it honestly before committing: the read has to survive Bismark's own length/XM path to reach the BAM, and the fixture genome FASTA must match the indexed sequence. Worth a short feasibility check — everything it needs is now known to exist.

**(2) Cheap fallback, if (1) does not come together.** Keep the `from_seqs` test but **rename it so nobody reads it as a wiring gate** (it gates the resolver + bridge, which V3/V4 already do), and add a source-drift guard in the style of the repo's own `tests/summary_template_drift.rs`: assert that `include_str!(".../mod.rs")` contains no `::rammap::Preset::` literal outside `inprocess_preset`. That is a structural assertion, but unlike §9a's version it actually fails injection (i).

Either way: **rev 2 must stop claiming the described V7 catches (i).** Shipping "the fix is gated" when it is not is worse than shipping it labelled ungated — that is the same honesty standard the plan applies to A4.

### C2 — V2 has no teeth. A6 is false, and V8 injection (iii) cannot fail.

§3.1, A6 and V2 all rest on: "Their message strings are Perl-faithful and **asserted by** `minimap2_preset_conflicts_die`". The test (`options.rs:914-929`) is:

```rust
for conflict in [["--mm2_short_reads","--mm2_nanopore"],
                 ["--mm2_short_reads","--mm2_pacbio"],
                 ["--mm2_pacbio","--mm2_nanopore"]] {
    assert!(build_aligner_options(&cli, Aligner::Minimap2, …).is_err(), "{conflict:?} should die");
}
```

It asserts **`is_err()` only**. No message text, ever. Consequences:

- The extraction can rewrite, merge or truncate any of the three Perl-faithful strings and V2 stays green.
- **Reordering is invisible by construction**: with only *pairs* as inputs, every order still errors on every pair. V8's "reorder the conflict dies → V2 must fail" is false.
- Order is observable *only* on a triple. `--mm2_short_reads --mm2_pacbio --mm2_nanopore` today yields *"Please select minimap2 in Short Read or Nanopore mode, but not both..."* (short⊕nanopore is checked first, `options.rs:261-266`). **No test covers that case.**

So the plan's entire safety argument for relocating Perl-faithful stderr — §11 risk 3's "mechanical, but it is stderr behaviour with tests" — is unsupported: there are no such tests.

**Required:** V2 must *add* assertions, not merely "pass untouched": exact `assert_eq!`/`contains` on all three messages, plus the triple-conflict case pinning which message wins. Land them **before** the extraction so they are captured against today's code — a post-extraction baseline pins whatever the extraction produced, which is exactly the failure mode being guarded.

---

## 2. Important

### I3 — §3.3, §7 and step 6 overstate what changes for `--mm2_pacbio`; §7's "different `k`/`w`" is wrong for every preset

`from_index` → `from_loaded_index` → `build_options(preset, index.kmer_size, index.window_size)` (`api.rs:233-243`, `:455-473`). Inside `build_options`, `k`/`w`/`is_hpc` are **locals**: `apply_preset_str` overwrites them from the preset and `build_options` then **discards them** and keeps the loaded index untouched. Bismark's `.mmi` is built by genome prep with a fixed `-k 20` and no preset (`genome_prep/indexer.rs:117-124`). So on the in-process path:

- **No preset changes `k`/`w`/HPC.** §7's "and a different `k`/`w`" is wrong — for `Sr` as well as `MapPb`.
- `map-pb` sets *only* `is_hpc = true; k = 19` (`api.rs:621-623`), both discarded. Measured residue vs `MapOnt` under `from_index`: **exactly one option**, `chn_pen_gap` 0.12 → 0.152 — a chaining-gap coefficient derived from the *preset's* k at `api.rs:756`. Identical AS/CIGAR/MAPQ on every 150 bp cell I ran.
- `Sr`'s ~19 option deltas (scoring, flags, `min_chain_score`, `min_dp_max`, `end_bonus`, bandwidth…) **all** survive, which is why `Sr` is the consequential arm.

Two corrections follow. First, the user-facing text: §3.3's "`--mm2_pacbio` … after: **`map-pb`**" is true of the value passed, not of the output; step 6's "alignments change for those two invocations" over-promises. The release note should say `sr` changes alignments and `map-pb` is near-inert in-process (its distinguishing parameters are index-build parameters that a prebuilt index overrides — very probably the same in the subprocess backend, which loads the same `.mmi`, though I did not verify the rammap CLI). Second, this *shrinks* §11 risk 1: a mis-mapped `MapPb` arm has essentially no observable effect on the production path, so the missing behavioural gate for `MapPb` matters less than the plan fears — and for a stronger reason than `SPIKE.md` F2's "changes seeding, not scoring" (under `from_index` it changes neither).

### I4 — §5 step 3's ordering question is answerable now; make it a hard placement constraint

The plan defers this to implementation ("Confirm it cannot error…"). It is decidable from the source, and the answer is yes-if-placed-correctly:

- `resolve_mm2_max_length(cli, aligner)?` runs at **`config.rs:683`** and dies for **any** of `--mm2_short_reads` / `--mm2_maximum_length` / `--mm2_pacbio` / `--mm2_nanopore` when the aligner ∉ {Minimap2, Rammap} (`config.rs:1281-1310`).
- Every conflict die needs **≥2** of the three selectors. So for Bowtie 2 / HISAT2, `:683` always fires first — including the non-obvious `--illumina_5base --bowtie2 --five_base_index X` route (`config.rs:1222-1233`), which resolves to `Aligner::Bowtie2`.
- For Minimap2 / Rammap, `build_aligner_options` at **`config.rs:831`** already runs `minimap2_options` → the same three dies with the same strings.

Therefore a new `resolve_mm2_preset` call placed beside `score_model` (`config.rs:841`) is **provably never the first error for any input**. Rewrite step 3 as a constraint — *"the new call must be placed after `:683` and after `:831`; anywhere earlier changes which message a `--bowtie2 --mm2_short_reads` run prints"* — rather than a to-confirm. (Corollary: the resolver then runs twice per minimap-family run. Harmless — same function, cannot drift — and see O11 for why the single-call alternative is worse.)

### I5 — §4's `Mm2Preset` doc is false for one reachable configuration

> "Meaningful only for minimap2/rammap; `MapOnt` otherwise"

`--illumina_5base --bowtie2 --five_base_index X` → `Aligner::Bowtie2` with `illumina_5base = true`, so `resolve_mm2_preset` returns **`Sr`** for a Bowtie 2 run. Inert (nothing reads it), but the comment will read as a lie to the next person. Either reword ("the resolver's answer for these flags, regardless of aligner"), or make the field `Option<Mm2Preset>` — `None` for non-minimap-family runs, with the bridge returning a fail-loud error rather than defaulting. The `Option` shape fits the plan's own no-silent-default philosophy (§3.4's exhaustive-match-no-`_` argument) better than a documented dummy.

### I6 — the new `Sr` tests will be the first to touch a process-global SIMD cap

`apply_preset_str` calls `set_simd_cap` on every `Aligner::from_*`, and that is a **documented process-global** `AtomicU8` (`align/dp/mod.rs:45-52`): `sr` pins AVX2, the other presets reset to `Auto`. Every existing feature-gated test uses `MapOnt`, so this has never been exercised. Once V4/V7 construct `Sr` aligners, tests running concurrently in the same binary will flip the cap mid-map on the x86 `rammap-inprocess` CI job. Perf-only **if** the AVX2 and AVX-512 DP kernels are score-identical; if they are not, this is a new order-dependent nondeterminism source and the gate cell is what would redden. Confirm kernel equivalence, or note it and keep the preset assertions inside a single test function. (No production consequence — both indexes load under one preset.)

### I7 — the `Preset::MapOnt` literal census is one site short

Step 5 aims for "no `Preset::MapOnt` literal survives anywhere; keeps a future grep honest", listing `inprocess.rs:702` and `:754`. There is a fourth: **`rust/bismark/tests/aligner_rammap_inprocess_crosscheck.rs:147`**. That file is also option (c)'s host, so it is the natural place to take the preset from an env var or loop the presets — and if the fallback guard in C1 is adopted, it must not be defeated by a stale literal there.

---

## 3. Optional / accuracy

- **O8 — wrong file in §2.** The Files table points the implementer at `rust/bismark/tests/aligner_cli.rs` for `minimap2_preset_selection` / `minimap2_preset_conflicts_die`. They live in the inline `mod tests` of **`rust/bismark/src/aligner/options.rs`** (`:897`, `:916`). `tests/aligner_cli.rs` does exist, so this is a live wrong turn.
- **O9 — line drift.** `minimap2_options` is `options.rs:258-290` (plan says `:256-289`); the dies are `:261-278` (plan says `:259-274`); the design#3 comment is `config.rs:1281-1283` (plan says `:1278-1280`). Trivial, but this lineage navigates by `file:line`.
- **O10 — determinism provenance.** The spike's 3× re-run measured **`perfect_60`**, not the gate cell: `spike_preset_observable.rs:127` takes `discriminating.first()`, and `run.log:25` reads `determinism on perfect_60`. §9a's "deterministic across repeat runs" for `mm3_150` was therefore not measured. It is true — I measured it, 3/3 identical under `from_index` — but cite it correctly.
- **O11 — proportionality: the plan's shape is right, and I checked the obvious simplification.** Four moving parts for a one-line fix looks heavy, so I priced the natural alternative: return the preset from `minimap2_options` through `build_aligner_options`'s tuple, eliminating both the separate resolver and I4's ordering hazard. It is **worse** — ~40 call sites in `options.rs`'s tests destructure `(opts, _)`, so it breaks V1's "the tests pass untouched" property, which is the plan's single best inertness signal. The `Mm2Preset` + resolver + `RunConfig` field + bridge shape mirrors the existing `ScoreModel::from_emitted` idiom (`config.rs:841-847`); I would not call it over-engineered. The *document* is over-long: D-GATE is restated in the header table, §9a, §10 and §12, and §11 has two items numbered "3".
- **O12 — pin the whole derivation in the gate comment.** §11 risk 3's `2·150 − 3·(2+8) = 270` is correct and I confirmed it empirically. Note that `sr` also sets `end_bonus = 10` (`api.rs:~692`) which does **not** appear in this cell's score; a future rammap that applies end bonuses differently moves the number for a reason unrelated to `mismatch_penalty`. Comment the full arithmetic, not just the mismatch term, so the next bumper can tell which parameter moved.
- **O13 — V1 is the strongest thing in the plan.** "Any edit to an expected string means the extraction changed behaviour" is exactly right, and `options.rs` already pins the full string for the default, `-t 8`, rammap and all three selectors (`:876`, `:892`, `:900-911`), plus the clean-slate and Bowtie 2/HISAT2 regression guards. Unlike V2, V1 genuinely has teeth as written.

---

## 4. Bottom line

**Design: approve.** One production site (verified), the typed single-source approach is the right lesson from #1079, the default path provably does not move, and §1's scope claim — preset-only, `--mm2_maximum_length` and `-p` already honoured, no third defective knob — is independently confirmed.

**Validation: block on C1 and C2.** Both are cases where the plan states a gate is protective and the mechanism cannot detect the fault. C1 is the more serious: as specified, the entire fix could be reverted at `mod.rs:968` with CI fully green. C1 also has a better answer than rev 1 believes — `save_index` makes a real end-to-end gate buildable, which is what §9a wanted all along.

**Then fix the text:** I3 (do not promise `map-pb` alignment changes, drop the `k`/`w` claim), I4 (placement as a constraint), I5 (the doc comment), I7 + O8/O9/O10 (accuracy).

Report: `/Users/fkrueger/Github/Bismark/plans/08022026_rammap-inprocess-preset/PLAN_REVIEW_A.md`

# PLAN — in-process rammap must honour the `--mm2_*` preset selector

**Issue:** [FelixKrueger/Bismark#1092](https://github.com/FelixKrueger/Bismark/issues/1092) (found while implementing [#1081](https://github.com/FelixKrueger/Bismark/issues/1081))
**Type:** correctness fix — a silently-ignored CLI flag on the `--rammap` default backend
**Revision:** **rev 3** — dual plan-review folded (`PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md`); decisions locked in rev 1 (see §12)
**Spike:** `SPIKE.md` — option (b) succeeded, but ⚠️ **its blocking premise was wrong**: `rammap::Aligner::save_index` exists, so the *production* constructor `from_index` is hermetically testable and a true end-to-end gate is buildable (rev 2, both reviewers)
**Base:** `dev` `11efbab`

**Decisions locked (Felix, 2026-08-02):**

| # | Decision | Consequence |
|---|---|---|
| **D-GATE** | **(a) structural + spike (b); (c) as a follow-up.** | Honoured, and rev 2 makes it *stronger* than rev 1 managed: `save_index` lets V7 drive the real `from_index`, so injection (i) is genuinely caught — see §9a. |
| **D-A4** | In-process `sr` ships **unmeasured** (deferring (c) is what defers the measurement). | The release note must say `sr` is newly reachable in-process and that its concordance is ungated — never imply otherwise. |

---

## 1. Goal

`--rammap` defaults to the **in-process** backend, and that backend hard-codes the preset:

```rust
::rammap::Aligner::from_index(mmi_str, ::rammap::Preset::MapOnt)   // mod.rs:968
```

It never reads the preset the CLI selected. The **subprocess** path does honour it (`options::minimap2_options` fires for `Aligner::Rammap` too, so `-x sr` reaches the external binary). So today:

| backend | `--rammap --mm2_short_reads` actually runs |
|---|---|
| in-process (**the default**) | `map-ont` |
| `--rammap_subprocess` | `sr` |

The flag is accepted, nothing is printed, and nothing warns that it had no effect. Make the in-process backend use the selected preset.

### Why this is a fix and not a feature

`config.rs:1278-1280` states the design intent explicitly:

> rammap is minimap-like — it honors the **SAME `--mm2_*` knobs** + length cutoff (design#3, for apples-to-apples fairness vs `--minimap2`).

Accepting these flags for rammap is deliberate, precisely so the two backends can be compared under identical presets. A backend that accepts the flag and ignores it defeats the reason the flag was accepted, and it silently breaks the comparison the design exists to enable. It also violates the suite's never-silent rule.

### Scope: the preset selector only (the other two `--mm2_*` knobs were checked and are fine)

| knob | in-process status |
|---|---|
| `--mm2_short_reads` / `--mm2_pacbio` / `--mm2_nanopore` | ❌ **ignored** — this fix |
| `--mm2_maximum_length` | ✅ honoured. Applied at the **convert** stage (`convert.rs:332`, drops over-long reads), which runs before alignment and produces the temp files *both* backends read (`mod.rs:836` passes `&converted` into the in-process builder). Backend-independent by construction |
| `-p` / `--multicore` (the `-t` analogue) | ✅ honoured. `inprocess_rammap_threads` takes `-p N` first (#1074; `config.rs:385`, `mod.rs:1136`) |

### Explicit non-goals

| Non-goal | Why |
|---|---|
| Changing subprocess-rammap or minimap2 behaviour | They already honour the preset. This fix makes in-process agree with them, not the reverse. |
| Widening the reachable preset set | Bismark selects exactly three (`map-ont`, `map-pb`, `sr`); `rammap::Preset` has all three plus 13 more. The extra 13 stay unreachable. |
| Making in-process `sr` **byte-identical** to subprocess `sr` | rammap is concordance-gated, not byte-frozen, and the two paths already diverge ≤0.022 %/cell on long reads via the library-vs-CLI API difference. See A4. |
| Fixing `--illumina_5base`'s use of `sr` | 5-Base is PE-only and runs its own path; it never reaches the SE in-process builder. Confirmed, A5. |

---

## 2. Context

### The blast radius is one production line

| Site | Status |
|---|---|
| **`mod.rs:968`** — `from_index(mmi_str, Preset::MapOnt)` inside `build_se_inprocess_streams` | **The only production site.** Has `config: &RunConfig` in scope |
| `inprocess.rs:702`, `:754` | ⚠️ **Both inside `#[cfg(test)]`** (verified) — `from_seqs` with a synthetic reference where the preset is immaterial. #1092's issue text listed them as if they were production; they are not. Leave them, or pass the preset explicitly for clarity |

### The real problem: the preset exists only as *text*

`options::minimap2_options(cli)` decides the preset and bakes `-x <preset>` into `aligner_options: String`. Nothing carries the *resolved choice*, so the in-process path has nothing to read. Re-parsing `-x (\S+)` out of the option string would work and is what a quick fix would reach for — **don't**: it makes the emitted text the source of truth for a typed decision, i.e. the same two-derivations-that-can-drift shape that produced this bug.

**Fix the class, not the instance** — exactly as #1079 did for `ScoreMinForm`: make `score_min_params` *return the form it selected* so the emitted option and the MAPQ consumer are one fact. Here: resolve the preset once into a typed value, render `-x` from it, and hand the same value to the in-process backend.

### Files

| File | Role |
|---|---|
| `rust/bismark/src/aligner/config.rs` | New `Mm2Preset` enum (next to `Aligner` / `ScoreMinForm`, matching #1079's placement decision). New `RunConfig` field. Build point in `resolve` (alongside `score_model`, `~:840`). `resolve_mm2_max_length` `:1277` documents design#3 |
| `rust/bismark/src/aligner/options.rs` | `minimap2_options` `:256-289` — the preset selection + the three conflict dies + the `illumina_5base` arm. Extraction target |
| `rust/bismark/src/aligner/mod.rs` | **`:968`** the fix. `build_se_inprocess_streams` `:947` |
| **`rust/bismark/src/aligner/options.rs`** (inline `mod tests`) | `minimap2_preset_selection` **`:897`**, `minimap2_preset_conflicts_die` **`:916`** — ⚠️ rev 1's Files table sent the implementer to `tests/aligner_cli.rs`, which contains neither. The one *integration*-level `-x sr` assertion is the 5-Base end-to-end test at `tests/aligner_cli.rs:6099+`, which is also A5's verification target |
| `CHANGELOG.md`, `rust/README.md` | user-visible note; the README rammap row claims the `--mm2_*` knobs work |

---

## 3. Behavior

### 3.1 Resolve the preset once, typed

```rust
/// Which minimap2/rammap preset the `--mm2_*` selectors chose. Bismark can reach exactly
/// these three (`options::minimap2_options`); `rammap::Preset` has 13 more that stay
/// unreachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mm2Preset {
    /// `-x map-ont` — the default, and `--mm2_nanopore`.
    MapOnt,
    /// `-x map-pb` — `--mm2_pacbio`.
    MapPb,
    /// `-x sr` — `--mm2_short_reads`, and `--illumina_5base` by default.
    Sr,
}
```

`options::resolve_mm2_preset(cli) -> Result<Mm2Preset>` carries the selection **and the three conflict dies**, verbatim from today's `minimap2_options` body. `minimap2_options` calls it and renders `-x {}` from `as_option_str()`. `config::resolve` calls it too (or reads it back from `build_aligner_options`) and stores it on `RunConfig`.

⚠️ **The conflict dies move with the extraction.** Their message strings are Perl-faithful (`options.rs:259-274`) and asserted by `minimap2_preset_conflicts_die`. Preserve the strings **and the check order** (short⊕nanopore, then short⊕pacbio, then pacbio⊕nanopore) — the order determines which message a triple-conflict produces.

### 3.2 The in-process site reads it

```rust
// mod.rs, inside build_se_inprocess_streams — `config` is already in scope
let preset = rammap_preset(config.mm2_preset);
… ::rammap::Aligner::from_index(mmi_str, preset) …
```

with the enum bridge feature-gated, because `rammap::Preset` only exists under `rammap-inprocess`:

```rust
#[cfg(feature = "rammap-inprocess")]
fn rammap_preset(p: Mm2Preset) -> ::rammap::Preset {
    match p {                               // exhaustive: a fourth Bismark preset must
        Mm2Preset::MapOnt => ::rammap::Preset::MapOnt,   // be mapped here, not defaulted
        Mm2Preset::MapPb => ::rammap::Preset::MapPb,
        Mm2Preset::Sr => ::rammap::Preset::Sr,
    }
}
```

`Mm2Preset` itself is **unconditionally compiled** and names no rammap type, so a default-feature build is unaffected.

### 3.3 Resulting behaviour

| invocation | before | after |
|---|---|---|
| `--rammap` (default) | `map-ont` | `map-ont` — unchanged |
| `--rammap --mm2_nanopore` | `map-ont` | `map-ont` — unchanged |
| `--rammap --mm2_short_reads` | `map-ont` ❌ | **`sr`** |
| `--rammap --mm2_pacbio` | `map-ont` ❌ | **`map-pb`** — but ⚠️ **near-inert**, see §3.5 |
| `--rammap_subprocess …` (any) | already correct | unchanged |
| `--minimap2 …` (any) | already correct | unchanged |

**The default path does not move**, which is what keeps this cheap: `--rammap` with no `--mm2_*` flag resolves to `MapOnt`, the value that is hard-coded today.

### 3.4 Edge cases

| Case | Handling |
|---|---|
| Default `--rammap` | `MapOnt` — bit-identical to today. The only paths that move are the two explicit non-default selectors |
| `--rammap --rammap_subprocess --mm2_short_reads` | Already correct; the subprocess reads `-x sr` from the option string. Untouched |
| FastA + `--rammap` | Falls back to the subprocess with a never-silent notice (existing behaviour), so it was already correct |
| Default-feature build (`rammap-inprocess` off) | `--rammap` runs the subprocess, already correct. `Mm2Preset` compiles; the bridge fn does not exist. **A default build must still compile and pass** — V5 |
| PE / `--combined_index` / `--local` with rammap | All rejected upstream; the in-process builder is SE-only |
| 5-Base | PE-only, own path, never reaches the SE in-process builder (A5) |
| A fourth Bismark preset added later | The bridge `match` is exhaustive over `Mm2Preset`, so it fails to compile until mapped — deliberately, per #1081's lesson about `_` arms silently absorbing new variants |

### 3.5 `map-pb` is near-inert against a pre-built index (rev 2 — both reviewers)

`map-pb`'s entire preset body is index-only (`api.rs:617-619`): `*is_hpc = true; *k = 19;`. It touches no `opt` field. And `build_options` discards the preset's `k`/`w`/`hpc` in favour of the loaded index's. So `from_index(mmi, MapPb)` is *almost* `from_index(mmi, MapOnt)`.

**Almost, not exactly** — and the reviewers disagreed here, resolved by measurement in A's favour. `apply_preset_str`'s final line (`api.rs:756`) is

```rust
opt.chaining.chn_pen_gap = (opt.chaining.chain_gap_scale as f64 * 0.01 * (*k as f64)) as f32;
```

which reads the **preset's** `k` — 15 for map-ont, 19 for map-pb — and writes it into `MapOptions`, so a k-*derived option* **survives** the k discard. A measured exactly that one differing field (`chn_pen_gap` 0.12 → 0.152; 19/15 = 1.267 = 0.152/0.12), with identical AS/CIGAR/MAPQ on every 150 bp cell. B read `build_options` correctly and concluded "identical", missing that the derivation happens inside `apply_preset_str`.

**So: `--rammap --mm2_pacbio` differs from `map-ont` by one chaining-gap coefficient and nothing else — no seeding change, no scoring change.** State it that way; do not claim it is provably identical (defensible only until someone hits a gap-heavy read) and do not claim alignments change.

The never-silent consequence stands: a user typing `--rammap --mm2_pacbio` still gets essentially `map-ont` behaviour and is told nothing. **Not** fixed by rejecting the flag — it is equally inert on subprocess rammap and on `--minimap2` against a pre-built `.mmi` (the rammap CLI takes `saved_k`/`saved_w`/`saved_is_hpc` from the loaded index, `rammap/src/main.rs:826-833`), so a die would be a new incompatibility. Fixed by naming the resolved preset in the run's never-silent notice (§5 step 4b).

---

## 4. Signature

```rust
// config.rs
pub enum Mm2Preset { MapOnt, MapPb, Sr }
impl Mm2Preset {
    /// The `-x` argument text. Single source of truth for the emitted option.
    pub fn as_option_str(self) -> &'static str;
}

pub struct RunConfig {
    …
    /// The `--mm2_*` selectors' answer, resolved once — **regardless of aligner**. Only
    /// minimap2/rammap read it. NB it is NOT always `MapOnt` for other aligners:
    /// `--illumina_5base --bowtie2 --five_base_index X` resolves to `Aligner::Bowtie2`
    /// with `illumina_5base = true`, hence `Sr` (A I5). Inert there, but do not document
    /// it as a dummy.
    pub mm2_preset: Mm2Preset,
}

// options.rs — carries the selection AND the three Perl-faithful conflict dies
pub fn resolve_mm2_preset(cli: &Cli) -> Result<Mm2Preset>;
```

---

## 5. Implementation outline

1. **`config.rs`** — add `Mm2Preset` + `as_option_str()` and the `RunConfig` field, doc'd per §4.
2. **`options.rs`** — extract `resolve_mm2_preset(cli)` from `minimap2_options`'s body, moving the three conflict dies **verbatim** (strings and order). `minimap2_options` becomes `format!("… -x {} -K 250K", preset.as_option_str())`. **The emitted string must not change for any input** — V1.
3. **`config.rs::resolve`** — call `resolve_mm2_preset` and store it, next to where `score_model` is built.
   - ⚠️ **Rev 2: this is a hard placement constraint, not a to-confirm, and rev 1 named a case that cannot discriminate.** `resolve_mm2_max_length` runs at `config.rs:683` and dies for any single `--mm2_*` flag when the aligner ∉ {Minimap2, Rammap}; every conflict die needs **≥2** selectors. So place the new call **after `:683` and after `:831`** (beside `score_model`, ~`:841`) and it is provably never the first error for any input. Rev 1's `--bowtie2 --mm2_short_reads` cannot discriminate — one selector never errors in the resolver, so both orderings give the same message. **The discriminating case is `--bowtie2 --mm2_short_reads --mm2_nanopore`** (B I3): today it dies with *"You cannot specify minimap2 options (--mm2_short_reads) unless you also use --minimap2…"*; a resolver placed early flips it to the short⊕nanopore text. **Add it as a test** — no file under `tests/` mentions these flags at all.
4. **`mod.rs`** — the `#[cfg(feature = "rammap-inprocess")] fn rammap_preset` bridge, and use it at `:968`.
4b. **Never-silent notice** (B I6) — add the resolved preset to the existing backend notice (`mod.rs:206-247`), which names the backend and thread count but never the preset. Plus one clause for `map-pb`'s near-inertness (§3.5). A CHANGELOG line reaches readers of CHANGELOGs; this reaches the user who actually typed `--mm2_short_reads`.
5. **Parameterize the crosscheck's preset**, not "remove every literal" — ⚠️ rev 1's step 5 was unachievable: `inprocess.rs:702`/`:754` *already* pass the preset explicitly (there is no config in a synthetic-reference test), and there is a **fourth** literal rev 1 never mentioned at `tests/aligner_rammap_inprocess_crosscheck.rs:147`. That file is also follow-up (c)'s host, so give it a `RAMMAP_PRESET` env var defaulting to `map-ont` — which both makes (c) a small delta later and stops it comparing in-process `map-ont` against a subprocess run someone launched with `-x sr`.
6. **Docs / CHANGELOG** — a short entry: `--rammap --mm2_short_reads`/`--mm2_pacbio` previously ran `map-ont` in-process and now run the selected preset; alignments change for those two invocations; the default is unaffected. Check whether `rust/README.md`'s rammap row claims the `--mm2_*` knobs work (it says "same `--mm2_*` knobs (apples-to-apples vs `--minimap2`)") — that claim only becomes true with this fix, so it needs no edit, but the Milestones line should record the fix.
7. **No version bump** — release cut.
8. **Gates** — `cargo fmt -p bismark -- --check`; **`cargo clippy --workspace --all-targets -- -D warnings`** (that is what CI runs, `rust_ci.yml:63-64`, **not** `-p bismark`); `cargo test -p bismark`; **and `cargo test -p bismark --features rammap-inprocess`**, a separate CI job and the only one that compiles the changed line (#1081's lesson — a green default-feature run means nothing here).
   - ⚠️ **Feature-gating placement is a live trip hazard both directions** (B I8): any new test naming `::rammap::Preset` needs a per-test `#[cfg(feature = "rammap-inprocess")]` (the `inprocess.rs:665,689` pattern) or a per-file `#![cfg(...)]`, because `mod.rs`'s `#[cfg(test)]` module is **not** feature-gated as a whole. Omit it and the *default* jobs fail to compile.

---

## 6. Efficiency

Nil. One enum resolved once per run at config time; the bridge is a three-arm `match` executed once per index load (2 per run). `Mm2Preset` is a `Copy` unit-only enum.

---

## 7. Integration

- **Reads:** `cli.mm2_short_read` / `mm2_pacbio` / `mm2_nanopore` / `illumina_5base` — all already read by `minimap2_options`.
- **Writes:** the `-x` text in `aligner_options` (must be unchanged) and the in-process aligner's preset (the fix).
- **Downstream — `--mm2_short_reads` only** (⚠️ rev-2 correction; rev 1 said "those two invocations" and claimed a `k`/`w` change, both wrong):
  - **`sr` changes alignments**, via scoring (`b=8, q=12` vs `4, 4`) **and** `min_chain_score=25`, `min_dp_max=40`, `pri_ratio=0.5`, `zdrop=100`, `end_bonus=10` and four flags. So **mapping *rate* moves, not just `AS`** — the spike's own table shows mapped→UNMAPPED flips at 60 and 100 bp, i.e. typical trimmed WGBS lengths. That shifts the SE report percentages, `--unmapped` contents and the unique/ambiguous split, then `AS` → MAPQ (`AS`-sensitive since #1081) → methylation calls (B I7).
  - **`map-pb` is near-inert** (§3.5). Do not promise it changes anything.
  - **No `k`/`w` change for any preset**: `from_index` → `build_options(preset, index.kmer_size, index.window_size)` treats the preset's `k`/`w`/`hpc` as locals and discards them; the loaded `.mmi` keeps its own (built at `-k 20`, `genome_prep/indexer.rs:118-124`).
  - Agreement with `--minimap2 --mm2_short_reads` therefore improves on **scoring** but not on **seeding** — in-process `sr` is a hybrid, `sr` options over a `k=20` index (B's A4 refinement).
- **Report bytes do NOT change.** V1 freezes the emitted option string, and the SE report has been echoing `-x sr` all along — i.e. the report was already asserting a preset the run did not use. The report simply stops lying (B I6).
- **CI:** the `rammap-inprocess` job is the only one that compiles `mod.rs:968`'s change.

---

## 8. Assumptions

| # | Assumption | Status |
|---|---|---|
| A1 | `rammap::Preset` has all three Bismark-reachable presets | ✅ Verified: `MapOnt`, `MapPb`, `Sr` at `api.rs:39-56` |
| A2 | `mod.rs:968` is the only production site | ✅ Verified: the other two `Preset::` uses are inside `#[cfg(test)]`. **Corrects #1092's own issue text**, which implied three production sites |
| A3 | The preset is not recoverable from `RunConfig` today | ✅ It exists only inside the `aligner_options` String |
| **A4** | **rammap in-process `sr` behaviour is UNMEASURED** | 🟠 **The honest risk.** rammap's concordance gate vs minimap2, and the in-process-vs-subprocess crosscheck (≤0.022 %/cell), were established on **`map-ont`**. Turning `sr` on in-process makes a previously-unreachable code path reachable, with no concordance figure attached. The fix is still right — running the preset the user asked for beats running a different one — but the release note must not imply `sr` is gated |
| A5 | 5-Base never reaches the SE in-process builder | ⚠️ Verify at implementation: 5-Base is PE-only (`mod.rs:591` `unreachable!()` for SE) and runs `run_pe_five_base`, but its `sr` default comes from the same `minimap2_options` arm, so the extracted resolver must reproduce that arm exactly |
| A6 | The conflict dies' messages and order are behaviour | ❌ **FALSE as stated in rev 1** — the messages and order are behaviour, but **nothing asserts them**: `minimap2_preset_conflicts_die` checks `.is_err()` only, over pairs only. Rev 2 adds the assertions, and lands them *before* the extraction |
| A7 | `--mm2_maximum_length` and `-p` are already honoured in-process | ✅ Verified (§1) — convert-stage and `inprocess_rammap_threads` respectively |

---

## 9. Validation

| # | Verify | How | Expected |
|---|---|---|---|
| **V1** | **The emitted option string is unchanged for every input** — the extraction is inert | `minimap2_preset_selection` + `minimap2_default_options` + the `--illumina_5base` and `-p` cases pass **untouched**; assert `-a --MD --secondary=no -t 2 -x {map-ont,map-pb,sr} -K 250K` for all three selectors | Byte-identical strings. Any edit to an expected string means the extraction changed behaviour |
| **V2** | The conflict dies are unmoved | ⚠️ **Rev 2 (both reviewers): the existing test has NO teeth** — `minimap2_preset_conflicts_die` (`options.rs:916-929`) asserts `.is_err()` only, never a message, and only over *pairs*, so reordering is invisible by construction (every order errors on every pair). **Land exact per-pair message assertions PLUS the triple case `[--mm2_short_reads, --mm2_pacbio, --mm2_nanopore]` (which must yield the short⊕nanopore text) BEFORE the extraction** — a post-extraction baseline pins whatever the extraction produced, i.e. the very fault being guarded (A's sequencing point) | The three exact messages + the triple. Only then does V8(iii) mean anything |
| **V3** | `resolve_mm2_preset` maps every selector | Unit: default→`MapOnt`, `--mm2_nanopore`→`MapOnt`, `--mm2_pacbio`→`MapPb`, `--mm2_short_reads`→`Sr`, `--illumina_5base`→`Sr`, and explicit-preset-beats-5-Base | As tabulated |
| **V4** | The bridge is exhaustive and correct | Feature-gated unit: all three `Mm2Preset` → the matching `rammap::Preset` | As tabulated |
| **V5** | A **default-feature** build is unaffected | `cargo test -p bismark` green; `Mm2Preset` compiles with no rammap dep | Green |
| **V6** | **The feature build compiles and passes** | `cargo test -p bismark --features rammap-inprocess` | Green. The only job that compiles the fix |
| **V7** | **The wiring, end-to-end through the PRODUCTION constructor** | ⚠️ **Rev 2 — rev 1's version could not fail injection (i)** (both reviewers). Feature-gated hermetic test: (1) `from_seqs(...).save_index(tmp/BS_CT.mmi)` writes a **real** `.mmi` (`api.rs:425`); same for `BS_GA.mmi`, since directional SE loads both (`mod.rs:973-985`); (2) hand-build a `RunConfig` pointing `genome.{ct,ga}_index_basename` at them — **not** via `resolve`, which execs `<aligner> --version` and the feature CI job installs minimap2, not rammap (B I1); (3) call **`build_se_inprocess_streams(&config, &converted)`** — the real function, real `load` closure, real `from_index`; (4) drain the stream and assert `SamRecord.alignment_score` on a 150 bp / 3-mismatch read | **Run and pin** the two scores (the gate's index is `k=15/w=10` from `from_seqs`, production's is `-k 20`, so do **not** copy the spike's numbers). Comment the full derivation. This covers `:968` + `from_index` + the new `RunConfig` field in one assertion, and dissolves `SPIKE.md` §7's fourth limitation. ⚠️ Still **`Sr` only** — `MapPb` is near-inert (§3.5), which *shrinks* rather than raises that gap |
| **V7b** | The single-sourcing that is this plan's whole purpose (B I2) | For each of the 5 clis (default, `--mm2_nanopore`, `--mm2_pacbio`, `--mm2_short_reads`, `--illumina_5base`): `assert!(minimap2_options(&cli)?.contains(&format!("-x {}", resolve_mm2_preset(&cli)?.as_option_str())))` | Holds. **Without this, an implementation that adds the resolver and leaves `minimap2_options`' own `if` chain in place passes every other gate while re-creating the drift the plan exists to remove** |
| **V7c** | `sr` does not silently drop reads via a clipped CIGAR (B I7) | Under `Sr`, assert `consumed_read_len(cigar) == read_len` for the gate read. `inprocess.rs:132-157` counts `M I S = X` and not `D N H P`; a CIGAR failing this is **silently skipped** by the methylation length guard | Equal. `sr`'s local-extension behaviour is a newly reachable source of clipped CIGARs, and this is the one silent-wrong-output mode the AS assertion misses |
| **V7d** | rammap honours #1081's `AS <= 2·len` bound under `sr` (B I7) | A perfect 150 bp read under `Sr` scores exactly **300** | 300. #1081's live bound gate (`tests/aligner_minimap2_as_bound.rs`) runs **minimap2 only**; nothing covers rammap, whose `sr` sets `end_bonus = 10`. This fix is what makes that path reachable |
| **V8** | Teeth | Injections, each alone: **(i)** revert `:968` to the `Preset::MapOnt` literal → **V7 must fail**; **(ii)** map `Sr → Preset::MapOnt` in the bridge → V4 **and** V7 must fail; **(iii)** reorder the conflict dies → **V2 must fail (only after V2 gains message assertions — it cannot today)**; **(iv)** change `as_option_str` for one variant → V1 must fail; **(v)** stub the `RunConfig` field to `MapOnt` while still calling the resolver → **V7 must fail** (B I1); **(vi)** keep `minimap2_options`' own `if` chain alongside the resolver → **V7b must fail** | All six fail. (i) and (v) are the ones that matter: rev 1's gate set caught neither |

### 9a. The wiring gate — RESOLVED by spike (b), which came back positive

#1081 could prove its wiring end-to-end because MAPQ lands in a BAM column a fake aligner can drive. Here the only observable is alignment output, and `from_index` needs a real `.mmi` while `make_genome_mmi` writes a 1-byte placeholder — so the fault *"the site still hard-codes `MapOnt`"* looked invisible to every hermetic test, which is the #1079 silent-no-op shape this lineage keeps hitting.

**Spike (b) resolved it: a preset difference IS observable through `from_seqs`, which needs no index file.** `SPIKE.md` has the full 20-cell table; the operative result:

| cell | `MapOnt` | `MapPb` | `Sr` |
|---|---|---|---|
| 150 bp, 3 mismatches | AS **282**, `150M`, mapq 60 | AS 282 | AS **270**, `150M`, mapq 60 |

A 12-point score gap from `3 × (8 − 4)`, the mismatch-penalty delta — same CIGAR, same MAPQ, both mapping comfortably, deterministic across repeat runs. So **V7 is a real behavioural gate**, and injection (i) is genuinely caught.

Three spike findings constrain how it must be written:

1. **Assert the score, not a mapped/unmapped flip or a MAPQ.** The two intuitive discriminators are the fragile ones: `sr`'s `min_chain_score = 25` / `min_dp_max = 40` produce real mapped-vs-UNMAPPED differences at 60–100 bp, but *at* those thresholds by construction, so a rammap bump flips them and reddens CI for an unrelated reason. `sr`'s `k = 21` vs `map-ont`'s `k = 15` is unusable outright — 16–22 bp reads were UNMAPPED under **all three** presets.
2. **Use a ≥ 150 bp read.** Everything at 30 bp was UNMAPPED under every preset; the three clean cells all live at 150 bp.
3. ⚠️ **The gate covers `Sr` only.** `map-pb` sets `k = 19` + HPC — **index** options, no scoring change — so it scored *identically to* `MapOnt` on every cell where both mapped. `MapPb`'s arm rests on V3 + V4 plus option (a)'s structural guarantee, and V7's description must say so rather than letting green read as "all three wired".

**Keep option (a) as well**, since it is free and prevents *re*-introduction: have the load closure take the preset as a parameter so a literal cannot reappear at `:968` without an obvious edit.

**⚠️ Rev 2 — rev 1's prescription here was wrong, and both reviewers caught it.** Rev 1 said: extract `inprocess_preset(config) -> ::rammap::Preset`, have `build_se_inprocess_streams` call it, and have V7 drive *that function* from a `RunConfig`. Trace injection (i) against that: revert `:968` to the literal, and `inprocess_preset` still exists, still returns `Sr`, V7 still gets its expected score, `dead_code` never fires — **green, with the fix a complete no-op.** The extraction *narrows* the fault (from "`:968` names a literal" to "`:968` calls the seam and ignores the result") without closing it. The seam that matters is the `from_index` **call**, not the preset **computation**, and rev 1 diagnosed exactly that one paragraph earlier before prescribing against it.

**And the blocker that forced the compromise does not exist.** §2 and rev 1 asserted "`from_index` needs a real `.mmi` while `make_genome_mmi` writes a 1-byte placeholder". True of the *fixture* — but `rammap::Aligner::save_index` (`api.rs:425` → `Index::save`, RMMI magic + bincode of the whole index including `seqs`) writes one, and `from_index` reads it back by magic sniffing (`align/index.rs:332,347`). Reviewer A built a 107 KB `.mmi` from a synthetic reference in a scratch crate and reproduced the spike table through the production constructor **exactly** — which also settles `SPIKE.md` §7's fourth limitation by making the assumption unnecessary.

So V7 is the end-to-end gate rev 1 wanted: real `.mmi` → hand-built `RunConfig` → `build_se_inprocess_streams` → real `from_index` → assert the emitted `alignment_score`. Injection (i) then flips the score at exactly `mod.rs:968`. **Keep option (a) too** — pass the preset into the `load` closure as a parameter so a literal cannot reappear.

**(c) remains a follow-up** and is the only thing that would close A4.

---

## 10. Questions and ambiguities

| Priority | Item |
|---|---|
| **Resolved — D-GATE (Felix, 2026-08-02)** | (a) structural + spike (b), (c) as a follow-up. **Spike (b) succeeded** (`SPIKE.md`), so V7 is a real behavioural gate on a 150 bp / 3-mismatch cell (282 vs 270) rather than structural-only — with the three constraints in §9a, including that it covers `Sr` and not `MapPb` |
| **Resolved — D-A4 (Felix, 2026-08-02)** | In-process `sr` ships **unmeasured**; deferring (c) is what defers the measurement. Justified because the alternative is knowingly running a preset the user did not ask for. **The release note must state that in-process `sr` is newly reachable and its concordance ungated** |
| **Open (follow-up, not blocking)** | Option (c) — extend the env-gated crosscheck (`RAMMAP_MMI`) to run per-preset and compare in-process vs subprocess. Closes A4. Worth its own issue once this merges |
| **Noted** | Whether to fix the two `#[cfg(test)]` literals (step 5). Cosmetic; makes a future grep for `Preset::MapOnt` meaningful |
| **Resolved** | Scope is the preset selector alone — `--mm2_maximum_length` and `-p` were checked and are already honoured (§1) |
| **Resolved** | Don't re-parse `-x` out of the option string; single-source the typed value (§2) |
| **Resolved** | #1092's issue text listed three production sites; there is **one** (A2) |

---

## 11. Self-Review

**Efficiency** — nil; one enum, resolved once.

**Logic** — traced the one production site and confirmed `config` is in scope, so no plumbing is needed beyond the `RunConfig` field. Confirmed the default path resolves to the value currently hard-coded, so `--rammap` with no `--mm2_*` flag is bit-identical and the change is confined to two explicit invocations. Confirmed the extraction must be inert on the emitted string (V1) — that is the one way this "small" change could break the subprocess and minimap2 paths, which are *not* in scope to move.

**Edge cases** — default-feature build (bridge absent, `Mm2Preset` still compiles), FastA fallback, PE/combined/local rejects, 5-Base's `sr` arm, and a future fourth preset (exhaustive `match`, no `_`).

**Integration** — the changed alignments propagate through `AS` → MAPQ (now `AS`-sensitive post-#1081) → methylation calls, for the two affected invocations only.

**Remaining risks (rev 2)**
1. **A4 / D-A4 — in-process `sr` is unmeasured, and it is a *hybrid***: `sr` mapping options over an index built at `-k 20`, because `from_index` discards the preset's `k`/`w` (§3.5's mechanism). So agreement with `--minimap2 --mm2_short_reads` improves on scoring but **not** seeding — minimap2 given a FASTA builds at the preset's own `k`. That is the figure follow-up (c) will produce. Top residual, and B's refinement of rev 1's A4.
2. **Mapping rate moves, so read counts change** (§7). The most user-visible consequence, and unmeasured for the same reason as (1).
3. **`MapPb` has no behavioural gate** — but §3.5 *shrinks* this: a mis-mapped `MapPb` arm has essentially no observable effect on the production path (one chaining coefficient), so V3/V4's unit coverage is proportionate.
4. **The gate rests on rammap `5ea62cd`'s `sr` parameters.** A dep bump that moves `mismatch_penalty` fails V7 with a wrong-number diff — the correct outcome, but comment the **full** arithmetic (`2·150 − 3·(2+8)`), including that `end_bonus = 10` does *not* appear in this cell, so the next bumper can tell which parameter moved (A O12).
5. ~~`set_simd_cap` is process-global~~ — **CLOSED by Reviewer B's upstream verification** (code review). The cap is documented as an AVX-512 *license-throttling* measure (`rammap-core align/dp/mod.rs:22-29`) and feeds only `use_avx512()`/`use_avx2()` kernel **selection**; rammap's own tests assert SSE-vs-AVX-512 equality of `score`, `max` and CIGAR-consumed length, plus scalar-vs-SIMD equality. So the exact-score gates (282/270/300) cannot flake on kernel choice — the exposure is **timing only**, not correctness. The plan's offered mitigation (serialize the two `Sr`-constructing tests) is therefore unnecessary; the coverage audit flagged it as unapplied without B's evidence to hand. Residual: those scores were measured on aarch64 and the only job compiling them is x86_64, so a first-run-on-x86 surprise would be a real (not flaky) signal.
3. **The extraction touching Perl-faithful error strings** (A6/V2). Mechanical, but it is stderr behaviour with tests, so "just moving code" is not free.
4. **Users on `--rammap --mm2_short_reads` today get different output.** They were getting output from a preset they did not ask for, so this is a correction — but it is still a change, and the CHANGELOG must say which two invocations move.

---

## 12. Implementation Notes (2026-08-02)

**Branch:** to be cut from `dev` `11efbab`. **Status:** implemented, not committed. `cargo fmt -p bismark -- --check` clean · **`cargo clippy --workspace --all-targets -- -D warnings` 0 warnings** (the CI invocation, not `-p bismark`) · default suite **2123 pass / 0 fail** (was 2120) · **`--features rammap-inprocess` suite 2133 pass / 0 fail**, with all three feature-gated gates confirmed *running* rather than skipped.

### The sequencing mattered

Per A's point, the **exact conflict-die message assertions were landed against pre-extraction code and verified passing there first**, so they are a true baseline rather than a snapshot of whatever the extraction produced. Then the extraction: all 32 pre-existing `options` tests passed **untouched**, which is V1's inertness property — the emitted option string is byte-identical for every input.

### The gate is real this time

`inprocess_rammap_honours_the_resolved_preset` writes a **real `.mmi`** with `rammap::Aligner::save_index`, hand-builds a `RunConfig` via a new `#[cfg(test)] run_config_stub`, and calls **`build_se_inprocess_streams`** — the production function, the real `load` closure, the real `from_index`. It asserts `map-ont` = **282** and `sr` = **270** on a 150 bp / 3-mismatch read.

**Both numbers came out exactly as the parameter arithmetic predicted** (`2·150 − 3·(2+4)` and `2·150 − 3·(2+8)`) even though the gate's index is `save_index`-written rather than the spike's `from_seqs` one — so they were derived, not baselined, and the plan's caution about re-running rather than copying turned out to be satisfied by derivation anyway.

### Fault injections — 6 run, and one instructive result

| Injection | Detected by | Result |
|---|---|---|
| **(i) revert the call site to the `Preset::MapOnt` literal** | **the wiring gate** — `left: 282, right: 270`, message naming the cause | **FAILED** ✅ |
| (ii) bridge maps `Sr → MapOnt` | bridge test **and** the wiring gate | FAILED |
| (iii) reorder the conflict dies | `minimap2_preset_conflicts_die` | FAILED — **only because** the triple-conflict case was added; over pairs it could not |
| (iv) `as_option_str` changed for one variant | the option-string tests | FAILED |
| **(v) stub the `RunConfig` field while still calling the resolver** | ⚠️ **no test** — caught by `unused variable: mm2_preset` under `clippy -D warnings` | passes tests, **fails CI** |
| (vi) `minimap2_options` re-derives its own preset | `emitted_preset_is_the_resolved_preset` | FAILED |

**(i) is the headline**: it is the fault both plan reviewers said rev 1's gate could not see, and it now fails loudly. **(v) is the honest caveat** — B predicted it exactly, and it is real: the gate builds its `RunConfig` directly (it must; `resolve` execs `<aligner> --version` and the feature CI job installs minimap2, not rammap), so no test observes `resolve`'s field assignment. The compiler does, because the binding would become unused and CI runs `-D warnings`. That is a legitimate gate but a *different kind*, and it is documented at the call site rather than claimed as test coverage.

### Everything else applied

- **Never-silent notice** — the backend notice now names the resolved preset, plus a dedicated line for `--mm2_pacbio`'s near-inertness. `Mm2Preset` had to be imported un-feature-gated for this (the notice runs on both builds).
- **Crosscheck parameterized** — `RAMMAP_PRESET` env var (default `map-ont`) replaces the hard-coded literal at `aligner_rammap_inprocess_crosscheck.rs:147`, so it can no longer compare in-process `map-ont` against a subprocess run launched with `-x sr`, and follow-up (c) is a small delta.
- **Placement pinned by test** — `mm2_flags_on_bowtie2_report_the_wrong_aligner_not_a_preset_conflict` uses B's discriminating case (`--bowtie2 --mm2_short_reads --mm2_nanopore`), which rev 1's single-selector case could not have caught.
- **Docs + CHANGELOG** — `sr` changes *which reads map*, not just their scores; `map-pb` is near-inert; in-process `sr` is unmeasured **and a hybrid** (sr options over a genome-prep-`k` index).

### Deviations

1. **`Mm2Preset` stayed non-`Option`** (A's I5 offered either). The doc comment now states the truth — including that `--illumina_5base --bowtie2` yields `Sr` for a Bowtie 2 run — rather than the false "MapOnt otherwise". `Option` would have touched every `RunConfig` construction for a field only one backend reads.
2. **`run_config_stub` is a new `#[cfg(test)]` helper in `config.rs`** — 37 fields written once, no `..Default::default()`, so a new `RunConfig` field breaks it loudly rather than being silently absorbed.
3. **A5 verified rather than assumed** — the resolver's `illumina_5base → Sr` arm is covered by `resolve_mm2_preset_maps_every_selector`, including that an explicit preset beats it.

### Not done

- **No commit or PR** — awaiting review.
- **Follow-up (c)** — the per-preset real-index concordance run that would close A4. `RAMMAP_PRESET` is the hook.
- The `set_simd_cap` process-global question (risk 5): the new `Sr` tests are the first to flip it. No cross-test interference observed, but AVX2-vs-AVX-512 kernel score-equivalence was **not** separately confirmed.

## 13. Revision History

**rev 3 (2026-08-02)** — implemented; see §12. The two blocking Criticals are closed and *proven* closed: injection (i) — reverting the call site to the hard-coded literal — now fails the wiring gate at `left: 282, right: 270`, and the conflict-die messages are pinned by assertions landed against pre-extraction code. One honest residual came out of the injection run: fault (v) (stubbing the `RunConfig` field) is caught by the **compiler** (`unused variable` under CI's `-D warnings`), not by any test, exactly as Reviewer B predicted — documented at the call site rather than claimed as test coverage.

**rev 2 (2026-08-02)** — dual plan-review folded. Both reviewers: **not ready as written**, with the design approved and the *validation* blocked. Four findings reached independently by both:

| Finding | A | B | Resolution |
|---|---|---|---|
| **V7 as specified cannot fail injection (i)** — the extracted `inprocess_preset` is *upstream* of the call site, so reverting `:968` leaves the gate green and the fix a no-op | C1 | C1 | **Both correct**, against rev 1's own §9a diagnosis. V7 rebuilt end-to-end |
| **`save_index` exists**, so `from_index` is hermetically testable — the premise the whole §9a debate rested on is false | C1 | C2 | Both correct; A *measured* it (107 KB `.mmi`, spike table reproduced through the production constructor). Dissolves `SPIKE.md` §7's fourth limitation |
| **`minimap2_preset_conflicts_die` asserts `.is_err()` only** — no message, pairs only, so reordering is invisible; A6/V2/V8(iii) were false | C2 | C4 | Both correct. Assertions added **before** the extraction (A's sequencing point) |
| **`MapPb` is near-inert on `from_index`**, and rev 1's "different `k`/`w`" was wrong for *every* preset | I3 | C3 | Both correct → new §3.5 |

**Contradiction, resolved in A's favour by measurement.** A found `MapOnt` vs `MapPb` differ by exactly one option (`chn_pen_gap` 0.12 → 0.152); B concluded "identical". Verified: `apply_preset_str`'s last line (`api.rs:756`) derives `chn_pen_gap` from the **preset's** `k` before `build_options` discards `k`, so a k-derived *option* survives (19/15 = 1.267 = 0.152/0.12). B read `build_options` correctly and missed the derivation inside `apply_preset_str`. So `map-pb` is **near**-inert, not provably identical — a weaker claim, and the honest one.

**Unique to B, both high-value:** nothing gated the **single-sourcing that is this plan's stated purpose** (an implementation that adds the resolver and keeps `minimap2_options`' `if` chain passes everything) → **V7b**; and **mapping *rate* changes, not just `AS`** — the spike's own mapped→UNMAPPED flips at 60/100 bp are typical trimmed WGBS lengths → §7 + risk 2, plus **V7c** (`consumed_read_len`, the silent-skip mode) and **V7d** (rammap's `AS ≤ 2·len`, which #1081's minimap2-only gate does not cover). Also: the `RunConfig` field assignment was ungated and `resolve` cannot be driven in the feature job (it execs `<aligner> --version`; that job installs minimap2, not rammap) → V7's hand-built `RunConfig` + injection (v); the run's notice never names the preset while the report has been echoing `-x sr` all along → step 4b; CI runs `clippy --workspace`, not `-p bismark` → step 8.

**Unique to A:** the die-order question is decidable now and became a hard placement constraint; `--illumina_5base --bowtie2` makes the resolver return `Sr` for a Bowtie 2 run, so §4's doc comment was false; the process-global SIMD cap (risk 5); the spike's determinism check measured `perfect_60`, not the gate cell; a fourth `Preset::MapOnt` literal; and — usefully — A priced the obvious simplification (returning the preset through `build_aligner_options`' tuple) and found it **worse**, since ~40 test sites destructure `(opts, _)` and would break V1's inertness signal. The shape is validated; the document was over-long.

**rev 1 (2026-08-02)** — D-GATE and D-A4 locked; spike (b) run and folded.

Felix chose **(a) structural + spike (b), (c) as a follow-up**, which also settles D-A4 (deferring (c) defers the measurement, so in-process `sr` ships unmeasured and labelled).

**Spike (b) succeeded**, so §9a flipped from "the plan's weakest point" to a real behavioural gate: a 150 bp / 3-mismatch read scores **282** under `MapOnt` and **270** under `Sr` — same CIGAR, same rammap MAPQ, deterministic, and 12 points clear of any threshold. Three constraints came out of it and are now written into V7 and §9a:

- assert the **score**, not a mapped/unmapped flip or a MAPQ — the two intuitive discriminators (`sr`'s `min_chain_score`/`min_dp_max`, and `k=21` vs `k=15`) are threshold-adjacent or dead;
- use a **≥150 bp** read — 30 bp is below the seeding floor for every preset;
- the gate covers **`Sr` only**. `map-pb` sets index options with no scoring change, so it scored identically to `MapOnt` wherever both mapped. Recorded at V7 and demoted to risk 1 so a green V7 is not misread as "all three presets wired".

Also surfaced by the spike: V7 must drive a production seam (`inprocess_preset(config)`), not build its own `from_seqs` aligner — otherwise it gates the bridge instead of the call site, which is the fault that matters.

**rev 0 (2026-08-02)** — initial plan. Triage established: one production site (not three as #1092 stated), all three presets available in `rammap::Preset`, and the other two `--mm2_*` knobs already honoured. Design follows #1079's `ScoreMinForm` precedent — single-source the resolved value rather than re-deriving it from emitted text.

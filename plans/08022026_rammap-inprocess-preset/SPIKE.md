# SPIKE — is a `rammap::Preset` difference observable through `from_seqs`?

**Issue:** [#1092](https://github.com/FelixKrueger/Bismark/issues/1092) · **Plan:** `PLAN.md` §9a option (b)
**Date:** 2026-08-02 · **Base:** `dev` `11efbab`
**Verdict:** **YES for `Sr` — a hermetic behavioural gate is buildable. NO for `MapPb`**, which changes only seeding and cannot be pinned robustly by score.

---

## 1. Question, criteria, strategy

**Question.** `PLAN.md` §9a's central problem: the fault *"`mod.rs:968` still hard-codes `Preset::MapOnt`"* is invisible to every hermetic test, because `from_index` needs a real `.mmi` and the test fixture writes a 1-byte placeholder. Option (b) asks whether `rammap::Aligner::from_seqs` — which needs no index file, and which the existing feature-gated tests already use — exposes a preset difference a test can assert on.

**Success criteria.** At least one `(reference, read)` cell where `MapOnt` and `Sr` give **different, deterministic, non-threshold-adjacent** observable output. "Non-threshold-adjacent" is the load-bearing qualifier: a cell that flips on a rammap version bump is a flaky gate, which is worse than none.

**Strategy.** Drive the real `map_seq_with` on a fixed-LCG synthetic reference across all three Bismark-reachable presets, over reads designed to separate each parameter that the presets actually differ on:

| | `MapOnt` | `MapPb` | `Sr` |
|---|---|---|---|
| `k` / `w` | 15 / 10 | 19 (+HPC) | 21 / 11 |
| mismatch penalty | 4 | 4 | **8** |
| gap open | 4 | 4 | **12** |
| `min_chain_score` / `min_dp_max` | default | default | **25 / 40** |

(`api.rs:612-700`; `map-pb` sets **index** options only — no scoring change. That asymmetry turns out to be the finding.)

**Out of scope.** Whether the fix is right (settled), and real-index concordance (option (c), a follow-up).

---

## 2. Script

- Throwaway: `rust/bismark/tests/spike_1092_preset_observable.rs`, run then **deleted**. Preserved for the record at `spikes/spike_preset_observable.rs`; output at `spikes/run.log`.
- `cargo test -p bismark --features rammap-inprocess --test spike_1092_preset_observable -- --nocapture`

---

## 3. Results — 9 of 20 cells discriminate `MapOnt` vs `Sr`

| case | MapOnt | MapPb | Sr | verdict |
|---|---|---|---|---|
| `perfect_60` | AS=120 `60M` mapq=18 | AS=120 mapq=17 | AS=120 mapq=**49** | differs, **MAPQ only** |
| `mm1_60` | AS=114 `60M` mapq=7 | UNMAPPED | **UNMAPPED** | differs, threshold |
| `del2_60` | AS=108 `29M2D29M` | UNMAPPED | AS=**100** | differs |
| `mm1_100` | AS=194 `100M` | AS=194 mapq=36 | AS=**190** | differs |
| `mm3_100` | AS=182 mapq=34 | UNMAPPED | **UNMAPPED** | differs, threshold |
| `del2_100` | AS=188 `50M2D48M` | AS=188 | AS=**180** | differs |
| **`mm1_150`** | **AS=294** `150M` mapq=60 | AS=294 | **AS=290** `150M` mapq=60 | ✅ **clean** |
| **`mm3_150`** | **AS=282** `150M` mapq=60 | AS=282 | **AS=270** `150M` mapq=60 | ✅ **cleanest** |
| **`del2_150`** | **AS=288** `75M2D73M` | AS=288 | **AS=280** | ✅ clean |

Non-discriminating: everything at 30 bp (all UNMAPPED — below every preset's seeding floor), the `short_16..22` set (all UNMAPPED, so `sr`'s `k=21` is **not** usable as a discriminator — `map-ont`'s `k=15` did not seed a 16–22 bp read either), and `perfect_100`/`perfect_150` (a perfect alignment scores `2·len` under every preset, since they share `match_score = 2`).

**Determinism:** the same cell re-run 3× under `Sr` gave identical `(score, cigar, mapq)` every time.

---

## 4. Findings

### F1 — `Sr` is robustly distinguishable **by score**, and the margin is wide

`mm3_150`: `MapOnt` **282** vs `Sr` **270**. Both produce the *same* `150M` CIGAR and the *same* rammap MAPQ 60 — only the score differs, by exactly `3 × (8 − 4) = 12`, the mismatch-penalty delta. Nothing here sits near a threshold: both presets map the read comfortably, and the difference is pure arithmetic on a parameter the presets explicitly set.

`mm1_150` (4-point delta) and `del2_150` (8-point, from `gap_open` 4 vs 12) are equally clean. **`mm3_150` is the recommended gate cell** — widest margin, simplest assertion.

### F2 — `MapPb` cannot be pinned this way, and that limits what the gate can prove

`map-pb` sets `k = 19` and HPC — **index** options only, no scoring change (`api.rs:617-619`). So on every cell where both mapped, `MapPb` scored *identically to* `MapOnt` (282 = 282, 294 = 294, 188 = 188). Where it did differ it was either UNMAPPED (a seeding threshold) or a MAPQ-only difference — both version-fragile.

**Consequence for the plan:** a hermetic gate can prove the resolved preset reaches `from_index` **for `Sr`**. It cannot do so for `MapPb`; that arm rests on the resolver/bridge unit tests plus option (a)'s structural guarantee. This is worth stating plainly rather than implying V7 covers all three.

### F3 — the two intuitive discriminators are the fragile ones

Both ideas I would have reached for first are bad gates:

- **`sr`'s `k=21` vs `map-ont`'s `k=15` on a short read** — unusable. Reads of 16/18/20/22 bp were UNMAPPED under *all three* presets, so the seeding floor for this synthetic reference is above `k` for other reasons (`min_cnt`, chain scoring). No signal.
- **mapped-vs-UNMAPPED cells** (`mm1_60`, `mm3_100`) — real differences, driven by `sr`'s `min_chain_score = 25` / `min_dp_max = 40`, but they sit *at* those thresholds by construction. A rammap version that nudges either constant flips the cell and reddens CI for an unrelated reason.

A score-difference cell where both presets map comfortably has neither problem.

### F4 — 30 bp is below the floor for this reference

Everything at 30 bp was UNMAPPED under all presets. So the gate cell must be **≥ 60 bp**, and 150 bp is where all three of the clean cells live.

---

## 5. Reference snippets for the implementation

The gate, in the shape the existing feature-gated tests already use (`inprocess.rs:687+`):

```rust
let aligner = rammap::Aligner::from_seqs(
    vec![("chr1_CT_converted".to_string(), reference.clone())],
    preset,                       // <-- the value under test
);
let res = aligner.map_seq_with(qname, read, rammap::MapOpts { cs: None, md: Some(true) });
let primary = res.mappings.iter().find(|m| m.is_primary && !m.is_supplementary);
assert_eq!(primary.unwrap().score, expected);   // 282 for MapOnt, 270 for Sr
```

Read construction for `mm3_150`: take `reference[5000..5150]`, then flip the bases at offsets `len/4`, `len/2`, `3*len/4` (`A<->C`, `G<->T`).

---

## 6. Recommendation

**Build the option-(b) gate on `mm3_150`, and keep option (a).** Concretely, `PLAN.md` V7 becomes a real behavioural assertion: a feature-gated hermetic test that runs the in-process seam under the preset the config resolved and asserts `score == 270` for `Sr` / `282` for `MapOnt`. Injection (i) — reverting `mod.rs:968` to the `Preset::MapOnt` literal — then fails it, which is the whole point of §9a.

Two honesty constraints to carry into the plan:

1. **The gate covers `Sr`, not `MapPb`** (F2). Say so where V7 is described; do not let a green V7 read as "all three presets are wired".
2. **Assert a score, not a mapped/unmapped flip or a MAPQ** (F3), and use a ≥150 bp read (F4).

One structural point: the gate needs the preset to reach the aligner construction through the same path production uses. If the test builds its own `from_seqs` aligner it proves the *bridge*, not the *wiring*. To gate the wiring the test must go through whatever function production calls — which is an argument for extracting a small `inprocess_preset(config) -> rammap::Preset` and having `build_se_inprocess_streams` call it, so a test can drive that function from a `RunConfig` and observe the resolved preset end-to-end.

---

## 7. Limitations

- **One reference, one seed.** A fixed-LCG 20 kb synthetic sequence. The AS deltas are pure parameter arithmetic so they should not be reference-sensitive, but the mapped/unmapped cells clearly are, which is exactly why the recommendation avoids them.
- **rammap `5ea62cd` only** (the pinned rev). A future bump could change `sr`'s penalties; the gate would then fail loudly with a wrong-number diff, which is the correct outcome rather than a silent pass.
- **Says nothing about concordance** — whether in-process `sr` agrees with subprocess `sr` or with minimap2 `sr` is option (c), deliberately deferred. This spike only establishes that a *wiring* gate is buildable.
- **`from_seqs`, not `from_index`.** Production uses `from_index`. The spike assumes the preset is honoured identically by both constructors; both take `preset` as their second parameter and hand it to the same option builder, but this was not separately verified.

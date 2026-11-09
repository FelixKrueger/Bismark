# PROGRESS — in-process rammap preset selector (#1092)

**Plan:** [`PLAN.md`](./PLAN.md) · **Issue:** [#1092](https://github.com/FelixKrueger/Bismark/issues/1092) (found while implementing [#1081](https://github.com/FelixKrueger/Bismark/issues/1081))
**Last updated:** 2026-08-02 · **Base:** `dev` `11efbab`

**Status legend:** 📋 Planned · 🔨 In progress · ✅ Done · ⛔ Blocked · ⏸ Awaiting user

| # | Pipeline step | Status | Notes |
|---|---|---|---|
| 1 | Triage / verify the issue | ✅ Done | Three corrections to #1092's own text: **one** production site, not three (`inprocess.rs:702`/`:754` are `#[cfg(test)]`); all three Bismark-reachable presets exist in `rammap::Preset`; and option 2 from the issue ("reject the combination") would **repeal design#3**, which deliberately accepts the `--mm2_*` knobs for rammap so it can be compared apples-to-apples with `--minimap2`. Scope confirmed narrow: `--mm2_maximum_length` (convert-stage) and `-p` (`inprocess_rammap_threads`) are already honoured |
| 2 | Plan written (rev 0) | ✅ Done | `PLAN.md`. Design follows #1079's `ScoreMinForm` precedent — single-source the typed preset rather than re-parsing `-x` out of the emitted option string |
| 3 | Manual review (Felix) | ✅ Done | D-GATE = (a)+spike(b), (c) follow-up; D-A4 = ship `sr` unmeasured. Then "implement" |
| 4 | **Agent review (dual `plan-reviewer`)** | ✅ Done | `PLAN_REVIEW_A.md` + `PLAN_REVIEW_B.md`. **Both: not ready as written** — design approved, *validation* blocked. **4 findings reached independently by both**, headed by: V7 could not fail its own injection (i), and **`save_index` exists**, so the premise the whole gate debate rested on was false. 1 contradiction (`MapPb` inert vs near-inert), resolved in A's favour by measurement |
| 4b | Fold both reviews → PLAN rev 2 | ✅ Done | Every Critical/Important applied. New §3.5 (`map-pb` near-inertness), V7 rebuilt end-to-end, V7b/V7c/V7d added, V8 grew to 6 injections |
| 5 | **Implement** | ✅ Done | fmt clean · **`clippy --workspace --all-targets -D warnings` 0** · default suite **2123 pass / 0 fail** · feature suite **2133 pass / 0 fail** (all 7 new gates ran). Conflict-die assertions landed **pre-extraction** (A's sequencing point); all 32 `options` tests then passed untouched = the extraction is inert. **Injection (i) now FAILS at `282 vs 270`** — the fault rev 1's gate was blind to. ⚠️ Injection (v) is caught by the **compiler**, not a test — documented, not papered over. See `PLAN.md` §12 |
| 6 | Verify (dual `code-reviewer` + `plan-manager`) | 📋 Planned | |
| 7 | Commit + PR → `dev` | 📋 Planned | ⚠️ `cargo test -p bismark --features rammap-inprocess` is a **separate CI job** and the only one that compiles the changed line — the #1081 lesson |
| 8 | Version bump | 📋 Release cut | Stays at `3.1.0` |

## Resolved decisions

- **D-GATE** (Felix, 2026-08-02) — (a) structural + spike (b), (c) as a follow-up. Spike (b) succeeded; then dual review showed rev 1's version of the gate still could not fail its own injection, and that **`save_index` exists** — so the shipped gate drives the real `from_index` and injection (i) genuinely fails. `PLAN.md` §9a, §12.
- **D-A4** (Felix, 2026-08-02) — in-process `sr` ships **unmeasured**; deferring (c) defers the measurement. Review sharpened it: `sr` in-process is also a *hybrid* (sr options over a genome-prep-`k` index), and **mapping rate changes**, not just scores. Both stated in the CHANGELOG.

## Open (non-blocking)

- **Follow-up (c)** — per-preset real-index concordance (in-process vs subprocess vs minimap2), which would close A4. The `RAMMAP_PRESET` env var added to the crosscheck is the hook. Worth its own issue once this merges.
- **Injection (v)** is caught by the compiler (`unused variable` under `-D warnings`), not by a test. Documented at the call site.
- `set_simd_cap` is process-global and the new `Sr` tests are the first to flip it; no interference observed, but AVX2-vs-AVX-512 score equivalence was not separately confirmed.

## Key facts established in triage

- **2026-08-02** — the only production site is `mod.rs:968`; the two other `rammap::Preset` uses are test-only.
- **2026-08-02** — the default path is unaffected: `--rammap` with no `--mm2_*` flag resolves to `MapOnt`, the value hard-coded today. Only `--rammap --mm2_short_reads` and `--rammap --mm2_pacbio` change behaviour.
- **2026-08-02** — do **not** re-parse `-x` from `aligner_options`; that would make emitted text the source of truth for a typed decision, which is the two-derivations-drift shape that caused this bug (and that #1079 fixed for `ScoreMinForm`).

# Per-preset rammap concordance — closes #1092's A4

**Date:** 2026-08-03 · **Base:** `dev` `16f65f6` (i.e. with #1092 merged)
**Question:** #1092 shipped in-process `sr` **unmeasured** (A4 / D-A4) — every existing rammap concordance figure was established on `map-ont`. How well does in-process rammap agree with minimap2 per preset, and specifically for the newly reachable `sr`?
**Answer:** **100 % agreement on every co-mapped read — locus, CIGAR and alignment score — for all three presets, including the production hybrid.** One read differs in *mappability*, in the benign direction.

---

## Why this could be done hermetically

The plan assumed A4 needed an oxy run (the env-gated crosscheck wants `RAMMAP_BIN` / `RAMMAP_MMI` / `RAMMAP_READS`). It does not, for the comparison that matters: **rammap-core is a library dependency and minimap2 2.31-r1302 is on PATH**, so in-process rammap can be compared against minimap2 directly, on synthetic data, in one test run.

Note this measures a **different axis** from the existing crosscheck:

| axis | compares | status |
|---|---|---|
| existing crosscheck | rammap **library** vs rammap **CLI** | `map-ont` only, env-gated, ≤0.022 %/cell |
| **this measurement** | rammap **library** vs **minimap2** | all three presets, hermetic |

The second is the axis rammap's headline claim rests on ("concordance-gated, NOT byte-identical to minimap2"), and it is the one #1092 made newly reachable for `sr`.

## Method

32 reads over a fixed-LCG 30 kb synthetic reference: perfect, reverse-complement, 1/3/6 mismatches, 3 bp deletion, 5 bp insertion and a 12 bp 5′ soft-clip bait, at 75/100/150/250 bp. Bismark's verbatim option string (`-a --MD --secondary=no -t 1 -x <preset> -K 250K`). Compared per read on `(rname, pos, strand)` and then additionally on `(cigar, AS)`.

Throwaway harness (run, then deleted): `spikes/spike_preset_concordance.rs`, output in `spikes/concordance.log`.

## Results

### (1) Same configuration — rammap `from_seqs(preset)` vs minimap2 `-x preset`

| preset | co-mapped | locus agree | full agree (locus+CIGAR+AS) | rammap-only | mm2-only |
|---|---|---|---|---|---|
| `map-ont` | 29/32 | **29 (100 %)** | **29 (100 %)** | 0 | 0 |
| `map-pb` | 27/32 | **27 (100 %)** | **27 (100 %)** | 0 | 0 |
| **`sr`** | 28/32 | **28 (100 %)** | **28 (100 %)** | 0 | 0 |

### (2) The production hybrid — `sr` mapping options over a `map-ont`-k index

This is what a real `bismark --rammap --mm2_short_reads` run does, because `from_index` discards a preset's `k`/`w` in favour of the loaded index's.

| configuration | co-mapped | locus agree | full agree | rammap-only | mm2-only |
|---|---|---|---|---|---|
| `sr`-over-`map-ont`-index vs mm2 `sr` | 28/32 | **28 (100 %)** | **28 (100 %)** | **1** | 0 |
| control: `map-ont`-index + `map-ont` vs mm2 `map-ont` | 29/32 | 29 (100 %) | 29 (100 %) | 0 | 0 |

## Findings

### F1 — scoring and placement converge exactly, on every preset

Every read both engines mapped agrees on chromosome, position, strand, CIGAR **and** `AS`. Not "within tolerance" — identical. So the `sr` path #1092 opened does not introduce engine divergence, and the plan's residual risk 1 ("scoring converges, seeding does not") is confirmed in its first half and quantified in its second.

### F2 — the hybrid is *more* sensitive than pure `sr`, not less

The single difference is `mm6_150` (150 bp, 6 mismatches): the hybrid maps it, minimap2 `-x sr` does not. Mechanism, exactly as predicted: 6 mismatches over 150 bp average one every ~21 bp, which breaks `sr`'s `k = 21` seeding but not `map-ont`'s `k = 15`. The hybrid seeds with the index's smaller `k` and then scores with `sr`'s parameters, so it recovers a read pure `sr` loses.

**Direction matters.** The hybrid gains a read rather than losing one — no `mm2-only` cell in any configuration. The feared failure mode was silently dropping reads; the observed behaviour is the opposite, on this read set.

### F3 — a harness correction worth recording, because it inverted the headline

The first run reported **85–86 %** full agreement, with every discrepancy a soft-clipped read: rammap `75M` vs minimap2 `12S75M`, at the *same* position and score. That was **my harness, not a divergence** — rammap's `Mapping.cigar` is the aligned **core** without soft clips, and production rebuilds the SAM CIGAR from `query_start`/`query_end` via `inprocess::reconstruct_cigar`. Comparing the raw core against minimap2's full SAM CIGAR marks every clipped read divergent.

Using the production reconstruction took the figure from 85.7 % to **100 %**. Anyone re-running this must go through `reconstruct_cigar`, or they will re-derive a divergence that does not exist.

## Limitations

- **Synthetic reference, 32 reads.** This establishes that the engines agree where they both map, on a read set built to hit mismatches, indels and clips. It is not a real-data rate: it says nothing about repeats, low-complexity regions, or bisulfite-converted composition.
- **Not the bisulfite path.** Reads were mapped against an unconverted synthetic reference, not Bismark's C→T/G→A converted genomes, and not through the merge. Engine-level, not pipeline-level.
- **Library vs minimap2, not library vs rammap CLI.** The existing env-gated crosscheck still owns that axis, and it is still `map-ont`-only unless run with `RAMMAP_PRESET=sr` (now threaded through both its arms, so it can be).
- **rammap `5ea62cd`, minimap2 2.31-r1302, aarch64.**

## What this changes

A4 / D-A4 said in-process `sr` ships unmeasured. It now has a measurement: **no engine divergence, and the hybrid's seeding difference is a sensitivity gain rather than a loss.** That is not a substitute for a real-data run — the residual is now "unmeasured on real bisulfite data" rather than "unmeasured at all", which is a much smaller claim.

The cheap next step, if wanted, is `RAMMAP_PRESET=sr` on the existing crosscheck with real oxy data, which now covers both arms and is a small delta.

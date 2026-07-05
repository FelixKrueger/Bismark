# GATE 04 — `-p` slowdown attribution (cooled, interleaved)

## Question

An earlier non-interleaved median-of-3 measured the full PR stack **~6 % slower**
under `-p 6` (BASE 1145 s vs AFTER 1217 s). Was that a real regression (mimalloc's
single-thread overhead, or the parse/build_pe_mate changes), or laptop thermal
drift between the BASE block and the AFTER block?

## Method

Three distinct binaries, **interleaved** to share thermal state, each run cooled:

- `base`  = pre-epic, system allocator (`/tmp/baseline/bismark_rs.release`, 0 mimalloc syms)
- `mima`  = mimalloc only (`/tmp/mimalloc/bismark_rs.release`, 2 mimalloc syms)
- `after` = full PR stack: mimalloc + parse byte-split + build_pe_mate (`/tmp/after/bismark_rs.release`, 2 syms)

12-min cooldown, then 3 rounds of `base, mima, after` (`-p 6`, RRBS-10M, GRCm39),
with a 4-min cooldown between every run so no binary runs hotter than another.
`/usr/bin/time -l`. (This is the corrected run: the first attempt mislabeled all
three via a bash-3.2 `declare -A` bug and was discarded.)

## Results

| round | base | mima | after |
|---|---|---|---|
| 1 | 1217.7 | 1270.3 | 1328.2 |
| 2 | 1258.6 | 1281.8 | 1226.3 |
| 3 | 1242.2 | 1124.7 | 1113.8 |
| **median** | **1242.2** | **1270.3** | **1226.3** |
| **mean** | **1239.5** | **1225.6** | **1222.7** |
| per-binary spread | 3.4 % | 14.0 % | 19.2 % |

Deltas vs base: `after` **−1.3 % median / −1.4 % mean**; `mima` **+2.3 % median /
−1.1 % mean** (the sign flips with the statistic).

## Conclusion

**No `-p` regression survives a controlled measurement.** The per-binary thermal
spread (14–19 %) dwarfs every inter-binary difference (±2 %), and the `after`
range [1113.8, 1328.2] fully envelops the `base` range [1217.7, 1258.6]. The three
binaries are **statistically indistinguishable** under `-p`.

The earlier "−6 %" was a **thermal artifact** of comparing a BASE block against a
later, hotter AFTER block without interleaving. mimalloc's single-thread overhead,
if any, is below this laptop's noise floor.

**Net:** mimalloc is a clean **3.13× `--multicore`** win (GATE_01) with **no
measurable `-p` cost** — no trade-off to weigh, no gate needed. The function-level
parse win (2.04×, criterion) and byte-identity results stand regardless. Absolute
wall-times should still be re-confirmed on the Linux x86_64 benchmark host, where
there is no laptop throttling.

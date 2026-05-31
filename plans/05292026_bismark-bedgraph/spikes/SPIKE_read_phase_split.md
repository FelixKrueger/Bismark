# SPIKE — `--CX` read-phase sub-split (decompress / parse / insert / merge)

**Date:** 2026-05-30 · **Machine:** oxy (`dockyard-oxy-0`) · **Status:** complete, recommendation below.
**Feeds:** the parallel multi-file parse+aggregate follow-up to epic #797 (post-#893-merge). No code committed; harness is throwaway.

## 1. Question, success criteria, strategy

**Question.** The full-size `--CX` run of `bismark2bedGraph_rs` spends ~83% of wall-clock (~797s of ~957s) in a **single-threaded read+parse+aggregate** phase. Of that, how much is (a) gzip **decompression**, (b) line **parse+validate**, (c) hashmap **insert** into the growing `(chr,pos)→(meth,unmeth)` map? And what does it cost to **merge** a per-file map into a global one? These decide **Family A** (per-file parallel parse + merge; ceiling bounded by the largest single file) vs **Family B** (sharded / intra-file parallelism; more complex, can split the dominant CHH file).

**Success criteria.** A per-sub-phase time/percentage breakdown on the dominant file(s) summing to the measured read time; the call:position ratio; the merge cost at scale; enough signal to recommend A vs B with a quantified expected speedup. **Met.**

**Strategy.** Staged passes over each file, each doing strictly more work than the last, so subtraction isolates each cost (decompress = P1; parse = P2−P1; insert = P3−P2). P4 then merges P3's per-file map into a fresh pre-sized global map = the Family-A reduce step. Faithful to production: reuses `bismark_bedgraph::validate::validate_call`, mirrors `Aggregator`'s chr-interning + `(u32 chr_id, u32 pos) → (u32 meth, u32 unmeth)` map and input.rs's header/`Bismark`/chomp handling.

## 2. Script & how to run

- **Runnable harness:** `rust/bismark-bedgraph/examples/read_phase_split.rs` (throwaway, uncommitted; archived copy: `plans/05292026_bismark-bedgraph/spikes/spike_read_phase_split.rs`).
- **Build (oxy):** `cd /tmp/Bismark-bg/rust && CARGO_TARGET_DIR=target_oxy ~/.cargo/bin/cargo build -p bismark-bedgraph --example read_phase_split --release`
- **Run:** `./target_oxy/release/examples/read_phase_split <file.gz> [more files...]`
- **Inputs:** `/tmp/bg_keep/ext_full/*.txt.gz` (the 6 full-size `--CX` context files; CHH_OT/OB 2.44 GB gz each).

## 3. Results

OT strand measured directly; OB is structurally symmetric (same context, opposite strand) so OB ≈ OT per context.

| File | Uncompressed | Calls | Positions | calls:pos | Decompress | Parse | **Insert** | **Read (P3)** | **Merge (P4)** |
|---|---|---|---|---|---|---|---|---|---|
| CpG_OT | 2.74 GB | 35.1M | 19.2M | 1.83 | 3.7s (29%) | 1.8s (15%) | 7.1s (56%) | 12.6s | 0.3s (~2%) |
| CHG_OT | 12.95 GB | 166.1M | 88.6M | 1.87 | 11.4s (17%) | 7.7s (11%) | 49.3s (72%) | 68.4s | 1.3s (~2%) |
| CHH_OT | 45.87 GB | 588.2M | 310.9M | 1.89 | 31.3s (11%) | 26.2s (9%) | 222.6s (79.5%) | 280.0s | 4.9s (~2%) |

**Iteration log.** #1: harness built locally + on oxy, smoke-tested on CpG_OT (small) — sane output (insert 56%, merge 0.3s), no bugs. #2: ran full CHG_OT + CHH_OT — clean exit, numbers above. No further iteration needed; criteria met on first real run.

### Derived totals

- **Serial sum of all 6 per-file reads** (OT+OB) = 2×(12.6 + 68.4 + 280.0) = **722s**. The measured shared-map read is **797.5s** → the single 838M-entry shared map pays a **~75s (~10%) cache-miss tax** that smaller per-file maps avoid.
- **Total calls** ≈ 2×(35.1+166.1+588.2)M ≈ **1.58 B**; **distinct positions** = 837.7M (known from the byte-identity run) → aggregate **calls:positions ≈ 1.88:1**.
- **Map footprint** validates ~40 B/entry: CHH_OT 310.9M × 40 B = 12.4 GB (matches the est.). Full single map: 837.7M × 40 B ≈ **~33.5 GB**.

## 4. Findings

1. **Read phase is insert-bound, and increasingly so with scale** — insert share CpG 56% → CHG 72% → CHH **79.5%**; decompress share falls 29% → 17% → **11%**. Cause: random-access inserts into an ever-larger map are memory-latency-bound; the bigger the map, the worse the cache behaviour.
2. **Decompression — the only part not parallelizable within a single gzip stream — is just ~11% at the dominant file.** So per-file parallelism captures ~89% of the dominant file's cost.
3. **Merge is ~2% at every scale** (CHH: 4.9s for 310.9M entries). The Family-A serial reduce is effectively free; it was the design's main risk and it's gone.
4. **Per-file maps are cheaper than one shared map** (722s vs 797s) — Family A also removes the ~10% shared-map cache tax.

### Projected Family A performance (fact-based)

- **Parallel parse wall-clock** = max per-file read = **CHH ≈ 280s** (the two CHH files run on separate cores concurrently; all others finish under that).
- **Merge** (fold 6 maps → global, incrementally) ≈ sum of per-file merges into a growing 838M map ≈ **~30–50s** (conservative; the pre-sized samples sum to ~13s).
- **Read+aggregate: ~797s → ~320s ≈ 2.5×.**
- **Full `--CX` pipeline:** current ~957s (read 797 + sort 31 + gen 25 + gzip 103) → **~479s ≈ 2.0×** (and **~7.9× vs Perl's 3783s**, up from 3.8×).

### Memory (the cost side)

- **Family A roughly doubles peak RAM.** Naïve "build all 6 maps then merge" peak ≈ sum of 6 maps (~33 GB) + global (~33.5 GB) ≈ **~67 GB** vs the current ~33.5 GB single map.
- **Mitigation — incremental merge:** fold each per-file map into the global *as it completes* (in argv order, preserving ownership) and free it → peak ≈ global + largest in-flight map ≈ 33.5 + 12.4 ≈ **~46 GB**.
- This **worsens the I3 ceiling**: a ~64 GB workstation that just fits today's --CX would OOM under Family A. Fine on oxy/colossal (256 GB+). Must be documented; the spill-to-disk future (SPEC §9) becomes more relevant if small-host --CX is ever a goal.

## 5. Reference snippets (carry to implementation)

- **Byte-identity is decoupled from parallelism** (confirmed by design, not just the spike): counts are commutative/associative (sum per `(chr,pos)`), and chromosome **ownership reduces to `chr → min(argv_index)`** — a tiny side-reduction over a few thousand chrs, independent of the count aggregation. The verified `Aggregator::into_sorted` ordering logic needs **no change**; only the map-fill parallelizes.
- **Reduce shape (cheap):** pre-size the global (`FxHashMap::with_capacity_and_hasher`), iterate each per-file map, `entry((id,pos)).or_insert((0,0))` then `+= counts`. ~63M merge-ops/s observed.
- **Ownership during merge:** merge per-file maps **in argv order**; first file to contribute a chr sets its `order_key` (`{owner_basename}.chr…`). Same rule as today's sequential `read_into` loop, just applied at merge time.
- **Thread model:** ≤6 (or ≤12 non-directional) input files → `std::thread::scope`, one thread per file. No rayon needed for Family A. (Family B would need a pool + sharded maps.)

## 6. Recommendation

**Proceed with Family A (per-file parallel parse + incremental merge).** It is simple (scoped threads + one cheap reduce), byte-identity-safe (ownership decouples to a trivial side-reduction; ordering logic unchanged), and delivers a **fact-based ~2.0× on full `--CX`** (~2.5× on the read phase). The merge — its only structural risk — is proven ~2%.

**Defer Family B (sharded / intra-file).** It can only beat Family A by splitting the single CHH file (decompress serially → fan parse+insert to N cores, ~31s decompress floor), buying perhaps another ~1.5–2× on the extreme `--CX` case — at a large complexity cost (sharded concurrent inserts, sharded reduce) and **only** for the power-user `--CX` path. Revisit only if `--CX` wall-clock becomes a real user pain after Family A ships.

**Condition:** document the ~2× peak-RAM increase (use incremental merge to cap it at ~46 GB) and the worsened small-host `--CX` ceiling.

## 7. Limitations

- **Peak RSS at full scale was estimated, not hard-measured** (838.7M × ~40 B/entry ≈ 33.5 GB), though the per-file footprints (measured distinct counts × 40 B) validate the per-entry constant. A `/usr/bin/time -v` on a full 6-file `--CX` run would confirm the absolute number (~16 min; not run to avoid another long job — offer pending).
- **OB strand inferred by symmetry**, not measured (OT measured for all three contexts). Real OB sizes differ marginally; the per-context split %s are structural and won't shift the recommendation.
- **Family A wall-clock is projected, not prototyped.** A 6-thread prototype would confirm the 280s parallel floor + real global-merge cost directly; the projection rests on measured per-file reads + measured merge rate. Could be a fast follow-up spike before the implementation plan.
- Single run per file (no repetition for variance); these are large deterministic CPU/IO jobs, so run-to-run noise is small relative to the >2× signal.

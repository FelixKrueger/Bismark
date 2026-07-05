# PLAN — `bismark2bedGraph_rs` parallel per-file parse (Family A)

**Status:** rev 1 (2026-05-30) — manual-approved (Felix) + dual plan-review folded in (both verdicts APPROVE WITH CHANGES, no design rework). Awaiting implementation trigger. See Revision History (§12).
**Branch:** fresh `rust/bedgraph-parallel-parse` off the merged `rust/iron-chancellor` (post-#893).
**Design source:** spike `plans/05292026_bismark-bedgraph/spikes/SPIKE_read_phase_split.md` (read phase is insert-bound; merge ~2%; Family A projected ~2.0× full `--CX`). Binding contract: `rust/bismark-bedgraph/SPEC.md` (decompressed-content byte-identity to Perl v0.25.1). Epic #797 is **closed**; this is a standalone perf follow-up that must **not** alter byte-identity.

## 1. Goal

Make `bismark2bedGraph_rs` parse its input files **concurrently** (one task per file) and merge the per-file count maps into the final aggregator, replacing today's single-threaded sequential read loop (`lib.rs:71-74`). Target: **~2.0× faster on full-size `--CX`** (957s → ~479s; ~7.9× vs Perl), with **byte-identical output at any thread count** (N-invariant). Parallelism is **always-on by default** (threads = `min(#files, cores)`); a new optional `--parallel N` caps the thread count (`N=1` reproduces today's exact sequential path) and doubles as a peak-RAM lever.

**Non-goals (unchanged from SPEC §9):** no external-spill; no change to `coverage2cytosine`; no wiring into the extractor's subprocess path (#798 domain); no Family-B intra-file/sharded parallelism (deferred — only beats Family A on the extreme `--CX` case at large complexity).

## 2. Context

- **Where the change lives:**
  - `src/lib.rs` `run()` — replace the sequential `for path in &inputs { read_into(...) }` loop with a call to a new parallel parse+merge driver.
  - `src/aggregate.rs` — add `Aggregator::merge_from(&mut self, other: Aggregator)` (the byte-identity-load-bearing new logic).
  - `src/parallel.rs` (NEW) — the bounded-pool, argv-order **batched** parse+merge driver.
  - `src/cli.rs` — add `--parallel <N>` flag + `ResolvedConfig.parallel: usize`; validate `N >= 1`.
- **Reused unchanged:** `input::read_into` / `input::basename` (per-file parsing is identical to today), `Aggregator::add` / `into_sorted` (the verified ordering + emission logic is untouched), `output::write_outputs`, `ucsc::write_ucsc`. The parallel path only changes **how the map gets filled**, not the map's contents or emission.
- **The C1 ownership rule (SPEC §2.1B):** chr ownership = first input file in **argv order** to emit a call for that chr; output order = bytewise sort of `{owner_basename}.chr{transformed}.methXtractor.temp`. The merge must preserve this exactly.
- **Dependencies:** none new — `std::thread::scope` (std) + `std::sync::atomic` if a pull-queue is needed. No rayon. `Aggregator` is `Send` (FxHashMap + Vec + Box<str>).
- **Verification reality:** because `aggregate.rs` is the most byte-identity-load-bearing module, the merged change must re-pass the full byte-identity gate on real data (oxy, full `--CX`), at **both `--parallel 1` and `--parallel ≥2`**, each byte-identical to Perl **and** to each other.

## 3. Behavior

Prerequisites: `select_input_files` returns the argv-ordered file list (unchanged).

1. Resolve `threads = min(cfg.parallel, files.len())`, where `cfg.parallel` defaults to `available_parallelism()` (clamped ≥1).
2. **Fast paths (identical to today's bytes & code path):** if `files.len() <= 1` **or** `threads <= 1`, run the existing sequential loop (`Aggregator::new()` + `read_into` per file in argv order). This guarantees `--parallel 1` is bit-for-bit the current implementation.
3. **Parallel path — argv-order batched:** iterate `files.chunks(threads)` (chunks preserve argv order):
   a. For each file in the batch, spawn a scoped thread that builds a **per-file** `Aggregator` via `read_into(path, no_header, &basename(path), &mut local_agg)`.
   b. Join the batch; collect results **in argv order** (the chunk's order).
   c. Merge each result into the running `global` via `global.merge_from(local_agg)`, **in argv order**, freeing each local map immediately after merge (the incremental-RAM behaviour).
   d. Propagate the first `Err` (malformed line) in argv order → exit 1, matching the sequential path.
4. Hand the merged `global` Aggregator to `output::write_outputs` (unchanged).

**Why this is byte-identical:** counts are commutative/associative (sum per `(chr,pos)`), so partition+merge yields identical counts. Ownership is preserved because chromosomes are first **observed by the merge in argv order** — across batches (chunks are argv-ordered) and within a batch (merged in chunk order). `merge_from` is **first-wins**: a chr already in `global` keeps its `order_key` (set by the earlier-argv file); a chr new to `global` adopts `other`'s `order_key` (built from `other`'s basename). This reproduces exactly what the sequential single-pass loop does (first file in argv order to see a chr owns it). `into_sorted` is unchanged, so emission order/format is identical.

### Edge cases

- **1 input file** → fast path (no threads). (`--CX` always has ≥2; CpG-only often 2.)
- **`--parallel 1`** → fast path = today's exact sequential code. Required N-invariance anchor.
- **`threads < #files`** → multiple batches; argv order preserved across chunk boundaries (chunk K is argv-earlier than chunk K+1). Lower peak RAM, less parallelism.
- **Empty / all-header file** → its per-file Aggregator is empty; `merge_from` is a no-op for it. Same as today.
- **Malformed line in any file** → that thread's `read_into` returns `Err`; driver returns it (exit 1). Inconsistent line → warn+skip in that thread (stderr; ordering of warnings across threads is not byte-gated — SPEC Q4 banners/stderr are not part of the contract).
- **Chr absent from the first argv file (the make-or-break MT case)** → owned by a later file; merge-in-argv-order reproduces Perl's `MT,1,2`. Mandatory test (§9).
- **Two contigs colliding on the transformed key** (`A|B`/`A/B`) → already a documented divergence (SPEC §7); merge behaviour unchanged (keyed on original name).
- **Very large `--CX`** → peak RAM rises (see §6); `--parallel N` lowers it. Documented ceiling, not guarded (I3 — allocator aborts on exhaustion).

## 4. Signature

```rust
// cli.rs — new flag
/// Number of input files to parse concurrently (default: min(#files, cores)).
/// `--parallel 1` forces the sequential path. Also bounds peak memory: fewer
/// concurrent per-file maps = lower peak RAM on large --CX runs.
#[arg(long = "parallel")]
pub parallel: Option<usize>,

// ResolvedConfig — new field (review B: Option, not a "0 = auto" sentinel)
/// Parse-thread cap. `None` = auto (`available_parallelism()`); `Some(n)` = an
/// explicit cap with `n >= 1` (`Some(1)` = sequential). `--parallel 0` is
/// rejected at validate() with `BadParallel`.
pub parallel: Option<usize>,

// aggregate.rs — new method
impl Aggregator {
    /// Merge `other` into `self`, summing per-`(chr,pos)` counts. Chromosome
    /// metadata is **first-wins**: a chr already in `self` keeps its `order_key`
    /// (an earlier-argv file owned it); a chr only in `other` is adopted with
    /// `other`'s metadata. MUST be called in argv order to preserve C1.
    pub fn merge_from(&mut self, other: Aggregator);
}

// parallel.rs (NEW)
/// Parse `files` (argv order) into one merged Aggregator using up to `threads`
/// concurrent per-file parsers + argv-order batched merge. `threads = None`
/// resolves to `available_parallelism()` (or 1 on error); `Some(1)` or a single
/// file → the sequential path. Byte-identical to the sequential result at any N.
pub fn parse_files(
    files: &[std::path::PathBuf],
    no_header: bool,
    threads: Option<usize>,
) -> Result<Aggregator, BismarkBedgraphError>;
```

## 5. Implementation outline

1. **`cli.rs`:** add `parallel: Option<usize>`. In `validate()`: reject `Some(0)` with a new `BismarkBedgraphError::BadParallel { value: 0 }` (mirror `BadCutoff`'s `> 0` guard, `cli.rs:180-188`); otherwise pass `self.parallel` straight through to `ResolvedConfig.parallel` (an `Option<usize>` — `None` = auto, resolved to `available_parallelism()` inside `parse_files`; `Some(n>=1)` = explicit cap). Add the `--help` line. No "0 = auto" sentinel.
2. **`aggregate.rs`:** implement `merge_from` — the **byte-identity-load-bearing** addition (review C1, both reviewers). Full body, exact:

   ```rust
   /// Merge `other` into `self`, summing per-`(chr,pos)` counts. Chromosome
   /// metadata is FIRST-WINS: a chr already in `self` keeps its `order_key`
   /// (an earlier-argv file owned it); a chr only in `other` is ADOPTED with
   /// `other`'s metadata VERBATIM (its `order_key` — built from `other`'s
   /// basename — is never recomputed). MUST be called in argv order.
   pub fn merge_from(&mut self, other: Aggregator) {
       // Drop other's chr_ids; remap by original name via self.chr_ids.
       let Aggregator { chrs: o_chrs, counts: o_counts, .. } = other;
       // remap[other_id] -> self_id. Iterate other.chrs in **Vec (id) order**,
       // NEVER the hashmap (iteration order is nondeterministic). The order
       // here only assigns ids to NEW chrs; output order is set later by
       // into_sorted's order_key sort, so any consistent remap is byte-identical.
       let mut remap = vec![0u32; o_chrs.len()];
       for (o_id, meta) in o_chrs.into_iter().enumerate() {
           let self_id = match self.chr_ids.get(meta.original.as_ref()) {
               Some(&id) => id, // self owns it (earlier argv) — keep self's order_key
               None => {
                   let id = self.chrs.len() as u32;
                   self.chr_ids.insert(meta.original.clone(), id);
                   self.chrs.push(meta); // adopt other's metadata verbatim
                   id
               }
           };
           remap[o_id] = self_id;
       }
       // Every key's remapped id stays a valid index into self.chrs (required:
       // into_sorted indexes per_chr/chrs by chr_id — review C1/A-C3).
       for ((o_chr_id, pos), (m, u)) in o_counts {
           let e = self.counts.entry((remap[o_chr_id as usize], pos)).or_insert((0, 0));
           e.0 += m;
           e.1 += u;
       }
   }
   ```
   (The `Some`/`None` borrow split is the same NLL pattern as the existing `intern`, `aggregate.rs:81-92`.) Add unit tests (§9).
3. **`parallel.rs` (NEW):** implement `parse_files`:
   - Resolve threads: `let threads = match threads { None => available_parallelism().map(|n| n.get()).unwrap_or(1), Some(n) => n }; let threads = threads.min(files.len()).max(1);` (fall back to **1 = sequential** if `available_parallelism()` errors — review A/§11 risk 4).
   - **Fast path:** `files.len() <= 1 || threads == 1` → today's exact sequential loop (`Aggregator::new()` + `read_into` per file in argv order), return. This is the guaranteed N-invariance anchor.
   - **Parallel path — seed-from-first (pre-sizes the global; review A pre-size):** seed `global` from the **first** per-file Aggregator (argv file 0, which therefore owns its chrs), then merge all others into it in argv order. Concretely, iterate `files.chunks(threads)` (chunks preserve argv order); for each chunk, `std::thread::scope` spawns one `parse_one` per file, join collecting results **in chunk order**; then for the very first result overall do `global = result0`, and for every subsequent result `global.merge_from(result)`. Propagate the first `Err` in argv order. Return `Ok(global)`.
   - **LYNCHPIN INVARIANT (review B-C2 — state explicitly):** `parse_one` builds **one Aggregator per file** using **that file's own basename** (`read_into(path, no_header, &basename(path), &mut a)`), so each per-file map's `order_key`s are already the correct owner-basename strings. This is *why* merge-in-argv-order first-wins == sequential single-pass ownership. A worker must NEVER accumulate two files into one Aggregator (would break within-worker argv order).
4. **`lib.rs` `run()`:** replace the sequential loop (`:70-74`) with `let agg = parallel::parse_files(&inputs, cfg.no_header, cfg.parallel)?;`.
5. **`lib.rs`** module decl: `pub mod parallel;`.
6. **Docs:** `--help` line for `--parallel`; `README.md` + `CHANGELOG.md` note (parallel parse, ~2× on `--CX`, RAM note); SPEC addendum referencing this plan + the spike (the in-memory-aggregation section gains a "parallel fill" note; byte-identity contract unchanged).

## 6. Efficiency

- **Time (speedup is a function of N, NOT a flat 2.0× — review B):** with the batched design, batches serialize, so only `N ≥ #files` (on ≥#files cores) overlaps the two big CHH files and reaches the floor. read+aggregate ~797s → ~320s (parse floor = largest file ≈ CHH 280s + merge ~30–50s); full `--CX` ~957s → ~479s. Lower N serializes the CHH pair across batches and gives less. CpG-only path: sub-second read at any N (no regression).
- **Merge cost:** ~2% of read (spike: 4.9s for 310.9M entries) — negligible; `global` is seed-from-first (reuses file 0's allocation) to limit rehash churn.
- **Memory — corrected (review A-Important / B-C4).** Earlier "~46 GB at default" was wrong: ~46 GB is the *streaming-merger* peak (deferred §10), not the *batched* design. The batched peak ≈ (per-file maps held in the largest concurrent chunk) + (`global` at that moment). Per-entry ≈ 40 B (validated: CHH_OT 310.9M × 40 B = 12.4 GB; full map 837.7M ≈ 33.5 GB). Estimated peaks (full `--CX`, 6 files; **estimates pending the V8 hard `/usr/bin/time -v` measurement**):

  | `--parallel N` | concurrent per-file maps | est. peak RAM | parse wall-clock |
  |---|---|---|---|
  | **1** (sequential) | none — single growing map | **~33.5 GB (= today)** | ~722–797s (no speedup) |
  | 2 | ≤2 (incl. the CHH pair in one chunk) | ~40–46 GB | CHH pair in 1 chunk → ~350s |
  | 4 | ≤4 | ~46–55 GB | ~300–330s |
  | **#files (default, ≥6 cores)** | all 6 at once (single chunk) | **~50–67 GB** | ~320s (**full ~2×**) |

  **`--parallel` is the combined speed/RAM lever.** To match today's RAM exactly, use **`--parallel 1`** (review B — README must say this, not "low N"). **Worsens the I3 small-host ceiling**: a ~64 GB host that just fits today's `--CX` must use `--parallel 1`. Documented; not guarded (allocator aborts on exhaustion — I3).
- **gzp compression threads are a separate pool** (`output.rs:69` — `available_parallelism().min(4)`, already saturates at 4); `--parallel` does **not** touch them (confirmed by both reviewers). No conflation.

## 7. Integration

- **Reads/writes:** identical inputs/outputs to today. `into_sorted` + `write_outputs` + `ucsc` unchanged → identical bytes.
- **Order relative to other steps:** parse+merge replaces the read loop; sort/emit/compress unchanged and still sequential after the barrier (all calls aggregated before sort).
- **Downstream impact:** none — the produced `Aggregator` is identical to the sequential one. The extractor's subprocess path (Perl `bismark2bedGraph`, still used by the Rust extractor per SPEC §1) is unaffected.
- **Byte-identity gate (mandatory, Phase-F-equivalent):** re-run on oxy full `--CX` at `--parallel 1` AND `--parallel 6`, each decompressed-byte-identical to Perl v0.25.1 and to each other. The 8 hermetic CI fixtures must pass unchanged through the new default (parallel) path.

## 8. Assumptions

- **Merge-in-argv-order reuses the existing `order_key`** (no recomputation) → first-wins reproduces sequential ownership. *Fixed rule.* (Alternative "decoupled ownership = `chr→min(argv_index)`, merge in completion order" is a deferred RAM/throughput optimization — not v1.)
- **`--parallel` = parse-thread cap only**, default `min(#files, cores)`; `N=1` = sequential; `N=0` rejected. *Configurable.*
- **Batched (chunked) merge** is the v1 strategy: simplest correct design giving argv-order ownership + a RAM lever via `threads`. *Fixed for v1; merger-thread streaming is a future option.*
- `std::thread::scope` + per-file `Aggregator` (one per file, not per worker — a worker must never accumulate two files into one map, which would break within-worker argv order). *Fixed rule.*
- `Aggregator` and `BismarkBedgraphError` are `Send`. (Verify: `BismarkBedgraphError` may carry `PathBuf` — `Send`. ✓)
- Extractor emits ≤6 (directional) / ≤12 (non-directional) files; thread counts beyond `#files` are clamped. *Fixed.*
- **`global` is seeded from the first per-file Aggregator** (argv file 0) and the rest merged into it — reuses file 0's allocation, limiting rehash churn on the fold (review A pre-size). *Fixed rule.*
- **Error semantics (review A precision):** the *returned* error matches sequential — the driver propagates the **first `Err` in argv order**, so a malformed line yields the same `MalformedCallLine` + exit 1 as today. Two differences that are explicitly **not** byte-gated (SPEC Q4 — only data streams are): (a) the parallel path fully parses all files in a chunk before propagating, so on malformed input it does *more* work before failing than sequential's early stop — but **no output is written either way** (the error precedes `write_outputs`); (b) inconsistent-line `warn`s to stderr may interleave across threads. *Fixed rule.*

## 9. Validation

| # | Verify | How | Expected |
|---|--------|-----|----------|
| V1 | `merge_from` sums counts | unit: build 2 per-file aggs (chr1@100 + vs −), merge | `(100,1,1)` — matches today's `cross_file_position_merge` |
| V2 | **Ownership first-wins via merge (the make-or-break case)** | unit: per-file agg for `CpG_OT`(1,2) + `CpG_OB`(2,MT), `merge_from` in argv order | chr order `MT,1,2`; counts MT`(5,0,1)`,1`(5,1,0)`,2`(5,1,0),(6,0,1)` — identical to `make_or_break_chr_only_in_later_file` |
| V3 | Ownership tracks argv order, not strand | unit: reverse argv order, merge | matches `ownership_is_argv_order_not_strand_order` |
| V4 | **N-invariance** | run fixtures through `parse_files` at threads=1,2,6 | identical `into_sorted()` output at every N. ⚠️ Must use the V10 ≥3-file fixture — ≤2-file fixtures collapse to one chunk and never exercise the multi-batch path (review A-C2/B-C3, the blind spot). |
| V5 | `--parallel 0` rejected; default resolves | cli unit + `parse_files(threads=0)` | `BadParallel`; auto = `available_parallelism` |
| V6 | 8 hermetic CI byte-identity fixtures pass through the **default (parallel)** path | `cargo test -p bismark-bedgraph` | all pass unchanged |
| V7 | **Real-data byte-identity (oxy, full `--CX`)** at `--parallel 1`, **`3`** (multi-batch on 6 files), and `6` | `scripts/bedgraph_byte_identity.sh` vs Perl v0.25.1 (decompressed) | all identical to Perl + to each other. `--parallel 3` is required — 1 and 6 are both single-chunk at 6 files and never exercise multi-batch on real data (review A-Important/B-C3). |
| V8 | Perf + **peak RAM** | `/usr/bin/time -v` full `--CX` at `--parallel 1`, `2`, `6` on oxy | ~2× wall-clock at N=6; confirms ~280s parse floor AND the §6 RAM table (replaces the estimates with hard `Maximum resident set size`). |
| V9 | Malformed line still fails | unit: one file has a short line | `parse_files` returns `MalformedCallLine`, exit 1 |
| V10 | **Multi-batch chunk-boundary ownership** (the test V4 needs) | unit: ≥3 per-file aggs where a chr owned by file 1 reappears in file 3; run `parse_files` at threads=1,2,3 | byte-identical `into_sorted()` at all three N; file-1 ownership preserved across the chunk boundary (review A-C2/B-C3) |
| V11 | **`into_sorted` remap correctness after merge** | unit: merge two aggs whose chr-id orders DIFFER (other's id order ≠ self's) | every count maps to the correct chr; no panic / out-of-bounds; counts + order match the sequential build (review A-C3) |
| V12 | `\|`/`/` transformed-key collision unchanged under merge | unit: merge aggs containing `A\|B` and `A/B` | kept separate (keyed on original name), matching today's documented divergence (review B; SPEC §7) |

## 10. Questions or ambiguities

- **(RESOLVED, Felix 2026-05-30):** trigger = **always-on + optional `--parallel N` cap**; `N=1` = sequential. Byte-identity is N-invariant, so this is a UX/RAM choice.
- **(Open, non-blocking):** `--parallel` default — `available_parallelism()` (all cores) vs a conservative cap (e.g. `min(#files, 8)`). Taken for now: `min(#files, available_parallelism())`. Files are ≤6/≤12 so this self-caps; revisit only if oversubscription on shared hosts is observed.
- **(Open, non-blocking):** RAM strategy — batched (v1) vs a streaming merger-thread (~46 GB regardless of argv order). Taken for now: batched (simpler, correct, `threads` is the lever). Streaming is a documented future optimization.

**Design alternative considered — decoupled ownership (`chr → min(argv_index)`)** (raised by both reviewers; the spike's own framing). Instead of "merge must happen in argv order so first-wins picks the right owner," compute ownership *independently*: `owner(chr) = the file with the smallest argv index containing it` (a tiny side-reduction over a few thousand chrs), then merge counts in **any** order (e.g. completion order, freeing maps ASAP → the spike's ~46 GB peak regardless of argv order), recomputing each chr's `order_key` from its true owner's basename at the end.

**Why v1 chooses batched merge-in-argv-order anyway:** (1) it reuses the *already-verified* `order_key` machinery verbatim — first-wins == the sequential single-pass loop, so the byte-identity argument is "same observations, same order" rather than new ownership code; (2) `merge_from` adds ~15 lines and recomputes nothing; the decoupled variant needs new `order_key`-recomputation logic keyed on a separately-maintained `min(argv_index)` map — *more* new byte-identity-load-bearing code to audit, the opposite of low-risk for the most load-bearing module. The decoupled variant's advantages (order-independent N-invariance proof; ~46 GB peak at any N) are real and make it the right **follow-up** once batched v1 is gate-proven — but they buy a better RAM profile, not output, and v1 prioritizes auditability. Recorded so the choice is deliberate, not default.
- **(Open, non-blocking):** should `--parallel` appear in the extractor's forwarded-flag set later? Out of scope here (#798 domain).

## 11. Self-Review

- **Efficiency:** parse floor = largest file (CHH 280s); merge ~2%; CpG path unaffected. Default `threads=#files` maximizes parallelism on the target (≥6-core) hosts. Memory is the real cost — surfaced in §6 with `--parallel` as the lever and the I3 ceiling noted. No accidental gzp-thread conflation.
- **Logic:** the byte-identity argument rests on (a) count commutativity and (b) merge-in-argv-order = first-wins = sequential ownership. Both hold for batched chunks (argv-ordered across and within chunks). The `--parallel 1`/single-file fast path is literally today's code → a guaranteed N-invariance anchor. `into_sorted` untouched.
- **Edge cases:** covered — 1 file, `--parallel 1`, `threads<#files` multi-batch, empty file, malformed (fail) / inconsistent (warn+skip) lines, the MT make-or-break ordering, `|`/`/` collision (unchanged), large-`--CX` RAM.
- **Integration:** outputs provably identical (same final Aggregator → unchanged emit). Mandatory real-data re-gate at N=1 and N≥2 because `aggregate.rs` is byte-identity-load-bearing.
- **Risks remaining:** (1) per-file maps held within a batch raise peak RAM — mitigated by `--parallel`; (2) a worker accumulating >1 file would break ownership — explicitly forbidden (one Aggregator per file); (3) thread-panic vs `Result` error — `join().expect()` turns a panic into a hard fail, while `read_into` errors propagate as `Result` (exit 1), matching sequential; (4) `available_parallelism()` can fail on exotic platforms — fall back to 1 (sequential) on `Err`.

## 12. Revision history

**rev 1 (2026-05-30)** — manual approval (Felix) + dual plan-review (`PLAN_REVIEW_A.md` / `PLAN_REVIEW_B.md`, both APPROVE WITH CHANGES, no design rework) folded in:
- **C1 (both):** `merge_from` written out as exact code (§5) — iterate `other.chrs` in **Vec/id order** (never the hashmap), first-wins by name, **adopt `order_key` verbatim**, clone `Box<str>`, keep remapped ids valid `self.chrs` indices (the `into_sorted` invariant).
- **C2 (B) / lynchpin:** the per-file-Aggregator-uses-its-own-basename invariant stated explicitly (§5 step 3) — it is *why* first-wins == sequential ownership.
- **C2/C3 (both):** validation gap closed — V4 flagged as needing the new **V10 multi-batch chunk-boundary** test (≤2-file fixtures collapse to one chunk and never exercise the new path); added **V11** (`into_sorted` remap correctness, distinct chr-id orders) and **V12** (`|`/`/` collision under merge); V7 real-data gate gains a **`--parallel 3`** multi-batch cell; V8 now also captures hard peak RAM.
- **RAM (both, Critical):** §6 corrected — the batched default holds all per-file maps concurrently (~50–67 GB at `N=#files`), NOT the spike's ~46 GB (that's the deferred *streaming* peak). Added an N-indexed (peak, speedup) table; README guidance = "**use `--parallel 1` to match today's RAM**." Speedup framed as a function of N, not a flat 2.0×.
- **CLI (B):** `ResolvedConfig.parallel` is `Option<usize>` (`None`=auto), not a "0=auto" sentinel; `parse_files(threads: Option<usize>)`.
- **Alternatives (both):** §10 now justifies batched merge-in-argv-order vs the decoupled `chr→min(argv_index)` variant explicitly (auditability + reuse of verified machinery over a better-but-newer RAM profile); decoupled framed as the gate-proven follow-up.
- **A precision:** error semantics clarified (returned error matches sequential via first-Err-in-argv-order; extra parse work + stderr interleave are not byte-gated); `global` seed-from-first to limit rehash churn; `available_parallelism()` failure → sequential fallback.

**rev 0 (2026-05-30)** — initial plan from the read-phase spike; one critical question resolved with Felix (always-on + optional `--parallel` cap).

## 13. Implementation notes (2026-05-30)

Implemented on branch `rust/bedgraph-parallel-parse` (off merged `rust/iron-chancellor` @ `700acf3`). Files: `error.rs` (`BadParallel`), `cli.rs` (`--parallel Option<usize>` + `ResolvedConfig.parallel` + `Some(0)` rejection), `aggregate.rs` (`merge_from`, exact per §5), `parallel.rs` (NEW — `parse_files` + `parse_sequential` + `parse_one`, batched seed-from-first), `lib.rs` (`pub mod parallel;` + `run()` wiring + doc).

**Local verification (all green):** `cargo fmt --check` clean; `cargo clippy --all-targets -- -D warnings` clean; **74 unit** tests (64 prior + 10 new: aggregate V1/V2/V3/V11/V12, parallel V4+V10/single-file/V9, cli V5×2), **8 hermetic byte-identity fixtures pass through the new default parallel path (V6)**, 5 CLI-behavior, 3 doctests; 3 real-data tests `ignored` (env-gated).

**Minor deviations from the plan (none material to behavior):**
1. `parse_one(path: &Path, …)` not `&PathBuf` — clippy `ptr_arg`; call sites pass `&PathBuf` via deref coercion. (Plan §4/§5 wrote `&PathBuf`.)
2. Added a `parse_sequential` helper (shared by the two fast paths) rather than inlining the loop twice — same behavior, less duplication.
3. Test helper `run_sorted` returns a `SortedRows` type alias (clippy `type_complexity`); V9 test uses `match` not `unwrap_err()` (the `Ok` type `Aggregator` is not `Debug`).

**Iteration log:** #1 first build — clippy `ptr_arg` (`&PathBuf`→`&Path`) + `cargo test` needed `Aggregator: Debug` for `unwrap_err()`. Fixed: `&Path` signature + `match` in the V9 test. #2 — clippy `needless_borrow` (`&dir.path()`→`dir.path()`, 7 sites) + `type_complexity` on `run_sorted`. Fixed: dropped the `&`, added the `SortedRows` alias. #3 — all gates green.

**Still pending (the definitive runtime gate — NOT yet run):** V7 real-data byte-identity on oxy full `--CX` at `--parallel 1`, `3`, `6` (each identical to Perl v0.25.1 + to each other) and V8 (hard wall-clock + peak RSS). The hermetic V6 fixtures already prove parallel-path byte-identity at small scale; V7/V8 prove it at production scale + confirm the ~2× / RAM-table numbers.

## 14. Outcome — parallel REMOVED, mimalloc kept (2026-05-31)

The feature was implemented, verified **byte-identical** (N-invariant, == Perl on real data), then **removed after three independent measurements proved it anti-scales**. Only the mimalloc allocator change was kept. End state = merged #893 + mimalloc; parse/aggregate stays single-threaded.

**Gate #1 (system allocator, full `--CX`, 837,741,418 cov rows).** All `--parallel 1/3/6` decompressed-byte-identical to Perl v0.25.1. Wall: Perl 3741s; rust **p1=973s, p3=1790s, p6=1508s** — parallel SLOWER than sequential. maxRSS 27.7/35.7/45.7 GB.

**Gate #2 (mimalloc, reused Perl baseline).** Same byte-identity (all PASS). Wall: **p1=854s, p3=1382s, p6=1125s**. mimalloc helped every run (−12/−23/−25%) but `p1` (sequential) is still fastest — anti-scaling persists.

**Controlled experiment (parse-only, mimalloc, interleaved ×3 + live thread sampling).** `p1` ≈ 650s @ **99% CPU** (1 core, fully busy); `p6` ≈ 896s @ **145% CPU** (~1.45 cores) — slower, **reproducible across 3 reps with load-before 5.7–10.6** ⇒ NOT external contention (the originally-raised hypothesis, ruled out). Live sample: the `p6` run degenerates to the lone CHH thread at 60% CPU. cgroup check: cpuset `0–127`, no quota ⇒ not core-throttled. **Mechanism:** (1) CHH-file load imbalance caps effective parallelism at ~2×; (2) concurrent multi-GB map builds contend on shared **memory bandwidth**, so the overlap is slower than doing them sequentially. Allocator-independent (mimalloc didn't fix) and single-address-space-bound (Family B / sharding wouldn't fix either). **The read phase is memory-bound — threads are the wrong lever.**

**Decision (Felix, 2026-05-31): remove `--parallel`, keep mimalloc.** Reverted `parse_files`/`merge_from`/`--parallel`/`BadParallel` + their tests + the parallel docs/harness changes; kept only the mimalloc `#[global_allocator]` + dep (a free ~12% sequential win, byte-identical). `bismark2bedGraph_rs` is sequential like #893, plus mimalloc. The remaining lever, if `--CX` speed ever matters more, is **algorithmic** (cache-friendlier aggregation — e.g. sort-then-tally to make inserts sequential), not parallelism — deferred. The `merge_from` design + this investigation remain on record here for any future streaming/external-spill work.

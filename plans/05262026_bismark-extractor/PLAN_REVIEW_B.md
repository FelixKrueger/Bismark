# Phase E Plan Review — Reviewer B

**Plan reviewed:** `plans/05262026_bismark-extractor/PHASE_E_PLAN.md` (rev 0, 2026-05-27)
**Reviewer:** B (independent of A)
**Date:** 2026-05-27
**Verdict:** **APPROVE WITH CRITICAL FIXES** — one byte-identity-blocking divergence from Perl in yacht's `read_end` derivation (see Critical-1), plus several Important assumptions that need locking before code lands.

---

## 1. Logic review

### Strengths

- The mode dispatch design is clean: a single `mode_keys(mode, basename, gzip)` builder feeds eager-open and routing. Phase B's `route_call` → `OutputFileMap::write_call` flow is preserved verbatim with one extra `match` arm per call (`mode`).
- Phase B's `mbias_only` short-circuit was pre-wired with foresight; Phase E correctly flips a single boolean in `ExtractState::new` to unlock it. No new short-circuit logic.
- Cleanup-on-error inherits Phase B's `HashMap`-iterating implementation untouched, automatically generalising to any mode's key set (including the empty `MbiasOnly` set).
- `flate2` choice is justified (plain gzip, not BGZF) and pinned to the version already in the transitive graph from `noodles_bgzf` — avoids dep duplication, a real foot-gun in cargo workspaces.
- The plan correctly distinguishes "Phase A already rejects this combo" from "Phase E adds defensive handling" and points to the exact Perl lines (`:1037-1038`, `:1328-1336`).

### Logic gaps & divergences

#### Critical-1 — Yacht's `read_end` differs from Perl on reverse-strand reads.

Plan §4.2 specifies:
> `read_end = record.cigar().reference_end(read_start)` (1-based inclusive last ref position; from Phase B's `bismark-io::CigarExt`).

This is computed **unconditionally** in `route_call` (§5.3 pseudocode: `let record_end = record.cigar().reference_end(record_start as usize) as u32;`).

But Perl `bismark_methylation_extractor:4350, 4382-4384, 4403-4409`:
```perl
my $end = $start;   # line 4350 — initialised
if ($yacht and $strand eq '+') {  # ONLY adjusts $end for forward reads
    $end += $1 - 1;               # (linear CIGAR)
    # or
    $end += $MDN_count - 1;       # (indel CIGAR)
}
```

For `$strand eq '-'`, `$end` **stays equal to `$start`** (initialised at 4350, never re-assigned). Meanwhile `$start` itself gets adjusted for `-` reads at lines 4422-4447 (`$start += $MDN_count - 1`). By the time yacht prints `($start, $end)` for `-` reads:
- `$start` = read's 3'-most genomic position (largest)
- `$end` = read's original 5' position (smallest)

So the Perl yacht emit order for `-` reads is `(large, small)`, not `(small, large)`. The plan's `(record_start, reference_end)` is `(small, large)` on both strands.

**Why this matters:** Phase H's byte-identity gate against Perl will fail on every yacht run with at least one OB/CTOT read. The downstream `NOMe_filtering` consumer reads these columns and would mis-classify fragment orientation if we silently fix it.

**Recommended fix:**
1. Document this as the Perl behaviour (cite `:4350, 4382, 4403, 4422`).
2. Add yacht-specific start/end computation in `route_call` that mirrors Perl: `record_start_emit` and `record_end_emit` switch by `strand`:
   - OT/CTOB (`+`): `start_emit = record.alignment_start()`; `end_emit = reference_end()`.
   - OB/CTOT (`-`): `start_emit = reference_end()` (the bismark-adjusted "start"); `end_emit = record.alignment_start()` (the genomic 5').
3. Test fixture: a yacht OB read where `reference_end != alignment_start`, asserting column 6 > column 7. Without this test, the divergence ships silently.

#### Critical-2 — `read_orientation` value source (review prompt #3 verified — partial).

Prompt asked: is yacht col-8 `read_orientation` from `pair_strand` or SAM flag bit 16?

Verified from Perl `:4472, 4485, 4498, 4511, 4524, 4537`: yacht passes `$strand` literally as col-8. `$strand` here is the bismark-classified pair_strand ('+' or '-' string), **not** the SAM flag bit 16. The plan's claim ("`+` for OT|CTOB, `-` for OB|CTOT") matches Perl exactly.

**However:** the plan's §4.2 says `read_orientation = + for forward-class pair_strand (OT|CTOB)`. This is correct, but the plan should explicitly state that on SE-only (which yacht enforces) `pair_strand == record_strand`, so the source variable is unambiguous. The plan already says this — good — but Phase F's eventual multicore yacht support (if ever) would need to revisit. **Optional follow-up:** add a `#[doc]` note on `format_yacht_row` clarifying this never receives a different value than `record_strand` because yacht is SE-only.

#### Critical-3 — Counter increment under `--mbias_only`.

Plan §2 row "Per-context counters in splitting report" says:
> Counts still accumulate per `(context, methylated)` regardless of output mode.

Cross-checking Perl `:2949-2974` and `:4470-4546`: counters (`$counting{total_meCHG_count}++` etc.) ARE incremented **before** the `print ... unless($mbias_only)` guard. So Perl agrees with the plan: counters always accumulate. ✓

But there's a subtle one: Perl `:4470` `total_meCHG_count++` is in the `($full and $merge_non_CpG)` branch (4453+); the `--mbias_only` short-circuit at the route-level in Rust runs **before** mode dispatch. The plan's §5.3 shows `route_call` does:
1. M-bias accumulation
2. Counter increment
3. mbias_only short-circuit
4. Mode-dispatch + write

This order is correct (counters before short-circuit). **But the plan doesn't show step (2) explicitly in the §5.3 pseudocode.** It says "(unchanged)" — I have to trust Phase B got it right. Add a one-line assertion test: "splitting-report counts match across `--mbias_only` and `--no_split_files` (logical no-op) modes."

Actually checking: there's no `--no_split_files` flag. Test should compare `--mbias_only` vs Default mode on the same input — counters should be identical.

#### Important-1 — `mbias_only` dual-tracking (review prompt #2).

Prompt asks: keeping `config.output_mode == OutputMode::MbiasOnly` AND `state.mbias_only` in sync is error-prone.

The plan derives `state.mbias_only` from `config.output_mode == OutputMode::MbiasOnly` at `ExtractState::new` (line 362 §5.6). Once derived, the bool is the only flag read by hot-path code. This is a one-shot derivation — no ongoing sync needed.

**However:** the plan also independently derives `mbias_only_silence = config.output_mode == OutputMode::MbiasOnly` in `pipeline.rs::extract_se/pe` (§5.5, line 351, 355). And `OutputFileMap::new` independently consults `config.mbias_only` (§4.4 step 2). That's **three independent derivations** of the same predicate.

If anyone ever introduces a second `OutputMode` value that should also suppress per-context writes (unlikely, but e.g. a future `--bedGraph --no-context-split`), updating one site and missing another is a silent-divergence trap.

**Recommendation (Important):** centralise the predicate as a single method:
```rust
impl ResolvedConfig {
    pub fn is_mbias_only(&self) -> bool { self.output_mode == OutputMode::MbiasOnly }
}
```
Then call `config.is_mbias_only()` at all three sites. Trivial; reduces drift risk to zero. The plan doesn't mention this; suggest adding to §5.5/5.6.

#### Important-2 — `Box<dyn Write + Send>` rationale (review prompt #4).

Prompt asks: why `Send` at Phase E (parallel=1)?

Plan §4.3 specifies `Box<dyn Write + Send>` but doesn't explain `Send`. §11 mentions Phase F (multicore) will need per-worker maps. Phase F could trivially add `Send` then — there's no current need.

**However:** if Phase F's design is "each worker owns its own `OutputFileMap`", then writers are never shared across threads and `Send` on the boxed writer is moot (the whole map is `Send` because `BufWriter<Box<dyn Write>>: Send` iff inner is `Send`, but if no thread-crossing happens, `Send` isn't required even then). Conversely if Phase F's design is "a single map, multiple producers via channel", you'd put the map behind a `Mutex` and the channel does the `Send`, not the writer.

So `+ Send` is genuinely speculative. **Two valid choices:**
(a) Drop `+ Send` now; Phase F adds it back if needed. Cleaner, smaller bound.
(b) Keep `+ Send` and document it as forward-looking for Phase F.

The plan picks (b) implicitly without comment. **Recommendation (Important):** add a one-sentence justification in §2 ("`+ Send` bound is forward-looking for Phase F's per-worker map model; trivially satisfied by `File` and `GzEncoder<File>`"). Or drop it.

Either is defensible; the plan should pick a side explicitly. Mind the review prompt's hint: "if forward-looking, `Send` could be dropped from the bound and added in Phase F" — I'd lean toward dropping now, since YAGNI.

#### Important-3 — Vec vs HashMap for `mode_keys` ordering (review prompt #5).

Plan §5.1 returns `Vec<(OutputKey, String)>`. The plan does not call out that file open order matters for test determinism (test snapshot of "first file opened", error-message ordering, etc.).

**Verified:** Perl opens files in a fixed source-code order (`:5085 CpG_context_`, then `:5109 Non_CpG_context_`, etc.). Tests asserting filename presence are order-agnostic, but tests asserting **error message content** on a partial-failure (e.g. "failed at file 3 of 8") would depend on iteration order.

`Vec` is the right call. **Recommendation (Important):** §5.1 docstring should say "ORDER MATTERS: files are opened in this order; an error mid-eager-open leaves the prior files orphaned for `cleanup_partial_outputs`." And the documented order should match Perl's `:5082-5403` reading order, for byte-identity diagnostics.

#### Important-4 — Test coverage of `--merge_non_CpG` x methylation polarity (review prompt #6).

Prompt asks: are `x`/`h` (unmethylated) and `X`/`H` (methylated) **both** tested for `Non_CpG` routing under MergeNonCpG?

Reviewing §7.1: test `output_file_map_merge_non_cpg_routes_chg_to_non_cpg` says "CHG-meth call lands in `Non_CpG_OT_x.txt` (not `CHG_OT_x.txt`)." That's `X` only.

**Gap:** there's no parallel test for `x` (unmethylated CHG), `H` (methylated CHH), or `h` (unmethylated CHH) under MergeNonCpG. All four should route to `Non_CpG_*`; if only one is tested, a routing bug that special-cases methylated-only could ship.

**Recommendation (Important):** expand §7.1 to four parametric tests or one parameterised test covering `{x, X, h, H}` × MergeNonCpG → `Non_CpG_*`. Cheap addition; closes a real coverage gap.

#### Important-5 — XM bytes `U`/`u`/`.` interaction with `mbias_only_silence` (review prompt #7).

Plan §4.5 says only `InvalidXmByte` is silenced under `mbias_only_silence`. Reviewing Perl `:2969-2972, 3051-3054`:
```perl
elsif ($methylation_calls[$index] eq '.'){}                     # noop, no count
elsif (lc$methylation_calls[$index] eq 'u'){}                   # noop, no count
else{
    die "...unrecognised character: $..." unless ($mbias_only); # InvalidXmByte
}
```

Confirmed: `.`, `u`, `U` are unconditionally no-op (not error-eligible). Only the fall-through `else` is gated by `unless ($mbias_only)`. The plan's claim is correct.

**However:** the plan's §4.5 pseudocode is:
```rust
Ok(XmClassification::SkipUnknownContext | XmClassification::SkipNonCytosine) => {}
Err(e) if mbias_only_silence => {}
Err(e) => return Err(e),
```

This is sound iff `classify_xm_byte` already returns `Skip*` variants for `.`, `u`, `U` (which it should from Phase B). **Recommendation (Important):** add an explicit test that with `--mbias_only` AND a normal XM containing `.` AND `u`, the call count and M-bias matrix are identical to a non-`--mbias_only` run on the same input. This verifies the silencing only affects InvalidXmByte, not the `Skip*` paths.

#### Important-6 — `cleanup_all` mid-write panic safety (review prompt #1).

Prompt asks: if a panic occurs between `write` and `cleanup_all`, can junk `.gz` files leak?

Plan §R2 claim: "cleanup_all removes the .gz files entirely." Verified — `cleanup_all` calls `std::fs::remove_file` per entry. So **if cleanup_all runs**, junk is gone.

**But the prompt's real concern is panics:** Rust panics unwind by default. The `BufWriter<Box<GzEncoder<File>>>` chain has Drop impls that flush; on panic, drops still run (unwind), but `Drop` impls swallow errors silently. The plan does NOT guarantee `cleanup_all` is called on panic — that requires either a panic hook or `catch_unwind`.

**Reality check:** If the binary panics, the OS will clean nothing automatically. Partial `.gz` files will remain on disk. The plan's R2 only addresses **clean error paths** (where `cleanup_all` is invoked explicitly by `main.rs::run`'s error handler).

**Recommendation (Important):** add to §4.6 edge cases: "Panic during write → `.gz` files left on disk in possibly-truncated state. Acceptable: panics are bugs; Phase H byte-identity gate skips panic scenarios. Document in `OutputFileMap::write_call` docstring." Alternatively, install a panic hook in `main.rs::run` that invokes cleanup; that's heavier than warranted at Phase E. Either way, the plan should acknowledge this gap explicitly so it doesn't read as if R2 covers panics.

#### Optional — Crate version bump (review prompt #8).

`1.0.0-alpha.4 → 1.0.0-alpha.5`. Sanity-check: alpha series, mid-port, no API stabilisation yet. Bumping to `beta.1` would prematurely signal API stability. `alpha.5` is correct. ✓

#### Optional — Phase F merge model (review prompt #9).

Prompt asks: does `Box<dyn Write + Send>` actually help Phase F's per-worker merge?

Per Phase F's eventual design (Phase E §11): per-worker `OutputFileMap`s that "merge at finalize". If each worker owns its own files (per-worker temp files merged via concat at the end), each worker's writer is `Box<File>` or `Box<GzEncoder<File>>` and `Send` is needed to *move* the worker's `OutputFileMap` into a join-handle return value, not to share the writer across threads.

So `Send` will help Phase F move the **OutputFileMap value** between threads at join time. The inner `Box<dyn Write + Send>` is then over-constrained — `Box<dyn Write>` would suffice if `OutputFileMap: Send` is achieved by deriving on the outer struct (which requires inner Send-able, but Box<dyn Write> alone isn't Send unless we add `+ Send`).

So `+ Send` **is** the right way to satisfy Phase F's move-at-join, and the plan's choice is forward-looking-correct. Drop my Important-2 partially: keep `+ Send`, but **add the §2 justification** ("required for Phase F's per-worker OutputFileMap move at join").

#### Optional — Gzip byte-identity test (review prompt #10).

Plan test `output_file_map_gzip_writes_valid_gz_content` says:
> decompress the file via `GzDecoder` → assert content matches the plain-mode equivalent byte-for-byte.

Verified ✓. Strong test — catches compression-level drift, footer bugs, header timestamp non-determinism.

**Minor caveat:** `flate2::write::GzEncoder::default()` writes the gzip header with `os = 0xFF` (unknown) and no mtime by default. Should be byte-deterministic on the **decompressed** content but not on the compressed bytes. The test asserts decompressed content match — that's the right assertion. Good.

---

## 2. Assumptions analysis

### Implicit assumptions (need to surface)

1. **Yacht `read_end` formula uniform across strands** — incorrect, see Critical-1. Plan implicitly assumes the formula is strand-agnostic.
2. **`Box<dyn Write + Send>` overhead is amortized by BufWriter** — stated in §4.3, §8; not measured at Phase E. The 8 KiB BufWriter at ~50 bytes/row gives ~160 rows/syscall, ~160 vtable hops per syscall. Plausible but unverified. Acceptable risk.
3. **`flate2 = "=1.0.34"` is transitively present from `noodles_bgzf`** — claimed in §9.1; should be verified before committing the dep (`cargo tree | grep flate2`). If `noodles_bgzf` already pins a different version, the workspace pin will conflict.
4. **`GzEncoder` Drop semantics flush the footer correctly when `BufWriter` Drop calls inner flush** — true per `flate2` docs (>=1.0.18), but the call chain `BufWriter::drop → flush → GzEncoder::write_all(empty) → footer-on-drop` is subtle. The test `output_file_map_gzip_writes_valid_gz_content` catches this if you actually let the writer drop before reading. Make sure the test takes the writer out of the map first or relies on `OutputFileMap` going out of scope before decoding.
5. **`mode_keys` Vec order is documented** — implied, not stated. See Important-3.

### Stated assumptions that hold

- `--mbias_only` skips eager-open: verified Perl `:5094, 5097, 5118, 5121, 5148, 5151, 5342, 5345, 5366, 5369, 5390, 5393, 5418, 5421` — every `open(...)` in the per-context paths has `unless($mbias_only)`. ✓
- `mbias_only_silence` is plain-fallthrough on InvalidXmByte: verified `:2972, 3054`. ✓
- `_context_` infix in Comprehensive mode: verified `:5085, 5109` (CMNCpG), `:5333, 5357, 5381` (Comprehensive). SPEC fix is real and needed. ✓
- Yacht 8-col format: verified `:4472, 4485, 4498, 4511, 4524, 4537`. ✓

---

## 3. Efficiency analysis

The plan's §8 efficiency notes are accurate. Three minor refinements:

1. **`format_yacht_row` allocation** — §8 notes one `Vec<u8>` per call. Worth quantifying: at ~80 bytes/row, 55M reads × 5 calls/read = ~22 GB allocation pressure over a run. Even with allocator pooling that's noticeable. **Recommendation (Optional):** consider `write!` to the `BufWriter` directly instead of allocating a `Vec<u8>` then calling `write_all`. The plan's signature `format_yacht_row(...) -> Vec<u8>` precludes this. Could change to `fn write_yacht_row(writer: &mut impl Write, ...) -> io::Result<()>` for zero-alloc. Phase F profiling concern; flag now.

2. **`record.cigar().reference_end(record_start)` per call** — §R3 already flagged this. Note: `route_call` is invoked once **per call** (per cytosine), not per record. So `reference_end` runs O(calls × cigar_ops). For yacht specifically (where this is needed), caching once per record at `extract_calls` time and threading through is a 5-10× speedup of the yacht path. Plan acknowledges as Phase F concern; reasonable to defer.

3. **`Box<dyn Write>` overhead** — §4.3 amortization claim is correct but lacks measurement. Phase F profiling should compare static-dispatch (`enum Either<File, GzEncoder<File>>`) vs `Box<dyn>` once parallel=N is in play (multicore I/O changes the cost balance).

---

## 4. Alternatives

### Alt-1: Skip the `Box<dyn Write + Send>` design entirely; use a typed enum.

```rust
enum FileWriter {
    Plain(BufWriter<File>),
    Gzip(BufWriter<GzEncoder<File>>),
}
impl Write for FileWriter { ... }
```

Trade-off: ~30 lines of impl boilerplate vs no vtable cost. Static dispatch wins at >1M writes; vtable amortization wins on code clarity. The plan picks `Box<dyn>` for simplicity — defensible at Phase E parallel=1. **No change recommended;** revisit in Phase F if profiling shows it.

### Alt-2: Build the filename derivation as a single `match` rather than `mode_keys` returning a `Vec`.

Plan's `mode_keys(...) -> Vec<(OutputKey, String)>` is fine but allocates the Vec on every `OutputFileMap::new`. Alternative: an iterator-returning function or a const-array per mode. Saves one heap alloc per binary run. Trivial; not worth the cognitive overhead. **No change recommended.**

### Alt-3: Treat yacht as a totally separate code path.

Yacht's 8-col rows and strand-conditional `end` derivation make it the odd-mode-out. The plan integrates yacht into the unified `write_call` signature with two extra args. Alternative: a separate `write_yacht_row` entry point invoked only from yacht-mode dispatch. Trade-off: cleaner `write_call` for non-yacht (no dead args); two write entry points to maintain.

Given Critical-1's strand-conditional `end` derivation makes yacht's row-format logic genuinely more complex than other modes, **mildly recommend** splitting `write_call` into:
- `write_meth_row` (5-col, all non-yacht modes)
- `write_yacht_row` (8-col, yacht only)

The mode dispatch in `route_call` picks the right one. Avoids passing two yacht-only args to all modes. Optional refactor.

### Alt-4: Move SPEC fix to a separate PR.

Plan §16 rolls the SPEC §4.1 `CpG_*` → `CpG_context_*` fix into the same PR. Per Phase D's convention this is fine. Alternative: a separate doc-only PR upstream. Not worth the overhead. **No change recommended.**

---

## 5. Validation sufficiency

The plan proposes ~25 unit + 8 smoke tests. Coverage gaps:

| Gap | Severity | Suggested test |
|-----|----------|----------------|
| Yacht reverse-strand `end < start` ordering (Critical-1) | **Critical** | `format_yacht_row_reverse_strand_swaps_start_end` — OB read, asserts col-6 > col-7. |
| MergeNonCpG × `x`/`H`/`h` (only `X` is tested) | Important | Parametric `merge_non_cpg_routes_{x,X,h,H}_to_non_cpg`. |
| `--mbias_only` × `.`/`u` XM bytes (not just invalid) | Important | `mbias_only_preserves_skip_paths_for_dot_and_u`. |
| `--mbias_only` counter equivalence vs Default mode | Important | `splitting_report_counts_match_mbias_only_vs_default`. |
| `--gzip` × `--no_header` (no header line in .gz) | Optional | Already implicit in existing tests if `no_header` is parameterised. |
| `--gzip` × empty BAM (decompressing produces just header) | Optional | Edge case already in §4.6 but no explicit test listed. |
| `flate2` version pin consistency | Optional | `cargo tree | grep flate2 | wc -l == 1` in CI, not a unit test. |
| `Box<dyn Write + Send>` actually `Send` (compile check) | Optional | `fn assert_send<T: Send>() {}; assert_send::<OutputFileMap>();` — Phase F insurance. |

The plan's regression coverage (151 existing tests + new) is solid. The gaps above are mostly Importants tied to specific edge-case behaviours rather than missing categories.

---

## 6. Action items (prioritized)

### Critical (must address before implementation trigger)

- **C1 — Yacht `read_end` strand divergence.** Plan §4.2/§5.3 unconditionally use `reference_end(record_start)`. Perl only adjusts `$end` for `+` reads; for `-` reads, the printed `($start, $end)` is `(corrected_3prime, original_5prime)`. Must mirror Perl bytewise or document deliberate divergence with downstream impact assessment. Add a reverse-strand yacht test. (Plan §4.2, §5.3, §7.1 yacht tests, §9.1 assumptions list.)

### Important (should address; defer only with explicit acknowledgement)

- **I1 — Centralise `is_mbias_only()` predicate** as a `ResolvedConfig` method. Currently derived in three independent sites (§4.4 step 2, §5.5, §5.6). One change-site instead of three.
- **I2 — Justify `+ Send` bound explicitly.** Either drop it (YAGNI at Phase E parallel=1) or add a §2 line citing Phase F's per-worker OutputFileMap move-at-join requirement.
- **I3 — Document `mode_keys` Vec ordering.** §5.1 docstring should specify file-open order matches Perl `:5082-5403` reading order. Affects cleanup-on-error file deletion order and any future test snapshots.
- **I4 — Expand MergeNonCpG routing tests** to cover all four polarity × context pairs (`X`/`x`/`H`/`h`), not just `X`. Closes silent-routing-bug risk.
- **I5 — Add `mbias_only` counter-equivalence test** (Default mode counters == MbiasOnly mode counters on the same input) — verifies Critical-3's order-of-operations.
- **I6 — Add panic-safety acknowledgement** in §4.6: panics leak partial `.gz` files (cleanup_all only runs on clean error paths). Either accept and document, or install a panic hook.
- **I7 — Verify `flate2 = =1.0.34` actually deduplicates** in the workspace dep graph before adding (run `cargo tree | grep flate2` against current workspace).

### Optional (nice-to-have; defer freely)

- **O1 — Add `mbias_only` × `.`/`u` skip-path preservation test.** Demonstrates `mbias_only_silence` doesn't accidentally short-circuit unrelated skip paths.
- **O2 — Consider `write_yacht_row(writer: &mut impl Write, ...)` instead of `Vec<u8>` allocation.** ~22 GB allocation pressure on full-cohort runs. Phase F concern.
- **O3 — Compile-time `Send` assertion** for `OutputFileMap` (one-line guard against future regressions).
- **O4 — Split `write_call` into `write_meth_row` + `write_yacht_row`** if Critical-1's fix makes the unified signature ugly with yacht-only args.
- **O5 — Add Phase F note** in §11: `is_paired` field stays as `ExtractState` field (already locked in §2), but a brief note that per-worker `ExtractState`s in Phase F all share the same `is_paired` value (it's a config property) so reducer logic is trivial.

---

## 7. Summary

The plan is well-structured, correctly identifies all six output modes' filename topology, locks the right design decisions (mode-aware key set, eager-open dispatch, `flate2` for plain gzip), and threads the deferred `mbias_only_silence` kernel param through with clear Perl citations. Phase B's pre-wiring discipline pays off here — Phase E is mostly "flip the bit and add the test coverage."

**The single blocker is Critical-1** — yacht's `read_end` formula diverges from Perl on reverse-strand reads, which Phase H's byte-identity gate will catch but the plan doesn't acknowledge. Fix the formula, add the reverse-strand test, and the plan is implementation-ready.

The Important items are mostly hygiene: predicate centralisation, ordering docstrings, test coverage parity. None block implementation but the plan benefits visibly from each.

**Verdict:** APPROVE WITH CRITICAL FIX — address C1 before implementation trigger; consider I1-I7 inline with implementation.

---

## File path

`/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PLAN_REVIEW_B.md`

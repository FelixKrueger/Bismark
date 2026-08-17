# Plan: 5-Base simplex consensus output (#1104)

**Issue:** [#1104](https://github.com/FelixKrueger/Bismark/issues/1104) · requested in [#1095](https://github.com/FelixKrueger/Bismark/issues/1095) by @Danielsm8 (cfDNA, coverage-recovery case)
**Revision:** 1 (2026-08-14)

## Revision history

- **rev 0** (2026-08-14): initial plan after design decisions with Felix (separate BAM + `mx` tag; `--five_base_emit_multiplicity` enum; emit all family sizes).
- **rev 1** (2026-08-14): after dual plan review (A and B, both REQUEST CHANGES — `PLAN_REVIEW_A.md` / `PLAN_REVIEW_B.md`). Changes: **duplex emission order becomes deterministic (Felix's decision)** — resolves the flaky byte-gate both reviewers flagged (their C1); histogram re-based on **read counts** (their C2 — `/2` broke under per-mate MAPQ filtering); `mx` mechanism corrected to a `BismarkRecord` setter (`data_mut()` does not exist); `--five_base_bisulfite_bam` combination explicitly rejected (dispatch precedes `resolve()`); absent-key and double-emit paths fail loud via a unit-testable eviction helper; `simplex` mode skips storing paired families; `EmitPaths` enum replaces the two-`Option` signature; stderr contract pinned verbatim for the default; fact 2's rationale strengthened (contamination, not just emptiness); XM asserted for both simplex orientations; FR-pair and UMI-canonicalization assumptions surfaced; counts derived in one sweep; CKey size corrected to 24 B.

## 1. Goal

`--five_base_consensus` collapses only duplex families (both molecule strands seen) and discards simplex families (one strand seen) at `mod.rs:2519-2521`. Emit those simplex families too, as consensus reads kept distinguishable from duplex:

- New flag `--five_base_emit_multiplicity {duplex, simplex, both}` (default `duplex` = today's behaviour).
- Simplex consensus goes to a **separate BAM** (`*_pe.5base_simplex.bam` in-run; `five_base_simplex.bam` standalone).
- In `simplex`/`both` modes, **every** consensus record (both files) carries `mx:i:2` (duplex) / `mx:i:1` (simplex), so the distinction survives a user's own `samtools merge`.
- The run report gains simplex counts and a family-size histogram (read counts).
- **Prerequisite (step 0): duplex consensus emission order becomes deterministic.** Today it follows `std` HashMap iteration — random per process — so no byte-gate on this output was ever well-defined. Sorting family keys fixes that for this plan's regression gate and every future one. **Default output order therefore changes vs 3.1.0** (record *content* is unchanged); disclosed in the CHANGELOG.

Decisions fixed with Felix: 2026-08-14 (a) separate BAM **and** tag; (b) enum flag (DRAGEN-shaped); (c) emit **all** simplex families including single-read ones, sizes reported; (d) **deterministic emission order** rather than sorted-comparison-forever.

## 2. Context

All changes live in `rust/bismark/src/aligner/` plus one setter in `rust/bismark/src/io/record.rs`:

| File | Role |
|---|---|
| `mod.rs:2349-2676` `run_five_base_consensus` | The two-pass collapse. PASS 1 (`:2483-2505`) counts OT/OB reads per `CKey`, builds `paired = {ot>0 && ob>0}`, drops counts. PASS 2 (`:2508-2563`) stores `Member` covered-maps for paired families only. Emission (`:2569-2665`) iterates `fams.values()` (**random order — see step 0**), builds `cons_seq` per position via `consensus_base`, emits **two** records per family (FLAG `0`/`0x10`) through `five_base_emit_record` |
| `mod.rs:1803-1830` | In-run call site; derives `_pe.5base_consensus.bam` |
| `mod.rs:531-602` `run_five_base_consensus_standalone` | `--five_base_consensus_from_bam` entry; PE-probe, writes `<out_dir>/five_base_consensus.bam` |
| `mod.rs:165-185` | Standalone dispatch — runs **before** `resolve()`, so config-level flag checks never fire on these paths (drives step 2's placement) |
| `five_base_duplex.rs:355-371` `consensus_base` | Handles an absent opposite strand: own present + `opp = None` → own base (the `_` arm); own absent → `N` |
| `five_base_duplex.rs:330-344` `reconcile_generic` | `(Some(x), None) → x` — non-CpG positions need no new rule |
| `io/record.rs:181-230` `BismarkRecord` | `inner()` + targeted setters (`set_umi`, `set_rx`); **no public `data_mut()`** — the `mx` tag needs a new setter here (step 6) |
| `cli.rs:134-186` | The `five_base_*` flag group |

**Facts the design derives from (verified by both reviewers against source):**

1. **No new calling rule.** `consensus_base` and `reconcile_generic` already return the correct simplex result when one strand is `None`; the variant mask never fires — that *is* the (documented) confidence difference.
2. **One record per simplex family, on the molecule's own strand — because the other record would be contaminated, not merely empty.** For an OT-only family every `−`-strand CpG reconciles to `N` (no calls), **and** at non-CpG genomic-G positions `reconcile_generic (Some, None)` returns the *top-strand* base — a FLAG `0x10`/GA record built from it would emit systematically wrong "unmethylated" CHG/CHH calls for a strand never sequenced. Symmetrically for OB-only forward records. OT family → FLAG `0` only; OB → `0x10` only, reusing the existing revcomp/reverse-qual handoff (`mod.rs:2623-2629`; `five_base_emit_record` maps `0x10 →` OB/GA at `mod.rs:1679`). This rationale goes in the `emit_family` doc comment so a future "emit both for symmetry" change cannot silently reintroduce the contamination.
3. **Both mates of an FR proper pair land in one family** (`molecule_is_ot = (0x40 set) == coverage-forward`, `mod.rs:2459`, all four FLAG combos check out; `CKey` is mate-symmetric via `min(pos, mate-pos)` + `|TLEN|`). **But the minimum simplex family is 1 read, not one pair:** `key_of`'s MAPQ gate (`mod.rs:2431`) is per-record while 0x2/TLEN are pair-level, so `--five_base_min_mapq` (the DRAGEN-parity setting) can orphan one mate → odd-sized families. Collapse is unaffected; the *report* must be (histogram on read counts, §3.7).
4. **Memory is the real design problem.** `CKey` (24 B aligned) count maps reach ~0.7–1 GB at 20M-family WGS depth; PASS 2 is bounded *because* it stores only paired families. Fix: **emit-and-evict** on PASS 1's exact counts (§5 step 5).

Dependencies: none new. Reuses `five_base_emit_record`, `write_record`, `crate::io::BamWriter`, `canonical_umi`, the existing `key_of` closure.

## 3. Behavior

1. **Step 0 — deterministic duplex emission.** Before the emit loop, collect `fams` keys into a `Vec` and sort by `(ref_id, start, end, umi_hash)`; iterate in that order. Content per family unchanged. Two-records-per-family order (FLAG `0` then `0x10`) already fixed. Simplex emission (step 5) is inherently deterministic: eviction fires on each family's last arriving record, so simplex BAM order = family-completion order in the input stream.
2. **CLI.** `--five_base_emit_multiplicity <MODE>`, clap `ValueEnum {duplex, simplex, both}`, default `duplex`. Validation (all three dispatch paths, §5 step 2): requires `--five_base_consensus` or `--five_base_consensus_from_bam`; **explicitly rejected with `--five_base_bisulfite_bam`** (that standalone dispatches at `mod.rs:183` before any config check and would otherwise silently ignore the flag).
3. **Mode `duplex` (default).** Byte-identical to the step-0 baseline: same single BAM, no `mx` tag, **stderr verbatim** — the current summary line (`mod.rs:2670-2674`) and the standalone `Note:` line (`mod.rs:588-592`) print unchanged.
4. **Mode `both`.** Duplex families → existing BAM, two records each, tagged `mx:i:2`. Simplex families → simplex BAM, one record each, `mx:i:1`, QNAME prefix `spx:` (an OT-only and OB-only family at the same span+UMI cannot coexist — they would be paired — so `spx:` qnames cannot self-collide).
5. **Mode `simplex`.** Only the simplex BAM is written (DRAGEN emit-multiplicity semantics; the missing duplex BAM is called out in the flag doc). Duplex families are **counted but not stored**: PASS 2 skips building their covered maps (their counts for the report come from PASS 1).
6. **Same filters.** `key_of` is shared, so `--five_base_min_mapq`, unmapped/secondary/supplementary skips, and pair gates apply identically, and PASS 1 counts equal PASS 2 arrivals by construction.
7. **Report.** In simplex modes the stderr summary extends with: `emitted_spx`, `skipped_spx`, and a family-size histogram over **read counts**, buckets `1, 2, 3, 4, ≥5`, covering **all** simplex families (emitted + skipped). The doc notes 2 reads ≈ 1 fragment. Count identity (validated): `emitted_spx + skipped_spx == simplex_expected.len()`.
8. **Edge cases.**
   - `end <= start` and chromosome-edge (`five_base_emit_record → None`): skip + count, per multiplicity.
   - **Absent key in PASS 2** (in neither `paired` nor `simplex_expected`): `AlignerError::Validation` — input mutated between passes; never an indexing panic.
   - **Residual families at EOF** (arrived < expected): hard error, same rationale.
   - **Post-eviction re-arrival**: a `done: HashSet<CKey>` catches records for already-emitted families → hard error (closes the silent double-emit hole; one bit per emitted family).
   - Empty simplex set in `simplex`/`both`: header-only simplex BAM still created; report says 0.
   - No UMI flags: QNAME UMI field is `NA` (existing convention).

## 4. Signature

```rust
/// Which consensus multiplicities to emit (#1104; DRAGEN --umi-emit-multiplicity shape).
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum EmitMultiplicity { Duplex, Simplex, Both }

/// Output paths — invalid states unrepresentable (no path/mode cross-check needed).
enum EmitPaths<'a> {
    Duplex(&'a Path),
    Simplex(&'a Path),
    Both { duplex: &'a Path, simplex: &'a Path },
}

fn run_five_base_consensus(
    genome: &Genome,
    refid: &HashMap<String, usize>,
    bam_paths: &[&Path],
    outputs: EmitPaths<'_>,
    header: &noodles_sam::Header,
    umi_swap: Option<UmiSwap>,
    min_mapq: u8,
) -> Result<()>
```

New in `crate::io` (`record.rs`, beside `set_rx` at `:224`):

```rust
/// #1104: stamp the consensus-multiplicity tag (mx:i). Lowercase = SAM local space.
pub fn set_mx(&mut self, multiplicity: i32)   // Tag::from(*b"mx"), Value::from(multiplicity)
```

Eviction bookkeeping extracted for unit-testability (step 5's error paths need no production test hooks):

```rust
/// Tracks arrivals against expected counts; yields families ready to emit.
/// Errors: absent key, arrival after emission (done-set), residuals at finish().
struct SimplexLedger { expected: HashMap<CKey, u32>, arrived: HashMap<CKey, Fam>, done: HashSet<CKey> }
```

## 5. Implementation outline

1. **Step 0 (own commit): deterministic duplex emission.** Sort `fams` keys by `(ref_id, start, end, umi_hash)` before the emit loop (`mod.rs:2569`). CHANGELOG sentence: consensus BAM record order is now deterministic (was random per process); record content unchanged. Gate: V1a.
2. **CLI + validation (`cli.rs`, `mod.rs`).** Add `EmitMultiplicity` + flag (default `Duplex`); doc-comment covers the `[#1104]` marker, DRAGEN analogy, `mx` values, the no-variant-check caveat, the `simplex`-mode-has-no-duplex-BAM note, and one sentence distinguishing `mx` from `XM` (case-sensitive, unrelated). Validation at each dispatch path: (a) align run — in the config `five_base_*` cross-checks (`config.rs:706-764`); (b) `--five_base_consensus_from_bam` standalone — satisfied by construction; (c) `--five_base_bisulfite_bam` standalone — **reject** `emit != Duplex` at dispatch (`mod.rs:183`) before anything runs.
3. **PASS 1 (`mod.rs:2483-2506`).** Unchanged counting. At the end, **one sweep** over `counts` derives `paired` and (simplex modes only) `simplex_expected: HashMap<CKey, u32>` (exactly one strand nonzero → `ot+ob`), then `drop(counts)`. Default mode: identical to today, no new map. (Implementer may equivalently `retain`+reuse `counts` — reviewer A's variant; either satisfies the peak-memory bound.)
4. **PASS 2 storage rule.** Store paired families' members **only when duplex is emitted** (`Duplex`/`Both`); in `Simplex` mode the `paired.contains` branch skips storage. Simplex-family records feed the `SimplexLedger`.
5. **Emit-and-evict via `SimplexLedger`.** On each simplex-family record: build `Member` (existing code), push, increment; `arrived == expected` → collapse + emit + move key to `done`. Absent key → error; key in `done` → error; `finish()` with residuals → error (naming counts). Peak memory = in-flight incomplete families — duplication-rate-bound (a multi-fragment family is resident first-to-last member; the dominant single-fragment class evicts on the adjacent mate in Bismark-written PE BAMs).
6. **Shared emission helper.** Extract the per-family body (`mod.rs:2569-2665`) into `emit_family(fam, flags: &[u16], qname_prefix: &str, mx: Option<i32>, writer, ...) -> Result<bool>`. Duplex: `&[0, 0x10]`, `"dpx"`, `Some(2)` in new modes / `None` in default. Simplex: `&[0]` or `&[0x10]` by molecule strand, `"spx"`, `Some(1)`. The tag is applied via the new `BismarkRecord::set_mx` **only when `mx.is_some()`** — the default path never calls it. The doc comment carries fact 2's contamination rationale.
7. **Writers.** Created per `EmitPaths` variant, up-front (empty run ⇒ valid header-only BAM).
8. **Call sites.** In-run (`mod.rs:1820-1830`): derive `_pe.5base_simplex.bam`; standalone (`mod.rs:585`): `five_base_simplex.bam`; build `EmitPaths` per mode. The standalone `Note:` line is extended **only when `emit != Duplex`**.
9. **Report.** Counters `(emitted_dpx, emitted_spx, skipped_dpx, skipped_spx, hist: [u64; 5])` (read-count buckets 1/2/3/4/≥5 over all simplex families). Default mode prints the existing line **verbatim**.
10. **Docs + CHANGELOG.** `illumina-5-base.md`: flag, tag, caveat, and a recommendation to pair simplex mode with `--five_base_deconvolution` (the population-level variant check) — turns the caveat into a workflow. CHANGELOG: one sentence for the feature + one for step 0's order change.
11. **Tests** — §9.

**Declined (from review, with reasons):** per-record family-size tag (B-alt 3) — additive later, no one asked; renaming to `five_base_simplex_consensus.bam` (A-opt 14) — the duplex file's shipped name breaks full symmetry anyway and `simplex` already implies consensus context here.

## 6. Efficiency

- **Default:** zero-cost — no new map (single-sweep derivation only runs in simplex modes), no tag calls, `drop(counts)` kept; step 0's sort is `O(F log F)` over key structs, negligible against the collapse itself.
- **Simplex modes:** PASS 1 peak ≈ today's `counts` plus the derived `simplex_expected` momentarily (~0.7–1 GB at 20M-family WGS; opt-in, documented in the flag doc). PASS 2 in-flight is **duplication-rate-bound**, not merely "incomplete families" — stated in the doc. `done`-set: one `CKey` per emitted simplex family. Two hash lookups per PASS-2 record.
- `Simplex` mode stores no paired-family members (step 4) — without this, simplex mode would pay the full duplex memory bill for zero output.

## 7. Integration

- Simplex records are ordinary single-strand Bismark records (XM/XR/XG via unchanged `five_base_emit_record`) — `bismark extract`, `bam2pat`, `--five_base_bisulfite_bam` consume them as they do duplex consensus reads. Extractor consumers parse only XR/XG/XM (`io/record.rs:116-130`); `mx` is inert provenance.
- The file split is the separation; the tag is the merge-survivor.
- Step 0 changes the shipped duplex BAM's record **order** (content identical) — the only default-visible change, disclosed.
- No change to the `--five_base_duplex` TXT report pass.

## 8. Assumptions

1. "Byte-identical default" is defined **against the step-0 (deterministic) baseline**; V1a separately proves step 0 is order-only. Byte comparisons use the suite's convention: decompressed, whole-@PG-block filtered, same VERSION on both sides.
2. **FR-oriented proper pairs** (what minimap2 `-x sr`/bowtie2 FR mark with 0x2). An FF/RR pair carrying 0x2 would split mates OT/OB — pre-existing duplex-path behaviour, out of scope, now stated.
3. **Mates' UMIs canonicalize equal**: guaranteed for `--five_base_umi_qname` (shared QNAME); for `--five_base_umi_len` it rests on the mates' inline UMIs being revcomp partners (chemistry) — a violation splits a pair into two 1-read families, which the read-count histogram now represents honestly.
4. Mate adjacency in Bismark-written PE BAMs affects only peak memory; eviction keys on exact counts.
5. PASS 1 ≡ PASS 2 record sets (same files, same `key_of`); every violation direction now fails loud (absent key / residual / done-set).
6. `mx` is unused across the suite (verified: AS, CB, MD, NM, RX, UR, XG, XM, XR only) and lowercase = SAM local space.
7. PE-only (`mod.rs:562-568` rejects SE), matching #1104's scope.
8. Concordance-gated like the duplex path; validation is invariant-based.

## 9. Validation

| # | What | How | Expect |
|---|---|---|---|
| 1a | Step 0 is order-only | Same binary twice → byte-identical BAM (was impossible before). Old vs new binary → **sorted** decompressed-SAM comparison identical | Determinism proven; content unchanged |
| 1b | Default-mode gate | Step-0 baseline vs feature binary, flag absent, multi-family fixture (≥2 families or the gate is vacuous) | Identical BAM bytes; stderr verbatim (summary + standalone `Note:`) |
| 2 | Simplex calling at CpGs | Unit tests: OT-only at PlusCpG → own base, MinusCpG → `N`; OB-only symmetric | Matches §2 fact 1-2 |
| 3 | One record, right strand, right XM | Synthetic BAM: one OT-only + one OB-only family, `both` mode | Exactly 2 simplex records: FLAG 0/`spx:`/`mx:i:1` and FLAG 16/`spx:`/`mx:i:1`; **exact XM strings asserted for both orientations** (OB record's calls at `−`-strand CpGs only); no `dpx:` qname in the simplex BAM; duplex records `mx:i:2` |
| 4 | Ledger error paths | Unit tests on `SimplexLedger` (no production hooks): absent key; arrival after done; residual at finish | Each → clean `AlignerError`, never a panic; cross-file family (2 BAMs) emits once, after the second file |
| 5 | MAPQ-orphan family | `min_mapq` set so exactly one mate of a pair is filtered | 1-read family emitted; histogram bucket `1`; XM spot-checked |
| 6 | Single-fragment simplex | One lone proper pair, `both` mode | 1 simplex record; histogram bucket `2`; XM equals the per-read path's calls |
| 7 | Mode matrix, both entry points | `duplex`/`simplex`/`both` through the in-run fixture **and** `--from_bam` on its BAM | File presence per mode; default `Note:` unchanged, non-default extended; count identity `emitted_spx + skipped_spx == simplex_expected.len()` cross-checked against the duplex TXT `singletons` figure |
| 8 | CLI guards | `emit != duplex` without consensus flags; with `--five_base_bisulfite_bam` | Validation errors naming the flag — the bisulfite combination must not be silently ignored |
| 9 | Real-data sanity (oxy, 5-Base PE set) | `both` mode | Duplex records byte-identical to a `duplex`-mode run **modulo the mx tag** (order now deterministic, so this is exact); count identity holds at scale; extractor runs clean over the simplex BAM |

## 10. Questions or ambiguities

- **Resolved with Felix:** separate BAM + tag; enum flag; emit all sizes; **deterministic emission order (2026-08-14)** — default order changes vs 3.1.0, CHANGELOG discloses.
- **Open (non-blocking), taken as specified:** histogram buckets `1/2/3/4/≥5` (read counts) — adjustable in review; declined items listed in §5.

## 11b. Implementation notes (2026-08-15)

Implemented on branch `1104-simplex-consensus` off `dev` (`53744d5`), in two commits as planned.

**Step 0 — `50f1eef`.** Sort `fams` keys by `(ref_id, start, end, umi_hash)` before the duplex emit loop (`mod.rs`), + CHANGELOG sentence + the V1a determinism test.

**Feature — the second commit.** `cli.rs`: `EmitMultiplicity` enum (`clap::ValueEnum`, `as_str()` for notices) + the flag. `config.rs`: struct field, resolve copy, the requires-`--five_base_consensus` guard, test-default. `io/record.rs`: `set_mx()` beside `set_rx()`. `five_base_duplex.rs`: `LedgerError` + `SimplexLedger` + 9 unit tests. `mod.rs`: `EmitPaths`, the bisulfite-standalone rejection, both call sites, PASS 1 one-sweep derivation + read-count histogram, PASS-2 storage rule, emit-and-evict, `emit_family` extraction, per-multiplicity report.

**Deviations from the plan (all minor):**

1. **`emit_family` is a nested `fn`, not a closure**, and takes `genome`/`refid` explicitly — a closure capturing them would have borrowed across the `fams` iteration. Same behaviour; the plan's signature sketch is otherwise as-written.
2. **`SimplexLedger::arrive_with(key, add)` takes a mutator closure** rather than the plan's implied "push then check": PASS 2 must build the `Member` before it knows whether the family completes, and `arrive_with` returns the completed `Fam` by value. Cleaner than the plan's two-step and keeps the ledger generic (`SimplexLedger<K, F>`), which is what made it unit-testable without any consensus machinery.
3. **`config.rs` imports `EmitMultiplicity` directly** (`use crate::aligner::cli::{Cli, EmitMultiplicity}`) — that file imports `Cli` by name, not the `cli` module, so `cli::EmitMultiplicity` did not resolve.
4. **The plan's optional `counts.retain` variant was not used**; the one-sweep derivation (also sanctioned by §5 step 3) is what shipped.

**Added beyond the plan:** a 10th test, `simplex_never_calls_a_strand_it_did_not_sequence`. The sabotage below showed that in the CpG-only fixture geometry the suppressed opposite-strand record is merely *empty*, so the contamination rationale (§2 fact 2) was documented but not exercised. The new fixture carries a standalone non-CpG `G` — the position where `reconcile_generic` passes the own-strand base through — and asserts that column stays a no-call in every simplex record. That converts the central design decision from argued to tested.

**Sabotage log (G19 — a check whose failure has never been observed is not yet a check):**

| # | Sabotage | Result |
|---|---|---|
| 1 | Removed the step-0 sort | `consensus_emission_order_is_deterministic` **FAILED** on the byte-stability assertion. Gate confirmed |
| 2 | Emitted both flags (`[0, 0x10]`) for simplex families | `both_mode_emits_one_own_strand_record_per_simplex_family` **FAILED** on the record count, printing the spurious second record per family. Gate confirmed |

Both restored from a `command cp -f` backup in the scratchpad, never `git checkout --` (N2), and re-verified green afterwards.

**Verification run:** full `cargo test -p bismark` = **1502 unit + all integration suites, 0 failures**; the new suite is 10/10; `aligner_five_base_groundtruth` (7) and `aligner_five_base_bisulfite` (11) unchanged. `cargo clippy -p bismark --all-targets` clean on **default**, `binseq-input`, and `rammap-inprocess`; `cargo fmt -p bismark -- --check` clean.

## 11c. Review response + V9 (2026-08-15)

**Dual code review: APPROVE ×2. Coverage audit: INCOMPLETE — 3 validation-row items, 0 MISSING of 63 ledger items.** Reports: `CODE_REVIEW_A.md`, `CODE_REVIEW_B.md`, `COVERAGE.md`.

**Commit `790e0ae` — review findings.** One real defect, found independently by *both* reviewers, same patch: `SimplexLedger::finish()` inspected only the in-flight map, so a family PASS 1 counted whose records all vanished before PASS 2 created no entry and passed silently. Residual is now `expected.len() - done.len()`. §3.8 had promised this direction; the unit test meant to cover it used families with ≥1 arrival, so the zero-arrival case was never exercised — **a sabotage only tests the gate it is aimed at**, which is why my two sabotages missed it. Also: B proved one gate genuinely vacuous — the `both`-mode `mx:i:2` check was `.all()` over a possibly-empty iterator, and with the PASS-2 storage rule sabotaged to drop every duplex family the *entire suite still passed*; a record-count assertion now precedes it. Plus the histogram's 3/≥5 buckets (no fixture had >2 reads) and a dead docs anchor (`#flags`).

**Commit `2564fa9` — V5/V7 closure.** The in-run entry point had no test in any non-default mode; the new minimap2-gated gate runs `both` in-run and asserts the split plus the `_pe.5base_simplex.bam` suffix, cross-checked against the duplex pass's independent `singletons` count. Sabotage-verified — and worth recording that the *first* sabotage attempt passed because `derive_output_path` takes a primary pattern and a fallback and I had only changed the fallback. V5's XM spot-check and the non-default `Note:` assertion added.

**V9 — reframed and run (oxy, 2026-08-15).** The real 5-Base PE dataset is **not on oxy** (only plan dirs; the NA12878 demo used for the original DRAGEN concordance is gone), so V9 as written is blocked on re-acquiring it. What V9 was actually *for* — the one design claim with no evidence — is the emit-and-evict memory bound at scale, and that is measurable on any Bismark-convention PE BAM. Run over `10M_PE/…deduplicated_all_meth.bam` (6,850,092 records, all properly paired, deduplicated ⇒ singleton-dominated, the exact class the design targets):

| Mode | Peak RSS | Wall | Emitted |
|---|---|---|---|
| `duplex` (baseline) | 3,427,028 kB | 0:18 | 63 duplex |
| `both` | 3,511,828 kB | 1:45 | 63 duplex + **3,424,919 simplex** |

**+83 MB for 3.42M families (~25 B each)** — emit-and-evict holds; storing covered-maps would have been gigabytes. Also verified at scale: the duplex BAM is **byte-identical between the two runs modulo `mx`** (126 records); `mx` occurs **0** times in duplex mode (default-path byte identity at 6.85M records, not just a 2-family fixture); count identity **exact** (3,424,919 emitted + 0 skipped == 3,424,919 families); histogram `2:3424918 4:1`, consistent with deduplicated input. The extractor consumed the simplex BAM clean (exit 0, 7.3 s, 283 MB, all six context files, 3,424,919 call strings → 6.3M CpG calls).

⚠️ **What this run does NOT show.** The input is bisulfite data driven through the 5-Base inverted-polarity path, so the *methylation values are meaningless* (98.6 % CHH). It validates memory, counts, tag placement, byte-identity and downstream consumption at scale — **not** biological correctness, which rests on the synthetic groundtruth gates and the pre-existing DRAGEN concordance. A true 5-Base real-data concordance run still needs the Illumina demo dataset and remains open.

**Still outstanding:** biological V9 on real 5-Base data (dataset not available). Everything else in §9 is committed and green.

## 11. Self-Review (rev 1)

- **Every review action item traced:** A-crit 1-2 / B-crit 1-2 → step 0 + read-count histogram; A-imp 3-8 and B-imp 3-8 → steps 2, 4, 5, 6, 8, 9 + §4's ledger; optionals adopted (EmitPaths, single-sweep derivation, exact count identity, both-entry-point matrix, FR/UMI assumptions, contamination rationale, deconvolution doc note, mx-vs-XM note) or explicitly declined with reasons (§5).
- **Step 0 ordering:** sorting by the full `CKey` (incl. `umi_hash`) is total and deterministic; simplex order needs no sort (completion order is input-determined).
- **Remaining risks:** (a) the OB-orientation slip is now covered by a synthetic exact-XM assertion (V3), not just the oxy spot-check; (b) PASS 1 memory at ultra-deep WGS in simplex modes is documented, not eliminated; (c) step 0 changes shipped output order — content-identity is proven by V1a, but downstream byte-comparisons users built on two runs of 3.1.0 were already impossible, so nothing real breaks.

## 11d. Follow-up disposition (2026-08-17)

The three reviewer optionals carried in the session handoff were checked before acting; two needed nothing:

| Item | Disposition |
|---|---|
| Per-record family-size tag | **Stays declined** — §5 already declined it ("additive later, no one asked", B-alt 3) and nobody has asked since. It would also widen the aux-tag surface in the non-default modes for a figure the report already gives per family-size bucket |
| In-run `both`-mode test for `--five_base_umi_qname` | **Already existed** — `five_base_in_run_both_mode_splits_duplex_and_simplex` (`tests/aligner_five_base_groundtruth.rs:1077`) passes `--five_base_umi_qname` and `--five_base_emit_multiplicity both`, so UMI keying is live while it asserts the family split |
| `debug_assert!(n >= 1)` beside the histogram subtraction | **Applied** (`mod.rs`). The invariant is that a `CKey` only enters `counts` because a record incremented it; without the assert a zero would die on the bucket subtraction naming nothing |

**§3.5-vs-§3.7 resolved in favour of §3.5.** §3.5's promised duplex figure in `simplex` mode was genuinely
absent, and the skip site's comment claimed the families were "counted for the report" when nothing
counted or reported them. The simplex report line now names them (`paired.len()`) **only when no duplex
line is printed**, so `simplex`-mode users get the denominator they lacked and `both` mode does not repeat
what its own duplex line already says. Both directions are asserted, and both assertions were
sabotage-verified.

# Plan Review B — 5-Base simplex consensus output (#1104)

**Plan:** `plans/08142026_5base-simplex-consensus/PLAN.md` (rev 0, 2026-08-14)
**Reviewer:** B (independent, fresh context)
**Verdict:** REQUEST CHANGES — the core design (emit-and-evict, one record per simplex family on its own strand, separate BAM + `mx` tag) is sound and its derivations check out against source, but the plan's central validation gate is flaky as written, and the histogram's read-count/2 assumption breaks on a realistic input class. Several implementation claims need correction before an implementation plan is drafted.

All line references verified against the working tree at `rust/bismark/src/aligner/` (branch `1100-five-base-index-validation`).

---

## 1. Logic review

### 1.1 Fact 1/2 — simplex calling and "one record per family" (VERIFIED, rationale incomplete)

`consensus_base` (`five_base_duplex.rs:355-371`): for an OT-only family, `PlusCpG` hits the `_` arm (own `Some`, opp `None`) → own base carries the call; `MinusCpG` has own = `ob` = `None` → `(b'N', q)`. `reconcile_generic` (`:330-344`) `(Some(x), None) → x`. The derivation is correct, and the OB (`0x10`) handoff correctly reuses the existing revcomp/reverse-qual transform (`mod.rs:2623-2629`) so `five_base_emit_record` sees a read-oriented sequence.

**However, fact 2's rationale understates the case.** "The reverse record would carry zero methylation information" is true only at CpGs (they reconcile to `N` → no call). At **non-CpG minus-strand cytosines** (genomic `G` positions), an OT-only family's `cons_seq` carries the *top-strand* base via `reconcile_generic(Some, None)`, and a `0x10`/GA record would call those positions — systematically wrong (opposite-strand bases masquerading as unmethylated calls in CHG/CHH context). The suppressed record is not empty, it is actively *contaminated*. The plan's decision (emit only the own-strand record) is therefore correct and *more strongly* justified than stated — but the code comment/doc should state the contamination reason, or a future "emit both records for symmetry, the CpGs are just N" change would silently reintroduce it. The same argument symmetrically protects the OB-only forward record.

Consequence for tests: the synthetic test (V3) asserts FLAG + tag only; nothing in the validation table asserts the **XM content of an OB-only simplex record**, which the plan itself names as its top remaining risk (§11a). V5 spot-checks one family's XM but does not say which orientation. Require XM assertions for *both* orientations in V3/V5.

### 1.2 Fact 3 — both mates in one family (VERIFIED for FR pairs; two unstated caveats)

`molecule_is_ot = (flag & 0x40 != 0) == coverage_forward` (`mod.rs:2459`), checked for all four proper-pair FLAG combinations:

| Combo | R1 | R2 | Result |
|---|---|---|---|
| FR, top | 0x40 set, fwd → `true==true` = OT | 0x40 unset, rev → `false==false` = OT | both OT ✓ |
| RF, bottom | 0x40 set, rev → `true==false` = OB | 0x40 unset, fwd → `false==true` = OB | both OB ✓ |
| FF | OT | OB | **mates split** |
| RR | OB | OT | **mates split** |

Both mates also share the `CKey` (`kstart = min(own, mate)`, `kend = kstart + |tlen|` — identical from either mate, `mod.rs:2456-2458`). So fact 3 holds **for FR-oriented proper pairs**, which is what minimap2 `-x sr`/bowtie2 `--fr` mark with 0x2. Two implicit assumptions to surface:

1. **FR orientation is assumed, not checked.** An FF/RR pair carrying 0x2 (never produced by the current aligners, but `--from_bam` accepts arbitrary 5-Base BAMs) would split its mates OT/OB and register a single fragment as a fake *duplex* family. Pre-existing behavior in the duplex path, unchanged by this plan — but the plan's fact 3 states the mate-consistency as unconditional. One sentence in §2 suffices.
2. **Mates' UMIs must canonicalize equal.** For `--five_base_umi_qname` this is guaranteed (both mates share the QNAME). For `--five_base_umi_len`, R1 and R2 each carry their *own* inline prefix in `RX`, and `canonical_umi(_, RevComp)` (`five_base_duplex.rs:59-77`) collapses them only if they are revcomp partners — a chemistry assumption. If violated, each mate forms its own single-*read* family (see 1.3).

### 1.3 Fact/assumption 4 — "fragment count = read count / 2" (BROKEN on a realistic input)

`key_of`'s gates are per-record: 0x2, TLEN≠0, and **MAPQ ≥ min_mapq** (`mod.rs:2431`). The 0x2/TLEN bits are pair-level, but MAPQ is not — mates routinely have different MAPQ, and `--five_base_min_mapq` (DRAGEN's MAPQ<20 precedent, prominent in the flag docs) is exactly the setting a real user runs with. One mate filtered + the other kept ⇒ a family with an **odd read count**, minimum **1**:

- §2 fact 3's "the minimum simplex family is one full read pair (counts = 2)" is false under MAPQ asymmetry (and under the umi_len UMI caveat above).
- §5 step 8's histogram bucket `expected/2` gives **0** for a 1-read family — no bucket exists for it (buckets are 1, 2, 3, ≥4), so the implementation either mis-bins, silently drops, or panics depending on how the index is computed. A 3-read family (one pair + one orphan) truncates to 1 fragment, silently losing the orphan from the size accounting.
- Emission itself is unaffected (emit-and-evict keys on read counts, not `/2`), and emitting a single-mate family is consistent with the "it's coverage" decision — but the plan must *define* the behavior: bucket on read counts directly, or `ceil(expected/2)`, or bucket odd families explicitly. Add a test: `min_mapq` set so exactly one mate of a pair is filtered → family of 1 read is emitted and binned sanely.

### 1.4 Emit-and-evict + residual guard (design sound; two unspecified paths)

PASS 1 and PASS 2 read the same files through the same `key_of` closure (`mod.rs:2423-2481`), which is deterministic in the record; any I/O error aborts either pass. So the record sets genuinely cannot differ absent mid-run input mutation — the fail-loud residual check (`!sfams.is_empty()` → error) is correctly aimed. Two paths the plan does not specify:

1. **Key missing from `simplex_expected` in PASS 2.** Step 4 writes `simplex_expected[key]` — on a mutated input that gained a record, direct indexing **panics** instead of producing the clean `AlignerError::Validation` the plan promises everywhere else. Specify: absent key (and not in `paired`) → hard error.
2. **Post-eviction re-arrival.** After `remove()`, a late duplicate record re-creates a fresh entry that counts up from 1. If fewer than `expected` extras arrive, EOF catches it (good). If *exactly* `expected` extras arrive (input duplicated), the family **silently emits twice** and EOF sees nothing. Pathological (requires input mutation, same class as the under-count case the plan does guard), but the stated invariant "a mismatch means the input changed mid-run — fail loud" only covers one direction. A `done: HashSet<CKey>` (or sentinel in `simplex_expected`) closes it for one bit per family.

Memory: the dominant singleton class evicts on the adjacent mate in Bismark-written BAMs (verified: in-run PE BAM is read-order, mates adjacent). Coordinate-sorted `--from_bam` inputs keep in-flight families bounded by the overlapping window. Multi-lane inputs keep *cross-file multi-fragment* families in flight until their last member — bounded by the multi-fragment family count, acceptable; worth one clause in §6.

### 1.5 Byte-identity gate for the default mode (CRITICAL — flaky as specified)

The plan's changes to the default path are indeed contained: PASS 1 skips the simplex map and drops `counts` as today; the `mx` insertion is gated on `emit != Duplex`; the `emit_family` extraction is byte-neutral if the `dpx` qname format and per-family `emitted`/`skipped` accounting are preserved (the forward/reverse records share pos+length, so the chromosome-edge guard fires for both or neither — `wrote_any` semantics survive the extraction). Two leaks remain:

1. **V1 "Identical BAM bytes" cannot hold as written.** Duplex emission iterates `fams.values()` over a `std::collections::HashMap` (`mod.rs:2509`, `:2569`) — iteration order is randomized per process (`RandomState`). The consensus BAM's *record order is nondeterministic today*: two runs of the **same** binary on a multi-family fixture produce different bytes. The existing tests (`tests/aligner_five_base_groundtruth.rs:274`, `:912`) are content-assertions, so this was never visible. V1's before/after binary comparison and V8's "duplex records byte-identical modulo the mx tag" are both flaky-by-design. Fix one of two ways: (a) define the gate as the suite's canonical form — decompressed SAM, @PG-block filtered, **records sorted** — or (b) make emission order deterministic first (sort family keys before the emit loop; since today's order is random, imposing an order cannot break identity with any stable baseline, and it buys true byte-gates for every consensus output from now on). The plan must pick one; as written its hard gate does not gate.
2. **Standalone `Note:` line.** Step 7 "extend the 'Note:' line with the mode" — if unconditional, the *default-mode* standalone stderr changes, contradicting §3.2/V1. Extend only when `emit != Duplex`.

### 1.6 `mx` tag mechanism (claim wrong as written)

Verified: nothing in `rust/` reads or writes an `mx` tag (grep clean); the suite's tag inventory is XM/XR/XG/NM/MD/RX/CB/UR (§8.5's list omits NM/MD — harmless, both standard, no collision either way). Lowercase-first two-char tags are SAM local/user space — `mx:i` is legal and free. But step 5's mechanism is wrong on two counts:

1. `five_base_emit_record` returns `Option<crate::io::BismarkRecord>` (`mod.rs:1661-1671`), **not** a `RecordBuf`. `BismarkRecord` exposes `inner()` (immutable) and targeted setters only — there is no public `data_mut()`. The one-line fix is a setter mirroring `set_rx` (`io/record.rs:224-230`).
2. House API is `Tag::from(*b"mx")` + `Value::from(mx)` (see `output.rs:485-495`); `Tag::new(b'm', b'x')` is not the codebase pattern and may not exist in the pinned noodles.

Neither invalidates the design; both would stall an implementation plan copied from this text.

### 1.7 Remaining logic checks (pass)

- Line citations: PASS 1 `:2483-2505` ✓, PASS 2 `:2508-2563` ✓, singleton skip `:2519-2521` ✓, emission `:2569-2665` ✓, SE rejection `:562-568` ✓, call site / standalone / cli ranges ✓.
- QNAME uniqueness: an OT-only and OB-only family at the same span+UMI cannot coexist (they'd be paired), so `spx:{chrom}:{start}-{end}:{umi}` cannot collide with itself; `NA` convention preserved.
- Signature invariant (§4): the `Some`/`None`-vs-`emit` cross-check is a good fail-loud touch.
- `simplex` mode DRAGEN semantics (duplex counted, not emitted) — coherent, and §10 already commits to documenting the missing-duplex-BAM surprise.

## 2. Assumptions

| # | Plan assumption | Status |
|---|---|---|
| 1 | Default byte-identical | **Needs redefinition** — see 1.5; "byte-identical" is currently undefined for this output because record order is random. Gate must be order-canonical (or ordering made deterministic first). |
| 2 | Mates adjacent affects only peak memory | Verified ✓ (eviction keys on exact counts). |
| 3 | PASS 1/PASS 2 sets identical, violation fails loud | Verified for the under-count direction; over-count/absent-key paths unspecified (1.4). |
| 4 | Fragment count = read count / 2 | **Wrong** under per-mate MAPQ filtering and the umi_len UMI caveat (1.3). |
| 5 | `mx` unused in suite | Verified ✓ (grep clean; lowercase = SAM local space). Mechanism claim wrong (1.6). |
| 6 | PE-only per `mod.rs:562-568` | Verified ✓. |
| 7 | Concordance-gated, no byte oracle | Consistent with the flag docs ✓. |

**Unstated assumptions to surface in §8:** FR-orientation proper pairs (1.2); mates' UMIs canonicalize equal — guaranteed for the qname path, chemistry-dependent for `umi_len` (1.2); before/after builds share VERSION/@PG content for any byte comparison; whether the histogram counts emitted families or all simplex families (incl. skipped) is undefined.

## 3. Efficiency

- **Default:** zero-cost, explicit in step 3 ✓.
- **PASS 1 map:** step 3 says "build alongside `paired`" *during counting* — that is a second hash op per record for nothing. `counts` already holds everything: derive `simplex_expected` in the same single sweep that derives `paired` (exactly one of ot/ob nonzero → insert ot+ob), then drop `counts`. Peak = `counts` + the two derived structures momentarily; no per-record cost. The ~600 MB worst-case estimate is the right order (std HashMap ≈ 32 B/entry at 0.875 load → 0.7–1 GB at 20M families); fine for an opt-in mode as documented.
- **`simplex` mode stores duplex members for nothing.** Step 4 changes only the simplex branch; the `paired.contains` branch would still build covered maps for families that are never emitted. When `emit == Simplex`, skip storing paired families entirely — otherwise simplex mode pays the full duplex memory bill for zero output.
- **Emit-and-evict:** sound; the eviction bound and the cross-file caveat are covered in 1.4. One extra hash lookup per PASS-2 record (two with a done-set) — negligible.

## 4. Validation sufficiency

The highest-risk silent failures are (a) an orientation slip in the OB simplex path (wrong XM, plausible-looking BAM), (b) default-mode drift, (c) mis-accounted family sizes. Against those:

| Gap | Fix |
|---|---|
| V1/V8 order-flakiness | Redefine as sorted decompressed-SAM comparison, or make emission deterministic first (1.5). Ensure the fixture has ≥2 families or the gate is vacuous. |
| No XM-content assertion for the OB simplex record | Extend V3 (or V5) to assert the exact XM string for **both** OT-only and OB-only families — this is the plan's own top-listed risk and currently rides on a real-data "spot-check". |
| Odd-count families untested | Add: `min_mapq` filters one mate → 1-read family emitted, histogram bin defined (1.3). |
| V4 "artificially truncating expected counts (test-only)" | Unspecified mechanism smelling of a test-only production branch. Extract the eviction bookkeeping (expected/arrived/done/remove) into a small unit-testable helper and drive the residual + absent-key + re-arrival cases there. |
| V8 "simplex emitted ≈ PASS-1 singleton count" | Make it exact: `emitted_spx + skipped_spx == simplex_expected.len()`, cross-checked against the duplex TXT's `singletons` figure (`five_base_duplex.rs:304-308`). "≈" is not a gate. |
| Mode matrix only exercises the in-run entry | Add the standalone (`--from_bam`) entry to V6: file naming, default `Note:` line unchanged, non-default mode extended. |
| UMI-qname simplex fixture | Optional: one `both`-mode case with `--five_base_umi_qname` (UMI body in qname vs `NA`). |
| Cross-file leakage in `both` mode | Covered implicitly by V3's exact-2-records assertion; make the no-`dpx:`-in-simplex-BAM check explicit for free. |

## 5. Alternatives

1. **Deterministic emission order as a prerequisite commit** (sort family keys before the duplex emit loop). Today's order is random, so no stable baseline exists to break; afterwards every consensus output supports true byte-gates, which this plan's own V1 wants. Strongly preferred over sorted-comparison-forever.
2. **Bucket the histogram on read counts, not fragments.** Sidesteps 1.3 entirely and is what the data actually is; report "reads" and let the doc note that 2 reads ≈ 1 fragment.
3. **Per-record family-size tag** (cf. samtools markdup `dc:i`, fgbio `cD`) alongside `mx` — cheap now, lets cfDNA users weight by consensus depth without re-running. Optional.
4. Single-file / tag-only designs — already decided with Felix (separate BAM + tag); no re-litigation. The `mx`-vs-`XM` visual proximity is worth one docs sentence (case-sensitive, unrelated) but the name is fine and collision-free.

## 6. Action items

### Critical
1. **Redefine the byte-identity gate (V1, V8) or make emission order deterministic first.** `fams.values()` iteration order is randomized per process — "identical BAM bytes" between two runs is unachievable today on any multi-family fixture; the plan's hard regression gate does not currently gate (§1.5).
2. **Fix the read-count/2 model.** Odd-sized (min 1-read) simplex families are real under `--five_base_min_mapq` (and the umi_len UMI caveat); define histogram binning for them, correct §2 fact 3's "minimum = one full pair", and add the orphaned-mate test (§1.3).

### Important
3. Correct the `mx` insertion mechanism: `five_base_emit_record` returns `BismarkRecord` (no public `data_mut()`) — add a `set_rx`-style setter; use `Tag::from(*b"mx")` + `Value::from(i32)` per house style (§1.6).
4. Make the standalone `Note:` line extension conditional on `emit != Duplex`, or the default-stderr gate is violated (§1.5.2).
5. Specify PASS 2's absent-key path (clean `AlignerError`, not an indexing panic) and add a done-set so post-eviction re-arrivals fail loud instead of silently double-emitting (§1.4).
6. In `simplex` mode, skip storing paired families' covered maps in PASS 2 (§3).
7. Strengthen fact 2's rationale (the suppressed record would carry systematically *wrong* non-CpG calls, not merely empty CpGs) in the plan/doc-comment, and assert XM content for both simplex orientations in the tests (§1.1).
8. Replace the "test-only truncation" residual-guard test with a unit-testable eviction helper (§4).

### Optional
9. Derive `simplex_expected` from `counts` at end of PASS 1 rather than per-record "alongside" (§3).
10. Separate `skipped` counters per multiplicity and define whether the histogram covers emitted or all simplex families (§2).
11. Make V8's count identity exact and cross-check against the duplex TXT `singletons` line (§4).
12. Add the standalone entry to the mode-matrix test; optional umi_qname simplex fixture (§4).
13. Surface the FR-orientation and umi_len-revcomp-UMI assumptions in §8; one-line docs note distinguishing `mx` from `XM`; consider a family-size tag (§5).

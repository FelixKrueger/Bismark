# Plan Review A — 5-Base simplex consensus output (#1104)

**Plan:** `plans/08142026_5base-simplex-consensus/PLAN.md` (rev 0, 2026-08-14)
**Reviewer:** A (fresh context, independent)
**Verdict:** REQUEST CHANGES — the design is sound and the source-derived facts largely check out, but the plan's #1 regression gate is not well-defined as written (nondeterministic duplex emission order), one stated assumption is falsifiable under `--five_base_min_mapq` (odd read counts break the histogram), and the tag-insertion mechanism named in step 5 does not exist on the actual return type.

All line references below were verified against the working tree
(`rust/bismark/src/aligner/mod.rs`, `five_base_duplex.rs`, `cli.rs`, `config.rs`,
`rust/bismark/src/io/record.rs`, `write.rs`).

---

## 1. Logic review

### 1.1 Fact 1-2: one record per simplex family, on the molecule's own strand — VERIFIED, and stronger than stated

`consensus_base` (`five_base_duplex.rs:355-371`): at `PlusCpG` own = `ot`, at `MinusCpG`
own = `ob`; `own == None → (b'N', …)`. For an OT-only family every `-`-strand CpG is `N`,
so the FLAG `0x10` (GA-call) record carries no CpG calls. The revcomp handoff mirrors the
existing duplex reverse path (`mod.rs:2623-2629`: `revcomp(cons_seq)` + reversed quals →
`five_base_emit_record`, which maps `0x10 → index 1` (OB/GA) at `mod.rs:1679`). Correct.

One strengthening the plan misses: for an OT-only family the suppressed `0x10` record
would not merely be information-free — it would be **actively wrong**. At non-CpG genomic-G
positions `reconcile_generic` `(Some(x), None) → x` returns the OT (top-strand) base, which
is `G` regardless of bottom-strand methylation; a GA-call record built from that would emit
systematically "unmethylated" CHG/CHH calls for a strand that was never sequenced. Emitting
only the own-strand record avoids this. Worth one sentence in the flag doc / emit_family
comment, because it is the real reason the single-record rule is not a style choice.

### 1.2 Fact 3: both mates land on the same molecule strand — VERIFIED for FR proper pairs

`molecule_is_ot = (flag & 0x40 != 0) == coverage_forward` (`mod.rs:2459`). The four
combinations: R1-fwd → `true==true` OT; R2-rev → `false==false` OT; R1-rev → OB; R2-fwd →
OB. So the two mates of an FR proper pair always agree. `kstart = min(ref_start, mate0)`,
`kend = kstart + |tlen|` are mate-symmetric, so both mates get the same `CKey`. Caveat: a
hypothetical FF/RR pair flagged proper would split into a fake OT+OB "duplex"; minimap2
`-x sr` / bowtie2 FR mode never set `0x2` for FF/RR, so this is theoretical — but the claim
holds only *given* FR proper pairs, and the plan states it unconditionally.

**However, "the minimum simplex family is one full read pair (counts = 2)" is FALSE under
`--five_base_min_mapq`.** The MAPQ filter in `key_of` (`mod.rs:2431`) is per-record and
runs before the pair gates; minimap2 assigns per-mate MAPQ, so one mate can pass at MAPQ 30
while the other is dropped at MAPQ 10. That yields odd family counts and single-read
families. All other `key_of` filters (0x4/0x100/0x800, 0x2, TLEN==0, missing mate-start)
are mate-symmetric — so odd counts occur *exactly and only* when `min_mapq > 0`, which is
the DRAGEN-parity recommended setting for consensus runs. Consequences:

- Histogram bucket `expected/2` (§5.8) integer-divides a count-1 family to fragment count
  **0**, a bucket that does not exist (`1, 2, 3, ≥4`) — silent mis-report or index bug.
- §2 fact 3 and §8 assumption 4 are wrong as stated (assumption 4's justification "proper
  pairs only reach the family" ignores the per-mate MAPQ drop).
- Collapse correctness is unaffected (a 1-read family collapses to that read's bases; span
  still comes from the surviving member), but the report — half the feature — is wrong.

Fix: bucket on read count, or define fragments as `(expected + 1) / 2` with a doc note, and
add a test with `min_mapq` filtering one mate (see §4).

### 1.3 Emit-and-evict + residual guard — sound, two unspecified holes

PASS 1/PASS 2 identity: `key_of` is deterministic on record bytes and both passes re-read
the same paths (`mod.rs:2486-2500`, `2510-2521`), so the multiset of `(CKey,
molecule_is_ot)` can only differ if a file mutates between passes — the hard-error residual
check is the right shape and matches house fail-loud style. Two gaps:

1. **Unknown key in PASS 2.** Step 4 indexes `simplex_expected[key]`. A record whose key is
   in neither `paired` nor `simplex_expected` (same mutated-input scenario the residual
   check exists for) must fail loud, not panic on a missing-key index or silently skip. The
   plan doesn't say which happens.
2. **Over-arrival blind spot.** After emit+`remove()`, extra arrivals re-enter `sfams` as a
   fresh family; if they reach `expected` again the family is emitted **twice** with no
   error (only a partial refill is caught at EOF). Practically requires an exact duplication
   of the family mid-run — acceptable, but document it as the known limit of the invariant.

Also unstated: in `simplex` mode, are paired families' `Member`s still stored in PASS 2?
They are not emitted, so storing their covered maps is pure waste; the plan should say PASS
2 skips storage for paired families when duplex is not emitted (the counters for the report
come from PASS 1 / `paired.len()`).

### 1.4 Byte-identity gate for the default mode — the gate itself is broken as specified

Question: does anything besides the mx tag leak into the default path? The mx gating is
fine (`emit != Duplex` only), `drop(counts)` is preserved, writers per mode are fine, and
the `emit_family` extraction is a behavior-preserving refactor *gated by* validation 1. But:

**Validation 1 ("before/after binary → identical BAM bytes") is not a well-defined gate,
because today's duplex emission order is nondeterministic.** Emission iterates
`fams.values()` (`mod.rs:2569`) over a `std::collections::HashMap` (`mod.rs:2509`,
`RandomState`), whose iteration order varies run-to-run within the *same* binary. Two runs
of the unmodified binary on a multi-family fixture already produce differently-ordered
BAMs. So the gate as written is flaky (multi-family fixture) or vacuous (single-family
fixture), and validation 8's "duplex records byte-identical modulo the mx tag" on oxy will
fail spuriously on millions of families. No existing test byte-compares consensus output
(the groundtruth tests at `bismark/tests/aligner_five_base_groundtruth.rs` assert content,
not bytes) — this gate is new to this plan, and it needs one of:

- (a) order-normalized comparison — `samtools view | sort` (or sort decompressed SAM lines)
  on both sides; the qname `dpx:{chrom}:{start}-{end}:{umi}` + FLAG makes a total order; or
- (b) make duplex emission deterministic (sort `CKey`s by `(ref_id, start, end, umi_hash)`
  before emission). Note (b) technically changes default output — defensible since there
  is no stable byte baseline today, but it must be an explicit decision, not a side effect.

Pick one and write it into §9 rows 1 and 8. Until then the plan's hard regression gate
cannot pass reliably.

Two smaller default-path leaks to pin down:

- **stderr:** §3.7's "modes not exercised print what they emitted only" is ambiguous; state
  explicitly that `emit == Duplex` prints the current line **verbatim**
  (`mod.rs:2670-2674`), since validation 1 demands identical stderr.
- **Standalone "Note:" line (step 7):** "extend the 'Note:' line with the mode" — if
  extended unconditionally, default-mode standalone stderr changes, contradicting
  validation 1. Gate the extension to non-default modes (or scope the stderr gate).

### 1.5 The mx insertion mechanism named in step 5 does not exist

`five_base_emit_record` returns `crate::io::BismarkRecord` (`mod.rs:1661-1671`), not a
`RecordBuf`. `BismarkRecord` keeps `inner` private and exposes only `inner(&self)`,
`set_umi`, `set_rx` (`io/record.rs:181, 217, 224`) — there is no `data_mut()`. Also the
codebase's tag API is `Tag::from(*b"mx")`, not `Tag::new(b'm', b'x')`. The fix is small and
has precedent (`set_rx` at `io/record.rs:224` inserts an RX tag): add a targeted mutator
(e.g. `set_mx(i32)` or a generic int-tag setter) on `BismarkRecord` — it is `crate::io`, an
intra-crate module, so no crate-version cascade. The plan must name this enabling change;
as written, step 5 doesn't compile.

### 1.6 New-flag validation placement misses one dispatch path

The standalone entries dispatch **before** `resolve()` (`mod.rs:165-185`), so config.rs's
"existing `five_base_*` cross-checks" (config.rs:706-764) never run for them. The plan's
placement covers: normal align run (resolve fires — good) and the consensus standalone
(from_bam satisfies the requirement — good). It misses:
`--five_base_emit_multiplicity both --five_base_bisulfite_bam X.bam` → dispatched at
`mod.rs:183` before any check → **new flag silently ignored**, violating fail-loud. Add an
explicit rejection in (or before) the bisulfite standalone path, and state in §5.2 where
each of the three dispatch paths gets its check.

### 1.7 Verified minor claims

- Line references in §2's table all check out against the working tree.
- `--five_base_consensus` implies `--five_base_duplex` (config.rs:990), so the in-run nested
  call site is always reached when consensus is on; the new flag rides the same path.
- `end <= start` skip, chromosome-edge `None` from `five_base_emit_record`
  (`mod.rs:1694-1697` length guard), `NA` UMI convention (`mod.rs:2610-2614`), header-only
  BAM on empty output (matches today's unconditional writer creation) — all consistent.
- `mx` collision claim: verified by grep — the suite touches exactly
  AS, CB, MD, NM, RX, UR, XG, XM, XR; no `mx` anywhere. (Assumption 5's list is incomplete
  — it omits AS/NM/MD — but its conclusion holds.) Lowercase two-letter tags are SAM
  local-use space; `mx:i` is safe. Extractor consumers only parse XR/XG/XM
  (`io/record.rs:116-130`), so the tag is inert downstream, as claimed.

---

## 2. Assumptions

| # | Plan assumption | Status |
|---|---|---|
| 1 | Default byte-identical, tag only in new modes | Right intent; the *gate* is unsound as specified (§1.4) — fix the comparison, then this holds |
| 2 | Mates adjacent affects only peak memory | Verified — eviction keys on exact counts; coordinate-sorted `--from_bam` input degrades memory only. See §3 for the PCR-duplicate span caveat the plan understates |
| 3 | PASS 1 ≡ PASS 2 record sets | Verified deterministic; add the unknown-key hard error (§1.3.1) to make the fail-loud claim complete |
| 4 | Fragment count = read count / 2 | **FALSE when `min_mapq > 0`** (per-mate MAPQ, `mod.rs:2431`); the "0x2/TLEN gates" justification doesn't cover it (§1.2) |
| 5 | `mx` free in the suite | Verified (list incomplete but conclusion correct) |
| 6 | PE-only | Verified (`mod.rs:562-568` rejects SE) |
| 7 | Concordance-gated, no oracle | Consistent with the 5-Base precedent |

Implicit assumptions worth surfacing:

- **FR-only proper pairs** underpin fact 3 (§1.2) — true for minimap2 `-x sr` and
  bowtie2/hisat2 FR mode; say it rather than assume it.
- **Both mates carry the same RX** (else `umi_hash` splits a pair into two families) —
  existing behavior of the 5-Base RX writer, unchanged, but it silently underlies "one pair
  = one family".
- **`Tag::from`/mutator availability** — see §1.5; the plan assumed a mutable surface that
  isn't there.

---

## 3. Efficiency

- **Default:** genuinely zero-cost — verified that step 3 keeps `drop(counts)` and skips
  the new map.
- **PASS 1 arithmetic is off, and the extra map is avoidable.** `CKey` is 20 bytes of data
  (3×u32 + u64), 24 aligned — not 16. More importantly, PASS 1 already holds
  `counts: HashMap<CKey,(u32,u32)>` over **all** families today; building
  `simplex_expected` *alongside* it (step 3) transiently doubles PASS-1 memory in simplex
  modes. Alternative: build `paired`, then `counts.retain(|_,(ot,ob)| *ot==0 || *ob==0)` (+
  `shrink_to_fit`) and reuse it as the expected-count map (`ot+ob`) — near-zero marginal
  peak, no second 20M-entry map, and the 8-byte value overhead is trivial. Recommend this
  over the plan's design.
- **In-flight PASS 2 memory is duplication-rate-bound, not just "incomplete families".** A
  multi-fragment simplex family stays resident from its *first* to its *last* member, and
  PCR copies are scattered across a read-order BAM — so peak scales with the duplication
  rate. Still strictly better than storing everything, and no worse per family than what
  the duplex path already does for paired families, but the doc note should say
  "duplication-dependent", not only "single-fragment families evict immediately".
- Emit-during-PASS-2 interleaving with the post-pass duplex emission is fine (two
  independent writers).
- In `simplex` mode, skip storing paired families' members (§1.3) — the plan's §6 doesn't
  claim this saving but should.

---

## 4. Validation sufficiency

The eight rows cover the main behaviors, but the two highest-risk *silent* failures slip
through as specified:

1. **Rows 1 & 8 are flaky/vacuous until order-normalized** (§1.4). As written, the
   plan's primary regression gate can fail on an unmodified binary. Must specify sorted
   comparison or deterministic emission.
2. **No odd-count / MAPQ-orphan test.** Add: family where one mate passes `min_mapq` and
   the other doesn't → asserts (a) the single-read family still collapses and emits, (b)
   the histogram buckets it sanely under the fixed definition. Without it the histogram
   ships wrong for exactly the DRAGEN-parity configuration.
3. **Row 4's error-path mechanism is hand-waved.** "Artificially truncating expected counts
   (test-only)" implies test-only hooks in production code. Specify the mechanism —
   cleanest is to extract the eviction loop into a function taking the expected-map + a
   record iterator so the residual/unknown-key errors are unit-testable without hooks.
4. **Missing: the silently-ignored-flag case** — `--five_base_emit_multiplicity both`
   + `--five_base_bisulfite_bam` must error (§1.6); add a CLI test beside row 7.
5. Row 6 should state *which entry point* the mode matrix runs through — ideally both
   (in-run fixture and `--from_bam` on its BAM), since path derivation and validation
   plumbing differ between them.
6. Row 3 is good; extend it to assert the OB-simplex record's XM calls sit at `-`-strand
   CpGs only (the orientation slip named in §11 risk (a) deserves a synthetic assertion,
   not only the oxy spot-check).

---

## 5. Alternatives

1. **Reuse `counts` via `retain` as the expected map** (§3) — strictly less memory, less
   code. Recommended.
2. **Paths enum instead of two `Option<&Path>` + `emit` + debug_assert:**
   `enum EmitPaths<'a> { Duplex(&'a Path), Simplex(&'a Path), Both { duplex: &'a Path, simplex: &'a Path } }`
   makes the §4 invariant unrepresentable instead of asserted. Small, idiomatic, removes
   the runtime check the plan itself flags as a call-site risk.
3. **Deterministic duplex emission order** (sort keys) — solves §1.4 at the source and
   makes every future byte gate meaningful; needs an explicit "default output order
   changes (was random)" decision with Felix.
4. **Tag naming:** fgbio uses `cD`/`aD`/`bD` for consensus depth; `mx` is fine and
   collision-free (verified), but if cross-tool affinity ever matters, note the precedent
   in the doc. Not a change request.
5. **Docs mitigation pairing:** simplex calls lack the cross-strand variant check; the
   suite already has the population-level check (`--five_base_deconvolution`). One doc
   sentence recommending deconvolution alongside simplex mode turns the caveat into a
   workflow.

---

## 6. Action items

### Critical

1. **Fix the byte-identity gate methodology (§9 rows 1, 8):** duplex emission iterates a
   `std` HashMap (`mod.rs:2509/2569`) — record order is nondeterministic run-to-run, so
   "before/after binary byte compare" is flaky/vacuous as written. Specify order-normalized
   comparison (sort decompressed SAM) or make emission order deterministic as an explicit
   decision.
2. **Fix the histogram + assumption 4 for odd read counts:** per-mate `min_mapq` filtering
   (`mod.rs:2431`) produces odd-count and single-read families; `expected/2` buckets a
   1-read family into nonexistent bucket 0. Redefine the bucket basis (read count, or
   `(expected+1)/2` documented), correct §2 fact 3 / §8 assumption 4, and add the
   MAPQ-orphan test (§4.2).

### Important

3. **Name the real mx insertion mechanism:** `five_base_emit_record` returns
   `BismarkRecord` with no `data_mut()`; add a mutator on `BismarkRecord` (precedent:
   `set_rx`, `io/record.rs:224`) and use `Tag::from(*b"mx")`. Step 5 as written doesn't
   compile.
4. **Close the silently-ignored-flag path:** standalone dispatch precedes `resolve()`
   (`mod.rs:165-185`), so `--five_base_emit_multiplicity` + `--five_base_bisulfite_bam`
   would be silently dropped. Add an explicit rejection; document where each of the three
   dispatch paths validates the flag.
5. **Specify PASS-2 unknown-key handling as a hard error** (mutated-input key absent from
   both `paired` and `simplex_expected`), and document the exact-refill double-emission
   blind spot of the residual check.
6. **Adopt the `counts.retain` reuse** instead of a parallel `simplex_expected` map
   (halves the added PASS-1 transient peak; also fix "16-byte key" → 24 bytes).
7. **Specify row 4's error-injection mechanism** without test-only production hooks —
   extract the eviction loop into a unit-testable function.
8. **Pin the default stderr contract:** `emit == Duplex` prints the current summary line
   verbatim; the standalone "Note:" extension appears only in non-default modes.

### Optional

9. Replace the two-`Option` signature with an `EmitPaths` enum (invalid states
   unrepresentable).
10. State in `simplex` mode that PASS 2 does not store paired families' members.
11. Strengthen the one-record rationale in the doc: the suppressed opposite record would
    emit wrong-strand non-CpG calls, not just empty ones.
12. Docs: recommend `--five_base_deconvolution` alongside simplex mode as the
    population-level variant mitigation; note the fgbio consensus-tag precedent if wanted.
13. Note the FR-proper-pair premise under fact 3, and the duplication-rate dependence of
    PASS-2 in-flight memory in §6.
14. Naming nit: `five_base_simplex.bam` → consider `five_base_simplex_consensus.bam` for
    symmetry with the duplex file (both contain consensus reads).

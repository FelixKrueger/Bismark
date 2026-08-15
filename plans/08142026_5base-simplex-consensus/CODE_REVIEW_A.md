# Code review A — #1104 simplex 5-Base consensus output

**Branch:** `1104-simplex-consensus` (base `dev` @ `53744d5`) · commits `50f1eef` (step 0) + `a80aa3a` (feature)
**Reviewer:** A (independent, fresh context) · 2026-08-15
**Verdict: APPROVE** — one High defect fixed in-review, one coverage gap closed; both sabotage-verified.

Files reviewed against source (not the plan): `rust/bismark/src/aligner/{cli,config,mod,five_base_duplex}.rs`, `rust/bismark/src/io/record.rs`, `rust/bismark/tests/aligner_five_base_simplex.rs`, CHANGELOG/docs/rust-README.

## Summary

The implementation is faithful to the plan and careful where it matters: the default `duplex` path is verifiably byte-identical to the step-0 baseline (no new maps, no tag calls, identical emit code), the emit-and-evict ledger keeps simplex memory bounded, and the own-strand rule is both documented and *tested* (the non-CpG-G fixture converts the contamination rationale from argued to exercised). I found **one real defect** — the ledger's `finish()` missed the zero-arrival residual direction — which I fixed directly (small, unambiguous, plan-mandated behaviour), plus one untested branch (the histogram's `≥5` clamp) which I closed with a test. One Medium (A15) is about the suite's pre-existing samtools guard silently no-opping gates, not about this feature. Everything else is Low/informational, and no finding challenges the design.

## Findings by review area

### Logic

**A1 (High, FIXED): `SimplexLedger::finish()` did not detect a family with zero PASS-2 arrivals.**
`finish()` checked only `self.arrived.is_empty()`. A family PASS 1 counted (`expected` ≥ 1) that PASS 2 never saw at all is in neither `arrived` nor `done`, so `finish()` returned `Ok(())` — the family silently never emits, and the stderr count identity `emitted + skipped == n single-strand families` breaks without any error. The plan (§3.8) requires "arrived < expected at EOF → hard error"; zero arrivals is the extreme case of exactly that. Additionally, with a mix of in-flight and never-seen families the `Residual(n)` count under-reported (it counted in-flight only). The unit test `ledger_reports_residual_families_at_finish` used families with ≥ 1 arrival each, so this direction was never exercised — exactly the "gate not sabotage-verified" class.
**Fix applied** (`rust/bismark/src/aligner/five_base_duplex.rs`): `finish()` now computes `expected.len() - done.len()` (done ⊆ expected keys, no underflow) and errors when non-zero; new unit test `ledger_reports_a_family_that_never_arrived` covers the direction. All existing ledger tests pass unchanged (the residual counts coincide for their fixtures). Reachability is admittedly defensive-only (requires the input to change between passes), but the codebase convention is fail-loud and the plan promised this direction.

**A2 (verified OK): `emit_family` is behaviour-preserving for the duplex path.**
Line-by-line against the pre-extraction code: the three skip guards (`sq_order`, `genome.get`, `end <= start`) map to `Ok(false)` → `skipped += 1`; the `[0, 0x10]` FLAG loop, the revcomp/reverse-qual handoff for `0x10`, per-flag `Counters::default()`, the `dpx:{chrom}:{start}-{end}:{umi_str}` qname (via `qname_prefix = "dpx"`), and `five_base_emit_record`'s argument list are all identical. `set_mx` is called **only when `mx.is_some()`**, and the duplex loop passes `spx_path.is_some().then_some(2)` — `None` in default mode (`EmitPaths::Duplex` ⇒ `spx_path = None`), so the default BAM carries no tag and no other byte changes. The byte-identity claim for default mode holds by inspection; the integration test asserts the observable proxies (no `mx:i:`, no simplex BAM, verbatim stderr).

**A3 (verified OK): own-strand rule.** `fam.ob.is_empty()` is a sound discriminator: a simplex family is by construction one-strand-only (PASS 1 classifies `ot>0 && ob>0` as paired; anything else is simplex), and PASS 2 uses the same `key_of`, so ledger-completed families have members on exactly one strand. A mixed family could only arise from input mutation between passes — and then the surplus/deficit is caught by `AfterEmission`/`Residual` before the run succeeds. `fam.ot`/`fam.ob` both empty is impossible (`expected ≥ 1` structurally, each arrival pushes to one vector).

**A4 (verified OK): PASS 2 storage rule.** `is_paired_fam && dpx_path.is_none()` (simplex mode: paired counted, never stored) and `!is_paired_fam && spx_path.is_none()` (default: exact old behaviour) are both placed **before** the CIGAR walk, so no wasted work and no dropped records: in `both` mode neither guard fires and every record is either stored (paired) or ledgered (simplex). Counters are internally consistent per mode.

The `expect("simplex writer exists whenever the ledger yields a family")` at `mod.rs:2800` is genuinely unreachable, on three independent couplings: `simplex_expected` is populated only under `spx_path.is_some()` (`:2574`); `spx_writer` is `Some` iff `spx_path.is_some()` (`:2590`); and a simplex record is `continue`d before reaching the ledger when `spx_path.is_none()` (`:2745`). Even a hypothetical leak would hit `LedgerError::UnknownKey` (empty `expected`) and error out before touching the writer.

**A5 (verified OK): determinism.** `CKey` has exactly the four fields `(ref_id, start, end, umi_hash)`; sorting by all four is a total order over *distinct* keys — two keys equal on all four **are the same `CKey`** and cannot coexist as separate `HashMap` entries, so `sort_unstable` has no ties and is fully deterministic. (Two distinct *molecules* colliding on `umi_hash` within one span merge into one family upstream — pre-existing, documented at the `CKey` definition, unaffected by the sort.) Simplex order = family-completion order in the input stream, deterministic given the same input. Step 0's commit (`50f1eef`) is minimal: 3-line sort + CHANGELOG + the determinism test.

**A6 (verified OK): histogram.** `size_hist[(n as usize).min(5) - 1]` cannot panic: every `counts` entry is created only by incrementing one side, so `n = ot + ob ≥ 1`. Computed in the PASS-1 sweep over **all** simplex families (before any emit/skip), as specified — so it covers families that are later skipped, per §3.7. The clamp is correct; it was simply untested, which A11 closes.

**A7 (verified OK): validation placement is complete.** Three paths reach or could ignore the flag: (a) align run → `resolve()` guard (`config.rs:769`); (b) `--five_base_consensus_from_bam` standalone → the flag is honoured by construction (dispatch at `mod.rs:189`); (c) `--five_base_bisulfite_bam` standalone → rejected at `mod.rs:178` **before** both dispatches (and the pre-existing mutual-exclusion check at `:168` covers the combined case). No other pre-`resolve()` dispatch exists in `run()`; SE cannot reach the consensus at all (`--illumina_5base` is PE-only at `config.rs:848/858`).

**A8 (verified OK): `set_mx`.** `mx` (lowercase = SAM local space) collides with nothing: a sweep over every `Tag::from(*b"..")` in the suite finds AS/CB/MD/NM/RX/UR/XG/XM/XR only; the extractor parses XR/XG/XM exclusively. `Value::from(i32)` → Int32; consensus records are synthesized fresh so "overwrites any existing mx" is moot but harmless.

### Errors

Covered by A1 (fixed). The other two ledger directions (`UnknownKey`, `AfterEmission`) are correct and reachable: `arrive_with` checks `done` first, then `expected`, before mutating `arrived` — an unknown key never touches state. On the caller's question about `arrive_with` mutating before the completion check: a duplicated record inflates `arrived` so the family completes *early* with wrong membership, but the genuine last record then hits `AfterEmission` and the run **fails** before returning success — fail-loud holds. The only undetectable corruption is a *count-preserving* record substitution between passes (PASS 1 sees {A,B}, PASS 2 sees {A,A}), which no count-based ledger can catch; the design documents the ledger as a count-disagreement detector, so this is an accepted limit, not a bug.

### Efficiency

Default mode is zero-cost as claimed: the sweep replaces the old `filter+collect` one-for-one, `simplex_expected`/`size_hist` are untouched, the ledger is empty, and no tag calls occur. Simplex modes add two hash lookups per PASS-2 record and the momentary `simplex_expected` map, matching the plan's budget. `emit_family`'s per-flag `cons_seq.clone()` predates this change (unchanged from the duplex path). In-run default mode derives `simplex_path` it never uses — a string allocation, negligible.

### Structure

Clean. `EmitPaths` makes invalid path/mode combinations unrepresentable; `emit_family` as a nested `fn` with explicit `genome`/`refid` (deviation 1) is the right call given the borrow across `fams`; `SimplexLedger<K, F>` generic over the key/family (deviation 2) is what makes the error paths unit-testable without consensus machinery. Comments follow the one-line current-state convention. CHANGELOG entries follow this file's established long-form house style.

## Test quality (9 committed integration tests + ledger/consensus-base units)

The integration suite (9 tests in `aligner_five_base_simplex.rs` as committed; 10 with my A11 addition) asserts real behaviour: exact XM strings for **both** simplex orientations (V3), the MAPQ-orphan 1-read family with histogram bucket (V5), the lone-pair bucket-2 + header-only duplex BAM in `both` mode (V6), mode-matrix file presence + count identity (V7), both CLI guards end-to-end (V8), and the non-CpG-G no-call fixture that pins the own-strand design decision. The two sabotages (sort removal; both-flags emission) were re-verified as documented. Gates that could *not* pass against broken code: the no-`mx` default assertion, the `!exists` simplex-BAM checks, and the exact-XM strings are all falsifiable. Remaining soft spots are listed below.

## Fixes applied (by reviewer A, working tree — uncommitted)

1. `rust/bismark/src/aligner/five_base_duplex.rs` — **A1**. `SimplexLedger::finish()` now computes the residual as `self.expected.len() - self.done.len()` instead of inspecting `self.arrived`, so a family PASS 1 counted but PASS 2 never delivered is reported (and mixed in-flight/never-seen residuals are counted correctly). Doc comment updated to "in flight OR never seen at all". New unit test `ledger_reports_a_family_that_never_arrived`.

   Diff:

   ```rust
   // before
   if self.arrived.is_empty() { Ok(()) } else { Err(LedgerError::Residual(self.arrived.len())) }
   // after
   let residual = self.expected.len() - self.done.len();
   if residual == 0 { Ok(()) } else { Err(LedgerError::Residual(residual)) }
   ```

   `done` only ever receives keys that were present in `expected` (`arrive_with` looks the key up in `expected` before inserting into `done`), so `done ⊆ expected` and the subtraction cannot underflow.

   **Sabotage-verified** (the discipline §11b applies to the implementer's own gates): with `finish()` reverted to the old body, `ledger_reports_a_family_that_never_arrived` FAILS with `left: Ok(()) right: Err(Residual(1))` — i.e. the pre-fix code declares a clean finish while a counted family silently never emitted. File restored from a `command cp -f` scratchpad backup (never `git checkout --`) and re-verified green.

2. `rust/bismark/tests/aligner_five_base_simplex.rs` — **A11**. New integration test `histogram_buckets_deeper_families_and_clamps_at_five` covers the histogram's upper buckets and the `.min(5)` clamp, which no fixture reached (every other family in the suite holds 1 or 2 reads). Window 0 gets three OT pairs (6 reads → one family, since span + empty UMI are identical) and window 1 a pair plus a lone mate (3 reads); the test asserts `reads per family 1:0 2:0 3:1 4:0 >=5:1` and `of 2 single-strand family(ies)`.

   **Sabotage-verified**: with `.min(5)` changed to `.min(4)` in `mod.rs`, the test FAILS, printing the mis-bucketed line `[reads per family 1:0 2:0 3:1 4:1 >=5:0]` — the 6-read family landed in bucket 4 instead of `>=5`. `mod.rs` restored from a scratchpad backup and confirmed byte-identical to the commit (`git diff` shows no `mod.rs` change).

   No production code was touched by this fix — it closes a coverage gap, and the shipped clamp is correct.

## Recommendations

| # | Priority | Recommendation |
|---|---|---|
| A9 | Low | Plan §3.5 says duplex families' "counts for the report come from PASS 1" in `simplex` mode, but no duplex count is printed there (`paired.len()` is computed and discarded; V7 even asserts the duplex line's absence, matching §3.7's stderr contract instead — the plan is self-inconsistent). If the figure matters to users, append e.g. `N duplex family(ies) not emitted (emit multiplicity: simplex)` to the simplex line; otherwise note the deviation in §11b. |
| A10 | Low | Plan §9 V5 promised an XM spot-check on the MAPQ-orphan family; the test asserts count + histogram only. The emit path is shared with V3's exact-XM tests, so risk is low — add one XM assertion to `mapq_orphaned_mate_forms_a_one_read_family` if cheap. |
| A11 | Low | **CLOSED in-review** — histogram buckets 3/4/≥5 and the `.min(5)` clamp were exercised by no test. Added `histogram_buckets_deeper_families_and_clamps_at_five` (see Fixes applied 2); the shipped clamp is correct. |
| A12 | Low | The new resolve() guard has no `config.rs` unit test, unlike its siblings (`five_base_duplex_guards`, `five_base_consensus_guards_…`); the integration test covers it end-to-end, but note it relies on the guard firing before genome/read validation (it does today, and the message assertion fails loud if that ordering ever changes). |
| A13 | Info | Plan §9 V7's cross-check against the duplex TXT `singletons` figure was not implemented (standalone from_bam has no duplex TXT); V9 (oxy real-data, `both` mode) is outstanding and disclosed in §11b. Track both before merge to dev. |
| A14 | Info | §11b says "the new suite is 10/10" and "1502 unit"; as committed the suite is **9** integration tests and the lib count is **1502**. With my two additions they become 10 and 1503. Cosmetic; correct when V9 lands. |
| A15 | **Medium** | `samtools_available()` returns `false` and each test `return`s early unless `$CI` is set, so 8 of the 10 integration gates no-op **silently** — and cargo captures the "skipping" line for a passing test, so the log looks identical either way. Not hypothetical: run in isolation the suite takes **~22 s** wall (verified with `--nocapture`, no "skipping" line, samtools 1.21 on PATH), but in the full `cargo test -p bismark` sweep the same suite reported **0.12 s** for all 10 tests — far too fast for 8 tests that each spawn samtools plus the bismark binary, i.e. consistent with the gates having early-returned there while still reporting `ok`. Since the guard's whole purpose is CI, consider gating on an explicit opt-out (`BISMARK_SKIP_SAMTOOLS_TESTS`) and panicking otherwise, so a missing/unreachable samtools fails rather than passes vacuously. This is a pre-existing suite-wide convention (copied from the #1095 suite), not something this PR introduced. |

## Verification (this review, foreground, from `/Users/fkrueger/Github/Bismark/rust`)

All commands run with the A1 fix applied; each assertion is on the command's **own** exit code, not a pipe's last stage.

| Command | Result |
|---|---|
| `cargo fmt -p bismark -- --check` | exit 0 — clean |
| `cargo clippy -p bismark --all-targets` | exit 0, **0** `warning`/`error` lines |
| `cargo test -p bismark --lib` | exit 0 — **1503 passed, 0 failed** (1502 baseline + the new ledger test) |
| `cargo test -p bismark --lib ledger_` | exit 0 — **6 passed, 0 failed** (all five original ledger tests plus the new one) |
| `cargo test -p bismark --test aligner_five_base_simplex` | exit 0 — **9 passed, 0 failed** as committed; **10 passed, 0 failed** with the A11 test (~22 s wall, so the samtools gates really execute) |
| `cargo test -p bismark --doc` | exit 0 — **8 passed, 0 failed** |
| `cargo test -p bismark` (full) | **`TEST_EXIT=0`** — **81 suites ok, 0 failed**, zero `FAILED` lines |

Per-suite results extracted from the full sweep — the neighbouring 5-Base gates are unaffected, matching §11b:

```
tests/aligner_five_base_bisulfite.rs   -> ok. 11 passed; 0 failed
tests/aligner_five_base_groundtruth.rs -> ok.  7 passed; 0 failed
tests/aligner_five_base_simplex.rs     -> ok. 10 passed; 0 failed
```

The simplex suite's timing in that sweep (0.12 s vs ~22 s isolated) is the A15 hazard, not a result problem — the isolated run is the one that proves the gates executed.

Confirms the implementer's reported state, with two corrections: the unit count is 1502 **before** my A1 test (1503 after), and the committed integration suite is **9** tests, not 10 (§11b/A14) — it is 10 with my A11 addition.

**Verification caveat (not a code defect).** Two other agents were building in this tree concurrently. One full-suite run reported 8 **doctest** failures, all `error: extern location for bismark does not exist: .../libbismark-595924614cb2b4e3.rlib`, in modules unrelated to #1104 (`bedgraph::filename`, `dedup::filename`, `io::strand`, `io::umi`, `nome_filtering::filename`) — a sibling `cargo` invocation replaced the rlib mid-run. Re-running `cargo test -p bismark --doc` in isolation passes all 8. Anyone re-verifying should assert on **cargo's own exit status**, not a pipeline's last stage: `cargo … | tail -40` reports `tail`'s 0 and hides a real failure (that is precisely how the first run here looked green).

## Verdict

**APPROVE.** One High defect (A1) found and fixed in the working tree, sabotage-verified; one coverage gap (A11) closed the same way. One Medium (A15) is a pre-existing suite-wide test-guard hazard, not a defect in this feature. No finding challenges the design. Default-mode byte identity, the own-strand rule, determinism, validation placement, ledger error directions, and tag hygiene are all verified against source rather than taken from the plan.

**Before the dev-merge:**

1. Commit my two working-tree changes (`five_base_duplex.rs` + `aligner_five_base_simplex.rs`) — currently uncommitted.
2. V9 (oxy real-data `both` mode) per §11b, still outstanding.
3. Optionally settle A9 (the plan promises a duplex count in `simplex` mode that the code does not print — the code matches §3.7's stderr contract, so I read this as a plan inconsistency, not a code bug).
4. Consider A15 separately from this PR — it decides whether CI's 5-Base gates can pass vacuously anywhere samtools is missing.

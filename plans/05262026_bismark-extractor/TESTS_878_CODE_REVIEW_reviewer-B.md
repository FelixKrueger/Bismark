# Code Review (Reviewer B) — #878 regression-guard tests for `MethCall.read_pos` rebase

**Target:** uncommitted diff in worktree `/Users/fkrueger/Github/Bismark-extractor` (base detached at `2bfe722`).
**Files:** `src/call.rs`, `src/parallel.rs`, `tests/parallel_phase_f.rs` (3 files, +320/-7).
**Scope claim:** 4 regression-guard tests, **no source `*.rs` logic changes**.
**Plan:** `plans/05262026_bismark-extractor/TESTS_878_PLAN.md`.

---

## Verdict

**APPROVE.** The implementation matches the plan, the fixtures are correctly oriented
(independently re-derived from `bismark-io` source, not trusted from comments), and all four
guards are **real** — I confirmed them empirically with the revert-smoke and two extra
mis-wire smokes. No correctness, efficiency, or structural defects of any priority above Low.
The scope claim (tests-only, no source logic change) is verified: the base commit already
carries the `call.rs:204` fix, and applying the diff to a clean checkout leaves line 204
untouched.

No issues were fixed in source (the brief forbids source edits, and none were warranted).

---

## How this review was conducted (important — shared-worktree hazard)

The live worktree `Bismark-extractor` was being **concurrently mutated** during my review — a
sibling process (almost certainly Reviewer A running the same revert-smoke) repeatedly toggled
`call.rs:204` between the fix (`aligned.read_pos_5p.saturating_sub(ignore_5p)`) and the revert
(`aligned.read_pos_5p`). My first `cargo test` in the live tree showed Test 3 FAILING with
`mbias[0].cpg[1].meth == 2` because the fix had been transiently reverted out from under me;
the `Edit` tool also threw "File has been modified since read." This is a **process-isolation
problem in the review harness, not a code defect** — but it means any test run against the live
worktree is unreliable while a second reviewer is active.

To get a trustworthy result I:
1. Captured the *intended* test-only patch from a moment when the live tree had the fix in place
   (verified the captured patch contains **no** `read_pos: aligned` revert line).
2. Created an isolated detached worktree at `2bfe722` and applied the patch there.
3. Ran all builds/tests/smokes in that isolated tree, immune to the concurrent toggling.
4. Cleaned up the isolated worktree and confirmed the live tree settled back to the clean
   intended state (fix present, only the 3 test files modified, no stray diagnostic).

All "PASS/FAIL" results below are from the **isolated** worktree.

---

## Independent verification results

### Build / test / lint (isolated worktree, fix in place)
- `cargo test -p bismark-extractor`: **all green**. lib **105** (plan claims +3 → 102→105 ✓);
  `parallel_phase_f` **18** (+1 ✓); every other suite green.
- The 4 new tests pass: `extract_calls_ob_strand_rebases_read_pos_after_ignore_5p`,
  `parallel_se_worker_m_bias_rebased`, `parallel_pe_worker_m_bias_uses_r2_ignore_for_r2`,
  `se_driver_vs_parallel_driver_m_bias_equality`.
- `cargo clippy -p bismark-extractor --all-targets -- -D warnings`: **clean**.

### Revert-smoke (the key acceptance gate, #878)
Reverted `call.rs:204` → `aligned.read_pos_5p` in the isolated tree:
- Test 1 (`extract_calls_ob_strand_...`): **FAILS** (panics at `call.rs:346`). ✓
- Test 2 (`parallel_se_worker_m_bias_rebased`): **FAILS** (panics at `parallel.rs:1236`). ✓
- Test 3 (`parallel_pe_worker_m_bias_uses_r2_ignore_for_r2`): **FAILS** (`parallel.rs:1292`). ✓
- Test 4 (`se_driver_vs_parallel_driver_m_bias_equality`): **stays GREEN**. ✓ — exactly the
  documented divergence-guard behaviour (plan A-I4); both drivers share the rebase, so a revert
  does not diverge them.

### Extra mis-wire smokes (beyond the plan's revert-smoke — to prove Test 3's claims)
With the fix in place, I injected two independent regressions:
- **R2 ignore mis-wire** (`parallel.rs:791` `ignore_5p_r2` → `ignore_5p_r1`): Test 3 **FAILS**
  (`mbias[1].cpg[1].meth` 0 ≠ 1). Confirms the test really guards "R2 uses `--ignore_r2`".
- **R2 cross-table mis-wire** (`parallel.rs:838` `mbias[1]` → `mbias[0]`): Test 3 **FAILS**
  (`mbias[0].cpg[1].meth` 2 ≠ 1; the `mbias_total` assertions also catch it). Confirms the
  `mbias[1]` index guard is bidirectional.

These two extra smokes give high confidence that Test 3 is not a vacuous pass.

---

## Fixture-orientation re-derivation (the dual-review trap — done from source, not comments)

### Strand classification (`bismark-io/src/strand.rs:65-68`)
OT = `CT/CT` (forward), OB = `CT/GA` (reverse), CTOT = `GA/CT` (reverse), CTOB = `GA/GA`.

### OB reversal kernel (`bismark-io/src/record.rs:299-307`)
For non-forward records: `read_pos_5p = seq_len - 1 - BAM_index`, then the call list is
`reverse()`d (emitted ascending in `read_pos_5p`). Confirmed verbatim in source.

### Test 1 (OB)
- OT `"..Zxh."` (forward): Z@2 (CpG-meth), x@3 (CHG-unmeth), h@4 (CHH-unmeth). `ignore_5p=2`
  keeps read_pos_5p ≥ 2 → survive at 2,3,4 → rebased **[0,1,2]**.
- OB `".hxZ.."` = `reverse("..Zxh.")`. BAM bytes h@1, x@2, Z@3 → read_pos_5p `5-1-BAM` =
  h→4, x→3, Z→2; after reverse the 5'-order is Z@2, x@3, h@4 → survive → rebased **[0,1,2]**.
- Asserted `ob_pos == [0,1,2]`, `ot_pos == ob_pos`, and contexts `[CpG-true, CHG-false,
  CHH-false]` — **all match my re-derivation**. The fixture is genuinely the byte-reverse and
  asserts the right thing (not merely passing). ✓

### Test 3 (PE R2 = CTOT)
- `BismarkPair::from_mates` (`pair.rs:40`) requires R1 flag with `ReadIdentity::R1` and R2 with
  `R2`; Test 3 uses `0x41`/`0x81` (matches the existing `make_pair_records` fixture). An OT pair
  has R2 = CTOT (`pair.rs:142` `from_mates_ot_pair`). ✓
- R1 `"...Z.."` OT: Z@BAM3 → read_pos_5p 3; `--ignore 3` → rebased **0** → `mbias[0].cpg[1]`.
- R2 `".Z......."` CTOT (9 bp, reverse): Z@BAM1 → read_pos_5p `9-1-1 = 7`; `--ignore_r2 7` →
  rebased **0** → `mbias[1].cpg[1]`. ✓ (single `Z`, so exactly one call.)
- Overlap: R1 ref span 100–105; R2's Z at ref 201; `drop_overlap` keeps `c.ref_pos >
  r1_ref_end` (OT branch, `overlap.rs:113`) → 201 > 105 → R2 survives. Non-overlapping by
  design. ✓
- The deviation note #2 in the plan ("R2 of an OT pair is CTOT; Z at BAM `seq_len-1-ignore_r2`
  = 9-1-7 = 1") is accurate.

---

## Are the guards real, non-vacuous? (brief item 2)

- **Tests 1–3 fail on revert** — confirmed empirically (above). Each is bidirectional:
  asserts the rebased slot is nonzero AND the absolute (reverted) slot is zero.
- **Test 2 absolute-slot assertions are safe AND meaningful.** `MbiasTable.{cpg,chg,chh}` are
  **lazily-grown `Vec`s** (`mbias.rs:51-63`, resize to `idx+1` only on write). The test uses
  `.get(4)/.get(5)/.get(6).map_or(0, …)` — when the fix is in place those vecs never grow that
  far, so `.get()` returns `None` → 0 (no panic). On revert they grow to len ≥ 5 and `.get(4)`
  returns `Some` with the count → the `== 0` assertion fails. Using `.get()` (not `[idx]`) is the
  *correct* choice given lazy growth. ✓
- **Test 4 non-emptiness is non-vacuous.** I re-derived `write_se_directional_bam` under
  `--ignore 2` over its 5-bp reads: r_OT_1 `Zz...` → 0 (both dropped), r_OT_2 `..X.x` → 2,
  r_OT_3 `H.h..` → 1, r_OB_1 `Z....` (OB) → 1, r_OB_2 `..h..` (OB) → 1. **5 surviving calls**,
  matching the plan's iteration log. The assertion scans CpG_/CHG_/CHH_ split files, skips empty
  and `Bismark`-header lines, and requires `call_lines > 0` — sound and genuinely exercised
  (`lo=2 < hi=5`, so the `lo >= hi` early-out at `call.rs:166` is *not* hit). ✓

---

## Helper soundness (brief item 3)

- **`config_with` (parallel.rs):** writes a temp `.bam`, runs `Cli::try_parse_from(...).validate()`,
  then `drop(tmp)`. `validate()` checks input-file existence **during the call**
  (`cli.rs:432-436`, `path.exists()` loop); the resulting `ResolvedConfig.files` owns the
  `PathBuf`, and `process_se`/`process_pe` **never read the file** (verified — they only read
  `config` fields). So dropping the tempfile after `validate()` is **not** a use-after-free:
  `validate()` does not need the file to persist past its own return. ✓
- **`synth_rec` flags/tags:** sets `flags`, single-`M` CIGAR of `xm.len()`, `seq = vec![b'A';
  xm.len()]`, refid 0, and XR/XG/XM tags. `from_noodles_record` enforces XM-len == seq-len
  (`record.rs:125`) — satisfied. PE flags `0x41`/`0x81` satisfy `from_mates`. ✓
- **`mbias_total`:** correctly sums `meth+unmeth` across all three context vecs; the per-table
  `== 1` assertions in Test 3 add a strong "exactly one call, no leakage" guard. ✓
- **`synth_se_record_strand` refactor (call.rs):** the original 3-arg `synth_se_record` is
  preserved as a `b"CT"` (OT) wrapper. All three pre-existing OT callers
  (`call.rs:303, 375, 400`) still compile and assert the same values — confirmed: the OT
  template test still passes with `[0,1,2,3]`/4-calls. Non-breaking refactor. ✓

---

## Edge / coverage (brief item 4)

- **CHG/CHH exercised, not only CpG:** Test 1 asserts CpG+CHG+CHH; Test 2 asserts a CpG-meth,
  a CHG-unmeth, AND a CHH-unmeth at three different rebased slots — so the context routing in
  `MbiasTable::accumulate` is genuinely exercised. ✓ Test 3 is CpG-only, but that is appropriate
  (its purpose is R1/R2 table + ignore dispatch, and Test 2 already covers multi-context routing).
- **Methylated/unmethylated mix:** Test 2 covers both (`meth` for CpG, `unmeth` for CHG/CHH). ✓
- No missing assertion that would let a real regression slip — see the two extra mis-wire smokes.

---

## Efficiency / structure (brief item 5)

- **Duplication** between `parallel.rs::synth_rec` and integration `synth_record`: acceptable.
  The unit test lives in the lib crate and cannot reach the integration-test helper; the only
  difference (`synth_rec` derives `seq` from XM length vs `synth_record` taking explicit `seq`)
  is benign. Low.
- **Import-block placement:** the new `use` block (`parallel.rs:1151-1158`) sits mid-module
  rather than grouped with `use super::*;` at the top of the `tests` mod (`:1071`). Compiles
  cleanly and clippy is silent (so the imports are needed, not redundant), but conventionally
  these would live at the top of the module. Purely cosmetic. Low.
- Naming is clear and self-documenting; doc-comments accurately state the guarded line numbers
  and the revert/divergence semantics.

---

## Issues (prioritised)

**Critical / High / Medium:** none.

**Low:**
1. *(Process, not code)* **Shared-worktree race during review.** Concurrent revert-smoke runs by
   two reviewers in the same worktree corrupt each other's test state (I observed `call.rs:204`
   toggling mid-run, producing a spurious Test 3 failure, and an `Edit`-conflict error). This is
   a harness/process-isolation gap, not a defect in the #878 code. Recommendation: future
   dual-review of mutating smokes should use **separate worktrees per reviewer**. I worked around
   it via an isolated worktree; the live tree is confirmed back to clean intended state.
2. **`BUG_876_FIXES_PLAN.md §6` not yet updated.** The plan's implementation-outline step 4
   ("mark §6 deferrals #8/#9/#10 as landed") is **not done** in this diff. However,
   `TESTS_878_PLAN.md:255` explicitly defers this to PR time ("Remaining: update
   `BUG_876_FIXES_PLAN.md §6` ... at PR time"), and it is a doc-only task outside the
   tests-only code scope. Flagging for tracking, not blocking.
3. *(Cosmetic)* Mid-module import block placement (see Structure above).

---

## Recommendations

- **Ship it.** The tests are correct, the fixtures are independently verified, and the guards
  are demonstrably real (revert-smoke + two mis-wire smokes all behave as designed).
- Before/at PR: complete the `BUG_876_FIXES_PLAN.md §6` doc-update (Low #2).
- Process: adopt per-reviewer worktrees for any review that runs mutating smokes (Low #1).
- Optional polish: move the `parallel.rs` tests-mod `use` block to the top of the module (Low #3).

---

## Key `file:line` references
- Fix under guard: `rust/bismark-extractor/src/call.rs:204`.
- M-bias accumulators guarded: `src/parallel.rs:710-711` (SE), `:814-815` (PE R1),
  `:837-838` (PE R2 → `mbias[1]`, `ignore_5p_r2` via `:791`).
- New tests: `src/call.rs:330` (Test 1), `src/parallel.rs:1232` (Test 2), `:1282` (Test 3),
  `tests/parallel_phase_f.rs:438` (Test 4).
- New helpers: `synth_se_record_strand` (`call.rs:243`), `synth_rec`/`config_with`/`mbias_total`
  (`parallel.rs:1160/1192/1208`).
- Orientation sources independently checked: `bismark-io/src/strand.rs:65-68`,
  `bismark-io/src/record.rs:299-307`, `bismark-io/src/pair.rs:40-78/142`,
  `src/overlap.rs:99-113`, `src/mbias.rs:51-63`, `src/cli.rs:432-436/484-500`.

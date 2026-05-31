# Code Review — #878 regression-guard tests (Reviewer A)

**Verdict: APPROVE.** All 4 tests are correct, the fixtures match what `extract_calls`/`process_*` actually produce, the regression guards genuinely fire on revert (verified by running the smoke), and there are no source logic changes. No Critical/High findings. A few Low/Medium polish notes only.

**Scope reviewed:** uncommitted diff in worktree `/Users/fkrueger/Github/Bismark-extractor` across 3 files (`call.rs`, `parallel.rs`, `tests/parallel_phase_f.rs`), verified against the actual semantics of `call.rs:extract_calls`, `bismark-io record.rs:iter_aligned`, `bismark-io strand.rs:from_xr_xg`, `bismark-io pair.rs:from_mates`, `parallel.rs:process_se/process_pe`, `mbias.rs:accumulate`, `overlap.rs:drop_overlap`, and `cli.rs:validate`.

---

## Verification performed

### Build / test (sandbox-disabled)
- `cargo test -p bismark-extractor`: **lib 105 + parallel_phase_f 18**, all green (matches plan). Total 17 "test result: ok" lines, 0 FAILED.
- `cargo clippy --all-targets -- -D warnings`: **clean** (no warnings).
- `cargo fmt --check` (from `rust/`): **clean** (exit 0).

### Revert-smoke (the #878 acceptance gate) — RAN IT MYSELF
Temporarily set `call.rs:204` → `read_pos: aligned.read_pos_5p` (drop the rebase):
- `extract_calls_ob_strand_rebases_read_pos_after_ignore_5p` — **FAILED** ✓
- `parallel_se_worker_m_bias_rebased` — **FAILED** ✓
- `parallel_pe_worker_m_bias_uses_r2_ignore_for_r2` — **FAILED** ✓
- (pre-existing `extract_calls_rebases_read_pos_after_ignore_5p` OT guard — also FAILED, expected) ✓
- `se_driver_vs_parallel_driver_m_bias_equality` (Test 4) — **stayed GREEN** ✓ (confirms it is a *divergence* guard, not a revert guard, exactly per A-I4).

Restored `call.rs:204`. Re-ran: all green.

### R2-ignore mis-wire smoke — RAN IT MYSELF
Temporarily set the R2 accumulate site (`parallel.rs:838`) `mbias[1]` → `mbias[0]`:
- `parallel_pe_worker_m_bias_uses_r2_ignore_for_r2` — **FAILED** ✓ (the `mbias[1].cpg[1].meth == 1` assert + `mbias_total(&mbias[1]) == 1` catch the wrong-table write).
Restored.

> Note: the source files were touched by an auto-format/save hook between my temporary edits; I re-verified `git diff` ends with **zero source-logic lines** added or removed (`read_pos: aligned…` and `mbias[N].accumulate` appear in neither `+` nor `-`). Final `git status` shows only the 3 intended files; diff stat `+320/-7`, test-only.

---

## Fixture correctness (the #1 risk) — all verified

### Test 1 (OB byte-reversal)
- `from_xr_xg` (`strand.rs:65-68`): `(CT,CT)→OT` forward, `(CT,GA)→OB` reverse. ✓
- `iter_aligned` (`record.rs:299-307`): for `-`-strand, remaps `read_pos_5p = seq_len-1-BAM`, then `reverse()`s order. **XM byte is taken as-is** (`xm[ap.read_pos]`, `record.rs:290`) — NOT complemented. The fixtures rely on this, and it holds. ✓
- OT `"..Zxh."` (forward): surviving BAM 2,3,4 → `Z`(CpG,T)@2, `x`(CHG,F)@3, `h`(CHH,F)@4; `ignore_5p=2` rebases to `[0,1,2]`. ✓
- OB `".hxZ.."` (= `reverse("..Zxh.")`): `Z`@BAM3→`read_pos_5p=5-3=2`, `x`@BAM2→3, `h`@BAM1→4; after reverse, emitted in 5'-order as `Z,x,h` at `read_pos_5p [2,3,4]`; rebase→`[0,1,2]`. ✓ The asserted contexts `(CpG,true),(CHG,false),(CHH,false)` match. The fixture is **correct**, not plausible-but-wrong.

### Test 2 (SE)
`"...Zxh"` OT, `--ignore 3`: surviving read_pos_5p 3,4,5 (`Z`,`x`,`h`) → rebased 0,1,2 → 1-based slots 1,2,3. Asserts `cpg[1].meth=1, chg[2].unmeth=1, chh[3].unmeth=1`; absolute (reverted) slots `cpg[4]/chg[5]/chh[6]` checked zero via `.get(n).map_or(0,…)`. Includes non-CpG (CHG+CHH) — exercises `accumulate` context routing. `mbias[1]` asserted empty (SE). ✓ The `.get(n)` checks are correct: under the fix the Vec grows only to index 3, so `.get(4..6)`→`None`→0; under revert it grows to 6 and `.get(4)`→`Some{meth:1}`→fails. Bidirectional. ✓

### Test 3 (PE R2-is-CTOT reversed)
- `from_mates` (`pair.rs:40-78`): requires R1 identity (flag 0x40) + R2 identity (flag 0x80); fixtures use `0x41`/`0x81`. Pair strand from R1 = OT. R2 `(GA,CT)` = CTOT (reverse). ✓ (mirrors the in-tree `from_mates_ot_pair` test exactly.)
- R1 `"...Z.."` OT start=100, `--ignore 3` (`ignore_5p_r1`): `Z`@read_pos_5p 3 → rebase 0 → `mbias[0].cpg[1].meth=1`. ✓
- R2 `".Z......."` CTOT start=200, 9M, `--ignore_r2 7` (`ignore_5p_r2`): `Z`@BAM1 → `read_pos_5p = 9-1-1 = 7` → ref_pos = 200+1 = **201**; rebase by 7 → 0 → `mbias[1].cpg[1].meth=1`. ✓ The R2-ignore (`config.ignore_5p_r2`, used at `process_pe` `:791`) and the R2 table index (`mbias[1]`, `:838`) are both exercised.
- Overlap: pair is forward (`is_forward_pair_strand(OT)=true`); `drop_overlap` keeps R2 calls with `ref_pos > r1_ref_end`. r1_ref_end = 100+6-1 = 105; R2 call ref_pos = 201 > 105 → kept. ✓ Non-overlapping placement is correct; R2 survives without `--include_overlap`.
- Cross-table leak guards: `mbias[0].cpg.get(4)`==0, `mbias[1].cpg.get(8)`==0, and `mbias_total` per table == 1. ✓ Each fixture has exactly one call so totals are exact.

### Test 4 (divergence guard + non-empty)
- `write_se_directional_bam` emits **5-bp** reads (`b"ACGTC"`). `--ignore 5` → `lo=5 >= hi=5` → empty (the C1 trap). `--ignore 2` → lo=2<hi=5; surviving calls = r_OT_2(2) + r_OT_3(1) + r_OB_1(1) + r_OB_2(1) = **5** (r_OT_1 `"Zz..."` contributes 0). Non-empty. ✓
- Non-emptiness scan: split files are plain text by default (gzip only under `--gzip`, not passed); header = `"Bismark methylation extractor version v0.25.1\n"` (`output.rs:36`), so `!line.starts_with("Bismark")` correctly excludes only the header; prefixes `CpG_/CHG_/CHH_` match the default-mode split file names. The `call_lines > 0` assertion is non-vacuous and prevents the C1 silent-degradation. ✓
- Correctly documented as guarding driver divergence, not revert. ✓

---

## Issues by area

### Logic
None.

### Efficiency
None (test-only; fixtures are tiny).

### Errors
None. `config_with`'s tempfile lifetime is **sound**: `validate()` only checks `path.exists()` (`cli.rs:431-436`), never opens the file; the tmp is alive through the parse+validate call and dropped afterward; `cfg` retains only the `PathBuf`, which `process_*` never reads. (Verified.)

### Structure / style
- **Low — helper duplication.** `parallel.rs::synth_rec` (6 args) duplicates most of `tests/parallel_phase_f.rs::synth_record` (7 args, adds `seq`). They live in different test scopes (unit vs integration crate) so can't share without a `tests/common/mod.rs` — which the integration file already flags as a deferred TODO (`parallel_phase_f.rs:44-46`). The unit-side `synth_rec` could not reuse the integration helper regardless. Acceptable; matches existing pattern. No action required.
- **Low — `synth_se_record` refactor is correctly non-breaking.** `synth_se_record(xm,n_soft,n_match)` now wraps `synth_se_record_strand(…, b"CT")`; the existing OT test (`extract_calls_rebases_read_pos_after_ignore_5p`) is unchanged in behavior (XG:Z:CT) and still passes. Doc-comment on the new `_strand` fn accurately describes `b"CT"→OT`, `b"GA"→OB`. ✓
- **Low — doc-comment line refs.** The Test 2/3 block comment cites `parallel.rs:711 / :815 / :838` for the accumulate sites; these match the current source (`:711` SE, `:815` PE-R1, `:838` PE-R2). Test 1's doc cites `record.rs:236` for the OB reversal — the formula is documented at `record.rs:236` and implemented at `:305`; close enough, accurate in spirit. No fix needed.

---

## Recommendations (priority-ranked)

- **Critical:** none.
- **High:** none.
- **Medium:** none.
- **Low (optional, non-blocking):**
  1. If a future PR adds `tests/common/mod.rs`, fold `synth_rec`/`synth_record`/`synth_se_record_strand` into one parameterized builder (already a tracked deferral).
  2. Consider asserting `mbias_total(&mbias[0]) == 3` in Test 2 (currently asserts the 3 specific slots + that 3 absolute slots are zero, which is equivalent but the explicit total would mirror Test 3's style). Cosmetic.

---

## Acceptance summary
- 4 tests added, all passing. ✓
- Tests 1–3 fail on `call.rs:204` revert (verified by running). ✓
- Test 4 stays green on revert, fails only on driver divergence (by design). ✓
- R2-ignore mis-wire caught by Test 3 (verified). ✓
- Non-emptiness guard prevents the C1 vacuous-pass. ✓
- Zero source `*.rs` logic changes (only the non-breaking `synth_se_record` test-helper refactor). ✓
- clippy `-D warnings` + fmt clean. ✓

**Report path:** `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/TESTS_878_CODE_REVIEW_reviewer-A.md`

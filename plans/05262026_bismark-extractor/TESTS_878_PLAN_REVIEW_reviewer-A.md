# Plan Review — TESTS_878_PLAN.md (Reviewer A)

**Plan:** `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/TESTS_878_PLAN.md`
**Issue:** #878 — add 4 regression-guard tests (no source changes) locking the #876 Bug B `MethCall.read_pos` rebase into CI.
**Code reviewed at:** worktree `/Users/fkrueger/Github/Bismark-extractor`, HEAD `2bfe722` (newer than the plan's drafting tip — line refs still hold, see below).
**Verdict:** **APPROVE WITH CHANGES.** The plan is well-grounded and feasible; all line refs and the central design (slot-1 vs slot-`ignore+1` discriminator) are confirmed against real code. **One Critical defect**: Test 4's `--ignore 5` produces *zero* surviving calls on the existing 5-base fixture, so it exercises nothing and is vacuously byte-identical. Two Important clarifications. Otherwise sound.

---

## 1. Logic review

### Line-ref verification (all confirmed against `2bfe722`)
| Plan claim | Actual | Status |
|---|---|---|
| rebase at `call.rs:204` | `call.rs:204` `read_pos: aligned.read_pos_5p.saturating_sub(ignore_5p)` | ✅ |
| `extract_calls` at `:152` | `:152` | ✅ |
| call.rs test mod `:222`, helper `synth_se_record`, OT test `:283` | `:222`, `:243`, `:283` | ✅ |
| OB invariant doc `:131-134` | `:131-134` | ✅ |
| `process_se` `:659` / `process_pe` `:742` (private) | `:659` / `:742`, both `fn` (no `pub`) | ✅ |
| M-bias sites `:711` SE / `:815` PE-R1 / `:838` PE-R2 | exact | ✅ |
| `mbias.rs:51` 1-based `accumulate`, slot 0 unused | `:51`, `debug_assert!(>=1)`, `pub cpg/chg/chh: Vec<MbiasPos>`, `pub meth/unmeth` | ✅ |
| single-threaded consumer `route.rs:95` | `:95` `pos_1based = call.read_pos.saturating_add(1)` | ✅ |
| `parallel.rs` test mod `:1069` | `:1069` | ✅ |
| `tests/parallel_phase_f.rs` helpers `synth_record:57`, `write_se_directional_bam:86`, `write_pe_directional_bam:156`, `resolved_config:244`, `assert_dirs_byte_identical:290`, template `legacy_vs_parallel_n4_se_default_byte_identical:392` | all exact | ✅ |
| `se_phase_b.rs` OB fixtures `:107`, OB tests `:299-317` | `ob_record` `:108`, `extract_calls_minus_strand_orients_*` `:297-334` | ✅ (plan says `:107`; actual `:108`, off-by-one, cosmetic) |

The consumer comment inside `call.rs:193-194` references the **old** parallel.rs lines `625/729/752`; the plan correctly uses the **current** `711/815/838`. No action — that stale in-source comment is out of scope for a tests-only PR.

### The core discriminator is correct
`accumulate` is strictly 1-based (`mbias.rs:51-71`); workers compute `pos_1based = call.read_pos + 1` (`parallel.rs:710/814/837`). With the fix, the first surviving call has `read_pos == 0` → slot **1**; reverted, `read_pos == ignore_5p` → slot **`ignore+1`**. The plan's "slot 1 vs slot ignore+1, assert both" design is the right bidirectional guard and matches BUG_876 §6 decision T1. ✅

### Test 1 (OB orientation) — verified correct, this was the flagged risk
I independently grounded the OB reasoning against `bismark-io/src/record.rs:263-311` (`iter_aligned`):
- Forward (OT/CTOB): `read_pos_5p == BAM read_pos`.
- `-`-strand (OB/CTOT): `read_pos_5p = seq_len - 1 - BAM_read_pos`, then the `Vec` is **reversed** (`:299-307`), so the first emitted item is `read_pos_5p == 0` at the sequenced 5' end.
- `seq_len = xm.len()` includes soft-clip; soft-clip read positions are counted (only `ref_offset?` filters them from emission), so the OB formula uses the full length.

Cross-checked against the existing fixture `extract_calls_minus_strand_orients_both_calls` (`se_phase_b.rs:315-334`): `ob_record(b"Zh...", ...)`, 5M, seq_len 5 → BAM pos 0 (`Z`) emits last at `read_pos_5p = 4`; BAM pos 1 (`h`) emits at `read_pos_5p = 3`. The test asserts exactly that. The plan's instruction to "reason in sequenced-5' order, mirror se_phase_b.rs:304-317" is therefore **correct**, and the OT≡OB invariance claim holds: a fixture whose XM, *written in sequenced-5' order*, is identical on both strands will produce identical `read_pos` sequences. **No correctness problem in Test 1's design** — but see Important I-2 (the fixture must be authored in BAM-stored order, which is reversed from the order the assertions read).

### Tests 2–3 (parallel worker) — feasible, but the plan's signature sketch is incomplete
The plan sketches `process_se(&record, &config, …, false, false, &mut mbias, …)`. The **real** signature (`parallel.rs:659-668`) is:
```
fn process_se(record: &BismarkRecord, chr_id: u32, chr_table: &Arc<[String]>,
              config: &ResolvedConfig, mbias_only_silence: bool, mbias_only: bool,
              mbias: &mut [MbiasTable; 2], report: &mut SplittingReport) -> Result<Vec<RoutedCall>, _>
```
`process_pe` (`:742`) is the same shape with `pair: &BismarkPair` instead of `record`. The plan's "…" hides three **required** args: `chr_id: u32`, `chr_table: &Arc<[String]>`, `report: &mut SplittingReport`. All are trivially constructible in a unit test:
- `chr_id = 0`, `chr_table = Arc::from(vec!["chr1".to_string()].into_boxed_slice())` (already done at `parallel.rs:1177`).
- `report = SplittingReport::default()` (`state.rs` uses `SplittingReport::default()`; confirmed it derives Default).

This is **non-blocking** — the plan's Assumption 1 already says "if their signatures need other args … construct minimal ones." Recommend the plan name `chr_id`/`chr_table`/`report` explicitly so the implementer doesn't have to rediscover them (Important I-1).

### Test 3 — wrong-table/wrong-slot cross-checks are sound
R1 → `mbias[0]` (`:815`), R2 → `mbias[1]` (`:838`), with `config.ignore_5p_r2` driving R2's `extract_calls` (`:791`). Asserting "R2 lands in `mbias[1]` slot 1, and `mbias[0]` is unchanged by R2; rebased by 7 not 3" guards all three failure modes (rebase revert, R1-ignore-applied-to-R2, wrong-table-index). ✅ One caveat: PE default has `no_overlap = true` (`cli.rs:484`), so `drop_overlap` runs on R2 calls (`parallel.rs:795-799`). The fixture must place R1 and R2 calls at **non-overlapping ref positions** or pass `--include_overlap`, else R2 calls may be silently dropped and the R2 assertion sees nothing. Flag this (Important I-3).

---

## 2. Assumptions audit

| Plan assumption | Verdict |
|---|---|
| A1: `process_se`/`process_pe` callable from `parallel.rs` test mod via `super::*` | ✅ Confirmed — they are module-private `fn`s; the test mod is `mod tests { use super::*; }` (`:1071`). Existing tests already call module-private items (`worker_loop`, `update_best_err`). |
| A2 (flagged main risk): `ResolvedConfig` with `--ignore`/`--ignore_r2` constructible in unit test | ✅ **Strongly confirmed** — the parallel.rs test mod *already* builds configs via `Cli::try_parse_from([...]).validate()` (`:1173-1174`, `:1263-1273`). All `ResolvedConfig` fields are `pub` (`cli.rs:271-300`: `ignore_5p_r1:279`, `ignore_5p_r2:283`, `mbias_off:300`). The plan's "fallback to direct construction" is **unnecessary** — the CLI-parse path is the established idiom. Remove the fallback hedge or downgrade it to a footnote. |
| CLI flag mapping `--ignore→ignore_5p_r1`, `--ignore_r2→ignore_5p_r2` | ✅ `cli.rs:498-501`; flags are `--ignore`/`--ignore_r2`/`--ignore_3prime`/`--ignore_3prime_r2` (`:56-69`). |
| XM byte→context mapping `Z/z`=CpG, `X/x`=CHG, `H/h`=CHH | ✅ `call.rs:94-99`. |
| OB `iter_aligned` reverses | ✅ `record.rs:299-307`. |
| dev-deps (`noodles-sam`, `noodles-core`, `bstr`) available for synthetic records in tests | ✅ `Cargo.toml [dev-dependencies]`. |

**Unstated assumption the plan should make explicit:** Tests 2–3 with `mbias_only=false` will execute the `RoutedCall` emission path, which calls `compute_yacht_columns` (`route.rs:38`). That's safe only because default `output_mode != Yacht` returns `(0,0)` early (`route.rs:43-45`). If the implementer ever passes `--yacht` it would need `alignment_start` set. Cleaner: pass `mbias_only=true` so the worker skips `RoutedCall` emission entirely (`parallel.rs:715-718/819-821`) and the test isolates M-bias accumulation. Recommend the plan say `mbias_only=true` for Tests 2–3 (Optional O-1).

---

## 3. Efficiency

- 4 tests, no source changes, reuse of established fixtures/helpers — appropriately minimal. ✅
- Tests 2–3 as in-memory unit tests (no fixture file, no reader/collector spin-up) are the fastest option and assert the slot directly. Good call vs the integration fallback.
- Test 4 reuses `write_se_directional_bam` + `assert_dirs_byte_identical` — cheap.

No efficiency concerns.

---

## 4. Validation sufficiency

**Confirmed strong:**
- `assert_dirs_byte_identical` (`parallel_phase_f.rs:290-324`) compares **all** files including `M-bias.txt` strictly (only `*_splitting_report.txt` is path-normalized). ✅ Plan's claim holds.
- No existing `legacy_vs_parallel_*` test uses nonzero `--ignore` (grep confirmed; the only test `--ignore` use is `sanity.rs:50`, unrelated). So **Test 4 is not redundant** — it is the sole nonzero-ignore cross-driver cell. ✅
- Revert-smoke gate (Validation §2) correctly predicts Tests 1–3 FAIL and Test 4 MAY pass on revert (both drivers share the bug). The plan is honest that Test 4 is a *structural cross-driver* guard, not a revert-fail guard. ✅
- CHG/CHH coverage called out in Self-Review (not just CpG) — good; `MbiasTable` has separate `cpg/chg/chh` vecs so context routing must be exercised.

**Gaps:**
- **Critical C-1 (below)** breaks Test 4's validation entirely under `--ignore 5`.
- The issue/BUG_876 acceptance says "each test fails on revert." Test 4 *by design* does not fail on revert. This is defensible (it's a different guard class) but the plan should state this divergence from the literal acceptance text explicitly so the verifier doesn't flag Test 4 as failing acceptance. (Important I-4.)
- Not covered, and arguably should be acknowledged as out-of-scope: `--mbias_off` (skips accumulation — `parallel.rs:709/813/836`), `ignore_3prime` interaction with the rebase (3' trim does not change `read_pos`, only the `hi` bound — so no rebase interaction, safe to omit but worth a one-line note), soft-clip + OB combined (existing `extract_calls_rebase_combined_with_soft_clip` covers OT+softclip; an OB+softclip case is the only genuinely untested rebase permutation, but it lives in bismark-io's `iter_aligned` cigar tests, not here). I'd accept omitting these for a 4-test guard, but the plan should say so.

---

## 5. Critical / Important / Optional findings

### Critical
- **C-1 — Test 4's `--ignore 5` exercises nothing on the 5-base fixture; it is vacuously byte-identical and guards nothing.**
  `write_se_directional_bam` (and `write_pe_directional_bam`) emit **only 5-base reads** (`b"ACGTC"`, XM length 5, all `5M`). In `extract_calls` (`call.rs:161-168`): `lo = ignore_5p = 5`, `hi = xm_len - ignore_3p = 5`, so `lo >= hi` → **early return `Ok(Vec::new())`** for *every* record. M-bias.txt has zero data rows in both runs → byte-identical with OR without the fix → the rebase path is never touched.
  I enumerated surviving calls per ignore value across all 5 SE records: `--ignore 5` → **0 surviving calls** for every record (and `--ignore 4` → only `r_OT_2`/`r_OB_1` survive at one position each).
  **Fix:** use `--ignore 1` (max surviving calls: r_OT_1→slot1, r_OT_2→slots2,4, r_OT_3→slot2, r_OB_1→slot4, r_OB_2→slot2) or `--ignore 2` (r_OT_2→slots1,3, r_OT_3→slot1, r_OB_1→slot3, r_OB_2→slot1 — note r_OT_1's two calls at pos 0,1 are *both* trimmed). `--ignore 1` keeps the most non-trivial data and the widest slot spread; recommend `--ignore 1` (or `2`). **Do not use `--ignore 5`.**
  *Root cause note:* the `--ignore 5` value was inherited verbatim from BUG_876_FIXES_PLAN §6 test #10, which itself never reconciled the value against the fixture length — a latent error now propagated. Worth a line in the plan's deviation log.

### Important
- **I-1 — Name the three hidden `process_se`/`process_pe` params.** The plan's signature sketch omits `chr_id: u32`, `chr_table: &Arc<[String]>`, `report: &mut SplittingReport` (all required; see §1). Add: `chr_id=0`, `chr_table = Arc::from(vec!["chr1".to_string()].into_boxed_slice())`, `report = SplittingReport::default()`. (`parallel.rs:659-668/742-751`, `state.rs`.)
- **I-2 — Test 1 fixture must be authored in BAM-stored order, asserted in 5'-order.** The OB `read_pos` the assertions read is `seq_len-1-BAM_pos` after reversal. The plan says "reason in sequenced-5' order" (correct for assertions) but the implementer writes the XM bytes in **BAM order**. Spell out: "place the intended-5'-first call at the *last* BAM XM index." Mirror `se_phase_b.rs:315-334` exactly. A plausible-but-wrong fixture here is the single most likely silent error.
- **I-3 — Test 3 PE fixture must avoid the overlap drop.** PE default `no_overlap=true` (`cli.rs:484`) runs `drop_overlap` on R2 (`parallel.rs:795-799`). Place R1 and R2 at non-overlapping reference positions (as `write_pe_directional_bam` does — R1@100, R2@110 etc.) or pass `--include_overlap`, else the R2 `mbias[1]` assertion may see zero calls for the wrong reason.
- **I-4 — State Test 4's divergence from literal acceptance.** Issue acceptance is "each test fails on revert"; Test 4 does not (by design, both drivers share the bug). The plan acknowledges this in Validation §2 but should also note it where the verifier checks acceptance, so Test 4 isn't logged as failing the acceptance criterion.

### Optional
- **O-1 — Pass `mbias_only=true` for Tests 2–3** to skip `RoutedCall`/`compute_yacht_columns` emission and isolate M-bias accumulation (counters still increment under `mbias_only`; `parallel.rs:707-712` runs before the `mbias_only` `continue`). Simpler, no need to set `alignment_start` for yacht.
- **O-2 — Drop the "construct `ResolvedConfig` directly" fallback** in Assumption 2. The CLI-parse path is already the established idiom in the same test module (`:1173`, `:1263`); the fallback is dead weight.
- **O-3 — Add one sentence** noting `ignore_3prime`/`--mbias_off`/OB+soft-clip are deliberately out of scope (3' trim doesn't touch `read_pos`; `mbias_off` short-circuits accumulation; OB+soft-clip is covered by bismark-io cigar tests), so the verifier doesn't flag them as gaps.
- **O-4 — Fix the cosmetic `:107`→`:108` ref** for `ob_record` (minor).

---

## 6. Alternatives considered
- **Test 4 as a unit-level `process_se` vs `extract_se`-state equality test** (as BUG_876 §6 test #10 originally envisioned with custom synthetic records) would let the implementer pick a read length and ignore value that *guarantees* surviving calls, sidestepping C-1's fixture-length trap. But the plan's integration form is fine once the ignore value is fixed; no need to change the form.
- The plan could fold Test 4 into a parameterized variant of `legacy_vs_parallel_n4_se_default_byte_identical` (same body, `--ignore 1`), reducing duplication. Cosmetic.

---

## 7. Bottom line
Design is correct and feasible; every load-bearing line ref and the slot-discriminator mechanism check out against real code, and Assumption 2 (the flagged risk) is solidly satisfied by an existing idiom. **Must-fix before implementation: C-1 (change Test 4 `--ignore 5` → `1` or `2`).** Address I-1..I-4 to de-risk implementation. Then this is a clean, low-risk tests-only PR.

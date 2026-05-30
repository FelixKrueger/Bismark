# Plan Review — TESTS_878_PLAN.md (Reviewer B)

**Plan:** `plans/05262026_bismark-extractor/TESTS_878_PLAN.md`
**Scope:** 4 regression-guard tests, no source `*.rs` changes, locking #876 Bug B (`call.rs:204` `read_pos` rebase) into CI.
**Verdict:** **APPROVE WITH CHANGES.** The overall strategy is sound and the placement decisions (unit-in-`call.rs`, unit-in-`parallel.rs`, integration) are all viable against the real code. But **Test 4 as written is a silent-pass no-op** (Critical), and **Test 1's OT≡OB equivalence omits the load-bearing byte-reversal detail** that is exactly where a plausible-but-wrong test would slip through (Important). All claims about callability/constructibility check out.

All line refs below verified against the worktree `/Users/fkrueger/Github/Bismark-extractor`.

---

## 1. Logic review

### Test 1 — OB/`-`-strand rebase (unit, `call.rs`)
The orientation model is confirmed real, not an artifact. `bismark-io/src/record.rs:299-307`: for `-`-strand (OB/CTOT) records, `iter_aligned` remaps `read_pos_5p = seq_len - 1 - BAM_read_pos` **and** reverses the call vector, so the first emitted item sits at `read_pos_5p == 0` and corresponds to the **last** BAM byte. The ignore filter at `call.rs:179` operates on this already-5'-oriented `read_pos_5p`, so with `ignore_5p=N` the first surviving OB call has `read_pos_5p == N`, rebased by `:204` to `0`. On revert it becomes `N` (absolute) → assertion fails. **The OB guard is genuinely bidirectional and works identically to OT.** ✓

The existing OB tests `extract_calls_minus_strand_orients_5prime` (`se_phase_b.rs:297-313`) and `extract_calls_minus_strand_orients_both_calls` (`:315-334`) already prove orientation at `--ignore 0`, but **neither stacks a non-zero `--ignore` onto OB** — so the rebase-on-OB cell is a genuine gap. Test 1 is not redundant. ✓

**However — the OT≡OB equivalence claim is under-specified and is the trap the brief warned about.** "the same logical XM (oriented identically in sequenced order) yields identical `read_pos` sequences on OT and OB" is *true*, but to realize it the OB record's **BAM-stored XM must be the byte-reverse of the OT record's BAM-stored XM** (because `iter_aligned` reverses OB back to sequenced order). The plan never states this. Concretely: OT BAM XM `zXhZxH` ⟺ OB BAM XM `HxZhXz`. An implementer who naively passes the *same* BAM XM string to both an OT and OB builder will get **reversed `read_pos` sequences**, and either (a) the assertion fails for the wrong reason, or (b) — worse — if they pick a palindromic/symmetric fixture, the test passes while asserting nonsense. This must be spelled out in the plan, not deferred to "reason it at impl time."

### Test 2 — `parallel_se_worker_m_bias_rebased` (unit, `parallel.rs`)
`process_se` (`parallel.rs:659`) signature is `(record, chr_id, chr_table, config, mbias_only_silence, mbias_only, mbias, report)`. All args constructible in-module (see §2). The slot logic is correct: `process_se:710` computes `pos_1based = call.read_pos.saturating_add(1)`, and with the rebase a call surviving `--ignore 3` lands at `read_pos==0` → slot 1; on revert it lands at `read_pos==3` → slot 4. Asserting `mbias[0].<ctx>[1]` nonzero **and** `mbias[0].<ctx>[4]` zero is a bidirectional guard. ✓ One caveat: the fixture must have a read **longer than the ignore region** so the early-out at `call.rs:166` (`lo >= hi`) does not fire and zero the whole record — see Critical C1 for why this matters most acutely in Test 4.

### Test 3 — `parallel_pe_worker_m_bias_uses_r2_ignore_for_r2` (unit, `parallel.rs`)
`process_pe` (`parallel.rs:742`) is reachable; `BismarkPair::from_mates(r1, r2)` (`bismark-io/src/pair.rs`) is the public pair constructor, usable in-module. The two failure modes are independent and both guarded: (1) R2-ignore value — `process_pe:789-794` calls `extract_calls(pair.r2(), config.ignore_5p_r2, …)`; (2) table index — `:838` writes `mbias[1]`. Asserting R1→`mbias[0]`[slot rebased-by-3] and R2→`mbias[1]`[slot rebased-by-7], with the cross cells zero, catches all of: rebase revert, R1-ignore-applied-to-R2, and `mbias[0]`-instead-of-`mbias[1]`. ✓

**Not redundant** with existing coverage: `route_call_r2_goes_to_mbias_index_1` (`se_phase_b.rs:901-930`) only exercises the **single-threaded** `route.rs` path with a hand-built `MethCall{read_pos:5}` (no ignore rebase). `extract_pe_per_mate_ignore_r2_only_skips_r2_positions` (`pe_phase_c.rs:1142`) verifies the R2-ignore *filter* via split-file output, not the M-bias *slot*. Test 3 is the **only** test touching parallel `process_pe`'s M-bias accumulation with distinct R1/R2 ignore — keep it. ✓

**Subtlety to flag for the implementer:** for a PE pair, R1 must be `+`-strand and R2 `-`-strand (or vice versa) for both to yield non-empty calls; the rebase for R2 happens in `read_pos_5p` (already 5'-oriented). Pick the fixture so R2's *surviving* call after `--ignore_r2 7` is unambiguous, and ensure R2 reads are >7 bp. Also: with `--no_overlap` default ON, `process_pe:795` calls `drop_overlap` — the plan's `process_pe(…)` call must use a config where R1/R2 don't overlap (distinct ref positions), or pass `--include_overlap`, else R2 calls may be silently dropped and the test asserts on an empty table. The plan does not mention this.

### Test 4 — `se_driver_vs_parallel_driver_m_bias_equality` (integration) — **BROKEN AS SPECIFIED**
The plan says: run `extract_se` vs `extract_se_parallel` on `write_se_directional_bam` with `--ignore 5`. **This is a silent-pass no-op.** `write_se_directional_bam` (`parallel_phase_f.rs:86-117`) writes **5-base reads** (`b"Zz..."`, `b"..X.x"`, `b"H.h.."`, `b"Z...."`, `b"..h.."`). In `extract_calls` (`call.rs:161-168`): with `ignore_5p=5`, `xm_len=5`, `ignore_3p=0` → `lo=5`, `hi=5`, `lo >= hi` → **returns empty `Vec` for every record**. Result: empty split files + header-only `M-bias.txt` (`max_position()==0`, `mbias.rs:114`) in BOTH dirs → `assert_dirs_byte_identical` trivially passes while exercising **zero** rebase logic. It would also pass with the fix fully reverted. The plan's own §Validation point 2 even concedes "Test 4 may still pass" on revert — but the real problem is it passes on revert **because it tests nothing**, not because "both drivers share the bug."

Fix: either (a) use `--ignore 2` (well under 5 bp, leaves surviving calls), or (b) introduce a longer fixture (≥8 bp reads) and use `--ignore 5`. Given the goal is "exercise the rebase under non-zero ignore," `--ignore 2` on the existing fixture is the minimal correct change. After fixing, also confirm the M-bias.txt actually contains non-zero data rows (otherwise it still proves nothing) — consider asserting `M-bias.txt` is non-trivial, or better, that the two dirs match **and** at least one split file is non-empty.

---

## 2. Assumptions

- **Assumption 1 (process_se/process_pe callable via `super::*`): VALID.** Both are private fns in `parallel.rs`; the existing `#[cfg(test)] mod tests` (`:1069`) already imports `super::*` and calls `worker_loop`, `update_best_err`, builds `InputBatch`/`WorkerInputItem`. `BismarkRecord`, `BismarkPair`, `MbiasTable`, `extract_calls`, `CytosineContext`, `SplittingReport` are all in module scope via the top-of-file `use` (`:73-82`). The test mod will need to add noodles imports for record-building (not currently present in the mod) — trivial, mirror `call.rs:233-238`.

- **Assumption 2 (ResolvedConfig constructible with `--ignore`/`--ignore_r2`): VALID — stronger than the plan claims.** The existing parallel.rs tests (`:1156`, `:1255`) already build a `ResolvedConfig` via `Cli::try_parse_from(args).unwrap().validate().unwrap()`, and `cli.rs:498-500` maps `--ignore`→`ignore_5p_r1`, `--ignore_r2`→`ignore_5p_r2`, `--ignore_3prime`→`ignore_3p_r1`. So the recommended unit placement is **definitely viable** — the plan's "main remaining risk" is over-stated; this should be downgraded from a flagged risk to "confirmed." One gotcha the plan omits: `Cli::validate()` checks input-file existence (`validate_rejects_input_file_not_found`), so the test must write a throwaway `.bam` tmpfile first — exactly as `:1164` and `:1260` already do.

- **Assumption (XM byte→context): VALID** per `classify_xm_byte` (`call.rs:88-109`).

- **Unstated assumption — fixture read length vs ignore:** The plan never states that fixtures must be longer than the ignore value. This omission is what makes Test 4 a no-op and is a latent trap for Tests 2/3 too.

- **Unstated assumption — PE overlap (`--no_overlap` default):** see Test 3 note above. `process_pe:795` applies `drop_overlap` unless `--include_overlap`.

---

## 3. Efficiency

No concerns. Four small tests, no source changes, reuse established fixtures/helpers. Test 4 is an extra full driver round-trip on a 5-record BAM — negligible. The unit tests (2/3) avoid disk I/O for the hot path (only the throwaway tmpfile for `validate()`), which is the right call over an integration variant.

---

## 4. Validation sufficiency

- **"Revert `call.rs:204` → tests fail" gate:** Sufficient for Tests 1/2/3 (all assert the absolute slot is zero, not just driver-equality). **Insufficient for Test 4** as written (it tests nothing). The plan's §Validation step 2 correctly predicts Tests 1-3 fail and Test 4 may pass, but mis-attributes Test 4's pass to "shared bug" rather than "empty fixture."
- **R2-specific mis-wire smoke (§Validation step 3):** Good — temporarily swapping `mbias[1]`→`mbias[0]` or R2-ignore→R1-ignore at `:838` and confirming Test 3 fails is the right adversarial check. Keep it.
- **Silent-pass risks identified:**
  1. Test 4 empty fixture (Critical C1).
  2. Test 1 reversed/palindromic OB fixture asserting nonsense (Important I1).
  3. Tests 2/3 where the surviving call's slot coincides between rebased and absolute — avoided as long as `ignore >= 1` and the asserted "absolute" slot (`ignore+1`) differs from slot 1, which it does for `ignore=3`/`7`. ✓ But add: assert the call **count** (e.g. `mbias[0].cpg[1].meth == 1`), not just non-zero, to catch a double-count or wrong-context routing.
  4. Context coverage: the plan's self-review commits to exercising ≥1 non-CpG context. Worth making this a hard assertion in Test 2 (e.g. a CHG call), since `accumulate` routes by context (`mbias.rs:56-60`) and a CpG-only fixture wouldn't catch a context-routing regression.

---

## 5. Alternatives

- **Table-driven OT/OB for Test 1:** A single parametric helper `assert_rebase_for_xg(xg: &[u8], bam_xm: &[u8], ignore, expected_read_pos: &[u32])` driven over `(CT-OT, sequenced_xm)` and `(GA-OB, reverse(sequenced_xm))` rows would make the byte-reversal explicit in code and structurally prevent the I1 trap. Recommended over two ad-hoc tests. The plan's "generalize `synth_se_record` to take `xg`" is a good first step but doesn't force the reversal.
- **Test 4 granularity:** Comparing whole dirs is acceptable (it's the established pattern and catches split-file divergence too), but the plan should add an explicit non-emptiness assertion so a future fixture/flag change can't silently re-empty it. Alternatively target `M-bias.txt` specifically with a "contains a non-zero data row" check — strictly stronger for this test's stated purpose.
- **Consider folding Test 4's value into a cross-N cell:** `parallel_se_byte_identical_across_n_1_2_4_8` (`:467`) could gain an `--ignore 2` variant, but the legacy-vs-parallel driver comparison (the dual-driver trap) is distinct and worth its own test. Keep Test 4, fix the ignore value.

---

## Action items

### Critical
- **C1 — Test 4 is a silent-pass no-op.** `write_se_directional_bam` uses 5-bp reads; `--ignore 5` triggers the `lo >= hi` early-out (`call.rs:166`) for every record → both dirs empty → trivially identical, passes even on full revert. Change to `--ignore 2` (or add an ≥8-bp fixture), and add a non-emptiness assertion (M-bias.txt has a non-zero data row, or a split file is non-empty). (`parallel_phase_f.rs:86-117`, `call.rs:161-168`)

### Important
- **I1 — Spell out the OB BAM-XM byte-reversal in Test 1.** The OT≡OB equivalence requires the OB record's BAM-stored XM to be the byte-reverse of the OT's (because `iter_aligned` reverses OB; `bismark-io/src/record.rs:305-307`). Without this stated, an implementer can pass the same string to both and assert reversed positions, or pick a symmetric fixture that passes while asserting nonsense. State the concrete fixture pair (e.g. OT `zXhZxH` ⟺ OB `HxZhXz`) and the expected `read_pos` sequence for each. Prefer a table-driven helper that forces the reversal.
- **I2 — Test 3 must control PE overlap.** `process_pe:795` applies `drop_overlap` unless `--include_overlap`. Use non-overlapping R1/R2 ref positions or pass `--include_overlap` so R2's call isn't silently dropped before it reaches `mbias[1]`. (`parallel.rs:795`)
- **I3 — State the fixture-length invariant.** Add an assumption: "all fixtures use reads longer than the largest ignore value." This is the root cause of C1 and a latent trap for Tests 2/3.

### Optional
- **O1 — Assert exact counts and a non-CpG context** in Tests 2/3 (`mbias[0].chg[1].meth == 1`, not just non-zero) to catch double-count / context-misroute regressions. (`mbias.rs:56-60`)
- **O2 — Downgrade Assumption 2's risk framing.** ResolvedConfig-via-CLI-parse in `parallel.rs` tests is already proven (`parallel.rs:1156, 1255`); the plan's "only thing that could push to integration fallback" is not a real risk. Note the throwaway-`.bam` requirement for `validate()`.
- **O3 — Table-driven OT/OB helper** (see Alternatives) to make Test 1 robust by construction.

---

## Summary table

| Test | Placement viable? | Bidirectional guard? | Redundant? | Issue |
|------|-------------------|----------------------|------------|-------|
| 1 OB rebase | ✓ unit call.rs | ✓ (once reversal correct) | No | **I1** reversal under-specified |
| 2 SE worker | ✓ unit parallel.rs | ✓ | No | I3 fixture length; O1 exact count/context |
| 3 PE worker | ✓ unit parallel.rs | ✓ | No | **I2** overlap; I3 fixture length |
| 4 driver equality | ✓ integration | ✗ as written | No (distinct cell) | **C1 no-op fixture/ignore** |

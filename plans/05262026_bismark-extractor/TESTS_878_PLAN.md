# Plan — #878: close CI coverage gap for `MethCall.read_pos` rebase (tests only)

**Epic:** `#798` (extractor release). Follow-up to #876 / PR #877.
**Issue:** #878. **Scope:** add 4 regression-guard tests; **no source `*.rs` changes.**

## Revision history
- **rev 1 (2026-05-29):** Folded dual plan-review (`TESTS_878_PLAN_REVIEW_reviewer-{A,B}.md`).
  Both APPROVE WITH CHANGES; both independently found the same Critical.
  - **C1 (both, Critical):** Test 4's `--ignore 5` was a silent no-op (5-bp fixtures →
    `lo>=hi` early-out → empty outputs → trivially byte-identical even on revert). → use
    **`--ignore 2`** + a **non-emptiness assertion**. Added the **fixture-length > ignore**
    invariant (B-I3) everywhere ignore is set.
  - **I (both):** Test 1 OB fixture = **byte-reverse** of OT (intended-5'-first call at the
    **last BAM index**); Test 3 must **control PE overlap** (non-overlapping ref positions).
  - **A-I1:** `process_se/pe` need 3 more args (`chr_id`, `chr_table`, `report`).
  - **A-I4:** Test 4 guards driver **divergence**, not revert — acceptance reconciled.
  - **Assumption 2 RESOLVED:** `ResolvedConfig` via `Cli::try_parse_from([...]).validate()`
    (precedent `parallel.rs:1173/1263`); the "construct directly" fallback is dropped.
  - Adopted: `mbias_only=true` + exact-count + non-CpG-context assertions for Tests 2–3.

## Goal
Lock the #876 **Bug B** fix (`MethCall.read_pos` rebased at construction) into in-repo
CI so a future refactor can't silently regress it. The fix is at `call.rs:204`:

```rust
read_pos: aligned.read_pos_5p.saturating_sub(ignore_5p)   // rebased ("0-based-after-clip")
// pre-fix was: aligned.read_pos_5p                         // absolute
```

CI currently exercises only the **OT/`+`-strand** path of `extract_calls` and **none**
of the 3 `parallel.rs` worker M-bias accumulator sites. Colossal Phase H (SE matrix +
PE smoke) verifies these on real data, but colossal is the manual release-walk gate, not
CI. This plan adds 4 tests covering (1) the OB/`-`-strand kernel path and (2) the parallel
worker M-bias dispatch, each engineered to **fail if the fix is reverted**.

## Context (code under test + existing patterns)

**Fix + its consumers** (all read `MethCall.read_pos`; M-bias uses `pos_1based = read_pos + 1`):
- Source of the rebase: `call.rs:204` (`extract_calls`).
- Single-threaded consumer: `route.rs:95`.
- Parallel worker consumers: `parallel.rs:711` (SE → `mbias[0]`), `:815` (PE R1 → `mbias[0]`),
  `:838` (PE R2 → `mbias[1]`). All compute `pos_1based = call.read_pos.saturating_add(1)`.

**Drivers / visibility (decides test placement):**
- `extract_calls(record, ignore_5p, ignore_3p, mbias_only_silence) -> Result<Vec<MethCall>>`
  — `pub`, `call.rs:152`. Has a `#[cfg(test)] mod tests` (`call.rs:222`) with helper
  `synth_se_record(xm, n_soft, n_match)` (hardcodes `XR:Z:CT` + **`XG:Z:CT`** = OT) and the
  OT template test `extract_calls_rebases_read_pos_after_ignore_5p` (`:283`).
- `process_se` / `process_pe` — **private** (`parallel.rs:659` / `:742`); take
  `&mut [MbiasTable; 2]`. Reachable only from `parallel.rs`'s own `#[cfg(test)] mod tests`
  (`:1069`). → Tests 2–3 are **unit tests inside parallel.rs**.
- `extract_se` / `extract_pe` — `pub`, single-threaded (`pipeline.rs:73` / `:220`).
- `extract_se_parallel` / `extract_pe_parallel` — `pub` (`parallel.rs:183` / `:192`).
  → Test 4 is an **integration test** in `tests/parallel_phase_f.rs`.

**M-bias table:** `MbiasTable::accumulate(context, position_1based, methylated)` is **1-based**
(`mbias.rs:51`; slot 0 unused). Counters at `mbias.cpg[slot]` / `.chg[slot]` / `.chh[slot]`
(`MbiasPos { meth, unmeth }`). So "first surviving call lands at **slot 1**" ⟺ rebase applied;
"lands at slot `ignore+1`" ⟺ rebase reverted.

**OB orientation (Test 1's subtlety):** for `-`-strand records (`XR:Z:CT` + `XG:Z:GA`),
`iter_aligned` walks the read from the **sequenced 5' end**, so `read_pos_5p` is counted in
sequenced order while `record.xm()` stays BAM-stored (`call.rs:131-134` invariant). Existing
OB orientation fixtures/tests to mirror: `tests/se_phase_b.rs:107, 299-317`.

**Integration fixtures to reuse** (`tests/parallel_phase_f.rs`): `synth_record(...)` (`:57`),
`write_se_directional_bam` (`:86`, already mixes OT+OB), `write_pe_directional_bam` (`:156`),
`resolved_config(args)` (`:244`, parses CLI → `ResolvedConfig`), `assert_dirs_byte_identical`
(`:290`, compares **all** output files incl. `M-bias.txt`). Template: `legacy_vs_parallel_n4_se_default_byte_identical` (`:392`).

## Behavior — the 4 tests

### Test 1 — OB/`-`-strand rebase (unit, `call.rs` tests mod)
Parameterize the record builder by XG (or add `synth_se_record_ob`). Build an OB record
(`XR:Z:CT`, `XG:Z:GA`) whose XM yields known calls; run `extract_calls(rec, ignore_5p=N>0, 0, false)`.
- **OB byte-reversal — MUST author the fixture correctly (dual-review I, the likeliest
  silent-error site).** `iter_aligned` walks OB records from the sequenced 5' end:
  `read_pos_5p = seq_len - 1 - BAM_index` (`bismark-io record.rs:299-307`). XM bytes are
  authored in **BAM order** but the assertions read **sequenced-5' order** — so the call you
  intend to be "5'-first" must sit at the **LAST BAM index** of the XM string. Concretely:
  to make OB assert the *same* `read_pos` sequence as an OT fixture `XM_OT`, the OB fixture's
  XM must be **`reverse(XM_OT)`**. Mirror the existing pattern at `se_phase_b.rs:315-334`.
- **Assert:** `read_pos` values are rebased (first surviving 5'-call → `0`), computed in
  sequenced-5' order.
- **Parametric OT≡OB:** OT with `XM_OT` and OB with `reverse(XM_OT)` yield **identical
  `read_pos` sequences** under the same `ignore_5p`. Orientation-invariance guard.
- **Fixture length > ignore_5p** (see invariant below) so ≥1 call survives.
- **Regression-guard property:** with the fix reverted, OB `read_pos` becomes absolute
  (`>= ignore_5p`) → assertion fails. (Bidirectional: also assert the count is correct.)

### Test 2 — `parallel_se_worker_m_bias_rebased` (unit, `parallel.rs` tests mod)
Construct a synthetic SE `BismarkRecord` with **XM length > 3** (so `--ignore 3` leaves ≥1
call; e.g. a 6-byte XM placing a CpG **and** a non-CpG call at sequenced positions ≥3), an
empty `let mut mbias = [MbiasTable::default(), MbiasTable::default()]`, and a `ResolvedConfig`
built via `Cli::try_parse_from(["…","--ignore","3","--mbias_only", <throwaway.bam>]).validate()`
(precedent `parallel.rs:1173/1263`; `--mbias_only` isolates accumulation from RoutedCall
emission). Call `process_se(&record, &config, chr_id=0, &chr_table, /*mbias_only_silence=*/true,
/*mbias_only=*/true, &mut mbias, &mut report)` (full signature below).
- **Assert (exact counts):** the surviving call lands at `mbias[0].<ctx>[1]` with the **exact**
  `{meth|unmeth}` count == 1 (slot **1**, rebased), and `mbias[0].<ctx>[ignore+1=4]` is **zero**.
  Include a **non-CpG (CHG or CHH)** call so the context routing in `accumulate` is exercised.
- **Regression-guard:** revert → count moves to slot 4 → assertion fails (bidirectional:
  slot 1 nonzero AND slot 4 zero).

### Test 3 — `parallel_pe_worker_m_bias_uses_r2_ignore_for_r2` (unit, `parallel.rs` tests mod)
Synthetic PE pair via the public `BismarkPair::from_mates(r1, r2)` constructor; `ResolvedConfig`
with `--ignore 3 --ignore_r2 7 --mbias_only` (CLI-parse idiom). Call `process_pe(&pair, &config,
chr_id=0, &chr_table, true, true, &mut mbias, &mut report)`.
- **Fixture-length invariant:** R1 XM length **> 3** and R2 XM length **> 7** (e.g. R1 6-byte,
  R2 ≥9-byte) so each mate leaves ≥1 surviving call after its own ignore.
- **Control PE overlap (dual-review I):** PE default is `no_overlap=true` → `process_pe`
  (`parallel.rs:795`) runs `drop_overlap` on R2. Place R1 and R2 at **non-overlapping reference
  positions** (distinct `alignment_start` + non-overlapping spans) — OR pass `--include_overlap`
  — so R2's call survives to `mbias[1]`. (Non-overlapping positions preferred: keeps the test
  about ignore-rebasing, not overlap policy.)
- **Assert:** R1's surviving call → **`mbias[0]`** slot 1 (rebased by 3); R2's surviving call →
  **`mbias[1]`** slot 1 (rebased by 7), exact count == 1. Cross-check the wrong table/slot is
  zero — guards the R2-ignore value (`:838` uses `ignore_r2`) AND the `mbias[1]` index.
- **Regression-guard:** revert rebase, or apply R1's ignore to R2, or write R2 into `mbias[0]`
  → assertion fails.

### Test 4 — `se_driver_vs_parallel_driver_m_bias_equality` (integration, `parallel_phase_f.rs`)
Run `extract_se` (single-threaded, `pipeline.rs`) and `extract_se_parallel` (parallel) on the
same `write_se_directional_bam` input with `resolved_config([... "--ignore", "2", ...])` into two
dirs; `assert_dirs_byte_identical` (compares `M-bias.txt` among all outputs).
- **⚠️ `--ignore` MUST be < read length (dual-review C1 — Critical).** `write_se_directional_bam`
  emits **5-bp** reads; `--ignore 5` trips the `lo >= hi` early-out (`call.rs:166`) → every
  record yields 0 calls → empty split files + header-only `M-bias.txt` → `assert_dirs_byte_identical`
  passes **trivially, even on a full revert** (tests nothing). Use **`--ignore 2`** (lo=2 < hi=5
  → ~3 surviving positions per record) **AND add a non-emptiness assertion** (≥1 data row in
  `M-bias.txt`, or a non-header line in a split file) so the cell can never silently degrade to
  the no-op.
- **Purpose:** the **dual-driver-trap structural guard** (`[[feedback_dual_driver_back_port]]`)
  — catches a fix landing in one driver but not the other under a non-zero `--ignore`.
- **Acceptance nuance (dual-review A-I4):** Test 4 guards **driver divergence**, NOT revert —
  both drivers share the rebase, so it stays green on a revert. The issue's "each test fails on
  revert" applies to **Tests 1–3**; Test 4 is the divergence guard (Tests 2–3 cover absolute
  correctness of the parallel path). State this in the test doc-comment.
- **Distinct from existing `legacy_vs_parallel_*` tests:** those run default (`--ignore 0`),
  which doesn't exercise the rebase. This adds the non-zero-`--ignore` cell.

## Implementation outline
1. **call.rs (Test 1):** generalize `synth_se_record` to take an `xg: &[u8]` arg (or add
   `synth_se_record_ob`); add `extract_calls_ob_strand_rebases_read_pos` + the parametric
   OT≡OB assertion. Reason the OB XM in sequenced-5' order per `call.rs:131-134` /
   `se_phase_b.rs:304-317`.
2. **parallel.rs tests mod (`:1069`) (Tests 2–3):** add the two unit tests calling `process_se` /
   `process_pe` and asserting `mbias[idx].<ctx>[slot]`.
   - **Full signatures (dual-review A-I1 — confirm at impl):**
     `process_se(record: &BismarkRecord, config: &ResolvedConfig, chr_id: u32,
     chr_table: &Arc<[String]>, mbias_only_silence: bool, mbias_only: bool,
     mbias: &mut [MbiasTable; 2], report: &mut SplittingReport) -> …`; `process_pe` takes a
     `&BismarkPair` instead of `&BismarkRecord`. Construct: `report = SplittingReport::default()`,
     `chr_table: Arc<[String]> = Arc::from(vec!["chr1".to_string()])`, `chr_id = 0`.
   - **Config:** `Cli::try_parse_from(["bin","--ignore","3", …, <throwaway.bam path>])
     .unwrap().validate().unwrap()` — the idiom already used at `parallel.rs:1173/1263`
     (needs a throwaway `.bam` path arg to satisfy the parser; a `tempfile` path is fine since
     `process_*` never reads the file, only `config`).
   - **PE pair:** `BismarkPair::from_mates(r1, r2)` (public constructor).
3. **tests/parallel_phase_f.rs (Test 4):** add `se_driver_vs_parallel_driver_m_bias_equality`
   mirroring `legacy_vs_parallel_n4_se_default_byte_identical` (`:392`) but with `--ignore 5`.
4. **Docs:** mark `BUG_876_FIXES_PLAN.md` §6 deferrals (#8/#9/#10) as landed; close #878's
   acceptance checklist.

## Assumptions
- **[CONFIRMED by dual review]** `process_se` / `process_pe` are callable from `parallel.rs`'s
  `#[cfg(test)] mod tests` via `super::*`; full signatures captured in Implementation outline §2.
- **[CONFIRMED by dual review — Assumption 2 resolved]** `ResolvedConfig` is constructible with
  `--ignore` / `--ignore_r2` via `Cli::try_parse_from([...]).validate()` (precedent
  `parallel.rs:1173/1263`); all `ResolvedConfig` fields are `pub` if direct construction is ever
  preferred. The earlier "construct directly as fallback" hedge is dropped — the CLI-parse idiom
  is the path. (Only caveat: the parser needs a throwaway `.bam` path arg.)
- **Fixture-length > ignore invariant [dual-review C1/I3]:** every test's fixture read length
  MUST exceed its `--ignore` (and R2 length > `--ignore_r2`), else `extract_calls` early-returns
  empty at `call.rs:166` (`lo >= hi`) and the test silently asserts over nothing.
- XM byte→context mapping: `Z/z`=CpG, `X/x`=CHG, `H/h`=CHH (upper=meth). Pick fixture XM so
  the surviving call's context is unambiguous for the slot assertion.
- **OB `iter_aligned` reversal [CONFIRMED]:** `read_pos_5p = seq_len-1-BAM_index`
  (`bismark-io record.rs:299-307`); OB fixture XM = byte-reverse of the OT fixture (Test 1).

## Validation (acceptance = Tests 1–3 fail when the fix is reverted; Test 4 = divergence guard)
1. **Green run:** `cargo test -p bismark-extractor` — all 4 new tests pass (+ existing 102+).
2. **Revert-smoke (the key acceptance gate, #878):** temporarily change `call.rs:204` to
   `read_pos: aligned.read_pos_5p,` → `cargo test` → confirm **Tests 1, 2, 3 FAIL** (slots
   shift to absolute). **Test 4 stays green by design** (both drivers share the rebase — it
   guards divergence, not revert; this is why Tests 2–3 assert *absolute* slots). Restore.
3. **Non-emptiness self-check (guards the C1 no-op class):** confirm Test 4's `M-bias.txt`
   actually has ≥1 data row (the test asserts this); a green Test 4 over empty output is the
   failure mode the dual review caught. Equivalently: each test's fixture length > its ignore.
4. **R2-ignore mis-wire smoke:** temporarily swap `mbias[1]`→`mbias[0]` or `ignore_r2`→`ignore`
   at `parallel.rs:838` → confirm **Test 3 FAILS**. Restore.
5. `cargo clippy --all-targets -- -D warnings` + `cargo fmt --check` clean.

## Questions or ambiguities
- **(Open, non-critical)** Tests 2–3 placement: unit-in-`parallel.rs` (recommended — asserts
  in-memory slots directly via the private fns) vs integration via `extract_*_parallel` + parse
  `M-bias.txt`. Recommend unit; fall back to integration only if `ResolvedConfig`/`process_*`
  proves unergonomic from the test module.
- **(Open)** Whether to extend `synth_se_record` in place vs add an OB variant — cosmetic.
- No **Critical** ambiguities: the issue specifies test names, inputs, and acceptance.

## Self-Review
- **Logic:** the `pos_1based = read_pos+1` mapping makes "slot 1 vs slot ignore+1" the precise
  discriminator; each test names the absolute (reverted) slot it checks is zero, so the guard is
  bidirectional. ✓
- **Edge cases:** Test 1 covers OB orientation (the gap Reviewer B flagged); Test 3 covers the
  R1≠R2 ignore + `mbias[0]`≠`mbias[1]` index (two independent failure modes); Test 4 covers
  cross-driver divergence. CHG/CHH (not just CpG) should appear in ≥1 fixture so the context
  routing in `accumulate` is exercised, not only CpG. ✓ (added to outline step 1/2)
- **Integration:** no source changes → zero risk to byte-identity; tests reuse established
  fixtures/assert helpers. ✓
- **Remaining risk:** `ResolvedConfig` construction ergonomics in `parallel.rs` unit tests
  (Assumption 2) — the only thing that could push Tests 2–3 to the integration-style fallback.
  Flagged, non-blocking.

---

## Implementation notes (2026-05-29 — DONE, all 4 tests landed)

**What was built (tests only, zero source `*.rs` logic changes):**
- **Test 1** (`call.rs` tests): refactored `synth_se_record` → `synth_se_record_strand(xm,
  n_soft, n_match, xg)` (non-breaking; `synth_se_record` is now a `b"CT"` wrapper) + added
  `extract_calls_ob_strand_rebases_read_pos_after_ignore_5p`. OT `"..Zxh."` vs OB
  `".hxZ.."` (= `reverse`), `ignore_5p=2`, both assert rebased `read_pos [0,1,2]` + OT≡OB.
- **Tests 2–3** (`parallel.rs` tests mod): added `synth_rec`, `config_with`, `mbias_total`
  helpers + `parallel_se_worker_m_bias_rebased` (`--ignore 3`, SE → `mbias[0]` slot 1) and
  `parallel_pe_worker_m_bias_uses_r2_ignore_for_r2` (`--ignore 3 --ignore_r2 7`, R1→`mbias[0]`
  slot 1, R2→`mbias[1]` slot 1). Called the private `process_se`/`process_pe` via `super::*`.
- **Test 4** (`tests/parallel_phase_f.rs`): `se_driver_vs_parallel_driver_m_bias_equality`
  (`--ignore 2`, `extract_se` vs `extract_se_parallel`, non-emptiness guard + `assert_dirs`).

**Deviations from the plan (documented per code-impl skill):**
1. **`process_se`/`process_pe` arg order** is `(record/pair, chr_id, chr_table, config,
   mbias_only_silence, mbias_only, mbias, report)` — `chr_id`/`chr_table` come *before*
   `config`, not after as the rev-1 outline sketched. Corrected in the tests.
2. **R2 of an OT pair is CTOT (`-`-strand, reversed)** — confirmed empirically. R2 fixture
   places its `Z` at BAM index `seq_len-1-ignore_r2` (9-1-7=1) so `read_pos_5p=7` rebases to 0.
   The rev-1 plan flagged "control R2 orientation"; this pins the exact placement.
3. Used `--mbias_only` for Tests 2–3 to isolate accumulation (adopted optional).

**Iteration log:** Test 1 — passed first run (OB byte-reversal correct). Tests 2–3 — passed
first run (PE R2-reversed placement correct). Test 4 — passed first run; `--ignore 2` yields 5
surviving calls across the 5-bp fixtures (r_OT_1 `"Zz..."` drops, the other 4 contribute),
non-empty. One `cargo fmt` pass reflowed dense `assert_eq!`/builder lines (no logic change).

**Verification (all PASS):**
- Full suite: lib **105** (+3), `parallel_phase_f` **18** (+1); all other suites green.
- **Revert-smoke** (temporarily set `call.rs:204` → `aligned.read_pos_5p`): Tests 1, 2, 3
  FAIL (+ existing #876 OT guards); **Test 4 stays GREEN** (divergence guard, not revert —
  confirms A-I4). Fix restored; re-verified green.
- `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean.

**Acceptance (#878):** ✅ 4 tests added + passing; ✅ each of Tests 1–3 fails on revert;
✅ no source `*.rs` logic changes (only the non-breaking `synth_se_record` test-helper
refactor). Remaining: update `BUG_876_FIXES_PLAN.md §6` (#8/#9/#10 → landed) at PR time.

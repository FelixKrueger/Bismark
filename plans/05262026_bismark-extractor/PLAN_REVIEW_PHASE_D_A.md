# Plan Review — Phase D (`M-bias.txt` writer), Reviewer A

**Plan:** `plans/05262026_bismark-extractor/PHASE_D_PLAN.md` rev 0
**Reviewer:** A (independent, fresh context)
**Verdict:** **APPROVE-WITH-NITS** — implementation can proceed. The nits are small omissions in test coverage and one wording precision issue in the helper doc. No critical defects.

---

## 1. Logic review

### 1.1 Filename byte-identity (plan §4.1, §5.2)

Traced Perl `:632-642` manually for each suffix listed in the plan; all match.

| Input              | Perl pipeline                                                                                   | Final M-bias.txt name      | Plan claim |
|--------------------|-------------------------------------------------------------------------------------------------|----------------------------|------------|
| `sample.bam`       | `s/gz$//` → `sample.bam`; `s/sam$//` → ...; `s/bam$//` → `sample.`                              | `sample.M-bias.txt`        | ✓ matches  |
| `sample.bam.gz`    | `s/gz$//` → `sample.bam.`; `bam$` does NOT match (trailing is `.`)                              | `sample.bam.M-bias.txt`    | ✓ matches  |
| `sample.sam.gz`    | `s/gz$//` → `sample.sam.`; `sam$` does NOT match (trailing `.`)                                 | `sample.sam.M-bias.txt`    | ✓ matches  |
| `sample.cram`      | `s/cram$//` → `sample.`                                                                          | `sample.M-bias.txt`        | ✓ matches  |
| `sample`           | no strip                                                                                         | `sampleM-bias.txt`         | ✓ matches  |
| `sample.txt`       | `s/txt$//` → `sample.`                                                                           | `sample.M-bias.txt`        | not in plan tests; see §3 Optional |
| `sample.gz.bam`    | `s/gz$//` → no match; `s/bam$//` → `sample.gz.`                                                  | `sample.gz.M-bias.txt`     | not in plan tests; consistent with helper |

**Important nit (§3 below):** the `derive_mbias_basename` plan doc on PHASE_D_PLAN line 272-275 says "strip path → strip trailing `gz`/`sam`/`bam`/`cram`/`txt` (one suffix at a time, in that order)". The phrase "one at a time" is slightly misleading — Perl's chain unconditionally tries each pattern exactly once, in order, and any subset can strip. It's NOT a loop that runs until no match. The plan §4.1's prose docstring is clearer (says "one at a time, in that order"). Implementer should mirror Perl exactly: 5 sequential `strip_suffix` attempts, each replacing the running string with the stripped result if matched, then moving on. The plan §6 step 2 wording ("mirrors Perl `:632-637` regex chain") supports this — fine in practice but warrants a unit test (see §3.1).

### 1.2 Equals-line widths (plan §4.2)

Verified byte-counts manually against Perl `:722`, `:726`, `:825`:

- SE: `"$context context\n===========\n"` — 11 `=`. "CpG context" = 3+1+7 = 11 chars; uniform across CpG/CHG/CHH (all 3-letter labels). ✓
- PE R1: `"$context context (R1)\n================\n"` — 16 `=`. "CpG context (R1)" = 11+5 = 16. ✓
- PE R2: same 16 `=`. ✓

Equals-line width does NOT vary with context. The plan's enum-based dispatch (R1OrSe / R2) is the correct abstraction.

### 1.3 `max_position` semantics (plan §4.3)

Verified `1..$max_length_1` Perl semantics: when `max_length_1 == 0`, the loop body is empty, so the section emits header + column header + trailing `\n` only. Plan §4.3 + §4.7 row 1 + test `write_mbias_txt_empty_mbias_emits_headers_only` cover this. ✓

The `MbiasTable::max_position` formula (`max(cpg.len, chg.len, chh.len).saturating_sub(1)` per-vec, then max-of-three) is correct: `vec.len() - 1` gives the highest valid 1-based index, and `saturating_sub` handles empty vecs (returning 0).

**Important nit:** the plan's snippet at lines 144-146 applies `saturating_sub(1)` independently to each vec. That's correct, but consider whether the **writer should treat `max_position == 0` after merging** the same way as "no rows". This is what the plan claims, but there's a subtle case: if only `cpg.len() == 1` (i.e. slot 0 allocated but never written to), `max_position` returns 0 → header-only. Position 0 is never used (plan §4.7 confirms), so this case is identical to "fully empty". ✓ No bug.

### 1.4 Zero-coverage empty-percent format (plan §4.2 row 3 + §7.1 test)

Verified Perl `:740-746`. `$percent = ''` initially; only overwritten inside the `if (meth+un > 0)` branch. So zero-coverage rows interpolate the empty string, yielding literal `\t\t` between unmeth and coverage. Plan's test `write_mbias_txt_per_position_row_zero_coverage_empty_percent` asserts `"5\t0\t0\t\t0\n"`. ✓ byte-identical.

### 1.5 SE vs PE threading via `is_paired: bool` (plan §4.6, §5.3)

The plan's rejection of "infer from `mbias[1]` emptiness" is correct: an empty PE BAM (zero valid records) would leave `mbias[1]` empty and produce SE-shaped output, which is wrong. Threading explicit `is_paired` through `ExtractState::new` is the right call.

**Alternative considered (and worth a sentence in §9):** store `paired_mode` in `ResolvedConfig` instead of `ExtractState`. Pros: ResolvedConfig is read-only, threaded everywhere already. Cons: `paired_mode` is determined at runtime by `extract_se` vs `extract_pe` dispatch — it's a function of dispatch, not config. Putting it in `ExtractState` (which is constructed per-dispatch) is the right home. The plan's choice is defensible. ✓

### 1.6 Phase B/C test ripple (plan §6 step 7)

Audited callsites with `grep -rn "ExtractState::new" rust/bismark-extractor/`:

| File                          | Callsites | New arg |
|-------------------------------|-----------|---------|
| `src/pipeline.rs:74`          | `extract_se` | `false` |
| `src/pipeline.rs:195`         | `extract_pe` | `true`  |
| `tests/se_phase_b.rs:646,681,730,765,795` | 5 SE unit tests | `false` |
| `tests/pe_phase_c.rs`         | **0** (no direct constructions) | n/a   |
| `tests/se_phase_b_smoke.rs`   | 0 (goes through binary)         | n/a   |
| `tests/pe_phase_c_smoke.rs`   | 0 (goes through binary)         | n/a   |

Total: 2 production callers + 5 unit-test callers. All ripple cleanly with `false`. The PE test file has no direct construction — that's noteworthy: it means the PE direct-state path is currently exercised only via the binary smoke test. Phase D's `is_paired=true` codepath has no unit-level coverage in `pe_phase_c.rs`; the **only PE direct-state coverage will be the new Phase D `mbias_writer_phase_d.rs` tests** (`extract_state_new_pe_sets_is_paired_true` etc.). Plan §7.1 covers this — fine. ✓

### 1.7 `--mbias_only` deferral (plan §2 row, §11)

Phase D's writer is invoked from `state.finalize` independent of `route_call`. `--mbias_only` only short-circuits `route_call`'s split-file write path. So Phase D's writer would be reached identically under Phase E's `--mbias_only`. The accumulator runs unconditionally (already in Phase B/C), the writer runs when `!mbias_off` (added in Phase D). When Phase E removes the main-dispatch rejection for `--mbias_only`, no further Phase D changes are needed. ✓ correct architecture.

### 1.8 `coverage` column (plan §2 row "Column header")

Verified Perl `:729`: literal `"position\tcount methylated\tcount unmethylated\t% methylation\tcoverage\n"` — 5 tab-separated columns. Plan correctly notes the SPEC §4.2 "4-col" claim is wrong, queues SPEC corrective edit as follow-up task §16.1. ✓

### 1.9 Position 0 handling (plan §4.7)

`MbiasTable::accumulate` (mbias.rs:43-59) takes `position_1based: u32` and grows the vec via `vec.resize(idx + 1, ...)`, where `idx = position_1based as usize`. If `position_1based == 0`, this resizes to 1 (slot 0). Plan claims `route_call` always passes `>= 1`.

**Optional nit:** the plan asserts "route_call always passes `pos_1based >= 1`" but doesn't cite or test this invariant. A defensive `debug_assert!(position_1based >= 1)` inside `MbiasTable::accumulate` would surface a regression if a future refactor passes a 0-based value by mistake. Low risk — current Phase B/C tests should catch this — but worth considering.

### 1.10 Order in `finalize` (plan §4.5)

Plan §4.5 says: "split-file flush → M-bias.txt → splitting report. Matches Perl line ordering at `:314-317`." Verified looking at Perl source: line 314 calls `produce_mbias_plots` (writes M-bias.txt) before line 317 emits splitting report content. ✓ The plan's order is correct.

---

## 2. Assumptions

### 2.1 Stated and verified
- `[CpG, CHG, CHH]` iteration: Perl `:718` `qw(CpG CHG CHH)`. ✓
- Trailing blank-line `\n` after each section: Perl `:762`. ✓
- Position 0 never used: confirmed by current Phase B/C `accumulate` call sites (all pass `pos_1based = call.read_pos + 1`).
- `BufWriter<File>` 8 KiB is enough: total file size ~60 KB max for typical reads, so 7-8 syscalls — fine.

### 2.2 Implicit assumptions worth surfacing

1. **`File::create` truncates an existing M-bias.txt.** If Phase E's `--mbias_only` later writes M-bias.txt before the split files run, or if a user re-runs without `rm`, the previous run's M-bias.txt will be silently overwritten. This matches Perl's `open(MBIAS,'>',...)` semantics (truncate). Plan doesn't note this but it's standard Unix behavior. Optional.

2. **Output path uses string concatenation, not `Path::join`.** Perl `:644` does `"$output_dir$mbias"` — naive string append, no path separator inserted. If `$output_dir` doesn't end in `/`, the result will be `outdirsample.M-bias.txt`. The plan's `mbias_txt_path(&output_dir, ...) -> PathBuf` should mirror this: if `output_dir` lacks trailing separator, does Perl's behavior diverge from Rust's `Path::join`? In Perl Bismark, `$output_dir` is always normalized to end with `/` at config-time (in main script). Phase A's `ResolvedConfig::output_dir` is a `PathBuf` — Rust's `Path::join` always inserts a separator. So `Path::join` will match Perl's normalized-output_dir behavior. **The plan should explicitly state this assumption** in §9.1: "ResolvedConfig.output_dir is canonicalized; `output_dir.join(filename)` produces Perl-equivalent paths." Optional — but worth noting.

3. **`MbiasPos::default() == { meth: 0, unmeth: 0 }`** is implicit. Already true (see mbias.rs:16). The writer's "0 if vec shorter than pos or cell is default" claim depends on this. ✓

---

## 3. Findings, prioritized

### Critical
**None.**

### Important
1. **Add test for `sample.txt` input.** Perl `:637` strips `txt$` too. Currently the plan's `derive_mbias_basename_strips_known_suffixes` test covers `bam`, `sam.gz`, `cram`, no-ext — but not `txt`. Phase H byte-identity might surface this if any test corpus uses `.txt.gz` SAM streams. Add `sample.txt` → `sample.` to that test (one extra line).

2. **Test `sample.bam.gz` end-to-end.** Plan §7.1 lists `sample.sam.gz` → `sample.sam.` but not `sample.bam.gz` → `sample.bam.`. Trivial to add; closes a real-world test gap (Bismark often runs on `.bam.gz` outputs from various aligners). Add to `derive_mbias_basename_strips_known_suffixes`.

3. **No test for `derive_mbias_basename` vs `derive_basename` divergence.** The plan calls out that these two helpers are intentionally different (one strips with dot, one without). A test asserting `derive_mbias_basename("sample.bam") == "sample."` (note trailing dot) and `derive_basename("sample.bam") == "sample"` (no dot) — both in the same test — would lock the divergence in. Worth one new test.

4. **`max_position == 0` ambiguity edge case.** When `cpg.len() == 1` (slot 0 allocated, slot 1 unset), the writer iterates `1..=0` (empty). Plan §4.3 says "Returns 0 if all three vecs are empty"; but if a vec has length 1 and slot 0 is `MbiasPos::default()`, `saturating_sub(1)` returns 0 too — that's "non-empty vec, but no valid 1-based positions". Plan claims `route_call` never passes 0, so this case shouldn't arise in production. **Add a unit test `mbias_table_max_position_only_slot_0_returns_zero`** that explicitly accumulates at position 0 (or constructs `cpg: vec![MbiasPos::default()]` directly) and asserts `max_position() == 0` — locks the invariant for future refactors.

### Optional
1. **`debug_assert!(position_1based >= 1)`** in `MbiasTable::accumulate` to lock the invariant the writer relies on. Cheap.

2. **Test for percent precision at `1/3`-style edge case.** Plan §7.1 includes `meth=1, unmeth=2` → `33.33` and `meth=2, unmeth=1` → `66.67`. Good — but also worth asserting Perl's rounding behavior matches `format!("{:.2}", ...)`: e.g. `meth=1, unmeth=5` (1/6 = 16.666...) → Perl `sprintf("%.2f", 16.666...)` rounds to `16.67`. Rust `format!("{:.2}", 100.0/6.0)` also yields `16.67` (banker's rounding agrees here). Worth one regression test to lock it.

3. **Document the `Path::join` vs string-concat behavior** in §9.1 (see §2.2 item 2 above). One sentence.

4. **Helper doc precision** (plan line 272-275): the "one suffix at a time, in that order" phrasing is slightly fuzzy. Consider rewording to "5 sequential strip attempts, in order; each strip is attempted exactly once".

5. **Add a `sample.gz.bam` test case** (`gz$` doesn't match because trailing is `bam` → strips `bam$` → `sample.gz.`). Validates the "in order, each once" behavior. Adds one row to existing test.

---

## 4. Efficiency

Nothing to flag. ~60 KB max output, 8 KiB BufWriter, O(max_length) writes per section, runs once at finalize. No per-record cost. Plan §8 already articulates this well.

---

## 5. Alternatives considered (but plan's choices are good)

1. **Could `is_paired` be derived from `[MbiasTable; 2]`?** No — empty-PE-BAM ambiguity (plan §2 explicitly addresses this). Plan's choice is correct.

2. **Could the writer live in `mbias.rs` instead of a new `mbias_writer.rs`?** Plan keeps accumulator and writer concerns separate. That's the cleaner choice — accumulator is `&mut`-heavy hot-path code; writer is one-shot I/O. Splitting modules is good.

3. **Could `MbiasTable::max_position` be cached during accumulation?** Could be — but the cost of the 3-length lookup at finalize is O(1). Premature optimization. Plan's choice is right.

4. **Could we share code between SE and PE section emission?** Plan does — `write_mbias_sections` takes a `ReadIdentitySection` enum. Good. The 11-vs-16 equals dispatch is one branch on the variant. Clean.

---

## 6. Validation sufficiency

Plan §7.1 lists 22 unit tests + 3 smoke extensions. The byte-identity surfaces covered: section header (SE + PE R1 + PE R2), column header, per-position row (with calls, zero-coverage), iteration range, blank-line separator, empty mbias, empty R2, percent precision, mbias_off gating, finalize integration. **Coverage is strong**.

Gaps (covered in §3 above): `sample.txt` and `sample.bam.gz` filename tests, divergence test vs `derive_basename`, `max_position==0`-via-slot-0 test, percent precision at `1/6`-style edge.

---

## 7. Verdict: APPROVE-WITH-NITS

The plan is solid. Logic, byte-identity surfaces, and ripple analysis are accurate; the Perl source citations all check out manually. The `is_paired` threading decision is well-reasoned. The 22-test suite covers the high-risk surfaces.

The nits are small test-coverage additions (4 new test cases at most) and one doc-precision tweak — none requiring re-plan or re-design. Implementation can proceed; reviewer recommends folding the **Important** items in §3 before merging Phase D's PR.

Total Important: 4 small fixes. Total Optional: 5 polish items. Critical: none.

---
*Reviewer A — independent fresh context — no shared state with Reviewer B.*

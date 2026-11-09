# Plan Coverage Report

**Mode:** B (code vs. plan — PLAN.md rev 1 doubles as the implementation spec)
**Plan:** `plans/08142026_5base-simplex-consensus/PLAN.md` (rev 1, 2026-08-14, incl. §11b implementation notes)
**Code:** branch `1104-simplex-consensus`, commits `50f1eef` (step 0) + `a80aa3a` (feature), base `dev` @ `53744d5`
**Date:** 2026-08-15
**Auditor:** plan-manager (fresh context)
**Tree audited:** commits `50f1eef` + `a80aa3a` **plus uncommitted reviewer fixes** applied during
this audit (see "Post-audit tree changes")
**Verdict:** INCOMPLETE — 3 items unresolved (V5 partial, V7 partial, V9 pending-as-documented)

## Summary

- Total ledger items: 63
- DONE: 56
- PARTIAL: 2 (V5, V7)
- MISSING: 0
- DEVIATED (documented, verified): 3 (§11b devs 1–3; dev 4 is plan-conforming, so counted DONE)
- PENDING (documented as outstanding): 1 (V9)
- Declined items verified declined: 2

## Post-audit tree changes (included in the verification below)

Three uncommitted edits landed while this audit ran; all are verified present and green, and none
changes what PLAN.md rev 1 specified:

1. **`SimplexLedger::finish` residual fix** (`five_base_duplex.rs`): residual is now
   `expected.len() - done.len()` rather than `arrived.len()`, so a family PASS 1 counted but PASS 2
   never saw is caught too — previously it was in neither `arrived` nor `done` and slipped through.
   New unit test `ledger_reports_a_family_that_never_arrived`. This **strengthens** ledger item 15
   (§3.8c residuals at EOF) beyond the plan's literal wording.
2. **New integration test `histogram_buckets_deeper_families_and_clamps_at_five`**: exercises the
   histogram's `3` bucket and the `>=5` clamp (every prior fixture had only 1- or 2-read families).
   Strengthens ledger item 12 (§3.7).
3. **Vacuous-assertion hardening in the V3 test**: asserts the duplex record count (2) before the
   `.all(|l| mx:i:2)` check, which would hold vacuously on a header-only BAM.

All statuses below are from reading the actual diff (`git diff dev...HEAD`), running the
tests/lints, and two **empirical old-vs-new binary comparisons this audit performed itself**
(dev `53744d5` vs step-0 `50f1eef` vs HEAD builds) — not from the implementer's claims.

## Coverage ledger

### §1 Goal

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1 | `--five_base_emit_multiplicity {duplex,simplex,both}`, default `duplex` | DONE | `cli.rs:171-190` flag + `EmitMultiplicity` ValueEnum (`cli.rs:479-499`) |
| 2 | Separate BAM: `*_pe.5base_simplex.bam` in-run; `five_base_simplex.bam` standalone | DONE | `mod.rs:1858-1863` (in-run, incl. `_bismark_{tok}` variant) + `mod.rs:597` (standalone) |
| 3 | `mx:i:2`/`mx:i:1` on every record of both files in simplex/both modes | DONE | `emit_family` `mx` param; duplex loop `mx = spx_path.is_some().then_some(2)`; simplex `Some(1)`. Tested (V3 test asserts both) |
| 4 | Report gains simplex counts + family-size histogram (read counts) | DONE | `mod.rs:2846-2857`: emitted/skipped/total + `[reads per family 1:.. 2:.. 3:.. 4:.. >=5:..]` |
| 5 | Step 0: deterministic duplex emission order (own commit, CHANGELOG disclosure) | DONE | Commit `50f1eef`: 4-line sort insertion + CHANGELOG sentence + V1a test. Diff is manifestly order-only |

### §3 Behavior

| # | Item | Status | Notes |
|---|------|--------|-------|
| 6 | 3.1 Sort by `(ref_id, start, end, umi_hash)`; simplex order = completion order | DONE | `mod.rs:2812-2813` `sort_unstable_by_key`; simplex evicts on last arrival (inherently deterministic) |
| 7 | 3.2 CLI validation on all 3 dispatch paths; reject with `--five_base_bisulfite_bam` | DONE | resolve() guard `config.rs:769-777`; bisulfite rejection `mod.rs:176-187` fires **before both** standalone dispatches; from_bam satisfied by construction. Both guards integration-tested |
| 8 | 3.3 Default mode byte-identical to step-0 baseline, stderr verbatim | DONE | **Empirically verified by this audit** (see Test verification): step-0 vs HEAD binaries, default mode, decompressed SAM modulo @PG byte-identical; full stderr identical modulo output path (covers summary line AND standalone `Note:` verbatim). Committed test additionally asserts no `mx`, no simplex BAM, no `spx:` |
| 9 | 3.4 `both`: duplex→existing BAM `mx:i:2`; simplex→simplex BAM, 1 record, `mx:i:1`, `spx:` prefix | DONE | Tested exactly (`both_mode_emits_one_own_strand_record_per_simplex_family`) |
| 10 | 3.5 `simplex`: only simplex BAM; duplex counted, not stored | DONE | `mod.rs:2757-2759` skips paired storage when `dpx_path.is_none()`. Test asserts no duplex BAM + no duplex report line. Note: no duplex-family count is printed in simplex mode — the plan's "(their counts for the report come from PASS 1)" was read as "the paired set is still derived"; no plan text specifies a duplex count line for simplex mode |
| 11 | 3.6 Same filters via shared `key_of` (min_mapq, skips, pair gates) | DONE | `key_of` untouched; PASS 1 counts ≡ PASS 2 arrivals by construction; MAPQ-orphan test exercises it |
| 12 | 3.7 Report: `emitted_spx`, `skipped_spx`, histogram 1/2/3/4/≥5 over ALL simplex families; count identity | DONE | Histogram built in the PASS-1 sweep (`mod.rs:2570-2578`), so it covers emitted + skipped; identity asserted in test ("3 ... emitted, 0 ... skipped, of 3 single-strand"). Buckets `1`/`2` tested by V5/V6; buckets `3` and the `>=5` clamp now tested by the post-audit `histogram_buckets_deeper_families_and_clamps_at_five` (bucket `4` remains untested — clamp arithmetic covers it) |
| 13 | 3.8a `end <= start` / chromosome-edge → skip + count per multiplicity | DONE | `emit_family` returns `Ok(false)` → per-multiplicity `skipped`/`skipped_spx`. No dedicated test (plan's §9 has no row for it; guard is the pre-existing duplex one, now shared) |
| 14 | 3.8b Absent key in PASS 2 → `AlignerError::Validation` | DONE | `LedgerError::UnknownKey` → `ledger_msg` → Validation; unit-tested |
| 15 | 3.8c Residuals at EOF → hard error | DONE (strengthened post-audit) | `SimplexLedger::finish` → `Residual(n)`, naming counts; wired via `ledger.finish().map_err(ledger_msg)?` (`mod.rs:2810`). Residual is now `expected.len() - done.len()`, so a never-arrived family is caught as well as an in-flight one; two unit tests |
| 16 | 3.8d Post-eviction re-arrival (`done` set) → hard error | DONE | `LedgerError::AfterEmission`; unit-tested |
| 17 | 3.8e Empty simplex set → header-only simplex BAM; report says 0 | DONE | Writers created up-front (`mod.rs:2582-2597`) before any emission; report prints zeros. Directly tested only for the duplex side (V6 header-only duplex BAM in `both` mode); same code path |
| 18 | 3.8f No UMI → qname UMI field `NA` | DONE | Existing convention preserved in `emit_family`; test qnames parse `...:NA` |

### §4 Signature

| # | Item | Status | Notes |
|---|------|--------|-------|
| 19 | `EmitMultiplicity` enum (Clone, Copy, PartialEq, Eq, ValueEnum) | DONE | Adds `Debug` (harmless); `as_str()` helper added for notices |
| 20 | `EmitPaths<'a>` enum | DONE | `mod.rs:2377-2381`, exactly as sketched |
| 21 | `run_five_base_consensus(..., outputs: EmitPaths<'_>, ...)` | DONE | Signature matches (`consensus_bam_path` → `outputs`) |
| 22 | `BismarkRecord::set_mx(i32)` beside `set_rx` | DONE | `io/record.rs:232-241`, `Tag::from(*b"mx")` + `Value::from`, lowercase-local doc |
| 23 | `SimplexLedger { expected, arrived, done }` | DEVIATED (documented) | §11b dev 2, verified: generic `SimplexLedger<K, F>`, `arrived: HashMap<K, (F, u32)>`, `arrive_with(key, add-closure)` returning the completed `F` by value. Same error surface (UnknownKey / AfterEmission / Residual) |

### §5 Implementation outline

| # | Item | Status | Notes |
|---|------|--------|-------|
| 24 | Step 0 own commit + CHANGELOG + V1a gate | DONE | `50f1eef` |
| 25 | Step 2 CLI + validation; doc-comment: #1104 marker, DRAGEN analogy, mx values, no-variant-check caveat, no-duplex-BAM note, mx-vs-XM sentence | DONE | All six elements present in the flag doc (`cli.rs:171-183`) |
| 26 | Step 3 PASS 1 one-sweep derivation of `paired` + `simplex_expected`; `drop(counts)`; default = no new map | DONE | `mod.rs:2564-2580`; default mode inserts nothing into `simplex_expected` (spx branch gated on `spx_path.is_some()`). §11b dev 4 ("retain variant not used") is plan-conforming, not a real deviation — the one-sweep was §5's primary path |
| 27 | Step 4 PASS 2 storage rule (Simplex mode skips paired storage) | DONE | `mod.rs:2757-2759` |
| 28 | Step 5 emit-and-evict via `SimplexLedger`; all three error paths | DONE | `mod.rs:2820-2837` arrival path; `ledger.finish()` after the file loop |
| 29 | Step 6 `emit_family(fam, flags, qname_prefix, mx, writer, ...) -> Result<bool>`; tag only when `mx.is_some()`; contamination rationale in doc comment | DEVIATED (documented) | §11b dev 1, verified: nested `fn` (not closure), takes `genome`/`refid` explicitly. Everything else as planned, incl. the fact-2 contamination rationale in the doc comment |
| 30 | Step 7 writers up-front per `EmitPaths` variant | DONE | `mod.rs:2582-2597` |
| 31 | Step 8 call sites (in-run + standalone); `Note:` extended only when `emit != Duplex` | DONE | Default `Note:` byte-frozen (verified empirically); non-default gets its own variant naming the multiplicity |
| 32 | Step 9 report counters; default prints existing line verbatim | DONE | Verbatim-ness verified empirically (stderr diff, step-0 vs HEAD) |
| 33 | Step 10 docs (`illumina-5-base.md`: flag, tag, caveat, deconvolution pairing) + CHANGELOG (feature + step 0) | DONE | Docs section + caution block + deconvolution recommendation present. CHANGELOG: both entries under Unreleased → bismark (aligner). Note: the feature entry is a multi-sentence paragraph (house style), not the plan's "one sentence" |
| 34 | Step 11 tests | See §9 rows | — |
| 35 | Declined: per-record family-size tag (B-alt 3) | DONE (declined) | Verified absent — only `mx` is written |
| 36 | Declined: rename to `five_base_simplex_consensus.bam` (A-opt 14) | DONE (declined) | Verified: file is `five_base_simplex.bam` |

### §7 Integration

| # | Item | Status | Notes |
|---|------|--------|-------|
| 37 | Simplex records are ordinary single-strand Bismark records via unchanged `five_base_emit_record`; `mx` inert to consumers | DONE | Emission goes through the same `five_base_emit_record`; extractor parses XR/XG/XM only. At-scale extractor pass = V9 (pending) |
| 38 | File split = separation; tag = merge-survivor | DONE | — |
| 39 | Step 0 changes default order only, disclosed | DONE | CHANGELOG sentence; content-identity empirically proven (V1a) |
| 40 | No change to `--five_base_duplex` TXT report pass | DONE | `write_report` path untouched in the diff |

### §8 Assumptions honoured in code

| # | Item | Status | Notes |
|---|------|--------|-------|
| 41 | 1. Byte-identical default defined vs step-0 baseline; suite byte convention | DONE | Empirically verified (both binaries VERSION 3.1.0, decompressed, @PG-filtered) |
| 42 | 2. FR proper pairs (stated, out-of-scope FF/RR) | DONE | Stated in plan; no code change required |
| 43 | 3. Mates' UMIs canonicalize equal; violation → two 1-read families, histogram represents honestly | DONE | Read-count histogram implements the honest representation |
| 44 | 4. Mate adjacency affects peak memory only; eviction on exact counts | DONE | `arrive_with` completes on `arrived == expected` regardless of adjacency |
| 45 | 5. PASS 1 ≡ PASS 2; every violation direction fails loud | DONE | Three ledger error paths, all mapped to `AlignerError::Validation` |
| 46 | 6. `mx` unused across the suite | DONE | **Verified by grep**: aux-tag inventory is AS, CB, MD, NM, RX, UR, XG, XM, XR + only the new `mx` (single site, `set_mx`) |
| 47 | 7. PE-only | DONE | **Verified**: `--illumina_5base` rejects SE at resolve (`config.rs:848,860`); standalone probes FLAG 0x1 and rejects SE (`mod.rs:573-579`) |
| 48 | 8. Concordance-gated; invariant-based validation | DONE | Validation is synthetic-fixture + invariant based, like the duplex path |

### §9 Validation

| # | Row | Status | Notes |
|---|-----|--------|-------|
| 49 | V1a step 0 order-only | DONE | Committed test `consensus_emission_order_is_deterministic` PASSES (same binary twice → identical; ascending span order). The row's second half (old vs new binary, sorted comparison) was **not evidenced by the implementer** — this audit ran it: dev (`53744d5`) vs step-0 binaries on a 6-family fixture, **sorted content identical (12 records)**; two dev runs observed to differ in order (bug confirmed real) |
| 50 | V1b default-mode gate | DONE | Committed test asserts the proxies (no `mx`, no simplex BAM, no `spx:`, duplex stderr line, ≥2-family fixture). The actual step-0-baseline-vs-feature byte comparison was **not evidenced by the implementer** — this audit ran it: step-0 vs HEAD binaries, default mode, 3-duplex+1-simplex fixture → **decompressed SAM (modulo @PG) byte-identical; stderr verbatim** (incl. the standalone `Note:` line) modulo the output path |
| 51 | V2 simplex calling at CpGs (unit) | DONE | `simplex_ot_only_calls_plus_cpg_masks_minus_cpg`, `simplex_ob_only_calls_minus_cpg_masks_plus_cpg` (+ a non-CpG pass-through test beyond plan). All pass |
| 52 | V3 one record, right strand, right XM (`both` mode) | DONE | Exactly 2 simplex records; FLAG 0 + FLAG 16; `spx:`/`mx:i:1`; no `dpx:` leak; duplex `mx:i:2`; **exact XM asserted for both orientations** (`......Z.............` / `.......z............`). Passes |
| 53 | V4 ledger error paths (unit, no production hooks) | DONE | Unknown key, arrival-after-done, residual-at-finish each unit-tested → clean `LedgerError` (mapped to `AlignerError::Validation` in production via `ledger_msg`); cross-input completion unit-tested (`ledger_family_split_across_inputs_completes_on_last_arrival`); post-audit, never-arrived residual too. The "2 BAMs" clause is covered at ledger level, as the row's "How" column scopes it |
| 54 | V5 MAPQ-orphan family | PARTIAL | 1-read family emitted + histogram bucket `1` asserted (`mapq_orphaned_mate_forms_a_one_read_family`, passes). **"XM spot-checked" is not asserted** — re-verified on the current tree: zero `XM` occurrences in that test, and no XM assertion for a 1-read family anywhere in the suite |
| 55 | V6 single-fragment simplex | DONE | 1 record + bucket `2` + header-only duplex BAM asserted. "XM equals the per-read path's calls" is not asserted in this test, but V3's test asserts the exact first-principles XM for an identical single-pair OT family — materially covered |
| 56 | V7 mode matrix, both entry points | PARTIAL | `--from_bam` matrix: file presence per mode DONE (duplex: no simplex BAM; simplex: no duplex BAM; both: both files incl. header-only duplex); count identity asserted via the report line; default `Note:` verbatim verified empirically by this audit. Re-verified on the current tree — **still missing: (a) the in-run entry point is never exercised with `simplex`/`both`** (`emit_multiplicity` appears in exactly one test file, and that file drives only `--five_base_consensus_from_bam`; the in-run wiring at `mod.rs:1855-1878` incl. `_pe.5base_simplex.bam` naming is implemented but untested); **(b) no cross-check against the duplex TXT `singletons` figure** (zero `singletons` occurrences in the suite); **(c) the non-default extended `Note:` line is not asserted** (zero `emit multiplicity:` occurrences; the simplex *summary* line is asserted) |
| 57 | V8 CLI guards | DONE | Both tests pass; error messages name the flags (`requires --five_base_consensus`; `has no effect on --five_base_bisulfite_bam`) |
| 58 | V9 real-data sanity (oxy, 5-Base PE, `both` mode) | PENDING (documented) | Not run — needs the 5-Base PE dataset on oxy; §11b and PROGRESS.md both mark it outstanding. Marked pending per instruction, not done |

### §11b documented deviations (verified against code)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 59 | Dev 1: `emit_family` nested `fn` (not closure), explicit `genome`/`refid` | DEVIATED (documented, verified) | Matches code |
| 60 | Dev 2: `SimplexLedger::arrive_with(key, add)` mutator-closure; generic `<K, F>` | DEVIATED (documented, verified) | Matches code |
| 61 | Dev 3: `config.rs` imports `EmitMultiplicity` directly | DEVIATED (documented, verified) | `use crate::aligner::cli::{Cli, EmitMultiplicity}` |
| 62 | Dev 4: `counts.retain` variant not used; one-sweep shipped | DONE | Not actually a deviation — §5 step 3's primary path |
| 63 | Added test: `simplex_never_calls_a_strand_it_did_not_sequence` | DONE (beyond plan) | Exists, passes; fixture carries a standalone non-CpG `G` (offset 12) and asserts that column is a no-call — converts §2 fact 2 from documented to tested |

## Test verification (Mode B)

All results below are **observed by this audit** on 2026-08-15 (macOS, samtools 1.21 on PATH — the
samtools-gated tests did not skip; `$CI` unset, but samtools being present means none of the
`samtools_available()` guards no-opped).

Every command's **own** exit code was captured (not a pipe's last stage). All runs were on the
current tree, i.e. including the three post-audit reviewer edits.

| Check | Result |
|-------|--------|
| `cargo test -p bismark` (full suite, foreground) | **PASS — exit code 0.** 78 result lines, **2154 tests passed, 0 failed, 0 ignored**; zero `test result: FAILED`, zero `error: aborting`, zero doctest failures |
| ↳ lib unit tests within that run | **1503 passed, 0 failed** (§11b's "1502" + the one post-audit ledger test = 1503, corroborating the implementer's figure) |
| ↳ doctests within that run | **8 passed, 0 failed** |
| `cargo test -p bismark --test aligner_five_base_simplex` | **PASS — exit code 0, 10/10** (0.62s) on the current tree. Earlier in this audit, before the post-audit test landed, the same command gave **9/9** |
| New `five_base_duplex.rs` unit tests | **9** new tests, all pass: 3 `consensus_base` simplex + **6** ledger (5 committed + `ledger_reports_a_family_that_never_arrived`) |
| `cargo clippy -p bismark --all-targets` (default features) | **PASS — exit code 0, zero warnings/errors** (re-run on the current tree, 2m01s). §11b's additional clean `binseq-input`/`rammap-inprocess` feature runs were not re-verified by this audit |
| `cargo fmt -p bismark -- --check` | **PASS — exit code 0** (re-run on the current tree) |
| Empirical V1a (audit-run): dev vs step-0 binaries, 6-family fixture | **PASS** — sorted decompressed-SAM content identical (12 records); two dev runs differed in raw order (HashMap randomness observed, confirming the step-0 premise) |
| Empirical V1b (audit-run): step-0 vs HEAD binaries, default mode, 3-duplex+1-simplex fixture | **PASS** — decompressed SAM (modulo @PG) byte-identical; stderr verbatim modulo output path; no simplex BAM created |

**Discarded run:** an earlier full-suite invocation aborted in its doctest stage with
`error: extern location for bismark does not exist: …libbismark-*.rlib` — a build-artifact race
caused by concurrent agents rebuilding the lib mid-run, not a code failure. Its apparent exit 0 came
from a trailing pipe stage, which is why the re-run above asserts cargo's own exit code. The clean
foreground re-run supersedes it.
| Empirical V1a (audit-run): dev vs step-0 binaries, 6-family fixture | **PASS** — sorted decompressed-SAM content identical (12 records); two dev runs differed in raw order (HashMap randomness observed, confirming the step-0 premise) |
| Empirical V1b (audit-run): step-0 vs HEAD binaries, default mode, 3-duplex+1-simplex fixture | **PASS** — decompressed SAM (modulo @PG) byte-identical; stderr verbatim modulo output path; no simplex BAM created |

## Gaps (detail)

### Item 54: V5 — "XM spot-checked" for the MAPQ-orphaned 1-read family

**Expected:** V5's Expect column: "1-read family emitted; histogram bucket `1`; **XM spot-checked**".
**Found:** `mapq_orphaned_mate_forms_a_one_read_family` asserts the record count (1) and the histogram bucket (`reads per family 1:1 2:0`) — no XM assertion (verified on the current tree: the test body contains no `XM`). No other test asserts XM for a 1-read family.
**Gap:** one assertion on the orphan record's XM string (the fixture geometry makes the expected string statable from first principles, as V3 does).

### Item 56: V7 — mode matrix through the in-run entry point + TXT cross-check + extended Note

**Expected:** "duplex/simplex/both through the **in-run fixture** and `--from_bam` on its BAM; default `Note:` unchanged, **non-default extended**; count identity ... **cross-checked against the duplex TXT `singletons` figure**".
**Found:** the matrix runs only through `--from_bam` (the new suite's header defers the in-run entry to `aligner_five_base_groundtruth.rs`, which never uses the new flag). The in-run call-site wiring (`mod.rs:1855-1878`) is implemented and reviewed but has no test. The `singletons` cross-check and an assertion on the extended non-default `Note:` line are absent.
**Gap:** an in-run (minimap2-gated, like the existing groundtruth tests) run with `--five_base_emit_multiplicity both` asserting `_pe.5base_simplex.bam` presence; plus the `emitted_spx + skipped_spx` vs duplex-TXT-`singletons` cross-check; plus one `Note:`-line assertion for a non-default mode.

### Item 58: V9 — real-data oxy run

**Expected:** `both` mode on the 5-Base PE set; duplex records byte-identical to a `duplex`-mode run modulo the `mx` tag; count identity at scale; extractor runs clean over the simplex BAM.
**Found:** not run; documented as outstanding in §11b and PROGRESS.md (needs the oxy dataset, not runnable locally).
**Gap:** the oxy run itself. This is the plan's own acknowledged remainder, not an undisclosed omission.

### Minor record corrections (not gaps)

- §11b/PROGRESS.md state the new suite is "10/10", five_base_duplex gained "9 unit tests", and the full suite is "1502 unit tests". As committed (`a80aa3a`), the observed counts were **9** integration tests and **8** new unit tests. After the post-audit reviewer edits the tree now matches §11b's figures exactly: **10** integration tests, **9** new unit tests, and lib unit tests total **1503** (= 1502 + the new ledger test). So the discrepancy was an off-by-one in the commit-time record, now moot.
- The V1a/V1b old-vs-new binary comparisons were not evidenced in the implementation record; this audit performed both and they **pass** (see Test verification), so the default-mode byte gate and the order-only claim are now empirically established, not just structurally argued.

## Verdict

**INCOMPLETE — 3 items unresolved.** The implementation itself covers every behavior, signature,
edge case, and documented deviation in PLAN.md rev 1; the unresolved items are validation-row
gaps, two of them small:

1. **V5 (PARTIAL):** add the XM spot-check on the MAPQ-orphaned 1-read family's record.
2. **V7 (PARTIAL):** the in-run entry point is untested in `simplex`/`both` modes; the duplex-TXT
   `singletons` cross-check and the non-default `Note:` assertion are absent.
3. **V9 (PENDING, documented):** the oxy real-data `both`-mode run — already tracked as the
   explicit remainder in §11b and PROGRESS.md.

Everything else — including both empirical byte gates the committed tests could not express —
is DONE and observed passing: full suite **exit 0, 2154 passed / 0 failed**, clippy **exit 0 with
zero warnings**, fmt **exit 0**, simplex suite **10/10**.

No plan item is MISSING. The three §11b deviations are documented and were each verified to match
the code, and the two §5 declined items were verified to have stayed declined (no per-record
family-size tag; the file is `five_base_simplex.bam`, not `five_base_simplex_consensus.bam`).

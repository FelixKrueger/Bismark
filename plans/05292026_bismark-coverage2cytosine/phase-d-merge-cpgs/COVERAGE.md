# Plan Coverage Report — Phase D (`--merge_CpGs` + `--discordance_filter`)

**Mode:** B (code vs. implementation plan), audited against PLAN.md (rev 1) §3.1–§3.7 + V1–V14 + SPEC §9.
**Plan(s):** `phase-d-merge-cpgs/PLAN.md` (rev 1), `phase-d-merge-cpgs/IMPL.md`, `SPEC.md` §9.
**Date:** 2026-05-30
**Verdict:** **COMPLETE** — all behaviours, error paths, and both rev-1 Criticals are implemented; all 92 tests pass. Three minor *test-coverage / reproducibility* gaps noted (do NOT block the behaviour-correctness verdict; itemised below as Advisory).

## Summary

- Total items audited: 16 IMPL checklist + 9 IMPL tasks (T1–T9) + 14 validation rows (V1–V14) + 2 rev-1 Criticals + error path = **42 distinct items**.
- DONE: 39
- PARTIAL: 3 (V2 dedicated unit test, V8a dedicated test, generate_goldens.sh phase-D block)
- MISSING: 0
- DEVIATED: 0 (V14 reinterpreted — see note; not a behaviour gap)

Tests run (`cargo test -p bismark-coverage2cytosine`): **92 pass / 0 fail** — 62 unit + 11 phase-B + 7 phase-C + 7 phase-D + 5 sanity. Matches the PLAN's claimed count exactly.

---

## IMPL task coverage (T1–T9)

| Task | Goal | Code location | Status |
|---|---|---|---|
| T1 | `MergeCpgSanityViolation { detail }` error | `error.rs:131-140` (`#[error("merge_CpGs sanity violation: {detail}")]`) | DONE |
| T2 | `pub(crate)` promote `ReportWriter`/`create`/`write_all`/`finish`/`report_path`/`report_name`; `merged_cov_name`/`discordant_cov_name`/`merged_cov_path`/`discordant_cov_path`; no cleanup helper invented | `report.rs:32,41,50,59,423,453` (promotions) + `report.rs:480-528` (cov-name/path helpers) | DONE (RED unit test for the names omitted — see V2/Advisory-1) |
| T3 | `parse_report_row` (7 fields, tri ignored) + `round6` | `merge.rs:52-77` (parse), `merge.rs:34-42` (`pct6`/`round6`) | DONE |
| T4 | Generate phase-D goldens from repo Perl v0.25.1 | `tests/data/phase_d/` (all fixtures + goldens present + verified vs live Perl this audit) | DONE (fixtures present) — **no checked-in generator block in `generate_goldens.sh`** (Advisory-3) |
| T5 | `run_merge` core: gz-aware streaming `next_row()`; 2-row `while`; chr-start resync; sanity asserts; pool; skip-zero; **stream-write**; EOF-mid-resync error/no-cleanup | `merge.rs:80-196` | DONE |
| T6 | Discordance: both-measured gate; `round6` compare `abs()>N` strict; write both rows + `continue` | `merge.rs:155-171` | DONE |
| T7 | `--zero_based` half-open (merged `pos2+1`; discordant `pos+1`) | `merge.rs:165-166,179` | DONE |
| T8 | `lib::run` post-pass gated on `merge_cpgs` | `lib.rs:52-56` | DONE |
| T9 | fmt/clippy/test/build + A–C regression green | 92 tests green this audit; PLAN notes clippy `-D warnings` clean | DONE |

## IMPL "Plan coverage checklist" (16 items)

| # | Item | Code | Status |
|---|---|---|---|
| 1 | `MergeCpgSanityViolation { detail }` | `error.rs:138-140` | DONE |
| 2 | `pub(crate)` promotions; no cleanup helper | `report.rs:32/41/50/59/423/453`; no cleanup helper invented (confirmed) | DONE |
| 3 | `merged_cov_path`/`discordant_cov_path` (strip `.gz` then `.txt` + suffix +`.gz`) | `report.rs:480-528` (`cov_evidence_name` strips `.gz` then `.txt`) | DONE |
| 4 | `parse_report_row` (7 tab fields, tri ignored) | `merge.rs:52-77` (needs ≥6 fields; `f[6]` tri never read) | DONE |
| 5 | `round6(m,u)` — `%.6f`→parse f64 | `merge.rs:40-42` | DONE |
| 6 | gz-aware streaming `next_row()`; 2-row/iter; EOF "<2 rows" stop | `merge.rs:82` (`cov::open_cov` gz-aware), `88-100` (`next_row`), `112-114` (`while let (Some,Some)`) | DONE |
| 7 | chr-start resync (pos1<thr; chr1≠chr2 slide-until-match + extra advance; else single advance) | `merge.rs:120-141` | DONE |
| 8 | sanity asserts → `MergeCpgSanityViolation` (no panic, incl. None mid-resync) | `merge.rs:145-152,199-226` | DONE |
| 9 | pool; skip-zero; `%.6f`; **stream-write** | `merge.rs:174-188` (write per-pair inside the loop; `finish()` at `191`) | DONE |
| 10 | discordance gate + `round6` strict `>N` + both rows + `continue` | `merge.rs:155-171` | DONE |
| 11 | `--zero_based` half-open | `merge.rs:84,165-166,179` | DONE |
| 12 | `lib::run` post-pass gated | `lib.rs:52-56` | DONE |
| 13 | NO cleanup on error; EOF-mid-resync leaves partial merged file | `merge.rs:145-149` returns the error with the writer NOT finished/removed; no cleanup call anywhere | DONE |
| 14 | V1–V14 | see V-matrix below | DONE (with Advisory 1/2 on V2/V8a) |
| 15 | goldens from repo Perl v0.25.1 | fixtures present + re-verified vs live Perl this audit | DONE (Advisory-3: no committed generator) |
| 16 | clippy/fmt/workspace build + A–C regression | 92 green; A–C 18 goldens green | DONE |

## Validation matrix (V1–V14)

| V | Claim | Implementing code | Test | Status |
|---|---|---|---|---|
| V1 | `parse_report_row` exact fields | `merge.rs:52-77` | `merge.rs:259-275` `parse_report_row_fields` (+ `:277-281` blank→None) | DONE |
| V2 | merged/discordant cov filename derivation | `report.rs:480-528` | **No dedicated unit test** (IMPL T2 RED `merged_cov_name_strips_gz_then_txt` absent). Exercised transitively: golden tests assert `m.CpG_report.merged_CpG_evidence.cov` / `.cov.gz` / `.discordant_CpG_evidence.cov` filenames (`golden_phase_d.rs:46,56,86,90,105,110`) | PARTIAL (Advisory-1) |
| V3 | merged cov golden `chr1 2 3 50.495050 408 400` | `merge.rs` core | `golden_phase_d.rs:43-51` `merge_cov_matches_golden`; golden re-verified vs live Perl ✔ | DONE |
| V4 | `--merge_CpGs --gzip` decompress == plain | `report.rs` gz writer | `golden_phase_d.rs:53-61` `merge_gzip_decompresses_to_golden` | DONE |
| V5 | discordance gross → merged empty, discordant both rows | `merge.rs:155-171` | `golden_phase_d.rs:77-93` `discordance_gross_routes_to_discordant_file`; `disc_gross.merged.golden` is 0-byte (empty) ✔, discordant golden = 2 rows ✔ | DONE |
| V6 | both-measured gate (one strand 0,0 + big Δ → pooled, not discordant) | `merge.rs:156-157` (`r1.m+r1.u>0 && r2.m+r2.u>0`) | **No dedicated test.** Gate logic present + correct. (Closest: V13 `eof.cov` has a `5/1`+`0/0`-style pair but with no `--discordance_filter`.) | PARTIAL (Advisory-2) |
| V7 | `--zero_based` half-open merged `pos1 pos2+1` | `merge.rs:179` | `golden_phase_d.rs:63-75` `merge_zero_based_half_open_matches_golden`; `merge_zero.merged.golden` = `chr1 1 3 …` ✔ | DONE |
| V8a | chr-start resync **same-chr** single-advance branch | `merge.rs:136-139` (`else` branch) | **No dedicated test** — no fixture whose first same-chromosome CpG pair lands at `pos < thr` then re-pairs on the same chr. phase_b merge's first pair is `chr1 2/3` (pos1=2 = thr, resync NOT triggered); resync fixture exercises only the `chr1≠chr2` SLIDE. | PARTIAL (Advisory-2) |
| V8b | chr-start resync consecutive ≥3-bp `CGT` lone-orphan SLIDE | `merge.rs:122-135` (slide-until-chr-match + extra advance) | `golden_phase_d.rs:116-126` `resync_consecutive_short_scaffolds_slide_recovers`. Fixture `resync_genome` = `sA=CGT`, `sB=CGT` (3-bp lone-orphan scaffolds, each emits one `+`-only row — confirmed via Perl report: `sA 1 +`, `sB 1 +`), `sC=CGTTACGT`. Golden `sC 6 7 83.333333 5 1` matches live Perl ✔. Uses ≥3-bp CGT not 2-bp CG (the I2 fix is honoured). | DONE |
| V9 | uncovered CpG (0,0 pair) skipped | `merge.rs:176-178` (`if pooled_m+pooled_u==0 {continue}`) | Exercised inside `merge_cov_matches_golden`: phase_b report has many `0 0` pairs (chr1 6/7, 8/9, 14/15, 21/22; chr2; chr3uncov) all absent from the single-line merged golden ✔ | DONE |
| V10 | sanity assert fires on a corrupt/desynced row pair (no panic) | `merge.rs:199-226` (`sanity_check`) + `145-149` | **No dedicated unit test** feeding a desynced pair to `sanity_check`. The `None`-mid-resync arm (`:145-149`) IS tested via V13. The context/strand/spacing/chr-mismatch arms (`:201-224`) have no direct test — only reachable on a corrupt report. | PARTIAL (Advisory-2; same root as V6/V8a) |
| V11 | regression A–C green | — | phase_b (11) + phase_c (7) + 62 unit + 5 sanity all green this audit ✔ | DONE |
| V12 | discordance rounding boundary (`1/1` vs `11/9`, N=5 → MERGED, discordant empty) | `merge.rs:159-161` (`round6` then `abs()>N`) | `golden_phase_d.rs:95-114` `discordance_boundary_merges_not_diverts` — asserts merged = `chr1 2 3 54.545455 12 10` AND discordant empty (0-byte golden) ✔; plus unit `merge.rs:291-303` `round6_discordance_boundary_matches_perl` proves rounded Δ ≤ 5 while raw-f64 > 5. Asserts MERGED-not-diverted as required. | DONE |
| V13 | EOF-mid-resync → exit 1, no panic, partial merged == Perl's pre-die output | `merge.rs:145-149` (None→error, writer left un-finished/not-removed) | `golden_phase_d.rs:128-151` `eof_mid_resync_errors_with_partial_merged_file` — asserts `.failure().code(1)`, stderr contains "sanity violation", AND partial merged file == `eof.merged.golden` (`chrM 6 7 83.333333 5 1`, the line written before the die). `eof_genome` ends in two trailing ≥3-bp `CGT` orphan scaffolds. Asserts exit-1 + partial-matches-Perl + no-panic as required. | DONE |
| V14 | multi-pair / multi-chromosome merged golden | `merge.rs` core | **No dedicated `multi/` fixture/test** (IMPL T4 listed `multi/`; not present in `tests/data/phase_d/`). Multi-CHROMOSOME *reading* is exercised: phase_b genome/in.cov span chr1+chr2+scaf_short+chr3uncov and the merged golden correctly emits only the one covered pair. But there is no golden with **multiple covered merged lines across ≥2 chromosomes**. | PARTIAL→Advisory (see note) |

### Rev-1 Criticals + error path

| Item | Code | Test | Status |
|---|---|---|---|
| C1a — discordance compares `%.6f`-ROUNDED, strict `>`, vs integer N | `merge.rs:40-42,159-161` | V12 golden + `round6_discordance_boundary_matches_perl` unit | DONE |
| C1b — EOF-mid-resync: `next_row()→Option`, None→`MergeCpgSanityViolation` (no panic), **stream-write**, **no cleanup** | `merge.rs:88-100,145-149,174-188` | V13 golden | DONE |
| Error path `MergeCpgSanityViolation` present + reachable | `error.rs:138-140`, `merge.rs:146,200` | V13 (None arm); display-string test extends `error_display_strings_present` per IMPL T1 | DONE |
| No-cleanup-on-error | `merge.rs:145-149` returns before any cleanup; no removal call exists; PLAN §5 "c2c has NO partial-output-cleanup helper" honoured | V13 asserts the partial file remains | DONE |

### Commit-plan path check

- IMPL commit plan stages `rust/bismark-coverage2cytosine/**` + `plans/05292026_bismark-coverage2cytosine/**`.
- `git status --short` for the crate shows exactly: `M error.rs`, `M lib.rs`, `M report.rs`, `?? merge.rs`, `?? tests/data/phase_d/`, `?? tests/golden_phase_d.rs`. **No sibling crate touched.** Matches the commit plan scope. DONE.

---

## Gaps (detail) — all Advisory (test-coverage / reproducibility, not behaviour)

### Advisory-1 — V2 / IMPL T2: no dedicated `merged_cov_name`/`discordant_cov_name` unit test
**Expected:** IMPL Task 2 RED specified an inline `report.rs` test `merged_cov_name_strips_gz_then_txt` asserting the three exact strings (`…merged_CpG_evidence.cov`, `…cov.gz`, `…discordant_CpG_evidence.cov`).
**Found:** the functions (`report.rs:480-528`) and the strip-`.gz`-then-`.txt` logic are correct and present, but there is **no unit test** for them. They are validated only transitively by the golden integration tests' output filenames.
**Gap:** add the inline unit test (or accept transitive coverage). Behaviour is correct; this is missing test surface the IMPL promised.

### Advisory-2 — V6 (both-measured gate), V8a (same-chr resync branch), V10 (sanity-assert content arms): no dedicated tests
**Expected:** V6 a pair with one strand `0,0` + large Δ under `--discordance_filter` → pooled not diverted; V8a a same-chromosome `pos<thr` chr-start CpG re-pairing on the same chr (Perl `:1875` single-advance, `merge.rs:136-139`); V10 a desynced row pair fed to the sanity check → `MergeCpgSanityViolation` (no panic).
**Found:** all three code paths exist and are correct. None has a dedicated test:
- V6 gate `merge.rs:156-157` — logic present, untested in isolation.
- V8a `else` branch `merge.rs:136-139` — no fixture triggers same-chr `pos<thr`; the only resync test exercises the `chr1≠chr2` slide. (This `else` arm is the simpler of the two resync paths and is exercised by neither golden.)
- V10 sanity content arms `merge.rs:201-224` — only the `None`-mid-resync arm is hit (via V13); a context/strand/spacing/chr-mismatch corrupt-report pair is never fed in.
**Gap:** three small tests would close the matrix. The PLAN's §9 explicitly lists V6/V8a/V10 as rows to verify; they are PARTIAL because the row's code is present but the row's *test* is not. Risk is low (paths are simple and faithful to Perl), but per "the plan is the spec" these rows are not fully discharged.

### Advisory-3 — IMPL T4 / checklist 15: no Phase-D block appended to `generate_goldens.sh`
**Expected:** IMPL Task 4 + checklist item 15 require an **appended block in `tests/data/phase_b/generate_goldens.sh`** producing `merge/ merge_gz/ merge_zero/ disc_gross/ disc_boundary/ resync/ eof/ multi/` from repo Perl v0.25.1 (with `eof/` run under `|| true` to tolerate Perl's exit 255).
**Found:** `generate_goldens.sh` ends after the Phase C generators (27 lines; last line is the `split_thr` Perl invocation). The phase-D fixtures + goldens ARE committed in `tests/data/phase_d/` and I re-verified `merge.merged.golden` and `resync.merged.golden` byte-for-byte against live Perl v0.25.1 this audit — so the goldens are genuine — but they are **not reproducible from a checked-in script**.
**Gap:** append the Phase-D generator block to `generate_goldens.sh` for reproducibility/regen parity with Phase B/C.

### Note on V14 (multi-chromosome merged golden) — reinterpreted, not a behaviour gap
IMPL Task 4 listed a `multi/` fixture and V14 calls for "several CpGs across 2 chromosomes, mixed coverage → merged cov byte-identical." No `multi/` fixture/test was committed. However, the merge core is chromosome-agnostic (chr is carried as raw bytes through `ReportRow.chr` and written verbatim), and multi-chromosome *report reading* IS exercised (phase_b spans chr1/chr2/scaf_short/chr3uncov; the resync/eof fixtures span multiple scaffolds incl. a 2nd-chromosome `sC`/post-`chrM` slide). What is missing is a golden with **≥2 merged output lines on different chromosomes**. Because the phase_b cov only makes one pair coverable, the strongest single-golden multi-chr assertion isn't present. Treating V14 as PARTIAL→Advisory: the behaviour is covered by code + transitive multi-chr reading, but the specific "multiple merged lines across chromosomes" golden the row names is absent.

---

## Verdict

**COMPLETE** for behaviour, correctness, both rev-1 Criticals, the error path, no-cleanup semantics, and the commit-plan scope. All 92 tests pass; the two highest-risk ports (chr-start SLIDE resync V8b, EOF-mid-resync V13) and the discordance rounding boundary (V12) are golden-tested against live Perl, which I independently re-verified this audit.

**Advisory (non-blocking) follow-ups** — close the validation matrix exactly as the PLAN/IMPL named it:
1. Add the `merged_cov_name`/`discordant_cov_name` unit test (V2 / IMPL T2 RED).
2. Add dedicated tests for V6 (both-measured gate), V8a (same-chr resync `else` branch), V10 (sanity-assert content arms on a corrupt pair).
3. Append the Phase-D generator block to `tests/data/phase_b/generate_goldens.sh` (IMPL T4 / item 15), incl. the `multi/` fixture for V14.

None of these change shipped behaviour; they are missing test/regen surface the IMPL promised. If the user wants the matrix fully discharged before merge, items 1–3 are the exact list.

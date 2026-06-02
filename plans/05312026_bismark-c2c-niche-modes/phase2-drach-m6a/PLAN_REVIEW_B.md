# PLAN_REVIEW_B — Phase 2 `--drach` / `--m6A` (DRACH-motif m6A filtering)

**Reviewer:** Plan Reviewer B (independent; no shared state with Reviewer A)
**Plan under review:** `plans/05312026_bismark-c2c-niche-modes/phase2-drach-m6a/PLAN.md` (rev 1)
**Perl oracle:** repo-root `./coverage2cytosine` v0.25.1 (Perl 5.34.1, macOS) — fixtures built from scratch, not trusted from the plan.
**Date:** 2026-05-31

---

## Top-line verdict: **APPROVE-WITH-CHANGES**

The plan is accurate, well-grounded, and reproduces Perl v0.25.1 on every behavior I tested. The byte-identity model is correct on **both** strands (including the `pos-1` bottom-strand C anchor, the truncated-5-mer "pos-5 missing → pass" rule, and every filter arm). The reuse claims (`perl_substr`, `revcomp`, `ReportWriter`, `pct6`, cov parse, `validate`/`ResolvedConfig`) all point at code that genuinely exists, and Phase 1's `gpc.rs` is a near-exact structural template for `run_drach`.

I found **0 Critical** and **3 Important** items — all are *implementation-detail completeness* / *test-coverage* gaps, not byte-identity defects in the plan's stated logic. The headline risk (bottom-strand position) is correctly resolved.

---

## Live-Perl checks I ran (all reproduced from scratch)

| # | Fixture (seq / cov) | What it probes | Perl v0.25.1 result | Matches plan? |
|---|---------------------|----------------|---------------------|----------------|
| B1 | `TTTGAACATTTGTACATTTGAACGTTTGAACNTTTCAACATTT`, cov pos 7/15/23/31/39 | top-strand each filter arm + non-ACGT pos5 | only `7 + GAACA CAT` and `31 + GAACN CNT` emitted; GTACA(R), GAACG(pos5=G), CAACA(pos1=C) skipped | ✓ §3.2, Q4/A14 |
| B2 | `AAATGTTCAAAGTACGTACGT`, cov pos 5/12/16/20 | bottom-strand filter arms + `pos-1` report | only `5 - GAACA CAT` emitted; GTACT(R), GTACG(pos5=G) skipped | ✓ §3.3, `pos-1` |
| B3 | `TTTTTGAAC`, cov pos 9 (end AC) | top-strand end-truncation; filter-passes-but-`len<3`-guard-kills | **empty** output (drach `GAAC` passes filter, tri `C` len1 → guard skip) | ✓ §3.2.5, V10 |
| B4 | `AAAGTA`, cov pos 4 (end GT) | bottom truncated-5-mer that *passes* (pos5 missing) and tri len≥3 | `4 - TACT CTT` emitted | ✓ §3.3, A14 (truncated→pass producing output) |
| B5 | `GTACGTACGT`, cov pos 1/5/9 | chromosome-start `pos<4` negative-substr (GT at idx0 → `pos=2`) | empty (start motif: drach `A` len1 / tri `AC` len2 → guard skip) | ✓ §3.3.6 / V7 (negative wrap exercised, killed by guard) |
| B6 | direct Perl `substr` probes (`-1,5`/`-2,3`/`-9,3`/`-12,3`) | exact negative-offset semantics vs the Rust `perl_substr` helper | `-2,3`→`GT`, `-1,5`→`T` **match**; but `substr("ACGT",-5,3)`→`"AC"`, `-9,3`→`undef`/`""` — Rust helper returns `ACG` (see Important-1) | partial — see Important-1 |
| B7 | suffixed `-o foo.CpG_report.txt --drach` | filename no-strip | `foo.CpG_report.txt_DRACH_report.txt` / `..._DRACH.cov` | ✓ §3.1, V4 |
| B8 | `--drach --zero_based` vs `--drach` | zero_based ignored | byte-identical | ✓ §3.0.3, V13 |
| B9 | `--drach --CX` | no mutex, early-exit | exit 0, **no** CX report/summary, DRACH produced | ✓ §3.0.2, V1 |
| B10 | `--drach --merge_CpGs` | no mutex, early-exit | exit 0, **no** merge/discordant output, DRACH produced | ✓ §3.0.2, V1 |
| B11 | 2-chr MFA `chr1`/`chr2`, `--split_by_chromosome` | `.chr<NAME>` infix + `.chrchr1` doubling | `samp.chrchr1_DRACH_report.txt`, `samp.chrchr2_...` | ✓ §3.1, V4/V12 |
| B12 | same, single file, cov order chr2-then-chr1 | insertion (cov-appearance) order, not sorted | chr2 line precedes chr1 line | ✓ §3.5.3 (never BTreeMap) |
| B13 | one chr with both top(+7) and bottom(-15) hits | `+` before `-` within a chromosome | `7 +` then `15 -` | ✓ §3.5.2 |
| B14 | empty `.cov` | empty files vs `die` (Q2) | **exit 0, two 0-byte files**; STDERR warns `uninitialized value $chromosomes{""}` (final flush with `last_chr=""`) | ✓ §3.5.5 / V8 — **but see Important-2** |
| B15 | uncovered bottom motif, threshold 1 | threshold-skip | absent from both files | ✓ V9 |
| B16 | `-o … --coverage_threshold 5`, motif cov=4 | explicit threshold survives auto-set | 0-byte report (4 < 5) | ✓ §3.0.4, V2 |
| B17 | duplicate cov `chr1 7 … 1 9` then `… 9 1` | last-write-wins | reports `9 1` (later wins) | ✓ §3.5.4 |
| B18 | `--drach --gzip` | gz topology + content | `_DRACH_report.txt.gz`/`_DRACH.cov.gz`; gunzip == plain golden | ✓ V11 |
| B19 | `TTTNAACATTTGNACATTT`, cov 7/15 | non-ACGT at DRACH pos1 / pos2 | `7 + NAACA CAT` emitted (N≠C passes pos1); `GNACA` skipped (N∉{A,G}) | ✓ V3 (extends the non-ACGT case) |
| B20 | `--CX` on B2 fixture, pos 5 | cross-check 3: drach vs CX agree on bottom position | `--CX` emits `5 - CHH CAT` — same pos + same trinucleotide as drach `5 - … CAT` | ✓ §3.6 cross-check 3 |

**Every behavioral claim in the plan reproduced byte-for-byte on live Perl.** The DRACH filter is exactly `byte[0]!=b'C' && byte[1]∈{A,G} && byte5_or_missing!=b'G'`; the bottom strand reports at `pos-1`; the truncated-5-mer passes pos-5; the filter runs before the `len<3` guard; standalone early-exit + no-mutex + threshold-auto-set + zero_based-ignore + raw-`-o` filenames all hold.

---

## Findings by area

### Logic — correct; two precision notes

- **DRACH filter semantics (§3.2.4, §10 Q4): verified exact.** B1/B4/B19 confirm the literal byte-tests, including a non-ACGT pos-5 (`N`) *passing* (`GAACN` emitted) and a non-ACGT pos-1 (`NAACA` passes) / pos-2 (`GNACA` fails). The `is_drach_motif` signature in §4 is precisely right.
- **Truncated <5-mer "pos-5 missing → not-G → pass" (§3.2.5, A14): verified PRODUCING OUTPUT.** B4 (`AAAGTA` → `4 - TACT CTT`) is the strongest possible confirmation: a 4-byte drach passes the filter and emits. The plan's wording is exact.
- **Filter-before-`len<3`-guard ordering (§3.2.5): NOT byte-observable — plan slightly overstates.** B3 shows the end-AC drach `GAAC` passes the filter but is then killed by the `len<3` tri guard → empty output. Because **neither** the filter-skip nor the guard-skip has any side effect before the `next` (no counter, no partial write), the *relative order* of the two skips is unobservable: any motif reaching the guard with `tri.len()<3` is dropped regardless of filter outcome, and any motif failing the filter is dropped regardless of tri length. The plan calls the order "a byte-identity fact" / "keep that order" — harmless to preserve, but it is **not** load-bearing for byte-identity. (Optional wording fix; no code impact — keeping the Perl order is still the safe default.)

### Assumptions — all validated

- **A6–A14** all reproduced (B7–B19 above). Nothing speculative survives unverified.
- **A12 (bottom `pos-1` = intended C anchor):** independently re-derived and confirmed via B2 + B20 (the `--CX` report keys the same bottom C at the same position with the same trinucleotide). Felix's resolution is sound; the bottom strand is a plain byte-identical port.
- **A15 (`pos<4` negative-substr wrap via `perl_substr`):** confirmed *reachable and faithful for the offsets DRACH actually uses* — see Important-1 for the boundary nuance.

### Efficiency — fine

- Two O(genome) linear scans + O(cov) parse, single-threaded, one chromosome buffered at a time, no extra genome copy. Identical posture to the shipped GpC walk (`gpc.rs`). No concerns.
- Minor: the `AC`/`GT` scans cannot overlap (the two dimer bytes differ), so a naive `i += 1` advance is output-equivalent to Perl's non-overlapping `/(AC)/g` `pos()`. The plan should state the advance explicitly (Optional-1) but there is no correctness risk either way.

### Validation-sufficiency — strong, with two coverage gaps

The V1–V14 matrix pins the genuinely-risky paths well (each filter arm, truncated-5-mer-that-passes, `pos<4` wrap, ordering, empty cov, gzip, split, threshold). Gaps:

1. **No explicit "non-ACGT at DRACH pos-1/pos-2" golden** (only pos-5 is called out in V3). B19 shows pos-1 `N` *passes* and pos-2 `N` *fails* — worth an explicit `is_drach_motif` unit assertion so a future refactor (e.g. switching to an enum/whitelist instead of the literal byte-tests) can't silently change it. (Important-3.)
2. **No 2-chromosome single-file ordering golden** distinct from the split golden (V12 is split-mode only). B12 proves cov-appearance order matters in single-file mode too (chr2-before-chr1). Add a single-file 2-chr golden. (Important-3, same fix area.)
3. **Empty-cov final-flush guard is asserted as behavior (V8) but not as an implementation hazard** — see Important-2.

### Alternatives — none needed

The early-exit-branch-in-`lib::run` mirroring Perl `:38-42`, reusing the GpC per-chr-buffer/flush/split skeleton, is the right design. No alternative worth the churn.

---

## Action items

### Critical
*(none)*

### Important
1. **Document the `perl_substr` negative-offset boundary and confirm DRACH never crosses it.** Live Perl: `substr("ACGT",-5,3)` → `"AC"` and `substr(s,-9,3)` → `undef`/`""` (Perl subtracts the over-shoot from the *length*, not just clamping the start). The shipped Rust `perl_substr` (report.rs:99–111) instead clamps `start` to 0 and keeps the full `want`, so it returns `"ACG"` for `-9` — and report.rs:652 *asserts* that wrong value. **For DRACH this is unreachable**: the most-negative offsets used are `pos-4` and `pos-3` with `pos = i+2 ≥ 2`, so offset `≥ -2 ≥ -len` for any chromosome of length ≥ 2 (the minimum that can contain a `GT`), and I confirmed (B6, `substr("GT",-2,3)="GT"`, `substr("GT",-1,5)="T"`) the helper matches Perl at that boundary. **Add one sentence to §3.3.6 / A15 stating that DRACH offsets never go below `-len`, so the helper's known divergence at `offset < -len` cannot fire.** (Optionally file the pre-existing `perl_substr`/test bug separately — it is out of Phase-2 scope but is a latent correctness issue the plan currently leans on without bounding.)
2. **Pin the empty-cov final-flush guard explicitly.** B14: with an empty `.cov`, Perl never sets `$last_chr`, then unconditionally runs the *final* `drach_filtering_top/bottom_strand($last_chr, %chr)` with `$last_chr = "" (undef)`, which warns `uninitialized value $chromosomes{""}` and walks an empty/undef sequence (no output) — exit 0, two 0-byte files. The Rust `run_drach` mirrors Phase-1's `cur_chr: Option<…>` skeleton, where the final flush is `if let Some(prev) = cur_chr.take()` — i.e. it **skips** the flush when no cov line was ever read. That still yields two 0-byte files (the writers are opened up-front in single-file mode), so the *output* is byte-identical, but the plan should state (a) the writers must be created **before** the read loop in single-file mode (so empty cov still produces the two empty files), and (b) the Rust correctly **omits** the phantom `""`-chromosome flush (it must NOT call `genome.get("")` / must not panic). Add this to §3.5.5 / step 6 so the implementer doesn't accidentally `unwrap` a never-set `last_chr`.
3. **Add two goldens to the V-matrix:** (i) a `is_drach_motif` unit asserting non-ACGT at **pos-1 passes** and **pos-2 fails** (B19), and (ii) a **single-file** 2-chromosome ordering golden with cov in non-sorted appearance order (B12) — V12 currently only covers split mode.

### Optional
1. State the `AC`/`GT` scan advance explicitly in §3.2/§3.3 (e.g. "`i += 1`; overlap is impossible for these distinct-byte dimers, so this is output-equivalent to Perl's non-overlapping `/g`").
2. Soften §3.2.5 / §3.5.2 wording: the filter-before-guard *order* is not byte-observable (both are side-effect-free skips); keeping the Perl order is correct but is a robustness choice, not a byte-identity requirement.
3. Note that the cov parse inherits the v1.0 accepted divergence (strict `u32` → `MalformedCovLine` vs Perl's lenient coercion); harmless on real `bismark2bedGraph` output, but worth a one-liner so it isn't rediscovered.

---

## Bottom line

The plan is **correct and implementable as written** — I could not find a byte-identity divergence, off-by-one, or missing guard in its stated logic; every surprising claim reproduced on live Perl v0.25.1. The three Important items are completeness/coverage refinements (a documented bound on the reused `perl_substr` helper, an explicit empty-cov final-flush guard, and two extra goldens), none of which block implementation. Recommend addressing them in the plan text before the implement trigger.

# CODE_REVIEW_B — Phase 2b: HISAT2 paired-end read-1 `ZS` asymmetry fix

**Reviewer:** B (independent, fresh context) · **Date:** 2026-06-05
**Target:** uncommitted 2b diff on `rust/aligner-v1x` @ `376a6d9`, crate `rust/bismark-aligner`
(`src/{lib,merge,report}.rs`, `tests/cli.rs`).
**Oracle:** repo-root `bismark` v0.25.1. **Build:** 233 lib + 44 integration = **277 tests green**, `clippy --all-targets -D warnings` clean, `cargo fmt --check` clean (all re-run here).

## Verdict: **APPROVE** — no Critical/High. The fix is correct, faithful, and minimally scoped.

The single new production conditional (`merge.rs:609-613`) reproduces Perl's read-1-has-no-`ZS`-branch
asymmetry exactly. I independently re-derived all four mate-tag cases against the Perl backfill
(3466-3473) and the no-second-best branch (3593), and confirmed the masked outcomes match Perl
byte-for-byte. SE and Bowtie 2 paths are untouched. Findings below are Low/Medium polish + two
latent-assumption notes worth recording — none block the merge or the gate.

---

## Independent verification of the five review angles

### 1. Is `merge.rs:609` the ONLY consumer of `r1.second_best` in the PE path? — CONFIRMED
Exhaustive grep of `second_best` across `src/`:
- `r1.second_best` / `r2.second_best` are read in the PE function at **exactly one site** (L612/L614).
- Every other PE `second_best` reference is `StoredPair.sum_second_best` (L683, L695, L738), which is
  populated **only** from the `Some(sum_second)` computed at L621/L636 from the **already-masked** `sb1`,
  or `None` (L651). I traced `insert_pair` (L746-783): it stores `sum_second_best` verbatim; it never
  re-reads `r1.second_best`. So no unmasked read-1 value can leak into the stored record, the
  cross-instance-tie path (L687-698), the single-mate MAPQ path (L683), or `amb_same_thread`.
- `Stored.second_best` (L160) belongs to the **SE** struct `check_results_single_end` only — not the PE
  path, and SE is intentionally unmasked. No cross-contamination.
- The OQ-2b-2 claim that L609 is the single sufficient chokepoint **holds against the code**, not just on faith.

### 2. All 4 mate-tag cases vs Perl backfill (3466-3473) + the `sb1.is_some()||sb2.is_some()` gate — CONFIRMED
Read the Perl read-1 loop (3372-3382: `if AS / elsif XS:i: / elsif MD:Z:` — **no `ZS` branch**) and read-2
loop (3386-3403: `else { if($bowtie2){XS} else{ZS} }` — **does** capture `ZS`). Walking each case:

| Case | Inputs (HISAT2) | Perl | Rust (masked) | Test asserts |
|---|---|---|---|---|
| A both ZS | r1 ZS=-6, r2 ZS=-6 | sb1 undef→as1=0, sb2=-6 → **-6** | sb1 None→0, sb2=-6 → **-6** | `Some(-6)` ✓ (not -12) |
| B Bowtie 2 both | r1 XS=-6, r2 XS=-6 | sb1=-6, sb2=-6 → **-12** | unmasked → **-12** | `Some(-12)` ✓ |
| C r1 ZS only | r1 ZS=-6, r2 none | both undef → gate FALSE → **no-second-best** branch | sb1 None, sb2 None → gate false → **None** | `None` ✓ (+ Bowtie 2 contrast `-6`) |
| D r2 ZS only | r1 none, r2 ZS=-6 | sb1 undef→as1=0, sb2=-6 → **-6** | sb1 None→0, sb2=-6 → **-6** | `Some(-6)` ✓ |

**Case C subtlety (the load-bearing one):** I confirmed Perl **cannot** compute a second-best here.
Under HISAT2, read-1 physically cannot set `$second_best_1` (no `ZS` branch in the read-1 loop), and
Case C has read-2 with no tag → both undef → `if (defined sb1 or defined sb2)` is false → the
no-second-best `else` (3593). The Rust mask makes the gate false identically. The `None` outcome is
exactly what Perl produces, flipping `calc_mapq` to the no-second-best ladder (cap 42). Verified
`calc_mapq` (mapq.rs:29-46 vs 48-114) genuinely yields a different MAPQ for `None` vs `Some(...)` and
for `-6` vs `-12` (`best_diff = |as_best.abs() − sec.abs()|`), so every case is **byte-visible**.

### 3. Interaction with `Stored.second_best` / `insert_pair` — CONFIRMED clean
PE uses `StoredPair`, not `Stored`. `insert_pair` receives the masked `Some(sum_second)`/`None` and stores
it verbatim. The MAPQ second-best selection (L695: `match b.sum_second_best { Some(sb) if sb > runner_up … }`)
reads the masked-derived value. There is **no path** by which an unmasked `r1.second_best` reaches the
stored record or MAPQ.

### 4. Test rigor — UNIT tests assert the consequence; INTEGRATION test does not (see L-1)
The four unit tests (`pe_hisat2_mate1_zs_is_ignored`, `pe_bowtie2_mate1_second_best_is_kept`,
`pe_hisat2_mate1_only_demotes_to_no_second_best`, `pe_hisat2_mate2_only_backfills_mate1`) assert the exact
`sum_of_alignment_scores_second_best` value (-6 / -12 / None / -6) — i.e. the **consequence**, which flows
verbatim into `calc_mapq`. They are real catches, not smoke. The discriminator is correctly the `aligner`
**argument** (parser unifies XS/ZS into one field), so the `Some(-6)` test inputs faithfully stand in for
HISAT2's `ZS`. No false coverage: the Bowtie 2 contrast cases (B, and C's `Some(-6)` branch) prove the mask
is HISAT2-only and not a no-op. See L-1 re: the integration test.

### 5. Regression surface — CONFIRMED all call sites updated; `run_pe` stays Bowtie 2
- Production PE call site: `lib.rs:1231` updated with `config.aligner` (drive_merge_pe reads it from its
  `&RunConfig`, no signature churn; `Aligner` is `Copy`).
- `parallel.rs` does **not** call `check_results_paired_end` (it calls `process_pe_chunk` → `drive_merge_pe`)
  — confirmed by grep; the rev-1 "one call site, no parallel.rs edit" is accurate.
- Test call sites: `merge.rs:1316` (run_pe_aln) and the error-path test at `merge.rs:1697` (now passes
  `Aligner::Bowtie2`) both updated. `run_pe` delegates with `Aligner::Bowtie2`, so all pre-existing PE
  tests are semantically unchanged. SE function signature untouched.

---

## Findings

### Medium
- **M-1 (latent assumption, not a code bug): the mask assumes HISAT2 read-1 NEVER emits `XS:i:`.**
  Perl's read-1 loop *does* have an `elsif XS:i:` branch — so if HISAT2 ever put an `XS:i:` tag on a read-1
  record, **Perl would capture it as `$second_best_1`**, whereas the Rust mask drops read-1's second-best
  *unconditionally* for HISAT2 (it keys on `aligner == Hisat2`, not on which tag was present). The two
  diverge in that hypothetical. The plan's premise (HISAT2 emits `ZS`, not `XS`, on read-1) is almost
  certainly correct for HISAT2 2.2.2, and the 1M PE oxy gate (V8) would surface any violation, but this
  assumption is **not asserted in code** and not called out as an explicit risk in §8. Recommend: add a
  one-line note to §8/Self-Review that the mask is broader than Perl's `XS`-only read-1 capture and is
  justified only because HISAT2 read-1 emits `ZS` exclusively — and rely on the gate to confirm. No code
  change needed if the gate passes; this is a documentation/traceability gap, not a correctness defect for
  real HISAT2 output.

### Low
- **L-1 (test coverage nuance): the integration test `hisat2_pe_mapped_names_and_report` exercises the mask
  code path but does not assert its numeric effect.** The pair maps to a single instance (CT only; GA
  unmapped) → `entries.len()==1` → MAPQ derives from the stored masked `sum_second_best`, but the test only
  asserts the BAM record **count** (2), the `hisat2` naming token, and the report text. The mate-1 `ZS:i:-2`
  in the fake is consumed (masked) but the resulting MAPQ is never checked. This is acceptable — the unit
  tests (V2-V4b) are the designated reliable catch (OQ-2b-3) and the oxy gate is the at-scale proof — but
  the comment on the test ("The mate-1 `ZS` is consumed by the merge (masked), not emitted") slightly
  over-claims: nothing in the test would *fail* if the mask were removed (both mates carry ZS, so removing
  the mask changes only the MAPQ value, which isn't asserted). Optional: assert the BAM MAPQ equals the
  masked expectation to make the integration test a genuine end-to-end guard of the fix, not just naming.

- **L-2 (cosmetic): emoji in a production source comment.** `merge.rs:601` carries a `🔴` glyph in the
  doc/comment block. It's harmless and clippy/fmt-clean, but non-ASCII in source comments is unusual for
  this crate's style; consider plain text for consistency. (Not a blocker; matches the plan's own prose.)

- **L-3 (observation, no action): `r1_second_best` binding vs the plan's `sb1_src` name.** The plan §4/§5
  sketch used `sb1_src`; the implementation named it `r1_second_best`. The implemented name is clearer
  (it mirrors `r1.second_best`). No issue — noting only that the code is the source of truth.

---

## What I did NOT find (explicitly checked, all clean)
- No second read site of `r1.second_best` anywhere in the PE path (angle 1).
- No leak of unmasked read-1 second-best into `StoredPair`, the tie path, the single-mate path, or
  `amb_same_thread` (angle 3).
- No SE regression (different function, untouched) and no Bowtie 2 behavior change (`aligner != Hisat2`).
- No missed call site of `check_results_paired_end` (prod + 2 test sites all threaded; no `parallel.rs` call).
- Backfill arithmetic, the `is_some()||is_some()` gate, key-form selection (`min_max_key`), and the
  no-second-best branch all match Perl 3466-3593.

## Recommendation
APPROVE for the PE oxy byte-identity gate (V8). Address M-1 as a one-line plan/comment note (no code change),
and optionally L-1 (assert MAPQ in the PE integration test) to harden end-to-end coverage. L-2/L-3 are
cosmetic. The 1M PE gate + the `--ambig_bam` PE cell remain the required at-scale confirmation per the plan.

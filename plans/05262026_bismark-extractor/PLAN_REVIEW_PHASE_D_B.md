# Phase D Plan Review — Reviewer B

**Plan:** `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PHASE_D_PLAN.md` (rev 0)
**Date:** 2026-05-26
**Reviewer:** B (independent, no shared state with Reviewer A)

## Verdict

**NEEDS-REVISIONS** — one byte-identity correctness issue (finalize order vs Perl) and one
SPEC-vs-Perl-vs-test inconsistency that would silently mis-document the column header. Both
are small fixes; once addressed the plan is APPROVE-quality.

---

## Critical

### C1. `finalize` step order contradicts Perl single-core ordering

Plan §4.5 declares: `split-file flush → M-bias.txt → splitting report`, and claims this
"matches Perl line ordering at `:314-317`."

Reading the Perl single-core path:

- In `process_single_end_read_file` (and the PE equivalent) the splitting report is emitted
  inline at `bismark_methylation_extractor:2463`:
  ```perl
  if ($multicore == 1){
      print_splitting_report ();
  }
  ```
  That call returns to the main flow, which then proceeds at `:314-317`:
  ```perl
  unless ($mbias_off){
      produce_mbias_plots ($filename);
  }
  ```
- So the single-core Perl effective order is **splitting_report → M-bias.txt**. The plan's
  Rust order inverts that.

This matters for two reasons:

1. **Byte-identity at filesystem-snapshot level.** If Phase H ever compares mtime / on-disk
   ordering or runs a diff that streams files in a deterministic listing, no file content
   changes — but the user-observable order in stderr/logs (and in any "first warn line"
   that the splitting report prints) differs. The plan claims Perl parity but doesn't
   actually achieve it on this point.
2. **Failure semantics.** If `write_mbias_txt` errors out (e.g. disk full), the Perl run
   has already emitted the splitting report; the plan's Rust order would lose the
   splitting report entirely when the M-bias.txt write fails. That's a regression in
   diagnostic information for a partial-failure case.

**Action:** Swap the order in `finalize` to `flush → splitting_report → M-bias.txt`, and
update the §4.5 commentary + §2 table row "Writer trigger" to cite the real Perl call
sites (`:2463` + `:314-317`), not the misleading "`:314-317`" reference alone.

### C2. SPEC §4.2 column-count error: "4-col" vs Perl's 5-col

Plan §2 row "Column header" and §9.2 (Q1) both flag that SPEC §4.2 says 4 columns but
Perl emits 5 (`position\tcount methylated\tcount unmethylated\t% methylation\tcoverage`).
The plan defers the SPEC fix to "follow-up task" (§16 item 1).

Two problems:

1. **Plan-of-record drift.** Leaving the SPEC wrong while implementing the 5-col version
   means any later reviewer (or any agent loading the SPEC as context) will see
   contradictory specs and may "fix" the writer to 4 columns. The SPEC is the contract.
   Phase H will catch the bug, but the cost of edit is one line — fix it inside Phase D's
   PR.
2. **SPEC §10 row D may also need a tweak.** Quickly verify Phase H's byte-identity gate
   list mentions M-bias.txt explicitly; the column count was almost certainly cited
   somewhere else (e.g. epic doc, recon notes). Audit and reconcile.

**Action:** Promote follow-up §16 item 1 into Phase D's implementation outline §6 as an
explicit task. ~5 LOC edit. No risk; high consistency win.

---

## Important

### I1. `is_paired` threading: consider resolving `PairedMode::AutoDetect` once, upstream

Plan §4.6 adds `is_paired: bool` to `ExtractState` as a separate field. The justification
in §2 ("Empty PE BAM would yield empty mbias[1] too") is sound — `mbias[1].max_position()`
alone can't disambiguate. But there's a cleaner alternative.

Today `ResolvedConfig.paired_mode` is a tri-state including `AutoDetect`, and
`main.rs::run` already does the auto-detect dispatch (`cli.rs:107-110`). Looking at the
current code:

```
$ grep "match config.paired_mode" rust/bismark-extractor/src/main.rs
106:    match config.paired_mode {
107:        PairedMode::SingleEnd => extract_se(...)
108:        PairedMode::PairedEnd => extract_pe(...)
109:        PairedMode::AutoDetect => ...
```

By the time the binary enters `extract_se` or `extract_pe`, the SE-vs-PE branch is
already taken — there is no remaining AutoDetect ambiguity inside the pipeline. So
`is_paired` is a direct function of which entry point we're in. The plan's design is
fine, but a slightly cleaner alternative for Phase E/F to consider:

- Make `ExtractState::new` *infer* `is_paired` from the call site by accepting a
  resolved `PairedMode::{SingleEnd, PairedEnd}` (panic on `AutoDetect`), or
- Drop `is_paired` as a separate field and let `write_mbias_txt` accept it as a direct
  argument from `extract_pe` / `extract_se` (skipping the field on `ExtractState`
  altogether — the writer doesn't actually need any other piece of state to make this
  decision, just the mbias array).

This isn't a blocker — the plan's choice works. But Phase E's mode dispatch is going to
revisit this exact area, and it's worth a sentence in §9.2 acknowledging the alternative.

**Action:** Optional — add a row in §9.2 noting the alternative ("infer from caller
identity instead of storing") and the reason it wasn't chosen (the writer is called from
`finalize`, which doesn't know the caller).

### I2. Smoke-test edits ride on a Phase B/C-touching PR — review-hygiene risk

Plan §7.2 modifies `tests/se_phase_b_smoke.rs` and `tests/pe_phase_c_smoke.rs` to assert
M-bias.txt content. Both files belong to PRs (#849, #851) still in review. If Phase D's
PR (stacked on Phase C) modifies them, then:

1. The base-PR diff visible to reviewers grows.
2. Any change requested upstream on the smoke files conflicts with Phase D and forces
   re-rebase.
3. A reviewer of PR #849 / #851 sees Phase D edits that don't belong to their PR (only
   visible until the rebase, but confusing).

Cleaner: add a **new** `tests/mbias_writer_phase_d_smoke.rs` that runs the SE binary AND
the PE binary and asserts M-bias.txt content. Leaves `se_phase_b_smoke.rs` /
`pe_phase_c_smoke.rs` untouched.

**Action:** Replace the §7.2 plan to extend existing smoke files with a new
`mbias_writer_phase_d_smoke.rs` (still §7.2). Drop the extension from Phase B/C smoke
files.

### I3. `ExtractState::new` callsite ripple count is non-trivial

Plan §3.2 and §6 step 7 describe the signature-change ripple as small. Actual count:

```
$ grep -c "ExtractState::new" rust/bismark-extractor/tests/se_phase_b.rs
5
$ grep -c "ExtractState::new" rust/bismark-extractor/src/pipeline.rs
2
$ grep -c "ExtractState::new" rust/bismark-extractor/tests/pe_phase_c.rs tests/sanity.rs
0
```

So: 5 + 2 = 7 callsites need editing. That's 7 lines of `, /*is_paired=*/ false)` (or
`true`). Not a code-quality problem, just a planning-clarity nit — §6 step 7 says "Phase
B + C tests that construct `ExtractState` directly need a `false` arg added" but doesn't
quantify. State the count for downstream reviewer confidence.

**Action:** Update §6 step 7 to "5 sites in `tests/se_phase_b.rs`, 2 sites in
`src/pipeline.rs`; 0 sites in `tests/pe_phase_c.rs`, `tests/sanity.rs`."

---

## Optional

### O1. `max_position` semantics: empty-vec saturating math is correct, but document it

Plan §4.3:
```rust
let m1 = self.cpg.len().saturating_sub(1) as u32;
```

This is correct given Phase B/C's invariant that `accumulate(_, pos_1based, _)` calls
`Vec::resize_with(pos_1based + 1, ...)` — i.e. for `pos_1based = N`, the vec ends up
length `N + 1` with the highest valid index at `N`. So `len.saturating_sub(1)` returns
`N`, matching Perl's max-key scan.

The corner cases:
- `len == 0` (vec never touched) → `0`. Writer loops `1..=0` which is empty in Rust
  (verified: `RangeInclusive` with `start > end` yields zero elements). Matches Perl's
  `foreach my $pos (1..0)`.
- `len == 1` (only slot 0 was ever allocated, e.g. by a defensive grow at construction
  time) → `0`. Same empty loop. Fine — but flag in a comment that "len=1 means no real
  data."

**Action:** Add a doc-comment line to `MbiasTable::max_position` clarifying the
invariant (positions are 1-based, slot 0 is reserved/unused, return value of 0 means
"no data observed").

### O2. Position-0 footgun: writer silently drops slot-0 data

Plan §4.7 row "Position 0 in the mbias vec" notes that `route_call` always passes
`pos_1based >= 1`, so slot 0 is never written. The writer iterates `1..=max_position`,
ignoring slot 0. If a future kernel change introduced a 0-based pos bug, the writer
would silently drop the slot-0 data from M-bias.txt — and Phase H would catch it only at
the byte-comparison gate, not at the unit-test level.

Cheap defense: `debug_assert!(table.cpg.get(0).map_or(true, |p| p.is_default()))` (or
similar) at the top of `write_mbias_txt`. Fires loud in `cargo test`; zero cost in
release.

**Action:** Optional — add a `debug_assert!` in `write_mbias_txt` checking that slot 0
of each context vec is `MbiasPos::default()`. Document the assumption in a comment.

### O3. `%.2f` rounding semantics

Plan §4.2 row "Percent format" uses `format!("{:.2}", ...)`. Rust's `{:.2}` uses
round-half-to-even (banker's rounding) for `f64` formatting since Rust 1.65 (via the new
default `f64::to_string`). Perl's `sprintf("%.2f", ...)` defers to the platform libc,
which is round-half-away-from-zero on macOS/glibc.

The edge case is values like `0.125`:
- Rust `{:.2}` → `"0.12"` (round to even)
- libc → `"0.13"` (round away from zero)

For Phase D's specific math (`100.0 * meth / (meth + un)`), the exact-half edge is
*possible* but rare: e.g. `meth=1, un=7` → 12.5000% → both `12.50` (no rounding
required); `meth=125, un=875` → 12.5% → same. To trigger the divergence you need the
last-digit binary representation of `100.0 * meth / total` to land at a midpoint, which
on f64 is uncommon but not impossible.

Phase H byte-identity will catch any divergence. But it's cheaper to know in advance:

**Action:** Optional — add a Phase D test like
`write_mbias_txt_percent_rounding_matches_perl_at_midpoint` with a handful of
(meth, un) values chosen to land exactly on `.5` percent boundaries. If any diverges
from Perl's expected output, switch to a manual half-away-from-zero rounding helper
before Phase H rather than after.

### O4. Filename test for the "no extension" case

Plan §7.1 first row covers `sample.bam`, `sample.sam.gz`, `sample.cram`, `sample`. The
"no extension" case is the one most likely to surprise a reader: `sample` → `sample`
→ file `sampleM-bias.txt` (no dot between basename and `M-bias.txt`). Confirmed against
Perl `:632-642`: with no `.bam`/`.sam`/`.cram`/`.txt`/`.gz` suffix, the regex chain is a
no-op, `$mbias` stays as `"sample"`, then the file path is `$output_dir . "sample" .
"M-bias.txt"` = `sampleM-bias.txt`.

Plan's test row already covers it. Just verify the asserted filename in the test is
exactly `"sampleM-bias.txt"` (no dot), and add an explicit comment in the test that this
matches Perl's quirky no-extension behaviour.

**Action:** Optional — comment the test row to flag the no-dot-in-output behaviour.

### O5. `derive_mbias_basename` vs `derive_basename` — name and discoverability

Plan §4.1 introduces `derive_mbias_basename` alongside `pipeline::derive_basename`. The
divergence (one strips `.bam`, the other strips `bam`) is subtle. A future maintainer
looking for "the basename function" via `rg derive_basename` will find both and may
guess wrong.

Cheap mitigations:
- Doc-comment on each function explicitly cross-references the other and explains why
  they differ.
- A lock-down test fixture in `tests/mbias_writer_phase_d.rs` that exercises
  `(input, derive_basename(input), derive_mbias_basename(input), mbias_txt_path(...))`
  side-by-side for `sample.bam`, `sample.sam`, `sample.bam.gz`, `sample.cram`,
  `sample`. Makes the divergence visible in a single test.

The plan already covers each function's tests; what's missing is the side-by-side
comparison.

**Action:** Optional — add one combined fixture test asserting both helpers' outputs
together, to lock the divergence.

### O6. Empty-mbias semantics with `mbias_off == false` and zero records

§4.7 row "Empty input BAM" — verified against Perl: with `%mbias_1 = ()`, the
`max_length_1 = 0` init survives (inner foreach never runs), so the `1..0` loop
emits nothing, just headers. Plan matches. Worth adding one explicit smoke test (already
done in §7.1 `write_mbias_txt_empty_mbias_emits_headers_only`).

No action.

---

## Validation sufficiency

Unit-test coverage in §7.1 looks strong: header bytes, column-header bytes, per-position
row bytes (with-calls and zero-coverage variants), max_position empty/single/cross,
SE-vs-PE section counts, finalize gates. The plan covers all the byte-edge cases I would
have asked about.

Gaps:
- **Rounding edge cases** (see O3) — not in the test set.
- **`max_position` with vec length 1 only** (slot-0-only vec) — not tested. Cheap to add.
- **Side-by-side basename comparison** with `derive_basename` (see O5).
- **`finalize` ordering test** — once C1 is fixed, add a test that asserts the
  splitting-report file mtime ≤ M-bias.txt mtime (or that the splitting report is
  written even when M-bias.txt write fails).

---

## Efficiency

Section §8 is accurate: O(max_length) per section, 8-KiB BufWriter, runs once. No
concerns. Phase F (multicore) plan-row §11 correctly identifies that mbias merging
happens before the writer call.

---

## Alternatives worth considering

1. **Skip `is_paired` as a state field** (see I1) — let the writer accept it as a
   parameter from `finalize`, threaded from the caller of `finalize`. Reduces struct
   field count. Trade-off: `finalize` signature grows by one bool. Probably not worth it
   now; mentioned for Phase E consideration.

2. **Write M-bias.txt incrementally** (e.g. per-section) instead of one big write at
   finalize. Not worth it — total output ≤ 100 KB, single-shot is simpler and matches
   Perl's open/write/close pattern at `:644-:836`.

3. **Defer SPEC fix to a separate PR** (status quo per §16) — disagree (see C2). The
   SPEC fix is a one-line edit; bundling it with Phase D minimizes drift.

---

## Summary

Plan is well-structured, Perl line citations are mostly accurate, and the test plan
covers the byte-identity cases. Two issues need fixing before implementation: the
finalize step order (C1: Perl writes splitting_report **before** M-bias.txt, not after)
and the SPEC §4.2 4-col vs 5-col inconsistency (C2: fix in this PR rather than deferring).
The smoke-test extension strategy (I2) is a review-hygiene nit worth addressing while
making the C1/C2 fixes. Everything else is `Optional`.

**Verdict: NEEDS-REVISIONS** — Critical items C1 (finalize order) and C2 (SPEC fix
in-PR) need addressing. ~10 LOC of edits. After that, APPROVE.

# CODE REVIEW A — `bismark-filter-nonconversion` (port of Perl `filter_non_conversion`)

**Reviewer:** Code Reviewer A (fresh context).
**Date:** 2026-05-31.
**Scope:** `rust/bismark-filter-nonconversion/src/{filter,error,filename,report,cli,pipeline,lib,main}.rs`
+ `tests/{byte_identity,edge_cases,byte_identity_real_data}.rs` + `tests/data/generate_goldens.sh`.
**Ground truth:** Perl `filter_non_conversion` v0.25.1 (724 lines), SPEC rev 1, IMPLEMENTATION_NOTES.

## Verdict: **APPROVE.**

The port is a faithful, well-documented reproduction of the Perl semantics and the byte-identity
contract holds across every case I verified. Build is clean, all 66 tests pass (55 unit + 1
byte-identity-9-cells + 8 edge + 2 ignored real-data), `clippy --all-targets -D warnings` is
clean, `fmt --check` is clean. I made **no source changes** — I found no unambiguous low-risk
defect worth fixing; the only findings are Low-severity faithfulness/robustness notes on
non-gated paths.

## Verification performed

- `cargo test -p bismark-filter-nonconversion` → **66 passed, 0 failed** (2 ignored real-data).
- `cargo clippy -p bismark-filter-nonconversion --all-targets -- -D warnings` → clean.
- `cargo fmt -p bismark-filter-nonconversion -- --check` → clean.
- **Live Perl v0.25.1 + samtools 1.21 spot-checks** (beyond the committed goldens), all
  **body-byte-identical + report-byte-identical** to Perl:
  - SE `--percentage_cutoff` at cutoffs **12 / 13 / 20**: the half-to-even tie **5/40 = 12.5%**
    (kept at 13, removed at 12), the **2/16 = 12.5%** tie, exact-20.0 boundary (`Hhhhh`),
    min-count gating (`HHHH` 100%-but-<5 kept), `1/6→16.7`, `5/6→83.3`. **IDENTICAL.**
  - SE `--consecutive` with **transparent** chars `Z`/`u`/`U`/`.` interleaved (run survives) vs
    **reset** chars `z`/`h`/`x` (run breaks), plus dot-transparent. **IDENTICAL.**
  - PE default routing: R1-passes/R2-fails, R1-fails (R2 not examined), both-pass, neither-reaches-3.
    **IDENTICAL.**
  - SE auto-detect from `@PG` (no `-s`/`-p`). **IDENTICAL.**
  - **N/A non-dotted branch** (`emptyfoobam`, header-only): report `0 (N/A%)`, exit 0 —
    **byte-identical** to Perl (confirms SPEC §4.3 C1 reachability).
  - **PE lone-trailing-R1 die**: Perl exit 255 / Rust exit 1 (both nonzero, error path, not gated);
    **both leave a 0-byte report**; **both write the 2 prior complete-pair records** and the kept
    bodies are **byte-identical**. (Output BAM file sizes differ only by header/`@PG` framing —
    excluded by the D1 body-only gate.)

## Issues by area

### Logic — consistency with Perl semantics
- **Correct.** The `filter.rs` XM decision matches Perl exactly: only `H`/`X` increment
  `non_cpg_count`; `H`/`X`/`h`/`x` increment `total_non_cg`; `Z`/`z`/`u`/`U`/`.` are non-counting;
  consecutive reset on exactly `z`/`h`/`x`; increment → reset → threshold-check ordering with an
  early `return true` mirroring Perl's `last`. Percentage mode scans the whole string and only the
  min-count-gated `round_1dp(…) >= cutoff` decides — the per-char threshold is correctly *not*
  applied (Perl `unless (defined $percentage_cutoff)` guard). `round_1dp` (`format!("{:.1}")`)
  reproduces Perl `sprintf("%.1f")` round-half-to-even; **verified against live Perl at the 12.5%
  tie boundary**, the highest-risk rounding case.
- **PE path correct.** Two-at-a-time; either-mate-fails-pair via `||` short-circuit (R2 not
  examined when R1 fails); both-mates-must-have-nonempty-XM (Perl `and`-truthiness) via
  `.filter(|s| !s.is_empty())`; lone-trailing-R1 `None` → die with prior pairs flushed and 0-byte
  report. Adjacent-qname check (with legacy `/1`,`/2` strip) folded into the loop, matching the
  Perl pre-pass's only effective check.
- **CLI validation order correct** (percentage block → `-s`/`-p` exclusion → unconditional
  threshold validation), matching Perl `process_commandline`; the no-files check precedes option
  validation in `main` (Perl `@ARGV`-empty at line 513). `--threshold` co-supplied with
  `--percentage_cutoff` is accepted/ignored but still validated — faithful and tested.
- **Filename derivation correct**: dot-anchored `.bam`-strip only, **no directory strip** (distinct
  from dedup), path preserved; `foobam` → no strip.

### Efficiency
- No concerns. Single streaming pass; raw `RecordBuf` passthrough; the peek-then-`chain` re-stream
  is O(1) overhead (one stashed record). `read_fails` is a tight byte loop with an early exit in
  threshold mode. `mimalloc` global allocator pinned as in siblings. `String::with_capacity(256)`
  for the report. Nothing to optimize.

### Errors / robustness
- **No panics on data-reachable paths.** Every non-test `unwrap`/`expect`/`unreachable` is provably
  infallible: `cli.rs:162` `unreachable!` is guarded by the prior `BothSingleAndPaired`
  early-return; `round_1dp`'s `expect` parses a `{:.1}`-formatted float (always valid);
  `report.rs` `expect("write to String never fails")` is on infallible `String` writes. All
  record-decode I/O errors flow through `?` → `BismarkFilterError`. `extract_xm(...).unwrap_or(b"")`
  and the qname `unwrap_or(b"")`/`strip_suffix(...).unwrap_or(...)` are total. `#![forbid(unsafe_code)]`.
- **Writers finalised before error propagation** (`kept_w.try_finish()?; removed_w.try_finish()?;`
  then `stream_result?`), so a mid-stream die leaves valid partial BAMs + 0-byte report — verified
  against live Perl.

### Structure / style
- Module layout matches SPEC §9; docs are unusually thorough and cite exact Perl line numbers.
  Naming is clear and idiomatic. The four report Line-B variants and the SE/PE space quirk are
  isolated in `report.rs` with explicit byte-exact unit tests. No duplication of concern.

## Findings (all Low — non-gated / theoretical)

- **L1 — `i64 as u32` truncation for `--threshold` / `--minimum_count`** (`cli.rs:119,142`).
  Both are validated `> 0` but have no upper bound; a value `> u32::MAX` (e.g. `--threshold
  5000000000`) silently truncates, whereas Perl keeps the full integer. Could in principle flip a
  decision, but only with an absurd CLI value (real thresholds/min-counts are tiny). Recommend a
  guard or `u64`/`usize` counters if you want strict parity; otherwise document as accepted (the
  decision-relevant counters in `filter.rs` are `u32`, so the comparison would also need widening).
- **L2 — Truncation detected only at header/first-record; mid-stream truncation → generic `Io`
  error + partial output** (`pipeline.rs`). Perl's `bam_isTruncated` runs up-front and dies before
  opening writers, so it never writes partial output on a truncated file. The Rust maps only the
  *initial* read error to `Truncated`; a BGZF/EOF error surfacing mid-stream becomes
  `BismarkFilterError::Io` and leaves partial BAMs. Error path, message bytes not gated — an
  accurate divergence analogous to the documented SPEC §10.5 PE case, but **not currently
  documented**. Recommend adding a §10 deviation note.
- **L3 — Missing the `Checking file >>$file<< for signs of file truncation...` STDERR notice**
  that SPEC §4.2 explicitly calls for. STDERR-only, not gated. Either emit it (one `eprintln!` on
  the dotted-`.bam` branch) or strike the requirement from the SPEC.
- **L4 — Closing advisory abbreviated** (`lib.rs:81`): Rust prints "Please continue with
  deduplication or methylation extraction now"; Perl appends "(depending on your application)".
  STDERR, not gated. Trivial.
- **L5 — PE empty-XM truthiness vs Perl `"0"`** : Rust uses `!s.is_empty()`, Perl uses string
  truthiness where the literal `"0"` is *falsy*. XM is never `"0"` (it is a `.HXZhxzuU` string), so
  no real divergence; noting for completeness (the SPEC's "absent or empty" framing is faithful).
- **L6 — `predicates` dev-dependency is unused** (`Cargo.toml:41`); `grep` finds zero references.
  Harmless; remove to keep the manifest honest.
- **L7 — Rust qname check covers *all* PE pairs; Perl's pre-pass stops at 100,000 reads.** Rust is
  stricter (arguably more correct) and cannot break byte-identity on well-formed Bismark PE input
  (R1/R2 always share a qname). Noting as a benign behavioral difference on malformed input only.

## Recommendations (prioritized)

- **Critical:** none.
- **High:** none.
- **Medium:** none.
- **Low:** L1 (decide: guard the cast or document the truncation-on-huge-value as accepted),
  L2 + L3 (add the mid-stream-truncation deviation note and/or emit the truncation STDERR notice
  per SPEC §4.2), L6 (drop unused `predicates`). L4/L5/L7 are informational.

## Fixes applied

**None.** No source file was modified. All findings are Low-severity, on non-byte-gated paths
(STDERR text, error-path messages, theoretical extreme-value casts) where the right action is a
documentation/SPEC decision rather than an unambiguous code fix — outside the "fix directly"
mandate. The byte-identity-critical logic (XM decision, rounding, report formatting, SE/PE
streaming, PE die semantics, N/A branch, filename derivation) is correct and proven against live
Perl v0.25.1.

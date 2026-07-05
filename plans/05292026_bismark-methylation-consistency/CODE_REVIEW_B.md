# CODE_REVIEW_B — `bismark-methylation-consistency`

**Reviewer:** B (independent; fresh context; no shared state with Reviewer A)
**Date:** 2026-05-29
**Scope:** the new crate `rust/bismark-methylation-consistency` (`lib`, `main`, `error`, `cli`, `classify`, `filename`, `report`, `logging`, `pipeline`, `tests/integration.rs`, `Cargo.toml`), cross-checked against the Perl `methylation_consistency`, the SPEC/PLAN, the `bismark-io` API it builds on, and the `bismark-dedup` sibling idiom.
**Build/test status (verified this session):** `cargo test -p bismark-methylation-consistency` → **48 lib + 16 integration tests pass** (incl. the 3 `perl_vs_rust_*` byte-identity tests, which actually ran — perl + samtools present). `cargo clippy --all-targets` → **clean, no warnings**. The implementation-notes claim of green tests is confirmed.

---

## Summary

This is a faithful, well-structured port. The hard-to-get-right pieces are all correct:

- **Round-then-compare** classification (`classify.rs`) is pinned to the exact op-order (`meth as f64 / total as f64 * 100.0`), formats to one decimal, parses back, and compares inclusive `<= lower` / `>= upper`. This matches Perl `methylation_consistency:266,272,282` exactly. The unit tests pin the load-bearing boundary cases (`10.04→unmeth`, `10.05→mixed`) and the power-of-two ties.
- **Report templates** (`report.rs`) are byte-for-byte identical to Perl lines 334–343 (verified character-by-character against the source this session: 5 spaces before the first `-`, label spacing 4/2/1/3, exactly 49 hyphens, no leading `\n`, no trailing blank line, `{:.2}` percentages, `N/A` when total 0). PE counts pairs, not records.
- **count_xm** counts exactly `Z`/`z` (or `H`/`h`), ignoring everything else — matching Perl `tr/Z//` / `tr/z//`.
- **filename.rs** correctly keeps the full input path, strips one trailing `.bam` only, does NOT basename-strip (the documented #1 trap vs `bismark-dedup`). Outputs land adjacent to the input.
- **finalize-on-all-paths**: traced every early-return and `?`; on the success/graceful-stop path `writers.finish()` runs; on the fatal-error path `let _ = writers.finish()` runs before propagating. No path drops an opened `BamWriter` without `finish()`. (One narrow caveat — Medium 1 below.)
- **Empty-file skip**, **PE odd-trailing-R1 drop**, **exact-qname mate check (no /1,/2 stripping)**, **PE coordinate-sort guard (SE accepts it)**, **missing-XM graceful stop**, and the **`min_count == 0` skip** are all implemented and tested matching the SPEC decisions.

No **Critical** issues. The findings below are about (a) one strictness divergence whose graceful-stop scope is narrower than Perl's leniency (already documented as accepted, but worth a final confirmation), and (b) a small set of robustness/clarity items that could bite on real 10M data which the synthetic tests do not exercise.

---

## Issues by area

### Logic / correctness

**[High 1] Missing/invalid XR/XG and XM-length mismatch are FATAL in Rust but silently processed (XR/XG) or graceful-stopped (XM) in Perl — only `MissingTag{"XM"}` triggers the graceful stop.**

`pipeline.rs::is_missing_xm` matches *only* `BismarkIoError::MissingTag { tag } if *tag == "XM"`. But `BismarkRecord::from_noodles_record` (bismark-io `record.rs:116`) errors on several other conditions that Perl tolerates:

- **Missing `XR` or `XG`** → `MissingTag{"XR"}`/`{"XG"}` → falls into the `Err(e) => return Err(e.into())` arm → **fatal, nonzero exit.** Perl reads *only* `XM` (line 209), so a Bismark-ish record with `XM` but no `XR`/`XG` is **processed normally** by Perl and would land in a bucket.
- **Invalid XR/XG combination** (anything other than the 4 canonical `CT`/`GA` pairs) → `InvalidStrandTags` → **fatal.** Perl never inspects XR/XG, so this is processed.
- **`XM.len() != seq.len()`** → `XmSeqLengthMismatch` → **fatal.** Perl never compares XM to the sequence, so this is processed.

This is **documented and accepted** in SPEC §4.1 / PLAN decision #2 ("keep `BismarkRecord` for max reuse; document the strictness"), and the argument that genuine Bismark BAMs always carry all three tags with matching lengths is sound. **I am not asking to change the behavior** — but I want to flag two things the SPEC's framing glosses over:

1. SPEC §4.1 says "Perl would `warn`+`last` (missing XM) **or process it (missing XR/XG)**." The Rust port does *neither* for missing XR/XG — it **aborts the whole file fatally** (nonzero exit, partial/empty BAMs, no report on the fatal path because `process_file` returns `Err` before writing the report). That is a *stronger* divergence than "process it": Perl would still emit the record and finish; Rust drops the entire file's report. On genuine data this never fires, but the SPEC text undersells the divergence direction.
2. There is **no test** asserting this behavior either way (the graceful-stop test only covers missing-XM). If a future bismark-io change widened `MissingTag` to other tags, or if real data ever carries a record missing XR/XG (e.g. a hand-edited or third-party BAM), this would surface as a confusing hard failure. A short integration test documenting "missing XR/XG → fatal error, nonzero exit" (or, if desired, widening the graceful-stop to all `MissingTag` variants to better mirror Perl's `last`) would lock the intent.

**Recommendation:** No code change required for v1.0 acceptance. Either (a) add a one-line test pinning the missing-XR/XG → fatal behavior so the divergence is explicit and regression-guarded, or (b) if matching Perl's leniency is preferred, broaden `is_missing_xm` to treat any `MissingTag`/`InvalidStrandTags`/`XmSeqLengthMismatch` as the graceful-stop trigger (Perl's missing-XM `last` is the closest analog, and for the XR/XG case Perl would actually *process* the record — so neither (a) nor (b) is a perfect match; (a) + an explicit doc note is the lowest-risk choice). Confirm with the user which semantics are intended on malformed input.

**[Medium 1] Secondary (0x100) / supplementary (0x800) alignments pass the `bismark-io` filter and could desync PE R1/R2 pairing — but Perl reads them too, so output diverges either way on such input.**

The reader's iterator filter (`bismark-io read.rs:580 filter_unmapped_then_classify`) drops only `FLAG & 0x4` (unmapped). Secondary/supplementary records flow through. Perl's `samtools view` (no `-F`) also emits them. So:

- **SE**: both Perl and Rust process secondary/supplementary records identically (each as its own read) → no divergence.
- **PE**: `stream_pe` pairs records strictly two-at-a-time by adjacency. If a secondary/supplementary record sits between a genuine R1 and R2, *both* Perl and Rust would mis-pair (Perl pairs by consecutive `<IN>` lines exactly the same way). So this is **output-equivalent to Perl** on such input — but note the SPEC's §4.2 claim ("Bismark emits only mapped concordant pairs") is the real guarantee here; the 0x4-only filter does *not* by itself protect PE adjacency. The unmapped filter is the one asymmetry vs Perl: a single unmapped mate in a "pair" *would* be dropped by Rust (desyncing every subsequent pair) but kept by Perl. On genuine Bismark PE BAMs (only concordant pairs, both mates mapped) this cannot happen.

**Recommendation:** No change. This is correctly reasoned in SPEC §4.2. Worth a sentence in the SPEC/PLAN clarifying that the *unmapped* filter (not secondary/supplementary) is the PE-adjacency risk, and that it is null on real data. Real-data Phase D (PE 10M) is the right place to confirm empirically.

**[Medium 2] `Counts` uses `u32` counters; `count_xm` increments per byte. On a single pathological/huge XM string this could overflow in debug (panic) / wrap in release.**

`Counts { meth: u32, unmeth: u32 }` and `meth += 1` per matching byte. A `u32` overflows at ~4.29e9. No single read's XM is anywhere near that (reads are ≤ ~10^3 bp; even with InDels the XM is read-length-bounded), and PE sums only two reads — so this is **not reachable on real data**. Perl uses arbitrary-precision scalars and never overflows. The `Tally` counters are `u64` (good — those accumulate over the whole file). This is fine as-is; I note it only because the prompt asked about overflow risk. The per-read `u32` is more than sufficient.

**Recommendation:** None. (If you wanted belt-and-suspenders, `total()` returning `meth + unmeth` is also `u32` and is fed into `rounded_percent(meth, total)` as `f64` — no overflow there either since both are ≤ read length.)

### Efficiency

**[Low 1] `name_string` / `MateMismatch` only allocate on the error path — good. `count_xm` is a single pass — good. No bottlenecks.**

The hot path (`stream_se`/`stream_pe` → `count_xm` → `classify` → `write`) is allocation-free except `rounded_percent`'s `format!` + `parse` per routed read. That `format!`+`parse` round-trip is **required** for Perl parity (round-then-compare) and is gated behind the discard/skip checks (only runs for records that will actually be classified), so it is not wasted work. `String::with_capacity(256)` in `render` is called once per file. No concern.

**[Low 2] `resolve_mode` serializes the entire header to SAM text per file via `detect_paired_from_header`.** This is inherited from `bismark-io` (it round-trips the header to text to find the Bismark `@PG`). It runs once per file, not per record, so it is negligible. No change.

### Errors / robustness

**[Medium 3] On the fatal-error path, `let _ = writers.finish()` discards the finalize result — which can mask a *write/flush* error, but more importantly leaves three partially-written BAMs on disk with no cleanup, and writes NO report.**

`process_file` error arm (`pipeline.rs:156-161`):
```rust
Err(e) => {
    let _ = writers.finish();
    Err(e)
}
```
This is the right *intent* (don't leave EOF-less BAMs), and the `let _` is acceptable for the EOF marker (we're already returning an error, so a secondary finalize error shouldn't override the primary cause). Two observations:

- **No report on the fatal path.** Perl, on a *fatal* condition (e.g. `die` for mate mismatch), also writes no report — so this matches. But note: on the fatal path the three BAMs are left on disk (finalized, valid, but containing only the records seen before the error). Perl leaves the same partial files. So this is **output-equivalent** for the fatal case. Fine.
- **The masked error is harmless here** (we're propagating a more important error), so the `let _` is justified — unlike a success-path `let _` would be. Good judgment. I would add a one-line comment noting *why* the result is intentionally discarded only on the error path (the success path correctly uses `writers.finish()?`).

**Recommendation:** No behavior change. Optionally add a comment clarifying the asymmetry (success: `?`; error: `let _`).

**[Low 3] `is_coordinate_sorted` is duplicated from `bismark-io`'s private `check_not_coordinate_sorted`.** `pipeline.rs:276` re-implements the SO-field check because the bismark-io function is private and only fires on `BamReader::new` (not `without_sort_check`). The duplication is small (8 lines) and correct (same `SORT_ORDER`/`COORDINATE` constants). This is the documented consequence of the "open no-sort-check, guard PE manually" decision (SPEC §4.11). Acceptable; a future bismark-io could expose a public `header_is_coordinate_sorted(&Header) -> bool` to dedupe, but that is out of scope.

### Structure / style

**[Low 4] `error.rs` doc-comment numbering points at Perl lines that don't all line up.** E.g. `NoInputFiles` cites "Perl's usage `die` (line 131)" — correct. `MateMismatch` cites line 239 — correct. `UpperThresholdOutOfRange` cites line 76, `LowerThresholdOutOfRange` line 85 — correct. These are accurate. No issue; verified.

**[Low 5] `MethConsError::NoInputFiles` message hard-codes the binary name in the usage string** (`methylation_consistency_rs [--min-count=5] [bam file(s)]`). Perl's string is `split_bismark_by_consistency [--min-count=5] [bam file]` (line 131) — an old internal name. Since error/usage text is explicitly **out of the byte-identity gate** (SPEC §7, §4.4), updating it to the real Rust binary name is the **correct** choice and an improvement over Perl's stale string. No change needed; flagging only so it is a conscious decision, not an oversight.

**[Low 6] `cli.rs` `unreachable!("clap conflicts_with prevents this")` for `(single_end, paired_end) == (true, true)`.** This is sound — clap's `conflicts_with` rejects it at parse, and there is a test (`rejects_both_single_and_paired`). Clean.

**[Low 7] `--version` with `disable_version_flag = true` + manual handling in `main.rs`.** `Cli::version` is a plain `bool` flag, and `files` is a `Vec<PathBuf>` (not required-min-1 at the clap layer — the empty check is in `validate()`). So `--version` with no files parses fine (test `version_flag_parses_without_files` confirms) and short-circuits in `main` before `validate()`. Correct. Mirrors the dedup precedent.

---

## Cross-check against the acceptance contract (SPEC §7)

| Gate item | Verdict |
|---|---|
| `_consistency_report.txt` byte-for-byte | **PASS** — templates verified vs Perl lines 334–343 char-by-char; `perl_vs_rust_*` tests assert byte-equality on SE/PE/CHH. |
| Populated BAMs identical at record level, in order (PE: R1 then R2) | **PASS** — `route` writes r1 then (if paired) r2; integration tests assert order; `perl_vs_rust_*` assert per-bucket record sets. |
| Header: `@HD`/`@SQ`/`@PG ID:Bismark` preserved; `@PG ID:samtools*` excluded | **PASS (intended divergence)** — header written verbatim via `header.clone()`; Rust adds no `@PG`; the test harness's `samtools_record_set` strips the header. |
| Empty buckets = valid empty BAMs (not Perl 0-byte) | **PASS (intended divergence)** — eager-open all three writers; `se_empty_bucket_is_a_valid_empty_bam` confirms. |
| Bucket counts == report | **PASS** — `Tally::record`/`discarded` increment in lockstep with the writes. |

The only residual is **Phase D real-data byte-identity on colossal** (10M SE/PE/CHH), which is correctly deferred and which the `perl_vs_rust_*` harness generalizes to. Real-data is also where High 1 (strictness) and Medium 1 (PE unmapped filter) would be empirically confirmed as null — synthetic tests cannot.

---

## Recommendations, prioritized

| # | Priority | Item | Action |
|---|---|---|---|
| High 1 | **High** | Missing XR/XG / invalid strand / XM-length-mismatch are fatal (abort whole file, no report), unlike Perl which processes them. Only `MissingTag{"XM"}` graceful-stops. | Confirm intended semantics with user. Add an explicit test pinning the "missing XR/XG → fatal, nonzero exit" behavior; OR broaden the graceful-stop. Update SPEC §4.1 wording (it implies Perl-leniency is preserved for XR/XG; it is not). |
| Medium 1 | Medium | PE *unmapped*-mate filter (not secondary/supplementary) is the real R1/R2-desync risk vs Perl; null on real Bismark data. | No code change. One clarifying sentence in SPEC §4.2. Confirm in Phase D PE run. |
| Medium 3 | Medium | Error-path `let _ = writers.finish()` is correct but undocumented; leaves partial (finalized) BAMs + no report (matches Perl's fatal case). | No behavior change. Add a one-line comment on the success/error asymmetry. |
| Medium 2 | Low/Med | Per-read `u32` counters (overflow only on absurd inputs; unreachable on real reads). | None. |
| Low 1–7 | Low | Style/clarity (duplicated SO check, stale-name-corrected usage string, doc-line citations). | All acceptable as-is; optional polish (e.g. a public `bismark-io` SO-check helper to dedupe Low 3). |

**Bottom line:** No Critical or blocking issues. The byte-identity-critical logic (round-then-compare, report templates, count_xm, filename derivation, PE pair counting, finalize-on-all-paths) is correct and well-tested. The one item worth an explicit decision before sign-off is **High 1** — the fatal-vs-process divergence on records missing XR/XG, which is documented as accepted but whose actual behavior (abort the whole file with no report) is stronger than the SPEC's "process it" wording implies and is currently untested. Recommend a confirming test (or a broadened graceful-stop) plus a SPEC wording tweak.

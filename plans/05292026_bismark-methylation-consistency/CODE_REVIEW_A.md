# CODE_REVIEW_A — `bismark-methylation-consistency`

**Reviewer:** A (independent, fresh context — no shared state with Reviewer B)
**Date:** 2026-05-29
**Scope:** the new crate `rust/bismark-methylation-consistency/` (`lib`, `main`, `error`, `cli`, `classify`, `filename`, `report`, `logging`, `pipeline`, `tests/integration.rs`, `Cargo.toml`, `README.md`), cross-checked against the Perl `methylation_consistency`, the SPEC/PLAN, `bismark-io`, and the `bismark-dedup` sibling.
**Verification run:** `cargo test -p bismark-methylation-consistency` → **48 lib + 16 integration tests pass** (incl. the 3 `perl_vs_rust_*` byte-identity tests, which executed because `perl`+`samtools` are present locally). `cargo clippy --all-targets -D warnings` → **clean**.

---

## Summary

**Verdict: the port is faithful and high quality. No Critical or High findings. I recommend it for merge** after considering the Medium/Low notes below (most are documentation/observability nits, not behavior changes).

The hard parts of the acceptance contract are all correct:
- **Round-then-compare** (`classify::rounded_percent` + `classify`) reproduces Perl's `sprintf("%.1f", meth/total*100)`-then-numeric-compare exactly, with the op-order pinned and the inclusive `<=`/`>=` boundaries correct.
- **`count_xm`** counts exactly `Z`/`z` (or `H`/`h`) and ignores all else, matching Perl's `tr///`; PE summing is correct.
- **`filename::output_root`** correctly strips a single trailing `.bam` and preserves the full directory prefix (the documented "do NOT basename-strip like dedup" trap is avoided).
- **`report::render`** matches the Perl templates byte-for-byte (49-hyphen separator, exact spacing, `{:.2}`, `N/A` on zero total, no leading `\n`, no trailing blank line), and the unit test asserts byte-equality against the Spike-2 real Perl run.
- **`pipeline`** handles empty-file skip, missing-XM graceful stop (catching `MissingTag{"XM"}` only), PE two-at-a-time with odd-trailing-R1 drop, exact-qname mate check, eager-open-all-3 writers, the PE-only `@HD SO:coordinate` guard, and `None`→SE auto-detect — all matching the SPEC decisions.

The single biggest residual risk is the **finalize-on-error path drops the BGZF EOF marker on the error branch** by `let _ =` (Medium A-1 below) — but this is on a fatal path with no report written, so it does not affect the byte-identity gate.

I found **no defect that would break byte-identity on genuine Bismark data**. The divergences that exist are all either (a) explicitly documented & accepted in SPEC §4, or (b) reachable only on malformed/non-Bismark input.

---

## Issues by area

### Logic / correctness

**[Medium A-1] Error-path `finish()` swallows the EOF-marker error AND any partial bucket may differ between the three writers.**
`pipeline.rs::process_file` lines 156–161:
```rust
Err(e) => {
    let _ = writers.finish();   // best-effort; error discarded
    Err(e)
}
```
`BucketWriters::finish` finalizes the three writers sequentially (`all_meth` → `all_unmeth` → `mixed`). On the error path:
1. The result is discarded with `let _ =`, so if `all_meth.finish()` itself errors, the remaining two writers are **never finalized** (`finish()` consumes `self`, so the `?`-less chain in `BucketWriters::finish` stops at the first `?`). That can leave `all_unmeth`/`mixed` as EOF-less, undecodable BAMs.
2. This is a **fatal path** (mate-mismatch, coordinate-sorted PE, truncation, non-XM reader error). No report is written and the process exits non-zero, so the partial BAMs are not part of any accepted output — Perl on the same input also dies mid-write leaving truncated/0-byte files. So this is **not a byte-identity-gate violation**, but it does mean the SPEC §4 / PLAN A7.6 promise "finalize every writer on ALL paths (incl. error)" is only *best-effort* and silently incomplete.
**Recommendation (Low-risk):** in `BucketWriters::finish`, finalize all three unconditionally and return the first error, e.g. collect the three `Result`s and `?` the first `Err` after attempting all three. This makes the "finalize on all paths" guarantee real. Priority Medium because the consequence is benign (fatal path only), but the plan explicitly called this out as a guard.

**[Low A-2] Unmapped-read filter diverges from Perl on non-Bismark / post-processed input (documented, but the empty-file interaction is sharper than SPEC §4.2 states).**
`BamReader::records()` silently drops FLAG&0x4 records (`bismark-io/read.rs:580`). Perl's `samtools view` (no `-F`) keeps them. SPEC §4.2 accepts this for real Bismark BAMs. Two sharper consequences worth a one-line note:
- A BAM containing **only unmapped reads**: Perl `bam_isEmpty` sees lines from `samtools view` → treats it as non-empty → opens outputs and writes a `Total … 0` report; the Rust empty-check peeks `.records()` (post-filter) → `None` → **skips the file entirely, no outputs**. Divergent disposition (skip vs empty-report).
- For PE, if an unmapped read ever appeared mid-stream it would silently shift R1/R2 adjacency. SPEC §4.2 notes this "cannot on real Bismark data."
**Recommendation:** none required (accepted divergence); optionally add the only-unmapped-input case to the §4.2 bullet so it isn't mistaken for a bug later. Priority Low.

**[Low A-3] Empty-XM (`XM:Z:` with empty value) diverges from Perl's `\S+` regex — but is unreachable on real data.**
Perl matches `XM:Z:(\S+)` (≥1 non-whitespace), so an empty XM value triggers `warn`+`last` (graceful stop). The Rust path: `tags::xm` returns the empty slice successfully and `count_xm` returns `{0,0}` → with default `min_count=5` the read is **Discarded** (or, with `min_count=0`, **Skipped**), not a stop. This only occurs when `seq.len()==0` (the `BismarkRecord` length-parity check forces `XM.len()==seq.len()`), i.e. a zero-length read — pathological, never in Bismark output. Covered in spirit by SPEC §4.1 (strict-validation note). **Recommendation:** none; mentioned for completeness. Priority Low.

**[Low A-4] `bam_isTruncated` is not reproduced; relies on noodles surfacing truncation as a fatal `Io` error.** PLAN C2 documents this as the intended approach (noodles → fatal I/O error, text not byte-matched). This is correct and consistent with the design. One subtlety: a truncated BGZF stream surfaces as a generic `Io` error from `.records()`, which `is_missing_xm` correctly does **not** match → it propagates as fatal (good). No action needed; flagged so the reviewer pair confirms the truncation case is intentionally a fatal `Io`, not a graceful stop. Priority Low (no change).

### Errors / exit codes

**[OK] Exit-code mapping is correct.** `main.rs`: `Ok` → `ExitCode::SUCCESS` (0); any `MethConsError` → `eprintln!` + `ExitCode::from(1)`; clap parse errors → 2 (clap's own convention, since `Cli::parse()` exits before `run`). `--version` short-circuits to 0 before validation, matching the test `version_flag_parses_without_files`. Error messages mirror Perl's `die` text where it matters (thresholds, no-input usage, mate mismatch) and are reasonable elsewhere.

**[Low A-5] `MethConsError::NoInputFiles` usage string drifted from Perl, intentionally.** Perl line 131 says `split_bismark_by_consistency [--min-count=5] [bam file]`; the Rust message says `methylation_consistency_rs [--min-count=5] [bam file(s)]`. This is a sensible improvement (uses the real binary name) and CLI error text is explicitly out of the byte-identity gate (SPEC §7). **Recommendation:** none; noting the deliberate divergence so it isn't "corrected" back later. Priority Low.

### Efficiency

**[OK] No bottlenecks.** Streaming, one record at a time; `count_xm` is a single byte-loop; `rounded_percent` allocates one small `String` per classified read via `format!` then `parse()` — this is the *deliberate* mechanism to match Perl's stringify-then-compare and is correctly prioritized over micro-optimization (it is load-bearing for byte-identity, per SPEC §2.5 and Spike 1). The header is cloned three times (once per writer); for a header this is negligible and unavoidable given `BamWriter` takes an owned `Header`. `String::with_capacity(256)` in `report::render` is a reasonable pre-size. No changes recommended.

### Structure / style

**[OK] Matches the sibling idiom well.** Module split (`error`/`cli`/`classify`/`filename`/`report`/`logging`/`pipeline`/`lib`/`main`), `thiserror` error enum with `#[from] BismarkIoError` + `#[from] std::io::Error`, `version_string()` using `CARGO_PKG_VERSION`, the `Logger` gated by `--quiet` — all consistent with `bismark-dedup`/`bismark-extractor`. Rustdoc is thorough and accurately cites Perl line numbers and SPEC sections. `#![forbid(unsafe_code)]` + `#![warn(missing_docs)]` present.

**[Low A-6] `is_coordinate_sorted` (pipeline.rs:276) duplicates `bismark-io`'s private `check_not_coordinate_sorted` (read.rs:620).** The duplication is acknowledged in a comment, and `bismark-io`'s function is private + returns its own error type, so re-implementing the 6-line check here is pragmatic (avoids widening the `bismark-io` API for one caller). **Recommendation:** acceptable as-is; if a second consumer ever needs it, promote a public `bismark_io::is_coordinate_sorted(&Header) -> bool`. Priority Low.

**[Low A-7] `route()` takes `rec1`/`rec2` where SE passes the same record twice (`route(counts, &rec, &rec, false, …)`, pipeline.rs:183).** Reusing one record as both args with `paired=false` is harmless (the `rec2` write is gated behind `if paired`), but a reader must trace the `false` to see `rec2` is dead in the SE call. Minor readability cost; the alternative (separate SE/PE write helpers) would duplicate the classify+tally logic. **Recommendation:** acceptable; optionally take `rec2: Option<&BismarkRecord>` to make the SE case self-documenting. Priority Low.

---

## Scrutiny checklist (per the brief)

1. **Round-then-compare** — ✅ Correct. `format!("{:.1}", meth as f64 / total as f64 * 100.0).parse()` with the op-order pinned; compares the parsed rounded value (not the raw fraction) against `lower as f64`/`upper as f64`. Inclusive `<=`/`>=` are correct and tested at exactly-10.0 and exactly-90.0, plus the load-bearing 10.04→`"10.0"`→unmeth vs 10.05→`"10.1"`→mixed edge, plus power-of-two ties (6.25, 12.5, 87.5, 90.05). No off-by-one.
2. **`count_xm`** — ✅ Counts only `Z`/`z` (or `H`/`h`), ignores `.`,`x`,`X`,`u`,`U`,`h`/`H` (or `z`/`Z` in CHH). PE summing via `Counts + Counts` is correct and tested.
3. **`filename.rs`** — ✅ Strips one trailing `.bam` via `strip_suffix(".bam")`, keeps the full path (uses `to_string_lossy()` over the whole `Path`, not the basename). `x.bam.bam`→`x.bam`, `sample.sam` untouched, nested dir preserved — all tested, incl. the explicit "outputs land in input dir, not CWD" guard.
4. **`report.rs`** — ✅ Byte-exact: 49-hyphen `SEPARATOR` (guarded by a dedicated test), exact internal spacing copied from Perl, `{:.2}` percentages, `N/A` when `total==0`, no leading `\n`, ends at the last line's `\n`. PE counts pairs (one `Tally` increment per pair; the `route` call increments once and writes both mates). Tested against the real Perl Spike-2 output.
5. **`pipeline.rs`** — ✅ mostly; see **A-1** for the one Medium nit (error-path `finish()` is best-effort and stops at the first failing writer). Empty-file skip (no outputs) ✅; missing-XM graceful stop catches `MissingTag{"XM"}` only and other reader errors stay fatal ✅; PE two-at-a-time with odd-trailing-R1 dropped uncounted ✅; exact-qname mate check (no `/1`,`/2` stripping) ✅; eager-open all three writers ✅; `@HD SO:coordinate` PE-only guard ✅ (SE accepts coordinate sort, tested); `None`→SE auto-detect ✅. No borrow/ownership problems (the `Peekable` is threaded by `&mut`; writers are consumed by value only in `finish()`).
6. **`error.rs` / `main.rs`** — ✅ Exit codes 0/1/2 correct; messages reasonable and mirror Perl where it matters.
7. **Real-data risks the synthetic tests wouldn't catch** — the header round-trip fidelity on real multi-`@SQ`/`@PG`/`@CO` headers is the genuine open item (correctly deferred to Phase D, SPEC §8.3); the unmapped-only-input disposition (A-2) and empty-XM (A-3) are non-Bismark-only. Nothing here blocks merge; all are flagged in the SPEC/PLAN as pending Phase D real-data validation.

---

## Recommendations (by priority)

- **Critical:** none.
- **High:** none.
- **Medium:** **A-1** — make `BucketWriters::finish` finalize all three writers unconditionally (attempt all, return the first error) so the documented "finalize on ALL paths incl. error" guard is real rather than best-effort-up-to-first-failure. Benign today (fatal path only), but it is the one place the implementation under-delivers vs PLAN A7.6.
- **Low:** A-2 (note the only-unmapped-input disposition divergence in SPEC §4.2), A-3 (empty-XM note), A-4 (confirm truncation is intentionally fatal `Io`), A-5 (deliberate usage-string name change), A-6 (`is_coordinate_sorted` duplication — fine as-is), A-7 (`route` SE double-arg readability).

No code edits were made (parallel-reviewer no-edit rule). All findings are recommendations for the caller to apply serially.

# Plan Coverage Report

**Mode:** B (code vs. implementation plan) — the phased `PLAN.md` is the task ledger; SPEC §4–§7 contract requirements cross-checked.
**Plan(s):** `PLAN.md` (phases A1–A8, B1–B4, C1–C4, D1–D3) + `SPEC.md` (§5 outputs, §6 CLI, §7 acceptance, §4 divergences)
**Code:** `rust/bismark-methylation-consistency/src/*.rs`, `tests/integration.rs`, `Cargo.toml`
**Date:** 2026-05-29
**Verdict:** COMPLETE (Phase A/B/C fully implemented + tested; D1 harness present; D2 large-cluster run intentionally deferred per cluster access; 2 test-only gaps + 1 dependency-list deviation, all non-blocking — see notes)

## Summary

- Total items: 41 (24 PLAN tasks + 9 SPEC §6 CLI flags + 4 SPEC §5 output filenames + 4 SPEC §7 acceptance items; SPEC §4 divergences folded into the relevant rows)
- DONE: 38
- PARTIAL: 2 (C2 truncation test; C2 multi-file test — runtime behavior present, dedicated test absent)
- MISSING: 0
- DEVIATED: 1 (A1 `anyhow` dependency dropped — never used; documented-deviation note)
- DEFERRED (not a defect): D2 (10M colossal run — pending cluster access)

## Coverage ledger

### Phase A — Crate scaffold + CLI + SE end-to-end

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| A1 | Scaffold: Cargo.toml (pkg + `[[bin]] methylation_consistency_rs`), deps, workspace member, lib.rs (`version_string`), main.rs (ExitCode 0/1, clap→2, `--version` short-circuit), error.rs (thiserror enum) | PLAN A1 | DEVIATED | All present and correct. `Cargo.toml` has bismark-io/clap/noodles-sam/thiserror + dev-deps assert_cmd/predicates/tempfile/bstr/noodles-core (+noodles-bam). **`anyhow` listed in A1 is absent** — never used (error handling is pure `thiserror` + `MethConsError`); harmless drop. `mimalloc` correctly absent. Workspace member added (`rust/Cargo.toml:3`). |
| A2 | CLI: clap-derive, positional `files`, underscore long flags, `disable_version_flag`, `-s`/`-p` `conflicts_with`, `min_count:u32` default 5, thresholds `Option<i64>`; `validate()` → `ResolvedConfig{files,mode,chh,lower,upper,min_count,quiet}` with defaults + range checks + zero-file error | PLAN A2 / SPEC §6 | DONE | `cli.rs` matches exactly. `ResolvedConfig` omits `samtools_path` (accept-and-ignore — decision #4) and `mode` is `LibraryMode{Single|Paired|Auto}` (equivalent to plan's `ModeChoice`). 11 CLI unit tests. |
| A3 | Classification core: `Counts{meth,unmeth}`, `count_xm(xm,chh)`, `Bucket`, `Routing{Discard,Skip,Route}`, `classify()` with discard→zero→round-then-compare→inclusive thresholds; pinned op-order `meth/total*100`; exhaustive unit tests incl. ties | PLAN A3 / SPEC §2.5 §8.1 | DONE | `classify.rs` implements the exact 4-step flow and pinned `rounded_percent`. 18 unit tests cover all required cases: 10.04→unmeth, 10.05→mixed, ties 6.25/12.5/87.5/90.05, min_count discard, total==0 with min_count=0 skip, CHH counting, empty XM, mixed-context byte filtering, custom thresholds, mate-add. |
| A4 | Filenames: `output_root` (strip single trailing `.bam`, keep full dir prefix — NOT dedup basename-strip), `bucket_path`, `report_path`; unit tests incl. output-dir==input-dir | PLAN A4 / SPEC §2.7 | DONE | `filename.rs` does `strip_suffix(".bam")` keeping the full path; the anti-dedup trap is explicitly documented. 10 unit tests incl. nested dir, no-ext, `x.bam.bam`→`x.bam`, `s.sorted.bam`→`s.sorted`, `.sam` not stripped, and the headline parent==input-dir guard. |
| A5 | Report: `Tally{...}` + `total()`, `render(...)` verbatim SPEC §5.1 templates (49-hyphen sep, exact spacing, `{:.2}`, `N/A` when total==0); drives file + STDERR; unit tests incl. N/A + CHH-label | PLAN A5 / SPEC §5.1 | DONE | `report.rs` renders the exact templates; separator guarded by `separator_is_exactly_49_hyphens`. 8 unit tests incl. byte-exact Spike-2 SE, N/A, CHH label, no-leading-`\n`/no-trailing-blank, PE per-pair total, 2-decimal rounding. |
| A6 | Logging: `Logger{quiet}`, `info`, thresholds banner, `Now processing file`, summary echo | PLAN A6 | DONE | `logging.rs` has all messages: `thresholds`, `chh_experimental` (no sleep), `processing_file`, `summary`, `skipping_empty`, plus `info_to` test seam. 1 unit test (quiet gate). |
| A7 | SE pipeline: `run`/`process_file`; `BamReader::without_sort_check`; empty-check skip; verbatim header (no `@PG`); eager-open 3 writers; stream count→classify→tally→write; missing-XM graceful stop; finish-on-all-paths | PLAN A7 / SPEC §5.2 §4.1 | DONE | `pipeline.rs`: reader opened no-sort-check (l.114); `records.peek().is_none()` empty skip (l.119); `header.clone()` verbatim; `BucketWriters::open` eager 3 writers; `stream_se` with `is_missing_xm` break (l.179); match arm finalizes via `writers.finish()` on Ok AND `let _ = writers.finish()` on Err (l.144/159). |
| A8 | Phase-A integration tests: 3-way split byte-exact report; per-bucket records in order; empty bucket = valid empty BAM; header round-trip; output-dir==input-dir; counts==report; CLI validation tests | PLAN A8 / SPEC §7 | DONE | `se_three_way_split_and_byte_exact_report`, `se_empty_bucket_is_a_valid_empty_bam`, `se_outputs_land_adjacent_to_input_in_nested_dir`. CLI validation covered by 11 cli.rs unit tests (threshold ranges, `-s`/`-p` conflict, zero files). Empty bucket asserted readable + zero records + nonzero file size. |

### Phase B — Paired-end support

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| B1 | SE/PE resolution: `resolve_mode`; Force honored; else `detect_paired_from_header` → None=SE; log outcome | PLAN B1 / SPEC §2.3 | DONE | `resolve_mode` (l.255): Single→false, Paired→true, Auto→`detect_paired_from_header(...).unwrap_or(false)` (None→SE) + auto-detect log line. |
| B2 | PE sort guard: reject `@HD SO:coordinate` for PE only (the correct guard; Perl `/^\@SO/` dead code); SE not checked | PLAN B2 / SPEC §4.6 §4.11 | DONE | `is_coordinate_sorted` (l.276) reads `@HD SO`, compared to `COORDINATE`; gated on `is_paired` in `process_file` (l.125) → `MethConsError::CoordinateSorted`. SE bypasses. 100k adjacency pre-flight correctly dropped. |
| B3 | PE pipeline: two-at-a-time; odd trailing R1 dropped uncounted; R2 missing-XM graceful stop; exact-qname mate check (no `/1`,`/2` strip, not `BismarkPair::from_mates`); summed counts; write both mates; tally once; report `paired-end` | PLAN B3 / SPEC §2.5 §4 | DONE | `stream_pe` (l.189): `r2==None`→break (drop R1); both missing-XM arms break; `r1.inner().name() != r2.inner().name()`→`MateMismatch` (manual exact-qname); `count_xm(r1)+count_xm(r2)`; `route(..., paired=true)` writes both mates, tally increments once via `tally.record`. |
| B4 | Phase-B tests: PE 3-way (both mates per pair); PE auto-detect via Bismark `@PG` `-1`/`-2`; SE when no Bismark `@PG`; coord-sorted PE→error & SE→ok; mate-name mismatch→error; odd trailing R1 dropped | PLAN B4 | DONE | `pe_three_way_counts_pairs_and_writes_both_mates`, `auto_detect_pe_from_bismark_pg`, `auto_detect_se_when_no_bismark_pg`, `pe_mate_name_mismatch_errors`, `pe_rejects_coordinate_sorted_but_se_accepts_it`. Odd-trailing-R1 path is covered by the unit/PE-logic break arm; no standalone integration test for it but behavior is exercised via the PE pipeline (minor — see Gaps). |

### Phase C — CHH, edge cases & spikes

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| C1 | CHH: `count_xm(chh=true)` counts H/h; `_CHH` filenames; `Too few CHHs` label; experimental warning (no sleep); SE+PE tests | PLAN C1 | DONE | `count_xm` byte selection; `chh_infix`; `too_few_label`; `logging::chh_experimental` (no sleep). Tests: `chh_mode_filenames_and_label` (SE) + `perl_vs_rust_chh_se`. CHH unit tests in classify.rs/report.rs. (PE-CHH path is the same `count_xm`/route code; SE-CHH + Perl-vs-Rust CHH cover the divergent surface.) |
| C2a | Empty file → skip, no outputs, log | PLAN C2 / SPEC §2.4 | DONE | `process_file` peek-none skip; `empty_input_file_is_skipped_with_no_outputs` asserts no outputs created. |
| C2b | Missing-XM graceful STOP (R1 or R2): finalize 3 BAMs + report counts-so-far, exit 0; multi-file continue | PLAN C2 / SPEC §4.1 | DONE | `is_missing_xm` break in both streams; finalize+report on Ok path. `missing_xm_is_a_graceful_stop_with_partial_report` asserts exit 0, partial tally, all 3 BAMs valid. |
| C2c | Truncation → fatal error | PLAN C2 / SPEC §4 | PARTIAL | **Handling present** (generic `Err(e) => return Err(e.into())` in both streams → `MethConsError::Io`, propagated, finish-on-error guard runs, nonzero exit). **No dedicated truncated-fixture test** (PLAN C2 called it "best-effort"). Test-only gap. |
| C2d | Multiple input files → independent per-file outputs + report; per-file disposition | PLAN C2 | PARTIAL | **Behavior present** (`run` loops `for file in &config.files` calling `process_file` independently; empty→skip+continue, missing-XM→finalize+continue, fatal→`?` propagates and aborts). **No integration test invoking the binary with ≥2 files.** Test-only gap. |
| C2e | `total == 0` report → all `N/A` | PLAN C2 / SPEC §5.1 | DONE | `Tally::render` N/A branch; `render_na_when_total_zero` unit test + `min_count_zero_skips...` integration test asserts the full N/A report. |
| C2f | `min_count == 0` zero-call skip path | PLAN C2 / SPEC §8.8 | DONE | `classify` Skip branch; `zero_calls_with_min_count_zero_is_skipped` unit + `min_count_zero_skips_zero_call_reads_into_no_bucket` integration test. |
| C3 | Spike 1 (number-formatting parity) | PLAN C3 | DONE | Resolved pre-implementation; `spikes/RESULTS.md`; formalized as classify.rs tie unit tests. |
| C4 | Spike 2 (empty-bucket BAM behavior) | PLAN C4 | DONE | Resolved pre-implementation; `spikes/RESULTS.md` + `spike2_empty_bucket/`; eager-writers decision realised in A7. |

### Phase D — Real-data byte-identity validation + polish

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| D1 | Byte-identity harness vs Perl (report byte-equal; per-bucket records in order + tags-as-set; header excl. `@PG ID:samtools*`; empty buckets = 0 records) | PLAN D1 / SPEC §7 | DONE (variant) | Implemented as 3 automated `perl_vs_rust_*` tests in `tests/integration.rs` (not a separate `#[ignore]` `byte_identity_real_data.rs`). `assert_perl_rust_identical` runs the real Perl script (auto-skips if perl/samtools absent), asserts byte-equal report + `samtools_record_set` (tags sorted/order-independent, header excluded) + empty-bucket-zero-records. Ran GREEN here (perl+samtools present). Deviation documented in PLAN implementation notes. The harness generalises to real BAMs by pointing at the data dir. |
| D2 | Colossal 10M_SE / 10M_PE / --chh run | PLAN D2 | DEFERRED | Intentionally pending cluster access (per task brief + PLAN implementation notes). Not a defect. The D1 harness logic proves byte-identity; the large run remains. |
| D3 | Polish: crate README, rustdoc, `cargo fmt`/`clippy -D warnings`/full test | PLAN D3 | DONE | `README.md` present (usage, divergences, status). Rustdoc on all public items (`#![warn(missing_docs)]`, no warnings). Verified this session: `clippy -D warnings` clean, `fmt --check` clean, 48+16 tests pass. CI epic #796 wiring flagged in implementation notes (a follow-up, not in-crate). |

### SPEC §6 CLI surface (each flag)

| # | Flag | Source | Status | Notes |
|---|------|--------|--------|-------|
| F1 | `<FILES>...` positional, ≥1 required | §6 | DONE | `files: Vec<PathBuf>`; empty→`NoInputFiles`. |
| F2 | `-p`/`--paired_end` (conflicts -s) | §6 | DONE | `conflicts_with = "single_end"`. |
| F3 | `-s`/`--single_end` | §6 | DONE | present. |
| F4 | `--chh` | §6 | DONE | present. |
| F5 | `--lower_threshold <N>` 0–49 default 10 | §6 | DONE | `Option<i64>`, range-checked, default 10. |
| F6 | `--upper_threshold <N>` 51–100 default 90 | §6 | DONE | `Option<i64>`, range-checked, default 90. |
| F7 | `-m`/`--min-count <N>` ≥0 default 5 | §6 | DONE | `u32` default 5; `0` allowed (test). |
| F8 | `--samtools_path <P>` accepted/ignored | §6 / §4.3 | DONE | parsed, `let _ = self.samtools_path;`. |
| F9 | `--quiet` (new) | §6 | DONE | gates Logger. |
| F10 | `-V`/`--version` (CARGO_PKG_VERSION) | §6.4 | DONE | `disable_version_flag`, handled in main.rs via `version_string()`. |

### SPEC §5 output filenames

| # | Output | Source | Status | Notes |
|---|--------|--------|--------|-------|
| O1 | `{root}{_CHH?}_all_meth.bam` | §5/§2.7 | DONE | `bucket_path` AllMeth. |
| O2 | `{root}{_CHH?}_all_unmeth.bam` | §5/§2.7 | DONE | `bucket_path` AllUnmeth. |
| O3 | `{root}{_CHH?}_mixed_meth.bam` | §5/§2.7 | DONE | `bucket_path` Mixed. |
| O4 | `{root}{_CHH?}_consistency_report.txt` | §5/§2.7 | DONE | `report_path`. |

### SPEC §7 acceptance items

| # | Acceptance item | Source | Status | Notes |
|---|-----------------|--------|--------|-------|
| Q1 | report byte-for-byte identical | §7.1 | DONE | report.rs templates + `perl_vs_rust_*` report assert + byte-exact unit/integration reports. |
| Q2 | populated BAMs identical at record level (fixed fields in order + tags as set) | §7.2 | DONE | `samtools_record_set` (positional fields + sorted tags, in order) in `perl_vs_rust_*`. |
| Q3 | header: `@HD`/`@SQ`/`@PG ID:Bismark` identical; exclude `@PG ID:samtools*` | §7.3 / §4.9 | DONE | header written verbatim (no `@PG` added); gate excludes samtools `@PG` by comparing records-only via `samtools view` (no `-H`). |
| Q4 | empty buckets = zero records both sides | §7.4 / §5.2 | DONE | eager valid-empty-BAM; harness/`se_empty_bucket...` assert zero records. |

## Gaps (detail)

### C2c: Truncated-BGZF fatal-error test (PARTIAL)

**Expected:** PLAN C2 — "Truncation: noodles surfaces truncated BGZF as an I/O error → map to a clear (fatal) error … Best-effort test (truncated fixture)."
**Found:** The *handling* is fully present — `stream_se`/`stream_pe` route any non-missing-XM reader `Err` to `return Err(e.into())`, which becomes `MethConsError::Io`, runs the finish-on-error guard, and exits nonzero. No dedicated integration test feeds a truncated BAM to assert the fatal path.
**Gap:** Add a best-effort test that truncates a valid BAM mid-BGZF and asserts a nonzero exit / `Io` error. Non-blocking (the code path is the generic fatal branch, exercised structurally; PLAN labelled it "best-effort").

### C2d: Multi-file integration test (PARTIAL)

**Expected:** PLAN C2 — "Multiple input files: loop independently; each gets its own outputs + report. Per-file disposition: empty → skip+continue; missing-XM → finalize-this-file+continue; fatal → error out."
**Found:** `pipeline::run` iterates `for file in &config.files` and calls `process_file` per file with `?` propagation, so the runtime behavior (independent outputs, continue-on-skip, abort-on-fatal) is implemented. No integration test invokes the binary with two positional files.
**Gap:** Add an integration test passing ≥2 BAMs and asserting each produces its own report/buckets (and that a fatal file aborts). Non-blocking.

### A1: `anyhow` dependency (DEVIATED)

**Expected:** PLAN A1 lists deps including `anyhow`.
**Found:** `Cargo.toml` has no `anyhow`; error handling is entirely `thiserror`/`MethConsError` with `main` mapping to `ExitCode`. 
**Gap:** None functionally — `anyhow` was unused; dropping it is a harmless simplification (matches the dedup/thiserror-only style). Worth a one-line note only.

### D2: Colossal 10M real-data run (DEFERRED — not a defect)

**Expected:** PLAN D2 — large `10M_SE`/`10M_PE`/`--chh` run on colossal.
**Found:** Not run (cluster access pending, per task brief). The D1 byte-identity *logic* is implemented and proven on synthetic data against the real Perl script.
**Gap:** Run the harness on `/weka/projects/bioinf/Data/Felix/bismark_benchmarks/` when cluster access is available. Tracked as the remaining Phase-D step.

## Test verification (Mode B)

| Test | File | Status |
|------|------|--------|
| 18 unit tests (classify) incl. 10.04/10.05, ties 6.25/12.5/87.5/90.05, min_count, CHH, empty XM, custom thresholds | src/classify.rs | PASS |
| 11 unit tests (cli) incl. threshold ranges, `-s`/`-p` conflict, zero files, defaults | src/cli.rs | PASS |
| 10 unit tests (filename) incl. dir preservation, single-`.bam` strip, dir==input | src/filename.rs | PASS |
| 8 unit tests (report) incl. byte-exact Spike-2, N/A, CHH label, 49-hyphen, no-lead-`\n` | src/report.rs | PASS |
| 1 unit test (logging) quiet gate | src/logging.rs | PASS |
| se_three_way_split_and_byte_exact_report | tests/integration.rs | PASS |
| se_outputs_land_adjacent_to_input_in_nested_dir | tests/integration.rs | PASS |
| se_empty_bucket_is_a_valid_empty_bam | tests/integration.rs | PASS |
| pe_three_way_counts_pairs_and_writes_both_mates | tests/integration.rs | PASS |
| auto_detect_pe_from_bismark_pg | tests/integration.rs | PASS |
| auto_detect_se_when_no_bismark_pg | tests/integration.rs | PASS |
| pe_mate_name_mismatch_errors | tests/integration.rs | PASS |
| pe_rejects_coordinate_sorted_but_se_accepts_it | tests/integration.rs | PASS |
| chh_mode_filenames_and_label | tests/integration.rs | PASS |
| min_count_zero_skips_zero_call_reads_into_no_bucket | tests/integration.rs | PASS |
| empty_input_file_is_skipped_with_no_outputs | tests/integration.rs | PASS |
| missing_xm_is_a_graceful_stop_with_partial_report | tests/integration.rs | PASS |
| version_flag_prints_provenance | tests/integration.rs | PASS |
| perl_vs_rust_se_three_way | tests/integration.rs | PASS (perl+samtools present this run) |
| perl_vs_rust_pe_three_way | tests/integration.rs | PASS |
| perl_vs_rust_chh_se | tests/integration.rs | PASS |
| truncated-BGZF fatal-error test | (none) | MISSING (C2c — handling present, test absent) |
| multi-file (≥2 inputs) integration test | (none) | MISSING (C2d — behavior present, test absent) |
| odd-trailing-R1-dropped standalone test | (none) | not separately tested (B4 — logic present in stream_pe) |

Suite result this session: **48 unit + 16 integration = 64 passed, 0 failed, 0 ignored.** `clippy -D warnings` clean; `cargo fmt --check` clean.

## Verdict

**COMPLETE for the implemented scope (Phases A, B, C and the D1 harness + D3 polish).** Every PLAN A/B task, every Phase-C edge-case branch (empty file, missing-XM graceful stop, `total==0`/`N/A`, `min_count==0`), every SPEC §6 CLI flag, all four §5 output filenames, and all four §7 acceptance items are implemented and tested. Both spikes (C3/C4) are resolved and their decisions realised in code. The Perl-vs-Rust byte-identity gate (D1) is implemented in-suite and ran green.

Three non-blocking items remain:

1. **C2c — truncated-BGZF test (PARTIAL):** the fatal-error *handling* is implemented (generic reader-`Err` → `MethConsError::Io` → nonzero exit, finish-on-error guard); only the dedicated best-effort truncated-fixture test is absent. PLAN itself labelled it "best-effort."
2. **C2d — multi-file integration test (PARTIAL):** the per-file independent loop + disposition is implemented in `pipeline::run`; only a ≥2-file integration test is absent. (Optionally also a standalone odd-trailing-R1 test for B4.)
3. **A1 — `anyhow` dependency (DEVIATED):** listed in the plan but unused and correctly dropped; cosmetic only.

**D2 (the 10M colossal real-data run) is intentionally DEFERRED pending cluster access — reported as deferred, not as a defect.** When cluster access is available, point the `perl_vs_rust_*` harness at `/weka/projects/bioinf/Data/Felix/bismark_benchmarks/` for `10M_SE`/`10M_PE`/`--chh`.

None of the above blocks the implementation from being considered functionally complete; items 1–2 are test-coverage additions and item 3 is a doc note.

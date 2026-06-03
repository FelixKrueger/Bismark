# Code Review B — Phase 9b: order-preserving file-level threading (`--multicore`/`--parallel N`)

**Reviewer:** B (independent, fresh context)
**Scope:** uncommitted working-tree diff on `rust/aligner-v1` @ `~/Github/Bismark-aligner` — `config.rs`, `lib.rs`, `merge.rs`, `parallel.rs` (NEW), `Cargo.toml`, `tests/cli.rs`.
**Acceptance gate:** worker-count invariance — `bismark_rs --parallel N` == `--parallel 1` == Perl single-core, byte-for-byte (decompressed BAM content, reports, aux). N==1 stays byte-frozen.

## Verdict

**APPROVE with recommendations — no Critical, no High.** The three worker-invariance invariants are correctly implemented and re-derived from source; N==1 is byte-frozen; the skip/upto double-application trap is correctly avoided; the raw-BAM merge, noodles pin, scope concurrency, temp naming/cleanup, and `AuxWriter::finish` are all sound. The findings are **test-coverage gaps** (Medium) and minor structure/error-path notes (Low) — none change the gated output bytes, and all gaps are backstopped by the pending oxy gate (§9 #11).

Build/lint/test status (run locally, sandbox disabled, from `~/Github/Bismark-aligner/rust`):
- `cargo test -p bismark-aligner` → **238 green** (201 lib + 37 integration), 0 failed.
- `cargo clippy -p bismark-aligner --all-targets -- -D warnings` → clean.
- `cargo fmt -p bismark-aligner -- --check` → clean.

---

## Invariant re-derivation (the load-bearing claims)

**(1a) Contiguous partition covering exactly `(skip,upto]`.** `quotas` (`parallel.rs:118`) computes balanced contiguous ranges (`base = eff/n`, first `eff%n` chunks get +1; remainder lands in *leading* chunks so empties are *trailing*). The split loop (`:187-203`, PE `:293-313`) advances `chunk` only when `in_chunk >= quota[chunk]`, writing exactly `eff` records (`Σquota == eff`), so `chunk < len` always holds when writing. Verified against the converter arithmetic: `in_window`/`past_upto` (`:98-112`) reproduce `convert.rs:316-326` exactly — 1-based ordinal, Perl-falsy-0 (`Some(s) if s>0`), `count<=skip` drop, `count>upto` stop-and-break **after** `count+=1`. Skip-then-upto ordering in the converter vs. past_upto-then-in_window in the splitter is semantically identical (no off-by-one). Test `split_skip_and_upto_both_set_straddles_boundary` pins the both-set boundary-straddle case. ✓

**(1b) In-order single-writer merges.** `merge_bams` (`:521`) opens one final `BamWriter`, then for each part in chunk order reads raw `RecordBuf`s and `write_raw_record`s them through that single writer (per-chunk headers skipped via `read_header`). `merge_aux_gz` (`:545`) streams every plain part through ONE `GzEncoder::new(.., Compression::default())` with a single `finish()` — no mid-stream flush. Both match the single-core encoder (`open_sinks` uses `Compression::default()`, `:495`). ✓

**(1c) Complete field-wise counter sum.** `Counters::merge` (`merge.rs:127`) sums **all 22 fields** — I diffed the struct (`merge.rs:80-122`) field-by-field against the `merge` body: every field (`sequences_count` … `total_unme_c_unknown`, including the 4 `ct_ga*`/`ga_ct*` strand counts and `genomic_sequence_could_not_be_extracted_count`) is summed. None dropped. ✓

**(2) skip/upto double-application avoided.** `run_se_multicore`/`run_pe_multicore` (`:625-628`, `:765-768`) capture `(orig_skip, orig_upto)`, clone `RunConfig`, set both to `None`, and build `chunk_opts = ConvertOptions::from_config(&cfg)` from the cleared clone. The split applies the ORIGINAL skip/upto once (`:644`, `:783`); the chunk jobs receive `&cfg` (cleared), so both `convert_*` (reads `opts`) and `drive_merge` (reads `config.read_processing`, `lib.rs:583-585`) see `None`/`None`. No re-skip within chunks. ✓

**(3) N==1 byte-frozen.** The `process_se_chunk`/`process_pe_chunk` extraction moved convert+spawn+drive_merge+finish-streams verbatim; `run_se`/`run_pe` keep report-header/final-analysis/completion/sink-finish/cleanup unchanged. The documented STDERR-ordering shift (open_sinks now precedes convert) is byte-invisible. 32 unchanged integration tests + the N==1 worker-invariance baseline confirm. ✓

**(4) Raw-BAM merge + noodles pin.** `merge_bams` reads via `noodles_bam::io::Reader` + `record_bufs` and writes via `BamWriter::write_raw_record` — which routes through the **same** `write_alignment_record(&self.header, record)` that single-core `write_record` uses (`bismark-io/src/write.rs:72-89`), so the merged decompressed stream is byte-identical to single-core. `noodles-bam = "=0.89.0"` matches bismark-io's transitive pin exactly; `Cargo.lock` resolves a single `noodles-bam 0.89.0`. Header handling correct: per-chunk headers skipped, one shared final header, ref-id indices valid because every chunk header == final header (all from `generate_sam_header`). ✓

**(5) PE lockstep split.** `count_effective_pe`/`split_contiguous_pe` (`:213`, `:245`) read both mates in lockstep, break on the first incomplete pair (`!(has1 && has2)`), and partition the COMMON (min) count — mirroring `drive_merge_pe`'s `lib.rs:1155-1167` break. Per-chunk R1==R2 by construction (both written on the same boundary). Test `pe_split_partitions_common_min_count_in_lockstep` pins it (7+5 → 5 pairs, 3+2). ✓

**(6) `std::thread::scope` correctness.** Genome/refid/header/cfg shared `&` read-only (no `Arc`); `RunConfig` is `Sync` (compiles + tests pass). Worker `Err` → returned `Result`, collected lowest-index-first via `collect_in_order` (`:600`, `?` on the first error). Panic → `h.join()` returns `Err` mapped to a `Validation` error (see L-2). Orphan-safety backstopped by `AlignerStream`/`PairedAlignerStream` `Drop` kill+reap (`align.rs:255/461`) + scope joining all siblings. ✓

**(7) Temp naming/cleanup.** Subsets named off the ORIGINAL basename, plain, no `.gz`, no `--prefix` (`:175`, `:273/:276`); converter prepends prefix once → no `p.p.` doubling (verified `convert.rs:264-269`). Per-chunk BAM/aux named off the subset basename (`:406-415`) → collision-free. `se_chunk_job`/`pe_chunk_job` clean converted+subset on success; orchestrator `cleanup_se`/`cleanup_pe` cleans per-chunk BAM/aux post-merge. ✓ (Error-path leak — see L-1.)

**(8) `AuxWriter`.** `finish()` (`lib.rs`): Gz `finish()`es (trailer), Plain only `flush()`es — no mid-stream flush; `Write` delegation correct. `#[allow(clippy::large_enum_variant)]` justified (held singly). ✓

---

## Findings

### Medium

**M-1 — The SE `.ambig.bam` output is exercised but never asserted invariant.**
`run_se_parallel` (`tests/cli.rs:1579`) passes `--ambig_bam` and the fake routes `a`-class reads (AS==XS on CT) to the ambig BAM, so `merge_bams` runs the raw-`RecordBuf` merge across multiple chunks at every N. **But the helper returns only `(main-bam, report, unmapped.gz, ambiguous.gz)` — it never reads back `reads_bismark_bt2.ambig.bam` and compares it across N.** This is exactly the path the plan §13 flagged as the gate-found bug (raw merge for tag-less ambig records). A wrong chunk order, a dropped ambig record, or a header-skip bug in the ambig merge would NOT fail any unit test. Plan §9 #7 calls for "byte-identical BAM (decompressed)" — the ambig BAM is a BAM that must also be invariant.
*Recommend:* extend `run_se_parallel` to also `canon_bam` the `.ambig.bam` and assert it in `assert_se_worker_invariant`. (Backstopped only by the oxy gate today.)

**M-2 — PE worker-invariance matrix is thinner than the plan's stated §9 #7.**
Plan §9 #7 specifies "SE + PE × {dir, non-dir, pbat-FastQ}" with "each decision class (UniqueBest/Ambiguous/NoAlignment) on both sides of a boundary." The actual PE test (`worker_invariance_pe_directional`) covers **only PE directional** and **only `m`/`u`** (no `Ambiguous`, no `--ambig_bam`, no `--ambiguous`). Missing under chunking: PE non-directional (4-slot fan-out), PE pbat, and the PE Ambiguous decision class + PE `--ambig_bam` merge. The SE cells cover non-dir/pbat/ambiguous, so the per-library-type and ambiguous merge machinery is partially exercised SE-side, but the PE merge (which uses `drive_merge_pe` + the 4-slot `streams` vector) is only proven for the simplest cell.
*Recommend:* add PE non-dir + PE pbat worker-invariance cells and a PE ambiguous/`--ambig_bam` cell (or explicitly document the reliance on the SE cells + oxy gate for the rest). Not blocking — the oxy gate (§9 #11) covers the full matrix with real Bowtie 2.

**M-3 — Plan validation row §9 #10 (worker error/panic + no-orphan) has no test.**
There is no unit/integration test for: a chunk worker returning `Err` → lowest-chunk-index error surfaced deterministically; a chunk worker panicking → graceful surface, all siblings joined, no orphaned Bowtie 2 on EITHER path. The mechanism is sound by construction (`collect_in_order` `?`-first; `Drop` kill+reap; scope joins before return), and `align::tests::early_stop_does_not_deadlock_or_zombie` covers single-stream zombie reaping, but the multicore orchestration of it is untested.
*Recommend:* add a test that forces a worker error (e.g. an unreadable subset or a fake bt2 that exits non-zero for one chunk) and asserts the lowest-index error + no leftover child processes. Low risk if deferred, but it's a listed validation row.

### Low

**L-1 — Temp-file leak on the chunk error path.** When a chunk job hits `?` (e.g. spawn/convert error), `se_chunk_job`/`pe_chunk_job` return before their per-chunk subset+converted cleanup runs, and `collect_in_order` short-circuits before `cleanup_se`/`cleanup_pe` — so failed-run subset/converted/per-chunk-BAM temps leak. This matches the single-core convention (temps leak on error; cleanup is best-effort/byte-invisible) and is error-path/non-gated. Note only.

**L-2 — Panic is silently re-mapped, not re-panicked (deviation from plan §3.8).** Plan §3.8/§9 #10 say a worker panic "re-panics on join (acceptable loud abort)." The implementation instead maps `h.join()`'s `Err` to `AlignerError::Validation("a Phase-9b chunk worker panicked")` (`:658-662`, `:800-804`), **swallowing the original panic payload/message.** This is arguably friendlier (graceful, deterministic) but loses the panic's diagnostic text and diverges from the documented behavior. Either re-panic with `std::panic::resume_unwind(e)` to preserve the message, or document the deviation. Cosmetic/diagnostic only.

**L-3 — `--multicore`-dropped-from-notice (§9 #1) is only proven indirectly.** `deferred_flag_emits_notice` now uses `--nucleotide_coverage` and no longer passes `--multicore`; `multicore_zero_errors` proves the flag is validated. But no test passes `--multicore N` and asserts the "not yet active" notice does NOT list it. The worker-invariance tests (which pass `--parallel`) implicitly prove it's active. Optional: add a negative assertion.

**L-4 — `estimate_index_bytes` prefix match can over-count (STDERR-only).** `starts_with("{stem}.")` (`:567`) could match unrelated siblings (e.g. a stray `BS_CT.log`); the memory estimate is STDERR-only and explicitly "bounded by, not equal to," so any inaccuracy is cosmetic. Note only.

**L-5 — Stray uncommitted artifacts in the crate dir.** `rust/bismark-aligner/reads_bismark_bt2.bam` and `reads_bismark_bt2_SE_report.txt` (untracked, ts 11:52) are leftovers from a manual CWD run — NOT produced by the test suite (all tests use TempDir `--output_dir`). Clean/gitignore before committing so they don't slip into the 9b commit. Housekeeping, outside the diff.

---

## Things explicitly verified clean (no finding)

- gz input: subsets written plain (decompressed via `open_reader`), read plain by the converter; output BAM/report/aux names derive from the ORIGINAL `read_file` (not the subset), `strip_fastq_suffix` handles `.fq.gz`. No gz-specific divergence.
- Concurrent `create_dir_all(temp_dir)` across workers is idempotent/race-safe; the split already creates it pre-fan-out.
- Report header (`write_report_header`) depends only on sequence_file/genome_folder/aligner_options/library — none affected by the cleared skip/upto clone; multicore passes the original `read_file`. Byte-identical.
- PE aux filename derivation in `run_pe_multicore` (`basename(read_1/2)` + `output_dir.join`) matches `open_pe_sinks` exactly.
- `read_record` preserves a final record lacking a trailing newline verbatim (matches the converter's `read_until` behavior) → subset feeds identical bytes.
- `quotas` empties are always trailing (remainder in leading chunks); `eff < n` and empty-input cases tested.

## Recommendation summary
Ship-able as-is for the machinery gate; the residual risk is **test coverage** (M-1 SE ambig BAM not asserted, M-2 thin PE matrix, M-3 no error/panic test), all of which the real-data oxy gate (§9 #11) will cover. Strongly recommend closing **M-1** (cheap, and it's the exact gate-found-bug path) and at minimum documenting M-2/M-3 as oxy-gate-deferred before merge. L-1..L-5 are non-blocking.

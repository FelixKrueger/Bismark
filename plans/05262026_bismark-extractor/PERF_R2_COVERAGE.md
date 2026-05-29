# Plan Coverage Report

**Mode:** B (code vs. the plan's implementation spec — the "## R2 FINAL approach — gzp-in-collector (ALT-1)" section)
**Plan:** `plans/05262026_bismark-extractor/PERF_R2_WORKER_OUTPUT_PLAN.md`
**Date:** 2026-05-29
**Verdict:** COMPLETE

> Scope note: This audit deliberately covers ONLY the "R2 FINAL approach —
> gzp-in-collector (ALT-1)" section ("What was implemented" + "Single-member
> framing" + "Verification status"). The "## Design (rescoped) — SUPERSEDED"
> and "## Implementation outline" (worker-side-members) sections were explicitly
> NOT shipped and are out of scope per the verification request.
>
> Code state: worktree `/Users/fkrueger/Github/Bismark-extractor`, branch
> `spike-gzp` @ `65e5ff1`; net R2 diff = `git diff 8a2a147 -- rust/`.

## Summary

- Total items: 7
- DONE: 7
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 0
- Known-pending (non-code): 1 (Colossal `--gzip` SE/PE `phase_h_smoke` — recorded, not a gap)

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | `output.rs::open_writer` swaps flate2 `GzEncoder` → `gzp::par::compress::ParCompress<Gzip>` (`deflate_rust`); writer stays single `Box<dyn Write + Send>`; collector/ordering/empty-sweep/header/plain-`.txt` paths unchanged | "What was implemented" #1 | DONE | `open_writer` (output.rs:407-431) now boxes `ParCompressBuilder::<gzp::deflate::Gzip>::new().num_threads(...).from_writer(file)`. `BoxedWriter = BufWriter<Box<dyn Write + Send>>` unchanged. Plain branch (`else { Box::new(file) }`) intact. Functional diff in output.rs is exactly the import drop + const + this swap; everything else is doc-comment-only — `write_call`, `write_routed_call`, empty-sweep, version header, `batch_seq` ordering untouched. `parallel.rs` has ZERO lines in the R2 diff. |
| 2 | `GZIP_COMPRESS_THREADS = 4` named const, decoupled from `--parallel` | "What was implemented" #2 | DONE | `const GZIP_COMPRESS_THREADS: usize = 4;` (output.rs:391) with doc explaining decoupling from `--parallel`; passed to `.num_threads(GZIP_COMPRESS_THREADS)`. No reference to `--parallel`/worker count in its value. |
| 3 | Cargo: `gzp = "=0.11.3"` (default-features off, `deflate_rust`); `flate2` → `[dev-dependencies]`; dead `flate2::write::GzEncoder`/`Compression` imports dropped | "What was implemented" #3 | DONE | `gzp = { version = "=0.11.3", default-features = false, features = ["deflate_rust"] }` under `[dependencies]` (line 37). `flate2 = "=1.1.9"` moved under `[dev-dependencies]` (line 66). Both `use flate2::Compression;` and `use flate2::write::GzEncoder;` removed from output.rs imports. |
| 4 | New test `parallel_gzip_multibatch_decompresses_identical_across_n_and_to_plain` (8199 records > 2×BATCH_SIZE) in `tests/parallel_phase_f.rs` | "What was implemented" #4 | DONE | Added at parallel_phase_f.rs:816. Uses `write_se_large_bam(&bam_path, 8199)` (> 2×4096). Asserts: gz decode == plain peer; cross-N (1 vs 4) decompressed-byte identity; plus non-gz file/report equivalence. |
| 5 | Single-member framing: existing `GzDecoder` tests retained, NOT migrated to `MultiGzDecoder` | "Single-member framing" | DONE | `use flate2::read::GzDecoder;` (test line 30) retained; `decompress_gz` (line 346) uses single-member `GzDecoder`. Pre-existing `parallel_gzip_n4_decompresses_identical_to_legacy_plain` (line 741) retained, still single-member. `MultiGzDecoder` appears nowhere in code — only in one output.rs doc comment (line 408) stating it is NOT needed. |
| 6 | Single-member framing: no worker-side `WorkerOutput`/`records_written` changes | "Single-member framing" | DONE | `parallel.rs` is entirely absent from the R2 diff stat. The collector path, `WorkerOutput` payload, and `records_written` accounting are untouched (confirms C6a/C4 N/A claim). |
| 7 | Local verification (`cargo fmt --check`, `clippy -D warnings`, `cargo test -p bismark-extractor` 102+ tests) all PASS | "Verification status" | DONE | `cargo test -p bismark-extractor` rerun 2026-05-29: integration crate 102 passed; both gzip tests (`..._n4...`, `..._multibatch...`) pass; all binaries 0 failed (320 tests total across binaries). |

## Gaps (detail)

None. All 7 ledger items are DONE as specified.

## Known-pending (non-code, recorded — NOT a gap)

### Colossal `--gzip` SE/PE `phase_h_smoke` (real-data byte-identity)

The plan's "Verification status" subsection explicitly lists this as **PENDING**.
This is a real-data validation step on the colossal cluster, not a code
deliverable. Per the verification request it is recorded here as a known-pending
verification item, not a code gap. It does not affect the COMPLETE verdict for
the R2 code scope.

## Test verification (Mode B)

| Test name | File | Status |
|-----------|------|--------|
| parallel_gzip_multibatch_decompresses_identical_across_n_and_to_plain | tests/parallel_phase_f.rs | PASS |
| parallel_gzip_n4_decompresses_identical_to_legacy_plain (retained) | tests/parallel_phase_f.rs | PASS |
| parallel_se_byte_identical_across_batch_boundary (pre-existing, plain) | tests/parallel_phase_f.rs | PASS |
| Full `bismark-extractor` suite (integration crate 102 + all binaries) | rust/bismark-extractor | PASS (0 failed) |
| Colossal `--gzip` SE/PE phase_h_smoke | (real-data, colossal) | PENDING (non-code; per plan) |

## Verdict

**COMPLETE.** Every item in the plan's "R2 FINAL approach — gzp-in-collector
(ALT-1)" implementation spec ("What was implemented" #1–#4 + the two
"Single-member framing" consequences) is present in the code exactly as
specified, and the full local test suite passes. No code gaps. The only
outstanding item is the colossal `--gzip` SE/PE `phase_h_smoke` real-data
byte-identity run, which the plan itself marks PENDING and is a deployment-time
verification, not a code deliverable.

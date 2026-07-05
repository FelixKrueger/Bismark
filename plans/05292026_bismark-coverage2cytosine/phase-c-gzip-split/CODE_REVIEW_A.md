# Code Review A — Phase C (`--gzip` + `--split_by_chromosome`)

**Reviewer:** Code Reviewer A (fresh context, no shared state).
**Scope:** `rust/bismark-coverage2cytosine/src/report.rs`, `src/cli.rs`, `Cargo.toml`, `tests/golden_phase_c.rs`, `tests/data/phase_b/phase_c/` goldens + `generate_goldens.sh`.
**Worktree:** `/Users/fkrueger/Github/Bismark-c2c` (branch `rust/coverage2cytosine`; Phase C uncommitted on top of committed Phase B `778669e`).
**Contract:** byte-identical to Perl v0.25.1.

## Verdict: APPROVE

Phase C is **correct and byte-identical to live Perl v0.25.1** across every mode I tested (cross-checked by running the repo Perl directly): `--gzip`, `--CX --gzip`, `--split_by_chromosome`, `--split --gzip`, suffixed-`-o` split, threshold>0 split, the re-appearance truncate quirk, single-chromosome split, and empty-input error. **81 tests pass** (58 unit + 11 Phase-B golden + 7 Phase-C golden + 5 sanity); `clippy --all-targets -D warnings` clean; `cargo fmt --check` clean; workspace builds; siblings untouched. The Phase-B kernel/walk/ordering/summary are provably unchanged.

**No Critical or High issues.** Findings are all Low (documentation accuracy + minor test-coverage completeness). None block merge.

---

## Focus-area findings

### 1. Phase B preserved (non-split path) — CONFIRMED
`git diff 778669e -- src/report.rs` shows the Phase-B `flush_chromosome(…, w: &mut dyn Write)` was renamed to `chromosome_report_bytes(…) -> Vec<u8>` with the kernel call (`emit_position`) and the `for i in 0..seq.len()` walk **byte-for-byte unchanged** (0 kernel-internal lines removed). `run_single` is line-equivalent to the shipped Phase-B `run_report`: same streaming loop, same transition flush, same final-chr flush, same `threshold == 0` uncovered pass over `names_sorted()`, same `ContextSummary` accumulation. The only deltas are the sink type (`BufWriter` → `ReportWriter`), `flush()` → `finish()`, and the new filename helpers. `emit_position`/`extract`/`perl_substr`/`revcomp`/`classify_context` are untouched. **All 11 Phase-B goldens pass.**

### 2. `--gzip` — CONFIRMED against live Perl
Ran Perl `--gzip` and Rust `--gzip`; gunzipped both reports → **byte-identical**. File set identical (`gz.CpG_report.txt.gz` + `gz.cytosine_context_summary.txt`). Summary is plain ASCII (`xxd` → `7570` = "up", not `1f8b`) and byte-identical to Perl's plain summary. `finish()` is explicit (no Drop reliance) — `ReportWriter::finish(self)` consumes the encoder and calls `GzEncoder::finish()` (Plain → `flush()`). No newline/trailing-byte drift. Empty-gzip stream for zero-emit chrs verified: `scaf_short.gz` is a valid 20-byte `1f8b…` stream decompressing to 0 bytes (matches Perl's 20-byte output).

### 3. Split filename C1 (raw `-o`) — CONFIRMED against live Perl
`report_name` builds the split base as `format!("{output_raw}.chr{name}")` with **no** suffix strip — faithfully reproducing Perl `handle_filehandles:101` appending `.chr${my_chr}` to the raw `$cytosine_out` *before* the `:107-112` strip (which then no-ops). Live Perl cross-check with `-o foo.CpG_report.txt --split` produced the doubled suffix `foo.CpG_report.txt.chrchr1.CpG_report.txt`, and the whole output directory **recursively diffed IDENTICAL** to Rust (incl. all 4 chrs + summary-in-`scaf_short`). Bare `-o split` → `split.chrchr1.CpG_report.txt`. Both confirmed.

### 4. Split truncate-on-reopen (B-C1) — CONFIRMED against live Perl
`flush_split_chromosome` opens a fresh `ReportWriter::create` (= `File::create`, truncating) per chr with **no caching**. Live Perl cross-check on cov `chrA,chrB,chrA` over genome `chrA/chrB`: Rust and Perl directories **recursively diffed IDENTICAL**. chrA's report shows pos2 as `0 0` (first segment's `5/0` lost on truncating reopen) and the full summary (1307 bytes) lands in chrA (last reopened), chrB summary 0 bytes. Mirrors Perl `:457-466` (`close CYT; handle_filehandles(...)` per transition).

### 5. Split summary quirk + threshold>0 — CONFIRMED against live Perl
- threshold==0 split: N empty summary files + the last (`scaf_short`, bytewise-sorted last uncovered) holding the full 1310-byte summary == the non-split summary. Recursive diff vs Perl IDENTICAL.
- threshold>0 split (`--coverage_threshold 5`): only covered chrs (chr1, chr2) get files; uncovered (scaf_short, chr3uncov) get **NO files** (no report, no summary); the full summary lands in chr2 (last covered, no uncovered pass). Recursive diff vs Perl IDENTICAL. Matches Perl `:714` (threshold>0 skips the uncovered foreach).

### 6. `run_split` summary-routing logic — CONFIRMED correct
`last_summary_path: PathBuf` is seeded from the final-chr match (always ≥1 chr or `EmptyCoverageInput`), then overwritten by each uncovered chr (sorted) when threshold==0. Last assignment wins → final cur_chr when no uncovered, last bytewise uncovered when present. This exactly mirrors Perl: `print_context_summary()` (line 49) runs once at the very end, writing to whichever `CONTEXTSUMMARY` filehandle was last reopened — and `process_unprocessed_chromosomes:1396` reopens it per uncovered chr. Edge cases verified: single covered chr (summary lands in it — live-Perl IDENTICAL); empty input errors before the uncovered pass (Rust exit 1 + clear message, Perl exit 255, neither writes files); all-uncovered is impossible (empty cov → error).

### 7. Golden adequacy — ADEQUATE (one minor gap, Low)
- `split_dir_matches_perl_golden` does a **bidirectional** file-set compare (`BTreeSet` equality both ways) **and** per-file byte compare against the committed Perl golden dir — strong; catches spurious/missing files and any byte drift incl. the empty-vs-full summary quirk.
- Regression discrimination verified by reasoning + the Perl cross-checks: a stem-instead-of-raw split-filename regression fails `suffixed_output_split…` (asserts the doubled-suffix name exists); a cached-writer/append bug fails `split_reappearance…` (`!contains "5/0"`); a gzipped-summary bug fails `gzip_report…` (`!= 1f8b` + byte compare) and the split-gzip plain-summary compare.
- `generate_goldens.sh` is **fully reproducible** (re-ran it; phase_c + phase_b goldens regenerate identically) and leaves **no stray files** (Phase-C runs write into `phase_c/{split,split_thr}` via `--dir`).
- **Minor gap (Low):** `gzip_cx_report_decompresses_to_plain_golden` checks only the CX report, not that the CX-mode summary is plain/correct (the non-CX gzip test does check the summary). Harmless — `summary_path` ignores CX for the `.gz` decision — but the V4/V5 "summary plain" assertion isn't exercised under `--CX`.

---

## Issues by area

### Logic / correctness
None. The split/non-split dispatch, truncate-on-reopen, summary routing, and empty-input guard all match live Perl byte-for-byte.

### Errors / edge cases
None blocking. Empty input → error in both paths before any file is written (verified vs Perl). Zero-emitting chrs get their (empty / empty-gzip) report + (empty) summary file (verified).

### Structure / docs (Low only)
- **L1 — IMPL/PLAN/focus mislabel `flate2` as a "dev-dep" + claim a Cargo.toml change.** `git diff 778669e -- Cargo.toml` is **empty**: flate2 is (correctly) a **regular** `[dependencies]` entry (line 28, present since Phase A for `.fa.gz` genome reading) because `src/report.rs` uses it in production code; the test dev-deps (`assert_cmd`, `tempfile`) were already present. No Cargo.toml change was made. Documentation-only inaccuracy.
- **L2 — SPEC §5 split row not yet synced.** SPEC shows `{stem}.CpG_report.txt.chr<NAME>` (stripped stem, `.chr` after the suffix) — the real Perl uses raw `-o` with `.chr` *before* the no-op strip, hence the doubling. The PLAN §11 already flags this as an intentional, documented deviation ("will sync the SPEC wording at next rev"); the **implementation follows the verified Perl behavior**, which is authoritative. Tracking only.

---

## Prioritized recommendations

### Critical / High
None.

### Medium
None.

### Low (optional, non-blocking)

**L-A — Add a summary assertion to the `--CX --gzip` test (close the V4/V5 gap under CX).**
In `tests/golden_phase_c.rs::gzip_cx_report_decompresses_to_plain_golden`, after the report compare:
```rust
    // Summary stays plain even under --CX --gzip.
    let sum = tmp.path().join("cx.cytosine_context_summary.txt");
    let sum_bytes = std::fs::read(&sum).unwrap();
    assert!(
        !(sum_bytes.len() >= 2 && sum_bytes[0] == 0x1f && sum_bytes[1] == 0x8b),
        "CX summary must NOT be gzipped"
    );
    assert_eq!(
        sum_bytes,
        std::fs::read(d.join("cx.summary.golden")).unwrap()
    );
```
(`cx.summary.golden` already exists in `tests/data/phase_b/`.)

**L-B — Correct the docs.** In `phase-c-gzip-split/PLAN.md` (Implementation notes) and `IMPL.md` (Plan-coverage row / commit-plan staging), drop "`Cargo.toml` `flate2` dev-dep" — flate2 is a pre-existing **regular** dependency and Cargo.toml was not modified. No code change.

---

## Build / verify evidence
- `cargo test -p bismark-coverage2cytosine` → 58 + 11 + 7 + 5 = **81 pass, 0 fail**.
- `cargo clippy -p bismark-coverage2cytosine --all-targets -- -D warnings` → clean.
- `cargo fmt -p bismark-coverage2cytosine --check` → clean.
- Live Perl recursive-diff IDENTICAL for: `--gzip` (gunzip), `--CX --gzip` (gunzip), `--split`, `--split --gzip` (per-`.gz` gunzip; gzip *containers* differ in size — 104 vs 101 etc. — as the decompressed-only contract intends), suffixed-`-o` split, threshold>0 split, re-appearance (chrA,chrB,chrA), single-chr split, empty-input error.
- `generate_goldens.sh` re-run → phase_b + phase_c goldens reproduce identically, no stray files.

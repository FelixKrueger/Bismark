# IMPL — Phase C (TDD task list)

**Source plan:** `phase-c-gzip-split/PLAN.md` (rev 1). **Goal:** `--gzip` + `--split_by_chromosome` output shaping, **byte-identical to Perl v0.25.1**, without touching the Phase-B kernel/walk.

**Mode:** TDD. **Worktree:** `/Users/fkrueger/Github/Bismark-c2c`; crate `rust/bismark-coverage2cytosine/`.
**Command base:** cargo from `/Users/fkrueger/Github/Bismark-c2c/rust`; perl from the repo root. **All cargo/perl commands need `dangerouslyDisableSandbox: true`** (worktree outside the sandbox root). Do NOT touch `rust/bismark-extractor` or `rust/bismark-bedgraph`.

## Test infrastructure
- Unit: inline `#[cfg(test)]` in `report.rs` (ReportWriter, filename derivation) + `cli.rs` (`output_raw`).
- Integration: `tests/golden_phase_c.rs` (assert_cmd; `flate2::read::MultiGzDecoder` for gz decompress).
- Goldens: extend `tests/data/phase_b/generate_goldens.sh` with a `phase_c` section (run the repo Perl v0.25.1 for the new modes); fixtures reuse `tests/data/phase_b/genome` + `in.cov`, plus a suffixed-`-o` run and a threshold>0 run. Commit fixtures + goldens.

## Plan coverage checklist

| # | Plan item | Source | Task(s) |
|---|-----------|--------|---------|
| 1 | `ResolvedConfig.output_raw` (verbatim `-o`) | §3.5 | T1 |
| 2 | `ReportWriter` {Plain,Gz} + create/write_all/finish (explicit) | §4 | T2 |
| 3 | `base(config,chr)`: split=raw+`.chr`+name (no strip), non-split=stem | §3.2/§4 | T3 |
| 4 | report_path = base+suffix+(gzip?`.gz`); summary_path = base+`.cytosine_context_summary.txt` (never gz) | §3.1/§3.2/§4 | T3 |
| 5 | suffixed-`-o` split doubling; bare-`-o` split; non-split strip | §3.2 (C1) | T3, T7 |
| 6 | `--gzip` non-split: report gz, summary plain, finish before summary | §3.1 | T5 |
| 7 | `--gzip` byte-identity = decompressed == plain golden | §3.1/§9 V3/V5 | T5 |
| 8 | `--split`: per-chr truncating `File::create` every transition, NO caching | §3.2 (C1-B) | T6 |
| 9 | split re-appearance keeps last segment; summary → last reopened chr | §3.2 (C1-B) | T6 (V12) |
| 10 | zero-emitting chr still gets file (0-byte / empty-gzip via finish()) | §3.2 | T6, T7 |
| 11 | split context-summary quirk: N empty + last full (== non-split summary) | §3.2 | T6 (V8) |
| 12 | `--split --gzip` combined | §3.3 | T7 (V9) |
| 13 | `--split --threshold N` → uncovered chrs get NO files | §3.2 | T6 (V14) |
| 14 | kernel/walk/cov/ContextSummary unchanged; Phase B regression | §3.4 | T8 (V11) |
| 15 | goldens generated locally from repo Perl v0.25.1 | §9 | T4 |
| 16 | V1–V14 validations | §9 | T2–T8 |
| 17 | clippy/fmt/workspace build | §9 | T8 |

All items map. ✔ Single stream (`report.rs` shared across T2/T3/T5/T6/T7).

---

## Task 1 — `ResolvedConfig.output_raw`
**Files:** `src/cli.rs`.
- **RED:** add a `validate` test: `cli(&["-o","foo.CpG_report.txt","-g","g","in.cov"]).validate().unwrap()` has `output_raw == "foo.CpG_report.txt"` AND `output_stem == "foo"` (non-split strip unchanged).
- **GREEN:** add `pub output_raw: String` to `ResolvedConfig`; in `validate`, set `output_raw = output.clone()` (the verbatim `-o`) before computing `output_stem`.
- **Regression:** all existing `cli::tests` + Phase B goldens stay green.

## Task 2 — `ReportWriter` enum
**Files:** `src/report.rs`.
- **RED:**
```rust
#[test] fn report_writer_plain_round_trip() {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("a.txt");
    let mut w = ReportWriter::create(&p, false).unwrap();
    w.write_all(b"hello\n").unwrap(); w.finish().unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), b"hello\n");
}
#[test] fn report_writer_gz_round_trip() {
    use std::io::Read;
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("a.gz");
    let mut w = ReportWriter::create(&p, true).unwrap();
    w.write_all(b"hello\n").unwrap(); w.finish().unwrap();
    let mut d = flate2::read::MultiGzDecoder::new(std::fs::File::open(&p).unwrap());
    let mut s = Vec::new(); d.read_to_end(&mut s).unwrap();
    assert_eq!(s, b"hello\n");
}
#[test] fn report_writer_gz_empty_is_valid_stream() {
    use std::io::Read;
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("e.gz");
    let w = ReportWriter::create(&p, true).unwrap(); w.finish().unwrap(); // no writes
    let bytes = std::fs::read(&p).unwrap();
    assert!(bytes.len() >= 20 && bytes[0]==0x1f && bytes[1]==0x8b, "empty gzip stream");
    let mut d = flate2::read::MultiGzDecoder::new(&bytes[..]); let mut s=Vec::new(); d.read_to_end(&mut s).unwrap();
    assert!(s.is_empty());
}
```
- **GREEN:** `enum ReportWriter { Plain(BufWriter<File>), Gz(GzEncoder<BufWriter<File>>) }`; `create(path,gzip)` = `File::create` (truncate) → `BufWriter` → optionally `GzEncoder::new(_, Compression::default())`; `write_all` delegates; `finish(self)` = `Gz(e)=>{e.finish()?; Ok(())}`, `Plain(mut w)=>{w.flush()?; Ok(())}`. `use flate2::{write::GzEncoder, Compression};`

## Task 3 — filename derivation (`base`/`report_path`/`summary_path`)
**Files:** `src/report.rs` (replace Phase B's `report_filename`/`output_path` helpers).
- **RED** (unit):
```rust
// non-split (chr=None): output_stem path
assert_eq!(report_rel("foo", "foo", None, false, false), "foo.CpG_report.txt");
assert_eq!(report_rel("foo", "foo", None, true,  true ), "foo.CX_report.txt.gz");
// split (chr=Some): RAW output, .chr infix, NO strip
assert_eq!(report_rel("split", "split", Some("chr1"), false, false), "split.chrchr1.CpG_report.txt");
// suffixed -o split → doubled suffix (extractor path, C1)
assert_eq!(report_rel("foo.CpG_report.txt", "foo", Some("chr1"), false, false),
           "foo.CpG_report.txt.chrchr1.CpG_report.txt");
// summary never gz
assert_eq!(summary_rel("split", "split", Some("chr1")), "split.chrchr1.cytosine_context_summary.txt");
```
(where `report_rel(raw, stem, chr, cx, gz)` / `summary_rel(raw, stem, chr)` are thin wrappers over the real `base`+path fns returning the filename string for the test.)
- **GREEN:** `fn base(output_raw, output_stem, chr: Option<&str>) -> String { match chr { Some(name)=>format!("{output_raw}.chr{name}"), None=>output_stem.to_string() } }`; `report_path`/`summary_path` build `{output_dir}{base}{suffix}[.gz]` / `{...}.cytosine_context_summary.txt`. Thread `config.output_raw`.

## Task 4 — generate Phase C goldens (repo Perl v0.25.1)
**Files:** `tests/data/phase_b/generate_goldens.sh` (+ new goldens).
- Append a Phase-C block: run the repo Perl for `--gzip` (gunzip → `gz.report.golden` == default), `--CX --gzip`, `--split_by_chromosome` (collect each `*.chrCHRNAME.CpG_report.txt` → `split.chrCHRNAME.report.golden` + record which summary is non-empty), `--split --gzip`, **`-o foo.CpG_report.txt --split`** (→ doubled-suffix names), and **`--split --coverage_threshold 5`** (→ only covered chrs get files). Commit the goldens + a short `phase_c_manifest.txt` listing the expected file set per mode.
- Run it once: `cd tests/data/phase_b && ./generate_goldens.sh`. Inspect + commit.

## Task 5 — `--gzip` (non-split) wiring
**Files:** `src/report.rs` `run_report` (non-split branch).
- **RED** (`tests/golden_phase_c.rs`): run binary `--gzip`; gunzip `gz.CpG_report.txt.gz`; assert raw bytes == `default.report.golden`; assert `gz.cytosine_context_summary.txt` exists, is **plain** (not gzip — first 2 bytes ≠ `1f 8b`), and == `default.summary.golden`. Same for `--CX --gzip` vs `cx.*.golden`.
- **GREEN:** in the non-split branch, build the single report writer via `ReportWriter::create(report_path, config.gzip)`; write per Phase B; `finish()` it BEFORE writing the (always-plain) summary via a plain `File`.

## Task 6 — `--split_by_chromosome` (the core)
**Files:** `src/report.rs` `run_report` (split branch) + a `flush_chromosome_to_own_file`.
- **RED** (`tests/golden_phase_c.rs`):
  - **V6/V7** file-SET + per-chr bytes: run `--split`; assert the output dir's file set exactly matches the manifest (`{stem}.chrCHRNAME.CpG_report.txt` for every genome chr incl. zero-emitting `scaf_short`, + per-chr summary files); each report file byte-equals its golden.
  - **V8** summary quirk: all-but-last summary files are 0 bytes; the last-processed chr's summary == `default.summary.golden` (the non-split summary).
  - **V12** re-appearance: genome `>chrA\nACGT\n>chrB\nACGT\n`, cov `chrA p2=5,chrB p2=1,chrA p4=7`; assert `*.chrchrA.CpG_report.txt` shows pos2 as `0 0` (truncated; only the 2nd chrA segment) and the full summary file is the chrA one (not chrB).
  - **V14** threshold split: `--split --coverage_threshold 5`; assert uncovered chrs have NO files (report or summary); only covered chrs present.
- **GREEN:** split branch — on each chromosome transition (covered, as streamed, incl. re-appearance) and each uncovered chr (sorted, threshold==0 only): open per-chr `ReportWriter::create(report_path(Some(name)), config.gzip)` (fresh truncate, **no caching**), walk via `emit_position` into a `Vec`, `write_all` + `finish()`; then `File::create(summary_path(Some(name)))` (empty) and set `last_summary_path = Some(that)`. After all chrs: write the full `ContextSummary` to `last_summary_path` (plain). Empty-input guard unchanged.

## Task 7 — `--split --gzip` + suffixed-`-o` split
**Files:** `tests/golden_phase_c.rs`.
- **V9** combined: run `--split --gzip`; each `*.chrCHRNAME.CpG_report.txt.gz` decompresses to its split golden; zero-emitting chr `.gz` is a valid empty-gzip (≥20 bytes, decompresses empty); per-chr summaries plain; last == non-split summary.
- **V13** suffixed-`-o`: run `-o foo.CpG_report.txt --split`; assert files named `foo.CpG_report.txt.chrCHRNAME.CpG_report.txt` exist + byte-equal the split goldens (content identical to bare-`-o` split; only names differ).

## Task 8 — final verification
```
cd /Users/fkrueger/Github/Bismark-c2c/rust
cargo fmt -p bismark-coverage2cytosine
cargo clippy -p bismark-coverage2cytosine --all-targets -- -D warnings   # clean
cargo test -p bismark-coverage2cytosine                                  # all green (incl. Phase B regression V11)
cargo build                                                              # workspace; siblings untouched
git -C /Users/fkrueger/Github/Bismark-c2c status --short                 # only c2c crate + plans
```
Update PLAN implementation-notes + iteration log; flip PROGRESS Phase C → ✅ contingent on plan-manager.

## Commit plan
On `rust/coverage2cytosine` (stacks onto PR #892):
```
feat(c2c): Phase C — --gzip + --split_by_chromosome

ReportWriter {Plain,Gz} (explicit finish); split-mode per-chromosome
truncating writers with the .chr-infix raw-`-o` filename (incl. the Perl
suffix-doubling) + the last-chr context-summary quirk; --gzip compares
byte-identical after decompression; summary never gzipped. Byte-identical to
Perl v0.25.1 on the gzip/split/combined/suffixed/threshold golden matrix.
```
Stage `rust/bismark-coverage2cytosine/**` + `plans/05292026_bismark-coverage2cytosine/**`.

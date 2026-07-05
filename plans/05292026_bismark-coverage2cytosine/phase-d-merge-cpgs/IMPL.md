# IMPL — Phase D (TDD task list)

**Source plan:** `phase-d-merge-cpgs/PLAN.md` (rev 1). **Goal:** `--merge_CpGs` (+ `--discordance_filter`) post-pass — re-read the CpG report, pool `+`/`-` strand pairs (with the chr-start resync), optionally divert discordant pairs. **Byte-identical to Perl v0.25.1.**

**Mode:** TDD. **Worktree:** `/Users/fkrueger/Github/Bismark-c2c`; crate `rust/bismark-coverage2cytosine/`.
**Command base:** cargo from `/Users/fkrueger/Github/Bismark-c2c/rust`; perl from the repo root. **All cargo/perl commands need `dangerouslyDisableSandbox: true`** (worktree outside the sandbox root). Do NOT touch `rust/bismark-extractor` or `rust/bismark-bedgraph`.

## Test infrastructure
- Unit: inline `#[cfg(test)]` in `merge.rs` (parse_report_row, round6, sanity) + `report.rs` (cov-path derivation).
- Integration: `tests/golden_phase_d.rs` (assert_cmd; `flate2::read::MultiGzDecoder` for gz — flate2 is a runtime dep, available to integration tests).
- Goldens: `tests/data/phase_d/` generated from the repo Perl v0.25.1 by an appended block in `tests/data/phase_b/generate_goldens.sh` (reuses the phase_b genome/in.cov + new fixtures for resync/boundary/EOF). Commit fixtures + goldens.

## Plan coverage checklist
| # | Plan item | Source | Task(s) |
|---|-----------|--------|---------|
| 1 | `MergeCpgSanityViolation { detail }` error | §3.4/§5 | T1 |
| 2 | `pub(crate)` promote `ReportWriter`/`report_path`/`report_name`; no cleanup helper | §5 (I1) | T2 |
| 3 | `merged_cov_path`/`discordant_cov_path` (report-basename strip .gz/.txt + suffix +.gz) | §3.7 | T2 |
| 4 | `parse_report_row` (7 tab fields, tri ignored) | §3.2 | T3 |
| 5 | `round6(m,u)` — `%.6f`→parse f64 | §3.5 (C1a) | T3 |
| 6 | gz-aware streaming `next_row()`; 2-row/iter `while`; EOF "<2 rows" stop | §3.2/§3.3 | T5 |
| 7 | chr-start resync (pos1<2/<1; chr1≠chr2 slide-until-match + extra advance; else single advance) | §3.3 | T5 |
| 8 | sanity asserts → `MergeCpgSanityViolation` (no panic, incl. None mid-resync) | §3.4 (C1b) | T5 |
| 9 | pool m1+m2/u1+u2; skip-zero; `%.6f` pct; **stream-write** merged lines | §3.6 | T5 |
| 10 | discordance: both-measured gate; `round6` compare `abs()>N` strict; write both rows + `continue` | §3.5 | T6 |
| 11 | `--zero_based` half-open (merged pos2+1; discordant pos+1) | §3.6/§3.5 | T7 |
| 12 | `lib::run` post-pass gated on `merge_cpgs` | §3.1 | T8 |
| 13 | NO cleanup on error; EOF-mid-resync leaves partial merged file | §3.4 | T5 (V13) |
| 14 | V1–V14 | §9 | T3–T9 |
| 15 | goldens from repo Perl v0.25.1 | §9 | T4 |
| 16 | clippy/fmt/workspace build + Phase A–C regression | §9 V11 | T9 |

All items map. ✔ Single stream (`merge.rs` central; `report.rs`/`lib.rs`/`error.rs` shared).

---

## Task 1 — `MergeCpgSanityViolation` error
**Files:** `src/error.rs`.
- RED: `error_display_strings_present` extended — `BismarkC2cError::MergeCpgSanityViolation { detail: "x".into() }.to_string()` contains "x".
- GREEN: add `#[error("merge_CpGs sanity violation: {detail}")] MergeCpgSanityViolation { detail: String }`.

## Task 2 — `report.rs` visibility + cov-path helpers
**Files:** `src/report.rs`.
- RED (inline): `merged_cov_name`/`discordant_cov_name` from a report filename:
```rust
#[test] fn merged_cov_name_strips_gz_then_txt() {
    assert_eq!(merged_cov_name("merge.CpG_report.txt", false), "merge.CpG_report.merged_CpG_evidence.cov");
    assert_eq!(merged_cov_name("merge.CpG_report.txt.gz", true), "merge.CpG_report.merged_CpG_evidence.cov.gz");
    assert_eq!(discordant_cov_name("merge.CpG_report.txt", false), "merge.CpG_report.discordant_CpG_evidence.cov");
}
```
- GREEN: promote `ReportWriter` (+ `create`/`write_all`/`finish`), `report_path`, `report_name` to `pub(crate)`. Add `pub(crate) fn merged_cov_name(report_filename, gzip)` / `discordant_cov_name(...)` (strip trailing `.gz` then `.txt`, append `.merged_CpG_evidence.cov`/`.discordant_CpG_evidence.cov`, + `.gz` if gzip) and `pub(crate) fn merged_cov_path(config)`/`discordant_cov_path(config)` = `output_dir` + name-from-`report_name(output_raw? no—None basename)`. (The merged name derives from the **report** filename = `report_name(&output_raw|output_stem, None, cx, gzip)`; since merge is non-split + non-CX, it's `{output_stem}.CpG_report.txt[.gz]`.)
- Regression: Phase B/C tests green (promotion is additive).

## Task 3 — `parse_report_row` + `round6`
**Files:** `src/merge.rs` (+ `pub mod merge;` in lib.rs).
- RED:
```rust
#[test] fn parse_report_row_fields() {
    let r = parse_report_row(b"chr1\t2\t+\t403\t400\tCG\tCGT", 1).unwrap().unwrap();
    assert_eq!((r.chr.as_slice(), r.pos, r.strand, r.m, r.u, r.context.as_slice()),
               (b"chr1".as_slice(), 2, b'+', 403, 400, b"CG".as_slice()));
}
#[test] fn round6_matches_perl_sprintf() {
    // 408/808*100 = 50.4950495… → "50.495050" → 50.495050
    assert!((round6(408,400) - 50.495050).abs() < 1e-9);
    // boundary: 11/(11+9)=55.000000? → 55.0 (NOT 55.000…007)
    assert_eq!(round6(11,9), 55.0);
}
```
- GREEN: `struct ReportRow{chr:Vec<u8>,pos:u32,strand:u8,m:u32,u:u32,context:Vec<u8>}`; `parse_report_row(line,line_no)->Result<Option<ReportRow>,_>` (strip `\r`/`\n`; blank→None; split `\t`; need ≥6 fields (Perl binds 6 vars; the trinucleotide is unused); pos/m/u strict u32; tri ignored; malformed→`MalformedCovLine`); `round6(m,u)->f64 = format!("{:.6}", m as f64/(m+u) as f64*100.0).parse().unwrap()`.

## Task 4 — generate Phase D goldens (repo Perl v0.25.1)
**Files:** append to `tests/data/phase_b/generate_goldens.sh`; new `tests/data/phase_d/`.
Produce (run Perl, capture output dirs):
- `merge/` = `--merge_CpGs` on phase_b genome+in.cov (merged cov).
- `merge_gz/` = `--merge_CpGs --gzip`.
- `merge_zero/` = `--merge_CpGs --zero_based`.
- `disc_gross/` = a both-measured Δ80 cov + `--discordance_filter 20`.
- `disc_boundary/` = cov `chr1 2 …1/1`, `chr1 3 …11/9` + `--discordance_filter 5` (→ merged, NOT discordant).
- `resync/` = genome `>chr1\nCGAACGT…` (chr-start CpG) + two consecutive `>sA\nCGT\n>sB\nCGT\n` lone-orphan scaffolds + a cov; `--merge_CpGs`.
- `eof/` = genome ending in two trailing `CGT` scaffolds whose orphans are the last report rows; `--merge_CpGs` → **Perl dies (exit 255)**; capture the partial merged file + record the nonzero exit.
- `multi/` = a 2-chromosome multi-CpG genome + cov; `--merge_CpGs`.
Run once; inspect; commit. (For `eof/`, the script must tolerate Perl's nonzero exit — run it with `|| true` and snapshot whatever merged file exists.)

## Task 5 — `run_merge` core (pairing + resync + pool + stream-write); NO discordance yet
**Files:** `src/merge.rs`, `src/lib.rs` (wire — or wire in T8).
- RED (`tests/golden_phase_d.rs`): V3 merged golden (`merge/`), V8a chr-start same-chr, V8b consecutive-short-scaffold slide (`resync/`), V9 uncovered-skip, V13 EOF (`eof/`: assert exit 1, no panic, partial merged file == Perl's partial), V4 `--gzip` (decompress vs `merge/`), V14 multi-pair (`multi/`).
- GREEN: `run_merge(config)`: open report (`report_path(config,None)`, gz-aware via `MultiGzDecoder`); `next_row()` streaming `Option<ReportRow>`; `while` loop reads `line1=next_row()`, `line2=next_row()`, `break` if either `None`; **chr-start resync** (§3.3: `pos1 < if zero {1} else {2}` → if `chr1!=chr2` slide `line1=line2; line2=next_row()` until `chr1==chr2` (or `None`→fall to asserts), then if still `pos1<thr` advance once; else single advance); **sanity asserts** → `MergeCpgSanityViolation` (a `None` row reached here also errors — no panic); pool `m1+m2`/`u1+u2`, skip if 0, `pct=format!("{:.6}",…)`; **stream-write** each merged line to a `ReportWriter::create(merged_cov_path, gzip)` opened once; `finish()` at end. Wire `lib::run` to call it (T8 if not here).

## Task 6 — discordance (rounded, both-measured, continue)
**Files:** `src/merge.rs`.
- RED: V5 gross (`disc_gross/`: merged empty + discordant golden), **V12 boundary** (`disc_boundary/`: merged has the pair, discordant empty — the raw-f64 trap), V6 gate (one strand 0,0 + big Δ → pooled, not discordant).
- GREEN: if `config.discordance.is_some()` and both measured (`m1+u1>0 && m2+u2>0`): `top=round6(m1,u1); bottom=round6(m2,u2); if (top-bottom).abs() > N as f64 {` write both rows to a `discordant` `ReportWriter` (opened once, lazily/always when discordance set), `continue` `}`. Discordant row pct = `format!("{:.6}", round6-ish)` — actually write the per-strand `%.6f` (top/bottom) as Perl does.

## Task 7 — `--zero_based` half-open
**Files:** `src/merge.rs`.
- RED: V7 (`merge_zero/`): merged `pos1 pos2+1`; (if discordance) discordant `pos pos+1`.
- GREEN: thread `config.zero_based` → merged 3rd col = `if zero {pos2+1} else {pos2}`; discordant 3rd col = `if zero {pos+1} else {pos}`.

## Task 8 — wire `lib::run`
**Files:** `src/lib.rs`.
- GREEN: after `report::run_report(config,&genome)?;` add `if config.merge_cpgs { merge::run_merge(config)?; }`. Update the lib status docstring (Phase D).

## Task 9 — final verification
```
cd /Users/fkrueger/Github/Bismark-c2c/rust
cargo fmt -p bismark-coverage2cytosine
cargo clippy -p bismark-coverage2cytosine --all-targets -- -D warnings   # clean
cargo test -p bismark-coverage2cytosine                                  # all green (incl. A–C regression V11)
cargo build                                                              # workspace; siblings untouched
git -C /Users/fkrueger/Github/Bismark-c2c status --short                 # only c2c crate + plans
```
Update PLAN implementation-notes + iteration log; flip PROGRESS Phase D → ✅ contingent on plan-manager.

## Commit plan
On `rust/coverage2cytosine` (stacks onto PR #892):
```
feat(c2c): Phase D — --merge_CpGs + --discordance_filter

merge.rs post-pass: re-read the CpG report, pair +/- strands with the
chromosome-start resync (Perl :1809-1883, incl. consecutive short scaffolds),
pool into *.merged_CpG_evidence.cov; --discordance_filter routes %.6f-rounded
discordant pairs (strict > N) to *.discordant_CpG_evidence.cov. Streamed writes
match Perl's partial-file-on-die at EOF-mid-resync (MergeCpgSanityViolation, no
panic, no cleanup). pub(crate) promotion of ReportWriter/report_path. Byte-
identical to Perl v0.25.1 on the merged/gzip/discordance/boundary/zero_based/
resync/EOF/multi golden matrix. Phase D of epic #891.
```
Stage `rust/bismark-coverage2cytosine/**` + `plans/05292026_bismark-coverage2cytosine/**`.

# Code Review B — bam2nuc test-gap closure (4 robustness cells)

**Reviewer:** B (independent, fresh context)
**Date:** 2026-05-31
**Target:** `rust/bismark-bam2nuc` test-gap closure (PR #922), branch `rust/bam2nuc`
**Worktree:** `/Users/fkrueger/Github/Bismark-bam2nuc`
**Intent doc:** `plans/05312026_bismark-bam2nuc/TEST_GAPS_PLAN.md`

---

## Summary

Four regression tests + two BAM fixtures + one golden were added to close the 4 optional
robustness handoff gaps. I verified each cell against the actual code, fixtures, and a live
build. **No production code changed** (the `count.rs` diff is purely a new `#[cfg(test)]`
function; `golden.rs` is purely additive). Every cell genuinely exercises its target branch —
none is a tautology, and the Cell-4 cross-check is real (the two BAMs differ in record order).
Build is fully green: `cargo test -p bismark-bam2nuc` = 72 unit + 17 golden + 2 sanity (1
ignored), `clippy --all-targets -D warnings` exit 0, `fmt --check` exit 0.

**Verdict: ship as-is.** No Critical/High/Medium issues. Two Low notes below are optional
polish, not blockers.

---

## Issues by area

### 1. Tautology hunt — Cell 4 (`se_sorted_stats_byte_identical`) — PASS, genuine

The cell asserts the run output `== se_sorted_stats.golden` **and** `== se_stats.golden`. I
confirmed this is **not** a "file equals itself" check:

- `report.rs:60-90 write_stats` iterates **fixed** constants `MONO` (`golden.rs`→`report.rs:38`)
  and `DI` (`report.rs:41-58`) and reads only **aggregated tallies** from `NucCounts`
  (`sample.mono(b)`, `sample.di(..)`). The per-record loop in `count.rs:170-199` accumulates
  into `NucCounts` with `+=`; nothing in the output path depends on record order. So output
  bytes are provably order-independent — the invariant the cell asserts is real, not assumed.
- `samtools view se_sorted.bam` vs `se.bam` confirms the records **are** reordered (the
  `3M1I4M` indel read `r5` moves from position 5 to position 2). So `se_sorted.bam` is a
  *different byte stream* than `se.bam`, and `se_sorted_stats.golden` was minted by Perl from
  that reordered BAM — yet `diff se_stats.golden se_sorted_stats.golden` ⇒ **IDENTICAL**.
  The cross-assert therefore proves (a) Perl agrees order doesn't matter and (b) the Rust port
  agrees, byte-for-byte. Genuine.

### 2. Fixture validity — PASS

- **`no_bismark_pg.bam`** (`samtools view -H`): `@HD SO:unsorted`, `@SQ SN:chr1 LN:17`,
  `@PG ID:bowtie2` (NO `ID:Bismark`). The samtools-injected `@PG` lines are `ID:samtools` /
  `ID:samtools.1` — neither contains the `ID:Bismark` substring `detect_paired_from_header`
  searches for (`bismark-io/src/read.rs:665`). One mapped read `r1 flag 0 chr1:1 8M`. So
  `detect_paired_from_header` → `None` → `SePeUndetermined`. Correct.
- **`se_sorted.bam`**: `@HD SO:coordinate` (genuinely sorted) AND still carries the
  `@PG ID:Bismark` (samtools appends `ID:samtools`/`.1`/`.2` *after*). So SE/PE detection still
  finds `ID:Bismark` and (no `-1`/`-2` tokens) returns `Some(false)` = SE — the same SE path as
  the unsorted golden. Correct.

### 3. No-stats-file assertion (Cell 3) — PASS, cannot flake

Traced the path: `lib.rs:75` calls `count::count_reads_in_file(...)?` **before**
`std::fs::File::create(&out_path)` at `lib.rs:84`. Inside `count_reads_in_file`
(`count.rs:151-152`), `detect_paired_from_header(&header).ok_or(SePeUndetermined)?` returns the
error *before* `count_records` is even reached. The `?` propagates up through `run()` before any
output file is opened, so `assert!(!out.join("no_bismark_pg.nucleotide_stats.txt").exists())`
(`golden.rs:332`) is deterministic. This correctly contrasts the `all_indel` cell
(`golden.rs:204-209`): there counting *succeeds*, the header-only partial IS created at
`lib.rs:84`, and the later `ZeroDivision` fires inside `write_stats` (`report.rs:105-108`).
The two assertions are reconciled and both correct.

### 4. Cell 2 (`build_chr_name_table_rejects_non_ascii_sq_name`) — PASS

- Hits the target branch: `count.rs:49 if !bytes.is_ascii()`. The `b"chr\xff"` key has a
  non-ASCII byte (0xFF) → `Err(NonAsciiChromosomeName)`. Confirmed by the lib-test run
  (`test count::tests::build_chr_name_table_rejects_non_ascii_sq_name ... ok`).
- Construction valid for `noodles-sam =0.85.0` (confirmed pinned in `Cargo.lock`): the test
  compiles and runs, so `reference_sequences_mut().insert(BString::from(..),
  Map::<ReferenceSequence>::new(ln))` is the correct API shape for that version.
- **Positive control is real**: the ASCII arm asserts `build_chr_name_table(&ok).unwrap() ==
  vec![b"chr1".to_vec()]` (`count.rs:318`), which would fail if the guard were
  unconditional. Guards against a false pass. Good.

### 5. Script edits (`generate_goldens.sh`) — PASS, reproducible

- **Ordering correct**: `se_sorted.bam` is created by `samtools sort -o ... se.bam`
  immediately *after* `se.bam` is built (diff context confirms it follows `make_bam ... se.bam`).
  `no_bismark_pg.bam` is added after `all_indel`. The `se_sorted` Perl harvest is placed after
  the `se` harvest in the run-Perl section. Matches the plan's stated ordering exactly.
- **Reproducibility**: I re-ran *only* the two new fixture commands from a fresh `mktemp` dir
  and `diff`'d records against the committed BAMs — `RECORDS IDENTICAL` for both
  `no_bismark_pg.bam` and `se_sorted.bam`. Headers match modulo the cosmetic absolute tmp-path
  embedded in samtools' own `@PG CL:` field (machine/run-specific; never consumed by any golden
  or by `detect_paired_from_header`).
- The plan's "mint only the new artifacts by hand" discipline held: `git status` shows exactly
  3 new files (`no_bismark_pg.bam`, `se_sorted.bam`, `goldens/se_sorted_stats.golden`); the 8
  existing goldens are untouched.

### 6. Build / verify — all green

| Check | Command | Result |
|---|---|---|
| Full suite | `cargo test -p bismark-bam2nuc` | 17 golden + 2 sanity + 1 ignored, 0 fail |
| Lib units | `cargo test -p bismark-bam2nuc --lib` | **72** passed, 0 fail |
| Lint | `cargo clippy -p bismark-bam2nuc --all-targets -- -D warnings` | exit **0**, no warnings |
| Format | `cargo fmt -p bismark-bam2nuc -- --check` | exit **0** |

Counts match the plan's claim (was 71/13/2 → now 72/17/2). No flakiness risk observed: all four
new cells use deterministic inputs; the version cells assert stable substrings; the temp-dir
helper (`copy_genome`/`run_stats`) is the established hermetic pattern.

### 7. Structure / style — PASS

- New `golden.rs` cells reuse the existing helpers (`bin()`, `copy_genome`, `run_stats`,
  `data_dir`, `golden`, `assert_bytes_eq`) and sit under a clearly-labelled
  "Test-gap closure cells" section (`golden.rs:288`). Consistent with file conventions.
- Cell 2 follows the established `#[cfg(test)]` pattern (function-local `use`s to avoid touching
  the module import block, as the plan specified). Matches the dedup `header_with_chrs` idiom.
- Comments are accurate and load-bearing (e.g. the Cell-3 comment correctly explains the
  pre-write error ordering; the Cell-4 comment correctly states why the second assert isn't a
  self-comparison). No dead code, no misleading comments, no naming issues.

---

## Fixes applied

None. (No unambiguous low-risk defect found; nothing to fix.)

---

## Recommendations (prioritized)

**Critical / High / Medium:** none.

**Low (optional polish — do NOT block ship):**

- **L-1 (Cell 1 OS substring is a weak e2e signal).** `version_flag_long_*` asserts stdout
  contains `std::env::consts::OS` (`golden.rs:300`). Because `version_string()` (`lib.rs:96-103`)
  embeds the same `OS` constant, the test compares the binary's stdout against the same compile-
  time constant rather than an independently-known value — it proves the constant round-trips,
  which is exactly the intent, so this is fine. Optional: also assert the literal
  `env!("CARGO_PKG_VERSION")` is present to pin that the *version* (not just the name+OS) is
  printed. Marginal value; the existing asserts already cover the clap-wiring/main-branch gap.

- **L-2 (committed BAM `@PG` carries an absolute dev-box path).** Both new BAMs embed
  `/Users/fkrueger/.../tests/data/...` and `/var/folders/...` tmp paths inside samtools' own
  `@PG CL:` fields. Harmless (no golden or detection logic reads them), and an unavoidable
  artifact of `samtools view`/`sort` provenance — the pre-existing `se.bam`/`pe.bam` fixtures
  have the same property. Noting only for completeness; no action recommended.

---

## Production-code change check

**None.** `git diff src/count.rs` shows only the added `#[cfg(test)]` function; all other `src/`
files are untouched. No public API change, no version bump, no `Cargo.toml` edit. CI gate
(`cargo test` + workspace `clippy -D warnings` + `fmt`) verified green locally.

# Code Review B — Phase C (`--gzip` + `--split_by_chromosome`)

**Reviewer:** Code Reviewer B (independent, fresh context)
**Date:** 2026-05-29
**Scope:** `rust/bismark-coverage2cytosine/src/{report,cli}.rs`, `Cargo.toml`,
`tests/golden_phase_c.rs`, `tests/data/phase_b/phase_c/` + `generate_goldens.sh`
**Contract:** byte-identical to Perl `coverage2cytosine` v0.25.1 (repo Perl, confirmed v0.25.1, runs locally)
**Worktree:** `/Users/fkrueger/Github/Bismark-c2c` (branch `rust/coverage2cytosine`)

---

## Verdict: APPROVE

Phase C is **correct and byte-identical to the live Perl v0.25.1** across every mode I
tested — well beyond the committed goldens. The two rev-1 Criticals (raw-`-o` split
filename doubling; truncate-on-reopen) are faithfully implemented and verified against
live Perl. The Phase-B non-split path is byte-equivalent after the refactor (regression
guard green). **81 tests pass; clippy `-D warnings` clean.** I found **no Critical or
High issues.** Only Low-priority doc/dependency hygiene items.

### Independent verification performed (all against live Perl, not just goldens)

| Check | Result |
|-------|--------|
| Re-derived `phase_c/{split,split_thr}` goldens from live Perl → `diff -rq` vs committed | **IDENTICAL** (genuine, reproducible) |
| `generate_goldens.sh` run in-place → reproduces **every** committed golden bit-for-bit | **IDENTICAL** (idempotent) |
| Rust binary vs Perl: `--split` whole-dir | **IDENTICAL** |
| Rust binary vs Perl: `--split --gzip` (file set + decompressed reports + plain summaries) | **IDENTICAL** |
| Rust binary vs Perl: `--gzip` non-split (file set + decompressed report + plain summary) | **IDENTICAL** |
| Rust binary vs Perl: `--CX --split --gzip` suffixed-`-o` | file set IDENTICAL |
| Rust binary vs Perl: `--zero_based --split` | whole-dir IDENTICAL |
| Rust binary vs Perl: mismatched-suffix (`-o weird.CpG_report.txt --CX --split`) | whole-dir IDENTICAL |
| Perl `--gzip` summary plain (`up...` header, not `1f 8b`); report `1f 8b`; empty-emit chr = valid 20-byte gzip → 0 bytes | confirmed; Rust matches |
| `cargo test -p bismark-coverage2cytosine` | 58 + 11 + 7 + 5 = **81 pass** |
| `cargo clippy --all-targets -- -D warnings` | **clean** |

---

## Issues by area

### Logic & correctness — none found

I traced the prompt's specific concerns and all check out:

1. **Goldens genuine / non-vacuous.** Re-derived `phase_c/split` (8 files) and
   `phase_c/split_thr` (4 files) from live Perl: byte-identical to the committed dirs.
   The whole-dir compare reads RAW bytes (`fs::read` → `Vec<u8>`), and the `file_set`
   assertion is **bidirectional** (`assert_eq!(file_set(tmp), file_set(golden))` —
   a spurious Rust file is caught by set inequality; a missing one too). Golden dirs
   are non-empty (verified) so no test passes vacuously.

2. **`run_single` regression risk — none.** The Phase-B → C refactor is byte-equivalent
   for the non-split path:
   - Summary: Phase B `BufWriter::new(File::create)` + `write_to` + `flush()`; Phase C
     `BufWriter::new(File::create)` + `write_to` + `flush()` — **identical bytes**.
   - Report (non-gzip): Phase B `BufWriter<File>` + `write_all` + `flush()`; Phase C
     `ReportWriter::Plain(BufWriter<File>)` + `write_all` + `finish()`(=`flush()`) — **identical**.
   - Filename: non-split uses `output_stem` in both. Identical.
   - All 11 `golden_phase_b.rs` tests still pass.

3. **gzip determinism — correct call.** Container is NOT byte-asserted (decompress-compare
   only); `MultiGzDecoder` is the right reader for `GzEncoder` single-member output (and
   robust to multi-member). Empty-encoder `finish()` → valid empty-gzip stream
   (test asserts ≥18 bytes + `1f 8b` magic, decompresses empty); confirmed Perl produces
   the same 20-byte empty stream for zero-emit chrs.

4. **`output_raw` plumbing — correct order, not swapped.** `cli.rs:200` sets
   `output_raw = output.clone()` **before** the strip computes `output_stem`. In
   `report_name`/`summary_name`, the `Some` (split) arm uses `output_raw` (un-stripped);
   the `None` (non-split) arm uses `output_stem` (stripped). Verified against Perl
   `handle_filehandles:99-117`: the `.chr{name}` append precedes the suffix-strip (which
   then no-ops), so a suffixed `-o` doubles its suffix — exactly reproduced. Non-UTF-8 chr
   names use `from_utf8_lossy` (documented; real names are ASCII — acceptable).

5. **`File::create` truncation / handle ordering — correct.** In `flush_split_chromosome`,
   the per-chr empty summary is `File::create(&summary_path)?;` (result **not bound** → the
   handle drops at end of statement, file closed). The path is returned and, after all
   chromosomes, `run_split` re-`File::create`s the **last** path and writes the full summary.
   No handle-ordering bug: the earlier empty `File::create` is fully closed before the final
   write to the same path. Earlier chrs' empty `File::create`s leave genuine 0-byte files
   (confirmed: 3 × 0-byte + 1 full in `phase_c/split`).

6. **Combined `--split --gzip` zero-emit chr — correct.** `GzEncoder::finish()` is called
   on the unwritten encoder → valid empty-gzip `.gz`. Test decompress-compares (does NOT
   byte-assert the container) — the right call, matching Perl's `gzip -c` pipe whose exact
   bytes are impl-dependent. Verified live: Perl scaf_short `.gz` = 20 bytes → 0 decompressed.

7. **Re-appearance summary quirk — correct.** For `chrA…chrB…chrA`, the loop discards the
   non-final flush paths; only the post-loop final flush (chrA) assigns `last_summary_path`.
   V12 test confirms chrA gets the full summary and pos2 shows `0 0` (truncated). Matches Perl.

### Efficiency

- **Per-chr `Vec<u8>` buffering (inherited from Phase B).** `chromosome_report_bytes`
  materializes a whole chromosome's report in RAM before writing. For a human chr1 the
  transient buffer is hundreds of MB. **This is in-policy:** SPEC §10.7 explicitly accepts
  the single-threaded, whole-genome-in-RAM model for v1.0 (matches Perl). Not a defect;
  noted for the candidate v1.x perf phase.

### Errors / edge cases

- **Empty-cov input (pre-existing Phase B behavior, NOT a Phase C regression).** On empty
  cov: Perl non-split writes an empty `*.CpG_report.txt` **and** a
  `*.cytosine_context_summary.txt`, then exits 255; Rust writes only the empty report
  (errors before the summary write) and exits 1. In **split** mode both Perl and Rust
  produce **zero** files (agree). The non-split file-set/exit-code divergence is inherited
  from the Phase-B `EmptyCoverageInput` guard (unchanged by Phase C); STDERR/exit-code
  identity is exempt per the epic contract. Flagging for the Phase-B/E owner, not Phase C.

### Structure / style

- clippy `-D warnings` clean. Pedantic-only warnings (single-char test bindings, doc
  backticks, "more than 3 bools", the sign-checked `isize as usize` in `perl_substr`) are
  all benign and mostly Phase-B code.

---

## Prioritized recommendations

### Critical — none
### High — none

### Medium — none

### Low

**L1 — `flate2` dev-dependency is redundant.** `flate2 = "=1.1.9"` is already a **normal**
dependency (`Cargo.toml:28`, used at runtime by `report.rs`'s `GzEncoder`). Normal deps are
available to unit + integration tests, so the dev-dep added in Phase C (`Cargo.toml:36-38`)
is unnecessary. I verified by removing it: all 81 tests still build and pass. The PLAN/IMPL
wording ("`flate2` dev-dep") also slightly mischaracterizes it (it's a runtime dep). Harmless
but tidier to drop. **Diff:**
```diff
--- a/rust/bismark-coverage2cytosine/Cargo.toml
+++ b/rust/bismark-coverage2cytosine/Cargo.toml
@@ -33,6 +33,3 @@ predicates = "=3.1.2"
 tempfile = "=3.10.1"
 # BGZF fixture writer for the .fa.gz BGZF test (genome.rs V12b).
 noodles-bgzf = "=0.47.0"
-# Phase C golden tests decompress the binary's `--gzip` output to compare
-# against the plain Perl golden.
-flate2 = "=1.1.9"
```
(If you prefer the explicit dev-dep for documentation/intent, leave it — it does no harm.
Only flagged for hygiene.)

**L2 — stale "Phase B" doc strings.** Now that Phase C ships, several doc comments are
out of date: `lib.rs:13-25` ("**Phase B** … PLAIN output … `--gzip`/`--split_by_chromosome`
… land in Phases C–E"), `main.rs:4` ("Phase B"), and `Cargo.toml:4` `description`
("Phase A: scaffold…"). Cosmetic; suggest a one-line refresh when convenient. **Suggested
`lib.rs` edit:**
```diff
-//! **Phase B** — core genome-wide report (CpG / `--CX`, `--zero_based`,
-//! `--coverage_threshold`, cytosine-context summary), PLAIN output. Builds on
-//! Phase A (CLI/validation + genome reader). Public surface:
+//! **Phase C** — core report (CpG / `--CX`, `--zero_based`,
+//! `--coverage_threshold`, cytosine-context summary) plus `--gzip` and
+//! `--split_by_chromosome` output shaping. Builds on Phase A/B. Public surface:
...
-//! `--gzip`/`--split_by_chromosome`, `--merge_CpGs`, and the real-data
-//! byte-identity gate land in Phases C–E.
+//! `--merge_CpGs` and the real-data byte-identity gate land in Phases D–E.
```

**L3 — SPEC §5 / §10.5 wording lag (already tracked).** SPEC §5 line 108 shows
`{stem}.CpG_report.txt.chr<NAME>[.gz]` (suffix *before* `.chr`) and §10.5 says
`BufWriter<GzEncoder<File>>`. The implementation correctly does the **opposite** (suffix
doubles *after* `.chr` per real Perl; `GzEncoder<BufWriter<File>>` for buffered compressed
output). The PLAN §11 already documents these as intentional deviations to "sync at next
SPEC rev." No code change — just confirming the implementation, not the SPEC, is right, and
the SPEC update remains owed.

---

## Notes for the comparison step

- Both rev-1 Criticals are implemented and **independently verified against live Perl**, not
  merely against goldens.
- The strongest evidence: I re-derived the goldens from the live Perl AND ran the Rust binary
  head-to-head with Perl across split / split+gzip / non-split-gzip / CX / zero_based /
  mismatched-suffix — all byte-identical (decompressed for gz).
- No state was left behind: I temporarily removed the `flate2` dev-dep and edited `Cargo.toml`
  to test L1, then **fully restored** it (verified `git diff` shows only the intended +3-line
  Phase C addition). `generate_goldens.sh` was re-run in place but is idempotent (byte-identical
  output, no git diff on tracked goldens).

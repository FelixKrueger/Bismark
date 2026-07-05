# Code Review A — `bismark-genome-preparation` (Rust port)

**Reviewer:** A (independent, fresh context)
**Date:** 2026-05-30
**Scope:** all `src/*.rs` + `tests/integration.rs` under
`rust/bismark-genome-preparation/`, against `SPEC.md` rev 3 / `PLAN.md` rev 1 and
the Perl source of truth (`bismark_genome_preparation`, 848 lines).
**Acceptance gate:** byte-identical CT/GA converted FASTA vs Perl v0.25.1.

> Per the caller's instruction I made **no edits** (a second reviewer runs in
> parallel). All findings are recommendations below.

---

## Summary

This is a clean, faithful, well-documented port. The byte-identity core
(`convert.rs::map_into`, `header_line`, `convert_split`) is **correct** and I
could not find a single divergence from Perl in the gated output path. I
verified the subtle cases empirically against a live Perl interpreter:

- **`map_into`**: uppercase → `[^ATCGN\r\n]→N` → `tr` ordering matches; CRLF
  preserved (`\r` in keep-set); final-no-newline preserved (raw `read_until`,
  never re-terminated); interior whitespace → N; **non-ASCII high bytes → N
  per-byte** (verified `0xC3 0x28 A 0xFF G` → `NNANG` in both Perl and the Rust
  logic); slam direction correct; `_CT_`/`_GA_` suffix fixed even in slam.
- **`extract_chromosome_name`**: bare `>` → empty name → `>_CT_converted\n`
  (verified Perl prints exactly this — Perl's `split` yields `undef`, which
  stringifies to `""`; the Rust `""` produces identical bytes); leading
  whitespace → empty (uses a split that keeps the leading empty field, not
  `split_whitespace`); only first-byte-not-`>` errors. All confirmed.
- **`convert_split` / `write_combined`**: MFA vs single_fasta writer handling,
  flush-on-replace, combined-from-stream (mode-independent) all correct;
  uniqueness checked across files in `convert_split`.
- **`indexer.rs`**: discovery tier (`BISMARK_BIN` strict → `which` → `current_exe`),
  no-fallback on explicit path, always-`--threads N`, concurrent CT/GA with
  error propagation, `*.fa` re-glob+sort — all match the contract.
- **`pipeline.rs`**: early explicit-path validation before conversion;
  `--genomic_composition` accept-and-ignore is a true no-op; step ordering correct.

**Verification:** re-ran `cargo test -p bismark-genome-preparation` →
27 lib + 5 integration tests pass (incl. both Perl-oracle byte-identity tests);
`clippy --all-targets -D warnings` clean.

The findings below are all **Low/Medium** — pathological edges and test-coverage
gaps, not gate-breaking defects. **No Critical, no High.**

---

## Issues by area

### Logic / byte-identity (the gate)

The gated path is correct. The only logic observations are non-gate edges:

1. **`find_fasta_files` excludes directories that match the glob; Perl does
   not.** `discovery.rs:40` filters with `p.is_file()`. I verified that Perl's
   `<*.fa>` **includes** a directory named e.g. `dirnamed.fa` in its result list
   (`a.fa|b.fa|dirnamed.fa|subdir.fa`). With Perl, that directory then flows into
   `process_sequence_files`, where `open(IN, $dir)` succeeds but the first
   `<IN>` read fails/returns undef → Perl proceeds to `extract_chromosome_name`
   on `undef` → `die`. So on a genome dir containing a `*.fa`-named
   **subdirectory**, Perl **errors** while the Rust port **silently skips** it
   and may proceed successfully. This is an extreme pathological case (nobody
   names a directory `chr1.fa`) and arguably the Rust behavior is *better*, but
   it is a behavioral divergence. **Priority: Low.**

2. **Non-UTF-8 filenames are silently dropped by discovery.** `discovery.rs:41-43`
   filters via `file_name().and_then(|n| n.to_str())` — a file whose name is not
   valid UTF-8 fails `to_str()` and is skipped, whereas Perl's glob would include
   it (and conversion uses byte-faithful `per_chr_path`, so it *could* be
   processed). On Linux genome dirs this is effectively never hit (FASTA names are
   ASCII), but it is an inconsistency with the byte-faithful handling elsewhere in
   the crate. Note the *indexer* re-glob (`indexer.rs:106`) uses
   `to_string_lossy()` instead, so the two globs use different UTF-8 policies.
   **Priority: Low.**

3. **Stale-`.fa` contamination on re-run is possible but matches Perl.** Both
   `indexer::build_command` (`indexer.rs:103-110`) and `combined::build` re-glob
   `*.fa` in the conversion/combined dirs and pass *everything* to the indexer.
   If a prior run left extra `*.fa` files (the `Bisulfite_Genome/` overwrite path
   warns but does not clean), they would be picked up. This is **identical to
   Perl's behavior** (Perl also re-globs `<*.fa>` post-conversion), so it is not
   a regression — noting only for completeness. **Priority: Low (no action;
   parity with Perl).**

### Efficiency

4. **`write_combined` re-reads and re-converts every input file a second time
   (and the GA pass re-reads them a third time).** `convert.rs:286-312` loops
   `for side in [Ct, Ga] { for file in files { open + stream } }`. For a human
   genome under `--combined_genome` this doubles the I/O+conversion already done
   by `convert_split`. It is correct and acceptable (combined is opt-in and the
   conversion is cheap relative to indexing), and re-streaming avoids holding the
   genome in memory — a deliberate, reasonable trade. Noting only as a known cost.
   **Priority: Low (no action recommended; document if desired).**

### Errors / robustness

5. **No `unwrap()`/`expect()`/`panic!` on any valid-input path.** I audited every
   `unwrap`/`expect` in non-test code:
   - `indexer.rs:186` `handle.join().unwrap_or_else(...)` — recovers a panicked
     CT thread into a typed `IndexerFailed` error rather than propagating the
     panic. Correct.
   - `discovery.rs:50-52` `file_name().unwrap_or(a.as_os_str())` — has a fallback,
     cannot panic.
   - All other `unwrap()`s are inside `#[cfg(test)]`. 
   No streaming slurp anywhere on the conversion path (`read_until` line-by-line);
   large-genome-safe. **No defect.**

6. **`IndexerFailed` collapses "could not spawn" and "exited non-zero" into one
   error** (`indexer.rs:147-156`). A missing-but-discovered binary, a permission
   error, and a genuine build failure all surface the same message. Diagnostics
   are explicitly **not** byte-gated (SPEC §4.2), so this is cosmetic, but it
   could make a user's "indexer not on PATH after discovery said it was" harder to
   diagnose. **Priority: Low.**

### Structure / style

7. **Doc comment on the bare-`>` case is slightly imprecise.** `discovery.rs:74`
   and `error.rs:32` say Perl "yields an empty chromosome name." Strictly, Perl's
   `split /\s+/, ""` returns an **empty list**, so `($name) = split` assigns
   `undef`; it is the subsequent `print ">",$name,...` that stringifies `undef`
   to `""` (under `no warnings`/with a STDERR warning). The **printed bytes are
   identical** to the Rust `""` path — I verified `>_CT_converted\n` byte-for-byte
   — so this is purely a comment-accuracy nit, not a behavioral issue.
   **Priority: Low.**

8. **`cli.rs` uses `std::fs::canonicalize` where Perl uses `chdir`+`getcwd`.**
   `canonicalize` resolves symlinks; `getcwd` after `chdir` does **not** (it
   returns the logical path the kernel reports, which on most systems is also
   resolved, but `getcwd` can differ for bind-mounts/automounts). This only
   affects the *path* used to locate inputs and place `Bisulfite_Genome/`, not the
   converted bytes, and SPEC §2.1 accepts absolutization as a documented
   normalization. **Priority: Low (accepted divergence; worth a one-line note in
   §4 if not already implied).**

---

## Test-coverage gaps (the tests "miss")

The SPEC (§8.9, §8 gotchas 9/15/16) calls out several edges to lock with
fixtures. The conversion logic handles them all correctly (I verified against
Perl), but several have **no explicit test**, so a future regression in
`convert_split`'s header/loop handling would not be caught:

9. **Zero-sequence record** — a header-only file (`>chr1\n`) and a header
   immediately followed by another header (`>chr1\n>chr2\nACGT\n`). I confirmed
   the Rust streaming emits just the converted header(s) with no sequence, exactly
   like Perl, but there is **no unit/integration test** for it. SPEC §8.9
   explicitly says "cover in Phase A." **Priority: Medium** (real Perl path,
   currently untested).

10. **CRLF end-to-end through `convert_split`** — `map_into` has a CRLF unit test
    (`crlf_preserved`), but no test runs a CRLF-containing **file** through
    `convert_split`/the binary to confirm headers come out LF while sequence lines
    keep CRLF (SPEC §8.3, gotcha 14 — alanhoyle's divergence #1). **Priority:
    Medium** (this is *the* signature byte-identity trap and is only covered at
    the `map_into` unit level, not the file level).

11. **CR-only (old-Mac) line endings** (SPEC §8.15) — no fixture. Behavior is
    "whole file read as one header line." **Priority: Low** (documented as a
    happens-to-agree case).

12. **slam-mode byte-identity vs Perl** — there is a `slam_direction` unit test on
    `map_into` and a `header_line` suffix test, but **no `perl_vs_rust` oracle test
    with `--slam`** confirming the full file (incl. the fixed `_CT_`/`_GA_`
    headers) matches Perl end-to-end. SPEC §8.13 / §9 decision 2 wanted a "slam
    byte-identity test." Given alanhoyle's port diverges *precisely* here, an
    end-to-end slam oracle test would be the highest-value addition. **Priority:
    Medium.**

13. **`--single_fasta` set-of-files assertion** — `perl_vs_rust_byte_identical_single_fasta`
    diffs four named files but does not assert the **set** of produced files
    matches (SPEC §7.3 "the set of files matches"). A stray/missing per-chr file
    would slip through. **Priority: Low.**

14. **0-byte FASTA file** — `empty_file_errors` covers a 0-byte file at the
    `convert_split` level (→ `NotFasta`). Good. I confirmed this matches Perl
    (first `<IN>` is undef → die). No gap.

---

## Recommendations (by priority)

**Critical:** none.

**High:** none.

**Medium (test coverage of real-but-untested edges; recommend adding before
declaring the gate fully locked):**
- (#9) Add a zero-sequence-record test (header-only file + back-to-back headers)
  at the `convert_split` or binary level.
- (#10) Add a CRLF-input **file** test through `convert_split`/the binary
  (assert LF headers + CRLF sequence bytes) — the signature byte-identity trap.
- (#12) Add a `--slam` Perl-oracle byte-identity test (end-to-end), since that is
  exactly where the prior-art port diverges.

**Low (behavioral edges / nits — accept, document, or fix at leisure):**
- (#1) Decide whether to match Perl's "directory matching `*.fa` → error" or keep
  the (arguably nicer) silent skip; if keeping, add a one-line note to SPEC §4.
- (#2) Reconcile the two globs' UTF-8 policy (`to_str` skip in `discovery.rs`
  vs `to_string_lossy` in `indexer.rs`); pick one and document.
- (#6) Optionally split `IndexerFailed` into spawn-vs-exit for friendlier
  diagnostics (not gated).
- (#7) Tighten the bare-`>` doc comment (`undef`, not `""`).
- (#8) Note the `canonicalize`-vs-`getcwd` symlink divergence in §4 (no code
  change needed).
- (#11) Add a CR-only fixture; (#13) assert the single_fasta file *set*.

---

## Verdict

The **byte-identity acceptance gate is met**: I independently verified the
conversion, header-rewriting, name-extraction, glob ordering, and slam/combined
paths against a live Perl interpreter and found **no divergence in any gated
output**. All current tests pass and clippy/fmt are clean. The remaining items
are pathological non-gate edges (Low) and a few worthwhile end-to-end test
additions (Medium) — none block acceptance.

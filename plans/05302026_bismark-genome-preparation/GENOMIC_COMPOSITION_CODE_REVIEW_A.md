# Code Review A — `--genomic_composition` (Bismark #919)

**Reviewer:** A (independent; a second reviewer runs concurrently)
**Date:** 2026-05-31
**Scope:** Rust port of Perl `bismark_genome_preparation`'s genomic mono-/di-nucleotide
frequency table (`genomic_nucleotide_frequencies.txt`).
**Branch / worktree:** `rust/genomeprep-genomic-composition` @ `/Users/fkrueger/Github/Bismark-genomeprep`
**Acceptance gate:** byte-identity of `genomic_nucleotide_frequencies.txt` to Perl Bismark v0.25.1.
**Mode:** REPORT-ONLY (no source edited).

---

## Summary

**Verdict: No correctness issues found. Ship it.**

The implementation is a careful, faithful port. I read `composition.rs` in full, traced
the Perl original (`get_genomic_frequencies` / `process_sequence` / `read_genome_into_memory`
/ `extract_chromosome_name`), hand-recomputed the two trickiest expected byte strings, and
ran the suite. All 10 byte-identity invariants from the brief hold. The live Perl-oracle test
(`perl_vs_rust_genomic_composition`) passes, which is the strongest available signal short of
the real-data gate.

Test results (run with `dangerouslyDisableSandbox: true` in the worktree, matching the
author's report):

- `cargo test -p bismark-genome-preparation --lib` → **59 passed; 0 failed** (incl. all 20
  `composition::tests`).
- `cargo test -p bismark-genome-preparation` (integration) → **13 passed; 0 failed**, including
  the live Perl oracle `perl_vs_rust_genomic_composition` and `perl_vs_rust_*` peers.
- `cargo clippy -p bismark-genome-preparation --all-targets` → **clean** (no warnings).

The only findings are Low/informational (no behavioural impact on the gate).

---

## Byte-identity invariants — verification

| # | Invariant | Verdict | Notes |
|---|-----------|---------|-------|
| 1 | Freq pass `uc`-only, NOT the `[^ATCGN]→N` conversion; IUPAC + stray bytes are their own keys; only literal `N` skipped (mono) / excludes a di-mer if EITHER base is `N` | ✅ | `count_bytes` does `to_ascii_uppercase` then `if u != b'N' { mono++ }` and `if p != b'N' && u != b'N' { di++ }`. No `[^ATCGN]→N` map. Matches Perl `unless ($mono eq 'N')` + `index($di,'N') < 0`. Confirmed against `convert::map_into` (the *other* path that DOES map to N) — they are correctly distinct. |
| 2 | `prev` (di-carry) reset per file AND at every header | ✅ | `prev` is a local in `count_file` (per file); set to `None` after every in-file `>` header (line 108). Tests `di_does_not_span_chromosomes` / `di_does_not_span_files` pin it. |
| 3 | `chomp` then `s/\r//` (first `\r` only); di-carry continues across the removed byte | ✅ | `count_sequence_line` drops one trailing `\n`, then `position(== b'\r')` removes only the FIRST `\r`, counting the two segments with a **shared** `prev` — exactly reproducing Perl's post-`s/\r//` adjacency. Hand-traced `A\r\rC` (see below). |
| 4 | Output sorted by Perl `sort keys %freqs` (plain byte cmp over mixed 1-byte mono + 2-byte di keys); NOT `fasta_name_cmp` | ✅ | `write_counts` iterates leading byte `b` ascending, emitting the 1-byte mono key first then its `[b, c]` di block for `c` ascending. A 1-byte key is a strict prefix of its 2-byte extensions, so it sorts before them; smaller leading bytes come first → exact global byte-lexical order. Uses raw `b as u8` indices, **not** the case-folding `fasta_name_cmp` (which is only used for the glob). |
| 5 | Error before any write (dup chromosome / non-`>` first line) | ✅ | `write_genomic_composition` counts **all** files (`count_file` can `Err`) and only **then** calls `write_table`. `count_file`/`check_header` return `Err` before `write_table` is reached → no orphan file. Tests `*_errors_and_no_file` / `*_no_orphan_file` assert the file does not exist. Pipeline runs this with `?` *before* `convert_split`, so a converted FASTA isn't written either. |
| 6 | First line unconditionally a header; empty file → NotFasta | ✅ | `count_file` reads the first line; `n == 0` → `NotFasta`; otherwise `check_header` (which errors if first byte ≠ `>`). Header never counted. Tests `first_line_not_header_errors_and_no_file`, `empty_file_errors_and_no_file`, `bare_gt_first_line_is_not_counted`. |
| 7 | `Mus_musculus.NCBIM37.fa` excluded from counting but NOT conversion | ✅ | `write_genomic_composition` `continue`s on `file_name() == "Mus_musculus.NCBIM37.fa"` (byte match). `convert.rs` has no such skip. Confirmed Perl has the `next if ...` ONLY at line 694 in `read_genome_into_memory`, not in `process_sequence_files`. Test `mus_musculus_file_excluded_from_counting`. |
| 8 | Non-fatal write (warn + skip on open/write/flush error); empty / N-only → 0-byte file | ✅ | `write_table` swallows `File::create` and `write_counts`/`flush` errors into `logger.note`, returns `()`, and `write_genomic_composition` returns `Ok(())`. Empty counters → `write_counts` emits nothing → a 0-byte file (still `File::create`d). Tests `n_only_genome_is_zero_byte_file`, `header_only_record_is_zero_byte_file`. |
| 9 | Wiring: AFTER `create_tree`, BEFORE `convert_split` | ✅ | `pipeline.rs` Step I.5 (lines 47–54) sits between `folders::create_tree` (line 42) and `convert::convert_split` (line 58). Matches Perl `create_bisulfite_genome_folders` → `get_genomic_frequencies` → `process_sequence_files`. |
| 10 | `to_ascii_uppercase` == Perl `uc` for the gate | ✅ | Perl's default `uc` (no `unicode_strings`/locale) folds only ASCII a–z on a byte string; bytes ≥ 0x80 are unchanged. `to_ascii_uppercase` folds exactly a–z (0x61–0x7A) and leaves all other bytes (incl. ≥ 0x80) unchanged. Identical for every byte. |

### Dup-detection semantics (insert-at-header-read vs Perl store-at-next-header)

Verified the two strategies detect the **same set of duplicates with the same (no-file)
outcome**. Perl stores a name into `%chromosomes` deferred by one header (it checks
`exists $chromosomes{$prev_name}` when it reads the *next* header / hits EOF), whereas Rust
inserts into `seen` at the moment each header is read. Traced `>A … >A … >B` and
`>A` immediately followed by `>A`: Perl dies one header later than Rust, but both die, and
on the identical condition (a name appearing ≥2 times). Because every header is read by Rust
and eventually stored by Perl, the inserted **set** is identical, so any name occurring ≥2×
trips both. Error *text* is STDERR-only (not byte-gated). The `seen` set is also shared
across files (cross-file dup), matching Perl's global `%chromosomes`; test
`duplicate_across_files_errors` covers it. **Equivalent — no divergence.**

---

## Hand-recomputed tricky expectations (both correct)

**`carriage_return_first_only_removed`** — input seq line `A\r\rC\n`:
- chomp → `A\r\rC`; `s/\r//` removes first `\r` → segments `A` and `\rC` (shared `prev`).
- mono: `A`=1, `\r`(0x0D)=1, `C`=1. di: `A\r`=1 (prev `A`→`\r`), `\rC`=1 (`\r`→`C`).
- Emit (leading byte asc, mono before its di-block): `\r`(0x0D) mono, `\rC` di; `A`(0x41) mono,
  `A\r` di; `C`(0x43) mono.
- → `\r\t1\n\rC\t1\nA\t1\nA\r\t1\nC\t1\n`. **Matches the test literal.** ✓

**`stray_space_counted_as_own_key`** — input seq line `A C\n`:
- mono `A`=1, ` `(0x20)=1, `C`=1. di `A ` (A→space)=1, ` C` (space→C)=1.
- Emit: ` `(0x20) mono, ` C` di; `A`(0x41) mono, `A ` di; `C`(0x43) mono.
- → ` \t1\n C\t1\nA\t1\nA \t1\nC\t1\n`. **Matches the test literal.** ✓

Both confirm the byte-lexical interleave (1-byte key directly before its 2-byte extensions,
including a sub-0x41 leading byte's keys sorting ahead of `A`'s).

---

## Issues by area

### Logic / correctness
None. The N-handling, di-carry (including the across-the-removed-`\r` continuation), per-file
`prev` reset, header non-counting, dup-before-write ordering, and sort all match Perl.

### Efficiency
- `[u64; 256]` mono + `vec![0u64; 65536]` di is allocation-free on the hot path (no per-base
  `Vec<u8>` keys). The `write_counts` final pass is a fixed 256×257 scan regardless of genome
  size — negligible. `count_sequence_line` allocates nothing in the common (no-`\r`) case. Good.

### Errors / robustness
- Write path is correctly non-fatal and never propagates from `write_table`. Read/count errors
  propagate via `?` *before* any write. Both match Perl.

### Structure / style
- Module is well-documented, the doc comments correctly flag the load-bearing distinctions
  (NOT the conversion path; first-`\r`-only; byte sort not `fasta_name_cmp`). Clippy clean.

---

## Recommendations (priority)

- **Low (informational, no fix needed):** The CHANGELOG entry says "20 unit tests" — there are
  exactly 20 `#[test]` fns in `composition.rs`, so this is **accurate**. (I flag it only because
  the SKILL.md count and brief mentioned "59 lib"; the per-module 20 is correct.) No action.

- **Low (optional, future hardening):** There is no unit/oracle test for a **high byte (≥0x80)**
  in a sequence line (e.g. `0xFF` counted as its own mono/di key and sorted after `T`). The
  `to_ascii_uppercase`/`uc` equivalence for such bytes is reasoned correct (and the `[u8;256]`
  arrays cover the full range), but a 1-line oracle case would pin invariant #10's tail
  explicitly. Not required for the gate (real genomes are ACGTN + occasional IUPAC); recommend
  only if a future input might carry stray high bytes.

- **Low (optional):** `perl_vs_rust_genomic_composition` is excellent but does not include a
  CRLF record or a stray-`\r` line in its oracle genome (those are pinned by unit tests with
  hand-computed expectations, not against live Perl). Adding a CRLF chromosome to that oracle's
  synthetic genome would close the last gap between "unit-verified" and "Perl-verified" for the
  `s/\r//` path. Again, not blocking — the conversion-path oracle (`perl_vs_rust_edge_inputs_mfa`)
  already exercises CRLF/CR end-to-end against Perl, and the composition `s/\r//` logic is the
  same shape.

---

## Files reviewed

- `/Users/fkrueger/Github/Bismark-genomeprep/rust/bismark-genome-preparation/src/composition.rs` (full)
- `/Users/fkrueger/Github/Bismark-genomeprep/rust/bismark-genome-preparation/src/pipeline.rs`
- `/Users/fkrueger/Github/Bismark-genomeprep/rust/bismark-genome-preparation/src/convert.rs`
- `/Users/fkrueger/Github/Bismark-genomeprep/rust/bismark-genome-preparation/src/discovery.rs`
- `/Users/fkrueger/Github/Bismark-genomeprep/rust/bismark-genome-preparation/src/error.rs`
- `/Users/fkrueger/Github/Bismark-genomeprep/rust/bismark-genome-preparation/src/cli.rs`
- `/Users/fkrueger/Github/Bismark-genomeprep/rust/bismark-genome-preparation/src/lib.rs`
- `/Users/fkrueger/Github/Bismark-genomeprep/rust/bismark-genome-preparation/src/logging.rs`
- `/Users/fkrueger/Github/Bismark-genomeprep/rust/bismark-genome-preparation/tests/integration.rs`
- `/Users/fkrueger/Github/Bismark-genomeprep/rust/bismark-genome-preparation/tests/byte_identity_real_data.rs`
- `/Users/fkrueger/Github/Bismark-genomeprep/rust/bismark-genome-preparation/CHANGELOG.md` + `Cargo.toml`
- Perl original `/Users/fkrueger/Github/Bismark/bismark_genome_preparation` (lines 186–192, 518–582, 665–751)
- Plan `GENOMIC_COMPOSITION_PLAN.md` (rev 1)

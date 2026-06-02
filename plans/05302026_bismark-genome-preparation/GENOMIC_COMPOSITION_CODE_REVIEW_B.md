# Code Review B — `--genomic_composition` (issue #919)

**Reviewer:** B (independent)
**Date:** 2026-05-31
**Branch / worktree:** `rust/genomeprep-genomic-composition` @ `/Users/fkrueger/Github/Bismark-genomeprep`
**Acceptance gate:** `genomic_nucleotide_frequencies.txt` **byte-identical** to Perl `bismark_genome_preparation` v0.25.1.

## Verdict

**APPROVE — ship.** The implementation is byte-identical to Perl across every edge case I could construct, including ones the in-tree tests do not cover (interior `\r`, blank CRLF lines, high bytes ≥0x80, large counts, multi-file di non-spanning, orphan-file suppression). No correctness defects found. Counters are allocation-free and bounds-safe. Recommendations below are all **Low** (cosmetic / optional).

## What I verified

### Live byte-identity against real Perl v0.25.1 (the strongest evidence)
I built the Rust binary (`bismark_genome_preparation_rs`) and ran both it and the real Perl script (`/Users/fkrueger/Github/Bismark/bismark_genome_preparation`) with `--genomic_composition` on a fake-indexer harness, comparing only `genomic_nucleotide_frequencies.txt` byte-for-byte. **17/17 cases byte-identical (or "neither file written", as appropriate):**

| Case | Input | Result |
|------|-------|--------|
| basic | multi-record, lowercase, IUPAC (`NRYK`), final no-newline | identical |
| single interior `\r` | `AB\rCD` (di must bridge removed `\r`) | identical |
| double `\r` | `A\r\rC` (first removed, 2nd survives, sorts before `A`) | identical |
| blank LF line | `AC\n\nGT` (di carries across blank) | identical |
| blank CRLF line | `AC\r\n\r\nGT\r\n` | identical |
| stray space+tab | `A C\tG` (counted as own keys) | identical |
| all-N | `NNNN` | both 0-byte |
| bare `>` header | `>\nACGT` | identical |
| leading-ws header | `>   chrX  desc` | identical |
| high byte ≥0x80 | `A\xc3\xa9C` (UTF-8 `é`) — `uc` vs `to_ascii_uppercase` | identical |
| lowercase `n` | `AnCg` (uppercased to `N`, then skipped) | identical |
| **duplicate chr** | `>dup…>dup…` | **neither file written** (orphan check OK) |
| Mus_musculus skip | mouse file + real chr | identical (mouse excluded) |
| multi-file di | two `.fa`, di must not span files | identical |
| gzip `.fa.gz` | gzipped multi-line | identical |
| `ANC` (prev-past-N) | A, C monos; **no** di | identical |
| large counts | 64 kbp wrapped, repeated motif (`A 12000`, `KA 3999`) | identical |

The `KA 3999` vs `K 4000` result confirms the per-chromosome trailing-base correctly drops the final di, and IUPAC codes are counted as their own keys (NOT mapped to N — the load-bearing divergence from the conversion path).

### Test suite + lint
- `cargo test -p bismark-genome-preparation`: **all pass** (59 lib unit tests incl. 20 in `composition::tests`; 13 integration incl. the live `perl_vs_rust_genomic_composition` oracle; 2 binary e2e). `11.67s`.
- `cargo clippy -p bismark-genome-preparation --all-targets`: **clean** (no warnings).

## Targeted failure-mode findings (the items the brief asked me to hunt)

1. **Di adjacency direction / order** — Correct. `count_bytes` records `(prev, current)` as `di[p*256 + u]` in source order; reproduced exactly by the live oracle (e.g. `AR`/`RC`, `RY`/`YK`). The last base of a chromosome produces no trailing di because `prev` is reset (`None`) before the next chromosome and never has a successor at end-of-file.

2. **`s/\r//` first-only + di bridge** — Correct. `count_sequence_line` splits at the *first* `\r` and calls `count_bytes` on the two segments **in order with a shared `prev`**, so (a) only the first `\r` is removed, the second is counted as its own byte, and (b) the byte before and the byte after the removed `\r` become an adjacent di-mer. `A\r\rC` → mono `\r`,`A`,`C`; di `A\r`,`\rC`. Matches Perl (`composition.rs:140-155`, test `carriage_return_first_only_removed`, and my `c3_double_cr` live case).

3. **Sort correctness over mixed 1-/2-byte keys** — Correct. For each leading byte `b` ascending, the 1-byte mono key is emitted *before* its 2-byte di block (`[b,c]`, `c` ascending). This equals Perl's `sort keys %freqs` because a 1-byte key is a prefix of (hence `lt`) every 2-byte extension, and all keys with a smaller leading byte sort first. I tried to construct a counterexample with the space (0x20) interleaving against `A`-keys — `stray_space_counted_as_own_key` expects `" \t1\n C\t1\nA\t1\nA \t1\nC\t1\n"`, which my `c6_space_tab` live run confirmed identical to Perl. No counterexample exists.

4. **Counter bounds** — Safe. `mono: [u64;256]` indexed by a `u8` (0–255). `di: vec![0u64; 256*256]` (= 65536) indexed `p*256+u` with max `255*256+255 = 65535`. No possible OOB.

5. **Allocation / perf on a 3 Gbp genome** — Clean. Counters are stack/heap-once (`[u64;256]` + one 512 KiB `Vec`). The hot `count_bytes` loop does no allocation. `count_sequence_line`'s no-`\r` fast path takes the `None` branch and slices `body` directly — **no allocation**. The only per-key allocation is in `write_table`/`emit` (`count.to_string()`), which runs ≤ 65 792 times total (once per non-zero key), not per base. `read_until(b'\n', &mut line)` reuses the same `line` buffer across the file (cleared, not reallocated). Good.

6. **Error path leaves no orphan file** — Correct and verified live. `write_genomic_composition` counts **all** files first (`count_file` can return `Err`), and only then calls `write_table`. A `DuplicateChromosome` / `NotFasta` propagates via `?` before `write_table` is reached. My `c1`/`A_dup_orphan` live case confirms neither Perl nor Rust writes a table on a dup name; `duplicate_chromosome_errors_and_no_orphan_file` asserts the same.

7. **Wiring order & no double-globbing** — Correct. `pipeline::run` discovers `files` once (`discovery::find_fasta_files`, line 32) and **reuses** that slice for composition (line 52) and conversion (line 59). No re-glob. Even if a later glob ran, the `.txt` output cannot match any FASTA extension group (`.fa`/`.fa.gz`/`.fasta`/`.fasta.gz` — see `discovery::in_group`). Placement is after `create_tree`, before `convert_split` — mirrors Perl's `get_genomic_frequencies()` → `process_sequence_files()`.

8. **`Mus_musculus.NCBIM37.fa` skip** — Correct. Matched on raw `file_name().as_encoded_bytes()` against the byte literal (`composition.rs:63`), only inside the composition loop; the conversion path (`convert_split`) iterates the same `files` slice and is **not** filtered, so the mouse file is still converted. Verified by `B_mus_skip` (excluded from counts) — the conversion side is covered by existing convert tests.

9. **N-handling (mono vs di vs prev-advance)** — Correct. Mono skips only `N`; di skips if `p == N || u == N`; `prev` advances unconditionally (`*prev = Some(u)` after the checks). `ANC` → monos `A`,`C`, no di — confirmed live identical to Perl.

10. **`to_ascii_uppercase` vs Perl `uc`** — No divergence for any real genome. Perl's `uc` without `use feature 'unicode_strings'`/locale only folds ASCII `a`–`z`; bytes ≥ 0x80 are left unchanged — same as `to_ascii_uppercase`. My `c10_highbyte` case (UTF-8 `é` = `0xc3 0xa9`) confirms byte-identical output. (Genomes are ASCII; this is belt-and-braces.)

## Recommendations (all Low / optional — none block the gate)

- **[Low] Double blank line on STDERR.** `pipeline.rs:53` calls `logger.note("Finished processing genomic nucleotide frequencies\n")`; `Logger::note` uses `eprintln!`, which appends its own `\n`, so the message is followed by **two** newlines. This is STDERR-only (explicitly *not* byte-gated per `error.rs`/`logging.rs` docs) and harmlessly mirrors Perl's `warn "...\n\n"`. Optional: drop the trailing `\n` for a single blank line, or leave as-is to match Perl's `\n\n`. No action required.

- **[Low] `extract_chromosome_name` borrows `line` while `seen` is mutated — fine, but note the lifetime.** `check_header` extracts `name: &[u8]` borrowing `line`, then does `seen.insert(name.to_vec())`. Correct (the `to_vec` copies before any conflicting borrow). No issue; flagging only that the `.to_vec()` is load-bearing for the borrow checker, not just for ownership — keep it.

- **[Low] `write_table` reports the genome-folder path in the warning, matching Perl's text intent but not byte-for-byte.** Perl's warn is `"Failed to write out file ${genome_folder}genomic_nucleotide_frequencies.txt because of: $!. Skipping..."`. The Rust message is equivalent in content. STDERR is not gated, so this is purely informational. No action.

- **[Low] CHANGELOG accuracy check — passes.** Claims "20 unit tests" (exactly 20 `#[test]` in `composition::tests`), "2 binary end-to-end" (present), live oracle + `#[ignore]` real-data gate (present). `cli.rs` doc no longer says "deferred/ignored" — confirmed (now "Calculate the genomic mono-/di-nucleotide composition…"). The `pipeline.rs` accept-and-ignore `logger.note` block is removed. Version bumped `1.0.0-alpha.1 → alpha.2` in `Cargo.toml` and `Cargo.lock`. All consistent.

## Files reviewed
- NEW `rust/bismark-genome-preparation/src/composition.rs` (full)
- `src/pipeline.rs`, `src/convert.rs` (open_fasta + map_into), `src/lib.rs`, `src/cli.rs`, `src/discovery.rs` (extract_chromosome_name, find_fasta_files), `src/error.rs`, `src/logging.rs`
- `tests/integration.rs`, `tests/byte_identity_real_data.rs`, `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock`

# Phase A Code Review — `bismark-coverage2cytosine` (Reviewer A)

**Scope:** `rust/bismark-coverage2cytosine/src/{lib,error,cli,genome,main}.rs`, `tests/sanity.rs`, `Cargo.toml`, workspace `members` line. Reviewed against PLAN.md rev 1, SPEC.md rev 2, and Perl `coverage2cytosine` v0.25.1 ground truth (`process_commandline:1990-2197`, `read_genome_into_memory:1648-1739`, `extract_chromosome_name:1741-1751`, `handle_filehandles:89-165`).

**Mode:** RECOMMEND-ONLY (per caller override). No source files edited. Fixes below are diffs/snippets for the caller to apply.

**Build state confirmed:** `cargo test -p bismark-coverage2cytosine` → 40 passed / 0 failed (35 unit + 5 integration). `cargo clippy --all-targets -- -D warnings` → clean. `cargo build` → no warnings.

---

## Verdict

**APPROVE-WITH-MINOR-FINDINGS.** Phase A is correct and faithful on every load-bearing path: the C1 context-conditional stem strip, the C2 `output_dir=""` vs `parent_dir=getcwd()` split, `cpg_only = !cx`, the full priority-ordered validation chain, the four-suffix glob priority, the Mus skip-inside-loop semantics, uppercase-on-load, duplicate-name detection, malformed/empty → `MalformedFastaHeader`, the `u32` guard, and the no-public-iterator invariant. I verified all of these line-by-line against the Perl and against the noodles-fasta 0.61.0 source.

**No Critical or High issues.** Findings are Medium (one genuine but narrow genome-header divergence + one error-mapping breadth concern), and Low (dead deps, regex-dot literal-vs-pattern nuance, a couple of test/doc tightenings). None block merge; the Medium items are worth a documented decision before the byte-identity gate (Phase E), and the leading-whitespace divergence should at minimum get a test that pins the chosen behavior.

---

## Issues by area

### 1. Logic / correctness vs Perl

**[Medium] M1 — Leading-whitespace FASTA header: Perl stores an empty-name chromosome, Rust errors.**
The PLAN (§3.3 step 4) and SPEC (§6.3) assert that noodles `record.name()` returns "exactly Perl's `split /\s+/` token 0." That is true *only when there is no whitespace immediately after `>`*. I verified the divergence with the actual Perl:

```
$ perl: split(/\s+/, "  chr1 desc")  =>  ("", "chr1", "desc")   # token 0 is ""
$ perl: extract_chromosome_name(">  chr1 desc")  =>  name = ""  # stored under empty name, NO die
$ perl: extract_chromosome_name(">")             =>  name = undef # stored, NO die
```

noodles parses `>  chr1` via `splitn(2, is_ascii_whitespace)` → first component is `""` → `name.is_empty()` → `ParseError::MissingName`, which the records iterator turns into an `io::Error(InvalidData)`. `collect_records` maps that to `MalformedFastaHeader`. So:

- Perl: `>  chr1` and bare `>` → stored chromosome with empty/undef name, **no error**.
- Rust: both → `MalformedFastaHeader` error (hard fail).

This is a real divergence, but the likelihood on a Bowtie2-built Bismark genome is essentially nil (the genome-prep step writes clean `>chrN` headers with no leading whitespace). Still, it contradicts the SPEC's stated equivalence claim and is exactly the kind of thing the Phase-E byte-identity gate is meant to be airtight about.
**Recommendation:** Make this an explicit, documented decision rather than an accidental one. Two acceptable resolutions:
  (a) **Accept the divergence** (recommended) — Bismark genomes never have leading-ws headers; document it in `genome.rs` and SPEC §6 as a known, intentional difference, and add a test that *pins* the Rust behavior (error) so it can't silently flip later. Trim the SPEC/PLAN "exactly Perl's token 0" wording to "Perl's token 0 for well-formed (`>name…`) headers; leading-whitespace headers — which Bismark genome prep never emits — error rather than store an empty-name chromosome."
  (b) Reproduce Perl exactly (store empty name) — not worth the complexity for a case that cannot occur in-pipeline.
Either way: **add a unit test** so the chosen behavior is locked.

**[Verified-correct] C1 context-conditional stem strip** — `cli.rs:190-198` strips exactly one suffix gated on `cx_context`, matching Perl `handle_filehandles:107-112` (`if ($CX_context){ s/\.CX_report.txt$// } else { s/\.CpG_report.txt$// }`). V7 asserts all four cross cases + plain. Correct.

**[Verified-correct] C2 dir defaults** — `output_dir` defaults to `""` (empty prefix), `parent_dir` to `current_dir()` (Perl `:2070-2071` `getcwd()` vs `:2108-2110` `''`). `resolve_output_dir` makes `--dir` absolute + trailing `/` (Perl `:2084-2106`). Correct; V7b covers it.

**[Verified-correct] `cpg_only = !cx`** (`cli.rs:185`, Perl `:2112-2115`); **`threshold Some(0)` → error vs `None` → 0** (`cli.rs:169,180,186`, Perl `:2174-2186`). Correct.

**[Verified-correct] Validation ordering** — v1.x flags → missing infile → missing `-o` → missing `-g` → merge mutexes (CX, split, threshold) → discordance-without-merge → discordance range → threshold-zero. Matches Perl `process_commandline` order closely enough that the *set* of triggered errors is identical for any single-defect input. (Minor ordering note in L4 below — non-functional.)

**[Verified-correct] Genome quirks** — uppercase on load (`genome.rs:188-191`, Perl `:1720` `uc`); Mus skip *inside* the per-file loop (`genome.rs:66-70`) so a Mus-only tier → empty genome with no error (V9b); four-suffix glob priority by filename, first non-empty tier wins, no union, no subdir descent (`discover_fasta_files:121-147`, Perl `:1654-1673`); duplicate-name across files → error (`genome.rs:72-76`, Perl `:1702-1705`); `u32` guard (`check_chr_len`, SPEC §15). All correct and tested.

**[Verified-correct] No-public-iterator invariant** — `chromosomes` is private; the only name accessor is `names_sorted()` (bytewise sort via `Vec<u8>: Ord` under the `LC_ALL=C` gate). No `iter()`/`keys()`/`IntoIterator` leak. Invariant upheld.

### 2. Errors

**[Medium] M2 — `MalformedFastaHeader` masks genuine I/O / decompression errors mid-file.**
`collect_records` (`genome.rs:181-184`) maps **every** `Err` from `reader.records()` to `MalformedFastaHeader { file }`:
```rust
let record = result.map_err(|_| BismarkC2cError::MalformedFastaHeader { file: ... })?;
```
But the records iterator can yield errors that are **not** header-format problems — e.g. a truncated/corrupt gzip member surfaced by `MultiGzDecoder` as an `io::Error` partway through a large `.fa.gz`, or a mid-file read error. Those get reported as "file does not look like FASTA (no '>' header / empty)", which is misleading and would send someone down the wrong debugging path on a real corrupt-genome incident. (For Phase A this only affects the error *message*, not byte-identity, hence Medium not High — but it is a genuine swallowed-failure pattern the review brief explicitly asked about.)
**Recommendation:** Preserve the underlying I/O error and only synthesize `MalformedFastaHeader` for the genuinely-empty / no-record case (which is already handled separately at `genome.rs:170-174`). The header-parse vs I/O distinction is available: noodles emits `ErrorKind::InvalidData` for parse problems and other kinds for I/O. Suggested diff:

```rust
fn collect_records<R: BufRead>(inner: R, path: &Path) -> Result<FastaRecords, BismarkC2cError> {
    let mut reader = noodles_fasta::io::Reader::new(inner);
    let mut out: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for result in reader.records() {
        let record = match result {
            Ok(r) => r,
            // noodles signals malformed definitions via InvalidData; treat
            // those as a not-FASTA file, but let real I/O errors (truncated
            // gzip, read failure) surface as Io rather than be mislabelled.
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                return Err(BismarkC2cError::MalformedFastaHeader { file: path.to_path_buf() });
            }
            Err(e) => return Err(BismarkC2cError::Io(e)),
        };
        let name = record.name().to_vec();
        let seq: Vec<u8> = record.sequence().as_ref().iter().map(u8::to_ascii_uppercase).collect();
        out.push((name, seq));
    }
    Ok(out)
}
```
Note: this interacts with M1 — `MissingName` from a leading-ws/bare-`>` header is `InvalidData`, so it would still map to `MalformedFastaHeader` (consistent with resolution (a) of M1). If the caller instead chooses to accept empty-name chromosomes (M1 resolution (b)), that branch would need separate handling. Recommend doing M1(a) + M2 together.

**[Low] L1 — `File::open` failure inside the winning tier is silent vs the dir-read.**
`read_one_fasta` (`genome.rs:153`) `?`-propagates `File::open` as `Io`, which is fine. No action needed; noting that the message won't name which file failed the way Perl's `die "Failed to read from sequence file $chromosome_filename"` (`:1681,1684`) does. STDERR is out of byte-identity scope (SPEC §2), so Low/optional only.

### 3. Efficiency

No issues. Genome held in RAM (expected, matches Perl). Single uppercase pass per record (`map(u8::to_ascii_uppercase)`). The only avoidable allocation is `name.clone()` at `genome.rs:72` for the `seen` set membership check — but it's K allocations in chromosome count K (tiny), and removing it would complicate the borrow against the subsequent `chromosomes.insert(name, seq)`. Not worth changing. The `tier_files.sort()` at `genome.rs:139` is documented as test-stability-only (order is output-irrelevant per D1) — correct and cheap.

### 4. Structure / style

**[Low] L2 — Two unused dependencies: `noodles-core` and `bstr`.**
Grep confirms neither is referenced anywhere in `src/` or `tests/`:
```
$ grep -rn "noodles_core\|noodles-core"  src tests   # no matches
$ grep -rn "bstr"                         src tests   # no matches
```
`noodles-fasta` 0.61.0's public API used here (`Reader`, `Record::name() -> &[u8]`, `Record::sequence()`) does not require a direct `noodles-core` dep, and `bstr` (a dev-dep) is unused by `sanity.rs`. They were copied from the `cram_ref.rs` pin set in the PLAN but aren't needed in Phase A. Cargo doesn't warn on unused deps, so this slipped through clippy. They add lockfile/build weight and imply a dependency that doesn't exist.
**Recommendation:** Remove both from `Cargo.toml`:
```toml
# [dependencies] — drop:
noodles-core = "=0.20.0"
# [dev-dependencies] — drop:
bstr = "=1.10.0"
```
If a later phase needs them, re-add then. (If you want to be conservative and keep `noodles-core` because Phase B is imminent and will use it, that's defensible — but `bstr` should go regardless, and a comment should mark any kept-but-unused pin as "Phase B".)

**[Low] L3 — `predicates::str::is_match` imported two ways in `sanity.rs`.**
`tests/sanity.rs:8-9` has both `use predicates::prelude::*;` and `use predicates::str::is_match;`, then the file calls `is_match(...)` (bare) in two tests and `predicates::str::contains(...)` (fully-qualified) elsewhere. It compiles and clippy is clean, but the mixed bare-vs-qualified style is slightly inconsistent. Cosmetic only.
**Recommendation (optional):** pick one style — either drop the `use predicates::str::is_match;` line and call `predicates::str::is_match`, or also import `contains`. Non-blocking.

**[Low] L4 — Validation order: `MissingCovInput` fires before `MissingOutput`; Perl is the reverse.**
`cli.rs:157-161` resolves cov infile (→ `MissingCovInput`) **before** `-o` (→ `MissingOutput`). Perl checks `-o` (`:2077`) before the cov infile is meaningfully required and actually exits on missing `@ARGV` *first* (`:2059`, before the `-o` check at `:2077`). So Perl's order for "both `-o` and infile missing" is: no-infile message first. The Rust order (infile first) happens to *match* Perl's "infile first" for the both-missing case — but it places infile before `-o` for the *one-missing* cases, which is fine since only one error surfaces per run. **Functionally equivalent** for every single-defect input; the v1.x-flags-first ordering is also fine (those are pre-`GetOptions`-failure analogues). No behavior change needed; noting for completeness since the brief asked whether *all* rules are "correctly ordered." They are, in the sense that the emitted error for any given malformed invocation matches Perl.

### 5. Test quality

Tests are substantive — they assert exact bytes (`b"ACGTACGT"`), exact stems, exact error variants via `matches!`, and exact sorted orders. No vacuous asserts. V1–V17 all map to real tests. Specific notes:

**[Low] L5 — No test pins the M1 leading-whitespace / bare-`>` behavior.**
`headerless_file_errors` covers a *non-`>`* first line, and `empty_file_in_winning_tier_errors` covers an empty file, but neither covers `>  chr1` (leading ws) or bare `>`. Whichever way M1 is resolved, add a test so the behavior is locked:
```rust
#[test]
fn leading_whitespace_header_errors() {
    // Perl would store an empty-name chromosome; we intentionally error
    // (Bismark genome prep never emits leading-whitespace headers). Pin it.
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "g.fa", ">  chr1 desc\nACGT\n");
    assert!(matches!(
        Genome::load(t.path()).unwrap_err(),
        BismarkC2cError::MalformedFastaHeader { .. }
    ));
}
```
(If M1 resolution (b) is chosen, invert the assertion to expect an empty-name chromosome.)

**[Low] L6 — `u32` overflow guard only tested via the private helper, not end-to-end.**
`u32_overflow_guard_helper` exercises `check_chr_len` directly (a >4 GiB FASTA fixture is correctly deemed infeasible — agreed). This is an acceptable compromise and the PLAN documents it. No action; noting it's the one V-row that isn't a black-box test.

**[Low] L7 — `discordance` parse-vs-validate exit-code split is untested (and deferred by design).**
`--discordance_filter abc` is a clap *parse* error (exit 2), while `--discordance_filter 0` is `DiscordanceOutOfRange` (exit 1, via `validate`). The PLAN §11 flags this as "documented, not changed." Fine for Phase A. No action.

---

## Recommendations summary (by priority)

| Pri | ID | Item | Action |
|-----|----|------|--------|
| — | — | (No Critical / High findings) | — |
| Medium | M1 | Leading-ws / bare-`>` header: Perl stores empty-name chr, Rust errors | Decide + document as intentional divergence; trim SPEC §6 "exactly token 0" claim; add test (L5). Recommend resolution (a) accept+pin. |
| Medium | M2 | `MalformedFastaHeader` masks real I/O/gzip errors mid-file | Apply the `match` diff above: `InvalidData` → `MalformedFastaHeader`, other → `Io`. Pairs with M1(a). |
| Low | L2 | Unused deps `noodles-core` + `bstr` | Remove from `Cargo.toml` (or comment `bstr`/`noodles-core` as Phase-B if keeping). |
| Low | L1 | `read_one_fasta` open-failure doesn't name the file (STDERR-only) | Optional; out of byte-identity scope. |
| Low | L3 | Mixed bare/qualified `predicates` usage in `sanity.rs` | Cosmetic; pick one style. |
| Low | L4 | `MissingCovInput` before `MissingOutput` (reverse of Perl) | No change — functionally equivalent per single-defect input. |
| Low | L5 | No test pins leading-ws/bare-`>` behavior | Add test (snippet above) regardless of M1 resolution. |
| Low | L6/L7 | `u32` guard helper-only; discordance exit-code split untested | Acceptable, documented in PLAN. No action. |

## What I verified line-by-line (no defect found)

- C1 context-conditional stem strip (cli.rs:190-198 ↔ Perl :107-112) ✔
- C2 `output_dir=""` vs `parent_dir=getcwd()` (cli.rs:200-205, resolve_output_dir ↔ Perl :2070-2071/:2084-2110) ✔
- `cpg_only = !cx`, `threshold Some(0)`→err vs `None`→0 (cli.rs:169/180/185-186 ↔ Perl :2112-2115/:2174-2186) ✔
- Full validation chain + v1.x rejections (cli.rs:142-182 ↔ Perl :2138-2186) ✔
- Glob four-suffix priority, first-filename-tier wins, no union, no subdir descent (genome.rs:121-147 ↔ Perl :1654-1673) ✔
- Mus skip inside loop ⇒ Mus-only tier empty-no-error (genome.rs:66-70 ↔ Perl :1675-1678) ✔
- Uppercase on load (genome.rs:188-191 ↔ Perl :1720) ✔
- Duplicate-name error (genome.rs:72-76 ↔ Perl :1702-1705) ✔
- noodles `record.name()` = bytes up-to-first-ASCII-whitespace; auto-strips `\r` from header+sequence (read noodles-fasta 0.61.0 `definition.rs`/`reader/sequence.rs` source) ✔
- `Genome` no-public-iterator invariant (genome.rs: private `chromosomes`, only `names_sorted()`) ✔
- main exit codes 0/1/(2-clap) + `version_string` shape match dedup house style (main.rs ↔ bismark-dedup/src/main.rs) ✔
- Empty-sequence record kept (V17), CRLF strip (V16), `.fa`-beats-`.fa.gz` (V8), bytewise sort (V13) ✔

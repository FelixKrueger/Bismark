# CODE_REVIEW_B — `bismark-genome-preparation` (Rust port of Perl `bismark_genome_preparation`)

**Reviewer:** B (independent, fresh context; no coordination with Reviewer A)
**Date:** 2026-05-30
**Scope:** all `src/*.rs` + `tests/integration.rs` of `rust/bismark-genome-preparation`, against Perl `bismark_genome_preparation` (848 lines) and SPEC rev 3 / PLAN rev 1.
**Acceptance gate:** byte-identical bisulfite-converted CT/GA FASTA vs Perl v0.25.1.
**Mode:** report-only (a second reviewer edits the same files in parallel; I did not modify anything).

---

## Summary

The crate is small, well-structured, and **faithful to the Perl byte-identity contract on the hard parts**: the raw-byte transform (`uc → [^ATCGN\n\r]→N → tr`) preserves CRLF, final-no-newline, interior-whitespace→N and non-ASCII→N exactly; the `_CT_`/`_GA_` header suffix is fixed even in slam; chromosome-name extraction reproduces Perl's bare-`>`→empty and leading-whitespace→empty semantics (the alanhoyle divergences are correctly avoided); MFA writers persist across files while single_fasta writers flush-then-replace per header. I re-ran `cargo test -p bismark-genome-preparation`: **32 tests pass** including both Perl-oracle byte-identity tests (MFA + single_fasta).

I found **one High-priority real divergence** that the current tests cannot catch (FASTA-file glob ordering uses bytewise sort, but Perl's `File::Glob` sorts **case-insensitively** — locale-independent), plus several Medium/Low items. None of the issues affect the all-lowercase-`chrN.fa` common case (which is exactly why the oracle tests pass), but the glob-order divergence is load-bearing for byte-identity on real genome dirs that mix filename case.

**Critical: 0 · High: 1 · Medium: 3 · Low: 4**

---

## Issues by area

### Logic

#### H1 (High) — Glob ordering is **bytewise**, but Perl `<*.fa>` sorts **case-insensitively** (locale-independent). Load-bearing for byte-identity.
- **Where:** `discovery.rs:48–53` (`group.sort_by(... as_encoded_bytes().cmp(...))`).
- **SPEC/PLAN claim (SPEC §8.1, PLAN A3):** "Sort on the `file_name()` bytes (C-locale / bytewise) … *Expected* to match Perl for ASCII filenames."
- **Reality — verified empirically on this host:** Perl `<*.fa>` returns, for files `{aa.fa, ab.fa, Ba.fa, ZZ.fa}`:
  - `aa.fa | ab.fa | Ba.fa | ZZ.fa`  (Perl glob — **case-insensitive collation**)
  - whereas a **bytewise** sort (what Rust does) yields `Ba.fa | ZZ.fa | aa.fa | ab.fa` (`B`=0x42, `Z`=0x5A < `a`=0x61).
  - I confirmed this is **not** locale-driven: `LC_ALL=C` and `LC_COLLATE=C` give the *same* `aa, ab, Ba, ZZ` from Perl. `File::Glob`'s default comparator is case-folding (BSD `glob` collation), **not** `strcmp`. So the SPEC's "C-locale / bytewise == Perl" assumption is **false** for mixed-case names.
- **Impact:** the glob order fixes (a) the MFA concatenation order in `genome_mfa.{CT,GA}_conversion.fa` and (b) the comma-joined indexer `file_list`. For any genome dir whose FASTA filenames mix upper/lower case (e.g. `GRCh38_primary.fa` next to `chrUn_*.fa`, or capitalised scaffold names), the **Rust MFA byte output differs from Perl** — a direct byte-identity-gate failure. Note `indexer.rs:110` (`fa.sort()` on `String`) has the same bytewise mismatch for `file_list`, but that only affects the (non-gated) index input order.
- **Why tests miss it:** the oracle fixtures use only lowercase `chr1.fa/chr10.fa/chr2.fa`, which sort identically under both orderings.
- **Recommendation:** match Perl's `File::Glob` comparator. A faithful key is a **case-insensitive primary compare with a bytewise tiebreak** — e.g. sort by `(name.to_ascii_lowercase_bytes(), name.as_bytes())`. Validate against a mixed-case fixture **through the Perl oracle** on a case-sensitive FS (oxy / Phase E), and add a unit fixture `{aa.fa, ab.fa, Ba.fa, ZZ.fa}` asserting `aa, ab, Ba, ZZ`. (If you instead decide bytewise is acceptable, the SPEC's "matches Perl" claim must be corrected and the limitation documented — but for the *gate* this is a divergence, not a doc nit.)

#### M1 (Medium) — Non-UTF-8 FASTA filename is **silently skipped** (Perl includes it).
- **Where:** `discovery.rs:40–44` — the filter is `p.file_name().and_then(|n| n.to_str()).is_some_and(|n| in_group(n, ext))`. `to_str()` returns `None` for a non-UTF-8 name, so the file is dropped from the group **without error**.
- **Perl:** `<*.fa>` globs raw bytes; a non-UTF-8-named `.fa` is included (and converted). The convert path itself is carefully byte-faithful (`per_chr_path` uses `OsStrExt`), so the discovery layer is the only place UTF-8 is assumed.
- **Impact:** a genome with a non-UTF-8 `.fa` filename produces a **silently incomplete** converted genome (missing a chromosome) rather than an error or a faithful conversion — a quiet correctness gap, contrary to the project's "fail explicitly, don't silently produce wrong results" principle.
- **Recommendation:** match on the encoded bytes instead of `to_str()`. Use `file_name().map(|n| n.as_encoded_bytes())` and run the `in_group` suffix checks on bytes (the extensions are pure ASCII, so `ends_with(b".fa")` etc. is straightforward). This also keeps the matcher consistent with the bytewise sort key right below it. Severity is Medium only because non-UTF-8 genome filenames are rare.

#### M2 (Medium) — `find_fasta_files` filters to `is_file()`, excluding a directory named `*.fa` that Perl would include.
- **Where:** `discovery.rs:39` (`p.is_file() && …`).
- **Verified:** Perl `<*.fa>` matches a **directory** `adir.fa` and would push it into `@filenames`; it then `die`s later at `open(IN, …)`/`gunzip`. Rust's `is_file()` silently excludes it.
- **Impact:** benign and arguably *more* correct (Rust won't crash on a stray `*.fa` directory). But it is a behavioral divergence worth a one-line doc note, and in the pathological case where a dir named `genome.fa` is the *only* match, Perl errors ("not in FASTA") while Rust reports `NoFasta`. Different error, same nonzero exit. Both also exclude dotfiles (verified — `.hidden.fa` matched by neither). No fix required; **document** the `is_file()` choice.

#### M3 (Medium) — Combined-genome path **re-decompresses / re-reads every source file a second time**.
- **Where:** `combined.rs:29` → `convert::write_combined` (`convert.rs:279–316`) re-opens and re-streams all `files` (CT pass + GA pass = the source read **twice more**, on top of the `convert_split` pass).
- **Impact:** efficiency only (correctness is fine; `--combined_genome` is opt-in and off by default). For a mammalian genome this triples the FASTA read I/O (and gzip decompression) for the combined run. Not a gate concern. **Recommendation (optional):** if the combined output ever needs to be cheaper, build it from the already-converted MFA bytes in MFA mode and only re-stream in single_fasta mode — but the current mode-independent "build from the converted stream" approach is the simplest correct one and matches SPEC §10.4. Leave as-is unless profiling on real data flags it; just note the 3× read cost.

### Errors / edge cases

#### L1 (Low) — Partial MFA output is left on a mid-run error (matches Perl; document, don't fix).
- **Where:** `convert.rs:211–222` creates (truncates) both `genome_mfa.*` files **before** the per-file loop; an error during file *k* (e.g. `DuplicateChromosome`, or a `NotFasta` on a later file) leaves the two MFA files partially written.
- **Perl parity:** Perl opens both MFA handles up front (lines 385–388) and equally leaves partial output on a later `die`. So this is **faithful**, not a regression. The early `--path_to_aligner` validation (pipeline.rs:45–48, before `create_tree`/conversion) correctly prevents the *one* case SPEC §4.7 cares about (bad aligner path must not leave a converted-but-unindexed genome). **Recommendation:** none; optionally note in the module doc that partial MFA output on a later-file error is intentional Perl parity.

#### L2 (Low) — Genome path that is a **file** (not a dir) yields an `Io` error instead of a validation error.
- **Where:** `cli.rs:191` `canonicalize` **succeeds** on a file; `discovery.rs:36` `read_dir` then fails with `Io("Not a directory")`.
- **Perl:** `chdir $genome_folder` fails → clean `die "Couldn't move to directory …"`. Both exit nonzero; only the message differs (not gated). **Recommendation:** optional — after canonicalize, check `is_dir()` and raise `Validation` for a friendlier message. Cosmetic.

#### L3 (Low) — `IndexerFailed` collapses "could not spawn" and "exited non-zero" into one error, losing the underlying `io::Error`.
- **Where:** `indexer.rs:147` `.map_err(|_| GenomePrepError::IndexerFailed{…})` discards the spawn error (e.g. ENOENT, EACCES). The error variant also has no `#[from]`/`source`, so the OS cause is gone.
- **Impact:** diagnostics only (not gated). A user whose `bowtie2-build` is present-but-not-executable gets "failed to build the index for <dir>" with no hint it was a spawn/permission problem. **Recommendation:** capture the `io::Error` (e.g. add a `source` field or a distinct `IndexerLaunch` variant — the PLAN A1/error.rs sketch even lists `IndexerLaunch{tool,source}`, which was dropped in the final `error.rs`). Low priority.

### Efficiency

#### L4 (Low) — `handle_header` clones the chromosome name twice.
- **Where:** `convert.rs:162–166`: `extract_chromosome_name(...)?.to_vec()` then `seen.insert(name.clone())`. The `to_vec()` is needed (the borrow ends), and `insert` needs an owned `Vec`, but `name.clone()` allocates a second copy purely to keep `name` for the header writes.
- **Impact:** negligible (one small alloc per header — there are only as many headers as chromosomes). **Recommendation:** optional — `seen.insert` can take the value and you re-derive, or use `seen.contains` + `insert`; not worth churn. Style nit.

### Structure / style (nits, no action required)

- `convert.rs` `Counts` and the `--verbose` per-chromosome stats are intentionally not byte-gated and are handled correctly (totals printed to STDOUT, `convert_split` returns `Counts`). Good.
- `logging.rs` routes both `note` and `info` to **STDERR**, whereas Perl mixes `warn` (STDERR) and `print` (STDOUT) for diagnostics. Since SPEC §4.2 explicitly de-gates all diagnostics, this is fine; just flagging that the STDOUT/STDERR split is not reproduced (intentional).
- The `#[cfg(not(unix))]` `per_chr_path` uses `String::from_utf8_lossy` on the name — on Windows a non-UTF-8 name would be lossily mangled, but this crate targets Unix (oxy/Linux) and the SPEC scopes Unix. Acceptable.
- `indexer.rs:70` pushes a synthetic `format!("$PATH/{tool}")` PathBuf into `searched` purely for the error message — harmless, slightly hacky. Fine.

---

## Verification performed

- Re-ran `cargo test -p bismark-genome-preparation` (sandbox-disabled, since `target/` is outside the write-allowlist): **27 lib unit tests + 5 integration tests pass**, including `perl_vs_rust_byte_identical_mfa` and `perl_vs_rust_byte_identical_single_fasta` (real Perl oracle, did not auto-skip — perl present).
- **Empirically probed Perl `File::Glob` ordering** (the H1 finding): `{aa.fa, ab.fa, Ba.fa, ZZ.fa}` → Perl `aa|ab|Ba|ZZ` under default, `LC_ALL=C`, and `LC_COLLATE=C` (case-insensitive, locale-independent) vs. Rust bytewise `Ba|ZZ|aa|ab`.
- Confirmed Perl `<*.fa>` matches a **directory** `adir.fa` (M2) and excludes dotfiles `.hidden.fa` (both tools agree on dotfiles).
- Confirmed Perl `chdir` on a file-path genome arg dies cleanly (L2).
- Traced the byte transform (`map_into`), header rewrite (`header_line`), name extraction (`extract_chromosome_name`), MFA-vs-single_fasta writer lifecycle (`convert_split`/`handle_header`), combined output (`write_combined`/`combined::build`), indexer command construction + discovery tier + concurrency (`indexer.rs`), and pipeline ordering (`pipeline.rs`) against the Perl source line-by-line; **no defects found in any of these** beyond the items above.

---

## Recommendations by priority

| Priority | Item | Action |
|---|---|---|
| **Critical** | — | none |
| **High** | **H1** glob order bytewise ≠ Perl case-insensitive | Sort by `(ascii-lowercased bytes, raw bytes)` to match `File::Glob`; add a mixed-case unit fixture + a Perl-oracle mixed-case run on a case-sensitive FS (Phase E). Affects the **byte-identity gate** for mixed-case genome dirs. |
| **Medium** | **M1** non-UTF-8 filename silently skipped | Match `in_group` on `as_encoded_bytes()` instead of `to_str()` (fail-loud or include faithfully). |
| **Medium** | **M2** `is_file()` excludes a `*.fa` directory Perl would include | Document the choice; no code change needed. |
| **Medium** | **M3** combined path re-reads sources (3× I/O) | Note the cost; optional optimization only if real-data profiling flags it. |
| **Low** | **L1** partial MFA on mid-run error | Matches Perl; optional doc note. |
| **Low** | **L2** file-as-genome-folder → `Io` not `Validation` | Optional `is_dir()` check for a friendlier message. |
| **Low** | **L3** `IndexerFailed` loses spawn `io::Error` | Capture `source` / restore `IndexerLaunch` variant. |
| **Low** | **L4** double-clone of chromosome name | Cosmetic. |

**Bottom line:** the byte-identity core (the transform, header rewrite, name extraction, line-ending fidelity, slam-suffix, MFA/single_fasta writer handling, combined stream) is correct and well-tested. The one finding that touches the **acceptance gate** is **H1 (glob ordering)** — it is dormant for all-lowercase `chrN.fa` dirs (hence green oracle tests) but will produce non-byte-identical MFA output for mixed-case genome filenames, and the SPEC's "C-locale/bytewise == Perl glob" premise is empirically wrong. Recommend fixing H1 and validating it through the Perl oracle on a case-sensitive filesystem before declaring the gate met.

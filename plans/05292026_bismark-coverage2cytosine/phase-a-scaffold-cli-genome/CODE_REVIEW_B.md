# Phase A Code Review — Reviewer B

**Crate:** `bismark-coverage2cytosine` (worktree `/Users/fkrueger/Github/Bismark-c2c`)
**Scope:** `src/{lib,error,cli,genome,main}.rs`, `tests/sanity.rs`, `Cargo.toml`, `rust/Cargo.toml` members.
**Contract:** byte-identical to Perl `coverage2cytosine` v0.25.1 (Phase A = scaffold/CLI/genome only).
**Reviewer:** B (independent; no shared state with Reviewer A or the auditor).
**Mode:** RECOMMEND-ONLY — no source edits applied (concurrent reviewers/auditor; caller consolidates).

---

## Verdict

**APPROVE WITH MINOR CHANGES.** The implementation is faithful, idiomatic, and well-documented; it builds clean, `cargo clippy --all-targets` is clean, and 40 tests pass (35 unit + 5 integration), verified in the worktree. The two Critical rev-1 plan folds (C1 context-conditional stem strip, C2 `output_dir`/`parent_dir` split-defaults) are correctly implemented and tested. All §3 validation rules, §5 error variants, and §3.3 genome quirks are present and match the Perl ground truth line-by-line.

**No Critical or High findings.** The findings below are Medium/Low: a genuine but low-probability **glob divergence from Perl** (leading-dot / directory entries), an **error-message conflation** (genuine I/O/decode errors reported as "not FASTA"), and the already-flagged `output_dir: String` non-UTF-8 risk (acceptable for Phase A; flagged for B/C). None block Phase A; all are worth a fix or a doc note before the byte-identity gate (Phase E) and before Phase B/C write paths land.

---

## What I verified (beyond reading)

- **Build/test/clippy** in the worktree (`dangerouslyDisableSandbox`): `cargo test -p bismark-coverage2cytosine` → 35 unit + 5 integration pass; `cargo clippy -p bismark-coverage2cytosine --all-targets` clean.
- **Perl ground truth** read line-by-line: `process_commandline` 1990–2197, `read_genome_into_memory` 1648–1739, `extract_chromosome_name` 1741–1751, `handle_filehandles` 90–165.
- **noodles-fasta 0.61.0 source** (`io/reader.rs`, `io/reader/records.rs`, `io/reader/definition.rs`, `record/definition.rs`) to nail down exactly which errors `collect_records` maps to `MalformedFastaHeader`.
- **Empirical probes** (throwaway, cleaned up — worktree confirmed clean afterward):
  - Suffix disjointness across the four tiers (`rustc` snippet).
  - Perl `<*.fa>` vs leading-dot files, directories, and uppercase `.FA` (Perl scripts).
  - Filesystem case-sensitivity on this darwin host.
  - Truncated-gzip error surfacing through noodles (`kind=UnexpectedEof`).
  - noodles behavior on empty / blank-line / comment-first / leading-whitespace files vs Perl `extract_chromosome_name`.
  - `--threshold`/`--coverage_threshold` aliases and `-V`/`--version` at runtime.

---

## Findings by area

### A. Genome glob discovery (`genome.rs::discover_fasta_files`)

#### B-1 (Medium) — `ends_with(tier)` matches leading-dot / hidden files that Perl's `<*.fa>` glob excludes
Perl's `<*.fa>` uses shell-glob semantics where `*` does **not** match a leading dot. Empirically confirmed on this host:

```
files on disk: .fa  .fa.gz  chr1.fa  chr2.fasta
Perl <*.fa>     -> [chr1.fa]      (count 1)   # .fa NOT matched
Perl <*.fa.gz>  -> []             (count 0)   # .fa.gz NOT matched
```

The Rust filter `n.ends_with(tier)` **does** match `.fa` and `.fa.gz` (confirmed: `".fa" -> matches [".fa"]`, `".fa.gz" -> matches [".fa.gz"]`). So a genome folder that contains a hidden artifact ending in a FASTA suffix — e.g. a partial download `.GRCh38.fa.gz`, an editor swap/backup `.chr1.fa`, a sync-tool tempfile — would be **picked up by Rust but skipped by Perl**. Two failure modes:
1. The hidden file is the *only* thing in its tier → Rust loads it (or errors on it), Perl falls through to the next tier or to `NoGenomeFasta`. Different genome → byte-divergent output.
2. The hidden file is a stray alongside real `.fa` files in the same tier → Rust reads an extra "chromosome" (or errors), Perl ignores it.

This is low-probability for a clean Bismark genome dir but is a true contract divergence with the byte-identity target. **Recommend** excluding dotfiles to match Perl glob semantics:

```rust
.filter(|p| {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| !n.starts_with('.') && n.ends_with(tier))
})
```

Add a test: a dir containing `.hidden.fa` + `chr1.fa` loads only `chr1`; a dir whose only `.fa`-suffixed entry is `.foo.fa` → `NoGenomeFasta`.

#### B-2 (Low) — `is_file()` filter silently drops directory entries that Perl's glob would match and then die on
Perl `<*.fa>` includes a **directory** named e.g. `subdir.fa` in its match list (confirmed: `<*.fa> -> [chr1.fa subdir.fa]`), then tries to `open`/read it as FASTA and ultimately dies. The Rust code filters with `p.is_file()`, so a directory named `*.fa` is silently excluded. Net effect: Perl errors, Rust proceeds. Pathological and arguably Rust's behavior is *better*, but it is an undocumented divergence. **Recommend** a one-line doc comment on the `is_file()` filter noting it intentionally diverges from Perl (which would attempt to open dir entries), or leave as-is and document in the SPEC's pitfall catalog. Not worth changing behavior.

#### B-3 (info, not a bug) — uppercase extensions and tier disjointness are correct
- `.FA` / `.Fasta`: Perl glob is case-sensitive **even on this case-insensitive darwin FS** (confirmed: `<*.fa>` on a dir with `genome.FA` → count 0). Rust `ends_with(".fa")` also rejects `genome.FA`. **Consistent — no action.**
- The four tiers are mutually disjoint for canonical names (each matches exactly one tier); `.fasta` does not end with `.fa`. `.fasta.gz` correctly routes through `MultiGzDecoder` because `read_one_fasta` keys off `ends_with(".gz")` (which `.fasta.gz` satisfies). **All correct.**

### B. Genome FASTA reading & error mapping (`genome.rs::collect_records` / `read_one_fasta`)

#### B-4 (Medium) — `collect_records` maps *every* noodles record error to `MalformedFastaHeader`, swallowing genuine I/O / decode errors
`records()` returns `io::Result<Record>` whose `Err` can be either (a) a header-parse `InvalidData` (genuinely "not FASTA") **or** (b) a real `io::Error` from `read_definition`/`read_sequence` — including a **truncated/corrupt gzip** mid-stream. Confirmed empirically: feeding a truncated `.fa.gz` through `MultiGzDecoder` → noodles yields `Err(kind=UnexpectedEof, "unexpected end of file")`, which `collect_records` reports to the user as:

> `file …/g.fa.gz does not look like FASTA (no '>' header / empty)`

That message is actively misleading for a partial download — the file *is* FASTA, the gzip is truncated. This is a **diagnostic-quality** issue, not a byte-identity one (Perl's `gunzip -c` would also fail, just with a different message; STDERR is not gated). Still worth fixing for operability. **Recommend** distinguishing the two error classes by `io::ErrorKind`:

```rust
for result in reader.records() {
    let record = result.map_err(|e| match e.kind() {
        std::io::ErrorKind::InvalidData => BismarkC2cError::MalformedFastaHeader { file: path.to_path_buf() },
        _ => BismarkC2cError::Io(e),   // truncated gzip, read error, etc. surface as Io
    })?;
    ...
}
```

(`BismarkC2cError::Io(std::io::Error)` already exists via `#[from]`, so this is low-risk.)

#### B-5 (info, not a bug) — "zero records ⇒ malformed" is correct
I checked the worry that a "validly whitespace/comment-only" file would be wrongly rejected. FASTA has **no comment syntax**, and Perl's `read_genome_into_memory` reads the first line and dies in `extract_chromosome_name` unless it starts with `>`. Verified Perl dies on: empty, single blank line, leading-whitespace line, leading-blank-then-header, and `#`-comment-first. noodles returns either zero records (empty → caught by the `records.is_empty()` guard → `MalformedFastaHeader`) or an `InvalidData` error (blank/comment/whitespace → mapped to `MalformedFastaHeader`). **Both implementations error on every such input** — the zero-records guard is faithful. No action (the only refinement is B-4's message split). The `b1` empty-file and headerless-file tests already cover this.

#### B-6 (info) — `record.name()` / `\r` / uppercase all confirmed
noodles `Definition::name()` returns bytes up to the first ASCII whitespace (matches Perl `split /\s+/` token 0); `read_line` strips trailing `\r` and `\r\n`; the code uppercases via `to_ascii_uppercase`. The `crlf_sequence_has_no_carriage_return`, `loads_multifasta_first_token_name_and_uppercases`, and `empty_sequence_record_kept` tests lock these. Matches Perl. **No action.**

### C. CLI parsing & validation (`cli.rs`)

#### C-1 (info, not a bug) — validation order vs Perl
The Rust order is: v1.x flags → missing infile → missing `-o` → missing `-g` → merge mutexes → discordance-without-merge → discordance range → `threshold==Some(0)`. Perl's order differs in two user-visible ways, both **acceptable**:
- **v1.x rejection happens first in Rust**, before missing `-o`/`-g`. In Perl those flags are accepted silently (the modes exist), so there's no Perl analogue to compare against; rejecting early with a clear "not supported in the Rust port" message is the documented SPEC §2/§3 behavior (P9). Fine.
- **Missing infile vs missing `-o`:** Perl checks `@ARGV` (infile) at :2059 (prints help + `exit 0`), then `-o` at :2078 (`die`). Rust checks infile first too (`MissingCovInput`) then `-o`. Order matches; Rust surfaces a typed error + exit 1 instead of help + exit 0 for the missing-infile case — a deliberate, documented divergence (`error.rs` doc on `MissingCovInput`). Acceptable; STDERR/exit-code parity is not gated.

The `discordance Some(0)` path is correctly handled: with `--merge_CpGs` set it passes `DiscordanceWithoutMerge` and trips `DiscordanceOutOfRange` on the `!(1..=100)` check — matching Perl :2168 `unless ($disco > 0 and $disco <= 100)`. The `rejects_discordance_out_of_range` test covers both `0` and `101`. Good.

#### C-2 (info, not a bug) — clap surface
Verified at runtime: `--CX` and `--CX_context` both set `cx_context`; `-CX` is rejected (no bundled `-C -X` shorts); `--threshold` and `--coverage_threshold` both accepted; `-V` and `--version` both print provenance with `disable_version_flag = true`. The v1.x flags parse (so the rejection message fires) rather than erroring at clap. Nothing should be `conflicts_with` at parse time — the mutex checks (merge+CX, merge+split, etc.) deliberately live in `validate()` to echo Perl's `die` wording and keep exit code 1 (a clap `conflicts_with` would exit 2 and emit clap's generic message, diverging from the SPEC's typed-error contract). **The validate-time placement is the correct choice.** No action.

#### C-3 (Low) — `output_dir: String` + `to_string_lossy` can corrupt a non-UTF-8 `--dir`
`resolve_output_dir` does `abs.to_string_lossy().into_owned()`, lossily replacing non-UTF-8 path bytes with U+FFFD. On a non-UTF-8 `--dir`, the resolved prefix would silently differ from the real directory, and Phase B/C would write to (or fail to create) the wrong path. This is **already flagged in PLAN §11 "Remaining risks"** and is **acceptable for Phase A** (nothing is written). The `String`-prefix choice faithfully mirrors Perl's `"${output_dir}${file}"` string concatenation, and Perl is equally non-UTF-8-fragile, so this is arguably byte-faithful. **Recommend (for Phase B/C, not now):** either keep `output_dir` as a `PathBuf` joined with the filename (avoids the lossy round-trip entirely) or add a hard error if `to_string_lossy` is lossy (`Cow::Owned` ⇒ contained replacements). Flagging here so it isn't lost when the write path lands.

### D. main / lib / error / Cargo

#### D-1 (info) — `main.rs`, `lib.rs`, `error.rs` are clean
Exit codes (0/1/2) match the dedup precedent and are documented; `--version` short-circuits before `validate` (correct — mirrors Perl :2042 before the infile check); `version_string()` provenance format is consistent with TG style and locked by the `sanity.rs` regex. `#![forbid(unsafe_code)]` + `#![warn(missing_docs)]` present; every public item is documented. The `Genome` no-public-iterator invariant (only `names_sorted()`) is held and documented — keeps SPEC D1 airtight. Error strings echo Perl wording where it exists. **No action.**

#### D-2 (Low) — `Cargo.toml` carries `bstr` as a dev-dependency that the test code does not use
`tests/sanity.rs` and the in-crate unit tests use `assert_cmd`, `predicates`, `tempfile`, `flate2`, `noodles-bgzf` — but I see no `bstr` usage in any test. PLAN §2 listed `bstr` as a planned dev-dep, but it appears unused in the delivered tests. **Recommend** removing the `bstr = "=1.10.0"` dev-dependency unless a Phase-A test actually references it (none found), or confirm it's an intentional pre-stage for Phase B. Minor (an unused dep, not a correctness issue). Worth a `cargo +nightly udeps` check at consolidation time.

### E. Test quality (`cli.rs` tests, `genome.rs` tests, `tests/sanity.rs`)

Overall the tests are strong, non-vacuous, and map cleanly to V1–V17. Specific notes:

#### E-1 (info, not a bug) — `given_dir_is_absolute_with_trailing_slash` is meaningful cross-platform
It asserts `output_dir.ends_with('/')` and `Path::new(&output_dir).is_absolute()`. On Windows `std::path::absolute` yields backslashes and `\` separators, so `ends_with('/')` would be wrong there — **but** the crate targets Unix (Bismark is Unix-only; CI/colossal are Linux), and the implementation hardcodes `'/'` in `resolve_output_dir`, so the test correctly locks the actual (Unix) behavior. Not a portability bug for this project. No action; optionally note "Unix-only" in a comment.

#### E-2 (Low) — no test asserts a `.fasta` tier is chosen when no `.fa`/`.fa.gz` exists, nor `.fasta.gz` as last resort
`glob_priority_fa_beats_fa_gz` covers tier 1 vs tier 2, but there is no test that:
- a dir with only `*.fasta` loads via tier 3, and
- a dir with only `*.fasta.gz` loads via tier 4 (and routes through the gz decoder).

These are the lower-priority tiers that the four-tier fallthrough exists for; an interior tier-selection bug (e.g. a future refactor reordering `FASTA_TIERS`) would go uncaught. **Recommend** adding two small tests (`.fasta` only → loads; `.fasta.gz` only → loads via MultiGzDecoder). Low because the tier loop is simple and tier 1/2 are exercised, but the gz-via-`.fasta.gz` path is currently untested end-to-end.

#### E-3 (Low) — no test for the dotfile / hidden-file glob behavior (ties to B-1)
Whichever way B-1 is resolved (exclude dotfiles to match Perl, or keep current behavior), add a test pinning the decision so it can't silently regress before the Phase E byte-identity gate.

#### E-4 (info) — `error_display_strings_present` is a light but adequate smoke test
It checks four representative variants contain key substrings. Not vacuous; fine for Phase A. The richer error-wording fidelity check belongs in the Phase E byte-identity comparison (and STDERR isn't gated anyway).

---

## Prioritized recommendations

### Critical
*(none)*

### High
*(none)*

### Medium
- **B-1 — Exclude leading-dot files in `discover_fasta_files`** to match Perl `<*.fa>` glob (which never matches dotfiles). Add `!n.starts_with('.')` to the filter + a test. This is the one finding with a real (if low-probability) path to byte-divergent output vs the v0.25.1 target.
  ```rust
  .is_some_and(|n| !n.starts_with('.') && n.ends_with(tier))
  ```
- **B-4 — Split error mapping in `collect_records`** so genuine I/O/decode errors (e.g. truncated gzip → `UnexpectedEof`) surface as `BismarkC2cError::Io` instead of the misleading `MalformedFastaHeader` ("no '>' header / empty"). Reserve `MalformedFastaHeader` for `ErrorKind::InvalidData`.
  ```rust
  let record = result.map_err(|e| match e.kind() {
      std::io::ErrorKind::InvalidData =>
          BismarkC2cError::MalformedFastaHeader { file: path.to_path_buf() },
      _ => BismarkC2cError::Io(e),
  })?;
  ```

### Low
- **C-3 — Phase B/C, not now:** plan to drop `output_dir`'s `to_string_lossy` round-trip (join a `PathBuf` with the filename, or hard-error on lossy conversion) when the write path lands. Already noted in PLAN risks; recording so it isn't dropped.
- **B-2 — Doc the `is_file()` divergence:** one comment noting Rust intentionally skips directory entries that Perl would try to open-and-die on. No behavior change.
- **D-2 — Remove the unused `bstr` dev-dependency** (no test references it), or confirm it's intentional Phase-B staging. Run `cargo udeps` at consolidation.
- **E-2 — Add `.fasta`-only and `.fasta.gz`-only tier-selection tests** (the latter also exercises the gz path for the fourth tier).
- **E-3 — Add a dotfile-glob test** pinning the B-1 decision.

### Info (no action / confirmations)
- B-3 (uppercase ext + tier disjointness correct), B-5 (zero-records-⇒-malformed is faithful), B-6 (name/`\r`/uppercase faithful), C-1 (validation order acceptable), C-2 (clap surface correct; mutexes correctly at validate-time not parse-time), D-1 (main/lib/error clean), E-1 (`--dir` test is Unix-correct), E-4 (error smoke test adequate).

---

## Build evidence

```
cargo test  -p bismark-coverage2cytosine  → 35 unit + 0 doctest + 5 integration, all pass
cargo clippy -p bismark-coverage2cytosine --all-targets → clean
runtime: -V/--version print provenance; --CX/--CX_context parse, -CX rejected;
         --threshold/--coverage_threshold both accepted
```

Worktree confirmed clean after all probes (no stray `examples/` or temp files; only the legitimate untracked Phase-A crate + `rust/Cargo.{toml,lock}` deltas remain).

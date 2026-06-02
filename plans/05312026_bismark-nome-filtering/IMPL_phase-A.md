# IMPL — Phase A (TDD): scaffold + CLI + promoted `bismark_io::genome`

**Source plan:** `plans/05312026_bismark-nome-filtering/SPEC.md` (rev 1). Phase A goal: the new `bismark-nome-filtering` crate (lib+bin `NOMe_filtering_rs`) **boots** — `--help`/`--version` work, the clap CLI validates per §4 (incl. the one die + inert-flag acceptance + the `--dir` path contract), the promoted `bismark_io::genome` module loads a genome, `BismarkNomeError` + `filename.rs` + `perl_substr` exist and are unit-tested, and `cargo build --workspace` still resolves all seven sibling `=1.0.0-beta.8` pins. **No per-read processing or output file — that is Phase B.**

**Mode:** TDD (RED → GREEN → REFACTOR). Rust/cargo conventions.

> ⚠️ **Sandbox:** the worktree `~/Github/Bismark-nome` is OUTSIDE the command sandbox. Every `cargo`/`git` Bash invocation below MUST be run with `dangerouslyDisableSandbox: true`. (Read/Write/Edit tools work normally.)

> **Reference crates (read, mirror idioms):** `rust/bismark-dedup/src/{cli,main,lib,error,filename}.rs` (scaffold), `rust/bismark-coverage2cytosine/src/genome.rs` (the gz-capable genome twin to tier-parameterize), `rust/bismark-io/src/{lib,error}.rs` + `Cargo.toml` (the crate being extended).

---

## Plan coverage checklist

| # | Plan item (SPEC §) | Source | Task(s) |
|---|--------------------|--------|---------|
| 1 | New crate `rust/bismark-nome-filtering` lib+bin `NOMe_filtering_rs`, 8th workspace member, `#![forbid(unsafe_code)]`/`#![warn(missing_docs)]` | §10, §11A | T1 |
| 2 | `version_string()` = `NOMe_filtering_rs <semver> (<os>/<arch>)`, crate `0.1.0-beta.1`, `disable_version_flag`+`--version`/`-V` | §10 | T1, T5 |
| 3 | Promote `bismark_io::genome` (distinct from `cram_ref`): tier-parameterized `load(folder, &[&str])`, first-non-empty-tier glob (no union, dotfile-exclude), uppercase, Mus skip, `\r` strip, first-token name, dup-name error, `u32` guard, `HashMap<Vec<u8>,Vec<u8>>`, NO public insertion-order iterator | §7, §D1 | T2 |
| 4 | Module-local `GenomeError` (NOT `BismarkIoError` variants) | §7, §D5, §P16 | T2 |
| 5 | `bismark-io` gains `flate2` dep; **NO version bump**; `cargo build --workspace` resolves all `=beta.8` pins | §7, §P7 | T2, Final |
| 6 | NOMe consumes `load(folder, &[".fa",".fasta"])` (two PLAIN tiers); `.fa.gz` footgun preserved via the tier list | §7, §P6, §P14 | T6 |
| 7 | Inherit c2c's bare-`>`-header divergence (errors) | §7 | T2 |
| 8 | `perl_substr` (§9): negative-in-range→tail, `\|offset\|>L`→empty, `start==L`→empty/no-panic, over-length→truncate | §9, §P1 | T3 |
| 9 | `filename.rs` `derive_manowar_name`: strip ONE `.gz` then ONE `.txt` (each once), append `.manOwar.txt`, force `.gz`; NO directory strip; NOT dedup's loop | §4, §P15 | T4 |
| 10 | clap `Cli` → `validate() -> Result<ResolvedConfig, BismarkNomeError>`: live flags (infile, `-g`, `--dir`, `--parent_dir`[inert], `--version`); inert flags accepted (`--zero_based`/`-CX`/`--GC`/`--gzip`/`--nome-seq`/`--merge_CpGs`); the `--merge_CpGs`+`-CX` die; mandatory-genome die; infile-exists; `--dir` path contract (input=`dir.join(infile)`, output=`dir.join(derived)`, no real chdir) | §4, §10 | T5 |
| 11 | `BismarkNomeError` (thiserror): `Genome(#[from] GenomeError)`, `MissingGenomeFolder`, `InfileNotFound`, `EmptyInput`, `MergeCpgsWithCx`, `Io(#[from] std::io::Error)` | §10 | T5, T6 |
| 12 | `main.rs` wiring: `parse → --version? → run → ExitCode`; Phase-A `run()` = validate + create dir + resolve paths + infile-exists + genome load (+ Perl-style stderr); NO output (Phase B) | §10, §11A | T6 |

*Every row maps to ≥1 task. `EmptyInput` is declared in T5 but only **raised** in Phase B (the header-then-error path, D4) — declared here so the enum is complete; noted, not left uncovered.*

## Test infrastructure

- Unit tests live in-module (`#[cfg(test)] mod tests`) per sibling convention — `bismark_io::genome` tests in `genome.rs`; `perl_substr`/`filename`/`cli` tests in their modules.
- Integration/CLI tests use `assert_cmd` + `predicates` + `tempfile` (dev-deps), mirroring c2c's `tests/`. Phase A integration tests assert: `--version` string, `--help` exits, validation errors (merge+CX, missing genome), and a valid invocation that loads a tiny synthetic genome and exits 0.
- Synthetic genome fixtures are built inline with `tempfile::tempdir()` + `std::fs::write` (the c2c `genome.rs` test pattern) — **no external test data needed for Phase A**.

---

## Task 1 — Crate scaffold + workspace member + version

**Files:**
- New: `rust/bismark-nome-filtering/Cargo.toml`
- New: `rust/bismark-nome-filtering/src/lib.rs` (modules + `version_string()`)
- New: `rust/bismark-nome-filtering/src/main.rs` (`--version` only for now)
- New: `rust/bismark-nome-filtering/src/error.rs` (skeleton — filled in T5/T6)
- Modify: `rust/Cargo.toml` — add `"bismark-nome-filtering"` to `members`

**Step 1: Write the failing test** (`src/lib.rs` test module)
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_string_has_binary_name_and_semver() {
        let v = version_string();
        assert!(v.starts_with("NOMe_filtering_rs "), "got: {v}");
        assert!(v.contains(env!("CARGO_PKG_VERSION")), "got: {v}");
        assert!(v.contains(std::env::consts::OS), "got: {v}");
    }
}
```

**Step 2: Run, confirm it fails** (crate doesn't exist / `version_string` undefined)
```bash
cargo test -p bismark-nome-filtering --lib   # dangerouslyDisableSandbox: true
```
Expected failure: package `bismark-nome-filtering` not found (until Cargo.toml + workspace member added), then `version_string` unresolved.

**Step 3: Implement**

`rust/bismark-nome-filtering/Cargo.toml` (mirror dedup; pin to the same versions the workspace already uses — verify against `rust/bismark-dedup/Cargo.toml` and `rust/bismark-coverage2cytosine/Cargo.toml`):
```toml
[package]
name = "bismark-nome-filtering"
version = "0.1.0-beta.1"
description = "Rust port of Bismark Perl's standalone NOMe_filtering script"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
keywords = ["bisulfite", "methylation", "bismark", "nome-seq", "bioinformatics"]
categories = ["science::bioinformatics", "command-line-utilities"]

[lib]
path = "src/lib.rs"

[[bin]]
name = "NOMe_filtering_rs"
path = "src/main.rs"

[dependencies]
bismark-io = { version = "=1.0.0-beta.8", path = "../bismark-io" }
clap = { version = "=4.5.30", features = ["derive"] }
thiserror = "=2.0.0"
flate2 = "=1.1.9"          # Phase B: always-gzipped output + gz input

[dev-dependencies]
assert_cmd = "=2.0.16"
predicates = "=3.1.2"
tempfile = "=3.10.1"
```

`rust/Cargo.toml` — append the member:
```toml
members = ["bismark-io", "bismark-dedup", "bismark-extractor", "bismark-methylation-consistency", "bismark-bedgraph", "bismark-coverage2cytosine", "bismark-genome-preparation", "bismark-nome-filtering"]
```

`src/lib.rs`:
```rust
//! `bismark-nome-filtering` — Rust port of Bismark Perl's standalone
//! `NOMe_filtering` (v0.25.1). Per-read NOMe-Seq classifier; byte-identical.
//! NOT `coverage2cytosine --nome-seq` (separate tool). See
//! `plans/05312026_bismark-nome-filtering/SPEC.md`.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;       // T5
pub mod error;     // T1 skeleton → T5/T6
pub mod filename;  // T4
pub mod substr;    // T3

/// TG-style provenance string for `--version`.
#[must_use]
pub fn version_string() -> String {
    format!(
        "NOMe_filtering_rs {} ({}/{})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}
```
*(Declare `pub mod cli/filename/substr` now but create the files in their tasks; until then comment them out or create empty stubs so the crate compiles. Recommended: create each file with just `//! TODO` + an empty `#[cfg(test)] mod tests {}` so `lib.rs` compiles from T1.)*

`src/main.rs` (Phase A: version only; `run()` wired in T6):
```rust
//! Binary entry point for `NOMe_filtering_rs`.
use std::process::ExitCode;
use clap::Parser;
use bismark_nome_filtering::cli::Cli;
use bismark_nome_filtering::version_string;

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.version {
        println!("{}", version_string());
        return ExitCode::SUCCESS;
    }
    // T6 wires: match bismark_nome_filtering::run(cli) { ... }
    ExitCode::SUCCESS
}
```
*(`main.rs` references `Cli` — create the minimal `cli.rs` in T1 stub with at least a `version: bool` field, OR sequence T5 before wiring main. Recommended: T1 ships a minimal `Cli` with the `version` flag so the binary builds; T5 fleshes it out.)*

`src/error.rs` (skeleton):
```rust
//! Typed errors for `bismark-nome-filtering`.
use bismark_io::genome::GenomeError;

/// All errors raised by the NOMe-filtering orchestration layer.
#[derive(Debug, thiserror::Error)]
pub enum BismarkNomeError {
    /// Genome load failure (promoted `bismark_io::genome`).
    #[error(transparent)]
    Genome(#[from] GenomeError),
    /// `--genome_folder` not supplied.
    #[error("Please specify a genome folder to proceed (full path only)")]
    MissingGenomeFolder,
    /// Positional input file does not exist.
    #[error("File did not exist in the current directory.")]
    InfileNotFound,
    /// Input yielded zero data lines (D4: raised AFTER the header is written).
    #[error("No last read was defined (input empty or all-header)")]
    EmptyInput,
    /// `--merge_CpGs` combined with `--CX` (the one reachable Perl die).
    #[error("Merging individual CpG calls into a single CpG dinucleotide entity is currently only supported if CpG-context is selected only (lose the option --CX)")]
    MergeCpgsWithCx,
    /// Direct I/O error (yacht read / output write).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
```
*(This references `bismark_io::genome::GenomeError` — so T2 must land for `error.rs` to compile. Order: do T2 before T6; for T1 you may temporarily stub `error.rs` without the `Genome` variant, then add it after T2. Note the dependency in the commit.)*

**Step 4: Run, confirm it passes**
```bash
cargo build --workspace                       # dangerouslyDisableSandbox: true
cargo test -p bismark-nome-filtering --lib    # dangerouslyDisableSandbox: true
cargo run -p bismark-nome-filtering --bin NOMe_filtering_rs -- --version   # prints the provenance line
```

**Step 5: Refactor** — No refactor needed.

**Step 6: Regression** — `cargo build --workspace` (the no-bump check belongs to T2/Final).

---

## Task 2 — Promote `bismark_io::genome` (module-local `GenomeError`, tier-parameterized)

**Files:**
- Modify: `rust/bismark-io/Cargo.toml` — add `flate2 = "=1.1.9"` to `[dependencies]` (match c2c's pin; verify `rust/bismark-coverage2cytosine/Cargo.toml`). **Do NOT change `version = "1.0.0-beta.8"`.**
- New: `rust/bismark-io/src/genome.rs`
- Modify: `rust/bismark-io/src/lib.rs` — add `pub mod genome;` and `pub use genome::{Genome, GenomeError};`

**Step 1: Write the failing tests** (`genome.rs` test module — adapt c2c's `genome.rs` tests; the load signature gains a `tiers` arg). Key cases:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn write(dir: &Path, name: &str, content: &str) { std::fs::write(dir.join(name), content).unwrap(); }

    #[test]
    fn loads_multifasta_first_token_name_and_uppercases() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "g.fa", ">chr1 some description\nacgt\nACGT\n>chr2\nGGCC\n");
        let g = Genome::load(t.path(), &[".fa", ".fasta"]).unwrap();
        assert_eq!(g.len(), 2);
        assert_eq!(g.get(b"chr1").unwrap(), b"ACGTACGT");
        assert_eq!(g.get(b"chr2").unwrap(), b"GGCC");
    }

    #[test]
    fn two_plain_tiers_fa_beats_fasta_no_union() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "a.fa", ">chr1\nACGT\n");
        write(t.path(), "b.fasta", ">chrZ\nTTTT\n");   // wrong tier, never read
        let g = Genome::load(t.path(), &[".fa", ".fasta"]).unwrap();
        assert!(g.contains(b"chr1") && !g.contains(b"chrZ"));
    }

    #[test]
    fn fa_gz_invisible_with_two_plain_tiers() {
        // SPEC P14: a .fa.gz-only genome dies with NoGenomeFasta when tiers
        // are the two PLAIN suffixes (the intended, Perl-faithful footgun).
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "g.fa.gz", "irrelevant");
        assert!(matches!(
            Genome::load(t.path(), &[".fa", ".fasta"]).unwrap_err(),
            GenomeError::NoGenomeFasta { .. }
        ));
    }

    #[test]
    fn fasta_tier_used_when_no_fa() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "g.fasta", ">chr1\nACGT\n");
        assert_eq!(Genome::load(t.path(), &[".fa", ".fasta"]).unwrap().get(b"chr1").unwrap(), b"ACGT");
    }

    #[test]
    fn mus_skip_uppercase_crlf_dupname_bareheader_u32() {
        // Mus-only → empty; CRLF stripped; dup-name errors; bare `>` errors.
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "Mus_musculus.NCBIM37.fa", ">chrM\nAAAA\n");
        write(t.path(), "chr1.fa", ">chr1\r\nAC\r\nGT\r\n");
        let g = Genome::load(t.path(), &[".fa", ".fasta"]).unwrap();
        assert_eq!(g.get(b"chr1").unwrap(), b"ACGT");
        assert!(!g.contains(b"chrM"));

        let t2 = tempfile::tempdir().unwrap();
        write(t2.path(), "a.fa", ">chr1\nAAAA\n");
        write(t2.path(), "b.fa", ">chr1\nGGGG\n");
        assert!(matches!(Genome::load(t2.path(), &[".fa"]).unwrap_err(), GenomeError::DuplicateChromosomeName { name } if name == "chr1"));

        let t3 = tempfile::tempdir().unwrap();
        write(t3.path(), "g.fa", ">\nACGT\n");
        assert!(matches!(Genome::load(t3.path(), &[".fa"]).unwrap_err(), GenomeError::MalformedFastaHeader { .. }));

        assert!(matches!(check_chr_len(b"big", (u32::MAX as usize)+1).unwrap_err(), GenomeError::ChromosomeTooLong { .. }));
    }

    #[test]
    fn dotfiles_not_matched() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), ".partial.fa", ">ghost\nACGT\n");
        write(t.path(), "real.fa", ">chr1\nACGT\n");
        let g = Genome::load(t.path(), &[".fa"]).unwrap();
        assert!(g.contains(b"chr1") && !g.contains(b"ghost"));
    }

    #[test]
    fn names_sorted_is_bytewise_and_no_insertion_iter() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "g.fa", ">chr10\nA\n>chr2\nC\n>chrX\nG\n");
        let g = Genome::load(t.path(), &[".fa"]).unwrap();
        assert_eq!(g.names_sorted(), vec![&b"chr10"[..], &b"chr2"[..], &b"chrX"[..]]);
    }
}
```

**Step 2: Run, confirm it fails**
```bash
cargo test -p bismark-io genome   # dangerouslyDisableSandbox: true
```
Expected failure: `genome` module / `Genome` / `GenomeError` unresolved.

**Step 3: Implement** — copy `rust/bismark-coverage2cytosine/src/genome.rs` into `rust/bismark-io/src/genome.rs` and make exactly these changes:
1. **Tier-parameterize**: replace the `const FASTA_TIERS: [&str;4]` + `Genome::load(folder)` with `Genome::load(folder: &Path, tiers: &[&str])`, and pass `tiers` into `discover_fasta_files(dir, tiers)`. The glob loop iterates the supplied `tiers` (first-non-empty wins; dotfile-exclude unchanged).
2. **Module-local error**: define `GenomeError` in this module (NOT `BismarkIoError`):
```rust
/// Errors from the Bismark whole-genome FASTA reader.
#[derive(Debug, thiserror::Error)]
pub enum GenomeError {
    /// No FASTA matched any supplied tier at the top level.
    #[error("genome folder {dir:?} contains no FASTA files for the requested suffixes")]
    NoGenomeFasta { /// the folder searched
                    dir: std::path::PathBuf },
    /// A present file in the winning tier has no valid FASTA header / zero records.
    #[error("malformed or empty FASTA (no `>` header): {file:?}")]
    MalformedFastaHeader { /// the offending file
                           file: std::path::PathBuf },
    /// Two chromosomes share a name.
    #[error("duplicate chromosome name: {name}")]
    DuplicateChromosomeName { /// the repeated name
                              name: String },
    /// A chromosome exceeds `u32::MAX` bases.
    #[error("chromosome {name} length {len} exceeds u32::MAX")]
    ChromosomeTooLong { /// name
                        name: String, /// length
                        len: usize },
    /// Underlying I/O failure.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
```
   Swap every `BismarkC2cError::X` → `GenomeError::X` and `-> Result<_, BismarkC2cError>` → `-> Result<_, GenomeError>`. The `noodles_fasta` InvalidData → `MalformedFastaHeader`, other io → `GenomeError::Io` mapping is unchanged.
3. Keep `flate2::read::MultiGzDecoder` gz support verbatim (so the module is a true general promotion; NOMe's plain tiers simply never exercise it). Keep `noodles-fasta`, the uppercase map, Mus skip, `check_chr_len`, `names_sorted`, the no-insertion-iterator invariant, and the module doc (update the glob-priority note to "tier-parameterized: first supplied tier with ≥1 match wins").
4. `lib.rs`: add `pub mod genome;` and `pub use genome::{Genome, GenomeError};`.

**Step 4: Run, confirm it passes**
```bash
cargo test -p bismark-io        # dangerouslyDisableSandbox: true
cargo build --workspace         # dangerouslyDisableSandbox: true — proves no-bump (P7)
```

**Step 5: Refactor** — Ensure no `BismarkC2cError` import leaked in; `genome.rs` must be self-contained (only `std`, `flate2`, `noodles_fasta`, `thiserror`).

**Step 6: Regression** — `cargo test -p bismark-io`.

---

## Task 3 — `perl_substr` helper + adversarial unit tests (§9)

**Files:** New: `rust/bismark-nome-filtering/src/substr.rs`

**Step 1: Write the failing tests**
```rust
#[cfg(test)]
mod tests {
    use super::perl_substr;
    const S: &[u8] = b"ABCDEFGH"; // L = 8
    #[test] fn negative_offset_in_range_returns_tail() { assert_eq!(perl_substr(S, -3, 3), b"FGH"); }
    #[test] fn negative_offset_beyond_len_is_empty()   { assert_eq!(perl_substr(S, -20, 3), b""); }
    #[test] fn over_length_truncates()                 { assert_eq!(perl_substr(S, 6, 5), b"GH"); }
    #[test] fn offset_past_end_is_empty()              { assert_eq!(perl_substr(S, 20, 3), b""); }
    #[test] fn offset_equals_len_is_empty_no_panic()   { assert_eq!(perl_substr(S, 8, 3), b""); }  // SPEC P1/A1
    #[test] fn interior_slice()                        { assert_eq!(perl_substr(S, 2, 3), b"CDE"); }
    #[test] fn zero_len_returns_empty()                { assert_eq!(perl_substr(S, 2, 0), b""); }
}
```

**Step 2: Run, confirm it fails** — `perl_substr` unresolved.
```bash
cargo test -p bismark-nome-filtering substr   # dangerouslyDisableSandbox: true
```

**Step 3: Implement**
```rust
//! Faithful reproduction of Perl's `substr(EXPR, OFFSET, LEN)` rvalue
//! semantics (SPEC §9). The sole caller passing a possibly-negative offset
//! is the reverse-read genome-window extraction (Phase B); all other calls
//! pass non-negative offsets.

/// Perl `substr` rvalue: negative offset counts from the end; an out-of-range
/// `start` (`<0` or `>L`) yields an empty slice (Perl returns undef → length 0);
/// `start == L` yields an empty slice (no panic); otherwise `min(len, L-start)`
/// bytes from `start`.
#[must_use]
pub fn perl_substr(s: &[u8], offset: isize, len: usize) -> &[u8] {
    let l = s.len() as isize;
    let start = if offset >= 0 { offset } else { l + offset };
    if start < 0 || start > l {
        return &[];
    }
    let start = start as usize;          // 0..=L
    let end = start.saturating_add(len).min(s.len());
    &s[start..end]                       // start==L → &s[L..L] == &[]
}
```

**Step 4: Run, confirm it passes**
```bash
cargo test -p bismark-nome-filtering substr   # dangerouslyDisableSandbox: true
```
**Step 5: Refactor** — No refactor needed. **Step 6:** `cargo test -p bismark-nome-filtering --lib`.

---

## Task 4 — `filename.rs` `derive_manowar_name` (§4, P15)

**Files:** New: `rust/bismark-nome-filtering/src/filename.rs`

**Step 1: Write the failing tests** (Perl `:464-468` + `:74-76`; NO directory strip)
```rust
#[cfg(test)]
mod tests {
    use super::derive_manowar_name;
    #[test] fn txt_gz()        { assert_eq!(derive_manowar_name("x.txt.gz"),  "x.manOwar.txt.gz"); }
    #[test] fn gz_only()       { assert_eq!(derive_manowar_name("x.gz"),      "x.manOwar.txt.gz"); }
    #[test] fn txt_only()      { assert_eq!(derive_manowar_name("x.txt"),     "x.manOwar.txt.gz"); }
    #[test] fn no_ext()        { assert_eq!(derive_manowar_name("x"),         "x.manOwar.txt.gz"); }
    #[test] fn gz_gz_single()  { assert_eq!(derive_manowar_name("x.gz.gz"),   "x.gz.manOwar.txt.gz"); }   // P15
    #[test] fn txt_txt_single(){ assert_eq!(derive_manowar_name("x.txt.txt"), "x.txt.manOwar.txt.gz"); }  // P15
    #[test] fn other_ext_kept(){ assert_eq!(derive_manowar_name("x.cov"),     "x.cov.manOwar.txt.gz"); }
}
```

**Step 2: Run, confirm it fails.**
```bash
cargo test -p bismark-nome-filtering filename   # dangerouslyDisableSandbox: true
```

**Step 3: Implement**
```rust
//! Output-filename derivation matching Perl `NOMe_filtering` (`:464-468`,
//! `:74-76`): strip ONE trailing `.gz`, then ONE trailing `.txt`, append
//! `.manOwar.txt`, then force `.gz`. NOTE: unlike `bismark-dedup`, Perl does
//! NOT strip the leading directory here, and only `.gz`/`.txt` are stripped
//! (each at most once) — do NOT reuse dedup's `.gz/.sam/.bam/.txt` loop.

/// Derive the `.manOwar.txt.gz` output filename from the raw infile string.
#[must_use]
pub fn derive_manowar_name(infile: &str) -> String {
    let mut s = infile.to_string();
    if let Some(t) = s.strip_suffix(".gz") { s = t.to_string(); }   // one .gz
    if let Some(t) = s.strip_suffix(".txt") { s = t.to_string(); }  // one .txt
    s.push_str(".manOwar.txt");                                     // append
    s.push_str(".gz");                                              // force .gz (never already .gz here)
    s
}
```

**Step 4–6:** `cargo test -p bismark-nome-filtering filename` (pass); no refactor; `--lib` regression.

---

## Task 5 — `cli.rs`: `Cli` + `validate()` + `ResolvedConfig` + `BismarkNomeError` finalize

**Files:**
- New/replace: `rust/bismark-nome-filtering/src/cli.rs`
- Confirm: `src/error.rs` carries the final variants (T1 skeleton + `Genome` from T2).

**Step 1: Write the failing tests** (mirror dedup's `cli.rs` test style)
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut full = vec!["NOMe_filtering_rs"]; full.extend(args.iter().copied());
        Cli::try_parse_from(full)
    }
    #[test] fn clap_def_valid() { Cli::command().debug_assert(); }

    #[test]
    fn merge_cpgs_with_cx_is_rejected() {
        let cli = parse(&["-g", "/g", "--merge_CpGs", "--CX", "in.txt"]).unwrap();
        assert!(matches!(cli.validate().unwrap_err(), BismarkNomeError::MergeCpgsWithCx));
    }
    #[test]
    fn missing_genome_folder_is_rejected() {
        let cli = parse(&["in.txt"]).unwrap();
        assert!(matches!(cli.validate().unwrap_err(), BismarkNomeError::MissingGenomeFolder));
    }
    #[test]
    fn inert_flags_accepted_no_effect() {
        let cli = parse(&["-g", "/g", "--zero_based", "--CX", "--GC", "--gzip", "--nome-seq", "in.txt"]).unwrap();
        let cfg = cli.validate().unwrap();
        assert_eq!(cfg.genome_folder, std::path::PathBuf::from("/g"));
    }
    #[test]
    fn dir_path_contract_resolves_input_and_output_under_dir() {
        let cli = parse(&["-g", "/g", "--dir", "/out", "sample.txt"]).unwrap();
        let cfg = cli.validate().unwrap();
        assert_eq!(cfg.input_path, std::path::PathBuf::from("/out/sample.txt"));
        assert_eq!(cfg.output_path, std::path::PathBuf::from("/out/sample.manOwar.txt.gz"));
    }
    #[test]
    fn no_dir_resolves_against_cwd_dot() {
        let cli = parse(&["-g", "/g", "sample.txt.gz"]).unwrap();
        let cfg = cli.validate().unwrap();
        assert_eq!(cfg.output_path.file_name().unwrap(), "sample.manOwar.txt.gz");
    }
    #[test] fn version_parses_without_infile() {
        let cli = parse(&["--version"]).unwrap();
        assert!(cli.version);
    }
}
```

**Step 2: Run, confirm it fails.**
```bash
cargo test -p bismark-nome-filtering cli   # dangerouslyDisableSandbox: true
```

**Step 3: Implement** `cli.rs`:
```rust
//! CLI for `NOMe_filtering_rs`. clap `Cli` → `validate()` → `ResolvedConfig`.
//! Live flags + Perl-faithful acceptance of inert flags + the one reachable die
//! (`--merge_CpGs` + `--CX`) + the `--dir` input/output path contract (SPEC §4).
use std::path::PathBuf;
use clap::Parser;
use crate::error::BismarkNomeError;
use crate::filename::derive_manowar_name;

#[derive(Parser, Debug)]
#[command(name = "NOMe_filtering_rs", about = "Per-read NOMe-Seq methylation filtering (Bismark)", long_about = None, disable_version_flag = true)]
pub struct Cli {
    /// Yacht input file (from `bismark_methylation_extractor --yacht`).
    pub infile: Option<PathBuf>,
    /// Genome FASTA folder (mandatory; full path).
    #[arg(short = 'g', long = "genome_folder")]
    pub genome_folder: Option<PathBuf>,
    /// Output directory; input AND output are resolved relative to it (SPEC §4).
    #[arg(long = "dir")]
    pub dir: Option<PathBuf>,
    /// Accepted for Perl compatibility; inert in the Rust port.
    #[arg(long = "parent_dir")]
    pub parent_dir: Option<PathBuf>,
    /// Inert (output is always gzipped).
    #[arg(long = "gzip")] pub gzip: bool,
    /// Inert (coords are always 1-based here).
    #[arg(long = "zero_based")] pub zero_based: bool,
    /// Inert; combined with `--merge_CpGs` it triggers the one reachable die.
    #[arg(long = "CX", visible_alias = "CX_context")] pub cx: bool,
    /// Inert (NOMe GC reporting is unconditional).
    #[arg(long = "GC", visible_alias = "GC_context")] pub gc: bool,
    /// Inert (NOMe filtering is unconditional; `$nome` defaults on in Perl).
    #[arg(long = "nome-seq")] pub nome_seq: bool,
    /// Inert alone; dies only with `--CX`.
    #[arg(long = "merge_CpGs")] pub merge_cpgs: bool,
    /// Print version and exit.
    #[arg(short = 'V', long = "version")] pub version: bool,
}

/// Resolved, validated configuration.
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// Genome FASTA folder.
    pub genome_folder: PathBuf,
    /// Yacht input resolved under `--dir` (SPEC §4).
    pub input_path: PathBuf,
    /// `.manOwar.txt.gz` output resolved under `--dir` (SPEC §4).
    pub output_path: PathBuf,
    /// Output directory (created if missing).
    pub output_dir: PathBuf,
}

impl Cli {
    /// Validate flag combinations and resolve paths (SPEC §4).
    pub fn validate(self) -> Result<ResolvedConfig, BismarkNomeError> {
        if self.merge_cpgs && self.cx {
            return Err(BismarkNomeError::MergeCpgsWithCx);
        }
        let genome_folder = self.genome_folder.ok_or(BismarkNomeError::MissingGenomeFolder)?;
        // No positional infile → handled in main (Perl prints help). validate
        // is only reached with an infile present.
        let infile = self.infile.ok_or(BismarkNomeError::InfileNotFound)?;
        let output_dir = self.dir.unwrap_or_else(|| PathBuf::from("."));
        // SPEC §4: input AND output resolved relative to --dir (no real chdir).
        let infile_str = infile.to_string_lossy();
        let input_path = output_dir.join(&infile);
        let output_path = output_dir.join(derive_manowar_name(&infile_str));
        let _ = (self.gzip, self.zero_based, self.cx, self.gc, self.nome_seq,
                 self.merge_cpgs, self.parent_dir); // inert — accepted, no effect
        Ok(ResolvedConfig { genome_folder, input_path, output_path, output_dir })
    }
}
```
*(`derive_manowar_name` operates on the infile's string; for a bare filename `sample.txt` it yields `sample.manOwar.txt.gz`. The `infile-exists` check is done in `run()` against `input_path` — T6 — matching Perl's `-e` on the resolved path.)*

**Step 4–6:** `cargo test -p bismark-nome-filtering cli` (pass); no refactor; `--lib`.

---

## Task 6 — `main.rs` wiring + `run()` (Phase-A: validate + genome load) + CLI integration tests

**Files:**
- Modify: `rust/bismark-nome-filtering/src/lib.rs` — add `run(cli: Cli) -> Result<(), BismarkNomeError>` (Phase-A scope).
- Modify: `rust/bismark-nome-filtering/src/main.rs` — wire `run`.
- New: `rust/bismark-nome-filtering/tests/cli_phase_a.rs` — `assert_cmd` integration tests.

**Step 1: Write the failing integration tests** (`tests/cli_phase_a.rs`)
```rust
use assert_cmd::Command;
use std::io::Write;

fn bin() -> Command { Command::cargo_bin("NOMe_filtering_rs").unwrap() }

#[test]
fn version_prints_provenance() {
    bin().arg("--version").assert().success()
        .stdout(predicates::str::starts_with("NOMe_filtering_rs "));
}

#[test]
fn merge_cpgs_with_cx_errors_nonzero() {
    bin().args(["-g", "/nonexistent", "--merge_CpGs", "--CX", "in.txt"])
        .assert().failure();
}

#[test]
fn missing_genome_errors_nonzero() {
    bin().arg("in.txt").assert().failure();
}

#[test]
fn valid_invocation_loads_genome_and_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    // tiny genome
    let gdir = dir.path().join("genome");
    std::fs::create_dir(&gdir).unwrap();
    std::fs::write(gdir.join("chr1.fa"), ">chr1\nACGTACGTACGT\n").unwrap();
    // empty-but-present yacht input under --dir (Phase A does not process it)
    let mut f = std::fs::File::create(dir.path().join("in.txt")).unwrap();
    writeln!(f, "Bismark methylation extractor header").unwrap();
    bin().args(["-g"]).arg(&gdir).args(["--dir"]).arg(dir.path()).arg("in.txt")
        .assert().success();
}
```

**Step 2: Run, confirm it fails** — `run` unresolved / main not wired.
```bash
cargo test -p bismark-nome-filtering --test cli_phase_a   # dangerouslyDisableSandbox: true
```

**Step 3: Implement** — add to `lib.rs`:
```rust
use crate::cli::Cli;
use crate::error::BismarkNomeError;

/// Phase A: validate, create the output dir, resolve paths, verify the input
/// exists, and load the genome. Per-read processing + output is Phase B.
pub fn run(cli: Cli) -> Result<(), BismarkNomeError> {
    let cfg = cli.validate()?;
    if !cfg.output_dir.as_os_str().is_empty() && cfg.output_dir != std::path::Path::new(".") {
        std::fs::create_dir_all(&cfg.output_dir)?;
    }
    if !cfg.input_path.exists() {
        return Err(BismarkNomeError::InfileNotFound);
    }
    let genome = bismark_io::genome::Genome::load(&cfg.genome_folder, &[".fa", ".fasta"])?;
    eprintln!("Stored sequence information of {} chromosomes/scaffolds in total", genome.len());
    // Phase B: per-read filtering + .manOwar.txt.gz output land here.
    Ok(())
}
```
and wire `main.rs`:
```rust
fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.version { println!("{}", version_string()); return ExitCode::SUCCESS; }
    match bismark_nome_filtering::run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => { eprintln!("error: {e}"); ExitCode::from(1) }
    }
}
```
*(`run` takes `Cli` and calls `validate()` internally — matches dedup. The `no-infile` case: `validate` maps `None` infile to `InfileNotFound`; acceptable for Phase A. If exact Perl help-on-no-arg is wanted, handle `cli.infile.is_none()` in `main` before `run` — note as an optional polish; help/exit text is NOT byte-gated per §2.)*

**Step 4: Run, confirm it passes**
```bash
cargo test -p bismark-nome-filtering --test cli_phase_a   # dangerouslyDisableSandbox: true
```

**Step 5: Refactor** — collapse any duplicated path logic; ensure `run` emits no output file (Phase A).

**Step 6: Regression** — full crate tests.

---

## Final verification

```bash
# all dangerouslyDisableSandbox: true
cargo build --workspace                              # P7: all 7 sibling =beta.8 pins still resolve
cargo test -p bismark-io                             # genome module green
cargo test -p bismark-nome-filtering                 # lib + cli_phase_a green
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p bismark-nome-filtering --bin NOMe_filtering_rs -- --version
cargo run -p bismark-nome-filtering --bin NOMe_filtering_rs -- --help
```
Expected: workspace builds; all tests pass; clippy clean; `--version` prints `NOMe_filtering_rs 0.1.0-beta.1 (<os>/<arch>)`.

## Commit plan

Two commits on `rust/nome-filtering` (commit only when Felix asks):
1. `feat(bismark-io): add tier-parameterized genome:: reader (additive, no version bump)` — `rust/bismark-io/{Cargo.toml,src/genome.rs,src/lib.rs}`.
2. `feat(nome-filtering): Phase A — crate scaffold, CLI, filename, perl_substr, errors` — `rust/Cargo.toml` (member) + `rust/bismark-nome-filtering/**`.

## Notes / decisions taken in this plan
- **perl_substr placed in Phase A** (`substr.rs`) as a self-contained, fully-unit-tested helper (it has no other deps); Phase B's `nome.rs` consumes it.
- **Genome module is gz-capable** (mirrors c2c verbatim) even though NOMe passes plain tiers — a true general promotion; the `.fa.gz` footgun (P14) is preserved by NOMe's *tier list*, not by crippling the reader. Adds `flate2` to `bismark-io` (additive).
- **`run()` is Phase-A-scoped** (validate + genome load, no output) — honest scaffold milestone; the SPEC Phase-A gate is "crate boots / `--help` / `--version` / genome loads." Per-read processing + `.manOwar.txt.gz` output is Phase B.
- **`error.rs`/`cli.rs` depend on T2** (`bismark_io::genome::GenomeError`) — sequence T2 before T6 (T1 may stub `error.rs` without the `Genome` variant to compile early).

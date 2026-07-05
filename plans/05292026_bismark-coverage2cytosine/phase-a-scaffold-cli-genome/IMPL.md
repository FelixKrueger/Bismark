# IMPL — Phase A (TDD task list)

**Source plan:** `phase-a-scaffold-cli-genome/PLAN.md` (rev 1). **Goal:** stand up the `bismark-coverage2cytosine` crate (lib+bin) with clap CLI/validation (incl. v1.x rejects + C1 context-conditional stem strip + C2 split dir defaults) and a `genome.rs` whole-genome FASTA reader reproducing Perl's quirks. No report algorithm (Phase B).

**Mode:** TDD (RED→GREEN→REFACTOR).
**Worktree:** all paths under `/Users/fkrueger/Github/Bismark-c2c`. Crate dir: `rust/bismark-coverage2cytosine/`.
**Command base:** run cargo from `/Users/fkrueger/Github/Bismark-c2c/rust`. Per-crate: `cargo test -p bismark-coverage2cytosine`. (If cwd differs, append `--manifest-path /Users/fkrueger/Github/Bismark-c2c/rust/Cargo.toml`.) Do NOT touch `rust/bismark-extractor` or `rust/bismark-bedgraph`.

## Test infrastructure
- **Unit tests:** `#[cfg(test)] mod tests` inline in `src/cli.rs` and `src/genome.rs`.
- **Integration tests:** `tests/sanity.rs` (binary via `assert_cmd::Command::cargo_bin("coverage2cytosine_rs")`), mirroring `bismark-dedup/tests/sanity.rs`.
- **Fixtures:** synthetic, built in-test with `tempfile::TempDir` + a local `write_fasta(dir, name, content)` helper (exactly like `bismark-io/src/cram_ref.rs` tests). No external test data.
- **Dev-deps:** `assert_cmd`, `predicates`, `tempfile`, `bstr`, `flate2` (for the BGZF/gzip fixture writers in V12/V12b).

## Plan coverage checklist

| # | Plan item | Source section | Task(s) |
|---|-----------|----------------|---------|
| 1 | Workspace member crate (lib+bin `coverage2cytosine_rs`) | §1, §2 | T1 |
| 2 | `version_string()` TG-style provenance; `--version`/`-V` | §3.1, §4 | T1 |
| 3 | `#![forbid(unsafe_code)]`, `#![warn(missing_docs)]` | §8.4 | T1 |
| 4 | `BismarkC2cError` enum (all Phase-A variants) | §5 | T2 |
| 5 | `Cli` struct — all flags incl. positional cov infile | §3.1 | T3 |
| 6 | `--CX_context` + `visible_alias CX`; `-CX` NOT a short | §3.1 (rev1) | T3 |
| 7 | v1.x flags declared but rejected (`--gc/--nome-seq/--drach/--ffs`) | §3.1, §3.2.2 | T3, T4 |
| 8 | `disable_version_flag`; clap `debug_assert` valid | §3.1 | T3 |
| 9 | Reject missing `-o` → `MissingOutput` | §3.2.3 | T4 |
| 10 | Reject missing `-g` → `MissingGenomeFolder` | §3.2.4 | T4 |
| 11 | `merge_cpgs`+`cx` / +`split` / +`threshold` mutexes | §3.2.5-7 | T4 |
| 12 | `discordance` without merge; range 1..=100 | §3.2.8-9 | T4 |
| 13 | `threshold == Some(0)` is error; `None`⇒0 report-all | §3.2.10 | T4 |
| 14 | `cpg_only = !cx_context` coupling | §3.2.11 | T5 |
| 15 | **C1** context-conditional `output_stem` strip | §3.2.11 (rev1) | T5 |
| 16 | **C2** `output_dir=""` vs `parent_dir=getcwd()`; `--dir`→abs+trailing `/` | §3.2.11 (rev1) | T5 |
| 17 | Genome load: multi-FASTA, name=first-token, uppercase | §3.3.3-5 | T6 |
| 18 | `Genome` API: get/contains/names_sorted/len; **no public order iterator** | §3.3 (rev1 invariant) | T6 |
| 19 | Four-suffix glob **priority** (first tier with ≥1 filename wins) | §3.3.1 | T7 |
| 20 | `Mus_musculus.NCBIM37.fa` skip; Mus-only tier⇒empty,no error | §3.3.2 | T7 |
| 21 | All-four-globs-empty ⇒ `NoGenomeFasta` | §3.3.1 | T7 |
| 22 | `.fa.gz`/`.fasta.gz` via `MultiGzDecoder` (plain + BGZF) | §3.3.3 (rev1) | T8 |
| 23 | Duplicate name (incl. cross-file) ⇒ `DuplicateChromosomeName` | §3.3.6 | T9 |
| 24 | Malformed/empty file in winning tier ⇒ `MalformedFastaHeader` | §3.3.2a (rev1) | T9 |
| 25 | CRLF handled (noodles auto-strip locked by test) | §3.3.5 (rev1) | T9 |
| 26 | Empty-sequence record kept | §3.3.5 | T9 |
| 27 | `u32` length overflow guard ⇒ `ChromosomeTooLong` | §3.3.7 | T9 |
| 28 | `--help` lists v1.0 flags | §9 V2 | T3 |
| 29 | Dep pins + add to workspace `members` | §2 | T1 |
| 30 | Final clippy/fmt clean + regression sweep | §9 V1 | T10 |

All 30 items map to ≥1 task. ✔

---

## Task 1 — Crate scaffold, workspace member, `--version` sanity

**Files:** `rust/Cargo.toml` (add member); new `rust/bismark-coverage2cytosine/Cargo.toml`, `src/lib.rs`, `src/main.rs`; `tests/sanity.rs`.

**Step 1 — RED** `tests/sanity.rs`:
```rust
use assert_cmd::Command;
use predicates::str::is_match;

#[test]
fn version_output_matches_provenance_regex() {
    Command::cargo_bin("coverage2cytosine_rs").unwrap()
        .arg("--version").assert().success()
        .stdout(is_match(r"^coverage2cytosine_rs \d+\.\d+\.\d+(-[\w.]+)? \(\S+/\S+\)\n$").unwrap());
}

#[test]
fn short_version_flag_works_too() {
    Command::cargo_bin("coverage2cytosine_rs").unwrap()
        .arg("-V").assert().success()
        .stdout(is_match(r"^coverage2cytosine_rs ").unwrap());
}
```

**Step 2 — confirm fail:** `cargo test -p bismark-coverage2cytosine` → fails: crate/bin does not exist.

**Step 3 — GREEN:**
- `rust/Cargo.toml`: add `"bismark-coverage2cytosine"` to `members`.
- `rust/bismark-coverage2cytosine/Cargo.toml`: `[package]` inheriting workspace fields; `[lib] path="src/lib.rs"`; `[[bin]] name="coverage2cytosine_rs" path="src/main.rs"`; `[dependencies]` `clap = { version="=4.5.30", features=["derive"] }`, `thiserror="=2.0.0"`, `noodles-fasta="=0.61.0"`, `noodles-core="=0.20.0"`, `flate2="=1.1.9"`; `[dev-dependencies]` `assert_cmd="=2.0.16"`, `predicates="=3.1.2"`, `tempfile="=3.10.1"`, `bstr="=1.10.0"`.
- `src/lib.rs`: crate docs; `#![forbid(unsafe_code)]`, `#![warn(missing_docs)]`; `pub mod cli; pub mod error; pub mod genome;`; re-exports; `pub fn version_string() -> String` → `format!("coverage2cytosine_rs {} ({}/{})", env!("CARGO_PKG_VERSION"), OS, ARCH)`. (Modules can be near-empty stubs that later tasks fill; keep it compiling.)
- `src/main.rs`: `fn main() -> ExitCode`; parse `Cli`; if `cli.version` print `version_string()` and exit 0; else `cli.validate()` and (Phase A) print a "Phase A: not yet implemented past config+genome" or simply load genome and exit 0. Exit map: 0 ok / 1 `BismarkC2cError` / 2 clap parse.

**Step 4 — pass:** `cargo test -p bismark-coverage2cytosine` → version tests green.
**Step 5 — REFACTOR:** none.
**Step 6 — regression:** `cargo build -p bismark-coverage2cytosine`.

---

## Task 2 — `BismarkC2cError` enum

**Files:** `src/error.rs`.

**Step 1 — RED** (inline test in `error.rs`):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn error_display_strings_present() {
        assert!(BismarkC2cError::MissingOutput.to_string().contains("output"));
        assert!(BismarkC2cError::MergeCpgsWithCx.to_string().contains("CX"));
        assert!(BismarkC2cError::UnsupportedFlag { flag: "--nome-seq" }
            .to_string().contains("--nome-seq"));
    }
}
```

**Step 2 — fail:** type doesn't exist.

**Step 3 — GREEN:** `#[derive(Debug, thiserror::Error)] pub enum BismarkC2cError` with variants (per PLAN §5): `Io(#[from] std::io::Error)`, `MissingOutput`, `MissingGenomeFolder`, `UnsupportedFlag { flag: &'static str }`, `MergeCpgsWithCx`, `MergeCpgsWithSplit`, `MergeCpgsWithThreshold`, `DiscordanceWithoutMerge`, `DiscordanceOutOfRange { value: u8 }`, `ThresholdNotPositive`, `NoGenomeFasta { dir: PathBuf }`, `DuplicateChromosomeName { name: String }`, `ChromosomeTooLong { name: String, len: usize }`, `MalformedFastaHeader { file: PathBuf }`. `#[error("…")]` strings echo Perl `die` wording where it exists.

**Step 4 — pass.** **Step 6 — regression:** `cargo test -p bismark-coverage2cytosine`.

---

## Task 3 — `Cli` struct (parse, `--help`, `--CX` alias, clap valid)

**Files:** `src/cli.rs`.

**Step 1 — RED** (inline tests):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut full = vec!["coverage2cytosine_rs"]; full.extend(args.iter().copied());
        Cli::try_parse_from(full)
    }
    #[test] fn clap_definition_is_valid() { Cli::command().debug_assert(); }
    #[test] fn cx_long_and_alias_both_parse() {
        assert!(parse(&["-o","x","-g","gdir","--CX_context","in.cov"]).unwrap().cx_context);
        assert!(parse(&["-o","x","-g","gdir","--CX","in.cov"]).unwrap().cx_context);
    }
    #[test] fn dash_cx_is_not_a_valid_short() {
        // `-CX` must NOT set cx_context (clap would otherwise bundle shorts).
        assert!(parse(&["-o","x","-g","gdir","-CX","in.cov"]).is_err());
    }
    #[test] fn parses_positional_cov_infile() {
        let cli = parse(&["-o","x","-g","gdir","sample.bismark.cov.gz"]).unwrap();
        assert_eq!(cli.cov_infile, std::path::PathBuf::from("sample.bismark.cov.gz"));
    }
}
```
Integration (`tests/sanity.rs`):
```rust
#[test]
fn help_lists_v1_flags() {
    Command::cargo_bin("coverage2cytosine_rs").unwrap()
        .arg("--help").assert().success()
        .stdout(predicates::str::contains("--merge_CpGs")
            .and(predicates::str::contains("--CX_context"))
            .and(predicates::str::contains("--split_by_chromosome")));
}
```

**Step 2 — fail:** `Cli` undefined.

**Step 3 — GREEN:** `#[derive(Parser, Debug)] #[command(name="coverage2cytosine_rs", disable_version_flag=true, about=…)] pub struct Cli` with fields per PLAN §3.1 table: `cov_infile: PathBuf` (positional, required), `output: Option<String>` (`-o`/`--output`), `dir: Option<PathBuf>`, `genome_folder: Option<PathBuf>` (`-g`/`--genome_folder`), `parent_dir: Option<PathBuf>`, `zero_based: bool`, `cx_context: bool` (`#[arg(long="CX_context", visible_alias="CX")]`), `split_by_chromosome: bool`, `merge_cpgs: bool` (`long="merge_CpGs"`), `discordance: Option<u8>` (`long="discordance_filter"`), `threshold: Option<u32>` (`long="coverage_threshold", visible_alias="threshold"`), `gzip: bool`, `version: bool` (`-V`/`--version`), and v1.x flags `gc: bool`(`long="GC_context", visible_alias="GC"` + a separate `--gc`? — model `--gc`/`--GC_context`/`--GC` as Perl: `long="GC_context", visible_aliases=["GC","gc"]`), `nome_seq: bool`(`long="nome-seq"`), `drach: bool`(`long="drach", visible_alias="m6A"`), `ffs: bool`.

**Step 4 — pass.** **Step 6 — regression:** crate test suite.

---

## Task 4 — `validate()` rejections (mutexes, ranges, v1.x, required args)

**Files:** `src/cli.rs` (`impl Cli { fn validate }` + `ResolvedConfig`).

**Step 1 — RED** (inline tests) — one per rule, e.g.:
```rust
fn cli(args: &[&str]) -> Cli { parse(args).unwrap() }
#[test] fn rejects_v1x_flags() {
    for (f, frag) in [("--gc","--gc"),("--nome-seq","--nome-seq"),("--drach","--drach"),("--ffs","--ffs")] {
        let e = cli(&["-o","x","-g","g", f, "in.cov"]).validate().unwrap_err();
        assert!(matches!(e, BismarkC2cError::UnsupportedFlag { flag } if flag.contains(frag.trim_start_matches('-'))));
    }
}
#[test] fn rejects_missing_output() {
    let e = cli(&["-g","g","in.cov"]).validate().unwrap_err();
    assert!(matches!(e, BismarkC2cError::MissingOutput));
}
#[test] fn rejects_missing_genome() {
    let e = cli(&["-o","x","in.cov"]).validate().unwrap_err();
    assert!(matches!(e, BismarkC2cError::MissingGenomeFolder));
}
#[test] fn rejects_merge_with_cx() {
    let e = cli(&["-o","x","-g","g","--merge_CpGs","--CX","in.cov"]).validate().unwrap_err();
    assert!(matches!(e, BismarkC2cError::MergeCpgsWithCx));
}
#[test] fn rejects_merge_with_split() { /* MergeCpgsWithSplit */ }
#[test] fn rejects_merge_with_threshold() { /* MergeCpgsWithThreshold */ }
#[test] fn rejects_discordance_without_merge() { /* DiscordanceWithoutMerge */ }
#[test] fn rejects_discordance_out_of_range() {
    for v in ["0","101"] {
        let e = cli(&["-o","x","-g","g","--merge_CpGs","--discordance_filter",v,"in.cov"]).validate().unwrap_err();
        assert!(matches!(e, BismarkC2cError::DiscordanceOutOfRange { .. }));
    }
}
#[test] fn rejects_threshold_zero() {
    let e = cli(&["-o","x","-g","g","--coverage_threshold","0","in.cov"]).validate().unwrap_err();
    assert!(matches!(e, BismarkC2cError::ThresholdNotPositive));
}
```

**Step 2 — fail:** `validate` undefined.

**Step 3 — GREEN:** `pub fn validate(self) -> Result<ResolvedConfig, BismarkC2cError>` implementing PLAN §3.2 rejections **in order**: v1.x flags (gc→nome→drach→ffs, return `UnsupportedFlag`); missing output; missing genome; merge+cx; merge+split; merge+threshold; discordance-without-merge; discordance range `!(1..=100)`; threshold `Some(0)`. (Resolution logic added in T5.)

**Step 4 — pass.** **Step 6 — regression.**

---

## Task 5 — `validate()` resolution (C1 stem strip, C2 dir defaults, cpg_only)

**Files:** `src/cli.rs` (resolution tail of `validate` + `ResolvedConfig` fields).

**Step 1 — RED:**
```rust
#[test] fn cpg_only_coupling() {
    assert!(cli(&["-o","x","-g","g","in.cov"]).validate().unwrap().cpg_only);
    assert!(!cli(&["-o","x","-g","g","--CX","in.cov"]).validate().unwrap().cpg_only);
}
#[test] fn output_stem_strip_is_context_conditional() {
    let stem = |a:&[&str]| cli(a).validate().unwrap().output_stem;
    // default (CpG): strip .CpG_report.txt only
    assert_eq!(stem(&["-o","foo.CpG_report.txt","-g","g","in.cov"]), "foo");
    // default mode + .CX_report.txt: NOT stripped
    assert_eq!(stem(&["-o","foo.CX_report.txt","-g","g","in.cov"]), "foo.CX_report.txt");
    // --CX: strip .CX_report.txt
    assert_eq!(stem(&["-o","foo.CX_report.txt","-g","g","--CX","in.cov"]), "foo");
    // --CX + .CpG_report.txt: NOT stripped
    assert_eq!(stem(&["-o","foo.CpG_report.txt","-g","g","--CX","in.cov"]), "foo.CpG_report.txt");
    // plain
    assert_eq!(stem(&["-o","foo","-g","g","in.cov"]), "foo");
}
#[test] fn dir_defaults_split() {
    let c = cli(&["-o","x","-g","g","in.cov"]).validate().unwrap();
    assert_eq!(c.output_dir, "");                       // empty prefix
    assert_eq!(c.parent_dir, std::env::current_dir().unwrap()); // getcwd()
}
#[test] fn threshold_none_defaults_zero() {
    assert_eq!(cli(&["-o","x","-g","g","in.cov"]).validate().unwrap().threshold, 0);
}
```

**Step 2 — fail.**

**Step 3 — GREEN:** append to `validate`: `cpg_only = !cx_context`; `threshold = threshold.unwrap_or(0)`; **context-conditional stem strip** (`if cx_context { strip ".CX_report.txt" } else { strip ".CpG_report.txt" }` — `strip_suffix`, keep original if no match); `output_dir`: `dir.map(make_absolute_with_trailing_slash).unwrap_or_default()` (default `String::new()`); `parent_dir`: `parent_dir.unwrap_or(std::env::current_dir()?)`. Define `ResolvedConfig { cov_infile, output_stem: String, output_dir: String, parent_dir: PathBuf, genome_folder: PathBuf, cpg_only, cx_context, zero_based, split_by_chromosome, threshold: u32, gzip, merge_cpgs, discordance: Option<u8> }`.

**Step 4 — pass.** **Step 5 — REFACTOR:** extract the stem-strip + dir-resolve helpers if `validate` gets long. **Step 6 — regression.**

---

## Task 6 — `genome.rs` reader: multi-FASTA, names, uppercase, accessors

**Files:** `src/genome.rs`.

**Step 1 — RED** (inline; local `write_fasta` helper + `tempfile`):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn write(dir:&Path, name:&str, c:&str){ std::fs::write(dir.join(name), c).unwrap(); }

    #[test] fn loads_multifasta_first_token_name_and_uppercases() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "g.fa", ">chr1 some description\nacgt\nACGT\n>chr2\nGGCC\n");
        let g = Genome::load(t.path()).unwrap();
        assert_eq!(g.len(), 2);
        assert_eq!(g.get(b"chr1").unwrap(), b"ACGTACGT");      // uppercased + joined
        assert!(g.get(b"chr2").is_some());
        assert!(g.contains(b"chr1") && !g.contains(b"chrZ"));
    }
    #[test] fn names_sorted_is_bytewise() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "g.fa", ">chr10\nA\n>chr2\nC\n>chrX\nG\n");
        let g = Genome::load(t.path()).unwrap();
        assert_eq!(g.names_sorted(), vec![&b"chr10"[..], &b"chr2"[..], &b"chrX"[..]]);
    }
}
```

**Step 2 — fail.**

**Step 3 — GREEN:** `pub struct Genome { chromosomes: HashMap<Vec<u8>, Vec<u8>> }` with `load`, `get`, `contains`, `names_sorted` (collect keys, `sort()`, return `Vec<&[u8]>`), `len`, `is_empty`. **Do NOT add `iter()`/`keys()` passthroughs** (invariant). `load` (this task: single plain `.fa` only — glob/gz/edge in T7-T9): pick `.fa` files, read each via noodles `fasta::io::Reader` over `BufReader<File>`, name = `record.name().to_vec()`, sequence = `record.sequence().as_ref()` uppercased (`to_ascii_uppercase` per byte).

**Step 4 — pass.** **Step 6 — regression.**

---

## Task 7 — glob priority + Mus skip + empty-dir error

**Files:** `src/genome.rs` (the file-discovery front of `load`).

**Step 1 — RED:**
```rust
#[test] fn glob_priority_fa_beats_fa_gz() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "chr.fa", ">chr1\nACGT\n");
    write(t.path(), "chr.fa.gz", "ignored-not-even-gzip");  // wrong tier; must be ignored
    let g = Genome::load(t.path()).unwrap();
    assert_eq!(g.len(), 1);
    assert_eq!(g.get(b"chr1").unwrap(), b"ACGT");
}
#[test] fn mus_only_tier_yields_empty_genome_no_error() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "Mus_musculus.NCBIM37.fa", ">chrM\nACGT\n");
    let g = Genome::load(t.path()).unwrap();   // NOT an error
    assert!(g.is_empty());
}
#[test] fn mus_skipped_among_others() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "Mus_musculus.NCBIM37.fa", ">chrM\nAAAA\n");
    write(t.path(), "chr1.fa", ">chr1\nACGT\n");
    let g = Genome::load(t.path()).unwrap();
    assert_eq!(g.len(), 1); assert!(g.contains(b"chr1") && !g.contains(b"chrM"));
}
#[test] fn no_fasta_anywhere_errors() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "readme.txt", "nope");
    assert!(matches!(Genome::load(t.path()).unwrap_err(), BismarkC2cError::NoGenomeFasta { .. }));
}
```

**Step 2 — fail.**

**Step 3 — GREEN:** add `discover_fasta_files(dir)`: read_dir top-level (no recursion); build tiers by case-... (Perl is literal `*.fa` etc. — match exact lowercase suffixes `.fa`,`.fa.gz`,`.fasta`,`.fasta.gz`); return the **first tier with ≥1 matching filename**; if all empty → `NoGenomeFasta { dir }`. Inside the per-file loop, `continue` if filename == `Mus_musculus.NCBIM37.fa`. (Within tier, sort filenames for determinism — order doesn't affect output but keeps tests stable.)

**Step 4 — pass.** **Step 6 — regression.**

---

## Task 8 — gz FASTA support (plain gzip + BGZF) via `MultiGzDecoder`

**Files:** `src/genome.rs` (open helper).

**Step 1 — RED:**
```rust
#[test] fn loads_plain_gzip_fa_gz() {
    use flate2::{write::GzEncoder, Compression};
    use std::io::Write;
    let t = tempfile::tempdir().unwrap();
    let mut e = GzEncoder::new(std::fs::File::create(t.path().join("g.fa.gz")).unwrap(), Compression::default());
    e.write_all(b">chrG\nacgtACGT\n").unwrap(); e.finish().unwrap();
    let g = Genome::load(t.path()).unwrap();
    assert_eq!(g.get(b"chrG").unwrap(), b"ACGTACGT");
}
#[test] fn loads_bgzf_fa_gz() {
    use std::io::Write;
    let t = tempfile::tempdir().unwrap();
    let mut w = noodles_bgzf::io::Writer::new(std::fs::File::create(t.path().join("g.fa.gz")).unwrap());
    w.write_all(b">chrB\nACGT\n").unwrap(); w.finish().unwrap();
    let g = Genome::load(t.path()).unwrap();
    assert_eq!(g.get(b"chrB").unwrap(), b"ACGT");
}
```
(Add `noodles-bgzf="=0.47.0"` to dev-deps for the BGZF fixture writer.)

**Step 2 — fail** (no `.fa.gz` tier wired, or noodles can't read plain gzip).

**Step 3 — GREEN:** open helper: if filename ends `.gz` → `flate2::read::MultiGzDecoder::new(File)`, else `File`; wrap in `BufReader`; feed `noodles_fasta::io::Reader::new`. `MultiGzDecoder` decodes both plain gzip and gzip-framed BGZF.

**Step 4 — pass.** **Step 6 — regression.**

---

## Task 9 — genome edge cases: dup name, malformed/empty file, CRLF, empty-seq, u32 guard

**Files:** `src/genome.rs`.

**Step 1 — RED:**
```rust
#[test] fn duplicate_name_cross_file_errors() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "a.fa", ">chr1\nAAAA\n");
    write(t.path(), "b.fa", ">chr1\nGGGG\n");
    assert!(matches!(Genome::load(t.path()).unwrap_err(),
        BismarkC2cError::DuplicateChromosomeName { name } if name == "chr1"));
}
#[test] fn malformed_or_empty_file_errors() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "bad.fa", "");                       // empty file in winning tier
    assert!(matches!(Genome::load(t.path()).unwrap_err(), BismarkC2cError::MalformedFastaHeader { .. }));
    let t2 = tempfile::tempdir().unwrap();
    write(t2.path(), "bad.fa", "no-header-line\nACGT\n");  // no `>`
    assert!(matches!(Genome::load(t2.path()).unwrap_err(), BismarkC2cError::MalformedFastaHeader { .. }));
}
#[test] fn crlf_sequence_has_no_carriage_return() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "g.fa", ">chr1\r\nAC\r\nGT\r\n");
    let g = Genome::load(t.path()).unwrap();
    assert_eq!(g.get(b"chr1").unwrap(), b"ACGT");          // no \r
    assert!(!g.get(b"chr1").unwrap().contains(&b'\r'));
}
#[test] fn empty_sequence_record_kept() {
    let t = tempfile::tempdir().unwrap();
    write(t.path(), "g.fa", ">chrEmpty\n>chr1\nACGT\n");
    let g = Genome::load(t.path()).unwrap();
    assert_eq!(g.get(b"chrEmpty").unwrap(), b"");
    assert_eq!(g.get(b"chr1").unwrap(), b"ACGT");
}
```
(`u32` guard: cannot allocate a >4 GiB fixture; cover by a unit test on the guard helper `check_len(name, len)` with `len = u32::MAX as usize + 1` → `ChromosomeTooLong`. Test the helper directly, not via a real file.)
```rust
#[test] fn u32_overflow_guard_helper() {
    assert!(matches!(check_chr_len("big", (u32::MAX as usize)+1).unwrap_err(),
        BismarkC2cError::ChromosomeTooLong { .. }));
    assert!(check_chr_len("ok", 1000).is_ok());
}
```

**Step 2 — fail.**

**Step 3 — GREEN:** in `load`: track a `HashSet` of seen names → `DuplicateChromosomeName` on collision; for each present file, if it yields **zero records** (noodles) → `MalformedFastaHeader { file }` (covers empty + headerless — verify noodles errors or yields none for a non-`>` first byte; if it errors, map that error to `MalformedFastaHeader`); rely on noodles for `\r` strip (test locks it); keep empty-sequence records (store empty `Vec`); `check_chr_len(name, len) -> Result<u32, _>` returns `ChromosomeTooLong` if `len > u32::MAX as usize`, else `len as u32` (call it after building each sequence; store seq regardless — the guard is about position arithmetic, so erroring is correct).

**Step 4 — pass.** **Step 5 — REFACTOR:** factor the per-file read into `read_one_fasta(path) -> Result<Vec<(Vec<u8>,Vec<u8>)>, _>`. **Step 6 — regression.**

---

## Task 10 — Final verification + integration sanity

**Step 1:** `cargo fmt -p bismark-coverage2cytosine` (or workspace fmt).
**Step 2:** `cargo clippy -p bismark-coverage2cytosine --all-targets -- -D warnings` → clean.
**Step 3:** full suite `cargo test -p bismark-coverage2cytosine` → all green.
**Step 4:** workspace sanity — `cargo build` from `rust/` (confirms the new member doesn't break the workspace; does NOT modify sibling crates).
**Step 5:** add a `tests/sanity.rs` check that a missing-`-o` invocation exits non-zero with a clear message (exit-code map).
**Step 6:** update `PLAN.md` implementation-notes + iteration log; flip PROGRESS Phase A → 🚧 Implementing (IMPL exists) then ✅ contingent on plan-manager.

## Final verification (suite)
```
cd /Users/fkrueger/Github/Bismark-c2c/rust
cargo fmt --all && cargo clippy -p bismark-coverage2cytosine --all-targets -- -D warnings
cargo test -p bismark-coverage2cytosine
cargo build      # workspace still builds with the new member
```
Expected: clippy clean; all unit + integration tests pass; workspace builds; sibling crates untouched (`git -C /Users/fkrueger/Github/Bismark-c2c status` shows only new `rust/bismark-coverage2cytosine/**` + `rust/Cargo.toml` + `rust/Cargo.lock`).

## Commit plan
Single commit on `rust/coverage2cytosine`:
```
feat(c2c): Phase A — scaffold + CLI/validation + genome reader

New crate bismark-coverage2cytosine (lib + bin coverage2cytosine_rs):
clap Cli/validate (all Perl process_commandline rules incl. v1.x rejects,
context-conditional output-stem strip, output_dir/parent_dir defaults);
genome.rs whole-genome FASTA reader (uppercase, Mus skip, four-suffix glob
priority, plain+BGZF gz, dup/malformed detection, u32 guard, no-public-
iterator Genome); BismarkC2cError. Byte-identity Phase A of epic
(coverage2cytosine v0.25.1 port).
```
Stage: `rust/bismark-coverage2cytosine/**`, `rust/Cargo.toml`, `rust/Cargo.lock`, the plan dir updates.
```

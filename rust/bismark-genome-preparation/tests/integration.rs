//! Integration tests for `bismark_genome_preparation_rs`.
//!
//! - Binary end-to-end with a **fake indexer** (`BISMARK_BIN`) so the run
//!   completes without a real `bowtie2-build` installed; asserts the converted
//!   CT/GA FASTA bytes.
//! - **Perl oracle**: runs the actual `bismark_genome_preparation` and the Rust
//!   binary on the same input and diffs the converted FASTA byte-for-byte.
//!   Auto-skips if `perl` is unavailable.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

/// Create a temp directory containing a fake `bowtie2-build` that exits 0,
/// so Step III "succeeds" without a real indexer. Returns the bin dir.
fn fake_indexer_dir(parent: &Path) -> PathBuf {
    let bin = parent.join("fakebin");
    fs::create_dir_all(&bin).unwrap();
    let script = bin.join("bowtie2-build");
    fs::write(&script, b"#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

/// Path to the repo-root Perl `bismark_genome_preparation` (two levels up from
/// the crate manifest: `rust/bismark-genome-preparation/` → repo root).
fn perl_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../legacy/bismark_genome_preparation")
        .canonicalize()
        .unwrap()
}

fn have_perl() -> bool {
    std::process::Command::new("perl")
        .arg("-e")
        .arg("1")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// CI sets `BISMARK_REQUIRE_PERL=1` so a "missing tool" turns the silent skip
/// into a hard failure — the point of issue #796. `== "1"` (not `is_some`) so an
/// accidental empty export doesn't trip local dev.
fn require_perl() -> bool {
    std::env::var("BISMARK_REQUIRE_PERL").as_deref() == Ok("1")
}

/// Skip the oracle (local dev without Perl/tools) or panic (CI, where a missing
/// tool means the byte-identity check would silently not run — see #796).
fn skip_or_panic(reason: &str) {
    if require_perl() {
        panic!("BISMARK_REQUIRE_PERL=1 but {reason}");
    }
    eprintln!("skipping: {reason}");
}

#[test]
fn binary_end_to_end_mfa_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let genome = tmp.path().join("genome");
    fs::create_dir_all(&genome).unwrap();
    // lowercase + ambiguity + a second record + final-no-newline.
    fs::write(
        genome.join("g.fa"),
        b">chr1 a description\nACGTacgtNRYK\nACGT\n>chr2\nGGGCCCttt",
    )
    .unwrap();
    let bin = fake_indexer_dir(tmp.path());

    Command::cargo_bin("bismark_genome_preparation")
        .unwrap()
        .env("BISMARK_BIN", &bin)
        .arg(&genome)
        .assert()
        .success();

    let ct = fs::read(genome.join("Bisulfite_Genome/CT_conversion/genome_mfa.CT_conversion.fa"))
        .unwrap();
    // chr1: ACGTacgtNRYK → uc ACGTACGTNRYK → ambiguity R,Y,K→N → ACGTACGTNNNN → C→T → ATGTATGTNNNN
    // chr2 (no trailing newline): GGGCCCttt → GGGCCCTTT → C→T → GGGTTTTTT
    assert_eq!(
        ct,
        b">chr1_CT_converted\nATGTATGTNNNN\nATGT\n>chr2_CT_converted\nGGGTTTTTT".to_vec()
    );

    let ga = fs::read(genome.join("Bisulfite_Genome/GA_conversion/genome_mfa.GA_conversion.fa"))
        .unwrap();
    // chr1: ACGTACGTNNNN → G→A → ACATACATNNNN ; chr2: GGGCCCTTT → AAACCCTTT
    assert_eq!(
        ga,
        b">chr1_GA_converted\nACATACATNNNN\nACAT\n>chr2_GA_converted\nAAACCCTTT".to_vec()
    );
}

#[test]
fn binary_combined_genome_is_ct_concat_ga() {
    let tmp = tempfile::tempdir().unwrap();
    let genome = tmp.path().join("genome");
    fs::create_dir_all(&genome).unwrap();
    fs::write(genome.join("a.fa"), b">chr1\nACGT\n").unwrap();
    fs::write(genome.join("b.fa"), b">chr2\nGGCC\n").unwrap();
    let bin = fake_indexer_dir(tmp.path());

    Command::cargo_bin("bismark_genome_preparation")
        .unwrap()
        .env("BISMARK_BIN", &bin)
        .arg("--combined_genome")
        .arg(&genome)
        .assert()
        .success();

    let bg = genome.join("Bisulfite_Genome");
    let mut expected = fs::read(bg.join("CT_conversion/genome_mfa.CT_conversion.fa")).unwrap();
    expected.extend(fs::read(bg.join("GA_conversion/genome_mfa.GA_conversion.fa")).unwrap());
    let combined = fs::read(bg.join("Combined/genome_mfa.combined.fa")).unwrap();
    assert_eq!(combined, expected);
}

#[test]
fn binary_genomic_composition_freq_table_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let genome = tmp.path().join("genome");
    fs::create_dir_all(&genome).unwrap();
    fs::write(genome.join("g.fa"), b">chr1\nACGT\n").unwrap();
    let bin = fake_indexer_dir(tmp.path());

    Command::cargo_bin("bismark_genome_preparation")
        .unwrap()
        .env("BISMARK_BIN", &bin)
        .arg("--genomic_composition")
        .arg(&genome)
        .assert()
        .success();

    // "ACGT" → mono A,C,G,T=1; di AC,CG,GT=1; byte-lexical (mono before its di).
    let freq = fs::read(genome.join("genomic_nucleotide_frequencies.txt")).unwrap();
    assert_eq!(
        freq,
        b"A\t1\nAC\t1\nC\t1\nCG\t1\nG\t1\nGT\t1\nT\t1\n".to_vec()
    );
}

/// Without `--genomic_composition`, the frequency table is NOT produced.
#[test]
fn binary_no_genomic_composition_flag_writes_no_table() {
    let tmp = tempfile::tempdir().unwrap();
    let genome = tmp.path().join("genome");
    fs::create_dir_all(&genome).unwrap();
    fs::write(genome.join("g.fa"), b">chr1\nACGT\n").unwrap();
    let bin = fake_indexer_dir(tmp.path());

    Command::cargo_bin("bismark_genome_preparation")
        .unwrap()
        .env("BISMARK_BIN", &bin)
        .arg(&genome)
        .assert()
        .success();

    assert!(
        !genome.join("genomic_nucleotide_frequencies.txt").exists(),
        "freq table must not exist without --genomic_composition"
    );
}

#[test]
fn binary_no_fasta_dir_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let genome = tmp.path().join("empty_genome");
    fs::create_dir_all(&genome).unwrap();
    let bin = fake_indexer_dir(tmp.path());
    Command::cargo_bin("bismark_genome_preparation")
        .unwrap()
        .env("BISMARK_BIN", &bin)
        .arg(&genome)
        .assert()
        .failure();
}

/// Run the Perl script and the Rust binary on copies of the same genome and
/// assert the converted CT + GA FASTA are byte-identical. Auto-skips if `perl`
/// is unavailable. Uses a fake `bowtie2-build` on PATH so both complete Step III.
#[test]
fn perl_vs_rust_byte_identical_mfa() {
    if !have_perl() {
        skip_or_panic("perl_vs_rust_byte_identical_mfa: perl not available");
        return;
    }
    let perl = perl_script();
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_indexer_dir(tmp.path());

    // A representative genome exercising: dropped header description, lowercase,
    // IUPAC ambiguity, multi-record, multi-file glob order, final-no-newline.
    let make_genome = |dir: &Path| {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("chr1.fa"),
            b">chr1 Homo sapiens chromosome 1\nACGTacgtNRYKMSWB\nTTTTCCCCGGGGAAAA\n",
        )
        .unwrap();
        fs::write(dir.join("chr10.fa"), b">chr10\nGGGCCCttt").unwrap();
        fs::write(dir.join("chr2.fa"), b">chr2\nacgtACGT\n").unwrap();
    };

    let perl_dir = tmp.path().join("perl_genome");
    let rust_dir = tmp.path().join("rust_genome");
    make_genome(&perl_dir);
    make_genome(&rust_dir);

    // Perl: bowtie2-build found via PATH (prepend the fake bin dir).
    let path_env = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let perl_status = std::process::Command::new("perl")
        .arg(&perl)
        .arg(&perl_dir)
        .env("PATH", &path_env)
        .status()
        .expect("failed to run perl");
    assert!(perl_status.success(), "perl genome prep failed");

    // Rust: fake indexer via BISMARK_BIN.
    Command::cargo_bin("bismark_genome_preparation")
        .unwrap()
        .env("BISMARK_BIN", &bin)
        .arg(&rust_dir)
        .assert()
        .success();

    for sub in [
        "Bisulfite_Genome/CT_conversion/genome_mfa.CT_conversion.fa",
        "Bisulfite_Genome/GA_conversion/genome_mfa.GA_conversion.fa",
    ] {
        let p = fs::read(perl_dir.join(sub)).unwrap();
        let r = fs::read(rust_dir.join(sub)).unwrap();
        assert_eq!(
            p,
            r,
            "byte mismatch vs Perl in {sub}\nperl={:?}\nrust={:?}",
            String::from_utf8_lossy(&p),
            String::from_utf8_lossy(&r)
        );
    }
}

/// Same oracle but in `--single_fasta` mode: compare each per-chromosome file.
#[test]
fn perl_vs_rust_byte_identical_single_fasta() {
    if !have_perl() {
        skip_or_panic("perl_vs_rust_byte_identical_single_fasta: perl not available");
        return;
    }
    let perl = perl_script();
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_indexer_dir(tmp.path());

    let make_genome = |dir: &Path| {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("chr1.fa"), b">chr1 desc\nACGTacgtNRYK\nGATTACA\n").unwrap();
        fs::write(dir.join("chr2.fa"), b">chr2\nTTTTGGGGCCCCAAAA").unwrap();
    };
    let perl_dir = tmp.path().join("perl_genome");
    let rust_dir = tmp.path().join("rust_genome");
    make_genome(&perl_dir);
    make_genome(&rust_dir);

    let path_env = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let perl_status = std::process::Command::new("perl")
        .arg(&perl)
        .arg("--single_fasta")
        .arg(&perl_dir)
        .env("PATH", &path_env)
        .status()
        .expect("failed to run perl");
    assert!(perl_status.success(), "perl genome prep failed");

    Command::cargo_bin("bismark_genome_preparation")
        .unwrap()
        .env("BISMARK_BIN", &bin)
        .arg("--single_fasta")
        .arg(&rust_dir)
        .assert()
        .success();

    for sub in [
        "Bisulfite_Genome/CT_conversion/chr1.CT_conversion.fa",
        "Bisulfite_Genome/CT_conversion/chr2.CT_conversion.fa",
        "Bisulfite_Genome/GA_conversion/chr1.GA_conversion.fa",
        "Bisulfite_Genome/GA_conversion/chr2.GA_conversion.fa",
    ] {
        let p = fs::read(perl_dir.join(sub)).unwrap();
        let r = fs::read(rust_dir.join(sub)).unwrap();
        assert_eq!(p, r, "byte mismatch vs Perl in {sub}");
    }
}

fn have_cmd(cmd: &str) -> bool {
    std::process::Command::new(cmd)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// `--path_to_aligner` is validated in Step I, BEFORE any conversion. A bad
/// path must fail and leave NO converted FASTA on disk (code-review / plan
/// item A4).
#[test]
fn bad_path_to_aligner_fails_before_conversion() {
    let tmp = tempfile::tempdir().unwrap();
    let genome = tmp.path().join("genome");
    fs::create_dir_all(&genome).unwrap();
    fs::write(genome.join("g.fa"), b">chr1\nACGT\n").unwrap();
    let badpath = tmp.path().join("no_such_aligner_dir");

    Command::cargo_bin("bismark_genome_preparation")
        .unwrap()
        .arg("--path_to_aligner")
        .arg(&badpath)
        .arg(&genome)
        .assert()
        .failure();

    // Validated before Step II → no converted output written.
    assert!(
        !genome
            .join("Bisulfite_Genome/CT_conversion/genome_mfa.CT_conversion.fa")
            .exists(),
        "conversion output must not exist when --path_to_aligner is bad"
    );
}

/// Helper: run Perl + Rust on copies of the same genome (created by `setup`),
/// with the given extra args, and assert each `rel` output file is byte-equal.
/// Auto-skips if `perl` is absent.
fn oracle_compare(setup: impl Fn(&Path), extra_args: &[&str], rel_files: &[&str]) {
    if !have_perl() {
        skip_or_panic("oracle test: perl not available");
        return;
    }
    let perl = perl_script();
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_indexer_dir(tmp.path());
    let perl_dir = tmp.path().join("perl_genome");
    let rust_dir = tmp.path().join("rust_genome");
    setup(&perl_dir);
    setup(&rust_dir);

    let path_env = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut pcmd = std::process::Command::new("perl");
    pcmd.arg(&perl);
    for a in extra_args {
        pcmd.arg(a);
    }
    pcmd.arg(&perl_dir).env("PATH", &path_env);
    assert!(pcmd.status().expect("run perl").success(), "perl failed");

    let mut rcmd = Command::cargo_bin("bismark_genome_preparation").unwrap();
    rcmd.env("BISMARK_BIN", &bin);
    for a in extra_args {
        rcmd.arg(a);
    }
    rcmd.arg(&rust_dir).assert().success();

    for rel in rel_files {
        let p = fs::read(perl_dir.join(rel)).unwrap();
        let r = fs::read(rust_dir.join(rel)).unwrap();
        assert_eq!(
            p,
            r,
            "byte mismatch vs Perl in {rel}\nperl={:?}\nrust={:?}",
            String::from_utf8_lossy(&p),
            String::from_utf8_lossy(&r)
        );
    }
}

/// Edge inputs in one run: CRLF record, zero-sequence record (header-at-EOF and
/// header→header), final-no-newline, CR-only file, lowercase + ambiguity.
#[test]
fn perl_vs_rust_edge_inputs_mfa() {
    oracle_compare(
        |dir| {
            fs::create_dir_all(dir).unwrap();
            // CRLF + lowercase + ambiguity codes (R,Y,K → N).
            fs::write(dir.join("a.fa"), b">chr1 desc\r\nACGTacgtNRYK\r\nTTTT\r\n").unwrap();
            // zero-sequence record (header at EOF).
            fs::write(dir.join("b.fa"), b">chrB\n").unwrap();
            // header→header (chrC empty) then final-no-newline (chrD).
            fs::write(dir.join("c.fa"), b">chrC\n>chrD\nGGGCCC").unwrap();
            // CR-only file (read as a single header line by both).
            fs::write(dir.join("d.fa"), b">chrE\rACGTACGT\r").unwrap();
        },
        &[],
        &[
            "Bisulfite_Genome/CT_conversion/genome_mfa.CT_conversion.fa",
            "Bisulfite_Genome/GA_conversion/genome_mfa.GA_conversion.fa",
        ],
    );
}

/// Mixed-case glob order vs real Perl. Perl `<*.fa>` (bundled `File::Glob`
/// csh_glob) case-folds on **both** Linux and macOS — confirmed on Linux CI:
/// `{chr1, Chr10, CHR2, Scaffold_a, scaffold_b}` → Perl folded order. Rust's
/// case-insensitive `fasta_name_cmp` matches on both platforms, so this runs
/// everywhere (it is the authoritative pin for the glob-order contract).
#[test]
fn perl_vs_rust_mixed_case_glob_order() {
    oracle_compare(
        |dir| {
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join("chr1.fa"), b">s_chr1\nAAAA\n").unwrap();
            fs::write(dir.join("Chr10.fa"), b">s_chr10\nCCCC\n").unwrap();
            fs::write(dir.join("CHR2.fa"), b">s_chr2\nGGGG\n").unwrap();
            fs::write(dir.join("Scaffold_a.fa"), b">s_sa\nTTTT\n").unwrap();
            fs::write(dir.join("scaffold_b.fa"), b">s_sb\nACGT\n").unwrap();
        },
        &[],
        &["Bisulfite_Genome/CT_conversion/genome_mfa.CT_conversion.fa"],
    );
}

/// `--slam` (T→C / A→G) with fixed `_CT_`/`_GA_` headers — vs real Perl.
#[test]
fn perl_vs_rust_slam() {
    oracle_compare(
        |dir| {
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join("g.fa"), b">chr1\nACGTacgtNRYK\nTTTTCCCCGGGGAAAA\n").unwrap();
        },
        &["--slam"],
        &[
            "Bisulfite_Genome/CT_conversion/genome_mfa.CT_conversion.fa",
            "Bisulfite_Genome/GA_conversion/genome_mfa.GA_conversion.fa",
        ],
    );
}

/// `--genomic_composition`: the mono/di-nucleotide frequency table vs real
/// Perl. Exercises lowercase (`uc`), IUPAC ambiguity codes (**counted**, NOT
/// mapped to `N` — unlike the conversion path), `N`-skipping, di across line
/// boundaries (within `chr1`), multi-record + multi-file (no di across
/// chromosomes/files), and a final line without a trailing newline.
///
/// `chr4.fa` additionally pins the `s/\r//`-first-only path and the high-byte
/// (`uc` vs `to_ascii_uppercase`) tail against **live Perl**: a CRLF header, a
/// double-`\r` line (first `\r` removed, second survives + counted), a stray
/// high byte (`0xc3 0xa9`, UTF-8 `é`, counted as its own keys), and a lowercase
/// `n` (uppercased to `N`, then skipped). Since `oracle_compare` diffs Perl's
/// output against Rust's, no hand-computed expectation is needed. The table
/// lands in the genome folder root, not `Bisulfite_Genome/`.
#[test]
fn perl_vs_rust_genomic_composition() {
    oracle_compare(
        |dir| {
            fs::create_dir_all(dir).unwrap();
            fs::write(
                dir.join("chr1.fa"),
                b">chr1 desc\nACGTacgtNRYK\nTTTTCCCCGGGG\n>chr2\nGATTACA\n",
            )
            .unwrap();
            fs::write(dir.join("chr3.fa"), b">chr3\nGGGCCCttt").unwrap();
            fs::write(
                dir.join("chr4.fa"),
                b">chr4\r\nAC\r\rGT\r\nGG\xc3\xa9CCnnTT\r\n",
            )
            .unwrap();
        },
        &["--genomic_composition"],
        &["genomic_nucleotide_frequencies.txt"],
    );
}

/// gzipped `.fa.gz` input (Perl `gunzip -c` vs Rust `MultiGzDecoder`).
#[test]
fn perl_vs_rust_gzip_input() {
    if !have_perl() || !have_cmd("gzip") {
        skip_or_panic("perl_vs_rust_gzip_input: perl or gzip not available");
        return;
    }
    oracle_compare(
        |dir| {
            fs::create_dir_all(dir).unwrap();
            let plain = dir.join("g.fa");
            fs::write(&plain, b">chr1 desc\nACGTacgtNRYK\nTTTTCCCC\n").unwrap();
            // gzip in place → g.fa.gz (removes g.fa), so only the .fa.gz group matches.
            let ok = std::process::Command::new("gzip")
                .arg(&plain)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            assert!(ok, "gzip failed");
        },
        &[],
        &[
            "Bisulfite_Genome/CT_conversion/genome_mfa.CT_conversion.fa",
            "Bisulfite_Genome/GA_conversion/genome_mfa.GA_conversion.fa",
        ],
    );
}

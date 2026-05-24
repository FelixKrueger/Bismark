//! Real-data byte-identity gate for `deduplicate_bismark_rs`.
//!
//! This is **the headline correctness test** for `bismark-dedup` v1.0:
//! given a real Bismark-aligned BAM, the Rust port must produce a
//! deduplicated BAM with the **same set of retained qnames** as
//! Bismark Perl v0.25.1, AND a deduplication report whose bytes
//! are exactly Perl's.
//!
//! ## Why `#[ignore]`?
//!
//! The dataset is ~1.35 GB and takes 1-3 minutes to dedup. We don't want
//! every `cargo test` invocation to run this — it's the "go for v1.x
//! release" gate, not the unit-test loop. Invoke explicitly with:
//!
//! ```sh
//! # v1.0 gate (single-threaded):
//! BISMARK_REAL_DATA_DIR=/path/to/dataset/dir \
//!   cargo test --release -- --ignored --exact byte_identity_real_data_10m_pe_wgbs
//!
//! # v1.1 gate (--parallel 4, BGZF-threaded path):
//! BISMARK_REAL_DATA_DIR=/path/to/dataset/dir \
//!   cargo test --release -- --ignored --exact byte_identity_real_data_10m_pe_wgbs_parallel_4
//! ```
//!
//! `--exact` matters: the v1.0 gate name is a prefix of the v1.1 one,
//! and `cargo test`'s default substring-match would run both.
//!
//! ## Dataset location
//!
//! Set the env var `BISMARK_REAL_DATA_DIR` to override; default is
//! `~/Desktop/TrimG_Bismark_test/profiling/` (Felix's local profiling
//! dataset). If the dataset is not present at the resolved path, the
//! test prints a clear skip message and returns success — non-Felix
//! developers / CI without the dataset see no false failures.
//!
//! ## What's the dataset?
//!
//! `SRR24827378_10M_R1_val_1_bismark_bt2_pe.bam` (~1.35 GB,
//! 8,592,524 records / 4,296,262 pairs / GRCh38 / directional WGBS).
//! Perl-Bismark v0.25.1's deduplicate_bismark output of this file is
//! the byte-identity ground truth.
//!
//! ## Path-string coupling (B-C3 from plan rev 2)
//!
//! Perl echoes `$ARGV[i]` verbatim in the dedup report — so the report
//! bytes include the input filename **as supplied on the CLI**. The
//! Perl baseline at `profiling/.deduplication_report.txt` was generated
//! with the basename `SRR24827378_10M_R1_val_1_bismark_bt2_pe.bam` (no
//! directory prefix). This test invokes the Rust binary with **the same
//! basename** by running it via `Command::current_dir(dataset_dir)`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

const DEFAULT_DATASET_DIR: &str = "/Users/fkrueger/Desktop/TrimG_Bismark_test/profiling";
const INPUT_BAM: &str = "SRR24827378_10M_R1_val_1_bismark_bt2_pe.bam";
const PERL_DEDUP_BAM: &str = "SRR24827378_10M_R1_val_1_bismark_bt2_pe.deduplicated.bam";
const PERL_DEDUP_REPORT: &str = "SRR24827378_10M_R1_val_1_bismark_bt2_pe.deduplication_report.txt";

fn resolve_dataset_dir() -> Option<PathBuf> {
    let dir = match std::env::var("BISMARK_REAL_DATA_DIR") {
        Ok(s) => PathBuf::from(s),
        Err(_) => PathBuf::from(DEFAULT_DATASET_DIR),
    };

    let input = dir.join(INPUT_BAM);
    let perl_bam = dir.join(PERL_DEDUP_BAM);
    let perl_report = dir.join(PERL_DEDUP_REPORT);

    if !input.exists() || !perl_bam.exists() || !perl_report.exists() {
        eprintln!(
            "SKIP: real-data byte-identity test — dataset incomplete at {}\n\
             Required files:\n\
             - {}: {}\n\
             - {}: {}\n\
             - {}: {}\n\
             Set BISMARK_REAL_DATA_DIR to override the default path.",
            dir.display(),
            INPUT_BAM,
            if input.exists() { "found" } else { "MISSING" },
            PERL_DEDUP_BAM,
            if perl_bam.exists() {
                "found"
            } else {
                "MISSING"
            },
            PERL_DEDUP_REPORT,
            if perl_report.exists() {
                "found"
            } else {
                "MISSING"
            },
        );
        return None;
    }
    Some(dir)
}

/// Read all qnames from a BAM file into a HashSet of owned Strings.
/// At 1.3 GB / ~8.6M records this is a ~30s op and a peak ~500 MB allocation.
fn read_qname_set(path: &Path) -> HashSet<String> {
    let mut reader =
        bismark_io::open_reader(path, None).expect("failed to open BAM for qname extraction");
    let mut set: HashSet<String> = HashSet::with_capacity(8_000_000);
    for record_result in reader.records() {
        let record = record_result.expect("BAM record decode failed");
        if let Some(name) = record.inner().name() {
            let qname = String::from_utf8_lossy(AsRef::as_ref(name)).into_owned();
            set.insert(qname);
        }
    }
    set
}

/// Shared body for the v1.0 (single-threaded) and v1.1 (`--parallel 4`)
/// byte-identity gates. Invoking with `parallel == None` runs without the
/// `--parallel` flag at all, which matches the v1.0 invocation byte-for-byte
/// (and is the safest control). Invoking with `parallel == Some(N)` adds
/// `--parallel N` to the command line.
///
/// In **both** invocations the assertion is the same: the Rust output's
/// retained-qname set and report bytes must equal the **single Perl
/// baseline** (Perl is single-threaded; the v1.1 contract is that
/// threading doesn't change observable output).
fn run_byte_identity_at_parallel(parallel: Option<u32>) {
    let Some(dataset_dir) = resolve_dataset_dir() else {
        // SKIP is graceful — print-and-return-success so CI without the
        // dataset doesn't see a spurious failure.
        return;
    };

    let perl_bam_path = dataset_dir.join(PERL_DEDUP_BAM);
    let perl_report_path = dataset_dir.join(PERL_DEDUP_REPORT);

    // Output to a temp dir so we don't clobber the existing Perl baseline.
    let out_dir = TempDir::new().expect("create tmp dir");

    eprintln!(
        "running bismark-dedup_rs against {} (dataset dir: {}, parallel: {:?})",
        INPUT_BAM,
        dataset_dir.display(),
        parallel,
    );

    // Run from the dataset dir with the basename so the report includes
    // exactly the same `file_label` as the Perl baseline.
    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.current_dir(&dataset_dir)
        .arg("--paired")
        .arg("--output_dir")
        .arg(out_dir.path());
    if let Some(n) = parallel {
        cmd.arg("--parallel").arg(n.to_string());
    }
    cmd.arg(INPUT_BAM).assert().success();

    // 1) Retained-qname set comparison.
    let rust_bam_path = out_dir
        .path()
        .join("SRR24827378_10M_R1_val_1_bismark_bt2_pe.deduplicated.bam");
    assert!(
        rust_bam_path.exists(),
        "Rust output BAM not produced at {}",
        rust_bam_path.display()
    );

    eprintln!(
        "reading qnames from Rust output ({})...",
        rust_bam_path.display()
    );
    let rust_qnames = read_qname_set(&rust_bam_path);

    eprintln!(
        "reading qnames from Perl baseline ({})...",
        perl_bam_path.display()
    );
    let perl_qnames = read_qname_set(&perl_bam_path);

    if rust_qnames != perl_qnames {
        let only_rust: Vec<&String> = rust_qnames.difference(&perl_qnames).take(5).collect();
        let only_perl: Vec<&String> = perl_qnames.difference(&rust_qnames).take(5).collect();
        panic!(
            "byte-identity FAIL (parallel={parallel:?}): retained-qname sets differ.\n\
             Rust: {} qnames\n\
             Perl: {} qnames\n\
             First 5 only-in-Rust: {:?}\n\
             First 5 only-in-Perl: {:?}",
            rust_qnames.len(),
            perl_qnames.len(),
            only_rust,
            only_perl,
        );
    }
    eprintln!(
        "✓ qname sets match: {} retained qnames each",
        rust_qnames.len()
    );

    // 2) Dedup report byte equality.
    let rust_report_path = out_dir
        .path()
        .join("SRR24827378_10M_R1_val_1_bismark_bt2_pe.deduplication_report.txt");
    assert!(
        rust_report_path.exists(),
        "Rust dedup report not produced at {}",
        rust_report_path.display()
    );

    let rust_report = std::fs::read_to_string(&rust_report_path).expect("read Rust report");
    let perl_report = std::fs::read_to_string(&perl_report_path).expect("read Perl report");

    if rust_report != perl_report {
        // Print a side-by-side diff for actionable debugging.
        eprintln!("--- Perl report (bytes={}) ---", perl_report.len());
        eprintln!("{perl_report}");
        eprintln!("--- Rust report (bytes={}) ---", rust_report.len());
        eprintln!("{rust_report}");
        panic!(
            "byte-identity FAIL (parallel={parallel:?}): dedup report bytes differ — \
             see eprintln above"
        );
    }
    eprintln!("✓ report bytes match: {} bytes each", rust_report.len());

    eprintln!(
        "byte-identity gate PASSED (parallel={parallel:?}): \
         {} retained qnames + {} report bytes",
        rust_qnames.len(),
        rust_report.len()
    );
}

/// THE byte-identity gate for `bismark-dedup` v1.0 (single-threaded).
///
/// Runs `deduplicate_bismark_rs --paired <basename>` from the dataset
/// directory (so the report includes the basename, matching Perl's
/// baseline). Compares:
///
/// 1. **Retained-qname set equality** — Rust's output BAM has exactly
///    the same set of qnames as Perl's.
///
/// 2. **Dedup report byte equality** — Rust's `.deduplication_report.txt`
///    is byte-equal to Perl's.
///
/// If either fails, the test prints a diff-style summary so the failure
/// is actionable without re-running.
#[test]
#[ignore]
fn byte_identity_real_data_10m_pe_wgbs() {
    run_byte_identity_at_parallel(None);
}

/// v1.1 headline gate: real-data byte-identity at `--parallel 4`.
///
/// Asserts that the BGZF-threaded BAM path produces **the same retained
/// qnames + the same report bytes** as Perl v0.25.1's single-threaded
/// `deduplicate_bismark`. Perl's output is the single baseline for both
/// the single-threaded Rust path and the `--parallel 4` Rust path — the
/// v1.1 byte-identity contract is that threading doesn't change anything
/// observable.
///
/// Run on **oxy** (where the 10M PE WGBS dataset + Perl baselines live):
///
/// ```sh
/// BISMARK_REAL_DATA_DIR=<oxy-dataset-dir> \
///   cargo test --release -- --ignored byte_identity_real_data_10m_pe_wgbs_parallel_4
/// ```
#[test]
#[ignore]
fn byte_identity_real_data_10m_pe_wgbs_parallel_4() {
    run_byte_identity_at_parallel(Some(4));
}

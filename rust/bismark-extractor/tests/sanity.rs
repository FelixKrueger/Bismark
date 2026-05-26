//! Phase A sanity tests.
//!
//! Spawns the `bismark-methylation-extractor-rs` binary with basic flags
//! and asserts the binary boots correctly + `--help` lists all 35 flags
//! + `--version` matches the provenance regex.

use assert_cmd::Command;
use predicates::prelude::*; // PredicateBooleanExt for `.and()`
use predicates::str::is_match;

#[test]
fn version_output_matches_provenance_regex() {
    let mut cmd = Command::cargo_bin("bismark-methylation-extractor-rs").unwrap();
    cmd.arg("--version").assert().success().stdout(
        is_match(r"^bismark-methylation-extractor-rs \d+\.\d+\.\d+(-[\w.]+)? \(\S+/\S+\)\n$")
            .unwrap(),
    );
}

#[test]
fn short_version_flag_works_too() {
    let mut cmd = Command::cargo_bin("bismark-methylation-extractor-rs").unwrap();
    cmd.arg("-V")
        .assert()
        .success()
        .stdout(is_match(r"^bismark-methylation-extractor-rs ").unwrap());
}

#[test]
fn invocation_with_no_input_files_errors_with_clear_message() {
    let mut cmd = Command::cargo_bin("bismark-methylation-extractor-rs").unwrap();
    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("no input file"));
}

/// All 35 flags must appear in the help output. We don't pin the exact
/// text (clap formatting may evolve), but we DO assert every flag name
/// is present so Phase A doesn't silently drop one.
#[test]
fn help_text_lists_all_35_flags() {
    let mut cmd = Command::cargo_bin("bismark-methylation-extractor-rs").unwrap();
    let output = cmd.arg("--help").assert().success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout).into_owned();

    let expected_flags = [
        "--single-end",
        "--paired-end",
        "--fasta",
        "--ignore",
        "--ignore_r2",
        "--ignore_3prime",
        "--ignore_3prime_r2",
        "--comprehensive",
        "--report",
        "--no_overlap",
        "--include_overlap",
        "--merge_non_CpG",
        "--output_dir",
        "--no_header",
        "--bedGraph",
        "--cutoff",
        "--remove_spaces",
        "--counts",
        "--cytosine_report",
        "--genome_folder",
        "--zero_based",
        "--CX",
        "--split_by_chromosome",
        "--buffer_size",
        "--samtools_path",
        "--gzip",
        "--mbias_only",
        "--mbias_off",
        "--gazillion",
        "--ample_memory",
        "--parallel",
        "--yacht",
        "--ucsc",
        "--version",
        "--help",
    ];
    assert_eq!(
        expected_flags.len(),
        35,
        "expected-flags array must have 35 entries (the SPEC §3 count)"
    );

    for flag in expected_flags {
        assert!(
            stdout.contains(flag),
            "--help missing flag {flag}; output was:\n{stdout}"
        );
    }
}

/// Placeholder note from main.rs should appear when a real input is
/// passed (validation passes but no pipeline exists yet).
#[test]
fn phase_a_placeholder_note_emitted_on_valid_invocation() {
    // Create a temp file so the validate(..InputFileNotFound..) branch
    // doesn't fire.
    let tmp = tempfile::Builder::new().suffix(".bam").tempfile().unwrap();
    std::fs::write(tmp.path(), b"x").unwrap();

    let mut cmd = Command::cargo_bin("bismark-methylation-extractor-rs").unwrap();
    cmd.arg(tmp.path()).assert().success().stderr(
        predicates::str::contains("Phase A (scaffold + CLI)")
            .and(predicates::str::contains("issues/798")),
    );
}

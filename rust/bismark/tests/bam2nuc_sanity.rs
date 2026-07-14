//! Phase A sanity checks: crate builds, version string + error Display work.

use assert_cmd::Command;
use bismark::bam2nuc::error::BismarkBam2nucError;
use bismark::bam2nuc::version_string;

#[test]
fn version_string_has_binary_name_and_platform() {
    let v = version_string();
    assert!(
        v.starts_with("bam2nuc (Bismark Rust suite) "),
        "version: {v}"
    );
    assert!(v.contains(std::env::consts::OS), "version omits OS: {v}");
}

#[test]
fn error_display_round_trips() {
    let e = BismarkBam2nucError::MissingGenomeFolder;
    assert!(e.to_string().contains("genome folder"));
}

#[test]
fn no_args_shows_help() {
    // No input is never a valid run, so a bare invocation renders the tool's help
    // (exit 2) via `cli::help_if_no_args` instead of a terse one-line error.
    Command::cargo_bin("bam2nuc")
        .unwrap()
        .assert()
        .code(2)
        .stderr(predicates::str::contains(
            "Calculate mono- and di-nucleotide coverage",
        ));
}

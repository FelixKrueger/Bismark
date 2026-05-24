//! Phase A sanity test.
//!
//! Spawns the `deduplicate_bismark_rs` binary with `--version` and asserts the
//! output matches the documented provenance regex. This is the minimum bar for
//! Phase A: the binary builds, runs, and emits a recognisable identity string.

use assert_cmd::Command;
use predicates::str::is_match;

#[test]
fn version_output_matches_provenance_regex() {
    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        // `deduplicate_bismark_rs <semver> (<os>/<arch>)`
        .stdout(is_match(r"^deduplicate_bismark_rs \d+\.\d+\.\d+ \(\S+/\S+\)\n$").unwrap());
}

#[test]
fn short_version_flag_works_too() {
    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.arg("-V")
        .assert()
        .success()
        .stdout(is_match(r"^deduplicate_bismark_rs ").unwrap());
}

#[test]
fn invocation_with_no_input_files_errors_with_clear_message() {
    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("no input file"));
}

#[test]
fn representative_flag_errors_with_perl_verbatim_joke() {
    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.arg("--representative")
        .arg("dummy.bam")
        .assert()
        .failure()
        .stderr(predicates::str::contains("Please stop wanting that"));
}

#[test]
fn barcode_flag_errors_with_v1_deferral_message() {
    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.arg("--barcode")
        .arg("dummy.bam")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "not supported in bismark-dedup v1.0",
        ));
}

//! Phase A integration sanity tests for `coverage2cytosine_rs`.
//!
//! Spawns the built binary and asserts the minimum Phase-A bar: it builds,
//! prints a recognisable `--version` provenance string, lists the v1.0 flags
//! in `--help`, and fails clearly on bad invocations.

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::is_match;

#[test]
fn version_output_matches_provenance_regex() {
    Command::cargo_bin("coverage2cytosine_rs")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(is_match(r"^coverage2cytosine_rs \d+\.\d+\.\d+(-[\w.]+)? \(\S+/\S+\)\n$").unwrap());
}

#[test]
fn short_version_flag_works_too() {
    Command::cargo_bin("coverage2cytosine_rs")
        .unwrap()
        .arg("-V")
        .assert()
        .success()
        .stdout(is_match(r"^coverage2cytosine_rs ").unwrap());
}

#[test]
fn help_lists_v1_flags() {
    Command::cargo_bin("coverage2cytosine_rs")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(
            predicates::str::contains("--merge_CpGs")
                .and(predicates::str::contains("--CX_context"))
                .and(predicates::str::contains("--split_by_chromosome")),
        );
}

#[test]
fn missing_output_fails_with_clear_message() {
    // validate() returns MissingOutput before any I/O.
    Command::cargo_bin("coverage2cytosine_rs")
        .unwrap()
        .arg("-g")
        .arg("genome_dir")
        .arg("in.bismark.cov")
        .assert()
        .failure()
        .code(1)
        .stderr(predicates::str::contains("output"));
}

#[test]
fn unsupported_v1x_flag_is_rejected() {
    // --gc / --nome-seq are now supported (Phase 1); --drach / --ffs stay rejected.
    Command::cargo_bin("coverage2cytosine_rs")
        .unwrap()
        .arg("-o")
        .arg("out")
        .arg("-g")
        .arg("genome_dir")
        .arg("--drach")
        .arg("in.bismark.cov")
        .assert()
        .failure()
        .code(1)
        .stderr(predicates::str::contains("not supported"));
}

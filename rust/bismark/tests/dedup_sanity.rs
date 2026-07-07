//! Phase A sanity test.
//!
//! Spawns the `deduplicate_bismark` binary with `--version` and asserts the
//! output matches the uniform suite provenance regex. This is the minimum bar for
//! Phase A: the binary builds, runs, and emits a recognisable identity string.

use assert_cmd::Command;
use predicates::prelude::*; // PredicateBooleanExt for `.not()` + `.and()`
use predicates::str::is_match;

#[test]
fn version_output_matches_provenance_regex() {
    let mut cmd = Command::cargo_bin("deduplicate_bismark").unwrap();
    cmd.arg("--version")
        .assert()
        .success()
        // Uniform suite one-liner:
        // `deduplicate_bismark (Bismark Rust suite) v<semver> (<hash> — <os>/<arch> — built <ts>)`
        // — canonical name (no `_rs`), suite version, provenance body.
        .stdout(
            is_match(
                r"^deduplicate_bismark \(Bismark Rust suite\) v\d+\.\d+\.\d+(-[\w.]+)? \(.+\)\n$",
            )
            .unwrap(),
        );
}

#[test]
fn short_version_flag_works_too() {
    let mut cmd = Command::cargo_bin("deduplicate_bismark").unwrap();
    cmd.arg("-V")
        .assert()
        .success()
        .stdout(is_match(r"^deduplicate_bismark \(Bismark Rust suite\) ").unwrap());
}

#[test]
fn invocation_with_no_input_files_errors_with_clear_message() {
    let mut cmd = Command::cargo_bin("deduplicate_bismark").unwrap();
    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("no input file"));
}

#[test]
fn representative_flag_errors_with_perl_verbatim_joke() {
    let mut cmd = Command::cargo_bin("deduplicate_bismark").unwrap();
    cmd.arg("--representative")
        .arg("dummy.bam")
        .assert()
        .failure()
        .stderr(predicates::str::contains("Please stop wanting that"));
}

/// Phase B (v1.2): `--barcode` engages UMI mode. The startup banner
/// matches Perl `deduplicate_bismark:167` byte-for-byte. (The previous
/// banner string was fabricated; dual code review C2/H1 caught it.)
///
/// Uses the Phase 0-bis 10K barcode fixture so the auto-detect (v1.2.1)
/// passes through and the banner can be observed.
#[test]
fn barcode_flag_emits_perl_line_167_startup_banner() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam");
    let tmp = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("deduplicate_bismark").unwrap();
    cmd.arg("--paired")
        .arg("--barcode")
        .arg("--output_dir")
        .arg(tmp.path())
        .arg(&fixture)
        .assert()
        .success()
        .stderr(predicates::str::contains("Deduplicating data in UMI mode"));
}

/// Phase B (v1.2): `--bclconvert` engages UMI mode with the bcl-convert
/// extractor. The startup banner matches Perl
/// `deduplicate_bismark:172` byte-for-byte.
#[test]
fn bclconvert_flag_emits_perl_line_172_startup_banner() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/dedup/synth_bclconvert_10k_R1_val_1_bismark_bt2_pe.bam");
    let tmp = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("deduplicate_bismark").unwrap();
    cmd.arg("--paired")
        .arg("--bclconvert")
        .arg("--output_dir")
        .arg(tmp.path())
        .arg(&fixture)
        .assert()
        .success()
        .stderr(predicates::str::contains(
            "Deduplicating data in bcl-convert UMI mode",
        ));
}

/// Negative case (Reviewer B M3): a non-UMI invocation must NOT emit
/// either UMI startup banner. Locks the conditional emission in `main.rs`.
#[test]
fn non_umi_invocation_does_not_emit_umi_startup_banner() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam");
    let tmp = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("deduplicate_bismark").unwrap();
    cmd.arg("--paired")
        .arg("--output_dir")
        .arg(tmp.path())
        .arg(&fixture)
        .assert()
        .success()
        .stderr(
            predicates::str::contains("Deduplicating data in UMI mode")
                .not()
                .and(predicates::str::contains("Deduplicating data in bcl-convert UMI mode").not()),
        );
}

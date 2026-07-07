//! Binary-level behaviors: exit codes, output naming, `-o`, and error paths
//! (PLAN D4 / §6.1). These do not need Perl.

use std::path::Path;

use assert_cmd::Command;
use tempfile::tempdir;

fn bin() -> Command {
    Command::cargo_bin("bismark2report").unwrap()
}

fn minimal_pe_report() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/report/minimal_pe/sampleD_PE_report.txt"),
    )
    .unwrap()
}

#[test]
fn version_exits_zero() {
    bin()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains("Bismark"));
}

#[test]
fn help_exits_zero() {
    bin().arg("--help").assert().success();
}

#[test]
fn man_exits_zero() {
    bin().arg("--man").assert().success();
}

#[test]
fn no_alignment_report_in_empty_dir_errors() {
    let dir = tempdir().unwrap();
    bin().current_dir(dir.path()).assert().failure();
}

#[test]
fn output_flag_with_multiple_reports_errors() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("one_PE_report.txt"), minimal_pe_report()).unwrap();
    std::fs::write(dir.path().join("two_PE_report.txt"), minimal_pe_report()).unwrap();
    bin()
        .current_dir(dir.path())
        .args(["-o", "single.html", "--__test_timestamp", "0"])
        .assert()
        .failure();
}

#[test]
fn derives_html_name_from_alignment_report() {
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("sampleD_PE_report.txt"),
        minimal_pe_report(),
    )
    .unwrap();
    bin()
        .current_dir(dir.path())
        .args([
            "--alignment_report",
            "sampleD_PE_report.txt",
            "--__test_timestamp",
            "0",
        ])
        .assert()
        .success();
    assert!(dir.path().join("sampleD_PE_report.html").exists());
}

#[test]
fn honors_explicit_output_name_and_dir() {
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("sampleD_PE_report.txt"),
        minimal_pe_report(),
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("out")).unwrap();
    bin()
        .current_dir(dir.path())
        .args([
            "--alignment_report",
            "sampleD_PE_report.txt",
            "-o",
            "custom_name.html",
            "--dir",
            "out/",
            "--__test_timestamp",
            "0",
        ])
        .assert()
        .success();
    assert!(dir.path().join("out/custom_name.html").exists());
}

#[test]
fn auto_detects_multiple_reports_produces_one_html_each() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("one_PE_report.txt"), minimal_pe_report()).unwrap();
    std::fs::write(dir.path().join("two_PE_report.txt"), minimal_pe_report()).unwrap();
    bin()
        .current_dir(dir.path())
        .args(["--__test_timestamp", "0"])
        .assert()
        .success();
    assert!(dir.path().join("one_PE_report.html").exists());
    assert!(dir.path().join("two_PE_report.html").exists());
}

#[test]
fn output_zero_is_perl_falsy_and_derives_name() {
    // Perl truthiness: `-o 0` is falsy → derive the name (NOT a file named "0").
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("sampleD_PE_report.txt"),
        minimal_pe_report(),
    )
    .unwrap();
    bin()
        .current_dir(dir.path())
        .args([
            "--alignment_report",
            "sampleD_PE_report.txt",
            "-o",
            "0",
            "--__test_timestamp",
            "0",
        ])
        .assert()
        .success();
    assert!(dir.path().join("sampleD_PE_report.html").exists());
    assert!(
        !dir.path().join("0").exists(),
        "must not write a file literally named 0"
    );
}

//! Phase A integration tests for the `NOMe_filtering_rs` binary.
//!
//! Covers the boot surface: `--version`, the `--merge_CpGs`+`--CX` die, the
//! mandatory-genome die, and a valid invocation that loads a tiny synthetic
//! genome and exits 0. Per-read processing + output is Phase B (not tested here
//! — Phase A's `run()` deliberately writes no output file).

use std::io::Write;

use assert_cmd::Command;

fn bin() -> Command {
    Command::cargo_bin("NOMe_filtering_rs").unwrap()
}

#[test]
fn version_prints_provenance() {
    bin()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::starts_with("NOMe_filtering_rs "));
}

#[test]
fn help_exits_successfully() {
    bin().arg("--help").assert().success();
}

#[test]
fn merge_cpgs_with_cx_errors_nonzero() {
    bin()
        .args(["-g", "/nonexistent", "--merge_CpGs", "--CX", "in.txt"])
        .assert()
        .failure();
}

#[test]
fn missing_genome_errors_nonzero() {
    bin().arg("in.txt").assert().failure();
}

#[test]
fn valid_invocation_produces_gzipped_report() {
    // Phase B: a valid invocation with a real (suitable) yacht read processes
    // the input and writes the always-gzipped `.manOwar.txt.gz` report.
    let dir = tempfile::tempdir().unwrap();

    let gdir = dir.path().join("genome");
    std::fs::create_dir(&gdir).unwrap();
    std::fs::write(gdir.join("chr1.fa"), ">chr1\nACGTACGTACGT\n").unwrap();

    // A `^Bismark` header line (skipped) + one suitable forward read
    // (start=4,end=8 on a 12 bp chr → passes the suitability guard).
    let mut f = std::fs::File::create(dir.path().join("in.txt")).unwrap();
    writeln!(f, "Bismark methylation extractor; version v0.25.1").unwrap();
    writeln!(f, "r1\t+\tchr1\t6\tz\t4\t8\t+").unwrap();

    bin()
        .args(["-g"])
        .arg(&gdir)
        .args(["--dir"])
        .arg(dir.path())
        .arg("in.txt")
        .assert()
        .success();

    // The derived output (`in.txt` → `in.manOwar.txt.gz`) must exist under --dir.
    assert!(dir.path().join("in.manOwar.txt.gz").exists());
}

#[test]
fn nonexistent_infile_errors_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let gdir = dir.path().join("genome");
    std::fs::create_dir(&gdir).unwrap();
    std::fs::write(gdir.join("chr1.fa"), ">chr1\nACGT\n").unwrap();

    // Genome is valid, but the input file does not exist under --dir.
    bin()
        .args(["-g"])
        .arg(&gdir)
        .args(["--dir"])
        .arg(dir.path())
        .arg("does_not_exist.txt")
        .assert()
        .failure();
}

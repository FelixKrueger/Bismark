//! CLI-behavior tests for the flags that don't shape the *content* of the
//! data streams (and so aren't covered by the byte-identity cells):
//! `--dir` (output placement + directory creation), the accepted-but-ignored
//! no-op flags (`--remove_spaces`, `--counts`, `--buffer_size`,
//! `--ample_memory`, `--gazillion`/`--scaffolds`), and the meta flags
//! (`--man`, `--version`, `--help`).

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use flate2::read::GzDecoder;
use predicates::prelude::*;
use tempfile::TempDir;

/// Minimal valid CpG input (the committed byte-identity fixtures).
const CPG: &[&str] = &["CpG_OT_s.txt", "CpG_OB_s.txt"];

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn gunzip(path: &Path) -> Vec<u8> {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut d = GzDecoder::new(&bytes[..]);
    let mut out = Vec::new();
    d.read_to_end(&mut out)
        .unwrap_or_else(|e| panic!("gunzip {}: {e}", path.display()));
    out
}

/// Copy the CpG fixtures into `workdir`, run the binary there with `extra`
/// flags + `-o out.bedGraph`, and return the decompressed (bedGraph, coverage)
/// read from `out_dir` (which is `workdir` unless `--dir` redirected it).
fn run_and_read(workdir: &Path, out_dir: &Path, extra: &[&str]) -> (Vec<u8>, Vec<u8>) {
    let fx = fixtures_dir();
    for f in CPG {
        fs::copy(fx.join(f), workdir.join(f)).unwrap();
    }
    let mut cmd = Command::cargo_bin("bismark2bedGraph_rs").unwrap();
    cmd.current_dir(workdir);
    cmd.args(extra);
    cmd.args(["-o", "out.bedGraph"]);
    cmd.args(CPG);
    cmd.assert().success();
    (
        gunzip(&out_dir.join("out.bedGraph.gz")),
        gunzip(&out_dir.join("out.bismark.cov.gz")),
    )
}

// ── --dir: output placement + directory creation ─────────────────────────

#[test]
fn dir_flag_writes_into_a_created_subdirectory() {
    let tmp = TempDir::new().unwrap();
    let out_dir = tmp.path().join("new").join("sub"); // does NOT exist yet
    assert!(!out_dir.exists(), "precondition: out dir must not exist");

    let (bg, cov) = run_and_read(
        tmp.path(),
        &out_dir,
        &["--dir", "new/sub"], // relative to current_dir (tmp)
    );

    assert!(out_dir.is_dir(), "--dir should create the directory");
    assert!(!bg.is_empty() && bg.starts_with(b"track type=bedGraph"));
    assert!(!cov.is_empty());

    // Content must match a plain run (no --dir) — placement only, not content.
    let base = TempDir::new().unwrap();
    let (bg0, cov0) = run_and_read(base.path(), base.path(), &[]);
    assert_eq!(bg, bg0, "--dir must not change bedGraph content");
    assert_eq!(cov, cov0, "--dir must not change coverage content");
}

// ── accepted-but-ignored flags are genuine no-ops on output ───────────────

#[test]
fn accepted_but_ignored_flags_do_not_change_output() {
    // Baseline: no extra flags.
    let base = TempDir::new().unwrap();
    let (bg0, cov0) = run_and_read(base.path(), base.path(), &[]);

    // Each flag is run individually (some are mutually exclusive, e.g.
    // --ample_memory vs --buffer_size) and must reproduce the baseline.
    let cases: &[&[&str]] = &[
        &["--remove_spaces"],
        &["--counts"],
        &["--buffer_size", "2G"],
        &["--ample_memory"],
        &["--gazillion"],
        &["--scaffolds"],
    ];
    for extra in cases {
        let tmp = TempDir::new().unwrap();
        let (bg, cov) = run_and_read(tmp.path(), tmp.path(), extra);
        assert_eq!(
            bg, bg0,
            "{extra:?} changed bedGraph output (should be a no-op)"
        );
        assert_eq!(
            cov, cov0,
            "{extra:?} changed coverage output (should be a no-op)"
        );
        // And the no-op flags must NOT leave stray intermediate files
        // (e.g. Perl's `.spaces_removed.txt` is deliberately not produced).
        let stray: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("spaces_removed"))
            .collect();
        assert!(stray.is_empty(), "{extra:?} left stray files: {stray:?}");
    }
}

// ── meta flags: --man / --version / --help ────────────────────────────────

#[test]
fn version_flag_prints_provenance_and_exits_zero() {
    Command::cargo_bin("bismark2bedGraph_rs")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("bismark2bedGraph_rs"));
}

#[test]
fn man_flag_prints_long_help_and_exits_zero() {
    Command::cargo_bin("bismark2bedGraph_rs")
        .unwrap()
        .arg("--man")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

#[test]
fn help_flag_smoke() {
    Command::cargo_bin("bismark2bedGraph_rs")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}

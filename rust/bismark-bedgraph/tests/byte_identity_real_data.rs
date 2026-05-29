//! Real-data byte-identity gate (Phase 6, `#[ignore]`).
//!
//! Runs Perl `bismark2bedGraph` v0.25.1 and `bismark2bedGraph_rs` on the
//! SAME methylation-extractor call files in the SAME argv order, and asserts
//! their **decompressed** bedGraph + coverage are byte-identical (SPEC §1.1
//! D1). This is the genuinely-independent-producer gate that unblocks
//! extractor sub-gate 2 (#798).
//!
//! Skipped unless the dataset dir is provided (so `cargo test --ignored`
//! is safe on a machine without the data / without Perl Bismark):
//!
//! ```sh
//! BISMARK_BEDGRAPH_REAL_DATA_DIR=/weka/.../CpG_dir \
//!   PERL_BG=~/miniforge3/envs/bioinf/bin/bismark2bedGraph \
//!   cargo test -p bismark-bedgraph --release -- --ignored byte_identity_real_data
//! ```
//!
//! On colossal (see memory `colossal-access`): use a distinct out-dir from
//! any other running session; the test writes only under `TempDir`.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use flate2::read::GzDecoder;
use tempfile::TempDir;

const RUST_BIN: &str = env!("CARGO_BIN_EXE_bismark2bedGraph_rs");

fn gunzip(path: &Path) -> Vec<u8> {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut d = GzDecoder::new(&bytes[..]);
    let mut out = Vec::new();
    d.read_to_end(&mut out)
        .unwrap_or_else(|e| panic!("gunzip {}: {e}", path.display()));
    out
}

/// Enumerate the input call files in `dir`, sorted (the SAME ordered list is
/// passed to both producers — SPEC C1).
fn input_files(dir: &Path, cx: bool) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| {
            n.starts_with("CpG_") || (cx && (n.starts_with("CHG_") || n.starts_with("CHH_")))
        })
        .filter(|n| n.ends_with(".txt") || n.ends_with(".txt.gz"))
        .collect();
    names.sort();
    names
}

fn run(prog: &str, dir: &Path, flags: &[&str], files: &[String]) {
    let mut cmd = Command::new(prog);
    cmd.current_dir(dir).env("LC_ALL", "C");
    cmd.args(flags).args(["-o", "out.bedGraph"]).args(files);
    let status = cmd.status().unwrap_or_else(|e| panic!("spawn {prog}: {e}"));
    assert!(status.success(), "{prog} exited with {status}");
}

fn gate(label: &str, flags: &[&str], cx: bool) {
    let Ok(data_dir) = std::env::var("BISMARK_BEDGRAPH_REAL_DATA_DIR") else {
        eprintln!("skipping {label}: set BISMARK_BEDGRAPH_REAL_DATA_DIR to run");
        return;
    };
    let data_dir = PathBuf::from(data_dir);
    let perl = std::env::var("PERL_BG").unwrap_or_else(|_| "bismark2bedGraph".to_string());

    let files = input_files(&data_dir, cx);
    assert!(
        !files.is_empty(),
        "no CpG_* input files in {}",
        data_dir.display()
    );

    let tmp = TempDir::new().unwrap();
    let p_dir = tmp.path().join("perl");
    let r_dir = tmp.path().join("rust");
    fs::create_dir_all(&p_dir).unwrap();
    fs::create_dir_all(&r_dir).unwrap();
    for f in &files {
        fs::copy(data_dir.join(f), p_dir.join(f)).unwrap();
        fs::copy(data_dir.join(f), r_dir.join(f)).unwrap();
    }

    run(&perl, &p_dir, flags, &files);
    run(RUST_BIN, &r_dir, flags, &files);

    for gz in ["out.bedGraph.gz", "out.bismark.cov.gz"] {
        let p = gunzip(&p_dir.join(gz));
        let r = gunzip(&r_dir.join(gz));
        assert!(
            p == r,
            "{label}: decompressed {gz} differs (perl {} bytes vs rust {} bytes)",
            p.len(),
            r.len()
        );
    }
    eprintln!(
        "{label}: byte-identical (decompressed) ✓  [{} input files]",
        files.len()
    );
}

#[test]
#[ignore = "requires BISMARK_BEDGRAPH_REAL_DATA_DIR + Perl bismark2bedGraph"]
fn byte_identity_real_data_default() {
    gate("real-data default (CpG)", &[], false);
}

#[test]
#[ignore = "requires BISMARK_BEDGRAPH_REAL_DATA_DIR + Perl bismark2bedGraph"]
fn byte_identity_real_data_cutoff5() {
    gate("real-data --cutoff 5", &["--cutoff", "5"], false);
}

#[test]
#[ignore = "requires BISMARK_BEDGRAPH_REAL_DATA_DIR + Perl bismark2bedGraph"]
fn byte_identity_real_data_cx() {
    gate("real-data --CX", &["--CX"], true);
}

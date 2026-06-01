//! Byte-identity gate: the generated HTML must be byte-for-byte identical to the
//! live Perl `bismark2report v0.25.1`, modulo the single `localtime` timestamp
//! line (normalized in both before comparison). The Perl script is the PRIMARY
//! oracle (mirror genomeprep/methcons). Auto-skips if `perl` is unavailable.
//!
//! Each case copies its fixture `*.txt` into a temp dir and runs BOTH tools with
//! that dir as the working directory (so companion auto-detection is exercised),
//! writing into separate `perl/` and `rust/` subdirs. The Rust run pins the
//! timestamp via `--__test_timestamp 0`; the Perl run uses `localtime`, so both
//! outputs are timestamp-normalized before the byte compare.

use std::path::{Path, PathBuf};
use std::process::Command;

fn perl_available() -> bool {
    Command::new("perl")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Replace the single `Data processed at HH:MM:SS on YYYY-MM-DD` span with a
/// fixed token. Asserts EXACTLY ONE occurrence (so a stray timestamp-shaped
/// string elsewhere can't silently mask a real divergence — SPEC §7).
fn normalize_ts(html: &[u8]) -> Vec<u8> {
    let key = b"Data processed at ";
    let occurrences = html.windows(key.len()).filter(|w| *w == key).count();
    assert_eq!(
        occurrences, 1,
        "expected exactly one timestamp line, found {occurrences}"
    );
    let p = html.windows(key.len()).position(|w| w == key).unwrap();
    let end_rel = html[p..]
        .windows(4)
        .position(|w| w == b"</p>")
        .expect("timestamp line should be closed by </p>");
    let end = p + end_rel;
    let mut out = Vec::with_capacity(html.len());
    out.extend_from_slice(&html[..p]);
    out.extend_from_slice(b"Data processed at TIME on DATE");
    out.extend_from_slice(&html[end..]);
    out
}

/// Run both tools with `work` as the working directory (auto-detecting the
/// `*E_report.txt` reports placed there) into `work/perl` and `work/rust`, then
/// assert every produced HTML is byte-identical after timestamp normalization.
fn run_both_and_compare(work: &Path, label: &str) {
    std::fs::create_dir_all(work.join("perl")).unwrap();
    std::fs::create_dir_all(work.join("rust")).unwrap();

    let perl_script = repo_root().join("bismark2report");
    let perl_status = Command::new("perl")
        .arg(&perl_script)
        .args(["--dir", "perl/"])
        .current_dir(work)
        .output()
        .expect("run perl bismark2report");
    assert!(
        perl_status.status.success(),
        "perl bismark2report failed for `{label}`"
    );

    let rust_status = Command::new(env!("CARGO_BIN_EXE_bismark2report_rs"))
        .args(["--dir", "rust/", "--__test_timestamp", "0"])
        .current_dir(work)
        .output()
        .expect("run bismark2report_rs");
    assert!(
        rust_status.status.success(),
        "bismark2report_rs failed for `{label}`"
    );

    let mut html_count = 0;
    for entry in std::fs::read_dir(work.join("perl")).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().map(|e| e == "html").unwrap_or(false) {
            let name = p.file_name().unwrap();
            let rust_html = work.join("rust").join(name);
            assert!(
                rust_html.exists(),
                "Rust did not produce {name:?} for `{label}`"
            );
            let perl_bytes = normalize_ts(&std::fs::read(&p).unwrap());
            let rust_bytes = normalize_ts(&std::fs::read(&rust_html).unwrap());
            assert!(
                perl_bytes == rust_bytes,
                "byte mismatch for {name:?} in `{label}` (perl {} vs rust {} normalized bytes)",
                perl_bytes.len(),
                rust_bytes.len()
            );
            html_count += 1;
        }
    }
    assert!(html_count > 0, "no HTML produced for `{label}`");
}

/// Copy a fixture case's `*.txt` into a temp dir and compare Perl vs Rust.
fn assert_case_byte_identical(case: &str) {
    if !perl_available() {
        eprintln!("skipping `{case}`: perl not available");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    let case_dir = fixtures_dir().join(case);
    for entry in std::fs::read_dir(&case_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().map(|e| e == "txt").unwrap_or(false) {
            std::fs::copy(&path, work.join(path.file_name().unwrap())).unwrap();
        }
    }
    run_both_and_compare(work, case);
}

#[test]
fn pe_full_companions_byte_identical() {
    // alignment + dedup + splitting + M-bias (PE, R1+R2) + nucleotide.
    assert_case_byte_identical("wgbs_pe");
}

#[test]
fn se_r1_only_mbias_byte_identical() {
    // SE: M-bias R1 only (→ R2 section excised, {{mbias2_*}} survive); other
    // companions absent (→ sections excised).
    assert_case_byte_identical("wgbs_se");
}

#[test]
fn nondirectional_unknown_context_byte_identical() {
    // Unknown-context <tr> inject snippets (alignment + splitting).
    assert_case_byte_identical("nondir_pe");
}

#[test]
fn minimal_alignment_only_byte_identical() {
    // No companions: dedup/splitting/nuc + both M-bias sections excised, and the
    // 24 {{mbias*}} script-block placeholders survive — matched against Perl.
    assert_case_byte_identical("minimal_pe");
}

#[test]
fn crlf_alignment_byte_identical() {
    // CRLF (Windows) line endings: regression for the `Bismark report for:`
    // version parse (code-review B). The trailing `\r` must not drop the
    // filename/version fields. Build the CRLF report from the LF fixture.
    if !perl_available() {
        eprintln!("skipping crlf: perl not available");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    let lf = std::fs::read(fixtures_dir().join("minimal_pe/sampleD_PE_report.txt")).unwrap();
    let mut crlf = Vec::with_capacity(lf.len());
    for &b in &lf {
        if b == b'\n' {
            crlf.push(b'\r');
        }
        crlf.push(b);
    }
    std::fs::write(work.join("crlf_PE_report.txt"), &crlf).unwrap();
    run_both_and_compare(work, "crlf");
}

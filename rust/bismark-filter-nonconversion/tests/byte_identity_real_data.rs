//! Real-data byte-identity gate for `filter_non_conversion_rs`.
//!
//! `#[ignore]`d + env-gated: runs the REAL Perl `filter_non_conversion`
//! v0.25.1 AND the Rust binary on a real Bismark BAM, then asserts the
//! decompressed kept/removed bodies (`samtools view`) and the report
//! (timing-line-normalized) are byte-identical. Mirrors the sibling crates'
//! real-data gates ([[reference_colossal_access]]).
//!
//! Runs on colossal/oxy where Perl Bismark v0.25.1 + samtools + the real
//! 10M SE/PE BAMs live. Set:
//!   FNC_PERL      = path to the Perl `filter_non_conversion` (v0.25.1)
//!   FNC_REAL_SE   = path to a 10M SE Bismark BAM   (runs the SE cells)
//!   FNC_REAL_PE   = path to a 10M PE Bismark BAM   (runs the PE cells)
//! Any unset var skips its cells (prints + returns success). Example:
//!
//! ```sh
//! FNC_PERL=~/Github/Bismark/filter_non_conversion \
//! FNC_REAL_SE=/weka/.../10M_SE/directional_10M_R1_val_1_bismark_bt2.bam \
//! FNC_REAL_PE=/weka/.../10M_PE/..._pe.deduplicated.bam \
//!   cargo test -p bismark-filter-nonconversion --release --test byte_identity_real_data \
//!     -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command as AssertCommand;
use tempfile::TempDir;

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.exists())
}

fn samtools_view_body(bam: &Path) -> Vec<u8> {
    let out = StdCommand::new("samtools")
        .arg("view")
        .arg(bam)
        .output()
        .expect("spawn samtools");
    assert!(
        out.status.success(),
        "samtools view {} failed",
        bam.display()
    );
    out.stdout
}

fn strip_timing(s: &str) -> &str {
    match s.find("filter_non_conversion completed in") {
        Some(i) => &s[..i],
        None => s,
    }
}

/// Run Perl + Rust on `real_bam` with `mode_flag` (`-s`/`-p`) + `extra` flags,
/// each in its own temp dir using the SAME basename, and assert byte-identity
/// of both output bodies + the (normalized) report.
fn compare_cell(perl: &Path, real_bam: &Path, mode_flag: &str, extra: &[&str], label: &str) {
    let basename = real_bam.file_name().unwrap().to_str().unwrap().to_string();
    let stem = basename
        .strip_suffix(".bam")
        .unwrap_or(&basename)
        .to_string();

    let perl_dir = TempDir::new().unwrap();
    let rust_dir = TempDir::new().unwrap();
    std::fs::copy(real_bam, perl_dir.path().join(&basename)).unwrap();
    std::fs::copy(real_bam, rust_dir.path().join(&basename)).unwrap();

    // Perl baseline.
    let mut perl_cmd = StdCommand::new("perl");
    perl_cmd
        .current_dir(perl_dir.path())
        .arg(perl)
        .arg(mode_flag);
    for f in extra {
        perl_cmd.arg(f);
    }
    let perl_status = perl_cmd.arg(&basename).status().expect("run Perl");
    assert!(perl_status.success(), "[{label}] Perl run failed");

    // Rust.
    let mut rust_cmd = AssertCommand::cargo_bin("filter_non_conversion_rs").unwrap();
    rust_cmd.current_dir(rust_dir.path()).arg(mode_flag);
    for f in extra {
        rust_cmd.arg(f);
    }
    rust_cmd.arg(&basename).assert().success();

    for suffix in ["nonCG_filtered.bam", "nonCG_removed_seqs.bam"] {
        let perl_body = samtools_view_body(&perl_dir.path().join(format!("{stem}.{suffix}")));
        let rust_body = samtools_view_body(&rust_dir.path().join(format!("{stem}.{suffix}")));
        assert!(
            perl_body == rust_body,
            "[{label}] {suffix} body differs ({} vs {} records-bytes)",
            perl_body.len(),
            rust_body.len()
        );
    }

    let perl_report = std::fs::read_to_string(
        perl_dir
            .path()
            .join(format!("{stem}.non-conversion_filtering.txt")),
    )
    .unwrap();
    let rust_report = std::fs::read_to_string(
        rust_dir
            .path()
            .join(format!("{stem}.non-conversion_filtering.txt")),
    )
    .unwrap();
    assert_eq!(
        strip_timing(&rust_report),
        strip_timing(&perl_report),
        "[{label}] report (pre-timing) differs"
    );
    eprintln!("[{label}] byte-identical ✓");
}

fn run_cells(mode_flag: &str, kind: &str, real_var: &str) {
    let Some(perl) = env_path("FNC_PERL") else {
        eprintln!("SKIP: FNC_PERL unset");
        return;
    };
    let Some(real_bam) = env_path(real_var) else {
        eprintln!("SKIP: {real_var} unset/missing — {kind} cells skipped");
        return;
    };
    compare_cell(&perl, &real_bam, mode_flag, &[], &format!("{kind}/default"));
    compare_cell(
        &perl,
        &real_bam,
        mode_flag,
        &["--threshold", "5"],
        &format!("{kind}/threshold5"),
    );
    compare_cell(
        &perl,
        &real_bam,
        mode_flag,
        &["--consecutive"],
        &format!("{kind}/consecutive"),
    );
    compare_cell(
        &perl,
        &real_bam,
        mode_flag,
        &["--percentage_cutoff", "20"],
        &format!("{kind}/percentage20"),
    );
}

#[test]
#[ignore]
fn byte_identity_real_data_se() {
    run_cells("-s", "SE", "FNC_REAL_SE");
}

#[test]
#[ignore]
fn byte_identity_real_data_pe() {
    run_cells("-p", "PE", "FNC_REAL_PE");
}

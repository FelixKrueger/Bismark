//! Phase A sanity checks: crate builds, version string + error Display work.

use bismark_bam2nuc::error::BismarkBam2nucError;
use bismark_bam2nuc::version_string;

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

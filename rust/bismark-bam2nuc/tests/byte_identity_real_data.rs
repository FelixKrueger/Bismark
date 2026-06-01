//! Real-data smoke (`#[ignore]`) for the oxy byte-identity gate.
//!
//! The AUTHORITATIVE real-data gate is `scripts/bam2nuc_byte_identity.sh`, which
//! runs BOTH the Rust `bam2nuc_rs` and the Perl `bam2nuc` v0.25.1 over a real
//! genome + BAMs and diffs the outputs byte-for-byte (see that script's header
//! for the oxy procedure). This Rust test is a lighter, env-driven smoke that
//! confirms the binary runs to completion on a real BAM and emits a non-empty
//! stats file — useful as a quick check before invoking the full shell gate.
//!
//! Run (on oxy, after `cargo build --release -p bismark-bam2nuc`):
//! ```text
//! BAM2NUC_REAL_GENOME=~/bismark_benchmarks/genome \
//! BAM2NUC_REAL_BAM=/path/to/sample.bam \
//!   cargo test -p bismark-bam2nuc --test byte_identity_real_data -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use assert_cmd::Command;

#[test]
#[ignore = "real-data smoke; needs BAM2NUC_REAL_GENOME + BAM2NUC_REAL_BAM (oxy)"]
fn real_bam_produces_nonempty_stats() {
    let genome = std::env::var("BAM2NUC_REAL_GENOME")
        .expect("set BAM2NUC_REAL_GENOME to a genome FASTA directory");
    let bam = std::env::var("BAM2NUC_REAL_BAM").expect("set BAM2NUC_REAL_BAM to a Bismark BAM");

    let out = tempfile::tempdir().unwrap();
    let dir_arg = format!("{}/", out.path().display());

    Command::cargo_bin("bam2nuc_rs")
        .unwrap()
        .arg("-g")
        .arg(&genome)
        .arg("--dir")
        .arg(&dir_arg)
        .arg(&bam)
        .assert()
        .success();

    let stem = PathBuf::from(&bam)
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let stats = out.path().join(format!("{stem}.nucleotide_stats.txt"));
    let body = std::fs::read_to_string(&stats)
        .unwrap_or_else(|e| panic!("stats file {stats:?} not written: {e}"));
    // Header + 4 mono + 16 di rows.
    assert_eq!(
        body.lines().count(),
        1 + 4 + 16,
        "expected 21 lines in the stats file, got:\n{body}"
    );
    assert!(
        body.starts_with("(di-)nucleotide\t"),
        "missing header:\n{body}"
    );
}

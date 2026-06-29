//! methylseq CLI-surface conformance — `BISMARK_COVERAGE2CYTOSINE`
//! (nf-core/methylseq **4.2.0**).
//!
//! Asserts the Rust `coverage2cytosine` CLI accepts every command methylseq's
//! c2c module emits. See `plans/06122026_methylseq-cli-conformance/`.
//!
//! **Command template** (`modules/nf-core/bismark/coverage2cytosine/main.nf`
//! + `conf/modules/bismark_coverage2cytosine.config` ext.args):
//! ```text
//!   coverage2cytosine <cov> --genome <idx> --output <prefix> --gzip <ext.args>
//!   ext.args: --nome-seq (params.nomeseq) else ''
//! ```
//! NOTE: `--genome` is the alias methylseq uses for `--genome_folder` (Perl took it
//! via Getopt prefix-match); accepting it was a beta.3 drop-in fix — the rows below
//! guard it.
//!
//! **Tiers:** Tier 1 parse + Tier 2 `validate()` (fixture-free — c2c's `validate()`
//! is all flag-mutex checks; it does not stat the genome dir or the coverage file).
//!
//! Re-scout on any methylseq version bump (pinned 4.2.0).

use bismark_coverage2cytosine::cli::Cli;
use clap::Parser;

#[test]
fn methylseq_coverage2cytosine_accept_rows() {
    let rows: Vec<(&str, Vec<&str>)> = vec![
        (
            "default (--genome alias + --output + --gzip)",
            vec![
                "coverage2cytosine",
                "sample.cov.gz",
                "--genome",
                "idx",
                "--output",
                "sample",
                "--gzip",
            ],
        ),
        (
            "NOMe-seq (--nome-seq)",
            vec![
                "coverage2cytosine",
                "sample.cov.gz",
                "--genome",
                "idx",
                "--output",
                "sample",
                "--gzip",
                "--nome-seq",
            ],
        ),
    ];
    for (label, argv) in rows {
        // Tier 1: parse.
        let cli = Cli::try_parse_from(&argv)
            .unwrap_or_else(|e| panic!("c2c must parse [{label}]: {argv:?}\n{e}"));
        // Tier 2: validate (fixture-free).
        assert!(
            cli.validate().is_ok(),
            "c2c methylseq command must validate [{label}]: {argv:?}"
        );
    }
}

/// Tier 3 — runtime: methylseq's c2c shape (`--genome <idx> --output <prefix>
/// --gzip`) on an EMPTY `.cov.gz` (the no-alignment sample) must exit 0 AND
/// produce the module-required `*report.txt.gz` + `*cytosine_context_summary.txt`.
/// Guards the graceful-empty fix (plan 06142026_empty-sample-extractor-c2c) at
/// the binary level — the exact `BISMARK_COVERAGE2CYTOSINE` contract.
#[test]
fn methylseq_coverage2cytosine_empty_runtime_emits_required_outputs() {
    use assert_cmd::Command;
    use std::io::Write;

    let tmp = tempfile::tempdir().unwrap();
    let gdir = tmp.path().join("idx");
    std::fs::create_dir(&gdir).unwrap();
    std::fs::write(gdir.join("g.fa"), ">chrA\nACGT\n").unwrap();
    // A valid empty gzip stream named `.cov.gz`.
    let cov = tmp.path().join("sample.cov.gz");
    let f = std::fs::File::create(&cov).unwrap();
    let mut enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
    enc.write_all(b"").unwrap();
    enc.finish().unwrap();

    Command::cargo_bin("coverage2cytosine")
        .unwrap()
        .arg(&cov)
        .arg("--genome")
        .arg(&gdir)
        .arg("--output")
        .arg("sample")
        .arg("--gzip")
        .arg("--dir")
        .arg(tmp.path())
        .assert()
        .success();

    let has_suffix = |suffix: &str| {
        std::fs::read_dir(tmp.path()).unwrap().any(|e| {
            e.ok()
                .and_then(|e| e.file_name().into_string().ok())
                .is_some_and(|n| n.ends_with(suffix))
        })
    };
    assert!(
        has_suffix("report.txt.gz"),
        "methylseq-required *report.txt.gz missing for a no-alignment c2c sample"
    );
    assert!(
        has_suffix("cytosine_context_summary.txt"),
        "methylseq-required *cytosine_context_summary.txt missing"
    );
}

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

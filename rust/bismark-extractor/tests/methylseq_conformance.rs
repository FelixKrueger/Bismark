//! methylseq CLI-surface conformance — `BISMARK_METHYLATIONEXTRACTOR`
//! (nf-core/methylseq **4.2.0**).
//!
//! Asserts the Rust `bismark_methylation_extractor` CLI accepts every command
//! methylseq's extractor module emits. See `plans/06122026_methylseq-cli-conformance/`.
//!
//! **Command template** (`modules/nf-core/bismark/methylationextractor/main.nf`
//! + `conf/modules/bismark_methylationextractor.config` ext.args):
//! ```text
//!   bismark_methylation_extractor <bam> --bedGraph --counts --gzip --report -s|-p \
//!     <ext.args> [--multicore N] [--buffer_size NG]
//!   ext.args: --comprehensive ; --cutoff N ; --CX (nomeseq) ; --ignore N ;
//!             --ignore_3prime N ; (PE) --no_overlap|--include_overlap,
//!             --ignore_r2 N, --ignore_3prime_r2 N
//! ```
//! NOTE: under `params.nomeseq` methylseq passes `--CX` to the extractor WITHOUT
//! `--cytosine_report` (the all-context coverage feeds a separate
//! `coverage2cytosine --nome-seq` step). That `--CX --bedGraph` combination was
//! rejected by an over-strict CLI until beta.5 (PR #978) — the `--CX` Tier-2 row
//! below guards that fix at the `validate()` layer.
//!
//! **Tiers:** Tier 1 parse (all rows, fixture-free) + Tier 2 `validate()` for the
//! key rows (extractor's `validate()` stats the positional input file → a temp file).
//!
//! Re-scout on any methylseq version bump (pinned 4.2.0).

use bismark_extractor::cli::Cli;
use clap::Parser;

/// A temp `.bam` so `validate()`'s input-existence check (cli.rs ~:456) passes.
/// Content is irrelevant — `validate()` does not open/parse the file (that happens
/// in `run()`); it only stats existence.
fn temp_input() -> tempfile::NamedTempFile {
    let f = tempfile::Builder::new()
        .suffix(".bam")
        .tempfile()
        .expect("tempfile");
    std::fs::write(f.path(), b"x").expect("write temp input");
    f
}

/// Tier 1 — every methylseq-emitted extractor command must clap-parse.
#[test]
fn methylseq_extractor_accept_rows_parse() {
    let rows: Vec<(&str, Vec<&str>)> = vec![
        (
            "SE CpG default",
            vec![
                "bismark_methylation_extractor",
                "in.bam",
                "--bedGraph",
                "--counts",
                "--gzip",
                "--report",
                "-s",
            ],
        ),
        (
            "SE NOMe-seq --CX (no --cytosine_report — beta.5 fix #978)",
            vec![
                "bismark_methylation_extractor",
                "in.bam",
                "--bedGraph",
                "--counts",
                "--gzip",
                "--report",
                "-s",
                "--CX",
            ],
        ),
        (
            "SE comprehensive",
            vec![
                "bismark_methylation_extractor",
                "in.bam",
                "--bedGraph",
                "--counts",
                "--gzip",
                "--report",
                "-s",
                "--comprehensive",
            ],
        ),
        (
            "SE cutoff + ignore + ignore_3prime",
            vec![
                "bismark_methylation_extractor",
                "in.bam",
                "--bedGraph",
                "--counts",
                "--gzip",
                "--report",
                "-s",
                "--cutoff",
                "1",
                "--ignore",
                "2",
                "--ignore_3prime",
                "2",
            ],
        ),
        (
            "SE auto --multicore + --buffer_size",
            vec![
                "bismark_methylation_extractor",
                "in.bam",
                "--bedGraph",
                "--counts",
                "--gzip",
                "--report",
                "-s",
                "--multicore",
                "4",
                "--buffer_size",
                "10G",
            ],
        ),
        (
            "PE default (--no_overlap)",
            vec![
                "bismark_methylation_extractor",
                "in.bam",
                "--bedGraph",
                "--counts",
                "--gzip",
                "--report",
                "-p",
                "--no_overlap",
            ],
        ),
        (
            "PE include_overlap + ignore_r2 + ignore_3prime_r2",
            vec![
                "bismark_methylation_extractor",
                "in.bam",
                "--bedGraph",
                "--counts",
                "--gzip",
                "--report",
                "-p",
                "--include_overlap",
                "--ignore_r2",
                "2",
                "--ignore_3prime_r2",
                "2",
            ],
        ),
    ];
    for (label, argv) in rows {
        assert!(
            Cli::try_parse_from(&argv).is_ok(),
            "extractor methylseq command must parse [{label}]: {argv:?}\n\
             (a parse rejection here = a methylseq drop-in gap)"
        );
    }
}

/// Tier 2 — the key accept rows must also pass `validate()` (the layer the `--CX`
/// gap lived in). Guards the beta.5 `--CX --bedGraph` (no `--cytosine_report`) fix.
#[test]
fn methylseq_extractor_validate_accept_rows() {
    let f = temp_input();
    let p = f.path().to_str().unwrap();
    let rows: Vec<(&str, Vec<&str>)> = vec![
        (
            "SE CpG default",
            vec![
                "bismark_methylation_extractor",
                p,
                "--bedGraph",
                "--counts",
                "--gzip",
                "--report",
                "-s",
            ],
        ),
        (
            "SE NOMe-seq --CX (beta.5 fix #978 — must validate Ok)",
            vec![
                "bismark_methylation_extractor",
                p,
                "--bedGraph",
                "--counts",
                "--gzip",
                "--report",
                "-s",
                "--CX",
            ],
        ),
        (
            "SE comprehensive",
            vec![
                "bismark_methylation_extractor",
                p,
                "--bedGraph",
                "--counts",
                "--gzip",
                "--report",
                "-s",
                "--comprehensive",
            ],
        ),
        (
            "PE no_overlap",
            vec![
                "bismark_methylation_extractor",
                p,
                "--bedGraph",
                "--counts",
                "--gzip",
                "--report",
                "-p",
                "--no_overlap",
            ],
        ),
    ];
    for (label, argv) in rows {
        let cli = Cli::try_parse_from(&argv)
            .unwrap_or_else(|e| panic!("extractor must parse [{label}]: {argv:?}\n{e}"));
        assert!(
            cli.validate().is_ok(),
            "extractor methylseq command must validate [{label}]: {argv:?}"
        );
    }
}

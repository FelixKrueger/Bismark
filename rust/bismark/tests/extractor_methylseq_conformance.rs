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

use bismark::extractor::cli::Cli;
use clap::Parser;
use std::path::Path;

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

/// Write a header-only BAM (a single `chr1` reference, zero records) — a
/// no-alignment sample, the empty-sample case beta.6 dedup now produces.
#[cfg(unix)]
fn write_header_only_bam(path: &Path) {
    use bismark::io::BamWriter;
    use bstr::BString;
    use noodles_sam::Header;
    use noodles_sam::header::record::value::Map;
    use noodles_sam::header::record::value::map::ReferenceSequence;
    use std::num::NonZeroUsize;

    let mut header = Header::default();
    header.reference_sequences_mut().insert(
        BString::from(b"chr1".to_vec()),
        Map::<ReferenceSequence>::new(NonZeroUsize::new(60).unwrap()),
    );
    let writer = BamWriter::from_path(path, header).unwrap();
    writer.finish().unwrap();
}

/// Tier 3 — runtime: the methylseq no-alignment command shape (`--bedGraph
/// --counts --gzip --report -s --CX`) on a header-only (zero-call) BAM must
/// exit 0 AND produce the module-required outputs. Guards the empty-sample
/// graceful-output fix (plan 06142026_empty-sample-extractor-c2c) at the
/// binary level — the exact `BISMARK_METHYLATIONEXTRACTOR` contract.
#[cfg(unix)]
#[test]
fn methylseq_extractor_no_alignment_runtime_emits_required_outputs() {
    use assert_cmd::Command;

    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("noalign.bam");
    write_header_only_bam(&bam);
    let out_dir = work.path().join("out");

    Command::cargo_bin("bismark_methylation_extractor")
        .unwrap()
        .arg(&bam)
        .args(["--bedGraph", "--counts", "--gzip", "--report", "-s", "--CX"])
        .arg("--output_dir")
        .arg(&out_dir)
        .assert()
        .success();

    // The 5 methylseq-required output globs (none optional).
    let has_suffix = |suffix: &str| {
        std::fs::read_dir(&out_dir).unwrap().any(|e| {
            e.ok()
                .and_then(|e| e.file_name().into_string().ok())
                .is_some_and(|n| n.ends_with(suffix))
        })
    };
    for suffix in [
        ".bedGraph.gz",
        ".bismark.cov.gz",
        ".txt.gz",
        "_splitting_report.txt",
        "M-bias.txt",
    ] {
        assert!(
            has_suffix(suffix),
            "methylseq-required output *{suffix} missing for a no-alignment sample"
        );
    }
}

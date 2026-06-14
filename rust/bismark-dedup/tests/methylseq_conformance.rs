//! methylseq CLI-surface conformance — `BISMARK_DEDUPLICATE` (nf-core/methylseq **4.2.0**).
//!
//! Asserts the Rust `deduplicate_bismark` CLI accepts every command methylseq's
//! `BISMARK_DEDUPLICATE` module emits. See `plans/06122026_methylseq-cli-conformance/`.
//!
//! **Command template** (`modules/nf-core/bismark/deduplicate/main.nf`;
//! `conf/modules/bismark_deduplicate.config` sets `ext.args = ''`):
//! ```text
//!   deduplicate_bismark <ext.args=''> -s|-p --bam <bam>
//! ```
//! **Tiers:** Tier 1 parse + Tier 2 `validate()` (fixture-free — dedup's `validate()`
//! only checks flag/arity rules, it does not stat the input file) + a Tier 3
//! **empty-input runtime** row (rev 1, plans/06132026_dedup-empty-input): runs
//! the binary on a header-only BAM and asserts it exits 0 with a valid empty
//! output — the methylseq drop-in class that parse/validate rows can't catch
//! (a no-alignment sample reaching `BISMARK_DEDUPLICATE`). Bismark dies on this;
//! the Rust port deliberately handles it gracefully so the pipeline survives.
//! The downstream `BISMARK_METHYLATIONEXTRACTOR` (Rust extractor) was verified to
//! also handle a header-only BAM gracefully, so the dedup fix unblocks the chain.
//!
//! Re-scout on any methylseq version bump (pinned 4.2.0).

use bismark_dedup::cli::Cli;
use clap::Parser;

use assert_cmd::Command;
use bismark_io::BamWriter;
use bstr::BString;
use noodles_sam::Header;
use noodles_sam::header::record::value::Map;
use noodles_sam::header::record::value::map::ReferenceSequence;
use std::num::NonZeroUsize;
use tempfile::TempDir;

#[test]
fn methylseq_deduplicate_accept_rows() {
    let rows: Vec<(&str, Vec<&str>)> = vec![
        (
            "single-end",
            vec!["deduplicate_bismark", "-s", "--bam", "aligned.bam"],
        ),
        (
            "paired-end",
            vec!["deduplicate_bismark", "-p", "--bam", "aligned.bam"],
        ),
    ];
    for (label, argv) in rows {
        // Tier 1: parse.
        let cli = Cli::try_parse_from(&argv)
            .unwrap_or_else(|e| panic!("BISMARK_DEDUPLICATE must parse [{label}]: {argv:?}\n{e}"));
        // Tier 2: validate (fixture-free).
        assert!(
            cli.validate().is_ok(),
            "BISMARK_DEDUPLICATE must validate [{label}]: {argv:?}"
        );
    }
}

/// Write a header-only BAM (valid header, zero alignment records) at `path` —
/// exactly what the Bismark aligner emits when nothing aligns.
fn write_header_only_bam(path: &std::path::Path) {
    let mut header = Header::default();
    header.reference_sequences_mut().insert(
        BString::from("chr1"),
        Map::<ReferenceSequence>::new(NonZeroUsize::try_from(1_000_000).unwrap()),
    );
    let writer = BamWriter::from_path(path, header).unwrap();
    writer.finish().unwrap();
}

/// Tier 3 (rev 1): a no-alignment sample reaching `BISMARK_DEDUPLICATE` must
/// NOT crash the pipeline. Running the binary on a header-only BAM (both `-s`
/// and `-p`, as methylseq emits) must exit 0 and leave a valid deduplicated
/// BAM + a deduplication report on disk. Parse/validate rows cannot catch this
/// — only actually running the offending command does.
#[test]
fn methylseq_deduplicate_empty_input_does_not_crash_pipeline() {
    for mode in ["-s", "-p"] {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("aligned.bam");
        write_header_only_bam(&input);

        Command::cargo_bin("deduplicate_bismark_rs")
            .unwrap()
            .arg(mode)
            .arg("--bam")
            .arg("--output_dir")
            .arg(dir.path())
            .arg(&input)
            .assert()
            .success();

        assert!(
            dir.path().join("aligned.deduplicated.bam").exists(),
            "[{mode}] empty-input dedup must write an output BAM"
        );
        assert!(
            dir.path().join("aligned.deduplication_report.txt").exists(),
            "[{mode}] empty-input dedup must write a report"
        );
    }
}

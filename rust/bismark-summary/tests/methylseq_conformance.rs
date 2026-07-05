//! methylseq CLI-surface conformance — `BISMARK_SUMMARY` (nf-core/methylseq **4.2.0**).
//!
//! Asserts the Rust `bismark2summary` CLI accepts the command methylseq's summary
//! module emits. See `plans/06122026_methylseq-cli-conformance/`.
//!
//! **Command template** (`modules/nf-core/bismark/summary/main.nf`;
//! `conf/modules/bismark_summary.config` sets `ext.args = ''`):
//! ```text
//!   bismark2summary <bam1> <bam2> ...
//! ```
//! methylseq passes the per-sample BAMs positionally and no flags.
//!
//! **Tiers:** Tier 1 parse only (`bismark-summary::validate()` returns `ResolvedConfig`
//! with no `Result`, so it cannot reject — parse is the meaningful conformance layer).
//!
//! Re-scout on any methylseq version bump (pinned 4.2.0).

use bismark_summary::cli::Cli;
use clap::Parser;

#[test]
fn methylseq_summary_positional_bams_parse() {
    let rows: Vec<(&str, Vec<&str>)> = vec![
        ("single sample", vec!["bismark2summary", "sample1.bam"]),
        (
            "multiple samples",
            vec![
                "bismark2summary",
                "sample1.bam",
                "sample2.bam",
                "sample3.bam",
            ],
        ),
    ];
    for (label, argv) in rows {
        assert!(
            Cli::try_parse_from(&argv).is_ok(),
            "BISMARK_SUMMARY methylseq command must parse [{label}]: {argv:?}\n\
             (a parse rejection here = a methylseq drop-in gap)"
        );
    }
}

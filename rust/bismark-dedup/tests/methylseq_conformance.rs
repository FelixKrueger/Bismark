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
//! only checks flag/arity rules, it does not stat the input file).
//!
//! Re-scout on any methylseq version bump (pinned 4.2.0).

use bismark_dedup::cli::Cli;
use clap::Parser;

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

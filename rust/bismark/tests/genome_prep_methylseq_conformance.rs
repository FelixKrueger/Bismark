//! methylseq CLI-surface conformance — `BISMARK_GENOMEPREPARATION`
//! (nf-core/methylseq **4.2.0**).
//!
//! Asserts the Rust `bismark_genome_preparation` CLI accepts every command
//! methylseq's genome-prep module emits. See `plans/06122026_methylseq-cli-conformance/`.
//!
//! **Command template** (`modules/nf-core/bismark/genomepreparation/main.nf`
//! + `conf/modules/bismark_genomepreparation.config` ext.args):
//! ```text
//!   bismark_genome_preparation <ext.args> BismarkIndex
//!   ext.args: --bowtie2 | --hisat2 ; --slam (params.slamseq)
//! ```
//! **Tiers:** Tier 1 parse only. The conformance concern is "are `--bowtie2` /
//! `--hisat2` / `--slam` accepted flags" — Tier 1 settles that. Tier 2 `validate()`
//! is intentionally skipped: this crate's `validate()` canonicalizes the positional
//! genome dir (needs a real path on disk) and there is no over-strict-validate risk
//! for these three flags, so a `validate()` row would add a filesystem fixture for no
//! extra conformance signal.
//!
//! Re-scout on any methylseq version bump (pinned 4.2.0).

use bismark::genome_prep::cli::Cli;
use clap::Parser;

#[test]
fn methylseq_genomepreparation_accept_rows_parse() {
    let rows: Vec<(&str, Vec<&str>)> = vec![
        (
            "default bowtie2 (--aligner bismark)",
            vec!["bismark_genome_preparation", "--bowtie2", "BismarkIndex"],
        ),
        (
            "hisat2 (--aligner bismark_hisat)",
            vec!["bismark_genome_preparation", "--hisat2", "BismarkIndex"],
        ),
        (
            "slamseq (params.slamseq → --slam)",
            vec![
                "bismark_genome_preparation",
                "--bowtie2",
                "--slam",
                "BismarkIndex",
            ],
        ),
    ];
    for (label, argv) in rows {
        assert!(
            Cli::try_parse_from(&argv).is_ok(),
            "genome-prep methylseq command must parse [{label}]: {argv:?}\n\
             (a parse rejection here = a methylseq drop-in gap)"
        );
    }
}

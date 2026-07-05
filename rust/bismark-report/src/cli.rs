//! Command-line surface (clap derive).
//!
//! Flag spellings match the Perl `bismark2report` exactly (underscores
//! preserved: `--alignment_report`, `--dedup_report`, `--splitting_report`,
//! `--mbias_report`, `--nucleotide_report`). `--version`/`--man` are handled
//! manually (clap's auto-version is disabled) so the binary can print the
//! Bismark provenance banner / long help, mirroring `bismark-genome-preparation`.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "bismark2report_rs",
    about = "Generate a graphical HTML report from a Bismark alignment report (+ optional companion reports).",
    disable_version_flag = true
)]
pub struct Cli {
    /// Bismark alignment report (mandatory data source). If omitted, auto-detect
    /// `*E_report.txt` in the current directory (one HTML per match).
    #[arg(long = "alignment_report")]
    pub alignment_report: Option<String>,

    /// Deduplication report; `none` to skip; auto-detect by basename if omitted.
    #[arg(long = "dedup_report")]
    pub dedup_report: Option<String>,

    /// Methylation-extractor splitting report; `none` to skip; auto-detect.
    #[arg(long = "splitting_report")]
    pub splitting_report: Option<String>,

    /// M-bias report; `none` to skip; auto-detect.
    #[arg(long = "mbias_report")]
    pub mbias_report: Option<String>,

    /// Nucleotide-coverage report; `none` to skip; auto-detect.
    #[arg(long = "nucleotide_report")]
    pub nucleotide_report: Option<String>,

    /// Output directory (default: current directory).
    #[arg(long = "dir")]
    pub dir: Option<String>,

    /// Output filename (only legal with a single alignment report).
    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,

    /// Verbose diagnostics.
    #[arg(long)]
    pub verbose: bool,

    /// HIDDEN test-only: inject a fixed UNIX epoch, formatted in **UTC**, into
    /// `{{date}}`/`{{time}}` so committed golden HTML is byte-stable. Default
    /// (unset) = local time (Perl `localtime`).
    #[arg(long = "__test_timestamp", hide = true)]
    pub test_timestamp: Option<i64>,

    /// Print the full help (alias of `--help`).
    #[arg(long)]
    pub man: bool,

    /// Print version and exit.
    #[arg(long, short = 'V')]
    pub version: bool,
}

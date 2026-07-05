//! Command-line interface for `bismark2summary_rs`.
//!
//! [`Cli`] is the clap-derived parser. [`Cli::validate`] resolves it into a
//! [`ResolvedConfig`], applying Perl's basename/title defaults with Perl
//! *truthiness* semantics (`bismark2summary:207-212`): an unset, **empty**,
//! or literal `"0"` value falls back to the default. (clap yields
//! `Some("0")` for `-o 0`, so the fallback must test the value, not merely
//! `Option::is_none`.)
//!
//! CLI surface (Perl `process_commandline`, `bismark2summary:36-41`):
//! `-o/--basename`, `--title`, `--verbose`, `--version`, `--help`/`--man`,
//! and optional positional BAM file(s). There is **no** `--dir` and **no**
//! per-report flag.
//!
//! `--__test_timestamp <EPOCH>` is a hidden, test-only flag (clap
//! `hide = true`): it injects a fixed UNIX epoch (formatted in UTC with
//! Perl's scalar-`localtime` ctime layout) into `{{report_timestamp}}` so
//! committed HTML goldens are byte-stable. Default = local `localtime`.

use std::path::PathBuf;

use clap::Parser;

/// Default output basename (Perl `bismark2summary:208`).
pub const DEFAULT_BASENAME: &str = "bismark_summary_report";
/// Default HTML report title (Perl `bismark2summary:211`).
pub const DEFAULT_TITLE: &str = "Bismark Summary Report";

/// Parsed command-line arguments. Use [`Cli::validate`] to convert to a
/// [`ResolvedConfig`] after parsing.
#[derive(Parser, Debug)]
#[command(
    name = "bismark2summary_rs",
    about = "Generate a project-level multi-sample summary (.txt + .html) from Bismark report files",
    long_about = None,
    disable_version_flag = true
)]
pub struct Cli {
    /// Optional explicit Bismark BAM file(s). If none are given, the current
    /// directory is scanned for `*bismark_{bt2,hisat2}[_pe].bam`.
    pub bam_files: Vec<PathBuf>,

    /// Basename of the output files; emits `<basename>.txt` and
    /// `<basename>.html`. Default: `bismark_summary_report`.
    #[arg(short = 'o', long = "basename")]
    pub basename: Option<String>,

    /// HTML report title. Default: `Bismark Summary Report`.
    #[arg(long = "title")]
    pub title: Option<String>,

    /// Extra diagnostics on STDOUT/STDERR (not byte-gated).
    #[arg(long = "verbose")]
    pub verbose: bool,

    /// Print version information and exit.
    #[arg(short = 'V', long = "version")]
    pub version: bool,

    /// Alias for `--help` (Perl accepts `--help|--man`). Prints the long
    /// help and exits 0.
    #[arg(long = "man")]
    pub man: bool,

    /// HIDDEN test-only: inject a fixed UNIX epoch (formatted in UTC with
    /// Perl's scalar-`localtime` ctime layout) into `{{report_timestamp}}`
    /// for byte-stable HTML goldens. Default = local `localtime`.
    #[arg(long = "__test_timestamp", hide = true)]
    pub test_timestamp: Option<i64>,
}

/// The resolved, validated configuration passed to the pipeline.
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// Explicit positional BAM inputs (verbatim argv order). Empty ⇒
    /// auto-detect in the current directory.
    pub bam_files: Vec<PathBuf>,
    /// Output basename with the default applied (Perl truthiness).
    pub report_basename: String,
    /// HTML page title with the default applied (Perl truthiness).
    pub page_title: String,
    /// `--verbose` diagnostics toggle.
    pub verbose: bool,
    /// Fixed UNIX epoch for the HTML timestamp (test-only); `None` ⇒ local
    /// `localtime` at runtime.
    pub test_timestamp: Option<i64>,
}

impl Cli {
    /// Resolve the parsed arguments into a [`ResolvedConfig`].
    ///
    /// There are no conflicting-flag rules to reject (the CLI is flat).
    /// The only resolution is the basename/title default with Perl
    /// truthiness (`unset` / `""` / `"0"` ⇒ default).
    #[must_use]
    pub fn validate(self) -> ResolvedConfig {
        ResolvedConfig {
            bam_files: self.bam_files,
            report_basename: resolve_truthy(self.basename, DEFAULT_BASENAME),
            page_title: resolve_truthy(self.title, DEFAULT_TITLE),
            verbose: self.verbose,
            test_timestamp: self.test_timestamp,
        }
    }
}

/// Apply a default the way Perl's `unless ($x)` does: an unset, empty, or
/// literal `"0"` value is "false" and falls back to `default`.
fn resolve_truthy(opt: Option<String>, default: &str) -> String {
    match opt {
        Some(v) if !v.is_empty() && v != "0" => v,
        _ => default.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut full = vec!["bismark2summary_rs"];
        full.extend(args.iter().copied());
        Cli::try_parse_from(full)
    }

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_no_args() {
        let cli = parse(&[]).unwrap();
        assert!(cli.bam_files.is_empty());
        assert!(cli.basename.is_none() && cli.title.is_none());
        assert!(!cli.verbose && !cli.version && !cli.man);
    }

    #[test]
    fn parses_positional_bams_in_argv_order() {
        let cli = parse(&["b.bam", "a.bam"]).unwrap();
        assert_eq!(
            cli.bam_files,
            vec![PathBuf::from("b.bam"), PathBuf::from("a.bam")]
        );
    }

    #[test]
    fn default_basename_and_title_when_unset() {
        let cfg = parse(&[]).unwrap().validate();
        assert_eq!(cfg.report_basename, DEFAULT_BASENAME);
        assert_eq!(cfg.page_title, DEFAULT_TITLE);
    }

    #[test]
    fn explicit_basename_and_title_pass_through() {
        let cfg = parse(&["-o", "run42", "--title", "My Run"])
            .unwrap()
            .validate();
        assert_eq!(cfg.report_basename, "run42");
        assert_eq!(cfg.page_title, "My Run");
    }

    #[test]
    fn truthiness_zero_falls_back_to_default() {
        // Perl `unless ($x)`: "0" is falsy → default applies.
        let cfg = parse(&["-o", "0", "--title", "0"]).unwrap().validate();
        assert_eq!(cfg.report_basename, DEFAULT_BASENAME);
        assert_eq!(cfg.page_title, DEFAULT_TITLE);
    }

    #[test]
    fn empty_basename_falls_back_to_default() {
        let cfg = parse(&["-o", ""]).unwrap().validate();
        assert_eq!(cfg.report_basename, DEFAULT_BASENAME);
    }

    #[test]
    fn basename_literal_value_is_not_a_path_anchor() {
        // A basename that itself is "00" is truthy (only the exact "0" is
        // falsy in Perl); pass through.
        let cfg = parse(&["-o", "00"]).unwrap().validate();
        assert_eq!(cfg.report_basename, "00");
    }

    #[test]
    fn version_and_man_flags_parse_without_bams() {
        assert!(parse(&["--version"]).unwrap().version);
        assert!(parse(&["--man"]).unwrap().man);
    }

    #[test]
    fn hidden_test_timestamp_parses() {
        let cli = parse(&["--__test_timestamp", "1700000000"]).unwrap();
        assert_eq!(cli.test_timestamp, Some(1_700_000_000));
    }
}

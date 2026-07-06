//! Command-line interface for `methylation_consistency`.
//!
//! [`Cli`] is the clap-derived parser; [`Cli::validate`] resolves it into a
//! [`ResolvedConfig`], applying defaults and the Perl threshold range checks.
//!
//! Long flag names keep Perl's **underscores** (`--paired_end`, `--single_end`,
//! `--lower_threshold`, `--upper_threshold`, `--samtools_path`) for drop-in
//! compatibility; `--min-count` keeps its hyphen. `--samtools_path` is accepted
//! but ignored (bismark-io is pure Rust). `--quiet` (new) suppresses STDERR
//! diagnostics. `--version`/`-V` emits a provenance string via
//! [`crate::version_string`]; clap's auto-version is disabled.

use std::path::PathBuf;

use clap::Parser;

use crate::error::MethConsError;

/// `--help` footer: the per-tool last-modified date (embedded by build.rs).
const HELP_FOOTER: &str = concat!("Last modified: ", env!("BISMARK_LAST_MODIFIED"));

/// Parsed command-line arguments. Use [`Cli::validate`] after parsing.
#[derive(Parser, Debug)]
#[command(
    name = "methylation_consistency",
    about = "Split a Bismark BAM into three BAMs by read-level methylation consistency",
    long_about = None,
    disable_version_flag = true,
    after_help = HELP_FOOTER
)]
pub struct Cli {
    /// Bismark BAM file(s) to split by read methylation consistency.
    pub files: Vec<PathBuf>,

    /// Force paired-end mode (R1+R2 calls are summed). Default: auto-detect.
    #[arg(short = 'p', long = "paired_end", conflicts_with = "single_end")]
    pub paired_end: bool,

    /// Force single-end mode. Default: auto-detect from the `@PG` line.
    #[arg(short = 's', long = "single_end")]
    pub single_end: bool,

    /// Experimental: classify on CHH (`H`/`h`) context instead of CpG (`Z`/`z`).
    #[arg(long = "chh")]
    pub chh: bool,

    /// Percentage up to which a read counts as unmethylated (0–49) [default 10].
    #[arg(long = "lower_threshold")]
    pub lower_threshold: Option<i64>,

    /// Percentage above which a read counts as methylated (51–100) [default 90].
    #[arg(long = "upper_threshold")]
    pub upper_threshold: Option<i64>,

    /// Minimum number of cytosine calls for a read to be considered [default 5].
    // `hide_default_value`: the doc comment already states "[default 5]", so
    // suppress clap's auto-appended "[default: 5]" (it would print twice).
    // lower/upper_threshold show their defaults the same way (doc text only).
    #[arg(
        short = 'm',
        long = "min-count",
        default_value_t = 5u32,
        hide_default_value = true
    )]
    pub min_count: u32,

    /// Path to a `samtools` binary (accepted for Perl compatibility,
    /// **ignored** — bismark-io is pure Rust).
    #[arg(long = "samtools_path")]
    pub samtools_path: Option<PathBuf>,

    /// Suppress STDERR diagnostics (new; not in Perl).
    #[arg(long = "quiet")]
    pub quiet: bool,

    /// Print version information and exit.
    #[arg(short = 'V', long = "version")]
    pub version: bool,
}

/// Library mode resolved from the `-s`/`-p` flags (or deferred to per-file
/// header auto-detection).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryMode {
    /// Forced single-end (`-s`).
    Single,
    /// Forced paired-end (`-p`).
    Paired,
    /// Auto-detect per file from the Bismark `@PG` line (default). A missing
    /// Bismark `@PG` falls through to single-end (SPEC §2.3).
    Auto,
}

/// The resolved, validated CLI config passed to the pipeline.
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// Positional input files (≥1, processed independently).
    pub files: Vec<PathBuf>,
    /// Resolved library mode.
    pub mode: LibraryMode,
    /// CHH context instead of CpG.
    pub chh: bool,
    /// Lower threshold (validated 0–49; default 10).
    pub lower: i64,
    /// Upper threshold (validated 51–100; default 90).
    pub upper: i64,
    /// Minimum cytosine calls per read (default 5; 0 allowed).
    pub min_count: u32,
    /// Suppress STDERR diagnostics.
    pub quiet: bool,
}

impl Cli {
    /// Resolve and validate the parsed arguments.
    ///
    /// - `--upper_threshold` (if given) must be 51–100, else
    ///   [`MethConsError::UpperThresholdOutOfRange`] (Perl line 76).
    /// - `--lower_threshold` (if given) must be 0–49, else
    ///   [`MethConsError::LowerThresholdOutOfRange`] (Perl line 85).
    /// - Empty `files` → [`MethConsError::NoInputFiles`] (Perl line 131).
    /// - `-s`/`-p` are mutually exclusive (enforced by clap `conflicts_with`).
    ///   Neither → `LibraryMode::Auto`.
    pub fn validate(self) -> Result<ResolvedConfig, MethConsError> {
        let upper = match self.upper_threshold {
            Some(v) => {
                if !(51..=100).contains(&v) {
                    return Err(MethConsError::UpperThresholdOutOfRange);
                }
                v
            }
            None => 90,
        };
        let lower = match self.lower_threshold {
            Some(v) => {
                if !(0..=49).contains(&v) {
                    return Err(MethConsError::LowerThresholdOutOfRange);
                }
                v
            }
            None => 10,
        };

        if self.files.is_empty() {
            return Err(MethConsError::NoInputFiles);
        }

        let mode = match (self.single_end, self.paired_end) {
            (true, false) => LibraryMode::Single,
            (false, true) => LibraryMode::Paired,
            (false, false) => LibraryMode::Auto,
            // clap `conflicts_with` rejects (true, true) at parse time.
            (true, true) => unreachable!("clap conflicts_with prevents this"),
        };

        // --samtools_path is accepted and ignored (bismark-io is pure Rust).
        let _ = self.samtools_path;

        Ok(ResolvedConfig {
            files: self.files,
            mode,
            chh: self.chh,
            lower,
            upper,
            min_count: self.min_count,
            quiet: self.quiet,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut full = vec!["methylation_consistency"];
        full.extend(args.iter().copied());
        Cli::try_parse_from(full)
    }

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_defaults() {
        let cfg = parse(&["sample.bam"]).unwrap().validate().unwrap();
        assert_eq!(cfg.files, vec![PathBuf::from("sample.bam")]);
        assert_eq!(cfg.mode, LibraryMode::Auto);
        assert!(!cfg.chh);
        assert_eq!(cfg.lower, 10);
        assert_eq!(cfg.upper, 90);
        assert_eq!(cfg.min_count, 5);
        assert!(!cfg.quiet);
    }

    #[test]
    fn min_count_short_and_long() {
        assert_eq!(parse(&["-m", "3", "x.bam"]).unwrap().min_count, 3);
        assert_eq!(parse(&["--min-count", "7", "x.bam"]).unwrap().min_count, 7);
        // 0 is allowed (Perl `^\d+$` matches 0).
        assert_eq!(parse(&["-m", "0", "x.bam"]).unwrap().min_count, 0);
    }

    #[test]
    fn underscore_long_flags() {
        let cfg = parse(&["--single_end", "x.bam"])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(cfg.mode, LibraryMode::Single);
        let cfg = parse(&["--paired_end", "x.bam"])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(cfg.mode, LibraryMode::Paired);
    }

    #[test]
    fn rejects_both_single_and_paired() {
        let err = parse(&["-s", "-p", "x.bam"]).unwrap_err();
        assert!(
            err.to_string().contains("cannot be used with"),
            "got: {err}"
        );
    }

    #[test]
    fn rejects_no_input_files() {
        let err = parse(&[]).unwrap().validate().unwrap_err();
        assert!(matches!(err, MethConsError::NoInputFiles));
    }

    #[test]
    fn upper_threshold_range() {
        assert!(
            parse(&["--upper_threshold", "51", "x.bam"])
                .unwrap()
                .validate()
                .is_ok()
        );
        assert!(
            parse(&["--upper_threshold", "100", "x.bam"])
                .unwrap()
                .validate()
                .is_ok()
        );
        assert!(matches!(
            parse(&["--upper_threshold", "50", "x.bam"])
                .unwrap()
                .validate()
                .unwrap_err(),
            MethConsError::UpperThresholdOutOfRange
        ));
        assert!(matches!(
            parse(&["--upper_threshold", "101", "x.bam"])
                .unwrap()
                .validate()
                .unwrap_err(),
            MethConsError::UpperThresholdOutOfRange
        ));
    }

    #[test]
    fn lower_threshold_range() {
        assert!(
            parse(&["--lower_threshold", "0", "x.bam"])
                .unwrap()
                .validate()
                .is_ok()
        );
        assert!(
            parse(&["--lower_threshold", "49", "x.bam"])
                .unwrap()
                .validate()
                .is_ok()
        );
        assert!(matches!(
            parse(&["--lower_threshold", "50", "x.bam"])
                .unwrap()
                .validate()
                .unwrap_err(),
            MethConsError::LowerThresholdOutOfRange
        ));
        // Negative must use the `=` form: clap treats space-form `-1` as a
        // flag (standard CLI behavior), erroring at parse — Perl's Getopt::Long
        // would accept it and range-reject; both reject (CLI errors aren't
        // byte-gated). The `=-1` form reaches the validate-layer range check.
        assert!(matches!(
            parse(&["--lower_threshold=-1", "x.bam"])
                .unwrap()
                .validate()
                .unwrap_err(),
            MethConsError::LowerThresholdOutOfRange
        ));
    }

    #[test]
    fn chh_and_quiet_flags() {
        let cfg = parse(&["--chh", "--quiet", "x.bam"])
            .unwrap()
            .validate()
            .unwrap();
        assert!(cfg.chh);
        assert!(cfg.quiet);
    }

    #[test]
    fn samtools_path_accepted_and_ignored() {
        let cfg = parse(&["--samtools_path", "/usr/bin/samtools", "x.bam"])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(cfg.files.len(), 1);
    }

    #[test]
    fn version_flag_parses_without_files() {
        let cli = parse(&["--version"]).unwrap();
        assert!(cli.version);
        assert!(cli.files.is_empty());
    }
}

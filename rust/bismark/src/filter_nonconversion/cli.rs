//! Command-line interface for `filter_non_conversion`.
//!
//! [`Cli`] is the clap-derived parser; [`Cli::validate`] reproduces Perl
//! `filter_non_conversion`'s `process_commandline` validation (lines
//! 469–606) and resolves to a [`ResolvedConfig`].
//!
//! Faithfulness notes:
//! - The integer options are parsed as **signed `i64`** (not `u32`) so that
//!   negative values reach the Perl-style *validation* checks (e.g.
//!   `--threshold -1` → "sensible value for -1") rather than being rejected
//!   by clap's parser. Matches Perl `GetOptions(...=i)`.
//! - `-s`/`-p` mutual exclusion is enforced in `validate()` (Perl's `die`
//!   path, line 545), NOT via clap `conflicts_with`, so the error message and
//!   exit code match Perl.
//! - `--samtools_path` is accepted and **ignored** (noodles is pure-Rust).
//! - clap's auto `--version` is disabled so a custom provenance string is
//!   emitted (see [`crate::filter_nonconversion::version_string`]); `--help` uses clap's default
//!   (exit 0 — a documented deviation from Perl's `print_helpfile`/`exit 1`).

use std::path::PathBuf;

use clap::Parser;

use crate::filter_nonconversion::error::BismarkFilterError;
use crate::filter_nonconversion::filter::FilterMode;

/// `--help` footer: the per-tool last-modified date (embedded by build.rs).
const HELP_FOOTER: &str = concat!("Last modified: ", env!("BISMARK_LAST_MODIFIED"));

/// Parsed command-line arguments. Use [`Cli::validate`] to resolve.
#[derive(Parser, Debug)]
#[command(
    name = "filter_non_conversion",
    about = "Filter reads/read-pairs with apparent incomplete bisulfite conversion \
             (too much non-CG methylation) from Bismark BAM files",
    long_about = None,
    disable_version_flag = true,
    // Accept negative-number values (e.g. `--threshold -1`) so they reach the
    // Perl-style validation checks rather than being mis-parsed as flags.
    // Matches Perl `GetOptions(...=i)`. See cli.rs module docs.
    allow_negative_numbers = true,
    after_help = HELP_FOOTER
)]
pub struct Cli {
    /// Bismark BAM file(s) to filter. Each is processed independently.
    pub files: Vec<PathBuf>,

    /// Force single-end mode (auto-detected from the @PG line if unset).
    #[arg(short = 's', long = "single")]
    pub single: bool,

    /// Force paired-end mode (either mate failing removes the whole pair).
    #[arg(short = 'p', long = "paired")]
    pub paired: bool,

    /// Methylated-non-CG count at which a read/pair is removed (default 3).
    #[arg(long = "threshold")]
    pub threshold: Option<i64>,

    /// Require the methylated non-CG calls to be CONSECUTIVE; any unmethylated
    /// cytosine (z/h/x) resets the counter. Mutually exclusive with
    /// --percentage_cutoff.
    #[arg(long = "consecutive")]
    pub consecutive: bool,

    /// Remove on an overall non-CG methylation percentage (0-100) instead of
    /// an absolute count. Requires at least --minimum_count non-CG calls.
    #[arg(long = "percentage_cutoff")]
    pub percentage_cutoff: Option<i64>,

    /// Minimum non-CG cytosines before --percentage_cutoff applies (default 5).
    #[arg(long = "minimum_count")]
    pub minimum_count: Option<i64>,

    /// Path to samtools (accepted for Perl compatibility, IGNORED — this port
    /// is pure-Rust and needs no samtools).
    #[arg(long = "samtools_path")]
    pub samtools_path: Option<PathBuf>,

    /// Print version information and exit.
    #[arg(short = 'V', long = "version")]
    pub version: bool,
}

/// The validated, resolved configuration handed to the pipeline.
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// Positional input files (each processed independently).
    pub files: Vec<PathBuf>,
    /// `Some(true)` = PE, `Some(false)` = SE, `None` = auto-detect from @PG.
    pub explicit_mode: Option<bool>,
    /// The resolved decision mode + parameters.
    pub mode: FilterMode,
}

impl Cli {
    /// Validate flag combinations and resolve to a [`ResolvedConfig`],
    /// reproducing Perl `process_commandline`'s order:
    /// 1. percentage block (consecutive-exclusive, 0–100 range, min-count
    ///    default 5) — only when `--percentage_cutoff` is set;
    /// 2. `-s` + `-p` mutual exclusion;
    /// 3. threshold validation (`> 0`, default 3) — **unconditional**, so a
    ///    bad `--threshold` is rejected even alongside `--percentage_cutoff`.
    ///
    /// The empty-`files` case is handled by the caller (`main`) *before*
    /// this, matching Perl's `@ARGV`-empty check preceding option validation
    /// (so no-files + a bad option yields the no-files message, not the
    /// option error).
    pub fn validate(self) -> Result<ResolvedConfig, BismarkFilterError> {
        // 1. Percentage block (Perl lines 520–536).
        let percentage = if let Some(pct) = self.percentage_cutoff {
            if self.consecutive {
                return Err(BismarkFilterError::PercentageAndConsecutive);
            }
            if !(0..=100).contains(&pct) {
                return Err(BismarkFilterError::PercentageOutOfRange);
            }
            let minimum_count = match self.minimum_count {
                Some(m) => {
                    if m <= 0 {
                        return Err(BismarkFilterError::InvalidMinimumCount);
                    }
                    m as u64
                }
                None => 5,
            };
            Some((pct as u32, minimum_count))
        } else {
            None
        };

        // 2. -s + -p (Perl line 545).
        if self.single && self.paired {
            return Err(BismarkFilterError::BothSingleAndPaired);
        }

        // 3. Threshold (Perl lines 596–603) — validated unconditionally; the
        //    value is only *used* in threshold mode, but a bad value dies
        //    regardless (faithful to the Perl, which validates outside the
        //    percentage block).
        let threshold = match self.threshold {
            Some(t) => {
                if t <= 0 {
                    return Err(BismarkFilterError::InvalidThreshold { value: t });
                }
                t as u64
            }
            None => 3,
        };

        let mode = match percentage {
            Some((cutoff, minimum_count)) => FilterMode::Percentage {
                cutoff,
                minimum_count,
            },
            None => FilterMode::Threshold {
                threshold,
                consecutive: self.consecutive,
            },
        };

        let explicit_mode = match (self.single, self.paired) {
            (true, false) => Some(false),
            (false, true) => Some(true),
            (false, false) => None,
            (true, true) => unreachable!("rejected by BothSingleAndPaired above"),
        };

        // --samtools_path is accepted and ignored (pure-Rust I/O).
        let _ = self.samtools_path;

        Ok(ResolvedConfig {
            files: self.files,
            explicit_mode,
            mode,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut full = vec!["filter_non_conversion"];
        full.extend(args.iter().copied());
        Cli::try_parse_from(full)
    }

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn default_mode_is_threshold_3() {
        let cfg = parse(&["x.bam"]).unwrap().validate().unwrap();
        assert_eq!(
            cfg.mode,
            FilterMode::Threshold {
                threshold: 3,
                consecutive: false
            }
        );
        assert_eq!(cfg.explicit_mode, None);
    }

    #[test]
    fn explicit_single_and_paired() {
        assert_eq!(
            parse(&["-s", "x.bam"])
                .unwrap()
                .validate()
                .unwrap()
                .explicit_mode,
            Some(false)
        );
        assert_eq!(
            parse(&["-p", "x.bam"])
                .unwrap()
                .validate()
                .unwrap()
                .explicit_mode,
            Some(true)
        );
    }

    #[test]
    fn both_single_and_paired_rejected_in_validate() {
        // Perl validates this (die), not a clap conflict — so it parses fine
        // and fails in validate() with the Perl message.
        let err = parse(&["-s", "-p", "x.bam"])
            .unwrap()
            .validate()
            .unwrap_err();
        assert!(matches!(err, BismarkFilterError::BothSingleAndPaired));
    }

    #[test]
    fn consecutive_threshold_mode() {
        let cfg = parse(&["--consecutive", "x.bam"])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(
            cfg.mode,
            FilterMode::Threshold {
                threshold: 3,
                consecutive: true
            }
        );
    }

    #[test]
    fn custom_threshold() {
        let cfg = parse(&["--threshold", "7", "x.bam"])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(
            cfg.mode,
            FilterMode::Threshold {
                threshold: 7,
                consecutive: false
            }
        );
    }

    #[test]
    fn threshold_zero_rejected_with_value_in_message() {
        let err = parse(&["--threshold", "0", "x.bam"])
            .unwrap()
            .validate()
            .unwrap_err();
        match err {
            BismarkFilterError::InvalidThreshold { value } => assert_eq!(value, 0),
            other => panic!("expected InvalidThreshold, got {other:?}"),
        }
    }

    #[test]
    fn threshold_negative_reaches_validation_not_parse_error() {
        // Signed type: -1 parses, then validation rejects it (Perl path).
        let cli = parse(&["--threshold", "-1", "x.bam"]).unwrap();
        let err = cli.validate().unwrap_err();
        match err {
            BismarkFilterError::InvalidThreshold { value } => assert_eq!(value, -1),
            other => panic!("expected InvalidThreshold(-1), got {other:?}"),
        }
    }

    #[test]
    fn percentage_mode_defaults_min_count_5() {
        let cfg = parse(&["--percentage_cutoff", "20", "x.bam"])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(
            cfg.mode,
            FilterMode::Percentage {
                cutoff: 20,
                minimum_count: 5
            }
        );
    }

    #[test]
    fn percentage_custom_min_count() {
        let cfg = parse(&["--percentage_cutoff", "20", "--minimum_count", "8", "x.bam"])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(
            cfg.mode,
            FilterMode::Percentage {
                cutoff: 20,
                minimum_count: 8
            }
        );
    }

    #[test]
    fn percentage_and_consecutive_mutually_exclusive() {
        let err = parse(&["--percentage_cutoff", "20", "--consecutive", "x.bam"])
            .unwrap()
            .validate()
            .unwrap_err();
        assert!(matches!(err, BismarkFilterError::PercentageAndConsecutive));
    }

    #[test]
    fn percentage_out_of_range_high() {
        let err = parse(&["--percentage_cutoff", "101", "x.bam"])
            .unwrap()
            .validate()
            .unwrap_err();
        assert!(matches!(err, BismarkFilterError::PercentageOutOfRange));
    }

    #[test]
    fn percentage_negative_reaches_range_check() {
        let err = parse(&["--percentage_cutoff", "-5", "x.bam"])
            .unwrap()
            .validate()
            .unwrap_err();
        assert!(matches!(err, BismarkFilterError::PercentageOutOfRange));
    }

    #[test]
    fn percentage_zero_and_hundred_are_valid() {
        assert!(
            parse(&["--percentage_cutoff", "0", "x.bam"])
                .unwrap()
                .validate()
                .is_ok()
        );
        assert!(
            parse(&["--percentage_cutoff", "100", "x.bam"])
                .unwrap()
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn minimum_count_zero_rejected() {
        let err = parse(&["--percentage_cutoff", "20", "--minimum_count", "0", "x.bam"])
            .unwrap()
            .validate()
            .unwrap_err();
        assert!(matches!(err, BismarkFilterError::InvalidMinimumCount));
    }

    #[test]
    fn threshold_ignored_under_percentage_mode_but_still_validated() {
        // Co-supplied --threshold (valid) is accepted; mode stays Percentage.
        let cfg = parse(&["--percentage_cutoff", "20", "--threshold", "7", "x.bam"])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(
            cfg.mode,
            FilterMode::Percentage {
                cutoff: 20,
                minimum_count: 5
            }
        );
        // A bad co-supplied --threshold still dies (unconditional validation).
        let err = parse(&["--percentage_cutoff", "20", "--threshold", "-1", "x.bam"])
            .unwrap()
            .validate()
            .unwrap_err();
        assert!(matches!(
            err,
            BismarkFilterError::InvalidThreshold { value: -1 }
        ));
    }

    #[test]
    fn samtools_path_accepted_and_ignored() {
        let cfg = parse(&["--samtools_path", "/usr/bin/samtools", "x.bam"])
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(cfg.files.len(), 1);
    }
}

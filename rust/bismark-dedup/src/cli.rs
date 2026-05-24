//! Command-line interface for `deduplicate_bismark_rs`.
//!
//! [`Cli`] is the clap-derived parser. [`Cli::validate`] resolves the
//! parsed arguments into a [`ResolvedConfig`] and rejects unsupported /
//! conflicting flag combinations:
//!
//! - `--barcode` / `--bclconvert` → [`BismarkDedupError::UnsupportedFlagV1`]
//!   (v1.1 will add these).
//! - `--representative` → [`BismarkDedupError::RepresentativeRemoved`]
//!   (Bismark deprecated this upstream).
//! - `--outfile` with multiple positional inputs but no `--multiple` →
//!   [`BismarkDedupError::OutfileWithMultipleInputs`].
//!
//! Flags accepted for Perl compatibility but ignored:
//! - `--parallel <N>` (silently — Perl is also silent on this flag).
//! - `--samtools_path <PATH>` (silently — bismark-io is pure Rust).
//!
//! The `--version` / `-V` flag emits a TG-style provenance string via
//! [`crate::version_string`]; clap's auto-version is disabled to allow
//! the custom format.

use std::path::PathBuf;

use clap::Parser;

use crate::error::BismarkDedupError;

/// Parsed command-line arguments. Use [`Cli::validate`] to convert to a
/// [`ResolvedConfig`] after parsing.
#[derive(Parser, Debug)]
#[command(
    name = "deduplicate_bismark_rs",
    about = "Remove PCR duplicate alignments from Bismark BAM/SAM/CRAM files",
    long_about = None,
    disable_version_flag = true
)]
pub struct Cli {
    /// Bismark BAM/SAM/CRAM file(s) to deduplicate.
    pub files: Vec<PathBuf>,

    /// Force single-end mode (auto-detected from @PG if not specified).
    #[arg(short = 's', long = "single", conflicts_with = "paired")]
    pub single: bool,

    /// Force paired-end mode (auto-detected from @PG if not specified).
    #[arg(short = 'p', long = "paired")]
    pub paired: bool,

    /// Output in BAM format (default; accepted for Perl compatibility).
    #[arg(long = "bam", conflicts_with = "sam")]
    pub bam: bool,

    /// Output in SAM format instead of BAM.
    #[arg(long = "sam")]
    pub sam: bool,

    /// CRAM reference FASTA (required when input or output is CRAM).
    #[arg(long = "cram_ref")]
    pub cram_ref: Option<PathBuf>,

    /// Custom output basename (any path prefix is stripped to basename).
    #[arg(short = 'o', long = "outfile")]
    pub outfile: Option<String>,

    /// Output directory (created if it does not exist).
    #[arg(long = "output_dir", default_value = ".")]
    pub output_dir: PathBuf,

    /// Treat all positional inputs as one combined sample (Perl `--multiple`).
    #[arg(long = "multiple")]
    pub multiple: bool,

    /// **Not supported in v1.0** — use the Perl `deduplicate_bismark` for
    /// UMI/RRBS mode.
    #[arg(long = "barcode", visible_alias = "umi")]
    pub barcode: bool,

    /// **Not supported in v1.0** — use the Perl `deduplicate_bismark` for
    /// bcl-convert-style internal UMIs.
    #[arg(long = "bclconvert")]
    pub bclconvert: bool,

    /// Number of threads (accepted for Perl compatibility, **ignored** in
    /// v1.0 — bismark-dedup is single-threaded; rayon support deferred to
    /// v1.1).
    #[arg(long = "parallel", default_value_t = 1u32)]
    pub parallel: u32,

    /// Removed in upstream Bismark; exits non-zero with a clear message.
    #[arg(long = "representative")]
    pub representative: bool,

    /// Path to a `samtools` binary (accepted for Perl compatibility,
    /// **ignored** — bismark-io is pure Rust, no subprocess).
    #[arg(long = "samtools_path")]
    pub samtools_path: Option<PathBuf>,

    /// Print version information and exit.
    #[arg(short = 'V', long = "version")]
    pub version: bool,
}

/// The resolved, validated subset of CLI arguments passed to the
/// pipeline. Constructed by [`Cli::validate`].
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// Positional inputs.
    pub files: Vec<PathBuf>,
    /// `Some(true)` for PE, `Some(false)` for SE, `None` for auto-detect.
    pub explicit_mode: Option<bool>,
    /// `true` if `--sam` was passed, else BAM output.
    pub sam_output: bool,
    /// CRAM reference path, if `--cram_ref` was supplied.
    pub cram_ref: Option<PathBuf>,
    /// User-supplied output basename, if `--outfile` was supplied.
    pub outfile: Option<String>,
    /// Output directory (defaults to `.`).
    pub output_dir: PathBuf,
    /// `--multiple` mode flag.
    pub multiple: bool,
}

impl Cli {
    /// Reject unsupported / conflicting flag combinations; return a
    /// [`ResolvedConfig`] on success.
    ///
    /// Reject (in priority order):
    /// 1. `--representative` → [`BismarkDedupError::RepresentativeRemoved`]
    /// 2. `--barcode` / `--bclconvert` → [`BismarkDedupError::UnsupportedFlagV1`]
    /// 3. Empty `files` → [`BismarkDedupError::NoInputFiles`]
    /// 4. `--outfile` with `>1` files and no `--multiple` →
    ///    [`BismarkDedupError::OutfileWithMultipleInputs`]
    ///
    /// `--single` / `--paired` are mutually exclusive (enforced by clap
    /// at parse time via `conflicts_with`). Neither set → `explicit_mode = None`
    /// (caller must auto-detect from the BAM header).
    pub fn validate(self) -> Result<ResolvedConfig, BismarkDedupError> {
        if self.representative {
            return Err(BismarkDedupError::RepresentativeRemoved);
        }
        if self.barcode {
            return Err(BismarkDedupError::UnsupportedFlagV1 { flag: "barcode" });
        }
        if self.bclconvert {
            return Err(BismarkDedupError::UnsupportedFlagV1 { flag: "bclconvert" });
        }
        if self.files.is_empty() {
            return Err(BismarkDedupError::NoInputFiles);
        }
        if self.outfile.is_some() && self.files.len() > 1 && !self.multiple {
            return Err(BismarkDedupError::OutfileWithMultipleInputs {
                n_files: self.files.len(),
            });
        }

        let explicit_mode = match (self.single, self.paired) {
            (true, false) => Some(false),
            (false, true) => Some(true),
            (false, false) => None,
            // clap rejects (true, true) at parse time via conflicts_with.
            (true, true) => unreachable!("clap conflicts_with prevents this"),
        };

        // --parallel and --samtools_path are silently accepted and ignored
        // (matches Perl's silence on --parallel). No warning needed.
        let _ = self.parallel;
        let _ = self.samtools_path;
        let _ = self.bam; // implicit default; --sam overrides.

        Ok(ResolvedConfig {
            files: self.files,
            explicit_mode,
            sam_output: self.sam,
            cram_ref: self.cram_ref,
            outfile: self.outfile,
            output_dir: self.output_dir,
            multiple: self.multiple,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut full = vec!["deduplicate_bismark_rs"];
        full.extend(args.iter().copied());
        Cli::try_parse_from(full)
    }

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_single_positional_input_default_mode() {
        let cli = parse(&["sample.bam"]).unwrap();
        assert_eq!(cli.files, vec![PathBuf::from("sample.bam")]);
        assert!(!cli.single && !cli.paired);
        assert!(!cli.bam && !cli.sam);
        assert!(!cli.multiple);
    }

    #[test]
    fn rejects_simultaneous_single_and_paired() {
        // clap's conflicts_with kicks in here.
        let err = parse(&["-s", "-p", "sample.bam"]).unwrap_err();
        assert!(
            err.to_string().contains("cannot be used with"),
            "got: {err}"
        );
    }

    #[test]
    fn rejects_simultaneous_bam_and_sam() {
        let err = parse(&["--bam", "--sam", "sample.bam"]).unwrap_err();
        assert!(
            err.to_string().contains("cannot be used with"),
            "got: {err}"
        );
    }

    #[test]
    fn validate_rejects_representative() {
        let cli = parse(&["--representative", "sample.bam"]).unwrap();
        let err = cli.validate().unwrap_err();
        assert!(matches!(err, BismarkDedupError::RepresentativeRemoved));
    }

    #[test]
    fn validate_rejects_barcode_with_v1_deferral() {
        let cli = parse(&["--barcode", "sample.bam"]).unwrap();
        let err = cli.validate().unwrap_err();
        assert!(matches!(
            err,
            BismarkDedupError::UnsupportedFlagV1 { flag: "barcode" }
        ));
    }

    #[test]
    fn validate_rejects_umi_alias_with_v1_deferral() {
        // --umi is a visible_alias for --barcode.
        let cli = parse(&["--umi", "sample.bam"]).unwrap();
        let err = cli.validate().unwrap_err();
        assert!(matches!(
            err,
            BismarkDedupError::UnsupportedFlagV1 { flag: "barcode" }
        ));
    }

    #[test]
    fn validate_rejects_bclconvert_with_v1_deferral() {
        let cli = parse(&["--bclconvert", "sample.bam"]).unwrap();
        let err = cli.validate().unwrap_err();
        assert!(matches!(
            err,
            BismarkDedupError::UnsupportedFlagV1 { flag: "bclconvert" }
        ));
    }

    #[test]
    fn validate_rejects_no_positional_inputs() {
        let cli = parse(&[]).unwrap();
        let err = cli.validate().unwrap_err();
        assert!(matches!(err, BismarkDedupError::NoInputFiles));
    }

    #[test]
    fn validate_rejects_outfile_with_multiple_inputs_no_multiple_flag() {
        let cli = parse(&["--outfile", "x.bam", "a.bam", "b.bam"]).unwrap();
        let err = cli.validate().unwrap_err();
        match err {
            BismarkDedupError::OutfileWithMultipleInputs { n_files } => {
                assert_eq!(n_files, 2);
            }
            other => panic!("expected OutfileWithMultipleInputs, got {other:?}"),
        }
    }

    #[test]
    fn validate_accepts_outfile_with_multiple_inputs_when_multiple_set() {
        let cli = parse(&["--multiple", "--outfile", "x.bam", "a.bam", "b.bam"]).unwrap();
        let config = cli.validate().unwrap();
        assert_eq!(config.files.len(), 2);
        assert!(config.multiple);
        assert_eq!(config.outfile.as_deref(), Some("x.bam"));
    }

    #[test]
    fn validate_explicit_mode_single() {
        let cli = parse(&["-s", "sample.bam"]).unwrap();
        let config = cli.validate().unwrap();
        assert_eq!(config.explicit_mode, Some(false));
    }

    #[test]
    fn validate_explicit_mode_paired() {
        let cli = parse(&["--paired", "sample.bam"]).unwrap();
        let config = cli.validate().unwrap();
        assert_eq!(config.explicit_mode, Some(true));
    }

    #[test]
    fn validate_explicit_mode_none_when_neither_flag_set() {
        let cli = parse(&["sample.bam"]).unwrap();
        let config = cli.validate().unwrap();
        assert_eq!(config.explicit_mode, None);
    }

    #[test]
    fn parallel_and_samtools_path_silently_accepted() {
        let cli = parse(&[
            "--parallel",
            "4",
            "--samtools_path",
            "/usr/bin/samtools",
            "sample.bam",
        ])
        .unwrap();
        assert_eq!(cli.parallel, 4);
        assert_eq!(cli.samtools_path, Some(PathBuf::from("/usr/bin/samtools")));
        let config = cli.validate().unwrap();
        assert_eq!(config.files.len(), 1);
    }

    #[test]
    fn output_dir_defaults_to_current_directory() {
        let cli = parse(&["sample.bam"]).unwrap();
        let config = cli.validate().unwrap();
        assert_eq!(config.output_dir, PathBuf::from("."));
    }

    #[test]
    fn cram_ref_passed_through_to_config() {
        let cli = parse(&["--cram_ref", "/path/to/genome.fa", "sample.cram"]).unwrap();
        let config = cli.validate().unwrap();
        assert_eq!(config.cram_ref, Some(PathBuf::from("/path/to/genome.fa")));
    }

    #[test]
    fn version_flag_parses_without_files() {
        // `--version` is a soft check by the caller in main(); clap doesn't
        // reject the absence of positional files at parse time.
        let cli = parse(&["--version"]).unwrap();
        assert!(cli.version);
        assert!(cli.files.is_empty());
    }
}

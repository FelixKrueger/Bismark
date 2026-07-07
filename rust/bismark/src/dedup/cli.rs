//! Command-line interface for `deduplicate_bismark`.
//!
//! [`Cli`] is the clap-derived parser. [`Cli::validate`] resolves the
//! parsed arguments into a [`ResolvedConfig`] and rejects unsupported /
//! conflicting flag combinations:
//!
//! - `--barcode` / `--umi` → engages UMI-aware dedup (v1.2+; tail-of-qname
//!   format).
//! - `--bclconvert` → engages UMI-aware dedup with bcl-convert internal
//!   UMI format. Implies `--barcode` semantically (matches Perl's
//!   `$rrbs = 1` auto-coupling at deduplicate_bismark:1377).
//! - `--representative` → [`BismarkDedupError::RepresentativeRemoved`]
//!   (Bismark deprecated this upstream).
//! - `--outfile` with multiple positional inputs but no `--multiple` →
//!   [`BismarkDedupError::OutfileWithMultipleInputs`].
//!
//! ## UMI mode caveat
//!
//! `bismark-dedup` v1.2 trusts the user's UMI flag choice — it does NOT
//! auto-detect qname format. Running `--barcode` on bcl-convert qnames
//! silently extracts the wrong tail (the i7 tail, not the UMI),
//! producing nonsense dedup keys. Match the flag to your data: if your
//! reads come from bcl-convert, you MUST pass `--bclconvert`. (v1.3
//! plans a sniff-first-record auto-detect; tracked.)
//!
//! Flags accepted for Perl compatibility but ignored:
//! - `--parallel <N>` (silently — Perl is also silent on this flag).
//! - `--samtools_path <PATH>` (silently — bismark-io is pure Rust).
//!
//! The `--version` / `-V` flag emits a TG-style provenance string via
//! [`crate::dedup::version_string`]; clap's auto-version is disabled to allow
//! the custom format.

use std::path::PathBuf;

use clap::Parser;

use crate::dedup::error::BismarkDedupError;

/// `--help` footer: the per-tool last-modified date (git commit date of this
/// crate, embedded by `build.rs` via `crate::meta::last_modified_date`).
const HELP_FOOTER: &str = concat!("Last modified: ", env!("BISMARK_LAST_MODIFIED"));

/// Parsed command-line arguments. Use [`Cli::validate`] to convert to a
/// [`ResolvedConfig`] after parsing.
#[derive(Parser, Debug)]
#[command(
    name = "deduplicate_bismark",
    about = "Remove PCR duplicate alignments from Bismark BAM/SAM/CRAM files",
    long_about = None,
    disable_version_flag = true,
    after_help = HELP_FOOTER
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

    /// UMI/RRBS dedup mode: the UMI is the tail-of-qname token after the
    /// last `:`, e.g. `MISEQ:...:CTCCTTAG` (8-mer ACGT). New in v1.2;
    /// matches Perl `deduplicate_bismark --barcode` (line 659).
    ///
    /// **Warning**: this trusts the user's flag — running `--barcode` on
    /// bcl-convert qnames silently extracts the i7 tail, NOT the UMI. If
    /// your reads come from bcl-convert, pass `--bclconvert` instead.
    #[arg(long = "barcode", visible_alias = "umi")]
    pub barcode: bool,

    /// bcl-convert UMI dedup mode: the UMI is at an internal position in
    /// the qname, e.g. `A00001:...:CAAGAG_1:N:0:AATGACGC`. New in v1.2;
    /// matches Perl `deduplicate_bismark --bclconvert` (line 650).
    /// Engages UMI mode unconditionally (the Rust port does NOT have the
    /// v0.25.1 released-Perl `--bclconvert`-alone-falls-through bug).
    #[arg(long = "bclconvert")]
    pub bclconvert: bool,

    /// Number of BGZF (de)compression worker threads for BAM I/O.
    /// `1` = single-threaded (the v1.0 path). `>= 2` spawns N BGZF
    /// workers for the input reader AND the output writer via noodles'
    /// `MultithreadedReader`/`MultithreadedWriter`; output is
    /// byte-identical to `--parallel 1`. CRAM I/O falls back to
    /// single-threaded with a one-line stderr warning. Measured ~4.9×
    /// speedup at N=4 on 10M PE WGBS; N >= 8 typically saturates (the
    /// dedup state itself is serial). Reject `--parallel 0`.
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

/// UMI dedup mode selected at the CLI. New in v1.2 for the v1.2 UMI/RRBS
/// epic. `None` (in the `umi_mode` field) = position-only dedup
/// (the v1.0/v1.1 default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UmiMode {
    /// `--barcode` / `--umi`: UMI is the tail-of-qname token after the
    /// last `:`. Bismark-io extractor: [`crate::io::umi::extract_barcode`].
    Barcode,
    /// `--bclconvert`: UMI is at an internal position in the qname.
    /// Bismark-io extractor: [`crate::io::umi::extract_bclconvert`].
    /// Wins over `--barcode` if both flags are passed.
    Bclconvert,
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
    /// BGZF decoder/encoder worker thread count. `1` = single-threaded
    /// (the v1.0 default; honoured by [`pipeline::run_single`]).
    /// `> 1` enables BGZF parallel decode + encode for BAM input/output
    /// via [`pipeline::run_single_parallel`]. `0` is rejected at
    /// [`Cli::validate`]. Ignored for SAM (no BGZF) and CRAM (CRAM
    /// container decode isn't BGZF-based; v1.1 falls back to
    /// single-threaded + emits a one-line stderr warning).
    pub parallel: usize,
    /// UMI dedup mode (v1.2+). `None` = position-only (v1.0/v1.1
    /// behaviour, no behaviour change for non-UMI callers).
    pub umi_mode: Option<UmiMode>,
}

impl Cli {
    /// Reject unsupported / conflicting flag combinations; return a
    /// [`ResolvedConfig`] on success.
    ///
    /// Reject (in priority order):
    /// 1. `--representative` → [`BismarkDedupError::RepresentativeRemoved`]
    /// 2. `--parallel 0` → [`BismarkDedupError::InvalidParallelValue`]
    ///    (clap's `u32` parser accepts 0; explicit check needed here)
    /// 3. Empty `files` → [`BismarkDedupError::NoInputFiles`]
    /// 4. `--outfile` with `>1` files and no `--multiple` →
    ///    [`BismarkDedupError::OutfileWithMultipleInputs`]
    ///
    /// `--barcode` / `--umi` / `--bclconvert` are accepted in v1.2+ and
    /// engage UMI-aware dedup. If both `--barcode` and `--bclconvert` are
    /// set, `--bclconvert` wins (matches Perl's precedence at
    /// `deduplicate_bismark:1377`).
    ///
    /// `--single` / `--paired` are mutually exclusive (enforced by clap
    /// at parse time via `conflicts_with`). Neither set → `explicit_mode = None`
    /// (caller must auto-detect from the BAM header).
    pub fn validate(self) -> Result<ResolvedConfig, BismarkDedupError> {
        if self.representative {
            return Err(BismarkDedupError::RepresentativeRemoved);
        }
        if self.parallel == 0 {
            return Err(BismarkDedupError::InvalidParallelValue { value: 0 });
        }
        if self.files.is_empty() {
            return Err(BismarkDedupError::NoInputFiles);
        }
        if self.outfile.is_some() && self.files.len() > 1 && !self.multiple {
            return Err(BismarkDedupError::OutfileWithMultipleInputs {
                n_files: self.files.len(),
            });
        }

        // Resolve UMI mode (Phase B of v1.2 UMI epic):
        //   --bclconvert wins over --barcode if both are set (matches
        //   Perl's auto-coupling at deduplicate_bismark:1377).
        let umi_mode = if self.bclconvert {
            Some(UmiMode::Bclconvert)
        } else if self.barcode {
            Some(UmiMode::Barcode)
        } else {
            None
        };

        let explicit_mode = match (self.single, self.paired) {
            (true, false) => Some(false),
            (false, true) => Some(true),
            (false, false) => None,
            // clap rejects (true, true) at parse time via conflicts_with.
            (true, true) => unreachable!("clap conflicts_with prevents this"),
        };

        // --samtools_path is silently accepted and ignored (no warning;
        // bismark-io is pure-Rust). --parallel is honoured in v1.1.
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
            parallel: self.parallel as usize,
            umi_mode,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut full = vec!["deduplicate_bismark"];
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

    // ─── Phase B (v1.2): --barcode / --umi / --bclconvert now resolve to
    // UmiMode rather than rejecting with UnsupportedFlagV1.

    #[test]
    fn validate_barcode_resolves_to_umi_mode_barcode() {
        let cli = parse(&["--barcode", "sample.bam"]).unwrap();
        let config = cli.validate().unwrap();
        assert_eq!(config.umi_mode, Some(UmiMode::Barcode));
    }

    #[test]
    fn validate_umi_alias_resolves_to_umi_mode_barcode() {
        // --umi is a visible_alias for --barcode.
        let cli = parse(&["--umi", "sample.bam"]).unwrap();
        let config = cli.validate().unwrap();
        assert_eq!(config.umi_mode, Some(UmiMode::Barcode));
    }

    #[test]
    fn validate_bclconvert_resolves_to_umi_mode_bclconvert() {
        let cli = parse(&["--bclconvert", "sample.bam"]).unwrap();
        let config = cli.validate().unwrap();
        assert_eq!(config.umi_mode, Some(UmiMode::Bclconvert));
    }

    #[test]
    fn validate_bclconvert_plus_barcode_bclconvert_wins() {
        // Both flags set → bclconvert wins (matches Perl's auto-coupling
        // precedence at deduplicate_bismark:1377).
        let cli = parse(&["--bclconvert", "--barcode", "sample.bam"]).unwrap();
        let config = cli.validate().unwrap();
        assert_eq!(config.umi_mode, Some(UmiMode::Bclconvert));
    }

    #[test]
    fn validate_no_umi_flag_yields_none() {
        let cli = parse(&["sample.bam"]).unwrap();
        let config = cli.validate().unwrap();
        assert_eq!(config.umi_mode, None);
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

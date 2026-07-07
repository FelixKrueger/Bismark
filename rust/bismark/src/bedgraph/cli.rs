//! Command-line interface for `bismark2bedGraph`.
//!
//! [`Cli`] is the clap-derived parser; [`Cli::validate`] resolves it into a
//! [`ResolvedConfig`], rejecting unsupported / conflicting flag combos with
//! Perl-parity error messages (see [`crate::bedgraph::error`]).
//!
//! Flags accepted for Perl compatibility but **ignored** at runtime
//! (SPEC §1.1 D2/D3 — the in-memory aggregator makes them unnecessary):
//! - `--counts` (Perl no-op too — counts are always emitted)
//! - `--buffer_size <SIZE>` (format-validated for CLI parity, then ignored)
//! - `--ample_memory` (we always aggregate in memory)
//! - `--gazillion` / `--scaffolds` (no open-filehandle limit to work around)
//! - `--remove_spaces` (the read-id field is unused, so it has no effect on
//!   bedGraph/coverage output; the Perl `.spaces_removed.txt` intermediate
//!   is not produced)
//!
//! `--version` is handled in `main` (clap's auto-version is disabled to
//! emit a custom provenance string).

use std::path::PathBuf;

use clap::Parser;

use crate::bedgraph::error::BismarkBedgraphError;
use crate::bedgraph::filename;

/// `--help` footer: the per-tool last-modified date (git commit date of this
/// crate, embedded by `build.rs` via `crate::meta::last_modified_date`).
const HELP_FOOTER: &str = concat!("Last modified: ", env!("BISMARK_LAST_MODIFIED"));

/// Parsed command-line arguments. Use [`Cli::validate`] after parsing.
#[derive(Parser, Debug)]
#[command(
    name = "bismark2bedGraph",
    about = "Generate a sorted bedGraph + coverage file from Bismark methylation-extractor output",
    long_about = None,
    disable_version_flag = true,
    after_help = HELP_FOOTER
)]
pub struct Cli {
    /// Methylation-extractor call file(s) (`.txt` or `.txt.gz`). Default
    /// (CpG-only) mode uses only files whose basename starts with `CpG`.
    pub files: Vec<PathBuf>,

    /// Output bedGraph filename (mandatory). No path separators — use
    /// `--dir` for the directory.
    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,

    /// Output directory (created if missing; defaults to the CWD).
    #[arg(long = "dir")]
    pub dir: Option<PathBuf>,

    /// Inputs have no version-header line — do not drop the first line of
    /// each file (default: the first line is treated as a header and skipped).
    #[arg(long = "no_header")]
    pub no_header: bool,

    /// Minimum read coverage before a position is reported (default: 1).
    // allow_hyphen_values: Perl `GetOptions cutoff=i` accepts negatives, so a
    // value like `-3` reaches our `> 0` check (a clear error) instead of being
    // rejected by clap as an unknown flag.
    #[arg(long = "cutoff", allow_hyphen_values = true)]
    pub cutoff: Option<i64>,

    /// Replace whitespace in read IDs with underscores (accepted for Perl
    /// compatibility; has no effect on the output).
    #[arg(long = "remove_spaces")]
    pub remove_spaces: bool,

    /// Include per-position methylation counts in the coverage file
    /// (always on; accepted for Perl compatibility).
    #[arg(long = "counts")]
    pub counts: bool,

    /// Process all cytosine contexts (CpG, CHG, CHH), not just CpG.
    #[arg(long = "CX", visible_alias = "CX_context")]
    pub cx: bool,

    /// Sort buffer size (accepted for Perl compatibility; an in-memory sort
    /// is always used).
    #[arg(long = "buffer_size")]
    pub buffer_size: Option<String>,

    /// Many-scaffold workaround (accepted for Perl compatibility; unnecessary
    /// here — the in-memory aggregator has no open-filehandle limit).
    #[arg(long = "gazillion", visible_alias = "scaffolds")]
    pub gazillion: bool,

    /// In-memory array sort (accepted for Perl compatibility; an in-memory
    /// sort is always used).
    #[arg(long = "ample_memory")]
    pub ample_memory: bool,

    /// Also write a 0-based half-open coverage file (`.bismark.zero.cov`,
    /// plain text).
    #[arg(long = "zero_based")]
    pub zero_based: bool,

    /// Also write a UCSC-compatible bedGraph (`chr` prefix, `MT`→`chrM`).
    #[arg(long = "ucsc")]
    pub ucsc: bool,

    /// Print the long help text and exit (Perl `--man` alias of `--help`).
    #[arg(long = "man")]
    pub man: bool,

    /// Print version information and exit.
    #[arg(short = 'V', long = "version")]
    pub version: bool,
}

/// Resolved, validated configuration consumed by the pipeline. Output
/// filenames are pre-derived from the normalized `-o` value (SPEC §4).
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// Positional input files, in the **exact argv order given** — never
    /// reordered (chr ownership depends on it; SPEC §2.1B C1).
    pub files: Vec<PathBuf>,
    /// Output directory (defaults to `.`).
    pub output_dir: PathBuf,
    /// Normalized bedGraph filename (always ends `.gz`).
    pub bedgraph_name: String,
    /// Coverage filename (`.bismark.cov.gz`).
    pub coverage_name: String,
    /// Zero-based coverage filename (plain `.zero.cov`; only written if
    /// [`Self::zero_based`]).
    pub zero_name: String,
    /// UCSC bedGraph filename (only written if [`Self::ucsc`]).
    pub ucsc_name: String,
    /// Minimum total coverage to emit a position (≥ 1).
    pub cutoff: u32,
    /// `--CX`: use all input files / all cytosine contexts.
    pub cx: bool,
    /// `--no_header`: first input line is data, not a version header.
    pub no_header: bool,
    /// `--remove_spaces`: accepted; no effect on output (id unused).
    pub remove_spaces: bool,
    /// `--zero_based`: also emit the plain `.zero.cov` file.
    pub zero_based: bool,
    /// `--ucsc`: also emit the UCSC bedGraph.
    pub ucsc: bool,
}

/// Validate a `--buffer_size` value against Perl's accepted forms
/// (`bismark2bedGraph:766`): `\d+%` or `\d+[KMGT]`. Note Perl requires a
/// suffix — a bare number like `2048` is rejected.
fn valid_buffer_size(s: &str) -> bool {
    if let Some(digits) = s.strip_suffix('%') {
        return !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit());
    }
    if let Some(last) = s.chars().last()
        && matches!(last, 'K' | 'M' | 'G' | 'T')
    {
        let digits = &s[..s.len() - 1];
        return !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit());
    }
    false
}

impl Cli {
    /// Reject unsupported / conflicting flag combinations and derive the
    /// output filenames. Validation order mirrors Perl `process_commandline`
    /// (`bismark2bedGraph:680-794`):
    ///
    /// 1. empty input files → [`BismarkBedgraphError::NoInputFiles`]
    /// 2. `-o` absent → [`BismarkBedgraphError::BedGraphOutputRequired`]
    /// 3. `-o` contains `/` → [`BismarkBedgraphError::OutputHasPath`]
    /// 4. `--cutoff <= 0` → [`BismarkBedgraphError::BadCutoff`]
    /// 5. explicit `--buffer_size`: `--ample_memory` mutex → error; bad
    ///    format → [`BismarkBedgraphError::BadBufferSize`]
    /// 6. `--gazillion` + `--ample_memory` → error
    pub fn validate(self) -> Result<ResolvedConfig, BismarkBedgraphError> {
        if self.files.is_empty() {
            return Err(BismarkBedgraphError::NoInputFiles);
        }

        let output = self
            .output
            .ok_or(BismarkBedgraphError::BedGraphOutputRequired)?;
        if output.contains('/') {
            return Err(BismarkBedgraphError::OutputHasPath);
        }

        let cutoff = match self.cutoff {
            Some(c) => {
                if c <= 0 {
                    return Err(BismarkBedgraphError::BadCutoff { value: c });
                }
                c as u32
            }
            None => 1,
        };

        // Perl gates the buffer-size checks on the flag being *explicitly*
        // given (default "2G" is never validated). The `--ample_memory`
        // mutex lives inside that block (`:762-764`).
        if let Some(ref s) = self.buffer_size {
            if self.ample_memory {
                return Err(BismarkBedgraphError::AmpleMemoryWithBufferSize);
            }
            if !valid_buffer_size(s) {
                return Err(BismarkBedgraphError::BadBufferSize { value: s.clone() });
            }
        }

        if self.gazillion && self.ample_memory {
            return Err(BismarkBedgraphError::AmpleMemoryWithGazillion);
        }

        let bedgraph_name = filename::normalize_bedgraph_name(&output);
        let coverage_name = filename::coverage_name(&bedgraph_name);
        let zero_name = filename::zero_name(&bedgraph_name);
        let ucsc_name = filename::ucsc_name(&bedgraph_name);
        let output_dir = self.dir.unwrap_or_else(|| PathBuf::from("."));

        // Accepted-but-ignored flags (SPEC §1.1 D2/D3, §3).
        let _ = (
            self.counts,
            self.buffer_size,
            self.ample_memory,
            self.gazillion,
        );

        Ok(ResolvedConfig {
            files: self.files,
            output_dir,
            bedgraph_name,
            coverage_name,
            zero_name,
            ucsc_name,
            cutoff,
            cx: self.cx,
            no_header: self.no_header,
            remove_spaces: self.remove_spaces,
            zero_based: self.zero_based,
            ucsc: self.ucsc,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut full = vec!["bismark2bedGraph"];
        full.extend(args.iter().copied());
        Cli::try_parse_from(full)
    }

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn cx_context_alias_parses() {
        let cli = parse(&["--CX_context", "-o", "x.bedGraph", "CpG_OT_s.txt"]).unwrap();
        assert!(cli.cx);
        let cli2 = parse(&["--CX", "-o", "x.bedGraph", "CpG_OT_s.txt"]).unwrap();
        assert!(cli2.cx);
    }

    #[test]
    fn scaffolds_alias_parses() {
        let cli = parse(&["--scaffolds", "-o", "x.bedGraph", "CpG_OT_s.txt"]).unwrap();
        assert!(cli.gazillion);
    }

    #[test]
    fn validate_rejects_no_positional_inputs() {
        let cli = parse(&["-o", "x.bedGraph"]).unwrap();
        assert!(matches!(
            cli.validate().unwrap_err(),
            BismarkBedgraphError::NoInputFiles
        ));
    }

    #[test]
    fn validate_rejects_missing_output() {
        let cli = parse(&["CpG_OT_s.txt"]).unwrap();
        assert!(matches!(
            cli.validate().unwrap_err(),
            BismarkBedgraphError::BedGraphOutputRequired
        ));
    }

    #[test]
    fn validate_rejects_output_with_path() {
        let cli = parse(&["-o", "sub/x.bedGraph", "CpG_OT_s.txt"]).unwrap();
        assert!(matches!(
            cli.validate().unwrap_err(),
            BismarkBedgraphError::OutputHasPath
        ));
    }

    #[test]
    fn validate_rejects_zero_cutoff() {
        let cli = parse(&["--cutoff", "0", "-o", "x.bedGraph", "CpG_OT_s.txt"]).unwrap();
        match cli.validate().unwrap_err() {
            BismarkBedgraphError::BadCutoff { value } => assert_eq!(value, 0),
            other => panic!("expected BadCutoff, got {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_negative_cutoff() {
        let cli = parse(&["--cutoff", "-3", "-o", "x.bedGraph", "CpG_OT_s.txt"]).unwrap();
        assert!(matches!(
            cli.validate().unwrap_err(),
            BismarkBedgraphError::BadCutoff { value: -3 }
        ));
    }

    #[test]
    fn validate_default_cutoff_is_one() {
        let cli = parse(&["-o", "x.bedGraph", "CpG_OT_s.txt"]).unwrap();
        assert_eq!(cli.validate().unwrap().cutoff, 1);
    }

    #[test]
    fn validate_buffer_size_mutex_with_ample_memory() {
        let cli = parse(&[
            "--buffer_size",
            "2G",
            "--ample_memory",
            "-o",
            "x.bedGraph",
            "CpG_OT_s.txt",
        ])
        .unwrap();
        assert!(matches!(
            cli.validate().unwrap_err(),
            BismarkBedgraphError::AmpleMemoryWithBufferSize
        ));
    }

    #[test]
    fn validate_gazillion_mutex_with_ample_memory() {
        let cli = parse(&[
            "--gazillion",
            "--ample_memory",
            "-o",
            "x.bedGraph",
            "CpG_OT_s.txt",
        ])
        .unwrap();
        assert!(matches!(
            cli.validate().unwrap_err(),
            BismarkBedgraphError::AmpleMemoryWithGazillion
        ));
    }

    #[test]
    fn validate_rejects_bad_buffer_size() {
        // Bare number with no suffix is rejected by Perl.
        let cli = parse(&["--buffer_size", "2048", "-o", "x.bedGraph", "CpG_OT_s.txt"]).unwrap();
        assert!(matches!(
            cli.validate().unwrap_err(),
            BismarkBedgraphError::BadBufferSize { .. }
        ));
    }

    #[test]
    fn validate_accepts_good_buffer_sizes() {
        for ok in ["2G", "20%", "512M", "1024K", "1T", "50%"] {
            let cli = parse(&["--buffer_size", ok, "-o", "x.bedGraph", "CpG_OT_s.txt"]).unwrap();
            assert!(cli.validate().is_ok(), "buffer_size {ok} should validate");
        }
    }

    #[test]
    fn valid_buffer_size_helper() {
        assert!(valid_buffer_size("2G"));
        assert!(valid_buffer_size("20%"));
        assert!(valid_buffer_size("1024K"));
        assert!(!valid_buffer_size("2048")); // no suffix
        assert!(!valid_buffer_size("G")); // no digits
        assert!(!valid_buffer_size("%")); // no digits
        assert!(!valid_buffer_size("2X")); // bad suffix
        assert!(!valid_buffer_size("2.5G")); // non-digit
    }

    #[test]
    fn validate_derives_output_names() {
        let cli = parse(&["-o", "foo.bedGraph", "CpG_OT_s.txt"]).unwrap();
        let cfg = cli.validate().unwrap();
        assert_eq!(cfg.bedgraph_name, "foo.bedGraph.gz");
        assert_eq!(cfg.coverage_name, "foo.bismark.cov.gz");
        assert_eq!(cfg.zero_name, "foo.bedGraph.gz.bismark.zero.cov");
        assert_eq!(cfg.ucsc_name, "foo.bedGraph_UCSC.bedGraph.gz");
        assert_eq!(cfg.output_dir, PathBuf::from("."));
        assert_eq!(cfg.cutoff, 1);
    }

    #[test]
    fn validate_preserves_argv_file_order() {
        // Chr ownership depends on argv order — never reorder (SPEC C1).
        let cli = parse(&["-o", "x.bedGraph", "CpG_OB_s.txt", "CpG_OT_s.txt"]).unwrap();
        let cfg = cli.validate().unwrap();
        assert_eq!(
            cfg.files,
            vec![PathBuf::from("CpG_OB_s.txt"), PathBuf::from("CpG_OT_s.txt")]
        );
    }

    #[test]
    fn version_and_man_parse_without_required_args() {
        assert!(parse(&["--version"]).unwrap().version);
        assert!(parse(&["--man"]).unwrap().man);
    }
}

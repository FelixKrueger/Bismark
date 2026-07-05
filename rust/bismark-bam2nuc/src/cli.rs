//! Command-line interface for `bam2nuc_rs`.
//!
//! [`Cli`] is the clap-derived parser; [`Cli::validate`] resolves it into a
//! [`ResolvedConfig`], reproducing Perl `bam2nuc`'s `process_commandline`
//! (`:346-465`) rules:
//! - `-g/--genome_folder` is **mandatory** (Perl dies otherwise).
//! - no input files AND not `--genomic_composition_only` → error (Perl prints
//!   help + exits).
//! - `--dir` defaults to `""` (a path *prefix*; Perl `:411-420` — NOT made
//!   absolute, only a trailing `/` appended when non-empty).
//! - `--parent_dir` defaults to `getcwd()` (Perl `:403-405`).
//! - `--samtools_path` is **accepted-but-ignored** (the Rust port reads BAM via
//!   pure-Rust noodles; SPEC Q4 / D1). It is parsed so the `bismark` pipeline's
//!   invocation doesn't error, then dropped here (never stored in the config,
//!   never validated for existence — D1a divergence from Perl).

use std::path::PathBuf;

use clap::Parser;

use crate::error::BismarkBam2nucError;

/// Parsed command-line arguments. Use [`Cli::validate`] to convert to a
/// [`ResolvedConfig`].
#[derive(Parser, Debug)]
#[command(
    name = "bam2nuc_rs",
    about = "Calculate mono- and di-nucleotide coverage of a Bismark alignment (genomic-sequence composition QC)",
    long_about = None,
    disable_version_flag = true
)]
pub struct Cli {
    /// Input alignment file(s) in BAM format (`*.bam`). One stats file is
    /// written per input. Not required with `--genomic_composition_only`.
    #[arg(value_name = "INPUT")]
    pub inputs: Vec<PathBuf>,

    /// Genome FASTA directory (mandatory; full path). Accepts `.fa`/`.fasta`
    /// and their gzipped forms.
    #[arg(short = 'g', long = "genome_folder")]
    pub genome_folder: Option<PathBuf>,

    /// Output directory (default: current directory).
    #[arg(long = "dir")]
    pub dir: Option<PathBuf>,

    /// Base directory (default: cwd). Accepted for Perl compatibility; the Rust
    /// port does not `chdir`, so paths resolve against the real cwd.
    #[arg(long = "parent_dir")]
    pub parent_dir: Option<PathBuf>,

    /// Path to a Samtools installation. **Accepted but ignored** — the Rust
    /// port reads BAM with pure-Rust noodles (no samtools subprocess).
    #[arg(long = "samtools_path")]
    pub samtools_path: Option<String>,

    /// Only calculate + write the genomic composition table
    /// (`genomic_nucleotide_frequencies.txt`) and exit; no input files needed.
    #[arg(long = "genomic_composition_only")]
    pub genomic_composition_only: bool,

    /// Print version information and exit.
    #[arg(short = 'V', long = "version")]
    pub version: bool,
}

/// Validated, resolved configuration consumed by [`crate::run`].
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// Positional input alignment files (argv order preserved).
    pub inputs: Vec<PathBuf>,
    /// Genome FASTA directory.
    pub genome_folder: PathBuf,
    /// Output directory as a path *prefix* (empty string = cwd-relative; else a
    /// trailing-`/` value — matches Perl's `"${output_dir}${file}"` concat).
    pub output_dir: String,
    /// Base directory (Perl compat; the Rust port does not `chdir`).
    pub parent_dir: PathBuf,
    /// `--genomic_composition_only` requested.
    pub genomic_composition_only: bool,
}

impl Cli {
    /// Reject invalid argument combinations and resolve defaults, mirroring
    /// Perl `process_commandline`.
    pub fn validate(self) -> Result<ResolvedConfig, BismarkBam2nucError> {
        let genome_folder = self
            .genome_folder
            .ok_or(BismarkBam2nucError::MissingGenomeFolder)?;

        // No input files is only allowed in --genomic_composition_only mode.
        if self.inputs.is_empty() && !self.genomic_composition_only {
            return Err(BismarkBam2nucError::MissingInput);
        }

        let output_dir = resolve_output_dir(self.dir);
        let parent_dir = match self.parent_dir {
            Some(p) => p,
            None => std::env::current_dir()?,
        };

        // self.samtools_path is intentionally dropped (accepted-but-ignored).

        Ok(ResolvedConfig {
            inputs: self.inputs,
            genome_folder,
            output_dir,
            parent_dir,
            genomic_composition_only: self.genomic_composition_only,
        })
    }
}

/// Resolve `--dir` to a path *prefix* with a trailing `/`. `None` → empty
/// string (cwd-relative). An explicit empty string stays empty (Perl `:412`
/// special-cases the empty string Bismark passes). NOT made absolute — Perl
/// `bam2nuc` keeps `output_dir` relative (only file *content* is byte-gated).
fn resolve_output_dir(dir: Option<PathBuf>) -> String {
    match dir {
        None => String::new(),
        Some(d) => {
            let mut s = d.to_string_lossy().into_owned();
            if !s.is_empty() && !s.ends_with('/') {
                s.push('/');
            }
            s
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut full = vec!["bam2nuc_rs"];
        full.extend(args.iter().copied());
        Cli::try_parse_from(full)
    }

    fn cli(args: &[&str]) -> Cli {
        parse(args).unwrap()
    }

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_inputs_and_genome() {
        let c = cli(&["-g", "gdir", "a.bam", "b.bam"]);
        assert_eq!(
            c.genome_folder.as_deref(),
            Some(std::path::Path::new("gdir"))
        );
        assert_eq!(c.inputs.len(), 2);
        assert_eq!(c.inputs[0], PathBuf::from("a.bam"));
        assert_eq!(c.inputs[1], PathBuf::from("b.bam"));
    }

    #[test]
    fn parses_all_options() {
        let c = cli(&[
            "-g",
            "gdir",
            "--dir",
            "out",
            "--parent_dir",
            "pd",
            "--samtools_path",
            "/usr/bin/samtools",
            "--genomic_composition_only",
            "x.bam",
        ]);
        assert_eq!(c.dir.as_deref(), Some(std::path::Path::new("out")));
        assert_eq!(c.parent_dir.as_deref(), Some(std::path::Path::new("pd")));
        assert_eq!(c.samtools_path.as_deref(), Some("/usr/bin/samtools"));
        assert!(c.genomic_composition_only);
    }

    #[test]
    fn long_genome_folder_alias_parses() {
        assert!(
            cli(&["--genome_folder", "g", "x.bam"])
                .genome_folder
                .is_some()
        );
    }

    #[test]
    fn rejects_missing_genome() {
        let e = cli(&["x.bam"]).validate().unwrap_err();
        assert!(matches!(e, BismarkBam2nucError::MissingGenomeFolder));
    }

    #[test]
    fn rejects_no_input_without_genomic_composition_only() {
        let e = cli(&["-g", "g"]).validate().unwrap_err();
        assert!(matches!(e, BismarkBam2nucError::MissingInput));
    }

    #[test]
    fn genomic_composition_only_needs_no_input() {
        let c = cli(&["-g", "g", "--genomic_composition_only"])
            .validate()
            .unwrap();
        assert!(c.genomic_composition_only);
        assert!(c.inputs.is_empty());
    }

    #[test]
    fn output_dir_defaults_to_empty_prefix() {
        let c = cli(&["-g", "g", "x.bam"]).validate().unwrap();
        assert_eq!(c.output_dir, "");
    }

    #[test]
    fn output_dir_gets_trailing_slash_but_not_absolute() {
        let c = cli(&["-g", "g", "--dir", "out", "x.bam"])
            .validate()
            .unwrap();
        assert_eq!(c.output_dir, "out/"); // relative + trailing slash, NOT absolute
    }

    #[test]
    fn output_dir_keeps_existing_trailing_slash() {
        let c = cli(&["-g", "g", "--dir", "out/", "x.bam"])
            .validate()
            .unwrap();
        assert_eq!(c.output_dir, "out/");
    }

    #[test]
    fn parent_dir_defaults_to_cwd() {
        let c = cli(&["-g", "g", "x.bam"]).validate().unwrap();
        assert_eq!(c.parent_dir, std::env::current_dir().unwrap());
    }

    #[test]
    fn samtools_path_is_dropped_not_stored() {
        // Accepted but ignored: a garbage path validates fine (no existence check).
        let c = cli(&["-g", "g", "--samtools_path", "/no/such/samtools", "x.bam"])
            .validate()
            .unwrap();
        // ResolvedConfig has no samtools field — the value is simply gone.
        assert_eq!(c.inputs.len(), 1);
    }
}

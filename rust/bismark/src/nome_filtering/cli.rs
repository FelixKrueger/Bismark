//! Command-line interface for `NOMe_filtering`.
//!
//! [`Cli`] is the clap-derived parser; [`Cli::validate`] resolves it into a
//! [`ResolvedConfig`], enforcing the SPEC §4 rules:
//!
//! - **Live flags:** positional `<infile>`, `-g`/`--genome_folder` (mandatory),
//!   `--dir`, `--version`. `--parent_dir` is accepted but inert in the Rust
//!   port (Perl uses it only to `chdir` back after reading the genome).
//! - **Inert flags** accepted for Perl compatibility with no output effect:
//!   `--zero_based`, `--CX`/`--CX_context`, `--GC`/`--GC_context`, `--gzip`
//!   (output is always gzipped), `--nome-seq` (`$nome` defaults on in Perl, so
//!   NOMe filtering is unconditional), `--merge_CpGs` (alone).
//! - **The one reachable die:** `--merge_CpGs` + `--CX`.
//! - **`--dir` path contract:** Perl `chdir`s into `--dir` and opens BOTH the
//!   input and the output by bare filename relative to it. We reproduce this by
//!   resolving `input = dir.join(infile)` and `output = dir.join(derived)`
//!   without changing the process CWD.

use std::path::PathBuf;

use clap::Parser;

use crate::nome_filtering::error::BismarkNomeError;
use crate::nome_filtering::filename::derive_manowar_name;

/// `--help` footer: the per-tool last-modified date (embedded by build.rs).
const HELP_FOOTER: &str = concat!("Last modified: ", env!("BISMARK_LAST_MODIFIED"));

/// Parsed command-line arguments. Use [`Cli::validate`] to resolve.
#[derive(Parser, Debug)]
#[command(
    name = "NOMe_filtering",
    about = "Per-read NOMe-Seq methylation filtering (standalone Bismark NOMe_filtering)",
    long_about = None,
    disable_version_flag = true,
    after_help = HELP_FOOTER
)]
pub struct Cli {
    /// Yacht input file (from `bismark_methylation_extractor --yacht`).
    pub infile: Option<PathBuf>,

    /// Genome FASTA folder (mandatory; full path). Accepts `.fa` / `.fasta`.
    #[arg(short = 'g', long = "genome_folder")]
    pub genome_folder: Option<PathBuf>,

    /// Output directory; the input AND the output are resolved relative to it.
    #[arg(long = "dir")]
    pub dir: Option<PathBuf>,

    /// Accepted for Perl compatibility; **inert** in the Rust port.
    #[arg(long = "parent_dir")]
    pub parent_dir: Option<PathBuf>,

    /// Inert (output is always gzipped).
    #[arg(long = "gzip")]
    pub gzip: bool,

    /// Inert (coordinates are always 1-based here).
    #[arg(long = "zero_based")]
    pub zero_based: bool,

    /// Inert alone; combined with `--merge_CpGs` it triggers the one
    /// reachable die.
    #[arg(long = "CX", visible_alias = "CX_context")]
    pub cx: bool,

    /// Inert (NOMe GC reporting is unconditional).
    #[arg(long = "GC", visible_alias = "GC_context")]
    pub gc: bool,

    /// Inert (NOMe filtering is unconditional; Perl `$nome` defaults on and is
    /// non-negatable).
    #[arg(long = "nome-seq")]
    pub nome_seq: bool,

    /// Inert alone; dies only when combined with `--CX`.
    #[arg(long = "merge_CpGs")]
    pub merge_cpgs: bool,

    /// Print version information and exit.
    #[arg(short = 'V', long = "version")]
    pub version: bool,
}

/// The resolved, validated configuration passed to [`crate::nome_filtering::run`].
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// Genome FASTA folder.
    pub genome_folder: PathBuf,
    /// Yacht input, resolved under `--dir` (SPEC §4).
    pub input_path: PathBuf,
    /// `.manOwar.txt.gz` output, resolved under `--dir` (SPEC §4).
    pub output_path: PathBuf,
    /// Output directory (created if missing; defaults to `.`).
    pub output_dir: PathBuf,
}

impl Cli {
    /// Validate flag combinations and resolve the `--dir`-relative paths.
    ///
    /// # Errors
    /// - [`BismarkNomeError::MergeCpgsWithCx`] for `--merge_CpGs` + `--CX`.
    /// - [`BismarkNomeError::MissingGenomeFolder`] if `-g` is absent.
    /// - [`BismarkNomeError::InfileNotFound`] if no positional infile is given.
    pub fn validate(self) -> Result<ResolvedConfig, BismarkNomeError> {
        // The one reachable Perl die (NOMe_filtering:498-500).
        if self.merge_cpgs && self.cx {
            return Err(BismarkNomeError::MergeCpgsWithCx);
        }

        let genome_folder = self
            .genome_folder
            .ok_or(BismarkNomeError::MissingGenomeFolder)?;
        let infile = self.infile.ok_or(BismarkNomeError::InfileNotFound)?;
        let output_dir = self.dir.unwrap_or_else(|| PathBuf::from("."));

        // SPEC §4: input AND output are resolved relative to `--dir` (Perl
        // chdir's into it, then opens both by bare filename). We join
        // explicitly rather than changing the process CWD.
        //
        // Edge (code-review B-M1): if `infile` is an ABSOLUTE path, `Path::join`
        // discards `output_dir`, so both paths become that absolute location.
        // This matches Perl — `chdir(--dir)` followed by opening an absolute
        // path ignores the cwd. Real callers (the extractor) always pass a bare
        // filename; the absolute case is pinned by `absolute_infile_ignores_dir`.
        let infile_str = infile.to_string_lossy().into_owned();
        let input_path = output_dir.join(&infile);
        let output_path = output_dir.join(derive_manowar_name(&infile_str));

        // Inert flags — accepted for Perl compatibility, no output effect.
        let _ = (
            self.gzip,
            self.zero_based,
            self.cx,
            self.gc,
            self.nome_seq,
            self.merge_cpgs,
            self.parent_dir,
        );

        Ok(ResolvedConfig {
            genome_folder,
            input_path,
            output_path,
            output_dir,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut full = vec!["NOMe_filtering"];
        full.extend(args.iter().copied());
        Cli::try_parse_from(full)
    }

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn merge_cpgs_with_cx_is_rejected() {
        let cli = parse(&["-g", "/g", "--merge_CpGs", "--CX", "in.txt"]).unwrap();
        assert!(matches!(
            cli.validate().unwrap_err(),
            BismarkNomeError::MergeCpgsWithCx
        ));
    }

    #[test]
    fn merge_cpgs_alone_is_accepted() {
        let cli = parse(&["-g", "/g", "--merge_CpGs", "in.txt"]).unwrap();
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn missing_genome_folder_is_rejected() {
        let cli = parse(&["in.txt"]).unwrap();
        assert!(matches!(
            cli.validate().unwrap_err(),
            BismarkNomeError::MissingGenomeFolder
        ));
    }

    #[test]
    fn inert_flags_accepted_no_effect() {
        let cli = parse(&[
            "-g",
            "/g",
            "--zero_based",
            "--CX",
            "--GC",
            "--gzip",
            "--nome-seq",
            "--parent_dir",
            "/p",
            "in.txt",
        ])
        .unwrap();
        let cfg = cli.validate().unwrap();
        assert_eq!(cfg.genome_folder, PathBuf::from("/g"));
    }

    #[test]
    fn dir_path_contract_resolves_input_and_output_under_dir() {
        let cli = parse(&["-g", "/g", "--dir", "/out", "sample.txt"]).unwrap();
        let cfg = cli.validate().unwrap();
        assert_eq!(cfg.input_path, PathBuf::from("/out/sample.txt"));
        assert_eq!(cfg.output_path, PathBuf::from("/out/sample.manOwar.txt.gz"));
    }

    #[test]
    fn no_dir_resolves_against_cwd_dot() {
        let cli = parse(&["-g", "/g", "sample.txt.gz"]).unwrap();
        let cfg = cli.validate().unwrap();
        assert_eq!(cfg.output_dir, PathBuf::from("."));
        assert_eq!(
            cfg.output_path.file_name().unwrap(),
            "sample.manOwar.txt.gz"
        );
    }

    #[test]
    fn absolute_infile_ignores_dir() {
        // code-review B-M1: an absolute infile makes Path::join discard --dir,
        // so both resolved paths are absolute — matching Perl's
        // chdir-then-absolute-open behaviour.
        let cli = parse(&["-g", "/g", "--dir", "/out", "/abs/sample.txt"]).unwrap();
        let cfg = cli.validate().unwrap();
        assert_eq!(cfg.input_path, PathBuf::from("/abs/sample.txt"));
        assert_eq!(cfg.output_path, PathBuf::from("/abs/sample.manOwar.txt.gz"));
    }

    #[test]
    fn version_parses_without_infile() {
        let cli = parse(&["--version"]).unwrap();
        assert!(cli.version);
        assert!(cli.infile.is_none());
    }

    #[test]
    fn cx_context_alias_parses() {
        let cli = parse(&["-g", "/g", "--CX_context", "--merge_CpGs", "in.txt"]).unwrap();
        assert!(cli.cx);
        assert!(matches!(
            cli.validate().unwrap_err(),
            BismarkNomeError::MergeCpgsWithCx
        ));
    }
}

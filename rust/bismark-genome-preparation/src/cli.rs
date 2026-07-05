//! Command-line surface (clap derive) + validation into [`ResolvedConfig`].
//!
//! Flag spellings match the Perl `bismark_genome_preparation` exactly
//! (underscores preserved: `--single_fasta`, `--path_to_aligner`,
//! `--large-index`, `--genomic_composition`). `--version` is handled manually
//! (clap's auto-version is disabled) so the binary can print the Bismark
//! provenance banner.

use std::path::PathBuf;

use clap::Parser;

use crate::error::GenomePrepError;

/// Which external indexer to build for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aligner {
    /// `bowtie2-build` (default).
    Bowtie2,
    /// `hisat2-build`.
    Hisat2,
    /// `minimap2 -d` (`-k 20`).
    Minimap2,
}

impl Aligner {
    /// The indexer binary name to discover/run.
    pub fn binary_name(self) -> &'static str {
        match self {
            Aligner::Bowtie2 => "bowtie2-build",
            Aligner::Hisat2 => "hisat2-build",
            Aligner::Minimap2 => "minimap2",
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "bismark_genome_preparation_rs",
    about = "Prepare bisulfite-converted genome references (CT + GA) and index them.",
    disable_version_flag = true
)]
pub struct Cli {
    /// Path to the folder containing the genome FASTA file(s). Mandatory
    /// (optional at the clap layer so `--version`/`--man` work without it).
    pub genome_folder: Option<PathBuf>,

    /// Build Bowtie 2 indices (default).
    #[arg(long)]
    pub bowtie2: bool,

    /// Build HISAT2 indices.
    #[arg(long)]
    pub hisat2: bool,

    /// Build minimap2 indices.
    #[arg(long = "minimap2", alias = "mm2")]
    pub minimap2: bool,

    /// Folder containing the indexer binary (not the executable itself).
    #[arg(long = "path_to_aligner")]
    pub path_to_aligner: Option<PathBuf>,

    /// Threads per indexing process (≥ 2). Two indexers run concurrently, so
    /// `--parallel 4` uses 8 cores total.
    #[arg(long)]
    pub parallel: Option<u32>,

    /// Write each chromosome to an individual FASTA file instead of one MFA.
    #[arg(long = "single_fasta")]
    pub single_fasta: bool,

    /// DEPRECATED (slated for removal): SLAM-seq mode — T→C / A→G instead of
    /// C→T / G→A.
    #[arg(long)]
    pub slam: bool,

    /// Force a large index (Bowtie 2 / HISAT2).
    #[arg(long = "large-index")]
    pub large_index: bool,

    /// Calculate the genomic mono-/di-nucleotide composition and write
    /// `<genome>/genomic_nucleotide_frequencies.txt` (before conversion).
    #[arg(long = "genomic_composition")]
    pub genomic_composition: bool,

    /// Bismark-Rust extension: ALSO build a single combined CT+GA reference +
    /// index (additive; default OFF).
    #[arg(long = "combined_genome")]
    pub combined_genome: bool,

    /// Verbose diagnostics.
    #[arg(long)]
    pub verbose: bool,

    /// Print the full help (alias of `--help`).
    #[arg(long)]
    pub man: bool,

    /// Print version and exit.
    #[arg(long, short = 'V')]
    pub version: bool,
}

/// Validated, normalised configuration ready for the pipeline.
#[derive(Debug)]
pub struct ResolvedConfig {
    /// Absolutised genome folder.
    pub genome_folder: PathBuf,
    /// Selected indexer.
    pub aligner: Aligner,
    /// Optional explicit indexer directory (validated early in Step I).
    pub path_to_aligner: Option<PathBuf>,
    /// Threads passed to the indexer (always emitted; `parallel.unwrap_or(1)`).
    pub threads: u32,
    /// Per-chromosome output instead of MFA.
    pub single_fasta: bool,
    /// SLAM mode (deprecated).
    pub slam: bool,
    /// Force large index.
    pub large_index: bool,
    /// `--genomic_composition` requested — write the nucleotide-frequency table.
    pub genomic_composition: bool,
    /// `--combined_genome` requested.
    pub combined_genome: bool,
    /// Verbose diagnostics.
    pub verbose: bool,
}

impl Cli {
    /// Validate flag combinations (mirrors Perl's `die`s) and normalise into a
    /// [`ResolvedConfig`]. Does NOT touch the filesystem beyond absolutising
    /// the genome folder.
    pub fn validate(&self) -> Result<ResolvedConfig, GenomePrepError> {
        let genome_folder = self.genome_folder.clone().ok_or_else(|| {
            GenomePrepError::Validation(
                "please specify a genome folder to be used for bisulfite conversion".to_string(),
            )
        })?;

        // Aligner selection: at most one of bowtie2/hisat2/minimap2.
        let n = self.bowtie2 as u8 + self.hisat2 as u8 + self.minimap2 as u8;
        if n > 1 {
            return Err(GenomePrepError::Validation(
                "you may not select more than one aligner — pick one of --bowtie2 / --hisat2 / \
                 --minimap2 (default is Bowtie 2)"
                    .to_string(),
            ));
        }
        let aligner = if self.hisat2 {
            Aligner::Hisat2
        } else if self.minimap2 {
            Aligner::Minimap2
        } else {
            Aligner::Bowtie2
        };

        // minimap2 exclusions (Perl lines 154–177).
        if aligner == Aligner::Minimap2 {
            if self.single_fasta {
                return Err(GenomePrepError::Validation(
                    "minimap2 mode does not work in conjunction with --single_fasta — please respecify"
                        .to_string(),
                ));
            }
            if self.slam {
                return Err(GenomePrepError::Validation(
                    "minimap2 mode does not work in conjunction with --slam — please respecify"
                        .to_string(),
                ));
            }
            if self.large_index {
                return Err(GenomePrepError::Validation(
                    "minimap2 mode does not work in conjunction with --large-index — please respecify"
                        .to_string(),
                ));
            }
        }

        // --parallel must be ≥ 2 if given (Perl line 110).
        if let Some(p) = self.parallel
            && p < 2
        {
            return Err(GenomePrepError::Validation(
                "--parallel should have a value of 2 or more — please respecify".to_string(),
            ));
        }
        let threads = self.parallel.unwrap_or(1);

        // Absolutise the genome folder (Perl chdir + getcwd). Errors if absent.
        let genome_folder = std::fs::canonicalize(&genome_folder).map_err(|e| {
            GenomePrepError::Validation(format!(
                "could not access genome folder {}: {e}",
                genome_folder.display()
            ))
        })?;

        Ok(ResolvedConfig {
            genome_folder,
            aligner,
            path_to_aligner: self.path_to_aligner.clone(),
            threads,
            single_fasta: self.single_fasta,
            slam: self.slam,
            large_index: self.large_index,
            genomic_composition: self.genomic_composition,
            combined_genome: self.combined_genome,
            verbose: self.verbose,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn cli(args: &[&str]) -> Cli {
        Cli::parse_from(std::iter::once("prog").chain(args.iter().copied()))
    }

    #[test]
    fn default_aligner_is_bowtie2() {
        let d = tempdir().unwrap();
        let c = cli(&[d.path().to_str().unwrap()]);
        assert_eq!(c.validate().unwrap().aligner, Aligner::Bowtie2);
    }

    #[test]
    fn mm2_alias_selects_minimap2() {
        let d = tempdir().unwrap();
        let c = cli(&["--mm2", d.path().to_str().unwrap()]);
        assert_eq!(c.validate().unwrap().aligner, Aligner::Minimap2);
    }

    #[test]
    fn conflicting_aligners_error() {
        let d = tempdir().unwrap();
        let c = cli(&["--bowtie2", "--hisat2", d.path().to_str().unwrap()]);
        assert!(matches!(c.validate(), Err(GenomePrepError::Validation(_))));
    }

    #[test]
    fn minimap2_excludes_single_fasta_slam_large_index() {
        let d = tempdir().unwrap();
        for extra in ["--single_fasta", "--slam", "--large-index"] {
            let c = cli(&["--minimap2", extra, d.path().to_str().unwrap()]);
            assert!(
                matches!(c.validate(), Err(GenomePrepError::Validation(_))),
                "expected error for --minimap2 {extra}"
            );
        }
    }

    #[test]
    fn parallel_less_than_two_errors() {
        let d = tempdir().unwrap();
        let c = cli(&["--parallel", "1", d.path().to_str().unwrap()]);
        assert!(matches!(c.validate(), Err(GenomePrepError::Validation(_))));
    }

    #[test]
    fn parallel_default_threads_one_explicit_sets_n() {
        let d = tempdir().unwrap();
        assert_eq!(
            cli(&[d.path().to_str().unwrap()])
                .validate()
                .unwrap()
                .threads,
            1
        );
        assert_eq!(
            cli(&["--parallel", "4", d.path().to_str().unwrap()])
                .validate()
                .unwrap()
                .threads,
            4
        );
    }

    #[test]
    fn missing_genome_folder_errors() {
        // `--version`/`--man` are handled in main before validate(); validate
        // itself requires the positional.
        assert!(matches!(
            cli(&["--bowtie2"]).validate(),
            Err(GenomePrepError::Validation(_))
        ));
    }

    #[test]
    fn underscore_long_flags_parse() {
        let d = tempdir().unwrap();
        let c = cli(&[
            "--single_fasta",
            "--combined_genome",
            "--genomic_composition",
            d.path().to_str().unwrap(),
        ]);
        let r = c.validate().unwrap();
        assert!(r.single_fasta && r.combined_genome && r.genomic_composition);
    }
}

//! Command-line surface (clap derive) + validation into [`ResolvedConfig`].
//!
//! Flag spellings match the Perl `bismark_genome_preparation` exactly
//! (underscores preserved: `--single_fasta`, `--path_to_aligner`,
//! `--large-index`, `--genomic_composition`). `--version` is handled manually
//! (clap's auto-version is disabled) so the binary can print the Bismark
//! provenance banner.

use std::path::PathBuf;

use clap::Parser;

use crate::genome_prep::error::GenomePrepError;

/// Which external indexer to build for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aligner {
    /// `bowtie2-build` (default).
    Bowtie2,
    /// `hisat2-build`.
    Hisat2,
    /// `minimap2 -d` (`-k 20`).
    Minimap2,
    /// `bwa-mem3 index --meth` on the unconverted genome.
    BwaMem3,
}

impl Aligner {
    /// The indexer binary name to discover/run.
    pub fn binary_name(self) -> &'static str {
        match self {
            Aligner::Bowtie2 => "bowtie2-build",
            Aligner::Hisat2 => "hisat2-build",
            Aligner::Minimap2 => "minimap2",
            Aligner::BwaMem3 => "bwa-mem3",
        }
    }
}

/// `--help` footer: the per-tool last-modified date (embedded by build.rs).
const HELP_FOOTER: &str = concat!("Last modified: ", env!("BISMARK_LAST_MODIFIED"));

#[derive(Parser, Debug)]
#[command(
    name = "bismark_genome_preparation",
    about = "Prepare bisulfite-converted genome references (CT + GA) and index them, or build a \
             bwa-mem3 methylation index (--bwamem3).",
    disable_version_flag = true,
    after_help = HELP_FOOTER
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

    /// Build a bwa-mem3 methylation index (`bwa-mem3 index --meth`) of the unconverted genome in
    /// `<genome>/Bwamem3_Index/`, for `bismark --illumina_5base --bwamem3` and `bismark --taps`.
    /// Writes no bisulfite-converted genome.
    #[arg(long)]
    pub bwamem3: bool,

    /// With --bwamem3: bwa-mem3's memory budget (`--max-memory`, e.g. `120G`). Default: detected
    /// RAM less a reserve.
    #[arg(long = "bwamem3_max_memory", value_name = "SIZE")]
    pub bwamem3_max_memory: Option<String>,

    /// Folder containing the indexer binary (not the executable itself).
    #[arg(long = "path_to_aligner")]
    pub path_to_aligner: Option<PathBuf>,

    /// Threads per indexing process (≥ 2). Bowtie 2, HISAT2 and minimap2 run two indexers
    /// concurrently, so `--parallel 4` uses 8 cores total. With --bwamem3 it is bwa-mem3's `-t`
    /// (default: all cores).
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
    #[arg(long, short = 'V', short_alias = 'v')]
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
    /// `--parallel` as given; bwa-mem3 gets `-t` only when it is set.
    pub parallel: Option<u32>,
    /// `--bwamem3_max_memory`, passed to bwa-mem3 as `--max-memory`.
    pub bwamem3_max_memory: Option<String>,
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

        // Aligner selection: at most one of bowtie2/hisat2/minimap2/bwamem3.
        let n = self.bowtie2 as u8 + self.hisat2 as u8 + self.minimap2 as u8 + self.bwamem3 as u8;
        if n > 1 {
            return Err(GenomePrepError::Validation(
                "you may not select more than one aligner — pick one of --bowtie2 / --hisat2 / \
                 --minimap2 / --bwamem3 (default is Bowtie 2)"
                    .to_string(),
            ));
        }
        let aligner = if self.hisat2 {
            Aligner::Hisat2
        } else if self.minimap2 {
            Aligner::Minimap2
        } else if self.bwamem3 {
            Aligner::BwaMem3
        } else {
            Aligner::Bowtie2
        };

        if aligner == Aligner::BwaMem3 {
            for (set, flag) in [
                (self.single_fasta, "--single_fasta"),
                (self.slam, "--slam"),
                (self.large_index, "--large-index"),
                (self.combined_genome, "--combined_genome"),
            ] {
                if set {
                    return Err(GenomePrepError::Validation(format!(
                        "bwa-mem3 mode does not work in conjunction with {flag} — please respecify"
                    )));
                }
            }
        }
        match self.bwamem3_max_memory.as_deref() {
            Some(_) if aligner != Aligner::BwaMem3 => {
                return Err(GenomePrepError::Validation(
                    "--bwamem3_max_memory requires --bwamem3".to_string(),
                ));
            }
            Some(m) if m.trim().is_empty() => {
                return Err(GenomePrepError::Validation(
                    "--bwamem3_max_memory needs a size, e.g. 120G".to_string(),
                ));
            }
            _ => {}
        }

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
            parallel: self.parallel,
            bwamem3_max_memory: self.bwamem3_max_memory.clone(),
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
    fn bwamem3_selects_bwamem3_and_keeps_parallel() {
        let d = tempdir().unwrap();
        let g = d.path().to_str().unwrap();
        let r = cli(&["--bwamem3", g]).validate().unwrap();
        assert_eq!(r.aligner, Aligner::BwaMem3);
        assert_eq!(r.parallel, None);
        let r = cli(&["--bwamem3", "--parallel", "4", "--genomic_composition", g])
            .validate()
            .unwrap();
        assert_eq!((r.parallel, r.genomic_composition), (Some(4), true));
        let r = cli(&["--bwamem3", "--bwamem3_max_memory", "8G", g])
            .validate()
            .unwrap();
        assert_eq!(r.bwamem3_max_memory.as_deref(), Some("8G"));
    }

    #[test]
    fn bwamem3_conflicts() {
        let d = tempdir().unwrap();
        let g = d.path().to_str().unwrap();
        for other in ["--bowtie2", "--hisat2", "--minimap2"] {
            let e = cli(&["--bwamem3", other, g])
                .validate()
                .unwrap_err()
                .to_string();
            assert!(
                e.contains("more than one aligner") && e.contains("--bwamem3"),
                "{e}"
            );
        }
        for extra in [
            "--single_fasta",
            "--slam",
            "--large-index",
            "--combined_genome",
        ] {
            let e = cli(&["--bwamem3", extra, g])
                .validate()
                .unwrap_err()
                .to_string();
            assert!(e.contains("bwa-mem3 mode") && e.contains(extra), "{e}");
        }
        let e = cli(&["--bwamem3_max_memory", "8G", g])
            .validate()
            .unwrap_err()
            .to_string();
        assert!(e.contains("requires --bwamem3"), "{e}");
        let e = cli(&["--bwamem3", "--bwamem3_max_memory", " ", g])
            .validate()
            .unwrap_err()
            .to_string();
        assert!(e.contains("needs a size"), "{e}");
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

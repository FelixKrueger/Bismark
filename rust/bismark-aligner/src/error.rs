//! Typed errors for `bismark-aligner` (Phase 1: CLI / discovery / aligner
//! detection).
//!
//! `main` maps any of these to exit code `1`; clap parse errors map to `2`
//! (clap convention). None of the error *text* is part of the byte-identity
//! gate (diagnostics go to STDERR), but messages mirror Perl `die`s where it
//! helps users migrating from the Perl tool.

use std::path::PathBuf;

/// All errors raised by the Phase-1 pipeline.
#[derive(Debug, thiserror::Error)]
pub enum AlignerError {
    /// Direct `std::io::Error` (stat-ing the genome dir, running the aligner).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// CLI validation failure (mirrors Perl's `die` for bad flag combinations).
    #[error("{0}")]
    Validation(String),

    /// A mode/option that parses but is **deferred** to a later phase or a
    /// follow-up epic (e.g. HISAT2/minimap2, SAM/CRAM output). Fails loudly
    /// rather than silently half-supporting it.
    #[error("{0}")]
    Unsupported(String),

    /// The genome folder is missing or is not a directory (Perl `die` after a
    /// failed `chdir`, 7631–32).
    #[error("failed to access genome folder {0:?} (does it exist and is it a directory?)")]
    GenomeFolder(PathBuf),

    /// The genome folder contains no FASTA file with any recognised extension.
    #[error(
        "the specified genome folder {0:?} does not contain any sequence files in FastA format \
         (with .fa, .fa.gz, .fasta or .fasta.gz file extensions)"
    )]
    NoFasta(PathBuf),

    /// A required bisulfite index file is missing (mirrors Perl 7654–58 /
    /// 7743/7791). The `aligner` name distinguishes a faulty Bowtie 2 `.bt2`
    /// index from a faulty HISAT2 `.ht2` one.
    #[error(
        "the {aligner} index of the {converted}->converted genome seems to be faulty or \
         non-existant ('{missing}'). Please run the bismark_genome_preparation before running Bismark"
    )]
    FaultyIndex {
        /// Human-readable aligner name (`Bowtie 2` / `HISAT2`).
        aligner: String,
        /// `C->T` or `G->A`.
        converted: String,
        /// The first missing index file name.
        missing: String,
    },

    /// The aligner binary could not be executed (mirrors Perl 7071–72).
    #[error(
        "failed to execute {aligner} properly (could not run '{cmd} --version'). Please install \
         {aligner} and make sure it is in the PATH, or specify the path with {path_flag} /path/to/dir"
    )]
    AlignerNotWorking {
        /// Human-readable aligner name (`Bowtie 2` / `HISAT2`).
        aligner: String,
        /// The command that failed (the resolved binary path).
        cmd: String,
        /// The `--path_to_<aligner>` flag to hint in the diagnostic.
        path_flag: String,
    },

    /// A supplied input read file does not exist (mirrors Perl 8102/8117).
    #[error("supplied filename '{0}' does not exist, please respecify!")]
    InputFileMissing(String),
}

/// Convenience alias for Phase-1 fallible operations.
pub type Result<T> = std::result::Result<T, AlignerError>;

//! Typed errors for `bismark-genome-preparation`.
//!
//! The binary's `main` maps any of these to exit code `1`; clap parse errors
//! map to `2` (clap convention). None of the error *text* is part of the
//! byte-identity gate (diagnostics are STDERR).

use std::path::PathBuf;

/// All errors raised by the genome-preparation pipeline.
#[derive(Debug, thiserror::Error)]
pub enum GenomePrepError {
    /// Direct `std::io::Error` (reading FASTA, creating dirs, writing output).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// CLI validation failure (mirrors Perl's `die` for bad flag combinations).
    /// Text is surfaced to STDERR; not byte-gated.
    #[error("{0}")]
    Validation(String),

    /// The genome folder contains no FASTA files with any recognised extension
    /// (`.fa` / `.fa.gz` / `.fasta` / `.fasta.gz`). Mirrors Perl's `die` at
    /// lines 624–626.
    #[error(
        "the specified genome folder {0} does not contain any sequence files in FastA format \
         (with .fa, .fa.gz, .fasta or .fasta.gz file extensions)"
    )]
    NoFasta(PathBuf),

    /// A FASTA file's first line does not start with `>` — not in FASTA format.
    /// Mirrors Perl's `die` in `extract_chromosome_name` (lines 579–581). Note a
    /// *bare* `>` is NOT this error (it yields an empty chromosome name).
    #[error(
        "the file {0} doesn't seem to be in FASTA format as required (first line is not a `>` header)"
    )]
    NotFasta(PathBuf),

    /// A chromosome name was seen in more than one place (across all input
    /// files). Mirrors Perl's uniqueness `die` (lines 409–411).
    #[error(
        "exiting because chromosome name '{0}' already exists — please make sure all chromosomes \
         have a unique name"
    )]
    DuplicateChromosome(String),

    /// The external indexer binary could not be located. `searched` lists the
    /// paths that were probed (BISMARK_BIN / PATH / current_exe, or the
    /// explicit `--path_to_aligner` directory).
    #[error("could not locate the indexer '{tool}' (searched: {searched:?})")]
    IndexerNotFound {
        /// Indexer binary name (`bowtie2-build` / `hisat2-build` / `minimap2`).
        tool: String,
        /// Paths probed during discovery.
        searched: Vec<PathBuf>,
    },

    /// The external indexer ran but exited non-zero (or could not be spawned).
    #[error("the indexer '{tool}' failed to build the index for {dir}")]
    IndexerFailed {
        /// Indexer binary name.
        tool: String,
        /// Conversion directory whose index build failed.
        dir: PathBuf,
    },
}

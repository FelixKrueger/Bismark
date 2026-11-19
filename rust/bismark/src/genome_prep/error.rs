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

    /// An I/O error on a named path.
    #[error("{}: {source}", path.display())]
    IoAt {
        /// The file or folder the operation was on.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },

    /// `bwa-mem3 index` exited 3: its memory budget is below what the genome needs.
    #[error(
        "bwa-mem3 needs more memory than its budget to index {genome} (see its message above). \
         {hint}"
    )]
    Bwamem3Memory {
        /// Genome folder being indexed.
        genome: PathBuf,
        /// Where the budget came from and how to raise it.
        hint: String,
    },

    /// A complete, checked index could not be moved into place.
    #[error(
        "the new bwa-mem3 index is complete in {} but could not be moved into place ({detail}). \
         Replace {} with it by hand: remove {} if it exists, then rename the new folder to it.",
        partial.display(), dest.display(), dest.display()
    )]
    Bwamem3NotPublished {
        /// The finished index folder.
        partial: PathBuf,
        /// Where it belongs (`<genome>/Bwamem3_Index`).
        dest: PathBuf,
        /// Why the move failed.
        detail: String,
    },

    /// `bwa-mem3 index` failed, or its output is incomplete or does not match the genome.
    #[error("bwa-mem3 could not build the methylation index for {genome}: {detail}")]
    Bwamem3Failed {
        /// Genome folder being indexed.
        genome: PathBuf,
        /// What went wrong.
        detail: String,
    },
}

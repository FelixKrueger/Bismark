//! Typed errors for `bismark-bedgraph`.
//!
//! Variants are produced at CLI-validation and pipeline boundaries.
//! Messages mirror Perl `bismark2bedGraph` v0.25.1 where a user-facing
//! string exists (cited by Perl line number). The binary's `main` maps
//! any [`BismarkBedgraphError`] to exit code 1 (clap parse errors are
//! exit 2 by clap convention).

use std::path::PathBuf;

/// All errors raised by the `bismark-bedgraph` orchestration layer.
#[derive(Debug, thiserror::Error)]
pub enum BismarkBedgraphError {
    /// No positional input files supplied. Perl prints the help text and
    /// exits; we surface a clear error (help text is not byte-gated).
    /// Mirrors Perl `bismark2bedGraph:684`.
    #[error(
        "You need to provide one or more Bismark methylation caller files to create an \
         individual C methylation bedGraph output. Please respecify!"
    )]
    NoInputFiles,

    /// `-o`/`--output` was not supplied. Perl `bismark2bedGraph:726`.
    #[error("Please provide the name of the output file using the option -o/--output filename")]
    BedGraphOutputRequired,

    /// The output filename contained a path separator. Perl `bismark2bedGraph:730`.
    #[error("Please specify a file name without any path information (or use --dir if necessary)")]
    OutputHasPath,

    /// `--cutoff` was non-positive. Perl `bismark2bedGraph:750`.
    #[error("Please select a coverage greater than 0 (positive integers only)")]
    BadCutoff {
        /// The invalid value the user supplied.
        value: i64,
    },

    /// `--buffer_size` did not match `\d+%` or `\d+[KMGT]`. Perl
    /// `bismark2bedGraph:767`. (Accepted-but-ignored at runtime per SPEC
    /// D3, but format-validated for CLI parity.)
    #[error(
        "Please select a buffer size as percentage (e.g. --buffer_size 20%) or a number to be \
         multiplied with K, M, G, T etc. (e.g. --buffer_size 20G). For more information on sort \
         type 'info sort' on a command line"
    )]
    BadBufferSize {
        /// The malformed value the user supplied.
        value: String,
    },

    /// `--ample_memory` given together with an explicit `--buffer_size`.
    /// Perl `bismark2bedGraph:763`.
    #[error(
        "The options '--ample_mem' and using the UNIX sort function are mutually exclusive. \
         Please make your pick!"
    )]
    AmpleMemoryWithBufferSize,

    /// `--ample_memory` given together with `--gazillion`/`--scaffolds`.
    /// Perl `bismark2bedGraph:784`.
    #[error(
        "You can't currently select '--ample_mem' together with '--gazillion'. Make your pick!"
    )]
    AmpleMemoryWithGazillion,

    /// Default (CpG-only) mode but none of the input files' basenames start
    /// with `CpG`. Perl `bismark2bedGraph:111`.
    #[error(
        "It seems that you are trying to generate bedGraph files for files not starting with \
         CpG.... Please specify the option '--CX' and try again"
    )]
    NoCpgFiles,

    /// A methylation-call line could not be parsed — either a required field
    /// (strand/call) was missing, or the position was not a positive integer
    /// in `u32` range. Perl `croak`s on the missing-field case
    /// (`validate_methylation_call`, `bismark2bedGraph:560`/`:562`) — a hard,
    /// fatal error. `reason` distinguishes the two so the message is accurate
    /// (dual code-review finding A3/B-L1).
    #[error("Malformed methylation-call line in {file} (line {line}): {reason}")]
    MalformedCallLine {
        /// Input file the bad line came from.
        file: PathBuf,
        /// 1-based line number within that file.
        line: u64,
        /// Specific reason (which field was missing / bad position).
        reason: &'static str,
    },

    /// Direct `std::io::Error` from the orchestration layer (reading inputs,
    /// writing/gzipping outputs, creating the output directory).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

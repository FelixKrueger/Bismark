//! Typed errors for `bismark-methylation-consistency`.
//!
//! Produced at the CLI-validation and pipeline-orchestration boundaries.
//! Library errors from [`bismark_io`] propagate via the
//! [`MethConsError::Io`] variant. `main` maps any of these to exit code 1;
//! clap parse errors exit 2 (clap convention). None of these messages are
//! part of the byte-identity gate (they go to STDERR) — they mirror Perl's
//! `die` text in spirit where practical.

use std::path::PathBuf;

use crate::io::BismarkIoError;

/// All errors raised by the `methylation_consistency` orchestration layer.
#[derive(Debug, thiserror::Error)]
pub enum MethConsError {
    /// Underlying I/O / decoding error from [`bismark_io`] (includes
    /// truncated-BGZF and malformed-record cases — both fatal).
    #[error(transparent)]
    Io(#[from] BismarkIoError),

    /// Direct `std::io::Error` from the orchestration layer (e.g. writing
    /// the report file, opening an output BAM).
    #[error("std I/O: {0}")]
    StdIo(#[from] std::io::Error),

    /// No positional input files. Mirrors Perl's usage `die` (line 131).
    #[error(
        "No input file(s) supplied. USAGE is:\n\n\tmethylation_consistency [--min-count=5] [bam file(s)]\n"
    )]
    NoInputFiles,

    /// `--upper_threshold` outside 51–100. Mirrors Perl line 76.
    #[error(
        "The upper methylation threshold needs to be a number between 51 and 100% [default is 90%]. Please select something more sensible and try again...\n"
    )]
    UpperThresholdOutOfRange,

    /// `--lower_threshold` outside 0–49. Mirrors Perl line 85.
    #[error(
        "The lower methylation threshold needs to be a number between 0 and 49% [default is 10%]. Please select something more sensible and try again...\n"
    )]
    LowerThresholdOutOfRange,

    /// PE: adjacent R1/R2 read names did not match. Mirrors Perl line 239.
    #[error(
        "READ IDs of R1 ({id1}) and R2 ({id2}) did not match. This doesn't look like paired-end data. Please correct settings and try again.\n"
    )]
    MateMismatch {
        /// R1 read name.
        id1: String,
        /// R2 read name.
        id2: String,
    },

    /// PE: the input BAM header declares coordinate sort (`@HD SO:coordinate`),
    /// which destroys R1/R2 adjacency.
    ///
    /// This is the **correct** guard that Perl's `test_positional_sorting`
    /// *intended* — its own `/^\@SO/` check (line 471) is dead code (no real
    /// header line starts with `@SO`). Implemented here as an intentional,
    /// output-equivalent fix (see SPEC §4.6/§4.11).
    #[error(
        "SAM/BAM header line indicates that the Bismark alignment file ({input}) has been sorted by chromosomal positions, which is incompatible with paired-end methylation consistency. Please use an unsorted file instead (e.g. samtools sort -n)\n"
    )]
    CoordinateSorted {
        /// Input file whose header declared coordinate sort.
        input: PathBuf,
    },
}

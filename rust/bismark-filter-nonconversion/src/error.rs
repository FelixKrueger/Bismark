//! Typed errors for `filter_non_conversion`.
//!
//! Display strings echo the Perl `filter_non_conversion` `die`/`warn`
//! messages where practical. These are surfaced on STDERR (not part of the
//! byte-identity gate, which covers the BAM bodies + report file), but
//! keeping them faithful eases cross-checking against the Perl. Perl's exact
//! wording — including the `repecify` typo on the percentage-range message —
//! is preserved deliberately.

use std::path::PathBuf;

use thiserror::Error;

/// Errors raised by the filter pipeline and CLI validation.
#[derive(Debug, Error)]
pub enum BismarkFilterError {
    // ── CLI validation (process_commandline, lines 519–603) ──────────────
    /// Both `-s` and `-p` supplied (Perl line 545).
    #[error(
        "Please select either -s for single-end files or -p for paired-end files, \
         but not both at the same time!"
    )]
    BothSingleAndPaired,

    /// `--percentage_cutoff` and `--consecutive` are mutually exclusive (line 521).
    #[error(
        "The options --percentage_cutoff and --consecutive are mutually exclusive. Please respecify!"
    )]
    PercentageAndConsecutive,

    /// `--percentage_cutoff` outside 0–100 (line 524; Perl's `repecify` typo kept).
    #[error(
        "The percentage cutoff value has to be within the range of 0-100 [%]. Please repecify!"
    )]
    PercentageOutOfRange,

    /// `--minimum_count` not > 0 (line 530).
    #[error("Please select a sensible number of non-CG cytosines as minimum count (1 or more...)")]
    InvalidMinimumCount,

    /// `--threshold` not > 0 (line 598; message interpolates the supplied value).
    #[error("Please use a sensible value for {value} (positive numbers only, default: [3])")]
    InvalidThreshold {
        /// The offending value, interpolated exactly as Perl does.
        value: i64,
    },

    // ── Per-file input handling (main loop, lines 37–72) ─────────────────
    /// Input filename does not end in `bam` (Perl line 38, `=~ /bam$/`).
    #[error("Please provide a BAM file to continue!")]
    NotABamFile,

    /// Input BAM has no alignment records (Perl `bam_isEmpty`, line 627).
    #[error(
        "\n### File appears to be empty, terminating non-CG filtering process. \
         Please make sure the input file has not been truncated. ###"
    )]
    EmptyInput,

    /// Input BAM appears truncated (Perl `bam_isTruncated`, line 647). Native
    /// noodles BGZF/EOF error rather than the Perl samtools-stderr scrape.
    #[error(
        "[ERROR] The file appears to be truncated, please ensure that there were \
         no errors while copying the file!!! Exiting... (underlying: {source})"
    )]
    Truncated {
        /// The underlying noodles/BGZF I/O error.
        source: std::io::Error,
    },

    /// Neither `-s`/`-p` given nor a Bismark `@PG` line found (Perl line 66).
    #[error(
        "Please specify either -s (single-end) or -p (paired-end) file, or provide \
         a SAM/BAM file that contains the @PG header line"
    )]
    CannotAutoDetectMode {
        /// The input whose mode could not be determined.
        input: PathBuf,
    },

    /// PE input declares coordinate sorting in `@HD SO:` (incompatible with
    /// the R1/R2-adjacency the filter relies on). Detected before any output
    /// is written (Perl `test_positional_sorting`, line 431, faithful no-output).
    #[error(
        "SAM/BAM header indicates the alignment file has been sorted by chromosomal \
         positions, which is incompatible with non-conversion filtering. Please use \
         an unsorted file instead (e.g. samtools sort -n)."
    )]
    CoordinateSorted,

    /// PE adjacent records' qnames disagree (likely position-sorted input;
    /// Perl `test_positional_sorting`, line 458).
    #[error(
        "The IDs of Read 1 ({read1}) and Read 2 ({read2}) are not the same. This might \
         be a result of sorting the paired-end SAM/BAM files by chromosomal position, \
         which is not compatible with non-conversion filtering. Please use an unsorted \
         file instead (e.g. samtools sort -n)."
    )]
    PairedIdMismatch {
        /// Read 1's qname.
        read1: String,
        /// Read 2's qname.
        read2: String,
    },

    /// PE: a mate is missing or has an empty XM call string, OR a lone
    /// trailing R1 left R2 absent at end-of-stream (Perl line 195,
    /// `unless($meth_call_1 and $meth_call_2)`). Faithful: prior complete
    /// pairs are already written; no report is produced.
    #[error("Failed to extract methylation calls from Read 1 or Read 2 for sequence pair")]
    PairedMissingMethCall,

    /// Any underlying I/O / BAM decode error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

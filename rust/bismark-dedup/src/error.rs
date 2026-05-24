//! Typed errors for `bismark-dedup`.
//!
//! All variants are produced at orchestration / pipeline boundaries.
//! Library errors from [`bismark_io`] propagate through the
//! [`BismarkDedupError::Io`] variant via `#[from]`. The binary's `main`
//! wraps these in `anyhow::Error` for top-level reporting; exit-code
//! mapping is documented in `PLAN.md` §5.

use std::path::PathBuf;

use bismark_io::BismarkIoError;

/// All errors raised by the `bismark-dedup` orchestration layer.
#[derive(Debug, thiserror::Error)]
pub enum BismarkDedupError {
    /// Underlying I/O / decoding error from [`bismark_io`].
    #[error(transparent)]
    Io(#[from] BismarkIoError),

    /// The input file contained zero alignment records after the unmapped
    /// filter. Mirrors Perl's `bam_isEmpty` check (lines 995–1017): error
    /// before any output file is created.
    #[error("input file is empty: {0}")]
    EmptyInput(PathBuf),

    /// Paired-end input ended with an unpaired R1 — odd record count.
    #[error("PE input ended with an unpaired R1 (qname={qname})")]
    UnpairedFinalRecord {
        /// qname (read identifier) of the orphan R1.
        qname: String,
    },

    /// CLI flag deferred to v1.1; v1.0 stub for explicit error rather than
    /// silent acceptance.
    #[error(
        "--{flag} is not supported in bismark-dedup v1.0; use the Perl `deduplicate_bismark` for this mode"
    )]
    UnsupportedFlagV1 {
        /// Name of the flag (e.g. `"barcode"`, `"bclconvert"`).
        flag: &'static str,
    },

    /// `--outfile` specified with multiple positional inputs but no
    /// `--multiple` flag.
    #[error("--outfile requires a single input file unless --multiple is set (got {n_files})")]
    OutfileWithMultipleInputs {
        /// Number of positional inputs supplied.
        n_files: usize,
    },

    /// Perl-verbatim joke retained for CLI parity. Bismark deprecated
    /// `--representative` long ago.
    #[error(
        "Deduplication in '--representative' mode is no longer supported. Please stop wanting that."
    )]
    RepresentativeRemoved,

    /// CRAM input or output requires `--cram_ref <FASTA>`.
    #[error("CRAM I/O requires --cram_ref <FASTA>")]
    MissingCramRef,

    /// `--multiple` inputs have different `@SQ` name sets — chromosome
    /// names from `offending_file` are not the same set as file1's.
    #[error(
        "--multiple inputs have non-identical @SQ name sets: {offending_file} \
         missing chr name(s) {missing_chrs:?}; extra chr name(s) {extra_chrs:?}"
    )]
    MultipleSqMismatch {
        /// Path of the input file whose @SQ name set diverges from file1's.
        offending_file: PathBuf,
        /// Chromosome names present in file1 but absent in `offending_file`.
        missing_chrs: Vec<String>,
        /// Chromosome names present in `offending_file` but absent in file1.
        extra_chrs: Vec<String>,
    },

    /// `--multiple` inputs span more than one file format. All inputs must
    /// share format (matches Perl's all-BAM-or-all-SAM assumption at
    /// lines 195–201).
    #[error("--multiple inputs must all share the same format (got mixed kinds)")]
    MultipleMixedFormat,

    /// A record's `reference_sequence_id` did not resolve to a chr name
    /// present in the intern map. Defensive — shouldn't happen with
    /// well-formed BAM input.
    #[error(
        "record has reference_sequence_id={refid} which is not present in the input's @SQ header"
    )]
    MissingChrInIntern {
        /// The unmapped refID encountered.
        refid: usize,
    },

    /// A record had no `reference_sequence_id` despite passing the
    /// unmapped-record filter. Defensive — shouldn't happen with
    /// well-formed BAM input.
    #[error("record has no reference_sequence_id despite being mapped (qname={qname:?})")]
    MissingReferenceId {
        /// qname (read identifier) of the offending record.
        qname: String,
    },

    /// A record's alignment_start was `None` despite passing the
    /// unmapped-record filter. Defensive — shouldn't happen with
    /// well-formed BAM input.
    #[error("record has no alignment_start despite being mapped (qname={qname:?})")]
    MissingAlignmentStart {
        /// qname (read identifier) of the offending record.
        qname: String,
    },

    /// User passed no positional input files.
    #[error("no input file(s) provided — pass one or more BAM/SAM/CRAM paths on the command line")]
    NoInputFiles,

    /// User specified neither `--single`/`--paired` AND the input BAM
    /// header has no `@PG ID:Bismark` line with both `-1`/`--1` and
    /// `-2`/`--2` args, so library mode cannot be inferred.
    #[error(
        "cannot auto-detect single-end vs paired-end mode from {input}'s @PG header; \
         please pass --single or --paired explicitly"
    )]
    CannotAutoDetectMode {
        /// Input file whose @PG header was inspected.
        input: PathBuf,
    },

    /// Direct `std::io::Error` from the orchestration layer (e.g. writing
    /// the report file).
    #[error("std I/O: {0}")]
    StdIo(#[from] std::io::Error),
}

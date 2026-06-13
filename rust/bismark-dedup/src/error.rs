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

    /// The `--multiple` input file **list** was empty (defensive; normally
    /// blocked upstream by [`BismarkDedupError::NoInputFiles`] in
    /// `Cli::validate`).
    ///
    /// NOTE (rev 1, plans/06132026_dedup-empty-input): a file with zero
    /// *alignment records* (header-only BAM, e.g. nothing aligned) is **NOT**
    /// an error. It is handled gracefully — a header-only deduplicated output +
    /// a zero-count report (`0 (0.00%)`) + exit 0 — so nf-core/methylseq does
    /// not crash on no-alignment samples. This is a deliberate divergence from
    /// Perl, which dies on empty input (`bam_isEmpty`, deduplicate_bismark:1014).
    #[error("input file is empty: {0}")]
    EmptyInput(PathBuf),

    /// Paired-end input ended with an unpaired R1 — odd record count.
    #[error("PE input ended with an unpaired R1 (qname={qname})")]
    UnpairedFinalRecord {
        /// qname (read identifier) of the orphan R1.
        qname: String,
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

    /// `--parallel N` was given with `N == 0`. Clap's `u32` parser accepts
    /// 0 (since 0 is a valid `u32`), so an explicit validate-stage check
    /// is needed.
    #[error("--parallel must be ≥ 1 (got {value})")]
    InvalidParallelValue {
        /// The invalid value the user supplied.
        value: u32,
    },

    /// Direct `std::io::Error` from the orchestration layer (e.g. writing
    /// the report file).
    #[error("std I/O: {0}")]
    StdIo(#[from] std::io::Error),

    /// v1.2.1-beta.1: the input file's qnames look like bcl-convert format
    /// (matches `:([CAGTN\+]+)_\d:N:\d:([CAGTN\+]+)$`) but the user passed
    /// `--barcode` / `--umi` instead of `--bclconvert`. Running `--barcode`
    /// against bcl-convert qnames silently extracts the i7 tail (NOT the
    /// UMI), producing nonsense dedup keys.
    ///
    /// Mirrors Perl `deduplicate_bismark`'s fatal error at lines 173-178
    /// inside the `test_readIDs_for_bclconvert` path (Perl function at
    /// line 915-995). Issue reference:
    /// <https://github.com/FelixKrueger/Bismark/issues/699>.
    #[error(
        "input file's qnames look like bcl-convert format (e.g. {qname:?}) — \
         the data carries internal UMIs added by bcl-convert (Illumina). \
         Running --barcode/--umi against this format silently extracts the \
         i7 tail instead of the UMI, producing nonsense dedup keys. \
         Solutions:\n  \
         a) re-run with --bclconvert to use the internal UMI, OR\n  \
         b) reform the readIDs to move UMIs to the end of the readID, OR\n  \
         c) re-run Bismark alignment with --icpc.\n\
         See https://github.com/FelixKrueger/Bismark/issues/699"
    )]
    BclconvertFormatWithBarcodeFlag {
        /// First record's qname that matched the bcl-convert format.
        qname: String,
    },

    /// UMI mode (`--barcode` / `--umi` / `--bclconvert`) was requested but a
    /// record's qname did not match the extractor's pattern.
    ///
    /// Mirrors Perl `deduplicate_bismark`'s "Failed to extract a barcode
    /// from the read ID" error at line 662-663. Errors at the first
    /// failed record; no partial output left on disk.
    #[error(
        "--{flag} mode requires UMI in qname; failed to extract from qname={qname:?}. \
         Check that the input was generated with the matching UMI flag — see `--help` \
         for which qname format each flag expects."
    )]
    UmiExtractionFailed {
        /// CLI flag name driving extraction: `"barcode"` or `"bclconvert"`.
        flag: &'static str,
        /// The offending qname (lossy UTF-8).
        qname: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// v1.0.0-beta.3 magic-byte detection adds new variants to
    /// `BismarkIoError`. Verify they propagate through
    /// `BismarkDedupError`'s `#[from] BismarkIoError` and that the
    /// Display impl carries the inner variant's information.
    #[test]
    fn bismark_dedup_error_from_propagates_unrecognized_bgzf_payload() {
        let io_err = BismarkIoError::UnrecognizedBgzfPayload {
            path: PathBuf::from("/tmp/x.bam"),
            payload_head: [b'V', b'C', b'F', 0x02],
        };
        let dedup_err: BismarkDedupError = io_err.into();
        let s = dedup_err.to_string();
        assert!(s.contains("/tmp/x.bam"), "Display omits inner path: {s}");
        assert!(
            s.contains("bgzipped"),
            "Display omits 'bgzipped' marker: {s}"
        );
    }
}

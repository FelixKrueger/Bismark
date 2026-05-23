//! Typed errors for `bismark-io`.
//!
//! See `DESIGN.md` Q4 and `PLAN.md` rev 3 for rationale. The crate uses
//! [`thiserror`] for typed errors so callers can match on specific failure
//! modes. Binary crates that consume `bismark-io` wrap these in
//! [`anyhow::Error`] at the orchestration layer.

use std::path::PathBuf;

/// All errors raised by `bismark-io`.
///
/// Variants describe specific failure modes — callers can `match` on them to
/// handle each appropriately (skip, abort, retry, etc.). The crate never
/// panics on user input; all malformed-input paths return one of these
/// variants.
#[derive(Debug, thiserror::Error)]
pub enum BismarkIoError {
    /// A required Bismark optional tag is absent from the record.
    ///
    /// `XR:Z:` and `XG:Z:` are required by `BismarkRecord` construction.
    #[error("missing required Bismark tag: {tag}")]
    MissingTag { tag: &'static str },

    /// A Bismark optional tag is present but cannot be decoded.
    ///
    /// For example: `XR:Z:` tag with non-UTF-8 bytes, or `NM:i:` with a
    /// non-integer value.
    #[error("malformed Bismark tag {tag}: {reason}")]
    MalformedTag { tag: &'static str, reason: String },

    /// The combination of XR/XG values is not one of the four valid
    /// Bismark strand encodings.
    ///
    /// Valid combinations: `(CT, CT) → OT`, `(GA, CT) → CTOT`,
    /// `(CT, GA) → OB`, `(GA, GA) → CTOB`.
    #[error("invalid XR/XG combination: XR={xr:?}, XG={xg:?}")]
    InvalidStrandTags { xr: Vec<u8>, xg: Vec<u8> },

    /// The XM methylation-call string length does not match the read
    /// sequence length.
    ///
    /// This is a data-integrity check. Corrupted BAMs or records re-edited
    /// by external tools can produce length-mismatched XM tags that would
    /// silently misalign methylation calls.
    #[error("XM/seq length mismatch: XM={xm_len}, seq={seq_len}")]
    XmSeqLengthMismatch { xm_len: usize, seq_len: usize },

    /// Paired-end mate validation: R1 and R2 records have different qnames.
    #[error("paired-end mate mismatch: r1 qname={r1_qname:?}, r2 qname={r2_qname:?}")]
    MateMismatch {
        r1_qname: Vec<u8>,
        r2_qname: Vec<u8>,
    },

    /// Paired-end mate validation: the read identity of a supplied record
    /// is not what was expected (e.g. caller passed two R1 records to
    /// `BismarkPair::from_mates`).
    ///
    /// Stored as a string to keep the error module decoupled from the
    /// `ReadIdentity` enum in `record.rs`.
    #[error("read identity mismatch: {description}")]
    ReadIdentityMismatch { description: String },

    /// The input BAM is coordinate-sorted (`@HD SO:coordinate`). Bismark
    /// downstream tools require name-grouped or unsorted input for
    /// paired-end work. Use `BamReader::without_sort_check()` (or
    /// equivalent) on SE-only callers to opt out.
    #[error(
        "input BAM is coordinate-sorted; Bismark downstream tools require name-grouped or unsorted input"
    )]
    UnsortedInput,

    /// A CRAM operation was requested but no reference FASTA was supplied.
    /// Pass the reference via the binary's `--cram_ref <path>` flag.
    #[error("CRAM reference required for {0} (pass --cram_ref <path>)")]
    MissingCramReference(PathBuf),

    /// A FASTA was supplied as the CRAM reference but it has no `.fai`
    /// index sidecar. Generate one with `samtools faidx <fasta>` (or
    /// equivalent) and retry.
    #[error("FASTA index (.fai) missing for CRAM reference: {0} — generate with `samtools faidx`")]
    MissingFastaIndex(PathBuf),

    /// The reconstituted multi-FASTA would contain duplicate chromosome
    /// names because multiple input FASTAs declared the same chromosome.
    /// Inspect the Bismark genome directory.
    #[error(
        "duplicate chromosome name in reconstituted reference: {name} \
         (multiple input FASTAs in the Bismark genome directory declared this chromosome)"
    )]
    DuplicateChromosomeName { name: String },

    /// The given file path does not have a recognised BAM/SAM/CRAM
    /// extension.
    #[error("unsupported file kind for path: {0}")]
    UnsupportedKind(PathBuf),

    /// Underlying I/O failure.
    ///
    /// noodles' BAM, SAM, and CRAM readers all surface their errors as
    /// `std::io::Error`, so a single `From<std::io::Error>` variant
    /// captures all of them.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

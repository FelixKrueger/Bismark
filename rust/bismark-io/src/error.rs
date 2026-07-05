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
    MissingTag {
        /// Name of the missing tag (e.g. `"XR"`, `"XG"`, `"XM"`).
        tag: &'static str,
    },

    /// A Bismark optional tag is present but cannot be decoded.
    ///
    /// For example: `XR:Z:` tag with non-UTF-8 bytes, or `NM:i:` with a
    /// non-integer value.
    #[error("malformed Bismark tag {tag}: {reason}")]
    MalformedTag {
        /// Name of the malformed tag.
        tag: &'static str,
        /// Human-readable description of the malformation.
        reason: String,
    },

    /// The combination of XR/XG values is not one of the four valid
    /// Bismark strand encodings.
    ///
    /// Valid combinations: `(CT, CT) → OT`, `(GA, CT) → CTOT`,
    /// `(CT, GA) → OB`, `(GA, GA) → CTOB`.
    #[error("invalid XR/XG combination: XR={xr:?}, XG={xg:?}")]
    InvalidStrandTags {
        /// The raw `XR:Z:` bytes that were not `b"CT"` or `b"GA"`.
        xr: Vec<u8>,
        /// The raw `XG:Z:` bytes that were not `b"CT"` or `b"GA"`.
        xg: Vec<u8>,
    },

    /// The XM methylation-call string length does not match the read
    /// sequence length.
    ///
    /// This is a data-integrity check. Corrupted BAMs or records re-edited
    /// by external tools can produce length-mismatched XM tags that would
    /// silently misalign methylation calls.
    #[error("XM/seq length mismatch: XM={xm_len}, seq={seq_len}")]
    XmSeqLengthMismatch {
        /// Length of the `XM:Z:` methylation-call string.
        xm_len: usize,
        /// Length of the read sequence (must equal `xm_len`).
        seq_len: usize,
    },

    /// Paired-end mate validation: R1 and R2 records have different qnames.
    #[error("paired-end mate mismatch: r1 qname={r1_qname:?}, r2 qname={r2_qname:?}")]
    MateMismatch {
        /// Raw qname bytes of the R1 record.
        r1_qname: Vec<u8>,
        /// Raw qname bytes of the R2 record.
        r2_qname: Vec<u8>,
    },

    /// Paired-end mate validation: the read identity of a supplied record
    /// is not what was expected.
    ///
    /// **No longer produced** as of the #1030 fix: `BismarkPair::from_mates`
    /// used to gate on the SAM first/second-in-pair FLAG bits, but Bismark
    /// deliberately swaps those bits for non-directional CTOT/CTOB pairs, so
    /// the gate rejected legitimate input. Pairing is now by file order +
    /// qname (matching Perl), and this variant has no remaining constructor.
    /// Retained for public-API stability.
    ///
    /// Stored as a string to keep the error module decoupled from the
    /// `ReadIdentity` enum in `record.rs`.
    #[error("read identity mismatch: {description}")]
    ReadIdentityMismatch {
        /// Human-readable description of which read-identity mismatch
        /// was detected (e.g. "expected R1 for first mate, got R2").
        description: String,
    },

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
    DuplicateChromosomeName {
        /// The duplicate chromosome name (lossy-decoded for display).
        name: String,
    },

    /// The given file path does not have a recognised BAM/SAM/CRAM
    /// extension.
    ///
    /// As of `bismark-io v1.0.0-beta.3`, this variant is emitted only by
    /// [`crate::AlignmentKind::from_extension`] (writer-side dispatch — the
    /// output file doesn't exist yet, so content sniffing is impossible).
    /// Reader-side dispatch via [`crate::AlignmentKind::from_path`] sniffs
    /// magic bytes and emits [`Self::UnrecognizedFormat`] or
    /// [`Self::UnrecognizedBgzfPayload`] instead.
    #[error("unsupported file kind for path: {0}")]
    UnsupportedKind(PathBuf),

    /// File was shorter than the minimum bytes needed for magic-byte
    /// format detection.
    ///
    /// Emitted by [`crate::AlignmentKind::from_path`] when the file is
    /// truncated below the 4 bytes needed to identify BAM/SAM/CRAM
    /// signatures.
    #[error("file {path} is too short to detect format ({bytes_read} bytes; need at least 4)")]
    TooShortToDetect {
        /// Path the caller tried to detect.
        path: PathBuf,
        /// Number of bytes successfully read before the file ended.
        bytes_read: usize,
    },

    /// File's first byte matched none of `1f` (BGZF/BAM), `@` (SAM
    /// header), or `C` (CRAM).
    ///
    /// Emitted by [`crate::AlignmentKind::from_path`]. Common causes:
    /// truly unrelated file types, or headerless SAM (records only, no
    /// `@HD`/`@SQ`/`@PG` lines).
    #[error(
        "file format not recognised at {path} — first byte is 0x{magic_first_byte:02x}; \
         expected BAM (`1f 8b`), SAM (starts with `@`), or CRAM (`CRAM`). \
         If this is a headerless SAM file (no `@HD`/`@SQ`/`@PG` line), \
         bismark-dedup needs the standard SAM header — try `samtools view -h` first."
    )]
    UnrecognizedFormat {
        /// Path the caller tried to detect.
        path: PathBuf,
        /// The first byte read from the file (helps the user identify the format).
        magic_first_byte: u8,
    },

    /// File starts with BGZF magic but the decompressed payload doesn't
    /// begin with `BAM\x01`. Common case: `.vcf.gz` or `.bcf` or any
    /// other bgzipped non-BAM file mis-routed to a BAM-expecting caller.
    ///
    /// Emitted by [`crate::AlignmentKind::from_path`].
    #[error(
        "file {path} is bgzipped but the decompressed payload starts with \
         {payload_head:02x?}, not `BAM\\x01` — looks like a non-BAM \
         BGZF file (VCF, BCF, BED, …?)"
    )]
    UnrecognizedBgzfPayload {
        /// Path the caller tried to detect.
        path: PathBuf,
        /// The first 4 bytes of the decompressed payload.
        payload_head: [u8; 4],
    },

    /// Underlying I/O failure.
    ///
    /// noodles' BAM, SAM, and CRAM readers all surface their errors as
    /// `std::io::Error`, so a single `From<std::io::Error>` variant
    /// captures all of them.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

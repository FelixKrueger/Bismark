//! BAM, SAM, and CRAM readers that yield [`BismarkRecord`]s.
//!
//! All three readers wrap their underlying noodles reader and produce
//! [`BismarkRecord`] via [`BismarkRecord::from_noodles_record`]. The
//! iterator-level adapter:
//!
//! - **Silently filters unmapped reads** (SAM FLAG & 0x4). The
//!   [`BismarkRecord`] constructor never sees them. Callers needing raw
//!   access to unmapped reads should use the underlying noodles reader
//!   directly.
//! - **Detects coordinate-sorted input** by inspecting the SAM header's
//!   `@HD SO:` field at construction time. Coordinate-sorted BAMs make
//!   PE work meaningless (R1 and R2 are not adjacent); raises
//!   [`BismarkIoError::UnsortedInput`]. SE-only callers can opt out by
//!   calling `without_sort_check` on the relevant reader.
//!
//! ## Path-dispatching helper
//!
//! [`open_reader`] returns an [`AnyReader`] enum that wraps the concrete
//! reader. Use this when the input format is determined at runtime (CLI
//! argument or file extension).
//!
//! **Deviation from #807's body:** the original sub-issue body specified
//! a `BismarkRecordReader` trait + `Box<dyn>` dispatch. noodles-cram 0.93
//! exposes records only via a `Records<'r, 'h, R>` iterator that borrows
//! the reader, with no stepwise `read_record_buf` equivalent. Implementing
//! an object-safe `next_record(&mut self)` would have required self-cell
//! / self-referential storage. The enum-dispatch design here achieves the
//! same functional outcome (single call site for path-dispatch) without
//! the self-referential storage problem or a new dependency.
//!
//! ## What's still deferred
//!
//! BAM/SAM/CRAM writers (Phase D), integration tests on committed fixture
//! BAM (Phase F1), and `proptest` round-trip property tests (Phase F3)
//! live in subsequent sub-issues under epic #794.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek};
use std::path::Path;

use noodles_sam::Header;

use crate::cram_ref::build_fasta_repository;
use noodles_sam::alignment::RecordBuf;
use noodles_sam::header::record::value::map::header::sort_order::COORDINATE;
use noodles_sam::header::record::value::map::header::tag::SORT_ORDER;

use crate::error::BismarkIoError;
use crate::record::BismarkRecord;

/// Recognised input file kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignmentKind {
    /// Binary alignment map.
    Bam,
    /// Sequence alignment map (plain text).
    Sam,
    /// CRAM. (Reader landed in a follow-up sub-issue under #794.)
    Cram,
}

impl AlignmentKind {
    /// Infer from a file path's **extension**. I/O-free.
    ///
    /// Returns [`BismarkIoError::UnsupportedKind`] if the extension is
    /// none of `.bam`, `.sam`, `.cram` (case-insensitive).
    ///
    /// Used by [`crate::open_writer`] (where the file doesn't exist yet,
    /// so content sniffing is impossible) and by any caller that wants
    /// explicit extension-only dispatch. Reader-side dispatch should
    /// prefer [`Self::from_path`] (magic-byte sniff).
    ///
    /// This function preserves the pre-`v1.0.0-beta.3` behaviour of
    /// `AlignmentKind::from_path` byte-for-byte; the relocation allows
    /// the magic-byte variant below to take the more general name.
    pub fn from_extension(path: &Path) -> Result<Self, BismarkIoError> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        match ext.as_deref() {
            Some("bam") => Ok(Self::Bam),
            Some("sam") => Ok(Self::Sam),
            Some("cram") => Ok(Self::Cram),
            _ => Err(BismarkIoError::UnsupportedKind(path.to_path_buf())),
        }
    }

    /// Detect the file format by reading + (for BGZF) decompressing
    /// enough bytes to authoritatively identify BAM/SAM/CRAM. Opens the
    /// file; for the BGZF/BAM case, decompresses the first block
    /// (~100-700 µs, dominated by the inflate of a 32-64 KiB BGZF block)
    /// and verifies the BAM-specific `BAM\x01` magic in the
    /// decompressed payload.
    ///
    /// Returns:
    /// - [`AlignmentKind::Bam`] if the file starts with BGZF magic AND the
    ///   decompressed first block starts with `BAM\x01`.
    /// - [`AlignmentKind::Sam`] if the first byte is `@` (SAM header).
    /// - [`AlignmentKind::Cram`] if the first 4 bytes are `CRAM`.
    /// - [`BismarkIoError::UnrecognizedFormat`] if no magic matches.
    /// - [`BismarkIoError::UnrecognizedBgzfPayload`] if BGZF magic
    ///   matched but the inflated payload isn't `BAM\x01`.
    /// - [`BismarkIoError::TooShortToDetect`] if the file is too small.
    /// - [`BismarkIoError::Io`] on a system I/O error.
    ///
    /// Used by [`crate::open_reader`] and any caller that wants
    /// tolerance for mis-named files. For writer-side dispatch (where
    /// the file doesn't exist yet) prefer [`Self::from_extension`].
    pub fn from_path(path: &Path) -> Result<Self, BismarkIoError> {
        let mut file = std::fs::File::open(path)?;
        // `Read::read` is permitted to return short even when more data
        // is available, so use the take+read_to_end loop pattern that
        // either fills the buffer or hits real EOF.
        let mut first_byte = Vec::with_capacity(1);
        (&mut file).take(1).read_to_end(&mut first_byte)?;
        if first_byte.is_empty() {
            return Err(BismarkIoError::TooShortToDetect {
                path: path.to_path_buf(),
                bytes_read: 0,
            });
        }
        match first_byte[0] {
            // BGZF/BAM: gzip magic byte 1. Re-open the file fresh and
            // let `noodles_bgzf::Reader` validate the full block, then
            // check the payload for `BAM\x01`.
            0x1f => detect_bgzf_payload(path),
            // SAM header line marker.
            b'@' => Ok(Self::Sam),
            // CRAM file definition starts with the ASCII string `CRAM`.
            b'C' => detect_cram_magic(&mut file, path),
            // No recognised magic.
            other => Err(BismarkIoError::UnrecognizedFormat {
                path: path.to_path_buf(),
                magic_first_byte: other,
            }),
        }
    }
}

/// Open the path as a BGZF stream and read 4 bytes from the
/// decompressed payload. If those bytes are `BAM\x01` it's a BAM file;
/// otherwise it's some other BGZF-wrapped format (VCF, BCF, etc.) and we
/// return [`BismarkIoError::UnrecognizedBgzfPayload`].
///
/// Implementation note: this helper re-opens the file fresh rather than
/// seeking the caller's existing `File` handle back to offset 0 and
/// wrapping it. Two reasons: (1) `noodles_bgzf::Reader::new` expects a
/// `Read` source positioned at the start of a BGZF stream — passing a
/// `File` whose cursor we've already advanced (even after `seek(0)`)
/// couples error semantics across the two read attempts; (2) the second
/// `open(2)` syscall is ~1 µs on a warm page cache, dwarfed by the
/// inflate cost. Cleaner code wins.
fn detect_bgzf_payload(path: &Path) -> Result<AlignmentKind, BismarkIoError> {
    let file = std::fs::File::open(path)?;
    let bgzf = noodles_bgzf::io::Reader::new(file);
    // `noodles_bgzf::Reader::read` returns one BGZF block at a time per
    // the `Read` contract — a single `read()` call may legally return
    // fewer than 4 bytes even when more data is available. Use
    // take+read_to_end so we loop until we have 4 bytes OR genuine EOF.
    let mut payload_head_buf = Vec::with_capacity(4);
    bgzf.take(4).read_to_end(&mut payload_head_buf)?;
    if payload_head_buf.len() < 4 {
        return Err(BismarkIoError::TooShortToDetect {
            path: path.to_path_buf(),
            bytes_read: payload_head_buf.len(),
        });
    }
    let payload_head: [u8; 4] = payload_head_buf
        .as_slice()
        .try_into()
        .expect("we just checked len == 4");
    if &payload_head == b"BAM\x01" {
        Ok(AlignmentKind::Bam)
    } else {
        Err(BismarkIoError::UnrecognizedBgzfPayload {
            path: path.to_path_buf(),
            payload_head,
        })
    }
}

/// Verify that a file whose first byte is `C` is actually CRAM (full
/// magic `CRAM`). The caller has already consumed the first byte from
/// `file`; this helper reads the remaining 3 bytes.
fn detect_cram_magic(
    file: &mut std::fs::File,
    path: &Path,
) -> Result<AlignmentKind, BismarkIoError> {
    // `Read::read` may return short even with more data available; use
    // take+read_to_end to loop until 3 bytes OR genuine EOF.
    let mut rest_buf = Vec::with_capacity(3);
    file.take(3).read_to_end(&mut rest_buf)?;
    if rest_buf.len() < 3 {
        return Err(BismarkIoError::TooShortToDetect {
            path: path.to_path_buf(),
            // First-byte peek (1) + however many we got here.
            bytes_read: 1 + rest_buf.len(),
        });
    }
    if rest_buf.as_slice() == b"RAM" {
        Ok(AlignmentKind::Cram)
    } else {
        Err(BismarkIoError::UnrecognizedFormat {
            path: path.to_path_buf(),
            magic_first_byte: b'C',
        })
    }
}

/// BAM reader producing [`BismarkRecord`]s.
///
/// Wraps `noodles_bam::io::Reader` which itself wraps a BGZF reader around
/// the underlying byte stream.
pub struct BamReader<R: BufRead> {
    inner: noodles_bam::io::Reader<noodles_bgzf::io::Reader<R>>,
    header: Header,
}

impl BamReader<BufReader<File>> {
    /// Open a BAM file from a path. Reads the header eagerly and detects
    /// coordinate sort.
    pub fn from_path(path: &Path) -> Result<Self, BismarkIoError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        Self::new(reader)
    }
}

impl<R: BufRead> BamReader<R> {
    /// Construct from any [`BufRead`] producing BAM bytes. Reads the
    /// header eagerly and rejects coordinate-sorted input.
    pub fn new(reader: R) -> Result<Self, BismarkIoError> {
        let mut inner = noodles_bam::io::Reader::new(reader);
        let header = inner.read_header()?;
        check_not_coordinate_sorted(&header)?;
        Ok(Self { inner, header })
    }

    /// Construct without rejecting coordinate-sorted input. For SE-only
    /// callers (or callers that have other reasons to accept coordinate
    /// sort).
    pub fn without_sort_check(reader: R) -> Result<Self, BismarkIoError> {
        let mut inner = noodles_bam::io::Reader::new(reader);
        let header = inner.read_header()?;
        Ok(Self { inner, header })
    }

    /// Header from the BAM file.
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Iterator yielding one [`BismarkRecord`] per mapped alignment.
    /// Unmapped reads (SAM FLAG & 0x4) are silently filtered.
    pub fn records(&mut self) -> impl Iterator<Item = Result<BismarkRecord, BismarkIoError>> + '_ {
        let header = &self.header;
        self.inner
            .record_bufs(header)
            .filter_map(filter_unmapped_then_classify)
    }

    /// Iterator yielding one [`BismarkRecord`] per mapped alignment, with
    /// the UMI pre-extracted from each record's qname using `extractor`.
    ///
    /// `extractor` is typically [`crate::umi::extract_barcode`] (for
    /// `--barcode` / `--umi` mode) or [`crate::umi::extract_bclconvert`]
    /// (for `--bclconvert` mode). Records whose qname does NOT match the
    /// extractor's pattern still flow through with `umi == None`; the
    /// downstream dedup pipeline emits `UmiExtractionFailed` faithful to
    /// Perl `deduplicate_bismark:662-663`.
    ///
    /// Added in `bismark-io` v1.0.0-beta.5 for Phase B of the v1.2 UMI epic.
    pub fn records_with_umi(
        &mut self,
        extractor: fn(&[u8]) -> Option<&[u8]>,
    ) -> impl Iterator<Item = Result<BismarkRecord, BismarkIoError>> + '_ {
        let header = &self.header;
        self.inner
            .record_bufs(header)
            .filter_map(move |item| filter_unmapped_then_classify_with_umi(item, extractor))
    }
}

/// **Threaded** BAM reader that uses [`noodles_bgzf::io::MultithreadedReader`]
/// for parallel BGZF block decompression.
///
/// This is a separate concrete type from [`BamReader`] (which is generic
/// over `R: BufRead` and uses noodles' default single-threaded BGZF
/// reader). The threaded variant always wraps a `File` directly with a
/// worker-thread pool sized at construction time.
///
/// Use this when `--parallel N > 1` is requested AND the input is BAM
/// (SAM is text — no BGZF — and CRAM uses its own container format).
/// For `N == 1`, prefer [`BamReader::from_path`] — the threaded
/// constructor always spawns at least one worker thread regardless of
/// the worker count.
///
/// Public API mirrors [`BamReader`]'s exactly: `header()`, `records()`,
/// `without_sort_check`-equivalent via a separate constructor.
///
/// Added in `bismark-io` v1.0.0-beta.2 to support `bismark-dedup`'s
/// `--parallel N` flag (parallel-BAM-I/O variant).
pub struct ThreadedBamReader {
    inner: noodles_bam::io::Reader<noodles_bgzf::io::MultithreadedReader<File>>,
    header: Header,
}

impl ThreadedBamReader {
    /// Open a BAM file with `parallel` BGZF decoder worker threads.
    /// Reads the header eagerly and rejects coordinate-sorted input.
    ///
    /// `parallel` must be ≥ 1 (enforced by the [`std::num::NonZero`]
    /// type). For `parallel == 1`, prefer [`BamReader::from_path`]
    /// — this constructor still spawns one worker thread per noodles'
    /// `MultithreadedReader` contract.
    pub fn from_path(
        path: &Path,
        parallel: std::num::NonZero<usize>,
    ) -> Result<Self, BismarkIoError> {
        let file = File::open(path)?;
        let bgzf = noodles_bgzf::io::MultithreadedReader::with_worker_count(parallel, file);
        let mut inner = noodles_bam::io::Reader::from(bgzf);
        let header = inner.read_header()?;
        check_not_coordinate_sorted(&header)?;
        Ok(Self { inner, header })
    }

    /// Open a BAM file with `parallel` workers, without rejecting
    /// coordinate-sorted input. For SE-only callers.
    pub fn from_path_without_sort_check(
        path: &Path,
        parallel: std::num::NonZero<usize>,
    ) -> Result<Self, BismarkIoError> {
        let file = File::open(path)?;
        let bgzf = noodles_bgzf::io::MultithreadedReader::with_worker_count(parallel, file);
        let mut inner = noodles_bam::io::Reader::from(bgzf);
        let header = inner.read_header()?;
        Ok(Self { inner, header })
    }

    /// Header from the BAM file.
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Iterator yielding one [`BismarkRecord`] per mapped alignment.
    /// Unmapped reads (SAM FLAG & 0x4) are silently filtered.
    pub fn records(&mut self) -> impl Iterator<Item = Result<BismarkRecord, BismarkIoError>> + '_ {
        let header = &self.header;
        self.inner
            .record_bufs(header)
            .filter_map(filter_unmapped_then_classify)
    }

    /// As [`Self::records`] but pre-extracts a UMI from each record's
    /// qname using `extractor`. See [`BamReader::records_with_umi`] for
    /// details. Added in v1.0.0-beta.5 for Phase B of the v1.2 UMI epic.
    pub fn records_with_umi(
        &mut self,
        extractor: fn(&[u8]) -> Option<&[u8]>,
    ) -> impl Iterator<Item = Result<BismarkRecord, BismarkIoError>> + '_ {
        let header = &self.header;
        self.inner
            .record_bufs(header)
            .filter_map(move |item| filter_unmapped_then_classify_with_umi(item, extractor))
    }
}

/// SAM reader producing [`BismarkRecord`]s.
pub struct SamReader<R: BufRead> {
    inner: noodles_sam::io::Reader<R>,
    header: Header,
}

impl SamReader<BufReader<File>> {
    /// Open a SAM file from a path.
    pub fn from_path(path: &Path) -> Result<Self, BismarkIoError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        Self::new(reader)
    }
}

impl<R: BufRead> SamReader<R> {
    /// Construct from any [`BufRead`] producing SAM bytes.
    pub fn new(reader: R) -> Result<Self, BismarkIoError> {
        let mut inner = noodles_sam::io::Reader::new(reader);
        let header = inner.read_header()?;
        check_not_coordinate_sorted(&header)?;
        Ok(Self { inner, header })
    }

    /// Construct without rejecting coordinate-sorted input.
    pub fn without_sort_check(reader: R) -> Result<Self, BismarkIoError> {
        let mut inner = noodles_sam::io::Reader::new(reader);
        let header = inner.read_header()?;
        Ok(Self { inner, header })
    }

    /// Header from the SAM file.
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Iterator yielding one [`BismarkRecord`] per mapped alignment.
    pub fn records(&mut self) -> impl Iterator<Item = Result<BismarkRecord, BismarkIoError>> + '_ {
        let header = &self.header;
        self.inner
            .record_bufs(header)
            .filter_map(filter_unmapped_then_classify)
    }

    /// As [`Self::records`] but pre-extracts a UMI per record. See
    /// [`BamReader::records_with_umi`] for details. Added in v1.0.0-beta.5
    /// for Phase B of the v1.2 UMI epic.
    pub fn records_with_umi(
        &mut self,
        extractor: fn(&[u8]) -> Option<&[u8]>,
    ) -> impl Iterator<Item = Result<BismarkRecord, BismarkIoError>> + '_ {
        let header = &self.header;
        self.inner
            .record_bufs(header)
            .filter_map(move |item| filter_unmapped_then_classify_with_umi(item, extractor))
    }
}

/// CRAM reader producing [`BismarkRecord`]s.
///
/// CRAM requires a reference FASTA — passed at construction time via the
/// `cram_ref` path. The FASTA must have a sibling `.fai` index file in
/// v1.0 (we use noodles-fasta's IndexedReader adapter). Auto-generating
/// the index is future work.
pub struct CramReader<R: Read + Seek> {
    inner: noodles_cram::io::Reader<R>,
    header: Header,
}

impl CramReader<File> {
    /// Open a CRAM file from a path. The `cram_ref` path must point at a
    /// FASTA reference with an existing `.fai` index alongside.
    pub fn from_path(path: &Path, cram_ref: &Path) -> Result<Self, BismarkIoError> {
        let repo = build_fasta_repository(cram_ref)?;
        let mut inner = noodles_cram::io::reader::Builder::default()
            .set_reference_sequence_repository(repo)
            .build_from_path(path)?;
        let header = inner.read_header()?;
        check_not_coordinate_sorted(&header)?;
        Ok(Self { inner, header })
    }

    /// Open without the coordinate-sort check. For SE-only callers.
    pub fn from_path_without_sort_check(
        path: &Path,
        cram_ref: &Path,
    ) -> Result<Self, BismarkIoError> {
        let repo = build_fasta_repository(cram_ref)?;
        let mut inner = noodles_cram::io::reader::Builder::default()
            .set_reference_sequence_repository(repo)
            .build_from_path(path)?;
        let header = inner.read_header()?;
        Ok(Self { inner, header })
    }
}

impl<R: Read + Seek> CramReader<R> {
    /// Header from the CRAM file.
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Iterator yielding one [`BismarkRecord`] per mapped alignment.
    /// Unmapped reads are silently filtered.
    pub fn records(&mut self) -> impl Iterator<Item = Result<BismarkRecord, BismarkIoError>> + '_ {
        let header = &self.header;
        self.inner
            .records(header)
            .filter_map(filter_unmapped_then_classify)
    }

    /// As [`Self::records`] but pre-extracts a UMI per record. See
    /// [`BamReader::records_with_umi`] for details. Added in v1.0.0-beta.5.
    pub fn records_with_umi(
        &mut self,
        extractor: fn(&[u8]) -> Option<&[u8]>,
    ) -> impl Iterator<Item = Result<BismarkRecord, BismarkIoError>> + '_ {
        let header = &self.header;
        self.inner
            .records(header)
            .filter_map(move |item| filter_unmapped_then_classify_with_umi(item, extractor))
    }
}

// `build_fasta_repository` is imported at the top of this file; it lives
// in `crate::cram_ref` so both reader and writer can share it.

/// Path-dispatching reader that returns the concrete reader for the
/// detected alignment kind. Use when the input format is determined at
/// runtime.
///
/// See module-level docs for why this is an enum rather than a `Box<dyn>`
/// trait object.
pub enum AnyReader<R: BufRead, RC: Read + Seek> {
    /// BAM-format reader.
    Bam(BamReader<R>),
    /// SAM-format reader.
    Sam(SamReader<R>),
    /// CRAM-format reader.
    Cram(CramReader<RC>),
}

impl<R: BufRead, RC: Read + Seek> AnyReader<R, RC> {
    /// Header from the underlying reader.
    pub fn header(&self) -> &Header {
        match self {
            Self::Bam(r) => r.header(),
            Self::Sam(r) => r.header(),
            Self::Cram(r) => r.header(),
        }
    }

    /// Iterator yielding one [`BismarkRecord`] per mapped alignment.
    /// Unmapped reads are silently filtered. The returned iterator is
    /// boxed (one allocation per call); per-record dispatch is via the
    /// vtable, ~5-10 ns per record per the plan's quantification.
    pub fn records(
        &mut self,
    ) -> Box<dyn Iterator<Item = Result<BismarkRecord, BismarkIoError>> + '_> {
        match self {
            Self::Bam(r) => Box::new(r.records()),
            Self::Sam(r) => Box::new(r.records()),
            Self::Cram(r) => Box::new(r.records()),
        }
    }

    /// As [`Self::records`] but pre-extracts a UMI from each record's
    /// qname using `extractor`. See [`BamReader::records_with_umi`] for
    /// details. Added in v1.0.0-beta.5 for Phase B of the v1.2 UMI epic.
    pub fn records_with_umi(
        &mut self,
        extractor: fn(&[u8]) -> Option<&[u8]>,
    ) -> Box<dyn Iterator<Item = Result<BismarkRecord, BismarkIoError>> + '_> {
        match self {
            Self::Bam(r) => Box::new(r.records_with_umi(extractor)),
            Self::Sam(r) => Box::new(r.records_with_umi(extractor)),
            Self::Cram(r) => Box::new(r.records_with_umi(extractor)),
        }
    }
}

/// Open a BAM, SAM, or CRAM file by path, dispatching on extension.
///
/// `cram_ref` is required when the path resolves to CRAM; for BAM/SAM
/// the argument is ignored. Returns [`BismarkIoError::MissingCramReference`]
/// if a CRAM file is opened without `cram_ref`.
///
/// The returned `AnyReader` enforces the coordinate-sort check by default
/// (`UnsortedInput` error for coordinate-sorted input). SE-only callers
/// that need to opt out should construct the concrete reader directly via
/// its `without_sort_check` constructor.
pub fn open_reader(
    path: &Path,
    cram_ref: Option<&Path>,
) -> Result<AnyReader<BufReader<File>, File>, BismarkIoError> {
    match AlignmentKind::from_path(path)? {
        AlignmentKind::Bam => Ok(AnyReader::Bam(BamReader::from_path(path)?)),
        AlignmentKind::Sam => Ok(AnyReader::Sam(SamReader::from_path(path)?)),
        AlignmentKind::Cram => {
            let cram_ref =
                cram_ref.ok_or_else(|| BismarkIoError::MissingCramReference(path.to_path_buf()))?;
            Ok(AnyReader::Cram(CramReader::from_path(path, cram_ref)?))
        }
    }
}

/// Filter out unmapped records (FLAG & 0x4) and classify the rest as
/// [`BismarkRecord`]. Surfaces noodles I/O errors and `BismarkIoError`
/// from classification.
fn filter_unmapped_then_classify(
    item: std::io::Result<RecordBuf>,
) -> Option<Result<BismarkRecord, BismarkIoError>> {
    match item {
        Ok(rec) => {
            let flags = u16::from(rec.flags());
            if (flags & 0x4) != 0 {
                None // unmapped — silently drop
            } else {
                Some(BismarkRecord::from_noodles_record(rec))
            }
        }
        Err(e) => Some(Err(BismarkIoError::Io(e))),
    }
}

/// Companion to [`filter_unmapped_then_classify`] that also pre-extracts
/// the UMI via `extractor`. Used by `records_with_umi` on all reader
/// variants. Added in v1.0.0-beta.5 for Phase B of the v1.2 UMI epic.
fn filter_unmapped_then_classify_with_umi(
    item: std::io::Result<RecordBuf>,
    extractor: fn(&[u8]) -> Option<&[u8]>,
) -> Option<Result<BismarkRecord, BismarkIoError>> {
    match item {
        Ok(rec) => {
            let flags = u16::from(rec.flags());
            if (flags & 0x4) != 0 {
                None
            } else {
                Some(BismarkRecord::from_noodles_record_with_umi(rec, extractor))
            }
        }
        Err(e) => Some(Err(BismarkIoError::Io(e))),
    }
}

/// Verify that the BAM/SAM header does not declare coordinate sort.
///
/// The SO sort-order field is in the HD map's `other_fields`. We compare
/// against the canonical noodles byte constant `COORDINATE`.
fn check_not_coordinate_sorted(header: &Header) -> Result<(), BismarkIoError> {
    if let Some(hd) = header.header()
        && let Some(so) = hd.other_fields().get(&SORT_ORDER)
    {
        let so_bytes: &[u8] = AsRef::as_ref(so);
        if so_bytes == COORDINATE {
            return Err(BismarkIoError::UnsortedInput);
        }
    }
    Ok(())
}

/// Auto-detect library mode (single-end vs paired-end) from a Bismark
/// BAM header.
///
/// Walks the `@PG` lines, finds the Bismark-aligner entry (ID starting
/// with `Bismark`), and inspects its command line for `-1`/`--1` AND
/// `-2`/`--2` arguments (which Bismark only passes in paired-end mode).
///
/// Returns:
/// - `Some(true)` — PE (Bismark @PG found with both `-1`/`--1` and `-2`/`--2`)
/// - `Some(false)` — SE (Bismark @PG found, missing one or both of those)
/// - `None` — no Bismark @PG line in the header; caller must error out
///   (typically by demanding the user pass `--single`/`--paired` explicitly).
///
/// Mirrors Perl `deduplicate_bismark` lines 90–116 / `filter_non_conversion`
/// `determine_file_type` lines 374–399. Promoted from
/// `bismark-dedup/src/pipeline.rs:137` in `bismark-io 1.0.0-beta.7` to share
/// the same header-detection logic with `bismark-extractor`.
///
/// **Last-Bismark-`@PG` wins.** If the header carries more than one
/// `ID:Bismark` `@PG` line, the result reflects the **last** one — matching
/// Perl's `while` loop, which re-assigns `$paired`/`$single` for every Bismark
/// `@PG` it sees so the final occurrence decides. Byte-neutral for all real
/// Bismark BAMs (exactly one Bismark `@PG`); only a re-processed BAM with two
/// distinct Bismark `@PG` lines is affected. (Fix for the two-`@PG`
/// first-vs-last divergence surfaced by the `filter_non_conversion` port's
/// code review.)
#[must_use]
pub fn detect_paired_from_header(header: &Header) -> Option<bool> {
    // Serialize the header to its on-disk SAM text representation and
    // search for the Bismark @PG line. This is robust to noodles API
    // shape changes across versions (the SAM text format is the stable
    // contract here, not the in-memory `Programs` type).
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut writer = noodles_sam::io::Writer::new(&mut buf);
        if writer.write_header(header).is_err() {
            return None;
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let mut result = None;
    for line in text.lines() {
        // SAM header @PG line format: `@PG\tID:<id>\t...\tCL:<args>...`
        // The `ID:Bismark` substring identifies the Bismark @PG.
        if !line.starts_with("@PG") || !line.contains("ID:Bismark") {
            continue;
        }
        // Look for -1/--1 AND -2/--2 in the command-line args. Bismark's
        // PE invocation always has both; SE has neither.
        // We accept space-separated, tab-separated, or end-of-line
        // boundaries to be robust to argument quoting differences.
        let has_1 = arg_present(line, "-1") || arg_present(line, "--1");
        let has_2 = arg_present(line, "-2") || arg_present(line, "--2");
        // Do NOT return here: keep scanning so the LAST Bismark @PG wins
        // (Perl re-assigns on each match).
        result = Some(has_1 && has_2);
    }
    result
}

/// True if `arg` appears as a standalone token in `text`, delimited by
/// whitespace or tab on **both** sides.
///
/// Matches Perl's `/\s+--?1\s+/` semantics: a `-1` at the very end of the
/// line (without trailing whitespace) is NOT considered present, even
/// though Bismark in practice always appends a path after `-1`/`-2`.
/// Being strict here matches Perl exactly — important for byte-identity
/// when the same input is run through both implementations.
fn arg_present(text: &str, arg: &str) -> bool {
    let arg_space = format!(" {arg} ");
    let arg_tab_left = format!("\t{arg} ");
    let arg_tab_right = format!(" {arg}\t");
    let arg_tab_both = format!("\t{arg}\t");
    text.contains(&arg_space)
        || text.contains(&arg_tab_left)
        || text.contains(&arg_tab_right)
        || text.contains(&arg_tab_both)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bstr::BString;
    use noodles_sam::header::record::value::Map;
    use noodles_sam::header::record::value::map::header::Version;
    use std::io::Cursor;
    use std::path::PathBuf;

    /// Build a SAM header with the given SO value (or no SO if `so` is None).
    fn header_with_sort_order(so: Option<&[u8]>) -> Header {
        let mut hd =
            Map::<noodles_sam::header::record::value::map::Header>::new(Version::new(1, 6));
        if let Some(so_bytes) = so {
            hd.other_fields_mut()
                .insert(SORT_ORDER, BString::from(so_bytes.to_vec()));
        }
        noodles_sam::Header::builder().set_header(hd).build()
    }

    // ─────────────── from_extension (legacy extension dispatch) ───────────────

    #[test]
    fn alignment_kind_from_extension_bam() {
        assert_eq!(
            AlignmentKind::from_extension(Path::new("x.bam")).unwrap(),
            AlignmentKind::Bam
        );
        assert_eq!(
            AlignmentKind::from_extension(Path::new("x.BAM")).unwrap(),
            AlignmentKind::Bam
        );
    }

    #[test]
    fn alignment_kind_from_extension_sam() {
        assert_eq!(
            AlignmentKind::from_extension(Path::new("x.sam")).unwrap(),
            AlignmentKind::Sam
        );
    }

    #[test]
    fn alignment_kind_from_extension_cram() {
        assert_eq!(
            AlignmentKind::from_extension(Path::new("x.cram")).unwrap(),
            AlignmentKind::Cram
        );
    }

    #[test]
    fn alignment_kind_from_extension_unknown_errors() {
        let err = AlignmentKind::from_extension(Path::new("x.txt")).unwrap_err();
        assert!(matches!(err, BismarkIoError::UnsupportedKind(_)));
    }

    #[test]
    fn alignment_kind_from_extension_no_extension_errors() {
        let err = AlignmentKind::from_extension(Path::new("noext")).unwrap_err();
        assert!(matches!(err, BismarkIoError::UnsupportedKind(_)));
    }

    // ─────────────── from_path (new magic-byte sniff) ───────────────

    /// The fixture BAM produced by Perl Bismark is the canonical
    /// real-data BAM for sniff verification.
    #[test]
    fn from_path_detects_bam_via_bgzf_payload_on_fixture() {
        let fixture = Path::new("test_files/tiny_pe_bismark.bam");
        assert_eq!(
            AlignmentKind::from_path(fixture).unwrap(),
            AlignmentKind::Bam
        );
    }

    /// SAM bytes in a file: classification follows content, not extension.
    /// The temp file has no specific extension (NamedTempFile gives it a
    /// random one), but `from_path` should ignore extension and classify
    /// by the `@HD` first-byte content.
    #[test]
    fn from_path_detects_sam_by_at_sign_first_byte() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"@HD\tVN:1.6\n").unwrap();
        assert_eq!(
            AlignmentKind::from_path(tmp.path()).unwrap(),
            AlignmentKind::Sam
        );
    }

    /// CRAM magic in a file with no extension at all.
    #[test]
    fn from_path_detects_cram_by_magic() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"CRAM\x03\x00").unwrap();
        assert_eq!(
            AlignmentKind::from_path(tmp.path()).unwrap(),
            AlignmentKind::Cram
        );
    }

    #[test]
    fn from_path_errors_on_empty_file() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        // Empty file (NamedTempFile::new is empty by default).
        match AlignmentKind::from_path(tmp.path()).unwrap_err() {
            BismarkIoError::TooShortToDetect { bytes_read: 0, .. } => {}
            other => panic!("expected TooShortToDetect, got {other:?}"),
        }
    }

    #[test]
    fn from_path_errors_on_unrecognized_first_byte() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"XXXX").unwrap();
        match AlignmentKind::from_path(tmp.path()).unwrap_err() {
            BismarkIoError::UnrecognizedFormat {
                magic_first_byte: b'X',
                ..
            } => {}
            other => panic!("expected UnrecognizedFormat with byte=0x58, got {other:?}"),
        }
    }

    #[test]
    fn from_path_errors_on_missing_file() {
        let err = AlignmentKind::from_path(Path::new("/nonexistent/path/should-not-exist.bam"))
            .unwrap_err();
        assert!(matches!(err, BismarkIoError::Io(_)));
    }

    #[test]
    fn from_path_errors_on_partial_cram_magic() {
        // 2 bytes: matches `C` first-byte dispatch, then reads 1 of the
        // expected 3 trailing bytes before EOF. Total bytes read = 1 + 1 = 2.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"CR").unwrap();
        match AlignmentKind::from_path(tmp.path()).unwrap_err() {
            BismarkIoError::TooShortToDetect { bytes_read: 2, .. } => {}
            other => panic!("expected TooShortToDetect with bytes_read=2, got {other:?}"),
        }
    }

    /// End-to-end test for the load-bearing case: a real BGZF stream
    /// whose decompressed payload starts with non-BAM bytes (e.g. a
    /// `.vcf.gz` mis-routed to a BAM-expecting caller). Synthesizes the
    /// BGZF wrapper via `noodles_bgzf::io::Writer` so the BGZF block
    /// structure is spec-valid.
    #[test]
    fn from_path_rejects_bgzf_non_bam_payload() {
        use std::io::Write;

        let tmp = tempfile::NamedTempFile::new().unwrap();
        // Build a valid BGZF stream whose payload starts with VCF magic.
        {
            let file = std::fs::File::create(tmp.path()).unwrap();
            let mut bgzf = noodles_bgzf::io::Writer::new(file);
            bgzf.write_all(b"##fileformat=VCFv4.2\n").unwrap();
            bgzf.finish().unwrap();
        }
        match AlignmentKind::from_path(tmp.path()).unwrap_err() {
            BismarkIoError::UnrecognizedBgzfPayload { payload_head, .. } => {
                // The first 4 decompressed bytes are `##fi`.
                assert_eq!(
                    &payload_head, b"##fi",
                    "payload_head should reflect the first 4 inflated bytes"
                );
            }
            other => panic!("expected UnrecognizedBgzfPayload, got {other:?}"),
        }
    }

    /// New `UnrecognizedBgzfPayload` Display: includes path + hex head.
    #[test]
    fn unrecognized_bgzf_payload_display_includes_path_and_head() {
        let err = BismarkIoError::UnrecognizedBgzfPayload {
            path: PathBuf::from("/tmp/x.bam"),
            payload_head: [b'V', b'C', b'F', 0x02],
        };
        let s = err.to_string();
        assert!(s.contains("/tmp/x.bam"), "Display omits path: {s}");
        assert!(s.contains("bgzipped"), "Display omits 'bgzipped': {s}");
        assert!(s.contains("BAM"), "Display omits 'BAM' reference: {s}");
    }

    /// `UnrecognizedFormat` Display: includes the `samtools view -h` hint
    /// so users with headerless SAM get an actionable next step.
    #[test]
    fn unrecognized_format_display_includes_samtools_hint() {
        let err = BismarkIoError::UnrecognizedFormat {
            path: PathBuf::from("/tmp/y"),
            magic_first_byte: b'X',
        };
        let s = err.to_string();
        assert!(
            s.contains("samtools view -h"),
            "Display omits samtools-view-h hint: {s}"
        );
        assert!(s.contains("0x58"), "Display omits hex first-byte: {s}");
    }

    #[test]
    fn check_not_coordinate_sorted_rejects_coordinate() {
        let header = header_with_sort_order(Some(b"coordinate"));
        let err = check_not_coordinate_sorted(&header).unwrap_err();
        assert!(matches!(err, BismarkIoError::UnsortedInput));
    }

    #[test]
    fn check_not_coordinate_sorted_allows_queryname() {
        let header = header_with_sort_order(Some(b"queryname"));
        assert!(check_not_coordinate_sorted(&header).is_ok());
    }

    #[test]
    fn check_not_coordinate_sorted_allows_unsorted() {
        let header = header_with_sort_order(Some(b"unsorted"));
        assert!(check_not_coordinate_sorted(&header).is_ok());
    }

    #[test]
    fn check_not_coordinate_sorted_allows_unknown() {
        let header = header_with_sort_order(Some(b"unknown"));
        assert!(check_not_coordinate_sorted(&header).is_ok());
    }

    #[test]
    fn check_not_coordinate_sorted_allows_no_so_field() {
        let header = header_with_sort_order(None);
        assert!(check_not_coordinate_sorted(&header).is_ok());
    }

    #[test]
    fn check_not_coordinate_sorted_allows_no_hd_at_all() {
        // A header with no @HD record at all (Default header).
        let header = noodles_sam::Header::default();
        assert!(check_not_coordinate_sorted(&header).is_ok());
    }

    /// Minimal SAM bytes with a single mapped record carrying valid Bismark
    /// tags. Useful for iterator-level integration tests.
    const SAM_ONE_MAPPED: &[u8] = b"@HD\tVN:1.6\tSO:unsorted\n\
@SQ\tSN:chr1\tLN:1000\n\
read1\t0\tchr1\t10\t60\t5M\t*\t0\t0\tACGTC\tIIIII\tXM:Z:.....\tXR:Z:CT\tXG:Z:CT\n";

    /// SAM with one mapped + one unmapped record. Unmapped has FLAG 0x4.
    const SAM_MAPPED_AND_UNMAPPED: &[u8] = b"@HD\tVN:1.6\tSO:unsorted\n\
@SQ\tSN:chr1\tLN:1000\n\
mapped_read\t0\tchr1\t10\t60\t5M\t*\t0\t0\tACGTC\tIIIII\tXM:Z:.....\tXR:Z:CT\tXG:Z:CT\n\
unmapped_read\t4\t*\t0\t0\t*\t*\t0\t0\tACGTC\tIIIII\n";

    /// SAM with one record that is missing the XR tag entirely.
    const SAM_MISSING_XR: &[u8] = b"@HD\tVN:1.6\tSO:unsorted\n\
@SQ\tSN:chr1\tLN:1000\n\
read1\t0\tchr1\t10\t60\t5M\t*\t0\t0\tACGTC\tIIIII\tXM:Z:.....\tXG:Z:CT\n";

    #[test]
    fn sam_reader_yields_mapped_record() {
        let mut reader = SamReader::new(Cursor::new(SAM_ONE_MAPPED)).unwrap();
        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 1);
        let rec = records.into_iter().next().unwrap().unwrap();
        assert_eq!(rec.record_strand(), crate::BismarkStrand::OT);
    }

    #[test]
    fn sam_reader_silently_drops_unmapped_records() {
        let mut reader = SamReader::new(Cursor::new(SAM_MAPPED_AND_UNMAPPED)).unwrap();
        let records: Vec<_> = reader.records().collect();
        // Only the mapped record should appear; unmapped is silently filtered.
        assert_eq!(
            records.len(),
            1,
            "unmapped read (FLAG & 0x4) must be silently dropped, not surfaced"
        );
        let rec = records.into_iter().next().unwrap().unwrap();
        // Verify it's the mapped one by checking we got a successful BismarkRecord.
        let _ = rec.record_strand();
    }

    #[test]
    fn sam_reader_propagates_missing_tag_error() {
        let mut reader = SamReader::new(Cursor::new(SAM_MISSING_XR)).unwrap();
        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 1);
        let err = records.into_iter().next().unwrap().unwrap_err();
        assert!(
            matches!(err, BismarkIoError::MissingTag { tag: "XR" }),
            "expected MissingTag {{ tag: \"XR\" }}, got {err:?}"
        );
    }

    // ---- AnyReader + open_reader tests ----

    fn expect_err<T>(r: Result<T, BismarkIoError>) -> BismarkIoError {
        // Helper: AnyReader/CramReader don't impl Debug (noodles internals
        // don't), so `unwrap_err()` won't compile. Match-and-panic instead.
        match r {
            Ok(_) => panic!("expected Err, got Ok"),
            Err(e) => e,
        }
    }

    #[test]
    fn open_reader_cram_without_cram_ref_errors() {
        // A CRAM-magic file with no cram_ref should fail with
        // MissingCramReference. Since v1.0.0-beta.3 `open_reader` uses
        // magic-byte sniff (not extension), the file must exist for the
        // sniff to detect CRAM before the cram_ref check fires.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"CRAM\x03\x00").unwrap();
        let err = expect_err(open_reader(tmp.path(), None));
        assert!(
            matches!(err, BismarkIoError::MissingCramReference(_)),
            "expected MissingCramReference, got {err:?}"
        );
    }

    #[test]
    fn open_reader_unrecognized_format_errors() {
        // A file whose contents match no recognised magic byte:
        // post-beta.3 this is `UnrecognizedFormat`, regardless of
        // extension (which is no longer consulted).
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"XXXX").unwrap();
        let err = expect_err(open_reader(tmp.path(), None));
        assert!(
            matches!(
                err,
                BismarkIoError::UnrecognizedFormat {
                    magic_first_byte: b'X',
                    ..
                }
            ),
            "expected UnrecognizedFormat with byte=0x58, got {err:?}"
        );
    }

    #[test]
    fn open_reader_too_short_to_detect_errors() {
        // Empty file: too short for any magic. Post-beta.3 this is
        // TooShortToDetect.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let err = expect_err(open_reader(tmp.path(), None));
        assert!(
            matches!(err, BismarkIoError::TooShortToDetect { bytes_read: 0, .. }),
            "expected TooShortToDetect with bytes_read=0, got {err:?}"
        );
    }

    #[test]
    fn any_reader_sam_variant_delegates_header_and_records() {
        // Construct AnyReader::Sam manually and verify header() + records()
        // delegate correctly to the underlying SamReader.
        let inner = SamReader::new(Cursor::new(SAM_ONE_MAPPED)).unwrap();
        // AnyReader is generic over the inner reader types; for this test,
        // both type parameters are Cursor<&[u8]> and File respectively. We
        // only use the Sam variant, so the RC param is whatever phantom
        // type the compiler infers from the constructor.
        let mut any: AnyReader<Cursor<&[u8]>, File> = AnyReader::Sam(inner);
        // header() works
        let _hdr = any.header();
        // records() yields exactly the one mapped record from the fixture
        let records: Vec<_> = any.records().collect();
        assert_eq!(records.len(), 1);
        assert!(records[0].as_ref().is_ok());
    }

    #[test]
    fn any_reader_sam_variant_silently_drops_unmapped() {
        // Verify AnyReader inherits the iterator-level unmapped filter
        // behavior from the underlying SamReader.
        let inner = SamReader::new(Cursor::new(SAM_MAPPED_AND_UNMAPPED)).unwrap();
        let mut any: AnyReader<Cursor<&[u8]>, File> = AnyReader::Sam(inner);
        let records: Vec<_> = any.records().collect();
        assert_eq!(
            records.len(),
            1,
            "AnyReader must inherit the unmapped silent filter"
        );
    }

    // ─── Phase B (v1.2 UMI): records_with_umi() reader tests ───────────

    /// SAM with one mapped record whose qname has a `--barcode`-format
    /// UMI tail. Used to verify `records_with_umi` populates the `umi`
    /// field at parse time.
    const SAM_ONE_MAPPED_WITH_UMI: &[u8] = b"@HD\tVN:1.6\tSO:unsorted\n\
@SQ\tSN:chr1\tLN:1000\n\
read1:CTCCTTAG\t0\tchr1\t10\t60\t5M\t*\t0\t0\tACGTC\tIIIII\tXM:Z:.....\tXR:Z:CT\tXG:Z:CT\n";

    #[test]
    fn sam_records_with_umi_populates_umi_field() {
        let mut reader = SamReader::new(Cursor::new(SAM_ONE_MAPPED_WITH_UMI)).unwrap();
        let records: Vec<_> = reader
            .records_with_umi(crate::umi::extract_barcode)
            .collect();
        assert_eq!(records.len(), 1);
        let rec = records.into_iter().next().unwrap().unwrap();
        assert_eq!(rec.umi().unwrap().as_slice(), b"CTCCTTAG");
    }

    #[test]
    fn sam_records_with_umi_no_umi_in_qname_yields_none() {
        // SAM_ONE_MAPPED's qname is `read1` (no `:`). With --barcode mode,
        // extractor returns None → record's umi field is None → dedup
        // pipeline will surface UmiExtractionFailed downstream.
        let mut reader = SamReader::new(Cursor::new(SAM_ONE_MAPPED)).unwrap();
        let records: Vec<_> = reader
            .records_with_umi(crate::umi::extract_barcode)
            .collect();
        assert_eq!(records.len(), 1);
        let rec = records.into_iter().next().unwrap().unwrap();
        assert!(rec.umi().is_none());
    }

    #[test]
    fn cram_reader_from_path_missing_fai_errors() {
        // The .fai existence check short-circuits before the cram is opened.
        // For a nonexistent cram_ref path, the .fai sibling also doesn't
        // exist → MissingFastaIndex with the .fai path embedded.
        let nonexistent_cram = Path::new("/tmp/bismark_io_definitely_nonexistent_88912.cram");
        let nonexistent_ref = Path::new("/tmp/bismark_io_definitely_nonexistent_88912.fa");
        let err = expect_err(CramReader::from_path(nonexistent_cram, nonexistent_ref));
        assert!(
            matches!(err, BismarkIoError::MissingFastaIndex(_)),
            "expected MissingFastaIndex (the .fai sidecar of the nonexistent ref \
             doesn't exist), got {err:?}"
        );
    }

    // ─── detect_paired_from_header tests (promoted from bismark-dedup in v1.0.0-beta.7) ───

    use noodles_sam::header::record::value::map::Program;

    /// Build a SAM header with a `@PG` line whose ID and CL fields are as
    /// given. CL is set as-is (verbatim) so tests can construct PE vs SE
    /// arg patterns.
    fn header_with_pg(id: &str, cl: Option<&str>) -> Header {
        let mut builder = Header::builder();
        let mut prog = Map::<Program>::default();
        if let Some(cl_text) = cl {
            use noodles_sam::header::record::value::map::program::tag::COMMAND_LINE;
            prog.other_fields_mut()
                .insert(COMMAND_LINE, BString::from(cl_text.as_bytes().to_vec()));
        }
        builder = builder.add_program(BString::from(id.as_bytes().to_vec()), prog);
        builder.build()
    }

    #[test]
    fn detect_paired_from_header_returns_some_true_for_pe_bismark_pg() {
        let header = header_with_pg(
            "Bismark",
            Some("bismark --genome /path/genome -1 R1.fq.gz -2 R2.fq.gz"),
        );
        assert_eq!(detect_paired_from_header(&header), Some(true));
    }

    #[test]
    fn detect_paired_from_header_returns_some_false_for_se_bismark_pg() {
        let header = header_with_pg("Bismark", Some("bismark --genome /path/genome reads.fq.gz"));
        assert_eq!(detect_paired_from_header(&header), Some(false));
    }

    #[test]
    fn detect_paired_from_header_returns_none_when_no_bismark_pg() {
        let header = header_with_pg("bowtie2", Some("bowtie2 -x index -U reads.fq.gz"));
        assert_eq!(detect_paired_from_header(&header), None);
    }

    #[test]
    fn detect_paired_from_header_returns_none_for_empty_header() {
        let header = Header::default();
        assert_eq!(detect_paired_from_header(&header), None);
    }

    #[test]
    fn detect_paired_from_header_accepts_double_dash_form() {
        // Bismark also accepts `--1` / `--2` (long form).
        let header = header_with_pg("Bismark_v0.25.1", Some("bismark --1 R1.fq --2 R2.fq"));
        assert_eq!(detect_paired_from_header(&header), Some(true));
    }

    #[test]
    fn arg_present_strict_boundary_check() {
        // Token must have whitespace/tab on both sides — `-1` at end-of-line
        // (no trailing space) is NOT considered present. Matches Perl's
        // `/\s+--?1\s+/` strict semantics.
        assert!(arg_present("foo -1 bar", "-1"));
        assert!(arg_present("foo\t-1\tbar", "-1"));
        assert!(arg_present("foo -1\tbar", "-1"));
        assert!(arg_present("foo\t-1 bar", "-1"));
        assert!(!arg_present("foo -1", "-1"));
        assert!(!arg_present("-1 bar", "-1"));
        assert!(!arg_present("foo--1 bar", "-1")); // no preceding boundary
    }

    /// Build a header with TWO Bismark `@PG` lines in the given CL order.
    fn header_with_two_bismark_pg(first_cl: &str, second_cl: &str) -> Header {
        use noodles_sam::header::record::value::map::program::tag::COMMAND_LINE;
        let mut p1 = Map::<Program>::default();
        p1.other_fields_mut()
            .insert(COMMAND_LINE, BString::from(first_cl.as_bytes().to_vec()));
        let mut p2 = Map::<Program>::default();
        p2.other_fields_mut()
            .insert(COMMAND_LINE, BString::from(second_cl.as_bytes().to_vec()));
        // Distinct IDs (SAM requires unique @PG IDs); both contain "ID:Bismark".
        Header::builder()
            .add_program(BString::from("Bismark"), p1)
            .add_program(BString::from("Bismark.1"), p2)
            .build()
    }

    #[test]
    fn detect_paired_two_bismark_pg_last_wins_se() {
        // PE-style @PG first, SE-style @PG last → the LAST wins → SE.
        // Matches Perl `determine_file_type`'s re-assign-on-each-match loop.
        let header = header_with_two_bismark_pg(
            "bismark --genome /g -1 R1.fq -2 R2.fq",
            "bismark --genome /g reads.fq",
        );
        assert_eq!(detect_paired_from_header(&header), Some(false));
    }

    #[test]
    fn detect_paired_two_bismark_pg_last_wins_pe() {
        // SE-style first, PE-style last → the LAST wins → PE.
        let header = header_with_two_bismark_pg(
            "bismark --genome /g reads.fq",
            "bismark --genome /g -1 R1.fq -2 R2.fq",
        );
        assert_eq!(detect_paired_from_header(&header), Some(true));
    }
}

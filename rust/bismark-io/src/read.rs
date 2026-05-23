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
    /// Infer from a file path's extension. Returns
    /// [`BismarkIoError::UnsupportedKind`] if the extension is none of
    /// `.bam`, `.sam`, `.cram`.
    pub fn from_path(path: &Path) -> Result<Self, BismarkIoError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use bstr::BString;
    use noodles_sam::header::record::value::Map;
    use noodles_sam::header::record::value::map::header::Version;
    use std::io::Cursor;

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

    #[test]
    fn alignment_kind_from_path_bam() {
        assert_eq!(
            AlignmentKind::from_path(Path::new("x.bam")).unwrap(),
            AlignmentKind::Bam
        );
        assert_eq!(
            AlignmentKind::from_path(Path::new("x.BAM")).unwrap(),
            AlignmentKind::Bam
        );
    }

    #[test]
    fn alignment_kind_from_path_sam() {
        assert_eq!(
            AlignmentKind::from_path(Path::new("x.sam")).unwrap(),
            AlignmentKind::Sam
        );
    }

    #[test]
    fn alignment_kind_from_path_cram() {
        assert_eq!(
            AlignmentKind::from_path(Path::new("x.cram")).unwrap(),
            AlignmentKind::Cram
        );
    }

    #[test]
    fn alignment_kind_from_path_unknown_errors() {
        let err = AlignmentKind::from_path(Path::new("x.txt")).unwrap_err();
        assert!(matches!(err, BismarkIoError::UnsupportedKind(_)));
    }

    #[test]
    fn alignment_kind_from_path_no_extension_errors() {
        let err = AlignmentKind::from_path(Path::new("noext")).unwrap_err();
        assert!(matches!(err, BismarkIoError::UnsupportedKind(_)));
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
        // A .cram path with no cram_ref should fail with MissingCramReference,
        // even before any file I/O is attempted (we check the missing-ref
        // condition before opening the CRAM).
        let err = expect_err(open_reader(Path::new("nonexistent.cram"), None));
        assert!(
            matches!(err, BismarkIoError::MissingCramReference(_)),
            "expected MissingCramReference, got {err:?}"
        );
    }

    #[test]
    fn open_reader_unsupported_extension_errors() {
        let err = expect_err(open_reader(Path::new("foo.txt"), None));
        assert!(matches!(err, BismarkIoError::UnsupportedKind(_)));
    }

    #[test]
    fn open_reader_no_extension_errors() {
        let err = expect_err(open_reader(Path::new("noext"), None));
        assert!(matches!(err, BismarkIoError::UnsupportedKind(_)));
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
}

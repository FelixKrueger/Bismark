//! BAM and SAM readers that yield [`BismarkRecord`]s.
//!
//! Both readers wrap their underlying noodles reader and produce
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
//!   calling [`BamReader::without_sort_check`].
//!
//! CRAM reader, BAM/SAM writers, and CRAM writer are delivered in
//! subsequent sub-issues under epic #794.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use noodles_sam::Header;
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
}

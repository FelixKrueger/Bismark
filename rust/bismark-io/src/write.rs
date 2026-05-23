//! BAM, SAM, and CRAM writers consuming [`BismarkRecord`]s.
//!
//! Mirrors the structure of [`crate::read`]:
//!
//! - `BamWriter<W>`, `SamWriter<W>`, `CramWriter<W>` are the concrete
//!   writers. Each has a `from_path` constructor and a generic `new`.
//! - `AnyWriter` enum + `open_writer` path-dispatcher cover the
//!   "format is determined at runtime" case.
//!
//! Headers are written eagerly at construction time. `finish()` flushes +
//! finalises (BAM end-of-file marker, CRAM EOF block, SAM no-op). Callers
//! MUST call `finish()` (taking `self` by value) before dropping — drop
//! alone does not finalise. This matches the noodles writer convention.
//!
//! ## CRAM compression
//!
//! Defaults to lossless compression. Acceptance is **semantic** round-
//! trip (qname/flag/position/CIGAR/seq/qual/required-tags), not byte-
//! identical CRAM container output.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use noodles_sam::Header;
use noodles_sam::alignment::io::Write as SamAlignmentWrite;

use crate::cram_ref::build_fasta_repository;
use crate::error::BismarkIoError;
use crate::read::AlignmentKind;
use crate::record::BismarkRecord;

/// BAM writer producing BGZF-compressed BAM output.
pub struct BamWriter<W: Write> {
    inner: noodles_bam::io::Writer<noodles_bgzf::io::Writer<W>>,
    header: Header,
}

impl BamWriter<BufWriter<File>> {
    /// Create a BAM writer at `path`.
    pub fn from_path(path: &Path, header: Header) -> Result<Self, BismarkIoError> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        Self::new(writer, header)
    }
}

impl<W: Write> BamWriter<W> {
    /// Create a BAM writer over any `Write`. Writes the header eagerly.
    pub fn new(writer: W, header: Header) -> Result<Self, BismarkIoError> {
        let mut inner = noodles_bam::io::Writer::new(writer);
        inner.write_header(&header)?;
        Ok(Self { inner, header })
    }

    /// Header that was written at construction time.
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Write one `BismarkRecord` to the BAM stream.
    pub fn write_record(&mut self, record: &BismarkRecord) -> Result<(), BismarkIoError> {
        self.inner
            .write_alignment_record(&self.header, record.inner())?;
        Ok(())
    }

    /// Finalise the BAM stream (writes the BGZF end-of-file marker).
    /// Consumes `self`.
    pub fn finish(mut self) -> Result<(), BismarkIoError> {
        SamAlignmentWrite::finish(&mut self.inner, &self.header)?;
        Ok(())
    }
}

/// SAM writer producing uncompressed SAM text.
pub struct SamWriter<W: Write> {
    inner: noodles_sam::io::Writer<W>,
    header: Header,
}

impl SamWriter<BufWriter<File>> {
    /// Create a SAM writer at `path`.
    pub fn from_path(path: &Path, header: Header) -> Result<Self, BismarkIoError> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        Self::new(writer, header)
    }
}

impl<W: Write> SamWriter<W> {
    /// Create a SAM writer over any `Write`. Writes the header eagerly.
    pub fn new(writer: W, header: Header) -> Result<Self, BismarkIoError> {
        let mut inner = noodles_sam::io::Writer::new(writer);
        inner.write_header(&header)?;
        Ok(Self { inner, header })
    }

    /// Header that was written at construction time.
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Write one `BismarkRecord` to the SAM stream.
    pub fn write_record(&mut self, record: &BismarkRecord) -> Result<(), BismarkIoError> {
        self.inner
            .write_alignment_record(&self.header, record.inner())?;
        Ok(())
    }

    /// Finalise the SAM stream (no-op for plain text; flushes the wrapped
    /// writer). Consumes `self`.
    pub fn finish(mut self) -> Result<(), BismarkIoError> {
        SamAlignmentWrite::finish(&mut self.inner, &self.header)?;
        Ok(())
    }
}

/// CRAM writer.
///
/// Requires the same reference FASTA as the reader; passed at
/// construction time. The FASTA must have a sibling `.fai` index in v1.0
/// (see [`crate::cram_ref::build_fasta_repository`]). Auto-fai-generation
/// is future work.
pub struct CramWriter<W: Write> {
    inner: noodles_cram::io::Writer<W>,
    header: Header,
}

impl CramWriter<File> {
    /// Create a CRAM writer at `path`, using `cram_ref` as the reference
    /// FASTA. The FASTA must have a sibling `.fai` index.
    pub fn from_path(path: &Path, header: Header, cram_ref: &Path) -> Result<Self, BismarkIoError> {
        let repo = build_fasta_repository(cram_ref)?;
        let mut inner = noodles_cram::io::writer::Builder::default()
            .set_reference_sequence_repository(repo)
            .build_from_path(path)?;
        inner.write_header(&header)?;
        Ok(Self { inner, header })
    }
}

impl<W: Write> CramWriter<W> {
    /// Header that was written at construction time.
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Write one `BismarkRecord` to the CRAM stream.
    pub fn write_record(&mut self, record: &BismarkRecord) -> Result<(), BismarkIoError> {
        self.inner
            .write_alignment_record(&self.header, record.inner())?;
        Ok(())
    }

    /// Finalise the CRAM stream (writes the EOF container). Consumes
    /// `self`.
    pub fn finish(mut self) -> Result<(), BismarkIoError> {
        SamAlignmentWrite::finish(&mut self.inner, &self.header)?;
        Ok(())
    }
}

/// Path-dispatching writer enum. Mirrors [`crate::read::AnyReader`] for
/// write-side use. The enum-dispatch shape (rather than `Box<dyn>`) was
/// chosen for the same reason as the reader: noodles-cram's writer
/// lifetime story is awkward to express via a dyn trait.
pub enum AnyWriter<W: Write, WC: Write> {
    /// BAM-format writer.
    Bam(BamWriter<W>),
    /// SAM-format writer.
    Sam(SamWriter<W>),
    /// CRAM-format writer.
    Cram(CramWriter<WC>),
}

impl<W: Write, WC: Write> AnyWriter<W, WC> {
    /// Header from the underlying writer.
    pub fn header(&self) -> &Header {
        match self {
            Self::Bam(w) => w.header(),
            Self::Sam(w) => w.header(),
            Self::Cram(w) => w.header(),
        }
    }

    /// Write one `BismarkRecord` to the underlying writer.
    pub fn write_record(&mut self, record: &BismarkRecord) -> Result<(), BismarkIoError> {
        match self {
            Self::Bam(w) => w.write_record(record),
            Self::Sam(w) => w.write_record(record),
            Self::Cram(w) => w.write_record(record),
        }
    }

    /// Finalise the underlying writer. Consumes `self`.
    pub fn finish(self) -> Result<(), BismarkIoError> {
        match self {
            Self::Bam(w) => w.finish(),
            Self::Sam(w) => w.finish(),
            Self::Cram(w) => w.finish(),
        }
    }
}

/// Open a writer at `path`, dispatching on file extension.
///
/// CRAM requires a `cram_ref`; BAM/SAM ignore it.
pub fn open_writer(
    path: &Path,
    header: Header,
    cram_ref: Option<&Path>,
) -> Result<AnyWriter<BufWriter<File>, File>, BismarkIoError> {
    match AlignmentKind::from_path(path)? {
        AlignmentKind::Bam => Ok(AnyWriter::Bam(BamWriter::from_path(path, header)?)),
        AlignmentKind::Sam => Ok(AnyWriter::Sam(SamWriter::from_path(path, header)?)),
        AlignmentKind::Cram => {
            let cram_ref =
                cram_ref.ok_or_else(|| BismarkIoError::MissingCramReference(path.to_path_buf()))?;
            Ok(AnyWriter::Cram(CramWriter::from_path(
                path, header, cram_ref,
            )?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BismarkStrand;
    use crate::read::{BamReader, SamReader};
    use bstr::BString;
    use noodles_sam::alignment::RecordBuf;
    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::Sequence;
    use noodles_sam::alignment::record_buf::data::field::Value;
    use noodles_sam::header::record::value::Map;
    use noodles_sam::header::record::value::map::ReferenceSequence;
    use noodles_sam::header::record::value::map::header::Version;
    use std::io::Cursor;
    use std::num::NonZeroUsize;
    use tempfile::TempDir;

    fn synth_record_buf() -> RecordBuf {
        let mut record = RecordBuf::default();
        *record.name_mut() = Some(BString::from("read1".as_bytes().to_vec()));
        // FLAG=0 (mapped, no R1/R2 bits)
        *record.flags_mut() = noodles_sam::alignment::record::Flags::from(0u16);
        *record.reference_sequence_id_mut() = Some(0);
        *record.alignment_start_mut() = Some(noodles_core::Position::try_from(10).unwrap());
        *record.sequence_mut() = Sequence::from(b"ACGTC".to_vec());
        record
            .data_mut()
            .insert(Tag::from(*b"XM"), Value::String(BString::from(".....")));
        record
            .data_mut()
            .insert(Tag::from(*b"XR"), Value::String(BString::from("CT")));
        record
            .data_mut()
            .insert(Tag::from(*b"XG"), Value::String(BString::from("CT")));
        record
    }

    fn synth_header() -> Header {
        let hd = Map::<noodles_sam::header::record::value::map::Header>::new(Version::new(1, 6));
        let mut header = noodles_sam::Header::builder().set_header(hd).build();
        header.reference_sequences_mut().insert(
            BString::from("chr1"),
            Map::<ReferenceSequence>::new(NonZeroUsize::try_from(1000).unwrap()),
        );
        header
    }

    fn synth_bismark_record() -> BismarkRecord {
        BismarkRecord::from_noodles_record(synth_record_buf()).unwrap()
    }

    #[test]
    fn sam_writer_roundtrip_via_cursor() {
        // Write a SAM with one record, then read it back; assert strand
        // classification survives the round-trip.
        let header = synth_header();
        let rec = synth_bismark_record();

        let buf: Vec<u8> = Vec::new();
        let mut writer = SamWriter::new(buf, header).unwrap();
        writer.write_record(&rec).unwrap();
        let buf = writer.inner.into_inner();
        // (use into_inner instead of finish() since SamWriter::finish takes self;
        // we can't easily extract the inner buf after finish(). Workaround for tests.)

        // The SAM bytes should be parseable by SamReader.
        let mut reader = SamReader::new(Cursor::new(buf)).unwrap();
        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 1);
        let read_back = records.into_iter().next().unwrap().unwrap();
        assert_eq!(read_back.record_strand(), BismarkStrand::OT);
        assert_eq!(read_back.xm(), b".....");
    }

    #[test]
    fn bam_writer_roundtrip_via_tempfile() {
        // BAM is BGZF-compressed binary; testing in-memory is awkward
        // because the BgzfWriter's into_inner consumes the wrapper.
        // A tempfile-based round-trip is the cleanest.
        let tmp = TempDir::new().unwrap();
        let bam_path = tmp.path().join("test.bam");

        let header = synth_header();
        let rec = synth_bismark_record();
        {
            let mut writer = BamWriter::from_path(&bam_path, header).unwrap();
            writer.write_record(&rec).unwrap();
            writer.finish().unwrap();
        }

        let mut reader = BamReader::from_path(&bam_path).unwrap();
        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 1);
        let read_back = records.into_iter().next().unwrap().unwrap();
        assert_eq!(read_back.record_strand(), BismarkStrand::OT);
        assert_eq!(read_back.xm(), b".....");
    }

    #[test]
    fn open_writer_dispatches_on_extension_bam() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("out.bam");
        let writer = open_writer(&path, synth_header(), None).unwrap();
        assert!(matches!(writer, AnyWriter::Bam(_)));
        // Properly finalise so the tempdir cleanup doesn't see leftover handles.
        writer.finish().unwrap();
    }

    #[test]
    fn open_writer_dispatches_on_extension_sam() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("out.sam");
        let writer = open_writer(&path, synth_header(), None).unwrap();
        assert!(matches!(writer, AnyWriter::Sam(_)));
        writer.finish().unwrap();
    }

    #[test]
    fn open_writer_cram_without_cram_ref_errors() {
        let err = expect_err(open_writer(
            Path::new("nonexistent.cram"),
            synth_header(),
            None,
        ));
        assert!(matches!(err, BismarkIoError::MissingCramReference(_)));
    }

    #[test]
    fn open_writer_unsupported_extension_errors() {
        let err = expect_err(open_writer(Path::new("foo.txt"), synth_header(), None));
        assert!(matches!(err, BismarkIoError::UnsupportedKind(_)));
    }

    #[test]
    fn any_writer_records_dispatch_to_sam() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("out.sam");
        let mut writer = open_writer(&path, synth_header(), None).unwrap();
        let rec = synth_bismark_record();
        writer.write_record(&rec).unwrap();
        writer.finish().unwrap();

        // Verify SAM bytes are read-back-able.
        let mut reader = SamReader::from_path(&path).unwrap();
        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 1);
        assert!(records[0].as_ref().is_ok());
    }

    fn expect_err<T>(r: Result<T, BismarkIoError>) -> BismarkIoError {
        match r {
            Ok(_) => panic!("expected Err, got Ok"),
            Err(e) => e,
        }
    }
}

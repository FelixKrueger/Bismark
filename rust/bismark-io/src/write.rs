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
use noodles_sam::alignment::RecordBuf;
use noodles_sam::alignment::io::Write as SamAlignmentWrite;

use crate::cram_ref::build_fasta_repository;
use crate::error::BismarkIoError;
use crate::read::AlignmentKind;
use crate::record::BismarkRecord;

/// BAM writer producing BGZF-compressed BAM output.
///
/// **Must be finalised with `finish()` before being dropped.** Without
/// `finish()`, the BGZF EOF marker is written only via `Drop`, which
/// silently swallows I/O errors — corrupt output goes undetected.
#[must_use = "BamWriter must be finalised with finish() before being dropped; \
              otherwise EOF marker errors are silently lost via Drop"]
pub struct BamWriter<W: Write> {
    inner: noodles_bam::io::Writer<noodles_bgzf::io::Writer<W>>,
    header: Header,
}

impl BamWriter<BufWriter<File>> {
    /// Create a BAM writer at `path`. On header-write failure the
    /// partially-created file is removed (best-effort cleanup).
    pub fn from_path(path: &Path, header: Header) -> Result<Self, BismarkIoError> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        Self::new(writer, header).inspect_err(|_| {
            let _ = std::fs::remove_file(path);
        })
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

    /// Write a **raw** [`RecordBuf`] to the BAM stream, **bypassing** the
    /// [`BismarkRecord`] strand/`XR`/`XG`/`XM` validation.
    ///
    /// For records that are deliberately *not* Bismark-shaped — e.g. the
    /// aligner port's `--ambig_bam` output, which is the external aligner's own
    /// raw SAM line (carrying `AS:i`/`XS:i` tags, not `XM`/`XR`/`XG`). Such a
    /// record would be rejected by every `BismarkRecord` constructor, so it
    /// cannot go through [`Self::write_record`]. Added in v1.0.0-beta.9.
    pub fn write_raw_record(&mut self, record: &RecordBuf) -> Result<(), BismarkIoError> {
        self.inner.write_alignment_record(&self.header, record)?;
        Ok(())
    }

    /// Finalise the BAM stream by writing the BGZF EOF marker. Consumes
    /// `self`.
    ///
    /// Uses `noodles_bam::io::Writer::try_finish` directly rather than
    /// the trait-level `SamAlignmentWrite::finish` because the latter is
    /// a no-op in noodles-bam 0.89; the BGZF EOF marker would otherwise
    /// only be written via Drop, which silently swallows errors.
    pub fn finish(mut self) -> Result<(), BismarkIoError> {
        self.inner.try_finish()?;
        Ok(())
    }
}

/// **Threaded** BAM writer that uses [`noodles_bgzf::io::MultithreadedWriter`]
/// for parallel BGZF block compression.
///
/// Separate concrete type from [`BamWriter`] (which is generic over
/// `W: Write` and uses noodles' default single-threaded BGZF writer).
/// The threaded variant always wraps a `File` directly with a worker-thread
/// pool sized at construction time.
///
/// **Must be finalised with `finish()` before being dropped.** Same
/// contract as [`BamWriter`] — without `finish()`, the BGZF EOF marker
/// is written only via `Drop`, which silently swallows I/O errors.
///
/// BAM output byte-stream from this writer is **functionally identical**
/// to the single-threaded [`BamWriter`] — same records, same header,
/// valid BGZF EOF marker. Block boundaries may differ between the two
/// (different worker assignment patterns produce different block sizes),
/// but the decompressed record stream is byte-identical.
///
/// Added in `bismark-io` v1.0.0-beta.2 to support `bismark-dedup`'s
/// `--parallel N` flag.
#[must_use = "ThreadedBamWriter must be finalised with finish() before being dropped; \
              otherwise EOF marker errors are silently lost via Drop"]
pub struct ThreadedBamWriter {
    inner: noodles_bam::io::Writer<noodles_bgzf::io::MultithreadedWriter<File>>,
    header: Header,
}

impl ThreadedBamWriter {
    /// Create a BAM writer at `path` with `parallel` BGZF encoder worker
    /// threads. On header-write failure the partially-created file is
    /// removed (best-effort cleanup).
    ///
    /// `parallel` must be ≥ 1 (enforced by [`std::num::NonZero`]). For
    /// `parallel == 1`, prefer [`BamWriter::from_path`].
    pub fn from_path(
        path: &Path,
        header: Header,
        parallel: std::num::NonZero<usize>,
    ) -> Result<Self, BismarkIoError> {
        let file = File::create(path)?;
        let bgzf = noodles_bgzf::io::MultithreadedWriter::with_worker_count(parallel, file);
        let mut inner = noodles_bam::io::Writer::from(bgzf);
        inner.write_header(&header).inspect_err(|_| {
            let _ = std::fs::remove_file(path);
        })?;
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

    /// Finalise the BAM stream, writing the BGZF EOF marker. Consumes
    /// `self`.
    ///
    /// Internally invokes [`noodles_bgzf::io::MultithreadedWriter::finish`]
    /// on the inner BGZF writer (via `get_mut`). This is the parallel
    /// equivalent of [`BamWriter::finish`]'s `try_finish` call — both
    /// produce a valid BAM file ending in the canonical BGZF EOF marker.
    ///
    /// **Drop interaction**: after `finish()` returns, the
    /// `MultithreadedWriter`'s internal state transitions to `Done`.
    /// The subsequent `Drop` (when `self` falls out of scope) is a
    /// no-op for the BGZF layer — it does not attempt to re-write the
    /// EOF marker or re-flush pending blocks. This avoids the silent-
    /// double-finalise bug that would arise if Drop ran the same logic
    /// `finish()` already did.
    pub fn finish(mut self) -> Result<(), BismarkIoError> {
        // `noodles_bam::io::Writer::get_mut` exposes the inner BGZF writer.
        // For the MultithreadedWriter, `finish()` produces the EOF marker
        // + flushes pending blocks, returning the underlying File.
        let bgzf = self.inner.get_mut();
        bgzf.finish()?;
        Ok(())
    }
}

/// SAM writer producing uncompressed SAM text.
///
/// **Must be finalised with `finish()` before being dropped.** SAM has
/// no EOF marker, but `finish()` flushes the underlying buffer; drop-only
/// finalisation silently swallows buffer-flush errors (e.g. disk full).
#[must_use = "SamWriter must be finalised with finish() before being dropped; \
              otherwise buffer-flush errors are silently lost via Drop"]
pub struct SamWriter<W: Write> {
    inner: noodles_sam::io::Writer<W>,
    header: Header,
}

impl SamWriter<BufWriter<File>> {
    /// Create a SAM writer at `path`. On header-write failure the
    /// partially-created file is removed (best-effort cleanup).
    pub fn from_path(path: &Path, header: Header) -> Result<Self, BismarkIoError> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        Self::new(writer, header).inspect_err(|_| {
            let _ = std::fs::remove_file(path);
        })
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

    /// Finalise the SAM stream by flushing the wrapped writer. Consumes
    /// `self`.
    ///
    /// noodles-sam's `SamAlignmentWrite::finish` is a no-op in 0.85, so
    /// we flush the underlying writer directly to ensure buffered bytes
    /// reach the file. Without this, `BufWriter::Drop` swallows flush
    /// errors silently.
    pub fn finish(mut self) -> Result<(), BismarkIoError> {
        self.inner.get_mut().flush()?;
        Ok(())
    }
}

/// CRAM writer.
///
/// Requires the same reference FASTA as the reader; passed at
/// construction time. The FASTA must have a sibling `.fai` index in v1.0
/// (see [`crate::cram_ref::build_fasta_repository`]). Auto-fai-generation
/// is future work.
///
/// **Must be finalised with `finish()` before being dropped.** Records
/// are buffered in memory and written out at finalisation; dropping
/// without `finish()` silently loses all buffered records.
#[must_use = "CramWriter must be finalised with finish() before being dropped; \
              otherwise all buffered records are silently lost via Drop"]
pub struct CramWriter<W: Write> {
    inner: noodles_cram::io::Writer<W>,
    header: Header,
}

impl CramWriter<File> {
    /// Create a CRAM writer at `path`, using `cram_ref` as the reference
    /// FASTA. The FASTA must have a sibling `.fai` index.
    ///
    /// On header-write failure the partially-created CRAM file is
    /// removed (best-effort cleanup).
    pub fn from_path(path: &Path, header: Header, cram_ref: &Path) -> Result<Self, BismarkIoError> {
        let repo = build_fasta_repository(cram_ref)?;
        let mut inner = noodles_cram::io::writer::Builder::default()
            .set_reference_sequence_repository(repo)
            .build_from_path(path)?;
        match inner.write_header(&header) {
            Ok(()) => Ok(Self { inner, header }),
            Err(e) => {
                drop(inner);
                let _ = std::fs::remove_file(path);
                Err(BismarkIoError::Io(e))
            }
        }
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
/// write-side use and keeps the API symmetric with the reader. Unlike
/// the reader case, the writer side has no self-referential-iterator
/// lifetime issue; enum-dispatch here is purely for API symmetry and
/// to avoid `Box<dyn>` allocation per-record.
///
/// **Must be finalised with `finish()` before being dropped.** Without
/// finalisation, the underlying writer's EOF marker (BAM/CRAM) or buffer
/// flush (SAM) may be silently lost via Drop.
#[must_use = "AnyWriter must be finalised with finish() before being dropped; \
              otherwise the underlying writer's finalisation step is silently \
              dropped via Drop"]
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
    // Writers can't sniff bytes (the file doesn't exist yet), so they
    // dispatch on the file's extension via `from_extension`. Reader-side
    // dispatch (via `AlignmentKind::from_path`) is the magic-byte-sniff
    // path added in `bismark-io v1.0.0-beta.3`.
    match AlignmentKind::from_extension(path)? {
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
        // Quality scores: 5 bytes matching the sequence length. CRAM encodes
        // quality scores into the QualityScores external block (#28); a record
        // with no qualities causes the writer to skip the block, but the reader
        // then errors with "missing external block: 28" when iterating records.
        // Real Bismark BAMs always have qualities; synthetic records must
        // include them too.
        *record.quality_scores_mut() =
            noodles_sam::alignment::record_buf::QualityScores::from(vec![30u8; 5]);
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
    fn write_raw_record_bypasses_bismark_validation() {
        // A raw aligner-style record with NO XR/XG/XM (only AS:i) — every
        // BismarkRecord constructor rejects it, but write_raw_record writes it
        // verbatim (the aligner's --ambig_bam passthrough path).
        let mut raw = RecordBuf::default();
        *raw.name_mut() = Some(BString::from("amb1".as_bytes().to_vec()));
        *raw.flags_mut() = noodles_sam::alignment::record::Flags::from(0u16);
        *raw.reference_sequence_id_mut() = Some(0);
        *raw.alignment_start_mut() = Some(noodles_core::Position::try_from(10).unwrap());
        *raw.sequence_mut() = Sequence::from(b"ACGTC".to_vec());
        *raw.quality_scores_mut() =
            noodles_sam::alignment::record_buf::QualityScores::from(vec![30u8; 5]);
        raw.data_mut().insert(Tag::from(*b"AS"), Value::Int32(-6));

        // It is NOT a valid BismarkRecord (no XR/XG/XM) — write_record can't take it.
        assert!(BismarkRecord::from_noodles_record(raw.clone()).is_err());

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ambig.bam");
        let mut w = BamWriter::from_path(&path, synth_header()).unwrap();
        w.write_raw_record(&raw).unwrap();
        w.finish().unwrap();

        // Read back via RAW noodles (BamReader::records would filter/reject it).
        let file = std::fs::File::open(&path).unwrap();
        let mut reader = noodles_bam::io::Reader::new(std::io::BufReader::new(file));
        let hdr = reader.read_header().unwrap();
        let recs: Vec<RecordBuf> = reader
            .record_bufs(&hdr)
            .collect::<std::io::Result<_>>()
            .unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].name().map(|n| n.to_vec()), Some(b"amb1".to_vec()));
        let as_val = match recs[0].data().get(&Tag::from(*b"AS")).unwrap() {
            Value::Int8(n) => i64::from(*n),
            Value::Int16(n) => i64::from(*n),
            Value::Int32(n) => i64::from(*n),
            other => panic!("AS not an integer: {other:?}"),
        };
        assert_eq!(as_val, -6);
        assert!(recs[0].data().get(&Tag::from(*b"XR")).is_none());
    }

    #[test]
    fn sam_writer_roundtrip_via_cursor() {
        // Write a SAM with one record, then read it back; assert strand
        // classification survives the round-trip. Uses Cursor over a
        // borrowed Vec so we can recover the bytes AFTER finish().
        let header = synth_header();
        let rec = synth_bismark_record();

        let mut buf: Vec<u8> = Vec::new();
        {
            let cursor = Cursor::new(&mut buf);
            let mut writer = SamWriter::new(cursor, header).unwrap();
            writer.write_record(&rec).unwrap();
            writer.finish().unwrap();
        } // cursor drops, releasing the &mut buf borrow

        let mut reader = SamReader::new(Cursor::new(buf)).unwrap();
        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 1);
        let read_back = records.into_iter().next().unwrap().unwrap();
        assert_eq!(read_back.record_strand(), BismarkStrand::OT);
        assert_eq!(read_back.xm(), b".....");
    }

    /// BGZF EOF marker — a specific 28-byte sequence that valid BAM/BGZF
    /// streams must end with. Defined in the SAM/BAM spec §4.1.2.
    const BGZF_EOF_MARKER: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x06, 0x00, 0x42, 0x43, 0x02,
        0x00, 0x1b, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn bam_writer_finish_writes_bgzf_eof_marker() {
        // Directly verify that BamWriter::finish() puts the BGZF EOF
        // marker on disk. This is the test the previous round was missing.
        let tmp = TempDir::new().unwrap();
        let bam_path = tmp.path().join("eof_test.bam");
        {
            let mut writer = BamWriter::from_path(&bam_path, synth_header()).unwrap();
            writer.write_record(&synth_bismark_record()).unwrap();
            writer.finish().unwrap();
        }
        let bytes = std::fs::read(&bam_path).unwrap();
        assert!(
            bytes.len() >= BGZF_EOF_MARKER.len(),
            "BAM file shorter than the EOF marker"
        );
        let tail = &bytes[bytes.len() - BGZF_EOF_MARKER.len()..];
        assert_eq!(
            tail, BGZF_EOF_MARKER,
            "BAM file must end with the BGZF EOF marker"
        );
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

    /// Write a tiny FASTA + matching `.fai` index. chr1 = 100 bases:
    /// 9×N + "ACGTC" + 86×N. The "ACGTC" at positions 10-14 matches the
    /// synth_record's alignment_start=10 + seq=ACGTC so CRAM stores zero
    /// diffs against the reference.
    fn write_tiny_fasta_with_fai(dir: &Path) -> std::path::PathBuf {
        let fasta_path = dir.join("ref.fa");
        let fai_path = dir.join("ref.fa.fai");
        let mut seq = vec![b'N'; 9];
        seq.extend_from_slice(b"ACGTC");
        seq.extend(std::iter::repeat_n(b'N', 86));
        assert_eq!(seq.len(), 100);
        let mut fasta_content = b">chr1\n".to_vec();
        fasta_content.extend(&seq);
        fasta_content.push(b'\n');
        std::fs::write(&fasta_path, &fasta_content).unwrap();
        // .fai columns: name, length, offset, linebases, linewidth.
        // ">chr1\n" is 6 bytes; sequence starts at offset 6; 100 bases on
        // one line; line is 101 bytes (100 + newline).
        std::fs::write(&fai_path, "chr1\t100\t6\t100\t101\n").unwrap();
        fasta_path
    }

    #[test]
    fn cram_writer_produces_cram_file_with_magic_bytes() {
        // Soft check: CramWriter::from_path + write_record + finish
        // produces a file beginning with the CRAM file-definition magic
        // (`CRAM\x03\x00` for CRAM 3.0). The full write-then-read round-trip
        // is exercised by `cram_writer_roundtrip_via_tempfile` below.
        let tmp = TempDir::new().unwrap();
        let fasta_path = write_tiny_fasta_with_fai(tmp.path());
        let cram_path = tmp.path().join("test.cram");

        {
            let mut writer =
                CramWriter::from_path(&cram_path, synth_header(), &fasta_path).unwrap();
            writer.write_record(&synth_bismark_record()).unwrap();
            writer.finish().unwrap();
        }

        let bytes = std::fs::read(&cram_path).unwrap();
        // CRAM file definition starts with bytes `C R A M` followed by
        // a major.minor version.
        assert!(
            bytes.len() >= 4,
            "CRAM file too short to contain magic bytes"
        );
        assert_eq!(
            &bytes[..4],
            b"CRAM",
            "CRAM file must start with the magic bytes"
        );
    }

    #[test]
    fn cram_writer_roundtrip_via_tempfile() {
        // Full CRAM write → read round-trip. Requires the synthetic record
        // to include quality scores; without them, the QualityScores
        // external block (#28) is omitted by the writer and the reader
        // errors with "missing external block: 28" on iteration. Real
        // Bismark BAMs always have quality scores, so this only affects
        // hand-constructed test records.
        let tmp = TempDir::new().unwrap();
        let fasta_path = write_tiny_fasta_with_fai(tmp.path());
        let cram_path = tmp.path().join("test.cram");

        {
            let mut writer =
                CramWriter::from_path(&cram_path, synth_header(), &fasta_path).unwrap();
            writer.write_record(&synth_bismark_record()).unwrap();
            writer.finish().unwrap();
        }

        let mut reader = crate::read::CramReader::from_path(&cram_path, &fasta_path).unwrap();
        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 1, "CRAM round-trip should yield 1 record");
        let read_back = records.into_iter().next().unwrap().unwrap();

        // Semantic round-trip assertions: strand, methylation call string,
        // alignment position, qname, and sequence length survive the
        // round-trip. Byte-identical CRAM container is NOT a goal
        // (per DESIGN.md §Q3).
        assert_eq!(
            read_back.record_strand(),
            BismarkStrand::OT,
            "strand classification survives CRAM round-trip"
        );
        assert_eq!(read_back.xm(), b".....", "XM tag survives CRAM round-trip");
        assert_eq!(
            read_back.alignment_start(),
            Some(10),
            "alignment_start survives CRAM round-trip"
        );
        let qname_bytes: &[u8] = AsRef::as_ref(
            read_back
                .inner()
                .name()
                .expect("synthetic record has a name"),
        );
        assert_eq!(qname_bytes, b"read1", "qname survives CRAM round-trip");
        // Byte-equality on the sequence: stronger than length-only check.
        // CRAM reference-based compression decodes against the fixture
        // FASTA, which has "ACGTC" at positions 10..14 — so the round-trip
        // must recover those exact bytes.
        assert_eq!(
            read_back.inner().sequence().as_ref(),
            b"ACGTC",
            "sequence bytes survive CRAM reference-based round-trip"
        );
    }

    // ───────── ThreadedBamWriter tests (v1.0.0-beta.2) ─────────

    /// `ThreadedBamWriter::finish()` writes a valid BGZF EOF marker, just
    /// like the single-threaded `BamWriter::finish()`. Per B-H1 from the
    /// rev 2 plan-review: noodles' `MultithreadedWriter::finish()` returns
    /// a different type than `bgzf::Writer::try_finish()`, so the EOF-marker
    /// contract needs explicit verification.
    #[test]
    fn threaded_bam_writer_finish_writes_bgzf_eof_marker() {
        let tmp = TempDir::new().unwrap();
        let bam_path = tmp.path().join("eof_threaded.bam");
        {
            let mut writer = crate::write::ThreadedBamWriter::from_path(
                &bam_path,
                synth_header(),
                std::num::NonZero::new(4).unwrap(),
            )
            .unwrap();
            writer.write_record(&synth_bismark_record()).unwrap();
            writer.finish().unwrap();
        }
        let bytes = std::fs::read(&bam_path).unwrap();
        assert!(
            bytes.len() >= BGZF_EOF_MARKER.len(),
            "threaded BAM file shorter than the EOF marker"
        );
        let tail = &bytes[bytes.len() - BGZF_EOF_MARKER.len()..];
        assert_eq!(
            tail, BGZF_EOF_MARKER,
            "threaded BAM file must end with the BGZF EOF marker — \
             noodles' MultithreadedWriter::finish() must produce the same \
             EOF block bytes as the single-threaded writer"
        );
    }

    /// Round-trip: write via threaded writer, read via threaded reader,
    /// assert the record stream decodes identically. The BAM bytes on disk
    /// may differ between threaded and single-threaded writers (different
    /// BGZF block boundaries) but the decompressed record stream MUST be
    /// identical.
    #[test]
    fn threaded_bam_writer_roundtrip_via_tempfile() {
        let tmp = TempDir::new().unwrap();
        let bam_path = tmp.path().join("threaded_roundtrip.bam");
        {
            let mut writer = crate::write::ThreadedBamWriter::from_path(
                &bam_path,
                synth_header(),
                std::num::NonZero::new(4).unwrap(),
            )
            .unwrap();
            writer.write_record(&synth_bismark_record()).unwrap();
            writer.write_record(&synth_bismark_record()).unwrap();
            writer.write_record(&synth_bismark_record()).unwrap();
            writer.finish().unwrap();
        }

        // Read back via the threaded reader.
        let mut reader = crate::read::ThreadedBamReader::from_path(
            &bam_path,
            std::num::NonZero::new(4).unwrap(),
        )
        .unwrap();
        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 3, "wrote 3 records, expected 3 back");
        for r in records {
            let rec = r.unwrap();
            assert_eq!(rec.record_strand(), crate::strand::BismarkStrand::OT);
        }
    }

    /// Cross-writer / cross-reader matrix: writing via the threaded writer
    /// and reading via the single-threaded reader must work — and vice versa.
    /// Proves the BGZF byte-stream is canonical regardless of which writer
    /// produced it.
    #[test]
    fn threaded_bam_writer_output_readable_by_single_threaded_reader() {
        let tmp = TempDir::new().unwrap();
        let bam_path = tmp.path().join("threaded_to_single.bam");
        {
            let mut writer = crate::write::ThreadedBamWriter::from_path(
                &bam_path,
                synth_header(),
                std::num::NonZero::new(4).unwrap(),
            )
            .unwrap();
            writer.write_record(&synth_bismark_record()).unwrap();
            writer.finish().unwrap();
        }

        // Read via the single-threaded reader.
        let mut reader = crate::read::BamReader::from_path(&bam_path).unwrap();
        let records: Vec<_> = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(records.len(), 1);
    }
}

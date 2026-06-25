//! #1025 Phase 1 — integration tests for the uBAM → temp-FASTQ transcode driver.
//!
//! Builds real (tiny) unaligned BAM files with the noodles BAM writer, then checks
//! `transcode_ubam_to_fastq_se`/`_pe` produce the expected `samtools fastq`-equivalent
//! FASTQ — exercising the raw `record_bufs` read path the unit tests can't.

use std::io::Read;
use std::path::Path;

use noodles_sam as sam;
use sam::Header;
use sam::alignment::RecordBuf;
use sam::alignment::io::Write as _;
use sam::alignment::record_buf::{QualityScores, Sequence};

use bismark_aligner::ubam;

fn record(name: &str, flags: u16, seq: &[u8], quals: Vec<u8>) -> RecordBuf {
    let mut r = RecordBuf::default();
    *r.name_mut() = Some(name.as_bytes().into());
    *r.flags_mut() = flags.into();
    *r.sequence_mut() = Sequence::from(seq.to_vec());
    *r.quality_scores_mut() = QualityScores::from(quals);
    r
}

fn write_ubam(path: &Path, header: &Header, records: &[RecordBuf]) {
    let mut w = noodles_bam::io::Writer::new(std::fs::File::create(path).unwrap());
    w.write_header(header).unwrap();
    for rec in records {
        w.write_alignment_record(header, rec).unwrap();
    }
    w.finish(header).unwrap();
}

fn read_to_string(path: &Path) -> String {
    let mut s = String::new();
    std::fs::File::open(path)
        .unwrap()
        .read_to_string(&mut s)
        .unwrap();
    s
}

#[test]
fn se_ubam_transcodes_to_expected_fastq_and_names_stem() {
    let dir = tempfile::tempdir().unwrap();
    let bam = dir.path().join("sample.bam");
    let header = Header::default();
    write_ubam(
        &bam,
        &header,
        &[
            // qual [37;4] → +33 = 'F'
            record("r1", 0, b"ACGT", vec![37, 37, 37, 37]),
            // qual [2,4,6,8] → +33 = "#%')"
            record("r2", 0, b"AACG", vec![2, 4, 6, 8]),
            // secondary (0x100) — MUST be skipped (samtools fastq -F 0x900)
            record("sec", 0x100, b"GGGG", vec![20, 20, 20, 20]),
        ],
    );

    let out = ubam::transcode_ubam_to_fastq_se(&bam, dir.path()).unwrap();
    // R3: temp named `<stem>.fastq` so the downstream output stem == the
    // equivalent `samtools fastq > sample.fastq` run's stem.
    assert_eq!(out.file_name().unwrap().to_str().unwrap(), "sample.fastq");

    assert_eq!(
        read_to_string(&out),
        "@r1\nACGT\n+\nFFFF\n@r2\nAACG\n+\n#%')\n"
    );
}

#[test]
fn se_ubam_missing_quality_synthesizes_b() {
    let dir = tempfile::tempdir().unwrap();
    let bam = dir.path().join("noqual.bam");
    let header = Header::default();
    // empty quals → samtools-fastq placeholder 'B' × seq_len
    write_ubam(&bam, &header, &[record("nq", 0, b"ACGTN", vec![])]);
    let out = ubam::transcode_ubam_to_fastq_se(&bam, dir.path()).unwrap();
    assert_eq!(read_to_string(&out), "@nq\nACGTN\n+\nBBBBB\n");
}

#[test]
fn pe_ubam_splits_collated_mates_into_two_files() {
    let dir = tempfile::tempdir().unwrap();
    let bam = dir.path().join("pe.bam");
    let header = Header::default();
    // collated: read1 (0x1|0x40) then its mate read2 (0x1|0x80), same QNAME.
    write_ubam(
        &bam,
        &header,
        &[
            record("p1", 0x1 | 0x40, b"ACGT", vec![40, 40, 40, 40]), // +33 = 'I'
            record("p1", 0x1 | 0x80, b"TTGG", vec![40, 40, 40, 40]),
        ],
    );
    let (r1, r2) = ubam::transcode_ubam_to_fastq_pe(&bam, dir.path()).unwrap();
    assert_eq!(r1.file_name().unwrap().to_str().unwrap(), "pe_1.fastq");
    assert_eq!(r2.file_name().unwrap().to_str().unwrap(), "pe_2.fastq");
    assert_eq!(read_to_string(&r1), "@p1\nACGT\n+\nIIII\n");
    assert_eq!(read_to_string(&r2), "@p1\nTTGG\n+\nIIII\n");
}

#[test]
fn pe_ubam_desync_fails_loud() {
    let dir = tempfile::tempdir().unwrap();
    let bam = dir.path().join("desync.bam");
    let header = Header::default();
    // two read1s with different QNAMEs adjacent → not name-collated.
    write_ubam(
        &bam,
        &header,
        &[
            record("a", 0x1 | 0x40, b"ACGT", vec![30, 30, 30, 30]),
            record("b", 0x1 | 0x40, b"ACGT", vec![30, 30, 30, 30]),
        ],
    );
    let err = ubam::transcode_ubam_to_fastq_pe(&bam, dir.path()).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("not name-collated") || msg.contains("out of sync"),
        "unexpected error: {msg}"
    );
}

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

#[test]
fn is_bam_input_only_true_for_real_bam() {
    // The load-bearing R4 invariant: only an authentic BAM (magic, NOT extension)
    // is treated as uBAM, so the normal FASTQ/FASTA path is never broken.
    let dir = tempfile::tempdir().unwrap();

    let bam = dir.path().join("real.bam");
    write_ubam(
        &bam,
        &Header::default(),
        &[record("r", 0, b"ACGT", vec![30, 30, 30, 30])],
    );
    assert!(
        ubam::is_bam_input(&bam),
        "a real BAM must be detected as uBAM"
    );

    // Plain FASTQ (first byte '@') sniffs as SAM, NOT BAM.
    let fq = dir.path().join("reads.fastq");
    std::fs::write(&fq, b"@r1\nACGT\n+\nIIII\n").unwrap();
    assert!(!ubam::is_bam_input(&fq), "plain FASTQ must not be uBAM");

    // SAM TEXT mis-named `.ubam` (the gate-script bug) — magic, not extension.
    let sam = dir.path().join("reads.ubam");
    std::fs::write(&sam, b"@HD\tVN:1.6\n@SQ\tSN:c\tLN:9\n").unwrap();
    assert!(
        !ubam::is_bam_input(&sam),
        "SAM text must not be uBAM (detection is by BGZF+BAM magic, not extension)"
    );

    // gzip-magic, non-BAM payload (a `.fq.gz`-like file) → not uBAM, never panics.
    let gz = dir.path().join("reads.fq.gz");
    std::fs::write(&gz, [0x1f_u8, 0x8b, 0x08, 0x00, 0, 0, 0, 0, 0, 0]).unwrap();
    assert!(!ubam::is_bam_input(&gz), "gzip non-BAM must not be uBAM");

    // Missing file → false, never panics.
    assert!(!ubam::is_bam_input(&dir.path().join("nope.bam")));
}

#[test]
fn se_ubam_header_only_yields_empty_fastq() {
    // Validation #3: a header-only (zero-record) uBAM transcodes to an empty FASTQ,
    // which flows into the existing graceful-empty handling downstream.
    let dir = tempfile::tempdir().unwrap();
    let bam = dir.path().join("empty.bam");
    write_ubam(&bam, &Header::default(), &[]);
    let out = ubam::transcode_ubam_to_fastq_se(&bam, dir.path()).unwrap();
    assert_eq!(read_to_string(&out), "", "header-only uBAM → empty FASTQ");
}

#[test]
fn is_paired_classifies_from_first_primary_record() {
    let dir = tempfile::tempdir().unwrap();

    // Paired uBAM: first primary record carries 0x1.
    let pe = dir.path().join("pe.bam");
    write_ubam(
        &pe,
        &Header::default(),
        &[
            record("p", 0x1 | 0x40, b"ACGT", vec![30, 30, 30, 30]),
            record("p", 0x1 | 0x80, b"TTGG", vec![30, 30, 30, 30]),
        ],
    );
    assert!(ubam::is_paired(&pe).unwrap(), "0x1 records → paired");

    // Single-end uBAM: no 0x1.
    let se = dir.path().join("se.bam");
    write_ubam(
        &se,
        &Header::default(),
        &[record("s", 0, b"ACGT", vec![30, 30, 30, 30])],
    );
    assert!(!ubam::is_paired(&se).unwrap(), "no 0x1 → single-end");

    // A leading secondary (0x100) record is skipped; the first PRIMARY decides.
    let lead_sec = dir.path().join("lead_sec.bam");
    write_ubam(
        &lead_sec,
        &Header::default(),
        &[
            record("x", 0x100, b"GGGG", vec![20, 20, 20, 20]), // secondary, skipped
            record("x", 0, b"ACGT", vec![30, 30, 30, 30]),     // first primary: SE
        ],
    );
    assert!(
        !ubam::is_paired(&lead_sec).unwrap(),
        "secondary skipped → first primary (SE) decides"
    );

    // Header-only → false (→ SE path → empty → graceful).
    let empty = dir.path().join("empty.bam");
    write_ubam(&empty, &Header::default(), &[]);
    assert!(
        !ubam::is_paired(&empty).unwrap(),
        "header-only → not paired"
    );
}

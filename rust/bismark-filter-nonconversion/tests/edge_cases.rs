//! Edge-case + error-path integration tests for `filter_non_conversion_rs`.
//!
//! These cover paths that don't fit the uniform Perl-golden body comparison
//! in `byte_identity.rs` — they assert exit codes, partial output, report
//! sizes, and output record counts. Fixtures are built in-test via noodles
//! and outputs are read back via noodles (no samtools / no Perl needed), so
//! these run fully hermetically in CI.
//!
//! Reviewer-flagged items covered:
//! - C2: PE lone-trailing-R1 → die, prior pairs written (valid BAMs), 0-byte report.
//! - Empty `.bam` → die before any output (no output files).
//! - Unmapped mate in PE (no XM) → die (missing-XM path).
//! - `@PG`-absent + no `-s`/`-p` → CannotAutoDetectMode die.
//! - PE `@HD SO:coordinate` → reject before any output.
//! - Multi-file: run-time line only on the LAST file's report.
//! - Non-`bam` filename → "Please provide a BAM file" die.

use std::fs::File;
use std::num::NonZeroUsize;
use std::path::Path;

use assert_cmd::Command;
use bstr::BString;
use noodles_bam as bam;
use noodles_core::Position;
use noodles_sam::Header;
use noodles_sam::alignment::RecordBuf;
use noodles_sam::alignment::io::Write as _;
use noodles_sam::alignment::record::Flags;
use noodles_sam::alignment::record::cigar::Op;
use noodles_sam::alignment::record::cigar::op::Kind;
use noodles_sam::alignment::record::data::field::Tag;
use noodles_sam::alignment::record_buf::Cigar;
use noodles_sam::alignment::record_buf::QualityScores;
use noodles_sam::alignment::record_buf::Sequence;
use noodles_sam::alignment::record_buf::data::field::Value;
use noodles_sam::header::record::value::Map;
use noodles_sam::header::record::value::map::ReferenceSequence;
use tempfile::TempDir;

// ───────────────────────────── fixture helpers ─────────────────────────

fn header(coord_sorted: bool, bismark_pg: bool) -> Header {
    use noodles_sam::header::record::value::map::header::Version;
    let mut hd = Map::<noodles_sam::header::record::value::map::Header>::new(Version::new(1, 6));
    if coord_sorted {
        use noodles_sam::header::record::value::map::header::tag::SORT_ORDER;
        hd.other_fields_mut()
            .insert(SORT_ORDER, BString::from("coordinate"));
    } else {
        use noodles_sam::header::record::value::map::header::tag::SORT_ORDER;
        hd.other_fields_mut()
            .insert(SORT_ORDER, BString::from("unsorted"));
    }
    let mut builder = Header::builder().set_header(hd);
    builder = builder.add_reference_sequence(
        BString::from("chr1"),
        Map::<ReferenceSequence>::new(NonZeroUsize::try_from(10_000).unwrap()),
    );
    if bismark_pg {
        use noodles_sam::header::record::value::map::Program;
        use noodles_sam::header::record::value::map::program::tag::COMMAND_LINE;
        let mut prog = Map::<Program>::default();
        prog.other_fields_mut().insert(
            COMMAND_LINE,
            BString::from("bismark --genome /g -1 R1.fq.gz -2 R2.fq.gz"),
        );
        builder = builder.add_program(BString::from("Bismark"), prog);
    }
    builder.build()
}

fn mapped(qname: &str, flag: u16, xm: &str) -> RecordBuf {
    let len = xm.len();
    let mut r = RecordBuf::default();
    *r.name_mut() = Some(BString::from(qname));
    *r.flags_mut() = Flags::from(flag);
    *r.reference_sequence_id_mut() = Some(0);
    *r.alignment_start_mut() = Some(Position::try_from(100).unwrap());
    *r.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, len)]);
    *r.sequence_mut() = Sequence::from(vec![b'A'; len]);
    *r.quality_scores_mut() = QualityScores::from(vec![30u8; len]);
    r.data_mut()
        .insert(Tag::from(*b"XM"), Value::String(BString::from(xm)));
    r.data_mut()
        .insert(Tag::from(*b"XR"), Value::String(BString::from("CT")));
    r.data_mut()
        .insert(Tag::from(*b"XG"), Value::String(BString::from("CT")));
    r
}

/// Unmapped mate (no XM tag).
fn unmapped(qname: &str, flag: u16) -> RecordBuf {
    let mut r = RecordBuf::default();
    *r.name_mut() = Some(BString::from(qname));
    *r.flags_mut() = Flags::from(flag);
    *r.sequence_mut() = Sequence::from(b"AAAA".to_vec());
    *r.quality_scores_mut() = QualityScores::from(vec![30u8; 4]);
    r
}

fn write_bam(path: &Path, header: &Header, records: &[RecordBuf]) {
    let mut w = bam::io::Writer::new(File::create(path).unwrap());
    w.write_header(header).unwrap();
    for r in records {
        w.write_alignment_record(header, r).unwrap();
    }
    w.try_finish().unwrap();
}

fn count_records(path: &Path) -> usize {
    let mut reader = bam::io::Reader::new(std::io::BufReader::new(File::open(path).unwrap()));
    let hdr = reader.read_header().unwrap();
    reader.record_bufs(&hdr).count()
}

fn bin() -> Command {
    Command::cargo_bin("filter_non_conversion_rs").unwrap()
}

// ─────────────────────────────── tests ─────────────────────────────────

/// C2: PE input with an odd record count (lone trailing R1). Perl dies after
/// writing the complete preceding pairs, leaving a 0-byte report; the Rust
/// port must match (exit nonzero, valid partial BAMs, 0-byte report).
#[test]
fn pe_lone_trailing_r1_dies_with_partial_output_and_empty_report() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("odd.bam");
    let recs = vec![
        mapped("pairA", 99, ".........."),
        mapped("pairA", 147, ".........."),
        mapped("loneB", 99, ".........."), // no R2 → lone trailing R1
    ];
    write_bam(&input, &header(false, false), &recs);

    bin()
        .current_dir(tmp.path())
        .arg("-p")
        .arg("odd.bam")
        .assert()
        .failure();

    // Prior complete pair (2 records) flushed to the kept BAM as a valid file.
    let kept = tmp.path().join("odd.nonCG_filtered.bam");
    assert!(kept.exists(), "kept BAM should exist (partial output)");
    assert_eq!(
        count_records(&kept),
        2,
        "the one complete pair should be written"
    );

    // Report exists but is 0 bytes (Perl opens REPORT upfront, dies before SUMMARY).
    let report = tmp.path().join("odd.non-conversion_filtering.txt");
    assert!(report.exists(), "report file should exist");
    assert_eq!(
        std::fs::metadata(&report).unwrap().len(),
        0,
        "report must be 0 bytes on the lone-R1 die"
    );
}

/// Empty `.bam` (header only) → die before opening any output (Perl bam_isEmpty).
#[test]
fn empty_dotted_bam_dies_with_no_output_files() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("empty.bam");
    write_bam(&input, &header(false, false), &[]);

    bin()
        .current_dir(tmp.path())
        .arg("-s")
        .arg("empty.bam")
        .assert()
        .failure();

    assert!(
        !tmp.path().join("empty.nonCG_filtered.bam").exists(),
        "no kept BAM should be created for an empty .bam"
    );
    assert!(
        !tmp.path().join("empty.nonCG_removed_seqs.bam").exists(),
        "no removed BAM should be created for an empty .bam"
    );
    assert!(
        !tmp.path()
            .join("empty.non-conversion_filtering.txt")
            .exists(),
        "no report should be created for an empty .bam"
    );
}

/// PE pair whose R2 is unmapped (no XM) → missing-XM die (Perl line 195).
#[test]
fn pe_unmapped_mate_without_xm_dies() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("unmapped_mate.bam");
    let recs = vec![
        mapped("pairX", 99, "H.X......."),
        unmapped("pairX", 0x1 | 0x4 | 0x80), // paired, unmapped, read2; no XM
    ];
    write_bam(&input, &header(false, false), &recs);

    bin()
        .current_dir(tmp.path())
        .arg("-p")
        .arg("unmapped_mate.bam")
        .assert()
        .failure();
}

/// No `-s`/`-p` and no Bismark `@PG` → cannot auto-detect → die.
#[test]
fn no_mode_and_no_bismark_pg_dies() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("nopg.bam");
    write_bam(
        &input,
        &header(false, false),
        &[mapped("r", 0, "..........")],
    );

    bin()
        .current_dir(tmp.path())
        .arg("nopg.bam")
        .assert()
        .failure();
}

/// PE input declaring `@HD SO:coordinate` → rejected before any output.
#[test]
fn pe_coordinate_sorted_rejected_before_output() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sorted.bam");
    write_bam(
        &input,
        &header(true, false),
        &[
            mapped("p", 99, ".........."),
            mapped("p", 147, ".........."),
        ],
    );

    bin()
        .current_dir(tmp.path())
        .arg("-p")
        .arg("sorted.bam")
        .assert()
        .failure();

    assert!(
        !tmp.path().join("sorted.nonCG_filtered.bam").exists(),
        "coordinate-sorted PE must be rejected before opening writers"
    );
}

/// Non-`bam` filename → "Please provide a BAM file" die.
#[test]
fn non_bam_filename_rejected() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("reads.sam");
    write_bam(
        &input,
        &header(false, false),
        &[mapped("r", 0, "..........")],
    );

    bin()
        .current_dir(tmp.path())
        .arg("-s")
        .arg("reads.sam")
        .assert()
        .failure();
}

/// Multi-file: the run-time line is appended ONLY to the LAST file's report
/// (Perl reuses the REPORT handle; only the last stays open at exit).
#[test]
fn multifile_runtime_line_only_on_last_report() {
    let tmp = TempDir::new().unwrap();
    let f1 = tmp.path().join("a.bam");
    let f2 = tmp.path().join("b.bam");
    write_bam(&f1, &header(false, false), &[mapped("r1", 0, "HXH.......")]);
    write_bam(&f2, &header(false, false), &[mapped("r2", 0, "..........")]);

    bin()
        .current_dir(tmp.path())
        .arg("-s")
        .arg("a.bam")
        .arg("b.bam")
        .assert()
        .success();

    let r1 = std::fs::read_to_string(tmp.path().join("a.non-conversion_filtering.txt")).unwrap();
    let r2 = std::fs::read_to_string(tmp.path().join("b.non-conversion_filtering.txt")).unwrap();
    assert!(
        !r1.contains("filter_non_conversion completed in"),
        "file 1 report must NOT carry the run-time line, got: {r1:?}"
    );
    assert!(
        r2.contains("filter_non_conversion completed in"),
        "file 2 (last) report MUST carry the run-time line, got: {r2:?}"
    );
}

/// Sanity: SE with no XM tag on a read keeps it (verbatim) — the read is
/// routed to the kept BAM, not removed or dropped.
#[test]
fn se_missing_xm_is_kept() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("noxm.bam");
    let mut r = mapped("r", 0, "..........");
    r.data_mut().remove(&Tag::from(*b"XM")); // strip XM
    write_bam(&input, &header(false, false), &[r]);

    bin()
        .current_dir(tmp.path())
        .arg("-s")
        .arg("noxm.bam")
        .assert()
        .success();

    assert_eq!(
        count_records(&tmp.path().join("noxm.nonCG_filtered.bam")),
        1
    );
    assert_eq!(
        count_records(&tmp.path().join("noxm.nonCG_removed_seqs.bam")),
        0
    );
}

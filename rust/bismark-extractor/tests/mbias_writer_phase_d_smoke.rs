//! Phase D end-to-end smoke — runs the `bismark_methylation_extractor_rs`
//! binary on synthetic BAMs and asserts M-bias.txt content.
//!
//! Rev 1 design choice (Reviewer B I2): this file is NEW rather than
//! extending `tests/se_phase_b_smoke.rs` or `tests/pe_phase_c_smoke.rs`.
//! Those files belong to in-review PRs #849/#851; modifying them here
//! would create review-hygiene churn (base-PR diff grows, conflicts on
//! rebase). New file leaves the upstream PRs untouched.

use std::fs;
use std::num::NonZeroUsize;
use std::path::PathBuf;

use assert_cmd::Command;
use bismark_io::{BamWriter, BismarkRecord};
use bstr::BString;
use noodles_core::Position;
use noodles_sam::Header;
use noodles_sam::alignment::record::Flags;
use noodles_sam::alignment::record::cigar::Op;
use noodles_sam::alignment::record::cigar::op::Kind;
use noodles_sam::alignment::record::data::field::Tag;
use noodles_sam::alignment::record_buf::data::field::Value;
use noodles_sam::alignment::record_buf::{Cigar, RecordBuf, Sequence};
use noodles_sam::header::record::value::Map;
use noodles_sam::header::record::value::map::header::Version;
use noodles_sam::header::record::value::map::program::tag::COMMAND_LINE;
use noodles_sam::header::record::value::map::{Program, ReferenceSequence};

/// Build a SAM header with a Bismark `@PG` line so AutoDetect dispatches
/// to PE for PE smoke tests.
fn header_with_bismark_pe_pg() -> Header {
    let mut hd = Map::<noodles_sam::header::record::value::map::Header>::new(Version::new(1, 6));
    hd.other_fields_mut().insert(
        noodles_sam::header::record::value::map::header::tag::SORT_ORDER,
        BString::from(b"unsorted".to_vec()),
    );
    let mut prog = Map::<Program>::default();
    prog.other_fields_mut().insert(
        COMMAND_LINE,
        BString::from(b"bismark --genome /path/genome -1 R1.fq.gz -2 R2.fq.gz".to_vec()),
    );
    let mut header = Header::builder()
        .set_header(hd)
        .add_program(BString::from(b"Bismark".to_vec()), prog)
        .build();
    header.reference_sequences_mut().insert(
        BString::from(b"chr1".to_vec()),
        Map::<ReferenceSequence>::new(NonZeroUsize::new(10_000).unwrap()),
    );
    header
}

fn header_no_pg() -> Header {
    let mut header = Header::default();
    header.reference_sequences_mut().insert(
        BString::from(b"chr1".to_vec()),
        Map::<ReferenceSequence>::new(NonZeroUsize::new(10_000).unwrap()),
    );
    header
}

#[allow(clippy::too_many_arguments)]
fn synth_record(
    qname: &[u8],
    xr: &[u8],
    xg: &[u8],
    xm: &[u8],
    seq: &[u8],
    alignment_start: usize,
    flags: u16,
    refid: usize,
) -> BismarkRecord {
    let mut record = RecordBuf::default();
    *record.flags_mut() = Flags::from(flags);
    *record.sequence_mut() = Sequence::from(seq.to_vec());
    *record.alignment_start_mut() = Some(Position::try_from(alignment_start).unwrap());
    *record.reference_sequence_id_mut() = Some(refid);
    *record.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, xm.len())]);
    *record.name_mut() = Some(BString::from(qname.to_vec()));
    record
        .data_mut()
        .insert(Tag::from(*b"XR"), Value::String(BString::from(xr.to_vec())));
    record
        .data_mut()
        .insert(Tag::from(*b"XG"), Value::String(BString::from(xg.to_vec())));
    record
        .data_mut()
        .insert(Tag::from(*b"XM"), Value::String(BString::from(xm.to_vec())));
    BismarkRecord::from_noodles_record(record).unwrap()
}

#[test]
fn smoke_mbias_se_directional_produces_se_format_mbias_txt() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path: PathBuf = workdir.path().join("se_d.bam");

    // Build a simple SE directional BAM (3 OT records).
    let header = header_no_pg();
    let mut writer = BamWriter::from_path(&bam_path, header).unwrap();
    for (i, qname) in [b"r1".as_slice(), b"r2", b"r3"].iter().enumerate() {
        writer
            .write_record(&synth_record(
                qname,
                b"CT",
                b"CT",
                b"Z....",
                b"ACGTC",
                100 + i * 50,
                0, // SE: no PAIRED bit
                0,
            ))
            .unwrap();
    }
    writer.finish().unwrap();

    let output_dir = workdir.path().join("out");
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(&bam_path)
        .arg("--single-end")
        .arg("--output_dir")
        .arg(&output_dir)
        .assert()
        .success();

    // Filename: per derive_mbias_basename, "se_d.bam" → "se_d." + "M-bias.txt".
    let mbias_path = output_dir.join("se_d.M-bias.txt");
    assert!(
        mbias_path.exists(),
        "M-bias.txt missing at {:?}",
        mbias_path
    );

    let content = fs::read_to_string(&mbias_path).unwrap();

    // SE format: 3 sections with 11-equals rule, no (R1)/(R2) markers.
    assert!(content.contains("CpG context\n===========\n"));
    assert!(content.contains("CHG context\n===========\n"));
    assert!(content.contains("CHH context\n===========\n"));
    assert!(
        !content.contains("(R1)"),
        "SE output should not contain (R1)"
    );
    assert!(
        !content.contains("(R2)"),
        "SE output should not contain (R2)"
    );
    // Column header byte-exact.
    assert!(
        content
            .contains("position\tcount methylated\tcount unmethylated\t% methylation\tcoverage\n")
    );
    // At least one position row should appear (R1 records each emit a Z call at read_pos 0 → position 1).
    assert!(
        content.contains("\n1\t"),
        "expected at least one row at position 1; got:\n{content}"
    );
}

#[test]
fn smoke_mbias_pe_auto_detect_produces_pe_format_mbias_txt() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path: PathBuf = workdir.path().join("pe_d.bam");

    // PE BAM with Bismark @PG line → AutoDetect → extract_pe.
    let header = header_with_bismark_pe_pg();
    let mut writer = BamWriter::from_path(&bam_path, header).unwrap();
    // One OT pair: R1 OT, R2 CTOT.
    writer
        .write_record(&synth_record(
            b"pair1", b"CT", b"CT", b"Z....", b"ACGTC", 100, 0x41, 0,
        ))
        .unwrap();
    writer
        .write_record(&synth_record(
            b"pair1", b"GA", b"CT", b"....z", b"ACGTC", 100, 0x81, 0,
        ))
        .unwrap();
    writer.finish().unwrap();

    let output_dir = workdir.path().join("out");
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    // No --paired-end / --single-end — let AutoDetect dispatch.
    cmd.arg(&bam_path)
        .arg("--output_dir")
        .arg(&output_dir)
        .assert()
        .success();

    let mbias_path = output_dir.join("pe_d.M-bias.txt");
    assert!(
        mbias_path.exists(),
        "M-bias.txt missing at {:?}",
        mbias_path
    );

    let content = fs::read_to_string(&mbias_path).unwrap();

    // PE format: 6 sections with 16-equals rule, both (R1) and (R2) markers.
    assert!(
        content.contains("CpG context (R1)\n================\n"),
        "missing R1 CpG header"
    );
    assert!(
        content.contains("CHG context (R1)\n================\n"),
        "missing R1 CHG header"
    );
    assert!(
        content.contains("CHH context (R1)\n================\n"),
        "missing R1 CHH header"
    );
    assert!(
        content.contains("CpG context (R2)\n================\n"),
        "missing R2 CpG header"
    );
    assert!(
        content.contains("CHG context (R2)\n================\n"),
        "missing R2 CHG header"
    );
    assert!(
        content.contains("CHH context (R2)\n================\n"),
        "missing R2 CHH header"
    );
    // No SE-style headers should appear.
    assert!(
        !content.contains("CpG context\n===========\n"),
        "PE output should not contain SE-style CpG header"
    );
}

#[test]
fn smoke_mbias_txt_absent_with_mbias_off() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path: PathBuf = workdir.path().join("se_off.bam");
    let header = header_no_pg();
    let mut writer = BamWriter::from_path(&bam_path, header).unwrap();
    writer
        .write_record(&synth_record(
            b"r1", b"CT", b"CT", b"Z....", b"ACGTC", 100, 0, 0,
        ))
        .unwrap();
    writer.finish().unwrap();

    let output_dir = workdir.path().join("out");
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(&bam_path)
        .arg("--single-end")
        .arg("--mbias_off")
        .arg("--output_dir")
        .arg(&output_dir)
        .assert()
        .success();

    let mbias_path = output_dir.join("se_off.M-bias.txt");
    assert!(
        !mbias_path.exists(),
        "M-bias.txt should NOT exist with --mbias_off; found at {:?}",
        mbias_path
    );

    // Splitting report + split files still exist (Phase B/C behaviour unchanged).
    let report_path = output_dir.join("se_off_splitting_report.txt");
    assert!(
        report_path.exists(),
        "splitting report should still exist with --mbias_off"
    );
    let cpg_ot = output_dir.join("CpG_OT_se_off.txt");
    assert!(cpg_ot.exists(), "split files should still exist");
}

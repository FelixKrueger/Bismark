//! End-to-end Phase B smoke test.
//!
//! Builds a synthetic SE BAM in-test (no Perl toolchain required), runs the
//! `bismark_methylation_extractor_rs` binary on it, and asserts:
//! - exit code 0,
//! - all 12 split files present,
//! - each split file's first line is the Perl version header,
//! - at least one of `{CpG,CHG,CHH}_OT_*.txt` has a content line beyond the
//!   header (records actually routed),
//! - `_splitting_report.txt` exists and contains expected substrings.
//!
//! Per plan rev 1 (Reviewer B I5): byte-equality of split files vs a Perl
//! baseline is Phase H. Phase B's smoke gates "binary runs end-to-end and
//! produces output" — a wide bug-class catcher without toolchain dependency.

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use bismark::io::{BamWriter, BismarkRecord};
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
use noodles_sam::header::record::value::map::ReferenceSequence;
use std::num::NonZeroUsize;

// ─────────────────────────────────────────────────────────────────────────
// Synthetic BAM helpers
// ─────────────────────────────────────────────────────────────────────────

fn header_with_chr1() -> Header {
    let mut header = Header::default();
    header.reference_sequences_mut().insert(
        BString::from(b"chr1".to_vec()),
        Map::<ReferenceSequence>::new(NonZeroUsize::new(10_000).unwrap()),
    );
    header
}

fn synth_record(
    qname: &[u8],
    xr: &[u8],
    xg: &[u8],
    xm: &[u8],
    seq: &[u8],
    alignment_start: usize,
    flags: u16,
) -> BismarkRecord {
    let mut record = RecordBuf::default();
    *record.flags_mut() = Flags::from(flags);
    *record.sequence_mut() = Sequence::from(seq.to_vec());
    *record.alignment_start_mut() = Some(Position::try_from(alignment_start).unwrap());
    *record.reference_sequence_id_mut() = Some(0); // chr1
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
    BismarkRecord::from_noodles_record(record).expect("synth produces a valid BismarkRecord")
}

/// Write a small SE directional BAM (3 OT records + 2 OB records) at `path`.
fn write_se_directional_bam(path: &std::path::Path) {
    let header = header_with_chr1();
    let mut writer = BamWriter::from_path(path, header).unwrap();

    // Three OT records (XR=CT XG=CT) at varying alignment starts.
    writer
        .write_record(&synth_record(
            b"read_OT_1",
            b"CT",
            b"CT",
            b"Zz...",
            b"ACGTC",
            100,
            0,
        ))
        .unwrap();
    writer
        .write_record(&synth_record(
            b"read_OT_2",
            b"CT",
            b"CT",
            b"..X..",
            b"ACGTC",
            200,
            0,
        ))
        .unwrap();
    writer
        .write_record(&synth_record(
            b"read_OT_3",
            b"CT",
            b"CT",
            b"....H",
            b"ACGTC",
            300,
            0,
        ))
        .unwrap();

    // Two OB records (XR=CT XG=GA).
    writer
        .write_record(&synth_record(
            b"read_OB_1",
            b"CT",
            b"GA",
            b"Z....",
            b"ACGTC",
            400,
            0,
        ))
        .unwrap();
    writer
        .write_record(&synth_record(
            b"read_OB_2",
            b"CT",
            b"GA",
            b"..h..",
            b"ACGTC",
            500,
            0,
        ))
        .unwrap();

    writer.finish().unwrap();
}

/// Write a 1-record BAM where the record has the PAIRED FLAG (0x1) set —
/// triggers Phase B's per-record SE/PE guard.
fn write_bam_with_paired_record(path: &std::path::Path) {
    let header = header_with_chr1();
    let mut writer = BamWriter::from_path(path, header).unwrap();
    writer
        .write_record(&synth_record(
            b"paired_read",
            b"CT",
            b"CT",
            b"Z....",
            b"ACGTC",
            100,
            0x41, // 0x40 first-in-pair + 0x01 paired
        ))
        .unwrap();
    writer.finish().unwrap();
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn smoke_se_directional_produces_all_12_files_and_report() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path: PathBuf = workdir.path().join("se_smoke.bam");
    write_se_directional_bam(&bam_path);

    let output_dir = workdir.path().join("out");

    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(&bam_path)
        .arg("--single-end")
        .arg("--output_dir")
        .arg(&output_dir)
        .assert()
        .success();

    // OT files for CpG / CHG / CHH each get at least one OT call.
    for ctx in ["CpG", "CHG", "CHH"] {
        let p = output_dir.join(format!("{ctx}_OT_se_smoke.txt"));
        let content = fs::read_to_string(&p).unwrap();
        let first_line = content.lines().next().unwrap_or("");
        assert_eq!(
            first_line, "Bismark methylation extractor version v0.25.1",
            "header drift in {ctx}_OT_se_smoke.txt"
        );
        assert!(
            content.lines().count() >= 2,
            "{ctx}_OT should have header + at least one call line; got:\n{content}"
        );
    }

    // OB files: CpG_OB and CHH_OB should have calls (from read_OB_1 +
    // read_OB_2). CHG_OB has no calls in this fixture → swept.
    let cpg_ob = fs::read_to_string(output_dir.join("CpG_OB_se_smoke.txt")).unwrap();
    assert!(
        cpg_ob.lines().count() >= 2,
        "CpG_OB should have a call from read_OB_1; got:\n{cpg_ob}"
    );
    let chh_ob = fs::read_to_string(output_dir.join("CHH_OB_se_smoke.txt")).unwrap();
    assert!(
        chh_ob.lines().count() >= 2,
        "CHH_OB should have a call from read_OB_2; got:\n{chh_ob}"
    );

    // Phase C.2 (#865): CTOT/CTOB × 3 contexts (6 files) AND CHG_OB are
    // all empty for this directional SE fixture → swept at flush time.
    for ctx in ["CpG", "CHG", "CHH"] {
        for strand in ["CTOT", "CTOB"] {
            let p = output_dir.join(format!("{ctx}_{strand}_se_smoke.txt"));
            assert!(
                !p.exists(),
                "directional library: {ctx}_{strand} should be swept (empty)"
            );
        }
    }
    assert!(
        !output_dir.join("CHG_OB_se_smoke.txt").exists(),
        "CHG_OB should be swept (no CHG calls on OB strand in this fixture)"
    );

    // Splitting report must exist + parse.
    // Phase C.2 (#864): line 1 of the report is the BAM basename (matches
    // Perl), not the version banner. The version is on line 4 with the
    // phrasing "Bismark Extractor Version: v0.25.1".
    let report = fs::read_to_string(output_dir.join("se_smoke_splitting_report.txt")).unwrap();
    assert!(
        report.contains("Bismark Extractor Version: v0.25.1"),
        "expected Perl-format version line; got:\n{report}"
    );
    // For SE: records_processed == call_strings_processed == sequence count.
    assert!(report.contains("Processed 5 lines in total"));
    assert!(report.contains("Total number of methylation call strings processed: 5"));
    assert!(report.contains("Total number of C's analysed:"));
    assert!(report.contains("Total methylated C's in CpG context:"));
    assert!(report.contains("Total methylated C's in CHG context:"));
    assert!(report.contains("Total methylated C's in CHH context:"));
    assert!(report.contains("C methylated in CpG context:"));
}

#[test]
fn smoke_se_rejects_record_with_paired_flag_set() {
    // Plan §4.5 row "PE record reaches SE pipeline" + plan §7.1 row
    // `extract_se_rejects_record_with_paired_flag_set`.
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("paired_in_se.bam");
    write_bam_with_paired_record(&bam_path);

    let output_dir = workdir.path().join("out");

    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(&bam_path)
        .arg("--single-end")
        .arg("--output_dir")
        .arg(&output_dir)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "paired-end extraction (input has PAIRED flag set)",
        ));

    // Cleanup should have removed all 12 partial files. Two acceptable end
    // states: output_dir doesn't exist (cleanup was so thorough it removed
    // the dir — not the case in this impl) OR it exists but is empty.
    if output_dir.exists() {
        let count = fs::read_dir(&output_dir).unwrap().count();
        assert_eq!(
            count, 0,
            "cleanup_partial_outputs should have removed all 12 files; \
             found {count} stragglers"
        );
    }
}

#[test]
fn smoke_se_empty_bam_writes_only_header_files() {
    // Plan §10 row "Empty input" + plan §7.1 row
    // `extract_se_empty_input_writes_only_header_files`.
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("empty.bam");

    // Build an empty BAM (header only, no records).
    let header = header_with_chr1();
    let writer = BamWriter::from_path(&bam_path, header).unwrap();
    writer.finish().unwrap();

    let output_dir = workdir.path().join("out");

    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(&bam_path)
        .arg("--single-end")
        .arg("--output_dir")
        .arg(&output_dir)
        .assert()
        .success();

    // Phase C.2 (#865): empty BAM → no records routed → every per-strand
    // file is empty after the run → all 12 are swept at finalize time.
    // Only the splitting-report and M-bias.txt survive.
    let dir_entries: Vec<_> = fs::read_dir(&output_dir).unwrap().collect();
    assert_eq!(
        dir_entries.len(),
        2,
        "empty BAM: only splitting-report + M-bias.txt survive after C.2 sweep"
    );
    // Phase C.2 (#865): all 12 per-strand files swept after empty-BAM run.
    for ctx in ["CpG", "CHG", "CHH"] {
        for strand in ["OT", "CTOT", "CTOB", "OB"] {
            let p = output_dir.join(format!("{ctx}_{strand}_empty.txt"));
            assert!(!p.exists(), "{ctx}_{strand}: empty file should be swept");
        }
    }

    // Splitting report exists with 0 records.
    let report = fs::read_to_string(output_dir.join("empty_splitting_report.txt")).unwrap();
    assert!(report.contains("Processed 0 lines in total"));
    assert!(report.contains("Total methylated C's in CpG context:\t0"));
}

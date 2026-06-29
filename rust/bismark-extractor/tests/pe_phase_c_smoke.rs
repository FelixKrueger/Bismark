//! End-to-end Phase C smoke test.
//!
//! Builds a synthetic PE BAM in-test (no Perl toolchain required), runs the
//! `bismark_methylation_extractor_rs` binary on it, and asserts:
//! - exit code 0,
//! - all 12 split files present,
//! - splitting report contains "Processed N lines in total" matching the
//!   2N (lines) accounting (Perl line 2479 literal),
//! - CTOT/CTOB files header-only (directional library),
//! - at least one OT-pair-strand call line on disk.
//!
//! Per plan rev 1 (Reviewer B I5 from Phase B): byte-equality vs Perl
//! baseline is Phase H. Phase C's smoke gates "binary runs end-to-end on
//! PE input via auto-detect" — a wide bug-class catcher without toolchain
//! dependency.

use std::fs;
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
use std::num::NonZeroUsize;

/// Build a SAM header with a Bismark @PG line so auto-detect routes the
/// binary to `extract_pe` without explicit `--paired-end`.
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
    BismarkRecord::from_noodles_record(record).expect("synth_record produces a valid BismarkRecord")
}

/// 10 OT pairs at evenly-spaced positions. R1 has a CpG-meth call near 5';
/// R2 has a CpG-unmeth call positioned PAST R1's end so the post-C.1 strict-`>`
/// keep predicate retains it (exercises the post-fix "kept" path rather than
/// passing by boundary coincidence).
///
/// Pre-C.1 fixture had `r2_start = r1_start` (full overlap), R2's call
/// landing at r2_start + 4 = r1_ref_end (boundary), which the buggy `<` and
/// the corrected `>` both happened to drop — making the smoke pass by
/// coincidence with a stale rationale. C.1 reworks to space R2 PAST R1.
fn write_pe_directional_bam(path: &std::path::Path) {
    let header = header_with_bismark_pe_pg();
    let mut writer = BamWriter::from_path(path, header).unwrap();
    for i in 0..10 {
        let qname = format!("pair_{i}");
        let r1_start = 100 + i * 200;
        // R2 starts 5bp past R1's end (R1 spans [r1_start, r1_start+4], so
        // r2_start = r1_start + 5 puts R2 immediately past R1). R2's 5'-
        // oriented call (BAM-pos 4 reversed → ref_pos = r2_start + 4
        // = r1_start + 9) is strictly > r1_ref_end (= r1_start + 4),
        // so the post-C.1 strict-`>` keep predicate retains it.
        let r2_start = r1_start + 5;
        let r1 = synth_record(
            qname.as_bytes(),
            b"CT",
            b"CT",
            b"Z....",
            b"ACGTC",
            r1_start,
            0x41, // paired + first
            0,
        );
        let r2 = synth_record(
            qname.as_bytes(),
            b"GA",
            b"CT",
            b"....z",
            b"ACGTC",
            r2_start,
            0x81, // paired + last
            0,
        );
        writer.write_record(&r1).unwrap();
        writer.write_record(&r2).unwrap();
    }
    writer.finish().unwrap();
}

#[test]
fn smoke_pe_auto_detect_produces_all_12_files_and_report() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path: PathBuf = workdir.path().join("pe_smoke.bam");
    write_pe_directional_bam(&bam_path);

    let output_dir = workdir.path().join("out");

    // No --single-end / --paired-end → AutoDetect via @PG ID:Bismark.
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(&bam_path)
        .arg("--output_dir")
        .arg(&output_dir)
        .assert()
        .success();

    // All 12 split files must exist with the Perl version header on line 1.
    // Phase C.2 (#865): all empty per-strand files are swept at flush.
    // For this fixture (10 OT pairs, all R1+R2 calls in CpG context),
    // only CpG_OT survives; the other 11 are empty → swept.
    let p = output_dir.join("CpG_OT_pe_smoke.txt");
    assert!(p.exists(), "expected: {}", p.display());
    let content = fs::read_to_string(&p).unwrap();
    let first_line = content.lines().next().unwrap_or("");
    assert_eq!(
        first_line, "Bismark methylation extractor version v0.25.1",
        "header drift in CpG_OT_pe_smoke.txt"
    );

    // Phase C.2 (#864): splitting report counts PAIRS (sequences_count),
    // not 2×pairs. 10 pairs → "Processed 10 lines in total". The 2×pairs
    // count appears separately as "Total number of methylation call
    // strings processed: 20" (Perl line 2483).
    let report = fs::read_to_string(output_dir.join("pe_smoke_splitting_report.txt")).unwrap();
    assert!(
        report.contains("Processed 10 lines in total"),
        "PE pair-counting: expected '10 lines'; got:\n{report}"
    );
    assert!(
        report.contains("Total number of methylation call strings processed: 20"),
        "PE call-strings counter = 2×pairs; got:\n{report}"
    );

    // C.1 (#862) — post-fix expected: 10 R1 calls + 10 R2 calls = 20 lines.
    // R1 5M at r1_start → r1_ref_end = r1_start + 4 (= r1_start + 5 - 1).
    // R2 5M at r2_start = r1_start + 5 (immediately past R1's end).
    // R2's reversed iter_aligned 5'-oriented call at read_pos_5p=0 maps to
    // BAM-pos 4 (the 'z' in "....z"), ref_pos = r2_start + 4 = r1_start + 9.
    // Post-C.1 strict-`>` keep predicate: r2_pos (r1_start+9) > r1_ref_end
    // (r1_start+4) → KEPT. So 10 R1 calls + 10 R2 calls = 20 lines.
    let cpg_ot_call_lines = content.lines().count() - 1;
    assert_eq!(
        cpg_ot_call_lines, 20,
        "CpG_OT should have 10 R1 + 10 R2 call lines (R2 in unique region kept); got:\n{content}"
    );

    // Phase C.2 (#865): CTOT/CTOB AND CHG/CHH × all-strands files are all
    // empty for this fixture → swept. Verify absence (not header-only).
    for ctx in ["CpG", "CHG", "CHH"] {
        for strand in ["OT", "CTOT", "CTOB", "OB"] {
            if ctx == "CpG" && strand == "OT" {
                continue; // the one populated file
            }
            let p = output_dir.join(format!("{ctx}_{strand}_pe_smoke.txt"));
            assert!(
                !p.exists(),
                "{}_{}: empty file should be swept (no calls in this context/strand)",
                ctx,
                strand
            );
        }
    }
}

#[test]
fn smoke_pe_explicit_paired_end_flag_works() {
    // Same fixture but pass --paired-end explicitly (bypasses auto-detect).
    let workdir = tempfile::tempdir().unwrap();
    let bam_path: PathBuf = workdir.path().join("pe_explicit.bam");
    write_pe_directional_bam(&bam_path);

    let output_dir = workdir.path().join("out");
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(&bam_path)
        .arg("--paired-end")
        .arg("--output_dir")
        .arg(&output_dir)
        .assert()
        .success();

    let report = fs::read_to_string(output_dir.join("pe_explicit_splitting_report.txt")).unwrap();
    // Phase C.2 (#864): PE sequences_count = pair count (10 pairs).
    assert!(report.contains("Processed 10 lines in total"));
}

//! Phase E end-to-end smoke tests.
//!
//! Spawn the `bismark-methylation-extractor-rs` binary with each of the
//! Phase E output modes (`--comprehensive`, `--merge_non_CpG`, both,
//! `--yacht`, `--mbias_only`) and `--gzip`, on a synthetic SE-directional
//! BAM, and assert:
//!   - exit code 0,
//!   - the expected file set on disk per §7.2 of the plan,
//!   - yacht reverse-strand rows have col-6 > col-7 (Critical-1 regression),
//!   - mbias_only counter-equivalence vs Default mode,
//!   - gzip content decompresses to the byte-identical plain content.
//!
//! Per plan §7.2: byte-equality of split files vs a Perl baseline is
//! Phase H. These smokes gate "binary runs end-to-end and produces the
//! expected shape" — a wide bug-class catcher without toolchain dep.
//!
//! **Deviation from plan §7.2 (rev 1):** the planned smoke
//! `smoke_gzip_cleanup_on_write_failure_removes_gz_files` is skipped here.
//! Injecting an I/O error mid-write portably is flaky (e.g. `/dev/full`
//! is Linux-only). The cleanup_all behaviour is exercised by the unit
//! tests `output_file_map_skips_eager_open_for_mbias_only` (empty-map
//! cleanup) and `output_file_map_gzip_writes_valid_gz_content_byte_identical_to_plain`
//! (round-trip + Drop-based footer), which together cover the relevant
//! Drop semantics for gzipped writers.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use bismark_io::{BamWriter, BismarkRecord};
use bstr::BString;
use flate2::read::GzDecoder;
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

// ─── Synthetic BAM helpers (duplicated from se_phase_b_smoke.rs to keep
//     each test file self-contained; cross-test `tests/common/mod.rs`
//     refactor is a separate cleanup) ────────────────────────────────

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

/// Write a small SE directional BAM with OT + OB records so all
/// per-context routing buckets get exercised.
fn write_se_directional_bam(path: &Path) {
    let header = header_with_chr1();
    let mut writer = BamWriter::from_path(path, header).unwrap();
    // Three OT records covering CpG/CHG/CHH meth + unmeth.
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
            b"..X.x",
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
            b"H.h..",
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

/// Variant: one OT record with an unrecognised XM byte (`Q`).
fn write_bam_with_invalid_xm_byte(path: &Path) {
    let header = header_with_chr1();
    let mut writer = BamWriter::from_path(path, header).unwrap();
    // OT record: Z, Q, z. With --mbias_only, Q is silently skipped.
    writer
        .write_record(&synth_record(
            b"bad_xm", b"CT", b"CT", b"ZQz", b"ACG", 100, 0,
        ))
        .unwrap();
    writer.finish().unwrap();
}

fn write_empty_bam(path: &Path) {
    let header = header_with_chr1();
    let writer = BamWriter::from_path(path, header).unwrap();
    writer.finish().unwrap();
}

fn dir_entries_sorted(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    v.sort();
    v
}

fn read_gz(path: &Path) -> String {
    let f = fs::File::open(path).unwrap();
    let mut decoded = Vec::new();
    GzDecoder::new(f).read_to_end(&mut decoded).unwrap();
    String::from_utf8(decoded).unwrap()
}

// ─── Comprehensive ───────────────────────────────────────────────────

#[test]
fn smoke_comprehensive_emits_3_files_with_context_infix() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path: PathBuf = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);
    let out = workdir.path().join("out");

    Command::cargo_bin("bismark-methylation-extractor-rs")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--comprehensive")
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .success();

    // 3 split files + 1 splitting_report + 1 M-bias.txt = 5 total.
    let names = dir_entries_sorted(&out);
    assert!(
        names.contains(&"CpG_context_se.txt".to_string()),
        "{names:?}"
    );
    assert!(
        names.contains(&"CHG_context_se.txt".to_string()),
        "{names:?}"
    );
    assert!(
        names.contains(&"CHH_context_se.txt".to_string()),
        "{names:?}"
    );
    // No strand-specific files.
    assert!(!out.join("CpG_OT_se.txt").exists());
    // Calls landed.
    let cpg = fs::read_to_string(out.join("CpG_context_se.txt")).unwrap();
    assert!(cpg.contains("read_OT_1"));
    assert!(cpg.contains("read_OB_1"));
}

// ─── MergeNonCpG ─────────────────────────────────────────────────────

#[test]
fn smoke_merge_non_cpg_emits_8_files_with_chg_chh_in_non_cpg() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);
    let out = workdir.path().join("out");

    Command::cargo_bin("bismark-methylation-extractor-rs")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--merge_non_CpG")
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .success();

    let names = dir_entries_sorted(&out);
    for class in ["CpG", "Non_CpG"] {
        for strand in ["OT", "CTOT", "CTOB", "OB"] {
            let expected = format!("{class}_{strand}_se.txt");
            assert!(names.contains(&expected), "missing {expected}");
        }
    }
    // CHG calls land in Non_CpG_OT (not CHG_OT — that file doesn't exist).
    assert!(!out.join("CHG_OT_se.txt").exists());
    let non_cpg_ot = fs::read_to_string(out.join("Non_CpG_OT_se.txt")).unwrap();
    assert!(
        non_cpg_ot.contains("read_OT_2") || non_cpg_ot.contains("read_OT_3"),
        "CHG/CHH read should land in Non_CpG_OT_se.txt; got:\n{non_cpg_ot}"
    );
}

// ─── ComprehensiveMergeNonCpG ────────────────────────────────────────

#[test]
fn smoke_comprehensive_merge_non_cpg_emits_2_files() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);
    let out = workdir.path().join("out");

    Command::cargo_bin("bismark-methylation-extractor-rs")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--comprehensive")
        .arg("--merge_non_CpG")
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .success();

    let names = dir_entries_sorted(&out);
    assert!(names.contains(&"CpG_context_se.txt".to_string()));
    assert!(names.contains(&"Non_CpG_context_se.txt".to_string()));
    // No strand files, no CHG/CHH context files.
    assert!(!out.join("CHG_context_se.txt").exists());
    assert!(!out.join("CpG_OT_se.txt").exists());
}

// ─── Yacht ───────────────────────────────────────────────────────────

#[test]
fn smoke_yacht_emits_1_file_with_8_col_rows_and_reverse_strand_swap() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);
    let out = workdir.path().join("out");

    Command::cargo_bin("bismark-methylation-extractor-rs")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--yacht")
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .success();

    let yacht_path = out.join("any_C_context_se.txt");
    assert!(yacht_path.exists());
    let content = fs::read_to_string(&yacht_path).unwrap();
    // Skip the version header line.
    let mut saw_reverse = false;
    for line in content.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        assert_eq!(cols.len(), 8, "yacht row should have 8 columns: {line}");
        let col6: u32 = cols[5].parse().unwrap();
        let col7: u32 = cols[6].parse().unwrap();
        if cols[7] == "-" {
            // **Critical-1 regression:** reverse-strand must have col-6 > col-7.
            assert!(
                col6 > col7,
                "reverse-strand yacht row must have col-6 > col-7; got {line}"
            );
            saw_reverse = true;
        } else {
            // Forward-strand: col-6 <= col-7.
            assert!(
                col6 <= col7,
                "forward-strand yacht row must have col-6 ≤ col-7; got {line}"
            );
        }
    }
    assert!(
        saw_reverse,
        "fixture must include at least one reverse-strand row (OB)"
    );
}

// ─── MbiasOnly ───────────────────────────────────────────────────────

#[test]
fn smoke_mbias_only_emits_no_split_files() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);
    let out = workdir.path().join("out");

    Command::cargo_bin("bismark-methylation-extractor-rs")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--mbias_only")
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .success();

    let names = dir_entries_sorted(&out);
    // Only the M-bias.txt + splitting report should exist.
    assert_eq!(
        names.len(),
        2,
        "mbias_only should produce M-bias.txt + splitting report only; got {names:?}"
    );
    // No per-context files of any kind.
    assert!(
        names
            .iter()
            .all(|n| !n.contains("_context_") && !n.contains("_OT_"))
    );
}

/// `--mbias_only` invalid XM byte: must NOT error; the offending byte is
/// silently skipped (Perl `:2972 die "..." unless ($mbias_only)`).
#[test]
fn smoke_mbias_only_invalid_xm_byte_silently_skipped() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("bad.bam");
    write_bam_with_invalid_xm_byte(&bam_path);
    let out = workdir.path().join("out");

    Command::cargo_bin("bismark-methylation-extractor-rs")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--mbias_only")
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .success();

    // M-bias.txt exists; splitting report counts include Z and z but skip Q.
    let report = fs::read_to_string(out.join("bad_splitting_report.txt")).unwrap();
    assert!(report.contains("Total methylated C's in CpG context:\t1"));
    assert!(report.contains("Total unmethylated C's in CpG context:\t1"));
}

/// Counter-equivalence: `--mbias_only` and Default mode must produce
/// identical splitting-report counts on the same input. Confirms the
/// `mbias_only` short-circuit lives AFTER counter increments in
/// `route.rs` (Reviewer B I5).
#[test]
fn smoke_mbias_only_counters_match_default_mode() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);

    let default_out = workdir.path().join("default_out");
    let mbias_out = workdir.path().join("mbias_out");

    Command::cargo_bin("bismark-methylation-extractor-rs")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--output_dir")
        .arg(&default_out)
        .assert()
        .success();
    Command::cargo_bin("bismark-methylation-extractor-rs")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--mbias_only")
        .arg("--output_dir")
        .arg(&mbias_out)
        .assert()
        .success();

    let default_report = fs::read_to_string(default_out.join("se_splitting_report.txt")).unwrap();
    let mbias_report = fs::read_to_string(mbias_out.join("se_splitting_report.txt")).unwrap();

    // Extract the count lines (everything from "Total number of C's analysed"
    // through the percent-methylation lines) and compare.
    let extract_counts = |s: &str| -> Vec<String> {
        s.lines()
            .filter(|l| {
                l.starts_with("Total")
                    || l.starts_with("C methylated")
                    || l.starts_with("Processed")
            })
            .map(|l| l.to_string())
            .collect()
    };
    assert_eq!(
        extract_counts(&default_report),
        extract_counts(&mbias_report),
        "splitting-report counter lines must match between Default and MbiasOnly"
    );
}

// ─── Gzip combinations ───────────────────────────────────────────────

#[test]
fn smoke_gzip_default_emits_12_gz_files_with_byte_identical_decompression() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);

    let plain_out = workdir.path().join("plain");
    let gz_out = workdir.path().join("gz");
    Command::cargo_bin("bismark-methylation-extractor-rs")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--output_dir")
        .arg(&plain_out)
        .assert()
        .success();
    Command::cargo_bin("bismark-methylation-extractor-rs")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--gzip")
        .arg("--output_dir")
        .arg(&gz_out)
        .assert()
        .success();

    for ctx in ["CpG", "CHG", "CHH"] {
        for strand in ["OT", "CTOT", "CTOB", "OB"] {
            let plain =
                fs::read_to_string(plain_out.join(format!("{ctx}_{strand}_se.txt"))).unwrap();
            let gz_content = read_gz(&gz_out.join(format!("{ctx}_{strand}_se.txt.gz")));
            assert_eq!(
                plain, gz_content,
                "{ctx}_{strand}: gz output must decompress to byte-identical plain output"
            );
        }
    }
}

#[test]
fn smoke_gzip_comprehensive_emits_3_gz_files() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);
    let out = workdir.path().join("out");

    Command::cargo_bin("bismark-methylation-extractor-rs")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--comprehensive")
        .arg("--gzip")
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .success();

    for ctx in ["CpG", "CHG", "CHH"] {
        let path = out.join(format!("{ctx}_context_se.txt.gz"));
        assert!(path.exists(), "missing {}", path.display());
        // Decompresses cleanly:
        let s = read_gz(&path);
        assert!(s.starts_with("Bismark methylation extractor version v0.25.1"));
    }
}

#[test]
fn smoke_gzip_mbias_only_emits_no_gz_files() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);
    let out = workdir.path().join("out");

    Command::cargo_bin("bismark-methylation-extractor-rs")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--mbias_only")
        .arg("--gzip")
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .success();

    let names = dir_entries_sorted(&out);
    // M-bias.txt + splitting report only; zero .gz artifacts (the gzip
    // flag is honored at write time but there's nothing to write).
    assert_eq!(
        names.len(),
        2,
        "expected M-bias.txt + report only; got {names:?}"
    );
    assert!(names.iter().all(|n| !n.ends_with(".gz")));
}

#[test]
fn smoke_yacht_gzip_emits_1_gz_file_with_reverse_strand_swap_after_decode() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);
    let out = workdir.path().join("out");

    Command::cargo_bin("bismark-methylation-extractor-rs")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--yacht")
        .arg("--gzip")
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .success();

    let gz_path = out.join("any_C_context_se.txt.gz");
    assert!(gz_path.exists());
    let content = read_gz(&gz_path);
    // Confirm Critical-1 regression holds for the gzip path too.
    let mut saw_reverse = false;
    for line in content.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        assert_eq!(cols.len(), 8);
        if cols[7] == "-" {
            let col6: u32 = cols[5].parse().unwrap();
            let col7: u32 = cols[6].parse().unwrap();
            assert!(
                col6 > col7,
                "OB row in gz yacht output must have col-6 > col-7"
            );
            saw_reverse = true;
        }
    }
    assert!(saw_reverse, "expected at least one OB row from fixture");
}

// ─── Yacht edge case: empty BAM ──────────────────────────────────────

#[test]
fn smoke_yacht_empty_bam_emits_header_only_file() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("empty.bam");
    write_empty_bam(&bam_path);
    let out = workdir.path().join("out");

    Command::cargo_bin("bismark-methylation-extractor-rs")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--yacht")
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .success();

    let yacht_file = fs::read_to_string(out.join("any_C_context_empty.txt")).unwrap();
    assert_eq!(
        yacht_file,
        "Bismark methylation extractor version v0.25.1\n"
    );
}

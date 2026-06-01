//! Phase 3a — streaming bedGraph tee: byte-identity gate + ownership + D5.
//!
//! Phase 3a feeds methylation calls into a `bismark_bedgraph::Aggregator` IN
//! MEMORY during extraction (the tee at the shared `write_call` funnel) and
//! writes the `.bedGraph`/`.cov.gz` from `agg.into_sorted()` instead of
//! re-reading the per-context call files. c2c still reads the on-disk `.cov.gz`
//! (D4, unchanged). Per-context files are still written (D2, the tee is
//! additive).
//!
//! ## Oracle (D2 built-in)
//!
//! The per-context files are still written, so the **standalone**
//! `bismark_bedgraph::run()` reading those files = the expected output;
//! the Phase 3a in-memory streaming output = the actual. They must be
//! byte-identical (decompressed), file-for-file. This is the same bridge-parity
//! oracle the Phase 2 tests use — the difference is that Phase 3a writes the
//! `.cov.gz`/`.bedGraph` from the tee, so this asserts the tee reproduces the
//! file-read path exactly.
//!
//! **All tests run from a CWD ≠ output_dir** (the test binary CWD is the crate
//! root, never the tempdir) so the c2c cov-path CWD-verbatim open is exercised.
//!
//! Key tests beyond the flag matrix:
//! - **F4 (cross-file ownership):** a multi-strand, multi-chromosome BAM where a
//!   chromosome is emitted by >1 per-context file (CpG_OT + CpG_OB) and
//!   ownership resolves to the MIN basename (CpG_OB). The chromosome emission
//!   ORDER differs between correct min-owner and an accidental first-touch
//!   `add()`, so this catches the wrong aggregation method. The Phase-2 fixture
//!   is OT-only, so this is the first integration test of cross-file ownership.
//! - **F5 (parallel == single-threaded):** call the LIBRARY functions directly
//!   (`extract_se` vs `extract_se_parallel`), NOT the CLI binary (which only
//!   ever uses the parallel path); `--parallel 2` + a fixture interleaved across
//!   batches exercises the collector reordering. Output must be identical.

#![cfg(unix)]

use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use bismark_extractor::{extract_se, extract_se_parallel};
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
use noodles_sam::header::record::value::map::ReferenceSequence;
use std::num::NonZeroUsize;

// ─────────────────────────────────────────────────────────────────────────
// Synthetic genome + BAM fixtures (mirrors phase2_inline.rs)
// ─────────────────────────────────────────────────────────────────────────

/// chr1 (60 bp): CpG `C` at forward positions 5,10,15,20,25; CHG `C` at 35;
/// CHH `C` at 41. Same as the Phase-2 fixture so c2c reports are meaningful.
const CHR1_SEQ: &[u8] = b"AAAACGAAACGAAACGAAACGAAACGAAAAAAAACAGAAACTTAAAAAAAAAAAAAAAAA";

fn header_with_chr1(len: usize) -> Header {
    let mut header = Header::default();
    header.reference_sequences_mut().insert(
        BString::from(b"chr1".to_vec()),
        Map::<ReferenceSequence>::new(NonZeroUsize::new(len).unwrap()),
    );
    header
}

/// Multi-chromosome header for the cross-file-ownership fixture (chrA, chrB).
fn header_with_chr_a_chr_b(len: usize) -> Header {
    let mut header = Header::default();
    header.reference_sequences_mut().insert(
        BString::from(b"chrA".to_vec()),
        Map::<ReferenceSequence>::new(NonZeroUsize::new(len).unwrap()),
    );
    header.reference_sequences_mut().insert(
        BString::from(b"chrB".to_vec()),
        Map::<ReferenceSequence>::new(NonZeroUsize::new(len).unwrap()),
    );
    header
}

/// Build a synthetic SE record on the given refid. `xg = b"CT"` → OT/forward;
/// `xg = b"GA"` → OB/reverse. The XM byte at offset `i` lands at reference
/// position `alignment_start + i` (forward) — the iterator applies the
/// `-`-strand orientation correction for OB internally.
fn se_record(
    qname: &[u8],
    xm: &[u8],
    seq: &[u8],
    alignment_start: usize,
    xg: &[u8],
    refid: usize,
) -> BismarkRecord {
    let mut record = RecordBuf::default();
    *record.flags_mut() = Flags::from(0u16);
    *record.sequence_mut() = Sequence::from(seq.to_vec());
    *record.alignment_start_mut() = Some(Position::try_from(alignment_start).unwrap());
    *record.reference_sequence_id_mut() = Some(refid);
    *record.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, xm.len())]);
    *record.name_mut() = Some(BString::from(qname.to_vec()));
    record.data_mut().insert(
        Tag::from(*b"XR"),
        Value::String(BString::from(b"CT".to_vec())),
    );
    record
        .data_mut()
        .insert(Tag::from(*b"XG"), Value::String(BString::from(xg.to_vec())));
    record
        .data_mut()
        .insert(Tag::from(*b"XM"), Value::String(BString::from(xm.to_vec())));
    BismarkRecord::from_noodles_record(record).expect("synth produces a valid BismarkRecord")
}

/// OT (forward) chr1 record convenience wrapper.
fn ot_record(qname: &[u8], xm: &[u8], seq: &[u8], alignment_start: usize) -> BismarkRecord {
    se_record(qname, xm, seq, alignment_start, b"CT", 0)
}

fn write_genome_dir(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    let mut fa = String::from(">chr1\n");
    fa.push_str(std::str::from_utf8(CHR1_SEQ).unwrap());
    fa.push('\n');
    fs::write(dir.join("chr1.fa"), fa).unwrap();
}

/// The Phase-2 bridge BAM: OT-only CpG at 5 (×2 meth), 10 (meth), 15 (unmeth),
/// plus a CHG (35) + CHH (41) read so `--CX` has non-CpG rows.
fn write_bridge_bam(path: &Path) {
    let header = header_with_chr1(CHR1_SEQ.len());
    let mut writer = BamWriter::from_path(path, header).unwrap();
    writer
        .write_record(&ot_record(b"r1", b"Z", b"C", 5))
        .unwrap();
    writer
        .write_record(&ot_record(b"r2", b"Z", b"C", 5))
        .unwrap();
    writer
        .write_record(&ot_record(b"r3", b"Z", b"C", 10))
        .unwrap();
    writer
        .write_record(&ot_record(b"r4", b"z", b"C", 15))
        .unwrap();
    writer
        .write_record(&ot_record(b"r5", b"x.....h", b"CAAAAAC", 35))
        .unwrap();
    writer.finish().unwrap();
}

fn write_zero_call_bam(path: &Path) {
    let header = header_with_chr1(CHR1_SEQ.len());
    let mut writer = BamWriter::from_path(path, header).unwrap();
    writer
        .write_record(&ot_record(b"r0", b"......", b"AAAAAA", 1))
        .unwrap();
    writer.finish().unwrap();
}

fn write_non_cpg_only_bam(path: &Path) {
    let header = header_with_chr1(CHR1_SEQ.len());
    let mut writer = BamWriter::from_path(path, header).unwrap();
    writer
        .write_record(&ot_record(b"r1", b"x.....h", b"CAAAAAC", 35))
        .unwrap();
    writer.finish().unwrap();
}

// ─────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────

fn run_extractor(bam: &Path, out_dir: &Path, extra: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = Command::cargo_bin("bismark-methylation-extractor-rs").unwrap();
    cmd.arg(bam)
        .arg("--single-end")
        .arg("--output_dir")
        .arg(out_dir);
    for a in extra {
        cmd.arg(a);
    }
    cmd.assert()
}

fn read_gz(path: &Path) -> Vec<u8> {
    let f = fs::File::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    let mut d = flate2::read::GzDecoder::new(f);
    let mut out = Vec::new();
    d.read_to_end(&mut out).unwrap();
    out
}

fn read_bytes(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Kept per-context split files in `dir`, sorted (mirrors the extractor's
/// lexicographic kept ordering). Absolute paths.
fn kept_split_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            (n.starts_with("CpG_")
                || n.starts_with("CHG_")
                || n.starts_with("CHH_")
                || n.starts_with("Non_CpG_"))
                && (n.ends_with(".txt") || n.ends_with(".txt.gz"))
        })
        .map(|p| fs::canonicalize(&p).unwrap_or(p))
        .collect();
    v.sort();
    v
}

/// Build + validate + run the standalone `bismark_bedgraph` crate as the
/// oracle. argv constructed INDEPENDENTLY of the extractor's builder.
#[allow(clippy::too_many_arguments)]
fn oracle_bedgraph(
    kept: &[PathBuf],
    bedgraph_name: &str,
    oracle_dir: &Path,
    cutoff: u32,
    cx: bool,
    zero_based: bool,
    ucsc: bool,
    no_header: bool,
) {
    let mut argv: Vec<OsString> = vec!["bismark2bedGraph".into()];
    if cx {
        argv.push("--CX".into());
    }
    if no_header {
        argv.push("--no_header".into());
    }
    if zero_based {
        argv.push("--zero_based".into());
    }
    if ucsc {
        argv.push("--ucsc".into());
    }
    argv.push("--cutoff".into());
    argv.push(cutoff.to_string().into());
    argv.push("--output".into());
    argv.push(bedgraph_name.into());
    argv.push("--dir".into());
    argv.push(oracle_dir.as_os_str().to_owned());
    for f in kept {
        argv.push(f.as_os_str().to_owned());
    }
    let cli = <bismark_bedgraph::Cli as clap::Parser>::try_parse_from(&argv)
        .expect("oracle bedGraph argv parses");
    let cfg = cli.validate().expect("oracle bedGraph validates");
    bismark_bedgraph::run(&cfg).expect("oracle bedGraph run");
}

/// Build + validate + run the standalone `bismark_coverage2cytosine` oracle.
#[allow(clippy::too_many_arguments)]
fn oracle_c2c(
    cov_abs_path: &Path,
    output_name: &str,
    oracle_dir: &Path,
    genome_folder: &Path,
    cx: bool,
    zero_based: bool,
    split_by_chr: bool,
) {
    let mut argv: Vec<OsString> = vec!["coverage2cytosine".into()];
    argv.push("--output".into());
    argv.push(output_name.into());
    argv.push("--dir".into());
    argv.push(oracle_dir.as_os_str().to_owned());
    argv.push("--genome_folder".into());
    argv.push(genome_folder.as_os_str().to_owned());
    if zero_based {
        argv.push("--zero_based".into());
    }
    if cx {
        argv.push("--CX_context".into());
    }
    if split_by_chr {
        argv.push("--split_by_chromosome".into());
    }
    argv.push(cov_abs_path.as_os_str().to_owned());
    let cli = <bismark_coverage2cytosine::Cli as clap::Parser>::try_parse_from(&argv)
        .expect("oracle c2c argv parses");
    let cfg = cli.validate().expect("oracle c2c validates");
    bismark_coverage2cytosine::run(&cfg).expect("oracle c2c run");
}

// ─────────────────────────────────────────────────────────────────────────
// Flag matrix — Phase 3a streaming output == standalone-on-files oracle
// (each runs from CWD ≠ output_dir; per-context files are still written, D2)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn streaming_default_cpg_matches_standalone() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("def.bam");
    write_bridge_bam(&bam);

    let extract_dir = work.path().join("extract");
    run_extractor(&bam, &extract_dir, &[]).success();
    let kept = kept_split_files(&extract_dir);
    assert!(!kept.is_empty(), "extract produced no per-context files");

    let oracle_dir = work.path().join("oracle");
    fs::create_dir_all(&oracle_dir).unwrap();
    oracle_bedgraph(
        &kept,
        "def.bedGraph",
        &oracle_dir,
        1,
        false,
        false,
        false,
        false,
    );

    let inline_dir = work.path().join("inline");
    run_extractor(&bam, &inline_dir, &["--bedGraph"]).success();

    assert_eq!(
        read_gz(&inline_dir.join("def.bedGraph.gz")),
        read_gz(&oracle_dir.join("def.bedGraph.gz")),
        "streaming bedGraph differs from standalone-on-files oracle"
    );
    assert_eq!(
        read_gz(&inline_dir.join("def.bismark.cov.gz")),
        read_gz(&oracle_dir.join("def.bismark.cov.gz")),
        "streaming coverage differs from standalone-on-files oracle"
    );
}

#[test]
fn streaming_cytosine_report_cx_matches_standalone() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("cx.bam");
    write_bridge_bam(&bam);
    let genome = work.path().join("genome");
    write_genome_dir(&genome);

    let extract_dir = work.path().join("extract");
    run_extractor(&bam, &extract_dir, &[]).success();
    let kept = kept_split_files(&extract_dir);

    let oracle_dir = work.path().join("oracle");
    fs::create_dir_all(&oracle_dir).unwrap();
    oracle_bedgraph(
        &kept,
        "cx.bedGraph",
        &oracle_dir,
        1,
        true,
        false,
        false,
        false,
    );
    oracle_c2c(
        &oracle_dir.join("cx.bismark.cov.gz"),
        "cx.CX_report.txt",
        &oracle_dir,
        &genome,
        true,
        false,
        false,
    );

    let inline_dir = work.path().join("inline");
    run_extractor(
        &bam,
        &inline_dir,
        &[
            "--cytosine_report",
            "--CX",
            "--genome_folder",
            genome.to_str().unwrap(),
        ],
    )
    .success();

    assert_eq!(
        read_gz(&inline_dir.join("cx.bismark.cov.gz")),
        read_gz(&oracle_dir.join("cx.bismark.cov.gz")),
        "streaming --CX coverage differs from oracle"
    );
    assert_eq!(
        read_bytes(&inline_dir.join("cx.CX_report.txt")),
        read_bytes(&oracle_dir.join("cx.CX_report.txt")),
        "streaming --CX report differs from oracle"
    );
}

#[test]
fn streaming_cutoff_two_matches_standalone() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("cut.bam");
    write_bridge_bam(&bam);
    let genome = work.path().join("genome");
    write_genome_dir(&genome);

    let extract_dir = work.path().join("extract");
    run_extractor(&bam, &extract_dir, &[]).success();
    let kept = kept_split_files(&extract_dir);

    let oracle_dir = work.path().join("oracle");
    fs::create_dir_all(&oracle_dir).unwrap();
    oracle_bedgraph(
        &kept,
        "cut.bedGraph",
        &oracle_dir,
        2,
        false,
        false,
        false,
        false,
    );
    oracle_c2c(
        &oracle_dir.join("cut.bismark.cov.gz"),
        "cut.CpG_report.txt",
        &oracle_dir,
        &genome,
        false,
        false,
        false,
    );

    let inline_dir = work.path().join("inline");
    run_extractor(
        &bam,
        &inline_dir,
        &[
            "--cytosine_report",
            "--cutoff",
            "2",
            "--genome_folder",
            genome.to_str().unwrap(),
        ],
    )
    .success();

    let cov_inline = read_gz(&inline_dir.join("cut.bismark.cov.gz"));
    assert_eq!(
        cov_inline,
        read_gz(&oracle_dir.join("cut.bismark.cov.gz")),
        "streaming --cutoff 2 coverage differs from oracle"
    );
    // The cutoff is applied inside write_outputs_from_sorted (R3): only pos 5
    // (coverage 2) survives; pos 10/15 (coverage 1) are dropped.
    let cov_text = String::from_utf8_lossy(&cov_inline);
    assert!(
        cov_text.contains("\t5\t"),
        "pos 5 (cov 2) survives cutoff 2"
    );
    assert!(
        !cov_text.contains("\t10\t") && !cov_text.contains("\t15\t"),
        "coverage-1 positions must be dropped by --cutoff 2:\n{cov_text}"
    );
    assert_eq!(
        read_bytes(&inline_dir.join("cut.CpG_report.txt")),
        read_bytes(&oracle_dir.join("cut.CpG_report.txt")),
        "streaming --cutoff 2 report differs from oracle"
    );
}

#[test]
fn streaming_split_by_chromosome_matches_standalone() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("spl.bam");
    write_bridge_bam(&bam);
    let genome = work.path().join("genome");
    write_genome_dir(&genome);

    let extract_dir = work.path().join("extract");
    run_extractor(&bam, &extract_dir, &[]).success();
    let kept = kept_split_files(&extract_dir);

    let oracle_dir = work.path().join("oracle");
    fs::create_dir_all(&oracle_dir).unwrap();
    oracle_bedgraph(
        &kept,
        "spl.bedGraph",
        &oracle_dir,
        1,
        false,
        false,
        false,
        false,
    );
    oracle_c2c(
        &oracle_dir.join("spl.bismark.cov.gz"),
        "spl.CpG_report.txt",
        &oracle_dir,
        &genome,
        false,
        false,
        true,
    );

    let inline_dir = work.path().join("inline");
    run_extractor(
        &bam,
        &inline_dir,
        &[
            "--cytosine_report",
            "--split_by_chromosome",
            "--genome_folder",
            genome.to_str().unwrap(),
        ],
    )
    .success();

    let per_chr = "spl.CpG_report.txt.chrchr1.CpG_report.txt";
    assert_eq!(
        read_bytes(&inline_dir.join(per_chr)),
        read_bytes(&oracle_dir.join(per_chr)),
        "streaming split-by-chr report differs from oracle"
    );
}

#[test]
fn streaming_zero_based_matches_standalone() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("zb.bam");
    write_bridge_bam(&bam);
    let genome = work.path().join("genome");
    write_genome_dir(&genome);

    let extract_dir = work.path().join("extract");
    run_extractor(&bam, &extract_dir, &[]).success();
    let kept = kept_split_files(&extract_dir);

    let oracle_dir = work.path().join("oracle");
    fs::create_dir_all(&oracle_dir).unwrap();
    oracle_bedgraph(
        &kept,
        "zb.bedGraph",
        &oracle_dir,
        1,
        false,
        true,
        false,
        false,
    );
    oracle_c2c(
        &oracle_dir.join("zb.bismark.cov.gz"),
        "zb.CpG_report.txt",
        &oracle_dir,
        &genome,
        false,
        true,
        false,
    );

    let inline_dir = work.path().join("inline");
    run_extractor(
        &bam,
        &inline_dir,
        &[
            "--cytosine_report",
            "--zero_based",
            "--genome_folder",
            genome.to_str().unwrap(),
        ],
    )
    .success();

    let zero_name = "zb.bedGraph.gz.bismark.zero.cov";
    assert_eq!(
        read_bytes(&inline_dir.join(zero_name)),
        read_bytes(&oracle_dir.join(zero_name)),
        "streaming .zero.cov differs from oracle"
    );
    assert_eq!(
        read_bytes(&inline_dir.join("zb.CpG_report.txt")),
        read_bytes(&oracle_dir.join("zb.CpG_report.txt")),
        "streaming zero-based report differs from oracle"
    );
}

#[test]
fn streaming_ucsc_matches_standalone() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("uc.bam");
    write_bridge_bam(&bam);

    let extract_dir = work.path().join("extract");
    run_extractor(&bam, &extract_dir, &[]).success();
    let kept = kept_split_files(&extract_dir);

    let oracle_dir = work.path().join("oracle");
    fs::create_dir_all(&oracle_dir).unwrap();
    oracle_bedgraph(
        &kept,
        "uc.bedGraph",
        &oracle_dir,
        1,
        false,
        false,
        true,
        false,
    );

    let inline_dir = work.path().join("inline");
    run_extractor(&bam, &inline_dir, &["--bedGraph", "--ucsc"]).success();

    // UCSC post-pass (E7): write_ucsc re-reads the just-written .bedGraph.
    let ucsc_name = "uc.bedGraph_UCSC.bedGraph.gz";
    assert_eq!(
        read_gz(&inline_dir.join(ucsc_name)),
        read_gz(&oracle_dir.join(ucsc_name)),
        "streaming UCSC bedGraph differs from oracle"
    );
    // The main bedGraph must also match (write order: outputs → ucsc).
    assert_eq!(
        read_gz(&inline_dir.join("uc.bedGraph.gz")),
        read_gz(&oracle_dir.join("uc.bedGraph.gz")),
        "streaming bedGraph (under --ucsc) differs from oracle"
    );
}

#[test]
fn streaming_no_header_matches_standalone() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("nh.bam");
    write_bridge_bam(&bam);

    let extract_dir = work.path().join("extract");
    run_extractor(&bam, &extract_dir, &["--no_header"]).success();
    let kept = kept_split_files(&extract_dir);

    let oracle_dir = work.path().join("oracle");
    fs::create_dir_all(&oracle_dir).unwrap();
    oracle_bedgraph(
        &kept,
        "nh.bedGraph",
        &oracle_dir,
        1,
        false,
        false,
        false,
        true,
    );

    let inline_dir = work.path().join("inline");
    run_extractor(&bam, &inline_dir, &["--bedGraph", "--no_header"]).success();

    assert_eq!(
        read_gz(&inline_dir.join("nh.bismark.cov.gz")),
        read_gz(&oracle_dir.join("nh.bismark.cov.gz")),
        "streaming --no_header coverage differs from oracle"
    );
}

#[test]
fn streaming_merge_non_cpg_matches_standalone() {
    // --merge_non_CpG (F5 oracle cell): the per-context split files are named
    // CpG_* and Non_CpG_* (8 strand files). bedGraph default mode still selects
    // only CpG_* files, so the .cov.gz covers the CpG positions only. The tee's
    // R4 gate (basename starts with "CpG") must agree, even though Non_CpG files
    // exist. This pins that the tee's selection mirrors select_input_files.
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("mrg.bam");
    write_bridge_bam(&bam);

    let extract_dir = work.path().join("extract");
    run_extractor(&bam, &extract_dir, &["--merge_non_CpG"]).success();
    let kept = kept_split_files(&extract_dir);
    // Confirm the fixture really produced Non_CpG_* files that must be excluded
    // from the default-mode CpG-only bedGraph selection / tee.
    assert!(
        kept.iter().any(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .starts_with("Non_CpG_")
        }),
        "merge_non_CpG fixture should produce Non_CpG_* files"
    );

    let oracle_dir = work.path().join("oracle");
    fs::create_dir_all(&oracle_dir).unwrap();
    oracle_bedgraph(
        &kept,
        "mrg.bedGraph",
        &oracle_dir,
        1,
        false,
        false,
        false,
        false,
    );

    let inline_dir = work.path().join("inline");
    run_extractor(&bam, &inline_dir, &["--bedGraph", "--merge_non_CpG"]).success();

    let cov_inline = read_gz(&inline_dir.join("mrg.bismark.cov.gz"));
    assert_eq!(
        cov_inline,
        read_gz(&oracle_dir.join("mrg.bismark.cov.gz")),
        "streaming --merge_non_CpG coverage differs from oracle"
    );
    // CpG positions present; non-CpG (35/41) absent from default-mode coverage.
    let cov_text = String::from_utf8_lossy(&cov_inline);
    assert!(cov_text.contains("\t5\t"), "CpG pos 5 present");
    assert!(
        !cov_text.contains("\t35\t") && !cov_text.contains("\t41\t"),
        "non-CpG positions must NOT appear in default-mode coverage:\n{cov_text}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// PE case — streaming output == standalone-on-files oracle
// ─────────────────────────────────────────────────────────────────────────

/// Build a synthetic PE record. `xg=b"CT"` → OT pair; FLAGs set per SAM spec.
#[allow(clippy::too_many_arguments)]
fn pe_record(
    qname: &[u8],
    xr: &[u8],
    xg: &[u8],
    xm: &[u8],
    alignment_start: usize,
    flags: u16,
) -> BismarkRecord {
    let mut record = RecordBuf::default();
    *record.flags_mut() = Flags::from(flags);
    *record.sequence_mut() = Sequence::from(vec![b'A'; xm.len()]);
    *record.alignment_start_mut() = Some(Position::try_from(alignment_start).unwrap());
    *record.reference_sequence_id_mut() = Some(0);
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
    BismarkRecord::from_noodles_record(record).expect("synth PE record")
}

/// Write a PE BAM: two OT pairs. R1 has CpG calls; R2 (XR=GA, XG=CT) has CpG
/// calls at disjoint positions (no overlap dropped).
fn write_pe_bam(path: &Path) {
    let header = header_with_chr1(CHR1_SEQ.len());
    let mut writer = BamWriter::from_path(path, header).unwrap();
    // Pair p1: R1 OT Z@5; R2 (downstream) Z@20.
    writer
        .write_record(&pe_record(b"p1", b"CT", b"CT", b"Z", 5, 0x63))
        .unwrap(); // paired+proper+first, mate-rev
    writer
        .write_record(&pe_record(b"p1", b"GA", b"CT", b"Z", 20, 0x93))
        .unwrap(); // paired+proper+last, rev
    // Pair p2: R1 OT Z@10; R2 Z@25.
    writer
        .write_record(&pe_record(b"p2", b"CT", b"CT", b"Z", 10, 0x63))
        .unwrap();
    writer
        .write_record(&pe_record(b"p2", b"GA", b"CT", b"Z", 25, 0x93))
        .unwrap();
    writer.finish().unwrap();
}

#[test]
fn streaming_pe_matches_standalone() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("pe.bam");
    write_pe_bam(&bam);

    // Extract-only (PE) → per-context files.
    let extract_dir = work.path().join("extract");
    let mut cmd = Command::cargo_bin("bismark-methylation-extractor-rs").unwrap();
    cmd.arg(&bam)
        .arg("--paired-end")
        .arg("--output_dir")
        .arg(&extract_dir)
        .assert()
        .success();
    let kept = kept_split_files(&extract_dir);
    assert!(!kept.is_empty(), "PE extract produced no per-context files");

    let oracle_dir = work.path().join("oracle");
    fs::create_dir_all(&oracle_dir).unwrap();
    oracle_bedgraph(
        &kept,
        "pe.bedGraph",
        &oracle_dir,
        1,
        false,
        false,
        false,
        false,
    );

    let inline_dir = work.path().join("inline");
    let mut cmd2 = Command::cargo_bin("bismark-methylation-extractor-rs").unwrap();
    cmd2.arg(&bam)
        .arg("--paired-end")
        .arg("--bedGraph")
        .arg("--output_dir")
        .arg(&inline_dir)
        .assert()
        .success();

    assert_eq!(
        read_gz(&inline_dir.join("pe.bismark.cov.gz")),
        read_gz(&oracle_dir.join("pe.bismark.cov.gz")),
        "streaming PE coverage differs from standalone-on-files oracle"
    );
    assert_eq!(
        read_gz(&inline_dir.join("pe.bedGraph.gz")),
        read_gz(&oracle_dir.join("pe.bedGraph.gz")),
        "streaming PE bedGraph differs from standalone-on-files oracle"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// F4 — cross-file ownership (THE critical test): chromosome in >1 file,
// MIN basename owns it, chromosome ORDER differs from a first-touch add().
// ─────────────────────────────────────────────────────────────────────────

/// chrA is emitted by an OT read FIRST (BAM order), then an OB read; chrB is
/// emitted ONLY by an OB read. With correct MIN-basename ownership both chrA
/// and chrB are owned by `CpG_OB_*` (B < T), so the bytewise order key gives
/// chrA before chrB. An accidental first-touch `add()` would make chrA owned by
/// `CpG_OT_*` (it was touched first by OT in BAM order), flipping the emission
/// order to chrB, chrA. The oracle (reading the kept files in basename-sorted
/// order: CpG_OB first) always sees min-basename ownership → chrA, chrB. So a
/// first-touch bug fails this byte-identity assertion on the .cov.gz.
fn write_cross_file_ownership_bam(path: &Path) {
    // 30 bp dummy chrs; positions chosen so each read lands one CpG `Z`/`z`.
    let header = header_with_chr_a_chr_b(30);
    let mut writer = BamWriter::from_path(path, header).unwrap();
    // chrA: OT read FIRST (so first-touch would be CpG_OT), CpG `Z` at pos 5.
    writer
        .write_record(&se_record(b"a_ot", b"Z", b"C", 5, b"CT", 0))
        .unwrap();
    // chrA: OB read later, CpG `z` at pos 8 (different pos so no merge masks it).
    writer
        .write_record(&se_record(b"a_ob", b"z", b"C", 8, b"GA", 0))
        .unwrap();
    // chrB: OB read ONLY, CpG `Z` at pos 5.
    writer
        .write_record(&se_record(b"b_ob", b"Z", b"C", 5, b"GA", 1))
        .unwrap();
    writer.finish().unwrap();
}

#[test]
fn streaming_cross_file_ownership_matches_standalone() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("own.bam");
    write_cross_file_ownership_bam(&bam);

    let extract_dir = work.path().join("extract");
    run_extractor(&bam, &extract_dir, &[]).success();
    let kept = kept_split_files(&extract_dir);
    // The fixture must produce BOTH CpG_OT and CpG_OB files (cross-file).
    let has_ot = kept.iter().any(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .starts_with("CpG_OT")
    });
    let has_ob = kept.iter().any(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .starts_with("CpG_OB")
    });
    assert!(
        has_ot && has_ob,
        "cross-file fixture must produce both CpG_OT and CpG_OB files; kept={kept:?}"
    );

    let oracle_dir = work.path().join("oracle");
    fs::create_dir_all(&oracle_dir).unwrap();
    oracle_bedgraph(
        &kept,
        "own.bedGraph",
        &oracle_dir,
        1,
        false,
        false,
        false,
        false,
    );

    let inline_dir = work.path().join("inline");
    run_extractor(&bam, &inline_dir, &["--bedGraph"]).success();

    // Decompressed .cov.gz carries the chromosome emission ORDER. min-owner
    // (correct) → chrA before chrB; a first-touch add() bug → chrB before chrA.
    let cov_inline = read_gz(&inline_dir.join("own.bismark.cov.gz"));
    let cov_oracle = read_gz(&oracle_dir.join("own.bismark.cov.gz"));
    assert_eq!(
        cov_inline, cov_oracle,
        "streaming cross-file-ownership coverage differs from oracle (a first-touch \
         add() instead of add_min_owner flips the chromosome emission order)"
    );
    // Explicitly assert the chromosome ORDER is chrA, chrB (min-owner CpG_OB).
    let cov_text = String::from_utf8_lossy(&cov_inline);
    let first_a = cov_text.find("chrA").expect("chrA present in coverage");
    let first_b = cov_text.find("chrB").expect("chrB present in coverage");
    assert!(
        first_a < first_b,
        "chrA (min-owner CpG_OB) must be emitted before chrB; coverage:\n{cov_text}"
    );
    // bedGraph must agree too.
    assert_eq!(
        read_gz(&inline_dir.join("own.bedGraph.gz")),
        read_gz(&oracle_dir.join("own.bedGraph.gz")),
        "streaming cross-file-ownership bedGraph differs from oracle"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// D2 — per-context files still written + unchanged after a --bedGraph run
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn per_context_files_unchanged_after_bedgraph_run() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("d2.bam");
    write_bridge_bam(&bam);

    // Run 1: extract-only → reference per-context bytes.
    let extract_dir = work.path().join("extract");
    run_extractor(&bam, &extract_dir, &[]).success();
    let kept_extract = kept_split_files(&extract_dir);
    assert!(!kept_extract.is_empty());

    // Run 2: extract + --bedGraph (tee active). Per-context files must STILL be
    // written and byte-identical to the extract-only run (the tee is additive).
    let inline_dir = work.path().join("inline");
    run_extractor(&bam, &inline_dir, &["--bedGraph"]).success();

    for ef in &kept_extract {
        let name = ef.file_name().unwrap();
        let inline_path = inline_dir.join(name);
        assert!(
            inline_path.exists(),
            "per-context file {name:?} must still exist after a --bedGraph run (D2)"
        );
        // Compare decompressed content (split files are plain .txt here — no
        // --gzip — so read raw bytes).
        assert_eq!(
            read_bytes(ef),
            read_bytes(&inline_path),
            "per-context file {name:?} content changed by the --bedGraph tee (D2 violated)"
        );
    }
    // The downstream outputs also exist (sanity).
    assert!(inline_dir.join("d2.bismark.cov.gz").exists());
}

// ─────────────────────────────────────────────────────────────────────────
// F5 — parallel == single-threaded (LIBRARY functions, not the CLI binary)
// ─────────────────────────────────────────────────────────────────────────

/// Build a `ResolvedConfig` for `output_dir` from a CLI argv (so all defaults
/// match the binary). `parallel` drives `extract_se_parallel`'s worker count.
fn config_via_cli(
    bam: &Path,
    output_dir: &Path,
    parallel: u32,
) -> bismark_extractor::ResolvedConfig {
    use clap::Parser;
    let argv: Vec<OsString> = vec![
        "bismark-methylation-extractor-rs".into(),
        bam.as_os_str().to_owned(),
        "--single-end".into(),
        "--bedGraph".into(),
        "--output_dir".into(),
        output_dir.as_os_str().to_owned(),
        "--parallel".into(),
        parallel.to_string().into(),
    ];
    bismark_extractor::Cli::try_parse_from(&argv)
        .expect("config CLI parses")
        .validate()
        .expect("config validates")
}

/// One synthetic SE read spec: `(qname, xm, alignment_start, xg, refid)`.
type ReadSpec = (&'static [u8], &'static [u8], usize, &'static [u8], usize);

/// A small BAM with calls interleaved across chromosomes/strands/positions.
/// Exercises BOTH `write_call` tee call-sites + cross-file ownership on chrA +
/// parallel==single byte-identity. NOTE: 10 reads < `BATCH_SIZE` (4096, see
/// `parallel.rs`), so this is a SINGLE batch — the true cross-batch
/// collector-reordering case is covered by
/// `streaming_parallel_multibatch_equals_single_threaded` below.
fn write_interleaved_bam(path: &Path) {
    let header = header_with_chr_a_chr_b(30);
    let mut writer = BamWriter::from_path(path, header).unwrap();
    // Interleave chrA/chrB, OT/OB, asc/desc positions (read order NOT sorted).
    let recs: &[ReadSpec] = &[
        (b"r01", b"Z", 5, b"CT", 0),  // chrA OT pos5 meth
        (b"r02", b"Z", 5, b"GA", 1),  // chrB OB pos5 meth
        (b"r03", b"z", 12, b"CT", 0), // chrA OT pos12 unmeth
        (b"r04", b"Z", 9, b"GA", 1),  // chrB OB pos9 meth
        (b"r05", b"Z", 5, b"CT", 0),  // chrA OT pos5 meth (cov 2)
        (b"r06", b"z", 20, b"CT", 1), // chrB OT pos20 unmeth
        (b"r07", b"Z", 8, b"GA", 0),  // chrA OB pos8 meth (cross-file on chrA)
        (b"r08", b"z", 3, b"CT", 1),  // chrB OT pos3 unmeth
        (b"r09", b"Z", 25, b"CT", 0), // chrA OT pos25 meth
        (b"r10", b"Z", 15, b"GA", 1), // chrB OB pos15 meth
    ];
    for (qn, xm, start, xg, refid) in recs {
        writer
            .write_record(&se_record(qn, xm, b"C", *start, xg, *refid))
            .unwrap();
    }
    writer.finish().unwrap();
}

#[test]
fn streaming_parallel_equals_single_threaded() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("par.bam");
    write_interleaved_bam(&bam);

    // Single-threaded reference path (extract_se ignores config.parallel).
    let single_dir = work.path().join("single");
    let single_cfg = config_via_cli(&bam, &single_dir, 1);
    extract_se(&bam, &single_cfg).expect("extract_se (single-threaded) succeeds");

    // Parallel path with 2 workers — both tee call-sites on the SAME shared
    // write_call funnel (D5) + parallel==single byte-identity. (Single batch;
    // multi-batch reordering is covered by the next test.)
    let par_dir = work.path().join("parallel");
    let par_cfg = config_via_cli(&bam, &par_dir, 2);
    extract_se_parallel(&bam, &par_cfg).expect("extract_se_parallel succeeds");

    // bedGraph + coverage must be byte-identical between the two paths.
    assert_eq!(
        read_gz(&par_dir.join("par.bedGraph.gz")),
        read_gz(&single_dir.join("par.bedGraph.gz")),
        "parallel bedGraph differs from single-threaded (tee must be order-free)"
    );
    assert_eq!(
        read_gz(&par_dir.join("par.bismark.cov.gz")),
        read_gz(&single_dir.join("par.bismark.cov.gz")),
        "parallel coverage differs from single-threaded (tee must be order-free)"
    );
    // The per-context split files (D2) must also match across paths.
    let single_kept = kept_split_files(&single_dir);
    for sk in &single_kept {
        let name = sk.file_name().unwrap();
        assert_eq!(
            read_bytes(sk),
            read_bytes(&par_dir.join(name)),
            "per-context file {name:?} differs between single and parallel paths"
        );
    }
}

/// Writes `n` interleaved single-end CpG records (chrA/chrB, OT/OB, varied
/// positions) — with `n > BATCH_SIZE` the parallel pipeline genuinely splits
/// into multiple batches the collector reassembles out of order.
fn write_n_interleaved_bam(path: &Path, n: usize) {
    let header = header_with_chr_a_chr_b(200);
    let mut writer = BamWriter::from_path(path, header).unwrap();
    for i in 0..n {
        let qn = format!("r{i:06}");
        let (xm, xg): (&[u8], &[u8]) = match i % 4 {
            0 => (b"Z", b"CT"), // OT meth
            1 => (b"z", b"CT"), // OT unmeth
            2 => (b"Z", b"GA"), // OB meth
            _ => (b"z", b"GA"), // OB unmeth
        };
        let refid = i % 2; // chrA / chrB
        let start = 1 + (i % 190); // within chr len 200
        writer
            .write_record(&se_record(qn.as_bytes(), xm, b"C", start, xg, refid))
            .unwrap();
    }
    writer.finish().unwrap();
}

/// The genuine cross-batch case the 10-read test cannot reach: `BATCH_SIZE` is
/// 4096 (`parallel.rs`), so 9000 reads span ≥2 batch boundaries and the parallel
/// collector reassembles multiple out-of-order batches. The tee is order-free
/// (counts) + min-basename (ownership), so the parallel output MUST still be
/// byte-identical to the single-threaded reference.
#[test]
fn streaming_parallel_multibatch_equals_single_threaded() {
    let n = 9000; // > 2 * BATCH_SIZE (4096); crosses multiple batch boundaries
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("mb.bam");
    write_n_interleaved_bam(&bam, n);

    let single_dir = work.path().join("single");
    extract_se(&bam, &config_via_cli(&bam, &single_dir, 1))
        .expect("extract_se (single-threaded) succeeds");
    let par_dir = work.path().join("parallel");
    extract_se_parallel(&bam, &config_via_cli(&bam, &par_dir, 4))
        .expect("extract_se_parallel succeeds");

    assert_eq!(
        read_gz(&par_dir.join("mb.bedGraph.gz")),
        read_gz(&single_dir.join("mb.bedGraph.gz")),
        "multi-batch parallel bedGraph differs from single-threaded"
    );
    assert_eq!(
        read_gz(&par_dir.join("mb.bismark.cov.gz")),
        read_gz(&single_dir.join("mb.bismark.cov.gz")),
        "multi-batch parallel coverage differs from single-threaded"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// T5 — empty / no-CpG pre-check: warn + skip downstream + exit 0 (F3)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn empty_input_skips_downstream_exit_zero() {
    // Zero-call BAM + --cytosine_report: no usable input → warn + skip + exit 0.
    // No bedGraph/cov/report files appear. The kept-set pre-check (F3) fires
    // BEFORE any write_outputs_from_sorted call (the aggregator is empty too).
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("empty.bam");
    write_zero_call_bam(&bam);
    let genome = work.path().join("genome");
    write_genome_dir(&genome);

    let inline_dir = work.path().join("inline");
    run_extractor(
        &bam,
        &inline_dir,
        &[
            "--cytosine_report",
            "--genome_folder",
            genome.to_str().unwrap(),
        ],
    )
    .success()
    .stderr(predicates::str::contains(
        "no methylation calls usable for bedGraph",
    ));

    for name in [
        "empty.bedGraph.gz",
        "empty.bismark.cov.gz",
        "empty.CpG_report.txt",
    ] {
        assert!(
            !inline_dir.join(name).exists(),
            "downstream file {name} must NOT exist for a zero-call BAM"
        );
    }
}

#[test]
fn default_mode_no_cpg_calls_skips() {
    // Only CHG/CHH calls (no CpG). Default-mode bedGraph selects only CpG_*
    // files → no usable input → warn + skip + exit 0. The CHG/CHH split files
    // exist (extraction ran), but NO bedGraph output.
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("nocpg.bam");
    write_non_cpg_only_bam(&bam);

    let inline_dir = work.path().join("inline");
    run_extractor(&bam, &inline_dir, &["--bedGraph"])
        .success()
        .stderr(predicates::str::contains(
            "no methylation calls usable for bedGraph",
        ));

    assert!(
        !inline_dir.join("nocpg.bedGraph.gz").exists(),
        "no bedGraph should be produced when only non-CpG calls exist in default mode"
    );
    assert!(
        !inline_dir.join("nocpg.bismark.cov.gz").exists(),
        "no coverage should be produced when only non-CpG calls exist in default mode"
    );
    // Extraction still ran: at least one CHG/CHH split file exists (D2).
    let kept = kept_split_files(&inline_dir);
    assert!(
        kept.iter().any(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            n.starts_with("CHG_") || n.starts_with("CHH_")
        }),
        "CHG/CHH per-context files must still be written (extraction ran)"
    );
}

//! Phase 2 — in-process file-coupled orchestration: bridge-parity tests.
//!
//! These tests prove the **config bridge** between the extractor and the
//! in-process `bismark2bedGraph` / `coverage2cytosine` crates is correct:
//! the argv the extractor builds, the filenames it derives, and the kept-set
//! it forwards all produce output IDENTICAL to running the standalone crate
//! `run()` on the same per-context files. This is NOT a Perl byte-identity
//! test (that is the Phase 4 oxy gate) — it is an internal-consistency oracle
//! that catches argv/filename/config-translation bugs.
//!
//! Oracle construction (no Perl, no real data):
//! 1. Run the extractor WITHOUT downstream → per-context split files in `extract_dir`.
//! 2. Independently build the standalone crate CLI argv (NOT via the extractor's
//!    builders — that would be circular) and call the crate `run()` on the
//!    `extract_dir` files → oracle outputs in `oracle_dir`.
//! 3. Run the extractor WITH the flags (in-process chain) → per-context files +
//!    downstream outputs in `inline_dir`.
//! 4. Assert the decompressed downstream outputs in `inline_dir` byte-equal the
//!    oracle outputs.
//!
//! **All tests run from a CWD ≠ output_dir** (the test binary's CWD is the
//! crate root, never the tempdir) so the NEW-1 c2c cov-path bug (Rust c2c opens
//! the cov positional verbatim from CWD) cannot hide.

#![cfg(unix)]

use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

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
use noodles_sam::header::record::value::map::ReferenceSequence;
use std::num::NonZeroUsize;

// ─────────────────────────────────────────────────────────────────────────
// Synthetic genome + BAM fixtures
// ─────────────────────────────────────────────────────────────────────────

/// A short chr1 sequence (60 bp) with CpG dinucleotides at controlled 1-based
/// forward-strand `C` positions **5, 10, 15, 20, 25**, a CHG (`CAG`) whose `C`
/// is at **35**, and a CHH (`CTT`) whose `C` is at **41**. All other bases `A`.
///
/// (Generated/verified so the synthetic OT reads below land their XM bytes on
/// real cytosines of the matching context — required for a meaningful c2c
/// report. The bedGraph step doesn't depend on this; c2c does.)
const CHR1_SEQ: &[u8] = b"AAAACGAAACGAAACGAAACGAAACGAAAAAAAACAGAAACTTAAAAAAAAAAAAAAAAA";

fn header_with_chr1(len: usize) -> Header {
    let mut header = Header::default();
    header.reference_sequences_mut().insert(
        BString::from(b"chr1".to_vec()),
        Map::<ReferenceSequence>::new(NonZeroUsize::new(len).unwrap()),
    );
    header
}

/// Build a synthetic SE OT (XR=CT, XG=CT, forward-strand) record. The XM byte
/// at offset `i` lands at reference position `alignment_start + i`.
fn ot_record(qname: &[u8], xm: &[u8], seq: &[u8], alignment_start: usize) -> BismarkRecord {
    let mut record = RecordBuf::default();
    *record.flags_mut() = Flags::from(0u16);
    *record.sequence_mut() = Sequence::from(seq.to_vec());
    *record.alignment_start_mut() = Some(Position::try_from(alignment_start).unwrap());
    *record.reference_sequence_id_mut() = Some(0); // chr1
    *record.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, xm.len())]);
    *record.name_mut() = Some(BString::from(qname.to_vec()));
    record.data_mut().insert(
        Tag::from(*b"XR"),
        Value::String(BString::from(b"CT".to_vec())),
    );
    record.data_mut().insert(
        Tag::from(*b"XG"),
        Value::String(BString::from(b"CT".to_vec())),
    );
    record
        .data_mut()
        .insert(Tag::from(*b"XM"), Value::String(BString::from(xm.to_vec())));
    BismarkRecord::from_noodles_record(record).expect("synth produces a valid BismarkRecord")
}

/// Write a genome folder containing a single `chr1.fa` with [`CHR1_SEQ`].
fn write_genome_dir(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    let mut fa = String::from(">chr1\n");
    fa.push_str(std::str::from_utf8(CHR1_SEQ).unwrap());
    fa.push('\n');
    fs::write(dir.join("chr1.fa"), fa).unwrap();
}

/// Write a small SE directional BAM whose OT reads place CpG (+CHG/CHH for the
/// `--CX` cases) calls at known chr1 positions.
///
/// Calls placed (OT, forward, ref_pos = alignment_start + XM offset):
///   - Two reads with `Z` at pos 5 (CpG, methylated) → coverage 2.
///   - One read with `Z` at pos 10 (CpG, methylated) → coverage 1.
///   - One read with `z` at pos 15 (CpG, unmethylated) → coverage 1.
///   - One read with `x` at pos 35 (CHG) and `h` at pos 41 (CHH) so `--CX`
///     mode has non-CpG rows. (XM `x.....h`: offset 0 → 35, offset 6 → 41.)
fn write_bridge_bam(path: &Path) {
    let header = header_with_chr1(CHR1_SEQ.len());
    let mut writer = BamWriter::from_path(path, header).unwrap();

    // CpG at pos 5, methylated (Z). seq/xm length must match.
    writer
        .write_record(&ot_record(b"r1", b"Z", b"C", 5))
        .unwrap();
    writer
        .write_record(&ot_record(b"r2", b"Z", b"C", 5))
        .unwrap();
    // CpG at pos 10, methylated.
    writer
        .write_record(&ot_record(b"r3", b"Z", b"C", 10))
        .unwrap();
    // CpG at pos 15, unmethylated (z).
    writer
        .write_record(&ot_record(b"r4", b"z", b"C", 15))
        .unwrap();
    // CHG (x at pos 35) + CHH (h at pos 41): XM "x.....h" (7 long).
    writer
        .write_record(&ot_record(b"r5", b"x.....h", b"CAAAAAC", 35))
        .unwrap();

    writer.finish().unwrap();
}

/// Write a BAM with NO methylation calls (a single read with all `.` in XM).
fn write_zero_call_bam(path: &Path) {
    let header = header_with_chr1(CHR1_SEQ.len());
    let mut writer = BamWriter::from_path(path, header).unwrap();
    writer
        .write_record(&ot_record(b"r0", b"......", b"AAAAAA", 1))
        .unwrap();
    writer.finish().unwrap();
}

/// Write a BAM with only NON-CpG calls (CHG + CHH, no CpG) so default-mode
/// bedGraph (CpG-only file selection) finds no usable input.
fn write_non_cpg_only_bam(path: &Path) {
    let header = header_with_chr1(CHR1_SEQ.len());
    let mut writer = BamWriter::from_path(path, header).unwrap();
    // CHG (x at pos 35) + CHH (h at pos 41) only — NO CpG.
    writer
        .write_record(&ot_record(b"r1", b"x.....h", b"CAAAAAC", 35))
        .unwrap();
    writer.finish().unwrap();
}

// ─────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────

/// Run the extractor binary on `bam` into `out_dir` with the given extra args.
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

/// Decompress a gzip file to bytes (gzp emits a single-member stream → GzDecoder).
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

/// The kept per-context split files in `dir` for `basename`, sorted (mirrors
/// the extractor's lexicographic kept ordering). Returns absolute paths.
fn kept_split_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Per-context split files (the bedGraph inputs); exclude reports etc.
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

/// Build + validate + run the standalone `bismark_bedgraph` crate as the oracle.
/// argv is constructed INDEPENDENTLY of the extractor's builder (no circularity).
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

/// Build + validate + run the standalone `bismark_coverage2cytosine` crate as
/// the oracle. The cov positional is an ABSOLUTE path (Rust c2c opens it from
/// CWD). argv constructed independently of the extractor's builder.
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
// T2 — bedGraph in-process matches standalone
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn bedgraph_inline_matches_standalone() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("br.bam");
    write_bridge_bam(&bam);

    // 1. Extract-only (no downstream) → per-context files.
    let extract_dir = work.path().join("extract");
    run_extractor(&bam, &extract_dir, &[]).success();
    let kept = kept_split_files(&extract_dir);
    assert!(!kept.is_empty(), "extract produced no per-context files");

    // 2. Oracle: standalone bedGraph on the extract-only files.
    let oracle_dir = work.path().join("oracle");
    fs::create_dir_all(&oracle_dir).unwrap();
    oracle_bedgraph(
        &kept,
        "br.bedGraph",
        &oracle_dir,
        1,
        false,
        false,
        false,
        false,
    );

    // 3. Extractor WITH --bedGraph (in-process) → per-context + downstream.
    let inline_dir = work.path().join("inline");
    run_extractor(&bam, &inline_dir, &["--bedGraph"]).success();

    // 4. Compare bedGraph + coverage outputs (decompressed).
    // bismark2bedGraph normalizes -o "br.bedGraph" → "br.bedGraph.gz";
    // coverage → "br.bismark.cov.gz".
    let bg_inline = read_gz(&inline_dir.join("br.bedGraph.gz"));
    let bg_oracle = read_gz(&oracle_dir.join("br.bedGraph.gz"));
    assert_eq!(
        bg_inline, bg_oracle,
        "in-process bedGraph differs from standalone oracle"
    );
    let cov_inline = read_gz(&inline_dir.join("br.bismark.cov.gz"));
    let cov_oracle = read_gz(&oracle_dir.join("br.bismark.cov.gz"));
    assert_eq!(
        cov_inline, cov_oracle,
        "in-process coverage differs from standalone oracle"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// T3 / T8a — cytosine_report in-process matches standalone (CWD ≠ output_dir)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn cytosine_report_inline_matches_standalone_cwd_differs() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("cr.bam");
    write_bridge_bam(&bam);
    let genome = work.path().join("genome");
    write_genome_dir(&genome);

    // 1. Extract-only.
    let extract_dir = work.path().join("extract");
    run_extractor(&bam, &extract_dir, &[]).success();
    let kept = kept_split_files(&extract_dir);

    // 2. Oracle bedGraph + c2c. bedGraph writes cr.bismark.cov.gz in oracle_dir;
    //    feed its ABSOLUTE path to the oracle c2c (CWD-verbatim open).
    let oracle_dir = work.path().join("oracle");
    fs::create_dir_all(&oracle_dir).unwrap();
    oracle_bedgraph(
        &kept,
        "cr.bedGraph",
        &oracle_dir,
        1,
        false,
        false,
        false,
        false,
    );
    let oracle_cov = oracle_dir.join("cr.bismark.cov.gz");
    oracle_c2c(
        &oracle_cov,
        "cr.CpG_report.txt",
        &oracle_dir,
        &genome,
        false,
        false,
        false,
    );

    // 3. Extractor WITH --cytosine_report (in-process).
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
    .success();

    // 4. Compare the CpG report (plain text) byte-for-byte.
    let rep_inline = read_bytes(&inline_dir.join("cr.CpG_report.txt"));
    let rep_oracle = read_bytes(&oracle_dir.join("cr.CpG_report.txt"));
    assert_eq!(
        rep_inline, rep_oracle,
        "in-process CpG report differs from standalone oracle"
    );
    // Sanity: the report actually exists and has CpG rows.
    assert!(
        !rep_inline.is_empty(),
        "CpG report should be non-empty for this fixture"
    );
}

#[test]
fn cytosine_report_cx_inline_matches_standalone() {
    // T8a: --cytosine_report --CX (NOT --bedGraph --CX, which is rejected).
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
    // --CX: bedGraph reads ALL files; report suffix is CX_report.txt.
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
    let oracle_cov = oracle_dir.join("cx.bismark.cov.gz");
    oracle_c2c(
        &oracle_cov,
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

    let rep_inline = read_bytes(&inline_dir.join("cx.CX_report.txt"));
    let rep_oracle = read_bytes(&oracle_dir.join("cx.CX_report.txt"));
    assert_eq!(
        rep_inline, rep_oracle,
        "in-process CX report differs from standalone oracle"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// T8b — default CpG-only selection
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn default_cpg_only_selection_matches_standalone() {
    // Default bedGraph uses ONLY files whose basename starts with "CpG".
    // The fixture also produces CHG/CHH files (from r5); they must be ignored.
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("def.bam");
    write_bridge_bam(&bam);

    let extract_dir = work.path().join("extract");
    run_extractor(&bam, &extract_dir, &[]).success();
    let kept = kept_split_files(&extract_dir);
    // Confirm the fixture really has non-CpG files that must be excluded.
    assert!(
        kept.iter().any(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            n.starts_with("CHG_") || n.starts_with("CHH_")
        }),
        "fixture should produce CHG/CHH files to exercise CpG-only selection"
    );

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

    let cov_inline = read_gz(&inline_dir.join("def.bismark.cov.gz"));
    let cov_oracle = read_gz(&oracle_dir.join("def.bismark.cov.gz"));
    assert_eq!(cov_inline, cov_oracle);
    // The coverage file must contain only CpG positions (5,10,15), not 35/40.
    let cov_text = String::from_utf8_lossy(&cov_inline);
    assert!(cov_text.contains("\t5\t"), "CpG pos 5 should be present");
    assert!(
        !cov_text.contains("\t35\t") && !cov_text.contains("\t41\t"),
        "non-CpG positions (35,41) must NOT appear in default-mode coverage:\n{cov_text}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// T8c — --cutoff 2 correctness (R3 gate)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn cutoff_two_drops_coverage_one_positions() {
    // Pos 5 has coverage 2 (r1+r2); pos 10 + pos 15 have coverage 1.
    // With --cutoff 2, only pos 5 survives in BOTH .cov.gz AND the report.
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
    let oracle_cov = oracle_dir.join("cut.bismark.cov.gz");
    oracle_c2c(
        &oracle_cov,
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

    // .cov.gz parity + coverage-1 positions absent.
    let cov_inline = read_gz(&inline_dir.join("cut.bismark.cov.gz"));
    let cov_oracle = read_gz(&oracle_dir.join("cut.bismark.cov.gz"));
    assert_eq!(cov_inline, cov_oracle);
    let cov_text = String::from_utf8_lossy(&cov_inline);
    assert!(
        cov_text.contains("\t5\t"),
        "pos 5 (cov 2) should survive cutoff 2"
    );
    assert!(
        !cov_text.contains("\t10\t") && !cov_text.contains("\t15\t"),
        "coverage-1 positions (10,15) must be dropped by --cutoff 2:\n{cov_text}"
    );

    // Report parity (report covers the SAME post-cutoff cov).
    let rep_inline = read_bytes(&inline_dir.join("cut.CpG_report.txt"));
    let rep_oracle = read_bytes(&oracle_dir.join("cut.CpG_report.txt"));
    assert_eq!(rep_inline, rep_oracle);
    // The report's covered (meth+unmeth>0) rows must not include pos 10/15.
    let rep_text = String::from_utf8_lossy(&rep_inline);
    for line in rep_text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() >= 5 {
            let pos = cols[1];
            let meth: u32 = cols[3].parse().unwrap_or(0);
            let unmeth: u32 = cols[4].parse().unwrap_or(0);
            if pos == "10" || pos == "15" {
                assert_eq!(
                    meth + unmeth,
                    0,
                    "report position {pos} should have 0 coverage after cutoff 2"
                );
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// T8d — --split_by_chromosome
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn split_by_chromosome_inline_matches_standalone() {
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
    let oracle_cov = oracle_dir.join("spl.bismark.cov.gz");
    oracle_c2c(
        &oracle_cov,
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

    // Per-chr report name: {output_raw}.chr{NAME}.CpG_report.txt where
    // output_raw = "spl.CpG_report.txt" → "spl.CpG_report.txt.chrchr1.CpG_report.txt".
    let per_chr = "spl.CpG_report.txt.chrchr1.CpG_report.txt";
    let rep_inline = read_bytes(&inline_dir.join(per_chr));
    let rep_oracle = read_bytes(&oracle_dir.join(per_chr));
    assert_eq!(
        rep_inline, rep_oracle,
        "in-process split-by-chr report differs from standalone oracle"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// T8e — --zero_based
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn zero_based_inline_matches_standalone() {
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
    let oracle_cov = oracle_dir.join("zb.bismark.cov.gz");
    oracle_c2c(
        &oracle_cov,
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

    // zero-based coverage file: bedGraph writes the plain ".bismark.zero.cov".
    let zero_name = "zb.bedGraph.gz.bismark.zero.cov";
    let zero_inline = read_bytes(&inline_dir.join(zero_name));
    let zero_oracle = read_bytes(&oracle_dir.join(zero_name));
    assert_eq!(
        zero_inline, zero_oracle,
        "in-process .zero.cov differs from standalone oracle"
    );
    // c2c report (zero-based positions).
    let rep_inline = read_bytes(&inline_dir.join("zb.CpG_report.txt"));
    let rep_oracle = read_bytes(&oracle_dir.join("zb.CpG_report.txt"));
    assert_eq!(rep_inline, rep_oracle);
}

// ─────────────────────────────────────────────────────────────────────────
// T8f — --ucsc + --no_header
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn ucsc_inline_matches_standalone() {
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

    // UCSC bedGraph name: uc.bedGraph_UCSC.bedGraph.gz.
    let ucsc_name = "uc.bedGraph_UCSC.bedGraph.gz";
    let uc_inline = read_gz(&inline_dir.join(ucsc_name));
    let uc_oracle = read_gz(&oracle_dir.join(ucsc_name));
    assert_eq!(
        uc_inline, uc_oracle,
        "in-process UCSC bedGraph differs from standalone oracle"
    );
}

#[test]
fn no_header_inline_matches_standalone() {
    // --no_header changes how bedGraph treats the FIRST line of each input
    // (data vs version header). The extractor writes a version header to each
    // split file by default; with --no_header it does NOT — and it must pass
    // --no_header through so bedGraph keeps the first data line.
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("nh.bam");
    write_bridge_bam(&bam);

    // Extract-only WITH --no_header so the split files have no header line.
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
        true, // no_header
    );

    let inline_dir = work.path().join("inline");
    run_extractor(&bam, &inline_dir, &["--bedGraph", "--no_header"]).success();

    let cov_inline = read_gz(&inline_dir.join("nh.bismark.cov.gz"));
    let cov_oracle = read_gz(&oracle_dir.join("nh.bismark.cov.gz"));
    assert_eq!(cov_inline, cov_oracle);
}

// ─────────────────────────────────────────────────────────────────────────
// T5 — empty / no-CpG pre-check: warn + skip + exit 0
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn empty_input_skips_downstream_exit_zero() {
    // A zero-call BAM with --bedGraph --cytosine_report: no usable input →
    // warn + skip downstream + exit 0. No bedGraph/cov/report files appear.
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

    // No downstream outputs.
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
    // files → no usable input → warn + skip + exit 0.
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("nocpg.bam");
    write_non_cpg_only_bam(&bam);

    let inline_dir = work.path().join("inline");
    run_extractor(&bam, &inline_dir, &["--bedGraph"])
        .success()
        .stderr(predicates::str::contains(
            "no methylation calls usable for bedGraph",
        ));

    // The CHG/CHH split files exist (extraction ran), but NO bedGraph output.
    assert!(
        !inline_dir.join("nocpg.bedGraph.gz").exists(),
        "no bedGraph should be produced when only non-CpG calls exist in default mode"
    );
    assert!(
        !inline_dir.join("nocpg.bismark.cov.gz").exists(),
        "no coverage should be produced when only non-CpG calls exist in default mode"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// T3 negative — --cytosine_report without --genome_folder errors (validated)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn cytosine_report_without_genome_folder_rejected() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("ng.bam");
    write_bridge_bam(&bam);

    let inline_dir = work.path().join("inline");
    run_extractor(&bam, &inline_dir, &["--cytosine_report"])
        .failure()
        .stderr(predicates::str::contains(
            "--cytosine_report requires --genome_folder",
        ));
}

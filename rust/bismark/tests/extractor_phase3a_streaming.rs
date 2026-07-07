//! Phase 3a — streaming bedGraph tee: byte-identity gate + ownership + D5.
//!
//! Phase 3a feeds methylation calls into a `bismark::bedgraph::Aggregator` IN
//! MEMORY during extraction (the tee at the shared `write_call` funnel) and
//! writes the `.bedGraph`/`.cov.gz` from `agg.into_sorted()` instead of
//! re-reading the per-context call files. c2c still reads the on-disk `.cov.gz`
//! (D4, unchanged). Per-context files are still written (D2, the tee is
//! additive).
//!
//! ## Oracle (D2 built-in)
//!
//! The per-context files are still written, so the **standalone**
//! `bismark::bedgraph::run()` reading those files = the expected output;
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
//!   ownership resolves to the **minimum creation-rank** file (CpG_OT, rank 0 —
//!   matching Perl's first-in-creation-order owner), NOT the min basename (which
//!   would wrongly pick CpG_OB). F4 guards min-rank-vs-min-basename + the
//!   creation-order oracle de-contamination. (first-touch-vs-min-rank is covered
//!   by the `aggregate.rs` unit tests + the `--CX` cross-context test, since F4's
//!   BAM feeds the rank-0 call first so first-touch coincides with min-rank here.)
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
use bismark::extractor::{OutputMode, extract_se, extract_se_parallel};
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
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
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
///
/// **Order-INDEPENDENT use only.** The lexicographic sort here matches the
/// extractor's `kept.sort()` but does NOT match Perl's chromosome ownership
/// (Perl hands `bismark2bedGraph` the files in *creation* order, not sorted).
/// For tests that assert the downstream chromosome EMISSION ORDER, use
/// [`per_context_files_in_creation_order`] so the standalone-bedgraph oracle
/// sees the SAME ownership the streaming tee does — otherwise the oracle would
/// re-encode the very min-basename bug this fix removes.
fn kept_split_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_per_context_file(p))
        .map(|p| fs::canonicalize(&p).unwrap_or(p))
        .collect();
    v.sort();
    v
}

/// True for a kept per-context split file (CpG/CHG/CHH/Non_CpG `.txt[.gz]`).
fn is_per_context_file(p: &Path) -> bool {
    let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    (n.starts_with("CpG_")
        || n.starts_with("CHG_")
        || n.starts_with("CHH_")
        || n.starts_with("Non_CpG_"))
        && (n.ends_with(".txt") || n.ends_with(".txt.gz"))
}

/// Kept per-context split files in `dir`, in Perl's **creation order** for
/// `mode` (the `mode_keys` order: `CpG_OT, CpG_CTOT, CpG_CTOB, CpG_OB, CHG_OT,
/// …, CHH_OB` for Default). Absolute paths.
///
/// This is the Perl-faithful oracle argv ordering: Perl creates the
/// per-context files in this order and hands them to `bismark2bedGraph`
/// WITHOUT sorting, so the FIRST file to emit a chromosome owns it. Feeding the
/// standalone bedgraph in this order reproduces the same ownership the
/// streaming tee resolves via min creation-rank — the whole point of the fix.
///
/// We derive the order by matching each kept file's `{context}_{strand}_`
/// prefix against the ordered prefixes from
/// [`bismark::extractor::mode_keys`], independent of the input basename. Any
/// kept file whose prefix is missing from `mode_keys` (shouldn't happen) is
/// appended last in lexicographic order so it is never silently dropped.
fn per_context_files_in_creation_order(
    dir: &Path,
    mode: bismark::extractor::OutputMode,
) -> Vec<PathBuf> {
    // Ordered list of `{context}_{strand}_` prefixes from mode_keys (creation
    // order). mode_keys filenames look like "CpG_OT_<basename>.txt"; the prefix
    // is everything up to and including the second underscore.
    let ordered_prefixes: Vec<String> = bismark::extractor::mode_keys(mode, "", false)
        .into_iter()
        .map(|(_, filename)| {
            // filename == "CpG_OT_.txt" when basename is "" → prefix "CpG_OT_".
            // Strip from the LAST occurrence of the basename marker: take up to
            // the third token boundary. Simpler: drop the trailing ".txt" and
            // the empty basename, keeping the "{ctx}_{strand}_" head.
            let stem = filename.strip_suffix(".txt").unwrap_or(&filename);
            // stem is e.g. "CpG_OT_" (Default) or "CpG_context_" (Comprehensive)
            // — keep it verbatim as the match prefix.
            stem.to_string()
        })
        .collect();

    let kept = kept_split_files(dir); // canonical, lexicographically sorted
    let mut ordered: Vec<PathBuf> = Vec::with_capacity(kept.len());
    let mut used = vec![false; kept.len()];
    for prefix in &ordered_prefixes {
        for (i, p) in kept.iter().enumerate() {
            if used[i] {
                continue;
            }
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if n.starts_with(prefix.as_str()) {
                ordered.push(p.clone());
                used[i] = true;
            }
        }
    }
    // Defensive: append any unmatched kept files (none expected) so the oracle
    // never silently loses an input.
    for (i, p) in kept.iter().enumerate() {
        if !used[i] {
            ordered.push(p.clone());
        }
    }
    ordered
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
    let cli = <bismark::bedgraph::Cli as clap::Parser>::try_parse_from(&argv)
        .expect("oracle bedGraph argv parses");
    let cfg = cli.validate().expect("oracle bedGraph validates");
    bismark::bedgraph::run(&cfg).expect("oracle bedGraph run");
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
    let cli = <bismark::coverage2cytosine::Cli as clap::Parser>::try_parse_from(&argv)
        .expect("oracle c2c argv parses");
    let cfg = cli.validate().expect("oracle c2c validates");
    bismark::coverage2cytosine::run(&cfg).expect("oracle c2c run");
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

/// `--CX --bedGraph` WITHOUT `--cytosine_report` (the nf-core/methylseq drop-in
/// case fixed on 2026-06-12). Perl accepts this and emits an all-C-context
/// coverage/bedGraph; the Rust CLI previously rejected it. This asserts:
///   5a — the all-context cov/bedGraph are byte-identical to the standalone
///        `bismark_bedgraph --CX` oracle over the kept per-context files;
///   5b — the cov genuinely carries non-CpG (CHG @35, CHH @41) rows, not just
///        CpG (guards against an all-context regression an order-equivalent
///        oracle could mask);
///   5c — skipping the trailing c2c step does NOT perturb the cov: the
///        `--CX --bedGraph` cov equals the `--CX --cytosine_report` cov on the
///        same input (c2c only READS the cov). Combined with
///        `streaming_cytosine_report_cx_matches_standalone` (which pins the c2c
///        path to the same oracle), this re-confirms Perl-identity in-repo.
#[test]
fn streaming_cx_bedgraph_without_cytosine_report_matches_standalone() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("cxbg.bam");
    write_bridge_bam(&bam);
    let genome = work.path().join("genome");
    write_genome_dir(&genome);

    // Oracle: extract per-context files, then run the STANDALONE bismark_bedgraph
    // --CX over them (independent of the extractor's in-memory tee).
    let extract_dir = work.path().join("extract");
    run_extractor(&bam, &extract_dir, &[]).success();
    let kept = kept_split_files(&extract_dir);
    assert!(!kept.is_empty(), "extract produced no per-context files");

    let oracle_dir = work.path().join("oracle");
    fs::create_dir_all(&oracle_dir).unwrap();
    oracle_bedgraph(
        &kept,
        "cxbg.bedGraph",
        &oracle_dir,
        1,
        /*cx=*/ true,
        false,
        false,
        false,
    );

    // Actual: --CX --bedGraph (NO --cytosine_report, NO --genome_folder).
    let inline_dir = work.path().join("inline");
    run_extractor(&bam, &inline_dir, &["--CX", "--bedGraph"]).success();

    // 5a — byte-identity to the standalone-bedgraph --CX oracle.
    let inline_cov = read_gz(&inline_dir.join("cxbg.bismark.cov.gz"));
    assert_eq!(
        inline_cov,
        read_gz(&oracle_dir.join("cxbg.bismark.cov.gz")),
        "--CX --bedGraph coverage differs from standalone bismark_bedgraph --CX oracle"
    );
    assert_eq!(
        read_gz(&inline_dir.join("cxbg.bedGraph.gz")),
        read_gz(&oracle_dir.join("cxbg.bedGraph.gz")),
        "--CX --bedGraph bedGraph differs from standalone bismark_bedgraph --CX oracle"
    );

    // 5b — all-context content: the cov must carry the CHG (pos 35) and CHH
    // (pos 41) rows from the fixture, not just CpG (5/10/15). Column 2 of the
    // bismark cov is the 1-based position.
    let cov_text = String::from_utf8(inline_cov).expect("cov is UTF-8");
    let positions: std::collections::HashSet<&str> = cov_text
        .lines()
        .filter_map(|l| l.split('\t').nth(1))
        .collect();
    assert!(
        positions.contains("35"),
        "--CX cov missing the CHG position 35 — all-context capture broken; positions={positions:?}"
    );
    assert!(
        positions.contains("41"),
        "--CX cov missing the CHH position 41 — all-context capture broken; positions={positions:?}"
    );

    // 5c — skipping c2c does not perturb the cov: --CX --bedGraph cov ==
    // --CX --cytosine_report cov on the same input.
    let c2c_dir = work.path().join("c2c");
    run_extractor(
        &bam,
        &c2c_dir,
        &[
            "--CX",
            "--cytosine_report",
            "--genome_folder",
            genome.to_str().unwrap(),
        ],
    )
    .success();
    assert_eq!(
        read_gz(&c2c_dir.join("cxbg.bismark.cov.gz")),
        read_gz(&inline_dir.join("cxbg.bismark.cov.gz")),
        "--CX --bedGraph cov must equal --CX --cytosine_report cov (c2c only reads the cov)"
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
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
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
    let mut cmd2 = Command::cargo_bin("bismark_methylation_extractor").unwrap();
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
/// emitted ONLY by an OB read. With correct MIN creation-RANK ownership chrA is
/// owned by `CpG_OT_*` (rank 0, the file created FIRST — Perl's `OT, CTOT,
/// CTOB, OB` order) and chrB by `CpG_OB_*` (rank 3). The bytewise order key is
/// `{owner}.chr{name}`, so `CpG_OB_*.chrB` < `CpG_OT_*.chrA` (the OB owner
/// prefix sorts first) → emission order is **chrB before chrA**.
///
/// This is the OPPOSITE of the interim min-basename rule, which owned BOTH
/// chromosomes by `CpG_OB_*` (B < T) and emitted chrA before chrB. The oracle
/// must read the kept files in **creation order** (`CpG_OT` before `CpG_OB`) so
/// its first-touch `add()` resolves chrA → `CpG_OT` exactly like the tee's
/// min-rank rule — reading them basename-sorted would re-encode the old bug.
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
    // The fixture must produce BOTH CpG_OT and CpG_OB files (cross-file).
    let sorted_kept = kept_split_files(&extract_dir);
    let has_ot = sorted_kept.iter().any(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .starts_with("CpG_OT")
    });
    let has_ob = sorted_kept.iter().any(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .starts_with("CpG_OB")
    });
    assert!(
        has_ot && has_ob,
        "cross-file fixture must produce both CpG_OT and CpG_OB files; kept={sorted_kept:?}"
    );

    // C1: feed the oracle in CREATION order (CpG_OT before CpG_OB) so its
    // first-touch ownership matches the tee's min-rank ownership. The
    // lexicographically-sorted order would put CpG_OB first → owns chrA → the
    // old bug. The two orders MUST differ for this fixture, else the oracle is
    // contaminated.
    let kept = per_context_files_in_creation_order(&extract_dir, OutputMode::Default);
    assert_ne!(
        kept, sorted_kept,
        "creation-order oracle must DIFFER from the lexicographically-sorted kept list \
         (else the oracle re-encodes the min-basename bug); creation={kept:?} sorted={sorted_kept:?}"
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

    // Decompressed .cov.gz carries the chromosome emission ORDER. min creation-
    // rank (correct) → chrA owned by CpG_OT (rank 0), chrB by CpG_OB (rank 3) →
    // key CpG_OB.chrB < CpG_OT.chrA → chrB BEFORE chrA. (The old min-basename
    // bug owned both by CpG_OB → chrA before chrB.)
    let cov_inline = read_gz(&inline_dir.join("own.bismark.cov.gz"));
    let cov_oracle = read_gz(&oracle_dir.join("own.bismark.cov.gz"));
    assert_eq!(
        cov_inline, cov_oracle,
        "streaming cross-file-ownership coverage differs from the creation-order oracle"
    );
    // Explicitly assert the chromosome ORDER is chrB, chrA (owners: chrB→CpG_OB
    // rank 3, chrA→CpG_OT rank 0; key CpG_OB.chrB < CpG_OT.chrA).
    let cov_text = String::from_utf8_lossy(&cov_inline);
    let first_a = cov_text.find("chrA").expect("chrA present in coverage");
    let first_b = cov_text.find("chrB").expect("chrB present in coverage");
    assert!(
        first_b < first_a,
        "chrB (owner CpG_OB) must be emitted before chrA (owner CpG_OT); coverage:\n{cov_text}"
    );
    // bedGraph must agree too.
    assert_eq!(
        read_gz(&inline_dir.join("own.bedGraph.gz")),
        read_gz(&oracle_dir.join("own.bedGraph.gz")),
        "streaming cross-file-ownership bedGraph differs from oracle"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Cross-CONTEXT ownership under --CX (the rank that only --CX reaches)
// ─────────────────────────────────────────────────────────────────────────

/// Two-chromosome genome for the --CX cross-context fixture:
///   chrA: CpG `C` at pos 5 (C@5, G@6), CHG `C` at pos 10 (C@10, A@11, G@12).
///   chrB: CHG `C` at pos 5 (C@5, A@6, G@7).
/// (1-based positions; everything else is `A`.)
const CX_CHR_A_SEQ: &[u8] = b"AAAACGAAACAGAAAAAAAAAA"; // 22 bp
const CX_CHR_B_SEQ: &[u8] = b"AAAACAGAAAAAAAAAA"; // 17 bp

fn write_cx_cross_context_genome(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    let mut fa = String::new();
    fa.push_str(">chrA\n");
    fa.push_str(std::str::from_utf8(CX_CHR_A_SEQ).unwrap());
    fa.push('\n');
    fa.push_str(">chrB\n");
    fa.push_str(std::str::from_utf8(CX_CHR_B_SEQ).unwrap());
    fa.push('\n');
    fs::write(dir.join("genome.fa"), fa).unwrap();
}

fn cx_header() -> Header {
    let mut header = Header::default();
    header.reference_sequences_mut().insert(
        BString::from(b"chrA".to_vec()),
        Map::<ReferenceSequence>::new(NonZeroUsize::new(CX_CHR_A_SEQ.len()).unwrap()),
    );
    header.reference_sequences_mut().insert(
        BString::from(b"chrB".to_vec()),
        Map::<ReferenceSequence>::new(NonZeroUsize::new(CX_CHR_B_SEQ.len()).unwrap()),
    );
    header
}

/// chrA's CHG read (→ `CHG_OT`, creation rank 4) is emitted in the BAM BEFORE
/// chrA's CpG read (→ `CpG_OT`, rank 0). With min creation-RANK ownership chrA
/// must end up owned by `CpG_OT` (rank 0), NOT `CHG_OT` (rank 4), even though
/// CHG touched it first in BAM order — this is the cross-context rank revision
/// that ONLY `--CX` exercises (without `--CX`, CHG calls are never teed). chrB
/// appears only in a CHG read → owned by `CHG_OT` (rank 4). Keys
/// `CHG_OT.chrB` < `CpG_OT.chrA` ('H' 0x48 < 'p' 0x70) → emission order chrB,
/// chrA.
fn write_cx_cross_context_bam(path: &Path) {
    let header = cx_header();
    let mut writer = BamWriter::from_path(path, header).unwrap();
    // chrA CHG read FIRST (first-touch would pick CHG_OT, rank 4).
    writer
        .write_record(&se_record(b"a_chg", b"X..", b"CAG", 10, b"CT", 0))
        .unwrap();
    // chrA CpG read SECOND (lower rank 0 must take over ownership).
    writer
        .write_record(&se_record(b"a_cpg", b"Z.", b"CG", 5, b"CT", 0))
        .unwrap();
    // chrB CHG read ONLY → owned by CHG_OT (rank 4).
    writer
        .write_record(&se_record(b"b_chg", b"X..", b"CAG", 5, b"CT", 1))
        .unwrap();
    writer.finish().unwrap();
}

#[test]
fn streaming_cx_cross_context_ownership_matches_standalone() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("cx_own.bam");
    write_cx_cross_context_bam(&bam);
    let genome = work.path().join("genome");
    write_cx_cross_context_genome(&genome);

    let extract_dir = work.path().join("extract");
    run_extractor(&bam, &extract_dir, &[]).success();

    // The fixture must produce BOTH a CpG and a CHG file (cross-context).
    let sorted_kept = kept_split_files(&extract_dir);
    let has_cpg = sorted_kept.iter().any(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .starts_with("CpG_")
    });
    let has_chg = sorted_kept.iter().any(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .starts_with("CHG_")
    });
    assert!(
        has_cpg && has_chg,
        "cross-context fixture must produce both a CpG and a CHG file; kept={sorted_kept:?}"
    );

    // C1: feed the oracle in CREATION order (CpG block before CHG block, so
    // CpG_OT precedes CHG_OT). Under --CX the cov ownership for a chr touched
    // by both contexts must resolve to the lower-rank CpG file — reading the
    // files basename-sorted would put CHG before CpG and contaminate the oracle.
    let kept = per_context_files_in_creation_order(&extract_dir, OutputMode::Default);
    // For this single-strand fixture the kept set is {CpG_OT, CHG_OT}; sorted
    // gives [CHG_OT, CpG_OT] but creation order gives [CpG_OT, CHG_OT] — they
    // MUST differ, else the oracle re-encodes the wrong cross-context order.
    assert_ne!(
        kept, sorted_kept,
        "creation-order oracle must DIFFER from the lexicographically-sorted kept list \
         (CpG block must precede CHG block); creation={kept:?} sorted={sorted_kept:?}"
    );

    let oracle_dir = work.path().join("oracle");
    fs::create_dir_all(&oracle_dir).unwrap();
    oracle_bedgraph(
        &kept,
        "cx_own.bedGraph",
        &oracle_dir,
        1,
        /*cx=*/ true,
        false,
        false,
        false,
    );
    oracle_c2c(
        &oracle_dir.join("cx_own.bismark.cov.gz"),
        "cx_own.CX_report.txt",
        &oracle_dir,
        &genome,
        /*cx=*/ true,
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

    // The .cov.gz carries the chromosome emission ORDER set by ownership.
    let cov_inline = read_gz(&inline_dir.join("cx_own.bismark.cov.gz"));
    let cov_oracle = read_gz(&oracle_dir.join("cx_own.bismark.cov.gz"));
    assert_eq!(
        cov_inline, cov_oracle,
        "streaming --CX cross-context coverage differs from the creation-order oracle"
    );
    // chrA owned by CpG_OT (rank 0, won over the higher-rank CHG_OT it was
    // touched by first); chrB owned by CHG_OT (rank 4). Keys CHG_OT.chrB <
    // CpG_OT.chrA → chrB before chrA.
    let cov_text = String::from_utf8_lossy(&cov_inline);
    let first_a = cov_text.find("chrA").expect("chrA present in coverage");
    let first_b = cov_text.find("chrB").expect("chrB present in coverage");
    assert!(
        first_b < first_a,
        "chrB (owner CHG_OT) must be emitted before chrA (owner CpG_OT, rank 0 won over \
         the CHG_OT it was touched by first); coverage:\n{cov_text}"
    );
    // The CX report must also be byte-identical (genome-driven content).
    assert_eq!(
        read_bytes(&inline_dir.join("cx_own.CX_report.txt")),
        read_bytes(&oracle_dir.join("cx_own.CX_report.txt")),
        "streaming --CX cross-context report differs from the creation-order oracle"
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
) -> bismark::extractor::ResolvedConfig {
    use clap::Parser;
    let argv: Vec<OsString> = vec![
        "bismark_methylation_extractor_rs".into(),
        bam.as_os_str().to_owned(),
        "--single-end".into(),
        "--bedGraph".into(),
        "--output_dir".into(),
        output_dir.as_os_str().to_owned(),
        "--parallel".into(),
        parallel.to_string().into(),
    ];
    bismark::extractor::Cli::try_parse_from(&argv)
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
// Empty-sample graceful outputs through the inline --cytosine_report chain
// (plan 06142026_empty-sample-extractor-c2c). A zero-total-calls run must
// flow through bedGraph + the inline c2c feed instead of skipping.
// ─────────────────────────────────────────────────────────────────────────

/// Files in `dir` whose name matches a simple `*suffix` glob, sorted.
fn glob_suffix(dir: &Path, suffix: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(suffix))
        })
        .collect();
    v.sort();
    v
}

#[test]
fn empty_input_emits_graceful_outputs_with_cytosine_report() {
    // REWRITTEN (was `empty_input_skips_downstream_exit_zero`): a zero-call BAM
    // with --cytosine_report now flows through the full inline chain — empty
    // bedGraph/cov + force-created per-context files, and the inline c2c feed
    // produces a genome-wide ALL-ZERO CpG report (the standard-path graceful
    // empty). Exit 0. DELIBERATE divergence from Perl.
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
    .success();

    // bedGraph + cov emitted (0 data rows).
    assert_eq!(
        glob_suffix(&inline_dir, ".bedGraph.gz").len(),
        1,
        "empty bedGraph must be emitted"
    );
    assert_eq!(
        glob_suffix(&inline_dir, ".bismark.cov.gz").len(),
        1,
        "empty cov must be emitted"
    );
    // ≥1 retained per-context split file.
    assert!(
        !kept_split_files(&inline_dir).is_empty(),
        "≥1 per-context file must be retained on the zero-call --cytosine_report path"
    );
    // The inline c2c produced an all-zero genome-wide CpG report (not gzipped:
    // --cytosine_report inline does not pass --gzip to c2c by default).
    let cx = glob_suffix(&inline_dir, "CpG_report.txt");
    assert_eq!(
        cx.len(),
        1,
        "the inline c2c must produce a CpG_report.txt on the graceful-empty path"
    );
    let report = std::fs::read_to_string(&cx[0]).unwrap();
    // Genome chr1 (CHR1_SEQ) has cytosines; all rows are 0/0 (no coverage).
    let data_rows: Vec<&str> = report.lines().filter(|l| !l.is_empty()).collect();
    assert!(
        !data_rows.is_empty(),
        "all-zero CpG report must contain genome cytosine rows"
    );
    assert!(
        data_rows.iter().all(|l| l.contains("\t0\t0\t")),
        "every CpG report row must be all-zero (0/0) on the empty path; got {data_rows:?}"
    );
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

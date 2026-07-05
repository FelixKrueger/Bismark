//! End-to-end tests for v1.x: multiple input files + single-end
//! coordinate-sorted input.
//!
//! Builds synthetic BAMs in-test (no Perl/samtools toolchain), runs the
//! `bismark_methylation_extractor_rs` binary, and asserts the v1.x contract:
//!
//! - SE coordinate-sorted input is ACCEPTED (explicit `--single-end` and
//!   AutoDetect), faithful to Perl which only sort-checks paired-end.
//! - PE coordinate-sorted input is REJECTED (`UnsortedInput`) under explicit
//!   `--paired-end` AND under AutoDetect-that-detects-PE.
//! - Multiple files are processed per-file (no pooling); per-file outputs are
//!   byte-identical to running each file alone (no cross-file state bleed).
//! - Fail-fast: a mid-batch failure keeps earlier files' (valid) outputs and
//!   cleans the failing file's partials; a missing file aborts pre-flight with
//!   zero outputs written.
//!
//! C1 (dual plan-review): SE split files are written in INPUT-RECORD order, so
//! they are NOT byte-identical across input reorderings — only the
//! position-aggregated bedGraph/.cov (and count-based reports) are
//! order-invariant. Tests assert the correct oracle accordingly.

use std::fs;
use std::io::Read;
use std::path::Path;

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
use noodles_sam::header::record::value::map::header::tag::SORT_ORDER;
use noodles_sam::header::record::value::map::program::tag::COMMAND_LINE;
use noodles_sam::header::record::value::map::{Header as HeaderMap, Program, ReferenceSequence};
use std::num::NonZeroUsize;

// ─────────────────────────────────────────────────────────────────────────
// Header / record / BAM helpers
// ─────────────────────────────────────────────────────────────────────────

const SO_COORDINATE: &[u8] = b"coordinate";
const SO_UNSORTED: &[u8] = b"unsorted";

fn chr1_ref(header: &mut Header) {
    header.reference_sequences_mut().insert(
        BString::from(b"chr1".to_vec()),
        Map::<ReferenceSequence>::new(NonZeroUsize::new(10_000).unwrap()),
    );
}

/// Header carrying `@HD ... SO:<so>` and a single `chr1` reference.
fn header_with_so(so: &[u8]) -> Header {
    let mut hd = Map::<HeaderMap>::new(Version::new(1, 0));
    hd.other_fields_mut()
        .insert(SORT_ORDER, BString::from(so.to_vec()));
    let mut header = Header::builder().set_header(hd).build();
    chr1_ref(&mut header);
    header
}

/// Header with `@HD SO:coordinate` plus a Bismark `@PG` whose CL is `cl`
/// (used to drive AutoDetect's SE/PE classification).
fn coord_header_with_bismark_pg(cl: &str) -> Header {
    let mut hd = Map::<HeaderMap>::new(Version::new(1, 0));
    hd.other_fields_mut()
        .insert(SORT_ORDER, BString::from(SO_COORDINATE.to_vec()));
    let mut prog = Map::<Program>::default();
    prog.other_fields_mut()
        .insert(COMMAND_LINE, BString::from(cl.as_bytes().to_vec()));
    let mut header = Header::builder()
        .set_header(hd)
        .add_program(BString::from("Bismark"), prog)
        .build();
    chr1_ref(&mut header);
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
    BismarkRecord::from_noodles_record(record).expect("valid synthetic BismarkRecord")
}

/// One OT CpG record (XR=CT XG=CT, `Z` at read pos 0 → genomic `start`).
fn ot_cpg(qname: &[u8], start: usize) -> BismarkRecord {
    synth_record(qname, b"CT", b"CT", b"Z....", b"ACGTC", start, 0)
}

/// One OT CpG record with an UNMETHYLATED call (`z`).
fn ot_cpg_unmeth(qname: &[u8], start: usize) -> BismarkRecord {
    synth_record(qname, b"CT", b"CT", b"z....", b"ACGTC", start, 0)
}

fn write_bam(path: &Path, header: Header, records: &[BismarkRecord]) {
    let mut w = BamWriter::from_path(path, header).unwrap();
    for r in records {
        w.write_record(r).unwrap();
    }
    w.finish().unwrap();
}

fn extractor() -> Command {
    Command::cargo_bin("bismark_methylation_extractor").unwrap()
}

fn read_gz(path: &Path) -> Vec<u8> {
    let f = fs::File::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    let mut d = flate2::read::GzDecoder::new(f);
    let mut out = Vec::new();
    d.read_to_end(&mut out).unwrap();
    out
}

/// Content lines of a split file (skips the version-banner first line).
fn split_line_set(path: &Path) -> Vec<String> {
    let content = fs::read_to_string(path).unwrap();
    let mut lines: Vec<String> = content.lines().skip(1).map(str::to_string).collect();
    lines.sort();
    lines
}

// ─────────────────────────────────────────────────────────────────────────
// SE coordinate-sorted acceptance
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn se_coordinate_sorted_accepted_explicit_single_end() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("coord.bam");
    write_bam(
        &bam,
        header_with_so(SO_COORDINATE),
        &[ot_cpg(b"r1", 100), ot_cpg(b"r2", 200), ot_cpg(b"r3", 300)],
    );
    let out = work.path().join("out");

    extractor()
        .arg(&bam)
        .arg("--single-end")
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .success();

    // CpG_OT must carry the 3 OT calls (header + 3 lines).
    let cpg_ot = fs::read_to_string(out.join("CpG_OT_coord.txt")).unwrap();
    assert!(
        cpg_ot.lines().count() >= 4,
        "expected header + 3 OT calls; got:\n{cpg_ot}"
    );
}

#[test]
fn se_coordinate_sorted_aggregated_outputs_order_invariant() {
    // C1: bedGraph/.cov are position-aggregated → byte-identical regardless of
    // input record order. Split files preserve input order → same line SET,
    // not necessarily same bytes.
    let work = tempfile::tempdir().unwrap();

    // Same three records, ascending (coordinate-sorted) vs descending (unsorted).
    let asc = work.path().join("asc.bam");
    write_bam(
        &asc,
        header_with_so(SO_COORDINATE),
        &[ot_cpg(b"r1", 100), ot_cpg(b"r2", 200), ot_cpg(b"r3", 300)],
    );
    let desc = work.path().join("desc.bam");
    write_bam(
        &desc,
        header_with_so(SO_UNSORTED),
        &[ot_cpg(b"r3", 300), ot_cpg(b"r2", 200), ot_cpg(b"r1", 100)],
    );

    let asc_out = work.path().join("asc_out");
    let desc_out = work.path().join("desc_out");
    for (bam, out) in [(&asc, &asc_out), (&desc, &desc_out)] {
        extractor()
            .arg(bam)
            .arg("--single-end")
            .arg("--bedGraph")
            .arg("--output_dir")
            .arg(out)
            .assert()
            .success();
    }

    // bedGraph + coverage are order-invariant → decompressed bytes equal.
    assert_eq!(
        read_gz(&asc_out.join("asc.bedGraph.gz")),
        read_gz(&desc_out.join("desc.bedGraph.gz")),
        "bedGraph must be order-invariant (position-aggregated)"
    );
    assert_eq!(
        read_gz(&asc_out.join("asc.bismark.cov.gz")),
        read_gz(&desc_out.join("desc.bismark.cov.gz")),
        "coverage must be order-invariant"
    );

    // Split files: same line SET across the two orders (input-order-preserving,
    // so the raw byte order differs but the content set is identical).
    assert_eq!(
        split_line_set(&asc_out.join("CpG_OT_asc.txt")),
        split_line_set(&desc_out.join("CpG_OT_desc.txt")),
        "split-file line SET must match across input orderings"
    );
}

#[test]
fn autodetect_se_coordinate_sorted_processed() {
    // No -s/-p: AutoDetect probes the @PG Bismark CL (SE: no -1/-2) on a
    // coordinate-sorted file (probe opens WITHOUT the sort check) → SE → runs.
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("auto_se.bam");
    write_bam(
        &bam,
        coord_header_with_bismark_pg("bismark reads.fq.gz --genome idx --bowtie2"),
        &[ot_cpg(b"r1", 100), ot_cpg(b"r2", 200)],
    );
    let out = work.path().join("out");

    extractor()
        .arg(&bam)
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .success();

    assert!(out.join("CpG_OT_auto_se.txt").exists());
}

// ─────────────────────────────────────────────────────────────────────────
// PE coordinate-sorted rejection (the relaxation must NOT leak to PE)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn pe_coordinate_sorted_rejected_explicit_paired_end() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("coord_pe.bam");
    write_bam(&bam, header_with_so(SO_COORDINATE), &[ot_cpg(b"r1", 100)]);
    let out = work.path().join("out");

    extractor()
        .arg(&bam)
        .arg("--paired-end")
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "input BAM is coordinate-sorted; Bismark downstream tools require \
             name-grouped or unsorted input",
        ));
}

#[test]
fn pe_coordinate_sorted_rejected_autodetect() {
    // AutoDetect on a coordinate-sorted file whose @PG Bismark CL has -1/-2
    // (PE) → detected PE → PE re-open WITH the sort check → UnsortedInput.
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("auto_pe.bam");
    write_bam(
        &bam,
        coord_header_with_bismark_pg("bismark -1 r1.fq.gz -2 r2.fq.gz --genome idx"),
        &[ot_cpg(b"r1", 100)],
    );
    let out = work.path().join("out");

    extractor()
        .arg(&bam)
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .failure()
        .stderr(predicates::str::contains("coordinate-sorted"));
}

// ─────────────────────────────────────────────────────────────────────────
// Multiple files — per-file outputs, no cross-file bleed
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn multifile_bedgraph_per_file_no_bleed() {
    // Two files covering the SAME chromosome + position (the strongest test of
    // aggregator / chr-name-table reset): `a` methylated, `b` unmethylated at
    // chr1:100. Each file's bedGraph must equal running that file ALONE.
    let work = tempfile::tempdir().unwrap();
    let a = work.path().join("a.bam");
    let b = work.path().join("b.bam");
    write_bam(&a, header_with_so(SO_COORDINATE), &[ot_cpg(b"ra", 100)]);
    write_bam(
        &b,
        header_with_so(SO_COORDINATE),
        &[ot_cpg_unmeth(b"rb", 100)],
    );

    // Solo runs (oracle).
    let solo_a = work.path().join("solo_a");
    let solo_b = work.path().join("solo_b");
    for (bam, out) in [(&a, &solo_a), (&b, &solo_b)] {
        extractor()
            .arg(bam)
            .arg("--single-end")
            .arg("--bedGraph")
            .arg("--output_dir")
            .arg(out)
            .assert()
            .success();
    }

    // Batch run: a.bam b.bam together.
    let batch = work.path().join("batch");
    extractor()
        .arg(&a)
        .arg(&b)
        .arg("--single-end")
        .arg("--bedGraph")
        .arg("--output_dir")
        .arg(&batch)
        .assert()
        .success();

    for (name, solo) in [("a", &solo_a), ("b", &solo_b)] {
        assert_eq!(
            read_gz(&batch.join(format!("{name}.bedGraph.gz"))),
            read_gz(&solo.join(format!("{name}.bedGraph.gz"))),
            "{name}.bedGraph in batch must equal running {name} alone (no bleed)"
        );
        assert_eq!(
            read_gz(&batch.join(format!("{name}.bismark.cov.gz"))),
            read_gz(&solo.join(format!("{name}.bismark.cov.gz"))),
            "{name}.bismark.cov in batch must equal running {name} alone"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Fail-fast + missing-file pre-flight
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn multifile_gzip_fail_fast_keeps_first_file_valid() {
    // File 1 valid SE; file 2 has a PAIRED-flag record → SE per-record error.
    // With --gzip: file 1's outputs must be COMPLETE + valid gzip; file 2's
    // partials must be cleaned up.
    let work = tempfile::tempdir().unwrap();
    let good = work.path().join("good.bam");
    let bad = work.path().join("bad.bam");
    write_bam(&good, header_with_so(SO_COORDINATE), &[ot_cpg(b"g1", 100)]);
    write_bam(
        &bad,
        header_with_so(SO_COORDINATE),
        &[synth_record(
            b"paired", b"CT", b"CT", b"Z....", b"ACGTC", 100, 0x41,
        )],
    );
    let out = work.path().join("out");

    extractor()
        .arg(&good)
        .arg(&bad)
        .arg("--single-end")
        .arg("--gzip")
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "paired-end extraction (input has PAIRED flag set)",
        ));

    // File 1's gzipped split file is present AND decompresses cleanly.
    let good_cpg = out.join("CpG_OT_good.txt.gz");
    assert!(
        good_cpg.exists(),
        "good.bam's gzipped output should survive"
    );
    assert!(
        !read_gz(&good_cpg).is_empty(),
        "good.bam's gzip stream must be valid + non-empty"
    );

    // File 2 left no per-context split files behind (cleanup_partial_outputs).
    for ctx in ["CpG", "CHG", "CHH"] {
        for strand in ["OT", "CTOT", "CTOB", "OB"] {
            let p = out.join(format!("{ctx}_{strand}_bad.txt.gz"));
            assert!(
                !p.exists(),
                "bad.bam partial {ctx}_{strand} must be cleaned"
            );
        }
    }
}

#[test]
fn multifile_missing_file_aborts_preflight_zero_outputs() {
    // A missing file in the batch is caught in validate() BEFORE any
    // processing → zero outputs (not even the first file's).
    let work = tempfile::tempdir().unwrap();
    let good = work.path().join("good.bam");
    write_bam(&good, header_with_so(SO_COORDINATE), &[ot_cpg(b"g1", 100)]);
    let missing = work.path().join("does_not_exist.bam");
    let out = work.path().join("out");

    extractor()
        .arg(&good)
        .arg(&missing)
        .arg("--single-end")
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .failure()
        .stderr(predicates::str::contains("input file does not exist"));

    // No output for the good file: validate() rejected before run() looped.
    assert!(
        !out.join("CpG_OT_good.txt").exists(),
        "pre-flight abort must write zero outputs"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// --ucsc over multiple files — per-file UCSC output, no bleed
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn multifile_bedgraph_ucsc_per_file_no_bleed() {
    // `--bedGraph --ucsc` over two files: each input must produce its OWN
    // UCSC-translated bedGraph (`<base>.bedGraph_UCSC.bedGraph.gz`), and that
    // output must be byte-identical to running the file alone (per-file UCSC
    // state, no cross-file bleed).
    let work = tempfile::tempdir().unwrap();
    let a = work.path().join("a.bam");
    let b = work.path().join("b.bam");
    write_bam(&a, header_with_so(SO_COORDINATE), &[ot_cpg(b"ra", 100)]);
    write_bam(&b, header_with_so(SO_COORDINATE), &[ot_cpg(b"rb", 200)]);

    let solo_a = work.path().join("solo_a");
    let solo_b = work.path().join("solo_b");
    for (bam, out) in [(&a, &solo_a), (&b, &solo_b)] {
        extractor()
            .arg(bam)
            .arg("--single-end")
            .arg("--bedGraph")
            .arg("--ucsc")
            .arg("--output_dir")
            .arg(out)
            .assert()
            .success();
    }

    let batch = work.path().join("batch");
    extractor()
        .arg(&a)
        .arg(&b)
        .arg("--single-end")
        .arg("--bedGraph")
        .arg("--ucsc")
        .arg("--output_dir")
        .arg(&batch)
        .assert()
        .success();

    // Inline UCSC output is `<base>.bedGraph_UCSC.bedGraph.gz` (see
    // phase2_inline.rs::ucsc_inline_matches_standalone).
    for (name, solo) in [("a", &solo_a), ("b", &solo_b)] {
        let ucsc = format!("{name}.bedGraph_UCSC.bedGraph.gz");
        assert!(
            batch.join(&ucsc).exists(),
            "per-file UCSC output {ucsc} must exist in the batch run"
        );
        assert_eq!(
            read_gz(&batch.join(&ucsc)),
            read_gz(&solo.join(&ucsc)),
            "{ucsc} in batch must equal running {name} alone (no UCSC-state bleed)"
        );
    }
}

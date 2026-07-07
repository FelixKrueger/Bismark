//! Regression tests for issue #1030 — the methylation extractor must accept
//! Bismark's **non-directional** PE output, where the SAM first/second-in-pair
//! FLAG bits are deliberately swapped for CTOT/CTOB pairs (the first-in-file
//! record, still sequencing Read 1, carries `0x80`). Before the fix,
//! `BismarkPair::from_mates` rejected this and the extractor aborted with
//! `read identity mismatch: expected R1 for first mate, got R2`.
//!
//! These tests exercise the path the pre-existing PE tests miss: their helpers
//! build pairs with idealized `0x41`/`0x81` flags, which Bismark never emits
//! for CTOT/CTOB, so they never replayed the swap. Here we use the **real**
//! swapped flags (147/99 for CTOT, 163/83 for CTOB) and the real `repro.bam`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use bismark::io::{BamWriter, BismarkRecord};
use bstr::BString;
use noodles_core::Position;
use noodles_sam::Header;
use noodles_sam::alignment::RecordBuf;
use noodles_sam::alignment::record::Flags;
use noodles_sam::alignment::record::cigar::Op;
use noodles_sam::alignment::record::cigar::op::Kind;
use noodles_sam::alignment::record::data::field::Tag;
use noodles_sam::alignment::record_buf::data::field::Value;
use noodles_sam::alignment::record_buf::{Cigar, Sequence};
use noodles_sam::header::record::value::Map;
use noodles_sam::header::record::value::map::ReferenceSequence;
use std::num::NonZeroUsize;

// ───────────────────────────── helpers ─────────────────────────────────

fn header_chr1() -> Header {
    let mut header = Header::default();
    header.reference_sequences_mut().insert(
        BString::from("chr1"),
        Map::<ReferenceSequence>::new(NonZeroUsize::try_from(1_000_000).unwrap()),
    );
    header
}

#[allow(clippy::too_many_arguments)]
fn synth(
    xr: &[u8],
    xg: &[u8],
    xm: &[u8],
    start: usize,
    read_len: usize,
    flags: u16,
    qname: &[u8],
) -> BismarkRecord {
    let mut record = RecordBuf::default();
    *record.name_mut() = Some(BString::from(qname.to_vec()));
    *record.flags_mut() = Flags::from(flags);
    *record.reference_sequence_id_mut() = Some(0);
    *record.alignment_start_mut() = Some(Position::try_from(start).unwrap());
    *record.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, read_len)]);
    *record.sequence_mut() = Sequence::from(vec![b'A'; read_len]);
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

fn write_bam(path: &Path, records: &[BismarkRecord]) {
    let mut writer = BamWriter::from_path(path, header_chr1()).unwrap();
    for r in records {
        writer.write_record(r).unwrap();
    }
    writer.finish().unwrap();
}

fn run_extractor(bam: &Path, outdir: &Path, extra: &[&str]) -> assert_cmd::assert::Assert {
    fs::create_dir_all(outdir).unwrap();
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(bam)
        .arg("--paired-end")
        .arg("--output_dir")
        .arg(outdir);
    for a in extra {
        cmd.arg(a);
    }
    cmd.assert()
}

/// Read every regular file in `dir` into a name→bytes map (recursive one
/// level is unnecessary — the extractor writes flat into the output dir).
fn dir_snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut map = BTreeMap::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            let name = entry.file_name().to_string_lossy().into_owned();
            map.insert(name, fs::read(entry.path()).unwrap());
        }
    }
    map
}

fn fixture_repro() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/extractor/nondir_pe_1030.bam")
}

// ───────────────────────────── tests ───────────────────────────────────

/// The headline #1030 regression: the real non-directional `repro.bam` (10
/// CTOT/CTOB pairs, first-in-file FLAG 147/163) must extract without the
/// `read identity mismatch` abort, and produce a splitting report + context
/// output.
#[test]
fn nondir_pe_repro_1030_extracts_without_crash() {
    let work = tempfile::tempdir().unwrap();
    let outdir = work.path().join("out");
    run_extractor(&fixture_repro(), &outdir, &["--comprehensive"]).success();

    let report = fs::read_to_string(outdir.join("nondir_pe_1030_splitting_report.txt"))
        .expect("splitting report produced");
    // 10 pairs in the fixture.
    assert!(
        report.contains("Processed 10 lines in total"),
        "expected 10 pairs processed; got:\n{report}"
    );
    // At least one context output file must exist (CpG/CHG/CHH).
    let snap = dir_snapshot(&outdir);
    assert!(
        snap.keys()
            .any(|k| k.starts_with("CpG_") || k.starts_with("CHG_") || k.starts_with("CHH_")),
        "expected a context output file; got files: {:?}",
        snap.keys().collect::<Vec<_>>()
    );
}

/// `--parallel N` byte-invariance on non-directional input (#1030 reviewer
/// V-gap-2): `parallel.rs` pairs via a separate `from_mates` call site, so the
/// fix must hold there too. Output of `--parallel 1` and `--parallel 4` must be
/// byte-identical.
#[test]
fn nondir_pe_repro_1030_parallel_invariant() {
    let work = tempfile::tempdir().unwrap();
    let out1 = work.path().join("p1");
    let out4 = work.path().join("p4");
    run_extractor(
        &fixture_repro(),
        &out1,
        &["--comprehensive", "--parallel", "1"],
    )
    .success();
    run_extractor(
        &fixture_repro(),
        &out4,
        &["--comprehensive", "--parallel", "4"],
    )
    .success();
    assert_eq!(
        dir_snapshot(&out1),
        dir_snapshot(&out4),
        "--parallel 4 output must be byte-identical to --parallel 1 on non-directional PE"
    );
}

/// Core proof that the fix is **output-neutral w.r.t. the FLAG bits**: an
/// *overlapping* CTOT pair built with the real swapped flags (147/99) must
/// produce byte-identical extractor output to the same logical pair built with
/// the idealized flags (0x41/0x81) the old gate required. This simultaneously
/// exercises `--no_overlap`'s reverse-class (`drop_overlap`) branch on a
/// swapped pair — the calls are routed by `pair_strand` (XR/XG) and bucketed by
/// file order, neither of which the FLAG swap perturbs.
#[test]
fn ctot_overlap_swapped_flags_equal_idealized_flags() {
    // R1 (first-in-file, CTOT) spans 1000..1049; R2 (OT) spans 1030..1079 —
    // overlapping by ~20bp so --no_overlap (default) has something to drop.
    // Distinct XM calls so the output is non-trivial.
    let r1_xm = b"Z....z....Z....z....Z....z....Z....z....Z....z....";
    let r2_xm = b"x..X..h..H..z..Z..x..X..h..H..z..Z..x..X..h..H..z.";

    let build = |r1_flag: u16, r2_flag: u16| -> Vec<BismarkRecord> {
        vec![
            synth(b"GA", b"CT", r1_xm, 1000, r1_xm.len(), r1_flag, b"ovl"),
            synth(b"CT", b"CT", r2_xm, 1030, r2_xm.len(), r2_flag, b"ovl"),
        ]
    };

    let work = tempfile::tempdir().unwrap();

    // Use an identical input basename (`sample.bam`) in two separate dirs so
    // output filenames AND the filename echoed inside the splitting report
    // match — any remaining difference is a real divergence in the calls.
    let swapped_dir = work.path().join("swapped");
    fs::create_dir_all(&swapped_dir).unwrap();
    let swapped_bam = swapped_dir.join("sample.bam");
    write_bam(&swapped_bam, &build(147, 99)); // real Bismark swapped flags
    let swapped_out = swapped_dir.join("out");
    run_extractor(&swapped_bam, &swapped_out, &[]).success();

    let ideal_dir = work.path().join("ideal");
    fs::create_dir_all(&ideal_dir).unwrap();
    let ideal_bam = ideal_dir.join("sample.bam");
    write_bam(&ideal_bam, &build(0x41, 0x81)); // idealized flags the old gate demanded
    let ideal_out = ideal_dir.join("out");
    run_extractor(&ideal_bam, &ideal_out, &[]).success();

    assert_eq!(
        dir_snapshot(&swapped_out),
        dir_snapshot(&ideal_out),
        "swapped-flag CTOT output must equal idealized-flag output (FLAG bits do not affect extraction)"
    );
}

/// Mixed 4-strand single file (#1030 reviewer V-gap-3): OT, OB, CTOT, CTOB
/// pairs interleaved must all extract (directional first=0x40 and swapped
/// first=0x80 pairs coexisting in one BAM).
#[test]
fn mixed_four_strand_extractor_coexist() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("mixed.bam");
    let xm = b"Z....z....";
    let len = xm.len();
    let records = vec![
        // OT: R1 XR=CT XG=CT flag 99, R2 XR=GA XG=CT flag 147
        synth(b"CT", b"CT", xm, 1000, len, 99, b"ot"),
        synth(b"GA", b"CT", xm, 1030, len, 147, b"ot"),
        // OB: R1 XR=CT XG=GA flag 83, R2 XR=GA XG=GA flag 163
        synth(b"CT", b"GA", xm, 2000, len, 83, b"ob"),
        synth(b"GA", b"GA", xm, 2030, len, 163, b"ob"),
        // CTOT (swapped): R1 XR=GA XG=CT flag 147, R2 XR=CT XG=CT flag 99
        synth(b"GA", b"CT", xm, 3000, len, 147, b"ctot"),
        synth(b"CT", b"CT", xm, 3030, len, 99, b"ctot"),
        // CTOB (swapped): R1 XR=GA XG=GA flag 163, R2 XR=CT XG=GA flag 83
        synth(b"GA", b"GA", xm, 4000, len, 163, b"ctob"),
        synth(b"CT", b"GA", xm, 4030, len, 83, b"ctob"),
    ];
    write_bam(&bam, &records);
    let outdir = work.path().join("out");
    run_extractor(&bam, &outdir, &["--comprehensive"]).success();

    let report = fs::read_to_string(outdir.join("mixed_splitting_report.txt")).unwrap();
    assert!(
        report.contains("Processed 4 lines in total"),
        "expected all 4 pairs processed; got:\n{report}"
    );
}

// ── Perl-vs-Rust byte identity: NON-DIRECTIONAL PE (#1030 regression cell) ──
//
// The CI `perl-oracle` job runs this against the in-repo Perl
// `bismark_methylation_extractor` v0.25.1 — the matching extractor cell to the
// dedup one in `bismark-dedup`. Closes the non-directional coverage gap that let
// #1030 ship. Auto-skips locally without perl/samtools; CI sets
// `BISMARK_REQUIRE_PERL=1` to turn a missing tool into a hard failure (#796).

/// Locate the in-repo Perl `bismark_methylation_extractor` and confirm perl +
/// samtools are available. Returns `None` (caller skips) if anything is missing.
fn perl_extractor_script() -> Option<PathBuf> {
    let script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bismark_methylation_extractor");
    if !script.exists() {
        return None;
    }
    let ok = |bin: &str, arg: &str| {
        std::process::Command::new(bin)
            .arg(arg)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if !ok("perl", "-v") || !ok("samtools", "--version") {
        return None;
    }
    Some(script)
}

fn require_perl() -> bool {
    std::env::var("BISMARK_REQUIRE_PERL").as_deref() == Ok("1")
}

fn skip_or_panic(reason: &str) {
    if require_perl() {
        panic!("BISMARK_REQUIRE_PERL=1 but {reason}");
    }
    eprintln!("skipping: {reason}");
}

/// Perl-vs-Rust byte identity of `bismark_methylation_extractor --paired-end
/// --comprehensive` on the real non-directional PE reproducer from #1030.
/// Asserts the CpG/CHG/CHH context files, M-bias, and splitting report are all
/// byte-identical to Perl v0.25.1 (the extractor PE path the swap reaches).
#[test]
fn perl_vs_rust_nondirectional_pe_extractor() {
    let Some(script) = perl_extractor_script() else {
        skip_or_panic("nondir PE extractor oracle: perl/samtools/script not available");
        return;
    };
    let fixture = fixture_repro();
    assert!(fixture.exists(), "fixture missing: {}", fixture.display());

    let work = tempfile::tempdir().unwrap();
    let perl_dir = work.path().join("perl");
    let rust_dir = work.path().join("rust");
    fs::create_dir_all(&perl_dir).unwrap();

    // Perl oracle.
    let perl_out = std::process::Command::new("perl")
        .arg(&script)
        .arg("--paired-end")
        .arg("--comprehensive")
        .arg("--output_dir")
        .arg(&perl_dir)
        .arg(&fixture)
        .output()
        .expect("run perl bismark_methylation_extractor");
    assert!(
        perl_out.status.success(),
        "perl bismark_methylation_extractor failed: {}",
        String::from_utf8_lossy(&perl_out.stderr)
    );

    // Rust.
    run_extractor(&fixture, &rust_dir, &["--comprehensive"]).success();

    // Every primary output byte-identical (these are plain text; the Rust port
    // mirrors Perl's version line, so the splitting report matches too).
    for f in [
        "CpG_context_nondir_pe_1030.txt",
        "CHG_context_nondir_pe_1030.txt",
        "CHH_context_nondir_pe_1030.txt",
        "nondir_pe_1030.M-bias.txt",
        "nondir_pe_1030_splitting_report.txt",
    ] {
        let perl_f =
            fs::read(perl_dir.join(f)).unwrap_or_else(|e| panic!("perl output {f} missing: {e}"));
        let rust_f =
            fs::read(rust_dir.join(f)).unwrap_or_else(|e| panic!("rust output {f} missing: {e}"));
        assert_eq!(
            String::from_utf8_lossy(&perl_f),
            String::from_utf8_lossy(&rust_f),
            "non-directional PE extractor output {f} differs between Perl and Rust (#1030)"
        );
    }
}

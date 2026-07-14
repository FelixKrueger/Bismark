//! End-to-end integration tests for `methylation_consistency_rs`.
//!
//! Each test builds a synthetic Bismark BAM with the raw noodles writer (so we
//! can inject arbitrary records, including the XM-less record needed for the
//! graceful-stop test), runs the binary via `assert_cmd`, then reads the
//! output BAMs back via `bismark_io` and checks the report bytes.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use assert_cmd::Command;
use bstr::BString;
use noodles_core::Position;
use noodles_sam::Header;
use noodles_sam::alignment::RecordBuf;
use noodles_sam::alignment::io::Write as _;
use noodles_sam::alignment::record::Flags;
use noodles_sam::alignment::record::cigar::Op;
use noodles_sam::alignment::record::cigar::op::Kind;
use noodles_sam::alignment::record::data::field::Tag;
use noodles_sam::alignment::record_buf::data::field::Value;
use noodles_sam::alignment::record_buf::{Cigar, QualityScores, Sequence};
use noodles_sam::header::record::value::Map;
use noodles_sam::header::record::value::map::header::Version;
use noodles_sam::header::record::value::map::header::tag::SORT_ORDER;
use noodles_sam::header::record::value::map::program::tag::COMMAND_LINE;
use noodles_sam::header::record::value::map::{Program, ReferenceSequence};
use std::num::NonZeroUsize;
use tempfile::TempDir;

// ── helpers ───────────────────────────────────────────────────────────────

/// Build a header: optional Bismark `@PG` command line (for SE/PE auto-detect)
/// and optional `@HD SO:` sort order. Always has one `@SQ` (`chr1`).
fn header(bismark_cl: Option<&str>, sort_order: Option<&str>) -> Header {
    let mut hd = Map::<noodles_sam::header::record::value::map::Header>::new(Version::new(1, 6));
    if let Some(so) = sort_order {
        hd.other_fields_mut()
            .insert(SORT_ORDER, BString::from(so.as_bytes().to_vec()));
    }
    let mut builder = Header::builder().set_header(hd);
    if let Some(cl) = bismark_cl {
        let mut prog = Map::<Program>::default();
        prog.other_fields_mut()
            .insert(COMMAND_LINE, BString::from(cl.as_bytes().to_vec()));
        builder = builder.add_program(BString::from(b"Bismark".to_vec()), prog);
    }
    let mut h = builder.build();
    h.reference_sequences_mut().insert(
        BString::from(b"chr1".to_vec()),
        Map::<ReferenceSequence>::new(NonZeroUsize::new(100_000).unwrap()),
    );
    h
}

/// A mapped record carrying the given `XM` string (seq length matched to it;
/// XR/XG = CT/CT so `bismark_io` classifies it as a valid OT read).
fn rec(name: &str, flags: u16, pos: usize, xm: &[u8]) -> RecordBuf {
    rec_inner(name, flags, pos, Some(xm), xm.len())
}

/// A mapped record with NO `XM` tag (triggers the graceful-stop path).
fn rec_no_xm(name: &str, flags: u16, pos: usize, seq_len: usize) -> RecordBuf {
    rec_inner(name, flags, pos, None, seq_len)
}

fn rec_inner(name: &str, flags: u16, pos: usize, xm: Option<&[u8]>, seq_len: usize) -> RecordBuf {
    let mut r = RecordBuf::default();
    *r.name_mut() = Some(BString::from(name.as_bytes().to_vec()));
    *r.flags_mut() = Flags::from(flags);
    *r.reference_sequence_id_mut() = Some(0);
    *r.alignment_start_mut() = Some(Position::try_from(pos).unwrap());
    *r.sequence_mut() = Sequence::from(vec![b'A'; seq_len]);
    *r.quality_scores_mut() = QualityScores::from(vec![30u8; seq_len]);
    *r.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, seq_len)]);
    if let Some(x) = xm {
        r.data_mut()
            .insert(Tag::from(*b"XM"), Value::String(BString::from(x.to_vec())));
    }
    r.data_mut().insert(
        Tag::from(*b"XR"),
        Value::String(BString::from(b"CT".to_vec())),
    );
    r.data_mut().insert(
        Tag::from(*b"XG"),
        Value::String(BString::from(b"CT".to_vec())),
    );
    r
}

/// Write `records` to a BAM at `path` via the raw noodles writer.
fn write_bam(path: &Path, header: &Header, records: &[RecordBuf]) {
    let mut w = noodles_bam::io::Writer::new(File::create(path).unwrap());
    w.write_header(header).unwrap();
    for r in records {
        w.write_alignment_record(header, r).unwrap();
    }
    w.try_finish().unwrap();
}

/// Read an output BAM back, returning the record qnames in order.
fn read_names(path: &Path) -> Vec<String> {
    let mut reader =
        bismark::io::BamReader::without_sort_check(BufReader::new(File::open(path).unwrap()))
            .unwrap();
    reader
        .records()
        .map(|r| {
            let rec = r.unwrap();
            String::from_utf8_lossy(AsRef::<[u8]>::as_ref(rec.inner().name().unwrap())).into_owned()
        })
        .collect()
}

/// The 49-hyphen report separator (built so the count can't drift).
fn sep() -> String {
    "-".repeat(49)
}

fn run(dir: &TempDir, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = Command::cargo_bin("methylation_consistency").unwrap();
    cmd.current_dir(dir.path());
    cmd.args(args);
    cmd.assert()
}

#[test]
fn no_args_shows_help() {
    // No input is never a valid run, so a bare invocation renders the tool's help
    // (exit 2) via `cli::help_if_no_args` instead of a terse one-line error.
    Command::cargo_bin("methylation_consistency")
        .unwrap()
        .assert()
        .code(2)
        .stderr(predicates::str::contains(
            "Split a Bismark BAM into three BAMs",
        ));
}

// ── Phase A: single-end ─────────────────────────────────────────────────────

#[test]
fn se_three_way_split_and_byte_exact_report() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    write_bam(
        &input,
        &header(None, Some("unsorted")),
        &[
            rec("meth1", 0, 100, b"ZZZZZZ"),   // 6/6 → all_meth
            rec("meth2", 0, 200, b"ZZZZZZ"),   // all_meth
            rec("unmeth1", 0, 300, b"zzzzzz"), // 0/6 → all_unmeth
            rec("unmeth2", 0, 400, b"zzzzzz"), // all_unmeth
            rec("mixed1", 0, 500, b"ZZZzzz"),  // 3/6 = 50% → mixed
            rec("few1", 0, 600, b"Zz"),        // total 2 < 5 → discarded
        ],
    );

    run(&dir, &["-s", "input.bam"]).success();

    let report = std::fs::read_to_string(dir.path().join("input_consistency_report.txt")).unwrap();
    let expected = format!(
        "Total single-end records     -\t6\n{}\n\
         All methylated    [ >= 90% ] -\t2 (33.33%)\n\
         All unmethylated  [ <= 10% ] -\t2 (33.33%)\n\
         Mixed methylation [ 10-90% ] -\t1 (16.67%)\n\
         Too few CpGs   [min-count 5] -\t1 (16.67%)\n",
        sep()
    );
    assert_eq!(report, expected);

    assert_eq!(
        read_names(&dir.path().join("input_all_meth.bam")),
        vec!["meth1", "meth2"]
    );
    assert_eq!(
        read_names(&dir.path().join("input_all_unmeth.bam")),
        vec!["unmeth1", "unmeth2"]
    );
    assert_eq!(
        read_names(&dir.path().join("input_mixed_meth.bam")),
        vec!["mixed1"]
    );
}

#[test]
fn se_outputs_land_adjacent_to_input_in_nested_dir() {
    let dir = TempDir::new().unwrap();
    let sub = dir.path().join("nested");
    std::fs::create_dir(&sub).unwrap();
    let input = sub.join("sample.bam");
    write_bam(&input, &header(None, None), &[rec("r", 0, 10, b"ZZZZZZ")]);

    // Invoke with an absolute path; outputs must appear in `sub`, not CWD.
    run(&dir, &["-s", input.to_str().unwrap()]).success();

    assert!(sub.join("sample_all_meth.bam").exists());
    assert!(sub.join("sample_consistency_report.txt").exists());
    // CWD (dir.path()) must NOT have received the outputs.
    assert!(!dir.path().join("sample_all_meth.bam").exists());
}

#[test]
fn se_empty_bucket_is_a_valid_empty_bam() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    // All reads fully methylated → all_unmeth and mixed receive zero records.
    write_bam(
        &input,
        &header(None, None),
        &[rec("a", 0, 10, b"ZZZZZZ"), rec("b", 0, 20, b"ZZZZZZ")],
    );
    run(&dir, &["-s", "input.bam"]).success();

    // Empty buckets exist and are readable as valid empty BAMs (zero records),
    // NOT Perl's 0-byte unreadable files (SPEC §5.2 decision).
    let unmeth = dir.path().join("input_all_unmeth.bam");
    let mixed = dir.path().join("input_mixed_meth.bam");
    assert!(unmeth.exists() && std::fs::metadata(&unmeth).unwrap().len() > 0);
    assert_eq!(read_names(&unmeth).len(), 0);
    assert_eq!(read_names(&mixed).len(), 0);
    assert_eq!(
        read_names(&dir.path().join("input_all_meth.bam")),
        vec!["a", "b"]
    );
}

// ── Phase B: paired-end ─────────────────────────────────────────────────────

#[test]
fn pe_three_way_counts_pairs_and_writes_both_mates() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    write_bam(
        &input,
        &header(None, Some("unsorted")),
        &[
            rec("p1", 0x41, 100, b"ZZZ"),
            rec("p1", 0x81, 100, b"ZZZ"), // 6 meth → all_meth
            rec("p2", 0x41, 200, b"zzz"),
            rec("p2", 0x81, 200, b"zzz"), // all_unmeth
            rec("p3", 0x41, 300, b"ZZZ"),
            rec("p3", 0x81, 300, b"zzz"), // 3 meth + 3 unmeth → 50% mixed
        ],
    );

    run(&dir, &["-p", "input.bam"]).success();

    let report = std::fs::read_to_string(dir.path().join("input_consistency_report.txt")).unwrap();
    let expected = format!(
        "Total paired-end records     -\t3\n{}\n\
         All methylated    [ >= 90% ] -\t1 (33.33%)\n\
         All unmethylated  [ <= 10% ] -\t1 (33.33%)\n\
         Mixed methylation [ 10-90% ] -\t1 (33.33%)\n\
         Too few CpGs   [min-count 5] -\t0 (0.00%)\n",
        sep()
    );
    assert_eq!(report, expected);

    // Each populated bucket holds BOTH mates of its pair, R1 then R2.
    assert_eq!(
        read_names(&dir.path().join("input_all_meth.bam")),
        vec!["p1", "p1"]
    );
    assert_eq!(
        read_names(&dir.path().join("input_all_unmeth.bam")),
        vec!["p2", "p2"]
    );
    assert_eq!(
        read_names(&dir.path().join("input_mixed_meth.bam")),
        vec!["p3", "p3"]
    );
}

#[test]
fn auto_detect_pe_from_bismark_pg() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    write_bam(
        &input,
        &header(
            Some("bismark --genome /g -1 r1.fq.gz -2 r2.fq.gz"),
            Some("unsorted"),
        ),
        &[rec("p1", 0x41, 100, b"ZZZ"), rec("p1", 0x81, 100, b"ZZZ")],
    );
    // No -s/-p: must auto-detect PE and produce a paired-end report.
    run(&dir, &["input.bam"]).success();
    let report = std::fs::read_to_string(dir.path().join("input_consistency_report.txt")).unwrap();
    assert!(
        report.starts_with("Total paired-end records     -\t1\n"),
        "got: {report}"
    );
}

#[test]
fn auto_detect_se_when_no_bismark_pg() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    // No Bismark @PG at all → detect returns None → fall through to SE.
    write_bam(&input, &header(None, None), &[rec("r", 0, 100, b"ZZZZZZ")]);
    run(&dir, &["input.bam"]).success();
    let report = std::fs::read_to_string(dir.path().join("input_consistency_report.txt")).unwrap();
    assert!(
        report.starts_with("Total single-end records     -\t1\n"),
        "got: {report}"
    );
}

#[test]
fn pe_mate_name_mismatch_errors() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    write_bam(
        &input,
        &header(None, Some("unsorted")),
        &[rec("a", 0x41, 100, b"ZZZ"), rec("b", 0x81, 100, b"ZZZ")],
    );
    run(&dir, &["-p", "input.bam"])
        .failure()
        .stderr(predicates::str::contains("READ IDs"));
}

#[test]
fn pe_rejects_coordinate_sorted_but_se_accepts_it() {
    // PE + coordinate-sorted → error (the correct guard).
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    write_bam(
        &input,
        &header(None, Some("coordinate")),
        &[rec("p1", 0x41, 100, b"ZZZ"), rec("p1", 0x81, 100, b"ZZZ")],
    );
    run(&dir, &["-p", "input.bam"]).failure();

    // SE + coordinate-sorted → accepted (Perl never sort-checks SE).
    let dir2 = TempDir::new().unwrap();
    let input2 = dir2.path().join("input.bam");
    write_bam(
        &input2,
        &header(None, Some("coordinate")),
        &[rec("r", 0, 100, b"ZZZZZZ")],
    );
    run(&dir2, &["-s", "input.bam"]).success();
}

// ── Phase C: CHH + edge cases ───────────────────────────────────────────────

#[test]
fn chh_mode_filenames_and_label() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    write_bam(
        &input,
        &header(None, None),
        &[rec("a", 0, 10, b"HHHHHH"), rec("b", 0, 20, b"hhhhhh")],
    );
    run(&dir, &["--chh", "-s", "input.bam"]).success();

    // _CHH infix on every output.
    assert!(dir.path().join("input_CHH_all_meth.bam").exists());
    assert!(dir.path().join("input_CHH_all_unmeth.bam").exists());
    assert!(dir.path().join("input_CHH_mixed_meth.bam").exists());
    let report =
        std::fs::read_to_string(dir.path().join("input_CHH_consistency_report.txt")).unwrap();
    assert!(
        report.contains("Too few CHHs   [min-count 5] -\t"),
        "got: {report}"
    );
    assert!(!report.contains("Too few CpGs"));
    // H counts as methylated in CHH mode.
    assert_eq!(
        read_names(&dir.path().join("input_CHH_all_meth.bam")),
        vec!["a"]
    );
    assert_eq!(
        read_names(&dir.path().join("input_CHH_all_unmeth.bam")),
        vec!["b"]
    );
}

#[test]
fn min_count_zero_skips_zero_call_reads_into_no_bucket() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    // One read with zero cytosine calls (all `.`). With -m 0 it is SKIPPED
    // (counted in no bucket), so the grand total is 0 → N/A report.
    write_bam(&input, &header(None, None), &[rec("r", 0, 10, b"......")]);
    run(&dir, &["-m", "0", "-s", "input.bam"]).success();

    let report = std::fs::read_to_string(dir.path().join("input_consistency_report.txt")).unwrap();
    let expected = format!(
        "Total single-end records     -\t0\n{}\n\
         All methylated    [ >= 90% ] -\t0 (N/A%)\n\
         All unmethylated  [ <= 10% ] -\t0 (N/A%)\n\
         Mixed methylation [ 10-90% ] -\t0 (N/A%)\n\
         Too few CpGs   [min-count 0] -\t0 (N/A%)\n",
        sep()
    );
    assert_eq!(report, expected);
}

#[test]
fn empty_input_file_is_skipped_with_no_outputs() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    write_bam(&input, &header(None, None), &[]); // zero records
    run(&dir, &["-s", "input.bam"]).success();
    // Perl `bam_isEmpty`: skipped before any output is created.
    assert!(!dir.path().join("input_all_meth.bam").exists());
    assert!(!dir.path().join("input_consistency_report.txt").exists());
}

#[test]
fn missing_xm_is_a_graceful_stop_with_partial_report() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    // Record 1 is valid (→ all_meth); record 2 has NO XM tag → graceful stop.
    write_bam(
        &input,
        &header(None, None),
        &[rec("ok", 0, 10, b"ZZZZZ"), rec_no_xm("noxm", 0, 20, 5)],
    );
    // Exit 0 (graceful), report tallies only record 1.
    run(&dir, &["-s", "input.bam"]).success();

    let report = std::fs::read_to_string(dir.path().join("input_consistency_report.txt")).unwrap();
    assert!(
        report.starts_with("Total single-end records     -\t1\n"),
        "got: {report}"
    );
    assert!(
        report.contains("All methylated    [ >= 90% ] -\t1 (100.00%)\n"),
        "got: {report}"
    );
    // All three output BAMs are valid/decodable.
    assert_eq!(
        read_names(&dir.path().join("input_all_meth.bam")),
        vec!["ok"]
    );
    assert_eq!(
        read_names(&dir.path().join("input_all_unmeth.bam")).len(),
        0
    );
}

#[test]
fn version_flag_prints_provenance() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["--version"])
        .success()
        .stdout(predicates::str::starts_with(
            "methylation_consistency (Bismark Rust suite) v",
        ));
}

// ── Perl-vs-Rust byte identity (the §7 gate; auto-skips if tooling absent) ──

/// Locate the Perl `methylation_consistency` at the repo root and confirm
/// `perl` + `samtools` are on PATH. Returns the script path, or `None` (the
/// caller then skips) if anything is missing — so this test runs locally /
/// on colossal where the tooling exists, and is a graceful no-op elsewhere.
fn perl_script() -> Option<std::path::PathBuf> {
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../methylation_consistency");
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

/// CI sets `BISMARK_REQUIRE_PERL=1` so a "missing tool" turns the silent skip
/// into a hard failure — the point of issue #796. `== "1"` (not `is_some`) so an
/// accidental empty export doesn't trip local dev.
fn require_perl() -> bool {
    std::env::var("BISMARK_REQUIRE_PERL").as_deref() == Ok("1")
}

/// Skip the oracle (local dev without Perl/samtools) or panic (CI, where a
/// missing tool means the byte-identity check would silently not run — #796).
fn skip_or_panic(reason: &str) {
    if require_perl() {
        panic!("BISMARK_REQUIRE_PERL=1 but {reason}");
    }
    eprintln!("skipping: {reason}");
}

/// One record line per output record (no header — so the samtools `@PG`
/// provenance lines that Perl injects but the Rust port omits are excluded),
/// with optional tags compared as an order-independent set (SPEC §7).
fn samtools_record_set(path: &Path) -> Vec<String> {
    let out = std::process::Command::new("samtools")
        .arg("view")
        .arg(path)
        .output()
        .expect("samtools view");
    assert!(
        out.status.success(),
        "samtools view {} failed: {}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|line| {
            let mut fields: Vec<&str> = line.split('\t').collect();
            // Fields 0..11 are positional; 11.. are optional tags → sort them
            // so tag *order* differences (semantically irrelevant) don't fail.
            if fields.len() > 11 {
                fields[11..].sort_unstable();
            }
            fields.join("\t")
        })
        .collect()
}

/// Run both Perl and Rust on the same synthetic BAM in `mode` (`-s`/`-p`,
/// plus optional `--chh`), then assert the report is byte-identical and each
/// populated bucket's records match (header `@PG` provenance excluded).
fn assert_perl_rust_identical(records: &[RecordBuf], so: Option<&str>, extra_args: &[&str]) {
    let Some(script) = perl_script() else {
        skip_or_panic("Perl-vs-Rust byte-identity: perl/samtools/script not available");
        return;
    };

    let chh = extra_args.contains(&"--chh");
    let infix = if chh { "_CHH" } else { "" };

    // Two sibling dirs so the two runs never clobber each other.
    let root = TempDir::new().unwrap();
    let perl_dir = root.path().join("perl");
    let rust_dir = root.path().join("rust");
    std::fs::create_dir(&perl_dir).unwrap();
    std::fs::create_dir(&rust_dir).unwrap();
    let h = header(None, so);
    write_bam(&perl_dir.join("in.bam"), &h, records);
    write_bam(&rust_dir.join("in.bam"), &h, records);

    // Perl run.
    let mut perl = std::process::Command::new("perl");
    perl.arg(&script)
        .args(extra_args)
        .arg(perl_dir.join("in.bam"));
    let perl_out = perl.output().expect("run perl methylation_consistency");
    assert!(
        perl_out.status.success(),
        "perl failed: {}",
        String::from_utf8_lossy(&perl_out.stderr)
    );

    // Rust run.
    let mut rust = Command::cargo_bin("methylation_consistency").unwrap();
    rust.args(extra_args).arg(rust_dir.join("in.bam"));
    rust.assert().success();

    // 1) Report byte-identical.
    let perl_report =
        std::fs::read(perl_dir.join(format!("in{infix}_consistency_report.txt"))).unwrap();
    let rust_report =
        std::fs::read(rust_dir.join(format!("in{infix}_consistency_report.txt"))).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&perl_report),
        String::from_utf8_lossy(&rust_report),
        "consistency_report.txt differs"
    );

    // 2) Per-bucket records identical (skip empty buckets: Perl writes a
    //    0-byte unreadable file there, Rust a valid empty BAM — both 0 records).
    for bucket in ["all_meth", "all_unmeth", "mixed_meth"] {
        let perl_bam = perl_dir.join(format!("in{infix}_{bucket}.bam"));
        let rust_bam = rust_dir.join(format!("in{infix}_{bucket}.bam"));
        let perl_empty = std::fs::metadata(&perl_bam)
            .map(|m| m.len() == 0)
            .unwrap_or(true);
        if perl_empty {
            // Rust side must also hold zero records (valid empty BAM).
            assert_eq!(read_names(&rust_bam).len(), 0, "{bucket}: rust not empty");
            continue;
        }
        assert_eq!(
            samtools_record_set(&perl_bam),
            samtools_record_set(&rust_bam),
            "{bucket}: records differ between Perl and Rust"
        );
    }
}

#[test]
fn perl_vs_rust_se_three_way() {
    assert_perl_rust_identical(
        &[
            rec("meth1", 0, 100, b"ZZZZZZ"),
            rec("unmeth1", 0, 200, b"zzzzzz"),
            rec("mixed1", 0, 300, b"ZZZzzz"),
            rec("few1", 0, 400, b"Zz"),
        ],
        Some("unsorted"),
        &["-s"],
    );
}

#[test]
fn perl_vs_rust_pe_three_way() {
    assert_perl_rust_identical(
        &[
            rec("p1", 0x41, 100, b"ZZZ"),
            rec("p1", 0x81, 100, b"ZZZ"),
            rec("p2", 0x41, 200, b"zzz"),
            rec("p2", 0x81, 200, b"zzz"),
            rec("p3", 0x41, 300, b"ZZZ"),
            rec("p3", 0x81, 300, b"zzz"),
        ],
        Some("unsorted"),
        &["-p"],
    );
}

#[test]
fn perl_vs_rust_chh_se() {
    assert_perl_rust_identical(
        &[
            rec("a", 0, 100, b"HHHHHH"),
            rec("b", 0, 200, b"hhhhhh"),
            rec("c", 0, 300, b"HHHhhh"),
        ],
        Some("unsorted"),
        &["--chh", "-s"],
    );
}

// ── coverage additions from the post-implementation audit (2026-05-29) ──────

#[test]
fn multiple_input_files_each_get_own_outputs() {
    // Perl processes each positional file independently with its own outputs.
    let dir = TempDir::new().unwrap();
    write_bam(
        &dir.path().join("a.bam"),
        &header(None, None),
        &[rec("a1", 0, 10, b"ZZZZZZ")],
    );
    write_bam(
        &dir.path().join("b.bam"),
        &header(None, None),
        &[rec("b1", 0, 10, b"zzzzzz")],
    );
    run(&dir, &["-s", "a.bam", "b.bam"]).success();

    // Each file gets its own report + buckets.
    assert!(dir.path().join("a_consistency_report.txt").exists());
    assert!(dir.path().join("b_consistency_report.txt").exists());
    assert_eq!(read_names(&dir.path().join("a_all_meth.bam")), vec!["a1"]);
    assert_eq!(read_names(&dir.path().join("b_all_unmeth.bam")), vec!["b1"]);
}

#[test]
fn pe_odd_trailing_r1_is_dropped_uncounted() {
    // Three records: one full pair + a dangling R1 with no following mate.
    // Perl's `$_ = <IN>` returns undef → `last`: the dangling R1 is dropped,
    // uncounted. The report total is therefore 1 (the single pair).
    let dir = TempDir::new().unwrap();
    write_bam(
        &dir.path().join("input.bam"),
        &header(None, Some("unsorted")),
        &[
            rec("p1", 0x41, 100, b"ZZZ"),
            rec("p1", 0x81, 100, b"ZZZ"),
            rec("dangling", 0x41, 200, b"ZZZ"), // no R2 → dropped
        ],
    );
    run(&dir, &["-p", "input.bam"]).success();

    let report = std::fs::read_to_string(dir.path().join("input_consistency_report.txt")).unwrap();
    assert!(
        report.starts_with("Total paired-end records     -\t1\n"),
        "dangling R1 must be dropped + uncounted; got: {report}"
    );
    // Only the one pair's mates are written; the dangling R1 is not.
    assert_eq!(
        read_names(&dir.path().join("input_all_meth.bam")),
        vec!["p1", "p1"]
    );
}

#[test]
fn malformed_record_missing_xr_is_fatal() {
    // Documented strictness divergence (SPEC §4.1): a record missing XR/XG
    // (which Perl would happily process — it reads only XM) is FATAL in the
    // Rust port (bismark-io's BismarkRecord requires XR/XG). Never triggers
    // on genuine Bismark BAMs. Pin the behavior so it can't change silently.
    let dir = TempDir::new().unwrap();
    let mut bad = RecordBuf::default();
    *bad.name_mut() = Some(BString::from(b"bad".to_vec()));
    *bad.flags_mut() = Flags::from(0);
    *bad.reference_sequence_id_mut() = Some(0);
    *bad.alignment_start_mut() = Some(Position::try_from(20).unwrap());
    *bad.sequence_mut() = Sequence::from(vec![b'A'; 5]);
    *bad.quality_scores_mut() = QualityScores::from(vec![30u8; 5]);
    *bad.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, 5)]);
    bad.data_mut().insert(
        Tag::from(*b"XM"),
        Value::String(BString::from(b"ZZZZZ".to_vec())),
    );
    bad.data_mut().insert(
        Tag::from(*b"XG"),
        Value::String(BString::from(b"CT".to_vec())),
    );
    // NB: NO XR tag.
    write_bam(
        &dir.path().join("input.bam"),
        &header(None, None),
        &[rec("ok", 0, 10, b"ZZZZZ"), bad],
    );
    // The second record's missing XR aborts the file with a nonzero exit.
    run(&dir, &["-s", "input.bam"]).failure();
}

#[test]
fn truncated_bam_is_fatal() {
    // A truncated BAM (BGZF stream cut mid-block) must fail, not silently
    // produce partial output. Perl's `bam_isTruncated` dies; the Rust port
    // surfaces the reader's I/O error as a fatal nonzero exit.
    let dir = TempDir::new().unwrap();
    let good = dir.path().join("good.bam");
    write_bam(
        &good,
        &header(None, None),
        &[
            rec("r1", 0, 10, b"ZZZZZZ"),
            rec("r2", 0, 20, b"zzzzzz"),
            rec("r3", 0, 30, b"ZZZzzz"),
        ],
    );
    let bytes = std::fs::read(&good).unwrap();
    let truncated = dir.path().join("truncated.bam");
    // Keep only the first half → cuts into the BGZF/BAM stream.
    std::fs::write(&truncated, &bytes[..bytes.len() / 2]).unwrap();

    run(&dir, &["-s", "truncated.bam"]).failure();
}

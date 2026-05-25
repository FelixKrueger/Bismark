//! Integration tests for `deduplicate_bismark_rs` end-to-end.
//!
//! Each test:
//! 1. Constructs a synthetic Bismark BAM at a temp path via `bismark-io`'s
//!    `BamWriter` (records hand-built with the right strand tags, CIGAR,
//!    flags, etc.).
//! 2. Spawns the `deduplicate_bismark_rs` binary via `assert_cmd` with
//!    the appropriate CLI flags.
//! 3. Reads the deduplicated output BAM back via `bismark-io::open_reader`
//!    and asserts:
//!    - The set of retained-read qnames (the byte-identity invariant per
//!      `PLAN.md` §9 assumption #1).
//!    - PE: R1-followed-by-R2 adjacency in the output.
//!    - The `.deduplication_report.txt` byte content.
//!
//! This is the first phase where bismark-dedup runs end-to-end against
//! real BAM bytes. See `PLAN.md` §6 Phase E.

use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

use assert_cmd::Command;
use bismark_io::BamWriter;
use bismark_io::BismarkRecord;
use bstr::BString;
use noodles_core::Position;
use noodles_sam::Header;
use noodles_sam::alignment::RecordBuf;
use noodles_sam::alignment::record::Flags;
use noodles_sam::alignment::record::cigar::Op;
use noodles_sam::alignment::record::cigar::op::Kind;
use noodles_sam::alignment::record::data::field::Tag;
use noodles_sam::alignment::record_buf::Cigar;
use noodles_sam::alignment::record_buf::QualityScores;
use noodles_sam::alignment::record_buf::Sequence;
use noodles_sam::alignment::record_buf::data::field::Value;
use noodles_sam::header::record::value::Map;
use noodles_sam::header::record::value::map::ReferenceSequence;
use predicates::prelude::*;
use std::num::NonZeroUsize;
use tempfile::TempDir;

// ───────────────────────────── helpers ─────────────────────────────────

fn synth_header() -> Header {
    let mut header = Header::default();
    header.reference_sequences_mut().insert(
        BString::from("chr1"),
        Map::<ReferenceSequence>::new(NonZeroUsize::try_from(1_000_000).unwrap()),
    );
    header
}

/// Build a single-record `RecordBuf` with the given strand tags, CIGAR,
/// flags, and qname. Sequence and qualities have length matching the CIGAR
/// read-span; XM is a "no methylation calls" placeholder of the same length.
fn build_record(
    qname: &str,
    xr: &[u8],
    xg: &[u8],
    flags: u16,
    refid: usize,
    start: usize,
    read_len: usize,
) -> RecordBuf {
    let mut record = RecordBuf::default();
    *record.name_mut() = Some(BString::from(qname.as_bytes().to_vec()));
    *record.flags_mut() = Flags::from(flags);
    *record.reference_sequence_id_mut() = Some(refid);
    *record.alignment_start_mut() = Some(Position::try_from(start).unwrap());
    *record.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, read_len)]);
    *record.sequence_mut() = Sequence::from(vec![b'A'; read_len]);
    *record.quality_scores_mut() = QualityScores::from(vec![30u8; read_len]);
    let xm = vec![b'.'; read_len];
    record
        .data_mut()
        .insert(Tag::from(*b"XR"), Value::String(BString::from(xr.to_vec())));
    record
        .data_mut()
        .insert(Tag::from(*b"XG"), Value::String(BString::from(xg.to_vec())));
    record
        .data_mut()
        .insert(Tag::from(*b"XM"), Value::String(BString::from(xm)));
    record
}

/// Write a sequence of `RecordBuf`s to a BAM file at `path` with a synthetic
/// header.
fn write_bam(path: &Path, records: &[RecordBuf]) {
    let header = synth_header();
    let mut writer = BamWriter::from_path(path, header).unwrap();
    for record in records {
        let bismark_record = BismarkRecord::from_noodles_record(record.clone()).unwrap();
        writer.write_record(&bismark_record).unwrap();
    }
    writer.finish().unwrap();
}

/// Read all qnames from a BAM and return them as a sorted Vec of unique
/// values. For PE input, each pair contributes ONE qname (R1 and R2 share
/// the qname); for SE input, each record contributes one. Deduplication
/// via HashSet ensures the caller sees the *set* of retained reads/pairs.
fn read_qnames(path: &Path) -> Vec<String> {
    let mut reader = bismark_io::open_reader(path, None).unwrap();
    let qnames: HashSet<String> = reader
        .records()
        .map(|r| {
            let record = r.unwrap();
            String::from_utf8_lossy(AsRef::as_ref(record.inner().name().unwrap())).into_owned()
        })
        .collect();
    let mut sorted: Vec<String> = qnames.into_iter().collect();
    sorted.sort();
    sorted
}

/// Read all records from a BAM and return them as Vec<BismarkRecord> in
/// input order (no sorting).
fn read_records(path: &Path) -> Vec<BismarkRecord> {
    let mut reader = bismark_io::open_reader(path, None).unwrap();
    reader.records().map(|r| r.unwrap()).collect()
}

fn qname_of(record: &BismarkRecord) -> String {
    String::from_utf8_lossy(AsRef::as_ref(record.inner().name().unwrap())).into_owned()
}

/// Construct a PE pair: R1 (flags 0x41 = paired + first-in-pair) at
/// `r1_start`, R2 (flags 0x81 = paired + second-in-pair) at `r2_start`.
/// For an OT pair: R1 has XR=CT XG=CT, R2 has XR=GA XG=CT.
fn ot_pair(qname: &str, r1_start: usize, r2_start: usize) -> [RecordBuf; 2] {
    [
        build_record(qname, b"CT", b"CT", 0x41, 0, r1_start, 50),
        build_record(qname, b"GA", b"CT", 0x81, 0, r2_start, 50),
    ]
}

/// CTOT pair (non-directional library): R1 has XR=GA XG=CT (→ CTOT
/// per-record strand and CTOT pair-strand since R1-derived).
fn ctot_pair(qname: &str, r1_start: usize, r2_start: usize) -> [RecordBuf; 2] {
    [
        build_record(qname, b"GA", b"CT", 0x41, 0, r1_start, 50),
        build_record(qname, b"CT", b"CT", 0x81, 0, r2_start, 50),
    ]
}

/// Single-end OT record (no PE flag bits set).
fn se_ot(qname: &str, start: usize) -> RecordBuf {
    build_record(qname, b"CT", b"CT", 0, 0, start, 50)
}

/// Generate a high-entropy sequence (length `len`) seeded by `seed`. The
/// `--parallel N` equivalence tests need fixtures large enough to span
/// multiple BGZF blocks — uniform `'A'`s compress so heavily that several
/// hundred records still fit in a single ~64 KB block. Using an LCG-driven
/// ACGT stream defeats that and forces the BAM to span ≥3 blocks at the
/// chosen record counts.
fn varied_seq(seed: u64, len: usize) -> Vec<u8> {
    let bases = [b'A', b'C', b'G', b'T'];
    let mut state = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(0xBF58_476D_1CE4_E5B9);
    (0..len)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            bases[((state >> 33) & 3) as usize]
        })
        .collect()
}

/// Companion to [`varied_seq`] — generate a varied XM call string. Uses
/// the seven canonical Bismark XM symbols (`.zZxXhH`) so the resulting
/// byte stream has real entropy and resists BGZF dictionary compression.
fn varied_xm(seed: u64, len: usize) -> Vec<u8> {
    let symbols = [b'.', b'z', b'Z', b'x', b'X', b'h', b'H'];
    let mut state = seed
        .wrapping_mul(0xD1B5_4A32_D192_ED03)
        .wrapping_add(0x94D0_49BB_1331_11EB);
    (0..len)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            symbols[((state >> 33) % 7) as usize]
        })
        .collect()
}

/// PE pair with varied (high-entropy) bases + XM string, seeded by `seed`.
/// Used by the `--parallel N` equivalence tests to force the synthetic
/// BAM to span multiple BGZF blocks; the standard [`ot_pair`] uses a
/// uniform `'A'`-only sequence that compresses to a tiny fraction of one
/// block at any record count and would mask threading-order bugs.
fn ot_pair_varied(qname: &str, r1_start: usize, r2_start: usize, seed: u64) -> [RecordBuf; 2] {
    [
        build_record_varied(qname, b"CT", b"CT", 0x41, 0, r1_start, 100, seed),
        build_record_varied(
            qname,
            b"GA",
            b"CT",
            0x81,
            0,
            r2_start,
            100,
            seed.wrapping_add(1),
        ),
    ]
}

/// SE OT record with varied bases + XM. See [`ot_pair_varied`] for rationale.
fn se_ot_varied(qname: &str, start: usize, seed: u64) -> RecordBuf {
    build_record_varied(qname, b"CT", b"CT", 0, 0, start, 100, seed)
}

/// Varied-base counterpart of [`build_record`]. The 8-arg signature
/// mirrors `build_record` plus a per-record `seed` — refactoring into a
/// struct would obscure call-sites that are already wide.
#[allow(clippy::too_many_arguments)]
fn build_record_varied(
    qname: &str,
    xr: &[u8],
    xg: &[u8],
    flags: u16,
    refid: usize,
    start: usize,
    read_len: usize,
    seed: u64,
) -> RecordBuf {
    let mut record = RecordBuf::default();
    *record.name_mut() = Some(BString::from(qname.as_bytes().to_vec()));
    *record.flags_mut() = Flags::from(flags);
    *record.reference_sequence_id_mut() = Some(refid);
    *record.alignment_start_mut() = Some(Position::try_from(start).unwrap());
    *record.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, read_len)]);
    *record.sequence_mut() = Sequence::from(varied_seq(seed, read_len));
    *record.quality_scores_mut() = QualityScores::from(vec![30u8; read_len]);
    record
        .data_mut()
        .insert(Tag::from(*b"XR"), Value::String(BString::from(xr.to_vec())));
    record
        .data_mut()
        .insert(Tag::from(*b"XG"), Value::String(BString::from(xg.to_vec())));
    record.data_mut().insert(
        Tag::from(*b"XM"),
        Value::String(BString::from(varied_xm(seed ^ 0xDEAD_BEEF, read_len))),
    );
    record
}

// ───────────────────────────── tests ───────────────────────────────────

/// PE dedup smoke test: 5 unique pairs + 2 duplicate pairs (same chr/start/end
/// as two of the originals, but with distinct qnames). After dedup:
/// the FIRST occurrence at each (strand, chr, start, end) tuple wins;
/// 2 records are flagged as duplicates, leaving 5 pairs in the output.
#[test]
fn pe_dedup_retains_first_occurrence_and_removes_subsequent_duplicates() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");

    let mut records = Vec::new();
    // 5 unique OT pairs at different positions
    for i in 0..5 {
        records.extend(ot_pair(
            &format!("unique_{i}"),
            1000 + i * 1000,
            1100 + i * 1000,
        ));
    }
    // 2 duplicate pairs: same start positions as unique_0 and unique_1
    // → same dedup key, despite different qname.
    records.extend(ot_pair("dup_of_0", 1000, 1100));
    records.extend(ot_pair("dup_of_1", 2000, 2100));

    write_bam(&input, &records);

    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.arg("--paired")
        .arg("--output_dir")
        .arg(dir.path())
        .arg(&input)
        .assert()
        .success();

    let out_path = dir.path().join("input.deduplicated.bam");
    assert!(
        out_path.exists(),
        "expected output BAM at {}",
        out_path.display()
    );

    let retained_qnames = read_qnames(&out_path);
    let expected: Vec<String> = (0..5).map(|i| format!("unique_{i}")).collect();
    let mut expected_sorted = expected.clone();
    expected_sorted.sort();
    assert_eq!(
        retained_qnames, expected_sorted,
        "exactly the 5 unique pairs should be retained; \
         dup_of_0 and dup_of_1 should have been removed (they share \
         dedup keys with unique_0/unique_1)"
    );
}

/// PE dedup output preserves R1-then-R2 adjacency for every retained pair.
/// This is the SAM/BAM convention `bismark-dedup` must uphold per
/// `PLAN.md` §10.2.
#[test]
fn pe_dedup_output_preserves_r1_followed_by_r2_adjacency() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    let mut records = Vec::new();
    for i in 0..3 {
        records.extend(ot_pair(
            &format!("read_{i}"),
            1000 + i * 1000,
            1100 + i * 1000,
        ));
    }
    write_bam(&input, &records);

    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.arg("--paired")
        .arg("--output_dir")
        .arg(dir.path())
        .arg(&input)
        .assert()
        .success();

    let out_records = read_records(&dir.path().join("input.deduplicated.bam"));
    assert_eq!(out_records.len(), 6, "3 pairs × 2 records = 6");
    // R1/R2 adjacency: even indices are R1 (flags & 0x40), odd are R2 (flags & 0x80).
    for (i, record) in out_records.iter().enumerate() {
        let flags = u16::from(record.inner().flags());
        if i % 2 == 0 {
            assert!(flags & 0x40 != 0, "record {i} must be R1 (flag 0x40)");
        } else {
            assert!(flags & 0x80 != 0, "record {i} must be R2 (flag 0x80)");
        }
    }
    // Consecutive pairs share qnames.
    for i in (0..out_records.len()).step_by(2) {
        assert_eq!(
            qname_of(&out_records[i]),
            qname_of(&out_records[i + 1]),
            "pair {} qnames disagree",
            i / 2
        );
    }
}

/// SE dedup smoke test: 3 unique reads + 2 duplicates. Verifies the SE
/// branch of `compute_se_key` end-to-end through the binary.
#[test]
fn se_dedup_retains_first_occurrence_and_removes_subsequent_duplicates() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");

    // Duplicates: same (strand, chr, start) but distinct qnames.
    let records = vec![
        se_ot("se_unique_0", 1000),
        se_ot("se_unique_1", 2000),
        se_ot("se_unique_2", 3000),
        se_ot("se_dup_of_0", 1000),
        se_ot("se_dup_of_2", 3000),
    ];

    write_bam(&input, &records);

    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.arg("--single")
        .arg("--output_dir")
        .arg(dir.path())
        .arg(&input)
        .assert()
        .success();

    let out_path = dir.path().join("input.deduplicated.bam");
    let retained = read_qnames(&out_path);
    let mut expected = vec![
        "se_unique_0".to_string(),
        "se_unique_1".to_string(),
        "se_unique_2".to_string(),
    ];
    expected.sort();
    assert_eq!(retained, expected);
}

/// Non-directional library: a CTOT pair (R1 XR=GA XG=CT → CTOT pair-strand)
/// dedups correctly. This is the only non-OT/OB path through `compute_pe_key`,
/// and `bismark-io`'s existing test fixture is directional-only.
#[test]
fn ctot_pair_non_directional_dedup_works_end_to_end() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");

    let mut records = Vec::new();
    // One CTOT pair at one position
    records.extend(ctot_pair("ctot_unique", 1000, 1100));
    // One duplicate at the same position
    records.extend(ctot_pair("ctot_dup", 1000, 1100));

    write_bam(&input, &records);

    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.arg("--paired")
        .arg("--output_dir")
        .arg(dir.path())
        .arg(&input)
        .assert()
        .success();

    let retained = read_qnames(&dir.path().join("input.deduplicated.bam"));
    assert_eq!(retained, vec!["ctot_unique".to_string()]);
}

/// Dedup report bytes: exact match against the Perl format.
/// PLAN.md §10.2 calls for byte-equality of the report file.
#[test]
fn pe_dedup_report_bytes_match_perl_format() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");

    let mut records = Vec::new();
    // 4 unique pairs + 1 duplicate.
    for i in 0..4 {
        records.extend(ot_pair(&format!("u{i}"), 1000 + i * 1000, 1100 + i * 1000));
    }
    records.extend(ot_pair("dup", 1000, 1100));
    write_bam(&input, &records);

    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.arg("--paired")
        .arg("--output_dir")
        .arg(dir.path())
        .arg(&input)
        .assert()
        .success();

    let report =
        std::fs::read_to_string(dir.path().join("input.deduplication_report.txt")).unwrap();

    // count = 5 (pairs), removed = 1, leftover = 4 → 1/5 = 20.00%, 4/5 = 80.00%.
    // n_positions = 1 (one composite seen twice).
    let input_path_str = input.display().to_string();
    let expected = format!(
        "\nTotal number of alignments analysed in {input_path_str}:\t5\n\
         Total number duplicated alignments removed:\t1 (20.00%)\n\
         Duplicated alignments were found at:\t1 different position(s)\n\n\
         Total count of deduplicated leftover sequences: 4 (80.00% of total)\n\n"
    );
    assert_eq!(report, expected, "report bytes diverged from Perl format");
}

/// `--outfile /tmp/sample.bam` (path-prefixed user outfile) must produce
/// `<output_dir>/sample.deduplicated.bam` — basename-stripped per Perl's
/// `s/.*\///` regex (lines 145/225/576). Closes plan §10.12.
#[test]
fn outfile_with_directory_prefix_strips_path_per_perl_regex() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    write_bam(&input, &ot_pair("u0", 1000, 1100));

    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.arg("--paired")
        .arg("--output_dir")
        .arg(dir.path())
        .arg("--outfile")
        .arg("/should/be/stripped/sample.bam")
        .arg(&input)
        .assert()
        .success();

    // Output filename uses the basename-stripped stem `sample`, not the
    // full path.
    let expected_out = dir.path().join("sample.deduplicated.bam");
    assert!(
        expected_out.exists(),
        "expected basename-stripped output at {}, got dir contents: {:?}",
        expected_out.display(),
        std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect::<Vec<_>>()
    );
    // And no `sample.deduplicated.bam` should exist with the prefix path
    // (note: Path::join with an absolute argument would discard the dir
    // prefix — we check the literal filesystem path the wrongly-stripped
    // logic would have produced).
    let wrongly_prefixed = PathBuf::from("/should/be/stripped/sample.deduplicated.bam");
    assert!(
        !wrongly_prefixed.exists(),
        "path prefix should have been stripped — the absolute path \
         {} should not exist (and certainly not as the dedup output)",
        wrongly_prefixed.display()
    );
}

/// `--multiple` mode: two input files, accumulating into one combined
/// dedup state. Across-file duplicates are detected.
#[test]
fn multiple_mode_accumulates_dedup_state_across_inputs() {
    let dir = TempDir::new().unwrap();
    let input1 = dir.path().join("file1.bam");
    let input2 = dir.path().join("file2.bam");

    // file1: 3 unique pairs
    let mut f1 = Vec::new();
    for i in 0..3 {
        f1.extend(ot_pair(
            &format!("f1_u{i}"),
            1000 + i * 1000,
            1100 + i * 1000,
        ));
    }
    write_bam(&input1, &f1);

    // file2: 2 unique pairs + 1 pair duplicating file1's pair 0
    let mut f2 = Vec::new();
    for i in 0..2 {
        f2.extend(ot_pair(
            &format!("f2_u{i}"),
            5000 + i * 1000,
            5100 + i * 1000,
        ));
    }
    f2.extend(ot_pair("f2_dup_of_f1_u0", 1000, 1100));
    write_bam(&input2, &f2);

    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.arg("--multiple")
        .arg("--paired")
        .arg("--output_dir")
        .arg(dir.path())
        .arg(&input1)
        .arg(&input2)
        .assert()
        .success();

    let out_path = dir.path().join("file1.multiple.deduplicated.bam");
    assert!(
        out_path.exists(),
        "expected `.multiple.` output at {}",
        out_path.display()
    );

    let retained_set: HashSet<String> = read_qnames(&out_path).into_iter().collect();
    let expected: HashSet<String> = ["f1_u0", "f1_u1", "f1_u2", "f2_u0", "f2_u1"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        retained_set, expected,
        "f2_dup_of_f1_u0 should have been detected as a cross-file duplicate"
    );
}

/// Empty input (header-only BAM, zero records) → `EmptyInput` error AND
/// no output BAM or report file should be created.
#[test]
fn empty_input_errors_before_any_output_file_is_created() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("empty.bam");
    write_bam(&input, &[]); // header only, no records

    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.arg("--paired")
        .arg("--output_dir")
        .arg(dir.path())
        .arg(&input)
        .assert()
        .failure()
        .stderr(predicates::str::contains("input file is empty"));

    // Verify no output BAM or report was created.
    let out_bam = dir.path().join("empty.deduplicated.bam");
    let out_report = dir.path().join("empty.deduplication_report.txt");
    assert!(
        !out_bam.exists(),
        "EmptyInput error should leave no output BAM behind, found: {}",
        out_bam.display()
    );
    assert!(
        !out_report.exists(),
        "EmptyInput error should leave no report behind, found: {}",
        out_report.display()
    );
}

/// `--multiple` with cross-file `@SQ` mismatch errors at startup before
/// any record is processed (PLAN §10.7).
#[test]
fn multiple_mode_rejects_different_sq_name_sets_across_inputs() {
    let dir = TempDir::new().unwrap();
    let input1 = dir.path().join("f1.bam");
    let input2 = dir.path().join("f2.bam");

    // file1 with chr1
    write_bam(&input1, &ot_pair("u0", 1000, 1100));

    // file2 with chr2 (different @SQ)
    let mut header2 = Header::default();
    header2.reference_sequences_mut().insert(
        BString::from("chr2"),
        Map::<ReferenceSequence>::new(NonZeroUsize::try_from(1_000_000).unwrap()),
    );
    let mut writer = BamWriter::from_path(&input2, header2).unwrap();
    let r1 = build_record("u0", b"CT", b"CT", 0x41, 0, 1000, 50);
    let r2 = build_record("u0", b"GA", b"CT", 0x81, 0, 1100, 50);
    writer
        .write_record(&BismarkRecord::from_noodles_record(r1).unwrap())
        .unwrap();
    writer
        .write_record(&BismarkRecord::from_noodles_record(r2).unwrap())
        .unwrap();
    writer.finish().unwrap();

    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.arg("--multiple")
        .arg("--paired")
        .arg("--output_dir")
        .arg(dir.path())
        .arg(&input1)
        .arg(&input2)
        .assert()
        .failure()
        // Tighten per B-M2: assert path-of-offender + missing-chr name
        // so a future regression that mis-blames file1 wouldn't pass.
        .stderr(predicates::str::contains("non-identical @SQ name sets"))
        .stderr(predicates::str::contains("f2.bam"))
        .stderr(predicates::str::contains("\"chr1\""));
}

/// `--multiple` with empty file1 errors out AND leaves no output BAM
/// or report file behind. This is the headline regression test for the
/// Phase E rev-2 writer-before-peek fix.
///
/// Both Phase E reviewers (A-H1 and B-M3) independently found that
/// `run_multiple` opened the writer BEFORE the file1 empty-peek, leaving
/// a header-only output BAM on disk if file1 was empty. The rev-2 fix
/// moves the peek before the writer-open via the `iter::once+chain`
/// pattern (PLAN.md rev 1's original design — confirmed correct here).
#[test]
fn multiple_mode_empty_file1_leaves_no_output_files_behind() {
    let dir = TempDir::new().unwrap();
    let input1 = dir.path().join("empty1.bam");
    let input2 = dir.path().join("nonempty2.bam");

    // file1: header only, no records.
    write_bam(&input1, &[]);
    // file2: one PE pair.
    write_bam(&input2, &ot_pair("u0", 1000, 1100));

    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.arg("--multiple")
        .arg("--paired")
        .arg("--output_dir")
        .arg(dir.path())
        .arg(&input1)
        .arg(&input2)
        .assert()
        .failure()
        .stderr(predicates::str::contains("input file is empty"));

    // Critical: no output BAM, no report file — the writer must NOT have
    // been opened before the empty-peek detected file1's emptiness.
    let out_bam = dir.path().join("empty1.multiple.deduplicated.bam");
    let out_report = dir.path().join("empty1.multiple.deduplication_report.txt");
    assert!(
        !out_bam.exists(),
        "empty file1 should not leave a header-only output BAM behind: {}",
        out_bam.display()
    );
    assert!(
        !out_report.exists(),
        "empty file1 should not leave a report behind: {}",
        out_report.display()
    );
}

/// `--multiple` with mixed input formats (one BAM + one SAM) errors out
/// at startup (PLAN §10.8). Phase D's [`Cli::validate`] does not catch
/// this — it's enforced by `pipeline::run_multiple`'s pre-flight check.
#[test]
fn multiple_mode_rejects_mixed_input_formats() {
    let dir = TempDir::new().unwrap();
    let input_bam = dir.path().join("f1.bam");
    let input_sam = dir.path().join("f2.sam");

    write_bam(&input_bam, &ot_pair("u0", 1000, 1100));

    // Construct a SAM file with the same content as a BAM but text format.
    // Easiest: write a BAM, samtools view -h to text, save as .sam. But
    // we don't depend on samtools in tests. Instead, use bismark-io's
    // SamWriter directly.
    {
        use bismark_io::SamWriter;
        let header = synth_header();
        let file = std::fs::File::create(&input_sam).unwrap();
        let writer_inner = std::io::BufWriter::new(file);
        let mut writer = SamWriter::new(writer_inner, header).unwrap();
        for record in ot_pair("u0_sam", 1000, 1100) {
            let bismark_record = BismarkRecord::from_noodles_record(record).unwrap();
            writer.write_record(&bismark_record).unwrap();
        }
        writer.finish().unwrap();
    }

    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.arg("--multiple")
        .arg("--paired")
        .arg("--output_dir")
        .arg(dir.path())
        .arg(&input_bam)
        .arg(&input_sam)
        .assert()
        .failure()
        .stderr(predicates::str::contains("must all share the same format"));
}

/// Single input with **no duplicates at all** → report shows
/// `removed = 0 (0.00%)`, `n_positions = 0 different position(s)`, and
/// `leftover = count (100.00% of total)`. Pins the contract from
/// PLAN §10.14 / Phase B's `format_removed_zero_no_duplicates` test —
/// here verified end-to-end through the binary.
#[test]
fn pe_dedup_report_with_no_duplicates_renders_zero_percent() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    let mut records = Vec::new();
    for i in 0..5 {
        records.extend(ot_pair(
            &format!("uniq_{i}"),
            1000 + i * 1000,
            1100 + i * 1000,
        ));
    }
    write_bam(&input, &records);

    let mut cmd = Command::cargo_bin("deduplicate_bismark_rs").unwrap();
    cmd.arg("--paired")
        .arg("--output_dir")
        .arg(dir.path())
        .arg(&input)
        .assert()
        .success();

    let report =
        std::fs::read_to_string(dir.path().join("input.deduplication_report.txt")).unwrap();
    let input_path_str = input.display().to_string();
    let expected = format!(
        "\nTotal number of alignments analysed in {input_path_str}:\t5\n\
         Total number duplicated alignments removed:\t0 (0.00%)\n\
         Duplicated alignments were found at:\t0 different position(s)\n\n\
         Total count of deduplicated leftover sequences: 5 (100.00% of total)\n\n"
    );
    assert_eq!(report, expected);
}

// ────────── v1.1: --parallel N tests (BGZF-threaded BAM I/O) ──────────

/// PE dedup with `--parallel 4` produces the same retained-qname set as
/// `--parallel 1`. The headline equivalence check for the v1.1 threaded
/// path (PLAN rev 2 V3).
#[test]
fn pe_parallel_4_produces_same_qname_set_as_single_threaded() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");

    // Build a fixture large AND high-entropy enough to span ≥3 BGZF
    // blocks (~192 KB compressed). 2000 pairs × 2 records × ~300 B raw ≈
    // 1.2 MB raw; with varied-base/varied-XM data the compression ratio
    // is only ~3-4x, so the BAM spans many blocks. Uniform-base records
    // (the standard `ot_pair`'s `'A'×50`) compress ~25× and would fit a
    // single block at any reasonable record count, leaving the threading
    // queue unstressed.
    let mut records = Vec::new();
    for i in 0..2000u64 {
        records.extend(ot_pair_varied(
            &format!("u{i}"),
            1000 + (i as usize) * 100,
            1100 + (i as usize) * 100,
            i,
        ));
    }
    // Inject 3 duplicates at known positions (matching unique reads at
    // i=0, i=100, i=500).
    records.extend(ot_pair_varied("dup_a", 1000, 1100, 1_000_000));
    records.extend(ot_pair_varied(
        "dup_b",
        1000 + 100 * 100,
        1100 + 100 * 100,
        1_000_001,
    ));
    records.extend(ot_pair_varied(
        "dup_c",
        1000 + 500 * 100,
        1100 + 500 * 100,
        1_000_002,
    ));
    write_bam(&input, &records);

    // Run with --parallel 1 (single-threaded path).
    let out1 = dir.path().join("single");
    std::fs::create_dir_all(&out1).unwrap();
    Command::cargo_bin("deduplicate_bismark_rs")
        .unwrap()
        .arg("--paired")
        .arg("--parallel")
        .arg("1")
        .arg("--output_dir")
        .arg(&out1)
        .arg(&input)
        .assert()
        .success();

    // Run with --parallel 4 (threaded path).
    let out4 = dir.path().join("threaded");
    std::fs::create_dir_all(&out4).unwrap();
    Command::cargo_bin("deduplicate_bismark_rs")
        .unwrap()
        .arg("--paired")
        .arg("--parallel")
        .arg("4")
        .arg("--output_dir")
        .arg(&out4)
        .arg(&input)
        .assert()
        .success();

    let single_qnames: HashSet<String> = read_qnames(&out1.join("input.deduplicated.bam"))
        .into_iter()
        .collect();
    let threaded_qnames: HashSet<String> = read_qnames(&out4.join("input.deduplicated.bam"))
        .into_iter()
        .collect();

    assert_eq!(
        threaded_qnames, single_qnames,
        "--parallel 4 must produce same retained-qname set as --parallel 1"
    );
    assert_eq!(
        threaded_qnames.len(),
        2000,
        "2000 unique pairs retained (3 dups removed)"
    );

    // Confirm the fixture actually spans ≥2 BGZF blocks — otherwise the
    // MultithreadedReader's in-order frame contract is unstressed. A
    // BGZF block compresses to ≤65_536 bytes; > 64 KiB on disk guarantees
    // ≥2 blocks were written.
    let bam_size = std::fs::metadata(&input).unwrap().len();
    assert!(
        bam_size > 64 * 1024,
        "PE parallel fixture too small to span multiple BGZF blocks ({bam_size} bytes); \
         increase pair count or use higher-entropy data"
    );
}

/// Same equivalence check for SE mode.
#[test]
fn se_parallel_4_produces_same_qname_set_as_single_threaded() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");

    let mut records = Vec::new();
    for i in 0..3000u64 {
        records.push(se_ot_varied(&format!("u{i}"), 1000 + (i as usize) * 100, i));
    }
    records.push(se_ot_varied("dup_a", 1000, 1_000_000));
    records.push(se_ot_varied("dup_b", 1000 + 50 * 100, 1_000_001));
    write_bam(&input, &records);

    let out1 = dir.path().join("single");
    std::fs::create_dir_all(&out1).unwrap();
    Command::cargo_bin("deduplicate_bismark_rs")
        .unwrap()
        .arg("--single")
        .arg("--parallel")
        .arg("1")
        .arg("--output_dir")
        .arg(&out1)
        .arg(&input)
        .assert()
        .success();

    let out4 = dir.path().join("threaded");
    std::fs::create_dir_all(&out4).unwrap();
    Command::cargo_bin("deduplicate_bismark_rs")
        .unwrap()
        .arg("--single")
        .arg("--parallel")
        .arg("4")
        .arg("--output_dir")
        .arg(&out4)
        .arg(&input)
        .assert()
        .success();

    let single: HashSet<String> = read_qnames(&out1.join("input.deduplicated.bam"))
        .into_iter()
        .collect();
    let threaded: HashSet<String> = read_qnames(&out4.join("input.deduplicated.bam"))
        .into_iter()
        .collect();
    assert_eq!(threaded, single);
    assert_eq!(
        threaded.len(),
        3000,
        "3000 unique reads retained (2 dups removed)"
    );

    let bam_size = std::fs::metadata(&input).unwrap().len();
    assert!(
        bam_size > 64 * 1024,
        "SE parallel fixture too small to span multiple BGZF blocks ({bam_size} bytes)"
    );
}

/// `--multiple --parallel 4` produces the same retained-qname set as
/// `--multiple --parallel 1`.
#[test]
fn multiple_parallel_4_produces_same_qname_set_as_single_threaded() {
    let dir = TempDir::new().unwrap();
    let input1 = dir.path().join("file1.bam");
    let input2 = dir.path().join("file2.bam");

    let mut f1 = Vec::new();
    for i in 0..1000u64 {
        f1.extend(ot_pair_varied(
            &format!("f1_u{i}"),
            1000 + (i as usize) * 100,
            1100 + (i as usize) * 100,
            i,
        ));
    }
    write_bam(&input1, &f1);

    let mut f2 = Vec::new();
    // Space file2 well above file1's range (1000..101_000) to avoid
    // unintended cross-file position-key collisions.
    for i in 0..1000u64 {
        f2.extend(ot_pair_varied(
            &format!("f2_u{i}"),
            500_000 + (i as usize) * 100,
            500_100 + (i as usize) * 100,
            i + 10_000,
        ));
    }
    // Cross-file duplicate of f1_u0 (same chr/positions).
    f2.extend(ot_pair_varied("dup_of_f1_u0", 1000, 1100, 1_000_000));
    write_bam(&input2, &f2);

    let out1 = dir.path().join("single");
    std::fs::create_dir_all(&out1).unwrap();
    Command::cargo_bin("deduplicate_bismark_rs")
        .unwrap()
        .arg("--multiple")
        .arg("--paired")
        .arg("--parallel")
        .arg("1")
        .arg("--output_dir")
        .arg(&out1)
        .arg(&input1)
        .arg(&input2)
        .assert()
        .success();

    let out4 = dir.path().join("threaded");
    std::fs::create_dir_all(&out4).unwrap();
    Command::cargo_bin("deduplicate_bismark_rs")
        .unwrap()
        .arg("--multiple")
        .arg("--paired")
        .arg("--parallel")
        .arg("4")
        .arg("--output_dir")
        .arg(&out4)
        .arg(&input1)
        .arg(&input2)
        .assert()
        .success();

    let single: HashSet<String> = read_qnames(&out1.join("file1.multiple.deduplicated.bam"))
        .into_iter()
        .collect();
    let threaded: HashSet<String> = read_qnames(&out4.join("file1.multiple.deduplicated.bam"))
        .into_iter()
        .collect();
    assert_eq!(threaded, single);
    assert_eq!(
        threaded.len(),
        2000,
        "2000 unique pairs retained (1 cross-file dup removed)"
    );

    let total =
        std::fs::metadata(&input1).unwrap().len() + std::fs::metadata(&input2).unwrap().len();
    assert!(
        total > 64 * 1024,
        "--multiple parallel fixture too small to span multiple BGZF blocks ({total} bytes)"
    );
}

/// PE pair adjacency (R1-then-R2) preserved under `--parallel 4`.
#[test]
fn pe_parallel_4_preserves_r1_followed_by_r2_adjacency() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    // 1500 pairs of varied-base records spans multiple BGZF blocks, so
    // the in-order FIFO contract of `MultithreadedReader` is actually
    // exercised across worker boundaries rather than collapsed into a
    // single block.
    let mut records = Vec::new();
    for i in 0..1500u64 {
        records.extend(ot_pair_varied(
            &format!("read_{i}"),
            1000 + (i as usize) * 100,
            1100 + (i as usize) * 100,
            i,
        ));
    }
    write_bam(&input, &records);

    let bam_size = std::fs::metadata(&input).unwrap().len();
    assert!(
        bam_size > 64 * 1024,
        "PE adjacency fixture too small to span multiple BGZF blocks ({bam_size} bytes)"
    );

    Command::cargo_bin("deduplicate_bismark_rs")
        .unwrap()
        .arg("--paired")
        .arg("--parallel")
        .arg("4")
        .arg("--output_dir")
        .arg(dir.path())
        .arg(&input)
        .assert()
        .success();

    let out_records = read_records(&dir.path().join("input.deduplicated.bam"));
    assert_eq!(out_records.len(), 3000, "1500 pairs × 2 = 3000");
    for (i, record) in out_records.iter().enumerate() {
        let flags = u16::from(record.inner().flags());
        if i % 2 == 0 {
            assert!(
                flags & 0x40 != 0,
                "record {i} must be R1 under --parallel 4"
            );
        } else {
            assert!(
                flags & 0x80 != 0,
                "record {i} must be R2 under --parallel 4"
            );
        }
    }
}

/// `--parallel N` with N > 4 emits a soft "diminishing returns" warning
/// once per invocation. Saturates at N=4 per the oxy benchmark — the
/// dedup state is single-threaded, so only BGZF (de)compression scales.
/// The warning is informational; the run still succeeds.
#[test]
fn parallel_above_four_emits_diminishing_returns_warning() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    write_bam(&input, &ot_pair("u0", 1000, 1100));

    let output = Command::cargo_bin("deduplicate_bismark_rs")
        .unwrap()
        .arg("--paired")
        .arg("--parallel")
        .arg("8")
        .arg("--output_dir")
        .arg(dir.path())
        .arg(&input)
        .assert()
        .success()
        .stderr(predicates::str::contains(
            "exceeds the typical sweet spot (N ≤ 4)",
        ))
        .stderr(predicates::str::contains("--parallel 8"))
        .get_output()
        .clone();

    // Exactly one warning line per invocation, not one per file or per
    // record.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let warning_count = stderr
        .lines()
        .filter(|l| l.contains("exceeds the typical sweet spot"))
        .count();
    assert_eq!(
        warning_count, 1,
        "diminishing-returns warning must appear exactly once per invocation"
    );
}

/// `--parallel 4` is at the sweet-spot threshold and must NOT emit the
/// diminishing-returns warning. The boundary check is `N > 4`, not
/// `N >= 4`.
#[test]
fn parallel_equal_to_four_does_not_emit_diminishing_returns_warning() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    write_bam(&input, &ot_pair("u0", 1000, 1100));

    Command::cargo_bin("deduplicate_bismark_rs")
        .unwrap()
        .arg("--paired")
        .arg("--parallel")
        .arg("4")
        .arg("--output_dir")
        .arg(dir.path())
        .arg(&input)
        .assert()
        .success()
        .stderr(predicates::str::contains("exceeds the typical sweet spot").not());
}

/// `--parallel 0` is rejected at validate stage.
#[test]
fn parallel_zero_is_rejected_at_validate() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bam");
    write_bam(&input, &ot_pair("u0", 1000, 1100));

    Command::cargo_bin("deduplicate_bismark_rs")
        .unwrap()
        .arg("--paired")
        .arg("--parallel")
        .arg("0")
        .arg(&input)
        .assert()
        .failure()
        .stderr(predicates::str::contains("must be ≥ 1"));
}

/// Write a small FASTA (chr1 of length `len`) + its FAI alongside, suitable
/// as a `--cram_ref` for tests that construct a CRAM input. All `N`s — CRAM
/// stores sequence diffs against this reference, so the only constraint is
/// that `len` covers every record's reference span.
fn write_test_fasta(dir: &Path, len: usize) -> PathBuf {
    let fasta_path = dir.join("ref.fa");
    let fai_path = dir.join("ref.fa.fai");
    let mut fasta_content = b">chr1\n".to_vec();
    fasta_content.extend(std::iter::repeat_n(b'N', len));
    fasta_content.push(b'\n');
    std::fs::write(&fasta_path, &fasta_content).unwrap();
    // .fai cols: name, length, offset, linebases, linewidth.
    // ">chr1\n" is 6 bytes; the sequence is on one line of `len` bases.
    std::fs::write(&fai_path, format!("chr1\t{len}\t6\t{len}\t{}\n", len + 1)).unwrap();
    fasta_path
}

/// Build a synthetic header whose chr1 has length `chr1_len` (rather than
/// the standard 1_000_000). Needed for CRAM tests so the reference FASTA
/// can be small (the FASTA must cover every record's reference span).
fn synth_header_with_chr1_len(chr1_len: usize) -> Header {
    let mut header = Header::default();
    header.reference_sequences_mut().insert(
        BString::from("chr1"),
        Map::<ReferenceSequence>::new(NonZeroUsize::try_from(chr1_len).unwrap()),
    );
    header
}

/// PLAN V5: `--parallel N > 1` with CRAM input emits a single-line stderr
/// warning and falls back to single-threaded execution. The retained
/// records must still be correct.
#[test]
fn cram_with_parallel_n_logs_warning_and_runs_single_threaded() {
    let dir = TempDir::new().unwrap();
    let fasta = write_test_fasta(dir.path(), 10_000);

    let cram_in = dir.path().join("input.cram");
    {
        let header = synth_header_with_chr1_len(10_000);
        let mut writer = bismark_io::open_writer(&cram_in, header, Some(&fasta)).unwrap();
        let mut records = Vec::new();
        for i in 0..5 {
            records.extend(ot_pair(&format!("u{i}"), 100 + i * 200, 200 + i * 200));
        }
        // Duplicate of u0.
        records.extend(ot_pair("dup_a", 100, 200));
        for r in &records {
            let bismark_record = BismarkRecord::from_noodles_record(r.clone()).unwrap();
            writer.write_record(&bismark_record).unwrap();
        }
        writer.finish().unwrap();
    }

    let output = Command::cargo_bin("deduplicate_bismark_rs")
        .unwrap()
        .arg("--paired")
        .arg("--parallel")
        .arg("4")
        .arg("--cram_ref")
        .arg(&fasta)
        .arg("--output_dir")
        .arg(dir.path())
        .arg(&cram_in)
        .assert()
        .success()
        .stderr(predicates::str::contains(
            "CRAM input/output runs single-threaded",
        ))
        .stderr(predicates::str::contains("--parallel 4"))
        // The threaded-path startup banner must NOT appear — CRAM took
        // the single-threaded fallback.
        .stderr(predicates::str::contains("BGZF threading:").not())
        .get_output()
        .clone();

    // Exactly ONE warning line per invocation (not per record / per file).
    let stderr = String::from_utf8_lossy(&output.stderr);
    let warning_count = stderr
        .lines()
        .filter(|l| l.contains("CRAM input/output runs single-threaded"))
        .count();
    assert_eq!(warning_count, 1, "CRAM warning must appear exactly once");

    // v1.0 output-naming: for CRAM input, the stem keeps the `.cram`
    // extension (filename.rs:194-195), so the output is BAM-named
    // `<input>.cram.deduplicated.bam`. CRAM-mirror output is README-
    // aspirational, not implemented in v1.0; the important contract here
    // is the dedup correctness on the single-threaded fallback path.
    let out_path = dir.path().join("input.cram.deduplicated.bam");
    assert!(
        out_path.exists(),
        "expected dedup output at {}",
        out_path.display()
    );
    let qnames: HashSet<String> = read_qnames(&out_path).into_iter().collect();
    let expected: HashSet<String> = (0..5).map(|i| format!("u{i}")).collect();
    assert_eq!(
        qnames, expected,
        "CRAM fallback must retain the same 5 unique pairs"
    );
}

/// PLAN V8 (Phase C): the v1.1 `ThreadedBamWriter` must emit the
/// canonical 28-byte BGZF EOF marker as the final bytes of the output
/// BAM under `--multiple --parallel 4`. This exercises the
/// `run_multiple_parallel` path (multiple `ThreadedBamReader`s feeding
/// one `ThreadedBamWriter`), end-to-end through the binary.
///
/// `bismark-io` v1.0.0-beta.2 has a writer-level unit test for the same
/// invariant on the single-input path; this test confirms the
/// guarantee survives the binary's multi-file orchestration.
#[test]
fn parallel_4_multiple_mode_output_ends_with_bgzf_eof_marker() {
    // Canonical BGZF EOF marker — an empty BGZF block. Wire format is
    // stable per the BGZF spec (RFC 1952 + custom BC subfield) and
    // documented in htslib / noodles.
    const BGZF_EOF: [u8; 28] = [
        0x1f, 0x8b, 0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x06, 0x00, 0x42, 0x43, 0x02,
        0x00, 0x1b, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    let dir = TempDir::new().unwrap();
    let input1 = dir.path().join("file1.bam");
    let input2 = dir.path().join("file2.bam");

    let mut f1 = Vec::new();
    for i in 0..400u64 {
        f1.extend(ot_pair_varied(
            &format!("a{i}"),
            1000 + (i as usize) * 100,
            1100 + (i as usize) * 100,
            i,
        ));
    }
    write_bam(&input1, &f1);

    let mut f2 = Vec::new();
    for i in 0..400u64 {
        f2.extend(ot_pair_varied(
            &format!("b{i}"),
            500_000 + (i as usize) * 100,
            500_100 + (i as usize) * 100,
            i + 100_000,
        ));
    }
    write_bam(&input2, &f2);

    let combined =
        std::fs::metadata(&input1).unwrap().len() + std::fs::metadata(&input2).unwrap().len();
    assert!(
        combined > 64 * 1024,
        "fixture combined size ({combined} B) too small to span multiple BGZF blocks"
    );

    Command::cargo_bin("deduplicate_bismark_rs")
        .unwrap()
        .arg("--multiple")
        .arg("--paired")
        .arg("--parallel")
        .arg("4")
        .arg("--output_dir")
        .arg(dir.path())
        .arg(&input1)
        .arg(&input2)
        .assert()
        .success();

    let out_path = dir.path().join("file1.multiple.deduplicated.bam");
    let bytes = std::fs::read(&out_path).unwrap();
    assert!(
        bytes.len() >= BGZF_EOF.len(),
        "output BAM too short to contain a BGZF EOF marker ({} B)",
        bytes.len()
    );
    let trailer = &bytes[bytes.len() - BGZF_EOF.len()..];
    assert_eq!(
        trailer, &BGZF_EOF,
        "ThreadedBamWriter under `--multiple --parallel 4` failed to emit the canonical \
         BGZF EOF marker; got trailer: {trailer:?}"
    );
}

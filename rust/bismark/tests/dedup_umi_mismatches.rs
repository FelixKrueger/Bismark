//! `deduplicate_bismark --umi_mismatches N`: grouping rule against UMI-tools and end-to-end behaviour.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::Path;

use assert_cmd::Command;
use bismark::dedup::umi_group::group_position;
use bismark::io::BamWriter;
use bismark::io::BismarkRecord;
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

fn build_record(qname: &str, xr: &[u8], xg: &[u8], flags: u16, start: usize) -> RecordBuf {
    let read_len = 50;
    let mut record = RecordBuf::default();
    *record.name_mut() = Some(BString::from(qname.as_bytes().to_vec()));
    *record.flags_mut() = Flags::from(flags);
    *record.reference_sequence_id_mut() = Some(0);
    *record.alignment_start_mut() = Some(Position::try_from(start).unwrap());
    *record.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, read_len)]);
    *record.sequence_mut() = Sequence::from(vec![b'A'; read_len]);
    *record.quality_scores_mut() = QualityScores::from(vec![30u8; read_len]);
    for (tag, value) in [
        (*b"XR", xr.to_vec()),
        (*b"XG", xg.to_vec()),
        (*b"XM", vec![b'.'; read_len]),
    ] {
        record
            .data_mut()
            .insert(Tag::from(tag), Value::String(BString::from(value)));
    }
    record
}

fn se_ot(qname: &str, start: usize) -> RecordBuf {
    build_record(qname, b"CT", b"CT", 0, start)
}

fn ot_pair(qname: &str, r1_start: usize, r2_start: usize) -> [RecordBuf; 2] {
    [
        build_record(qname, b"CT", b"CT", 0x41, r1_start),
        build_record(qname, b"GA", b"CT", 0x81, r2_start),
    ]
}

/// Non-directional CTOT pair with Bismark's swapped first/second-in-pair bits (#1030).
fn ctot_pair(qname: &str, r1_start: usize, r2_start: usize) -> [RecordBuf; 2] {
    [
        build_record(qname, b"GA", b"CT", 147, r1_start),
        build_record(qname, b"CT", b"CT", 99, r2_start),
    ]
}

fn write_bam(path: &Path, records: &[RecordBuf]) {
    let mut writer = BamWriter::from_path(path, synth_header()).unwrap();
    for record in records {
        writer
            .write_record(&BismarkRecord::from_noodles_record(record.clone()).unwrap())
            .unwrap();
    }
    writer.finish().unwrap();
}

/// Qnames of all records in file order (PE pairs appear twice, R1 then R2).
fn qnames_in_order(path: &Path) -> Vec<String> {
    let mut reader = bismark::io::open_reader_without_sort_check(path, None).unwrap();
    reader
        .records()
        .map(|r| {
            let record = r.unwrap();
            String::from_utf8_lossy(AsRef::as_ref(record.inner().name().unwrap())).into_owned()
        })
        .collect()
}

/// Run `deduplicate_bismark` with `args` on `input`; returns (kept qnames, report text, stderr).
fn dedup(dir: &Path, input: &Path, args: &[&str]) -> (Vec<String>, String, String) {
    let out_dir = dir.join(format!("out_{}", args.join("_").replace('-', "")));
    let assert = Command::cargo_bin("deduplicate_bismark")
        .unwrap()
        .args(args)
        .arg("--output_dir")
        .arg(&out_dir)
        .arg(input)
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    let stem = input.file_stem().unwrap().to_string_lossy().into_owned();
    let kept = qnames_in_order(&out_dir.join(format!("{stem}.deduplicated.bam")));
    let report =
        std::fs::read_to_string(out_dir.join(format!("{stem}.deduplication_report.txt"))).unwrap();
    (kept, report, stderr)
}

fn removed_count(report: &str) -> u64 {
    let line = report
        .lines()
        .find(|l| l.starts_with("Total number duplicated alignments removed:"))
        .unwrap();
    line.split('\t')
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn positions_count(report: &str) -> u64 {
    let line = report
        .lines()
        .find(|l| l.starts_with("Duplicated alignments were found at:"))
        .unwrap();
    line.split('\t')
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

/// SE truth table: r0/r2/r5 exact copies, r1 one mismatch, r3 two mismatches (a distinct twin),
/// r4/r6 one apart with equal count 1.
fn se_fixture(path: &Path) {
    let reads = [
        ("r0:AAAAAAAA", 1000),
        ("r1:AAAAAAAT", 1000),
        ("r2:AAAAAAAA", 1000),
        ("r3:AAAATTAA", 1000),
        ("r4:CCCCCCCC", 2000),
        ("r5:AAAAAAAA", 1000),
        ("r6:CCCCCCCG", 2000),
    ];
    let records: Vec<RecordBuf> = reads.iter().map(|(q, s)| se_ot(q, *s)).collect();
    write_bam(path, &records);
}

// ───────────────────────────── grouping rule ─────────────────────────────

/// One golden row: UMI, count, UMI-tools cluster index.
type GoldenRow = (String, u32, u32);

#[test]
fn grouping_matches_umi_tools_directional() {
    let golden = include_str!("data/dedup/umi_tools_directional_golden.tsv");
    // (case, threshold) -> rows of (umi, count, cluster)
    let mut cases: BTreeMap<(u32, u8), Vec<GoldenRow>> = BTreeMap::new();
    for line in golden
        .lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("case"))
    {
        let f: Vec<&str> = line.split('\t').collect();
        cases
            .entry((f[0].parse().unwrap(), f[1].parse().unwrap()))
            .or_default()
            .push((
                f[2].to_string(),
                f[3].parse().unwrap(),
                f[4].parse().unwrap(),
            ));
    }
    assert_eq!(
        cases.len(),
        600,
        "golden file should hold 300 cases x 2 thresholds"
    );
    for ((case, threshold), rows) in &cases {
        let umis: Vec<(&[u8], u32)> = rows.iter().map(|(u, c, _)| (u.as_bytes(), *c)).collect();
        let seed_of = group_position(&umis, *threshold);
        let partition = |labels: Vec<usize>| {
            let mut groups: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
            for (i, label) in labels.into_iter().enumerate() {
                groups.entry(label).or_default().push(rows[i].0.as_str());
            }
            let mut out: Vec<Vec<&str>> = groups
                .into_values()
                .map(|mut g| {
                    g.sort_unstable();
                    g
                })
                .collect();
            out.sort();
            out
        };
        let ours = partition(seed_of);
        let theirs = partition(rows.iter().map(|r| r.2 as usize).collect());
        assert_eq!(ours, theirs, "case {case}, threshold {threshold}");
    }
}

// ───────────────────────────── end to end ─────────────────────────────

#[test]
fn one_mismatch_copies_are_removed_and_twins_kept() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("se.bam");
    se_fixture(&input);

    let (kept, report, stderr) = dedup(
        dir.path(),
        &input,
        &["--single", "--barcode", "--umi_mismatches", "1"],
    );
    let kept: Vec<&str> = kept.iter().map(|q| q.split(':').next().unwrap()).collect();
    assert_eq!(kept, ["r0", "r3", "r4"]);
    assert_eq!(removed_count(&report), 4);
    assert_eq!(positions_count(&report), 2);
    assert!(report.contains("(UMI mode)"), "{report}");
    assert!(
        stderr.contains(
            "UMI grouping (up to 1 mismatch): 2 UMIs merged into 2 groups at 2 positions"
        ),
        "{stderr}"
    );
}

#[test]
fn grouped_output_is_a_subsequence_of_exact_output() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("se.bam");
    se_fixture(&input);

    let (exact, exact_report, _) = dedup(dir.path(), &input, &["--single", "--barcode"]);
    let (grouped, grouped_report, _) = dedup(
        dir.path(),
        &input,
        &["--single", "--barcode", "--umi_mismatches", "1"],
    );
    let mut it = exact.iter();
    assert!(
        grouped.iter().all(|q| it.any(|e| e == q)),
        "grouped {grouped:?} is not an in-order subset of exact {exact:?}"
    );
    // The two extra removals are exactly the two merged UMIs.
    assert_eq!(
        removed_count(&grouped_report) - removed_count(&exact_report),
        2
    );
}

#[test]
fn zero_mismatches_matches_the_exact_path() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("se.bam");
    se_fixture(&input);

    let (a, report_a, stderr_a) = dedup(dir.path(), &input, &["--single", "--barcode"]);
    let (b, report_b, stderr_b) = dedup(
        dir.path(),
        &input,
        &["--single", "--barcode", "--umi_mismatches", "0"],
    );
    assert_eq!(a, b);
    assert_eq!(report_a, report_b);
    assert!(!stderr_b.contains("UMI grouping"));
    assert_eq!(
        stderr_a
            .lines()
            .filter(|l| !l.starts_with("Output file is"))
            .count(),
        stderr_b
            .lines()
            .filter(|l| !l.starts_with("Output file is"))
            .count()
    );
}

#[test]
fn report_counts_one_position_for_one_merged_molecule() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("one.bam");
    let records: Vec<RecordBuf> = ["a:ACGTACGT", "b:ACGTACGT", "c:ACGTACGT", "d:ACGTACGA"]
        .iter()
        .map(|q| se_ot(q, 5000))
        .collect();
    write_bam(&input, &records);
    let (kept, report, _) = dedup(
        dir.path(),
        &input,
        &["--single", "--umi", "--umi_mismatches", "1"],
    );
    assert_eq!(kept, ["a:ACGTACGT"]);
    assert_eq!(removed_count(&report), 3);
    assert_eq!(positions_count(&report), 1);
}

#[test]
fn paired_end_including_non_directional_pairs() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("pe.bam");
    let mut records = Vec::new();
    records.extend(ot_pair("p0:AAAAAAAA", 1000, 1100));
    records.extend(ot_pair("p1:AAAAAAAC", 1000, 1100));
    records.extend(ctot_pair("q0:GGGGGGGG", 3000, 3100));
    records.extend(ctot_pair("q1:GGGGGGGG", 3000, 3100));
    records.extend(ctot_pair("q2:GGGGGGGA", 3000, 3100));
    write_bam(&input, &records);

    let (exact, _, _) = dedup(dir.path(), &input, &["--paired", "--barcode"]);
    let (grouped, report, _) = dedup(
        dir.path(),
        &input,
        &["--paired", "--barcode", "--umi_mismatches", "1"],
    );
    let names = |v: &[String]| {
        v.iter()
            .map(|q| q.split(':').next().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(&exact),
        ["p0", "p0", "p1", "p1", "q0", "q0", "q2", "q2"]
    );
    assert_eq!(names(&grouped), ["p0", "p0", "q0", "q0"]);
    assert_eq!(removed_count(&report), 3);
}

#[test]
fn parallel_threads_give_the_same_output() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("se.bam");
    se_fixture(&input);
    let (one, report_one, _) = dedup(
        dir.path(),
        &input,
        &[
            "--single",
            "--barcode",
            "--umi_mismatches",
            "1",
            "--parallel",
            "1",
        ],
    );
    let (four, report_four, _) = dedup(
        dir.path(),
        &input,
        &[
            "--single",
            "--barcode",
            "--umi_mismatches",
            "1",
            "--parallel",
            "4",
        ],
    );
    assert_eq!(one, four);
    assert_eq!(report_one, report_four);
}

#[test]
fn multiple_inputs_group_across_files() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.bam");
    let b = dir.path().join("b.bam");
    write_bam(&a, &[se_ot("a0:TTTTTTTT", 700), se_ot("a1:TTTTTTTT", 700)]);
    write_bam(&b, &[se_ot("b0:TTTTTTTA", 700), se_ot("b1:GGGGGGGG", 900)]);
    let out_dir = dir.path().join("out");
    Command::cargo_bin("deduplicate_bismark")
        .unwrap()
        .args([
            "--single",
            "--barcode",
            "--umi_mismatches",
            "1",
            "--multiple",
            "--output_dir",
        ])
        .arg(&out_dir)
        .arg(&a)
        .arg(&b)
        .assert()
        .success();
    let bam = std::fs::read_dir(&out_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.to_string_lossy().ends_with(".deduplicated.bam"))
        .unwrap();
    let kept: Vec<String> = qnames_in_order(&bam);
    assert_eq!(kept, ["a0:TTTTTTTT", "b1:GGGGGGGG"]);
}

#[test]
fn bclconvert_umis_are_grouped_too() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("bcl.bam");
    let q =
        |i: u32, umi: &str| format!("A00001:001:HN2F7DRX1:1:1101:1452:{i}:{umi}_1:N:0:AATGACGC");
    let records: Vec<RecordBuf> = [q(1, "CAAGAG"), q(2, "CAAGAG"), q(3, "CAAGAT")]
        .iter()
        .map(|n| se_ot(n, 1234))
        .collect();
    write_bam(&input, &records);
    let (kept, report, _) = dedup(
        dir.path(),
        &input,
        &["--single", "--bclconvert", "--umi_mismatches", "1"],
    );
    assert_eq!(kept.len(), 1);
    assert_eq!(removed_count(&report), 2);
}

#[test]
fn umi_mismatches_needs_a_umi_mode() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("se.bam");
    se_fixture(&input);
    Command::cargo_bin("deduplicate_bismark")
        .unwrap()
        .args(["--umi_mismatches", "1"])
        .arg(&input)
        .assert()
        .failure()
        .stderr(predicate::str::contains("--umi_mismatches needs --barcode"));
}

#[cfg(unix)]
#[test]
fn non_regular_input_is_rejected_before_reading() {
    let dir = TempDir::new().unwrap();
    Command::cargo_bin("deduplicate_bismark")
        .unwrap()
        .args(["--barcode", "--umi_mismatches", "1", "--output_dir"])
        .arg(dir.path())
        .arg("/dev/null")
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be a regular file"));
}

#[test]
fn odd_paired_end_input_fails_without_output() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("odd.bam");
    let mut records: Vec<RecordBuf> = ot_pair("p0:AAAAAAAA", 1000, 1100).to_vec();
    records.push(build_record("p1:AAAAAAAA", b"CT", b"CT", 0x41, 2000));
    write_bam(&input, &records);
    Command::cargo_bin("deduplicate_bismark")
        .unwrap()
        .args([
            "--paired",
            "--barcode",
            "--umi_mismatches",
            "1",
            "--output_dir",
        ])
        .arg(dir.path())
        .arg(&input)
        .assert()
        .failure()
        .stderr(predicate::str::contains("unpaired"));
    assert!(!dir.path().join("odd.deduplicated.bam").exists());
}

// ───────────────────────── checked-in fixtures ─────────────────────────

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/dedup")
        .join(name)
}

/// Run on `input` into a fresh `out_dir`; returns (BAM bytes, report bytes, stderr without the output-path line).
fn dedup_bytes(out_dir: &Path, input: &Path, args: &[&str]) -> (Vec<u8>, Vec<u8>, String) {
    let assert = Command::cargo_bin("deduplicate_bismark")
        .unwrap()
        .args(args)
        .arg("--output_dir")
        .arg(out_dir)
        .arg(input)
        .assert()
        .success();
    let stderr: String = String::from_utf8_lossy(&assert.get_output().stderr)
        .lines()
        .filter(|l| !l.starts_with("Output file is"))
        .collect::<Vec<_>>()
        .join("\n");
    let stem = input.file_stem().unwrap().to_string_lossy().into_owned();
    let bam = std::fs::read(out_dir.join(format!("{stem}.deduplicated.bam"))).unwrap();
    let report = std::fs::read(out_dir.join(format!("{stem}.deduplication_report.txt"))).unwrap();
    (bam, report, stderr)
}

const FIXTURES: [(&str, &str); 3] = [
    ("synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam", "--barcode"),
    (
        "synth_bclconvert_10k_R1_val_1_bismark_bt2_pe.bam",
        "--bclconvert",
    ),
    ("nondir_pe_1030.bam", "--barcode"),
];

#[test]
fn zero_mismatches_is_byte_identical_on_every_fixture() {
    for (name, mode) in FIXTURES {
        let dir = TempDir::new().unwrap();
        let input = fixture(name);
        let exact = dedup_bytes(&dir.path().join("exact"), &input, &[mode]);
        let zero = dedup_bytes(
            &dir.path().join("zero"),
            &input,
            &[mode, "--umi_mismatches", "0"],
        );
        assert!(exact.0 == zero.0, "{name}: BAM bytes differ");
        assert!(exact.1 == zero.1, "{name}: report bytes differ");
        assert_eq!(exact.2, zero.2, "{name}: stderr differs");
    }
}

#[test]
fn grouping_on_fixtures_keeps_an_in_order_subset_and_is_thread_invariant() {
    for (name, mode) in FIXTURES {
        let dir = TempDir::new().unwrap();
        let input = fixture(name);
        let (exact, _, _) = dedup(dir.path(), &input, &[mode]);
        let (grouped, _, _) = dedup(dir.path(), &input, &[mode, "--umi_mismatches", "1"]);
        let mut it = exact.iter();
        assert!(
            grouped.iter().all(|q| it.any(|e| e == q)),
            "{name}: not an in-order subset"
        );
        // PE output stays R1, R2 per kept pair.
        assert_eq!(grouped.len() % 2, 0, "{name}: odd record count");
        assert!(
            grouped.chunks(2).all(|p| p[0] == p[1]),
            "{name}: mates not adjacent"
        );

        let one = dedup_bytes(
            &dir.path().join("p1"),
            &input,
            &[mode, "--umi_mismatches", "1", "--parallel", "1"],
        );
        let four = dedup_bytes(
            &dir.path().join("p4"),
            &input,
            &[mode, "--umi_mismatches", "1", "--parallel", "4"],
        );
        assert!(one.0 == four.0, "{name}: --parallel 4 BAM bytes differ");
        assert!(one.1 == four.1, "{name}: --parallel 4 report differs");
    }
}

#[test]
fn removed_count_does_not_depend_on_record_order() {
    let reads = [
        ("r0:AAAAAAAA", 1000),
        ("r1:AAAAAAAT", 1000),
        ("r2:AAAAAAAA", 1000),
        ("r3:AAAATTAA", 1000),
        ("r4:CCCCCCCC", 2000),
        ("r5:AAAAAAAA", 1000),
        ("r6:CCCCCCCG", 2000),
        ("r7:GGGGCCCC", 2000),
        ("r8:GGGGCCCA", 2000),
        ("r9:GGGGCCCA", 2000),
    ];
    let dir = TempDir::new().unwrap();
    let mut counts = Vec::new();
    for rotation in 0..reads.len() {
        for reverse in [false, true] {
            let mut order: Vec<(&str, usize)> = reads.to_vec();
            order.rotate_left(rotation);
            if reverse {
                order.reverse();
            }
            let input = dir.path().join(format!("o{rotation}_{reverse}.bam"));
            let records: Vec<RecordBuf> = order.iter().map(|(q, s)| se_ot(q, *s)).collect();
            write_bam(&input, &records);
            let (kept, report, _) = dedup(
                dir.path(),
                &input,
                &["--single", "--barcode", "--umi_mismatches", "1"],
            );
            counts.push((kept.len(), removed_count(&report), positions_count(&report)));
        }
    }
    assert!(counts.iter().all(|c| *c == counts[0]), "{counts:?}");
}

#[test]
fn cram_input_groups_like_bam() {
    let dir = TempDir::new().unwrap();
    let len = 5_000;
    let fasta = dir.path().join("ref.fa");
    let mut seq = b">chr1\n".to_vec();
    seq.extend(std::iter::repeat_n(b'N', len));
    seq.push(b'\n');
    std::fs::write(&fasta, &seq).unwrap();
    std::fs::write(
        dir.path().join("ref.fa.fai"),
        format!("chr1\t{len}\t6\t{len}\t{}\n", len + 1),
    )
    .unwrap();

    let reads = [
        ("a:ACGTACGT", 100),
        ("b:ACGTACGA", 100),
        ("c:ACGTACGT", 100),
        ("d:TTTTTTTT", 900),
    ];
    let records: Vec<RecordBuf> = reads.iter().map(|(q, s)| se_ot(q, *s)).collect();
    let mut header = Header::default();
    header.reference_sequences_mut().insert(
        BString::from("chr1"),
        Map::<ReferenceSequence>::new(NonZeroUsize::try_from(len).unwrap()),
    );
    let cram = dir.path().join("in.cram");
    let mut writer = bismark::io::open_writer(&cram, header, Some(&fasta)).unwrap();
    for r in &records {
        writer
            .write_record(&BismarkRecord::from_noodles_record(r.clone()).unwrap())
            .unwrap();
    }
    writer.finish().unwrap();

    let out_dir = dir.path().join("out");
    let assert = Command::cargo_bin("deduplicate_bismark")
        .unwrap()
        .args([
            "--single",
            "--barcode",
            "--umi_mismatches",
            "1",
            "--cram_ref",
        ])
        .arg(&fasta)
        .arg("--output_dir")
        .arg(&out_dir)
        .arg(&cram)
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        stderr.contains("1 UMIs merged into 1 groups at 1 positions"),
        "{stderr}"
    );
    let out = std::fs::read_dir(&out_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.to_string_lossy().contains(".deduplicated."))
        .unwrap();
    let mut reader = bismark::io::open_reader(&out, Some(&fasta)).unwrap();
    let kept: Vec<String> = reader
        .records()
        .map(|r| {
            String::from_utf8_lossy(AsRef::as_ref(r.unwrap().inner().name().unwrap())).into_owned()
        })
        .collect();
    assert_eq!(kept, ["a:ACGTACGT", "d:TTTTTTTT"]);
}

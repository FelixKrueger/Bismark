//! End-to-end tests for the opt-in `--allow_discordant` mode, which lets the
//! methylation extractor call methylation on records the Bismark aligner never
//! emits (cross-chromosome / same-chromosome discordant pairs, orphans, and —
//! skipped — secondary/supplementary alignments), so it can consume a
//! general-purpose bisulfite aligner's Bismark-format output directly instead
//! of requiring a `samtools view -f 0x2 -F 0x900` pre-filter.
//!
//! Each new-class row is run twice: with the flag (asserting calls emitted +
//! correct counters) and without (asserting today's exact error / behaviour is
//! unchanged). Plus a flag-on ≡ flag-off byte-identity lock on concordant-only
//! input and a `--parallel` invariant on a mixed fixture.
//!
//! Harness style mirrors `tests/extractor_nondir_swapped_flags_1030.rs`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use assert_cmd::Command;
use bismark::extractor::cli::Cli;
use bismark::extractor::{extract_pe, extract_pe_parallel};
use bismark::io::{BamWriter, BismarkRecord};
use bstr::BString;
use clap::Parser;
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

fn header_two_chr() -> Header {
    let mut header = Header::default();
    for name in ["chr1", "chr2"] {
        header.reference_sequences_mut().insert(
            BString::from(name),
            Map::<ReferenceSequence>::new(NonZeroUsize::try_from(1_000_000).unwrap()),
        );
    }
    header
}

/// One record to write: a validated Bismark record, or a raw noodles record
/// (for unmapped / SEQ-less secondary records that cannot construct a
/// `BismarkRecord`).
enum Rec {
    Bism(BismarkRecord),
    Raw(RecordBuf),
}

/// Build a Bismark-shaped record (`M`-only CIGAR) on the given `refid`.
#[allow(clippy::too_many_arguments)]
fn synth(
    qname: &[u8],
    xr: &[u8],
    xg: &[u8],
    xm: &[u8],
    start: usize,
    refid: usize,
    flags: u16,
) -> BismarkRecord {
    let mut record = RecordBuf::default();
    *record.name_mut() = Some(BString::from(qname.to_vec()));
    *record.flags_mut() = Flags::from(flags);
    *record.reference_sequence_id_mut() = Some(refid);
    *record.alignment_start_mut() = Some(Position::try_from(start).unwrap());
    *record.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, xm.len())]);
    *record.sequence_mut() = Sequence::from(vec![b'A'; xm.len()]);
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

/// A raw unmapped mate (FLAG 0x4): no Bismark tags. The io filter drops it (via
/// the always-on 0x4 check) before validation, leaving its mate an orphan.
fn raw_unmapped(qname: &[u8], flags: u16) -> RecordBuf {
    let mut record = RecordBuf::default();
    *record.name_mut() = Some(BString::from(qname.to_vec()));
    *record.flags_mut() = Flags::from(flags | 0x4);
    *record.sequence_mut() = Sequence::from(vec![b'A'; 10]);
    record
}

/// A raw secondary alignment (FLAG 0x100) with `SEQ='*'` and no XM — the bwa
/// convention. Cannot construct a `BismarkRecord`; under the flag it is skipped
/// at the io layer, without it, it hard-errors at record construction.
fn raw_secondary(qname: &[u8], start: usize, refid: usize) -> RecordBuf {
    let mut record = RecordBuf::default();
    *record.name_mut() = Some(BString::from(qname.to_vec()));
    *record.flags_mut() = Flags::from(0x1 | 0x100);
    *record.reference_sequence_id_mut() = Some(refid);
    *record.alignment_start_mut() = Some(Position::try_from(start).unwrap());
    *record.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, 10)]);
    // SEQ='*' → empty sequence, no XM/XR/XG.
    record
}

fn write_bam(path: &Path, header: Header, records: Vec<Rec>) {
    let mut writer = BamWriter::from_path(path, header).unwrap();
    for r in records {
        match r {
            Rec::Bism(b) => writer.write_record(&b).unwrap(),
            Rec::Raw(raw) => writer.write_raw_record(&raw).unwrap(),
        }
    }
    writer.finish().unwrap();
}

fn run(bam: &Path, outdir: &Path, paired: bool, extra: &[&str]) -> assert_cmd::assert::Assert {
    fs::create_dir_all(outdir).unwrap();
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(bam)
        .arg(if paired {
            "--paired-end"
        } else {
            "--single-end"
        })
        .arg("--comprehensive")
        .arg("--output_dir")
        .arg(outdir);
    for a in extra {
        cmd.arg(a);
    }
    cmd.assert()
}

fn dir_snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut map = BTreeMap::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            map.insert(
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).unwrap(),
            );
        }
    }
    map
}

fn read_report(outdir: &Path, base: &str) -> String {
    fs::read_to_string(outdir.join(format!("{base}_splitting_report.txt")))
        .expect("splitting report produced")
}

/// Parse a `label:\t<u64>` line from the splitting report.
fn report_value(report: &str, label: &str) -> u64 {
    for line in report.lines() {
        let Some(rest) = line.strip_prefix(label) else {
            continue;
        };
        let value = rest
            .trim_start_matches([':', '\t', ' '])
            .split('\t')
            .next()
            .and_then(|v| v.trim().parse::<u64>().ok());
        if let Some(n) = value {
            return n;
        }
    }
    panic!("label {label:?} not found in report:\n{report}");
}

/// Total methylation calls = "Total number of C's analysed".
fn total_calls(report: &str) -> u64 {
    report_value(report, "Total number of C's analysed:")
}

/// Concatenated bytes of the three comprehensive context files.
fn context_blob(outdir: &Path, base: &str) -> String {
    let mut s = String::new();
    for ctx in ["CpG", "CHG", "CHH"] {
        let p = outdir.join(format!("{ctx}_context_{base}.txt"));
        if p.exists() {
            s.push_str(&fs::read_to_string(&p).unwrap());
        }
    }
    s
}

// ───────────────────────────── tests ───────────────────────────────────

/// Cross-chromosome pair: R1 on chr1, R2 on chr2. With the flag both mates are
/// called against their own chromosome and the pair is counted; without the
/// flag the run dies with the cross-chromosome error.
#[test]
fn cross_chr_pair_called_independently() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("sample.bam");
    // R1 OT on chr1 @100 (Z@100), R2 CTOT on chr2 @500 (reverse: Z@ its own ref).
    let r1 = synth(b"x", b"CT", b"CT", b"Z.........", 100, 0, 0x1 | 0x40);
    let r2 = synth(b"x", b"GA", b"CT", b"Z.........", 500, 1, 0x1 | 0x80);
    write_bam(&bam, header_two_chr(), vec![Rec::Bism(r1), Rec::Bism(r2)]);

    // Flag on.
    let out = work.path().join("on");
    run(&bam, &out, true, &["--allow_discordant"]).success();
    let report = read_report(&out, "sample");
    assert_eq!(
        report_value(&report, "Cross-chromosome pairs called independently:"),
        1
    );
    assert_eq!(
        total_calls(&report),
        2,
        "both mates' single Z calls counted"
    );
    let blob = context_blob(&out, "sample");
    assert!(blob.contains("\tchr1\t"), "R1 called on chr1: {blob}");
    assert!(blob.contains("\tchr2\t"), "R2 called on chr2: {blob}");

    // Flag off → cross-chromosome hard error.
    let out_off = work.path().join("off");
    run(&bam, &out_off, true, &[])
        .failure()
        .stderr(predicates::str::contains("different chromosomes"));
}

/// Same-chromosome, non-overlapping discordant pair (R2 far downstream of an
/// OT-class R1 — but a same-orientation FF pair so it is NOT concordant). Both
/// mates called; span-disjoint so no dedup.
#[test]
fn same_chr_disjoint_discordant_pair_called() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("sample.bam");
    // Both forward-class (OT + CTOB) → same orientation → Independent. Disjoint.
    let r1 = synth(b"x", b"CT", b"CT", b"ZZ........", 100, 0, 0x1 | 0x40); // Z@100,101
    let r2 = synth(b"x", b"GA", b"GA", b"ZZ........", 500, 0, 0x1 | 0x80); // Z@500,501
    write_bam(&bam, header_two_chr(), vec![Rec::Bism(r1), Rec::Bism(r2)]);

    let out = work.path().join("on");
    run(&bam, &out, true, &["--allow_discordant"]).success();
    let report = read_report(&out, "sample");
    assert_eq!(
        report_value(
            &report,
            "Same-chromosome discordant pairs called independently:"
        ),
        1
    );
    assert_eq!(total_calls(&report), 4, "all 4 disjoint calls kept");
}

/// **The double-count guard.** A same-chromosome *overlapping* FF pair (both
/// forward-class, spans overlapping, shared cytosine positions). The Independent
/// path must dedup generically (keep R1, drop R2 at shared ref positions) — NOT
/// reuse the FR-only half-plane `drop_overlap`, and NOT call both mates raw.
#[test]
fn same_chr_ff_overlapping_pair_not_double_counted() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("sample.bam");
    // R1 OT @100, CpG Z at ref 100,101,102,103,104.
    // R2 CTOB @102 (forward-class, forward iteration), Z at ref 102,103,104,105,106.
    // Shared ref positions: 102,103,104. Independent (both forward). Overlapping.
    let r1 = synth(b"ovl", b"CT", b"CT", b"ZZZZZ.....", 100, 0, 0x1 | 0x40);
    let r2 = synth(b"ovl", b"GA", b"GA", b"ZZZZZ.....", 102, 0, 0x1 | 0x80);
    write_bam(&bam, header_two_chr(), vec![Rec::Bism(r1), Rec::Bism(r2)]);

    // Flag on: R1's 5 calls + R2's 2 unique calls (105,106); 102-104 deduped.
    let out = work.path().join("on");
    run(&bam, &out, true, &["--allow_discordant"]).success();
    let report = read_report(&out, "sample");
    assert_eq!(
        total_calls(&report),
        7,
        "shared cytosines must be counted ONCE (5 + 2), not double-counted (would be 10)"
    );
    assert_eq!(
        report_value(
            &report,
            "Same-chromosome discordant pairs called independently:"
        ),
        1
    );
    // Each reference position appears exactly once across the context files.
    let blob = context_blob(&out, "sample");
    for pos in ["100", "101", "102", "103", "104", "105", "106"] {
        let needle = format!("\tchr1\t{pos}\t");
        assert_eq!(
            blob.matches(&needle).count(),
            1,
            "ref pos {pos} must appear exactly once (no double count):\n{blob}"
        );
    }

    // Flag off (concordant path): the half-plane drop_overlap treats the pair as
    // OT and drops ALL of R2 (the silent mis-drop the flag fixes) → only R1's 5.
    let out_off = work.path().join("off");
    run(&bam, &out_off, true, &[]).success();
    let report_off = read_report(&out_off, "sample");
    assert_eq!(
        total_calls(&report_off),
        5,
        "flag-off pins today's behaviour (half-plane drops all of R2)"
    );
}

/// Orphan (mate unmapped) — mid-file (pushback path) and final-record. With the
/// flag R1 is called single-end style; without it the survivor mis-pairs
/// (`MateMismatch`) or is the unpaired final record.
#[test]
fn orphan_midfile_and_final_called() {
    let work = tempfile::tempdir().unwrap();

    // Mid-file: A_R1, A_R2(unmapped→dropped), B_R1, B_R2.
    let bam_mid = work.path().join("mid.bam");
    write_bam(
        &bam_mid,
        header_two_chr(),
        vec![
            Rec::Bism(synth(b"A", b"CT", b"CT", b"Z.........", 100, 0, 0x1 | 0x40)),
            Rec::Raw(raw_unmapped(b"A", 0x1 | 0x80)),
            // B is a concordant, NON-overlapping FR pair (2 calls kept).
            Rec::Bism(synth(b"B", b"CT", b"CT", b"Z.........", 200, 0, 0x1 | 0x40)),
            Rec::Bism(synth(b"B", b"GA", b"CT", b"Z.........", 300, 0, 0x1 | 0x80)),
        ],
    );
    let out = work.path().join("mid_on");
    run(&bam_mid, &out, true, &["--allow_discordant"]).success();
    let report = read_report(&out, "mid");
    assert_eq!(
        report_value(&report, "Orphan reads (mate unmapped) called:"),
        1
    );
    // A's orphan (1 call) + B's concordant pair (2 calls) = 3.
    assert_eq!(total_calls(&report), 3);

    // Flag off: after the unmapped mate vanishes, A_R1 mis-pairs with B_R1.
    let out_off = work.path().join("mid_off");
    run(&bam_mid, &out_off, true, &[])
        .failure()
        .stderr(predicates::str::contains("mate mismatch"));

    // Final-record orphan: A_R1, A_R2(unmapped→dropped) → A_R1 is the last record.
    let bam_fin = work.path().join("fin.bam");
    write_bam(
        &bam_fin,
        header_two_chr(),
        vec![
            Rec::Bism(synth(b"A", b"CT", b"CT", b"Z.........", 100, 0, 0x1 | 0x40)),
            Rec::Raw(raw_unmapped(b"A", 0x1 | 0x80)),
        ],
    );
    let out_fin = work.path().join("fin_on");
    run(&bam_fin, &out_fin, true, &["--allow_discordant"]).success();
    assert_eq!(
        report_value(
            &read_report(&out_fin, "fin"),
            "Orphan reads (mate unmapped) called:"
        ),
        1
    );

    // Flag off: the historical unpaired-final-record error.
    let out_fin_off = work.path().join("fin_off");
    run(&bam_fin, &out_fin_off, true, &[])
        .failure()
        .stderr(predicates::str::contains("unpaired final record"));
}

/// Secondary (`SEQ='*'`) interleaved between primary pairs: skipped + counted
/// with the flag, pairing of the surrounding primaries unaffected. Without the
/// flag it hard-errors at record construction (no XR tag).
#[test]
fn secondary_skipped_and_counted() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("sample.bam");
    write_bam(
        &bam,
        header_two_chr(),
        vec![
            Rec::Bism(synth(b"P", b"CT", b"CT", b"Z.........", 100, 0, 0x1 | 0x40)),
            Rec::Bism(synth(b"P", b"GA", b"CT", b"Z.........", 200, 0, 0x1 | 0x80)),
            Rec::Raw(raw_secondary(b"P", 500, 0)),
            Rec::Bism(synth(b"Q", b"CT", b"CT", b"Z.........", 300, 0, 0x1 | 0x40)),
            Rec::Bism(synth(b"Q", b"GA", b"CT", b"Z.........", 400, 0, 0x1 | 0x80)),
        ],
    );

    let out = work.path().join("on");
    run(&bam, &out, true, &["--allow_discordant"]).success();
    let report = read_report(&out, "sample");
    assert_eq!(report_value(&report, "Secondary alignments skipped:"), 1);
    // Two concordant pairs, 1 call each mate = 4; the secondary contributed none.
    assert_eq!(total_calls(&report), 4);

    // Flag off: the SEQ='*'/tag-less secondary is not dropped and fails record
    // construction at the missing XR tag (pin the error identity, like the
    // sibling flag-off checks).
    let out_off = work.path().join("off");
    run(&bam, &out_off, true, &[])
        .failure()
        .stderr(predicates::str::contains("missing required Bismark tag"));
}

/// Supplementary (FLAG 0x800, hard-clipped, WITH valid tags) interleaved between
/// primary pairs: skipped + counted with the flag; without it, it breaks R1/R2
/// adjacency → `MateMismatch`.
#[test]
fn supplementary_skipped_and_counted() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("sample.bam");
    write_bam(
        &bam,
        header_two_chr(),
        vec![
            Rec::Bism(synth(b"P", b"CT", b"CT", b"Z.........", 100, 0, 0x1 | 0x40)),
            Rec::Bism(synth(b"P", b"GA", b"CT", b"Z.........", 200, 0, 0x1 | 0x80)),
            // Supplementary carries valid tags (constructs fine) → flag-off it
            // is NOT dropped and breaks adjacency.
            Rec::Bism(synth(b"P", b"CT", b"CT", b"Zz...", 500, 0, 0x1 | 0x800)),
            Rec::Bism(synth(b"Q", b"CT", b"CT", b"Z.........", 300, 0, 0x1 | 0x40)),
            Rec::Bism(synth(b"Q", b"GA", b"CT", b"Z.........", 400, 0, 0x1 | 0x80)),
        ],
    );

    let out = work.path().join("on");
    run(&bam, &out, true, &["--allow_discordant"]).success();
    let report = read_report(&out, "sample");
    assert_eq!(
        report_value(&report, "Supplementary alignments skipped:"),
        1
    );
    assert_eq!(
        total_calls(&report),
        4,
        "supplementary contributed no calls"
    );

    let out_off = work.path().join("off");
    run(&bam, &out_off, true, &[])
        .failure()
        .stderr(predicates::str::contains("mate mismatch"));
}

/// Byte-identity lock: on a concordant-only PE fixture, flag-on output equals
/// flag-off output for every file EXCEPT the splitting report, which differs
/// only by the appended (all-zero) `--allow_discordant` section.
#[test]
fn flag_on_equals_flag_off_on_concordant_input_modulo_report_section() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("sample.bam");
    // Two proper FR (OT/CTOT, OB/CTOB) concordant pairs.
    write_bam(
        &bam,
        header_two_chr(),
        vec![
            // OT pair: R1 (OT) upstream, R2 (CTOT) downstream.
            Rec::Bism(synth(b"a", b"CT", b"CT", b"Zz.X.h....", 100, 0, 0x1 | 0x40)),
            Rec::Bism(synth(b"a", b"GA", b"CT", b"Zz.X.h....", 120, 0, 0x1 | 0x80)),
            // OB pair: R1 (OB) downstream, R2 (CTOB) upstream — proper FR geometry.
            Rec::Bism(synth(b"b", b"CT", b"GA", b"z.H.x.....", 320, 0, 0x1 | 0x40)),
            Rec::Bism(synth(b"b", b"GA", b"GA", b"z.H.x.....", 300, 0, 0x1 | 0x80)),
        ],
    );

    let off = work.path().join("off");
    let on = work.path().join("on");
    run(&bam, &off, true, &[]).success();
    run(&bam, &on, true, &["--allow_discordant"]).success();

    let snap_off = dir_snapshot(&off);
    let snap_on = dir_snapshot(&on);
    // Same set of files.
    assert_eq!(
        snap_off.keys().collect::<Vec<_>>(),
        snap_on.keys().collect::<Vec<_>>()
    );
    // Every file except the splitting report is byte-identical.
    for (name, off_bytes) in &snap_off {
        let on_bytes = &snap_on[name];
        if name.ends_with("_splitting_report.txt") {
            let off_s = String::from_utf8_lossy(off_bytes);
            let on_s = String::from_utf8_lossy(on_bytes);
            assert!(
                on_s.starts_with(off_s.as_ref()),
                "flag-off report must be a prefix of flag-on report"
            );
            let suffix = &on_s[off_s.len()..];
            assert!(
                suffix.contains("Discordant read handling (--allow_discordant)"),
                "flag-on report appends the discordant section; got suffix: {suffix:?}"
            );
            assert!(
                suffix.contains("Cross-chromosome pairs called independently:\t0"),
                "section reports the (zero) policy counts"
            );
            assert!(
                !off_s.contains("Discordant read handling"),
                "flag-off report must NOT contain the section"
            );
        } else {
            assert_eq!(
                off_bytes, on_bytes,
                "non-report file {name} must be byte-identical flag-on vs flag-off"
            );
        }
    }
}

/// `--parallel N` byte-invariance with the flag on, over a mixed fixture
/// exercising all classes (concordant, cross-chr, orphan, secondary): the
/// producer's classification rides the same `(batch_seq, within_idx)` order, so
/// `--parallel 1` and `--parallel 4` output must be byte-identical.
#[test]
fn parallel_invariant_with_flag_on_mixed_fixture() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("mixed.bam");
    write_bam(
        &bam,
        header_two_chr(),
        vec![
            // concordant FR
            Rec::Bism(synth(b"c", b"CT", b"CT", b"Zz.X.h....", 100, 0, 0x1 | 0x40)),
            Rec::Bism(synth(b"c", b"GA", b"CT", b"Zz.X.h....", 120, 0, 0x1 | 0x80)),
            // orphan (mate unmapped)
            Rec::Bism(synth(b"o", b"CT", b"CT", b"Z.H.x.....", 400, 0, 0x1 | 0x40)),
            Rec::Raw(raw_unmapped(b"o", 0x1 | 0x80)),
            // cross-chr
            Rec::Bism(synth(b"x", b"CT", b"CT", b"Z.........", 500, 0, 0x1 | 0x40)),
            Rec::Bism(synth(b"x", b"GA", b"CT", b"Z.........", 700, 1, 0x1 | 0x80)),
            // secondary (skipped)
            Rec::Raw(raw_secondary(b"x", 900, 0)),
            // another concordant FR
            Rec::Bism(synth(b"d", b"CT", b"GA", b"z.H.x.....", 800, 0, 0x1 | 0x40)),
            Rec::Bism(synth(b"d", b"GA", b"GA", b"z.H.x.....", 820, 0, 0x1 | 0x80)),
        ],
    );

    let p1 = work.path().join("p1");
    let p4 = work.path().join("p4");
    run(&bam, &p1, true, &["--allow_discordant", "--parallel", "1"]).success();
    run(&bam, &p4, true, &["--allow_discordant", "--parallel", "4"]).success();
    assert_eq!(
        dir_snapshot(&p1),
        dir_snapshot(&p4),
        "--parallel 4 output must be byte-identical to --parallel 1 with the flag on"
    );
}

/// Legacy single-threaded `extract_pe` ≡ parallel `extract_pe_parallel` on a
/// mixed fixture with the flag on (the `mod.rs` PHASE-F oracle, extended to the
/// new discordant classes). Keeps the "parallel ≡ single-threaded" invariant
/// meaningful for `PeIndependent` / `Orphan`.
#[test]
fn legacy_equals_parallel_with_flag_on_mixed_fixture() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("mixed.bam");
    write_bam(
        &bam,
        header_two_chr(),
        vec![
            Rec::Bism(synth(b"c", b"CT", b"CT", b"Zz.X.h....", 100, 0, 0x1 | 0x40)),
            Rec::Bism(synth(b"c", b"GA", b"CT", b"Zz.X.h....", 300, 0, 0x1 | 0x80)),
            Rec::Bism(synth(b"o", b"CT", b"CT", b"Z.H.x.....", 400, 0, 0x1 | 0x40)),
            Rec::Raw(raw_unmapped(b"o", 0x1 | 0x80)),
            Rec::Bism(synth(b"x", b"CT", b"CT", b"Z.........", 500, 0, 0x1 | 0x40)),
            Rec::Bism(synth(b"x", b"GA", b"CT", b"Z.........", 700, 1, 0x1 | 0x80)),
            Rec::Raw(raw_secondary(b"x", 900, 0)),
            // same-chr FF overlapping (Independent + generic dedup)
            Rec::Bism(synth(b"f", b"CT", b"CT", b"ZZZZZ.....", 100, 0, 0x1 | 0x40)),
            Rec::Bism(synth(b"f", b"GA", b"GA", b"ZZZZZ.....", 102, 0, 0x1 | 0x80)),
        ],
    );

    let cfg = |dir: &Path, parallel: &str| {
        Cli::try_parse_from([
            "bismark_methylation_extractor",
            "--paired-end",
            "--comprehensive",
            "--allow_discordant",
            "--parallel",
            parallel,
            "--output_dir",
            dir.to_str().unwrap(),
            bam.to_str().unwrap(),
        ])
        .unwrap()
        .validate()
        .unwrap()
    };

    let legacy = work.path().join("legacy");
    let parallel = work.path().join("parallel");
    fs::create_dir_all(&legacy).unwrap();
    fs::create_dir_all(&parallel).unwrap();
    extract_pe(&bam, &cfg(&legacy, "1")).unwrap();
    extract_pe_parallel(&bam, &cfg(&parallel, "4")).unwrap();

    assert_eq!(
        dir_snapshot(&legacy),
        dir_snapshot(&parallel),
        "single-threaded extract_pe must equal extract_pe_parallel with --allow_discordant"
    );
}

/// R2-orphan: the *R1* mate is unmapped (io-dropped), so the surviving primary
/// is a read-2 record (FLAG 0x80). Its `read_identity()` is `R2`, driving the
/// `ReadIdentity::R2` arm of `process_orphan` / `handle_one_orphan` (R2 trims +
/// M-bias slot 1) — the branch every other orphan fixture (all R1 survivors)
/// leaves uncovered. Asserts the read-2 survivor is called and counted.
#[test]
fn r2_orphan_r1_unmapped_called_via_r2_identity() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("sample.bam");
    // R1 unmapped (0x40 | 0x4 via raw_unmapped) → dropped at the io layer; the
    // mapped read-2 (0x80) survivor is the orphan.
    write_bam(
        &bam,
        header_two_chr(),
        vec![
            Rec::Raw(raw_unmapped(b"A", 0x1 | 0x40)),
            Rec::Bism(synth(b"A", b"GA", b"CT", b"Z.........", 100, 0, 0x1 | 0x80)),
        ],
    );

    let out = work.path().join("on");
    run(&bam, &out, true, &["--allow_discordant"]).success();
    let report = read_report(&out, "sample");
    assert_eq!(
        report_value(&report, "Orphan reads (mate unmapped) called:"),
        1
    );
    // The read-2 survivor's single Z call is emitted (proves the R2 arm ran and
    // produced output, not just that it didn't panic).
    assert_eq!(total_calls(&report), 1);
    let blob = context_blob(&out, "sample");
    assert!(
        blob.contains("\tchr1\t"),
        "read-2 orphan called on chr1: {blob}"
    );
}

/// Cross-chromosome pair whose mates share the *same numeric* reference position
/// on different chromosomes. Pins the `r1_chr_id == r2_chr_id` guard: cross-chr
/// positions coincide numerically but denote different loci, so they must NOT be
/// deduped. Both Z@100 calls (chr1 and chr2) must survive — a regression that
/// dropped the guard and deduped by bare `ref_pos` would lose one.
#[test]
fn cross_chr_coincident_positions_not_deduped() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("sample.bam");
    // R1 chr1 @100 (Z@100), R2 chr2 @100 (Z@100) — identical numeric position.
    let r1 = synth(b"y", b"CT", b"CT", b"Z.........", 100, 0, 0x1 | 0x40);
    let r2 = synth(b"y", b"GA", b"CT", b"Z.........", 100, 1, 0x1 | 0x80);
    write_bam(&bam, header_two_chr(), vec![Rec::Bism(r1), Rec::Bism(r2)]);

    let out = work.path().join("on");
    run(&bam, &out, true, &["--allow_discordant"]).success();
    let report = read_report(&out, "sample");
    assert_eq!(
        total_calls(&report),
        2,
        "coincident cross-chr positions denote different loci — both must be kept"
    );
    let blob = context_blob(&out, "sample");
    assert!(
        blob.contains("\tchr1\t100\t"),
        "R1 called at chr1:100: {blob}"
    );
    assert!(
        blob.contains("\tchr2\t100\t"),
        "R2 called at chr2:100 (not deduped against chr1:100): {blob}"
    );
}

/// **Opposite-strand-of-origin overlap guard.** A same-chromosome *overlapping*
/// pair whose mates come from opposite original strands — R1 `OT` (top) + R2
/// `OB` (bottom) — passes the complementary-orientation check but is NOT a
/// genuine Bismark pair. It must route to `Independent` (each mate called
/// against its own strand), NOT `Concordant`: on the concordant path the FR-only
/// half-plane `drop_overlap` treats the pair as `OT`-forward and silently
/// discards R2's bottom-strand overlap-region calls. The two strands' cytosines
/// never share a reference position, so `drop_overlap_generic` keeps every call.
#[test]
fn opposite_origin_ot_ob_overlap_not_misdropped() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("sample.bam");
    // R1 OT @100 [100,109], one Z (ref 100). R2 OB @102 [102,111], ten Z's
    // (refs 102-111). Spans overlap (102-109); R1's single call never collides
    // numerically with R2's (100 ∉ 102..111).
    let r1 = synth(b"op", b"CT", b"CT", b"Z.........", 100, 0, 0x1 | 0x40); // OT
    let r2 = synth(b"op", b"CT", b"GA", b"ZZZZZZZZZZ", 102, 0, 0x1 | 0x80); // OB
    write_bam(&bam, header_two_chr(), vec![Rec::Bism(r1), Rec::Bism(r2)]);

    // Flag on: OT+OB → Independent; opposite strands never collide, so all
    // 1 + 10 = 11 calls are kept.
    let out = work.path().join("on");
    run(&bam, &out, true, &["--allow_discordant"]).success();
    let report = read_report(&out, "sample");
    assert_eq!(
        report_value(
            &report,
            "Same-chromosome discordant pairs called independently:"
        ),
        1,
        "OT+OB (opposite origin) is a same-chromosome discordant pair"
    );
    let on_total = total_calls(&report);
    assert_eq!(
        on_total, 11,
        "all calls kept (opposite strands never collide)"
    );

    // Flag off: the pre-fix hazard — OT+OB accepted as Concordant, drop_overlap
    // (as OT-forward) discards R2's overlap-region calls (ref ≤ 109). Fewer
    // calls survive, proving the routing fix recovers real data.
    let out_off = work.path().join("off");
    run(&bam, &out_off, true, &[]).success();
    let off_total = total_calls(&read_report(&out_off, "sample"));
    assert!(
        off_total < on_total,
        "flag-off concordant path drops R2 overlap-region calls: off={off_total} on={on_total}"
    );
}

/// SE-mode `--allow_discordant`: the flag enables only the io-layer 0x900
/// skip-and-count (there is no pairing to make discordant). A secondary
/// interleaved among single-end primaries is skipped + counted; the primaries
/// are called. Every other test in this file is paired-end, so this is the SE
/// path's only coverage.
#[test]
fn se_mode_secondary_skipped_and_counted() {
    let work = tempfile::tempdir().unwrap();
    let bam = work.path().join("sample.bam");
    write_bam(
        &bam,
        header_two_chr(),
        vec![
            Rec::Bism(synth(b"a", b"CT", b"CT", b"Z.........", 100, 0, 0x0)),
            Rec::Raw(raw_secondary(b"a", 500, 0)),
            Rec::Bism(synth(b"b", b"CT", b"CT", b"Z.........", 200, 0, 0x0)),
        ],
    );

    // Flag on (single-end): secondary dropped + counted, both primaries called.
    let out = work.path().join("on");
    run(&bam, &out, false, &["--allow_discordant"]).success();
    let report = read_report(&out, "sample");
    assert_eq!(report_value(&report, "Secondary alignments skipped:"), 1);
    assert_eq!(total_calls(&report), 2, "both SE primaries' single Z calls");

    // Flag off: the tag-less secondary is not dropped → record-construction error.
    let out_off = work.path().join("off");
    run(&bam, &out_off, false, &[])
        .failure()
        .stderr(predicates::str::contains("missing required Bismark tag"));
}

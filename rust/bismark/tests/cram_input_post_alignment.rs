//! CRAM 3.1 input for `bam2nuc`, `filter_non_conversion` and
//! `methylation_consistency` (#1167).
//!
//! Each tool runs on a small Bismark paired-end BAM and on a CRAM 3.1 copy of
//! it, and the outputs must match. The fixture is built at test time: a 2 kb
//! reference (also the `bam2nuc` genome), a BAM of reads taken from it, and a
//! CRAM written by noodles with the CRAM 3.1 rANS Nx16 codec. (The 3.1 name
//! tokenizer is avoided: noodles 0.93 writes it wrongly and its debug build
//! panics decoding it.)
//! No samtools is needed.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use bstr::BString;
use noodles_core::Position;
use noodles_cram::codecs::{Encoder, rans_nx16};
use noodles_cram::container::BlockContentEncoderMap;
use noodles_cram::container::compression_header::data_series_encodings::DataSeries;
use noodles_sam::Header;
use noodles_sam::alignment::RecordBuf;
use noodles_sam::alignment::io::Write as _;
use noodles_sam::alignment::record::Flags;
use noodles_sam::alignment::record::MappingQuality;
use noodles_sam::alignment::record::cigar::Op;
use noodles_sam::alignment::record::cigar::op::Kind;
use noodles_sam::alignment::record::data::field::Tag;
use noodles_sam::alignment::record_buf::data::field::Value;
use noodles_sam::alignment::record_buf::{Cigar, QualityScores, Sequence};
use tempfile::TempDir;

const REF_LEN: usize = 2000;
const READ_LEN: usize = 40;

/// Methylation call strings, one per pair (R1 and R2 share it). They put
/// pairs in every `methylation_consistency` bucket and make `filter` remove
/// the `HHH` pairs.
const CALLS: [&str; 6] = ["ZZZZZZ", "zzzzzz", "ZzZzZz", "ZZZZZZHHH", "zzzzzzhh", "Zz"];

struct Fixture {
    dir: TempDir,
    reference: PathBuf,
    bam: PathBuf,
    cram: PathBuf,
}

fn fixture() -> Fixture {
    let dir = TempDir::new().unwrap();

    // Reference = bam2nuc genome folder, single line so the .fai is trivial.
    let mut state: u32 = 1;
    let seq: Vec<u8> = (0..REF_LEN)
        .map(|_| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            b"ACGT"[(state >> 16) as usize % 4]
        })
        .collect();
    let genome = dir.path().join("genome");
    std::fs::create_dir(&genome).unwrap();
    let reference = genome.join("chr1.fa");
    let mut fasta = b">chr1\n".to_vec();
    fasta.extend_from_slice(&seq);
    fasta.push(b'\n');
    std::fs::write(&reference, fasta).unwrap();
    std::fs::write(
        genome.join("chr1.fa.fai"),
        format!("chr1\t{REF_LEN}\t6\t{REF_LEN}\t{}\n", REF_LEN + 1),
    )
    .unwrap();

    let header: Header = "@HD\tVN:1.6\tSO:unsorted\n\
         @SQ\tSN:chr1\tLN:2000\n\
         @PG\tID:Bismark\tVN:v0.25.1\tCL:\"bismark --genome genome -1 r1.fq -2 r2.fq\"\n"
        .parse()
        .unwrap();

    let mut records = Vec::new();
    for (i, calls) in CALLS.iter().enumerate() {
        let name = format!("pair{i}");
        let p1 = 50 + i * 300;
        let p2 = p1 + 60;
        let tlen = (p2 + READ_LEN - p1) as i32;
        let mut xm = vec![b'.'; READ_LEN];
        for (j, c) in calls.bytes().enumerate() {
            xm[2 + j * 4] = c;
        }
        records.push(record(&name, 99, p1, p2, tlen, &seq, &xm));
        records.push(record(&name, 147, p2, p1, -tlen, &seq, &xm));
    }

    let bam = dir.path().join("in.bam");
    let mut writer = noodles_bam::io::Writer::new(std::fs::File::create(&bam).unwrap());
    writer.write_header(&header).unwrap();
    for r in &records {
        writer.write_alignment_record(&header, r).unwrap();
    }
    writer.finish(&header).unwrap();
    drop(writer);

    let cram = dir.path().join("in.cram");
    let repo =
        noodles_fasta::Repository::new(noodles_fasta::repository::adapters::IndexedReader::new(
            noodles_fasta::io::indexed_reader::Builder::default()
                .build_from_path(&reference)
                .unwrap(),
        ));
    // A 3.1 codec makes noodles write a CRAM 3.1 file.
    let codecs = BlockContentEncoderMap::builder()
        .set_data_series_encoder(
            DataSeries::Names,
            Some(Encoder::RansNx16(rans_nx16::Flags::ORDER)),
        )
        .set_data_series_encoder(
            DataSeries::QualityScores,
            Some(Encoder::RansNx16(rans_nx16::Flags::ORDER)),
        )
        .build();
    let mut writer = noodles_cram::io::writer::Builder::default()
        .set_reference_sequence_repository(repo)
        .set_block_content_encoder_map(codecs)
        .build_from_path(&cram)
        .unwrap();
    writer.write_header(&header).unwrap();
    for r in &records {
        writer.write_alignment_record(&header, r).unwrap();
    }
    writer.finish(&header).unwrap();
    // File definition: "CRAM", major, minor.
    assert_eq!(&std::fs::read(&cram).unwrap()[..6], b"CRAM\x03\x01");

    Fixture {
        dir,
        reference,
        bam,
        cram,
    }
}

fn record(
    name: &str,
    flags: u16,
    pos: usize,
    mate_pos: usize,
    tlen: i32,
    reference: &[u8],
    xm: &[u8],
) -> RecordBuf {
    let mut r = RecordBuf::default();
    *r.name_mut() = Some(BString::from(name));
    *r.flags_mut() = Flags::from(flags);
    *r.reference_sequence_id_mut() = Some(0);
    *r.alignment_start_mut() = Some(Position::try_from(pos).unwrap());
    *r.mapping_quality_mut() = MappingQuality::new(42);
    *r.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, READ_LEN)]);
    *r.mate_reference_sequence_id_mut() = Some(0);
    *r.mate_alignment_start_mut() = Some(Position::try_from(mate_pos).unwrap());
    *r.template_length_mut() = tlen;
    *r.sequence_mut() = Sequence::from(reference[pos - 1..pos - 1 + READ_LEN].to_vec());
    *r.quality_scores_mut() = QualityScores::from(vec![40u8; READ_LEN]);
    let data = r.data_mut();
    data.insert(Tag::EDIT_DISTANCE, Value::UInt8(0));
    data.insert(
        Tag::MISMATCHED_POSITIONS,
        Value::String(BString::from(READ_LEN.to_string())),
    );
    data.insert(Tag::from(*b"XM"), Value::String(BString::from(xm)));
    data.insert(Tag::from(*b"XR"), Value::String(BString::from("CT")));
    data.insert(Tag::from(*b"XG"), Value::String(BString::from("CT")));
    r
}

/// Copy `src` into a new subdirectory `name` of the fixture (the filter and
/// consistency tools write their outputs next to the input).
fn stage(fx: &Fixture, name: &str, src: &Path) -> PathBuf {
    let d = fx.dir.path().join(name);
    std::fs::create_dir(&d).unwrap();
    let dst = d.join(src.file_name().unwrap());
    std::fs::copy(src, &dst).unwrap();
    dst
}

fn read(p: &Path) -> Vec<u8> {
    std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// The records of an output BAM. Headers are not compared: an output keeps
/// its input's header, and a CRAM header carries `@SQ M5:`.
fn bam_records(p: &Path) -> Vec<RecordBuf> {
    let mut reader = noodles_bam::io::reader::Builder.build_from_path(p).unwrap();
    let header = reader.read_header().unwrap();
    reader.record_bufs(&header).map(Result::unwrap).collect()
}

fn assert_cram_without_ref_fails(bin: &str, args: &[&Path]) {
    Command::cargo_bin(bin)
        .unwrap()
        .args(args)
        .assert()
        .failure()
        .code(1)
        .stderr(predicates::str::contains("CRAM reference required for"));
}

#[test]
fn cram31_decodes_to_same_core_fields_as_bam() {
    let fx = fixture();
    let mut bam = bismark::io::open_reader(&fx.bam, None).unwrap();
    let mut cram = bismark::io::open_reader(&fx.cram, Some(&fx.reference)).unwrap();
    let bam: Vec<_> = bam.records().map(Result::unwrap).collect();
    let cram: Vec<_> = cram.records().map(Result::unwrap).collect();
    assert_eq!(bam.len(), CALLS.len() * 2);
    assert_eq!(bam.len(), cram.len());
    // NM/MD/TLEN are left out: their CRAM fidelity is fix/1167-36's job.
    let tags = [*b"XM", *b"XR", *b"XG"].map(Tag::from);
    for (b, c) in bam.iter().zip(&cram) {
        let (b, c) = (b.inner(), c.inner());
        assert_eq!(b.name(), c.name());
        assert_eq!(b.flags(), c.flags());
        assert_eq!(b.reference_sequence_id(), c.reference_sequence_id());
        assert_eq!(b.alignment_start(), c.alignment_start());
        assert_eq!(b.mapping_quality(), c.mapping_quality());
        assert_eq!(b.cigar(), c.cigar());
        assert_eq!(b.sequence(), c.sequence());
        assert_eq!(b.quality_scores(), c.quality_scores());
        for tag in &tags {
            assert_eq!(b.data().get(tag), c.data().get(tag), "{tag:?}");
        }
    }
}

#[test]
fn bam2nuc_cram31_matches_bam() {
    let fx = fixture();
    let genome = fx.reference.parent().unwrap();
    let run = |input: &Path, out: &str, cram_ref: bool| {
        let out = fx.dir.path().join(out);
        std::fs::create_dir(&out).unwrap();
        let mut cmd = Command::cargo_bin("bam2nuc").unwrap();
        cmd.arg("-g").arg(genome).arg("--dir").arg(&out);
        if cram_ref {
            cmd.arg("--cram_ref").arg(&fx.reference);
        }
        cmd.arg(input).assert().success();
        read(&out.join("in.nucleotide_stats.txt"))
    };
    let from_bam = run(&fx.bam, "out_bam", false);
    let from_cram = run(&fx.cram, "out_cram", true);
    assert_eq!(
        String::from_utf8_lossy(&from_bam),
        String::from_utf8_lossy(&from_cram)
    );
    assert_cram_without_ref_fails("bam2nuc", &[Path::new("-g"), genome, &fx.cram]);
}

#[test]
fn filter_non_conversion_cram31_matches_bam() {
    let fx = fixture();
    let run = |input: &Path, cram_ref: bool| {
        let mut cmd = Command::cargo_bin("filter_non_conversion").unwrap();
        if cram_ref {
            cmd.arg("--cram_ref").arg(&fx.reference);
        }
        cmd.arg(input).assert().success();
        let dir = input.parent().unwrap();
        let report = String::from_utf8(read(&dir.join("in.non-conversion_filtering.txt")))
            .unwrap()
            .replace(&*input.to_string_lossy(), "IN");
        let report =
            report[..report.find("filter_non_conversion completed in").unwrap()].to_owned();
        (
            bam_records(&dir.join("in.nonCG_filtered.bam")),
            bam_records(&dir.join("in.nonCG_removed_seqs.bam")),
            report,
        )
    };
    let from_bam = run(&stage(&fx, "bam", &fx.bam), false);
    let cram = stage(&fx, "cram", &fx.cram);
    let from_cram = run(&cram, true);
    assert!(from_bam.2.contains("in total:\t6\n"), "{}", from_bam.2);
    assert!(from_bam.2.contains("\t1 (16.7%)"), "{}", from_bam.2);
    assert_eq!(from_bam.0.len(), 10);
    assert_eq!(from_bam, from_cram);
    assert_cram_without_ref_fails("filter_non_conversion", &[&cram]);
}

#[test]
fn methylation_consistency_cram31_matches_bam() {
    let fx = fixture();
    let run = |input: &Path, cram_ref: bool| {
        let mut cmd = Command::cargo_bin("methylation_consistency").unwrap();
        if cram_ref {
            cmd.arg("--cram_ref").arg(&fx.reference);
        }
        cmd.arg(input).assert().success();
        let dir = input.parent().unwrap();
        let bams = ["in_all_meth.bam", "in_all_unmeth.bam", "in_mixed_meth.bam"]
            .map(|o| bam_records(&dir.join(o)));
        (bams, read(&dir.join("in_consistency_report.txt")))
    };
    let from_bam = run(&stage(&fx, "bam", &fx.bam), false);
    let cram = stage(&fx, "cram", &fx.cram);
    let from_cram = run(&cram, true);
    // One pair (2 records) per bucket, at least.
    assert!(from_bam.0.iter().all(|b| b.len() >= 2), "{:?}", from_bam.0);
    assert_eq!(from_bam, from_cram);
    assert_cram_without_ref_fails("methylation_consistency", &[&cram]);
}

#[test]
fn filter_non_conversion_detects_cram_by_content_not_name() {
    let fx = fixture();
    let run = |input: &Path, cram_ref: bool| {
        let mut cmd = Command::cargo_bin("filter_non_conversion").unwrap();
        if cram_ref {
            cmd.arg("--cram_ref").arg(&fx.reference);
        }
        cmd.arg(input).assert().success();
        let dir = input.parent().unwrap();
        (
            bam_records(&dir.join("in.nonCG_filtered.bam")),
            bam_records(&dir.join("in.nonCG_removed_seqs.bam")),
        )
    };
    let from_bam = run(&stage(&fx, "bam", &fx.bam), false);
    let d = fx.dir.path().join("cram_named_bam");
    std::fs::create_dir(&d).unwrap();
    let cram_as_bam = d.join("in.bam");
    std::fs::copy(&fx.cram, &cram_as_bam).unwrap();
    assert_eq!(run(&cram_as_bam, true), from_bam);
}

#[test]
fn filter_non_conversion_keeps_its_errors_for_empty_and_missing_bam() {
    let dir = TempDir::new().unwrap();
    let empty = dir.path().join("empty.bam");
    std::fs::write(&empty, b"").unwrap();
    Command::cargo_bin("filter_non_conversion")
        .unwrap()
        .arg(&empty)
        .assert()
        .failure()
        .stderr(predicates::str::contains("appears to be truncated"));
    Command::cargo_bin("filter_non_conversion")
        .unwrap()
        .arg(dir.path().join("missing.bam"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("os error 2"));
}

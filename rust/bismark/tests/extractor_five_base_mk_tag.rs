//! The extractor output is the same whether or not 5-Base records carry the `mk:B:I` tag.

use std::collections::BTreeMap;
use std::fs;
use std::num::NonZeroUsize;
use std::path::Path;

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
use noodles_sam::alignment::record_buf::data::field::value::Array;
use noodles_sam::alignment::record_buf::{Cigar, Sequence};
use noodles_sam::header::record::value::Map;
use noodles_sam::header::record::value::map::ReferenceSequence;

fn header() -> Header {
    let mut header = Header::default();
    header.reference_sequences_mut().insert(
        BString::from("chr1"),
        Map::<ReferenceSequence>::new(NonZeroUsize::try_from(1_000).unwrap()),
    );
    header
}

fn record(
    flags: u16,
    start: usize,
    mate_start: usize,
    tlen: i32,
    xr: &[u8],
    mk: bool,
) -> BismarkRecord {
    let mut r = RecordBuf::default();
    *r.name_mut() = Some(BString::from("pair1"));
    *r.flags_mut() = Flags::from(flags);
    *r.reference_sequence_id_mut() = Some(0);
    *r.alignment_start_mut() = Some(Position::try_from(start).unwrap());
    *r.mate_reference_sequence_id_mut() = Some(0);
    *r.mate_alignment_start_mut() = Some(Position::try_from(mate_start).unwrap());
    *r.template_length_mut() = tlen;
    *r.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, 10)]);
    *r.sequence_mut() = Sequence::from(b"ACGTACGTAC".to_vec());
    let data = r.data_mut();
    data.insert(Tag::from(*b"XR"), Value::String(BString::from(xr.to_vec())));
    data.insert(Tag::from(*b"XG"), Value::String(BString::from("CT")));
    data.insert(
        Tag::from(*b"XM"),
        Value::String(BString::from("..Z...z.x.")),
    );
    if mk {
        data.insert(Tag::from(*b"mk"), Value::Array(Array::UInt32(vec![2, 0])));
    }
    BismarkRecord::from_noodles_record(r).unwrap()
}

fn extract(dir: &Path, mk: bool) -> BTreeMap<String, Vec<u8>> {
    fs::create_dir_all(dir).unwrap();
    let bam = dir.join("in.bam");
    let mut w = BamWriter::from_path(&bam, header()).unwrap();
    w.write_record(&record(99, 1, 21, 30, b"CT", mk)).unwrap();
    w.write_record(&record(147, 21, 1, -30, b"GA", mk)).unwrap();
    w.finish().unwrap();
    let out = dir.join("out");
    fs::create_dir_all(&out).unwrap();
    Command::cargo_bin("bismark_methylation_extractor")
        .unwrap()
        .arg(&bam)
        .arg("--paired-end")
        .arg("--output_dir")
        .arg(&out)
        .assert()
        .success();
    let mut files = BTreeMap::new();
    for e in fs::read_dir(&out).unwrap() {
        let e = e.unwrap();
        let name = e.file_name().to_string_lossy().into_owned();
        files.insert(name, fs::read(e.path()).unwrap());
    }
    files
}

#[test]
fn extractor_output_ignores_the_mk_tag() {
    let tmp = tempfile::tempdir().unwrap();
    let plain = extract(&tmp.path().join("plain"), false);
    let tagged = extract(&tmp.path().join("tagged"), true);
    assert!(!plain.is_empty());
    assert_eq!(
        plain.keys().collect::<Vec<_>>(),
        tagged.keys().collect::<Vec<_>>()
    );
    for (name, bytes) in &plain {
        let other = &tagged[name];
        if name.ends_with("_splitting_report.txt") {
            let strip = |b: &[u8]| -> String {
                String::from_utf8_lossy(b)
                    .lines()
                    .filter(|l| !l.contains("in.bam"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            assert_eq!(strip(bytes), strip(other), "{name}");
        } else {
            assert_eq!(bytes, other, "{name}");
        }
    }
}

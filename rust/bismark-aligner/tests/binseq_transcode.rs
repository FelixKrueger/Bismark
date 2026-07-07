//! #1025 Phase 2, integration tests for the BINSEQ (`.vbq`) → temp-FASTQ transcode.
//!
//! Builds real (tiny) VBQ files with the `binseq` writer (re-exported by the lib under
//! the `binseq-input` feature), then checks `transcode_binseq_to_fastq_se`/`_pe` produce
//! the expected `bqtools decode`-equivalent FASTQ, exercising the real
//! `vbq::MmapReader` serial decode path the format unit tests can't.
//!
//! The whole file is gated on `binseq-input` (the default/feature-off build does not
//! compile the `binseq` crate). Run via:
//!   `cargo test -p bismark-aligner --features binseq-input --test binseq_transcode`
//!
//! The VBQ fixtures use ACGT-only sequences because VBQ's 2-bit encoding cannot represent
//! `N` (an `N` would trigger binseq's invalid-nucleotide policy on encode). CBQ, by contrast,
//! natively encodes `N` (a 4-bit/N-index columnar format) — its whole reason to exist over
//! VBQ — so the CBQ suite additionally carries explicit **N round-trip** tests
//! (`cbq_preserves_n_bases_*`). Those pin the exact defect that parked Phase 3a: binseq 0.9.2
//! silently mis-decoded `N` → `A` in release (panic in debug), fixed upstream in 0.9.3
//! (ArcInstitute/binseq#94). Quality is stored verbatim (raw ASCII).
#![cfg(feature = "binseq-input")]

use std::io::Read;
use std::path::Path;

use bismark_aligner::binseq::SequencingRecordBuilder;
use bismark_aligner::binseq::write::{BinseqWriterBuilder, Format};
use bismark_aligner::binseq_decode;

/// Write a single-end VBQ with quality + headers from `(header, seq, qual)` triples.
fn write_vbq_se(path: &Path, records: &[(&str, &[u8], &[u8])]) {
    let mut w = BinseqWriterBuilder::new(Format::Vbq)
        .paired(false)
        .quality(true)
        .headers(true)
        .build(std::fs::File::create(path).unwrap())
        .unwrap();
    for (h, s, q) in records {
        let rec = SequencingRecordBuilder::default()
            .s_header(h.as_bytes())
            .s_seq(s)
            .s_qual(q)
            .build()
            .unwrap();
        assert!(w.push(rec).unwrap(), "record skipped (invalid nucleotide?)");
    }
    w.finish().unwrap();
}

/// Write a paired-end VBQ with quality + headers. Each tuple is
/// `(s_header, s_seq, s_qual, x_header, x_seq, x_qual)`, one record carries both mates.
#[allow(clippy::type_complexity)]
fn write_vbq_pe(path: &Path, records: &[(&str, &[u8], &[u8], &str, &[u8], &[u8])]) {
    let mut w = BinseqWriterBuilder::new(Format::Vbq)
        .paired(true)
        .quality(true)
        .headers(true)
        .build(std::fs::File::create(path).unwrap())
        .unwrap();
    for (sh, ss, sq, xh, xs, xq) in records {
        let rec = SequencingRecordBuilder::default()
            .s_header(sh.as_bytes())
            .s_seq(ss)
            .s_qual(sq)
            .x_header(xh.as_bytes())
            .x_seq(xs)
            .x_qual(xq)
            .build()
            .unwrap();
        assert!(w.push(rec).unwrap(), "paired record skipped");
    }
    w.finish().unwrap();
}

fn read_to_bytes(path: &Path) -> Vec<u8> {
    let mut v = Vec::new();
    std::fs::File::open(path)
        .unwrap()
        .read_to_end(&mut v)
        .unwrap();
    v
}

#[test]
fn se_vbq_transcodes_to_expected_fastq_and_names_stem() {
    let dir = tempfile::tempdir().unwrap();
    let vbq = dir.path().join("sample.vbq");
    // A space-bearing header (exercises that the full stored id is emitted verbatim) and
    // a verbatim ASCII quality string.
    write_vbq_se(
        &vbq,
        &[
            ("read1 some comment", b"ACGTACGT", b"IIIIIIII"),
            ("read2", b"TTGGCCAA", b"#%')+-/1"),
        ],
    );

    let out = binseq_decode::transcode_binseq_to_fastq_se(&vbq, dir.path()).unwrap();
    // R6: temp named `<stem>.fastq` so the downstream output stem == the equivalent
    // `bqtools decode > sample.fastq` run's stem (file_stem strips `.vbq`).
    assert_eq!(out.file_name().unwrap().to_str().unwrap(), "sample.fastq");
    assert_eq!(
        read_to_bytes(&out),
        b"@read1 some comment\nACGTACGT\n+\nIIIIIIII\n@read2\nTTGGCCAA\n+\n#%')+-/1\n".to_vec()
    );
}

#[test]
fn pe_vbq_splits_mates_into_two_files() {
    let dir = tempfile::tempdir().unwrap();
    let vbq = dir.path().join("pe.vbq");
    write_vbq_pe(
        &vbq,
        &[
            (
                "p1",
                b"ACGTACGT",
                b"IIIIIIII",
                "p1",
                b"TTTTGGGG",
                b"JJJJJJJJ",
            ),
            (
                "p2",
                b"AAAACCCC",
                b"$$$$%%%%",
                "p2",
                b"GGGGTTTT",
                b"&&&&((((",
            ),
        ],
    );

    let (r1, r2) = binseq_decode::transcode_binseq_to_fastq_pe(&vbq, dir.path()).unwrap();
    assert_eq!(r1.file_name().unwrap().to_str().unwrap(), "pe_1.fastq");
    assert_eq!(r2.file_name().unwrap().to_str().unwrap(), "pe_2.fastq");
    // R5: primary → R1, extended → R2 (one record carries both mates; no collation).
    assert_eq!(
        read_to_bytes(&r1),
        b"@p1\nACGTACGT\n+\nIIIIIIII\n@p2\nAAAACCCC\n+\n$$$$%%%%\n".to_vec()
    );
    assert_eq!(
        read_to_bytes(&r2),
        b"@p1\nTTTTGGGG\n+\nJJJJJJJJ\n@p2\nGGGGTTTT\n+\n&&&&((((\n".to_vec()
    );
}

#[test]
fn se_vbq_without_quality_is_rejected_fail_loud() {
    // R2: D2 reject enforced at the FILE-HEADER level. A VBQ written with quality(false)
    // would have `bqtools decode` `?`-fill it; we reject instead (never-silent).
    let dir = tempfile::tempdir().unwrap();
    let vbq = dir.path().join("noqual.vbq");
    let mut w = BinseqWriterBuilder::new(Format::Vbq)
        .paired(false)
        .quality(false)
        .headers(true)
        .build(std::fs::File::create(&vbq).unwrap())
        .unwrap();
    let rec = SequencingRecordBuilder::default()
        .s_header(b"r")
        .s_seq(b"ACGT")
        .build()
        .unwrap();
    w.push(rec).unwrap();
    w.finish().unwrap();

    let err = binseq_decode::transcode_binseq_to_fastq_se(&vbq, dir.path()).unwrap_err();
    assert!(
        format!("{err}").contains("no per-read quality"),
        "expected a quality reject, got: {err}"
    );
}

#[test]
fn se_vbq_without_headers_is_rejected_fail_loud() {
    // R2: a VBQ without headers would emit synthesized numeric QNAMEs; reject instead.
    let dir = tempfile::tempdir().unwrap();
    let vbq = dir.path().join("noheaders.vbq");
    let mut w = BinseqWriterBuilder::new(Format::Vbq)
        .paired(false)
        .quality(true)
        .headers(false)
        .build(std::fs::File::create(&vbq).unwrap())
        .unwrap();
    let rec = SequencingRecordBuilder::default()
        .s_seq(b"ACGT")
        .s_qual(b"IIII")
        .build()
        .unwrap();
    w.push(rec).unwrap();
    w.finish().unwrap();

    let err = binseq_decode::transcode_binseq_to_fastq_se(&vbq, dir.path()).unwrap_err();
    assert!(
        format!("{err}").contains("no per-read names"),
        "expected a header/name reject, got: {err}"
    );
}

#[test]
fn decode_is_deterministic() {
    // R1: the serial `vbq::MmapReader` block iteration is file-order, so decoding the
    // SAME file twice yields byte-identical FASTQ (the parallel path would not).
    let dir = tempfile::tempdir().unwrap();
    let vbq = dir.path().join("det.vbq");
    let recs: Vec<(&str, &[u8], &[u8])> = (0..50)
        .map(|_| ("r", b"ACGTACGTACGT".as_slice(), b"IIIIIIIIIIII".as_slice()))
        .collect();
    write_vbq_se(&vbq, &recs);

    let a = dir.path().join("a");
    let b = dir.path().join("b");
    let out_a = binseq_decode::transcode_binseq_to_fastq_se(&vbq, &a).unwrap();
    let out_b = binseq_decode::transcode_binseq_to_fastq_se(&vbq, &b).unwrap();
    assert_eq!(read_to_bytes(&out_a), read_to_bytes(&out_b));
}

#[test]
fn is_paired_classifies_se_and_pe() {
    let dir = tempfile::tempdir().unwrap();
    let se = dir.path().join("se.vbq");
    write_vbq_se(&se, &[("r", b"ACGT", b"IIII")]);
    assert!(
        !binseq_decode::is_paired(&se).unwrap(),
        "SE VBQ → not paired"
    );

    let pe = dir.path().join("pe.vbq");
    write_vbq_pe(&pe, &[("r", b"ACGT", b"IIII", "r", b"TTGG", b"JJJJ")]);
    assert!(binseq_decode::is_paired(&pe).unwrap(), "PE VBQ → paired");
}

#[test]
fn empty_se_vbq_yields_empty_fastq() {
    // A header+quality-bearing VBQ with zero records transcodes to an empty FASTQ, which
    // flows into the existing graceful-empty handling downstream.
    let dir = tempfile::tempdir().unwrap();
    let vbq = dir.path().join("empty.vbq");
    write_vbq_se(&vbq, &[]);
    let out = binseq_decode::transcode_binseq_to_fastq_se(&vbq, dir.path()).unwrap();
    assert_eq!(
        read_to_bytes(&out),
        Vec::<u8>::new(),
        "empty VBQ → empty FASTQ"
    );
}

#[test]
fn multi_block_vbq_decodes_all_records_in_order() {
    // The production path: a real `.vbq` spans many 128 KB blocks, so the
    // `while read_block_into(&mut block)` loop iterates with block reuse + cross-block
    // index continuity. The other fixtures are single-block, so force MULTIPLE blocks
    // with a tiny `block_size` and enough records, then assert every record is decoded
    // exactly once, in file order (no dropped/duplicated/reordered record at a boundary).
    let dir = tempfile::tempdir().unwrap();
    let vbq = dir.path().join("multiblock.vbq");

    const N: usize = 200;
    let seq: &[u8] = b"ACGTACGTACGTACGTACGTACGTACGTACGT"; // 32 bp, exact under 2-bit
    let qual: &[u8] = b"IIIIIIIIIIIIIIIIIIIIIIIIIIIIIIII"; // 32

    // block_size = 256 bytes holds only a few ~70-byte records → ~tens of blocks for N=200.
    let mut w = BinseqWriterBuilder::new(Format::Vbq)
        .paired(false)
        .quality(true)
        .headers(true)
        .block_size(256)
        .build(std::fs::File::create(&vbq).unwrap())
        .unwrap();
    let mut expected = Vec::new();
    for i in 0..N {
        let header = format!("read{i}");
        let rec = SequencingRecordBuilder::default()
            .s_header(header.as_bytes())
            .s_seq(seq)
            .s_qual(qual)
            .build()
            .unwrap();
        assert!(w.push(rec).unwrap());
        expected.extend_from_slice(b"@");
        expected.extend_from_slice(header.as_bytes());
        expected.push(b'\n');
        expected.extend_from_slice(seq);
        expected.extend_from_slice(b"\n+\n");
        expected.extend_from_slice(qual);
        expected.push(b'\n');
    }
    w.finish().unwrap();

    let out = binseq_decode::transcode_binseq_to_fastq_se(&vbq, dir.path()).unwrap();
    let got = read_to_bytes(&out);
    // Byte-exact over all N records ⇒ no record dropped, duplicated, or reordered across
    // the many block boundaries, and every header (`read0`..`read199`) is in file order.
    assert_eq!(
        got, expected,
        "multi-block decode must be complete + in file order"
    );
    assert_eq!(
        got.iter().filter(|&&b| b == b'@').count(),
        N,
        "exactly N headers"
    );
}

// ===========================================================================
// #1025 Phase 3, CBQ (`.cbq`) transcode. Same contract as VBQ ("identical to the
// equivalent `bqtools decode` run"), driven through the columnar `cbq::Reader` serial
// path instead of the verbose `vbq::MmapReader`. The high-level `BinseqWriterBuilder`
// writes both formats, so the only difference from the VBQ helpers is `Format::Cbq`.
// ===========================================================================

/// Write a single-end CBQ with quality + headers from `(header, seq, qual)` triples.
fn write_cbq_se(path: &Path, records: &[(&str, &[u8], &[u8])]) {
    let mut w = BinseqWriterBuilder::new(Format::Cbq)
        .paired(false)
        .quality(true)
        .headers(true)
        .build(std::fs::File::create(path).unwrap())
        .unwrap();
    for (h, s, q) in records {
        let rec = SequencingRecordBuilder::default()
            .s_header(h.as_bytes())
            .s_seq(s)
            .s_qual(q)
            .build()
            .unwrap();
        assert!(w.push(rec).unwrap(), "record skipped (invalid nucleotide?)");
    }
    w.finish().unwrap();
}

/// Write a paired-end CBQ with quality + headers. Each tuple is
/// `(s_header, s_seq, s_qual, x_header, x_seq, x_qual)`, one record carries both mates.
#[allow(clippy::type_complexity)]
fn write_cbq_pe(path: &Path, records: &[(&str, &[u8], &[u8], &str, &[u8], &[u8])]) {
    let mut w = BinseqWriterBuilder::new(Format::Cbq)
        .paired(true)
        .quality(true)
        .headers(true)
        .build(std::fs::File::create(path).unwrap())
        .unwrap();
    for (sh, ss, sq, xh, xs, xq) in records {
        let rec = SequencingRecordBuilder::default()
            .s_header(sh.as_bytes())
            .s_seq(ss)
            .s_qual(sq)
            .x_header(xh.as_bytes())
            .x_seq(xs)
            .x_qual(xq)
            .build()
            .unwrap();
        assert!(w.push(rec).unwrap(), "paired record skipped");
    }
    w.finish().unwrap();
}

#[test]
fn se_cbq_transcodes_to_expected_fastq_and_names_stem() {
    let dir = tempfile::tempdir().unwrap();
    let cbq = dir.path().join("sample.cbq");
    write_cbq_se(
        &cbq,
        &[
            ("read1 some comment", b"ACGTACGT", b"IIIIIIII"),
            ("read2", b"TTGGCCAA", b"#%')+-/1"),
        ],
    );

    let out = binseq_decode::transcode_binseq_to_fastq_se(&cbq, dir.path()).unwrap();
    // R6: temp named `<stem>.fastq` (file_stem strips `.cbq`).
    assert_eq!(out.file_name().unwrap().to_str().unwrap(), "sample.fastq");
    assert_eq!(
        read_to_bytes(&out),
        b"@read1 some comment\nACGTACGT\n+\nIIIIIIII\n@read2\nTTGGCCAA\n+\n#%')+-/1\n".to_vec()
    );
}

#[test]
fn pe_cbq_splits_mates_into_two_files() {
    let dir = tempfile::tempdir().unwrap();
    let cbq = dir.path().join("pe.cbq");
    write_cbq_pe(
        &cbq,
        &[
            (
                "p1",
                b"ACGTACGT",
                b"IIIIIIII",
                "p1",
                b"TTTTGGGG",
                b"JJJJJJJJ",
            ),
            (
                "p2",
                b"AAAACCCC",
                b"$$$$%%%%",
                "p2",
                b"GGGGTTTT",
                b"&&&&((((",
            ),
        ],
    );

    let (r1, r2) = binseq_decode::transcode_binseq_to_fastq_pe(&cbq, dir.path()).unwrap();
    assert_eq!(r1.file_name().unwrap().to_str().unwrap(), "pe_1.fastq");
    assert_eq!(r2.file_name().unwrap().to_str().unwrap(), "pe_2.fastq");
    // R5: primary → R1, extended → R2 (one record carries both mates; no collation).
    assert_eq!(
        read_to_bytes(&r1),
        b"@p1\nACGTACGT\n+\nIIIIIIII\n@p2\nAAAACCCC\n+\n$$$$%%%%\n".to_vec()
    );
    assert_eq!(
        read_to_bytes(&r2),
        b"@p1\nTTTTGGGG\n+\nJJJJJJJJ\n@p2\nGGGGTTTT\n+\n&&&&((((\n".to_vec()
    );
}

#[test]
fn se_cbq_without_quality_is_rejected_fail_loud() {
    // R2: D2 reject enforced at the FILE-HEADER level (CBQ uses `has_qualities()`).
    let dir = tempfile::tempdir().unwrap();
    let cbq = dir.path().join("noqual.cbq");
    let mut w = BinseqWriterBuilder::new(Format::Cbq)
        .paired(false)
        .quality(false)
        .headers(true)
        .build(std::fs::File::create(&cbq).unwrap())
        .unwrap();
    let rec = SequencingRecordBuilder::default()
        .s_header(b"r")
        .s_seq(b"ACGT")
        .build()
        .unwrap();
    w.push(rec).unwrap();
    w.finish().unwrap();

    let err = binseq_decode::transcode_binseq_to_fastq_se(&cbq, dir.path()).unwrap_err();
    assert!(
        format!("{err}").contains("no per-read quality"),
        "expected a quality reject, got: {err}"
    );
}

#[test]
fn se_cbq_without_headers_is_rejected_fail_loud() {
    // R2: a CBQ without headers would emit synthesized numeric QNAMEs; reject instead.
    let dir = tempfile::tempdir().unwrap();
    let cbq = dir.path().join("noheaders.cbq");
    let mut w = BinseqWriterBuilder::new(Format::Cbq)
        .paired(false)
        .quality(true)
        .headers(false)
        .build(std::fs::File::create(&cbq).unwrap())
        .unwrap();
    let rec = SequencingRecordBuilder::default()
        .s_seq(b"ACGT")
        .s_qual(b"IIII")
        .build()
        .unwrap();
    w.push(rec).unwrap();
    w.finish().unwrap();

    let err = binseq_decode::transcode_binseq_to_fastq_se(&cbq, dir.path()).unwrap_err();
    assert!(
        format!("{err}").contains("no per-read names"),
        "expected a header/name reject, got: {err}"
    );
}

#[test]
fn cbq_is_paired_classifies_se_and_pe() {
    let dir = tempfile::tempdir().unwrap();
    let se = dir.path().join("se.cbq");
    write_cbq_se(&se, &[("r", b"ACGT", b"IIII")]);
    assert!(
        !binseq_decode::is_paired(&se).unwrap(),
        "SE CBQ → not paired"
    );

    let pe = dir.path().join("pe.cbq");
    write_cbq_pe(&pe, &[("r", b"ACGT", b"IIII", "r", b"TTGG", b"JJJJ")]);
    assert!(binseq_decode::is_paired(&pe).unwrap(), "PE CBQ → paired");
}

#[test]
fn empty_se_cbq_yields_empty_fastq() {
    let dir = tempfile::tempdir().unwrap();
    let cbq = dir.path().join("empty.cbq");
    write_cbq_se(&cbq, &[]);
    let out = binseq_decode::transcode_binseq_to_fastq_se(&cbq, dir.path()).unwrap();
    assert_eq!(
        read_to_bytes(&out),
        Vec::<u8>::new(),
        "empty CBQ → empty FASTQ"
    );
}

#[test]
fn cbq_decode_is_deterministic() {
    // The serial `cbq::Reader` block iteration is file-order, so decoding the SAME file
    // twice yields byte-identical FASTQ (the parallel path would not).
    let dir = tempfile::tempdir().unwrap();
    let cbq = dir.path().join("det.cbq");
    let recs: Vec<(&str, &[u8], &[u8])> = (0..50)
        .map(|_| ("r", b"ACGTACGTACGT".as_slice(), b"IIIIIIIIIIII".as_slice()))
        .collect();
    write_cbq_se(&cbq, &recs);

    let a = dir.path().join("a");
    let b = dir.path().join("b");
    let out_a = binseq_decode::transcode_binseq_to_fastq_se(&cbq, &a).unwrap();
    let out_b = binseq_decode::transcode_binseq_to_fastq_se(&cbq, &b).unwrap();
    assert_eq!(read_to_bytes(&out_a), read_to_bytes(&out_b));
}

#[test]
fn multi_block_cbq_decodes_all_records_in_order() {
    // The production path: a real `.cbq` spans many blocks, so the `while read_block()`
    // loop iterates across block boundaries. Force MULTIPLE blocks with a tiny
    // `block_size` and enough records, then assert every record is decoded exactly once,
    // in file order (no dropped/duplicated/reordered record at a boundary, and the
    // cross-block `cumulative_records` bookkeeping stays correct).
    let dir = tempfile::tempdir().unwrap();
    let cbq = dir.path().join("multiblock.cbq");

    const N: usize = 200;
    let seq: &[u8] = b"ACGTACGTACGTACGTACGTACGTACGTACGT"; // 32 bp
    let qual: &[u8] = b"IIIIIIIIIIIIIIIIIIIIIIIIIIIIIIII"; // 32

    let mut w = BinseqWriterBuilder::new(Format::Cbq)
        .paired(false)
        .quality(true)
        .headers(true)
        .block_size(256)
        .build(std::fs::File::create(&cbq).unwrap())
        .unwrap();
    let mut expected = Vec::new();
    for i in 0..N {
        let header = format!("read{i}");
        let rec = SequencingRecordBuilder::default()
            .s_header(header.as_bytes())
            .s_seq(seq)
            .s_qual(qual)
            .build()
            .unwrap();
        assert!(w.push(rec).unwrap());
        expected.extend_from_slice(b"@");
        expected.extend_from_slice(header.as_bytes());
        expected.push(b'\n');
        expected.extend_from_slice(seq);
        expected.extend_from_slice(b"\n+\n");
        expected.extend_from_slice(qual);
        expected.push(b'\n');
    }
    w.finish().unwrap();

    let out = binseq_decode::transcode_binseq_to_fastq_se(&cbq, dir.path()).unwrap();
    let got = read_to_bytes(&out);
    assert_eq!(
        got, expected,
        "multi-block CBQ decode must be complete + in file order"
    );
    assert_eq!(
        got.iter().filter(|&&b| b == b'@').count(),
        N,
        "exactly N headers"
    );
}

#[test]
fn cbq_preserves_n_bases_se() {
    // THE regression test that parked Phase 3a. CBQ natively encodes `N` (its whole reason
    // to exist over VBQ). binseq 0.9.2's streaming CBQ reader silently mis-decoded every `N`
    // as `A` in release builds (and panicked in debug) — a no-call turned into fabricated
    // signal, the worst failure class for a methylation caller. Fixed upstream in 0.9.3
    // (ArcInstitute/binseq#94). Assert every `N` survives VERBATIM at every position
    // (leading, interior, trailing, all-N) — never `A`.
    let dir = tempfile::tempdir().unwrap();
    let cbq = dir.path().join("ns.cbq");
    write_cbq_se(
        &cbq,
        &[
            ("nlead", b"NACGTACGT", b"IIIIIIIII"),
            ("nmid", b"ACGTNACGT", b"#%')+-/13"),
            ("ntrail", b"ACGTACGN", b"JJJJJJJJ"),
            ("nall", b"NNNNNNNN", b"IIIIIIII"),
        ],
    );

    let out = binseq_decode::transcode_binseq_to_fastq_se(&cbq, dir.path()).unwrap();
    let expected = b"@nlead\nNACGTACGT\n+\nIIIIIIIII\n\
                     @nmid\nACGTNACGT\n+\n#%')+-/13\n\
                     @ntrail\nACGTACGN\n+\nJJJJJJJJ\n\
                     @nall\nNNNNNNNN\n+\nIIIIIIII\n"
        .to_vec();
    let got = read_to_bytes(&out);
    assert_eq!(
        got, expected,
        "CBQ N bases must round-trip verbatim (0.9.3 fix; 0.9.2 gave N->A)"
    );
    // Belt-and-braces vs the exact 0.9.2 bug: the input had 11 `N`s and 0 stray `A`s in the
    // N-only record — count them in the decoded sequence lines to prove none silently became `A`.
    let seq_lines: Vec<&[u8]> = got
        .split(|&b| b == b'\n')
        .enumerate()
        .filter(|(i, _)| i % 4 == 1) // every 2nd of each 4-line record = the SEQ line
        .map(|(_, l)| l)
        .collect();
    let n_count: usize = seq_lines.iter().map(|l| bytecount(l, b'N')).sum();
    assert_eq!(
        n_count,
        1 + 1 + 1 + 8,
        "all 11 N bases preserved, not decoded to A"
    );
}

#[test]
fn cbq_preserves_n_bases_pe() {
    // Same N-preservation guarantee across both mates of a paired CBQ.
    let dir = tempfile::tempdir().unwrap();
    let cbq = dir.path().join("ns_pe.cbq");
    write_cbq_pe(
        &cbq,
        &[("np1", b"NNACGT", b"IIIIII", "np1", b"ACGTNN", b"JJJJJJ")],
    );

    let (r1, r2) = binseq_decode::transcode_binseq_to_fastq_pe(&cbq, dir.path()).unwrap();
    assert_eq!(
        read_to_bytes(&r1),
        b"@np1\nNNACGT\n+\nIIIIII\n".to_vec(),
        "R1 N bases must round-trip verbatim"
    );
    assert_eq!(
        read_to_bytes(&r2),
        b"@np1\nACGTNN\n+\nJJJJJJ\n".to_vec(),
        "R2 N bases must round-trip verbatim"
    );
}

fn bytecount(hay: &[u8], needle: u8) -> usize {
    hay.iter().filter(|&&b| b == needle).count()
}

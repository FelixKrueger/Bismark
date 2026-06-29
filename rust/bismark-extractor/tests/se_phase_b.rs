//! Phase B unit tests — kernel + routing + output map + state machinery.
//!
//! Organisation mirrors the plan §7.1 test list, grouped by module under
//! `#[cfg(test)] mod` blocks. Heavy synthetic-record construction sits in
//! a `helpers` module shared by all groups.
//!
//! End-to-end smoke that runs the binary on a real BAM lives at
//! `tests/se_phase_b_smoke.rs` (does not require Perl).

// Test names intentionally mirror the SPEC §8.1 / plan §7.1 labels, which
// embed uppercase XM byte names (`Z`, `X`, `H`, `R2`) for at-a-glance
// mapping to the documented test surface.
#![allow(non_snake_case)]

use std::fs;

use assert_cmd::Command;
use bismark_extractor::call::{CytosineContext, MethCall, classify_xm_byte, extract_calls};
use bismark_extractor::cli::{Cli, OutputMode, ResolvedConfig};
use bismark_extractor::error::BismarkExtractorError;
use bismark_extractor::header::build_chr_name_table;
use bismark_extractor::mbias::{MbiasPos, MbiasTable};
use bismark_extractor::output::{
    BISMARK_VERSION, OutputFileMap, SPLIT_FILE_HEADER, SplittingReport, write_splitting_report,
};
use bismark_extractor::pipeline::derive_basename;
use bismark_extractor::route::route_call;
use bismark_extractor::state::ExtractState;
use bismark_io::{BismarkStrand, ReadIdentity};
use clap::Parser;
use predicates::prelude::PredicateBooleanExt;

// ─────────────────────────────────────────────────────────────────────────
// Test helpers — synthetic BismarkRecord construction
// ─────────────────────────────────────────────────────────────────────────

mod helpers {
    use bismark_io::BismarkRecord;
    use bstr::BString;
    use noodles_sam::Header;
    use noodles_sam::alignment::record::cigar::Op;
    use noodles_sam::alignment::record::cigar::op::Kind;
    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::data::field::Value;
    use noodles_sam::alignment::record_buf::{Cigar, RecordBuf, Sequence};
    use noodles_sam::header::record::value::Map;
    use noodles_sam::header::record::value::map::ReferenceSequence;
    use std::num::NonZeroUsize;

    /// Build a synthetic `BismarkRecord` with the given XR/XG/XM, sequence,
    /// alignment_start (1-based), and CIGAR ops. FLAG defaults to 0
    /// (single-end, mapped, plus-strand) unless overridden.
    #[allow(clippy::too_many_arguments)]
    pub fn synth(
        xr: &[u8],
        xg: &[u8],
        xm: &[u8],
        seq: &[u8],
        alignment_start: usize,
        cigar_ops: &[(Kind, usize)],
        flags: u16,
        qname: &[u8],
        refid: usize,
    ) -> BismarkRecord {
        use noodles_core::Position;
        use noodles_sam::alignment::record::Flags;
        let mut record = RecordBuf::default();
        *record.flags_mut() = Flags::from(flags);
        *record.sequence_mut() = Sequence::from(seq.to_vec());
        *record.alignment_start_mut() = Some(Position::try_from(alignment_start).unwrap());
        *record.reference_sequence_id_mut() = Some(refid);
        *record.cigar_mut() = Cigar::from(
            cigar_ops
                .iter()
                .map(|(k, n)| Op::new(*k, *n))
                .collect::<Vec<_>>(),
        );
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
        BismarkRecord::from_noodles_record(record).expect("synth produces a valid BismarkRecord")
    }

    /// Most-common SE OT record builder for tests that don't care about FLAG/QNAME.
    pub fn ot_record(xm: &[u8], seq: &[u8]) -> BismarkRecord {
        synth(
            b"CT",
            b"CT",
            xm,
            seq,
            100,
            &[(Kind::Match, xm.len())],
            0,
            b"read1",
            0,
        )
    }

    /// OB pair-strand SE record. Used for the orientation-invariant test.
    pub fn ob_record(xm: &[u8], seq: &[u8]) -> BismarkRecord {
        synth(
            b"CT",
            b"GA",
            xm,
            seq,
            100,
            &[(Kind::Match, xm.len())],
            0,
            b"read1",
            0,
        )
    }

    /// Build a header containing a single @SQ named `name` (with length 1000),
    /// so `reference_sequence_id == 0` resolves to `name`.
    pub fn header_with_single_chr(name: &str) -> Header {
        let mut header = Header::default();
        header.reference_sequences_mut().insert(
            BString::from(name.as_bytes().to_vec()),
            Map::<ReferenceSequence>::new(NonZeroUsize::new(1000).unwrap()),
        );
        header
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 1. classify_xm_byte tests
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn classify_xm_byte_classifies_all_six_methylation_bytes() {
    use bismark_extractor::call::XmClassification;
    for (byte, expected_ctx, expected_meth) in [
        (b'Z', CytosineContext::CpG, true),
        (b'z', CytosineContext::CpG, false),
        (b'X', CytosineContext::CHG, true),
        (b'x', CytosineContext::CHG, false),
        (b'H', CytosineContext::CHH, true),
        (b'h', CytosineContext::CHH, false),
    ] {
        match classify_xm_byte(byte, 1, "r").unwrap() {
            XmClassification::Call(ctx, meth) => {
                assert_eq!(ctx, expected_ctx, "byte {}", byte as char);
                assert_eq!(meth, expected_meth, "byte {}", byte as char);
            }
            _ => panic!("byte {} should classify as Call", byte as char),
        }
    }
}

#[test]
fn classify_xm_byte_skips_U_u_dot() {
    use bismark_extractor::call::XmClassification;
    for byte in [b'U', b'u'] {
        assert!(matches!(
            classify_xm_byte(byte, 1, "r").unwrap(),
            XmClassification::SkipUnknownContext
        ));
    }
    assert!(matches!(
        classify_xm_byte(b'.', 1, "r").unwrap(),
        XmClassification::SkipNonCytosine
    ));
}

#[test]
fn classify_xm_byte_rejects_invalid() {
    let err = classify_xm_byte(b'Q', 42, "myread").unwrap_err();
    match err {
        BismarkExtractorError::InvalidXmByte {
            byte,
            byte_char,
            ref_pos,
            read_id,
        } => {
            assert_eq!(byte, b'Q');
            assert_eq!(byte_char, 'Q');
            assert_eq!(ref_pos, 42);
            assert_eq!(read_id, "myread");
        }
        _ => panic!("expected InvalidXmByte"),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 2. extract_calls tests
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn extract_calls_classifies_all_six_methylation_bytes() {
    // 6-base read on OT strand with all 6 methylation bytes in order.
    let record = helpers::ot_record(b"ZzXxHh", b"ACGTAC");
    let calls = extract_calls(&record, 0, 0, /*mbias_only_silence=*/ false).unwrap();
    assert_eq!(calls.len(), 6);
    assert_eq!(calls[0].xm_byte, b'Z');
    assert_eq!(calls[0].context, CytosineContext::CpG);
    assert!(calls[0].methylated);
    assert_eq!(calls[5].xm_byte, b'h');
    assert_eq!(calls[5].context, CytosineContext::CHH);
    assert!(!calls[5].methylated);
}

#[test]
fn extract_calls_respects_ignore_5p() {
    let record = helpers::ot_record(b"ZzXxHh", b"ACGTAC");
    let calls = extract_calls(
        &record, /*ignore_5p=*/ 3, 0, /*mbias_only_silence=*/ false,
    )
    .unwrap();
    // First 3 positions skipped → 3 calls remain (absolute positions 3,4,5
    // rebased to 0,1,2 per #876 Bug B fix at call.rs:177).
    assert_eq!(calls.len(), 3);
    assert_eq!(
        calls[0].read_pos, 0,
        "first surviving call must rebase to 0 (not absolute 3) — \
         matches Perl substr($meth_call, $ignore) at :1627"
    );
    assert_eq!(calls[0].xm_byte, b'x');
}

#[test]
fn extract_calls_respects_ignore_3p() {
    let record = helpers::ot_record(b"ZzXxHh", b"ACGTAC");
    let calls = extract_calls(
        &record, 0, /*ignore_3p=*/ 3, /*mbias_only_silence=*/ false,
    )
    .unwrap();
    // Last 3 positions skipped → 3 calls remain (positions 0,1,2)
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[2].read_pos, 2);
    assert_eq!(calls[2].xm_byte, b'X');
}

#[test]
fn extract_calls_walks_cigar_with_indels() {
    use noodles_sam::alignment::record::cigar::op::Kind;
    // 5M2D5M — read 10 bases, ref 12 bases (deletion advances ref only).
    let record = helpers::synth(
        b"CT",
        b"CT",
        b"ZZZZZZZZZZ",
        b"AAAAAAAAAA",
        100,
        &[(Kind::Match, 5), (Kind::Deletion, 2), (Kind::Match, 5)],
        0,
        b"r",
        0,
    );
    let calls = extract_calls(&record, 0, 0, /*mbias_only_silence=*/ false).unwrap();
    assert_eq!(calls.len(), 10);
    assert_eq!(calls[4].ref_pos, 104);
    // Position 5 of the read jumps to ref_pos 107 (deletion-skip).
    assert_eq!(calls[5].read_pos, 5);
    assert_eq!(calls[5].ref_pos, 107);
}

#[test]
fn extract_calls_walks_cigar_with_soft_clips() {
    use noodles_sam::alignment::record::cigar::op::Kind;
    // 2S8M — first 2 read positions are soft-clipped; emitted calls start
    // at read_pos == 2 (rev 1 invariant: iter_aligned counts soft-clip
    // positions in read_pos).
    let record = helpers::synth(
        b"CT",
        b"CT",
        b"..ZZZZZZZZ", // 10 XM bytes; first 2 are `.` (Bismark convention for soft-clip)
        b"AAAAAAAAAA",
        100,
        &[(Kind::SoftClip, 2), (Kind::Match, 8)],
        0,
        b"r",
        0,
    );
    let calls = extract_calls(&record, 0, 0, /*mbias_only_silence=*/ false).unwrap();
    // 8 aligned positions emitted (soft-clip filtered by iter_aligned).
    assert_eq!(calls.len(), 8);
    // First emitted call's read_pos == 2, NOT 0 (rev 1 correction).
    assert_eq!(calls[0].read_pos, 2);
    assert_eq!(calls[0].ref_pos, 100);
}

#[test]
fn extract_calls_empty_xm_yields_empty_vec() {
    let record = helpers::ot_record(b"......", b"ACGTAC");
    let calls = extract_calls(&record, 0, 0, /*mbias_only_silence=*/ false).unwrap();
    assert!(calls.is_empty());
}

#[test]
fn extract_calls_minus_strand_orients_5prime() {
    // OB strand record. `iter_aligned` reverses XM iteration so the first
    // emitted (read_pos_5p=0) call corresponds to the LAST BAM-stored byte.
    // Critical orientation invariant: flipping this corrupts every `-` strand
    // record's M-bias positions end-to-end.
    //
    // Fixture: XM `....Z` (BAM-stored position 4 = 'Z'). On OB strand the
    // first emitted call should have read_pos_5p=0 with xm_byte='Z'.
    let record = helpers::ob_record(b"....Z", b"ACGTC");
    let calls = extract_calls(&record, 0, 0, /*mbias_only_silence=*/ false).unwrap();
    assert_eq!(calls.len(), 1, "only one methylation byte in fixture");
    assert_eq!(calls[0].read_pos, 0, "5'-end of sequenced read");
    assert_eq!(calls[0].xm_byte, b'Z');
    // Ref position is alignment_start + 4 (BAM read_pos 4 maps to ref 104).
    assert_eq!(calls[0].ref_pos, 104);
}

#[test]
fn extract_calls_minus_strand_orients_both_calls() {
    // Stronger orientation check: BAM-stored XM `Zh...` on OB strand.
    // After 5'-orientation, emission order is `.`, `.`, `.`, h, Z.
    // (Non-call bytes skipped.) Two calls expected: at read_pos_5p=3 (h, CHH-unmeth)
    // and read_pos_5p=4 (Z, CpG-meth).
    let record = helpers::ob_record(b"Zh...", b"ACGTC");
    let calls = extract_calls(&record, 0, 0, /*mbias_only_silence=*/ false).unwrap();
    assert_eq!(calls.len(), 2);
    // First call in 5'-order corresponds to BAM-stored position 1 ('h').
    assert_eq!(calls[0].read_pos, 3);
    assert_eq!(calls[0].xm_byte, b'h');
    assert_eq!(calls[0].context, CytosineContext::CHH);
    assert!(!calls[0].methylated);
    // Second call corresponds to BAM-stored position 0 ('Z').
    assert_eq!(calls[1].read_pos, 4);
    assert_eq!(calls[1].xm_byte, b'Z');
    assert_eq!(calls[1].context, CytosineContext::CpG);
    assert!(calls[1].methylated);
}

#[test]
fn extract_calls_rejects_invalid_xm_byte_with_error() {
    let record = helpers::ot_record(b"ZzQXx.", b"ACGTAC");
    let err = extract_calls(&record, 0, 0, /*mbias_only_silence=*/ false).unwrap_err();
    match err {
        BismarkExtractorError::InvalidXmByte {
            byte, byte_char, ..
        } => {
            assert_eq!(byte, b'Q');
            assert_eq!(byte_char, 'Q');
        }
        _ => panic!("expected InvalidXmByte"),
    }
}

#[test]
fn extract_calls_ignore_larger_than_seq_returns_empty() {
    let record = helpers::ot_record(b"ZzXxHh", b"ACGTAC");
    let calls = extract_calls(&record, 100, 0, /*mbias_only_silence=*/ false).unwrap();
    assert!(calls.is_empty());
}

// ─── Phase E: mbias_only_silence kernel-level behaviour ─────────────

/// Phase E rev 1: `mbias_only_silence=true` silently skips invalid XM
/// bytes, mirroring Perl `:2972/3054 die "..." unless ($mbias_only)`.
/// Calls before and after the bad byte must still be preserved.
#[test]
fn extract_calls_mbias_only_silence_skips_invalid_xm_byte() {
    // XM: Z (valid), Q (invalid), z (valid). With silence=true, Q is skipped
    // but Z and z are still emitted.
    let record = helpers::ot_record(b"ZQz", b"ACG");
    let calls = extract_calls(&record, 0, 0, /*mbias_only_silence=*/ true).unwrap();
    // Q is skipped; Z and z are emitted.
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].xm_byte, b'Z');
    assert_eq!(calls[1].xm_byte, b'z');
}

/// Phase E rev 1 (narrowed catch-arm): `mbias_only_silence=true` does NOT
/// alter the `Skip*` paths for `.`, `u`, `U` — those continue to take the
/// `Ok(XmClassification::Skip*)` arms regardless. Verifies the silencing
/// scope is exactly `InvalidXmByte`, not "any error or skip".
#[test]
fn extract_calls_mbias_only_silence_preserves_dot_and_u_paths() {
    // XM bytes: Z (call), . (Skip), u (Skip), z (call).
    let record = helpers::ot_record(b"Z.uz", b"ACGT");
    let with_silence = extract_calls(&record, 0, 0, /*mbias_only_silence=*/ true).unwrap();
    let without_silence = extract_calls(&record, 0, 0, /*mbias_only_silence=*/ false).unwrap();
    // The two runs must produce identical results because the silence flag
    // only changes the InvalidXmByte-error path, and there are no invalid
    // bytes here.
    assert_eq!(
        with_silence.len(),
        without_silence.len(),
        "skip paths must be unaffected by mbias_only_silence"
    );
    assert_eq!(with_silence.len(), 2);
}

/// Phase E rev 1: with `mbias_only_silence=false`, invalid XM bytes
/// continue to raise `InvalidXmByte` (Phase B byte-identity).
#[test]
fn extract_calls_mbias_only_silence_false_still_errors_on_invalid_xm_byte() {
    let record = helpers::ot_record(b"ZQ", b"AC");
    let err = extract_calls(&record, 0, 0, /*mbias_only_silence=*/ false).unwrap_err();
    assert!(matches!(
        err,
        BismarkExtractorError::InvalidXmByte { byte: b'Q', .. }
    ));
}

// ─────────────────────────────────────────────────────────────────────────
// 3. MbiasTable tests
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn mbias_accumulate_increments_meth_for_Z() {
    let mut t = MbiasTable::default();
    t.accumulate(CytosineContext::CpG, 5, true);
    assert_eq!(t.cpg.get(5), Some(&MbiasPos { meth: 1, unmeth: 0 }));
}

#[test]
fn mbias_accumulate_increments_unmeth_for_z() {
    let mut t = MbiasTable::default();
    t.accumulate(CytosineContext::CpG, 5, false);
    assert_eq!(t.cpg.get(5), Some(&MbiasPos { meth: 0, unmeth: 1 }));
}

#[test]
fn mbias_accumulate_routes_to_chg_for_X() {
    let mut t = MbiasTable::default();
    t.accumulate(CytosineContext::CHG, 7, true);
    assert_eq!(t.chg.get(7), Some(&MbiasPos { meth: 1, unmeth: 0 }));
    assert_eq!(
        t.cpg.get(7).cloned().unwrap_or_default(),
        MbiasPos::default()
    );
}

#[test]
fn mbias_accumulate_routes_to_chg_for_x() {
    let mut t = MbiasTable::default();
    t.accumulate(CytosineContext::CHG, 7, false);
    assert_eq!(t.chg.get(7), Some(&MbiasPos { meth: 0, unmeth: 1 }));
}

#[test]
fn mbias_accumulate_routes_to_chh_for_H() {
    let mut t = MbiasTable::default();
    t.accumulate(CytosineContext::CHH, 9, true);
    assert_eq!(t.chh.get(9), Some(&MbiasPos { meth: 1, unmeth: 0 }));
}

#[test]
fn mbias_accumulate_routes_to_chh_for_h() {
    let mut t = MbiasTable::default();
    t.accumulate(CytosineContext::CHH, 9, false);
    assert_eq!(t.chh.get(9), Some(&MbiasPos { meth: 0, unmeth: 1 }));
}

// ─────────────────────────────────────────────────────────────────────────
// 4. OutputFileMap eager-open tests
// ─────────────────────────────────────────────────────────────────────────

/// Drive one `write_call` to lazily materialize the split file for
/// `(context, strand)`. `OutputFileMap::new` is lazy (#889 item 1): a split
/// file exists on disk only once written. (The 5-col row is `r\t+\tchr1\t100\tZ\n`.)
fn touch(map: &mut OutputFileMap, context: CytosineContext, strand: BismarkStrand) {
    let call = MethCall {
        ref_pos: 100,
        read_pos: 0,
        context,
        methylated: true,
        xm_byte: b'Z',
    };
    map.write_call(b"r", "chr1", call, strand, 0, 0, None, false)
        .unwrap();
}

/// Materialize all 12 Default-mode (context × strand) split files by writing
/// one row to each.
fn touch_all_12_default(map: &mut OutputFileMap) {
    for ctx in [
        CytosineContext::CpG,
        CytosineContext::CHG,
        CytosineContext::CHH,
    ] {
        for strand in [
            BismarkStrand::OT,
            BismarkStrand::CTOT,
            BismarkStrand::CTOB,
            BismarkStrand::OB,
        ] {
            touch(map, ctx, strand);
        }
    }
}

#[test]
fn output_file_map_lazily_creates_strand_files_on_write_for_default_mode() {
    let dir = tempfile::tempdir().unwrap();
    let mut map = OutputFileMap::new(
        dir.path(),
        "myinput",
        /*no_header=*/ false,
        OutputMode::Default,
        /*gzip=*/ false,
    )
    .unwrap();
    // Lazy-open (#889 item 1): no files exist until written. Writing one row to
    // every (context × strand) materializes all 12, each beginning with header.
    touch_all_12_default(&mut map);
    map.flush_all().unwrap();
    drop(map);

    let expected = [
        "CpG_OT_myinput.txt",
        "CpG_CTOT_myinput.txt",
        "CpG_CTOB_myinput.txt",
        "CpG_OB_myinput.txt",
        "CHG_OT_myinput.txt",
        "CHG_CTOT_myinput.txt",
        "CHG_CTOB_myinput.txt",
        "CHG_OB_myinput.txt",
        "CHH_OT_myinput.txt",
        "CHH_CTOT_myinput.txt",
        "CHH_CTOB_myinput.txt",
        "CHH_OB_myinput.txt",
    ];
    for name in expected {
        let path = dir.path().join(name);
        assert!(path.exists(), "expected {} to exist", path.display());
        let content = fs::read_to_string(&path).unwrap();
        assert!(
            content.starts_with(SPLIT_FILE_HEADER),
            "{} should begin with the version header",
            name
        );
    }
}

#[test]
fn output_file_map_omits_header_when_no_header_true() {
    let dir = tempfile::tempdir().unwrap();
    let mut map = OutputFileMap::new(
        dir.path(),
        "x",
        /*no_header=*/ true,
        OutputMode::Default,
        /*gzip=*/ false,
    )
    .unwrap();
    // Lazy-open: write one row so the file materializes; with no_header it must
    // start with the data row, NOT the version header.
    touch(&mut map, CytosineContext::CpG, BismarkStrand::OT);
    map.flush_all().unwrap();
    drop(map);
    let content = fs::read_to_string(dir.path().join("CpG_OT_x.txt")).unwrap();
    assert!(
        !content.starts_with("Bismark methylation extractor"),
        "no-header mode must not write the version header; got: {content:?}"
    );
    assert_eq!(content, "r\t+\tchr1\t100\tZ\n");
}

#[test]
fn output_file_header_matches_perl_format() {
    // The literal Perl header bytes — byte-identity-critical (Phase H gate).
    assert_eq!(
        SPLIT_FILE_HEADER, "Bismark methylation extractor version v0.25.1\n",
        "header drift would break Phase H byte-identity"
    );
    assert_eq!(BISMARK_VERSION, "v0.25.1");

    // And verify it actually reaches disk (lazy-open: after the first write).
    let dir = tempfile::tempdir().unwrap();
    let mut map = OutputFileMap::new(
        dir.path(),
        "x",
        false,
        OutputMode::Default,
        /*gzip=*/ false,
    )
    .unwrap();
    touch(&mut map, CytosineContext::CpG, BismarkStrand::OT);
    map.flush_all().unwrap();
    drop(map);
    let content = fs::read_to_string(dir.path().join("CpG_OT_x.txt")).unwrap();
    assert!(content.starts_with("Bismark methylation extractor version v0.25.1\n"));
}

#[test]
fn output_file_map_creates_output_dir_if_missing() {
    let parent = tempfile::tempdir().unwrap();
    let nested = parent.path().join("does/not/exist/yet");
    assert!(!nested.exists());
    let mut map = OutputFileMap::new(
        &nested,
        "x",
        false,
        OutputMode::Default,
        /*gzip=*/ false,
    )
    .unwrap();
    assert!(
        nested.exists(),
        "OutputFileMap::new should create output_dir (create_dir_all) even before any write"
    );
    // Lazy-open: the split file appears once written, inside the created dir.
    touch(&mut map, CytosineContext::CpG, BismarkStrand::OT);
    map.flush_all().unwrap();
    assert!(nested.join("CpG_OT_x.txt").exists());
}

#[test]
fn output_file_map_write_call_appends_after_header() {
    let dir = tempfile::tempdir().unwrap();
    let mut map = OutputFileMap::new(
        dir.path(),
        "x",
        false,
        OutputMode::Default,
        /*gzip=*/ false,
    )
    .unwrap();
    let call = MethCall {
        ref_pos: 200,
        read_pos: 5,
        context: CytosineContext::CpG,
        methylated: true,
        xm_byte: b'Z',
    };
    map.write_call(
        b"read1",
        "chr1",
        call,
        BismarkStrand::OT,
        /*yacht_col6=*/ 0,
        /*yacht_col7=*/ 0,
        /*agg=*/ None,
        /*cx=*/ false,
    )
    .unwrap();
    map.flush_all().unwrap();
    drop(map);
    let content = fs::read_to_string(dir.path().join("CpG_OT_x.txt")).unwrap();
    let expected = format!("{}read1\t+\tchr1\t200\tZ\n", SPLIT_FILE_HEADER);
    assert_eq!(content, expected);
}

#[test]
fn format_meth_line_exact_bytes_for_unmethylated() {
    let dir = tempfile::tempdir().unwrap();
    let mut map = OutputFileMap::new(
        dir.path(),
        "x",
        /*no_header=*/ true,
        OutputMode::Default,
        /*gzip=*/ false,
    )
    .unwrap();
    let call = MethCall {
        ref_pos: 42,
        read_pos: 0,
        context: CytosineContext::CHH,
        methylated: false,
        xm_byte: b'h',
    };
    map.write_call(
        b"r1",
        "1",
        call,
        BismarkStrand::CTOT,
        /*yacht_col6=*/ 0,
        /*yacht_col7=*/ 0,
        /*agg=*/ None,
        /*cx=*/ false,
    )
    .unwrap();
    map.flush_all().unwrap();
    drop(map);
    let content = fs::read_to_string(dir.path().join("CHH_CTOT_x.txt")).unwrap();
    assert_eq!(content, "r1\t-\t1\t42\th\n");
}

#[test]
fn cleanup_partial_outputs_removes_all_12_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut map = OutputFileMap::new(
        dir.path(),
        "x",
        false,
        OutputMode::Default,
        /*gzip=*/ false,
    )
    .unwrap();
    // Lazy-open: materialize all 12 by writing to each strand, then clean up.
    touch_all_12_default(&mut map);
    map.flush_all().unwrap();
    let count_before = fs::read_dir(dir.path()).unwrap().count();
    assert_eq!(count_before, 12);
    map.cleanup_all();
    let count_after = fs::read_dir(dir.path()).unwrap().count();
    assert_eq!(count_after, 0, "all 12 files should be removed");
}

#[test]
fn cleanup_partial_outputs_continues_past_one_failure() {
    // Rev 2 (Reviewer A S1 / Reviewer B L3 / plan-manager T-27): locks the
    // best-effort cleanup contract — one `remove_file` failure must not
    // prevent the other 11 from being removed.
    //
    // Trigger: pre-delete one of the 12 files out from under the OutputFileMap
    // so its eventual `remove_file` call returns `NotFound`. The map's
    // `cleanup_all` should iterate past that error and successfully remove
    // the remaining 11.
    let dir = tempfile::tempdir().unwrap();
    let mut map = OutputFileMap::new(
        dir.path(),
        "x",
        false,
        OutputMode::Default,
        /*gzip=*/ false,
    )
    .unwrap();
    // Lazy-open: materialize all 12 first, so there are files to clean up.
    touch_all_12_default(&mut map);
    map.flush_all().unwrap();

    // Pre-delete CpG_CTOT (a directional-library "always 0-byte" file).
    let pre_deleted = dir.path().join("CpG_CTOT_x.txt");
    fs::remove_file(&pre_deleted).unwrap();
    assert!(
        !pre_deleted.exists(),
        "pre-condition: file deleted out-of-band"
    );

    // 11 files left on disk; map still holds all 12 entries.
    let before = fs::read_dir(dir.path()).unwrap().count();
    assert_eq!(before, 11);

    // cleanup_all should not panic on the missing-file branch (eprintln only)
    // and should remove the other 11.
    map.cleanup_all();

    let after = fs::read_dir(dir.path()).unwrap().count();
    assert_eq!(
        after, 0,
        "11 remaining files should all be removed even though 1 was pre-deleted"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// 5. SplittingReport tests
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn splitting_report_percentage_handles_zero_denominator() {
    // Empty context (no calls) → 0.00, not NaN, not panic.
    let pct = SplittingReport::percent_meth(0, 0);
    assert_eq!(pct, 0.0);
}

#[test]
fn splitting_report_percentage_for_50_50() {
    let pct = SplittingReport::percent_meth(50, 50);
    assert_eq!(pct, 50.0);
}

#[test]
fn splitting_report_emits_per_context_counts() {
    let dir = tempfile::tempdir().unwrap();
    let report_path = dir.path().join("test_splitting_report.txt");
    let input_path = std::path::PathBuf::from("/some/input.bam");
    let cli = Cli::try_parse_from(["bismark_methylation_extractor_rs", "/tmp/x.bam"]).unwrap();
    let config = cli.validate().unwrap_or_else(|_| {
        // Validate fails because /tmp/x.bam doesn't exist, but we want a config.
        // Re-issue with a real temp file.
        let tmp = tempfile::Builder::new().suffix(".bam").tempfile().unwrap();
        std::fs::write(tmp.path(), b"x").unwrap();
        let cli = Cli::try_parse_from([
            "bismark_methylation_extractor_rs",
            tmp.path().to_str().unwrap(),
        ])
        .unwrap();
        // Leak the tempfile so it isn't deleted while config holds the path.
        let _ = tmp.into_temp_path().keep().unwrap();
        cli.validate().unwrap()
    });
    let report = SplittingReport {
        records_processed: 100,
        call_strings_processed: 100, // SE: equal to records
        calls_total: 600,
        calls_cpg_meth: 50,
        calls_cpg_unmeth: 50,
        calls_chg_meth: 100,
        calls_chg_unmeth: 0,
        calls_chh_meth: 0,
        calls_chh_unmeth: 400,
    };
    // Phase C.2 (#864): added `is_paired: bool` argument (SE here).
    write_splitting_report(&report_path, &input_path, &config, false, &report).unwrap();
    let content = fs::read_to_string(&report_path).unwrap();
    assert!(content.contains("Processed 100 lines in total"));
    assert!(content.contains("Total methylated C's in CpG context:\t50"));
    // Phase C.2 (#864): unmethylated phrasing changed from
    // `Total unmethylated C's in {ctx}` to Perl's
    // `Total C to T conversions in {ctx} context:`.
    assert!(content.contains("Total C to T conversions in CHH context:\t400"));
    // Phase C.2 (#864): percentages use 1 decimal, matching Perl's `%.1f`.
    assert!(content.contains("C methylated in CpG context:\t50.0%"));
    assert!(content.contains("C methylated in CHG context:\t100.0%"));
    assert!(content.contains("C methylated in CHH context:\t0.0%"));
}

// ─────────────────────────────────────────────────────────────────────────
// 6. route_call tests
// ─────────────────────────────────────────────────────────────────────────

/// Helper: minimal valid `ResolvedConfig` for state construction in tests.
fn test_config(output_dir: &std::path::Path) -> ResolvedConfig {
    let tmp = tempfile::Builder::new().suffix(".bam").tempfile().unwrap();
    std::fs::write(tmp.path(), b"x").unwrap();
    let tmp_path = tmp.into_temp_path();
    let path_str = tmp_path.to_str().unwrap().to_string();
    let _ = tmp_path.keep().unwrap(); // leak so path remains valid

    let cli = Cli::try_parse_from([
        "bismark_methylation_extractor_rs",
        &path_str,
        "--output_dir",
        output_dir.to_str().unwrap(),
    ])
    .unwrap();
    cli.validate().expect("validate test config")
}

#[test]
fn route_call_default_mode_routes_to_strand_specific_file() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let mut state =
        ExtractState::new(&config, std::path::Path::new("/tmp/x.bam"), "x", false).unwrap();
    let record = helpers::ot_record(b"Z....", b"ACGTC");
    let call = MethCall {
        ref_pos: 100,
        read_pos: 0,
        context: CytosineContext::CpG,
        methylated: true,
        xm_byte: b'Z',
    };
    route_call(
        &mut state,
        &record,
        "chr1",
        BismarkStrand::OT,
        call,
        ReadIdentity::Single,
    )
    .unwrap();
    state.fhs.flush_all().unwrap();

    let cpg_ot = fs::read_to_string(dir.path().join("CpG_OT_x.txt")).unwrap();
    assert!(cpg_ot.ends_with("read1\t+\tchr1\t100\tZ\n"));
    // Lazy-open: no call routed to CpG_CTOT → that file is never created
    // (formerly an eager header-only file; final swept state is identical).
    assert!(
        !dir.path().join("CpG_CTOT_x.txt").exists(),
        "CpG_CTOT must not exist — no call routed there (lazy-open)"
    );
}

#[test]
fn route_single_record_with_mixed_contexts_routes_to_one_strand_directory() {
    // Closes Alan Hoyle's "one record split across multiple strand files" bug
    // at unit-test level: a record on OT strand with calls in all 3 contexts
    // must produce non-header content in CpG_OT, CHG_OT, CHH_OT only — not in
    // any CTOT/CTOB/OB file.
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let mut state =
        ExtractState::new(&config, std::path::Path::new("/tmp/x.bam"), "x", false).unwrap();
    let record = helpers::ot_record(b"ZXH..", b"ACGTC");

    for call in extract_calls(&record, 0, 0, /*mbias_only_silence=*/ false).unwrap() {
        route_call(
            &mut state,
            &record,
            "chr1",
            BismarkStrand::OT,
            call,
            ReadIdentity::Single,
        )
        .unwrap();
    }
    state.fhs.flush_all().unwrap();

    // OT files for each context should have an extra line beyond the header.
    for ctx in ["CpG", "CHG", "CHH"] {
        let ot_path = dir.path().join(format!("{}_OT_x.txt", ctx));
        let content = fs::read_to_string(&ot_path).unwrap();
        assert!(
            content.lines().count() == 2,
            "{}_OT_x.txt should have header + 1 call line; got: {:?}",
            ctx,
            content
        );
    }
    // Lazy-open: CTOT/CTOB/OB strands receive no calls → their files are never
    // created (formerly eager header-only files; final swept state is identical).
    for ctx in ["CpG", "CHG", "CHH"] {
        for strand in ["CTOT", "CTOB", "OB"] {
            let p = dir.path().join(format!("{}_{}_x.txt", ctx, strand));
            assert!(
                !p.exists(),
                "{}_{}_x.txt must not exist — no call routed there (lazy-open)",
                ctx,
                strand
            );
        }
    }
}

#[test]
fn route_call_increments_counter_before_mbias_only_short_circuit() {
    // Rev 1 (Reviewer B I4): under --mbias_only, Perl still accumulates
    // splitting-report counters. We force `state.mbias_only = true` in this
    // test (even though Phase B's main dispatch rejects --mbias_only) and
    // verify counters still tick.
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let mut state =
        ExtractState::new(&config, std::path::Path::new("/tmp/x.bam"), "x", false).unwrap();
    state.mbias_only = true; // force the short-circuit branch
    let record = helpers::ot_record(b"Z....", b"ACGTC");
    let call = MethCall {
        ref_pos: 100,
        read_pos: 0,
        context: CytosineContext::CpG,
        methylated: true,
        xm_byte: b'Z',
    };
    route_call(
        &mut state,
        &record,
        "chr1",
        BismarkStrand::OT,
        call,
        ReadIdentity::Single,
    )
    .unwrap();
    // Counter must have incremented EVEN THOUGH the file write was short-circuited.
    assert_eq!(state.report.calls_total, 1);
    assert_eq!(state.report.calls_cpg_meth, 1);
    // Verify the split-file write was short-circuited — lazy-open: no file is
    // ever created for the routed strand when the write is skipped.
    state.fhs.flush_all().unwrap();
    assert!(
        !dir.path().join("CpG_OT_x.txt").exists(),
        "write should have been short-circuited (no file under lazy-open)"
    );
}

#[test]
fn route_call_r2_goes_to_mbias_index_1() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let mut state =
        ExtractState::new(&config, std::path::Path::new("/tmp/x.bam"), "x", false).unwrap();
    let record = helpers::ot_record(b"Z....", b"ACGTC");
    let call = MethCall {
        ref_pos: 100,
        read_pos: 5,
        context: CytosineContext::CpG,
        methylated: true,
        xm_byte: b'Z',
    };
    route_call(
        &mut state,
        &record,
        "chr1",
        BismarkStrand::OT,
        call,
        ReadIdentity::R2,
    )
    .unwrap();
    // 1-based position 6 in mbias[1].cpg should now be (meth=1, unmeth=0).
    let cell = state.mbias[1].cpg.get(6).copied().unwrap_or_default();
    assert_eq!(cell, MbiasPos { meth: 1, unmeth: 0 });
    // mbias[0] (R1/SE) should remain empty for this position.
    let cell_r1 = state.mbias[0].cpg.get(6).copied().unwrap_or_default();
    assert_eq!(cell_r1, MbiasPos::default());
}

#[test]
fn mbias_R2_index_ready() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let state = ExtractState::new(&config, std::path::Path::new("/tmp/x.bam"), "x", false).unwrap();
    // mbias is [MbiasTable; 2] — both indices exist after construction.
    assert_eq!(state.mbias[0].cpg.len(), 0);
    assert_eq!(state.mbias[1].cpg.len(), 0);
}

// ─────────────────────────────────────────────────────────────────────────
// 7. header / chr-name table tests
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn build_chr_name_table_returns_ascii_names_in_order() {
    let mut header = helpers::header_with_single_chr("chr1");
    use noodles_sam::header::record::value::Map;
    use noodles_sam::header::record::value::map::ReferenceSequence;
    use std::num::NonZeroUsize;
    header.reference_sequences_mut().insert(
        bstr::BString::from(b"chr2".to_vec()),
        Map::<ReferenceSequence>::new(NonZeroUsize::new(2000).unwrap()),
    );
    let table = build_chr_name_table(&header).unwrap();
    assert_eq!(table, vec!["chr1".to_string(), "chr2".to_string()]);
}

#[test]
fn build_chr_name_table_rejects_non_ascii() {
    use noodles_sam::Header;
    use noodles_sam::header::record::value::Map;
    use noodles_sam::header::record::value::map::ReferenceSequence;
    use std::num::NonZeroUsize;
    let mut header = Header::default();
    // Non-ASCII chr name (UTF-8 bytes for "chr_α").
    let non_ascii = b"chr_\xce\xb1";
    header.reference_sequences_mut().insert(
        bstr::BString::from(non_ascii.to_vec()),
        Map::<ReferenceSequence>::new(NonZeroUsize::new(1000).unwrap()),
    );
    let err = build_chr_name_table(&header).unwrap_err();
    assert!(matches!(
        err,
        BismarkExtractorError::NonAsciiChromosomeName { .. }
    ));
}

// ─────────────────────────────────────────────────────────────────────────
// 8. derive_basename tests
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn derive_basename_strips_known_suffixes() {
    use std::path::Path;
    assert_eq!(derive_basename(Path::new("a.bam")), "a");
    assert_eq!(derive_basename(Path::new("a.sam")), "a");
    assert_eq!(derive_basename(Path::new("a.cram")), "a");
    // No suffix: leave as-is.
    assert_eq!(derive_basename(Path::new("a")), "a");
    // Case-sensitive (Perl `s/bam$/txt/` doesn't match uppercase).
    assert_eq!(derive_basename(Path::new("a.BAM")), "a.BAM");
    // Compound suffix: only strip the LAST single extension. Per Perl,
    // `a.bam.gz` does NOT match `s/bam$/txt/` because the suffix is `.gz`.
    assert_eq!(derive_basename(Path::new("a.bam.gz")), "a.bam.gz");
    // Directory components: only basename matters.
    assert_eq!(derive_basename(Path::new("/path/to/sample.bam")), "sample");
}

// ─────────────────────────────────────────────────────────────────────────
// 9. main.rs phase-gate dispatch tests
// ─────────────────────────────────────────────────────────────────────────

/// Helper: build a tempfile with a `.bam` suffix that passes `Cli::validate`'s
/// file-existence check (won't be opened as a real BAM — phase-gate fires first).
fn tempbam() -> tempfile::NamedTempFile {
    let f = tempfile::Builder::new()
        .suffix(".bam")
        .tempfile()
        .expect("tempfile");
    std::fs::write(f.path(), b"x").expect("write tempfile");
    f
}

#[test]
fn main_paired_end_no_longer_rejected_phase_c() {
    // Phase C update: `--paired-end` now dispatches to extract_pe instead of
    // returning `PhaseNotYetImplemented`. The tempfile isn't a valid BAM so
    // the call fails at the bismark-io reader stage (NOT at the phase-gate).
    // We assert the error message does NOT contain Phase B's old gate text.
    let bam = tempbam();
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(bam.path())
        .arg("--paired-end")
        .assert()
        .failure()
        .stderr(predicates::str::contains("paired-end extraction; arrives in Phase C").not());
}

// NOTE: multiple input files are now SUPPORTED (v1.x) — processed per-file
// with no pooling, faithful to Perl's `foreach my $filename`. The end-to-end
// per-file behavior (incl. coordinate-sorted SE acceptance, fail-fast, and
// no cross-file bleed) is exercised in `tests/multifile_coordsorted.rs`. The
// former `main_rejects_multiple_input_files` rejection test was removed when
// the `files.len() != 1` gate was lifted in `main::run`.

/// Phase F (per plan §7.1): `--parallel N` is no longer phase-gated. The
/// run still fails because `tempbam()` writes junk content, but the
/// failure text must NOT mention the previous "--parallel N (only
/// --parallel 1 supported)" Phase F gate string.
#[test]
fn main_accepts_multicore_no_longer_rejected() {
    let bam = tempbam();
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(bam.path())
        .arg("--parallel")
        .arg("4")
        .assert()
        .failure()
        .stderr(predicates::str::contains("only --parallel 1 supported").not());
}

/// Phase E (rev 1, per plan §7.1): `--gzip` is no longer phase-gated.
/// The run still fails because `tempbam()` writes junk content, but the
/// failure text must NOT mention "Phase E" — that'd indicate the gate
/// is still in place.
#[test]
fn main_accepts_gzip_no_longer_rejected() {
    let bam = tempbam();
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(bam.path())
        .arg("--gzip")
        .assert()
        .failure()
        .stderr(predicates::str::contains("--gzip; arrives in Phase E").not());
}

/// Phase E (rev 1): `--comprehensive` is no longer phase-gated. Asserts
/// the failure (driven by junk-BAM content) does NOT carry the Phase E
/// rejection string.
#[test]
fn main_accepts_comprehensive_no_longer_rejected() {
    let bam = tempbam();
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(bam.path())
        .arg("--comprehensive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("output mode Comprehensive").not());
}

/// Phase E (rev 1): `--merge_non_CpG` is no longer phase-gated.
#[test]
fn main_accepts_merge_non_cpg_no_longer_rejected() {
    let bam = tempbam();
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(bam.path())
        .arg("--merge_non_CpG")
        .assert()
        .failure()
        .stderr(predicates::str::contains("output mode MergeNonCpG").not());
}

/// Phase E (rev 1): `--yacht` is no longer phase-gated (still SE-only
/// per Phase A `Cli::validate`).
#[test]
fn main_accepts_yacht_no_longer_rejected() {
    let bam = tempbam();
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(bam.path())
        .arg("--yacht")
        .assert()
        .failure()
        .stderr(predicates::str::contains("output mode Yacht").not());
}

/// Phase E (rev 1): `--mbias_only` is no longer phase-gated.
#[test]
fn main_accepts_mbias_only_no_longer_rejected() {
    let bam = tempbam();
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(bam.path())
        .arg("--mbias_only")
        .assert()
        .failure()
        .stderr(predicates::str::contains("output mode MbiasOnly").not());
}

/// Inline-streaming epic Phase 2 (T4): `--bedGraph` is no longer phase-gated.
/// `tempbam()` writes junk content, so the run still fails at the bismark-io
/// reader stage — but the failure text must NOT mention the old Phase G gate
/// string. A full exit-0 + `.cov.gz` bridge-parity test lives in
/// `tests/phase2_inline.rs` (it needs a real synthetic BAM with CpG calls).
#[test]
fn main_bedgraph_no_longer_rejected_phase_g() {
    let bam = tempbam();
    let mut cmd = Command::cargo_bin("bismark_methylation_extractor").unwrap();
    cmd.arg(bam.path())
        .arg("--bedGraph")
        .assert()
        .failure()
        .stderr(
            predicates::str::contains(
                "--bedGraph / --cytosine_report subprocess chain; arrives in Phase G",
            )
            .not(),
        );
}

// Note: `extract_se_rejects_record_with_paired_flag_set` requires a real
// BAM to drive through `open_reader → records()`. Implemented in
// `tests/se_phase_b_smoke.rs` where the BAM-construction harness lives.

// Note: `output_mode_default` is implicitly tested by the absence of a
// phase-gate failure in `tests/se_phase_b_smoke.rs` (which uses default mode).

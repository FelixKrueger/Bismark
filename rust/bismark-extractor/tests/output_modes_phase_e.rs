//! Phase E unit tests — output-mode dispatch + gzip + yacht row format.
//!
//! Kernel `mbias_only_silence` tests live in `se_phase_b.rs` where the
//! synthetic-record helpers are defined; this file is pure mode/key/yacht
//! testing that needs no synthetic BAM records.

#![allow(non_snake_case)]

use std::fs;
use std::io::Read;

use bismark_extractor::call::{CytosineContext, MethCall};
use bismark_extractor::cli::OutputMode;
use bismark_extractor::output::{OutputFileMap, SPLIT_FILE_HEADER};
use bismark_extractor::output_mode::{
    CpGOrNonCpG, OutputKey, mode_keys, orient_byte, route_to_key, write_yacht_row,
};
use bismark_io::BismarkStrand;
use flate2::read::GzDecoder;

// ─── mode_keys: per-mode key count + filenames ────────────────────────

#[test]
fn mode_keys_default_has_12_keys() {
    let keys = mode_keys(OutputMode::Default, "x", false);
    assert_eq!(keys.len(), 12);
    for (key, _) in &keys {
        assert!(matches!(key, OutputKey::Default(_, _)), "got {:?}", key);
    }
}

#[test]
fn mode_keys_default_filenames_match_perl_open_order() {
    let names: Vec<_> = mode_keys(OutputMode::Default, "input", false)
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    // Perl order: CpG block (4 strands), CHG block (4 strands), CHH block.
    assert_eq!(names[0], "CpG_OT_input.txt");
    assert_eq!(names[1], "CpG_CTOT_input.txt");
    assert_eq!(names[2], "CpG_CTOB_input.txt");
    assert_eq!(names[3], "CpG_OB_input.txt");
    assert_eq!(names[4], "CHG_OT_input.txt");
    assert_eq!(names[7], "CHG_OB_input.txt");
    assert_eq!(names[8], "CHH_OT_input.txt");
    assert_eq!(names[11], "CHH_OB_input.txt");
}

#[test]
fn mode_keys_comprehensive_has_3_keys_with_context_infix() {
    let names: Vec<_> = mode_keys(OutputMode::Comprehensive, "x", false)
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    assert_eq!(
        names,
        vec![
            "CpG_context_x.txt",
            "CHG_context_x.txt",
            "CHH_context_x.txt",
        ]
    );
}

#[test]
fn mode_keys_merge_non_cpg_has_8_keys_without_context_infix() {
    let names: Vec<_> = mode_keys(OutputMode::MergeNonCpG, "x", false)
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    assert_eq!(names.len(), 8);
    assert_eq!(names[0], "CpG_OT_x.txt");
    assert_eq!(names[3], "CpG_OB_x.txt");
    assert_eq!(names[4], "Non_CpG_OT_x.txt");
    assert_eq!(names[7], "Non_CpG_OB_x.txt");
    // Perl `:5139` uses `s/^/CpG_OT_/` — no _context_ infix in this mode.
    assert!(names.iter().all(|n| !n.contains("_context_")));
}

#[test]
fn mode_keys_comprehensive_merge_non_cpg_has_2_keys_with_context_infix() {
    let names: Vec<_> = mode_keys(OutputMode::ComprehensiveMergeNonCpG, "x", false)
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    assert_eq!(names, vec!["CpG_context_x.txt", "Non_CpG_context_x.txt"]);
}

#[test]
fn mode_keys_yacht_has_1_key() {
    let keys = mode_keys(OutputMode::Yacht, "x", false);
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].1, "any_C_context_x.txt");
    assert_eq!(keys[0].0, OutputKey::Yacht);
}

#[test]
fn mode_keys_mbias_only_has_0_keys() {
    assert!(mode_keys(OutputMode::MbiasOnly, "x", false).is_empty());
    assert!(mode_keys(OutputMode::MbiasOnly, "x", true).is_empty());
}

#[test]
fn mode_keys_gzip_appends_dot_gz_to_every_mode() {
    for mode in [
        OutputMode::Default,
        OutputMode::Comprehensive,
        OutputMode::MergeNonCpG,
        OutputMode::ComprehensiveMergeNonCpG,
        OutputMode::Yacht,
    ] {
        let keys = mode_keys(mode, "x", true);
        assert!(!keys.is_empty(), "{:?} should produce non-empty keys", mode);
        for (_, name) in &keys {
            assert!(
                name.ends_with(".txt.gz"),
                "mode={:?} name={:?} should end .txt.gz",
                mode,
                name
            );
        }
    }
}

// ─── route_to_key: per-mode call routing ──────────────────────────────

#[test]
fn route_to_key_default_returns_default_variant() {
    let k = route_to_key(OutputMode::Default, CytosineContext::CpG, BismarkStrand::OT);
    assert_eq!(
        k,
        Some(OutputKey::Default(CytosineContext::CpG, BismarkStrand::OT))
    );
}

#[test]
fn route_to_key_comprehensive_drops_strand() {
    let k_ot = route_to_key(
        OutputMode::Comprehensive,
        CytosineContext::CpG,
        BismarkStrand::OT,
    );
    let k_ob = route_to_key(
        OutputMode::Comprehensive,
        CytosineContext::CpG,
        BismarkStrand::OB,
    );
    assert_eq!(k_ot, k_ob);
    assert_eq!(k_ot, Some(OutputKey::Comprehensive(CytosineContext::CpG)));
}

#[test]
fn route_to_key_merge_non_cpg_routes_chg_to_non_cpg() {
    let k = route_to_key(
        OutputMode::MergeNonCpG,
        CytosineContext::CHG,
        BismarkStrand::OT,
    );
    assert_eq!(
        k,
        Some(OutputKey::MergeNonCpG(
            CpGOrNonCpG::NonCpG,
            BismarkStrand::OT
        ))
    );
}

#[test]
fn route_to_key_merge_non_cpg_routes_chh_to_non_cpg() {
    let k = route_to_key(
        OutputMode::MergeNonCpG,
        CytosineContext::CHH,
        BismarkStrand::OT,
    );
    assert_eq!(
        k,
        Some(OutputKey::MergeNonCpG(
            CpGOrNonCpG::NonCpG,
            BismarkStrand::OT
        ))
    );
}

#[test]
fn route_to_key_merge_non_cpg_keeps_cpg_as_cpg() {
    let k = route_to_key(
        OutputMode::MergeNonCpG,
        CytosineContext::CpG,
        BismarkStrand::OT,
    );
    assert_eq!(
        k,
        Some(OutputKey::MergeNonCpG(CpGOrNonCpG::CpG, BismarkStrand::OT))
    );
}

#[test]
fn route_to_key_comprehensive_merge_non_cpg_routes_chh_to_non_cpg() {
    let k = route_to_key(
        OutputMode::ComprehensiveMergeNonCpG,
        CytosineContext::CHH,
        BismarkStrand::OT,
    );
    assert_eq!(
        k,
        Some(OutputKey::ComprehensiveMergeNonCpG(CpGOrNonCpG::NonCpG))
    );
}

#[test]
fn route_to_key_yacht_collapses_all_to_unit() {
    for ctx in [
        CytosineContext::CpG,
        CytosineContext::CHG,
        CytosineContext::CHH,
    ] {
        for s in [
            BismarkStrand::OT,
            BismarkStrand::CTOT,
            BismarkStrand::CTOB,
            BismarkStrand::OB,
        ] {
            assert_eq!(
                route_to_key(OutputMode::Yacht, ctx, s),
                Some(OutputKey::Yacht)
            );
        }
    }
}

#[test]
fn route_to_key_mbias_only_returns_none() {
    let k = route_to_key(
        OutputMode::MbiasOnly,
        CytosineContext::CpG,
        BismarkStrand::OT,
    );
    assert_eq!(k, None);
}

// ─── OutputFileMap construction per mode ──────────────────────────────

#[test]
fn output_file_map_skips_eager_open_for_mbias_only() {
    let dir = tempfile::tempdir().unwrap();
    let mut map = OutputFileMap::new(dir.path(), "x", false, OutputMode::MbiasOnly, false).unwrap();
    let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
    assert!(
        entries.is_empty(),
        "MbiasOnly should produce no split files"
    );
    // Both finalize-time operations must be valid no-ops on the empty map
    // (Reviewer A A5).
    map.flush_all()
        .expect("flush_all on empty map must succeed");
    map.cleanup_all();
}

#[test]
fn output_file_map_comprehensive_creates_3_files_with_context_infix() {
    let dir = tempfile::tempdir().unwrap();
    let _map = OutputFileMap::new(dir.path(), "x", true, OutputMode::Comprehensive, false).unwrap();
    let names = on_disk_filenames(dir.path());
    assert_eq!(names.len(), 3);
    assert!(names.contains("CpG_context_x.txt"));
    assert!(names.contains("CHG_context_x.txt"));
    assert!(names.contains("CHH_context_x.txt"));
}

#[test]
fn output_file_map_merge_non_cpg_creates_8_files() {
    let dir = tempfile::tempdir().unwrap();
    let _map = OutputFileMap::new(dir.path(), "x", true, OutputMode::MergeNonCpG, false).unwrap();
    let names = on_disk_filenames(dir.path());
    assert_eq!(names.len(), 8);
    for class in ["CpG", "Non_CpG"] {
        for strand in ["OT", "CTOT", "CTOB", "OB"] {
            let expected = format!("{class}_{strand}_x.txt");
            assert!(names.contains(&expected), "missing {expected}");
        }
    }
}

#[test]
fn output_file_map_yacht_creates_1_file() {
    let dir = tempfile::tempdir().unwrap();
    let _map = OutputFileMap::new(dir.path(), "x", true, OutputMode::Yacht, false).unwrap();
    let names = on_disk_filenames(dir.path());
    assert_eq!(names.len(), 1);
    assert!(names.contains("any_C_context_x.txt"));
}

#[test]
fn output_file_map_gzip_appends_dot_gz_to_disk_filenames() {
    let dir = tempfile::tempdir().unwrap();
    let _map = OutputFileMap::new(
        dir.path(),
        "x",
        true,
        OutputMode::Comprehensive,
        true, // gzip
    )
    .unwrap();
    let names = on_disk_filenames(dir.path());
    assert_eq!(names.len(), 3);
    for name in &names {
        assert!(name.ends_with(".txt.gz"), "got {name:?}");
    }
}

/// `--gzip` round-trip: decompress the .gz output and assert byte-for-byte
/// equality with the plain-mode equivalent on the same input.
#[test]
fn output_file_map_gzip_writes_valid_gz_content_byte_identical_to_plain() {
    let plain_dir = tempfile::tempdir().unwrap();
    let gz_dir = tempfile::tempdir().unwrap();
    let mut plain =
        OutputFileMap::new(plain_dir.path(), "x", false, OutputMode::Default, false).unwrap();
    let mut gz = OutputFileMap::new(gz_dir.path(), "x", false, OutputMode::Default, true).unwrap();

    let call = MethCall {
        ref_pos: 100,
        read_pos: 0,
        context: CytosineContext::CpG,
        methylated: true,
        xm_byte: b'Z',
    };
    plain
        .write_call(b"read1", "chr1", call, BismarkStrand::OT, 0, 0)
        .unwrap();
    gz.write_call(b"read1", "chr1", call, BismarkStrand::OT, 0, 0)
        .unwrap();
    plain.flush_all().unwrap();
    gz.flush_all().unwrap();
    drop(plain);
    drop(gz); // GzEncoder Drop writes the gzip footer here.

    let plain_bytes = fs::read(plain_dir.path().join("CpG_OT_x.txt")).unwrap();
    assert!(
        plain_bytes.starts_with(SPLIT_FILE_HEADER.as_bytes()),
        "plain file should begin with version header"
    );
    let gz_file = fs::File::open(gz_dir.path().join("CpG_OT_x.txt.gz")).unwrap();
    let mut decoded = Vec::new();
    GzDecoder::new(gz_file).read_to_end(&mut decoded).unwrap();
    assert_eq!(
        decoded, plain_bytes,
        "gz must decompress to byte-identical plain output"
    );
}

// ─── write_call routing across modes ──────────────────────────────────

#[test]
fn write_call_comprehensive_routes_OT_and_OB_to_single_per_context_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut map =
        OutputFileMap::new(dir.path(), "x", true, OutputMode::Comprehensive, false).unwrap();
    let call_ot = MethCall {
        ref_pos: 100,
        read_pos: 0,
        context: CytosineContext::CpG,
        methylated: true,
        xm_byte: b'Z',
    };
    let call_ob = MethCall {
        ref_pos: 200,
        read_pos: 0,
        context: CytosineContext::CpG,
        methylated: false,
        xm_byte: b'z',
    };
    map.write_call(b"read_ot", "chr1", call_ot, BismarkStrand::OT, 0, 0)
        .unwrap();
    map.write_call(b"read_ob", "chr1", call_ob, BismarkStrand::OB, 0, 0)
        .unwrap();
    map.flush_all().unwrap();
    drop(map);

    let cpg = fs::read_to_string(dir.path().join("CpG_context_x.txt")).unwrap();
    assert!(cpg.contains("read_ot"));
    assert!(cpg.contains("read_ob"));
    assert!(!dir.path().join("CpG_OT_x.txt").exists());
}

#[test]
fn write_call_merge_non_cpg_routes_X_to_non_cpg_OT() {
    routes_byte_to_non_cpg(b'X', CytosineContext::CHG, true);
}

#[test]
fn write_call_merge_non_cpg_routes_x_to_non_cpg_OT() {
    routes_byte_to_non_cpg(b'x', CytosineContext::CHG, false);
}

#[test]
fn write_call_merge_non_cpg_routes_H_to_non_cpg_OT() {
    routes_byte_to_non_cpg(b'H', CytosineContext::CHH, true);
}

#[test]
fn write_call_merge_non_cpg_routes_h_to_non_cpg_OT() {
    routes_byte_to_non_cpg(b'h', CytosineContext::CHH, false);
}

#[test]
fn write_call_merge_non_cpg_keeps_Z_in_cpg_OT() {
    let dir = tempfile::tempdir().unwrap();
    let mut map =
        OutputFileMap::new(dir.path(), "x", true, OutputMode::MergeNonCpG, false).unwrap();
    let call = MethCall {
        ref_pos: 100,
        read_pos: 0,
        context: CytosineContext::CpG,
        methylated: true,
        xm_byte: b'Z',
    };
    map.write_call(b"read1", "chr1", call, BismarkStrand::OT, 0, 0)
        .unwrap();
    map.flush_all().unwrap();
    drop(map);
    let cpg = fs::read_to_string(dir.path().join("CpG_OT_x.txt")).unwrap();
    assert!(cpg.contains("read1"));
}

// ─── write_yacht_row ──────────────────────────────────────────────────

#[test]
fn write_yacht_row_forward_strand_emits_8_cols_with_col6_lt_col7() {
    let call = MethCall {
        ref_pos: 100,
        read_pos: 0,
        context: CytosineContext::CpG,
        methylated: true,
        xm_byte: b'Z',
    };
    let mut buf: Vec<u8> = Vec::new();
    // For OT strand: route_call would compute (alignment_start=90, reference_end=140).
    write_yacht_row(
        &mut buf,
        b"read1",
        "chr1",
        &call,
        90,
        140,
        BismarkStrand::OT,
    )
    .unwrap();
    let s = std::str::from_utf8(&buf).unwrap();
    let cols: Vec<&str> = s.trim_end_matches('\n').split('\t').collect();
    assert_eq!(cols.len(), 8);
    assert_eq!(
        cols,
        vec!["read1", "+", "chr1", "100", "Z", "90", "140", "+"]
    );
}

/// **Critical-1 regression guard (Phase E rev 1).** OB-strand yacht rows
/// emit `(reference_end, alignment_start)` for col-6/col-7, i.e.
/// col-6 > col-7. Mirrors Perl `:4350, 4382, 4422-4447`.
#[test]
fn write_yacht_row_reverse_strand_swaps_col6_col7() {
    let call = MethCall {
        ref_pos: 120,
        read_pos: 0,
        context: CytosineContext::CpG,
        methylated: false,
        xm_byte: b'z',
    };
    let mut buf: Vec<u8> = Vec::new();
    // For OB: route_call would compute (reference_end=140, alignment_start=90).
    write_yacht_row(&mut buf, b"r2", "chr1", &call, 140, 90, BismarkStrand::OB).unwrap();
    let s = std::str::from_utf8(&buf).unwrap();
    let cols: Vec<&str> = s.trim_end_matches('\n').split('\t').collect();
    assert_eq!(cols.len(), 8);
    // The load-bearing assertions for Critical-1:
    assert_eq!(
        cols[5], "140",
        "OB col-6 must be reference_end (larger value)"
    );
    assert_eq!(
        cols[6], "90",
        "OB col-7 must be alignment_start (smaller value)"
    );
    let col6: u32 = cols[5].parse().unwrap();
    let col7: u32 = cols[6].parse().unwrap();
    assert!(
        col6 > col7,
        "OB-strand yacht row must have col-6 > col-7 for Perl byte-identity \
         (rev 0 of plan got this wrong; rev 1 + this test catch the regression)"
    );
    assert_eq!(cols[7], "-", "OB orientation byte must be '-'");
    assert_eq!(cols[1], "-", "unmethylated z gives '-' meth char");
}

#[test]
fn yacht_orient_byte_plus_for_forward_class() {
    assert_eq!(orient_byte(BismarkStrand::OT), b'+');
    assert_eq!(orient_byte(BismarkStrand::CTOB), b'+');
}

#[test]
fn yacht_orient_byte_minus_for_reverse_class() {
    assert_eq!(orient_byte(BismarkStrand::OB), b'-');
    assert_eq!(orient_byte(BismarkStrand::CTOT), b'-');
}

// ─── helpers ──────────────────────────────────────────────────────────

fn on_disk_filenames(dir: &std::path::Path) -> std::collections::BTreeSet<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect()
}

fn routes_byte_to_non_cpg(xm_byte: u8, context: CytosineContext, methylated: bool) {
    let dir = tempfile::tempdir().unwrap();
    let mut map =
        OutputFileMap::new(dir.path(), "x", true, OutputMode::MergeNonCpG, false).unwrap();
    let call = MethCall {
        ref_pos: 100,
        read_pos: 0,
        context,
        methylated,
        xm_byte,
    };
    map.write_call(b"read1", "chr1", call, BismarkStrand::OT, 0, 0)
        .unwrap();
    map.flush_all().unwrap();
    drop(map);
    let non_cpg = fs::read_to_string(dir.path().join("Non_CpG_OT_x.txt")).unwrap();
    assert!(
        non_cpg.contains("read1"),
        "byte {:?} should land in Non_CpG_OT_x.txt; got {non_cpg:?}",
        xm_byte as char
    );
    // The context-specific Phase B file should never be created in this mode:
    let stray = format!(
        "{}_OT_x.txt",
        match context {
            CytosineContext::CpG => "CpG",
            CytosineContext::CHG => "CHG",
            CytosineContext::CHH => "CHH",
        }
    );
    if context != CytosineContext::CpG {
        assert!(
            !dir.path().join(&stray).exists(),
            "non-CpG byte should not create {stray}"
        );
    }
}

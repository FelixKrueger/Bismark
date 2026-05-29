//! Phase F byte-identity tests.
//!
//! Three categories of assertion:
//!   1. **Legacy vs parallel** — `extract_se` / `extract_pe` (single-threaded,
//!      Phase B/C reference) vs `extract_se_parallel` / `extract_pe_parallel`
//!      (the new pipeline) on the same input. Byte-identical output is the
//!      load-bearing Phase H invariant.
//!   2. **Cross-N** — `--multicore 1` vs `--multicore 4` vs `--multicore 8`
//!      via the parallel pipeline. Byte-identity across N is the SPEC §9.7
//!      target.
//!   3. **Error propagation** — invalid XM byte, unpaired PE record,
//!      mbias_only invalid byte silence, all at N=4.
//!
//! Tests use the library API (`extract_se` / `extract_se_parallel` directly,
//! not the binary) for speed and direct error inspection. The binary's
//! end-to-end behaviour at N=1 is already exercised by every existing
//! Phase B-E smoke test (they all flow through `extract_*_parallel` now).

#![allow(non_snake_case)]

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use bismark_extractor::cli::{Cli, ResolvedConfig};
use bismark_extractor::{extract_pe, extract_pe_parallel, extract_se, extract_se_parallel};
use bismark_io::{BamWriter, BismarkRecord};
use bstr::BString;
use clap::Parser;
use flate2::read::GzDecoder;
use noodles_core::Position;
use noodles_sam::Header;
use noodles_sam::alignment::record::Flags;
use noodles_sam::alignment::record::cigar::Op;
use noodles_sam::alignment::record::cigar::op::Kind;
use noodles_sam::alignment::record::data::field::Tag;
use noodles_sam::alignment::record_buf::data::field::Value;
use noodles_sam::alignment::record_buf::{Cigar, RecordBuf, Sequence};
use noodles_sam::header::record::value::Map;
use noodles_sam::header::record::value::map::ReferenceSequence;
use std::io::Read;
use std::num::NonZeroUsize;

// ─── Synthetic BAM helpers (duplicated from se_phase_b_smoke.rs +
//     output_modes_phase_e_smoke.rs; cross-test `tests/common/mod.rs`
//     refactor is a Phase E deferred TODO) ──────────────────────────────

fn header_with_chr1() -> Header {
    let mut header = Header::default();
    header.reference_sequences_mut().insert(
        BString::from(b"chr1".to_vec()),
        Map::<ReferenceSequence>::new(NonZeroUsize::new(10_000).unwrap()),
    );
    header
}

fn synth_record(
    qname: &[u8],
    xr: &[u8],
    xg: &[u8],
    xm: &[u8],
    seq: &[u8],
    alignment_start: usize,
    flags: u16,
) -> BismarkRecord {
    let mut record = RecordBuf::default();
    *record.flags_mut() = Flags::from(flags);
    *record.sequence_mut() = Sequence::from(seq.to_vec());
    *record.alignment_start_mut() = Some(Position::try_from(alignment_start).unwrap());
    *record.reference_sequence_id_mut() = Some(0);
    *record.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, xm.len())]);
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

/// SE-directional BAM with mixed methylation contexts + both OT and OB strands.
fn write_se_directional_bam(path: &Path) {
    let header = header_with_chr1();
    let mut writer = BamWriter::from_path(path, header).unwrap();
    // 3 OT records (XR=CT XG=CT) covering CpG/CHG/CHH meth + unmeth.
    writer
        .write_record(&synth_record(
            b"r_OT_1", b"CT", b"CT", b"Zz...", b"ACGTC", 100, 0,
        ))
        .unwrap();
    writer
        .write_record(&synth_record(
            b"r_OT_2", b"CT", b"CT", b"..X.x", b"ACGTC", 200, 0,
        ))
        .unwrap();
    writer
        .write_record(&synth_record(
            b"r_OT_3", b"CT", b"CT", b"H.h..", b"ACGTC", 300, 0,
        ))
        .unwrap();
    // 2 OB records (XR=CT XG=GA).
    writer
        .write_record(&synth_record(
            b"r_OB_1", b"CT", b"GA", b"Z....", b"ACGTC", 400, 0,
        ))
        .unwrap();
    writer
        .write_record(&synth_record(
            b"r_OB_2", b"CT", b"GA", b"..h..", b"ACGTC", 500, 0,
        ))
        .unwrap();
    writer.finish().unwrap();
}

/// SE BAM with `n` records — used to cross the parallel pipeline's
/// `BATCH_SIZE` (4096) boundary so the multi-batch path (full-flush +
/// `batch_seq` increment + multi-entry `BTreeMap` reorder) is exercised. The
/// small fixtures above are all ≤6 records → a single partial batch at
/// `batch_seq=0`, leaving the multi-batch path (which runs on the real
/// 15.4M-record workload + the Phase H gate) untested (#884 R1 code-review:
/// both reviewers flagged this gap). Cycles 4 context/strand patterns so the
/// output spans multiple split files.
fn write_se_large_bam(path: &Path, n: usize) {
    let header = header_with_chr1();
    let mut writer = BamWriter::from_path(path, header).unwrap();
    let pats: [(&[u8], &[u8], &[u8]); 4] = [
        (b"CT", b"CT", b"Zz..."),
        (b"CT", b"CT", b"..X.x"),
        (b"CT", b"GA", b"H.h.."),
        (b"CT", b"GA", b"Z..h."),
    ];
    for i in 0..n {
        let (xr, xg, xm) = pats[i % pats.len()];
        let qname = format!("r{i}");
        let pos = 100 + (i % 400);
        writer
            .write_record(&synth_record(
                qname.as_bytes(),
                xr,
                xg,
                xm,
                b"ACGTC",
                pos,
                0,
            ))
            .unwrap();
    }
    writer.finish().unwrap();
}

/// PE-directional BAM (R1+R2 pairs). Adjacent records form a pair.
fn write_pe_directional_bam(path: &Path) {
    let header = header_with_chr1();
    let mut writer = BamWriter::from_path(path, header).unwrap();
    // Pair 1: R1 + R2 at OT.
    // R1: 0x01 PAIRED | 0x40 FIRST_IN_PAIR = 0x41
    // R2: 0x01 PAIRED | 0x80 SECOND_IN_PAIR = 0x81
    writer
        .write_record(&synth_record(
            b"pair1", b"CT", b"CT", b"Zz...", b"ACGTC", 100, 0x41,
        ))
        .unwrap();
    writer
        .write_record(&synth_record(
            b"pair1", b"GA", b"CT", b"..X..", b"ACGTC", 110, 0x81,
        ))
        .unwrap();
    // Pair 2: R1 + R2 at OB.
    writer
        .write_record(&synth_record(
            b"pair2", b"CT", b"GA", b"Z..h.", b"ACGTC", 400, 0x41,
        ))
        .unwrap();
    writer
        .write_record(&synth_record(
            b"pair2", b"GA", b"GA", b"..z..", b"ACGTC", 410, 0x81,
        ))
        .unwrap();
    // Pair 3: R1 + R2 at OT, different positions.
    writer
        .write_record(&synth_record(
            b"pair3", b"CT", b"CT", b"H..h.", b"ACGTC", 700, 0x41,
        ))
        .unwrap();
    writer
        .write_record(&synth_record(
            b"pair3", b"GA", b"CT", b".X..x", b"ACGTC", 710, 0x81,
        ))
        .unwrap();
    writer.finish().unwrap();
}

/// BAM with one record carrying an invalid XM byte (`Q`).
fn write_bam_with_invalid_xm(path: &Path) {
    let header = header_with_chr1();
    let mut writer = BamWriter::from_path(path, header).unwrap();
    writer
        .write_record(&synth_record(
            b"bad_xm", b"CT", b"CT", b"ZQz", b"ACG", 100, 0,
        ))
        .unwrap();
    writer.finish().unwrap();
}

/// PE BAM with an odd record count (orphan R1 at the end).
fn write_pe_unpaired_final_bam(path: &Path) {
    let header = header_with_chr1();
    let mut writer = BamWriter::from_path(path, header).unwrap();
    // Complete pair.
    writer
        .write_record(&synth_record(
            b"pair1", b"CT", b"CT", b"Z....", b"ACGTC", 100, 0x41,
        ))
        .unwrap();
    writer
        .write_record(&synth_record(
            b"pair1", b"GA", b"CT", b"..z..", b"ACGTC", 110, 0x81,
        ))
        .unwrap();
    // Orphan R1.
    writer
        .write_record(&synth_record(
            b"orphan", b"CT", b"CT", b"Z....", b"ACGTC", 200, 0x41,
        ))
        .unwrap();
    writer.finish().unwrap();
}

/// Empty BAM (header only).
fn write_empty_bam(path: &Path) {
    let header = header_with_chr1();
    let writer = BamWriter::from_path(path, header).unwrap();
    writer.finish().unwrap();
}

// ─── Config + comparison helpers ──────────────────────────────────────

/// Build a `ResolvedConfig` from CLI-style args. Convenient because
/// `ResolvedConfig` has 28 fields and constructing it directly is verbose.
fn resolved_config(args: &[&str]) -> ResolvedConfig {
    let mut full = vec!["bismark-methylation-extractor-rs"];
    full.extend(args.iter().copied());
    let cli = Cli::try_parse_from(&full).expect("CLI should parse");
    cli.validate().expect("CLI should validate")
}

/// Run a closure that's expected to terminate within 30 seconds. Panics
/// with a deadlock-detected message if it doesn't — preventing a hung
/// test from blocking CI indefinitely. Plan rev 1 §7 universal timeout
/// guard commitment (Reviewer B V2).
///
/// The closure runs on a dedicated thread; the main test thread polls
/// the JoinHandle for up to 30 seconds. If the thread hasn't finished,
/// the main thread panics — the spawned thread is abandoned (deliberate
/// leak; tests are short-lived processes and CI will reap them).
fn with_timeout<F, R>(label: &str, f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let handle = std::thread::spawn(f);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if handle.is_finished() {
            return handle
                .join()
                .unwrap_or_else(|_| panic!("test thread for `{label}` panicked"));
        }
        if std::time::Instant::now() >= deadline {
            panic!("DEADLOCK DETECTED: `{label}` did not finish within 30 seconds");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Assert the file-name set + every file's contents are byte-identical
/// between two directories.
///
/// For `*_splitting_report.txt`, the `Input file:` and `Output directory:`
/// lines are stripped before comparison — those are inherently path-
/// dependent across runs (the helper uses different temp dirs for the
/// two runs being compared). The remaining content (counts, percentages,
/// `Processed N lines in total`) IS asserted byte-identical.
///
/// All other files (split files, M-bias.txt, etc.) are compared strictly.
fn assert_dirs_byte_identical(dir_a: &Path, dir_b: &Path, label_a: &str, label_b: &str) {
    let names_a: BTreeSet<String> = fs::read_dir(dir_a)
        .unwrap_or_else(|e| panic!("read_dir({}): {e}", dir_a.display()))
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    let names_b: BTreeSet<String> = fs::read_dir(dir_b)
        .unwrap_or_else(|e| panic!("read_dir({}): {e}", dir_b.display()))
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        names_a, names_b,
        "{label_a} and {label_b} file-name sets differ\n{label_a}: {names_a:?}\n{label_b}: {names_b:?}"
    );
    for name in &names_a {
        let bytes_a = fs::read(dir_a.join(name)).unwrap();
        let bytes_b = fs::read(dir_b.join(name)).unwrap();
        if name.ends_with("_splitting_report.txt") {
            // Strip the two path-dependent lines before comparing.
            let a_norm = normalize_report(&bytes_a);
            let b_norm = normalize_report(&bytes_b);
            assert_eq!(
                a_norm, b_norm,
                "{label_a} vs {label_b}: splitting report {name} differs (after path normalization)"
            );
        } else {
            assert_eq!(
                bytes_a,
                bytes_b,
                "{label_a} vs {label_b}: file {name} differs (len {} vs {})",
                bytes_a.len(),
                bytes_b.len()
            );
        }
    }
}

/// Strip lines starting with `Input file:` or `Output directory:` from
/// the splitting-report bytes. Those lines record the actual paths used
/// by the run, which legitimately differ between test runs that use
/// different temp dirs. Counter lines, percentages, and the
/// `Processed N lines in total` line are preserved (those ARE the
/// byte-identity-load-bearing content).
fn normalize_report(bytes: &[u8]) -> Vec<u8> {
    let s = String::from_utf8_lossy(bytes);
    let mut out = String::new();
    for line in s.lines() {
        if line.starts_with("Input file:") || line.starts_with("Output directory:") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.into_bytes()
}

/// Read a gzipped file and return its decompressed bytes.
fn decompress_gz(path: &Path) -> Vec<u8> {
    let file = fs::File::open(path).unwrap();
    let mut decoded = Vec::new();
    GzDecoder::new(file).read_to_end(&mut decoded).unwrap();
    decoded
}

// ═══════════════════════════════════════════════════════════════════════
// 1. LEGACY vs PARALLEL byte-identity
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn legacy_vs_parallel_n1_se_default_byte_identical() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);

    let legacy_dir = workdir.path().join("legacy");
    let parallel_dir = workdir.path().join("parallel");

    let bam_s = bam_path.to_str().unwrap();
    let legacy_s = legacy_dir.to_str().unwrap();
    let parallel_s = parallel_dir.to_str().unwrap();

    extract_se(
        &bam_path,
        &resolved_config(&["--single-end", "--output_dir", legacy_s, bam_s]),
    )
    .unwrap();
    extract_se_parallel(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--parallel",
            "1",
            "--output_dir",
            parallel_s,
            bam_s,
        ]),
    )
    .unwrap();

    assert_dirs_byte_identical(&legacy_dir, &parallel_dir, "legacy", "parallel-n1");
}

#[test]
fn legacy_vs_parallel_n4_se_default_byte_identical() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);

    let legacy_dir = workdir.path().join("legacy");
    let parallel_dir = workdir.path().join("parallel_n4");

    let bam_s = bam_path.to_str().unwrap();
    extract_se(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--output_dir",
            legacy_dir.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();
    extract_se_parallel(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--parallel",
            "4",
            "--output_dir",
            parallel_dir.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();

    assert_dirs_byte_identical(&legacy_dir, &parallel_dir, "legacy", "parallel-n4");
}

#[test]
fn legacy_vs_parallel_n4_pe_default_byte_identical() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("pe.bam");
    write_pe_directional_bam(&bam_path);

    let legacy_dir = workdir.path().join("legacy");
    let parallel_dir = workdir.path().join("parallel_n4");

    let bam_s = bam_path.to_str().unwrap();
    extract_pe(
        &bam_path,
        &resolved_config(&["-p", "--output_dir", legacy_dir.to_str().unwrap(), bam_s]),
    )
    .unwrap();
    extract_pe_parallel(
        &bam_path,
        &resolved_config(&[
            "-p",
            "--parallel",
            "4",
            "--output_dir",
            parallel_dir.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();

    assert_dirs_byte_identical(&legacy_dir, &parallel_dir, "legacy-pe", "parallel-pe-n4");
}

// ═══════════════════════════════════════════════════════════════════════
// 2. CROSS-N byte-identity (parallel at different N values)
// ═══════════════════════════════════════════════════════════════════════

/// Wrapped in `with_timeout` to demonstrate the helper's intended usage:
/// future parallel tests that risk deadlock should use the same pattern.
/// At N=8 this is the test most likely to expose any latent ordering /
/// channel-sizing regression, so guarding it makes sense.
#[test]
fn parallel_se_byte_identical_across_n_1_2_4_8() {
    with_timeout("parallel_se_byte_identical_across_n_1_2_4_8", || {
        let workdir = tempfile::tempdir().unwrap();
        let bam_path = workdir.path().join("se.bam");
        write_se_directional_bam(&bam_path);
        let bam_s = bam_path.to_str().unwrap().to_string();

        let mut dirs = Vec::new();
        for n in [1u32, 2, 4, 8] {
            let dir = workdir.path().join(format!("n{n}"));
            extract_se_parallel(
                &bam_path,
                &resolved_config(&[
                    "--single-end",
                    "--parallel",
                    &n.to_string(),
                    "--output_dir",
                    dir.to_str().unwrap(),
                    &bam_s,
                ]),
            )
            .unwrap();
            dirs.push((n, dir));
        }

        // Compare N=1 vs each of N=2/4/8.
        let ref_dir = dirs[0].1.clone();
        for (n, dir) in &dirs[1..] {
            assert_dirs_byte_identical(&ref_dir, dir, "n1", &format!("n{n}"));
        }
    });
}

/// Crosses `BATCH_SIZE` (4096): 8199 records = two full batches + a 7-record
/// partial, forcing the full-flush + `batch_seq` increment + multi-entry
/// `BTreeMap` reorder paths that the ≤6-record fixtures never reach (#884 R1).
/// Guards the multi-batch byte-identity the real workload + Phase H rely on.
#[test]
fn parallel_se_byte_identical_across_batch_boundary() {
    with_timeout("parallel_se_byte_identical_across_batch_boundary", || {
        let workdir = tempfile::tempdir().unwrap();
        let bam_path = workdir.path().join("large.bam");
        write_se_large_bam(&bam_path, 8199); // > 2 * BATCH_SIZE (4096)
        let bam_s = bam_path.to_str().unwrap().to_string();

        let mut dirs = Vec::new();
        for n in [1u32, 4, 8] {
            let dir = workdir.path().join(format!("n{n}"));
            extract_se_parallel(
                &bam_path,
                &resolved_config(&[
                    "--single-end",
                    "--parallel",
                    &n.to_string(),
                    "--output_dir",
                    dir.to_str().unwrap(),
                    &bam_s,
                ]),
            )
            .unwrap();
            dirs.push((n, dir));
        }
        let ref_dir = dirs[0].1.clone();
        for (n, dir) in &dirs[1..] {
            assert_dirs_byte_identical(&ref_dir, dir, "n1", &format!("n{n}"));
        }
    });
}

#[test]
fn parallel_pe_byte_identical_across_n_1_4_8() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("pe.bam");
    write_pe_directional_bam(&bam_path);
    let bam_s = bam_path.to_str().unwrap();

    let mut dirs = Vec::new();
    for n in [1u32, 4, 8] {
        let dir = workdir.path().join(format!("n{n}"));
        extract_pe_parallel(
            &bam_path,
            &resolved_config(&[
                "-p",
                "--parallel",
                &n.to_string(),
                "--output_dir",
                dir.to_str().unwrap(),
                bam_s,
            ]),
        )
        .unwrap();
        dirs.push((n, dir));
    }

    let ref_dir = dirs[0].1.clone();
    for (n, dir) in &dirs[1..] {
        assert_dirs_byte_identical(&ref_dir, dir, "pe-n1", &format!("pe-n{n}"));
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 3. MODE-specific byte-identity at N=4
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn parallel_comprehensive_n4_byte_identical_to_legacy() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);
    let bam_s = bam_path.to_str().unwrap();

    let legacy = workdir.path().join("legacy");
    let parallel = workdir.path().join("parallel");
    extract_se(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--comprehensive",
            "--output_dir",
            legacy.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();
    extract_se_parallel(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--comprehensive",
            "--parallel",
            "4",
            "--output_dir",
            parallel.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();

    assert_dirs_byte_identical(&legacy, &parallel, "legacy-comprehensive", "parallel-n4");
}

#[test]
fn parallel_merge_non_cpg_n4_byte_identical_to_legacy() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);
    let bam_s = bam_path.to_str().unwrap();

    let legacy = workdir.path().join("legacy");
    let parallel = workdir.path().join("parallel");
    extract_se(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--merge_non_CpG",
            "--output_dir",
            legacy.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();
    extract_se_parallel(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--merge_non_CpG",
            "--parallel",
            "4",
            "--output_dir",
            parallel.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();

    assert_dirs_byte_identical(&legacy, &parallel, "legacy-mnc", "parallel-mnc-n4");
}

/// Critical-1 (yacht reverse-strand polarity) at N=4 — exercises the
/// strand-conditional col-6/col-7 in the parallel path via
/// `compute_yacht_columns` shared with the legacy path.
#[test]
fn parallel_yacht_n4_byte_identical_to_legacy() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);
    let bam_s = bam_path.to_str().unwrap();

    let legacy = workdir.path().join("legacy");
    let parallel = workdir.path().join("parallel");
    extract_se(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--yacht",
            "--output_dir",
            legacy.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();
    extract_se_parallel(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--yacht",
            "--parallel",
            "4",
            "--output_dir",
            parallel.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();

    assert_dirs_byte_identical(&legacy, &parallel, "legacy-yacht", "parallel-yacht-n4");

    // Sanity-check Critical-1 invariant on the output: at least one OB
    // row exists and has col-6 > col-7.
    let content = fs::read_to_string(parallel.join("any_C_context_se.txt")).unwrap();
    let mut saw_reverse = false;
    for line in content.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() == 8 && cols[7] == "-" {
            let col6: u32 = cols[5].parse().unwrap();
            let col7: u32 = cols[6].parse().unwrap();
            assert!(
                col6 > col7,
                "OB yacht row must have col-6 > col-7 (parallel path)"
            );
            saw_reverse = true;
        }
    }
    assert!(saw_reverse, "fixture must include at least one OB row");
}

#[test]
fn parallel_mbias_only_n4_byte_identical_to_legacy() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);
    let bam_s = bam_path.to_str().unwrap();

    let legacy = workdir.path().join("legacy");
    let parallel = workdir.path().join("parallel");
    extract_se(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--mbias_only",
            "--output_dir",
            legacy.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();
    extract_se_parallel(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--mbias_only",
            "--parallel",
            "4",
            "--output_dir",
            parallel.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();

    assert_dirs_byte_identical(&legacy, &parallel, "legacy-mo", "parallel-mo-n4");
}

#[test]
fn parallel_gzip_n4_decompresses_identical_to_legacy_plain() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);
    let bam_s = bam_path.to_str().unwrap();

    let legacy = workdir.path().join("legacy"); // plain
    let parallel = workdir.path().join("parallel"); // gzipped
    extract_se(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--output_dir",
            legacy.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();
    extract_se_parallel(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--gzip",
            "--parallel",
            "4",
            "--output_dir",
            parallel.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();

    // Compare every .gz file's decompressed content to its plain peer.
    // The non-gz files (M-bias.txt, splitting_report) must be byte-identical.
    for entry in fs::read_dir(&parallel).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(stem) = name.strip_suffix(".gz") {
            let decoded = decompress_gz(&entry.path());
            let plain = fs::read(legacy.join(stem)).unwrap();
            assert_eq!(
                decoded, plain,
                "gz {name} decompressed differs from plain peer"
            );
        } else {
            let parallel_bytes = fs::read(entry.path()).unwrap();
            let legacy_bytes = fs::read(legacy.join(&name)).unwrap();
            if name.ends_with("_splitting_report.txt") {
                assert_eq!(
                    normalize_report(&parallel_bytes),
                    normalize_report(&legacy_bytes),
                    "non-gz splitting report {name} differs (after path normalization)"
                );
            } else {
                assert_eq!(parallel_bytes, legacy_bytes, "non-gz file {name} differs");
            }
        }
    }
}

/// #884 R2 regression guard: gzp's `ParCompress<Gzip>` must keep emitting a
/// **single-member** gzip even when the input spans multiple `BATCH_SIZE`
/// (4096) batches, and the decompressed content must be byte-identical across
/// `--parallel` 1 vs 4 and to the plain (non-gzip) peer.
///
/// 8199 records = two full batches + a 7-record partial, so gzp stitches
/// several internal sync-flushed DEFLATE blocks into one gzip stream.
/// `decompress_gz` uses a single-member `GzDecoder`: if a future gzp change
/// (or a switch to its `Mgzip`/`Bgzf` formats) emitted multi-member output,
/// the decode would truncate at member 0 and the `== plain` assertion would
/// fail. Decompressed *byte* identity (not merely sorted-equivalence) holds
/// because the collector concatenates per-batch output strictly in
/// `batch_seq` order, so the stream matches N=1's exactly (dual plan-review
/// C2). Complements the small-fixture single-batch test above.
#[test]
fn parallel_gzip_multibatch_decompresses_identical_across_n_and_to_plain() {
    with_timeout(
        "parallel_gzip_multibatch_decompresses_identical_across_n_and_to_plain",
        || {
            let workdir = tempfile::tempdir().unwrap();
            let bam_path = workdir.path().join("large.bam");
            write_se_large_bam(&bam_path, 8199); // > 2 * BATCH_SIZE (4096)
            let bam_s = bam_path.to_str().unwrap().to_string();

            // Reference: plain (non-gzip) single-threaded output.
            let plain_dir = workdir.path().join("plain");
            extract_se(
                &bam_path,
                &resolved_config(&[
                    "--single-end",
                    "--output_dir",
                    plain_dir.to_str().unwrap(),
                    &bam_s,
                ]),
            )
            .unwrap();

            // Gzipped at N=1 and N=4.
            let mut gz_dirs = Vec::new();
            for n in [1u32, 4] {
                let dir = workdir.path().join(format!("gz_n{n}"));
                extract_se_parallel(
                    &bam_path,
                    &resolved_config(&[
                        "--single-end",
                        "--gzip",
                        "--parallel",
                        &n.to_string(),
                        "--output_dir",
                        dir.to_str().unwrap(),
                        &bam_s,
                    ]),
                )
                .unwrap();
                gz_dirs.push((n, dir));
            }
            let gz_n1_dir = gz_dirs[0].1.clone();

            for (n, dir) in &gz_dirs {
                for entry in fs::read_dir(dir).unwrap() {
                    let entry = entry.unwrap();
                    let name = entry.file_name().to_string_lossy().to_string();
                    if let Some(stem) = name.strip_suffix(".gz") {
                        // Single-member decode of a multi-block stream → full
                        // plain content. Truncation here = a multi-member regression.
                        let decoded = decompress_gz(&entry.path());
                        let plain = fs::read(plain_dir.join(stem)).unwrap();
                        assert_eq!(
                            decoded, plain,
                            "gz {name} at n{n} decompressed differs from plain peer \
                             (truncation here would mean gzp regressed to multi-member)"
                        );
                        // Cross-N decompressed-byte identity.
                        let decoded_n1 = decompress_gz(&gz_n1_dir.join(&name));
                        assert_eq!(
                            decoded, decoded_n1,
                            "gz {name} decompressed differs between n1 and n{n}"
                        );
                    } else if name.ends_with("_splitting_report.txt") {
                        assert_eq!(
                            normalize_report(&fs::read(entry.path()).unwrap()),
                            normalize_report(&fs::read(plain_dir.join(&name)).unwrap()),
                            "non-gz splitting report {name} at n{n} differs"
                        );
                    } else {
                        assert_eq!(
                            fs::read(entry.path()).unwrap(),
                            fs::read(plain_dir.join(&name)).unwrap(),
                            "non-gz file {name} at n{n} differs from plain"
                        );
                    }
                }
            }
        },
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 4. ERROR PROPAGATION at N=4
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn parallel_invalid_xm_byte_propagates_error_at_n4() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("bad.bam");
    write_bam_with_invalid_xm(&bam_path);

    let out_dir = workdir.path().join("out");
    let bam_s = bam_path.to_str().unwrap();
    let result = extract_se_parallel(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--parallel",
            "4",
            "--output_dir",
            out_dir.to_str().unwrap(),
            bam_s,
        ]),
    );
    let err = result.expect_err("invalid XM should propagate as Err");
    assert!(
        format!("{err}").to_lowercase().contains("invalid xm")
            || format!("{err}").to_lowercase().contains("unrecognised"),
        "expected InvalidXmByte-shaped error; got: {err}"
    );
}

#[test]
fn parallel_pe_unpaired_final_record_at_n4() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("orphan.bam");
    write_pe_unpaired_final_bam(&bam_path);

    let out_dir = workdir.path().join("out");
    let bam_s = bam_path.to_str().unwrap();
    let result = extract_pe_parallel(
        &bam_path,
        &resolved_config(&[
            "-p",
            "--parallel",
            "4",
            "--output_dir",
            out_dir.to_str().unwrap(),
            bam_s,
        ]),
    );
    let err = result.expect_err("orphan R1 should propagate as Err");
    assert!(
        format!("{err}").to_lowercase().contains("unpaired"),
        "expected UnpairedFinalRecord-shaped error; got: {err}"
    );
}

#[test]
fn parallel_mbias_only_invalid_xm_silently_skipped_at_n4() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("bad.bam");
    write_bam_with_invalid_xm(&bam_path);

    let out_dir = workdir.path().join("out");
    let bam_s = bam_path.to_str().unwrap();
    // --mbias_only should silently skip the Q byte (per Phase E
    // mbias_only_silence). Parallel path must honour this too.
    extract_se_parallel(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--mbias_only",
            "--parallel",
            "4",
            "--output_dir",
            out_dir.to_str().unwrap(),
            bam_s,
        ]),
    )
    .expect("--mbias_only must silently skip invalid XM bytes");

    // Splitting-report should show 1 meth (Z) + 1 unmeth (z) — the Q is skipped.
    let report = fs::read_to_string(out_dir.join("bad_splitting_report.txt")).unwrap();
    assert!(report.contains("Total methylated C's in CpG context:\t1"));
    // Phase C.2 (#864): unmethylated phrasing now matches Perl's
    // `Total C to T conversions in {ctx} context:`.
    assert!(report.contains("Total C to T conversions in CpG context:\t1"));
}

// ═══════════════════════════════════════════════════════════════════════
// 5. EDGE CASES
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn parallel_empty_bam_at_n4_produces_header_only_files() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("empty.bam");
    write_empty_bam(&bam_path);

    let out_dir = workdir.path().join("out");
    let bam_s = bam_path.to_str().unwrap();
    extract_se_parallel(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--parallel",
            "4",
            "--output_dir",
            out_dir.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();

    // Phase C.2 (#865): empty BAM → no records routed → every per-strand
    // file is empty after the run → all 12 are swept (unlinked) at
    // finalize time. Only the splitting-report and M-bias.txt survive.
    for ctx in ["CpG", "CHG", "CHH"] {
        for strand in ["OT", "CTOT", "CTOB", "OB"] {
            let path = out_dir.join(format!("{ctx}_{strand}_empty.txt"));
            assert!(
                !path.exists(),
                "{}: empty per-strand file should be swept",
                path.display()
            );
        }
    }
    // Splitting report shows 0 records.
    let report = fs::read_to_string(out_dir.join("empty_splitting_report.txt")).unwrap();
    assert!(report.contains("Processed 0 lines in total"));
}

#[test]
fn parallel_n1_via_extract_se_parallel_matches_legacy_extract_se_pe() {
    // Belt-and-suspenders: N=1 through the parallel pipeline produces
    // the same output as the legacy single-threaded extract_pe (since
    // every Phase B–E smoke test now flows through this path, this
    // assertion just makes the equivalence explicit in a Phase F-named test).
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("pe.bam");
    write_pe_directional_bam(&bam_path);

    let legacy = workdir.path().join("legacy");
    let parallel = workdir.path().join("parallel");
    let bam_s = bam_path.to_str().unwrap();
    extract_pe(
        &bam_path,
        &resolved_config(&["-p", "--output_dir", legacy.to_str().unwrap(), bam_s]),
    )
    .unwrap();
    extract_pe_parallel(
        &bam_path,
        &resolved_config(&[
            "-p",
            "--parallel",
            "1",
            "--output_dir",
            parallel.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();
    assert_dirs_byte_identical(&legacy, &parallel, "legacy-pe", "parallel-pe-n1");
}

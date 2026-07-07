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

use bismark::extractor::cli::{Cli, ResolvedConfig};
use bismark::extractor::{extract_pe, extract_pe_parallel, extract_se, extract_se_parallel};
use bismark::io::{BamWriter, BismarkRecord, SamWriter};
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

/// The 5 SE-directional records (3 OT XR=CT XG=CT + 2 OB XR=CT XG=GA), covering
/// CpG/CHG/CHH meth+unmeth. Shared by the BAM and SAM fixtures so the two
/// containers hold byte-for-byte the same records (the #884 R3 dispatch test
/// compares their extractor output).
fn se_directional_records() -> Vec<BismarkRecord> {
    vec![
        synth_record(b"r_OT_1", b"CT", b"CT", b"Zz...", b"ACGTC", 100, 0),
        synth_record(b"r_OT_2", b"CT", b"CT", b"..X.x", b"ACGTC", 200, 0),
        synth_record(b"r_OT_3", b"CT", b"CT", b"H.h..", b"ACGTC", 300, 0),
        synth_record(b"r_OB_1", b"CT", b"GA", b"Z....", b"ACGTC", 400, 0),
        synth_record(b"r_OB_2", b"CT", b"GA", b"..h..", b"ACGTC", 500, 0),
    ]
}

/// SE-directional BAM with mixed methylation contexts + both OT and OB strands.
fn write_se_directional_bam(path: &Path) {
    let mut writer = BamWriter::from_path(path, header_with_chr1()).unwrap();
    for rec in se_directional_records() {
        writer.write_record(&rec).unwrap();
    }
    writer.finish().unwrap();
}

/// Same records as [`write_se_directional_bam`], written as **SAM** (#884 R3:
/// SAM is not BGZF, so it must take the single-threaded reader, not the
/// `ThreadedBamReader`).
fn write_se_directional_sam(path: &Path) {
    let mut writer = SamWriter::from_path(path, header_with_chr1()).unwrap();
    for rec in se_directional_records() {
        writer.write_record(&rec).unwrap();
    }
    writer.finish().unwrap();
}

/// #884 R3 dispatch guard: SAM input must route to the single-threaded reader
/// (NOT the BAM-only `ThreadedBamReader`) and yield the SAME methylation data as
/// the equivalent BAM — which now goes through the fixed-2-thread parallel-BGZF
/// reader. Identical records in → identical split-file calls out, regardless of
/// container or decode threading. Guards the new `is_bam ? Threaded : Any` else-arm.
#[test]
fn sam_input_matches_bam_through_r3_dispatch() {
    let workdir = tempfile::tempdir().unwrap();
    let bam = workdir.path().join("se.bam");
    let sam = workdir.path().join("se.sam");
    write_se_directional_bam(&bam);
    write_se_directional_sam(&sam);

    let bam_dir = workdir.path().join("from_bam");
    let sam_dir = workdir.path().join("from_sam");
    extract_se_parallel(
        &bam,
        &resolved_config(&[
            "--single-end",
            "--parallel",
            "4",
            "--output_dir",
            bam_dir.to_str().unwrap(),
            bam.to_str().unwrap(),
        ]),
    )
    .unwrap();
    extract_se_parallel(
        &sam,
        &resolved_config(&[
            "--single-end",
            "--parallel",
            "4",
            "--output_dir",
            sam_dir.to_str().unwrap(),
            sam.to_str().unwrap(),
        ]),
    )
    .unwrap();

    // Compare the methylation split-data files (CpG/CHG/CHH) — these carry no
    // input filename (unlike splitting_report), so SAM (single-threaded reader)
    // vs BAM (threaded 2-decode reader) must be byte-equal. Same `se` basename
    // ⇒ same filenames in both dirs.
    let mut compared = 0usize;
    for entry in fs::read_dir(&sam_dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();
        if (name.starts_with("CpG_") || name.starts_with("CHG_") || name.starts_with("CHH_"))
            && name.ends_with(".txt")
        {
            let sam_bytes = fs::read(entry.path()).unwrap();
            let bam_bytes = fs::read(bam_dir.join(&name)).unwrap();
            assert_eq!(
                sam_bytes, bam_bytes,
                "split file {name}: SAM (Any path) differs from BAM (threaded path)"
            );
            compared += 1;
        }
    }
    assert!(
        compared > 0,
        "expected >=1 methylation split file to compare"
    );
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
    let mut full = vec!["bismark_methylation_extractor_rs"];
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

/// #878 Test 4 — single-threaded `extract_se` vs parallel `extract_se_parallel`
/// produce identical output (incl. `M-bias.txt`) under a NON-ZERO `--ignore`.
///
/// Structural guard for the dual-driver back-port trap: a rebase that landed in
/// one driver but not the other would diverge here. Existing `legacy_vs_parallel_*`
/// tests run `--ignore 0`, which never exercises the rebase.
///
/// NOTE (dual-review I-4): this guards driver **divergence**, NOT revert — both
/// drivers share the rebase, so it stays green on a revert (Tests 1–3 cover
/// absolute correctness). `--ignore 2` (< the 5-bp fixture reads) is mandatory:
/// `--ignore 5` would trip the `lo>=hi` early-out (`call.rs:166`) → empty output
/// → vacuously identical (dual-review C1). The non-emptiness assertion locks that out.
#[test]
fn se_driver_vs_parallel_driver_m_bias_equality() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("se.bam");
    write_se_directional_bam(&bam_path);
    let bam_s = bam_path.to_str().unwrap();

    let single_dir = workdir.path().join("single");
    let parallel_dir = workdir.path().join("parallel_n4");
    extract_se(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--ignore",
            "2",
            "--output_dir",
            single_dir.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();
    extract_se_parallel(
        &bam_path,
        &resolved_config(&[
            "--single-end",
            "--ignore",
            "2",
            "--parallel",
            "4",
            "--output_dir",
            parallel_dir.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();

    // Non-emptiness guard (dual-review C1): --ignore must leave surviving calls,
    // else both dirs are empty and the equality below is vacuous.
    let mut call_lines = 0usize;
    for entry in fs::read_dir(&single_dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();
        if (name.starts_with("CpG_") || name.starts_with("CHG_") || name.starts_with("CHH_"))
            && name.ends_with(".txt")
        {
            for line in fs::read_to_string(entry.path()).unwrap().lines() {
                if !line.is_empty() && !line.starts_with("Bismark") {
                    call_lines += 1;
                }
            }
        }
    }
    assert!(
        call_lines > 0,
        "--ignore 2 must leave surviving methylation calls (guards the C1 vacuous-pass no-op)"
    );

    // M-bias.txt + all split/report files identical across the two drivers.
    assert_dirs_byte_identical(&single_dir, &parallel_dir, "single", "parallel-n4");
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

// ═══════════════════════════════════════════════════════════════════════
// 6. PARALLEL DECODE / GZIP HARDENING (#904, #889 item 3)
// ═══════════════════════════════════════════════════════════════════════

/// Count the **data-bearing** BGZF blocks in a BAM/BGZF file: blocks whose
/// uncompressed payload size (`ISIZE`, the last 4 bytes of each block) is `> 0`.
/// This excludes the trailing BGZF EOF marker AND any empty flush blocks the
/// writer emits (noodles writes an empty block before the EOF marker), so the
/// count reflects the number of blocks the threaded reader must actually decode
/// records across — which is what #904 cares about.
///
/// The block chain is walked via the BC-subfield `BSIZE` (each block's total
/// size = `BSIZE + 1`), not by scanning for the 4 magic bytes, so a magic
/// sequence occurring inside a compressed payload can't produce a false split.
fn count_bgzf_data_blocks(path: &Path) -> usize {
    let buf = fs::read(path).unwrap();
    let mut off = 0usize;
    let mut count = 0usize;
    while off + 12 <= buf.len() {
        // BGZF block header: gzip magic ID1=0x1f ID2=0x8b, CM=8, FLG=4 (FEXTRA).
        assert!(
            buf[off] == 0x1f
                && buf[off + 1] == 0x8b
                && buf[off + 2] == 0x08
                && buf[off + 3] == 0x04,
            "not a BGZF block header at offset {off}"
        );
        let xlen = u16::from_le_bytes([buf[off + 10], buf[off + 11]]) as usize;
        let extra = &buf[off + 12..off + 12 + xlen];
        // Scan the extra field for the BC subfield (SI1='B', SI2='C', SLEN=2).
        let mut bsize: Option<usize> = None;
        let mut p = 0usize;
        while p + 4 <= extra.len() {
            let slen = u16::from_le_bytes([extra[p + 2], extra[p + 3]]) as usize;
            if extra[p] == b'B' && extra[p + 1] == b'C' && slen == 2 && p + 6 <= extra.len() {
                bsize = Some(u16::from_le_bytes([extra[p + 4], extra[p + 5]]) as usize);
                break;
            }
            p += 4 + slen;
        }
        let block_len = bsize.expect("BGZF block missing BC subfield") + 1;
        // ISIZE = uncompressed payload size = the block's last 4 bytes (LE u32).
        // Empty blocks (EOF marker + empty flush blocks) have ISIZE == 0.
        let z = off + block_len - 4;
        let isize = u32::from_le_bytes([buf[z], buf[z + 1], buf[z + 2], buf[z + 3]]);
        if isize > 0 {
            count += 1;
        }
        off += block_len;
    }
    count
}

/// Unit-tests the block counter against known fixtures. A header-only BAM has
/// exactly **one** data-bearing block (the header) — the empty flush block and
/// EOF marker (both `ISIZE==0`) are excluded. The large fixture must span
/// several data blocks (the basis of the #904 guard below).
#[test]
fn count_bgzf_data_blocks_counts_data_bearing_blocks() {
    let workdir = tempfile::tempdir().unwrap();

    let empty = workdir.path().join("empty.bam");
    write_empty_bam(&empty);
    let small = workdir.path().join("small.bam");
    write_se_directional_bam(&small);
    let large = workdir.path().join("large.bam");
    write_se_large_bam(&large, 8199);

    let (n_empty, n_small, n_large) = (
        count_bgzf_data_blocks(&empty),
        count_bgzf_data_blocks(&small),
        count_bgzf_data_blocks(&large),
    );

    // A header-only BAM has exactly one data-bearing block (the header); the
    // empty flush block + EOF marker (both ISIZE==0) are excluded.
    assert_eq!(n_empty, 1, "header-only BAM = 1 data block");
    // 5 tiny records fit alongside the header in a single data block.
    assert_eq!(n_small, 1, "5 tiny records = 1 data block");
    // The 8199-record fixture is the basis of the #904 ≥3-block guard.
    assert!(
        n_large >= 3,
        "8199 records must span ≥3 data blocks; got {n_large}"
    );
}

/// #904: prove parallel BGZF decode preserves record order across **≥3 BGZF
/// blocks**. The reference is the single-threaded sequential decode
/// (`extract_se` → `open_reader`), which is order-correct by construction; the
/// subject is `extract_se_parallel`, which decodes BAM via the fixed-2-thread
/// `ThreadedBamReader` (#884 R3). A cross-block reordering in the threaded
/// reader would make the subject diverge from the reference.
///
/// The existing multi-batch test compares parallel-vs-parallel (same threaded
/// reader → blind to a reorder bug), and the other legacy-vs-parallel tests use
/// a ≤1-block fixture — so this is the only in-repo test that pits
/// single-threaded decode against threaded decode on a genuinely multi-block
/// BAM. The `assert!(blocks >= 3)` makes the fixture self-verifying: if a future
/// `BamWriter` change collapsed it to ≤2 blocks it fails loudly rather than
/// silently under-testing.
#[test]
fn parallel_se_byte_identical_ge3_bgzf_blocks_legacy_vs_threaded() {
    with_timeout(
        "parallel_se_byte_identical_ge3_bgzf_blocks_legacy_vs_threaded",
        || {
            let workdir = tempfile::tempdir().unwrap();
            let bam_path = workdir.path().join("large.bam");
            write_se_large_bam(&bam_path, 8199);
            let bam_s = bam_path.to_str().unwrap().to_string();

            let blocks = count_bgzf_data_blocks(&bam_path);
            assert!(
                blocks >= 3,
                "fixture must span ≥3 BGZF blocks to exercise multi-block parallel \
                 decode ordering; got {blocks}"
            );

            // Reference: single-threaded sequential decode.
            let legacy = workdir.path().join("legacy");
            extract_se(
                &bam_path,
                &resolved_config(&[
                    "--single-end",
                    "--output_dir",
                    legacy.to_str().unwrap(),
                    &bam_s,
                ]),
            )
            .unwrap();

            // Subject: 2-thread ThreadedBamReader decode, at N=1 and N=4.
            for n in [1u32, 4] {
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
                assert_dirs_byte_identical(&legacy, &dir, "legacy", &format!("threaded-n{n}"));
            }
        },
    );
}

/// #889 item 3: PE + `--gzip` byte-identity, plus an explicit empty-`.gz` sweep
/// assertion. PE flows through the same mode-agnostic `open_writer` as SE (only
/// SE+gzip was previously tested), and the directional PE fixture leaves the 6
/// CTOT/CTOB strands empty so the sweep is genuinely exercised under `--gzip`.
#[test]
fn parallel_pe_gzip_n4_decompresses_identical_to_legacy_plain() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path = workdir.path().join("pe.bam");
    write_pe_directional_bam(&bam_path);
    let bam_s = bam_path.to_str().unwrap();

    let legacy = workdir.path().join("legacy"); // plain
    let parallel = workdir.path().join("parallel"); // gzipped
    extract_pe(
        &bam_path,
        &resolved_config(&["-p", "--output_dir", legacy.to_str().unwrap(), bam_s]),
    )
    .unwrap();
    extract_pe_parallel(
        &bam_path,
        &resolved_config(&[
            "-p",
            "--gzip",
            "--parallel",
            "4",
            "--output_dir",
            parallel.to_str().unwrap(),
            bam_s,
        ]),
    )
    .unwrap();

    // Every .gz decompresses to its plain peer; non-gz files byte-identical
    // (splitting report normalized for the path-dependent lines).
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

    // (a) Empty-.gz sweep fired identically under --gzip: kept-file set (with
    // .gz stripped) equals the plain run's file set.
    let gzip_stems: BTreeSet<String> = fs::read_dir(&parallel)
        .unwrap()
        .map(|e| {
            let n = e.unwrap().file_name().to_string_lossy().to_string();
            match n.strip_suffix(".gz") {
                Some(stem) => stem.to_string(),
                None => n,
            }
        })
        .collect();
    let plain_names: BTreeSet<String> = fs::read_dir(&legacy)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        gzip_stems, plain_names,
        "empty-.gz sweep must keep the same file set under --gzip as the plain run"
    );

    // (b) Non-vacuity guard: the directional PE fixture writes only OT/OB, so
    // the 6 CTOT/CTOB context strands must be created-then-swept — NOT all 12
    // kept, and NOT vacuously never-created. Counting kept .gz context files
    // proves the sweep actually ran under --gzip.
    let gz_context: Vec<String> = fs::read_dir(&parallel)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".txt.gz"))
        .collect();
    assert!(
        !gz_context.is_empty() && gz_context.len() < 12,
        "expected some-but-not-all context strands kept (sweep ran under gzip); \
         got {} kept: {gz_context:?}",
        gz_context.len()
    );
    assert!(
        gz_context
            .iter()
            .all(|n| !n.contains("_CTOT_") && !n.contains("_CTOB_")),
        "zero-record CTOT/CTOB strands must be swept under --gzip; got {gz_context:?}"
    );
}

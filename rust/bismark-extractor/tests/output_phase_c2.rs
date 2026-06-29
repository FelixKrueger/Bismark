//! Phase C.2 integration tests.
//!
//! Covers:
//! - **#865** empty-file-sweep STDERR log lines match Perl's `warn` output
//!   (`{filename} contains data ->\tkept\n` and `{filename} was empty
//!   ->\tdeleted\n`).
//! - **#864** splitting-report byte-shape on a known fixture (regression
//!   guard for the 4 Critical findings from the C.2 dual plan-review).
//!
//! The five `write_percent_or_fallback` unit tests live inline with the
//! function in `src/output.rs::tests`. These end-to-end tests verify the
//! binary's actual STDERR output and on-disk file shapes.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use bismark_io::{BamWriter, BismarkRecord};
use bstr::BString;
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
use std::num::NonZeroUsize;

fn header_with_chr1() -> Header {
    let mut header = Header::default();
    header.reference_sequences_mut().insert(
        BString::from(b"chr1".to_vec()),
        Map::<ReferenceSequence>::new(NonZeroUsize::new(10_000).unwrap()),
    );
    header
}

#[allow(clippy::too_many_arguments)]
fn synth_record(
    qname: &[u8],
    xr: &[u8],
    xg: &[u8],
    xm: &[u8],
    alignment_start: usize,
) -> BismarkRecord {
    let mut record = RecordBuf::default();
    *record.flags_mut() = Flags::from(0u16); // unpaired SE record
    *record.sequence_mut() = Sequence::from(vec![b'A'; xm.len()]);
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
    BismarkRecord::from_noodles_record(record).expect("synth_record valid")
}

fn write_se_directional_one_cpg_record(path: &Path) {
    let header = header_with_chr1();
    let mut writer = BamWriter::from_path(path, header).unwrap();
    writer
        .write_record(&synth_record(b"r1", b"CT", b"CT", b"Z....", 100))
        .unwrap();
    writer.finish().unwrap();
}

/// **#865 stderr capture test.** Runs the binary on a synthetic SE
/// directional BAM with exactly one CpG-meth call. Verifies that the
/// empty-file sweep emits the Perl-format `{filename} contains data
/// ->\tkept` and `{filename} was empty ->\tdeleted` lines to **STDERR**
/// (not stdout — the rev-0 of the C.2 plan had this wrong).
///
/// Regression guard for plan rev 1 Critical C3 absorption.
#[test]
fn empty_file_sweep_emits_perl_format_log_lines_on_stderr() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path: PathBuf = workdir.path().join("one_record.bam");
    write_se_directional_one_cpg_record(&bam_path);
    let outdir = workdir.path().join("out");

    let output = Command::cargo_bin("bismark_methylation_extractor")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--output_dir")
        .arg(&outdir)
        .output()
        .expect("binary should run");

    assert!(output.status.success(), "binary exit failure: {:?}", output);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // The single OT record has a CpG-meth call (Z at offset 0), so
    // CpG_OT_one_record.txt is the one populated file. All other 11
    // per-strand files should be swept as empty.
    assert!(
        stderr.contains("CpG_OT_one_record.txt contains data ->\tkept"),
        "expected `kept` line for CpG_OT on stderr; got:\n{stderr}"
    );
    // Phase C.2 code-review B H1 absorption: stderr lines should carry
    // the FULL path (matches Perl `:607/615`), not the bare basename.
    // The output dir is `workdir/out/...`, so the full-path line will
    // contain the temp-dir prefix `out/CpG_OT_one_record.txt`.
    let outdir_str = outdir.display().to_string();
    assert!(
        stderr.contains(&format!(
            "{outdir_str}/CpG_OT_one_record.txt contains data ->\tkept"
        )),
        "stderr should emit FULL path (Perl-compat per H1); got:\n{stderr}"
    );
    // At least one was-empty line should be present (CTOT/CTOB/OB × CpG +
    // all CHG and CHH × all strands = 11 files in default mode all empty).
    assert!(
        stderr.contains("was empty ->\tdeleted"),
        "expected at least one `was empty -> deleted` line on stderr; got:\n{stderr}"
    );
    // Specifically the CTOT and CTOB files for a directional library.
    for ctx in ["CpG", "CHG", "CHH"] {
        for strand in ["CTOT", "CTOB"] {
            let fname = format!("{ctx}_{strand}_one_record.txt");
            assert!(
                stderr.contains(&format!("{fname} was empty ->\tdeleted")),
                "expected `{fname} was empty -> deleted` on stderr (directional library); got:\n{stderr}"
            );
        }
    }

    // STDOUT should NOT contain the sweep log lines (must go to stderr).
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("contains data ->\tkept"),
        "STDOUT must not contain sweep log lines (regression guard for C.2 rev 1 C3); got stdout:\n{stdout}"
    );
    assert!(
        !stdout.contains("was empty ->\tdeleted"),
        "STDOUT must not contain sweep log lines; got stdout:\n{stdout}"
    );
}

/// **#864 byte-shape smoke test.** Runs the binary on a synthetic
/// 1-record SE BAM and verifies key Perl-format invariants in the
/// splitting report: bare-basename line 1, "Parameters used..." header,
/// "Bismark Extractor Version:" not the version banner on line 1,
/// methylation/conversion phrasing, 1-decimal percentages, three trailing
/// newlines after the CHH percentage.
///
/// Regression guards for plan rev 1 Critical C1 + C2.
#[test]
fn splitting_report_byte_shape_matches_perl_format() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path: PathBuf = workdir.path().join("shape.bam");
    write_se_directional_one_cpg_record(&bam_path);
    let outdir = workdir.path().join("out");

    Command::cargo_bin("bismark_methylation_extractor")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--output_dir")
        .arg(&outdir)
        .assert()
        .success();

    let report = std::fs::read_to_string(outdir.join("shape_splitting_report.txt")).unwrap();

    // Line 1: bare basename (Perl :4995), NOT the version banner.
    let line1 = report.lines().next().unwrap();
    assert_eq!(line1, "shape.bam", "line 1 must be bare basename");

    // Header block (Perl :4996-5001).
    assert!(report.contains("Parameters used to extract methylation information:\n"));
    assert!(report.contains("Bismark Extractor Version: v0.25.1\n"));
    assert!(report.contains("Bismark result file: single-end (SAM format)\n"));
    assert!(report.contains("Output specified: strand-specific (default)\n"));

    // Default-zero --ignore flags should NOT emit any "Ignoring …" line.
    assert!(
        !report.contains("Ignoring first"),
        "no `Ignoring first` line for default-zero ignore"
    );
    assert!(
        !report.contains("Ignoring last"),
        "no `Ignoring last` line for default-zero ignore"
    );

    // C2 absorption: header → body has TWO blank lines (3 consecutive \n
    // bytes: end-of-last-header-line \n + close-header \n + leading \n
    // of "Processed").
    let target_idx = report
        .find("Processed 1 lines in total")
        .expect("body should start with `Processed N lines in total`");
    // The two bytes before `Processed` must both be `\n`.
    let preceding = &report.as_bytes()[target_idx.saturating_sub(2)..target_idx];
    assert_eq!(
        preceding, b"\n\n",
        "two blank lines between header and body"
    );

    // Body counters.
    assert!(report.contains("Processed 1 lines in total\n"));
    assert!(report.contains("Total number of methylation call strings processed: 1\n"));
    assert!(report.contains("Final Cytosine Methylation Report\n"));
    assert!(report.contains("=================================\n")); // exactly 33 `=`
    assert!(report.contains("Total number of C's analysed:\t1\n"));

    // Unmethylated phrasing: "Total C to T conversions in" NOT "Total
    // unmethylated C's in" (the rev-0 phrasing).
    assert!(report.contains("Total C to T conversions in CpG context:\t0\n"));
    assert!(report.contains("Total C to T conversions in CHG context:\t0\n"));
    assert!(report.contains("Total C to T conversions in CHH context:\t0\n"));

    // Percentages: 1 decimal place (Perl `%.1f`).
    assert!(report.contains("C methylated in CpG context:\t100.0%\n"));

    // C1 absorption: the file MUST end in exactly three \n bytes after the
    // CHH percentage's `%`. Test by reading the last 4 bytes.
    let bytes = report.as_bytes();
    assert!(
        bytes.ends_with(b"\n\n\n"),
        "splitting report must end in 3 trailing newlines; got last 8 bytes: {:?}",
        &bytes[bytes.len().saturating_sub(8)..]
    );
    // And the byte immediately before those 3 newlines must NOT be \n
    // (i.e., there are exactly 3, not 4 — the C1 failure mode).
    assert_ne!(
        bytes[bytes.len() - 4],
        b'\n',
        "exactly 3 trailing \\n bytes (not 4 — rev 1 Critical C1 regression guard)"
    );
}

/// **Code-review B C1 absorption.** `--yacht` mode sets Perl `$full=1`
/// AND `$merge_non_CpG=1`, so the splitting report must emit:
/// 1. `Output specified: comprehensive` (NOT `strand-specific (default)`)
/// 2. `Methylation in CHG and CHH context will be merged …` note
/// 3. 2-context percentage block (CpG + Non-CpG), NOT 3 contexts.
///
/// Pre-C1 fix: my `write_splitting_report` matched only
/// `Comprehensive | ComprehensiveMergeNonCpG` for the comprehensive
/// arm + merge-note + 2-context-percentage-block; `OutputMode::Yacht`
/// fell through to the strand-specific 3-context branch — three
/// byte-divergences in one mode.
#[test]
fn splitting_report_yacht_mode_matches_perl_comprehensive_merge() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path: PathBuf = workdir.path().join("yacht_smoke.bam");
    write_se_directional_one_cpg_record(&bam_path);
    let outdir = workdir.path().join("out");

    Command::cargo_bin("bismark_methylation_extractor")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--yacht")
        .arg("--output_dir")
        .arg(&outdir)
        .assert()
        .success();

    let report = std::fs::read_to_string(outdir.join("yacht_smoke_splitting_report.txt")).unwrap();

    // (1) Output specified: comprehensive (Perl :1331 sets $full=1).
    assert!(
        report.contains("Output specified: comprehensive\n"),
        "--yacht must emit `comprehensive` (Perl :1331 $full=1); got:\n{report}"
    );
    assert!(
        !report.contains("Output specified: strand-specific"),
        "--yacht must NOT emit `strand-specific (default)`; got:\n{report}"
    );

    // (2) Merge note (Perl :1333 sets $merge_non_CpG=1).
    assert!(
        report.contains(
            "Methylation in CHG and CHH context will be merged into \"non-CpG context\" output\n"
        ),
        "--yacht must emit merge_non_CpG note (Perl :1333); got:\n{report}"
    );

    // (3) 2-context percentage block: CpG + non-CpG (NOT CpG/CHG/CHH).
    // The non-CpG line may be either the content variant
    // (`C methylated in non-CpG context:\t…%`) or the zero-denominator
    // fallback (`Can't determine percentage of methylated Cs in non-CpG
    // context if value was 0`) depending on whether the fixture has CHG
    // or CHH calls. Either is correct; what matters for C1 is that the
    // mode emits a non-CpG-context line (not a CHG and CHH one).
    assert!(
        report.contains("C methylated in CpG context:")
            || report.contains(
                "Can't determine percentage of methylated Cs in CpG context if value was 0"
            ),
        "expected a CpG percentage or fallback line; got:\n{report}"
    );
    assert!(
        report.contains("C methylated in non-CpG context:")
            || report.contains(
                "Can't determine percentage of methylated Cs in non-CpG context if value was 0"
            ),
        "--yacht must emit a `non-CpG` percentage or fallback line (2-context block); got:\n{report}"
    );
    assert!(
        !report.contains("C methylated in CHG context:")
            && !report.contains(
                "Can't determine percentage of methylated Cs in CHG context if value was 0"
            ),
        "--yacht 2-context block must NOT include CHG percentage/fallback line; got:\n{report}"
    );
    assert!(
        !report.contains("C methylated in CHH context:")
            && !report.contains(
                "Can't determine percentage of methylated Cs in CHH context if value was 0"
            ),
        "--yacht 2-context block must NOT include CHH percentage/fallback line; got:\n{report}"
    );

    // EOF still has exactly 3 trailing newlines (last line is non-CpG,
    // baked `\n\n\n` per write_percent_or_fallback(is_last=true)).
    let bytes = report.as_bytes();
    assert!(
        bytes.ends_with(b"\n\n\n"),
        "yacht report must end in 3 trailing newlines; got last 8 bytes: {:?}",
        &bytes[bytes.len().saturating_sub(8)..]
    );
    assert_ne!(
        bytes[bytes.len() - 4],
        b'\n',
        "exactly 3 trailing newlines, not 4"
    );
}

/// **Code-review B H2 absorption.** Perl `:319` gates the entire sweep
/// with `unless ($mbias_only)`. In `--mbias_only` mode Rust's
/// `OutputFileMap` is empty, but the unguarded sweep would still emit
/// two trailing `eprintln!()` blank lines on stderr — Perl emits
/// nothing. After H2 fix, `state.rs::finalize` guards the sweep call
/// on `!self.mbias_only`.
#[test]
fn mbias_only_mode_does_not_call_sweep_no_trailing_blank_lines() {
    let workdir = tempfile::tempdir().unwrap();
    let bam_path: PathBuf = workdir.path().join("mbias_only.bam");
    write_se_directional_one_cpg_record(&bam_path);
    let outdir = workdir.path().join("out");

    let output = Command::cargo_bin("bismark_methylation_extractor")
        .unwrap()
        .arg(&bam_path)
        .arg("--single-end")
        .arg("--mbias_only")
        .arg("--output_dir")
        .arg(&outdir)
        .output()
        .expect("binary should run");

    assert!(output.status.success(), "binary exit failure: {:?}", output);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Sweep never invoked → no `kept`/`deleted` lines on stderr.
    assert!(
        !stderr.contains("contains data ->\tkept"),
        "MbiasOnly: sweep should not emit `kept` lines (H2 guard); got:\n{stderr}"
    );
    assert!(
        !stderr.contains("was empty ->\tdeleted"),
        "MbiasOnly: sweep should not emit `deleted` lines (H2 guard); got:\n{stderr}"
    );
}

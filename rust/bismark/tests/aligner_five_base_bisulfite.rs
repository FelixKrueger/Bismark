//! `[#1095]` Integration gates for `--five_base_bisulfite_bam`.
//!
//! The per-record algorithm is unit-tested in `aligner::five_base_bisulfite`. These tests
//! cover the driver: the CLI surface, the reader/writer round trip, the report, and the two
//! properties that only hold end-to-end.
//!
//! The **idempotence gate** is the load-bearing one: converting an ordinary *bisulfite*
//! Bismark BAM must return identical SAM text. It holds by construction — for bisulfite data
//! `SEQ` already carries meth/unmeth at every letter position — so it exercises the encoding
//! table, the `XG`→pair mapping, the positional zip, OB-strand orientation, the `NM`/`MD`
//! arithmetic, **and** the masking rule (whose set is provably empty on unmasked input) in
//! one comparison, using a committed fixture and no 5-Base data.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bismark_bin() -> PathBuf {
    // target/debug/deps/<test binary> -> target/debug/bismark
    let mut p = std::env::current_exe().expect("test exe path");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("bismark")
}

fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
}

/// The committed soft-clip + indel fixture: 8 SE records over pUC19, both `XG` values,
/// leading (`9S81M`) and trailing (`81M9S`) clips, insertions and deletions on both strands.
/// Byte-identical between the Rust aligner and live Perl v0.25.1.
fn fixture() -> PathBuf {
    data_dir()
        .join("five_base_bisulfite")
        .join("softclip_indel_se.bam")
}

fn samtools_available() -> bool {
    let present = Command::new("samtools")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    // Fail loud in CI: these gates must not pass vacuously (#1095 review HIGH-3; the same
    // guard `aligner_five_base_groundtruth.rs` uses for minimap2). A silent skip left the
    // load-bearing idempotence gate reporting green in CI while asserting nothing.
    if !present && std::env::var_os("CI").is_some() {
        panic!(
            "samtools not found but $CI is set: the #1095 gates compare decompressed SAM text \
             and require it. Refusing to no-op."
        );
    }
    present
}

fn sam_body(bam: &Path) -> String {
    let out = Command::new("samtools")
        .args(["view", &bam.to_string_lossy()])
        .output()
        .expect("samtools view");
    assert!(
        out.status.success(),
        "samtools view failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn run_converter(input: &Path, out_dir: &Path) -> std::process::Output {
    Command::new(bismark_bin())
        .arg("--illumina_5base")
        .arg("--five_base_bisulfite_bam")
        .arg(input)
        .arg("--output_dir")
        .arg(out_dir)
        .output()
        .expect("run bismark")
}

/// 🔑 The idempotence gate. A bisulfite BAM in must give identical SAM text out.
#[test]
fn bisulfite_input_round_trips_to_identical_sam_text() {
    if !samtools_available() {
        eprintln!("skipping: samtools not on PATH");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let out = run_converter(&fixture(), tmp.path());
    assert!(
        out.status.success(),
        "converter failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let produced = tmp.path().join("softclip_indel_se.bisulfite.bam");
    assert!(produced.exists(), "no output BAM written");

    // Compare decompressed SAM bodies, not bytes: BGZF block boundaries are writer-dependent
    // and `NM` is re-encoded as i32, so a byte comparison would fail for reasons that have
    // nothing to do with the conversion.
    assert_eq!(
        sam_body(&fixture()),
        sam_body(&produced),
        "converting a bisulfite BAM must not change a single record"
    );
}

/// The report must say so too — 0 flips, 0 masked. `flipped == 0` is the file-level assertion
/// that the input was not 5-Base; anything else would mean the encoding table is inverted.
#[test]
fn bisulfite_input_reports_a_zero_flip_rate() {
    let tmp = tempfile::tempdir().unwrap();
    let out = run_converter(&fixture(), tmp.path());
    assert!(out.status.success());

    let report = std::fs::read_to_string(tmp.path().join("softclip_indel_se.bisulfite_report.txt"))
        .expect("report written");
    assert!(report.contains("records read\t8"), "report:\n{report}");
    assert!(
        report.contains("records re-encoded\t8"),
        "report:\n{report}"
    );
    assert!(report.contains("bases flipped\t0"), "report:\n{report}");
    assert!(report.contains("flip rate\t0.000000"), "report:\n{report}");
    assert!(
        report.contains("no-call cytosines masked to N\t0"),
        "masking must be provably empty on input that used no --five_base_baseq:\n{report}"
    );
    assert!(
        report.contains("already bisulfite convention"),
        "report:\n{report}"
    );
    // Non-vacuity: the fixture really does contain calls, so "0 flipped" is a result rather
    // than an artefact of there being nothing to flip.
    assert!(
        report.contains("methylation calls\t150"),
        "report:\n{report}"
    );
}

/// `XM` is never modified — that is what keeps the output readable by Bismark's own extractor
/// and by `bam2pat --ds_test`, and it is why the idempotence gate can be a text comparison.
#[test]
fn xm_is_left_untouched() {
    if !samtools_available() {
        eprintln!("skipping: samtools not on PATH");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    assert!(run_converter(&fixture(), tmp.path()).status.success());
    let produced = tmp.path().join("softclip_indel_se.bisulfite.bam");

    let xms = |s: &str| -> Vec<String> {
        s.lines()
            .filter_map(|l| {
                l.split('\t')
                    .find(|f| f.starts_with("XM:Z:"))
                    .map(str::to_string)
            })
            .collect()
    };
    let before = xms(&sam_body(&fixture()));
    assert_eq!(before.len(), 8, "fixture should have 8 XM tags");
    assert_eq!(before, xms(&sam_body(&produced)));
}

/// The appended `@PG` must have a **distinct** ID: the input already carries `ID:Bismark`, and
/// duplicate `@PG` IDs are invalid SAM.
#[test]
fn appends_a_pg_with_a_distinct_id() {
    if !samtools_available() {
        eprintln!("skipping: samtools not on PATH");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    assert!(run_converter(&fixture(), tmp.path()).status.success());

    let hdr = Command::new("samtools")
        .args([
            "view",
            "-H",
            &tmp.path()
                .join("softclip_indel_se.bisulfite.bam")
                .to_string_lossy(),
        ])
        .output()
        .unwrap();
    let hdr = String::from_utf8_lossy(&hdr.stdout);
    let ids: Vec<&str> = hdr
        .lines()
        .filter(|l| l.starts_with("@PG"))
        .filter_map(|l| l.split('\t').find(|f| f.starts_with("ID:")))
        .collect();
    assert!(
        ids.contains(&"ID:bismark-five-base-bisulfite"),
        "converter @PG missing; got {ids:?}"
    );
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "duplicate @PG IDs: {ids:?}");
}

// ---- fail-loud -----------------------------------------------------------------------

#[test]
fn requires_illumina_5base() {
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new(bismark_bin())
        .arg("--five_base_bisulfite_bam")
        .arg(fixture())
        .arg("--output_dir")
        .arg(tmp.path())
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "should refuse without --illumina_5base"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("requires --illumina_5base"),
        "message must name the missing flag; got:\n{err}"
    );
}

/// The mutual-exclusion check has to run BEFORE either standalone dispatch, because both of
/// them `return` — a check placed beside one would never fire for the other.
#[test]
fn rejects_both_standalone_bam_modes_together() {
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new(bismark_bin())
        .arg("--illumina_5base")
        .arg("--five_base_bisulfite_bam")
        .arg(fixture())
        .arg("--five_base_consensus_from_bam")
        .arg(fixture())
        .arg("--output_dir")
        .arg(tmp.path())
        .output()
        .unwrap();
    assert!(!out.status.success(), "should refuse both modes at once");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("mutually exclusive"),
        "expected a mutual-exclusion error; got:\n{err}"
    );
}

/// A non-Bismark BAM must be refused rather than silently copied. The uBAM fixtures are the
/// sharp case: every record is unmapped, so each one takes the verbatim pass-through path and
/// the run would otherwise "succeed" while producing a file identical to its input — a user
/// would reasonably believe they had converted something.
#[test]
fn rejects_input_without_bismark_tags() {
    let ubam = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("test_files")
        .join("BS-seq_10K_se_trimmed_from_TrimGalore.bam");
    if !ubam.exists() {
        eprintln!("skipping: uBAM fixture not present");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let out = run_converter(&ubam, tmp.path());
    assert!(!out.status.success(), "should refuse a BAM with no XM");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("none could be re-encoded") || err.contains("XM"),
        "message should explain nothing was convertible; got:\n{err}"
    );
    // Unconditional. An earlier version was `!exists() || err.contains(..)`, which
    // short-circuits on the message and so never checked the file — the very defect it
    // claimed to guard (review HIGH-1).
    assert!(
        !tmp.path()
            .join("BS-seq_10K_se_trimmed_from_TrimGalore.bisulfite.bam")
            .exists(),
        "a refused run must leave NO output: BamWriter's Drop writes the BGZF EOF marker, so a \
         leftover file passes `samtools quickcheck` and reads as a finished conversion"
    );
}

// NOTE — the unmapped/secondary pass-through (§3.7) has NO committed test. It needs a BAM
// with unmapped records AND full Bismark tags, and no tracked fixture has both (the
// `filter_nonconversion/se_unmapped` records carry no `MD`, and the Trim Galore uBAMs are
// untracked and have no tags at all). The behaviour itself was verified during review by
// crafting such a record: a `FLAG 4` record with tags passes through with its tags intact.
// Closing this properly means adding an unmapped record to the `five_base_bisulfite` fixture,
// which would invalidate the record/call counts asserted above.

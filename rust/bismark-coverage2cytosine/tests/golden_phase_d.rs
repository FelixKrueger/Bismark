//! Phase D byte-identity golden tests: `--merge_CpGs` (+ `--discordance_filter`).
//! Goldens in `tests/data/phase_d/` are generated from the repo Perl v0.25.1.
//! gzip is compared after decompression.

use std::io::Read;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use flate2::read::MultiGzDecoder;

fn pb() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/phase_b")
}
fn pd() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/phase_d")
}

fn gunzip(path: &Path) -> Vec<u8> {
    let mut d = MultiGzDecoder::new(std::fs::File::open(path).unwrap());
    let mut out = Vec::new();
    d.read_to_end(&mut out).unwrap();
    out
}

/// Run `--merge_CpGs …` into a tempdir; assert success; return the tempdir.
fn run_merge(genome: &Path, cov: &Path, flags: &[&str]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("coverage2cytosine").unwrap();
    cmd.arg("-o")
        .arg("m")
        .arg("-g")
        .arg(genome)
        .arg("--dir")
        .arg(tmp.path())
        .arg("--merge_CpGs")
        .args(flags)
        .arg(cov)
        .assert()
        .success();
    tmp
}

#[test]
fn merge_cov_matches_golden() {
    let tmp = run_merge(&pb().join("genome"), &pb().join("in.cov"), &[]);
    let got = std::fs::read(tmp.path().join("m.CpG_report.merged_CpG_evidence.cov")).unwrap();
    assert_eq!(
        got,
        std::fs::read(pd().join("merge.merged.golden")).unwrap()
    );
}

#[test]
fn merge_gzip_decompresses_to_golden() {
    let tmp = run_merge(&pb().join("genome"), &pb().join("in.cov"), &["--gzip"]);
    let got = gunzip(&tmp.path().join("m.CpG_report.merged_CpG_evidence.cov.gz"));
    assert_eq!(
        got,
        std::fs::read(pd().join("merge.merged.golden")).unwrap()
    );
}

#[test]
fn merge_zero_based_half_open_matches_golden() {
    let tmp = run_merge(
        &pb().join("genome"),
        &pb().join("in.cov"),
        &["--zero_based"],
    );
    let got = std::fs::read(tmp.path().join("m.CpG_report.merged_CpG_evidence.cov")).unwrap();
    assert_eq!(
        got,
        std::fs::read(pd().join("merge_zero.merged.golden")).unwrap()
    );
}

#[test]
fn discordance_gross_routes_to_discordant_file() {
    let tmp = run_merge(
        &pb().join("genome"),
        &pd().join("disc_gross.cov"),
        &["--discordance_filter", "20"],
    );
    // The Δ80 pair is diverted → merged empty, discordant has both rows.
    assert_eq!(
        std::fs::read(tmp.path().join("m.CpG_report.merged_CpG_evidence.cov")).unwrap(),
        std::fs::read(pd().join("disc_gross.merged.golden")).unwrap()
    );
    assert_eq!(
        std::fs::read(tmp.path().join("m.CpG_report.discordant_CpG_evidence.cov")).unwrap(),
        std::fs::read(pd().join("disc_gross.discordant.golden")).unwrap()
    );
}

#[test]
fn discordance_boundary_merges_not_diverts() {
    // THE rounding trap: 50% vs 55% with N=5 → rounded Δ = 5.0, NOT > 5 → MERGED.
    // A raw-f64 compare would wrongly divert (5.0000000000000071 > 5).
    let tmp = run_merge(
        &pb().join("genome"),
        &pd().join("disc_boundary.cov"),
        &["--discordance_filter", "5"],
    );
    assert_eq!(
        std::fs::read(tmp.path().join("m.CpG_report.merged_CpG_evidence.cov")).unwrap(),
        std::fs::read(pd().join("disc_boundary.merged.golden")).unwrap(),
        "boundary pair must be MERGED (rounded Δ=5.0, not >5)"
    );
    assert_eq!(
        std::fs::read(tmp.path().join("m.CpG_report.discordant_CpG_evidence.cov")).unwrap(),
        std::fs::read(pd().join("disc_boundary.discordant.golden")).unwrap(),
        "discordant file must be empty at the boundary"
    );
}

#[test]
fn resync_consecutive_short_scaffolds_slide_recovers() {
    // Two lone-orphan scaffolds (sA,sB) before a real-pair scaffold (sC) →
    // the chromosome-start slide consumes the orphans and lands on sC's pair.
    let tmp = run_merge(&pd().join("resync_genome"), &pd().join("resync.cov"), &[]);
    let got = std::fs::read(tmp.path().join("m.CpG_report.merged_CpG_evidence.cov")).unwrap();
    assert_eq!(
        got,
        std::fs::read(pd().join("resync.merged.golden")).unwrap()
    );
}

#[test]
fn eof_mid_resync_errors_with_partial_merged_file() {
    // Trailing lone-orphan scaffolds (sA,sB after chrM) → the resync read-ahead
    // hits EOF; Perl dies (255) leaving chrM's merged line. Rust errors (exit 1,
    // no panic) and leaves the SAME partial merged file.
    let tmp = tempfile::tempdir().unwrap();
    Command::cargo_bin("coverage2cytosine")
        .unwrap()
        .arg("-o")
        .arg("m")
        .arg("-g")
        .arg(pd().join("eof_genome"))
        .arg("--dir")
        .arg(tmp.path())
        .arg("--merge_CpGs")
        .arg(pd().join("eof.cov"))
        .assert()
        .failure()
        .code(1)
        .stderr(predicates::str::contains("sanity violation"));
    // Partial merged file matches the lines Perl wrote before its die.
    let got = std::fs::read(tmp.path().join("m.CpG_report.merged_CpG_evidence.cov")).unwrap();
    assert_eq!(got, std::fs::read(pd().join("eof.merged.golden")).unwrap());
}

#[test]
fn discordance_both_measured_gate_pools_unmeasured_partner() {
    // V6: + strand 9/0 (100%), - strand uncovered (0/0). The both-measured gate
    // (Perl :1903) blocks discordance routing → the pair is POOLED, not diverted,
    // even though the only measured strand sits at an extreme percentage.
    let tmp = run_merge(
        &pb().join("genome"),
        &pd().join("gate.cov"),
        &["--discordance_filter", "20"],
    );
    assert_eq!(
        std::fs::read(tmp.path().join("m.CpG_report.merged_CpG_evidence.cov")).unwrap(),
        std::fs::read(pd().join("gate.merged.golden")).unwrap(),
        "unmeasured-partner pair must be pooled (the both-measured gate blocks routing)"
    );
    assert_eq!(
        std::fs::read(tmp.path().join("m.CpG_report.discordant_CpG_evidence.cov")).unwrap(),
        std::fs::read(pd().join("gate.discordant.golden")).unwrap(),
        "discordant file must be empty (the gate blocks routing)"
    );
}

#[test]
fn resync_same_chromosome_branch_skips_orphan() {
    // V8a: chr1 starts with CG → a lone + orphan at pos 1 (its - partner at pos 2
    // needs an upstream base off the 5' end, so it is dropped). The next CpG pair is
    // on the SAME chromosome, so the resync takes the chr1==chr2 single-advance
    // branch (Perl :1875), skipping the orphan and re-pairing from pos 5.
    let tmp = run_merge(&pd().join("samechr_genome"), &pd().join("samechr.cov"), &[]);
    let got = std::fs::read(tmp.path().join("m.CpG_report.merged_CpG_evidence.cov")).unwrap();
    assert_eq!(
        got,
        std::fs::read(pd().join("samechr.merged.golden")).unwrap()
    );
}

#[test]
fn merge_multi_chromosome_matches_golden() {
    // V14: covered CpGs on chr1 AND chr2 → merged lines spanning a chromosome
    // transition inside the pair loop (the existing single-chr golden never does).
    let tmp = run_merge(&pd().join("multi_genome"), &pd().join("multi.cov"), &[]);
    let got = std::fs::read(tmp.path().join("m.CpG_report.merged_CpG_evidence.cov")).unwrap();
    assert_eq!(
        got,
        std::fs::read(pd().join("multi.merged.golden")).unwrap()
    );
}
